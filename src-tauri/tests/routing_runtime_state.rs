#![allow(dead_code)]

mod routing_engine {
    #[path = "../../src/application/routing_engine/affinity.rs"]
    pub(crate) mod affinity;
    #[path = "../../src/application/routing_engine/runtime_metrics.rs"]
    pub(crate) mod runtime_metrics;
}

use routing_engine::{
    affinity::{AffinityKind, AffinityLookup, AffinityMiss, AffinityPolicy, AffinityRegistry},
    runtime_metrics::{
        parse_retry_after_ms, RuntimeAdmission, RuntimeAttemptObservation, RuntimeDegradedReason,
        RuntimeEndpointKind, RuntimeFailureKind, RuntimeMetricKey, RuntimeModelClass,
        RuntimeOutlierPolicyV1, RuntimeRouteState,
    },
};

fn key(id: &str) -> RuntimeMetricKey {
    RuntimeMetricKey::new(
        id,
        RuntimeEndpointKind::ChatCompletions,
        Some("gpt-4o-mini"),
        10,
        20,
    )
}

fn failure(state: &mut RuntimeRouteState, attempt_id: &str, key: RuntimeMetricKey, now_ms: i64) {
    state.report_attempt(
        attempt_id,
        key,
        RuntimeAttemptObservation::Failure(RuntimeFailureKind::Ordinary),
        now_ms,
    );
}

fn suppress_with_five_failures(state: &mut RuntimeRouteState, key: RuntimeMetricKey) {
    for index in 0..5 {
        failure(
            state,
            &format!("attempt-{}-{index}", key.station_key_id),
            key.clone(),
            i64::from(index),
        );
    }
}

#[test]
fn runtime_metric_key_bounds_unknown_and_high_cardinality_model_class() {
    assert_eq!(RuntimeModelClass::normalize(None), RuntimeModelClass::Other);
    assert_eq!(
        RuntimeModelClass::normalize(Some("tenant/model/with/unbounded/shape")),
        RuntimeModelClass::Other
    );
    assert_eq!(
        RuntimeModelClass::normalize(Some("GPT-4O.MINI")),
        RuntimeModelClass::Named("gpt-4o.mini".to_string())
    );

    let policy = RuntimeOutlierPolicyV1 {
        max_entries: 2,
        ..RuntimeOutlierPolicyV1::default()
    };
    let mut state = RuntimeRouteState::new(policy).expect("policy");
    failure(&mut state, "a", key("key-a"), 1);
    failure(&mut state, "b", key("key-b"), 2);
    failure(&mut state, "c", key("key-c"), 3);

    assert_eq!(state.entry_count(), 2);
    assert!(!state.has_entry(&key("key-a")));
}

#[test]
fn outlier_policy_uses_v1_window_threshold_and_max_ejection_protection() {
    let mut state = RuntimeRouteState::default();
    let key_a = key("key-a");
    let key_b = key("key-b");
    let key_c = key("key-c");
    let threshold_key = key("key-threshold");

    for index in 0..2 {
        failure(
            &mut state,
            &format!("attempt-threshold-pre-{index}"),
            threshold_key.clone(),
            i64::from(index),
        );
    }
    for index in 2..4 {
        state.report_attempt(
            format!("attempt-threshold-success-{index}"),
            threshold_key.clone(),
            RuntimeAttemptObservation::Success,
            i64::from(index),
        );
    }
    let threshold_pre = state.snapshot_overlay(&[threshold_key.clone()], 4);
    assert_eq!(
        threshold_pre.entries[&threshold_key].admission,
        RuntimeAdmission::Available
    );

    failure(
        &mut state,
        "attempt-threshold-fifth-sample-third-failure",
        threshold_key.clone(),
        5,
    );
    let threshold_snapshot = state.snapshot_overlay(&[threshold_key.clone(), key("key-safe")], 6);
    assert!(
        matches!(
            threshold_snapshot.entries[&threshold_key].admission,
            RuntimeAdmission::Suppressed { .. }
        ),
        "{:?}",
        threshold_snapshot.entries[&threshold_key]
    );

    suppress_with_five_failures(&mut state, key_a.clone());
    suppress_with_five_failures(&mut state, key_b.clone());
    suppress_with_five_failures(&mut state, key_c.clone());

    let single = state.snapshot_overlay(&[key_a.clone()], 10);
    assert_eq!(
        single.entries[&key_a].admission,
        RuntimeAdmission::Degraded {
            reason: RuntimeDegradedReason::SingleCandidateOutlierProtected
        }
    );

    let snapshot = state.snapshot_overlay(&[key_a.clone(), key_b.clone(), key_c.clone()], 10);
    let suppressed = snapshot
        .entries
        .values()
        .filter(|entry| matches!(entry.admission, RuntimeAdmission::Suppressed { .. }))
        .count();
    let protected = snapshot
        .entries
        .values()
        .filter(|entry| {
            entry.admission
                == RuntimeAdmission::Degraded {
                    reason: RuntimeDegradedReason::MaxPassiveEjectionProtected,
                }
        })
        .count();
    assert_eq!(suppressed, 1);
    assert_eq!(protected, 2);
    assert_eq!(snapshot.policy_version, "runtime_outlier_policy_v1");
}

#[test]
fn retry_after_is_only_used_when_parseable_positive_and_clamped() {
    assert_eq!(parse_retry_after_ms(Some("2")), Some(2_000));
    assert_eq!(parse_retry_after_ms(Some("-1")), None);
    assert_eq!(parse_retry_after_ms(Some("not-a-number")), None);

    let mut state = RuntimeRouteState::default();
    let key_a = key("key-a");
    state.report_attempt(
        "attempt-rate-limited",
        key_a.clone(),
        RuntimeAttemptObservation::Failure(RuntimeFailureKind::RateLimited {
            retry_after_ms: parse_retry_after_ms(Some("7200")),
        }),
        10_000,
    );
    let snapshot = state.snapshot_overlay(&[key_a.clone(), key("key-b")], 10_001);
    assert_eq!(
        snapshot.entries[&key_a].admission,
        RuntimeAdmission::Suppressed {
            until_ms: 10_000 + 60 * 60 * 1_000
        }
    );
}

#[test]
fn half_open_requires_two_successes_cancel_releases_and_recovery_slow_starts() {
    let mut state = RuntimeRouteState::default();
    let key_a = key("key-a");
    suppress_with_five_failures(&mut state, key_a.clone());

    assert!(state.try_acquire_half_open_probe(&key_a, 30_004).is_some());
    {
        let _permit = state
            .try_acquire_half_open_probe(&key_a, 30_005)
            .expect("permit after previous cancel/drop");
    }
    let permit = state
        .try_acquire_half_open_probe(&key_a, 30_006)
        .expect("first success permit");
    permit.record_success(30_006);
    let snapshot = state.snapshot_overlay(&[key_a.clone()], 30_007);
    assert_eq!(
        snapshot.entries[&key_a].admission,
        RuntimeAdmission::HalfOpen { successes: 1 }
    );

    let permit = state
        .try_acquire_half_open_probe(&key_a, 30_008)
        .expect("second success permit");
    permit.record_success(30_008);
    let snapshot = state.snapshot_overlay(&[key_a.clone()], 30_009);
    assert_eq!(
        snapshot.entries[&key_a].admission,
        RuntimeAdmission::Degraded {
            reason: RuntimeDegradedReason::SlowStart
        }
    );
    assert!(snapshot.entries[&key_a].slow_start_penalty > 0.0);

    let snapshot = state.snapshot_overlay(&[key_a.clone()], 91_000);
    assert_eq!(
        snapshot.entries[&key_a].admission,
        RuntimeAdmission::Available
    );
}

#[test]
fn runtime_feedback_applies_once_and_revision_changes_ignore_old_state() {
    let mut state = RuntimeRouteState::default();
    let old_key = key("key-a");
    for _ in 0..5 {
        let outcome = state.report_attempt(
            "same-attempt",
            old_key.clone(),
            RuntimeAttemptObservation::Failure(RuntimeFailureKind::Ordinary),
            1,
        );
        if outcome.applied {
            assert_eq!(outcome.policy_version, "runtime_outlier_policy_v1");
        }
    }
    let snapshot = state.snapshot_overlay(&[old_key.clone()], 2);
    assert_eq!(
        snapshot.entries[&old_key].admission,
        RuntimeAdmission::Available
    );

    suppress_with_five_failures(&mut state, old_key.clone());
    let new_key = RuntimeMetricKey::new(
        "key-a",
        RuntimeEndpointKind::ChatCompletions,
        Some("gpt-4o-mini"),
        11,
        20,
    );
    let snapshot = state.snapshot_overlay(&[new_key.clone()], 10);
    assert_eq!(
        snapshot.entries[&new_key].admission,
        RuntimeAdmission::Available
    );
    assert_eq!(state.retain_live_revisions(&[new_key.clone()]), 1);
    assert!(!state.has_entry(&old_key));
}

#[test]
fn affinity_bind_lookup_ttl_mismatch_and_bounds_are_explicit() {
    let mut registry = AffinityRegistry::new(AffinityPolicy {
        max_entries: 1,
        ttl_ms: 100,
    });
    let lookup = AffinityLookup::new(
        AffinityKind::Session,
        "group-a",
        "session-a",
        7,
        Some("gpt-4o-mini"),
    );
    registry
        .bind(lookup.clone(), "key-a", 0, 100)
        .expect("bind");
    let hit = registry.lookup(&lookup, 50).expect("hit");
    assert_eq!(hit.station_key_id, "key-a");
    assert_eq!(hit.expires_at_ms, 100);
    assert_eq!(registry.lookup(&lookup, 100), Err(AffinityMiss::Expired));

    registry
        .bind(lookup.clone(), "key-a", 200, 100)
        .expect("rebind");
    let group_mismatch = AffinityLookup::new(
        AffinityKind::Session,
        "group-b",
        "session-a",
        7,
        Some("gpt-4o-mini"),
    );
    assert_eq!(
        registry.lookup(&group_mismatch, 201),
        Err(AffinityMiss::GroupScopeMismatch)
    );
    let revision_mismatch = AffinityLookup::new(
        AffinityKind::Session,
        "group-a",
        "session-a",
        8,
        Some("gpt-4o-mini"),
    );
    assert_eq!(
        registry.lookup(&revision_mismatch, 201),
        Err(AffinityMiss::EndpointRevisionMismatch)
    );
    let model_mismatch = AffinityLookup::new(
        AffinityKind::Session,
        "group-a",
        "session-a",
        7,
        Some("gpt-4o"),
    );
    assert_eq!(
        registry.lookup(&model_mismatch, 201),
        Err(AffinityMiss::ModelMismatch)
    );

    registry
        .bind(
            AffinityLookup::new(AffinityKind::PreviousResponse, "group-a", "resp-a", 7, None),
            "key-b",
            202,
            100,
        )
        .expect("bounded bind");
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.lookup(&lookup, 203), Err(AffinityMiss::NotFound));
}
