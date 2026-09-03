use super::{
    read_model_revision::load_station_detail_revision,
    station_assets::{
        load_station_state_summaries, missing_authorization_summary, missing_collection_summary,
        server_group_identity_hash,
    },
};
use crate::{
    application::{error::ApplicationError, queries::collector_history::CollectorHistoryQuery},
    models::routing_read_models::{
        ReadModelEnvelope, ReadModelPage, StationAssetReadRow, StationDetailIncident,
        StationDetailLimits, StationDetailReadModel, ASSET_READ_MODEL_SCHEMA_VERSION,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::{
            alerting::workspace::WorkspaceStore, collector_store::CollectorStore,
            credential_store::CredentialStore, routing_store::RoutingStore,
            station_catalog::StationCatalogStore,
        },
    },
};

const GROUP_BINDING_LIMIT: u32 = 500;
const GROUP_RATE_LIMIT: u32 = 500;
const COLLECTOR_RUN_LIMIT: u32 = 100;
const BALANCE_LIMIT: u32 = 200;
const INCIDENT_LIMIT: u32 = 100;

#[derive(Clone)]
pub(crate) struct StationDetailQuery {
    runtime: PersistenceHandle,
    stations: StationCatalogStore,
    credentials: CredentialStore,
    collectors: CollectorStore,
    collector_history: CollectorHistoryQuery,
    routing: RoutingStore,
    alerting: WorkspaceStore,
}

impl StationDetailQuery {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime: runtime.clone(),
            stations: StationCatalogStore,
            credentials: CredentialStore,
            collectors: CollectorStore,
            collector_history: CollectorHistoryQuery::new(runtime.clone()),
            routing: RoutingStore,
            alerting: WorkspaceStore,
        }
    }

    pub(crate) async fn load(
        &self,
        station_id: &str,
    ) -> Result<ReadModelEnvelope<StationDetailReadModel>, ApplicationError> {
        if station_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }

        let mut read = self.runtime.begin_read().await?;
        let station = self.stations.get(&mut read, station_id).await?;
        let domain_revision = load_station_detail_revision(&mut read, station_id).await?;
        let (mut collection_summaries, mut authorization_summaries) =
            load_station_state_summaries(&mut read, std::slice::from_ref(&station)).await?;
        let station_ids_json =
            serde_json::to_string(&[station_id]).map_err(|_| ApplicationError::Internal)?;
        let keys = self
            .credentials
            .list_key_pool_items_for_stations(&mut read, &station_ids_json)
            .await?;
        let group_identity_hashes = keys.iter().filter_map(server_group_identity_hash).collect();
        let collection_summary = collection_summaries
            .remove(station_id)
            .unwrap_or_else(|| missing_collection_summary(station.endpoint_revision));
        let authorization_summary = authorization_summaries
            .remove(station_id)
            .unwrap_or_else(|| missing_authorization_summary(station.endpoint_revision));

        let credentials = self
            .credentials
            .station_credentials(&mut read, station_id)
            .await?;
        let group_bindings = self
            .collectors
            .list_station_group_bindings(&mut read, station_id, GROUP_BINDING_LIMIT)
            .await?;
        let group_rates = self
            .collectors
            .list_group_rate_records(&mut read, station_id, GROUP_RATE_LIMIT)
            .await?;
        let collector_runs = self
            .collector_history
            .list_collector_runs_in_session(&mut read, station_id, COLLECTOR_RUN_LIMIT)
            .await?;
        let latest_snapshot = self
            .collector_history
            .latest_station_snapshot_in_session(&mut read, station_id)
            .await?;
        let balances = self
            .routing
            .list_balance_snapshots_for_station_bounded(&mut read, station_id, BALANCE_LIMIT)
            .await?;
        let (incident_rows, _, _) = self
            .alerting
            .list_current(
                &mut read,
                Some(station_id),
                None,
                None,
                None,
                None,
                INCIDENT_LIMIT,
            )
            .await?;
        let incidents = incident_rows
            .into_iter()
            .take(INCIDENT_LIMIT as usize)
            .map(station_detail_incident)
            .collect();

        Ok(ReadModelEnvelope {
            schema_version: ASSET_READ_MODEL_SCHEMA_VERSION,
            generated_at_ms: chrono::Utc::now().timestamp_millis(),
            domain_revision,
            page: ReadModelPage {
                limit: 1,
                returned: 1,
                next_cursor: None,
            },
            data: StationDetailReadModel {
                asset: StationAssetReadRow {
                    station,
                    keys,
                    group_identity_hashes,
                    collection_summary,
                    authorization_summary,
                },
                credentials,
                group_bindings,
                group_rates,
                collector_runs,
                latest_snapshot,
                balances,
                incidents,
                limits: StationDetailLimits {
                    group_bindings: GROUP_BINDING_LIMIT,
                    group_rates: GROUP_RATE_LIMIT,
                    collector_runs: COLLECTOR_RUN_LIMIT,
                    balances: BALANCE_LIMIT,
                    incidents: INCIDENT_LIMIT,
                },
            },
        })
    }

    pub(crate) async fn revision(&self, station_id: &str) -> Result<i64, ApplicationError> {
        if station_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        load_station_detail_revision(&mut read, station_id)
            .await
            .map_err(ApplicationError::from)
    }
}

fn station_detail_incident(
    row: crate::persistence::stores::alerting::workspace::WorkspaceIncidentRow,
) -> StationDetailIncident {
    let group_name = serde_json::from_str::<serde_json::Value>(&row.last_observation_summary_json)
        .ok()
        .and_then(|value| {
            value
                .get("groupName")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.len() <= 160)
                .map(str::to_owned)
        });
    StationDetailIncident {
        id: row.id,
        event_type: row.event_type,
        lifecycle_state: row.lifecycle_state,
        severity: row.severity,
        group_name,
        station_id: row.station_id,
        episode_number: row.episode_number,
        occurrence_count: row.occurrence_count,
        last_seen_at_ms: row.last_seen_at_ms,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        application::{clock::SystemClock, ids::UuidV7Generator, stations::StationService},
        models::stations::{CreateStationInput, Station},
        persistence::{error::PersistenceError, runtime::PersistenceRuntime},
    };

    #[test]
    fn limits_are_explicit_and_bounded() {
        assert_eq!(GROUP_BINDING_LIMIT, 500);
        assert_eq!(GROUP_RATE_LIMIT, 500);
        assert_eq!(COLLECTOR_RUN_LIMIT, 100);
        assert_eq!(BALANCE_LIMIT, 200);
        assert_eq!(INCIDENT_LIMIT, 100);
    }

    #[tokio::test]
    async fn empty_and_missing_station_ids_fail_closed() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let runtime = PersistenceRuntime::initialize_new(&directory.path().join("detail.sqlite3"))
            .await
            .expect("runtime");
        let query = StationDetailQuery::new(runtime.handle());

        assert!(matches!(
            query.load(" ").await,
            Err(ApplicationError::ConstraintViolation)
        ));
        assert!(matches!(
            query.load("missing-station").await,
            Err(ApplicationError::NotFound)
        ));

        runtime.close().await.expect("runtime closes");
    }

    #[tokio::test]
    async fn empty_detail_is_typed_and_all_history_collections_are_bounded() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let runtime = PersistenceRuntime::initialize_new(&directory.path().join("detail.sqlite3"))
            .await
            .expect("runtime");
        let station = create_station(&runtime).await;
        let detail = StationDetailQuery::new(runtime.handle())
            .load(&station.id)
            .await
            .expect("detail");

        assert_eq!(detail.page.limit, 1);
        assert_eq!(detail.page.returned, 1);
        assert_eq!(detail.data.asset.station.id, station.id);
        assert_eq!(detail.data.asset.collection_summary.status, "not_collected");
        assert_eq!(detail.data.asset.authorization_summary.status, "unknown");
        assert!(detail.data.asset.keys.is_empty());
        assert!(detail.data.group_bindings.is_empty());
        assert!(detail.data.group_rates.is_empty());
        assert!(detail.data.collector_runs.is_empty());
        assert!(detail.data.latest_snapshot.is_none());
        assert!(detail.data.balances.is_empty());
        assert!(detail.data.incidents.is_empty());
        assert_eq!(detail.data.limits.collector_runs, COLLECTOR_RUN_LIMIT);
        assert_eq!(detail.data.limits.balances, BALANCE_LIMIT);

        let first_revision = detail.domain_revision;
        let station_id = station.id.clone();
        runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query("UPDATE stations SET note = 'changed' WHERE id = ?1")
                        .bind(station_id)
                        .execute(write.connection())
                        .await?;
                    Ok::<(), PersistenceError>(())
                })
            })
            .await
            .expect("station update");
        let next_revision = StationDetailQuery::new(runtime.handle())
            .revision(&station.id)
            .await
            .expect("detail revision");
        assert!(next_revision > first_revision);

        runtime.close().await.expect("runtime closes");
    }

    #[tokio::test]
    async fn detail_revision_is_owned_by_triggers_for_every_read_source() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let runtime = PersistenceRuntime::initialize_new(&directory.path().join("detail.sqlite3"))
            .await
            .expect("runtime");
        let expected = [
            "stations",
            "station_keys",
            "key_capabilities",
            "endpoint_health",
            "authorization",
            "collection",
            "credentials",
            "station_secrets",
            "collector_runs",
            "collector_snapshots",
            "group_bindings",
            "group_rates",
            "balances",
            "incidents",
        ];
        let trigger_names = runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query_scalar::<_, String>(
                        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'station_detail_revision_%'",
                    )
                    .fetch_all(write.connection())
                    .await
                    .map_err(PersistenceError::from)
                })
            })
            .await
            .expect("detail triggers");
        for source in expected {
            assert!(
                trigger_names.iter().any(|name| name.contains(source)),
                "missing detail revision trigger for {source}"
            );
        }
        assert!(trigger_names.len() >= expected.len() * 2);

        runtime.close().await.expect("runtime closes");
    }

    #[tokio::test]
    async fn moving_a_detail_row_advances_both_station_revisions() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let runtime = PersistenceRuntime::initialize_new(&directory.path().join("detail.sqlite3"))
            .await
            .expect("runtime");
        let first = create_station(&runtime).await;
        let second = create_station(&runtime).await;
        let first_id = first.id.clone();
        let second_id = second.id.clone();

        runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO change_incidents (
                            id, condition_key, event_type, lifecycle_state, base_severity, severity,
                            object_type, station_id, lifecycle_policy_fingerprint, episode_number,
                            first_seen_at_ms, last_seen_at_ms, occurrence_count,
                            last_observation_summary_json, created_at_ms, updated_at_ms
                         ) VALUES ('detail-move', 'detail-move', 'collector_failed', 'open', 'warning',
                                   'warning', 'station', ?1, 'fixture', 1, 1, 1, 1, '{}', 1, 1)",
                    )
                    .bind(&first_id)
                    .execute(write.connection())
                    .await?;
                    Ok::<(), PersistenceError>(())
                })
            })
            .await
            .expect("incident fixture");

        let first_before = StationDetailQuery::new(runtime.handle())
            .revision(&first.id)
            .await
            .expect("first revision");
        let second_before = StationDetailQuery::new(runtime.handle())
            .revision(&second.id)
            .await
            .expect("second revision");

        runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE change_incidents SET station_id = ?1 WHERE id = 'detail-move'",
                    )
                    .bind(second_id)
                    .execute(write.connection())
                    .await?;
                    Ok::<(), PersistenceError>(())
                })
            })
            .await
            .expect("move incident");

        let first_after = StationDetailQuery::new(runtime.handle())
            .revision(&first.id)
            .await
            .expect("first revision after move");
        let second_after = StationDetailQuery::new(runtime.handle())
            .revision(&second.id)
            .await
            .expect("second revision after move");
        assert!(first_after > first_before);
        assert!(second_after > second_before);

        runtime.close().await.expect("runtime closes");
    }

    #[tokio::test]
    async fn malformed_collection_projection_fails_the_detail_read() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let runtime = PersistenceRuntime::initialize_new(&directory.path().join("detail.sqlite3"))
            .await
            .expect("runtime");
        let station = create_station(&runtime).await;
        let station_id = station.id.clone();
        runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO station_collection_projection (
                            station_id, status, reason_codes_json, revision,
                            endpoint_revision, credential_revision, intent_sequence,
                            operation_id, updated_at_ms
                         ) VALUES (?1, 'degraded', '{\"unexpected\":true}', 1, 1, 1, 1, 'test', 1)",
                    )
                    .bind(station_id)
                    .execute(write.connection())
                    .await?;
                    Ok::<(), PersistenceError>(())
                })
            })
            .await
            .expect("malformed projection fixture");

        assert!(matches!(
            StationDetailQuery::new(runtime.handle())
                .load(&station.id)
                .await,
            Err(ApplicationError::Internal)
        ));

        runtime.close().await.expect("runtime closes");
    }

    async fn create_station(runtime: &PersistenceRuntime) -> Station {
        StationService::new(
            runtime.handle(),
            Arc::new(SystemClock),
            Arc::new(UuidV7Generator),
        )
        .create(CreateStationInput {
            name: "Detail Station".to_string(),
            station_type: "sub2api".to_string(),
            website_url: "https://example.test".to_string(),
            api_base_url: "https://example.test/v1".to_string(),
            api_key: "sk-test-fake-value".to_string(),
            collector_proxy_mode: "inherit".to_string(),
            collector_proxy_url: None,
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: None,
            collection_interval_minutes: 30,
            note: None,
        })
        .await
        .expect("station")
    }
}
