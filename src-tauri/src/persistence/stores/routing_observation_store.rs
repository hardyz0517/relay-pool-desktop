use serde_json::Value;
use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoutingObservationAppend {
    pub(crate) id: String,
    pub(crate) producer_id: String,
    pub(crate) producer_sequence: u64,
    pub(crate) payload_hash: String,
    pub(crate) event_at_ms: i64,
    pub(crate) ingested_at_ms: i64,
    pub(crate) scope: String,
    pub(crate) source: String,
    pub(crate) traffic_equivalence: String,
    pub(crate) outcome_kind: String,
    pub(crate) latency_ms: Option<i64>,
    pub(crate) mass_basis_points: Option<u16>,
    pub(crate) evidence: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObservationAppendResult {
    Inserted,
    Existing,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingObservationStore;

impl RoutingObservationStore {
    pub(crate) async fn append(
        &self,
        connection: &mut SqliteConnection,
        observation: &RoutingObservationAppend,
        now_ms: i64,
    ) -> Result<ObservationAppendResult, PersistenceError> {
        validate_observation(observation, now_ms)?;
        let evidence_json = serde_json::to_string(&observation.evidence)
            .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
        if let Some(row) = sqlx::query(
            "SELECT producer_id, producer_sequence, payload_hash FROM routing_observations WHERE id = ?1",
        )
        .bind(&observation.id)
        .fetch_optional(&mut *connection)
        .await?
        {
            if row.get::<String, _>("producer_id") == observation.producer_id
                && row.get::<i64, _>("producer_sequence") == observation.producer_sequence as i64
                && row.get::<String, _>("payload_hash") == observation.payload_hash
            {
                return Ok(ObservationAppendResult::Existing);
            }
            return Err(PersistenceError::InvariantViolation("routing observation id collision".into()));
        }
        if let Some(row) = sqlx::query(
            "SELECT id, payload_hash FROM routing_observations WHERE producer_id = ?1 AND producer_sequence = ?2",
        )
        .bind(&observation.producer_id)
        .bind(i64::try_from(observation.producer_sequence).map_err(|_| PersistenceError::ConstraintViolation)?)
        .fetch_optional(&mut *connection)
        .await?
        {
            if row.get::<String, _>("id") == observation.id
                && row.get::<String, _>("payload_hash") == observation.payload_hash
            {
                return Ok(ObservationAppendResult::Existing);
            }
            return Err(PersistenceError::InvariantViolation("routing observation producer sequence collision".into()));
        }
        sqlx::query(
            "INSERT INTO routing_observations (id, producer_id, producer_sequence, payload_hash, event_at_ms, ingested_at_ms, scope, source, traffic_equivalence, outcome_kind, latency_ms, mass_basis_points, evidence_json, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )
        .bind(&observation.id)
        .bind(&observation.producer_id)
        .bind(i64::try_from(observation.producer_sequence).map_err(|_| PersistenceError::ConstraintViolation)?)
        .bind(&observation.payload_hash)
        .bind(observation.event_at_ms)
        .bind(observation.ingested_at_ms)
        .bind(&observation.scope)
        .bind(&observation.source)
        .bind(&observation.traffic_equivalence)
        .bind(&observation.outcome_kind)
        .bind(observation.latency_ms)
        .bind(observation.mass_basis_points.map(i64::from))
        .bind(evidence_json)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        Ok(ObservationAppendResult::Inserted)
    }
}

fn validate_observation(value: &RoutingObservationAppend, now_ms: i64) -> Result<(), PersistenceError> {
    let values = [&value.id, &value.producer_id, &value.payload_hash, &value.scope, &value.source, &value.traffic_equivalence, &value.outcome_kind];
    if values.iter().any(|text| text.is_empty() || text.len() > 192 || text.chars().any(char::is_control))
        || value.payload_hash.len() < 16
        || value.event_at_ms < 0
        || value.ingested_at_ms < 0
        || now_ms < 0
        || value.latency_ms.is_some_and(|latency| latency < 0)
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}
