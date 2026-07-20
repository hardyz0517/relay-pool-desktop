use std::collections::{BTreeSet, HashSet};

use serde_json::Value;
use sqlx::Row;

use crate::persistence::{error::PersistenceError, write_session::WriteSession};

#[derive(Debug, Clone)]
pub(crate) struct CollectorRunStart {
    pub id: String,
    pub run_key: String,
    pub request_hash: String,
    pub station_id: String,
    pub endpoint_revision: i64,
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

#[derive(Debug, Clone)]
pub(crate) struct ModelWrite {
    pub station_id: String,
    pub model: String,
    pub available: bool,
    pub source: String,
    pub confidence: f64,
    pub run_id: String,
    pub now: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ModelTransitions {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CollectorTaskStateWrite {
    pub station_id: String,
    pub task_type: String,
    pub run_id: String,
    pub status: String,
    pub finished_at: String,
    pub next_due_at: Option<String>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct CollectorStore;

impl CollectorStore {
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
                status, source, confidence, collected_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                       ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, NULL, ?23, ?24,
                       ?25, ?26, ?27, ?27)",
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

    pub(crate) async fn replace_models(
        &self,
        session: &mut WriteSession,
        station_id: &str,
        run_id: &str,
        models: &[ModelWrite],
        now: &str,
    ) -> Result<ModelTransitions, PersistenceError> {
        let previous = sqlx::query_scalar::<_, String>(
            "SELECT model FROM collector_model_facts WHERE station_id = ?1 AND available = 1",
        )
        .bind(station_id)
        .fetch_all(session.connection())
        .await?
        .into_iter()
        .collect::<BTreeSet<_>>();
        let current = models
            .iter()
            .filter(|model| model.available)
            .map(|model| model.model.clone())
            .collect::<BTreeSet<_>>();
        for model in models {
            sqlx::query(
                "INSERT INTO collector_model_facts (
                    station_id, model, available, source, confidence, last_seen_run_id, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(station_id, model) DO UPDATE SET
                    available = excluded.available, source = excluded.source,
                    confidence = excluded.confidence, last_seen_run_id = excluded.last_seen_run_id,
                    updated_at = excluded.updated_at",
            )
            .bind(&model.station_id)
            .bind(&model.model)
            .bind(i64::from(model.available))
            .bind(&model.source)
            .bind(model.confidence.clamp(0.0, 1.0))
            .bind(&model.run_id)
            .bind(&model.now)
            .execute(session.connection())
            .await?;
        }
        sqlx::query(
            "UPDATE collector_model_facts SET available = 0, last_seen_run_id = ?1, updated_at = ?2
             WHERE station_id = ?3 AND available = 1 AND last_seen_run_id != ?1",
        )
        .bind(run_id)
        .bind(now)
        .bind(station_id)
        .execute(session.connection())
        .await?;
        Ok(ModelTransitions {
            added: current.difference(&previous).cloned().collect(),
            removed: previous.difference(&current).cloned().collect(),
        })
    }

    pub(crate) async fn update_task_state(
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
