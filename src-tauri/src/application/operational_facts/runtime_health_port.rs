use super::health_projector::{HealthProjectionTarget, RuntimeHealthSuppression};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DurableHealthCommit {
    pub(crate) target: HealthProjectionTarget,
    pub(crate) endpoint_revision: i64,
    pub(crate) durable_updated_at_ms: i64,
    pub(crate) recovered_from_ordinary_suppression: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeHealthProjectionCommand {
    ClearMatchingSuppression {
        target: HealthProjectionTarget,
        endpoint_revision: i64,
    },
    RecordRevisionLag {
        target: HealthProjectionTarget,
        durable_revision: i64,
        runtime_revision: i64,
    },
    Keep,
}

pub(crate) fn plan_runtime_health_projection_update(
    commit: &DurableHealthCommit,
    runtime: Option<&RuntimeHealthSuppression>,
) -> RuntimeHealthProjectionCommand {
    let Some(runtime) = runtime else {
        return RuntimeHealthProjectionCommand::Keep;
    };
    if runtime.target != commit.target {
        return RuntimeHealthProjectionCommand::Keep;
    }
    if runtime.endpoint_revision < commit.endpoint_revision {
        return RuntimeHealthProjectionCommand::RecordRevisionLag {
            target: commit.target.clone(),
            durable_revision: commit.endpoint_revision,
            runtime_revision: runtime.endpoint_revision,
        };
    }
    if runtime.endpoint_revision > commit.endpoint_revision {
        return RuntimeHealthProjectionCommand::Keep;
    }
    if commit.recovered_from_ordinary_suppression {
        return RuntimeHealthProjectionCommand::ClearMatchingSuppression {
            target: commit.target.clone(),
            endpoint_revision: commit.endpoint_revision,
        };
    }
    RuntimeHealthProjectionCommand::Keep
}
