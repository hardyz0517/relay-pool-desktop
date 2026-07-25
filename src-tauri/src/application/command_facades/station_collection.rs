use std::sync::Arc;

use crate::{
    application::{
        collectors::CollectorService, credentials::CredentialService, error::ApplicationError,
        settings::SettingsService,
    },
    models::collector::CollectorRunResult,
    services::collectors::{self, adapters::CollectorTask, V2CollectorSourceAdapter},
};

#[derive(Debug)]
pub(crate) enum StationCollectionCommandError {
    Prepare(ApplicationError),
    Apply(ApplicationError),
    Internal,
}

#[derive(Clone)]
pub(crate) struct StationCollectionCommandFacade {
    collectors: Arc<CollectorService>,
    credentials: Arc<CredentialService>,
    settings: Arc<SettingsService>,
    data_key: [u8; 32],
}

impl StationCollectionCommandFacade {
    pub(crate) fn new(
        collectors: Arc<CollectorService>,
        credentials: Arc<CredentialService>,
        settings: Arc<SettingsService>,
        data_key: [u8; 32],
    ) -> Self {
        Self {
            collectors,
            credentials,
            settings,
            data_key,
        }
    }

    pub(crate) async fn run_station_collection(
        &self,
        station_id: String,
        task: CollectorTask,
    ) -> Result<CollectorRunResult, StationCollectionCommandError> {
        let source = self.source();
        let data_key = self.data_key;
        let prepared = tokio::task::spawn_blocking(move || {
            collectors::prepare_station_collection_v2(&source, &data_key, station_id, task)
        })
        .await
        .map_err(|_| StationCollectionCommandError::Internal)?
        .map_err(StationCollectionCommandError::Prepare)?;
        self.apply_prepared_collection(prepared).await
    }

    pub(crate) async fn test_station_login(
        &self,
        station_id: String,
    ) -> Result<CollectorRunResult, StationCollectionCommandError> {
        let source = self.source();
        let data_key = self.data_key;
        let prepared = tokio::task::spawn_blocking(move || {
            collectors::prepare_station_login_test_v2(&source, &data_key, station_id)
        })
        .await
        .map_err(|_| StationCollectionCommandError::Internal)?
        .map_err(StationCollectionCommandError::Prepare)?;
        self.apply_prepared_collection(prepared).await
    }

    fn source(&self) -> V2CollectorSourceAdapter {
        V2CollectorSourceAdapter::new(
            Arc::clone(&self.collectors),
            Arc::clone(&self.credentials),
            Arc::clone(&self.settings),
        )
    }

    async fn apply_prepared_collection(
        &self,
        prepared: collectors::PreparedStationCollection,
    ) -> Result<CollectorRunResult, StationCollectionCommandError> {
        let apply = collectors::apply::V2CollectorApplyAdapter::new((*self.collectors).clone());
        collectors::apply_prepared_station_collection_v2(&self.collectors, &apply, prepared)
            .await
            .map_err(StationCollectionCommandError::Apply)
    }
}
