#![allow(dead_code)]

#[path = "../src/models/operational/mod.rs"]
mod operational_model;

mod models {
    pub(crate) mod operational {
        pub(crate) use crate::operational_model::*;
    }
}

mod operational_facts {
    #[path = "../../src/application/operational_facts/health_projector.rs"]
    pub mod health_projector;
    #[path = "../../src/application/operational_facts/runtime_health_port.rs"]
    pub mod runtime_health_port;
}

use operational_facts::health_projector::{
    project_effective_health, DurableHealthGate, DurableHealthProjection, HealthAdmission,
    HealthProjectionTarget, PoolEjectionGuard, RuntimeHealthSuppression, RuntimeSuppressionKind,
};
use operational_facts::runtime_health_port::{
    plan_runtime_health_projection_update, DurableHealthCommit, RuntimeHealthProjectionCommand,
};
use operational_model::{
    EndpointId, EndpointRef, EndpointRevision, ModelName, StationId, StationKeyId, UnixMillis,
};

fn now() -> UnixMillis {
    UnixMillis::new(10_000).expect("now")
}

fn until(value: i64) -> UnixMillis {
    UnixMillis::new(value).expect("until")
}

fn station_key_target(id: &str) -> HealthProjectionTarget {
    HealthProjectionTarget::StationKey(StationKeyId::new(id).expect("key"))
}

fn account_target(id: &str) -> HealthProjectionTarget {
    HealthProjectionTarget::StationAccount(StationId::new(id).expect("station"))
}

fn endpoint_target(station_id: &str, endpoint_id: &str, revision: i64) -> HealthProjectionTarget {
    HealthProjectionTarget::Endpoint(EndpointRef::new(
        StationId::new(station_id).expect("station"),
        EndpointId::new(endpoint_id).expect("endpoint"),
        EndpointRevision::new(revision).expect("revision"),
    ))
}

fn model_target(key_id: &str, model: &str) -> HealthProjectionTarget {
    HealthProjectionTarget::Model {
        station_key_id: StationKeyId::new(key_id).expect("key"),
        model: ModelName::new(model).expect("model"),
    }
}

fn durable(target: HealthProjectionTarget, gate: DurableHealthGate) -> DurableHealthProjection {
    DurableHealthProjection {
        target,
        endpoint_revision: 2,
        gate,
        cooldown_until_ms: None,
        updated_at_ms: now(),
    }
}

fn runtime(target: HealthProjectionTarget, revision: i64) -> RuntimeHealthSuppression {
    RuntimeHealthSuppression {
        target,
        endpoint_revision: revision,
        kind: RuntimeSuppressionKind::OrdinaryOutlier,
        suppress_until_ms: until(20_000),
        created_at_ms: now(),
    }
}

fn guard(active: bool) -> PoolEjectionGuard {
    PoolEjectionGuard {
        ordinary_suppression_relaxed: active,
    }
}

#[test]
fn runtime_overlay_requires_same_target_and_revision() {
    let projection = project_effective_health(
        durable(station_key_target("key-1"), DurableHealthGate::Available),
        Some(runtime(endpoint_target("station-1", "endpoint-1", 2), 2)),
        guard(false),
        now(),
    );

    assert_eq!(projection.admission, HealthAdmission::Admit);
    assert!(!projection.runtime_overlay_applied);
    assert!(projection.stale_runtime_overlay_ignored);

    let projection = project_effective_health(
        durable(station_key_target("key-1"), DurableHealthGate::Available),
        Some(runtime(station_key_target("key-1"), 1)),
        guard(false),
        now(),
    );

    assert_eq!(projection.admission, HealthAdmission::Admit);
    assert!(!projection.runtime_overlay_applied);
    assert!(projection.stale_runtime_overlay_ignored);
}

#[test]
fn pool_ejection_guard_relaxes_only_ordinary_runtime_suppression() {
    let projection = project_effective_health(
        durable(station_key_target("key-1"), DurableHealthGate::Available),
        Some(runtime(station_key_target("key-1"), 2)),
        guard(true),
        now(),
    );

    assert_eq!(projection.admission, HealthAdmission::Admit);
    assert_eq!(
        projection.reasons,
        vec!["pool_ejection_guard_relaxed_runtime_outlier"]
    );

    let projection = project_effective_health(
        durable(station_key_target("key-1"), DurableHealthGate::AuthBlocked),
        Some(runtime(station_key_target("key-1"), 2)),
        guard(true),
        now(),
    );

    assert_eq!(projection.admission, HealthAdmission::HardReject);
    assert_eq!(projection.reasons, vec!["durable_auth_block"]);
    assert!(!projection.runtime_overlay_applied);
}

#[test]
fn durable_cooldown_and_model_hard_reject_are_not_cross_target_runtime_state() {
    let mut durable_cooldown = durable(
        station_key_target("key-1"),
        DurableHealthGate::OrdinaryCooldown,
    );
    durable_cooldown.cooldown_until_ms = Some(until(11_000));
    let projection = project_effective_health(
        durable_cooldown,
        Some(runtime(model_target("key-1", "gpt-4.1"), 2)),
        guard(true),
        now(),
    );

    assert_eq!(
        projection.admission,
        HealthAdmission::SuppressDurableCooldown
    );
    assert_eq!(projection.reasons, vec!["durable_ordinary_cooldown"]);
    assert!(!projection.runtime_overlay_applied);
    assert!(projection.stale_runtime_overlay_ignored);

    let projection = project_effective_health(
        durable(
            model_target("key-1", "gpt-4.1"),
            DurableHealthGate::ModelUnsupported,
        ),
        Some(runtime(station_key_target("key-1"), 2)),
        guard(true),
        now(),
    );

    assert_eq!(projection.admission, HealthAdmission::HardReject);
    assert_eq!(projection.reasons, vec!["durable_model_unsupported"]);
}

#[test]
fn runtime_port_clears_matching_revision_after_durable_recovery() {
    let target = station_key_target("key-1");
    let command = plan_runtime_health_projection_update(
        &DurableHealthCommit {
            target: target.clone(),
            endpoint_revision: 2,
            durable_updated_at_ms: 12_000,
            recovered_from_ordinary_suppression: true,
        },
        Some(&runtime(target.clone(), 2)),
    );

    assert_eq!(
        command,
        RuntimeHealthProjectionCommand::ClearMatchingSuppression {
            target,
            endpoint_revision: 2,
        }
    );
}

#[test]
fn runtime_port_records_lag_without_rolling_back_durable_truth() {
    let target = account_target("station-1");
    let command = plan_runtime_health_projection_update(
        &DurableHealthCommit {
            target: target.clone(),
            endpoint_revision: 3,
            durable_updated_at_ms: 12_000,
            recovered_from_ordinary_suppression: true,
        },
        Some(&runtime(target.clone(), 2)),
    );

    assert_eq!(
        command,
        RuntimeHealthProjectionCommand::RecordRevisionLag {
            target,
            durable_revision: 3,
            runtime_revision: 2,
        }
    );
}
