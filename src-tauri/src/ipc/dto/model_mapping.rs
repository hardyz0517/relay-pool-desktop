use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{invalid_input, TypeDescriptor};

const MAX_ID_BYTES: usize = 128;
const MAX_MODEL_BYTES: usize = 256;
const MAX_NOTE_BYTES: usize = 4_096;
const MAX_GLOB_BYTES: usize = 256;
const MAX_FALLBACK_TARGETS: usize = 3;
const MAX_RULES: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingDocumentDto {
    pub format_version: u16,
    pub base_revision: u64,
    pub policy: ModelMappingPolicyDto,
    pub rules: Vec<ModelMappingRuleDto>,
    pub profiles: Vec<ModelMappingProfileDto>,
    pub bindings: Vec<ModelMappingBindingDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingPolicyDto {
    pub unmatched_model_behavior: UnmatchedModelBehaviorDto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnmatchedModelBehaviorDto {
    Preserve,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingRuleDto {
    pub id: String,
    pub priority: u32,
    pub enabled: bool,
    pub matcher: ModelMappingMatcherDto,
    pub conditions: ModelMappingConditionsDto,
    pub action: ModelMappingActionDto,
    pub note: Option<String>,
    pub revision: u64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelMappingMatcherDto {
    Exact { model: String },
    Default,
    Glob { pattern: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingConditionsDto {
    pub endpoint_kinds: Vec<ModelMappingEndpointKindDto>,
    pub stream: ModelMappingConditionModeDto,
    pub tools: ModelMappingConditionModeDto,
    pub vision: ModelMappingConditionModeDto,
    pub reasoning: ModelMappingConditionModeDto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMappingConditionModeDto {
    Any,
    Required,
    Forbidden,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMappingEndpointKindDto {
    ChatCompletions,
    Responses,
    Embeddings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelMappingActionDto {
    MapFixed {
        target: ModelMappingTargetRefDto,
    },
    Preserve,
    Reject {
        #[serde(rename = "rejectionKind")]
        rejection_kind: ModelMappingRejectionKindDto,
        message: Option<String>,
    },
    MapFallbackChain {
        targets: Vec<ModelMappingTargetRefDto>,
        #[serde(rename = "fallbackTrigger")]
        fallback_trigger: ModelMappingFallbackTriggerDto,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelMappingTargetRefDto {
    Literal {
        #[serde(rename = "upstreamModel")]
        upstream_model: String,
    },
    ModelProfile {
        #[serde(rename = "modelProfileId")]
        model_profile_id: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMappingRejectionKindDto {
    UnsupportedModel,
    Policy,
    InvalidRequest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMappingFallbackTriggerDto {
    NoEligibleTarget,
    RetryExhaustedBeforeOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingProfileDto {
    pub id: String,
    pub canonical_model: String,
    pub display_name: Option<String>,
    pub default_upstream_model: Option<String>,
    pub status: ModelMappingProfileStatusDto,
    pub note: Option<String>,
    pub revision: u64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMappingProfileStatusDto {
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingBindingDto {
    pub id: String,
    pub model_profile_id: String,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub upstream_model: String,
    pub source: ModelMappingBindingSourceDto,
    pub enabled: bool,
    pub note: Option<String>,
    pub revision: u64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMappingBindingSourceDto {
    User,
    Discovered,
    Migration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingDocumentStatusDto {
    pub active_revision: u64,
    pub sync_state: ModelMappingSyncStateDto,
    pub source: ModelMappingDocumentSourceDto,
    pub file_present: bool,
    pub last_error_code: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMappingSyncStateDto {
    Synchronized,
    PendingMaterialization,
    ExternalChange,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMappingDocumentSourceDto {
    Ui,
    File,
    Migration,
    Restore,
    Import,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingDiagnosticDto {
    pub severity: ModelMappingDiagnosticSeverityDto,
    pub code: String,
    pub path: String,
    pub message: String,
    pub rule_id: Option<String>,
    pub target_index: Option<u16>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMappingDiagnosticSeverityDto {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingLegacyReviewDto {
    pub id: String,
    pub legacy_alias_id: String,
    pub requested_model: String,
    pub selected_target: String,
    pub discarded_target: Option<String>,
    pub status: ModelMappingLegacyReviewStatusDto,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMappingLegacyReviewStatusDto {
    Pending,
    Accepted,
    Discarded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingWorkspaceDto {
    pub document: ModelMappingDocumentDto,
    pub status: ModelMappingDocumentStatusDto,
    pub known_model_options: Vec<String>,
    pub legacy_reviews: Vec<ModelMappingLegacyReviewDto>,
    pub diagnostics: Vec<ModelMappingDiagnosticDto>,
    pub candidate_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingValidationResultDto {
    pub valid: bool,
    pub diagnostics: Vec<ModelMappingDiagnosticDto>,
    pub normalized_document: Option<ModelMappingDocumentDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidateModelMappingDocumentInputDto {
    pub document: ModelMappingDocumentDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyModelMappingDocumentInputDto {
    pub document: ModelMappingDocumentDto,
    pub source: ModelMappingDocumentSourceDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreModelMappingRevisionInputDto {
    pub revision: u64,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulateModelMappingInputDto {
    pub model: String,
    pub endpoint: ModelMappingEndpointKindDto,
    pub stream: bool,
    pub uses_tools: bool,
    pub uses_vision: bool,
    pub uses_reasoning: bool,
    pub draft: Option<ModelMappingDocumentDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingSimulationResultDto {
    pub requested_model: String,
    pub route_model: Option<String>,
    pub upstream_model: Option<String>,
    pub disposition: ModelMappingDispositionDto,
    pub matched_rule_id: Option<String>,
    pub diagnostics: Vec<ModelMappingDiagnosticDto>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMappingDispositionDto {
    Preserve,
    Mapped,
    Reject,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelMappingTraceStatusDto {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveRequestMappingTraceInputDto {
    pub request_log_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMappingTraceDto {
    pub request_log_id: String,
    pub status: ModelMappingTraceStatusDto,
    pub requested_model: Option<String>,
    pub route_model: Option<String>,
    pub upstream_model: Option<String>,
    pub mapping_revision: Option<u64>,
    pub resolution_fence: Option<String>,
    pub matched_rule_id: Option<String>,
    pub target_rank: Option<u16>,
    pub disposition: Option<ModelMappingDispositionDto>,
    pub failure_code: Option<String>,
}

pub const MODEL_MAPPING_TYPE: TypeDescriptor = TypeDescriptor {
    name: "ModelMapping",
    typescript: r#"export type ModelMappingDocumentDto = {
  formatVersion: number; baseRevision: number; policy: ModelMappingPolicyDto;
  rules: ModelMappingRuleDto[]; profiles: ModelMappingProfileDto[]; bindings: ModelMappingBindingDto[];
};
export type ModelMappingPolicyDto = { unmatchedModelBehavior: UnmatchedModelBehaviorDto };
export type UnmatchedModelBehaviorDto = "preserve" | "reject";
export type ModelMappingRuleDto = {
  id: string; priority: number; enabled: boolean; matcher: ModelMappingMatcherDto;
  conditions: ModelMappingConditionsDto; action: ModelMappingActionDto; note: string | null;
  revision: number; createdAtMs: number; updatedAtMs: number;
};
export type ModelMappingMatcherDto =
  | { kind: "exact"; model: string } | { kind: "default" } | { kind: "glob"; pattern: string };
export type ModelMappingConditionsDto = {
  endpointKinds: ModelMappingEndpointKindDto[]; stream: ModelMappingConditionModeDto;
  tools: ModelMappingConditionModeDto; vision: ModelMappingConditionModeDto; reasoning: ModelMappingConditionModeDto;
};
export type ModelMappingConditionModeDto = "any" | "required" | "forbidden";
export type ModelMappingEndpointKindDto = "chat_completions" | "responses" | "embeddings";
export type ModelMappingActionDto =
  | { kind: "map_fixed"; target: ModelMappingTargetRefDto }
  | { kind: "preserve" }
  | { kind: "reject"; rejectionKind: ModelMappingRejectionKindDto; message: string | null }
  | { kind: "map_fallback_chain"; targets: ModelMappingTargetRefDto[]; fallbackTrigger: ModelMappingFallbackTriggerDto };
export type ModelMappingTargetRefDto =
  | { kind: "literal"; upstreamModel: string } | { kind: "model_profile"; modelProfileId: string };
export type ModelMappingRejectionKindDto = "unsupported_model" | "policy" | "invalid_request";
export type ModelMappingFallbackTriggerDto = "no_eligible_target" | "retry_exhausted_before_output";
export type ModelMappingProfileDto = {
  id: string; canonicalModel: string; displayName: string | null; defaultUpstreamModel: string | null;
  status: ModelMappingProfileStatusDto; note: string | null; revision: number; createdAtMs: number; updatedAtMs: number;
};
export type ModelMappingProfileStatusDto = "active" | "archived";
export type ModelMappingBindingDto = {
  id: string; modelProfileId: string; stationId: string | null; stationKeyId: string | null;
  upstreamModel: string; source: ModelMappingBindingSourceDto; enabled: boolean; note: string | null;
  revision: number; createdAtMs: number; updatedAtMs: number;
};
export type ModelMappingBindingSourceDto = "user" | "discovered" | "migration";
export type ModelMappingDocumentStatusDto = {
  activeRevision: number; syncState: ModelMappingSyncStateDto; source: ModelMappingDocumentSourceDto;
  filePresent: boolean; lastErrorCode: string | null;
};
export type ModelMappingSyncStateDto = "synchronized" | "pending_materialization" | "external_change" | "error";
export type ModelMappingDocumentSourceDto = "ui" | "file" | "migration" | "restore" | "import";
export type ModelMappingDiagnosticDto = {
  severity: ModelMappingDiagnosticSeverityDto; code: string; path: string; message: string;
  ruleId: string | null; targetIndex: number | null;
};
export type ModelMappingDiagnosticSeverityDto = "error" | "warning" | "info";
export type ModelMappingLegacyReviewDto = {
  id: string; legacyAliasId: string; requestedModel: string; selectedTarget: string;
  discardedTarget: string | null; status: ModelMappingLegacyReviewStatusDto; createdAtMs: number;
};
export type ModelMappingLegacyReviewStatusDto = "pending" | "accepted" | "discarded";
export type ModelMappingWorkspaceDto = {
  document: ModelMappingDocumentDto; status: ModelMappingDocumentStatusDto; knownModelOptions: string[];
  legacyReviews: ModelMappingLegacyReviewDto[]; diagnostics: ModelMappingDiagnosticDto[]; candidateCount: number;
};
export type ModelMappingValidationResultDto = {
  valid: boolean; diagnostics: ModelMappingDiagnosticDto[]; normalizedDocument: ModelMappingDocumentDto | null;
};
export type ValidateModelMappingDocumentInputDto = { document: ModelMappingDocumentDto };
export type ApplyModelMappingDocumentInputDto = { document: ModelMappingDocumentDto; source: ModelMappingDocumentSourceDto };
export type RestoreModelMappingRevisionInputDto = { revision: number; expectedRevision: number };
export type SimulateModelMappingInputDto = {
  model: string; endpoint: ModelMappingEndpointKindDto; stream: boolean; usesTools: boolean;
  usesVision: boolean; usesReasoning: boolean; draft: ModelMappingDocumentDto | null;
};
export type ModelMappingSimulationResultDto = {
  requestedModel: string; routeModel: string | null; upstreamModel: string | null;
  disposition: ModelMappingDispositionDto; matchedRuleId: string | null; diagnostics: ModelMappingDiagnosticDto[];
};
export type ModelMappingDispositionDto = "preserve" | "mapped" | "reject";
export type ModelMappingTraceStatusDto = "available" | "unavailable";
export type ResolveRequestMappingTraceInputDto = { requestLogId: string };
export type ModelMappingTraceDto = {
  requestLogId: string; status: ModelMappingTraceStatusDto; requestedModel: string | null;
  routeModel: string | null; upstreamModel: string | null; mappingRevision: number | null;
  resolutionFence: string | null; matchedRuleId: string | null; targetRank: number | null;
  disposition: ModelMappingDispositionDto | null; failureCode: string | null;
};"#,
};

pub const MODEL_MAPPING_INPUT_TYPE: TypeDescriptor = TypeDescriptor {
    name: "ModelMappingInputs",
    typescript: r#"export type GetModelMappingWorkspaceInputDto = EmptyInputDto;
export type GetModelMappingDocumentInputDto = EmptyInputDto;"#,
};

pub fn validate_document(
    document: &ModelMappingDocumentDto,
) -> Result<(), crate::commands::error::CommandError> {
    if document.format_version != 1 {
        return Err(invalid_input(
            "document.formatVersion",
            "unsupported_version",
            "The model mapping document version is unsupported.",
        ));
    }
    if document.base_revision == 0 {
        return Err(invalid_input(
            "document.baseRevision",
            "out_of_range",
            "The model mapping document base revision must be positive.",
        ));
    }
    if document.rules.len() > MAX_RULES
        || document.profiles.len() > MAX_RULES
        || document.bindings.len() > MAX_RULES
    {
        return Err(invalid_input(
            "document",
            "too_many_items",
            "The model mapping document contains too many items.",
        ));
    }
    let mut ids = HashSet::new();
    for (index, rule) in document.rules.iter().enumerate() {
        validate_id(&format!("document.rules[{index}].id"), &rule.id)?;
        if !ids.insert(rule.id.as_str()) {
            return Err(invalid_input(
                "document.rules",
                "duplicate_id",
                "The model mapping document contains duplicate rule IDs.",
            ));
        }
        if rule.priority == 0 {
            return Err(invalid_input(
                "document.rules.priority",
                "out_of_range",
                "Rule priority must be positive.",
            ));
        }
        validate_optional_note(
            &format!("document.rules[{index}].note"),
            rule.note.as_deref(),
        )?;
        match &rule.matcher {
            ModelMappingMatcherDto::Exact { model } => {
                validate_model(&format!("document.rules[{index}].matcher.model"), model)?
            }
            ModelMappingMatcherDto::Default => {}
            ModelMappingMatcherDto::Glob { pattern } => {
                validate_glob(&format!("document.rules[{index}].matcher.pattern"), pattern)?
            }
        }
        match &rule.action {
            ModelMappingActionDto::MapFixed { target } => {
                validate_target(&format!("document.rules[{index}].action.target"), target)?
            }
            ModelMappingActionDto::Preserve => {}
            ModelMappingActionDto::Reject { message, .. } => validate_optional_note(
                &format!("document.rules[{index}].action.message"),
                message.as_deref(),
            )?,
            ModelMappingActionDto::MapFallbackChain { targets, .. } => {
                if !(2..=MAX_FALLBACK_TARGETS).contains(&targets.len()) {
                    return Err(dynamic_invalid_input(
                        &format!("document.rules[{index}].action.targets"),
                        "invalid_fallback",
                        "A fallback chain must contain between 2 and 3 targets.",
                    ));
                }
                let mut seen = HashSet::new();
                for (target_index, target) in targets.iter().enumerate() {
                    validate_target(
                        &format!("document.rules[{index}].action.targets[{target_index}]"),
                        target,
                    )?;
                    let key = serde_json::to_string(target).unwrap_or_default();
                    if !seen.insert(key) {
                        return Err(dynamic_invalid_input(
                            &format!("document.rules[{index}].action.targets"),
                            "duplicate_target",
                            "Fallback targets must be unique.",
                        ));
                    }
                }
            }
        }
    }
    let profile_ids: HashSet<&str> = document
        .profiles
        .iter()
        .map(|profile| profile.id.as_str())
        .collect();
    let mut canonical_models = HashSet::new();
    for (index, profile) in document.profiles.iter().enumerate() {
        validate_id(&format!("document.profiles[{index}].id"), &profile.id)?;
        validate_model(
            &format!("document.profiles[{index}].canonicalModel"),
            &profile.canonical_model,
        )?;
        if !canonical_models.insert(profile.canonical_model.as_str()) {
            return Err(invalid_input(
                "document.profiles",
                "duplicate_model",
                "Profile canonical models must be unique.",
            ));
        }
        if let Some(display_name) = profile.display_name.as_deref() {
            validate_text(
                &format!("document.profiles[{index}].displayName"),
                display_name,
                MAX_NOTE_BYTES,
            )?;
        }
        if let Some(default_model) = profile.default_upstream_model.as_deref() {
            validate_model(
                &format!("document.profiles[{index}].defaultUpstreamModel"),
                default_model,
            )?;
        }
        validate_optional_note(
            &format!("document.profiles[{index}].note"),
            profile.note.as_deref(),
        )?;
    }
    let mut binding_scopes = HashSet::new();
    let mut binding_ids = HashSet::new();
    for (index, binding) in document.bindings.iter().enumerate() {
        validate_id(&format!("document.bindings[{index}].id"), &binding.id)?;
        if !binding_ids.insert(binding.id.as_str()) {
            return Err(invalid_input(
                "document.bindings",
                "duplicate_id",
                "Binding IDs must be unique.",
            ));
        }
        if !profile_ids.contains(binding.model_profile_id.as_str()) {
            return Err(invalid_input(
                "document.bindings.modelProfileId",
                "invalid_profile_reference",
                "Binding profile does not exist.",
            ));
        }
        match (
            binding.station_key_id.as_deref(),
            binding.station_id.as_deref(),
        ) {
            (Some(key), None) => {
                validate_id(&format!("document.bindings[{index}].stationKeyId"), key)?
            }
            (None, Some(station)) => {
                validate_id(&format!("document.bindings[{index}].stationId"), station)?
            }
            _ => {
                return Err(invalid_input(
                    "document.bindings",
                    "invalid_scope",
                    "Exactly one binding scope is required.",
                ))
            }
        }
        let scope = (
            binding.model_profile_id.as_str(),
            binding.station_key_id.as_deref(),
            binding.station_id.as_deref(),
        );
        if !binding_scopes.insert(scope) {
            return Err(invalid_input(
                "document.bindings",
                "duplicate_scope",
                "A profile may have only one binding per station or key.",
            ));
        }
        validate_model(
            &format!("document.bindings[{index}].upstreamModel"),
            &binding.upstream_model,
        )?;
        validate_optional_note(
            &format!("document.bindings[{index}].note"),
            binding.note.as_deref(),
        )?;
    }
    Ok(())
}

fn validate_target(
    field: &str,
    target: &ModelMappingTargetRefDto,
) -> Result<(), crate::commands::error::CommandError> {
    match target {
        ModelMappingTargetRefDto::Literal { upstream_model } => {
            validate_model(field, upstream_model)
        }
        ModelMappingTargetRefDto::ModelProfile { model_profile_id } => {
            validate_id(&format!("{field}.modelProfileId"), model_profile_id)
        }
    }
}

fn validate_glob(field: &str, value: &str) -> Result<(), crate::commands::error::CommandError> {
    if value.is_empty() || value.len() > MAX_GLOB_BYTES || value.chars().any(char::is_control) {
        return Err(dynamic_invalid_input(
            field,
            "invalid_glob",
            "The glob pattern is invalid.",
        ));
    }
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
        }
    }
    if escaped {
        return Err(dynamic_invalid_input(
            field,
            "invalid_glob",
            "The glob pattern has a trailing escape.",
        ));
    }
    Ok(())
}

fn validate_model(field: &str, value: &str) -> Result<(), crate::commands::error::CommandError> {
    if value.trim() != value
        || value.is_empty()
        || value.len() > MAX_MODEL_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(dynamic_invalid_input(
            field,
            "invalid_model",
            "The model name is invalid.",
        ));
    }
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), crate::commands::error::CommandError> {
    if value.is_empty() || value.len() > MAX_ID_BYTES || value.chars().any(char::is_control) {
        return Err(dynamic_invalid_input(
            field,
            "invalid_id",
            "The identifier is invalid.",
        ));
    }
    Ok(())
}

fn validate_optional_note(
    field: &str,
    value: Option<&str>,
) -> Result<(), crate::commands::error::CommandError> {
    if value.is_some_and(|text| text.len() > MAX_NOTE_BYTES || text.chars().any(char::is_control)) {
        return Err(dynamic_invalid_input(
            field,
            "invalid_text",
            "The text value is invalid.",
        ));
    }
    Ok(())
}

fn validate_text(
    field: &str,
    value: &str,
    max: usize,
) -> Result<(), crate::commands::error::CommandError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(dynamic_invalid_input(
            field,
            "invalid_text",
            "The text value is invalid.",
        ));
    }
    Ok(())
}

fn dynamic_invalid_input(
    field: &str,
    code: &'static str,
    message: &'static str,
) -> crate::commands::error::CommandError {
    crate::commands::error::CommandError::try_new(
        crate::commands::error::CommandErrorCode::InvalidInput,
        "The command input is invalid.",
        false,
        Some(crate::commands::error::PublicErrorDetails::Validation {
            fields: vec![crate::commands::error::PublicFieldError {
                field: field.to_string(),
                code: code.to_string(),
                message: message.to_string(),
            }],
        }),
        None,
    )
    .expect("bounded dynamic validation error")
}

impl ValidateModelMappingDocumentInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The model mapping validation payload is invalid.",
            )
        })?;
        validate_document(&input.document)?;
        Ok(input)
    }
}

impl ApplyModelMappingDocumentInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The model mapping apply payload is invalid.",
            )
        })?;
        validate_document(&input.document)?;
        Ok(input)
    }
}

impl RestoreModelMappingRevisionInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The model mapping restore payload is invalid.",
            )
        })?;
        if input.revision == 0 || input.expected_revision == 0 {
            return Err(invalid_input(
                "revision",
                "out_of_range",
                "The revision must be positive.",
            ));
        }
        Ok(input)
    }
}

impl ResolveRequestMappingTraceInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The mapping trace payload is invalid.",
            )
        })?;
        validate_stable_id("requestLogId", &input.request_log_id)?;
        Ok(input)
    }
}

fn validate_stable_id(
    field: &'static str,
    value: &str,
) -> Result<(), crate::commands::error::CommandError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'));
    if !valid {
        return Err(invalid_input(
            field,
            "invalid_id",
            "The stable ID is invalid.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> ModelMappingDocumentDto {
        ModelMappingDocumentDto {
            format_version: 1,
            base_revision: 1,
            policy: ModelMappingPolicyDto {
                unmatched_model_behavior: UnmatchedModelBehaviorDto::Preserve,
            },
            rules: vec![ModelMappingRuleDto {
                id: "rule-1".into(),
                priority: 10,
                enabled: true,
                matcher: ModelMappingMatcherDto::Exact {
                    model: "codex-5.4".into(),
                },
                conditions: ModelMappingConditionsDto {
                    endpoint_kinds: vec![],
                    stream: ModelMappingConditionModeDto::Any,
                    tools: ModelMappingConditionModeDto::Any,
                    vision: ModelMappingConditionModeDto::Any,
                    reasoning: ModelMappingConditionModeDto::Any,
                },
                action: ModelMappingActionDto::MapFixed {
                    target: ModelMappingTargetRefDto::Literal {
                        upstream_model: "deepseek-v4-flash".into(),
                    },
                },
                note: None,
                revision: 1,
                created_at_ms: 0,
                updated_at_ms: 0,
            }],
            profiles: vec![],
            bindings: vec![],
        }
    }

    #[test]
    fn phase_one_document_accepts_exact_literal_mapping() {
        validate_document(&document()).expect("valid phase-one document");
    }

    #[test]
    fn apply_payload_accepts_camel_case_target_fields() {
        let payload = serde_json::json!({
            "document": serde_json::to_value(document()).expect("serialize document"),
            "source": "ui",
        });
        let parsed = ApplyModelMappingDocumentInputDto::parse(payload)
            .expect("camelCase apply payload should match the IPC contract");
        match &parsed.document.rules[0].action {
            ModelMappingActionDto::MapFixed {
                target: ModelMappingTargetRefDto::Literal { upstream_model },
            } => assert_eq!(upstream_model, "deepseek-v4-flash"),
            _ => panic!("expected a fixed literal mapping"),
        }
    }

    #[test]
    fn document_accepts_glob_and_rejects_unknown_fields() {
        let mut value = serde_json::to_value(document()).expect("serialize");
        value["rules"][0]["matcher"] = serde_json::json!({ "kind": "glob", "pattern": "codex-*" });
        assert!(ValidateModelMappingDocumentInputDto::parse(
            serde_json::json!({ "document": value })
        )
        .is_ok());
        assert!(ValidateModelMappingDocumentInputDto::parse(serde_json::json!({ "document": serde_json::to_value(document()).unwrap(), "extra": true })).is_err());
    }

    #[test]
    fn document_rejects_duplicate_rule_ids_and_oversized_model() {
        let mut value = document();
        value.rules.push(value.rules[0].clone());
        assert!(validate_document(&value).is_err());
        value.rules.truncate(1);
        if let ModelMappingMatcherDto::Exact { model } = &mut value.rules[0].matcher {
            *model = "x".repeat(MAX_MODEL_BYTES + 1);
        }
        assert!(validate_document(&value).is_err());
    }

    #[test]
    fn mapping_trace_input_rejects_url_and_query_shaped_ids() {
        for request_log_id in [
            "https://provider.example/v1?token=fake",
            "request/log?authorization=Bearer-fake",
        ] {
            let error = ResolveRequestMappingTraceInputDto::parse(
                serde_json::json!({ "requestLogId": request_log_id }),
            )
            .expect_err("URL/query-shaped IDs must not be echoed by trace responses");
            let serialized = serde_json::to_string(&error).expect("serialize error");
            assert!(!serialized.contains(request_log_id));
            assert!(!serialized.contains("provider.example"));
            assert!(!serialized.contains("token="));
        }
    }

    #[test]
    fn mapping_trace_input_accepts_generated_request_ids() {
        let input = ResolveRequestMappingTraceInputDto::parse(serde_json::json!({
            "requestLogId": "req_0198108c8411_00003039_0000000000000001"
        }))
        .expect("generated request IDs use the stable ID alphabet");
        assert_eq!(
            input.request_log_id,
            "req_0198108c8411_00003039_0000000000000001"
        );
    }
}
