use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TablePolicy {
    InternalRebuild,
    Include,
    IncludeWithTransform,
    OptionalHistory,
    Reset,
    Exclude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataCategory {
    CoreData,
    History,
    SessionCredentials,
    DeviceRuntimeState,
    ProviderDrafts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DependencyStage {
    Internal,
    Stations,
    Secrets,
    StationChildren,
    Routing,
    Pricing,
    History,
    Excluded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldTransform {
    Copy,
    RequireEmpty,
    SecretReference,
    ResetNull,
    ResetText(&'static str),
    RedactText,
    RedactJson,
    BoundedJson,
    ReencryptSecret,
    Exclude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldRule {
    pub(crate) name: &'static str,
    pub(crate) transform: FieldTransform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TableCatalog {
    pub(crate) name: &'static str,
    pub(crate) policy: TablePolicy,
    pub(crate) category: DataCategory,
    pub(crate) dependency_stage: DependencyStage,
    pub(crate) counts_for_occupancy: bool,
    pub(crate) columns: &'static [&'static str],
    pub(crate) field_rules: &'static [FieldRule],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingPolicy {
    Include,
    IncludeWithTransform,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecretPolicy {
    IncludeAndRekey,
    IncludeWhenRememberedAndRekey,
    DeleteAndResetReference,
    ExcludeAndRegenerate,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CatalogError {
    #[error("portable migration catalog has duplicate table declarations")]
    DuplicateTable,
    #[error("portable migration catalog is missing a table policy")]
    MissingTablePolicy(String),
    #[error("portable migration catalog contains an unexpected table")]
    UnexpectedTable(String),
    #[error("portable migration catalog is missing a column policy")]
    MissingColumnPolicy { table: String, column: String },
    #[error("portable migration catalog contains an unexpected column")]
    UnexpectedColumn { table: String, column: String },
    #[error("portable migration catalog is missing a sensitive field rule")]
    MissingSensitiveFieldRule { table: String, column: String },
}

// The v1 catalog describes the post-alerting-cutover user schema. Historical
// `change_events` is intentionally absent; the six alerting tables below are
// the durable replacement and must be recognized by portable migration.
pub(crate) const EXPECTED_USER_TABLE_COUNT_V1: usize = 77;

pub(crate) fn migration_data_catalog() -> &'static [TableCatalog] {
    TABLES
}

pub(crate) fn table_catalog(name: &str) -> Option<&'static TableCatalog> {
    TABLES.iter().find(|table| table.name == name)
}

pub(crate) fn field_rule(table: &str, column: &str) -> Option<FieldTransform> {
    table_catalog(table)?
        .field_rules
        .iter()
        .find(|rule| rule.name == column)
        .map(|rule| rule.transform)
}

pub(crate) fn setting_policy(key: &str) -> Option<SettingPolicy> {
    match key {
        "local_proxy_port"
        | "collector_proxy_mode"
        | "collector_proxy_url"
        | "low_balance_threshold_cny"
        | "collector_interval_minutes"
        | "balance_interval_minutes"
        | "group_rate_interval_minutes"
        | "published_status_interval_minutes"
        | "pricing_refresh_interval_minutes"
        | "collector_timeout_seconds"
        | "collector_max_concurrency"
        | "developer_mode_enabled"
        | "show_decision_explanation"
        | "tray_behavior" => Some(SettingPolicy::Include),
        "default_routing_strategy"
        | "default_routing_group_filter"
        | "max_rate_multiplier"
        | "dispatch_algorithm_profile_json"
        | "allow_depleted_fallback" => Some(SettingPolicy::Reset),
        super::common_login_contract::LEGACY_COMMON_LOGIN_SETTING
        | super::common_login_contract::COMMON_LOGIN_SETTING => {
            Some(SettingPolicy::IncludeWithTransform)
        }
        "local_key" | "local_proxy_start_on_launch" => Some(SettingPolicy::Reset),
        _ => None,
    }
}

pub(crate) fn secret_policy(scope: &str, kind: &str) -> Option<SecretPolicy> {
    match (scope, kind) {
        ("station_key", "api_key") => Some(SecretPolicy::IncludeAndRekey),
        ("station_credentials", "login_password") => {
            Some(SecretPolicy::IncludeWhenRememberedAndRekey)
        }
        ("station_credentials", "access_token")
        | ("station_credentials", "refresh_token")
        | ("station_credentials", "cookie") => Some(SecretPolicy::DeleteAndResetReference),
        (
            super::common_login_contract::LEGACY_PASSWORD_SCOPE,
            super::common_login_contract::PASSWORD_KIND,
        )
        | (
            super::common_login_contract::PASSWORD_SCOPE,
            super::common_login_contract::PASSWORD_KIND,
        ) => Some(SecretPolicy::IncludeAndRekey),
        ("application", "local_proxy_access_key") => Some(SecretPolicy::ExcludeAndRegenerate),
        _ => None,
    }
}

pub(crate) fn validate_schema_snapshot(actual: &[(&str, &[&str])]) -> Result<(), CatalogError> {
    validate_no_duplicate_tables()?;
    let declared_names = TABLES
        .iter()
        .map(|table| table.name)
        .collect::<BTreeSet<_>>();
    let actual_names = actual
        .iter()
        .map(|(name, _)| *name)
        .collect::<BTreeSet<_>>();

    if let Some(name) = actual_names.difference(&declared_names).next() {
        return Err(CatalogError::MissingTablePolicy((*name).to_string()));
    }
    if let Some(name) = declared_names.difference(&actual_names).next() {
        return Err(CatalogError::UnexpectedTable((*name).to_string()));
    }
    for (name, columns) in actual {
        validate_table_columns(name, columns)?;
    }
    validate_sensitive_field_rules()
}

pub(crate) fn validate_table_columns(
    table: &str,
    actual_columns: &[&str],
) -> Result<(), CatalogError> {
    let catalog =
        table_catalog(table).ok_or_else(|| CatalogError::MissingTablePolicy(table.to_string()))?;
    let declared = catalog.columns.iter().copied().collect::<BTreeSet<_>>();
    let actual = actual_columns.iter().copied().collect::<BTreeSet<_>>();
    if let Some(column) = actual.difference(&declared).next() {
        return Err(CatalogError::MissingColumnPolicy {
            table: table.to_string(),
            column: (*column).to_string(),
        });
    }
    if let Some(column) = declared.difference(&actual).next() {
        return Err(CatalogError::UnexpectedColumn {
            table: table.to_string(),
            column: (*column).to_string(),
        });
    }
    Ok(())
}

fn validate_no_duplicate_tables() -> Result<(), CatalogError> {
    let mut seen = BTreeSet::new();
    for table in TABLES {
        if !seen.insert(table.name) {
            return Err(CatalogError::DuplicateTable);
        }
    }
    Ok(())
}

fn validate_sensitive_field_rules() -> Result<(), CatalogError> {
    for table in TABLES {
        let ruled = table
            .field_rules
            .iter()
            .map(|rule| rule.name)
            .collect::<BTreeSet<_>>();
        for column in table.columns {
            if is_sensitive_or_structured_column(column) && !ruled.contains(column) {
                return Err(CatalogError::MissingSensitiveFieldRule {
                    table: table.name.to_string(),
                    column: (*column).to_string(),
                });
            }
        }
    }
    Ok(())
}

fn is_sensitive_or_structured_column(column: &str) -> bool {
    let lower = column.to_ascii_lowercase();
    if is_token_usage_metric_column(&lower) {
        return false;
    }
    lower.contains("api_key")
        || lower.contains("secret")
        || lower.contains("token")
        || lower.contains("cookie")
        || lower.contains("password")
        || lower.ends_with("_json")
        || lower == "json"
        || lower.contains("error_message")
        || lower.contains("error_summary")
        || lower == "message"
        || lower == "sanitized_detail"
}

fn is_token_usage_metric_column(column: &str) -> bool {
    matches!(
        column,
        "token_count"
            | "today_token_count"
            | "total_token_count"
            | "today_input_token_count"
            | "today_output_token_count"
            | "total_input_token_count"
            | "total_output_token_count"
            | "input_tokens"
            | "output_tokens"
            | "prompt_tokens"
            | "completion_tokens"
            | "total_tokens"
            | "cache_creation_tokens"
            | "cache_read_tokens"
            | "first_token_ms"
    )
}

const SCHEMA_COMPATIBILITY_COLUMNS: &[&str] = &[
    "singleton_key",
    "database_generation",
    "schema_version",
    "min_reader_app_version",
    "min_writer_app_version",
    "updated_by_migration",
    "updated_at",
];
const RUNTIME_HEALTH_COLUMNS: &[&str] = &[
    "singleton_key",
    "write_probe_count",
    "last_open_mode",
    "last_checked_at",
];
const SETTINGS_COLUMNS: &[&str] = &["key", "value", "updated_at"];
const SECRETS_COLUMNS: &[&str] = &[
    "id",
    "scope",
    "owner_id",
    "kind",
    "masked_value",
    "ciphertext",
    "nonce",
    "created_at",
    "updated_at",
    "key_id",
    "encryption_version",
    "value_hash",
];
const STATIONS_COLUMNS: &[&str] = &[
    "id",
    "name",
    "station_type",
    "website_url",
    "api_base_url",
    "endpoint_revision",
    "api_key",
    "api_key_secret_id",
    "upstream_api_format",
    "collector_proxy_mode",
    "collector_proxy_url",
    "enabled",
    "priority",
    "credit_per_cny",
    "balance_raw",
    "balance_cny",
    "low_balance_threshold_cny",
    "collection_interval_minutes",
    "status",
    "latency_ms",
    "last_checked_at",
    "last_pricing_fetched_at",
    "note",
    "created_at",
    "updated_at",
];
const STATION_CAPACITY_DOMAINS_COLUMNS: &[&str] = &[
    "station_id",
    "provider_family",
    "deployment_identity",
    "region_identity",
    "revision",
    "updated_at",
];
const STATION_KEYS_COLUMNS: &[&str] = &[
    "id",
    "station_id",
    "name",
    "api_key",
    "api_key_secret_id",
    "enabled",
    "priority",
    "max_concurrency",
    "load_factor",
    "schedulable",
    "group_name",
    "tier_label",
    "group_binding_id",
    "group_id_hash",
    "rate_multiplier",
    "manual_rate_multiplier",
    "manual_rate_updated_at",
    "rate_source",
    "rate_collected_at",
    "balance_scope",
    "status",
    "last_checked_at",
    "last_used_at",
    "note",
    "created_at",
    "updated_at",
    "routing_order",
];
const STATION_CREDENTIALS_COLUMNS: &[&str] = &[
    "station_id",
    "login_password",
    "login_password_secret_id",
    "remember_password",
    "login_status",
    "login_error",
    "last_login_at",
    "session_status",
    "session_expires_at",
    "access_token_secret_id",
    "refresh_token_secret_id",
    "cookie_secret_id",
    "newapi_user_id",
    "token_expires_at",
    "token_refreshed_at",
    "session_source",
    "session_user_agent",
    "updated_at",
    "login_username",
    "created_at",
];
const REMOTE_STATION_KEYS_COLUMNS: &[&str] = &[
    "id",
    "station_id",
    "remote_key_id_hash",
    "remote_key_name",
    "api_key_masked",
    "api_key_fingerprint",
    "group_id_hash",
    "group_name",
    "tier_label",
    "rate_multiplier",
    "rate_source",
    "created_at",
    "last_used_at",
    "raw_source",
    "match_status",
    "matched_station_key_id",
    "match_confidence",
    "collected_at",
    "updated_at",
    "discovery_order",
];
const STATION_KEY_CAPABILITIES_COLUMNS: &[&str] = &[
    "station_key_id",
    "supports_chat_completions",
    "supports_responses",
    "supports_embeddings",
    "supports_stream",
    "supports_tools",
    "supports_vision",
    "supports_reasoning",
    "model_allowlist_json",
    "model_blocklist_json",
    "preferred_models_json",
    "only_use_as_backup",
    "routing_tags_json",
    "updated_at",
];

const MODEL_MAPPING_POLICIES_COLUMNS: &[&str] = &[
    "singleton_key",
    "revision",
    "unmatched_model_behavior",
    "updated_at_ms",
];
const MODEL_MAPPING_RULES_COLUMNS: &[&str] = &[
    "id",
    "priority",
    "enabled",
    "matcher_kind",
    "matcher_value",
    "endpoint_conditions_json",
    "stream_condition",
    "tools_condition",
    "vision_condition",
    "reasoning_condition",
    "action_kind",
    "fallback_trigger",
    "rejection_kind",
    "rejection_message",
    "note",
    "created_at_ms",
    "updated_at_ms",
    "revision",
];
const MODEL_MAPPING_RULE_TARGETS_COLUMNS: &[&str] = &[
    "id",
    "rule_id",
    "position",
    "target_kind",
    "literal_upstream_model",
    "model_profile_id",
];
const MODEL_PROFILES_COLUMNS: &[&str] = &[
    "id",
    "canonical_model",
    "display_name",
    "default_upstream_model",
    "status",
    "note",
    "created_at_ms",
    "updated_at_ms",
    "revision",
];
const MODEL_OFFERING_BINDINGS_COLUMNS: &[&str] = &[
    "id",
    "model_profile_id",
    "station_key_id",
    "station_id",
    "upstream_model",
    "source",
    "enabled",
    "note",
    "created_at_ms",
    "updated_at_ms",
    "revision",
];
const LEGACY_MODEL_ALIAS_MIGRATION_REVIEWS_COLUMNS: &[&str] = &[
    "id",
    "legacy_alias_id",
    "requested_model",
    "selected_target",
    "discarded_target",
    "migration_status",
    "created_at_ms",
];
const MODEL_MAPPING_DOCUMENT_HISTORY_COLUMNS: &[&str] =
    &["revision", "document_json", "source", "created_at_ms"];
const ROUTING_DOCUMENT_SYNC_COLUMNS: &[&str] = &[
    "document_kind",
    "desired_revision",
    "desired_canonical_digest",
    "materialized_revision",
    "materialized_canonical_digest",
    "sync_state",
    "last_observed_raw_digest",
    "last_error_code",
    "retry_count",
    "attempt_token",
    "lease_owner",
    "lease_expires_at_ms",
    "updated_at_ms",
];

const STATION_ENDPOINT_HEALTH_COLUMNS: &[&str] = &[
    "station_id",
    "endpoint_revision",
    "status",
    "latency_ms",
    "checked_at",
    "error_summary",
    "updated_at",
];

const STATION_KEY_HEALTH_COLUMNS: &[&str] = &[
    "station_key_id",
    "endpoint_revision",
    "last_success_at",
    "last_failure_at",
    "consecutive_failures",
    "success_count",
    "failure_count",
    "total_duration_ms",
    "avg_latency_ms",
    "last_error_summary",
    "cooldown_until",
    "updated_at",
];

const STATION_ENDPOINT_HEALTH_RULES: &[FieldRule] = &[FieldRule {
    name: "error_summary",
    transform: FieldTransform::RedactText,
}];

const STATION_KEY_HEALTH_RULES: &[FieldRule] = &[FieldRule {
    name: "last_error_summary",
    transform: FieldTransform::RedactText,
}];
const MODEL_ALIASES_COLUMNS: &[&str] = &[
    "id",
    "client_model",
    "upstream_model",
    "enabled",
    "note",
    "created_at",
    "updated_at",
];
const BALANCE_SNAPSHOTS_COLUMNS: &[&str] = &[
    "id",
    "station_id",
    "station_key_id",
    "scope",
    "value",
    "currency",
    "credit_unit",
    "used_value",
    "total_value",
    "today_request_count",
    "total_request_count",
    "today_consumption",
    "total_consumption",
    "today_base_consumption",
    "total_base_consumption",
    "today_token_count",
    "total_token_count",
    "today_input_token_count",
    "today_output_token_count",
    "total_input_token_count",
    "total_output_token_count",
    "account_concurrency_limit",
    "low_balance_threshold",
    "status",
    "source",
    "confidence",
    "collected_at",
    "created_at",
    "updated_at",
    "evidence_confidence",
    "spendability_authority",
    "observed_at_ms",
    "valid_until_ms",
    "evidence_profile_version",
    "spendability_reason_code",
];
const REQUEST_LOGS_COLUMNS: &[&str] = &[
    "id",
    "request_id",
    "started_at",
    "received_at_ms",
    "finished_at",
    "duration_ms",
    "method",
    "path",
    "endpoint",
    "model",
    "stream",
    "status",
    "lifecycle_status",
    "station_key_id",
    "station_id",
    "upstream_base_url",
    "fallback_count",
    "error_message",
    "route_policy",
    "route_reason",
    "rejected_candidates_json",
    "body_bytes",
    "attempt_count",
    "route_wait_ms",
    "upstream_headers_ms",
    "failure_source",
    "attempts_json",
    "completion_source",
    "prompt_tokens",
    "completion_tokens",
    "total_tokens",
    "cache_creation_tokens",
    "cache_read_tokens",
    "reasoning_effort",
    "first_token_ms",
    "terminal_kind",
    "terminal_code",
    "terminal_detail",
    "protocol_completed",
    "delivery_terminal",
    "selected_attempt_ordinal",
    "terminal_at_ms",
    "created_at",
    "billing_mode",
    "estimated_input_cost",
    "estimated_output_cost",
    "estimated_total_cost",
    "cost_currency",
    "pricing_source",
    "cost_status",
    "usage_status",
    "group_binding_id",
    "normalization_status",
    "balance_scope",
    "economic_context_json",
    "http_status",
];
const REQUEST_ATTEMPTS_COLUMNS: &[&str] = &[
    "request_id",
    "ordinal",
    "station_id",
    "station_key_id",
    "endpoint_revision",
    "started_at_ms",
    "terminal_kind",
    "failure_kind",
    "failure_blame",
    "retry_disposition",
    "health_effect",
    "health_cooldown_until_ms",
    "public_code",
    "sanitized_detail",
    "output_committed",
    "terminal_at_ms",
];
const REQUEST_LOG_URL_SANITIZER_PROGRESS_COLUMNS: &[&str] = &[
    "id",
    "status",
    "sanitized_count",
    "redacted_unparseable_count",
    "redacted_non_http_count",
    "last_request_log_id",
    "last_reason",
    "updated_at",
];
const ROUTE_DECISIONS_COLUMNS: &[&str] = &[
    "id",
    "request_id",
    "decided_at_ms",
    "ordering_profile",
    "selected_station_key_id",
    "selected_station_id",
    "selected_endpoint_revision",
    "candidate_count",
    "candidate_detail_count",
    "candidate_detail_truncated",
    "rejection_counts_json",
    "snapshot_id",
    "fact_version_vector",
    "planner_version",
    "projector_version",
    "runtime_overlay_revision",
    "trace_status",
    "created_at_ms",
    "updated_at_ms",
];
const ROUTE_CANDIDATE_DECISIONS_COLUMNS: &[&str] = &[
    "id",
    "decision_id",
    "request_id",
    "station_key_id",
    "station_id",
    "endpoint_revision",
    "selected",
    "attempted",
    "retained_reason",
    "availability_tier",
    "hard_rejection_code",
    "hard_rejection_gate",
    "priority",
    "cost_basis",
    "cost_currency",
    "cost_unit",
    "cost_comparison_value",
    "snapshot_id",
    "fact_version_vector",
    "evidence_json",
    "created_at_ms",
];
const ROUTING_ATTEMPT_COSTS_COLUMNS: &[&str] = &[
    "request_id",
    "ordinal",
    "pricing_context_id",
    "pricing_basis",
    "pricing_status_label",
    "usage_status",
    "input_tokens",
    "output_tokens",
    "total_tokens",
    "cache_creation_tokens",
    "cache_read_tokens",
    "cost_status",
    "currency",
    "total_cost_micro",
    "created_at_ms",
];
const ROUTING_REQUEST_COST_AGGREGATES_COLUMNS: &[&str] = &[
    "request_id",
    "status",
    "totals_by_currency_json",
    "compatibility_currency",
    "compatibility_total_cost_micro",
    "incomplete_attempts_json",
    "created_at_ms",
    "updated_at_ms",
];
const REQUEST_ROUTING_OUTCOME_SUMMARIES_COLUMNS: &[&str] = &[
    "request_id",
    "profile_version",
    "terminal_kind",
    "terminal_code",
    "classification",
    "confidence",
    "evidence_source",
    "request_accepted",
    "send_phase",
    "replay_disposition",
    "billing_state",
    "retry_disposition",
    "effect_summary",
    "failure_domain_commitment_version",
    "failure_domain_commitment_digest",
    "attempt_count",
    "fallback_count",
    "terminal_at_ms",
];
const REQUEST_DECISION_EVENTS_COLUMNS: &[&str] = &[
    "request_id",
    "event_key",
    "sequence",
    "occurred_at_ms",
    "event_kind",
    "detail_code",
    "attempt_ordinal",
    "retry_disposition",
    "output_committed",
];
const ROUTING_ERROR_RATE_HISTORY_COLUMNS: &[&str] = &[
    "ingestion_sequence",
    "observation_id",
    "observed_at_ms",
    "scope_kind",
    "scope_commitment",
    "outcome",
    "failure_code",
    "sample_count",
    "failure_count",
    "failure_rate_percent",
    "transition",
    "created_at_ms",
];
const ROUTING_ERROR_RATE_HISTORY_META_COLUMNS: &[&str] =
    &["singleton_key", "dropped_events", "updated_at_ms"];
const ROUTING_LIFECYCLE_RECONCILIATION_PROGRESS_COLUMNS: &[&str] = &[
    "singleton_key",
    "last_request_id",
    "last_run_at_ms",
    "batches_completed",
    "requests_interrupted",
    "attempt_cost_gaps_inserted",
    "decisions_marked_trace_incomplete",
    "completed",
];
const REQUEST_TERMINAL_OUTBOX_COLUMNS: &[&str] = &[
    "request_id",
    "payload_json",
    "payload_sha256",
    "created_at_ms",
    "lease_owner",
    "lease_expires_at_ms",
    "attempts",
];
const REQUEST_TERMINAL_OUTBOX_RULES: &[FieldRule] = &[FieldRule {
    // The outbox is reset during export/import, but the catalog still requires
    // an explicit policy for structured payloads so a future policy change
    // cannot accidentally copy an unreviewed terminal body.
    name: "payload_json",
    transform: FieldTransform::Exclude,
}];
const COLLECTOR_RUNS_COLUMNS: &[&str] = &[
    "id",
    "run_key",
    "request_hash",
    "station_id",
    "endpoint_revision",
    "parent_run_id",
    "adapter",
    "task_type",
    "status",
    "started_at",
    "finished_at",
    "duration_ms",
    "endpoint_count",
    "success_count",
    "failure_count",
    "manual_action_required",
    "error_code",
    "error_message",
    "snapshot_id",
    "created_at",
];
const COLLECTOR_SNAPSHOTS_COLUMNS: &[&str] = &[
    "id",
    "run_id",
    "station_id",
    "endpoint_revision",
    "source",
    "status",
    "fetched_at",
    "summary_json",
    "normalized_json",
    "raw_json_redacted",
    "error_message",
    "created_at",
];
const STATION_GROUP_BINDINGS_COLUMNS: &[&str] = &[
    "id",
    "station_id",
    "station_key_id",
    "binding_kind",
    "parent_group_binding_id",
    "group_key_hash",
    "group_id_hash",
    "group_name",
    "binding_status",
    "default_rate_multiplier",
    "user_rate_multiplier",
    "effective_rate_multiplier",
    "inferred_group_category",
    "group_category_override",
    "rate_source",
    "confidence",
    "last_seen_at",
    "last_checked_at",
    "last_rate_changed_at",
    "last_seen_run_id",
    "raw_json_redacted",
    "created_at",
    "updated_at",
];
const GROUP_RATE_RECORDS_COLUMNS: &[&str] = &[
    "id",
    "station_id",
    "station_key_id",
    "group_binding_id",
    "binding_kind",
    "group_key_hash",
    "group_name",
    "default_rate_multiplier",
    "user_rate_multiplier",
    "effective_rate_multiplier",
    "inferred_group_category",
    "source",
    "confidence",
    "raw_json_redacted",
    "checked_at",
    "created_at",
];
const COLLECTOR_MODEL_FACTS_COLUMNS: &[&str] = &[
    "station_id",
    "model",
    "available",
    "source",
    "confidence",
    "last_seen_run_id",
    "updated_at",
];
const COLLECTOR_TASK_STATE_COLUMNS: &[&str] = &[
    "station_id",
    "task_type",
    "last_run_id",
    "last_status",
    "last_success_at",
    "last_failure_at",
    "consecutive_failures",
    "next_due_at",
    "updated_at",
];
const STATION_PUBLISHED_STATUS_SOURCES_COLUMNS: &[&str] = &[
    "station_id",
    "endpoint_revision",
    "source_kind",
    "source_state",
    "last_attempt_at",
    "last_success_at",
    "last_complete_at",
    "last_error_kind",
    "monitor_count",
    "created_at",
    "updated_at",
];
const STATION_PUBLISHED_MONITORS_COLUMNS: &[&str] = &[
    "id",
    "station_id",
    "endpoint_revision",
    "source_kind",
    "upstream_monitor_id",
    "identity_kind",
    "name",
    "provider",
    "group_name",
    "primary_model",
    "extra_models_json",
    "presence_status",
    "current_outcome",
    "source_status",
    "current_latency_ms",
    "current_ping_latency_ms",
    "availability_7d_percent",
    "upstream_checked_at",
    "last_seen_run_id",
    "last_seen_at",
    "created_at",
    "updated_at",
];
const STATION_PUBLISHED_MONITOR_SAMPLES_COLUMNS: &[&str] = &[
    "id",
    "monitor_id",
    "model",
    "checked_at",
    "outcome",
    "source_status",
    "latency_ms",
    "ping_latency_ms",
    "safe_message",
    "first_seen_run_id",
    "last_seen_run_id",
    "created_at",
    "updated_at",
];
const STATION_PUBLISHED_STATUS_SOURCE_RULES: &[FieldRule] = &[FieldRule {
    name: "last_error_kind",
    transform: FieldTransform::RedactText,
}];
const STATION_PUBLISHED_MONITOR_RULES: &[FieldRule] = &[FieldRule {
    name: "extra_models_json",
    transform: FieldTransform::BoundedJson,
}];
const STATION_PUBLISHED_MONITOR_SAMPLE_RULES: &[FieldRule] = &[FieldRule {
    name: "safe_message",
    transform: FieldTransform::RedactText,
}];
const ALERT_POLICIES_COLUMNS: &[&str] = &[
    "id",
    "name",
    "enabled",
    "state",
    "scope_kind",
    "event_type",
    "station_id",
    "station_key_id",
    "minimum_severity",
    "severity_offset",
    "trigger_mode",
    "trigger_count",
    "trigger_duration_seconds",
    "recovery_mode",
    "recovery_count",
    "recovery_duration_seconds",
    "in_app_enabled",
    "desktop_enabled",
    "repeat_mode",
    "repeat_interval_seconds",
    "cooldown_seconds",
    "recovery_notification_enabled",
    "quiet_hours_policy",
    "priority",
    "revision",
    "created_at_ms",
    "updated_at_ms",
];
const CHANGE_INCIDENTS_COLUMNS: &[&str] = &[
    "id",
    "condition_key",
    "event_type",
    "lifecycle_state",
    "base_severity",
    "severity",
    "object_type",
    "object_id",
    "station_id",
    "station_key_id",
    "policy_id",
    "policy_revision",
    "lifecycle_policy_fingerprint",
    "episode_number",
    "first_seen_at_ms",
    "last_seen_at_ms",
    "opened_at_ms",
    "recovering_at_ms",
    "resolved_at_ms",
    "occurrence_count",
    "episode_occurrence_count",
    "consecutive_abnormal_count",
    "consecutive_healthy_count",
    "pending_since_ms",
    "healthy_since_ms",
    "last_observation_id",
    "last_observation_summary_json",
    "fact_fresh_until_ms",
    "next_state_evaluation_at_ms",
    "last_notification_at_ms",
    "next_notification_at_ms",
    "version",
    "created_at_ms",
    "updated_at_ms",
];
const CHANGE_EVENT_OCCURRENCES_COLUMNS: &[&str] = &[
    "id",
    "source_observation_key",
    "event_type",
    "category",
    "observation_kind",
    "severity",
    "condition_key",
    "incident_id",
    "episode_number",
    "object_type",
    "object_id",
    "station_id",
    "station_key_id",
    "request_log_id",
    "source",
    "reason_code",
    "old_value_json",
    "new_value_json",
    "impact_json",
    "observed_at_ms",
    "created_at_ms",
    "seen_at_ms",
];
const INCIDENT_ATTENTION_COLUMNS: &[&str] = &[
    "incident_id",
    "episode_number",
    "seen_at_ms",
    "acknowledged_at_ms",
    "acknowledged_reason",
    "snoozed_until_ms",
    "updated_at_ms",
];
const NOTIFICATION_DELIVERIES_COLUMNS: &[&str] = &[
    "id",
    "delivery_key",
    "incident_id",
    "episode_number",
    "delivery_sequence",
    "policy_id",
    "policy_revision",
    "policy_snapshot_json",
    "channel",
    "delivery_kind",
    "status",
    "scheduled_at_ms",
    "claim_token",
    "claimed_at_ms",
    "lease_expires_at_ms",
    "attempt_count",
    "attempted_at_ms",
    "outcome_unknown_at_ms",
    "retry_not_before_ms",
    "delivered_at_ms",
    "suppressed_reason",
    "error_code",
    "created_at_ms",
    "updated_at_ms",
];
const ALERTING_UPGRADE_PROGRESS_COLUMNS: &[&str] = &[
    "singleton_key",
    "phase",
    "source_high_water_cursor",
    "last_copied_cursor",
    "copied_count",
    "rebuild_version",
    "last_error_code",
    "started_at_ms",
    "updated_at_ms",
    "completed_at_ms",
];
const MODEL_BASE_PRICES_COLUMNS: &[&str] = &[
    "id",
    "provider",
    "model",
    "input_price",
    "output_price",
    "currency",
    "unit",
    "source_url",
    "source_label",
    "source_checked_at",
    "enabled",
    "built_in",
    "note",
    "created_at",
    "updated_at",
    "input_price_priority",
    "output_price_priority",
    "cache_creation_price",
    "cache_creation_price_priority",
    "cache_creation_price_above_1hr",
    "cache_read_price",
    "cache_read_price_priority",
    "long_context_input_token_threshold",
    "long_context_input_cost_multiplier",
    "long_context_output_cost_multiplier",
    "supports_service_tier",
    "supports_prompt_caching",
];
const CHANNEL_MONITOR_TEMPLATE_COLUMNS: &[&str] = &[
    "id",
    "name",
    "endpoint_kind",
    "method",
    "path",
    "request_body_json",
    "enabled",
    "built_in",
    "note",
    "created_at",
    "updated_at",
];
const CHANNEL_MONITORS_COLUMNS: &[&str] = &[
    "id",
    "name",
    "target_type",
    "station_id",
    "station_key_id",
    "template_id",
    "enabled",
    "interval_seconds",
    "jitter_seconds",
    "timeout_seconds",
    "max_concurrency",
    "consecutive_failure_threshold",
    "fallback_models_json",
    "last_run_at",
    "last_run_id",
    "next_run_at",
    "last_status",
    "last_error_message",
    "note",
    "created_at",
    "updated_at",
    "protocol_kind",
    "client_profile_id",
    "client_profile_version",
    "primary_model",
    "fallback_models_v2_json",
    "retry_max_attempts_per_model",
    "retry_initial_backoff_ms",
    "retry_max_backoff_ms",
    "risk_daily_probe_budget",
    "health_policy_mode",
    "health_failure_threshold",
    "health_recovery_threshold",
    "attempt_timeout_ms",
    "execution_timeout_ms",
    "schedule_revision",
    "next_due_at_ms",
    "pause_on_zero_balance",
    "proxy_mode",
    "proxy_url",
];
const CHANNEL_MONITOR_EXECUTIONS_COLUMNS: &[&str] = &[
    "id",
    "monitor_id",
    "trigger_kind",
    "trigger_request_id",
    "status",
    "planned_at_ms",
    "started_at_ms",
    "finished_at_ms",
    "schedule_lag_ms",
    "config_revision",
    "config_snapshot_hash",
    "endpoint_revision",
    "target_count",
    "available_count",
    "degraded_count",
    "unavailable_count",
    "skipped_count",
    "summary_outcome",
    "summary_failure_kind",
    "created_at_ms",
];
const CHANNEL_MONITOR_ATTEMPTS_COLUMNS: &[&str] = &[
    "id",
    "execution_id",
    "monitor_id",
    "station_id",
    "station_key_id",
    "endpoint_revision",
    "model",
    "model_role",
    "model_index",
    "attempt_number",
    "protocol_kind",
    "client_profile_id",
    "client_profile_version",
    "request_profile_hash",
    "transport_mode",
    "started_at_ms",
    "headers_received_at_ms",
    "first_content_at_ms",
    "finished_at_ms",
    "latency_ms",
    "ttfb_ms",
    "first_content_ms",
    "http_status",
    "outcome",
    "failure_kind",
    "retryable",
    "retry_after_ms",
    "response_model",
    "content_extracted",
    "validation_kind",
    "validation_passed",
    "output_bytes",
    "input_tokens",
    "output_tokens",
    "total_tokens",
    "error_summary",
    "canonical_failure_class",
    "failure_origin",
    "failure_scope_kind",
    "failure_dimension",
    "evidence_code",
    "evidence_confidence",
    "classifier_profile_version",
    "created_at_ms",
];
const CHANNEL_MONITOR_TARGET_RESULTS_COLUMNS: &[&str] = &[
    "id",
    "execution_id",
    "monitor_id",
    "station_id",
    "station_key_id",
    "endpoint_revision",
    "terminal_outcome",
    "terminal_failure_kind",
    "terminal_reason",
    "requested_model",
    "effective_model",
    "used_fallback",
    "attempt_count",
    "decisive_attempt_id",
    "protocol_kind",
    "resolved_adapter_kind",
    "resolved_dialect",
    "client_profile_id",
    "client_profile_version",
    "request_profile_hash",
    "traffic_equivalence",
    "latency_ms",
    "availability_eligible",
    "latency_eligible",
    "exclusion_reason",
    "technical_health_effect",
    "disposition_profile_version",
    "ttfb_ms",
    "first_content_ms",
    "semantic_confidence",
    "started_at_ms",
    "finished_at_ms",
    "created_at_ms",
];
const CHANNEL_MONITOR_BUCKET_ROLLUPS_COLUMNS: &[&str] = &[
    "id",
    "monitor_id",
    "station_key_id",
    "bucket_kind",
    "bucket_start_ms",
    "bucket_end_ms",
    "total_count",
    "available_count",
    "degraded_count",
    "unavailable_count",
    "skipped_count",
    "excluded_count",
    "exclusion_counts_json",
    "failure_counts_json",
    "p50_latency_ms",
    "p95_latency_ms",
    "updated_at_ms",
];
const CHANNEL_MONITOR_ROLLUP_DIRTY_RANGES_COLUMNS: &[&str] = &[
    "id",
    "monitor_id",
    "station_key_id",
    "range_start_ms",
    "range_end_ms",
    "reason",
    "created_at_ms",
];
const STATION_KEY_HEALTH_OBSERVATIONS_COLUMNS: &[&str] = &[
    "id",
    "station_key_id",
    "target_result_id",
    "source",
    "source_event_id",
    "observed_at_ms",
    "endpoint_revision",
    "outcome",
    "failure_kind",
    "latency_ms",
    "retry_after_ms",
    "traffic_equivalence",
    "error_summary",
    "writeback_decision",
    "created_at_ms",
];
const CHANNEL_MONITOR_PROBE_BUDGET_USAGE_COLUMNS: &[&str] = &[
    "id",
    "monitor_id",
    "station_key_id",
    "budget_window_start_ms",
    "budget_window_end_ms",
    "attempt_count",
    "updated_at_ms",
];
const PROVIDER_DRAFTS_COLUMNS: &[&str] = &[
    "id",
    "base_station_id",
    "revision",
    "state",
    "payload_schema_version",
    "payload_json",
    "commit_key",
    "committed_station_id",
    "created_at",
    "updated_at",
    "expires_at",
];
const PROVIDER_DRAFT_PREVIEWS_COLUMNS: &[&str] = &[
    "draft_id",
    "kind",
    "runtime_fingerprint",
    "status",
    "result_json",
    "collected_at",
    "updated_at",
];
const APP_SECRET_BINDINGS_COLUMNS: &[&str] = &[
    "binding_scope",
    "binding_owner_id",
    "binding_kind",
    "secret_id",
    "created_at",
    "updated_at",
];
const DOMAIN_REVISIONS_COLUMNS: &[&str] = &["scope", "revision", "updated_at_ms", "provenance"];
const ROUTING_POLICY_COLUMNS: &[&str] = &[
    "singleton_key",
    "config_json",
    "config_revision",
    "policy_version",
    "system_version",
    "status",
    "created_at_ms",
    "updated_at_ms",
];
const ROUTING_POLICY_HISTORY_COLUMNS: &[&str] = &[
    "config_revision",
    "config_json",
    "policy_version",
    "system_version",
    "status",
    "created_at_ms",
];
const ROUTING_OBSERVATIONS_COLUMNS: &[&str] = &[
    "id",
    "producer_id",
    "producer_sequence",
    "payload_hash",
    "event_at_ms",
    "ingested_at_ms",
    "scope",
    "source",
    "traffic_equivalence",
    "outcome_kind",
    "latency_ms",
    "mass_basis_points",
    "evidence_json",
    "created_at_ms",
];
const ROUTING_PROJECTOR_CHECKPOINTS_COLUMNS: &[&str] = &[
    "projector",
    "projector_version",
    "scope",
    "checkpoint_sequence",
    "status",
    "error_code",
    "updated_at_ms",
];
const ROUTING_QUALITY_SUMMARIES_COLUMNS: &[&str] =
    &["scope", "quality_revision", "summary_json", "updated_at_ms"];
const ROUTING_HEALTH_AXES_COLUMNS: &[&str] = &[
    "scope",
    "axis",
    "health_revision",
    "value_basis_points",
    "updated_at_ms",
];
const ROUTING_HEALTH_GENERATIONS_COLUMNS: &[&str] = &[
    "generation_id",
    "projector_version",
    "status",
    "watermark_ingested_at_ms",
    "watermark_ingestion_sequence",
    "watermark_observation_id",
    "projected_row_count",
    "projected_content_hash",
    "created_at_ms",
    "activated_at_ms",
];
const ROUTING_HEALTH_OBSERVATIONS_COLUMNS: &[&str] = &[
    "ingestion_sequence",
    "observation_id",
    "producer_id",
    "producer_sequence",
    "payload_hash",
    "logical_request_id",
    "attempt_ordinal",
    "terminal_kind",
    "ingested_at_ms",
    "scope",
    "scope_kind",
    "failure_dimension",
    "station_id",
    "station_key_id",
    "group_binding_id",
    "resolved_model_commitment",
    "credential_revision",
    "account_revision",
    "group_revision",
    "endpoint_revision",
    "model_alias_revision",
    "verdict",
    "cooldown_until_ms",
    "evidence_code",
    "projector_profile_version",
    "created_at_ms",
];
const ROUTING_HEALTH_VERDICTS_COLUMNS: &[&str] = &[
    "generation_id",
    "scope",
    "scope_kind",
    "failure_dimension",
    "station_id",
    "station_key_id",
    "group_binding_id",
    "resolved_model_commitment",
    "credential_revision",
    "account_revision",
    "group_revision",
    "endpoint_revision",
    "model_alias_revision",
    "verdict",
    "cooldown_until_ms",
    "evidence_code",
    "source_observation_id",
    "source_ingested_at_ms",
    "source_ingestion_sequence",
    "projector_version",
    "updated_at_ms",
];
const ROUTING_HEALTH_PROJECTOR_STATE_COLUMNS: &[&str] = &[
    "singleton_key",
    "projector_version",
    "active_generation_id",
    "watermark_ingested_at_ms",
    "watermark_ingestion_sequence",
    "watermark_observation_id",
    "updated_at_ms",
];
const ROUTING_HEALTH_PROTECTION_STATE_COLUMNS: &[&str] = &[
    "singleton_key",
    "profile_version",
    "profile_json",
    "snapshot_version",
    "snapshot_json",
    "content_hash",
    "generated_at_ms",
    "updated_at_ms",
];
const ROUTING_HEALTH_PROTECTION_STATE_RULES: &[FieldRule] = &[
    FieldRule {
        name: "profile_json",
        transform: FieldTransform::Exclude,
    },
    FieldRule {
        name: "snapshot_json",
        transform: FieldTransform::Exclude,
    },
];
const ROUTING_CAPABILITY_MODEL_OBSERVATIONS_COLUMNS: &[&str] = &[
    "ingestion_sequence",
    "observation_id",
    "payload_hash",
    "logical_request_id",
    "attempt_ordinal",
    "station_key_id",
    "resolved_model",
    "credential_revision",
    "endpoint_revision",
    "model_alias_revision",
    "endpoint_kind",
    "protocol_kind",
    "identity_version",
    "model_mapping_revision",
    "model_resolution_fence",
    "verdict",
    "evidence_code",
    "classifier_profile_version",
    "created_at_ms",
];
const ROUTING_CAPABILITY_MODEL_VERDICTS_COLUMNS: &[&str] = &[
    "station_key_id",
    "resolved_model",
    "credential_revision",
    "endpoint_revision",
    "model_alias_revision",
    "endpoint_kind",
    "protocol_kind",
    "identity_version",
    "model_mapping_revision",
    "model_resolution_fence",
    "verdict",
    "source_observation_id",
    "source_ingestion_sequence",
    "projector_version",
    "updated_at_ms",
];

const SETTINGS_RULES: &[FieldRule] = &[];
const SECRETS_RULES: &[FieldRule] = &[
    FieldRule {
        name: "ciphertext",
        transform: FieldTransform::ReencryptSecret,
    },
    FieldRule {
        name: "nonce",
        transform: FieldTransform::ReencryptSecret,
    },
    FieldRule {
        name: "masked_value",
        transform: FieldTransform::Copy,
    },
    FieldRule {
        name: "key_id",
        transform: FieldTransform::ReencryptSecret,
    },
    FieldRule {
        name: "value_hash",
        transform: FieldTransform::ReencryptSecret,
    },
];
const STATIONS_RULES: &[FieldRule] = &[
    FieldRule {
        name: "api_key",
        transform: FieldTransform::RequireEmpty,
    },
    FieldRule {
        name: "api_key_secret_id",
        transform: FieldTransform::SecretReference,
    },
    FieldRule {
        name: "status",
        transform: FieldTransform::ResetText("unchecked"),
    },
    FieldRule {
        name: "latency_ms",
        transform: FieldTransform::ResetNull,
    },
    FieldRule {
        name: "last_checked_at",
        transform: FieldTransform::ResetNull,
    },
];
const STATION_KEYS_RULES: &[FieldRule] = &[
    FieldRule {
        name: "api_key",
        transform: FieldTransform::RequireEmpty,
    },
    FieldRule {
        name: "api_key_secret_id",
        transform: FieldTransform::SecretReference,
    },
    FieldRule {
        name: "status",
        transform: FieldTransform::ResetText("unchecked"),
    },
    FieldRule {
        name: "last_checked_at",
        transform: FieldTransform::ResetNull,
    },
];
const STATION_CREDENTIALS_RULES: &[FieldRule] = &[
    FieldRule {
        name: "login_password",
        transform: FieldTransform::RequireEmpty,
    },
    FieldRule {
        name: "login_password_secret_id",
        transform: FieldTransform::SecretReference,
    },
    FieldRule {
        name: "remember_password",
        transform: FieldTransform::Copy,
    },
    FieldRule {
        name: "access_token_secret_id",
        transform: FieldTransform::ResetNull,
    },
    FieldRule {
        name: "refresh_token_secret_id",
        transform: FieldTransform::ResetNull,
    },
    FieldRule {
        name: "cookie_secret_id",
        transform: FieldTransform::ResetNull,
    },
    FieldRule {
        name: "login_status",
        transform: FieldTransform::ResetText("unknown"),
    },
    FieldRule {
        name: "login_error",
        transform: FieldTransform::ResetNull,
    },
    FieldRule {
        name: "session_status",
        transform: FieldTransform::ResetText("none"),
    },
    FieldRule {
        name: "session_expires_at",
        transform: FieldTransform::ResetNull,
    },
    FieldRule {
        name: "token_expires_at",
        transform: FieldTransform::ResetNull,
    },
    FieldRule {
        name: "token_refreshed_at",
        transform: FieldTransform::ResetNull,
    },
    FieldRule {
        name: "session_source",
        transform: FieldTransform::ResetText("none"),
    },
    FieldRule {
        name: "session_user_agent",
        transform: FieldTransform::ResetNull,
    },
];
const REMOTE_KEY_RULES: &[FieldRule] = &[
    FieldRule {
        name: "api_key_masked",
        transform: FieldTransform::Copy,
    },
    FieldRule {
        name: "api_key_fingerprint",
        transform: FieldTransform::Copy,
    },
];
const JSON_RULES: &[FieldRule] = &[
    FieldRule {
        name: "model_allowlist_json",
        transform: FieldTransform::BoundedJson,
    },
    FieldRule {
        name: "model_blocklist_json",
        transform: FieldTransform::BoundedJson,
    },
    FieldRule {
        name: "preferred_models_json",
        transform: FieldTransform::BoundedJson,
    },
    FieldRule {
        name: "routing_tags_json",
        transform: FieldTransform::BoundedJson,
    },
];
const REQUEST_LOG_RULES: &[FieldRule] = &[
    FieldRule {
        name: "error_message",
        transform: FieldTransform::RedactText,
    },
    FieldRule {
        name: "rejected_candidates_json",
        transform: FieldTransform::RedactJson,
    },
    FieldRule {
        name: "attempts_json",
        transform: FieldTransform::RedactJson,
    },
    FieldRule {
        name: "economic_context_json",
        transform: FieldTransform::RedactJson,
    },
];
const MODEL_BASE_PRICE_RULES: &[FieldRule] = &[FieldRule {
    name: "long_context_input_token_threshold",
    transform: FieldTransform::Copy,
}];
const REQUEST_ATTEMPT_RULES: &[FieldRule] = &[FieldRule {
    name: "sanitized_detail",
    transform: FieldTransform::RedactText,
}];
const ROUTE_DECISION_RULES: &[FieldRule] = &[
    FieldRule {
        name: "rejection_counts_json",
        transform: FieldTransform::BoundedJson,
    },
    FieldRule {
        name: "fact_version_vector",
        transform: FieldTransform::BoundedJson,
    },
];
const ROUTE_CANDIDATE_DECISION_RULES: &[FieldRule] = &[
    FieldRule {
        name: "fact_version_vector",
        transform: FieldTransform::BoundedJson,
    },
    FieldRule {
        name: "evidence_json",
        transform: FieldTransform::BoundedJson,
    },
];
const ROUTING_REQUEST_COST_AGGREGATE_RULES: &[FieldRule] = &[
    FieldRule {
        name: "totals_by_currency_json",
        transform: FieldTransform::BoundedJson,
    },
    FieldRule {
        name: "incomplete_attempts_json",
        transform: FieldTransform::BoundedJson,
    },
];
const COLLECTOR_RUN_RULES: &[FieldRule] = &[
    FieldRule {
        name: "status",
        transform: FieldTransform::Copy,
    },
    FieldRule {
        name: "error_message",
        transform: FieldTransform::RedactText,
    },
];
const COLLECTOR_SNAPSHOT_RULES: &[FieldRule] = &[
    FieldRule {
        name: "summary_json",
        transform: FieldTransform::RedactJson,
    },
    FieldRule {
        name: "normalized_json",
        transform: FieldTransform::RedactJson,
    },
    FieldRule {
        name: "raw_json_redacted",
        transform: FieldTransform::RedactJson,
    },
    FieldRule {
        name: "error_message",
        transform: FieldTransform::RedactText,
    },
];
const STATION_GROUP_BINDING_RULES: &[FieldRule] = &[
    FieldRule {
        name: "raw_json_redacted",
        transform: FieldTransform::RedactJson,
    },
    FieldRule {
        name: "last_seen_run_id",
        transform: FieldTransform::ResetNull,
    },
];
const GROUP_RATE_RECORD_RULES: &[FieldRule] = &[FieldRule {
    name: "raw_json_redacted",
    transform: FieldTransform::RedactJson,
}];
const ALERT_POLICY_RULES: &[FieldRule] = &[];
const CHANGE_INCIDENT_RULES: &[FieldRule] = &[FieldRule {
    name: "last_observation_summary_json",
    transform: FieldTransform::RedactJson,
}];
const CHANGE_EVENT_OCCURRENCE_RULES: &[FieldRule] = &[
    FieldRule {
        name: "old_value_json",
        transform: FieldTransform::RedactJson,
    },
    FieldRule {
        name: "new_value_json",
        transform: FieldTransform::RedactJson,
    },
    FieldRule {
        name: "impact_json",
        transform: FieldTransform::RedactJson,
    },
];
const INCIDENT_ATTENTION_RULES: &[FieldRule] = &[];
const NOTIFICATION_DELIVERY_RULES: &[FieldRule] = &[
    FieldRule {
        name: "policy_snapshot_json",
        transform: FieldTransform::RedactJson,
    },
    FieldRule {
        name: "claim_token",
        transform: FieldTransform::Exclude,
    },
];
const ALERTING_UPGRADE_PROGRESS_RULES: &[FieldRule] = &[];
const CHANNEL_TEMPLATE_RULES: &[FieldRule] = &[FieldRule {
    name: "request_body_json",
    transform: FieldTransform::RedactJson,
}];
const CHANNEL_MONITOR_RULES: &[FieldRule] = &[
    FieldRule {
        name: "fallback_models_json",
        transform: FieldTransform::BoundedJson,
    },
    FieldRule {
        name: "fallback_models_v2_json",
        transform: FieldTransform::BoundedJson,
    },
    FieldRule {
        name: "last_run_at",
        transform: FieldTransform::ResetNull,
    },
    FieldRule {
        name: "last_run_id",
        transform: FieldTransform::ResetNull,
    },
    FieldRule {
        name: "next_run_at",
        transform: FieldTransform::ResetNull,
    },
    FieldRule {
        name: "next_due_at_ms",
        transform: FieldTransform::ResetNull,
    },
    FieldRule {
        name: "last_status",
        transform: FieldTransform::ResetNull,
    },
    FieldRule {
        name: "last_error_message",
        transform: FieldTransform::ResetNull,
    },
];
const CHANNEL_EXECUTION_RULES: &[FieldRule] = &[FieldRule {
    name: "summary_failure_kind",
    transform: FieldTransform::RedactText,
}];
const CHANNEL_ATTEMPT_RULES: &[FieldRule] = &[
    FieldRule {
        name: "input_tokens",
        transform: FieldTransform::Copy,
    },
    FieldRule {
        name: "output_tokens",
        transform: FieldTransform::Copy,
    },
    FieldRule {
        name: "total_tokens",
        transform: FieldTransform::Copy,
    },
    FieldRule {
        name: "error_summary",
        transform: FieldTransform::RedactText,
    },
];
const CHANNEL_TARGET_RESULT_RULES: &[FieldRule] = &[FieldRule {
    name: "terminal_reason",
    transform: FieldTransform::RedactText,
}];
const CHANNEL_BUCKET_ROLLUP_RULES: &[FieldRule] = &[
    FieldRule {
        name: "failure_counts_json",
        transform: FieldTransform::BoundedJson,
    },
    FieldRule {
        name: "exclusion_counts_json",
        transform: FieldTransform::BoundedJson,
    },
];
const STATION_KEY_HEALTH_OBSERVATION_RULES: &[FieldRule] = &[FieldRule {
    name: "error_summary",
    transform: FieldTransform::RedactText,
}];
const PROVIDER_DRAFT_RULES: &[FieldRule] = &[FieldRule {
    name: "payload_json",
    transform: FieldTransform::Exclude,
}];
const PROVIDER_DRAFT_PREVIEW_RULES: &[FieldRule] = &[FieldRule {
    name: "result_json",
    transform: FieldTransform::Exclude,
}];
const APP_SECRET_BINDING_RULES: &[FieldRule] = &[FieldRule {
    name: "secret_id",
    transform: FieldTransform::SecretReference,
}];
const ROUTING_POLICY_RULES: &[FieldRule] = &[FieldRule {
    name: "config_json",
    transform: FieldTransform::BoundedJson,
}];
const ROUTING_OBSERVATION_RULES: &[FieldRule] = &[FieldRule {
    name: "evidence_json",
    transform: FieldTransform::BoundedJson,
}];
const ROUTING_QUALITY_RULES: &[FieldRule] = &[FieldRule {
    name: "summary_json",
    transform: FieldTransform::BoundedJson,
}];
const MODEL_MAPPING_RULES_RULES: &[FieldRule] = &[FieldRule {
    name: "endpoint_conditions_json",
    transform: FieldTransform::BoundedJson,
}];
const MODEL_MAPPING_DOCUMENT_HISTORY_RULES: &[FieldRule] = &[FieldRule {
    name: "document_json",
    transform: FieldTransform::BoundedJson,
}];
const ROUTING_DOCUMENT_SYNC_RULES: &[FieldRule] = &[FieldRule {
    name: "attempt_token",
    transform: FieldTransform::Exclude,
}];

// Existing channel monitor tables are declared here only to classify the current
// database schema. Portable migration carries configuration only, never runtime fields.
const TABLES: &[TableCatalog] = &[
    table(
        "persistence_schema_compatibility",
        TablePolicy::InternalRebuild,
        DataCategory::DeviceRuntimeState,
        DependencyStage::Internal,
        false,
        SCHEMA_COMPATIBILITY_COLUMNS,
        &[],
    ),
    table(
        "persistence_runtime_health",
        TablePolicy::InternalRebuild,
        DataCategory::DeviceRuntimeState,
        DependencyStage::Internal,
        false,
        RUNTIME_HEALTH_COLUMNS,
        &[],
    ),
    table(
        "settings",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Internal,
        true,
        SETTINGS_COLUMNS,
        SETTINGS_RULES,
    ),
    table(
        "secrets",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Secrets,
        true,
        SECRETS_COLUMNS,
        SECRETS_RULES,
    ),
    table(
        "stations",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Stations,
        true,
        STATIONS_COLUMNS,
        STATIONS_RULES,
    ),
    table(
        "station_capacity_domains",
        TablePolicy::Include,
        DataCategory::CoreData,
        DependencyStage::StationChildren,
        true,
        STATION_CAPACITY_DOMAINS_COLUMNS,
        &[],
    ),
    table(
        "station_keys",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::StationChildren,
        true,
        STATION_KEYS_COLUMNS,
        STATION_KEYS_RULES,
    ),
    table(
        "station_credentials",
        TablePolicy::IncludeWithTransform,
        DataCategory::SessionCredentials,
        DependencyStage::StationChildren,
        true,
        STATION_CREDENTIALS_COLUMNS,
        STATION_CREDENTIALS_RULES,
    ),
    table(
        "remote_station_keys",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::StationChildren,
        true,
        REMOTE_STATION_KEYS_COLUMNS,
        REMOTE_KEY_RULES,
    ),
    table(
        "station_key_capabilities",
        TablePolicy::Include,
        DataCategory::CoreData,
        DependencyStage::Routing,
        true,
        STATION_KEY_CAPABILITIES_COLUMNS,
        JSON_RULES,
    ),
    table(
        "model_mapping_policies",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Internal,
        false,
        MODEL_MAPPING_POLICIES_COLUMNS,
        &[],
    ),
    table(
        "model_mapping_rules",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Routing,
        true,
        MODEL_MAPPING_RULES_COLUMNS,
        MODEL_MAPPING_RULES_RULES,
    ),
    table(
        "model_profiles",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Routing,
        true,
        MODEL_PROFILES_COLUMNS,
        &[],
    ),
    table(
        "model_mapping_rule_targets",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Routing,
        true,
        MODEL_MAPPING_RULE_TARGETS_COLUMNS,
        &[],
    ),
    table(
        "model_offering_bindings",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Routing,
        true,
        MODEL_OFFERING_BINDINGS_COLUMNS,
        &[],
    ),
    table(
        "legacy_model_alias_migration_reviews",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Routing,
        false,
        LEGACY_MODEL_ALIAS_MIGRATION_REVIEWS_COLUMNS,
        &[],
    ),
    table(
        "model_mapping_document_history",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        false,
        MODEL_MAPPING_DOCUMENT_HISTORY_COLUMNS,
        MODEL_MAPPING_DOCUMENT_HISTORY_RULES,
    ),
    table(
        "model_aliases",
        TablePolicy::Include,
        DataCategory::CoreData,
        DependencyStage::Routing,
        true,
        MODEL_ALIASES_COLUMNS,
        &[],
    ),
    table(
        "alert_policies",
        TablePolicy::Include,
        DataCategory::CoreData,
        DependencyStage::Routing,
        true,
        ALERT_POLICIES_COLUMNS,
        ALERT_POLICY_RULES,
    ),
    table(
        "balance_snapshots",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        BALANCE_SNAPSHOTS_COLUMNS,
        &[],
    ),
    table(
        "request_logs",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        REQUEST_LOGS_COLUMNS,
        REQUEST_LOG_RULES,
    ),
    table(
        "request_attempts",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        REQUEST_ATTEMPTS_COLUMNS,
        REQUEST_ATTEMPT_RULES,
    ),
    table(
        "request_log_url_sanitizer_progress",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::History,
        false,
        REQUEST_LOG_URL_SANITIZER_PROGRESS_COLUMNS,
        &[],
    ),
    table(
        "route_decisions",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        ROUTE_DECISIONS_COLUMNS,
        ROUTE_DECISION_RULES,
    ),
    table(
        "route_candidate_decisions",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        ROUTE_CANDIDATE_DECISIONS_COLUMNS,
        ROUTE_CANDIDATE_DECISION_RULES,
    ),
    table(
        "routing_attempt_costs",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        ROUTING_ATTEMPT_COSTS_COLUMNS,
        &[],
    ),
    table(
        "routing_request_cost_aggregates",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        ROUTING_REQUEST_COST_AGGREGATES_COLUMNS,
        ROUTING_REQUEST_COST_AGGREGATE_RULES,
    ),
    table(
        "request_routing_outcome_summaries",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        REQUEST_ROUTING_OUTCOME_SUMMARIES_COLUMNS,
        &[],
    ),
    table(
        "request_decision_events",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        REQUEST_DECISION_EVENTS_COLUMNS,
        &[],
    ),
    table(
        "routing_error_rate_history",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        ROUTING_ERROR_RATE_HISTORY_COLUMNS,
        &[],
    ),
    table(
        "routing_error_rate_history_meta",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::History,
        false,
        ROUTING_ERROR_RATE_HISTORY_META_COLUMNS,
        &[],
    ),
    table(
        "request_terminal_outbox",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::History,
        false,
        REQUEST_TERMINAL_OUTBOX_COLUMNS,
        REQUEST_TERMINAL_OUTBOX_RULES,
    ),
    table(
        "routing_lifecycle_reconciliation_progress",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::History,
        false,
        ROUTING_LIFECYCLE_RECONCILIATION_PROGRESS_COLUMNS,
        &[],
    ),
    table(
        "collector_runs",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        COLLECTOR_RUNS_COLUMNS,
        COLLECTOR_RUN_RULES,
    ),
    table(
        "collector_snapshots",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        COLLECTOR_SNAPSHOTS_COLUMNS,
        COLLECTOR_SNAPSHOT_RULES,
    ),
    table(
        "station_group_bindings",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Routing,
        true,
        STATION_GROUP_BINDINGS_COLUMNS,
        STATION_GROUP_BINDING_RULES,
    ),
    table(
        "group_rate_records",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        GROUP_RATE_RECORDS_COLUMNS,
        GROUP_RATE_RECORD_RULES,
    ),
    table(
        "collector_model_facts",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::StationChildren,
        false,
        COLLECTOR_MODEL_FACTS_COLUMNS,
        &[],
    ),
    table(
        "collector_task_state",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::StationChildren,
        false,
        COLLECTOR_TASK_STATE_COLUMNS,
        &[],
    ),
    table(
        "station_published_status_sources",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::StationChildren,
        false,
        STATION_PUBLISHED_STATUS_SOURCES_COLUMNS,
        STATION_PUBLISHED_STATUS_SOURCE_RULES,
    ),
    table(
        "station_published_monitors",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        STATION_PUBLISHED_MONITORS_COLUMNS,
        STATION_PUBLISHED_MONITOR_RULES,
    ),
    table(
        "station_published_monitor_samples",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        STATION_PUBLISHED_MONITOR_SAMPLES_COLUMNS,
        STATION_PUBLISHED_MONITOR_SAMPLE_RULES,
    ),
    table(
        "change_incidents",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        CHANGE_INCIDENTS_COLUMNS,
        CHANGE_INCIDENT_RULES,
    ),
    table(
        "change_event_occurrences",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        CHANGE_EVENT_OCCURRENCES_COLUMNS,
        CHANGE_EVENT_OCCURRENCE_RULES,
    ),
    table(
        "incident_attention",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        INCIDENT_ATTENTION_COLUMNS,
        INCIDENT_ATTENTION_RULES,
    ),
    table(
        "notification_deliveries",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        NOTIFICATION_DELIVERIES_COLUMNS,
        NOTIFICATION_DELIVERY_RULES,
    ),
    table(
        "alerting_upgrade_progress",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::Internal,
        false,
        ALERTING_UPGRADE_PROGRESS_COLUMNS,
        ALERTING_UPGRADE_PROGRESS_RULES,
    ),
    table(
        "model_base_prices",
        TablePolicy::Include,
        DataCategory::CoreData,
        DependencyStage::Pricing,
        true,
        MODEL_BASE_PRICES_COLUMNS,
        MODEL_BASE_PRICE_RULES,
    ),
    table(
        "channel_monitor_request_templates",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Routing,
        true,
        CHANNEL_MONITOR_TEMPLATE_COLUMNS,
        CHANNEL_TEMPLATE_RULES,
    ),
    table(
        "channel_monitors",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Routing,
        true,
        CHANNEL_MONITORS_COLUMNS,
        CHANNEL_MONITOR_RULES,
    ),
    table(
        "channel_monitor_executions",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        CHANNEL_MONITOR_EXECUTIONS_COLUMNS,
        CHANNEL_EXECUTION_RULES,
    ),
    table(
        "channel_monitor_attempts",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        CHANNEL_MONITOR_ATTEMPTS_COLUMNS,
        CHANNEL_ATTEMPT_RULES,
    ),
    table(
        "channel_monitor_target_results",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        CHANNEL_MONITOR_TARGET_RESULTS_COLUMNS,
        CHANNEL_TARGET_RESULT_RULES,
    ),
    table(
        "channel_monitor_bucket_rollups",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        CHANNEL_MONITOR_BUCKET_ROLLUPS_COLUMNS,
        CHANNEL_BUCKET_ROLLUP_RULES,
    ),
    table(
        "station_key_health_observations",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        STATION_KEY_HEALTH_OBSERVATIONS_COLUMNS,
        STATION_KEY_HEALTH_OBSERVATION_RULES,
    ),
    // Retained solely so databases created before the routing cutover remain
    // readable. These tables are never imported and are not routing inputs.
    table(
        "endpoint_health_snapshot",
        TablePolicy::Exclude,
        DataCategory::DeviceRuntimeState,
        DependencyStage::Excluded,
        false,
        STATION_ENDPOINT_HEALTH_COLUMNS,
        STATION_ENDPOINT_HEALTH_RULES,
    ),
    table(
        "routing_health_snapshot",
        TablePolicy::Exclude,
        DataCategory::DeviceRuntimeState,
        DependencyStage::Excluded,
        false,
        STATION_KEY_HEALTH_COLUMNS,
        STATION_KEY_HEALTH_RULES,
    ),
    table(
        "channel_monitor_rollup_dirty_ranges",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::StationChildren,
        false,
        CHANNEL_MONITOR_ROLLUP_DIRTY_RANGES_COLUMNS,
        &[],
    ),
    table(
        "channel_monitor_probe_budget_usage",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::StationChildren,
        false,
        CHANNEL_MONITOR_PROBE_BUDGET_USAGE_COLUMNS,
        &[],
    ),
    table(
        "provider_drafts",
        TablePolicy::Exclude,
        DataCategory::ProviderDrafts,
        DependencyStage::Excluded,
        true,
        PROVIDER_DRAFTS_COLUMNS,
        PROVIDER_DRAFT_RULES,
    ),
    table(
        "provider_draft_previews",
        TablePolicy::Exclude,
        DataCategory::ProviderDrafts,
        DependencyStage::Excluded,
        true,
        PROVIDER_DRAFT_PREVIEWS_COLUMNS,
        PROVIDER_DRAFT_PREVIEW_RULES,
    ),
    table(
        "app_secret_bindings",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Secrets,
        true,
        APP_SECRET_BINDINGS_COLUMNS,
        APP_SECRET_BINDING_RULES,
    ),
    table(
        "domain_revisions",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::Internal,
        false,
        DOMAIN_REVISIONS_COLUMNS,
        &[],
    ),
    table(
        "routing_policy",
        TablePolicy::IncludeWithTransform,
        DataCategory::CoreData,
        DependencyStage::Routing,
        false,
        ROUTING_POLICY_COLUMNS,
        ROUTING_POLICY_RULES,
    ),
    table(
        "routing_policy_history",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        ROUTING_POLICY_HISTORY_COLUMNS,
        ROUTING_POLICY_RULES,
    ),
    table(
        "routing_observations",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        ROUTING_OBSERVATIONS_COLUMNS,
        ROUTING_OBSERVATION_RULES,
    ),
    table(
        "routing_projector_checkpoints",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::History,
        false,
        ROUTING_PROJECTOR_CHECKPOINTS_COLUMNS,
        &[],
    ),
    table(
        "routing_quality_summaries",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::History,
        false,
        ROUTING_QUALITY_SUMMARIES_COLUMNS,
        ROUTING_QUALITY_RULES,
    ),
    table(
        "routing_health_axes",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::History,
        false,
        ROUTING_HEALTH_AXES_COLUMNS,
        &[],
    ),
    table(
        "routing_health_generations",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::History,
        false,
        ROUTING_HEALTH_GENERATIONS_COLUMNS,
        &[],
    ),
    table(
        "routing_health_observations",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        ROUTING_HEALTH_OBSERVATIONS_COLUMNS,
        &[],
    ),
    table(
        "routing_health_verdicts",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::History,
        false,
        ROUTING_HEALTH_VERDICTS_COLUMNS,
        &[],
    ),
    table(
        "routing_health_projector_state",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::History,
        false,
        ROUTING_HEALTH_PROJECTOR_STATE_COLUMNS,
        &[],
    ),
    table(
        "routing_health_protection_state",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::History,
        false,
        ROUTING_HEALTH_PROTECTION_STATE_COLUMNS,
        ROUTING_HEALTH_PROTECTION_STATE_RULES,
    ),
    table(
        "routing_capability_model_observations",
        TablePolicy::OptionalHistory,
        DataCategory::History,
        DependencyStage::History,
        true,
        ROUTING_CAPABILITY_MODEL_OBSERVATIONS_COLUMNS,
        &[],
    ),
    table(
        "routing_capability_model_verdicts",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::History,
        false,
        ROUTING_CAPABILITY_MODEL_VERDICTS_COLUMNS,
        &[],
    ),
    table(
        "routing_document_sync",
        TablePolicy::Reset,
        DataCategory::DeviceRuntimeState,
        DependencyStage::Internal,
        false,
        ROUTING_DOCUMENT_SYNC_COLUMNS,
        ROUTING_DOCUMENT_SYNC_RULES,
    ),
];

const fn table(
    name: &'static str,
    policy: TablePolicy,
    category: DataCategory,
    dependency_stage: DependencyStage,
    counts_for_occupancy: bool,
    columns: &'static [&'static str],
    field_rules: &'static [FieldRule],
) -> TableCatalog {
    TableCatalog {
        name,
        policy,
        category,
        dependency_stage,
        counts_for_occupancy,
        columns,
        field_rules,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_exhaustive_unique_and_has_sensitive_field_rules() {
        let actual = TABLES
            .iter()
            .map(|table| (table.name, table.columns))
            .collect::<Vec<_>>();

        assert_eq!(TABLES.len(), EXPECTED_USER_TABLE_COUNT_V1);
        validate_schema_snapshot(&actual).expect("self catalog is valid");
    }

    #[test]
    fn catalog_matches_required_policy_matrix_for_configuration_and_runtime_boundaries() {
        assert_eq!(
            table_catalog("provider_drafts").unwrap().policy,
            TablePolicy::Exclude
        );
        assert_eq!(
            table_catalog("collector_model_facts").unwrap().policy,
            TablePolicy::Reset
        );
        assert_eq!(
            table_catalog("group_rate_records").unwrap().policy,
            TablePolicy::OptionalHistory
        );
        assert_eq!(
            table_catalog("station_published_status_sources")
                .unwrap()
                .policy,
            TablePolicy::Reset
        );
        assert_eq!(
            table_catalog("station_published_monitors").unwrap().policy,
            TablePolicy::OptionalHistory
        );
        assert_eq!(
            table_catalog("station_published_monitor_samples")
                .unwrap()
                .policy,
            TablePolicy::OptionalHistory
        );
        assert_eq!(
            field_rule("channel_monitors", "last_status"),
            Some(FieldTransform::ResetNull)
        );
        assert_eq!(
            field_rule("station_keys", "status"),
            Some(FieldTransform::ResetText("unchecked"))
        );
    }

    #[test]
    fn schema_drift_and_sensitive_field_drift_fail_closed() {
        assert_eq!(
            validate_table_columns("stations", &["id", "api_key", "future_column"]).unwrap_err(),
            CatalogError::MissingColumnPolicy {
                table: "stations".to_string(),
                column: "future_column".to_string()
            }
        );
        assert_eq!(
            validate_schema_snapshot(&[("future_table", &["id"][..])]).unwrap_err(),
            CatalogError::MissingTablePolicy("future_table".to_string())
        );
    }

    #[test]
    fn setting_and_secret_allowlists_fail_closed() {
        assert_eq!(setting_policy("local_key"), Some(SettingPolicy::Reset));
        assert_eq!(
            setting_policy("published_status_interval_minutes"),
            Some(SettingPolicy::Include)
        );
        assert_eq!(setting_policy("future_setting"), None);
        assert_eq!(
            setting_policy("common_login_profiles_json"),
            Some(SettingPolicy::IncludeWithTransform)
        );
        assert_eq!(
            setting_policy("common_login_catalog_json"),
            Some(SettingPolicy::IncludeWithTransform)
        );
        assert_eq!(
            secret_policy("station_key", "api_key"),
            Some(SecretPolicy::IncludeAndRekey)
        );
        assert_eq!(
            secret_policy("application", "local_proxy_access_key"),
            Some(SecretPolicy::ExcludeAndRegenerate)
        );
        assert_eq!(
            secret_policy("common_login_profile", "password"),
            Some(SecretPolicy::IncludeAndRekey)
        );
        assert_eq!(
            secret_policy("common_login_password", "password"),
            Some(SecretPolicy::IncludeAndRekey)
        );
        assert_eq!(secret_policy("common_login_password", "token"), None);
        assert_eq!(secret_policy("station_key", "future_secret"), None);
    }
}
