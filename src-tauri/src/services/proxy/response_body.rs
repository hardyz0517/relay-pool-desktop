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
    Box::pin(BufferedFinalizingStream {
        body: Some(body),
        outcome: Some(outcome),
        lease: Some(lease),
    })
}

struct BufferedFinalizingStream {
    body: Option<Bytes>,
    outcome: Option<FinalRequestOutcome>,
    lease: Option<FinalizationLease>,
}

impl Stream for BufferedFinalizingStream {
    type Item = Result<Bytes, ProxyFailure>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(body) = self.body.take() {
            return Poll::Ready(Some(Ok(body)));
        }
        self.finalize_once();
        Poll::Ready(None)
    }
}

impl BufferedFinalizingStream {
    fn finalize_once(&mut self) {
        if let (Some(outcome), Some(lease)) = (self.outcome.take(), self.lease.take()) {
            lease.finalize(outcome);
        }
    }
}

impl Drop for BufferedFinalizingStream {
    fn drop(&mut self) {
        if self.body.is_some() {
            if let Some(outcome) = self.outcome.as_mut() {
                outcome.status = "interrupted".to_string();
                outcome.lifecycle_status = Some("downstream_dropped".to_string());
                outcome.error_message =
                    Some("downstream disconnected before body completion".to_string());
                outcome.feedback = None;
            }
        }
        self.finalize_once();
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
