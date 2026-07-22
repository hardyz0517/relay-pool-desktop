use std::sync::{
    atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering},
    Arc,
};

use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use super::{
    attempt::AttemptTerminalRecord,
    ports::{
        AttemptCommitAck, LifecycleWriteError, RequestCommitAck, RequestLifecycleStore,
        RequestStartAck,
    },
    request::{FinalRequestRecord, RequestStartRecord},
};

const WRITER_HEALTHY: u8 = 0;
const WRITER_UNHEALTHY: u8 = 1;
const WRITER_CLOSED: u8 = 2;

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
        ack: oneshot::Sender<Result<RequestStartAck, LifecycleWriteError>>,
    },
    FinishAttempt {
        record: Box<AttemptTerminalRecord>,
        ack: oneshot::Sender<Result<AttemptCommitAck, LifecycleWriteError>>,
    },
    FinishRequest {
        record: Box<FinalRequestRecord>,
        ack: oneshot::Sender<Result<RequestCommitAck, LifecycleWriteError>>,
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
                    LifecycleWriteCommand::StartRequest { record, ack } => {
                        let result = store.start_request(*record).await;
                        let failed = result.is_err();
                        if failed {
                            worker_health.mark_unhealthy();
                        }
                        completion.finish(failed);
                        let _ = ack.send(result);
                    }
                    LifecycleWriteCommand::FinishAttempt { record, ack } => {
                        let result = store.finish_attempt(*record).await;
                        let failed = result.is_err();
                        if failed {
                            worker_health.mark_unhealthy();
                        }
                        completion.finish(failed);
                        let _ = ack.send(result);
                    }
                    LifecycleWriteCommand::FinishRequest { record, ack } => {
                        let result = store.finish_request(*record).await;
                        let failed = result.is_err();
                        if failed {
                            worker_health.mark_unhealthy();
                        }
                        completion.finish(failed);
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

    fn ensure_healthy(&self) -> Result<(), WriterAdmissionError> {
        if self.health.is_healthy() {
            Ok(())
        } else {
            Err(WriterAdmissionError::Unhealthy)
        }
    }
}

impl RequestWriteReservation {
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
    use std::sync::Mutex;
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

        let attempt_id = AttemptId::new("req-1", 0);
        let attempt_ack = attempt.send(AttemptTerminalRecord {
            context: AttemptContext {
                attempt_id: attempt_id.clone(),
                station_id: "station-1".to_string(),
                station_key_id: "key-1".to_string(),
                endpoint_revision: 1,
                started_at_ms: 2,
            },
            terminal: AttemptTerminal::Succeeded,
            output_committed: true,
            terminal_at_ms: 3,
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
            vec!["start:req-1", "attempt:req-1:0", "finish:req-1"]
        );
    }

    #[tokio::test]
    async fn permanent_store_error_marks_writer_unhealthy_before_new_admission() {
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
        assert!(!writer.health().is_healthy());
        assert!(matches!(
            writer.try_reserve_request(),
            Err(WriterAdmissionError::Unhealthy)
        ));
        drop(writer);
        tokio::time::timeout(Duration::from_secs(2), worker.join())
            .await
            .expect("worker join timeout")
            .expect("worker join");
    }
}
