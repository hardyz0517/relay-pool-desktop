use serde::{Deserialize, Serialize};

use super::{
    policy::{HealthPolicy, RetryPolicy, RiskPolicy, SchedulePolicy},
    ProtocolKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DefinitionRevision(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetScopeKind {
    Station,
    StationKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetScope {
    Station {
        station_id: String,
    },
    StationKey {
        station_id: String,
        station_key_id: String,
    },
}

impl TargetScope {
    fn from_parts(
        target_scope: TargetScopeKind,
        station_id: Option<String>,
        station_key_id: Option<String>,
    ) -> Result<Self, String> {
        let station_id = non_empty(station_id, "station_id")?;
        match target_scope {
            TargetScopeKind::Station => {
                if station_key_id
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty())
                {
                    return Err("station scope must not include station_key_id".to_string());
                }
                Ok(Self::Station { station_id })
            }
            TargetScopeKind::StationKey => Ok(Self::StationKey {
                station_id,
                station_key_id: non_empty(station_key_id, "station_key_id")?,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientProfileId {
    StandardApi,
    CodexCliCompat,
    ClaudeCodeCompat,
    GeminiCliCompat,
    GrokCliCompat,
}

impl ClientProfileId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StandardApi => "standard_api",
            Self::CodexCliCompat => "codex_cli_compat",
            Self::ClaudeCodeCompat => "claude_code_compat",
            Self::GeminiCliCompat => "gemini_cli_compat",
            Self::GrokCliCompat => "grok_cli_compat",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "standard_api" => Some(Self::StandardApi),
            "codex_cli_compat" => Some(Self::CodexCliCompat),
            "claude_code_compat" => Some(Self::ClaudeCodeCompat),
            "gemini_cli_compat" => Some(Self::GeminiCliCompat),
            "grok_cli_compat" => Some(Self::GrokCliCompat),
            _ => None,
        }
    }

    pub fn supports_protocol(self, protocol: ProtocolKind) -> bool {
        match self {
            Self::StandardApi => true,
            Self::CodexCliCompat => matches!(
                protocol,
                ProtocolKind::OpenAiChat
                    | ProtocolKind::OpenAiResponses
                    | ProtocolKind::GenericOpenAi
            ),
            Self::ClaudeCodeCompat => matches!(protocol, ProtocolKind::AnthropicMessages),
            Self::GeminiCliCompat => matches!(protocol, ProtocolKind::GeminiNative),
            Self::GrokCliCompat => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientProfileRef {
    pub id: ClientProfileId,
    pub version: u32,
}

impl ClientProfileRef {
    pub fn new(id: ClientProfileId, version: u32) -> Result<Self, String> {
        if version == 0 {
            return Err("client_profile_version must be positive".to_string());
        }
        Ok(Self { id, version })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorDefinitionDraft {
    pub id: String,
    pub revision: DefinitionRevision,
    pub target_scope: TargetScopeKind,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub protocol_kind: ProtocolKind,
    pub client_profile: ClientProfileRef,
    pub primary_model: String,
    pub fallback_models: Vec<String>,
    pub schedule_policy: SchedulePolicy,
    pub retry_policy: RetryPolicy,
    pub risk_policy: RiskPolicy,
    pub health_policy: HealthPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorDefinition {
    pub id: String,
    pub revision: DefinitionRevision,
    pub target_scope: TargetScope,
    pub protocol_kind: ProtocolKind,
    pub client_profile: ClientProfileRef,
    pub primary_model: String,
    pub fallback_models: Vec<String>,
    pub schedule_policy: SchedulePolicy,
    pub retry_policy: RetryPolicy,
    pub risk_policy: RiskPolicy,
    pub health_policy: HealthPolicy,
}

impl MonitorDefinition {
    pub fn from_draft(draft: MonitorDefinitionDraft) -> Result<Self, String> {
        let id = non_empty(Some(draft.id), "id")?;
        let target_scope =
            TargetScope::from_parts(draft.target_scope, draft.station_id, draft.station_key_id)?;
        if !draft
            .client_profile
            .id
            .supports_protocol(draft.protocol_kind)
        {
            return Err("client profile does not support protocol".to_string());
        }
        let primary_model = non_empty(Some(draft.primary_model), "primary_model")?;
        let fallback_models = normalize_fallback_models(&primary_model, draft.fallback_models)?;
        Ok(Self {
            id,
            revision: draft.revision,
            target_scope,
            protocol_kind: draft.protocol_kind,
            client_profile: draft.client_profile,
            primary_model,
            fallback_models,
            schedule_policy: draft.schedule_policy,
            retry_policy: draft.retry_policy,
            risk_policy: draft.risk_policy,
            health_policy: draft.health_policy,
        })
    }

    pub fn theoretical_max_attempts(&self) -> u32 {
        let model_count = 1 + self.fallback_models.len() as u32;
        model_count * u32::from(self.retry_policy.max_attempts_per_model)
    }

    pub fn primary_attempt_fits_deadline(&self) -> bool {
        self.schedule_policy.attempt_timeout_ms < self.schedule_policy.execution_timeout_ms
    }
}

fn normalize_fallback_models(
    primary_model: &str,
    fallback_models: Vec<String>,
) -> Result<Vec<String>, String> {
    let mut normalized = Vec::<String>::new();
    for model in fallback_models {
        let model = model.trim().to_string();
        if model.is_empty()
            || model == primary_model
            || normalized.iter().any(|item| item == &model)
        {
            continue;
        }
        normalized.push(model);
    }
    if normalized.len() > 3 {
        return Err("fallback_models must contain at most 3 unique models".to_string());
    }
    Ok(normalized)
}

fn non_empty(value: Option<String>, field: &str) -> Result<String, String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{field} must be non-empty"))
}
