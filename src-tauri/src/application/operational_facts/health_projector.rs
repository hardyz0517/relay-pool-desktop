use crate::models::operational::{EndpointRef, ModelName, StationId, StationKeyId, UnixMillis};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HealthProjectionTarget {
    StationKey(StationKeyId),
    StationAccount(StationId),
    Endpoint(EndpointRef),
    Model {
        station_key_id: StationKeyId,
        model: ModelName,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DurableHealthGate {
    Available,
    Degraded,
    OrdinaryCooldown,
    AuthBlocked,
    UserDisabled,
    ModelUnsupported,
    MultiplierCeiling,
    Unknown,
}

impl DurableHealthGate {
    pub(crate) fn is_hard_reject(self) -> bool {
        matches!(
            self,
            Self::AuthBlocked
                | Self::UserDisabled
                | Self::ModelUnsupported
                | Self::MultiplierCeiling
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DurableHealthProjection {
    pub(crate) target: HealthProjectionTarget,
    pub(crate) endpoint_revision: i64,
    pub(crate) gate: DurableHealthGate,
    pub(crate) cooldown_until_ms: Option<UnixMillis>,
    pub(crate) updated_at_ms: UnixMillis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeSuppressionKind {
    OrdinaryOutlier,
    HalfOpenProbe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeHealthSuppression {
    pub(crate) target: HealthProjectionTarget,
    pub(crate) endpoint_revision: i64,
    pub(crate) kind: RuntimeSuppressionKind,
    pub(crate) suppress_until_ms: UnixMillis,
    pub(crate) created_at_ms: UnixMillis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PoolEjectionGuard {
    pub(crate) ordinary_suppression_relaxed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HealthAdmission {
    Admit,
    AdmitDegraded,
    SuppressOrdinaryRuntime,
    SuppressDurableCooldown,
    HardReject,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EffectiveHealthProjection {
    pub(crate) target: HealthProjectionTarget,
    pub(crate) admission: HealthAdmission,
    pub(crate) reasons: Vec<&'static str>,
    pub(crate) runtime_overlay_applied: bool,
    pub(crate) stale_runtime_overlay_ignored: bool,
}

pub(crate) fn project_effective_health(
    durable: DurableHealthProjection,
    runtime: Option<RuntimeHealthSuppression>,
    guard: PoolEjectionGuard,
    now_ms: UnixMillis,
) -> EffectiveHealthProjection {
    let mut reasons = Vec::new();
    let mut stale_runtime_overlay_ignored = false;
    let applicable_runtime = runtime.filter(|entry| {
        let same_target = entry.target == durable.target;
        let same_revision = entry.endpoint_revision == durable.endpoint_revision;
        let fresh = entry.suppress_until_ms.get() > now_ms.get();
        if !same_target || !same_revision {
            stale_runtime_overlay_ignored = true;
        }
        same_target && same_revision && fresh
    });

    if durable.gate.is_hard_reject() {
        reasons.push(match durable.gate {
            DurableHealthGate::AuthBlocked => "durable_auth_block",
            DurableHealthGate::UserDisabled => "durable_user_disabled",
            DurableHealthGate::ModelUnsupported => "durable_model_unsupported",
            DurableHealthGate::MultiplierCeiling => "durable_multiplier_ceiling",
            _ => unreachable!("checked by is_hard_reject"),
        });
        return EffectiveHealthProjection {
            target: durable.target,
            admission: HealthAdmission::HardReject,
            reasons,
            runtime_overlay_applied: false,
            stale_runtime_overlay_ignored,
        };
    }

    if durable
        .cooldown_until_ms
        .map(|until| until.get() > now_ms.get())
        .unwrap_or(false)
        || durable.gate == DurableHealthGate::OrdinaryCooldown
    {
        reasons.push("durable_ordinary_cooldown");
        return EffectiveHealthProjection {
            target: durable.target,
            admission: HealthAdmission::SuppressDurableCooldown,
            reasons,
            runtime_overlay_applied: false,
            stale_runtime_overlay_ignored,
        };
    }

    if let Some(runtime) = applicable_runtime {
        if guard.ordinary_suppression_relaxed
            && runtime.kind == RuntimeSuppressionKind::OrdinaryOutlier
        {
            reasons.push("pool_ejection_guard_relaxed_runtime_outlier");
        } else {
            reasons.push(match runtime.kind {
                RuntimeSuppressionKind::OrdinaryOutlier => "runtime_ordinary_outlier",
                RuntimeSuppressionKind::HalfOpenProbe => "runtime_half_open_probe",
            });
            return EffectiveHealthProjection {
                target: durable.target,
                admission: HealthAdmission::SuppressOrdinaryRuntime,
                reasons,
                runtime_overlay_applied: true,
                stale_runtime_overlay_ignored,
            };
        }
    }

    let admission = match durable.gate {
        DurableHealthGate::Available => HealthAdmission::Admit,
        DurableHealthGate::Degraded => HealthAdmission::AdmitDegraded,
        DurableHealthGate::Unknown => HealthAdmission::Unknown,
        DurableHealthGate::OrdinaryCooldown => HealthAdmission::SuppressDurableCooldown,
        DurableHealthGate::AuthBlocked
        | DurableHealthGate::UserDisabled
        | DurableHealthGate::ModelUnsupported
        | DurableHealthGate::MultiplierCeiling => HealthAdmission::HardReject,
    };
    if reasons.is_empty() {
        reasons.push(match admission {
            HealthAdmission::Admit => "durable_available",
            HealthAdmission::AdmitDegraded => "durable_degraded",
            HealthAdmission::Unknown => "durable_unknown",
            HealthAdmission::SuppressDurableCooldown => "durable_ordinary_cooldown",
            HealthAdmission::SuppressOrdinaryRuntime => "runtime_ordinary_suppression",
            HealthAdmission::HardReject => "durable_hard_reject",
        });
    }

    EffectiveHealthProjection {
        target: durable.target,
        admission,
        reasons,
        runtime_overlay_applied: false,
        stale_runtime_overlay_ignored,
    }
}
