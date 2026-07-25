use std::sync::Arc;

use crate::{
    application::{error::ApplicationError, routing::RoutingService},
    models::{
        routing::{ModelAlias, StationKeyHealth, UpsertModelAliasInput},
        stations::StationEndpointHealth,
    },
};

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

    pub(crate) async fn get_station_key_health(
        &self,
        station_key_id: String,
    ) -> Result<StationKeyHealth, ApplicationError> {
        self.routing.station_key_health_by_id(&station_key_id).await
    }
}
