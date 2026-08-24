use std::{num::NonZeroUsize, sync::Arc};

use crate::{
    application::{
        error::ApplicationError, queries::station_assets::StationAssetsQuery,
        settings::SettingsService, station_capacity_domains::StationCapacityDomainService,
        stations::StationService,
    },
    models::{
        settings::{AppSettings, UpdateSettingsInput},
        station_capacity_domains::{
            ClearStationCapacityDomainInput, StationCapacityDomain,
            UpsertStationCapacityDomainInput,
        },
        stations::{CreateStationInput, Station, UpdateStationInput},
    },
    services::station_collection_coordinator::StationCollectionCoordinator,
    TrayBehavior, TrayBehaviorState,
};

#[derive(Clone)]
pub(crate) struct SettingsStationsCommandFacade {
    stations: Arc<StationService>,
    settings: Arc<SettingsService>,
    tray_behavior: Arc<TrayBehaviorState>,
    station_assets: Arc<StationAssetsQuery>,
    station_capacity_domains: Arc<StationCapacityDomainService>,
    station_collection_coordinator: StationCollectionCoordinator,
}

impl SettingsStationsCommandFacade {
    pub(crate) fn new(
        stations: Arc<StationService>,
        settings: Arc<SettingsService>,
        station_assets: Arc<StationAssetsQuery>,
        station_capacity_domains: Arc<StationCapacityDomainService>,
        tray_behavior: Arc<TrayBehaviorState>,
        station_collection_coordinator: StationCollectionCoordinator,
    ) -> Self {
        Self {
            stations,
            settings,
            tray_behavior,
            station_assets,
            station_capacity_domains,
            station_collection_coordinator,
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

    pub(crate) async fn get_station_capacity_domain(
        &self,
        station_id: String,
    ) -> Result<Option<StationCapacityDomain>, ApplicationError> {
        self.station_capacity_domains.get(station_id).await
    }

    pub(crate) async fn upsert_station_capacity_domain(
        &self,
        input: UpsertStationCapacityDomainInput,
    ) -> Result<StationCapacityDomain, ApplicationError> {
        self.station_capacity_domains.upsert(input).await
    }

    pub(crate) async fn clear_station_capacity_domain(
        &self,
        input: ClearStationCapacityDomainInput,
    ) -> Result<(), ApplicationError> {
        self.station_capacity_domains.clear(input).await
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
        let settings = persist_and_apply_collection_runtime_settings(
            self.settings.as_ref(),
            &self.station_collection_coordinator,
            input,
        )
        .await?;
        self.tray_behavior
            .set(TrayBehavior::from_setting(&settings.tray_behavior));
        Ok(settings)
    }
}

async fn persist_and_apply_collection_runtime_settings(
    settings_service: &SettingsService,
    coordinator: &StationCollectionCoordinator,
    input: UpdateSettingsInput,
) -> Result<AppSettings, ApplicationError> {
    let settings = settings_service.update(input).await?;
    coordinator.set_max_concurrency(validated_collection_limit(&settings));
    Ok(settings)
}

fn validated_collection_limit(settings: &AppSettings) -> NonZeroUsize {
    NonZeroUsize::new(usize::from(settings.collector_max_concurrency))
        .expect("settings store validates collector concurrency as non-zero")
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroUsize, sync::Arc};

    use crate::{
        application::{clock::SystemClock, ids::UuidV7Generator},
        persistence::runtime::PersistenceRuntime,
        services::{
            secrets::vault::DataKeyVault,
            station_collection_coordinator::StationCollectionCoordinator,
        },
    };

    use super::*;

    #[tokio::test]
    async fn settings_persistence_updates_runtime_limit_only_after_success() {
        let temporary_directory = tempfile::tempdir().expect("temporary directory");
        let runtime = PersistenceRuntime::initialize_new(
            &temporary_directory.path().join("settings.sqlite3"),
        )
        .await
        .expect("runtime");
        let settings_service = SettingsService::new(
            runtime.handle(),
            Arc::new(SystemClock),
            Arc::new(UuidV7Generator),
            Arc::new(DataKeyVault::for_test([7; 32])),
            temporary_directory.path().display().to_string(),
            None,
        );
        let coordinator = StationCollectionCoordinator::new(
            NonZeroUsize::new(3).expect("non-zero coordinator limit"),
        );

        let mut successful_input = settings_input();
        successful_input.collector_max_concurrency = 1;
        let updated = persist_and_apply_collection_runtime_settings(
            &settings_service,
            &coordinator,
            successful_input,
        )
        .await
        .expect("settings update succeeds");
        assert_eq!(updated.collector_max_concurrency, 1);
        assert_eq!(coordinator.max_concurrency().get(), 1);
        assert_eq!(
            settings_service
                .load()
                .await
                .expect("settings reload")
                .collector_max_concurrency,
            1
        );

        let mut invalid_input = settings_input();
        invalid_input.collector_max_concurrency = 0;
        assert!(persist_and_apply_collection_runtime_settings(
            &settings_service,
            &coordinator,
            invalid_input,
        )
        .await
        .is_err());
        assert_eq!(coordinator.max_concurrency().get(), 1);
        assert_eq!(
            settings_service
                .load()
                .await
                .expect("settings remain persisted")
                .collector_max_concurrency,
            1
        );

        runtime.close().await.expect("runtime closes");
    }

    fn settings_input() -> UpdateSettingsInput {
        UpdateSettingsInput {
            local_proxy_port: 8787,
            routing_policy_name: "cost_stable_first".to_string(),
            collector_proxy_mode: "direct".to_string(),
            collector_proxy_url: None,
            max_rate_multiplier: None,
            routing_group_scope: None,
            scheduler_config: None,
            low_balance_threshold_cny: 15.0,
            collector_interval_minutes: 30,
            balance_interval_minutes: 5,
            group_rate_interval_minutes: 20,
            published_status_interval_minutes: 5,
            pricing_refresh_interval_minutes: 60,
            collector_timeout_seconds: 15,
            collector_max_concurrency: 3,
            allow_depleted_fallback: false,
            developer_mode_enabled: false,
            show_decision_explanation: false,
            tray_behavior: None,
        }
    }
}
