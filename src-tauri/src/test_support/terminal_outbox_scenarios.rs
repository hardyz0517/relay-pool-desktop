use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::persistence::{
    error::PersistenceError,
    runtime::{PersistenceHandle, PersistenceRuntime},
    stores::{
        request_log_write::{
            RequestLogAnnotationsWrite, RequestRoutingOutcomeSummaryWrite, RequestTerminalWrite,
        },
        request_terminal_outbox::RequestTerminalOutboxStore,
    },
};

pub async fn expired_lease_replays_without_payload_changes() {
    let (runtime, _root) = runtime().await;
    let handle = runtime.handle();
    seed_request(&handle, "req-outbox-replay").await;
    let expected = record("req-outbox-replay", "upstream_unavailable");
    let mut session = handle.begin_write().await.expect("write session");
    RequestTerminalOutboxStore
        .enqueue(session.connection(), &expected, 10)
        .await
        .expect("enqueue");
    let (first, _) = RequestTerminalOutboxStore
        .claim_batch(session.connection(), "crashed-owner", 20, 10, 16)
        .await
        .expect("first claim");
    assert_eq!(first, vec![expected.clone()]);
    session.commit().await.expect("commit claim");

    let mut session = handle.begin_write().await.expect("write session");
    let (before_expiry, _) = RequestTerminalOutboxStore
        .claim_batch(session.connection(), "new-owner", 29, 10, 16)
        .await
        .expect("before expiry");
    assert!(before_expiry.is_empty());
    let (replayed, _) = RequestTerminalOutboxStore
        .claim_batch(session.connection(), "new-owner", 30, 10, 16)
        .await
        .expect("reclaimed claim");
    assert_eq!(replayed, vec![expected]);
}

pub async fn collision_and_digest_tamper_fail_closed() {
    let (runtime, _root) = runtime().await;
    let handle = runtime.handle();
    seed_request(&handle, "req-outbox-collision").await;
    let mut session = handle.begin_write().await.expect("write session");
    RequestTerminalOutboxStore
        .enqueue(
            session.connection(),
            &record("req-outbox-collision", "server_error"),
            10,
        )
        .await
        .expect("enqueue");
    let collision = RequestTerminalOutboxStore
        .enqueue(
            session.connection(),
            &record("req-outbox-collision", "upstream_unavailable"),
            11,
        )
        .await
        .expect_err("different terminal must fail");
    assert!(matches!(
        collision,
        PersistenceError::InvariantViolation(ref detail) if detail == "request terminal outbox payload collision"
    ));

    sqlx::query("UPDATE request_terminal_outbox SET payload_sha256 = ?1 WHERE request_id = ?2")
        .bind("0".repeat(64))
        .bind("req-outbox-collision")
        .execute(session.connection())
        .await
        .expect("tamper fixture");
    let digest = RequestTerminalOutboxStore
        .claim_batch(session.connection(), "owner", 20, 10, 16)
        .await
        .expect_err("tampered digest must fail");
    assert!(matches!(
        digest,
        PersistenceError::InvariantViolation(ref detail) if detail == "request terminal outbox payload digest mismatch"
    ));
}

async fn runtime() -> (PersistenceRuntime, TempRoot) {
    let root = TempRoot::new("relay-terminal-outbox");
    let path = root.path.join("relay-pool.sqlite3");
    let runtime = PersistenceRuntime::initialize_new(&path)
        .await
        .expect("initialize runtime");
    (runtime, root)
}

async fn seed_request(runtime: &PersistenceHandle, request_id: &str) {
    let mut session = runtime.begin_write().await.expect("write session");
    sqlx::query(
        "INSERT INTO request_logs (
            id, request_id, started_at, method, path, endpoint, status, lifecycle_status, created_at
         ) VALUES (?1, ?1, '1', 'POST', '/v1/responses', 'responses', 'in_progress', 'admitted', '1')",
    )
    .bind(request_id)
    .execute(session.connection())
    .await
    .expect("request");
    session.commit().await.expect("commit request");
}

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{now_nanos}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp root");
        Self { path }
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn record(request_id: &str, terminal_code: &str) -> RequestTerminalWrite {
    RequestTerminalWrite {
        request_id: request_id.to_string(),
        received_at_ms: 1,
        status: "failed".to_string(),
        lifecycle_status: "failed".to_string(),
        usage_status: "missing_usage".to_string(),
        terminal_kind: "failed".to_string(),
        terminal_code: Some(terminal_code.to_string()),
        terminal_detail: Some("sanitized_failure".to_string()),
        protocol_completed: false,
        delivery_terminal: "NotStarted".to_string(),
        selected_attempt_ordinal: Some(0),
        attempt_count: 1,
        fallback_count: 0,
        terminal_at_ms: 2,
        routing_outcome: RequestRoutingOutcomeSummaryWrite {
            terminal_kind: "failed".to_string(),
            terminal_code: terminal_code.to_string(),
            classification: "generic".to_string(),
            confidence: "not_applicable".to_string(),
            evidence_source: "none".to_string(),
            request_accepted: "unknown".to_string(),
            send_phase: "unknown".to_string(),
            replay_disposition: "stopped_uncertain".to_string(),
            billing_state: "possibly_billed".to_string(),
            retry_disposition: "none".to_string(),
            effect_summary: "neutral".to_string(),
            failure_domain_commitment_version: None,
            failure_domain_commitment_digest: None,
            attempt_count: 1,
            fallback_count: 0,
            terminal_at_ms: 2,
        },
        annotations: RequestLogAnnotationsWrite::default(),
    }
}
