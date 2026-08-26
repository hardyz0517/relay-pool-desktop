use std::{collections::BTreeMap, sync::Arc, time::Duration};

use sha2::{Digest, Sha256};

/// Bounded budget for top-level local read-model operations that invoke the
/// planner without an ingress request. Proxy traffic supplies its own
/// ingress-owned absolute deadline instead.
const NON_PROXY_PLANNING_DEADLINE: Duration = Duration::from_secs(5);

use crate::{
    application::routing_engine::{
        algorithm_profile::DispatchAlgorithmProfile,
        candidate_plan::RoutePlanPricingSnapshot,
        intelligent_planner::{
            candidate_score_breakdown_with_cost_basis, plan_snapshot_with_budget,
        },
        planning_snapshot::{PlanningSnapshot, RuntimeOverlaySnapshot},
        request::{
            CanonicalRouteRequest, PlanningRequestContext, RouteKind, RouteRequestClassifier,
        },
    },
    application::{
        credentials::SecretRef,
        error::ApplicationError,
        error_rate_protection::ErrorRateProtectionService,
        health_transitions::HealthTransitionService,
        operational_facts::{
            candidate_projection::{
                route_projection_from_runtime_candidate_with_pricing,
                route_request_facts_for_read_model, validated_route_settings,
            },
            planning_snapshot::PlanningSnapshotBuilder,
            pricing_projector::{
                pricing_context_from_resolution, request_cost_comparison_context, PricingRouteKind,
            },
            target_resolver::ExecutionTargetRef,
        },
        queries::{
            operational_detail::{
                operational_detail_from_projection, unavailable_operational_detail,
                StationKeyOperationalDetail,
            },
            routing_protection::{
                project_routing_protection_status_with_reducer_and_domains, CapacityProtectionFact,
                FailureDomainCandidateFact, RoutingProtectionStatus,
            },
            routing_runtime::{
                monitoring_target_snapshots_from_facts, runtime_overlay_from_candidates,
                RoutingMonitoringTargetFacts, RoutingMonitoringTargetSnapshot,
                RoutingRuntimeActivity, RoutingRuntimeCandidateFact, RoutingRuntimeOverlay,
            },
            routing_workspace::{
                workspace_snapshot_from_canonical_candidates, RoutingWorkspaceSnapshot,
                RoutingWorkspaceSnapshotInput,
            },
        },
        routing_policy::RoutingPolicyAggregate,
    },
    models::{
        document_sync::TrustedDocumentSource,
        health::{
            HealthObservation, HealthObservationOutcome, HealthObservationSource,
            HealthWritebackMode, TrafficEquivalence,
        },
        pricing::{BalanceSnapshot, ResolvedPricingContext},
        routing::{
            CanonicalRoutingCandidate, RouteCandidateExplanation, RouteEndpointKind,
            RouteSimulationInput, RouteSimulationResult, RoutingGroupFilter,
            RuntimeRoutingSettings,
        },
        stations::StationEndpointHealth,
    },
    persistence::{
        error::PersistenceError,
        runtime::PersistenceHandle,
        stores::pricing_store::PricingStore,
        stores::routing_policy_store::RoutingPolicyStore,
        stores::routing_quality_store::RoutingQualityStore,
        stores::routing_store::{RoutingStore, StationEndpointProbeTarget},
    },
    services::policy_documents::{
        canonical_json, decode_strict_json, PolicyDocumentCoordinator, PolicyDocumentError,
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
    error_rate: ErrorRateProtectionService,
}

impl RoutingService {
    pub(crate) fn routing_policy_config_directory(&self) -> Option<std::path::PathBuf> {
        self.runtime
            .database_path()
            .parent()
            .map(|root| root.join("config"))
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

    /// Compatibility read used only by the proxy execution bridge for the
    /// local usage response. Command/query callers use their dedicated read
    /// owners; keeping this narrow bridge method avoids coupling proxy
    /// startup to the command facade while its balance port is migrated.
    pub(crate) async fn list_balance_snapshots(
        &self,
    ) -> Result<Vec<BalanceSnapshot>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .list_balance_snapshots(&mut read)
            .await
            .map_err(Into::into)
    }

    #[cfg(test)]
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self::new_with_error_rate(runtime, ErrorRateProtectionService::disabled())
    }

    pub(crate) fn new_with_error_rate(
        runtime: PersistenceHandle,
        error_rate: ErrorRateProtectionService,
    ) -> Self {
        Self {
            runtime,
            store: RoutingStore,
            error_rate,
        }
    }

    pub(crate) async fn load_routing_policy(
        &self,
    ) -> Result<
        crate::persistence::stores::routing_policy_store::StoredRoutingPolicy,
        ApplicationError,
    > {
        let mut read = self.runtime.begin_read().await?;
        RoutingPolicyStore
            .load(read.connection())
            .await
            .map_err(ApplicationError::from)?
            .ok_or(ApplicationError::NotFound)
    }

    pub(crate) async fn refresh_protection_configuration(&self) -> Result<(), ApplicationError> {
        let stored = self.load_routing_policy().await?;
        let policy =
            crate::models::routing_policy::RoutingPolicyConfigV2::from_stored_value(&stored.config)
                .map_err(|_| ApplicationError::ConstraintViolation)?;
        self.error_rate
            .set_enabled(policy.protection_profile.enabled);
        Ok(())
    }

    pub(crate) async fn get_routing_protection_status(
        &self,
        generated_at_ms: i64,
        capacity: &[CapacityProtectionFact],
        runtime_capacity_available: bool,
        requested_model: Option<&str>,
    ) -> Result<RoutingProtectionStatus, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let durable =
            crate::persistence::stores::routing_health_verdict_store::RoutingHealthVerdictStore
                .load_active_all(read.connection())
                .await
                .map_err(ApplicationError::from)?;
        let protection_enabled = self.load_protection_enabled(&mut read).await?;
        let reducer_statuses = if protection_enabled {
            crate::persistence::stores::routing_health_verdict_store::RoutingHealthVerdictStore
                .load_health_protection_statuses(read.connection(), generated_at_ms.max(0))
                .await
                .map_err(ApplicationError::from)?
        } else {
            Vec::new()
        };
        let legacy = self
            .store
            .list_station_key_health(&mut read)
            .await
            .map_err(ApplicationError::from)?;
        let domain_facts = self
            .store
            .load_runtime_candidates(&mut read)
            .await
            .map_err(ApplicationError::from)?
            .into_iter()
            .map(|candidate| FailureDomainCandidateFact {
                provider_family: candidate.capacity_provider_family,
                deployment_identity: candidate.capacity_deployment_identity,
                region_identity: candidate.capacity_region_identity,
                revision: candidate.capacity_domain_revision,
                schedulable: candidate.schedulable,
            })
            .collect::<Vec<_>>();
        Ok(project_routing_protection_status_with_reducer_and_domains(
            generated_at_ms.max(0),
            &durable,
            &legacy,
            capacity,
            runtime_capacity_available,
            &reducer_statuses,
            &domain_facts,
            requested_model,
        ))
    }

    pub(crate) async fn load_health_protection_statuses(
        &self,
        now_ms: i64,
    ) -> Result<Vec<crate::application::health_protection::HealthProtectionStatus>, ApplicationError>
    {
        let mut read = self.runtime.begin_read().await?;
        let enabled = self.load_protection_enabled(&mut read).await?;
        if !enabled {
            return Ok(Vec::new());
        }
        crate::persistence::stores::routing_health_verdict_store::RoutingHealthVerdictStore
            .load_health_protection_statuses(read.connection(), now_ms.max(0))
            .await
            .map_err(ApplicationError::from)
    }

    async fn load_protection_enabled(
        &self,
        read: &mut crate::persistence::ReadSession,
    ) -> Result<bool, ApplicationError> {
        let stored = RoutingPolicyStore
            .load(read.connection())
            .await
            .map_err(ApplicationError::from)?
            .ok_or(ApplicationError::NotFound)?;
        let policy =
            crate::models::routing_policy::RoutingPolicyConfigV2::from_stored_value(&stored.config)
                .map_err(|_| ApplicationError::ConstraintViolation)?;
        Ok(policy.protection_profile.enabled)
    }

    pub(crate) async fn begin_health_protection_probe(
        &self,
        scope: crate::application::health_protection::HealthProtectionScope,
        now_ms: i64,
    ) -> Result<
        Option<crate::application::health_protection::HealthProtectionProbe>,
        ApplicationError,
    > {
        let store =
            crate::persistence::stores::routing_health_verdict_store::RoutingHealthVerdictStore;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .begin_health_protection_probe(write.connection(), &scope, now_ms.max(0))
                        .await
                })
            })
            .await
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn cancel_health_protection_probe(
        &self,
        probe: crate::application::health_protection::HealthProtectionProbe,
        now_ms: i64,
    ) -> Result<bool, ApplicationError> {
        let store =
            crate::persistence::stores::routing_health_verdict_store::RoutingHealthVerdictStore;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .cancel_health_protection_probe(write.connection(), &probe, now_ms.max(0))
                        .await
                })
            })
            .await
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn apply_routing_policy_document_v2(
        &self,
        document: crate::models::routing_policy::RoutingPolicyDocumentV2,
        _source: TrustedDocumentSource,
    ) -> Result<
        crate::persistence::stores::routing_policy_store::StoredRoutingPolicy,
        ApplicationError,
    > {
        document
            .validate()
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let value = serde_json::to_value(&document.policy)
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut write = self.runtime.begin_write().await?;
        let stored = RoutingPolicyStore
            .save_compare_and_swap(
                write.connection(),
                Some(document.base_revision),
                &value,
                "routing-policy-v2",
                "routing-system-v1",
                "active",
                now_ms,
            )
            .await
            .map_err(ApplicationError::from)?;
        write.commit().await?;
        // SQLite remains the active truth. The managed JSON mirror is updated
        // only after the transaction commits, and a failed materialization is
        // recorded in document-sync state without rolling back the policy.
        sync_routing_policy_file(self.runtime.clone(), &stored, true).await?;
        self.error_rate
            .set_enabled(document.policy.protection_profile.enabled);
        // A no-op CAS returns the current revision and must not emit a false
        // invalidation. Every changed revision publishes only after commit.
        if stored.revision != document.base_revision {
            crate::application::queries::read_model_revision::publish_domain_revision_notice(
                crate::application::queries::read_model_revision::DomainRevisionNotice::for_scope(
                    "routing_policy",
                    i64::try_from(stored.revision)
                        .map_err(|_| ApplicationError::ConstraintViolation)?,
                ),
            );
        }
        Ok(stored)
    }

    pub(crate) async fn reconcile_external_routing_policy_document(
        &self,
    ) -> Result<
        Option<crate::persistence::stores::routing_policy_store::StoredRoutingPolicy>,
        PersistenceError,
    > {
        reconcile_external_routing_policy_document(self.runtime.clone(), self).await
    }

    /// Build the production planner input from one read transaction. The
    /// policy aggregate is deliberately read here, at the application
    /// boundary, so the proxy never parses settings or assembles candidates
    /// itself. A missing aggregate is a configuration-required state.
    pub async fn load_intelligent_planning_snapshot(
        &self,
        request: &crate::application::routing_engine::request::RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
        context: PlanningRequestContext,
    ) -> Result<Option<PlanningSnapshot>, ApplicationError> {
        self.load_intelligent_planning_snapshot_with_probe(
            request,
            runtime,
            context,
            None,
            crate::application::health_protection::HealthProbeAdmissionMode::Normal,
        )
        .await
    }

    pub(crate) async fn load_intelligent_planning_snapshot_with_probe(
        &self,
        request: &crate::application::routing_engine::request::RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
        context: PlanningRequestContext,
        health_probe: Option<crate::application::health_protection::HealthProtectionProbe>,
        health_probe_mode: crate::application::health_protection::HealthProbeAdmissionMode,
    ) -> Result<Option<PlanningSnapshot>, ApplicationError> {
        tokio::time::timeout_at(
            tokio::time::Instant::from_std(context.deadline()),
            self.load_intelligent_planning_snapshot_within_deadline(
                request,
                runtime,
                health_probe,
                health_probe_mode,
            ),
        )
        .await
        .map_err(|_| ApplicationError::DeadlineExceeded)?
    }

    async fn load_intelligent_planning_snapshot_within_deadline(
        &self,
        request: &crate::application::routing_engine::request::RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
        health_probe: Option<crate::application::health_protection::HealthProtectionProbe>,
        health_probe_mode: crate::application::health_protection::HealthProbeAdmissionMode,
    ) -> Result<Option<PlanningSnapshot>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let stored = RoutingPolicyStore
            .load(read.connection())
            .await
            .map_err(ApplicationError::from)?;
        let Some(stored) = stored else {
            return Ok(None);
        };
        let routing_policy_revision = stored.revision;
        let aggregate = RoutingPolicyAggregate::from_stored(stored)
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let compiled = aggregate
            .compile_v2()
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let options = request
            .requested_model()
            .map(crate::models::operational::OperationalFactReadOptions::for_request_model)
            .map(|options| options.without_legacy_aliases())
            .unwrap_or_else(|| {
                crate::models::operational::OperationalFactReadOptions::for_model_catalog()
                    .without_legacy_aliases()
            });
        let policy = aggregate.policy.clone();
        let builder = PlanningSnapshotBuilder;
        // Protection activation is part of the immutable policy snapshot;
        // the service only supplies the typed adapter and bounded history
        // settings. This keeps a stale process-local default from overriding
        // the policy revision selected for this request.
        let error_rate_admission = self
            .error_rate
            .admission_config_for_policy(compiled.protection_enabled);
        let error_rate_statuses = if error_rate_admission.enabled {
            crate::persistence::stores::routing_health_verdict_store::RoutingHealthVerdictStore
                .load_health_protection_statuses(
                    read.connection(),
                    chrono::Utc::now().timestamp_millis().max(0),
                )
                .await
                .map_err(ApplicationError::from)?
        } else {
            Vec::new()
        };
        let mut snapshot = builder
            .build(
                &mut read,
                &options,
                policy,
                routing_policy_revision,
                compiled.attempt_budget,
                DispatchAlgorithmProfile::default(),
                runtime,
                request,
                error_rate_admission,
                &error_rate_statuses,
                health_probe.as_ref(),
                health_probe_mode,
            )
            .await
            .map_err(|error| {
                #[cfg(test)]
                crate::observability::runtime::bootstrap::emit(
                    crate::services::proxy::runtime_events::planning_snapshot_failed(),
                );
                let _ = error;
                ApplicationError::ConstraintViolation
            })?;
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
                    let resolved = pricing_context_from_resolution(
                        &candidate.station_key_id,
                        model,
                        Some(resolution),
                    );
                    let request_pricing = request_cost_comparison_context(
                        PricingRouteKind::Inference,
                        Some(&resolved),
                    );
                    // The workspace score is a key-level routing signal. Use
                    // the trusted effective multiplier as its stable cost
                    // proxy instead of inventing a request price from one
                    // input/output tariff.
                    candidate.cost_basis_points = resolved
                        .effective_rate_multiplier
                        .and_then(crate::application::routing_engine::factors::cost_efficiency_from_multiplier);
                    candidate.pricing = RoutePlanPricingSnapshot {
                        basis: request_pricing.basis,
                        rate_multiplier: resolved.effective_rate_multiplier,
                        currency: request_pricing.currency,
                        unit: request_pricing.unit,
                        estimated_input_price: request_pricing.estimated_input_price,
                        estimated_output_price: request_pricing.estimated_output_price,
                        estimated_cache_creation_price: request_pricing
                            .estimated_cache_creation_price,
                        estimated_cache_read_price: request_pricing.estimated_cache_read_price,
                        status_label: request_pricing.status_label,
                    };
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
                    .resolve_station_key_pricing_many(read, &station_key_ids, requested_model, &at)
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
                    station_type: row.station_type,
                    capacity_provider_family: row.capacity_provider_family,
                    capacity_deployment_identity: row.capacity_deployment_identity,
                    capacity_region_identity: row.capacity_region_identity,
                    capacity_domain_revision: row.capacity_domain_revision,
                    group_binding_id: row.group_binding_id,
                    endpoint_revision: row.endpoint_revision,
                    credential_revision: row.credential_revision,
                    account_revision: row.account_revision,
                    group_revision: row.group_revision,
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
        // Read-model callers own a bounded budget at their application
        // boundary. The same absolute context is reused for every planning
        // read in this operation; each helper must not restart it.
        let planning_context = PlanningRequestContext::from_now(NON_PROXY_PLANNING_DEADLINE);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let (settings, policy_config, request, candidates) = {
            let mut read = self.runtime.begin_read().await?;
            let settings = self.store.load_execution_settings(&mut read).await?;
            let stored_policy = RoutingPolicyStore
                .load(read.connection())
                .await
                .map_err(ApplicationError::from)?
                .ok_or(ApplicationError::NotFound)?;
            let aggregate = RoutingPolicyAggregate::from_stored(stored_policy)
                .map_err(|_| ApplicationError::ConstraintViolation)?;
            let policy_config = aggregate.policy.clone();
            let request = route_request_facts_for_read_model(&settings, now_ms);
            let candidates = self
                .load_workspace_candidates_with_request_pricing_in_read(&mut read, &request)
                .await?
                .into_iter()
                .map(|row| (row.candidate, row.pricing_context))
                .collect::<Vec<_>>();
            (settings, policy_config, request, candidates)
        };
        // Use the immutable planner snapshot as the score source so this
        // read model stays aligned with the actual routing policy semantics.
        let multiplier_by_key = candidates
            .iter()
            .filter_map(|(candidate, pricing)| {
                let multiplier = pricing
                    .as_ref()
                    .and_then(|context| context.effective_rate_multiplier)
                    .or_else(|| {
                        let economics = candidate.economic_snapshot.as_ref()?;
                        crate::application::operational_facts::pricing_projector::effective_rate_multiplier(
                            economics.rate_multiplier,
                            economics.credit_per_cny.unwrap_or(1.0),
                        )
                    })?;
                Some((candidate.station_key_id.clone(), multiplier))
            })
            .collect::<BTreeMap<_, _>>();
        let planning_snapshot = match self
            .load_intelligent_planning_snapshot(
                &request,
                RuntimeOverlaySnapshot {
                    runtime_instance_id: "routing-workspace".to_string(),
                    runtime_revision: 1,
                    candidate_set_revision: 1,
                    in_flight: 0,
                    max_concurrency: 1,
                    affinity_station_key_id: None,
                },
                planning_context,
            )
            .await
        {
            Ok(snapshot) => snapshot,
            // A local workspace read may tolerate an unavailable planner
            // snapshot, but it must not hide the caller-owned deadline.
            Err(ApplicationError::DeadlineExceeded) => {
                return Err(ApplicationError::DeadlineExceeded)
            }
            Err(_) => None,
        };
        let score_by_key = planning_snapshot
            .map(|snapshot| {
                snapshot
                    .candidates
                    .iter()
                    .filter_map(|candidate| {
                        let multiplier_cost_basis = multiplier_by_key
                            .get(&candidate.station_key_id)
                            .copied()
                            .and_then(
                                crate::application::routing_engine::factors::
                                    cost_efficiency_from_multiplier,
                            );
                        candidate_score_breakdown_with_cost_basis(
                            candidate,
                            &policy_config,
                            None,
                            multiplier_cost_basis,
                        )
                        .map(|breakdown| (candidate.station_key_id.clone(), breakdown.into()))
                    })
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        let quality_summaries = {
            let scopes = candidates
                .iter()
                .map(|(candidate, _)| format!("station_key:{}", candidate.station_key_id))
                .collect::<Vec<_>>();
            let mut read = self.runtime.begin_read().await?;
            RoutingQualityStore
                .load_summary_json(read.connection(), &scopes)
                .await?
                .into_iter()
                .filter_map(|(scope, value)| {
                    serde_json::from_value::<crate::application::quality_projection::QualitySummary>(value)
                        .ok()
                        .map(|summary| (scope, summary))
                })
                .collect::<BTreeMap<_, _>>()
        };
        Ok(workspace_snapshot_from_canonical_candidates(
            policy_config,
            settings.max_rate_multiplier,
            settings.routing_group_scope,
            candidates,
            &score_by_key,
            &quality_summaries,
            &request,
            input,
            now_ms,
        ))
    }

    pub(crate) async fn load_routing_runtime_overlay(
        &self,
        proxy: Arc<dyn RoutingRuntimeActivity>,
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
                .active_for_station(
                    &candidate.station_type,
                    &candidate.station_id,
                    &candidate.station_key_id,
                )
                .await
                .or(candidate.load_factor);
            let station_key_in_flight = proxy
                .active_for_station_key(&candidate.station_key_id)
                .await;
            facts.push(RoutingRuntimeCandidateFact {
                station_key_id: candidate.station_key_id,
                station_id: candidate.station_id,
                endpoint_revision: candidate.station_endpoint_revision,
                in_flight,
                station_key_in_flight,
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
        // Simulation is a local read-model operation, so it creates one
        // bounded context at its command boundary and carries that absolute
        // deadline through settings, planning and projection work.
        let planning_context = PlanningRequestContext::from_now(NON_PROXY_PLANNING_DEADLINE);
        let mut read = self.runtime.begin_read().await?;
        let settings = self.store.load_execution_settings(&mut read).await?;
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
        let mapping_plan = input.model.clone().and_then(|requested| {
            crate::application::model_mapping::resolve_request(
                Some(requested),
                mapping_endpoint_kind(&input.endpoint),
                input.stream,
                input.uses_tools,
                input.uses_vision,
                input.uses_reasoning,
            )
            .ok()
        });
        if mapping_plan.as_ref().is_some_and(|plan| {
            matches!(
                plan.disposition,
                crate::application::model_mapping::Disposition::Reject
            )
        }) {
            return Ok(RouteSimulationResult {
                preview_policy_version: "intelligent_planner_v1".to_string(),
                capacity_mode: "snapshot_only".to_string(),
                selected_capacity_acquired: false,
                selected_station_key_id: None,
                selected_station_id: None,
                mapped_model: None,
                policy,
                max_rate_multiplier,
                routing_group_filter,
                planner_error_code: Some("model_mapping_rejected".to_string()),
                candidates: Vec::new(),
                message: "Route simulation rejected request: model_mapping_rejected".to_string(),
            });
        }
        let mapped_model = match mapping_plan.as_ref() {
            Some(plan) => match plan.execution_target() {
                Ok(target) => target.map(|target| target.route_model.clone()),
                Err(error) => {
                    return Ok(RouteSimulationResult {
                        preview_policy_version: "intelligent_planner_v1".to_string(),
                        capacity_mode: "snapshot_only".to_string(),
                        selected_capacity_acquired: false,
                        selected_station_key_id: None,
                        selected_station_id: None,
                        mapped_model: None,
                        policy,
                        max_rate_multiplier,
                        routing_group_filter,
                        planner_error_code: Some(error.code().to_string()),
                        candidates: Vec::new(),
                        message: "Route simulation cannot execute the model mapping plan."
                            .to_string(),
                    });
                }
            },
            None => None,
        };
        let validated_settings = validated_route_settings(&RuntimeRoutingSettings {
            policy: policy.clone(),
            max_rate_multiplier,
            routing_group_scope: routing_group_filter.clone(),
            scheduler_config: settings.scheduler_config.clone(),
            allow_depleted_fallback: settings.allow_depleted_fallback,
            outbound_proxy_mode: settings.outbound_proxy_mode.clone(),
            outbound_proxy_url: settings.outbound_proxy_url.clone(),
            global_proxy_mode: settings.global_proxy_mode.clone(),
            global_proxy_url: settings.global_proxy_url.clone(),
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
                planning_context,
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
                hard_rejection_codes: if candidate.hard_eligible {
                    Vec::new()
                } else {
                    vec!["hard_ineligible".to_string()]
                },
            })
            .collect::<Vec<_>>();
        let canonical_plan =
            plan_snapshot_with_budget(&planning_snapshot, b"simulation", 1, None).ok();
        let explanations = canonical_plan
            .as_ref()
            .map(|plan| {
                canonical_simulation_explanations(
                    &projected,
                    plan,
                    mapped_model.clone(),
                    routing_group_filter.clone(),
                )
            })
            .unwrap_or_else(|| {
                projected
                    .iter()
                    .map(|candidate| {
                        canonical_rejected_explanation(
                            candidate,
                            mapped_model.clone(),
                            routing_group_filter.clone(),
                        )
                    })
                    .collect()
            });
        let selected_station_key_id = canonical_plan
            .as_ref()
            .map(|plan| plan.selected_station_key_id.clone());
        let selected_station_id = selected_station_key_id
            .as_deref()
            .and_then(|station_key_id| {
                projected
                    .iter()
                    .find(|candidate| candidate.station_key_id == station_key_id)
                    .map(|candidate| candidate.station_id.clone())
            });
        let planner_error_code = if selected_station_key_id.is_none() {
            Some("no_eligible_candidate".to_string())
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

fn routing_document_coordinator(runtime: &PersistenceHandle) -> PolicyDocumentCoordinator {
    let root = runtime
        .database_path()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    PolicyDocumentCoordinator::shared(root)
}

fn routing_document_from_stored(
    stored: &crate::persistence::stores::routing_policy_store::StoredRoutingPolicy,
) -> Result<crate::models::routing_policy::RoutingPolicyDocumentV2, PersistenceError> {
    let policy =
        crate::models::routing_policy::RoutingPolicyConfigV2::from_stored_value(&stored.config)
            .map_err(|error| {
                PersistenceError::InvariantViolation(format!(
                    "routing policy config is invalid: {error:?}"
                ))
            })?;
    let revision = u64::try_from(stored.revision).map_err(|_| {
        PersistenceError::InvariantViolation("routing policy revision is invalid".into())
    })?;
    Ok(crate::models::routing_policy::RoutingPolicyDocumentV2 {
        format_version: crate::models::routing_policy::ROUTING_POLICY_DOCUMENT_FORMAT_VERSION,
        base_revision: revision,
        policy,
    })
}

fn routing_document_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

async fn mark_routing_sync_error(
    runtime: &PersistenceHandle,
    code: &str,
) -> Result<(), PersistenceError> {
    let mut write = runtime.begin_write().await?;
    crate::persistence::stores::document_sync_store::DocumentSyncStore
        .mark_error(
            write.connection(),
            crate::models::document_sync::ROUTING_POLICY_DOCUMENT_KIND,
            code,
            chrono::Utc::now().timestamp_millis().max(0),
        )
        .await?;
    write.commit().await
}

/// Decode historical and active managed-document shapes at the file
/// boundary, then normalize the result to the active V2 document shape.
fn decode_routing_document(
    bytes: &[u8],
) -> Result<crate::models::routing_policy::RoutingPolicyDocumentV2, PolicyDocumentError> {
    let value = decode_strict_json::<serde_json::Value>(bytes)?;
    let policy_version = value
        .get("policy")
        .and_then(|policy| policy.get("version"))
        .and_then(serde_json::Value::as_u64);
    match policy_version {
        Some(2) => {
            let document = serde_json::from_value::<
                crate::models::routing_policy::RoutingPolicyDocumentV2,
            >(value)
            .map_err(|error| PolicyDocumentError::InvalidJson(error.to_string()))?;
            document
                .validate()
                .map_err(|error| PolicyDocumentError::InvalidJson(format!("{error:?}")))?;
            Ok(document)
        }
        Some(1) => {
            let legacy = serde_json::from_value::<
                crate::models::routing_policy::RoutingPolicyDocumentV1,
            >(value)
            .map_err(|error| PolicyDocumentError::InvalidJson(error.to_string()))?;
            crate::models::routing_policy::RoutingPolicyDocumentV2::from_v1(&legacy)
                .map_err(|error| PolicyDocumentError::InvalidJson(format!("{error:?}")))
        }
        Some(version) => Err(PolicyDocumentError::InvalidJson(format!(
            "unsupported routing policy version {version}"
        ))),
        None => Err(PolicyDocumentError::InvalidJson(
            "routing policy version is missing".into(),
        )),
    }
}

/// Publish the latest active policy as the desired target and materialize it
/// through the shared coordinator. Existing external bytes are preserved only
/// when `replace_existing` is false (startup/recovery); an explicit UI save
/// owns the new revision and may replace the mirror atomically.
pub(crate) async fn sync_routing_policy_file(
    runtime: PersistenceHandle,
    stored: &crate::persistence::stores::routing_policy_store::StoredRoutingPolicy,
    replace_existing: bool,
) -> Result<(), PersistenceError> {
    let document = routing_document_from_stored(stored)?;
    document.validate().map_err(|error| {
        PersistenceError::InvariantViolation(format!("routing policy document rejected: {error:?}"))
    })?;
    let canonical = canonical_json(&document)
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    let digest = routing_document_digest(&canonical);
    let kind = crate::models::document_sync::ROUTING_POLICY_DOCUMENT_KIND;
    let coordinator = routing_document_coordinator(&runtime);
    let _operation_guard = coordinator.acquire_operation_guard().await;
    let revision_i64 = i64::try_from(document.base_revision).map_err(|_| {
        PersistenceError::InvariantViolation("routing policy revision exceeds SQLite range".into())
    })?;
    let current_revision: i64 = {
        let mut read = runtime.begin_read().await?;
        sqlx::query_scalar("SELECT config_revision FROM routing_policy WHERE singleton_key = 1")
            .fetch_one(read.connection())
            .await?
    };
    if current_revision != revision_i64 {
        return Ok(());
    }
    {
        let mut write = runtime.begin_write().await?;
        crate::persistence::stores::document_sync_store::DocumentSyncStore
            .upsert_desired(
                write.connection(),
                kind,
                document.base_revision,
                Some(&digest),
                chrono::Utc::now().timestamp_millis().max(0),
            )
            .await?;
        write.commit().await?;
    }
    let previous_materialized_digest = {
        let mut read = runtime.begin_read().await?;
        crate::persistence::stores::document_sync_store::DocumentSyncStore
            .load(read.connection(), kind)
            .await?
            .and_then(|sync| sync.materialized_canonical_digest)
    };

    let existing = coordinator
        .files()
        .read_once(crate::models::document_sync::DocumentKind::RoutingPolicy);
    let should_materialize = match existing {
        Err(PolicyDocumentError::Missing) => true,
        Ok(observed)
            if replace_existing
                || observed.digest == digest
                || previous_materialized_digest.as_deref() == Some(observed.digest.as_str()) =>
        {
            true
        }
        Ok(observed) => match decode_routing_document(&observed.bytes) {
            Ok(incoming) => {
                let incoming_canonical = canonical_json(&incoming)
                    .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
                if incoming_canonical == canonical {
                    true
                } else {
                    let mut write = runtime.begin_write().await?;
                    crate::persistence::stores::document_sync_store::DocumentSyncStore
                        .mark_external_change(
                            write.connection(),
                            kind,
                            Some(&observed.digest),
                            Some("external_change"),
                            chrono::Utc::now().timestamp_millis().max(0),
                        )
                        .await?;
                    write.commit().await?;
                    false
                }
            }
            Err(_) => {
                mark_routing_sync_error(&runtime, "invalid_document").await?;
                false
            }
        },
        Err(_) => {
            mark_routing_sync_error(&runtime, "document_unavailable").await?;
            false
        }
    };
    if !should_materialize {
        return Ok(());
    }
    let coordinator_result = coordinator.files().materialize(
        crate::models::document_sync::DocumentKind::RoutingPolicy,
        &canonical,
    );
    if coordinator_result.is_err() {
        mark_routing_sync_error(&runtime, "materialization_failed").await?;
        return Ok(());
    }
    // SQLite CAS and filesystem replacement are separate resources. A newer
    // process may commit while this operation is publishing the old target;
    // re-read the active fence and immediately materialize the current policy
    // so a stale mirror does not survive until the periodic reconciliation.
    let latest = {
        let mut read = runtime.begin_read().await?;
        RoutingPolicyStore
            .load(read.connection())
            .await?
            .ok_or(PersistenceError::NotFound)?
    };
    if latest.revision != document.base_revision {
        drop(_operation_guard);
        Box::pin(sync_routing_policy_file(runtime, &latest, true)).await?;
        return Ok(());
    }
    let mut write = runtime.begin_write().await?;
    crate::persistence::stores::document_sync_store::DocumentSyncStore
        .mark_materialized(
            write.connection(),
            kind,
            document.base_revision,
            Some(&digest),
            chrono::Utc::now().timestamp_millis().max(0),
        )
        .await?;
    write.commit().await
}

/// Startup reconciliation for routing policy. It creates a missing managed
/// mirror and records an external change, but never lets a file silently
/// replace the active SQLite aggregate during startup.
pub(crate) async fn initialize_routing_policy_document_sync(
    runtime: PersistenceHandle,
) -> Result<(), PersistenceError> {
    let stored = {
        let mut read = runtime.begin_read().await?;
        RoutingPolicyStore
            .load(read.connection())
            .await?
            .ok_or(PersistenceError::NotFound)?
    };
    sync_routing_policy_file(runtime, &stored, false).await
}

/// Import a stable externally edited routing-policy document through the same
/// aggregate CAS used by the UI. This adapter is intentionally separate from
/// read-model reconciliation so querying workspace state cannot mutate the
/// active policy. Invalid JSON, invalid policy values, or a stale base
/// revision only update document-sync diagnostics and leave SQLite untouched.
pub(crate) async fn reconcile_external_routing_policy_document(
    runtime: PersistenceHandle,
    service: &RoutingService,
) -> Result<
    Option<crate::persistence::stores::routing_policy_store::StoredRoutingPolicy>,
    PersistenceError,
> {
    use crate::models::document_sync::{DocumentKind, ROUTING_POLICY_DOCUMENT_KIND};

    let (current_sync, current_policy) = {
        let mut read = runtime.begin_read().await?;
        let sync = crate::persistence::stores::document_sync_store::DocumentSyncStore
            .load(read.connection(), ROUTING_POLICY_DOCUMENT_KIND)
            .await?;
        let policy = RoutingPolicyStore
            .load(read.connection())
            .await?
            .ok_or(PersistenceError::NotFound)?;
        (sync, policy)
    };
    let Some(current_sync) = current_sync else {
        return Ok(None);
    };
    let current_revision = u64::try_from(current_policy.revision).map_err(|_| {
        PersistenceError::InvariantViolation("routing policy revision is invalid".into())
    })?;
    let coordinator = routing_document_coordinator(&runtime);
    let stable = match coordinator.read_stable(DocumentKind::RoutingPolicy).await {
        Ok(stable) => stable,
        Err(PolicyDocumentError::Missing) => {
            mark_routing_sync_error(&runtime, "document_missing").await?;
            return Ok(None);
        }
        Err(PolicyDocumentError::Unstable) => return Ok(None),
        Err(_) => {
            mark_routing_sync_error(&runtime, "document_unavailable").await?;
            return Ok(None);
        }
    };
    if current_sync
        .desired_canonical_digest
        .as_deref()
        .is_some_and(|digest| digest == stable.digest)
    {
        let mut write = runtime.begin_write().await?;
        let _ = crate::persistence::stores::document_sync_store::DocumentSyncStore
            .mark_materialized(
                write.connection(),
                ROUTING_POLICY_DOCUMENT_KIND,
                current_sync.desired_revision,
                Some(&stable.digest),
                chrono::Utc::now().timestamp_millis().max(0),
            )
            .await?;
        write.commit().await?;
        return Ok(None);
    }
    let document = match decode_routing_document(&stable.bytes) {
        Ok(document) => document,
        Err(_) => {
            mark_routing_sync_error(&runtime, "invalid_document").await?;
            return Ok(None);
        }
    };
    if document.base_revision != current_revision
        || document.validate().is_err()
        || crate::application::routing_policy::compile_config_v2(
            &document.policy,
            document.base_revision,
            &current_policy.policy_version,
            &current_policy.system_version,
        )
        .is_err()
    {
        let mut write = runtime.begin_write().await?;
        let _ = crate::persistence::stores::document_sync_store::DocumentSyncStore
            .mark_external_change(
                write.connection(),
                ROUTING_POLICY_DOCUMENT_KIND,
                Some(&stable.digest),
                Some("revision_conflict"),
                chrono::Utc::now().timestamp_millis().max(0),
            )
            .await?;
        write.commit().await?;
        return Ok(None);
    }
    let stored = match service
        .apply_routing_policy_document_v2(document, TrustedDocumentSource::file_watch())
        .await
    {
        Ok(stored) => Some(stored),
        Err(ApplicationError::StaleRevision) => {
            let mut mark = runtime.begin_write().await?;
            let _ = crate::persistence::stores::document_sync_store::DocumentSyncStore
                .mark_external_change(
                    mark.connection(),
                    ROUTING_POLICY_DOCUMENT_KIND,
                    Some(&stable.digest),
                    Some("revision_conflict"),
                    chrono::Utc::now().timestamp_millis().max(0),
                )
                .await?;
            mark.commit().await?;
            None
        }
        Err(error) => {
            return Err(PersistenceError::InvariantViolation(error.to_string()));
        }
    };
    Ok(stored)
}

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

fn mapping_endpoint_kind(
    endpoint: &RouteEndpointKind,
) -> crate::models::model_mapping::EndpointKind {
    match endpoint {
        RouteEndpointKind::Models => crate::models::model_mapping::EndpointKind::Models,
        RouteEndpointKind::ChatCompletions => {
            crate::models::model_mapping::EndpointKind::ChatCompletions
        }
        RouteEndpointKind::Responses => crate::models::model_mapping::EndpointKind::Responses,
        RouteEndpointKind::Embeddings => crate::models::model_mapping::EndpointKind::Embeddings,
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
            route_explanation_from_canonical_candidate(
                candidate,
                mapped_model.clone(),
                routing_group_scope.clone(),
                accepted,
                reasons,
                rejection_reasons,
                rank,
            )
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
fn simulation_explanations(
    candidates: &[SimulatedRouteCandidateProjection],
    plan: &crate::application::routing_engine::candidate_plan::RoutePlan,
    mapped_model: Option<String>,
    routing_group_scope: RoutingGroupFilter,
) -> Vec<RouteCandidateExplanation> {
    let ordered =
        crate::application::routing_engine::hierarchical_preview::ordered_plan_candidates(plan);
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
    use std::time::{Duration, Instant};

    use super::*;
    use crate::application::operational_facts::{
        candidate_projection::route_projection_from_runtime_candidate,
        pricing_projector::RoutingCostBasis,
    };
    use crate::application::routing_engine::hierarchical_preview::{plan_route, PlanningInput};
    use crate::application::routing_engine::request::{
        PlanningRequestContext, PlanningRoundContext, RouteProgress,
    };
    use crate::models::{
        pricing::{PricingStatus, RequestKind},
        proxy::UpstreamApiFormat,
        routing::{RoutingPolicy, StationKeyCapabilities},
        routing_policy::{RoutingPolicyConfigV1, RoutingPolicyDocumentV2},
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

    #[tokio::test]
    async fn planning_snapshot_rejects_an_expired_caller_deadline() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = crate::persistence::runtime::PersistenceRuntime::initialize_new(
            &temp.path().join("routing.sqlite3"),
        )
        .await
        .expect("runtime");
        let service = RoutingService::new(runtime.handle());
        let settings = RuntimeRoutingSettings::default();
        let request =
            route_request_facts_for_read_model(&settings, chrono::Utc::now().timestamp_millis());

        let result = service
            .load_intelligent_planning_snapshot(
                &request,
                RuntimeOverlaySnapshot {
                    runtime_instance_id: "expired-context".to_string(),
                    runtime_revision: 1,
                    candidate_set_revision: 1,
                    in_flight: 0,
                    max_concurrency: 1,
                    affinity_station_key_id: None,
                },
                PlanningRequestContext::from_deadline(Instant::now() - Duration::from_millis(1)),
            )
            .await;

        assert!(matches!(result, Err(ApplicationError::DeadlineExceeded)));
        runtime.close().await.expect("close persistence runtime");
    }

    #[tokio::test]
    async fn stale_routing_materialization_cannot_regress_newer_active_revision() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = crate::persistence::runtime::PersistenceRuntime::initialize_new(
            &temp.path().join("routing.sqlite3"),
        )
        .await
        .expect("runtime");
        let service = RoutingService::new(runtime.handle());
        let baseline = service.load_routing_policy().await.expect("baseline");

        let mut first_config = RoutingPolicyConfigV1::default();
        first_config.max_candidates = 63;
        let first = service
            .apply_routing_policy_document_v2(
                RoutingPolicyDocumentV2 {
                    format_version:
                        crate::models::routing_policy::ROUTING_POLICY_DOCUMENT_FORMAT_VERSION,
                    base_revision: baseline.revision,
                    policy: first_config.into(),
                },
                TrustedDocumentSource::ui(),
            )
            .await
            .expect("first policy apply");

        let mut second_config = RoutingPolicyConfigV1::default();
        second_config.max_candidates = 62;
        let second = service
            .apply_routing_policy_document_v2(
                RoutingPolicyDocumentV2 {
                    format_version:
                        crate::models::routing_policy::ROUTING_POLICY_DOCUMENT_FORMAT_VERSION,
                    base_revision: first.revision,
                    policy: second_config.into(),
                },
                TrustedDocumentSource::ui(),
            )
            .await
            .expect("second policy apply");

        // Simulate a delayed file-side continuation from the first commit.
        sync_routing_policy_file(runtime.handle(), &first, true)
            .await
            .expect("stale materialization is ignored");
        let bytes = std::fs::read(temp.path().join("config").join("routing-policy.json"))
            .expect("managed routing document");
        let materialized: RoutingPolicyDocumentV2 =
            serde_json::from_slice(&bytes).expect("managed routing document decodes");
        assert_eq!(materialized.base_revision, second.revision);
        assert_eq!(materialized.policy.max_candidates, 62);
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
            ..Default::default()
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
    fn local_workspace_candidate_uses_token_pricing_projection() {
        let settings = RuntimeRoutingSettings {
            policy: RoutingPolicy::CostStableFirst,
            max_rate_multiplier: Some(2.0),
            routing_group_scope: RoutingGroupFilter::AllGroups,
            scheduler_config: Default::default(),
            allow_depleted_fallback: false,
            ..Default::default()
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
            base_input_price: Some(0.42),
            base_output_price: None,
            base_cache_creation_price: None,
            base_cache_read_price: None,
            currency: "USD".to_string(),
            unit: "per_1m_tokens".to_string(),
            base_price_source: Some("fixture".to_string()),
            effective_rate_multiplier: Some(1.0),
            rate_source: Some("fixture".to_string()),
            rate_collected_at: Some("2026-07-31T00:00:00Z".to_string()),
            estimated_input_price: Some(0.42),
            estimated_output_price: None,
            estimated_cache_creation_price: None,
            estimated_cache_read_price: None,
            pricing_status: PricingStatus::Priced,
            confidence: 0.99,
            source_chain: vec!["model_base_price".to_string()],
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
        assert_eq!(projection.pricing.currency.as_deref(), Some("USD"));
        assert_eq!(
            projection.pricing.source_chain,
            vec!["model_base_price".to_string()]
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
            capacity_provider_family: None,
            capacity_deployment_identity: None,
            capacity_region_identity: None,
            capacity_domain_revision: None,
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
