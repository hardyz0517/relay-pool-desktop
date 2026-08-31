use std::sync::Arc;

use crate::{
    application::{
        error::ApplicationError,
        operational_facts::target_resolver::ExecutionTargetRef,
        routing::RoutingService,
        routing_engine::planning_snapshot::{PlanningSnapshot, RuntimeOverlaySnapshot},
        routing_engine::request::{PlanningRequestContext, RouteRequestFacts},
        station_key_circuit::{CircuitAdmissionResult, StationKeyCircuitStatus},
    },
    models::{pricing::BalanceSnapshot, routing::RuntimeRoutingSettings},
};

/// Stable capabilities exposed from the application layer to the proxy
/// execution boundary. This intentionally contains reads needed while a
/// request is running. Legacy scoped-health reads and probes are deliberately
/// absent: v3 station-key circuit state is the sole production admission path.
pub(crate) trait RoutingExecutionReadPort: Send + Sync {
    fn load_planning_snapshot(
        &self,
        request: RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
        context: PlanningRequestContext,
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

    fn admit_station_key_circuit_with_attempt(
        &self,
        _expected_runtime_generation_id: Option<String>,
        _expected_fence_revision: u64,
        _station_key_id: String,
        _lifecycle_revision: u64,
        _policy_revision: u64,
        _now_ms: u64,
        _deadline_at_ms: u64,
        _score_gate_passed: bool,
        _attempt_id: String,
        _correlation_id: String,
        _attempt_index: u16,
        _capacity_lease_id: String,
        _consecutive_failure_threshold: u16,
        _recovery_success_threshold: u16,
        _recovery_wait_ms: u64,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<CircuitAdmissionResult, RoutingExecutionReadError>,
    > {
        Box::pin(async { Ok(CircuitAdmissionResult::AllowedClosed { state_revision: 1 }) })
    }

    fn load_station_key_circuit_statuses(
        &self,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<StationKeyCircuitStatus>, RoutingExecutionReadError>,
    > {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn load_routing_generation_admission_guard(
        &self,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<
            crate::models::routing_generation::RoutingGenerationAdmissionGuard,
            RoutingExecutionReadError,
        >,
    >;

    /// Marks every durable attempt at the outbound boundary. Half-Open
    /// attempts also advance their circuit lease in the same transaction.
    /// Test ports default to a no-op because they do not persist attempts.
    fn mark_station_key_attempt_boundary(
        &self,
        _station_key_id: String,
        _lifecycle_revision: u64,
        _attempt_id: String,
        _lease_revision: Option<u64>,
        _now_ms: u64,
    ) -> futures_util::future::BoxFuture<'static, Result<bool, RoutingExecutionReadError>> {
        Box::pin(async { Ok(true) })
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub(crate) enum RoutingExecutionReadError {
    #[error("routing execution read deadline exceeded")]
    DeadlineExceeded,
    #[error("routing candidate count {actual} exceeds system limit {limit}")]
    CandidateLimitExceeded { actual: usize, limit: usize },
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
            ApplicationError::CandidateLimitExceeded { actual, limit } => {
                Self::CandidateLimitExceeded { actual, limit }
            }
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
        assert_eq!(
            RoutingExecutionReadError::from_application(ApplicationError::CandidateLimitExceeded {
                actual: 1_025,
                limit: 1_024,
            }),
            RoutingExecutionReadError::CandidateLimitExceeded {
                actual: 1_025,
                limit: 1_024,
            }
        );
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

    fn admit_station_key_circuit_with_attempt(
        &self,
        expected_runtime_generation_id: Option<String>,
        expected_fence_revision: u64,
        station_key_id: String,
        lifecycle_revision: u64,
        policy_revision: u64,
        now_ms: u64,
        deadline_at_ms: u64,
        score_gate_passed: bool,
        attempt_id: String,
        correlation_id: String,
        attempt_index: u16,
        capacity_lease_id: String,
        consecutive_failure_threshold: u16,
        recovery_success_threshold: u16,
        recovery_wait_ms: u64,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<CircuitAdmissionResult, RoutingExecutionReadError>,
    > {
        let routing = Arc::clone(&self.routing);
        Box::pin(async move {
            routing
                .admit_station_key_circuit_with_attempt(
                    expected_runtime_generation_id,
                    expected_fence_revision,
                    station_key_id,
                    lifecycle_revision,
                    policy_revision,
                    now_ms,
                    deadline_at_ms,
                    score_gate_passed,
                    attempt_id,
                    correlation_id,
                    attempt_index,
                    capacity_lease_id,
                    consecutive_failure_threshold,
                    recovery_success_threshold,
                    recovery_wait_ms,
                )
                .await
                .map_err(RoutingExecutionReadError::from_application)
        })
    }

    fn load_station_key_circuit_statuses(
        &self,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<StationKeyCircuitStatus>, RoutingExecutionReadError>,
    > {
        let routing = Arc::clone(&self.routing);
        Box::pin(async move {
            routing
                .load_station_key_circuit_statuses()
                .await
                .map_err(RoutingExecutionReadError::from_application)
        })
    }

    fn load_routing_generation_admission_guard(
        &self,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<
            crate::models::routing_generation::RoutingGenerationAdmissionGuard,
            RoutingExecutionReadError,
        >,
    > {
        let routing = Arc::clone(&self.routing);
        Box::pin(async move {
            routing
                .load_routing_generation_admission_guard()
                .await
                .map_err(RoutingExecutionReadError::from_application)
        })
    }

    fn mark_station_key_attempt_boundary(
        &self,
        station_key_id: String,
        lifecycle_revision: u64,
        attempt_id: String,
        lease_revision: Option<u64>,
        now_ms: u64,
    ) -> futures_util::future::BoxFuture<'static, Result<bool, RoutingExecutionReadError>> {
        let routing = Arc::clone(&self.routing);
        Box::pin(async move {
            routing
                .mark_station_key_attempt_boundary(
                    station_key_id,
                    lifecycle_revision,
                    attempt_id,
                    lease_revision,
                    now_ms,
                )
                .await
                .map_err(RoutingExecutionReadError::from_application)
        })
    }
}
