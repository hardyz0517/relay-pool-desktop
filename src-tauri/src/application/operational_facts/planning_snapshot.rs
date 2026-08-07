use crate::{
    application::routing_engine::{
        algorithm_profile::DispatchAlgorithmProfile,
        planning_snapshot::{CandidateSnapshot, PlanningSnapshot, RuntimeOverlaySnapshot},
    },
    application::routing_engine::{
        factors::{reliability_posterior, responsiveness_score},
        request::{GroupFilterMode, RouteKind, RouteRequestFacts},
        routing_health::health_values_are_blocked,
    },
    models::routing_policy::RoutingPolicyConfigV1,
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
        profile: DispatchAlgorithmProfile,
        runtime: RuntimeOverlaySnapshot,
        request: &RouteRequestFacts,
    ) -> Result<PlanningSnapshot, PlanningSnapshotBuildError> {
        let reader = OperationalFactReader::new(OperationalFactStore);
        let facts = reader.load_bundle(read, options).await?;
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
        ]
        .into_iter()
        .max()
        .filter(|revision| *revision > 0)
        .ok_or(PlanningSnapshotBuildError::Invalid("revision_unavailable"))?
            as u64;
        // The raw query has a broader fixed upper bound to contain database
        // work. The policy is the actual planner limit, applied after hard
        // gates so an ineligible early row cannot starve a usable candidate.
        let candidates = facts
            .candidates()
            .iter()
            .map(|candidate| CandidateSnapshot {
                station_key_id: candidate.station_key_id().as_str().to_string(),
                station_id: candidate.station_id().as_str().to_string(),
                endpoint_revision: candidate.endpoint().endpoint_ref().revision().get(),
                credential_revision: candidate.credential().record_revision().get(),
                credential_available: candidate.credential().available(),
                hard_eligible: candidate_hard_eligible(candidate, request, &policy),
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
                preference_basis_points: preference_score(candidate, request),
                failure_domains: vec![
                    format!("station:{}", candidate.station_id().as_str()),
                    format!("key:{}", candidate.station_key_id().as_str()),
                ],
            })
            .filter(|candidate| candidate.hard_eligible)
            .take(usize::from(policy.max_candidates))
            .collect();
        let snapshot = PlanningSnapshot {
            snapshot_id: facts.snapshot_id().as_str().to_string(),
            durable_revision,
            policy,
            profile,
            candidates,
            runtime,
        };
        snapshot
            .validate()
            .map_err(PlanningSnapshotBuildError::Invalid)?;
        Ok(snapshot)
    }
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
    let model = request.requested_model();
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
    let health_ok = !health_values_are_blocked(
        candidate.cooldown_until(),
        candidate.last_error_summary(),
        request.admitted_at_ms(),
    );
    let depleted = candidate_is_depleted(candidate);
    protocol_ok
        && model_ok
        && features_ok
        && tags_ok
        && group_ok
        && health_ok
        && (!depleted || policy.allow_depleted_fallback)
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
    fn hard_gate_rejects_active_cooldown_and_offline_durable_health() {
        let mut candidate = test_candidate(None, None, None);
        candidate.set_durable_health_for_planning_test(Some("1001"), None);
        assert!(!candidate_hard_eligible(
            &candidate,
            &test_request(GroupFilterMode::Any, None),
            &RoutingPolicyConfigV1::default(),
        ));
        candidate.set_durable_health_for_planning_test(
            None,
            Some("auth_error: upstream returned HTTP 401"),
        );
        assert!(!candidate_hard_eligible(
            &candidate,
            &test_request(GroupFilterMode::Any, None),
            &RoutingPolicyConfigV1::default(),
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
