mod models {
    pub(crate) mod change_events {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/change_events.rs"
        ));
    }
}

mod persistence {
    pub(crate) mod error {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/error.rs"
        ));
    }
    pub(crate) mod write_coordinator {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/write_coordinator.rs"
        ));
    }
    pub(crate) mod write_session {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/write_session.rs"
        ));
    }
    pub(crate) mod read_session {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/read_session.rs"
        ));
    }
    pub(crate) mod backup {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/backup.rs"
        ));
    }
    pub(crate) mod schema_compatibility {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/schema_compatibility.rs"
        ));
    }
    pub(crate) mod health_check {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/health_check.rs"
        ));
    }
    pub(crate) mod migrations {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/migrations.rs"
        ));
    }
    pub(crate) mod runtime {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/runtime.rs"
        ));
    }
    pub(crate) mod stores {
        pub(crate) mod collector_store {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/persistence/stores/collector_store.rs"
            ));
        }
        pub(crate) mod change_store {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/persistence/stores/change_store.rs"
            ));
        }
    }
}

mod application {
    pub(crate) mod clock {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/clock.rs"
        ));
    }
    pub(crate) mod error {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/error.rs"
        ));
    }
    pub(crate) mod ids {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/ids.rs"
        ));
    }
    pub(crate) mod collectors {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/collectors.rs"
        ));
    }
}

use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use application::{
    clock::Clock,
    collectors::{
        CanonicalCollectorFacts, CanonicalGroupFact, CanonicalModelFact, CanonicalRateFact,
        CollectorApplyRequest, CollectorService,
    },
    error::ApplicationError,
    ids::IdGenerator,
};
use chrono::{TimeZone, Utc};
use persistence::{runtime::PersistenceRuntime, schema_compatibility::BinaryCompatibility};
use semver::Version;
use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, Connection, Row};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

#[tokio::test]
async fn collector_apply_rolls_back_snapshot_facts_events_and_run_on_failure() {
    let fixture = Fixture::create("rollback").await;
    fixture
        .execute(
            "CREATE TRIGGER fail_collector_finish BEFORE UPDATE OF status ON collector_runs
             WHEN NEW.status != 'running'
             BEGIN SELECT RAISE(ABORT, 'injected collector finish failure'); END",
        )
        .await;
    let service = fixture.service().await;

    let error = service
        .apply_result(full_result("run-rollback", 1, "sub2api"))
        .await
        .unwrap_err();

    assert!(matches!(error, ApplicationError::Internal));
    for table in [
        "collector_runs",
        "collector_snapshots",
        "station_group_bindings",
        "group_rate_records",
        "collector_model_facts",
        "change_events",
    ] {
        assert_eq!(fixture.count(table).await, 0, "{table} must roll back");
    }
}

#[tokio::test]
async fn collector_run_and_change_events_are_idempotent_and_preserve_dismissed_state() {
    let fixture = Fixture::create("idempotent").await;
    let service = fixture.service().await;
    let request = full_result("run-idempotent", 1, "sub2api");

    let first = service
        .apply_result(request.clone())
        .await
        .expect("first apply");
    let event_id: String = fixture
        .scalar("SELECT id FROM change_events WHERE event_type = 'group_added'")
        .await;
    fixture
        .execute(&format!(
            "UPDATE change_events SET status = 'dismissed' WHERE id = '{}'",
            event_id.replace('\'', "''")
        ))
        .await;
    let second = service
        .apply_result(request)
        .await
        .expect("idempotent retry");

    assert!(first.inserted);
    assert!(!second.inserted);
    assert_eq!(first.run_id, second.run_id);
    assert_eq!(fixture.count("collector_runs").await, 1);
    assert_eq!(fixture.count("collector_snapshots").await, 1);
    assert_eq!(fixture.count("group_rate_records").await, 1);
    assert_eq!(
        fixture
            .scalar::<String>("SELECT status FROM change_events WHERE id = (SELECT id FROM change_events WHERE event_type = 'group_added' LIMIT 1)")
            .await,
        "dismissed"
    );
}

#[tokio::test]
async fn stale_revision_and_unsupported_model_events_have_no_side_effects() {
    let fixture = Fixture::create("revision").await;
    let service = fixture.service().await;

    let stale = service
        .apply_result(full_result("run-stale", 2, "sub2api"))
        .await
        .unwrap_err();
    assert!(matches!(stale, ApplicationError::StaleRevision));
    assert_eq!(fixture.count("collector_runs").await, 0);

    service
        .apply_result(full_result("run-custom", 1, "custom"))
        .await
        .expect("custom facts persist");
    assert_eq!(fixture.count("collector_model_facts").await, 1);
    assert_eq!(
        fixture
            .scalar::<i64>(
                "SELECT COUNT(*) FROM change_events WHERE event_type IN ('model_added', 'model_removed')",
            )
            .await,
        0
    );
}

fn full_result(run_key: &str, endpoint_revision: i64, adapter: &str) -> CollectorApplyRequest {
    CollectorApplyRequest {
        run_key: run_key.to_string(),
        station_id: "station-1".to_string(),
        endpoint_revision,
        parent_run_id: None,
        adapter: adapter.to_string(),
        task_type: "full".to_string(),
        status: "success".to_string(),
        facts: CanonicalCollectorFacts {
            groups: vec![CanonicalGroupFact {
                station_id: "station-1".to_string(),
                group_id: Some("group-remote".to_string()),
                group_key_hash: "group-hash".to_string(),
                group_name: "default".to_string(),
                source: "sub2api_groups_available".to_string(),
                confidence: 0.9,
                inferred_group_category: Some("general".to_string()),
                raw_json_redacted: None,
            }],
            rates: vec![CanonicalRateFact {
                station_id: "station-1".to_string(),
                station_key_id: None,
                group_id: Some("group-remote".to_string()),
                group_key_hash: "group-hash".to_string(),
                group_name: "default".to_string(),
                default_rate_multiplier: Some(1.0),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(1.0),
                inferred_group_category: Some("general".to_string()),
                source: "sub2api_groups_rates".to_string(),
                confidence: 0.9,
                checked_at: Some("1700000000000".to_string()),
                raw_json_redacted: None,
            }],
            models: vec![CanonicalModelFact {
                station_id: "station-1".to_string(),
                model: "gpt-test".to_string(),
                available: true,
                source: "models_api".to_string(),
                confidence: 1.0,
            }],
            ..CanonicalCollectorFacts::default()
        },
        summary_json: serde_json::json!({ "endpointResults": [{ "ok": true }] }),
        normalized_json: serde_json::json!({ "models": ["gpt-test"] }),
        raw_json_redacted: None,
        error_code: None,
        error_message: None,
        endpoint_count: 1,
        success_count: 1,
        failure_count: 0,
        manual_action_required: false,
        next_due_at: Some("1700000060000".to_string()),
    }
}

struct FixedClock;

impl Clock for FixedClock {
    fn now_utc(&self) -> chrono::DateTime<Utc> {
        Utc.timestamp_millis_opt(1_700_000_000_000)
            .single()
            .expect("fixed time")
    }
}

struct SequentialIds(AtomicU64);

impl IdGenerator for SequentialIds {
    fn next_id(&self) -> String {
        format!("test-id-{}", self.0.fetch_add(1, Ordering::Relaxed))
    }
}

struct Fixture {
    path: PathBuf,
}

impl Fixture {
    async fn create(name: &str) -> Self {
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("relay-pool-collector-{name}-{id}"));
        if root.exists() {
            std::fs::remove_dir_all(&root).expect("remove stale fixture directory");
        }
        std::fs::create_dir_all(&root).expect("fixture directory");
        let path = root.join("relay-pool-v2.sqlite3");
        let mut connection = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .connect()
            .await
            .expect("connect fixture");
        persistence::migrations::migrator()
            .run(&mut connection)
            .await
            .expect("migrate fixture");
        sqlx::query(
            "INSERT INTO stations (
                id, name, station_type, website_url, api_base_url,
                endpoint_revision, created_at, updated_at
             ) VALUES ('station-1', 'Station', 'sub2api', 'https://example.test',
                       'https://example.test/v1', 1, '1', '1')",
        )
        .execute(&mut connection)
        .await
        .expect("seed station");
        connection.close().await.expect("close fixture");
        Self { path }
    }

    async fn service(&self) -> CollectorService {
        let runtime = PersistenceRuntime::open(&self.path, binary_7())
            .await
            .expect("open runtime");
        CollectorService::new(
            runtime.handle(),
            Arc::new(FixedClock),
            Arc::new(SequentialIds(AtomicU64::new(1))),
        )
    }

    async fn execute(&self, sql: &str) {
        let mut connection = SqliteConnectOptions::new()
            .filename(&self.path)
            .connect()
            .await
            .expect("connect fixture");
        sqlx::query(sql)
            .execute(&mut connection)
            .await
            .expect("execute fixture SQL");
        connection.close().await.expect("close fixture");
    }

    async fn count(&self, table: &str) -> i64 {
        self.scalar(&format!("SELECT COUNT(*) FROM {table}")).await
    }

    async fn scalar<T>(&self, sql: &str) -> T
    where
        T: for<'r> sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Send + Unpin,
    {
        let mut connection = SqliteConnectOptions::new()
            .filename(&self.path)
            .connect()
            .await
            .expect("connect fixture");
        let row = sqlx::query(sql)
            .fetch_one(&mut connection)
            .await
            .expect("query scalar");
        let value = row.get(0);
        connection.close().await.expect("close fixture");
        value
    }
}

fn binary_7() -> BinaryCompatibility {
    BinaryCompatibility {
        app_version: Version::new(0, 3, 1),
        database_generation: 2,
        readable_schema: 1..=7,
        writable_schema: BTreeSet::from([7]),
    }
}
