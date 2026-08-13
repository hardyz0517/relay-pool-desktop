#![allow(dead_code, unfulfilled_lint_expectations)]

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};

use futures_util::future::BoxFuture;
use tokio::sync::{Notify, Semaphore};

mod observability {
    pub(crate) mod correlation {
        #[derive(Clone)]
        pub(crate) struct CorrelationId;

        pub(crate) fn current_or_new() -> CorrelationId {
            CorrelationId
        }

        pub(crate) fn with_scope<T>(
            _scope: &'static str,
            _correlation_id: CorrelationId,
            operation: impl FnOnce() -> T,
        ) -> T {
            operation()
        }
    }
}

mod application {
    #[path = "../../src/application/request_lifecycle/mod.rs"]
    pub(crate) mod request_lifecycle;

    pub(crate) mod request_finalization {
        #[path = "../../../src/application/request_finalization/failure.rs"]
        pub(crate) mod failure;

        #[path = "../../../src/application/request_finalization/outcome.rs"]
        pub(crate) mod outcome;

        #[path = "../../../src/application/request_finalization/outcome_orchestrator.rs"]
        pub(crate) mod outcome_orchestrator;
    }
}

mod services {
    pub(crate) mod time {
        pub(crate) fn now_millis_for_services() -> u128 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_millis()
        }
    }

    pub(crate) mod proxy {
        #[path = "../../../src/services/proxy/lifecycle/mod.rs"]
        pub(crate) mod lifecycle;

        #[path = "../../../src/services/proxy/limits.rs"]
        pub(crate) mod limits;

        pub(crate) mod finalization {
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
        }

        pub(crate) mod response_body {
            pub(crate) use super::finalization::FinalizationOutcome;
        }

        #[path = "../../../src/services/proxy/attempt.rs"]
        pub(crate) mod attempt;
    }
}

use services::proxy::{
    attempt::{
        DownstreamRequestFinalizationLease, DualTerminalFinalizationLease,
        UpstreamAttemptFinalizationLease,
    },
    finalization::FinalizationOutcome,
    lifecycle::{
        attempt::{AttemptContext, AttemptTerminal, AttemptTerminalRecord},
        delivery::DeliveryTerminal,
        ports::{
            AttemptCommitAck, LifecycleWriteError, RequestCommitAck, RequestLifecycleStore,
            RequestStartAck,
        },
        request::{
            AttemptId, FinalRequestRecord, PendingFinalRequestRecord, RequestContextSnapshot,
            RequestLogAnnotations, RequestStartRecord, RequestTerminal,
        },
        writer::LifecycleWriter,
    },
    limits::RequestLease,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    Start(String),
    Attempt(String, u16),
    Request(String),
}

#[derive(Default)]
struct AckGatedStore {
    events: Arc<Mutex<Vec<Event>>>,
    attempts: Arc<Mutex<Vec<AttemptTerminalRecord>>>,
    requests: Arc<Mutex<Vec<FinalRequestRecord>>>,
    attempt_started: Arc<Notify>,
    attempt_release: Arc<Notify>,
    fail_attempt: bool,
}

impl AckGatedStore {
    fn with_attempt_failure() -> Self {
        Self {
            fail_attempt: true,
            ..Self::default()
        }
    }

    fn events(&self) -> Vec<Event> {
        self.events.lock().expect("events").clone()
    }

    fn attempt_calls(&self) -> usize {
        self.attempts.lock().expect("attempts").len()
    }

    fn request_calls(&self) -> usize {
        self.requests.lock().expect("requests").len()
    }

    fn last_request(&self) -> Option<FinalRequestRecord> {
        self.requests.lock().expect("requests").last().cloned()
    }

    async fn wait_for_attempt_calls(&self, expected: usize) {
        for _ in 0..1_000 {
            if self.attempt_calls() >= expected {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        assert_eq!(self.attempt_calls(), expected);
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

    fn release_attempt_ack(&self) {
        self.attempt_release.notify_waiters();
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
                .expect("events")
                .push(Event::Start(record.context.request_id));
            Ok(RequestStartAck { inserted: true })
        })
    }

    fn finish_attempt(
        &self,
        record: AttemptTerminalRecord,
    ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
        let events = Arc::clone(&self.events);
        let attempts = Arc::clone(&self.attempts);
        let started = Arc::clone(&self.attempt_started);
        let release = Arc::clone(&self.attempt_release);
        let fail_attempt = self.fail_attempt;
        Box::pin(async move {
            events.lock().expect("events").push(Event::Attempt(
                record.context.attempt_id.request_id.clone(),
                record.context.attempt_id.ordinal,
            ));
            attempts.lock().expect("attempts").push(record);
            started.notify_waiters();
            if fail_attempt {
                return Err(LifecycleWriteError::Unavailable(
                    "attempt ack unavailable".to_string(),
                ));
            }
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
        let requests = Arc::clone(&self.requests);
        Box::pin(async move {
            events
                .lock()
                .expect("events")
                .push(Event::Request(record.context.request_id.clone()));
            requests.lock().expect("requests").push(record);
            Ok(RequestCommitAck { finalized: true })
        })
    }
}

#[tokio::test]
async fn request_terminal_waits_for_selected_attempt_durable_ack() {
    let store = Arc::new(AckGatedStore::default());
    let (writer, worker) = LifecycleWriter::start(4, store.clone()).expect("writer");
    let active_requests = Arc::new(AtomicU32::new(0));

    let request = writer.try_reserve_request().expect("request reservation");
    let context = context("dual-terminal-lifecycle");
    let (request_terminal, start_ack) = request.send_start(RequestStartRecord {
        context: context.clone(),
    });
    start_ack
        .await
        .expect("start ack channel")
        .expect("start ack");

    let attempt_reservation = writer.try_reserve_attempt().expect("attempt reservation");
    let request_lease = request_lease(Arc::clone(&active_requests)).await;
    let finalizer = DualTerminalFinalizationLease::new(
        DownstreamRequestFinalizationLease::new(request_terminal, request_lease),
        Some(UpstreamAttemptFinalizationLease::new(
            attempt_reservation,
            attempt_context(&context),
        )),
        None,
    );
    let join = finalizer
        .finalize(
            pending_record(context),
            DeliveryTerminal::BodyCompleted,
            FinalizationOutcome::Completed,
            Some(AttemptTerminal::Succeeded),
            true,
        )
        .expect("finalization job");

    store.wait_for_attempt_calls(1).await;
    assert_eq!(store.request_calls(), 0);
    assert_eq!(
        writer.snapshot().submitted,
        2,
        "request terminal must not be submitted before selected attempt ack"
    );
    assert_eq!(active_requests.load(Ordering::SeqCst), 1);

    store.release_attempt_ack();
    store.wait_for_request_calls(1).await;
    join.await.expect("finalization job join");

    assert_eq!(
        store.events(),
        vec![
            Event::Start("dual-terminal-lifecycle".to_string()),
            Event::Attempt("dual-terminal-lifecycle".to_string(), 0),
            Event::Request("dual-terminal-lifecycle".to_string()),
        ]
    );
    let request = store.last_request().expect("final request");
    assert!(matches!(
        request.terminal.terminal,
        RequestTerminal::Completed(_)
    ));
    assert_eq!(active_requests.load(Ordering::SeqCst), 0);

    drop(writer);
    worker.join().await.expect("worker join");
}

#[tokio::test]
async fn attempt_ack_failure_records_interrupted_request_terminal_and_releases_request_lease() {
    let store = Arc::new(AckGatedStore::with_attempt_failure());
    let (writer, worker) = LifecycleWriter::start(4, store.clone()).expect("writer");
    let active_requests = Arc::new(AtomicU32::new(0));

    let request = writer.try_reserve_request().expect("request reservation");
    let context = context("dual-terminal-attempt-ack-failure");
    let (request_terminal, start_ack) = request.send_start(RequestStartRecord {
        context: context.clone(),
    });
    start_ack
        .await
        .expect("start ack channel")
        .expect("start ack");

    let attempt_reservation = writer.try_reserve_attempt().expect("attempt reservation");
    let request_lease = request_lease(Arc::clone(&active_requests)).await;
    let finalizer = DualTerminalFinalizationLease::new(
        DownstreamRequestFinalizationLease::new(request_terminal, request_lease),
        Some(UpstreamAttemptFinalizationLease::new(
            attempt_reservation,
            attempt_context(&context),
        )),
        None,
    );
    let join = finalizer
        .finalize(
            pending_record(context),
            DeliveryTerminal::BodyCompleted,
            FinalizationOutcome::Completed,
            Some(AttemptTerminal::Succeeded),
            true,
        )
        .expect("finalization job");

    join.await.expect("finalization job join");
    store.wait_for_attempt_calls(1).await;

    store.wait_for_request_calls(1).await;
    let request = store.last_request().expect("interrupted request terminal");
    assert!(matches!(
        request.terminal.terminal,
        RequestTerminal::Interrupted(_)
    ));
    assert_eq!(
        active_requests.load(Ordering::SeqCst),
        0,
        "request lease must be released even when attempt ack is unavailable"
    );
    assert_eq!(
        writer.snapshot().submitted,
        3,
        "attempt ack failure must record a fail-closed request terminal"
    );

    drop(writer);
    worker.join().await.expect("worker join");
}

#[test]
fn production_constructor_has_deleted_old_request_coupled_finalizer() {
    let source_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/services/proxy/response_body.rs");
    let source = std::fs::read_to_string(source_path).expect("response_body source");

    assert!(
        !source.contains("LifecycleFinalizationLease"),
        "Task 28 removes the request-coupled finalizer type instead of hiding it behind debug/test code"
    );
    assert!(
        !source.contains("FinalizationTarget::Lifecycle"),
        "Task 28 keeps only the dual-terminal finalization target"
    );
    assert!(
        !source.contains("pub(crate) fn lifecycle_finalizing_stream_with_idle_timeout"),
        "Task 28 removes the old request-coupled stream constructor"
    );

    let dual_constructor = function_body(
        &source,
        "pub(crate) fn dual_terminal_lifecycle_finalizing_stream_with_idle_timeout_and_diagnostic_memory",
    );
    assert!(
        dual_constructor.contains("dual_terminal_finalizing_stream("),
        "the production constructor must delegate into the shared dual-terminal finalizer"
    );
    let delegate = function_body(&source, "fn dual_terminal_finalizing_stream");
    assert!(delegate.contains("FinalizationTarget::DualTerminal"));
    assert!(
        !source.contains(
            "#[cfg(test)]\r\npub(crate) fn dual_terminal_lifecycle_finalizing_stream_with_idle_timeout_and_diagnostic_memory"
        ) && !source.contains(
            "#[cfg(test)]\npub(crate) fn dual_terminal_lifecycle_finalizing_stream_with_idle_timeout_and_diagnostic_memory"
        ),
        "dual-terminal path must be a real composition option, not a cfg(test)-hidden adapter"
    );
}

async fn request_lease(active_requests: Arc<AtomicU32>) -> RequestLease {
    let permit = Arc::new(Semaphore::new(1))
        .acquire_owned()
        .await
        .expect("request permit");
    RequestLease::new(permit, active_requests)
}

fn context(request_id: &str) -> RequestContextSnapshot {
    RequestContextSnapshot {
        request_id: request_id.to_string(),
        method: "POST".to_string(),
        local_path: "/v1/chat/completions".to_string(),
        endpoint: "/v1/chat/completions".to_string(),
        received_at_ms: 1_000,
    }
}

fn attempt_context(context: &RequestContextSnapshot) -> AttemptContext {
    AttemptContext {
        attempt_id: AttemptId::new(context.request_id.clone(), 0),
        station_id: "station-test".to_string(),
        station_key_id: "key-test".to_string(),
        endpoint_revision: 1,
        credential_revision: 1,
        account_revision: 1,
        group_binding_id: None,
        group_revision: None,
        resolved_upstream_model: None,
        model_alias_revision: 1,
        started_at_ms: context.received_at_ms,
    }
}

fn pending_record(context: RequestContextSnapshot) -> PendingFinalRequestRecord {
    PendingFinalRequestRecord::new(
        context.clone(),
        Some(AttemptId::new(context.request_id, 0)),
        1,
        0,
        RequestLogAnnotations::default(),
    )
}

fn function_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source.find(signature).expect("function signature");
    let rest = &source[start..];
    let body_start = rest.find('{').expect("function body start");
    let mut depth = 0usize;
    for (offset, ch) in rest[body_start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &rest[..body_start + offset + ch.len_utf8()];
                }
            }
            _ => {}
        }
    }
    panic!("function body end")
}
