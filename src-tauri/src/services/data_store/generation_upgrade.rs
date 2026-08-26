//! Generation-2 data-store preparation.
//!
//! This module owns the Gen2 create/open/validate boundary; ordinary schema work
//! is planned by `startup_upgrade_plan` and executed by
//! `startup_upgrade_executor`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::{SecondsFormat, Utc};

use crate::{
    persistence::{self, runtime::PersistenceRuntime},
    services::{
        data_store::{
            alerting_upgrade::ALERTING_FOUNDATION_SCHEMA_VERSION,
            config::{
                create_installation_marker, read_config_v3, write_config_v3_with_faults,
                DataDirConfigV3, DatabaseGeneration,
            },
            startup_upgrade_executor::execute_startup_upgrade_plan,
            startup_upgrade_plan::StartupUpgradeStep,
            types::{RecoveryReason, StartupUpgradeError},
        },
        secrets::{
            baseline_conversion::{
                initialize_fresh_database_at_baseline, ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION,
            },
            validation::validate_database_secrets,
            DeviceKeyResolver,
        },
    },
};

use super::atomic_file::{sync_file, sync_parent};

const DATA_DIR_CONFIG_FILE: &str = "relay-pool-data-dir.json";

pub(crate) async fn initialize_empty_generation_two(path: &Path) -> Result<(), String> {
    let runtime = PersistenceRuntime::initialize_new(path)
        .await
        .map_err(|error| format!("failed to initialize generation 2 data store: {error}"))?;
    runtime
        .close()
        .await
        .map_err(|error| format!("failed to close generation 2 data store: {error}"))
}

pub(crate) fn prepare_generation_two_with_resolver(
    default_data_dir: &Path,
    active_data_dir: &Path,
    selected_database_path: Option<&Path>,
    planned_existing_v2_steps: Option<&[StartupUpgradeStep]>,
    device_keys: &DeviceKeyResolver,
) -> Result<(PersistenceRuntime, PathBuf), StartupUpgradeError> {
    let final_path = active_data_dir.join(DatabaseGeneration::Two.database_file());

    if let Some(selected_path) = selected_database_path {
        if selected_path != final_path {
            return Err(StartupUpgradeError::new(
                RecoveryReason::InconsistentSchemaMetadata,
                "selected data store is not a generation 2 database",
            ));
        }
        let steps = planned_existing_v2_steps.ok_or_else(|| {
            StartupUpgradeError::new(
                RecoveryReason::InternalUpgradeError,
                "existing generation 2 startup requires a preplanned upgrade plan",
            )
        })?;
        let runtime = open_and_validate_v2(default_data_dir, &final_path, device_keys, steps)?;
        return Ok((runtime, final_path));
    }

    if final_path.exists() {
        return Err(StartupUpgradeError::new(
            RecoveryReason::InterruptedUpgrade,
            "generation 2 database exists without a selected active candidate",
        ));
    }

    prepare_fresh_install(default_data_dir, active_data_dir, &final_path, device_keys)
}

fn prepare_fresh_install(
    default_data_dir: &Path,
    active_data_dir: &Path,
    final_path: &Path,
    device_keys: &DeviceKeyResolver,
) -> Result<(PersistenceRuntime, PathBuf), StartupUpgradeError> {
    let staging_path = final_path.with_extension("sqlite3.first-run.tmp");
    remove_database_artifacts(&staging_path)?;
    initialize_fresh_database_at_baseline(&staging_path, device_keys).map_err(|error| {
        StartupUpgradeError::new(
            RecoveryReason::SchemaMigrationFailed,
            format!("failed to initialize encrypted baseline database: {error}"),
        )
    })?;
    device_keys
        .with_active_key(|key| validate_v2_artifact(&staging_path, *key))
        .map_err(|error| error.to_string())??;

    sync_file(&staging_path).map_err(|error| {
        StartupUpgradeError::new(
            RecoveryReason::InternalUpgradeError,
            format!("failed to durably sync generation 2 staging database: {error}"),
        )
    })?;
    fs::rename(&staging_path, final_path).map_err(|error| {
        StartupUpgradeError::new(
            RecoveryReason::InternalUpgradeError,
            format!("failed to publish generation 2 database: {error}"),
        )
    })?;

    sync_parent(active_data_dir).map_err(|error| {
        StartupUpgradeError::new(
            RecoveryReason::InternalUpgradeError,
            format!("failed to durably publish generation 2 database: {error}"),
        )
    })?;
    device_keys
        .with_active_key(|key| validate_v2_artifact(final_path, *key))
        .map_err(|error| error.to_string())??;
    commit_generation_two_config(default_data_dir, active_data_dir)?;
    let steps = encrypted_baseline_ready_startup_steps();
    let runtime = open_and_validate_v2(default_data_dir, final_path, device_keys, &steps)?;
    Ok((runtime, final_path.to_path_buf()))
}

fn encrypted_baseline_ready_startup_steps() -> Vec<StartupUpgradeStep> {
    let mut steps = Vec::new();
    let latest_schema = persistence::current_schema_version();
    if latest_schema > ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION {
        steps.push(StartupUpgradeStep::EnsureSchema {
            target_schema: latest_schema.min(ALERTING_FOUNDATION_SCHEMA_VERSION),
        });
    }
    if latest_schema >= ALERTING_FOUNDATION_SCHEMA_VERSION {
        steps.push(StartupUpgradeStep::EnsureAlertingUpgrade);
        if latest_schema > ALERTING_FOUNDATION_SCHEMA_VERSION {
            steps.push(StartupUpgradeStep::EnsureLegacyChangeEventsRemoval);
        }
    }
    steps.extend([
        StartupUpgradeStep::OpenRuntime,
        StartupUpgradeStep::VerifyWritableRuntime,
        StartupUpgradeStep::VerifySecrets,
    ]);
    steps
}

fn validate_v2_artifact(path: &Path, data_key: [u8; 32]) -> Result<(), String> {
    tauri::async_runtime::block_on(persistence::validate_read_only_sqlite(path))
        .map_err(|error| format!("generation 2 database validation failed: {error}"))?;
    validate_database_secrets(path, &data_key)
}

fn open_and_validate_v2(
    default_data_dir: &Path,
    final_path: &Path,
    device_keys: &DeviceKeyResolver,
    steps: &[StartupUpgradeStep],
) -> Result<PersistenceRuntime, StartupUpgradeError> {
    execute_startup_upgrade_plan(
        default_data_dir,
        final_path,
        device_keys,
        &crate::persistence::upgrade_fault::NoUpgradeFaults,
        steps,
    )
}

fn commit_generation_two_config(
    default_data_dir: &Path,
    active_data_dir: &Path,
) -> Result<(), StartupUpgradeError> {
    let config_path = default_data_dir.join(DATA_DIR_CONFIG_FILE);
    let previous = read_config_v3(&config_path).map_err(|error| {
        StartupUpgradeError::new(
            RecoveryReason::InternalUpgradeError,
            format!("failed to read data directory config: {error}"),
        )
    })?;
    write_config_v3_with_faults(
        &config_path,
        &DataDirConfigV3 {
            version: 3,
            active_data_dir: Some(active_data_dir.to_path_buf()),
            pending_data_dir: previous
                .as_ref()
                .and_then(|config| config.pending_data_dir.clone()),
            source_data_dir: previous
                .as_ref()
                .and_then(|config| config.source_data_dir.clone()),
            database_generation: DatabaseGeneration::Two,
            updated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        },
        &crate::persistence::upgrade_fault::NoUpgradeFaults,
    )
    .map_err(|error| {
        StartupUpgradeError::new(
            RecoveryReason::InternalUpgradeError,
            format!("failed to commit generation 2 data directory config: {error}"),
        )
    })?;
    create_installation_marker(default_data_dir).map_err(|error| {
        StartupUpgradeError::new(
            RecoveryReason::InternalUpgradeError,
            format!("failed to commit installation marker: {error}"),
        )
    })
}

fn remove_database_artifacts(path: &Path) -> Result<(), StartupUpgradeError> {
    for artifact in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ] {
        if artifact.is_file() {
            fs::remove_file(&artifact).map_err(|error| {
                StartupUpgradeError::new(
                    RecoveryReason::InternalUpgradeError,
                    format!("failed to remove owned generation 2 staging artifact: {error}"),
                )
            })?;
        }
    }
    Ok(())
}
