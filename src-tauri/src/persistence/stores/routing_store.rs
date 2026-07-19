use sqlx::Row;

use crate::{
    models::{
        pricing::BalanceSnapshot,
        proxy::UpstreamApiFormat,
        routing::{
            ModelAlias, RoutingProxyDefaults, RuntimeRoutingCandidate, RuntimeRoutingSecret,
            StationKeyCapabilities, StationKeyHealth,
        },
    },
    persistence::{error::PersistenceError, read_session::ReadSession},
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RoutingStore;

impl RoutingStore {
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
                k.api_key,
                sec.id AS secret_id,
                sec.scope AS secret_scope,
                sec.owner_id AS secret_owner_id,
                sec.kind AS secret_kind,
                sec.masked_value AS secret_masked_value,
                sec.ciphertext AS secret_ciphertext,
                sec.nonce AS secret_nonce,
                COALESCE(c.supports_chat_completions, 1) AS supports_chat_completions,
                COALESCE(c.supports_responses, 1) AS supports_responses,
                COALESCE(c.supports_embeddings, 0) AS supports_embeddings,
                COALESCE(c.supports_stream, 1) AS supports_stream,
                COALESCE(c.supports_tools, 0) AS supports_tools,
                COALESCE(c.supports_vision, 0) AS supports_vision,
                COALESCE(c.supports_reasoning, 0) AS supports_reasoning,
                COALESCE(c.model_allowlist_json, '[]') AS model_allowlist_json,
                COALESCE(c.model_blocklist_json, '[]') AS model_blocklist_json,
                COALESCE(c.preferred_models_json, '[]') AS preferred_models_json,
                COALESCE(c.only_use_as_backup, 0) AS only_use_as_backup,
                COALESCE(c.routing_tags_json, '[]') AS routing_tags_json,
                COALESCE(c.updated_at, '0') AS capabilities_updated_at,
                h.station_key_id AS health_station_key_id,
                h.last_success_at AS health_last_success_at,
                h.last_failure_at AS health_last_failure_at,
                h.consecutive_failures AS health_consecutive_failures,
                h.success_count AS health_success_count,
                h.failure_count AS health_failure_count,
                h.avg_latency_ms AS health_avg_latency_ms,
                h.last_error_summary AS health_last_error_summary,
                h.cooldown_until AS health_cooldown_until,
                h.updated_at AS health_updated_at,
                bs.id AS balance_id,
                bs.station_id AS balance_station_id,
                bs.station_key_id AS balance_station_key_id,
                bs.scope AS balance_scope,
                bs.value AS balance_value,
                bs.currency AS balance_currency,
                bs.credit_unit AS balance_credit_unit,
                bs.used_value AS balance_used_value,
                bs.total_value AS balance_total_value,
                bs.today_request_count AS balance_today_request_count,
                bs.total_request_count AS balance_total_request_count,
                bs.today_consumption AS balance_today_consumption,
                bs.total_consumption AS balance_total_consumption,
                bs.today_base_consumption AS balance_today_base_consumption,
                bs.total_base_consumption AS balance_total_base_consumption,
                bs.today_token_count AS balance_today_token_count,
                bs.total_token_count AS balance_total_token_count,
                bs.today_input_token_count AS balance_today_input_token_count,
                bs.today_output_token_count AS balance_today_output_token_count,
                bs.total_input_token_count AS balance_total_input_token_count,
                bs.total_output_token_count AS balance_total_output_token_count,
                bs.account_concurrency_limit AS balance_account_concurrency_limit,
                bs.low_balance_threshold AS balance_low_balance_threshold,
                bs.status AS balance_status,
                bs.source AS balance_source,
                bs.confidence AS balance_confidence,
                bs.collected_at AS balance_collected_at,
                bs.created_at AS balance_created_at,
                bs.updated_at AS balance_updated_at
            FROM station_keys k
            JOIN stations s ON s.id = k.station_id
            LEFT JOIN secrets sec ON sec.id = k.api_key_secret_id
            LEFT JOIN station_key_capabilities c ON c.station_key_id = k.id
            LEFT JOIN station_key_health h
                   ON h.station_key_id = k.id
                  AND h.endpoint_revision = s.endpoint_revision
            LEFT JOIN balance_snapshots bs ON bs.id = (
                SELECT latest_balance.id
                FROM balance_snapshots latest_balance
                WHERE latest_balance.station_key_id = k.id
                   OR (
                      latest_balance.station_key_id IS NULL
                      AND latest_balance.station_id = k.station_id
                      AND latest_balance.scope = 'station'
                   )
                ORDER BY latest_balance.updated_at DESC,
                         latest_balance.created_at DESC,
                         latest_balance.id DESC
                LIMIT 1
            )
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
        Ok(rows.into_iter().map(row_to_runtime_candidate).collect())
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
}

fn row_to_runtime_candidate(row: sqlx::sqlite::SqliteRow) -> RuntimeRoutingCandidate {
    let station_key_id: String = row.get("station_key_id");
    RuntimeRoutingCandidate {
        station_key_id: station_key_id.clone(),
        station_id: row.get("station_id"),
        station_endpoint_revision: row.get("endpoint_revision"),
        upstream_base_url: row.get("api_base_url"),
        upstream_api_format: parse_upstream_api_format(row.get::<String, _>("upstream_api_format")),
        routing_order: row.get("routing_order"),
        priority: row.get("priority"),
        max_concurrency: row.get("max_concurrency"),
        load_factor: row.get("load_factor"),
        schedulable: i64_to_bool(row.get("schedulable")),
        collector_proxy_mode: row.get("collector_proxy_mode"),
        collector_proxy_url: row.get("collector_proxy_url"),
        station_name: row.get("station_name"),
        key_name: row.get("key_name"),
        capabilities: StationKeyCapabilities {
            station_key_id,
            supports_chat_completions: i64_to_bool(row.get("supports_chat_completions")),
            supports_responses: i64_to_bool(row.get("supports_responses")),
            supports_embeddings: i64_to_bool(row.get("supports_embeddings")),
            supports_stream: i64_to_bool(row.get("supports_stream")),
            supports_tools: i64_to_bool(row.get("supports_tools")),
            supports_vision: i64_to_bool(row.get("supports_vision")),
            supports_reasoning: i64_to_bool(row.get("supports_reasoning")),
            model_allowlist: parse_json_string_list(row.get::<String, _>("model_allowlist_json")),
            model_blocklist: parse_json_string_list(row.get::<String, _>("model_blocklist_json")),
            preferred_models: parse_json_string_list(row.get::<String, _>("preferred_models_json")),
            only_use_as_backup: i64_to_bool(row.get("only_use_as_backup")),
            routing_tags: parse_json_string_list(row.get::<String, _>("routing_tags_json")),
            updated_at: row.get("capabilities_updated_at"),
        },
        health: row
            .get::<Option<String>, _>("health_station_key_id")
            .map(|station_key_id| {
                row_to_station_key_health_with_prefix(&row, station_key_id, "health_")
            }),
        balance_snapshot: row
            .get::<Option<String>, _>("balance_id")
            .map(|id| row_to_balance_snapshot_with_prefix(&row, id, "balance_")),
        api_key: row
            .get::<String, _>("api_key")
            .trim()
            .to_string()
            .into_non_empty(),
        api_key_secret: row
            .get::<Option<String>, _>("secret_id")
            .map(|id| RuntimeRoutingSecret {
                id,
                scope: row.get("secret_scope"),
                owner_id: row.get("secret_owner_id"),
                kind: row.get("secret_kind"),
                masked_value: row.get("secret_masked_value"),
                ciphertext: row.get("secret_ciphertext"),
                nonce: row.get("secret_nonce"),
            }),
    }
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

fn row_to_balance_snapshot_with_prefix(
    row: &sqlx::sqlite::SqliteRow,
    id: String,
    prefix: &str,
) -> BalanceSnapshot {
    BalanceSnapshot {
        id,
        station_id: row.get(format!("{prefix}station_id").as_str()),
        station_key_id: row.get(format!("{prefix}station_key_id").as_str()),
        scope: row.get(format!("{prefix}scope").as_str()),
        value: row.get(format!("{prefix}value").as_str()),
        currency: row.get(format!("{prefix}currency").as_str()),
        credit_unit: row.get(format!("{prefix}credit_unit").as_str()),
        used_value: row.get(format!("{prefix}used_value").as_str()),
        total_value: row.get(format!("{prefix}total_value").as_str()),
        today_request_count: row.get(format!("{prefix}today_request_count").as_str()),
        total_request_count: row.get(format!("{prefix}total_request_count").as_str()),
        today_consumption: row.get(format!("{prefix}today_consumption").as_str()),
        total_consumption: row.get(format!("{prefix}total_consumption").as_str()),
        today_base_consumption: row.get(format!("{prefix}today_base_consumption").as_str()),
        total_base_consumption: row.get(format!("{prefix}total_base_consumption").as_str()),
        today_token_count: row.get(format!("{prefix}today_token_count").as_str()),
        total_token_count: row.get(format!("{prefix}total_token_count").as_str()),
        today_input_token_count: row.get(format!("{prefix}today_input_token_count").as_str()),
        today_output_token_count: row.get(format!("{prefix}today_output_token_count").as_str()),
        total_input_token_count: row.get(format!("{prefix}total_input_token_count").as_str()),
        total_output_token_count: row.get(format!("{prefix}total_output_token_count").as_str()),
        account_concurrency_limit: row.get(format!("{prefix}account_concurrency_limit").as_str()),
        low_balance_threshold: row.get(format!("{prefix}low_balance_threshold").as_str()),
        status: row.get(format!("{prefix}status").as_str()),
        source: row.get(format!("{prefix}source").as_str()),
        confidence: row.get(format!("{prefix}confidence").as_str()),
        collected_at: row.get(format!("{prefix}collected_at").as_str()),
        created_at: row.get(format!("{prefix}created_at").as_str()),
        updated_at: row.get(format!("{prefix}updated_at").as_str()),
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

fn row_to_station_key_health_with_prefix(
    row: &sqlx::sqlite::SqliteRow,
    station_key_id: String,
    prefix: &str,
) -> StationKeyHealth {
    StationKeyHealth {
        station_key_id,
        last_success_at: row.get(format!("{prefix}last_success_at").as_str()),
        last_failure_at: row.get(format!("{prefix}last_failure_at").as_str()),
        consecutive_failures: row.get(format!("{prefix}consecutive_failures").as_str()),
        success_count: row.get(format!("{prefix}success_count").as_str()),
        failure_count: row.get(format!("{prefix}failure_count").as_str()),
        avg_latency_ms: row.get(format!("{prefix}avg_latency_ms").as_str()),
        last_error_summary: row.get(format!("{prefix}last_error_summary").as_str()),
        cooldown_until: row.get(format!("{prefix}cooldown_until").as_str()),
        updated_at: row.get(format!("{prefix}updated_at").as_str()),
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
    serde_json::from_str::<Vec<String>>(&value).unwrap_or_default()
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
