use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    time::Duration,
};

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Executor, Sqlite,
};

use crate::persistence::{
    backup::{create_verified_backup_from_path, validate_read_only_sqlite},
    error::PersistenceError,
    maintenance::request_log_url_sanitizer::{
        sanitize_request_log_upstream_urls, sanitize_request_log_upstream_urls_before_schema18,
        RequestLogUrlSanitizerOptions,
    },
    schema_compatibility::{decide_open_mode, load_schema_compatibility, BinaryCompatibility},
    schema_registry,
};

pub(crate) fn migrator() -> &'static sqlx::migrate::Migrator {
    static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./src/persistence/migrations");
    &MIGRATOR
}

pub(crate) async fn applied_schema_version<'e, E>(executor: E) -> Result<i64, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let row = sqlx::query!(
        r#"
        SELECT version AS "version!: i64"
        FROM _sqlx_migrations
        WHERE success = 1
        ORDER BY version DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(executor)
    .await?;
    row.map(|row| row.version)
        .ok_or(PersistenceError::MissingMigrationMetadata)
}

pub(crate) async fn initialize_v2_database(path: &Path) -> Result<(), PersistenceError> {
    if path.exists() {
        return Err(PersistenceError::InvariantViolation(
            "generation 2 database already exists".to_string(),
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pool = migration_pool_create(path).await?;
    migrator().run(&pool).await?;
    sanitize_request_log_upstream_urls(&pool, RequestLogUrlSanitizerOptions::default()).await?;
    let compatibility = load_schema_compatibility(&pool).await?;
    let schema_version = applied_schema_version(&pool).await?;
    decide_open_mode(
        &current_binary_compatibility(),
        &compatibility,
        schema_version,
    )?;
    pool.close().await;
    Ok(())
}

#[cfg(test)]
pub(crate) async fn initialize_v2_database_at_schema_for_test(
    path: &Path,
    target_schema: i64,
) -> Result<(), PersistenceError> {
    if path.exists() {
        return Err(PersistenceError::InvariantViolation(
            "generation 2 database already exists".to_string(),
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pool = migration_pool_create(path).await?;
    let bounded = migrator_through(target_schema)?;
    bounded.run(&pool).await?;
    pool.close().await;
    Ok(())
}

pub(crate) async fn upgrade_existing_v2_database(
    path: &Path,
) -> Result<Option<PathBuf>, PersistenceError> {
    if !path.is_file() {
        return Err(PersistenceError::MissingDatabase);
    }

    let pool = migration_pool_existing(path).await?;
    let compatibility = load_schema_compatibility(&pool).await?;
    let schema_version = applied_schema_version(&pool).await?;
    let binary = current_binary_compatibility();
    let open_mode = decide_open_mode(&binary, &compatibility, schema_version)?;
    if open_mode == crate::persistence::schema_compatibility::OpenMode::Writable {
        if schema_version >= 18 {
            sanitize_request_log_upstream_urls(&pool, RequestLogUrlSanitizerOptions::default())
                .await?;
        }
        pool.close().await;
        return Ok(None);
    }
    if schema_version >= current_schema_version()
        || binary.app_version < compatibility.min_writer_app_version
    {
        pool.close().await;
        return Err(PersistenceError::InvariantViolation(
            "generation 2 schema is not eligible for an in-place upgrade".to_string(),
        ));
    }
    if (5..18).contains(&schema_version) {
        sanitize_request_log_upstream_urls_before_schema18(
            &pool,
            RequestLogUrlSanitizerOptions::default(),
        )
        .await?;
    }
    pool.close().await;

    let backup_path = schema_upgrade_backup_path(path, schema_version)?;
    create_verified_backup_from_path(path, &backup_path).await?;

    let pool = migration_pool_existing(path).await?;
    if let Err(error) = migrator().run(&pool).await {
        pool.close().await;
        return Err(error.into());
    }
    sanitize_request_log_upstream_urls(&pool, RequestLogUrlSanitizerOptions::default()).await?;
    let compatibility = load_schema_compatibility(&pool).await?;
    let schema_version = applied_schema_version(&pool).await?;
    let mode = decide_open_mode(&binary, &compatibility, schema_version)?;
    pool.close().await;
    if mode != crate::persistence::schema_compatibility::OpenMode::Writable {
        return Err(PersistenceError::InvariantViolation(
            "generation 2 schema upgrade did not produce a writable database".to_string(),
        ));
    }
    validate_read_only_sqlite(path).await?;
    Ok(Some(backup_path))
}

pub(crate) async fn upgrade_existing_v2_database_to_schema(
    path: &Path,
    target_schema: i64,
) -> Result<Option<PathBuf>, PersistenceError> {
    schema_registry::validate_migration_registry()?;
    if !path.is_file() {
        return Err(PersistenceError::MissingDatabase);
    }
    if target_schema >= current_schema_version() {
        return upgrade_existing_v2_database(path).await;
    }

    let pool = migration_pool_existing(path).await?;
    let compatibility = load_schema_compatibility(&pool).await?;
    let schema_version = applied_schema_version(&pool).await?;
    let binary = current_binary_compatibility();
    let open_mode = decide_open_mode(&binary, &compatibility, schema_version)?;
    if compatibility.schema_version >= target_schema && schema_version >= target_schema {
        pool.close().await;
        return Ok(None);
    }
    if compatibility.schema_version > target_schema
        || schema_version != compatibility.schema_version
        || open_mode == crate::persistence::schema_compatibility::OpenMode::Writable
    {
        pool.close().await;
        return Err(PersistenceError::InvariantViolation(
            "generation 2 schema is not eligible for a bounded upgrade".to_string(),
        ));
    }
    pool.close().await;

    let backup_path =
        schema_upgrade_backup_path_to_schema(path, compatibility.schema_version, target_schema)?;
    create_verified_backup_from_path(path, &backup_path).await?;

    let pool = migration_pool_existing(path).await?;
    let bounded = migrator_through(target_schema)?;
    if let Err(error) = bounded.run(&pool).await {
        pool.close().await;
        return Err(error.into());
    }
    let compatibility = load_schema_compatibility(&pool).await?;
    let schema_version = applied_schema_version(&pool).await?;
    pool.close().await;
    if compatibility.schema_version != target_schema || schema_version != target_schema {
        return Err(PersistenceError::InvariantViolation(
            "bounded schema upgrade did not reach the requested target".to_string(),
        ));
    }
    validate_read_only_sqlite(path).await?;
    Ok(Some(backup_path))
}

pub(crate) fn current_binary_compatibility() -> BinaryCompatibility {
    schema_registry::current_binary_compatibility()
}

pub(crate) fn current_schema_version() -> i64 {
    migrator()
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or_default()
}

async fn migration_pool_create(database_path: &Path) -> Result<sqlx::SqlitePool, PersistenceError> {
    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    Ok(SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await?)
}

async fn migration_pool_existing(
    database_path: &Path,
) -> Result<sqlx::SqlitePool, PersistenceError> {
    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    Ok(SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await?)
}

fn schema_upgrade_backup_path(
    database_path: &Path,
    source_schema: i64,
) -> Result<PathBuf, PersistenceError> {
    let parent = database_path.parent().ok_or(PersistenceError::IoFailed {
        kind: std::io::ErrorKind::InvalidInput,
    })?;
    Ok(parent.join("backups").join(format!(
        "relay-pool-v2-schema-{source_schema}-to-{}-{}.sqlite3",
        current_schema_version(),
        uuid::Uuid::now_v7()
    )))
}

fn schema_upgrade_backup_path_to_schema(
    database_path: &Path,
    source_schema: i64,
    target_schema: i64,
) -> Result<PathBuf, PersistenceError> {
    let parent = database_path.parent().ok_or(PersistenceError::IoFailed {
        kind: std::io::ErrorKind::InvalidInput,
    })?;
    Ok(parent.join("backups").join(format!(
        "relay-pool-v2-schema-{source_schema}-to-{target_schema}-{}.sqlite3",
        uuid::Uuid::now_v7()
    )))
}

pub(crate) fn migrator_through(
    target_schema: i64,
) -> Result<sqlx::migrate::Migrator, PersistenceError> {
    let migrations: Vec<_> = migrator()
        .iter()
        .filter(|migration| migration.version <= target_schema)
        .cloned()
        .collect();
    if !migrations
        .iter()
        .any(|migration| migration.version == target_schema)
    {
        return Err(PersistenceError::InvariantViolation(format!(
            "target schema {target_schema} is not present in migration registry"
        )));
    }
    Ok(sqlx::migrate::Migrator {
        migrations: Cow::Owned(migrations),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    })
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use sha2::{Digest, Sha384};
    use sqlx::{migrate::Migrator, Connection, Row};

    use super::*;
    use crate::persistence::runtime::PersistenceRuntime;

    #[test]
    fn model_mapping_foundation_checksum_is_frozen() {
        let mut hasher = Sha384::new();
        hasher.update(include_bytes!(
            "migrations/0043_model_mapping_foundation.sql"
        ));
        let checksum = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>();
        assert_eq!(
            checksum,
            "3D6D2CFC7A8708FB1FBF7F5053EBBB7A151C01AD6A23C9CDE7AF95A8C64589DF6D061CDD60BFDEF7F4FBE75E2A5080BF"
        );
    }

    #[tokio::test]
    async fn schema_43_upgrade_applies_legacy_priority_repair() {
        let mut connection = sqlx::SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open sqlite");
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&mut connection)
            .await
            .expect("enable foreign keys");
        migrator_through(42)
            .expect("schema 42 migrator")
            .run(&mut connection)
            .await
            .expect("migrate schema 42");
        sqlx::query(
            "INSERT INTO model_aliases
                (id, client_model, upstream_model, enabled, created_at, updated_at)
             VALUES ('mapping-upgrade-alias', 'codex-upgrade', 'native-upgrade', 1, '1', '1')",
        )
        .execute(&mut connection)
        .await
        .expect("insert legacy alias");

        migrator_through(43)
            .expect("schema 43 migrator")
            .run(&mut connection)
            .await
            .expect("migrate schema 43");
        sqlx::query(
            "UPDATE model_mapping_rules SET priority = 0
             WHERE id = 'legacy-model-alias-rule:636F6465782D75706772616465'",
        )
        .execute(&mut connection)
        .await
        .expect("simulate legacy priority variant");
        sqlx::query(
            "UPDATE model_mapping_document_history
             SET document_json = json_set(document_json, '$.rules[0].priority', 0)
             WHERE revision = 1 AND source = 'migration'",
        )
        .execute(&mut connection)
        .await
        .expect("simulate legacy history variant");
        let before_repair: i64 = sqlx::query_scalar(
            "SELECT priority FROM model_mapping_rules
             WHERE id = 'legacy-model-alias-rule:636F6465782D75706772616465'",
        )
        .fetch_one(&mut connection)
        .await
        .expect("legacy priority before repair");
        assert_eq!(before_repair, 0);

        migrator()
            .run(&mut connection)
            .await
            .expect("upgrade schema 43 to latest");
        let after_repair: i64 = sqlx::query_scalar(
            "SELECT priority FROM model_mapping_rules
             WHERE id = 'legacy-model-alias-rule:636F6465782D75706772616465'",
        )
        .fetch_one(&mut connection)
        .await
        .expect("legacy priority after repair");
        assert_eq!(after_repair, 1);
        let history_priority: i64 = sqlx::query_scalar(
            "SELECT json_extract(document_json, '$.rules[0].priority')
             FROM model_mapping_document_history
             WHERE revision = 1 AND source = 'migration'",
        )
        .fetch_one(&mut connection)
        .await
        .expect("history priority");
        assert_eq!(history_priority, 1);
        let current_schema: i64 = sqlx::query_scalar(
            "SELECT schema_version FROM persistence_schema_compatibility
             WHERE singleton_key = 1",
        )
        .fetch_one(&mut connection)
        .await
        .expect("current schema");
        assert_eq!(current_schema, current_schema_version());
    }

    #[tokio::test]
    async fn current_schema_seeds_builtin_monitor_templates_idempotently() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_v2_database(&path)
            .await
            .expect("initialize database");

        let pool = migration_pool_existing(&path).await.expect("open database");
        migrator().run(&pool).await.expect("rerun migrations");
        let templates: Vec<(String, i64, i64)> = sqlx::query_as(
            r#"
            SELECT id, enabled, built_in
            FROM channel_monitor_request_templates
            WHERE id IN (
                'builtin-openai-chat-low-token',
                'builtin-openai-responses-low-token'
            )
            ORDER BY id
            "#,
        )
        .fetch_all(&pool)
        .await
        .expect("list builtin templates");
        pool.close().await;

        assert_eq!(templates.len(), 2);
        assert!(templates
            .iter()
            .all(|(_, enabled, built_in)| *enabled == 1 && *built_in == 1));
    }

    #[tokio::test]
    async fn existing_schema_is_backed_up_and_migrated_to_current() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 9).await;
        write_migration_canary(&path).await;

        let backup = upgrade_existing_v2_database(&path)
            .await
            .expect("upgrade schema")
            .expect("backup path");

        assert!(backup.is_file());
        assert_eq!(database_schema_version(&backup).await, 9);
        assert_eq!(
            database_schema_version(&path).await,
            current_schema_version()
        );
        assert_eq!(read_migration_canary(&backup).await, "preserved");
        assert_eq!(read_migration_canary(&path).await, "preserved");
        let runtime = PersistenceRuntime::open_current(&path)
            .await
            .expect("open migrated runtime");
        assert_eq!(
            runtime.health().await.expect("health").open_mode,
            "writable"
        );
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn current_schema_upgrade_is_idempotent_and_creates_no_backup() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_v2_database(&path)
            .await
            .expect("initialize database");

        assert_eq!(
            upgrade_existing_v2_database(&path)
                .await
                .expect("current schema"),
            None
        );
        assert!(!root.path().join("backups").exists());
    }

    #[tokio::test]
    async fn schema_50_materializes_v1_routing_rows_without_changing_revision() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 49).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        let v1 = r#"{
            "version":1,
            "reliability_weight":4000,
            "responsiveness_weight":2500,
            "cost_weight":2000,
            "preference_weight":1500,
            "max_candidates":32,
            "exploration_share_basis_points":500,
            "allow_depleted_fallback":false,
            "affinity_enabled":false,
            "affinity_ttl_seconds":300,
            "max_rate_multiplier":null,
            "routing_group_filter":"all_groups",
            "outbound_proxy_mode":"inherit",
            "outbound_proxy_url":null
        }"#;
        sqlx::query(
            "UPDATE routing_policy SET config_json = ?1, policy_version = 'routing-policy-v1', config_revision = 7 WHERE singleton_key = 1",
        )
        .bind(v1)
        .execute(&pool)
        .await
        .expect("legacy active row");
        sqlx::query("UPDATE domain_revisions SET revision = 7 WHERE scope = 'routing_policy'")
            .execute(&pool)
            .await
            .expect("legacy policy revision");
        sqlx::query(
            "INSERT OR REPLACE INTO routing_policy_history (config_revision, config_json, policy_version, system_version, status, created_at_ms) VALUES (7, ?1, 'routing-policy-v1', 'intelligent-routing-engine', 'active', 0)",
        )
        .bind(v1)
        .execute(&pool)
        .await
        .expect("legacy history row");

        migrator_through(50)
            .expect("schema 50 migrator")
            .run(&pool)
            .await
            .expect("schema 50 migration");
        let active: String =
            sqlx::query_scalar("SELECT config_json FROM routing_policy WHERE singleton_key = 1")
                .fetch_one(&pool)
                .await
                .expect("active policy");
        let history: String = sqlx::query_scalar(
            "SELECT config_json FROM routing_policy_history WHERE config_revision = 7",
        )
        .fetch_one(&pool)
        .await
        .expect("policy history");
        for json in [active, history] {
            let value: serde_json::Value = serde_json::from_str(&json).expect("valid V2 JSON");
            assert_eq!(value["version"], 2);
            assert_eq!(value["maxCandidates"], 32);
            assert_eq!(value["retryFailover"]["maxTotalAttempts"], 4);
            assert_eq!(value["retryFailover"]["maxSameTargetCapacityRetries"], 2);
            assert_eq!(value["retryFailover"]["capacityRetryWaitBudgetMs"], 2000);
            assert_eq!(
                value["retryFailover"]["allowCrossCapacityDomainFallback"],
                true
            );
        }
        let revision: i64 = sqlx::query_scalar(
            "SELECT config_revision FROM routing_policy WHERE singleton_key = 1",
        )
        .fetch_one(&pool)
        .await
        .expect("active revision");
        assert_eq!(revision, 7, "materialization must not create a policy edit");
        let policy_version: String =
            sqlx::query_scalar("SELECT policy_version FROM routing_policy WHERE singleton_key = 1")
                .fetch_one(&pool)
                .await
                .expect("policy version");
        assert_eq!(policy_version, "routing-policy-v2");
        pool.close().await;
    }

    #[tokio::test]
    async fn schema_50_leaves_wrong_typed_v1_rows_for_typed_recovery() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 49).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        let malformed = r#"{
            "version":1,
            "reliability_weight":4000,
            "responsiveness_weight":2500,
            "cost_weight":2000,
            "preference_weight":1500,
            "max_candidates":32,
            "exploration_share_basis_points":500,
            "allow_depleted_fallback":"false",
            "affinity_enabled":false,
            "affinity_ttl_seconds":300
        }"#;
        sqlx::query(
            "UPDATE routing_policy SET config_json = ?1, policy_version = 'routing-policy-v1' WHERE singleton_key = 1",
        )
        .bind(malformed)
        .execute(&pool)
        .await
        .expect("malformed active row");

        migrator_through(50)
            .expect("schema 50 migrator")
            .run(&pool)
            .await
            .expect("schema 50 migration");
        let config_json: String =
            sqlx::query_scalar("SELECT config_json FROM routing_policy WHERE singleton_key = 1")
                .fetch_one(&pool)
                .await
                .expect("active policy");
        assert_eq!(config_json, malformed);
        let policy_version: String =
            sqlx::query_scalar("SELECT policy_version FROM routing_policy WHERE singleton_key = 1")
                .fetch_one(&pool)
                .await
                .expect("policy version");
        assert_eq!(policy_version, "routing-policy-v1");
        pool.close().await;
    }

    #[tokio::test]
    async fn schema_52_materializes_missing_protection_profile_on_existing_v2_rows() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 51).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        let v2_without_profile = serde_json::json!({
            "version": 2,
            "reliabilityWeight": 4000,
            "responsivenessWeight": 2500,
            "costWeight": 2000,
            "preferenceWeight": 1500,
            "maxCandidates": 32,
            "explorationShareBasisPoints": 500,
            "allowDepletedFallback": false,
            "affinityEnabled": false,
            "affinityTtlSeconds": 300,
            "maxRateMultiplier": null,
            "routingGroupFilter": "all_groups",
            "outboundProxyMode": "inherit",
            "outboundProxyUrl": null,
            "retryFailover": {
                "version": 1,
                "maxTotalAttempts": 4,
                "maxSameTargetCapacityRetries": 2,
                "capacityRetryWaitBudgetMs": 2000,
                "allowCrossCapacityDomainFallback": true
            }
        });
        let v2_json = serde_json::to_string(&v2_without_profile).expect("V2 JSON");
        sqlx::query("UPDATE routing_policy SET config_json = ?1, policy_version = 'routing-policy-v2' WHERE singleton_key = 1")
            .bind(&v2_json)
            .execute(&pool)
            .await
            .expect("legacy V2 active row");
        sqlx::query(
            "INSERT OR REPLACE INTO routing_policy_history (config_revision, config_json, policy_version, system_version, status, created_at_ms) VALUES (8, ?1, 'routing-policy-v2', 'intelligent-routing-engine', 'active', 0)",
        )
        .bind(&v2_json)
        .execute(&pool)
        .await
        .expect("legacy V2 history row");
        let mut explicit_profile = v2_without_profile.clone();
        explicit_profile["protectionProfile"] = serde_json::json!({
            "version": 1,
            "enabled": true,
            "windowMaxSamples": 8,
            "windowMs": 10000,
            "minSamples": 2,
            "failureThresholdPercent": 40,
            "halfOpenSuccessesToClose": 3
        });
        sqlx::query(
            "INSERT OR REPLACE INTO routing_policy_history (config_revision, config_json, policy_version, system_version, status, created_at_ms) VALUES (9, ?1, 'routing-policy-v2', 'intelligent-routing-engine', 'active', 0)",
        )
        .bind(serde_json::to_string(&explicit_profile).expect("explicit profile JSON"))
        .execute(&pool)
        .await
        .expect("explicit V2 history row");

        migrator_through(52)
            .expect("schema 52 migrator")
            .run(&pool)
            .await
            .expect("schema 52 migration");
        for json in [
            sqlx::query_scalar::<_, String>(
                "SELECT config_json FROM routing_policy WHERE singleton_key = 1",
            )
            .fetch_one(&pool)
            .await
            .expect("active policy"),
            sqlx::query_scalar::<_, String>(
                "SELECT config_json FROM routing_policy_history WHERE config_revision = 8",
            )
            .fetch_one(&pool)
            .await
            .expect("history policy"),
        ] {
            let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
            assert_eq!(value["version"], 2);
            assert_eq!(value["protectionProfile"]["version"], 1);
            assert_eq!(value["protectionProfile"]["enabled"], false);
            assert_eq!(value["protectionProfile"]["windowMaxSamples"], 64);
            assert_eq!(value["protectionProfile"]["windowMs"], 300000);
        }
        let preserved: String = sqlx::query_scalar(
            "SELECT config_json FROM routing_policy_history WHERE config_revision = 9",
        )
        .fetch_one(&pool)
        .await
        .expect("preserved explicit profile");
        let preserved: serde_json::Value = serde_json::from_str(&preserved).expect("valid JSON");
        assert_eq!(preserved["protectionProfile"]["enabled"], true);
        assert_eq!(preserved["protectionProfile"]["windowMaxSamples"], 8);
        let revision: i64 = sqlx::query_scalar(
            "SELECT config_revision FROM routing_policy WHERE singleton_key = 1",
        )
        .fetch_one(&pool)
        .await
        .expect("active revision");
        assert_eq!(revision, 1, "materialization must not create a policy edit");
        pool.close().await;
    }

    #[tokio::test]
    async fn schema_54_converts_active_and_history_policy_durations_to_seconds() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 53).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        let current: String =
            sqlx::query_scalar("SELECT config_json FROM routing_policy WHERE singleton_key = 1")
                .fetch_one(&pool)
                .await
                .expect("current policy");
        let mut legacy: serde_json::Value =
            serde_json::from_str(&current).expect("legacy policy JSON");
        legacy["retryFailover"]["capacityRetryWaitBudgetMs"] = serde_json::json!(750);
        legacy["protectionProfile"]["windowMs"] = serde_json::json!(300_500);
        legacy["timeoutPolicy"]["connectMs"] = serde_json::json!(1_250);
        legacy["timeoutPolicy"]["firstByteMs"] = serde_json::json!(30_500);
        legacy["timeoutPolicy"]["precommitMs"] = serde_json::json!(60_250);
        legacy["timeoutPolicy"]["bufferedExecutionMs"] = serde_json::json!(300_750);
        legacy["timeoutPolicy"]["streamIdleMs"] = serde_json::json!(90_125);
        let legacy_json = serde_json::to_string(&legacy).expect("legacy policy serialization");
        sqlx::query(
            "UPDATE routing_policy SET config_json = ?1, config_revision = 17 WHERE singleton_key = 1",
        )
        .bind(&legacy_json)
        .execute(&pool)
        .await
        .expect("legacy active policy");
        sqlx::query(
            "INSERT OR REPLACE INTO routing_policy_history (config_revision, config_json, policy_version, system_version, status, created_at_ms) VALUES (18, ?1, 'routing-policy-v2', 'intelligent-routing-engine', 'active', 0)",
        )
        .bind(&legacy_json)
        .execute(&pool)
        .await
        .expect("legacy history policy");

        migrator_through(54)
            .expect("schema 54 migrator")
            .run(&pool)
            .await
            .expect("schema 54 migration");
        for json in [
            sqlx::query_scalar::<_, String>(
                "SELECT config_json FROM routing_policy WHERE singleton_key = 1",
            )
            .fetch_one(&pool)
            .await
            .expect("migrated active policy"),
            sqlx::query_scalar::<_, String>(
                "SELECT config_json FROM routing_policy_history WHERE config_revision = 18",
            )
            .fetch_one(&pool)
            .await
            .expect("migrated history policy"),
        ] {
            let value: serde_json::Value = serde_json::from_str(&json).expect("migrated JSON");
            assert_eq!(value["retryFailover"]["version"], 2);
            assert_eq!(
                value["retryFailover"]["capacityRetryWaitBudgetSeconds"],
                0.75
            );
            assert!(value["retryFailover"]["capacityRetryWaitBudgetMs"].is_null());
            assert_eq!(value["protectionProfile"]["version"], 2);
            assert_eq!(value["protectionProfile"]["windowSeconds"], 300.5);
            assert!(value["protectionProfile"]["windowMs"].is_null());
            assert_eq!(value["timeoutPolicy"]["version"], 2);
            assert_eq!(value["timeoutPolicy"]["connectSeconds"], 1.25);
            assert_eq!(value["timeoutPolicy"]["streamIdleSeconds"], 90.125);
            assert!(value["timeoutPolicy"]["connectMs"].is_null());
        }
        let revision: i64 = sqlx::query_scalar(
            "SELECT config_revision FROM routing_policy WHERE singleton_key = 1",
        )
        .fetch_one(&pool)
        .await
        .expect("active revision");
        assert_eq!(revision, 17, "unit migration must not create a policy edit");
        pool.close().await;
    }

    #[tokio::test]
    async fn group_missing_incidents_upgrade_to_one_informational_change() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 41).await;

        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        sqlx::query(
            "INSERT INTO change_incidents (
                id, condition_key, event_type, lifecycle_state, base_severity, severity,
                object_type, lifecycle_policy_fingerprint, episode_number, first_seen_at_ms,
                last_seen_at_ms, occurrence_count, episode_occurrence_count,
                last_observation_summary_json, created_at_ms, updated_at_ms
             ) VALUES (
                'legacy-group-missing', 'station_group:station-1:group-1', 'group_missing',
                'resolved', 'warning', 'warning', 'station_group_binding', 'legacy',
                1, 100, 300, 1, 1, '{\"groupName\":\"Legacy group\"}', 100, 300
             )",
        )
        .execute(&pool)
        .await
        .expect("legacy group incident");
        sqlx::query(
            "INSERT INTO incident_attention (
                incident_id, episode_number, seen_at_ms, updated_at_ms
             ) VALUES ('legacy-group-missing', 1, 250, 250)",
        )
        .execute(&pool)
        .await
        .expect("legacy attention");
        for (id, kind, observed_at_ms) in [
            ("legacy-group-missing-abnormal", "abnormal", 200_i64),
            ("legacy-group-missing-healthy", "healthy", 300_i64),
        ] {
            sqlx::query(
                "INSERT INTO change_event_occurrences (
                    id, source_observation_key, event_type, category, observation_kind,
                    severity, condition_key, incident_id, episode_number, object_type, source,
                    new_value_json, observed_at_ms, created_at_ms
                 ) VALUES (?1, ?2, 'group_missing', 'condition_observation', ?3,
                           'warning', 'station_group:station-1:group-1',
                           'legacy-group-missing', 1, 'station_group_binding', 'legacy',
                           '{\"groupName\":\"Legacy group\"}', ?4, ?4)",
            )
            .bind(id)
            .bind(format!("fixture:{id}"))
            .bind(kind)
            .bind(observed_at_ms)
            .execute(&pool)
            .await
            .expect("legacy group occurrence");
        }

        migrator_through(42)
            .expect("schema 42 migrator")
            .run(&pool)
            .await
            .expect("upgrade missing group information");

        let incident_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM change_incidents WHERE event_type = 'group_missing'",
        )
        .fetch_one(&pool)
        .await
        .expect("group incident count");
        assert_eq!(incident_count, 0);
        let information_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM change_event_occurrences
             WHERE event_type = 'group_missing'
               AND category = 'audit_change'
               AND observation_kind = 'change'
               AND severity = 'info'
               AND incident_id IS NULL",
        )
        .fetch_one(&pool)
        .await
        .expect("group information count");
        assert_eq!(information_count, 1);
        let seen_at_ms: Option<i64> = sqlx::query_scalar(
            "SELECT seen_at_ms FROM change_event_occurrences
             WHERE event_type = 'group_missing' AND category = 'audit_change'",
        )
        .fetch_one(&pool)
        .await
        .expect("migrated seen state");
        assert_eq!(seen_at_ms, Some(250));
        let recovery_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM change_event_occurrences
             WHERE event_type = 'group_missing' AND observation_kind = 'healthy'",
        )
        .fetch_one(&pool)
        .await
        .expect("group recovery count");
        assert_eq!(recovery_count, 0);
        let attention_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM incident_attention WHERE incident_id = 'legacy-group-missing'",
        )
        .fetch_one(&pool)
        .await
        .expect("legacy attention cleanup");
        assert_eq!(attention_count, 0);

        pool.close().await;
    }

    #[tokio::test]
    async fn station_published_status_migration_creates_constrained_fact_tables() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_v2_database(&path)
            .await
            .expect("initialize database");

        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        assert_eq!(
            applied_schema_version(&pool).await.expect("schema version"),
            current_schema_version()
        );
        for table in [
            "station_published_status_sources",
            "station_published_monitors",
            "station_published_monitor_samples",
        ] {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("table existence");
            assert_eq!(exists, 1, "missing {table}");
        }
        for index in [
            "idx_station_published_status_sources_station_revision",
            "idx_station_published_monitors_workspace",
            "idx_station_published_monitor_samples_timeline",
        ] {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
            )
            .bind(index)
            .fetch_one(&pool)
            .await
            .expect("index existence");
            assert_eq!(exists, 1, "missing {index}");
        }
        let interval: String = sqlx::query_scalar(
            "SELECT value FROM settings WHERE key = 'published_status_interval_minutes'",
        )
        .fetch_one(&pool)
        .await
        .expect("published status interval setting");
        assert_eq!(interval, "5");

        let missing_station = sqlx::query(
            r#"
            INSERT INTO station_published_status_sources (
                station_id, endpoint_revision, source_kind, source_state, last_attempt_at,
                monitor_count, created_at, updated_at
            ) VALUES ('missing', 1, 'fixture', 'available', '0', 0, '0', '0')
            "#,
        )
        .execute(&pool)
        .await;
        assert!(
            missing_station.is_err(),
            "source station foreign key must hold"
        );

        sqlx::query(
            r#"
            INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, created_at, updated_at
            ) VALUES (
                'published-status-station', 'Published Status Fixture', 'sub2api',
                'https://example.invalid', 'https://example.invalid/v1', '0', '0'
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("fixture station");
        sqlx::query(
            r#"
            INSERT INTO station_published_status_sources (
                station_id, endpoint_revision, source_kind, source_state, last_attempt_at,
                monitor_count, created_at, updated_at
            ) VALUES ('published-status-station', 1, 'fixture', 'available', '0', 1, '0', '0')
            "#,
        )
        .execute(&pool)
        .await
        .expect("fixture source");
        sqlx::query(
            r#"
            INSERT INTO station_published_monitors (
                id, station_id, endpoint_revision, source_kind, upstream_monitor_id,
                identity_kind, name, provider, primary_model, extra_models_json,
                presence_status, current_outcome, source_status, last_seen_run_id,
                last_seen_at, created_at, updated_at
            ) VALUES (
                'published-status-monitor', 'published-status-station', 1, 'fixture',
                'upstream-monitor', 'upstream_id', 'Fixture Monitor', 'fixture', 'fixture-model',
                '[]', 'current', 'available', 'available', 'fixture-run', '0', '0', '0'
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("fixture monitor");
        sqlx::query(
            r#"
            INSERT INTO station_published_monitor_samples (
                id, monitor_id, model, checked_at, outcome, source_status,
                first_seen_run_id, last_seen_run_id, created_at, updated_at
            ) VALUES (
                'published-status-sample', 'published-status-monitor', 'fixture-model', '0',
                'available', 'available', 'fixture-run', 'fixture-run', '0', '0'
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("fixture sample");
        let duplicate = sqlx::query(
            r#"
            INSERT INTO station_published_monitor_samples (
                id, monitor_id, model, checked_at, outcome, source_status,
                first_seen_run_id, last_seen_run_id, created_at, updated_at
            ) VALUES (
                'duplicate-published-status-sample', 'published-status-monitor', 'fixture-model',
                '0', 'available', 'available', 'fixture-run', 'fixture-run', '0', '0'
            )
            "#,
        )
        .execute(&pool)
        .await;
        assert!(duplicate.is_err(), "sample identity must be unique");

        sqlx::query("DELETE FROM stations WHERE id = 'published-status-station'")
            .execute(&pool)
            .await
            .expect("delete fixture station");
        let remaining: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM station_published_monitor_samples WHERE monitor_id = 'published-status-monitor'",
        )
        .fetch_one(&pool)
        .await
        .expect("remaining samples");
        assert_eq!(remaining, 0, "station delete must cascade published facts");
        pool.close().await;
    }

    #[tokio::test]
    async fn remote_key_upgrade_repairs_duplicate_local_owners_before_enforcing_uniqueness() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 10).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        sqlx::query(
            "INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, created_at, updated_at
             ) VALUES ('station-1', 'Station', 'sub2api', 'https://example.test',
                       'https://example.test/v1', '1', '1')",
        )
        .execute(&pool)
        .await
        .expect("insert station");
        sqlx::query(
            "INSERT INTO station_keys (id, station_id, note)
             VALUES ('local-1', 'station-1', '由远端发现开关自动创建：remote-owned')",
        )
        .execute(&pool)
        .await
        .expect("insert station key");
        for (id, confidence, collected_at) in
            [("remote-other", 1.0, "3"), ("remote-owned", 0.9, "2")]
        {
            sqlx::query(
                "INSERT INTO remote_station_keys (
                    id, station_id, remote_key_id_hash, raw_source, match_status,
                    matched_station_key_id, match_confidence, collected_at, updated_at
                 ) VALUES (?1, 'station-1', ?1, 'fixture', 'matched',
                           'local-1', ?2, ?3, ?3)",
            )
            .bind(id)
            .bind(confidence)
            .bind(collected_at)
            .execute(&pool)
            .await
            .expect("insert duplicate remote match");
        }
        pool.close().await;

        upgrade_existing_v2_database(&path)
            .await
            .expect("upgrade schema");

        let pool = migration_pool_existing(&path).await.expect("upgraded pool");
        let owner: String = sqlx::query_scalar(
            "SELECT id FROM remote_station_keys WHERE matched_station_key_id = 'local-1'",
        )
        .fetch_one(&pool)
        .await
        .expect("one local owner");
        assert_eq!(owner, "remote-owned");
        let repaired: (String, Option<String>, f64) = sqlx::query_as(
            "SELECT match_status, matched_station_key_id, match_confidence
             FROM remote_station_keys WHERE id = 'remote-other'",
        )
        .fetch_one(&pool)
        .await
        .expect("repaired duplicate");
        assert_eq!(repaired, ("unbound".to_string(), None, 0.0));
        let duplicate = sqlx::query(
            "UPDATE remote_station_keys
             SET matched_station_key_id = 'local-1', match_status = 'matched'
             WHERE id = 'remote-other'",
        )
        .execute(&pool)
        .await;
        assert!(
            duplicate.is_err(),
            "schema must reject a second local owner"
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn profile_v2_upgrade_only_updates_outdated_builtin_cli_definitions() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 13).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        sqlx::query(
            "INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, created_at, updated_at
             ) VALUES (
                'profile-station', 'Profile Station', 'openai_compatible',
                'https://example.test', 'https://example.test/v1', '1', '1'
             )",
        )
        .execute(&pool)
        .await
        .expect("insert station");

        for (id, profile_id, profile_version) in [
            ("standard-v1", "standard_api", 1_i64),
            ("codex-v1", "codex_cli_compat", 1),
            ("claude-v1", "claude_code_compat", 1),
            ("gemini-v1", "gemini_cli_compat", 1),
            ("grok-v1", "grok_cli_compat", 1),
            ("codex-future", "codex_cli_compat", 3),
        ] {
            sqlx::query(
                "INSERT INTO channel_monitors (
                    id, name, target_type, station_id, template_id,
                    interval_seconds, timeout_seconds, created_at, updated_at,
                    client_profile_id, client_profile_version
                 ) VALUES (
                    ?1, ?1, 'station', 'profile-station',
                    'builtin-openai-chat-low-token', 60, 30, '1', '1', ?2, ?3
                 )",
            )
            .bind(id)
            .bind(profile_id)
            .bind(profile_version)
            .execute(&pool)
            .await
            .expect("insert monitor definition");
        }
        pool.close().await;

        upgrade_existing_v2_database(&path)
            .await
            .expect("upgrade schema");

        let pool = migration_pool_existing(&path).await.expect("upgraded pool");
        let versions: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT id, client_profile_version, schedule_revision
             FROM channel_monitors
             ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .expect("read migrated profiles");
        pool.close().await;

        assert_eq!(
            versions,
            vec![
                ("claude-v1".to_string(), 2, 2),
                ("codex-future".to_string(), 3, 1),
                ("codex-v1".to_string(), 2, 2),
                ("gemini-v1".to_string(), 2, 2),
                ("grok-v1".to_string(), 1, 1),
                ("standard-v1".to_string(), 1, 1),
            ]
        );
    }

    #[tokio::test]
    async fn schema_21_quarantines_removed_collector_providers_without_deleting_assets() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 20).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");

        for (id, station_type) in [
            ("legacy-openai", "openai-compatible"),
            ("legacy-custom", "custom"),
            ("newapi-station", "newapi"),
            ("sub2api-station", "sub2api"),
        ] {
            sqlx::query(
                "INSERT INTO stations (
                    id, name, station_type, website_url, api_base_url,
                    enabled, status, created_at, updated_at
                 ) VALUES (?1, ?1, ?2, 'https://example.test',
                           'https://example.test/v1', 1, 'healthy', '1', '1')",
            )
            .bind(id)
            .bind(station_type)
            .execute(&pool)
            .await
            .expect("insert station");
            sqlx::query(
                "INSERT INTO station_keys (id, station_id, created_at, updated_at)
                 VALUES (?1 || '-key', ?1, '1', '1')",
            )
            .bind(id)
            .execute(&pool)
            .await
            .expect("insert station key");
            sqlx::query(
                "INSERT INTO collector_runs (
                    id, run_key, request_hash, station_id, endpoint_revision,
                    adapter, task_type, status, started_at, created_at
                 ) VALUES (?1 || '-run', ?1 || '-run-key', 'hash', ?1, 1,
                           ?2, 'models', 'success', '1', '1')",
            )
            .bind(id)
            .bind(station_type)
            .execute(&pool)
            .await
            .expect("insert collector run");
            sqlx::query(
                "INSERT INTO collector_task_state (
                    station_id, task_type, last_run_id, last_status, updated_at
                 ) VALUES (?1, 'models', ?1 || '-run', 'success', '1')",
            )
            .bind(id)
            .execute(&pool)
            .await
            .expect("insert task state");
            sqlx::query(
                "INSERT INTO collector_model_facts (
                    station_id, model, available, source, confidence, last_seen_run_id, updated_at
                 ) VALUES (?1, 'fixture-model', 1, 'fixture', 1.0, ?1 || '-run', '1')",
            )
            .bind(id)
            .execute(&pool)
            .await
            .expect("insert model fact");
            sqlx::query(
                "INSERT INTO change_events (
                    id, severity, event_type, status, title, message, object_type,
                    object_id, station_id, dedupe_key, source, detected_at, created_at, updated_at
                 ) VALUES (
                    ?1 || '-model-event', 'info', 'model_added', 'unread',
                    'Model added', 'fixture', 'model', 'fixture-model', ?1,
                    ?1 || '-model-event', 'collector', '1', '1', '1'
                 )",
            )
            .bind(id)
            .execute(&pool)
            .await
            .expect("insert change event");
        }
        migrator_through(21)
            .expect("schema 21 migrator")
            .run(&pool)
            .await
            .expect("apply schema 21 quarantine");
        let quarantined_model_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM change_events WHERE event_type IN ('model_added', 'model_removed')",
        )
        .fetch_one(&pool)
        .await
        .expect("schema 21 model events");
        assert_eq!(quarantined_model_events, 0);
        pool.close().await;

        upgrade_existing_v2_database(&path)
            .await
            .expect("upgrade schema");

        let pool = migration_pool_existing(&path).await.expect("upgraded pool");
        let disabled_legacy_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM stations
             WHERE station_type IN ('openai-compatible', 'openai_compatible', 'custom')
               AND enabled = 0
               AND status = 'disabled'",
        )
        .fetch_one(&pool)
        .await
        .expect("disabled legacy count");
        assert_eq!(disabled_legacy_count, 2);
        let station_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stations")
            .fetch_one(&pool)
            .await
            .expect("station count");
        let station_key_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM station_keys")
            .fetch_one(&pool)
            .await
            .expect("station key count");
        assert_eq!(station_count, 4);
        assert_eq!(station_key_count, 4);
        let remaining_model_facts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM collector_model_facts
             WHERE station_id IN ('newapi-station', 'legacy-openai', 'legacy-custom')",
        )
        .fetch_one(&pool)
        .await
        .expect("remaining model facts");
        assert_eq!(remaining_model_facts, 0);
        let sub2api_model_facts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM collector_model_facts WHERE station_id = 'sub2api-station'",
        )
        .fetch_one(&pool)
        .await
        .expect("sub2api model facts");
        assert_eq!(sub2api_model_facts, 1);
        let removed_model_task_state: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM collector_task_state WHERE task_type = 'models'",
        )
        .fetch_one(&pool)
        .await
        .expect("models task state");
        assert_eq!(removed_model_task_state, 0);
        let legacy_change_events_table: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'change_events'",
        )
        .fetch_one(&pool)
        .await
        .expect("legacy change events table postcondition");
        assert_eq!(legacy_change_events_table, 0);
        let model_setting_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM settings WHERE key = 'model_list_interval_minutes'",
        )
        .fetch_one(&pool)
        .await
        .expect("model setting");
        assert_eq!(model_setting_count, 0);
        pool.close().await;

        assert_eq!(
            database_schema_version(&path).await,
            current_schema_version()
        );
    }

    async fn initialize_database_through(path: &Path, target_version: i64) {
        let pool = migration_pool_create(path).await.expect("migration pool");
        let partial = Migrator {
            migrations: Cow::Owned(
                migrator()
                    .iter()
                    .filter(|migration| migration.version <= target_version)
                    .cloned()
                    .collect(),
            ),
            ignore_missing: false,
            locking: true,
            no_tx: false,
        };
        partial.run(&pool).await.expect("partial migrations");
        pool.close().await;
    }

    async fn database_schema_version(path: &Path) -> i64 {
        let pool = migration_pool_existing(path).await.expect("open database");
        let row = sqlx::query(
            "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
        )
        .fetch_one(&pool)
        .await
        .expect("schema version");
        let version = row.get("schema_version");
        pool.close().await;
        version
    }

    async fn write_migration_canary(path: &Path) {
        let pool = migration_pool_existing(path).await.expect("open database");
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('schema-upgrade-canary', 'preserved', '1')",
        )
        .execute(&pool)
        .await
        .expect("write canary");
        pool.close().await;
    }

    async fn read_migration_canary(path: &Path) -> String {
        let pool = migration_pool_existing(path).await.expect("open database");
        let row = sqlx::query("SELECT value FROM settings WHERE key = 'schema-upgrade-canary'")
            .fetch_one(&pool)
            .await
            .expect("read canary");
        let value = row.get("value");
        pool.close().await;
        value
    }
}
