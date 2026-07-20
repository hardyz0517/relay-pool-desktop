use crate::{
    application::routing::RoutingService,
    models::{
        pricing::BalanceSnapshot,
        routing::{
            RoutingGroupFilter, RoutingPolicy, RoutingProxyDefaults, RuntimeRoutingCandidate,
            SchedulerAdvancedSettings,
        },
    },
    services::{
        database::AppDatabase,
        outbound::resolve_proxy_config,
        proxy::{
            router::{RichRouteCandidate, RouteCandidateEconomics},
            RouteCandidate,
        },
        secrets::crypto::{decrypt_secret, EncryptedPayload},
    },
};
use base64::{engine::general_purpose, Engine as _};

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

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct V2RoutingRepository {
    routing: RoutingService,
    data_key: [u8; 32],
}

#[allow(dead_code)]
impl V2RoutingRepository {
    pub(crate) fn new(routing: RoutingService, data_key: [u8; 32]) -> Self {
        Self { routing, data_key }
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

impl RoutingRepository for V2RoutingRepository {
    fn load_runtime_candidates(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<RichRouteCandidate>, String>> {
        let routing = self.routing.clone();
        let data_key = self.data_key;
        Box::pin(async move {
            let proxy_defaults = routing
                .load_proxy_defaults()
                .await
                .map_err(|error| format!("load proxy defaults failed: {error}"))?;
            let candidates = routing
                .load_runtime_candidates()
                .await
                .map_err(|error| format!("load V2 route candidates failed: {error}"))?;
            candidates
                .into_iter()
                .map(|candidate| {
                    rich_route_candidate_from_v2(candidate, &data_key, &proxy_defaults)
                })
                .collect()
        })
    }

    fn load_model_alias_pairs(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<(String, String)>, String>> {
        let routing = self.routing.clone();
        Box::pin(async move {
            routing
                .list_model_alias_pairs()
                .await
                .map_err(|error| format!("load V2 model aliases failed: {error}"))
        })
    }

    fn load_balance_snapshots(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<BalanceSnapshot>, String>> {
        let routing = self.routing.clone();
        Box::pin(async move {
            routing
                .list_balance_snapshots()
                .await
                .map_err(|error| format!("load V2 balance snapshots failed: {error}"))
        })
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

fn rich_route_candidate_from_v2(
    candidate: RuntimeRoutingCandidate,
    data_key: &[u8; 32],
    proxy_defaults: &RoutingProxyDefaults,
) -> Result<RichRouteCandidate, String> {
    let proxy = resolve_proxy_config(
        &candidate.collector_proxy_mode,
        candidate.collector_proxy_url.clone(),
        &proxy_defaults.collector_proxy_mode,
        proxy_defaults.collector_proxy_url.clone(),
    );
    let api_key = runtime_candidate_api_key(&candidate, data_key)?;
    Ok(RichRouteCandidate {
        candidate: RouteCandidate {
            station_key_id: candidate.station_key_id.clone(),
            station_id: candidate.station_id.clone(),
            station_endpoint_revision: candidate.station_endpoint_revision,
            upstream_base_url: candidate.upstream_base_url,
            api_key,
            collector_proxy_mode: proxy.mode,
            collector_proxy_url: proxy.url,
            upstream_api_format: candidate.upstream_api_format,
            priority: candidate.routing_order.unwrap_or(candidate.priority),
            max_concurrency: candidate.max_concurrency,
            load_factor: candidate.load_factor,
            schedulable: candidate.schedulable,
        },
        station_name: candidate.station_name,
        key_name: candidate.key_name,
        capabilities: candidate.capabilities,
        health: candidate.health,
        economics: candidate
            .balance_snapshot
            .as_ref()
            .map(route_candidate_economics_from_balance),
        scheduler_group_binding_id: None,
        scheduler_group_id_hash: None,
        scheduler_group_type: None,
        scheduler_effective_multiplier: None,
        scheduler_multiplier_reject_reason: None,
    })
}

fn runtime_candidate_api_key(
    candidate: &RuntimeRoutingCandidate,
    data_key: &[u8; 32],
) -> Result<String, String> {
    if let Some(api_key) = candidate
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|api_key| !api_key.is_empty())
    {
        return Ok(api_key.to_string());
    }
    let secret = candidate
        .api_key_secret
        .as_ref()
        .ok_or_else(|| "station key secret unavailable".to_string())?;
    decrypt_secret(
        data_key,
        &EncryptedPayload {
            ciphertext: general_purpose::STANDARD.encode(&secret.ciphertext),
            nonce: general_purpose::STANDARD.encode(&secret.nonce),
            aad: format!("{}:{}:{}", secret.scope, secret.owner_id, secret.kind),
            value_hash: String::new(),
        },
    )
    .map_err(|_| "station key secret unavailable".to_string())
}

fn route_candidate_economics_from_balance(snapshot: &BalanceSnapshot) -> RouteCandidateEconomics {
    RouteCandidateEconomics {
        balance_status: Some(snapshot.status.clone()),
        balance_value: snapshot.value,
        low_balance_threshold: snapshot.low_balance_threshold,
        balance_currency: Some(snapshot.currency.clone()),
        balance_scope: Some(snapshot.scope.clone()),
        balance_collected_at: snapshot.collected_at.clone(),
        ..RouteCandidateEconomics::default()
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
