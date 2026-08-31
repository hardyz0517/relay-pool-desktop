use serde_json::Value;
use sqlx::{Row, SqliteConnection};

use crate::{
    models::routing_generation::{
        NewRoutingRuntimeGeneration, RoutingCutoverMode, RoutingGenerationAdmissionGuard,
        RoutingGenerationEligibility, RoutingGenerationFence, RoutingGenerationIngestionFence,
        RoutingGenerationMarker, RoutingGenerationQualification, RoutingGenerationRegistrySnapshot,
        RoutingGenerationStatus, RoutingRuntimeGeneration,
        ROUTING_GENERATION_QUALIFICATION_VERSION,
    },
    persistence::error::PersistenceError,
};

const REGISTRY_CORRUPT: &str = "routing_generation_registry_corrupt";
const GENERATION_NOT_QUALIFIED: &str = "routing_generation_not_qualified";

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingGenerationStore;

impl RoutingGenerationStore {
    pub(crate) async fn policy_fingerprint_matches(
        &self,
        connection: &mut SqliteConnection,
        generation: &NewRoutingRuntimeGeneration,
    ) -> Result<bool, PersistenceError> {
        let staged = self
            .load_staged_policy_json(connection, &generation.policy_generation_id)
            .await?;
        let hash = crate::application::routing_generation::canonical_json_sha256(&staged)
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        Ok(generation.policy_input_hash == hash && generation.policy_content_hash == hash)
    }

    pub(crate) async fn count_pending_admitted_attempts(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<u64, PersistenceError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_attempt_v3
             WHERE candidate_admitted = 1 AND terminal_state = 'pending'",
        )
        .fetch_one(&mut *connection)
        .await?;
        u64::try_from(count).map_err(|_| {
            PersistenceError::InvariantViolation(
                "routing attempt pending count is negative".to_string(),
            )
        })
    }

    pub(crate) async fn load_registry_snapshot(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<RoutingGenerationRegistrySnapshot, PersistenceError> {
        let marker_row = sqlx::query(
            "SELECT status, runtime_generation_id, fenced_runtime_generation_id, fence_revision,
                    updated_at_ms
             FROM routing_runtime_cutover_marker WHERE singleton_key = 1",
        )
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| corrupt("cutover marker is missing"))?;
        let marker_mode = RoutingCutoverMode::parse(&marker_row.get::<String, _>("status"))
            .ok_or_else(|| corrupt("cutover marker status is invalid"))?;
        let fence_revision = to_u64(marker_row.get::<i64, _>("fence_revision"), "fence revision")?;
        let marker = RoutingGenerationMarker {
            mode: marker_mode,
            active_runtime_generation_id: marker_row.get("runtime_generation_id"),
            fenced_runtime_generation_id: marker_row.get("fenced_runtime_generation_id"),
            fence_revision,
            updated_at_ms: marker_row.get("updated_at_ms"),
        };

        let active_rows = self
            .load_by_status(connection, RoutingGenerationStatus::Active)
            .await?;
        let fencing_rows = self
            .load_by_status(connection, RoutingGenerationStatus::CutoverFencing)
            .await?;
        if active_rows.len() > 1 || fencing_rows.len() > 1 {
            return Err(corrupt("generation status uniqueness is violated"));
        }
        let active = active_rows.into_iter().next();
        let fencing = fencing_rows.into_iter().next();
        match marker.mode {
            RoutingCutoverMode::PreCutover => {
                if marker.active_runtime_generation_id.is_some() || active.is_some() {
                    return Err(corrupt("pre-cutover marker has an active generation"));
                }
            }
            RoutingCutoverMode::V3Active => {
                let Some(active) = active.as_ref() else {
                    return Err(corrupt("v3 marker has no active generation"));
                };
                if marker.active_runtime_generation_id.as_deref()
                    != Some(active.runtime_generation_id.as_str())
                {
                    return Err(corrupt(
                        "active pointer does not match the active registry row",
                    ));
                }
                self.validate_component_bindings(connection, active).await?;
            }
        }
        if marker.fenced_runtime_generation_id.as_deref()
            != fencing
                .as_ref()
                .map(|generation| generation.runtime_generation_id.as_str())
        {
            return Err(corrupt(
                "fence pointer does not match the fencing registry row",
            ));
        }
        Ok(RoutingGenerationRegistrySnapshot {
            marker,
            active,
            fencing,
        })
    }

    /// Read this inside the same SQLite transaction that admits an attempt or
    /// appends an observation.  A pre-cutover shadow build and an active fence
    /// both route new evidence to the next generation.
    pub(crate) async fn load_ingestion_fence(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<RoutingGenerationIngestionFence, PersistenceError> {
        let snapshot = self.load_registry_snapshot(connection).await?;
        let eligibility =
            if snapshot.marker.mode == RoutingCutoverMode::V3Active && snapshot.fencing.is_none() {
                RoutingGenerationEligibility::Active
            } else {
                RoutingGenerationEligibility::Next
            };
        Ok(RoutingGenerationIngestionFence {
            eligibility,
            active_runtime_generation_id: snapshot.marker.active_runtime_generation_id,
            fence_revision: snapshot.marker.fence_revision,
        })
    }

    pub(crate) async fn load_admission_guard(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<RoutingGenerationAdmissionGuard, PersistenceError> {
        let snapshot = self.load_registry_snapshot(connection).await?;
        Ok(RoutingGenerationAdmissionGuard {
            active_runtime_generation_id: snapshot.marker.active_runtime_generation_id,
            fence_revision: snapshot.marker.fence_revision,
            fencing: snapshot.fencing.is_some(),
        })
    }

    pub(crate) async fn load_runtime_generation(
        &self,
        connection: &mut SqliteConnection,
        runtime_generation_id: &str,
    ) -> Result<Option<RoutingRuntimeGeneration>, PersistenceError> {
        let row = sqlx::query(
            "SELECT runtime_generation_id, policy_generation_id, quality_generation_id,
                    circuit_generation_id, policy_revision, quality_policy_revision,
                    circuit_policy_revision, algorithm_version, status,
                    input_observation_watermark, input_circuit_event_watermark,
                    policy_input_hash, quality_input_hash, circuit_input_hash,
                    policy_content_hash, quality_content_hash, circuit_content_hash,
                    checkpoint_ref, cutover_fence_revision, created_at_ms
             FROM routing_runtime_generation WHERE runtime_generation_id = ?1",
        )
        .bind(runtime_generation_id)
        .fetch_optional(&mut *connection)
        .await?;
        row.map(runtime_generation_from_row).transpose()
    }

    pub(crate) async fn load_staged_policy_json(
        &self,
        connection: &mut SqliteConnection,
        policy_generation_id: &str,
    ) -> Result<Value, PersistenceError> {
        let row = sqlx::query(
            "SELECT config_json FROM routing_policy_v3_staged
             WHERE policy_generation_id = ?1 AND staged_policy_version = 'routing-policy-v3'",
        )
        .bind(policy_generation_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| corrupt("staged policy generation is missing"))?;
        serde_json::from_str(&row.get::<String, _>("config_json"))
            .map_err(|_| corrupt("staged policy generation is invalid"))
    }

    pub(crate) async fn insert_building_runtime_generation(
        &self,
        connection: &mut SqliteConnection,
        generation: &NewRoutingRuntimeGeneration,
    ) -> Result<(), PersistenceError> {
        if let Some(existing) = self
            .load_runtime_generation(connection, &generation.runtime_generation_id)
            .await?
        {
            if runtime_matches_new(&existing, generation) {
                return Ok(());
            }
            return Err(corrupt("runtime generation identity collision"));
        }
        sqlx::query(
            "INSERT INTO routing_runtime_generation (
                 runtime_generation_id, policy_generation_id, quality_generation_id,
                 circuit_generation_id, policy_revision, quality_policy_revision,
                 circuit_policy_revision, algorithm_version, status,
                 input_observation_watermark, input_circuit_event_watermark,
                 policy_input_hash, quality_input_hash, circuit_input_hash,
                 policy_content_hash, quality_content_hash, circuit_content_hash,
                 checkpoint_ref, policy_checkpoint_ref, quality_checkpoint_ref,
                 circuit_checkpoint_ref, created_at_ms, updated_at_ms
             ) VALUES (
                 ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'building',
                 ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                 ?17, ?18, ?19, ?20, ?21, ?21
             )",
        )
        .bind(&generation.runtime_generation_id)
        .bind(&generation.policy_generation_id)
        .bind(&generation.quality_generation_id)
        .bind(&generation.circuit_generation_id)
        .bind(to_i64(generation.policy_revision)?)
        .bind(to_i64(generation.quality_policy_revision)?)
        .bind(to_i64(generation.circuit_policy_revision)?)
        .bind(&generation.algorithm_version)
        .bind(to_i64(generation.input_observation_watermark)?)
        .bind(to_i64(generation.input_circuit_event_watermark)?)
        .bind(&generation.policy_input_hash)
        .bind(&generation.quality_input_hash)
        .bind(&generation.circuit_input_hash)
        .bind(&generation.policy_content_hash)
        .bind(&generation.quality_content_hash)
        .bind(&generation.circuit_content_hash)
        .bind(&generation.checkpoint_ref)
        .bind(&generation.policy_checkpoint_ref)
        .bind(&generation.quality_checkpoint_ref)
        .bind(&generation.circuit_checkpoint_ref)
        .bind(generation.created_at_ms)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    pub(crate) async fn mark_runtime_generation_ready(
        &self,
        connection: &mut SqliteConnection,
        runtime_generation_id: &str,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        let generation = self
            .load_runtime_generation(connection, runtime_generation_id)
            .await?
            .ok_or(PersistenceError::NotFound)?;
        self.validate_component_bindings(connection, &generation)
            .await?;
        let policy_status: String = sqlx::query_scalar(
            "SELECT status FROM routing_policy_v3_staged
             WHERE policy_generation_id = ?1",
        )
        .bind(&generation.policy_generation_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(PersistenceError::NotFound)?;
        // A rollback rebuilds from a retired policy generation. That policy
        // must remain retired while its replacement runtime generation is
        // qualified; activation will atomically move it back to active.
        let policy_affected = if policy_status == "retired" {
            1
        } else {
            sqlx::query(
                "UPDATE routing_policy_v3_staged
                 SET status = 'ready', updated_at_ms = ?2
                WHERE policy_generation_id = ?1 AND status IN ('staged', 'ready')",
            )
            .bind(&generation.policy_generation_id)
            .bind(now_ms)
            .execute(&mut *connection)
            .await?
            .rows_affected()
        };
        if policy_affected != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_policy_v3_staged".into(),
            ));
        }
        let affected = sqlx::query(
            "UPDATE routing_runtime_generation
             SET status = 'ready', ready_at_ms = ?2, updated_at_ms = ?2
             WHERE runtime_generation_id = ?1 AND status = 'building'",
        )
        .bind(runtime_generation_id)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if affected == 0 && generation.status != RoutingGenerationStatus::Ready {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_generation".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn mark_ready_generation_stale(
        &self,
        connection: &mut SqliteConnection,
        runtime_generation_id: &str,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        if runtime_generation_id.is_empty() || now_ms < 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let affected = sqlx::query(
            "UPDATE routing_runtime_generation
             SET status = 'failed', failure_code = 'superseded_by_input_tail',
                 failed_at_ms = ?2, updated_at_ms = ?2
             WHERE runtime_generation_id = ?1 AND status = 'ready'",
        )
        .bind(runtime_generation_id)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_generation".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn begin_fence(
        &self,
        connection: &mut SqliteConnection,
        target_runtime_generation_id: &str,
        expected_active_runtime_generation_id: Option<&str>,
        rollback: bool,
        reason_code: Option<&str>,
        now_ms: i64,
    ) -> Result<RoutingGenerationFence, PersistenceError> {
        validate_reason(reason_code, now_ms)?;
        let snapshot = self.load_registry_snapshot(connection).await?;
        if snapshot.fencing.is_some()
            || snapshot.marker.active_runtime_generation_id.as_deref()
                != expected_active_runtime_generation_id
        {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_generation".into(),
            ));
        }
        let target = self
            .load_runtime_generation(connection, target_runtime_generation_id)
            .await?
            .ok_or(PersistenceError::NotFound)?;
        let expected_status = if rollback {
            RoutingGenerationStatus::Retired
        } else {
            RoutingGenerationStatus::Ready
        };
        if target.status != expected_status
            || rollback && expected_active_runtime_generation_id.is_none()
        {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_generation".into(),
            ));
        }
        self.validate_component_bindings(connection, &target)
            .await?;
        self.require_qualification(connection, target_runtime_generation_id)
            .await?;
        let fence_revision = snapshot
            .marker
            .fence_revision
            .checked_add(1)
            .ok_or_else(|| corrupt("generation fence revision is exhausted"))?;
        let affected = sqlx::query(
            "UPDATE routing_runtime_generation
             SET status = 'cutover_fencing', cutover_fence_revision = ?2,
                 updated_at_ms = ?3
             WHERE runtime_generation_id = ?1 AND status = ?4",
        )
        .bind(target_runtime_generation_id)
        .bind(to_i64(fence_revision)?)
        .bind(now_ms)
        .bind(expected_status.as_str())
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_generation".into(),
            ));
        }
        let marker_affected = sqlx::query(
            "UPDATE routing_runtime_cutover_marker
             SET fenced_runtime_generation_id = ?1, fence_revision = ?2,
                 updated_at_ms = ?3
             WHERE singleton_key = 1 AND fence_revision = ?4
               AND fenced_runtime_generation_id IS NULL",
        )
        .bind(target_runtime_generation_id)
        .bind(to_i64(fence_revision)?)
        .bind(now_ms)
        .bind(to_i64(snapshot.marker.fence_revision)?)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if marker_affected != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_cutover_marker".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO routing_generation_transition_audit (
                 transition_kind, source_runtime_generation_id,
                 target_runtime_generation_id, fence_revision,
                 reason_code, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(if rollback {
            "rollback_started"
        } else {
            "cutover_started"
        })
        .bind(expected_active_runtime_generation_id)
        .bind(target_runtime_generation_id)
        .bind(to_i64(fence_revision)?)
        .bind(reason_code)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        Ok(RoutingGenerationFence {
            source_runtime_generation_id: expected_active_runtime_generation_id.map(str::to_owned),
            target_runtime_generation_id: target_runtime_generation_id.to_string(),
            fence_revision,
        })
    }

    /// Replace a stale fenced candidate without releasing the admission
    /// fence. The original cutover audit row and marker timestamp remain the
    /// durable timeout/start identity for the whole drain operation.
    pub(crate) async fn retarget_fence(
        &self,
        connection: &mut SqliteConnection,
        fence: &RoutingGenerationFence,
        replacement_runtime_generation_id: &str,
        now_ms: i64,
    ) -> Result<RoutingGenerationFence, PersistenceError> {
        if replacement_runtime_generation_id.is_empty()
            || replacement_runtime_generation_id == fence.target_runtime_generation_id
            || now_ms < 0
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        self.assert_fence(connection, fence).await?;
        let replacement = self
            .load_runtime_generation(connection, replacement_runtime_generation_id)
            .await?
            .ok_or(PersistenceError::NotFound)?;
        if replacement.status != RoutingGenerationStatus::Ready {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_generation".into(),
            ));
        }
        self.validate_component_bindings(connection, &replacement)
            .await?;
        self.validate_no_tail_events(connection, &replacement)
            .await?;
        self.require_qualification(connection, replacement_runtime_generation_id)
            .await?;

        let retired = sqlx::query(
            "UPDATE routing_runtime_generation
             SET status = 'failed', failure_code = 'superseded_by_fence_tail',
                 failed_at_ms = ?2, cutover_fence_revision = NULL,
                 updated_at_ms = ?2
             WHERE runtime_generation_id = ?1 AND status = 'cutover_fencing'
               AND cutover_fence_revision = ?3",
        )
        .bind(&fence.target_runtime_generation_id)
        .bind(now_ms)
        .bind(to_i64(fence.fence_revision)?)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if retired != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_generation".into(),
            ));
        }
        let promoted = sqlx::query(
            "UPDATE routing_runtime_generation
             SET status = 'cutover_fencing', cutover_fence_revision = ?2,
                 updated_at_ms = ?3
             WHERE runtime_generation_id = ?1 AND status = 'ready'",
        )
        .bind(replacement_runtime_generation_id)
        .bind(to_i64(fence.fence_revision)?)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if promoted != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_generation".into(),
            ));
        }
        let marker_updated = sqlx::query(
            "UPDATE routing_runtime_cutover_marker
             SET fenced_runtime_generation_id = ?2
             WHERE singleton_key = 1 AND fenced_runtime_generation_id = ?1
               AND fence_revision = ?3",
        )
        .bind(&fence.target_runtime_generation_id)
        .bind(replacement_runtime_generation_id)
        .bind(to_i64(fence.fence_revision)?)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if marker_updated != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_cutover_marker".into(),
            ));
        }
        Ok(RoutingGenerationFence {
            source_runtime_generation_id: fence.source_runtime_generation_id.clone(),
            target_runtime_generation_id: replacement_runtime_generation_id.to_string(),
            fence_revision: fence.fence_revision,
        })
    }

    pub(crate) async fn load_fence_origin_observation_watermark(
        &self,
        connection: &mut SqliteConnection,
        fence: &RoutingGenerationFence,
    ) -> Result<u64, PersistenceError> {
        let value: i64 = sqlx::query_scalar(
            "SELECT g.input_observation_watermark
             FROM routing_generation_transition_audit a
             JOIN routing_runtime_generation g
               ON g.runtime_generation_id = a.target_runtime_generation_id
             WHERE a.transition_kind IN ('cutover_started', 'rollback_started')
               AND a.fence_revision = ?1
               AND a.source_runtime_generation_id IS ?2
             ORDER BY a.transition_id ASC LIMIT 1",
        )
        .bind(to_i64(fence.fence_revision)?)
        .bind(fence.source_runtime_generation_id.as_deref())
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| corrupt("fence origin audit is missing"))?;
        to_u64(value, "fence origin observation watermark")
    }

    /// The durable transition audit is the source of truth for whether a
    /// cutover fence was started as an operator rollback.  The target may be
    /// retargeted while the fence drains, so this lookup is intentionally
    /// keyed by the immutable fence revision and source generation rather
    /// than the current fenced target id.
    pub(crate) async fn is_rollback_fence(
        &self,
        connection: &mut SqliteConnection,
        fence: &RoutingGenerationFence,
    ) -> Result<bool, PersistenceError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM routing_generation_transition_audit
             WHERE transition_kind = 'rollback_started'
               AND fence_revision = ?1
               AND source_runtime_generation_id IS ?2",
        )
        .bind(to_i64(fence.fence_revision)?)
        .bind(fence.source_runtime_generation_id.as_deref())
        .fetch_one(&mut *connection)
        .await?;
        if count > 1 {
            return Err(corrupt("duplicate rollback fence audit"));
        }
        Ok(count == 1)
    }

    pub(crate) async fn abort_fence(
        &self,
        connection: &mut SqliteConnection,
        fence: &RoutingGenerationFence,
        rollback: bool,
        reason_code: &str,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        validate_reason(Some(reason_code), now_ms)?;
        self.assert_fence(connection, fence).await?;
        let restored = if rollback { "retired" } else { "ready" };
        let affected = sqlx::query(
            "UPDATE routing_runtime_generation
             SET status = ?2, cutover_fence_revision = NULL, updated_at_ms = ?3
             WHERE runtime_generation_id = ?1 AND status = 'cutover_fencing'
               AND cutover_fence_revision = ?4",
        )
        .bind(&fence.target_runtime_generation_id)
        .bind(restored)
        .bind(now_ms)
        .bind(to_i64(fence.fence_revision)?)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_generation".into(),
            ));
        }
        self.clear_fence_pointer(connection, fence, now_ms).await?;
        self.insert_transition_audit(
            connection,
            "cutover_aborted",
            fence,
            Some(reason_code),
            now_ms,
        )
        .await
    }

    pub(crate) async fn activate_fenced_generation(
        &self,
        connection: &mut SqliteConnection,
        fence: &RoutingGenerationFence,
        rollback: bool,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        if now_ms < 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        self.assert_fence(connection, fence).await?;
        let target = self
            .load_runtime_generation(connection, &fence.target_runtime_generation_id)
            .await?
            .ok_or(PersistenceError::NotFound)?;
        self.validate_component_bindings(connection, &target)
            .await?;
        self.validate_no_tail_events(connection, &target).await?;

        if let Some(source) = fence.source_runtime_generation_id.as_deref() {
            let affected = sqlx::query(
                "UPDATE routing_runtime_generation
                 SET status = 'retired', retired_at_ms = ?2, updated_at_ms = ?2
                 WHERE runtime_generation_id = ?1 AND status = 'active'",
            )
            .bind(source)
            .bind(now_ms)
            .execute(&mut *connection)
            .await?
            .rows_affected();
            if affected != 1 {
                return Err(PersistenceError::RevisionConflict(
                    "routing_runtime_generation".into(),
                ));
            }
        }

        let activated = sqlx::query(
            "UPDATE routing_runtime_generation
             SET status = 'active', activated_at_ms = ?2, retired_at_ms = NULL,
                 updated_at_ms = ?2
             WHERE runtime_generation_id = ?1 AND status = 'cutover_fencing'
               AND cutover_fence_revision = ?3",
        )
        .bind(&fence.target_runtime_generation_id)
        .bind(now_ms)
        .bind(to_i64(fence.fence_revision)?)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if activated != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_generation".into(),
            ));
        }

        self.switch_component_statuses(connection, fence, &target, now_ms)
            .await?;
        self.replace_live_circuit_state(connection, &target, now_ms)
            .await?;
        let marker_updated = sqlx::query(
            "UPDATE routing_runtime_cutover_marker
             SET status = 'v3_active', runtime_generation_id = ?1,
                 fenced_runtime_generation_id = NULL, updated_at_ms = ?2
             WHERE singleton_key = 1 AND fenced_runtime_generation_id = ?1
               AND fence_revision = ?3",
        )
        .bind(&fence.target_runtime_generation_id)
        .bind(now_ms)
        .bind(to_i64(fence.fence_revision)?)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if marker_updated != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_cutover_marker".into(),
            ));
        }
        self.insert_transition_audit(
            connection,
            if rollback {
                "rollback_activated"
            } else {
                "cutover_activated"
            },
            fence,
            None,
            now_ms,
        )
        .await
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=v3-generation-rollback-query; owner=persistence/stores/routing_generation_store; remove_when=rollback recovery command selects candidates through the coordinator"
        )
    )]
    pub(crate) async fn latest_rollback_candidate(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<Option<RoutingRuntimeGeneration>, PersistenceError> {
        let row = sqlx::query(
            "SELECT runtime_generation_id, policy_generation_id, quality_generation_id,
                    circuit_generation_id, policy_revision, quality_policy_revision,
                    circuit_policy_revision, algorithm_version, status,
                    input_observation_watermark, input_circuit_event_watermark,
                    policy_input_hash, quality_input_hash, circuit_input_hash,
                    policy_content_hash, quality_content_hash, circuit_content_hash,
                    checkpoint_ref, cutover_fence_revision, created_at_ms
             FROM routing_runtime_generation
             WHERE status = 'retired'
             ORDER BY retired_at_ms DESC, runtime_generation_id ASC LIMIT 1",
        )
        .fetch_optional(&mut *connection)
        .await?;
        row.map(runtime_generation_from_row).transpose()
    }

    pub(crate) async fn record_qualification(
        &self,
        connection: &mut SqliteConnection,
        qualification: &RoutingGenerationQualification,
    ) -> Result<(), PersistenceError> {
        if !crate::models::routing_generation::qualification_reports_are_activation_ready(
            &qualification.runtime_generation_id,
            &qualification.comparison_report,
            &qualification.replay_report,
        ) {
            return Err(PersistenceError::InvariantViolation(
                GENERATION_NOT_QUALIFIED.into(),
            ));
        }
        let comparison_hash = crate::application::routing_generation::canonical_json_sha256(
            &qualification.comparison_report,
        )
        .map_err(|_| PersistenceError::ConstraintViolation)?;
        let replay_hash = crate::application::routing_generation::canonical_json_sha256(
            &qualification.replay_report,
        )
        .map_err(|_| PersistenceError::ConstraintViolation)?;
        if comparison_hash != qualification.comparison_report_hash
            || replay_hash != qualification.replay_report_hash
        {
            return Err(PersistenceError::InvariantViolation(
                "qualification report hash does not match canonical content".into(),
            ));
        }
        self.record_qualification_row(connection, qualification)
            .await?;
        let comparison_json = String::from_utf8(
            crate::application::routing_generation::canonical_json_bytes(
                &qualification.comparison_report,
            )
            .map_err(|_| PersistenceError::ConstraintViolation)?,
        )
        .map_err(|_| PersistenceError::ConstraintViolation)?;
        let replay_json = String::from_utf8(
            crate::application::routing_generation::canonical_json_bytes(
                &qualification.replay_report,
            )
            .map_err(|_| PersistenceError::ConstraintViolation)?,
        )
        .map_err(|_| PersistenceError::ConstraintViolation)?;
        sqlx::query(
            "INSERT INTO routing_generation_qualification_report_v2 (
                 runtime_generation_id, comparison_report_json,
                 comparison_report_hash, replay_report_json,
                 replay_report_hash, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(runtime_generation_id) DO NOTHING",
        )
        .bind(&qualification.runtime_generation_id)
        .bind(comparison_json)
        .bind(&qualification.comparison_report_hash)
        .bind(replay_json)
        .bind(&qualification.replay_report_hash)
        .bind(qualification.qualified_at_ms)
        .execute(&mut *connection)
        .await?;
        let report = sqlx::query(
            "SELECT comparison_report_json, comparison_report_hash,
                    replay_report_json, replay_report_hash
             FROM routing_generation_qualification_report_v2
             WHERE runtime_generation_id = ?1",
        )
        .bind(&qualification.runtime_generation_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| corrupt("qualification report disappeared after insert"))?;
        if report.get::<String, _>("comparison_report_hash") != qualification.comparison_report_hash
            || report.get::<String, _>("replay_report_hash") != qualification.replay_report_hash
            || serde_json::from_str::<Value>(
                report.get::<String, _>("comparison_report_json").as_str(),
            )
            .ok()
                != Some(qualification.comparison_report.clone())
            || serde_json::from_str::<Value>(report.get::<String, _>("replay_report_json").as_str())
                .ok()
                != Some(qualification.replay_report.clone())
        {
            return Err(corrupt("qualification report identity collision"));
        }
        Ok(())
    }

    async fn record_qualification_row(
        &self,
        connection: &mut SqliteConnection,
        qualification: &RoutingGenerationQualification,
    ) -> Result<(), PersistenceError> {
        if qualification.runtime_generation_id.is_empty()
            || !is_sha256_hex(&qualification.comparison_report_hash)
            || !is_sha256_hex(&qualification.replay_report_hash)
            || qualification.qualified_at_ms < 0
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        let generation = self
            .load_runtime_generation(connection, &qualification.runtime_generation_id)
            .await?
            .ok_or(PersistenceError::NotFound)?;
        if !matches!(
            generation.status,
            RoutingGenerationStatus::Ready
                | RoutingGenerationStatus::CutoverFencing
                | RoutingGenerationStatus::Active
                | RoutingGenerationStatus::Retired
        ) {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_generation".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO routing_generation_qualification_v2 (
                 runtime_generation_id, qualification_version,
                 comparison_status, comparison_report_hash,
                 replay_status, replay_report_hash, qualified_at_ms
             ) VALUES (?1, ?2, 'passed', ?3, 'passed', ?4, ?5)
             ON CONFLICT(runtime_generation_id) DO NOTHING",
        )
        .bind(&qualification.runtime_generation_id)
        .bind(ROUTING_GENERATION_QUALIFICATION_VERSION)
        .bind(&qualification.comparison_report_hash)
        .bind(&qualification.replay_report_hash)
        .bind(qualification.qualified_at_ms)
        .execute(&mut *connection)
        .await?;
        let persisted = self
            .load_qualification(connection, &qualification.runtime_generation_id)
            .await?
            .ok_or_else(|| corrupt("qualification disappeared after insert"))?;
        if persisted.runtime_generation_id != qualification.runtime_generation_id
            || persisted.comparison_report_hash != qualification.comparison_report_hash
            || persisted.replay_report_hash != qualification.replay_report_hash
            || persisted.qualified_at_ms != qualification.qualified_at_ms
        {
            return Err(corrupt("qualification identity collision"));
        }
        Ok(())
    }

    async fn load_by_status(
        &self,
        connection: &mut SqliteConnection,
        status: RoutingGenerationStatus,
    ) -> Result<Vec<RoutingRuntimeGeneration>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT runtime_generation_id, policy_generation_id, quality_generation_id,
                    circuit_generation_id, policy_revision, quality_policy_revision,
                    circuit_policy_revision, algorithm_version, status,
                    input_observation_watermark, input_circuit_event_watermark,
                    policy_input_hash, quality_input_hash, circuit_input_hash,
                    policy_content_hash, quality_content_hash, circuit_content_hash,
                    checkpoint_ref, cutover_fence_revision, created_at_ms
             FROM routing_runtime_generation WHERE status = ?1
             ORDER BY created_at_ms DESC, runtime_generation_id ASC LIMIT 2",
        )
        .bind(status.as_str())
        .fetch_all(&mut *connection)
        .await?;
        rows.into_iter().map(runtime_generation_from_row).collect()
    }

    async fn load_qualification(
        &self,
        connection: &mut SqliteConnection,
        runtime_generation_id: &str,
    ) -> Result<Option<RoutingGenerationQualification>, PersistenceError> {
        let row = sqlx::query(
            "SELECT qualification_version, comparison_status,
                    comparison_report_hash, replay_status,
                    replay_report_hash, qualified_at_ms
             FROM routing_generation_qualification_v2
             WHERE runtime_generation_id = ?1",
        )
        .bind(runtime_generation_id)
        .fetch_optional(&mut *connection)
        .await?;
        row.map(|row| {
            let version = row.get::<String, _>("qualification_version");
            let comparison_status = row.get::<String, _>("comparison_status");
            let replay_status = row.get::<String, _>("replay_status");
            let comparison_report_hash = required_string(&row, "comparison_report_hash")?;
            let replay_report_hash = required_string(&row, "replay_report_hash")?;
            let qualified_at_ms = row.get::<i64, _>("qualified_at_ms");
            if version != ROUTING_GENERATION_QUALIFICATION_VERSION
                || comparison_status != "passed"
                || replay_status != "passed"
                || !is_sha256_hex(&comparison_report_hash)
                || !is_sha256_hex(&replay_report_hash)
                || qualified_at_ms < 0
            {
                return Err(corrupt("generation qualification is invalid"));
            }
            Ok(RoutingGenerationQualification {
                runtime_generation_id: runtime_generation_id.to_string(),
                comparison_report_hash,
                comparison_report: Value::Null,
                replay_report_hash,
                replay_report: Value::Null,
                qualified_at_ms,
            })
        })
        .transpose()
    }

    async fn require_qualification(
        &self,
        connection: &mut SqliteConnection,
        runtime_generation_id: &str,
    ) -> Result<(), PersistenceError> {
        let evidence = sqlx::query(
            "SELECT q.qualification_version, q.comparison_status, q.replay_status,
                    q.comparison_report_hash, q.replay_report_hash,
                    r.comparison_report_json, r.replay_report_json
             FROM routing_generation_qualification_v2 q
             JOIN routing_generation_qualification_report_v2 r
               ON r.runtime_generation_id = q.runtime_generation_id
              AND r.comparison_report_hash = q.comparison_report_hash
              AND r.replay_report_hash = q.replay_report_hash
             WHERE q.runtime_generation_id = ?1",
        )
        .bind(runtime_generation_id)
        .fetch_optional(&mut *connection)
        .await?;
        let Some(evidence) = evidence else {
            return Err(PersistenceError::InvariantViolation(
                GENERATION_NOT_QUALIFIED.into(),
            ));
        };
        let comparison_report =
            serde_json::from_str::<Value>(&evidence.get::<String, _>("comparison_report_json"))
                .map_err(|_| {
                    PersistenceError::InvariantViolation(GENERATION_NOT_QUALIFIED.into())
                })?;
        let replay_report =
            serde_json::from_str::<Value>(&evidence.get::<String, _>("replay_report_json"))
                .map_err(|_| {
                    PersistenceError::InvariantViolation(GENERATION_NOT_QUALIFIED.into())
                })?;
        let comparison_hash =
            crate::application::routing_generation::canonical_json_sha256(&comparison_report)
                .map_err(|_| {
                    PersistenceError::InvariantViolation(GENERATION_NOT_QUALIFIED.into())
                })?;
        let replay_hash = crate::application::routing_generation::canonical_json_sha256(
            &replay_report,
        )
        .map_err(|_| PersistenceError::InvariantViolation(GENERATION_NOT_QUALIFIED.into()))?;
        if evidence.get::<String, _>("qualification_version")
            != ROUTING_GENERATION_QUALIFICATION_VERSION
            || evidence.get::<String, _>("comparison_status") != "passed"
            || evidence.get::<String, _>("replay_status") != "passed"
            || evidence.get::<String, _>("comparison_report_hash") != comparison_hash
            || evidence.get::<String, _>("replay_report_hash") != replay_hash
            || !crate::models::routing_generation::qualification_reports_are_activation_ready(
                runtime_generation_id,
                &comparison_report,
                &replay_report,
            )
        {
            return Err(PersistenceError::InvariantViolation(
                GENERATION_NOT_QUALIFIED.into(),
            ));
        }
        Ok(())
    }

    async fn validate_no_tail_events(
        &self,
        connection: &mut SqliteConnection,
        generation: &RoutingRuntimeGeneration,
    ) -> Result<(), PersistenceError> {
        let observation_tail: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM routing_observations
                 WHERE generation_eligibility = 'active'
                   AND ingestion_sequence > ?1
             )",
        )
        .bind(to_i64(generation.input_observation_watermark)?)
        .fetch_one(&mut *connection)
        .await?;
        let circuit_tail: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM routing_circuit_event_v3
                 WHERE ingestion_sequence > ?1
             )",
        )
        .bind(to_i64(generation.input_circuit_event_watermark)?)
        .fetch_one(&mut *connection)
        .await?;
        if observation_tail != 0 || circuit_tail != 0 {
            return Err(PersistenceError::RevisionConflict(
                "routing_generation_tail_rebuild_required".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn validate_component_bindings(
        &self,
        connection: &mut SqliteConnection,
        generation: &RoutingRuntimeGeneration,
    ) -> Result<(), PersistenceError> {
        let row = sqlx::query(
            "SELECT
                 p.config_revision AS policy_revision,
                 p.staged_policy_version AS staged_policy_version,
                 q.quality_policy_revision AS quality_policy_revision,
                 q.quality_algorithm_version AS quality_algorithm_version,
                 q.status AS quality_status,
                 q.input_observation_watermark AS quality_watermark,
                 q.input_observation_hash AS quality_input_hash,
                 q.output_content_hash AS quality_content_hash,
                 q.checkpoint_ref AS quality_checkpoint_ref,
                 qc.status AS quality_checkpoint_status,
                 qc.input_observation_watermark AS quality_checkpoint_watermark,
                 c.circuit_policy_revision AS circuit_policy_revision,
                 c.circuit_algorithm_version AS circuit_algorithm_version,
                 c.status AS circuit_status,
                 c.input_circuit_event_watermark AS circuit_watermark,
                 c.input_circuit_event_hash AS circuit_input_hash,
                 c.output_content_hash AS circuit_content_hash,
                 c.checkpoint_ref AS circuit_checkpoint_ref,
                 cc.status AS circuit_checkpoint_status,
                 cc.input_circuit_event_watermark AS circuit_checkpoint_watermark
             FROM routing_policy_v3_staged p
             JOIN routing_quality_generation_v3 q ON q.quality_generation_id = ?2
             JOIN routing_quality_generation_v3_checkpoint qc
               ON qc.quality_generation_id = q.quality_generation_id
             JOIN routing_circuit_generation_v3 c ON c.circuit_generation_id = ?3
             JOIN routing_circuit_generation_v3_checkpoint cc
               ON cc.circuit_generation_id = c.circuit_generation_id
             WHERE p.policy_generation_id = ?1",
        )
        .bind(&generation.policy_generation_id)
        .bind(&generation.quality_generation_id)
        .bind(&generation.circuit_generation_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| corrupt("runtime generation component reference is missing"))?;

        let valid_component_status =
            |status: String| matches!(status.as_str(), "ready" | "active" | "retired");
        let quality_watermark = required_u64(&row, "quality_watermark")?;
        let circuit_watermark = required_u64(&row, "circuit_watermark")?;
        let valid = required_u64(&row, "policy_revision")? == generation.policy_revision
            && row.get::<String, _>("staged_policy_version") == "routing-policy-v3"
            && required_u64(&row, "quality_policy_revision")? == generation.quality_policy_revision
            && required_u64(&row, "circuit_policy_revision")? == generation.circuit_policy_revision
            && !row.get::<String, _>("quality_algorithm_version").is_empty()
            && !row.get::<String, _>("circuit_algorithm_version").is_empty()
            && valid_component_status(row.get("quality_status"))
            && valid_component_status(row.get("circuit_status"))
            && quality_watermark == generation.input_observation_watermark
            && circuit_watermark == generation.input_circuit_event_watermark
            && required_string(&row, "quality_input_hash")? == generation.quality_input_hash
            && required_string(&row, "circuit_input_hash")? == generation.circuit_input_hash
            && required_string(&row, "quality_content_hash")? == generation.quality_content_hash
            && required_string(&row, "circuit_content_hash")? == generation.circuit_content_hash
            && required_string(&row, "quality_checkpoint_ref")?
                == generation_checkpoint_ref(connection, &generation.runtime_generation_id, true)
                    .await?
            && required_string(&row, "circuit_checkpoint_ref")?
                == generation_checkpoint_ref(connection, &generation.runtime_generation_id, false)
                    .await?
            && row.get::<String, _>("quality_checkpoint_status") == "ready"
            && row.get::<String, _>("circuit_checkpoint_status") == "ready"
            && required_u64(&row, "quality_checkpoint_watermark")? == quality_watermark
            && required_u64(&row, "circuit_checkpoint_watermark")? == circuit_watermark;
        if !valid {
            return Err(corrupt(
                "runtime generation component metadata does not match",
            ));
        }
        Ok(())
    }

    async fn assert_fence(
        &self,
        connection: &mut SqliteConnection,
        fence: &RoutingGenerationFence,
    ) -> Result<(), PersistenceError> {
        let snapshot = self.load_registry_snapshot(connection).await?;
        if snapshot.marker.fence_revision != fence.fence_revision
            || snapshot.marker.fenced_runtime_generation_id.as_deref()
                != Some(fence.target_runtime_generation_id.as_str())
            || snapshot.marker.active_runtime_generation_id.as_deref()
                != fence.source_runtime_generation_id.as_deref()
        {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_generation".into(),
            ));
        }
        Ok(())
    }

    async fn clear_fence_pointer(
        &self,
        connection: &mut SqliteConnection,
        fence: &RoutingGenerationFence,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        let affected = sqlx::query(
            "UPDATE routing_runtime_cutover_marker
             SET fenced_runtime_generation_id = NULL, updated_at_ms = ?2
             WHERE singleton_key = 1 AND fenced_runtime_generation_id = ?1
               AND fence_revision = ?3",
        )
        .bind(&fence.target_runtime_generation_id)
        .bind(now_ms)
        .bind(to_i64(fence.fence_revision)?)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_runtime_cutover_marker".into(),
            ));
        }
        Ok(())
    }

    async fn switch_component_statuses(
        &self,
        connection: &mut SqliteConnection,
        fence: &RoutingGenerationFence,
        target: &RoutingRuntimeGeneration,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        if let Some(source_id) = fence.source_runtime_generation_id.as_deref() {
            let source = self
                .load_runtime_generation(connection, source_id)
                .await?
                .ok_or_else(|| corrupt("source generation disappeared during cutover"))?;
            if source.policy_generation_id != target.policy_generation_id {
                let affected = sqlx::query(
                    "UPDATE routing_policy_v3_staged
                     SET status = 'retired', updated_at_ms = ?2
                     WHERE policy_generation_id = ?1 AND status = 'active'",
                )
                .bind(&source.policy_generation_id)
                .bind(now_ms)
                .execute(&mut *connection)
                .await?
                .rows_affected();
                if affected != 1 {
                    return Err(PersistenceError::RevisionConflict(
                        "routing_policy_v3_staged".into(),
                    ));
                }
            }
            if source.quality_generation_id != target.quality_generation_id {
                sqlx::query(
                    "UPDATE routing_quality_generation_v3 SET status = 'retired', updated_at_ms = ?2
                     WHERE quality_generation_id = ?1 AND status = 'active'",
                )
                .bind(&source.quality_generation_id)
                .bind(now_ms)
                .execute(&mut *connection)
                .await?;
            }
            if source.circuit_generation_id != target.circuit_generation_id {
                sqlx::query(
                    "UPDATE routing_circuit_generation_v3 SET status = 'retired', updated_at_ms = ?2
                     WHERE circuit_generation_id = ?1 AND status = 'active'",
                )
                .bind(&source.circuit_generation_id)
                .bind(now_ms)
                .execute(&mut *connection)
                .await?;
            }
        }
        let policy_activated = sqlx::query(
            "UPDATE routing_policy_v3_staged
             SET status = 'active', updated_at_ms = ?2
             WHERE policy_generation_id = ?1
               AND status IN ('ready', 'active', 'retired')",
        )
        .bind(&target.policy_generation_id)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if policy_activated != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_policy_v3_staged".into(),
            ));
        }
        let quality_activated = sqlx::query(
            "UPDATE routing_quality_generation_v3
             SET status = 'active', activated_at_ms = COALESCE(activated_at_ms, ?2),
                 updated_at_ms = ?2
             WHERE quality_generation_id = ?1 AND status IN ('ready', 'active', 'retired')",
        )
        .bind(&target.quality_generation_id)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if quality_activated != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_quality_generation_v3".into(),
            ));
        }
        let circuit_activated = sqlx::query(
            "UPDATE routing_circuit_generation_v3
             SET status = 'active', activated_at_ms = COALESCE(activated_at_ms, ?2),
                 updated_at_ms = ?2
             WHERE circuit_generation_id = ?1 AND status IN ('ready', 'active', 'retired')",
        )
        .bind(&target.circuit_generation_id)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        if circuit_activated != 1 {
            return Err(PersistenceError::RevisionConflict(
                "routing_circuit_generation_v3".into(),
            ));
        }
        Ok(())
    }

    async fn replace_live_circuit_state(
        &self,
        connection: &mut SqliteConnection,
        target: &RoutingRuntimeGeneration,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        // Candidate admission still consumes the single mutable live table.
        // Replace it from the qualified generation in this pointer-swap
        // transaction so readers cannot observe a new policy paired with an
        // old circuit reducer state. Quiescent cutover guarantees no live
        // Half-Open lease is discarded here.
        let active_leases: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_circuit_state_v3
             WHERE state = 'half_open' AND lease_id IS NOT NULL
               AND released_at_ms IS NULL",
        )
        .fetch_one(&mut *connection)
        .await?;
        if active_leases != 0 {
            return Err(PersistenceError::RevisionConflict(
                "routing_circuit_state_v3".into(),
            ));
        }
        sqlx::query("DELETE FROM routing_circuit_state_v3")
            .execute(&mut *connection)
            .await?;
        let inserted = sqlx::query(
            "INSERT INTO routing_circuit_state_v3 (
                 station_key_id, station_key_lifecycle_revision, state,
                 state_revision, consecutive_failures, reopen_level,
                 opened_at_ms, cooldown_until_ms, recovery_successes,
                 lease_id, lease_revision, lease_attempt_id,
                 lease_expires_at_ms, lease_deadline_at_ms, boundary_crossed,
                 released_at_ms, lease_terminal_state,
                 monotonic_clock_watermark_ms, updated_at_ms
             )
             SELECT station_key_id, station_key_lifecycle_revision, state,
                    state_revision, consecutive_failures, reopen_level,
                    opened_at_ms, cooldown_until_ms, recovery_successes,
                    NULL,
                    CASE WHEN state = 'half_open' THEN state_revision ELSE NULL END,
                    NULL, NULL, NULL, NULL, NULL, NULL,
                    monotonic_clock_watermark_ms, ?2
             FROM routing_circuit_state_generation_v3
             WHERE circuit_generation_id = ?1",
        )
        .bind(&target.circuit_generation_id)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?
        .rows_affected();
        let expected: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_circuit_state_generation_v3
             WHERE circuit_generation_id = ?1",
        )
        .bind(&target.circuit_generation_id)
        .fetch_one(&mut *connection)
        .await?;
        if inserted != u64::try_from(expected).map_err(|_| PersistenceError::ConstraintViolation)? {
            return Err(PersistenceError::InvariantViolation(
                "live circuit state did not match activated generation".into(),
            ));
        }
        Ok(())
    }

    async fn insert_transition_audit(
        &self,
        connection: &mut SqliteConnection,
        transition_kind: &str,
        fence: &RoutingGenerationFence,
        reason_code: Option<&str>,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            "INSERT INTO routing_generation_transition_audit (
                 transition_kind, source_runtime_generation_id,
                 target_runtime_generation_id, fence_revision,
                 reason_code, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(transition_kind)
        .bind(fence.source_runtime_generation_id.as_deref())
        .bind(&fence.target_runtime_generation_id)
        .bind(to_i64(fence.fence_revision)?)
        .bind(reason_code)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }
}

fn runtime_generation_from_row(
    row: sqlx::sqlite::SqliteRow,
) -> Result<RoutingRuntimeGeneration, PersistenceError> {
    let status = RoutingGenerationStatus::parse(&row.get::<String, _>("status"))
        .ok_or_else(|| corrupt("runtime generation status is invalid"))?;
    Ok(RoutingRuntimeGeneration {
        runtime_generation_id: row.get("runtime_generation_id"),
        policy_generation_id: row.get("policy_generation_id"),
        quality_generation_id: row.get("quality_generation_id"),
        circuit_generation_id: row.get("circuit_generation_id"),
        policy_revision: required_u64(&row, "policy_revision")?,
        quality_policy_revision: required_u64(&row, "quality_policy_revision")?,
        circuit_policy_revision: required_u64(&row, "circuit_policy_revision")?,
        algorithm_version: row.get("algorithm_version"),
        status,
        input_observation_watermark: required_u64(&row, "input_observation_watermark")?,
        input_circuit_event_watermark: required_u64(&row, "input_circuit_event_watermark")?,
        policy_input_hash: required_string(&row, "policy_input_hash")?,
        quality_input_hash: required_string(&row, "quality_input_hash")?,
        circuit_input_hash: required_string(&row, "circuit_input_hash")?,
        policy_content_hash: required_string(&row, "policy_content_hash")?,
        quality_content_hash: required_string(&row, "quality_content_hash")?,
        circuit_content_hash: required_string(&row, "circuit_content_hash")?,
        checkpoint_ref: required_string(&row, "checkpoint_ref")?,
        cutover_fence_revision: row
            .get::<Option<i64>, _>("cutover_fence_revision")
            .map(|value| to_u64(value, "cutover fence revision"))
            .transpose()?,
        created_at_ms: row.get("created_at_ms"),
    })
}

fn runtime_matches_new(
    existing: &RoutingRuntimeGeneration,
    generation: &NewRoutingRuntimeGeneration,
) -> bool {
    existing.runtime_generation_id == generation.runtime_generation_id
        && existing.policy_generation_id == generation.policy_generation_id
        && existing.quality_generation_id == generation.quality_generation_id
        && existing.circuit_generation_id == generation.circuit_generation_id
        && existing.policy_revision == generation.policy_revision
        && existing.quality_policy_revision == generation.quality_policy_revision
        && existing.circuit_policy_revision == generation.circuit_policy_revision
        && existing.algorithm_version == generation.algorithm_version
        && existing.input_observation_watermark == generation.input_observation_watermark
        && existing.input_circuit_event_watermark == generation.input_circuit_event_watermark
        && existing.policy_input_hash == generation.policy_input_hash
        && existing.quality_input_hash == generation.quality_input_hash
        && existing.circuit_input_hash == generation.circuit_input_hash
        && existing.policy_content_hash == generation.policy_content_hash
        && existing.quality_content_hash == generation.quality_content_hash
        && existing.circuit_content_hash == generation.circuit_content_hash
        && existing.checkpoint_ref == generation.checkpoint_ref
}

async fn generation_checkpoint_ref(
    connection: &mut SqliteConnection,
    runtime_generation_id: &str,
    quality: bool,
) -> Result<String, PersistenceError> {
    let column = if quality {
        "quality_checkpoint_ref"
    } else {
        "circuit_checkpoint_ref"
    };
    let sql = format!(
        "SELECT {column} AS checkpoint_ref FROM routing_runtime_generation
         WHERE runtime_generation_id = ?1"
    );
    let row = sqlx::query(&sql)
        .bind(runtime_generation_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| corrupt("runtime generation disappeared during validation"))?;
    required_string(&row, "checkpoint_ref")
}

fn required_u64(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<u64, PersistenceError> {
    let value = row
        .get::<Option<i64>, _>(column)
        .ok_or_else(|| corrupt("required generation integer is missing"))?;
    to_u64(value, column)
}

fn required_string(
    row: &sqlx::sqlite::SqliteRow,
    column: &str,
) -> Result<String, PersistenceError> {
    row.get::<Option<String>, _>(column)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| corrupt("required generation string is missing"))
}

fn to_u64(value: i64, field: &str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| corrupt(&format!("{field} is negative")))
}

fn to_i64(value: u64) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| PersistenceError::ConstraintViolation)
}

fn validate_reason(reason: Option<&str>, now_ms: i64) -> Result<(), PersistenceError> {
    if now_ms < 0
        || reason.is_some_and(|value| {
            value.is_empty() || value.len() > 96 || value.chars().any(char::is_control)
        })
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn corrupt(detail: &str) -> PersistenceError {
    PersistenceError::InvariantViolation(format!("{REGISTRY_CORRUPT}: {detail}"))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}
