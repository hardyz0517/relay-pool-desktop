#![allow(dead_code)]

#[path = "../src/application/routing_engine/capacity.rs"]
mod capacity;

use capacity::{
    effective_load_denominator, CapacityAcquireFailure, CapacityConstraintKey,
    CompositeCapacityRegistry, CompositeCapacityRequest, ProviderAccountConstraint,
};

fn request(station_id: &str, station_key_id: &str) -> CompositeCapacityRequest {
    CompositeCapacityRequest {
        station_id: station_id.to_string(),
        station_key_id: station_key_id.to_string(),
        half_open_probe_id: None,
        global_max_concurrency: 8,
        station_account_max_concurrency: 2,
        station_key_max_concurrency: 1,
        provider_account_constraint: ProviderAccountConstraint::NotApplicable,
    }
}

#[test]
fn composite_capacity_acquires_fixed_order_and_rolls_back_middle_failure() {
    let registry = CompositeCapacityRegistry::default();
    let mut first = request("station-a", "key-a");
    first.station_account_max_concurrency = 1;
    let first_lease = registry.try_acquire(first).expect("first lease");
    assert_eq!(registry.gauge(&CapacityConstraintKey::Global).active, 1);
    assert_eq!(
        registry
            .gauge(&CapacityConstraintKey::StationAccount(
                "station-a".to_string()
            ))
            .active,
        1
    );

    let mut second = request("station-a", "key-b");
    second.station_account_max_concurrency = 1;
    let failure = registry.try_acquire(second).expect_err("station cap");
    assert!(matches!(
        failure,
        CapacityAcquireFailure::ConstraintUnavailable {
            constraint: CapacityConstraintKey::StationAccount(_),
            ..
        }
    ));
    assert_eq!(
        registry.gauge(&CapacityConstraintKey::Global).active,
        1,
        "failed second acquire must roll back global"
    );

    drop(first_lease);
    assert_eq!(registry.gauge(&CapacityConstraintKey::Global).active, 0);
    assert_eq!(
        registry
            .gauge(&CapacityConstraintKey::StationAccount(
                "station-a".to_string()
            ))
            .active,
        0
    );
}

#[test]
fn station_account_limit_is_shared_across_keys_and_key_limit_is_separate() {
    let registry = CompositeCapacityRegistry::default();
    let mut first = request("station-a", "key-a");
    first.station_account_max_concurrency = 1;
    let _first_lease = registry.try_acquire(first).expect("first lease");

    let mut second = request("station-a", "key-b");
    second.station_account_max_concurrency = 1;
    assert!(matches!(
        registry.try_acquire(second),
        Err(CapacityAcquireFailure::ConstraintUnavailable {
            constraint: CapacityConstraintKey::StationAccount(_),
            ..
        })
    ));

    let third = request("station-b", "key-a");
    assert!(matches!(
        registry.try_acquire(third),
        Err(CapacityAcquireFailure::ConstraintUnavailable {
            constraint: CapacityConstraintKey::StationKey(_),
            ..
        })
    ));
}

#[test]
fn provider_account_limit_only_applies_with_trusted_scope() {
    let registry = CompositeCapacityRegistry::default();
    let mut trusted_a = request("station-a", "key-a");
    trusted_a.provider_account_constraint = ProviderAccountConstraint::Trusted {
        provider_account_id: "provider-account-a".to_string(),
        max_concurrency: 1,
    };
    let _trusted_lease = registry.try_acquire(trusted_a).expect("trusted lease");

    let mut trusted_b = request("station-b", "key-b");
    trusted_b.provider_account_constraint = ProviderAccountConstraint::Trusted {
        provider_account_id: "provider-account-a".to_string(),
        max_concurrency: 1,
    };
    assert!(matches!(
        registry.try_acquire(trusted_b),
        Err(CapacityAcquireFailure::ConstraintUnavailable {
            constraint: CapacityConstraintKey::ProviderAccount(_),
            ..
        })
    ));

    let mut untrusted = request("station-c", "key-c");
    untrusted.provider_account_constraint = ProviderAccountConstraint::EvidenceGap {
        reason: "provider_scope_untrusted",
    };
    let lease = registry
        .try_acquire(untrusted)
        .expect("untrusted not enforced");
    assert_eq!(lease.evidence_gaps()[0].reason, "provider_scope_untrusted");
    assert_eq!(
        registry
            .gauge(&CapacityConstraintKey::ProviderAccount(
                "provider-account-a".to_string()
            ))
            .active,
        1
    );
}

#[test]
fn zero_max_concurrency_is_unlimited_and_load_factor_never_expands_hard_limit() {
    let registry = CompositeCapacityRegistry::default();
    let mut unlimited = request("station-a", "key-a");
    unlimited.global_max_concurrency = 0;
    unlimited.station_account_max_concurrency = 0;
    unlimited.station_key_max_concurrency = 0;
    let _a = registry.try_acquire(unlimited.clone()).expect("a");
    let _b = registry.try_acquire(unlimited).expect("b");
    assert_eq!(registry.gauge(&CapacityConstraintKey::Global).active, 2);
    assert_eq!(effective_load_denominator(2, 10), 10);
    assert_eq!(effective_load_denominator(2, 0), 2);
    assert_eq!(effective_load_denominator(0, 0), 1);
}

#[test]
fn runtime_limit_down_keeps_existing_lease_but_blocks_new_acquire() {
    let registry = CompositeCapacityRegistry::default();
    let mut first = request("station-a", "key-a");
    first.station_key_max_concurrency = 2;
    let _lease = registry.try_acquire(first.clone()).expect("first");

    registry.set_runtime_max(CapacityConstraintKey::StationKey("key-a".to_string()), 1);
    assert_eq!(
        registry
            .gauge(&CapacityConstraintKey::StationKey("key-a".to_string()))
            .active,
        1
    );
    assert!(matches!(
        registry.try_acquire(first),
        Err(CapacityAcquireFailure::ConstraintUnavailable {
            constraint: CapacityConstraintKey::StationKey(_),
            ..
        })
    ));
}
