use std::sync::Arc;

use crate::{
    application::routing_engine::{
        algorithm_profile::DispatchAlgorithmProfile,
        intelligent_planner::plan_snapshot_with_budget,
        planning_snapshot::{PlanningSnapshot, RuntimeOverlaySnapshot},
        model_alias::mapped_model,
        request::{
            CanonicalRouteRequest, RouteKind,
            RouteRequestClassifier,
        },
    },
    application::{
        credentials::SecretRef,
        error::ApplicationError,
        health_transitions::HealthTransitionService,
        operational_facts::{
            planning_snapshot::PlanningSnapshotBuilder,
            pricing_projector::pricing_context_from_resolution,
            candidate_projection::{
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
                RoutingRuntimeCandidateFact,
                RoutingRuntimeOverlay,
            },
            routing_workspace::{
                workspace_snapshot_from_canonical_candidates,
                RoutingWorkspaceSnapshot, RoutingWorkspaceSnapshotInput,
            },
            request_decision_trace::{
                decision_cursor, decision_trace_from_decision, recent_route_decisions_from_page,
                RecentRouteDecisionsInput, RecentRouteDecisionsPage, RequestDecisionTrace,
            },
        },
        routing_policy::RoutingPolicyAggregate,
    },
    models::{
        health::{
            HealthObservation, HealthObservationOutcome, HealthObservationSource,
            HealthWritebackMode, TrafficEquivalence,
        },
        pricing::{BalanceSnapshot, ResolvedPricingContext},
        routing::{
            ModelAlias, RouteCandidateExplanation, RouteEndpointKind, RouteSimulationInput,
            RouteSimulationResult, RoutingGroupFilter, CanonicalRoutingCandidate,
            RuntimeRoutingSettings, StationKeyHealth, UpsertModelAliasInput,
        },
        stations::StationEndpointHealth,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::pricing_store::PricingStore,
        stores::routing_policy_store::RoutingPolicyStore,
        stores::routing_decisions::queries::RoutingDecisionQueries,
        stores::routing_store::{RoutingStore, StationEndpointProbeTarget},
    },
};

#[derive(Debug, Clone)]
pub struct CanonicalRoutingCandidateWithPricing {
    pub(crate) candidate: CanonicalRoutingCandidate,
    pub(crate) pricing_context: Option<ResolvedPricingContext>,
}
#[derive(Clone)]
pub(crate) struct RoutingService {
    runtime: PersistenceHandle,
    store: RoutingStore,
}

impl RoutingService {
    pub async fn list_recent_route_decisions(
        &self,
        input: RecentRouteDecisionsInput,
    ) -> Result<RecentRouteDecisionsPage, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let page = RoutingDecisionQueries
            .list_decisions(
                read.connection(),
                decision_cursor(input.cursor.as_deref()).as_ref(),
                input.limit.unwrap_or(50).clamp(1, 200) as u32,
            )
            .await
            .map_err(ApplicationError::from)?;
        Ok(recent_route_decisions_from_page(page))
    }

    pub async fn get_request_decision_trace(
        &self,
        decision_id: String,
    ) -> Result<RequestDecisionTrace, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let queries = RoutingDecisionQueries;
        let summary = queries
            .get_decision(read.connection(), &decision_id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or(ApplicationError::NotFound)?;
        let candidates = queries
            .list_candidate_details(read.connection(), &summary.id, 500)
            .await
            .map_err(ApplicationError::from)?;
        Ok(decision_trace_from_decision(summary, candidates))
    }

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

    pub(crate) async fn load_routing_policy(
        &self,
    ) -> Result<crate::persistence::stores::routing_policy_store::StoredRoutingPolicy, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        RoutingPolicyStore
            .load(read.connection())
            .await
            .map_err(ApplicationError::from)?
            .ok_or(ApplicationError::NotFound)
    }

    pub(crate) async fn save_routing_policy(
        &self,
        config: crate::models::routing_policy::RoutingPolicyConfigV1,
        expected_revision: Option<u64>,
    ) -> Result<crate::persistence::stores::routing_policy_store::StoredRoutingPolicy, ApplicationError> {
        config
            .validate()
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let value = serde_json::to_value(&config)
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut write = self.runtime.begin_write().await?;
        let stored = RoutingPolicyStore
            .save_compare_and_swap(
                write.connection(),
                expected_revision,
                &value,
                "routing-policy-v1",
                "routing-system-v1",
                "active",
                now_ms,
            )
            .await
            .map_err(ApplicationError::from)?;
        write.commit().await?;
        Ok(stored)
    }

    /// Build the production planner input from one read transaction. The
    /// policy aggregate is deliberately read here, at the application
    /// boundary, so the proxy never parses settings or assembles candidates
    /// itself. A missing aggregate is a configuration-required state.
    pub async fn load_intelligent_planning_snapshot(
        &self,
        request: &crate::application::routing_engine::request::RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
    ) -> Result<Option<PlanningSnapshot>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let stored = RoutingPolicyStore
            .load(read.connection())
            .await
            .map_err(ApplicationError::from)?;
        let Some(stored) = stored else {
            return Ok(None);
        };
        let aggregate = RoutingPolicyAggregate::from_stored(stored)
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let compiled = aggregate
            .compile()
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let options = request
            .requested_model()
            .map(crate::models::operational::OperationalFactReadOptions::for_request_model)
            .unwrap_or_else(crate::models::operational::OperationalFactReadOptions::for_model_catalog);
        let policy = aggregate.config;
        let builder = PlanningSnapshotBuilder;
        let mut snapshot = builder
            .build(
                &mut read,
                &options,
                policy,
                DispatchAlgorithmProfile::default(),
                runtime,
                request,
            )
            .await
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        if let Some(model) = request.requested_model() {
            let ids = snapshot
                .candidates
                .iter()
                .map(|candidate| candidate.station_key_id.clone())
                .collect::<Vec<_>>();
            let pricing = PricingStore
                .resolve_station_key_pricing_many(
                    &mut read,
                    &ids,
                    model,
                    &request.admitted_at_ms().to_string(),
                )
                .await
                .map_err(ApplicationError::from)?;
            for candidate in &mut snapshot.candidates {
                if let Some(resolution) = pricing.get(&candidate.station_key_id) {
                    let value = resolution
                        .pricing_rule
                        .as_ref()
                        .and_then(|rule| {
                            rule.fixed_price
                                .or(rule.input_price)
                                .or(rule.output_price)
                                .map(|price| {
                                    price * rule.rate_multiplier.unwrap_or(1.0)
                                        * resolution.group_rate_multiplier.unwrap_or(1.0)
                                })
                        })
                        .or_else(|| resolution.model_base_price.as_ref().and_then(|base| base.input_price));
                    candidate.cost_basis_points = value.and_then(
                        crate::application::routing_engine::factors::cost_efficiency_from_comparable_value,
                    );
                }
            }
        }
        // Keep compilation at this boundary even though the V1 planner uses
        // the typed config directly; this rejects a malformed status/version
        // before any candidate can reach proxy admission.
        let _ = compiled;
        Ok(Some(snapshot))
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

    /// Read-model-only compatibility projection. This path is intentionally
    /// unavailable to proxy execution; production routing consumes the
    /// immutable PlanningSnapshot instead.
    pub(crate) async fn load_workspace_candidates_with_request_pricing(
        &self,
        request: &crate::application::routing_engine::request::RouteRequestFacts,
    ) -> Result<Vec<CanonicalRoutingCandidateWithPricing>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.load_workspace_candidates_with_request_pricing_in_read(&mut read, request)
            .await
    }

    async fn load_workspace_candidates_with_request_pricing_in_read(
        &self,
        read: &mut crate::persistence::ReadSession,
        request: &crate::application::routing_engine::request::RouteRequestFacts,
    ) -> Result<Vec<CanonicalRoutingCandidateWithPricing>, ApplicationError> {
        let candidates = self.store.load_runtime_candidates(read).await?;
        let mut pricing_contexts = if request.route_kind() == RouteKind::Inference {
            if let Some(requested_model) = request.requested_model() {
                let station_key_ids = candidates
                    .iter()
                    .map(|candidate| candidate.station_key_id.clone())
                    .collect::<Vec<_>>();
                let at = request.admitted_at_ms().to_string();
                PricingStore
                    .resolve_station_key_pricing_many(
                        read,
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
            .map(|candidate| CanonicalRoutingCandidateWithPricing {
                pricing_context: pricing_contexts.remove(&candidate.station_key_id),
                candidate,
            })
            .collect())
    }

    #[cfg(test)]
    pub(crate) async fn load_runtime_candidates_with_request_pricing(
        &self,
        request: &crate::application::routing_engine::request::RouteRequestFacts,
    ) -> Result<Vec<CanonicalRoutingCandidateWithPricing>, ApplicationError> {
        self.load_workspace_candidates_with_request_pricing(request)
            .await
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
                    credential_revision: row.credential_revision,
                    api_base_url: row.api_base_url,
                    upstream_api_format: row.upstream_api_format,
                    collector_proxy_mode: row.collector_proxy_mode,
                    collector_proxy_url: row.collector_proxy_url,
                    enabled: row.key_enabled && row.station_enabled,
                    api_key_secret_ref,
                    inline_api_key_present: row.inline_api_key_present,
                    station_account_max_concurrency: row.station_account_max_concurrency,
                    station_key_max_concurrency: row.station_key_max_concurrency,
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
        let stored_policy = RoutingPolicyStore
            .load(read.connection())
            .await
            .map_err(ApplicationError::from)?
            .ok_or(ApplicationError::NotFound)?;
        let policy_config = RoutingPolicyAggregate::from_stored(stored_policy)
            .map_err(|_| ApplicationError::ConstraintViolation)?
            .config;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let request = route_request_facts_for_read_model(&settings, now_ms);
        let candidates = self
            .load_workspace_candidates_with_request_pricing_in_read(&mut read, &request)
            .await?
            .into_iter()
            .map(|row| (row.candidate, row.pricing_context))
            .collect::<Vec<_>>();
        Ok(workspace_snapshot_from_canonical_candidates(
            policy_config,
            settings.max_rate_multiplier,
            settings.routing_group_scope,
            candidates,
            &request,
            input,
            now_ms,
        ))
    }

    pub(crate) async fn load_routing_runtime_overlay(
        &self,
        proxy: Arc<crate::services::proxy::runtime::ProxyRuntimeState>,
    ) -> Result<RoutingRuntimeOverlay, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let candidates = self.store.load_runtime_candidates(&mut read).await?;
        let mut facts = Vec::with_capacity(candidates.len());
        for candidate in candidates {
                let cooldown_until = candidate
                    .health
                    .as_ref()
                    .and_then(|health| health.cooldown_until.clone());
                let health_state = candidate
                    .health
                    .as_ref()
                    .map(|health| {
                        if health.cooldown_until.is_some() {
                            "cooldown"
                        } else if health.consecutive_failures > 0 {
                            "degraded"
                        } else {
                            "ready"
                        }
                    })
                    .unwrap_or("unknown")
                    .to_string();
                let in_flight = proxy
                    .active_for_station(&candidate.station_type, &candidate.station_id, &candidate.station_key_id)
                    .await
                    .or(candidate.load_factor);
                facts.push(RoutingRuntimeCandidateFact {
                    station_key_id: candidate.station_key_id,
                    station_id: candidate.station_id,
                    endpoint_revision: candidate.station_endpoint_revision,
                    in_flight,
                    health_state,
                    cooldown_until,
                });
        }
        Ok(runtime_overlay_from_candidates(
            facts,
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
            .load_workspace_candidates_with_request_pricing(&request)
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

        // The simulation accepts the canonical config for contract parity. The
        // legacy enum remains only for request classification compatibility;
        // the actual planner policy is loaded from the policy aggregate below.
        let policy = settings.policy.clone();
        let max_rate_multiplier = input.max_rate_multiplier.or(settings.max_rate_multiplier);
        let routing_group_filter = input
            .routing_group_filter
            .clone()
            .unwrap_or(settings.routing_group_scope);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mapped_model = mapped_model(input.model.as_deref(), &aliases);
        let validated_settings = validated_route_settings(&RuntimeRoutingSettings {
            policy: policy.clone(),
            max_rate_multiplier,
            routing_group_scope: routing_group_filter.clone(),
            scheduler_config: settings.scheduler_config.clone(),
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
        let planning_snapshot = self
            .load_intelligent_planning_snapshot(
                &request,
                RuntimeOverlaySnapshot {
                    runtime_instance_id: "simulation".to_string(),
                    runtime_revision: 1,
                    candidate_set_revision: 1,
                    in_flight: 0,
                    max_concurrency: 1,
                    affinity_station_key_id: None,
                },
            )
            .await?
            .ok_or(ApplicationError::ConstraintViolation)?;
        let projected = planning_snapshot
            .candidates
            .iter()
            .map(|candidate| CanonicalSimulationCandidate {
                station_key_id: candidate.station_key_id.clone(),
                station_id: candidate.station_id.clone(),
                station_name: candidate.station_id.clone(),
                key_name: candidate.station_key_id.clone(),
                hard_rejection_codes: if candidate.hard_eligible { Vec::new() } else { vec!["hard_ineligible".to_string()] },
            })
            .collect::<Vec<_>>();
        let canonical_plan = plan_snapshot_with_budget(&planning_snapshot, b"simulation", 1, None)
            .ok();
        let explanations = canonical_plan
            .as_ref()
            .map(|plan| canonical_simulation_explanations(&projected, plan, mapped_model.clone(), routing_group_filter.clone()))
            .unwrap_or_else(|| {
                projected
                    .iter()
                    .map(|candidate| canonical_rejected_explanation(candidate, mapped_model.clone(), routing_group_filter.clone()))
                    .collect()
            });
        let selected_station_key_id = canonical_plan.as_ref().map(|plan| plan.selected_station_key_id.clone());
        let selected_station_id = selected_station_key_id
            .as_deref()
            .and_then(|station_key_id| {
                projected
                .iter()
                    .find(|candidate| candidate.station_key_id == station_key_id)
                    .map(|candidate| candidate.station_id.clone())
            });
        let planner_error_code = if selected_station_key_id.is_none() {
            Some(
                "no_eligible_candidate".to_string(),
            )
        } else {
            None
        };
        let message = selected_station_key_id
            .as_deref()
            .map(|station_key_id| {
                let key_name = projected
                    .iter()
                    .find(|candidate| candidate.station_key_id == station_key_id)
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
            preview_policy_version: "intelligent_planner_v1".to_string(),
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
#[cfg(test)]
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

#[cfg(test)]
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

#[derive(Debug, Clone)]
struct CanonicalSimulationCandidate {
    station_key_id: String,
    station_id: String,
    station_name: String,
    key_name: String,
    hard_rejection_codes: Vec<String>,
}

fn canonical_simulation_explanations(
    candidates: &[CanonicalSimulationCandidate],
    plan: &crate::application::routing_engine::intelligent_planner::RoutePlan,
    mapped_model: Option<String>,
    routing_group_scope: RoutingGroupFilter,
) -> Vec<RouteCandidateExplanation> {
    candidates
        .iter()
        .map(|candidate| {
            let planned = plan
                .candidates
                .iter()
                .find(|planned| planned.station_key_id == candidate.station_key_id);
            let rank = plan
                .candidates
                .iter()
                .position(|planned| planned.station_key_id == candidate.station_key_id)
                .map(|index| index as i64 + 1);
            let mut reasons = vec!["canonical_planner".to_string()];
            if let Some(planned) = planned {
                reasons.push(format!("utility:{}", planned.utility.value()));
            }
            let rejection_reasons = if planned.is_some() {
                candidate.hard_rejection_codes.clone()
            } else {
                vec!["not_selected_or_ineligible".to_string()]
            };
            let accepted = planned.is_some() && rejection_reasons.is_empty();
            route_explanation_from_canonical_candidate(candidate, mapped_model.clone(), routing_group_scope.clone(), accepted, reasons, rejection_reasons, rank)
        })
        .collect()
}

fn canonical_rejected_explanation(
    candidate: &CanonicalSimulationCandidate,
    mapped_model: Option<String>,
    routing_group_scope: RoutingGroupFilter,
) -> RouteCandidateExplanation {
    route_explanation_from_canonical_candidate(
        candidate,
        mapped_model,
        routing_group_scope,
        false,
        vec!["canonical_planner".to_string()],
        vec!["no_eligible_candidate".to_string()],
        None,
    )
}

fn route_explanation_from_canonical_candidate(
    candidate: &CanonicalSimulationCandidate,
    mapped_model: Option<String>,
    routing_group_scope: RoutingGroupFilter,
    accepted: bool,
    reasons: Vec<String>,
    rejection_reasons: Vec<String>,
    top_k_rank: Option<i64>,
) -> RouteCandidateExplanation {
    RouteCandidateExplanation {
        station_key_id: candidate.station_key_id.clone(),
        station_id: candidate.station_id.clone(),
        station_name: candidate.station_name.clone(),
        key_name: candidate.key_name.clone(),
        accepted,
        reasons,
        rejection_reasons,
        mapped_model,
        pricing_rule_id: None,
        group_binding_id: None,
        rate_multiplier: None,
        normalization_status: Some("planning_snapshot".to_string()),
        price_confidence: None,
        estimated_input_price: None,
        estimated_output_price: None,
        price_currency: None,
        balance_status: None,
        balance_value: None,
        balance_scope: None,
        balance_collected_at: None,
        economic_freshness: None,
        economic_reasons: Vec::new(),
        routing_group_scope: Some(routing_group_scope),
        routing_group_match: true,
        top_k_rank,
        slot_result: Some("snapshot_only".to_string()),
    }
}

#[cfg(test)]
fn route_explanation_from_projection(
    candidate: &SimulatedRouteCandidateProjection,
    mapped_model: Option<String>,
    routing_group_scope: RoutingGroupFilter,
    accepted: bool,
    reasons: Vec<String>,
    rejection_reasons: Vec<String>,
    top_k_rank: Option<i64>,
) -> RouteCandidateExplanation {
    let projection = &candidate.projection;
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
        group_binding_id: projection.group.as_ref().map(|group| group.stable_key.clone()),
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
        economic_reasons: projection.pricing.reason.map(|reason| vec![reason.to_string()]).unwrap_or_default(),
        routing_group_scope: Some(routing_group_scope),
        routing_group_match: projection.policy.group_matches,
        top_k_rank,
        slot_result: Some("snapshot_only".to_string()),
    }
}

#[cfg(test)]
fn simulation_explanations(
    candidates: &[SimulatedRouteCandidateProjection],
    plan: &crate::application::routing_engine::candidate_plan::RoutePlan,
    mapped_model: Option<String>,
    routing_group_scope: RoutingGroupFilter,
) -> Vec<RouteCandidateExplanation> {
    let ordered = crate::application::routing_engine::hierarchical_preview::ordered_plan_candidates(plan);
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

#[cfg(test)]
fn simulation_explanation(
    candidate: &SimulatedRouteCandidateProjection,
    plan: &crate::application::routing_engine::candidate_plan::RoutePlan,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::operational_facts::{
        pricing_projector::RoutingCostBasis,
        candidate_projection::route_projection_from_runtime_candidate,
    };
    use crate::application::routing_engine::request::{PlanningRoundContext, RouteProgress};
    use crate::application::routing_engine::hierarchical_preview::{
        plan_route, PlanningInput,
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
        assert_eq!(defaults.policy, RoutingPolicy::AutomaticBalanced);
        assert!(!defaults.allow_depleted_fallback);
        runtime.close().await.expect("close persistence runtime");
    }

    #[test]
    fn simulation_explanation_uses_projection_plan_without_legacy_scheduler_fields() {
        let now_ms = 1_800_000_000_000;
        let settings = RuntimeRoutingSettings {
            policy: RoutingPolicy::PriorityFallback,
            max_rate_multiplier: None,
            routing_group_scope: RoutingGroupFilter::AllGroups,
            scheduler_config: Default::default(),
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
            routing_group_scope: RoutingGroupFilter::AllGroups,
            scheduler_config: Default::default(),
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

        let projection = route_projection_from_runtime_candidate_with_pricing(
            &request,
            runtime_candidate("key-a", "station-a", 1, Some("sk-test")),
            Some(&pricing_context),
        )
        .expect("projection");
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
        candidate: CanonicalRoutingCandidate,
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
    ) -> CanonicalRoutingCandidate {
        CanonicalRoutingCandidate {
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
