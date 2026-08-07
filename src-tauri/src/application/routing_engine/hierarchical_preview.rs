use crate::application::{
    operational_facts::candidate_projector::RouteCandidateProjection,
    routing_engine::{
        candidate_plan::{
            build_route_plan, RoutePlan, RoutePlanCandidate, RoutePlannerError,
            MAX_ROUTE_PLAN_CANDIDATES,
        },
        eligibility::evaluate_candidate,
        request::PlanningRoundContext,
    },
};

pub struct PlanningInput<'a> {
    pub context: &'a PlanningRoundContext,
    pub candidates: &'a [RouteCandidateProjection],
    pub affinity_station_key_id: Option<&'a str>,
}

pub fn plan_route(input: PlanningInput<'_>) -> Result<RoutePlan, RoutePlannerError> {
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

pub fn ordered_plan_candidates(plan: &RoutePlan) -> Vec<&RoutePlanCandidate> {
    plan.strata
        .iter()
        .flat_map(|stratum| stratum.candidates.iter())
        .collect()
}
