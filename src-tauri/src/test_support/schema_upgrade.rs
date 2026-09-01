//! Debug-only integration seam for exercising the production startup upgrade route.
//!
//! This module intentionally exposes only bounded, non-sensitive facts. The
//! integration tests still go through the real read-only probe, pure planner,
//! ordered executor, runtime health check, and shutdown path.

use std::path::Path;

use crate::{
    persistence::{self, upgrade_fault::NoUpgradeFaults},
    services::{
        data_store::startup_upgrade_executor::execute_startup_upgrade_plan,
        secrets::{DeviceKeyId, DeviceKeyResolver, SecretKeyMaterial},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaUpgradeHarnessResult {
    pub schema_version: i64,
    pub open_mode: String,
    pub plan_step_count: usize,
    pub restart_ready: bool,
}

/// Execute one complete existing-database startup upgrade using production
/// probe/plan/executor code, then close the runtime cleanly.
pub fn run_schema_upgrade(
    default_data_dir: &Path,
    database_path: &Path,
    key_id: &str,
    key_bytes: [u8; 32],
) -> Result<SchemaUpgradeHarnessResult, String> {
    let resolver = DeviceKeyResolver::active(
        DeviceKeyId::new(key_id),
        SecretKeyMaterial::from_bytes(key_bytes),
        crate::services::secrets::CURRENT_SECRET_ENCRYPTION_VERSION,
    );
    let journal_path = default_data_dir
        .join(persistence::baseline_conversion_support::BASELINE_CONVERSION_JOURNAL_FILE);
    let plan = crate::plan_schema_upgrade_for_test(
        database_path,
        Some(&journal_path),
        Some(resolver.active_key_id().as_str()),
    )?;
    let plan_step_count = plan.len();
    let runtime = execute_startup_upgrade_plan(
        default_data_dir,
        database_path,
        &resolver,
        &NoUpgradeFaults,
        &plan,
    )
    .map_err(|error| error.to_string())?;
    let health =
        tauri::async_runtime::block_on(runtime.health()).map_err(|error| error.to_string())?;
    let schema_version = health.schema_version;
    let open_mode = health.open_mode;
    tauri::async_runtime::block_on(runtime.close()).map_err(|error| error.to_string())?;

    // Re-probe after closing the runtime to make the result useful as a
    // restart-readiness signal rather than reporting a constant success bit.
    // The next startup must still produce an executable plan that reaches
    // OpenRuntime; a stale journal or incomplete maintenance therefore fails
    // this harness even when the first runtime happened to be writable.
    let restart_ready = crate::schema_upgrade_restart_ready_for_test(
        database_path,
        Some(&journal_path),
        Some(resolver.active_key_id().as_str()),
    )
    .unwrap_or(false);
    if !restart_ready {
        return Err("schema upgrade did not leave a restart-ready database".to_string());
    }

    let result = SchemaUpgradeHarnessResult {
        schema_version,
        open_mode,
        plan_step_count,
        restart_ready,
    };
    Ok(result)
}
