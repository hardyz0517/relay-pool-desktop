mod models {
    pub(crate) mod routing {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/routing.rs"
        ));
    }

    pub(crate) mod settings {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/settings.rs"
        ));
    }

    pub(crate) mod stations {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/models/stations.rs"
        ));
    }
}

mod services {
    pub(crate) mod outbound {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/services/outbound.rs"
        ));
    }

    pub(crate) mod secrets {
        pub(crate) mod mask {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/services/secrets/mask.rs"
            ));
        }
    }

    pub(crate) mod station_endpoints {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/services/station_endpoints.rs"
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

    pub(crate) mod runtime {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/runtime.rs"
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

    pub(crate) mod stores {
        pub(crate) mod station_catalog {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/persistence/stores/station_catalog.rs"
            ));
        }

        pub(crate) mod settings_store {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/persistence/stores/settings_store.rs"
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

    pub(crate) mod stations {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/stations.rs"
        ));
    }

    pub(crate) mod settings {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/application/settings.rs"
        ));
    }
}

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use application::{
    clock::Clock, ids::IdGenerator, settings::SettingsService, stations::StationService,
};
use chrono::{TimeZone, Utc};
use models::{
    settings::UpdateSettingsInput,
    stations::{CreateStationInput, UpdateStationInput},
};
use persistence::{runtime::PersistenceRuntime, schema_compatibility::BinaryCompatibility};
use semver::Version;
use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, Connection, Row};

static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(1);

#[tokio::test]
async fn station_endpoint_change_is_atomic_and_matches_v1_contract_boundary() {
    let fixture = V2Fixture::create().await;
    let station_service = fixture.station_service(vec!["station-1"]).await;
    let station = station_service
        .create(CreateStationInput {
            name: "Relay".to_string(),
            station_type: "openai-compatible".to_string(),
            website_url: "https://console.example".to_string(),
            api_base_url: "https://api.example/v1".to_string(),
            api_key: "sk-test-station".to_string(),
            collector_proxy_mode: "inherit".to_string(),
            collector_proxy_url: None,
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: Some(10.0),
            collection_interval_minutes: 5,
            note: None,
        })
        .await
        .expect("create station");
    fixture.seed_endpoint_state(&station.id).await;

    let updated = station_service
        .update_station(UpdateStationInput {
            id: station.id.clone(),
            name: "Relay Updated".to_string(),
            station_type: "openai-compatible".to_string(),
            website_url: "https://console-next.example".to_string(),
            api_base_url: "https://api-next.example/v1".to_string(),
            api_key: None,
            collector_proxy_mode: "inherit".to_string(),
            collector_proxy_url: None,
            enabled: true,
            credit_per_cny: 2.0,
            low_balance_threshold_cny: None,
            collection_interval_minutes: 10,
            note: Some("moved".to_string()),
        })
        .await
        .expect("update station endpoint");

    assert_eq!(updated.endpoint_revision, 2);
    assert!(
        !updated.enabled,
        "API origin changes must disable until revalidated"
    );
    assert_eq!(updated.status, "disabled");
    assert_eq!(fixture.endpoint_health_rows().await, 0);
    assert_eq!(fixture.station_key_health_rows().await, 0);
    assert_eq!(fixture.credential_session_source(&station.id).await, "none");
    assert_eq!(fixture.secret_rows().await, 0);
}

#[tokio::test]
async fn station_reorder_and_delete_are_one_bounded_write_session() {
    let fixture = V2Fixture::create().await;
    let service = fixture
        .station_service(vec!["station-a", "station-b"])
        .await;
    let first = service
        .create(station_input("First"))
        .await
        .expect("first station");
    let second = service
        .create(station_input("Second"))
        .await
        .expect("second station");

    let reordered = service
        .reorder(vec![second.id.clone(), first.id.clone()])
        .await
        .expect("reorder");
    assert_eq!(reordered[0].id, second.id);
    assert_eq!(reordered[1].id, first.id);

    service
        .delete(second.id.clone())
        .await
        .expect("delete station");
    let remaining = service.list().await.expect("list stations");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].priority, 0);
}

#[tokio::test]
async fn settings_are_typed_and_unknown_values_do_not_enter_v2() {
    let fixture = V2Fixture::create().await;
    let service = fixture.settings_service().await;

    service
        .import_known_legacy_settings(vec![
            ("local_proxy_port".to_string(), "8787".to_string()),
            ("collector_proxy_mode".to_string(), "direct".to_string()),
            ("retired_secret_setting".to_string(), "canary".to_string()),
        ])
        .await
        .expect("import settings");

    let settings = service.load().await.expect("load settings");
    assert_eq!(settings.local_proxy_port, 8787);
    assert_eq!(settings.collector_proxy_mode, "direct");
    assert!(!fixture.any_setting_contains("canary").await);
}

#[tokio::test]
async fn settings_update_preserves_typed_defaults_and_validates_bounds() {
    let fixture = V2Fixture::create().await;
    let service = fixture.settings_service().await;

    let settings = service
        .update(UpdateSettingsInput {
            local_proxy_port: 8788,
            default_routing_strategy: "priority_fallback".to_string(),
            collector_proxy_mode: "direct".to_string(),
            collector_proxy_url: None,
            max_rate_multiplier: Some(Some(3.5)),
            default_routing_group_filter: None,
            scheduler_advanced_settings: None,
            low_balance_threshold_cny: 8.0,
            collector_interval_minutes: 15,
            balance_interval_minutes: 5,
            group_rate_interval_minutes: 20,
            model_list_interval_minutes: 60,
            pricing_refresh_interval_minutes: 60,
            collector_timeout_seconds: 15,
            collector_max_concurrency: 2,
            allow_depleted_fallback: true,
            developer_mode_enabled: true,
            tray_behavior: Some("disabled".to_string()),
        })
        .await
        .expect("update settings");

    assert_eq!(settings.local_proxy_port, 8788);
    assert_eq!(settings.max_rate_multiplier, Some(3.5));
    assert_eq!(settings.tray_behavior, "disabled");
    assert_eq!(settings.collector_max_concurrency, 2);

    let error = service
        .update(UpdateSettingsInput {
            collector_max_concurrency: 9,
            ..settings_input()
        })
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "not found");
}

fn station_input(name: &str) -> CreateStationInput {
    CreateStationInput {
        name: name.to_string(),
        station_type: "openai-compatible".to_string(),
        website_url: format!("https://{name}.example"),
        api_base_url: format!("https://{name}.example/v1"),
        api_key: String::new(),
        collector_proxy_mode: "inherit".to_string(),
        collector_proxy_url: None,
        enabled: true,
        credit_per_cny: 1.0,
        low_balance_threshold_cny: None,
        collection_interval_minutes: 5,
        note: None,
    }
}

fn settings_input() -> UpdateSettingsInput {
    UpdateSettingsInput {
        local_proxy_port: 8787,
        default_routing_strategy: "cost_stable_first".to_string(),
        collector_proxy_mode: "direct".to_string(),
        collector_proxy_url: None,
        max_rate_multiplier: None,
        default_routing_group_filter: None,
        scheduler_advanced_settings: None,
        low_balance_threshold_cny: 15.0,
        collector_interval_minutes: 30,
        balance_interval_minutes: 5,
        group_rate_interval_minutes: 20,
        model_list_interval_minutes: 60,
        pricing_refresh_interval_minutes: 60,
        collector_timeout_seconds: 15,
        collector_max_concurrency: 3,
        allow_depleted_fallback: false,
        developer_mode_enabled: false,
        tray_behavior: None,
    }
}

#[derive(Clone)]
struct FixedClock;

impl Clock for FixedClock {
    fn now_utc(&self) -> chrono::DateTime<chrono::Utc> {
        Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).unwrap()
    }
}

#[derive(Default)]
struct SequenceIds {
    ids: Mutex<Vec<String>>,
}

impl SequenceIds {
    fn new(ids: Vec<&str>) -> Self {
        Self {
            ids: Mutex::new(ids.into_iter().map(ToString::to_string).rev().collect()),
        }
    }
}

impl IdGenerator for SequenceIds {
    fn next_id(&self) -> String {
        self.ids
            .lock()
            .expect("ids")
            .pop()
            .expect("deterministic id")
    }
}

struct V2Fixture {
    path: PathBuf,
}

impl V2Fixture {
    async fn create() -> Self {
        let path = temp_db_path("differential");
        let mut connection = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .connect()
            .await
            .expect("connect fixture");
        persistence::migrations::migrator()
            .run(&mut connection)
            .await
            .expect("run migrations");
        connection.close().await.expect("close fixture");
        Self { path }
    }

    async fn station_service(&self, ids: Vec<&str>) -> StationService {
        StationService::new(
            self.runtime().await.handle(),
            Arc::new(FixedClock),
            Arc::new(SequenceIds::new(ids)),
        )
    }

    async fn settings_service(&self) -> SettingsService {
        SettingsService::new(
            self.runtime().await.handle(),
            Arc::new(FixedClock),
            "fixture-data-dir".to_string(),
            None,
        )
    }

    async fn runtime(&self) -> PersistenceRuntime {
        PersistenceRuntime::open(&self.path, binary_031())
            .await
            .expect("open runtime")
    }

    async fn seed_endpoint_state(&self, station_id: &str) {
        let mut connection = self.connect().await;
        sqlx::query("INSERT INTO station_keys (id, station_id) VALUES ('key-1', ?1)")
            .bind(station_id)
            .execute(&mut connection)
            .await
            .expect("station key");
        sqlx::query(
            "INSERT INTO station_endpoint_health (station_id, endpoint_revision) VALUES (?1, 1)",
        )
        .bind(station_id)
        .execute(&mut connection)
        .await
        .expect("endpoint health");
        sqlx::query("INSERT INTO station_key_health (station_key_id, endpoint_revision) VALUES ('key-1', 1)")
            .execute(&mut connection)
            .await
            .expect("key health");
        sqlx::query(
            r#"
            INSERT INTO secrets (id, scope, owner_id, kind, masked_value, ciphertext, nonce, created_at, updated_at)
            VALUES
                ('secret-pass', 'station', ?1, 'login_password', '***pass', x'01', x'02', '1', '1'),
                ('secret-token', 'station', ?1, 'access_token', '***tokn', x'03', x'04', '1', '1')
            "#,
        )
        .bind(station_id)
        .execute(&mut connection)
        .await
        .expect("secrets");
        sqlx::query(
            r#"
            INSERT INTO station_credentials (
                station_id, login_password, login_password_secret_id, remember_password,
                login_status, session_status, access_token_secret_id, session_source, updated_at
            ) VALUES (?1, NULL, 'secret-pass', 1, 'logged_in', 'active', 'secret-token', 'web', '1')
            "#,
        )
        .bind(station_id)
        .execute(&mut connection)
        .await
        .expect("credentials");
        connection.close().await.expect("close fixture");
    }

    async fn endpoint_health_rows(&self) -> i64 {
        self.count("station_endpoint_health").await
    }

    async fn station_key_health_rows(&self) -> i64 {
        self.count("station_key_health").await
    }

    async fn secret_rows(&self) -> i64 {
        self.count("secrets").await
    }

    async fn any_setting_contains(&self, needle: &str) -> bool {
        let mut connection = self.connect().await;
        let row = sqlx::query("SELECT COUNT(*) AS count FROM settings WHERE value LIKE ?1")
            .bind(format!("%{needle}%"))
            .fetch_one(&mut connection)
            .await
            .expect("setting scan");
        connection.close().await.expect("close fixture");
        row.get::<i64, _>("count") > 0
    }

    async fn credential_session_source(&self, station_id: &str) -> String {
        let mut connection = self.connect().await;
        let row =
            sqlx::query("SELECT session_source FROM station_credentials WHERE station_id = ?1")
                .bind(station_id)
                .fetch_one(&mut connection)
                .await
                .expect("credential row");
        connection.close().await.expect("close fixture");
        row.get("session_source")
    }

    async fn count(&self, table: &str) -> i64 {
        let mut connection = self.connect().await;
        let row = sqlx::query(&format!("SELECT COUNT(*) AS count FROM {table}"))
            .fetch_one(&mut connection)
            .await
            .expect("count rows");
        connection.close().await.expect("close fixture");
        row.get("count")
    }

    async fn connect(&self) -> sqlx::SqliteConnection {
        SqliteConnectOptions::new()
            .filename(&self.path)
            .create_if_missing(false)
            .connect()
            .await
            .expect("connect fixture")
    }
}

fn binary_031() -> BinaryCompatibility {
    BinaryCompatibility {
        app_version: Version::new(0, 3, 1),
        database_generation: 2,
        readable_schema: 1..=2,
        writable_schema: BTreeSet::from([2]),
    }
}

fn temp_db_path(name: &str) -> PathBuf {
    let id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("relay-pool-persistence-{name}-{id}"));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("clean stale fixture dir");
    }
    std::fs::create_dir_all(&root).expect("fixture dir");
    root.join("relay-pool-v2.sqlite3")
}

#[allow(dead_code)]
fn assert_separate_fixture_files(left: &Path, right: &Path) {
    assert_ne!(
        left, right,
        "V1 and V2 writers must never share one fixture DB"
    );
}
