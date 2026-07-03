use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoutingPolicy {
    PriorityFallback,
    StableFirst,
    BackupOnly,
    CheapFirst,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteEndpointKind {
    Models,
    ChatCompletions,
    Responses,
    Embeddings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationKeyCapabilities {
    pub station_key_id: String,
    pub supports_chat_completions: bool,
    pub supports_responses: bool,
    pub supports_embeddings: bool,
    pub supports_stream: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_reasoning: bool,
    pub model_allowlist: Vec<String>,
    pub model_blocklist: Vec<String>,
    pub preferred_models: Vec<String>,
    pub only_use_as_backup: bool,
    pub routing_tags: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStationKeyCapabilitiesInput {
    pub station_key_id: String,
    pub supports_chat_completions: bool,
    pub supports_responses: bool,
    pub supports_embeddings: bool,
    pub supports_stream: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_reasoning: bool,
    pub model_allowlist: Vec<String>,
    pub model_blocklist: Vec<String>,
    pub preferred_models: Vec<String>,
    pub only_use_as_backup: bool,
    pub routing_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelAlias {
    pub id: String,
    pub client_model: String,
    pub upstream_model: String,
    pub enabled: bool,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertModelAliasInput {
    pub id: Option<String>,
    pub client_model: String,
    pub upstream_model: String,
    pub enabled: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationKeyHealth {
    pub station_key_id: String,
    pub last_success_at: Option<String>,
    pub last_failure_at: Option<String>,
    pub consecutive_failures: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub avg_latency_ms: Option<i64>,
    pub last_error_summary: Option<String>,
    pub cooldown_until: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSimulationInput {
    pub endpoint: RouteEndpointKind,
    pub model: Option<String>,
    pub stream: bool,
    pub uses_tools: bool,
    pub uses_vision: bool,
    pub uses_reasoning: bool,
    pub policy: Option<RoutingPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteCandidateExplanation {
    pub station_key_id: String,
    pub station_id: String,
    pub station_name: String,
    pub key_name: String,
    pub accepted: bool,
    pub score: i64,
    pub reasons: Vec<String>,
    pub rejection_reasons: Vec<String>,
    pub mapped_model: Option<String>,
    pub pricing_rule_id: Option<String>,
    pub group_binding_id: Option<String>,
    pub rate_multiplier: Option<f64>,
    pub normalization_status: Option<String>,
    pub price_confidence: Option<f64>,
    pub estimated_input_price: Option<f64>,
    pub estimated_output_price: Option<f64>,
    pub price_currency: Option<String>,
    pub balance_status: Option<String>,
    pub balance_value: Option<f64>,
    pub balance_scope: Option<String>,
    pub balance_collected_at: Option<String>,
    pub economic_freshness: Option<String>,
    pub economic_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSimulationResult {
    pub selected_station_key_id: Option<String>,
    pub selected_station_id: Option<String>,
    pub mapped_model: Option<String>,
    pub policy: RoutingPolicy,
    pub candidates: Vec<RouteCandidateExplanation>,
    pub message: String,
}
