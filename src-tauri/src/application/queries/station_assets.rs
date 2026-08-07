use std::collections::BTreeMap;

use super::read_model_revision::load_asset_revision;
use crate::{
    application::{error::ApplicationError, pagination::PageLimit},
    models::{
        routing_read_models::{
            ReadModelEnvelope, ReadModelPage, StationAssetReadRow, StationAssetsReadModel,
            ASSET_READ_MODEL_SCHEMA_VERSION,
        },
        station_keys::KeyPoolItem,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::{credential_store::CredentialStore, station_catalog::StationCatalogStore},
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
        let stations = self.stations.list(&mut read).await?;
        let keys = self.credentials.list_key_pool_items(&mut read).await?;
        let mut by_station: BTreeMap<String, Vec<KeyPoolItem>> = BTreeMap::new();
        for key in keys {
            by_station
                .entry(key.station_id.clone())
                .or_default()
                .push(key);
        }
        let rows = stations
            .into_iter()
            .take(limit as usize)
            .map(|station| {
                let keys = by_station.remove(&station.id).unwrap_or_default();
                let group_identity_hashes =
                    keys.iter().filter_map(server_group_identity_hash).collect();
                StationAssetReadRow {
                    station,
                    keys,
                    group_identity_hashes,
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
    use super::server_group_identity_hash;
    use crate::models::station_keys::KeyPoolItem;

    fn key() -> KeyPoolItem {
        KeyPoolItem {
            id: "key-1".into(),
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
}
