use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

pub(crate) mod effect_planner;
pub(crate) mod failure;
pub(crate) mod outcome;
pub(crate) mod outcome_orchestrator;

use futures_util::future::BoxFuture;

use crate::{
    application::request_lifecycle::{
        attempt::{
            AttemptTerminal, AttemptTerminalRecord, DurableCapabilityEffect,
            DurableFailureDimension, DurableHealthScope, DurableVerdict, HealthEffect,
        },
        ports::{
            AttemptCommitAck, AttemptCostCommitAck, AttemptCostCommitRecord, LifecycleWriteError,
            RequestCommitAck, RequestCostAggregateCommitAck, RequestCostAggregateCommitRecord,
            RequestLifecycleStore, RequestStartAck,
        },
        request::{FinalRequestRecord, RequestStartRecord, RequestTerminal},
    },
    application::{
        clock::{Clock, SystemClock},
        error_rate_protection::ErrorRateProtectionService,
        health_protection::HealthProtectionScope,
        health_transitions::HealthTransitionService,
        observation_ingestion::ObservationIngestion,
    },
    models::health::{
        HealthObservation, HealthObservationOutcome, HealthObservationSource, HealthWritebackMode,
        TrafficEquivalence,
    },
    models::routing_observation::{
        ObservationOrder, ObservationOutcome, ObservationScope, ObservationSource,
        RoutingObservation,
    },
    persistence::{
        error::PersistenceError,
        runtime::PersistenceHandle,
        stores::request_lifecycle_reconciliation::{
            default_startup_reconciliation_batch_size, reconcile_startup_interrupted_batch,
            StartupReconciliationReport,
        },
        stores::request_log_store::{
            AttemptPersistenceResult, RequestLogStore, RequestStartPersistenceResult,
        },
        stores::request_log_write::{
            AttemptDurableEffectWrite, AttemptHealthUpdate, AttemptTerminalWrite,
            RequestLogAnnotationsWrite, RequestRoutingOutcomeSummaryWrite, RequestStartWrite,
            RequestTerminalWrite,
        },
        stores::request_outcome_store::{
            AttemptCostWrite, RequestCostAggregateWrite, RequestOutcomeStore,
        },
        stores::request_terminal_outbox::RequestTerminalOutboxStore,
        stores::routing_health_verdict_store::{
            DurableHealthVerdict, FailureDimension, RoutingHealthVerdictStore,
            ScopedHealthObservation, ScopedHealthSubject, UnsupportedModelObservation,
        },
    },
};

#[derive(Clone)]
pub(crate) struct RequestFinalizationService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    health: HealthTransitionService,
    observations: ObservationIngestion,
    observation_sequence: Arc<AtomicU64>,
}

const TERMINAL_OUTBOX_BATCH_SIZE: u32 = 64;
const TERMINAL_OUTBOX_LEASE_MS: i64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalOutboxReconciliationReport {
    pub(crate) batches_completed: u64,
    pub(crate) terminals_projected: u64,
}

impl RequestFinalizationService {
    #[expect(
        dead_code,
        reason = "contract=request-finalization.test-constructor; owner=application/request_finalization; remove_when=all test fixtures compose the explicit error-rate adapter"
    )]
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self::new_with_error_rate(runtime, ErrorRateProtectionService::disabled())
    }

    pub(crate) fn new_with_error_rate(
        runtime: PersistenceHandle,
        error_rate: ErrorRateProtectionService,
    ) -> Self {
        Self {
            runtime,
            clock: Arc::new(SystemClock),
            health: HealthTransitionService::new(),
            observations: ObservationIngestion::with_error_rate(error_rate),
            observation_sequence: Arc::new(AtomicU64::new(1)),
        }
    }

    pub(crate) async fn reconcile_startup_interrupted_request_lifecycle(
        &self,
    ) -> Result<StartupReconciliationReport, LifecycleWriteError> {
        self.ensure_scoped_health_projection().await?;
        let mut total = StartupReconciliationReport::empty();
        loop {
            let now_ms = self.clock.now_utc().timestamp_millis();
            let mut session = self
                .runtime
                .begin_write()
                .await
                .map_err(map_persistence_error)?;
            let batch = reconcile_startup_interrupted_batch(
                session.connection(),
                now_ms,
                default_startup_reconciliation_batch_size(),
            )
            .await
            .map_err(map_persistence_error)?;
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

    async fn ensure_scoped_health_projection(&self) -> Result<(), LifecycleWriteError> {
        let now_ms = self.clock.now_utc().timestamp_millis();
        let mut session = self
            .runtime
            .begin_write()
            .await
            .map_err(map_persistence_error)?;
        RoutingHealthVerdictStore
            .ensure_current_projection(session.connection(), now_ms)
            .await
            .map_err(map_persistence_error)?;
        RoutingHealthVerdictStore
            .ensure_health_protection_state(session.connection(), now_ms)
            .await
            .map_err(map_persistence_error)?;
        session.commit().await.map_err(map_persistence_error)
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
        let runtime = self.runtime.clone();
        let health = self.health;
        let observations = self.observations.clone();
        let observation_sequence = Arc::clone(&self.observation_sequence);
        let write = map_attempt_terminal(record);
        Box::pin(async move {
            let mut session = runtime.begin_write().await.map_err(map_persistence_error)?;
            let outcome: AttemptPersistenceResult = RequestLogStore
                .finish_attempt(&mut session, &write)
                .await
                .map_err(map_persistence_error)?;
            let mut health_applied = false;
            if outcome.inserted {
                let is_probe_outcome = matches!(
                    write.health_update,
                    AttemptHealthUpdate::ProbeSuccess | AttemptHealthUpdate::ProbeFailure { .. }
                );
                // Probe observations must consume the Half-Open fence before
                // any scoped verdict write. The latter intentionally updates
                // the same durable reducer and would otherwise invalidate the
                // probe as stale inside this transaction.
                if !is_probe_outcome {
                    apply_durable_attempt_effect(&mut session, &write)
                        .await
                        .map_err(map_persistence_error)?;
                    if let Some(observation) = attempt_health_observation(&write) {
                        health_applied = health
                            .record_observation(&mut session, observation)
                            .await
                            .map_err(map_persistence_error)?
                            .health_applied;
                    } else if matches!(write.health_update, AttemptHealthUpdate::Neutral) {
                        if let (Some(probe_state_revision), Some(scope)) =
                            (write.probe_state_revision, write.probe_scope.clone())
                        {
                            crate::persistence::stores::routing_health_verdict_store::RoutingHealthVerdictStore
                                .cancel_health_protection_probe(
                                    session.connection(),
                                    &crate::application::health_protection::HealthProtectionProbe {
                                        scope,
                                        state_revision: probe_state_revision,
                                    },
                                    write.terminal_at_ms.max(0),
                                )
                                .await
                                .map_err(map_persistence_error)?;
                        }
                    }
                }
                if let Some(observation) = routing_observation(
                    &write,
                    observation_sequence.fetch_add(1, Ordering::Relaxed),
                ) {
                    observations
                        .append(&mut session, observation)
                        .await
                        .map_err(map_persistence_error)?;
                }
                if is_probe_outcome {
                    if matches!(write.health_update, AttemptHealthUpdate::ProbeSuccess) {
                        if let Some(scope) = write.probe_scope.clone() {
                            apply_probe_recovery(&mut session, &write, &scope)
                                .await
                                .map_err(map_persistence_error)?;
                        }
                    } else {
                        apply_durable_attempt_effect(&mut session, &write)
                            .await
                            .map_err(map_persistence_error)?;
                    }
                }
            }
            session.commit().await.map_err(map_persistence_error)?;
            Ok(AttemptCommitAck {
                inserted: outcome.inserted,
                health_applied,
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

fn attempt_health_observation(record: &AttemptTerminalWrite) -> Option<HealthObservation> {
    let outcome = match record.health_update {
        AttemptHealthUpdate::Success => HealthObservationOutcome::Success,
        AttemptHealthUpdate::ProbeSuccess | AttemptHealthUpdate::ProbeFailure { .. } => {
            return None
        }
        AttemptHealthUpdate::ObserveFailure => HealthObservationOutcome::ObserveFailure,
        AttemptHealthUpdate::Cooldown { .. } => HealthObservationOutcome::Cooldown,
        AttemptHealthUpdate::HardFail => HealthObservationOutcome::HardFail,
        AttemptHealthUpdate::Neutral => return None,
    };
    let source_event_id = format!("proxy:{}:{}", record.request_id, record.ordinal);
    Some(HealthObservation {
        id: format!("health-observation-{source_event_id}"),
        station_key_id: record.station_key_id.clone(),
        target_result_id: None,
        source: HealthObservationSource::ProxyRequest,
        source_event_id,
        observed_at_ms: record.terminal_at_ms,
        endpoint_revision: record.endpoint_revision,
        outcome,
        failure_kind: record.failure_kind.clone(),
        latency_ms: Some(record.terminal_at_ms.saturating_sub(record.started_at_ms)),
        retry_after_ms: match record.health_update {
            AttemptHealthUpdate::Cooldown { retry_after_ms } => retry_after_ms,
            _ => None,
        },
        error_summary: record.sanitized_detail.clone(),
        writeback_mode: HealthWritebackMode::Authoritative,
        traffic_equivalence: TrafficEquivalence::RealUserTraffic,
    })
}

fn routing_observation(
    record: &AttemptTerminalWrite,
    producer_sequence: u64,
) -> Option<RoutingObservation> {
    let outcome = match record.health_update {
        AttemptHealthUpdate::Success | AttemptHealthUpdate::ProbeSuccess => {
            ObservationOutcome::Success
        }
        AttemptHealthUpdate::ObserveFailure => ObservationOutcome::EndpointFailure,
        AttemptHealthUpdate::Cooldown { .. } => ObservationOutcome::RateLimited,
        AttemptHealthUpdate::ProbeFailure { .. } => ObservationOutcome::EndpointFailure,
        AttemptHealthUpdate::HardFail => ObservationOutcome::EndpointFailure,
        AttemptHealthUpdate::Neutral => return None,
    };
    let event_at_ms = record.terminal_at_ms.max(0);
    Some(RoutingObservation {
        id: format!(
            "routing-observation-{}-{}",
            record.request_id, record.ordinal
        ),
        order: ObservationOrder {
            producer_id: "request_finalization".to_string(),
            producer_sequence,
            event_at_ms,
            ingested_at_ms: event_at_ms,
        },
        scope: ObservationScope {
            station_id: Some(record.station_id.clone()),
            station_key_id: Some(record.station_key_id.clone()),
            model: None,
            endpoint_revision: Some(record.endpoint_revision),
        },
        source: ObservationSource::RealRequest,
        traffic_equivalence: crate::models::routing_observation::TrafficEquivalence::ExactRequest,
        outcome,
        latency_ms: u32::try_from(record.terminal_at_ms.saturating_sub(record.started_at_ms)).ok(),
        evidence_mass_basis_points: 10_000,
        probe_state_revision: record.probe_state_revision,
        probe_scope: record.probe_scope.clone(),
    })
}

/// A successful real-request probe is also a recovery for the explicit
/// durable scoped verdict that admitted it.  The error-rate reducer owns the
/// Half-Open transition through `RoutingObservation`; this helper clears the
/// separate scoped-verdict projection without ever guessing an identity from
/// the opaque commitment.
async fn apply_probe_recovery(
    session: &mut crate::persistence::WriteSession,
    write: &AttemptTerminalWrite,
    scope: &crate::application::health_protection::HealthProtectionScope,
) -> Result<(), PersistenceError> {
    use crate::application::health_protection::HealthProtectionScopeKind;

    let (subject, dimension, expected_scope) = match scope.kind {
        HealthProtectionScopeKind::Credential => {
            let subject = ScopedHealthSubject::credential(
                write.station_id.clone(),
                write.station_key_id.clone(),
                write.credential_revision,
            )?;
            // Durable credential verdicts include the station and credential
            // revision. The error-rate adapter also has a coarser key-only
            // credential scope; a probe leased against that scope is already
            // recovered by observation ingestion and must not write a fake
            // recovery row for the revisioned durable subject.
            let expected = HealthProtectionScope::new(
                HealthProtectionScopeKind::Credential,
                subject.scope().to_string(),
            )
            .map_err(|_| PersistenceError::ConstraintViolation)?;
            (subject, FailureDimension::Credential, expected)
        }
        HealthProtectionScopeKind::Endpoint => {
            let subject =
                ScopedHealthSubject::endpoint(write.station_id.clone(), write.endpoint_revision)?;
            let expected = crate::application::error_rate_protection::endpoint_health_scope(
                &write.station_id,
                write.endpoint_revision,
            )
            .ok_or(PersistenceError::ConstraintViolation)?;
            (subject, FailureDimension::EndpointAvailability, expected)
        }
        // These scopes are not currently leased by the request planner. Keep
        // the match explicit so adding a new lease type cannot silently clear
        // a verdict with insufficient identity fields.
        HealthProtectionScopeKind::Account
        | HealthProtectionScopeKind::Group
        | HealthProtectionScopeKind::Model
        | HealthProtectionScopeKind::CapacityDomain => return Ok(()),
    };
    if expected_scope != *scope {
        if scope.kind == HealthProtectionScopeKind::Credential {
            return Ok(());
        }
        return Err(PersistenceError::InvariantViolation(
            "probe scope does not match terminal identity".into(),
        ));
    }
    RoutingHealthVerdictStore
        .apply_observation(
            session.connection(),
            &ScopedHealthObservation {
                observation_id: format!(
                    "scoped-health-probe-recovery:{}:{}:{}",
                    write.request_id,
                    write.ordinal,
                    dimension.as_str()
                ),
                producer_id: format!("request-finalization-probe:{}", write.request_id),
                producer_sequence: u64::from(write.ordinal),
                logical_request_id: write.request_id.clone(),
                attempt_ordinal: u8::try_from(write.ordinal)
                    .map_err(|_| PersistenceError::ConstraintViolation)?,
                terminal_kind: "probe_recovery".to_string(),
                subject,
                dimension,
                verdict: None,
                cooldown_until_ms: None,
                evidence_code: "probe_success_recovery".to_string(),
                classifier_profile_version: "probe_recovery_v1".to_string(),
            },
            write.terminal_at_ms.max(0),
        )
        .await?;
    Ok(())
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
                HealthEffect::Success if record.probe_scope.is_some() => {
                    AttemptHealthUpdate::ProbeSuccess
                }
                HealthEffect::Success => AttemptHealthUpdate::Success,
                HealthEffect::ObserveFailure if record.probe_scope.is_some() => {
                    AttemptHealthUpdate::ProbeFailure {
                        retry_after_ms: None,
                    }
                }
                HealthEffect::ObserveFailure => AttemptHealthUpdate::ObserveFailure,
                HealthEffect::Cooldown { retry_after_ms } => {
                    if record.probe_scope.is_some() {
                        AttemptHealthUpdate::ProbeFailure { retry_after_ms }
                    } else {
                        AttemptHealthUpdate::Cooldown { retry_after_ms }
                    }
                }
                HealthEffect::HardFail if record.probe_scope.is_some() => {
                    AttemptHealthUpdate::ProbeFailure {
                        retry_after_ms: None,
                    }
                }
                HealthEffect::HardFail => AttemptHealthUpdate::HardFail,
                HealthEffect::Neutral | HealthEffect::Scoped(_) | HealthEffect::Capability(_) => {
                    if record.probe_scope.is_some() {
                        AttemptHealthUpdate::ProbeFailure {
                            retry_after_ms: None,
                        }
                    } else {
                        AttemptHealthUpdate::Neutral
                    }
                }
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
        terminal_at_ms: record.terminal_at_ms,
        probe_state_revision: record.probe_state_revision,
        probe_scope: record.probe_scope.clone(),
    }
}

async fn apply_durable_attempt_effect(
    session: &mut crate::persistence::WriteSession,
    write: &AttemptTerminalWrite,
) -> Result<(), PersistenceError> {
    let Some(effect) = &write.durable_effect else {
        return Ok(());
    };
    match effect {
        AttemptDurableEffectWrite::UnsupportedModel {
            station_key_id,
            model: _,
            evidence_code,
            classifier_profile_version,
        } => {
            let resolved_model = write
                .resolved_upstream_model
                .as_ref()
                .ok_or(PersistenceError::ConstraintViolation)?;
            RoutingHealthVerdictStore
                .apply_unsupported_model(
                    session.connection(),
                    &UnsupportedModelObservation {
                        observation_id: format!(
                            "capability:{}:{}",
                            write.request_id, write.ordinal
                        ),
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
                        // Mapping revision is provenance, never native capability
                        // identity. The legacy column remains nullable for old
                        // records, while v2 facts are keyed by native model and
                        // execution revisions only.
                        model_mapping_revision: None,
                        model_resolution_fence: None,
                        evidence_code: evidence_code.clone(),
                        classifier_profile_version: classifier_profile_version.clone(),
                    },
                    write.terminal_at_ms.max(0),
                )
                .await?;
        }
        _ => {
            let (subject, dimension, verdict, retry_after_ms, evidence_code, profile) = match effect
            {
                AttemptDurableEffectWrite::Credential {
                    station_key_id,
                    dimension,
                    verdict,
                    retry_after_ms,
                    evidence_code,
                    classifier_profile_version,
                } => (
                    ScopedHealthSubject::credential(
                        &write.station_id,
                        station_key_id,
                        write.credential_revision,
                    )?,
                    dimension,
                    verdict,
                    retry_after_ms,
                    evidence_code,
                    classifier_profile_version,
                ),
                AttemptDurableEffectWrite::Account {
                    station_id,
                    dimension,
                    verdict,
                    retry_after_ms,
                    evidence_code,
                    classifier_profile_version,
                } => (
                    ScopedHealthSubject::account(station_id, write.account_revision)?,
                    dimension,
                    verdict,
                    retry_after_ms,
                    evidence_code,
                    classifier_profile_version,
                ),
                AttemptDurableEffectWrite::Group {
                    station_id,
                    group_binding_id,
                    dimension,
                    verdict,
                    retry_after_ms,
                    evidence_code,
                    classifier_profile_version,
                } => (
                    ScopedHealthSubject::group(
                        station_id,
                        group_binding_id,
                        write
                            .group_revision
                            .filter(|_| write.group_binding_id.as_deref() == Some(group_binding_id))
                            .ok_or(PersistenceError::ConstraintViolation)?,
                    )?,
                    dimension,
                    verdict,
                    retry_after_ms,
                    evidence_code,
                    classifier_profile_version,
                ),
                AttemptDurableEffectWrite::Endpoint {
                    station_id,
                    endpoint_revision,
                    dimension,
                    verdict,
                    retry_after_ms,
                    evidence_code,
                    classifier_profile_version,
                } => (
                    ScopedHealthSubject::endpoint(station_id, *endpoint_revision)?,
                    dimension,
                    verdict,
                    retry_after_ms,
                    evidence_code,
                    classifier_profile_version,
                ),
                AttemptDurableEffectWrite::UnsupportedModel { .. } => unreachable!(),
            };
            let dimension = match dimension.as_str() {
                "credential" => FailureDimension::Credential,
                "account_lifecycle" => FailureDimension::AccountLifecycle,
                "group_subscription" => FailureDimension::GroupSubscription,
                "balance" => FailureDimension::Balance,
                "quota" => FailureDimension::Quota,
                "rate_limit" => FailureDimension::RateLimit,
                "endpoint_availability" => FailureDimension::EndpointAvailability,
                _ => return Err(PersistenceError::ConstraintViolation),
            };
            let verdict = match verdict.as_str() {
                "degraded" => DurableHealthVerdict::Degraded,
                "cooldown" => DurableHealthVerdict::Cooldown,
                "blocked" => DurableHealthVerdict::Blocked,
                _ => return Err(PersistenceError::ConstraintViolation),
            };
            let cooldown_until_ms = matches!(verdict, DurableHealthVerdict::Cooldown).then(|| {
                write
                    .terminal_at_ms
                    .saturating_add(retry_after_ms.unwrap_or(30_000).max(0))
            });
            RoutingHealthVerdictStore
                .apply_observation(
                    session.connection(),
                    &ScopedHealthObservation {
                        observation_id: format!(
                            "scoped-health:{}:{}:{}",
                            write.request_id,
                            write.ordinal,
                            dimension.as_str()
                        ),
                        producer_id: format!("request-finalization:{}", write.request_id),
                        producer_sequence: u64::from(write.ordinal),
                        logical_request_id: write.request_id.clone(),
                        attempt_ordinal: u8::try_from(write.ordinal)
                            .map_err(|_| PersistenceError::ConstraintViolation)?,
                        terminal_kind: write.terminal_kind.clone(),
                        subject,
                        dimension,
                        verdict: Some(verdict),
                        cooldown_until_ms,
                        evidence_code: evidence_code.clone(),
                        classifier_profile_version: profile.clone(),
                    },
                    write.terminal_at_ms.max(0),
                )
                .await?;
        }
    }
    Ok(())
}

fn map_durable_effect(effect: &HealthEffect) -> Option<AttemptDurableEffectWrite> {
    let dimension = |value: DurableFailureDimension| {
        match value {
            DurableFailureDimension::Credential => "credential",
            DurableFailureDimension::AccountLifecycle => "account_lifecycle",
            DurableFailureDimension::GroupSubscription => "group_subscription",
            DurableFailureDimension::Balance => "balance",
            DurableFailureDimension::Quota => "quota",
            DurableFailureDimension::RateLimit => "rate_limit",
            DurableFailureDimension::EndpointAvailability => "endpoint_availability",
        }
        .to_string()
    };
    let verdict = |value: DurableVerdict| match value {
        DurableVerdict::Degraded => ("degraded".to_string(), None),
        DurableVerdict::Cooldown { retry_after_ms } => ("cooldown".to_string(), retry_after_ms),
        DurableVerdict::Blocked => ("blocked".to_string(), None),
    };
    match effect {
        HealthEffect::Scoped(effect) => {
            let (verdict, retry_after_ms) = verdict(effect.verdict);
            let dimension = dimension(effect.dimension);
            Some(match &effect.scope {
                DurableHealthScope::Credential { station_key_id } => {
                    AttemptDurableEffectWrite::Credential {
                        station_key_id: station_key_id.clone(),
                        dimension,
                        verdict,
                        retry_after_ms,
                        evidence_code: effect.evidence_code.clone(),
                        classifier_profile_version: effect.classifier_profile_version.clone(),
                    }
                }
                DurableHealthScope::Account { station_id } => AttemptDurableEffectWrite::Account {
                    station_id: station_id.clone(),
                    dimension,
                    verdict,
                    retry_after_ms,
                    evidence_code: effect.evidence_code.clone(),
                    classifier_profile_version: effect.classifier_profile_version.clone(),
                },
                DurableHealthScope::Group {
                    station_id,
                    group_binding_id,
                } => AttemptDurableEffectWrite::Group {
                    station_id: station_id.clone(),
                    group_binding_id: group_binding_id.clone(),
                    dimension,
                    verdict,
                    retry_after_ms,
                    evidence_code: effect.evidence_code.clone(),
                    classifier_profile_version: effect.classifier_profile_version.clone(),
                },
                DurableHealthScope::Endpoint {
                    station_id,
                    endpoint_revision,
                } => AttemptDurableEffectWrite::Endpoint {
                    station_id: station_id.clone(),
                    endpoint_revision: *endpoint_revision,
                    dimension,
                    verdict,
                    retry_after_ms,
                    evidence_code: effect.evidence_code.clone(),
                    classifier_profile_version: effect.classifier_profile_version.clone(),
                },
            })
        }
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
            Some(format!("{:?}", failure.terminal)),
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
                |facts| facts.retry_disposition.clone(),
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
    use crate::application::error_rate_protection::{
        admission_scope, endpoint_health_scope, ErrorRateProtectionAdapter,
        ErrorRateProtectionConfigV1, ErrorRateProtectionService,
    };
    use crate::application::health_protection::{
        HealthProtectionScope, HealthProtectionScopeKind, HealthProtectionState,
    };
    use crate::application::request_lifecycle::{
        attempt::{
            AttemptContext, AttemptFailureKind, ClassifiedAttemptFailure, DurableHealthEffect,
            FailureBlame, RetryDisposition,
        },
        delivery::DeliveryTerminal,
        request::{
            AttemptId, DeliveryFailure, RequestContextSnapshot, RequestLogAnnotations,
            RequestRoutingOutcomeFacts, RequestTerminalSnapshot,
        },
    };
    use crate::persistence::runtime::PersistenceRuntime;
    use crate::persistence::stores::routing_error_rate_history_store::RoutingErrorRateHistoryStore;
    use crate::persistence::stores::routing_health_verdict_store::ScopedObservationApplyResult;

    fn context(request_id: &str) -> RequestContextSnapshot {
        RequestContextSnapshot {
            request_id: request_id.to_string(),
            method: "POST".to_string(),
            local_path: "/v1/responses".to_string(),
            endpoint: "responses".to_string(),
            received_at_ms: 1_000,
        }
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
                model_alias_revision: 1,
                started_at_ms: 1_010,
                probe_scope: None,
                probe_state_revision: Some(7),
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
            probe_scope: None,
            probe_state_revision: Some(7),
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
        assert_eq!(write.probe_state_revision, Some(7));
    }

    #[test]
    fn probe_mapping_keeps_success_and_failure_as_typed_terminal_updates() {
        use crate::application::error_rate_protection::admission_scope;
        use crate::application::health_protection::HealthProtectionScopeKind;

        let probe_scope = admission_scope(HealthProtectionScopeKind::Credential, "key-probe");
        let base_context = AttemptContext {
            attempt_id: AttemptId::new("req-probe", 0),
            station_id: "station-probe".to_string(),
            station_key_id: "key-probe".to_string(),
            endpoint_revision: 4,
            credential_revision: 2,
            account_revision: 1,
            group_binding_id: None,
            group_revision: None,
            resolved_upstream_model: Some("gpt-probe".to_string()),
            model_alias_revision: 1,
            started_at_ms: 10,
            probe_scope: Some(probe_scope.clone()),
            probe_state_revision: Some(9),
        };

        let success = map_attempt_terminal(AttemptTerminalRecord {
            context: base_context.clone(),
            terminal: AttemptTerminal::Failed(ClassifiedAttemptFailure {
                kind: AttemptFailureKind::HttpStatus,
                blame: FailureBlame::Upstream,
                retry: RetryDisposition::TryNextCandidate,
                health: HealthEffect::Success,
                public_code: "upstream_recovered".to_string(),
                sanitized_detail: None,
            }),
            output_committed: false,
            terminal_at_ms: 20,
            probe_scope: Some(probe_scope.clone()),
            probe_state_revision: Some(9),
        });
        assert_eq!(success.health_update, AttemptHealthUpdate::ProbeSuccess);
        assert!(success.durable_effect.is_none());

        let failure = map_attempt_terminal(AttemptTerminalRecord {
            context: base_context,
            terminal: AttemptTerminal::Failed(ClassifiedAttemptFailure {
                kind: AttemptFailureKind::HttpStatus,
                blame: FailureBlame::Upstream,
                retry: RetryDisposition::TryNextCandidate,
                health: HealthEffect::Scoped(DurableHealthEffect {
                    scope: DurableHealthScope::Credential {
                        station_key_id: "key-probe".to_string(),
                    },
                    dimension: DurableFailureDimension::Credential,
                    verdict: DurableVerdict::Blocked,
                    evidence_code: "invalid_api_key".to_string(),
                    classifier_profile_version: "probe-test-v1".to_string(),
                }),
                public_code: "upstream_auth_failed".to_string(),
                sanitized_detail: Some("credential rejected".to_string()),
            }),
            output_committed: false,
            terminal_at_ms: 21,
            probe_scope: Some(probe_scope),
            probe_state_revision: Some(9),
        });
        assert_eq!(
            failure.health_update,
            AttemptHealthUpdate::ProbeFailure {
                retry_after_ms: None
            }
        );
        assert!(matches!(
            failure.durable_effect,
            Some(AttemptDurableEffectWrite::Credential { .. })
        ));
    }

    #[test]
    fn probe_scope_round_trips_without_reconstructing_identity_from_terminal_fields() {
        let scope = admission_scope(HealthProtectionScopeKind::Endpoint, "station-probe:4");
        let encoded = serde_json::to_string(&scope).expect("serialize probe scope");
        let restored: crate::application::health_protection::HealthProtectionScope =
            serde_json::from_str(&encoded).expect("restore probe scope");
        assert_eq!(restored, scope);
        assert_eq!(restored.kind, HealthProtectionScopeKind::Endpoint);
    }

    fn scoped_verdict_observation(
        id: &str,
        subject: ScopedHealthSubject,
        dimension: FailureDimension,
        verdict: Option<DurableHealthVerdict>,
    ) -> ScopedHealthObservation {
        ScopedHealthObservation {
            observation_id: id.to_string(),
            producer_id: format!("request-finalization-test-{id}"),
            producer_sequence: 1,
            logical_request_id: id.to_string(),
            attempt_ordinal: 0,
            terminal_kind: "failed".to_string(),
            subject,
            dimension,
            verdict,
            cooldown_until_ms: None,
            evidence_code: if verdict.is_some() {
                "invalid_api_key".to_string()
            } else {
                "probe_success_recovery".to_string()
            },
            classifier_profile_version: "request-finalization-test-v1".to_string(),
        }
    }

    fn probe_write(
        request_id: &str,
        scope: HealthProtectionScope,
        state_revision: u64,
        health_update: AttemptHealthUpdate,
        durable_effect: Option<AttemptDurableEffectWrite>,
    ) -> AttemptTerminalWrite {
        AttemptTerminalWrite {
            request_id: request_id.to_string(),
            ordinal: 0,
            station_id: "station-probe".to_string(),
            station_key_id: "key-probe".to_string(),
            endpoint_revision: 4,
            credential_revision: 2,
            account_revision: 1,
            group_binding_id: None,
            group_revision: None,
            resolved_upstream_model: Some("gpt-probe".to_string()),
            model_alias_revision: 1,
            started_at_ms: 10,
            terminal_kind: "failed".to_string(),
            failure_kind: Some("HttpStatus".to_string()),
            failure_blame: Some("Upstream".to_string()),
            retry_disposition: Some("TryNextCandidate".to_string()),
            health_effect: "probe".to_string(),
            health_cooldown_until_ms: None,
            health_update,
            durable_effect,
            public_code: Some("upstream_probe_result".to_string()),
            sanitized_detail: None,
            output_committed: false,
            terminal_at_ms: 100_001,
            probe_scope: Some(scope),
            probe_state_revision: Some(state_revision),
        }
    }

    #[tokio::test]
    async fn probe_success_clears_endpoint_without_clearing_revisioned_credential_verdict() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("probe-recovery.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize runtime");
        let endpoint_scope = endpoint_health_scope("station-probe", 4).expect("endpoint scope");
        let credential_scope = admission_scope(HealthProtectionScopeKind::Credential, "key-probe");
        let endpoint_subject = ScopedHealthSubject::endpoint("station-probe", 4).expect("subject");
        let credential_subject =
            ScopedHealthSubject::credential("station-probe", "key-probe", 2).expect("subject");
        let credential_durable_scope = HealthProtectionScope::new(
            HealthProtectionScopeKind::Credential,
            credential_subject.scope().to_string(),
        )
        .expect("credential durable scope");

        let mut seed = runtime.begin_write().await.expect("seed write");
        for (id, subject, dimension) in [
            (
                "endpoint-blocked",
                endpoint_subject.clone(),
                FailureDimension::EndpointAvailability,
            ),
            (
                "credential-blocked",
                credential_subject.clone(),
                FailureDimension::Credential,
            ),
        ] {
            assert_eq!(
                RoutingHealthVerdictStore
                    .apply_observation(
                        seed.connection(),
                        &scoped_verdict_observation(
                            id,
                            subject,
                            dimension,
                            Some(DurableHealthVerdict::Blocked),
                        ),
                        1000,
                    )
                    .await
                    .expect("seed durable verdict"),
                ScopedObservationApplyResult::Applied
            );
        }
        seed.commit().await.expect("commit seed");

        let mut recovery = runtime.begin_write().await.expect("recovery write");
        let endpoint_write = probe_write(
            "endpoint-probe-success",
            endpoint_scope.clone(),
            1,
            AttemptHealthUpdate::ProbeSuccess,
            None,
        );
        apply_probe_recovery(&mut recovery, &endpoint_write, &endpoint_scope)
            .await
            .expect("endpoint recovery");
        let credential_write = probe_write(
            "credential-probe-success",
            credential_scope.clone(),
            1,
            AttemptHealthUpdate::ProbeSuccess,
            None,
        );
        apply_probe_recovery(&mut recovery, &credential_write, &credential_scope)
            .await
            .expect("credential recovery");
        recovery.commit().await.expect("commit recovery");

        let mut read = runtime.handle().begin_read().await.expect("read recovery");
        let active = RoutingHealthVerdictStore
            .load_active_batch(
                read.connection(),
                &[endpoint_subject.clone(), credential_subject.clone()],
            )
            .await
            .expect("active verdicts");
        assert_eq!(active.len(), 1);
        assert_eq!(
            active.values().next().map(|row| row.verdict),
            Some(DurableHealthVerdict::Blocked)
        );
        let statuses = RoutingHealthVerdictStore
            .load_health_protection_statuses(read.connection(), 100_002)
            .await
            .expect("health statuses");
        assert!(statuses.iter().any(|status| {
            status.scope == endpoint_scope && status.state == HealthProtectionState::Closed
        }));
        assert!(statuses.iter().any(|status| {
            status.scope == credential_durable_scope && status.state == HealthProtectionState::Open
        }));
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn probe_failure_consumes_reducer_fence_before_reopening_endpoint_verdict() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("probe-failure.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize runtime");
        let mut policy = runtime.begin_write().await.expect("policy write");
        let policy_json: String =
            sqlx::query_scalar("SELECT config_json FROM routing_policy WHERE singleton_key = 1")
                .fetch_one(policy.connection())
                .await
                .expect("load routing policy");
        let mut policy_value: serde_json::Value =
            serde_json::from_str(&policy_json).expect("decode routing policy");
        policy_value["protectionProfile"]["enabled"] = serde_json::Value::Bool(true);
        sqlx::query("UPDATE routing_policy SET config_json = ?1 WHERE singleton_key = 1")
            .bind(policy_value.to_string())
            .execute(policy.connection())
            .await
            .expect("enable protection profile");
        policy.commit().await.expect("commit policy");

        let endpoint_scope = endpoint_health_scope("station-probe", 4).expect("endpoint scope");
        let endpoint_subject = ScopedHealthSubject::endpoint("station-probe", 4).expect("subject");
        let mut seed = runtime.begin_write().await.expect("seed write");
        RoutingHealthVerdictStore
            .apply_observation(
                seed.connection(),
                &scoped_verdict_observation(
                    "endpoint-blocked",
                    endpoint_subject.clone(),
                    FailureDimension::EndpointAvailability,
                    Some(DurableHealthVerdict::Blocked),
                ),
                1000,
            )
            .await
            .expect("seed endpoint verdict");
        seed.commit().await.expect("commit seed");

        // The reservation must be committed before the terminal write uses it.
        let mut reserve = runtime.begin_write().await.expect("reserve write");
        let probe = RoutingHealthVerdictStore
            .begin_health_protection_probe(reserve.connection(), &endpoint_scope, 100_000)
            .await
            .expect("reserve endpoint probe")
            .expect("cooldown expired");
        reserve.commit().await.expect("commit reservation");

        let error_rate = ErrorRateProtectionService::from_adapter(
            ErrorRateProtectionAdapter::new(ErrorRateProtectionConfigV1 {
                enabled: true,
                ..Default::default()
            })
            .expect("error-rate adapter"),
        );
        let service =
            RequestFinalizationService::new_with_error_rate(runtime.handle(), error_rate.clone());
        service
            .start_request(RequestStartRecord {
                context: context("req-probe-failure"),
            })
            .await
            .expect("start request");
        let terminal_record = AttemptTerminalRecord {
            context: AttemptContext {
                attempt_id: AttemptId::new("req-probe-failure", 0),
                station_id: "station-probe".to_string(),
                station_key_id: "key-probe".to_string(),
                endpoint_revision: 4,
                credential_revision: 2,
                account_revision: 1,
                group_binding_id: None,
                group_revision: None,
                resolved_upstream_model: Some("gpt-probe".to_string()),
                model_alias_revision: 1,
                started_at_ms: 100_000,
                probe_scope: Some(endpoint_scope.clone()),
                probe_state_revision: Some(probe.state_revision),
            },
            terminal: AttemptTerminal::Failed(ClassifiedAttemptFailure {
                kind: AttemptFailureKind::HttpStatus,
                blame: FailureBlame::Upstream,
                retry: RetryDisposition::TryNextCandidate,
                health: HealthEffect::Scoped(DurableHealthEffect {
                    scope: DurableHealthScope::Endpoint {
                        station_id: "station-probe".to_string(),
                        endpoint_revision: 4,
                    },
                    dimension: DurableFailureDimension::EndpointAvailability,
                    verdict: DurableVerdict::Blocked,
                    evidence_code: "endpoint_unavailable".to_string(),
                    classifier_profile_version: "probe-test-v1".to_string(),
                }),
                public_code: "upstream_endpoint_failed".to_string(),
                sanitized_detail: None,
            }),
            output_committed: false,
            terminal_at_ms: 100_001,
            probe_scope: Some(endpoint_scope.clone()),
            probe_state_revision: Some(probe.state_revision),
        };
        let write = map_attempt_terminal(terminal_record);
        assert_eq!(
            write.health_update,
            AttemptHealthUpdate::ProbeFailure {
                retry_after_ms: None
            }
        );
        let mut session = runtime.begin_write().await.expect("terminal write");
        RequestLogStore
            .finish_attempt(&mut session, &write)
            .await
            .expect("write attempt terminal");
        let probe_observation = routing_observation(&write, 1).expect("routing probe observation");
        ObservationIngestion::with_error_rate(error_rate.clone())
            .append(&mut session, probe_observation)
            .await
            .expect("append probe observation");
        apply_durable_attempt_effect(&mut session, &write)
            .await
            .expect("reopen durable endpoint verdict");
        session.commit().await.expect("commit terminal write");

        let mut read = runtime.handle().begin_read().await.expect("read failure");
        let status = RoutingHealthVerdictStore
            .load_health_protection_statuses(read.connection(), 100_002)
            .await
            .expect("health statuses")
            .into_iter()
            .find(|status| status.scope == endpoint_scope)
            .expect("endpoint status");
        assert_eq!(status.state, HealthProtectionState::Open);
        assert!(!status.half_open_probe_in_flight);
        let active = RoutingHealthVerdictStore
            .load_active_batch(read.connection(), &[endpoint_subject])
            .await
            .expect("active endpoint verdict");
        assert_eq!(active.len(), 1);
        assert_eq!(
            active.values().next().map(|row| row.verdict),
            Some(DurableHealthVerdict::Blocked)
        );
        let history = RoutingErrorRateHistoryStore
            .list_page(
                read.connection(),
                None,
                10,
                &ErrorRateProtectionConfigV1 {
                    enabled: true,
                    ..Default::default()
                },
                100_002,
            )
            .await
            .expect("probe history");
        assert_eq!(
            history.events.last().and_then(|event| event.transition),
            Some(
                crate::application::error_rate_protection::HealthProtectionTransitionCode::Reopened
            )
        );
        let evidence: String = sqlx::query_scalar(
            "SELECT evidence_json FROM routing_observations WHERE id = 'routing-observation-req-probe-failure-0'",
        )
        .fetch_one(read.connection())
        .await
        .expect("probe evidence");
        let evidence: serde_json::Value = serde_json::from_str(&evidence).expect("decode evidence");
        assert_eq!(evidence["probe_state_revision"], probe.state_revision);
        assert_eq!(
            evidence["probe_scope"],
            serde_json::to_value(&endpoint_scope).expect("encode probe scope")
        );
        drop(read);
        runtime.close().await.expect("close runtime");
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
            Some("DownstreamWriteFailed")
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

    #[test]
    fn unknown_commit_outcome_remains_distinguishable_at_the_lifecycle_port() {
        let error = map_persistence_error(PersistenceError::CommitOutcomeUnknown);

        assert!(matches!(
            error,
            LifecycleWriteError::CommitOutcomeUnknown(_)
        ));
    }
}
