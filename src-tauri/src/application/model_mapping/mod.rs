//! Pure model mapping compiler and resolver.

mod compiler;
mod glob;
mod resolver;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use sha2::{Digest, Sha256};

use crate::models::document_sync::TrustedDocumentSource;
use crate::models::model_mapping::{
    normalize_model_name, Action, ConditionRequirement, EndpointKind, Matcher,
    ModelMappingDocumentV1, ModelMappingPolicy, ModelMappingRule, ModelRequestFacts, RejectionKind,
    RuleConditions, TargetRef,
};
use crate::services::policy_documents::{
    decode_strict_json, PolicyDocumentCoordinator, PolicyDocumentError,
};

pub(crate) use compiler::{
    canonical_document_json, compile_at_revision, decode_document, CompileError,
    CompiledModelMappingConfiguration,
};
pub(crate) use resolver::{
    candidate_variants, resolve, resolve_for_candidate, CandidateModelVariant,
    CandidateResolutionContext, DecisionEvidence, Disposition, ModelMappingResolutionError,
    ResolvedModelPlan, TargetPolicy,
};
#[cfg(test)]
pub(crate) use resolver::{ResolutionReason, ResolvedTarget};

const REQUEST_MAPPING_TRACE_CAPACITY: usize = 512;

/// Runtime-only mapping evidence. It is deliberately bounded and never
/// reconstructs a historical decision from the mutable active document.
/// Durable request traces can adopt these fields later without changing the
/// resolver contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestMappingTrace {
    pub(crate) request_log_id: String,
    pub(crate) requested_model: Option<String>,
    pub(crate) route_model: Option<String>,
    pub(crate) upstream_model: Option<String>,
    pub(crate) mapping_revision: u64,
    pub(crate) resolution_fence: String,
    pub(crate) matched_rule_id: Option<String>,
    pub(crate) target_rank: Option<u16>,
    pub(crate) disposition: Disposition,
    pub(crate) failure_code: Option<String>,
}

static REQUEST_MAPPING_TRACES: OnceLock<Mutex<VecDeque<RequestMappingTrace>>> = OnceLock::new();

fn request_mapping_traces() -> &'static Mutex<VecDeque<RequestMappingTrace>> {
    REQUEST_MAPPING_TRACES
        .get_or_init(|| Mutex::new(VecDeque::with_capacity(REQUEST_MAPPING_TRACE_CAPACITY)))
}

pub(crate) fn record_request_trace(request_log_id: &str, plan: &ResolvedModelPlan) {
    if request_log_id.is_empty() {
        return;
    }
    // A fallback plan intentionally contains multiple targets.  The trace is
    // recorded at resolution time, so retain the first (rank-zero) target as
    // the preferred route instead of using the single-target accessor, which
    // would reject a valid fallback plan and erase its model identity.
    let execution_target = plan.target_models.first();
    let trace = RequestMappingTrace {
        request_log_id: request_log_id.to_string(),
        requested_model: plan.requested_model.clone(),
        route_model: execution_target.map(|target| target.route_model.clone()),
        upstream_model: execution_target.map(|target| target.route_model.clone()),
        mapping_revision: plan.mapping_revision,
        resolution_fence: plan.model_resolution_fence.clone(),
        matched_rule_id: plan.matched_rule_id.clone(),
        target_rank: execution_target.map(|target| target.target_rank),
        disposition: plan.disposition,
        failure_code: if plan.disposition == Disposition::Reject {
            Some("model_mapping_rejected".to_string())
        } else {
            None
        },
    };
    let mut traces = request_mapping_traces()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(existing) = traces
        .iter_mut()
        .find(|existing| existing.request_log_id == request_log_id)
    {
        *existing = trace;
        return;
    }
    if traces.len() >= REQUEST_MAPPING_TRACE_CAPACITY {
        traces.pop_front();
    }
    traces.push_back(trace);
}

pub(crate) fn request_trace(request_log_id: &str) -> Option<RequestMappingTrace> {
    request_mapping_traces()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find(|trace| trace.request_log_id == request_log_id)
        .cloned()
}

struct RuntimeMappingState {
    document: ModelMappingDocumentV1,
    compiled: CompiledModelMappingConfiguration,
}

/// Immutable mapping inputs captured for one evaluation. Callers should pass
/// this value through planning instead of reading the mutable global state at
/// multiple points in a request lifecycle.
#[derive(Debug, Clone)]
pub(crate) struct ModelMappingSnapshot {
    pub(crate) configuration: CompiledModelMappingConfiguration,
    pub(crate) revision: u64,
}

static RUNTIME_STATE: OnceLock<RwLock<RuntimeMappingState>> = OnceLock::new();

// Model mapping is intentionally process-wide in production.  Test harnesses
// use isolated databases, so they must serialize the persisted document and
// the process-wide compiled snapshot as one lifetime-scoped critical section.
#[cfg(any(test, debug_assertions))]
static MODEL_MAPPING_TEST_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();

#[cfg(any(test, debug_assertions))]
pub(crate) async fn acquire_model_mapping_test_guard() -> tokio::sync::OwnedMutexGuard<()> {
    MODEL_MAPPING_TEST_LOCK
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
        .lock_owned()
        .await
}

fn runtime_state() -> &'static RwLock<RuntimeMappingState> {
    RUNTIME_STATE.get_or_init(|| {
        let document = ModelMappingDocumentV1::default();
        let compiled = compile_at_revision(&document, 1).expect("default mapping is valid");
        RwLock::new(RuntimeMappingState { document, compiled })
    })
}

fn install_document(document: ModelMappingDocumentV1, revision: u64) -> Result<(), String> {
    let compiled = compile_at_revision(&document, revision).map_err(|error| error.to_string())?;
    let state = runtime_state();
    let mut guard = state
        .write()
        .map_err(|_| "mapping runtime lock poisoned".to_string())?;
    guard.document = document;
    guard.compiled = compiled;
    Ok(())
}

/// Loads the normalized mapping aggregate once during runtime composition. The
/// proxy only reads the compiled in-memory snapshot after this succeeds.
pub(crate) async fn initialize_from_persistence(
    runtime: crate::persistence::runtime::PersistenceHandle,
) -> Result<(), String> {
    let mut read = runtime
        .begin_read()
        .await
        .map_err(|error| format!("begin model mapping read failed: {error}"))?;
    let store = crate::persistence::stores::model_mapping_store::ModelMappingStore;
    let policy = store
        .load_policy(read.connection())
        .await
        .map_err(|error| format!("load model mapping policy failed: {error}"))?;
    let stored_rules = store
        .list_rules(read.connection(), false)
        .await
        .map_err(|error| format!("load model mapping rules failed: {error}"))?;
    let stored_profiles = store
        .list_profiles(read.connection(), false)
        .await
        .map_err(|error| format!("load model mapping profiles failed: {error}"))?;
    let stored_bindings = store
        .list_bindings(read.connection(), None, false)
        .await
        .map_err(|error| format!("load model mapping bindings failed: {error}"))?;
    let mut rules = Vec::with_capacity(stored_rules.len());
    for stored in stored_rules {
        let matcher = match stored.matcher_kind.as_str() {
            "exact" => Matcher::Exact {
                model: stored
                    .matcher_value
                    .ok_or_else(|| "model mapping exact matcher has no value".to_string())?,
            },
            "glob" => Matcher::Glob {
                pattern: stored
                    .matcher_value
                    .ok_or_else(|| "model mapping glob matcher has no value".to_string())?,
            },
            "default" => Matcher::Default,
            _ => return Err("model mapping contains an unsupported matcher".to_string()),
        };
        let endpoint_kinds = serde_json::from_str::<Vec<String>>(&stored.endpoint_conditions_json)
            .map_err(|_| "model mapping endpoint conditions are invalid".to_string())?
            .into_iter()
            .map(|value| match value.as_str() {
                "chat_completions" => Ok(EndpointKind::ChatCompletions),
                "responses" => Ok(EndpointKind::Responses),
                "embeddings" => Ok(EndpointKind::Embeddings),
                "models" => Ok(EndpointKind::Models),
                "usage" => Ok(EndpointKind::Usage),
                _ => Err("model mapping contains an unsupported endpoint condition".to_string()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let action = match stored.action_kind.as_str() {
            "preserve" => Action::Preserve,
            "reject" => Action::Reject {
                rejection_kind: match stored.rejection_kind.as_deref() {
                    Some("unsupported_model") => RejectionKind::UnsupportedModel,
                    Some("client_not_allowed") => RejectionKind::ClientNotAllowed,
                    Some("policy_denied") | None => RejectionKind::PolicyDenied,
                    Some(_) => return Err("model mapping rejection kind is invalid".to_string()),
                },
                message: stored.rejection_message,
            },
            "map_fixed" | "map_fallback_chain" => {
                let targets = store
                    .list_rule_targets(read.connection(), &stored.id)
                    .await
                    .map_err(|error| format!("load model mapping target failed: {error}"))?;
                let targets = targets
                    .into_iter()
                    .map(|target| match target.target_kind.as_str() {
                        "literal"
                            if target.literal_upstream_model.is_some()
                                && target.model_profile_id.is_none() =>
                        {
                            Ok(TargetRef::Literal {
                                upstream_model: target
                                    .literal_upstream_model
                                    .expect("checked above"),
                            })
                        }
                        "model_profile"
                            if target.model_profile_id.is_some()
                                && target.literal_upstream_model.is_none() =>
                        {
                            Ok(TargetRef::ModelProfile {
                                model_profile_id: target.model_profile_id.expect("checked above"),
                            })
                        }
                        _ => Err("model mapping target row is inconsistent".to_string()),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if stored.action_kind == "map_fixed" {
                    if targets.len() != 1 {
                        return Err(
                            "model mapping fixed rule must have exactly one target".to_string()
                        );
                    }
                    Action::MapFixed {
                        target: targets.into_iter().next().expect("length checked"),
                    }
                } else {
                    let fallback_trigger = match stored.fallback_trigger.as_deref() {
                        Some("no_eligible_target") => crate::models::model_mapping::FallbackTrigger::NoEligibleTarget,
                        Some("retry_exhausted_before_output") => crate::models::model_mapping::FallbackTrigger::RetryExhaustedBeforeOutput,
                        _ => return Err("model mapping fallback trigger is invalid".to_string()),
                    };
                    Action::MapFallbackChain {
                        targets,
                        fallback_trigger,
                    }
                }
            }
            _ => return Err("model mapping contains an unsupported action".to_string()),
        };
        rules.push(ModelMappingRule {
            id: stored.id,
            priority: stored.priority.max(1) as u32,
            enabled: stored.enabled,
            matcher,
            conditions: RuleConditions {
                endpoint_kinds: if endpoint_kinds.is_empty() {
                    None
                } else {
                    Some(endpoint_kinds)
                },
                stream: parse_requirement(&stored.stream_condition)?,
                tools: parse_requirement(&stored.tools_condition)?,
                vision: parse_requirement(&stored.vision_condition)?,
                reasoning: parse_requirement(&stored.reasoning_condition)?,
            },
            action,
            note: stored.note,
            revision: stored.revision.max(1) as u64,
        });
    }
    drop(read);
    let document = ModelMappingDocumentV1 {
        format_version: crate::models::model_mapping::MODEL_MAPPING_FORMAT_VERSION,
        base_revision: policy.revision.max(1) as u64,
        policy: ModelMappingPolicy {
            unmatched_model_behavior: if policy.unmatched_model_behavior == "reject" {
                crate::models::model_mapping::UnmatchedModelBehavior::Reject
            } else {
                crate::models::model_mapping::UnmatchedModelBehavior::Preserve
            },
        },
        rules,
        profiles: stored_profiles
            .into_iter()
            .map(|profile| {
                Ok(crate::models::model_mapping::ModelProfile {
                    id: profile.id,
                    canonical_model: profile.canonical_model,
                    display_name: profile.display_name,
                    default_upstream_model: profile.default_upstream_model,
                    status: match profile.status.as_str() {
                        "active" => crate::models::model_mapping::ModelProfileStatus::Active,
                        "archived" => crate::models::model_mapping::ModelProfileStatus::Archived,
                        _ => return Err("model mapping profile status is invalid".to_string()),
                    },
                    note: profile.note,
                    revision: u64::try_from(profile.revision)
                        .map_err(|_| "model mapping profile revision is invalid".to_string())?,
                    created_at_ms: profile.created_at_ms,
                    updated_at_ms: profile.updated_at_ms,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        bindings: stored_bindings
            .into_iter()
            .map(|binding| {
                Ok(crate::models::model_mapping::ModelOfferingBinding {
                    id: binding.id,
                    model_profile_id: binding.model_profile_id,
                    station_key_id: binding.station_key_id,
                    station_id: binding.station_id,
                    upstream_model: binding.upstream_model,
                    source: match binding.source.as_str() {
                        "manual" => crate::models::model_mapping::ModelBindingSource::Manual,
                        "discovered" => {
                            crate::models::model_mapping::ModelBindingSource::Discovered
                        }
                        "migrated" => crate::models::model_mapping::ModelBindingSource::Migrated,
                        _ => return Err("model mapping binding source is invalid".to_string()),
                    },
                    enabled: binding.enabled,
                    note: binding.note,
                    revision: u64::try_from(binding.revision)
                        .map_err(|_| "model mapping binding revision is invalid".to_string())?,
                    created_at_ms: binding.created_at_ms,
                    updated_at_ms: binding.updated_at_ms,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    };
    install_document(document.clone(), policy.revision.max(1) as u64)
        .map_err(|error| error.to_string())?;
    // The active aggregate is authoritative. Startup may recreate a missing
    // mirror, but an existing invalid/different file is never auto-overwritten.
    sync_model_mapping_file(runtime, &document, policy.revision.max(1) as u64, false)
        .await
        .map_err(|error| format!("reconcile model mapping document failed: {error}"))?;
    Ok(())
}

fn mapping_document_coordinator(
    runtime: &crate::persistence::runtime::PersistenceHandle,
) -> PolicyDocumentCoordinator {
    let root = runtime
        .database_path()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    PolicyDocumentCoordinator::shared(root)
}

/// The workspace read model needs both the durable projection and a bounded
/// observation of the managed file.  `materialized_revision` alone is not a
/// file-existence signal: the file may have been removed or changed outside
/// the application after the last successful publish.
#[derive(Debug, Clone)]
pub(crate) struct ModelMappingDocumentSyncSnapshot {
    pub(crate) sync: Option<crate::persistence::stores::document_sync_store::StoredDocumentSync>,
    pub(crate) file_present: bool,
}

/// Reconcile the model-mapping mirror on a read-model request.  This is
/// intentionally observation-only: an external or invalid file never
/// replaces the active SQLite aggregate.  A stable file can repair durable
/// materialization evidence, while all other observations are represented by
/// the existing shared document-sync state machine.
pub(crate) async fn reconcile_model_mapping_document_sync(
    runtime: crate::persistence::runtime::PersistenceHandle,
) -> Result<ModelMappingDocumentSyncSnapshot, crate::persistence::error::PersistenceError> {
    use crate::models::document_sync::{DocumentKind, MODEL_MAPPING_DOCUMENT_KIND};
    use crate::services::policy_documents::ReconciliationState;

    let kind = DocumentKind::ModelMapping;
    let current = {
        let mut read = runtime.begin_read().await?;
        crate::persistence::stores::document_sync_store::DocumentSyncStore
            .load(read.connection(), MODEL_MAPPING_DOCUMENT_KIND)
            .await?
    };
    let Some(current) = current else {
        return Ok(ModelMappingDocumentSyncSnapshot {
            sync: None,
            file_present: false,
        });
    };

    let coordinator = mapping_document_coordinator(&runtime);
    let observation = coordinator
        .reconcile(kind, current.desired_canonical_digest.as_deref())
        .await;
    let now_ms = chrono::Utc::now().timestamp_millis().max(0);
    {
        let mut write = runtime.begin_write().await?;
        let store = crate::persistence::stores::document_sync_store::DocumentSyncStore;
        match observation.state {
            ReconciliationState::Stable => {
                // The digest comparison above proves that this is the
                // current desired bytes; mark evidence only for the current
                // desired revision, so a concurrent apply cannot be
                // overwritten by a stale observation.
                if let Some(digest) = current.desired_canonical_digest.as_deref() {
                    let _ = store
                        .mark_materialized(
                            write.connection(),
                            MODEL_MAPPING_DOCUMENT_KIND,
                            current.desired_revision,
                            Some(digest),
                            now_ms,
                        )
                        .await?;
                }
            }
            ReconciliationState::Changed => {
                let _ = store
                    .mark_external_change(
                        write.connection(),
                        MODEL_MAPPING_DOCUMENT_KIND,
                        observation.digest.as_deref(),
                        Some("external_change"),
                        now_ms,
                    )
                    .await?;
            }
            ReconciliationState::Missing => {
                let _ = store
                    .mark_error(
                        write.connection(),
                        MODEL_MAPPING_DOCUMENT_KIND,
                        "document_missing",
                        now_ms,
                    )
                    .await?;
            }
            ReconciliationState::Unavailable => {
                let _ = store
                    .mark_error(
                        write.connection(),
                        MODEL_MAPPING_DOCUMENT_KIND,
                        "document_unavailable",
                        now_ms,
                    )
                    .await?;
            }
        }
        write.commit().await?;
    }

    let sync = {
        let mut read = runtime.begin_read().await?;
        crate::persistence::stores::document_sync_store::DocumentSyncStore
            .load(read.connection(), MODEL_MAPPING_DOCUMENT_KIND)
            .await?
    };
    Ok(ModelMappingDocumentSyncSnapshot {
        sync,
        file_present: observation.identity.is_some(),
    })
}

/// Process a stable external mirror as a trusted file-sync input. The file is
/// never read by proxy execution; this adapter performs strict decode and
/// compiler admission before routing the complete document through the same
/// SQLite CAS path as a UI save. A stale base revision is retained as an
/// external conflict instead of being force-applied.
pub(crate) async fn reconcile_external_model_mapping_document(
    runtime: crate::persistence::runtime::PersistenceHandle,
) -> Result<(), crate::persistence::error::PersistenceError> {
    use crate::models::document_sync::{DocumentKind, MODEL_MAPPING_DOCUMENT_KIND};

    let (current_sync, current_revision) = {
        let mut read = runtime.begin_read().await?;
        let sync = crate::persistence::stores::document_sync_store::DocumentSyncStore
            .load(read.connection(), MODEL_MAPPING_DOCUMENT_KIND)
            .await?;
        let revision = crate::persistence::stores::model_mapping_store::ModelMappingStore
            .load_policy(read.connection())
            .await?
            .revision;
        let revision = u64::try_from(revision).map_err(|_| {
            crate::persistence::error::PersistenceError::InvariantViolation(
                "model mapping revision is invalid".into(),
            )
        })?;
        (sync, revision)
    };
    let Some(current_sync) = current_sync else {
        return Ok(());
    };
    let coordinator = mapping_document_coordinator(&runtime);
    let stable = match coordinator.read_stable(DocumentKind::ModelMapping).await {
        Ok(stable) => stable,
        Err(PolicyDocumentError::Missing) => {
            mark_mapping_sync_error(&runtime, "document_missing").await?;
            return Ok(());
        }
        Err(PolicyDocumentError::Unstable) => return Ok(()),
        Err(_) => {
            mark_mapping_sync_error(&runtime, "document_unavailable").await?;
            return Ok(());
        }
    };
    if current_sync
        .desired_canonical_digest
        .as_deref()
        .is_some_and(|digest| digest == stable.digest)
    {
        let mut write = runtime.begin_write().await?;
        let _ = crate::persistence::stores::document_sync_store::DocumentSyncStore
            .mark_materialized(
                write.connection(),
                MODEL_MAPPING_DOCUMENT_KIND,
                current_sync.desired_revision,
                Some(&stable.digest),
                chrono::Utc::now().timestamp_millis().max(0),
            )
            .await?;
        write.commit().await?;
        return Ok(());
    }
    let document = match decode_strict_json::<ModelMappingDocumentV1>(&stable.bytes) {
        Ok(document) => document,
        Err(_) => {
            mark_mapping_sync_error(&runtime, "invalid_document").await?;
            return Ok(());
        }
    };
    if document.base_revision != current_revision {
        let mut write = runtime.begin_write().await?;
        let _ = crate::persistence::stores::document_sync_store::DocumentSyncStore
            .mark_external_change(
                write.connection(),
                MODEL_MAPPING_DOCUMENT_KIND,
                Some(&stable.digest),
                Some("revision_conflict"),
                chrono::Utc::now().timestamp_millis().max(0),
            )
            .await?;
        write.commit().await?;
        return Ok(());
    }
    if let Err(error) = compile_at_revision(&document, document.base_revision) {
        mark_mapping_sync_error(&runtime, "invalid_document").await?;
        let _ = error;
        return Ok(());
    }
    match persist_document_at_revision(
        runtime.clone(),
        document,
        current_revision,
        TrustedDocumentSource::file_watch(),
    )
    .await
    {
        Ok(_) => Ok(()),
        Err(crate::persistence::error::PersistenceError::RevisionConflict(_)) => {
            let mut write = runtime.begin_write().await?;
            let _ = crate::persistence::stores::document_sync_store::DocumentSyncStore
                .mark_external_change(
                    write.connection(),
                    MODEL_MAPPING_DOCUMENT_KIND,
                    Some(&stable.digest),
                    Some("revision_conflict"),
                    chrono::Utc::now().timestamp_millis().max(0),
                )
                .await?;
            write.commit().await?;
            Ok(())
        }
        Err(error) => Err(error),
    }
}

async fn mark_mapping_sync_error(
    runtime: &crate::persistence::runtime::PersistenceHandle,
    code: &str,
) -> Result<(), crate::persistence::error::PersistenceError> {
    let mut write = runtime.begin_write().await?;
    crate::persistence::stores::document_sync_store::DocumentSyncStore
        .mark_error(
            write.connection(),
            crate::models::document_sync::MODEL_MAPPING_DOCUMENT_KIND,
            code,
            chrono::Utc::now().timestamp_millis().max(0),
        )
        .await?;
    write.commit().await
}

/// Reconciles the model-mapping mirror after an apply or at startup.  The
/// caller chooses whether an explicit apply may replace an existing file;
/// startup only repairs a missing mirror and records external observations.
async fn sync_model_mapping_file(
    runtime: crate::persistence::runtime::PersistenceHandle,
    document: &ModelMappingDocumentV1,
    revision: u64,
    replace_existing: bool,
) -> Result<(), crate::persistence::error::PersistenceError> {
    let canonical = canonical_document_json(document).map_err(|error| {
        crate::persistence::error::PersistenceError::InvariantViolation(error.to_string())
    })?;
    let digest = hex_digest(&canonical);
    let kind = crate::models::document_sync::MODEL_MAPPING_DOCUMENT_KIND;
    let coordinator = mapping_document_coordinator(&runtime);
    let _operation_guard = coordinator.acquire_operation_guard().await;
    let revision_i64 = i64::try_from(revision).map_err(|_| {
        crate::persistence::error::PersistenceError::InvariantViolation(
            "model mapping revision exceeds SQLite range".into(),
        )
    })?;
    let current_revision: i64 = {
        let mut read = runtime.begin_read().await?;
        crate::persistence::stores::model_mapping_store::ModelMappingStore
            .load_policy(read.connection())
            .await?
            .revision
    };
    if current_revision != revision_i64 {
        return Ok(());
    }
    {
        let mut write = runtime.begin_write().await?;
        crate::persistence::stores::document_sync_store::DocumentSyncStore
            .upsert_desired(
                write.connection(),
                kind,
                revision,
                Some(&digest),
                chrono::Utc::now().timestamp_millis().max(0),
            )
            .await?;
        write.commit().await?;
    }
    let previous_materialized_digest = {
        let mut read = runtime.begin_read().await?;
        crate::persistence::stores::document_sync_store::DocumentSyncStore
            .load(read.connection(), kind)
            .await?
            .and_then(|sync| sync.materialized_canonical_digest)
    };
    let existing = coordinator
        .files()
        .read_once(crate::models::document_sync::DocumentKind::ModelMapping);
    let should_materialize = match existing {
        Err(PolicyDocumentError::Missing) => true,
        Ok(observed)
            if replace_existing
                || observed.digest == digest
                || previous_materialized_digest.as_deref() == Some(observed.digest.as_str()) =>
        {
            true
        }
        Ok(observed) => {
            let incoming = decode_strict_json::<ModelMappingDocumentV1>(&observed.bytes);
            match incoming {
                Ok(incoming) => {
                    let incoming_canonical =
                        canonical_document_json(&incoming).map_err(|error| {
                            crate::persistence::error::PersistenceError::InvariantViolation(
                                error.to_string(),
                            )
                        })?;
                    if incoming_canonical == canonical {
                        true
                    } else {
                        let mut write = runtime.begin_write().await?;
                        crate::persistence::stores::document_sync_store::DocumentSyncStore
                            .mark_external_change(
                                write.connection(),
                                kind,
                                Some(&observed.digest),
                                Some("external_change"),
                                chrono::Utc::now().timestamp_millis().max(0),
                            )
                            .await?;
                        write.commit().await?;
                        false
                    }
                }
                Err(_) => {
                    mark_mapping_sync_error(&runtime, "invalid_document").await?;
                    false
                }
            }
        }
        Err(_) => {
            mark_mapping_sync_error(&runtime, "document_unavailable").await?;
            false
        }
    };
    if !should_materialize {
        return Ok(());
    }
    if let Err(error) = coordinator.files().materialize(
        crate::models::document_sync::DocumentKind::ModelMapping,
        &canonical,
    ) {
        mark_mapping_sync_error(&runtime, "materialization_failed").await?;
        let _ = error;
        return Ok(());
    }

    // A SQLite CAS is process-safe, while the file replace is a separate
    // resource. Another process may commit a newer revision after the
    // pre-materialization check but before this replace. Re-read the active
    // fence after publishing and immediately converge to the newest history
    // document instead of leaving a stale mirror behind.
    let latest_revision: i64 = {
        let mut read = runtime.begin_read().await?;
        crate::persistence::stores::model_mapping_store::ModelMappingStore
            .load_policy(read.connection())
            .await?
            .revision
    };
    if latest_revision != revision_i64 {
        let latest_json: Option<String> = {
            let mut read = runtime.begin_read().await?;
            crate::persistence::stores::model_mapping_store::ModelMappingStore
                .load_history_revision(read.connection(), latest_revision)
                .await?
                .map(|history| history.document_json)
        };
        drop(_operation_guard);
        if let Some(latest_json) = latest_json {
            let latest_document = decode_document(&latest_json).map_err(|error| {
                crate::persistence::error::PersistenceError::InvariantViolation(error.to_string())
            })?;
            let latest_revision = u64::try_from(latest_revision).map_err(|_| {
                crate::persistence::error::PersistenceError::InvariantViolation(
                    "model mapping revision is invalid".into(),
                )
            })?;
            Box::pin(sync_model_mapping_file(
                runtime,
                &latest_document,
                latest_revision,
                true,
            ))
            .await?;
        }
        return Ok(());
    }
    let mut write = runtime.begin_write().await?;
    crate::persistence::stores::document_sync_store::DocumentSyncStore
        .mark_materialized(
            write.connection(),
            kind,
            revision,
            Some(&digest),
            chrono::Utc::now().timestamp_millis().max(0),
        )
        .await?;
    write.commit().await
}

fn parse_requirement(value: &str) -> Result<ConditionRequirement, String> {
    match value {
        "any" => Ok(ConditionRequirement::Any),
        "required" => Ok(ConditionRequirement::Required),
        "forbidden" => Ok(ConditionRequirement::Forbidden),
        _ => Err("model mapping condition is invalid".to_string()),
    }
}

pub(crate) fn current_document() -> ModelMappingDocumentV1 {
    runtime_state()
        .read()
        .expect("mapping runtime lock poisoned")
        .document
        .clone()
}

pub(crate) fn current_configuration() -> CompiledModelMappingConfiguration {
    runtime_state()
        .read()
        .expect("mapping runtime lock poisoned")
        .compiled
        .clone()
}

/// Captures the compiled mapping and its revision atomically under one read
/// lock. The returned pair is self-consistent even if another task installs a
/// newer document immediately afterwards.
pub(crate) fn current_snapshot() -> ModelMappingSnapshot {
    let guard = runtime_state()
        .read()
        .expect("mapping runtime lock poisoned");
    ModelMappingSnapshot {
        revision: guard.compiled.mapping_revision,
        configuration: guard.compiled.clone(),
    }
}

pub(crate) async fn persist_document(
    runtime: crate::persistence::runtime::PersistenceHandle,
    document: ModelMappingDocumentV1,
    source: TrustedDocumentSource,
) -> Result<ModelMappingDocumentV1, crate::persistence::error::PersistenceError> {
    let expected_revision = document.base_revision;
    persist_document_at_revision(runtime, document, expected_revision, source).await
}

pub(crate) async fn persist_document_at_revision(
    runtime: crate::persistence::runtime::PersistenceHandle,
    mut document: ModelMappingDocumentV1,
    expected_revision: u64,
    source: TrustedDocumentSource,
) -> Result<ModelMappingDocumentV1, crate::persistence::error::PersistenceError> {
    let idempotent_document = (document.base_revision == expected_revision).then(|| {
        let mut candidate = document.clone();
        candidate.base_revision = expected_revision;
        for rule in &mut candidate.rules {
            rule.revision = expected_revision;
        }
        candidate
    });
    let idempotent_json = idempotent_document
        .as_ref()
        .map(canonical_document_json)
        .transpose()
        .map_err(|error| {
            crate::persistence::error::PersistenceError::InvariantViolation(error.to_string())
        })?;
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or_else(|| crate::persistence::error::PersistenceError::ConstraintViolation)?;
    document.base_revision = next_revision;
    for rule in &mut document.rules {
        rule.revision = next_revision;
    }
    let compiled = compile_at_revision(&document, next_revision).map_err(|error| {
        crate::persistence::error::PersistenceError::InvariantViolation(error.to_string())
    })?;
    let canonical = canonical_document_json(&document).map_err(|error| {
        crate::persistence::error::PersistenceError::InvariantViolation(error.to_string())
    })?;
    let document_json = String::from_utf8(canonical).map_err(|_| {
        crate::persistence::error::PersistenceError::InvariantViolation(
            "mapping document is not UTF-8".into(),
        )
    })?;
    let source_label = source.history_label();
    let mut session = runtime.begin_write().await?;
    let connection = session.connection();
    let expected_revision_i64 = i64::try_from(expected_revision).map_err(|_| {
        crate::persistence::error::PersistenceError::InvariantViolation(
            "model mapping revision exceeds SQLite range".into(),
        )
    })?;
    let next_revision_i64 = i64::try_from(next_revision).map_err(|_| {
        crate::persistence::error::PersistenceError::InvariantViolation(
            "model mapping revision exceeds SQLite range".into(),
        )
    })?;
    let current = crate::persistence::stores::model_mapping_store::ModelMappingStore
        .load_policy(connection)
        .await?
        .revision;
    if current != expected_revision_i64 {
        return Err(
            crate::persistence::error::PersistenceError::RevisionConflict("model_mapping".into()),
        );
    }
    if let Some(idempotent_json) = idempotent_json.as_deref() {
        let current_history = crate::persistence::stores::model_mapping_store::ModelMappingStore
            .load_history_revision(connection, expected_revision_i64)
            .await?
            .map(|history| history.document_json);
        let current_history_json = current_history
            .as_deref()
            .and_then(|json| decode_document(json).ok())
            .and_then(|mut document| {
                document.base_revision = expected_revision;
                for rule in &mut document.rules {
                    rule.revision = expected_revision;
                }
                canonical_document_json(&document).ok()
            });
        if current_history_json.as_deref() == Some(idempotent_json) {
            let active_document = idempotent_document.expect("idempotent document");
            let now_ms = chrono::Utc::now().timestamp_millis();
            let digest = hex_digest(idempotent_json);
            crate::persistence::stores::document_sync_store::DocumentSyncStore
                .upsert_desired(
                    connection,
                    crate::models::document_sync::MODEL_MAPPING_DOCUMENT_KIND,
                    expected_revision,
                    Some(&digest),
                    now_ms,
                )
                .await?;
            session.commit().await?;
            install_document(active_document.clone(), expected_revision)
                .map_err(crate::persistence::error::PersistenceError::InvariantViolation)?;
            sync_model_mapping_file(runtime, &active_document, expected_revision, true).await?;
            return Ok(active_document);
        }
    }
    let digest = hex_digest(document_json.as_bytes());
    let now_ms = chrono::Utc::now().timestamp_millis();
    crate::persistence::stores::model_mapping_store::ModelMappingStore
        .replace_aggregate(
            connection,
            &document,
            expected_revision_i64,
            next_revision_i64,
            &document_json,
            source_label,
            &digest,
            now_ms,
        )
        .await?;
    session.commit().await?;
    install_document(document.clone(), next_revision)
        .map_err(crate::persistence::error::PersistenceError::InvariantViolation)?;
    sync_model_mapping_file(runtime, &document, next_revision, true).await?;
    crate::application::queries::read_model_revision::publish_domain_revision_notice(
        crate::application::queries::read_model_revision::DomainRevisionNotice::for_scope(
            "model_mapping",
            next_revision_i64,
        ),
    );
    let _ = compiled;
    Ok(document)
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

pub(crate) fn resolve_request(
    requested_model: Option<String>,
    endpoint: EndpointKind,
    stream: bool,
    tools: bool,
    vision: bool,
    reasoning: bool,
) -> Result<ResolvedModelPlan, ModelMappingResolutionError> {
    let configuration = current_configuration();
    let facts = ModelRequestFacts {
        requested_model,
        endpoint,
        stream,
        tools,
        vision,
        reasoning,
    };
    match resolve(&configuration, &facts) {
        Ok(plan) => Ok(plan),
        Err(ModelMappingResolutionError::TargetRequiresCandidateContext) => Ok(ResolvedModelPlan {
            requested_model: facts
                .requested_model
                .as_deref()
                .and_then(normalize_model_name),
            disposition: Disposition::Mapped,
            matched_rule_id: None,
            mapping_revision: configuration.mapping_revision,
            model_resolution_fence: configuration.model_resolution_fence,
            target_policy: TargetPolicy::None,
            fallback_trigger: None,
            target_models: Vec::new(),
            rejection_kind: None,
            rejection_message: None,
            decision_evidence: vec![DecisionEvidence {
                code: "model_mapping_candidate_context_deferred",
                rule_id: None,
            }],
        }),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::document_sync::MODEL_MAPPING_DOCUMENT_KIND;
    use crate::persistence::runtime::PersistenceRuntime;

    // The production mapping snapshot is process-wide. Serialize tests that
    // persist documents so parallel test execution cannot observe another
    // fixture's installed revision.
    async fn model_mapping_test_guard() -> tokio::sync::OwnedMutexGuard<()> {
        super::acquire_model_mapping_test_guard().await
    }

    fn persisted_document(base_revision: u64, upstream_model: &str) -> ModelMappingDocumentV1 {
        ModelMappingDocumentV1 {
            format_version: crate::models::model_mapping::MODEL_MAPPING_FORMAT_VERSION,
            base_revision,
            policy: ModelMappingPolicy::default(),
            rules: vec![ModelMappingRule {
                id: "rule-idempotency".to_string(),
                priority: 10,
                enabled: true,
                matcher: Matcher::Exact {
                    model: "client-model".to_string(),
                },
                conditions: RuleConditions::default(),
                action: Action::MapFixed {
                    target: TargetRef::Literal {
                        upstream_model: upstream_model.to_string(),
                    },
                },
                note: None,
                revision: 1,
            }],
            profiles: Vec::new(),
            bindings: Vec::new(),
        }
    }

    #[tokio::test]
    async fn identical_document_is_idempotent_and_changed_document_bumps_revision() {
        let _guard = model_mapping_test_guard().await;
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&temp.path().join("mapping.sqlite3"))
            .await
            .expect("runtime");

        let baseline = ModelMappingDocumentV1 {
            base_revision: 1,
            ..ModelMappingDocumentV1::default()
        };
        let baseline_result =
            persist_document(runtime.handle(), baseline, TrustedDocumentSource::ui())
                .await
                .expect("baseline idempotent apply");
        assert_eq!(baseline_result.base_revision, 1);
        let mut baseline_read = runtime
            .handle()
            .begin_read()
            .await
            .expect("baseline sync read");
        let baseline_sync_revision: i64 = sqlx::query_scalar(
            "SELECT desired_revision
             FROM routing_document_sync
             WHERE document_kind = 'model_mapping'",
        )
        .fetch_one(baseline_read.connection())
        .await
        .expect("baseline desired revision");
        let baseline_sync_digest: String = sqlx::query_scalar(
            "SELECT desired_canonical_digest
             FROM routing_document_sync
             WHERE document_kind = 'model_mapping'",
        )
        .fetch_one(baseline_read.connection())
        .await
        .expect("baseline desired digest");
        let expected_baseline_digest = hex_digest(
            &canonical_document_json(&baseline_result).expect("canonical baseline document"),
        );
        assert_eq!(baseline_sync_revision, 1);
        assert_eq!(baseline_sync_digest, expected_baseline_digest);
        drop(baseline_read);

        let first = persist_document(
            runtime.handle(),
            persisted_document(1, "native-model"),
            TrustedDocumentSource::ui(),
        )
        .await
        .expect("first apply");
        assert_eq!(first.base_revision, 2);

        let mut sync_read = runtime.handle().begin_read().await.expect("sync read");
        let desired_revision: i64 = sqlx::query_scalar(
            "SELECT desired_revision
             FROM routing_document_sync
             WHERE document_kind = 'model_mapping'",
        )
        .fetch_one(sync_read.connection())
        .await
        .expect("mapping desired revision");
        let desired_digest: String = sqlx::query_scalar(
            "SELECT desired_canonical_digest
             FROM routing_document_sync
             WHERE document_kind = 'model_mapping'",
        )
        .fetch_one(sync_read.connection())
        .await
        .expect("mapping desired digest");
        let expected_digest =
            hex_digest(&canonical_document_json(&first).expect("canonical first document"));
        assert_eq!(desired_revision, 2);
        assert_eq!(desired_digest, expected_digest);
        let sync_row_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM routing_document_sync
             WHERE document_kind = 'model_mapping'",
        )
        .fetch_one(sync_read.connection())
        .await
        .expect("mapping sync row count");
        assert_eq!(sync_row_count, 1);
        drop(sync_read);

        let repeated =
            persist_document(runtime.handle(), first.clone(), TrustedDocumentSource::ui())
                .await
                .expect("idempotent apply");
        assert_eq!(repeated, first);

        let mut idempotent_read = runtime
            .handle()
            .begin_read()
            .await
            .expect("idempotent read");
        let idempotent_policy_revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM model_mapping_policies WHERE singleton_key = 1",
        )
        .fetch_one(idempotent_read.connection())
        .await
        .expect("idempotent policy revision");
        let idempotent_history_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM model_mapping_document_history")
                .fetch_one(idempotent_read.connection())
                .await
                .expect("idempotent history count");
        let idempotent_sync_revision: i64 = sqlx::query_scalar(
            "SELECT desired_revision
             FROM routing_document_sync
             WHERE document_kind = 'model_mapping'",
        )
        .fetch_one(idempotent_read.connection())
        .await
        .expect("idempotent desired revision");
        let idempotent_sync_digest: String = sqlx::query_scalar(
            "SELECT desired_canonical_digest
             FROM routing_document_sync
             WHERE document_kind = 'model_mapping'",
        )
        .fetch_one(idempotent_read.connection())
        .await
        .expect("idempotent desired digest");
        assert_eq!(idempotent_policy_revision, 2);
        assert_eq!(idempotent_history_count, 2);
        assert_eq!(idempotent_sync_revision, desired_revision);
        assert_eq!(idempotent_sync_digest, desired_digest);
        drop(idempotent_read);

        let changed = persist_document(
            runtime.handle(),
            persisted_document(2, "changed-native-model"),
            TrustedDocumentSource::ui(),
        )
        .await
        .expect("changed apply");
        assert_eq!(changed.base_revision, 3);

        let mut replacement = persisted_document(3, "replacement-native-model");
        replacement.rules[0].id = "rule-replacement".to_string();
        let replaced = persist_document(runtime.handle(), replacement, TrustedDocumentSource::ui())
            .await
            .expect("replacement apply");
        assert_eq!(replaced.base_revision, 4);

        let mut read = runtime.handle().begin_read().await.expect("read session");
        let policy_revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM model_mapping_policies WHERE singleton_key = 1",
        )
        .fetch_one(read.connection())
        .await
        .expect("policy revision");
        let history_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM model_mapping_document_history")
                .fetch_one(read.connection())
                .await
                .expect("history count");
        let rule_revision_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM domain_revisions
             WHERE scope LIKE 'model_mapping_rule:%'",
        )
        .fetch_one(read.connection())
        .await
        .expect("rule revision count");
        let replacement_rule_revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM domain_revisions
             WHERE scope = 'model_mapping_rule:rule-replacement'",
        )
        .fetch_one(read.connection())
        .await
        .expect("replacement rule revision");
        let final_sync_revision: i64 = sqlx::query_scalar(
            "SELECT desired_revision
             FROM routing_document_sync
             WHERE document_kind = 'model_mapping'",
        )
        .fetch_one(read.connection())
        .await
        .expect("final desired revision");
        let final_sync_digest: String = sqlx::query_scalar(
            "SELECT desired_canonical_digest
             FROM routing_document_sync
             WHERE document_kind = 'model_mapping'",
        )
        .fetch_one(read.connection())
        .await
        .expect("final desired digest");
        let expected_final_digest = hex_digest(
            &canonical_document_json(&replaced).expect("canonical replacement document"),
        );
        assert_eq!(policy_revision, 4);
        assert_eq!(history_count, 4);
        assert_eq!(rule_revision_count, 1);
        assert_eq!(replacement_rule_revision, 4);
        assert_eq!(final_sync_revision, 4);
        assert_eq!(final_sync_digest, expected_final_digest);
        let old_rule_revision: Option<i64> = sqlx::query_scalar(
            "SELECT revision FROM domain_revisions
             WHERE scope = 'model_mapping_rule:rule-idempotency'",
        )
        .fetch_optional(read.connection())
        .await
        .expect("old rule revision lookup");
        assert!(old_rule_revision.is_none());
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn concurrent_document_applies_share_one_base_revision_winner() {
        let _guard = model_mapping_test_guard().await;
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&temp.path().join("mapping.sqlite3"))
            .await
            .expect("runtime");
        persist_document(
            runtime.handle(),
            ModelMappingDocumentV1 {
                base_revision: 1,
                ..ModelMappingDocumentV1::default()
            },
            TrustedDocumentSource::ui(),
        )
        .await
        .expect("publish baseline");

        let left = persisted_document(1, "concurrent-left");
        let right = persisted_document(1, "concurrent-right");
        let (left_result, right_result) = tokio::join!(
            persist_document(runtime.handle(), left, TrustedDocumentSource::ui(),),
            persist_document(runtime.handle(), right, TrustedDocumentSource::ui(),),
        );
        let successes = [left_result.as_ref(), right_result.as_ref()]
            .into_iter()
            .filter(|result| result.is_ok())
            .count();
        let conflicts = [left_result.as_ref(), right_result.as_ref()]
            .into_iter()
            .filter(|result| {
                matches!(
                    result,
                    Err(crate::persistence::error::PersistenceError::RevisionConflict(scope))
                        if scope == "model_mapping"
                )
            })
            .count();
        assert_eq!(successes, 1, "exactly one concurrent apply may win");
        assert_eq!(conflicts, 1, "the stale concurrent apply must conflict");

        let active = current_document();
        assert_eq!(active.base_revision, 2);
        let target = match &active.rules[0].action {
            Action::MapFixed {
                target: TargetRef::Literal { upstream_model },
            } => upstream_model.as_str(),
            action => panic!("unexpected winning action: {action:?}"),
        };
        assert!(
            matches!(target, "concurrent-left" | "concurrent-right"),
            "active document must be one complete concurrent winner"
        );
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn workspace_reconcile_reports_real_file_state_without_overwriting_active_document() {
        let _guard = model_mapping_test_guard().await;
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&temp.path().join("mapping.sqlite3"))
            .await
            .expect("runtime");
        let baseline = ModelMappingDocumentV1 {
            base_revision: 1,
            ..ModelMappingDocumentV1::default()
        };
        let active = persist_document(runtime.handle(), baseline, TrustedDocumentSource::ui())
            .await
            .expect("publish baseline");
        let path = temp.path().join("config").join("model-mapping.json");

        let stable = reconcile_model_mapping_document_sync(runtime.handle())
            .await
            .expect("stable reconcile");
        assert!(stable.file_present);
        assert_eq!(
            stable.sync.expect("sync row").state,
            crate::models::document_sync::DocumentSyncState::Synchronized
        );

        std::fs::remove_file(&path).expect("remove managed mirror");
        let missing = reconcile_model_mapping_document_sync(runtime.handle())
            .await
            .expect("missing reconcile");
        assert!(!missing.file_present);
        let missing_sync = missing.sync.expect("sync row after missing");
        assert_eq!(
            missing_sync.state,
            crate::models::document_sync::DocumentSyncState::Error
        );
        assert_eq!(
            missing_sync.last_error_code.as_deref(),
            Some("document_missing")
        );

        std::fs::write(&path, br#"{"formatVersion":1,"rules":[]}"#).expect("write external mirror");
        let changed = reconcile_model_mapping_document_sync(runtime.handle())
            .await
            .expect("external reconcile");
        assert!(changed.file_present);
        assert_eq!(
            changed.sync.expect("sync row after external change").state,
            crate::models::document_sync::DocumentSyncState::ExternalChange
        );
        assert_eq!(current_document().base_revision, active.base_revision);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn external_stable_document_is_imported_through_cas() {
        let _guard = model_mapping_test_guard().await;
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&temp.path().join("mapping.sqlite3"))
            .await
            .expect("runtime");
        persist_document(
            runtime.handle(),
            ModelMappingDocumentV1 {
                base_revision: 1,
                ..ModelMappingDocumentV1::default()
            },
            TrustedDocumentSource::ui(),
        )
        .await
        .expect("publish baseline");
        let incoming = persisted_document(1, "file-native-model");
        let bytes = canonical_document_json(&incoming).expect("canonical incoming document");
        std::fs::write(temp.path().join("config").join("model-mapping.json"), bytes)
            .expect("write external document");

        reconcile_external_model_mapping_document(runtime.handle())
            .await
            .expect("import external document");
        let active = current_document();
        assert_eq!(active.base_revision, 2);
        assert_eq!(
            active.rules[0].action,
            Action::MapFixed {
                target: TargetRef::Literal {
                    upstream_model: "file-native-model".to_string(),
                }
            }
        );
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn stale_external_document_is_rejected_by_active_revision_fence() {
        let _guard = model_mapping_test_guard().await;
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&temp.path().join("mapping.sqlite3"))
            .await
            .expect("runtime");
        let current = persist_document(
            runtime.handle(),
            persisted_document(1, "current-model"),
            TrustedDocumentSource::ui(),
        )
        .await
        .expect("publish current document");
        let stale = persisted_document(1, "stale-model");
        let bytes = canonical_document_json(&stale).expect("canonical stale document");
        std::fs::write(temp.path().join("config").join("model-mapping.json"), bytes)
            .expect("write stale external document");

        reconcile_external_model_mapping_document(runtime.handle())
            .await
            .expect("reconcile stale external document");
        assert_eq!(current_document(), current);
        let mut read = runtime.begin_read().await.expect("read runtime");
        let sync = crate::persistence::stores::document_sync_store::DocumentSyncStore
            .load(read.connection(), MODEL_MAPPING_DOCUMENT_KIND)
            .await
            .expect("load sync")
            .expect("sync row");
        assert_eq!(
            sync.state,
            crate::models::document_sync::DocumentSyncState::ExternalChange
        );
        assert_eq!(sync.last_error_code.as_deref(), Some("revision_conflict"));
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn startup_replaces_crash_left_previous_materialized_mapping() {
        let _guard = model_mapping_test_guard().await;
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&temp.path().join("mapping.sqlite3"))
            .await
            .expect("runtime");
        let first = persist_document(
            runtime.handle(),
            persisted_document(1, "first-materialized-model"),
            TrustedDocumentSource::ui(),
        )
        .await
        .expect("publish first document");
        let second = persist_document(
            runtime.handle(),
            persisted_document(2, "second-materialized-model"),
            TrustedDocumentSource::ui(),
        )
        .await
        .expect("publish second document");
        let path = temp.path().join("config").join("model-mapping.json");
        let first_digest =
            hex_digest(&canonical_document_json(&first).expect("first canonical document"));
        std::fs::write(
            &path,
            canonical_document_json(&first).expect("first canonical document"),
        )
        .expect("simulate stale file after crash");
        let mut write = runtime.begin_write().await.expect("write runtime");
        sqlx::query(
            "UPDATE routing_document_sync
             SET materialized_revision = 2,
                 materialized_canonical_digest = ?1,
                 sync_state = 'pending_materialization'
             WHERE document_kind = 'model_mapping'",
        )
        .bind(first_digest)
        .execute(write.connection())
        .await
        .expect("simulate stale materialization evidence");
        write.commit().await.expect("commit stale evidence");

        sync_model_mapping_file(runtime.handle(), &second, second.base_revision, false)
            .await
            .expect("startup reconciliation");
        let bytes = std::fs::read(path).expect("managed mapping document");
        let materialized =
            decode_document(std::str::from_utf8(&bytes).expect("managed mapping document is utf8"))
                .expect("managed mapping document decodes");
        assert_eq!(materialized, second);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn profile_and_fallback_targets_round_trip_through_normalized_rows() {
        let _guard = model_mapping_test_guard().await;
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&temp.path().join("mapping.sqlite3"))
            .await
            .expect("runtime");
        let document = ModelMappingDocumentV1 {
            base_revision: 1,
            rules: vec![ModelMappingRule {
                id: "fallback-rule".to_string(),
                priority: 10,
                enabled: true,
                matcher: Matcher::Exact {
                    model: "client-model".to_string(),
                },
                conditions: RuleConditions::default(),
                action: Action::MapFallbackChain {
                    targets: vec![
                        TargetRef::ModelProfile {
                            model_profile_id: "profile-a".to_string(),
                        },
                        TargetRef::Literal {
                            upstream_model: "backup-model".to_string(),
                        },
                    ],
                    fallback_trigger:
                        crate::models::model_mapping::FallbackTrigger::NoEligibleTarget,
                },
                note: Some("fallback test".to_string()),
                revision: 1,
            }],
            profiles: vec![crate::models::model_mapping::ModelProfile {
                id: "profile-a".to_string(),
                canonical_model: "client-model".to_string(),
                display_name: "Client Model".to_string(),
                default_upstream_model: Some("primary-model".to_string()),
                status: crate::models::model_mapping::ModelProfileStatus::Active,
                note: None,
                revision: 1,
                created_at_ms: 0,
                updated_at_ms: 0,
            }],
            bindings: Vec::new(),
            ..ModelMappingDocumentV1::default()
        };
        let persisted = persist_document(runtime.handle(), document, TrustedDocumentSource::ui())
            .await
            .expect("persist phase2 document");
        assert_eq!(persisted.base_revision, 2);
        initialize_from_persistence(runtime.handle())
            .await
            .expect("reload phase2 document");
        let active = current_document();
        assert_eq!(active.profiles.len(), 1);
        assert_eq!(active.profiles[0].id, "profile-a");
        assert_eq!(active.rules.len(), 1);
        match &active.rules[0].action {
            Action::MapFallbackChain {
                targets,
                fallback_trigger,
            } => {
                assert_eq!(targets.len(), 2);
                assert!(matches!(targets[0], TargetRef::ModelProfile { .. }));
                assert!(matches!(targets[1], TargetRef::Literal { .. }));
                assert_eq!(
                    *fallback_trigger,
                    crate::models::model_mapping::FallbackTrigger::NoEligibleTarget
                );
            }
            action => panic!("expected fallback action, got {action:?}"),
        }
        runtime.close().await.expect("close runtime");
    }

    #[test]
    fn request_mapping_trace_preserves_resolver_evidence_without_secrets() {
        let plan = ResolvedModelPlan {
            requested_model: Some("codex-5.4".to_string()),
            disposition: Disposition::Mapped,
            matched_rule_id: Some("rule-codex".to_string()),
            mapping_revision: 7,
            model_resolution_fence: "fence-7".to_string(),
            target_policy: TargetPolicy::Fixed,
            fallback_trigger: None,
            target_models: vec![ResolvedTarget {
                target_rank: 0,
                route_model: "deepseek-v4-flash".to_string(),
                resolution_reason: ResolutionReason::RuleMatch,
                binding_revision: None,
            }],
            rejection_kind: None,
            rejection_message: None,
            decision_evidence: Vec::new(),
        };
        record_request_trace("trace-test-mapped", &plan);
        let trace = request_trace("trace-test-mapped").expect("recorded trace");
        assert_eq!(trace.requested_model.as_deref(), Some("codex-5.4"));
        assert_eq!(trace.route_model.as_deref(), Some("deepseek-v4-flash"));
        assert_eq!(trace.upstream_model.as_deref(), Some("deepseek-v4-flash"));
        assert_eq!(trace.mapping_revision, 7);
        assert_eq!(trace.resolution_fence, "fence-7");
        assert_eq!(trace.matched_rule_id.as_deref(), Some("rule-codex"));
        assert_eq!(trace.target_rank, Some(0));
        assert_eq!(trace.failure_code, None);
    }

    #[test]
    fn request_mapping_trace_keeps_rank_zero_for_fallback_plan() {
        let plan = ResolvedModelPlan {
            requested_model: Some("logical-model".to_string()),
            disposition: Disposition::Mapped,
            matched_rule_id: Some("rule-fallback".to_string()),
            mapping_revision: 9,
            model_resolution_fence: "fence-9".to_string(),
            target_policy: TargetPolicy::Fallback,
            fallback_trigger: Some(crate::models::model_mapping::FallbackTrigger::NoEligibleTarget),
            target_models: vec![
                ResolvedTarget {
                    target_rank: 0,
                    route_model: "primary-model".to_string(),
                    resolution_reason: ResolutionReason::RuleMatch,
                    binding_revision: None,
                },
                ResolvedTarget {
                    target_rank: 1,
                    route_model: "backup-model".to_string(),
                    resolution_reason: ResolutionReason::RuleMatch,
                    binding_revision: None,
                },
            ],
            rejection_kind: None,
            rejection_message: None,
            decision_evidence: Vec::new(),
        };
        record_request_trace("trace-test-fallback", &plan);
        let trace = request_trace("trace-test-fallback").expect("recorded fallback trace");
        assert_eq!(trace.route_model.as_deref(), Some("primary-model"));
        assert_eq!(trace.upstream_model.as_deref(), Some("primary-model"));
        assert_eq!(trace.target_rank, Some(0));
    }

    #[test]
    fn missing_request_mapping_trace_is_explicitly_empty() {
        assert!(request_trace("trace-test-missing").is_none());
    }
}
