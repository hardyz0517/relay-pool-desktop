use std::collections::HashSet;

use serde_json::Value;
use sqlx::Row;

use crate::{
    models::{
        collector::CollectorSnapshot,
        collector_runs::CollectorRun,
        group_facts::{GroupRateRecord, StationGroupBinding},
    },
    persistence::{
        error::PersistenceError, read_session::ReadSession, write_session::WriteSession,
    },
};

#[derive(Debug, Clone)]
pub(crate) struct CollectorRunStart {
    pub id: String,
    pub run_key: String,
    pub request_hash: String,
    pub station_id: String,
    pub endpoint_revision: i64,
    /// Historical parent linkage. New control flow must use operation_id /
    /// intent_sequence; this field is populated only for compatibility reads.
    pub parent_run_id: Option<String>,
    pub adapter: String,
    pub task_type: String,
    pub started_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CollectorSnapshotWrite {
    pub id: String,
    pub run_id: String,
    pub station_id: String,
    pub endpoint_revision: i64,
    pub source: String,
    pub status: String,
    pub fetched_at: String,
    pub summary_json: Value,
    pub normalized_json: Value,
    pub raw_json_redacted: Option<Value>,
    pub error_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CollectorRunFinish {
    pub id: String,
    pub status: String,
    pub finished_at: String,
    pub duration_ms: i64,
    pub endpoint_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub manual_action_required: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub snapshot_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct StationCollectionProjectionWrite {
    pub station_id: String,
    pub status: String,
    pub reason_codes_json: String,
    pub revision: i64,
    pub endpoint_revision: i64,
    pub credential_revision: i64,
    pub intent_sequence: i64,
    pub operation_id: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredCollectorApply {
    pub run_id: String,
    pub snapshot_id: String,
    pub inserted: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ExistingCollectorApply {
    pub request_hash: String,
    pub outcome: StoredCollectorApply,
}

#[derive(Debug, Clone)]
pub(crate) struct BalanceWrite {
    pub id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub scope: String,
    pub value: Option<f64>,
    pub used_value: Option<f64>,
    pub total_value: Option<f64>,
    pub today_request_count: Option<i64>,
    pub total_request_count: Option<i64>,
    pub today_consumption: Option<f64>,
    pub total_consumption: Option<f64>,
    pub today_base_consumption: Option<f64>,
    pub total_base_consumption: Option<f64>,
    pub today_token_count: Option<i64>,
    pub total_token_count: Option<i64>,
    pub today_input_token_count: Option<i64>,
    pub today_output_token_count: Option<i64>,
    pub total_input_token_count: Option<i64>,
    pub total_output_token_count: Option<i64>,
    pub account_concurrency_limit: Option<i64>,
    pub currency: String,
    pub credit_unit: Option<String>,
    pub status: String,
    pub source: String,
    pub confidence: f64,
    pub collected_at: Option<String>,
    pub evidence_confidence: String,
    pub spendability_authority: String,
    pub observed_at_ms: Option<i64>,
    pub valid_until_ms: Option<i64>,
    pub evidence_profile_version: String,
    pub spendability_reason_code: Option<String>,
    pub now: String,
}

#[derive(Debug, Clone)]
pub(crate) struct GroupWrite {
    pub id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub binding_kind: String,
    pub group_key_hash: String,
    pub group_id_hash: Option<String>,
    pub group_name: String,
    pub binding_status: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<String>,
    pub source: String,
    pub confidence: f64,
    pub last_seen_at: Option<String>,
    pub raw_json_redacted: Option<Value>,
    pub run_id: String,
    pub now: String,
}

#[derive(Debug, Clone)]
pub(crate) struct StationGroupBindingWrite {
    pub id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub binding_kind: String,
    pub parent_group_binding_id: Option<String>,
    pub group_key_hash: String,
    pub group_id_hash: Option<String>,
    pub group_name: String,
    pub binding_status: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<String>,
    pub group_category_override: Option<String>,
    pub rate_source: Option<String>,
    pub confidence: f64,
    pub last_seen_at: Option<String>,
    pub raw_json_redacted: Option<Value>,
    pub now: String,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredStationGroupBindingUpsert {
    pub binding: StationGroupBinding,
    pub transition: GroupTransition,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GroupState {
    pub id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub binding_kind: String,
    pub group_key_hash: String,
    pub group_name: String,
    pub binding_status: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub source: String,
}

#[derive(Debug, Clone)]
pub(crate) struct GroupTransition {
    pub previous: Option<GroupState>,
    pub current: GroupState,
}

#[derive(Debug, Clone)]
pub(crate) struct RateWrite {
    pub id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub group_binding_id: String,
    pub binding_kind: String,
    pub group_key_hash: String,
    pub group_name: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<String>,
    pub source: String,
    pub confidence: f64,
    pub raw_json_redacted: Option<Value>,
    pub checked_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RateTransition {
    pub group_binding_id: String,
    pub group_name: String,
    pub old_effective_rate_multiplier: Option<f64>,
    pub new_effective_rate_multiplier: Option<f64>,
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct CollectorTaskStateWrite {
    pub station_id: String,
    pub task_type: String,
    pub run_id: String,
    pub status: String,
    pub finished_at: String,
    pub next_due_at: Option<String>,
}

const COLLECTOR_OPERATION_PLAN_VERSION: &str = "collector-plan-v1";
const COLLECTOR_OPERATION_REASON_MAX_BYTES: usize = 128;

pub(crate) fn collector_operation_key(
    station_id: &str,
    endpoint_revision: i64,
    credential_revision: i64,
    intent_sequence: i64,
) -> String {
    format!(
        "collector-intent:{station_id}:{endpoint_revision}:{credential_revision}:{intent_sequence}"
    )
}

pub(crate) fn capture_operation_key(
    station_id: &str,
    endpoint_revision: i64,
    credential_revision: i64,
    intent_sequence: i64,
) -> String {
    format!(
        "capture-intent:{station_id}:{endpoint_revision}:{credential_revision}:{intent_sequence}"
    )
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct CollectorStore;

impl CollectorStore {
    pub(crate) async fn start_capture_operation(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        endpoint_revision: i64,
        credential_revision: i64,
        now_ms: i64,
    ) -> Result<(String, i64), PersistenceError> {
        if station_id.trim().is_empty() || now_ms < 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        self.assert_endpoint_revision(session, station_id, endpoint_revision)
            .await?;
        self.assert_station_credential_revision(session, station_id, credential_revision)
            .await?;
        let scope = format!("station_capture_intent:{station_id}");
        sqlx::query(
            "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
             VALUES (?1, 1, ?2, 'transactional_write')
             ON CONFLICT(scope) DO UPDATE SET
               revision = domain_revisions.revision + 1,
               updated_at_ms = excluded.updated_at_ms,
               provenance = 'transactional_write'",
        )
        .bind(&scope)
        .bind(now_ms)
        .execute(session.connection())
        .await?;
        let intent_sequence =
            sqlx::query_scalar::<_, i64>("SELECT revision FROM domain_revisions WHERE scope = ?1")
                .bind(scope)
                .fetch_one(session.connection())
                .await?;
        let operation_id = capture_operation_key(
            station_id,
            endpoint_revision,
            credential_revision,
            intent_sequence,
        );
        sqlx::query(
            "INSERT INTO collector_operations (
                operation_id, operation_key, station_id, endpoint_revision,
                credential_revision, intent_sequence, plan_version, task_type,
                trigger_kind, status, started_at_ms, finished_at_ms,
                reason_code, reason_detail, created_at_ms, updated_at_ms
             ) VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, 'capture',
                       'webview', 'running', ?7, NULL, NULL, NULL, ?7, ?7)",
        )
        .bind(&operation_id)
        .bind(station_id)
        .bind(endpoint_revision)
        .bind(credential_revision)
        .bind(intent_sequence)
        .bind(COLLECTOR_OPERATION_PLAN_VERSION)
        .bind(now_ms)
        .execute(session.connection())
        .await?;
        Ok((operation_id, intent_sequence))
    }

    pub(crate) async fn finish_capture_operation(
        &self,
        session: &mut WriteSession,
        operation_id: &str,
        station_id: &str,
        endpoint_revision: i64,
        credential_revision: i64,
        intent_sequence: i64,
        terminal_status: &str,
        reason_code: Option<&str>,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        if operation_id
            != capture_operation_key(
                station_id,
                endpoint_revision,
                credential_revision,
                intent_sequence,
            )
            || !matches!(terminal_status, "succeeded" | "cancelled" | "interrupted")
            || now_ms < 0
            || reason_code.is_some_and(|value| value.len() > COLLECTOR_OPERATION_REASON_MAX_BYTES)
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        let updated = sqlx::query(
            "UPDATE collector_operations
             SET status = ?1, finished_at_ms = ?2, reason_code = ?3,
                 reason_detail = NULL, updated_at_ms = ?2
             WHERE operation_id = ?4 AND station_id = ?5
               AND endpoint_revision = ?6 AND credential_revision = ?7
               AND intent_sequence = ?8 AND task_type = 'capture'
               AND status IN ('queued', 'running')",
        )
        .bind(terminal_status)
        .bind(now_ms)
        .bind(reason_code.map(str::trim).filter(|value| !value.is_empty()))
        .bind(operation_id)
        .bind(station_id)
        .bind(endpoint_revision)
        .bind(credential_revision)
        .bind(intent_sequence)
        .execute(session.connection())
        .await?
        .rows_affected();
        if updated == 0 {
            let status = sqlx::query_scalar::<_, String>(
                "SELECT status FROM collector_operations WHERE operation_id = ?1",
            )
            .bind(operation_id)
            .fetch_optional(session.connection())
            .await?;
            if status.as_deref() == Some(terminal_status) {
                return Ok(());
            }
        }
        if updated != 1 {
            return Err(PersistenceError::InvariantViolation(
                "capture operation cannot enter terminal state".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn interrupt_active_capture_operations(
        &self,
        session: &mut WriteSession,
        now_ms: i64,
    ) -> Result<u64, PersistenceError> {
        if now_ms < 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        sqlx::query(
            "UPDATE collector_operations
             SET status = 'interrupted', finished_at_ms = ?1,
                 reason_code = 'process_shutdown', reason_detail = NULL,
                 updated_at_ms = ?1
             WHERE task_type = 'capture' AND status IN ('queued', 'running')",
        )
        .bind(now_ms)
        .execute(session.connection())
        .await
        .map(|result| result.rows_affected())
        .map_err(Into::into)
    }

    /// Reconcile collector work that was left active when the process exited.
    ///
    /// Collection intents are durable reservations, so a queued/running row
    /// must never be left blocking the scheduler indefinitely.  Rows whose
    /// endpoint, credential, or intent fence is no longer current are terminal
    /// `superseded`; rows still carrying the current fence are recoverable
    /// `interrupted` work and may be re-admitted by the scheduler.
    pub(crate) async fn recover_active_collector_operations(
        &self,
        session: &mut WriteSession,
        now_ms: i64,
    ) -> Result<u64, PersistenceError> {
        if now_ms < 0 {
            return Err(PersistenceError::ConstraintViolation);
        }

        let operations = sqlx::query(
            "SELECT operation_id, station_id, endpoint_revision,
                    credential_revision, intent_sequence
             FROM collector_operations
             WHERE status IN ('queued', 'running')
               AND (
                    (task_type = 'unspecified' AND trigger_kind = 'unspecified')
                    OR (
                        task_type IN ('detect', 'balance', 'groups', 'published_status', 'full')
                        AND trigger_kind = 'collector'
                    )
               )",
        )
        .fetch_all(session.connection())
        .await?;

        let mut recovered = 0_u64;
        for operation in operations {
            let operation_id = operation.get::<String, _>("operation_id");
            let station_id = operation.get::<String, _>("station_id");
            let endpoint_revision = operation.get::<i64, _>("endpoint_revision");
            let credential_revision = operation.get::<i64, _>("credential_revision");
            let intent_sequence = operation.get::<i64, _>("intent_sequence");

            let current = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*)
                 FROM stations AS station
                 JOIN domain_revisions AS credential
                   ON credential.scope = 'station_account:' || station.id
                 JOIN domain_revisions AS intent
                   ON intent.scope = 'station_collection_intent:' || station.id
                 WHERE station.id = ?1
                   AND station.endpoint_revision = ?2
                   AND credential.revision = ?3
                   AND intent.revision = ?4",
            )
            .bind(&station_id)
            .bind(endpoint_revision)
            .bind(credential_revision)
            .bind(intent_sequence)
            .fetch_one(session.connection())
            .await?
                == 1;
            let (status, reason_code) = if current {
                ("interrupted", "process_shutdown")
            } else {
                ("superseded", "stale_revision")
            };

            let updated = sqlx::query(
                "UPDATE collector_operations
                 SET status = ?1, finished_at_ms = ?2, reason_code = ?3,
                     reason_detail = NULL, updated_at_ms = ?2
                 WHERE operation_id = ?4
                   AND status IN ('queued', 'running')
                   AND (
                        (task_type = 'unspecified' AND trigger_kind = 'unspecified')
                        OR (
                            task_type IN ('detect', 'balance', 'groups', 'published_status', 'full')
                            AND trigger_kind = 'collector'
                        )
                   )",
            )
            .bind(status)
            .bind(now_ms)
            .bind(reason_code)
            .bind(&operation_id)
            .execute(session.connection())
            .await?
            .rows_affected();
            recovered += updated;
        }

        // A process can exit after the collector run is inserted but before
        // the operation ledger is terminalized.  Close those historical rows
        // as interrupted so the due query sees a bounded completion time.
        sqlx::query(
            "UPDATE collector_runs
             SET status = 'interrupted', finished_at = ?1,
                 error_code = COALESCE(error_code, 'process_shutdown'),
                 error_message = COALESCE(error_message, 'collector interrupted by process shutdown')
             WHERE task_type IN ('detect', 'balance', 'groups', 'published_status', 'full')
               AND status = 'running'",
        )
        .bind(now_ms.to_string())
        .execute(session.connection())
        .await?;

        Ok(recovered)
    }

    pub(crate) async fn list_station_snapshots(
        &self,
        read: &mut ReadSession,
        station_id: &str,
        limit: i64,
    ) -> Result<Vec<CollectorSnapshot>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT id, station_id, endpoint_revision, source, status, fetched_at,
                   summary_json, normalized_json, raw_json_redacted, error_message, created_at
            FROM collector_snapshots
            WHERE station_id = ?1
            ORDER BY created_at DESC, id DESC
            LIMIT ?2
            "#,
        )
        .bind(station_id)
        .bind(limit)
        .fetch_all(read.connection())
        .await?;
        rows.into_iter().map(row_to_collector_snapshot).collect()
    }

    pub(crate) async fn latest_station_snapshot(
        &self,
        read: &mut ReadSession,
        station_id: &str,
    ) -> Result<Option<CollectorSnapshot>, PersistenceError> {
        let row = sqlx::query(
            r#"
            SELECT snapshots.id, snapshots.station_id, snapshots.endpoint_revision,
                   snapshots.source, snapshots.status, snapshots.fetched_at,
                   snapshots.summary_json, snapshots.normalized_json,
                   snapshots.raw_json_redacted, snapshots.error_message, snapshots.created_at
            FROM collector_snapshots AS snapshots
            WHERE snapshots.station_id = ?1
            -- Snapshots are historical evidence. Current collection and
            -- authorization state come from typed projections; do not let an
            -- old manual-required row outrank a newer terminal result.
            ORDER BY snapshots.created_at DESC, snapshots.id DESC
            LIMIT 1
            "#,
        )
        .bind(station_id)
        .fetch_optional(read.connection())
        .await?;
        row.map(row_to_collector_snapshot).transpose()
    }

    pub(crate) async fn snapshot_by_id(
        &self,
        read: &mut ReadSession,
        snapshot_id: &str,
    ) -> Result<CollectorSnapshot, PersistenceError> {
        let row = sqlx::query(
            r#"
            SELECT id, station_id, endpoint_revision, source, status, fetched_at,
                   summary_json, normalized_json, raw_json_redacted, error_message, created_at
            FROM collector_snapshots
            WHERE id = ?1
            "#,
        )
        .bind(snapshot_id)
        .fetch_optional(read.connection())
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
        row_to_collector_snapshot(row)
    }

    pub(crate) async fn list_station_group_bindings(
        &self,
        read: &mut ReadSession,
        station_id: &str,
        limit: u32,
    ) -> Result<Vec<StationGroupBinding>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT id, station_id, station_key_id, binding_kind, parent_group_binding_id,
                   group_key_hash, group_id_hash, group_name, binding_status,
                   default_rate_multiplier, user_rate_multiplier, effective_rate_multiplier,
                   inferred_group_category, group_category_override, rate_source, confidence,
                   last_seen_at, last_checked_at, last_rate_changed_at, raw_json_redacted,
                   created_at, updated_at
            FROM station_group_bindings
            WHERE station_id = ?1
            ORDER BY binding_kind ASC, binding_status ASC, group_name COLLATE NOCASE ASC, id ASC
            LIMIT ?2
            "#,
        )
        .bind(station_id)
        .bind(limit)
        .fetch_all(read.connection())
        .await?;
        rows.into_iter().map(row_to_station_group_binding).collect()
    }

    pub(crate) async fn list_selectable_station_group_bindings(
        &self,
        read: &mut ReadSession,
        station_id: &str,
        limit: u32,
    ) -> Result<Vec<StationGroupBinding>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT id, station_id, station_key_id, binding_kind, parent_group_binding_id,
                   group_key_hash, group_id_hash, group_name, binding_status,
                   default_rate_multiplier, user_rate_multiplier, effective_rate_multiplier,
                   inferred_group_category, group_category_override, rate_source, confidence,
                   last_seen_at, last_checked_at, last_rate_changed_at, raw_json_redacted,
                   created_at, updated_at
            FROM station_group_bindings
            WHERE station_id = ?1
              AND binding_kind = 'station_group'
              AND binding_status NOT IN ('disabled', 'manual_legacy')
              AND COALESCE(rate_source, '') != 'legacy_key_group'
            ORDER BY group_name COLLATE NOCASE ASC, id ASC
            LIMIT ?2
            "#,
        )
        .bind(station_id)
        .bind(limit)
        .fetch_all(read.connection())
        .await?;
        rows.into_iter().map(row_to_station_group_binding).collect()
    }

    pub(crate) async fn list_group_rate_records(
        &self,
        read: &mut ReadSession,
        station_id: &str,
        limit: u32,
    ) -> Result<Vec<GroupRateRecord>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT id, station_id, station_key_id, group_binding_id, binding_kind,
                   group_key_hash, group_name, default_rate_multiplier, user_rate_multiplier,
                   effective_rate_multiplier, inferred_group_category, source, confidence,
                   raw_json_redacted, checked_at, created_at
            FROM group_rate_records
            WHERE station_id = ?1
            ORDER BY checked_at DESC, created_at DESC, id DESC
            LIMIT ?2
            "#,
        )
        .bind(station_id)
        .bind(limit)
        .fetch_all(read.connection())
        .await?;
        rows.into_iter().map(row_to_group_rate_record).collect()
    }

    pub(crate) async fn list_latest_station_group_rates(
        &self,
        read: &mut ReadSession,
        station_id: &str,
        limit: u32,
    ) -> Result<Vec<GroupRateRecord>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            WITH ranked AS (
                SELECT r.*,
                       ROW_NUMBER() OVER (
                           PARTITION BY CASE
                               WHEN r.group_binding_id IS NULL
                               THEN 'group:' || r.group_key_hash
                               ELSE 'binding:' || r.group_binding_id
                           END
                           ORDER BY r.checked_at DESC, r.created_at DESC, r.id DESC
                       ) AS row_number
                FROM group_rate_records r
                WHERE r.station_id = ?1 AND r.binding_kind = 'station_group'
            )
            SELECT id, station_id, station_key_id, group_binding_id, binding_kind,
                   group_key_hash, group_name, default_rate_multiplier, user_rate_multiplier,
                   effective_rate_multiplier, inferred_group_category, source, confidence,
                   raw_json_redacted, checked_at, created_at
            FROM ranked
            WHERE row_number = 1
            ORDER BY group_name COLLATE NOCASE ASC, checked_at DESC, id DESC
            LIMIT ?2
            "#,
        )
        .bind(station_id)
        .bind(limit)
        .fetch_all(read.connection())
        .await?;
        rows.into_iter().map(row_to_group_rate_record).collect()
    }

    pub(crate) async fn list_collector_runs(
        &self,
        read: &mut ReadSession,
        station_id: &str,
        limit: u32,
    ) -> Result<Vec<CollectorRun>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT id, station_id, endpoint_revision, parent_run_id, adapter, task_type,
                   status, started_at, finished_at, duration_ms, endpoint_count,
                   success_count, failure_count, manual_action_required, error_code,
                   error_message, snapshot_id, created_at
            FROM collector_runs
            WHERE station_id = ?1
            ORDER BY created_at DESC, id DESC
            LIMIT ?2
            "#,
        )
        .bind(station_id)
        .bind(limit)
        .fetch_all(read.connection())
        .await?;
        rows.into_iter().map(row_to_collector_run).collect()
    }

    pub(crate) async fn assert_endpoint_revision(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        endpoint_revision: i64,
    ) -> Result<(), PersistenceError> {
        let revision =
            sqlx::query_scalar::<_, i64>("SELECT endpoint_revision FROM stations WHERE id = ?1")
                .bind(station_id)
                .fetch_optional(session.connection())
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
        if revision != endpoint_revision {
            return Err(PersistenceError::StaleRevision);
        }
        Ok(())
    }

    pub(crate) async fn assert_station_credential_revision(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        credential_revision: i64,
    ) -> Result<(), PersistenceError> {
        if credential_revision < 1 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let scope = format!("station_account:{station_id}");
        let revision =
            sqlx::query_scalar::<_, i64>("SELECT revision FROM domain_revisions WHERE scope = ?1")
                .bind(&scope)
                .fetch_optional(session.connection())
                .await?
                .ok_or_else(|| PersistenceError::RevisionUnavailable(scope.clone()))?;
        if revision != credential_revision {
            return Err(PersistenceError::StaleRevision);
        }
        Ok(())
    }

    /// Advance the durable station-collection watermark in the same terminal
    /// write transaction as the run and projection. The returned revision is
    /// safe to publish after commit as a freshness hint.
    pub(crate) async fn advance_station_collection_revision(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        updated_at_ms: i64,
    ) -> Result<i64, PersistenceError> {
        if station_id.trim().is_empty() || updated_at_ms < 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let scope = format!("station_collection:{station_id}");
        sqlx::query(
            "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
             VALUES (?1, 1, ?2, 'transactional_write')
             ON CONFLICT(scope) DO UPDATE SET
               revision = domain_revisions.revision + 1,
               updated_at_ms = excluded.updated_at_ms,
               provenance = 'transactional_write'",
        )
        .bind(&scope)
        .bind(updated_at_ms)
        .execute(session.connection())
        .await?;
        sqlx::query_scalar::<_, i64>("SELECT revision FROM domain_revisions WHERE scope = ?1")
            .bind(scope)
            .fetch_one(session.connection())
            .await
            .map_err(Into::into)
    }

    /// Persist a stale completion as terminal history before the application
    /// returns a stale-revision error. No current projection or fact write is
    /// allowed after this method reports `false`.
    pub(crate) async fn fence_collector_operation_for_commit(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        endpoint_revision: i64,
        credential_revision: i64,
        intent_sequence: i64,
        updated_at_ms: i64,
    ) -> Result<bool, PersistenceError> {
        if station_id.trim().is_empty()
            || endpoint_revision < 1
            || credential_revision < 1
            || intent_sequence < 1
            || updated_at_ms < 0
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        let current = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*)
             FROM stations AS station
             JOIN domain_revisions AS credential
               ON credential.scope = 'station_account:' || station.id
             JOIN domain_revisions AS intent
               ON intent.scope = 'station_collection_intent:' || station.id
             WHERE station.id = ?1
               AND station.endpoint_revision = ?2
               AND credential.revision = ?3
               AND intent.revision = ?4",
        )
        .bind(station_id)
        .bind(endpoint_revision)
        .bind(credential_revision)
        .bind(intent_sequence)
        .fetch_one(session.connection())
        .await?
            == 1;
        if current {
            return Ok(true);
        }
        let operation_key = collector_operation_key(
            station_id,
            endpoint_revision,
            credential_revision,
            intent_sequence,
        );
        let updated = sqlx::query(
            "UPDATE collector_operations
             SET status = 'superseded', finished_at_ms = ?1,
                 reason_code = 'stale_revision', reason_detail = NULL,
                 updated_at_ms = ?1
             WHERE operation_key = ?2
               AND status IN ('queued', 'running')",
        )
        .bind(updated_at_ms)
        .bind(&operation_key)
        .execute(session.connection())
        .await?
        .rows_affected();
        if updated == 0 {
            let existing_status = sqlx::query_scalar::<_, String>(
                "SELECT status FROM collector_operations
                 WHERE operation_key = ?1",
            )
            .bind(&operation_key)
            .fetch_optional(session.connection())
            .await?;
            if existing_status.as_deref() == Some("superseded") {
                return Ok(false);
            }
        }
        if updated != 1 {
            return Err(PersistenceError::InvariantViolation(
                "stale collector completion has no active operation ledger row".into(),
            ));
        }
        Ok(false)
    }

    pub(crate) async fn mark_collector_operation_running(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        endpoint_revision: i64,
        credential_revision: i64,
        intent_sequence: i64,
        task_type: &str,
        started_at_ms: i64,
    ) -> Result<(), PersistenceError> {
        if task_type.trim().is_empty() || task_type.len() > 64 || started_at_ms < 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let operation_key = collector_operation_key(
            station_id,
            endpoint_revision,
            credential_revision,
            intent_sequence,
        );
        let updated = sqlx::query(
            "UPDATE collector_operations
             SET task_type = CASE WHEN task_type = 'unspecified' THEN ?1 ELSE task_type END,
                 trigger_kind = CASE WHEN trigger_kind = 'unspecified' THEN 'collector' ELSE trigger_kind END,
                 status = 'running', started_at_ms = COALESCE(started_at_ms, ?2),
                 updated_at_ms = ?2
             WHERE operation_key = ?3
               AND status IN ('queued', 'running')",
        )
        .bind(task_type)
        .bind(started_at_ms)
        .bind(operation_key)
        .execute(session.connection())
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(PersistenceError::InvariantViolation(
                "collector operation cannot enter running state".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn finish_collector_operation(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        endpoint_revision: i64,
        credential_revision: i64,
        intent_sequence: i64,
        terminal_status: &str,
        reason_code: Option<&str>,
        finished_at_ms: i64,
    ) -> Result<(), PersistenceError> {
        if !matches!(
            terminal_status,
            "succeeded"
                | "partially_succeeded"
                | "failed"
                | "cancelled"
                | "interrupted"
                | "superseded"
        ) || finished_at_ms < 0
            || reason_code.is_some_and(|value| value.len() > COLLECTOR_OPERATION_REASON_MAX_BYTES)
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        let operation_key = collector_operation_key(
            station_id,
            endpoint_revision,
            credential_revision,
            intent_sequence,
        );
        let updated = sqlx::query(
            "UPDATE collector_operations
             SET status = ?1, finished_at_ms = ?2, reason_code = ?3,
                 reason_detail = NULL, updated_at_ms = ?2
             WHERE operation_key = ?4
               AND status IN ('queued', 'running')",
        )
        .bind(terminal_status)
        .bind(finished_at_ms)
        .bind(reason_code.map(str::trim).filter(|value| !value.is_empty()))
        .bind(&operation_key)
        .execute(session.connection())
        .await?
        .rows_affected();
        if updated == 0 {
            let existing_status = sqlx::query_scalar::<_, String>(
                "SELECT status FROM collector_operations
                 WHERE operation_key = ?1",
            )
            .bind(&operation_key)
            .fetch_optional(session.connection())
            .await?;
            if existing_status.as_deref() == Some(terminal_status) {
                return Ok(());
            }
        }
        if updated != 1 {
            return Err(PersistenceError::InvariantViolation(
                "collector operation cannot enter terminal state".into(),
            ));
        }
        Ok(())
    }

    /// Read the durable station-collection revision inside an existing write
    /// transaction. Keeping this query in the persistence store prevents
    /// application services from depending on SQLx details while preserving
    /// the transaction's revision fence.
    pub(crate) async fn station_collection_revision(
        &self,
        session: &mut WriteSession,
        station_id: &str,
    ) -> Result<Option<i64>, PersistenceError> {
        if station_id.trim().is_empty() {
            return Err(PersistenceError::ConstraintViolation);
        }
        sqlx::query_scalar::<_, i64>("SELECT revision FROM domain_revisions WHERE scope = ?1")
            .bind(format!("station_collection:{station_id}"))
            .fetch_optional(session.connection())
            .await
            .map_err(Into::into)
    }

    /// Allocate the next durable station-scoped collection intent before any
    /// provider request leaves the process.  SQLite serializes this write,
    /// making the returned sequence monotonic across concurrent callers and
    /// process restarts.
    pub(crate) async fn allocate_station_collection_intent(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        endpoint_revision: i64,
        credential_revision: i64,
        updated_at_ms: i64,
    ) -> Result<i64, PersistenceError> {
        if station_id.trim().is_empty()
            || endpoint_revision < 1
            || credential_revision < 1
            || updated_at_ms < 0
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        self.assert_endpoint_revision(session, station_id, endpoint_revision)
            .await?;
        self.assert_station_credential_revision(session, station_id, credential_revision)
            .await?;
        let scope = format!("station_collection_intent:{station_id}");
        sqlx::query(
            "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
             VALUES (?1, 1, ?2, 'transactional_write')
             ON CONFLICT(scope) DO UPDATE SET
               revision = domain_revisions.revision + 1,
               updated_at_ms = excluded.updated_at_ms,
               provenance = 'transactional_write'",
        )
        .bind(&scope)
        .bind(updated_at_ms)
        .execute(session.connection())
        .await?;
        let intent_sequence =
            sqlx::query_scalar::<_, i64>("SELECT revision FROM domain_revisions WHERE scope = ?1")
                .bind(&scope)
                .fetch_one(session.connection())
                .await?;
        let operation_key = collector_operation_key(
            station_id,
            endpoint_revision,
            credential_revision,
            intent_sequence,
        );
        sqlx::query(
            "INSERT INTO collector_operations (
                operation_id, operation_key, station_id, endpoint_revision,
                credential_revision, intent_sequence, plan_version, task_type,
                trigger_kind, status, started_at_ms, finished_at_ms,
                reason_code, reason_detail, created_at_ms, updated_at_ms
             ) VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, 'unspecified',
                       'unspecified', 'queued', NULL, NULL, NULL, NULL, ?7, ?7)",
        )
        .bind(operation_key)
        .bind(station_id)
        .bind(endpoint_revision)
        .bind(credential_revision)
        .bind(intent_sequence)
        .bind(COLLECTOR_OPERATION_PLAN_VERSION)
        .bind(updated_at_ms)
        .execute(session.connection())
        .await?;
        Ok(intent_sequence)
    }

    /// Reject a result carrying an intent allocated before a newer intent.
    /// This check is deliberately independent of completion timestamps: an
    /// older, slower provider response must never mutate current facts.
    pub(crate) async fn assert_station_collection_intent(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        endpoint_revision: i64,
        credential_revision: i64,
        intent_sequence: i64,
    ) -> Result<(), PersistenceError> {
        if station_id.trim().is_empty() || intent_sequence < 1 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let scope = format!("station_collection_intent:{station_id}");
        let current =
            sqlx::query_scalar::<_, i64>("SELECT revision FROM domain_revisions WHERE scope = ?1")
                .bind(&scope)
                .fetch_optional(session.connection())
                .await?;
        let current = match current {
            Some(current) => current,
            None => {
                #[cfg(test)]
                {
                    sqlx::query(
                        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
                         VALUES (?1, 1, 0, 'transactional_write')",
                    )
                    .bind(&scope)
                    .execute(session.connection())
                    .await?;
                    1
                }
                #[cfg(not(test))]
                {
                    return Err(PersistenceError::RevisionUnavailable(scope.clone()));
                }
            }
        };
        if intent_sequence != current {
            return Err(PersistenceError::StaleRevision);
        }
        self.assert_endpoint_revision(session, station_id, endpoint_revision)
            .await?;
        self.assert_station_credential_revision(session, station_id, credential_revision)
            .await
    }

    /// Upsert the typed station collection projection in the same terminal
    /// transaction as the run and its facts.
    pub(crate) async fn upsert_station_collection_projection(
        &self,
        session: &mut WriteSession,
        projection: &StationCollectionProjectionWrite,
    ) -> Result<(), PersistenceError> {
        if projection.station_id.trim().is_empty()
            || projection.status.trim().is_empty()
            || projection.reason_codes_json.trim().is_empty()
            || projection.operation_id.trim().is_empty()
            || projection.revision < 1
            || projection.endpoint_revision < 1
            || projection.credential_revision < 1
            || projection.intent_sequence < 1
            || projection.updated_at_ms < 0
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        // Resolve the current watermark before issuing the write. A SQL
        // UPSERT condition can prevent a stale update, but it cannot
        // distinguish that case from an equal-fence write belonging to a
        // different operation. Surface the latter as an invariant violation
        // so the surrounding terminal transaction rolls back completely.
        if let Some(row) = sqlx::query(
            "SELECT revision, intent_sequence, operation_id, updated_at_ms
             FROM station_collection_projection WHERE station_id = ?1",
        )
        .bind(&projection.station_id)
        .fetch_optional(session.connection())
        .await?
        {
            let current_revision = row.try_get::<i64, _>("revision")?;
            let current_intent_sequence = row.try_get::<i64, _>("intent_sequence")?;
            let current_operation_id = row.try_get::<String, _>("operation_id")?;
            let current_updated_at_ms = row.try_get::<i64, _>("updated_at_ms")?;

            if projection.intent_sequence < current_intent_sequence {
                return Err(PersistenceError::StaleRevision);
            }
            if projection.intent_sequence == current_intent_sequence {
                if projection.operation_id != current_operation_id {
                    return Err(PersistenceError::InvariantViolation(
                        "equal station collection intent belongs to a different operation"
                            .to_string(),
                    ));
                }
                // An idempotent replay may arrive with an older timestamp or
                // revision. Keep the first committed projection in that case.
                if projection.updated_at_ms < current_updated_at_ms
                    || projection.revision < current_revision
                {
                    return Ok(());
                }
            }
        }
        sqlx::query(
            "INSERT INTO station_collection_projection
                (station_id, status, reason_codes_json, revision, endpoint_revision,
                 credential_revision, intent_sequence, operation_id, updated_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(station_id) DO UPDATE SET
                status = excluded.status,
                reason_codes_json = excluded.reason_codes_json,
                revision = excluded.revision,
                endpoint_revision = excluded.endpoint_revision,
                credential_revision = excluded.credential_revision,
                intent_sequence = excluded.intent_sequence,
                operation_id = excluded.operation_id,
                updated_at_ms = excluded.updated_at_ms",
        )
        .bind(&projection.station_id)
        .bind(&projection.status)
        .bind(&projection.reason_codes_json)
        .bind(projection.revision)
        .bind(projection.endpoint_revision)
        .bind(projection.credential_revision)
        .bind(projection.intent_sequence)
        .bind(&projection.operation_id)
        .bind(projection.updated_at_ms)
        .execute(session.connection())
        .await?;
        Ok(())
    }

    /// Return the typed task owning the current authorization-expiry state.
    pub(crate) async fn authorization_expiry_task_type(
        &self,
        session: &mut WriteSession,
        station_id: &str,
    ) -> Result<Option<String>, PersistenceError> {
        if station_id.trim().is_empty() {
            return Err(PersistenceError::ConstraintViolation);
        }
        sqlx::query_scalar::<_, String>(
            "WITH ranked AS (
                 SELECT task_type, status, manual_action_required, error_code,
                         ROW_NUMBER() OVER (
                             PARTITION BY task_type
                             ORDER BY CAST(COALESCE(finished_at, started_at, created_at) AS INTEGER) DESC,
                                      created_at DESC, id DESC
                        ) AS row_number
                 FROM collector_runs
                 WHERE station_id = ?1
                   AND task_type IN ('balance', 'groups', 'detect', 'full', 'published_status')
                   AND status IN ('success', 'partial', 'failed', 'manual_required')
             ), live_failure AS (
                 SELECT task_type
                 FROM ranked
                 WHERE row_number = 1
                   AND (status = 'manual_required'
                        OR manual_action_required = 1
                        OR error_code = 'manual_authorization_required')
                 ORDER BY task_type ASC
                 LIMIT 1
             ), projected_failure AS (
                 SELECT operation.task_type
                 FROM station_authorization_projection AS projection
                 JOIN collector_operations AS operation
                   ON operation.operation_key = projection.source_operation_id
                 WHERE projection.station_id = ?1
                   AND projection.status = 'reauthorization_required'
                   AND projection.authority = 'driver_probe'
                   AND operation.station_id = projection.station_id
                   AND operation.credential_revision = projection.credential_revision
                   AND operation.task_type IN ('balance', 'groups', 'detect', 'full', 'published_status')
                 LIMIT 1
             )
             SELECT task_type
             FROM live_failure
             UNION ALL
             SELECT task_type
             FROM projected_failure
             WHERE NOT EXISTS (SELECT 1 FROM live_failure)
             LIMIT 1",
        )
        .bind(station_id)
        .fetch_optional(session.connection())
        .await
        .map_err(Into::into)
    }

    pub(crate) async fn upsert_station_group_binding(
        &self,
        session: &mut WriteSession,
        binding: &StationGroupBindingWrite,
    ) -> Result<StoredStationGroupBindingUpsert, PersistenceError> {
        self.validate_group_binding_references(session, binding)
            .await?;
        let previous = self
            .group_by_identity(
                session,
                &binding.station_id,
                binding.station_key_id.as_deref(),
                &binding.binding_kind,
                &binding.group_key_hash,
            )
            .await?;
        let id = previous
            .as_ref()
            .map(|state| state.id.clone())
            .unwrap_or_else(|| binding.id.clone());
        if binding.parent_group_binding_id.as_deref() == Some(id.as_str()) {
            return Err(PersistenceError::ConstraintViolation);
        }
        let raw_json = binding
            .raw_json_redacted
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(invalid_json)?;

        sqlx::query(
            r#"
            INSERT INTO station_group_bindings (
                id, station_id, station_key_id, binding_kind, parent_group_binding_id,
                group_key_hash, group_id_hash, group_name, binding_status,
                default_rate_multiplier, user_rate_multiplier, effective_rate_multiplier,
                inferred_group_category, group_category_override, rate_source, confidence,
                last_seen_at, last_checked_at, last_rate_changed_at, raw_json_redacted,
                created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, NULL, ?19, ?18, ?18
            )
            ON CONFLICT(id) DO UPDATE SET
                station_key_id = excluded.station_key_id,
                parent_group_binding_id = excluded.parent_group_binding_id,
                group_id_hash = excluded.group_id_hash,
                group_name = excluded.group_name,
                binding_status = CASE
                    WHEN station_group_bindings.binding_status = 'bound'
                         AND excluded.binding_status NOT IN ('missing', 'disabled')
                    THEN station_group_bindings.binding_status
                    ELSE excluded.binding_status
                END,
                default_rate_multiplier = excluded.default_rate_multiplier,
                user_rate_multiplier = excluded.user_rate_multiplier,
                effective_rate_multiplier = excluded.effective_rate_multiplier,
                inferred_group_category = excluded.inferred_group_category,
                group_category_override = CASE
                    WHEN excluded.rate_source IN ('manual', 'remote_scan')
                    THEN excluded.group_category_override
                    ELSE COALESCE(
                        station_group_bindings.group_category_override,
                        excluded.group_category_override
                    )
                END,
                rate_source = excluded.rate_source,
                confidence = excluded.confidence,
                last_seen_at = excluded.last_seen_at,
                last_checked_at = excluded.last_checked_at,
                last_rate_changed_at = CASE
                    WHEN station_group_bindings.effective_rate_multiplier
                         IS NOT excluded.effective_rate_multiplier
                    THEN excluded.updated_at
                    ELSE station_group_bindings.last_rate_changed_at
                END,
                raw_json_redacted = excluded.raw_json_redacted,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&id)
        .bind(&binding.station_id)
        .bind(&binding.station_key_id)
        .bind(&binding.binding_kind)
        .bind(&binding.parent_group_binding_id)
        .bind(&binding.group_key_hash)
        .bind(&binding.group_id_hash)
        .bind(&binding.group_name)
        .bind(&binding.binding_status)
        .bind(binding.default_rate_multiplier)
        .bind(binding.user_rate_multiplier)
        .bind(binding.effective_rate_multiplier)
        .bind(&binding.inferred_group_category)
        .bind(&binding.group_category_override)
        .bind(&binding.rate_source)
        .bind(binding.confidence)
        .bind(&binding.last_seen_at)
        .bind(&binding.now)
        .bind(raw_json)
        .execute(session.connection())
        .await?;

        let saved = self.station_group_binding_by_id(session, &id).await?;
        self.disable_shadow_station_group_bindings(session, &saved, &binding.now)
            .await?;
        let current = self.group_by_id(session, &id).await?;
        Ok(StoredStationGroupBindingUpsert {
            binding: saved,
            transition: GroupTransition { previous, current },
        })
    }

    pub(crate) async fn existing_apply(
        &self,
        session: &mut WriteSession,
        run_key: &str,
    ) -> Result<Option<ExistingCollectorApply>, PersistenceError> {
        let row = sqlx::query(
            "SELECT request_hash, id, snapshot_id FROM collector_runs WHERE run_key = ?1",
        )
        .bind(run_key)
        .fetch_optional(session.connection())
        .await?;
        Ok(row.map(|row| ExistingCollectorApply {
            request_hash: row.get("request_hash"),
            outcome: StoredCollectorApply {
                run_id: row.get("id"),
                snapshot_id: row
                    .get::<Option<String>, _>("snapshot_id")
                    .unwrap_or_default(),
                inserted: false,
            },
        }))
    }

    pub(crate) async fn start_run(
        &self,
        session: &mut WriteSession,
        run: &CollectorRunStart,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            "INSERT INTO collector_runs (
                id, run_key, request_hash, station_id, endpoint_revision, parent_run_id,
                adapter, task_type, status, started_at, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'running', ?9, ?9)",
        )
        .bind(&run.id)
        .bind(&run.run_key)
        .bind(&run.request_hash)
        .bind(&run.station_id)
        .bind(run.endpoint_revision)
        .bind(&run.parent_run_id)
        .bind(&run.adapter)
        .bind(&run.task_type)
        .bind(&run.started_at)
        .execute(session.connection())
        .await?;
        Ok(())
    }

    pub(crate) async fn insert_snapshot(
        &self,
        session: &mut WriteSession,
        snapshot: &CollectorSnapshotWrite,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            "INSERT INTO collector_snapshots (
                id, run_id, station_id, endpoint_revision, source, status, fetched_at,
                summary_json, normalized_json, raw_json_redacted, error_message, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )
        .bind(&snapshot.id)
        .bind(&snapshot.run_id)
        .bind(&snapshot.station_id)
        .bind(snapshot.endpoint_revision)
        .bind(&snapshot.source)
        .bind(&snapshot.status)
        .bind(&snapshot.fetched_at)
        .bind(serde_json::to_string(&snapshot.summary_json).map_err(invalid_json)?)
        .bind(serde_json::to_string(&snapshot.normalized_json).map_err(invalid_json)?)
        .bind(
            snapshot
                .raw_json_redacted
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(invalid_json)?,
        )
        .bind(&snapshot.error_message)
        .bind(&snapshot.created_at)
        .execute(session.connection())
        .await?;
        Ok(())
    }

    pub(crate) async fn insert_balance(
        &self,
        session: &mut WriteSession,
        balance: &BalanceWrite,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            "INSERT INTO balance_snapshots (
                id, station_id, station_key_id, scope, value, currency, credit_unit,
                used_value, total_value, today_request_count, total_request_count,
                today_consumption, total_consumption, today_base_consumption,
                total_base_consumption, today_token_count, total_token_count,
                today_input_token_count, today_output_token_count, total_input_token_count,
                total_output_token_count, account_concurrency_limit, low_balance_threshold,
                status, source, confidence, collected_at, created_at, updated_at,
                evidence_confidence, spendability_authority, observed_at_ms, valid_until_ms,
                evidence_profile_version, spendability_reason_code
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                       ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, NULL, ?23, ?24,
                       ?25, ?26, ?27, ?27, ?28, ?29, ?30, ?31, ?32, ?33)",
        )
        .bind(&balance.id)
        .bind(&balance.station_id)
        .bind(&balance.station_key_id)
        .bind(&balance.scope)
        .bind(balance.value)
        .bind(&balance.currency)
        .bind(&balance.credit_unit)
        .bind(balance.used_value)
        .bind(balance.total_value)
        .bind(balance.today_request_count)
        .bind(balance.total_request_count)
        .bind(balance.today_consumption)
        .bind(balance.total_consumption)
        .bind(balance.today_base_consumption)
        .bind(balance.total_base_consumption)
        .bind(balance.today_token_count)
        .bind(balance.total_token_count)
        .bind(balance.today_input_token_count)
        .bind(balance.today_output_token_count)
        .bind(balance.total_input_token_count)
        .bind(balance.total_output_token_count)
        .bind(balance.account_concurrency_limit)
        .bind(&balance.status)
        .bind(&balance.source)
        .bind(balance.confidence)
        .bind(&balance.collected_at)
        .bind(&balance.now)
        .bind(&balance.evidence_confidence)
        .bind(&balance.spendability_authority)
        .bind(balance.observed_at_ms)
        .bind(balance.valid_until_ms)
        .bind(&balance.evidence_profile_version)
        .bind(&balance.spendability_reason_code)
        .execute(session.connection())
        .await?;
        Ok(())
    }

    pub(crate) async fn upsert_group(
        &self,
        session: &mut WriteSession,
        group: &GroupWrite,
    ) -> Result<GroupTransition, PersistenceError> {
        let previous = self
            .group_by_identity(
                session,
                &group.station_id,
                group.station_key_id.as_deref(),
                &group.binding_kind,
                &group.group_key_hash,
            )
            .await?;
        let id = previous
            .as_ref()
            .map(|state| state.id.clone())
            .unwrap_or_else(|| group.id.clone());
        let raw_json = group
            .raw_json_redacted
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(invalid_json)?;
        sqlx::query(
            "INSERT INTO station_group_bindings (
                id, station_id, station_key_id, binding_kind, group_key_hash, group_id_hash,
                group_name, binding_status, default_rate_multiplier, user_rate_multiplier,
                effective_rate_multiplier, inferred_group_category, rate_source, confidence,
                last_seen_at, last_checked_at, last_rate_changed_at, last_seen_run_id,
                raw_json_redacted, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                       ?14, ?15, ?16, NULL, ?17, ?18, ?16, ?16)
             ON CONFLICT(id) DO UPDATE SET
                group_id_hash = excluded.group_id_hash,
                group_name = excluded.group_name,
                binding_status = CASE
                    WHEN station_group_bindings.binding_status = 'bound'
                         AND excluded.binding_status = 'available'
                    THEN 'bound' ELSE excluded.binding_status END,
                default_rate_multiplier = excluded.default_rate_multiplier,
                user_rate_multiplier = excluded.user_rate_multiplier,
                effective_rate_multiplier = excluded.effective_rate_multiplier,
                inferred_group_category = excluded.inferred_group_category,
                rate_source = excluded.rate_source,
                confidence = excluded.confidence,
                last_seen_at = excluded.last_seen_at,
                last_checked_at = excluded.last_checked_at,
                last_seen_run_id = excluded.last_seen_run_id,
                raw_json_redacted = excluded.raw_json_redacted,
                updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(&group.station_id)
        .bind(&group.station_key_id)
        .bind(&group.binding_kind)
        .bind(&group.group_key_hash)
        .bind(&group.group_id_hash)
        .bind(&group.group_name)
        .bind(&group.binding_status)
        .bind(group.default_rate_multiplier)
        .bind(group.user_rate_multiplier)
        .bind(group.effective_rate_multiplier)
        .bind(&group.inferred_group_category)
        .bind(&group.source)
        .bind(group.confidence.clamp(0.0, 1.0))
        .bind(&group.last_seen_at)
        .bind(&group.now)
        .bind(&group.run_id)
        .bind(raw_json)
        .execute(session.connection())
        .await?;
        let current = self.group_by_id(session, &id).await?;
        Ok(GroupTransition { previous, current })
    }

    pub(crate) async fn refresh_station_key_group_projections(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        group_binding_ids: &HashSet<String>,
        now: &str,
    ) -> Result<Vec<String>, PersistenceError> {
        if group_binding_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r#"
            SELECT keys.id,
                   keys.group_name AS current_group_name,
                   keys.rate_multiplier AS current_rate_multiplier,
                   keys.rate_source AS current_rate_source,
                   keys.rate_collected_at AS current_rate_collected_at,
                   bindings.id AS group_binding_id,
                   bindings.group_name AS projected_group_name,
                   bindings.binding_status,
                   bindings.default_rate_multiplier,
                   bindings.user_rate_multiplier,
                   bindings.effective_rate_multiplier,
                   bindings.rate_source AS projected_rate_source
            FROM station_keys keys
            JOIN station_group_bindings bindings ON bindings.id = keys.group_binding_id
            WHERE keys.station_id = ?1
              AND bindings.station_id = ?1
              AND bindings.binding_kind = 'station_group'
            "#,
        )
        .bind(station_id)
        .fetch_all(session.connection())
        .await?;

        let mut updated_ids = Vec::new();
        for row in rows {
            let group_binding_id = row.get::<String, _>("group_binding_id");
            if !group_binding_ids.contains(&group_binding_id) {
                continue;
            }
            let binding_status = row.get::<String, _>("binding_status");
            let available = matches!(binding_status.as_str(), "available" | "bound");
            let projected_rate_multiplier = available
                .then(|| {
                    row.get::<Option<f64>, _>("user_rate_multiplier")
                        .or(row.get::<Option<f64>, _>("effective_rate_multiplier"))
                        .or(row.get::<Option<f64>, _>("default_rate_multiplier"))
                })
                .flatten();
            let projected_rate_source = row.get::<Option<String>, _>("projected_rate_source");
            let projected_group_name = row.get::<String, _>("projected_group_name");
            let current_group_name = row.get::<Option<String>, _>("current_group_name");
            let current_rate_multiplier = row.get::<Option<f64>, _>("current_rate_multiplier");
            let current_rate_source = row.get::<Option<String>, _>("current_rate_source");
            let current_rate_collected_at =
                row.get::<Option<String>, _>("current_rate_collected_at");
            if current_group_name.as_deref() == Some(projected_group_name.as_str())
                && current_rate_multiplier == projected_rate_multiplier
                && current_rate_source == projected_rate_source
                && current_rate_collected_at.as_deref() == Some(now)
            {
                continue;
            }

            let station_key_id = row.get::<String, _>("id");
            sqlx::query(
                r#"
                UPDATE station_keys
                SET group_name = ?1,
                    rate_multiplier = ?2,
                    rate_source = ?3,
                    rate_collected_at = ?4,
                    updated_at = ?4
                WHERE id = ?5 AND station_id = ?6 AND group_binding_id = ?7
                "#,
            )
            .bind(&projected_group_name)
            .bind(projected_rate_multiplier)
            .bind(&projected_rate_source)
            .bind(now)
            .bind(&station_key_id)
            .bind(station_id)
            .bind(&group_binding_id)
            .execute(session.connection())
            .await?;

            updated_ids.push(station_key_id);
        }
        Ok(updated_ids)
    }

    pub(crate) async fn mark_missing_groups(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        sources: &HashSet<String>,
        present_hashes: &HashSet<String>,
        now: &str,
    ) -> Result<Vec<GroupTransition>, PersistenceError> {
        if sources.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            "SELECT id, station_id, station_key_id, binding_kind, group_key_hash,
                    group_name, binding_status, default_rate_multiplier,
                    user_rate_multiplier, effective_rate_multiplier, COALESCE(rate_source, '') AS rate_source
             FROM station_group_bindings
             WHERE station_id = ?1 AND binding_kind = 'station_group' AND binding_status = 'available'",
        )
        .bind(station_id)
        .fetch_all(session.connection())
        .await?;
        let mut transitions = Vec::new();
        for row in rows {
            let previous = row_to_group_state(&row);
            if !sources.contains(&previous.source)
                || present_hashes.contains(&previous.group_key_hash)
            {
                continue;
            }
            sqlx::query(
                "UPDATE station_group_bindings SET binding_status = 'missing', updated_at = ?1
                 WHERE id = ?2 AND binding_status = 'available'",
            )
            .bind(now)
            .bind(&previous.id)
            .execute(session.connection())
            .await?;
            let current = self.group_by_id(session, &previous.id).await?;
            transitions.push(GroupTransition {
                previous: Some(previous),
                current,
            });
        }
        Ok(transitions)
    }

    pub(crate) async fn insert_rate_if_changed(
        &self,
        session: &mut WriteSession,
        rate: &RateWrite,
    ) -> Result<Option<RateTransition>, PersistenceError> {
        let previous = sqlx::query(
            "SELECT effective_rate_multiplier FROM group_rate_records
             WHERE group_binding_id = ?1 ORDER BY checked_at DESC, id DESC LIMIT 1",
        )
        .bind(&rate.group_binding_id)
        .fetch_optional(session.connection())
        .await?;
        let old = previous
            .as_ref()
            .and_then(|row| row.get::<Option<f64>, _>("effective_rate_multiplier"));
        if previous.is_some() && old == rate.effective_rate_multiplier {
            return Ok(None);
        }
        let raw_json = rate
            .raw_json_redacted
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(invalid_json)?;
        sqlx::query(
            "INSERT INTO group_rate_records (
                id, station_id, station_key_id, group_binding_id, binding_kind,
                group_key_hash, group_name, default_rate_multiplier, user_rate_multiplier,
                effective_rate_multiplier, inferred_group_category, source, confidence,
                raw_json_redacted, checked_at, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        )
        .bind(&rate.id)
        .bind(&rate.station_id)
        .bind(&rate.station_key_id)
        .bind(&rate.group_binding_id)
        .bind(&rate.binding_kind)
        .bind(&rate.group_key_hash)
        .bind(&rate.group_name)
        .bind(rate.default_rate_multiplier)
        .bind(rate.user_rate_multiplier)
        .bind(rate.effective_rate_multiplier)
        .bind(&rate.inferred_group_category)
        .bind(&rate.source)
        .bind(rate.confidence.clamp(0.0, 1.0))
        .bind(raw_json)
        .bind(&rate.checked_at)
        .bind(&rate.created_at)
        .execute(session.connection())
        .await?;
        sqlx::query(
            "UPDATE station_group_bindings SET last_rate_changed_at = ?1, updated_at = ?1
             WHERE id = ?2",
        )
        .bind(&rate.created_at)
        .bind(&rate.group_binding_id)
        .execute(session.connection())
        .await?;
        Ok(Some(RateTransition {
            group_binding_id: rate.group_binding_id.clone(),
            group_name: rate.group_name.clone(),
            old_effective_rate_multiplier: old,
            new_effective_rate_multiplier: rate.effective_rate_multiplier,
        }))
    }

    #[cfg(test)]
    pub(crate) async fn update_task_state_for_test(
        &self,
        session: &mut WriteSession,
        state: &CollectorTaskStateWrite,
    ) -> Result<(), PersistenceError> {
        let succeeded = matches!(state.status.as_str(), "success" | "partial");
        sqlx::query(
            "INSERT INTO collector_task_state (
                station_id, task_type, last_run_id, last_status, last_success_at,
                last_failure_at, consecutive_failures, next_due_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(station_id, task_type) DO UPDATE SET
                last_run_id = excluded.last_run_id,
                last_status = excluded.last_status,
                last_success_at = CASE WHEN ?10 = 1 THEN excluded.updated_at ELSE collector_task_state.last_success_at END,
                last_failure_at = CASE WHEN ?10 = 0 THEN excluded.updated_at ELSE collector_task_state.last_failure_at END,
                consecutive_failures = CASE WHEN ?10 = 1 THEN 0 ELSE collector_task_state.consecutive_failures + 1 END,
                next_due_at = excluded.next_due_at,
                updated_at = excluded.updated_at",
        )
        .bind(&state.station_id)
        .bind(&state.task_type)
        .bind(&state.run_id)
        .bind(&state.status)
        .bind(succeeded.then(|| state.finished_at.clone()))
        .bind((!succeeded).then(|| state.finished_at.clone()))
        .bind(i64::from(!succeeded))
        .bind(&state.next_due_at)
        .bind(&state.finished_at)
        .bind(i64::from(succeeded))
        .execute(session.connection())
        .await?;
        Ok(())
    }

    pub(crate) async fn failed_task_types(
        &self,
        session: &mut WriteSession,
        station_id: &str,
    ) -> Result<Vec<String>, PersistenceError> {
        let rows = sqlx::query_scalar::<_, String>(
            "WITH ranked AS (
                 SELECT task_type, status,
                        ROW_NUMBER() OVER (
                            PARTITION BY task_type
                            ORDER BY CAST(COALESCE(finished_at, started_at, created_at) AS INTEGER) DESC,
                                     created_at DESC, id DESC
                        ) AS row_number
                 FROM collector_runs
                 WHERE station_id = ?1
                   AND task_type IN ('balance', 'groups', 'detect', 'full')
                   AND status IN ('success', 'partial', 'failed', 'manual_required')
             )
             SELECT task_type
             FROM ranked
             WHERE row_number = 1
               -- `manual_required` is an authorization/action-required state,
               -- not a collector failure. It is projected separately as an
               -- authorization-expired incident by the application layer.
               AND status = 'failed'
             ORDER BY task_type ASC",
        )
        .bind(station_id)
        .fetch_all(session.connection())
        .await?;
        Ok(rows)
    }

    pub(crate) async fn finish_run(
        &self,
        session: &mut WriteSession,
        finish: &CollectorRunFinish,
    ) -> Result<StoredCollectorApply, PersistenceError> {
        let affected = sqlx::query(
            "UPDATE collector_runs SET status = ?1, finished_at = ?2, duration_ms = ?3,
                endpoint_count = ?4, success_count = ?5, failure_count = ?6,
                manual_action_required = ?7, error_code = ?8, error_message = ?9,
                snapshot_id = ?10 WHERE id = ?11 AND status = 'running'",
        )
        .bind(&finish.status)
        .bind(&finish.finished_at)
        .bind(finish.duration_ms.max(0))
        .bind(finish.endpoint_count.max(0))
        .bind(finish.success_count.max(0))
        .bind(finish.failure_count.max(0))
        .bind(i64::from(finish.manual_action_required))
        .bind(&finish.error_code)
        .bind(&finish.error_message)
        .bind(&finish.snapshot_id)
        .bind(&finish.id)
        .execute(session.connection())
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(PersistenceError::InvariantViolation(
                "collector run terminal transition was not unique".to_string(),
            ));
        }
        Ok(StoredCollectorApply {
            run_id: finish.id.clone(),
            snapshot_id: finish.snapshot_id.clone(),
            inserted: true,
        })
    }

    async fn validate_group_binding_references(
        &self,
        session: &mut WriteSession,
        binding: &StationGroupBindingWrite,
    ) -> Result<(), PersistenceError> {
        match (
            binding.binding_kind.as_str(),
            binding.station_key_id.as_deref(),
        ) {
            ("station_group", None) => {}
            ("key_binding", Some(station_key_id)) => {
                let owned = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM station_keys WHERE id = ?1 AND station_id = ?2",
                )
                .bind(station_key_id)
                .bind(&binding.station_id)
                .fetch_one(session.connection())
                .await?;
                if owned != 1 {
                    return Err(PersistenceError::ConstraintViolation);
                }
            }
            _ => return Err(PersistenceError::ConstraintViolation),
        }

        if let Some(parent_id) = binding.parent_group_binding_id.as_deref() {
            let owned = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM station_group_bindings
                 WHERE id = ?1 AND station_id = ?2 AND binding_kind = 'station_group'",
            )
            .bind(parent_id)
            .bind(&binding.station_id)
            .fetch_one(session.connection())
            .await?;
            if owned != 1 {
                return Err(PersistenceError::ConstraintViolation);
            }
        }
        Ok(())
    }

    async fn station_group_binding_by_id(
        &self,
        session: &mut WriteSession,
        id: &str,
    ) -> Result<StationGroupBinding, PersistenceError> {
        let row = sqlx::query(
            r#"
            SELECT id, station_id, station_key_id, binding_kind, parent_group_binding_id,
                   group_key_hash, group_id_hash, group_name, binding_status,
                   default_rate_multiplier, user_rate_multiplier, effective_rate_multiplier,
                   inferred_group_category, group_category_override, rate_source, confidence,
                   last_seen_at, last_checked_at, last_rate_changed_at, raw_json_redacted,
                   created_at, updated_at
            FROM station_group_bindings
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .fetch_optional(session.connection())
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
        row_to_station_group_binding(row)
    }

    async fn disable_shadow_station_group_bindings(
        &self,
        session: &mut WriteSession,
        saved: &StationGroupBinding,
        now: &str,
    ) -> Result<(), PersistenceError> {
        if saved.binding_kind != "station_group"
            || saved.binding_status != "available"
            || saved.rate_source.as_deref() == Some("remote_scan")
        {
            return Ok(());
        }
        sqlx::query(
            "UPDATE station_group_bindings
             SET binding_status = 'disabled', updated_at = ?1
             WHERE station_id = ?2
               AND binding_kind = 'station_group'
               AND id != ?3
               AND binding_status != 'disabled'
               AND rate_source = 'remote_scan'
               AND lower(trim(group_name)) = lower(trim(?4))",
        )
        .bind(now)
        .bind(&saved.station_id)
        .bind(&saved.id)
        .bind(&saved.group_name)
        .execute(session.connection())
        .await?;
        Ok(())
    }

    async fn group_by_identity(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        station_key_id: Option<&str>,
        binding_kind: &str,
        group_key_hash: &str,
    ) -> Result<Option<GroupState>, PersistenceError> {
        let row = if binding_kind == "station_group" {
            sqlx::query(
                "SELECT id, station_id, station_key_id, binding_kind, group_key_hash,
                        group_name, binding_status, default_rate_multiplier,
                        user_rate_multiplier, effective_rate_multiplier, COALESCE(rate_source, '') AS rate_source
                 FROM station_group_bindings
                 WHERE station_id = ?1 AND binding_kind = ?2 AND group_key_hash = ?3",
            )
            .bind(station_id)
            .bind(binding_kind)
            .bind(group_key_hash)
            .fetch_optional(session.connection())
            .await?
        } else {
            sqlx::query(
                "SELECT id, station_id, station_key_id, binding_kind, group_key_hash,
                        group_name, binding_status, default_rate_multiplier,
                        user_rate_multiplier, effective_rate_multiplier, COALESCE(rate_source, '') AS rate_source
                 FROM station_group_bindings
                 WHERE station_key_id = ?1 AND binding_kind = ?2 AND group_key_hash = ?3",
            )
            .bind(station_key_id)
            .bind(binding_kind)
            .bind(group_key_hash)
            .fetch_optional(session.connection())
            .await?
        };
        Ok(row.as_ref().map(row_to_group_state))
    }

    async fn group_by_id(
        &self,
        session: &mut WriteSession,
        id: &str,
    ) -> Result<GroupState, PersistenceError> {
        let row = sqlx::query(
            "SELECT id, station_id, station_key_id, binding_kind, group_key_hash,
                    group_name, binding_status, default_rate_multiplier,
                    user_rate_multiplier, effective_rate_multiplier, COALESCE(rate_source, '') AS rate_source
             FROM station_group_bindings WHERE id = ?1",
        )
        .bind(id)
        .fetch_one(session.connection())
        .await?;
        Ok(row_to_group_state(&row))
    }
}

fn row_to_collector_snapshot(
    row: sqlx::sqlite::SqliteRow,
) -> Result<CollectorSnapshot, PersistenceError> {
    let parse_json = |column: &str| -> Result<Value, PersistenceError> {
        serde_json::from_str(&row.get::<String, _>(column)).map_err(|_| {
            PersistenceError::InvariantViolation(format!(
                "collector snapshot contains invalid {column}"
            ))
        })
    };
    let raw_json_redacted = row
        .get::<Option<String>, _>("raw_json_redacted")
        .map(|value| {
            serde_json::from_str(&value).map_err(|_| {
                PersistenceError::InvariantViolation(
                    "collector snapshot contains invalid raw_json_redacted".into(),
                )
            })
        })
        .transpose()?;
    Ok(CollectorSnapshot {
        id: row.get("id"),
        station_id: row.get("station_id"),
        endpoint_revision: row.get("endpoint_revision"),
        source: row.get("source"),
        status: row.get("status"),
        fetched_at: row.get("fetched_at"),
        summary_json: parse_json("summary_json")?,
        normalized_json: parse_json("normalized_json")?,
        raw_json_redacted,
        error_message: row.get("error_message"),
        created_at: row.get("created_at"),
    })
}

fn row_to_station_group_binding(
    row: sqlx::sqlite::SqliteRow,
) -> Result<StationGroupBinding, PersistenceError> {
    let raw_json = parse_optional_json(
        row.try_get("raw_json_redacted")?,
        "station group binding raw_json_redacted",
    )?;
    Ok(StationGroupBinding {
        id: row.try_get("id")?,
        station_id: row.try_get("station_id")?,
        station_key_id: row.try_get("station_key_id")?,
        binding_kind: row.try_get("binding_kind")?,
        parent_group_binding_id: row.try_get("parent_group_binding_id")?,
        group_key_hash: row.try_get("group_key_hash")?,
        group_id_hash: row.try_get("group_id_hash")?,
        group_name: row.try_get("group_name")?,
        binding_status: row.try_get("binding_status")?,
        default_rate_multiplier: row.try_get("default_rate_multiplier")?,
        user_rate_multiplier: row.try_get("user_rate_multiplier")?,
        effective_rate_multiplier: row.try_get("effective_rate_multiplier")?,
        inferred_group_category: row.try_get("inferred_group_category")?,
        group_category_override: row.try_get("group_category_override")?,
        rate_source: row.try_get("rate_source")?,
        confidence: row.try_get("confidence")?,
        last_seen_at: row.try_get("last_seen_at")?,
        last_checked_at: row.try_get("last_checked_at")?,
        last_rate_changed_at: row.try_get("last_rate_changed_at")?,
        raw_json_redacted: raw_json,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_group_rate_record(
    row: sqlx::sqlite::SqliteRow,
) -> Result<GroupRateRecord, PersistenceError> {
    Ok(GroupRateRecord {
        id: row.try_get("id")?,
        station_id: row.try_get("station_id")?,
        station_key_id: row.try_get("station_key_id")?,
        group_binding_id: row.try_get("group_binding_id")?,
        binding_kind: row.try_get("binding_kind")?,
        group_key_hash: row.try_get("group_key_hash")?,
        group_name: row.try_get("group_name")?,
        default_rate_multiplier: row.try_get("default_rate_multiplier")?,
        user_rate_multiplier: row.try_get("user_rate_multiplier")?,
        effective_rate_multiplier: row.try_get("effective_rate_multiplier")?,
        inferred_group_category: row.try_get("inferred_group_category")?,
        source: row.try_get("source")?,
        confidence: row.try_get("confidence")?,
        raw_json_redacted: parse_optional_json(
            row.try_get("raw_json_redacted")?,
            "group rate raw_json_redacted",
        )?,
        checked_at: row.try_get("checked_at")?,
        created_at: row.try_get("created_at")?,
    })
}

fn row_to_collector_run(row: sqlx::sqlite::SqliteRow) -> Result<CollectorRun, PersistenceError> {
    Ok(CollectorRun {
        id: row.try_get("id")?,
        station_id: row.try_get("station_id")?,
        endpoint_revision: row.try_get("endpoint_revision")?,
        parent_run_id: row.try_get("parent_run_id")?,
        adapter: row.try_get("adapter")?,
        task_type: row.try_get("task_type")?,
        status: row.try_get("status")?,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
        duration_ms: row.try_get("duration_ms")?,
        endpoint_count: row.try_get("endpoint_count")?,
        success_count: row.try_get("success_count")?,
        failure_count: row.try_get("failure_count")?,
        manual_action_required: row.try_get::<i64, _>("manual_action_required")? != 0,
        error_code: row.try_get("error_code")?,
        error_message: row.try_get("error_message")?,
        snapshot_id: row.try_get("snapshot_id")?,
        created_at: row.try_get("created_at")?,
    })
}

fn parse_optional_json(
    value: Option<String>,
    field: &str,
) -> Result<Option<Value>, PersistenceError> {
    value
        .map(|json| {
            serde_json::from_str(&json).map_err(|_| {
                PersistenceError::InvariantViolation(format!("{field} contains invalid JSON"))
            })
        })
        .transpose()
}

fn row_to_group_state(row: &sqlx::sqlite::SqliteRow) -> GroupState {
    GroupState {
        id: row.get("id"),
        station_id: row.get("station_id"),
        station_key_id: row.get("station_key_id"),
        binding_kind: row.get("binding_kind"),
        group_key_hash: row.get("group_key_hash"),
        group_name: row.get("group_name"),
        binding_status: row.get("binding_status"),
        default_rate_multiplier: row.get("default_rate_multiplier"),
        user_rate_multiplier: row.get("user_rate_multiplier"),
        effective_rate_multiplier: row.get("effective_rate_multiplier"),
        source: row.get("rate_source"),
    }
}

fn invalid_json(error: serde_json::Error) -> PersistenceError {
    PersistenceError::InvariantViolation(format!("collector JSON serialization failed: {error}"))
}
