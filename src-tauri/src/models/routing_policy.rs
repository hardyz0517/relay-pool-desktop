use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Versioned managed-file envelope for the active routing policy aggregate.
/// The envelope keeps the CAS revision next to the user-editable policy so a
/// file import cannot accidentally overwrite a newer SQLite revision.
pub(crate) const ROUTING_POLICY_DOCUMENT_FORMAT_VERSION: u16 = 1;
const DEFAULT_OUTBOUND_PROXY_MODE: &str = "inherit";

fn default_outbound_proxy_mode() -> String {
    DEFAULT_OUTBOUND_PROXY_MODE.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RoutingPolicyDocumentV1 {
    pub(crate) format_version: u16,
    pub(crate) base_revision: u64,
    pub(crate) policy: RoutingPolicyConfigV1,
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
            policy: RoutingPolicyConfigV1::default(),
        }
    }
}

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
