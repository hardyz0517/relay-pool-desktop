use crate::{
    models::{
        pricing::BalanceSnapshot,
        routing::{RoutingGroupFilter, RoutingPolicy, SchedulerAdvancedSettings},
    },
    services::{database::AppDatabase, proxy::router::RichRouteCandidate},
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoutingExecutionSettings {
    pub policy: RoutingPolicy,
    pub max_rate_multiplier: Option<f64>,
    pub routing_group_filter: RoutingGroupFilter,
    pub scheduler_advanced_settings: SchedulerAdvancedSettings,
    pub allow_depleted_fallback: bool,
}

impl Default for RoutingExecutionSettings {
    fn default() -> Self {
        Self {
            policy: RoutingPolicy::PriorityFallback,
            max_rate_multiplier: None,
            routing_group_filter: RoutingGroupFilter::default(),
            scheduler_advanced_settings: SchedulerAdvancedSettings::default(),
            allow_depleted_fallback: false,
        }
    }
}

#[derive(Clone)]
pub(crate) struct SqliteRoutingRepository {
    database: AppDatabase,
    data_key: [u8; 32],
}

impl SqliteRoutingRepository {
    pub(crate) fn new(database: AppDatabase, data_key: [u8; 32]) -> Self {
        Self { database, data_key }
    }
}

pub(crate) trait RoutingRepository: Send + Sync {
    fn load_runtime_candidates(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<RichRouteCandidate>, String>>;

    fn load_model_alias_pairs(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<(String, String)>, String>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn load_execution_settings(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<RoutingExecutionSettings, String>> {
        Box::pin(async { Ok(RoutingExecutionSettings::default()) })
    }

    fn load_balance_snapshots(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<BalanceSnapshot>, String>> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

impl RoutingRepository for SqliteRoutingRepository {
    fn load_runtime_candidates(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<RichRouteCandidate>, String>> {
        let database = self.database.clone();
        let data_key = self.data_key;
        Box::pin(async move {
            tauri::async_runtime::spawn_blocking(move || {
                database.proxy_rich_route_candidates_with_data_key(&data_key)
            })
            .await
            .map_err(|error| format!("routing repository candidate task failed: {error}"))?
        })
    }

    fn load_execution_settings(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<RoutingExecutionSettings, String>> {
        let database = self.database.clone();
        Box::pin(async move {
            tauri::async_runtime::spawn_blocking(move || {
                let settings = database.get_settings()?;
                Ok(RoutingExecutionSettings {
                    policy: routing_policy_from_setting(&settings.default_routing_strategy),
                    max_rate_multiplier: settings.max_rate_multiplier,
                    routing_group_filter: settings.default_routing_group_filter,
                    scheduler_advanced_settings: settings.scheduler_advanced_settings,
                    allow_depleted_fallback: settings.allow_depleted_fallback,
                })
            })
            .await
            .map_err(|error| format!("routing settings task failed: {error}"))?
        })
    }

    fn load_model_alias_pairs(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<(String, String)>, String>> {
        let database = self.database.clone();
        Box::pin(async move {
            tauri::async_runtime::spawn_blocking(move || database.enabled_model_alias_pairs())
                .await
                .map_err(|error| format!("routing repository alias task failed: {error}"))?
        })
    }

    fn load_balance_snapshots(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<BalanceSnapshot>, String>> {
        let database = self.database.clone();
        Box::pin(async move {
            tauri::async_runtime::spawn_blocking(move || database.list_balance_snapshots())
                .await
                .map_err(|error| format!("routing repository balance task failed: {error}"))?
        })
    }
}

fn routing_policy_from_setting(value: &str) -> RoutingPolicy {
    match value.trim().to_ascii_lowercase().as_str() {
        "automatic_balanced" | "automatic" => RoutingPolicy::AutomaticBalanced,
        "stable_first" | "stable" => RoutingPolicy::StableFirst,
        "backup_only" => RoutingPolicy::BackupOnly,
        "cheap_first" => RoutingPolicy::CheapFirst,
        "cost_stable_first" => RoutingPolicy::CostStableFirst,
        _ => RoutingPolicy::PriorityFallback,
    }
}
#[cfg(test)]
mod tests {
    use std::{sync::mpsc, time::Duration};

    use crate::{
        models::{
            station_keys::CreateStationKeyInput,
            stations::{CreateStationInput, UpdateStationInput},
        },
        services::{database::AppDatabase, secrets::crypto::generate_data_key},
    };

    use super::*;

    #[tokio::test]
    async fn repository_loads_runtime_candidates_without_blocking_async_callers() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        seed_candidate(&database, &data_key, "async-load");
        let release = hold_database_connection(&database);
        let repository = SqliteRoutingRepository::new(database, data_key);

        let load = tokio::spawn(async move { repository.load_runtime_candidates().await });
        tokio::time::timeout(Duration::from_millis(100), async {
            tokio::task::yield_now().await;
        })
        .await
        .expect("runtime remains available while SQLite waits in blocking pool");

        release.send(()).expect("release DB lock");
        let candidates = load.await.expect("join load").expect("candidates");

        assert!(candidates
            .iter()
            .any(|candidate| candidate.candidate.api_key == "sk-async-load"));
    }

    struct SeededCandidate {
        station_id: String,
        station_key_id: String,
        station_endpoint_revision: i64,
    }

    fn seed_candidate(
        database: &AppDatabase,
        data_key: &[u8; 32],
        suffix: &str,
    ) -> SeededCandidate {
        let station = database
            .create_station(CreateStationInput {
                name: format!("Station {suffix}"),
                station_type: "openai-compatible".to_string(),
                website_url: "https://example.test".to_string(),
                api_base_url: format!("https://{suffix}.example.test/v1"),
                api_key: "sk-station".to_string(),
                collector_proxy_mode: "direct".to_string(),
                collector_proxy_url: None,
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        let key = database
            .create_station_key_with_data_key(
                CreateStationKeyInput {
                    station_id: station.id.clone(),
                    name: format!("Key {suffix}"),
                    api_key: format!("sk-{suffix}"),
                    enabled: true,
                    priority: Some(0),
                    max_concurrency: Some(0),
                    load_factor: None,
                    schedulable: Some(true),
                    group_name: None,
                    tier_label: None,
                    group_binding_id: None,
                    group_id_hash: None,
                    rate_multiplier: None,
                    manual_rate_multiplier: None,
                    rate_source: None,
                    balance_scope: None,
                    note: None,
                },
                data_key,
            )
            .expect("station key");
        SeededCandidate {
            station_id: station.id,
            station_key_id: key.id,
            station_endpoint_revision: station.endpoint_revision,
        }
    }

    fn hold_database_connection(database: &AppDatabase) -> mpsc::Sender<()> {
        let database = database.clone();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _guard = database
                .connection_for_repository_tests()
                .expect("connection");
            started_tx.send(()).expect("signal lock");
            release_rx.recv().expect("release lock");
        });
        started_rx.recv().expect("wait for lock");
        release_tx
    }

    fn change_station_endpoint(database: &AppDatabase, station_id: &str) {
        let station = database
            .list_stations()
            .expect("stations")
            .into_iter()
            .find(|station| station.id == station_id)
            .expect("station");
        database
            .update_station(UpdateStationInput {
                id: station.id,
                name: station.name,
                station_type: station.station_type,
                website_url: "https://changed.example.test".to_string(),
                api_base_url: "https://changed.example.test/v1".to_string(),
                api_key: None,
                collector_proxy_mode: station.collector_proxy_mode,
                collector_proxy_url: station.collector_proxy_url,
                enabled: station.enabled,
                credit_per_cny: station.credit_per_cny,
                low_balance_threshold_cny: station.low_balance_threshold_cny,
                collection_interval_minutes: station.collection_interval_minutes,
                note: station.note,
            })
            .expect("change station endpoint");
    }
}
