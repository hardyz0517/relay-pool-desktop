use crate::application::health_protection::HealthProtectionScope;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestStartWrite {
    pub request_id: String,
    pub method: String,
    pub local_path: String,
    pub endpoint: String,
    pub received_at_ms: i64,
    pub model: Option<String>,
    pub stream: bool,
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestRouteSelectionWrite {
    pub request_id: String,
    pub attempt_ordinal: u16,
    pub station_key_id: String,
    pub station_id: String,
    pub route_policy: String,
    pub route_reason: String,
    pub selected_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptHealthUpdate {
    Success,
    ProbeSuccess,
    ObserveFailure,
    Cooldown { retry_after_ms: Option<i64> },
    ProbeFailure { retry_after_ms: Option<i64> },
    HardFail,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttemptDurableEffectWrite {
    Credential {
        station_key_id: String,
        dimension: String,
        verdict: String,
        retry_after_ms: Option<i64>,
        evidence_code: String,
        classifier_profile_version: String,
    },
    Account {
        station_id: String,
        dimension: String,
        verdict: String,
        retry_after_ms: Option<i64>,
        evidence_code: String,
        classifier_profile_version: String,
    },
    Group {
        station_id: String,
        group_binding_id: String,
        dimension: String,
        verdict: String,
        retry_after_ms: Option<i64>,
        evidence_code: String,
        classifier_profile_version: String,
    },
    Endpoint {
        station_id: String,
        endpoint_revision: i64,
        dimension: String,
        verdict: String,
        retry_after_ms: Option<i64>,
        evidence_code: String,
        classifier_profile_version: String,
    },
    UnsupportedModel {
        station_key_id: String,
        model: String,
        evidence_code: String,
        classifier_profile_version: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptTerminalWrite {
    pub request_id: String,
    pub ordinal: u16,
    pub station_id: String,
    pub station_key_id: String,
    pub endpoint_revision: i64,
    pub credential_revision: i64,
    pub account_revision: i64,
    pub group_binding_id: Option<String>,
    pub group_revision: Option<i64>,
    pub resolved_upstream_model: Option<String>,
    pub comparability_key: Option<String>,
    pub model_alias_revision: i64,
    pub started_at_ms: i64,
    pub terminal_kind: String,
    pub failure_kind: Option<String>,
    pub failure_blame: Option<String>,
    pub retry_disposition: Option<String>,
    pub health_effect: String,
    pub health_cooldown_until_ms: Option<i64>,
    pub health_update: AttemptHealthUpdate,
    pub durable_effect: Option<AttemptDurableEffectWrite>,
    pub public_code: Option<String>,
    pub sanitized_detail: Option<String>,
    pub output_committed: bool,
    /// Canonical outcome time supplied by the attempt producer.
    pub event_at_ms: i64,
    /// Time at which the lifecycle owner observed the terminal outcome.
    pub observed_at_ms: i64,
    /// Stable persistence receive time captured before retrying the write.
    pub ingested_at_ms: i64,
    pub terminal_at_ms: i64,
    pub probe_scope: Option<HealthProtectionScope>,
    pub probe_state_revision: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RequestLogAnnotationsWrite {
    pub model: Option<String>,
    pub stream: bool,
    pub http_status: Option<i64>,
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
    pub billing_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RequestTerminalWrite {
    pub request_id: String,
    pub received_at_ms: i64,
    pub status: String,
    pub lifecycle_status: String,
    pub usage_status: String,
    pub terminal_kind: String,
    pub terminal_code: Option<String>,
    pub terminal_detail: Option<String>,
    pub protocol_completed: bool,
    pub delivery_terminal: String,
    pub selected_attempt_ordinal: Option<u16>,
    pub attempt_count: u16,
    pub fallback_count: u16,
    pub terminal_at_ms: i64,
    pub routing_outcome: RequestRoutingOutcomeSummaryWrite,
    pub annotations: RequestLogAnnotationsWrite,
}

/// Closed, versioned terminal facts safe to retain after the process-local
/// trace ring has been evicted. Do not add dynamic text or identities here.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RequestRoutingOutcomeSummaryWrite {
    pub terminal_kind: String,
    pub terminal_code: String,
    pub classification: String,
    pub confidence: String,
    pub evidence_source: String,
    pub request_accepted: String,
    pub send_phase: String,
    pub replay_disposition: String,
    pub billing_state: String,
    pub retry_disposition: String,
    pub effect_summary: String,
    pub failure_domain_commitment_version: Option<i64>,
    pub failure_domain_commitment_digest: Option<String>,
    pub attempt_count: u16,
    pub fallback_count: u16,
    pub terminal_at_ms: i64,
}
