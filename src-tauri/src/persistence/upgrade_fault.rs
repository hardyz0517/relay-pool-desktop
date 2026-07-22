use std::{error::Error, fmt};

use super::upgrade_journal::UpgradePhase;

pub(crate) const UPGRADE_INJECTED_FAILURE_CODE: &str = "persistence_upgrade_fault_injected";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AtomicStep {
    BeforeWrite,
    BeforeFileSync,
    BeforeReplace,
    AfterReplaceBeforeParentSync,
    AfterDurableSync,
}

impl AtomicStep {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 5] = [
        Self::BeforeWrite,
        Self::BeforeFileSync,
        Self::BeforeReplace,
        Self::AfterReplaceBeforeParentSync,
        Self::AfterDurableSync,
    ];

    const fn code(self) -> &'static str {
        match self {
            Self::BeforeWrite => "before_write",
            Self::BeforeFileSync => "before_file_sync",
            Self::BeforeReplace => "before_replace",
            Self::AfterReplaceBeforeParentSync => "after_replace_before_parent_sync",
            Self::AfterDurableSync => "after_durable_sync",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BackupStep {
    Create,
    FileSync,
    Verify,
}

impl BackupStep {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 3] = [Self::Create, Self::FileSync, Self::Verify];

    const fn code(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::FileSync => "file_sync",
            Self::Verify => "verify",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ImportPhase {
    SettingsAndInstallation,
    StationsAndEndpointRevisions,
    CredentialsAndSecretReferences,
    StationKeysAndCapabilities,
    GroupsRoutingAndModelAliases,
    ChannelMonitors,
    Pricing,
    EvidenceAndHistory,
    HealthAndChangeEvents,
    DerivedProjectionsAndIndexes,
}

impl ImportPhase {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 10] = [
        Self::SettingsAndInstallation,
        Self::StationsAndEndpointRevisions,
        Self::CredentialsAndSecretReferences,
        Self::StationKeysAndCapabilities,
        Self::GroupsRoutingAndModelAliases,
        Self::ChannelMonitors,
        Self::Pricing,
        Self::EvidenceAndHistory,
        Self::HealthAndChangeEvents,
        Self::DerivedProjectionsAndIndexes,
    ];

    const fn code(self) -> &'static str {
        match self {
            Self::SettingsAndInstallation => "settings_and_installation",
            Self::StationsAndEndpointRevisions => "stations_and_endpoint_revisions",
            Self::CredentialsAndSecretReferences => "credentials_and_secret_references",
            Self::StationKeysAndCapabilities => "station_keys_and_capabilities",
            Self::GroupsRoutingAndModelAliases => "groups_routing_and_model_aliases",
            Self::ChannelMonitors => "channel_monitors",
            Self::Pricing => "pricing",
            Self::EvidenceAndHistory => "evidence_and_history",
            Self::HealthAndChangeEvents => "health_and_change_events",
            Self::DerivedProjectionsAndIndexes => "derived_projections_and_indexes",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(
    dead_code,
    reason = "runtime composition fault tests construct close-step variants through integration includes"
)]
pub(crate) enum RuntimeCloseStep {
    StopAdmission,
    Drain,
    Close,
}

impl RuntimeCloseStep {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 3] = [Self::StopAdmission, Self::Drain, Self::Close];

    const fn code(self) -> &'static str {
        match self {
            Self::StopAdmission => "stop_admission",
            Self::Drain => "drain",
            Self::Close => "close",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TombstoneStep {
    BeforeWrite,
    BeforeFileSync,
    BeforeReplace,
    AfterReplaceBeforeParentSync,
    AfterDurableSync,
}

impl TombstoneStep {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 5] = [
        Self::BeforeWrite,
        Self::BeforeFileSync,
        Self::BeforeReplace,
        Self::AfterReplaceBeforeParentSync,
        Self::AfterDurableSync,
    ];

    const fn code(self) -> &'static str {
        match self {
            Self::BeforeWrite => "before_write",
            Self::BeforeFileSync => "before_file_sync",
            Self::BeforeReplace => "before_replace",
            Self::AfterReplaceBeforeParentSync => "after_replace_before_parent_sync",
            Self::AfterDurableSync => "after_durable_sync",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpgradeFailpoint {
    #[allow(
        dead_code,
        reason = "runtime composition fault tests inject the lease acquisition boundary"
    )]
    LeaseAcquire,
    SourceOpen,
    SourceIntegrityCheck,
    Journal {
        phase: UpgradePhase,
        edge: AtomicStep,
    },
    Backup(BackupStep),
    Import(ImportPhase),
    CompatibilityValidation,
    SecretValidation,
    #[allow(
        dead_code,
        reason = "runtime composition fault tests inject staging-runtime close boundaries"
    )]
    StagingRuntimeClose(RuntimeCloseStep),
    Activation(AtomicStep),
    SourceIdentityRecheck,
    Tombstone(TombstoneStep),
    ConfigCommit(AtomicStep),
    V2Reopen,
    JournalCleanup(AtomicStep),
    #[allow(
        dead_code,
        reason = "runtime composition fault tests inject the lease transfer boundary"
    )]
    LeaseTransfer,
    #[allow(
        dead_code,
        reason = "runtime composition fault tests inject the service registration boundary"
    )]
    ServiceRegistration,
    #[allow(
        dead_code,
        reason = "runtime composition fault tests inject the finalization drain boundary"
    )]
    FinalizationDrain,
    #[allow(
        dead_code,
        reason = "runtime composition fault tests inject active-runtime close boundaries"
    )]
    RuntimeClose(RuntimeCloseStep),
    #[allow(
        dead_code,
        reason = "runtime composition fault tests inject the lease release boundary"
    )]
    LeaseRelease,
}

impl UpgradeFailpoint {
    pub(crate) fn code(self) -> String {
        match self {
            Self::LeaseAcquire => "lease.acquire".to_owned(),
            Self::SourceOpen => "source.open".to_owned(),
            Self::SourceIntegrityCheck => "source.integrity_check".to_owned(),
            Self::Journal { phase, edge } => {
                format!("journal.{}.{}", phase_code(phase), edge.code())
            }
            Self::Backup(step) => format!("backup.{}", step.code()),
            Self::Import(phase) => format!("import.{}", phase.code()),
            Self::CompatibilityValidation => "validation.compatibility".to_owned(),
            Self::SecretValidation => "validation.secret".to_owned(),
            Self::StagingRuntimeClose(step) => {
                format!("staging_runtime_close.{}", step.code())
            }
            Self::Activation(edge) => format!("activation.{}", edge.code()),
            Self::SourceIdentityRecheck => "source.identity_recheck".to_owned(),
            Self::Tombstone(step) => format!("tombstone.{}", step.code()),
            Self::ConfigCommit(edge) => format!("config_commit.{}", edge.code()),
            Self::V2Reopen => "v2.reopen".to_owned(),
            Self::JournalCleanup(edge) => format!("journal_cleanup.{}", edge.code()),
            Self::LeaseTransfer => "lease.transfer".to_owned(),
            Self::ServiceRegistration => "runtime.service_registration".to_owned(),
            Self::FinalizationDrain => "runtime.finalization_drain".to_owned(),
            Self::RuntimeClose(step) => format!("runtime_close.{}", step.code()),
            Self::LeaseRelease => "lease.release".to_owned(),
        }
    }

    #[cfg(test)]
    fn all() -> Vec<Self> {
        let mut failpoints = vec![
            Self::LeaseAcquire,
            Self::SourceOpen,
            Self::SourceIntegrityCheck,
        ];
        for phase in UpgradePhase::ALL {
            for edge in AtomicStep::ALL {
                failpoints.push(Self::Journal { phase, edge });
            }
        }
        failpoints.extend(BackupStep::ALL.map(Self::Backup));
        failpoints.extend(ImportPhase::ALL.map(Self::Import));
        failpoints.extend([Self::CompatibilityValidation, Self::SecretValidation]);
        failpoints.extend(RuntimeCloseStep::ALL.map(Self::StagingRuntimeClose));
        failpoints.extend(AtomicStep::ALL.map(Self::Activation));
        failpoints.push(Self::SourceIdentityRecheck);
        failpoints.extend(TombstoneStep::ALL.map(Self::Tombstone));
        failpoints.extend(AtomicStep::ALL.map(Self::ConfigCommit));
        failpoints.push(Self::V2Reopen);
        failpoints.extend(AtomicStep::ALL.map(Self::JournalCleanup));
        failpoints.extend([
            Self::LeaseTransfer,
            Self::ServiceRegistration,
            Self::FinalizationDrain,
        ]);
        failpoints.extend(RuntimeCloseStep::ALL.map(Self::RuntimeClose));
        failpoints.push(Self::LeaseRelease);
        failpoints
    }
}

pub(crate) trait UpgradeFaultInjector: Send + Sync {
    fn check(&self, failpoint: UpgradeFailpoint) -> Result<(), UpgradeInjectedFailure>;
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct NoUpgradeFaults;

impl UpgradeFaultInjector for NoUpgradeFaults {
    fn check(&self, _failpoint: UpgradeFailpoint) -> Result<(), UpgradeInjectedFailure> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UpgradeInjectedFailure {
    failpoint: UpgradeFailpoint,
}

impl UpgradeInjectedFailure {
    #[cfg(test)]
    pub(crate) const fn new(failpoint: UpgradeFailpoint) -> Self {
        Self { failpoint }
    }

    #[cfg(test)]
    pub(crate) const fn error_code(&self) -> &'static str {
        UPGRADE_INJECTED_FAILURE_CODE
    }

    #[cfg(test)]
    pub(crate) const fn failpoint(&self) -> UpgradeFailpoint {
        self.failpoint
    }

    #[cfg(test)]
    pub(crate) fn failpoint_code(&self) -> String {
        self.failpoint.code()
    }
}

impl fmt::Display for UpgradeInjectedFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{UPGRADE_INJECTED_FAILURE_CODE} at {}",
            self.failpoint.code()
        )
    }
}

impl Error for UpgradeInjectedFailure {}

const fn phase_code(phase: UpgradePhase) -> &'static str {
    match phase {
        UpgradePhase::Prepared => "prepared",
        UpgradePhase::BackupVerified => "backup_verified",
        UpgradePhase::V2Validated => "v2_validated",
        UpgradePhase::LegacyDeactivated => "legacy_deactivated",
        UpgradePhase::GenerationCommitted => "generation_committed",
        UpgradePhase::V2Reopened => "v2_reopened",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn failpoint_codes_are_stable_and_unique() {
        let failpoints = UpgradeFailpoint::all();
        let codes = failpoints
            .iter()
            .copied()
            .map(UpgradeFailpoint::code)
            .collect::<Vec<_>>();

        assert_eq!(codes.len(), 80);
        assert_eq!(codes.iter().collect::<HashSet<_>>().len(), codes.len());
        assert_eq!(codes.first().map(String::as_str), Some("lease.acquire"));
        assert!(codes.contains(&"journal.prepared.before_write".to_owned()));
        assert!(codes.contains(&"journal.v2_reopened.after_durable_sync".to_owned()));
        assert!(codes.contains(&"import.credentials_and_secret_references".to_owned()));
        assert!(codes.contains(&"activation.after_replace_before_parent_sync".to_owned()));
        assert!(codes.contains(&"tombstone.before_file_sync".to_owned()));
        assert!(codes.contains(&"config_commit.after_durable_sync".to_owned()));
        assert!(codes.contains(&"runtime_close.close".to_owned()));
        assert_eq!(codes.last().map(String::as_str), Some("lease.release"));
    }

    #[test]
    fn no_upgrade_faults_never_injects() {
        let injector = NoUpgradeFaults;
        for failpoint in UpgradeFailpoint::all() {
            assert_eq!(injector.check(failpoint), Ok(()));
        }
    }

    #[test]
    fn injected_failure_is_typed_and_redacted() {
        let failure = UpgradeInjectedFailure::new(UpgradeFailpoint::Journal {
            phase: UpgradePhase::V2Validated,
            edge: AtomicStep::BeforeReplace,
        });
        let display = failure.to_string();
        let debug = format!("{failure:?}");

        assert_eq!(failure.error_code(), UPGRADE_INJECTED_FAILURE_CODE);
        assert_eq!(
            failure.failpoint(),
            UpgradeFailpoint::Journal {
                phase: UpgradePhase::V2Validated,
                edge: AtomicStep::BeforeReplace,
            }
        );
        assert_eq!(
            failure.failpoint_code(),
            "journal.v2_validated.before_replace"
        );
        assert!(display.contains("journal.v2_validated.before_replace"));
        for sensitive in [
            r"C:\Users\alice\AppData\secret.sqlite3",
            "sk-secret-canary",
            "cookie=session-secret",
        ] {
            assert!(!display.contains(sensitive));
            assert!(!debug.contains(sensitive));
        }
    }
}
