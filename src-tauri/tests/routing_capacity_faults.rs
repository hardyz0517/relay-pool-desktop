#![allow(dead_code)]

#[path = "../src/application/routing_engine/capacity.rs"]
mod capacity;

use capacity::{
    CapacityConstraintKey, CapacityMissObservation, CapacityWaitMiss, CompositeCapacityRegistry,
    CompositeCapacityRequest, PlanningRoundCapacityState, ProviderAccountConstraint,
    RetryBudgetMiss, RetryBudgetRegistry, RetryPermitDecision,
};

fn request(station_id: &str, station_key_id: &str) -> CompositeCapacityRequest {
    CompositeCapacityRequest {
        station_id: station_id.to_string(),
        station_key_id: station_key_id.to_string(),
        half_open_probe_id: Some(format!("half-open-{station_key_id}")),
        global_max_concurrency: 4,
        station_account_max_concurrency: 4,
        station_key_max_concurrency: 4,
        provider_account_constraint: ProviderAccountConstraint::NotApplicable,
    }
}

#[test]
fn capacity_lease_and_half_open_release_on_drop_without_underflow() {
    let registry = CompositeCapacityRegistry::default();
    let lease = registry
        .try_acquire(request("station-a", "key-a"))
        .expect("lease");
    assert_eq!(registry.gauge(&CapacityConstraintKey::Global).active, 1);
    assert_eq!(
        registry
            .gauge(&CapacityConstraintKey::HalfOpen(
                "half-open-key-a".to_string()
            ))
            .active,
        1
    );
    drop(lease);
    assert_eq!(registry.gauge(&CapacityConstraintKey::Global).active, 0);
    assert_eq!(
        registry
            .gauge(&CapacityConstraintKey::HalfOpen(
                "half-open-key-a".to_string()
            ))
            .active,
        0
    );

    let mut lease = registry
        .try_acquire(request("station-a", "key-a"))
        .expect("lease");
    lease.release();
    lease.release();
    assert_eq!(registry.gauge(&CapacityConstraintKey::Global).active, 0);
}

#[test]
fn retry_budget_is_global_twenty_percent_with_minimum_one_and_raii_release() {
    let budget = RetryBudgetRegistry::new(10);
    assert_eq!(budget.max_active(), 2);
    assert!(matches!(
        budget.acquire_for_round(0).expect("initial"),
        RetryPermitDecision::NotRequired
    ));
    let first = match budget.acquire_for_round(1).expect("first retry") {
        RetryPermitDecision::Acquired(permit) => permit,
        RetryPermitDecision::NotRequired => panic!("fallback round must require permit"),
    };
    let _second = match budget.acquire_for_round(2).expect("second retry") {
        RetryPermitDecision::Acquired(permit) => permit,
        RetryPermitDecision::NotRequired => panic!("fallback round must require permit"),
    };
    assert!(matches!(
        budget.acquire_for_round(3),
        Err(RetryBudgetMiss::Exhausted {
            active: 2,
            max_active: 2
        })
    ));
    drop(first);
    assert_eq!(budget.active(), 1);

    let tiny = RetryBudgetRegistry::new(1);
    assert_eq!(tiny.max_active(), 1);
}

#[test]
fn wait_plan_is_single_constraint_bounded_and_cancel_releases_waiter() {
    let registry = CompositeCapacityRegistry::default();
    let constraint = CapacityConstraintKey::StationKey("key-a".to_string());
    let mut round = PlanningRoundCapacityState::default();
    round.record_miss(CapacityMissObservation {
        constraint: constraint.clone(),
        waitable: true,
        in_flight: 1,
        max_concurrency: 1,
    });
    round.record_miss(CapacityMissObservation {
        constraint: CapacityConstraintKey::Global,
        waitable: true,
        in_flight: 4,
        max_concurrency: 4,
    });
    let plan = round.build_wait_plan(1_000, 2_000, 1).expect("wait plan");
    assert_eq!(plan.constraint, constraint);
    assert_eq!(plan.timeout_ms, 1_000);

    let permit = registry
        .try_enter_wait(plan.constraint.clone(), plan.max_waiters, 1_000, 2_000)
        .expect("wait permit");
    assert_eq!(permit.ticket(), 0);
    assert_eq!(registry.gauge(&plan.constraint).waiting, 1);
    assert!(matches!(
        registry.try_enter_wait(plan.constraint.clone(), plan.max_waiters, 1_001, 2_000),
        Err(CapacityWaitMiss::QueueFull)
    ));
    drop(permit);
    assert_eq!(registry.gauge(&plan.constraint).waiting, 0);
}

#[test]
fn non_waitable_or_expired_round_does_not_admit_wait() {
    let mut round = PlanningRoundCapacityState::default();
    round.record_miss(CapacityMissObservation {
        constraint: CapacityConstraintKey::StationKey("key-a".to_string()),
        waitable: false,
        in_flight: 1,
        max_concurrency: 1,
    });
    assert_eq!(
        round.build_wait_plan(1_000, 2_000, 1),
        Err(CapacityWaitMiss::NotAdmitted)
    );
    assert_eq!(
        round.build_wait_plan(2_000, 1_000, 1),
        Err(CapacityWaitMiss::NotAdmitted)
    );
}
