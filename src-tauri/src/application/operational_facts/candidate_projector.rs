use crate::application::{
    operational_facts::{
        balance_projector::{BalanceProjection, BalanceProjectionStatus},
        capability_projector::{CapabilityDecision, CapabilityProjection},
        group_projector::GroupProjection,
        health_projector::{EffectiveHealthProjection, HealthAdmission},
        multiplier_projector::{MultiplierProjection, MultiplierResolutionStatus},
        pricing_projector::{RequestCostComparisonContext, RoutingCostBasis},
    },
    routing_engine::request::{GroupFilterMode, RouteKind, RouteRequestFacts},
};

pub(crate) const ROUTE_CANDIDATE_PROJECTION_VERSION: &str = "route_candidate_projection_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=route-read-model.capacity-scope; owner=application/operational_facts; remove_when=runtime candidate adapter stops reserving endpoint/model capacity scopes"
    )
)]
pub(crate) enum CapacityScope {
    StationKey,
    StationAccount,
    Endpoint,
    Model,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CapacityScopeSnapshot {
    pub(crate) scope: CapacityScope,
    pub(crate) limit: Option<u32>,
    pub(crate) in_flight: u32,
    pub(crate) available: bool,
    pub(crate) source_revision: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CapacityProjection {
    pub(crate) scopes: Vec<CapacityScopeSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateIdentityProjection {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) sanitized_origin: String,
    pub(crate) credential_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidatePolicyProjection {
    pub(crate) group_filter_mode: GroupFilterMode,
    pub(crate) required_group_stable_key: Option<String>,
    pub(crate) group_matches: bool,
    pub(crate) backup_only: bool,
    pub(crate) preferred_model_match: bool,
    pub(crate) tag_filter_match: bool,
    pub(crate) allow_depleted_fallback: bool,
    pub(crate) affinity_eligible: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CandidateGroupProjection {
    pub(crate) stable_key: String,
    pub(crate) display_name: String,
    pub(crate) available: bool,
    pub(crate) reason: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CandidateMultiplierProjection {
    pub(crate) status: MultiplierResolutionStatus,
    pub(crate) multiplier: Option<f64>,
    pub(crate) selected_source: Option<&'static str>,
    pub(crate) ceiling_rejected: bool,
    pub(crate) reason: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CandidatePricingProjection {
    pub(crate) basis: RoutingCostBasis,
    pub(crate) comparison_value: Option<f64>,
    pub(crate) reason: Option<&'static str>,
    pub(crate) currency: Option<String>,
    pub(crate) unit: Option<String>,
    pub(crate) estimated_input_price: Option<f64>,
    pub(crate) estimated_output_price: Option<f64>,
    pub(crate) estimated_fixed_price: Option<f64>,
    pub(crate) status_label: String,
    pub(crate) source_chain: Vec<String>,
    pub(crate) observed_at: Option<String>,
    pub(crate) confidence: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateBalanceProjection {
    pub(crate) status: BalanceProjectionStatus,
    pub(crate) selected_scope: Option<String>,
    pub(crate) reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateCapabilityProjection {
    pub(crate) protocol: CapabilityDecision,
    pub(crate) model: CapabilityDecision,
    pub(crate) stream: CapabilityDecision,
    pub(crate) tools: CapabilityDecision,
    pub(crate) vision: CapabilityDecision,
    pub(crate) reasoning: CapabilityDecision,
    pub(crate) rejection_subjects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateHealthProjection {
    pub(crate) station_key: HealthAdmission,
    pub(crate) station_account: HealthAdmission,
    pub(crate) endpoint: HealthAdmission,
    pub(crate) model: HealthAdmission,
    pub(crate) runtime_overlay_applied: bool,
    pub(crate) reasons: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateProvenanceProjection {
    pub(crate) snapshot_id: String,
    pub(crate) fact_version_vector: String,
    pub(crate) projector_version: &'static str,
    pub(crate) endpoint_revision: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RouteCandidateProjection {
    pub(crate) identity: CandidateIdentityProjection,
    pub(crate) priority: i64,
    pub(crate) route_kind: RouteKind,
    pub(crate) requested_model: Option<String>,
    pub(crate) resolved_model: Option<String>,
    pub(crate) policy: CandidatePolicyProjection,
    pub(crate) group: Option<CandidateGroupProjection>,
    pub(crate) multiplier: CandidateMultiplierProjection,
    pub(crate) pricing: CandidatePricingProjection,
    pub(crate) balance: CandidateBalanceProjection,
    pub(crate) capability: CandidateCapabilityProjection,
    pub(crate) health: CandidateHealthProjection,
    pub(crate) capacity: CapacityProjection,
    pub(crate) provenance: CandidateProvenanceProjection,
    pub(crate) hard_rejection_codes: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CapabilityProjectionSet {
    pub(crate) protocol: CapabilityProjection,
    pub(crate) model: CapabilityProjection,
    pub(crate) stream: CapabilityProjection,
    pub(crate) tools: CapabilityProjection,
    pub(crate) vision: CapabilityProjection,
    pub(crate) reasoning: CapabilityProjection,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HealthProjectionSet {
    pub(crate) station_key: EffectiveHealthProjection,
    pub(crate) station_account: EffectiveHealthProjection,
    pub(crate) endpoint: EffectiveHealthProjection,
    pub(crate) model: EffectiveHealthProjection,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CandidateOperationalProjections {
    pub(crate) identity: CandidateIdentityProjection,
    pub(crate) priority: i64,
    pub(crate) resolved_model: Option<String>,
    pub(crate) group: Option<GroupProjection>,
    pub(crate) multiplier: MultiplierProjection,
    pub(crate) pricing: RequestCostComparisonContext,
    pub(crate) balance: BalanceProjection,
    pub(crate) capabilities: CapabilityProjectionSet,
    pub(crate) health: HealthProjectionSet,
    pub(crate) capacity: CapacityProjection,
    pub(crate) backup_only: bool,
    pub(crate) candidate_tags: Vec<String>,
    pub(crate) snapshot_id: String,
    pub(crate) fact_version_vector: String,
}

pub(crate) fn project_route_candidate(
    request: &RouteRequestFacts,
    candidate: CandidateOperationalProjections,
) -> RouteCandidateProjection {
    let group = candidate
        .group
        .as_ref()
        .map(|group| CandidateGroupProjection {
            stable_key: group.identity.stable_key(),
            display_name: group.display_name.clone(),
            available: group.available,
            reason: group.trace.reason,
        });
    let group_matches = match (
        request.group_filter_mode(),
        request.required_group_stable_key(),
        group.as_ref(),
    ) {
        (GroupFilterMode::Any, _, _) => true,
        (GroupFilterMode::Required, Some(required), Some(group)) => group.stable_key == required,
        (GroupFilterMode::Required, _, _) => false,
    };
    let resolved_model = candidate
        .resolved_model
        .clone()
        .or_else(|| request.requested_model().map(ToString::to_string));
    let preferred_model_match = resolved_model
        .as_deref()
        .map(|model| {
            request
                .preferred_models()
                .iter()
                .any(|preferred| preferred.eq_ignore_ascii_case(model))
        })
        .unwrap_or(false);
    let tag_filter_match = request.required_tags().iter().all(|tag| {
        candidate
            .candidate_tags
            .iter()
            .any(|candidate_tag| candidate_tag.eq_ignore_ascii_case(tag))
    });
    let policy = CandidatePolicyProjection {
        group_filter_mode: request.group_filter_mode(),
        required_group_stable_key: request.required_group_stable_key().map(ToString::to_string),
        group_matches,
        backup_only: candidate.backup_only,
        preferred_model_match,
        tag_filter_match,
        allow_depleted_fallback: request.allow_depleted_fallback(),
        affinity_eligible: request.affinity_enabled() && !candidate.backup_only,
    };
    let multiplier_value = candidate.multiplier.multiplier.map(|value| value.get());
    let ceiling_rejected = match request.route_kind() {
        RouteKind::Inference => request
            .max_rate_multiplier()
            .zip(multiplier_value)
            .map(|(ceiling, multiplier)| multiplier > ceiling)
            .unwrap_or(false),
        RouteKind::ModelCatalog => false,
    };
    let multiplier = CandidateMultiplierProjection {
        status: candidate.multiplier.status,
        multiplier: multiplier_value,
        selected_source: candidate
            .multiplier
            .selected_kind
            .map(multiplier_source_label),
        ceiling_rejected,
        reason: candidate.multiplier.trace.reason,
    };
    let pricing = CandidatePricingProjection {
        basis: candidate.pricing.basis,
        comparison_value: candidate.pricing.comparison_value,
        reason: candidate.pricing.reason,
        currency: candidate.pricing.currency,
        unit: candidate.pricing.unit,
        estimated_input_price: candidate.pricing.estimated_input_price,
        estimated_output_price: candidate.pricing.estimated_output_price,
        estimated_fixed_price: candidate.pricing.estimated_fixed_price,
        status_label: candidate.pricing.status_label,
        source_chain: candidate.pricing.source_chain,
        observed_at: candidate.pricing.observed_at,
        confidence: candidate.pricing.confidence,
    };
    let balance = CandidateBalanceProjection {
        status: candidate.balance.status,
        selected_scope: candidate
            .balance
            .selected_scope
            .map(|scope| format!("{scope:?}")),
        reason: candidate.balance.trace.reason,
    };
    let capability = capability_projection(candidate.capabilities);
    let health = health_projection(candidate.health);
    let mut hard_rejection_codes = Vec::new();
    if !candidate.identity.credential_available {
        hard_rejection_codes.push("credential_missing");
    }
    if !policy.group_matches {
        hard_rejection_codes.push("group_mismatch");
    }
    if !policy.tag_filter_match {
        hard_rejection_codes.push("tag_mismatch");
    }
    if ceiling_rejected {
        hard_rejection_codes.push("multiplier_ceiling");
    }
    match request.route_kind() {
        RouteKind::Inference if pricing.basis == RoutingCostBasis::NotApplicable => {
            hard_rejection_codes.push("pricing_not_applicable_for_inference");
        }
        RouteKind::Inference | RouteKind::ModelCatalog => {}
    }
    if capability.rejection_subjects.iter().any(|subject| {
        let subject = subject.split(':').next().unwrap_or(subject);
        match subject {
            "protocol" | "model" => true,
            "stream" => request.stream(),
            "tools" => request.uses_tools(),
            "vision" => request.uses_vision(),
            "reasoning" => request.uses_reasoning(),
            _ => false,
        }
    }) {
        hard_rejection_codes.push("capability_rejected");
    }
    if !request.allow_depleted_fallback()
        && balance.status == BalanceProjectionStatus::DepletedEmergency
    {
        hard_rejection_codes.push("balance_depleted");
    }
    if health.station_key == HealthAdmission::HardReject
        || health.station_account == HealthAdmission::HardReject
        || health.endpoint == HealthAdmission::HardReject
        || health.model == HealthAdmission::HardReject
    {
        hard_rejection_codes.push("health_hard_reject");
    }
    if candidate
        .capacity
        .scopes
        .iter()
        .any(|scope| !scope.available)
    {
        hard_rejection_codes.push("capacity_unavailable");
    }

    RouteCandidateProjection {
        identity: candidate.identity.clone(),
        priority: candidate.priority,
        route_kind: request.route_kind(),
        requested_model: request.requested_model().map(ToString::to_string),
        resolved_model,
        policy,
        group,
        multiplier,
        pricing,
        balance,
        capability,
        health,
        capacity: candidate.capacity,
        provenance: CandidateProvenanceProjection {
            snapshot_id: candidate.snapshot_id,
            fact_version_vector: candidate.fact_version_vector,
            projector_version: ROUTE_CANDIDATE_PROJECTION_VERSION,
            endpoint_revision: candidate.identity.endpoint_revision,
        },
        hard_rejection_codes,
    }
}

fn capability_projection(input: CapabilityProjectionSet) -> CandidateCapabilityProjection {
    let subjects = [
        ("protocol", input.protocol.decision),
        ("model", input.model.decision),
        ("stream", input.stream.decision),
        ("tools", input.tools.decision),
        ("vision", input.vision.decision),
        ("reasoning", input.reasoning.decision),
    ];
    CandidateCapabilityProjection {
        protocol: input.protocol.decision,
        model: input.model.decision,
        stream: input.stream.decision,
        tools: input.tools.decision,
        vision: input.vision.decision,
        reasoning: input.reasoning.decision,
        rejection_subjects: subjects
            .into_iter()
            .filter_map(|(subject, decision)| match decision {
                CapabilityDecision::Allow => None,
                CapabilityDecision::Reject => Some(subject.to_string()),
                CapabilityDecision::RequireStrictConfirmation => {
                    Some(format!("{subject}:strict_unknown"))
                }
            })
            .collect(),
    }
}

fn health_projection(input: HealthProjectionSet) -> CandidateHealthProjection {
    let mut reasons = Vec::new();
    reasons.extend(input.station_key.reasons.iter().copied());
    reasons.extend(input.station_account.reasons.iter().copied());
    reasons.extend(input.endpoint.reasons.iter().copied());
    reasons.extend(input.model.reasons.iter().copied());
    CandidateHealthProjection {
        station_key: input.station_key.admission,
        station_account: input.station_account.admission,
        endpoint: input.endpoint.admission,
        model: input.model.admission,
        runtime_overlay_applied: input.station_key.runtime_overlay_applied
            || input.station_account.runtime_overlay_applied
            || input.endpoint.runtime_overlay_applied
            || input.model.runtime_overlay_applied,
        reasons,
    }
}

fn multiplier_source_label(
    kind: crate::application::operational_facts::multiplier_projector::MultiplierEvidenceKind,
) -> &'static str {
    match kind {
        crate::application::operational_facts::multiplier_projector::MultiplierEvidenceKind::BindingLatestUser => {
            "binding_latest_user"
        }
        crate::application::operational_facts::multiplier_projector::MultiplierEvidenceKind::BindingLatestEffective => {
            "binding_latest_effective"
        }
        crate::application::operational_facts::multiplier_projector::MultiplierEvidenceKind::CurrentUser => {
            "current_user"
        }
        crate::application::operational_facts::multiplier_projector::MultiplierEvidenceKind::CurrentEffective => {
            "current_effective"
        }
        crate::application::operational_facts::multiplier_projector::MultiplierEvidenceKind::CurrentDefault => {
            "current_default"
        }
        crate::application::operational_facts::multiplier_projector::MultiplierEvidenceKind::ManualOverride => {
            "manual_override"
        }
    }
}
