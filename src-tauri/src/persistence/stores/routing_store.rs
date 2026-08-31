use std::collections::HashMap;

use sqlx::{QueryBuilder, Row, Sqlite};

use crate::{
    models::{
        pricing::BalanceSnapshot,
        proxy::UpstreamApiFormat,
        routing::{
            CanonicalRoutingCandidate, ModelAlias, RuntimeRoutingBalance,
            RuntimeRoutingEconomicSnapshot, RuntimeRoutingSecret, RuntimeRoutingSettings,
            StationKeyCapabilities,
        },
        routing_policy::RoutingPolicyConfigV2,
        stations::StationEndpointHealth,
    },
    persistence::{
        error::PersistenceError, read_session::ReadSession, write_session::WriteSession,
    },
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RoutingStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StationEndpointProbeTarget {
    pub(crate) station_id: String,
    pub(crate) api_base_url: String,
    pub(crate) endpoint_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationalExecutionTargetRefRow {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) station_type: String,
    pub(crate) group_binding_id: Option<String>,
    pub(crate) endpoint_revision: i64,
    pub(crate) credential_revision: i64,
    pub(crate) account_revision: i64,
    pub(crate) group_revision: Option<i64>,
    pub(crate) api_base_url: String,
    pub(crate) upstream_api_format: UpstreamApiFormat,
    pub(crate) collector_proxy_mode: String,
    pub(crate) collector_proxy_url: Option<String>,
    pub(crate) key_enabled: bool,
    pub(crate) station_enabled: bool,
    pub(crate) api_key_secret_id: Option<String>,
    pub(crate) api_key_secret_scope: Option<String>,
    pub(crate) api_key_secret_owner_id: Option<String>,
    pub(crate) api_key_secret_kind: Option<String>,
    pub(crate) inline_api_key_present: bool,
    pub(crate) station_account_max_concurrency: u32,
    pub(crate) station_key_max_concurrency: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationalMonitoringTargetSnapshotRow {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) station_key_lifecycle_revision: u64,
    pub(crate) endpoint_revision: i64,
    pub(crate) api_base_url: String,
    pub(crate) upstream_api_format: UpstreamApiFormat,
    pub(crate) supports_chat_completions: bool,
    pub(crate) supports_responses: bool,
}

struct RankedRuntimeBalance {
    balance: RuntimeRoutingBalance,
    updated_at: String,
    created_at: String,
    id: String,
}

const OPERATIONAL_EXECUTION_TARGET_REFS_QUERY_PREFIX: &str = r#"
    SELECT
        k.id AS station_key_id,
        k.station_id,
        s.station_type,
        k.group_binding_id,
        s.endpoint_revision,
        COALESCE(key_revision.revision, 0) AS credential_revision,
        COALESCE(account_revision.revision, 0) AS account_revision,
        group_revision.revision AS group_revision,
        s.api_base_url,
        s.upstream_api_format,
        s.collector_proxy_mode,
        s.collector_proxy_url,
        CASE
            WHEN LOWER(s.station_type) IN ('sub2api', 'newapi') THEN COALESCE((
                SELECT b.account_concurrency_limit
                FROM balance_snapshots b
                WHERE b.station_id = s.id
                  AND b.station_key_id IS NULL
                  AND b.account_concurrency_limit > 0
                ORDER BY b.updated_at DESC, b.created_at DESC, b.id DESC
                LIMIT 1
            ), 0)
            ELSE 0
        END AS station_account_max_concurrency,
        CASE
            WHEN LOWER(s.station_type) IN ('sub2api', 'newapi') THEN 0
            ELSE MAX(k.max_concurrency, 0)
        END AS station_key_max_concurrency,
        k.enabled AS key_enabled,
        s.enabled AS station_enabled,
        k.api_key_secret_id,
        sec.scope AS api_key_secret_scope,
        sec.owner_id AS api_key_secret_owner_id,
        sec.kind AS api_key_secret_kind,
        CASE WHEN TRIM(k.api_key) != '' THEN 1 ELSE 0 END AS inline_api_key_present
    FROM station_keys k
    JOIN stations s ON s.id = k.station_id
    LEFT JOIN domain_revisions key_revision ON key_revision.scope = 'station_key:' || k.id
    LEFT JOIN domain_revisions account_revision ON account_revision.scope = 'station_account:' || s.id
    LEFT JOIN domain_revisions group_revision ON group_revision.scope = 'station_group:' || k.group_binding_id
    LEFT JOIN secrets sec ON sec.id = k.api_key_secret_id
    WHERE k.id IN (
"#;

impl RoutingStore {
    pub(crate) async fn load_execution_settings(
        &self,
        read: &mut ReadSession,
    ) -> Result<RuntimeRoutingSettings, PersistenceError> {
        let config_json = sqlx::query_scalar::<_, String>(
            "SELECT config_json FROM routing_policy WHERE singleton_key = 1",
        )
        .fetch_optional(read.connection())
        .await?
        .ok_or(PersistenceError::NotFound)?;
        let config = serde_json::from_str::<serde_json::Value>(&config_json)
            .map_err(|_| PersistenceError::InvariantViolation("invalid routing policy".into()))?;
        let config = RoutingPolicyConfigV2::from_stored_value(&config)
            .map_err(|_| PersistenceError::InvariantViolation("invalid routing policy".into()))?;
        let global_proxy_mode = sqlx::query_scalar::<_, String>(
            "SELECT value FROM settings WHERE key = 'collector_proxy_mode'",
        )
        .fetch_optional(read.connection())
        .await?
        .unwrap_or_else(|| "direct".to_string());
        let global_proxy_url = sqlx::query_scalar::<_, String>(
            "SELECT value FROM settings WHERE key = 'collector_proxy_url'",
        )
        .fetch_optional(read.connection())
        .await?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
        Ok(RuntimeRoutingSettings {
            max_rate_multiplier: config.max_rate_multiplier,
            routing_group_scope: config.routing_group_filter,
            allow_depleted_fallback: config.allow_depleted_fallback,
            outbound_proxy_mode: config.outbound_proxy_mode,
            outbound_proxy_url: config.outbound_proxy_url,
            global_proxy_mode,
            global_proxy_url,
        })
    }

    pub(crate) async fn load_runtime_candidates(
        &self,
        read: &mut ReadSession,
    ) -> Result<Vec<CanonicalRoutingCandidate>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT
                k.id AS station_key_id,
                k.station_id,
                s.station_type,
                s.credit_per_cny AS station_credit_per_cny,
                s.endpoint_revision,
                s.api_base_url,
                s.upstream_api_format,
                k.routing_order,
                k.priority,
                k.max_concurrency,
                k.load_factor,
                k.schedulable,
                s.collector_proxy_mode,
                s.collector_proxy_url,
                s.name AS station_name,
                k.name AS key_name,
                k.api_key,
                k.group_name,
                k.group_binding_id,
                k.group_id_hash,
                k.rate_multiplier,
                k.manual_rate_multiplier,
                k.manual_rate_updated_at,
                k.rate_source,
                k.rate_collected_at,
                k.updated_at AS key_updated_at,
                b.group_key_hash AS binding_group_key_hash,
                b.group_id_hash AS binding_group_id_hash,
                b.group_name AS binding_group_name,
                b.binding_status,
                COALESCE(
                    NULLIF(TRIM(b.group_category_override), ''),
                    NULLIF(TRIM(b.inferred_group_category), '')
                ) AS binding_group_category,
                b.effective_rate_multiplier AS binding_effective_rate_multiplier,
                b.confidence AS binding_confidence,
                b.last_checked_at AS binding_last_checked_at
            FROM station_keys k
            JOIN stations s ON s.id = k.station_id
            LEFT JOIN station_group_bindings b ON b.id = k.group_binding_id
            WHERE k.enabled = 1
              AND s.enabled = 1
              AND (TRIM(k.api_key) != '' OR k.api_key_secret_id IS NOT NULL)
            ORDER BY COALESCE(k.routing_order, k.priority) ASC,
                     k.priority ASC,
                     k.created_at ASC,
                     k.id ASC
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        let candidates = rows
            .into_iter()
            .map(row_to_runtime_candidate)
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        // All association reads stay in this ReadSession, so the assembled
        // snapshot is consistent without a wide, multiplicative join.
        let mut secrets = load_runtime_secrets(read).await?;
        let mut capabilities = load_runtime_capabilities(read).await?;
        let mut key_balances = load_latest_key_balances(read).await?;
        let station_balances = load_latest_station_balances(read).await?;
        let station_concurrency_limits = load_latest_station_concurrency_limits(read).await?;

        Ok(candidates
            .into_iter()
            .map(|mut candidate| {
                candidate.api_key_secret = secrets.remove(&candidate.station_key_id);
                if let Some(value) = capabilities.remove(&candidate.station_key_id) {
                    candidate.capabilities = value;
                }
                candidate.station_account_concurrency_limit = station_concurrency_limits
                    .get(&candidate.station_id)
                    .copied();
                candidate.balance_snapshot = newest_balance(
                    key_balances.remove(&candidate.station_key_id),
                    station_balances.get(&candidate.station_id),
                );
                candidate
            })
            .collect())
    }

    pub(crate) async fn load_operational_execution_target_refs(
        &self,
        read: &mut ReadSession,
        station_key_ids: &[String],
    ) -> Result<Vec<OperationalExecutionTargetRefRow>, PersistenceError> {
        if station_key_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut query = QueryBuilder::<Sqlite>::new(OPERATIONAL_EXECUTION_TARGET_REFS_QUERY_PREFIX);
        let mut separated = query.separated(", ");
        for station_key_id in station_key_ids {
            separated.push_bind(station_key_id);
        }
        separated.push_unseparated(") ORDER BY k.id ASC");

        let rows = query.build().fetch_all(read.connection()).await?;
        Ok(rows
            .into_iter()
            .map(row_to_operational_execution_target_ref)
            .collect())
    }

    pub(crate) async fn load_operational_monitoring_target_snapshots(
        &self,
        read: &mut ReadSession,
    ) -> Result<Vec<OperationalMonitoringTargetSnapshotRow>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT
                k.id AS station_key_id,
                k.station_id,
                key_revision.revision AS station_key_lifecycle_revision,
                s.endpoint_revision,
                s.api_base_url,
                s.upstream_api_format,
                c.supports_chat_completions,
                c.supports_responses
            FROM station_keys k
            JOIN stations s ON s.id = k.station_id
            JOIN domain_revisions key_revision
              ON key_revision.scope = 'station_key:' || k.id
            LEFT JOIN station_key_capabilities c ON c.station_key_id = k.id
            WHERE k.enabled = 1
              AND s.enabled = 1
              AND key_revision.revision > 0
              AND (TRIM(k.api_key) != '' OR k.api_key_secret_id IS NOT NULL)
            ORDER BY k.id ASC
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        Ok(rows
            .into_iter()
            .map(row_to_operational_monitoring_target_snapshot)
            .collect())
    }

    #[cfg(test)]
    pub(crate) async fn list_model_alias_pairs(
        &self,
        read: &mut ReadSession,
    ) -> Result<Vec<(String, String)>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT client_model, upstream_model
            FROM model_aliases
            WHERE enabled = 1
            ORDER BY created_at ASC, id ASC
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("client_model"), row.get("upstream_model")))
            .collect())
    }

    pub(crate) async fn list_model_aliases(
        &self,
        read: &mut ReadSession,
    ) -> Result<Vec<ModelAlias>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT id, client_model, upstream_model, enabled, note, created_at, updated_at
            FROM model_aliases
            ORDER BY client_model ASC, upstream_model ASC, id ASC
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        Ok(rows.into_iter().map(row_to_model_alias).collect())
    }

    pub(crate) async fn list_balance_snapshots(
        &self,
        read: &mut ReadSession,
    ) -> Result<Vec<BalanceSnapshot>, PersistenceError> {
        let rows = sqlx::query(&balance_snapshot_select_sql(
            "ORDER BY updated_at DESC, created_at DESC, id DESC",
        ))
        .fetch_all(read.connection())
        .await?;
        Ok(rows.into_iter().map(row_to_balance_snapshot).collect())
    }

    pub(crate) async fn list_balance_snapshots_for_station(
        &self,
        read: &mut ReadSession,
        station_id: &str,
    ) -> Result<Vec<BalanceSnapshot>, PersistenceError> {
        let rows = sqlx::query(&balance_snapshot_select_sql(
            "WHERE station_id = ?1 ORDER BY updated_at DESC, created_at DESC, id DESC",
        ))
        .bind(station_id)
        .fetch_all(read.connection())
        .await?;
        Ok(rows.into_iter().map(row_to_balance_snapshot).collect())
    }

    pub(crate) async fn list_station_endpoint_health(
        &self,
        read: &mut ReadSession,
    ) -> Result<Vec<StationEndpointHealth>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT h.station_id, h.endpoint_revision, h.status, h.latency_ms,
                   h.checked_at, h.error_summary, h.updated_at
            FROM endpoint_health_snapshot h
            JOIN stations s ON s.id = h.station_id
            WHERE h.endpoint_revision = s.endpoint_revision
            ORDER BY h.updated_at DESC, h.station_id ASC
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        Ok(rows
            .into_iter()
            .map(row_to_station_endpoint_health)
            .collect())
    }

    pub(crate) async fn station_endpoint_probe_target(
        &self,
        read: &mut ReadSession,
        station_id: &str,
    ) -> Result<StationEndpointProbeTarget, PersistenceError> {
        let row =
            sqlx::query("SELECT id, api_base_url, endpoint_revision FROM stations WHERE id = ?1")
                .bind(station_id)
                .fetch_optional(read.connection())
                .await?
                .ok_or(PersistenceError::NotFound)?;
        Ok(StationEndpointProbeTarget {
            station_id: row.get("id"),
            api_base_url: row.get("api_base_url"),
            endpoint_revision: row.get("endpoint_revision"),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn record_station_endpoint_health(
        &self,
        write: &mut WriteSession,
        station_id: &str,
        expected_endpoint_revision: i64,
        status: &str,
        latency_ms: Option<i64>,
        checked_at: &str,
        error_summary: Option<&str>,
        updated_at: &str,
    ) -> Result<StationEndpointHealth, PersistenceError> {
        if !matches!(status, "unchecked" | "success" | "failed")
            || latency_ms.is_some_and(|latency| latency < 0)
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        assert_station_endpoint_revision(write, station_id, expected_endpoint_revision).await?;
        sqlx::query(
            r#"
            INSERT INTO endpoint_health_snapshot (
                station_id, endpoint_revision, status, latency_ms, checked_at,
                error_summary, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(station_id) DO UPDATE SET
                endpoint_revision = excluded.endpoint_revision,
                status = excluded.status,
                latency_ms = excluded.latency_ms,
                checked_at = excluded.checked_at,
                error_summary = excluded.error_summary,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(station_id)
        .bind(expected_endpoint_revision)
        .bind(status)
        .bind(latency_ms)
        .bind(checked_at)
        .bind(error_summary)
        .bind(updated_at)
        .execute(write.connection())
        .await?;
        station_endpoint_health_by_id(write, station_id, expected_endpoint_revision).await
    }
}

async fn load_runtime_secrets(
    read: &mut ReadSession,
) -> Result<HashMap<String, RuntimeRoutingSecret>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        SELECT k.id, sec.id, sec.scope, sec.owner_id, sec.kind,
               sec.masked_value, sec.ciphertext, sec.nonce,
               sec.encryption_version
        FROM station_keys k
        JOIN stations s ON s.id = k.station_id
        JOIN secrets sec ON sec.id = k.api_key_secret_id
        WHERE k.enabled = 1
          AND s.enabled = 1
          AND (TRIM(k.api_key) != '' OR k.api_key_secret_id IS NOT NULL)
        "#,
    )
    .fetch_all(read.connection())
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get(0),
                RuntimeRoutingSecret {
                    id: row.get(1),
                    scope: row.get(2),
                    owner_id: row.get(3),
                    kind: row.get(4),
                    masked_value: row.get(5),
                    ciphertext: row.get(6),
                    nonce: row.get(7),
                    encryption_version: row.get::<i64, _>(8) as u16,
                },
            )
        })
        .collect())
}

async fn load_runtime_capabilities(
    read: &mut ReadSession,
) -> Result<HashMap<String, StationKeyCapabilities>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        SELECT c.station_key_id, c.supports_chat_completions, c.supports_responses,
               c.supports_embeddings, c.supports_stream, c.supports_tools,
               c.supports_vision, c.supports_reasoning, c.model_allowlist_json,
               c.model_blocklist_json, c.preferred_models_json, c.only_use_as_backup,
               c.routing_tags_json, c.updated_at
        FROM station_key_capabilities c
        JOIN station_keys k ON k.id = c.station_key_id
        JOIN stations s ON s.id = k.station_id
        WHERE k.enabled = 1
          AND s.enabled = 1
          AND (TRIM(k.api_key) != '' OR k.api_key_secret_id IS NOT NULL)
        "#,
    )
    .fetch_all(read.connection())
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let station_key_id: String = row.get(0);
            (
                station_key_id.clone(),
                StationKeyCapabilities {
                    station_key_id,
                    supports_chat_completions: i64_to_bool(row.get(1)),
                    supports_responses: i64_to_bool(row.get(2)),
                    supports_embeddings: i64_to_bool(row.get(3)),
                    supports_stream: i64_to_bool(row.get(4)),
                    supports_tools: i64_to_bool(row.get(5)),
                    supports_vision: i64_to_bool(row.get(6)),
                    supports_reasoning: i64_to_bool(row.get(7)),
                    model_allowlist: parse_json_string_list(row.get(8)),
                    model_blocklist: parse_json_string_list(row.get(9)),
                    preferred_models: parse_json_string_list(row.get(10)),
                    only_use_as_backup: i64_to_bool(row.get(11)),
                    routing_tags: parse_json_string_list(row.get(12)),
                    updated_at: row.get(13),
                },
            )
        })
        .collect())
}

async fn load_latest_key_balances(
    read: &mut ReadSession,
) -> Result<HashMap<String, RankedRuntimeBalance>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        WITH ranked AS (
            SELECT b.station_key_id, b.scope, b.value, b.currency,
                   b.low_balance_threshold, b.status, b.collected_at,
                   b.updated_at, b.created_at, b.id,
                   ROW_NUMBER() OVER (
                       PARTITION BY b.station_key_id
                       ORDER BY b.updated_at DESC, b.created_at DESC, b.id DESC
                   ) AS row_number
            FROM balance_snapshots b
            JOIN station_keys k ON k.id = b.station_key_id
            JOIN stations s ON s.id = k.station_id
            WHERE k.enabled = 1
              AND s.enabled = 1
              AND (TRIM(k.api_key) != '' OR k.api_key_secret_id IS NOT NULL)
        )
        SELECT station_key_id, scope, value, currency, low_balance_threshold,
               status, collected_at, updated_at, created_at, id
        FROM ranked
        WHERE row_number = 1
        "#,
    )
    .fetch_all(read.connection())
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get(0), row_to_ranked_runtime_balance(&row, 1)))
        .collect())
}

async fn load_latest_station_balances(
    read: &mut ReadSession,
) -> Result<HashMap<String, RankedRuntimeBalance>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        WITH eligible_stations AS (
            SELECT DISTINCT k.station_id
            FROM station_keys k
            JOIN stations s ON s.id = k.station_id
            WHERE k.enabled = 1
              AND s.enabled = 1
              AND (TRIM(k.api_key) != '' OR k.api_key_secret_id IS NOT NULL)
        ), ranked AS (
            SELECT b.station_id, b.scope, b.value, b.currency,
                   b.low_balance_threshold, b.status, b.collected_at,
                   b.updated_at, b.created_at, b.id,
                   ROW_NUMBER() OVER (
                       PARTITION BY b.station_id
                       ORDER BY b.updated_at DESC, b.created_at DESC, b.id DESC
                   ) AS row_number
            FROM balance_snapshots b
            JOIN eligible_stations e ON e.station_id = b.station_id
            WHERE b.station_key_id IS NULL
              AND b.scope = 'station'
        )
        SELECT station_id, scope, value, currency, low_balance_threshold,
               status, collected_at, updated_at, created_at, id
        FROM ranked
        WHERE row_number = 1
        "#,
    )
    .fetch_all(read.connection())
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get(0), row_to_ranked_runtime_balance(&row, 1)))
        .collect())
}

async fn load_latest_station_concurrency_limits(
    read: &mut ReadSession,
) -> Result<HashMap<String, i64>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        WITH eligible_stations AS (
            SELECT DISTINCT k.station_id
            FROM station_keys k
            JOIN stations s ON s.id = k.station_id
            WHERE k.enabled = 1
              AND s.enabled = 1
              AND (TRIM(k.api_key) != '' OR k.api_key_secret_id IS NOT NULL)
        ), ranked AS (
            SELECT b.station_id, b.account_concurrency_limit,
                   ROW_NUMBER() OVER (
                       PARTITION BY b.station_id
                       ORDER BY b.updated_at DESC, b.created_at DESC, b.id DESC
                   ) AS row_number
            FROM balance_snapshots b
            JOIN eligible_stations e ON e.station_id = b.station_id
            WHERE b.account_concurrency_limit > 0
        )
        SELECT station_id, account_concurrency_limit
        FROM ranked
        WHERE row_number = 1
        "#,
    )
    .fetch_all(read.connection())
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect())
}

async fn assert_station_endpoint_revision(
    write: &mut WriteSession,
    station_id: &str,
    expected_endpoint_revision: i64,
) -> Result<(), PersistenceError> {
    let revision =
        sqlx::query_scalar::<_, i64>("SELECT endpoint_revision FROM stations WHERE id = ?1")
            .bind(station_id)
            .fetch_optional(write.connection())
            .await?
            .ok_or(PersistenceError::NotFound)?;
    if revision != expected_endpoint_revision {
        return Err(PersistenceError::StaleRevision);
    }
    Ok(())
}

async fn station_endpoint_health_by_id(
    write: &mut WriteSession,
    station_id: &str,
    endpoint_revision: i64,
) -> Result<StationEndpointHealth, PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT station_id, endpoint_revision, status, latency_ms, checked_at,
               error_summary, updated_at
        FROM endpoint_health_snapshot
        WHERE station_id = ?1 AND endpoint_revision = ?2
        "#,
    )
    .bind(station_id)
    .bind(endpoint_revision)
    .fetch_optional(write.connection())
    .await?
    .ok_or(PersistenceError::NotFound)?;
    Ok(row_to_station_endpoint_health(row))
}

fn row_to_station_endpoint_health(row: sqlx::sqlite::SqliteRow) -> StationEndpointHealth {
    StationEndpointHealth {
        station_id: row.get("station_id"),
        endpoint_revision: row.get("endpoint_revision"),
        status: row.get("status"),
        latency_ms: row.get("latency_ms"),
        checked_at: row.get("checked_at"),
        error_summary: row.get("error_summary"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_runtime_candidate(row: sqlx::sqlite::SqliteRow) -> CanonicalRoutingCandidate {
    let station_key_id: String = row.get(runtime_candidate_column::STATION_KEY_ID);
    CanonicalRoutingCandidate {
        station_key_id: station_key_id.clone(),
        station_id: row.get(runtime_candidate_column::STATION_ID),
        station_type: row.get(runtime_candidate_column::STATION_TYPE),
        station_account_concurrency_limit: None,
        station_endpoint_revision: row.get(runtime_candidate_column::ENDPOINT_REVISION),
        sanitized_origin: crate::models::station_endpoints::sanitized_api_base_url_for_trace(
            &row.get::<String, _>(runtime_candidate_column::API_BASE_URL),
        ),
        upstream_api_format: parse_upstream_api_format(
            row.get::<String, _>(runtime_candidate_column::UPSTREAM_API_FORMAT),
        ),
        routing_order: row.get(runtime_candidate_column::ROUTING_ORDER),
        priority: row.get(runtime_candidate_column::PRIORITY),
        max_concurrency: row.get(runtime_candidate_column::MAX_CONCURRENCY),
        load_factor: row.get(runtime_candidate_column::LOAD_FACTOR),
        schedulable: i64_to_bool(row.get(runtime_candidate_column::SCHEDULABLE)),
        collector_proxy_mode: row.get(runtime_candidate_column::COLLECTOR_PROXY_MODE),
        collector_proxy_url: row.get(runtime_candidate_column::COLLECTOR_PROXY_URL),
        station_name: row.get(runtime_candidate_column::STATION_NAME),
        key_name: row.get(runtime_candidate_column::KEY_NAME),
        capabilities: default_runtime_capabilities(&station_key_id),
        #[cfg(test)]
        health: None,
        balance_snapshot: None,
        economic_snapshot: Some(row_to_runtime_economic_snapshot(&row)),
        api_key: row
            .get::<String, _>(runtime_candidate_column::API_KEY)
            .trim()
            .to_string()
            .into_non_empty(),
        api_key_secret: None,
    }
}

fn row_to_runtime_economic_snapshot(
    row: &sqlx::sqlite::SqliteRow,
) -> RuntimeRoutingEconomicSnapshot {
    let group_binding_id = row.get::<Option<String>, _>(runtime_candidate_column::GROUP_BINDING_ID);
    let group_key_hash =
        row.get::<Option<String>, _>(runtime_candidate_column::BINDING_GROUP_KEY_HASH);
    let group_id_hash = row
        .get::<Option<String>, _>(runtime_candidate_column::GROUP_ID_HASH)
        .or_else(|| row.get(runtime_candidate_column::BINDING_GROUP_ID_HASH));
    let group_name = row
        .get::<Option<String>, _>(runtime_candidate_column::BINDING_GROUP_NAME)
        .or_else(|| row.get(runtime_candidate_column::GROUP_NAME));
    let binding_effective_rate_multiplier =
        row.get::<Option<f64>, _>(runtime_candidate_column::BINDING_EFFECTIVE_RATE_MULTIPLIER);
    let key_rate_multiplier = row.get::<Option<f64>, _>(runtime_candidate_column::RATE_MULTIPLIER);
    RuntimeRoutingEconomicSnapshot {
        credit_per_cny: row.get(runtime_candidate_column::STATION_CREDIT_PER_CNY),
        group_binding_id,
        group_key_hash,
        group_id_hash,
        group_name,
        group_category: row.get(runtime_candidate_column::BINDING_GROUP_CATEGORY),
        group_status: row.get(runtime_candidate_column::BINDING_STATUS),
        group_confidence: row.get(runtime_candidate_column::BINDING_CONFIDENCE),
        group_checked_at: row.get(runtime_candidate_column::BINDING_LAST_CHECKED_AT),
        // Keep the key-level multiplier in the runtime candidate projection.
        // It is the normalized value consumed by routing fallback/projections;
        // request-scoped pricing resolves the current group rate separately.
        rate_multiplier: key_rate_multiplier.or(binding_effective_rate_multiplier),
        manual_rate_multiplier: row.get(runtime_candidate_column::MANUAL_RATE_MULTIPLIER),
        manual_rate_updated_at: row.get(runtime_candidate_column::MANUAL_RATE_UPDATED_AT),
        rate_source: row.get(runtime_candidate_column::RATE_SOURCE),
        rate_collected_at: row
            .get::<Option<String>, _>(runtime_candidate_column::RATE_COLLECTED_AT)
            .or_else(|| row.get(runtime_candidate_column::BINDING_LAST_CHECKED_AT)),
        key_updated_at: row.get(runtime_candidate_column::KEY_UPDATED_AT),
    }
}

fn row_to_operational_execution_target_ref(
    row: sqlx::sqlite::SqliteRow,
) -> OperationalExecutionTargetRefRow {
    OperationalExecutionTargetRefRow {
        station_key_id: row.get("station_key_id"),
        station_id: row.get("station_id"),
        station_type: row.get("station_type"),
        group_binding_id: row.get("group_binding_id"),
        endpoint_revision: row.get("endpoint_revision"),
        credential_revision: row.get("credential_revision"),
        account_revision: row.get("account_revision"),
        group_revision: row.get("group_revision"),
        api_base_url: row.get("api_base_url"),
        upstream_api_format: parse_upstream_api_format(row.get::<String, _>("upstream_api_format")),
        collector_proxy_mode: row.get("collector_proxy_mode"),
        collector_proxy_url: row.get("collector_proxy_url"),
        key_enabled: i64_to_bool(row.get("key_enabled")),
        station_enabled: i64_to_bool(row.get("station_enabled")),
        api_key_secret_id: row.get("api_key_secret_id"),
        api_key_secret_scope: row.get("api_key_secret_scope"),
        api_key_secret_owner_id: row.get("api_key_secret_owner_id"),
        api_key_secret_kind: row.get("api_key_secret_kind"),
        inline_api_key_present: i64_to_bool(row.get("inline_api_key_present")),
        station_account_max_concurrency: row.get::<i64, _>("station_account_max_concurrency").max(0)
            as u32,
        station_key_max_concurrency: row.get::<i64, _>("station_key_max_concurrency").max(0) as u32,
    }
}

fn row_to_operational_monitoring_target_snapshot(
    row: sqlx::sqlite::SqliteRow,
) -> OperationalMonitoringTargetSnapshotRow {
    OperationalMonitoringTargetSnapshotRow {
        station_key_id: row.get("station_key_id"),
        station_id: row.get("station_id"),
        station_key_lifecycle_revision: u64::try_from(
            row.get::<i64, _>("station_key_lifecycle_revision"),
        )
        .unwrap_or_default(),
        endpoint_revision: row.get("endpoint_revision"),
        api_base_url: row.get("api_base_url"),
        upstream_api_format: parse_upstream_api_format(row.get::<String, _>("upstream_api_format")),
        supports_chat_completions: row
            .get::<Option<i64>, _>("supports_chat_completions")
            .map(i64_to_bool)
            .unwrap_or(true),
        supports_responses: row
            .get::<Option<i64>, _>("supports_responses")
            .map(i64_to_bool)
            .unwrap_or(true),
    }
}

mod runtime_candidate_column {
    pub(super) const STATION_KEY_ID: usize = 0;
    pub(super) const STATION_ID: usize = 1;
    pub(super) const STATION_TYPE: usize = 2;
    pub(super) const STATION_CREDIT_PER_CNY: usize = 3;
    pub(super) const ENDPOINT_REVISION: usize = 4;
    pub(super) const API_BASE_URL: usize = 5;
    pub(super) const UPSTREAM_API_FORMAT: usize = 6;
    pub(super) const ROUTING_ORDER: usize = 7;
    pub(super) const PRIORITY: usize = 8;
    pub(super) const MAX_CONCURRENCY: usize = 9;
    pub(super) const LOAD_FACTOR: usize = 10;
    pub(super) const SCHEDULABLE: usize = 11;
    pub(super) const COLLECTOR_PROXY_MODE: usize = 12;
    pub(super) const COLLECTOR_PROXY_URL: usize = 13;
    pub(super) const STATION_NAME: usize = 14;
    pub(super) const KEY_NAME: usize = 15;
    pub(super) const API_KEY: usize = 16;
    pub(super) const GROUP_NAME: usize = 17;
    pub(super) const GROUP_BINDING_ID: usize = 18;
    pub(super) const GROUP_ID_HASH: usize = 19;
    pub(super) const RATE_MULTIPLIER: usize = 20;
    pub(super) const MANUAL_RATE_MULTIPLIER: usize = 21;
    pub(super) const MANUAL_RATE_UPDATED_AT: usize = 22;
    pub(super) const RATE_SOURCE: usize = 23;
    pub(super) const RATE_COLLECTED_AT: usize = 24;
    pub(super) const KEY_UPDATED_AT: usize = 25;
    pub(super) const BINDING_GROUP_KEY_HASH: usize = 26;
    pub(super) const BINDING_GROUP_ID_HASH: usize = 27;
    pub(super) const BINDING_GROUP_NAME: usize = 28;
    pub(super) const BINDING_STATUS: usize = 29;
    pub(super) const BINDING_GROUP_CATEGORY: usize = 30;
    pub(super) const BINDING_EFFECTIVE_RATE_MULTIPLIER: usize = 31;
    pub(super) const BINDING_CONFIDENCE: usize = 32;
    pub(super) const BINDING_LAST_CHECKED_AT: usize = 33;
}

fn default_runtime_capabilities(station_key_id: &str) -> StationKeyCapabilities {
    StationKeyCapabilities {
        station_key_id: station_key_id.to_string(),
        supports_chat_completions: true,
        supports_responses: true,
        supports_embeddings: false,
        supports_stream: true,
        supports_tools: false,
        supports_vision: false,
        supports_reasoning: false,
        model_allowlist: Vec::new(),
        model_blocklist: Vec::new(),
        preferred_models: Vec::new(),
        only_use_as_backup: false,
        routing_tags: Vec::new(),
        updated_at: "0".to_string(),
    }
}

fn row_to_ranked_runtime_balance(
    row: &sqlx::sqlite::SqliteRow,
    offset: usize,
) -> RankedRuntimeBalance {
    RankedRuntimeBalance {
        balance: RuntimeRoutingBalance {
            scope: row.get(offset),
            value: row.get(offset + 1),
            currency: row.get(offset + 2),
            low_balance_threshold: row.get(offset + 3),
            status: row.get(offset + 4),
            collected_at: row.get(offset + 5),
        },
        updated_at: row.get(offset + 6),
        created_at: row.get(offset + 7),
        id: row.get(offset + 8),
    }
}

fn newest_balance(
    key: Option<RankedRuntimeBalance>,
    station: Option<&RankedRuntimeBalance>,
) -> Option<RuntimeRoutingBalance> {
    match (key, station) {
        // A finite positive amount is the strongest spendability fact. This
        // prevents stale textual depleted/exhausted metadata from masking a
        // usable balance at either scope.
        (Some(key), Some(_station)) if balance_has_positive_value(&key.balance) => {
            Some(key.balance)
        }
        (Some(_), Some(station)) if balance_has_positive_value(&station.balance) => {
            Some(station.balance.clone())
        }
        // A numeric exhausted balance is authoritative over a text-only
        // usable status. Without this, a key snapshot such as
        // `{ value: null, status: "normal" }` can hide a negative station
        // balance even though routing admission correctly treats it as
        // depleted.
        (Some(key), Some(_station)) if balance_has_finite_value(&key.balance) => Some(key.balance),
        (Some(_), Some(station)) if balance_has_finite_value(&station.balance) => {
            Some(station.balance.clone())
        }
        // When neither scope has a positive amount, key-scoped status remains
        // the narrower fact and therefore wins over station-level status.
        (Some(key), _) if balance_is_usable(&key.balance) => Some(key.balance),
        (Some(_), Some(station)) if balance_is_usable(&station.balance) => {
            Some(station.balance.clone())
        }
        (Some(key), Some(station)) if balance_rank_is_at_least(&key, station) => Some(key.balance),
        (Some(_), Some(station)) => Some(station.balance.clone()),
        (Some(key), None) => Some(key.balance),
        (None, Some(station)) => Some(station.balance.clone()),
        (None, None) => None,
    }
}

fn balance_is_usable(balance: &RuntimeRoutingBalance) -> bool {
    balance_has_positive_value(balance) || balance.has_explicit_status()
}

fn balance_has_positive_value(balance: &RuntimeRoutingBalance) -> bool {
    balance
        .value
        .is_some_and(|value| value.is_finite() && value > 0.0)
}

fn balance_has_finite_value(balance: &RuntimeRoutingBalance) -> bool {
    balance.value.is_some_and(f64::is_finite)
}

fn balance_rank_is_at_least(left: &RankedRuntimeBalance, right: &RankedRuntimeBalance) -> bool {
    (&left.updated_at, &left.created_at, &left.id)
        >= (&right.updated_at, &right.created_at, &right.id)
}

fn row_to_model_alias(row: sqlx::sqlite::SqliteRow) -> ModelAlias {
    ModelAlias {
        id: row.get("id"),
        client_model: row.get("client_model"),
        upstream_model: row.get("upstream_model"),
        enabled: i64_to_bool(row.get("enabled")),
        note: row.get("note"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn balance_snapshot_select_sql(tail: &str) -> String {
    format!(
        r#"
        SELECT id, station_id, station_key_id, scope, value, currency, credit_unit,
               used_value, total_value, today_request_count, total_request_count,
               today_consumption, total_consumption, today_base_consumption, total_base_consumption,
               today_token_count, total_token_count, today_input_token_count, today_output_token_count,
               total_input_token_count, total_output_token_count, account_concurrency_limit,
               low_balance_threshold, status, source, confidence, collected_at, created_at, updated_at
        FROM balance_snapshots
        {tail}
        "#
    )
}

fn row_to_balance_snapshot(row: sqlx::sqlite::SqliteRow) -> BalanceSnapshot {
    BalanceSnapshot {
        id: row.get("id"),
        station_id: row.get("station_id"),
        station_key_id: row.get("station_key_id"),
        scope: row.get("scope"),
        value: row.get("value"),
        currency: row.get("currency"),
        credit_unit: row.get("credit_unit"),
        used_value: row.get("used_value"),
        total_value: row.get("total_value"),
        today_request_count: row.get("today_request_count"),
        total_request_count: row.get("total_request_count"),
        today_consumption: row.get("today_consumption"),
        total_consumption: row.get("total_consumption"),
        today_base_consumption: row.get("today_base_consumption"),
        total_base_consumption: row.get("total_base_consumption"),
        today_token_count: row.get("today_token_count"),
        total_token_count: row.get("total_token_count"),
        today_input_token_count: row.get("today_input_token_count"),
        today_output_token_count: row.get("today_output_token_count"),
        total_input_token_count: row.get("total_input_token_count"),
        total_output_token_count: row.get("total_output_token_count"),
        account_concurrency_limit: row.get("account_concurrency_limit"),
        low_balance_threshold: row.get("low_balance_threshold"),
        status: row.get("status"),
        source: row.get("source"),
        confidence: row.get("confidence"),
        collected_at: row.get("collected_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn parse_json_string_list(value: String) -> Vec<String> {
    let value = value.trim();
    if value.is_empty() || value == "[]" {
        Vec::new()
    } else {
        serde_json::from_str::<Vec<String>>(value).unwrap_or_default()
    }
}

fn parse_upstream_api_format(value: String) -> UpstreamApiFormat {
    match value.as_str() {
        "openai_chat_completions" => UpstreamApiFormat::OpenAiChatCompletions,
        "openai_responses" => UpstreamApiFormat::OpenAiResponses,
        "custom_openai_compatible" => UpstreamApiFormat::CustomOpenAiCompatible,
        _ => UpstreamApiFormat::Auto,
    }
}

fn i64_to_bool(value: i64) -> bool {
    value != 0
}

trait NonEmptyString {
    fn into_non_empty(self) -> Option<String>;
}

impl NonEmptyString for String {
    fn into_non_empty(self) -> Option<String> {
        if self.is_empty() {
            None
        } else {
            Some(self)
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::Connection;

    use super::{
        newest_balance, row_to_operational_execution_target_ref, RankedRuntimeBalance,
        OPERATIONAL_EXECUTION_TARGET_REFS_QUERY_PREFIX,
    };
    use crate::models::routing::RuntimeRoutingBalance;

    fn ranked_balance(value: Option<f64>, status: &str, updated_at: &str) -> RankedRuntimeBalance {
        RankedRuntimeBalance {
            balance: RuntimeRoutingBalance {
                scope: "station".to_string(),
                value,
                currency: "USD".to_string(),
                low_balance_threshold: Some(5.0),
                status: status.to_string(),
                collected_at: None,
            },
            updated_at: updated_at.to_string(),
            created_at: updated_at.to_string(),
            id: updated_at.to_string(),
        }
    }

    #[test]
    fn explicit_station_status_overrides_stale_key_balance() {
        let key = ranked_balance(Some(0.0), "depleted", "3");
        let station = ranked_balance(Some(3.61), "normal", "2");

        let selected = newest_balance(Some(key), Some(&station)).expect("balance");

        assert_eq!(selected.value, Some(3.61));
        assert_eq!(selected.status, "normal");
    }

    #[test]
    fn unknown_station_status_falls_back_to_explicit_key_fact() {
        let key = ranked_balance(Some(2.5), "normal", "2");
        let station = ranked_balance(Some(0.0), "unknown", "3");

        let selected = newest_balance(Some(key), Some(&station)).expect("balance");

        assert_eq!(selected.value, Some(2.5));
        assert_eq!(selected.status, "normal");
    }

    #[test]
    fn low_station_status_remains_selected_as_routeable_advisory() {
        let key = ranked_balance(Some(0.0), "depleted", "3");
        let station = ranked_balance(Some(4.71), "low", "2");

        let selected = newest_balance(Some(key), Some(&station)).expect("balance");

        assert_eq!(selected.value, Some(4.71));
        assert_eq!(selected.status, "low");
        assert!(!selected.is_depleted());
    }

    #[test]
    fn positive_key_balance_wins_over_station_depleted_status() {
        let key = ranked_balance(Some(2.5), "normal", "2");
        let station = ranked_balance(Some(0.0), "depleted", "3");

        let selected = newest_balance(Some(key), Some(&station)).expect("balance");

        assert_eq!(selected.value, Some(2.5));
        assert_eq!(selected.status, "normal");
    }

    #[test]
    fn numeric_station_depletion_wins_over_text_only_normal_key_status() {
        let key = ranked_balance(None, "normal", "3");
        let station = ranked_balance(Some(-0.05), "normal", "2");

        let selected = newest_balance(Some(key), Some(&station)).expect("balance");

        assert_eq!(selected.value, Some(-0.05));
        assert!(selected.is_depleted());
    }

    #[tokio::test]
    async fn operational_execution_target_ref_does_not_require_capacity_domain_storage() {
        let mut connection = sqlx::SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open sqlite");
        for statement in [
            "CREATE TABLE station_keys (id TEXT PRIMARY KEY, station_id TEXT, group_binding_id TEXT, max_concurrency INTEGER, enabled INTEGER, api_key_secret_id TEXT, api_key TEXT)",
            "CREATE TABLE stations (id TEXT PRIMARY KEY, station_type TEXT, endpoint_revision INTEGER, api_base_url TEXT, upstream_api_format TEXT, collector_proxy_mode TEXT, collector_proxy_url TEXT, enabled INTEGER)",
            "CREATE TABLE domain_revisions (scope TEXT PRIMARY KEY, revision INTEGER)",
            "CREATE TABLE secrets (id TEXT PRIMARY KEY, scope TEXT, owner_id TEXT, kind TEXT)",
            "CREATE TABLE balance_snapshots (station_id TEXT, station_key_id TEXT, account_concurrency_limit INTEGER, updated_at TEXT, created_at TEXT, id TEXT)",
        ] {
            sqlx::query(statement)
                .execute(&mut connection)
                .await
                .expect("create fixture table");
        }
        sqlx::query("INSERT INTO stations VALUES ('station-a', 'sub2api', 7, 'https://example.test/v1', 'auto', 'system', NULL, 1)")
            .execute(&mut connection)
            .await
            .expect("station");
        sqlx::query(
            "INSERT INTO station_keys VALUES ('key-a', 'station-a', NULL, 9, 1, NULL, 'fake-key')",
        )
        .execute(&mut connection)
        .await
        .expect("station key");
        let mut query =
            sqlx::QueryBuilder::<sqlx::Sqlite>::new(OPERATIONAL_EXECUTION_TARGET_REFS_QUERY_PREFIX);
        query.push_bind("key-a");
        query.push(")");
        let row = query
            .build()
            .fetch_one(&mut connection)
            .await
            .expect("joined execution target row");
        let target = row_to_operational_execution_target_ref(row);

        assert_eq!(target.station_key_id, "key-a");
        assert_eq!(target.station_id, "station-a");
    }
}
