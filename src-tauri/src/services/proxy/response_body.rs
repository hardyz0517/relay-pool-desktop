use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_util::Stream;
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};

use super::{
    error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
    request::ByteStream,
    routing_repository::{FinalRequestOutcome, RoutingRepository},
};

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
    Box::pin(FinalizingStream {
        stream,
        outcome: Some(outcome),
        lease: Some(lease),
        completed: false,
    })
}

struct FinalizingStream {
    stream: ByteStream,
    outcome: Option<FinalRequestOutcome>,
    lease: Option<FinalizationLease>,
    completed: bool,
}

impl Stream for FinalizingStream {
    type Item = Result<Bytes, ProxyFailure>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.stream.as_mut().poll_next(_cx) {
            Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(bytes))),
            Poll::Ready(Some(Err(failure))) => {
                self.finalize_failure(&failure);
                Poll::Ready(Some(Err(failure)))
            }
            Poll::Ready(None) => {
                self.completed = true;
                self.finalize_once();
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl FinalizingStream {
    fn finalize_once(&mut self) {
        if let (Some(outcome), Some(lease)) = (self.outcome.take(), self.lease.take()) {
            lease.finalize(outcome);
        }
    }

    fn finalize_failure(&mut self, failure: &ProxyFailure) {
        if let Some(outcome) = self.outcome.as_mut() {
            outcome.status = "failed".to_string();
            outcome.lifecycle_status = Some(failure.code.as_str().to_string());
            outcome.error_message = Some(failure.public_message.clone());
            outcome.failure_source = Some(failure_source_label(failure.source).to_string());
            outcome.completion_source = Some("body_error".to_string());
            outcome.feedback = None;
        }
        self.completed = true;
        self.finalize_once();
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

    use super::{buffered_finalizing_stream, finalizing_stream, FinalizationDispatcher};

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
