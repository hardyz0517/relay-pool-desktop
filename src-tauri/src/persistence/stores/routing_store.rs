use std::collections::{HashMap, HashSet};

use sqlx::Row;

use crate::{
    models::{
        pricing::BalanceSnapshot,
        proxy::UpstreamApiFormat,
        routing::{
            ModelAlias, RoutingGroupFilter, RoutingPolicy, RoutingProxyDefaults,
            RuntimeRoutingBalance, RuntimeRoutingCandidate, RuntimeRoutingSecret,
            RuntimeRoutingSettings, SchedulerAdvancedSettings, StationKeyCapabilities,
            StationKeyHealth, UpsertModelAliasInput,
        },
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

struct RankedRuntimeBalance {
    balance: RuntimeRoutingBalance,
    updated_at: String,
    created_at: String,
    id: String,
}

impl RoutingStore {
    pub(crate) async fn load_execution_settings(
        &self,
        read: &mut ReadSession,
    ) -> Result<RuntimeRoutingSettings, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT key, value
            FROM settings
            WHERE key IN (
                'default_routing_strategy',
                'max_rate_multiplier',
                'default_routing_group_filter',
                'scheduler_advanced_settings_json',
                'allow_depleted_fallback'
            )
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        let mut values = std::collections::HashMap::new();
        for row in rows {
            values.insert(row.get::<String, _>("key"), row.get::<String, _>("value"));
        }
        let policy = parse_routing_policy(required_setting(&values, "default_routing_strategy")?)?;
        let max_rate_multiplier = parse_optional_multiplier(
            values
                .get("max_rate_multiplier")
                .map(String::as_str)
                .unwrap_or_default(),
        )?;
        let routing_group_filter = parse_routing_group_filter(
            values
                .get("default_routing_group_filter")
                .map(String::as_str)
                .unwrap_or("all_groups"),
        )?;
        let scheduler_advanced_settings = parse_scheduler_settings(
            values
                .get("scheduler_advanced_settings_json")
                .map(String::as_str)
                .unwrap_or_default(),
        )?;
        let allow_depleted_fallback = parse_bool_setting(
            values
                .get("allow_depleted_fallback")
                .map(String::as_str)
                .unwrap_or("false"),
        )?;
        Ok(RuntimeRoutingSettings {
            policy,
            max_rate_multiplier,
            routing_group_filter,
            scheduler_advanced_settings,
            allow_depleted_fallback,
        })
    }

    pub(crate) async fn load_runtime_candidates(
        &self,
        read: &mut ReadSession,
    ) -> Result<Vec<RuntimeRoutingCandidate>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT
                k.id AS station_key_id,
                k.station_id,
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
                k.api_key
            FROM station_keys k
            JOIN stations s ON s.id = k.station_id
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
        let mut health = load_runtime_health(read).await?;
        let mut key_balances = load_latest_key_balances(read).await?;
        let station_balances = load_latest_station_balances(read).await?;

        Ok(candidates
            .into_iter()
            .map(|mut candidate| {
                candidate.api_key_secret = secrets.remove(&candidate.station_key_id);
                if let Some(value) = capabilities.remove(&candidate.station_key_id) {
                    candidate.capabilities = value;
                }
                candidate.health = health.remove(&candidate.station_key_id);
                candidate.balance_snapshot = newest_balance(
                    key_balances.remove(&candidate.station_key_id),
                    station_balances.get(&candidate.station_id),
                );
                candidate
            })
            .collect())
    }

    pub(crate) async fn load_proxy_defaults(
        &self,
        read: &mut ReadSession,
    ) -> Result<RoutingProxyDefaults, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT key, value
            FROM settings
            WHERE key IN ('collector_proxy_mode', 'collector_proxy_url')
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        let mut collector_proxy_mode = "direct".to_string();
        let mut collector_proxy_url = None;
        for row in rows {
            let key: String = row.get("key");
            let value: String = row.get("value");
            match key.as_str() {
                "collector_proxy_mode" => collector_proxy_mode = value,
                "collector_proxy_url" => {
                    collector_proxy_url = Some(value).filter(|value| !value.trim().is_empty());
                }
                _ => {}
            }
        }
        Ok(RoutingProxyDefaults {
            collector_proxy_mode,
            collector_proxy_url,
        })
    }

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

    pub(crate) async fn upsert_model_alias(
        &self,
        write: &mut WriteSession,
        input: UpsertModelAliasInput,
        id: &str,
        now: &str,
    ) -> Result<ModelAlias, PersistenceError> {
        let client_model = input.client_model.trim();
        let upstream_model = input.upstream_model.trim();
        if client_model.is_empty() || upstream_model.is_empty() {
            return Err(PersistenceError::ConstraintViolation);
        }
        let note = input.note.and_then(|note| {
            let note = note.trim().to_string();
            (!note.is_empty()).then_some(note)
        });
        sqlx::query(
            r#"
            INSERT INTO model_aliases (
                id, client_model, upstream_model, enabled, note, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(client_model, upstream_model) DO UPDATE SET
                enabled = excluded.enabled,
                note = excluded.note,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(id)
        .bind(client_model)
        .bind(upstream_model)
        .bind(bool_to_i64(input.enabled))
        .bind(note)
        .bind(now)
        .bind(now)
        .execute(write.connection())
        .await?;
        model_alias_by_pair(write, client_model, upstream_model).await
    }

    pub(crate) async fn delete_model_alias(
        &self,
        write: &mut WriteSession,
        id: &str,
    ) -> Result<(), PersistenceError> {
        sqlx::query("DELETE FROM model_aliases WHERE id = ?1")
            .bind(id)
            .execute(write.connection())
            .await?;
        Ok(())
    }

    pub(crate) async fn reorder_local_routing_keys(
        &self,
        write: &mut WriteSession,
        station_key_ids: &[String],
        now: &str,
    ) -> Result<(), PersistenceError> {
        if station_key_ids.is_empty() {
            return Err(PersistenceError::ConstraintViolation);
        }
        let mut requested = HashSet::with_capacity(station_key_ids.len());
        if station_key_ids.iter().any(|id| !requested.insert(id)) {
            return Err(PersistenceError::ConstraintViolation);
        }
        for id in station_key_ids {
            let exists = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS(SELECT 1 FROM station_keys WHERE id = ?1)",
            )
            .bind(id)
            .fetch_one(write.connection())
            .await?;
            if exists == 0 {
                return Err(PersistenceError::NotFound);
            }
        }
        let all_ids = sqlx::query_scalar::<_, String>(
            r#"
            SELECT id FROM station_keys
            ORDER BY COALESCE(routing_order, priority) ASC,
                     priority ASC, created_at ASC, id ASC
            "#,
        )
        .fetch_all(write.connection())
        .await?;
        let ordered_ids = station_key_ids
            .iter()
            .cloned()
            .chain(all_ids.into_iter().filter(|id| !requested.contains(id)))
            .collect::<Vec<_>>();
        for (index, id) in ordered_ids.iter().enumerate() {
            sqlx::query(
                "UPDATE station_keys SET routing_order = ?1, updated_at = ?2 WHERE id = ?3",
            )
            .bind(index as i64)
            .bind(now)
            .bind(id)
            .execute(write.connection())
            .await?;
        }
        Ok(())
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

    pub(crate) async fn list_station_key_health(
        &self,
        read: &mut ReadSession,
    ) -> Result<Vec<StationKeyHealth>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT h.station_key_id, h.last_success_at, h.last_failure_at, h.consecutive_failures,
                   h.success_count, h.failure_count, h.avg_latency_ms, h.last_error_summary,
                   h.cooldown_until, h.updated_at
            FROM station_key_health h
            JOIN station_keys k ON k.id = h.station_key_id
            JOIN stations s ON s.id = k.station_id
            WHERE h.endpoint_revision = s.endpoint_revision
            ORDER BY h.updated_at DESC, h.station_key_id ASC
            "#,
        )
        .fetch_all(read.connection())
        .await?;
        Ok(rows.into_iter().map(row_to_station_key_health).collect())
    }

    pub(crate) async fn station_key_health_by_id(
        &self,
        read: &mut ReadSession,
        station_key_id: &str,
    ) -> Result<StationKeyHealth, PersistenceError> {
        let exists =
            sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM station_keys WHERE id = ?1)")
                .bind(station_key_id)
                .fetch_one(read.connection())
                .await?;
        if exists == 0 {
            return Err(PersistenceError::NotFound);
        }
        let row = sqlx::query(
            r#"
            SELECT h.station_key_id, h.last_success_at, h.last_failure_at, h.consecutive_failures,
                   h.success_count, h.failure_count, h.avg_latency_ms, h.last_error_summary,
                   h.cooldown_until, h.updated_at
            FROM station_key_health h
            JOIN station_keys k ON k.id = h.station_key_id
            JOIN stations s ON s.id = k.station_id
            WHERE h.station_key_id = ?1
              AND h.endpoint_revision = s.endpoint_revision
            "#,
        )
        .bind(station_key_id)
        .fetch_optional(read.connection())
        .await?;
        Ok(row
            .map(row_to_station_key_health)
            .unwrap_or_else(|| default_station_key_health(station_key_id)))
    }

    pub(crate) async fn list_station_endpoint_health(
        &self,
        read: &mut ReadSession,
    ) -> Result<Vec<StationEndpointHealth>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT h.station_id, h.endpoint_revision, h.status, h.latency_ms,
                   h.checked_at, h.error_summary, h.updated_at
            FROM station_endpoint_health h
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
            INSERT INTO station_endpoint_health (
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn record_station_key_connectivity(
        &self,
        write: &mut WriteSession,
        station_key_id: &str,
        station_id: &str,
        expected_endpoint_revision: i64,
        ok: bool,
        duration_ms: i64,
        error_summary: &str,
        now: &str,
    ) -> Result<(), PersistenceError> {
        assert_station_endpoint_revision(write, station_id, expected_endpoint_revision).await?;
        let belongs_to_station = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM station_keys WHERE id = ?1 AND station_id = ?2)",
        )
        .bind(station_key_id)
        .bind(station_id)
        .fetch_one(write.connection())
        .await?;
        if belongs_to_station == 0 {
            return Err(PersistenceError::NotFound);
        }
        let current =
            station_key_health_for_write(write, station_key_id, expected_endpoint_revision)
                .await?
                .unwrap_or_else(|| default_station_key_health(station_key_id));
        let (
            last_success_at,
            last_failure_at,
            consecutive_failures,
            success_count,
            failure_count,
            total_duration_ms,
            avg_latency_ms,
            last_error_summary,
            cooldown_until,
        ) = if ok {
            let success_count = current.success_count.saturating_add(1);
            let total_duration_ms = current
                .avg_latency_ms
                .unwrap_or(0)
                .saturating_mul(current.success_count)
                .saturating_add(duration_ms.max(0));
            (
                Some(now.to_string()),
                current.last_failure_at,
                0,
                success_count,
                current.failure_count,
                total_duration_ms,
                Some(total_duration_ms / success_count.max(1)),
                None,
                None,
            )
        } else {
            let consecutive_failures = current.consecutive_failures.saturating_add(1);
            let cooldown_until = connectivity_cooldown_until(consecutive_failures, now);
            (
                current.last_success_at,
                Some(now.to_string()),
                consecutive_failures,
                current.success_count,
                current.failure_count.saturating_add(1),
                current
                    .avg_latency_ms
                    .unwrap_or(0)
                    .saturating_mul(current.success_count),
                current.avg_latency_ms,
                Some(trim_error_summary(error_summary)),
                cooldown_until,
            )
        };
        sqlx::query(
            r#"
            INSERT INTO station_key_health (
                station_key_id, endpoint_revision, last_success_at, last_failure_at,
                consecutive_failures, success_count, failure_count, total_duration_ms,
                avg_latency_ms, last_error_summary, cooldown_until, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(station_key_id) DO UPDATE SET
                endpoint_revision = excluded.endpoint_revision,
                last_success_at = excluded.last_success_at,
                last_failure_at = excluded.last_failure_at,
                consecutive_failures = excluded.consecutive_failures,
                success_count = excluded.success_count,
                failure_count = excluded.failure_count,
                total_duration_ms = excluded.total_duration_ms,
                avg_latency_ms = excluded.avg_latency_ms,
                last_error_summary = excluded.last_error_summary,
                cooldown_until = excluded.cooldown_until,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(station_key_id)
        .bind(expected_endpoint_revision)
        .bind(last_success_at)
        .bind(last_failure_at)
        .bind(consecutive_failures)
        .bind(success_count)
        .bind(failure_count)
        .bind(total_duration_ms)
        .bind(avg_latency_ms)
        .bind(last_error_summary)
        .bind(cooldown_until)
        .bind(now)
        .execute(write.connection())
        .await?;
        sqlx::query(
            r#"
            UPDATE station_keys
            SET status = ?1, last_checked_at = ?2, updated_at = ?2
            WHERE id = ?3 AND station_id = ?4
            "#,
        )
        .bind(if ok { "healthy" } else { "error" })
        .bind(now)
        .bind(station_key_id)
        .bind(station_id)
        .execute(write.connection())
        .await?;
        Ok(())
    }
}

async fn load_runtime_secrets(
    read: &mut ReadSession,
) -> Result<HashMap<String, RuntimeRoutingSecret>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        SELECT k.id, sec.id, sec.scope, sec.owner_id, sec.kind,
               sec.masked_value, sec.ciphertext, sec.nonce
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

async fn load_runtime_health(
    read: &mut ReadSession,
) -> Result<HashMap<String, StationKeyHealth>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        SELECT h.station_key_id, h.last_success_at, h.last_failure_at,
               h.consecutive_failures, h.success_count, h.failure_count,
               h.avg_latency_ms, h.last_error_summary, h.cooldown_until, h.updated_at
        FROM station_key_health h
        JOIN station_keys k ON k.id = h.station_key_id
        JOIN stations s
          ON s.id = k.station_id
         AND s.endpoint_revision = h.endpoint_revision
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
                StationKeyHealth {
                    station_key_id,
                    last_success_at: row.get(1),
                    last_failure_at: row.get(2),
                    consecutive_failures: row.get(3),
                    success_count: row.get(4),
                    failure_count: row.get(5),
                    avg_latency_ms: row.get(6),
                    last_error_summary: row.get(7),
                    cooldown_until: row.get(8),
                    updated_at: row.get(9),
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

async fn model_alias_by_pair(
    write: &mut WriteSession,
    client_model: &str,
    upstream_model: &str,
) -> Result<ModelAlias, PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT id, client_model, upstream_model, enabled, note, created_at, updated_at
        FROM model_aliases
        WHERE client_model = ?1 AND upstream_model = ?2
        "#,
    )
    .bind(client_model)
    .bind(upstream_model)
    .fetch_optional(write.connection())
    .await?
    .ok_or(PersistenceError::NotFound)?;
    Ok(row_to_model_alias(row))
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
        FROM station_endpoint_health
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

async fn station_key_health_for_write(
    write: &mut WriteSession,
    station_key_id: &str,
    endpoint_revision: i64,
) -> Result<Option<StationKeyHealth>, PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT station_key_id, last_success_at, last_failure_at, consecutive_failures,
               success_count, failure_count, avg_latency_ms, last_error_summary,
               cooldown_until, updated_at
        FROM station_key_health
        WHERE station_key_id = ?1 AND endpoint_revision = ?2
        "#,
    )
    .bind(station_key_id)
    .bind(endpoint_revision)
    .fetch_optional(write.connection())
    .await?;
    Ok(row.map(row_to_station_key_health))
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

fn connectivity_cooldown_until(consecutive_failures: i64, now: &str) -> Option<String> {
    let now = now.parse::<i64>().ok()?;
    let duration_ms = match consecutive_failures {
        failures if failures < 3 => return None,
        3 => 2 * 60 * 1000,
        4 => 5 * 60 * 1000,
        _ => 15 * 60 * 1000,
    };
    Some(now.saturating_add(duration_ms).to_string())
}

fn trim_error_summary(value: &str) -> String {
    let mut chars = value.trim().chars();
    let mut summary = chars.by_ref().take(160).collect::<String>();
    if chars.next().is_some() {
        summary.push_str("...");
    }
    summary
}

fn required_setting<'a>(
    values: &'a std::collections::HashMap<String, String>,
    key: &str,
) -> Result<&'a str, PersistenceError> {
    values
        .get(key)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| PersistenceError::InvariantViolation(format!("missing setting: {key}")))
}

fn parse_routing_policy(value: &str) -> Result<RoutingPolicy, PersistenceError> {
    match value.trim() {
        "automatic_balanced" | "automatic" => Ok(RoutingPolicy::AutomaticBalanced),
        "priority_fallback" => Ok(RoutingPolicy::PriorityFallback),
        "stable_first" | "stable" => Ok(RoutingPolicy::StableFirst),
        "backup_only" => Ok(RoutingPolicy::BackupOnly),
        "cheap_first" => Ok(RoutingPolicy::CheapFirst),
        "cost_stable_first" => Ok(RoutingPolicy::CostStableFirst),
        _ => Err(PersistenceError::InvariantViolation(
            "invalid default routing strategy".to_string(),
        )),
    }
}

fn parse_optional_multiplier(value: &str) -> Result<Option<f64>, PersistenceError> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    let value = value
        .parse::<f64>()
        .map_err(|_| PersistenceError::InvariantViolation("invalid max rate multiplier".into()))?;
    if !value.is_finite() || value < 0.0 {
        return Err(PersistenceError::InvariantViolation(
            "invalid max rate multiplier".into(),
        ));
    }
    Ok(Some(value))
}

fn parse_routing_group_filter(value: &str) -> Result<RoutingGroupFilter, PersistenceError> {
    serde_json::from_str::<RoutingGroupFilter>(value)
        .or_else(|_| {
            serde_json::from_value::<RoutingGroupFilter>(serde_json::Value::String(
                value.to_string(),
            ))
        })
        .map_err(|_| PersistenceError::InvariantViolation("invalid routing group filter".into()))
}

fn parse_scheduler_settings(value: &str) -> Result<SchedulerAdvancedSettings, PersistenceError> {
    if value.trim().is_empty() {
        return Ok(SchedulerAdvancedSettings::default());
    }
    let settings = serde_json::from_str::<SchedulerAdvancedSettings>(value)
        .map_err(|_| PersistenceError::InvariantViolation("invalid scheduler settings".into()))?;
    settings
        .validate()
        .map_err(|_| PersistenceError::InvariantViolation("invalid scheduler settings".into()))?;
    Ok(settings)
}

fn parse_bool_setting(value: &str) -> Result<bool, PersistenceError> {
    value
        .parse::<bool>()
        .map_err(|_| PersistenceError::InvariantViolation("invalid boolean setting".into()))
}

fn bool_to_i64(value: bool) -> i64 {
    i64::from(value)
}

fn row_to_runtime_candidate(row: sqlx::sqlite::SqliteRow) -> RuntimeRoutingCandidate {
    let station_key_id: String = row.get(runtime_candidate_column::STATION_KEY_ID);
    RuntimeRoutingCandidate {
        station_key_id: station_key_id.clone(),
        station_id: row.get(runtime_candidate_column::STATION_ID),
        station_endpoint_revision: row.get(runtime_candidate_column::ENDPOINT_REVISION),
        upstream_base_url: row.get(runtime_candidate_column::API_BASE_URL),
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
        health: None,
        balance_snapshot: None,
        api_key: row
            .get::<String, _>(runtime_candidate_column::API_KEY)
            .trim()
            .to_string()
            .into_non_empty(),
        api_key_secret: None,
    }
}

mod runtime_candidate_column {
    pub(super) const STATION_KEY_ID: usize = 0;
    pub(super) const STATION_ID: usize = 1;
    pub(super) const ENDPOINT_REVISION: usize = 2;
    pub(super) const API_BASE_URL: usize = 3;
    pub(super) const UPSTREAM_API_FORMAT: usize = 4;
    pub(super) const ROUTING_ORDER: usize = 5;
    pub(super) const PRIORITY: usize = 6;
    pub(super) const MAX_CONCURRENCY: usize = 7;
    pub(super) const LOAD_FACTOR: usize = 8;
    pub(super) const SCHEDULABLE: usize = 9;
    pub(super) const COLLECTOR_PROXY_MODE: usize = 10;
    pub(super) const COLLECTOR_PROXY_URL: usize = 11;
    pub(super) const STATION_NAME: usize = 12;
    pub(super) const KEY_NAME: usize = 13;
    pub(super) const API_KEY: usize = 14;
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
        (Some(key), Some(station)) if balance_rank_is_at_least(&key, station) => Some(key.balance),
        (Some(_), Some(station)) => Some(station.balance.clone()),
        (Some(key), None) => Some(key.balance),
        (None, Some(station)) => Some(station.balance.clone()),
        (None, None) => None,
    }
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

fn row_to_station_key_health(row: sqlx::sqlite::SqliteRow) -> StationKeyHealth {
    StationKeyHealth {
        station_key_id: row.get("station_key_id"),
        last_success_at: row.get("last_success_at"),
        last_failure_at: row.get("last_failure_at"),
        consecutive_failures: row.get("consecutive_failures"),
        success_count: row.get("success_count"),
        failure_count: row.get("failure_count"),
        avg_latency_ms: row.get("avg_latency_ms"),
        last_error_summary: row.get("last_error_summary"),
        cooldown_until: row.get("cooldown_until"),
        updated_at: row.get("updated_at"),
    }
}

fn default_station_key_health(station_key_id: &str) -> StationKeyHealth {
    StationKeyHealth {
        station_key_id: station_key_id.to_string(),
        last_success_at: None,
        last_failure_at: None,
        consecutive_failures: 0,
        success_count: 0,
        failure_count: 0,
        avg_latency_ms: None,
        last_error_summary: None,
        cooldown_until: None,
        updated_at: "0".to_string(),
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
