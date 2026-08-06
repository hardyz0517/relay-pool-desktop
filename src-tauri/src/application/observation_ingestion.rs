use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    models::routing_observation::{
        ObservationOutcome, ObservationSource, RoutingObservation, TrafficEquivalence,
    },
    persistence::{
        error::PersistenceError,
        stores::routing_observation_store::{
            ObservationAppendResult, RoutingObservationAppend, RoutingObservationStore,
        },
        WriteSession,
    },
};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ObservationIngestion {
    store: RoutingObservationStore,
}

impl ObservationIngestion {
    pub(crate) fn new() -> Self {
        Self {
            store: RoutingObservationStore,
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
        observation
            .validate()
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        let append = to_append(&observation)?;
        let received_at_ms = chrono::Utc::now().timestamp_millis().max(0);
        self.store
            .append(write.connection(), &append, received_at_ms)
            .await
    }
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
        evidence,
    })
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
}
