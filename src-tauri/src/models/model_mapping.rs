//! Data-neutral model mapping document types.
//!
//! This module deliberately contains no persistence, routing or IPC concerns.  The
//! Phase 1 wire types are a small, closed tagged union; adding a future matcher or
//! action requires an explicit schema/compiler change instead of silently accepting
//! a document for which there is no runtime consumer.

use serde::{Deserialize, Serialize};

pub const MODEL_MAPPING_FORMAT_VERSION: u16 = 1;
pub const MAX_MODEL_NAME_BYTES: usize = 256;
pub const MAX_RULES: usize = 256;
pub const MAX_PROFILES: usize = 256;
pub const MAX_BINDINGS: usize = 512;
pub const MAX_TARGETS_PER_RULE: usize = 3;
pub const MAX_RULE_ID_BYTES: usize = 128;
pub const MAX_PRIORITY: u32 = 1_000_000;
pub const MAX_NOTE_BYTES: usize = 1_024;
pub const MAX_REJECTION_MESSAGE_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnmatchedModelBehavior {
    Preserve,
    Reject,
}
impl Default for UnmatchedModelBehavior {
    fn default() -> Self {
        Self::Preserve
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingPolicy {
    pub unmatched_model_behavior: UnmatchedModelBehavior,
}

impl Default for ModelMappingPolicy {
    fn default() -> Self {
        Self {
            unmatched_model_behavior: UnmatchedModelBehavior::Preserve,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointKind {
    ChatCompletions,
    Responses,
    Embeddings,
    Models,
    Usage,
}

impl EndpointKind {
    pub const fn is_mapping_bypass(self) -> bool {
        matches!(self, Self::Models | Self::Usage)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionRequirement {
    Any,
    Required,
    Forbidden,
}

impl Default for ConditionRequirement {
    fn default() -> Self {
        Self::Any
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleConditions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint_kinds: Option<Vec<EndpointKind>>,
    #[serde(default)]
    pub stream: ConditionRequirement,
    #[serde(default)]
    pub tools: ConditionRequirement,
    #[serde(default)]
    pub vision: ConditionRequirement,
    #[serde(default)]
    pub reasoning: ConditionRequirement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Matcher {
    Exact { model: String },
    Glob { pattern: String },
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TargetRef {
    Literal { upstream_model: String },
    ModelProfile { model_profile_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackTrigger {
    NoEligibleTarget,
    RetryExhaustedBeforeOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProfileStatus {
    Active,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelBindingSource {
    Manual,
    Discovered,
    Migrated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelProfile {
    pub id: String,
    pub canonical_model: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_upstream_model: Option<String>,
    pub status: ModelProfileStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelOfferingBinding {
    pub id: String,
    pub model_profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub station_key_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub station_id: Option<String>,
    pub upstream_model: String,
    pub source: ModelBindingSource,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectionKind {
    UnsupportedModel,
    PolicyDenied,
    ClientNotAllowed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    MapFixed {
        target: TargetRef,
    },
    MapFallbackChain {
        targets: Vec<TargetRef>,
        fallback_trigger: FallbackTrigger,
    },
    Preserve,
    Reject {
        rejection_kind: RejectionKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingRule {
    pub id: String,
    pub priority: u32,
    pub enabled: bool,
    pub matcher: Matcher,
    #[serde(default)]
    pub conditions: RuleConditions,
    pub action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default)]
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingDocumentV1 {
    pub format_version: u16,
    pub base_revision: u64,
    pub policy: ModelMappingPolicy,
    pub rules: Vec<ModelMappingRule>,
    #[serde(default)]
    pub profiles: Vec<ModelProfile>,
    #[serde(default)]
    pub bindings: Vec<ModelOfferingBinding>,
}

impl Default for ModelMappingDocumentV1 {
    fn default() -> Self {
        Self {
            format_version: MODEL_MAPPING_FORMAT_VERSION,
            base_revision: 0,
            policy: ModelMappingPolicy::default(),
            rules: Vec::new(),
            profiles: Vec::new(),
            bindings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequestFacts {
    pub requested_model: Option<String>,
    pub endpoint: EndpointKind,
    pub stream: bool,
    pub tools: bool,
    pub vision: bool,
    pub reasoning: bool,
}

impl ModelRequestFacts {
    pub fn inference(
        requested_model: impl Into<String>,
        endpoint: EndpointKind,
        stream: bool,
        tools: bool,
        vision: bool,
        reasoning: bool,
    ) -> Self {
        Self {
            requested_model: Some(requested_model.into()),
            endpoint,
            stream,
            tools,
            vision,
            reasoning,
        }
    }

    #[cfg(test)]
    pub const fn bypass(endpoint: EndpointKind) -> Self {
        Self {
            requested_model: None,
            endpoint,
            stream: false,
            tools: false,
            vision: false,
            reasoning: false,
        }
    }
}

/// Unicode-whitespace normalization used by every mapping identity comparison.
/// It deliberately does not case-fold, normalize Unicode, or alter provider prefixes.
pub fn normalize_model_name(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_MODEL_NAME_BYTES
        || value
            .chars()
            .any(|character| character == '\0' || character.is_control())
    {
        return None;
    }
    Some(value.to_owned())
}
