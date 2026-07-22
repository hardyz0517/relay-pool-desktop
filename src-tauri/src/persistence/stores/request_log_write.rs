#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestStartWrite {
    pub request_id: String,
    pub method: String,
    pub local_path: String,
    pub endpoint: String,
    pub received_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptHealthUpdate {
    Success,
    ObserveFailure,
    Cooldown { retry_after_ms: Option<i64> },
    HardFail,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptTerminalWrite {
    pub request_id: String,
    pub ordinal: u16,
    pub station_id: String,
    pub station_key_id: String,
    pub endpoint_revision: i64,
    pub started_at_ms: i64,
    pub terminal_kind: String,
    pub failure_kind: Option<String>,
    pub failure_blame: Option<String>,
    pub retry_disposition: Option<String>,
    pub health_effect: String,
    pub health_cooldown_until_ms: Option<i64>,
    pub health_update: AttemptHealthUpdate,
    pub public_code: Option<String>,
    pub sanitized_detail: Option<String>,
    pub output_committed: bool,
    pub terminal_at_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RequestLogAnnotationsWrite {
    pub model: Option<String>,
    pub stream: bool,
    pub selected_station_key_id: Option<String>,
    pub selected_station_id: Option<String>,
    pub upstream_base_url: Option<String>,
    pub route_policy: Option<String>,
    pub route_reason: Option<String>,
    pub rejected_candidates_json: Option<String>,
    pub body_bytes: Option<i64>,
    pub route_wait_ms: Option<i64>,
    pub upstream_headers_ms: Option<i64>,
    pub failure_source: Option<String>,
    pub attempts_json: Option<String>,
    pub completion_source: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub reasoning_effort: Option<String>,
    pub first_token_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestTerminalWrite {
    pub request_id: String,
    pub received_at_ms: i64,
    pub status: String,
    pub lifecycle_status: String,
    pub terminal_kind: String,
    pub terminal_code: Option<String>,
    pub terminal_detail: Option<String>,
    pub protocol_completed: bool,
    pub delivery_terminal: String,
    pub selected_attempt_ordinal: Option<u16>,
    pub attempt_count: u16,
    pub fallback_count: u16,
    pub terminal_at_ms: i64,
    pub annotations: RequestLogAnnotationsWrite,
}
