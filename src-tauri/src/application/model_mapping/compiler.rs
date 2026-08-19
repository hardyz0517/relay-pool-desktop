use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use crate::models::model_mapping::{
    normalize_model_name, Action, ConditionRequirement, EndpointKind, Matcher,
    ModelMappingDocumentV1, ModelMappingPolicy, ModelMappingRule, ModelOfferingBinding,
    ModelProfile, RuleConditions, TargetRef, MAX_BINDINGS, MAX_MODEL_NAME_BYTES, MAX_NOTE_BYTES,
    MAX_PRIORITY, MAX_PROFILES, MAX_REJECTION_MESSAGE_BYTES, MAX_RULES, MAX_RULE_ID_BYTES,
    MAX_TARGETS_PER_RULE, MODEL_MAPPING_FORMAT_VERSION,
};

use super::glob::{CompiledGlob, GlobCompileError, GlobIntersectionError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DiagnosticCode {
    InvalidDocument,
    InvalidRule,
    DuplicateRuleId,
    DuplicateBindingScope,
    InvalidModelName,
    InvalidCondition,
    InvalidProfileReference,
    InvalidDefault,
    RuleConflict,
    RuleShadowed,
    EmptyTarget,
    DuplicateTarget,
    InvalidTargetReference,
    InvalidFallback,
    InvalidGlob,
    GlobAnalysisBudget,
}

impl DiagnosticCode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidDocument => "model_mapping_invalid_document",
            Self::InvalidRule => "model_mapping_invalid_rule",
            Self::DuplicateRuleId => "model_mapping_duplicate_rule_id",
            Self::DuplicateBindingScope => "model_mapping_duplicate_binding_scope",
            Self::InvalidModelName => "model_mapping_invalid_model_name",
            Self::InvalidCondition => "model_mapping_invalid_condition",
            Self::InvalidProfileReference => "model_mapping_invalid_profile_reference",
            Self::InvalidDefault => "model_mapping_invalid_default",
            Self::RuleConflict => "model_mapping_conflict",
            Self::RuleShadowed => "model_mapping_shadowed_rule",
            Self::EmptyTarget => "model_mapping_empty_target",
            Self::DuplicateTarget => "model_mapping_duplicate_target",
            Self::InvalidTargetReference => "model_mapping_invalid_target_reference",
            Self::InvalidFallback => "model_mapping_invalid_fallback",
            Self::InvalidGlob => "model_mapping_invalid_glob",
            Self::GlobAnalysisBudget => "model_mapping_glob_analysis_budget",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelMappingDiagnostic {
    pub(crate) code: DiagnosticCode,
    pub(crate) rule_id: Option<String>,
    pub(crate) path: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompileError {
    pub(crate) diagnostics: Vec<ModelMappingDiagnostic>,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "model mapping document has {} error(s)",
            self.diagnostics.len()
        )
    }
}

impl std::error::Error for CompileError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledModelMappingConfiguration {
    pub(crate) mapping_revision: u64,
    pub(crate) model_resolution_fence: String,
    pub(crate) policy: ModelMappingPolicy,
    pub(crate) rules: Vec<CompiledRule>,
    pub(crate) profiles: Vec<ModelProfile>,
    pub(crate) bindings: Vec<ModelOfferingBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledRule {
    pub(crate) id: String,
    pub(crate) priority: u32,
    pub(crate) matcher: CompiledMatcher,
    pub(crate) conditions: RuleConditions,
    pub(crate) action: Action,
    pub(crate) revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompiledMatcher {
    Exact(String),
    Glob(CompiledGlob),
    Default,
}

#[cfg(test)]
pub(crate) fn compile(
    document: &ModelMappingDocumentV1,
) -> Result<CompiledModelMappingConfiguration, CompileError> {
    compile_at_revision(document, document.base_revision)
}

pub(crate) fn compile_at_revision(
    document: &ModelMappingDocumentV1,
    mapping_revision: u64,
) -> Result<CompiledModelMappingConfiguration, CompileError> {
    let mut diagnostics = Vec::new();
    if document.format_version != MODEL_MAPPING_FORMAT_VERSION {
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidDocument,
            None,
            "formatVersion",
            format!("formatVersion must be {}", MODEL_MAPPING_FORMAT_VERSION),
        ));
    }
    if document.rules.len() > MAX_RULES {
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidDocument,
            None,
            "rules",
            format!("at most {MAX_RULES} rules are supported"),
        ));
    }
    validate_profiles_and_bindings(document, &mut diagnostics);
    validate_rule_targets(document, &mut diagnostics);
    let mut ids = BTreeSet::new();
    let mut compiled = Vec::with_capacity(document.rules.len());
    for (index, rule) in document.rules.iter().enumerate() {
        if !ids.insert(rule.id.clone()) {
            diagnostics.push(diagnostic(
                DiagnosticCode::DuplicateRuleId,
                Some(rule.id.clone()),
                format!("rules[{index}].id"),
                "rule IDs must be unique".to_string(),
            ));
        }
        if let Some(compiled_rule) = compile_rule(rule, index, &mut diagnostics) {
            if rule.enabled {
                compiled.push(compiled_rule);
            }
        }
    }

    validate_default_rules(&compiled, &mut diagnostics);
    validate_overlaps_and_shadowing(&compiled, &mut diagnostics);
    diagnostics.sort_by(|left, right| {
        (
            left.rule_id.as_deref().unwrap_or(""),
            left.code,
            left.path.as_str(),
            left.message.as_str(),
        )
            .cmp(&(
                right.rule_id.as_deref().unwrap_or(""),
                right.code,
                right.path.as_str(),
                right.message.as_str(),
            ))
    });
    if !diagnostics.is_empty() {
        return Err(CompileError { diagnostics });
    }

    compiled.sort_by(rule_order);
    Ok(CompiledModelMappingConfiguration {
        mapping_revision,
        model_resolution_fence: resolution_fence(
            mapping_revision,
            &compiled,
            &document.profiles,
            &document.bindings,
        ),
        policy: document.policy.clone(),
        rules: compiled,
        profiles: document.profiles.clone(),
        bindings: document.bindings.clone(),
    })
}

fn validate_rule_targets(
    document: &ModelMappingDocumentV1,
    diagnostics: &mut Vec<ModelMappingDiagnostic>,
) {
    let profiles = document
        .profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect::<std::collections::BTreeMap<_, _>>();
    for (index, rule) in document.rules.iter().enumerate() {
        let targets = match &rule.action {
            Action::MapFixed { target } => std::slice::from_ref(target),
            Action::MapFallbackChain { targets, .. } => targets.as_slice(),
            Action::Preserve | Action::Reject { .. } => continue,
        };
        if matches!(&rule.action, Action::MapFallbackChain { .. })
            && !(2..=MAX_TARGETS_PER_RULE).contains(&targets.len())
        {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidFallback,
                Some(rule.id.clone()),
                format!("rules[{index}].action.targets"),
                format!("fallback chains must contain 2..={MAX_TARGETS_PER_RULE} targets"),
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for (target_index, target) in targets.iter().enumerate() {
            let key = match target {
                TargetRef::Literal { upstream_model } => {
                    format!("literal:{upstream_model}")
                }
                TargetRef::ModelProfile { model_profile_id } => {
                    if let Some(profile) = profiles.get(model_profile_id.as_str()) {
                        if profile.status
                            == crate::models::model_mapping::ModelProfileStatus::Archived
                        {
                            diagnostics.push(diagnostic(
                                DiagnosticCode::InvalidTargetReference,
                                Some(rule.id.clone()),
                                format!("rules[{index}].action.targets[{target_index}]"),
                                "target profile is archived".to_string(),
                            ));
                        }
                    } else {
                        diagnostics.push(diagnostic(
                            DiagnosticCode::InvalidTargetReference,
                            Some(rule.id.clone()),
                            format!("rules[{index}].action.targets[{target_index}]"),
                            "target profile does not exist in the document".to_string(),
                        ));
                    }
                    format!("profile:{model_profile_id}")
                }
            };
            if !seen.insert(key) {
                diagnostics.push(diagnostic(
                    DiagnosticCode::DuplicateTarget,
                    Some(rule.id.clone()),
                    format!("rules[{index}].action.targets[{target_index}]"),
                    "target references must be unique within a fallback chain".to_string(),
                ));
            }
        }
    }
}

fn validate_profiles_and_bindings(
    document: &ModelMappingDocumentV1,
    diagnostics: &mut Vec<ModelMappingDiagnostic>,
) {
    if document.profiles.len() > MAX_PROFILES {
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidDocument,
            None,
            "profiles",
            format!("at most {MAX_PROFILES} profiles are supported"),
        ));
    }
    if document.bindings.len() > MAX_BINDINGS {
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidDocument,
            None,
            "bindings",
            format!("at most {MAX_BINDINGS} bindings are supported"),
        ));
    }
    let mut profile_ids = BTreeSet::new();
    let mut canonical_models = BTreeSet::new();
    for (index, profile) in document.profiles.iter().enumerate() {
        let path = format!("profiles[{index}]");
        if !valid_identifier(&profile.id, MAX_RULE_ID_BYTES) {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidRule,
                None,
                format!("{path}.id"),
                "profile id is invalid".to_string(),
            ));
        }
        if !profile_ids.insert(profile.id.clone()) {
            diagnostics.push(diagnostic(
                DiagnosticCode::DuplicateRuleId,
                None,
                format!("{path}.id"),
                "profile IDs must be unique".to_string(),
            ));
        }
        if let Some(normalized_canonical) = normalize_model_name(&profile.canonical_model) {
            if !canonical_models.insert(normalized_canonical) {
                diagnostics.push(diagnostic(
                    DiagnosticCode::RuleConflict,
                    None,
                    format!("{path}.canonicalModel"),
                    "canonical model names must be unique".to_string(),
                ));
            }
        } else {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidModelName,
                None,
                format!("{path}.canonicalModel"),
                "canonical model is invalid".to_string(),
            ));
        }
        if !valid_display_name(&profile.display_name) {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidRule,
                None,
                format!("{path}.displayName"),
                "display name is invalid".to_string(),
            ));
        }
        if profile
            .note
            .as_deref()
            .is_some_and(|note| !valid_note(note))
        {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidRule,
                None,
                format!("{path}.note"),
                "profile note is invalid".to_string(),
            ));
        }
        if profile
            .default_upstream_model
            .as_deref()
            .is_some_and(|model| normalize_model_name(model).is_none())
        {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidModelName,
                None,
                format!("{path}.defaultUpstreamModel"),
                "default upstream model is invalid".to_string(),
            ));
        }
        validate_timestamp_metadata(
            &path,
            profile.created_at_ms,
            profile.updated_at_ms,
            diagnostics,
        );
    }
    let mut binding_ids = BTreeSet::new();
    let mut binding_key_scopes = BTreeSet::new();
    let mut binding_station_scopes = BTreeSet::new();
    for (index, binding) in document.bindings.iter().enumerate() {
        let path = format!("bindings[{index}]");
        if !valid_identifier(&binding.id, MAX_RULE_ID_BYTES) {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidRule,
                None,
                format!("{path}.id"),
                "binding id is invalid".to_string(),
            ));
        }
        if !binding_ids.insert(binding.id.clone()) {
            diagnostics.push(diagnostic(
                DiagnosticCode::DuplicateRuleId,
                None,
                format!("{path}.id"),
                "binding IDs must be unique".to_string(),
            ));
        }
        if !valid_identifier(&binding.model_profile_id, MAX_RULE_ID_BYTES) {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidRule,
                None,
                format!("{path}.modelProfileId"),
                "binding profile ID is invalid".to_string(),
            ));
        } else if !profile_ids.contains(&binding.model_profile_id) {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidProfileReference,
                None,
                format!("{path}.modelProfileId"),
                "binding must reference a profile in the same document".to_string(),
            ));
        }
        if binding.station_key_id.is_some() == binding.station_id.is_some() {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidCondition,
                None,
                format!("{path}.stationKeyId/stationId"),
                "exactly one binding scope must be set".to_string(),
            ));
        }
        if binding
            .station_key_id
            .as_deref()
            .or(binding.station_id.as_deref())
            .is_some_and(|id| !valid_identifier(id, MAX_RULE_ID_BYTES))
        {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidRule,
                None,
                format!("{path}.stationKeyId/stationId"),
                "binding scope ID is invalid".to_string(),
            ));
        }
        if let Some(station_key_id) = binding.station_key_id.as_deref() {
            if !binding_key_scopes.insert((binding.model_profile_id.as_str(), station_key_id)) {
                diagnostics.push(diagnostic(
                    DiagnosticCode::DuplicateBindingScope,
                    None,
                    format!("{path}.stationKeyId"),
                    "a profile may have at most one binding for a station key".to_string(),
                ));
            }
        }
        if let Some(station_id) = binding.station_id.as_deref() {
            if !binding_station_scopes.insert((binding.model_profile_id.as_str(), station_id)) {
                diagnostics.push(diagnostic(
                    DiagnosticCode::DuplicateBindingScope,
                    None,
                    format!("{path}.stationId"),
                    "a profile may have at most one binding for a station".to_string(),
                ));
            }
        }
        if normalize_model_name(&binding.upstream_model).is_none() {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidModelName,
                None,
                format!("{path}.upstreamModel"),
                "binding upstream model is invalid".to_string(),
            ));
        }
        if binding
            .note
            .as_deref()
            .is_some_and(|note| !valid_note(note))
        {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidRule,
                None,
                format!("{path}.note"),
                "binding note is invalid".to_string(),
            ));
        }
        validate_timestamp_metadata(
            &path,
            binding.created_at_ms,
            binding.updated_at_ms,
            diagnostics,
        );
    }
}

fn valid_identifier(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_bytes
        && !value
            .chars()
            .any(|character| character == '\0' || character.is_control())
}

fn valid_display_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_NOTE_BYTES
        && !value.chars().any(|character| character.is_control())
}

fn valid_note(value: &str) -> bool {
    value.len() <= MAX_NOTE_BYTES
        && !value
            .chars()
            .any(|character| character == '\0' || character.is_control())
}

fn validate_timestamp_metadata(
    path: &str,
    created_at_ms: i64,
    updated_at_ms: i64,
    diagnostics: &mut Vec<ModelMappingDiagnostic>,
) {
    if created_at_ms < 0 || updated_at_ms < 0 || updated_at_ms < created_at_ms {
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidRule,
            None,
            path.to_string(),
            "object timestamps are invalid".to_string(),
        ));
    }
}

fn compile_rule(
    rule: &ModelMappingRule,
    index: usize,
    diagnostics: &mut Vec<ModelMappingDiagnostic>,
) -> Option<CompiledRule> {
    let path = |field: &str| format!("rules[{index}].{field}");
    let mut valid = true;
    if rule.id.trim().is_empty()
        || rule.id.len() > MAX_RULE_ID_BYTES
        || rule
            .id
            .chars()
            .any(|character| character == '\0' || character.is_control())
    {
        valid = false;
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidRule,
            Some(rule.id.clone()),
            path("id"),
            format!("rule id must be 1..={MAX_RULE_ID_BYTES} bytes"),
        ));
    }
    if !(1..=MAX_PRIORITY).contains(&rule.priority) {
        valid = false;
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidRule,
            Some(rule.id.clone()),
            path("priority"),
            format!("priority must be in 1..={MAX_PRIORITY}"),
        ));
    }
    if rule.note.as_ref().is_some_and(|note| {
        note.len() > MAX_NOTE_BYTES
            || note
                .chars()
                .any(|character| character == '\0' || character.is_control())
    }) {
        valid = false;
        diagnostics.push(diagnostic(
            DiagnosticCode::InvalidRule,
            Some(rule.id.clone()),
            path("note"),
            format!(
                "note must be at most {MAX_NOTE_BYTES} bytes and contain no control characters"
            ),
        ));
    }

    let matcher = match &rule.matcher {
        Matcher::Exact { model } => {
            match normalize_model_name(model) {
                Some(model) => CompiledMatcher::Exact(model),
                None => {
                    valid = false;
                    diagnostics.push(diagnostic(
                    DiagnosticCode::InvalidModelName,
                    Some(rule.id.clone()),
                    path("matcher.model"),
                    format!("model must be 1..={MAX_MODEL_NAME_BYTES} bytes with no control characters"),
                ));
                    CompiledMatcher::Default
                }
            }
        }
        Matcher::Glob { pattern } => match CompiledGlob::compile(pattern) {
            Ok(glob) => CompiledMatcher::Glob(glob),
            Err(error) => {
                valid = false;
                diagnostics.push(diagnostic(
                    match error {
                        GlobCompileError::TooLong | GlobCompileError::TooManyTokens => {
                            DiagnosticCode::InvalidGlob
                        }
                        GlobCompileError::Empty
                        | GlobCompileError::TrailingEscape
                        | GlobCompileError::InvalidControl => DiagnosticCode::InvalidGlob,
                    },
                    Some(rule.id.clone()),
                    path("matcher.pattern"),
                    error.to_string(),
                ));
                CompiledMatcher::Default
            }
        },
        Matcher::Default => CompiledMatcher::Default,
    };

    if !validate_conditions(&rule.conditions, &rule.id, &path("conditions"), diagnostics) {
        valid = false;
    }
    if let Action::Reject { message, .. } = &rule.action {
        if message.as_ref().is_some_and(|message| {
            message.len() > MAX_REJECTION_MESSAGE_BYTES
                || message
                    .chars()
                    .any(|character| character == '\0' || character.is_control())
        }) {
            valid = false;
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidRule,
                Some(rule.id.clone()),
                path("action.message"),
                format!("rejection message must be at most {MAX_REJECTION_MESSAGE_BYTES} bytes and contain no control characters"),
            ));
        }
    }

    let action = match &rule.action {
        Action::MapFixed { target } => Action::MapFixed {
            target: compile_target(
                target,
                &rule.id,
                path("action.target"),
                diagnostics,
                &mut valid,
            ),
        },
        Action::MapFallbackChain {
            targets,
            fallback_trigger,
        } => Action::MapFallbackChain {
            targets: targets
                .iter()
                .enumerate()
                .map(|(target_index, target)| {
                    compile_target(
                        target,
                        &rule.id,
                        format!("rules[{index}].action.targets[{target_index}]"),
                        diagnostics,
                        &mut valid,
                    )
                })
                .collect(),
            fallback_trigger: *fallback_trigger,
        },
        action => action.clone(),
    };
    if valid {
        Some(CompiledRule {
            id: rule.id.clone(),
            priority: rule.priority,
            matcher,
            conditions: rule.conditions.clone(),
            action,
            revision: rule.revision,
        })
    } else {
        None
    }
}

fn compile_target(
    target: &TargetRef,
    rule_id: &str,
    path: String,
    diagnostics: &mut Vec<ModelMappingDiagnostic>,
    valid: &mut bool,
) -> TargetRef {
    match target {
        TargetRef::Literal { upstream_model } => match normalize_model_name(upstream_model) {
            Some(model) => TargetRef::Literal {
                upstream_model: model,
            },
            None => {
                *valid = false;
                diagnostics.push(diagnostic(
                    DiagnosticCode::EmptyTarget,
                    Some(rule_id.to_string()),
                    format!("{path}.upstreamModel"),
                    "literal target must be a non-empty valid model name".to_string(),
                ));
                target.clone()
            }
        },
        TargetRef::ModelProfile { model_profile_id } => TargetRef::ModelProfile {
            model_profile_id: model_profile_id.clone(),
        },
    }
}

fn validate_conditions(
    conditions: &RuleConditions,
    rule_id: &str,
    path: &str,
    diagnostics: &mut Vec<ModelMappingDiagnostic>,
) -> bool {
    let mut valid = true;
    if let Some(endpoints) = &conditions.endpoint_kinds {
        if endpoints.is_empty() {
            valid = false;
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidCondition,
                Some(rule_id.to_string()),
                path.to_string(),
                "endpointKinds cannot be empty".to_string(),
            ));
        }
        let mut unique = BTreeSet::new();
        if endpoints.iter().any(|endpoint| !unique.insert(*endpoint)) {
            valid = false;
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidCondition,
                Some(rule_id.to_string()),
                path.to_string(),
                "endpointKinds cannot contain duplicates".to_string(),
            ));
        }
        if endpoints
            .iter()
            .any(|endpoint| endpoint.is_mapping_bypass())
        {
            valid = false;
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidCondition,
                Some(rule_id.to_string()),
                path.to_string(),
                "mapping conditions may only reference inference endpoints".to_string(),
            ));
        }
    }
    valid
}

fn validate_default_rules(rules: &[CompiledRule], diagnostics: &mut Vec<ModelMappingDiagnostic>) {
    let defaults: Vec<&CompiledRule> = rules
        .iter()
        .filter(|rule| matches!(rule.matcher, CompiledMatcher::Default))
        .collect();
    let unconditional: Vec<&CompiledRule> = defaults
        .iter()
        .copied()
        .filter(|rule| is_unconditional(&rule.conditions))
        .collect();
    if unconditional.len() > 1 {
        for rule in &unconditional {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidDefault,
                Some(rule.id.clone()),
                "matcher".to_string(),
                "only one unconditional default rule may be enabled".to_string(),
            ));
        }
    }
    for default in unconditional {
        if rules
            .iter()
            .any(|other| other.id != default.id && other.priority <= default.priority)
        {
            diagnostics.push(diagnostic(
                DiagnosticCode::InvalidDefault,
                Some(default.id.clone()),
                "priority".to_string(),
                "an unconditional default must have the lowest enabled priority".to_string(),
            ));
        }
    }
}

fn validate_overlaps_and_shadowing(
    rules: &[CompiledRule],
    diagnostics: &mut Vec<ModelMappingDiagnostic>,
) {
    for (left_index, left) in rules.iter().enumerate() {
        for right in rules.iter().skip(left_index + 1) {
            if !conditions_intersect(&left.conditions, &right.conditions) {
                continue;
            }
            let matcher_intersects = match matcher_intersects(&left.matcher, &right.matcher) {
                Ok(value) => value,
                Err(GlobIntersectionError::BudgetExceeded) => {
                    diagnostics.push(diagnostic(
                        DiagnosticCode::GlobAnalysisBudget,
                        Some(left.id.clone()),
                        "matcher",
                        format!(
                            "glob overlap analysis with rule {} exceeded its bounded budget",
                            right.id
                        ),
                    ));
                    false
                }
            };
            if !matcher_intersects {
                continue;
            }
            if left.priority == right.priority && left.action != right.action {
                diagnostics.push(diagnostic(
                    DiagnosticCode::RuleConflict,
                    Some(left.id.clone()),
                    "priority".to_string(),
                    format!("rule overlaps same-priority rule {}", right.id),
                ));
                diagnostics.push(diagnostic(
                    DiagnosticCode::RuleConflict,
                    Some(right.id.clone()),
                    "priority".to_string(),
                    format!("rule overlaps same-priority rule {}", left.id),
                ));
            }
            if left.priority > right.priority
                && matcher_covers(&left.matcher, &right.matcher)
                && conditions_covers(&left.conditions, &right.conditions)
            {
                diagnostics.push(shadow_diagnostic(left, right));
            } else if right.priority > left.priority
                && matcher_covers(&right.matcher, &left.matcher)
                && conditions_covers(&right.conditions, &left.conditions)
            {
                diagnostics.push(shadow_diagnostic(right, left));
            }
        }
    }
}

fn shadow_diagnostic(higher: &CompiledRule, lower: &CompiledRule) -> ModelMappingDiagnostic {
    diagnostic(
        DiagnosticCode::RuleShadowed,
        Some(lower.id.clone()),
        "priority".to_string(),
        format!(
            "rule is completely shadowed by higher-priority rule {}",
            higher.id
        ),
    )
}

fn rule_order(left: &CompiledRule, right: &CompiledRule) -> std::cmp::Ordering {
    right
        .priority
        .cmp(&left.priority)
        .then_with(|| matcher_specificity(&right.matcher).cmp(&matcher_specificity(&left.matcher)))
        .then_with(|| {
            matcher_literal_specificity(&right.matcher)
                .cmp(&matcher_literal_specificity(&left.matcher))
        })
        .then_with(|| left.id.cmp(&right.id))
}

fn matcher_specificity(matcher: &CompiledMatcher) -> u8 {
    match matcher {
        CompiledMatcher::Exact(_) => 3,
        CompiledMatcher::Glob(_) => 2,
        CompiledMatcher::Default => 0,
    }
}

fn matcher_literal_specificity(matcher: &CompiledMatcher) -> usize {
    match matcher {
        CompiledMatcher::Glob(glob) => glob.literal_count(),
        _ => 0,
    }
}

fn matcher_intersects(
    left: &CompiledMatcher,
    right: &CompiledMatcher,
) -> Result<bool, GlobIntersectionError> {
    match (left, right) {
        (CompiledMatcher::Default, _) | (_, CompiledMatcher::Default) => Ok(true),
        (CompiledMatcher::Exact(left), CompiledMatcher::Exact(right)) => Ok(left == right),
        (CompiledMatcher::Glob(glob), CompiledMatcher::Exact(exact))
        | (CompiledMatcher::Exact(exact), CompiledMatcher::Glob(glob)) => Ok(glob.matches(exact)),
        (CompiledMatcher::Glob(left), CompiledMatcher::Glob(right)) => left.intersects(right),
    }
}

fn matcher_covers(left: &CompiledMatcher, right: &CompiledMatcher) -> bool {
    match (left, right) {
        (CompiledMatcher::Default, _) => true,
        (CompiledMatcher::Exact(left), CompiledMatcher::Exact(right)) => left == right,
        (CompiledMatcher::Glob(glob), CompiledMatcher::Exact(exact)) => glob.matches(exact),
        (CompiledMatcher::Glob(left), CompiledMatcher::Glob(right)) => left == right,
        (CompiledMatcher::Exact(_), CompiledMatcher::Default) => false,
        (CompiledMatcher::Exact(_), CompiledMatcher::Glob(_)) => false,
        (CompiledMatcher::Glob(_), CompiledMatcher::Default) => false,
    }
}

fn conditions_intersect(left: &RuleConditions, right: &RuleConditions) -> bool {
    endpoint_intersects(
        left.endpoint_kinds.as_deref(),
        right.endpoint_kinds.as_deref(),
    ) && requirement_intersects(left.stream, right.stream)
        && requirement_intersects(left.tools, right.tools)
        && requirement_intersects(left.vision, right.vision)
        && requirement_intersects(left.reasoning, right.reasoning)
}

fn conditions_covers(left: &RuleConditions, right: &RuleConditions) -> bool {
    endpoint_covers(
        left.endpoint_kinds.as_deref(),
        right.endpoint_kinds.as_deref(),
    ) && requirement_covers(left.stream, right.stream)
        && requirement_covers(left.tools, right.tools)
        && requirement_covers(left.vision, right.vision)
        && requirement_covers(left.reasoning, right.reasoning)
}

fn endpoint_intersects(left: Option<&[EndpointKind]>, right: Option<&[EndpointKind]>) -> bool {
    match (left, right) {
        (None, _) | (_, None) => true,
        (Some(left), Some(right)) => left.iter().any(|item| right.contains(item)),
    }
}

fn endpoint_covers(left: Option<&[EndpointKind]>, right: Option<&[EndpointKind]>) -> bool {
    match (left, right) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(left), Some(right)) => right.iter().all(|item| left.contains(item)),
    }
}

fn requirement_intersects(left: ConditionRequirement, right: ConditionRequirement) -> bool {
    !matches!(
        (left, right),
        (
            ConditionRequirement::Required,
            ConditionRequirement::Forbidden
        ) | (
            ConditionRequirement::Forbidden,
            ConditionRequirement::Required
        )
    )
}

fn requirement_covers(left: ConditionRequirement, right: ConditionRequirement) -> bool {
    matches!(left, ConditionRequirement::Any) || left == right
}

fn is_unconditional(conditions: &RuleConditions) -> bool {
    conditions.endpoint_kinds.is_none()
        && conditions.stream == ConditionRequirement::Any
        && conditions.tools == ConditionRequirement::Any
        && conditions.vision == ConditionRequirement::Any
        && conditions.reasoning == ConditionRequirement::Any
}

fn diagnostic(
    code: DiagnosticCode,
    rule_id: Option<String>,
    path: impl Into<String>,
    message: String,
) -> ModelMappingDiagnostic {
    ModelMappingDiagnostic {
        code,
        rule_id,
        path: path.into(),
        message,
    }
}

fn resolution_fence(
    revision: u64,
    rules: &[CompiledRule],
    profiles: &[crate::models::model_mapping::ModelProfile],
    bindings: &[crate::models::model_mapping::ModelOfferingBinding],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(revision.to_be_bytes());
    for rule in rules {
        hasher.update(rule.id.as_bytes());
        hasher.update(rule.revision.to_be_bytes());
        hasher.update(rule.priority.to_be_bytes());
    }
    let mut sorted_profiles = profiles.iter().collect::<Vec<_>>();
    sorted_profiles.sort_by(|left, right| left.id.cmp(&right.id));
    for profile in sorted_profiles {
        hasher.update(profile.id.as_bytes());
        hasher.update(profile.revision.to_be_bytes());
    }
    let mut sorted_bindings = bindings.iter().collect::<Vec<_>>();
    sorted_bindings.sort_by(|left, right| left.id.cmp(&right.id));
    for binding in sorted_bindings {
        hasher.update(binding.id.as_bytes());
        hasher.update(binding.revision.to_be_bytes());
    }
    format!("mapping-{}", hex_digest(hasher.finalize().as_slice()))
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DocumentCodecError {
    #[error("invalid model mapping JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported model mapping document format version: {0}")]
    UnsupportedVersion(u16),
}

pub(crate) fn decode_document(input: &str) -> Result<ModelMappingDocumentV1, DocumentCodecError> {
    let document: ModelMappingDocumentV1 = serde_json::from_str(input)?;
    if document.format_version != MODEL_MAPPING_FORMAT_VERSION {
        return Err(DocumentCodecError::UnsupportedVersion(
            document.format_version,
        ));
    }
    Ok(document)
}

pub(crate) fn canonical_document_json(
    document: &ModelMappingDocumentV1,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut canonical = document.clone();
    canonical
        .rules
        .sort_by(|left, right| left.id.cmp(&right.id));
    canonical
        .profiles
        .sort_by(|left, right| left.id.cmp(&right.id));
    canonical
        .bindings
        .sort_by(|left, right| left.id.cmp(&right.id));
    serde_json::to_vec(&canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::model_mapping::{
        Action, FallbackTrigger, Matcher, ModelBindingSource, ModelOfferingBinding, ModelProfile,
        ModelProfileStatus, TargetRef,
    };

    fn rule(id: &str, priority: u32, matcher: Matcher, action: Action) -> ModelMappingRule {
        ModelMappingRule {
            id: id.to_string(),
            priority,
            enabled: true,
            matcher,
            conditions: RuleConditions::default(),
            action,
            note: None,
            revision: 1,
        }
    }

    fn fixed(model: &str) -> Action {
        Action::MapFixed {
            target: TargetRef::Literal {
                upstream_model: model.to_string(),
            },
        }
    }

    fn document(rules: Vec<ModelMappingRule>) -> ModelMappingDocumentV1 {
        ModelMappingDocumentV1 {
            format_version: 1,
            base_revision: 42,
            policy: ModelMappingPolicy::default(),
            rules,
            ..Default::default()
        }
    }

    #[test]
    fn exact_rules_are_sorted_deterministically_and_trimmed() {
        let config = compile(&document(vec![
            rule(
                "b",
                10,
                Matcher::Exact {
                    model: " codex".into(),
                },
                fixed("qwen"),
            ),
            rule(
                "a",
                10,
                Matcher::Exact {
                    model: "codex".into(),
                },
                fixed("deepseek"),
            ),
        ]));
        let error = config.expect_err("same normalized exact rules must conflict");
        assert!(error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::RuleConflict));
    }

    #[test]
    fn same_priority_overlap_with_different_action_is_rejected() {
        let error = compile(&document(vec![
            rule(
                "a",
                10,
                Matcher::Exact {
                    model: "codex".into(),
                },
                fixed("one"),
            ),
            rule(
                "b",
                10,
                Matcher::Exact {
                    model: "codex".into(),
                },
                fixed("two"),
            ),
        ]))
        .expect_err("ambiguous rules");
        assert!(error.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::RuleConflict
                && diagnostic.rule_id.as_deref() == Some("a")
        }));
    }

    #[test]
    fn unconditional_default_must_be_lowest_priority() {
        let error = compile(&document(vec![
            rule("default", 20, Matcher::Default, Action::Preserve),
            rule(
                "exact",
                10,
                Matcher::Exact {
                    model: "codex".into(),
                },
                fixed("deepseek"),
            ),
        ]))
        .expect_err("default shadows an exact rule");
        assert!(error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidDefault));
    }

    #[test]
    fn higher_priority_default_shadows_lower_rule() {
        let error = compile(&document(vec![
            rule("default", 20, Matcher::Default, Action::Preserve),
            rule(
                "exact",
                10,
                Matcher::Exact {
                    model: "codex".into(),
                },
                fixed("deepseek"),
            ),
        ]))
        .expect_err("shadowed rule");
        assert!(error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::RuleShadowed));
    }

    #[test]
    fn profile_and_binding_json_are_strictly_decodable() {
        let profile_and_binding = r#"{
            "formatVersion":1,"baseRevision":0,"policy":{"unmatchedModelBehavior":"preserve"},
            "rules":[],
            "profiles":[{"id":"p","canonicalModel":"codex-5.4","displayName":"Codex 5.4","status":"active"}],
            "bindings":[{"id":"b","modelProfileId":"p","stationId":"station-1","upstreamModel":"native-model","source":"manual","enabled":true}]
        }"#;
        let decoded =
            decode_document(profile_and_binding).expect("profile metadata is phase-2 codec input");
        assert_eq!(decoded.profiles.len(), 1);
        assert_eq!(decoded.bindings.len(), 1);
        assert!(compile(&decoded).is_ok());
    }

    #[test]
    fn unknown_phase_variants_and_enum_values_are_rejected() {
        let unknown = r#"{
            "formatVersion":1,"baseRevision":0,"policy":{"unmatchedModelBehavior":"preserve"},
            "rules":[{"id":"r","priority":1,"enabled":true,"matcher":{"kind":"glob","pattern":"*"},"conditions":{},"action":{"kind":"preserve"}}],"profiles":[],"bindings":[]
        }"#;
        let glob_document = decode_document(unknown).expect("bounded glob is now supported");
        assert!(compile(&glob_document).is_ok());
        let unsupported_version = r#"{
            "formatVersion":2,"baseRevision":0,"policy":{"unmatchedModelBehavior":"preserve"},
            "rules":[],"profiles":[],"bindings":[]
        }"#;
        assert!(decode_document(unsupported_version).is_err());
        let unknown_status = r#"{
            "formatVersion":1,"baseRevision":0,"policy":{"unmatchedModelBehavior":"preserve"},
            "rules":[],"profiles":[{"id":"p","canonicalModel":"codex","displayName":"Codex","status":"active_now"}],"bindings":[]
        }"#;
        assert!(decode_document(unknown_status).is_err());
        let unknown_source = r#"{
            "formatVersion":1,"baseRevision":0,"policy":{"unmatchedModelBehavior":"preserve"},
            "rules":[],"profiles":[{"id":"p","canonicalModel":"codex","displayName":"Codex","status":"active"}],"bindings":[{"id":"b","modelProfileId":"p","stationId":"station-1","upstreamModel":"native","source":"scanner","enabled":true}]
        }"#;
        assert!(decode_document(unknown_source).is_err());
    }

    #[test]
    fn profile_binding_references_and_scopes_are_validated() {
        let mut missing_profile = document(Vec::new());
        missing_profile.bindings.push(ModelOfferingBinding {
            id: "binding".into(),
            model_profile_id: "missing".into(),
            station_key_id: Some("key".into()),
            station_id: None,
            upstream_model: "native".into(),
            source: ModelBindingSource::Manual,
            enabled: true,
            note: None,
            revision: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        });
        let error = compile(&missing_profile).expect_err("missing profile reference");
        assert!(error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidProfileReference));

        let mut both_scopes = missing_profile.clone();
        both_scopes.profiles.push(ModelProfile {
            id: "missing".into(),
            canonical_model: "codex".into(),
            display_name: "Codex".into(),
            default_upstream_model: None,
            status: ModelProfileStatus::Active,
            note: None,
            revision: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        });
        both_scopes.bindings[0].station_id = Some("station".into());
        let error = compile(&both_scopes).expect_err("binding scope xor");
        assert!(error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidCondition));
    }

    #[test]
    fn normalized_profile_canonical_models_must_be_unique() {
        let mut value = document(Vec::new());
        value.profiles = vec![
            ModelProfile {
                id: "a".into(),
                canonical_model: " codex ".into(),
                display_name: "A".into(),
                default_upstream_model: None,
                status: ModelProfileStatus::Active,
                note: None,
                revision: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            ModelProfile {
                id: "b".into(),
                canonical_model: "codex".into(),
                display_name: "B".into(),
                default_upstream_model: None,
                status: ModelProfileStatus::Active,
                note: None,
                revision: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        ];
        let error = compile(&value).expect_err("trimmed canonical names collide");
        assert!(error
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::RuleConflict));
    }

    #[test]
    fn profile_metadata_text_rejects_control_characters() {
        let mut value = document(Vec::new());
        value.profiles.push(ModelProfile {
            id: "profile".into(),
            canonical_model: "codex".into(),
            display_name: "Codex\n5.4".into(),
            default_upstream_model: None,
            status: ModelProfileStatus::Active,
            note: Some("safe\tlabel".into()),
            revision: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        });
        let error = compile(&value).expect_err("control characters are not display metadata");
        assert!(error.diagnostics.iter().any(|diagnostic| {
            diagnostic.path == "profiles[0].displayName"
                && diagnostic.code == DiagnosticCode::InvalidRule
        }));
        assert!(error.diagnostics.iter().any(|diagnostic| {
            diagnostic.path == "profiles[0].note" && diagnostic.code == DiagnosticCode::InvalidRule
        }));
    }

    #[test]
    fn fallback_chain_is_limited_to_the_frozen_three_target_bound() {
        let error = compile(&document(vec![rule(
            "fallback",
            1,
            Matcher::Exact {
                model: "codex".into(),
            },
            Action::MapFallbackChain {
                targets: vec![
                    TargetRef::Literal {
                        upstream_model: "one".into(),
                    },
                    TargetRef::Literal {
                        upstream_model: "two".into(),
                    },
                    TargetRef::Literal {
                        upstream_model: "three".into(),
                    },
                    TargetRef::Literal {
                        upstream_model: "four".into(),
                    },
                ],
                fallback_trigger: FallbackTrigger::NoEligibleTarget,
            },
        )]))
        .expect_err("fallback chains over the frozen bound must be rejected");
        assert!(error.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::InvalidFallback
                && diagnostic.path == "rules[0].action.targets"
        }));
    }

    #[test]
    fn canonical_json_is_stable_independent_of_rule_input_order() {
        let left = document(vec![
            rule("b", 1, Matcher::Default, Action::Preserve),
            rule("a", 2, Matcher::Exact { model: "x".into() }, fixed("y")),
        ]);
        let right = document(vec![
            rule("a", 2, Matcher::Exact { model: "x".into() }, fixed("y")),
            rule("b", 1, Matcher::Default, Action::Preserve),
        ]);
        assert_eq!(
            canonical_document_json(&left).unwrap(),
            canonical_document_json(&right).unwrap()
        );
    }

    #[test]
    fn canonical_json_is_stable_for_profile_and_binding_input_order() {
        let mut left = document(Vec::new());
        left.profiles = vec![
            ModelProfile {
                id: "profile-b".into(),
                canonical_model: "model-b".into(),
                display_name: "B".into(),
                default_upstream_model: None,
                status: ModelProfileStatus::Active,
                note: None,
                revision: 1,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            ModelProfile {
                id: "profile-a".into(),
                canonical_model: "model-a".into(),
                display_name: "A".into(),
                default_upstream_model: None,
                status: ModelProfileStatus::Active,
                note: None,
                revision: 1,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        ];
        left.bindings = vec![
            ModelOfferingBinding {
                id: "binding-b".into(),
                model_profile_id: "profile-b".into(),
                station_key_id: None,
                station_id: Some("station-b".into()),
                upstream_model: "native-b".into(),
                source: ModelBindingSource::Manual,
                enabled: true,
                note: None,
                revision: 1,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            ModelOfferingBinding {
                id: "binding-a".into(),
                model_profile_id: "profile-a".into(),
                station_key_id: None,
                station_id: Some("station-a".into()),
                upstream_model: "native-a".into(),
                source: ModelBindingSource::Manual,
                enabled: true,
                note: None,
                revision: 1,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        ];
        let mut right = left.clone();
        right.profiles.reverse();
        right.bindings.reverse();
        assert_eq!(
            canonical_document_json(&left).unwrap(),
            canonical_document_json(&right).unwrap()
        );
    }
}
