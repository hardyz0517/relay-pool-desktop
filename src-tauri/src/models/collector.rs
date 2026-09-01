use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stable machine-readable error code shared by collection producers and the
/// application-owned recovery projection. Keeping the value in the model
/// layer avoids coupling those two boundary owners to each other.
pub(crate) const MANUAL_AUTHORIZATION_ERROR_CODE: &str = "manual_authorization_required";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorSnapshot {
    pub id: String,
    pub station_id: String,
    pub endpoint_revision: i64,
    pub source: String,
    pub status: String,
    pub fetched_at: String,
    pub summary_json: Value,
    pub normalized_json: Value,
    pub raw_json_redacted: Option<Value>,
    pub error_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorEvent {
    pub event_type: String,
    pub message: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorRunResult {
    pub snapshot: CollectorSnapshot,
    pub events: Vec<CollectorEvent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationLoginTestInput {
    #[serde(default)]
    pub station_type: Option<String>,
    pub website_url: String,
    pub login_username: String,
    pub login_password: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationLoginTestResult {
    pub status: String,
    pub message: String,
    pub diagnosis: Option<String>,
    pub token_present: bool,
}
