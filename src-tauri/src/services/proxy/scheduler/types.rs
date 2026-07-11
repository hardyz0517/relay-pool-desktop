use crate::models::routing::{PricingGroupType, RouteEndpointKind, RoutingGroupFilter};

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleRequest {
    pub endpoint: RouteEndpointKind,
    pub requested_model: Option<String>,
    pub mapped_model: Option<String>,
    pub routing_group_filter: RoutingGroupFilter,
    pub stream: bool,
    pub uses_tools: bool,
    pub uses_vision: bool,
    pub uses_reasoning: bool,
    pub max_rate_multiplier: f64,
    pub session_hash: Option<String>,
    pub previous_response_id: Option<String>,
    pub excluded_key_ids: Vec<String>,
    pub now_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SchedulerCandidate {
    pub station_key_id: String,
    pub station_id: String,
    pub priority: i64,
    pub group_binding_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_type: Option<PricingGroupType>,
    pub station_enabled: bool,
    pub key_enabled: bool,
    pub schedulable: bool,
    pub supports_chat_completions: bool,
    pub supports_responses: bool,
    pub supports_embeddings: bool,
    pub supports_stream: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_reasoning: bool,
    pub model_allowlist: Vec<String>,
    pub model_blocklist: Vec<String>,
    pub health_blocked: bool,
    pub balance_depleted: bool,
    pub effective_multiplier: Option<EffectiveMultiplierFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateRejectionCode {
    AssetUnavailable,
    RoutingGroupMismatch,
    CapabilityMismatch,
    ModelMismatch,
    HealthBlocked,
    BalanceDepleted,
    NoMultiplierEvidence,
    MultiplierOverCeiling,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateRejection {
    pub primary_code: CandidateRejectionCode,
    pub reasons: Vec<CandidateRejectionCode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveMultiplierFact {
    pub station_key_id: String,
    pub value: f64,
    pub source: String,
    pub collected_at_ms: Option<i64>,
    pub valid_until_ms: Option<i64>,
    pub confidence: f64,
    pub group_binding_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MultiplierSourceFacts {
    pub station_key_id: String,
    pub manual_rate_multiplier: Option<f64>,
    pub manual_rate_updated_at: Option<String>,
    pub group_binding_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_name: Option<String>,
    pub collected_rate_multiplier: Option<f64>,
    pub collected_rate_source: Option<String>,
    pub collected_rate_confidence: Option<f64>,
    pub collected_rate_collected_at_ms: Option<i64>,
    pub collected_rate_valid_until_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiplierRejectReason {
    Missing,
    Invalid,
    Negative,
    Expired,
    UnboundGroup,
    LowConfidence,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleDecision {
    pub selected_station_key_id: Option<String>,
    pub ordered_station_key_ids: Vec<String>,
    pub candidate_decisions: Vec<SchedulerCandidateDecision>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SchedulerCandidateDecision {
    pub station_key_id: String,
    pub station_id: String,
    pub accepted: bool,
    pub rejection: Option<CandidateRejection>,
    pub routing_group_match: bool,
    pub effective_multiplier: Option<EffectiveMultiplierFact>,
    pub score: Option<f64>,
    pub factors: Vec<String>,
    pub top_k_rank: Option<usize>,
    pub slot_result: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleError {
    pub code: &'static str,
    pub candidate_decisions: Vec<SchedulerCandidateDecision>,
}

impl ScheduleError {
    pub fn new(code: &'static str, candidate_decisions: Vec<SchedulerCandidateDecision>) -> Self {
        Self {
            code,
            candidate_decisions,
        }
    }
}
