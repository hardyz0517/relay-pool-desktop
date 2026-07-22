use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PricingGroupType {
    Gpt,
    Claude,
    Gemini,
    Grok,
    ImageGeneration,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RoutingGroupFilter {
    #[default]
    AllGroups,
    UngroupedOnly,
    GroupBindingId(String),
    GroupIdHash(String),
    GroupType(PricingGroupType),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeRoutingSettings {
    pub policy: RoutingPolicy,
    pub max_rate_multiplier: Option<f64>,
    pub routing_group_filter: RoutingGroupFilter,
    pub scheduler_advanced_settings: SchedulerAdvancedSettings,
    pub allow_depleted_fallback: bool,
}

impl Default for RuntimeRoutingSettings {
    fn default() -> Self {
        Self {
            policy: RoutingPolicy::PriorityFallback,
            max_rate_multiplier: None,
            routing_group_filter: RoutingGroupFilter::default(),
            scheduler_advanced_settings: SchedulerAdvancedSettings::default(),
            allow_depleted_fallback: false,
        }
    }
}

impl Serialize for RoutingGroupFilter {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::AllGroups => serializer.serialize_str("all_groups"),
            Self::UngroupedOnly => serializer.serialize_str("ungrouped_only"),
            Self::GroupBindingId(id) => {
                serde_json::json!({ "group_binding_id": id }).serialize(serializer)
            }
            Self::GroupIdHash(hash) => {
                serde_json::json!({ "group_id_hash": hash }).serialize(serializer)
            }
            Self::GroupType(group_type) => {
                serde_json::json!({ "group_type": group_type }).serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for RoutingGroupFilter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::String(value) => match value.as_str() {
                "all_groups" => Ok(Self::AllGroups),
                "ungrouped_only" => Ok(Self::UngroupedOnly),
                other => Err(serde::de::Error::custom(format!(
                    "unknown routing group filter: {other}"
                ))),
            },
            Value::Object(mut object) => {
                if object.len() != 1 {
                    return Err(serde::de::Error::custom(
                        "routing group filter object must contain exactly one key",
                    ));
                }
                if let Some(value) = object.remove("group_binding_id") {
                    let id = value
                        .as_str()
                        .filter(|id| !id.trim().is_empty())
                        .ok_or_else(|| {
                            serde::de::Error::custom("group_binding_id must be a non-empty string")
                        })?;
                    return Ok(Self::GroupBindingId(id.to_string()));
                }
                if let Some(value) = object.remove("group_id_hash") {
                    let hash = value
                        .as_str()
                        .filter(|hash| !hash.trim().is_empty())
                        .ok_or_else(|| {
                            serde::de::Error::custom("group_id_hash must be a non-empty string")
                        })?;
                    return Ok(Self::GroupIdHash(hash.to_string()));
                }
                if let Some(value) = object.remove("group_type") {
                    let group_type = PricingGroupType::deserialize(value).map_err(|error| {
                        serde::de::Error::custom(format!("invalid group_type: {error}"))
                    })?;
                    return Ok(Self::GroupType(group_type));
                }
                Err(serde::de::Error::custom(
                    "unknown routing group filter object key",
                ))
            }
            _ => Err(serde::de::Error::custom(
                "routing group filter must be a string or object",
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg(test)]
pub struct AutomaticSchedulerSettings {
    pub max_rate_multiplier: Option<f64>,
    pub default_routing_group_filter: RoutingGroupFilter,
    pub advanced: SchedulerAdvancedSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerAdvancedSettings {
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

impl Default for SchedulerAdvancedSettings {
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
    MultiplierLimitNotConfigured,
    #[cfg(test)]
    InvalidMultiplierLimit,
    InvalidAdvancedSetting(&'static str),
}

#[cfg(test)]
impl AutomaticSchedulerSettings {
    pub fn validate_for_routing(&self) -> Result<(), SchedulerConfigError> {
        let Some(max_rate_multiplier) = self.max_rate_multiplier else {
            return Err(SchedulerConfigError::MultiplierLimitNotConfigured);
        };
        if !max_rate_multiplier.is_finite() || max_rate_multiplier < 0.0 {
            return Err(SchedulerConfigError::InvalidMultiplierLimit);
        }
        self.advanced.validate()
    }
}

impl SchedulerAdvancedSettings {
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
pub struct RuntimeRoutingSecret {
    pub id: String,
    pub scope: String,
    pub owner_id: String,
    pub kind: String,
    pub masked_value: String,
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRoutingCandidate {
    pub station_key_id: String,
    pub station_id: String,
    pub station_endpoint_revision: i64,
    pub upstream_base_url: String,
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
    pub api_key: Option<String>,
    pub api_key_secret: Option<RuntimeRoutingSecret>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoutingProxyDefaults {
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
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
    pub routing_group_scope: Option<RoutingGroupFilter>,
    pub routing_group_match: bool,
    pub group_id_hash: Option<String>,
    pub group_type: Option<PricingGroupType>,
    pub effective_multiplier_source: Option<String>,
    pub effective_multiplier_confidence: Option<f64>,
    pub scheduler_score: Option<f64>,
    pub scheduler_factors: Vec<String>,
    pub top_k_rank: Option<i64>,
    pub slot_result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSimulationResult {
    pub selected_station_key_id: Option<String>,
    pub selected_station_id: Option<String>,
    pub mapped_model: Option<String>,
    pub policy: RoutingPolicy,
    pub max_rate_multiplier: Option<f64>,
    pub routing_group_filter: RoutingGroupFilter,
    pub scheduler_error_code: Option<String>,
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
    fn routeable_settings_require_a_multiplier_ceiling() {
        let settings = AutomaticSchedulerSettings {
            max_rate_multiplier: None,
            default_routing_group_filter: RoutingGroupFilter::AllGroups,
            advanced: SchedulerAdvancedSettings::default(),
        };

        let error = settings
            .validate_for_routing()
            .expect_err("missing multiplier ceiling should fail routing validation");

        assert_eq!(error, SchedulerConfigError::MultiplierLimitNotConfigured);
    }
}
