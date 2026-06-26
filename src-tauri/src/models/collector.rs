use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorSnapshot {
    pub id: String,
    pub station_id: String,
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
