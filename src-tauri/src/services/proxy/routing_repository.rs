use crate::{
    application::routing::RoutingService,
    models::{
        pricing::BalanceSnapshot,
        routing::{RoutingProxyDefaults, RuntimeRoutingCandidate, RuntimeRoutingSettings},
    },
    services::{
        outbound::resolve_proxy_config,
        proxy::routing_types::{RichRouteCandidate, RouteCandidate, RouteCandidateEconomics},
        secrets::crypto::{decrypt_secret, EncryptedPayload},
    },
};
use base64::{engine::general_purpose, Engine as _};

pub(crate) type RoutingExecutionSettings = RuntimeRoutingSettings;

#[derive(Clone)]
pub(crate) struct V2RoutingRepository {
    routing: RoutingService,
    data_key: [u8; 32],
}

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

    fn load_execution_settings(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<RoutingExecutionSettings, String>> {
        let routing = self.routing.clone();
        Box::pin(async move {
            routing
                .load_execution_settings()
                .await
                .map_err(|error| format!("load V2 routing execution settings failed: {error}"))
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

fn route_candidate_economics_from_balance(
    snapshot: &crate::models::routing::RuntimeRoutingBalance,
) -> RouteCandidateEconomics {
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
