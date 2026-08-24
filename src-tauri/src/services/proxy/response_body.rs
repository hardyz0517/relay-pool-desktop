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
    attempt::{
        CostFinalizationReservations, DownstreamRequestFinalizationLease,
        DualTerminalFinalizationLease, UpstreamAttemptFinalizationLease,
    },
    diagnostic_memory::DiagnosticMemoryPermit,
    error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
    finalization::FinalizationOutcome,
    lifecycle::{
        attempt::{
            AttemptContext, AttemptFailureKind, AttemptTerminal, ClassifiedAttemptFailure,
            FailureBlame, HealthEffect, RetryDisposition,
        },
        delivery::DeliveryTerminal,
        request::{PendingFinalRequestRecord, RequestLogAnnotations},
        writer::{AttemptWriteReservation, RequestTerminalReservation},
    },
    limits::RequestLease,
    observability::{ObservedUsage, SseUsageObserver},
    protocol::{
        chat_sse::ChatSseMachine, responses_sse::ResponsesSseMachine, ProtocolFailure,
        ProtocolMachine, ProtocolTerminal,
    },
    request::ByteStream,
};

use crate::{observability::correlation, services::time::now_millis_for_services};

const DEFAULT_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

enum FinalizationState {
    Lifecycle(PendingFinalRequestRecord),
}

enum FinalizationTarget {
    DualTerminal(DualTerminalFinalizationLease),
}

impl FinalizationTarget {
    fn finalize(
        self,
        state: FinalizationState,
        delivery: DeliveryTerminal,
        outcome: FinalizationOutcome,
        attempt_terminal: Option<AttemptTerminal>,
        output_committed: bool,
    ) {
        match (self, state) {
            (Self::DualTerminal(lease), FinalizationState::Lifecycle(record)) => {
                let _join = lease.finalize(
                    record,
                    delivery,
                    outcome,
                    attempt_terminal,
                    output_committed,
                );
            }
        }
    }
}

pub(crate) fn dual_terminal_buffered_lifecycle_finalizing_stream(
    body: Bytes,
    mut record: PendingFinalRequestRecord,
    request_terminal: RequestTerminalReservation,
    selected_attempt: Option<(AttemptWriteReservation, AttemptContext)>,
    costs: Option<CostFinalizationReservations>,
    request_lease: RequestLease,
) -> ByteStream {
    if let Ok(value) = serde_json::from_slice(&body) {
        if let Some(usage) = ObservedUsage::from_json(&value) {
            apply_usage(record.annotations_mut(), &usage);
        }
    }
    dual_terminal_finalizing_stream(
        Box::pin(futures_util::stream::once(async move { Ok(body) })),
        record,
        request_terminal,
        selected_attempt,
        costs,
        request_lease,
        DEFAULT_STREAM_IDLE_TIMEOUT,
        None,
        false,
    )
}

#[cfg(test)]
pub(crate) fn dual_terminal_lifecycle_finalizing_stream_with_idle_timeout(
    stream: ByteStream,
    record: PendingFinalRequestRecord,
    request_terminal: RequestTerminalReservation,
    selected_attempt: Option<(AttemptWriteReservation, AttemptContext)>,
    costs: Option<CostFinalizationReservations>,
    request_lease: RequestLease,
    idle_timeout: Duration,
) -> ByteStream {
    dual_terminal_lifecycle_finalizing_stream_with_idle_timeout_and_diagnostic_memory(
        stream,
        record,
        request_terminal,
        selected_attempt,
        costs,
        request_lease,
        idle_timeout,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn dual_terminal_lifecycle_finalizing_stream_with_idle_timeout_and_diagnostic_memory(
    stream: ByteStream,
    record: PendingFinalRequestRecord,
    request_terminal: RequestTerminalReservation,
    selected_attempt: Option<(AttemptWriteReservation, AttemptContext)>,
    costs: Option<CostFinalizationReservations>,
    request_lease: RequestLease,
    idle_timeout: Duration,
    diagnostic_memory: Option<DiagnosticMemoryPermit>,
) -> ByteStream {
    dual_terminal_finalizing_stream(
        stream,
        record,
        request_terminal,
        selected_attempt,
        costs,
        request_lease,
        idle_timeout,
        diagnostic_memory,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn dual_terminal_finalizing_stream(
    stream: ByteStream,
    record: PendingFinalRequestRecord,
    request_terminal: RequestTerminalReservation,
    selected_attempt: Option<(AttemptWriteReservation, AttemptContext)>,
    costs: Option<CostFinalizationReservations>,
    request_lease: RequestLease,
    idle_timeout: Duration,
    diagnostic_memory: Option<DiagnosticMemoryPermit>,
    enforce_stream_protocol: bool,
) -> ByteStream {
    let request = DownstreamRequestFinalizationLease::new(request_terminal, request_lease);
    let selected_attempt = selected_attempt.map(|(reservation, context)| {
        let probe_scope = context.probe_scope.clone();
        let probe_state_revision = context.probe_state_revision;
        UpstreamAttemptFinalizationLease::new(
            reservation,
            context,
            probe_scope,
            probe_state_revision,
        )
    });
    finalizing_stream_with_target(
        stream,
        FinalizationState::Lifecycle(record),
        FinalizationTarget::DualTerminal(DualTerminalFinalizationLease::new(
            request,
            selected_attempt,
            costs,
        )),
        None,
        idle_timeout,
        diagnostic_memory,
        correlation::current(),
        enforce_stream_protocol,
    )
}

fn finalizing_stream_with_target(
    stream: ByteStream,
    state: FinalizationState,
    target: FinalizationTarget,
    request_lease: Option<RequestLease>,
    idle_timeout: Duration,
    mut diagnostic_memory: Option<DiagnosticMemoryPermit>,
    correlation_id: Option<correlation::CorrelationId>,
    enforce_stream_protocol: bool,
) -> ByteStream {
    let now_ms = now_millis_for_services() as i64;
    let started_at_ms = match &state {
        FinalizationState::Lifecycle(record) => record.context().received_at_ms.min(now_ms),
    };
    let protocol = enforce_stream_protocol
        .then(|| protocol_machine(&state, diagnostic_memory.take()))
        .flatten();
    Box::pin(LifecycleBody {
        stream,
        state: Some(state),
        target: Some(target),
        request_lease,
        observer: SseUsageObserver::default(),
        protocol,
        pending_terminal: None,
        idle_timeout,
        sleep: None,
        completed: false,
        body_bytes: 0,
        first_token_ms: None,
        started_at_ms,
        correlation_id,
        _diagnostic_memory: diagnostic_memory,
    })
}

struct LifecycleBody {
    stream: ByteStream,
    state: Option<FinalizationState>,
    target: Option<FinalizationTarget>,
    request_lease: Option<RequestLease>,
    observer: SseUsageObserver,
    protocol: Option<Box<dyn ProtocolMachine>>,
    pending_terminal: Option<ProtocolTerminal>,
    idle_timeout: Duration,
    sleep: Option<Pin<Box<Sleep>>>,
    completed: bool,
    body_bytes: i64,
    first_token_ms: Option<i64>,
    started_at_ms: i64,
    correlation_id: Option<correlation::CorrelationId>,
    _diagnostic_memory: Option<DiagnosticMemoryPermit>,
}

impl Stream for LifecycleBody {
    type Item = Result<Bytes, ProxyFailure>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let correlation_id = self.correlation_id.clone();
        if let Some(correlation_id) = correlation_id {
            return correlation::with_scope("proxy.request.body", correlation_id, || {
                self.as_mut().poll_next_inner(cx)
            });
        }
        self.poll_next_inner(cx)
    }
}

impl LifecycleBody {
    fn poll_next_inner(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, ProxyFailure>>> {
        if let Some(terminal) = self.pending_terminal.take() {
            self.finalize_protocol_terminal(terminal, DeliveryTerminal::BodyCompleted);
            return Poll::Ready(None);
        }
        if self.sleep.is_none() {
            self.reset_idle_sleep();
        }

        match self.stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                let terminal = match self.observe_chunk(&bytes) {
                    Ok(terminal) => terminal,
                    Err(failure) => {
                        self.finalize_failure(&failure, "body_protocol_error");
                        return Poll::Ready(Some(Err(failure)));
                    }
                };
                if let Some(terminal) = terminal {
                    self.pending_terminal = Some(terminal);
                    self.sleep = None;
                } else {
                    self.reset_idle_sleep();
                }
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(failure))) => {
                self.finalize_failure(&failure, "body_error");
                Poll::Ready(Some(Err(failure)))
            }
            Poll::Ready(None) => {
                if let Some(protocol) = self.protocol.as_mut() {
                    match protocol.finish_eof() {
                        Ok(ProtocolTerminal::Incomplete) => {
                            let failure = incomplete_stream_failure();
                            self.finalize_failure(&failure, "body_incomplete");
                            return Poll::Ready(Some(Err(failure)));
                        }
                        Ok(terminal) => {
                            self.finalize_protocol_terminal(
                                terminal,
                                DeliveryTerminal::BodyCompleted,
                            );
                            return Poll::Ready(None);
                        }
                        Err(protocol_failure) => {
                            let failure = protocol_stream_failure(protocol_failure);
                            self.finalize_failure(&failure, "body_protocol_error");
                            return Poll::Ready(Some(Err(failure)));
                        }
                    }
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

    fn finalize_once(
        &mut self,
        delivery: DeliveryTerminal,
        outcome: FinalizationOutcome,
        attempt_terminal: Option<AttemptTerminal>,
    ) {
        self.apply_observations();
        let output_committed = self.body_bytes > 0;
        if let (Some(state), Some(target)) = (self.state.take(), self.target.take()) {
            target.finalize(state, delivery, outcome, attempt_terminal, output_committed);
        }
        self.request_lease.take();
    }

    fn finalize_failure(&mut self, failure: &ProxyFailure, completion_source: &str) {
        self.finalize_failure_with_delivery(
            failure,
            completion_source,
            DeliveryTerminal::BodyCompleted,
        );
    }

    fn observe_chunk(&mut self, bytes: &Bytes) -> Result<Option<ProtocolTerminal>, ProxyFailure> {
        self.body_bytes += bytes.len() as i64;
        if self.first_token_ms.is_none() && !bytes.is_empty() {
            self.first_token_ms =
                Some((now_millis_for_services() as i64 - self.started_at_ms).max(0));
        }
        self.observer.push(bytes);
        let Some(protocol) = self.protocol.as_mut() else {
            return Ok(None);
        };
        // Delivery has already committed this chunk. Preserve per-event ordering only to
        // determine its terminal; precommit buffering is owned by SseBootstrapMachine.
        Ok(protocol
            .observe_chunk(bytes)
            .map_err(protocol_stream_failure)?
            .terminal())
    }

    fn finalize_protocol_terminal(
        &mut self,
        terminal: ProtocolTerminal,
        delivery: DeliveryTerminal,
    ) {
        self.completed = true;
        match terminal {
            ProtocolTerminal::Completed => {
                if let Some(FinalizationState::Lifecycle(record)) = self.state.as_mut() {
                    record.annotations_mut().completion_source =
                        Some("stream_complete".to_string());
                }
                self.finalize_once(
                    delivery,
                    FinalizationOutcome::Completed,
                    Some(AttemptTerminal::Succeeded),
                );
            }
            ProtocolTerminal::Failed | ProtocolTerminal::Incomplete => {
                let (completion_source, failure) = match terminal {
                    ProtocolTerminal::Failed => (
                        "protocol_failed",
                        explicit_terminal_failure("upstream stream reported a failed terminal"),
                    ),
                    ProtocolTerminal::Incomplete => (
                        "protocol_incomplete",
                        explicit_terminal_failure(
                            "upstream stream reported an incomplete terminal",
                        ),
                    ),
                    ProtocolTerminal::Completed => unreachable!(),
                };
                self.finalize_failure_with_delivery(&failure, completion_source, delivery);
            }
        }
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
                    apply_usage(annotations, usage);
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

    fn finalize_failure_with_delivery(
        &mut self,
        failure: &ProxyFailure,
        completion_source: &str,
        delivery: DeliveryTerminal,
    ) {
        match self.state.as_mut() {
            Some(FinalizationState::Lifecycle(record)) => {
                record.annotations_mut().failure_source =
                    Some(failure_source_label(failure.source).to_string());
                record.annotations_mut().completion_source = Some(completion_source.to_string());
                if let Some(outcome) = failure.routing_outcome_facts() {
                    record.set_routing_outcome(outcome);
                }
            }
            None => {}
        }
        self.completed = true;
        let attempt_terminal = (failure.source == FailureSource::Upstream).then(|| {
            AttemptTerminal::Failed(ClassifiedAttemptFailure {
                kind: AttemptFailureKind::StreamInterrupted,
                blame: FailureBlame::Upstream,
                retry: RetryDisposition::StopRequest,
                health: HealthEffect::ObserveFailure,
                public_code: failure.code.as_str().to_string(),
                sanitized_detail: Some(failure.public_message.clone()),
            })
        });
        self.finalize_once(
            delivery,
            FinalizationOutcome::Failed {
                code: failure.code.as_str().to_string(),
                detail: Some(failure.public_message.clone()),
            },
            attempt_terminal,
        );
    }
}

fn protocol_machine(
    state: &FinalizationState,
    diagnostic_memory: Option<DiagnosticMemoryPermit>,
) -> Option<Box<dyn ProtocolMachine>> {
    let FinalizationState::Lifecycle(record) = state;
    if !record.annotations().stream {
        return None;
    }
    match record.context().local_path.as_str() {
        "/v1/responses" => Some(match diagnostic_memory {
            Some(permit) => Box::new(ResponsesSseMachine::from_retained_memory(permit)),
            None => Box::new(ResponsesSseMachine::new()),
        }),
        "/v1/chat/completions" => Some(match diagnostic_memory {
            Some(permit) => Box::new(ChatSseMachine::from_retained_memory(permit)),
            None => Box::new(ChatSseMachine::new()),
        }),
        _ => None,
    }
}

fn apply_usage(annotations: &mut RequestLogAnnotations, usage: &ObservedUsage) {
    annotations.prompt_tokens = usage.input_tokens.or(annotations.prompt_tokens);
    annotations.completion_tokens = usage.output_tokens.or(annotations.completion_tokens);
    annotations.total_tokens = usage.total_tokens.or(annotations.total_tokens);
    annotations.cache_creation_tokens = usage
        .cache_creation_tokens
        .or(annotations.cache_creation_tokens);
    annotations.cache_read_tokens = usage.cache_read_tokens.or(annotations.cache_read_tokens);
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

fn incomplete_stream_failure() -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::UpstreamStreamFailed,
        FailureSource::Upstream,
        RetryClass::AfterCommitStop,
        http::StatusCode::BAD_GATEWAY,
        "upstream stream ended before its protocol terminal",
    )
}

fn explicit_terminal_failure(message: &'static str) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::UpstreamStreamFailed,
        FailureSource::Upstream,
        RetryClass::AfterCommitStop,
        http::StatusCode::BAD_GATEWAY,
        message,
    )
}

fn protocol_stream_failure(protocol_failure: ProtocolFailure) -> ProxyFailure {
    let mut failure = ProxyFailure::new(
        ProxyFailureCode::UpstreamStreamFailed,
        FailureSource::Upstream,
        RetryClass::AfterCommitStop,
        http::StatusCode::BAD_GATEWAY,
        "upstream stream violated the expected protocol",
    );
    failure.internal_detail = Some(format!(
        "{}: {}",
        protocol_failure.code, protocol_failure.detail
    ));
    failure
}

impl Drop for LifecycleBody {
    fn drop(&mut self) {
        if !self.completed {
            if let Some(terminal) = self.pending_terminal.take() {
                self.finalize_protocol_terminal(terminal, DeliveryTerminal::DownstreamDropped);
            } else {
                self.finalize_downstream_drop();
            }
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

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    };

    use bytes::Bytes;
    use futures_util::{future::BoxFuture, stream, StreamExt};
    use tokio::sync::{Notify, Semaphore};

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
            writer::{
                AttemptWriteReservation, LifecycleWriter, LifecycleWriterWorker,
                RequestTerminalReservation,
            },
        },
        limits::RequestLease,
    };

    use super::{
        dual_terminal_buffered_lifecycle_finalizing_stream,
        dual_terminal_lifecycle_finalizing_stream_with_idle_timeout,
    };

    #[tokio::test]
    async fn response_body_finalizes_success_only_after_eof() {
        let fixture = LifecycleBodyFixture::new("response-body-eof", "/v1/chat/completions").await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            active_requests,
        } = fixture;
        let mut body = dual_terminal_buffered_lifecycle_finalizing_stream(
            Bytes::from_static(b"ok"),
            record,
            request_terminal,
            selected_attempt,
            None,
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
    async fn buffered_json_usage_is_preserved_in_request_terminal() {
        let fixture =
            LifecycleBodyFixture::new("response-body-buffered-usage", "/v1/chat/completions").await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            ..
        } = fixture;
        let mut body = dual_terminal_buffered_lifecycle_finalizing_stream(
            Bytes::from_static(
                br#"{"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}}"#,
            ),
            record,
            request_terminal,
            selected_attempt,
            None,
            request_lease,
        );

        assert!(body.next().await.is_some());
        assert!(body.next().await.is_none());
        store.wait_for_calls(1).await;

        let record = store.last_request().expect("request terminal");
        assert_eq!(record.annotations.prompt_tokens, Some(2));
        assert_eq!(record.annotations.completion_tokens, Some(3));
        assert_eq!(record.annotations.total_tokens, Some(5));

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn request_terminal_is_committed_when_attempt_persistence_fails() {
        let fixture = LifecycleBodyFixture::new_with_selected_attempt(
            "response-body-attempt-failure",
            "/v1/chat/completions",
        )
        .await;
        fixture.store.fail_attempts();
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            ..
        } = fixture;

        let mut body = dual_terminal_buffered_lifecycle_finalizing_stream(
            Bytes::from_static(b"{}"),
            record,
            request_terminal,
            selected_attempt,
            None,
            request_lease,
        );
        assert!(body.next().await.is_some());
        assert!(body.next().await.is_none());
        store.wait_for_calls(1).await;
        assert!(matches!(
            store
                .last_request()
                .expect("request terminal")
                .terminal
                .terminal,
            RequestTerminal::Interrupted(_)
        ));
        assert_eq!(store.attempt_calls(), 0);

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn correlated_response_body_polls_inner_stream_under_request_scope() {
        let fixture =
            LifecycleBodyFixture::new("response-body-correlated", "/v1/chat/completions").await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            ..
        } = fixture;
        let correlation_id = crate::observability::correlation::CorrelationId::for_proxy_request(
            "response-body-correlated",
        );
        let observed = Arc::new(Mutex::new(None));
        let observed_in_stream = Arc::clone(&observed);
        let terminal = Bytes::from_static(b"data: [DONE]\n\n");
        let terminal_in_stream = terminal.clone();
        let inner = stream::once(async move {
            *observed_in_stream
                .lock()
                .expect("observed correlation lock") =
                crate::observability::correlation::current_id_string();
            Ok(terminal_in_stream)
        });
        let mut body = crate::observability::correlation::with_scope(
            "proxy.request.body",
            correlation_id.clone(),
            || {
                dual_terminal_lifecycle_finalizing_stream_with_idle_timeout(
                    Box::pin(inner),
                    record,
                    request_terminal,
                    selected_attempt,
                    None,
                    request_lease,
                    std::time::Duration::from_secs(1),
                )
            },
        );

        assert_eq!(body.next().await.unwrap().unwrap(), terminal);
        assert_eq!(
            observed
                .lock()
                .expect("observed correlation lock")
                .as_deref(),
            Some(correlation_id.as_str())
        );
        assert!(body.next().await.is_none());
        store.wait_for_calls(1).await;
        assert!(
            crate::observability::correlation::current_id_string().is_none(),
            "body correlation must not leak after stream polling"
        );

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
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            ..
        } = fixture;
        let mut body = dual_terminal_buffered_lifecycle_finalizing_stream(
            Bytes::from_static(b"ok"),
            record,
            request_terminal,
            selected_attempt,
            None,
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
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            active_requests,
        } = fixture;
        let mut body = dual_terminal_buffered_lifecycle_finalizing_stream(
            Bytes::from_static(b"ok"),
            record,
            request_terminal,
            selected_attempt,
            None,
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
    async fn slow_downstream_after_commit_does_not_poll_or_retry_and_drop_finalizes_once() {
        let fixture = LifecycleBodyFixture::new_with_selected_attempt(
            "response-body-slow-downstream",
            "/v1/responses",
        )
        .await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            active_requests,
        } = fixture;
        let first = Bytes::from_static(
            b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n",
        );
        let upstream = stream::once({
            let first = first.clone();
            async move { Ok(first) }
        })
        .chain(stream::pending());
        let mut body = dual_terminal_lifecycle_finalizing_stream_with_idle_timeout(
            Box::pin(upstream),
            record,
            request_terminal,
            selected_attempt,
            None,
            request_lease,
            std::time::Duration::from_secs(1),
        );

        assert_eq!(body.next().await.unwrap().unwrap(), first);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert_eq!(
            store.calls(),
            0,
            "a slow downstream must not poll upstream or terminalize before it resumes or drops"
        );

        drop(body);
        store.wait_for_calls(1).await;
        assert_eq!(store.calls(), 1);
        assert_eq!(store.attempt_calls(), 1);
        let request = store.last_request().expect("request terminal");
        assert!(matches!(
            request.terminal.terminal,
            RequestTerminal::Interrupted(_)
        ));
        assert_eq!(
            request.terminal.delivery,
            DeliveryTerminal::DownstreamDropped
        );
        let attempt = store.last_attempt().expect("attempt terminal");
        assert!(matches!(
            attempt.terminal,
            AttemptTerminal::Failed(ref failure)
                if failure.kind == AttemptFailureKind::DownstreamDrop
                    && failure.retry == RetryDisposition::StopRequest
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
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            active_requests,
        } = fixture;
        let body = dual_terminal_buffered_lifecycle_finalizing_stream(
            Bytes::from_static(b"ok"),
            record,
            request_terminal,
            selected_attempt,
            None,
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
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            ..
        } = fixture;
        let mut body = dual_terminal_lifecycle_finalizing_stream_with_idle_timeout(
            Box::pin(stream::iter(vec![Err(stream_failure())])),
            record,
            request_terminal,
            selected_attempt,
            None,
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
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            ..
        } = fixture;
        let mut body = dual_terminal_lifecycle_finalizing_stream_with_idle_timeout(
            Box::pin(stream::pending()),
            record,
            request_terminal,
            selected_attempt,
            None,
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
    async fn responses_terminal_closes_without_polling_a_later_transport_error() {
        let fixture =
            LifecycleBodyFixture::new("response-body-terminal-before-error", "/v1/responses").await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            ..
        } = fixture;
        let completed = Bytes::from_static(
            br#"data: {"type":"response.completed","response":{"id":"resp_done"}}

"#,
        );
        let mut body = dual_terminal_lifecycle_finalizing_stream_with_idle_timeout(
            Box::pin(stream::iter(vec![
                Ok(completed.clone()),
                Err(stream_failure()),
            ])),
            record,
            request_terminal,
            selected_attempt,
            None,
            request_lease,
            std::time::Duration::from_secs(1),
        );

        assert_eq!(body.next().await.unwrap().unwrap(), completed);
        assert!(body.next().await.is_none());
        store.wait_for_calls(1).await;

        let record = store.last_request().expect("request terminal");
        assert!(matches!(
            record.terminal.terminal,
            RequestTerminal::Completed(_)
        ));
        assert_eq!(
            record.annotations.completion_source.as_deref(),
            Some("stream_complete")
        );
        assert_eq!(record.annotations.failure_source, None);

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn responses_terminal_closes_even_when_upstream_stays_open() {
        let fixture =
            LifecycleBodyFixture::new("response-body-terminal-before-pending", "/v1/responses")
                .await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            ..
        } = fixture;
        let completed = Bytes::from_static(
            br#"data: {"type":"response.completed","response":{"id":"resp_done"}}

"#,
        );
        let upstream = stream::once({
            let completed = completed.clone();
            async move { Ok(completed) }
        })
        .chain(stream::pending());
        let mut body = dual_terminal_lifecycle_finalizing_stream_with_idle_timeout(
            Box::pin(upstream),
            record,
            request_terminal,
            selected_attempt,
            None,
            request_lease,
            std::time::Duration::from_millis(1),
        );

        assert_eq!(body.next().await.unwrap().unwrap(), completed);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), body.next())
                .await
                .expect("terminal must close without waiting for upstream")
                .is_none()
        );
        store.wait_for_calls(1).await;
        assert!(matches!(
            store
                .last_request()
                .expect("request terminal")
                .terminal
                .terminal,
            RequestTerminal::Completed(_)
        ));

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn responses_failed_terminal_is_forwarded_then_closes_cleanly() {
        let fixture =
            LifecycleBodyFixture::new("response-body-explicit-failure", "/v1/responses").await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            ..
        } = fixture;
        let failed = Bytes::from_static(
            br#"data: {"type":"response.failed","response":{"id":"resp_failed"}}

"#,
        );
        let mut body = dual_terminal_lifecycle_finalizing_stream_with_idle_timeout(
            Box::pin(stream::iter(vec![
                Ok(failed.clone()),
                Err(stream_failure()),
            ])),
            record,
            request_terminal,
            selected_attempt,
            None,
            request_lease,
            std::time::Duration::from_secs(1),
        );

        assert_eq!(body.next().await.unwrap().unwrap(), failed);
        assert!(body.next().await.is_none());
        store.wait_for_calls(1).await;

        let record = store.last_request().expect("request terminal");
        assert!(matches!(
            record.terminal.terminal,
            RequestTerminal::Failed(_)
        ));
        assert_eq!(
            record.annotations.completion_source.as_deref(),
            Some("protocol_failed")
        );
        assert_eq!(
            record.annotations.failure_source.as_deref(),
            Some("upstream")
        );

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn chat_done_closes_without_polling_a_later_transport_error() {
        let fixture = LifecycleBodyFixture::new(
            "response-body-chat-terminal-before-error",
            "/v1/chat/completions",
        )
        .await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            ..
        } = fixture;
        let done = Bytes::from_static(
            br#"data: {"choices":[{"delta":{"content":"done"}}]}

data: [DONE]

"#,
        );
        let mut body = dual_terminal_lifecycle_finalizing_stream_with_idle_timeout(
            Box::pin(stream::iter(vec![Ok(done.clone()), Err(stream_failure())])),
            record,
            request_terminal,
            selected_attempt,
            None,
            request_lease,
            std::time::Duration::from_secs(1),
        );

        assert_eq!(body.next().await.unwrap().unwrap(), done);
        assert!(body.next().await.is_none());
        store.wait_for_calls(1).await;

        let record = store.last_request().expect("request terminal");
        assert!(matches!(
            record.terminal.terminal,
            RequestTerminal::Completed(_)
        ));
        assert_eq!(
            record.annotations.completion_source.as_deref(),
            Some("stream_complete")
        );
        assert_eq!(record.annotations.failure_source, None);

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn chat_eof_without_done_finalizes_failure() {
        let fixture = LifecycleBodyFixture::new(
            "response-body-incomplete-chat-stream",
            "/v1/chat/completions",
        )
        .await;
        let LifecycleBodyFixture {
            store,
            writer,
            worker,
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            ..
        } = fixture;
        let event = Bytes::from_static(
            br#"data: {"choices":[{"delta":{"content":"partial"}}]}

"#,
        );
        let mut body = dual_terminal_lifecycle_finalizing_stream_with_idle_timeout(
            Box::pin(stream::once({
                let event = event.clone();
                async move { Ok(event) }
            })),
            record,
            request_terminal,
            selected_attempt,
            None,
            request_lease,
            std::time::Duration::from_secs(1),
        );

        assert_eq!(body.next().await.unwrap().unwrap(), event);
        let failure = body
            .next()
            .await
            .expect("incomplete stream failure")
            .expect_err("chat stream without [DONE] must fail");
        assert_eq!(failure.code, ProxyFailureCode::UpstreamStreamFailed);
        store.wait_for_calls(1).await;

        let record = store.last_request().expect("request terminal");
        assert!(matches!(
            record.terminal.terminal,
            RequestTerminal::Failed(_)
        ));
        assert_eq!(
            record.annotations.completion_source.as_deref(),
            Some("body_incomplete")
        );
        assert_eq!(
            record.annotations.failure_source.as_deref(),
            Some("upstream")
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
            request_terminal,
            selected_attempt,
            request_lease,
            record,
            ..
        } = fixture;
        let mut body = dual_terminal_buffered_lifecycle_finalizing_stream(
            Bytes::from_static(
                br#"data: {"type":"response.completed","response":{"id":"resp_v2","usage":{"input_tokens":9,"output_tokens":4,"total_tokens":13}}}

"#,
            ),
            record,
            request_terminal,
            selected_attempt,
            None,
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
            request_terminal,
            selected_attempt,
            request_lease,
            mut record,
            ..
        } = fixture;
        record.annotations_mut().stream = true;
        let mut body = dual_terminal_lifecycle_finalizing_stream_with_idle_timeout(
            Box::pin(stream::once(async {
                Ok(Bytes::from_static(
                    br#"data: {"type":"response.created","response":{"id":"resp_incomplete"}}

"#,
                ))
            })),
            record,
            request_terminal,
            selected_attempt,
            None,
            request_lease,
            std::time::Duration::from_secs(1),
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

    #[tokio::test]
    async fn dual_terminal_finalizer_waits_for_attempt_ack_before_request_terminal() {
        let store = Arc::new(AckGatedStore::new());
        let (writer, worker) = LifecycleWriter::start(4, store.clone()).expect("writer");
        let context = context("dual-terminal-ack", "/v1/chat/completions");
        let request = writer.try_reserve_request().expect("request reservation");
        let (terminal, start_ack) = request.send_start(RequestStartRecord {
            context: context.clone(),
        });
        start_ack
            .await
            .expect("start ack channel")
            .expect("start ack");
        let attempt_reservation = writer.try_reserve_attempt().expect("attempt reservation");
        let attempt_context = attempt_context("dual-terminal-ack", context.received_at_ms);
        let active_requests = Arc::new(AtomicU32::new(0));
        let request_permit = Arc::new(Semaphore::new(1))
            .acquire_owned()
            .await
            .expect("request permit");
        let request_lease = RequestLease::new(request_permit, Arc::clone(&active_requests));
        let record = PendingFinalRequestRecord::new(
            context,
            Some(AttemptId::new("dual-terminal-ack", 0)),
            1,
            0,
            annotations(),
        );

        let mut body = dual_terminal_buffered_lifecycle_finalizing_stream(
            Bytes::from_static(b"ok"),
            record,
            terminal,
            Some((attempt_reservation, attempt_context)),
            None,
            request_lease,
        );
        assert_eq!(
            body.next().await.unwrap().unwrap(),
            Bytes::from_static(b"ok")
        );
        assert!(body.next().await.is_none());
        store.wait_for_attempt_started().await;

        assert_eq!(store.request_calls(), 0);
        assert_eq!(
            writer.snapshot().submitted,
            2,
            "only start + attempt terminal may be submitted before attempt ack"
        );
        assert_eq!(
            active_requests.load(Ordering::SeqCst),
            1,
            "request lease is held until request terminal ack"
        );

        store.release_attempt_ack();
        store.wait_for_request_calls(1).await;
        assert_eq!(writer.snapshot().submitted, 3);
        assert_eq!(
            store.events(),
            vec![
                "start:dual-terminal-ack",
                "attempt:dual-terminal-ack:0",
                "request:dual-terminal-ack",
            ]
        );
        assert_eq!(active_requests.load(Ordering::SeqCst), 0);

        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn dual_terminal_downstream_drop_waits_for_attempt_ack_before_request_interrupted() {
        let store = Arc::new(AckGatedStore::new());
        let (writer, worker) = LifecycleWriter::start(4, store.clone()).expect("writer");
        let context = context("dual-terminal-drop", "/v1/chat/completions");
        let request = writer.try_reserve_request().expect("request reservation");
        let (terminal, start_ack) = request.send_start(RequestStartRecord {
            context: context.clone(),
        });
        start_ack
            .await
            .expect("start ack channel")
            .expect("start ack");
        let attempt_reservation = writer.try_reserve_attempt().expect("attempt reservation");
        let attempt_context = attempt_context("dual-terminal-drop", context.received_at_ms);
        let active_requests = Arc::new(AtomicU32::new(0));
        let request_permit = Arc::new(Semaphore::new(1))
            .acquire_owned()
            .await
            .expect("request permit");
        let request_lease = RequestLease::new(request_permit, Arc::clone(&active_requests));
        let record = PendingFinalRequestRecord::new(
            context,
            Some(AttemptId::new("dual-terminal-drop", 0)),
            1,
            0,
            annotations(),
        );

        let mut body = dual_terminal_buffered_lifecycle_finalizing_stream(
            Bytes::from_static(b"ok"),
            record,
            terminal,
            Some((attempt_reservation, attempt_context)),
            None,
            request_lease,
        );
        assert_eq!(
            body.next().await.unwrap().unwrap(),
            Bytes::from_static(b"ok")
        );
        drop(body);
        store.wait_for_attempt_started().await;
        assert_eq!(store.request_calls(), 0);
        assert_eq!(writer.snapshot().submitted, 2);

        store.release_attempt_ack();
        store.wait_for_request_calls(1).await;
        let request = store.last_request().expect("request");
        assert!(matches!(
            request.terminal.terminal,
            RequestTerminal::Interrupted(_)
        ));
        assert_eq!(
            request.terminal.delivery,
            DeliveryTerminal::DownstreamDropped
        );
        let attempt = store.last_attempt().expect("attempt");
        assert!(matches!(
            attempt.terminal,
            AttemptTerminal::Failed(ref failure)
                if failure.kind == AttemptFailureKind::DownstreamDrop
                    && failure.blame == FailureBlame::Downstream
        ));
        assert!(attempt.output_committed);
        assert_eq!(active_requests.load(Ordering::SeqCst), 0);

        drop(writer);
        worker.join().await.expect("worker join");
    }

    struct RecordingStore {
        start_records: Arc<Mutex<Vec<RequestStartRecord>>>,
        attempt_records: Arc<Mutex<Vec<AttemptTerminalRecord>>>,
        request_records: Arc<Mutex<Vec<FinalRequestRecord>>>,
        fail_attempt: Arc<AtomicBool>,
    }

    struct AckGatedStore {
        events: Arc<Mutex<Vec<String>>>,
        attempt_records: Arc<Mutex<Vec<AttemptTerminalRecord>>>,
        request_records: Arc<Mutex<Vec<FinalRequestRecord>>>,
        attempt_started: Arc<Notify>,
        attempt_release: Arc<Notify>,
    }

    impl AckGatedStore {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
                attempt_records: Arc::new(Mutex::new(Vec::new())),
                request_records: Arc::new(Mutex::new(Vec::new())),
                attempt_started: Arc::new(Notify::new()),
                attempt_release: Arc::new(Notify::new()),
            }
        }

        async fn wait_for_attempt_started(&self) {
            for _ in 0..1_000 {
                if self.attempt_calls() > 0 {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            assert!(
                self.attempt_calls() > 0,
                "attempt terminal was not submitted"
            );
        }

        fn release_attempt_ack(&self) {
            self.attempt_release.notify_waiters();
        }

        fn attempt_calls(&self) -> usize {
            self.attempt_records.lock().expect("attempt lock").len()
        }

        fn request_calls(&self) -> usize {
            self.request_records.lock().expect("request lock").len()
        }

        fn last_request(&self) -> Option<FinalRequestRecord> {
            self.request_records
                .lock()
                .expect("request lock")
                .last()
                .cloned()
        }

        fn last_attempt(&self) -> Option<AttemptTerminalRecord> {
            self.attempt_records
                .lock()
                .expect("attempt lock")
                .last()
                .cloned()
        }

        fn events(&self) -> Vec<String> {
            self.events.lock().expect("events lock").clone()
        }

        async fn wait_for_request_calls(&self, expected: usize) {
            for _ in 0..1_000 {
                if self.request_calls() >= expected {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            assert_eq!(self.request_calls(), expected);
        }
    }

    impl RequestLifecycleStore for AckGatedStore {
        fn start_request(
            &self,
            record: RequestStartRecord,
        ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
            let events = Arc::clone(&self.events);
            Box::pin(async move {
                events
                    .lock()
                    .expect("events lock")
                    .push(format!("start:{}", record.context.request_id));
                Ok(RequestStartAck { inserted: true })
            })
        }

        fn finish_attempt(
            &self,
            record: AttemptTerminalRecord,
        ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
            let events = Arc::clone(&self.events);
            let records = Arc::clone(&self.attempt_records);
            let started = Arc::clone(&self.attempt_started);
            let release = Arc::clone(&self.attempt_release);
            Box::pin(async move {
                events.lock().expect("events lock").push(format!(
                    "attempt:{}:{}",
                    record.context.attempt_id.request_id, record.context.attempt_id.ordinal
                ));
                records.lock().expect("attempt lock").push(record);
                started.notify_waiters();
                release.notified().await;
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
            let events = Arc::clone(&self.events);
            let records = Arc::clone(&self.request_records);
            Box::pin(async move {
                events
                    .lock()
                    .expect("events lock")
                    .push(format!("request:{}", record.context.request_id));
                records.lock().expect("request lock").push(record);
                Ok(RequestCommitAck { finalized: true })
            })
        }
    }

    impl RecordingStore {
        fn new() -> Self {
            Self {
                start_records: Arc::new(Mutex::new(Vec::new())),
                attempt_records: Arc::new(Mutex::new(Vec::new())),
                request_records: Arc::new(Mutex::new(Vec::new())),
                fail_attempt: Arc::new(AtomicBool::new(false)),
            }
        }

        fn fail_attempts(&self) {
            self.fail_attempt.store(true, Ordering::Relaxed);
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
            let fail_attempt = Arc::clone(&self.fail_attempt);
            Box::pin(async move {
                if fail_attempt.load(Ordering::Relaxed) {
                    return Err(LifecycleWriteError::Unavailable(
                        "injected attempt failure".to_string(),
                    ));
                }
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
        request_terminal: RequestTerminalReservation,
        selected_attempt: Option<(AttemptWriteReservation, AttemptContext)>,
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
                    credential_revision: 1,
                    account_revision: 1,
                    group_binding_id: None,
                    group_revision: None,
                    resolved_upstream_model: None,
                    model_alias_revision: 1,
                    started_at_ms: received_at_ms,
                    probe_scope: None,
                    probe_state_revision: None,
                };
                Some((reservation, context))
            } else {
                None
            };
            let selected_attempt_id = selected_attempt
                .as_ref()
                .map(|(_, context)| context.attempt_id.clone());

            Self {
                store,
                writer,
                worker,
                request_terminal: terminal,
                selected_attempt,
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

    fn context(request_id: &str, local_path: &str) -> RequestContextSnapshot {
        let received_at_ms = crate::services::time::now_millis_for_services() as i64;
        RequestContextSnapshot {
            request_id: request_id.to_string(),
            method: "POST".to_string(),
            local_path: local_path.to_string(),
            endpoint: local_path.to_string(),
            received_at_ms,
        }
    }

    fn attempt_context(request_id: &str, started_at_ms: i64) -> AttemptContext {
        AttemptContext {
            attempt_id: AttemptId::new(request_id, 0),
            station_id: "station-test".to_string(),
            station_key_id: "key-test".to_string(),
            endpoint_revision: 1,
            credential_revision: 1,
            account_revision: 1,
            group_binding_id: None,
            group_revision: None,
            resolved_upstream_model: None,
            model_alias_revision: 1,
            started_at_ms,
            probe_scope: None,
            probe_state_revision: None,
        }
    }

    fn annotations() -> RequestLogAnnotations {
        RequestLogAnnotations {
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
        }
    }
}
