use std::sync::Arc;

use crate::{
    TrayBehavior, TrayBehaviorState,
    application::{
        error::ApplicationError,
        queries::{station_assets::StationAssetsQuery, station_detail::StationDetailQuery},
        settings::SettingsService,
        stations::StationService,
    },
    models::{
        settings::{AppSettings, UpdateSettingsInput},
        stations::{CreateStationInput, Station, UpdateStationInput},
    },
};

#[derive(Clone)]
pub(crate) struct SettingsStationsCommandFacade {
    stations: Arc<StationService>,
    settings: Arc<SettingsService>,
    tray_behavior: Arc<TrayBehaviorState>,
    station_assets: Arc<StationAssetsQuery>,
    station_detail: Arc<StationDetailQuery>,
}

impl SettingsStationsCommandFacade {
    pub(crate) fn new(
        stations: Arc<StationService>,
        settings: Arc<SettingsService>,
        station_assets: Arc<StationAssetsQuery>,
        station_detail: Arc<StationDetailQuery>,
        tray_behavior: Arc<TrayBehaviorState>,
    ) -> Self {
        Self {
            stations,
            settings,
            tray_behavior,
            station_assets,
            station_detail,
        }
    }

    pub(crate) async fn list_stations(&self) -> Result<Vec<Station>, ApplicationError> {
        let read_model = self
            .station_assets
            .load(crate::application::pagination::PageLimit::new(500).expect("bounded limit"))
            .await?;
        Ok(read_model
            .data
            .rows
            .into_iter()
            .map(|row| row.station)
            .collect())
    }

    pub(crate) async fn create_station(
        &self,
        input: CreateStationInput,
    ) -> Result<Station, ApplicationError> {
        self.stations.create(input).await
    }

    pub(crate) async fn load_station_detail(
        &self,
        station_id: String,
    ) -> Result<crate::models::routing_read_models::StationDetailReadModel, ApplicationError> {
        Ok(self.station_detail.load(&station_id).await?.data)
    }

    pub(crate) async fn update_station(
        &self,
        input: UpdateStationInput,
    ) -> Result<Station, ApplicationError> {
        self.stations.update_station(input).await
    }

    pub(crate) async fn delete_station(&self, station_id: String) -> Result<(), ApplicationError> {
        self.stations.delete(station_id).await
    }

    pub(crate) async fn reorder_stations(
        &self,
        station_ids: Vec<String>,
    ) -> Result<Vec<Station>, ApplicationError> {
        self.stations.reorder(station_ids).await
    }

    pub(crate) async fn get_settings(&self) -> Result<AppSettings, ApplicationError> {
        self.settings.load().await
    }

    pub(crate) async fn get_local_access_key(&self) -> Result<String, ApplicationError> {
        self.settings.ensure_local_access_key().await
    }

    pub(crate) async fn update_local_access_key(
        &self,
        value: String,
    ) -> Result<AppSettings, ApplicationError> {
        self.settings.update_local_access_key(value).await
    }

    pub(crate) async fn update_settings(
        &self,
        input: UpdateSettingsInput,
    ) -> Result<AppSettings, ApplicationError> {
        let settings = self.settings.update(input).await?;
        self.tray_behavior
            .set(TrayBehavior::from_setting(&settings.tray_behavior));
        Ok(settings)
    }
}
