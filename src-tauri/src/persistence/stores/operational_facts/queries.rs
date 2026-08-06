use sqlx::Row;

use crate::{
    models::operational::{
        OperationalFactReadOptions, RawOperationalCandidateRow, RawOperationalFactRows,
        RawOperationalModelAliasRow, RawOperationalSettingRow,
    },
    persistence::{error::PersistenceError, ReadSession},
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum OperationalFactQueryError {
    #[error("operational candidate count {actual} exceeds limit {limit}")]
    CandidateLimitExceeded { actual: usize, limit: usize },
    #[error("routing revision is unavailable for scope {scope}")]
    RevisionUnavailable { scope: String },
    #[error("{0}")]
    Persistence(#[from] PersistenceError),
}

impl From<sqlx::Error> for OperationalFactQueryError {
    fn from(error: sqlx::Error) -> Self {
        Self::Persistence(PersistenceError::from(error))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OperationalFactStore;

impl OperationalFactStore {
    pub(crate) async fn load_raw(
        &self,
        read: &mut ReadSession,
        options: &OperationalFactReadOptions,
    ) -> Result<RawOperationalFactRows, OperationalFactQueryError> {
        let mut query_count = 0;
        let candidate_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM station_keys k
            JOIN stations s ON s.id = k.station_id
            WHERE k.enabled = 1
              AND s.enabled = 1
            "#,
        )
        .fetch_one(read.connection())
        .await?;
        query_count += 1;
        if candidate_count as usize > options.candidate_limit() {
            return Err(OperationalFactQueryError::CandidateLimitExceeded {
                actual: candidate_count as usize,
                limit: options.candidate_limit(),
            });
        }

        let candidate_rows = sqlx::query(
            r#"
            SELECT
                k.id AS station_key_id,
                k.station_id AS station_id,
                s.endpoint_revision AS endpoint_revision,
                s.api_base_url AS api_base_url,
                CASE
                    WHEN TRIM(k.api_key) != '' OR k.api_key_secret_id IS NOT NULL THEN 1
                    ELSE 0
                END AS credential_available,
                k.priority AS priority,
                COALESCE(c.only_use_as_backup, 0) AS backup_only,
                COALESCE(c.supports_chat_completions, 1) AS supports_chat_completions,
                COALESCE(c.supports_responses, 1) AS supports_responses,
                COALESCE(c.supports_stream, 1) AS supports_stream,
                COALESCE(c.supports_tools, 0) AS supports_tools,
                COALESCE(c.supports_vision, 0) AS supports_vision,
                COALESCE(c.supports_reasoning, 0) AS supports_reasoning,
                COALESCE(c.model_allowlist_json, '[]') AS model_allowlist_json,
                COALESCE(c.model_blocklist_json, '[]') AS model_blocklist_json,
                COALESCE(c.preferred_models_json, '[]') AS preferred_models_json,
                COALESCE(c.routing_tags_json, '[]') AS routing_tags_json,
                COALESCE(h.success_count, 0) AS success_count,
                COALESCE(h.failure_count, 0) AS failure_count,
                COALESCE(h.consecutive_failures, 0) AS consecutive_failures,
                h.avg_latency_ms AS avg_latency_ms,
                b.status AS balance_status,
                key_revision.revision AS key_record_revision,
                station_revision.revision AS station_record_revision
            FROM station_keys k
            JOIN stations s ON s.id = k.station_id
            LEFT JOIN station_key_capabilities c ON c.station_key_id = k.id
            LEFT JOIN routing_health_snapshot h
                ON h.station_key_id = k.id AND h.endpoint_revision = s.endpoint_revision
            LEFT JOIN balance_snapshots b ON b.id = (
                SELECT latest.id
                FROM balance_snapshots latest
                WHERE latest.station_key_id = k.id
                ORDER BY latest.updated_at DESC, latest.created_at DESC, latest.id DESC
                LIMIT 1
            )
            LEFT JOIN domain_revisions key_revision
                ON key_revision.scope = 'station_key:' || k.id
            LEFT JOIN domain_revisions station_revision
                ON station_revision.scope = 'station:' || s.id
            WHERE k.enabled = 1
              AND s.enabled = 1
            ORDER BY COALESCE(k.routing_order, k.priority) ASC,
                     k.priority ASC,
                     k.created_at ASC,
                     k.id ASC
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        query_count += 1;

        let settings_rows = sqlx::query(
            r#"
            SELECT 'routing_policy' AS key,
                   config_json AS value,
                   revisions.revision AS record_revision
            FROM routing_policy
            LEFT JOIN domain_revisions revisions
                ON revisions.scope = 'routing_policy'
            WHERE singleton_key = 1
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        query_count += 1;

        let alias_rows = if options.include_model_catalog() {
            sqlx::query(
                r#"
                SELECT model_aliases.client_model, model_aliases.upstream_model,
                       revision.revision AS record_revision, model_aliases.id AS alias_id
                FROM model_aliases
                LEFT JOIN domain_revisions revision
                    ON revision.scope = 'model_alias:' || model_aliases.id
                WHERE enabled = 1
                ORDER BY client_model ASC, upstream_model ASC, id ASC
                "#,
            )
            .fetch_all(read.connection())
            .await?
        } else if let Some(model) = options.requested_model() {
            sqlx::query(
                r#"
                SELECT model_aliases.client_model, model_aliases.upstream_model,
                       revision.revision AS record_revision, model_aliases.id AS alias_id
                FROM model_aliases
                LEFT JOIN domain_revisions revision
                    ON revision.scope = 'model_alias:' || model_aliases.id
                WHERE enabled = 1
                  AND (client_model = ?1 OR upstream_model = ?1)
                ORDER BY client_model ASC, upstream_model ASC, id ASC
                "#,
            )
            .bind(model)
            .fetch_all(read.connection())
            .await?
        } else {
            Vec::new()
        };
        query_count += 1;

        let candidates = candidate_rows
            .into_iter()
            .map(|row| {
                Ok(RawOperationalCandidateRow {
                    station_key_id: row.get("station_key_id"),
                    station_id: row.get("station_id"),
                    endpoint_revision: row.get("endpoint_revision"),
                    api_base_url: row.get("api_base_url"),
                    credential_available: row.get::<i64, _>("credential_available") != 0,
                    priority: row.get("priority"),
                    backup_only: row.get::<i64, _>("backup_only") != 0,
                    supports_chat_completions: row.get::<i64, _>("supports_chat_completions") != 0,
                    supports_responses: row.get::<i64, _>("supports_responses") != 0,
                    supports_stream: row.get::<i64, _>("supports_stream") != 0,
                    supports_tools: row.get::<i64, _>("supports_tools") != 0,
                    supports_vision: row.get::<i64, _>("supports_vision") != 0,
                    supports_reasoning: row.get::<i64, _>("supports_reasoning") != 0,
                    model_allowlist_json: row.get("model_allowlist_json"),
                    model_blocklist_json: row.get("model_blocklist_json"),
                    preferred_models_json: row.get("preferred_models_json"),
                    routing_tags_json: row.get("routing_tags_json"),
                    success_count: row.get("success_count"),
                    failure_count: row.get("failure_count"),
                    consecutive_failures: row.get("consecutive_failures"),
                    avg_latency_ms: row.get("avg_latency_ms"),
                    balance_status: row.get("balance_status"),
                    key_record_revision: required_revision(
                        row.get("key_record_revision"),
                        format!("station_key:{}", row.get::<String, _>("station_key_id")),
                    )?,
                    station_record_revision: required_revision(
                        row.get("station_record_revision"),
                        format!("station:{}", row.get::<String, _>("station_id")),
                    )?,
                })
            })
            .collect::<Result<Vec<_>, OperationalFactQueryError>>()?;
        let settings = settings_rows
            .into_iter()
            .map(|row| {
                Ok(RawOperationalSettingRow {
                    key: row.get("key"),
                    value: row.get("value"),
                    record_revision: required_revision(
                        row.get("record_revision"),
                        format!("setting:{}", row.get::<String, _>("key")),
                    )?,
                })
            })
            .collect::<Result<Vec<_>, OperationalFactQueryError>>()?;
        let model_aliases = alias_rows
            .into_iter()
            .map(|row| {
                Ok(RawOperationalModelAliasRow {
                    client_model: row.get("client_model"),
                    upstream_model: row.get("upstream_model"),
                    record_revision: required_revision(
                        row.get("record_revision"),
                        format!("model_alias:{}", row.get::<String, _>("alias_id")),
                    )?,
                })
            })
            .collect::<Result<Vec<_>, OperationalFactQueryError>>()?;

        Ok(RawOperationalFactRows {
            candidates,
            settings,
            model_aliases,
            query_count,
            loaded_full_model_catalog: options.include_model_catalog(),
        })
    }
}

fn required_revision(value: Option<i64>, scope: String) -> Result<i64, OperationalFactQueryError> {
    value
        .filter(|revision| *revision > 0)
        .ok_or(OperationalFactQueryError::RevisionUnavailable { scope })
}
