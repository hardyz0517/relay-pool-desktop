use sqlx::{Row, SqliteConnection};

use crate::{
    models::{
        group_facts::{GroupRateRecord, StationGroupBinding},
        pricing::{
            BalanceSnapshot, ModelBasePrice, PricingRule, UpsertBalanceSnapshotInput,
            UpsertModelBasePriceInput, UpsertPricingRuleInput,
        },
        station_keys::StationKey,
        stations::Station,
    },
    persistence::{
        error::PersistenceError, read_session::ReadSession, write_session::WriteSession,
    },
    services::secrets::mask::mask_secret,
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PricingStore;

#[derive(Debug, Clone)]
pub(crate) struct NewPricingRuleRow {
    pub(crate) id: String,
    pub(crate) now: String,
    pub(crate) input: UpsertPricingRuleInput,
}

#[derive(Debug, Clone)]
pub(crate) struct NewModelBasePriceRow {
    pub(crate) id: String,
    pub(crate) now: String,
    pub(crate) input: UpsertModelBasePriceInput,
}

#[derive(Debug, Clone)]
pub(crate) struct NewBalanceSnapshotRow {
    pub(crate) id: String,
    pub(crate) now: String,
    pub(crate) input: UpsertBalanceSnapshotInput,
}

#[derive(Debug, Clone)]
pub(crate) struct PricingComparisonRows {
    pub(crate) stations: Vec<Station>,
    pub(crate) station_keys: Vec<StationKey>,
    pub(crate) group_bindings: Vec<StationGroupBinding>,
    pub(crate) group_rates: Vec<GroupRateRecord>,
    pub(crate) pricing_rules: Vec<PricingRule>,
    pub(crate) developer_mode_enabled: bool,
}

impl PricingStore {
    pub(crate) async fn load_comparison_workspace(
        &self,
        read: &mut ReadSession,
        limit: u32,
    ) -> Result<PricingComparisonRows, PersistenceError> {
        let limit = i64::from(limit);
        let stations = list_stations(read.connection(), limit).await?;
        let station_keys = list_station_keys(read.connection(), limit).await?;
        let group_bindings = list_group_bindings(read.connection(), limit).await?;
        let group_rates = list_latest_group_rates(read.connection(), limit).await?;
        let pricing_rules = list_pricing_rules(read.connection(), limit).await?;
        let developer_mode_enabled = sqlx::query_scalar::<_, String>(
            "SELECT value FROM settings WHERE key = 'developer_mode_enabled'",
        )
        .fetch_optional(read.connection())
        .await?
        .as_deref()
            == Some("true");

        Ok(PricingComparisonRows {
            stations,
            station_keys,
            group_bindings,
            group_rates,
            pricing_rules,
            developer_mode_enabled,
        })
    }

    pub(crate) async fn list_model_base_prices(
        &self,
        read: &mut ReadSession,
        limit: u32,
    ) -> Result<Vec<ModelBasePrice>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT id, provider, model, input_price, output_price, currency, unit,
                   source_url, source_label, source_checked_at, enabled, built_in,
                   note, created_at, updated_at
            FROM model_base_prices
            ORDER BY enabled DESC, provider ASC, model ASC, updated_at DESC, id DESC
            LIMIT ?1
            "#,
        )
        .bind(i64::from(limit))
        .fetch_all(read.connection())
        .await?;
        Ok(rows.into_iter().map(row_to_model_base_price).collect())
    }

    pub(crate) async fn list_pricing_rules(
        &self,
        read: &mut ReadSession,
        limit: u32,
    ) -> Result<Vec<PricingRule>, PersistenceError> {
        list_pricing_rules(read.connection(), i64::from(limit)).await
    }

    pub(crate) async fn latest_station_balances(
        &self,
        read: &mut ReadSession,
        limit: u32,
    ) -> Result<Vec<BalanceSnapshot>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            WITH ranked AS (
                SELECT b.*,
                       ROW_NUMBER() OVER (
                           PARTITION BY b.station_id, b.scope
                           ORDER BY b.updated_at DESC, b.created_at DESC, b.id DESC
                       ) AS row_number
                FROM balance_snapshots b INDEXED BY idx_balance_snapshots_latest_station_scope
                WHERE b.scope = 'station'
            )
            SELECT id, station_id, station_key_id, scope, value, currency, credit_unit,
                   used_value, total_value, today_request_count, total_request_count,
                   today_consumption, total_consumption, today_base_consumption,
                   total_base_consumption, today_token_count, total_token_count,
                   today_input_token_count, today_output_token_count,
                   total_input_token_count, total_output_token_count,
                   account_concurrency_limit, low_balance_threshold, status, source,
                   confidence, collected_at, created_at, updated_at
            FROM ranked
            WHERE row_number = 1
            ORDER BY updated_at DESC, created_at DESC, id DESC
            LIMIT ?1
            "#,
        )
        .bind(i64::from(limit))
        .fetch_all(read.connection())
        .await?;
        Ok(rows.into_iter().map(row_to_balance_snapshot).collect())
    }

    pub(crate) async fn select_pricing_rule(
        &self,
        read: &mut ReadSession,
        station_id: &str,
        station_key_id: Option<&str>,
        group_binding_id: Option<&str>,
        model: &str,
        at: &str,
    ) -> Result<Option<PricingRule>, PersistenceError> {
        let row = sqlx::query(
            r#"
            SELECT id, station_id, station_key_id, group_binding_id, group_name,
                   tier_label, model, input_price, output_price, fixed_price,
                   rate_multiplier, currency, unit, price_type, base_price_source,
                   normalization_status, source, confidence, enabled, note,
                   collected_at, valid_from, valid_until, created_at, updated_at
            FROM pricing_rules
            WHERE station_id = ?1
              AND model = ?2
              AND enabled = 1
              AND (valid_from IS NULL OR CAST(valid_from AS INTEGER) <= CAST(?5 AS INTEGER))
              AND (valid_until IS NULL OR CAST(valid_until AS INTEGER) > CAST(?5 AS INTEGER))
              AND (station_key_id IS NULL OR station_key_id = ?3)
              AND (group_binding_id IS NULL OR group_binding_id = ?4)
            ORDER BY
                CASE WHEN station_key_id = ?3 THEN 1 ELSE 0 END DESC,
                CASE WHEN group_binding_id = ?4 THEN 1 ELSE 0 END DESC,
                updated_at DESC,
                created_at DESC,
                id DESC
            LIMIT 1
            "#,
        )
        .bind(station_id)
        .bind(model.trim())
        .bind(station_key_id)
        .bind(group_binding_id)
        .bind(at)
        .fetch_optional(read.connection())
        .await?;
        Ok(row.map(row_to_pricing_rule))
    }

    pub(crate) async fn select_model_base_price(
        &self,
        read: &mut ReadSession,
        model: &str,
    ) -> Result<Option<ModelBasePrice>, PersistenceError> {
        let row = sqlx::query(
            r#"
            SELECT id, provider, model, input_price, output_price, currency, unit,
                   source_url, source_label, source_checked_at, enabled, built_in,
                   note, created_at, updated_at
            FROM model_base_prices
            WHERE model = ?1 AND enabled = 1
            ORDER BY updated_at DESC, created_at DESC, id DESC
            LIMIT 1
            "#,
        )
        .bind(model.trim())
        .fetch_optional(read.connection())
        .await?;
        Ok(row.map(row_to_model_base_price))
    }

    pub(crate) async fn upsert_pricing_rule(
        &self,
        write: &mut WriteSession,
        row: NewPricingRuleRow,
    ) -> Result<PricingRule, PersistenceError> {
        validate_pricing_rule(&row.input)?;
        validate_optional_station_owners(
            write.connection(),
            &row.input.station_id,
            row.input.station_key_id.as_deref(),
            row.input.group_binding_id.as_deref(),
        )
        .await?;
        sqlx::query(
            r#"
            INSERT INTO pricing_rules (
                id, station_id, station_key_id, group_binding_id, group_name,
                tier_label, model, input_price, output_price, fixed_price,
                rate_multiplier, currency, unit, price_type, base_price_source,
                normalization_status, source, confidence, enabled, note,
                collected_at, valid_from, valid_until, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25
            )
            ON CONFLICT(id) DO UPDATE SET
                station_id = excluded.station_id,
                station_key_id = excluded.station_key_id,
                group_binding_id = excluded.group_binding_id,
                group_name = excluded.group_name,
                tier_label = excluded.tier_label,
                model = excluded.model,
                input_price = excluded.input_price,
                output_price = excluded.output_price,
                fixed_price = excluded.fixed_price,
                rate_multiplier = excluded.rate_multiplier,
                currency = excluded.currency,
                unit = excluded.unit,
                price_type = excluded.price_type,
                base_price_source = excluded.base_price_source,
                normalization_status = excluded.normalization_status,
                source = excluded.source,
                confidence = excluded.confidence,
                enabled = excluded.enabled,
                note = excluded.note,
                collected_at = excluded.collected_at,
                valid_from = excluded.valid_from,
                valid_until = excluded.valid_until,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&row.id)
        .bind(&row.input.station_id)
        .bind(normalize_optional(&row.input.station_key_id))
        .bind(normalize_optional(&row.input.group_binding_id))
        .bind(normalize_optional(&row.input.group_name))
        .bind(normalize_optional(&row.input.tier_label))
        .bind(row.input.model.trim())
        .bind(row.input.input_price)
        .bind(row.input.output_price)
        .bind(row.input.fixed_price)
        .bind(row.input.rate_multiplier)
        .bind(row.input.currency.trim().to_uppercase())
        .bind(row.input.unit.trim())
        .bind(row.input.price_type.trim())
        .bind(normalize_optional(&row.input.base_price_source))
        .bind(
            normalize_optional(&row.input.normalization_status)
                .unwrap_or_else(|| "manual".to_string()),
        )
        .bind(row.input.source.trim())
        .bind(row.input.confidence)
        .bind(bool_to_i64(row.input.enabled))
        .bind(normalize_optional(&row.input.note))
        .bind(normalize_optional(&row.input.collected_at))
        .bind(normalize_optional(&row.input.valid_from))
        .bind(normalize_optional(&row.input.valid_until))
        .bind(&row.now)
        .bind(&row.now)
        .execute(write.connection())
        .await?;
        pricing_rule_by_id(write.connection(), &row.id).await
    }

    pub(crate) async fn delete_pricing_rule(
        &self,
        write: &mut WriteSession,
        id: &str,
    ) -> Result<(), PersistenceError> {
        let deleted = sqlx::query("DELETE FROM pricing_rules WHERE id = ?1")
            .bind(id)
            .execute(write.connection())
            .await?
            .rows_affected();
        if deleted == 0 {
            return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
        }
        Ok(())
    }

    pub(crate) async fn upsert_model_base_price(
        &self,
        write: &mut WriteSession,
        row: NewModelBasePriceRow,
    ) -> Result<ModelBasePrice, PersistenceError> {
        validate_model_base_price(&row.input)?;
        sqlx::query(
            r#"
            INSERT INTO model_base_prices (
                id, provider, model, input_price, output_price, currency, unit,
                source_url, source_label, source_checked_at, enabled, built_in,
                note, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(id) DO UPDATE SET
                provider = excluded.provider,
                model = excluded.model,
                input_price = excluded.input_price,
                output_price = excluded.output_price,
                currency = excluded.currency,
                unit = excluded.unit,
                source_url = excluded.source_url,
                source_label = excluded.source_label,
                source_checked_at = excluded.source_checked_at,
                enabled = excluded.enabled,
                built_in = excluded.built_in,
                note = excluded.note,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&row.id)
        .bind(row.input.provider.trim())
        .bind(row.input.model.trim())
        .bind(row.input.input_price)
        .bind(row.input.output_price)
        .bind(row.input.currency.trim().to_uppercase())
        .bind(row.input.unit.trim())
        .bind(row.input.source_url.trim())
        .bind(row.input.source_label.trim())
        .bind(normalize_optional(&row.input.source_checked_at))
        .bind(bool_to_i64(row.input.enabled))
        .bind(bool_to_i64(row.input.built_in))
        .bind(normalize_optional(&row.input.note))
        .bind(&row.now)
        .bind(&row.now)
        .execute(write.connection())
        .await?;
        model_base_price_by_id(write.connection(), &row.id).await
    }

    pub(crate) async fn upsert_balance_snapshot(
        &self,
        write: &mut WriteSession,
        row: NewBalanceSnapshotRow,
    ) -> Result<BalanceSnapshot, PersistenceError> {
        validate_balance_snapshot(&row.input)?;
        validate_optional_station_owners(
            write.connection(),
            &row.input.station_id,
            row.input.station_key_id.as_deref(),
            None,
        )
        .await?;
        sqlx::query(
            r#"
            INSERT INTO balance_snapshots (
                id, station_id, station_key_id, scope, value, currency, credit_unit,
                used_value, total_value, today_request_count, total_request_count,
                today_consumption, total_consumption, today_base_consumption,
                total_base_consumption, today_token_count, total_token_count,
                today_input_token_count, today_output_token_count,
                total_input_token_count, total_output_token_count,
                account_concurrency_limit, low_balance_threshold, status, source,
                confidence, collected_at, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25,
                ?26, ?27, ?28, ?29
            )
            ON CONFLICT(id) DO UPDATE SET
                station_id = excluded.station_id,
                station_key_id = excluded.station_key_id,
                scope = excluded.scope,
                value = excluded.value,
                currency = excluded.currency,
                credit_unit = excluded.credit_unit,
                used_value = excluded.used_value,
                total_value = excluded.total_value,
                today_request_count = excluded.today_request_count,
                total_request_count = excluded.total_request_count,
                today_consumption = excluded.today_consumption,
                total_consumption = excluded.total_consumption,
                today_base_consumption = excluded.today_base_consumption,
                total_base_consumption = excluded.total_base_consumption,
                today_token_count = excluded.today_token_count,
                total_token_count = excluded.total_token_count,
                today_input_token_count = excluded.today_input_token_count,
                today_output_token_count = excluded.today_output_token_count,
                total_input_token_count = excluded.total_input_token_count,
                total_output_token_count = excluded.total_output_token_count,
                account_concurrency_limit = excluded.account_concurrency_limit,
                low_balance_threshold = excluded.low_balance_threshold,
                status = excluded.status,
                source = excluded.source,
                confidence = excluded.confidence,
                collected_at = excluded.collected_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&row.id)
        .bind(&row.input.station_id)
        .bind(normalize_optional(&row.input.station_key_id))
        .bind(row.input.scope.trim())
        .bind(row.input.value)
        .bind(row.input.currency.trim().to_uppercase())
        .bind(normalize_optional(&row.input.credit_unit))
        .bind(row.input.used_value)
        .bind(row.input.total_value)
        .bind(row.input.today_request_count)
        .bind(row.input.total_request_count)
        .bind(row.input.today_consumption)
        .bind(row.input.total_consumption)
        .bind(row.input.today_base_consumption)
        .bind(row.input.total_base_consumption)
        .bind(row.input.today_token_count)
        .bind(row.input.total_token_count)
        .bind(row.input.today_input_token_count)
        .bind(row.input.today_output_token_count)
        .bind(row.input.total_input_token_count)
        .bind(row.input.total_output_token_count)
        .bind(row.input.account_concurrency_limit)
        .bind(row.input.low_balance_threshold)
        .bind(row.input.status.trim())
        .bind(row.input.source.trim())
        .bind(row.input.confidence)
        .bind(normalize_optional(&row.input.collected_at))
        .bind(&row.now)
        .bind(&row.now)
        .execute(write.connection())
        .await?;
        balance_snapshot_by_id(write.connection(), &row.id).await
    }
}

async fn list_stations(
    connection: &mut SqliteConnection,
    limit: i64,
) -> Result<Vec<Station>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        SELECT s.id, s.name, s.station_type, s.website_url, s.api_base_url,
               s.endpoint_revision, s.collector_proxy_mode, s.collector_proxy_url,
               s.api_key, s.api_key_secret_id, sec.masked_value AS api_key_masked,
               (SELECT COUNT(*) FROM station_keys k WHERE k.station_id = s.id) AS key_count,
               s.enabled, s.priority, s.credit_per_cny, s.balance_raw, s.balance_cny,
               s.low_balance_threshold_cny, s.collection_interval_minutes, s.status,
               s.latency_ms, s.last_checked_at, s.last_pricing_fetched_at, s.note,
               s.created_at, s.updated_at
        FROM stations s
        LEFT JOIN secrets sec ON sec.id = s.api_key_secret_id
        ORDER BY s.priority ASC, s.created_at ASC, s.id ASC
        LIMIT ?1
        "#,
    )
    .bind(limit)
    .fetch_all(connection)
    .await?;
    rows.into_iter().map(row_to_station).collect()
}

async fn list_station_keys(
    connection: &mut SqliteConnection,
    limit: i64,
) -> Result<Vec<StationKey>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        SELECT k.id, k.station_id, k.name, k.api_key, k.api_key_secret_id,
               sec.masked_value AS api_key_masked, k.enabled, k.priority,
               k.max_concurrency, k.load_factor, k.schedulable, k.group_name,
               k.tier_label, k.group_binding_id, k.group_id_hash, k.rate_multiplier,
               k.manual_rate_multiplier, k.manual_rate_updated_at, k.rate_source,
               k.rate_collected_at, k.balance_scope, k.status, k.last_checked_at,
               k.last_used_at, k.note, k.created_at, k.updated_at
        FROM station_keys k
        LEFT JOIN secrets sec ON sec.id = k.api_key_secret_id
        ORDER BY k.station_id ASC, k.priority ASC, k.created_at ASC, k.id ASC
        LIMIT ?1
        "#,
    )
    .bind(limit)
    .fetch_all(connection)
    .await?;
    Ok(rows.into_iter().map(row_to_station_key).collect())
}

async fn list_group_bindings(
    connection: &mut SqliteConnection,
    limit: i64,
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
        ORDER BY station_id ASC, binding_kind ASC, group_name ASC, id ASC
        LIMIT ?1
        "#,
    )
    .bind(limit)
    .fetch_all(connection)
    .await?;
    rows.into_iter().map(row_to_group_binding).collect()
}

async fn list_latest_group_rates(
    connection: &mut SqliteConnection,
    limit: i64,
) -> Result<Vec<GroupRateRecord>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        WITH ranked AS (
            SELECT r.*,
                   ROW_NUMBER() OVER (
                       PARTITION BY r.station_id,
                           CASE WHEN r.group_binding_id IS NULL THEN 'group_key' ELSE 'binding' END,
                           COALESCE(r.group_binding_id, r.group_key_hash)
                       ORDER BY r.checked_at DESC, r.created_at DESC, r.id DESC
                   ) AS row_number
            FROM group_rate_records r INDEXED BY idx_group_rate_records_comparison
        )
        SELECT id, station_id, station_key_id, group_binding_id, binding_kind,
               group_key_hash, group_name, default_rate_multiplier, user_rate_multiplier,
               effective_rate_multiplier, inferred_group_category, source, confidence,
               raw_json_redacted, checked_at, created_at
        FROM ranked
        WHERE row_number = 1
        ORDER BY station_id ASC, group_name ASC, checked_at DESC, id DESC
        LIMIT ?1
        "#,
    )
    .bind(limit)
    .fetch_all(connection)
    .await?;
    rows.into_iter().map(row_to_group_rate).collect()
}

async fn list_pricing_rules(
    connection: &mut SqliteConnection,
    limit: i64,
) -> Result<Vec<PricingRule>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        SELECT id, station_id, station_key_id, group_binding_id, group_name,
               tier_label, model, input_price, output_price, fixed_price,
               rate_multiplier, currency, unit, price_type, base_price_source,
               normalization_status, source, confidence, enabled, note,
               collected_at, valid_from, valid_until, created_at, updated_at
        FROM pricing_rules INDEXED BY idx_pricing_rules_comparison
        ORDER BY enabled DESC, station_id ASC, model ASC, updated_at DESC, created_at DESC, id DESC
        LIMIT ?1
        "#,
    )
    .bind(limit)
    .fetch_all(connection)
    .await?;
    Ok(rows.into_iter().map(row_to_pricing_rule).collect())
}

async fn pricing_rule_by_id(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<PricingRule, PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT id, station_id, station_key_id, group_binding_id, group_name,
               tier_label, model, input_price, output_price, fixed_price,
               rate_multiplier, currency, unit, price_type, base_price_source,
               normalization_status, source, confidence, enabled, note,
               collected_at, valid_from, valid_until, created_at, updated_at
        FROM pricing_rules WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(connection)
    .await?;
    row.map(row_to_pricing_rule)
        .ok_or(PersistenceError::Sqlx(sqlx::Error::RowNotFound))
}

async fn model_base_price_by_id(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<ModelBasePrice, PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT id, provider, model, input_price, output_price, currency, unit,
               source_url, source_label, source_checked_at, enabled, built_in,
               note, created_at, updated_at
        FROM model_base_prices WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(connection)
    .await?;
    row.map(row_to_model_base_price)
        .ok_or(PersistenceError::Sqlx(sqlx::Error::RowNotFound))
}

async fn balance_snapshot_by_id(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<BalanceSnapshot, PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT id, station_id, station_key_id, scope, value, currency, credit_unit,
               used_value, total_value, today_request_count, total_request_count,
               today_consumption, total_consumption, today_base_consumption,
               total_base_consumption, today_token_count, total_token_count,
               today_input_token_count, today_output_token_count,
               total_input_token_count, total_output_token_count,
               account_concurrency_limit, low_balance_threshold, status, source,
               confidence, collected_at, created_at, updated_at
        FROM balance_snapshots WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(connection)
    .await?;
    row.map(row_to_balance_snapshot)
        .ok_or(PersistenceError::Sqlx(sqlx::Error::RowNotFound))
}

async fn validate_optional_station_owners(
    connection: &mut SqliteConnection,
    station_id: &str,
    station_key_id: Option<&str>,
    group_binding_id: Option<&str>,
) -> Result<(), PersistenceError> {
    let station_exists =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM stations WHERE id = ?1")
            .bind(station_id)
            .fetch_one(&mut *connection)
            .await?
            == 1;
    if !station_exists {
        return Err(PersistenceError::ConstraintViolation);
    }
    if let Some(station_key_id) = station_key_id {
        let owned = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM station_keys WHERE id = ?1 AND station_id = ?2",
        )
        .bind(station_key_id)
        .bind(station_id)
        .fetch_one(&mut *connection)
        .await?
            == 1;
        if !owned {
            return Err(PersistenceError::ConstraintViolation);
        }
    }
    if let Some(group_binding_id) = group_binding_id {
        let owned = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM station_group_bindings WHERE id = ?1 AND station_id = ?2",
        )
        .bind(group_binding_id)
        .bind(station_id)
        .fetch_one(connection)
        .await?
            == 1;
        if !owned {
            return Err(PersistenceError::ConstraintViolation);
        }
    }
    Ok(())
}

fn validate_pricing_rule(input: &UpsertPricingRuleInput) -> Result<(), PersistenceError> {
    if input.station_id.trim().is_empty()
        || input.model.trim().is_empty()
        || input.currency.trim().is_empty()
        || input.unit.trim().is_empty()
        || input.price_type.trim().is_empty()
        || input.source.trim().is_empty()
        || !valid_confidence(input.confidence)
        || !non_negative_optional(input.input_price)
        || !non_negative_optional(input.output_price)
        || !non_negative_optional(input.fixed_price)
        || input
            .rate_multiplier
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn validate_model_base_price(input: &UpsertModelBasePriceInput) -> Result<(), PersistenceError> {
    if input.provider.trim().is_empty()
        || input.model.trim().is_empty()
        || input.currency.trim().is_empty()
        || input.unit.trim().is_empty()
        || input.source_label.trim().is_empty()
        || !non_negative_optional(input.input_price)
        || !non_negative_optional(input.output_price)
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn validate_balance_snapshot(input: &UpsertBalanceSnapshotInput) -> Result<(), PersistenceError> {
    if input.station_id.trim().is_empty()
        || input.scope.trim().is_empty()
        || input.currency.trim().is_empty()
        || input.status.trim().is_empty()
        || input.source.trim().is_empty()
        || !valid_confidence(input.confidence)
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn valid_confidence(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn non_negative_optional(value: Option<f64>) -> bool {
    value.is_none_or(|value| value.is_finite() && value >= 0.0)
}

fn row_to_station(row: sqlx::sqlite::SqliteRow) -> Result<Station, PersistenceError> {
    let api_key: String = row.get("api_key");
    let secret_id: Option<String> = row.get("api_key_secret_id");
    let collection_interval = u16::try_from(row.get::<i64, _>("collection_interval_minutes"))
        .map_err(|_| PersistenceError::InvariantViolation("station collection interval".into()))?;
    Ok(Station {
        id: row.get("id"),
        name: row.get("name"),
        station_type: row.get("station_type"),
        website_url: row.get("website_url"),
        api_base_url: row.get("api_base_url"),
        endpoint_revision: row.get("endpoint_revision"),
        collector_proxy_mode: row.get("collector_proxy_mode"),
        collector_proxy_url: row.get("collector_proxy_url"),
        api_key_masked: row
            .get::<Option<String>, _>("api_key_masked")
            .unwrap_or_else(|| mask_secret(&api_key)),
        api_key_present: secret_id.is_some() || !api_key.trim().is_empty(),
        key_count: row.get("key_count"),
        enabled: i64_to_bool(row.get("enabled")),
        priority: row.get("priority"),
        credit_per_cny: row.get("credit_per_cny"),
        balance_raw: row.get("balance_raw"),
        balance_cny: row.get("balance_cny"),
        low_balance_threshold_cny: row.get("low_balance_threshold_cny"),
        collection_interval_minutes: collection_interval,
        status: row.get("status"),
        latency_ms: row.get("latency_ms"),
        last_checked_at: row.get("last_checked_at"),
        last_pricing_fetched_at: row.get("last_pricing_fetched_at"),
        note: row.get("note"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn row_to_station_key(row: sqlx::sqlite::SqliteRow) -> StationKey {
    let api_key: String = row.get("api_key");
    let secret_id: Option<String> = row.get("api_key_secret_id");
    StationKey {
        id: row.get("id"),
        station_id: row.get("station_id"),
        name: row.get("name"),
        api_key_masked: row
            .get::<Option<String>, _>("api_key_masked")
            .unwrap_or_else(|| mask_secret(&api_key)),
        api_key_present: secret_id.is_some() || !api_key.trim().is_empty(),
        enabled: i64_to_bool(row.get("enabled")),
        priority: row.get("priority"),
        max_concurrency: row.get("max_concurrency"),
        load_factor: row.get("load_factor"),
        schedulable: i64_to_bool(row.get("schedulable")),
        group_name: row.get("group_name"),
        tier_label: row.get("tier_label"),
        group_binding_id: row.get("group_binding_id"),
        group_id_hash: row.get("group_id_hash"),
        rate_multiplier: row.get("rate_multiplier"),
        manual_rate_multiplier: row.get("manual_rate_multiplier"),
        manual_rate_updated_at: row.get("manual_rate_updated_at"),
        rate_source: row.get("rate_source"),
        rate_collected_at: row.get("rate_collected_at"),
        balance_scope: row.get("balance_scope"),
        status: row.get("status"),
        last_checked_at: row.get("last_checked_at"),
        last_used_at: row.get("last_used_at"),
        note: row.get("note"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_group_binding(
    row: sqlx::sqlite::SqliteRow,
) -> Result<StationGroupBinding, PersistenceError> {
    Ok(StationGroupBinding {
        id: row.get("id"),
        station_id: row.get("station_id"),
        station_key_id: row.get("station_key_id"),
        binding_kind: row.get("binding_kind"),
        parent_group_binding_id: row.get("parent_group_binding_id"),
        group_key_hash: row.get("group_key_hash"),
        group_id_hash: row.get("group_id_hash"),
        group_name: row.get("group_name"),
        binding_status: row.get("binding_status"),
        default_rate_multiplier: row.get("default_rate_multiplier"),
        user_rate_multiplier: row.get("user_rate_multiplier"),
        effective_rate_multiplier: row.get("effective_rate_multiplier"),
        inferred_group_category: row.get("inferred_group_category"),
        group_category_override: row.get("group_category_override"),
        rate_source: row.get("rate_source"),
        confidence: row.get("confidence"),
        last_seen_at: row.get("last_seen_at"),
        last_checked_at: row.get("last_checked_at"),
        last_rate_changed_at: row.get("last_rate_changed_at"),
        raw_json_redacted: parse_optional_json(row.get("raw_json_redacted"))?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn row_to_group_rate(row: sqlx::sqlite::SqliteRow) -> Result<GroupRateRecord, PersistenceError> {
    Ok(GroupRateRecord {
        id: row.get("id"),
        station_id: row.get("station_id"),
        station_key_id: row.get("station_key_id"),
        group_binding_id: row.get("group_binding_id"),
        binding_kind: row.get("binding_kind"),
        group_key_hash: row.get("group_key_hash"),
        group_name: row.get("group_name"),
        default_rate_multiplier: row.get("default_rate_multiplier"),
        user_rate_multiplier: row.get("user_rate_multiplier"),
        effective_rate_multiplier: row.get("effective_rate_multiplier"),
        inferred_group_category: row.get("inferred_group_category"),
        source: row.get("source"),
        confidence: row.get("confidence"),
        raw_json_redacted: parse_optional_json(row.get("raw_json_redacted"))?,
        checked_at: row.get("checked_at"),
        created_at: row.get("created_at"),
    })
}

fn row_to_pricing_rule(row: sqlx::sqlite::SqliteRow) -> PricingRule {
    PricingRule {
        id: row.get("id"),
        station_id: row.get("station_id"),
        station_key_id: row.get("station_key_id"),
        group_binding_id: row.get("group_binding_id"),
        group_name: row.get("group_name"),
        tier_label: row.get("tier_label"),
        model: row.get("model"),
        input_price: row.get("input_price"),
        output_price: row.get("output_price"),
        fixed_price: row.get("fixed_price"),
        rate_multiplier: row.get("rate_multiplier"),
        currency: row.get("currency"),
        unit: row.get("unit"),
        price_type: row.get("price_type"),
        base_price_source: row.get("base_price_source"),
        normalization_status: row.get("normalization_status"),
        source: row.get("source"),
        confidence: row.get("confidence"),
        enabled: i64_to_bool(row.get("enabled")),
        note: row.get("note"),
        collected_at: row.get("collected_at"),
        valid_from: row.get("valid_from"),
        valid_until: row.get("valid_until"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_model_base_price(row: sqlx::sqlite::SqliteRow) -> ModelBasePrice {
    ModelBasePrice {
        id: row.get("id"),
        provider: row.get("provider"),
        model: row.get("model"),
        input_price: row.get("input_price"),
        output_price: row.get("output_price"),
        currency: row.get("currency"),
        unit: row.get("unit"),
        source_url: row.get("source_url"),
        source_label: row.get("source_label"),
        source_checked_at: row.get("source_checked_at"),
        enabled: i64_to_bool(row.get("enabled")),
        built_in: i64_to_bool(row.get("built_in")),
        note: row.get("note"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
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

fn parse_optional_json(
    value: Option<String>,
) -> Result<Option<serde_json::Value>, PersistenceError> {
    value
        .map(|value| {
            serde_json::from_str(&value)
                .map_err(|_| PersistenceError::InvariantViolation("invalid redacted JSON".into()))
        })
        .transpose()
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
