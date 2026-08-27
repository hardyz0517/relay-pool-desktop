use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoutingPolicy {
    AutomaticBalanced,
    PriorityFallback,
    StableFirst,
    BackupOnly,
    CheapFirst,
    CostStableFirst,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteEndpointKind {
    Models,
    ChatCompletions,
    Responses,
    Embeddings,
}

pub use crate::models::routing_policy::{PricingGroupType, RoutingGroupFilter};

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeRoutingSettings {
    pub policy: RoutingPolicy,
    pub max_rate_multiplier: Option<f64>,
    pub routing_group_scope: RoutingGroupFilter,
    pub scheduler_config: DispatchAlgorithmSettings,
    pub allow_depleted_fallback: bool,
    /// Local routing's proxy override. `inherit` delegates to the global
    /// network setting before station-level overrides are applied.
    pub outbound_proxy_mode: String,
    pub outbound_proxy_url: Option<String>,
    pub global_proxy_mode: String,
    pub global_proxy_url: Option<String>,
}

impl Default for RuntimeRoutingSettings {
    fn default() -> Self {
        Self {
            policy: RoutingPolicy::PriorityFallback,
            max_rate_multiplier: None,
            routing_group_scope: RoutingGroupFilter::default(),
            scheduler_config: DispatchAlgorithmSettings::default(),
            allow_depleted_fallback: false,
            outbound_proxy_mode: "inherit".to_string(),
            outbound_proxy_url: None,
            global_proxy_mode: "direct".to_string(),
            global_proxy_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg(test)]
pub struct AutomaticSchedulerSettings {
    pub max_rate_multiplier: Option<f64>,
    pub routing_group_scope: RoutingGroupFilter,
    pub advanced: DispatchAlgorithmSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DispatchAlgorithmSettings {
    pub top_k: u16,
    pub multiplier: f64,
    pub priority: f64,
    pub load: f64,
    pub queue: f64,
    pub error_rate: f64,
    pub ttft: f64,
    pub quota_headroom: f64,
    pub previous_response: f64,
    pub session_sticky: f64,
    pub multiplier_min_confidence: f64,
    pub sticky_weighted: bool,
    pub sticky_escape: bool,
    pub sticky_escape_ttft_ms: u64,
    pub sticky_escape_error_rate: f64,
    pub sticky_session_ttl_seconds: u64,
    pub sticky_response_ttl_seconds: u64,
    pub sticky_max_waiting: u64,
    pub sticky_wait_timeout_seconds: u64,
    pub fallback_max_waiting: u64,
    pub fallback_wait_timeout_seconds: u64,
}

impl Default for DispatchAlgorithmSettings {
    fn default() -> Self {
        Self {
            top_k: 7,
            multiplier: 1.0,
            priority: 1.0,
            load: 1.0,
            queue: 0.7,
            error_rate: 0.8,
            ttft: 0.5,
            quota_headroom: 0.0,
            previous_response: 5.0,
            session_sticky: 3.0,
            multiplier_min_confidence: 0.8,
            sticky_weighted: false,
            sticky_escape: true,
            sticky_escape_ttft_ms: 15_000,
            sticky_escape_error_rate: 0.5,
            sticky_session_ttl_seconds: 3_600,
            sticky_response_ttl_seconds: 3_600,
            sticky_max_waiting: 3,
            sticky_wait_timeout_seconds: 120,
            fallback_max_waiting: 100,
            fallback_wait_timeout_seconds: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchedulerConfigError {
    #[cfg(test)]
    InvalidMultiplierLimit,
    InvalidAdvancedSetting(&'static str),
}

#[cfg(test)]
impl AutomaticSchedulerSettings {
    pub fn validate_for_routing(&self) -> Result<(), SchedulerConfigError> {
        if let Some(max_rate_multiplier) = self.max_rate_multiplier {
            if !max_rate_multiplier.is_finite() || max_rate_multiplier < 0.0 {
                return Err(SchedulerConfigError::InvalidMultiplierLimit);
            }
        }
        self.advanced.validate()
    }
}

impl DispatchAlgorithmSettings {
    pub fn validate(&self) -> Result<(), SchedulerConfigError> {
        if self.top_k == 0 {
            return Err(SchedulerConfigError::InvalidAdvancedSetting("top_k"));
        }
        let weighted_values = [
            ("multiplier", self.multiplier),
            ("priority", self.priority),
            ("load", self.load),
            ("queue", self.queue),
            ("error_rate", self.error_rate),
            ("ttft", self.ttft),
            ("quota_headroom", self.quota_headroom),
            ("previous_response", self.previous_response),
            ("session_sticky", self.session_sticky),
        ];
        for (name, value) in weighted_values {
            if !value.is_finite() || value < 0.0 {
                return Err(SchedulerConfigError::InvalidAdvancedSetting(name));
            }
        }
        if self.multiplier == 0.0
            && self.priority == 0.0
            && self.load == 0.0
            && self.queue == 0.0
            && self.error_rate == 0.0
            && self.ttft == 0.0
            && self.quota_headroom == 0.0
        {
            return Err(SchedulerConfigError::InvalidAdvancedSetting("base_weights"));
        }
        if !self.multiplier_min_confidence.is_finite()
            || !(0.0..=1.0).contains(&self.multiplier_min_confidence)
        {
            return Err(SchedulerConfigError::InvalidAdvancedSetting(
                "multiplier_min_confidence",
            ));
        }
        if !self.sticky_escape_error_rate.is_finite()
            || !(0.0..=1.0).contains(&self.sticky_escape_error_rate)
        {
            return Err(SchedulerConfigError::InvalidAdvancedSetting(
                "sticky_escape_error_rate",
            ));
        }
        if self.sticky_escape_ttft_ms == 0 {
            return Err(SchedulerConfigError::InvalidAdvancedSetting(
                "sticky_escape_ttft_ms",
            ));
        }
        if self.sticky_session_ttl_seconds == 0 {
            return Err(SchedulerConfigError::InvalidAdvancedSetting(
                "sticky_session_ttl_seconds",
            ));
        }
        if self.sticky_response_ttl_seconds == 0 {
            return Err(SchedulerConfigError::InvalidAdvancedSetting(
                "sticky_response_ttl_seconds",
            ));
        }
        if self.sticky_max_waiting == 0 {
            return Err(SchedulerConfigError::InvalidAdvancedSetting(
                "sticky_max_waiting",
            ));
        }
        if self.sticky_wait_timeout_seconds == 0 {
            return Err(SchedulerConfigError::InvalidAdvancedSetting(
                "sticky_wait_timeout_seconds",
            ));
        }
        if self.fallback_max_waiting == 0 {
            return Err(SchedulerConfigError::InvalidAdvancedSetting(
                "fallback_max_waiting",
            ));
        }
        if self.fallback_wait_timeout_seconds == 0 {
            return Err(SchedulerConfigError::InvalidAdvancedSetting(
                "fallback_wait_timeout_seconds",
            ));
        }
        Ok(())
    }
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
pub struct RuntimeRoutingSecret {
    pub id: String,
    pub scope: String,
    pub owner_id: String,
    pub kind: String,
    pub masked_value: String,
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub encryption_version: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRoutingBalance {
    pub scope: String,
    pub value: Option<f64>,
    pub currency: String,
    pub low_balance_threshold: Option<f64>,
    pub status: String,
    pub collected_at: Option<String>,
}

impl RuntimeRoutingBalance {
    pub fn is_depleted(&self) -> bool {
        balance_is_depleted(self.value, Some(self.status.as_str()))
    }

    pub(crate) fn has_explicit_status(&self) -> bool {
        matches!(
            self.status.trim().to_ascii_lowercase().as_str(),
            "normal"
                | "available"
                | "usable"
                | "low"
                | "warning"
                | "depleted"
                | "exhausted"
                | "empty"
        )
    }
}

/// Returns whether a balance is exhausted for routing admission.
///
/// `low`/`warning` are advisory states: a positive balance remains routeable.
/// A finite numeric balance is the freshest spendability fact; a conflicting
/// textual status is treated as stale metadata. Explicit exhausted states are
/// only used when the provider did not return a numeric balance.
pub(crate) fn balance_is_depleted(value: Option<f64>, status: Option<&str>) -> bool {
    match value.filter(|value| value.is_finite()) {
        Some(value) => value <= 0.0,
        None => status.is_some_and(|status| {
            matches!(
                status.trim().to_ascii_lowercase().as_str(),
                "depleted" | "exhausted" | "empty"
            )
        }),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRoutingEconomicSnapshot {
    /// Station exchange rate needed to normalize station-native multipliers in
    /// read models that do not have a model-specific pricing context.
    pub credit_per_cny: Option<f64>,
    pub group_binding_id: Option<String>,
    pub group_key_hash: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_name: Option<String>,
    /// Canonical coarse category resolved by persistence. A manual override
    /// takes precedence over collector inference before this snapshot is built.
    pub group_category: Option<String>,
    pub group_status: Option<String>,
    pub group_confidence: Option<f64>,
    pub group_checked_at: Option<String>,
    pub rate_multiplier: Option<f64>,
    pub manual_rate_multiplier: Option<f64>,
    pub manual_rate_updated_at: Option<String>,
    pub rate_source: Option<String>,
    pub rate_collected_at: Option<String>,
    pub key_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalRoutingCandidate {
    pub station_key_id: String,
    pub station_id: String,
    pub station_type: String,
    /// Trusted, non-secret provider capacity identity. These values are
    /// intentionally separate from endpoint and credential identity so the
    /// routing read model can explain correlated capacity failures without
    /// exposing URLs or secrets.
    pub capacity_provider_family: Option<String>,
    pub capacity_deployment_identity: Option<String>,
    pub capacity_region_identity: Option<String>,
    pub capacity_domain_revision: Option<i64>,
    pub station_account_concurrency_limit: Option<i64>,
    pub station_endpoint_revision: i64,
    pub sanitized_origin: String,
    pub upstream_api_format: crate::models::proxy::UpstreamApiFormat,
    pub routing_order: Option<i64>,
    pub priority: i64,
    pub max_concurrency: i64,
    pub load_factor: Option<i64>,
    pub schedulable: bool,
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
    pub station_name: String,
    pub key_name: String,
    pub capabilities: StationKeyCapabilities,
    pub health: Option<StationKeyHealth>,
    pub balance_snapshot: Option<RuntimeRoutingBalance>,
    pub economic_snapshot: Option<RuntimeRoutingEconomicSnapshot>,
    pub api_key: Option<String>,
    pub api_key_secret: Option<RuntimeRoutingSecret>,
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
    pub policy: Option<crate::models::routing_policy::RoutingPolicyConfigV1>,
    #[serde(default)]
    pub max_rate_multiplier: Option<f64>,
    #[serde(default)]
    pub routing_group_filter: Option<RoutingGroupFilter>,
    #[serde(default)]
    pub session_hash: Option<String>,
    #[serde(default)]
    pub previous_response_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteCandidateExplanation {
    pub station_key_id: String,
    pub station_id: String,
    pub station_name: String,
    pub key_name: String,
    pub accepted: bool,
    pub reasons: Vec<String>,
    pub rejection_reasons: Vec<String>,
    pub mapped_model: Option<String>,
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
    pub routing_group_scope: Option<RoutingGroupFilter>,
    pub routing_group_match: bool,
    pub top_k_rank: Option<i64>,
    pub slot_result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSimulationResult {
    pub preview_policy_version: String,
    pub capacity_mode: String,
    pub selected_capacity_acquired: bool,
    pub selected_station_key_id: Option<String>,
    pub selected_station_id: Option<String>,
    pub mapped_model: Option<String>,
    pub policy: RoutingPolicy,
    pub max_rate_multiplier: Option<f64>,
    pub routing_group_filter: RoutingGroupFilter,
    pub planner_error_code: Option<String>,
    pub candidates: Vec<RouteCandidateExplanation>,
    pub message: String,
}

#[cfg(test)]
mod automatic_scheduler_contract_tests {
    use super::*;

    #[test]
    fn all_groups_filter_serializes_as_stable_snake_case_string() {
        let filter = RoutingGroupFilter::AllGroups;

        let serialized = serde_json::to_value(filter).expect("serialize filter");

        assert_eq!(serialized, serde_json::json!("all_groups"));
    }

    #[test]
    fn group_type_filter_serializes_as_tagged_group_type() {
        let filter = RoutingGroupFilter::GroupType(PricingGroupType::Gpt);

        let serialized = serde_json::to_value(filter).expect("serialize filter");

        assert_eq!(serialized, serde_json::json!({ "group_type": "gpt" }));
    }

    #[test]
    fn image_generation_group_type_decodes_from_snake_case() {
        let group_type: PricingGroupType =
            serde_json::from_str("\"image_generation\"").expect("decode group type");

        assert_eq!(group_type, PricingGroupType::ImageGeneration);
    }

    #[test]
    fn routeable_settings_allow_unlimited_multiplier_ceiling() {
        let settings = AutomaticSchedulerSettings {
            max_rate_multiplier: None,
            routing_group_scope: RoutingGroupFilter::AllGroups,
            advanced: DispatchAlgorithmSettings::default(),
        };

        settings
            .validate_for_routing()
            .expect("missing multiplier ceiling should mean unlimited routing");
    }

    #[test]
    fn negative_runtime_balance_is_depleted_even_with_stale_normal_status() {
        let balance = RuntimeRoutingBalance {
            scope: "station".to_string(),
            value: Some(-0.05),
            currency: "USD".to_string(),
            low_balance_threshold: None,
            status: "normal".to_string(),
            collected_at: None,
        };

        assert!(balance.is_depleted());
    }

    #[test]
    fn positive_runtime_balance_with_low_status_remains_routeable() {
        let balance = RuntimeRoutingBalance {
            scope: "station".to_string(),
            value: Some(4.71),
            currency: "USD".to_string(),
            low_balance_threshold: Some(5.0),
            status: "low".to_string(),
            collected_at: None,
        };

        assert!(!balance.is_depleted());
    }

    #[test]
    fn positive_balance_wins_over_stale_depleted_status() {
        assert!(!balance_is_depleted(Some(4.71), Some("depleted")));
        assert!(!balance_is_depleted(Some(4.71), Some("EXHAUSTED")));
        assert!(!balance_is_depleted(Some(4.71), Some("empty")));
    }

    #[test]
    fn explicit_depleted_status_is_used_when_value_is_missing() {
        assert!(balance_is_depleted(None, Some("depleted")));
        assert!(balance_is_depleted(None, Some("EXHAUSTED")));
        assert!(balance_is_depleted(None, Some("empty")));
    }
}
