use std::collections::BTreeMap;

use serde::Deserialize;

use crate::persistence::runtime::PersistenceHandle;

use super::UpgradeError;

#[derive(Debug, Deserialize)]
pub(crate) struct ExpectedImportManifest {
    pub(crate) profile: String,
    pub(crate) table_counts: BTreeMap<String, i64>,
}

pub(crate) async fn validate_import(
    target: &PersistenceHandle,
    expected: &ExpectedImportManifest,
) -> Result<(), UpgradeError> {
    if expected.profile.trim().is_empty() {
        return Err(UpgradeError::ValidationFailed);
    }
    let mut read = target.begin_read().await?;
    for (table, expected_count) in &expected.table_counts {
        if !VALIDATED_TABLES.contains(&table.as_str()) || *expected_count < 0 {
            return Err(UpgradeError::ValidationFailed);
        }
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let actual: i64 = sqlx::query_scalar(&sql)
            .fetch_one(read.connection())
            .await?;
        if actual != *expected_count {
            return Err(UpgradeError::ValidationFailed);
        }
    }
    Ok(())
}

const VALIDATED_TABLES: &[&str] = &[
    "settings",
    "secrets",
    "stations",
    "station_credentials",
    "station_keys",
    "remote_station_keys",
    "station_key_capabilities",
    "model_aliases",
    "collector_runs",
    "collector_snapshots",
    "station_group_bindings",
    "group_rate_records",
    "pricing_rules",
    "model_base_prices",
    "balance_snapshots",
    "channel_monitor_request_templates",
    "channel_monitors",
    "channel_monitor_runs",
    "request_logs",
    "station_key_health",
    "station_endpoint_health",
    "change_events",
];
