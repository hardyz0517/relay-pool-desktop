#![allow(dead_code)]

use std::collections::BTreeMap;

use crate::application::{
    operational_facts::{
        balance_projector::BalanceProjectionStatus, candidate_projector::RouteCandidateProjection,
        pricing_projector::RoutingCostBasis,
    },
    routing_engine::{
        eligibility::RouteRejection,
        request::{OrderingProfile, PlanningRoundContext},
    },
};

pub(crate) const HIERARCHICAL_ROUTE_PLANNER_VERSION: &str = "hierarchical_route_planner_v1";
pub(crate) const MAX_ROUTE_PLAN_CANDIDATES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AvailabilityTier {
    Primary,
    ConfiguredBackup,
    DepletedEmergency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecisionEvidence {
    pub(crate) code: &'static str,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoutePlanCandidate {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) priority: i64,
    pub(crate) tier: AvailabilityTier,
    pub(crate) pricing: RoutePlanPricingSnapshot,
    pub(crate) evidence: Vec<DecisionEvidence>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoutePlanPricingSnapshot {
    pub(crate) basis: RoutingCostBasis,
    pub(crate) currency: Option<String>,
    pub(crate) unit: Option<String>,
    pub(crate) estimated_input_price: Option<f64>,
    pub(crate) estimated_output_price: Option<f64>,
    pub(crate) estimated_fixed_price: Option<f64>,
    pub(crate) status_label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoutePlanStratum {
    pub(crate) tier: AvailabilityTier,
    pub(crate) candidates: Vec<RoutePlanCandidate>,
}

impl RoutePlanStratum {
    pub(crate) fn candidate_ids(&self) -> Vec<&str> {
        self.candidates
            .iter()
            .map(|candidate| candidate.station_key_id.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoutePlan {
    pub(crate) planner_version: &'static str,
    pub(crate) ordering_profile: OrderingProfile,
    pub(crate) snapshot_id: String,
    pub(crate) runtime_overlay_revision: u64,
    pub(crate) projector_versions: Vec<&'static str>,
    pub(crate) strata: Vec<RoutePlanStratum>,
    pub(crate) rejections: Vec<RouteRejection>,
    pub(crate) selected_station_key_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RoutePlannerError {
    CandidateLimitExceeded { actual: usize, limit: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CostBasisClass {
    PreferredExact,
    OtherExact,
    MultiplierProxy,
    Unpriced,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ExactCostFamily {
    currency: String,
    unit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CandidateOrderKey {
    tier: AvailabilityTier,
    first: i64,
    second: i64,
    third: i64,
    fourth: i64,
    station_key_id: String,
}

pub(crate) fn build_route_plan(
    context: &PlanningRoundContext,
    eligible: Vec<&RouteCandidateProjection>,
    rejections: Vec<RouteRejection>,
    affinity_station_key_id: Option<&str>,
) -> RoutePlan {
    let preferred_exact_family = preferred_exact_family(&eligible);
    let mut strata = Vec::new();
    for tier in [
        AvailabilityTier::Primary,
        AvailabilityTier::ConfiguredBackup,
        AvailabilityTier::DepletedEmergency,
    ] {
        let mut candidates = eligible
            .iter()
            .copied()
            .filter(|candidate| availability_tier(candidate) == tier)
            .collect::<Vec<_>>();
        candidates.sort_by_key(|candidate| {
            order_key(
                context.request.ordering_profile(),
                candidate,
                tier,
                preferred_exact_family.as_ref(),
                affinity_station_key_id,
            )
        });
        if candidates.is_empty() {
            continue;
        }
        strata.push(RoutePlanStratum {
            tier,
            candidates: candidates
                .into_iter()
                .map(|candidate| plan_candidate(candidate, tier, context))
                .collect(),
        });
    }
    let selected_station_key_id = strata
        .first()
        .and_then(|stratum| stratum.candidates.first())
        .map(|candidate| candidate.station_key_id.clone());

    RoutePlan {
        planner_version: HIERARCHICAL_ROUTE_PLANNER_VERSION,
        ordering_profile: context.request.ordering_profile(),
        snapshot_id: context.snapshot_id.clone(),
        runtime_overlay_revision: context.runtime_overlay_revision,
        projector_versions: projector_versions(&eligible),
        strata,
        rejections,
        selected_station_key_id,
    }
}

fn order_key(
    profile: OrderingProfile,
    candidate: &RouteCandidateProjection,
    tier: AvailabilityTier,
    preferred_exact_family: Option<&ExactCostFamily>,
    affinity_station_key_id: Option<&str>,
) -> CandidateOrderKey {
    match profile {
        OrderingProfile::PriorityFirst => {
            priority_first_key(candidate, tier, affinity_station_key_id)
        }
        OrderingProfile::CostFirst => cost_first_key(candidate, tier, preferred_exact_family),
    }
}

fn priority_first_key(
    candidate: &RouteCandidateProjection,
    tier: AvailabilityTier,
    affinity_station_key_id: Option<&str>,
) -> CandidateOrderKey {
    CandidateOrderKey {
        tier,
        first: candidate.priority,
        second: affinity_rank(candidate, affinity_station_key_id),
        third: preferred_rank(candidate),
        fourth: soft_cost_band(candidate).unwrap_or(i64::MAX),
        station_key_id: candidate.identity.station_key_id.clone(),
    }
}

fn cost_first_key(
    candidate: &RouteCandidateProjection,
    tier: AvailabilityTier,
    preferred_exact_family: Option<&ExactCostFamily>,
) -> CandidateOrderKey {
    let (basis_class, band) = cost_basis_class_and_band(candidate, preferred_exact_family);
    CandidateOrderKey {
        tier,
        first: basis_class as i64,
        second: band.unwrap_or(i64::MAX),
        third: candidate.priority,
        fourth: preferred_rank(candidate),
        station_key_id: candidate.identity.station_key_id.clone(),
    }
}

fn affinity_rank(
    candidate: &RouteCandidateProjection,
    affinity_station_key_id: Option<&str>,
) -> i64 {
    if candidate.policy.affinity_eligible
        && affinity_station_key_id == Some(candidate.identity.station_key_id.as_str())
    {
        0
    } else {
        1
    }
}

fn preferred_rank(candidate: &RouteCandidateProjection) -> i64 {
    if candidate.policy.preferred_model_match {
        0
    } else {
        1
    }
}

fn cost_basis_class_and_band(
    candidate: &RouteCandidateProjection,
    preferred_exact_family: Option<&ExactCostFamily>,
) -> (CostBasisClass, Option<i64>) {
    if let Some(family) = exact_cost_family(candidate) {
        if Some(&family) == preferred_exact_family {
            return (CostBasisClass::PreferredExact, soft_cost_band(candidate));
        }
        return (CostBasisClass::OtherExact, None);
    }
    if candidate.pricing.basis == RoutingCostBasis::MultiplierProxy
        && finite_positive(candidate.pricing.comparison_value).is_some()
    {
        return (CostBasisClass::MultiplierProxy, soft_cost_band(candidate));
    }
    (CostBasisClass::Unpriced, None)
}

fn preferred_exact_family(candidates: &[&RouteCandidateProjection]) -> Option<ExactCostFamily> {
    let mut counts = BTreeMap::<ExactCostFamily, usize>::new();
    for candidate in candidates {
        if let Some(family) = exact_cost_family(candidate) {
            *counts.entry(family).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .max_by(|(left_family, left_count), (right_family, right_count)| {
            left_count
                .cmp(right_count)
                .then_with(|| right_family.cmp(left_family))
        })
        .map(|(family, _)| family)
}

fn exact_cost_family(candidate: &RouteCandidateProjection) -> Option<ExactCostFamily> {
    if candidate.pricing.basis != RoutingCostBasis::ExactPrice {
        return None;
    }
    finite_positive(candidate.pricing.comparison_value)?;
    Some(ExactCostFamily {
        currency: candidate.pricing.currency.clone()?,
        unit: candidate.pricing.unit.clone()?,
    })
}

fn soft_cost_band(candidate: &RouteCandidateProjection) -> Option<i64> {
    let value = finite_positive(candidate.pricing.comparison_value)?;
    Some((value * 20.0).floor() as i64)
}

fn finite_positive(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

fn availability_tier(candidate: &RouteCandidateProjection) -> AvailabilityTier {
    if candidate.balance.status == BalanceProjectionStatus::DepletedEmergency {
        AvailabilityTier::DepletedEmergency
    } else if candidate.policy.backup_only {
        AvailabilityTier::ConfiguredBackup
    } else {
        AvailabilityTier::Primary
    }
}

fn plan_candidate(
    candidate: &RouteCandidateProjection,
    tier: AvailabilityTier,
    context: &PlanningRoundContext,
) -> RoutePlanCandidate {
    RoutePlanCandidate {
        station_key_id: candidate.identity.station_key_id.clone(),
        station_id: candidate.identity.station_id.clone(),
        endpoint_revision: candidate.identity.endpoint_revision,
        priority: candidate.priority,
        tier,
        pricing: RoutePlanPricingSnapshot {
            basis: candidate.pricing.basis,
            currency: candidate.pricing.currency.clone(),
            unit: candidate.pricing.unit.clone(),
            estimated_input_price: candidate.pricing.estimated_input_price,
            estimated_output_price: candidate.pricing.estimated_output_price,
            estimated_fixed_price: candidate.pricing.estimated_fixed_price,
            status_label: candidate.pricing.status_label.clone(),
        },
        evidence: bounded_evidence(candidate, context),
    }
}

fn bounded_evidence(
    candidate: &RouteCandidateProjection,
    context: &PlanningRoundContext,
) -> Vec<DecisionEvidence> {
    [
        DecisionEvidence {
            code: "planner_version",
            detail: HIERARCHICAL_ROUTE_PLANNER_VERSION.to_string(),
        },
        DecisionEvidence {
            code: "snapshot_id",
            detail: context.snapshot_id.clone(),
        },
        DecisionEvidence {
            code: "fact_version_vector",
            detail: candidate.provenance.fact_version_vector.clone(),
        },
        DecisionEvidence {
            code: "projector_version",
            detail: candidate.provenance.projector_version.to_string(),
        },
        DecisionEvidence {
            code: "runtime_overlay_revision",
            detail: context.runtime_overlay_revision.to_string(),
        },
    ]
    .into_iter()
    .collect()
}

fn projector_versions(candidates: &[&RouteCandidateProjection]) -> Vec<&'static str> {
    let mut versions = candidates
        .iter()
        .map(|candidate| candidate.provenance.projector_version)
        .collect::<Vec<_>>();
    versions.sort_unstable();
    versions.dedup();
    versions
}
