pub mod newapi;
pub mod openai_compatible;
pub mod sub2api;

use crate::services::collectors::facts::CollectorFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectorTask {
    Detect,
    Balance,
    Groups,
    Models,
    Full,
}

impl CollectorTask {
    pub fn as_str(self) -> &'static str {
        match self {
            CollectorTask::Detect => "detect",
            CollectorTask::Balance => "balance",
            CollectorTask::Groups => "groups",
            CollectorTask::Models => "models",
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
}
