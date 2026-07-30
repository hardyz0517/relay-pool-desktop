use std::path::Path;

use sqlx::{Connection, Row};

use super::{
    catalog::{migration_data_catalog, setting_policy},
    validate::{open_read_only_sqlite, quote_identifier, PortableMigrationValidationError},
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum RestoreTargetOccupancyError {
    #[error("portable migration restore target is not empty")]
    NotEmpty,
    #[error("portable migration restore target validation failed")]
    Validation(#[from] PortableMigrationValidationError),
}

pub(crate) async fn ensure_restore_target_is_empty(
    active_database_path: &Path,
) -> Result<(), RestoreTargetOccupancyError> {
    let mut connection = open_read_only_sqlite(active_database_path).await?;

    for table in migration_data_catalog()
        .iter()
        .filter(|table| table.counts_for_occupancy)
    {
        match table.name {
            "settings" => ensure_known_settings_only(&mut connection).await?,
            "secrets" => ensure_no_non_device_secret(&mut connection).await?,
            "app_secret_bindings" => ensure_no_non_device_secret_binding(&mut connection).await?,
            "channel_monitor_request_templates" => {
                ensure_no_custom_monitor_template(&mut connection).await?
            }
            table_name => ensure_table_has_no_rows(&mut connection, table_name).await?,
        }
    }

    connection
        .close()
        .await
        .map_err(|_| PortableMigrationValidationError::Sql)?;
    Ok(())
}

async fn ensure_known_settings_only(
    connection: &mut sqlx::SqliteConnection,
) -> Result<(), RestoreTargetOccupancyError> {
    let rows = sqlx::query("SELECT key FROM settings")
        .fetch_all(&mut *connection)
        .await
        .map_err(|_| PortableMigrationValidationError::Sql)?;
    for row in rows {
        let key: String = row.get("key");
        if setting_policy(&key).is_none() {
            return Err(RestoreTargetOccupancyError::NotEmpty);
        }
    }
    Ok(())
}

async fn ensure_no_non_device_secret(
    connection: &mut sqlx::SqliteConnection,
) -> Result<(), RestoreTargetOccupancyError> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM secrets
        WHERE NOT (scope = 'settings' AND owner_id = 'local_key' AND kind = 'local_access_key')
        "#,
    )
    .fetch_one(&mut *connection)
    .await
    .map_err(|_| PortableMigrationValidationError::Sql)?;
    if count == 0 {
        Ok(())
    } else {
        Err(RestoreTargetOccupancyError::NotEmpty)
    }
}

async fn ensure_no_non_device_secret_binding(
    connection: &mut sqlx::SqliteConnection,
) -> Result<(), RestoreTargetOccupancyError> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM app_secret_bindings
        WHERE NOT (
            binding_scope = 'settings'
            AND binding_owner_id = 'local_key'
            AND binding_kind = 'local_access_key'
        )
        "#,
    )
    .fetch_one(&mut *connection)
    .await
    .map_err(|_| PortableMigrationValidationError::Sql)?;
    if count == 0 {
        Ok(())
    } else {
        Err(RestoreTargetOccupancyError::NotEmpty)
    }
}

async fn ensure_no_custom_monitor_template(
    connection: &mut sqlx::SqliteConnection,
) -> Result<(), RestoreTargetOccupancyError> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM channel_monitor_request_templates
        WHERE built_in != 1
        "#,
    )
    .fetch_one(&mut *connection)
    .await
    .map_err(|_| PortableMigrationValidationError::Sql)?;
    if count == 0 {
        Ok(())
    } else {
        Err(RestoreTargetOccupancyError::NotEmpty)
    }
}

async fn ensure_table_has_no_rows(
    connection: &mut sqlx::SqliteConnection,
    table_name: &str,
) -> Result<(), RestoreTargetOccupancyError> {
    if occupancy_table_count(connection, table_name).await? == 0 {
        Ok(())
    } else {
        Err(RestoreTargetOccupancyError::NotEmpty)
    }
}

async fn occupancy_table_count(
    connection: &mut sqlx::SqliteConnection,
    table_name: &str,
) -> Result<usize, RestoreTargetOccupancyError> {
    let sql = format!("SELECT COUNT(*) FROM {}", quote_identifier(table_name)?);
    let count: i64 = sqlx::query_scalar(&sql)
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| PortableMigrationValidationError::Sql)?;
    usize::try_from(count)
        .map_err(|_| PortableMigrationValidationError::UnsupportedSchema)
        .map_err(RestoreTargetOccupancyError::from)
}
