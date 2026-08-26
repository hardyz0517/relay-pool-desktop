use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DueMonitorRow {
    pub(crate) id: String,
    pub(crate) station_id: String,
    pub(crate) station_key_id: Option<String>,
    pub(crate) protocol_kind: String,
    pub(crate) client_profile_id: String,
    pub(crate) primary_model: String,
    pub(crate) next_due_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MonitorDefinitionConfigRow {
    pub(crate) id: String,
    pub(crate) target_type: String,
    pub(crate) station_id: String,
    pub(crate) station_key_id: Option<String>,
    pub(crate) protocol_kind: String,
    pub(crate) client_profile_id: String,
    pub(crate) client_profile_version: i64,
    pub(crate) primary_model: String,
    pub(crate) fallback_models_json: String,
    pub(crate) retry_max_attempts_per_model: i64,
    pub(crate) retry_initial_backoff_ms: i64,
    pub(crate) retry_max_backoff_ms: i64,
    pub(crate) risk_daily_probe_budget: i64,
    pub health_policy_mode: String,
    pub(crate) health_failure_threshold: i64,
    pub(crate) health_recovery_threshold: i64,
    pub(crate) interval_seconds: i64,
    pub(crate) jitter_seconds: i64,
    pub(crate) attempt_timeout_ms: i64,
    pub(crate) execution_timeout_ms: i64,
    pub(crate) proxy_mode: String,
    pub(crate) proxy_url: Option<String>,
    pub(crate) schedule_revision: i64,
    pub(crate) next_due_at_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MonitoringDefinitionRepository;

impl MonitoringDefinitionRepository {
    pub(crate) async fn list_due(
        &self,
        connection: &mut SqliteConnection,
        now_ms: i64,
        limit: u32,
    ) -> Result<Vec<DueMonitorRow>, PersistenceError> {
        let bounded_limit = i64::from(limit.clamp(1, 500));
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
            SELECT id, station_id, station_key_id, protocol_kind, client_profile_id,
                   primary_model, next_due_at_ms
            FROM channel_monitors
            WHERE enabled = 1
              AND NOT (
                  pause_on_zero_balance = 1
                  AND (
                      (target_type = 'station' AND EXISTS (
                          SELECT 1 FROM latest_balance_spendability b
                          WHERE b.station_id = channel_monitors.station_id
                            AND b.station_key_id IS NULL
                            AND b.scope IN ('station', 'station_account')
                            AND b.status IN ('depleted', 'exhausted', 'empty')
                            AND b.evidence_confidence = 'confirmed'
                            AND b.spendability_authority = 'authoritative'
                            AND (b.valid_until_ms IS NULL OR b.valid_until_ms >= ?1)
                      ))
                      OR (target_type = 'station_key' AND NOT EXISTS (
                          SELECT 1 FROM latest_balance_spendability b
                          WHERE b.station_id = channel_monitors.station_id
                            AND b.station_key_id IS NULL
                            AND b.scope IN ('station', 'station_account')
                            AND b.status IN ('normal', 'available', 'usable', 'low', 'warning')
                            AND b.evidence_confidence = 'confirmed'
                            AND b.spendability_authority = 'authoritative'
                            AND (b.valid_until_ms IS NULL OR b.valid_until_ms >= ?1)
                      ) AND EXISTS (
                          SELECT 1 FROM latest_balance_spendability b
                          WHERE ((b.station_id = channel_monitors.station_id
                                  AND b.station_key_id IS NULL
                                  AND b.scope IN ('station', 'station_account'))
                             OR (b.station_key_id = channel_monitors.station_key_id
                                 AND b.scope = 'station_key'))
                            AND b.status IN ('depleted', 'exhausted', 'empty')
                            AND b.evidence_confidence = 'confirmed'
                            AND b.spendability_authority = 'authoritative'
                            AND (b.valid_until_ms IS NULL OR b.valid_until_ms >= ?1)
                      ))
                  )
              )
              AND (next_due_at_ms IS NULL OR next_due_at_ms <= ?1)
            ORDER BY COALESCE(next_due_at_ms, 0) ASC, id ASC
            LIMIT ?2
            "#,
        )
        .bind(now_ms)
        .bind(bounded_limit)
        .fetch_all(connection)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| DueMonitorRow {
                id: row.get("id"),
                station_id: row.get("station_id"),
                station_key_id: row.get("station_key_id"),
                protocol_kind: row.get("protocol_kind"),
                client_profile_id: row.get("client_profile_id"),
                primary_model: row.get("primary_model"),
                next_due_at_ms: row.get("next_due_at_ms"),
            })
            .collect())
    }

    pub(crate) async fn next_due_at_ms(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<Option<i64>, PersistenceError> {
        sqlx::query_scalar(
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
            SELECT MIN(next_due_at_ms)
            FROM channel_monitors
            WHERE enabled = 1
              AND NOT (
                  pause_on_zero_balance = 1
                  AND (
                      (target_type = 'station' AND EXISTS (
                          SELECT 1 FROM latest_balance_spendability b
                          WHERE b.station_id = channel_monitors.station_id
                            AND b.station_key_id IS NULL
                            AND b.scope IN ('station', 'station_account')
                            AND b.status IN ('depleted', 'exhausted', 'empty')
                            AND b.evidence_confidence = 'confirmed'
                            AND b.spendability_authority = 'authoritative'
                            AND (b.valid_until_ms IS NULL OR b.valid_until_ms >= CAST(strftime('%s','now') AS INTEGER) * 1000)
                      ))
                      OR (target_type = 'station_key' AND NOT EXISTS (
                          SELECT 1 FROM latest_balance_spendability b
                          WHERE b.station_id = channel_monitors.station_id
                            AND b.station_key_id IS NULL
                            AND b.scope IN ('station', 'station_account')
                            AND b.status IN ('normal', 'available', 'usable', 'low', 'warning')
                            AND b.evidence_confidence = 'confirmed'
                            AND b.spendability_authority = 'authoritative'
                            AND (b.valid_until_ms IS NULL OR b.valid_until_ms >= CAST(strftime('%s','now') AS INTEGER) * 1000)
                      ) AND EXISTS (
                          SELECT 1 FROM latest_balance_spendability b
                          WHERE ((b.station_id = channel_monitors.station_id AND b.station_key_id IS NULL
                                  AND b.scope IN ('station', 'station_account'))
                             OR (b.station_key_id = channel_monitors.station_key_id AND b.scope = 'station_key'))
                            AND b.status IN ('depleted', 'exhausted', 'empty')
                            AND b.evidence_confidence = 'confirmed'
                            AND b.spendability_authority = 'authoritative'
                            AND (b.valid_until_ms IS NULL OR b.valid_until_ms >= CAST(strftime('%s','now') AS INTEGER) * 1000)
                      ))
                  )
              )
              AND next_due_at_ms IS NOT NULL
            "#,
        )
        .fetch_one(connection)
        .await
        .map_err(Into::into)
    }

    pub(crate) async fn load_config(
        &self,
        connection: &mut SqliteConnection,
        monitor_id: &str,
    ) -> Result<MonitorDefinitionConfigRow, PersistenceError> {
        let row = sqlx::query(
            r#"
            SELECT id, target_type, station_id, station_key_id, protocol_kind,
                   client_profile_id, client_profile_version, primary_model,
                   fallback_models_v2_json, retry_max_attempts_per_model,
                   retry_initial_backoff_ms, retry_max_backoff_ms,
                   risk_daily_probe_budget, health_policy_mode,
                   health_failure_threshold, health_recovery_threshold,
                   interval_seconds, jitter_seconds, attempt_timeout_ms,
                   execution_timeout_ms, schedule_revision, next_due_at_ms,
                   proxy_mode, proxy_url
            FROM channel_monitors
            WHERE id = ?1
            "#,
        )
        .bind(monitor_id)
        .fetch_one(connection)
        .await?;

        Ok(MonitorDefinitionConfigRow {
            id: row.get("id"),
            target_type: row.get("target_type"),
            station_id: row.get("station_id"),
            station_key_id: row.get("station_key_id"),
            protocol_kind: row.get("protocol_kind"),
            client_profile_id: row.get("client_profile_id"),
            client_profile_version: row.get("client_profile_version"),
            primary_model: row.get("primary_model"),
            fallback_models_json: row.get("fallback_models_v2_json"),
            retry_max_attempts_per_model: row.get("retry_max_attempts_per_model"),
            retry_initial_backoff_ms: row.get("retry_initial_backoff_ms"),
            retry_max_backoff_ms: row.get("retry_max_backoff_ms"),
            risk_daily_probe_budget: row.get("risk_daily_probe_budget"),
            health_policy_mode: row.get("health_policy_mode"),
            health_failure_threshold: row.get("health_failure_threshold"),
            health_recovery_threshold: row.get("health_recovery_threshold"),
            interval_seconds: row.get("interval_seconds"),
            jitter_seconds: row.get("jitter_seconds"),
            attempt_timeout_ms: row.get("attempt_timeout_ms"),
            execution_timeout_ms: row.get("execution_timeout_ms"),
            schedule_revision: row.get("schedule_revision"),
            next_due_at_ms: row.get("next_due_at_ms"),
            proxy_mode: row.get("proxy_mode"),
            proxy_url: row.get("proxy_url"),
        })
    }
}
