use sqlx::{Row, SqliteConnection};

use crate::{
    models::channel_monitors::{
        ChannelMonitor, ChannelMonitorRequestTemplate, CreateChannelMonitorInput,
        CreateChannelMonitorTemplateInput, UpdateChannelMonitorInput,
        UpdateChannelMonitorTemplateInput,
    },
    persistence::{
        error::PersistenceError, read_session::ReadSession, write_session::WriteSession,
    },
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MonitoringStore;

#[derive(Debug, Clone)]
pub(crate) struct NewMonitorTemplateRow {
    pub(crate) id: String,
    pub(crate) now: String,
    pub(crate) input: CreateChannelMonitorTemplateInput,
}

#[derive(Debug, Clone)]
pub(crate) struct MonitorTemplatePatch {
    pub(crate) now: String,
    pub(crate) input: UpdateChannelMonitorTemplateInput,
}

#[derive(Debug, Clone)]
pub(crate) struct NewMonitorRow {
    pub(crate) id: String,
    pub(crate) now: String,
    pub(crate) next_run_at: Option<String>,
    pub(crate) input: CreateChannelMonitorInput,
}

#[derive(Debug, Clone)]
pub(crate) struct MonitorPatch {
    pub(crate) now: String,
    pub(crate) input: UpdateChannelMonitorInput,
}

impl MonitoringStore {
    pub(crate) async fn get_template(
        &self,
        read: &mut ReadSession,
        id: &str,
    ) -> Result<ChannelMonitorRequestTemplate, PersistenceError> {
        template_by_id(read.connection(), id).await
    }

    pub(crate) async fn list_templates(
        &self,
        read: &mut ReadSession,
        limit: u32,
    ) -> Result<Vec<ChannelMonitorRequestTemplate>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, endpoint_kind, method, path, request_body_json,
                   enabled, built_in, note, created_at, updated_at
            FROM channel_monitor_request_templates INDEXED BY idx_channel_monitor_templates_list
            ORDER BY enabled DESC, built_in DESC, updated_at DESC, id DESC
            LIMIT ?1
            "#,
        )
        .bind(i64::from(limit))
        .fetch_all(read.connection())
        .await?;
        Ok(rows.into_iter().map(row_to_template).collect())
    }

    pub(crate) async fn insert_template(
        &self,
        write: &mut WriteSession,
        row: NewMonitorTemplateRow,
    ) -> Result<ChannelMonitorRequestTemplate, PersistenceError> {
        validate_template(
            &row.input.name,
            &row.input.endpoint_kind,
            &row.input.method,
            &row.input.path,
            &row.input.request_body_json,
        )?;
        sqlx::query(
            r#"
            INSERT INTO channel_monitor_request_templates (
                id, name, endpoint_kind, method, path, request_body_json,
                enabled, built_in, note, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?10)
            "#,
        )
        .bind(&row.id)
        .bind(row.input.name.trim())
        .bind(row.input.endpoint_kind.trim())
        .bind(row.input.method.trim().to_uppercase())
        .bind(row.input.path.trim())
        .bind(row.input.request_body_json.trim())
        .bind(bool_to_i64(row.input.enabled))
        .bind(normalize_optional(&row.input.note))
        .bind(&row.now)
        .bind(&row.now)
        .execute(write.connection())
        .await?;
        template_by_id(write.connection(), &row.id).await
    }

    pub(crate) async fn update_template(
        &self,
        write: &mut WriteSession,
        patch: MonitorTemplatePatch,
    ) -> Result<ChannelMonitorRequestTemplate, PersistenceError> {
        validate_template(
            &patch.input.name,
            &patch.input.endpoint_kind,
            &patch.input.method,
            &patch.input.path,
            &patch.input.request_body_json,
        )?;
        let changed = sqlx::query(
            r#"
            UPDATE channel_monitor_request_templates
            SET name = ?1,
                endpoint_kind = ?2,
                method = ?3,
                path = ?4,
                request_body_json = ?5,
                enabled = ?6,
                note = ?7,
                updated_at = ?8
            WHERE id = ?9 AND built_in = 0
            "#,
        )
        .bind(patch.input.name.trim())
        .bind(patch.input.endpoint_kind.trim())
        .bind(patch.input.method.trim().to_uppercase())
        .bind(patch.input.path.trim())
        .bind(patch.input.request_body_json.trim())
        .bind(bool_to_i64(patch.input.enabled))
        .bind(normalize_optional(&patch.input.note))
        .bind(&patch.now)
        .bind(&patch.input.id)
        .execute(write.connection())
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        template_by_id(write.connection(), &patch.input.id).await
    }

    pub(crate) async fn delete_template(
        &self,
        write: &mut WriteSession,
        id: &str,
    ) -> Result<(), PersistenceError> {
        let deleted = sqlx::query(
            r#"
            DELETE FROM channel_monitor_request_templates
            WHERE id = ?1
              AND built_in = 0
              AND NOT EXISTS (SELECT 1 FROM channel_monitors WHERE template_id = ?1)
            "#,
        )
        .bind(id)
        .execute(write.connection())
        .await?
        .rows_affected();
        if deleted == 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        Ok(())
    }

    pub(crate) async fn list_monitors(
        &self,
        read: &mut ReadSession,
        limit: u32,
    ) -> Result<Vec<ChannelMonitor>, PersistenceError> {
        list_monitors(read.connection(), limit).await
    }

    pub(crate) async fn insert_monitor(
        &self,
        write: &mut WriteSession,
        row: NewMonitorRow,
    ) -> Result<ChannelMonitor, PersistenceError> {
        validate_monitor_input(write.connection(), &row.input).await?;
        let fallback_models =
            serialize_legacy_models(&row.input.primary_model, &row.input.fallback_models)?;
        let fallback_models_v2 = serialize_fallback_models(&row.input.fallback_models)?;
        let next_due_at_ms = row.next_run_at.as_deref().and_then(parse_millis);
        sqlx::query(
            r#"
            INSERT INTO channel_monitors (
                id, name, target_type, station_id, station_key_id, template_id,
                enabled, interval_seconds, jitter_seconds, timeout_seconds,
                max_concurrency, consecutive_failure_threshold, fallback_models_json,
                protocol_kind, client_profile_id, client_profile_version, primary_model,
                fallback_models_v2_json, retry_max_attempts_per_model,
                retry_initial_backoff_ms, retry_max_backoff_ms, risk_daily_probe_budget,
                health_policy_mode, health_failure_threshold, health_recovery_threshold,
                attempt_timeout_ms, execution_timeout_ms, schedule_revision, next_due_at_ms,
                last_run_at, next_run_at, last_status, last_error_message,
                note, created_at, updated_at, pause_on_zero_balance
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25,
                ?26, ?27, 1, ?28, NULL, ?29, NULL, NULL, ?30, ?31, ?32, ?33
            )
            "#,
        )
        .bind(&row.id)
        .bind(row.input.name.trim())
        .bind(row.input.target_type.trim())
        .bind(&row.input.station_id)
        .bind(normalize_optional(&row.input.station_key_id))
        .bind(&row.input.template_id)
        .bind(bool_to_i64(row.input.enabled))
        .bind(row.input.interval_seconds)
        .bind(row.input.jitter_seconds)
        .bind(row.input.timeout_seconds)
        .bind(row.input.max_concurrency)
        .bind(row.input.consecutive_failure_threshold)
        .bind(fallback_models)
        .bind(&row.input.protocol_kind)
        .bind(&row.input.client_profile_id)
        .bind(row.input.client_profile_version)
        .bind(row.input.primary_model.trim())
        .bind(fallback_models_v2)
        .bind(row.input.retry_max_attempts_per_model)
        .bind(row.input.retry_initial_backoff_ms)
        .bind(row.input.retry_max_backoff_ms)
        .bind(row.input.risk_daily_probe_budget)
        .bind(&row.input.health_policy_mode)
        .bind(row.input.health_failure_threshold)
        .bind(row.input.health_recovery_threshold)
        .bind(row.input.attempt_timeout_ms)
        .bind(row.input.execution_timeout_ms)
        .bind(next_due_at_ms)
        .bind(&row.next_run_at)
        .bind(normalize_optional(&row.input.note))
        .bind(&row.now)
        .bind(&row.now)
        .bind(bool_to_i64(row.input.pause_on_zero_balance))
        .execute(write.connection())
        .await?;
        monitor_by_id(write.connection(), &row.id).await
    }

    pub(crate) async fn update_monitor(
        &self,
        write: &mut WriteSession,
        patch: MonitorPatch,
    ) -> Result<ChannelMonitor, PersistenceError> {
        validate_monitor_update(write.connection(), &patch.input).await?;
        let fallback_models =
            serialize_legacy_models(&patch.input.primary_model, &patch.input.fallback_models)?;
        let fallback_models_v2 = serialize_fallback_models(&patch.input.fallback_models)?;
        let next_due_at_ms = patch
            .input
            .enabled
            .then(|| parse_millis(&patch.now).unwrap_or(0));
        let changed = sqlx::query(
            r#"
            UPDATE channel_monitors
            SET name = ?1,
                target_type = ?2,
                station_id = ?3,
                station_key_id = ?4,
                template_id = ?5,
                enabled = ?6,
                pause_on_zero_balance = ?32,
                interval_seconds = ?7,
                jitter_seconds = ?8,
                timeout_seconds = ?9,
                max_concurrency = ?10,
                consecutive_failure_threshold = ?11,
                fallback_models_json = ?12,
                protocol_kind = ?13,
                client_profile_id = ?14,
                client_profile_version = ?15,
                primary_model = ?16,
                fallback_models_v2_json = ?17,
                retry_max_attempts_per_model = ?18,
                retry_initial_backoff_ms = ?19,
                retry_max_backoff_ms = ?20,
                risk_daily_probe_budget = ?21,
                health_policy_mode = ?22,
                health_failure_threshold = ?23,
                health_recovery_threshold = ?24,
                attempt_timeout_ms = ?25,
                execution_timeout_ms = ?26,
                schedule_revision = schedule_revision + 1,
                next_due_at_ms = ?27,
                next_run_at = ?28,
                note = ?29,
                updated_at = ?30
            WHERE id = ?31
            "#,
        )
        .bind(patch.input.name.trim())
        .bind(patch.input.target_type.trim())
        .bind(&patch.input.station_id)
        .bind(normalize_optional(&patch.input.station_key_id))
        .bind(&patch.input.template_id)
        .bind(bool_to_i64(patch.input.enabled))
        .bind(patch.input.interval_seconds)
        .bind(patch.input.jitter_seconds)
        .bind(patch.input.timeout_seconds)
        .bind(patch.input.max_concurrency)
        .bind(patch.input.consecutive_failure_threshold)
        .bind(fallback_models)
        .bind(&patch.input.protocol_kind)
        .bind(&patch.input.client_profile_id)
        .bind(patch.input.client_profile_version)
        .bind(patch.input.primary_model.trim())
        .bind(fallback_models_v2)
        .bind(patch.input.retry_max_attempts_per_model)
        .bind(patch.input.retry_initial_backoff_ms)
        .bind(patch.input.retry_max_backoff_ms)
        .bind(patch.input.risk_daily_probe_budget)
        .bind(&patch.input.health_policy_mode)
        .bind(patch.input.health_failure_threshold)
        .bind(patch.input.health_recovery_threshold)
        .bind(patch.input.attempt_timeout_ms)
        .bind(patch.input.execution_timeout_ms)
        .bind(next_due_at_ms)
        .bind(next_due_at_ms.map(|value| value.to_string()))
        .bind(normalize_optional(&patch.input.note))
        .bind(&patch.now)
        .bind(&patch.input.id)
        .bind(bool_to_i64(patch.input.pause_on_zero_balance))
        .execute(write.connection())
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(PersistenceError::NotFound);
        }
        monitor_by_id(write.connection(), &patch.input.id).await
    }

    pub(crate) async fn delete_monitor(
        &self,
        write: &mut WriteSession,
        id: &str,
    ) -> Result<(), PersistenceError> {
        let deleted = sqlx::query("DELETE FROM channel_monitors WHERE id = ?1")
            .bind(id)
            .execute(write.connection())
            .await?
            .rows_affected();
        if deleted == 0 {
            return Err(PersistenceError::NotFound);
        }
        Ok(())
    }
}

async fn list_monitors(
    connection: &mut SqliteConnection,
    limit: u32,
) -> Result<Vec<ChannelMonitor>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        WITH latest_balance_spendability AS (
            SELECT * FROM (
                SELECT b.*, ROW_NUMBER() OVER (
                    PARTITION BY b.station_id, b.station_key_id, b.scope
                    ORDER BY b.updated_at DESC, b.created_at DESC, b.id DESC
                ) AS row_number
                FROM balance_snapshots b
            ) WHERE row_number = 1
        )
        SELECT m.id, m.name, m.target_type, m.station_id, m.station_key_id, m.template_id,
               m.enabled, m.pause_on_zero_balance,
               CASE
                   WHEN m.pause_on_zero_balance = 1 AND (
                       (m.target_type = 'station' AND EXISTS (
                           SELECT 1 FROM latest_balance_spendability b
                           WHERE b.station_id = m.station_id AND b.station_key_id IS NULL
                             AND b.scope IN ('station', 'station_account')
                             AND b.status IN ('depleted', 'exhausted', 'empty')
                             AND b.evidence_confidence = 'confirmed'
                             AND b.spendability_authority = 'authoritative'
                             AND (b.valid_until_ms IS NULL OR b.valid_until_ms >= CAST(strftime('%s','now') AS INTEGER) * 1000)
                       ))
                       OR (m.target_type = 'station_key' AND NOT EXISTS (
                           SELECT 1 FROM latest_balance_spendability b
                           WHERE b.station_id = m.station_id AND b.station_key_id IS NULL
                             AND b.scope IN ('station', 'station_account')
                             AND b.status IN ('normal', 'available', 'usable', 'low', 'warning')
                             AND b.evidence_confidence = 'confirmed'
                             AND b.spendability_authority = 'authoritative'
                             AND (b.valid_until_ms IS NULL OR b.valid_until_ms >= CAST(strftime('%s','now') AS INTEGER) * 1000)
                       ) AND EXISTS (
                           SELECT 1 FROM latest_balance_spendability b
                           WHERE ((b.station_id = m.station_id AND b.station_key_id IS NULL AND b.scope IN ('station', 'station_account'))
                              OR (b.station_key_id = m.station_key_id AND b.scope = 'station_key'))
                             AND b.status IN ('depleted', 'exhausted', 'empty')
                             AND b.evidence_confidence = 'confirmed'
                             AND b.spendability_authority = 'authoritative'
                             AND (b.valid_until_ms IS NULL OR b.valid_until_ms >= CAST(strftime('%s','now') AS INTEGER) * 1000)
                       ))
                   ) THEN 1 ELSE 0
               END AS balance_paused,
               m.interval_seconds, m.jitter_seconds, m.timeout_seconds,
               max_concurrency, consecutive_failure_threshold, fallback_models_json,
               protocol_kind, client_profile_id, client_profile_version, primary_model,
               fallback_models_v2_json, retry_max_attempts_per_model,
               retry_initial_backoff_ms, retry_max_backoff_ms, risk_daily_probe_budget,
               health_policy_mode, health_failure_threshold, health_recovery_threshold,
               attempt_timeout_ms, execution_timeout_ms, schedule_revision,
               note, created_at, updated_at
        FROM channel_monitors m INDEXED BY idx_channel_monitors_list
        ORDER BY m.enabled DESC, m.created_at ASC, m.id ASC
        LIMIT ?1
        "#,
    )
    .bind(i64::from(limit))
    .fetch_all(connection)
    .await?;
    rows.into_iter().map(row_to_monitor).collect()
}

async fn template_by_id(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<ChannelMonitorRequestTemplate, PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT id, name, endpoint_kind, method, path, request_body_json,
               enabled, built_in, note, created_at, updated_at
        FROM channel_monitor_request_templates WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(connection)
    .await?;
    row.map(row_to_template).ok_or(PersistenceError::NotFound)
}

async fn monitor_by_id(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<ChannelMonitor, PersistenceError> {
    let row = sqlx::query(
        r#"
        WITH latest_balance_spendability AS (
            SELECT * FROM (
                SELECT b.*, ROW_NUMBER() OVER (
                    PARTITION BY b.station_id, b.station_key_id, b.scope
                    ORDER BY b.updated_at DESC, b.created_at DESC, b.id DESC
                ) AS row_number
                FROM balance_snapshots b
            ) WHERE row_number = 1
        )
        SELECT m.id, m.name, m.target_type, m.station_id, m.station_key_id, m.template_id,
               m.enabled, m.pause_on_zero_balance,
               CASE
                   WHEN m.pause_on_zero_balance = 1 AND (
                       (m.target_type = 'station' AND EXISTS (
                           SELECT 1 FROM latest_balance_spendability b
                           WHERE b.station_id = m.station_id AND b.station_key_id IS NULL
                             AND b.scope IN ('station', 'station_account')
                             AND b.status IN ('depleted', 'exhausted', 'empty')
                             AND b.evidence_confidence = 'confirmed'
                             AND b.spendability_authority = 'authoritative'
                             AND (b.valid_until_ms IS NULL OR b.valid_until_ms >= CAST(strftime('%s','now') AS INTEGER) * 1000)
                       ))
                       OR (m.target_type = 'station_key' AND NOT EXISTS (
                           SELECT 1 FROM latest_balance_spendability b
                           WHERE b.station_id = m.station_id AND b.station_key_id IS NULL
                             AND b.scope IN ('station', 'station_account')
                             AND b.status IN ('normal', 'available', 'usable', 'low', 'warning')
                             AND b.evidence_confidence = 'confirmed'
                             AND b.spendability_authority = 'authoritative'
                             AND (b.valid_until_ms IS NULL OR b.valid_until_ms >= CAST(strftime('%s','now') AS INTEGER) * 1000)
                       ) AND EXISTS (
                           SELECT 1 FROM latest_balance_spendability b
                           WHERE ((b.station_id = m.station_id AND b.station_key_id IS NULL AND b.scope IN ('station', 'station_account'))
                              OR (b.station_key_id = m.station_key_id AND b.scope = 'station_key'))
                             AND b.status IN ('depleted', 'exhausted', 'empty')
                             AND b.evidence_confidence = 'confirmed'
                             AND b.spendability_authority = 'authoritative'
                             AND (b.valid_until_ms IS NULL OR b.valid_until_ms >= CAST(strftime('%s','now') AS INTEGER) * 1000)
                       ))
                   ) THEN 1 ELSE 0
               END AS balance_paused,
               m.interval_seconds, m.jitter_seconds, m.timeout_seconds,
               max_concurrency, consecutive_failure_threshold, fallback_models_json,
               protocol_kind, client_profile_id, client_profile_version, primary_model,
               fallback_models_v2_json, retry_max_attempts_per_model,
               retry_initial_backoff_ms, retry_max_backoff_ms, risk_daily_probe_budget,
               health_policy_mode, health_failure_threshold, health_recovery_threshold,
               attempt_timeout_ms, execution_timeout_ms, schedule_revision,
               note, created_at, updated_at
        FROM channel_monitors m WHERE m.id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(connection)
    .await?;
    row.map(row_to_monitor)
        .transpose()?
        .ok_or(PersistenceError::NotFound)
}

async fn validate_monitor_input(
    connection: &mut SqliteConnection,
    input: &CreateChannelMonitorInput,
) -> Result<(), PersistenceError> {
    validate_monitor_values(
        &input.name,
        &input.target_type,
        input.interval_seconds,
        input.jitter_seconds,
        input.timeout_seconds,
        input.max_concurrency,
        input.consecutive_failure_threshold,
    )?;
    validate_monitor_owners(
        connection,
        &input.station_id,
        input.station_key_id.as_deref(),
        &input.template_id,
        &input.target_type,
    )
    .await
}

async fn validate_monitor_update(
    connection: &mut SqliteConnection,
    input: &UpdateChannelMonitorInput,
) -> Result<(), PersistenceError> {
    validate_monitor_values(
        &input.name,
        &input.target_type,
        input.interval_seconds,
        input.jitter_seconds,
        input.timeout_seconds,
        input.max_concurrency,
        input.consecutive_failure_threshold,
    )?;
    validate_monitor_owners(
        connection,
        &input.station_id,
        input.station_key_id.as_deref(),
        &input.template_id,
        &input.target_type,
    )
    .await
}

async fn validate_monitor_owners(
    connection: &mut SqliteConnection,
    station_id: &str,
    station_key_id: Option<&str>,
    template_id: &str,
    target_type: &str,
) -> Result<(), PersistenceError> {
    let station_exists =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM stations WHERE id = ?1")
            .bind(station_id)
            .fetch_one(&mut *connection)
            .await?
            == 1;
    let template_enabled = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM channel_monitor_request_templates WHERE id = ?1 AND enabled = 1",
    )
    .bind(template_id)
    .fetch_one(&mut *connection)
    .await?
        == 1;
    if !station_exists || !template_enabled {
        return Err(PersistenceError::ConstraintViolation);
    }
    match (target_type.trim(), station_key_id) {
        ("station", None) => Ok(()),
        ("station_key", Some(station_key_id)) => {
            let owned = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM station_keys WHERE id = ?1 AND station_id = ?2",
            )
            .bind(station_key_id)
            .bind(station_id)
            .fetch_one(connection)
            .await?
                == 1;
            if owned {
                Ok(())
            } else {
                Err(PersistenceError::ConstraintViolation)
            }
        }
        _ => Err(PersistenceError::ConstraintViolation),
    }
}

fn validate_template(
    name: &str,
    endpoint_kind: &str,
    method: &str,
    path: &str,
    request_body_json: &str,
) -> Result<(), PersistenceError> {
    let body = serde_json::from_str::<serde_json::Value>(request_body_json)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    if name.trim().is_empty()
        || endpoint_kind.trim().is_empty()
        || method.trim().is_empty()
        || path.trim().is_empty()
        || !body.is_object()
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn validate_monitor_values(
    name: &str,
    target_type: &str,
    interval_seconds: i64,
    jitter_seconds: i64,
    timeout_seconds: i64,
    max_concurrency: i64,
    failure_threshold: i64,
) -> Result<(), PersistenceError> {
    if name.trim().is_empty()
        || !matches!(target_type.trim(), "station" | "station_key")
        || !(15..=3600).contains(&interval_seconds)
        || !(0..=600).contains(&jitter_seconds)
        || interval_seconds - jitter_seconds < 15
        || !(5..=120).contains(&timeout_seconds)
        || !(1..=16).contains(&max_concurrency)
        || !(1..=20).contains(&failure_threshold)
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn row_to_template(row: sqlx::sqlite::SqliteRow) -> ChannelMonitorRequestTemplate {
    ChannelMonitorRequestTemplate {
        id: row.get("id"),
        name: row.get("name"),
        endpoint_kind: row.get("endpoint_kind"),
        method: row.get("method"),
        path: row.get("path"),
        request_body_json: row.get("request_body_json"),
        enabled: i64_to_bool(row.get("enabled")),
        built_in: i64_to_bool(row.get("built_in")),
        note: row.get("note"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_monitor(row: sqlx::sqlite::SqliteRow) -> Result<ChannelMonitor, PersistenceError> {
    let fallback_models_json: String = row.get("fallback_models_v2_json");
    let fallback_models = serde_json::from_str(&fallback_models_json).map_err(|_| {
        PersistenceError::InvariantViolation("invalid monitor fallback models".into())
    })?;
    Ok(ChannelMonitor {
        id: row.get("id"),
        name: row.get("name"),
        target_type: row.get("target_type"),
        station_id: row.get("station_id"),
        station_key_id: row.get("station_key_id"),
        template_id: row.get("template_id"),
        enabled: i64_to_bool(row.get("enabled")),
        pause_on_zero_balance: i64_to_bool(row.get("pause_on_zero_balance")),
        balance_paused: i64_to_bool(row.get("balance_paused")),
        protocol_kind: row.get("protocol_kind"),
        client_profile_id: row.get("client_profile_id"),
        client_profile_version: row.get("client_profile_version"),
        primary_model: row.get("primary_model"),
        retry_max_attempts_per_model: row.get("retry_max_attempts_per_model"),
        retry_initial_backoff_ms: row.get("retry_initial_backoff_ms"),
        retry_max_backoff_ms: row.get("retry_max_backoff_ms"),
        risk_daily_probe_budget: row.get("risk_daily_probe_budget"),
        health_policy_mode: row.get("health_policy_mode"),
        health_failure_threshold: row.get("health_failure_threshold"),
        health_recovery_threshold: row.get("health_recovery_threshold"),
        attempt_timeout_ms: row.get("attempt_timeout_ms"),
        execution_timeout_ms: row.get("execution_timeout_ms"),
        schedule_revision: row.get("schedule_revision"),
        interval_seconds: row.get("interval_seconds"),
        jitter_seconds: row.get("jitter_seconds"),
        timeout_seconds: row.get("timeout_seconds"),
        max_concurrency: row.get("max_concurrency"),
        consecutive_failure_threshold: row.get("consecutive_failure_threshold"),
        fallback_models,
        note: row.get("note"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn serialize_fallback_models(values: &[String]) -> Result<String, PersistenceError> {
    let normalized = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    serde_json::to_string(&normalized).map_err(|_| PersistenceError::ConstraintViolation)
}

fn serialize_legacy_models(
    primary_model: &str,
    fallback_models: &[String],
) -> Result<String, PersistenceError> {
    let values = std::iter::once(primary_model.to_owned())
        .chain(fallback_models.iter().cloned())
        .collect::<Vec<_>>();
    serialize_fallback_models(&values)
}

fn parse_millis(value: &str) -> Option<i64> {
    value.trim().parse().ok()
}

fn normalize_optional(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn bool_to_i64(value: bool) -> i64 {
    i64::from(value)
}

fn i64_to_bool(value: i64) -> bool {
    value != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::runtime::PersistenceRuntime;

    #[tokio::test]
    async fn monitor_crud_persists_v2_definition_fields_and_revises_schedule() {
        let directory = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&directory.path().join("monitor-crud.sqlite3"))
                .await
                .expect("runtime");
        let handle = runtime.handle();
        handle
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        r#"
                        INSERT INTO stations (
                            id, name, station_type, website_url, api_base_url, enabled, priority,
                            credit_per_cny, collection_interval_minutes, status, created_at, updated_at
                        ) VALUES ('station-1', 'Station', 'openai-compatible', 'https://example.test',
                                  'https://example.test/v1', 1, 0, 1.0, 30, 'unchecked', '1', '1')
                        "#,
                    )
                    .execute(write.connection())
                    .await?;
                    sqlx::query("INSERT INTO station_keys (id, station_id) VALUES ('key-1', 'station-1')")
                        .execute(write.connection())
                        .await?;
                    sqlx::query(
                        r#"
                        INSERT INTO channel_monitor_request_templates (
                            id, name, endpoint_kind, method, path, request_body_json,
                            enabled, built_in, created_at, updated_at
                        ) VALUES ('template-1', 'Chat', 'chat_completions', 'POST',
                                  '/v1/chat/completions', '{}', 1, 0, '1', '1')
                        "#,
                    )
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("seed");

        let store = MonitoringStore;
        let created = handle
            .write(|write| {
                Box::pin(async move {
                    store
                        .insert_monitor(
                            write,
                            NewMonitorRow {
                                id: "monitor-1".into(),
                                now: "1000".into(),
                                next_run_at: Some("1000".into()),
                                input: monitor_input(),
                            },
                        )
                        .await
                })
            })
            .await
            .expect("create monitor");
        assert_eq!(created.protocol_kind, "anthropic_messages");
        assert_eq!(created.client_profile_id, "claude_code_compat");
        assert_eq!(created.primary_model, "claude-3-5-haiku");
        assert_eq!(created.fallback_models, vec!["claude-3-haiku"]);
        assert_eq!(created.retry_max_attempts_per_model, 2);
        assert_eq!(created.schedule_revision, 1);

        let mut update = monitor_input();
        update.protocol_kind = "gemini_native".into();
        update.client_profile_id = "gemini_cli_compat".into();
        update.primary_model = "gemini-2.0-flash".into();
        update.fallback_models = vec!["gemini-1.5-flash".into()];
        let updated = handle
            .write(|write| {
                Box::pin(async move {
                    store
                        .update_monitor(
                            write,
                            MonitorPatch {
                                now: "2000".into(),
                                input: UpdateChannelMonitorInput {
                                    id: "monitor-1".into(),
                                    name: update.name,
                                    target_type: update.target_type,
                                    station_id: update.station_id,
                                    station_key_id: update.station_key_id,
                                    template_id: update.template_id,
                                    enabled: update.enabled,
                                    pause_on_zero_balance: update.pause_on_zero_balance,
                                    protocol_kind: update.protocol_kind,
                                    client_profile_id: update.client_profile_id,
                                    client_profile_version: update.client_profile_version,
                                    primary_model: update.primary_model,
                                    retry_max_attempts_per_model: update
                                        .retry_max_attempts_per_model,
                                    retry_initial_backoff_ms: update.retry_initial_backoff_ms,
                                    retry_max_backoff_ms: update.retry_max_backoff_ms,
                                    risk_daily_probe_budget: update.risk_daily_probe_budget,
                                    health_policy_mode: update.health_policy_mode,
                                    health_failure_threshold: update.health_failure_threshold,
                                    health_recovery_threshold: update.health_recovery_threshold,
                                    attempt_timeout_ms: update.attempt_timeout_ms,
                                    execution_timeout_ms: update.execution_timeout_ms,
                                    interval_seconds: update.interval_seconds,
                                    jitter_seconds: update.jitter_seconds,
                                    timeout_seconds: update.timeout_seconds,
                                    max_concurrency: update.max_concurrency,
                                    consecutive_failure_threshold: update
                                        .consecutive_failure_threshold,
                                    fallback_models: update.fallback_models,
                                    note: update.note,
                                },
                            },
                        )
                        .await
                })
            })
            .await
            .expect("update monitor");
        assert_eq!(updated.protocol_kind, "gemini_native");
        assert_eq!(updated.client_profile_id, "gemini_cli_compat");
        assert_eq!(updated.primary_model, "gemini-2.0-flash");
        assert_eq!(updated.fallback_models, vec!["gemini-1.5-flash"]);
        assert_eq!(updated.schedule_revision, 2);

        let mut read = handle.begin_read().await.expect("read");
        let (legacy_models, v2_models, next_due_at_ms): (String, String, Option<i64>) =
            sqlx::query_as(
                "SELECT fallback_models_json, fallback_models_v2_json, next_due_at_ms FROM channel_monitors WHERE id = 'monitor-1'",
            )
            .fetch_one(read.connection())
            .await
            .expect("stored definition");
        assert_eq!(legacy_models, r#"["gemini-2.0-flash","gemini-1.5-flash"]"#);
        assert_eq!(v2_models, r#"["gemini-1.5-flash"]"#);
        assert_eq!(next_due_at_ms, Some(2000));
    }

    fn monitor_input() -> CreateChannelMonitorInput {
        CreateChannelMonitorInput {
            name: "Monitor".into(),
            target_type: "station_key".into(),
            station_id: "station-1".into(),
            station_key_id: Some("key-1".into()),
            template_id: "template-1".into(),
            enabled: true,
            pause_on_zero_balance: true,
            protocol_kind: "anthropic_messages".into(),
            client_profile_id: "claude_code_compat".into(),
            client_profile_version: 1,
            primary_model: "claude-3-5-haiku".into(),
            retry_max_attempts_per_model: 2,
            retry_initial_backoff_ms: 300,
            retry_max_backoff_ms: 1_200,
            risk_daily_probe_budget: 80,
            health_policy_mode: "observe_only".into(),
            health_failure_threshold: 3,
            health_recovery_threshold: 2,
            attempt_timeout_ms: 8_000,
            execution_timeout_ms: 30_000,
            interval_seconds: 300,
            jitter_seconds: 30,
            timeout_seconds: 30,
            max_concurrency: 1,
            consecutive_failure_threshold: 3,
            fallback_models: vec!["claude-3-haiku".into()],
            note: None,
        }
    }
}
