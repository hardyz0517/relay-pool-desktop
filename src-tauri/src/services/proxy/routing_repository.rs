use std::collections::BTreeMap;

use crate::{
    application::health_protection::{
        HealthProbeAdmissionMode, HealthProtectionProbe, HealthProtectionScope,
        HealthProtectionStatus,
    },
    application::{
        operational_facts::target_resolver::ExecutionTargetRef,
        routing_engine::planning_snapshot::{PlanningSnapshot, RuntimeOverlaySnapshot},
        routing_engine::{
            admission::CandidateAdmissionProfile,
            request::{PlanningRequestContext, RouteRequestFacts},
        },
        routing_execution_reader::{RoutingExecutionReadError, RoutingExecutionReadPort},
    },
    models::{pricing::BalanceSnapshot, routing::RuntimeRoutingSettings},
    services::outbound::resolve_routing_proxy_config,
};

use crate::application::routing_engine::candidate_plan::RoutePlanCandidate;

pub(crate) type RoutingExecutionSettings = RuntimeRoutingSettings;

#[cfg(test)]
pub(crate) use crate::application::operational_facts::candidate_projection::{
    admission_profile_from_runtime_candidate as admission_profile_from_candidate,
    route_projection_from_runtime_candidate as route_projection_from_runtime,
};

#[derive(Debug, Clone)]
pub(crate) struct OperationalRouteSnapshot {
    /// Execution-only candidate metadata. Durable planning facts and scoring
    /// remain owned by `PlanningSnapshot`; this list only carries what the
    /// admission/transport shell needs after planning.
    pub(crate) candidates: Vec<RoutePlanCandidate>,
    pub(crate) targets: BTreeMap<String, ExecutionTargetRef>,
    pub(crate) profiles: BTreeMap<String, CandidateAdmissionProfile>,
    #[cfg(test)]
    pub(crate) legacy_candidates:
        Vec<crate::application::operational_facts::candidate_projector::RouteCandidateProjection>,
}

#[derive(Clone)]
pub(crate) struct RoutingExecutionRepository {
    execution: std::sync::Arc<dyn RoutingExecutionReadPort>,
}

impl RoutingExecutionRepository {
    pub(crate) fn new(execution: std::sync::Arc<dyn RoutingExecutionReadPort>) -> Self {
        Self { execution }
    }
}

pub(crate) trait RoutingRepository: Send + Sync {
    /// Loads the canonical immutable planning input for one request. `None`
    /// means the V1 policy aggregate has not been configured yet; callers must
    /// keep that state distinct from an empty candidate set. `context` is
    /// caller-owned and absolute: replans must reuse it rather than starting
    /// another request budget.
    #[cfg(test)]
    fn load_planning_snapshot(
        &self,
        request: RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
        context: PlanningRequestContext,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<PlanningSnapshot>, RoutingExecutionReadError>,
    >;

    fn load_planning_snapshot_with_probe(
        &self,
        request: RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
        context: PlanningRequestContext,
        probe: Option<HealthProtectionProbe>,
        probe_mode: HealthProbeAdmissionMode,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<PlanningSnapshot>, RoutingExecutionReadError>,
    >;

    fn load_execution_settings(
        &self,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<RoutingExecutionSettings, RoutingExecutionReadError>,
    >;

    fn load_balance_snapshots(
        &self,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<BalanceSnapshot>, RoutingExecutionReadError>,
    >;

    fn load_operational_route_snapshot(
        &self,
        request: RouteRequestFacts,
        planning_snapshot: PlanningSnapshot,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<OperationalRouteSnapshot, RoutingExecutionReadError>,
    >;

    /// Reloads the authoritative execution row for a retry. The immutable
    /// planning snapshot remains the expected commitment; this fresh read is
    /// the current side of the resolver fence.
    fn load_current_execution_target(
        &self,
        station_key_id: String,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<ExecutionTargetRef>, RoutingExecutionReadError>,
    >;

    /// Returns the durable health protection projection used to decide whether
    /// a real request may consume a Half-Open probe lease. The default keeps
    /// lightweight test repositories fail-closed and does not invent health.
    fn load_health_protection_statuses(
        &self,
        _now_ms: i64,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<HealthProtectionStatus>, RoutingExecutionReadError>,
    >;

    /// Reserves one durable, revision-fenced probe. Reservation is atomic and
    /// never performs network I/O; the caller must attach the returned fence
    /// to a real request observation.
    fn begin_health_protection_probe(
        &self,
        _scope: HealthProtectionScope,
        _now_ms: i64,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<HealthProtectionProbe>, RoutingExecutionReadError>,
    >;

    /// Releases a reserved probe when no outbound attempt was started. The
    /// application/store implementation is revision-fenced and idempotent.
    fn cancel_health_protection_probe(
        &self,
        _probe: HealthProtectionProbe,
        _now_ms: i64,
    ) -> futures_util::future::BoxFuture<'static, Result<bool, RoutingExecutionReadError>>;
}

impl RoutingRepository for RoutingExecutionRepository {
    #[cfg(test)]
    fn load_planning_snapshot(
        &self,
        request: RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
        context: PlanningRequestContext,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<PlanningSnapshot>, RoutingExecutionReadError>,
    > {
        self.execution
            .load_planning_snapshot(request, runtime, context)
    }

    fn load_planning_snapshot_with_probe(
        &self,
        request: RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
        context: PlanningRequestContext,
        probe: Option<HealthProtectionProbe>,
        probe_mode: HealthProbeAdmissionMode,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<PlanningSnapshot>, RoutingExecutionReadError>,
    > {
        self.execution
            .load_planning_snapshot_with_probe(request, runtime, context, probe, probe_mode)
    }

    fn load_execution_settings(
        &self,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<RoutingExecutionSettings, RoutingExecutionReadError>,
    > {
        self.execution.load_execution_settings()
    }

    fn load_balance_snapshots(
        &self,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<BalanceSnapshot>, RoutingExecutionReadError>,
    > {
        self.execution.load_balance_snapshots()
    }

    fn load_operational_route_snapshot(
        &self,
        _request: RouteRequestFacts,
        planning_snapshot: PlanningSnapshot,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<OperationalRouteSnapshot, RoutingExecutionReadError>,
    > {
        let execution = self.execution.clone();
        Box::pin(async move {
            let execution_settings = execution.load_execution_settings().await?;
            let station_key_ids = planning_snapshot
                .candidates
                .iter()
                .map(|candidate| candidate.station_key_id.clone())
                .collect::<Vec<_>>();
            let target_rows = execution
                .load_operational_execution_target_refs(station_key_ids)
                .await?;
            let targets = target_rows
                .into_iter()
                .map(|mut target| {
                    apply_effective_proxy_config(&mut target, &execution_settings);
                    (target.station_key_id.clone(), target)
                })
                .collect::<BTreeMap<_, _>>();
            let mut profiles = BTreeMap::new();
            let candidates = planning_snapshot
                .candidates
                .iter()
                .flat_map(|candidate| {
                    let target = targets.get(&candidate.station_key_id);
                    profiles.insert(
                        candidate.station_key_id.clone(),
                        CandidateAdmissionProfile {
                            endpoint_revision: candidate.endpoint_revision,
                            expected_credential_revision: candidate.credential_revision,
                            credential_revision: target
                                .map(|target| target.credential_revision)
                                .unwrap_or_default(),
                            durable_generation: planning_snapshot.durable_revision,
                            global_max_concurrency: planning_snapshot.runtime.max_concurrency.max(1),
                            station_account_max_concurrency: target
                                .map(|target| target.station_account_max_concurrency)
                                .unwrap_or_default(),
                            station_key_max_concurrency: target
                                .map(|target| target.station_key_max_concurrency)
                                .unwrap_or_default(),
                            provider_account_constraint: crate::application::routing_engine::capacity::ProviderAccountConstraint::NotApplicable,
                            half_open_probe_id: None,
                        },
                    );
                    let variants = if candidate.model_variants.is_empty() {
                        vec![None]
                    } else {
                        candidate
                            .model_variants
                            .iter()
                            .cloned()
                            .map(Some)
                            .collect::<Vec<_>>()
                    };
                    variants.into_iter().map(move |variant| {
                        RoutePlanCandidate {
                        station_key_id: candidate.station_key_id.clone(),
                        station_id: candidate.station_id.clone(),
                        endpoint_revision: candidate.endpoint_revision,
                        credential_revision: candidate.credential_revision,
                        account_revision: candidate.account_revision,
                        group_binding_id: candidate.group_binding_id.clone(),
                        group_revision: candidate.group_revision,
                        resolved_upstream_model: variant
                            .as_ref()
                            .map(|value| value.upstream_model.clone())
                            .or_else(|| candidate.resolved_upstream_model.clone()),
                        model_alias_revision: candidate.model_alias_revision,
                        model_variant: variant,
                        capacity_domain: candidate.capacity_domain.clone(),
                        capacity_domain_revision: candidate.capacity_domain_revision,
                        priority: 0,
                        tier: crate::application::routing_engine::candidate_plan::AvailabilityTier::Primary,
                        pricing: candidate.pricing.clone(),
                        evidence: vec![],
                        }
                    })
                })
                .collect::<Vec<_>>();
            Ok(OperationalRouteSnapshot {
                candidates,
                targets,
                profiles,
                #[cfg(test)]
                legacy_candidates: Vec::new(),
            })
        })
    }

    fn load_current_execution_target(
        &self,
        station_key_id: String,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<ExecutionTargetRef>, RoutingExecutionReadError>,
    > {
        let execution = self.execution.clone();
        Box::pin(async move {
            let execution_settings = execution.load_execution_settings().await?;
            let mut target = execution
                .load_operational_execution_target_refs(vec![station_key_id])
                .await
                .map(|mut targets| targets.pop())?;
            if let Some(target) = target.as_mut() {
                apply_effective_proxy_config(target, &execution_settings);
            }
            Ok(target)
        })
    }

    fn load_health_protection_statuses(
        &self,
        now_ms: i64,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<HealthProtectionStatus>, RoutingExecutionReadError>,
    > {
        self.execution.load_health_protection_statuses(now_ms)
    }

    fn begin_health_protection_probe(
        &self,
        scope: HealthProtectionScope,
        now_ms: i64,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Option<HealthProtectionProbe>, RoutingExecutionReadError>,
    > {
        self.execution.begin_health_protection_probe(scope, now_ms)
    }

    fn cancel_health_protection_probe(
        &self,
        probe: HealthProtectionProbe,
        now_ms: i64,
    ) -> futures_util::future::BoxFuture<'static, Result<bool, RoutingExecutionReadError>> {
        self.execution.cancel_health_protection_probe(probe, now_ms)
    }
}

fn apply_effective_proxy_config(
    target: &mut ExecutionTargetRef,
    settings: &RoutingExecutionSettings,
) {
    let resolved = resolve_routing_proxy_config(
        &target.collector_proxy_mode,
        target.collector_proxy_url.clone(),
        &settings.outbound_proxy_mode,
        settings.outbound_proxy_url.clone(),
        &settings.global_proxy_mode,
        settings.global_proxy_url.clone(),
    );
    target.collector_proxy_mode = resolved.mode;
    target.collector_proxy_url = resolved.url;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{
            operational_facts::{
                candidate_projection::validated_route_settings, pricing_projector::RoutingCostBasis,
            },
            routing_engine::request::{CanonicalRouteRequest, RouteKind, RouteRequestClassifier},
        },
        models::{
            pricing::UpsertModelBasePriceInput,
            routing::{RoutingGroupFilter, RoutingPolicy, RuntimeRoutingSettings},
        },
        services::proxy::test_support::V2ProxyTestFixture,
    };

    #[tokio::test]
    async fn v2_repository_projects_request_scoped_exact_pricing_into_operational_snapshot() {
        let fixture = V2ProxyTestFixture::new().await;
        let seeded = fixture.seed_candidate("http://127.0.0.1:9").await;
        fixture
            .services
            .pricing
            .upsert_model_base_price(UpsertModelBasePriceInput {
                id: Some("repository-token-price".to_string()),
                provider: "fixture".to_string(),
                model: "gpt-5".to_string(),
                input_price: Some(0.37),
                output_price: Some(0.74),
                input_price_priority: None,
                output_price_priority: None,
                cache_creation_price: None,
                cache_creation_price_priority: None,
                cache_creation_price_above_1hr: None,
                cache_read_price: None,
                cache_read_price_priority: None,
                long_context_input_token_threshold: None,
                long_context_input_cost_multiplier: None,
                long_context_output_cost_multiplier: None,
                supports_service_tier: false,
                supports_prompt_caching: false,
                currency: "USD".to_string(),
                unit: "per_1m_tokens".to_string(),
                source_url: "https://fixture.invalid/pricing".to_string(),
                source_label: "fixture".to_string(),
                source_checked_at: Some("123457".to_string()),
                enabled: true,
                built_in: false,
                note: None,
            })
            .await
            .expect("model base price");

        let request = RouteRequestClassifier::classify(
            CanonicalRouteRequest {
                route_kind: RouteKind::Inference,
                requested_model: Some("gpt-5".to_string()),
                stream: false,
                uses_tools: false,
                uses_vision: false,
                uses_reasoning: false,
                untrusted_headers: Vec::new(),
            },
            validated_route_settings(&RuntimeRoutingSettings {
                policy: RoutingPolicy::CostStableFirst,
                max_rate_multiplier: Some(2.0),
                routing_group_scope: RoutingGroupFilter::AllGroups,
                scheduler_config: Default::default(),
                allow_depleted_fallback: false,
                ..Default::default()
            }),
            123458,
        );
        let repository = RoutingExecutionRepository::new(std::sync::Arc::new(
            crate::application::routing_execution_reader::RoutingExecutionReader::new(
                fixture.services.routing.clone(),
            ),
        ));

        let planning_snapshot = RoutingRepository::load_planning_snapshot(
            &repository,
            request.clone(),
            RuntimeOverlaySnapshot {
                runtime_instance_id: "repository-runtime".to_string(),
                runtime_revision: 1,
                candidate_set_revision: 1,
                in_flight: 0,
                max_concurrency: 64,
                affinity_station_key_id: None,
            },
            PlanningRequestContext::from_now(std::time::Duration::from_secs(5)),
        )
        .await
        .expect("planning snapshot")
        .expect("configured routing policy");
        let expected_credential_revision = planning_snapshot.candidates[0].credential_revision;
        let snapshot = RoutingRepository::load_operational_route_snapshot(
            &repository,
            request,
            planning_snapshot,
        )
        .await
        .expect("operational route snapshot");

        assert_eq!(snapshot.candidates.len(), 1);
        let candidate = &snapshot.candidates[0];
        assert_eq!(candidate.station_key_id, seeded.station_key_id);
        assert_eq!(candidate.pricing.basis, RoutingCostBasis::ExactPrice);
        assert_eq!(candidate.pricing.status_label, "priced");
        assert_eq!(candidate.pricing.estimated_input_price, Some(0.37));
        let profile = snapshot
            .profiles
            .get(&seeded.station_key_id)
            .expect("candidate admission profile");
        assert_eq!(profile.global_max_concurrency, 64);
        assert_eq!(
            profile.expected_credential_revision,
            expected_credential_revision
        );
        assert!(profile.credential_revision > 0);
        assert_eq!(profile.station_account_max_concurrency, 0);
        assert_eq!(profile.station_key_max_concurrency, 0);
    }
}
