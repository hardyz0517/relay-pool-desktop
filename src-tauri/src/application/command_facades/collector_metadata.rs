use std::sync::Arc;

use crate::{
    application::{collectors::CollectorService, error::ApplicationError, pagination::PageLimit},
    models::{
        collector::CollectorSnapshot,
        collector_runs::CollectorRun,
        group_facts::{GroupRateRecord, StationGroupBinding, UpsertStationGroupBindingInput},
        shared_capabilities::StationGroupOption,
    },
};

#[derive(Clone)]
pub(crate) struct CollectorMetadataCommandFacade {
    collectors: Arc<CollectorService>,
}

impl CollectorMetadataCommandFacade {
    pub(crate) fn new(collectors: Arc<CollectorService>) -> Self {
        Self { collectors }
    }

    pub(crate) async fn list_station_group_bindings(
        &self,
        station_id: &str,
    ) -> Result<Vec<StationGroupBinding>, ApplicationError> {
        self.collectors
            .list_station_group_bindings(station_id)
            .await
    }

    pub(crate) async fn list_station_group_options(
        &self,
        station_id: &str,
        limit: PageLimit,
    ) -> Result<Vec<StationGroupOption>, ApplicationError> {
        self.collectors
            .list_station_group_options(station_id, limit)
            .await
    }

    pub(crate) async fn upsert_station_group_binding(
        &self,
        input: UpsertStationGroupBindingInput,
    ) -> Result<StationGroupBinding, ApplicationError> {
        self.collectors.upsert_station_group_binding(input).await
    }

    pub(crate) async fn list_group_rate_records(
        &self,
        station_id: &str,
        limit: PageLimit,
    ) -> Result<Vec<GroupRateRecord>, ApplicationError> {
        self.collectors
            .list_group_rate_records(station_id, limit)
            .await
    }

    pub(crate) async fn list_collector_runs(
        &self,
        station_id: &str,
        limit: PageLimit,
    ) -> Result<Vec<CollectorRun>, ApplicationError> {
        self.collectors.list_collector_runs(station_id, limit).await
    }

    pub(crate) async fn list_collector_snapshots(
        &self,
        station_id: &str,
        limit: PageLimit,
    ) -> Result<Vec<CollectorSnapshot>, ApplicationError> {
        self.collectors
            .list_station_snapshots(station_id, limit)
            .await
    }

    pub(crate) async fn get_latest_collector_snapshot(
        &self,
        station_id: &str,
    ) -> Result<Option<CollectorSnapshot>, ApplicationError> {
        self.collectors.latest_station_snapshot(station_id).await
    }
}
