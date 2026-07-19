use crate::{
    application::{error::ApplicationError, routing::RoutingService},
    models::routing::{ModelAlias, StationKeyHealth},
};

#[derive(Debug, Clone)]
pub(crate) struct StationAssetsView {
    pub(crate) model_aliases: Vec<ModelAlias>,
    pub(crate) station_key_health: Vec<StationKeyHealth>,
}

pub(crate) async fn load_station_assets(
    routing: &RoutingService,
) -> Result<StationAssetsView, ApplicationError> {
    Ok(StationAssetsView {
        model_aliases: routing.list_model_aliases().await?,
        station_key_health: routing.list_station_key_health().await?,
    })
}
