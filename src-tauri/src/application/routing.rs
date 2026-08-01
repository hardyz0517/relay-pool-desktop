use crate::{
    application::routing_engine::{
        model_alias::mapped_model,
        planner::{ordered_plan_candidates, plan_route, PlanningInput},
        request::{
            CanonicalRouteRequest, PlanningRoundContext, RouteKind, RouteProgress,
            RouteRequestClassifier,
        },
        routing_snapshot::{build_local_routing_workspace, LocalRoutingReadCandidate},
        routing_types::{LocalRoutingWorkspace, RouteCandidateEconomics},
    },
    application::{
        credentials::SecretRef,
        error::ApplicationError,
        health_transitions::HealthTransitionService,
        operational_facts::{
            pricing_projector::pricing_context_from_resolution,
            runtime_candidate_adapter::{
                route_projection_from_runtime_candidate_with_pricing,
                route_request_facts_for_read_model, validated_route_settings,
            },
            target_resolver::ExecutionTargetRef,
        },
        queries::{
            operational_detail::{
                operational_detail_from_projection, unavailable_operational_detail,
                StationKeyOperationalDetail,
            },
            routing_runtime::{
                monitoring_target_snapshots_from_facts, runtime_overlay_from_candidates,
                RoutingMonitoringTargetFacts, RoutingMonitoringTargetSnapshot,
                RoutingRuntimeOverlay,
            },
            routing_workspace::{
                workspace_snapshot_from_projection_candidates, RoutingWorkspaceProjectionCandidate,
                RoutingWorkspaceSnapshot, RoutingWorkspaceSnapshotInput,
                ROUTING_PREVIEW_POLICY_VERSION,
            },
        },
    },
    models::{
        health::{
            HealthObservation, HealthObservationOutcome, HealthObservationSource,
            HealthWritebackMode, TrafficEquivalence,
        },
        pricing::{BalanceSnapshot, ResolvedPricingContext},
        proxy::{ProxyStatus, RequestLog},
        routing::{
            ModelAlias, RouteCandidateExplanation, RouteEndpointKind, RouteSimulationInput,
            RouteSimulationResult, RoutingGroupFilter, RoutingProxyDefaults,
            RuntimeRoutingCandidate, RuntimeRoutingSettings, StationKeyHealth,
            UpsertModelAliasInput,
        },
        settings::AppSettings,
        stations::StationEndpointHealth,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::pricing_store::PricingStore,
        stores::routing_store::{RoutingStore, StationEndpointProbeTarget},
    },
};

#[derive(Debug, Clone)]
pub(crate) struct RuntimeRoutingCandidateWithPricing {
    pub(crate) candidate: RuntimeRoutingCandidate,
    pub(crate) pricing_context: Option<ResolvedPricingContext>,
}
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

    pub(crate) async fn load_monitoring_target_snapshots(
        &self,
    ) -> Result<Vec<RoutingMonitoringTargetSnapshot>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let rows = self
            .store
            .load_operational_monitoring_target_snapshots(&mut read)
            .await?;
        let facts = rows
            .into_iter()
            .map(|row| RoutingMonitoringTargetFacts {
                station_id: row.station_id,
                station_key_id: row.station_key_id,
                endpoint_revision: row.endpoint_revision,
                api_base_url: row.api_base_url,
                upstream_api_format: row.upstream_api_format,
                supports_chat_completions: row.supports_chat_completions,
                supports_responses: row.supports_responses,
            })
            .collect();
        Ok(monitoring_target_snapshots_from_facts(facts))
    }

    pub(crate) async fn load_runtime_candidates_with_request_pricing(
        &self,
        request: &crate::application::routing_engine::request::RouteRequestFacts,
    ) -> Result<Vec<RuntimeRoutingCandidateWithPricing>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let candidates = self.store.load_runtime_candidates(&mut read).await?;
        let mut pricing_contexts = if request.route_kind() == RouteKind::Inference {
            if let Some(requested_model) = request.requested_model() {
                let station_key_ids = candidates
                    .iter()
                    .map(|candidate| candidate.station_key_id.clone())
                    .collect::<Vec<_>>();
                let at = request.admitted_at_ms().to_string();
                PricingStore
                    .resolve_station_key_pricing_many(
                        &mut read,
                        &station_key_ids,
                        requested_model,
                        &at,
                    )
                    .await?
                    .into_iter()
                    .map(|(station_key_id, resolution)| {
                        let mut context = pricing_context_from_resolution(
                            &station_key_id,
                            requested_model,
                            Some(&resolution),
                        );
                        if context.resolved_at == "unknown" {
                            context.resolved_at = at.clone();
                        }
                        (station_key_id, context)
                    })
                    .collect::<std::collections::BTreeMap<_, _>>()
            } else {
                std::collections::BTreeMap::new()
            }
        } else {
            std::collections::BTreeMap::new()
        };
        Ok(candidates
            .into_iter()
            .map(|candidate| RuntimeRoutingCandidateWithPricing {
                pricing_context: pricing_contexts.remove(&candidate.station_key_id),
                candidate,
            })
            .collect())
    }

    pub(crate) async fn load_operational_execution_target_refs(
        &self,
        station_key_ids: Vec<String>,
    ) -> Result<Vec<ExecutionTargetRef>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let rows = self
            .store
            .load_operational_execution_target_refs(&mut read, &station_key_ids)
            .await
            .map_err(ApplicationError::from)?;
        rows.into_iter()
            .map(|row| {
                let api_key_secret_ref = match (
                    row.api_key_secret_id,
                    row.api_key_secret_scope,
                    row.api_key_secret_owner_id,
                    row.api_key_secret_kind,
                ) {
                    (Some(id), Some(scope), Some(owner_id), Some(kind)) => Some(SecretRef {
                        id,
                        scope,
                        owner_id,
                        kind,
                    }),
                    (None, None, None, None) => None,
                    _ => return Err(ApplicationError::ConstraintViolation),
                };
                Ok(ExecutionTargetRef {
                    station_key_id: row.station_key_id,
                    station_id: row.station_id,
                    endpoint_revision: row.endpoint_revision,
                    api_base_url: row.api_base_url,
                    upstream_api_format: row.upstream_api_format,
                    collector_proxy_mode: row.collector_proxy_mode,
                    collector_proxy_url: row.collector_proxy_url,
                    enabled: row.key_enabled && row.station_enabled,
                    api_key_secret_ref,
                    inline_api_key_present: row.inline_api_key_present,
                })
            })
            .collect()
    }

    pub(crate) async fn load_routing_workspace_snapshot(
        &self,
        input: RoutingWorkspaceSnapshotInput,
    ) -> Result<RoutingWorkspaceSnapshot, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let settings = self.store.load_execution_settings(&mut read).await?;
        drop(read);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let request = route_request_facts_for_read_model(&settings, now_ms);
        let projected = self
            .load_runtime_candidates_with_request_pricing(&request)
            .await?
            .into_iter()
            .map(|row| {
                let station_name = row.candidate.station_name.clone();
                let key_name = row.candidate.key_name.clone();
                route_projection_from_runtime_candidate_with_pricing(
                    &request,
                    row.candidate,
                    row.pricing_context.as_ref(),
                )
                .map(|projection| RoutingWorkspaceProjectionCandidate {
                    station_name,
                    key_name,
                    projection,
                })
            })
            .collect::<Result<Vec<_>, String>>()
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        Ok(workspace_snapshot_from_projection_candidates(
            &settings, projected, input, now_ms,
        ))
    }

    pub(crate) async fn load_routing_runtime_overlay(
        &self,
    ) -> Result<RoutingRuntimeOverlay, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let candidates = self.store.load_runtime_candidates(&mut read).await?;
        Ok(runtime_overlay_from_candidates(
            candidates,
            chrono::Utc::now().timestamp_millis(),
            1,
            1024,
        ))
    }

    pub(crate) async fn get_station_key_operational_detail(
        &self,
        station_key_id: String,
    ) -> Result<StationKeyOperationalDetail, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let settings = self.store.load_execution_settings(&mut read).await?;
        drop(read);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let request = route_request_facts_for_read_model(&settings, now_ms);
        for row in self
            .load_runtime_candidates_with_request_pricing(&request)
            .await?
        {
            if row.candidate.station_key_id == station_key_id {
                let projection = route_projection_from_runtime_candidate_with_pricing(
                    &request,
                    row.candidate,
                    row.pricing_context.as_ref(),
                )
                .map_err(|_| ApplicationError::ConstraintViolation)?;
                return Ok(operational_detail_from_projection(&projection));
            }
        }
        Ok(unavailable_operational_detail(station_key_id))
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
        let now_ms = chrono::Utc::now().timestamp_millis();
        let runtime_settings = runtime_settings_from_app_settings(&settings);
        let request = route_request_facts_for_read_model(&runtime_settings, now_ms);
        let candidates = self
            .load_runtime_candidates_with_request_pricing(&request)
            .await?
            .into_iter()
            .map(|candidate| local_routing_candidate_from_runtime(candidate, &request))
            .collect::<Result<Vec<_>, String>>()
            .map_err(|_| ApplicationError::ConstraintViolation)?;
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
        let now_ms = chrono::Utc::now().timestamp_millis();
        let health = HealthTransitionService::new();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    let observation_id =
                        format!("health-observation-manual-connectivity-{station_key_id}-{now_ms}");
                    let source_event_id = format!(
                        "manual-connectivity:{station_id}:{expected_endpoint_revision}:{now_ms}"
                    );
                    health
                        .record_observation(
                            write,
                            HealthObservation {
                                id: observation_id,
                                station_key_id,
                                target_result_id: None,
                                source: HealthObservationSource::ManualConnectivity,
                                source_event_id,
                                observed_at_ms: now_ms,
                                endpoint_revision: expected_endpoint_revision,
                                outcome: if ok {
                                    HealthObservationOutcome::Success
                                } else {
                                    HealthObservationOutcome::ObserveFailure
                                },
                                failure_kind: (!ok).then_some("manual_connectivity".to_string()),
                                latency_ms: Some(duration_ms),
                                retry_after_ms: None,
                                error_summary: (!ok).then_some(error_summary),
                                writeback_mode: HealthWritebackMode::Authoritative,
                                traffic_equivalence: TrafficEquivalence::Diagnostic,
                            },
                        )
                        .await
                        .map(|_| ())
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
        let aliases = self.store.list_model_alias_pairs(&mut read).await?;
        drop(read);

        let policy = input.policy.clone().unwrap_or(settings.policy.clone());
        let max_rate_multiplier = input.max_rate_multiplier.or(settings.max_rate_multiplier);
        let routing_group_filter = input
            .routing_group_filter
            .clone()
            .unwrap_or(settings.routing_group_filter);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mapped_model = mapped_model(input.model.as_deref(), &aliases);
        let validated_settings = validated_route_settings(&RuntimeRoutingSettings {
            policy: policy.clone(),
            max_rate_multiplier,
            routing_group_filter: routing_group_filter.clone(),
            scheduler_advanced_settings: settings.scheduler_advanced_settings.clone(),
            allow_depleted_fallback: settings.allow_depleted_fallback,
        });
        let request = RouteRequestClassifier::classify(
            CanonicalRouteRequest {
                route_kind: route_kind_from_endpoint(&input.endpoint),
                requested_model: mapped_model.clone(),
                stream: input.stream,
                uses_tools: input.uses_tools,
                uses_vision: input.uses_vision,
                uses_reasoning: input.uses_reasoning,
                untrusted_headers: Vec::new(),
            },
            validated_settings,
            now_ms,
        );
        let projected = self
            .load_runtime_candidates_with_request_pricing(&request)
            .await?
            .into_iter()
            .map(|row| {
                let station_name = row.candidate.station_name.clone();
                let key_name = row.candidate.key_name.clone();
                route_projection_from_runtime_candidate_with_pricing(
                    &request,
                    row.candidate,
                    row.pricing_context.as_ref(),
                )
                .map(|projection| SimulatedRouteCandidateProjection {
                    station_name,
                    key_name,
                    projection,
                })
            })
            .collect::<Result<Vec<_>, String>>()
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let context = PlanningRoundContext {
            request,
            progress: RouteProgress::new(now_ms + 30_000).view(),
            snapshot_id: simulation_snapshot_id(&projected),
            runtime_overlay_revision: 1,
        };
        let projected_route_candidates = projected
            .iter()
            .map(|candidate| candidate.projection.clone())
            .collect::<Vec<_>>();
        let plan = plan_route(PlanningInput {
            context: &context,
            candidates: &projected_route_candidates,
            affinity_station_key_id: None,
        })
        .map_err(|_| ApplicationError::ConstraintViolation)?;
        let explanations = simulation_explanations(
            &projected,
            &plan,
            mapped_model.clone(),
            routing_group_filter.clone(),
        );
        let selected_station_key_id = plan.selected_station_key_id.clone();
        let selected_station_id = selected_station_key_id
            .as_deref()
            .and_then(|station_key_id| {
                projected
                    .iter()
                    .find(|candidate| {
                        candidate.projection.identity.station_key_id == station_key_id
                    })
                    .map(|candidate| candidate.projection.identity.station_id.clone())
            });
        let planner_error_code = if selected_station_key_id.is_none() {
            Some(
                plan.rejections
                    .first()
                    .map(|rejection| rejection.code.to_string())
                    .unwrap_or_else(|| "no_eligible_candidate".to_string()),
            )
        } else {
            None
        };
        let message = selected_station_key_id
            .as_deref()
            .map(|station_key_id| {
                let key_name = projected
                    .iter()
                    .find(|candidate| {
                        candidate.projection.identity.station_key_id == station_key_id
                    })
                    .map(|candidate| candidate.key_name.as_str())
                    .unwrap_or(station_key_id);
                format!("Route simulation selected {key_name}")
            })
            .unwrap_or_else(|| {
                planner_error_code
                    .as_deref()
                    .map(|code| format!("Route simulation rejected request: {code}"))
                    .unwrap_or_else(|| "Route simulation found no eligible route".to_string())
            });
        Ok(RouteSimulationResult {
            preview_policy_version: ROUTING_PREVIEW_POLICY_VERSION.to_string(),
            capacity_mode: "snapshot_only".to_string(),
            selected_capacity_acquired: false,
            selected_station_key_id,
            selected_station_id,
            mapped_model,
            policy,
            max_rate_multiplier,
            routing_group_filter,
            planner_error_code,
            candidates: explanations,
            message,
        })
    }
}

#[derive(Debug, Clone)]
struct SimulatedRouteCandidateProjection {
    station_name: String,
    key_name: String,
    projection:
        crate::application::operational_facts::candidate_projector::RouteCandidateProjection,
}

fn route_kind_from_endpoint(endpoint: &RouteEndpointKind) -> RouteKind {
    match endpoint {
        RouteEndpointKind::Models => RouteKind::ModelCatalog,
        RouteEndpointKind::ChatCompletions
        | RouteEndpointKind::Responses
        | RouteEndpointKind::Embeddings => RouteKind::Inference,
    }
}

fn simulation_snapshot_id(candidates: &[SimulatedRouteCandidateProjection]) -> String {
    let mut parts = candidates
        .iter()
        .map(|candidate| candidate.projection.provenance.snapshot_id.as_str())
        .collect::<Vec<_>>();
    parts.sort_unstable();
    parts.dedup();
    if parts.is_empty() {
        "empty-simulation-snapshot".to_string()
    } else {
        parts.join("|")
    }
}

fn simulation_explanations(
    candidates: &[SimulatedRouteCandidateProjection],
    plan: &crate::application::routing_engine::selector::RoutePlan,
    mapped_model: Option<String>,
    routing_group_scope: RoutingGroupFilter,
) -> Vec<RouteCandidateExplanation> {
    let ordered = ordered_plan_candidates(plan);
    candidates
        .iter()
        .map(|candidate| {
            let rank = ordered
                .iter()
                .position(|planned| {
                    planned.station_key_id == candidate.projection.identity.station_key_id
                })
                .map(|index| index as i64 + 1);
            simulation_explanation(
                candidate,
                plan,
                rank,
                mapped_model.clone(),
                routing_group_scope.clone(),
            )
        })
        .collect()
}

fn simulation_explanation(
    candidate: &SimulatedRouteCandidateProjection,
    plan: &crate::application::routing_engine::selector::RoutePlan,
    top_k_rank: Option<i64>,
    mapped_model: Option<String>,
    routing_group_scope: RoutingGroupFilter,
) -> RouteCandidateExplanation {
    let projection = &candidate.projection;
    let rejection_reasons = plan
        .rejections
        .iter()
        .filter(|rejection| rejection.station_key_id == projection.identity.station_key_id)
        .map(|rejection| rejection.code.to_string())
        .chain(
            projection
                .hard_rejection_codes
                .iter()
                .map(|code| (*code).to_string()),
        )
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let accepted = rejection_reasons.is_empty();
    let mut reasons = Vec::new();
    if accepted {
        reasons.push(format!("profile:{:?}", plan.ordering_profile).to_lowercase());
        reasons.push("snapshot_preview".to_string());
    }
    reasons.extend(
        projection
            .health
            .reasons
            .iter()
            .map(|reason| (*reason).to_string()),
    );
    RouteCandidateExplanation {
        station_key_id: projection.identity.station_key_id.clone(),
        station_id: projection.identity.station_id.clone(),
        station_name: candidate.station_name.clone(),
        key_name: candidate.key_name.clone(),
        accepted,
        reasons,
        rejection_reasons,
        mapped_model,
        pricing_rule_id: None,
        group_binding_id: projection
            .group
            .as_ref()
            .map(|group| group.stable_key.clone()),
        rate_multiplier: projection.multiplier.multiplier,
        normalization_status: Some(projection.pricing.status_label.clone()),
        price_confidence: projection.pricing.confidence,
        estimated_input_price: projection.pricing.estimated_input_price,
        estimated_output_price: projection.pricing.estimated_output_price,
        price_currency: projection.pricing.currency.clone(),
        balance_status: Some(format!("{:?}", projection.balance.status).to_lowercase()),
        balance_value: None,
        balance_scope: projection.balance.selected_scope.clone(),
        balance_collected_at: projection.pricing.observed_at.clone(),
        economic_freshness: projection.pricing.reason.map(ToString::to_string),
        economic_reasons: projection
            .pricing
            .reason
            .map(|reason| vec![reason.to_string()])
            .unwrap_or_default(),
        routing_group_scope: Some(routing_group_scope),
        routing_group_match: projection.policy.group_matches,
        top_k_rank,
        slot_result: Some("snapshot_only".to_string()),
    }
}

fn local_routing_candidate_from_runtime(
    row: RuntimeRoutingCandidateWithPricing,
    request: &crate::application::routing_engine::request::RouteRequestFacts,
) -> Result<LocalRoutingReadCandidate, String> {
    let RuntimeRoutingCandidateWithPricing {
        candidate,
        pricing_context,
    } = row;
    let economics = candidate
        .balance_snapshot
        .as_ref()
        .map(route_candidate_economics_from_balance);
    let projection = route_projection_from_runtime_candidate_with_pricing(
        request,
        candidate.clone(),
        pricing_context.as_ref(),
    )?;
    Ok(LocalRoutingReadCandidate {
        station_key_id: candidate.station_key_id,
        station_id: candidate.station_id,
        station_name: candidate.station_name,
        key_name: candidate.key_name,
        schedulable: candidate.schedulable,
        capabilities: candidate.capabilities,
        health: candidate.health,
        economics,
        projection: Some(projection),
    })
}

fn runtime_settings_from_app_settings(settings: &AppSettings) -> RuntimeRoutingSettings {
    RuntimeRoutingSettings {
        policy: routing_policy_from_settings(&settings.default_routing_strategy),
        max_rate_multiplier: settings.max_rate_multiplier,
        routing_group_filter: settings.default_routing_group_filter.clone(),
        scheduler_advanced_settings: settings.scheduler_advanced_settings.clone(),
        allow_depleted_fallback: settings.allow_depleted_fallback,
    }
}

fn routing_policy_from_settings(value: &str) -> crate::models::routing::RoutingPolicy {
    match value.trim() {
        "automatic_balanced" | "automatic" => {
            crate::models::routing::RoutingPolicy::AutomaticBalanced
        }
        "priority_fallback" => crate::models::routing::RoutingPolicy::PriorityFallback,
        "stable_first" | "stable" => crate::models::routing::RoutingPolicy::StableFirst,
        "backup_only" => crate::models::routing::RoutingPolicy::BackupOnly,
        "cheap_first" => crate::models::routing::RoutingPolicy::CheapFirst,
        "cost_stable_first" => crate::models::routing::RoutingPolicy::CostStableFirst,
        _ => crate::models::routing::RoutingPolicy::PriorityFallback,
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
    use crate::application::operational_facts::{
        pricing_projector::RoutingCostBasis,
        runtime_candidate_adapter::route_projection_from_runtime_candidate,
    };
    use crate::models::{
        pricing::{PricingStatus, RequestKind},
        proxy::UpstreamApiFormat,
        routing::{RoutingPolicy, StationKeyCapabilities},
    };

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

    #[test]
    fn simulation_explanation_uses_projection_plan_without_legacy_scheduler_fields() {
        let now_ms = 1_800_000_000_000;
        let settings = RuntimeRoutingSettings {
            policy: RoutingPolicy::PriorityFallback,
            max_rate_multiplier: None,
            routing_group_filter: RoutingGroupFilter::AllGroups,
            scheduler_advanced_settings: Default::default(),
            allow_depleted_fallback: false,
        };
        let request = route_request_facts_for_read_model(&settings, now_ms);
        let projected = vec![
            simulated_projection(
                runtime_candidate("key-b", "station-b", 20, Some("sk-b")),
                &request,
            ),
            simulated_projection(
                runtime_candidate("key-a", "station-a", 10, Some("sk-a")),
                &request,
            ),
            simulated_projection(runtime_candidate("missing", "station-c", 5, None), &request),
        ];
        let context = PlanningRoundContext {
            request,
            progress: RouteProgress::new(now_ms + 30_000).view(),
            snapshot_id: simulation_snapshot_id(&projected),
            runtime_overlay_revision: 1,
        };
        let projected_route_candidates = projected
            .iter()
            .map(|candidate| candidate.projection.clone())
            .collect::<Vec<_>>();
        let plan = plan_route(PlanningInput {
            context: &context,
            candidates: &projected_route_candidates,
            affinity_station_key_id: None,
        })
        .expect("route plan");

        assert_eq!(plan.selected_station_key_id.as_deref(), Some("key-a"));

        let explanations = simulation_explanations(
            &projected,
            &plan,
            Some("gpt-4o-mini".to_string()),
            RoutingGroupFilter::AllGroups,
        );
        let selected = explanations
            .iter()
            .find(|candidate| candidate.station_key_id == "key-a")
            .expect("selected explanation");
        assert!(selected.accepted);
        assert_eq!(selected.top_k_rank, Some(1));
        assert!(
            selected
                .reasons
                .iter()
                .any(|reason| reason == "snapshot_preview"),
            "simulation explanation should expose planner/projection reasons instead of scheduler factors",
        );

        let rejected = explanations
            .iter()
            .find(|candidate| candidate.station_key_id == "missing")
            .expect("rejected explanation");
        assert!(!rejected.accepted);
        assert!(rejected
            .rejection_reasons
            .iter()
            .any(|reason| reason == "credential_missing"));
    }

    #[test]
    fn local_workspace_candidate_uses_request_scoped_pricing_projection() {
        let settings = RuntimeRoutingSettings {
            policy: RoutingPolicy::CostStableFirst,
            max_rate_multiplier: Some(2.0),
            routing_group_filter: RoutingGroupFilter::AllGroups,
            scheduler_advanced_settings: Default::default(),
            allow_depleted_fallback: false,
        };
        let request = RouteRequestClassifier::classify(
            CanonicalRouteRequest {
                route_kind: RouteKind::Inference,
                requested_model: Some("gpt-5-mini".to_string()),
                stream: false,
                uses_tools: false,
                uses_vision: false,
                uses_reasoning: false,
                untrusted_headers: Vec::new(),
            },
            validated_route_settings(&settings),
            1_700_000,
        );
        let pricing_context = ResolvedPricingContext {
            station_key_id: "key-a".to_string(),
            station_id: "station-a".to_string(),
            requested_model: "gpt-5-mini".to_string(),
            resolved_model: "gpt-5-mini".to_string(),
            request_kind: RequestKind::Text,
            group_binding_id: None,
            base_input_price: None,
            base_output_price: None,
            base_fixed_price: Some(0.42),
            currency: "USD".to_string(),
            unit: "request".to_string(),
            base_price_source: Some("fixture".to_string()),
            effective_rate_multiplier: Some(1.0),
            rate_source: Some("fixture".to_string()),
            rate_collected_at: Some("2026-07-31T00:00:00Z".to_string()),
            estimated_input_price: None,
            estimated_output_price: None,
            estimated_fixed_price: Some(0.42),
            pricing_status: PricingStatus::Priced,
            confidence: 0.99,
            source_chain: vec!["pricing_rule:rule-local".to_string()],
            reason: None,
            resolved_at: "2026-07-31T00:00:00Z".to_string(),
        };

        let local = local_routing_candidate_from_runtime(
            RuntimeRoutingCandidateWithPricing {
                candidate: runtime_candidate("key-a", "station-a", 1, Some("sk-test")),
                pricing_context: Some(pricing_context),
            },
            &request,
        )
        .expect("local read candidate");

        let projection = local.projection.expect("projection");
        assert_eq!(projection.pricing.basis, RoutingCostBasis::ExactPrice);
        assert_eq!(projection.pricing.comparison_value, Some(0.42));
        assert_eq!(projection.pricing.estimated_fixed_price, Some(0.42));
        assert_eq!(projection.pricing.currency.as_deref(), Some("USD"));
        assert_eq!(
            projection.pricing.source_chain,
            vec!["pricing_rule:rule-local".to_string()]
        );
    }

    fn simulated_projection(
        candidate: RuntimeRoutingCandidate,
        request: &crate::application::routing_engine::request::RouteRequestFacts,
    ) -> SimulatedRouteCandidateProjection {
        let station_name = candidate.station_name.clone();
        let key_name = candidate.key_name.clone();
        let projection = route_projection_from_runtime_candidate(request, candidate)
            .expect("runtime projection");
        SimulatedRouteCandidateProjection {
            station_name,
            key_name,
            projection,
        }
    }

    fn runtime_candidate(
        station_key_id: &str,
        station_id: &str,
        priority: i64,
        api_key: Option<&str>,
    ) -> RuntimeRoutingCandidate {
        RuntimeRoutingCandidate {
            station_key_id: station_key_id.to_string(),
            station_id: station_id.to_string(),
            station_type: "newapi".to_string(),
            station_account_concurrency_limit: None,
            station_endpoint_revision: priority,
            sanitized_origin: format!("https://{station_key_id}.example.test"),
            upstream_api_format: UpstreamApiFormat::CustomOpenAiCompatible,
            routing_order: None,
            priority,
            max_concurrency: 4,
            load_factor: Some(0),
            schedulable: true,
            collector_proxy_mode: "inherit".to_string(),
            collector_proxy_url: None,
            station_name: station_id.to_string(),
            key_name: station_key_id.to_string(),
            capabilities: StationKeyCapabilities {
                station_key_id: station_key_id.to_string(),
                supports_chat_completions: true,
                supports_responses: true,
                supports_embeddings: false,
                supports_stream: true,
                supports_tools: true,
                supports_vision: true,
                supports_reasoning: true,
                model_allowlist: Vec::new(),
                model_blocklist: Vec::new(),
                only_use_as_backup: false,
                preferred_models: Vec::new(),
                routing_tags: Vec::new(),
                updated_at: "2026-07-31T00:00:00Z".to_string(),
            },
            health: None,
            balance_snapshot: None,
            economic_snapshot: None,
            api_key: api_key.map(ToString::to_string),
            api_key_secret: None,
        }
    }
}
