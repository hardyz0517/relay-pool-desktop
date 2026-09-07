//! Narrow read ports for routing workspace and runtime-overlay projections.
//!
//! `RoutingService` remains the transitional adapter implementation, while
//! command callers depend on the capabilities they need rather than its full
//! orchestration surface.

use futures_util::future::BoxFuture;
use std::sync::Arc;

use crate::application::{
    error::ApplicationError,
    queries::{
        routing_runtime::{RoutingRuntimeActivity, RoutingRuntimeOverlay},
        routing_workspace::{RoutingWorkspaceSnapshot, RoutingWorkspaceSnapshotInput},
    },
};

pub(crate) trait RoutingWorkspaceReadPort: Send + Sync {
    fn read_workspace_snapshot(
        &self,
        input: RoutingWorkspaceSnapshotInput,
    ) -> BoxFuture<'_, Result<RoutingWorkspaceSnapshot, ApplicationError>>;
}

pub(crate) trait RoutingRuntimeOverlayReadPort: Send + Sync {
    fn read_runtime_overlay(
        &self,
        activity: Arc<dyn RoutingRuntimeActivity>,
    ) -> BoxFuture<'_, Result<RoutingRuntimeOverlay, ApplicationError>>;
}

pub(crate) trait RoutingProtectionReadPort: Send + Sync {
    fn read_protection_status(
        &self,
        generated_at_ms: i64,
    ) -> BoxFuture<
        '_,
        Result<
            crate::application::queries::routing_protection::RoutingProtectionStatus,
            ApplicationError,
        >,
    >;
}

pub(crate) trait RoutingCircuitStatusReadPort: Send + Sync {
    fn read_circuit_status(
        &self,
        generated_at_ms: i64,
    ) -> BoxFuture<
        '_,
        Result<
            crate::application::queries::station_key_circuit_read::StationKeyCircuitReadSnapshot,
            ApplicationError,
        >,
    >;
}

pub(crate) trait RoutingSimulationReadPort: Send + Sync {
    fn read_simulation(
        &self,
        input: crate::models::routing::RouteSimulationInput,
    ) -> BoxFuture<'_, Result<crate::models::routing::RouteSimulationResult, ApplicationError>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn routing_read_ports_are_thread_safe_capability_boundaries() {
        assert_send_sync::<dyn RoutingWorkspaceReadPort>();
        assert_send_sync::<dyn RoutingRuntimeOverlayReadPort>();
        assert_send_sync::<dyn RoutingProtectionReadPort>();
        assert_send_sync::<dyn RoutingCircuitStatusReadPort>();
        assert_send_sync::<dyn RoutingSimulationReadPort>();
    }
}
