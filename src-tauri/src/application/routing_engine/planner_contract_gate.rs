//! Compile-time boundary between the production legacy planner and the
//! qualification-only snapshot planner. The contracts deliberately use
//! different inputs, outputs, and entrypoint names.

use super::{
    intelligent_planner, planner_legacy,
    planning_snapshot::PlanningSnapshot,
    selector::{RoutePlan as LegacyRoutePlan, RoutePlannerError},
};

type LegacyPlanRouteContract = for<'input> fn(
    planner_legacy::PlanningInput<'input>,
) -> Result<LegacyRoutePlan, RoutePlannerError>;

type IntelligentPlanSnapshotContract =
    fn(
        &PlanningSnapshot,
        &[u8],
        u64,
    ) -> Result<intelligent_planner::RoutePlan, intelligent_planner::PlannerError>;

const LEGACY_PLAN_ROUTE: LegacyPlanRouteContract = planner_legacy::plan_route;
const INTELLIGENT_PLAN_SNAPSHOT: IntelligentPlanSnapshotContract =
    intelligent_planner::plan_snapshot;

#[test]
fn planners_compile_against_distinct_contracts() {
    let legacy_entrypoint = LEGACY_PLAN_ROUTE as usize;
    let intelligent_entrypoint = INTELLIGENT_PLAN_SNAPSHOT as usize;

    assert_ne!(legacy_entrypoint, intelligent_entrypoint);
}
