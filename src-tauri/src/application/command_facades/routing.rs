use std::{sync::Arc, time::Duration};

use tokio_util::sync::CancellationToken;

use crate::{
    application::{
        error::ApplicationError,
        queries::{
            operational_detail::StationKeyOperationalDetail,
            request_decision_trace::{
                RecentRouteDecisionsInput, RecentRouteDecisionsPage, RequestDecisionTrace,
            },
            routing_runtime::{RoutingRuntimeActivity, RoutingRuntimeOverlay},
            routing_workspace::{RoutingWorkspaceSnapshot, RoutingWorkspaceSnapshotInput},
        },
        routing::RoutingService,
    },
    models::{
        pricing::BalanceSnapshot,
        routing::{
            ModelAlias, RouteSimulationInput, RouteSimulationResult, StationKeyHealth,
            UpsertModelAliasInput,
        },
        stations::{EndpointPingResult, StationEndpointHealth},
    },
    outbound::AsyncOutboundClient,
    services::{
        endpoint_ping::ping_station_endpoint as probe_station_endpoint,
        proxy::runtime::ProxyRuntimeState, time::now_millis_for_services,
    },
};

#[derive(Debug)]
pub(crate) enum EndpointPingCommandError {
    Application(ApplicationError),
    ResultUnknown,
}

impl From<ApplicationError> for EndpointPingCommandError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

#[derive(Clone)]
pub(crate) struct RoutingCommandFacade {
    routing: Arc<RoutingService>,
    outbound: AsyncOutboundClient,
    proxy: Arc<ProxyRuntimeState>,
}

impl RoutingCommandFacade {
    pub(crate) fn new(
        routing: Arc<RoutingService>,
        outbound: AsyncOutboundClient,
        proxy: Arc<ProxyRuntimeState>,
    ) -> Self {
        Self {
            routing,
            outbound,
            proxy,
        }
    }

    pub(crate) async fn list_model_aliases(&self) -> Result<Vec<ModelAlias>, ApplicationError> {
        self.routing.list_model_aliases().await
    }

    pub(crate) async fn load_routing_policy(
        &self,
    ) -> Result<
        crate::persistence::stores::routing_policy_store::StoredRoutingPolicy,
        ApplicationError,
    > {
        self.routing.load_routing_policy().await
    }

    pub(crate) async fn save_routing_policy(
        &self,
        config: crate::models::routing_policy::RoutingPolicyConfigV1,
        expected_revision: Option<u64>,
    ) -> Result<
        crate::persistence::stores::routing_policy_store::StoredRoutingPolicy,
        ApplicationError,
    > {
        self.routing
            .save_routing_policy(config, expected_revision)
            .await
    }

    pub(crate) async fn upsert_model_alias(
        &self,
        input: UpsertModelAliasInput,
    ) -> Result<ModelAlias, ApplicationError> {
        self.routing.upsert_model_alias(input).await
    }

    pub(crate) async fn delete_model_alias(&self, id: String) -> Result<(), ApplicationError> {
        self.routing.delete_model_alias(id).await
    }

    pub(crate) async fn list_station_key_health(
        &self,
    ) -> Result<Vec<StationKeyHealth>, ApplicationError> {
        self.routing.list_station_key_health().await
    }

    pub(crate) async fn list_station_endpoint_health(
        &self,
    ) -> Result<Vec<StationEndpointHealth>, ApplicationError> {
        self.routing.list_station_endpoint_health().await
    }

    pub(crate) async fn load_routing_workspace_snapshot(
        &self,
        input: RoutingWorkspaceSnapshotInput,
    ) -> Result<RoutingWorkspaceSnapshot, ApplicationError> {
        self.routing.load_routing_workspace_snapshot(input).await
    }

    pub(crate) async fn load_routing_runtime_overlay(
        &self,
    ) -> Result<RoutingRuntimeOverlay, ApplicationError> {
        let proxy: Arc<dyn RoutingRuntimeActivity> = self.proxy.clone();
        self.routing.load_routing_runtime_overlay(proxy).await
    }

    pub(crate) async fn list_recent_route_decisions(
        &self,
        input: RecentRouteDecisionsInput,
    ) -> Result<RecentRouteDecisionsPage, ApplicationError> {
        self.routing.list_recent_route_decisions(input).await
    }

    pub(crate) async fn get_station_key_operational_detail(
        &self,
        station_key_id: String,
    ) -> Result<StationKeyOperationalDetail, ApplicationError> {
        self.routing
            .get_station_key_operational_detail(station_key_id)
            .await
    }

    pub(crate) async fn get_request_decision_trace(
        &self,
        request_log_id: String,
    ) -> Result<RequestDecisionTrace, ApplicationError> {
        self.routing
            .get_request_decision_trace(request_log_id)
            .await
    }

    pub(crate) async fn simulate_route(
        &self,
        input: RouteSimulationInput,
    ) -> Result<RouteSimulationResult, ApplicationError> {
        self.routing.simulate_route(input).await
    }

    pub(crate) async fn list_balance_snapshots_for_station(
        &self,
        station_id: &str,
    ) -> Result<Vec<BalanceSnapshot>, ApplicationError> {
        self.routing
            .list_balance_snapshots_for_station(station_id)
            .await
    }

    pub(crate) async fn get_station_key_health(
        &self,
        station_key_id: String,
    ) -> Result<StationKeyHealth, ApplicationError> {
        self.routing.station_key_health_by_id(&station_key_id).await
    }

    pub(crate) async fn ping_station_endpoint(
        &self,
        station_id: String,
    ) -> Result<EndpointPingResult, EndpointPingCommandError> {
        let target = self
            .routing
            .station_endpoint_probe_target(&station_id)
            .await?;
        let checked_at = now_millis_for_services().to_string();
        let api_base_url = target.api_base_url.clone();
        let probe = probe_station_endpoint(
            &self.outbound,
            &api_base_url,
            Duration::from_secs(5),
            CancellationToken::new(),
        )
        .await;
        let health = self
            .routing
            .record_station_endpoint_health(
                target.station_id,
                target.endpoint_revision,
                probe.status,
                probe.latency_ms,
                checked_at.clone(),
                probe.error_summary,
            )
            .await
            .map_err(|_| EndpointPingCommandError::ResultUnknown)?;
        Ok(EndpointPingResult {
            station_id: health.station_id,
            ok: probe.ok,
            status: health.status,
            latency_ms: health.latency_ms,
            checked_at: health.checked_at.unwrap_or(checked_at),
            error_summary: health.error_summary,
        })
    }
}
