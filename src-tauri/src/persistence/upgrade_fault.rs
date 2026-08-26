use std::{error::Error, fmt};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum UpgradeFailpoint {
    SecretValidation,
    V2Reopen,
    ConfigCommit(AtomicStep),
    ServiceRegistration,
    FinalizationDrain,
}

impl UpgradeFailpoint {
    pub(crate) fn code(self) -> String {
        match self {
            Self::SecretValidation => "validation.secret".to_owned(),
            Self::V2Reopen => "v2.reopen".to_owned(),
            Self::ConfigCommit(edge) => format!("config_commit.{}", atomic_step_code(edge)),
            Self::ServiceRegistration => "runtime.service_registration".to_owned(),
            Self::FinalizationDrain => "runtime.finalization_drain".to_owned(),
        }
    }
}

fn atomic_step_code(step: AtomicStep) -> &'static str {
    match step {
        AtomicStep::BeforeWrite => "before_write",
        AtomicStep::BeforeFileSync => "before_file_sync",
        AtomicStep::BeforeReplace => "before_replace",
        AtomicStep::AfterReplaceBeforeParentSync => "after_replace_before_parent_sync",
        AtomicStep::AfterDurableSync => "after_durable_sync",
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

#[cfg(any(test, debug_assertions))]
impl UpgradeInjectedFailure {
    pub(crate) const fn new(failpoint: UpgradeFailpoint) -> Self {
        Self { failpoint }
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
