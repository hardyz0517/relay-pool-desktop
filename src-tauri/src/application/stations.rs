use std::sync::Arc;

use crate::{
    application::{clock::Clock, error::ApplicationError, ids::IdGenerator},
    models::stations::{CreateStationInput, Station, UpdateStationInput},
    persistence::{
        runtime::PersistenceHandle,
        stores::{
            alerting::IncidentStore,
            station_catalog::{NewStationRow, StationCatalogStore, StationChange},
        },
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

    #[cfg(test)]
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
        let incident_store = IncidentStore;
        let now_ms = self.clock.now_utc().timestamp_millis();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    incident_store
                        .resolve_for_deleted_station(write, &station_id, now_ms)
                        .await?;
                    store.delete_owned_state(write, &station_id).await
                })
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        application::queries::change_center_workspace::ChangeCenterWorkspaceQuery,
        application::{clock::SystemClock, ids::UuidV7Generator},
        models::stations::CreateStationInput,
        persistence::{error::PersistenceError, runtime::PersistenceRuntime},
    };

    use super::StationService;

    #[tokio::test]
    async fn deleting_station_resolves_its_active_alerts_and_suppresses_deliveries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("station-delete.sqlite3"))
                .await
                .expect("runtime");
        let service = StationService::new(
            runtime.handle(),
            Arc::new(SystemClock),
            Arc::new(UuidV7Generator),
        );
        let station = service
            .create(CreateStationInput {
                name: "Delete fixture".to_string(),
                station_type: "sub2api".to_string(),
                website_url: "https://station-delete.example".to_string(),
                api_base_url: "https://station-delete.example/v1".to_string(),
                api_key: String::new(),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 30,
                note: None,
            })
            .await
            .expect("create station");

        let station_id = station.id.clone();
        let fixture_station_id = station_id.clone();
        runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO change_incidents (
                            id, condition_key, event_type, lifecycle_state, base_severity, severity,
                            object_type, object_id, station_id, lifecycle_policy_fingerprint,
                            episode_number, first_seen_at_ms, last_seen_at_ms,
                            last_observation_summary_json, created_at_ms, updated_at_ms
                         ) VALUES (?1, ?2, 'collector_failed', 'open', 'warning', 'warning',
                                   'station', ?3, ?3, 'fixture', 1, 100, 100, '{}', 100, 100)",
                    )
                    .bind("incident-station-delete")
                    .bind("station:delete-fixture")
                    .bind(&fixture_station_id)
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        "INSERT INTO notification_deliveries (
                            id, delivery_key, incident_id, episode_number, delivery_sequence,
                            policy_snapshot_json, channel, delivery_kind, status,
                            scheduled_at_ms, created_at_ms, updated_at_ms
                         ) VALUES ('delivery-station-delete', 'delivery-key-station-delete',
                                   'incident-station-delete', 1, 1, '{}', 'in_app',
                                   'opened', 'scheduled', 100, 100, 100)",
                    )
                    .execute(write.connection())
                    .await?;
                    Ok::<(), PersistenceError>(())
                })
            })
            .await
            .expect("insert alerting fixture");

        service
            .delete(station_id.clone())
            .await
            .expect("delete station");

        let mut read = runtime.handle().begin_read().await.expect("read session");
        let lifecycle = sqlx::query_scalar::<_, String>(
            "SELECT lifecycle_state FROM change_incidents WHERE id = 'incident-station-delete'",
        )
        .fetch_one(read.connection())
        .await
        .expect("incident lifecycle");
        let delivery = sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT status, suppressed_reason FROM notification_deliveries
             WHERE id = 'delivery-station-delete'",
        )
        .fetch_one(read.connection())
        .await
        .expect("delivery state");
        let station_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM stations WHERE id = ?1")
                .bind(station_id)
                .fetch_one(read.connection())
                .await
                .expect("station count");

        assert_eq!(lifecycle, "resolved");
        assert_eq!(
            delivery,
            ("suppressed".to_string(), Some("stale_episode".to_string()))
        );
        assert_eq!(station_count, 0);
    }

    #[tokio::test]
    async fn listing_alerts_resolves_legacy_orphaned_station_alerts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&temp.path().join("orphan-alert.sqlite3"))
            .await
            .expect("runtime");
        let service = StationService::new(
            runtime.handle(),
            Arc::new(SystemClock),
            Arc::new(UuidV7Generator),
        );
        let station = service
            .create(CreateStationInput {
                name: "Legacy orphan fixture".to_string(),
                station_type: "sub2api".to_string(),
                website_url: "https://orphan.example".to_string(),
                api_base_url: "https://orphan.example/v1".to_string(),
                api_key: String::new(),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 30,
                note: None,
            })
            .await
            .expect("create station");
        let station_id = station.id;

        service
            .delete(station_id.clone())
            .await
            .expect("delete station");
        let fixture_station_id = station_id.clone();
        runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO change_incidents (
                            id, condition_key, event_type, lifecycle_state, base_severity, severity,
                            object_type, object_id, lifecycle_policy_fingerprint,
                            episode_number, first_seen_at_ms, last_seen_at_ms,
                            last_observation_summary_json, created_at_ms, updated_at_ms
                         ) VALUES ('legacy-orphan-incident', ?1, 'collector_failed', 'open',
                                   'warning', 'warning', 'station', ?2, 'fixture', 1, 100, 100,
                                   '{}', 100, 100)",
                    )
                    .bind(format!(
                        "collector:{fixture_station_id}:collector_failed:balance"
                    ))
                    .bind(&fixture_station_id)
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        "INSERT INTO notification_deliveries (
                            id, delivery_key, incident_id, episode_number, delivery_sequence,
                            policy_snapshot_json, channel, delivery_kind, status,
                            scheduled_at_ms, created_at_ms, updated_at_ms
                         ) VALUES ('legacy-orphan-delivery', 'legacy-orphan-delivery-key',
                                   'legacy-orphan-incident', 1, 1, '{}', 'in_app',
                                   'opened', 'scheduled', 100, 100, 100)",
                    )
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        "INSERT INTO change_incidents (
                            id, condition_key, event_type, lifecycle_state, base_severity, severity,
                            object_type, object_id, lifecycle_policy_fingerprint,
                            episode_number, first_seen_at_ms, last_seen_at_ms,
                            last_observation_summary_json, created_at_ms, updated_at_ms
                         ) VALUES ('legacy-orphan-key-incident', ?1, 'key_invalid', 'open',
                                   'critical', 'critical', 'station_key', ?2, 'fixture', 1, 100, 100,
                                   '{}', 100, 100)",
                    )
                    .bind(format!("key:{fixture_station_id}"))
                    .bind(&fixture_station_id)
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        "INSERT INTO notification_deliveries (
                            id, delivery_key, incident_id, episode_number, delivery_sequence,
                            policy_snapshot_json, channel, delivery_kind, status,
                            scheduled_at_ms, created_at_ms, updated_at_ms
                         ) VALUES ('legacy-orphan-key-delivery', 'legacy-orphan-key-delivery-key',
                                   'legacy-orphan-key-incident', 1, 1, '{}', 'in_app',
                                   'opened', 'scheduled', 100, 100, 100)",
                    )
                    .execute(write.connection())
                    .await?;
                    Ok::<(), PersistenceError>(())
                })
            })
            .await
            .expect("insert orphan fixture");

        let query = ChangeCenterWorkspaceQuery::new(runtime.handle());
        let page = query
            .list_current(None, None, Some("active"), None, 100)
            .await
            .expect("list current alerts");
        assert!(page.items.is_empty());

        let mut read = runtime.handle().begin_read().await.expect("read session");
        let lifecycle = sqlx::query_scalar::<_, String>(
            "SELECT lifecycle_state FROM change_incidents WHERE id = 'legacy-orphan-incident'",
        )
        .fetch_one(read.connection())
        .await
        .expect("incident lifecycle");
        let delivery_status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM notification_deliveries WHERE id = 'legacy-orphan-delivery'",
        )
        .fetch_one(read.connection())
        .await
        .expect("delivery status");
        assert_eq!(lifecycle, "resolved");
        assert_eq!(delivery_status, "suppressed");
        let key_lifecycle = sqlx::query_scalar::<_, String>(
            "SELECT lifecycle_state FROM change_incidents WHERE id = 'legacy-orphan-key-incident'",
        )
        .fetch_one(read.connection())
        .await
        .expect("key incident lifecycle");
        let key_delivery_status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM notification_deliveries WHERE id = 'legacy-orphan-key-delivery'",
        )
        .fetch_one(read.connection())
        .await
        .expect("key delivery status");
        assert_eq!(key_lifecycle, "resolved");
        assert_eq!(key_delivery_status, "suppressed");
    }
}
