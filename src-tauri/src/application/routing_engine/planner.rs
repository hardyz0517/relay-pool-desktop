#![allow(dead_code)]

use crate::application::{
    operational_facts::candidate_projector::RouteCandidateProjection,
    routing_engine::{
        eligibility::evaluate_candidate,
        request::PlanningRoundContext,
        selector::{
            build_route_plan, RoutePlan, RoutePlanCandidate, RoutePlannerError,
            MAX_ROUTE_PLAN_CANDIDATES,
        },
    },
};

pub(crate) struct PlanningInput<'a> {
    pub(crate) context: &'a PlanningRoundContext,
    pub(crate) candidates: &'a [RouteCandidateProjection],
    pub(crate) affinity_station_key_id: Option<&'a str>,
}

pub(crate) fn plan_route(input: PlanningInput<'_>) -> Result<RoutePlan, RoutePlannerError> {
    if input.candidates.len() > MAX_ROUTE_PLAN_CANDIDATES {
        return Err(RoutePlannerError::CandidateLimitExceeded {
            actual: input.candidates.len(),
            limit: MAX_ROUTE_PLAN_CANDIDATES,
        });
    }

    let mut eligible = Vec::new();
    let mut rejections = Vec::new();
    for candidate in input.candidates {
        match evaluate_candidate(candidate, &input.context.progress) {
            Ok(()) => eligible.push(candidate),
            Err(rejection) => rejections.push(rejection),
        }
    }

    Ok(build_route_plan(
        input.context,
        eligible,
        rejections,
        input.affinity_station_key_id,
    ))
}

pub(crate) fn ordered_plan_candidates(plan: &RoutePlan) -> Vec<&RoutePlanCandidate> {
    plan.strata
        .iter()
        .flat_map(|stratum| stratum.candidates.iter())
        .collect()
}

pub(crate) fn plan_candidate_count(plan: &RoutePlan) -> usize {
    plan.strata
        .iter()
        .map(|stratum| stratum.candidates.len())
        .sum()
}
