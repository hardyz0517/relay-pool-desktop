use std::{collections::BTreeMap, sync::Arc, time::Duration};

use sha2::{Digest, Sha256};

/// Bounded budget for top-level local read-model operations that invoke the
/// planner without an ingress request. Proxy traffic supplies its own
/// ingress-owned absolute deadline instead.
const NON_PROXY_PLANNING_DEADLINE: Duration = Duration::from_secs(5);

fn planner_error_code_for(error: &ApplicationError) -> String {
    match error {
        ApplicationError::DeadlineExceeded => "deadline".to_string(),
        ApplicationError::ConstraintViolation => "planner_constraint_violation".to_string(),
        ApplicationError::StaleRevision => "planner_stale_revision".to_string(),
        ApplicationError::Unavailable => "planner_fact_unavailable".to_string(),
        ApplicationError::NotFound => "planner_policy_missing".to_string(),
        ApplicationError::IncompatibleSchema => "planner_schema_incompatible".to_string(),
        ApplicationError::Internal => "planner_internal".to_string(),
        _ => "planner_unavailable".to_string(),
    }
}

fn circuit_persistence_failure(error: &PersistenceError) -> bool {
    matches!(
        error,
        PersistenceError::RuntimeUnavailable
            | PersistenceError::SessionClosed
            | PersistenceError::CommitOutcomeUnknown
            | PersistenceError::IoFailed { .. }
            | PersistenceError::DatabaseFailed
            | PersistenceError::DatabaseBusy
    )
}

use crate::{
    application::routing_engine::{
        algorithm_profile::DispatchAlgorithmProfile,
        candidate_plan::RoutePlanPricingSnapshot,
        intelligent_planner::{candidate_score_breakdown_with_cost_basis, plan_snapshot},
        planning_snapshot::{PlanningSnapshot, RuntimeOverlaySnapshot},
        request::{
            CanonicalRouteRequest, PlanningRequestContext, RouteKind, RouteRequestClassifier,
        },
    },
    application::{
        credentials::SecretRef,
        error::ApplicationError,
        operational_facts::{
            candidate_projection::{route_request_facts_for_read_model, validated_route_settings},
            planning_snapshot::{PlanningBuildResult, PlanningSnapshotBuilder},
            pricing_projector::{
                pricing_context_from_resolution, request_cost_comparison_context, PricingRouteKind,
            },
            target_resolver::ExecutionTargetRef,
        },
        queries::{
            routing_protection::{
                project_routing_protection_status_from_circuit, CapacityProtectionFact,
                RoutingProtectionStatus,
            },
            routing_runtime::{
                monitoring_target_snapshots_from_facts, runtime_overlay_from_candidates,
                RoutingMonitoringTargetFacts, RoutingMonitoringTargetSnapshot,
                RoutingRuntimeActivity, RoutingRuntimeCandidateFact, RoutingRuntimeOverlay,
            },
            routing_workspace::{
                workspace_snapshot_from_canonical_candidates, RoutingCandidatePlanDiagnostics,
                RoutingPlannerEvaluationStatus, RoutingScoreStatus,
                RoutingWorkspaceRevisionSnapshot, RoutingWorkspaceSnapshot,
                RoutingWorkspaceSnapshotInput,
            },
            station_key_circuit_read::StationKeyCircuitReadSnapshot,
        },
        routing_policy::RoutingPolicyAggregate,
        station_key_circuit::CircuitPersistenceGate,
    },
    models::{
        document_sync::TrustedDocumentSource,
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
        stores::routing_quality_store::RoutingQualityStore,
        stores::routing_store::{RoutingStore, StationEndpointProbeTarget},
    },
    services::policy_documents::{
        canonical_json, decode_strict_json, PolicyDocumentCoordinator, PolicyDocumentError,
    },
};

#[cfg(test)]
use crate::application::operational_facts::candidate_projection::route_projection_from_runtime_candidate_with_pricing;

#[derive(Debug, Clone)]
pub struct CanonicalRoutingCandidateWithPricing {
    pub(crate) candidate: CanonicalRoutingCandidate,
    pub(crate) pricing_context: Option<ResolvedPricingContext>,
}
#[derive(Clone)]
pub(crate) struct RoutingService {
    runtime: PersistenceHandle,
    store: RoutingStore,
    circuit_persistence_gate: Arc<CircuitPersistenceGate>,
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

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=routing-service-compat-constructor; owner=application/routing; remove_when=all compositions inject the shared circuit persistence gate"
        )
    )]
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self::new_with_circuit_persistence_gate(runtime, CircuitPersistenceGate::shared())
    }

    pub(crate) fn new_with_circuit_persistence_gate(
        runtime: PersistenceHandle,
        circuit_persistence_gate: Arc<CircuitPersistenceGate>,
    ) -> Self {
        Self {
            runtime,
            store: RoutingStore,
            circuit_persistence_gate,
        }
    }

    fn circuit_persistence_gate_active(
        &self,
        station_key_id: &str,
        lifecycle_revision: u64,
    ) -> bool {
        self.circuit_persistence_gate
            .is_active(station_key_id, lifecycle_revision)
    }

    fn mark_circuit_persistence_gate(&self, station_key_id: &str, lifecycle_revision: u64) {
        self.circuit_persistence_gate
            .mark_station_key(station_key_id, lifecycle_revision);
    }

    async fn persist_circuit_persistence_gate(
        &self,
        station_key_id: String,
        lifecycle_revision: u64,
        now_ms: u64,
    ) {
        let store = crate::persistence::stores::station_key_circuit_store::StationKeyCircuitStore;
        let _ = self
            .runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .mark_persistence_unavailable(
                            write.connection(),
                            &station_key_id,
                            lifecycle_revision,
                            now_ms,
                        )
                        .await
                })
            })
            .await;
    }

    /// Ordinary request traffic can open this gate but cannot clear it.
    pub(crate) async fn health_check_station_key_circuit_persistence(
        &self,
        now_ms: u64,
    ) -> Result<u64, ApplicationError> {
        let expected_gate_revision = self.circuit_persistence_gate.revision();
        let store = crate::persistence::stores::station_key_circuit_store::StationKeyCircuitStore;
        let cleared = self
            .runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .health_check_and_clear_persistence_gates(write.connection(), now_ms)
                        .await
                })
            })
            .await
            .map_err(ApplicationError::from)?;
        if !self
            .circuit_persistence_gate
            .clear_if_unchanged(expected_gate_revision)
        {
            return Err(ApplicationError::Unavailable);
        }
        Ok(cleared)
    }

    pub(crate) async fn load_routing_policy(
        &self,
    ) -> Result<
        crate::persistence::stores::routing_policy_store::StoredRoutingPolicy,
        ApplicationError,
    > {
        crate::persistence::stores::routing_policy_v3_stage_upgrade::load_effective_active(
            &self.runtime,
        )
        .await
        .map_err(ApplicationError::from)?
        .ok_or(ApplicationError::NotFound)
    }

    pub(crate) async fn get_routing_protection_status(
        &self,
        generated_at_ms: i64,
        capacity: &[CapacityProtectionFact],
        runtime_capacity_available: bool,
    ) -> Result<RoutingProtectionStatus, ApplicationError> {
        let circuit = self
            .load_station_key_circuit_read_snapshot(generated_at_ms)
            .await?;
        Ok(project_routing_protection_status_from_circuit(
            &circuit,
            capacity,
            runtime_capacity_available,
        ))
    }

    /// Atomically commits the durable Key-circuit admission and the v3
    /// attempt slot. The caller may cross the outbound boundary only after
    /// this transaction returns an allowed result.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn admit_station_key_circuit_with_attempt(
        &self,
        expected_runtime_generation_id: Option<String>,
        expected_fence_revision: u64,
        station_key_id: String,
        lifecycle_revision: u64,
        policy_revision: u64,
        now_ms: u64,
        deadline_at_ms: u64,
        score_gate_passed: bool,
        attempt_id: String,
        correlation_id: String,
        attempt_index: u16,
        capacity_lease_id: String,
        consecutive_failure_threshold: u16,
        recovery_success_threshold: u16,
        recovery_wait_ms: u64,
    ) -> Result<crate::application::station_key_circuit::CircuitAdmissionResult, ApplicationError>
    {
        use crate::application::station_key_circuit::CircuitAdmissionResult;
        use crate::persistence::stores::routing_attempt_store::{
            RoutingAttemptAdmission, RoutingAttemptStore,
        };

        if self.circuit_persistence_gate_active(&station_key_id, lifecycle_revision) {
            return Ok(CircuitAdmissionResult::DeniedPersistenceUnavailable);
        }
        let circuit = crate::persistence::stores::station_key_circuit_store::StationKeyCircuitStore;
        let gate_station_key_id = station_key_id.clone();
        let outcome = self.runtime
            .write(|write| {
                Box::pin(async move {
                    if circuit
                        .persistence_gate_active(
                            write.connection(),
                            &station_key_id,
                            lifecycle_revision,
                        )
                        .await?
                    {
                        return Ok(CircuitAdmissionResult::DeniedPersistenceUnavailable);
                    }
                    let generation_guard = crate::persistence::stores::routing_generation_store::RoutingGenerationStore
                        .load_admission_guard(write.connection())
                        .await?;
                    if generation_guard.fencing {
                        return Ok(CircuitAdmissionResult::DeniedGenerationFence);
                    }
                    if generation_guard.active_runtime_generation_id
                        != expected_runtime_generation_id
                        || generation_guard.fence_revision != expected_fence_revision
                    {
                        return Ok(CircuitAdmissionResult::DeniedStaleGeneration);
                    }
                    let generation_eligibility =
                        RoutingAttemptStore::resolve_generation_eligibility(write.connection())
                            .await?;
                    let attempt_admission = RoutingAttemptAdmission {
                        attempt_id: &attempt_id,
                        correlation_id: &correlation_id,
                        station_key_id: &station_key_id,
                        station_key_lifecycle_revision: lifecycle_revision,
                        attempt_index,
                        capacity_lease_id: &capacity_lease_id,
                        half_open_lease_id: None,
                        lease_revision: None,
                        deadline_at_ms,
                        admitted_at_ms: now_ms,
                        generation_eligibility,
                    };
                    if RoutingAttemptStore::audit_late_admission_if_finalized(
                        write.connection(),
                        &attempt_admission,
                    )
                    .await?
                    {
                        return Ok(CircuitAdmissionResult::DeniedLateAfterFinalization);
                    }
                    let result = circuit
                        .admit(
                            write.connection(),
                            &station_key_id,
                            lifecycle_revision,
                            policy_revision,
                            now_ms,
                            deadline_at_ms,
                            score_gate_passed,
                            &attempt_id,
                            consecutive_failure_threshold,
                            recovery_success_threshold,
                            recovery_wait_ms,
                        )
                        .await?;
                    let (half_open_lease_id, lease_revision) = match result {
                        CircuitAdmissionResult::AllowedHalfOpen { lease_revision, .. } => {
                            (Some(attempt_id.as_str()), Some(lease_revision))
                        }
                        CircuitAdmissionResult::AllowedClosed { .. } => (None, None),
                        _ => return Ok(result),
                    };
                    RoutingAttemptStore::admit(
                        write.connection(),
                        &RoutingAttemptAdmission {
                            attempt_id: &attempt_id,
                            correlation_id: &correlation_id,
                            station_key_id: &station_key_id,
                            station_key_lifecycle_revision: lifecycle_revision,
                            attempt_index,
                            capacity_lease_id: &capacity_lease_id,
                            half_open_lease_id,
                            lease_revision,
                            deadline_at_ms,
                            admitted_at_ms: now_ms,
                            generation_eligibility,
                        },
                    )
                    .await?;
                    Ok(result)
                })
            })
            .await;
        match outcome {
            Ok(result) => Ok(result),
            Err(error) if circuit_persistence_failure(&error) => {
                self.mark_circuit_persistence_gate(&gate_station_key_id, lifecycle_revision);
                self.persist_circuit_persistence_gate(
                    gate_station_key_id,
                    lifecycle_revision,
                    now_ms,
                )
                .await;
                Ok(CircuitAdmissionResult::DeniedPersistenceUnavailable)
            }
            Err(error) => Err(ApplicationError::from(error)),
        }
    }

    pub(crate) async fn load_station_key_circuit_statuses(
        &self,
    ) -> Result<
        Vec<crate::application::station_key_circuit::StationKeyCircuitStatus>,
        ApplicationError,
    > {
        use crate::persistence::stores::station_key_circuit_store::{
            StationKeyCircuitStore, SHARED_CIRCUIT_PERSISTENCE_GATE_KEY,
            SHARED_CIRCUIT_PERSISTENCE_GATE_REVISION,
        };

        if self.circuit_persistence_gate.is_active(
            SHARED_CIRCUIT_PERSISTENCE_GATE_KEY,
            SHARED_CIRCUIT_PERSISTENCE_GATE_REVISION,
        ) {
            return Err(ApplicationError::Unavailable);
        }
        let result = async {
            let mut read = self.runtime.begin_read().await?;
            StationKeyCircuitStore
                .list_statuses(read.connection())
                .await
                .map_err(ApplicationError::from)
        }
        .await;
        match result {
            Ok(statuses) => Ok(statuses),
            Err(error) => {
                self.circuit_persistence_gate.mark_global_unavailable();
                self.persist_circuit_persistence_gate(
                    SHARED_CIRCUIT_PERSISTENCE_GATE_KEY.to_string(),
                    SHARED_CIRCUIT_PERSISTENCE_GATE_REVISION,
                    chrono::Utc::now().timestamp_millis().max(0) as u64,
                )
                .await;
                Err(error)
            }
        }
    }

    /// Versioned read-only circuit snapshot for non-Proxy consumers. A gate
    /// revision change retries the whole durable read once; a second change
    /// is reported as unavailable instead of returning a torn snapshot.
    pub(crate) async fn load_station_key_circuit_read_snapshot(
        &self,
        generated_at_ms: i64,
    ) -> Result<StationKeyCircuitReadSnapshot, ApplicationError> {
        for attempt in 0..2 {
            let process_before = self.circuit_persistence_gate.snapshot();
            let mut read = self.runtime.begin_read().await?;
            let durable =
                crate::persistence::stores::station_key_circuit_store::StationKeyCircuitStore
                    .load_read_snapshot(read.connection())
                    .await
                    .map_err(ApplicationError::from)?;
            let process_after = self.circuit_persistence_gate.snapshot();
            if process_before.revision == process_after.revision {
                return Ok(StationKeyCircuitReadSnapshot::project(
                    generated_at_ms,
                    process_after,
                    durable,
                ));
            }
            if attempt == 1 {
                return Err(ApplicationError::Unavailable);
            }
        }
        Err(ApplicationError::Unavailable)
    }

    pub(crate) async fn load_routing_generation_admission_guard(
        &self,
    ) -> Result<crate::models::routing_generation::RoutingGenerationAdmissionGuard, ApplicationError>
    {
        let mut read = self.runtime.begin_read().await?;
        crate::persistence::stores::routing_generation_store::RoutingGenerationStore
            .load_admission_guard(read.connection())
            .await
            .map_err(ApplicationError::from)
    }

    /// Marks every admitted attempt at the durable outbound boundary. Closed
    /// attempts update only the ledger; Half-Open attempts additionally CAS
    /// the circuit lease in the same SQLite transaction.
    pub(crate) async fn mark_station_key_attempt_boundary(
        &self,
        station_key_id: String,
        lifecycle_revision: u64,
        attempt_id: String,
        lease_revision: Option<u64>,
        now_ms: u64,
    ) -> Result<bool, ApplicationError> {
        use crate::persistence::stores::routing_attempt_store::RoutingAttemptStore;

        let circuit = crate::persistence::stores::station_key_circuit_store::StationKeyCircuitStore;
        let gate_station_key_id = station_key_id.clone();
        let outcome = self
            .runtime
            .write(|write| {
                Box::pin(async move {
                    let ledger_marked = RoutingAttemptStore::mark_boundary_crossed(
                        write.connection(),
                        &attempt_id,
                        &station_key_id,
                        lifecycle_revision,
                        now_ms,
                    )
                    .await?;
                    if !ledger_marked {
                        return Ok(false);
                    }
                    let Some(lease_revision) = lease_revision else {
                        return Ok(true);
                    };
                    let circuit_marked = circuit
                        .mark_boundary_crossed(
                            write.connection(),
                            &station_key_id,
                            lifecycle_revision,
                            &attempt_id,
                            lease_revision,
                            now_ms,
                        )
                        .await?;
                    if !circuit_marked {
                        return Err(
                            crate::persistence::error::PersistenceError::RevisionConflict(
                                "station_key_attempt_boundary".to_string(),
                            ),
                        );
                    }
                    Ok(true)
                })
            })
            .await;
        match outcome {
            Ok(marked) => Ok(marked),
            Err(error) if circuit_persistence_failure(&error) => {
                self.mark_circuit_persistence_gate(&gate_station_key_id, lifecycle_revision);
                self.persist_circuit_persistence_gate(
                    gate_station_key_id,
                    lifecycle_revision,
                    now_ms,
                )
                .await;
                Err(ApplicationError::Unavailable)
            }
            Err(error) => Err(ApplicationError::from(error)),
        }
    }

    pub(crate) async fn apply_routing_policy_document_v3(
        &self,
        document: crate::models::routing_policy::RoutingPolicyDocumentV3,
        source: TrustedDocumentSource,
    ) -> Result<
        crate::persistence::stores::routing_policy_store::StoredRoutingPolicy,
        ApplicationError,
    > {
        document
            .validate()
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        crate::persistence::stores::routing_policy_v3_stage_upgrade::stage_user_policy(
            &self.runtime,
            document.base_revision,
            &document.policy,
            source.history_label(),
            now_ms,
        )
        .await
        .map_err(ApplicationError::from)
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
        tokio::time::timeout_at(
            tokio::time::Instant::from_std(context.deadline()),
            self.load_intelligent_planning_snapshot_within_deadline(request, runtime),
        )
        .await
        .map_err(|_| ApplicationError::DeadlineExceeded)?
    }

    async fn load_intelligent_planning_snapshot_within_deadline(
        &self,
        request: &crate::application::routing_engine::request::RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
    ) -> Result<Option<PlanningSnapshot>, ApplicationError> {
        Ok(self
            .load_intelligent_planning_build_result_within_deadline(request, runtime)
            .await?
            .map(|result| result.snapshot))
    }

    async fn load_intelligent_planning_build_result_within_deadline(
        &self,
        request: &crate::application::routing_engine::request::RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
    ) -> Result<Option<PlanningBuildResult>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.load_intelligent_planning_build_result_in_read(&mut read, request, runtime)
            .await
    }

    async fn load_intelligent_planning_build_result_in_read(
        &self,
        read: &mut crate::persistence::ReadSession,
        request: &crate::application::routing_engine::request::RouteRequestFacts,
        runtime: RuntimeOverlaySnapshot,
    ) -> Result<Option<PlanningBuildResult>, ApplicationError> {
        let stored =
            crate::persistence::stores::routing_policy_v3_stage_upgrade::load_effective_active_in(
                read.connection(),
            )
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
        let compiled_v3 = aggregate
            .compile_v3()
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let attempt_budget = compiled_v3
            .attempt_budget
            .into_execution_profile(&compiled_v3.circuit_breaker)
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let quality_config = crate::application::quality_projection::QualityProjectionConfig {
            quality_policy_revision: routing_policy_revision,
            recent_minimum_samples: u64::from(
                compiled_v3.reliability_sampling.recent_minimum_samples,
            ),
            historical_minimum_samples: u64::from(
                compiled_v3.reliability_sampling.historical_minimum_samples,
            ),
            optimistic_reliability_basis_points: compiled_v3
                .reliability_sampling
                .optimistic_reliability_basis_points(),
            optimistic_latency_ms: compiled_v3.reliability_sampling.optimistic_latency_ms,
            real_traffic_weight_basis_points: compiled_v3
                .reliability_source_weights
                .real_traffic_basis_points(),
            monitoring_weight_basis_points: compiled_v3
                .reliability_source_weights
                .monitoring_basis_points(),
            real_source_eligible: true,
            monitoring_source_eligible: true,
            current_lifecycle_revision: None,
        };
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
        let mut result = builder
            .build_with_assessments(
                read,
                &options,
                policy,
                routing_policy_revision,
                attempt_budget,
                quality_config,
                DispatchAlgorithmProfile::default(),
                runtime,
                request,
            )
            .await
            .map_err(|error| {
                #[cfg(test)]
                crate::observability::runtime::bootstrap::emit(
                    crate::services::proxy::runtime_events::planning_snapshot_failed(),
                );
                match error {
                    crate::application::operational_facts::planning_snapshot::PlanningSnapshotBuildError::CandidateLimitExceeded {
                        actual,
                        limit,
                    } => ApplicationError::CandidateLimitExceeded { actual, limit },
                    _ => ApplicationError::ConstraintViolation,
                }
            })?;
        if let Some(model) = request.requested_model() {
            let ids = result
                .snapshot
                .candidates
                .iter()
                .map(|candidate| candidate.station_key_id.clone())
                .collect::<Vec<_>>();
            let pricing = PricingStore
                .resolve_station_key_pricing_many(
                    read,
                    &ids,
                    model,
                    &request.admitted_at_ms().to_string(),
                )
                .await
                .map_err(ApplicationError::from)?;
            for candidate in &mut result.snapshot.candidates {
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
        Ok(Some(result))
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
                station_key_lifecycle_revision: row.station_key_lifecycle_revision,
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
    #[cfg(test)]
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
        let mut planner_error_code: Option<String> = None;
        let (
            settings,
            planner_policy_config,
            active_policy_config,
            request,
            candidates,
            planning_result,
            quality_summaries,
            attempt_diagnostics,
        ) = {
            let mut read = self.runtime.begin_read().await?;
            let settings = self.store.load_execution_settings(&mut read).await?;
            let stored_policy = crate::persistence::stores::routing_policy_v3_stage_upgrade::load_effective_active_in(
                read.connection(),
            )
                .await
                .map_err(ApplicationError::from)?
                .ok_or(ApplicationError::NotFound)?;
            let aggregate = RoutingPolicyAggregate::from_stored(stored_policy)
                .map_err(|_| ApplicationError::ConstraintViolation)?;
            let planner_policy_config = aggregate.policy.clone();
            let active_policy_config = aggregate
                .policy_v3
                .clone()
                .map_or_else(
                    || {
                        crate::models::routing_policy::RoutingPolicyConfigV3::from_v2(
                            &planner_policy_config,
                        )
                        .map(|upgrade| upgrade.policy)
                    },
                    Ok,
                )
                .map_err(|_| ApplicationError::ConstraintViolation)?;
            let request = route_request_facts_for_read_model(&settings, now_ms);
            let candidates = self
                .load_workspace_candidates_with_request_pricing_in_read(&mut read, &request)
                .await?
                .into_iter()
                .map(|row| (row.candidate, row.pricing_context))
                .collect::<Vec<_>>();
            // Keep the candidate source, planner assessment, and quality
            // summary inside the same caller-owned durable read. A later
            // write cannot produce a mixed-version workspace response.
            let planning_result = match tokio::time::timeout_at(
                tokio::time::Instant::from_std(planning_context.deadline()),
                self.load_intelligent_planning_build_result_in_read(
                    &mut read,
                    &request,
                    RuntimeOverlaySnapshot {
                        runtime_instance_id: "routing-workspace".to_string(),
                        runtime_revision: 1,
                        candidate_set_revision: 1,
                        in_flight: 0,
                        max_concurrency: 1,
                        affinity_station_key_id: None,
                    },
                ),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(ApplicationError::DeadlineExceeded)) | Err(_) => {
                    return Err(ApplicationError::DeadlineExceeded)
                }
                Ok(Err(error)) => {
                    planner_error_code = Some(planner_error_code_for(&error));
                    None
                }
            };
            let scopes = candidates
                .iter()
                .map(|(candidate, _)| format!("station_key:{}", candidate.station_key_id))
                .collect::<Vec<_>>();
            let quality_summaries = RoutingQualityStore
                .load_summary_json(read.connection(), &scopes)
                .await?
                .into_iter()
                .filter_map(|(scope, value)| {
                    serde_json::from_value::<
                        crate::application::quality_projection::QualitySummary,
                    >(value)
                    .ok()
                    .map(|summary| (scope, summary))
                })
                .collect::<BTreeMap<_, _>>();
            let attempt_diagnostics = RoutingQualityStore
                .load_attempt_count_diagnostics(read.connection(), &scopes)
                .await?;
            (
                settings,
                planner_policy_config,
                active_policy_config,
                request,
                candidates,
                planning_result,
                quality_summaries,
                attempt_diagnostics,
            )
        };
        let circuit_read_snapshot = self.load_station_key_circuit_read_snapshot(now_ms).await?;
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
        let mut score_statuses = BTreeMap::new();
        let mut planner_exclusion_codes = BTreeMap::new();
        let mut assessment_provenance = BTreeMap::new();
        let (
            score_by_key,
            plan_diagnostics,
            planner_evaluation,
            planner_evaluation_code,
            workspace_revisions,
        ) = planning_result
            .map(|result| {
                let workspace_revisions = RoutingWorkspaceRevisionSnapshot {
                    runtime_generation_id: result
                        .snapshot
                        .routing_runtime_generation_id
                        .clone(),
                    policy_revision: Some(result.snapshot.routing_policy_revision),
                    quality_revision: Some(result.snapshot.routing_quality_revision),
                    health_revision: Some(result.snapshot.routing_health_revision),
                    quality_projection_backlog: Some(
                        result.snapshot.quality_projection_backlog,
                    ),
                    quality_projection_lag_seconds: Some(
                        result.snapshot.quality_projection_lag_seconds,
                    ),
                    quality_stale: Some(result.snapshot.quality_stale),
                };
                // The planner assesses every enabled key, including keys
                // without a credential. The legacy Workspace source omits
                // those non-executable rows, so integrity is directional:
                // every displayed row must have exactly one compatible
                // assessment; undisplayed assessments are harmless.
                let assessment_integrity_ok = candidates.iter().all(|(candidate, _)| {
                    result
                        .assessments
                        .iter()
                        .filter(|assessment| {
                            assessment.station_key_id == candidate.station_key_id
                                && assessment.endpoint_revision
                                    == candidate.station_endpoint_revision
                                && assessment.snapshot_id == result.snapshot.snapshot_id
                                && assessment.durable_revision == result.snapshot.durable_revision
                        })
                        .count()
                        == 1
                }) && result.assessments.iter().all(|assessment| {
                    assessment.snapshot_id == result.snapshot.snapshot_id
                        && assessment.durable_revision == result.snapshot.durable_revision
                });
                if !assessment_integrity_ok {
                    return (
                        BTreeMap::new(),
                        BTreeMap::new(),
                        RoutingPlannerEvaluationStatus::Unavailable,
                        Some("planner_assessment_source_mismatch".to_string()),
                        workspace_revisions,
                    );
                }
                for assessment in &result.assessments {
                    let status = match (assessment.eligibility, assessment.candidate_set) {
                        (
                            crate::application::operational_facts::planning_snapshot::PlanningCandidateEligibility::AdmittedForScoring,
                            crate::application::operational_facts::planning_snapshot::PlanningCandidateSet::WithinLimit,
                        ) => RoutingScoreStatus::Scored,
                        _ => RoutingScoreStatus::Excluded,
                    };
                    score_statuses.insert(assessment.station_key_id.clone(), status);
                    if let Some(reason) = &assessment.primary_reason {
                        planner_exclusion_codes.insert(
                            assessment.station_key_id.clone(),
                            std::iter::once(reason.clone())
                                .chain(assessment.secondary_reason_codes.iter().cloned())
                                .collect(),
                        );
                    }
                    assessment_provenance.insert(
                        assessment.station_key_id.clone(),
                        (
                            assessment.snapshot_id.clone(),
                            assessment.durable_revision,
                            assessment.request_context_fingerprint.clone(),
                        ),
                    );
                }
                let score_by_key = result
                    .snapshot
                    .candidates
                    .iter()
                    .filter_map(|candidate| {
                        if score_statuses.get(&candidate.station_key_id)
                            != Some(&RoutingScoreStatus::Scored)
                        {
                            return None;
                        }
                        let multiplier_cost_basis = multiplier_by_key
                            .get(&candidate.station_key_id)
                            .copied()
                            .and_then(
                                crate::application::routing_engine::factors::
                                    cost_efficiency_from_multiplier,
                            );
                        candidate_score_breakdown_with_cost_basis(
                            candidate,
                            &planner_policy_config,
                            multiplier_cost_basis,
                        )
                        .map(|breakdown| (candidate.station_key_id.clone(), breakdown.into()))
                    })
                    .collect::<BTreeMap<_, _>>();
                let plan_diagnostics = plan_snapshot(&result.snapshot, b"routing-workspace", 0)
                    .map(|plan| {
                        plan.candidates
                            .into_iter()
                            .fold(BTreeMap::new(), |mut diagnostics, candidate| {
                                diagnostics.entry(candidate.station_key_id).or_insert(
                                    RoutingCandidatePlanDiagnostics {
                                        effective_score: candidate.utility.value().get(),
                                        base_score: candidate.base_utility.value().get(),
                                        target_rank: candidate.target_rank,
                                        tier: candidate.tier,
                                        lifecycle_revision: u64::try_from(
                                            candidate.lifecycle_revision.max(1),
                                        )
                                        .unwrap_or(1),
                                    },
                                );
                                diagnostics
                            })
                    })
                    .unwrap_or_default();
                (
                    score_by_key,
                    plan_diagnostics,
                    RoutingPlannerEvaluationStatus::Available,
                    None,
                    workspace_revisions,
                )
            })
            .unwrap_or_else(|| {
                (
                    BTreeMap::new(),
                    BTreeMap::new(),
                    RoutingPlannerEvaluationStatus::Unavailable,
                    Some(
                        planner_error_code
                            .take()
                            .unwrap_or_else(|| "planner_build_unavailable".to_string()),
                    ),
                    RoutingWorkspaceRevisionSnapshot::default(),
                )
            });
        Ok(workspace_snapshot_from_canonical_candidates(
            active_policy_config,
            settings.max_rate_multiplier,
            settings.routing_group_scope,
            candidates,
            &score_by_key,
            &score_statuses,
            &planner_exclusion_codes,
            &assessment_provenance,
            planner_evaluation,
            planner_evaluation_code,
            &quality_summaries,
            &plan_diagnostics,
            &attempt_diagnostics,
            &circuit_read_snapshot,
            workspace_revisions,
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
            });
        }
        Ok(runtime_overlay_from_candidates(
            facts,
            chrono::Utc::now().timestamp_millis(),
            1,
            1024,
        ))
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
                preview_policy_version:
                    crate::application::queries::routing_workspace::ROUTING_PREVIEW_POLICY_VERSION
                        .to_string(),
                capacity_mode: "snapshot_only".to_string(),
                selected_capacity_acquired: false,
                selected_station_key_id: None,
                selected_station_id: None,
                mapped_model: None,
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
                        preview_policy_version: crate::application::queries::routing_workspace::ROUTING_PREVIEW_POLICY_VERSION
                            .to_string(),
                        capacity_mode: "snapshot_only".to_string(),
                        selected_capacity_acquired: false,
                        selected_station_key_id: None,
                        selected_station_id: None,
                        mapped_model: None,
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
            max_rate_multiplier,
            routing_group_scope: routing_group_filter.clone(),
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
        let canonical_plan = plan_snapshot(&planning_snapshot, b"simulation", 1).ok();
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
            preview_policy_version:
                crate::application::queries::routing_workspace::ROUTING_PREVIEW_POLICY_VERSION
                    .to_string(),
            capacity_mode: "snapshot_only".to_string(),
            selected_capacity_acquired: false,
            selected_station_key_id,
            selected_station_id,
            mapped_model,
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
) -> Result<crate::models::routing_policy::RoutingPolicyDocumentV3, PersistenceError> {
    let policy = routing_policy_v3_from_stored(&stored.config)?;
    let revision = u64::try_from(stored.revision).map_err(|_| {
        PersistenceError::InvariantViolation("routing policy revision is invalid".into())
    })?;
    Ok(crate::models::routing_policy::RoutingPolicyDocumentV3 {
        format_version: crate::models::routing_policy::ROUTING_POLICY_DOCUMENT_FORMAT_VERSION,
        base_revision: revision,
        policy,
    })
}

pub(crate) fn routing_policy_v3_from_stored(
    value: &serde_json::Value,
) -> Result<crate::models::routing_policy::RoutingPolicyConfigV3, PersistenceError> {
    match value.get("version").and_then(serde_json::Value::as_u64) {
        Some(3) => crate::models::routing_policy::RoutingPolicyConfigV3::from_stored_value(value)
            .map_err(|error| {
                PersistenceError::InvariantViolation(format!(
                    "routing policy config is invalid: {error:?}"
                ))
            }),
        Some(1 | 2) => {
            let legacy =
                crate::models::routing_policy::RoutingPolicyConfigV2::from_stored_value(value)
                    .map_err(|error| {
                        PersistenceError::InvariantViolation(format!(
                            "routing policy config is invalid: {error:?}"
                        ))
                    })?;
            crate::models::routing_policy::RoutingPolicyConfigV3::from_v2(&legacy)
                .map(|upgrade| upgrade.policy)
                .map_err(|error| {
                    PersistenceError::InvariantViolation(format!(
                        "routing policy upgrade is invalid: {error:?}"
                    ))
                })
        }
        Some(version) => Err(PersistenceError::InvariantViolation(format!(
            "routing policy version {version} is unsupported"
        ))),
        None => Err(PersistenceError::InvariantViolation(
            "routing policy version is missing".into(),
        )),
    }
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
/// boundary, then normalize the result to the active V3 document shape.
fn decode_routing_document(
    bytes: &[u8],
) -> Result<crate::models::routing_policy::RoutingPolicyDocumentV3, PolicyDocumentError> {
    let value = decode_strict_json::<serde_json::Value>(bytes)?;
    let policy_version = value
        .get("policy")
        .and_then(|policy| policy.get("version"))
        .and_then(serde_json::Value::as_u64);
    match policy_version {
        Some(3) => {
            let document = serde_json::from_value::<
                crate::models::routing_policy::RoutingPolicyDocumentV3,
            >(value)
            .map_err(|error| PolicyDocumentError::InvalidJson(error.to_string()))?;
            document
                .validate()
                .map_err(|error| PolicyDocumentError::InvalidJson(format!("{error:?}")))?;
            Ok(document)
        }
        Some(2) => {
            let document = serde_json::from_value::<
                crate::models::routing_policy::RoutingPolicyDocumentV2,
            >(value)
            .map_err(|error| PolicyDocumentError::InvalidJson(error.to_string()))?;
            document
                .validate()
                .map_err(|error| PolicyDocumentError::InvalidJson(format!("{error:?}")))?;
            crate::models::routing_policy::RoutingPolicyDocumentV3::from_v2(&document)
                .map(|(document, _)| document)
                .map_err(|error| PolicyDocumentError::InvalidJson(format!("{error:?}")))
        }
        Some(1) => {
            let legacy = serde_json::from_value::<
                crate::models::routing_policy::RoutingPolicyDocumentV1,
            >(value)
            .map_err(|error| PolicyDocumentError::InvalidJson(error.to_string()))?;
            let v2 = crate::models::routing_policy::RoutingPolicyDocumentV2::from_v1(&legacy)
                .map_err(|error| PolicyDocumentError::InvalidJson(format!("{error:?}")))?;
            crate::models::routing_policy::RoutingPolicyDocumentV3::from_v2(&v2)
                .map(|(document, _)| document)
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
    let current_revision = {
        let mut read = runtime.begin_read().await?;
        crate::persistence::stores::routing_policy_v3_stage_upgrade::load_effective_active_in(
            read.connection(),
        )
        .await?
        .ok_or_else(|| PersistenceError::InvariantViolation("routing policy is missing".into()))?
        .revision
    };
    if current_revision != document.base_revision {
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
        crate::persistence::stores::routing_policy_v3_stage_upgrade::load_effective_active_in(
            read.connection(),
        )
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
    let stored =
        crate::persistence::stores::routing_policy_v3_stage_upgrade::load_effective_active(
            &runtime,
        )
        .await?
        .ok_or(PersistenceError::NotFound)?;
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
        let policy =
            crate::persistence::stores::routing_policy_v3_stage_upgrade::load_effective_active_in(
                read.connection(),
            )
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
        || crate::application::routing_policy::compile_config_v3(
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
        .apply_routing_policy_document_v3(document, TrustedDocumentSource::file_watch())
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
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::application::operational_facts::pricing_projector::RoutingCostBasis;
    use crate::application::routing_engine::request::PlanningRequestContext;
    use crate::models::{
        pricing::{PricingStatus, RequestKind},
        proxy::UpstreamApiFormat,
        routing::StationKeyCapabilities,
        routing_policy::RoutingPolicyDocumentV3,
    };

    #[tokio::test]
    async fn execution_settings_preserve_supported_runtime_defaults() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("routing.sqlite3");
        let runtime = crate::persistence::runtime::PersistenceRuntime::initialize_new(&path)
            .await
            .expect("runtime");
        let service = RoutingService::new(runtime.handle());

        let defaults = service.load_execution_settings().await.expect("defaults");
        assert_eq!(defaults.max_rate_multiplier, None);
        assert_eq!(defaults.routing_group_scope, RoutingGroupFilter::AllGroups);
        assert!(!defaults.allow_depleted_fallback);
        assert_eq!(defaults.outbound_proxy_mode, "inherit");
        assert_eq!(defaults.global_proxy_mode, "direct");
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
        let baseline_v3 = routing_policy_v3_from_stored(&baseline.config)
            .expect("baseline policy upgrades to v3");
        sync_routing_policy_file(runtime.handle(), &baseline, false)
            .await
            .expect("baseline materialization");

        let mut first_config = baseline_v3.clone();
        first_config.affinity_ttl_seconds = 301;
        let first = service
            .apply_routing_policy_document_v3(
                RoutingPolicyDocumentV3 {
                    format_version:
                        crate::models::routing_policy::ROUTING_POLICY_DOCUMENT_FORMAT_VERSION,
                    base_revision: baseline.revision,
                    policy: first_config,
                },
                TrustedDocumentSource::ui(),
            )
            .await
            .expect("first policy apply");

        let mut second_config =
            routing_policy_v3_from_stored(&first.config).expect("first policy is v3");
        second_config.affinity_ttl_seconds = 302;
        let _second = service
            .apply_routing_policy_document_v3(
                RoutingPolicyDocumentV3 {
                    format_version:
                        crate::models::routing_policy::ROUTING_POLICY_DOCUMENT_FORMAT_VERSION,
                    base_revision: first.revision,
                    policy: second_config,
                },
                TrustedDocumentSource::ui(),
            )
            .await
            .expect("second policy apply");

        // A delayed staged continuation must not publish over the active
        // mirror. Staged policy becomes materializable only after generation
        // activation, so the active baseline remains on disk here.
        sync_routing_policy_file(runtime.handle(), &first, true)
            .await
            .expect("stale materialization is ignored");
        let bytes = std::fs::read(temp.path().join("config").join("routing-policy.json"))
            .expect("managed routing document");
        let materialized: RoutingPolicyDocumentV3 =
            serde_json::from_slice(&bytes).expect("managed routing document decodes");
        assert_eq!(materialized.base_revision, baseline.revision);
        assert_eq!(materialized.policy, baseline_v3);
        runtime.close().await.expect("close persistence runtime");
    }

    #[test]
    fn local_workspace_candidate_uses_token_pricing_projection() {
        let settings = RuntimeRoutingSettings {
            max_rate_multiplier: Some(2.0),
            routing_group_scope: RoutingGroupFilter::AllGroups,
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
