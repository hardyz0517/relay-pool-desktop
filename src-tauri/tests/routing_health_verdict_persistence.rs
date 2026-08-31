#[path = "../src/application/health_protection.rs"]
mod application_health_protection;
#[path = "../src/persistence/stores/domain_revision_store.rs"]
mod domain_revision_store;
#[path = "../src/models/routing_observation.rs"]
mod model_routing_observation;
#[path = "../src/models/routing_policy.rs"]
mod model_routing_policy;
#[path = "../src/persistence/error.rs"]
mod persistence_error;

mod application {
    pub(crate) mod health_protection {
        pub(crate) use crate::application_health_protection::*;
    }
}

mod models {
    pub(crate) mod routing_policy {
        pub(crate) use crate::model_routing_policy::*;
    }
    pub(crate) mod routing_observation {
        pub(crate) use crate::model_routing_observation::*;
    }
}

mod persistence {
    pub(crate) mod error {
        pub(crate) use crate::persistence_error::*;
    }
    pub(crate) mod migrations {
        pub(crate) fn migrator() -> &'static sqlx::migrate::Migrator {
            static MIGRATOR: sqlx::migrate::Migrator =
                sqlx::migrate!("./src/persistence/migrations");
            &MIGRATOR
        }
    }
    pub(crate) mod stores {
        pub(crate) mod domain_revision_store {
            pub(crate) use crate::domain_revision_store::*;
        }
        pub(crate) mod routing_health_verdict_store {
            pub(crate) use crate::routing_health_verdict_store::*;
        }
    }
}

#[path = "../src/persistence/stores/routing_health_verdict_store.rs"]
mod routing_health_verdict_store;

use std::{fs, path::Path};

use sqlx::{Connection, SqliteConnection};

use persistence::migrations::migrator;

#[tokio::test]
async fn scoped_health_migration_has_typed_shapes_postcondition_and_revision_recovery() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open sqlite");
    migrator()
        .run(&mut connection)
        .await
        .expect("migrate current schema");
    let version: i64 = sqlx::query_scalar(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(&mut connection)
    .await
    .expect("schema version");
    let expected_version = migrator()
        .iter()
        .map(|migration| migration.version)
        .max()
        .expect("migration registry is non-empty");
    assert_eq!(version, expected_version);
    for table in [
        "routing_health_generations",
        "routing_health_observations",
        "routing_health_verdicts",
        "routing_health_projector_state",
    ] {
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        )
        .bind(table)
        .fetch_one(&mut connection)
        .await
        .unwrap();
        assert_eq!(exists, 1, "missing {table}");
    }
    let invalid_shape = sqlx::query(
        "INSERT INTO routing_health_verdicts (generation_id, scope, scope_kind, station_id, verdict, evidence_code, source_observation_id, source_ingested_at_ms, projector_version, updated_at_ms) VALUES ('scoped-health-bootstrap-v1', 'bad', 'station_key_credential', 'station', 'blocked', 'x', 'o', 1, 'v', 1)",
    )
    .execute(&mut connection)
    .await;
    assert!(
        invalid_shape.is_err(),
        "SQL shape guard must reject missing key/revision"
    );

    sqlx::query("INSERT INTO stations (id, name, station_type, website_url, api_base_url, created_at, updated_at) VALUES ('station-revision', 'Station', 'openai_compatible', 'https://example.test', 'https://example.test/v1', '1', '1')")
        .execute(&mut connection).await.unwrap();
    sqlx::query("INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance) VALUES ('station_account:station-revision', 1, 1, 'transactional_write')")
        .execute(&mut connection).await.unwrap();
    let initial: i64 = sqlx::query_scalar(
        "SELECT revision FROM domain_revisions WHERE scope = 'station_account:station-revision'",
    )
    .fetch_one(&mut connection)
    .await
    .unwrap();
    sqlx::query("UPDATE stations SET name = 'Changed' WHERE id = 'station-revision'")
        .execute(&mut connection)
        .await
        .unwrap();
    let unrelated: i64 = sqlx::query_scalar(
        "SELECT revision FROM domain_revisions WHERE scope = 'station_account:station-revision'",
    )
    .fetch_one(&mut connection)
    .await
    .unwrap();
    assert_eq!(
        unrelated, initial,
        "station metadata/endpoint changes cannot recover account health"
    );
    sqlx::query(
        "INSERT INTO station_credentials (station_id, updated_at) VALUES ('station-revision', '1')",
    )
    .execute(&mut connection)
    .await
    .unwrap();
    sqlx::query("UPDATE station_credentials SET login_status = 'active' WHERE station_id = 'station-revision'")
        .execute(&mut connection)
        .await
        .unwrap();
    let changed: i64 = sqlx::query_scalar(
        "SELECT revision FROM domain_revisions WHERE scope = 'station_account:station-revision'",
    )
    .fetch_one(&mut connection)
    .await
    .unwrap();
    assert_eq!(changed, initial + 1);
}

#[tokio::test]
async fn schema_34_upgrades_to_scoped_health_without_mutating_immutable_observations() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open sqlite");
    let bounded = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            migrator()
                .iter()
                .filter(|migration| migration.version <= 34)
                .cloned()
                .collect(),
        ),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    bounded.run(&mut connection).await.expect("schema 34");
    sqlx::query("INSERT INTO routing_observations (id, producer_id, producer_sequence, payload_hash, event_at_ms, ingested_at_ms, scope, source, traffic_equivalence, outcome_kind, evidence_json, created_at_ms) VALUES ('canary', 'producer', 1, ?1, 1, 1, 'station_key:key', 'real_request', 'exact_request', 'unknown', '{}', 1)")
        .bind("a".repeat(64)).execute(&mut connection).await.unwrap();
    migrator().run(&mut connection).await.expect("schema 35");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM routing_observations WHERE id = 'canary'"
        )
        .fetch_one(&mut connection)
        .await
        .unwrap(),
        1
    );
}

#[tokio::test]
async fn schema_35_moves_legacy_routing_boundaries_into_policy_without_resetting_them() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open sqlite");
    let bounded = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            migrator()
                .iter()
                .filter(|migration| migration.version <= 35)
                .cloned()
                .collect(),
        ),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    bounded.run(&mut connection).await.expect("schema 35");
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('max_rate_multiplier', '2.5', '1')",
    )
    .execute(&mut connection)
    .await
    .expect("legacy multiplier");
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('default_routing_group_filter', 'all_groups', '1')",
    )
    .execute(&mut connection)
    .await
    .expect("legacy group filter");

    let through_schema_36 = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            migrator()
                .iter()
                .filter(|migration| migration.version <= 36)
                .cloned()
                .collect(),
        ),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    through_schema_36
        .run(&mut connection)
        .await
        .expect("schema 36");
    let config: String =
        sqlx::query_scalar("SELECT config_json FROM routing_policy WHERE singleton_key = 1")
            .fetch_one(&mut connection)
            .await
            .expect("policy config");
    let value: serde_json::Value = serde_json::from_str(&config).expect("policy JSON");
    assert_eq!(value["max_rate_multiplier"], serde_json::json!(2.5));
    assert_eq!(
        value["routing_group_filter"],
        serde_json::json!("all_groups")
    );
}

#[test]
fn migration_registry_has_unique_contiguous_versions_and_task6_catalog_hooks() {
    let mut versions = migrator()
        .iter()
        .map(|migration| migration.version)
        .collect::<Vec<_>>();
    versions.sort_unstable();
    assert!(versions.windows(2).all(|pair| pair[1] == pair[0] + 1));
    let migration = read("src/persistence/migrations/0035_scoped_routing_health_verdicts.sql");
    for needle in [
        "scope_kind IN",
        "generation_id",
        "watermark_ingested_at_ms",
        "projected_content_hash",
        "persistence_v35_schema_guard",
        "station_group",
        "model_on_key",
    ] {
        assert!(migration.contains(needle), "migration missing {needle}");
    }
    let ownership = read("src/persistence/migrations/0036_routing_policy_ownership.sql");
    assert!(ownership.contains("max_rate_multiplier"));
    assert!(ownership.contains("routing_group_filter"));
    let catalog = read("src/services/portable_migration/catalog.rs");
    for table in [
        "routing_health_generations",
        "routing_health_observations",
        "routing_health_verdicts",
        "routing_health_projector_state",
    ] {
        assert!(catalog.contains(table), "portable catalog missing {table}");
    }
}

fn read(relative: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative))
        .unwrap_or_else(|error| panic!("failed reading {relative}: {error}"))
}
