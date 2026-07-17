use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use futures_util::Stream;
use tokio::{
    sync::{mpsc, OwnedSemaphorePermit, Semaphore},
    time::Sleep,
};

use super::{
    error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
    observability::SseUsageObserver,
    request::ByteStream,
    routing_repository::{FinalRequestOutcome, RoutingRepository},
};

use crate::services::database::now_millis_for_services;

const DEFAULT_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Clone)]
pub(crate) struct FinalizationDispatcher {
    sender: mpsc::Sender<FinalizationJob>,
    semaphore: Arc<Semaphore>,
}

impl FinalizationDispatcher {
    pub(crate) fn new(capacity: usize, repository: Arc<dyn RoutingRepository>) -> Self {
        let capacity = capacity.max(1);
        let (sender, mut receiver) = mpsc::channel::<FinalizationJob>(capacity);
        tauri::async_runtime::spawn(async move {
            while let Some(job) = receiver.recv().await {
                let _permit = job.permit;
                let _ = repository.record_final_outcome(job.outcome).await;
            }
        });
        Self {
            sender,
            semaphore: Arc::new(Semaphore::new(capacity)),
        }
    }

    pub(crate) fn try_reserve(&self) -> Option<FinalizationLease> {
        Arc::clone(&self.semaphore)
            .try_acquire_owned()
            .ok()
            .map(|permit| FinalizationLease {
                sender: self.sender.clone(),
                permit: Some(permit),
                finalized: Arc::new(AtomicBool::new(false)),
            })
    }
}

pub(crate) struct FinalizationLease {
    sender: mpsc::Sender<FinalizationJob>,
    permit: Option<OwnedSemaphorePermit>,
    finalized: Arc<AtomicBool>,
}

impl FinalizationLease {
    pub(crate) fn finalize(mut self, outcome: FinalRequestOutcome) {
        if self.finalized.swap(true, Ordering::AcqRel) {
            return;
        }
        let Some(permit) = self.permit.take() else {
            return;
        };
        let job = FinalizationJob { outcome, permit };
        match self.sender.try_send(job) {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(job))
            | Err(tokio::sync::mpsc::error::TrySendError::Closed(job)) => {
                let sender = self.sender.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = sender.send(job).await;
                });
            }
        }
    }
}

struct FinalizationJob {
    outcome: FinalRequestOutcome,
    permit: OwnedSemaphorePermit,
}

pub(crate) fn buffered_finalizing_stream(
    body: Bytes,
    outcome: FinalRequestOutcome,
    lease: FinalizationLease,
) -> ByteStream {
    finalizing_stream(
        Box::pin(futures_util::stream::once(async move { Ok(body) })),
        outcome,
        lease,
    )
}

pub(crate) fn finalizing_stream(
    stream: ByteStream,
    outcome: FinalRequestOutcome,
    lease: FinalizationLease,
) -> ByteStream {
    finalizing_stream_with_idle_timeout(stream, outcome, lease, DEFAULT_STREAM_IDLE_TIMEOUT)
}

pub(crate) fn finalizing_stream_with_idle_timeout(
    stream: ByteStream,
    outcome: FinalRequestOutcome,
    lease: FinalizationLease,
    idle_timeout: Duration,
) -> ByteStream {
    Box::pin(FinalizingStream {
        stream,
        outcome: Some(outcome),
        lease: Some(lease),
        observer: SseUsageObserver::default(),
        idle_timeout,
        sleep: None,
        completed: false,
        body_bytes: 0,
        first_token_ms: None,
        started_at_ms: now_millis_for_services() as i64,
    })
}

struct FinalizingStream {
    stream: ByteStream,
    outcome: Option<FinalRequestOutcome>,
    lease: Option<FinalizationLease>,
    observer: SseUsageObserver,
    idle_timeout: Duration,
    sleep: Option<Pin<Box<Sleep>>>,
    completed: bool,
    body_bytes: i64,
    first_token_ms: Option<i64>,
    started_at_ms: i64,
}

impl Stream for FinalizingStream {
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
                self.completed = true;
                self.finalize_once();
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

impl FinalizingStream {
    fn finalize_once(&mut self) {
        self.apply_observations();
        if let (Some(outcome), Some(lease)) = (self.outcome.take(), self.lease.take()) {
            lease.finalize(outcome);
        }
    }

    fn finalize_failure(&mut self, failure: &ProxyFailure, completion_source: &str) {
        if let Some(outcome) = self.outcome.as_mut() {
            outcome.status = "failed".to_string();
            outcome.lifecycle_status = Some(failure.code.as_str().to_string());
            outcome.error_message = Some(failure.public_message.clone());
            outcome.failure_source = Some(failure_source_label(failure.source).to_string());
            outcome.completion_source = Some(completion_source.to_string());
            outcome.feedback = None;
        }
        self.completed = true;
        self.finalize_once();
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
        if let Some(outcome) = self.outcome.as_mut() {
            if self.body_bytes > 0 {
                outcome.body_bytes = Some(self.body_bytes);
            }
            if outcome.first_token_ms.is_none() {
                outcome.first_token_ms = self.first_token_ms;
            }
            if let Some(usage) = self.observer.usage() {
                outcome.prompt_tokens = usage.input_tokens.or(outcome.prompt_tokens);
                outcome.completion_tokens = usage.output_tokens.or(outcome.completion_tokens);
                outcome.total_tokens = usage.total_tokens.or(outcome.total_tokens);
                outcome.cache_creation_tokens = usage
                    .cache_creation_tokens
                    .or(outcome.cache_creation_tokens);
                outcome.cache_read_tokens = usage.cache_read_tokens.or(outcome.cache_read_tokens);
            }
            let now = now_millis_for_services().to_string();
            outcome.finished_at = now;
            outcome.duration_ms =
                Some((now_millis_for_services() as i64 - self.started_at_ms).max(0));
        }
    }

    fn reset_idle_sleep(&mut self) {
        self.sleep = Some(Box::pin(tokio::time::sleep(self.idle_timeout)));
    }

    fn finalize_downstream_drop(&mut self) {
        if let Some(outcome) = self.outcome.as_mut() {
            outcome.status = "interrupted".to_string();
            outcome.lifecycle_status = Some("downstream_dropped".to_string());
            outcome.error_message =
                Some("downstream disconnected before body completion".to_string());
            outcome.failure_source = Some("downstream".to_string());
            outcome.completion_source = Some("downstream_dropped".to_string());
            outcome.feedback = None;
        }
        self.finalize_once();
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

impl Drop for FinalizingStream {
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
    use std::sync::{Arc, Mutex};

    use bytes::Bytes;
    use futures_util::{future::BoxFuture, stream, StreamExt};

    use crate::{
        models::{pricing::BalanceSnapshot, proxy::RequestLog},
        services::proxy::{
            error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
            router::RichRouteCandidate,
            routing_repository::{FinalRequestOutcome, RoutingRepository},
        },
    };

    use super::{
        buffered_finalizing_stream, finalizing_stream, finalizing_stream_with_idle_timeout,
        FinalizationDispatcher,
    };

    #[tokio::test]
    async fn response_body_finalizes_success_only_after_eof() {
        let repository = Arc::new(RecordingRepository::default());
        let dispatcher = FinalizationDispatcher::new(1, repository.clone());
        let lease = dispatcher.try_reserve().expect("lease");
        let mut body = buffered_finalizing_stream(
            Bytes::from_static(b"ok"),
            success_outcome("response-body-eof"),
            lease,
        );

        assert_eq!(repository.calls(), 0);
        assert_eq!(
            body.next().await.unwrap().unwrap(),
            Bytes::from_static(b"ok")
        );
        assert_eq!(repository.calls(), 0, "chunk delivery is not completion");
        assert!(body.next().await.is_none());
        repository.wait_for_calls(1).await;

        assert_eq!(repository.calls(), 1);
        let outcome = repository.last().expect("outcome");
        assert_eq!(outcome.status, "success");
        assert_eq!(outcome.completion_source.as_deref(), Some("upstream"));
    }

    #[tokio::test]
    async fn response_body_drop_after_chunk_before_eof_finalizes_downstream_disconnect_once() {
        let repository = Arc::new(RecordingRepository::default());
        let dispatcher = FinalizationDispatcher::new(1, repository.clone());
        let lease = dispatcher.try_reserve().expect("lease");
        let mut body = buffered_finalizing_stream(
            Bytes::from_static(b"ok"),
            success_outcome("response-body-drop-after-chunk"),
            lease,
        );

        assert_eq!(
            body.next().await.unwrap().unwrap(),
            Bytes::from_static(b"ok")
        );
        drop(body);
        repository.wait_for_calls(1).await;

        assert_eq!(repository.calls(), 1);
        let outcome = repository.last().expect("outcome");
        assert_eq!(outcome.status, "interrupted");
        assert_eq!(
            outcome.lifecycle_status.as_deref(),
            Some("downstream_dropped")
        );
        assert_eq!(
            outcome.completion_source.as_deref(),
            Some("downstream_dropped")
        );
        assert!(outcome.feedback.is_none());
    }

    #[tokio::test]
    async fn response_body_drop_before_poll_finalizes_downstream_disconnect_once() {
        let repository = Arc::new(RecordingRepository::default());
        let dispatcher = FinalizationDispatcher::new(1, repository.clone());
        let lease = dispatcher.try_reserve().expect("lease");
        let body = buffered_finalizing_stream(
            Bytes::from_static(b"ok"),
            success_outcome("response-body-drop-before-poll"),
            lease,
        );

        drop(body);
        repository.wait_for_calls(1).await;

        assert_eq!(repository.calls(), 1);
        let outcome = repository.last().expect("outcome");
        assert_eq!(outcome.status, "interrupted");
        assert_eq!(
            outcome.lifecycle_status.as_deref(),
            Some("downstream_dropped")
        );
    }

    #[tokio::test]
    async fn response_body_repository_failure_does_not_panic_or_retry() {
        let repository = Arc::new(RecordingRepository::failing());
        let dispatcher = FinalizationDispatcher::new(1, repository.clone());
        let lease = dispatcher.try_reserve().expect("lease");
        let mut body = buffered_finalizing_stream(
            Bytes::from_static(b"ok"),
            success_outcome("response-body-repository-failure"),
            lease,
        );

        assert_eq!(
            body.next().await.unwrap().unwrap(),
            Bytes::from_static(b"ok")
        );
        assert!(body.next().await.is_none());
        repository.wait_for_calls(1).await;
        tokio::task::yield_now().await;

        assert_eq!(repository.calls(), 1);
    }

    #[tokio::test]
    async fn response_body_stream_error_finalizes_failure_once() {
        let repository = Arc::new(RecordingRepository::default());
        let dispatcher = FinalizationDispatcher::new(1, repository.clone());
        let lease = dispatcher.try_reserve().expect("lease");
        let mut body = finalizing_stream(
            Box::pin(stream::iter(vec![Err(stream_failure())])),
            success_outcome("response-body-stream-error"),
            lease,
        );

        let failure = body.next().await.unwrap().expect_err("stream failure");
        assert_eq!(failure.code, ProxyFailureCode::UpstreamStreamFailed);
        drop(body);
        repository.wait_for_calls(1).await;

        assert_eq!(repository.calls(), 1);
        let outcome = repository.last().expect("outcome");
        assert_eq!(outcome.status, "failed");
        assert_eq!(
            outcome.lifecycle_status.as_deref(),
            Some("upstream_stream_failed")
        );
        assert_eq!(outcome.failure_source.as_deref(), Some("upstream"));
        assert_eq!(outcome.completion_source.as_deref(), Some("body_error"));
        assert!(outcome.feedback.is_none());
    }

    #[tokio::test]
    async fn response_body_stream_idle_timeout_finalizes_upstream_failure_once() {
        let repository = Arc::new(RecordingRepository::default());
        let dispatcher = FinalizationDispatcher::new(1, repository.clone());
        let lease = dispatcher.try_reserve().expect("lease");
        let mut body = finalizing_stream_with_idle_timeout(
            Box::pin(stream::pending()),
            success_outcome("response-body-idle-timeout"),
            lease,
            std::time::Duration::from_millis(1),
        );

        let failure = body.next().await.unwrap().expect_err("idle timeout");
        assert_eq!(failure.code, ProxyFailureCode::UpstreamStreamFailed);
        drop(body);
        repository.wait_for_calls(1).await;

        assert_eq!(repository.calls(), 1);
        let outcome = repository.last().expect("outcome");
        assert_eq!(outcome.status, "failed");
        assert_eq!(
            outcome.lifecycle_status.as_deref(),
            Some("upstream_stream_failed")
        );
        assert_eq!(outcome.failure_source.as_deref(), Some("upstream"));
        assert_eq!(
            outcome.completion_source.as_deref(),
            Some("body_idle_timeout")
        );
        assert!(outcome.feedback.is_none());
    }

    #[tokio::test]
    async fn response_body_stream_eof_records_sse_usage() {
        let repository = Arc::new(RecordingRepository::default());
        let dispatcher = FinalizationDispatcher::new(1, repository.clone());
        let lease = dispatcher.try_reserve().expect("lease");
        let mut body = finalizing_stream(
            Box::pin(stream::iter(vec![Ok(Bytes::from_static(
                br#"data: {"type":"response.completed","response":{"id":"resp_v2","usage":{"input_tokens":9,"output_tokens":4,"total_tokens":13}}}

"#,
            ))])),
            success_outcome("response-body-sse-usage"),
            lease,
        );

        assert!(body.next().await.unwrap().is_ok());
        assert!(body.next().await.is_none());
        repository.wait_for_calls(1).await;

        let outcome = repository.last().expect("outcome");
        assert_eq!(outcome.status, "success");
        assert_eq!(outcome.prompt_tokens, Some(9));
        assert_eq!(outcome.completion_tokens, Some(4));
        assert_eq!(outcome.total_tokens, Some(13));
    }

    #[derive(Default)]
    struct RecordingRepository {
        outcomes: Mutex<Vec<FinalRequestOutcome>>,
        fail: bool,
    }

    impl RecordingRepository {
        fn failing() -> Self {
            Self {
                outcomes: Mutex::new(Vec::new()),
                fail: true,
            }
        }

        fn calls(&self) -> usize {
            self.outcomes.lock().expect("outcomes lock").len()
        }

        fn last(&self) -> Option<FinalRequestOutcome> {
            self.outcomes.lock().expect("outcomes lock").last().cloned()
        }

        async fn wait_for_calls(&self, expected: usize) {
            for _ in 0..20 {
                if self.calls() >= expected {
                    return;
                }
                tokio::task::yield_now().await;
            }
            assert_eq!(self.calls(), expected);
        }
    }

    impl RoutingRepository for RecordingRepository {
        fn load_runtime_candidates(
            &self,
        ) -> BoxFuture<'static, Result<Vec<RichRouteCandidate>, String>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn record_final_outcome(
            &self,
            outcome: FinalRequestOutcome,
        ) -> BoxFuture<'static, Result<Option<RequestLog>, String>> {
            let fail = self.fail;
            self.outcomes.lock().expect("outcomes lock").push(outcome);
            Box::pin(async move {
                if fail {
                    Err("synthetic finalization failure".to_string())
                } else {
                    Ok(None)
                }
            })
        }

        fn load_balance_snapshots(
            &self,
        ) -> BoxFuture<'static, Result<Vec<BalanceSnapshot>, String>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    fn success_outcome(request_id: &str) -> FinalRequestOutcome {
        let mut outcome = FinalRequestOutcome::success("success");
        outcome.request_id = request_id.to_string();
        outcome.completion_source = Some("upstream".to_string());
        outcome
    }

    #[allow(dead_code)]
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
