use serde_json::Value;
use sqlx::{Row, SqliteConnection};

use crate::models::routing_observation::{
    ObservationOrder, ObservationOutcome, ObservationScope, ObservationSource, RoutingObservation,
    TrafficEquivalence,
};
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
    pub async fn list_after(
        &self,
        connection: &mut SqliteConnection,
        after: Option<(i64, String)>,
        limit: usize,
    ) -> Result<Vec<RoutingObservation>, PersistenceError> {
        let limit = i64::try_from(limit.clamp(1, 512))
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        let rows = sqlx::query(
            "SELECT id, producer_id, producer_sequence, event_at_ms, ingested_at_ms, scope, source, traffic_equivalence, outcome_kind, latency_ms, mass_basis_points, evidence_json FROM routing_observations WHERE (?1 IS NULL OR ingested_at_ms > ?1 OR (ingested_at_ms = ?1 AND (?2 IS NULL OR id > ?2))) ORDER BY ingested_at_ms ASC, id ASC LIMIT ?3",
        )
        .bind(after.as_ref().map(|value| value.0))
        .bind(after.as_ref().map(|value| value.1.as_str()))
        .bind(limit)
        .fetch_all(&mut *connection)
        .await?;
        rows.into_iter().map(row_to_observation).collect()
    }

    /// Returns the complete durable history for one projection scope. The
    /// projector uses this after discovering a changed scope from its bounded
    /// ingestion cursor, so replacing a summary cannot discard prior evidence.
    pub async fn list_for_scope(
        &self,
        connection: &mut SqliteConnection,
        scope: &str,
    ) -> Result<Vec<RoutingObservation>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT id, producer_id, producer_sequence, event_at_ms, ingested_at_ms, scope, source, traffic_equivalence, outcome_kind, latency_ms, mass_basis_points, evidence_json FROM routing_observations WHERE scope = ?1 ORDER BY ingested_at_ms ASC, id ASC",
        )
        .bind(scope)
        .fetch_all(&mut *connection)
        .await?;
        rows.into_iter().map(row_to_observation).collect()
    }

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
        let max_ingested_at_ms = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(ingested_at_ms) FROM routing_observations",
        )
        .fetch_one(&mut *connection)
        .await?;
        let ingested_at_ms = match max_ingested_at_ms {
            Some(value) => value.checked_add(1).ok_or_else(|| {
                PersistenceError::InvariantViolation(
                    "routing observation ingestion clock exhausted".into(),
                )
            })?,
            None => now_ms,
        }
        .max(now_ms);
        sqlx::query(
            "INSERT INTO routing_observations (id, producer_id, producer_sequence, payload_hash, event_at_ms, ingested_at_ms, scope, source, traffic_equivalence, outcome_kind, latency_ms, mass_basis_points, evidence_json, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )
        .bind(&observation.id)
        .bind(&observation.producer_id)
        .bind(i64::try_from(observation.producer_sequence).map_err(|_| PersistenceError::ConstraintViolation)?)
        .bind(&observation.payload_hash)
        .bind(observation.event_at_ms)
        .bind(ingested_at_ms)
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

fn row_to_observation(
    row: sqlx::sqlite::SqliteRow,
) -> Result<RoutingObservation, PersistenceError> {
    let scope_text = row.get::<String, _>("scope");
    let (station_key_id, station_id, model) =
        if let Some(value) = scope_text.strip_prefix("station_key:") {
            (Some(value.to_string()), None, None)
        } else if let Some(value) = scope_text.strip_prefix("station:") {
            (None, Some(value.to_string()), None)
        } else if let Some(value) = scope_text.strip_prefix("model:") {
            (None, None, Some(value.to_string()))
        } else {
            (None, None, None)
        };
    let evidence: Value = serde_json::from_str(&row.get::<String, _>("evidence_json"))
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    let endpoint_revision = evidence.get("endpoint_revision").and_then(Value::as_i64);
    Ok(RoutingObservation {
        id: row.get("id"),
        order: ObservationOrder {
            producer_id: row.get("producer_id"),
            producer_sequence: u64::try_from(row.get::<i64, _>("producer_sequence")).map_err(
                |_| PersistenceError::InvariantViolation("negative observation sequence".into()),
            )?,
            event_at_ms: row.get("event_at_ms"),
            ingested_at_ms: row.get("ingested_at_ms"),
        },
        scope: ObservationScope {
            station_id,
            station_key_id,
            model,
            endpoint_revision,
        },
        source: parse_source(&row.get::<String, _>("source"))?,
        traffic_equivalence: parse_traffic(&row.get::<String, _>("traffic_equivalence"))?,
        outcome: parse_outcome(&row.get::<String, _>("outcome_kind"))?,
        latency_ms: row
            .get::<Option<i64>, _>("latency_ms")
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    PersistenceError::InvariantViolation("invalid observation latency".into())
                })
            })
            .transpose()?,
        evidence_mass_basis_points: u16::try_from(
            row.get::<Option<i64>, _>("mass_basis_points").unwrap_or(0),
        )
        .map_err(|_| PersistenceError::InvariantViolation("invalid observation mass".into()))?,
    })
}

fn parse_source(value: &str) -> Result<ObservationSource, PersistenceError> {
    match value {
        "real_request" => Ok(ObservationSource::RealRequest),
        "active_probe" => Ok(ObservationSource::ActiveProbe),
        "administrative" => Ok(ObservationSource::Administrative),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown observation source".into(),
        )),
    }
}
fn parse_traffic(value: &str) -> Result<TrafficEquivalence, PersistenceError> {
    match value {
        "exact_request" => Ok(TrafficEquivalence::ExactRequest),
        "same_model_shape" => Ok(TrafficEquivalence::SameModelShape),
        "endpoint_only" => Ok(TrafficEquivalence::EndpointOnly),
        "anonymous" => Ok(TrafficEquivalence::Anonymous),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown observation traffic equivalence".into(),
        )),
    }
}
fn parse_outcome(value: &str) -> Result<ObservationOutcome, PersistenceError> {
    match value {
        "success" => Ok(ObservationOutcome::Success),
        "credential_failure" => Ok(ObservationOutcome::CredentialFailure),
        "endpoint_failure" => Ok(ObservationOutcome::EndpointFailure),
        "model_failure" => Ok(ObservationOutcome::ModelFailure),
        "rate_limited" => Ok(ObservationOutcome::RateLimited),
        "timeout" => Ok(ObservationOutcome::Timeout),
        "cancelled" => Ok(ObservationOutcome::Cancelled),
        "unknown" => Ok(ObservationOutcome::Unknown),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown observation outcome".into(),
        )),
    }
}

fn validate_observation(
    value: &RoutingObservationAppend,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    let values = [
        &value.id,
        &value.producer_id,
        &value.payload_hash,
        &value.scope,
        &value.source,
        &value.traffic_equivalence,
        &value.outcome_kind,
    ];
    if values
        .iter()
        .any(|text| text.is_empty() || text.len() > 192 || text.chars().any(char::is_control))
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

#[cfg(test)]
mod tests {
    use sqlx::Connection;

    use super::*;

    fn observation(id: &str, producer_sequence: u64, event_at_ms: i64) -> RoutingObservationAppend {
        RoutingObservationAppend {
            id: id.to_string(),
            producer_id: "test".to_string(),
            producer_sequence,
            payload_hash: "a".repeat(64),
            event_at_ms,
            ingested_at_ms: event_at_ms,
            scope: "station_key:key-1".to_string(),
            source: "real_request".to_string(),
            traffic_equivalence: "exact_request".to_string(),
            outcome_kind: "success".to_string(),
            latency_ms: Some(100),
            mass_basis_points: Some(10_000),
            evidence: serde_json::json!({ "endpoint_revision": 1 }),
        }
    }

    async fn connection() -> SqliteConnection {
        let mut connection = SqliteConnection::connect(":memory:")
            .await
            .expect("open sqlite");
        sqlx::query(
            "CREATE TABLE routing_observations (
                id TEXT PRIMARY KEY,
                producer_id TEXT NOT NULL,
                producer_sequence INTEGER NOT NULL,
                payload_hash TEXT NOT NULL,
                event_at_ms INTEGER NOT NULL,
                ingested_at_ms INTEGER NOT NULL,
                scope TEXT NOT NULL,
                source TEXT NOT NULL,
                traffic_equivalence TEXT NOT NULL,
                outcome_kind TEXT NOT NULL,
                latency_ms INTEGER,
                mass_basis_points INTEGER,
                evidence_json TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL,
                UNIQUE (producer_id, producer_sequence)
            )",
        )
        .execute(&mut connection)
        .await
        .expect("create routing observations");
        connection
    }

    #[tokio::test]
    async fn receive_clock_orders_late_events_after_already_persisted_observations() {
        let store = RoutingObservationStore;
        let mut connection = connection().await;
        store
            .append(
                &mut connection,
                &observation("newer-event", 1, 2_000),
                20_000,
            )
            .await
            .expect("append newer event");
        store
            .append(
                &mut connection,
                &observation("late-event", 2, 1_000),
                19_000,
            )
            .await
            .expect("append late event");

        let all = store
            .list_after(&mut connection, None, 8)
            .await
            .expect("list observations");
        assert_eq!(all[0].id, "newer-event");
        assert_eq!(all[0].order.ingested_at_ms, 20_000);
        assert_eq!(all[1].id, "late-event");
        assert_eq!(all[1].order.ingested_at_ms, 20_001);

        let after_first = store
            .list_after(
                &mut connection,
                Some((all[0].order.ingested_at_ms, all[0].id.clone())),
                8,
            )
            .await
            .expect("resume after cursor");
        assert_eq!(
            after_first
                .iter()
                .map(|value| value.id.as_str())
                .collect::<Vec<_>>(),
            ["late-event"]
        );
    }

    #[tokio::test]
    async fn scope_history_is_not_limited_by_the_ingestion_cursor_batch_size() {
        let store = RoutingObservationStore;
        let mut connection = connection().await;
        for index in 0..300_u64 {
            store
                .append(
                    &mut connection,
                    &observation(&format!("observation-{index}"), index, index as i64),
                    (index + 1) as i64,
                )
                .await
                .expect("append observation");
        }

        let history = store
            .list_for_scope(&mut connection, "station_key:key-1")
            .await
            .expect("load complete scope history");
        assert_eq!(history.len(), 300);
        assert_eq!(history.first().map(|value| value.id.as_str()), Some("observation-0"));
        assert_eq!(history.last().map(|value| value.id.as_str()), Some("observation-299"));
    }
}
