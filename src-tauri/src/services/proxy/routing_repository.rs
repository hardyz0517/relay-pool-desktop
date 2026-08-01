use std::collections::BTreeMap;

use crate::{
    application::{
        operational_facts::{
            candidate_projector::RouteCandidateProjection,
            runtime_candidate_adapter::{
                admission_profile_from_runtime_candidate,
                route_projection_from_runtime_candidate_with_pricing,
            },
            target_resolver::ExecutionTargetRef,
        },
        routing::RoutingService,
        routing_engine::{controller::CandidateAdmissionProfile, request::RouteRequestFacts},
    },
    models::{pricing::BalanceSnapshot, routing::RuntimeRoutingSettings},
};

pub(crate) type RoutingExecutionSettings = RuntimeRoutingSettings;

#[cfg(test)]
pub(crate) use crate::application::operational_facts::runtime_candidate_adapter::{
    admission_profile_from_runtime_candidate as admission_profile_from_candidate,
    route_projection_from_runtime_candidate as route_projection_from_runtime,
};

#[derive(Debug, Clone)]
pub(crate) struct OperationalRouteSnapshot {
    pub(crate) candidates: Vec<RouteCandidateProjection>,
    pub(crate) targets: BTreeMap<String, ExecutionTargetRef>,
    pub(crate) profiles: BTreeMap<String, CandidateAdmissionProfile>,
    pub(crate) snapshot_id: String,
    pub(crate) runtime_overlay_revision: u64,
    pub(crate) durable_generation: u64,
}

#[derive(Clone)]
pub(crate) struct V2RoutingRepository {
    routing: RoutingService,
}

impl V2RoutingRepository {
    pub(crate) fn new(routing: RoutingService) -> Self {
        Self { routing }
    }
}

pub(crate) trait RoutingRepository: Send + Sync {
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
    ) -> futures_util::future::BoxFuture<'static, Result<OperationalRouteSnapshot, String>>;
}

impl RoutingRepository for V2RoutingRepository {
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
        request: RouteRequestFacts,
    ) -> futures_util::future::BoxFuture<'static, Result<OperationalRouteSnapshot, String>> {
        let routing = self.routing.clone();
        Box::pin(async move {
            let candidates = routing
                .load_runtime_candidates_with_request_pricing(&request)
                .await
                .map_err(|error| format!("load V2 route candidates failed: {error}"))?;
            let station_key_ids = candidates
                .iter()
                .map(|row| row.candidate.station_key_id.clone())
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
            let projected = candidates
                .into_iter()
                .map(|row| {
                    let profile = admission_profile_from_runtime_candidate(&row.candidate);
                    profiles.insert(row.candidate.station_key_id.clone(), profile);
                    route_projection_from_runtime_candidate_with_pricing(
                        &request,
                        row.candidate,
                        row.pricing_context.as_ref(),
                    )
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(OperationalRouteSnapshot {
                candidates: projected,
                targets,
                profiles,
                snapshot_id: format!(
                    "runtime-candidates-{}",
                    chrono::Utc::now().timestamp_millis()
                ),
                runtime_overlay_revision: 1,
                durable_generation: 1,
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
                runtime_candidate_adapter::validated_route_settings,
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
                routing_group_filter: RoutingGroupFilter::AllGroups,
                scheduler_advanced_settings: Default::default(),
                allow_depleted_fallback: false,
            }),
            123458,
        );
        let repository = V2RoutingRepository::new(fixture.services.routing.as_ref().clone());

        let snapshot = RoutingRepository::load_operational_route_snapshot(&repository, request)
            .await
            .expect("operational route snapshot");

        assert_eq!(snapshot.candidates.len(), 1);
        let candidate = &snapshot.candidates[0];
        assert_eq!(candidate.identity.station_key_id, seeded.station_key_id);
        assert_eq!(candidate.pricing.basis, RoutingCostBasis::ExactPrice);
        assert_eq!(candidate.pricing.comparison_value, Some(0.37));
        assert_eq!(candidate.pricing.currency.as_deref(), Some("USD"));
        assert_eq!(candidate.pricing.status_label, "priced");
    }
}
