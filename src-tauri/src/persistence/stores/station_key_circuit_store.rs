//! Durable adapter for the station-key v3 circuit reducer.
//!
//! The reducer in `application::station_key_circuit` owns all state-machine
//! semantics.  This module only translates SQLite rows, performs optimistic
//! compare-and-swap writes, and records idempotent circuit events.

use sqlx::{Row, SqliteConnection};

use crate::{
    application::station_key_circuit::{
        CircuitAdmission, CircuitAdmissionResult, CircuitError, CircuitTransition,
        StationKeyCircuit, StationKeyCircuitConfig, StationKeyCircuitLeasePolicy,
        StationKeyCircuitState, StationKeyCircuitStatus,
    },
    persistence::error::PersistenceError,
};

const MAX_COOLDOWN_MS: u64 = 24 * 60 * 60 * 1_000;
pub(crate) const SHARED_CIRCUIT_PERSISTENCE_GATE_KEY: &str = "__routing_circuit_store__";
pub(crate) const SHARED_CIRCUIT_PERSISTENCE_GATE_REVISION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CircuitTerminalResult {
    /// True only when this terminal event advanced the reducer state. A
    /// duplicate or late audit keeps its durable sequence but is not replayed.
    pub(crate) applied: bool,
    pub(crate) transition: CircuitTransition,
    pub(crate) state_revision: u64,
    pub(crate) reducer_commit_sequence: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StationKeyCircuitStore;

impl StationKeyCircuitStore {
    pub(crate) async fn persistence_gate_active(
        &self,
        connection: &mut SqliteConnection,
        station_key_id: &str,
        lifecycle_revision: u64,
    ) -> Result<bool, PersistenceError> {
        if station_key_id.is_empty() || lifecycle_revision == 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let lifecycle_revision =
            i64::try_from(lifecycle_revision).map_err(|_| PersistenceError::ConstraintViolation)?;
        sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(
                 SELECT 1 FROM routing_circuit_persistence_gate_v3
                 WHERE ((station_key_id = ?1
                         AND station_key_lifecycle_revision = ?2)
                        OR (station_key_id = ?3
                            AND station_key_lifecycle_revision = ?4))
                   AND status = 'persistence_unavailable'
             )",
        )
        .bind(station_key_id)
        .bind(lifecycle_revision)
        .bind(SHARED_CIRCUIT_PERSISTENCE_GATE_KEY)
        .bind(i64::try_from(SHARED_CIRCUIT_PERSISTENCE_GATE_REVISION).unwrap_or(1))
        .fetch_one(&mut *connection)
        .await
        .map(|value| value != 0)
        .map_err(Into::into)
    }

    pub(crate) async fn mark_persistence_unavailable(
        &self,
        connection: &mut SqliteConnection,
        station_key_id: &str,
        lifecycle_revision: u64,
        now_ms: u64,
    ) -> Result<(), PersistenceError> {
        if station_key_id.is_empty() || lifecycle_revision == 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let lifecycle_revision =
            i64::try_from(lifecycle_revision).map_err(|_| PersistenceError::ConstraintViolation)?;
        let now_ms = i64::try_from(now_ms).map_err(|_| PersistenceError::ConstraintViolation)?;
        sqlx::query(
            "INSERT INTO routing_circuit_persistence_gate_v3 (
                 station_key_id, station_key_lifecycle_revision, status,
                 reason_code, opened_at_ms, updated_at_ms
             ) VALUES (?1, ?2, 'persistence_unavailable',
                       'circuit_persistence_unavailable', ?3, ?3)
             ON CONFLICT(station_key_id, station_key_lifecycle_revision) DO UPDATE SET
                 status = 'persistence_unavailable',
                 reason_code = 'circuit_persistence_unavailable',
                 updated_at_ms = MAX(updated_at_ms, excluded.updated_at_ms)",
        )
        .bind(station_key_id)
        .bind(lifecycle_revision)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    /// The supervised reaper is the only production caller. The sentinel
    /// update and circuit-state read prove both directions before gates are
    /// removed in the same transaction.
    pub(crate) async fn health_check_and_clear_persistence_gates(
        &self,
        connection: &mut SqliteConnection,
        now_ms: u64,
    ) -> Result<u64, PersistenceError> {
        let now_ms = i64::try_from(now_ms).map_err(|_| PersistenceError::ConstraintViolation)?;
        let _: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM routing_circuit_state_v3")
            .fetch_one(&mut *connection)
            .await?;
        let updated = sqlx::query(
            "UPDATE routing_circuit_persistence_health_v3
             SET check_revision = check_revision + 1, checked_at_ms = ?1
             WHERE singleton_key = 1",
        )
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(PersistenceError::InvariantViolation(
                "routing circuit persistence health sentinel is missing".into(),
            ));
        }
        let deleted = sqlx::query("DELETE FROM routing_circuit_persistence_gate_v3")
            .execute(&mut *connection)
            .await?;
        Ok(deleted.rows_affected())
    }

    /// Marks the exact Half-Open lease as having crossed the outbound
    /// boundary. This is intentionally a separate CAS from admission: a
    /// lease that never reaches the adapter must be released without opening
    /// the circuit, while a crashed outbound request must be recoverable by
    /// the supervised reaper.
    pub(crate) async fn mark_boundary_crossed(
        &self,
        connection: &mut SqliteConnection,
        station_key_id: &str,
        lifecycle_revision: u64,
        attempt_id: &str,
        lease_revision: u64,
        now_ms: u64,
    ) -> Result<bool, PersistenceError> {
        if station_key_id.is_empty() || attempt_id.is_empty() || lease_revision == 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let lifecycle_revision =
            i64::try_from(lifecycle_revision).map_err(|_| PersistenceError::ConstraintViolation)?;
        let lease_revision =
            i64::try_from(lease_revision).map_err(|_| PersistenceError::ConstraintViolation)?;
        let logical_now_ms = advance_circuit_clock(connection, now_ms).await?;
        let now_ms =
            i64::try_from(logical_now_ms).map_err(|_| PersistenceError::ConstraintViolation)?;
        let result = sqlx::query(
            "UPDATE routing_circuit_state_v3
             SET boundary_crossed = 1,
                 updated_at_ms = MAX(updated_at_ms, ?1),
                 monotonic_clock_watermark_ms = MAX(monotonic_clock_watermark_ms, ?1)
             WHERE station_key_id = ?2
               AND station_key_lifecycle_revision = ?3
               AND state = 'half_open'
               AND lease_attempt_id = ?4
               AND lease_id = ?4
               AND lease_revision = ?5
               AND released_at_ms IS NULL
               AND boundary_crossed IS NULL",
        )
        .bind(now_ms)
        .bind(station_key_id)
        .bind(lifecycle_revision)
        .bind(attempt_id)
        .bind(lease_revision)
        .execute(&mut *connection)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Reaps expired Half-Open leases in one write transaction. Leases that
    /// crossed the boundary reopen the circuit; leases that did not are only
    /// released back to idle Half-Open. Every action gets one durable lease
    /// event, so repeated supervisor ticks are idempotent.
    pub(crate) async fn reap_expired_leases(
        &self,
        connection: &mut SqliteConnection,
        now_ms: u64,
        policy_revision: u64,
        consecutive_failure_threshold: u16,
        recovery_success_threshold: u16,
        recovery_wait_ms: u64,
    ) -> Result<u32, PersistenceError> {
        let logical_now_ms = advance_circuit_clock(connection, now_ms).await?;
        let now =
            i64::try_from(logical_now_ms).map_err(|_| PersistenceError::ConstraintViolation)?;
        let rows = sqlx::query(
            "SELECT station_key_id, station_key_lifecycle_revision, state_revision,
                    consecutive_failures, reopen_level, recovery_successes, lease_id,
                    lease_revision, lease_expires_at_ms, boundary_crossed, policy_revision,
                    lease_policy_revision, lease_recovery_success_threshold,
                    lease_recovery_wait_ms
             FROM routing_circuit_state_v3
             WHERE state = 'half_open'
               AND lease_id IS NOT NULL
               AND lease_expires_at_ms IS NOT NULL
               AND lease_expires_at_ms <= ?1
             ORDER BY station_key_id ASC, station_key_lifecycle_revision ASC",
        )
        .bind(now)
        .fetch_all(&mut *connection)
        .await?;
        let mut reaped = 0_u32;
        for row in rows {
            let station_key_id = row.get::<String, _>("station_key_id");
            let lifecycle_revision =
                positive_u64(row.get::<i64, _>("station_key_lifecycle_revision"))?;
            let attempt_id = row.get::<Option<String>, _>("lease_id").ok_or_else(|| {
                PersistenceError::InvariantViolation("expired circuit lease has no id".into())
            })?;
            let state = StationKeyCircuitState::HalfOpen {
                state_revision: positive_u64(row.get::<i64, _>("state_revision"))?,
                lease_id: Some(attempt_id.clone()),
                lease_revision: positive_u64(
                    row.get::<Option<i64>, _>("lease_revision").ok_or_else(|| {
                        PersistenceError::InvariantViolation("expired lease has no revision".into())
                    })?,
                )?,
                lease_expires_at_ms: Some(positive_u64(
                    row.get::<Option<i64>, _>("lease_expires_at_ms")
                        .ok_or_else(|| {
                            PersistenceError::InvariantViolation(
                                "expired lease has no deadline".into(),
                            )
                        })?,
                )?),
                recovery_successes: u16::try_from(row.get::<i64, _>("recovery_successes"))
                    .map_err(|_| {
                        PersistenceError::InvariantViolation("invalid recovery count".into())
                    })?,
                reopen_level: u32::try_from(row.get::<i64, _>("reopen_level")).map_err(|_| {
                    PersistenceError::InvariantViolation("invalid reopen level".into())
                })?,
            };
            let state_policy_revision = positive_u64(row.get::<i64, _>("policy_revision"))?;
            let lease_policy = Some(StationKeyCircuitLeasePolicy {
                policy_revision: positive_u64(
                    row.get::<Option<i64>, _>("lease_policy_revision")
                        .ok_or_else(|| {
                            PersistenceError::InvariantViolation(
                                "active circuit lease has no policy revision".into(),
                            )
                        })?,
                )?,
                recovery_success_threshold: u16::try_from(
                    row.get::<Option<i64>, _>("lease_recovery_success_threshold")
                        .ok_or_else(|| {
                            PersistenceError::InvariantViolation(
                                "active circuit lease has no recovery threshold".into(),
                            )
                        })?,
                )
                .map_err(|_| {
                    PersistenceError::InvariantViolation(
                        "invalid active circuit lease recovery threshold".into(),
                    )
                })?,
                recovery_wait_ms: positive_u64(
                    row.get::<Option<i64>, _>("lease_recovery_wait_ms")
                        .ok_or_else(|| {
                            PersistenceError::InvariantViolation(
                                "active circuit lease has no recovery wait".into(),
                            )
                        })?,
                )?,
            });
            let state_revision = state_revision(&state);
            let original_lease_revision = lease_revision(&state);
            let boundary_crossed = row.get::<Option<i64>, _>("boundary_crossed") == Some(1);
            let reducer = StationKeyCircuit::from_state(
                StationKeyCircuitConfig {
                    policy_revision: policy_revision.max(1),
                    consecutive_failure_threshold: consecutive_failure_threshold.max(1),
                    recovery_success_threshold: recovery_success_threshold.max(1),
                    recovery_wait_ms: recovery_wait_ms.max(1),
                    max_cooldown_ms: MAX_COOLDOWN_MS.max(recovery_wait_ms),
                },
                state_policy_revision,
                lease_policy,
                state,
            )
            .map_err(map_circuit_error)?;
            let mut reducer = reducer;
            let transition = if boundary_crossed {
                reducer
                    .reap_expired_lease(logical_now_ms, &attempt_id)
                    .map_err(map_circuit_error)?
                    .then_some(CircuitTransition::Reopened)
            } else {
                Some(
                    reducer
                        .finish(logical_now_ms, Some(&attempt_id), false, false)
                        .map_err(map_circuit_error)?,
                )
            };
            let Some(transition) = transition else {
                continue;
            };
            update_state_cas(
                connection,
                &station_key_id,
                lifecycle_revision,
                state_revision,
                reducer.policy_revision(),
                reducer.lease_policy(),
                reducer.state(),
                logical_now_ms,
            )
            .await?;
            insert_lease_event(
                connection,
                &station_key_id,
                lifecycle_revision,
                &attempt_id,
                original_lease_revision,
                state_policy_revision,
                state_revision,
                logical_now_ms,
                boundary_crossed,
                transition,
            )
            .await?;
            reaped = reaped.saturating_add(1);
        }
        Ok(reaped)
    }

    pub(crate) async fn list_statuses(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<Vec<StationKeyCircuitStatus>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT station_key_id, station_key_lifecycle_revision, state, state_revision,
                    policy_revision, consecutive_failures, reopen_level, opened_at_ms,
                    cooldown_until_ms, recovery_successes, lease_id, lease_revision,
                    lease_expires_at_ms, lease_policy_revision,
                    lease_recovery_success_threshold, lease_recovery_wait_ms
             FROM routing_circuit_state_v3
             ORDER BY station_key_id ASC, station_key_lifecycle_revision ASC",
        )
        .fetch_all(&mut *connection)
        .await?;
        rows.into_iter().map(row_to_status).collect()
    }

    pub(crate) async fn ensure_state(
        &self,
        connection: &mut SqliteConnection,
        station_key_id: &str,
        lifecycle_revision: u64,
        policy_revision: u64,
        now_ms: u64,
    ) -> Result<StationKeyCircuitStatus, PersistenceError> {
        let revision =
            i64::try_from(lifecycle_revision).map_err(|_| PersistenceError::ConstraintViolation)?;
        let now = i64::try_from(now_ms).map_err(|_| PersistenceError::ConstraintViolation)?;
        let policy_revision =
            i64::try_from(policy_revision).map_err(|_| PersistenceError::ConstraintViolation)?;
        sqlx::query(
            "INSERT INTO routing_circuit_state_v3
                (station_key_id, station_key_lifecycle_revision, state, state_revision,
                 policy_revision, consecutive_failures, reopen_level, recovery_successes,
                 monotonic_clock_watermark_ms, updated_at_ms)
             VALUES (?1, ?2, 'closed', 1, ?3, 0, 0, 0, ?4, ?4)
             ON CONFLICT(station_key_id, station_key_lifecycle_revision) DO NOTHING",
        )
        .bind(station_key_id)
        .bind(revision)
        .bind(policy_revision)
        .bind(now)
        .execute(&mut *connection)
        .await?;
        self.load_status(connection, station_key_id, lifecycle_revision)
            .await?
            .ok_or_else(|| PersistenceError::InvariantViolation("circuit state disappeared".into()))
    }

    pub(crate) async fn load_status(
        &self,
        connection: &mut SqliteConnection,
        station_key_id: &str,
        lifecycle_revision: u64,
    ) -> Result<Option<StationKeyCircuitStatus>, PersistenceError> {
        let revision =
            i64::try_from(lifecycle_revision).map_err(|_| PersistenceError::ConstraintViolation)?;
        let row = sqlx::query(
            "SELECT station_key_id, station_key_lifecycle_revision, state, state_revision,
                    policy_revision, consecutive_failures, reopen_level, opened_at_ms,
                    cooldown_until_ms, recovery_successes, lease_id, lease_revision,
                    lease_expires_at_ms, lease_policy_revision,
                    lease_recovery_success_threshold, lease_recovery_wait_ms
             FROM routing_circuit_state_v3
             WHERE station_key_id = ?1 AND station_key_lifecycle_revision = ?2",
        )
        .bind(station_key_id)
        .bind(revision)
        .fetch_optional(&mut *connection)
        .await?;
        row.map(row_to_status).transpose()
    }

    /// Atomically reserves a Closed or eligible Half-Open key.  The caller
    /// supplies the score gate computed from the same planning snapshot.
    pub(crate) async fn admit(
        &self,
        connection: &mut SqliteConnection,
        station_key_id: &str,
        lifecycle_revision: u64,
        policy_revision: u64,
        now_ms: u64,
        deadline_at_ms: u64,
        score_gate_passed: bool,
        attempt_id: &str,
        consecutive_failure_threshold: u16,
        recovery_success_threshold: u16,
        recovery_wait_ms: u64,
    ) -> Result<CircuitAdmissionResult, PersistenceError> {
        if policy_revision == 0 || attempt_id.is_empty() {
            return Err(PersistenceError::ConstraintViolation);
        }
        let logical_now_ms = advance_circuit_clock(connection, now_ms).await?;
        let config = StationKeyCircuitConfig {
            policy_revision,
            consecutive_failure_threshold: consecutive_failure_threshold.max(1),
            recovery_success_threshold: recovery_success_threshold.max(1),
            recovery_wait_ms: recovery_wait_ms.max(1),
            max_cooldown_ms: MAX_COOLDOWN_MS.max(recovery_wait_ms),
        };
        let status = self
            .ensure_state(
                connection,
                station_key_id,
                lifecycle_revision,
                policy_revision,
                logical_now_ms,
            )
            .await?;
        let expected_revision = state_revision(&status.state);
        let original_policy_revision = status.policy_revision;
        let original_lease_policy = status.lease_policy;
        let original_state = status.state.clone();
        let mut reducer = StationKeyCircuit::from_state(
            config,
            status.policy_revision,
            status.lease_policy,
            status.state,
        )
        .map_err(map_circuit_error)?;
        let admission = reducer
            .admit(
                logical_now_ms,
                deadline_at_ms,
                score_gate_passed,
                attempt_id,
            )
            .map_err(map_circuit_error)?;
        if matches!(
            admission,
            CircuitAdmission::DeniedOpenCooldown
                | CircuitAdmission::DeniedHalfOpenLease
                | CircuitAdmission::DeniedScoreGate
        ) {
            if reducer.state() != &original_state
                || reducer.policy_revision() != original_policy_revision
                || reducer.lease_policy() != original_lease_policy
            {
                update_state_cas(
                    connection,
                    station_key_id,
                    lifecycle_revision,
                    expected_revision,
                    reducer.policy_revision(),
                    reducer.lease_policy(),
                    reducer.state(),
                    logical_now_ms,
                )
                .await?;
            }
            return Ok(match admission {
                CircuitAdmission::DeniedOpenCooldown => CircuitAdmissionResult::DeniedOpenCooldown,
                CircuitAdmission::DeniedHalfOpenLease => {
                    CircuitAdmissionResult::DeniedHalfOpenLease
                }
                CircuitAdmission::DeniedScoreGate => CircuitAdmissionResult::DeniedScoreGate,
                _ => unreachable!(),
            });
        }
        let new_state = reducer.state();
        update_state_cas(
            connection,
            station_key_id,
            lifecycle_revision,
            expected_revision,
            reducer.policy_revision(),
            reducer.lease_policy(),
            new_state,
            logical_now_ms,
        )
        .await?;
        let new_revision = state_revision(new_state);
        Ok(match admission {
            CircuitAdmission::AllowedClosed => CircuitAdmissionResult::AllowedClosed {
                state_revision: new_revision,
            },
            CircuitAdmission::AllowedHalfOpen => CircuitAdmissionResult::AllowedHalfOpen {
                state_revision: new_revision,
                lease_revision: lease_revision(new_state).unwrap_or(new_revision),
            },
            _ => unreachable!(),
        })
    }

    /// Applies one idempotent real-request terminal result and writes the
    /// reducer event in the same transaction as the state CAS.
    pub(crate) async fn finish_attempt(
        &self,
        connection: &mut SqliteConnection,
        input: CircuitTerminalInput<'_>,
    ) -> Result<CircuitTerminalResult, PersistenceError> {
        if input.policy_revision == 0
            || input.attempt_id.is_empty()
            || !matches!(
                input.recovery_origin,
                "normal" | "crash_recovery" | "lease_reaper"
            )
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        let logical_now_ms = advance_circuit_clock(connection, input.now_ms).await?;
        let status = self
            .ensure_state(
                connection,
                input.station_key_id,
                input.lifecycle_revision,
                input.policy_revision,
                logical_now_ms,
            )
            .await?;
        let expected_revision = state_revision(&status.state);
        let active_lease = match &status.state {
            StationKeyCircuitState::HalfOpen {
                lease_id: Some(active_lease),
                lease_revision,
                ..
            } if input.lease_id == Some(active_lease.as_str()) => {
                Some((*lease_revision, status.lease_policy))
            }
            _ => None,
        };
        let active_lease_revision = active_lease.map(|(revision, _)| revision);
        let effective_policy_revision = active_lease
            .and_then(|(_, policy)| policy.map(|policy| policy.policy_revision))
            .unwrap_or(input.policy_revision);
        // A terminal writer may not have carried the circuit lease revision
        // (for example after a process restart). Resolve it from the durable
        // state only when the lease identity still matches. If a caller did
        // carry a revision and it conflicts, treat the result as late rather
        // than allowing it to mutate a newer Half-Open cycle.
        let effective_lease_revision = input.lease_revision.or(active_lease_revision);
        let lease_revision_mismatch = input.lease_revision.is_some()
            && active_lease_revision.is_some()
            && input.lease_revision != active_lease_revision;
        // Idempotency is checked before running the reducer.  Checking only
        // at INSERT time would still mutate the streak for a duplicate
        // terminal message before the UNIQUE event key discarded it.
        let existing_event = sqlx::query(
            "SELECT reducer_commit_sequence FROM routing_circuit_event_v3
             WHERE effect_kind = 'circuit'
               AND station_key_id = ?1
               AND station_key_lifecycle_revision = ?2
               AND attempt_id = ?3
             LIMIT 1",
        )
        .bind(input.station_key_id)
        .bind(
            i64::try_from(input.lifecycle_revision)
                .map_err(|_| PersistenceError::ConstraintViolation)?,
        )
        .bind(input.attempt_id)
        .fetch_optional(&mut *connection)
        .await?;
        if let Some(existing_event) = existing_event {
            return Ok(CircuitTerminalResult {
                applied: false,
                transition: CircuitTransition::IgnoredLateResult,
                state_revision: expected_revision,
                reducer_commit_sequence: positive_u64(
                    existing_event.get::<i64, _>("reducer_commit_sequence"),
                )?,
            });
        }
        if lease_revision_mismatch {
            let reducer_commit_sequence = insert_event(
                connection,
                &input,
                expected_revision,
                CircuitTransition::IgnoredLateResult,
                effective_lease_revision,
                effective_policy_revision,
                logical_now_ms,
            )
            .await?;
            return Ok(CircuitTerminalResult {
                applied: false,
                transition: CircuitTransition::IgnoredLateResult,
                state_revision: expected_revision,
                reducer_commit_sequence,
            });
        }
        let config = StationKeyCircuitConfig {
            policy_revision: input.policy_revision,
            consecutive_failure_threshold: input.consecutive_failure_threshold.max(1),
            recovery_success_threshold: input.recovery_success_threshold.max(1),
            recovery_wait_ms: input.recovery_wait_ms.max(1),
            max_cooldown_ms: MAX_COOLDOWN_MS.max(input.recovery_wait_ms),
        };
        let mut reducer = StationKeyCircuit::from_state(
            config,
            status.policy_revision,
            status.lease_policy,
            status.state,
        )
        .map_err(map_circuit_error)?;
        let transition = reducer
            .finish(
                logical_now_ms,
                input.lease_id,
                input.success,
                input.boundary_crossed && input.affects_circuit,
            )
            .map_err(map_circuit_error)?;
        if matches!(transition, CircuitTransition::IgnoredLateResult) {
            let reducer_commit_sequence = insert_event(
                connection,
                &input,
                expected_revision,
                transition,
                effective_lease_revision,
                effective_policy_revision,
                logical_now_ms,
            )
            .await?;
            return Ok(CircuitTerminalResult {
                applied: false,
                transition,
                state_revision: expected_revision,
                reducer_commit_sequence,
            });
        }
        update_state_cas(
            connection,
            input.station_key_id,
            input.lifecycle_revision,
            expected_revision,
            reducer.policy_revision(),
            reducer.lease_policy(),
            reducer.state(),
            logical_now_ms,
        )
        .await?;
        let reducer_commit_sequence = insert_event(
            connection,
            &input,
            expected_revision,
            transition,
            effective_lease_revision,
            effective_policy_revision,
            logical_now_ms,
        )
        .await?;
        Ok(CircuitTerminalResult {
            applied: true,
            transition,
            state_revision: state_revision(reducer.state()),
            reducer_commit_sequence,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CircuitTerminalInput<'a> {
    pub(crate) station_key_id: &'a str,
    pub(crate) lifecycle_revision: u64,
    pub(crate) policy_revision: u64,
    pub(crate) attempt_id: &'a str,
    pub(crate) lease_id: Option<&'a str>,
    pub(crate) lease_revision: Option<u64>,
    pub(crate) now_ms: u64,
    /// Producer occurrence time is audit-only. Reducer time is derived from
    /// `now_ms` through the durable circuit clock watermark.
    pub(crate) occurred_at_ms: u64,
    pub(crate) success: bool,
    pub(crate) boundary_crossed: bool,
    /// False for deterministic business/capability rejections that should
    /// release a Half-Open lease without changing the Key failure streak.
    pub(crate) affects_circuit: bool,
    pub(crate) failure_code: Option<&'a str>,
    pub(crate) recovery_origin: &'a str,
    pub(crate) retry_disposition: &'a str,
    pub(crate) consecutive_failure_threshold: u16,
    pub(crate) recovery_success_threshold: u16,
    pub(crate) recovery_wait_ms: u64,
}

fn state_revision(state: &StationKeyCircuitState) -> u64 {
    match state {
        StationKeyCircuitState::Closed { state_revision, .. }
        | StationKeyCircuitState::Open { state_revision, .. }
        | StationKeyCircuitState::HalfOpen { state_revision, .. } => *state_revision,
    }
}

fn lease_revision(state: &StationKeyCircuitState) -> Option<u64> {
    match state {
        StationKeyCircuitState::HalfOpen { lease_revision, .. } => Some(*lease_revision),
        _ => None,
    }
}

async fn update_state_cas(
    connection: &mut SqliteConnection,
    station_key_id: &str,
    lifecycle_revision: u64,
    expected_revision: u64,
    policy_revision: u64,
    lease_policy: Option<StationKeyCircuitLeasePolicy>,
    state: &StationKeyCircuitState,
    now_ms: u64,
) -> Result<(), PersistenceError> {
    let revision =
        i64::try_from(lifecycle_revision).map_err(|_| PersistenceError::ConstraintViolation)?;
    let expected =
        i64::try_from(expected_revision).map_err(|_| PersistenceError::ConstraintViolation)?;
    let policy_revision =
        i64::try_from(policy_revision).map_err(|_| PersistenceError::ConstraintViolation)?;
    let now = i64::try_from(now_ms).map_err(|_| PersistenceError::ConstraintViolation)?;
    let values = state_values(state)?;
    let result = sqlx::query(
        "UPDATE routing_circuit_state_v3 SET state = ?1, state_revision = ?2,
            policy_revision = ?3, consecutive_failures = ?4, reopen_level = ?5,
            opened_at_ms = ?6, cooldown_until_ms = ?7, recovery_successes = ?8,
            lease_id = ?9, lease_revision = ?10, lease_attempt_id = ?11,
            lease_expires_at_ms = ?12, lease_deadline_at_ms = ?13,
            lease_policy_revision = ?14, lease_recovery_success_threshold = ?15,
            lease_recovery_wait_ms = ?16, boundary_crossed = NULL,
            released_at_ms = NULL, lease_terminal_state = NULL,
            updated_at_ms = ?17,
            monotonic_clock_watermark_ms = MAX(monotonic_clock_watermark_ms, ?17)
         WHERE station_key_id = ?18 AND station_key_lifecycle_revision = ?19
           AND state_revision = ?20",
    )
    .bind(values.state)
    .bind(i64::try_from(values.state_revision).map_err(|_| PersistenceError::ConstraintViolation)?)
    .bind(policy_revision)
    .bind(i64::from(values.consecutive_failures))
    .bind(i64::from(values.reopen_level))
    .bind(values.opened_at_ms)
    .bind(values.cooldown_until_ms)
    .bind(i64::from(values.recovery_successes))
    .bind(values.lease_id)
    .bind(values.lease_revision)
    .bind(values.lease_attempt_id)
    .bind(values.lease_expires_at_ms)
    .bind(values.lease_deadline_at_ms)
    .bind(
        lease_policy
            .map(|policy| i64::try_from(policy.policy_revision))
            .transpose()
            .map_err(|_| PersistenceError::ConstraintViolation)?,
    )
    .bind(lease_policy.map(|policy| i64::from(policy.recovery_success_threshold)))
    .bind(
        lease_policy
            .map(|policy| i64::try_from(policy.recovery_wait_ms))
            .transpose()
            .map_err(|_| PersistenceError::ConstraintViolation)?,
    )
    .bind(now)
    .bind(station_key_id)
    .bind(revision)
    .bind(expected)
    .execute(&mut *connection)
    .await?;
    if result.rows_affected() != 1 {
        return Err(PersistenceError::DatabaseBusy);
    }
    Ok(())
}

#[derive(Debug)]
struct StateValues<'a> {
    state: &'a str,
    state_revision: u64,
    consecutive_failures: u16,
    reopen_level: u32,
    opened_at_ms: Option<i64>,
    cooldown_until_ms: Option<i64>,
    recovery_successes: u16,
    lease_id: Option<&'a str>,
    lease_revision: Option<i64>,
    lease_attempt_id: Option<&'a str>,
    lease_expires_at_ms: Option<i64>,
    lease_deadline_at_ms: Option<i64>,
}

fn state_values(state: &StationKeyCircuitState) -> Result<StateValues<'_>, PersistenceError> {
    let values = match state {
        StationKeyCircuitState::Closed {
            state_revision,
            consecutive_failures,
            reopen_level,
        } => StateValues {
            state: "closed",
            state_revision: *state_revision,
            consecutive_failures: *consecutive_failures,
            reopen_level: *reopen_level,
            opened_at_ms: None,
            cooldown_until_ms: None,
            recovery_successes: 0,
            lease_id: None,
            lease_revision: None,
            lease_attempt_id: None,
            lease_expires_at_ms: None,
            lease_deadline_at_ms: None,
        },
        StationKeyCircuitState::Open {
            state_revision,
            opened_at_ms,
            cooldown_until_ms,
            consecutive_failures,
            reopen_level,
        } => StateValues {
            state: "open",
            state_revision: *state_revision,
            consecutive_failures: *consecutive_failures,
            reopen_level: *reopen_level,
            opened_at_ms: Some(
                i64::try_from(*opened_at_ms).map_err(|_| PersistenceError::ConstraintViolation)?,
            ),
            cooldown_until_ms: Some(
                i64::try_from(*cooldown_until_ms)
                    .map_err(|_| PersistenceError::ConstraintViolation)?,
            ),
            recovery_successes: 0,
            lease_id: None,
            lease_revision: None,
            lease_attempt_id: None,
            lease_expires_at_ms: None,
            lease_deadline_at_ms: None,
        },
        StationKeyCircuitState::HalfOpen {
            state_revision,
            lease_id,
            lease_revision,
            lease_expires_at_ms,
            recovery_successes,
            reopen_level,
        } => StateValues {
            state: "half_open",
            state_revision: *state_revision,
            consecutive_failures: 0,
            reopen_level: *reopen_level,
            opened_at_ms: None,
            cooldown_until_ms: None,
            recovery_successes: *recovery_successes,
            lease_id: lease_id.as_deref(),
            lease_revision: Some(
                i64::try_from(*lease_revision)
                    .map_err(|_| PersistenceError::ConstraintViolation)?,
            ),
            lease_attempt_id: lease_id.as_deref(),
            lease_expires_at_ms: lease_expires_at_ms
                .map(|value| {
                    i64::try_from(value).map_err(|_| PersistenceError::ConstraintViolation)
                })
                .transpose()?,
            lease_deadline_at_ms: lease_expires_at_ms
                .map(|value| {
                    i64::try_from(value).map_err(|_| PersistenceError::ConstraintViolation)
                })
                .transpose()?,
        },
    };
    Ok(values)
}

async fn insert_event(
    connection: &mut SqliteConnection,
    input: &CircuitTerminalInput<'_>,
    expected_state_revision: u64,
    transition: CircuitTransition,
    effective_lease_revision: Option<u64>,
    effective_policy_revision: u64,
    created_at_ms: u64,
) -> Result<u64, PersistenceError> {
    let outcome = if input.success {
        "success"
    } else if input.boundary_crossed {
        "attributable_failure"
    } else {
        "excluded"
    };
    // State revision does not advance for ignored late results, but every
    // terminal event still needs its own per-key reducer sequence. Allocate
    // from the durable event ledger so repeated late results cannot collide
    // on the UNIQUE(key, lifecycle, reducer_commit_sequence) constraint.
    let sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(reducer_commit_sequence), 0) + 1
         FROM routing_circuit_event_v3
         WHERE station_key_id = ?1 AND station_key_lifecycle_revision = ?2",
    )
    .bind(input.station_key_id)
    .bind(
        i64::try_from(input.lifecycle_revision)
            .map_err(|_| PersistenceError::ConstraintViolation)?,
    )
    .fetch_one(&mut *connection)
    .await?;
    if sequence <= 0 {
        return Err(PersistenceError::InvariantViolation(
            "circuit reducer sequence overflow".into(),
        ));
    }
    let expected_revision = i64::try_from(expected_state_revision)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    sqlx::query(
        "INSERT INTO routing_circuit_event_v3
            (event_id, effect_kind, source, attempt_id, station_key_id,
             station_key_lifecycle_revision, reducer_commit_sequence, policy_revision,
             expected_state_revision, occurred_at_ms, canonical_outcome, failure_code,
             recovery_origin, retry_disposition, lease_revision, boundary_crossed,
             applied, created_at_ms)
         VALUES (?1, 'circuit', 'real_request', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                  ?11, ?12, ?13, ?14, ?15, ?16)
         ON CONFLICT(effect_kind, station_key_id, station_key_lifecycle_revision, attempt_id)
         DO NOTHING",
    )
    .bind(format!("circuit:{}", input.attempt_id))
    .bind(input.attempt_id)
    .bind(input.station_key_id)
    .bind(
        i64::try_from(input.lifecycle_revision)
            .map_err(|_| PersistenceError::ConstraintViolation)?,
    )
    .bind(sequence)
    .bind(
        i64::try_from(effective_policy_revision)
            .map_err(|_| PersistenceError::ConstraintViolation)?,
    )
    .bind(expected_revision)
    .bind(i64::try_from(input.occurred_at_ms).map_err(|_| PersistenceError::ConstraintViolation)?)
    .bind(outcome)
    .bind(input.failure_code)
    .bind(input.recovery_origin)
    .bind(input.retry_disposition)
    .bind(
        effective_lease_revision
            .map(|value| i64::try_from(value))
            .transpose()
            .map_err(|_| PersistenceError::ConstraintViolation)?,
    )
    .bind(i64::from(input.boundary_crossed))
    .bind(i64::from(!matches!(
        transition,
        CircuitTransition::IgnoredLateResult
    )))
    .bind(i64::try_from(created_at_ms).map_err(|_| PersistenceError::ConstraintViolation)?)
    .execute(&mut *connection)
    .await?;
    positive_u64(sequence)
}

async fn insert_lease_event(
    connection: &mut SqliteConnection,
    station_key_id: &str,
    lifecycle_revision: u64,
    attempt_id: &str,
    lease_revision: Option<u64>,
    policy_revision: u64,
    expected_state_revision: u64,
    now_ms: u64,
    boundary_crossed: bool,
    transition: CircuitTransition,
) -> Result<(), PersistenceError> {
    let sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(reducer_commit_sequence), 0) + 1
         FROM routing_circuit_event_v3
         WHERE station_key_id = ?1 AND station_key_lifecycle_revision = ?2",
    )
    .bind(station_key_id)
    .bind(i64::try_from(lifecycle_revision).map_err(|_| PersistenceError::ConstraintViolation)?)
    .fetch_one(&mut *connection)
    .await?;
    let event_id = format!("lease-reaper:{attempt_id}");
    sqlx::query(
        "INSERT INTO routing_circuit_event_v3
            (event_id, effect_kind, source, attempt_id, station_key_id,
             station_key_lifecycle_revision, reducer_commit_sequence, policy_revision,
             expected_state_revision, occurred_at_ms, canonical_outcome, failure_code,
             recovery_origin, retry_disposition, lease_revision, boundary_crossed,
             applied, created_at_ms)
         VALUES (?1, 'lease', 'real_request', ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                  'excluded', ?9, 'lease_reaper', 'stop_request', ?10, ?11, 1, ?8)
         ON CONFLICT(effect_kind, station_key_id, station_key_lifecycle_revision, attempt_id)
         DO NOTHING",
    )
    .bind(event_id)
    .bind(attempt_id)
    .bind(station_key_id)
    .bind(i64::try_from(lifecycle_revision).map_err(|_| PersistenceError::ConstraintViolation)?)
    .bind(sequence)
    .bind(i64::try_from(policy_revision).map_err(|_| PersistenceError::ConstraintViolation)?)
    .bind(
        i64::try_from(expected_state_revision)
            .map_err(|_| PersistenceError::ConstraintViolation)?,
    )
    .bind(i64::try_from(now_ms).map_err(|_| PersistenceError::ConstraintViolation)?)
    .bind(match transition {
        CircuitTransition::Reopened => "lease_expired",
        _ => "lease_released",
    })
    .bind(
        lease_revision
            .map(|value| i64::try_from(value))
            .transpose()
            .map_err(|_| PersistenceError::ConstraintViolation)?,
    )
    .bind(i64::from(boundary_crossed))
    .execute(&mut *connection)
    .await?;
    Ok(())
}

fn row_to_status(
    row: sqlx::sqlite::SqliteRow,
) -> Result<StationKeyCircuitStatus, PersistenceError> {
    let state_name: String = row.get("state");
    let policy_revision = positive_u64(row.get::<i64, _>("policy_revision"))?;
    let lease_policy_revision = row.get::<Option<i64>, _>("lease_policy_revision");
    let lease_recovery_success_threshold =
        row.get::<Option<i64>, _>("lease_recovery_success_threshold");
    let lease_recovery_wait_ms = row.get::<Option<i64>, _>("lease_recovery_wait_ms");
    let lease_policy = match (
        lease_policy_revision,
        lease_recovery_success_threshold,
        lease_recovery_wait_ms,
    ) {
        (None, None, None) => None,
        (Some(revision), Some(threshold), Some(wait_ms)) => Some(StationKeyCircuitLeasePolicy {
            policy_revision: positive_u64(revision)?,
            recovery_success_threshold: u16::try_from(threshold).map_err(|_| {
                PersistenceError::InvariantViolation(
                    "invalid circuit lease recovery threshold".into(),
                )
            })?,
            recovery_wait_ms: positive_u64(wait_ms)?,
        }),
        _ => {
            return Err(PersistenceError::InvariantViolation(
                "partial circuit lease policy snapshot".into(),
            ));
        }
    };
    let state_revision = positive_u64(row.get::<i64, _>("state_revision"))?;
    let consecutive_failures =
        u16::try_from(row.get::<i64, _>("consecutive_failures")).map_err(|_| {
            PersistenceError::InvariantViolation("invalid circuit failure count".into())
        })?;
    let reopen_level = u32::try_from(row.get::<i64, _>("reopen_level"))
        .map_err(|_| PersistenceError::InvariantViolation("invalid circuit reopen level".into()))?;
    let recovery_successes =
        u16::try_from(row.get::<i64, _>("recovery_successes")).map_err(|_| {
            PersistenceError::InvariantViolation("invalid circuit recovery count".into())
        })?;
    let state = match state_name.as_str() {
        "closed" => StationKeyCircuitState::Closed {
            state_revision,
            consecutive_failures,
            reopen_level,
        },
        "open" => StationKeyCircuitState::Open {
            state_revision,
            opened_at_ms: positive_u64(row.get::<Option<i64>, _>("opened_at_ms").ok_or_else(
                || PersistenceError::InvariantViolation("open circuit missing opened_at".into()),
            )?)?,
            cooldown_until_ms: positive_u64(
                row.get::<Option<i64>, _>("cooldown_until_ms")
                    .ok_or_else(|| {
                        PersistenceError::InvariantViolation("open circuit missing cooldown".into())
                    })?,
            )?,
            consecutive_failures,
            reopen_level,
        },
        "half_open" => StationKeyCircuitState::HalfOpen {
            state_revision,
            lease_id: row.get::<Option<String>, _>("lease_id"),
            lease_revision: positive_u64(row.get::<Option<i64>, _>("lease_revision").unwrap_or(
                i64::try_from(state_revision).map_err(|_| {
                    PersistenceError::InvariantViolation(
                        "half-open circuit revision exceeds SQLite range".into(),
                    )
                })?,
            ))?,
            lease_expires_at_ms: row
                .get::<Option<i64>, _>("lease_expires_at_ms")
                .map(positive_u64)
                .transpose()?,
            recovery_successes,
            reopen_level,
        },
        _ => {
            return Err(PersistenceError::InvariantViolation(
                "unknown circuit state".into(),
            ))
        }
    };
    Ok(StationKeyCircuitStatus {
        station_key_id: row.get("station_key_id"),
        lifecycle_revision: positive_u64(row.get("station_key_lifecycle_revision"))?,
        policy_revision,
        lease_policy,
        state,
    })
}

/// Advances the database-wide circuit clock under the caller's write
/// transaction and returns the logical reducer time. Producer event times are
/// intentionally excluded from this watermark.
async fn advance_circuit_clock(
    connection: &mut SqliteConnection,
    system_utc_now_ms: u64,
) -> Result<u64, PersistenceError> {
    let sampled =
        i64::try_from(system_utc_now_ms).map_err(|_| PersistenceError::ConstraintViolation)?;
    sqlx::query(
        "UPDATE routing_circuit_clock_v3
         SET watermark_ms = MAX(watermark_ms, ?1),
             updated_at_ms = MAX(updated_at_ms, watermark_ms, ?1)
         WHERE singleton_key = 1",
    )
    .bind(sampled)
    .execute(&mut *connection)
    .await?;
    let watermark: i64 = sqlx::query_scalar(
        "SELECT watermark_ms FROM routing_circuit_clock_v3 WHERE singleton_key = 1",
    )
    .fetch_one(&mut *connection)
    .await?;
    positive_u64(watermark)
}

fn positive_u64(value: i64) -> Result<u64, PersistenceError> {
    u64::try_from(value)
        .map_err(|_| PersistenceError::InvariantViolation("negative circuit value".into()))
}

fn map_circuit_error(error: CircuitError) -> PersistenceError {
    match error {
        CircuitError::InvalidConfig | CircuitError::InvalidTime | CircuitError::InvalidLease => {
            PersistenceError::ConstraintViolation
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::runtime::PersistenceRuntime;

    fn terminal_input<'a>(attempt_id: &'a str, now_ms: u64) -> CircuitTerminalInput<'a> {
        CircuitTerminalInput {
            station_key_id: "key-1",
            lifecycle_revision: 1,
            policy_revision: 1,
            attempt_id,
            lease_id: Some(attempt_id),
            lease_revision: None,
            now_ms,
            occurred_at_ms: now_ms,
            success: false,
            boundary_crossed: true,
            affects_circuit: true,
            failure_code: Some("upstream_failed"),
            recovery_origin: "normal",
            retry_disposition: "retryable_before_commit",
            consecutive_failure_threshold: 3,
            recovery_success_threshold: 2,
            recovery_wait_ms: 10,
        }
    }

    #[tokio::test]
    async fn neutral_terminal_does_not_advance_failure_streak() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&root.path().join("neutral-circuit.sqlite3"))
                .await
                .expect("initialize runtime");
        let store = StationKeyCircuitStore;
        let mut write = runtime.handle().begin_write().await.expect("begin write");

        let mut neutral = terminal_input("neutral-attempt", 1);
        neutral.affects_circuit = false;
        neutral.failure_code = Some("upstream_insufficient_balance");
        store
            .finish_attempt(write.connection(), neutral)
            .await
            .expect("record neutral terminal");
        store
            .finish_attempt(write.connection(), terminal_input("ordinary-failure", 2))
            .await
            .expect("record ordinary failure");

        let status = store
            .load_status(write.connection(), "key-1", 1)
            .await
            .expect("load circuit")
            .expect("circuit status");
        assert!(matches!(
            status.state,
            StationKeyCircuitState::Closed {
                consecutive_failures: 1,
                ..
            }
        ));
        write
            .commit()
            .await
            .expect("commit neutral circuit fixture");
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn persistence_gate_is_fail_closed_until_supervised_health_check() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("circuit-gate.sqlite3"))
            .await
            .expect("initialize runtime");
        let store = StationKeyCircuitStore;
        let mut write = runtime.handle().begin_write().await.expect("begin write");
        store
            .mark_persistence_unavailable(write.connection(), "key-gated", 1, 10)
            .await
            .expect("open persistence gate");
        assert!(store
            .persistence_gate_active(write.connection(), "key-gated", 1)
            .await
            .expect("read persistence gate"));

        let cleared = store
            .health_check_and_clear_persistence_gates(write.connection(), 20)
            .await
            .expect("health check and clear");
        assert_eq!(cleared, 1);
        assert!(!store
            .persistence_gate_active(write.connection(), "key-gated", 1)
            .await
            .expect("read cleared persistence gate"));
        let revision: i64 = sqlx::query_scalar(
            "SELECT check_revision FROM routing_circuit_persistence_health_v3
             WHERE singleton_key = 1",
        )
        .fetch_one(write.connection())
        .await
        .expect("health check revision");
        assert_eq!(revision, 1);
        write.commit().await.expect("commit gate fixture");
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn duplicate_terminal_event_does_not_advance_failure_streak_twice() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("circuit.sqlite3"))
            .await
            .expect("initialize runtime");
        let store = StationKeyCircuitStore;
        let mut write = runtime.handle().begin_write().await.expect("begin write");
        assert_eq!(
            store
                .finish_attempt(write.connection(), terminal_input("attempt-0", 1))
                .await
                .expect("first failure")
                .transition,
            CircuitTransition::Observed
        );
        assert_eq!(
            store
                .finish_attempt(write.connection(), terminal_input("attempt-0", 2))
                .await
                .expect("duplicate failure")
                .transition,
            CircuitTransition::IgnoredLateResult
        );
        store
            .finish_attempt(write.connection(), terminal_input("attempt-1", 3))
            .await
            .expect("second failure");
        store
            .finish_attempt(write.connection(), terminal_input("attempt-2", 4))
            .await
            .expect("third failure");
        // Late results arrive after the circuit is already Open. They must be
        // recorded for audit without trying to mutate the new cycle or
        // colliding on the state revision used by the reducer.
        assert_eq!(
            store
                .finish_attempt(write.connection(), terminal_input("attempt-3", 5))
                .await
                .expect("late failure")
                .transition,
            CircuitTransition::IgnoredLateResult
        );
        assert_eq!(
            store
                .finish_attempt(write.connection(), terminal_input("attempt-4", 6))
                .await
                .expect("second late failure")
                .transition,
            CircuitTransition::IgnoredLateResult
        );
        write.commit().await.expect("commit circuit state");

        let mut read = runtime.handle().begin_read().await.expect("read state");
        let status = store
            .load_status(read.connection(), "key-1", 1)
            .await
            .expect("load status")
            .expect("status");
        assert!(matches!(status.state, StationKeyCircuitState::Open { .. }));
        let event_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_circuit_event_v3
             WHERE station_key_id = 'key-1' AND station_key_lifecycle_revision = 1",
        )
        .fetch_one(read.connection())
        .await
        .expect("event count");
        assert_eq!(event_count, 5);
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn durable_circuit_enforces_cooldown_half_open_lease_and_recovery_threshold() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&root.path().join("circuit-lifecycle.sqlite3"))
                .await
                .expect("initialize runtime");
        let store = StationKeyCircuitStore;
        let mut write = runtime.handle().begin_write().await.expect("begin write");
        for (attempt, now) in [("fail-0", 1), ("fail-1", 2), ("fail-2", 3)] {
            let result = store
                .finish_attempt(write.connection(), terminal_input(attempt, now))
                .await
                .expect("failure terminal");
            if attempt == "fail-2" {
                assert_eq!(result.transition, CircuitTransition::Opened);
            }
        }
        assert_eq!(
            store
                .admit(
                    write.connection(),
                    "key-1",
                    1,
                    1,
                    5,
                    100,
                    true,
                    "probe-0",
                    3,
                    2,
                    10,
                )
                .await
                .expect("cooldown admission"),
            CircuitAdmissionResult::DeniedOpenCooldown
        );
        assert!(matches!(
            store
                .admit(
                    write.connection(),
                    "key-1",
                    1,
                    1,
                    13,
                    100,
                    true,
                    "probe-0",
                    3,
                    2,
                    10,
                )
                .await
                .expect("half-open admission"),
            CircuitAdmissionResult::AllowedHalfOpen { .. }
        ));
        assert_eq!(
            store
                .admit(
                    write.connection(),
                    "key-1",
                    1,
                    1,
                    13,
                    100,
                    true,
                    "probe-1",
                    3,
                    2,
                    10,
                )
                .await
                .expect("second half-open admission"),
            CircuitAdmissionResult::DeniedHalfOpenLease
        );
        let mut recovery = terminal_input("probe-0", 14);
        recovery.success = true;
        recovery.failure_code = None;
        recovery.retry_disposition = "end";
        assert_eq!(
            store
                .finish_attempt(write.connection(), recovery)
                .await
                .expect("first recovery")
                .transition,
            CircuitTransition::RecoverySucceeded
        );
        assert!(matches!(
            store
                .admit(
                    write.connection(),
                    "key-1",
                    1,
                    1,
                    14,
                    100,
                    true,
                    "probe-1",
                    3,
                    2,
                    10,
                )
                .await
                .expect("second recovery admission"),
            CircuitAdmissionResult::AllowedHalfOpen { .. }
        ));
        let mut final_recovery = terminal_input("probe-1", 15);
        final_recovery.success = true;
        final_recovery.failure_code = None;
        final_recovery.retry_disposition = "end";
        assert_eq!(
            store
                .finish_attempt(write.connection(), final_recovery)
                .await
                .expect("second recovery")
                .transition,
            CircuitTransition::Closed
        );
        write.commit().await.expect("commit circuit lifecycle");
        let mut read = runtime.handle().begin_read().await.expect("read state");
        let status = store
            .load_status(read.connection(), "key-1", 1)
            .await
            .expect("load final state")
            .expect("final state");
        assert!(matches!(
            status.state,
            StationKeyCircuitState::Closed { .. }
        ));
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn boundary_mark_and_reaper_reopen_only_after_outbound_boundary() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&root.path().join("circuit-reaper.sqlite3"))
                .await
                .expect("initialize runtime");
        let store = StationKeyCircuitStore;
        let mut write = runtime.handle().begin_write().await.expect("begin write");
        for (attempt, now) in [("fail-0", 1), ("fail-1", 2), ("fail-2", 3)] {
            store
                .finish_attempt(write.connection(), terminal_input(attempt, now))
                .await
                .expect("open circuit");
        }
        let admission = store
            .admit(
                write.connection(),
                "key-1",
                1,
                1,
                20,
                100,
                true,
                "probe-attempt",
                3,
                2,
                10,
            )
            .await
            .expect("half-open admission");
        let lease_revision = match admission {
            CircuitAdmissionResult::AllowedHalfOpen { lease_revision, .. } => lease_revision,
            other => panic!("expected half-open admission, got {other:?}"),
        };
        // An expired lease that never crossed the boundary is released to idle
        // Half-Open and does not open the circuit again.
        let reaped = store
            .reap_expired_leases(write.connection(), 100, 1, 3, 2, 10)
            .await
            .expect("reap unstarted lease");
        assert_eq!(reaped, 1);
        let status = store
            .load_status(write.connection(), "key-1", 1)
            .await
            .expect("load idle half-open")
            .expect("status");
        assert!(matches!(
            status.state,
            StationKeyCircuitState::HalfOpen { lease_id: None, .. }
        ));

        let admission = store
            .admit(
                write.connection(),
                "key-1",
                1,
                1,
                102,
                110,
                true,
                "probe-attempt-2",
                3,
                2,
                10,
            )
            .await
            .expect("second half-open admission");
        let lease_revision_2 = match admission {
            CircuitAdmissionResult::AllowedHalfOpen { lease_revision, .. } => lease_revision,
            other => panic!("expected second half-open admission, got {other:?}"),
        };
        assert!(store
            .mark_boundary_crossed(
                write.connection(),
                "key-1",
                1,
                "probe-attempt-2",
                lease_revision_2,
                103,
            )
            .await
            .expect("mark boundary"));
        assert!(!store
            .mark_boundary_crossed(
                write.connection(),
                "key-1",
                1,
                "probe-attempt-2",
                lease_revision_2,
                104,
            )
            .await
            .expect("duplicate boundary mark"));
        assert_ne!(lease_revision, lease_revision_2);
        assert_eq!(
            store
                .reap_expired_leases(write.connection(), 111, 1, 3, 2, 10)
                .await
                .expect("reap crossed lease"),
            1
        );
        let status = store
            .load_status(write.connection(), "key-1", 1)
            .await
            .expect("load reopened")
            .expect("status");
        assert!(matches!(status.state, StationKeyCircuitState::Open { .. }));
        write.commit().await.expect("commit reaper state");
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn stale_explicit_lease_revision_is_audited_without_mutating_state() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&root.path().join("circuit-stale-lease.sqlite3"))
                .await
                .expect("initialize runtime");
        let store = StationKeyCircuitStore;
        let mut write = runtime.handle().begin_write().await.expect("begin write");
        for (attempt, now) in [("fail-0", 1), ("fail-1", 2), ("fail-2", 3)] {
            store
                .finish_attempt(write.connection(), terminal_input(attempt, now))
                .await
                .expect("open circuit");
        }
        let admission = store
            .admit(
                write.connection(),
                "key-1",
                1,
                1,
                20,
                100,
                true,
                "probe-stale",
                3,
                2,
                10,
            )
            .await
            .expect("half-open admission");
        let (state_revision, lease_revision) = match admission {
            CircuitAdmissionResult::AllowedHalfOpen {
                state_revision,
                lease_revision,
            } => (state_revision, lease_revision),
            other => panic!("expected half-open admission, got {other:?}"),
        };
        assert!(store
            .mark_boundary_crossed(
                write.connection(),
                "key-1",
                1,
                "probe-stale",
                lease_revision,
                21,
            )
            .await
            .expect("mark boundary"));

        let mut stale = terminal_input("probe-stale", 22);
        stale.lease_id = Some("probe-stale");
        stale.lease_revision = Some(lease_revision.saturating_add(1));
        assert_eq!(
            store
                .finish_attempt(write.connection(), stale)
                .await
                .expect("stale terminal result")
                .transition,
            CircuitTransition::IgnoredLateResult
        );
        let status = store
            .load_status(write.connection(), "key-1", 1)
            .await
            .expect("load circuit")
            .expect("circuit status");
        assert!(matches!(
            status.state,
            StationKeyCircuitState::HalfOpen {
                state_revision: current_revision,
                lease_id: Some(ref lease_id),
                ..
            } if current_revision == state_revision && lease_id == "probe-stale"
        ));
        let event: (String, i64) = sqlx::query_as(
            "SELECT canonical_outcome, lease_revision
             FROM routing_circuit_event_v3
             WHERE effect_kind = 'circuit' AND attempt_id = 'probe-stale'",
        )
        .fetch_one(write.connection())
        .await
        .expect("stale audit event");
        assert_eq!(event.0, "attributable_failure");
        assert_eq!(
            event.1,
            i64::try_from(lease_revision + 1).expect("lease revision")
        );

        write.commit().await.expect("commit stale audit");
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn lowered_failure_threshold_opens_on_admission_while_raise_preserves_streak() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&root.path().join("policy-change.sqlite3"))
                .await
                .expect("initialize runtime");
        let store = StationKeyCircuitStore;
        let mut write = runtime.handle().begin_write().await.expect("begin write");

        for (attempt, now) in [("fail-0", 1), ("fail-1", 2)] {
            store
                .finish_attempt(write.connection(), terminal_input(attempt, now))
                .await
                .expect("record failure streak");
        }
        assert_eq!(
            store
                .admit(
                    write.connection(),
                    "key-1",
                    1,
                    2,
                    3,
                    100,
                    true,
                    "lowered-threshold",
                    2,
                    2,
                    10,
                )
                .await
                .expect("admit with lower threshold"),
            CircuitAdmissionResult::DeniedOpenCooldown
        );
        let lowered = store
            .load_status(write.connection(), "key-1", 1)
            .await
            .expect("load lowered threshold state")
            .expect("state");
        assert_eq!(lowered.policy_revision, 2);
        assert!(matches!(
            lowered.state,
            StationKeyCircuitState::Open {
                consecutive_failures: 2,
                ..
            }
        ));

        for (attempt, now) in [("other-fail-0", 4), ("other-fail-1", 5)] {
            let mut input = terminal_input(attempt, now);
            input.station_key_id = "key-2";
            store
                .finish_attempt(write.connection(), input)
                .await
                .expect("record second key streak");
        }
        assert!(matches!(
            store
                .admit(
                    write.connection(),
                    "key-2",
                    1,
                    2,
                    6,
                    100,
                    true,
                    "raised-threshold",
                    5,
                    2,
                    10,
                )
                .await
                .expect("admit with raised threshold"),
            CircuitAdmissionResult::AllowedClosed { .. }
        ));
        let raised = store
            .load_status(write.connection(), "key-2", 1)
            .await
            .expect("load raised threshold state")
            .expect("state");
        assert_eq!(raised.policy_revision, 2);
        assert!(matches!(
            raised.state,
            StationKeyCircuitState::Closed {
                consecutive_failures: 2,
                ..
            }
        ));

        write.commit().await.expect("commit policy change states");
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn open_cooldown_is_not_shortened_by_new_policy() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("open-policy.sqlite3"))
            .await
            .expect("initialize runtime");
        let store = StationKeyCircuitStore;
        let mut write = runtime.handle().begin_write().await.expect("begin write");
        for (attempt, now) in [("fail-0", 1), ("fail-1", 2), ("fail-2", 3)] {
            let mut input = terminal_input(attempt, now);
            input.recovery_wait_ms = 100;
            store
                .finish_attempt(write.connection(), input)
                .await
                .expect("open circuit");
        }
        assert_eq!(
            store
                .admit(
                    write.connection(),
                    "key-1",
                    1,
                    2,
                    4,
                    100,
                    true,
                    "new-policy",
                    3,
                    2,
                    1,
                )
                .await
                .expect("admit during existing cooldown"),
            CircuitAdmissionResult::DeniedOpenCooldown
        );
        let status = store
            .load_status(write.connection(), "key-1", 1)
            .await
            .expect("load open state")
            .expect("state");
        assert_eq!(status.policy_revision, 2);
        assert!(matches!(
            status.state,
            StationKeyCircuitState::Open {
                opened_at_ms: 3,
                cooldown_until_ms: 103,
                ..
            }
        ));
        write.commit().await.expect("commit open state");
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn committed_half_open_lease_finishes_with_application_policy_revision() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("lease-policy.sqlite3"))
            .await
            .expect("initialize runtime");
        let store = StationKeyCircuitStore;
        let mut write = runtime.handle().begin_write().await.expect("begin write");
        for (attempt, now) in [("fail-0", 1), ("fail-1", 2), ("fail-2", 3)] {
            store
                .finish_attempt(write.connection(), terminal_input(attempt, now))
                .await
                .expect("open circuit");
        }
        store
            .admit(
                write.connection(),
                "key-1",
                1,
                1,
                13,
                100,
                true,
                "old-lease",
                3,
                2,
                10,
            )
            .await
            .expect("admit old revision lease");

        let mut old_lease_success = terminal_input("old-lease", 14);
        old_lease_success.policy_revision = 2;
        old_lease_success.success = true;
        old_lease_success.failure_code = None;
        old_lease_success.retry_disposition = "end";
        old_lease_success.recovery_success_threshold = 1;
        old_lease_success.recovery_wait_ms = 1;
        assert_eq!(
            store
                .finish_attempt(write.connection(), old_lease_success)
                .await
                .expect("finish old revision lease")
                .transition,
            CircuitTransition::RecoverySucceeded
        );
        let after_old_lease = store
            .load_status(write.connection(), "key-1", 1)
            .await
            .expect("load idle half-open")
            .expect("state");
        assert_eq!(after_old_lease.policy_revision, 1);
        assert!(after_old_lease.lease_policy.is_none());

        store
            .admit(
                write.connection(),
                "key-1",
                1,
                2,
                14,
                100,
                true,
                "new-lease",
                3,
                1,
                1,
            )
            .await
            .expect("admit new revision lease");
        let mut new_lease_success = terminal_input("new-lease", 15);
        new_lease_success.policy_revision = 2;
        new_lease_success.success = true;
        new_lease_success.failure_code = None;
        new_lease_success.retry_disposition = "end";
        new_lease_success.recovery_success_threshold = 1;
        new_lease_success.recovery_wait_ms = 1;
        assert_eq!(
            store
                .finish_attempt(write.connection(), new_lease_success)
                .await
                .expect("finish new revision lease")
                .transition,
            CircuitTransition::Closed
        );
        let event_policy: i64 = sqlx::query_scalar(
            "SELECT policy_revision FROM routing_circuit_event_v3
             WHERE effect_kind = 'circuit' AND attempt_id = 'old-lease'",
        )
        .fetch_one(write.connection())
        .await
        .expect("old lease event policy");
        assert_eq!(event_policy, 1);

        write.commit().await.expect("commit lease revisions");
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn durable_clock_watermark_survives_rollback_and_ignores_event_time() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("clock.sqlite3"))
            .await
            .expect("initialize runtime");
        let store = StationKeyCircuitStore;
        let mut write = runtime.handle().begin_write().await.expect("begin write");
        for (attempt, now) in [("fail-0", 98), ("fail-1", 99), ("fail-2", 100)] {
            let mut input = terminal_input(attempt, now);
            if attempt == "fail-2" {
                input.occurred_at_ms = 999_999;
            }
            store
                .finish_attempt(write.connection(), input)
                .await
                .expect("open with controlled clock");
        }
        store
            .admit(
                write.connection(),
                "key-1",
                1,
                1,
                110,
                200,
                true,
                "rollback-lease",
                3,
                2,
                10,
            )
            .await
            .expect("admit half-open lease");
        let mut failure_after_rollback = terminal_input("rollback-lease", 50);
        failure_after_rollback.policy_revision = 2;
        failure_after_rollback.recovery_wait_ms = 1;
        assert_eq!(
            store
                .finish_attempt(write.connection(), failure_after_rollback)
                .await
                .expect("finish after wall-clock rollback")
                .transition,
            CircuitTransition::Reopened
        );
        let status = store
            .load_status(write.connection(), "key-1", 1)
            .await
            .expect("load reopened state")
            .expect("state");
        assert_eq!(status.policy_revision, 1);
        assert!(matches!(
            status.state,
            StationKeyCircuitState::Open {
                opened_at_ms: 110,
                cooldown_until_ms: 130,
                reopen_level: 2,
                ..
            }
        ));
        let watermark: i64 = sqlx::query_scalar(
            "SELECT watermark_ms FROM routing_circuit_clock_v3 WHERE singleton_key = 1",
        )
        .fetch_one(write.connection())
        .await
        .expect("clock watermark");
        assert_eq!(watermark, 110);
        let future_occurred_at: i64 = sqlx::query_scalar(
            "SELECT occurred_at_ms FROM routing_circuit_event_v3
             WHERE effect_kind = 'circuit' AND attempt_id = 'fail-2'",
        )
        .fetch_one(write.connection())
        .await
        .expect("future audit event");
        assert_eq!(future_occurred_at, 999_999);

        write.commit().await.expect("commit clock state");
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn lease_reaper_uses_durable_clock_after_wall_clock_rollback() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("reaper-clock.sqlite3"))
            .await
            .expect("initialize runtime");
        let store = StationKeyCircuitStore;
        let mut write = runtime.handle().begin_write().await.expect("begin write");
        for (attempt, now) in [("fail-0", 1), ("fail-1", 2), ("fail-2", 3)] {
            store
                .finish_attempt(write.connection(), terminal_input(attempt, now))
                .await
                .expect("open circuit");
        }
        let admission = store
            .admit(
                write.connection(),
                "key-1",
                1,
                1,
                13,
                30,
                true,
                "reaper-lease",
                3,
                2,
                10,
            )
            .await
            .expect("admit reaper lease");
        let lease_revision = match admission {
            CircuitAdmissionResult::AllowedHalfOpen { lease_revision, .. } => lease_revision,
            other => panic!("expected half-open admission, got {other:?}"),
        };
        assert!(store
            .mark_boundary_crossed(
                write.connection(),
                "key-1",
                1,
                "reaper-lease",
                lease_revision,
                14,
            )
            .await
            .expect("mark boundary"));

        let mut clock_advance = terminal_input("clock-advance", 40);
        clock_advance.station_key_id = "key-2";
        clock_advance.boundary_crossed = false;
        store
            .finish_attempt(write.connection(), clock_advance)
            .await
            .expect("advance global clock on another key");
        assert_eq!(
            store
                .reap_expired_leases(write.connection(), 5, 2, 3, 2, 10)
                .await
                .expect("reap after rollback"),
            1
        );
        let status = store
            .load_status(write.connection(), "key-1", 1)
            .await
            .expect("load reaped state")
            .expect("state");
        assert!(matches!(
            status.state,
            StationKeyCircuitState::Open {
                opened_at_ms: 40,
                cooldown_until_ms: 60,
                reopen_level: 2,
                ..
            }
        ));

        write.commit().await.expect("commit reaper clock state");
        runtime.close().await.expect("close runtime");
    }
}
