#[path = "../src/application/error_rate_protection.rs"]
pub mod application_error_rate_protection;
#[path = "../src/application/health_protection.rs"]
pub mod application_health_protection;
#[path = "../src/application/spendability/mod.rs"]
pub mod application_spendability;
#[path = "../src/persistence/stores/domain_revision_store.rs"]
pub mod domain_revision_store;
#[path = "../src/persistence/stores/health_observation_store.rs"]
pub mod health_observation_store;
#[path = "../src/application/health_transitions.rs"]
pub mod health_transitions;
#[path = "../src/models/health.rs"]
pub mod model_health;
#[path = "../src/models/monitoring/mod.rs"]
pub mod model_monitoring;
#[path = "../src/models/pricing.rs"]
pub mod model_pricing;
#[path = "../src/models/routing_generation.rs"]
pub mod model_routing_generation;
#[path = "../src/models/routing_policy.rs"]
pub mod model_routing_policy;
#[path = "../src/persistence/stores/monitoring/executions.rs"]
pub mod monitoring_executions;
#[path = "../src/persistence/stores/monitoring/retention.rs"]
pub mod monitoring_retention;
#[path = "../src/application/observation_ingestion.rs"]
pub mod observation_ingestion;
#[path = "../src/persistence/error.rs"]
pub mod persistence_error;
#[path = "../src/persistence/stores/routing_error_rate_history_store.rs"]
pub mod routing_error_rate_history_store;
#[path = "../src/persistence/stores/routing_health_verdict_store.rs"]
pub mod routing_health_verdict_store;
#[path = "../src/models/routing_observation.rs"]
pub mod routing_observation;
#[path = "../src/persistence/stores/routing_observation_store.rs"]
pub mod routing_observation_store;
pub mod routing_generation_store {
    use sqlx::SqliteConnection;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum RoutingGenerationEligibility {
        Active,
        Next,
    }

    impl RoutingGenerationEligibility {
        pub(crate) const fn as_str(self) -> &'static str {
            match self {
                Self::Active => "active",
                Self::Next => "next",
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct RoutingIngestionFence {
        pub(crate) eligibility: RoutingGenerationEligibility,
    }

    #[derive(Debug, Clone, Copy, Default)]
    pub(crate) struct RoutingGenerationStore;

    impl RoutingGenerationStore {
        pub(crate) async fn load_ingestion_fence(
            &self,
            connection: &mut SqliteConnection,
        ) -> Result<RoutingIngestionFence, crate::persistence_error::PersistenceError> {
            let status = sqlx::query_scalar::<_, String>(
                "SELECT status FROM routing_runtime_cutover_marker WHERE singleton_key = 1",
            )
            .fetch_optional(&mut *connection)
            .await?;
            Ok(RoutingIngestionFence {
                eligibility: if status.as_deref() == Some("v3_active") {
                    RoutingGenerationEligibility::Active
                } else {
                    RoutingGenerationEligibility::Next
                },
            })
        }
    }
}
#[path = "../src/persistence/stores/routing_policy_store.rs"]
pub mod routing_policy_store;
mod persistence_runtime {
    use std::path::{Path, PathBuf};

    use sqlx::{sqlite::SqliteConnectOptions, Connection, Executor, SqliteConnection};

    use super::persistence::WriteSession;

    static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/persistence/migrations");

    #[derive(Clone, Debug)]
    pub(crate) struct PersistenceRuntime {
        handle: PersistenceHandle,
    }

    #[derive(Clone, Debug)]
    pub(crate) struct PersistenceHandle {
        path: PathBuf,
    }

    pub(crate) struct ReadSession {
        connection: SqliteConnection,
    }

    impl ReadSession {
        pub(crate) fn connection(&mut self) -> &mut SqliteConnection {
            &mut self.connection
        }
    }

    impl PersistenceRuntime {
        pub(crate) async fn initialize_new(path: &Path) -> Result<Self, sqlx::Error> {
            let mut connection = SqliteConnection::connect_with(
                &SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true),
            )
            .await?;
            MIGRATOR.run(&mut connection).await?;
            Ok(Self {
                handle: PersistenceHandle {
                    path: path.to_path_buf(),
                },
            })
        }

        pub(crate) async fn open_current(path: &Path) -> Result<Self, sqlx::Error> {
            let _ = SqliteConnection::connect_with(
                &SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(false),
            )
            .await?;
            Ok(Self {
                handle: PersistenceHandle {
                    path: path.to_path_buf(),
                },
            })
        }

        pub(crate) async fn begin_write(&self) -> Result<WriteSession, sqlx::Error> {
            let mut connection = SqliteConnection::connect_with(
                &SqliteConnectOptions::new()
                    .filename(&self.handle.path)
                    .create_if_missing(false),
            )
            .await?;
            connection.execute("BEGIN IMMEDIATE").await?;
            Ok(WriteSession::from_owned(connection))
        }

        pub(crate) async fn begin_read(&self) -> Result<ReadSession, sqlx::Error> {
            self.handle.begin_read().await
        }

        pub(crate) fn handle(&self) -> PersistenceHandle {
            self.handle.clone()
        }

        pub(crate) async fn close(self) -> Result<(), sqlx::Error> {
            Ok(())
        }
    }

    impl PersistenceHandle {
        pub(crate) async fn begin_read(&self) -> Result<ReadSession, sqlx::Error> {
            Ok(ReadSession {
                connection: SqliteConnection::connect_with(
                    &SqliteConnectOptions::new()
                        .filename(&self.path)
                        .create_if_missing(false),
                )
                .await?,
            })
        }
    }
}

mod models {
    pub(crate) mod health {
        pub(crate) use crate::model_health::*;
    }

    pub(crate) mod monitoring {
        pub(crate) use crate::model_monitoring::*;
    }

    pub(crate) mod routing_observation {
        pub(crate) use crate::routing_observation::*;
    }
    pub(crate) mod pricing {
        pub(crate) use crate::model_pricing::*;
    }
    pub(crate) mod routing_policy {
        pub(crate) use crate::model_routing_policy::*;
    }
    pub(crate) mod routing_generation {
        pub(crate) use crate::model_routing_generation::*;
    }
}

mod persistence {
    pub(crate) struct WriteSession {
        connection: *mut sqlx::SqliteConnection,
        owned_connection: Option<Box<sqlx::SqliteConnection>>,
    }

    impl WriteSession {
        pub(crate) fn new(connection: &mut sqlx::SqliteConnection) -> Self {
            Self {
                connection,
                owned_connection: None,
            }
        }

        pub(crate) fn from_owned(connection: sqlx::SqliteConnection) -> Self {
            let mut owned_connection = Box::new(connection);
            let connection = &mut *owned_connection as *mut sqlx::SqliteConnection;
            Self {
                connection,
                owned_connection: Some(owned_connection),
            }
        }

        pub(crate) fn connection(&mut self) -> &mut sqlx::SqliteConnection {
            // SAFETY: borrowed sessions cannot outlive their caller's mutable
            // borrow, and owned sessions keep the boxed connection alive.
            unsafe { &mut *self.connection }
        }

        pub(crate) async fn commit(
            mut self,
        ) -> Result<(), crate::persistence_error::PersistenceError> {
            if let Some(connection) = &mut self.owned_connection {
                sqlx::Executor::execute(&mut **connection, "COMMIT").await?;
            }
            Ok(())
        }
    }

    pub(crate) mod error {
        pub(crate) use crate::persistence_error::*;
    }
    pub(crate) mod migrations {
        pub(crate) fn migrator() -> &'static sqlx::migrate::Migrator {
            static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/persistence/migrations");
            &MIGRATOR
        }
    }
    pub(crate) mod runtime {
        pub(crate) use crate::persistence_runtime::*;
    }

    pub(crate) mod stores {
        pub(crate) mod health_observation_store {
            pub(crate) use crate::health_observation_store::*;
        }

        pub(crate) mod monitoring {
            pub(crate) mod executions {
                pub(crate) use crate::monitoring_executions::*;
            }
            pub(crate) mod retention {
                pub(crate) use crate::monitoring_retention::*;
            }
        }

        pub(crate) mod routing_observation_store {
            pub(crate) use crate::routing_observation_store::*;
        }
        pub(crate) mod routing_generation_store {
            use sqlx::SqliteConnection;

            use crate::models::routing_generation::{
                RoutingCutoverMode, RoutingGenerationMarker, RoutingGenerationRegistrySnapshot,
            };

            #[derive(Debug, Clone, Copy, Default)]
            pub(crate) struct RoutingGenerationStore;

            impl RoutingGenerationStore {
                pub(crate) async fn load_registry_snapshot(
                    &self,
                    _connection: &mut SqliteConnection,
                ) -> Result<
                    RoutingGenerationRegistrySnapshot,
                    crate::persistence_error::PersistenceError,
                > {
                    Ok(RoutingGenerationRegistrySnapshot {
                        marker: RoutingGenerationMarker {
                            mode: RoutingCutoverMode::PreCutover,
                            active_runtime_generation_id: None,
                            fenced_runtime_generation_id: None,
                            fence_revision: 0,
                            updated_at_ms: 0,
                        },
                        active: None,
                        fencing: None,
                    })
                }
            }
        }
        pub(crate) mod routing_policy_v3_stage_upgrade {
            use sqlx::SqliteConnection;

            pub(crate) async fn load_effective_active_in(
                connection: &mut SqliteConnection,
            ) -> Result<
                Option<crate::persistence::stores::routing_policy_store::StoredRoutingPolicy>,
                crate::persistence_error::PersistenceError,
            > {
                crate::persistence::stores::routing_policy_store::RoutingPolicyStore
                    .load(connection)
                    .await
            }
        }
        pub(crate) mod routing_health_verdict_store {
            pub(crate) use crate::routing_health_verdict_store::*;
        }
        pub(crate) mod routing_policy_store {
            pub(crate) use crate::routing_policy_store::*;
        }
        pub(crate) mod routing_error_rate_history_store {
            pub(crate) use crate::routing_error_rate_history_store::*;
        }
        pub(crate) mod domain_revision_store {
            pub(crate) use crate::domain_revision_store::*;
        }
    }
}

mod application {
    // The production observation writer consults the v3 policy staging owner
    // to decide whether the legacy error-rate bridge is enabled.  This test
    // assembles the writer from source files instead of the full application
    // module, so provide the same read-only pre-cutover behavior here.
    pub(crate) mod routing_policy_v3_stage_upgrade {
        use sqlx::SqliteConnection;

        pub(crate) async fn load_effective_active_in(
            connection: &mut SqliteConnection,
        ) -> Result<
            Option<crate::persistence::stores::routing_policy_store::StoredRoutingPolicy>,
            crate::persistence_error::PersistenceError,
        > {
            crate::persistence::stores::routing_policy_store::RoutingPolicyStore
                .load(connection)
                .await
        }
    }

    pub(crate) mod health_protection {
        pub(crate) use crate::application_health_protection::*;
    }
    pub(crate) mod error_rate_protection {
        pub(crate) use crate::application_error_rate_protection::*;
    }
    pub(crate) mod spendability {
        pub(crate) use crate::application_spendability::*;
    }

    pub(crate) mod observation_ingestion {
        pub(crate) use crate::observation_ingestion::*;
    }

    pub(crate) mod health_transitions {
        pub(crate) use crate::health_transitions::*;
    }

    pub(crate) mod monitoring {
        pub mod commands {
            #[derive(Debug, Clone, PartialEq, Eq)]
            pub(crate) struct MonitorExecutionReceipt {
                pub(crate) execution_id: String,
                pub(crate) reused_existing: bool,
            }
        }
        pub mod planner {
            use crate::model_monitoring::{
                ClientProfileRef, DefinitionRevision, HealthPolicy, ProtocolKind, RetryPolicy,
                RiskPolicy, SchedulePolicy, TriggerKind,
            };

            #[derive(Debug, Clone, PartialEq, Eq)]
            pub(crate) struct ProbePlan {
                pub(crate) monitor_id: String,
                pub(crate) revision: DefinitionRevision,
                pub(crate) trigger_kind: TriggerKind,
                pub(crate) config_snapshot_hash: String,
                pub(crate) target_plans: Vec<ProbeTargetPlan>,
                pub(crate) model_plans: Vec<ProbeModelPlan>,
                pub(crate) schedule_policy: SchedulePolicy,
                pub(crate) retry_policy: RetryPolicy,
                pub(crate) risk_policy: RiskPolicy,
                pub(crate) health_policy: HealthPolicy,
            }

            #[derive(Debug, Clone, PartialEq, Eq)]
            pub(crate) struct ProbeTargetPlan {
                pub(crate) station_id: String,
                pub(crate) station_key_id: String,
                pub(crate) station_key_lifecycle_revision: u64,
                pub(crate) endpoint_revision: i64,
                pub(crate) protocol_kind: Option<ProtocolKind>,
                pub(crate) skip_failure_kind: Option<crate::model_monitoring::FailureKind>,
                pub(crate) client_profile: ClientProfileRef,
                pub(crate) request_profile_hash: Option<String>,
            }

            #[derive(Debug, Clone, PartialEq, Eq)]
            pub(crate) struct ProbeModelPlan {
                pub(crate) model: String,
                pub(crate) role: ProbeModelRole,
            }

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum ProbeModelRole {
                Primary,
                Fallback { index: u8 },
            }
        }
        #[path = "../../../src/application/monitoring/recorder.rs"]
        pub mod recorder;
        #[path = "../../../src/application/monitoring/write_path.rs"]
        pub mod write_path;
    }
}

use application::monitoring::{
    planner::{ProbeModelPlan, ProbeModelRole, ProbePlan, ProbeTargetPlan},
    recorder::{
        BufferedExecution, RecordedAttempt, RecordedExecutionSummary, RecordedTargetResult,
    },
    write_path::MonitoringExecutionCommitter,
};
use model_monitoring::{
    ClientProfileId, ClientProfileRef, DefinitionRevision, FailureKind, HealthPolicy,
    HealthWritebackMode, ProbeOutcome, ProtocolKind, RetryPolicy, RiskPolicy, SchedulePolicy,
    SemanticConfidence, TriggerKind,
};
use persistence::error::PersistenceError;
use sqlx::{Connection, Row, SqliteConnection};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/persistence/migrations");

async fn commit_execution(
    committer: &MonitoringExecutionCommitter,
    connection: &mut SqliteConnection,
    execution: &BufferedExecution,
) -> Result<monitoring_executions::ExecutionSummaryRow, PersistenceError> {
    let mut write = persistence::WriteSession::new(connection);
    committer.commit(&mut write, execution).await
}

#[tokio::test]
async fn orchestrator_buffer_commits_v2_facts_without_legacy_run_writes_and_replays_once() {
    let mut connection = ready_connection().await;
    let committer = MonitoringExecutionCommitter::new();
    let execution = buffered_execution("execution-1", TriggerKind::Manual, ProbeOutcome::Available);

    commit_execution(&committer, &mut connection, &execution)
        .await
        .expect("commit execution");
    commit_execution(&committer, &mut connection, &execution)
        .await
        .expect("replay execution");

    assert_eq!(
        count(&mut connection, "channel_monitor_executions").await,
        1
    );
    assert_eq!(count(&mut connection, "channel_monitor_attempts").await, 1);
    let attempt = sqlx::query(
        "SELECT http_status, response_model, content_extracted, validation_passed, output_bytes \
         FROM channel_monitor_attempts WHERE execution_id = 'execution-1'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("attempt diagnostics");
    assert_eq!(attempt.get::<Option<i64>, _>("http_status"), Some(200));
    assert_eq!(
        attempt
            .get::<Option<String>, _>("response_model")
            .as_deref(),
        Some("upstream-model")
    );
    assert_eq!(attempt.get::<i64, _>("content_extracted"), 1);
    assert_eq!(attempt.get::<i64, _>("validation_passed"), 1);
    assert_eq!(attempt.get::<i64, _>("output_bytes"), 12);
    assert_eq!(
        count(&mut connection, "channel_monitor_target_results").await,
        1
    );
    assert_eq!(
        count(&mut connection, "station_key_health_observations").await,
        1
    );
    let routing_observation = sqlx::query(
        "SELECT station_key_lifecycle_revision, traffic_equivalence, comparability_key
         FROM routing_observations WHERE source = 'active_probe'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("routing observation identity");
    assert_eq!(
        routing_observation.get::<Option<i64>, _>("station_key_lifecycle_revision"),
        Some(1)
    );
    assert_eq!(
        routing_observation.get::<String, _>("traffic_equivalence"),
        "same_model_shape"
    );
    let comparability_key = routing_observation
        .get::<Option<String>, _>("comparability_key")
        .expect("comparable probe key");
    assert!(comparability_key.starts_with("cmp:v1:"));
    assert_eq!(comparability_key.len(), 71);
    assert_eq!(
        count(&mut connection, "channel_monitor_rollup_dirty_ranges").await,
        1
    );
    assert!(!table_exists(&mut connection, "channel_monitor_runs").await);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT next_due_at_ms FROM channel_monitors WHERE id = 'monitor-1'"
        )
        .fetch_one(&mut connection)
        .await
        .expect("next due"),
        999
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT success_count FROM routing_health_snapshot WHERE station_key_id = 'key-1'"
        )
        .fetch_one(&mut connection)
        .await
        .expect("health"),
        1
    );
}

#[tokio::test]
async fn scheduled_execution_advances_due_but_manual_execution_does_not() {
    let mut connection = ready_connection().await;
    let committer = MonitoringExecutionCommitter::new();

    commit_execution(
        &committer,
        &mut connection,
        &buffered_execution(
            "manual-execution",
            TriggerKind::Manual,
            ProbeOutcome::Available,
        ),
    )
    .await
    .expect("manual commit");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT next_due_at_ms FROM channel_monitors WHERE id = 'monitor-1'"
        )
        .fetch_one(&mut connection)
        .await
        .expect("manual next due"),
        999
    );

    commit_execution(
        &committer,
        &mut connection,
        &buffered_execution(
            "scheduled-execution",
            TriggerKind::Scheduled,
            ProbeOutcome::Available,
        ),
    )
    .await
    .expect("scheduled commit");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT next_due_at_ms FROM channel_monitors WHERE id = 'monitor-1'"
        )
        .fetch_one(&mut connection)
        .await
        .expect("scheduled next due"),
        301_000
    );
}

#[tokio::test]
async fn endpoint_revision_stale_health_writeback_rolls_back_v2_target_when_wrapped_in_write_tx() {
    let mut connection = ready_connection().await;
    sqlx::query("UPDATE stations SET endpoint_revision = 2 WHERE id = 'station-1'")
        .execute(&mut connection)
        .await
        .expect("bump endpoint revision");
    let committer = MonitoringExecutionCommitter::new();
    let execution = buffered_execution(
        "stale-execution",
        TriggerKind::Manual,
        ProbeOutcome::Available,
    );

    let mut tx = connection.begin().await.expect("begin");
    let mut write = persistence::WriteSession::new(&mut tx);
    let result = committer.commit(&mut write, &execution).await;
    drop(write);
    assert!(matches!(result, Err(PersistenceError::NotFound)));
    tx.rollback().await.expect("rollback");

    assert_eq!(
        count(&mut connection, "channel_monitor_executions").await,
        0
    );
    assert_eq!(count(&mut connection, "channel_monitor_attempts").await, 0);
    assert_eq!(
        count(&mut connection, "channel_monitor_target_results").await,
        0
    );
    assert_eq!(
        count(&mut connection, "station_key_health_observations").await,
        0
    );
    assert_eq!(
        count(&mut connection, "channel_monitor_rollup_dirty_ranges").await,
        0
    );
    assert!(!table_exists(&mut connection, "channel_monitor_runs").await);
}

async fn ready_connection() -> SqliteConnection {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await
        .expect("foreign keys");
    MIGRATOR
        .run(&mut connection)
        .await
        .expect("fresh migrations");
    seed_station_monitor(&mut connection).await;
    connection
}

async fn seed_station_monitor(connection: &mut SqliteConnection) {
    sqlx::query(
        r#"
        INSERT INTO stations (
            id, name, station_type, website_url, api_base_url, enabled, priority,
            credit_per_cny, collection_interval_minutes, status, created_at, updated_at
        ) VALUES ('station-1', 'Station', 'openai-compatible', 'https://example.test',
                  'https://example.test/v1', 1, 0, 1.0, 30, 'unchecked', '1', '1')
        "#,
    )
    .execute(&mut *connection)
    .await
    .expect("station");
    sqlx::query("INSERT INTO station_keys (id, station_id) VALUES ('key-1', 'station-1')")
        .execute(&mut *connection)
        .await
        .expect("station key");
    sqlx::query(
        r#"
        INSERT INTO channel_monitor_request_templates (
            id, name, endpoint_kind, method, path, request_body_json,
            enabled, built_in, created_at, updated_at
        ) VALUES ('template-1', 'Chat', 'chat', 'POST', '/v1/chat/completions', '{}', 1, 0, '1', '1')
        "#,
    )
    .execute(&mut *connection)
    .await
    .expect("template");
    sqlx::query(
        r#"
        INSERT INTO channel_monitors (
            id, name, target_type, station_id, station_key_id, template_id,
            enabled, interval_seconds, jitter_seconds, timeout_seconds,
            max_concurrency, consecutive_failure_threshold, fallback_models_json,
            next_run_at, created_at, updated_at, next_due_at_ms
        ) VALUES ('monitor-1', 'Primary', 'station_key', 'station-1', 'key-1', 'template-1',
                  1, 300, 0, 15, 1, 3, '["gpt-primary"]', '999', '1', '1', 999)
        "#,
    )
    .execute(connection)
    .await
    .expect("monitor");
}

fn buffered_execution(
    execution_id: &str,
    trigger_kind: TriggerKind,
    outcome: ProbeOutcome,
) -> BufferedExecution {
    let failure_kind = (outcome == ProbeOutcome::Unavailable).then_some(FailureKind::ServerError);
    let attempt = RecordedAttempt {
        execution_id: execution_id.to_string(),
        station_key_id: "key-1".to_string(),
        model: "gpt-primary".to_string(),
        model_index: 0,
        attempt_number: 0,
        started_at_ms: 1_000,
        finished_at_ms: 1_120,
        ttfb_ms: Some(40),
        first_content_ms: Some(55),
        outcome,
        failure_kind,
        retryable: false,
        http_status: Some(200),
        response_model: Some("upstream-model".to_string()),
        output_bytes: 12,
        semantic_confidence: SemanticConfidence::ProtocolValidated,
        error_summary: None,
    };
    BufferedExecution {
        execution_id: execution_id.to_string(),
        plan: probe_plan(trigger_kind),
        manual_idempotency_key: (trigger_kind == TriggerKind::Manual)
            .then(|| format!("manual:{execution_id}")),
        started_at_ms: 1_000,
        attempts: vec![attempt],
        targets: vec![RecordedTargetResult {
            execution_id: execution_id.to_string(),
            station_id: "station-1".to_string(),
            station_key_id: "key-1".to_string(),
            terminal_outcome: outcome,
            terminal_failure_kind: failure_kind,
            decisive_attempt_id: Some(format!("{execution_id}:key-1:0:0")),
            requested_model: Some("gpt-primary".to_string()),
            effective_model: Some("gpt-primary".to_string()),
            used_fallback: false,
            attempt_count: 1,
            protocol_kind: Some(ProtocolKind::GenericOpenAi),
            request_profile_hash: Some("profile-hash".to_string()),
            endpoint_revision: 1,
        }],
        summary: Some(RecordedExecutionSummary {
            execution_id: execution_id.to_string(),
            target_count: 1,
            available_count: u32::from(outcome == ProbeOutcome::Available),
            degraded_count: 0,
            unavailable_count: u32::from(outcome == ProbeOutcome::Unavailable),
            skipped_count: 0,
            summary_outcome: outcome,
        }),
    }
}

fn probe_plan(trigger_kind: TriggerKind) -> ProbePlan {
    ProbePlan {
        monitor_id: "monitor-1".to_string(),
        revision: DefinitionRevision(1),
        trigger_kind,
        config_snapshot_hash: "snapshot-hash".to_string(),
        target_plans: vec![ProbeTargetPlan {
            station_id: "station-1".to_string(),
            station_key_id: "key-1".to_string(),
            station_key_lifecycle_revision: 1,
            endpoint_revision: 1,
            protocol_kind: Some(ProtocolKind::GenericOpenAi),
            skip_failure_kind: None,
            client_profile: ClientProfileRef {
                id: ClientProfileId::StandardApi,
                version: 1,
            },
            request_profile_hash: Some("profile-hash".to_string()),
        }],
        model_plans: vec![ProbeModelPlan {
            model: "gpt-primary".to_string(),
            role: ProbeModelRole::Primary,
        }],
        schedule_policy: SchedulePolicy {
            interval_seconds: 300,
            jitter_seconds: 0,
            execution_timeout_ms: 60_000,
            attempt_timeout_ms: 45_000,
            slow_latency_threshold_ms: 6_000,
        },
        retry_policy: RetryPolicy::default(),
        risk_policy: RiskPolicy::default(),
        health_policy: HealthPolicy {
            writeback_mode: HealthWritebackMode::Authoritative,
            failure_threshold: 1,
            recovery_threshold: 1,
        },
    }
}

async fn count(connection: &mut SqliteConnection, table: &str) -> i64 {
    let sql = format!("SELECT COUNT(*) AS count FROM {table}");
    sqlx::query(&sql)
        .fetch_one(connection)
        .await
        .expect("count")
        .get("count")
}

async fn table_exists(connection: &mut SqliteConnection, table: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
    )
    .bind(table)
    .fetch_one(connection)
    .await
    .expect("table existence")
        != 0
}
