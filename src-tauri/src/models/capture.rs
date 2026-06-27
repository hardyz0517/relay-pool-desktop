use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedHttpEventInput {
    pub station_id: String,
    pub source_window_id: String,
    pub page_url: String,
    pub request_url: String,
    pub request_path: Option<String>,
    pub method: String,
    pub status: Option<i64>,
    pub content_type: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub response_kind: Option<String>,
    pub response_size: Option<i64>,
    pub response_json: Option<Value>,
    pub response_text: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedHttpEvent {
    pub id: String,
    pub station_id: String,
    pub source_window_id: String,
    pub page_url: String,
    pub request_url: String,
    pub request_path: String,
    pub method: String,
    pub status: Option<i64>,
    pub content_type: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub response_kind: String,
    pub response_size: i64,
    pub response_json_redacted: Option<Value>,
    pub response_text_preview_redacted: Option<String>,
    pub classification: String,
    pub confidence: f64,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSessionStatus {
    pub station_id: String,
    pub status: String,
    pub capture_count: usize,
    pub recognized_field_count: usize,
    pub pending_confirmation_count: usize,
    pub last_error: Option<String>,
}
