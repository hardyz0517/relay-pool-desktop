use serde_json::json;
use sha2::{Digest, Sha256};

#[cfg(test)]
use crate::application::error_rate_protection::ErrorRateProtectionService;
#[cfg(test)]
use crate::persistence::stores::routing_health_verdict_store::RoutingHealthVerdictStore;

use crate::{
    models::routing_observation::{
        FailureAttribution, ObservationOutcome, ObservationRetryDisposition, ObservationSource,
        RecoveryOrigin, ResponseOrigin, RoutingObservation, TrafficEquivalence,
    },
    persistence::{
        error::PersistenceError,
        stores::routing_observation_store::{
            ObservationAppendResult, RoutingObservationAppend, RoutingObservationStore,
        },
        WriteSession,
    },
};

#[derive(Debug, Clone)]
pub(crate) struct ObservationIngestion {
    store: RoutingObservationStore,
    #[cfg(test)]
    error_rate: ErrorRateProtectionService,
}

impl Default for ObservationIngestion {
    fn default() -> Self {
        Self::new()
    }
}

impl ObservationIngestion {
    pub(crate) fn new() -> Self {
        Self {
            store: RoutingObservationStore,
            #[cfg(test)]
            error_rate: ErrorRateProtectionService::disabled(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_error_rate(error_rate: ErrorRateProtectionService) -> Self {
        Self {
            store: RoutingObservationStore,
            error_rate,
        }
    }

    /// The only production entry point for routing observations. The caller
    /// owns the write transaction so an outcome and its observation can commit
    /// atomically with the source terminal row.
    pub(crate) async fn append(
        &self,
        write: &mut WriteSession,
        observation: RoutingObservation,
    ) -> Result<ObservationAppendResult, PersistenceError> {
        self.append_with_generation_eligibility(write, observation, None)
            .await
    }

    pub(crate) async fn append_with_generation_eligibility(
        &self,
        write: &mut WriteSession,
        observation: RoutingObservation,
        generation_eligibility: Option<&str>,
    ) -> Result<ObservationAppendResult, PersistenceError> {
        observation
            .validate()
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        #[cfg(test)]
        let legacy_projection = {
            let protection_enabled = routing_policy_protection_enabled(write).await?;
            (
                self.error_rate
                    .health_observation_for_policy(&observation, protection_enabled),
                self.error_rate.history_event_seed_for_policy(
                    &observation,
                    None,
                    protection_enabled,
                ),
                self.error_rate.config_for_policy(protection_enabled),
            )
        };
        let append = to_append(&observation)?;
        let received_at_ms = chrono::Utc::now().timestamp_millis().max(0);
        let result = self
            .store
            .append_with_generation_eligibility(
                write.connection(),
                &append,
                generation_eligibility,
                received_at_ms,
            )
            .await?;
        #[cfg(test)]
        if matches!(result, ObservationAppendResult::Inserted) {
            let (reducer_input, history_seed, config) = legacy_projection;
            if let (Some(reducer_input), Some(history_seed)) = (reducer_input, history_seed) {
                RoutingHealthVerdictStore
                    .apply_error_rate_observation(
                        write.connection(),
                        reducer_input,
                        history_seed,
                        &observation.id,
                        &config,
                        observation.order.event_at_ms.max(received_at_ms),
                    )
                    .await?;
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
async fn routing_policy_protection_enabled(
    write: &mut WriteSession,
) -> Result<bool, PersistenceError> {
    // The legacy error-rate reducer is a compatibility bridge only. Once the
    // effective v3 generation is active it must stay disabled even if the old
    // singleton row still contains `protectionProfile.enabled=true`.
    let Some(stored) =
        crate::persistence::stores::routing_policy_v3_stage_upgrade::load_effective_active_in(
            write.connection(),
        )
        .await?
    else {
        return Ok(false);
    };
    if stored
        .config
        .get("version")
        .and_then(serde_json::Value::as_u64)
        == Some(u64::from(
            crate::models::routing_policy::ROUTING_POLICY_CONFIG_VERSION_V3,
        ))
    {
        return Ok(false);
    }
    Ok(stored
        .config
        .get("protectionProfile")
        .and_then(serde_json::Value::as_object)
        .and_then(|profile| profile.get("enabled"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false))
}

fn to_append(
    observation: &RoutingObservation,
) -> Result<RoutingObservationAppend, PersistenceError> {
    let scope = observation_scope(&observation.scope);
    let source = source_name(&observation.source);
    let traffic_equivalence = traffic_name(&observation.traffic_equivalence);
    let outcome_kind = outcome_name(&observation.outcome);
    let evidence = json!({
        "station_id": observation.scope.station_id,
        "station_key_id": observation.scope.station_key_id,
        "model": observation.scope.model,
        "endpoint_revision": observation.scope.endpoint_revision,
        "source": source,
        "traffic_equivalence": traffic_equivalence,
        "outcome": outcome_kind,
        "latency_ms": observation.latency_ms,
        "evidence_mass_basis_points": observation.evidence_mass_basis_points,
        "comparability_key": observation.comparability_key,
        // Probe scope is an opaque commitment and is persisted so a reloaded
        // observation cannot be mistaken for an ordinary Credential sample.
        "probe_scope": observation.probe_scope,
        "probe_state_revision": observation.probe_state_revision,
        "correlation_id": observation.correlation_id,
        "attempt_index": observation.attempt_index,
        "station_key_lifecycle_revision": observation.station_key_lifecycle_revision,
        "cluster_finalized": observation.cluster_finalized,
        "cluster_expected_attempt_count": observation.cluster_expected_attempt_count,
        "boundary_crossed": observation.boundary_crossed,
        "event_time_status": observation.event_time_status,
        "response_origin": observation.response_origin,
        "failure_code": observation.failure_code,
        "failure_attribution": observation.failure_attribution,
        "recovery_origin": observation.recovery_origin,
        "retry_disposition": observation.retry_disposition,
    });
    let payload = serde_json::to_vec(&evidence)
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(&payload);
    let payload_hash = format!("{:x}", hasher.finalize());
    Ok(RoutingObservationAppend {
        id: observation.id.clone(),
        producer_id: observation.order.producer_id.clone(),
        producer_sequence: observation.order.producer_sequence,
        payload_hash,
        event_at_ms: observation.order.event_at_ms,
        ingested_at_ms: observation.order.ingested_at_ms,
        scope,
        source: source.to_string(),
        traffic_equivalence: traffic_equivalence.to_string(),
        outcome_kind: outcome_kind.to_string(),
        latency_ms: observation.latency_ms.map(i64::from),
        mass_basis_points: Some(observation.evidence_mass_basis_points),
        comparability_key: observation.comparability_key.clone(),
        evidence,
        correlation_id: observation.correlation_id.clone(),
        attempt_index: observation.attempt_index,
        station_key_lifecycle_revision: observation.station_key_lifecycle_revision,
        cluster_finalized: observation.cluster_finalized,
        cluster_expected_attempt_count: observation.cluster_expected_attempt_count,
        boundary_crossed: observation.boundary_crossed,
        event_time_status: observation.event_time_status,
        response_origin: response_origin_name(observation.response_origin).to_string(),
        failure_code: observation.failure_code.clone(),
        failure_attribution: failure_attribution_name(observation.failure_attribution).to_string(),
        recovery_origin: recovery_origin_name(observation.recovery_origin).to_string(),
        retry_disposition: retry_disposition_name(observation.retry_disposition).to_string(),
    })
}

fn response_origin_name(value: ResponseOrigin) -> &'static str {
    match value {
        ResponseOrigin::Upstream => "upstream",
        ResponseOrigin::Relay => "relay",
        ResponseOrigin::Unknown => "unknown",
    }
}

fn failure_attribution_name(value: FailureAttribution) -> &'static str {
    match value {
        FailureAttribution::Key => "key",
        FailureAttribution::Local => "local",
        FailureAttribution::Client => "client",
        FailureAttribution::Unknown => "unknown",
    }
}

fn recovery_origin_name(value: RecoveryOrigin) -> &'static str {
    match value {
        RecoveryOrigin::Normal => "normal",
        RecoveryOrigin::CrashRecovery => "crash_recovery",
        RecoveryOrigin::LeaseReaper => "lease_reaper",
    }
}

fn retry_disposition_name(value: ObservationRetryDisposition) -> &'static str {
    match value {
        ObservationRetryDisposition::End => "end",
        ObservationRetryDisposition::RetryableBeforeCommit => "retryable_before_commit",
        ObservationRetryDisposition::StopRequest => "stop_request",
    }
}

fn observation_scope(scope: &crate::models::routing_observation::ObservationScope) -> String {
    scope
        .station_key_id
        .as_deref()
        .map(|value| format!("station_key:{value}"))
        .or_else(|| {
            scope
                .station_id
                .as_deref()
                .map(|value| format!("station:{value}"))
        })
        .or_else(|| scope.model.as_deref().map(|value| format!("model:{value}")))
        .unwrap_or_else(|| "global".to_string())
}

fn source_name(source: &ObservationSource) -> &'static str {
    match source {
        ObservationSource::RealRequest => "real_request",
        ObservationSource::ActiveProbe => "active_probe",
        ObservationSource::Administrative => "administrative",
    }
}

fn traffic_name(traffic: &TrafficEquivalence) -> &'static str {
    match traffic {
        TrafficEquivalence::ExactRequest => "exact_request",
        TrafficEquivalence::SameModelShape => "same_model_shape",
        TrafficEquivalence::EndpointOnly => "endpoint_only",
        TrafficEquivalence::Anonymous => "anonymous",
    }
}

fn outcome_name(outcome: &ObservationOutcome) -> &'static str {
    match outcome {
        ObservationOutcome::Success => "success",
        ObservationOutcome::CredentialFailure => "credential_failure",
        ObservationOutcome::EndpointFailure => "endpoint_failure",
        ObservationOutcome::ModelFailure => "model_failure",
        ObservationOutcome::RateLimited => "rate_limited",
        ObservationOutcome::Timeout => "timeout",
        ObservationOutcome::Cancelled => "cancelled",
        ObservationOutcome::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::error_rate_protection::{
        ErrorRateProtectionAdapter, ErrorRateProtectionConfigV1, ErrorRateProtectionService,
    };
    use crate::models::routing_observation::EventTimeStatus;
    use crate::persistence::{
        runtime::PersistenceRuntime,
        stores::routing_error_rate_history_store::RoutingErrorRateHistoryStore,
    };
    use sqlx::Row;

    #[test]
    fn scope_and_taxonomy_are_stable_and_secret_free() {
        let scope = crate::models::routing_observation::ObservationScope {
            station_id: Some("station-1".into()),
            station_key_id: Some("key-1".into()),
            model: Some("model-x".into()),
            endpoint_revision: Some(2),
        };
        assert_eq!(observation_scope(&scope), "station_key:key-1");
        assert_eq!(source_name(&ObservationSource::ActiveProbe), "active_probe");
        assert_eq!(
            outcome_name(&ObservationOutcome::CredentialFailure),
            "credential_failure"
        );
    }

    fn observation(id: &str, event_at_ms: i64, outcome: ObservationOutcome) -> RoutingObservation {
        RoutingObservation {
            id: id.to_string(),
            order: crate::models::routing_observation::ObservationOrder {
                producer_id: "ingestion-test".to_string(),
                producer_sequence: event_at_ms as u64 + 1,
                event_at_ms,
                ingested_at_ms: event_at_ms,
            },
            scope: crate::models::routing_observation::ObservationScope {
                station_id: Some("station-1".to_string()),
                station_key_id: Some("key-1".to_string()),
                model: Some("model-1".to_string()),
                endpoint_revision: Some(1),
            },
            source: ObservationSource::RealRequest,
            traffic_equivalence: TrafficEquivalence::ExactRequest,
            outcome,
            latency_ms: Some(10),
            evidence_mass_basis_points: 10_000,
            comparability_key: None,
            correlation_id: id.to_string(),
            attempt_index: 0,
            station_key_lifecycle_revision: 1,
            cluster_finalized: true,
            cluster_expected_attempt_count: 1,
            boundary_crossed: true,
            event_time_status: EventTimeStatus::Valid,
            response_origin: crate::models::routing_observation::ResponseOrigin::Upstream,
            failure_code: None,
            failure_attribution: crate::models::routing_observation::FailureAttribution::Key,
            recovery_origin: crate::models::routing_observation::RecoveryOrigin::Normal,
            retry_disposition: crate::models::routing_observation::ObservationRetryDisposition::End,
            probe_scope: None,
            probe_state_revision: None,
        }
    }

    #[tokio::test]
    async fn append_commits_observation_reducer_and_history_as_one_restartable_unit() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("observation-ingestion.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize runtime");
        // The reducer switch is policy-owned. Enable it in this fixture so
        // the test exercises the enabled transactional path explicitly.
        let mut policy_write = runtime.begin_write().await.expect("policy write");
        let policy_json: String =
            sqlx::query_scalar("SELECT config_json FROM routing_policy WHERE singleton_key = 1")
                .fetch_one(policy_write.connection())
                .await
                .expect("load policy");
        let mut policy_value: serde_json::Value =
            serde_json::from_str(&policy_json).expect("decode policy");
        policy_value["protectionProfile"]["enabled"] = serde_json::Value::Bool(true);
        sqlx::query("UPDATE routing_policy SET config_json = ?1 WHERE singleton_key = 1")
            .bind(policy_value.to_string())
            .execute(policy_write.connection())
            .await
            .expect("enable protection profile");
        policy_write.commit().await.expect("commit policy");
        let config = ErrorRateProtectionConfigV1 {
            enabled: true,
            history_max_events: 16,
            history_retention_ms: 10_000,
            ..Default::default()
        };
        let service = ErrorRateProtectionService::from_adapter(
            ErrorRateProtectionAdapter::new(config.clone()).expect("adapter"),
        );
        let ingestion = ObservationIngestion::with_error_rate(service);

        let mut write = runtime.begin_write().await.expect("begin write");
        assert_eq!(
            ingestion
                .append(
                    &mut write,
                    observation("ingested-1", 1, ObservationOutcome::EndpointFailure),
                )
                .await
                .expect("append observation"),
            ObservationAppendResult::Inserted
        );
        write.commit().await.expect("commit observation");

        let handle = runtime.handle();
        let mut read = handle.begin_read().await.expect("begin read");
        let observation_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM routing_observations WHERE id = 'ingested-1'")
                .fetch_one(read.connection())
                .await
                .expect("count observation");
        let reducer_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_health_protection_state WHERE singleton_key = 1",
        )
        .fetch_one(read.connection())
        .await
        .expect("count reducer state");
        let history_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_error_rate_history WHERE observation_id = 'ingested-1'",
        )
        .fetch_one(read.connection())
        .await
        .expect("count history");
        assert_eq!(observation_count, 1);
        assert_eq!(reducer_count, 1);
        assert_eq!(history_count, 1);
        let v3 = sqlx::query(
            "SELECT event_id, attempt_id, generation_eligibility, outcome,
                    boundary_crossed, station_key_lifecycle_revision
             FROM routing_observations WHERE id = 'ingested-1'",
        )
        .fetch_one(read.connection())
        .await
        .expect("v3 observation");
        assert_eq!(v3.get::<String, _>("event_id"), "ingested-1");
        assert_eq!(v3.get::<String, _>("attempt_id"), "ingested-1");
        // A fresh database starts in pre-cutover mode, so new observations
        // are staged for the first v3 generation rather than the retired
        // legacy read model.
        assert_eq!(v3.get::<String, _>("generation_eligibility"), "next");
        assert_eq!(v3.get::<String, _>("outcome"), "attributable_failure");
        assert_eq!(v3.get::<i64, _>("boundary_crossed"), 1);
        assert_eq!(v3.get::<i64, _>("station_key_lifecycle_revision"), 1);
        drop(read);

        runtime.close().await.expect("close runtime");
        let reopened = PersistenceRuntime::open_current(&path)
            .await
            .expect("reopen runtime");
        let mut reopened_read = reopened.handle().begin_read().await.expect("reopen read");
        let history = RoutingErrorRateHistoryStore
            .list_page(reopened_read.connection(), None, 10, &config, 10)
            .await
            .expect("read durable history");
        assert_eq!(history.events.len(), 1);
        assert_eq!(history.events[0].observed_at_ms, 1);
        drop(reopened_read);
        reopened.close().await.expect("close reopened runtime");
    }

    #[tokio::test]
    async fn append_rolls_back_all_three_projections_together() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("observation-ingestion-rollback.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize runtime");
        let service = ErrorRateProtectionService::from_adapter(
            ErrorRateProtectionAdapter::new(ErrorRateProtectionConfigV1 {
                enabled: true,
                ..Default::default()
            })
            .expect("adapter"),
        );
        let ingestion = ObservationIngestion::with_error_rate(service);

        let mut write = runtime.begin_write().await.expect("begin write");
        ingestion
            .append(
                &mut write,
                observation("rolled-back", 5, ObservationOutcome::Timeout),
            )
            .await
            .expect("append observation");
        drop(write);

        let mut read = runtime.handle().begin_read().await.expect("begin read");
        for (table, predicate) in [
            ("routing_observations", "id = 'rolled-back'"),
            (
                "routing_error_rate_history",
                "observation_id = 'rolled-back'",
            ),
        ] {
            let sql = format!("SELECT COUNT(*) AS count FROM {table} WHERE {predicate}");
            let count: i64 = sqlx::query(&sql)
                .fetch_one(read.connection())
                .await
                .expect("count rolled-back row")
                .get("count");
            assert_eq!(count, 0, "{table} must roll back");
        }
        let reducer_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_health_protection_state WHERE singleton_key = 1",
        )
        .fetch_one(read.connection())
        .await
        .expect("count reducer state");
        assert_eq!(reducer_count, 0, "reducer snapshot must roll back");
        drop(read);
        runtime.close().await.expect("close runtime");
    }
}
