use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub(crate) mod effect_planner;
pub(crate) mod failure;
pub(crate) mod outcome;
pub(crate) mod outcome_orchestrator;

use futures_util::future::BoxFuture;

use crate::{
    application::request_lifecycle::{
        attempt::{AttemptTerminal, AttemptTerminalRecord, DurableCapabilityEffect, HealthEffect},
        ports::{
            AttemptCommitAck, AttemptCostCommitAck, AttemptCostCommitRecord, LifecycleWriteError,
            RequestCommitAck, RequestCostAggregateCommitAck, RequestCostAggregateCommitRecord,
            RequestLifecycleStore, RequestRouteSelectionAck, RequestStartAck,
        },
        request::{
            FinalRequestRecord, RequestRouteSelectionRecord, RequestStartRecord, RequestTerminal,
        },
    },
    application::{
        clock::{Clock, SystemClock},
        observation_ingestion::ObservationIngestion,
        station_key_circuit::CircuitPersistenceGate,
    },
    models::routing_observation::{
        FailureAttribution, ObservationOrder, ObservationOutcome, ObservationRetryDisposition,
        ObservationScope, ObservationSource, RecoveryOrigin, ResponseOrigin, RoutingObservation,
    },
    persistence::{
        error::PersistenceError,
        runtime::PersistenceHandle,
        stores::request_lifecycle_reconciliation::{
            default_startup_reconciliation_batch_size, reconcile_startup_interrupted_batch,
            StartupReconciliationReport,
        },
        stores::request_log_store::{
            AttemptPersistenceResult, RequestLogStore, RequestRouteSelectionPersistenceResult,
            RequestStartPersistenceResult,
        },
        stores::request_log_write::{
            AttemptDurableEffectWrite, AttemptHealthUpdate, AttemptTerminalWrite,
            RequestLogAnnotationsWrite, RequestRouteSelectionWrite,
            RequestRoutingOutcomeSummaryWrite, RequestStartWrite, RequestTerminalWrite,
        },
        stores::request_outcome_store::{
            AttemptCostWrite, RequestCostAggregateWrite, RequestOutcomeStore,
        },
        stores::request_terminal_outbox::RequestTerminalOutboxStore,
        stores::routing_health_verdict_store::{
            RoutingHealthVerdictStore, UnsupportedModelObservation,
        },
        stores::routing_policy_store::RoutingPolicyStore,
        stores::station_key_circuit_store::{CircuitTerminalInput, StationKeyCircuitStore},
    },
};

#[derive(Clone)]
pub(crate) struct RequestFinalizationService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    observations: ObservationIngestion,
    circuit_persistence_gate: Arc<CircuitPersistenceGate>,
    circuit_persistence_backlog: Arc<Mutex<CircuitPersistenceBacklog>>,
}

const TERMINAL_OUTBOX_BATCH_SIZE: u32 = 64;
const TERMINAL_OUTBOX_LEASE_MS: i64 = 30_000;
const MAX_CIRCUIT_PERSISTENCE_BACKLOG: usize = 4_096;

#[derive(Debug, Default)]
struct CircuitPersistenceBacklog {
    records: BTreeMap<String, AttemptTerminalRecord>,
    overflow_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalOutboxReconciliationReport {
    pub(crate) batches_completed: u64,
    pub(crate) terminals_projected: u64,
}

impl RequestFinalizationService {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=request-finalization-compat-constructor; owner=application/request_finalization; remove_when=all compositions inject the shared circuit persistence gate"
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
            clock: Arc::new(SystemClock),
            observations: ObservationIngestion::new(),
            circuit_persistence_gate,
            circuit_persistence_backlog: Arc::new(Mutex::new(CircuitPersistenceBacklog::default())),
        }
    }

    async fn persist_attempt_terminal_once(
        &self,
        record: AttemptTerminalRecord,
    ) -> Result<AttemptCommitAck, LifecycleWriteError> {
        let mut write = map_attempt_terminal(record);
        write.ingested_at_ms = self.clock.now_utc().timestamp_millis().max(0);
        let mut session = self
            .runtime
            .begin_write()
            .await
            .map_err(map_persistence_error)?;
        let outcome: AttemptPersistenceResult = RequestLogStore
            .finish_attempt(&mut session, &write)
            .await
            .map_err(map_persistence_error)?;
        if outcome.inserted {
            let circuit_policy = RoutingPolicyStore
                .load_circuit_policy_parameters(session.connection())
                .await
                .map_err(map_persistence_error)?;
            let boundary_crossed = outcome.boundary_crossed;
            let circuit_attempt_id = format!("{}:{}", write.request_id, write.ordinal);
            let circuit_success = write.terminal_kind == "succeeded";
            StationKeyCircuitStore
                .finish_attempt(
                    session.connection(),
                    CircuitTerminalInput {
                        station_key_id: &write.station_key_id,
                        lifecycle_revision: u64::try_from(write.credential_revision.max(1))
                            .map_err(|_| {
                                LifecycleWriteError::Unavailable(
                                    "invalid key lifecycle revision".into(),
                                )
                            })?,
                        policy_revision: circuit_policy.policy_revision,
                        attempt_id: &circuit_attempt_id,
                        lease_id: Some(circuit_attempt_id.as_str()),
                        lease_revision: None,
                        now_ms: u64::try_from(write.terminal_at_ms.max(0)).unwrap_or(0),
                        occurred_at_ms: u64::try_from(write.terminal_at_ms.max(0)).unwrap_or(0),
                        success: circuit_success,
                        boundary_crossed,
                        affects_circuit: circuit_success
                            || failure_counts_toward_key_circuit(write.public_code.as_deref()),
                        failure_code: write.public_code.as_deref(),
                        recovery_origin: "normal",
                        retry_disposition: circuit_retry_disposition(
                            write.retry_disposition.as_deref(),
                            circuit_success,
                        ),
                        consecutive_failure_threshold: circuit_policy.consecutive_failure_threshold,
                        recovery_success_threshold: circuit_policy.recovery_success_threshold,
                        recovery_wait_ms: circuit_policy.recovery_wait_ms,
                    },
                )
                .await
                .map_err(map_persistence_error)?;
            apply_durable_attempt_effect(&mut session, &write)
                .await
                .map_err(map_persistence_error)?;
        }
        session.commit().await.map_err(map_persistence_error)?;
        Ok(AttemptCommitAck {
            inserted: outcome.inserted,
        })
    }

    async fn record_circuit_persistence_failure(&self, record: &AttemptTerminalRecord) {
        let lifecycle_revision =
            u64::try_from(record.context.credential_revision.max(1)).unwrap_or(1);
        self.circuit_persistence_gate
            .mark_station_key(&record.context.station_key_id, lifecycle_revision);
        let attempt_id = format!(
            "{}:{}",
            record.context.attempt_id.request_id, record.context.attempt_id.ordinal
        );
        {
            let mut backlog = self
                .circuit_persistence_backlog
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !backlog.records.contains_key(&attempt_id) {
                if backlog.records.len() < MAX_CIRCUIT_PERSISTENCE_BACKLOG {
                    backlog.records.insert(attempt_id, record.clone());
                } else {
                    backlog.overflow_count = backlog.overflow_count.saturating_add(1);
                }
            }
        }
        let station_key_id = record.context.station_key_id.clone();
        let now_ms = self.clock.now_utc().timestamp_millis().max(0) as u64;
        let store = StationKeyCircuitStore;
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

    fn remove_circuit_persistence_backlog(&self, record: &AttemptTerminalRecord) {
        let attempt_id = format!(
            "{}:{}",
            record.context.attempt_id.request_id, record.context.attempt_id.ordinal
        );
        self.circuit_persistence_backlog
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .records
            .remove(&attempt_id);
    }

    /// Replays the bounded canonical terminal backlog. The supervised circuit
    /// reaper calls this before its explicit read/write health check; ordinary
    /// request traffic never clears the persistence gate.
    pub(crate) async fn replay_circuit_persistence_backlog(
        &self,
    ) -> Result<u64, LifecycleWriteError> {
        let records = {
            let backlog = self
                .circuit_persistence_backlog
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if backlog.overflow_count > 0 {
                return Err(LifecycleWriteError::Unavailable(
                    "circuit persistence backlog overflowed".into(),
                ));
            }
            backlog.records.values().cloned().collect::<Vec<_>>()
        };
        let mut replayed = 0_u64;
        for record in records {
            self.persist_attempt_terminal_once(record.clone()).await?;
            self.remove_circuit_persistence_backlog(&record);
            replayed = replayed.saturating_add(1);
        }
        Ok(replayed)
    }

    pub(crate) async fn reconcile_startup_interrupted_request_lifecycle(
        &self,
    ) -> Result<StartupReconciliationReport, LifecycleWriteError> {
        let mut total = StartupReconciliationReport::empty();
        loop {
            let now_ms = self.clock.now_utc().timestamp_millis();
            let mut session = self
                .runtime
                .begin_write()
                .await
                .map_err(map_persistence_error)?;
            let mut batch = reconcile_startup_interrupted_batch(
                session.connection(),
                now_ms,
                default_startup_reconciliation_batch_size(),
            )
            .await
            .map_err(map_persistence_error)?;
            if !batch.routing_samples.is_empty() {
                let circuit_policy = RoutingPolicyStore
                    .load_circuit_policy_parameters(session.connection())
                    .await
                    .map_err(map_persistence_error)?;
                for sample in batch.routing_samples.drain(..) {
                    StationKeyCircuitStore
                        .finish_attempt(
                            session.connection(),
                            CircuitTerminalInput {
                                station_key_id: &sample.station_key_id,
                                lifecycle_revision: sample.station_key_lifecycle_revision,
                                policy_revision: circuit_policy.policy_revision,
                                attempt_id: &sample.attempt_id,
                                lease_id: Some(sample.attempt_id.as_str()),
                                lease_revision: None,
                                now_ms: u64::try_from(sample.finalized_at_ms.max(0)).unwrap_or(0),
                                occurred_at_ms: u64::try_from(sample.finalized_at_ms.max(0))
                                    .unwrap_or(0),
                                success: sample.outcome == "success",
                                boundary_crossed: sample.boundary_crossed,
                                affects_circuit: sample.outcome == "success"
                                    || failure_counts_toward_key_circuit(
                                        sample.failure_code.as_deref(),
                                    ),
                                failure_code: sample.failure_code.as_deref(),
                                recovery_origin: "crash_recovery",
                                retry_disposition: "stop_request",
                                consecutive_failure_threshold: circuit_policy
                                    .consecutive_failure_threshold,
                                recovery_success_threshold: circuit_policy
                                    .recovery_success_threshold,
                                recovery_wait_ms: circuit_policy.recovery_wait_ms,
                            },
                        )
                        .await
                        .map_err(map_persistence_error)?;
                    let generation_eligibility = sample.generation_eligibility.as_str();
                    self.observations
                        .append_with_generation_eligibility(
                            &mut session,
                            routing_observation_from_finalized(sample)?,
                            Some(generation_eligibility),
                        )
                        .await
                        .map_err(map_persistence_error)?;
                }
            }
            session.commit().await.map_err(map_persistence_error)?;
            let has_more = batch.has_more;
            total.add_batch(batch);
            if !has_more {
                return Ok(total);
            }
        }
    }

    pub(crate) async fn reconcile_terminal_outbox(
        &self,
    ) -> Result<TerminalOutboxReconciliationReport, LifecycleWriteError> {
        let owner = format!("startup-terminal-outbox-{}", uuid::Uuid::now_v7());
        let mut report = TerminalOutboxReconciliationReport {
            batches_completed: 0,
            terminals_projected: 0,
        };
        loop {
            let now_ms = self.clock.now_utc().timestamp_millis();
            let mut claim_session = self
                .runtime
                .begin_write()
                .await
                .map_err(map_persistence_error)?;
            let (records, batch) = RequestTerminalOutboxStore
                .claim_batch(
                    claim_session.connection(),
                    &owner,
                    now_ms,
                    TERMINAL_OUTBOX_LEASE_MS,
                    TERMINAL_OUTBOX_BATCH_SIZE,
                )
                .await
                .map_err(map_persistence_error)?;
            claim_session
                .commit()
                .await
                .map_err(map_persistence_error)?;
            report.batches_completed += 1;
            for record in records {
                let mut session = self
                    .runtime
                    .begin_write()
                    .await
                    .map_err(map_persistence_error)?;
                let outcome = RequestLogStore
                    .finish_request(&mut session, &record)
                    .await
                    .map_err(map_persistence_error)?;
                for sample in outcome.routing_samples {
                    let generation_eligibility = sample.generation_eligibility.as_str();
                    self.observations
                        .append_with_generation_eligibility(
                            &mut session,
                            routing_observation_from_finalized(sample)?,
                            Some(generation_eligibility),
                        )
                        .await
                        .map_err(map_persistence_error)?;
                }
                RequestTerminalOutboxStore
                    .delete_claimed(session.connection(), &record.request_id, &owner)
                    .await
                    .map_err(map_persistence_error)?;
                session.commit().await.map_err(map_persistence_error)?;
                report.terminals_projected += u64::from(outcome.finalized);
            }
            if !batch.has_more {
                return Ok(report);
            }
        }
    }
}

impl RequestLifecycleStore for RequestFinalizationService {
    fn start_request(
        &self,
        record: RequestStartRecord,
    ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
        self.start_request_with_annotations(
            record,
            crate::application::request_lifecycle::request::RequestLogAnnotations::default(),
        )
    }

    fn start_request_with_annotations(
        &self,
        record: RequestStartRecord,
        annotations: crate::application::request_lifecycle::request::RequestLogAnnotations,
    ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
        let runtime = self.runtime.clone();
        let created_at_ms = self.clock.now_utc().timestamp_millis();
        let write = map_request_start(record, annotations);
        Box::pin(async move {
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome: RequestStartPersistenceResult = RequestLogStore
                .start_request(&mut session, &write, created_at_ms)
                .await
                .map_err(map_persistence_error)?;
            session.commit().await.map_err(map_persistence_error)?;
            Ok(RequestStartAck {
                inserted: outcome.inserted,
            })
        })
    }

    fn finish_attempt(
        &self,
        record: AttemptTerminalRecord,
    ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
        let service = self.clone();
        Box::pin(async move {
            let gate_on_failure = !matches!(record.terminal, AttemptTerminal::Abandoned { .. });
            let result = service.persist_attempt_terminal_once(record.clone()).await;
            match &result {
                Ok(_) => service.remove_circuit_persistence_backlog(&record),
                Err(_) if gate_on_failure => {
                    service.record_circuit_persistence_failure(&record).await;
                }
                Err(_) => {}
            }
            result
        })
    }

    fn record_route_selection(
        &self,
        record: RequestRouteSelectionRecord,
    ) -> BoxFuture<'static, Result<RequestRouteSelectionAck, LifecycleWriteError>> {
        let runtime = self.runtime.clone();
        let write = map_route_selection(record);
        Box::pin(async move {
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome: RequestRouteSelectionPersistenceResult = RequestLogStore
                .record_route_selection(&mut session, &write)
                .await
                .map_err(map_persistence_error)?;
            session.commit().await.map_err(map_persistence_error)?;
            Ok(RequestRouteSelectionAck {
                updated: outcome.updated,
            })
        })
    }

    fn finish_request(
        &self,
        record: FinalRequestRecord,
    ) -> BoxFuture<'static, Result<RequestCommitAck, LifecycleWriteError>> {
        let runtime = self.runtime.clone();
        let terminal_at_ms = self.clock.now_utc().timestamp_millis();
        let write = map_request_terminal(record, terminal_at_ms);
        let service = self.clone();
        Box::pin(async move {
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            RequestTerminalOutboxStore
                .enqueue(session.connection(), &write, terminal_at_ms)
                .await
                .map_err(map_persistence_error)?;
            session.commit().await.map_err(map_persistence_error)?;
            let outcome = service.reconcile_terminal_outbox().await?;
            Ok(RequestCommitAck {
                finalized: outcome.terminals_projected > 0,
            })
        })
    }

    fn finish_attempt_cost(
        &self,
        record: AttemptCostCommitRecord,
    ) -> BoxFuture<'static, Result<AttemptCostCommitAck, LifecycleWriteError>> {
        let runtime = self.runtime.clone();
        Box::pin(async move {
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome = RequestOutcomeStore
                .insert_attempt_cost(session.connection(), &map_attempt_cost(record))
                .await
                .map_err(map_persistence_error)?;
            session.commit().await.map_err(map_persistence_error)?;
            Ok(AttemptCostCommitAck {
                inserted: outcome.inserted,
            })
        })
    }

    fn finish_request_cost_aggregate(
        &self,
        record: RequestCostAggregateCommitRecord,
    ) -> BoxFuture<'static, Result<RequestCostAggregateCommitAck, LifecycleWriteError>> {
        let runtime = self.runtime.clone();
        Box::pin(async move {
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome = RequestOutcomeStore
                .insert_request_cost_aggregate(
                    session.connection(),
                    &map_request_cost_aggregate(record),
                )
                .await
                .map_err(map_persistence_error)?;
            session.commit().await.map_err(map_persistence_error)?;
            Ok(RequestCostAggregateCommitAck {
                inserted: outcome.inserted,
            })
        })
    }
}

fn circuit_retry_disposition(value: Option<&str>, success: bool) -> &'static str {
    if success {
        "end"
    } else if matches!(value, Some("RetrySameTarget" | "TryNextCandidate")) {
        "retryable_before_commit"
    } else {
        "stop_request"
    }
}

fn failure_counts_toward_key_circuit(public_code: Option<&str>) -> bool {
    !matches!(
        public_code,
        Some("upstream_insufficient_balance" | "upstream_model_unavailable")
    )
}

fn routing_observation_from_finalized(
    sample: crate::persistence::stores::routing_attempt_store::FinalizedRoutingAttemptSample,
) -> Result<RoutingObservation, LifecycleWriteError> {
    let outcome = match sample.outcome.as_str() {
        "success" => ObservationOutcome::Success,
        "attributable_failure" => match sample.failure_code.as_deref() {
            Some(code) if code.contains("rate_limit") || code.contains("429") => {
                ObservationOutcome::RateLimited
            }
            Some(code) if code.contains("timeout") => ObservationOutcome::Timeout,
            _ => ObservationOutcome::EndpointFailure,
        },
        "excluded" => ObservationOutcome::Cancelled,
        _ => {
            return Err(LifecycleWriteError::Unavailable(
                "finalized routing attempt has an invalid outcome".into(),
            ));
        }
    };
    let event_time_status = match sample.event_time_status.as_str() {
        "valid" => crate::models::routing_observation::EventTimeStatus::Valid,
        "missing" => crate::models::routing_observation::EventTimeStatus::Missing,
        "invalid" => crate::models::routing_observation::EventTimeStatus::Invalid,
        _ => {
            return Err(LifecycleWriteError::Unavailable(
                "finalized routing attempt has an invalid event time status".into(),
            ));
        }
    };
    let response_origin = match sample.response_origin.as_str() {
        "upstream" => ResponseOrigin::Upstream,
        "relay" => ResponseOrigin::Relay,
        "unknown" => ResponseOrigin::Unknown,
        _ => {
            return Err(LifecycleWriteError::Unavailable(
                "finalized routing attempt has an invalid response origin".into(),
            ));
        }
    };
    let failure_attribution = match sample.failure_attribution.as_str() {
        "key" => FailureAttribution::Key,
        "local" => FailureAttribution::Local,
        "client" => FailureAttribution::Client,
        "unknown" => FailureAttribution::Unknown,
        _ => {
            return Err(LifecycleWriteError::Unavailable(
                "finalized routing attempt has an invalid failure attribution".into(),
            ));
        }
    };
    let recovery_origin = match sample.recovery_origin.as_str() {
        "normal" => RecoveryOrigin::Normal,
        "crash_recovery" => RecoveryOrigin::CrashRecovery,
        "lease_reaper" => RecoveryOrigin::LeaseReaper,
        _ => {
            return Err(LifecycleWriteError::Unavailable(
                "finalized routing attempt has an invalid recovery origin".into(),
            ));
        }
    };
    let retry_disposition = match sample.retry_disposition.as_str() {
        "end" => ObservationRetryDisposition::End,
        "retryable_before_commit" => ObservationRetryDisposition::RetryableBeforeCommit,
        "stop_request" => ObservationRetryDisposition::StopRequest,
        _ => {
            return Err(LifecycleWriteError::Unavailable(
                "finalized routing attempt has an invalid retry disposition".into(),
            ));
        }
    };
    Ok(RoutingObservation {
        id: format!("routing-observation-{}", sample.attempt_id),
        order: ObservationOrder {
            producer_id: format!("request-finalization:{}", sample.correlation_id),
            producer_sequence: u64::from(sample.attempt_index),
            // Missing/invalid event time is represented by status; zero is a
            // storage placeholder and is never admitted to a quality window.
            event_at_ms: sample.event_at_ms.unwrap_or(0),
            ingested_at_ms: sample.finalized_at_ms.max(sample.observed_at_ms),
        },
        scope: ObservationScope {
            station_id: None,
            station_key_id: Some(sample.station_key_id),
            model: None,
            endpoint_revision: None,
        },
        source: ObservationSource::RealRequest,
        traffic_equivalence: crate::models::routing_observation::TrafficEquivalence::ExactRequest,
        outcome,
        latency_ms: sample.latency_ms,
        evidence_mass_basis_points: 10_000,
        comparability_key: sample.comparability_key,
        correlation_id: sample.correlation_id,
        attempt_index: sample.attempt_index,
        station_key_lifecycle_revision: sample.station_key_lifecycle_revision,
        cluster_finalized: true,
        cluster_expected_attempt_count: sample.expected_attempt_count,
        boundary_crossed: sample.boundary_crossed,
        event_time_status,
        response_origin,
        failure_code: sample.failure_code,
        failure_attribution,
        recovery_origin,
        retry_disposition,
        probe_state_revision: None,
        probe_scope: None,
    })
}

/// Returns true only when the attempt reached the outbound provider boundary.
/// This is deliberately independent from scoped health effects: a 429 or
/// another upstream error may have no durable health verdict but still must
/// count toward station-key circuit failures. Local adapter and downstream
/// failures must not penalize the key.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=v3-boundary-compat-helper; owner=application/request_finalization; remove_when=attempt ledger owns all boundary classification"
    )
)]
fn attempt_boundary_crossed(record: &AttemptTerminalWrite) -> bool {
    if record.terminal_kind == "succeeded" {
        return true;
    }
    if record.terminal_kind == "abandoned" {
        return false;
    }
    !matches!(
        record.failure_blame.as_deref(),
        Some("LocalAdapter") | Some("Downstream") | Some("local") | Some("downstream")
    )
}

fn map_request_start(
    record: RequestStartRecord,
    annotations: crate::application::request_lifecycle::request::RequestLogAnnotations,
) -> RequestStartWrite {
    RequestStartWrite {
        request_id: record.context.request_id,
        method: record.context.method,
        local_path: record.context.local_path,
        endpoint: record.context.endpoint,
        received_at_ms: record.context.received_at_ms,
        model: annotations.model,
        stream: annotations.stream,
        reasoning_effort: annotations.reasoning_effort,
    }
}

fn map_route_selection(record: RequestRouteSelectionRecord) -> RequestRouteSelectionWrite {
    RequestRouteSelectionWrite {
        request_id: record.request_id,
        attempt_ordinal: record.attempt_ordinal,
        station_key_id: record.station_key_id,
        station_id: record.station_id,
        route_policy: record.route_policy,
        route_reason: record.route_reason,
        selected_at_ms: record.selected_at_ms,
    }
}

fn map_attempt_terminal(record: AttemptTerminalRecord) -> AttemptTerminalWrite {
    let (
        terminal_kind,
        failure_kind,
        failure_blame,
        retry_disposition,
        health_effect,
        health_update,
        public_code,
        sanitized_detail,
        durable_effect,
    ) = match record.terminal {
        AttemptTerminal::Succeeded => (
            "succeeded".to_string(),
            None,
            None,
            None,
            "success".to_string(),
            AttemptHealthUpdate::Success,
            None,
            None,
            None,
        ),
        AttemptTerminal::Failed(failure) => {
            let durable_effect = map_durable_effect(&failure.health);
            let health_update = match failure.health {
                HealthEffect::Success => AttemptHealthUpdate::Success,
                HealthEffect::ObserveFailure => AttemptHealthUpdate::ObserveFailure,
                HealthEffect::Cooldown { retry_after_ms } => {
                    AttemptHealthUpdate::Cooldown { retry_after_ms }
                }
                HealthEffect::HardFail => AttemptHealthUpdate::HardFail,
                HealthEffect::Neutral | HealthEffect::Capability(_) => AttemptHealthUpdate::Neutral,
            };
            (
                "failed".to_string(),
                Some(format!("{:?}", failure.kind)),
                Some(format!("{:?}", failure.blame)),
                Some(format!("{:?}", failure.retry)),
                format!("{:?}", failure.health),
                health_update,
                Some(failure.public_code),
                failure.sanitized_detail,
                durable_effect,
            )
        }
        AttemptTerminal::Abandoned { reason } => (
            "abandoned".to_string(),
            None,
            None,
            Some("StopRequest".to_string()),
            "neutral".to_string(),
            AttemptHealthUpdate::Neutral,
            Some(reason),
            None,
            None,
        ),
    };

    AttemptTerminalWrite {
        request_id: record.context.attempt_id.request_id,
        ordinal: record.context.attempt_id.ordinal,
        station_id: record.context.station_id,
        station_key_id: record.context.station_key_id,
        endpoint_revision: record.context.endpoint_revision,
        credential_revision: record.context.credential_revision,
        account_revision: record.context.account_revision,
        group_binding_id: record.context.group_binding_id,
        group_revision: record.context.group_revision,
        resolved_upstream_model: record.context.resolved_upstream_model,
        comparability_key: record.context.comparability_key,
        model_alias_revision: record.context.model_alias_revision,
        started_at_ms: record.context.started_at_ms,
        terminal_kind,
        failure_kind,
        failure_blame,
        retry_disposition,
        health_effect,
        health_cooldown_until_ms: None,
        health_update,
        durable_effect,
        public_code,
        sanitized_detail,
        output_committed: record.output_committed,
        event_at_ms: record.terminal_at_ms,
        observed_at_ms: record.terminal_at_ms,
        ingested_at_ms: record.terminal_at_ms,
        terminal_at_ms: record.terminal_at_ms,
    }
}

async fn apply_durable_attempt_effect(
    session: &mut crate::persistence::WriteSession,
    write: &AttemptTerminalWrite,
) -> Result<(), PersistenceError> {
    let Some(effect) = &write.durable_effect else {
        return Ok(());
    };
    let AttemptDurableEffectWrite::UnsupportedModel {
        station_key_id,
        model: _,
        evidence_code,
        classifier_profile_version,
    } = effect;
    let resolved_model = write
        .resolved_upstream_model
        .as_ref()
        .ok_or(PersistenceError::ConstraintViolation)?;
    RoutingHealthVerdictStore
        .apply_unsupported_model(
            session.connection(),
            &UnsupportedModelObservation {
                observation_id: format!("capability:{}:{}", write.request_id, write.ordinal),
                logical_request_id: write.request_id.clone(),
                attempt_ordinal: u8::try_from(write.ordinal)
                    .map_err(|_| PersistenceError::ConstraintViolation)?,
                station_key_id: station_key_id.clone(),
                resolved_model: resolved_model.clone(),
                credential_revision: write.credential_revision,
                endpoint_revision: write.endpoint_revision,
                model_alias_revision: write.model_alias_revision,
                endpoint_kind: "unknown".to_string(),
                protocol_kind: "unknown".to_string(),
                model_mapping_revision: None,
                model_resolution_fence: None,
                evidence_code: evidence_code.clone(),
                classifier_profile_version: classifier_profile_version.clone(),
            },
            write.terminal_at_ms.max(0),
        )
        .await?;
    Ok(())
}

fn map_durable_effect(effect: &HealthEffect) -> Option<AttemptDurableEffectWrite> {
    match effect {
        HealthEffect::Capability(DurableCapabilityEffect::ConfirmUnsupportedModel {
            station_key_id,
            model,
            evidence_code,
            classifier_profile_version,
        }) => Some(AttemptDurableEffectWrite::UnsupportedModel {
            station_key_id: station_key_id.clone(),
            model: model.clone(),
            evidence_code: evidence_code.clone(),
            classifier_profile_version: classifier_profile_version.clone(),
        }),
        _ => None,
    }
}

/// The request outcome summary predates the v3 attempt/circuit vocabulary.
/// Keep that durable compatibility table closed while preserving the richer
/// disposition in the v3 attempt and decision-event records.
fn legacy_routing_retry_disposition(value: &str) -> &'static str {
    if value.eq_ignore_ascii_case("try_next_key")
        || value.eq_ignore_ascii_case("try_next_candidate")
        || value.eq_ignore_ascii_case("retryable_before_commit")
        || value.eq_ignore_ascii_case("trynextcandidate")
        || value.eq_ignore_ascii_case("retrysametarget")
    {
        "same_target_exhausted"
    } else if value.eq_ignore_ascii_case("fail_closed") {
        "fail_closed"
    } else {
        "none"
    }
}

fn map_request_terminal(record: FinalRequestRecord, terminal_at_ms: i64) -> RequestTerminalWrite {
    let (
        status,
        lifecycle_status,
        terminal_kind,
        terminal_code,
        terminal_detail,
        protocol_completed,
    ) = match record.terminal.terminal {
        RequestTerminal::Completed(_) => (
            "success",
            "completed",
            "completed",
            Some("request_completed".to_string()),
            None,
            true,
        ),
        RequestTerminal::PartialSuccess(_) => (
            "success",
            "partial_success",
            "partial_success",
            Some("request_partial_success".to_string()),
            None,
            true,
        ),
        RequestTerminal::Failed(failure) => (
            "failed",
            "failed",
            "failed",
            Some(failure.code),
            failure.detail,
            false,
        ),
        RequestTerminal::Interrupted(failure) => (
            "interrupted",
            "interrupted",
            "interrupted",
            Some(failure.terminal.code().to_string()),
            failure
                .detail
                .or_else(|| Some("downstream disconnected".to_string())),
            false,
        ),
    };
    let routing_outcome = record.routing_outcome;
    let annotations = record.annotations;
    let usage_status = request_usage_status(
        &record.context.endpoint,
        annotations.stream,
        annotations.total_tokens,
    );

    RequestTerminalWrite {
        request_id: record.context.request_id,
        received_at_ms: record.context.received_at_ms,
        status: status.to_string(),
        lifecycle_status: lifecycle_status.to_string(),
        usage_status: usage_status.to_string(),
        terminal_kind: terminal_kind.to_string(),
        terminal_code: terminal_code.clone(),
        terminal_detail,
        protocol_completed,
        delivery_terminal: format!("{:?}", record.terminal.delivery),
        selected_attempt_ordinal: record.selected_attempt_id.map(|attempt| attempt.ordinal),
        attempt_count: record.attempt_count,
        fallback_count: record.fallback_count,
        terminal_at_ms,
        routing_outcome: RequestRoutingOutcomeSummaryWrite {
            terminal_kind: terminal_kind.to_string(),
            terminal_code: terminal_code.unwrap_or_else(|| "request_completed".to_string()),
            classification: routing_outcome.as_ref().map_or_else(
                || request_terminal_classification(terminal_kind).to_string(),
                |facts| facts.classification.clone(),
            ),
            confidence: routing_outcome.as_ref().map_or_else(
                || "not_applicable".to_string(),
                |facts| facts.confidence.clone(),
            ),
            evidence_source: routing_outcome
                .as_ref()
                .map_or_else(|| "none".to_string(), |facts| facts.evidence_source.clone()),
            request_accepted: routing_outcome.as_ref().map_or_else(
                || {
                    if protocol_completed {
                        "accepted"
                    } else {
                        "unknown"
                    }
                    .to_string()
                },
                |facts| facts.request_accepted.clone(),
            ),
            send_phase: routing_outcome.as_ref().map_or_else(
                || {
                    if protocol_completed {
                        "response_started"
                    } else {
                        "unknown"
                    }
                    .to_string()
                },
                |facts| facts.send_phase.clone(),
            ),
            replay_disposition: routing_outcome.as_ref().map_or_else(
                || {
                    if protocol_completed {
                        "completed"
                    } else {
                        "stopped_uncertain"
                    }
                    .to_string()
                },
                |facts| facts.replay_disposition.clone(),
            ),
            billing_state: routing_outcome.as_ref().map_or_else(
                || {
                    if protocol_completed {
                        "completed"
                    } else {
                        "possibly_billed"
                    }
                    .to_string()
                },
                |facts| facts.billing_state.clone(),
            ),
            retry_disposition: routing_outcome.as_ref().map_or_else(
                || {
                    if record.fallback_count > 0 {
                        "same_target_exhausted"
                    } else {
                        "none"
                    }
                    .to_string()
                },
                |facts| legacy_routing_retry_disposition(&facts.retry_disposition).to_string(),
            ),
            effect_summary: routing_outcome.as_ref().map_or_else(
                || "neutral".to_string(),
                |facts| facts.effect_summary.clone(),
            ),
            failure_domain_commitment_version: routing_outcome
                .as_ref()
                .and_then(|facts| facts.failure_domain_commitment_version),
            failure_domain_commitment_digest: routing_outcome
                .and_then(|facts| facts.failure_domain_commitment_digest),
            attempt_count: record.attempt_count,
            fallback_count: record.fallback_count,
            terminal_at_ms,
        },
        annotations: RequestLogAnnotationsWrite {
            model: annotations.model,
            stream: annotations.stream,
            http_status: annotations.http_status.map(i64::from),
            selected_station_key_id: annotations.selected_station_key_id,
            selected_station_id: annotations.selected_station_id,
            upstream_base_url: None,
            route_policy: annotations.route_policy,
            route_reason: annotations.route_reason,
            rejected_candidates_json: annotations.rejected_candidates_json,
            body_bytes: annotations.body_bytes,
            route_wait_ms: annotations.route_wait_ms,
            upstream_headers_ms: annotations.upstream_headers_ms,
            failure_source: annotations.failure_source,
            attempts_json: annotations.attempts_json,
            completion_source: annotations.completion_source,
            prompt_tokens: annotations.prompt_tokens,
            completion_tokens: annotations.completion_tokens,
            total_tokens: annotations.total_tokens,
            cache_creation_tokens: annotations.cache_creation_tokens,
            cache_read_tokens: annotations.cache_read_tokens,
            reasoning_effort: annotations.reasoning_effort,
            first_token_ms: annotations.first_token_ms,
            billing_mode: annotations.billing_mode,
        },
    }
}

fn request_terminal_classification(terminal_kind: &str) -> &'static str {
    match terminal_kind {
        "completed" | "partial_success" => "success",
        "interrupted" => "downstream",
        "failed" => "generic",
        _ => "local",
    }
}

fn request_usage_status(endpoint: &str, stream: bool, total_tokens: Option<i64>) -> &'static str {
    let endpoint = endpoint.to_ascii_lowercase();
    if endpoint.contains("models") || endpoint.contains("usage") || endpoint.contains("embeddings")
    {
        return "not_applicable";
    }
    if total_tokens.is_some() {
        "complete"
    } else if stream {
        "stream_usage_missing"
    } else {
        "missing_usage"
    }
}

fn map_attempt_cost(record: AttemptCostCommitRecord) -> AttemptCostWrite {
    AttemptCostWrite {
        request_id: record.request_id,
        ordinal: record.ordinal,
        pricing_context_id: record.pricing_context_id,
        pricing_basis: record.pricing_basis,
        pricing_status_label: record.pricing_status_label,
        usage_status: record.usage_status,
        input_tokens: record.input_tokens,
        output_tokens: record.output_tokens,
        total_tokens: record.total_tokens,
        cache_creation_tokens: record.cache_creation_tokens,
        cache_read_tokens: record.cache_read_tokens,
        cost_status: record.cost_status,
        currency: record.currency,
        total_cost_micro: record.total_cost_micro,
        created_at_ms: record.created_at_ms,
    }
}

fn map_request_cost_aggregate(
    record: RequestCostAggregateCommitRecord,
) -> RequestCostAggregateWrite {
    RequestCostAggregateWrite {
        request_id: record.request_id,
        status: record.status,
        totals_by_currency_json: record.totals_by_currency_json,
        compatibility_currency: record.compatibility_currency,
        compatibility_total_cost_micro: record.compatibility_total_cost_micro,
        incomplete_attempts_json: record.incomplete_attempts_json,
        written_at_ms: record.written_at_ms,
    }
}

fn map_persistence_error(error: PersistenceError) -> LifecycleWriteError {
    match error {
        PersistenceError::DatabaseBusy => LifecycleWriteError::DatabaseBusy,
        PersistenceError::CommitOutcomeUnknown => LifecycleWriteError::CommitOutcomeUnknown(
            "request lifecycle commit outcome is unknown".to_string(),
        ),
        other => LifecycleWriteError::Unavailable(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::request_lifecycle::{
        attempt::{
            AttemptContext, AttemptFailureKind, ClassifiedAttemptFailure, FailureBlame,
            RetryDisposition,
        },
        delivery::DeliveryTerminal,
        request::{
            AttemptId, DeliveryFailure, RequestContextSnapshot, RequestLogAnnotations,
            RequestRoutingOutcomeFacts, RequestTerminalSnapshot,
        },
    };
    use crate::persistence::runtime::PersistenceRuntime;
    use crate::persistence::stores::routing_attempt_store::{
        RoutingAttemptAdmission, RoutingAttemptStore, RoutingGenerationEligibility,
    };
    use sqlx::Row;

    #[test]
    fn deterministic_business_and_capability_rejections_do_not_trip_key_circuit() {
        assert!(!failure_counts_toward_key_circuit(Some(
            "upstream_insufficient_balance"
        )));
        assert!(!failure_counts_toward_key_circuit(Some(
            "upstream_model_unavailable"
        )));
        assert!(failure_counts_toward_key_circuit(Some(
            "upstream_rate_limited"
        )));
        assert!(failure_counts_toward_key_circuit(Some(
            "upstream_authentication_failed"
        )));
    }

    #[test]
    fn retryable_attempt_actions_use_the_v3_circuit_disposition() {
        assert_eq!(
            circuit_retry_disposition(Some("RetrySameTarget"), false),
            "retryable_before_commit"
        );
        assert_eq!(
            circuit_retry_disposition(Some("TryNextCandidate"), false),
            "retryable_before_commit"
        );
        assert_eq!(circuit_retry_disposition(None, false), "stop_request");
        assert_eq!(circuit_retry_disposition(None, true), "end");
    }

    fn context(request_id: &str) -> RequestContextSnapshot {
        RequestContextSnapshot {
            request_id: request_id.to_string(),
            method: "POST".to_string(),
            local_path: "/v1/responses".to_string(),
            endpoint: "responses".to_string(),
            received_at_ms: 1_000,
        }
    }

    fn successful_attempt_record(request_id: &str, station_key_id: &str) -> AttemptTerminalRecord {
        AttemptTerminalRecord {
            context: AttemptContext {
                attempt_id: AttemptId::new(request_id, 0),
                station_id: "station-finalization".to_string(),
                station_key_id: station_key_id.to_string(),
                endpoint_revision: 1,
                credential_revision: 1,
                account_revision: 1,
                group_binding_id: None,
                group_revision: None,
                resolved_upstream_model: Some("gpt-test".to_string()),
                comparability_key: Some("fixture-comparability".to_string()),
                model_alias_revision: 1,
                started_at_ms: 1_000,
            },
            terminal: AttemptTerminal::Succeeded,
            output_committed: true,
            terminal_at_ms: 1_100,
        }
    }

    #[tokio::test]
    async fn successful_attempt_persistence_failure_gates_key_and_retains_bounded_replay() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("finalization.sqlite3"))
            .await
            .expect("runtime");
        let gate = CircuitPersistenceGate::shared();
        let service = RequestFinalizationService::new_with_circuit_persistence_gate(
            runtime.handle(),
            Arc::clone(&gate),
        );
        runtime.close().await.expect("close runtime");

        let result = RequestLifecycleStore::finish_attempt(
            &service,
            successful_attempt_record("req-persistence-failure", "key-persistence-failure"),
        )
        .await;

        assert!(result.is_err());
        assert!(gate.is_active("key-persistence-failure", 1));
        let backlog = service
            .circuit_persistence_backlog
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(backlog.records.len(), 1);
        assert_eq!(backlog.overflow_count, 0);
    }

    #[tokio::test]
    async fn attempt_terminal_writes_v3_circuit_without_legacy_health_side_effects() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("v3-only.sqlite3"))
            .await
            .expect("runtime");
        let service = RequestFinalizationService::new(runtime.handle());
        service
            .start_request(RequestStartRecord {
                context: context("req-v3-only"),
            })
            .await
            .expect("start request");

        let ack = service
            .finish_attempt(successful_attempt_record("req-v3-only", "key-v3-only"))
            .await
            .expect("finish attempt");
        assert!(ack.inserted);

        let mut read = runtime.handle().begin_read().await.expect("read");
        for table in ["station_key_health_observations", "routing_health_snapshot"] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(read.connection())
                .await
                .expect("legacy row count");
            assert_eq!(count, 0, "{table} must remain unchanged");
        }
        let circuit_events: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM routing_circuit_event_v3")
                .fetch_one(read.connection())
                .await
                .expect("circuit events");
        assert_eq!(circuit_events, 1);
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn unsupported_model_keeps_capability_persistence_without_scoped_health_write() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("capability.sqlite3"))
            .await
            .expect("runtime");
        let service = RequestFinalizationService::new(runtime.handle());
        service
            .start_request(RequestStartRecord {
                context: context("req-capability"),
            })
            .await
            .expect("start request");
        let mut record = successful_attempt_record("req-capability", "key-capability");
        record.terminal = AttemptTerminal::Failed(ClassifiedAttemptFailure {
            kind: AttemptFailureKind::CapabilityMismatch,
            blame: FailureBlame::Upstream,
            retry: RetryDisposition::TryNextCandidate,
            health: HealthEffect::Capability(DurableCapabilityEffect::ConfirmUnsupportedModel {
                station_key_id: "key-capability".to_string(),
                model: "gpt-test".to_string(),
                evidence_code: "upstream_model_unavailable".to_string(),
                classifier_profile_version: "capability-test-v1".to_string(),
            }),
            public_code: "upstream_model_unavailable".to_string(),
            sanitized_detail: Some("model is unavailable".to_string()),
        });

        service
            .finish_attempt(record)
            .await
            .expect("finish attempt");

        let mut read = runtime.handle().begin_read().await.expect("read");
        for table in [
            "routing_capability_model_observations",
            "routing_capability_model_verdicts",
        ] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(read.connection())
                .await
                .expect("capability row count");
            assert_eq!(count, 1, "{table} must retain the capability effect");
        }
        let scoped_health_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM routing_health_observations")
                .fetch_one(read.connection())
                .await
                .expect("scoped health row count");
        assert_eq!(scoped_health_count, 0);
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[test]
    fn request_start_mapping_preserves_context_and_early_annotations() {
        let annotations = RequestLogAnnotations {
            model: Some("gpt-test".to_string()),
            stream: true,
            reasoning_effort: Some("high".to_string()),
            ..RequestLogAnnotations::default()
        };
        let write = map_request_start(
            RequestStartRecord {
                context: context("req-start"),
            },
            annotations,
        );

        assert_eq!(write.request_id, "req-start");
        assert_eq!(write.method, "POST");
        assert_eq!(write.local_path, "/v1/responses");
        assert_eq!(write.endpoint, "responses");
        assert_eq!(write.received_at_ms, 1_000);
        assert_eq!(write.model.as_deref(), Some("gpt-test"));
        assert!(write.stream);
        assert_eq!(write.reasoning_effort.as_deref(), Some("high"));
    }

    #[test]
    fn attempt_mapping_preserves_failure_and_health_fields() {
        let write = map_attempt_terminal(AttemptTerminalRecord {
            context: AttemptContext {
                attempt_id: AttemptId::new("req-attempt", 7),
                station_id: "station-1".to_string(),
                station_key_id: "key-1".to_string(),
                endpoint_revision: 4,
                credential_revision: 1,
                account_revision: 1,
                group_binding_id: None,
                group_revision: None,
                resolved_upstream_model: None,
                comparability_key: None,
                model_alias_revision: 1,
                started_at_ms: 1_010,
            },
            terminal: AttemptTerminal::Failed(ClassifiedAttemptFailure {
                kind: AttemptFailureKind::RateLimit,
                blame: FailureBlame::Upstream,
                retry: RetryDisposition::TryNextCandidate,
                health: HealthEffect::Cooldown {
                    retry_after_ms: Some(30_000),
                },
                public_code: "upstream_rate_limited".to_string(),
                sanitized_detail: Some("retry later".to_string()),
            }),
            output_committed: false,
            terminal_at_ms: 1_100,
        });

        assert_eq!(write.request_id, "req-attempt");
        assert_eq!(write.ordinal, 7);
        assert_eq!(write.station_id, "station-1");
        assert_eq!(write.station_key_id, "key-1");
        assert_eq!(write.endpoint_revision, 4);
        assert_eq!(write.started_at_ms, 1_010);
        assert_eq!(write.terminal_kind, "failed");
        assert_eq!(write.failure_kind.as_deref(), Some("RateLimit"));
        assert_eq!(write.failure_blame.as_deref(), Some("Upstream"));
        assert_eq!(write.retry_disposition.as_deref(), Some("TryNextCandidate"));
        assert_eq!(
            write.health_effect,
            "Cooldown { retry_after_ms: Some(30000) }"
        );
        assert_eq!(
            write.health_update,
            AttemptHealthUpdate::Cooldown {
                retry_after_ms: Some(30_000)
            }
        );
        assert_eq!(write.public_code.as_deref(), Some("upstream_rate_limited"));
        assert_eq!(write.sanitized_detail.as_deref(), Some("retry later"));
        assert!(!write.output_committed);
        assert_eq!(write.terminal_at_ms, 1_100);
    }

    #[test]
    fn request_terminal_mapping_preserves_safe_annotations_and_redacts_upstream_base_url() {
        let annotations = RequestLogAnnotations {
            model: Some("gpt-test".to_string()),
            stream: true,
            http_status: Some(200),
            selected_station_key_id: Some("key-1".to_string()),
            selected_station_id: Some("station-1".to_string()),
            upstream_base_url: Some("https://station.test/v1".to_string()),
            route_policy: Some("stable_first".to_string()),
            route_reason: Some("healthy key".to_string()),
            rejected_candidates_json: Some("[]".to_string()),
            body_bytes: Some(128),
            route_wait_ms: Some(3),
            upstream_headers_ms: Some(7),
            failure_source: Some("downstream".to_string()),
            attempts_json: Some("[]".to_string()),
            completion_source: Some("response.completed".to_string()),
            prompt_tokens: Some(11),
            completion_tokens: Some(13),
            total_tokens: Some(24),
            cache_creation_tokens: Some(2),
            cache_read_tokens: Some(5),
            reasoning_effort: Some("high".to_string()),
            first_token_ms: Some(17),
            billing_mode: Some("token".to_string()),
        };
        let write = map_request_terminal(
            FinalRequestRecord::new(
                context("req-terminal"),
                RequestTerminalSnapshot {
                    terminal: RequestTerminal::Interrupted(DeliveryFailure {
                        terminal: DeliveryTerminal::DownstreamWriteFailed,
                        detail: None,
                    }),
                    delivery: DeliveryTerminal::DownstreamWriteFailed,
                },
                Some(AttemptId::new("req-terminal", 2)),
                3,
                2,
                annotations,
            ),
            1_250,
        );

        assert_eq!(write.request_id, "req-terminal");
        assert_eq!(write.received_at_ms, 1_000);
        assert_eq!(write.status, "interrupted");
        assert_eq!(write.lifecycle_status, "interrupted");
        assert_eq!(write.terminal_kind, "interrupted");
        assert_eq!(
            write.terminal_code.as_deref(),
            Some("downstream_write_failed")
        );
        assert_eq!(
            write.terminal_detail.as_deref(),
            Some("downstream disconnected")
        );
        assert!(!write.protocol_completed);
        assert_eq!(write.delivery_terminal, "DownstreamWriteFailed");
        assert_eq!(write.selected_attempt_ordinal, Some(2));
        assert_eq!(write.attempt_count, 3);
        assert_eq!(write.fallback_count, 2);
        assert_eq!(write.terminal_at_ms, 1_250);
        assert_eq!(write.annotations.model.as_deref(), Some("gpt-test"));
        assert!(write.annotations.stream);
        assert_eq!(write.annotations.http_status, Some(200));
        assert_eq!(
            write.annotations.selected_station_key_id.as_deref(),
            Some("key-1")
        );
        assert_eq!(write.annotations.total_tokens, Some(24));
        assert_eq!(write.annotations.first_token_ms, Some(17));
        assert_eq!(write.annotations.billing_mode.as_deref(), Some("token"));
        assert_eq!(write.annotations.upstream_base_url, None);
    }

    #[test]
    fn request_terminal_mapping_prefers_typed_canonical_outcome_over_terminal_inference() {
        let digest = "a".repeat(64);
        let write = map_request_terminal(
            FinalRequestRecord::new(
                context("req-canonical"),
                RequestTerminalSnapshot {
                    terminal: RequestTerminal::Failed(
                        crate::application::request_lifecycle::request::RequestFailure {
                            code: "server_error".to_string(),
                            detail: None,
                        },
                    ),
                    delivery: DeliveryTerminal::NotStarted,
                },
                Some(AttemptId::new("req-canonical", 0)),
                1,
                0,
                RequestLogAnnotations::default(),
            )
            .with_routing_outcome(RequestRoutingOutcomeFacts {
                classification: "capacity".to_string(),
                confidence: "confirmed".to_string(),
                evidence_source: "error_envelope".to_string(),
                request_accepted: "not_accepted".to_string(),
                send_phase: "response_started".to_string(),
                replay_disposition: "stopped_uncertain".to_string(),
                billing_state: "possibly_billed".to_string(),
                retry_disposition: "same_target_exhausted".to_string(),
                effect_summary: "neutral".to_string(),
                failure_domain_commitment_version: Some(1),
                failure_domain_commitment_digest: Some(digest.clone()),
            }),
            1_250,
        );

        assert_eq!(write.routing_outcome.classification, "capacity");
        assert_eq!(write.routing_outcome.confidence, "confirmed");
        assert_eq!(write.routing_outcome.evidence_source, "error_envelope");
        assert_eq!(write.routing_outcome.send_phase, "response_started");
        assert_eq!(
            write
                .routing_outcome
                .failure_domain_commitment_digest
                .as_deref(),
            Some(digest.as_str())
        );
    }

    #[tokio::test]
    async fn startup_reconciliation_terminalizes_v3_attempt_and_persists_quality_and_circuit() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("startup-v3.sqlite3"))
            .await
            .expect("initialize runtime");
        let service = RequestFinalizationService::new(runtime.handle());
        service
            .start_request(RequestStartRecord {
                context: context("startup-v3-request"),
            })
            .await
            .expect("start durable request");

        let mut write = runtime
            .begin_write()
            .await
            .expect("admit interrupted attempt");
        RoutingAttemptStore::admit(
            write.connection(),
            &RoutingAttemptAdmission {
                attempt_id: "startup-v3-request:0",
                correlation_id: "startup-v3-request",
                station_key_id: "startup-v3-key",
                station_key_lifecycle_revision: 1,
                attempt_index: 0,
                capacity_lease_id: "startup-capacity-lease",
                half_open_lease_id: None,
                lease_revision: None,
                deadline_at_ms: 10_000,
                admitted_at_ms: 1_100,
                generation_eligibility: RoutingGenerationEligibility::Next,
            },
        )
        .await
        .expect("admit v3 attempt");
        RoutingAttemptStore::mark_boundary_crossed(
            write.connection(),
            "startup-v3-request:0",
            "startup-v3-key",
            1,
            1_200,
        )
        .await
        .expect("mark outbound boundary");
        write.commit().await.expect("commit interrupted attempt");

        service
            .reconcile_startup_interrupted_request_lifecycle()
            .await
            .expect("reconcile interrupted v3 attempt");

        let mut read = runtime.begin_read().await.expect("read recovered state");
        let attempt = sqlx::query(
            "SELECT terminal_state, outcome, failure_attribution, response_origin,
                    recovery_origin, retry_disposition
             FROM routing_attempt_v3 WHERE attempt_id = 'startup-v3-request:0'",
        )
        .fetch_one(read.connection())
        .await
        .expect("recovered attempt");
        assert_eq!(
            attempt.get::<String, _>("terminal_state"),
            "upstream_uncertain"
        );
        assert_eq!(attempt.get::<String, _>("outcome"), "attributable_failure");
        assert_eq!(attempt.get::<String, _>("failure_attribution"), "key");
        assert_eq!(attempt.get::<String, _>("response_origin"), "unknown");
        assert_eq!(
            attempt.get::<String, _>("recovery_origin"),
            "crash_recovery"
        );
        assert_eq!(
            attempt.get::<String, _>("retry_disposition"),
            "stop_request"
        );

        let circuit = sqlx::query(
            "SELECT canonical_outcome, failure_code, recovery_origin
             FROM routing_circuit_event_v3
             WHERE attempt_id = 'startup-v3-request:0' AND effect_kind = 'circuit'",
        )
        .fetch_one(read.connection())
        .await
        .expect("recovered circuit event");
        assert_eq!(
            circuit.get::<String, _>("canonical_outcome"),
            "attributable_failure"
        );
        assert_eq!(
            circuit.get::<String, _>("failure_code"),
            "startup_interrupted"
        );
        assert_eq!(
            circuit.get::<String, _>("recovery_origin"),
            "crash_recovery"
        );

        let observation = sqlx::query(
            "SELECT outcome, failure_code, failure_attribution, response_origin,
                    recovery_origin, retry_disposition, generation_eligibility,
                    cluster_finalized, cluster_expected_attempt_count
             FROM routing_observations
             WHERE correlation_id = 'startup-v3-request'",
        )
        .fetch_one(read.connection())
        .await
        .expect("recovered quality observation");
        assert_eq!(
            observation.get::<String, _>("outcome"),
            "attributable_failure"
        );
        assert_eq!(observation.get::<String, _>("failure_attribution"), "key");
        assert_eq!(
            observation
                .get::<Option<String>, _>("failure_code")
                .as_deref(),
            Some("startup_interrupted")
        );
        assert_eq!(observation.get::<String, _>("response_origin"), "unknown");
        assert_eq!(
            observation.get::<String, _>("recovery_origin"),
            "crash_recovery"
        );
        assert_eq!(
            observation.get::<String, _>("retry_disposition"),
            "stop_request"
        );
        assert_eq!(
            observation.get::<String, _>("generation_eligibility"),
            "next"
        );
        assert_eq!(observation.get::<i64, _>("cluster_finalized"), 1);
        assert_eq!(
            observation.get::<i64, _>("cluster_expected_attempt_count"),
            1
        );
        drop(read);
        drop(service);
        runtime.close().await.expect("close runtime");

        let restarted = PersistenceRuntime::open_current(&root.path().join("startup-v3.sqlite3"))
            .await
            .expect("reopen recovered runtime");
        let mut read = restarted.begin_read().await.expect("read restarted state");
        let observations =
            crate::persistence::stores::routing_observation_store::RoutingObservationStore
                .list_v3_generation_key_cursor(
                    read.connection(),
                    "startup-v3-key",
                    10_000,
                    10_000,
                    None,
                    8,
                )
                .await
                .expect("reload recovered observation through v3 cursor");
        assert_eq!(observations.len(), 1);
        let observation = &observations[0];
        assert_eq!(observation.response_origin, ResponseOrigin::Unknown);
        assert_eq!(observation.failure_attribution, FailureAttribution::Key);
        assert_eq!(
            observation.failure_code.as_deref(),
            Some("startup_interrupted")
        );
        assert_eq!(observation.recovery_origin, RecoveryOrigin::CrashRecovery);
        assert_eq!(
            observation.retry_disposition,
            ObservationRetryDisposition::StopRequest
        );
        drop(read);
        restarted.close().await.expect("close restarted runtime");
    }

    #[test]
    fn unknown_commit_outcome_remains_distinguishable_at_the_lifecycle_port() {
        let error = map_persistence_error(PersistenceError::CommitOutcomeUnknown);

        assert!(matches!(
            error,
            LifecycleWriteError::CommitOutcomeUnknown(_)
        ));
    }
}
