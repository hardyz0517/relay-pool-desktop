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
                key_revision.revision AS key_record_revision,
                station_revision.revision AS station_record_revision
            FROM station_keys k
            JOIN stations s ON s.id = k.station_id
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
            SELECT settings.key, settings.value, revision.revision AS record_revision
            FROM settings
            LEFT JOIN domain_revisions revision
                ON revision.scope = 'setting:' || settings.key
            WHERE key IN (
                'default_routing_strategy',
                'max_rate_multiplier',
                'default_routing_group_filter',
                'scheduler_advanced_settings_json',
                'allow_depleted_fallback'
            )
            ORDER BY key ASC
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

        sqlx::query(
            r#"
            SELECT station_key_id, supports_tools, supports_vision, supports_reasoning, updated_at
            FROM station_key_capabilities
            WHERE station_key_id IN (
                SELECT k.id
                FROM station_keys k
                JOIN stations s ON s.id = k.station_id
                WHERE k.enabled = 1 AND s.enabled = 1
            )
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        query_count += 1;

        sqlx::query(
            r#"
            SELECT station_key_id, endpoint_revision, consecutive_failures, success_count, failure_count, updated_at
            FROM station_key_health
            WHERE station_key_id IN (
                SELECT k.id
                FROM station_keys k
                JOIN stations s ON s.id = k.station_id
                WHERE k.enabled = 1 AND s.enabled = 1
            )
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        query_count += 1;

        sqlx::query(
            r#"
            SELECT station_id, endpoint_revision
            FROM station_endpoint_health
            WHERE station_id IN (
                SELECT s.id
                FROM stations s
                WHERE s.enabled = 1
            )
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        query_count += 1;

        sqlx::query(
            r#"
            SELECT station_id, station_key_id, scope, value, currency, low_balance_threshold, status, updated_at
            FROM balance_snapshots
            WHERE id IN (
                SELECT MAX(id)
                FROM balance_snapshots
                GROUP BY station_id, COALESCE(station_key_id, ''), scope
            )
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        query_count += 1;

        sqlx::query(
            r#"
            SELECT id, station_id, station_key_id, model, input_price, output_price,
                   fixed_price, rate_multiplier, currency, unit, confidence, updated_at
            FROM pricing_rules
            WHERE enabled = 1
              AND (?1 IS NULL OR model = ?1)
            ORDER BY station_id ASC, model ASC, updated_at DESC, id ASC
            "#,
        )
        .bind(options.requested_model())
        .fetch_all(read.connection())
        .await?;
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
