use std::collections::BTreeMap;

use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

pub(crate) const ROUTING_ATTEMPT_ALGORITHM_VERSION: &str = "routing_quality_v3";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoutingGenerationEligibility {
    Active,
    Next,
    Legacy,
}

impl RoutingGenerationEligibility {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Next => "next",
            Self::Legacy => "legacy",
        }
    }

    fn parse(value: &str) -> Result<Self, PersistenceError> {
        match value {
            "active" => Ok(Self::Active),
            "next" => Ok(Self::Next),
            "legacy" => Ok(Self::Legacy),
            _ => Err(PersistenceError::InvariantViolation(
                "routing attempt has invalid generation eligibility".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingAttemptAdmission<'a> {
    pub(crate) attempt_id: &'a str,
    pub(crate) correlation_id: &'a str,
    pub(crate) station_key_id: &'a str,
    pub(crate) station_key_lifecycle_revision: u64,
    pub(crate) attempt_index: u16,
    pub(crate) capacity_lease_id: &'a str,
    pub(crate) half_open_lease_id: Option<&'a str>,
    pub(crate) lease_revision: Option<u64>,
    pub(crate) deadline_at_ms: u64,
    pub(crate) admitted_at_ms: u64,
    pub(crate) generation_eligibility: RoutingGenerationEligibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoutingAttemptAdmissionResult {
    Inserted,
    AlreadyAdmitted,
    LateAfterFinalization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingAttemptTerminal<'a> {
    pub(crate) attempt_id: &'a str,
    pub(crate) comparability_key: Option<&'a str>,
    pub(crate) failure_code: Option<&'a str>,
    pub(crate) failure_blame: Option<&'a str>,
    pub(crate) terminal_kind: &'a str,
    pub(crate) retry_disposition: &'a str,
    pub(crate) event_at_ms: u64,
    pub(crate) observed_at_ms: u64,
    pub(crate) ingested_at_ms: u64,
    pub(crate) latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingAttemptTerminalResult {
    pub(crate) updated: bool,
    pub(crate) boundary_crossed: bool,
    pub(crate) late_after_finalization: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinalizedRoutingAttemptSample {
    pub(crate) attempt_id: String,
    pub(crate) correlation_id: String,
    pub(crate) station_key_id: String,
    pub(crate) station_key_lifecycle_revision: u64,
    pub(crate) attempt_index: u16,
    pub(crate) expected_attempt_count: u16,
    pub(crate) candidate_admitted_at_ms: i64,
    pub(crate) boundary_crossed: bool,
    pub(crate) response_origin: String,
    pub(crate) event_time_status: String,
    pub(crate) outcome: String,
    pub(crate) failure_code: Option<String>,
    pub(crate) failure_attribution: String,
    pub(crate) recovery_origin: String,
    pub(crate) retry_disposition: String,
    pub(crate) latency_ms: Option<u32>,
    pub(crate) comparability_key: Option<String>,
    pub(crate) event_at_ms: Option<i64>,
    pub(crate) observed_at_ms: i64,
    pub(crate) generation_eligibility: RoutingGenerationEligibility,
    pub(crate) finalized_at_ms: i64,
    pub(crate) finalization_reason: String,
}

pub(crate) struct RoutingAttemptStore;

impl RoutingAttemptStore {
    pub(crate) async fn audit_late_admission_if_finalized(
        connection: &mut SqliteConnection,
        admission: &RoutingAttemptAdmission<'_>,
    ) -> Result<bool, PersistenceError> {
        validate_admission(admission)?;
        let event_id = format!("routing-attempt:{}", admission.attempt_id);
        audit_late_admission_if_finalized(connection, admission, &event_id).await
    }

    pub(crate) async fn resolve_generation_eligibility(
        connection: &mut SqliteConnection,
    ) -> Result<RoutingGenerationEligibility, PersistenceError> {
        let fence = super::routing_generation_store::RoutingGenerationStore
            .load_ingestion_fence(connection)
            .await?;
        Ok(match fence.eligibility.as_str() {
            "active" => RoutingGenerationEligibility::Active,
            _ => RoutingGenerationEligibility::Next,
        })
    }

    pub(crate) async fn admit(
        connection: &mut SqliteConnection,
        admission: &RoutingAttemptAdmission<'_>,
    ) -> Result<RoutingAttemptAdmissionResult, PersistenceError> {
        validate_admission(admission)?;
        let event_id = format!("routing-attempt:{}", admission.attempt_id);
        if Self::audit_late_admission_if_finalized(connection, admission).await? {
            return Ok(RoutingAttemptAdmissionResult::LateAfterFinalization);
        }
        let inserted = sqlx::query(
            "INSERT INTO routing_attempt_v3 (
                attempt_id, event_id, correlation_id, source, station_key_id,
                station_key_lifecycle_revision, attempt_index, candidate_admitted,
                candidate_admitted_at_ms, capacity_lease_id, half_open_lease_id,
                lease_revision, deadline_at_ms, boundary_crossed, response_origin,
                event_time_status, ingestion_sequence, algorithm_version,
                source_weight_revision, quality_policy_revision,
                generation_eligibility, terminal_state, created_at_ms, updated_at_ms
             ) VALUES (
                ?1, ?2, ?3, 'real_request', ?4, ?5, ?6, 1, ?7, ?8, ?9,
                ?10, ?11, 0, 'unknown', 'missing', NULL, ?12, 1, 1, ?13,
                'pending', ?7, ?7
             ) ON CONFLICT(attempt_id) DO NOTHING",
        )
        .bind(admission.attempt_id)
        .bind(&event_id)
        .bind(admission.correlation_id)
        .bind(admission.station_key_id)
        .bind(to_i64(admission.station_key_lifecycle_revision)?)
        .bind(i64::from(admission.attempt_index))
        .bind(to_i64(admission.admitted_at_ms)?)
        .bind(admission.capacity_lease_id)
        .bind(admission.half_open_lease_id)
        .bind(admission.lease_revision.map(to_i64).transpose()?)
        .bind(to_i64(admission.deadline_at_ms)?)
        .bind(ROUTING_ATTEMPT_ALGORITHM_VERSION)
        .bind(admission.generation_eligibility.as_str())
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if inserted == 1 {
            return Ok(RoutingAttemptAdmissionResult::Inserted);
        }

        let existing = sqlx::query(
            "SELECT event_id, correlation_id, source, station_key_id,
                    station_key_lifecycle_revision, attempt_index,
                    candidate_admitted, candidate_admitted_at_ms,
                    capacity_lease_id, half_open_lease_id, lease_revision,
                    deadline_at_ms, generation_eligibility
             FROM routing_attempt_v3 WHERE attempt_id = ?1",
        )
        .bind(admission.attempt_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "duplicate routing attempt admission has no durable row".into(),
            )
        })?;
        let matches = existing.get::<String, _>("event_id") == event_id
            && existing.get::<String, _>("correlation_id") == admission.correlation_id
            && existing.get::<String, _>("source") == "real_request"
            && existing.get::<String, _>("station_key_id") == admission.station_key_id
            && existing.get::<i64, _>("station_key_lifecycle_revision")
                == to_i64(admission.station_key_lifecycle_revision)?
            && existing.get::<i64, _>("attempt_index") == i64::from(admission.attempt_index)
            && existing.get::<i64, _>("candidate_admitted") == 1
            && existing.get::<i64, _>("candidate_admitted_at_ms")
                == to_i64(admission.admitted_at_ms)?
            && existing
                .get::<Option<String>, _>("capacity_lease_id")
                .as_deref()
                == Some(admission.capacity_lease_id)
            && existing
                .get::<Option<String>, _>("half_open_lease_id")
                .as_deref()
                == admission.half_open_lease_id
            && existing.get::<Option<i64>, _>("lease_revision")
                == admission.lease_revision.map(to_i64).transpose()?
            && existing.get::<Option<i64>, _>("deadline_at_ms")
                == Some(to_i64(admission.deadline_at_ms)?)
            && existing.get::<String, _>("generation_eligibility")
                == admission.generation_eligibility.as_str();
        if !matches {
            return Err(PersistenceError::InvariantViolation(
                "duplicate routing attempt admission does not match durable identity".into(),
            ));
        }
        Ok(RoutingAttemptAdmissionResult::AlreadyAdmitted)
    }

    pub(crate) async fn mark_boundary_crossed(
        connection: &mut SqliteConnection,
        attempt_id: &str,
        station_key_id: &str,
        lifecycle_revision: u64,
        crossed_at_ms: u64,
    ) -> Result<bool, PersistenceError> {
        let crossed_at_ms = to_i64(crossed_at_ms)?;
        let current = sqlx::query(
            "SELECT boundary_crossed, terminal_state FROM routing_attempt_v3
             WHERE attempt_id = ?1 AND station_key_id = ?2
               AND station_key_lifecycle_revision = ?3 AND candidate_admitted = 1",
        )
        .bind(attempt_id)
        .bind(station_key_id)
        .bind(to_i64(lifecycle_revision)?)
        .fetch_optional(&mut *connection)
        .await?;
        let Some(current) = current else {
            return Ok(false);
        };
        if current.get::<i64, _>("boundary_crossed") == 1 {
            return Ok(true);
        }
        if current.get::<String, _>("terminal_state") != "pending" {
            return Ok(false);
        }
        let ingestion_sequence = allocate_ingestion_sequence(connection).await?;
        let updated = sqlx::query(
            "UPDATE routing_attempt_v3
             SET boundary_crossed = 1, boundary_crossed_at_ms = ?4,
                 event_time_status = 'missing', ingestion_sequence = ?5,
                 updated_at_ms = MAX(updated_at_ms, ?4)
             WHERE attempt_id = ?1 AND station_key_id = ?2
               AND station_key_lifecycle_revision = ?3
               AND candidate_admitted = 1 AND terminal_state = 'pending'
               AND boundary_crossed = 0",
        )
        .bind(attempt_id)
        .bind(station_key_id)
        .bind(to_i64(lifecycle_revision)?)
        .bind(crossed_at_ms)
        .bind(ingestion_sequence)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if updated == 1 {
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) async fn terminalize(
        connection: &mut SqliteConnection,
        terminal: &RoutingAttemptTerminal<'_>,
    ) -> Result<RoutingAttemptTerminalResult, PersistenceError> {
        let current = sqlx::query(
            "SELECT event_id, correlation_id, station_key_id,
                    station_key_lifecycle_revision, attempt_index,
                    candidate_admitted, boundary_crossed, terminal_state,
                    outcome, failure_code, failure_attribution, event_at_ms,
                    observed_at_ms, ingested_at_ms, latency_ms, comparability_key,
                    retry_disposition
             FROM routing_attempt_v3 WHERE attempt_id = ?1",
        )
        .bind(terminal.attempt_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "routing attempt terminal has no durable admission slot".into(),
            )
        })?;
        if current.get::<i64, _>("candidate_admitted") != 1 {
            return Err(PersistenceError::InvariantViolation(
                "routing attempt terminal references an unadmitted slot".into(),
            ));
        }
        let boundary_crossed = current.get::<i64, _>("boundary_crossed") == 1;
        let canonical = canonical_terminal(terminal, boundary_crossed)?;
        if audit_late_terminal_if_finalized(connection, &current, terminal).await? {
            return Ok(RoutingAttemptTerminalResult {
                updated: false,
                boundary_crossed,
                late_after_finalization: true,
            });
        }
        if current.get::<String, _>("terminal_state") != "pending" {
            if let Err(error) = validate_duplicate_terminal(&current, terminal, &canonical) {
                return Err(error);
            }
            return Ok(RoutingAttemptTerminalResult {
                updated: false,
                boundary_crossed,
                late_after_finalization: false,
            });
        }
        let terminal_at_ms = to_i64(terminal.event_at_ms)?;
        let observed_at_ms = to_i64(terminal.observed_at_ms)?;
        let ingested_at_ms = to_i64(terminal.ingested_at_ms)?;
        let latency_ms = to_i64(terminal.latency_ms)?;
        let ingestion_sequence = allocate_ingestion_sequence(connection).await?;
        let updated = sqlx::query(
            "UPDATE routing_attempt_v3
             SET response_origin = ?2, event_time_status = 'valid', outcome = ?3,
                 failure_code = ?4, failure_attribution = ?5, latency_ms = ?6,
                 event_at_ms = ?7, observed_at_ms = ?8, ingested_at_ms = ?9,
                 comparability_key = ?10, ingestion_sequence = ?13,
                 recovery_origin = 'normal', retry_disposition = ?11,
                 terminal_state = ?12, terminal_at_ms = ?7,
                 released_at_ms = ?9, updated_at_ms = MAX(updated_at_ms, ?9)
             WHERE attempt_id = ?1 AND terminal_state = 'pending'",
        )
        .bind(terminal.attempt_id)
        .bind(canonical.response_origin)
        .bind(canonical.outcome)
        .bind(terminal.failure_code)
        .bind(canonical.failure_attribution)
        .bind(latency_ms)
        .bind(terminal_at_ms)
        .bind(observed_at_ms)
        .bind(ingested_at_ms)
        .bind(terminal.comparability_key)
        .bind(terminal.retry_disposition)
        .bind(canonical.terminal_state)
        .bind(ingestion_sequence)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(PersistenceError::InvariantViolation(
                "routing attempt terminal CAS lost without a visible terminal".into(),
            ));
        }
        Ok(RoutingAttemptTerminalResult {
            updated: true,
            boundary_crossed,
            late_after_finalization: false,
        })
    }

    /// Resolves attempt slots left pending by a previous process. A request
    /// that never crossed the outbound boundary is excluded from Key quality;
    /// a request that did cross it is conservatively recorded as an
    /// attributable upstream-uncertain failure. The caller owns the startup
    /// reconciliation transaction and finalizes request clusters afterwards.
    pub(crate) async fn recover_startup_interrupted(
        connection: &mut SqliteConnection,
        correlation_id: &str,
        recovered_at_ms: i64,
    ) -> Result<u64, PersistenceError> {
        if correlation_id.is_empty() || recovered_at_ms < 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let rows = sqlx::query(
            "SELECT attempt_id, boundary_crossed, candidate_admitted_at_ms
             FROM routing_attempt_v3
             WHERE source = 'real_request' AND correlation_id = ?1
               AND candidate_admitted = 1 AND terminal_state = 'pending'
             ORDER BY attempt_index ASC, attempt_id ASC",
        )
        .bind(correlation_id)
        .fetch_all(&mut *connection)
        .await?;
        let mut recovered = 0_u64;
        for row in rows {
            let attempt_id = row.get::<String, _>("attempt_id");
            let boundary_crossed = row.get::<i64, _>("boundary_crossed") == 1;
            let admitted_at_ms = row.get::<i64, _>("candidate_admitted_at_ms");
            let ingestion_sequence = allocate_ingestion_sequence(connection).await?;
            let (terminal_state, outcome, attribution, response_origin) = if boundary_crossed {
                (
                    "upstream_uncertain",
                    "attributable_failure",
                    "key",
                    "unknown",
                )
            } else {
                ("local_abandoned", "excluded", "local", "relay")
            };
            let updated = sqlx::query(
                "UPDATE routing_attempt_v3
                 SET response_origin = ?2, event_time_status = 'missing',
                     outcome = ?3, failure_code = 'startup_interrupted',
                     failure_attribution = ?4,
                     latency_ms = MIN(MAX(?5 - ?6, 0), 3600000),
                     event_at_ms = NULL, observed_at_ms = ?5,
                     ingested_at_ms = ?5, comparability_key = NULL,
                     ingestion_sequence = ?7, recovery_origin = 'crash_recovery',
                     retry_disposition = 'stop_request', terminal_state = ?8,
                     terminal_at_ms = ?5, released_at_ms = ?5,
                     updated_at_ms = MAX(updated_at_ms, ?5)
                 WHERE attempt_id = ?1 AND terminal_state = 'pending'",
            )
            .bind(&attempt_id)
            .bind(response_origin)
            .bind(outcome)
            .bind(attribution)
            .bind(recovered_at_ms)
            .bind(admitted_at_ms)
            .bind(ingestion_sequence)
            .bind(terminal_state)
            .execute(&mut *connection)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(PersistenceError::RevisionConflict(
                    "routing_attempt_v3".into(),
                ));
            }
            recovered = recovered.saturating_add(1);
        }
        Ok(recovered)
    }

    pub(crate) async fn finalize_request_clusters(
        connection: &mut SqliteConnection,
        correlation_id: &str,
        finalized_at_ms: i64,
    ) -> Result<Vec<FinalizedRoutingAttemptSample>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT attempt_id, correlation_id, station_key_id,
                    station_key_lifecycle_revision, attempt_index,
                    candidate_admitted_at_ms, boundary_crossed, response_origin,
                    event_time_status, outcome, failure_code, failure_attribution,
                    recovery_origin, retry_disposition, latency_ms,
                    comparability_key, event_at_ms, observed_at_ms,
                    generation_eligibility,
                    terminal_state
             FROM routing_attempt_v3
             WHERE source = 'real_request' AND correlation_id = ?1
               AND candidate_admitted = 1
               AND generation_eligibility IN ('active', 'next')
             ORDER BY station_key_id ASC, station_key_lifecycle_revision ASC,
                      attempt_index ASC, event_id ASC",
        )
        .bind(correlation_id)
        .fetch_all(&mut *connection)
        .await?;
        if rows.is_empty() {
            // A request rejected before candidate admission has no Key identity.
            // The explicit lifecycle row prevents an empty set from being
            // interpreted as a quality sample.
            let generation_eligibility = Self::resolve_generation_eligibility(connection).await?;
            sqlx::query(
                "INSERT INTO routing_attempt_cluster_v3 (
                    source, station_key_id, station_key_lifecycle_revision,
                    correlation_id, expected_attempt_count, cluster_finalized,
                    cluster_finalized_at_ms, cluster_finalization_reason,
                    generation_eligibility, created_at_ms, updated_at_ms
                 ) VALUES (
                    'real_request', NULL, NULL, ?1, 0, 1, ?2, 'no_attempts',
                    ?3, ?2, ?2
                 ) ON CONFLICT DO NOTHING",
            )
            .bind(correlation_id)
            .bind(finalized_at_ms.max(0))
            .bind(generation_eligibility.as_str())
            .execute(&mut *connection)
            .await?;
            return Ok(Vec::new());
        }
        if rows
            .iter()
            .any(|row| row.get::<String, _>("terminal_state") == "pending")
        {
            return Err(PersistenceError::InvariantViolation(
                "request terminal reached a pending durable routing attempt".into(),
            ));
        }

        let mut attempt_indices = rows
            .iter()
            .map(|row| row.get::<i64, _>("attempt_index"))
            .collect::<Vec<_>>();
        attempt_indices.sort_unstable();
        attempt_indices.dedup();
        if attempt_indices
            .iter()
            .enumerate()
            .any(|(expected, actual)| i64::try_from(expected).ok() != Some(*actual))
        {
            return Err(PersistenceError::InvariantViolation(
                "request terminal has a non-contiguous durable attempt ledger".into(),
            ));
        }
        let expected_attempt_count = u16::try_from(rows.len()).map_err(|_| {
            PersistenceError::InvariantViolation(
                "routing request cluster exceeds supported attempt count".into(),
            )
        })?;
        if expected_attempt_count > 1_023 {
            return Err(PersistenceError::InvariantViolation(
                "routing request cluster exceeds supported attempt count".into(),
            ));
        }
        let mut groups = BTreeMap::<(String, i64), Vec<sqlx::sqlite::SqliteRow>>::new();
        for row in rows {
            let key = (
                row.get::<String, _>("station_key_id"),
                row.get::<i64, _>("station_key_lifecycle_revision"),
            );
            groups.entry(key).or_default().push(row);
        }
        let mut samples = Vec::with_capacity(groups.len());
        for ((station_key_id, lifecycle_revision), attempts) in groups {
            let canonical = attempts.last().ok_or_else(|| {
                PersistenceError::InvariantViolation("routing attempt cluster is empty".into())
            })?;
            let generation_eligibility = RoutingGenerationEligibility::parse(
                canonical
                    .get::<String, _>("generation_eligibility")
                    .as_str(),
            )?;
            let inserted = sqlx::query(
                "INSERT INTO routing_attempt_cluster_v3 (
                    source, station_key_id, station_key_lifecycle_revision,
                    correlation_id, expected_attempt_count, cluster_finalized,
                    cluster_finalized_at_ms, cluster_finalization_reason,
                    generation_eligibility, created_at_ms, updated_at_ms
                 ) VALUES ('real_request', ?1, ?2, ?3, ?4, 1, ?5,
                           'request_terminal', ?6, ?5, ?5)
                 ON CONFLICT(source, station_key_id,
                             station_key_lifecycle_revision, correlation_id)
                 DO NOTHING",
            )
            .bind(&station_key_id)
            .bind(lifecycle_revision)
            .bind(correlation_id)
            .bind(i64::from(expected_attempt_count))
            .bind(finalized_at_ms.max(0))
            .bind(generation_eligibility.as_str())
            .execute(&mut *connection)
            .await?
            .rows_affected();
            if inserted == 0 {
                validate_existing_cluster(
                    connection,
                    correlation_id,
                    &station_key_id,
                    lifecycle_revision,
                    expected_attempt_count,
                    generation_eligibility,
                )
                .await?;
                continue;
            }
            samples.push(FinalizedRoutingAttemptSample {
                attempt_id: canonical.get("attempt_id"),
                correlation_id: canonical.get("correlation_id"),
                station_key_id,
                station_key_lifecycle_revision: u64::try_from(lifecycle_revision).map_err(
                    |_| {
                        PersistenceError::InvariantViolation(
                            "routing attempt lifecycle revision is invalid".into(),
                        )
                    },
                )?,
                attempt_index: u16::try_from(canonical.get::<i64, _>("attempt_index")).map_err(
                    |_| {
                        PersistenceError::InvariantViolation(
                            "routing attempt index is invalid".into(),
                        )
                    },
                )?,
                expected_attempt_count,
                candidate_admitted_at_ms: canonical.get("candidate_admitted_at_ms"),
                boundary_crossed: canonical.get::<i64, _>("boundary_crossed") == 1,
                response_origin: canonical.get("response_origin"),
                event_time_status: canonical.get("event_time_status"),
                outcome: canonical.get("outcome"),
                failure_code: canonical.get("failure_code"),
                failure_attribution: canonical.get("failure_attribution"),
                recovery_origin: canonical.get("recovery_origin"),
                retry_disposition: canonical.get("retry_disposition"),
                latency_ms: canonical
                    .get::<Option<i64>, _>("latency_ms")
                    .map(u32::try_from)
                    .transpose()
                    .map_err(|_| {
                        PersistenceError::InvariantViolation(
                            "routing attempt latency is invalid".into(),
                        )
                    })?,
                comparability_key: canonical.get("comparability_key"),
                event_at_ms: canonical.get("event_at_ms"),
                observed_at_ms: canonical
                    .get::<Option<i64>, _>("observed_at_ms")
                    .unwrap_or(finalized_at_ms.max(0)),
                generation_eligibility,
                finalized_at_ms: finalized_at_ms.max(0),
                finalization_reason: "request_terminal".into(),
            });
        }
        Ok(samples)
    }
}

struct CanonicalTerminal<'a> {
    terminal_state: &'a str,
    outcome: &'a str,
    failure_attribution: &'a str,
    response_origin: &'a str,
}

async fn audit_late_admission_if_finalized(
    connection: &mut SqliteConnection,
    admission: &RoutingAttemptAdmission<'_>,
    event_id: &str,
) -> Result<bool, PersistenceError> {
    let finalized = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(
             SELECT 1 FROM routing_attempt_cluster_v3
             WHERE source = 'real_request' AND correlation_id = ?1
               AND cluster_finalized = 1
         )",
    )
    .bind(admission.correlation_id)
    .fetch_one(&mut *connection)
    .await?
        != 0;
    if !finalized {
        return Ok(false);
    }
    let payload_commitment = format!(
        "{}:{}:{}:{}:{}",
        event_id,
        admission.correlation_id,
        admission.station_key_id,
        admission.station_key_lifecycle_revision,
        admission.attempt_index
    );
    sqlx::query(
        "INSERT OR IGNORE INTO routing_attempt_late_audit_v3 (
             event_kind, event_id, attempt_id, correlation_id, station_key_id,
             station_key_lifecycle_revision, attempt_index, reason_code,
             payload_commitment, observed_at_ms, created_at_ms
         ) VALUES (
             'admission', ?1, ?2, ?3, ?4, ?5, ?6,
             'late_after_finalization', ?7, ?8, ?8
         )",
    )
    .bind(event_id)
    .bind(admission.attempt_id)
    .bind(admission.correlation_id)
    .bind(admission.station_key_id)
    .bind(to_i64(admission.station_key_lifecycle_revision)?)
    .bind(i64::from(admission.attempt_index))
    .bind(payload_commitment)
    .bind(to_i64(admission.admitted_at_ms)?)
    .execute(&mut *connection)
    .await?;
    Ok(true)
}

async fn audit_late_terminal_if_finalized(
    connection: &mut SqliteConnection,
    current: &sqlx::sqlite::SqliteRow,
    terminal: &RoutingAttemptTerminal<'_>,
) -> Result<bool, PersistenceError> {
    let correlation_id = current.get::<String, _>("correlation_id");
    let finalized = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(
             SELECT 1 FROM routing_attempt_cluster_v3
             WHERE source = 'real_request' AND correlation_id = ?1
               AND cluster_finalized = 1
         )",
    )
    .bind(&correlation_id)
    .fetch_one(&mut *connection)
    .await?
        != 0;
    if !finalized {
        return Ok(false);
    }
    let event_id = current.get::<String, _>("event_id");
    let payload_commitment = format!(
        "{}:{}:{}:{}:{}:{}",
        terminal.attempt_id,
        terminal.terminal_kind,
        terminal.failure_code.unwrap_or_default(),
        terminal.failure_blame.unwrap_or_default(),
        terminal.retry_disposition,
        terminal.event_at_ms
    );
    sqlx::query(
        "INSERT OR IGNORE INTO routing_attempt_late_audit_v3 (
             event_kind, event_id, attempt_id, correlation_id, station_key_id,
             station_key_lifecycle_revision, attempt_index, reason_code,
             payload_commitment, observed_at_ms, created_at_ms
         ) VALUES (
             'terminal', ?1, ?2, ?3, ?4, ?5, ?6,
             'late_after_finalization', ?7, ?8, ?8
         )",
    )
    .bind(event_id)
    .bind(terminal.attempt_id)
    .bind(correlation_id)
    .bind(current.get::<Option<String>, _>("station_key_id"))
    .bind(current.get::<Option<i64>, _>("station_key_lifecycle_revision"))
    .bind(current.get::<i64, _>("attempt_index"))
    .bind(payload_commitment)
    .bind(to_i64(terminal.observed_at_ms)?)
    .execute(&mut *connection)
    .await?;
    Ok(true)
}

fn canonical_terminal<'a>(
    terminal: &RoutingAttemptTerminal<'a>,
    boundary_crossed: bool,
) -> Result<CanonicalTerminal<'a>, PersistenceError> {
    if terminal.terminal_kind == "succeeded" {
        if !boundary_crossed {
            return Err(PersistenceError::InvariantViolation(
                "successful routing attempt did not cross the outbound boundary".into(),
            ));
        }
        return Ok(CanonicalTerminal {
            terminal_state: "success",
            outcome: "success",
            failure_attribution: "key",
            response_origin: "upstream",
        });
    }
    if terminal.terminal_kind == "abandoned" || !boundary_crossed {
        return Ok(CanonicalTerminal {
            terminal_state: "local_abandoned",
            outcome: "excluded",
            failure_attribution: "local",
            response_origin: "relay",
        });
    }
    if matches!(
        terminal.failure_blame,
        Some("Downstream") | Some("downstream")
    ) {
        return Ok(CanonicalTerminal {
            terminal_state: "excluded",
            outcome: "excluded",
            failure_attribution: "client",
            response_origin: "relay",
        });
    }
    if matches!(terminal.failure_blame, Some("Upstream") | Some("upstream")) {
        return Ok(CanonicalTerminal {
            terminal_state: "attributable_failure",
            outcome: "attributable_failure",
            failure_attribution: "key",
            response_origin: "upstream",
        });
    }
    Ok(CanonicalTerminal {
        terminal_state: "upstream_uncertain",
        outcome: "attributable_failure",
        failure_attribution: "key",
        response_origin: "unknown",
    })
}

fn validate_admission(admission: &RoutingAttemptAdmission<'_>) -> Result<(), PersistenceError> {
    if admission.attempt_id.is_empty()
        || admission.attempt_id.len() > 144
        || admission.correlation_id.is_empty()
        || admission.correlation_id.len() > 192
        || admission.station_key_id.is_empty()
        || admission.station_key_id.len() > 160
        || admission.station_key_lifecycle_revision == 0
        || admission.capacity_lease_id.is_empty()
        || admission.deadline_at_ms < admission.admitted_at_ms
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    if admission.half_open_lease_id.is_some() != admission.lease_revision.is_some() {
        return Err(PersistenceError::InvariantViolation(
            "half-open lease identity and revision must be present together".into(),
        ));
    }
    Ok(())
}

fn validate_duplicate_terminal(
    current: &sqlx::sqlite::SqliteRow,
    terminal: &RoutingAttemptTerminal<'_>,
    canonical: &CanonicalTerminal<'_>,
) -> Result<(), PersistenceError> {
    let matches = current.get::<String, _>("terminal_state") == canonical.terminal_state
        && current.get::<String, _>("outcome") == canonical.outcome
        && current.get::<Option<String>, _>("failure_code").as_deref() == terminal.failure_code
        && current.get::<String, _>("failure_attribution") == canonical.failure_attribution
        && current.get::<Option<i64>, _>("event_at_ms") == Some(to_i64(terminal.event_at_ms)?)
        && current.get::<Option<i64>, _>("observed_at_ms")
            == Some(to_i64(terminal.observed_at_ms)?)
        && current.get::<Option<i64>, _>("ingested_at_ms")
            == Some(to_i64(terminal.ingested_at_ms)?)
        && current.get::<Option<i64>, _>("latency_ms") == Some(to_i64(terminal.latency_ms)?)
        && current
            .get::<Option<String>, _>("comparability_key")
            .as_deref()
            == terminal.comparability_key
        && current
            .get::<Option<String>, _>("retry_disposition")
            .as_deref()
            == Some(terminal.retry_disposition);
    if matches {
        Ok(())
    } else {
        Err(PersistenceError::InvariantViolation(
            "duplicate routing attempt terminal does not match durable outcome".into(),
        ))
    }
}

async fn validate_existing_cluster(
    connection: &mut SqliteConnection,
    correlation_id: &str,
    station_key_id: &str,
    lifecycle_revision: i64,
    expected_attempt_count: u16,
    generation_eligibility: RoutingGenerationEligibility,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT expected_attempt_count, cluster_finalized,
                cluster_finalization_reason, generation_eligibility
         FROM routing_attempt_cluster_v3
         WHERE source = 'real_request' AND station_key_id = ?1
           AND station_key_lifecycle_revision = ?2 AND correlation_id = ?3",
    )
    .bind(station_key_id)
    .bind(lifecycle_revision)
    .bind(correlation_id)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(|| {
        PersistenceError::InvariantViolation(
            "duplicate routing cluster finalization has no durable row".into(),
        )
    })?;
    if row.get::<i64, _>("expected_attempt_count") != i64::from(expected_attempt_count)
        || row.get::<i64, _>("cluster_finalized") != 1
        || row.get::<String, _>("cluster_finalization_reason") != "request_terminal"
        || row.get::<String, _>("generation_eligibility") != generation_eligibility.as_str()
    {
        return Err(PersistenceError::InvariantViolation(
            "duplicate routing cluster finalization does not match durable row".into(),
        ));
    }
    Ok(())
}

fn to_i64(value: u64) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| PersistenceError::ConstraintViolation)
}

async fn allocate_ingestion_sequence(
    connection: &mut SqliteConnection,
) -> Result<i64, PersistenceError> {
    sqlx::query(
        "UPDATE routing_v3_ingestion_sequence
         SET next_sequence = next_sequence + 1 WHERE singleton_key = 1",
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query_scalar(
        "SELECT next_sequence - 1 FROM routing_v3_ingestion_sequence WHERE singleton_key = 1",
    )
    .fetch_one(&mut *connection)
    .await
    .map_err(PersistenceError::from)
}

#[cfg(test)]
mod tests {
    use sqlx::Row;

    use crate::persistence::runtime::PersistenceRuntime;

    use super::*;

    async fn runtime() -> PersistenceRuntime {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("routing-attempt-store.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize runtime");
        std::mem::forget(root);
        runtime
    }

    fn admission<'a>(
        attempt_id: &'a str,
        correlation_id: &'a str,
        station_key_id: &'a str,
        attempt_index: u16,
    ) -> RoutingAttemptAdmission<'a> {
        RoutingAttemptAdmission {
            attempt_id,
            correlation_id,
            station_key_id,
            station_key_lifecycle_revision: 1,
            attempt_index,
            capacity_lease_id: "capacity-token",
            half_open_lease_id: None,
            lease_revision: None,
            deadline_at_ms: 10_000,
            admitted_at_ms: 1_000 + u64::from(attempt_index),
            generation_eligibility: RoutingGenerationEligibility::Next,
        }
    }

    fn terminal<'a>(attempt_id: &'a str, at_ms: u64) -> RoutingAttemptTerminal<'a> {
        RoutingAttemptTerminal {
            attempt_id,
            comparability_key: None,
            failure_code: Some("upstream_rate_limited"),
            failure_blame: Some("Upstream"),
            terminal_kind: "failed",
            retry_disposition: "retryable_before_commit",
            event_at_ms: at_ms,
            observed_at_ms: at_ms,
            ingested_at_ms: at_ms,
            latency_ms: 100,
        }
    }

    fn terminal_with_comparability<'a>(
        attempt_id: &'a str,
        comparability_key: &'a str,
        at_ms: u64,
    ) -> RoutingAttemptTerminal<'a> {
        RoutingAttemptTerminal {
            attempt_id,
            comparability_key: Some(comparability_key),
            failure_code: None,
            failure_blame: None,
            terminal_kind: "succeeded",
            retry_disposition: "end",
            event_at_ms: at_ms,
            observed_at_ms: at_ms,
            ingested_at_ms: at_ms,
            latency_ms: 100,
        }
    }

    #[tokio::test]
    async fn admission_boundary_terminal_and_cluster_are_durable_and_idempotent() {
        let runtime = runtime().await;
        let mut write = runtime.begin_write().await.expect("write");
        let admitted = admission("request-1:0", "request-1", "key-1", 0);
        assert_eq!(
            RoutingAttemptStore::admit(write.connection(), &admitted)
                .await
                .expect("admit"),
            RoutingAttemptAdmissionResult::Inserted
        );
        assert_eq!(
            RoutingAttemptStore::admit(write.connection(), &admitted)
                .await
                .expect("duplicate admit"),
            RoutingAttemptAdmissionResult::AlreadyAdmitted
        );
        assert!(RoutingAttemptStore::mark_boundary_crossed(
            write.connection(),
            "request-1:0",
            "key-1",
            1,
            1_100,
        )
        .await
        .expect("boundary"));
        assert!(RoutingAttemptStore::mark_boundary_crossed(
            write.connection(),
            "request-1:0",
            "key-1",
            1,
            1_100,
        )
        .await
        .expect("duplicate boundary"));
        let terminal = terminal("request-1:0", 1_200);
        assert!(
            RoutingAttemptStore::terminalize(write.connection(), &terminal)
                .await
                .expect("terminal")
                .updated
        );
        assert!(
            !RoutingAttemptStore::terminalize(write.connection(), &terminal)
                .await
                .expect("duplicate terminal")
                .updated
        );
        let samples =
            RoutingAttemptStore::finalize_request_clusters(write.connection(), "request-1", 1_300)
                .await
                .expect("finalize cluster");
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].attempt_id, "request-1:0");
        assert_eq!(samples[0].expected_attempt_count, 1);
        assert_eq!(samples[0].outcome, "attributable_failure");
        assert!(samples[0].boundary_crossed);
        assert!(RoutingAttemptStore::finalize_request_clusters(
            write.connection(),
            "request-1",
            1_300,
        )
        .await
        .expect("duplicate finalization")
        .is_empty());
        write.commit().await.expect("commit");
        runtime.close().await.expect("close");
    }

    #[tokio::test]
    async fn request_cluster_records_the_complete_request_attempt_count_for_each_key() {
        let runtime = runtime().await;
        let mut write = runtime.begin_write().await.expect("write");
        for (attempt_id, key, index) in [
            ("request-2:0", "key-a", 0),
            ("request-2:1", "key-b", 1),
            ("request-2:2", "key-a", 2),
        ] {
            RoutingAttemptStore::admit(
                write.connection(),
                &admission(attempt_id, "request-2", key, index),
            )
            .await
            .expect("admit");
            RoutingAttemptStore::mark_boundary_crossed(
                write.connection(),
                attempt_id,
                key,
                1,
                2_000 + u64::from(index),
            )
            .await
            .expect("boundary");
        }
        RoutingAttemptStore::terminalize(write.connection(), &terminal("request-2:0", 2_100))
            .await
            .expect("first terminal");
        let error =
            RoutingAttemptStore::finalize_request_clusters(write.connection(), "request-2", 2_200)
                .await
                .expect_err("pending slot must block finalization");
        assert!(matches!(error, PersistenceError::InvariantViolation(_)));

        RoutingAttemptStore::terminalize(write.connection(), &terminal("request-2:1", 2_150))
            .await
            .expect("second terminal");
        RoutingAttemptStore::terminalize(write.connection(), &terminal("request-2:2", 2_175))
            .await
            .expect("third terminal");
        let samples =
            RoutingAttemptStore::finalize_request_clusters(write.connection(), "request-2", 2_200)
                .await
                .expect("finalize");
        assert_eq!(samples.len(), 2);
        assert_eq!(
            samples
                .iter()
                .map(|sample| {
                    (
                        sample.station_key_id.as_str(),
                        sample.expected_attempt_count,
                        sample.attempt_id.as_str(),
                    )
                })
                .collect::<Vec<_>>(),
            vec![("key-a", 3, "request-2:2"), ("key-b", 3, "request-2:1")]
        );
        write.commit().await.expect("commit");
        runtime.close().await.expect("close");
    }

    #[tokio::test]
    async fn terminal_without_admission_is_rejected_and_zero_attempt_request_is_explicit() {
        let runtime = runtime().await;
        let mut write = runtime.begin_write().await.expect("write");
        let error =
            RoutingAttemptStore::terminalize(write.connection(), &terminal("missing:0", 3_000))
                .await
                .expect_err("unadmitted terminal");
        assert!(matches!(error, PersistenceError::InvariantViolation(_)));
        assert!(RoutingAttemptStore::finalize_request_clusters(
            write.connection(),
            "no-attempts",
            3_100,
        )
        .await
        .expect("finalize no attempts")
        .is_empty());
        let row = sqlx::query(
            "SELECT expected_attempt_count, cluster_finalized,
                    cluster_finalization_reason, station_key_id
             FROM routing_attempt_cluster_v3
             WHERE source = 'real_request' AND correlation_id = 'no-attempts'",
        )
        .fetch_one(write.connection())
        .await
        .expect("no-attempt cluster");
        assert_eq!(row.get::<i64, _>("expected_attempt_count"), 0);
        assert_eq!(row.get::<i64, _>("cluster_finalized"), 1);
        assert_eq!(
            row.get::<String, _>("cluster_finalization_reason"),
            "no_attempts"
        );
        assert_eq!(row.get::<Option<String>, _>("station_key_id"), None);
        write.commit().await.expect("commit");
        runtime.close().await.expect("close");
    }

    #[tokio::test]
    async fn duplicate_admission_with_changed_identity_is_rejected() {
        let runtime = runtime().await;
        let mut write = runtime.begin_write().await.expect("write");
        RoutingAttemptStore::admit(
            write.connection(),
            &admission("request-3:0", "request-3", "key-a", 0),
        )
        .await
        .expect("admit");
        let error = RoutingAttemptStore::admit(
            write.connection(),
            &admission("request-3:0", "request-3", "key-b", 0),
        )
        .await
        .expect_err("identity collision");
        assert!(matches!(error, PersistenceError::InvariantViolation(_)));
        drop(write);
        runtime.close().await.expect("close");
    }

    #[tokio::test]
    async fn finalized_cluster_rejects_late_work_and_keeps_an_immutable_audit() {
        let runtime = runtime().await;
        let mut write = runtime.begin_write().await.expect("write");
        let admitted = admission("request-finalized:0", "request-finalized", "key-a", 0);
        assert_eq!(
            RoutingAttemptStore::admit(write.connection(), &admitted)
                .await
                .expect("admit"),
            RoutingAttemptAdmissionResult::Inserted
        );
        RoutingAttemptStore::mark_boundary_crossed(
            write.connection(),
            admitted.attempt_id,
            admitted.station_key_id,
            admitted.station_key_lifecycle_revision,
            3_900,
        )
        .await
        .expect("cross boundary");
        RoutingAttemptStore::terminalize(write.connection(), &terminal(admitted.attempt_id, 4_000))
            .await
            .expect("terminalize");
        RoutingAttemptStore::finalize_request_clusters(
            write.connection(),
            admitted.correlation_id,
            4_100,
        )
        .await
        .expect("finalize");

        let late_admission = admission("request-finalized:1", "request-finalized", "key-b", 1);
        assert_eq!(
            RoutingAttemptStore::admit(write.connection(), &late_admission)
                .await
                .expect("late admission"),
            RoutingAttemptAdmissionResult::LateAfterFinalization
        );
        let late_terminal = RoutingAttemptStore::terminalize(
            write.connection(),
            &terminal(admitted.attempt_id, 4_000),
        )
        .await
        .expect("late terminal");
        assert!(!late_terminal.updated);
        assert!(late_terminal.late_after_finalization);

        let attempt_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_attempt_v3
             WHERE correlation_id = 'request-finalized'",
        )
        .fetch_one(write.connection())
        .await
        .expect("attempt count");
        assert_eq!(attempt_count, 1);
        let audits = sqlx::query(
            "SELECT event_kind, reason_code
             FROM routing_attempt_late_audit_v3
             WHERE correlation_id = 'request-finalized'
             ORDER BY audit_id",
        )
        .fetch_all(write.connection())
        .await
        .expect("late audit");
        assert_eq!(audits.len(), 2);
        assert_eq!(audits[0].get::<String, _>("event_kind"), "admission");
        assert_eq!(audits[1].get::<String, _>("event_kind"), "terminal");
        assert!(audits
            .iter()
            .all(|row| { row.get::<String, _>("reason_code") == "late_after_finalization" }));

        let mutation = sqlx::query(
            "UPDATE routing_attempt_cluster_v3
             SET expected_attempt_count = 2, updated_at_ms = 4_200
             WHERE correlation_id = 'request-finalized'",
        )
        .execute(write.connection())
        .await;
        assert!(mutation.is_err(), "finalized cluster must be immutable");
        let audit_mutation = sqlx::query(
            "DELETE FROM routing_attempt_late_audit_v3
             WHERE correlation_id = 'request-finalized'",
        )
        .execute(write.connection())
        .await;
        assert!(audit_mutation.is_err(), "late audit must be immutable");

        write.commit().await.expect("commit");
        runtime.close().await.expect("close");
    }

    #[tokio::test]
    async fn comparability_key_survives_terminal_cluster_finalization_and_restart() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("routing-attempt-comparability.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize runtime");
        let comparability_key = format!("cmp:v1:{}", "c".repeat(64));

        let mut write = runtime.begin_write().await.expect("write");
        let admitted = admission(
            "request-comparability:0",
            "request-comparability",
            "key-1",
            0,
        );
        RoutingAttemptStore::admit(write.connection(), &admitted)
            .await
            .expect("admit");
        RoutingAttemptStore::mark_boundary_crossed(
            write.connection(),
            admitted.attempt_id,
            admitted.station_key_id,
            admitted.station_key_lifecycle_revision,
            4_100,
        )
        .await
        .expect("cross boundary");
        let terminal = terminal_with_comparability(admitted.attempt_id, &comparability_key, 4_200);
        RoutingAttemptStore::terminalize(write.connection(), &terminal)
            .await
            .expect("terminalize");

        let samples = RoutingAttemptStore::finalize_request_clusters(
            write.connection(),
            admitted.correlation_id,
            4_300,
        )
        .await
        .expect("finalize cluster");
        assert_eq!(samples.len(), 1);
        assert_eq!(
            samples[0].comparability_key.as_deref(),
            Some(comparability_key.as_str())
        );

        let changed_comparability_key = format!("cmp:v1:{}", "d".repeat(64));
        let changed_terminal =
            terminal_with_comparability(admitted.attempt_id, &changed_comparability_key, 4_200);
        let late_terminal = RoutingAttemptStore::terminalize(write.connection(), &changed_terminal)
            .await
            .expect("changed terminal after finalization is audit-only");
        assert!(!late_terminal.updated);
        assert!(late_terminal.late_after_finalization);

        let durable_key: String = sqlx::query_scalar(
            "SELECT comparability_key FROM routing_attempt_v3 WHERE attempt_id = ?1",
        )
        .bind(admitted.attempt_id)
        .fetch_one(write.connection())
        .await
        .expect("read terminal comparability key");
        assert_eq!(durable_key, comparability_key);
        write.commit().await.expect("commit");
        runtime.close().await.expect("close");

        let restarted = PersistenceRuntime::open_current(&path)
            .await
            .expect("reopen runtime");
        let mut read = restarted.begin_read().await.expect("restart read");
        let restarted_key: String = sqlx::query_scalar(
            "SELECT comparability_key FROM routing_attempt_v3 WHERE attempt_id = ?1",
        )
        .bind(admitted.attempt_id)
        .fetch_one(read.connection())
        .await
        .expect("read persisted comparability key");
        assert_eq!(restarted_key, comparability_key);
        drop(read);
        restarted.close().await.expect("close restarted runtime");
    }
}
