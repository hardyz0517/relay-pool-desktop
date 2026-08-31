use crate::{
    application::model_mapping::{
        candidate_variants, resolve_for_candidate, CandidateModelVariant,
        CandidateResolutionContext,
    },
    application::routing_engine::{
        algorithm_profile::DispatchAlgorithmProfile,
        candidate_plan::RoutePlanPricingSnapshot,
        planning_snapshot::{CandidateSnapshot, PlanningSnapshot, RuntimeOverlaySnapshot},
    },
    application::routing_engine::{
        factors::responsiveness_score,
        request::{GroupFilterMode, RouteKind, RouteRequestFacts},
    },
    application::routing_policy::AttemptBudgetProfileV1,
    models::routing_policy::RoutingPolicyConfigV2,
    persistence::stores::routing_generation_store::RoutingGenerationStore,
    persistence::stores::routing_health_verdict_store::RoutingHealthVerdictStore,
    persistence::stores::routing_quality_store::RoutingQualityStore,
    persistence::{stores::operational_facts::OperationalFactStore, ReadSession},
};
use sha2::{Digest, Sha256};

use super::{
    reader::{OperationalFactReadError, OperationalFactReader, OperationalFactSource},
    OperationalFactReadOptions,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum PlanningSnapshotBuildError {
    #[error("operational facts unavailable: {0}")]
    Facts(#[from] OperationalFactReadError),
    #[error("planning snapshot is invalid: {0}")]
    Invalid(&'static str),
    #[error("routing candidate count {actual} exceeds system limit {limit}")]
    CandidateLimitExceeded { actual: usize, limit: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanningCandidateEligibility {
    AdmittedForScoring,
    Excluded,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-probe-discovery; owner=application/operational_facts; remove_when=all probe discovery callers are removed from compatibility planning"
        )
    )]
    ProbeDiscoveryOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanningCandidateSet {
    NotApplicable,
    WithinLimit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanningCandidateAssessment {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) credential_revision: i64,
    pub(crate) account_revision: i64,
    pub(crate) group_revision: Option<i64>,
    pub(crate) snapshot_id: String,
    pub(crate) durable_revision: u64,
    pub(crate) request_context_fingerprint: String,
    pub(crate) eligibility: PlanningCandidateEligibility,
    pub(crate) candidate_set: PlanningCandidateSet,
    pub(crate) primary_reason: Option<String>,
    pub(crate) secondary_reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlanningBuildResult {
    pub(crate) snapshot: PlanningSnapshot,
    pub(crate) assessments: Vec<PlanningCandidateAssessment>,
}

/// Builds the immutable durable half of a routing plan from one caller-owned
/// read transaction. Runtime capacity and circuit state are supplied explicitly
/// as an overlay so this builder never opens a second transaction.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PlanningSnapshotBuilder;

impl PlanningSnapshotBuilder {
    pub(crate) async fn build_with_assessments(
        &self,
        read: &mut ReadSession,
        options: &OperationalFactReadOptions,
        policy: RoutingPolicyConfigV2,
        routing_policy_revision: u64,
        attempt_budget: AttemptBudgetProfileV1,
        quality_config: crate::application::quality_projection::QualityProjectionConfig,
        profile: DispatchAlgorithmProfile,
        runtime: RuntimeOverlaySnapshot,
        request: &RouteRequestFacts,
    ) -> Result<PlanningBuildResult, PlanningSnapshotBuildError> {
        let reader = OperationalFactReader::new(OperationalFactStore);
        let facts = reader.load_bundle(read, options).await?;
        let generation_registry = RoutingGenerationStore
            .load_registry_snapshot(read.connection())
            .await
            .map_err(|error| {
                PlanningSnapshotBuildError::Facts(OperationalFactReadError::Source(
                    error.to_string(),
                ))
            })?;
        // Capture mapping inputs once.  Every subject and assessment in this
        // build must use the same compiled revision fence.
        let mapping_snapshot = crate::application::model_mapping::current_snapshot();
        let mapping_configuration = &mapping_snapshot.configuration;
        let mapping_facts = mapping_facts(request);
        let request_context_fingerprint = request_context_fingerprint(
            request,
            routing_policy_revision,
            mapping_snapshot.revision,
        );
        let capability_subjects = capability_subjects_for_planning(
            &facts,
            request,
            mapping_configuration,
            &mapping_facts,
        );
        let mut unsupported_models = std::collections::BTreeSet::new();
        for subjects in capability_subjects.chunks(4_096) {
            unsupported_models.extend(
                RoutingHealthVerdictStore
                    .load_unsupported_model_batch(read.connection(), subjects)
                    .await
                    .map_err(|error| {
                        PlanningSnapshotBuildError::Facts(OperationalFactReadError::Source(
                            error.to_string(),
                        ))
                    })?,
            );
        }
        let active_quality_generation_id = generation_registry
            .active
            .as_ref()
            .map(|generation| generation.quality_generation_id.as_str());
        let durable_revision = [
            facts.version_vector().max_station_revision(),
            facts.version_vector().max_key_revision(),
            facts.version_vector().max_settings_revision(),
            facts.version_vector().max_alias_revision(),
            request
                .model_mapping_revision()
                .unwrap_or(mapping_configuration.mapping_revision.min(i64::MAX as u64) as i64),
        ]
        .into_iter()
        .max()
        .filter(|revision| *revision > 0)
        .ok_or(PlanningSnapshotBuildError::Invalid("revision_unavailable"))?
            as u64;
        // Counts are evaluated from the complete configured set. The source
        // intentionally has no candidate LIMIT: the system cap belongs after
        // model/capability and static lifecycle evaluation.
        let configured_key_count = facts.candidates().len();
        let mut capability_match_count = 0_usize;
        let mut candidate_cap_count = 0_usize;
        let resolved_model = request
            .requested_model()
            .map(|model| (model.trim().to_string(), 1_i64));
        let mut mapping_fallback_trigger = None;
        let mut assessments = Vec::with_capacity(facts.candidates().len());
        let mut eligible_candidates = Vec::with_capacity(facts.candidates().len());
        for candidate in facts.candidates() {
            let resolved = resolve_candidate_mapping(
                mapping_configuration,
                &mapping_facts,
                candidate,
                request,
            );
            let (candidate_variants, fallback_trigger) = match resolved {
                    Ok(value) => value,
                    Err(crate::application::model_mapping::ModelMappingResolutionError::ProfileHasNoOffering)
                    | Err(crate::application::model_mapping::ModelMappingResolutionError::NoResolvedTargets) => {
                        assessments.push(candidate_assessment(
                            candidate,
                            facts.snapshot_id().as_str(),
                            durable_revision,
                            &request_context_fingerprint,
                            PlanningCandidateEligibility::Excluded,
                            PlanningCandidateSet::NotApplicable,
                            Some("model_mapping_no_target"),
                            Vec::new(),
                        ));
                        continue;
                    }
                    Err(error) => {
                        assessments.push(candidate_assessment(
                            candidate,
                            facts.snapshot_id().as_str(),
                            durable_revision,
                            &request_context_fingerprint,
                            PlanningCandidateEligibility::Excluded,
                            PlanningCandidateSet::NotApplicable,
                            Some(model_mapping_reason(&error)),
                            Vec::new(),
                        ));
                        continue;
                    }
                };
            if mapping_fallback_trigger.is_none() {
                mapping_fallback_trigger = fallback_trigger;
            }
            let mapped_variants = candidate_variants;
            let candidate_variants = if unsupported_models.is_empty() {
                mapped_variants.clone()
            } else {
                mapped_variants
                    .iter()
                    .cloned()
                    .filter(|variant| {
                        !unsupported_models.contains(&(
                            candidate.station_key_id().as_str().to_string(),
                            variant.upstream_model.clone(),
                            candidate.credential().record_revision().get(),
                            candidate.endpoint().endpoint_ref().revision().get(),
                            1_i64,
                        ))
                    })
                    .collect::<Vec<_>>()
            };
            if !mapped_variants.is_empty() && candidate_variants.is_empty() {
                assessments.push(candidate_assessment(
                    candidate,
                    facts.snapshot_id().as_str(),
                    durable_revision,
                    &request_context_fingerprint,
                    PlanningCandidateEligibility::Excluded,
                    PlanningCandidateSet::NotApplicable,
                    Some("unsupported_model"),
                    Vec::new(),
                ));
                continue;
            }
            let capability_rejection_reason =
                capability_rejection_reason_for_variants(candidate, request, &candidate_variants);
            let candidate_variants = candidate_variants
                .into_iter()
                .filter(|variant| {
                    candidate_capability_rejection_reason(
                        candidate,
                        request,
                        Some(variant.upstream_model.as_str()),
                    )
                    .is_none()
                })
                .collect::<Vec<_>>();
            let capability_matches =
                candidate_matches_request_capabilities(candidate, request, &candidate_variants);
            if !capability_matches {
                assessments.push(candidate_assessment(
                    candidate,
                    facts.snapshot_id().as_str(),
                    durable_revision,
                    &request_context_fingerprint,
                    PlanningCandidateEligibility::Excluded,
                    PlanningCandidateSet::NotApplicable,
                    Some(capability_rejection_reason.unwrap_or("capability_rejected")),
                    Vec::new(),
                ));
                continue;
            }
            capability_match_count = capability_match_count.saturating_add(1);
            if candidate_cap_eligible(candidate) {
                candidate_cap_count = candidate_cap_count.saturating_add(1);
            }

            // Preserve the first concrete post-cap hard-gate reason before
            // filtering variants. Otherwise a candidate whose every target is
            // rejected by a user filter would look like a capability miss.
            let hard_rejection_reason = hard_rejection_reason_for_variants(
                candidate,
                request,
                &policy,
                &candidate_variants,
            );
            let candidate_variants = candidate_variants
                .into_iter()
                .filter(|variant| {
                    candidate_hard_eligible(
                        candidate,
                        request,
                        &policy,
                        Some(variant.upstream_model.as_str()),
                    )
                })
                .collect::<Vec<_>>();
            if request.requested_model().is_some()
                && candidate_variants.is_empty()
                && !matches!(request.route_kind(), RouteKind::ModelCatalog)
            {
                assessments.push(candidate_assessment(
                    candidate,
                    facts.snapshot_id().as_str(),
                    durable_revision,
                    &request_context_fingerprint,
                    PlanningCandidateEligibility::Excluded,
                    PlanningCandidateSet::NotApplicable,
                    Some(hard_rejection_reason.unwrap_or("capability_rejected")),
                    Vec::new(),
                ));
                continue;
            }
            let resolved_model_for_candidate = candidate_variants
                .first()
                .map(|variant| (variant.upstream_model.clone(), 1_i64));
            let resolved_model = resolved_model_for_candidate.or_else(|| resolved_model.clone());
            let candidate_model_for_gates = candidate_variants
                .first()
                .map(|variant| variant.upstream_model.clone())
                .or_else(|| request.requested_model().map(ToOwned::to_owned));
            let hard_rejection_reason = candidate_hard_rejection_reason(
                candidate,
                request,
                &policy,
                candidate_model_for_gates.as_deref(),
            );
            let hard_eligible = hard_rejection_reason.is_none();
            let candidate_snapshot = CandidateSnapshot {
                station_key_id: candidate.station_key_id().as_str().to_string(),
                station_id: candidate.station_id().as_str().to_string(),
                endpoint_revision: candidate.endpoint().endpoint_ref().revision().get(),
                credential_revision: candidate.credential().record_revision().get(),
                account_revision: candidate.account_record_revision().get(),
                group_binding_id: candidate.group_binding_id().map(ToString::to_string),
                group_revision: candidate
                    .group_record_revision()
                    .map(|revision| revision.get()),
                resolved_upstream_model: resolved_model.as_ref().map(|(model, _)| model.clone()),
                model_alias_revision: resolved_model
                    .as_ref()
                    .map(|(_, revision)| *revision)
                    .unwrap_or(1),
                model_variants: candidate_variants.clone(),
                credential_available: candidate.credential().available(),
                hard_eligible,
                backup_only: candidate.backup_only(),
                depleted: candidate_is_depleted(candidate),
                capability_basis_points: 10_000,
                quality_available: true,
                reliability_basis_points: quality_config.optimistic_reliability_basis_points,
                responsiveness_basis_points: responsiveness_score(
                    Some(quality_config.optimistic_latency_ms),
                    profile.latency_cap_ms,
                )
                .get(),
                cost_basis_points: None,
                pricing: RoutePlanPricingSnapshot::unpriced("pricing_context_missing"),
                preference_basis_points: preference_score(candidate, request),
                failure_domains: vec![
                    format!("station:{}", candidate.station_id().as_str()),
                    format!("key:{}", candidate.station_key_id().as_str()),
                ],
            };
            let primary_reason = hard_rejection_reason;
            let eligibility = if candidate_snapshot.hard_eligible {
                PlanningCandidateEligibility::AdmittedForScoring
            } else {
                PlanningCandidateEligibility::Excluded
            };
            if matches!(
                eligibility,
                PlanningCandidateEligibility::AdmittedForScoring
            ) {
                eligible_candidates.push((candidate_snapshot, eligibility, primary_reason));
            } else {
                assessments.push(candidate_assessment(
                    candidate,
                    facts.snapshot_id().as_str(),
                    durable_revision,
                    &request_context_fingerprint,
                    eligibility,
                    PlanningCandidateSet::NotApplicable,
                    primary_reason,
                    Vec::new(),
                ));
            }
        }
        if candidate_cap_count > options.candidate_limit() {
            return Err(PlanningSnapshotBuildError::CandidateLimitExceeded {
                actual: candidate_cap_count,
                limit: options.candidate_limit(),
            });
        }

        // Only keys which can reach scoring need quality projections. Loading
        // this bounded set after the cap check prevents pre-filter database
        // order from silently dropping the quality of a later eligible key.
        let scopes = eligible_candidates
            .iter()
            .map(|(candidate, _, _)| format!("station_key:{}", candidate.station_key_id))
            .collect::<Vec<_>>();
        let quality_read = RoutingQualityStore
            .load_planning_read(
                read.connection(),
                active_quality_generation_id,
                &scopes,
                chrono::Utc::now().timestamp_millis().max(0),
            )
            .await
            .map_err(|error| {
                PlanningSnapshotBuildError::Facts(OperationalFactReadError::Source(
                    error.to_string(),
                ))
            })?;
        for (candidate, _, _) in &mut eligible_candidates {
            let quality_scope = format!("station_key:{}", candidate.station_key_id);
            candidate.quality_available = quality_read.quality_available
                && !quality_read.unavailable_scopes.contains(&quality_scope);
            candidate.reliability_basis_points = quality_read
                .axes
                .get(&quality_scope)
                .and_then(|axes| axes.get("reliability").copied())
                .unwrap_or(quality_config.optimistic_reliability_basis_points);
            candidate.responsiveness_basis_points = quality_read
                .axes
                .get(&quality_scope)
                .and_then(|axes| axes.get("latency").copied())
                .unwrap_or_else(|| {
                    responsiveness_score(
                        Some(quality_config.optimistic_latency_ms),
                        profile.latency_cap_ms,
                    )
                    .get()
                });
        }

        let mut scoring_candidates = Vec::new();
        for (candidate, eligibility, primary_reason) in eligible_candidates {
            match eligibility {
                PlanningCandidateEligibility::AdmittedForScoring => {
                    scoring_candidates.push((candidate, primary_reason));
                }
                PlanningCandidateEligibility::ProbeDiscoveryOnly => unreachable!(
                    "legacy error-rate probe discovery is not a production planner input"
                ),
                PlanningCandidateEligibility::Excluded => unreachable!(
                    "excluded candidates are assessed before entering the eligible set"
                ),
            }
        }
        let mut candidates = Vec::with_capacity(scoring_candidates.len());
        for (candidate, primary_reason) in scoring_candidates {
            assessments.push(candidate_assessment_from_snapshot(
                &candidate,
                facts.snapshot_id().as_str(),
                durable_revision,
                &request_context_fingerprint,
                PlanningCandidateEligibility::AdmittedForScoring,
                PlanningCandidateSet::WithinLimit,
                primary_reason,
                Vec::new(),
            ));
            candidates.push(candidate);
        }
        let snapshot = PlanningSnapshot {
            snapshot_id: facts.snapshot_id().as_str().to_string(),
            durable_revision,
            configured_key_count,
            capability_match_count,
            candidate_cap_count,
            routing_runtime_generation_id: generation_registry.marker.active_runtime_generation_id,
            routing_generation_fence_revision: generation_registry.marker.fence_revision,
            routing_policy_revision,
            routing_quality_revision: quality_read.quality_revision,
            routing_health_revision: quality_read.health_revision,
            quality_projection_backlog: quality_read.projection_backlog,
            quality_projection_lag_seconds: quality_read.projection_lag_seconds,
            quality_stale: quality_read.quality_stale,
            policy,
            attempt_budget,
            profile,
            candidates,
            model_fallback_trigger: mapping_fallback_trigger,
            runtime,
        };
        snapshot
            .validate()
            .map_err(PlanningSnapshotBuildError::Invalid)?;
        Ok(PlanningBuildResult {
            snapshot,
            assessments,
        })
    }
}

fn model_mapping_reason(
    error: &crate::application::model_mapping::ModelMappingResolutionError,
) -> &'static str {
    use crate::application::model_mapping::ModelMappingResolutionError as E;
    match error {
        E::InvalidModelName => "model_mapping_invalid_model",
        E::TargetRequiresCandidateContext => "model_mapping_context_required",
        E::ProfileNotFound => "model_mapping_profile_not_found",
        E::ProfileHasNoOffering | E::NoResolvedTargets => "model_mapping_no_target",
    }
}

fn request_context_fingerprint(
    request: &RouteRequestFacts,
    routing_policy_revision: u64,
    mapping_revision: u64,
) -> String {
    let material = format!(
        "route={:?}|model={:?}|mapping_model={:?}|endpoint={:?}|stream={}|tools={}|vision={}|reasoning={}|group={:?}|required_group={:?}|tags={:?}|policy={}|mapping={}",
        request.route_kind(),
        request.requested_model(),
        request.mapping_requested_model(),
        request.mapping_endpoint(),
        request.stream(),
        request.uses_tools(),
        request.uses_vision(),
        request.uses_reasoning(),
        request.group_filter_mode(),
        request.required_group_stable_key(),
        request.required_tags(),
        routing_policy_revision,
        mapping_revision,
    );
    format!("sha256:{:x}", Sha256::digest(material.as_bytes()))
}

fn candidate_assessment(
    candidate: &super::assembler::OperationalCandidateFact,
    snapshot_id: &str,
    durable_revision: u64,
    request_context_fingerprint: &str,
    eligibility: PlanningCandidateEligibility,
    candidate_set: PlanningCandidateSet,
    primary_reason: Option<&str>,
    secondary_reason_codes: Vec<String>,
) -> PlanningCandidateAssessment {
    PlanningCandidateAssessment {
        station_key_id: candidate.station_key_id().as_str().to_string(),
        station_id: candidate.station_id().as_str().to_string(),
        endpoint_revision: candidate.endpoint().endpoint_ref().revision().get(),
        credential_revision: candidate.credential().record_revision().get(),
        account_revision: candidate.account_record_revision().get(),
        group_revision: candidate
            .group_record_revision()
            .map(|revision| revision.get()),
        snapshot_id: snapshot_id.to_string(),
        durable_revision,
        request_context_fingerprint: request_context_fingerprint.to_string(),
        eligibility,
        candidate_set,
        primary_reason: primary_reason.map(ToOwned::to_owned),
        secondary_reason_codes,
    }
}

fn candidate_assessment_from_snapshot(
    candidate: &CandidateSnapshot,
    snapshot_id: &str,
    durable_revision: u64,
    request_context_fingerprint: &str,
    eligibility: PlanningCandidateEligibility,
    candidate_set: PlanningCandidateSet,
    primary_reason: Option<&str>,
    secondary_reason_codes: Vec<String>,
) -> PlanningCandidateAssessment {
    PlanningCandidateAssessment {
        station_key_id: candidate.station_key_id.clone(),
        station_id: candidate.station_id.clone(),
        endpoint_revision: candidate.endpoint_revision,
        credential_revision: candidate.credential_revision,
        account_revision: candidate.account_revision,
        group_revision: candidate.group_revision,
        snapshot_id: snapshot_id.to_string(),
        durable_revision,
        request_context_fingerprint: request_context_fingerprint.to_string(),
        eligibility,
        candidate_set,
        primary_reason: primary_reason.map(ToOwned::to_owned),
        secondary_reason_codes,
    }
}

fn mapping_facts(request: &RouteRequestFacts) -> crate::models::model_mapping::ModelRequestFacts {
    crate::models::model_mapping::ModelRequestFacts {
        requested_model: request
            .mapping_requested_model()
            .or_else(|| request.requested_model())
            .map(ToOwned::to_owned),
        endpoint: request
            .mapping_endpoint()
            .unwrap_or(crate::models::model_mapping::EndpointKind::Responses),
        stream: request.stream(),
        tools: request.uses_tools(),
        vision: request.uses_vision(),
        reasoning: request.uses_reasoning(),
    }
}

fn resolve_candidate_mapping(
    configuration: &crate::application::model_mapping::CompiledModelMappingConfiguration,
    facts: &crate::models::model_mapping::ModelRequestFacts,
    candidate: &super::assembler::OperationalCandidateFact,
    request: &RouteRequestFacts,
) -> Result<
    (
        Vec<CandidateModelVariant>,
        Option<crate::models::model_mapping::FallbackTrigger>,
    ),
    crate::application::model_mapping::ModelMappingResolutionError,
> {
    let endpoint = facts.endpoint;
    let context = CandidateResolutionContext {
        station_key_id: candidate.station_key_id().as_str(),
        station_id: candidate.station_id().as_str(),
        endpoint,
        credential_revision: candidate.credential().record_revision().get() as u64,
        endpoint_revision: candidate.endpoint().endpoint_ref().revision().get() as u64,
    };
    let plan = resolve_for_candidate(configuration, facts, context)?;
    let fallback_trigger = plan.fallback_trigger;
    let variants = candidate_variants(&plan, context);
    // A mapping bypass or missing model has no target variant. Preserve the
    // legacy snapshot shape so catalog/read-model callers remain eligible.
    if variants.is_empty() && request.requested_model().is_some() {
        return Err(
            crate::application::model_mapping::ModelMappingResolutionError::NoResolvedTargets,
        );
    }
    Ok((variants, fallback_trigger))
}

fn capability_subjects_for_planning(
    facts: &super::assembler::OperationalFactBundle,
    request: &RouteRequestFacts,
    configuration: &crate::application::model_mapping::CompiledModelMappingConfiguration,
    mapping_facts: &crate::models::model_mapping::ModelRequestFacts,
) -> Vec<(String, String, i64, i64, i64)> {
    // The fifth tuple slot is a legacy provenance column. Native capability
    // identity is the station key + upstream model + execution revisions, so
    // mapping revisions must never partition capability facts.
    let native_identity_revision = 1_i64;
    let mut subjects = Vec::new();
    for candidate in facts.candidates() {
        let variants = resolve_candidate_mapping(configuration, mapping_facts, candidate, request)
            .ok()
            .map(|(variants, _)| variants)
            .unwrap_or_default();
        let models = if variants.is_empty() {
            request
                .requested_model()
                .map(|model| vec![model.to_string()])
                .unwrap_or_default()
        } else {
            variants
                .into_iter()
                .map(|variant| variant.upstream_model)
                .collect()
        };
        for model in models {
            subjects.push((
                candidate.station_key_id().as_str().to_string(),
                model,
                candidate.credential().record_revision().get(),
                candidate.endpoint().endpoint_ref().revision().get(),
                native_identity_revision,
            ));
        }
    }
    subjects
}

fn candidate_is_depleted(candidate: &super::assembler::OperationalCandidateFact) -> bool {
    crate::models::routing::balance_is_depleted(
        candidate.balance_value(),
        candidate.balance_status(),
    )
}

fn candidate_cap_eligible(candidate: &super::assembler::OperationalCandidateFact) -> bool {
    // Record/endpoint lifecycle revisions have already been validated by the
    // fact assembler. Key enable/schedulable is a user gate and intentionally
    // remains outside this count so terminal classification has a stable base.
    candidate.station_enabled() && candidate.credential().available()
}

fn capability_rejection_reason_for_variants(
    candidate: &super::assembler::OperationalCandidateFact,
    request: &RouteRequestFacts,
    variants: &[CandidateModelVariant],
) -> Option<&'static str> {
    if variants.is_empty() {
        return candidate_capability_rejection_reason(candidate, request, None);
    }
    variants.iter().find_map(|variant| {
        candidate_capability_rejection_reason(
            candidate,
            request,
            Some(variant.upstream_model.as_str()),
        )
    })
}

fn candidate_matches_request_capabilities(
    candidate: &super::assembler::OperationalCandidateFact,
    request: &RouteRequestFacts,
    variants: &[CandidateModelVariant],
) -> bool {
    // The workspace baseline intentionally has no model. An empty variant set
    // therefore means "not model-scoped", not "model unsupported". Concrete
    // inference requests still require at least one mapped, capable variant.
    let is_model_less_baseline =
        request.requested_model().is_none() && request.mapping_requested_model().is_none();
    if is_model_less_baseline || matches!(request.route_kind(), RouteKind::ModelCatalog) {
        candidate_capability_rejection_reason(candidate, request, None).is_none()
    } else {
        !variants.is_empty()
    }
}

fn candidate_capability_rejection_reason(
    candidate: &super::assembler::OperationalCandidateFact,
    request: &RouteRequestFacts,
    model_override: Option<&str>,
) -> Option<&'static str> {
    let protocol_ok = candidate.supports_chat_completions() || candidate.supports_responses();
    let model_ok = model_override.is_none_or(|model| {
        !candidate
            .model_blocklist()
            .iter()
            .any(|blocked| blocked.eq_ignore_ascii_case(model))
            && (candidate.model_allowlist().is_empty()
                || candidate
                    .model_allowlist()
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(model)))
    });
    let features_ok = (!request.stream() || candidate.supports_stream())
        && (!request.uses_tools() || candidate.supports_tools())
        && (!request.uses_vision() || candidate.supports_vision())
        && (!request.uses_reasoning() || candidate.supports_reasoning());
    if protocol_ok && model_ok && features_ok {
        None
    } else {
        Some("capability_rejected")
    }
}

fn candidate_hard_eligible(
    candidate: &super::assembler::OperationalCandidateFact,
    request: &RouteRequestFacts,
    policy: &RoutingPolicyConfigV2,
    model_override: Option<&str>,
) -> bool {
    candidate_hard_rejection_reason(candidate, request, policy, model_override).is_none()
}

fn hard_rejection_reason_for_variants(
    candidate: &super::assembler::OperationalCandidateFact,
    request: &RouteRequestFacts,
    policy: &RoutingPolicyConfigV2,
    variants: &[CandidateModelVariant],
) -> Option<&'static str> {
    variants.iter().find_map(|variant| {
        candidate_hard_rejection_reason(
            candidate,
            request,
            policy,
            Some(variant.upstream_model.as_str()),
        )
    })
}

fn candidate_hard_rejection_reason(
    candidate: &super::assembler::OperationalCandidateFact,
    request: &RouteRequestFacts,
    policy: &RoutingPolicyConfigV2,
    model_override: Option<&str>,
) -> Option<&'static str> {
    if !candidate.station_enabled()
        || !candidate.key_enabled()
        || !candidate.schedulable()
        || !candidate.credential().available()
    {
        return if !candidate.station_enabled() {
            Some("station_disabled")
        } else if !candidate.key_enabled() {
            Some("key_disabled")
        } else if !candidate.schedulable() {
            Some("candidate_unschedulable")
        } else {
            Some("credential_unavailable")
        };
    }
    if let Some(reason) = candidate_capability_rejection_reason(candidate, request, model_override)
    {
        return Some(reason);
    }
    let tags_ok = request.required_tags().iter().all(|tag| {
        candidate
            .routing_tags()
            .iter()
            .any(|candidate_tag| candidate_tag.eq_ignore_ascii_case(tag))
    });
    let group_ok = candidate_matches_group_scope(candidate, request);
    let depleted = candidate_is_depleted(candidate);
    if !group_ok {
        return Some("group_mismatch");
    }
    let multiplier_ceiling_ok = match request.route_kind() {
        RouteKind::Inference => request
            .max_rate_multiplier()
            .zip(
                crate::application::operational_facts::pricing_projector::effective_rate_multiplier(
                    candidate.station_native_multiplier(),
                    candidate.credit_per_cny().unwrap_or(1.0),
                ),
            )
            .is_none_or(|(ceiling, multiplier)| multiplier <= ceiling),
        RouteKind::ModelCatalog => true,
    };
    if !multiplier_ceiling_ok {
        return Some("multiplier_ceiling");
    }
    if !tags_ok {
        return Some("tag_mismatch");
    }
    if depleted && !policy.allow_depleted_fallback {
        return Some("balance_depleted");
    }
    None
}

fn candidate_matches_group_scope(
    candidate: &super::assembler::OperationalCandidateFact,
    request: &RouteRequestFacts,
) -> bool {
    match request.group_filter_mode() {
        GroupFilterMode::Any => true,
        GroupFilterMode::UngroupedOnly => {
            candidate.group_binding_id().is_none() && candidate.group_id_hash().is_none()
        }
        GroupFilterMode::Required => {
            let Some(required) = request.required_group_stable_key() else {
                return false;
            };
            let binding_matches = required
                .strip_prefix("binding:")
                .is_some_and(|binding| candidate.group_binding_id() == Some(binding));
            let group_id_matches = required
                .strip_prefix("group-id:")
                .is_some_and(|group_id| candidate.group_id_hash() == Some(group_id));
            let category_matches = required
                .strip_prefix("group-type:")
                .is_some_and(|category| {
                    candidate
                        .group_category()
                        .is_some_and(|actual| actual.eq_ignore_ascii_case(category))
                });
            binding_matches || group_id_matches || category_matches
        }
    }
}

fn preference_score(
    candidate: &super::assembler::OperationalCandidateFact,
    request: &RouteRequestFacts,
) -> u16 {
    if let Some(model) = request.requested_model() {
        if candidate
            .preferred_models()
            .iter()
            .any(|preferred| preferred.eq_ignore_ascii_case(model))
        {
            return 10_000;
        }
    }
    let priority = candidate.priority().clamp(0, 10_000) as u16;
    10_000_u16.saturating_sub(priority)
}

// Keep the source bound visible at the composition boundary. This prevents a
// future caller from silently replacing the transactional fact source with a
// page-specific query facade.
fn _source_contract<S: OperationalFactSource>() {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::error_rate_protection::admission_scope;

    #[test]
    fn builder_type_is_bound_to_the_operational_source() {
        _source_contract::<OperationalFactStore>();
    }

    #[test]
    fn group_scope_matches_binding_id_group_id_category_and_ungrouped_candidates() {
        // Candidate construction belongs to the assembler; test the public
        // hard-gate semantics through the pure scope matcher with a compact
        // fixture assembled from the production field layout.
        let candidate = test_candidate(Some("binding-a"), Some("group-a"), Some("gpt"));
        assert!(candidate_matches_group_scope(
            &candidate,
            &test_request(GroupFilterMode::Required, Some("binding:binding-a")),
        ));
        assert!(candidate_matches_group_scope(
            &candidate,
            &test_request(GroupFilterMode::Required, Some("group-id:group-a")),
        ));
        assert!(candidate_matches_group_scope(
            &candidate,
            &test_request(GroupFilterMode::Required, Some("group-type:gpt")),
        ));
        assert!(!candidate_matches_group_scope(
            &candidate,
            &test_request(GroupFilterMode::UngroupedOnly, None),
        ));
        let ungrouped = test_candidate(None, None, None);
        assert!(candidate_matches_group_scope(
            &ungrouped,
            &test_request(GroupFilterMode::UngroupedOnly, None),
        ));
    }

    #[test]
    fn legacy_key_only_health_is_not_a_planner_authority_after_scoped_cutover() {
        let mut candidate = test_candidate(None, None, None);
        candidate.set_durable_health_for_planning_test(Some("1001"), None);
        assert!(candidate_hard_eligible(
            &candidate,
            &test_request(GroupFilterMode::Any, None),
            &RoutingPolicyConfigV2::default(),
            Some("gpt-4.1"),
        ));
        candidate.set_durable_health_for_planning_test(
            None,
            Some("auth_error: upstream returned HTTP 401"),
        );
        assert!(candidate_hard_eligible(
            &candidate,
            &test_request(GroupFilterMode::Any, None),
            &RoutingPolicyConfigV2::default(),
            Some("gpt-4.1"),
        ));
    }

    #[test]
    fn model_less_workspace_baseline_does_not_require_a_model_variant() {
        let candidate = test_candidate(None, None, None);
        let request = test_request_without_model();

        assert!(candidate_matches_request_capabilities(
            &candidate,
            &request,
            &[],
        ));
    }

    #[test]
    fn model_less_workspace_baseline_still_requires_a_supported_protocol() {
        let mut candidate = test_candidate(None, None, None);
        candidate.set_protocol_capabilities_for_planning_test(false, false);
        let request = test_request_without_model();

        assert!(!candidate_matches_request_capabilities(
            &candidate,
            &request,
            &[],
        ));
    }

    #[test]
    fn concrete_model_request_still_requires_a_resolved_capable_variant() {
        let candidate = test_candidate(None, None, None);
        let request = test_request(GroupFilterMode::Any, None);

        assert!(!candidate_matches_request_capabilities(
            &candidate,
            &request,
            &[],
        ));
    }

    #[test]
    fn original_mapping_model_also_requires_a_resolved_capable_variant() {
        let candidate = test_candidate(None, None, None);
        let request =
            test_request_without_model().with_mapping_requested_model(Some("gpt-4.1".to_string()));

        assert!(!candidate_matches_request_capabilities(
            &candidate,
            &request,
            &[],
        ));
    }

    #[test]
    fn multiplier_ceiling_uses_the_normalized_station_multiplier() {
        let mut candidate = test_candidate(None, None, None);
        candidate.set_multiplier_for_planning_test(Some(2.0), Some(27.0));
        let policy = RoutingPolicyConfigV2::default();
        let admitted =
            test_request(GroupFilterMode::Any, None).with_max_rate_multiplier_for_test(Some(0.08));

        assert!(candidate_hard_eligible(
            &candidate,
            &admitted,
            &policy,
            Some("gpt-4.1"),
        ));

        let rejected =
            test_request(GroupFilterMode::Any, None).with_max_rate_multiplier_for_test(Some(0.07));
        assert_eq!(
            candidate_hard_rejection_reason(&candidate, &rejected, &policy, Some("gpt-4.1")),
            Some("multiplier_ceiling"),
        );
    }

    #[test]
    fn invalid_exchange_rate_falls_back_to_one_for_multiplier_ceiling() {
        let mut candidate = test_candidate(None, None, None);
        candidate.set_multiplier_for_planning_test(Some(2.0), Some(0.0));
        let request =
            test_request(GroupFilterMode::Any, None).with_max_rate_multiplier_for_test(Some(1.0));

        assert_eq!(
            candidate_hard_rejection_reason(
                &candidate,
                &request,
                &RoutingPolicyConfigV2::default(),
                Some("gpt-4.1"),
            ),
            Some("multiplier_ceiling"),
        );
    }

    #[test]
    fn positive_low_balance_warning_does_not_reject_candidate() {
        let mut candidate = test_candidate(None, None, None);
        candidate.set_balance_for_planning_test(Some(4.71), Some("low"));

        assert!(!candidate_is_depleted(&candidate));
        assert!(candidate_hard_eligible(
            &candidate,
            &test_request(GroupFilterMode::Any, None),
            &RoutingPolicyConfigV2::default(),
            Some("gpt-4.1"),
        ));
    }

    #[test]
    fn explicit_depleted_balance_status_rejects_candidate() {
        let mut candidate = test_candidate(None, None, None);
        // Text-only exhaustion is authoritative when the provider did not
        // return a numeric balance. A positive numeric balance wins over a
        // stale/conflicting depleted label.
        candidate.set_balance_for_planning_test(None, Some("depleted"));

        assert_eq!(
            candidate_hard_rejection_reason(
                &candidate,
                &test_request(GroupFilterMode::Any, None),
                &RoutingPolicyConfigV2::default(),
                Some("gpt-4.1"),
            ),
            Some("balance_depleted"),
        );
    }

    fn test_request(
        group_filter_mode: GroupFilterMode,
        required_group_stable_key: Option<&str>,
    ) -> RouteRequestFacts {
        crate::application::routing_engine::request::RouteRequestClassifier::classify(
            crate::application::routing_engine::request::CanonicalRouteRequest {
                route_kind: RouteKind::Inference,
                requested_model: Some("gpt-4.1".to_string()),
                stream: false,
                uses_tools: false,
                uses_vision: false,
                uses_reasoning: false,
                untrusted_headers: Vec::new(),
            },
            crate::application::routing_engine::request::ValidatedLocalRouteSettings {
                ordering_profile:
                    crate::application::routing_engine::request::OrderingProfile::PriorityFirst,
                max_rate_multiplier: None,
                group_filter_mode,
                required_group_stable_key: required_group_stable_key.map(ToString::to_string),
                preferred_models: Vec::new(),
                required_tags: Vec::new(),
                allow_depleted_fallback: false,
                affinity_enabled: false,
            },
            1_000,
        )
    }

    fn test_request_without_model() -> RouteRequestFacts {
        crate::application::routing_engine::request::RouteRequestClassifier::classify(
            crate::application::routing_engine::request::CanonicalRouteRequest {
                route_kind: RouteKind::Inference,
                requested_model: None,
                stream: false,
                uses_tools: false,
                uses_vision: false,
                uses_reasoning: false,
                untrusted_headers: Vec::new(),
            },
            crate::application::routing_engine::request::ValidatedLocalRouteSettings {
                ordering_profile:
                    crate::application::routing_engine::request::OrderingProfile::PriorityFirst,
                max_rate_multiplier: None,
                group_filter_mode: GroupFilterMode::Any,
                required_group_stable_key: None,
                preferred_models: Vec::new(),
                required_tags: Vec::new(),
                allow_depleted_fallback: false,
                affinity_enabled: false,
            },
            1_000,
        )
    }

    fn test_candidate(
        group_binding_id: Option<&str>,
        group_id_hash: Option<&str>,
        group_category: Option<&str>,
    ) -> super::super::assembler::OperationalCandidateFact {
        super::super::assembler::OperationalCandidateFact::for_planning_test(
            group_binding_id,
            group_id_hash,
            group_category,
        )
    }
}
