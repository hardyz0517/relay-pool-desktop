use std::{sync::Arc, time::Duration};

use crate::{
    application::{error::ApplicationError, routing::RoutingService},
    models::{
        pricing::BalanceSnapshot,
        routing::{
            ModelAlias, RouteSimulationInput, RouteSimulationResult, StationKeyHealth,
            UpsertModelAliasInput,
        },
        stations::{EndpointPingResult, StationEndpointHealth},
    },
    services::{
        endpoint_ping::ping_station_endpoint as probe_station_endpoint,
        time::now_millis_for_services,
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
}

impl RoutingCommandFacade {
    pub(crate) fn new(routing: Arc<RoutingService>) -> Self {
        Self { routing }
    }

    pub(crate) async fn list_model_aliases(&self) -> Result<Vec<ModelAlias>, ApplicationError> {
        self.routing.list_model_aliases().await
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
        let probe = tokio::task::spawn_blocking(move || {
            probe_station_endpoint(&api_base_url, Duration::from_secs(5))
        })
        .await
        .map_err(|_| EndpointPingCommandError::ResultUnknown)?;
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
