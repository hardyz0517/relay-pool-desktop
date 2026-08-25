#[cfg(test)]
use crate::models::remote_keys::RemoteStationKey;
use crate::services::collectors::facts::CollectorFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectorTask {
    Detect,
    Balance,
    Groups,
    PublishedStatus,
    Full,
}

impl CollectorTask {
    pub fn as_str(self) -> &'static str {
        match self {
            CollectorTask::Detect => "detect",
            CollectorTask::Balance => "balance",
            CollectorTask::Groups => "groups",
            CollectorTask::PublishedStatus => "published_status",
            CollectorTask::Full => "full",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdapterOutput {
    pub adapter: String,
    pub task: CollectorTask,
    pub status: String,
    pub facts: CollectorFacts,
    pub summary_json: serde_json::Value,
    pub normalized_json: serde_json::Value,
    pub raw_json_redacted: Option<serde_json::Value>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    /// Wall-clock start paired with a monotonic elapsed duration. These values
    /// describe provider execution, not the later persistence transaction.
    pub execution_started_at_ms: Option<i64>,
    pub execution_duration_ms: Option<i64>,
}

impl AdapterOutput {
    pub(crate) fn with_execution_timing(mut self, started_at_ms: i64, duration_ms: i64) -> Self {
        self.execution_started_at_ms = Some(started_at_ms.max(0));
        self.execution_duration_ms = Some(duration_ms.max(0));
        self
    }
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub struct CreatedRemoteKey {
    pub remote_key: RemoteStationKey,
    pub full_key_once: Option<String>,
    pub message: String,
}
