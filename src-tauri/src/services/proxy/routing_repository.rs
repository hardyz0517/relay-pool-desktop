use std::collections::BTreeMap;

use crate::{
    application::{
        operational_facts::target_resolver::ExecutionTargetRef,
        routing_engine::planning_snapshot::{PlanningSnapshot, RuntimeOverlaySnapshot},
        routing::RoutingService,
        routing_engine::{admission::CandidateAdmissionProfile, request::RouteRequestFacts},
    },
    models::{pricing::BalanceSnapshot, routing::RuntimeRoutingSettings},
};

use crate::application::routing_engine::candidate_plan::{RoutePlanCandidate, RoutePlanPricingSnapshot};
use crate::application::operational_facts::pricing_projector::RoutingCostBasis;

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
    pub(crate) snapshot_id: String,
    pub(crate) runtime_overlay_revision: u64,
    pub(crate) durable_generation: u64,
    #[cfg(test)]
    pub(crate) legacy_candidates: Vec<crate::application::operational_facts::candidate_projector::RouteCandidateProjection>,
}

#[derive(Clone)]
pub(crate) struct RoutingExecutionRepository {
    routing: RoutingService,
}

impl RoutingExecutionRepository {
    pub(crate) fn new(routing: RoutingService) -> Self {
        Self { routing }
    }
}

pub(crate) trait RoutingRepository: Send + Sync {
    /// Loads the canonical immutable planning input for one request. `None`
    /// means the V1 policy aggregate has not been configured yet; callers must
    /// keep that state distinct from an empty candidate set.
    fn load_planning_snapshot(
        &self,
        request: RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
    ) -> futures_util::future::BoxFuture<'static, Result<Option<PlanningSnapshot>, String>>;

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

    fn load_operational_route_snapshot(
        &self,
        request: RouteRequestFacts,
        planning_snapshot: PlanningSnapshot,
    ) -> futures_util::future::BoxFuture<'static, Result<OperationalRouteSnapshot, String>>;
}

impl RoutingRepository for RoutingExecutionRepository {
    fn load_planning_snapshot(
        &self,
        request: RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
    ) -> futures_util::future::BoxFuture<'static, Result<Option<PlanningSnapshot>, String>> {
        let routing = self.routing.clone();
        Box::pin(async move {
            routing
                .load_intelligent_planning_snapshot(&request, runtime)
                .await
                .map_err(|error| format!("load intelligent planning snapshot failed: {error}"))
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

    fn load_operational_route_snapshot(
        &self,
        _request: RouteRequestFacts,
        planning_snapshot: PlanningSnapshot,
    ) -> futures_util::future::BoxFuture<'static, Result<OperationalRouteSnapshot, String>> {
        let routing = self.routing.clone();
        Box::pin(async move {
            let station_key_ids = planning_snapshot
                .candidates
                .iter()
                .map(|candidate| candidate.station_key_id.clone())
                .collect::<Vec<_>>();
            let target_rows = routing
                .load_operational_execution_target_refs(station_key_ids)
                .await
                .map_err(|error| format!("load operational target refs failed: {error}"))?;
            let targets = target_rows
                .into_iter()
                .map(|target| (target.station_key_id.clone(), target))
                .collect::<BTreeMap<_, _>>();
            let mut profiles = BTreeMap::new();
            let candidates = planning_snapshot
                .candidates
                .iter()
                .map(|candidate| {
                    let target = targets.get(&candidate.station_key_id);
                    profiles.insert(
                        candidate.station_key_id.clone(),
                        CandidateAdmissionProfile {
                            endpoint_revision: candidate.endpoint_revision,
                            expected_credential_revision: candidate.endpoint_revision,
                            credential_revision: candidate.endpoint_revision,
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
                    RoutePlanCandidate {
                        station_key_id: candidate.station_key_id.clone(),
                        station_id: candidate.station_id.clone(),
                        endpoint_revision: candidate.endpoint_revision,
                        priority: 0,
                        tier: crate::application::routing_engine::candidate_plan::AvailabilityTier::Primary,
                        pricing: RoutePlanPricingSnapshot {
                            basis: RoutingCostBasis::Unpriced,
                            currency: None,
                            unit: None,
                            estimated_input_price: None,
                            estimated_output_price: None,
                            estimated_fixed_price: None,
                            status_label: "planner_snapshot".to_string(),
                        },
                        evidence: vec![],
                    }
                })
                .collect::<Vec<_>>();
            Ok(OperationalRouteSnapshot {
                candidates,
                targets,
                profiles,
                snapshot_id: planning_snapshot.snapshot_id.clone(),
                runtime_overlay_revision: planning_snapshot.runtime.runtime_revision,
                durable_generation: planning_snapshot.durable_revision,
                #[cfg(test)]
                legacy_candidates: Vec::new(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{
            operational_facts::{
                pricing_projector::RoutingCostBasis,
                candidate_projection::validated_route_settings,
            },
            routing_engine::request::{CanonicalRouteRequest, RouteKind, RouteRequestClassifier},
        },
        models::{
            pricing::UpsertPricingRuleInput,
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
            .upsert_pricing_rule(UpsertPricingRuleInput {
                id: Some("repository-exact-price".to_string()),
                station_id: seeded.station_id.clone(),
                station_key_id: Some(seeded.station_key_id.clone()),
                group_binding_id: None,
                group_name: None,
                tier_label: None,
                model: "gpt-5".to_string(),
                input_price: None,
                output_price: None,
                fixed_price: Some(0.37),
                rate_multiplier: None,
                currency: "USD".to_string(),
                unit: "per_request".to_string(),
                price_type: "fixed".to_string(),
                base_price_source: None,
                normalization_status: Some("complete".to_string()),
                source: "manual".to_string(),
                confidence: 0.99,
                enabled: true,
                note: None,
                collected_at: Some("123457".to_string()),
                valid_from: None,
                valid_until: None,
            })
            .await
            .expect("pricing rule");

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
            }),
            123458,
        );
        let repository = RoutingExecutionRepository::new(fixture.services.routing.as_ref().clone());

        let planning_snapshot = PlanningSnapshot {
            snapshot_id: "repository-planning-snapshot".to_string(),
            durable_revision: 1,
            policy: crate::models::routing_policy::RoutingPolicyConfigV1::default(),
            profile: crate::application::routing_engine::algorithm_profile::DispatchAlgorithmProfile::default(),
            candidates: vec![crate::application::routing_engine::planning_snapshot::CandidateSnapshot {
                station_key_id: seeded.station_key_id.clone(),
                station_id: seeded.station_id.clone(),
                endpoint_revision: 1,
                credential_available: true,
                hard_eligible: true,
                backup_only: false,
                depleted: false,
                capability_basis_points: 10_000,
                reliability_basis_points: 8_000,
                responsiveness_basis_points: 8_000,
                cost_basis_points: Some(8_000),
                preference_basis_points: 5_000,
                failure_domains: vec![format!("station:{}", seeded.station_id)],
            }],
            runtime: crate::application::routing_engine::planning_snapshot::RuntimeOverlaySnapshot {
                runtime_instance_id: "repository-runtime".to_string(),
                runtime_revision: 1,
                candidate_set_revision: 1,
                in_flight: 0,
                max_concurrency: 64,
                affinity_station_key_id: None,
            },
        };
        let snapshot = RoutingRepository::load_operational_route_snapshot(&repository, request, planning_snapshot)
            .await
            .expect("operational route snapshot");

        assert_eq!(snapshot.candidates.len(), 1);
        let candidate = &snapshot.candidates[0];
        assert_eq!(candidate.station_key_id, seeded.station_key_id);
        assert_eq!(candidate.pricing.basis, RoutingCostBasis::Unpriced);
        assert_eq!(candidate.pricing.status_label, "planner_snapshot");
        let profile = snapshot
            .profiles
            .get(&seeded.station_key_id)
            .expect("candidate admission profile");
        assert_eq!(profile.global_max_concurrency, 64);
        assert_eq!(profile.station_account_max_concurrency, 0);
        assert_eq!(profile.station_key_max_concurrency, 0);
    }
}
