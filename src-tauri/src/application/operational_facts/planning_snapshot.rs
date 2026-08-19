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
        factors::{reliability_posterior, responsiveness_score},
        failure_domains::ProviderCapacityDomain,
        request::{GroupFilterMode, RouteKind, RouteRequestFacts},
    },
    models::routing_policy::RoutingPolicyConfigV1,
    persistence::stores::routing_health_verdict_store::{
        DurableHealthVerdict, FailureDimension, RoutingHealthVerdictStore, ScopedHealthSubject,
    },
    persistence::stores::routing_quality_store::RoutingQualityStore,
    persistence::{stores::operational_facts::OperationalFactStore, ReadSession},
};

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

/// Builds the immutable durable half of a routing plan from one caller-owned
/// read transaction. Runtime capacity and circuit state are supplied explicitly
/// as an overlay so this builder never opens a second transaction.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PlanningSnapshotBuilder;

impl PlanningSnapshotBuilder {
    pub(crate) async fn build(
        &self,
        read: &mut ReadSession,
        options: &OperationalFactReadOptions,
        policy: RoutingPolicyConfigV1,
        routing_policy_revision: u64,
        profile: DispatchAlgorithmProfile,
        runtime: RuntimeOverlaySnapshot,
        request: &RouteRequestFacts,
    ) -> Result<PlanningSnapshot, PlanningSnapshotBuildError> {
        let reader = OperationalFactReader::new(OperationalFactStore);
        let facts = reader.load_bundle(read, options).await?;
        let scoped_subjects = scoped_subjects_for_planning(&facts, request)?;
        let scoped_verdicts = RoutingHealthVerdictStore
            .load_active_batch(read.connection(), &scoped_subjects)
            .await
            .map_err(|error| {
                PlanningSnapshotBuildError::Facts(OperationalFactReadError::Source(
                    error.to_string(),
                ))
            })?;
        let capability_subjects = capability_subjects_for_planning(&facts, request);
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
            request.model_mapping_revision().unwrap_or_else(|| {
                crate::application::model_mapping::current_configuration()
                    .mapping_revision
                    .min(i64::MAX as u64) as i64
            }),
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
        let mapping_configuration = crate::application::model_mapping::current_configuration();
        let mapping_facts = mapping_facts(request);
        let mut mapping_fallback_trigger = None;
        let candidates = facts
            .candidates()
            .iter()
            .filter_map(|candidate| {
                let resolved = resolve_candidate_mapping(
                    &mapping_configuration,
                    &mapping_facts,
                    candidate,
                    request,
                );
                let (candidate_variants, fallback_trigger) = match resolved {
                    Ok(value) => value,
                    Err(crate::application::model_mapping::ModelMappingResolutionError::ProfileHasNoOffering)
                    | Err(crate::application::model_mapping::ModelMappingResolutionError::NoResolvedTargets) => {
                        return None;
                    }
                    Err(_) => return None,
                };
                let candidate_variants = if unsupported_models.is_empty() {
                    candidate_variants
                } else {
                    candidate_variants
                        .into_iter()
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
                if request.requested_model().is_some()
                    && candidate_variants.is_empty()
                    && !matches!(request.route_kind(), RouteKind::ModelCatalog)
                {
                    return None;
                }
                if mapping_fallback_trigger.is_none() {
                    mapping_fallback_trigger = fallback_trigger;
                }
                let resolved_model_for_candidate = candidate_variants
                    .first()
                    .map(|variant| (variant.upstream_model.clone(), 1_i64));
                let resolved_model = resolved_model_for_candidate.or_else(|| resolved_model.clone());
                let candidate_model_for_gates = candidate_variants
                    .first()
                    .map(|variant| variant.upstream_model.clone())
                    .or_else(|| request.requested_model().map(ToOwned::to_owned));
                Some(CandidateSnapshot {
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
                hard_eligible: candidate_hard_eligible(
                    candidate,
                    request,
                    &policy,
                    candidate_model_for_gates.as_deref(),
                ) && candidate_scoped_admitted(candidate, &scoped_verdicts),
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
                })
            })
            .filter(|candidate| candidate.hard_eligible)
            .take(usize::from(policy.max_candidates))
            .collect();
        let snapshot = PlanningSnapshot {
            snapshot_id: facts.snapshot_id().as_str().to_string(),
            durable_revision,
            routing_policy_revision,
            policy,
            profile,
            candidates,
            model_fallback_trigger: mapping_fallback_trigger,
            runtime,
        };
        snapshot
            .validate()
            .map_err(PlanningSnapshotBuildError::Invalid)?;
        Ok(snapshot)
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
) -> Vec<(String, String, i64, i64, i64)> {
    let configuration = crate::application::model_mapping::current_configuration();
    let mapping_facts = mapping_facts(request);
    // The fifth tuple slot is a legacy provenance column. Native capability
    // identity is the station key + upstream model + execution revisions, so
    // mapping revisions must never partition capability facts.
    let native_identity_revision = 1_i64;
    let mut subjects = Vec::new();
    for candidate in facts.candidates() {
        let variants =
            resolve_candidate_mapping(&configuration, &mapping_facts, candidate, request)
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
    candidate
        .balance_value()
        .is_some_and(|value| value.is_finite() && value <= 0.0)
        || candidate.balance_status().is_some_and(|status| {
            matches!(
                status.trim().to_ascii_lowercase().as_str(),
                "low" | "depleted" | "exhausted" | "empty"
            )
        })
}

fn candidate_hard_eligible(
    candidate: &super::assembler::OperationalCandidateFact,
    request: &RouteRequestFacts,
    policy: &RoutingPolicyConfigV1,
    model_override: Option<&str>,
) -> bool {
    if !candidate.credential().available() {
        return false;
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
    protocol_ok
        && model_ok
        && features_ok
        && tags_ok
        && group_ok
        && (!depleted || policy.allow_depleted_fallback)
}

fn scoped_subjects_for_planning(
    facts: &super::assembler::OperationalFactBundle,
    request: &RouteRequestFacts,
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
        for upstream in candidate_native_models(candidate, request) {
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

fn candidate_scoped_admitted(
    candidate: &super::assembler::OperationalCandidateFact,
    verdicts: &std::collections::BTreeMap<
        (String, FailureDimension),
        crate::persistence::stores::routing_health_verdict_store::ScopedHealthVerdictRow,
    >,
) -> bool {
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
) -> Vec<String> {
    let configuration = crate::application::model_mapping::current_configuration();
    let facts = mapping_facts(request);
    resolve_candidate_mapping(&configuration, &facts, candidate, request)
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
            &RoutingPolicyConfigV1::default(),
            Some("gpt-4.1"),
        ));
        candidate.set_durable_health_for_planning_test(
            None,
            Some("auth_error: upstream returned HTTP 401"),
        );
        assert!(candidate_hard_eligible(
            &candidate,
            &test_request(GroupFilterMode::Any, None),
            &RoutingPolicyConfigV1::default(),
            Some("gpt-4.1"),
        ));
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
