use std::collections::HashSet;

use crate::{
    application::{
        error::ApplicationError,
        pagination::{PageLimit, MAX_PAGE_LIMIT},
    },
    models::{collector::CollectorSnapshot, collector_runs::CollectorRun},
    persistence::{
        runtime::PersistenceHandle,
        stores::{collector_store::CollectorStore, station_catalog::StationCatalogStore},
        ReadSession,
    },
};

/// Read-only owner for collector history.
///
/// Collector history is immutable evidence.  It is intentionally kept out of
/// `CollectorService`, whose remaining surface is command orchestration and
/// terminal commits.  The `_in_session` methods are used by composite read
/// models so rows and their surrounding station data come from one snapshot.
#[derive(Clone)]
pub(crate) struct CollectorHistoryQuery {
    runtime: PersistenceHandle,
    collectors: CollectorStore,
    stations: StationCatalogStore,
}

impl CollectorHistoryQuery {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            collectors: CollectorStore,
            stations: StationCatalogStore,
        }
    }

    pub(crate) async fn list_collector_runs(
        &self,
        station_id: &str,
        limit: PageLimit,
    ) -> Result<Vec<CollectorRun>, ApplicationError> {
        validate_station_id(station_id)?;
        let mut read = self.runtime.begin_read().await?;
        self.stations.get(&mut read, station_id).await?;
        self.list_collector_runs_in_session(&mut read, station_id, limit.get())
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_station_snapshots(
        &self,
        station_id: &str,
        limit: PageLimit,
    ) -> Result<Vec<CollectorSnapshot>, ApplicationError> {
        validate_station_id(station_id)?;
        let mut read = self.runtime.begin_read().await?;
        self.list_station_snapshots_in_session(&mut read, station_id, i64::from(limit.get()))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn latest_station_snapshot(
        &self,
        station_id: &str,
    ) -> Result<Option<CollectorSnapshot>, ApplicationError> {
        validate_station_id(station_id)?;
        let mut read = self.runtime.begin_read().await?;
        self.latest_station_snapshot_in_session(&mut read, station_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_latest_station_snapshots(
        &self,
        station_ids: Vec<String>,
    ) -> Result<Vec<CollectorSnapshot>, ApplicationError> {
        if station_ids.is_empty() {
            return Ok(Vec::new());
        }
        if station_ids.len() > MAX_PAGE_LIMIT as usize {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        self.list_latest_station_snapshots_in_session(&mut read, station_ids)
            .await
    }

    pub(crate) async fn list_collector_runs_in_session(
        &self,
        read: &mut ReadSession,
        station_id: &str,
        limit: u32,
    ) -> Result<Vec<CollectorRun>, crate::persistence::error::PersistenceError> {
        self.collectors
            .list_collector_runs(read, station_id, limit)
            .await
    }

    pub(crate) async fn list_station_snapshots_in_session(
        &self,
        read: &mut ReadSession,
        station_id: &str,
        limit: i64,
    ) -> Result<Vec<CollectorSnapshot>, crate::persistence::error::PersistenceError> {
        self.collectors
            .list_station_snapshots(read, station_id, limit)
            .await
    }

    pub(crate) async fn latest_station_snapshot_in_session(
        &self,
        read: &mut ReadSession,
        station_id: &str,
    ) -> Result<Option<CollectorSnapshot>, crate::persistence::error::PersistenceError> {
        self.collectors
            .latest_station_snapshot(read, station_id)
            .await
    }

    pub(crate) async fn list_latest_station_snapshots_in_session(
        &self,
        read: &mut ReadSession,
        station_ids: Vec<String>,
    ) -> Result<Vec<CollectorSnapshot>, ApplicationError> {
        if station_ids.is_empty() {
            return Ok(Vec::new());
        }
        if station_ids.len() > MAX_PAGE_LIMIT as usize {
            return Err(ApplicationError::ConstraintViolation);
        }

        let mut unique = HashSet::with_capacity(station_ids.len());
        for station_id in &station_ids {
            validate_station_id(station_id)?;
            if !unique.insert(station_id.as_str()) {
                return Err(ApplicationError::ConstraintViolation);
            }
        }

        let mut snapshots = Vec::new();
        for station_id in station_ids {
            if let Some(snapshot) = self
                .latest_station_snapshot_in_session(read, &station_id)
                .await
                .map_err(ApplicationError::from)?
            {
                snapshots.push(snapshot);
            }
        }
        snapshots.sort_by(|left, right| left.station_id.cmp(&right.station_id));
        Ok(snapshots)
    }
}

fn validate_station_id(station_id: &str) -> Result<(), ApplicationError> {
    if station_id.trim().is_empty() {
        Err(ApplicationError::ConstraintViolation)
    } else {
        Ok(())
    }
}
