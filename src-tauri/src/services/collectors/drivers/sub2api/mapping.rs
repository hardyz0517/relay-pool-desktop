use std::collections::HashMap;

use serde_json::{json, Value};

use crate::{
    models::{
        remote_keys::{CreateRemoteStationKeyInput, RemoteKeyMatchStatus, RemoteStationKey},
        station_keys::StationKey,
    },
    services::{
        collectors::facts::{
            CollectedBalanceFact, CollectedGroupFact, CollectedRateFact, CollectorFacts,
            NORMALIZED_BALANCE_CURRENCY,
        },
        group_categories::infer_group_category,
    },
};

const TODAY_BASE_CONSUMPTION_FIELDS: &[&str] = &[
    "today_base_consumption",
    "today_base_used_amount",
    "today_base_cost",
    "todayBaseConsumption",
    "todayBaseUsedAmount",
    "todayBaseCost",
    "today_quota_consumption",
];
const TOTAL_BASE_CONSUMPTION_FIELDS: &[&str] = &[
    "total_base_consumption",
    "base_consumption",
    "base_used_amount",
    "total_base_used_amount",
    "base_cost",
    "total_base_cost",
    "totalBaseConsumption",
    "baseConsumption",
    "baseUsedAmount",
    "totalBaseUsedAmount",
    "baseCost",
    "totalBaseCost",
    "quota_consumption",
];

pub fn parse_usage_balance(
    station_id: &str,
    station_key_id: Option<String>,
    payload: &Value,
    credit_per_cny: f64,
) -> CollectedBalanceFact {
    let remaining = payload
        .pointer("/quota/remaining")
        .and_then(Value::as_f64)
        .or_else(|| payload.get("remaining").and_then(Value::as_f64))
        .or_else(|| payload.get("balance").and_then(Value::as_f64));
    let used = payload
        .pointer("/quota/used")
        .and_then(Value::as_f64)
        .or_else(|| payload.get("used").and_then(Value::as_f64));
    let total = payload
        .pointer("/quota/total")
        .and_then(Value::as_f64)
        .or_else(|| payload.get("total").and_then(Value::as_f64));
    let status = if remaining == Some(0.0) {
        "depleted"
    } else {
        "normal"
    };

    CollectedBalanceFact {
        station_id: station_id.to_string(),
        station_key_id,
        scope: "station_key".to_string(),
        value: normalize_credit_value(remaining, credit_per_cny),
        used_value: normalize_credit_value(used, credit_per_cny),
        total_value: normalize_credit_value(total, credit_per_cny),
        today_request_count: parse_i64_field(
            payload,
            &[
                "today_request_count",
                "today_requests",
                "todayRequestCount",
                "todayRequests",
            ],
        ),
        total_request_count: parse_i64_field(
            payload,
            &[
                "total_request_count",
                "request_count",
                "totalRequests",
                "requestCount",
                "requests",
            ],
        ),
        today_consumption: parse_f64_field(
            payload,
            &[
                "today_consumption",
                "today_used_amount",
                "todayConsume",
                "todayConsumption",
                "todayUsedAmount",
                "today_cost",
            ],
        ),
        total_consumption: parse_f64_field(
            payload,
            &[
                "total_consumption",
                "used_amount",
                "totalUsedAmount",
                "totalConsumption",
                "consumption",
                "cost",
            ],
        ),
        today_base_consumption: parse_f64_field(payload, TODAY_BASE_CONSUMPTION_FIELDS),
        total_base_consumption: parse_f64_field(payload, TOTAL_BASE_CONSUMPTION_FIELDS),
        today_token_count: parse_i64_field(
            payload,
            &[
                "today_token_count",
                "today_tokens",
                "todayTokenCount",
                "todayTokens",
            ],
        ),
        total_token_count: parse_i64_field(
            payload,
            &[
                "total_token_count",
                "total_tokens",
                "token_count",
                "totalTokenCount",
                "totalTokens",
                "tokens",
            ],
        ),
        today_input_token_count: parse_i64_field(
            payload,
            &[
                "today_input_token_count",
                "today_input_tokens",
                "today_prompt_tokens",
                "todayInputTokenCount",
                "todayInputTokens",
                "todayPromptTokens",
            ],
        ),
        today_output_token_count: parse_i64_field(
            payload,
            &[
                "today_output_token_count",
                "today_output_tokens",
                "today_completion_tokens",
                "todayOutputTokenCount",
                "todayOutputTokens",
                "todayCompletionTokens",
            ],
        ),
        total_input_token_count: parse_i64_field(
            payload,
            &[
                "total_input_token_count",
                "total_input_tokens",
                "input_tokens",
                "prompt_tokens",
                "totalInputTokenCount",
                "totalInputTokens",
                "inputTokens",
                "promptTokens",
            ],
        ),
        total_output_token_count: parse_i64_field(
            payload,
            &[
                "total_output_token_count",
                "total_output_tokens",
                "output_tokens",
                "completion_tokens",
                "totalOutputTokenCount",
                "totalOutputTokens",
                "outputTokens",
                "completionTokens",
            ],
        ),
        account_concurrency_limit: None,
        currency: NORMALIZED_BALANCE_CURRENCY.to_string(),
        credit_unit: payload
            .pointer("/quota/unit")
            .and_then(Value::as_str)
            .or_else(|| payload.get("unit").and_then(Value::as_str))
            .map(ToString::to_string),
        status: status.to_string(),
        source: "sub2api_usage".to_string(),
        confidence: if remaining.is_some() { 0.9 } else { 0.4 },
        collected_at: None,
    }
}

fn parse_f64_field(payload: &Value, names: &[&str]) -> Option<f64> {
    names.iter().find_map(|name| {
        parse_optional_f64(payload.get(*name))
            .or_else(|| parse_optional_f64(payload.pointer(&format!("/data/{name}"))))
    })
}

fn parse_i64_field(payload: &Value, names: &[&str]) -> Option<i64> {
    names.iter().find_map(|name| {
        parse_optional_i64(payload.get(*name))
            .or_else(|| parse_optional_i64(payload.pointer(&format!("/data/{name}"))))
    })
}

fn parse_account_concurrency_limit(payload: &Value) -> Option<i64> {
    parse_account_i64_field(
        payload,
        &[
            "concurrency_limit",
            "concurrent_limit",
            "concurrency",
            "request_concurrency",
            "parallel_limit",
            "max_concurrency",
            "concurrencyLimit",
            "concurrentLimit",
            "requestConcurrency",
            "parallelLimit",
            "maxConcurrency",
        ],
    )
    .filter(|value| *value > 0)
}

fn parse_optional_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
    })
}

fn parse_optional_i64(value: Option<&Value>) -> Option<i64> {
    value.and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| value.as_f64().map(|value| value.round() as i64))
            .or_else(|| value.as_str()?.trim().parse::<i64>().ok())
    })
}

#[derive(Debug, Clone)]
struct AvailableGroup {
    group_id: Option<String>,
    group_name: String,
    default_rate_multiplier: Option<f64>,
    raw_json_redacted: Option<Value>,
}

pub fn parse_group_rate_facts(
    station_id: &str,
    available: &Value,
    rates: &Value,
    _credit_per_cny: f64,
) -> CollectorFacts {
    let mut facts = CollectorFacts::default();
    let groups = collect_available_groups(available);
    let rate_map = collect_user_rate_map(rates);

    for group in groups {
        let group_id = group.group_id.clone();
        let group_key_hash = stable_group_key_hash(
            station_id,
            "sub2api",
            group_id.as_deref(),
            &group.group_name,
        );
        let user_rate = group_id.as_deref().and_then(|id| rate_map.get(id).copied());
        let effective = user_rate.or(group.default_rate_multiplier);
        let inferred_group_category =
            infer_group_category(&group.group_name, group.raw_json_redacted.as_ref());

        facts.groups.push(CollectedGroupFact {
            station_id: station_id.to_string(),
            group_id: group_id.clone(),
            group_key_hash: group_key_hash.clone(),
            group_name: group.group_name.clone(),
            visibility: "available".to_string(),
            inferred_group_category: Some(inferred_group_category.clone()),
            source: "sub2api_groups_available".to_string(),
            confidence: 0.9,
            raw_json_redacted: group.raw_json_redacted.clone(),
        });
        facts.rates.push(CollectedRateFact {
            station_id: station_id.to_string(),
            station_key_id: None,
            group_id,
            group_key_hash,
            group_name: group.group_name,
            default_rate_multiplier: group.default_rate_multiplier,
            user_rate_multiplier: user_rate,
            effective_rate_multiplier: effective,
            inferred_group_category: Some(inferred_group_category),
            source: "sub2api_groups_rates".to_string(),
            confidence: if effective.is_some() { 0.9 } else { 0.6 },
            checked_at: None,
            raw_json_redacted: group.raw_json_redacted,
        });
    }

    facts
}

fn normalize_credit_value(value: Option<f64>, credit_per_cny: f64) -> Option<f64> {
    let divisor = if credit_per_cny.is_finite() && credit_per_cny > 0.0 {
        credit_per_cny
    } else {
        1.0
    };
    value.map(|value| value / divisor)
}

fn collect_available_groups(payload: &Value) -> Vec<AvailableGroup> {
    group_items(payload)
        .into_iter()
        .filter_map(|value| {
            if let Some(group_name) = scalar_text(value) {
                return Some(AvailableGroup {
                    group_id: Some(group_name.clone()),
                    group_name,
                    default_rate_multiplier: None,
                    raw_json_redacted: Some(crate::services::secrets::mask::redact_value(value)),
                });
            }
            let group_id = string_field(value, &["id", "group_id", "groupId", "key"]);
            let group_name = string_field(
                value,
                &["name", "group_name", "groupName", "group", "label"],
            )
            .or_else(|| group_id.clone())?;
            Some(AvailableGroup {
                group_id,
                group_name,
                default_rate_multiplier: numeric_field(
                    value,
                    &[
                        "rate_multiplier",
                        "rateMultiplier",
                        "ratio",
                        "multiplier",
                        "rate",
                    ],
                ),
                raw_json_redacted: Some(crate::services::secrets::mask::redact_value(value)),
            })
        })
        .collect()
}

fn collect_user_rate_map(payload: &Value) -> HashMap<String, f64> {
    let mut rates = HashMap::new();
    collect_rates_from_value(payload, &mut rates);
    rates
}

fn collect_rates_from_value(value: &Value, rates: &mut HashMap<String, f64>) {
    match value {
        Value::Object(map) => {
            if let (Some(group_id), Some(rate)) = (
                string_field(value, &["id", "group_id", "groupId", "key", "name"]),
                numeric_field(
                    value,
                    &[
                        "rate_multiplier",
                        "rateMultiplier",
                        "ratio",
                        "multiplier",
                        "rate",
                    ],
                ),
            ) {
                rates.insert(group_id, rate);
            }

            if map.values().all(|item| item.as_f64().is_some()) {
                for (key, item) in map {
                    if let Some(rate) = item.as_f64() {
                        rates.insert(key.to_string(), rate);
                    }
                }
                return;
            }

            for key in ["data", "rates", "group_ratio", "groups", "items", "list"] {
                if let Some(child) = map.get(key) {
                    collect_rates_from_value(child, rates);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_rates_from_value(item, rates);
            }
        }
        _ => {}
    }
}

fn group_items(payload: &Value) -> Vec<&Value> {
    let mut items = Vec::new();
    collect_group_items(payload, &mut items);
    items
}

fn collect_group_items<'a>(value: &'a Value, items: &mut Vec<&'a Value>) {
    match value {
        Value::Array(values) => items.extend(values.iter()),
        Value::Object(map) => {
            for key in [
                "data",
                "groups",
                "available_groups",
                "availableGroups",
                "items",
                "list",
            ] {
                if let Some(child) = map.get(key) {
                    collect_group_items(child, items);
                }
            }
        }
        _ => {}
    }
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(*key))
        .find_map(scalar_text)
}

fn scalar_text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(ToString::to_string)
        .or_else(|| value.as_i64().map(|item| item.to_string()))
        .or_else(|| value.as_u64().map(|item| item.to_string()))
        .or_else(|| value.as_f64().map(|item| item.to_string()))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn numeric_field(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .filter_map(|key| value.get(*key))
        .find_map(Value::as_f64)
}

fn stable_group_key_hash(
    station_id: &str,
    adapter: &str,
    group_id: Option<&str>,
    group_name: &str,
) -> String {
    let adapter = adapter.trim().to_lowercase();
    let source = if let Some(group_id) = group_id.filter(|value| !value.trim().is_empty()) {
        format!("id:{adapter}:{}", group_id.trim())
    } else {
        format!(
            "name:{}:{}:{}",
            station_id,
            adapter,
            group_name.trim().to_lowercase()
        )
    };
    sha256_hex(source.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn parse_remote_key_payload(station_id: &str, payload: &Value) -> Vec<RemoteStationKey> {
    remote_key_items(payload)
        .into_iter()
        .enumerate()
        .filter_map(|(index, value)| remote_key_from_value(station_id, value, index))
        .collect()
}

pub(crate) fn remote_key_items(payload: &Value) -> Vec<&Value> {
    if let Some(items) = payload.as_array() {
        return items.iter().collect();
    }
    for pointer in [
        "/data/items",
        "/data/list",
        "/data/keys",
        "/data",
        "/items",
        "/list",
        "/keys",
    ] {
        if let Some(value) = payload.pointer(pointer) {
            if let Some(items) = value.as_array() {
                return items.iter().collect();
            }
        }
    }
    if payload.is_object() {
        vec![payload]
    } else {
        Vec::new()
    }
}

pub(crate) fn remote_key_from_value(
    station_id: &str,
    value: &Value,
    index: usize,
) -> Option<RemoteStationKey> {
    let remote_key_id = string_field(value, &["id", "key_id", "keyId", "token_id", "tokenId"]);
    let name = string_field(value, &["name", "key_name", "keyName", "label", "remark"]);
    let full_key = full_key_from_key_value(value);
    let masked = string_field(
        value,
        &[
            "api_key_masked",
            "apiKeyMasked",
            "masked_key",
            "maskedKey",
            "key_masked",
        ],
    )
    .or_else(|| {
        full_key
            .as_deref()
            .map(crate::services::secrets::mask::mask_secret)
    });
    let (identity_kind, identity, include_index) = remote_key_identity(
        remote_key_id.as_deref(),
        full_key.as_deref(),
        masked.as_deref(),
        name.as_deref(),
    )?;
    let remote_key_id_hash = remote_key_id
        .as_deref()
        .map(|value| sha256_hex(value.as_bytes()));
    let explicit_group_id = string_field(value, &["group_id", "groupId"]);
    let group_name = string_field(value, &["group_name", "groupName", "group", "group_label"])
        .or_else(|| explicit_group_id.clone());
    let group_id_hash = match (explicit_group_id.as_deref(), group_name.as_deref()) {
        (Some(group_id), Some(group_name)) => Some(stable_group_key_hash(
            station_id,
            "sub2api",
            Some(group_id),
            group_name,
        )),
        (None, Some(group_name)) => Some(stable_group_key_hash(
            station_id, "sub2api", None, group_name,
        )),
        _ => None,
    };
    let identity_seed = if include_index {
        format!("{station_id}:{identity_kind}:{identity}:{index}")
    } else {
        format!("{station_id}:{identity_kind}:{identity}")
    };

    Some(RemoteStationKey {
        id: format!(
            "sub2api-remote-key-{}",
            &sha256_hex(identity_seed.as_bytes())[..16]
        ),
        station_id: station_id.to_string(),
        remote_key_id_hash,
        remote_key_name: name,
        api_key_masked: masked,
        api_key_fingerprint: full_key
            .as_deref()
            .and_then(crate::models::remote_keys::api_key_fingerprint),
        group_id_hash,
        group_name,
        tier_label: string_field(value, &["tier", "tier_label", "tierLabel", "plan"]),
        rate_multiplier: numeric_field(
            value,
            &[
                "rate_multiplier",
                "rateMultiplier",
                "ratio",
                "multiplier",
                "rate",
            ],
        ),
        rate_source: Some("sub2api_keys".to_string()),
        created_at: string_field(value, &["created_at", "createdAt", "created"]),
        last_used_at: string_field(value, &["last_used_at", "lastUsedAt", "last_used"]),
        raw_source: "sub2api_keys".to_string(),
        match_status: RemoteKeyMatchStatus::Unbound,
        matched_station_key_id: None,
        match_confidence: 0.0,
        collected_at: crate::services::time::now_millis_for_services().to_string(),
    })
}

pub(crate) fn remote_key_provider_id(value: &Value) -> Option<String> {
    string_field(value, &["id", "key_id", "keyId", "token_id", "tokenId"])
}

fn remote_key_identity<'a>(
    remote_key_id: Option<&'a str>,
    full_key: Option<&'a str>,
    masked: Option<&'a str>,
    name: Option<&'a str>,
) -> Option<(&'static str, &'a str, bool)> {
    remote_key_id
        .map(|value| ("remote_id", value, false))
        .or_else(|| full_key.map(|value| ("full_key", value, false)))
        .or_else(|| masked.map(|value| ("masked_key", value, false)))
        .or_else(|| name.map(|value| ("name", value, true)))
}

pub(crate) fn sub2api_group_id_value(group_id: &str) -> Value {
    let trimmed = group_id.trim();
    if let Ok(numeric_id) = trimmed.parse::<i64>() {
        if numeric_id.to_string() == trimmed {
            return json!(numeric_id);
        }
    }
    json!(trimmed)
}

pub(crate) fn remote_key_from_create_input(
    station_id: &str,
    input: &CreateRemoteStationKeyInput,
    full_key: Option<&str>,
) -> RemoteStationKey {
    let identity = full_key.unwrap_or(input.name.as_str());
    RemoteStationKey {
        id: format!(
            "sub2api-remote-key-{}",
            &sha256_hex(format!("{station_id}:{identity}").as_bytes())[..16]
        ),
        station_id: station_id.to_string(),
        remote_key_id_hash: None,
        remote_key_name: Some(input.name.clone()),
        api_key_masked: full_key.map(crate::services::secrets::mask::mask_secret),
        api_key_fingerprint: full_key.and_then(crate::models::remote_keys::api_key_fingerprint),
        group_id_hash: input.group_id_hash.clone(),
        group_name: input.group_name.clone(),
        tier_label: None,
        rate_multiplier: None,
        rate_source: Some("sub2api_keys".to_string()),
        created_at: None,
        last_used_at: None,
        raw_source: "sub2api_keys".to_string(),
        match_status: RemoteKeyMatchStatus::Unbound,
        matched_station_key_id: None,
        match_confidence: 0.0,
        collected_at: crate::services::time::now_millis_for_services().to_string(),
    }
}

pub(crate) fn full_key_from_key_value(value: &Value) -> Option<String> {
    string_field(value, &["key", "api_key", "apiKey", "token"])
        .filter(|value| looks_like_full_api_key(value))
}

pub(crate) fn full_key_from_create_payload(payload: &Value) -> Option<String> {
    full_key_from_key_value(payload)
        .or_else(|| full_key_at_pointer(payload, "/data/key"))
        .or_else(|| full_key_at_pointer(payload, "/data/api_key"))
        .or_else(|| full_key_at_pointer(payload, "/data/apiKey"))
}

fn full_key_at_pointer(payload: &Value, pointer: &str) -> Option<String> {
    payload
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| looks_like_full_api_key(value))
        .map(ToString::to_string)
}

fn looks_like_full_api_key(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.len() < 12 {
        return false;
    }
    let lower = trimmed.to_lowercase();
    if lower == "[redacted]"
        || lower == "<redacted>"
        || lower == "redacted"
        || lower == "masked"
        || lower.contains("redacted")
        || lower.contains("masked")
        || lower.contains("[redacted]")
        || lower.contains("<redacted>")
    {
        return false;
    }
    if trimmed.contains('*') || trimmed.contains("...") || trimmed.contains('\u{2026}') {
        return false;
    }
    if lower.starts_with("sk-") && lower.contains("xxx") {
        return false;
    }
    true
}

pub(crate) fn add_single_group_key_bindings(facts: &mut CollectorFacts, keys: &[StationKey]) {
    if facts.groups.len() != 1 {
        return;
    }
    let group = facts.groups[0].clone();
    let station_rate = facts
        .rates
        .iter()
        .find(|rate| rate.group_key_hash == group.group_key_hash)
        .cloned();

    for key in keys {
        facts.rates.push(CollectedRateFact {
            station_id: group.station_id.clone(),
            station_key_id: Some(key.id.clone()),
            group_id: group.group_id.clone(),
            group_key_hash: group.group_key_hash.clone(),
            group_name: group.group_name.clone(),
            default_rate_multiplier: station_rate
                .as_ref()
                .and_then(|rate| rate.default_rate_multiplier),
            user_rate_multiplier: station_rate
                .as_ref()
                .and_then(|rate| rate.user_rate_multiplier),
            effective_rate_multiplier: station_rate
                .as_ref()
                .and_then(|rate| rate.effective_rate_multiplier),
            inferred_group_category: station_rate
                .as_ref()
                .and_then(|rate| rate.inferred_group_category.clone())
                .or(group.inferred_group_category.clone()),
            source: "single_group_low_confidence".to_string(),
            confidence: 0.5,
            checked_at: None,
            raw_json_redacted: None,
        });
    }
}

pub(crate) fn merge_account_profile_balance(
    balances: &mut Vec<CollectedBalanceFact>,
    profile_balance: CollectedBalanceFact,
) {
    let Some(limit) = profile_balance.account_concurrency_limit else {
        return;
    };
    if let Some(station_balance) = balances.iter_mut().find(|balance| {
        balance.station_id == profile_balance.station_id && balance.scope == "station"
    }) {
        station_balance.account_concurrency_limit = Some(limit);
        return;
    }

    let mut merged_into_key_balance = false;
    for key_balance in balances.iter_mut().filter(|balance| {
        balance.station_id == profile_balance.station_id && balance.scope == "station_key"
    }) {
        key_balance.account_concurrency_limit = Some(limit);
        merged_into_key_balance = true;
    }

    if !merged_into_key_balance {
        balances.push(profile_balance);
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DashboardUsageStats {
    today_request_count: Option<i64>,
    total_request_count: Option<i64>,
    today_consumption: Option<f64>,
    total_consumption: Option<f64>,
    today_base_consumption: Option<f64>,
    total_base_consumption: Option<f64>,
    today_token_count: Option<i64>,
    total_token_count: Option<i64>,
    today_input_token_count: Option<i64>,
    today_output_token_count: Option<i64>,
    total_input_token_count: Option<i64>,
    total_output_token_count: Option<i64>,
}

impl DashboardUsageStats {
    pub(crate) fn has_any(self) -> bool {
        self.today_request_count.is_some()
            || self.total_request_count.is_some()
            || self.today_consumption.is_some()
            || self.total_consumption.is_some()
            || self.today_base_consumption.is_some()
            || self.total_base_consumption.is_some()
            || self.today_token_count.is_some()
            || self.total_token_count.is_some()
            || self.today_input_token_count.is_some()
            || self.today_output_token_count.is_some()
            || self.total_input_token_count.is_some()
            || self.total_output_token_count.is_some()
    }

    pub(crate) fn apply_to(self, balance: &mut CollectedBalanceFact) {
        balance.today_request_count = self.today_request_count;
        balance.total_request_count = self.total_request_count;
        balance.today_consumption = self.today_consumption;
        balance.total_consumption = self.total_consumption;
        balance.today_base_consumption = self.today_base_consumption;
        balance.total_base_consumption = self.total_base_consumption;
        balance.today_token_count = self.today_token_count;
        balance.total_token_count = self.total_token_count;
        balance.today_input_token_count = self.today_input_token_count;
        balance.today_output_token_count = self.today_output_token_count;
        balance.total_input_token_count = self.total_input_token_count;
        balance.total_output_token_count = self.total_output_token_count;
    }
}

pub(crate) fn parse_dashboard_usage_stats(payload: &Value) -> Option<DashboardUsageStats> {
    let mut candidates = vec![payload];
    for pointer in ["/data", "/stats", "/data/stats"] {
        if let Some(candidate) = payload.pointer(pointer) {
            candidates.push(candidate);
        }
    }

    let find_i64 = |names: &[&str]| {
        candidates
            .iter()
            .find_map(|candidate| parse_i64_field(candidate, names))
    };
    let find_f64 = |names: &[&str]| {
        candidates
            .iter()
            .find_map(|candidate| parse_f64_field(candidate, names))
    };
    let stats = DashboardUsageStats {
        today_request_count: find_i64(&[
            "today_request_count",
            "today_requests",
            "todayRequestCount",
            "todayRequests",
        ]),
        total_request_count: find_i64(&[
            "total_request_count",
            "total_requests",
            "request_count",
            "totalRequests",
            "requestCount",
            "requests",
        ]),
        today_consumption: find_f64(&[
            "today_consumption",
            "today_actual_cost",
            "today_used_amount",
            "todayConsume",
            "todayConsumption",
            "todayActualCost",
            "todayUsedAmount",
            "today_cost",
        ]),
        total_consumption: find_f64(&[
            "total_consumption",
            "total_actual_cost",
            "used_amount",
            "totalUsedAmount",
            "totalConsumption",
            "totalActualCost",
            "consumption",
            "cost",
        ]),
        today_base_consumption: find_f64(TODAY_BASE_CONSUMPTION_FIELDS),
        total_base_consumption: find_f64(TOTAL_BASE_CONSUMPTION_FIELDS),
        today_token_count: find_i64(&[
            "today_token_count",
            "today_tokens",
            "todayTokenCount",
            "todayTokens",
        ]),
        total_token_count: find_i64(&[
            "total_token_count",
            "total_tokens",
            "token_count",
            "totalTokenCount",
            "totalTokens",
            "tokens",
        ]),
        today_input_token_count: find_i64(&[
            "today_input_token_count",
            "today_input_tokens",
            "today_prompt_tokens",
            "todayInputTokenCount",
            "todayInputTokens",
            "todayPromptTokens",
        ]),
        today_output_token_count: find_i64(&[
            "today_output_token_count",
            "today_output_tokens",
            "today_completion_tokens",
            "todayOutputTokenCount",
            "todayOutputTokens",
            "todayCompletionTokens",
        ]),
        total_input_token_count: find_i64(&[
            "total_input_token_count",
            "total_input_tokens",
            "input_tokens",
            "prompt_tokens",
            "totalInputTokenCount",
            "totalInputTokens",
            "inputTokens",
            "promptTokens",
        ]),
        total_output_token_count: find_i64(&[
            "total_output_token_count",
            "total_output_tokens",
            "output_tokens",
            "completion_tokens",
            "totalOutputTokenCount",
            "totalOutputTokens",
            "outputTokens",
            "completionTokens",
        ]),
    };
    stats.has_any().then_some(stats)
}

pub(crate) fn merge_dashboard_usage_stats(
    balances: &mut Vec<CollectedBalanceFact>,
    station_id: &str,
    stats: DashboardUsageStats,
) {
    if !stats.has_any() {
        return;
    }
    if let Some(station_balance) = balances
        .iter_mut()
        .find(|balance| balance.station_id == station_id && balance.scope == "station")
    {
        stats.apply_to(station_balance);
        return;
    }

    let key_balances = balances
        .iter()
        .filter(|balance| balance.station_id == station_id && balance.scope == "station_key")
        .collect::<Vec<_>>();
    let Some(value) = sum_present_f64_values(key_balances.iter().map(|balance| balance.value))
    else {
        return;
    };
    let used_value = sum_present_f64_values(key_balances.iter().map(|balance| balance.used_value));
    let total_value =
        sum_present_f64_values(key_balances.iter().map(|balance| balance.total_value));
    let currency = shared_balance_text_value(
        key_balances
            .iter()
            .map(|balance| Some(balance.currency.as_str())),
    )
    .unwrap_or(NORMALIZED_BALANCE_CURRENCY)
    .to_string();
    let credit_unit = shared_balance_text_value(
        key_balances
            .iter()
            .map(|balance| balance.credit_unit.as_deref()),
    )
    .map(ToString::to_string);
    let account_concurrency_limit = key_balances
        .iter()
        .find_map(|balance| balance.account_concurrency_limit);
    let confidence = key_balances
        .iter()
        .map(|balance| balance.confidence)
        .fold(1.0_f64, f64::min);
    let collected_at = key_balances
        .iter()
        .filter_map(|balance| balance.collected_at.as_ref())
        .max()
        .cloned();
    let mut station_balance = CollectedBalanceFact {
        station_id: station_id.to_string(),
        station_key_id: None,
        scope: "station".to_string(),
        value: Some(value),
        used_value,
        total_value,
        today_request_count: None,
        total_request_count: None,
        today_consumption: None,
        total_consumption: None,
        today_base_consumption: None,
        total_base_consumption: None,
        today_token_count: None,
        total_token_count: None,
        today_input_token_count: None,
        today_output_token_count: None,
        total_input_token_count: None,
        total_output_token_count: None,
        account_concurrency_limit,
        currency,
        credit_unit,
        status: if value == 0.0 { "depleted" } else { "normal" }.to_string(),
        source: "station_key_balance_aggregate".to_string(),
        confidence,
        collected_at,
    };
    stats.apply_to(&mut station_balance);
    balances.push(station_balance);
}

fn sum_present_f64_values(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    let mut total = 0.0_f64;
    let mut has_value = false;
    for value in values.flatten() {
        total += value;
        has_value = true;
    }
    has_value.then_some(total)
}

fn shared_balance_text_value<'a>(
    mut values: impl Iterator<Item = Option<&'a str>>,
) -> Option<&'a str> {
    let first = values.find_map(|value| value)?;
    values
        .flatten()
        .all(|value| value == first)
        .then_some(first)
}

pub(crate) fn parse_account_balance(
    station_id: &str,
    payload: &Value,
    credit_per_cny: f64,
) -> Option<CollectedBalanceFact> {
    let value = parse_account_credit_value(payload, "balance", "remaining");
    let account_concurrency_limit = parse_account_concurrency_limit(payload);
    if value.is_none() && account_concurrency_limit.is_none() {
        return None;
    }
    let used = parse_account_credit_value(payload, "used", "used");
    let total = parse_account_credit_value(payload, "total", "total");
    Some(CollectedBalanceFact {
        station_id: station_id.to_string(),
        station_key_id: None,
        scope: "station".to_string(),
        value: normalize_credit_value(value, credit_per_cny),
        used_value: normalize_credit_value(used, credit_per_cny),
        total_value: normalize_credit_value(total, credit_per_cny),
        today_request_count: parse_account_i64_field(
            payload,
            &[
                "today_request_count",
                "today_requests",
                "todayRequestCount",
                "todayRequests",
            ],
        ),
        total_request_count: parse_account_i64_field(
            payload,
            &[
                "total_request_count",
                "request_count",
                "totalRequests",
                "requestCount",
                "requests",
            ],
        ),
        today_consumption: parse_account_f64_field(
            payload,
            &[
                "today_consumption",
                "today_used_amount",
                "todayConsume",
                "todayConsumption",
                "todayUsedAmount",
                "today_cost",
            ],
        ),
        total_consumption: parse_account_f64_field(
            payload,
            &[
                "total_consumption",
                "used_amount",
                "totalUsedAmount",
                "totalConsumption",
                "consumption",
                "cost",
            ],
        ),
        today_base_consumption: parse_account_f64_field(payload, TODAY_BASE_CONSUMPTION_FIELDS),
        total_base_consumption: parse_account_f64_field(payload, TOTAL_BASE_CONSUMPTION_FIELDS),
        today_token_count: parse_account_i64_field(
            payload,
            &[
                "today_token_count",
                "today_tokens",
                "todayTokenCount",
                "todayTokens",
            ],
        ),
        total_token_count: parse_account_i64_field(
            payload,
            &[
                "total_token_count",
                "total_tokens",
                "token_count",
                "totalTokenCount",
                "totalTokens",
                "tokens",
            ],
        ),
        today_input_token_count: parse_account_i64_field(
            payload,
            &[
                "today_input_token_count",
                "today_input_tokens",
                "today_prompt_tokens",
                "todayInputTokenCount",
                "todayInputTokens",
                "todayPromptTokens",
            ],
        ),
        today_output_token_count: parse_account_i64_field(
            payload,
            &[
                "today_output_token_count",
                "today_output_tokens",
                "today_completion_tokens",
                "todayOutputTokenCount",
                "todayOutputTokens",
                "todayCompletionTokens",
            ],
        ),
        total_input_token_count: parse_account_i64_field(
            payload,
            &[
                "total_input_token_count",
                "total_input_tokens",
                "input_tokens",
                "prompt_tokens",
                "totalInputTokenCount",
                "totalInputTokens",
                "inputTokens",
                "promptTokens",
            ],
        ),
        total_output_token_count: parse_account_i64_field(
            payload,
            &[
                "total_output_token_count",
                "total_output_tokens",
                "output_tokens",
                "completion_tokens",
                "totalOutputTokenCount",
                "totalOutputTokens",
                "outputTokens",
                "completionTokens",
            ],
        ),
        account_concurrency_limit,
        currency: NORMALIZED_BALANCE_CURRENCY.to_string(),
        credit_unit: None,
        status: if value == Some(0.0) {
            "depleted"
        } else if value.is_some() {
            "normal"
        } else {
            "unknown"
        }
        .to_string(),
        source: "sub2api_account_profile".to_string(),
        confidence: 0.85,
        collected_at: None,
    })
}

fn account_profile_candidates(payload: &Value) -> [Option<&Value>; 6] {
    [
        payload.pointer("/data"),
        Some(payload),
        payload.pointer("/data/user"),
        payload.pointer("/data/profile"),
        payload.get("user"),
        payload.get("profile"),
    ]
}

fn parse_account_f64_field(payload: &Value, names: &[&str]) -> Option<f64> {
    account_profile_candidates(payload)
        .into_iter()
        .flatten()
        .find_map(|candidate| parse_f64_field(candidate, names))
}

fn parse_account_i64_field(payload: &Value, names: &[&str]) -> Option<i64> {
    account_profile_candidates(payload)
        .into_iter()
        .flatten()
        .find_map(|candidate| parse_i64_field(candidate, names))
}

fn parse_account_credit_value(
    payload: &Value,
    direct_field: &str,
    quota_field: &str,
) -> Option<f64> {
    account_profile_candidates(payload)
        .into_iter()
        .flatten()
        .find_map(|candidate| {
            parse_optional_f64(candidate.get(direct_field))
                .or_else(|| parse_optional_f64(candidate.pointer(&format!("/quota/{quota_field}"))))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_key_parser_reads_nested_key_payloads() {
        let keys = parse_remote_key_payload(
            "station-1",
            &json!({
                "data": {
                    "items": [
                        {
                            "id": "remote-1",
                            "name": "primary",
                            "key": "sk-test-full-key-123456",
                            "group_id": "vip",
                            "group_name": "VIP",
                            "rate_multiplier": 2.0
                        }
                    ]
                }
            }),
        );

        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].remote_key_name.as_deref(), Some("primary"));
        assert!(keys[0].api_key_masked.is_some());
        assert_ne!(
            keys[0].api_key_masked.as_deref(),
            Some("sk-test-full-key-123456")
        );
        assert_eq!(keys[0].group_name.as_deref(), Some("VIP"));
        assert_eq!(keys[0].rate_multiplier, Some(2.0));
    }

    #[test]
    fn full_key_parser_rejects_masked_values() {
        assert_eq!(
            full_key_from_key_value(&json!({"key": "sk-1234********abcd"})),
            None
        );
        assert_eq!(
            full_key_from_create_payload(&json!({"data": {"apiKey": "sk-test-full-key-abcdef"}})),
            Some("sk-test-full-key-abcdef".to_string())
        );
    }

    #[test]
    fn group_rate_parser_keeps_available_group_and_rate() {
        let facts = parse_group_rate_facts(
            "station-1",
            &json!({"data": [{"id": "vip", "name": "VIP"}]}),
            &json!({"data": {"vip": 1.5}}),
            500_000.0,
        );

        assert_eq!(facts.groups.len(), 1);
        assert_eq!(facts.groups[0].group_name, "VIP");
        assert_eq!(facts.rates.len(), 1);
        assert_eq!(facts.rates[0].effective_rate_multiplier, Some(1.5));
    }

    #[test]
    fn balance_parsers_normalize_currency_to_usd() {
        let key_balance = parse_usage_balance(
            "station-1",
            Some("key-1".to_string()),
            &json!({"quota": {"remaining": 72.8, "unit": "CNY"}}),
            10.0,
        );
        let account_balance = parse_account_balance(
            "station-1",
            &json!({"data": {"balance": 7.28, "currency": "CNY"}}),
            1.0,
        )
        .expect("account balance");

        assert!((key_balance.value.expect("key balance") - 7.28).abs() < f64::EPSILON * 10.0);
        assert_eq!(key_balance.currency, NORMALIZED_BALANCE_CURRENCY);
        assert_eq!(account_balance.currency, NORMALIZED_BALANCE_CURRENCY);
    }

    #[test]
    fn account_profile_and_dashboard_stats_merge_into_balance() {
        let mut balances = vec![parse_usage_balance(
            "station-1",
            Some("key-1".to_string()),
            &json!({"quota": {"remaining": 10.0, "used": 2.0}}),
            1.0,
        )];
        let profile =
            parse_account_balance("station-1", &json!({"data": {"concurrency_limit": 8}}), 1.0)
                .expect("profile balance");
        merge_account_profile_balance(&mut balances, profile);
        let stats = parse_dashboard_usage_stats(&json!({
            "data": {
                "today_request_count": 4,
                "total_request_count": 40,
                "today_consumption": 0.25
            }
        }))
        .expect("dashboard stats");
        merge_dashboard_usage_stats(&mut balances, "station-1", stats);

        assert_eq!(balances.len(), 2);
        assert_eq!(balances[0].account_concurrency_limit, Some(8));
        let station_balance = balances
            .iter()
            .find(|balance| balance.scope == "station")
            .expect("station aggregate");
        assert_eq!(station_balance.today_request_count, Some(4));
        assert_eq!(station_balance.total_request_count, Some(40));
        assert_eq!(station_balance.today_consumption, Some(0.25));
    }
}
