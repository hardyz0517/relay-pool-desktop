use std::sync::Arc;

use crate::{
    application::{
        error::ApplicationError,
        health_protection::{
            HealthProbeAdmissionMode, HealthProtectionProbe, HealthProtectionScope,
            HealthProtectionStatus,
        },
        operational_facts::target_resolver::ExecutionTargetRef,
        routing::RoutingService,
        routing_engine::planning_snapshot::{PlanningSnapshot, RuntimeOverlaySnapshot},
        routing_engine::request::{PlanningRequestContext, RouteRequestFacts},
    },
    models::{pricing::BalanceSnapshot, routing::RuntimeRoutingSettings},
};

/// Stable capabilities exposed from the application layer to the proxy
/// execution boundary. This intentionally contains reads needed while a
/// request is running, plus the small health-probe mutation used by admission.
pub(crate) trait RoutingExecutionReadPort: Send + Sync {
    #[cfg(test)]
    fn load_planning_snapshot(
        &self,
        request: RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
        context: PlanningRequestContext,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<PlanningSnapshot>, RoutingExecutionReadError>,
    >;

    fn load_planning_snapshot_with_probe(
        &self,
        request: RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
        context: PlanningRequestContext,
        probe: Option<HealthProtectionProbe>,
        probe_mode: HealthProbeAdmissionMode,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<PlanningSnapshot>, RoutingExecutionReadError>,
    >;

    fn load_execution_settings(
        &self,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<RuntimeRoutingSettings, RoutingExecutionReadError>,
    >;

    fn load_balance_snapshots(
        &self,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<BalanceSnapshot>, RoutingExecutionReadError>,
    >;

    fn load_operational_execution_target_refs(
        &self,
        station_key_ids: Vec<String>,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<ExecutionTargetRef>, RoutingExecutionReadError>,
    >;

    fn load_health_protection_statuses(
        &self,
        now_ms: i64,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<HealthProtectionStatus>, RoutingExecutionReadError>,
    >;

    fn begin_health_protection_probe(
        &self,
        scope: HealthProtectionScope,
        now_ms: i64,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<HealthProtectionProbe>, RoutingExecutionReadError>,
    >;

    fn cancel_health_protection_probe(
        &self,
        probe: HealthProtectionProbe,
        now_ms: i64,
    ) -> futures_util::future::BoxFuture<'static, Result<bool, RoutingExecutionReadError>>;
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub(crate) enum RoutingExecutionReadError {
    #[error("routing execution read deadline exceeded")]
    DeadlineExceeded,
    #[error("routing execution data unavailable: {0}")]
    Unavailable(String),
    #[error("routing execution state invalid: {0}")]
    InvalidState(String),
    #[error("routing execution read failed: {0}")]
    Internal(String),
}

impl RoutingExecutionReadError {
    fn from_application(error: ApplicationError) -> Self {
        match error {
            ApplicationError::DeadlineExceeded => Self::DeadlineExceeded,
            ApplicationError::Unavailable
            | ApplicationError::NotFound
            | ApplicationError::IoFailed => Self::Unavailable(error.to_string()),
            ApplicationError::ConstraintViolation
            | ApplicationError::StaleRevision
            | ApplicationError::IncompatibleSchema => Self::InvalidState(error.to_string()),
            other => Self::Internal(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_failures_keep_stable_execution_categories() {
        assert_eq!(
            RoutingExecutionReadError::from_application(ApplicationError::DeadlineExceeded),
            RoutingExecutionReadError::DeadlineExceeded
        );
        assert!(matches!(
            RoutingExecutionReadError::from_application(ApplicationError::Unavailable),
            RoutingExecutionReadError::Unavailable(_)
        ));
        assert!(matches!(
            RoutingExecutionReadError::from_application(ApplicationError::ConstraintViolation),
            RoutingExecutionReadError::InvalidState(_)
        ));
    }
}

/// Production adapter. The proxy receives this narrow port rather than the
/// broad routing command/query service; the adapter is the only place where
/// those legacy application methods are composed for request execution.
#[derive(Clone)]
pub(crate) struct RoutingExecutionReader {
    routing: Arc<RoutingService>,
}

impl RoutingExecutionReader {
    pub(crate) fn new(routing: Arc<RoutingService>) -> Self {
        Self { routing }
    }
}

impl RoutingExecutionReadPort for RoutingExecutionReader {
    #[cfg(test)]
    fn load_planning_snapshot(
        &self,
        request: RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
        context: PlanningRequestContext,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<PlanningSnapshot>, RoutingExecutionReadError>,
    > {
        let routing = Arc::clone(&self.routing);
        Box::pin(async move {
            routing
                .load_intelligent_planning_snapshot(&request, runtime, context)
                .await
                .map_err(RoutingExecutionReadError::from_application)
        })
    }

    fn load_planning_snapshot_with_probe(
        &self,
        request: RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
        context: PlanningRequestContext,
        probe: Option<HealthProtectionProbe>,
        probe_mode: HealthProbeAdmissionMode,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<PlanningSnapshot>, RoutingExecutionReadError>,
    > {
        let routing = Arc::clone(&self.routing);
        Box::pin(async move {
            routing
                .load_intelligent_planning_snapshot_with_probe(
                    &request, runtime, context, probe, probe_mode,
                )
                .await
                .map_err(RoutingExecutionReadError::from_application)
        })
    }

    fn load_execution_settings(
        &self,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<RuntimeRoutingSettings, RoutingExecutionReadError>,
    > {
        let routing = Arc::clone(&self.routing);
        Box::pin(async move {
            routing
                .load_execution_settings()
                .await
                .map_err(RoutingExecutionReadError::from_application)
        })
    }

    fn load_balance_snapshots(
        &self,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<BalanceSnapshot>, RoutingExecutionReadError>,
    > {
        let routing = Arc::clone(&self.routing);
        Box::pin(async move {
            routing
                .list_balance_snapshots()
                .await
                .map_err(RoutingExecutionReadError::from_application)
        })
    }

    fn load_operational_execution_target_refs(
        &self,
        station_key_ids: Vec<String>,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<ExecutionTargetRef>, RoutingExecutionReadError>,
    > {
        let routing = Arc::clone(&self.routing);
        Box::pin(async move {
            routing
                .load_operational_execution_target_refs(station_key_ids)
                .await
                .map_err(RoutingExecutionReadError::from_application)
        })
    }

    fn load_health_protection_statuses(
        &self,
        now_ms: i64,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<HealthProtectionStatus>, RoutingExecutionReadError>,
    > {
        let routing = Arc::clone(&self.routing);
        Box::pin(async move {
            routing
                .load_health_protection_statuses(now_ms)
                .await
                .map_err(RoutingExecutionReadError::from_application)
        })
    }

    fn begin_health_protection_probe(
        &self,
        scope: HealthProtectionScope,
        now_ms: i64,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<HealthProtectionProbe>, RoutingExecutionReadError>,
    > {
        let routing = Arc::clone(&self.routing);
        Box::pin(async move {
            routing
                .begin_health_protection_probe(scope, now_ms)
                .await
                .map_err(RoutingExecutionReadError::from_application)
        })
    }

    fn cancel_health_protection_probe(
        &self,
        probe: HealthProtectionProbe,
        now_ms: i64,
    ) -> futures_util::future::BoxFuture<'static, Result<bool, RoutingExecutionReadError>> {
        let routing = Arc::clone(&self.routing);
        Box::pin(async move {
            routing
                .cancel_health_protection_probe(probe, now_ms)
                .await
                .map_err(RoutingExecutionReadError::from_application)
        })
    }
}
