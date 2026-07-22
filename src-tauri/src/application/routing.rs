use crate::{
    application::error::ApplicationError,
    application::routing_engine::{
        router::select_route_candidates_with_scheduler,
        routing_snapshot::{build_local_routing_workspace, LocalRoutingReadCandidate},
        routing_types::{
            LocalRoutingWorkspace, RichRouteCandidate, RouteCandidate, RouteCandidateEconomics,
            RouteRequest,
        },
        scheduler::SchedulerRuntimeState,
    },
    models::{
        pricing::BalanceSnapshot,
        proxy::{ProxyStatus, RequestLog},
        routing::{
            ModelAlias, RouteSimulationInput, RouteSimulationResult, RoutingProxyDefaults,
            RuntimeRoutingCandidate, RuntimeRoutingSettings, StationKeyHealth,
            UpsertModelAliasInput,
        },
        settings::AppSettings,
        stations::StationEndpointHealth,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::routing_store::{RoutingStore, StationEndpointProbeTarget},
    },
};

#[derive(Clone)]
pub(crate) struct RoutingService {
    runtime: PersistenceHandle,
    store: RoutingStore,
}

impl RoutingService {
    pub(crate) async fn load_execution_settings(
        &self,
    ) -> Result<RuntimeRoutingSettings, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .load_execution_settings(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            store: RoutingStore,
        }
    }

    pub(crate) async fn load_runtime_candidates(
        &self,
    ) -> Result<Vec<RuntimeRoutingCandidate>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .load_runtime_candidates(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn load_proxy_defaults(
        &self,
    ) -> Result<RoutingProxyDefaults, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .load_proxy_defaults(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_model_alias_pairs(
        &self,
    ) -> Result<Vec<(String, String)>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_model_alias_pairs(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_model_aliases(&self) -> Result<Vec<ModelAlias>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_model_aliases(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn upsert_model_alias(
        &self,
        input: UpsertModelAliasInput,
    ) -> Result<ModelAlias, ApplicationError> {
        let store = self.store;
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
        let now = chrono::Utc::now().timestamp_millis().to_string();
        self.runtime
            .write(|write| {
                Box::pin(async move { store.upsert_model_alias(write, input, &id, &now).await })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn delete_model_alias(&self, id: String) -> Result<(), ApplicationError> {
        let store = self.store;
        self.runtime
            .write(|write| Box::pin(async move { store.delete_model_alias(write, &id).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn reorder_local_routing_keys(
        &self,
        station_key_ids: Vec<String>,
    ) -> Result<(), ApplicationError> {
        let store = self.store;
        let now = chrono::Utc::now().timestamp_millis().to_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .reorder_local_routing_keys(write, &station_key_ids, &now)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn load_local_routing_workspace(
        &self,
        settings: AppSettings,
        request_logs: Vec<RequestLog>,
        proxy_status: ProxyStatus,
    ) -> Result<LocalRoutingWorkspace, ApplicationError> {
        let candidates = self.load_runtime_candidates().await?;
        let candidates = candidates
            .into_iter()
            .map(local_routing_candidate_from_runtime)
            .collect();
        let request_logs = request_logs
            .into_iter()
            .filter(|log| log.route_policy.as_deref() != Some("channel_monitor"))
            .collect();
        Ok(build_local_routing_workspace(
            settings,
            candidates,
            request_logs,
            proxy_status,
        ))
    }

    pub(crate) async fn list_balance_snapshots(
        &self,
    ) -> Result<Vec<BalanceSnapshot>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_balance_snapshots(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_balance_snapshots_for_station(
        &self,
        station_id: &str,
    ) -> Result<Vec<BalanceSnapshot>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_balance_snapshots_for_station(&mut read, station_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_station_key_health(
        &self,
    ) -> Result<Vec<StationKeyHealth>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_station_key_health(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn station_key_health_by_id(
        &self,
        station_key_id: &str,
    ) -> Result<StationKeyHealth, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .station_key_health_by_id(&mut read, station_key_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_station_endpoint_health(
        &self,
    ) -> Result<Vec<StationEndpointHealth>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_station_endpoint_health(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn station_endpoint_probe_target(
        &self,
        station_id: &str,
    ) -> Result<StationEndpointProbeTarget, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .station_endpoint_probe_target(&mut read, station_id)
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn record_station_endpoint_health(
        &self,
        station_id: String,
        expected_endpoint_revision: i64,
        status: String,
        latency_ms: Option<i64>,
        checked_at: String,
        error_summary: Option<String>,
    ) -> Result<StationEndpointHealth, ApplicationError> {
        let store = self.store;
        let updated_at = chrono::Utc::now().timestamp_millis().to_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .record_station_endpoint_health(
                            write,
                            &station_id,
                            expected_endpoint_revision,
                            &status,
                            latency_ms,
                            &checked_at,
                            error_summary.as_deref(),
                            &updated_at,
                        )
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn record_station_key_connectivity(
        &self,
        station_key_id: String,
        station_id: String,
        expected_endpoint_revision: i64,
        ok: bool,
        duration_ms: i64,
        error_summary: String,
    ) -> Result<(), ApplicationError> {
        let store = self.store;
        let now = chrono::Utc::now().timestamp_millis().to_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .record_station_key_connectivity(
                            write,
                            &station_key_id,
                            &station_id,
                            expected_endpoint_revision,
                            ok,
                            duration_ms,
                            &error_summary,
                            &now,
                        )
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn simulate_route(
        &self,
        input: RouteSimulationInput,
    ) -> Result<RouteSimulationResult, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let settings = self.store.load_execution_settings(&mut read).await?;
        let candidates = self.store.load_runtime_candidates(&mut read).await?;
        let aliases = self.store.list_model_alias_pairs(&mut read).await?;
        drop(read);

        let policy = input.policy.clone().unwrap_or(settings.policy);
        let max_rate_multiplier = input.max_rate_multiplier.or(settings.max_rate_multiplier);
        let routing_group_filter = input
            .routing_group_filter
            .clone()
            .unwrap_or(settings.routing_group_filter);
        let request = RouteRequest {
            endpoint: input.endpoint,
            model: input.model,
            stream: input.stream,
            uses_tools: input.uses_tools,
            uses_vision: input.uses_vision,
            uses_reasoning: input.uses_reasoning,
            policy: policy.clone(),
            max_rate_multiplier,
            routing_group_filter: routing_group_filter.clone(),
            session_hash: input.session_hash,
            previous_response_id: input.previous_response_id,
            excluded_key_ids: Vec::new(),
            current_station_key_id: None,
            allow_depleted_fallback: settings.allow_depleted_fallback,
            now_ms: chrono::Utc::now().timestamp_millis(),
        };
        let candidates = candidates
            .into_iter()
            .map(rich_route_candidate_from_runtime)
            .collect();
        let selection = select_route_candidates_with_scheduler(
            &request,
            candidates,
            &aliases,
            &SchedulerRuntimeState::default(),
            &settings.scheduler_advanced_settings,
        )
        .map_err(|_| ApplicationError::ConstraintViolation)?;
        let selected = selection.accepted.first();
        let selected_station_key_id =
            selected.map(|candidate| candidate.candidate.station_key_id.clone());
        let selected_station_id = selected.map(|candidate| candidate.candidate.station_id.clone());
        let message = match selected {
            Some(candidate) => format!("Route simulation selected {}", candidate.key_name),
            None => selection
                .scheduler_error_code
                .as_deref()
                .map(|code| format!("Route simulation rejected request: {code}"))
                .unwrap_or_else(|| "Route simulation found no eligible route".to_string()),
        };
        Ok(RouteSimulationResult {
            selected_station_key_id,
            selected_station_id,
            mapped_model: selection.mapped_model,
            policy,
            max_rate_multiplier,
            routing_group_filter,
            scheduler_error_code: selection.scheduler_error_code,
            candidates: selection.explanations,
            message,
        })
    }
}

fn rich_route_candidate_from_runtime(candidate: RuntimeRoutingCandidate) -> RichRouteCandidate {
    let economics = candidate
        .balance_snapshot
        .as_ref()
        .map(route_candidate_economics_from_balance);
    RichRouteCandidate {
        candidate: RouteCandidate {
            station_key_id: candidate.station_key_id,
            station_id: candidate.station_id,
            station_endpoint_revision: candidate.station_endpoint_revision,
            upstream_base_url: candidate.upstream_base_url,
            api_key: String::new(),
            collector_proxy_mode: candidate.collector_proxy_mode,
            collector_proxy_url: candidate.collector_proxy_url,
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
        economics,
        scheduler_group_binding_id: None,
        scheduler_group_id_hash: None,
        scheduler_group_type: None,
        scheduler_effective_multiplier: None,
        scheduler_multiplier_reject_reason: None,
    }
}

fn local_routing_candidate_from_runtime(
    candidate: RuntimeRoutingCandidate,
) -> LocalRoutingReadCandidate {
    let candidate = rich_route_candidate_from_runtime(candidate);
    LocalRoutingReadCandidate {
        station_key_id: candidate.candidate.station_key_id,
        station_id: candidate.candidate.station_id,
        station_name: candidate.station_name,
        key_name: candidate.key_name,
        schedulable: candidate.candidate.schedulable,
        capabilities: candidate.capabilities,
        health: candidate.health,
        economics: candidate.economics,
        scheduler_group_binding_id: candidate.scheduler_group_binding_id,
        scheduler_group_id_hash: candidate.scheduler_group_id_hash,
        scheduler_group_type: candidate.scheduler_group_type,
        scheduler_effective_multiplier: candidate.scheduler_effective_multiplier,
        scheduler_multiplier_reject_reason: candidate.scheduler_multiplier_reject_reason,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::routing::RoutingPolicy;

    #[tokio::test]
    async fn execution_settings_preserve_persisted_routing_policy() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("routing.sqlite3");
        let runtime = crate::persistence::runtime::PersistenceRuntime::initialize_new(&path)
            .await
            .expect("runtime");
        let service = RoutingService::new(runtime.handle());

        let defaults = service.load_execution_settings().await.expect("defaults");
        assert_eq!(defaults.policy, RoutingPolicy::CostStableFirst);

        runtime
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE settings SET value = 'stable_first' WHERE key = 'default_routing_strategy'",
                    )
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        "UPDATE settings SET value = 'true' WHERE key = 'allow_depleted_fallback'",
                    )
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("update settings");

        let updated = service.load_execution_settings().await.expect("updated");
        assert_eq!(updated.policy, RoutingPolicy::StableFirst);
        assert!(updated.allow_depleted_fallback);
        runtime.close().await.expect("close persistence runtime");
    }
}
