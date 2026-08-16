use crate::{
    persistence::schema_registry::MINIMUM_AUTOMATIC_SCHEMA_BASELINE,
    services::data_store::{
        alerting_upgrade::ALERTING_FOUNDATION_SCHEMA_VERSION,
        startup_probe::{
            SecretFormatProbe, StartupJournalProbe, StartupKeyRequirementProbe, StartupUpgradeProbe,
        },
        types::RecoveryReason,
    },
    services::secrets::baseline_conversion::{
        ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION, PRE_BASELINE_SCHEMA_VERSION,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupUpgradeRecovery {
    UnsupportedVersion,
    InconsistentVersionMetadata,
    CorruptedDatabase,
    InterruptedUpgrade,
    MissingKey,
    KeyMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupUpgradeStep {
    EnsureStructuralPreBaseline,
    EnsureSecretBaseline,
    EnsureLatestSchema,
    EnsureAlertingUpgrade,
    EnsureLegacyChangeEventsRemoval,
    OpenRuntime,
    VerifyWritableRuntime,
    VerifySecrets,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StartupUpgradePlan {
    Execute(Vec<StartupUpgradeStep>),
    NeedsRecovery(StartupUpgradeRecovery),
}

pub(crate) fn plan_upgrade(probe: &StartupUpgradeProbe) -> StartupUpgradePlan {
    let compatibility_schema = probe.compatibility_schema_version;
    let sql_schema = probe.sql_migration_version;
    let latest_schema = probe.latest_sql_migration_version;

    if !probe.sqlite_quick_check_passed {
        return StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::CorruptedDatabase);
    }
    if probe.journal == StartupJournalProbe::Invalid {
        return StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::InterruptedUpgrade);
    }
    if compatibility_schema < MINIMUM_AUTOMATIC_SCHEMA_BASELINE {
        return StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::UnsupportedVersion);
    }
    if compatibility_schema > latest_schema || sql_schema < compatibility_schema {
        return StartupUpgradePlan::NeedsRecovery(
            StartupUpgradeRecovery::InconsistentVersionMetadata,
        );
    }
    if probe.secret_format == SecretFormatProbe::InvalidMetadata
        || (probe.secret_format == SecretFormatProbe::EncryptedBaseline
            && compatibility_schema < ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION)
        || (probe.secret_format == SecretFormatProbe::Legacy
            && compatibility_schema >= ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION)
    {
        return StartupUpgradePlan::NeedsRecovery(
            StartupUpgradeRecovery::InconsistentVersionMetadata,
        );
    }
    match &probe.key_requirement {
        StartupKeyRequirementProbe::LegacyFormat => {}
        StartupKeyRequirementProbe::Verified { .. } => {}
        StartupKeyRequirementProbe::MissingSystemKeyId { .. } => {
            return StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::MissingKey);
        }
        StartupKeyRequirementProbe::MismatchedKeyId { .. } => {
            return StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::KeyMismatch);
        }
        StartupKeyRequirementProbe::MissingPersistedKeyId { .. } => {
            return StartupUpgradePlan::NeedsRecovery(
                StartupUpgradeRecovery::InconsistentVersionMetadata,
            );
        }
    }
    if sql_schema > compatibility_schema
        && !(compatibility_schema == PRE_BASELINE_SCHEMA_VERSION
            && sql_schema == ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION
            && probe.secret_format == SecretFormatProbe::Legacy)
    {
        return StartupUpgradePlan::NeedsRecovery(
            StartupUpgradeRecovery::InconsistentVersionMetadata,
        );
    }

    let mut steps = Vec::new();
    if compatibility_schema < PRE_BASELINE_SCHEMA_VERSION {
        steps.push(StartupUpgradeStep::EnsureStructuralPreBaseline);
    }
    if probe.secret_format == SecretFormatProbe::Legacy {
        steps.push(StartupUpgradeStep::EnsureSecretBaseline);
    }
    let schema_after_secret_baseline = if probe.secret_format == SecretFormatProbe::Legacy {
        ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION
    } else {
        compatibility_schema
    };
    if schema_after_secret_baseline < latest_schema {
        steps.push(StartupUpgradeStep::EnsureLatestSchema);
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
    StartupUpgradePlan::Execute(steps)
}

impl StartupUpgradeRecovery {
    pub(crate) const fn recovery_reason(self) -> RecoveryReason {
        match self {
            Self::UnsupportedVersion => RecoveryReason::UnsupportedSchemaVersion,
            Self::InconsistentVersionMetadata => RecoveryReason::InconsistentSchemaMetadata,
            Self::CorruptedDatabase => RecoveryReason::CorruptedDatabase,
            Self::InterruptedUpgrade => RecoveryReason::InterruptedUpgrade,
            Self::MissingKey => RecoveryReason::MissingKey,
            Self::KeyMismatch => RecoveryReason::KeyMismatch,
        }
    }

    #[cfg(test)]
    pub(crate) fn message(self, compatibility_schema_version: i64) -> String {
        match self {
            Self::UnsupportedVersion => format!(
                "database schema {compatibility_schema_version} is below the minimum supported automatic upgrade baseline {MINIMUM_AUTOMATIC_SCHEMA_BASELINE}"
            ),
            Self::InconsistentVersionMetadata => {
                "database schema metadata is inconsistent and requires recovery".to_string()
            }
            Self::CorruptedDatabase => {
                "database failed SQLite startup integrity checks".to_string()
            }
            Self::InterruptedUpgrade => {
                "database has an invalid or interrupted upgrade journal".to_string()
            }
            Self::MissingKey => "database requires a missing device key".to_string(),
            Self::KeyMismatch => {
                "database was encrypted with a different active device key".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn probe(
        compatibility_schema_version: i64,
        sql_migration_version: i64,
        secret_format: SecretFormatProbe,
    ) -> StartupUpgradeProbe {
        probe_with_latest(
            compatibility_schema_version,
            sql_migration_version,
            crate::persistence::current_schema_version(),
            secret_format,
        )
    }

    fn probe_with_latest(
        compatibility_schema_version: i64,
        sql_migration_version: i64,
        latest_sql_migration_version: i64,
        secret_format: SecretFormatProbe,
    ) -> StartupUpgradeProbe {
        StartupUpgradeProbe {
            active_database_path: PathBuf::from("relay-pool.sqlite3"),
            compatibility_schema_version,
            sql_migration_version,
            latest_sql_migration_version,
            secret_format,
            key_requirement: match secret_format {
                SecretFormatProbe::EncryptedBaseline => StartupKeyRequirementProbe::Verified {
                    persisted_key_id: "device-key-v1".to_string(),
                    system_key_id: "device-key-v1".to_string(),
                },
                SecretFormatProbe::Legacy | SecretFormatProbe::InvalidMetadata => {
                    StartupKeyRequirementProbe::LegacyFormat
                }
            },
            journal: StartupJournalProbe::Missing,
            sqlite_quick_check_passed: true,
        }
    }

    #[test]
    fn schema_15_routes_through_structural_then_secret_baseline() {
        let plan = plan_upgrade(&probe(15, 15, SecretFormatProbe::Legacy));

        assert_eq!(
            plan,
            StartupUpgradePlan::Execute(vec![
                StartupUpgradeStep::EnsureStructuralPreBaseline,
                StartupUpgradeStep::EnsureSecretBaseline,
                StartupUpgradeStep::EnsureLatestSchema,
                StartupUpgradeStep::EnsureAlertingUpgrade,
                StartupUpgradeStep::EnsureLegacyChangeEventsRemoval,
                StartupUpgradeStep::OpenRuntime,
                StartupUpgradeStep::VerifyWritableRuntime,
                StartupUpgradeStep::VerifySecrets,
            ])
        );
    }

    #[test]
    fn transitional_sql_17_compatibility_16_routes_to_secret_baseline() {
        let plan = plan_upgrade(&probe(16, 17, SecretFormatProbe::Legacy));

        assert_eq!(
            plan,
            StartupUpgradePlan::Execute(vec![
                StartupUpgradeStep::EnsureSecretBaseline,
                StartupUpgradeStep::EnsureLatestSchema,
                StartupUpgradeStep::EnsureAlertingUpgrade,
                StartupUpgradeStep::EnsureLegacyChangeEventsRemoval,
                StartupUpgradeStep::OpenRuntime,
                StartupUpgradeStep::VerifyWritableRuntime,
                StartupUpgradeStep::VerifySecrets,
            ])
        );
    }

    #[test]
    fn future_schema_versions_are_explicit_latest_schema_steps() {
        let plan = plan_upgrade(&probe_with_latest(
            17,
            17,
            18,
            SecretFormatProbe::EncryptedBaseline,
        ));

        assert_eq!(
            plan,
            StartupUpgradePlan::Execute(vec![
                StartupUpgradeStep::EnsureLatestSchema,
                StartupUpgradeStep::OpenRuntime,
                StartupUpgradeStep::VerifyWritableRuntime,
                StartupUpgradeStep::VerifySecrets,
            ])
        );
    }

    #[test]
    fn schema_below_15_is_unsupported() {
        assert!(matches!(
            plan_upgrade(&probe(14, 14, SecretFormatProbe::Legacy)),
            StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::UnsupportedVersion)
        ));
    }

    #[test]
    fn explicit_secret_format_metadata_must_match_compatibility_state() {
        assert!(matches!(
            plan_upgrade(&probe(16, 16, SecretFormatProbe::EncryptedBaseline)),
            StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::InconsistentVersionMetadata)
        ));
        assert!(matches!(
            plan_upgrade(&probe(17, 17, SecretFormatProbe::Legacy)),
            StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::InconsistentVersionMetadata)
        ));
        assert!(matches!(
            plan_upgrade(&probe(17, 17, SecretFormatProbe::InvalidMetadata)),
            StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::InconsistentVersionMetadata)
        ));
    }

    #[test]
    fn invalid_journal_and_failed_integrity_stop_before_execution() {
        let mut invalid_journal = probe(17, 17, SecretFormatProbe::EncryptedBaseline);
        invalid_journal.journal = StartupJournalProbe::Invalid;
        assert!(matches!(
            plan_upgrade(&invalid_journal),
            StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::InterruptedUpgrade)
        ));

        let mut corrupt = probe(17, 17, SecretFormatProbe::EncryptedBaseline);
        corrupt.sqlite_quick_check_passed = false;
        assert!(matches!(
            plan_upgrade(&corrupt),
            StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::CorruptedDatabase)
        ));
    }

    #[test]
    fn current_encrypted_database_rejects_mismatched_key_identity_before_steps() {
        let mut mismatched = probe(17, 17, SecretFormatProbe::EncryptedBaseline);
        mismatched.key_requirement = StartupKeyRequirementProbe::MismatchedKeyId {
            persisted_key_id: "device-key-a".to_string(),
            system_key_id: "device-key-b".to_string(),
        };
        assert_eq!(
            plan_upgrade(&mismatched),
            StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::KeyMismatch)
        );

        let mut missing_system = probe(17, 17, SecretFormatProbe::EncryptedBaseline);
        missing_system.key_requirement = StartupKeyRequirementProbe::MissingSystemKeyId {
            persisted_key_id: "device-key-a".to_string(),
        };
        assert_eq!(
            plan_upgrade(&missing_system),
            StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::MissingKey)
        );

        let mut missing_persisted = probe(17, 17, SecretFormatProbe::EncryptedBaseline);
        missing_persisted.key_requirement = StartupKeyRequirementProbe::MissingPersistedKeyId {
            system_key_id: Some("device-key-b".to_string()),
        };
        assert_eq!(
            plan_upgrade(&missing_persisted),
            StartupUpgradePlan::NeedsRecovery(StartupUpgradeRecovery::InconsistentVersionMetadata)
        );
    }
}
