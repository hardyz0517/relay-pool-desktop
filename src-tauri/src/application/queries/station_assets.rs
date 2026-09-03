use std::collections::BTreeMap;

use super::read_model_revision::load_asset_revision;
use crate::{
    application::{error::ApplicationError, pagination::PageLimit},
    models::{
        routing_read_models::{
            ReadModelEnvelope, ReadModelPage, StationAssetReadRow, StationAssetsReadModel,
            StationAuthorizationReadSummary, StationCollectionReadSummary,
            ASSET_READ_MODEL_SCHEMA_VERSION,
        },
        station_keys::KeyPoolItem,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::{
            credential_store::CredentialStore, station_catalog::StationCatalogStore,
            station_state_store::StationStateStore,
        },
    },
};

#[derive(Clone)]
pub(crate) struct StationAssetsQuery {
    runtime: PersistenceHandle,
    stations: StationCatalogStore,
    credentials: CredentialStore,
}

impl StationAssetsQuery {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            stations: StationCatalogStore,
            credentials: CredentialStore,
        }
    }

    pub(crate) async fn load(
        &self,
        limit: PageLimit,
    ) -> Result<ReadModelEnvelope<StationAssetsReadModel>, ApplicationError> {
        let limit = limit.get();
        let mut read = self.runtime.begin_read().await?;
        let domain_revision = load_asset_revision(&mut read).await?;
        // Apply the page bound before loading any station-scoped projections.
        // This keeps the JSON id set passed to SQLite and the resulting maps
        // bounded even when the catalog contains more stations than one page.
        let stations = self
            .stations
            .list(&mut read)
            .await?
            .into_iter()
            .take(limit as usize)
            .collect::<Vec<_>>();
        let (mut collection_summaries, mut authorization_summaries) =
            load_station_state_summaries(&mut read, &stations).await?;
        let station_ids_json = serde_json::to_string(
            &stations
                .iter()
                .map(|station| station.id.as_str())
                .collect::<Vec<_>>(),
        )
        .map_err(|_| ApplicationError::Internal)?;
        let keys = self
            .credentials
            .list_key_pool_items_for_stations(&mut read, &station_ids_json)
            .await?;
        let mut by_station: BTreeMap<String, Vec<KeyPoolItem>> = BTreeMap::new();
        for key in keys {
            by_station
                .entry(key.station_id.clone())
                .or_default()
                .push(key);
        }
        let rows = stations
            .into_iter()
            .map(|station| {
                let keys = by_station.remove(&station.id).unwrap_or_default();
                let group_identity_hashes =
                    keys.iter().filter_map(server_group_identity_hash).collect();
                let collection_summary = collection_summaries
                    .remove(&station.id)
                    .unwrap_or_else(|| missing_collection_summary(station.endpoint_revision));
                let authorization_summary = authorization_summaries
                    .remove(&station.id)
                    .unwrap_or(missing_authorization_summary(station.endpoint_revision));
                StationAssetReadRow {
                    station,
                    keys,
                    group_identity_hashes,
                    collection_summary,
                    authorization_summary,
                }
            })
            .collect::<Vec<_>>();
        let returned = rows.len() as u32;
        Ok(ReadModelEnvelope {
            schema_version: ASSET_READ_MODEL_SCHEMA_VERSION,
            generated_at_ms: now_ms(),
            domain_revision,
            page: ReadModelPage {
                limit,
                returned,
                next_cursor: None,
            },
            data: StationAssetsReadModel { rows },
        })
    }

    /// Read only the durable family revision without assembling the asset rows.
    /// This is used by foreground reconciliation after a missed native event.
    pub(crate) async fn revision(&self) -> Result<i64, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        load_asset_revision(&mut read)
            .await
            .map_err(ApplicationError::from)
    }
}

pub(crate) fn missing_collection_summary(endpoint_revision: i64) -> StationCollectionReadSummary {
    // The projection is the authority.  A missing row is an explicit
    // not-collected state; do not infer current health from the legacy
    // stations.status compatibility column.
    StationCollectionReadSummary {
        status: "not_collected".to_string(),
        reason_codes: vec!["projection_missing".to_string()],
        revision: endpoint_revision.max(1),
    }
}

pub(crate) fn missing_authorization_summary(
    endpoint_revision: i64,
) -> StationAuthorizationReadSummary {
    StationAuthorizationReadSummary {
        status: "unknown".to_string(),
        credential_revision: 0,
        reason_code: Some("projection_missing".to_string()),
        revision: endpoint_revision.max(1),
    }
}

/// Load all typed current-state projections for a station set in a bounded
/// number of SQL statements.  This helper is shared with the detail query so
/// list and detail cannot accidentally grow independent authority/fallback
/// logic.  The station ids are passed through SQLite's JSON table-valued
/// function instead of interpolated into SQL, keeping the query parameterized
/// and bounded by the caller's page limit.
pub(crate) async fn load_station_state_summaries(
    read: &mut crate::persistence::ReadSession,
    stations: &[crate::models::stations::Station],
) -> Result<
    (
        BTreeMap<String, StationCollectionReadSummary>,
        BTreeMap<String, StationAuthorizationReadSummary>,
    ),
    ApplicationError,
> {
    if stations.is_empty() {
        return Ok((BTreeMap::new(), BTreeMap::new()));
    }

    let station_ids = stations
        .iter()
        .map(|station| station.id.as_str())
        .collect::<Vec<_>>();
    let station_ids_json =
        serde_json::to_string(&station_ids).map_err(|_| ApplicationError::Internal)?;

    let station_state = StationStateStore;
    let collection_rows = station_state
        .list_collection_projections(read, &station_ids_json)
        .await?;
    let mut collection_summaries = BTreeMap::new();
    for row in collection_rows {
        let station_id = row.station_id;
        let reason_codes_json = row.reason_codes_json;
        // A JSON-valid value is not necessarily the expected string array.
        // Treat malformed projection data as unavailable instead of silently
        // manufacturing a healthy/empty result in the UI.
        let reason_codes = parse_reason_codes(&reason_codes_json)?;
        collection_summaries.insert(
            station_id,
            StationCollectionReadSummary {
                status: row.status,
                reason_codes,
                revision: row.revision,
            },
        );
    }

    let authorization_rows = station_state
        .list_authorization_projections(read, &station_ids_json)
        .await?;
    let mut authorization_summaries = BTreeMap::new();
    for row in authorization_rows {
        let station_id = row.station_id;
        authorization_summaries.insert(
            station_id,
            StationAuthorizationReadSummary {
                status: row.status,
                credential_revision: row.credential_revision,
                reason_code: row.reason_code,
                revision: row.revision,
            },
        );
    }
    Ok((collection_summaries, authorization_summaries))
}

fn parse_reason_codes(raw: &str) -> Result<Vec<String>, ApplicationError> {
    serde_json::from_str::<Vec<String>>(raw).map_err(|_| ApplicationError::Internal)
}

pub(crate) fn server_group_identity_hash(key: &KeyPoolItem) -> Option<String> {
    let identity = key
        .group_binding_id
        .as_deref()
        .or(key.group_id_hash.as_deref())?;
    crate::models::routing_read_models::group_identity_hash(identity)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::{
        missing_authorization_summary, missing_collection_summary, server_group_identity_hash,
    };
    use crate::models::station_keys::KeyPoolItem;

    fn key() -> KeyPoolItem {
        KeyPoolItem {
            id: "key-1".into(),
            station_key_lifecycle_revision: 1,
            station_id: "station-1".into(),
            station_name: "Station".into(),
            station_type: "openai".into(),
            station_api_base_url: "https://example.test/v1".into(),
            station_endpoint_revision: 1,
            station_upstream_api_format: "openai".into(),
            name: "key".into(),
            api_key_masked: "sk-***".into(),
            api_key_present: true,
            enabled: true,
            priority: 1,
            max_concurrency: 3,
            load_factor: None,
            schedulable: true,
            group_name: Some("Pro".into()),
            tier_label: None,
            group_binding_id: None,
            group_id_hash: Some("group-id-1".into()),
            rate_multiplier: None,
            manual_rate_multiplier: None,
            manual_rate_updated_at: None,
            rate_source: None,
            rate_collected_at: None,
            balance_scope: None,
            status: "unknown".into(),
            last_checked_at: None,
            last_used_at: None,
            note: None,
            capability_summary: vec![],
            model_scope_summary: "all".into(),
            only_use_as_backup: false,
            circuit: None,
            cooldown_until: None,
            success_rate: None,
            avg_latency_ms: None,
            consecutive_failures: 0,
            last_error_summary: None,
            endpoint_ping_status: "unchecked".into(),
            endpoint_ping_ms: None,
            endpoint_ping_checked_at: None,
            endpoint_ping_error: None,
            created_at: "1".into(),
            updated_at: "1".into(),
        }
    }

    #[test]
    fn group_identity_is_server_issued_and_stable() {
        let first = server_group_identity_hash(&key()).expect("group identity");
        let second = server_group_identity_hash(&key()).expect("group identity");
        assert_eq!(first, second);
        assert!(first.starts_with("sha256:"));
        assert!(!first.contains("group-id-1"));
    }

    #[test]
    fn binding_identity_wins_over_legacy_name() {
        let mut item = key();
        item.group_binding_id = Some("binding-1".into());
        let binding = server_group_identity_hash(&item).expect("binding identity");
        item.group_name = Some("other".into());
        assert_eq!(
            binding,
            server_group_identity_hash(&item).expect("binding identity")
        );
    }

    #[test]
    fn legacy_display_name_is_not_a_join_identity() {
        let mut item = key();
        item.group_id_hash = None;
        item.group_binding_id = None;
        assert!(server_group_identity_hash(&item).is_none());
    }

    #[test]
    fn missing_projection_is_explicit_and_does_not_inherit_legacy_status() {
        let collection = missing_collection_summary(0);
        assert_eq!(collection.status, "not_collected");
        assert_eq!(collection.reason_codes, vec!["projection_missing"]);
        assert_eq!(collection.revision, 1);

        let authorization = missing_authorization_summary(0);
        assert_eq!(authorization.status, "unknown");
        assert_eq!(
            authorization.reason_code.as_deref(),
            Some("projection_missing")
        );
        assert_eq!(authorization.revision, 1);
    }

    #[test]
    fn malformed_reason_codes_are_not_coerced_to_an_empty_list() {
        assert!(super::parse_reason_codes(r#"{"unexpected":true}"#).is_err());
        assert!(super::parse_reason_codes(r#"[1,2]"#).is_err());
        assert_eq!(
            super::parse_reason_codes(r#"["collection_failed","timeout"]"#)
                .expect("typed reason code list"),
            vec!["collection_failed", "timeout"]
        );
    }
}
