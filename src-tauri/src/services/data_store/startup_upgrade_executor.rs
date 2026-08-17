use std::{collections::BTreeSet, path::Path};

use chrono::Utc;

use crate::{
    persistence::{
        self,
        runtime::PersistenceRuntime,
        upgrade_fault::{UpgradeFailpoint, UpgradeFaultInjector},
    },
    services::{
        data_store::{
            alerting_upgrade,
            startup_upgrade_plan::StartupUpgradeStep,
            types::{RecoveryReason, StartupUpgradeError},
        },
        secrets::{
            baseline_conversion::{
                ensure_active_database_baseline, record_successful_startup_metadata,
                PRE_BASELINE_SCHEMA_VERSION,
            },
            validation::{validate_database_secrets_typed, SecretValidationError},
            DeviceKeyResolver,
        },
    },
};

pub(crate) fn execute_startup_upgrade_plan(
    default_data_dir: &Path,
    final_path: &Path,
    device_keys: &DeviceKeyResolver,
    faults: &dyn UpgradeFaultInjector,
    steps: &[StartupUpgradeStep],
) -> Result<PersistenceRuntime, StartupUpgradeError> {
    validate_startup_upgrade_steps(steps)?;

    check(faults, UpgradeFailpoint::V2Reopen).map_err(|error| {
        StartupUpgradeError::new(
            RecoveryReason::InternalUpgradeError,
            format!("generation 2 reopen preflight failed: {error}"),
        )
    })?;

    let mut runtime = None;
    for step in steps {
        match step {
            StartupUpgradeStep::EnsureStructuralPreBaseline => {
                block_on(persistence::upgrade_existing_v2_database_to_schema(
                    final_path,
                    PRE_BASELINE_SCHEMA_VERSION,
                ))
                .map_err(|error| {
                    StartupUpgradeError::new(
                        RecoveryReason::SchemaMigrationFailed,
                        format!(
                            "failed to migrate generation 2 database to structural baseline: {error}"
                        ),
                    )
                })?;
            }
            StartupUpgradeStep::EnsureSecretBaseline => {
                let journal_existed = default_data_dir
                    .join(persistence::upgrade_recovery_executor::UPGRADE_JOURNAL_FILE)
                    .exists();
                ensure_active_database_baseline(default_data_dir, final_path, device_keys)
                    .map_err(|error| {
                        StartupUpgradeError::new(
                            if journal_existed {
                                RecoveryReason::InterruptedUpgrade
                            } else {
                                RecoveryReason::SecretBaselineFailed
                            },
                            format!("encrypted-secret baseline conversion failed: {error}"),
                        )
                    })?;
            }
            StartupUpgradeStep::EnsureSchema { target_schema } => {
                block_on(persistence::upgrade_existing_v2_database_to_schema(
                    final_path,
                    *target_schema,
                ))
                .map_err(|error| {
                        StartupUpgradeError::new(
                            RecoveryReason::SchemaMigrationFailed,
                            format!(
                                "failed to migrate generation 2 database to schema {target_schema}: {error}"
                            ),
                        )
                    })?;
            }
            StartupUpgradeStep::EnsureAlertingUpgrade => {
                // Schema 29 is writable only inside this bounded upgrade
                // window. The normal runtime remains writable solely at the
                // latest schema (30), so opening it with current compatibility
                // would downgrade to inspection-only and make the durable
                // backfill fail.
                let mut upgrade_binary = persistence::migrations::current_binary_compatibility();
                upgrade_binary.writable_schema =
                    BTreeSet::from([alerting_upgrade::ALERTING_FOUNDATION_SCHEMA_VERSION]);
                let upgrade_runtime = block_on(PersistenceRuntime::open_for_schema_upgrade(
                    final_path,
                    upgrade_binary,
                ))
                .map_err(|error| {
                    StartupUpgradeError::new(
                        RecoveryReason::AlertingUpgradeFailed,
                        format!("failed to open alerting upgrade runtime: {error}"),
                    )
                })?;
                let result = block_on(alerting_upgrade::run_durable_upgrade(
                    &upgrade_runtime.handle(),
                    Utc::now().timestamp_millis().max(0),
                ))
                .map_err(|error| {
                    StartupUpgradeError::new(
                        RecoveryReason::AlertingUpgradeFailed,
                        format!(
                            "durable alerting upgrade failed ({}): {error}",
                            error.code()
                        ),
                    )
                });
                let close_result = block_on(upgrade_runtime.close()).map_err(|error| {
                    StartupUpgradeError::new(
                        RecoveryReason::AlertingUpgradeFailed,
                        format!("failed to close alerting upgrade runtime: {error}"),
                    )
                });
                if let Err(error) = result {
                    let _ = close_result;
                    return Err(error);
                }
                close_result?;
            }
            StartupUpgradeStep::EnsureLegacyChangeEventsRemoval => {
                block_on(persistence::upgrade_existing_v2_database(final_path)).map_err(
                    |error| {
                        StartupUpgradeError::new(
                            RecoveryReason::SchemaMigrationFailed,
                            format!(
                                "failed to apply destructive legacy change-events migration: {error}"
                            ),
                        )
                    },
                )?;
            }
            StartupUpgradeStep::OpenRuntime => {
                runtime = Some(
                    block_on(PersistenceRuntime::open_current(final_path)).map_err(|error| {
                        StartupUpgradeError::new(
                            RecoveryReason::CorruptedDatabase,
                            format!("failed to open generation 2 database: {error}"),
                        )
                    })?,
                );
            }
            StartupUpgradeStep::VerifyWritableRuntime => {
                let runtime = runtime.as_ref().ok_or_else(|| {
                    StartupUpgradeError::new(
                        RecoveryReason::InternalUpgradeError,
                        "startup plan tried to verify runtime before opening it",
                    )
                })?;
                let health = block_on(runtime.health()).map_err(|error| {
                    StartupUpgradeError::new(
                        RecoveryReason::CorruptedDatabase,
                        format!("generation 2 health check failed: {error}"),
                    )
                })?;
                if health.open_mode != "writable" {
                    return Err(StartupUpgradeError::new(
                        RecoveryReason::InconsistentSchemaMetadata,
                        "generation 2 database did not reach writable mode after planned upgrades",
                    ));
                }
            }
            StartupUpgradeStep::VerifySecrets => {
                device_keys
                    .with_active_key(|key| {
                        validate_v2_artifact_for_startup(final_path, *key, faults)
                    })
                    .map_err(|error| {
                        StartupUpgradeError::new(
                            RecoveryReason::MissingKey,
                            format!(
                                "failed to access active key during startup validation: {error}"
                            ),
                        )
                    })??;
            }
        }
    }
    record_successful_startup_metadata(final_path, device_keys).map_err(|error| {
        StartupUpgradeError::new(
            RecoveryReason::InternalUpgradeError,
            format!("failed to record startup metadata: {error}"),
        )
    })?;
    runtime.ok_or_else(|| {
        StartupUpgradeError::new(
            RecoveryReason::InternalUpgradeError,
            "startup plan completed without opening a runtime",
        )
    })
}

fn validate_startup_upgrade_steps(steps: &[StartupUpgradeStep]) -> Result<(), StartupUpgradeError> {
    if steps.is_empty() {
        return Err(invalid_step_contract(
            "startup upgrade executor received an empty plan",
        ));
    }

    let mut opened_runtime = false;
    let mut verified_writable = false;
    let mut verified_secrets = false;
    let mut alerting_upgrade_seen = false;
    let mut legacy_removal_seen = false;
    for step in steps {
        match step {
            StartupUpgradeStep::EnsureStructuralPreBaseline
            | StartupUpgradeStep::EnsureSecretBaseline
            | StartupUpgradeStep::EnsureSchema { .. }
            | StartupUpgradeStep::EnsureAlertingUpgrade => {
                if opened_runtime {
                    return Err(invalid_step_contract(
                        "startup upgrade plan tried to run migration steps after opening runtime",
                    ));
                }
                if matches!(step, StartupUpgradeStep::EnsureSchema { .. }) && alerting_upgrade_seen
                {
                    return Err(invalid_step_contract(
                        "startup upgrade plan tried to migrate schema after alerting upgrade",
                    ));
                }
                if matches!(step, StartupUpgradeStep::EnsureAlertingUpgrade) {
                    if alerting_upgrade_seen {
                        return Err(invalid_step_contract(
                            "startup upgrade plan tried to run alerting upgrade more than once",
                        ));
                    }
                    alerting_upgrade_seen = true;
                }
            }
            StartupUpgradeStep::EnsureLegacyChangeEventsRemoval => {
                if opened_runtime {
                    return Err(invalid_step_contract(
                        "startup upgrade plan tried to run destructive migration after opening runtime",
                    ));
                }
                if !alerting_upgrade_seen {
                    return Err(invalid_step_contract(
                        "startup upgrade plan tried to remove legacy change events before durable alerting upgrade",
                    ));
                }
                if legacy_removal_seen {
                    return Err(invalid_step_contract(
                        "startup upgrade plan tried to run legacy change-events removal more than once",
                    ));
                }
                legacy_removal_seen = true;
            }
            StartupUpgradeStep::OpenRuntime => {
                if opened_runtime {
                    return Err(invalid_step_contract(
                        "startup upgrade plan tried to open runtime more than once",
                    ));
                }
                opened_runtime = true;
            }
            StartupUpgradeStep::VerifyWritableRuntime => {
                if !opened_runtime {
                    return Err(invalid_step_contract(
                        "startup upgrade plan tried to verify runtime before opening it",
                    ));
                }
                verified_writable = true;
            }
            StartupUpgradeStep::VerifySecrets => {
                if !verified_writable {
                    return Err(invalid_step_contract(
                        "startup upgrade plan tried to verify secrets before writable runtime",
                    ));
                }
                verified_secrets = true;
            }
        }
    }

    if !opened_runtime {
        return Err(invalid_step_contract(
            "startup upgrade plan completed without opening a runtime",
        ));
    }
    if !verified_writable || !verified_secrets {
        return Err(invalid_step_contract(
            "startup upgrade plan completed without final writable and secret verification",
        ));
    }
    Ok(())
}

fn invalid_step_contract(message: impl Into<String>) -> StartupUpgradeError {
    StartupUpgradeError::new(RecoveryReason::InternalUpgradeError, message)
}

fn validate_v2_artifact_for_startup(
    path: &Path,
    data_key: [u8; 32],
    faults: &dyn UpgradeFaultInjector,
) -> Result<(), StartupUpgradeError> {
    block_on(persistence::validate_read_only_sqlite(path)).map_err(|error| {
        StartupUpgradeError::new(
            RecoveryReason::CorruptedDatabase,
            format!("V2 database validation failed: {error}"),
        )
    })?;
    check(faults, UpgradeFailpoint::SecretValidation).map_err(|error| {
        StartupUpgradeError::new(
            RecoveryReason::InternalUpgradeError,
            format!("secret validation preflight failed: {error}"),
        )
    })?;
    validate_database_secrets_typed(path, &data_key).map_err(secret_validation_error)
}

fn secret_validation_error(error: SecretValidationError) -> StartupUpgradeError {
    match error {
        SecretValidationError::ReadFailed(message) => StartupUpgradeError::new(
            RecoveryReason::CorruptedDatabase,
            format!("failed to read database for secret validation: {message}"),
        ),
        SecretValidationError::KeyMismatch { row_id } => StartupUpgradeError::new(
            RecoveryReason::KeyMismatch,
            format!("secret validation failed for row {row_id}"),
        ),
    }
}

fn check(faults: &dyn UpgradeFaultInjector, failpoint: UpgradeFailpoint) -> Result<(), String> {
    faults.check(failpoint).map_err(|error| error.to_string())
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tauri::async_runtime::block_on(future)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_invalid_contract(steps: &[StartupUpgradeStep]) {
        let error = validate_startup_upgrade_steps(steps).expect_err("invalid step contract");

        assert_eq!(
            error.recovery_reason(),
            RecoveryReason::InternalUpgradeError
        );
    }

    #[test]
    fn startup_upgrade_executor_accepts_planner_shaped_step_sequence() {
        validate_startup_upgrade_steps(&[
            StartupUpgradeStep::EnsureStructuralPreBaseline,
            StartupUpgradeStep::EnsureSecretBaseline,
            StartupUpgradeStep::EnsureSchema { target_schema: 29 },
            StartupUpgradeStep::EnsureAlertingUpgrade,
            StartupUpgradeStep::EnsureLegacyChangeEventsRemoval,
            StartupUpgradeStep::OpenRuntime,
            StartupUpgradeStep::VerifyWritableRuntime,
            StartupUpgradeStep::VerifySecrets,
        ])
        .expect("planner-shaped sequence is executable");
    }

    #[test]
    fn startup_upgrade_executor_rejects_empty_or_unverified_step_sequence() {
        assert_invalid_contract(&[]);
        assert_invalid_contract(&[StartupUpgradeStep::OpenRuntime]);
        assert_invalid_contract(&[
            StartupUpgradeStep::OpenRuntime,
            StartupUpgradeStep::VerifyWritableRuntime,
        ]);
    }

    #[test]
    fn startup_upgrade_executor_rejects_steps_that_verify_before_runtime_is_ready() {
        assert_invalid_contract(&[
            StartupUpgradeStep::VerifyWritableRuntime,
            StartupUpgradeStep::OpenRuntime,
            StartupUpgradeStep::VerifySecrets,
        ]);
        assert_invalid_contract(&[
            StartupUpgradeStep::OpenRuntime,
            StartupUpgradeStep::VerifySecrets,
        ]);
    }

    #[test]
    fn startup_upgrade_executor_rejects_migrations_after_open_runtime() {
        assert_invalid_contract(&[
            StartupUpgradeStep::OpenRuntime,
            StartupUpgradeStep::VerifyWritableRuntime,
            StartupUpgradeStep::EnsureSchema { target_schema: 29 },
            StartupUpgradeStep::VerifySecrets,
        ]);
    }
}
