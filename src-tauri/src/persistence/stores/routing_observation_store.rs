use std::collections::BTreeMap;

use serde_json::Value;
use sqlx::{Row, SqliteConnection};

use crate::application::health_protection::HealthProtectionScope;
use crate::models::routing_observation::{
    EventTimeStatus, FailureAttribution, ObservationOrder, ObservationOutcome,
    ObservationRetryDisposition, ObservationScope, ObservationSource, RecoveryOrigin,
    ResponseOrigin, RoutingObservation, TrafficEquivalence,
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
    pub(crate) comparability_key: Option<String>,
    pub(crate) evidence: Value,
    pub(crate) correlation_id: String,
    pub(crate) attempt_index: u16,
    pub(crate) station_key_lifecycle_revision: u64,
    pub(crate) cluster_finalized: bool,
    pub(crate) cluster_expected_attempt_count: u16,
    pub(crate) boundary_crossed: bool,
    pub(crate) event_time_status: EventTimeStatus,
    pub(crate) response_origin: String,
    pub(crate) failure_code: Option<String>,
    pub(crate) failure_attribution: String,
    pub(crate) recovery_origin: String,
    pub(crate) retry_disposition: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObservationAppendResult {
    Inserted,
    Existing,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingObservationStore;

/// A projection row paired with the global receive-order cursor allocated by
/// migration 0061.  The domain model intentionally keeps producer sequence
/// separate; the runner must never use that producer-local value as a
/// generation watermark.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoutingObservationCursorRow {
    pub(crate) observation: RoutingObservation,
    pub(crate) ingestion_sequence: u64,
}

impl RoutingObservationStore {
    /// Applies the one-time compatibility mapping for observations whose Key
    /// lifecycle was advanced by the historical group-rate projection bug.
    /// The immutable fact remains unchanged; a genuine later credential
    /// change advances beyond the alias target and therefore starts fresh.
    pub(crate) async fn apply_quality_lifecycle_aliases(
        &self,
        connection: &mut SqliteConnection,
        observations: &mut [RoutingObservation],
    ) -> Result<(), PersistenceError> {
        let station_key_ids = observations
            .iter()
            .filter_map(|observation| observation.scope.station_key_id.as_deref())
            .filter(|station_key_id| !station_key_id.is_empty())
            .collect::<std::collections::BTreeSet<_>>();
        if station_key_ids.is_empty() {
            return Ok(());
        }
        let placeholders = (1..=station_key_ids.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "SELECT station_key_id, target_lifecycle_revision
             FROM routing_quality_lifecycle_alias_v1
             WHERE station_key_id IN ({placeholders})"
        );
        let mut statement = sqlx::query(&query);
        for station_key_id in station_key_ids {
            statement = statement.bind(station_key_id);
        }
        let aliases = statement.fetch_all(&mut *connection).await?;
        let aliases = aliases
            .into_iter()
            .map(|row| {
                let revision = row.get::<i64, _>("target_lifecycle_revision");
                let revision = u64::try_from(revision).map_err(|_| {
                    PersistenceError::InvariantViolation(
                        "quality lifecycle alias revision is negative".into(),
                    )
                })?;
                Ok((row.get::<String, _>("station_key_id"), revision))
            })
            .collect::<Result<BTreeMap<_, _>, PersistenceError>>()?;
        for observation in observations {
            if let Some(station_key_id) = observation.scope.station_key_id.as_deref() {
                if let Some(target_revision) = aliases.get(station_key_id) {
                    if observation.station_key_lifecycle_revision < *target_revision {
                        observation.station_key_lifecycle_revision = *target_revision;
                    }
                }
            }
        }
        Ok(())
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-observation-cursor; owner=persistence/stores/routing_observation_store; remove_when=pre-v3 observation readers are retired"
        )
    )]
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
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-observation-history-read; owner=persistence/stores/routing_observation_store; remove_when=pre-v3 observation readers are retired"
        )
    )]
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

    /// Returns complete histories for a bounded set of projection scopes in a
    /// single query.  Projection must use this method instead of calling
    /// `list_for_scope` once per key: the latter creates an N+1 query pattern
    /// when a refresh touches many keys.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-observation-history-read; owner=persistence/stores/routing_observation_store; remove_when=pre-v3 observation readers are retired"
        )
    )]
    pub async fn list_for_scopes(
        &self,
        connection: &mut SqliteConnection,
        scopes: &[String],
    ) -> Result<Vec<RoutingObservation>, PersistenceError> {
        let mut scopes = scopes
            .iter()
            .filter(|scope| !scope.is_empty())
            .take(1024)
            .cloned()
            .collect::<Vec<_>>();
        scopes.sort();
        scopes.dedup();
        if scopes.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = (1..=scopes.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, producer_id, producer_sequence, event_at_ms, ingested_at_ms, scope, source, traffic_equivalence, outcome_kind, latency_ms, mass_basis_points, evidence_json FROM routing_observations WHERE scope IN ({placeholders}) ORDER BY scope ASC, ingested_at_ms ASC, id ASC"
        );
        let mut query = sqlx::query(&sql);
        for scope in &scopes {
            query = query.bind(scope);
        }
        let rows = query.fetch_all(&mut *connection).await?;
        rows.into_iter().map(row_to_observation).collect()
    }

    /// Read active-generation observations after the shared ingestion cursor.
    /// This is the v3 counterpart to `list_after`; keeping it separate lets
    /// old migration fixtures continue to exercise the compatibility reader.
    pub(crate) async fn list_after_v3(
        &self,
        connection: &mut SqliteConnection,
        after: Option<(i64, String)>,
        limit: usize,
    ) -> Result<Vec<RoutingObservationCursorRow>, PersistenceError> {
        let limit = i64::try_from(limit.clamp(1, 512))
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        let rows = sqlx::query(
            "SELECT id, producer_id, producer_sequence, event_at_ms, ingested_at_ms, scope, source, traffic_equivalence, outcome_kind, latency_ms, mass_basis_points, evidence_json, ingestion_sequence AS v3_ingestion_sequence, event_id AS v3_event_id, attempt_id AS v3_attempt_id, correlation_id AS v3_correlation_id, station_key_id AS v3_station_key_id, station_key_lifecycle_revision AS v3_lifecycle_revision, attempt_index AS v3_attempt_index, boundary_crossed AS v3_boundary_crossed, response_origin AS v3_response_origin, event_time_status AS v3_event_time_status, outcome AS v3_outcome, failure_code AS v3_failure_code, failure_attribution AS v3_failure_attribution, recovery_origin AS v3_recovery_origin, retry_disposition AS v3_retry_disposition, ttft_ms AS v3_ttft_ms, comparability_key AS v3_comparability_key, cluster_finalized AS v3_cluster_finalized, cluster_expected_attempt_count AS v3_cluster_expected_attempt_count, generation_eligibility AS v3_generation_eligibility FROM routing_observations WHERE generation_eligibility = 'active' AND ingestion_sequence IS NOT NULL AND (?1 IS NULL OR ingestion_sequence > ?1 OR (ingestion_sequence = ?1 AND (?2 IS NULL OR id > ?2))) ORDER BY ingestion_sequence ASC, id ASC LIMIT ?3",
        )
        .bind(after.as_ref().map(|value| value.0))
        .bind(after.as_ref().map(|value| value.1.as_str()))
        .bind(limit)
        .fetch_all(&mut *connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                let sequence = row
                    .get::<Option<i64>, _>("v3_ingestion_sequence")
                    .ok_or_else(|| {
                        PersistenceError::InvariantViolation(
                            "active v3 observation has no ingestion sequence".into(),
                        )
                    })?;
                let sequence = u64::try_from(sequence).map_err(|_| {
                    PersistenceError::InvariantViolation(
                        "active v3 observation has a negative ingestion sequence".into(),
                    )
                })?;
                Ok(RoutingObservationCursorRow {
                    observation: row_to_observation_v3(row)?,
                    ingestion_sequence: sequence,
                })
            })
            .collect()
    }

    /// Read all active-generation observations for a bounded scope set in one
    /// query.  The result is ordered by scope and shared ingestion cursor so
    /// a runner can group it without issuing one query per Key.
    pub(crate) async fn list_for_scopes_v3(
        &self,
        connection: &mut SqliteConnection,
        scopes: &[String],
    ) -> Result<Vec<RoutingObservation>, PersistenceError> {
        self.list_for_scopes_v3_bounded(connection, scopes, None, false)
            .await
    }

    /// Reads generation-eligible immutable evidence through an inclusive
    /// shared ingestion watermark. Rebuilds include `next` rows because the
    /// watermark, not the current active pointer, owns their generation.
    pub(crate) async fn list_for_scopes_v3_through(
        &self,
        connection: &mut SqliteConnection,
        scopes: &[String],
        watermark: Option<u64>,
    ) -> Result<Vec<RoutingObservation>, PersistenceError> {
        self.list_for_scopes_v3_bounded(connection, scopes, watermark, true)
            .await
    }

    /// Reads the immutable input owned by one generation. Active evidence is
    /// admitted through the final drain watermark, while `next` evidence is
    /// capped at the watermark captured before the persistent fence started.
    pub(crate) async fn list_for_scopes_v3_for_generation(
        &self,
        connection: &mut SqliteConnection,
        scopes: &[String],
        watermark: u64,
        next_watermark: u64,
    ) -> Result<Vec<RoutingObservation>, PersistenceError> {
        let mut scopes = scopes
            .iter()
            .filter(|scope| !scope.is_empty())
            .take(1024)
            .cloned()
            .collect::<Vec<_>>();
        scopes.sort();
        scopes.dedup();
        if scopes.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = (1..=scopes.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let watermark_index = scopes.len() + 1;
        let next_watermark_index = scopes.len() + 2;
        let sql = format!(
            "SELECT id, producer_id, producer_sequence, event_at_ms, ingested_at_ms, scope, source, traffic_equivalence, outcome_kind, latency_ms, mass_basis_points, evidence_json, ingestion_sequence AS v3_ingestion_sequence, event_id AS v3_event_id, attempt_id AS v3_attempt_id, correlation_id AS v3_correlation_id, station_key_id AS v3_station_key_id, station_key_lifecycle_revision AS v3_lifecycle_revision, attempt_index AS v3_attempt_index, boundary_crossed AS v3_boundary_crossed, response_origin AS v3_response_origin, event_time_status AS v3_event_time_status, outcome AS v3_outcome, failure_code AS v3_failure_code, failure_attribution AS v3_failure_attribution, recovery_origin AS v3_recovery_origin, retry_disposition AS v3_retry_disposition, ttft_ms AS v3_ttft_ms, comparability_key AS v3_comparability_key, cluster_finalized AS v3_cluster_finalized, cluster_expected_attempt_count AS v3_cluster_expected_attempt_count, generation_eligibility AS v3_generation_eligibility FROM routing_observations WHERE ingestion_sequence IS NOT NULL AND scope IN ({placeholders}) AND ingestion_sequence <= ?{watermark_index} AND (generation_eligibility = 'active' OR (generation_eligibility = 'next' AND ingestion_sequence <= ?{next_watermark_index})) ORDER BY scope ASC, ingestion_sequence ASC, id ASC"
        );
        let mut query = sqlx::query(&sql);
        for scope in &scopes {
            query = query.bind(scope);
        }
        query = query
            .bind(i64::try_from(watermark).map_err(|_| PersistenceError::ConstraintViolation)?)
            .bind(
                i64::try_from(next_watermark).map_err(|_| PersistenceError::ConstraintViolation)?,
            );
        let rows = query.fetch_all(&mut *connection).await?;
        rows.into_iter().map(row_to_observation_v3).collect()
    }

    pub(crate) async fn list_v3_generation_cursor(
        &self,
        connection: &mut SqliteConnection,
        watermark: u64,
        next_watermark: u64,
        after_station_key_id: Option<&str>,
        after_observation_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RoutingObservation>, PersistenceError> {
        let limit = i64::try_from(limit.clamp(1, 1_024))
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        let rows = sqlx::query(
            "SELECT o.id, o.producer_id, o.producer_sequence, o.event_at_ms,
                    o.ingested_at_ms, o.scope, o.source, o.traffic_equivalence,
                    o.outcome_kind, o.latency_ms, o.mass_basis_points,
                    o.evidence_json, o.ingestion_sequence AS v3_ingestion_sequence,
                    o.event_id AS v3_event_id, o.attempt_id AS v3_attempt_id,
                    o.correlation_id AS v3_correlation_id,
                    o.station_key_id AS v3_station_key_id,
                    o.station_key_lifecycle_revision AS v3_lifecycle_revision,
                    o.attempt_index AS v3_attempt_index,
                    o.boundary_crossed AS v3_boundary_crossed,
                    o.response_origin AS v3_response_origin,
                    o.event_time_status AS v3_event_time_status,
                    o.outcome AS v3_outcome, o.failure_code AS v3_failure_code,
                    o.failure_attribution AS v3_failure_attribution,
                    o.recovery_origin AS v3_recovery_origin,
                    o.retry_disposition AS v3_retry_disposition,
                    o.ttft_ms AS v3_ttft_ms,
                    o.comparability_key AS v3_comparability_key,
                    o.cluster_finalized AS v3_cluster_finalized,
                    o.cluster_expected_attempt_count AS v3_cluster_expected_attempt_count,
                    o.generation_eligibility AS v3_generation_eligibility
             FROM routing_observations o
             JOIN station_keys k ON k.id = o.station_key_id
             JOIN domain_revisions r ON r.scope = 'station_key:' || k.id
             WHERE o.ingestion_sequence IS NOT NULL
               AND o.ingestion_sequence <= ?1
               AND (o.generation_eligibility = 'active'
                    OR (o.generation_eligibility = 'next'
                        AND o.ingestion_sequence <= ?2))
               AND (?3 IS NULL OR o.station_key_id > ?3
                    OR (o.station_key_id = ?3 AND (?4 IS NULL OR o.id > ?4)))
             ORDER BY o.station_key_id, o.id
             LIMIT ?5",
        )
        .bind(i64::try_from(watermark).map_err(|_| PersistenceError::ConstraintViolation)?)
        .bind(i64::try_from(next_watermark).map_err(|_| PersistenceError::ConstraintViolation)?)
        .bind(after_station_key_id)
        .bind(after_observation_id)
        .bind(limit)
        .fetch_all(&mut *connection)
        .await?;
        rows.into_iter().map(row_to_observation_v3).collect()
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=v3-generation-observation-read; owner=persistence/stores/routing_observation_store; remove_when=generation rebuild uses only the global cursor reader"
        )
    )]
    pub(crate) async fn list_v3_generation_key_cursor(
        &self,
        connection: &mut SqliteConnection,
        station_key_id: &str,
        watermark: u64,
        next_watermark: u64,
        after_observation_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RoutingObservation>, PersistenceError> {
        if station_key_id.is_empty() {
            return Err(PersistenceError::ConstraintViolation);
        }
        let limit = i64::try_from(limit.clamp(1, 1_024))
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        let rows = sqlx::query(
            "SELECT id, producer_id, producer_sequence, event_at_ms,
                    ingested_at_ms, scope, source, traffic_equivalence,
                    outcome_kind, latency_ms, mass_basis_points, evidence_json,
                    ingestion_sequence AS v3_ingestion_sequence,
                    event_id AS v3_event_id, attempt_id AS v3_attempt_id,
                    correlation_id AS v3_correlation_id,
                    station_key_id AS v3_station_key_id,
                    station_key_lifecycle_revision AS v3_lifecycle_revision,
                    attempt_index AS v3_attempt_index,
                    boundary_crossed AS v3_boundary_crossed,
                    response_origin AS v3_response_origin,
                    event_time_status AS v3_event_time_status,
                    outcome AS v3_outcome, failure_code AS v3_failure_code,
                    failure_attribution AS v3_failure_attribution,
                    recovery_origin AS v3_recovery_origin,
                    retry_disposition AS v3_retry_disposition,
                    ttft_ms AS v3_ttft_ms,
                    comparability_key AS v3_comparability_key,
                    cluster_finalized AS v3_cluster_finalized,
                    cluster_expected_attempt_count AS v3_cluster_expected_attempt_count,
                    generation_eligibility AS v3_generation_eligibility
             FROM routing_observations
             WHERE station_key_id = ?1 AND ingestion_sequence IS NOT NULL
               AND ingestion_sequence <= ?2
               AND (generation_eligibility = 'active'
                    OR (generation_eligibility = 'next'
                        AND ingestion_sequence <= ?3))
               AND (?4 IS NULL OR id > ?4)
             ORDER BY id
             LIMIT ?5",
        )
        .bind(station_key_id)
        .bind(i64::try_from(watermark).map_err(|_| PersistenceError::ConstraintViolation)?)
        .bind(i64::try_from(next_watermark).map_err(|_| PersistenceError::ConstraintViolation)?)
        .bind(after_observation_id)
        .bind(limit)
        .fetch_all(&mut *connection)
        .await?;
        rows.into_iter().map(row_to_observation_v3).collect()
    }

    async fn list_for_scopes_v3_bounded(
        &self,
        connection: &mut SqliteConnection,
        scopes: &[String],
        watermark: Option<u64>,
        include_next: bool,
    ) -> Result<Vec<RoutingObservation>, PersistenceError> {
        let mut scopes = scopes
            .iter()
            .filter(|scope| !scope.is_empty())
            .take(1024)
            .cloned()
            .collect::<Vec<_>>();
        scopes.sort();
        scopes.dedup();
        if scopes.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = (1..=scopes.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let watermark_index = scopes.len() + 1;
        let eligibility_filter = if include_next {
            "generation_eligibility IN ('active', 'next')"
        } else {
            "generation_eligibility = 'active'"
        };
        let sql = format!(
            "SELECT id, producer_id, producer_sequence, event_at_ms, ingested_at_ms, scope, source, traffic_equivalence, outcome_kind, latency_ms, mass_basis_points, evidence_json, ingestion_sequence AS v3_ingestion_sequence, event_id AS v3_event_id, attempt_id AS v3_attempt_id, correlation_id AS v3_correlation_id, station_key_id AS v3_station_key_id, station_key_lifecycle_revision AS v3_lifecycle_revision, attempt_index AS v3_attempt_index, boundary_crossed AS v3_boundary_crossed, response_origin AS v3_response_origin, event_time_status AS v3_event_time_status, outcome AS v3_outcome, failure_code AS v3_failure_code, failure_attribution AS v3_failure_attribution, recovery_origin AS v3_recovery_origin, retry_disposition AS v3_retry_disposition, ttft_ms AS v3_ttft_ms, comparability_key AS v3_comparability_key, cluster_finalized AS v3_cluster_finalized, cluster_expected_attempt_count AS v3_cluster_expected_attempt_count, generation_eligibility AS v3_generation_eligibility FROM routing_observations WHERE {eligibility_filter} AND ingestion_sequence IS NOT NULL AND scope IN ({placeholders}) AND (?{watermark_index} IS NULL OR ingestion_sequence <= ?{watermark_index}) ORDER BY scope ASC, ingestion_sequence ASC, id ASC"
        );
        let mut query = sqlx::query(&sql);
        for scope in &scopes {
            query = query.bind(scope);
        }
        query = query.bind(
            watermark
                .map(i64::try_from)
                .transpose()
                .map_err(|_| PersistenceError::ConstraintViolation)?,
        );
        let rows = query.fetch_all(&mut *connection).await?;
        rows.into_iter().map(row_to_observation_v3).collect()
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-observation-append; owner=persistence/stores/routing_observation_store; remove_when=all observation writes use the v3 ingestion coordinator"
        )
    )]
    pub(crate) async fn append(
        &self,
        connection: &mut SqliteConnection,
        observation: &RoutingObservationAppend,
        now_ms: i64,
    ) -> Result<ObservationAppendResult, PersistenceError> {
        self.append_with_generation_eligibility(connection, observation, None, now_ms)
            .await
    }

    pub(crate) async fn append_with_generation_eligibility(
        &self,
        connection: &mut SqliteConnection,
        observation: &RoutingObservationAppend,
        generation_eligibility_override: Option<&str>,
        now_ms: i64,
    ) -> Result<ObservationAppendResult, PersistenceError> {
        validate_observation(observation, now_ms)?;
        if generation_eligibility_override
            .is_some_and(|value| !matches!(value, "active" | "next" | "legacy"))
        {
            return Err(PersistenceError::ConstraintViolation);
        }
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
        let v3_columns = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('routing_observations') WHERE name IN ('event_id', 'candidate_admitted', 'response_origin', 'failure_attribution', 'retry_disposition', 'algorithm_version', 'source_weight_revision', 'quality_policy_revision', 'cluster_finalized_at_ms', 'cluster_finalization_reason')",
        )
        .fetch_one(&mut *connection)
        .await?
            == 10;
        let generation_eligibility = if let Some(eligibility) = generation_eligibility_override {
            eligibility
        } else if v3_columns {
            let marker_table_exists = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'routing_runtime_cutover_marker'",
            )
            .fetch_one(&mut *connection)
            .await?
                == 1;
            if marker_table_exists {
                super::routing_generation_store::RoutingGenerationStore
                    .load_ingestion_fence(connection)
                    .await?
                    .eligibility
                    .as_str()
            } else {
                // Compatibility-only unit schemas predate the generation
                // registry. Production schema 63 always takes the branch
                // above and therefore participates in the transaction fence.
                "active"
            }
        } else {
            "legacy"
        };
        let station_key_id = observation
            .scope
            .strip_prefix("station_key:")
            .unwrap_or_default();
        if v3_columns
            && !observation.correlation_id.is_empty()
            && observation.station_key_lifecycle_revision > 0
            && !station_key_id.is_empty()
            && matches!(observation.source.as_str(), "real_request" | "active_probe")
        {
            let outcome = if observation.outcome_kind == "success" {
                "success"
            } else if observation.boundary_crossed && observation.failure_attribution == "key" {
                "attributable_failure"
            } else {
                "excluded"
            };
            let event_time_status = match observation.event_time_status {
                EventTimeStatus::Valid => "valid",
                EventTimeStatus::Missing => "missing",
                EventTimeStatus::Invalid => "invalid",
            };
            let finalized_at_ms = observation
                .cluster_finalized
                .then_some(observation.event_at_ms);
            sqlx::query(
                "INSERT INTO routing_observations (id, producer_id, producer_sequence, payload_hash, event_at_ms, ingested_at_ms, scope, source, traffic_equivalence, outcome_kind, latency_ms, mass_basis_points, evidence_json, created_at_ms, event_id, attempt_id, correlation_id, station_key_id, station_key_lifecycle_revision, attempt_index, candidate_admitted, candidate_admitted_at_ms, boundary_crossed, response_origin, event_time_status, outcome, failure_code, failure_attribution, comparability_key, observed_at_ms, recovery_origin, retry_disposition, algorithm_version, source_weight_revision, quality_policy_revision, generation_eligibility, cluster_finalized, cluster_expected_attempt_count, cluster_finalized_at_ms, cluster_finalization_reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?16, ?17, ?18, ?19, 1, ?5, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?5, ?27, ?28, 'routing_quality_v3', 1, 1, ?29, ?30, ?31, ?32, ?33)",
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
            .bind(&observation.id)
            .bind(&observation.correlation_id)
            .bind(station_key_id)
            .bind(i64::try_from(observation.station_key_lifecycle_revision).map_err(|_| PersistenceError::ConstraintViolation)?)
            .bind(i64::from(observation.attempt_index))
            .bind(if observation.boundary_crossed { 1_i64 } else { 0_i64 })
            .bind(&observation.response_origin)
            .bind(event_time_status)
            .bind(outcome)
            .bind(&observation.failure_code)
            .bind(&observation.failure_attribution)
            .bind(&observation.comparability_key)
            .bind(&observation.recovery_origin)
            .bind(&observation.retry_disposition)
            .bind(generation_eligibility)
            .bind(if observation.cluster_finalized { 1_i64 } else { 0_i64 })
            .bind(i64::from(observation.cluster_expected_attempt_count))
            .bind(finalized_at_ms)
            .bind(if observation.cluster_finalized {
                Some("attempt_terminal")
            } else {
                None
            })
            .execute(&mut *connection)
            .await?;
        } else {
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
        }
        Ok(ObservationAppendResult::Inserted)
    }
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=legacy-observation-decoder; owner=persistence/stores/routing_observation_store; remove_when=pre-v3 observation rows are no longer read"
    )
)]
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
    let comparability_key = evidence
        .get("comparability_key")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let probe_scope = evidence
        .get("probe_scope")
        .cloned()
        .filter(|value| !value.is_null())
        .map(serde_json::from_value::<HealthProtectionScope>)
        .transpose()
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    let probe_state_revision = evidence.get("probe_state_revision").and_then(Value::as_u64);
    let correlation_id = evidence
        .get("correlation_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let attempt_index = evidence
        .get("attempt_index")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(0);
    let station_key_lifecycle_revision = evidence
        .get("station_key_lifecycle_revision")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let cluster_finalized = evidence
        .get("cluster_finalized")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let cluster_expected_attempt_count = evidence
        .get("cluster_expected_attempt_count")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(1);
    let boundary_crossed = evidence
        .get("boundary_crossed")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let event_time_status = evidence
        .get("event_time_status")
        .and_then(Value::as_str)
        .map(parse_event_time_status)
        .transpose()?
        .unwrap_or(EventTimeStatus::Valid);
    let response_origin = evidence
        .get("response_origin")
        .and_then(Value::as_str)
        .map(parse_response_origin)
        .transpose()?
        .unwrap_or_default();
    let failure_code = evidence
        .get("failure_code")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let failure_attribution = evidence
        .get("failure_attribution")
        .and_then(Value::as_str)
        .map(parse_failure_attribution)
        .transpose()?
        .unwrap_or_default();
    let recovery_origin = evidence
        .get("recovery_origin")
        .and_then(Value::as_str)
        .map(parse_recovery_origin)
        .transpose()?
        .unwrap_or_default();
    let retry_disposition = evidence
        .get("retry_disposition")
        .and_then(Value::as_str)
        .map(parse_retry_disposition)
        .transpose()?
        .unwrap_or_default();
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
        comparability_key,
        correlation_id,
        attempt_index,
        station_key_lifecycle_revision,
        cluster_finalized,
        cluster_expected_attempt_count,
        boundary_crossed,
        event_time_status,
        response_origin,
        failure_code,
        failure_attribution,
        recovery_origin,
        retry_disposition,
        probe_scope,
        probe_state_revision,
    })
}

fn row_to_observation_v3(
    row: sqlx::sqlite::SqliteRow,
) -> Result<RoutingObservation, PersistenceError> {
    let scope_text = row.get::<String, _>("scope");
    let evidence: Value = serde_json::from_str(&row.get::<String, _>("evidence_json"))
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    let station_key_id = row
        .get::<Option<String>, _>("v3_station_key_id")
        .or_else(|| {
            scope_text
                .strip_prefix("station_key:")
                .map(ToOwned::to_owned)
        });
    let station_id = if station_key_id.is_none() {
        scope_text.strip_prefix("station:").map(ToOwned::to_owned)
    } else {
        None
    };
    let model = if station_key_id.is_none() && station_id.is_none() {
        scope_text.strip_prefix("model:").map(ToOwned::to_owned)
    } else {
        None
    };
    let endpoint_revision = evidence.get("endpoint_revision").and_then(Value::as_i64);
    let comparability_key = row
        .get::<Option<String>, _>("v3_comparability_key")
        .or_else(|| {
            evidence
                .get("comparability_key")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        });
    // Probe metadata is intentionally kept in evidence for now.  It is not a
    // v3 observation identity column in migration 0061, so selecting a
    // non-existent SQL column here would make every projection tick fail at
    // runtime.  The evidence fallback also preserves rows written before the
    // structured observation writer gained probe metadata columns.
    let probe_scope = evidence
        .get("probe_scope")
        .cloned()
        .filter(|value| !value.is_null())
        .map(serde_json::from_value::<HealthProtectionScope>)
        .transpose()
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    let probe_state_revision = evidence.get("probe_state_revision").and_then(Value::as_u64);
    let correlation_id = row
        .get::<Option<String>, _>("v3_correlation_id")
        .or_else(|| {
            evidence
                .get("correlation_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_default();
    let attempt_index = row
        .get::<Option<i64>, _>("v3_attempt_index")
        .and_then(|value| u16::try_from(value).ok())
        .or_else(|| {
            evidence
                .get("attempt_index")
                .and_then(Value::as_u64)
                .and_then(|value| u16::try_from(value).ok())
        })
        .unwrap_or(0);
    let station_key_lifecycle_revision = row
        .get::<Option<i64>, _>("v3_lifecycle_revision")
        .and_then(|value| u64::try_from(value).ok())
        .or_else(|| {
            evidence
                .get("station_key_lifecycle_revision")
                .and_then(Value::as_u64)
        })
        .unwrap_or(0);
    let cluster_finalized = row
        .get::<Option<i64>, _>("v3_cluster_finalized")
        .map(|value| value != 0)
        .or_else(|| evidence.get("cluster_finalized").and_then(Value::as_bool))
        .unwrap_or(true);
    let cluster_expected_attempt_count = row
        .get::<Option<i64>, _>("v3_cluster_expected_attempt_count")
        .and_then(|value| u16::try_from(value).ok())
        .or_else(|| {
            evidence
                .get("cluster_expected_attempt_count")
                .and_then(Value::as_u64)
                .and_then(|value| u16::try_from(value).ok())
        })
        .unwrap_or(1);
    let boundary_crossed = row
        .get::<Option<i64>, _>("v3_boundary_crossed")
        .map(|value| value != 0)
        .or_else(|| evidence.get("boundary_crossed").and_then(Value::as_bool))
        .unwrap_or(true);
    let event_time_status = row
        .get::<Option<String>, _>("v3_event_time_status")
        .as_deref()
        .map(parse_event_time_status_v3)
        .transpose()?
        .or_else(|| {
            evidence
                .get("event_time_status")
                .and_then(Value::as_str)
                .map(parse_event_time_status_v3)
                .transpose()
                .ok()
                .flatten()
        })
        .unwrap_or(EventTimeStatus::Invalid);
    let outcome = row
        .get::<Option<String>, _>("v3_outcome")
        .as_deref()
        .and_then(|value| {
            v3_outcome(
                value,
                row.get::<Option<String>, _>("v3_failure_code").as_deref(),
            )
        })
        .or_else(|| parse_outcome(&row.get::<String, _>("outcome_kind")).ok())
        .unwrap_or(ObservationOutcome::Unknown);
    let response_origin = row
        .get::<Option<String>, _>("v3_response_origin")
        .as_deref()
        .map(parse_response_origin)
        .transpose()?
        .unwrap_or_default();
    let failure_code = row.get::<Option<String>, _>("v3_failure_code");
    let failure_attribution = row
        .get::<Option<String>, _>("v3_failure_attribution")
        .as_deref()
        .map(parse_failure_attribution)
        .transpose()?
        .unwrap_or_default();
    let recovery_origin = row
        .get::<Option<String>, _>("v3_recovery_origin")
        .as_deref()
        .map(parse_recovery_origin)
        .transpose()?
        .unwrap_or_default();
    let retry_disposition = row
        .get::<Option<String>, _>("v3_retry_disposition")
        .as_deref()
        .map(parse_retry_disposition)
        .transpose()?
        .unwrap_or_default();
    let source = parse_source(&row.get::<String, _>("source"))?;
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
        source,
        traffic_equivalence: parse_traffic(&row.get::<String, _>("traffic_equivalence"))?,
        outcome,
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
        comparability_key,
        correlation_id,
        attempt_index,
        station_key_lifecycle_revision,
        cluster_finalized,
        cluster_expected_attempt_count,
        boundary_crossed,
        event_time_status,
        response_origin,
        failure_code,
        failure_attribution,
        recovery_origin,
        retry_disposition,
        probe_scope,
        probe_state_revision,
    })
}

fn parse_response_origin(value: &str) -> Result<ResponseOrigin, PersistenceError> {
    match value {
        "upstream" | "Upstream" => Ok(ResponseOrigin::Upstream),
        "relay" | "Relay" => Ok(ResponseOrigin::Relay),
        "unknown" | "Unknown" => Ok(ResponseOrigin::Unknown),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown observation response origin".into(),
        )),
    }
}

fn parse_failure_attribution(value: &str) -> Result<FailureAttribution, PersistenceError> {
    match value {
        "key" | "Key" => Ok(FailureAttribution::Key),
        "local" | "Local" => Ok(FailureAttribution::Local),
        "client" | "Client" => Ok(FailureAttribution::Client),
        "unknown" | "Unknown" => Ok(FailureAttribution::Unknown),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown observation failure attribution".into(),
        )),
    }
}

fn parse_recovery_origin(value: &str) -> Result<RecoveryOrigin, PersistenceError> {
    match value {
        "normal" | "Normal" => Ok(RecoveryOrigin::Normal),
        "crash_recovery" | "CrashRecovery" => Ok(RecoveryOrigin::CrashRecovery),
        "lease_reaper" | "LeaseReaper" => Ok(RecoveryOrigin::LeaseReaper),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown observation recovery origin".into(),
        )),
    }
}

fn parse_retry_disposition(value: &str) -> Result<ObservationRetryDisposition, PersistenceError> {
    match value {
        "end" | "End" => Ok(ObservationRetryDisposition::End),
        "retryable_before_commit" | "RetryableBeforeCommit" => {
            Ok(ObservationRetryDisposition::RetryableBeforeCommit)
        }
        "stop_request" | "StopRequest" => Ok(ObservationRetryDisposition::StopRequest),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown observation retry disposition".into(),
        )),
    }
}

fn parse_event_time_status_v3(value: &str) -> Result<EventTimeStatus, PersistenceError> {
    match value {
        "valid" | "Valid" => Ok(EventTimeStatus::Valid),
        "missing" | "Missing" => Ok(EventTimeStatus::Missing),
        "invalid" | "Invalid" | "legacy" | "Legacy" => Ok(EventTimeStatus::Invalid),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown observation event time status".into(),
        )),
    }
}

fn v3_outcome(value: &str, failure_code: Option<&str>) -> Option<ObservationOutcome> {
    match value {
        "success" => Some(ObservationOutcome::Success),
        "excluded" => Some(ObservationOutcome::Unknown),
        "attributable_failure" => Some(match failure_code.unwrap_or_default() {
            "rate_limited" | "too_many_requests" | "429" => ObservationOutcome::RateLimited,
            "credential_failure" | "authentication" | "auth" => {
                ObservationOutcome::CredentialFailure
            }
            "model_failure" | "model" => ObservationOutcome::ModelFailure,
            "timeout" => ObservationOutcome::Timeout,
            _ => ObservationOutcome::EndpointFailure,
        }),
        _ => None,
    }
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=legacy-observation-time-status; owner=persistence/stores/routing_observation_store; remove_when=pre-v3 observation rows are no longer read"
    )
)]
fn parse_event_time_status(value: &str) -> Result<EventTimeStatus, PersistenceError> {
    match value {
        "valid" | "Valid" => Ok(EventTimeStatus::Valid),
        "missing" | "Missing" => Ok(EventTimeStatus::Missing),
        "invalid" | "Invalid" => Ok(EventTimeStatus::Invalid),
        _ => Err(PersistenceError::InvariantViolation(
            "unknown observation event time status".into(),
        )),
    }
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
        || !matches!(
            value.response_origin.as_str(),
            "upstream" | "relay" | "unknown"
        )
        || !matches!(
            value.failure_attribution.as_str(),
            "key" | "local" | "client" | "unknown"
        )
        || !matches!(
            value.recovery_origin.as_str(),
            "normal" | "crash_recovery" | "lease_reaper"
        )
        || !matches!(
            value.retry_disposition.as_str(),
            "end" | "retryable_before_commit" | "stop_request"
        )
        || value.failure_code.as_deref().is_some_and(|code| {
            code.is_empty() || code.len() > 96 || code.chars().any(char::is_control)
        })
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::{Connection, SqliteConnection};

    use crate::persistence::runtime::PersistenceRuntime;

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
            comparability_key: None,
            evidence: serde_json::json!({ "endpoint_revision": 1 }),
            correlation_id: format!("corr-{id}"),
            attempt_index: 0,
            station_key_lifecycle_revision: 1,
            cluster_finalized: true,
            cluster_expected_attempt_count: 1,
            boundary_crossed: true,
            event_time_status: EventTimeStatus::Valid,
            response_origin: "upstream".to_string(),
            failure_code: None,
            failure_attribution: "key".to_string(),
            recovery_origin: "normal".to_string(),
            retry_disposition: "end".to_string(),
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
        assert_eq!(
            history.first().map(|value| value.id.as_str()),
            Some("observation-0")
        );
        assert_eq!(
            history.last().map(|value| value.id.as_str()),
            Some("observation-299")
        );
    }

    #[tokio::test]
    async fn batch_scope_history_reads_multiple_scopes_in_one_result() {
        let store = RoutingObservationStore;
        let mut connection = connection().await;
        let mut second = observation("observation-2", 2, 2);
        second.scope = "station_key:key-2".to_string();
        second.evidence = serde_json::json!({
            "endpoint_revision": 1,
            "station_key_id": "key-2"
        });
        store
            .append(&mut connection, &observation("observation-1", 1, 1), 1)
            .await
            .expect("append first scope");
        store
            .append(&mut connection, &second, 2)
            .await
            .expect("append second scope");

        let histories = store
            .list_for_scopes(
                &mut connection,
                &[
                    "station_key:key-2".to_string(),
                    "station_key:key-1".to_string(),
                    "station_key:key-1".to_string(),
                ],
            )
            .await
            .expect("read batched scope histories");
        assert_eq!(
            histories
                .iter()
                .map(|value| value.scope.station_key_id.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("key-1"), Some("key-2")]
        );
    }

    #[tokio::test]
    async fn v3_cursor_query_uses_only_columns_owned_by_migration() {
        let store = RoutingObservationStore;
        let mut connection = connection().await;
        for statement in [
            "ALTER TABLE routing_observations ADD COLUMN ingestion_sequence INTEGER",
            "ALTER TABLE routing_observations ADD COLUMN event_id TEXT",
            "ALTER TABLE routing_observations ADD COLUMN attempt_id TEXT",
            "ALTER TABLE routing_observations ADD COLUMN correlation_id TEXT",
            "ALTER TABLE routing_observations ADD COLUMN station_key_id TEXT",
            "ALTER TABLE routing_observations ADD COLUMN station_key_lifecycle_revision INTEGER",
            "ALTER TABLE routing_observations ADD COLUMN attempt_index INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE routing_observations ADD COLUMN boundary_crossed INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE routing_observations ADD COLUMN response_origin TEXT NOT NULL DEFAULT 'legacy'",
            "ALTER TABLE routing_observations ADD COLUMN event_time_status TEXT NOT NULL DEFAULT 'legacy'",
            "ALTER TABLE routing_observations ADD COLUMN outcome TEXT",
            "ALTER TABLE routing_observations ADD COLUMN failure_code TEXT",
            "ALTER TABLE routing_observations ADD COLUMN failure_attribution TEXT",
            "ALTER TABLE routing_observations ADD COLUMN recovery_origin TEXT",
            "ALTER TABLE routing_observations ADD COLUMN retry_disposition TEXT",
            "ALTER TABLE routing_observations ADD COLUMN ttft_ms INTEGER",
            "ALTER TABLE routing_observations ADD COLUMN comparability_key TEXT",
            "ALTER TABLE routing_observations ADD COLUMN generation_eligibility TEXT NOT NULL DEFAULT 'legacy'",
            "ALTER TABLE routing_observations ADD COLUMN cluster_finalized INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE routing_observations ADD COLUMN cluster_expected_attempt_count INTEGER NOT NULL DEFAULT 1",
        ] {
            sqlx::query(statement)
                .execute(&mut connection)
                .await
                .expect("add v3 observation column");
        }
        sqlx::query(
            "INSERT INTO routing_observations (id, producer_id, producer_sequence, payload_hash, event_at_ms, ingested_at_ms, scope, source, traffic_equivalence, outcome_kind, latency_ms, mass_basis_points, evidence_json, created_at_ms, ingestion_sequence, event_id, attempt_id, correlation_id, station_key_id, station_key_lifecycle_revision, attempt_index, boundary_crossed, response_origin, event_time_status, outcome, failure_attribution, recovery_origin, retry_disposition, cluster_finalized, cluster_expected_attempt_count, generation_eligibility) VALUES ('v3-row', 'test', 1, ?, 1000, 1000, 'station_key:key-1', 'real_request', 'exact_request', 'success', 100, 10000, '{}', 1000, 1, 'event-1', 'attempt-1', 'correlation-1', 'key-1', 1, 0, 1, 'upstream', 'valid', 'success', 'key', 'normal', 'end', 1, 1, 'active')",
        )
        .bind("a".repeat(64))
        .execute(&mut connection)
        .await
        .expect("insert v3 observation");

        let rows = store
            .list_after_v3(&mut connection, None, 8)
            .await
            .expect("read v3 observation");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].ingestion_sequence, 1);
        assert_eq!(rows[0].observation.id, "v3-row");
        assert_eq!(rows[0].observation.correlation_id, "correlation-1");
        assert_eq!(
            rows[0].observation.response_origin,
            ResponseOrigin::Upstream
        );
        assert_eq!(
            rows[0].observation.failure_attribution,
            FailureAttribution::Key
        );
        assert_eq!(rows[0].observation.recovery_origin, RecoveryOrigin::Normal);
        assert_eq!(
            rows[0].observation.retry_disposition,
            ObservationRetryDisposition::End
        );
    }

    #[tokio::test]
    async fn v3_comparability_key_survives_database_restart_and_cursor_read() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root
            .path()
            .join("routing-observation-comparability.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize runtime");
        let store = RoutingObservationStore;
        let comparability_key = format!("cmp:v1:{}", "e".repeat(64));

        let mut append = observation("v3-comparability", 1, 5_000);
        append.comparability_key = Some(comparability_key.clone());
        append.correlation_id = "correlation-comparability".to_string();
        append.evidence = serde_json::json!({
            "endpoint_revision": 1,
            "comparability_key": comparability_key,
            "correlation_id": "correlation-comparability",
            "attempt_index": 0,
            "station_key_lifecycle_revision": 1,
            "cluster_finalized": true,
            "cluster_expected_attempt_count": 1,
            "boundary_crossed": true,
            "event_time_status": "valid"
        });

        let mut write = runtime.begin_write().await.expect("write");
        assert_eq!(
            store
                .append_with_generation_eligibility(
                    write.connection(),
                    &append,
                    Some("active"),
                    5_000,
                )
                .await
                .expect("append v3 observation"),
            ObservationAppendResult::Inserted
        );
        write.commit().await.expect("commit");
        runtime.close().await.expect("close");

        let restarted = PersistenceRuntime::open_current(&path)
            .await
            .expect("reopen runtime");
        let mut read = restarted.begin_read().await.expect("restart read");
        let rows = store
            .list_after_v3(read.connection(), None, 8)
            .await
            .expect("read persisted v3 observation");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].observation.comparability_key.as_deref(),
            Some(comparability_key.as_str())
        );
        drop(read);
        restarted.close().await.expect("close restarted runtime");
    }
}
