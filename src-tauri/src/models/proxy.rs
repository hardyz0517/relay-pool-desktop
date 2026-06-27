use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatus {
    pub running: bool,
    pub bind_addr: String,
    pub port: u16,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
    pub active_requests: u32,
    pub request_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestLog {
    pub id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub method: String,
    pub path: String,
    pub model: Option<String>,
    pub stream: bool,
    pub status: String,
    pub station_key_id: Option<String>,
    pub station_id: Option<String>,
    pub upstream_base_url: Option<String>,
    pub fallback_count: i64,
    pub error_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequestLogInput {
    pub method: String,
    pub path: String,
    pub model: Option<String>,
    pub stream: bool,
    pub status: String,
    pub station_key_id: Option<String>,
    pub station_id: Option<String>,
    pub upstream_base_url: Option<String>,
    pub fallback_count: i64,
    pub error_message: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientRequestKind {
    ChatCompletions,
    Responses,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamApiFormat {
    Auto,
    OpenAiChatCompletions,
    OpenAiResponses,
    CustomOpenAiCompatible,
}
