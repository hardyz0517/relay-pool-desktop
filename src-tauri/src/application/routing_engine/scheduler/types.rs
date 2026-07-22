use crate::models::routing::{PricingGroupType, RouteEndpointKind, RoutingGroupFilter};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScheduleRequest {
    pub(crate) endpoint: RouteEndpointKind,
    pub(crate) requested_model: Option<String>,
    pub(crate) mapped_model: Option<String>,
    pub(crate) routing_group_filter: RoutingGroupFilter,
    pub(crate) stream: bool,
    pub(crate) uses_tools: bool,
    pub(crate) uses_vision: bool,
    pub(crate) uses_reasoning: bool,
    pub(crate) max_rate_multiplier: f64,
    pub(crate) session_hash: Option<String>,
    pub(crate) previous_response_id: Option<String>,
    pub(crate) excluded_key_ids: Vec<String>,
    pub(crate) now_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SchedulerCandidate {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) priority: i64,
    pub(crate) max_concurrency: i64,
    pub(crate) load_factor: Option<i64>,
    pub(crate) group_binding_id: Option<String>,
    pub(crate) group_id_hash: Option<String>,
    pub(crate) group_type: Option<PricingGroupType>,
    pub(crate) station_enabled: bool,
    pub(crate) key_enabled: bool,
    pub(crate) schedulable: bool,
    pub(crate) supports_chat_completions: bool,
    pub(crate) supports_responses: bool,
    pub(crate) supports_embeddings: bool,
    pub(crate) supports_stream: bool,
    pub(crate) supports_tools: bool,
    pub(crate) supports_vision: bool,
    pub(crate) supports_reasoning: bool,
    pub(crate) model_allowlist: Vec<String>,
    pub(crate) model_blocklist: Vec<String>,
    pub(crate) health_blocked: bool,
    pub(crate) balance_depleted: bool,
    pub(crate) effective_multiplier: Option<EffectiveMultiplierFact>,
    pub(crate) multiplier_reject_reason: Option<MultiplierRejectReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateRejectionCode {
    AssetUnavailable,
    RoutingGroupMismatch,
    CapabilityMismatch,
    ModelMismatch,
    HealthBlocked,
    BalanceDepleted,
    NoMultiplierEvidence,
    MultiplierEvidenceInvalid,
    MultiplierEvidenceNegative,
    MultiplierEvidenceExpired,
    MultiplierEvidenceUnboundGroup,
    MultiplierEvidenceLowConfidence,
    MultiplierOverCeiling,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateRejection {
    pub(crate) primary_code: CandidateRejectionCode,
    pub(crate) reasons: Vec<CandidateRejectionCode>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EffectiveMultiplierFact {
    pub(crate) station_key_id: String,
    pub(crate) value: f64,
    pub(crate) source: String,
    pub(crate) collected_at_ms: Option<i64>,
    pub(crate) valid_until_ms: Option<i64>,
    pub(crate) confidence: f64,
    pub(crate) group_binding_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg(test)]
pub(crate) struct MultiplierSourceFacts {
    pub(crate) station_key_id: String,
    pub(crate) manual_rate_multiplier: Option<f64>,
    pub(crate) manual_rate_updated_at: Option<String>,
    pub(crate) group_binding_id: Option<String>,
    pub(crate) group_id_hash: Option<String>,
    pub(crate) group_name: Option<String>,
    pub(crate) inferred_group_category: Option<String>,
    pub(crate) group_category_override: Option<String>,
    pub(crate) collected_rate_multiplier: Option<f64>,
    pub(crate) collected_rate_source: Option<String>,
    pub(crate) collected_rate_confidence: Option<f64>,
    pub(crate) collected_rate_collected_at_ms: Option<i64>,
    pub(crate) collected_rate_valid_until_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "production routing policy consumes multiplier rejection evidence"
)]
pub(crate) enum MultiplierRejectReason {
    Missing,
    Invalid,
    Negative,
    Expired,
    UnboundGroup,
    LowConfidence,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScheduleDecision {
    pub(crate) selected_station_key_id: Option<String>,
    pub(crate) ordered_station_key_ids: Vec<String>,
    pub(crate) candidate_decisions: Vec<SchedulerCandidateDecision>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SchedulerCandidateDecision {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) accepted: bool,
    pub(crate) rejection: Option<CandidateRejection>,
    pub(crate) routing_group_match: bool,
    pub(crate) effective_multiplier: Option<EffectiveMultiplierFact>,
    pub(crate) score: Option<f64>,
    pub(crate) factors: Vec<String>,
    pub(crate) top_k_rank: Option<usize>,
    pub(crate) slot_result: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScheduleError {
    pub(crate) code: &'static str,
    pub(crate) candidate_decisions: Vec<SchedulerCandidateDecision>,
}

impl ScheduleError {
    pub(crate) fn new(
        code: &'static str,
        candidate_decisions: Vec<SchedulerCandidateDecision>,
    ) -> Self {
        Self {
            code,
            candidate_decisions,
        }
    }
}
