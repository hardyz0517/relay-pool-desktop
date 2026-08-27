use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    Scheduled,
    Manual,
    StartupRecovery,
    LegacyImport,
}

impl TriggerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Manual => "manual",
            Self::StartupRecovery => "startup_recovery",
            Self::LegacyImport => "legacy_import",
        }
    }
}
