use crate::{
    application::error_rate_protection::{
        candidate_health_scopes, scoped_admission_verdict_for_probe_candidate,
        scoped_admission_verdict_with_probe, ErrorRateAdmissionConfigV1,
    },
    application::health_protection::{HealthProbeAdmissionMode, HealthProtectionStatus},
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
        factors::{reliability_posterior, responsiveness_score},
        failure_domains::ProviderCapacityDomain,
        request::{GroupFilterMode, RouteKind, RouteRequestFacts},
    },
    application::routing_policy::AttemptBudgetProfileV1,
    models::routing_policy::RoutingPolicyConfigV2,
    persistence::stores::routing_health_verdict_store::{
        DurableHealthVerdict, FailureDimension, RoutingHealthVerdictStore, ScopedHealthSubject,
    },
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanningCandidateEligibility {
    AdmittedForScoring,
    Excluded,
    ProbeDiscoveryOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanningCandidateSet {
    NotApplicable,
    WithinLimit,
    CappedByCandidateLimit,
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
        profile: DispatchAlgorithmProfile,
        runtime: RuntimeOverlaySnapshot,
        request: &RouteRequestFacts,
        error_rate_admission: ErrorRateAdmissionConfigV1,
        error_rate_statuses: &[HealthProtectionStatus],
        health_probe: Option<&crate::application::health_protection::HealthProtectionProbe>,
        health_probe_mode: HealthProbeAdmissionMode,
    ) -> Result<PlanningBuildResult, PlanningSnapshotBuildError> {
        let reader = OperationalFactReader::new(OperationalFactStore);
        let facts = reader.load_bundle(read, options).await?;
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
        let scoped_subjects =
            scoped_subjects_for_planning(&facts, request, &mapping_configuration, &mapping_facts)?;
        let scoped_verdicts = RoutingHealthVerdictStore
            .load_active_batch(read.connection(), &scoped_subjects)
            .await
            .map_err(|error| {
                PlanningSnapshotBuildError::Facts(OperationalFactReadError::Source(
                    error.to_string(),
                ))
            })?;
        let capability_subjects = capability_subjects_for_planning(
            &facts,
            request,
            mapping_configuration,
            &mapping_facts,
        );
        let unsupported_models = RoutingHealthVerdictStore
            .load_unsupported_model_batch(read.connection(), &capability_subjects)
            .await
            .map_err(|error| {
                PlanningSnapshotBuildError::Facts(OperationalFactReadError::Source(
                    error.to_string(),
                ))
            })?;
        let scopes = facts
            .candidates()
            .iter()
            .map(|candidate| format!("station_key:{}", candidate.station_key_id().as_str()))
            .collect::<Vec<_>>();
        let quality_axes = RoutingQualityStore
            .load_health_axes(read.connection(), &scopes)
            .await
            .map_err(|error| {
                PlanningSnapshotBuildError::Facts(OperationalFactReadError::Source(
                    error.to_string(),
                ))
            })?;
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
        // The raw query has a broader fixed upper bound to contain database
        // work. The policy is the actual planner limit, applied after hard
        // gates so an ineligible early row cannot starve a usable candidate.
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
            // Preserve the first concrete hard-gate reason before filtering
            // variants.  Otherwise a candidate whose every mapped target is
            // rejected (for example by the multiplier ceiling or balance
            // gate) would be reported as the generic capability rejection.
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
            let model_gate_failed = !mapped_variants.is_empty() && candidate_variants.is_empty();
            let candidate_variants = candidate_variants
                .into_iter()
                .filter(|variant| {
                    candidate_model_scoped_admitted(
                        candidate,
                        &variant.upstream_model,
                        &scoped_verdicts,
                    )
                })
                .collect::<Vec<_>>();
            let scoped_model_failed =
                !mapped_variants.is_empty() && !model_gate_failed && candidate_variants.is_empty();
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
                    Some(if scoped_model_failed {
                        "model_health_rejected"
                    } else {
                        hard_rejection_reason.unwrap_or("capability_rejected")
                    }),
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
            let base_hard_eligible =
                hard_eligible && candidate_scoped_admitted(candidate, &scoped_verdicts, false);
            let error_rate_admitted = error_rate_candidate_admitted(
                candidate,
                error_rate_admission.clone(),
                error_rate_statuses,
                health_probe,
            );
            // An expired durable Open entry may be retained only so the
            // execution coordinator can discover it and atomically reserve
            // a Half-Open lease. It must remain planner-ineligible until
            // the second snapshot carries that exact revision fence.
            let probe_discovery_candidate = hard_eligible
                && candidate_scoped_admitted(candidate, &scoped_verdicts, true)
                && !error_rate_admitted
                && health_probe.is_none()
                && health_probe_mode == HealthProbeAdmissionMode::Normal
                && error_rate_probe_discovery_allowed(
                    candidate,
                    error_rate_admission.clone(),
                    error_rate_statuses,
                );
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
                capacity_domain: resolved_model.as_ref().and_then(|(model, _)| {
                    ProviderCapacityDomain::from_trusted_identity(
                        candidate.capacity_provider_family()?,
                        model,
                        candidate.capacity_deployment_identity(),
                        candidate.capacity_region_identity(),
                    )
                    .map(|domain| domain.commitment())
                }),
                capacity_domain_revision: candidate
                    .capacity_domain_revision()
                    .map(|revision| revision.get()),
                credential_available: candidate.credential().available(),
                hard_eligible: base_hard_eligible && error_rate_admitted,
                backup_only: candidate.backup_only(),
                depleted: candidate_is_depleted(candidate),
                capability_basis_points: 10_000,
                // No observation remains a neutral prior, while projected
                // axes become live routing inputs when available.
                reliability_basis_points: quality_axes
                    .get(&format!(
                        "station_key:{}",
                        candidate.station_key_id().as_str()
                    ))
                    .and_then(|axes| axes.get("reliability").copied())
                    .unwrap_or_else(|| {
                        reliability_posterior(
                            candidate.success_count().max(0) as u32,
                            candidate.failure_count().max(0) as u32,
                            profile.reliability_prior_alpha,
                            profile.reliability_prior_beta,
                        )
                        .map(|estimate| estimate.value.get())
                        .unwrap_or(5_000)
                    }),
                responsiveness_basis_points: quality_axes
                    .get(&format!(
                        "station_key:{}",
                        candidate.station_key_id().as_str()
                    ))
                    .and_then(|axes| axes.get("latency").copied())
                    .unwrap_or_else(|| {
                        responsiveness_score(
                            candidate.avg_latency_ms().map(|value| value as u32),
                            profile.latency_cap_ms,
                        )
                        .get()
                    }),
                cost_basis_points: None,
                pricing: RoutePlanPricingSnapshot::unpriced("pricing_context_missing"),
                preference_basis_points: preference_score(candidate, request),
                failure_domains: vec![
                    format!("station:{}", candidate.station_id().as_str()),
                    format!("key:{}", candidate.station_key_id().as_str()),
                ],
            };
            let primary_reason = if probe_discovery_candidate {
                Some("error_rate_probe_discovery")
            } else if let Some(reason) = hard_rejection_reason {
                Some(reason)
            } else if !base_hard_eligible {
                Some("scoped_health_rejected")
            } else if !error_rate_admitted {
                Some("error_rate_rejected")
            } else {
                None
            };
            let eligibility = if probe_discovery_candidate {
                PlanningCandidateEligibility::ProbeDiscoveryOnly
            } else if candidate_snapshot.hard_eligible {
                PlanningCandidateEligibility::AdmittedForScoring
            } else {
                PlanningCandidateEligibility::Excluded
            };
            if matches!(
                eligibility,
                PlanningCandidateEligibility::AdmittedForScoring
                    | PlanningCandidateEligibility::ProbeDiscoveryOnly
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
        let max_candidates = usize::from(policy.max_candidates);
        // Keep probe-discovery rows separate from ordinary scoring rows while
        // retaining the existing bounded PlanningSnapshot shape. Ordinary
        // candidates always get first claim on the limit; probes can only use
        // otherwise-unused slots and are marked NotApplicable for scoring.
        let mut scoring_candidates = Vec::new();
        let mut probe_candidates = Vec::new();
        for (candidate, eligibility, primary_reason) in eligible_candidates {
            match eligibility {
                PlanningCandidateEligibility::ProbeDiscoveryOnly => {
                    probe_candidates.push((candidate, primary_reason));
                }
                PlanningCandidateEligibility::AdmittedForScoring => {
                    scoring_candidates.push((candidate, primary_reason));
                }
                PlanningCandidateEligibility::Excluded => unreachable!(
                    "excluded candidates are assessed before entering the eligible set"
                ),
            }
        }
        let mut candidates = Vec::with_capacity(max_candidates);
        for (index, (candidate, primary_reason)) in scoring_candidates.into_iter().enumerate() {
            let candidate_set = if index < max_candidates {
                PlanningCandidateSet::WithinLimit
            } else {
                PlanningCandidateSet::CappedByCandidateLimit
            };
            assessments.push(candidate_assessment_from_snapshot(
                &candidate,
                facts.snapshot_id().as_str(),
                durable_revision,
                &request_context_fingerprint,
                PlanningCandidateEligibility::AdmittedForScoring,
                candidate_set,
                primary_reason,
                Vec::new(),
            ));
            if index < max_candidates {
                candidates.push(candidate);
            }
        }
        let remaining_slots = max_candidates.saturating_sub(candidates.len());
        for (index, (candidate, primary_reason)) in probe_candidates.into_iter().enumerate() {
            assessments.push(candidate_assessment_from_snapshot(
                &candidate,
                facts.snapshot_id().as_str(),
                durable_revision,
                &request_context_fingerprint,
                PlanningCandidateEligibility::ProbeDiscoveryOnly,
                PlanningCandidateSet::NotApplicable,
                primary_reason,
                Vec::new(),
            ));
            if index < remaining_slots {
                candidates.push(candidate);
            }
        }
        let snapshot = PlanningSnapshot {
            snapshot_id: facts.snapshot_id().as_str().to_string(),
            durable_revision,
            routing_policy_revision,
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
    if !candidate.schedulable() || !candidate.credential().available() {
        return if !candidate.schedulable() {
            Some("candidate_unschedulable")
        } else {
            Some("credential_unavailable")
        };
    }
    let protocol_ok = match request.route_kind() {
        RouteKind::ModelCatalog => {
            candidate.supports_chat_completions() || candidate.supports_responses()
        }
        RouteKind::Inference => {
            candidate.supports_chat_completions() || candidate.supports_responses()
        }
    };
    let model = model_override;
    let model_ok = model.is_none_or(|model| {
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
    if !protocol_ok || !model_ok || !features_ok {
        return Some("capability_rejected");
    }
    if !tags_ok {
        return Some("tag_mismatch");
    }
    if depleted && !policy.allow_depleted_fallback {
        return Some("balance_depleted");
    }
    None
}

fn scoped_subjects_for_planning(
    facts: &super::assembler::OperationalFactBundle,
    request: &RouteRequestFacts,
    configuration: &crate::application::model_mapping::CompiledModelMappingConfiguration,
    mapping_facts: &crate::models::model_mapping::ModelRequestFacts,
) -> Result<Vec<ScopedHealthSubject>, PlanningSnapshotBuildError> {
    let mut subjects = Vec::with_capacity(facts.candidates().len().saturating_mul(5));
    for candidate in facts.candidates() {
        let station_id = candidate.station_id().as_str();
        let station_key_id = candidate.station_key_id().as_str();
        let credential_revision = candidate.credential().record_revision().get();
        let endpoint_revision = candidate.endpoint().endpoint_ref().revision().get();
        subjects.push(
            ScopedHealthSubject::credential(station_id, station_key_id, credential_revision)
                .map_err(|_| PlanningSnapshotBuildError::Invalid("credential health scope"))?,
        );
        subjects.push(
            ScopedHealthSubject::account(station_id, candidate.account_record_revision().get())
                .map_err(|_| PlanningSnapshotBuildError::Invalid("account health scope"))?,
        );
        subjects.push(
            ScopedHealthSubject::endpoint(station_id, endpoint_revision)
                .map_err(|_| PlanningSnapshotBuildError::Invalid("endpoint health scope"))?,
        );
        if let (Some(binding), Some(revision)) = (
            candidate.group_binding_id(),
            candidate.group_record_revision(),
        ) {
            subjects.push(
                ScopedHealthSubject::group(station_id, binding, revision.get())
                    .map_err(|_| PlanningSnapshotBuildError::Invalid("group health scope"))?,
            );
        }
        for upstream in candidate_native_models(candidate, request, configuration, mapping_facts) {
            subjects.push(
                ScopedHealthSubject::model_on_key(
                    station_id,
                    station_key_id,
                    &upstream,
                    candidate.endpoint().sanitized_origin().as_str(),
                    credential_revision,
                    endpoint_revision,
                    1,
                )
                .map_err(|_| PlanningSnapshotBuildError::Invalid("model health scope"))?,
            );
        }
    }
    Ok(subjects)
}

fn error_rate_candidate_admitted(
    candidate: &super::assembler::OperationalCandidateFact,
    config: ErrorRateAdmissionConfigV1,
    statuses: &[HealthProtectionStatus],
    health_probe: Option<&crate::application::health_protection::HealthProtectionProbe>,
) -> bool {
    if !config.enabled {
        return true;
    }
    let Some(scopes) = candidate_health_scopes(
        candidate.station_id().as_str(),
        candidate.station_key_id().as_str(),
        candidate.endpoint().endpoint_ref().revision().get(),
    ) else {
        return false;
    };
    let admitted = scopes.iter().all(|scope| {
        let matching_probe = config
            .probe
            .as_ref()
            .or(health_probe)
            .filter(|probe| probe.scope == *scope);
        scoped_admission_verdict_with_probe(statuses, scope, matching_probe).is_admitted()
    });
    admitted
}

fn error_rate_probe_discovery_allowed(
    candidate: &super::assembler::OperationalCandidateFact,
    config: ErrorRateAdmissionConfigV1,
    statuses: &[HealthProtectionStatus],
) -> bool {
    if !config.enabled || config.probe.is_some() {
        return false;
    }
    let Some(scopes) = candidate_health_scopes(
        candidate.station_id().as_str(),
        candidate.station_key_id().as_str(),
        candidate.endpoint().endpoint_ref().revision().get(),
    ) else {
        return false;
    };
    let discovered = scopes.iter().any(|scope| {
        scoped_admission_verdict_for_probe_candidate(statuses, scope)
            .is_admitted()
            && statuses.iter().any(|status| {
                status.scope == *scope
                    && status.persistence_kind
                        == crate::application::health_protection::HealthProtectionPersistenceKind::Durable
                    && status.state
                        == crate::application::health_protection::HealthProtectionState::Open
                    && status.cooldown_remaining_ms == Some(0)
            })
    });
    discovered
}

fn candidate_scoped_admitted(
    candidate: &super::assembler::OperationalCandidateFact,
    verdicts: &std::collections::BTreeMap<
        (String, FailureDimension),
        crate::persistence::stores::routing_health_verdict_store::ScopedHealthVerdictRow,
    >,
    ignore_endpoint: bool,
) -> bool {
    let endpoint_scope = ScopedHealthSubject::endpoint(
        candidate.station_id().as_str(),
        candidate.endpoint().endpoint_ref().revision().get(),
    )
    .ok()
    .map(|subject| subject.scope().to_string());
    let mut subjects = vec![
        ScopedHealthSubject::credential(
            candidate.station_id().as_str(),
            candidate.station_key_id().as_str(),
            candidate.credential().record_revision().get(),
        ),
        ScopedHealthSubject::account(
            candidate.station_id().as_str(),
            candidate.account_record_revision().get(),
        ),
        ScopedHealthSubject::endpoint(
            candidate.station_id().as_str(),
            candidate.endpoint().endpoint_ref().revision().get(),
        ),
    ];
    if let (Some(binding), Some(revision)) = (
        candidate.group_binding_id(),
        candidate.group_record_revision(),
    ) {
        subjects.push(ScopedHealthSubject::group(
            candidate.station_id().as_str(),
            binding,
            revision.get(),
        ));
    }
    subjects.into_iter().all(|subject| {
        subject.ok().is_none_or(|subject| {
            if ignore_endpoint && endpoint_scope.as_deref() == Some(subject.scope()) {
                return true;
            }
            verdicts
                .iter()
                .filter(|((scope, _), _)| scope == subject.scope())
                .all(|(_, row)| row.verdict == DurableHealthVerdict::Degraded)
        })
    })
}

fn candidate_model_scoped_admitted(
    candidate: &super::assembler::OperationalCandidateFact,
    upstream_model: &str,
    verdicts: &std::collections::BTreeMap<
        (String, FailureDimension),
        crate::persistence::stores::routing_health_verdict_store::ScopedHealthVerdictRow,
    >,
) -> bool {
    ScopedHealthSubject::model_on_key(
        candidate.station_id().as_str(),
        candidate.station_key_id().as_str(),
        upstream_model,
        candidate.endpoint().sanitized_origin().as_str(),
        candidate.credential().record_revision().get(),
        candidate.endpoint().endpoint_ref().revision().get(),
        1,
    )
    .ok()
    .is_none_or(|subject| {
        verdicts
            .iter()
            .filter(|((scope, _), _)| scope == subject.scope())
            .all(|(_, row)| row.verdict == DurableHealthVerdict::Degraded)
    })
}

fn candidate_native_models(
    candidate: &super::assembler::OperationalCandidateFact,
    request: &RouteRequestFacts,
    configuration: &crate::application::model_mapping::CompiledModelMappingConfiguration,
    mapping_facts: &crate::models::model_mapping::ModelRequestFacts,
) -> Vec<String> {
    resolve_candidate_mapping(configuration, mapping_facts, candidate, request)
        .ok()
        .map(|(variants, _)| {
            variants
                .into_iter()
                .map(|variant| variant.upstream_model)
                .collect()
        })
        .unwrap_or_else(|| {
            request
                .requested_model()
                .map(|model| vec![model.trim().to_string()])
                .unwrap_or_default()
        })
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
    fn error_rate_admission_is_disabled_by_default_and_fail_closed_when_open() {
        let candidate = test_candidate(None, None, None);
        let statuses = Vec::new();
        assert!(error_rate_candidate_admitted(
            &candidate,
            ErrorRateAdmissionConfigV1::disabled(),
            &statuses,
            None,
        ));

        let scope = admission_scope(
            crate::application::health_protection::HealthProtectionScopeKind::Credential,
            candidate.station_key_id().as_str(),
        );
        let status = crate::application::health_protection::HealthProtectionStatus {
            version: crate::application::health_protection::HEALTH_PROTECTION_VERSION.to_string(),
            scope,
            state: crate::application::health_protection::HealthProtectionState::Open,
            persistence_kind:
                crate::application::health_protection::HealthProtectionPersistenceKind::Durable,
            state_revision: 1,
            opened_at_ms: Some(10),
            cooldown_until_ms: Some(100),
            cooldown_remaining_ms: Some(90),
            half_open_probe_in_flight: false,
            recent_failure_code: None,
            sample_count: 5,
            failure_rate_percent: 100,
            updated_at_ms: 10,
            detail_available: true,
        };
        assert!(!error_rate_candidate_admitted(
            &candidate,
            ErrorRateAdmissionConfigV1::enabled(),
            &[status],
            None,
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
