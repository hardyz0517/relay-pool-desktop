use serde_json::Value;

use crate::{
    application::command_facades::RoutingCommandFacade,
    application::model_mapping,
    commands::error,
    ipc::dto::{
        model_mapping::{
            ApplyModelMappingDocumentInputDto, ModelMappingActionDto, ModelMappingBindingDto,
            ModelMappingBindingSourceDto, ModelMappingConditionsDto, ModelMappingDiagnosticDto,
            ModelMappingDiagnosticSeverityDto, ModelMappingDispositionDto, ModelMappingDocumentDto,
            ModelMappingDocumentSourceDto, ModelMappingEndpointKindDto, ModelMappingMatcherDto,
            ModelMappingPolicyDto, ModelMappingProfileDto, ModelMappingProfileStatusDto,
            ModelMappingRuleDto, ModelMappingSimulationResultDto, ModelMappingTargetRefDto,
            ModelMappingTraceStatusDto, ModelMappingValidationResultDto, ModelMappingWorkspaceDto,
            ResolveRequestMappingTraceInputDto, RestoreModelMappingRevisionInputDto,
            SimulateModelMappingInputDto, ValidateModelMappingDocumentInputDto,
        },
        EmptyInputDto,
    },
    models::model_mapping::{
        Action, ConditionRequirement, EndpointKind, Matcher, ModelBindingSource,
        ModelMappingDocumentV1, ModelMappingPolicy, ModelMappingRule, ModelOfferingBinding,
        ModelProfile, ModelProfileStatus, RejectionKind, RuleConditions, TargetRef,
        UnmatchedModelBehavior,
    },
    observability::correlation,
};

// Keep IPC responses bounded even when historical review data or compiler
// diagnostics grow independently of the document limits.
const MAX_WORKSPACE_KNOWN_MODEL_OPTIONS: usize = 512;
const MAX_WORKSPACE_LEGACY_REVIEWS: usize = 256;
const MAX_MAPPING_DIAGNOSTICS: usize = 512;

fn invalid(message: &'static str) -> error::CommandError {
    error::CommandError::try_new(
        error::CommandErrorCode::InvalidInput,
        message,
        false,
        None,
        None,
    )
    .expect("bounded mapping validation error")
}

fn condition(
    value: crate::ipc::dto::model_mapping::ModelMappingConditionModeDto,
) -> ConditionRequirement {
    match value {
        crate::ipc::dto::model_mapping::ModelMappingConditionModeDto::Any => {
            ConditionRequirement::Any
        }
        crate::ipc::dto::model_mapping::ModelMappingConditionModeDto::Required => {
            ConditionRequirement::Required
        }
        crate::ipc::dto::model_mapping::ModelMappingConditionModeDto::Forbidden => {
            ConditionRequirement::Forbidden
        }
    }
}

fn endpoint(value: ModelMappingEndpointKindDto) -> EndpointKind {
    match value {
        ModelMappingEndpointKindDto::ChatCompletions => EndpointKind::ChatCompletions,
        ModelMappingEndpointKindDto::Responses => EndpointKind::Responses,
        ModelMappingEndpointKindDto::Embeddings => EndpointKind::Embeddings,
    }
}

fn to_domain(
    input: &ModelMappingDocumentDto,
) -> Result<ModelMappingDocumentV1, error::CommandError> {
    let rules = input.rules.iter().map(|rule| {
        let matcher = match &rule.matcher {
            ModelMappingMatcherDto::Exact { model } => Matcher::Exact { model: model.clone() },
            ModelMappingMatcherDto::Glob { pattern } => Matcher::Glob { pattern: pattern.clone() },
            ModelMappingMatcherDto::Default => Matcher::Default,
        };
        let action = match &rule.action {
            ModelMappingActionDto::MapFixed { target } => match target {
                ModelMappingTargetRefDto::Literal { upstream_model } => Action::MapFixed { target: TargetRef::Literal { upstream_model: upstream_model.clone() } },
                ModelMappingTargetRefDto::ModelProfile { model_profile_id } => Action::MapFixed { target: TargetRef::ModelProfile { model_profile_id: model_profile_id.clone() } },
            },
            ModelMappingActionDto::Preserve => Action::Preserve,
            ModelMappingActionDto::Reject { rejection_kind, message } => Action::Reject {
                rejection_kind: match rejection_kind {
                    crate::ipc::dto::model_mapping::ModelMappingRejectionKindDto::UnsupportedModel => RejectionKind::UnsupportedModel,
                    crate::ipc::dto::model_mapping::ModelMappingRejectionKindDto::Policy => RejectionKind::PolicyDenied,
                    crate::ipc::dto::model_mapping::ModelMappingRejectionKindDto::InvalidRequest => RejectionKind::ClientNotAllowed,
                },
                message: message.clone(),
            },
            ModelMappingActionDto::MapFallbackChain { targets, fallback_trigger } => Action::MapFallbackChain {
                targets: targets.iter().map(|target| match target {
                    ModelMappingTargetRefDto::Literal { upstream_model } => TargetRef::Literal { upstream_model: upstream_model.clone() },
                    ModelMappingTargetRefDto::ModelProfile { model_profile_id } => TargetRef::ModelProfile { model_profile_id: model_profile_id.clone() },
                }).collect(),
                fallback_trigger: match fallback_trigger {
                    crate::ipc::dto::model_mapping::ModelMappingFallbackTriggerDto::NoEligibleTarget => crate::models::model_mapping::FallbackTrigger::NoEligibleTarget,
                    crate::ipc::dto::model_mapping::ModelMappingFallbackTriggerDto::RetryExhaustedBeforeOutput => crate::models::model_mapping::FallbackTrigger::RetryExhaustedBeforeOutput,
                },
            },
        };
        Ok::<ModelMappingRule, error::CommandError>(ModelMappingRule {
            id: rule.id.clone(), priority: rule.priority, enabled: rule.enabled,
            matcher, conditions: RuleConditions {
                endpoint_kinds: if rule.conditions.endpoint_kinds.is_empty() { None } else { Some(rule.conditions.endpoint_kinds.iter().copied().map(endpoint).collect()) },
                stream: condition(rule.conditions.stream), tools: condition(rule.conditions.tools),
                vision: condition(rule.conditions.vision), reasoning: condition(rule.conditions.reasoning),
            },
            action, note: rule.note.clone(), revision: rule.revision,
        })
    }).collect::<Result<Vec<_>, _>>()?;
    Ok(ModelMappingDocumentV1 {
        format_version: input.format_version,
        base_revision: input.base_revision,
        policy: ModelMappingPolicy {
            unmatched_model_behavior: match input.policy.unmatched_model_behavior {
                crate::ipc::dto::model_mapping::UnmatchedModelBehaviorDto::Preserve => {
                    UnmatchedModelBehavior::Preserve
                }
                crate::ipc::dto::model_mapping::UnmatchedModelBehaviorDto::Reject => {
                    UnmatchedModelBehavior::Reject
                }
            },
        },
        rules,
        profiles: input
            .profiles
            .iter()
            .map(|profile| ModelProfile {
                id: profile.id.clone(),
                canonical_model: profile.canonical_model.clone(),
                display_name: profile
                    .display_name
                    .clone()
                    .unwrap_or_else(|| profile.canonical_model.clone()),
                default_upstream_model: profile.default_upstream_model.clone(),
                status: match profile.status {
                    ModelMappingProfileStatusDto::Active => ModelProfileStatus::Active,
                    ModelMappingProfileStatusDto::Archived => ModelProfileStatus::Archived,
                },
                note: profile.note.clone(),
                revision: profile.revision,
                created_at_ms: profile.created_at_ms,
                updated_at_ms: profile.updated_at_ms,
            })
            .collect(),
        bindings: input
            .bindings
            .iter()
            .map(|binding| ModelOfferingBinding {
                id: binding.id.clone(),
                model_profile_id: binding.model_profile_id.clone(),
                station_key_id: binding.station_key_id.clone(),
                station_id: binding.station_id.clone(),
                upstream_model: binding.upstream_model.clone(),
                source: match binding.source {
                    ModelMappingBindingSourceDto::User => ModelBindingSource::Manual,
                    ModelMappingBindingSourceDto::Discovered => ModelBindingSource::Discovered,
                    ModelMappingBindingSourceDto::Migration => ModelBindingSource::Migrated,
                },
                enabled: binding.enabled,
                note: binding.note.clone(),
                revision: binding.revision,
                created_at_ms: binding.created_at_ms,
                updated_at_ms: binding.updated_at_ms,
            })
            .collect(),
    })
}

fn mode(
    value: ConditionRequirement,
) -> crate::ipc::dto::model_mapping::ModelMappingConditionModeDto {
    match value {
        ConditionRequirement::Any => {
            crate::ipc::dto::model_mapping::ModelMappingConditionModeDto::Any
        }
        ConditionRequirement::Required => {
            crate::ipc::dto::model_mapping::ModelMappingConditionModeDto::Required
        }
        ConditionRequirement::Forbidden => {
            crate::ipc::dto::model_mapping::ModelMappingConditionModeDto::Forbidden
        }
    }
}

fn from_domain(document: &ModelMappingDocumentV1) -> ModelMappingDocumentDto {
    ModelMappingDocumentDto {
        format_version: document.format_version, base_revision: document.base_revision,
        policy: ModelMappingPolicyDto { unmatched_model_behavior: match document.policy.unmatched_model_behavior {
            UnmatchedModelBehavior::Preserve => crate::ipc::dto::model_mapping::UnmatchedModelBehaviorDto::Preserve,
            UnmatchedModelBehavior::Reject => crate::ipc::dto::model_mapping::UnmatchedModelBehaviorDto::Reject,
        } },
        rules: document.rules.iter().map(|rule| ModelMappingRuleDto {
            id: rule.id.clone(), priority: rule.priority, enabled: rule.enabled,
            matcher: match &rule.matcher { Matcher::Exact { model } => ModelMappingMatcherDto::Exact { model: model.clone() }, Matcher::Glob { pattern } => ModelMappingMatcherDto::Glob { pattern: pattern.clone() }, Matcher::Default => ModelMappingMatcherDto::Default },
            conditions: ModelMappingConditionsDto {
                endpoint_kinds: rule.conditions.endpoint_kinds.clone().unwrap_or_default().into_iter().filter_map(|item| match item { EndpointKind::ChatCompletions => Some(ModelMappingEndpointKindDto::ChatCompletions), EndpointKind::Responses => Some(ModelMappingEndpointKindDto::Responses), EndpointKind::Embeddings => Some(ModelMappingEndpointKindDto::Embeddings), _ => None }).collect(),
                stream: mode(rule.conditions.stream), tools: mode(rule.conditions.tools), vision: mode(rule.conditions.vision), reasoning: mode(rule.conditions.reasoning),
            },
            action: match &rule.action {
                Action::MapFixed { target } => ModelMappingActionDto::MapFixed { target: match target {
                    TargetRef::Literal { upstream_model } => ModelMappingTargetRefDto::Literal { upstream_model: upstream_model.clone() },
                    TargetRef::ModelProfile { model_profile_id } => ModelMappingTargetRefDto::ModelProfile { model_profile_id: model_profile_id.clone() },
                } },
                Action::MapFallbackChain { targets, fallback_trigger } => ModelMappingActionDto::MapFallbackChain {
                    targets: targets.iter().map(|target| match target {
                        TargetRef::Literal { upstream_model } => ModelMappingTargetRefDto::Literal { upstream_model: upstream_model.clone() },
                        TargetRef::ModelProfile { model_profile_id } => ModelMappingTargetRefDto::ModelProfile { model_profile_id: model_profile_id.clone() },
                    }).collect(),
                    fallback_trigger: match fallback_trigger {
                        crate::models::model_mapping::FallbackTrigger::NoEligibleTarget => crate::ipc::dto::model_mapping::ModelMappingFallbackTriggerDto::NoEligibleTarget,
                        crate::models::model_mapping::FallbackTrigger::RetryExhaustedBeforeOutput => crate::ipc::dto::model_mapping::ModelMappingFallbackTriggerDto::RetryExhaustedBeforeOutput,
                    },
                },
                Action::Preserve => ModelMappingActionDto::Preserve,
                Action::Reject { rejection_kind, message } => ModelMappingActionDto::Reject { rejection_kind: match rejection_kind { RejectionKind::UnsupportedModel => crate::ipc::dto::model_mapping::ModelMappingRejectionKindDto::UnsupportedModel, RejectionKind::PolicyDenied => crate::ipc::dto::model_mapping::ModelMappingRejectionKindDto::Policy, RejectionKind::ClientNotAllowed => crate::ipc::dto::model_mapping::ModelMappingRejectionKindDto::InvalidRequest }, message: message.clone() },
            },
            note: rule.note.clone(), revision: rule.revision, created_at_ms: 0, updated_at_ms: 0,
        }).collect(),
        profiles: document
            .profiles
            .iter()
            .map(|profile| ModelMappingProfileDto {
                id: profile.id.clone(),
                canonical_model: profile.canonical_model.clone(),
                display_name: Some(profile.display_name.clone()),
                default_upstream_model: profile.default_upstream_model.clone(),
                status: match profile.status {
                    ModelProfileStatus::Active => ModelMappingProfileStatusDto::Active,
                    ModelProfileStatus::Archived => ModelMappingProfileStatusDto::Archived,
                },
                note: profile.note.clone(),
                revision: profile.revision,
                created_at_ms: profile.created_at_ms,
                updated_at_ms: profile.updated_at_ms,
            })
            .collect(),
        bindings: document
            .bindings
            .iter()
            .map(|binding| ModelMappingBindingDto {
                id: binding.id.clone(),
                model_profile_id: binding.model_profile_id.clone(),
                station_id: binding.station_id.clone(),
                station_key_id: binding.station_key_id.clone(),
                upstream_model: binding.upstream_model.clone(),
                source: match binding.source {
                    ModelBindingSource::Manual => ModelMappingBindingSourceDto::User,
                    ModelBindingSource::Discovered => ModelMappingBindingSourceDto::Discovered,
                    ModelBindingSource::Migrated => ModelMappingBindingSourceDto::Migration,
                },
                enabled: binding.enabled,
                note: binding.note.clone(),
                revision: binding.revision,
                created_at_ms: binding.created_at_ms,
                updated_at_ms: binding.updated_at_ms,
            })
            .collect(),
    }
}

fn compile_diagnostics(error: &model_mapping::CompileError) -> Vec<ModelMappingDiagnosticDto> {
    error
        .diagnostics
        .iter()
        .take(MAX_MAPPING_DIAGNOSTICS)
        .map(|item| ModelMappingDiagnosticDto {
            severity: ModelMappingDiagnosticSeverityDto::Error,
            code: item.code.as_str().to_string(),
            path: item.path.clone(),
            message: item.message.clone(),
            rule_id: item.rule_id.clone(),
            target_index: None,
        })
        .collect()
}

fn workspace(
    document: ModelMappingDocumentV1,
    diagnostics: Vec<ModelMappingDiagnosticDto>,
    source: ModelMappingDocumentSourceDto,
    sync: Option<&crate::persistence::stores::document_sync_store::StoredDocumentSync>,
    file_present_override: Option<bool>,
) -> ModelMappingWorkspaceDto {
    let revision = document.base_revision;
    let (sync_state, file_present, last_error_code) = sync
        .map(|status| {
            (
                match status.state {
                    crate::models::document_sync::DocumentSyncState::Synchronized => {
                        crate::ipc::dto::model_mapping::ModelMappingSyncStateDto::Synchronized
                    }
                    crate::models::document_sync::DocumentSyncState::PendingMaterialization => {
                        crate::ipc::dto::model_mapping::ModelMappingSyncStateDto::PendingMaterialization
                    }
                    crate::models::document_sync::DocumentSyncState::ExternalChange => {
                        crate::ipc::dto::model_mapping::ModelMappingSyncStateDto::ExternalChange
                    }
                    crate::models::document_sync::DocumentSyncState::Error => {
                        crate::ipc::dto::model_mapping::ModelMappingSyncStateDto::Error
                    }
                },
                file_present_override.unwrap_or_else(|| status.materialized_revision.is_some()),
                status.last_error_code.clone(),
            )
        })
        .unwrap_or((
            crate::ipc::dto::model_mapping::ModelMappingSyncStateDto::PendingMaterialization,
            false,
            None,
        ));
    ModelMappingWorkspaceDto {
        document: from_domain(&document),
        status: crate::ipc::dto::model_mapping::ModelMappingDocumentStatusDto {
            active_revision: revision,
            sync_state,
            source,
            file_present,
            last_error_code,
        },
        known_model_options: document
            .rules
            .iter()
            .filter_map(|rule| match &rule.matcher {
                Matcher::Exact { model } => Some(model.clone()),
                Matcher::Glob { .. } => None,
                Matcher::Default => None,
            })
            .chain(
                document
                    .profiles
                    .iter()
                    .map(|profile| profile.canonical_model.clone()),
            )
            .take(MAX_WORKSPACE_KNOWN_MODEL_OPTIONS)
            .collect(),
        legacy_reviews: Vec::new(),
        diagnostics,
        candidate_count: 0,
    }
}

async fn with_context<'a, T, F>(
    name: &'static str,
    registry: &'a crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    runtime_context: Option<Value>,
    action: F,
) -> Result<T, error::CommandError>
where
    F: std::future::Future<Output = Result<T, error::CommandError>> + 'a,
{
    correlation::in_command_scope_with_runtime_context(name, registry, runtime_context, action)
        .await
}

#[tauri::command]
pub async fn get_model_mapping_workspace(
    input: Value,
    facade: tauri::State<'_, RoutingCommandFacade>,
    registry: tauri::State<'_, crate::ipc::dto::runtime_context::RuntimeContextRegistry>,
    runtime_context: Option<Value>,
) -> Result<ModelMappingWorkspaceDto, error::CommandError> {
    let facade = facade.inner().clone();
    with_context(
        "get_model_mapping_workspace",
        registry.inner(),
        runtime_context,
        async move {
            EmptyInputDto::parse(input)?;
            let sync = facade
                .reconcile_model_mapping_document_sync()
                .await
                .map_err(super::public_command_application_error)?;
            let reviews = facade
                .list_model_mapping_legacy_reviews()
                .await
                .map_err(super::public_command_application_error)?
                .into_iter()
                .filter_map(|review| {
                    Some(crate::ipc::dto::model_mapping::ModelMappingLegacyReviewDto {
                        id: review.id,
                        legacy_alias_id: review.legacy_alias_id.unwrap_or_default(),
                        requested_model: review.requested_model.unwrap_or_default(),
                        selected_target: review.selected_target.unwrap_or_default(),
                        discarded_target: review.discarded_target,
                        status: match review.migration_status.as_str() {
                            "pending" => crate::ipc::dto::model_mapping::ModelMappingLegacyReviewStatusDto::Pending,
                            "accepted" => crate::ipc::dto::model_mapping::ModelMappingLegacyReviewStatusDto::Accepted,
                            "discarded" => crate::ipc::dto::model_mapping::ModelMappingLegacyReviewStatusDto::Discarded,
                            _ => return None,
                        },
                        created_at_ms: review.created_at_ms,
                    })
                })
                .take(MAX_WORKSPACE_LEGACY_REVIEWS)
                .collect::<Vec<_>>();
            let mut result = workspace(
                model_mapping::current_document(),
                Vec::new(),
                ModelMappingDocumentSourceDto::Ui,
                sync.sync.as_ref(),
                Some(sync.file_present),
            );
            result.legacy_reviews = reviews;
            Ok(result)
        },
    )
    .await
}

#[tauri::command]
pub async fn get_model_mapping_document(
    input: Value,
    registry: tauri::State<'_, crate::ipc::dto::runtime_context::RuntimeContextRegistry>,
    runtime_context: Option<Value>,
) -> Result<ModelMappingDocumentDto, error::CommandError> {
    with_context(
        "get_model_mapping_document",
        registry.inner(),
        runtime_context,
        async move {
            EmptyInputDto::parse(input)?;
            Ok(from_domain(&model_mapping::current_document()))
        },
    )
    .await
}

#[tauri::command]
pub async fn validate_model_mapping_document(
    input: Value,
    registry: tauri::State<'_, crate::ipc::dto::runtime_context::RuntimeContextRegistry>,
    runtime_context: Option<Value>,
) -> Result<ModelMappingValidationResultDto, error::CommandError> {
    with_context(
        "validate_model_mapping_document",
        registry.inner(),
        runtime_context,
        async move {
            let input = ValidateModelMappingDocumentInputDto::parse(input)?;
            let document = to_domain(&input.document)?;
            match model_mapping::compile_at_revision(&document, document.base_revision) {
                Ok(_) => Ok(ModelMappingValidationResultDto {
                    valid: true,
                    diagnostics: Vec::new(),
                    normalized_document: Some(from_domain(&document)),
                }),
                Err(error) => Ok(ModelMappingValidationResultDto {
                    valid: false,
                    diagnostics: compile_diagnostics(&error),
                    normalized_document: None,
                }),
            }
        },
    )
    .await
}

#[tauri::command]
pub async fn apply_model_mapping_document(
    input: Value,
    facade: tauri::State<'_, RoutingCommandFacade>,
    registry: tauri::State<'_, crate::ipc::dto::runtime_context::RuntimeContextRegistry>,
    runtime_context: Option<Value>,
) -> Result<ModelMappingWorkspaceDto, error::CommandError> {
    let facade = facade.inner().clone();
    with_context(
        "apply_model_mapping_document",
        registry.inner(),
        runtime_context,
        async move {
            let input = ApplyModelMappingDocumentInputDto::parse(input)?;
            let document = to_domain(&input.document)?;
            // The public IPC payload is untrusted. Source provenance is
            // attached by the command owner, so a caller cannot claim a UI
            // edit is a migration/restore or bypass the file-sync audit path.
            let source = crate::models::document_sync::TrustedDocumentSource::ui();
            if let Err(compile_error) =
                model_mapping::compile_at_revision(&document, document.base_revision)
            {
                return Ok(workspace(
                    model_mapping::current_document(),
                    compile_diagnostics(&compile_error),
                    ModelMappingDocumentSourceDto::Ui,
                    None,
                    None,
                ));
            }
            let document = facade
                .apply_model_mapping_document(document, source)
                .await
                .map_err(super::public_command_application_error)?;
            let sync = facade
                .reconcile_model_mapping_document_sync()
                .await
                .map_err(super::public_command_application_error)?;
            Ok(workspace(
                document,
                Vec::new(),
                ModelMappingDocumentSourceDto::Ui,
                sync.sync.as_ref(),
                Some(sync.file_present),
            ))
        },
    )
    .await
}

#[tauri::command]
pub async fn restore_model_mapping_revision(
    input: Value,
    facade: tauri::State<'_, RoutingCommandFacade>,
    registry: tauri::State<'_, crate::ipc::dto::runtime_context::RuntimeContextRegistry>,
    runtime_context: Option<Value>,
) -> Result<ModelMappingWorkspaceDto, error::CommandError> {
    let facade = facade.inner().clone();
    with_context(
        "restore_model_mapping_revision",
        registry.inner(),
        runtime_context,
        async move {
            let input = RestoreModelMappingRevisionInputDto::parse(input)?;
            let document_json = facade
                .load_model_mapping_history_document(input.revision)
                .await
                .map_err(super::public_command_application_error)?
                .ok_or_else(|| {
                    super::public_command_application_error(
                        crate::application::error::ApplicationError::NotFound,
                    )
                })?;
            let document = model_mapping::decode_document(&document_json)
                .map_err(|_| invalid("The historical mapping document is invalid."))?;
            if document.base_revision != input.revision {
                return Err(invalid("The historical mapping document is invalid."));
            }
            model_mapping::compile_at_revision(&document, document.base_revision)
                .map_err(|_| invalid("The historical mapping document is invalid."))?;
            let document = facade
                .restore_model_mapping_document(document, input.expected_revision)
                .await
                .map_err(super::public_command_application_error)?;
            let sync = facade
                .reconcile_model_mapping_document_sync()
                .await
                .map_err(super::public_command_application_error)?;
            Ok(workspace(
                document,
                Vec::new(),
                ModelMappingDocumentSourceDto::Restore,
                sync.sync.as_ref(),
                Some(sync.file_present),
            ))
        },
    )
    .await
}

#[tauri::command]
pub async fn simulate_model_mapping(
    input: Value,
    registry: tauri::State<'_, crate::ipc::dto::runtime_context::RuntimeContextRegistry>,
    runtime_context: Option<Value>,
) -> Result<ModelMappingSimulationResultDto, error::CommandError> {
    with_context(
        "simulate_model_mapping",
        registry.inner(),
        runtime_context,
        async move {
            let input: SimulateModelMappingInputDto = serde_json::from_value(input)
                .map_err(|_| invalid("The simulation payload is invalid."))?;
            let document = input
                .draft
                .as_ref()
                .map(to_domain)
                .transpose()?
                .unwrap_or_else(model_mapping::current_document);
            let config = model_mapping::compile_at_revision(&document, document.base_revision)
                .map_err(|_| invalid("The mapping draft is invalid."))?;
            let facts = crate::models::model_mapping::ModelRequestFacts::inference(
                input.model.clone(),
                endpoint(input.endpoint),
                input.stream,
                input.uses_tools,
                input.uses_vision,
                input.uses_reasoning,
            );
            let plan = model_mapping::resolve(&config, &facts)
                .map_err(|_| invalid("The requested model is invalid."))?;
            let mapped = plan
                .execution_target()
                .map_err(|error| invalid(error.code()))?
                .map(|target| target.route_model.clone());
            Ok(ModelMappingSimulationResultDto {
                requested_model: input.model,
                route_model: mapped.clone(),
                upstream_model: mapped,
                disposition: match plan.disposition {
                    model_mapping::Disposition::Mapped => ModelMappingDispositionDto::Mapped,
                    model_mapping::Disposition::Reject => ModelMappingDispositionDto::Reject,
                    _ => ModelMappingDispositionDto::Preserve,
                },
                matched_rule_id: plan.matched_rule_id,
                diagnostics: Vec::new(),
            })
        },
    )
    .await
}

#[tauri::command]
pub async fn resolve_request_mapping_trace(
    input: Value,
    registry: tauri::State<'_, crate::ipc::dto::runtime_context::RuntimeContextRegistry>,
    runtime_context: Option<Value>,
) -> Result<crate::ipc::dto::model_mapping::ModelMappingTraceDto, error::CommandError> {
    with_context(
        "resolve_request_mapping_trace",
        registry.inner(),
        runtime_context,
        async move {
            let input = ResolveRequestMappingTraceInputDto::parse(input)?;
            let Some(trace) = model_mapping::request_trace(&input.request_log_id) else {
                return Ok(crate::ipc::dto::model_mapping::ModelMappingTraceDto {
                    request_log_id: input.request_log_id,
                    status: ModelMappingTraceStatusDto::Unavailable,
                    requested_model: None,
                    route_model: None,
                    upstream_model: None,
                    mapping_revision: None,
                    resolution_fence: None,
                    matched_rule_id: None,
                    target_rank: None,
                    disposition: None,
                    failure_code: Some("mapping_trace_unavailable".to_string()),
                });
            };
            Ok(crate::ipc::dto::model_mapping::ModelMappingTraceDto {
                request_log_id: trace.request_log_id,
                status: ModelMappingTraceStatusDto::Available,
                requested_model: trace.requested_model,
                route_model: trace.route_model,
                upstream_model: trace.upstream_model,
                mapping_revision: Some(trace.mapping_revision),
                resolution_fence: Some(trace.resolution_fence),
                matched_rule_id: trace.matched_rule_id,
                target_rank: trace.target_rank,
                disposition: Some(match trace.disposition {
                    model_mapping::Disposition::Preserve => ModelMappingDispositionDto::Preserve,
                    model_mapping::Disposition::Mapped => ModelMappingDispositionDto::Mapped,
                    model_mapping::Disposition::Reject => ModelMappingDispositionDto::Reject,
                    model_mapping::Disposition::Bypass => ModelMappingDispositionDto::Preserve,
                }),
                failure_code: trace.failure_code,
            })
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::from_domain;
    use crate::models::model_mapping::{
        ModelBindingSource, ModelMappingDocumentV1, ModelOfferingBinding, ModelProfile,
        ModelProfileStatus,
    };

    #[test]
    fn read_model_preserves_profile_and_binding_metadata() {
        let mut document = ModelMappingDocumentV1::default();
        document.profiles.push(ModelProfile {
            id: "profile-1".to_string(),
            canonical_model: "logical-codex".to_string(),
            display_name: "Logical Codex".to_string(),
            default_upstream_model: Some("native-codex".to_string()),
            status: ModelProfileStatus::Active,
            note: Some("metadata only until variant-aware routing".to_string()),
            revision: 2,
            created_at_ms: 10,
            updated_at_ms: 20,
        });
        document.bindings.push(ModelOfferingBinding {
            id: "binding-1".to_string(),
            model_profile_id: "profile-1".to_string(),
            station_key_id: Some("key-1".to_string()),
            station_id: None,
            upstream_model: "native-codex-key".to_string(),
            source: ModelBindingSource::Migrated,
            enabled: true,
            note: None,
            revision: 2,
            created_at_ms: 10,
            updated_at_ms: 20,
        });

        let dto = from_domain(&document);
        assert_eq!(dto.profiles.len(), 1);
        assert_eq!(dto.profiles[0].canonical_model, "logical-codex");
        assert_eq!(
            dto.profiles[0].display_name.as_deref(),
            Some("Logical Codex")
        );
        assert_eq!(dto.bindings.len(), 1);
        assert_eq!(dto.bindings[0].model_profile_id, "profile-1");
        assert_eq!(dto.bindings[0].station_key_id.as_deref(), Some("key-1"));
    }
}
