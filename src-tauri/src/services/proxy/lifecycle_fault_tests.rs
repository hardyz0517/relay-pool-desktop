use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use futures_util::future::BoxFuture;

use super::lifecycle::{
    attempt::{
        AttemptContext, AttemptTerminal, AttemptTerminalRecord, ClassifiedAttemptFailure,
        FailureBlame, HealthEffect, RetryDisposition,
    },
    delivery::DeliveryTerminal,
    ports::{
        AttemptCommitAck, LifecycleWriteError, RequestCommitAck, RequestLifecycleStore,
        RequestStartAck,
    },
    request::{
        AttemptId, FinalRequestRecord, RequestCompletion, RequestContextSnapshot,
        RequestLogAnnotations, RequestStartRecord, RequestTerminal, RequestTerminalSnapshot,
    },
    writer::{LifecycleWriter, WriterAdmissionError},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaultBoundary {
    RequestStart,
    AttemptTerminal,
    RequestTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaultKind {
    Unavailable,
    CommitOutcomeUnknown,
}

struct FaultingStore {
    boundary: FaultBoundary,
    fault: FaultKind,
}

impl RequestLifecycleStore for FaultingStore {
    fn start_request(
        &self,
        _record: RequestStartRecord,
    ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
        let result = if self.boundary == FaultBoundary::RequestStart {
            Err(write_error(self.fault, "request-start"))
        } else {
            Ok(RequestStartAck { inserted: true })
        };
        Box::pin(async move { result })
    }

    fn finish_attempt(
        &self,
        _record: AttemptTerminalRecord,
    ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
        let result = if self.boundary == FaultBoundary::AttemptTerminal {
            Err(write_error(self.fault, "attempt-terminal"))
        } else {
            Ok(AttemptCommitAck {
                inserted: true,
                health_applied: true,
            })
        };
        Box::pin(async move { result })
    }

    fn finish_request(
        &self,
        _record: FinalRequestRecord,
    ) -> BoxFuture<'static, Result<RequestCommitAck, LifecycleWriteError>> {
        let result = if self.boundary == FaultBoundary::RequestTerminal {
            Err(write_error(self.fault, "request-terminal"))
        } else {
            Ok(RequestCommitAck { finalized: true })
        };
        Box::pin(async move { result })
    }
}

struct PanicStore;

impl RequestLifecycleStore for PanicStore {
    fn start_request(
        &self,
        _record: RequestStartRecord,
    ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
        Box::pin(async { panic!("injected lifecycle writer panic") })
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

#[derive(Default)]
struct CountingStore {
    calls: Arc<AtomicUsize>,
}

impl RequestLifecycleStore for CountingStore {
    fn start_request(
        &self,
        _record: RequestStartRecord,
    ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
        let calls = Arc::clone(&self.calls);
        Box::pin(async move {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok(RequestStartAck { inserted: true })
        })
    }

    fn finish_attempt(
        &self,
        _record: AttemptTerminalRecord,
    ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
        let calls = Arc::clone(&self.calls);
        Box::pin(async move {
            calls.fetch_add(1, Ordering::Relaxed);
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
        let calls = Arc::clone(&self.calls);
        Box::pin(async move {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok(RequestCommitAck { finalized: true })
        })
    }
}

#[tokio::test]
async fn lifecycle_writer_propagates_each_db_boundary_failure_and_fails_closed() {
    for boundary in [
        FaultBoundary::RequestStart,
        FaultBoundary::AttemptTerminal,
        FaultBoundary::RequestTerminal,
    ] {
        for fault in [FaultKind::Unavailable, FaultKind::CommitOutcomeUnknown] {
            let (writer, worker) =
                LifecycleWriter::start(4, Arc::new(FaultingStore { boundary, fault }))
                    .expect("writer");

            let error = exercise_boundary(&writer, boundary).await;
            assert_write_error(error, fault, boundary);
            assert!(!writer.health().is_healthy(), "{boundary:?} {fault:?}");
            assert!(matches!(
                writer.try_reserve_request(),
                Err(WriterAdmissionError::Unhealthy)
            ));

            drop(writer);
            worker.join().await.expect("worker join");
        }
    }
}

#[tokio::test]
async fn lifecycle_writer_saturates_before_accepting_partial_request_reservation() {
    let (writer, worker) =
        LifecycleWriter::start(2, Arc::new(CountingStore::default())).expect("writer");
    let request = writer.try_reserve_request().expect("request permits");

    assert!(matches!(
        writer.try_reserve_attempt(),
        Err(WriterAdmissionError::Full)
    ));

    drop(request);
    drop(writer);
    worker.join().await.expect("worker join");
}

#[tokio::test]
async fn lifecycle_writer_snapshot_tracks_reservations_and_returns_to_zero_after_drain() {
    let (writer, worker) =
        LifecycleWriter::start(3, Arc::new(CountingStore::default())).expect("writer");
    let request = writer.try_reserve_request().expect("request permits");
    let attempt = writer.try_reserve_attempt().expect("attempt permit");

    assert_eq!(writer.snapshot().current_outstanding, 3);
    assert_eq!(writer.snapshot().peak_outstanding, 3);

    drop(attempt);
    let (terminal, start_ack) = request.send_start(start_record());
    start_ack
        .await
        .expect("start ack channel")
        .expect("start persisted");
    terminal
        .send(final_record())
        .await
        .expect("terminal ack channel")
        .expect("terminal persisted");

    let snapshot = writer.snapshot();
    assert_eq!(snapshot.capacity, 3);
    assert_eq!(snapshot.current_outstanding, 0);
    assert_eq!(snapshot.peak_outstanding, 3);
    assert_eq!(snapshot.submitted, 2);
    assert_eq!(snapshot.completed, 2);
    assert_eq!(snapshot.failed, 0);
    assert_eq!(snapshot.cancelled_before_submission, 1);

    drop(writer);
    worker.join().await.expect("worker join");
}

#[tokio::test]
async fn lifecycle_writer_worker_panic_closes_new_admission_and_drops_ack() {
    let (writer, worker) = LifecycleWriter::start(4, Arc::new(PanicStore)).expect("writer");
    let request = writer.try_reserve_request().expect("request permits");
    let queued_attempt = writer.try_reserve_attempt().expect("queued attempt permit");
    let (terminal, ack) = request.send_start(start_record());
    let queued_ack = queued_attempt.send(attempt_record());

    assert!(
        ack.await.is_err(),
        "worker panic must drop the command ack instead of reporting success"
    );
    assert!(
        queued_ack.await.is_err(),
        "worker panic must also drop queued command ack channels"
    );
    assert!(matches!(
        writer.try_reserve_request(),
        Err(WriterAdmissionError::Closed)
    ));
    drop(terminal);
    let snapshot = writer.snapshot();
    assert_eq!(snapshot.current_outstanding, 0);
    assert_eq!(snapshot.failed, 2);

    drop(writer);
    assert!(
        worker.join().await.is_err(),
        "worker task should report panic"
    );
}

#[tokio::test]
async fn lifecycle_writer_ignores_dropped_ack_receiver_without_poisoning_worker() {
    let store = CountingStore::default();
    let calls = Arc::clone(&store.calls);
    let (writer, worker) = LifecycleWriter::start(4, Arc::new(store)).expect("writer");
    let request = writer.try_reserve_request().expect("request permits");
    let (terminal, ack) = request.send_start(start_record());
    drop(ack);

    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    assert!(writer.health().is_healthy());

    let finish_ack = terminal.send(final_record());
    assert!(
        finish_ack
            .await
            .expect("finish ack channel")
            .expect("finish ack")
            .finalized
    );
    assert_eq!(calls.load(Ordering::Relaxed), 2);

    drop(writer);
    worker.join().await.expect("worker join");
}

async fn exercise_boundary(
    writer: &LifecycleWriter,
    boundary: FaultBoundary,
) -> LifecycleWriteError {
    let request = writer.try_reserve_request().expect("request permits");
    let (terminal, start_ack) = request.send_start(start_record());
    let start_result = start_ack.await.expect("start ack channel");
    if boundary == FaultBoundary::RequestStart {
        drop(terminal);
        return start_result.expect_err("request-start should fail");
    }
    start_result.expect("request-start");

    if boundary == FaultBoundary::AttemptTerminal {
        let attempt = writer.try_reserve_attempt().expect("attempt permit");
        let attempt_ack = attempt.send(attempt_record());
        drop(terminal);
        return attempt_ack
            .await
            .expect("attempt ack channel")
            .expect_err("attempt-terminal should fail");
    }

    terminal
        .send(final_record())
        .await
        .expect("finish ack channel")
        .expect_err("request-terminal should fail")
}

fn assert_write_error(error: LifecycleWriteError, fault: FaultKind, boundary: FaultBoundary) {
    let text = match (fault, error) {
        (FaultKind::Unavailable, LifecycleWriteError::Unavailable(text)) => text,
        (FaultKind::CommitOutcomeUnknown, LifecycleWriteError::CommitOutcomeUnknown(text)) => text,
        (expected, other) => panic!("expected {expected:?} from {boundary:?}, got {other:?}"),
    };
    assert!(text.contains(boundary.label()), "{text}");
}

fn write_error(fault: FaultKind, boundary: &'static str) -> LifecycleWriteError {
    match fault {
        FaultKind::Unavailable => LifecycleWriteError::Unavailable(format!("{boundary} locked")),
        FaultKind::CommitOutcomeUnknown => {
            LifecycleWriteError::CommitOutcomeUnknown(format!("{boundary} outcome unknown"))
        }
    }
}

impl FaultBoundary {
    fn label(self) -> &'static str {
        match self {
            Self::RequestStart => "request-start",
            Self::AttemptTerminal => "attempt-terminal",
            Self::RequestTerminal => "request-terminal",
        }
    }
}

fn start_record() -> RequestStartRecord {
    RequestStartRecord { context: context() }
}

fn attempt_record() -> AttemptTerminalRecord {
    AttemptTerminalRecord {
        context: attempt_context(),
        terminal: AttemptTerminal::Failed(ClassifiedAttemptFailure {
            kind: super::lifecycle::attempt::AttemptFailureKind::Persistence,
            blame: FailureBlame::Persistence,
            retry: RetryDisposition::StopRequest,
            health: HealthEffect::Neutral,
            public_code: "injected".to_string(),
            sanitized_detail: Some("injected failure".to_string()),
        }),
        output_committed: false,
        terminal_at_ms: 3,
    }
}

fn final_record() -> FinalRequestRecord {
    let attempt_id = AttemptId::new("req-fault", 0);
    FinalRequestRecord {
        context: context(),
        terminal: RequestTerminalSnapshot {
            terminal: RequestTerminal::Completed(RequestCompletion {
                protocol_completed: true,
                attempt_id: Some(attempt_id.clone()),
            }),
            delivery: DeliveryTerminal::BodyCompleted,
        },
        selected_attempt_id: Some(attempt_id),
        attempt_count: 1,
        fallback_count: 0,
        annotations: RequestLogAnnotations::default(),
    }
}

fn context() -> RequestContextSnapshot {
    RequestContextSnapshot {
        request_id: "req-fault".to_string(),
        method: "POST".to_string(),
        local_path: "/v1/chat/completions".to_string(),
        endpoint: "/v1/chat/completions".to_string(),
        received_at_ms: 1,
    }
}

fn attempt_context() -> AttemptContext {
    AttemptContext {
        attempt_id: AttemptId::new("req-fault", 0),
        station_id: "station-fault".to_string(),
        station_key_id: "key-fault".to_string(),
        endpoint_revision: 1,
        started_at_ms: 2,
    }
}
