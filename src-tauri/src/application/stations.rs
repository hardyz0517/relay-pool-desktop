use std::sync::Arc;

use crate::{
    application::{clock::Clock, error::ApplicationError, ids::IdGenerator},
    models::stations::{CreateStationInput, Station, UpdateStationInput},
    persistence::{
        runtime::PersistenceHandle,
        stores::station_catalog::{NewStationRow, StationCatalogStore, StationChange},
    },
};

#[derive(Clone)]
pub(crate) struct StationService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    store: StationCatalogStore,
}

impl StationService {
    pub(crate) fn new(
        runtime: PersistenceHandle,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            runtime,
            clock,
            ids,
            store: StationCatalogStore,
        }
    }

    pub(crate) async fn list(&self) -> Result<Vec<Station>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store.list(&mut read).await.map_err(Into::into)
    }

    pub(crate) async fn station_for_capture(
        &self,
        station_id: &str,
    ) -> Result<Station, ApplicationError> {
        if station_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.store
            .get(&mut read, station_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn create(
        &self,
        input: CreateStationInput,
    ) -> Result<Station, ApplicationError> {
        let store = self.store;
        let row = NewStationRow {
            id: self.ids.next_id(),
            now: self.now_ms_string(),
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.insert(write, row).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update_station(
        &self,
        input: UpdateStationInput,
    ) -> Result<Station, ApplicationError> {
        let store = self.store;
        let change = StationChange {
            now: self.now_ms_string(),
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.update_if_revision(write, change).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn delete(&self, station_id: String) -> Result<(), ApplicationError> {
        let store = self.store;
        self.runtime
            .write(|write| {
                Box::pin(async move { store.delete_owned_state(write, &station_id).await })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn reorder(
        &self,
        station_ids: Vec<String>,
    ) -> Result<Vec<Station>, ApplicationError> {
        let store = self.store;
        let now = self.now_ms_string();
        self.runtime
            .write(|write| Box::pin(async move { store.reorder(write, &station_ids, &now).await }))
            .await
            .map_err(Into::into)
    }

    fn now_ms_string(&self) -> String {
        self.clock.now_utc().timestamp_millis().to_string()
    }
}
