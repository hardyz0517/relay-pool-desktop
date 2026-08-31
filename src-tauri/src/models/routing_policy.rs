use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;

/// Versioned managed-file envelope for the active routing policy aggregate.
/// The envelope keeps the CAS revision next to the user-editable policy so a
/// file import cannot accidentally overwrite a newer SQLite revision.
pub(crate) const ROUTING_POLICY_DOCUMENT_FORMAT_VERSION: u16 = 1;
pub(crate) const ROUTING_POLICY_CONFIG_VERSION_V2: u16 = 2;
pub(crate) const RETRY_FAILOVER_POLICY_VERSION_V2: u16 = 2;
pub(crate) const PROTECTION_PROFILE_VERSION_V2: u16 = 2;
pub(crate) const TIMEOUT_POLICY_VERSION_V2: u16 = 2;
pub(crate) const MAX_TOTAL_ATTEMPTS_HARD_CAP: u16 = 4;
pub(crate) const MAX_SAME_TARGET_CAPACITY_RETRIES_HARD_CAP: u16 = 2;
pub(crate) const MAX_CAPACITY_RETRY_WAIT_BUDGET_SECONDS_HARD_CAP: f64 = 2.0;
pub(crate) const MAX_PROTECTION_WINDOW_SAMPLES: u16 = 256;
pub(crate) const MAX_PROTECTION_WINDOW_SECONDS: f64 = 24.0 * 60.0 * 60.0;
pub(crate) const MAX_PROTECTION_HALF_OPEN_SUCCESSES: u8 = 16;
pub(crate) const MIN_ROUTING_TIMEOUT_SECONDS: f64 = 1.0;
pub(crate) const MAX_ROUTING_CONNECT_TIMEOUT_SECONDS: f64 = 120.0;
pub(crate) const MAX_ROUTING_FIRST_BYTE_TIMEOUT_SECONDS: f64 = 300.0;
pub(crate) const MAX_ROUTING_PRECOMMIT_TIMEOUT_SECONDS: f64 = 600.0;
pub(crate) const MAX_ROUTING_BUFFERED_EXECUTION_TIMEOUT_SECONDS: f64 = 1_800.0;
pub(crate) const MAX_ROUTING_STREAM_IDLE_TIMEOUT_SECONDS: f64 = 600.0;
const DEFAULT_OUTBOUND_PROXY_MODE: &str = "inherit";

fn default_outbound_proxy_mode() -> String {
    DEFAULT_OUTBOUND_PROXY_MODE.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RoutingPolicyDocumentV1 {
    pub(crate) format_version: u16,
    pub(crate) base_revision: u64,
    pub(crate) policy: RoutingPolicyDocumentPolicyV1,
}

/// Strict public-file representation of the legacy policy payload.
///
/// `RoutingPolicyConfigV1` is intentionally a storage compatibility type and
/// therefore owns defaults for historical SQLite rows.  It must not be used
/// to decode a user supplied document: absent fields in a document are an
/// authoring error, not an instruction to inherit a storage default.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RoutingPolicyDocumentPolicyV1 {
    pub(crate) version: u16,
    pub(crate) reliability_weight: u16,
    pub(crate) responsiveness_weight: u16,
    pub(crate) cost_weight: u16,
    pub(crate) preference_weight: u16,
    pub(crate) max_candidates: u16,
    pub(crate) exploration_share_basis_points: u16,
    pub(crate) allow_depleted_fallback: bool,
    pub(crate) affinity_enabled: bool,
    pub(crate) affinity_ttl_seconds: u32,
    #[serde(deserialize_with = "deserialize_required_option")]
    pub(crate) max_rate_multiplier: Option<f64>,
    pub(crate) routing_group_filter: RoutingGroupFilter,
    pub(crate) outbound_proxy_mode: String,
    #[serde(deserialize_with = "deserialize_required_option")]
    pub(crate) outbound_proxy_url: Option<String>,
}

impl Default for RoutingPolicyDocumentPolicyV1 {
    fn default() -> Self {
        let value = RoutingPolicyConfigV1::default();
        Self::from_storage(value)
    }
}

impl RoutingPolicyDocumentPolicyV1 {
    fn from_storage(value: RoutingPolicyConfigV1) -> Self {
        Self {
            version: value.version,
            reliability_weight: value.reliability_weight,
            responsiveness_weight: value.responsiveness_weight,
            cost_weight: value.cost_weight,
            preference_weight: value.preference_weight,
            max_candidates: value.max_candidates,
            exploration_share_basis_points: value.exploration_share_basis_points,
            allow_depleted_fallback: value.allow_depleted_fallback,
            affinity_enabled: value.affinity_enabled,
            affinity_ttl_seconds: value.affinity_ttl_seconds,
            max_rate_multiplier: value.max_rate_multiplier,
            routing_group_filter: value.routing_group_filter,
            outbound_proxy_mode: value.outbound_proxy_mode,
            outbound_proxy_url: value.outbound_proxy_url,
        }
    }

    fn into_storage(self) -> RoutingPolicyConfigV1 {
        RoutingPolicyConfigV1 {
            version: self.version,
            reliability_weight: self.reliability_weight,
            responsiveness_weight: self.responsiveness_weight,
            cost_weight: self.cost_weight,
            preference_weight: self.preference_weight,
            max_candidates: self.max_candidates,
            exploration_share_basis_points: self.exploration_share_basis_points,
            allow_depleted_fallback: self.allow_depleted_fallback,
            affinity_enabled: self.affinity_enabled,
            affinity_ttl_seconds: self.affinity_ttl_seconds,
            max_rate_multiplier: self.max_rate_multiplier,
            routing_group_filter: self.routing_group_filter,
            outbound_proxy_mode: self.outbound_proxy_mode,
            outbound_proxy_url: self.outbound_proxy_url,
        }
    }

    #[cfg(test)]
    fn validate(&self) -> Result<(), &'static str> {
        self.clone().into_storage().validate()
    }
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
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(value) => match value.as_str() {
                "all_groups" => Ok(Self::AllGroups),
                "ungrouped_only" => Ok(Self::UngroupedOnly),
                other => Err(serde::de::Error::custom(format!(
                    "unknown routing group filter: {other}"
                ))),
            },
            serde_json::Value::Object(mut object) => {
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
#[serde(deny_unknown_fields)]
pub(crate) struct RoutingPolicyConfigV1 {
    pub version: u16,
    pub reliability_weight: u16,
    pub responsiveness_weight: u16,
    pub cost_weight: u16,
    pub preference_weight: u16,
    pub max_candidates: u16,
    pub exploration_share_basis_points: u16,
    pub allow_depleted_fallback: bool,
    pub affinity_enabled: bool,
    pub affinity_ttl_seconds: u32,
    #[serde(default)]
    pub max_rate_multiplier: Option<f64>,
    #[serde(default)]
    pub routing_group_filter: RoutingGroupFilter,
    /// The outbound proxy for requests sent through the local routing gateway.
    /// `inherit` resolves to the global network setting at execution time.
    #[serde(default = "default_outbound_proxy_mode")]
    pub outbound_proxy_mode: String,
    #[serde(default)]
    pub outbound_proxy_url: Option<String>,
}

/// A stable, field-addressable validation failure returned by the versioned
/// policy domain.  The public document/IPC layer can map these identifiers to
/// localized messages without parsing an error string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RoutingPolicyFieldValidationError {
    pub(crate) field: &'static str,
    pub(crate) code: &'static str,
    pub(crate) message_key: &'static str,
}

impl RoutingPolicyFieldValidationError {
    const fn new(field: &'static str, code: &'static str, message_key: &'static str) -> Self {
        Self {
            field,
            code,
            message_key,
        }
    }
}

/// Request-level retry and capacity failover controls.  These are deliberately
/// limited to the four controls backed by existing production capacity paths;
/// transport timeouts and generic failure-domain controls are not part of this
/// version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RetryFailoverPolicyV2 {
    pub(crate) version: u16,
    pub(crate) max_total_attempts: u16,
    pub(crate) max_same_target_capacity_retries: u16,
    pub(crate) capacity_retry_wait_budget_seconds: f64,
    pub(crate) allow_cross_capacity_domain_fallback: bool,
}

impl Default for RetryFailoverPolicyV2 {
    fn default() -> Self {
        Self {
            version: RETRY_FAILOVER_POLICY_VERSION_V2,
            max_total_attempts: u16::from(MAX_TOTAL_ATTEMPTS_HARD_CAP),
            max_same_target_capacity_retries: u16::from(MAX_SAME_TARGET_CAPACITY_RETRIES_HARD_CAP),
            capacity_retry_wait_budget_seconds: MAX_CAPACITY_RETRY_WAIT_BUDGET_SECONDS_HARD_CAP,
            allow_cross_capacity_domain_fallback: true,
        }
    }
}

impl RetryFailoverPolicyV2 {
    pub(crate) fn validate(&self) -> Result<(), RoutingPolicyFieldValidationError> {
        if self.version != RETRY_FAILOVER_POLICY_VERSION_V2 {
            return Err(RoutingPolicyFieldValidationError::new(
                "retryFailover.version",
                "unsupported_version",
                "routing.retryFailover.version.unsupported",
            ));
        }
        if !(1..=MAX_TOTAL_ATTEMPTS_HARD_CAP).contains(&self.max_total_attempts) {
            return Err(RoutingPolicyFieldValidationError::new(
                "retryFailover.maxTotalAttempts",
                "out_of_range",
                "routing.retryFailover.maxTotalAttempts.range",
            ));
        }
        if self.max_same_target_capacity_retries > MAX_SAME_TARGET_CAPACITY_RETRIES_HARD_CAP {
            return Err(RoutingPolicyFieldValidationError::new(
                "retryFailover.maxSameTargetCapacityRetries",
                "out_of_range",
                "routing.retryFailover.maxSameTargetCapacityRetries.range",
            ));
        }
        if self.max_same_target_capacity_retries >= self.max_total_attempts {
            return Err(RoutingPolicyFieldValidationError::new(
                "retryFailover.maxSameTargetCapacityRetries",
                "must_be_less_than_max_total_attempts",
                "routing.retryFailover.maxSameTargetCapacityRetries.lessThanTotal",
            ));
        }
        if !self.capacity_retry_wait_budget_seconds.is_finite()
            || !(0.0..=MAX_CAPACITY_RETRY_WAIT_BUDGET_SECONDS_HARD_CAP)
                .contains(&self.capacity_retry_wait_budget_seconds)
        {
            return Err(RoutingPolicyFieldValidationError::new(
                "retryFailover.capacityRetryWaitBudgetSeconds",
                "out_of_range",
                "routing.retryFailover.capacityRetryWaitBudgetSeconds.range",
            ));
        }
        Ok(())
    }

    pub(crate) fn capacity_retry_wait_budget_millis(&self) -> u64 {
        (self.capacity_retry_wait_budget_seconds * 1_000.0).round() as u64
    }
}

/// Versioned cross-request health-protection controls.  The profile is
/// deliberately nested in the routing-policy aggregate so it follows the
/// same revision/CAS and migration boundary as the other routing controls.
/// It can only make admission more conservative; it never grants permission
/// to replay a request or bypass the canonical replay gate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProtectionProfileConfigV2 {
    pub(crate) version: u16,
    pub(crate) enabled: bool,
    pub(crate) window_max_samples: u16,
    pub(crate) window_seconds: f64,
    pub(crate) min_samples: u16,
    pub(crate) failure_threshold_percent: u8,
    pub(crate) half_open_successes_to_close: u8,
}

impl Default for ProtectionProfileConfigV2 {
    fn default() -> Self {
        Self {
            version: PROTECTION_PROFILE_VERSION_V2,
            enabled: false,
            window_max_samples: 64,
            window_seconds: 5.0 * 60.0,
            min_samples: 5,
            failure_threshold_percent: 60,
            half_open_successes_to_close: 2,
        }
    }
}

impl ProtectionProfileConfigV2 {
    pub(crate) fn validate(&self) -> Result<(), RoutingPolicyFieldValidationError> {
        if self.version != PROTECTION_PROFILE_VERSION_V2 {
            return Err(RoutingPolicyFieldValidationError::new(
                "protectionProfile.version",
                "unsupported_version",
                "routing.protectionProfile.version.unsupported",
            ));
        }
        if self.window_max_samples == 0 || self.window_max_samples > MAX_PROTECTION_WINDOW_SAMPLES {
            return Err(RoutingPolicyFieldValidationError::new(
                "protectionProfile.windowMaxSamples",
                "out_of_range",
                "routing.protectionProfile.windowMaxSamples.range",
            ));
        }
        if !self.window_seconds.is_finite()
            || !(f64::MIN_POSITIVE..=MAX_PROTECTION_WINDOW_SECONDS).contains(&self.window_seconds)
        {
            return Err(RoutingPolicyFieldValidationError::new(
                "protectionProfile.windowSeconds",
                "out_of_range",
                "routing.protectionProfile.windowSeconds.range",
            ));
        }
        if self.min_samples == 0 || self.min_samples > self.window_max_samples {
            return Err(RoutingPolicyFieldValidationError::new(
                "protectionProfile.minSamples",
                "out_of_range",
                "routing.protectionProfile.minSamples.range",
            ));
        }
        if !(1..=100).contains(&self.failure_threshold_percent) {
            return Err(RoutingPolicyFieldValidationError::new(
                "protectionProfile.failureThresholdPercent",
                "out_of_range",
                "routing.protectionProfile.failureThresholdPercent.range",
            ));
        }
        if self.half_open_successes_to_close == 0
            || self.half_open_successes_to_close > MAX_PROTECTION_HALF_OPEN_SUCCESSES
        {
            return Err(RoutingPolicyFieldValidationError::new(
                "protectionProfile.halfOpenSuccessesToClose",
                "out_of_range",
                "routing.protectionProfile.halfOpenSuccessesToClose.range",
            ));
        }
        Ok(())
    }

    pub(crate) fn window_millis(&self) -> i64 {
        (self.window_seconds * 1_000.0).round() as i64
    }
}

/// User-controlled proxy transport timeouts. Values are persisted in seconds
/// and compiled into runtime durations only when a new local proxy starts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TimeoutPolicyV2 {
    pub(crate) version: u16,
    pub(crate) connect_seconds: f64,
    pub(crate) first_byte_seconds: f64,
    pub(crate) precommit_seconds: f64,
    pub(crate) buffered_execution_seconds: f64,
    pub(crate) stream_idle_seconds: f64,
}

impl Default for TimeoutPolicyV2 {
    fn default() -> Self {
        Self {
            version: TIMEOUT_POLICY_VERSION_V2,
            connect_seconds: 10.0,
            first_byte_seconds: 30.0,
            precommit_seconds: 60.0,
            buffered_execution_seconds: 300.0,
            stream_idle_seconds: 90.0,
        }
    }
}

impl TimeoutPolicyV2 {
    pub(crate) fn validate(&self) -> Result<(), RoutingPolicyFieldValidationError> {
        if self.version != TIMEOUT_POLICY_VERSION_V2 {
            return Err(RoutingPolicyFieldValidationError::new(
                "timeoutPolicy.version",
                "unsupported_version",
                "routing.timeoutPolicy.version.unsupported",
            ));
        }
        for (field, value, maximum) in [
            (
                "timeoutPolicy.connectSeconds",
                self.connect_seconds,
                MAX_ROUTING_CONNECT_TIMEOUT_SECONDS,
            ),
            (
                "timeoutPolicy.firstByteSeconds",
                self.first_byte_seconds,
                MAX_ROUTING_FIRST_BYTE_TIMEOUT_SECONDS,
            ),
            (
                "timeoutPolicy.precommitSeconds",
                self.precommit_seconds,
                MAX_ROUTING_PRECOMMIT_TIMEOUT_SECONDS,
            ),
            (
                "timeoutPolicy.bufferedExecutionSeconds",
                self.buffered_execution_seconds,
                MAX_ROUTING_BUFFERED_EXECUTION_TIMEOUT_SECONDS,
            ),
            (
                "timeoutPolicy.streamIdleSeconds",
                self.stream_idle_seconds,
                MAX_ROUTING_STREAM_IDLE_TIMEOUT_SECONDS,
            ),
        ] {
            if !value.is_finite() || !(MIN_ROUTING_TIMEOUT_SECONDS..=maximum).contains(&value) {
                return Err(RoutingPolicyFieldValidationError::new(
                    field,
                    "out_of_range",
                    "routing.timeoutPolicy.range",
                ));
            }
        }
        if self.precommit_seconds > self.buffered_execution_seconds {
            return Err(RoutingPolicyFieldValidationError::new(
                "timeoutPolicy.precommitSeconds",
                "must_not_exceed_buffered_execution",
                "routing.timeoutPolicy.precommitBeforeBuffered",
            ));
        }
        Ok(())
    }

    pub(crate) fn connect_millis(&self) -> u64 {
        (self.connect_seconds * 1_000.0).round() as u64
    }
    pub(crate) fn first_byte_millis(&self) -> u64 {
        (self.first_byte_seconds * 1_000.0).round() as u64
    }
    pub(crate) fn precommit_millis(&self) -> u64 {
        (self.precommit_seconds * 1_000.0).round() as u64
    }
    pub(crate) fn buffered_execution_millis(&self) -> u64 {
        (self.buffered_execution_seconds * 1_000.0).round() as u64
    }
    pub(crate) fn stream_idle_millis(&self) -> u64 {
        (self.stream_idle_seconds * 1_000.0).round() as u64
    }
}

/// Current versioned policy configuration.  V1 remains an input-only
/// compatibility type; runtime consumers should use this shape after the
/// additive upgrader has run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RoutingPolicyConfigV2 {
    pub(crate) version: u16,
    pub(crate) reliability_weight: u16,
    pub(crate) responsiveness_weight: u16,
    pub(crate) cost_weight: u16,
    pub(crate) preference_weight: u16,
    pub(crate) max_candidates: u16,
    pub(crate) exploration_share_basis_points: u16,
    pub(crate) allow_depleted_fallback: bool,
    pub(crate) affinity_enabled: bool,
    pub(crate) affinity_ttl_seconds: u32,
    #[serde(deserialize_with = "deserialize_required_option")]
    pub(crate) max_rate_multiplier: Option<f64>,
    pub(crate) routing_group_filter: RoutingGroupFilter,
    pub(crate) outbound_proxy_mode: String,
    #[serde(deserialize_with = "deserialize_required_option")]
    pub(crate) outbound_proxy_url: Option<String>,
    pub(crate) retry_failover: RetryFailoverPolicyV2,
    pub(crate) protection_profile: ProtectionProfileConfigV2,
    pub(crate) timeout_policy: TimeoutPolicyV2,
}

fn deserialize_required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

impl Default for RoutingPolicyConfigV2 {
    fn default() -> Self {
        let v1 = RoutingPolicyConfigV1::default();
        Self {
            version: ROUTING_POLICY_CONFIG_VERSION_V2,
            reliability_weight: v1.reliability_weight,
            responsiveness_weight: v1.responsiveness_weight,
            cost_weight: v1.cost_weight,
            preference_weight: v1.preference_weight,
            max_candidates: v1.max_candidates,
            exploration_share_basis_points: v1.exploration_share_basis_points,
            allow_depleted_fallback: v1.allow_depleted_fallback,
            affinity_enabled: v1.affinity_enabled,
            affinity_ttl_seconds: v1.affinity_ttl_seconds,
            max_rate_multiplier: v1.max_rate_multiplier,
            routing_group_filter: v1.routing_group_filter,
            outbound_proxy_mode: v1.outbound_proxy_mode,
            outbound_proxy_url: v1.outbound_proxy_url,
            retry_failover: RetryFailoverPolicyV2::default(),
            protection_profile: ProtectionProfileConfigV2::default(),
            timeout_policy: TimeoutPolicyV2::default(),
        }
    }
}

impl RoutingPolicyConfigV2 {
    /// Decode the persisted aggregate at the storage boundary. Historical
    /// rows may still contain the V1 shape, but every caller receives the
    /// canonical V2 domain object after this single upgrade point.
    pub(crate) fn from_stored_value(
        value: &serde_json::Value,
    ) -> Result<Self, RoutingPolicyFieldValidationError> {
        match value.get("version").and_then(serde_json::Value::as_u64) {
            Some(3) => {
                let v3 = serde_json::from_value::<RoutingPolicyConfigV3>(value.clone()).map_err(
                    |_| {
                        RoutingPolicyFieldValidationError::new(
                            "policy",
                            "invalid_v3_policy",
                            "routing.policy.invalid",
                        )
                    },
                )?;
                v3.validate()?;
                Self::from_v3_compat(&v3)
            }
            Some(2) => {
                let policy = serde_json::from_value::<Self>(upgrade_v2_duration_units(value))
                    .map_err(|_| {
                        RoutingPolicyFieldValidationError::new(
                            "policy",
                            "invalid_v2_policy",
                            "routing.policy.invalid",
                        )
                    })?;
                policy.validate().map(|()| policy).map_err(|error| error)
            }
            Some(1) => {
                let legacy = serde_json::from_value::<RoutingPolicyConfigV1>(value.clone())
                    .map_err(|_| {
                        RoutingPolicyFieldValidationError::new(
                            "policy",
                            "invalid_v1_policy",
                            "routing.policy.invalid",
                        )
                    })?;
                Self::from_v1(&legacy)
            }
            Some(_) => Err(RoutingPolicyFieldValidationError::new(
                "version",
                "unsupported_version",
                "routing.policy.version.unsupported",
            )),
            None => Err(RoutingPolicyFieldValidationError::new(
                "version",
                "required",
                "routing.policy.version.required",
            )),
        }
    }

    /// Temporary execution-shape projection used while the planner/admission
    /// structs finish moving to the v3 policy type.  It deliberately projects
    /// only fields that the legacy snapshot can represent: retry budget and
    /// common score/transport controls.  Removed exploration and capacity
    /// domain controls are hard-disabled, and the old error-rate switch is not
    /// re-enabled by a v3 document.
    fn from_v3_compat(
        value: &RoutingPolicyConfigV3,
    ) -> Result<Self, RoutingPolicyFieldValidationError> {
        let max_total_attempts = value.retry.max_total_attempts().try_into().map_err(|_| {
            RoutingPolicyFieldValidationError::new(
                "retry.maxRetryCount",
                "out_of_range",
                "routing.retry.maxRetryCount.range",
            )
        })?;
        let projected = Self {
            version: ROUTING_POLICY_CONFIG_VERSION_V2,
            reliability_weight: value.reliability_weight,
            responsiveness_weight: value.responsiveness_weight,
            cost_weight: value.cost_weight,
            preference_weight: value.preference_weight,
            // v3 intentionally has no candidate/exploration control. Keep
            // the bounded legacy field at its hard maximum so the planner can
            // inspect every eligible key before deterministic sorting.
            max_candidates: 1_024,
            exploration_share_basis_points: 0,
            allow_depleted_fallback: value.allow_depleted_fallback,
            affinity_enabled: value.affinity_enabled,
            affinity_ttl_seconds: value.affinity_ttl_seconds,
            max_rate_multiplier: value.max_rate_multiplier,
            routing_group_filter: value.routing_group_filter.clone(),
            outbound_proxy_mode: value.outbound_proxy_mode.clone(),
            outbound_proxy_url: value.outbound_proxy_url.clone(),
            retry_failover: RetryFailoverPolicyV2 {
                version: RETRY_FAILOVER_POLICY_VERSION_V2,
                max_total_attempts,
                max_same_target_capacity_retries: 0,
                capacity_retry_wait_budget_seconds: 0.0,
                allow_cross_capacity_domain_fallback: false,
            },
            protection_profile: ProtectionProfileConfigV2 {
                version: PROTECTION_PROFILE_VERSION_V2,
                // v3 circuit state is not the legacy error-rate switch. Keep
                // this false until the v3 circuit admission bridge is used.
                enabled: false,
                window_max_samples: 64,
                window_seconds: 300.0,
                min_samples: 1,
                failure_threshold_percent: 100,
                half_open_successes_to_close: value.circuit_breaker.recovery_success_threshold,
            },
            timeout_policy: value.timeout_policy.clone(),
        };
        projected.validate()?;
        Ok(projected)
    }

    /// Upgrade the existing V1 configuration without changing its routing
    /// factors.  The retry values are the already-verified production
    /// baseline, so upgrading a stored policy is behaviorally additive.
    pub(crate) fn from_v1(
        value: &RoutingPolicyConfigV1,
    ) -> Result<Self, RoutingPolicyFieldValidationError> {
        value.validate().map_err(|_| {
            RoutingPolicyFieldValidationError::new(
                "policy",
                "invalid_v1_policy",
                "routing.policy.invalid",
            )
        })?;
        let v1 = value.clone();
        let upgraded = Self {
            version: ROUTING_POLICY_CONFIG_VERSION_V2,
            reliability_weight: v1.reliability_weight,
            responsiveness_weight: v1.responsiveness_weight,
            cost_weight: v1.cost_weight,
            preference_weight: v1.preference_weight,
            max_candidates: v1.max_candidates,
            exploration_share_basis_points: v1.exploration_share_basis_points,
            allow_depleted_fallback: v1.allow_depleted_fallback,
            affinity_enabled: v1.affinity_enabled,
            affinity_ttl_seconds: v1.affinity_ttl_seconds,
            max_rate_multiplier: v1.max_rate_multiplier,
            routing_group_filter: v1.routing_group_filter,
            outbound_proxy_mode: v1.outbound_proxy_mode,
            outbound_proxy_url: v1.outbound_proxy_url,
            retry_failover: RetryFailoverPolicyV2::default(),
            protection_profile: ProtectionProfileConfigV2::default(),
            timeout_policy: TimeoutPolicyV2::default(),
        };
        upgraded.validate()?;
        Ok(upgraded)
    }

    pub(crate) fn validate(&self) -> Result<(), RoutingPolicyFieldValidationError> {
        if self.version != ROUTING_POLICY_CONFIG_VERSION_V2 {
            return Err(RoutingPolicyFieldValidationError::new(
                "version",
                "unsupported_version",
                "routing.policy.version.unsupported",
            ));
        }
        let base = RoutingPolicyConfigV1 {
            version: 1,
            reliability_weight: self.reliability_weight,
            responsiveness_weight: self.responsiveness_weight,
            cost_weight: self.cost_weight,
            preference_weight: self.preference_weight,
            max_candidates: self.max_candidates,
            exploration_share_basis_points: self.exploration_share_basis_points,
            allow_depleted_fallback: self.allow_depleted_fallback,
            affinity_enabled: self.affinity_enabled,
            affinity_ttl_seconds: self.affinity_ttl_seconds,
            max_rate_multiplier: self.max_rate_multiplier,
            routing_group_filter: self.routing_group_filter.clone(),
            outbound_proxy_mode: self.outbound_proxy_mode.clone(),
            outbound_proxy_url: self.outbound_proxy_url.clone(),
        };
        base.validate().map_err(|_| {
            RoutingPolicyFieldValidationError::new(
                "policy",
                "invalid_base_policy",
                "routing.policy.invalid",
            )
        })?;
        self.retry_failover.validate()?;
        self.protection_profile
            .validate()
            .and_then(|()| self.timeout_policy.validate())
    }
}

/// The routing-policy contract used by the v3 planner.  V2 remains available
/// only as an explicit migration input; production callers must not silently
/// decode this shape through the v2 compatibility path.
pub(crate) const ROUTING_POLICY_CONFIG_VERSION_V3: u16 = 3;
pub(crate) const ROUTING_POLICY_RETRY_VERSION_V3: u16 = 1;
pub(crate) const ROUTING_POLICY_CIRCUIT_BREAKER_VERSION_V1: u16 = 1;
pub(crate) const DEFAULT_REAL_TRAFFIC_PERCENT: u8 = 70;
pub(crate) const DEFAULT_MONITORING_PERCENT: u8 = 30;
pub(crate) const DEFAULT_HISTORICAL_MINIMUM_SAMPLES: u16 = 15;
pub(crate) const DEFAULT_RECENT_MINIMUM_SAMPLES: u16 = 5;
pub(crate) const DEFAULT_OPTIMISTIC_RELIABILITY_PERCENT: u8 = 95;
pub(crate) const DEFAULT_OPTIMISTIC_LATENCY_MS: u32 = 2_500;
pub(crate) const DEFAULT_MAX_RETRY_COUNT: u16 = 3;
pub(crate) const DEFAULT_CONSECUTIVE_FAILURE_THRESHOLD: u16 = 3;
pub(crate) const DEFAULT_RECOVERY_SUCCESS_THRESHOLD: u8 = 2;
pub(crate) const DEFAULT_RECOVERY_WAIT_SECONDS: u32 = 30;
pub(crate) const MAX_RETRY_COUNT_HARD_CAP: u16 = MAX_TOTAL_ATTEMPTS_HARD_CAP - 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReliabilitySourceWeightsV3 {
    pub(crate) real_traffic_percent: u8,
    pub(crate) monitoring_percent: u8,
}

impl Default for ReliabilitySourceWeightsV3 {
    fn default() -> Self {
        Self {
            real_traffic_percent: DEFAULT_REAL_TRAFFIC_PERCENT,
            monitoring_percent: DEFAULT_MONITORING_PERCENT,
        }
    }
}

impl ReliabilitySourceWeightsV3 {
    pub(crate) fn validate(&self) -> Result<(), RoutingPolicyFieldValidationError> {
        if u16::from(self.real_traffic_percent) + u16::from(self.monitoring_percent) != 100 {
            return Err(RoutingPolicyFieldValidationError::new(
                "reliabilitySourceWeights.monitoringPercent",
                "sum_must_equal_100",
                "routing.reliabilitySourceWeights.sum",
            ));
        }
        Ok(())
    }

    pub(crate) fn real_traffic_basis_points(&self) -> u16 {
        u16::from(self.real_traffic_percent) * 100
    }

    pub(crate) fn monitoring_basis_points(&self) -> u16 {
        u16::from(self.monitoring_percent) * 100
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReliabilitySamplingPolicyV3 {
    pub(crate) historical_minimum_samples: u16,
    pub(crate) recent_minimum_samples: u16,
    pub(crate) optimistic_reliability_percent: u8,
    pub(crate) optimistic_latency_ms: u32,
}

impl Default for ReliabilitySamplingPolicyV3 {
    fn default() -> Self {
        Self {
            historical_minimum_samples: DEFAULT_HISTORICAL_MINIMUM_SAMPLES,
            recent_minimum_samples: DEFAULT_RECENT_MINIMUM_SAMPLES,
            optimistic_reliability_percent: DEFAULT_OPTIMISTIC_RELIABILITY_PERCENT,
            optimistic_latency_ms: DEFAULT_OPTIMISTIC_LATENCY_MS,
        }
    }
}

impl ReliabilitySamplingPolicyV3 {
    pub(crate) fn validate(&self) -> Result<(), RoutingPolicyFieldValidationError> {
        if !(1..=10_000).contains(&self.historical_minimum_samples) {
            return Err(RoutingPolicyFieldValidationError::new(
                "reliabilitySampling.historicalMinimumSamples",
                "out_of_range",
                "routing.reliabilitySampling.historicalMinimumSamples.range",
            ));
        }
        if !(1..=10_000).contains(&self.recent_minimum_samples) {
            return Err(RoutingPolicyFieldValidationError::new(
                "reliabilitySampling.recentMinimumSamples",
                "out_of_range",
                "routing.reliabilitySampling.recentMinimumSamples.range",
            ));
        }
        if self.optimistic_reliability_percent > 100 {
            return Err(RoutingPolicyFieldValidationError::new(
                "reliabilitySampling.optimisticReliabilityPercent",
                "out_of_range",
                "routing.reliabilitySampling.optimisticReliabilityPercent.range",
            ));
        }
        if !(100..=120_000).contains(&self.optimistic_latency_ms) {
            return Err(RoutingPolicyFieldValidationError::new(
                "reliabilitySampling.optimisticLatencyMs",
                "out_of_range",
                "routing.reliabilitySampling.optimisticLatencyMs.range",
            ));
        }
        Ok(())
    }

    pub(crate) fn optimistic_reliability_basis_points(&self) -> u16 {
        u16::from(self.optimistic_reliability_percent) * 100
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RetryPolicyV3 {
    pub(crate) version: u16,
    pub(crate) max_retry_count: u16,
    pub(crate) consecutive_failure_threshold: u16,
}

impl Default for RetryPolicyV3 {
    fn default() -> Self {
        Self {
            version: ROUTING_POLICY_RETRY_VERSION_V3,
            max_retry_count: DEFAULT_MAX_RETRY_COUNT,
            consecutive_failure_threshold: DEFAULT_CONSECUTIVE_FAILURE_THRESHOLD,
        }
    }
}

impl RetryPolicyV3 {
    pub(crate) fn validate(&self) -> Result<(), RoutingPolicyFieldValidationError> {
        if self.version != ROUTING_POLICY_RETRY_VERSION_V3 {
            return Err(RoutingPolicyFieldValidationError::new(
                "retry.version",
                "unsupported_version",
                "routing.retry.version.unsupported",
            ));
        }
        if self.max_retry_count > MAX_RETRY_COUNT_HARD_CAP {
            return Err(RoutingPolicyFieldValidationError::new(
                "retry.maxRetryCount",
                "out_of_range",
                "routing.retry.maxRetryCount.range",
            ));
        }
        if !(1..=10).contains(&self.consecutive_failure_threshold) {
            return Err(RoutingPolicyFieldValidationError::new(
                "retry.consecutiveFailureThreshold",
                "out_of_range",
                "routing.retry.consecutiveFailureThreshold.range",
            ));
        }
        Ok(())
    }

    pub(crate) fn max_total_attempts(&self) -> u32 {
        u32::from(self.max_retry_count) + 1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CircuitBreakerPolicyV3 {
    pub(crate) version: u16,
    pub(crate) recovery_success_threshold: u8,
    pub(crate) recovery_wait_seconds: u32,
}

impl Default for CircuitBreakerPolicyV3 {
    fn default() -> Self {
        Self {
            version: ROUTING_POLICY_CIRCUIT_BREAKER_VERSION_V1,
            recovery_success_threshold: DEFAULT_RECOVERY_SUCCESS_THRESHOLD,
            recovery_wait_seconds: DEFAULT_RECOVERY_WAIT_SECONDS,
        }
    }
}

impl CircuitBreakerPolicyV3 {
    pub(crate) fn validate(&self) -> Result<(), RoutingPolicyFieldValidationError> {
        if self.version != ROUTING_POLICY_CIRCUIT_BREAKER_VERSION_V1 {
            return Err(RoutingPolicyFieldValidationError::new(
                "circuitBreaker.version",
                "unsupported_version",
                "routing.circuitBreaker.version.unsupported",
            ));
        }
        if !(1..=MAX_PROTECTION_HALF_OPEN_SUCCESSES).contains(&self.recovery_success_threshold) {
            return Err(RoutingPolicyFieldValidationError::new(
                "circuitBreaker.recoverySuccessThreshold",
                "out_of_range",
                "routing.circuitBreaker.recoverySuccessThreshold.range",
            ));
        }
        if !(5..=3_600).contains(&self.recovery_wait_seconds) {
            return Err(RoutingPolicyFieldValidationError::new(
                "circuitBreaker.recoveryWaitSeconds",
                "out_of_range",
                "routing.circuitBreaker.recoveryWaitSeconds.range",
            ));
        }
        Ok(())
    }
}

/// Result of the one-time V2 -> V3 upgrade.  Audit fields intentionally name
/// only policy concepts and never contain user secrets or raw JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingPolicyV3UpgradeAudit {
    pub(crate) from_version: u16,
    pub(crate) to_version: u16,
    pub(crate) discarded_fields: Vec<&'static str>,
    pub(crate) defaulted_fields: Vec<&'static str>,
    pub(crate) semantic_changes: Vec<&'static str>,
    pub(crate) quality_rebuild_required: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoutingPolicyV3Upgrade {
    pub(crate) policy: RoutingPolicyConfigV3,
    pub(crate) audit: RoutingPolicyV3UpgradeAudit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RoutingPolicyConfigV3 {
    pub(crate) version: u16,
    pub(crate) reliability_weight: u16,
    pub(crate) responsiveness_weight: u16,
    pub(crate) cost_weight: u16,
    pub(crate) preference_weight: u16,
    pub(crate) allow_depleted_fallback: bool,
    pub(crate) affinity_enabled: bool,
    pub(crate) affinity_ttl_seconds: u32,
    #[serde(deserialize_with = "deserialize_required_option")]
    pub(crate) max_rate_multiplier: Option<f64>,
    pub(crate) routing_group_filter: RoutingGroupFilter,
    pub(crate) outbound_proxy_mode: String,
    #[serde(deserialize_with = "deserialize_required_option")]
    pub(crate) outbound_proxy_url: Option<String>,
    pub(crate) reliability_source_weights: ReliabilitySourceWeightsV3,
    pub(crate) reliability_sampling: ReliabilitySamplingPolicyV3,
    pub(crate) retry: RetryPolicyV3,
    pub(crate) circuit_breaker: CircuitBreakerPolicyV3,
    pub(crate) timeout_policy: TimeoutPolicyV2,
}

impl Default for RoutingPolicyConfigV3 {
    fn default() -> Self {
        let v1 = RoutingPolicyConfigV1::default();
        Self {
            version: ROUTING_POLICY_CONFIG_VERSION_V3,
            reliability_weight: v1.reliability_weight,
            responsiveness_weight: v1.responsiveness_weight,
            cost_weight: v1.cost_weight,
            preference_weight: v1.preference_weight,
            allow_depleted_fallback: v1.allow_depleted_fallback,
            affinity_enabled: v1.affinity_enabled,
            affinity_ttl_seconds: v1.affinity_ttl_seconds,
            max_rate_multiplier: v1.max_rate_multiplier,
            routing_group_filter: v1.routing_group_filter,
            outbound_proxy_mode: v1.outbound_proxy_mode,
            outbound_proxy_url: v1.outbound_proxy_url,
            reliability_source_weights: ReliabilitySourceWeightsV3::default(),
            reliability_sampling: ReliabilitySamplingPolicyV3::default(),
            retry: RetryPolicyV3::default(),
            circuit_breaker: CircuitBreakerPolicyV3::default(),
            timeout_policy: TimeoutPolicyV2::default(),
        }
    }
}

impl RoutingPolicyConfigV3 {
    pub(crate) fn validate(&self) -> Result<(), RoutingPolicyFieldValidationError> {
        if self.version != ROUTING_POLICY_CONFIG_VERSION_V3 {
            return Err(RoutingPolicyFieldValidationError::new(
                "version",
                "unsupported_version",
                "routing.policy.version.unsupported",
            ));
        }
        let base = RoutingPolicyConfigV1 {
            version: 1,
            reliability_weight: self.reliability_weight,
            responsiveness_weight: self.responsiveness_weight,
            cost_weight: self.cost_weight,
            preference_weight: self.preference_weight,
            // V3 removes these user fields.  The values below only reuse the
            // existing common-field validator and never enter the V3 JSON.
            max_candidates: 1,
            exploration_share_basis_points: 0,
            allow_depleted_fallback: self.allow_depleted_fallback,
            affinity_enabled: self.affinity_enabled,
            affinity_ttl_seconds: self.affinity_ttl_seconds,
            max_rate_multiplier: self.max_rate_multiplier,
            routing_group_filter: self.routing_group_filter.clone(),
            outbound_proxy_mode: self.outbound_proxy_mode.clone(),
            outbound_proxy_url: self.outbound_proxy_url.clone(),
        };
        base.validate().map_err(|_| {
            RoutingPolicyFieldValidationError::new(
                "policy",
                "invalid_base_policy",
                "routing.policy.invalid",
            )
        })?;
        self.reliability_source_weights.validate()?;
        self.reliability_sampling.validate()?;
        self.retry.validate()?;
        self.circuit_breaker.validate()?;
        self.timeout_policy.validate()
    }

    /// Upgrade a validated V2 policy.  This function is deliberately explicit
    /// so runtime reads cannot silently re-enable removed V2 settings.
    pub(crate) fn from_v2(
        value: &RoutingPolicyConfigV2,
    ) -> Result<RoutingPolicyV3Upgrade, RoutingPolicyFieldValidationError> {
        value.validate()?;
        let max_retry_count = value
            .retry_failover
            .max_total_attempts
            .checked_sub(1)
            .ok_or_else(|| {
                RoutingPolicyFieldValidationError::new(
                    "retry.maxRetryCount",
                    "invalid_legacy_value",
                    "routing.retry.maxRetryCount.invalidLegacy",
                )
            })?;
        let policy = Self {
            version: ROUTING_POLICY_CONFIG_VERSION_V3,
            reliability_weight: value.reliability_weight,
            responsiveness_weight: value.responsiveness_weight,
            cost_weight: value.cost_weight,
            preference_weight: value.preference_weight,
            allow_depleted_fallback: value.allow_depleted_fallback,
            affinity_enabled: value.affinity_enabled,
            affinity_ttl_seconds: value.affinity_ttl_seconds,
            max_rate_multiplier: value.max_rate_multiplier,
            routing_group_filter: value.routing_group_filter.clone(),
            outbound_proxy_mode: value.outbound_proxy_mode.clone(),
            outbound_proxy_url: value.outbound_proxy_url.clone(),
            reliability_source_weights: ReliabilitySourceWeightsV3::default(),
            reliability_sampling: ReliabilitySamplingPolicyV3::default(),
            retry: RetryPolicyV3 {
                version: ROUTING_POLICY_RETRY_VERSION_V3,
                max_retry_count,
                consecutive_failure_threshold: DEFAULT_CONSECUTIVE_FAILURE_THRESHOLD,
            },
            circuit_breaker: CircuitBreakerPolicyV3 {
                version: ROUTING_POLICY_CIRCUIT_BREAKER_VERSION_V1,
                recovery_success_threshold: value.protection_profile.half_open_successes_to_close,
                recovery_wait_seconds: DEFAULT_RECOVERY_WAIT_SECONDS,
            },
            timeout_policy: value.timeout_policy.clone(),
        };
        policy.validate()?;
        Ok(RoutingPolicyV3Upgrade {
            policy,
            audit: RoutingPolicyV3UpgradeAudit {
                from_version: ROUTING_POLICY_CONFIG_VERSION_V2,
                to_version: ROUTING_POLICY_CONFIG_VERSION_V3,
                discarded_fields: vec![
                    "maxCandidates",
                    "explorationShareBasisPoints",
                    "retryFailover.maxSameTargetCapacityRetries",
                    "retryFailover.capacityRetryWaitBudgetSeconds",
                    "retryFailover.allowCrossCapacityDomainFallback",
                    "protectionProfile.enabled",
                    "protectionProfile.windowMaxSamples",
                    "protectionProfile.windowSeconds",
                    "protectionProfile.minSamples",
                    "protectionProfile.failureThresholdPercent",
                ],
                defaulted_fields: vec![
                    "reliabilitySourceWeights",
                    "reliabilitySampling",
                    "retry.consecutiveFailureThreshold",
                    "circuitBreaker.recoveryWaitSeconds",
                ],
                semantic_changes: vec![
                    "circuit_breaker_always_enabled",
                    "error_rate_protection_replaced_by_consecutive_failures",
                    "capacity_domain_controls_removed_from_routing",
                ],
                quality_rebuild_required: true,
            },
        })
    }

    /// Decode only canonical V3 storage. V1/V2 values must be upgraded by the
    /// migration boundary before reaching this method.
    pub(crate) fn from_stored_value(
        value: &serde_json::Value,
    ) -> Result<Self, RoutingPolicyFieldValidationError> {
        if value.get("version").and_then(serde_json::Value::as_u64)
            != Some(u64::from(ROUTING_POLICY_CONFIG_VERSION_V3))
        {
            return Err(RoutingPolicyFieldValidationError::new(
                "version",
                "unsupported_version",
                "routing.policy.version.unsupported",
            ));
        }
        let policy = serde_json::from_value::<Self>(value.clone()).map_err(|_| {
            RoutingPolicyFieldValidationError::new(
                "policy",
                "invalid_v3_policy",
                "routing.policy.invalid",
            )
        })?;
        policy.validate()?;
        Ok(policy)
    }
}

/// V3 managed-document envelope. The outer format remains version 1; the
/// nested policy version is the domain contract version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RoutingPolicyDocumentV3 {
    pub(crate) format_version: u16,
    pub(crate) base_revision: u64,
    pub(crate) policy: RoutingPolicyConfigV3,
}

impl Default for RoutingPolicyDocumentV3 {
    fn default() -> Self {
        Self {
            format_version: ROUTING_POLICY_DOCUMENT_FORMAT_VERSION,
            base_revision: 0,
            policy: RoutingPolicyConfigV3::default(),
        }
    }
}

impl RoutingPolicyDocumentV3 {
    pub(crate) fn validate(&self) -> Result<(), RoutingPolicyFieldValidationError> {
        if self.format_version != ROUTING_POLICY_DOCUMENT_FORMAT_VERSION {
            return Err(RoutingPolicyFieldValidationError::new(
                "formatVersion",
                "unsupported_version",
                "routing.document.formatVersion.unsupported",
            ));
        }
        if self.base_revision == 0 {
            return Err(RoutingPolicyFieldValidationError::new(
                "baseRevision",
                "must_be_positive",
                "routing.document.baseRevision.required",
            ));
        }
        self.policy.validate()
    }

    pub(crate) fn from_v2(
        value: &RoutingPolicyDocumentV2,
    ) -> Result<(Self, RoutingPolicyV3UpgradeAudit), RoutingPolicyFieldValidationError> {
        if value.format_version != ROUTING_POLICY_DOCUMENT_FORMAT_VERSION
            || value.base_revision == 0
        {
            return Err(RoutingPolicyFieldValidationError::new(
                "document",
                "invalid_envelope",
                "routing.document.invalid",
            ));
        }
        let upgraded = RoutingPolicyConfigV3::from_v2(&value.policy)?;
        Ok((
            Self {
                format_version: value.format_version,
                base_revision: value.base_revision,
                policy: upgraded.policy,
            },
            upgraded.audit,
        ))
    }
}

impl From<RoutingPolicyConfigV1> for RoutingPolicyConfigV2 {
    fn from(value: RoutingPolicyConfigV1) -> Self {
        // This conversion is intentionally infallible for callers that have
        // already validated storage.  Public/document paths should use
        // `from_v1` so malformed input returns a field-level error.
        Self {
            version: ROUTING_POLICY_CONFIG_VERSION_V2,
            reliability_weight: value.reliability_weight,
            responsiveness_weight: value.responsiveness_weight,
            cost_weight: value.cost_weight,
            preference_weight: value.preference_weight,
            max_candidates: value.max_candidates,
            exploration_share_basis_points: value.exploration_share_basis_points,
            allow_depleted_fallback: value.allow_depleted_fallback,
            affinity_enabled: value.affinity_enabled,
            affinity_ttl_seconds: value.affinity_ttl_seconds,
            max_rate_multiplier: value.max_rate_multiplier,
            routing_group_filter: value.routing_group_filter,
            outbound_proxy_mode: value.outbound_proxy_mode,
            outbound_proxy_url: value.outbound_proxy_url,
            retry_failover: RetryFailoverPolicyV2::default(),
            protection_profile: ProtectionProfileConfigV2::default(),
            timeout_policy: TimeoutPolicyV2::default(),
        }
    }
}

/// Convert the previous V2 nested duration contracts at the storage boundary.
///
/// Schema 54 materializes this representation in SQLite. Keeping this narrow
/// decoder upgrade protects existing local data if a prior upgrade was
/// interrupted, while remaining fail-closed for partial or malformed objects.
fn upgrade_v2_duration_units(value: &serde_json::Value) -> serde_json::Value {
    let mut upgraded = value.clone();
    let Some(policy) = upgraded.as_object_mut() else {
        return upgraded;
    };

    upgrade_nested_milliseconds_to_seconds(
        policy,
        "retryFailover",
        &[(
            "capacityRetryWaitBudgetMs",
            "capacityRetryWaitBudgetSeconds",
        )],
    );
    upgrade_nested_milliseconds_to_seconds(
        policy,
        "protectionProfile",
        &[("windowMs", "windowSeconds")],
    );
    upgrade_nested_milliseconds_to_seconds(
        policy,
        "timeoutPolicy",
        &[
            ("connectMs", "connectSeconds"),
            ("firstByteMs", "firstByteSeconds"),
            ("precommitMs", "precommitSeconds"),
            ("bufferedExecutionMs", "bufferedExecutionSeconds"),
            ("streamIdleMs", "streamIdleSeconds"),
        ],
    );

    upgraded
}

fn upgrade_nested_milliseconds_to_seconds(
    policy: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    fields: &[(&str, &str)],
) {
    let Some(serde_json::Value::Object(nested)) = policy.get_mut(key) else {
        return;
    };
    if nested.get("version").and_then(serde_json::Value::as_u64) != Some(1)
        || fields
            .iter()
            .any(|(legacy, current)| !nested.contains_key(*legacy) || nested.contains_key(*current))
    {
        return;
    }

    let Some(seconds) = fields
        .iter()
        .map(|(legacy, current)| {
            nested
                .get(*legacy)
                .and_then(serde_json::Value::as_f64)
                .map(|milliseconds| (*legacy, *current, milliseconds / 1_000.0))
        })
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };

    for (legacy, current, value) in seconds {
        nested.remove(legacy);
        nested.insert(current.to_string(), json!(value));
    }
    nested.insert("version".to_string(), json!(2));
}

/// Versioned managed-file envelope for a V2 policy.  The envelope format is
/// unchanged; only the nested policy version advances.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RoutingPolicyDocumentV2 {
    pub(crate) format_version: u16,
    pub(crate) base_revision: u64,
    pub(crate) policy: RoutingPolicyConfigV2,
}

impl Default for RoutingPolicyDocumentV2 {
    fn default() -> Self {
        Self {
            format_version: ROUTING_POLICY_DOCUMENT_FORMAT_VERSION,
            base_revision: 0,
            policy: RoutingPolicyConfigV2::default(),
        }
    }
}

impl RoutingPolicyDocumentV2 {
    pub(crate) fn from_v1(
        value: &RoutingPolicyDocumentV1,
    ) -> Result<Self, RoutingPolicyFieldValidationError> {
        if value.format_version != ROUTING_POLICY_DOCUMENT_FORMAT_VERSION {
            return Err(RoutingPolicyFieldValidationError::new(
                "formatVersion",
                "unsupported_version",
                "routing.document.formatVersion.unsupported",
            ));
        }
        if value.base_revision == 0 {
            return Err(RoutingPolicyFieldValidationError::new(
                "baseRevision",
                "must_be_positive",
                "routing.document.baseRevision.required",
            ));
        }
        Ok(Self {
            format_version: value.format_version,
            base_revision: value.base_revision,
            policy: RoutingPolicyConfigV2::from_v1(&value.policy.clone().into_storage())?,
        })
    }

    pub(crate) fn validate(&self) -> Result<(), RoutingPolicyFieldValidationError> {
        if self.format_version != ROUTING_POLICY_DOCUMENT_FORMAT_VERSION {
            return Err(RoutingPolicyFieldValidationError::new(
                "formatVersion",
                "unsupported_version",
                "routing.document.formatVersion.unsupported",
            ));
        }
        if self.base_revision == 0 {
            return Err(RoutingPolicyFieldValidationError::new(
                "baseRevision",
                "must_be_positive",
                "routing.document.baseRevision.required",
            ));
        }
        self.policy.validate()
    }
}

impl Default for RoutingPolicyConfigV1 {
    fn default() -> Self {
        Self {
            version: 1,
            reliability_weight: 4_000,
            responsiveness_weight: 2_500,
            cost_weight: 2_000,
            preference_weight: 1_500,
            max_candidates: 64,
            exploration_share_basis_points: 500,
            allow_depleted_fallback: false,
            affinity_enabled: false,
            affinity_ttl_seconds: 300,
            max_rate_multiplier: None,
            routing_group_filter: RoutingGroupFilter::AllGroups,
            outbound_proxy_mode: DEFAULT_OUTBOUND_PROXY_MODE.to_string(),
            outbound_proxy_url: None,
        }
    }
}

impl RoutingPolicyConfigV1 {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let total = u32::from(self.reliability_weight)
            + u32::from(self.responsiveness_weight)
            + u32::from(self.cost_weight)
            + u32::from(self.preference_weight);
        if self.version != 1
            || self.max_candidates == 0
            || self.max_candidates > 1_024
            || total != 10_000
            || self.exploration_share_basis_points > 2_000
            || self
                .max_rate_multiplier
                .is_some_and(|value| !value.is_finite() || value < 0.0)
            || !matches!(
                self.outbound_proxy_mode
                    .trim()
                    .to_ascii_lowercase()
                    .as_str(),
                "inherit" | "direct" | "system" | "manual"
            )
            || (self.outbound_proxy_mode.eq_ignore_ascii_case("manual")
                && self
                    .outbound_proxy_url
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty()))
            || self.outbound_proxy_url.as_deref().is_some_and(|value| {
                let value = value.trim();
                !(value.starts_with("http://")
                    || value.starts_with("https://")
                    || value.starts_with("socks5://")
                    || value.starts_with("socks5h://"))
            })
            || (self.affinity_enabled && !(1..=86_400).contains(&self.affinity_ttl_seconds))
        {
            return Err("invalid routing policy");
        }
        Ok(())
    }
}

impl Default for RoutingPolicyDocumentV1 {
    fn default() -> Self {
        Self {
            format_version: ROUTING_POLICY_DOCUMENT_FORMAT_VERSION,
            base_revision: 0,
            policy: RoutingPolicyDocumentPolicyV1::default(),
        }
    }
}

#[cfg(test)]
impl RoutingPolicyDocumentV1 {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.format_version != ROUTING_POLICY_DOCUMENT_FORMAT_VERSION || self.base_revision == 0
        {
            return Err("invalid routing policy document envelope");
        }
        self.policy.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_failover_defaults_are_baseline_equivalent_and_camel_case() {
        let retry = RetryFailoverPolicyV2::default();
        assert_eq!(retry.max_total_attempts, 4);
        assert_eq!(retry.max_same_target_capacity_retries, 2);
        assert_eq!(retry.capacity_retry_wait_budget_seconds, 2.0);
        assert!(retry.allow_cross_capacity_domain_fallback);
        assert!(retry.validate().is_ok());

        let value = serde_json::to_value(retry).expect("serialize retry policy");
        assert!(value.get("maxTotalAttempts").is_some());
        assert!(value.get("maxSameTargetCapacityRetries").is_some());
        assert!(value.get("capacityRetryWaitBudgetSeconds").is_some());
        assert!(value.get("allowCrossCapacityDomainFallback").is_some());
        assert!(value.get("max_total_attempts").is_none());
    }

    #[test]
    fn retry_failover_rejects_invalid_combinations_with_stable_field_errors() {
        let mut retry = RetryFailoverPolicyV2::default();
        retry.max_total_attempts = 0;
        let error = retry.validate().expect_err("zero attempts must fail");
        assert_eq!(error.field, "retryFailover.maxTotalAttempts");
        assert_eq!(error.code, "out_of_range");

        retry = RetryFailoverPolicyV2::default();
        retry.max_total_attempts = 2;
        retry.max_same_target_capacity_retries = 2;
        let error = retry
            .validate()
            .expect_err("same-target retries must leave an attempt");
        assert_eq!(error.field, "retryFailover.maxSameTargetCapacityRetries");
        assert_eq!(error.code, "must_be_less_than_max_total_attempts");

        retry = RetryFailoverPolicyV2::default();
        retry.capacity_retry_wait_budget_seconds =
            MAX_CAPACITY_RETRY_WAIT_BUDGET_SECONDS_HARD_CAP + 0.1;
        let error = retry.validate().expect_err("wait budget must be bounded");
        assert_eq!(error.field, "retryFailover.capacityRetryWaitBudgetSeconds");
    }

    #[test]
    fn protection_profile_is_disabled_by_default_and_has_bounded_fields() {
        let profile = ProtectionProfileConfigV2::default();
        assert!(!profile.enabled);
        assert!(profile.validate().is_ok());
        let mut invalid = profile.clone();
        invalid.min_samples = invalid.window_max_samples + 1;
        let error = invalid
            .validate()
            .expect_err("minimum samples must fit window");
        assert_eq!(error.field, "protectionProfile.minSamples");
        invalid = profile.clone();
        invalid.failure_threshold_percent = 0;
        let error = invalid.validate().expect_err("threshold must be positive");
        assert_eq!(error.field, "protectionProfile.failureThresholdPercent");
    }

    #[test]
    fn timeout_policy_matches_proxy_defaults_and_rejects_incoherent_budgets() {
        let policy = TimeoutPolicyV2::default();
        assert_eq!(policy.connect_seconds, 10.0);
        assert_eq!(policy.first_byte_seconds, 30.0);
        assert_eq!(policy.precommit_seconds, 60.0);
        assert_eq!(policy.buffered_execution_seconds, 300.0);
        assert_eq!(policy.stream_idle_seconds, 90.0);
        assert!(policy.validate().is_ok());

        let mut invalid = policy.clone();
        invalid.connect_seconds = MIN_ROUTING_TIMEOUT_SECONDS - 0.1;
        let error = invalid.validate().expect_err("zero-ish timeout must fail");
        assert_eq!(error.field, "timeoutPolicy.connectSeconds");

        invalid = policy;
        invalid.precommit_seconds = invalid.buffered_execution_seconds + 0.1;
        let error = invalid
            .validate()
            .expect_err("precommit must fit inside buffered execution");
        assert_eq!(error.field, "timeoutPolicy.precommitSeconds");
        assert_eq!(error.code, "must_not_exceed_buffered_execution");
    }

    #[test]
    fn v1_upgrade_preserves_routing_factors_and_adds_baseline_retry_policy() {
        let mut v1 = RoutingPolicyConfigV1::default();
        v1.reliability_weight = 5_000;
        v1.preference_weight = 500;
        v1.affinity_enabled = true;
        let v2 = RoutingPolicyConfigV2::from_v1(&v1).expect("upgrade valid v1 policy");
        assert_eq!(v2.version, ROUTING_POLICY_CONFIG_VERSION_V2);
        assert_eq!(v2.reliability_weight, v1.reliability_weight);
        assert_eq!(v2.affinity_enabled, v1.affinity_enabled);
        assert_eq!(v2.retry_failover, RetryFailoverPolicyV2::default());
        assert!(v2.validate().is_ok());
    }

    #[test]
    fn stored_decoder_normalizes_v1_and_validates_v2_without_unknown_fields() {
        let v1 = serde_json::to_value(RoutingPolicyConfigV1::default()).expect("v1 value");
        let upgraded = RoutingPolicyConfigV2::from_stored_value(&v1).expect("v1 upgrade");
        assert_eq!(upgraded, RoutingPolicyConfigV2::default());

        let v2 = serde_json::to_value(RoutingPolicyConfigV2::default()).expect("v2 value");
        assert_eq!(
            RoutingPolicyConfigV2::from_stored_value(&v2),
            Ok(RoutingPolicyConfigV2::default())
        );

        let mut unknown = v2.clone();
        unknown["unexpected"] = serde_json::json!(true);
        assert!(RoutingPolicyConfigV2::from_stored_value(&unknown).is_err());

        let mut invalid = v2;
        invalid["retryFailover"]["maxTotalAttempts"] = serde_json::json!(0);
        assert!(RoutingPolicyConfigV2::from_stored_value(&invalid).is_err());
    }

    #[test]
    fn stored_decoder_upgrades_complete_v2_millisecond_duration_profiles() {
        let mut legacy =
            serde_json::to_value(RoutingPolicyConfigV2::default()).expect("serialize v2 policy");
        legacy["retryFailover"]["version"] = json!(1);
        legacy["retryFailover"]["capacityRetryWaitBudgetMs"] = json!(750);
        legacy["retryFailover"]
            .as_object_mut()
            .expect("retry object")
            .remove("capacityRetryWaitBudgetSeconds");
        legacy["protectionProfile"]["version"] = json!(1);
        legacy["protectionProfile"]["windowMs"] = json!(300_500);
        legacy["protectionProfile"]
            .as_object_mut()
            .expect("protection object")
            .remove("windowSeconds");
        legacy["timeoutPolicy"]["version"] = json!(1);
        for (milliseconds, legacy_name, current_name) in [
            (1_250, "connectMs", "connectSeconds"),
            (30_500, "firstByteMs", "firstByteSeconds"),
            (60_250, "precommitMs", "precommitSeconds"),
            (300_750, "bufferedExecutionMs", "bufferedExecutionSeconds"),
            (90_125, "streamIdleMs", "streamIdleSeconds"),
        ] {
            legacy["timeoutPolicy"][legacy_name] = json!(milliseconds);
            legacy["timeoutPolicy"]
                .as_object_mut()
                .expect("timeout object")
                .remove(current_name);
        }

        let upgraded =
            RoutingPolicyConfigV2::from_stored_value(&legacy).expect("legacy milliseconds upgrade");
        assert_eq!(upgraded.retry_failover.version, 2);
        assert_eq!(
            upgraded.retry_failover.capacity_retry_wait_budget_seconds,
            0.75
        );
        assert_eq!(upgraded.protection_profile.window_seconds, 300.5);
        assert_eq!(upgraded.timeout_policy.connect_seconds, 1.25);
        assert_eq!(upgraded.timeout_policy.stream_idle_seconds, 90.125);
    }

    #[test]
    fn v2_document_requires_retry_failover_and_protection_profile() {
        let document = RoutingPolicyDocumentV2 {
            base_revision: 7,
            ..RoutingPolicyDocumentV2::default()
        };
        assert!(document.validate().is_ok());
        let value = serde_json::to_value(document).expect("serialize v2 document");
        assert_eq!(value["policy"]["version"], 2);
        assert!(value["policy"]["retryFailover"].is_object());
        assert!(value["policy"]["protectionProfile"].is_object());

        let missing_retry = serde_json::json!({
            "formatVersion": 1,
            "baseRevision": 7,
            "policy": {
                "version": 2,
                "reliabilityWeight": 4000,
                "responsivenessWeight": 2500,
                "costWeight": 2000,
                "preferenceWeight": 1500,
                "maxCandidates": 64,
                "explorationShareBasisPoints": 500,
                "allowDepletedFallback": false,
                "affinityEnabled": false,
                "affinityTtlSeconds": 300,
                "maxRateMultiplier": null,
                "routingGroupFilter": "all_groups",
                "outboundProxyMode": "inherit",
                "outboundProxyUrl": null
            }
        });
        assert!(serde_json::from_value::<RoutingPolicyDocumentV2>(missing_retry).is_err());

        let mut with_unknown = value;
        with_unknown["policy"]["futureField"] = serde_json::json!(true);
        assert!(serde_json::from_value::<RoutingPolicyDocumentV2>(with_unknown).is_err());
    }

    #[test]
    fn v2_storage_decoder_rejects_missing_base_fields_instead_of_using_compatibility_defaults() {
        let value = serde_json::to_value(RoutingPolicyConfigV2::default())
            .expect("serialize complete v2 policy");
        for field in [
            "reliabilityWeight",
            "responsivenessWeight",
            "costWeight",
            "preferenceWeight",
            "maxCandidates",
            "explorationShareBasisPoints",
            "allowDepletedFallback",
            "affinityEnabled",
            "affinityTtlSeconds",
            "maxRateMultiplier",
            "routingGroupFilter",
            "outboundProxyMode",
            "outboundProxyUrl",
            "retryFailover",
            "timeoutPolicy",
        ] {
            let mut missing = value.clone();
            missing
                .as_object_mut()
                .expect("policy object")
                .remove(field);
            assert!(
                serde_json::from_value::<RoutingPolicyConfigV2>(missing.clone()).is_err(),
                "missing V2 field {field} must fail closed"
            );
            assert!(
                RoutingPolicyConfigV2::from_stored_value(&missing).is_err(),
                "storage decoder must not default missing V2 field {field}"
            );
        }
        let mut missing_profile = value;
        missing_profile
            .as_object_mut()
            .expect("policy object")
            .remove("protectionProfile");
        assert!(
            serde_json::from_value::<RoutingPolicyConfigV2>(missing_profile.clone()).is_err(),
            "missing protection profile must fail closed"
        );
        assert!(
            RoutingPolicyConfigV2::from_stored_value(&missing_profile).is_err(),
            "stored V2 missing protection profile must fail closed"
        );
    }

    #[test]
    fn v2_retry_failover_decoder_rejects_missing_nested_fields() {
        let value = serde_json::to_value(RoutingPolicyConfigV2::default())
            .expect("serialize complete v2 policy");
        for field in [
            "version",
            "maxTotalAttempts",
            "maxSameTargetCapacityRetries",
            "capacityRetryWaitBudgetSeconds",
            "allowCrossCapacityDomainFallback",
        ] {
            let mut missing = value.clone();
            missing["retryFailover"]
                .as_object_mut()
                .expect("retry policy object")
                .remove(field);
            assert!(
                serde_json::from_value::<RoutingPolicyConfigV2>(missing).is_err(),
                "missing retryFailover field {field} must fail closed"
            );
        }
    }

    #[test]
    fn v2_timeout_policy_decoder_rejects_missing_nested_fields() {
        let value = serde_json::to_value(RoutingPolicyConfigV2::default())
            .expect("serialize complete v2 policy");
        for field in [
            "version",
            "connectSeconds",
            "firstByteSeconds",
            "precommitSeconds",
            "bufferedExecutionSeconds",
            "streamIdleSeconds",
        ] {
            let mut missing = value.clone();
            missing["timeoutPolicy"]
                .as_object_mut()
                .expect("timeout policy object")
                .remove(field);
            assert!(
                serde_json::from_value::<RoutingPolicyConfigV2>(missing).is_err(),
                "missing timeoutPolicy field {field} must fail closed"
            );
        }
    }

    #[test]
    fn v3_defaults_have_strict_shape_and_expected_retry_semantics() {
        let policy = RoutingPolicyConfigV3::default();
        assert!(policy.validate().is_ok());
        let value = serde_json::to_value(&policy).expect("serialize V3 policy");
        assert_eq!(value["version"], 3);
        assert_eq!(value["reliabilitySourceWeights"]["realTrafficPercent"], 70);
        assert_eq!(value["reliabilitySourceWeights"]["monitoringPercent"], 30);
        assert_eq!(value["retry"]["maxRetryCount"], 3);
        assert_eq!(value["retry"]["consecutiveFailureThreshold"], 3);
        assert!(value.get("maxCandidates").is_none());
        assert!(value.get("explorationShareBasisPoints").is_none());
        assert!(value.get("protectionProfile").is_none());
    }

    #[test]
    fn v3_rejects_invalid_source_weights_sampling_and_breaker_fields() {
        let mut policy = RoutingPolicyConfigV3::default();
        policy.reliability_source_weights.monitoring_percent = 31;
        let error = policy
            .validate()
            .expect_err("source weights must sum to 100");
        assert_eq!(error.field, "reliabilitySourceWeights.monitoringPercent");

        policy = RoutingPolicyConfigV3::default();
        policy.reliability_sampling.optimistic_latency_ms = 99;
        let error = policy.validate().expect_err("latency lower bound");
        assert_eq!(error.field, "reliabilitySampling.optimisticLatencyMs");

        policy = RoutingPolicyConfigV3::default();
        policy.retry.max_retry_count = MAX_RETRY_COUNT_HARD_CAP + 1;
        let error = policy.validate().expect_err("retry hard cap");
        assert_eq!(error.field, "retry.maxRetryCount");

        policy = RoutingPolicyConfigV3::default();
        policy.circuit_breaker.recovery_wait_seconds = 4;
        let error = policy.validate().expect_err("recovery wait lower bound");
        assert_eq!(error.field, "circuitBreaker.recoveryWaitSeconds");
    }

    #[test]
    fn v2_to_v3_upgrade_maps_attempts_and_records_removed_semantics() {
        let mut v2 = RoutingPolicyConfigV2::default();
        v2.retry_failover.max_total_attempts = 2;
        v2.retry_failover.max_same_target_capacity_retries = 0;
        v2.protection_profile.half_open_successes_to_close = 4;
        v2.protection_profile.enabled = false;
        let result = RoutingPolicyConfigV3::from_v2(&v2).expect("upgrade V2 policy");
        assert_eq!(result.policy.retry.max_retry_count, 1);
        assert_eq!(result.policy.retry.max_total_attempts(), 2);
        assert_eq!(result.policy.circuit_breaker.recovery_success_threshold, 4);
        assert_eq!(result.policy.circuit_breaker.recovery_wait_seconds, 30);
        assert!(result.audit.quality_rebuild_required);
        assert!(result.audit.discarded_fields.contains(&"maxCandidates"));
        assert!(result
            .audit
            .semantic_changes
            .contains(&"circuit_breaker_always_enabled"));
    }

    #[test]
    fn v3_storage_decoder_is_fail_closed_and_never_accepts_v2_fields() {
        let value = serde_json::to_value(RoutingPolicyConfigV3::default()).expect("V3 value");
        assert_eq!(
            RoutingPolicyConfigV3::from_stored_value(&value),
            Ok(RoutingPolicyConfigV3::default())
        );
        let mut unknown = value.clone();
        unknown["maxCandidates"] = serde_json::json!(64);
        assert!(RoutingPolicyConfigV3::from_stored_value(&unknown).is_err());
        let v2 = serde_json::to_value(RoutingPolicyConfigV2::default()).expect("V2 value");
        assert!(RoutingPolicyConfigV3::from_stored_value(&v2).is_err());
    }

    #[test]
    fn managed_document_requires_positive_revision_and_current_format() {
        let mut document = RoutingPolicyDocumentV1::default();
        assert!(document.validate().is_err());
        document.base_revision = 1;
        assert!(document.validate().is_ok());
        document.format_version = ROUTING_POLICY_DOCUMENT_FORMAT_VERSION + 1;
        assert!(document.validate().is_err());
    }

    #[test]
    fn managed_document_uses_camel_case_envelope() {
        let value = serde_json::to_value(RoutingPolicyDocumentV1 {
            base_revision: 4,
            ..RoutingPolicyDocumentV1::default()
        })
        .expect("serialize document");
        assert!(value.get("formatVersion").is_some());
        assert!(value.get("baseRevision").is_some());
        assert!(value.get("format_version").is_none());
    }
}
