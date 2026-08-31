use std::sync::{
    atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering},
    Arc,
};

use std::time::Duration;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use super::{
    attempt::AttemptTerminalRecord,
    ports::{
        AttemptCommitAck, AttemptCostCommitAck, AttemptCostCommitRecord, LifecycleWriteError,
        RequestCommitAck, RequestCostAggregateCommitAck, RequestCostAggregateCommitRecord,
        RequestLifecycleStore, RequestRouteSelectionAck, RequestStartAck,
    },
    request::{
        FinalRequestRecord, RequestLogAnnotations, RequestRouteSelectionRecord, RequestStartRecord,
    },
};

const WRITER_HEALTHY: u8 = 0;
const WRITER_UNHEALTHY: u8 = 1;
const WRITER_CLOSED: u8 = 2;
const MAX_WRITE_ATTEMPTS: usize = 3;
const RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(25), Duration::from_millis(100)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriterAdmissionError {
    Full,
    Closed,
    Unhealthy,
}

#[derive(Debug)]
pub(crate) struct WriterHealth {
    state: AtomicU8,
}

// Snapshot counters are test observability, not part of the production writer API.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LifecycleWriterSnapshot {
    pub(crate) capacity: usize,
    pub(crate) current_outstanding: usize,
    pub(crate) peak_outstanding: usize,
    pub(crate) submitted: u64,
    pub(crate) completed: u64,
    pub(crate) failed: u64,
    pub(crate) cancelled_before_submission: u64,
}

#[derive(Debug)]
struct LifecycleWriterMetrics {
    #[cfg(test)]
    capacity: usize,
    current_outstanding: AtomicUsize,
    peak_outstanding: AtomicUsize,
    submitted: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    cancelled_before_submission: AtomicU64,
}

impl WriterHealth {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(WRITER_HEALTHY),
        }
    }

    pub(crate) fn is_healthy(&self) -> bool {
        self.state.load(Ordering::Acquire) == WRITER_HEALTHY
    }

    fn mark_unhealthy(&self) {
        self.state.store(WRITER_UNHEALTHY, Ordering::Release);
    }

    fn mark_closed(&self) {
        self.state.store(WRITER_CLOSED, Ordering::Release);
    }
}

pub(crate) enum LifecycleWriteCommand {
    StartRequest {
        record: Box<RequestStartRecord>,
        annotations: RequestLogAnnotations,
        ack: oneshot::Sender<Result<RequestStartAck, LifecycleWriteError>>,
    },
    RecordRouteSelection {
        record: Box<RequestRouteSelectionRecord>,
        ack: oneshot::Sender<Result<RequestRouteSelectionAck, LifecycleWriteError>>,
    },
    FinishAttempt {
        record: Box<AttemptTerminalRecord>,
        ack: oneshot::Sender<Result<AttemptCommitAck, LifecycleWriteError>>,
    },
    FinishAttemptCost {
        record: Box<AttemptCostCommitRecord>,
        ack: oneshot::Sender<Result<AttemptCostCommitAck, LifecycleWriteError>>,
    },
    FinishRequest {
        record: Box<FinalRequestRecord>,
        ack: oneshot::Sender<Result<RequestCommitAck, LifecycleWriteError>>,
    },
    FinishRequestCostAggregate {
        record: Box<RequestCostAggregateCommitRecord>,
        ack: oneshot::Sender<Result<RequestCostAggregateCommitAck, LifecycleWriteError>>,
    },
}

#[derive(Clone)]
pub(crate) struct LifecycleWriter {
    sender: mpsc::Sender<QueuedLifecycleWriteCommand>,
    health: Arc<WriterHealth>,
    metrics: Arc<LifecycleWriterMetrics>,
}

pub(crate) struct LifecycleWriterWorker {
    join: JoinHandle<()>,
}

pub(crate) struct RequestWriteReservation {
    start: ReservationSlot,
    terminal: ReservationSlot,
}

pub(crate) struct AttemptWriteReservation {
    terminal: ReservationSlot,
}

pub(crate) struct AttemptCostWriteReservation {
    terminal: ReservationSlot,
}

pub(crate) struct RequestCostAggregateWriteReservation {
    terminal: ReservationSlot,
}

struct ReservationSlot {
    permit: Option<mpsc::OwnedPermit<QueuedLifecycleWriteCommand>>,
    metrics: Arc<LifecycleWriterMetrics>,
}

struct QueuedLifecycleWriteCommand {
    command: LifecycleWriteCommand,
    completion: CommandCompletion,
}

impl LifecycleWriter {
    pub(crate) fn start(
        capacity: usize,
        store: Arc<dyn RequestLifecycleStore>,
    ) -> Result<(Self, LifecycleWriterWorker), WriterAdmissionError> {
        if capacity < 2 {
            return Err(WriterAdmissionError::Full);
        }
        let (sender, mut receiver) = mpsc::channel(capacity);
        let health = Arc::new(WriterHealth::new());
        let metrics = Arc::new(LifecycleWriterMetrics::new(capacity));
        let worker_health = Arc::clone(&health);
        let join = tokio::spawn(async move {
            while let Some(queued) = receiver.recv().await {
                let QueuedLifecycleWriteCommand {
                    command,
                    completion,
                } = queued;
                match command {
                    LifecycleWriteCommand::StartRequest {
                        record,
                        annotations,
                        ack,
                    } => {
                        let request_id = record.context.request_id.clone();
                        let store = Arc::clone(&store);
                        let result = write_with_retry("start_request", &request_id, || {
                            store.start_request_with_annotations(
                                (*record).clone(),
                                annotations.clone(),
                            )
                        })
                        .await;
                        if matches!(result, Err(LifecycleWriteError::CommitOutcomeUnknown(_))) {
                            worker_health.mark_unhealthy();
                        }
                        completion.finish(result.is_err());
                        let _ = ack.send(result);
                    }
                    LifecycleWriteCommand::RecordRouteSelection { record, ack } => {
                        let request_id = record.request_id.clone();
                        let store = Arc::clone(&store);
                        let result =
                            write_with_retry("record_route_selection", &request_id, || {
                                store.record_route_selection((*record).clone())
                            })
                            .await;
                        // Route selection is an idempotent observational
                        // projection. Its failure cannot make terminal writes
                        // unsafe or poison lifecycle admission.
                        completion.finish(result.is_err());
                        let _ = ack.send(result);
                    }
                    LifecycleWriteCommand::FinishAttempt { record, ack } => {
                        let request_id = record.context.attempt_id.request_id.clone();
                        let store = Arc::clone(&store);
                        let result = write_with_retry("finish_attempt", &request_id, || {
                            store.finish_attempt((*record).clone())
                        })
                        .await;
                        if matches!(result, Err(LifecycleWriteError::CommitOutcomeUnknown(_))) {
                            worker_health.mark_unhealthy();
                        }
                        completion.finish(result.is_err());
                        let _ = ack.send(result);
                    }
                    LifecycleWriteCommand::FinishAttemptCost { record, ack } => {
                        let request_id = record.request_id.clone();
                        let store = Arc::clone(&store);
                        let result = write_with_retry("finish_attempt_cost", &request_id, || {
                            store.finish_attempt_cost((*record).clone())
                        })
                        .await;
                        if matches!(result, Err(LifecycleWriteError::CommitOutcomeUnknown(_))) {
                            worker_health.mark_unhealthy();
                        }
                        completion.finish(result.is_err());
                        let _ = ack.send(result);
                    }
                    LifecycleWriteCommand::FinishRequest { record, ack } => {
                        let request_id = record.context.request_id.clone();
                        let store = Arc::clone(&store);
                        let result = write_with_retry("finish_request", &request_id, || {
                            store.finish_request((*record).clone())
                        })
                        .await;
                        if matches!(result, Err(LifecycleWriteError::CommitOutcomeUnknown(_))) {
                            worker_health.mark_unhealthy();
                        }
                        completion.finish(result.is_err());
                        let _ = ack.send(result);
                    }
                    LifecycleWriteCommand::FinishRequestCostAggregate { record, ack } => {
                        let request_id = record.request_id.clone();
                        let store = Arc::clone(&store);
                        let result =
                            write_with_retry("finish_request_cost_aggregate", &request_id, || {
                                store.finish_request_cost_aggregate((*record).clone())
                            })
                            .await;
                        if matches!(result, Err(LifecycleWriteError::CommitOutcomeUnknown(_))) {
                            worker_health.mark_unhealthy();
                        }
                        completion.finish(result.is_err());
                        let _ = ack.send(result);
                    }
                }
            }
            worker_health.mark_closed();
        });
        Ok((
            Self {
                sender,
                health: Arc::clone(&health),
                metrics,
            },
            LifecycleWriterWorker { join },
        ))
    }

    #[cfg(test)]
    pub(crate) fn health(&self) -> &Arc<WriterHealth> {
        &self.health
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> LifecycleWriterSnapshot {
        self.metrics.snapshot()
    }

    pub(crate) fn try_reserve_request(
        &self,
    ) -> Result<RequestWriteReservation, WriterAdmissionError> {
        self.ensure_healthy()?;
        let start = reserve(&self.sender, &self.metrics)?;
        let terminal = reserve(&self.sender, &self.metrics)?;
        Ok(RequestWriteReservation { start, terminal })
    }

    pub(crate) fn try_reserve_attempt(
        &self,
    ) -> Result<AttemptWriteReservation, WriterAdmissionError> {
        self.ensure_healthy()?;
        Ok(AttemptWriteReservation {
            terminal: reserve(&self.sender, &self.metrics)?,
        })
    }

    pub(crate) fn try_record_route_selection(
        &self,
        record: RequestRouteSelectionRecord,
    ) -> Result<
        oneshot::Receiver<Result<RequestRouteSelectionAck, LifecycleWriteError>>,
        WriterAdmissionError,
    > {
        self.ensure_healthy()?;
        let slot = reserve(&self.sender, &self.metrics)?;
        let (ack, receiver) = oneshot::channel();
        slot.send(LifecycleWriteCommand::RecordRouteSelection {
            record: Box::new(record),
            ack,
        });
        Ok(receiver)
    }

    pub(crate) fn try_reserve_attempt_cost(
        &self,
    ) -> Result<AttemptCostWriteReservation, WriterAdmissionError> {
        self.ensure_healthy()?;
        Ok(AttemptCostWriteReservation {
            terminal: reserve(&self.sender, &self.metrics)?,
        })
    }

    pub(crate) fn try_reserve_request_cost_aggregate(
        &self,
    ) -> Result<RequestCostAggregateWriteReservation, WriterAdmissionError> {
        self.ensure_healthy()?;
        Ok(RequestCostAggregateWriteReservation {
            terminal: reserve(&self.sender, &self.metrics)?,
        })
    }

    fn ensure_healthy(&self) -> Result<(), WriterAdmissionError> {
        if self.health.is_healthy() {
            Ok(())
        } else {
            Err(WriterAdmissionError::Unhealthy)
        }
    }
}

impl RequestWriteReservation {
    pub(crate) fn into_terminal(self) -> RequestTerminalReservation {
        RequestTerminalReservation {
            terminal: self.terminal,
        }
    }

    pub(crate) fn send_start(
        self,
        record: RequestStartRecord,
    ) -> (
        RequestTerminalReservation,
        oneshot::Receiver<Result<RequestStartAck, LifecycleWriteError>>,
    ) {
        let (ack, receiver) = oneshot::channel();
        self.start.send(LifecycleWriteCommand::StartRequest {
            record: Box::new(record),
            annotations: RequestLogAnnotations::default(),
            ack,
        });
        (
            RequestTerminalReservation {
                terminal: self.terminal,
            },
            receiver,
        )
    }

    pub(crate) fn send_start_with_annotations(
        self,
        record: RequestStartRecord,
        annotations: RequestLogAnnotations,
    ) -> (
        RequestTerminalReservation,
        oneshot::Receiver<Result<RequestStartAck, LifecycleWriteError>>,
    ) {
        let (ack, receiver) = oneshot::channel();
        self.start.send(LifecycleWriteCommand::StartRequest {
            record: Box::new(record),
            annotations,
            ack,
        });
        (
            RequestTerminalReservation {
                terminal: self.terminal,
            },
            receiver,
        )
    }
}

pub(crate) struct RequestTerminalReservation {
    terminal: ReservationSlot,
}

impl RequestTerminalReservation {
    pub(crate) fn send(
        self,
        record: FinalRequestRecord,
    ) -> oneshot::Receiver<Result<RequestCommitAck, LifecycleWriteError>> {
        let (ack, receiver) = oneshot::channel();
        self.terminal.send(LifecycleWriteCommand::FinishRequest {
            record: Box::new(record),
            ack,
        });
        receiver
    }
}

impl AttemptWriteReservation {
    pub(crate) fn send(
        self,
        record: AttemptTerminalRecord,
    ) -> oneshot::Receiver<Result<AttemptCommitAck, LifecycleWriteError>> {
        let (ack, receiver) = oneshot::channel();
        self.terminal.send(LifecycleWriteCommand::FinishAttempt {
            record: Box::new(record),
            ack,
        });
        receiver
    }
}

impl AttemptCostWriteReservation {
    pub(crate) fn send(
        self,
        record: AttemptCostCommitRecord,
    ) -> oneshot::Receiver<Result<AttemptCostCommitAck, LifecycleWriteError>> {
        let (ack, receiver) = oneshot::channel();
        self.terminal
            .send(LifecycleWriteCommand::FinishAttemptCost {
                record: Box::new(record),
                ack,
            });
        receiver
    }
}

impl RequestCostAggregateWriteReservation {
    pub(crate) fn send(
        self,
        record: RequestCostAggregateCommitRecord,
    ) -> oneshot::Receiver<Result<RequestCostAggregateCommitAck, LifecycleWriteError>> {
        let (ack, receiver) = oneshot::channel();
        self.terminal
            .send(LifecycleWriteCommand::FinishRequestCostAggregate {
                record: Box::new(record),
                ack,
            });
        receiver
    }
}

impl LifecycleWriterWorker {
    pub(crate) async fn join(self) -> Result<(), tokio::task::JoinError> {
        self.join.await
    }
}

fn reserve(
    sender: &mpsc::Sender<QueuedLifecycleWriteCommand>,
    metrics: &Arc<LifecycleWriterMetrics>,
) -> Result<ReservationSlot, WriterAdmissionError> {
    let permit = sender
        .clone()
        .try_reserve_owned()
        .map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => WriterAdmissionError::Full,
            mpsc::error::TrySendError::Closed(_) => WriterAdmissionError::Closed,
        })?;
    metrics.reserve();
    Ok(ReservationSlot {
        permit: Some(permit),
        metrics: Arc::clone(metrics),
    })
}

fn retryable_write_error(error: &LifecycleWriteError) -> bool {
    match error {
        LifecycleWriteError::DatabaseBusy => true,
        LifecycleWriteError::Unavailable(detail) => {
            detail == "runtime is not accepting new persistence work"
        }
        LifecycleWriteError::CommitOutcomeUnknown(_) => false,
    }
}

async fn write_with_retry<T, F>(
    _operation: &'static str,
    _request_id: &str,
    mut write: F,
) -> Result<T, LifecycleWriteError>
where
    F: FnMut() -> futures_util::future::BoxFuture<'static, Result<T, LifecycleWriteError>>,
{
    for attempt in 1..=MAX_WRITE_ATTEMPTS {
        match write().await {
            Ok(value) => return Ok(value),
            Err(error) if attempt < MAX_WRITE_ATTEMPTS && retryable_write_error(&error) => {
                let _ = (attempt, MAX_WRITE_ATTEMPTS);
                emit_runtime_event(LifecycleWriterEvent::PersistenceRetry);
                tokio::time::sleep(RETRY_DELAYS[attempt - 1]).await;
            }
            Err(error) => {
                let _ = attempt;
                emit_runtime_event(LifecycleWriterEvent::PersistenceFailed);
                return Err(error);
            }
        }
    }
    unreachable!("lifecycle writer retry loop must return");
}

// Lifecycle writer is also compiled as a standalone source module by a few
// integration fixtures. Keep those fixtures independent from the full Tauri
// observability root while preserving the production event adapter.
#[derive(Clone, Copy)]
enum LifecycleWriterEvent {
    PersistenceRetry,
    PersistenceFailed,
}

#[cfg(test)]
fn emit_runtime_event(_event: LifecycleWriterEvent) {}

#[cfg(not(test))]
fn emit_runtime_event(event: LifecycleWriterEvent) {
    let descriptor = match event {
        LifecycleWriterEvent::PersistenceRetry => {
            crate::services::proxy::runtime_events::persistence_retry()
        }
        LifecycleWriterEvent::PersistenceFailed => {
            crate::services::proxy::runtime_events::persistence_failed()
        }
    };
    crate::observability::runtime::bootstrap::emit(descriptor);
}

impl LifecycleWriterMetrics {
    fn new(_capacity: usize) -> Self {
        Self {
            #[cfg(test)]
            capacity: _capacity,
            current_outstanding: AtomicUsize::new(0),
            peak_outstanding: AtomicUsize::new(0),
            submitted: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            cancelled_before_submission: AtomicU64::new(0),
        }
    }

    fn reserve(&self) {
        let current = self.current_outstanding.fetch_add(1, Ordering::AcqRel) + 1;
        self.peak_outstanding.fetch_max(current, Ordering::AcqRel);
    }

    fn submit(&self) {
        self.submitted.fetch_add(1, Ordering::Relaxed);
    }

    fn finish(&self, failed: bool) {
        if failed {
            self.failed.fetch_add(1, Ordering::Relaxed);
        } else {
            self.completed.fetch_add(1, Ordering::Relaxed);
        }
        self.release();
    }

    fn cancel(&self) {
        self.cancelled_before_submission
            .fetch_add(1, Ordering::Relaxed);
        self.release();
    }

    fn release(&self) {
        let previous = self.current_outstanding.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "lifecycle writer outstanding underflow");
    }

    #[cfg(test)]
    fn snapshot(&self) -> LifecycleWriterSnapshot {
        LifecycleWriterSnapshot {
            capacity: self.capacity,
            current_outstanding: self.current_outstanding.load(Ordering::Acquire),
            peak_outstanding: self.peak_outstanding.load(Ordering::Acquire),
            submitted: self.submitted.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            cancelled_before_submission: self.cancelled_before_submission.load(Ordering::Relaxed),
        }
    }
}

impl ReservationSlot {
    fn send(mut self, command: LifecycleWriteCommand) {
        let permit = self.permit.take().expect("reservation permit");
        self.metrics.submit();
        permit.send(QueuedLifecycleWriteCommand {
            command,
            completion: CommandCompletion::new(Arc::clone(&self.metrics)),
        });
    }
}

impl Drop for ReservationSlot {
    fn drop(&mut self) {
        if self.permit.is_some() {
            self.metrics.cancel();
        }
    }
}

struct CommandCompletion {
    metrics: Arc<LifecycleWriterMetrics>,
    finished: bool,
}

impl CommandCompletion {
    fn new(metrics: Arc<LifecycleWriterMetrics>) -> Self {
        Self {
            metrics,
            finished: false,
        }
    }

    fn finish(mut self, failed: bool) {
        self.metrics.finish(failed);
        self.finished = true;
    }
}

impl Drop for CommandCompletion {
    fn drop(&mut self) {
        if !self.finished {
            self.metrics.finish(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    use std::time::Duration;

    use futures_util::future::BoxFuture;

    use super::*;
    use crate::services::proxy::lifecycle::{
        attempt::{AttemptContext, AttemptTerminal},
        delivery::DeliveryTerminal,
        request::{
            AttemptId, RequestCompletion, RequestContextSnapshot, RequestTerminal,
            RequestTerminalSnapshot,
        },
    };

    #[derive(Default)]
    struct RecordingStore {
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl RequestLifecycleStore for RecordingStore {
        fn start_request(
            &self,
            record: RequestStartRecord,
        ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
            let calls = Arc::clone(&self.calls);
            Box::pin(async move {
                calls
                    .lock()
                    .expect("calls")
                    .push(format!("start:{}", record.context.request_id));
                Ok(RequestStartAck { inserted: true })
            })
        }

        fn record_route_selection(
            &self,
            record: RequestRouteSelectionRecord,
        ) -> BoxFuture<'static, Result<RequestRouteSelectionAck, LifecycleWriteError>> {
            let calls = Arc::clone(&self.calls);
            Box::pin(async move {
                calls.lock().expect("calls").push(format!(
                    "route:{}:{}",
                    record.request_id, record.station_key_id
                ));
                Ok(RequestRouteSelectionAck { updated: true })
            })
        }

        fn finish_attempt(
            &self,
            record: AttemptTerminalRecord,
        ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
            let calls = Arc::clone(&self.calls);
            Box::pin(async move {
                calls.lock().expect("calls").push(format!(
                    "attempt:{}:{}",
                    record.context.attempt_id.request_id, record.context.attempt_id.ordinal
                ));
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
            let calls = Arc::clone(&self.calls);
            Box::pin(async move {
                calls
                    .lock()
                    .expect("calls")
                    .push(format!("finish:{}", record.context.request_id));
                Ok(RequestCommitAck { finalized: true })
            })
        }

        fn finish_attempt_cost(
            &self,
            record: AttemptCostCommitRecord,
        ) -> BoxFuture<'static, Result<AttemptCostCommitAck, LifecycleWriteError>> {
            let calls = Arc::clone(&self.calls);
            Box::pin(async move {
                calls
                    .lock()
                    .expect("calls")
                    .push(format!("cost:{}:{}", record.request_id, record.ordinal));
                Ok(AttemptCostCommitAck { inserted: true })
            })
        }

        fn finish_request_cost_aggregate(
            &self,
            record: RequestCostAggregateCommitRecord,
        ) -> BoxFuture<'static, Result<RequestCostAggregateCommitAck, LifecycleWriteError>>
        {
            let calls = Arc::clone(&self.calls);
            Box::pin(async move {
                calls
                    .lock()
                    .expect("calls")
                    .push(format!("aggregate:{}", record.request_id));
                Ok(RequestCostAggregateCommitAck { inserted: true })
            })
        }
    }

    struct FailingStore;

    impl RequestLifecycleStore for FailingStore {
        fn start_request(
            &self,
            _record: RequestStartRecord,
        ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
            Box::pin(async {
                Err(LifecycleWriteError::Unavailable(
                    "test persistence failure".to_string(),
                ))
            })
        }

        fn finish_attempt(
            &self,
            _record: AttemptTerminalRecord,
        ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
            Box::pin(async {
                Err(LifecycleWriteError::Unavailable(
                    "test persistence failure".to_string(),
                ))
            })
        }

        fn finish_request(
            &self,
            _record: FinalRequestRecord,
        ) -> BoxFuture<'static, Result<RequestCommitAck, LifecycleWriteError>> {
            Box::pin(async {
                Err(LifecycleWriteError::Unavailable(
                    "test persistence failure".to_string(),
                ))
            })
        }

        fn finish_attempt_cost(
            &self,
            _record: AttemptCostCommitRecord,
        ) -> BoxFuture<'static, Result<AttemptCostCommitAck, LifecycleWriteError>> {
            Box::pin(async {
                Err(LifecycleWriteError::Unavailable(
                    "test persistence failure".to_string(),
                ))
            })
        }
    }

    struct BusyThenHealthyStore {
        calls: Arc<AtomicUsize>,
    }

    impl RequestLifecycleStore for BusyThenHealthyStore {
        fn start_request(
            &self,
            _record: RequestStartRecord,
        ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
            let calls = Arc::clone(&self.calls);
            Box::pin(async move {
                let call = calls.fetch_add(1, Ordering::Relaxed);
                if call < 2 {
                    Err(LifecycleWriteError::DatabaseBusy)
                } else {
                    Ok(RequestStartAck { inserted: true })
                }
            })
        }

        fn finish_attempt(
            &self,
            _record: AttemptTerminalRecord,
        ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
            Box::pin(async {
                Ok(AttemptCommitAck {
                    inserted: true,
                    health_applied: true,
                })
            })
        }

        fn finish_request(
            &self,
            _record: FinalRequestRecord,
        ) -> BoxFuture<'static, Result<RequestCommitAck, LifecycleWriteError>> {
            Box::pin(async { Ok(RequestCommitAck { finalized: true }) })
        }
    }

    fn context() -> RequestContextSnapshot {
        RequestContextSnapshot {
            request_id: "req-1".to_string(),
            method: "POST".to_string(),
            local_path: "/v1/chat/completions".to_string(),
            endpoint: "chat_completions".to_string(),
            received_at_ms: 1,
        }
    }

    fn attempt_cost_record(request_id: &str, ordinal: u16) -> AttemptCostCommitRecord {
        AttemptCostCommitRecord {
            request_id: request_id.to_string(),
            ordinal,
            pricing_context_id: format!("pricing-{request_id}-{ordinal}"),
            pricing_basis: "exact_price".to_string(),
            pricing_status_label: "exact".to_string(),
            usage_status: "complete".to_string(),
            input_tokens: Some(10),
            output_tokens: Some(5),
            total_tokens: Some(15),
            cache_creation_tokens: None,
            cache_read_tokens: None,
            cost_status: "priced".to_string(),
            currency: Some("USD".to_string()),
            total_cost_micro: Some(123),
            created_at_ms: 10,
        }
    }

    fn request_cost_aggregate_record(request_id: &str) -> RequestCostAggregateCommitRecord {
        RequestCostAggregateCommitRecord {
            request_id: request_id.to_string(),
            status: "complete_single_currency".to_string(),
            totals_by_currency_json: r#"{"USD":123}"#.to_string(),
            compatibility_currency: Some("USD".to_string()),
            compatibility_total_cost_micro: Some(123),
            incomplete_attempts_json: "[]".to_string(),
            written_at_ms: 20,
        }
    }

    #[tokio::test]
    async fn one_channel_preserves_parent_attempt_terminal_order() {
        let store = Arc::new(RecordingStore::default());
        let calls = Arc::clone(&store.calls);
        let (writer, worker) = LifecycleWriter::start(3, store).expect("writer");
        let request = writer.try_reserve_request().expect("request permits");
        let attempt = writer.try_reserve_attempt().expect("attempt permit");
        assert!(matches!(
            writer.try_reserve_attempt(),
            Err(WriterAdmissionError::Full)
        ));

        let (request_terminal, start_ack) =
            request.send_start(RequestStartRecord { context: context() });
        assert!(
            start_ack
                .await
                .expect("start ack channel")
                .expect("start ack")
                .inserted
        );

        let route_ack = writer
            .try_record_route_selection(RequestRouteSelectionRecord {
                request_id: "req-1".to_string(),
                attempt_ordinal: 0,
                station_key_id: "key-1".to_string(),
                station_id: "station-1".to_string(),
                route_policy: "stable_first".to_string(),
                route_reason: "selected key-1 for /v1/chat/completions".to_string(),
                selected_at_ms: 2,
            })
            .expect("route selection admission");
        drop(route_ack);

        let attempt_id = AttemptId::new("req-1", 0);
        let attempt_ack = attempt.send(AttemptTerminalRecord {
            context: AttemptContext {
                attempt_id: attempt_id.clone(),
                station_id: "station-1".to_string(),
                station_key_id: "key-1".to_string(),
                endpoint_revision: 1,
                credential_revision: 1,
                account_revision: 1,
                group_binding_id: None,
                group_revision: None,
                resolved_upstream_model: None,
                comparability_key: None,
                model_alias_revision: 1,
                started_at_ms: 2,
                probe_scope: None,
                probe_state_revision: None,
            },
            terminal: AttemptTerminal::Succeeded,
            output_committed: true,
            terminal_at_ms: 3,
            probe_scope: None,
            probe_state_revision: None,
        });
        assert!(
            attempt_ack
                .await
                .expect("attempt ack channel")
                .expect("attempt ack")
                .inserted
        );

        let terminal = RequestTerminal::Completed(RequestCompletion {
            protocol_completed: true,
            attempt_id: Some(attempt_id.clone()),
        });
        let finish_ack = request_terminal.send(FinalRequestRecord {
            context: context(),
            terminal: RequestTerminalSnapshot {
                terminal,
                delivery: DeliveryTerminal::BodyCompleted,
            },
            selected_attempt_id: Some(attempt_id),
            attempt_count: 1,
            fallback_count: 0,
            annotations: Default::default(),
            routing_outcome: None,
        });
        assert!(
            finish_ack
                .await
                .expect("finish ack channel")
                .expect("finish ack")
                .finalized
        );

        drop(writer);
        worker.join().await.expect("worker join");
        assert_eq!(
            *calls.lock().expect("calls"),
            vec![
                "start:req-1",
                "route:req-1:key-1",
                "attempt:req-1:0",
                "finish:req-1"
            ]
        );
    }

    #[tokio::test]
    async fn non_retryable_store_error_does_not_poison_new_admission() {
        let (writer, worker) = LifecycleWriter::start(2, Arc::new(FailingStore)).expect("writer");
        let request = writer.try_reserve_request().expect("request permits");
        let (terminal_reservation, ack) =
            request.send_start(RequestStartRecord { context: context() });
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(2), ack)
                .await
                .expect("start ack timeout")
                .expect("ack channel"),
            Err(LifecycleWriteError::Unavailable(_))
        ));
        drop(terminal_reservation);
        assert!(writer.health().is_healthy());
        let next = writer
            .try_reserve_request()
            .expect("new admission remains available");
        drop(next);
        drop(writer);
        tokio::time::timeout(Duration::from_secs(2), worker.join())
            .await
            .expect("worker join timeout")
            .expect("worker join");
    }

    #[tokio::test]
    async fn busy_store_error_retries_and_recovers_without_poisoning_writer() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (writer, worker) = LifecycleWriter::start(
            2,
            Arc::new(BusyThenHealthyStore {
                calls: Arc::clone(&calls),
            }),
        )
        .expect("writer");
        let request = writer.try_reserve_request().expect("request permits");
        let (terminal, ack) = request.send_start(RequestStartRecord { context: context() });
        assert!(
            ack.await
                .expect("ack channel")
                .expect("busy retry should recover")
                .inserted
        );
        assert_eq!(calls.load(Ordering::Relaxed), 3);
        assert!(writer.health().is_healthy());
        // The terminal reservation still owns an mpsc permit; dropping it
        // releases the last sender so the worker observes channel close and
        // the join below terminates instead of blocking forever.
        drop(terminal);
        drop(writer);
        worker.join().await.expect("worker join");
    }

    #[tokio::test]
    async fn cost_commands_use_the_same_bounded_writer_and_ack_path() {
        let store = Arc::new(RecordingStore::default());
        let calls = Arc::clone(&store.calls);
        let (writer, worker) = LifecycleWriter::start(2, store).expect("writer");
        let attempt_cost = writer.try_reserve_attempt_cost().expect("attempt cost");
        let aggregate = writer
            .try_reserve_request_cost_aggregate()
            .expect("aggregate cost");
        assert!(matches!(
            writer.try_reserve_attempt_cost(),
            Err(WriterAdmissionError::Full)
        ));

        let cost_ack = attempt_cost.send(attempt_cost_record("req-cost-writer", 0));
        assert!(
            cost_ack
                .await
                .expect("attempt cost ack channel")
                .expect("attempt cost ack")
                .inserted
        );
        let aggregate_ack = aggregate.send(request_cost_aggregate_record("req-cost-writer"));
        assert!(
            aggregate_ack
                .await
                .expect("aggregate ack channel")
                .expect("aggregate ack")
                .inserted
        );

        drop(writer);
        worker.join().await.expect("worker join");
        assert_eq!(
            *calls.lock().expect("calls"),
            vec!["cost:req-cost-writer:0", "aggregate:req-cost-writer"]
        );
    }

    #[tokio::test]
    async fn cost_command_store_error_does_not_poison_writer() {
        let (writer, worker) = LifecycleWriter::start(2, Arc::new(FailingStore)).expect("writer");
        let attempt_cost = writer.try_reserve_attempt_cost().expect("attempt cost");
        let ack = attempt_cost.send(attempt_cost_record("req-cost-fail", 0));
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(2), ack)
                .await
                .expect("attempt cost ack timeout")
                .expect("ack channel"),
            Err(LifecycleWriteError::Unavailable(_))
        ));
        assert!(writer.health().is_healthy());
        let reservation = writer
            .try_reserve_attempt_cost()
            .expect("new cost admission remains available");
        drop(reservation);
        drop(writer);
        tokio::time::timeout(Duration::from_secs(2), worker.join())
            .await
            .expect("worker join timeout")
            .expect("worker join");
    }
}
