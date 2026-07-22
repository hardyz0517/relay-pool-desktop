use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use futures_util::Stream;
use tokio::time::Sleep;

use super::{
    error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
    lifecycle::{
        attempt::{
            AttemptContext, AttemptFailureKind, AttemptTerminal, AttemptTerminalRecord,
            ClassifiedAttemptFailure, FailureBlame, HealthEffect, RetryDisposition,
        },
        delivery::DeliveryTerminal,
        request::PendingFinalRequestRecord,
        writer::{AttemptWriteReservation, RequestTerminalReservation},
    },
    limits::RequestLease,
    observability::SseUsageObserver,
    request::ByteStream,
};

use crate::services::time::now_millis_for_services;

const DEFAULT_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

pub(crate) struct SelectedAttemptFinalization {
    reservation: AttemptWriteReservation,
    context: AttemptContext,
}

impl SelectedAttemptFinalization {
    pub(crate) fn new(reservation: AttemptWriteReservation, context: AttemptContext) -> Self {
        Self {
            reservation,
            context,
        }
    }
}

pub(crate) struct LifecycleFinalizationLease {
    terminal: Option<RequestTerminalReservation>,
    selected_attempt: Option<SelectedAttemptFinalization>,
    finalized: bool,
}

enum FinalizationState {
    Lifecycle(PendingFinalRequestRecord),
}

pub(crate) enum FinalizationOutcome {
    Completed,
    Failed {
        code: String,
        detail: Option<String>,
    },
    Interrupted {
        detail: Option<String>,
    },
}

enum FinalizationTarget {
    Lifecycle(LifecycleFinalizationLease),
}

impl LifecycleFinalizationLease {
    pub(crate) fn new(
        terminal: RequestTerminalReservation,
        selected_attempt: Option<SelectedAttemptFinalization>,
    ) -> Self {
        Self {
            terminal: Some(terminal),
            selected_attempt,
            finalized: false,
        }
    }

    pub(crate) fn finalize(
        mut self,
        record: PendingFinalRequestRecord,
        delivery: DeliveryTerminal,
        outcome: FinalizationOutcome,
        attempt_terminal: Option<AttemptTerminal>,
    ) {
        if self.finalized {
            return;
        }
        self.finalized = true;
        let Some(terminal) = self.terminal.take() else {
            return;
        };
        if let Some(selected_attempt) = self.selected_attempt.take() {
            if let Some(attempt_terminal) = attempt_terminal {
                let record = AttemptTerminalRecord {
                    context: selected_attempt.context,
                    terminal: attempt_terminal,
                    output_committed: record.annotations().body_bytes.unwrap_or_default() > 0,
                    terminal_at_ms: now_millis_for_services() as i64,
                };
                let _attempt_ack = selected_attempt.reservation.send(record);
            }
        }
        let final_record = match outcome {
            FinalizationOutcome::Completed => record.complete(delivery),
            FinalizationOutcome::Failed { code, detail } => record.fail(code, detail, delivery),
            FinalizationOutcome::Interrupted { detail } => record.interrupt(delivery, detail),
        };
        let _ack = terminal.send(final_record);
    }
}

impl FinalizationTarget {
    fn finalize(
        self,
        state: FinalizationState,
        delivery: DeliveryTerminal,
        outcome: FinalizationOutcome,
        attempt_terminal: Option<AttemptTerminal>,
    ) {
        match (self, state) {
            (Self::Lifecycle(lease), FinalizationState::Lifecycle(record)) => {
                lease.finalize(record, delivery, outcome, attempt_terminal)
            }
        }
    }
}

pub(crate) fn buffered_lifecycle_finalizing_stream(
    body: Bytes,
    record: PendingFinalRequestRecord,
    lease: LifecycleFinalizationLease,
    request_lease: RequestLease,
) -> ByteStream {
    lifecycle_finalizing_stream(
        Box::pin(futures_util::stream::once(async move { Ok(body) })),
        record,
        lease,
        request_lease,
    )
}

pub(crate) fn lifecycle_finalizing_stream(
    stream: ByteStream,
    record: PendingFinalRequestRecord,
    lease: LifecycleFinalizationLease,
    request_lease: RequestLease,
) -> ByteStream {
    lifecycle_finalizing_stream_with_idle_timeout(
        stream,
        record,
        lease,
        request_lease,
        DEFAULT_STREAM_IDLE_TIMEOUT,
    )
}

pub(crate) fn lifecycle_finalizing_stream_with_idle_timeout(
    stream: ByteStream,
    record: PendingFinalRequestRecord,
    lease: LifecycleFinalizationLease,
    request_lease: RequestLease,
    idle_timeout: Duration,
) -> ByteStream {
    finalizing_stream_with_target(
        stream,
        FinalizationState::Lifecycle(record),
        FinalizationTarget::Lifecycle(lease),
        Some(request_lease),
        idle_timeout,
    )
}

fn finalizing_stream_with_target(
    stream: ByteStream,
    state: FinalizationState,
    target: FinalizationTarget,
    request_lease: Option<RequestLease>,
    idle_timeout: Duration,
) -> ByteStream {
    let now_ms = now_millis_for_services() as i64;
    let started_at_ms = match &state {
        FinalizationState::Lifecycle(record) => record.context().received_at_ms.min(now_ms),
    };
    Box::pin(LifecycleBody {
        stream,
        state: Some(state),
        target: Some(target),
        request_lease,
        observer: SseUsageObserver::default(),
        idle_timeout,
        sleep: None,
        completed: false,
        body_bytes: 0,
        first_token_ms: None,
        started_at_ms,
    })
}

struct LifecycleBody {
    stream: ByteStream,
    state: Option<FinalizationState>,
    target: Option<FinalizationTarget>,
    request_lease: Option<RequestLease>,
    observer: SseUsageObserver,
    idle_timeout: Duration,
    sleep: Option<Pin<Box<Sleep>>>,
    completed: bool,
    body_bytes: i64,
    first_token_ms: Option<i64>,
    started_at_ms: i64,
}

impl Stream for LifecycleBody {
    type Item = Result<Bytes, ProxyFailure>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.sleep.is_none() {
            self.reset_idle_sleep();
        }

        match self.stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                self.observe_chunk(&bytes);
                self.reset_idle_sleep();
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(failure))) => {
                self.finalize_failure(&failure, "body_error");
                Poll::Ready(Some(Err(failure)))
            }
            Poll::Ready(None) => {
                if self.responses_stream_ended_incomplete() {
                    let failure = incomplete_responses_stream_failure();
                    self.finalize_failure(&failure, "body_incomplete");
                    return Poll::Ready(Some(Err(failure)));
                }
                self.completed = true;
                self.finalize_once(
                    DeliveryTerminal::BodyCompleted,
                    FinalizationOutcome::Completed,
                    Some(AttemptTerminal::Succeeded),
                );
                Poll::Ready(None)
            }
            Poll::Pending => {
                let expired = self
                    .sleep
                    .as_mut()
                    .is_some_and(|sleep| sleep.as_mut().poll(cx).is_ready());
                if expired {
                    let failure = stream_idle_timeout_failure(self.idle_timeout);
                    self.finalize_failure(&failure, "body_idle_timeout");
                    Poll::Ready(Some(Err(failure)))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

impl LifecycleBody {
    fn responses_stream_ended_incomplete(&self) -> bool {
        match self.state.as_ref() {
            Some(FinalizationState::Lifecycle(record)) => {
                record.annotations().stream
                    && record.context().local_path == "/v1/responses"
                    && !self.observer.response_completed()
            }
            None => false,
        }
    }

    fn finalize_once(
        &mut self,
        delivery: DeliveryTerminal,
        outcome: FinalizationOutcome,
        attempt_terminal: Option<AttemptTerminal>,
    ) {
        self.apply_observations();
        if let (Some(state), Some(target)) = (self.state.take(), self.target.take()) {
            target.finalize(state, delivery, outcome, attempt_terminal);
        }
        self.request_lease.take();
    }

    fn finalize_failure(&mut self, failure: &ProxyFailure, completion_source: &str) {
        match self.state.as_mut() {
            Some(FinalizationState::Lifecycle(record)) => {
                record.annotations_mut().failure_source =
                    Some(failure_source_label(failure.source).to_string());
                record.annotations_mut().completion_source = Some(completion_source.to_string());
            }
            None => {}
        }
        self.completed = true;
        let attempt_terminal = if failure.source == FailureSource::Upstream {
            Some(AttemptTerminal::Failed(ClassifiedAttemptFailure {
                kind: AttemptFailureKind::StreamInterrupted,
                blame: FailureBlame::Upstream,
                retry: RetryDisposition::StopRequest,
                health: HealthEffect::ObserveFailure,
                public_code: failure.code.as_str().to_string(),
                sanitized_detail: Some(failure.public_message.clone()),
            }))
        } else {
            None
        };
        self.finalize_once(
            DeliveryTerminal::BodyCompleted,
            FinalizationOutcome::Failed {
                code: failure.code.as_str().to_string(),
                detail: Some(failure.public_message.clone()),
            },
            attempt_terminal,
        );
    }

    fn observe_chunk(&mut self, bytes: &Bytes) {
        self.body_bytes += bytes.len() as i64;
        if self.first_token_ms.is_none() && !bytes.is_empty() {
            self.first_token_ms =
                Some((now_millis_for_services() as i64 - self.started_at_ms).max(0));
        }
        self.observer.push(bytes);
    }

    fn apply_observations(&mut self) {
        match self.state.as_mut() {
            Some(FinalizationState::Lifecycle(record)) => {
                let annotations = record.annotations_mut();
                if self.body_bytes > 0 {
                    annotations.body_bytes = Some(self.body_bytes);
                }
                if annotations.first_token_ms.is_none() {
                    annotations.first_token_ms = self.first_token_ms;
                }
                if let Some(usage) = self.observer.usage() {
                    annotations.prompt_tokens = usage.input_tokens.or(annotations.prompt_tokens);
                    annotations.completion_tokens =
                        usage.output_tokens.or(annotations.completion_tokens);
                    annotations.total_tokens = usage.total_tokens.or(annotations.total_tokens);
                    annotations.cache_creation_tokens = usage
                        .cache_creation_tokens
                        .or(annotations.cache_creation_tokens);
                    annotations.cache_read_tokens =
                        usage.cache_read_tokens.or(annotations.cache_read_tokens);
                }
            }
            None => {}
        }
    }

    fn reset_idle_sleep(&mut self) {
        self.sleep = Some(Box::pin(tokio::time::sleep(self.idle_timeout)));
    }

    fn finalize_downstream_drop(&mut self) {
        match self.state.as_mut() {
            Some(FinalizationState::Lifecycle(record)) => {
                record.annotations_mut().failure_source = Some("downstream".to_string());
                record.annotations_mut().completion_source = Some("downstream_dropped".to_string());
            }
            None => {}
        }
        self.finalize_once(
            DeliveryTerminal::DownstreamDropped,
            FinalizationOutcome::Interrupted {
                detail: Some("downstream disconnected before body completion".to_string()),
            },
            Some(AttemptTerminal::Failed(ClassifiedAttemptFailure {
                kind: AttemptFailureKind::DownstreamDrop,
                blame: FailureBlame::Downstream,
                retry: RetryDisposition::StopRequest,
                health: HealthEffect::Neutral,
                public_code: "DownstreamDropped".to_string(),
                sanitized_detail: Some(
                    "downstream disconnected before body completion".to_string(),
                ),
            })),
        );
    }
}

fn stream_idle_timeout_failure(timeout: Duration) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::UpstreamStreamFailed,
        FailureSource::Upstream,
        RetryClass::AfterCommitStop,
        http::StatusCode::BAD_GATEWAY,
        format!("upstream stream idle for {} ms", timeout.as_millis()),
    )
}

fn incomplete_responses_stream_failure() -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::UpstreamStreamFailed,
        FailureSource::Upstream,
        RetryClass::AfterCommitStop,
        http::StatusCode::BAD_GATEWAY,
        "upstream responses stream ended before response.completed",
    )
}

impl Drop for LifecycleBody {
    fn drop(&mut self) {
        if !self.completed {
            self.finalize_downstream_drop();
        }
    }
}

fn failure_source_label(source: FailureSource) -> &'static str {
    match source {
        FailureSource::Local => "local",
        FailureSource::Routing => "routing",
        FailureSource::Upstream => "upstream",
        FailureSource::Downstream => "downstream",
        FailureSource::Internal => "internal",
    }
}

#[expect(
    dead_code,
    reason = "reserved by the downstream-disconnect failure contract"
)]
pub(crate) fn downstream_disconnected_failure() -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::DownstreamDisconnected,
        FailureSource::Downstream,
        RetryClass::AfterCommitStop,
        http::StatusCode::BAD_GATEWAY,
        "downstream disconnected",
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    };

    use bytes::Bytes;
    use futures_util::{future::BoxFuture, stream, StreamExt};
    use tokio::sync::Semaphore;

    use crate::services::proxy::{
        error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
        lifecycle::{
            attempt::{
                AttemptContext, AttemptFailureKind, AttemptTerminal, AttemptTerminalRecord,
                FailureBlame, HealthEffect, RetryDisposition,
            },
            delivery::DeliveryTerminal,
            ports::{
                AttemptCommitAck, LifecycleWriteError, RequestCommitAck, RequestLifecycleStore,
                RequestStartAck,
            },
            request::{
                AttemptId, FinalRequestRecord, PendingFinalRequestRecord, RequestContextSnapshot,
                RequestLogAnnotations, RequestStartRecord, RequestTerminal,
            },
            writer::{LifecycleWriter, LifecycleWriterWorker},
        },
        limits::RequestLease,
    };

    use super::{
        buffered_lifecycle_finalizing_stream, lifecycle_finalizing_stream_with_idle_timeout,
        LifecycleFinalizationLease, SelectedAttemptFinalization,
    };

    #[tokio::test]
    async fn response_body_finalizes_success_only_after_eof() {
        let fixture = LifecycleBodyFixture::new("response-body-eof", "/v1/chat/completions").await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            lease,
            request_lease,
            record,
            active_requests,
        } = fixture;
        let mut body = buffered_lifecycle_finalizing_stream(
            Bytes::from_static(b"ok"),
            record,
            lease,
            request_lease,
        );

        assert_eq!(store.calls(), 0);
        assert_eq!(
            body.next().await.unwrap().unwrap(),
            Bytes::from_static(b"ok")
        );
        assert_eq!(store.calls(), 0, "chunk delivery is not completion");
        assert!(body.next().await.is_none());
        store.wait_for_calls(1).await;

        assert_eq!(store.calls(), 1);
        let record = store.last_request().expect("record");
        assert_eq!(record.context.request_id, "response-body-eof");
        assert!(matches!(
            record.terminal.terminal,
            RequestTerminal::Completed(_)
        ));
        assert_eq!(record.terminal.delivery, DeliveryTerminal::BodyCompleted);
        assert_eq!(
            record.annotations.completion_source.as_deref(),
            Some("upstream")
        );
        assert_eq!(record.annotations.body_bytes, Some(2));
        assert_eq!(active_requests.load(Ordering::SeqCst), 0);

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn response_body_timing_uses_the_original_request_start() {
        let now = crate::services::time::now_millis_for_services() as i64;
        let fixture = LifecycleBodyFixture::new_with_start(
            "response-body-request-start",
            "/v1/chat/completions",
            now - 100,
        )
        .await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            lease,
            request_lease,
            record,
            ..
        } = fixture;
        let mut body = buffered_lifecycle_finalizing_stream(
            Bytes::from_static(b"ok"),
            record,
            lease,
            request_lease,
        );

        assert!(body.next().await.unwrap().is_ok());
        assert!(body.next().await.is_none());
        store.wait_for_calls(1).await;

        let record = store.last_request().expect("record");
        assert!(
            record
                .annotations
                .first_token_ms
                .is_some_and(|value| value >= 100),
            "first token timing must include time before the response wrapper was created"
        );

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn response_body_drop_after_chunk_before_eof_finalizes_downstream_disconnect_once() {
        let fixture = LifecycleBodyFixture::new_with_selected_attempt(
            "response-body-drop-after-chunk",
            "/v1/chat/completions",
        )
        .await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            lease,
            request_lease,
            record,
            active_requests,
        } = fixture;
        let mut body = buffered_lifecycle_finalizing_stream(
            Bytes::from_static(b"ok"),
            record,
            lease,
            request_lease,
        );

        assert_eq!(
            body.next().await.unwrap().unwrap(),
            Bytes::from_static(b"ok")
        );
        drop(body);
        store.wait_for_calls(1).await;

        assert_eq!(store.calls(), 1);
        let record = store.last_request().expect("record");
        assert!(matches!(
            record.terminal.terminal,
            RequestTerminal::Interrupted(_)
        ));
        assert_eq!(
            record.terminal.delivery,
            DeliveryTerminal::DownstreamDropped
        );
        assert_eq!(
            record.annotations.completion_source.as_deref(),
            Some("downstream_dropped")
        );
        assert_eq!(
            record.annotations.failure_source.as_deref(),
            Some("downstream")
        );
        assert_eq!(store.attempt_calls(), 1);
        let attempt = store.last_attempt().expect("attempt record");
        assert!(matches!(
            attempt.terminal,
            AttemptTerminal::Failed(ref failure)
                if failure.kind == AttemptFailureKind::DownstreamDrop
                    && failure.blame == FailureBlame::Downstream
                    && failure.retry == RetryDisposition::StopRequest
                    && failure.health == HealthEffect::Neutral
        ));
        assert!(attempt.output_committed);
        assert_eq!(active_requests.load(Ordering::SeqCst), 0);

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn response_body_drop_before_poll_finalizes_downstream_disconnect_once() {
        let fixture =
            LifecycleBodyFixture::new("response-body-drop-before-poll", "/v1/chat/completions")
                .await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            lease,
            request_lease,
            record,
            active_requests,
        } = fixture;
        let body = buffered_lifecycle_finalizing_stream(
            Bytes::from_static(b"ok"),
            record,
            lease,
            request_lease,
        );

        drop(body);
        store.wait_for_calls(1).await;

        assert_eq!(store.calls(), 1);
        let record = store.last_request().expect("record");
        assert!(matches!(
            record.terminal.terminal,
            RequestTerminal::Interrupted(_)
        ));
        assert_eq!(
            record.terminal.delivery,
            DeliveryTerminal::DownstreamDropped
        );
        assert_eq!(active_requests.load(Ordering::SeqCst), 0);

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn response_body_stream_error_finalizes_failure_once() {
        let fixture =
            LifecycleBodyFixture::new("response-body-stream-error", "/v1/chat/completions").await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            lease,
            request_lease,
            record,
            ..
        } = fixture;
        let mut body = lifecycle_finalizing_stream_with_idle_timeout(
            Box::pin(stream::iter(vec![Err(stream_failure())])),
            record,
            lease,
            request_lease,
            std::time::Duration::from_secs(1),
        );

        let failure = body.next().await.unwrap().expect_err("stream failure");
        assert_eq!(failure.code, ProxyFailureCode::UpstreamStreamFailed);
        drop(body);
        store.wait_for_calls(1).await;

        assert_eq!(store.calls(), 1);
        let record = store.last_request().expect("record");
        assert!(matches!(
            record.terminal.terminal,
            RequestTerminal::Failed(_)
        ));
        assert_eq!(record.terminal.delivery, DeliveryTerminal::BodyCompleted);
        assert_eq!(
            record.annotations.failure_source.as_deref(),
            Some("upstream")
        );
        assert_eq!(
            record.annotations.completion_source.as_deref(),
            Some("body_error")
        );

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn response_body_stream_idle_timeout_finalizes_upstream_failure_once() {
        let fixture =
            LifecycleBodyFixture::new("response-body-idle-timeout", "/v1/chat/completions").await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            lease,
            request_lease,
            record,
            ..
        } = fixture;
        let mut body = lifecycle_finalizing_stream_with_idle_timeout(
            Box::pin(stream::pending()),
            record,
            lease,
            request_lease,
            std::time::Duration::from_millis(1),
        );

        let failure = body.next().await.unwrap().expect_err("idle timeout");
        assert_eq!(failure.code, ProxyFailureCode::UpstreamStreamFailed);
        drop(body);
        store.wait_for_calls(1).await;

        assert_eq!(store.calls(), 1);
        let record = store.last_request().expect("record");
        assert!(matches!(
            record.terminal.terminal,
            RequestTerminal::Failed(_)
        ));
        assert_eq!(
            record.annotations.failure_source.as_deref(),
            Some("upstream")
        );
        assert_eq!(
            record.annotations.completion_source.as_deref(),
            Some("body_idle_timeout")
        );

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn response_body_stream_eof_records_sse_usage() {
        let fixture =
            LifecycleBodyFixture::new("response-body-sse-usage", "/v1/chat/completions").await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            lease,
            request_lease,
            record,
            ..
        } = fixture;
        let mut body = buffered_lifecycle_finalizing_stream(
            Bytes::from_static(
                br#"data: {"type":"response.completed","response":{"id":"resp_v2","usage":{"input_tokens":9,"output_tokens":4,"total_tokens":13}}}

"#,
            ),
            record,
            lease,
            request_lease,
        );

        assert!(body.next().await.unwrap().is_ok());
        assert!(body.next().await.is_none());
        store.wait_for_calls(1).await;

        let record = store.last_request().expect("record");
        assert_eq!(
            record.annotations.completion_source.as_deref(),
            Some("upstream")
        );
        assert_eq!(record.annotations.prompt_tokens, Some(9));
        assert_eq!(record.annotations.completion_tokens, Some(4));
        assert_eq!(record.annotations.total_tokens, Some(13));

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn response_body_responses_eof_without_completed_event_finalizes_failure() {
        let fixture =
            LifecycleBodyFixture::new("response-body-incomplete-responses-stream", "/v1/responses")
                .await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            lease,
            request_lease,
            mut record,
            ..
        } = fixture;
        record.annotations_mut().stream = true;
        let mut body = buffered_lifecycle_finalizing_stream(
            Bytes::from_static(
                br#"data: {"type":"response.created","response":{"id":"resp_incomplete"}}

"#,
            ),
            record,
            lease,
            request_lease,
        );

        assert!(body.next().await.unwrap().is_ok());
        let failure = body
            .next()
            .await
            .expect("incomplete stream failure")
            .expect_err("upstream stream must fail");
        assert_eq!(failure.code, ProxyFailureCode::UpstreamStreamFailed);
        store.wait_for_calls(1).await;

        let record = store.last_request().expect("record");
        assert!(matches!(
            record.terminal.terminal,
            RequestTerminal::Failed(_)
        ));
        assert_eq!(
            record.annotations.failure_source.as_deref(),
            Some("upstream")
        );
        assert_eq!(
            record.annotations.completion_source.as_deref(),
            Some("body_incomplete")
        );

        drop(writer);
        worker.join().await.expect("worker join");
    }

    struct RecordingStore {
        start_records: Arc<Mutex<Vec<RequestStartRecord>>>,
        attempt_records: Arc<Mutex<Vec<AttemptTerminalRecord>>>,
        request_records: Arc<Mutex<Vec<FinalRequestRecord>>>,
    }

    impl RecordingStore {
        fn new() -> Self {
            Self {
                start_records: Arc::new(Mutex::new(Vec::new())),
                attempt_records: Arc::new(Mutex::new(Vec::new())),
                request_records: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn calls(&self) -> usize {
            self.request_records.lock().expect("records lock").len()
        }

        fn last_request(&self) -> Option<FinalRequestRecord> {
            self.request_records
                .lock()
                .expect("records lock")
                .last()
                .cloned()
        }

        fn attempt_calls(&self) -> usize {
            self.attempt_records.lock().expect("attempt lock").len()
        }

        fn last_attempt(&self) -> Option<AttemptTerminalRecord> {
            self.attempt_records
                .lock()
                .expect("attempt lock")
                .last()
                .cloned()
        }

        async fn wait_for_calls(&self, expected: usize) {
            for _ in 0..1_000 {
                if self.calls() >= expected {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            assert_eq!(self.calls(), expected);
        }
    }

    impl RequestLifecycleStore for RecordingStore {
        fn start_request(
            &self,
            record: RequestStartRecord,
        ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
            let records = Arc::clone(&self.start_records);
            Box::pin(async move {
                records.lock().expect("start lock").push(record);
                Ok(RequestStartAck { inserted: true })
            })
        }

        fn finish_attempt(
            &self,
            record: AttemptTerminalRecord,
        ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
            let records = Arc::clone(&self.attempt_records);
            Box::pin(async move {
                records.lock().expect("attempt lock").push(record);
                Ok(AttemptCommitAck {
                    inserted: true,
                    health_applied: true,
                })
            })
        }

        fn finish_request(
            &self,
            record: FinalRequestRecord,
        ) -> BoxFuture<'static, Result<RequestCommitAck, LifecycleWriteError>> {
            let records = Arc::clone(&self.request_records);
            Box::pin(async move {
                records.lock().expect("finish lock").push(record);
                Ok(RequestCommitAck { finalized: true })
            })
        }
    }

    struct LifecycleBodyFixture {
        store: Arc<RecordingStore>,
        writer: LifecycleWriter,
        worker: LifecycleWriterWorker,
        lease: LifecycleFinalizationLease,
        request_lease: RequestLease,
        record: PendingFinalRequestRecord,
        active_requests: Arc<AtomicU32>,
    }

    impl LifecycleBodyFixture {
        async fn new(request_id: &str, local_path: &str) -> Self {
            Self::new_with_start(
                request_id,
                local_path,
                crate::services::time::now_millis_for_services() as i64,
            )
            .await
        }

        async fn new_with_start(request_id: &str, local_path: &str, received_at_ms: i64) -> Self {
            Self::new_with_start_and_attempt(request_id, local_path, received_at_ms, false).await
        }

        async fn new_with_selected_attempt(request_id: &str, local_path: &str) -> Self {
            Self::new_with_start_and_attempt(
                request_id,
                local_path,
                crate::services::time::now_millis_for_services() as i64,
                true,
            )
            .await
        }

        async fn new_with_start_and_attempt(
            request_id: &str,
            local_path: &str,
            received_at_ms: i64,
            include_selected_attempt: bool,
        ) -> Self {
            let context = RequestContextSnapshot {
                request_id: request_id.to_string(),
                method: "POST".to_string(),
                local_path: local_path.to_string(),
                endpoint: local_path.to_string(),
                received_at_ms,
            };
            let annotations = RequestLogAnnotations {
                model: Some("gpt-test".to_string()),
                stream: true,
                selected_station_key_id: Some("key-test".to_string()),
                selected_station_id: Some("station-test".to_string()),
                upstream_base_url: Some("https://example.test/v1".to_string()),
                route_policy: Some("priority_fallback".to_string()),
                route_reason: Some("selected test key".to_string()),
                rejected_candidates_json: Some("[]".to_string()),
                route_wait_ms: Some(0),
                completion_source: Some("upstream".to_string()),
                ..RequestLogAnnotations::default()
            };

            let store = Arc::new(RecordingStore::new());
            let (writer, worker) = LifecycleWriter::start(4, store.clone()).expect("writer");
            let reservation = writer.try_reserve_request().expect("request reservation");
            let (terminal, start_ack) = reservation.send_start(RequestStartRecord {
                context: context.clone(),
            });
            start_ack
                .await
                .expect("start ack channel")
                .expect("start ack");

            let active_requests = Arc::new(AtomicU32::new(0));
            let request_permit = Arc::new(Semaphore::new(1))
                .acquire_owned()
                .await
                .expect("request permit");
            let request_lease = RequestLease::new(request_permit, Arc::clone(&active_requests));
            let selected_attempt = if include_selected_attempt {
                let reservation = writer.try_reserve_attempt().expect("attempt reservation");
                let context = AttemptContext {
                    attempt_id: AttemptId::new(request_id, 0),
                    station_id: "station-test".to_string(),
                    station_key_id: "key-test".to_string(),
                    endpoint_revision: 1,
                    started_at_ms: received_at_ms,
                };
                Some(SelectedAttemptFinalization::new(reservation, context))
            } else {
                None
            };
            let selected_attempt_id = selected_attempt
                .as_ref()
                .map(|attempt| attempt.context.attempt_id.clone());

            Self {
                store,
                writer,
                worker,
                lease: LifecycleFinalizationLease::new(terminal, selected_attempt),
                request_lease,
                record: PendingFinalRequestRecord::new(
                    context,
                    selected_attempt_id,
                    1,
                    0,
                    annotations,
                ),
                active_requests,
            }
        }
    }

    fn stream_failure() -> ProxyFailure {
        ProxyFailure::new(
            ProxyFailureCode::UpstreamStreamFailed,
            FailureSource::Upstream,
            RetryClass::AfterCommitStop,
            http::StatusCode::BAD_GATEWAY,
            "upstream stream failed",
        )
    }
}
