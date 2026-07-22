mod models {
    pub(crate) mod proxy {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/models/proxy.rs"));
    }
    pub(crate) mod secrets {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/secrets.rs"
        ));
    }
    pub(crate) mod station_endpoints {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/station_endpoints.rs"
        ));
    }

    pub(crate) mod collector {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/collector.rs"
        ));
    }

    pub(crate) mod change_events {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/change_events.rs"
        ));
    }
    pub(crate) mod collector_runs {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/collector_runs.rs"
        ));
    }
    pub(crate) mod group_facts {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/group_facts.rs"
        ));
    }
    pub(crate) mod stations {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/stations.rs"
        ));
    }

    pub(crate) mod shared_capabilities {
        use serde::Serialize;

        #[derive(Debug, Clone, Serialize)]
        #[serde(rename_all = "camelCase")]
        pub struct StationGroupOption {
            pub value: String,
            pub group_binding_id: Option<String>,
            pub group_id_hash: Option<String>,
            pub group_name: String,
            pub rate_multiplier: Option<f64>,
            pub inferred_group_category: Option<String>,
            pub group_category_override: Option<String>,
            pub effective_group_category: String,
            pub rate_source: Option<String>,
            pub selectable_for_remote_key: bool,
        }
    }
}

mod services {
    pub(crate) mod group_categories {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/services/group_categories.rs"
        ));
    }
    pub(crate) mod shared_capabilities {
        use std::collections::{HashMap, HashSet};

        use crate::{
            models::{
                group_facts::{
                    GroupRateRecord, StationGroupBinding, BINDING_KIND_STATION_GROUP,
                    BINDING_STATUS_DISABLED, BINDING_STATUS_MANUAL_LEGACY,
                },
                shared_capabilities::StationGroupOption,
            },
            services::group_categories::{infer_group_category, normalize_group_category},
        };

        pub fn station_group_options_from_facts(
            bindings: Vec<StationGroupBinding>,
            rates: Vec<GroupRateRecord>,
        ) -> Vec<StationGroupOption> {
            let latest_rates = latest_rates_by_binding_or_hash(rates);
            let mut seen_values = HashSet::new();
            let mut options = bindings
                .into_iter()
                .filter(is_selectable_station_group_binding)
                .filter_map(|binding| {
                    let rate = latest_rates
                        .get(&rate_key_for_binding_id(&binding.id))
                        .or_else(|| {
                            latest_rates.get(&rate_key_for_group_hash(&binding.group_key_hash))
                        });
                    let value = format!("binding:{}", binding.id);
                    if !seen_values.insert(value.clone()) {
                        return None;
                    }
                    let group_id_hash = binding
                        .group_id_hash
                        .clone()
                        .filter(|value| !value.trim().is_empty())
                        .or_else(|| Some(binding.group_key_hash.clone()));
                    let rate_multiplier = binding
                        .effective_rate_multiplier
                        .or(binding.default_rate_multiplier)
                        .or_else(|| rate.and_then(|record| record.effective_rate_multiplier))
                        .or_else(|| rate.and_then(|record| record.default_rate_multiplier));
                    let rate_source = binding
                        .rate_source
                        .clone()
                        .or_else(|| rate.map(|record| record.source.clone()));
                    let selectable_for_remote_key = binding
                        .group_id_hash
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty());
                    let inferred_group_category = normalize_group_category(
                        binding.inferred_group_category.as_deref().or_else(|| {
                            rate.and_then(|record| record.inferred_group_category.as_deref())
                        }),
                    )
                    .unwrap_or_else(|| {
                        infer_group_category(
                            &binding.group_name,
                            rate.and_then(|record| record.raw_json_redacted.as_ref())
                                .or(binding.raw_json_redacted.as_ref()),
                        )
                    });
                    let group_category_override =
                        normalize_group_category(binding.group_category_override.as_deref());
                    let effective_group_category = group_category_override
                        .clone()
                        .unwrap_or_else(|| inferred_group_category.clone());

                    Some(StationGroupOption {
                        value,
                        group_binding_id: Some(binding.id),
                        group_id_hash,
                        group_name: binding.group_name,
                        rate_multiplier,
                        inferred_group_category: Some(inferred_group_category),
                        group_category_override,
                        effective_group_category,
                        rate_source,
                        selectable_for_remote_key,
                    })
                })
                .collect::<Vec<_>>();

            options.sort_by(|left, right| {
                left.group_name
                    .to_lowercase()
                    .cmp(&right.group_name.to_lowercase())
                    .then_with(|| left.value.cmp(&right.value))
            });
            options
        }

        fn latest_rates_by_binding_or_hash(
            rates: Vec<GroupRateRecord>,
        ) -> HashMap<String, GroupRateRecord> {
            let mut latest = HashMap::new();
            for rate in rates
                .into_iter()
                .filter(|rate| rate.binding_kind == BINDING_KIND_STATION_GROUP)
            {
                if let Some(group_binding_id) = rate.group_binding_id.as_deref() {
                    latest
                        .entry(rate_key_for_binding_id(group_binding_id))
                        .or_insert_with(|| rate.clone());
                }
                latest
                    .entry(rate_key_for_group_hash(&rate.group_key_hash))
                    .or_insert(rate);
            }
            latest
        }

        fn is_selectable_station_group_binding(binding: &StationGroupBinding) -> bool {
            binding.binding_kind == BINDING_KIND_STATION_GROUP
                && binding.binding_status != BINDING_STATUS_DISABLED
                && binding.binding_status != BINDING_STATUS_MANUAL_LEGACY
                && binding.rate_source.as_deref() != Some("legacy_key_group")
        }

        fn rate_key_for_binding_id(id: &str) -> String {
            format!("binding:{id}")
        }

        fn rate_key_for_group_hash(group_key_hash: &str) -> String {
            format!("group:{group_key_hash}")
        }
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
    pub(crate) mod runtime_lifecycle {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/runtime_lifecycle.rs"
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
        pub(crate) mod station_catalog {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/persistence/stores/station_catalog.rs"
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
    pub(crate) mod pagination {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/pagination.rs"
        ));
    }
    pub(crate) mod collectors {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/collectors.rs"
        ));
    }
    pub(crate) mod stations {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/stations.rs"
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
    clock::{Clock, SystemClock},
    collectors::{
        CanonicalCollectorFacts, CanonicalGroupFact, CanonicalModelFact, CanonicalRateFact,
        CollectorApplyRequest, CollectorService,
    },
    error::ApplicationError,
    ids::{IdGenerator, UuidV7Generator},
    pagination::PageLimit,
    stations::StationService,
};
use chrono::{TimeZone, Utc};
use models::{
    change_events::UpsertChangeEventInput,
    collector::{StationLoginTestInput, StationLoginTestResult},
    group_facts::UpdateStationKeyGroupBindingInput,
    stations::{EndpointPingResult, StationEndpointHealth, UpdateStationInput},
};
use persistence::{
    runtime::PersistenceRuntime,
    schema_compatibility::BinaryCompatibility,
    stores::change_store::{ChangeCursor, ChangeStore},
};
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

#[tokio::test]
async fn collector_queries_and_adjacent_store_contracts_share_the_v2_runtime() {
    let fixture = Fixture::create("query-contracts").await;
    let runtime = fixture.runtime().await;
    assert_eq!(runtime.compatibility_decision_code(), "writable");
    assert_eq!(
        runtime.health().await.expect("runtime health").open_mode,
        "writable"
    );

    let collector = CollectorService::new(
        runtime.handle(),
        Arc::new(FixedClock),
        Arc::new(SequentialIds(AtomicU64::new(1))),
    );
    collector
        .apply_result(full_result("run-query-contracts", 1, "sub2api"))
        .await
        .expect("collector apply");
    let limit = PageLimit::new(10).expect("page limit");
    let _due = collector.due_stations(limit).await.expect("due stations");
    assert_eq!(
        collector
            .list_station_snapshots("station-1", limit)
            .await
            .expect("station snapshots")
            .len(),
        1
    );
    assert!(collector
        .latest_station_snapshot("station-1")
        .await
        .expect("latest snapshot")
        .is_some());

    let changes = ChangeStore;
    let mut read = runtime.begin_read().await.expect("change read");
    let page = changes
        .list_page(&mut read, Some("station-1"), None, 10)
        .await
        .expect("change page");
    let first = page.items.first().expect("collector change event").clone();
    let cursor = ChangeCursor {
        updated_at: first.updated_at.clone(),
        id: first.id.clone(),
    };
    changes
        .list_page(&mut read, Some("station-1"), Some(&cursor), 10)
        .await
        .expect("cursor page");
    drop(read);

    let event_id = first.id.clone();
    let changes_for_status = changes;
    runtime
        .write(|write| {
            Box::pin(async move {
                changes_for_status
                    .set_status(write, &event_id, "unread", "1700000000001")
                    .await?;
                Ok(())
            })
        })
        .await
        .expect("reset event status");
    let event_id = first.id.clone();
    let changes_for_read = changes;
    runtime
        .write(|write| {
            Box::pin(async move {
                let events = changes_for_read
                    .mark_many_read(write, &[event_id], "1700000000002")
                    .await?;
                assert_eq!(events.len(), 1);
                Ok(())
            })
        })
        .await
        .expect("mark event read");

    let stations = StationService::new(
        runtime.handle(),
        Arc::new(SystemClock),
        Arc::new(UuidV7Generator),
    );
    assert_eq!(stations.list().await.expect("station list").len(), 1);
    assert_eq!(
        stations
            .station_for_capture("station-1")
            .await
            .expect("station capture")
            .id,
        "station-1"
    );
    let updated = stations
        .update_station(UpdateStationInput {
            id: "station-1".to_string(),
            name: "Station Updated".to_string(),
            station_type: "sub2api".to_string(),
            website_url: "https://updated.example.test".to_string(),
            api_base_url: "https://api.updated.example.test/v1".to_string(),
            api_key: None,
            collector_proxy_mode: "inherit".to_string(),
            collector_proxy_url: None,
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: None,
            collection_interval_minutes: 5,
            note: Some("updated".to_string()),
        })
        .await
        .expect("station update");
    assert_eq!(updated.endpoint_revision, 2);
    assert_eq!(
        stations
            .reorder(vec!["station-1".to_string()])
            .await
            .expect("station reorder")[0]
            .priority,
        0
    );

    let backup_path = fixture
        .path
        .with_file_name("collector-contract-backup.sqlite3");
    persistence::backup::create_verified_backup_from_path(&fixture.path, &backup_path)
        .await
        .expect("verified backup");
    std::fs::remove_file(&backup_path).expect("remove backup");

    let changes_for_clear = changes;
    runtime
        .write(|write| Box::pin(async move { changes_for_clear.clear(write).await }))
        .await
        .expect("clear events");
    stations
        .delete("station-1".to_string())
        .await
        .expect("station delete");
}

#[test]
fn source_included_command_models_keep_their_serialization_contracts() {
    let login: StationLoginTestInput = serde_json::from_value(serde_json::json!({
        "stationType": "sub2api",
        "websiteUrl": "https://example.test",
        "loginUsername": "user",
        "loginPassword": "secret"
    }))
    .expect("login input");
    assert_eq!(login.station_type.as_deref(), Some("sub2api"));
    assert_eq!(login.website_url, "https://example.test");
    assert_eq!(login.login_username, "user");
    assert_eq!(login.login_password, "secret");

    let login_result = StationLoginTestResult {
        status: "manual_required".to_string(),
        message: "manual login required".to_string(),
        diagnosis: Some("captcha".to_string()),
        token_present: false,
    };
    let result_json = serde_json::to_value(login_result).expect("login result");
    assert_eq!(result_json["status"], "manual_required");

    let binding: UpdateStationKeyGroupBindingInput = serde_json::from_value(serde_json::json!({
        "stationKeyId": "key-1",
        "groupBindingId": "binding-1"
    }))
    .expect("group binding input");
    assert_eq!(
        (
            binding.station_key_id.as_str(),
            binding.group_binding_id.as_str()
        ),
        ("key-1", "binding-1")
    );

    let event: UpsertChangeEventInput = serde_json::from_value(serde_json::json!({
        "severity": "info",
        "eventType": "collector_contract",
        "title": "Collector contract",
        "message": "verified",
        "objectType": "station",
        "objectId": "station-1",
        "stationId": "station-1",
        "stationKeyId": null,
        "pricingRuleId": null,
        "requestLogId": null,
        "oldValueJson": null,
        "newValueJson": null,
        "impactJson": null,
        "dedupeKey": "collector-contract",
        "source": "test"
    }))
    .expect("change input");
    assert_eq!(event.event_type, "collector_contract");
    assert_eq!(event.severity, "info");
    assert_eq!(event.title, "Collector contract");
    assert_eq!(event.message, "verified");
    assert_eq!(event.object_type, "station");
    assert_eq!(event.object_id.as_deref(), Some("station-1"));
    assert_eq!(event.station_id.as_deref(), Some("station-1"));
    assert!(event.station_key_id.is_none());
    assert!(event.pricing_rule_id.is_none());
    assert!(event.request_log_id.is_none());
    assert!(event.old_value_json.is_none());
    assert!(event.new_value_json.is_none());
    assert!(event.impact_json.is_none());
    assert_eq!(event.dedupe_key, "collector-contract");
    assert_eq!(event.source, "test");

    let health = StationEndpointHealth {
        station_id: "station-1".to_string(),
        endpoint_revision: 1,
        status: "healthy".to_string(),
        latency_ms: Some(10),
        checked_at: Some("1".to_string()),
        error_summary: None,
        updated_at: "1".to_string(),
    };
    let ping = EndpointPingResult {
        station_id: "station-1".to_string(),
        ok: true,
        status: "healthy".to_string(),
        latency_ms: Some(10),
        checked_at: "1".to_string(),
        error_summary: None,
    };
    assert_eq!(
        serde_json::to_value(health).expect("health")["endpointRevision"],
        1
    );
    assert_eq!(serde_json::to_value(ping).expect("ping")["ok"], true);
    assert_eq!(
        models::secrets::redact_text_preview("token=secret", 64),
        "[REDACTED]"
    );
    assert_eq!(
        ApplicationError::SecretValidationFailed.to_string(),
        "secret validation failed"
    );
    assert!(!UuidV7Generator.next_id().is_empty());
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
        let runtime = self.runtime().await;
        CollectorService::new(
            runtime.handle(),
            Arc::new(FixedClock),
            Arc::new(SequentialIds(AtomicU64::new(1))),
        )
    }

    async fn runtime(&self) -> PersistenceRuntime {
        PersistenceRuntime::open(&self.path, binary_8())
            .await
            .expect("open runtime")
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

fn binary_8() -> BinaryCompatibility {
    let schema_version = persistence::migrations::current_schema_version();
    BinaryCompatibility {
        app_version: Version::new(0, 3, 1),
        database_generation: 2,
        readable_schema: 1..=schema_version,
        writable_schema: BTreeSet::from([schema_version]),
    }
}
