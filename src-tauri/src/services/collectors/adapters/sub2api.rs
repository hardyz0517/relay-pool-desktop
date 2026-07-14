use std::collections::HashMap;

use serde_json::{json, Value};

use crate::{
    models::{
        credentials::UpdateStationSessionInput,
        remote_keys::{
            CreateRemoteStationKeyInput, RemoteKeyCapability, RemoteKeyMatchStatus,
            RemoteStationKey,
        },
        station_keys::StationKey,
        stations::Station,
    },
    services::{
        collectors::{
            adapters::{
                request_recovery::{
                    execute_json_request, CollectionAttemptBudget,
                    EndpointJsonResult as RecoverableEndpointJsonResult, RequestPolicy,
                },
                AdapterOutput, CollectorTask, CreatedRemoteKey,
            },
            facts::{CollectedBalanceFact, CollectedGroupFact, CollectedRateFact, CollectorFacts},
        },
        database::AppDatabase,
        group_categories::infer_group_category,
        outbound::{resolve_proxy_config, ProxyConfig},
        station_endpoints::{build_api_url, build_management_url},
    },
};

const COLLECTOR_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const COLLECTOR_TASK_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);
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
        currency: "CNY".to_string(),
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

pub fn remote_key_capability(station: &Station) -> Result<RemoteKeyCapability, String> {
    Ok(RemoteKeyCapability {
        station_id: station.id.clone(),
        station_type: station.station_type.trim().to_string(),
        can_list_remote_keys: true,
        can_create_remote_key: true,
        can_read_groups: true,
        requires_manual_session: true,
        unsupported_reason: None,
    })
}

pub fn scan_remote_keys(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
) -> Result<Vec<RemoteStationKey>, String> {
    let station = database.station_for_collector(station_id)?;
    let proxy = effective_station_proxy(database, &station)?;
    let access_token = resolve_sub2api_access_token(database, data_key, &station)?;
    let url = build_management_url(&station.website_url, "/api/v1/keys?page=1&page_size=100")?;
    let result = fetch_json_with_bearer(&url, &access_token, &proxy);
    let payload = result.payload.unwrap_or(Value::Null);
    if !result.ok {
        return Err(result.error_message.unwrap_or_else(|| {
            format!("Sub2API 远端 Key 扫描失败，HTTP 状态 {:?}。", result.status)
        }));
    }

    Ok(parse_remote_key_payload(&station.id, &payload))
}

pub fn scan_remote_key_full_secret(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    remote_key_id: &str,
) -> Result<(RemoteStationKey, String), String> {
    let station = database.station_for_collector(station_id)?;
    let proxy = effective_station_proxy(database, &station)?;
    let access_token = resolve_sub2api_access_token(database, data_key, &station)?;
    let url = build_management_url(&station.website_url, "/api/v1/keys?page=1&page_size=100")?;
    let result = fetch_json_with_bearer(&url, &access_token, &proxy);
    let payload = result.payload.unwrap_or(Value::Null);
    if !result.ok {
        return Err(result.error_message.unwrap_or_else(|| {
            format!("Sub2API 远端 Key 读取失败，HTTP 状态 {:?}。", result.status)
        }));
    }

    for (index, value) in remote_key_items(&payload).into_iter().enumerate() {
        let Some(remote_key) = remote_key_from_value(&station.id, value, index) else {
            continue;
        };
        if remote_key.id != remote_key_id {
            continue;
        }
        let full_key = full_key_from_key_value(value).ok_or_else(|| {
            "远端 Key 列表没有返回完整 Key，无法自动保存到本地。请到网站复制后手动补全。"
                .to_string()
        })?;
        return Ok((remote_key, full_key));
    }

    Err("远端 Key 已不存在，无法创建本地 Key。".to_string())
}

pub fn create_remote_key(
    database: &AppDatabase,
    data_key: &[u8; 32],
    input: CreateRemoteStationKeyInput,
) -> Result<CreatedRemoteKey, String> {
    let station = database.station_for_collector(&input.station_id)?;
    let proxy = effective_station_proxy(database, &station)?;
    let access_token = resolve_sub2api_access_token(database, data_key, &station)?;
    let url = build_management_url(&station.website_url, "/api/v1/keys")?;
    let mut body = json!({
        "name": input.name,
    });
    if let Some(group_name) = input
        .group_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        body["group"] = json!(group_name);
    }
    if let Some(group_id) = remote_group_id_for_create(database, &input)? {
        body["group_id"] = sub2api_group_id_value(&group_id);
    }

    let result = post_json_with_bearer(&url, &access_token, &body, &proxy);
    let payload = result.payload.unwrap_or(Value::Null);
    if !result.ok {
        return Err(result.error_message.unwrap_or_else(|| {
            format!("Sub2API 远端 Key 创建失败，HTTP 状态 {:?}。", result.status)
        }));
    }

    let full_key_once = full_key_from_create_payload(&payload);
    if full_key_once.is_none() {
        return Err(
            "Sub2API 远端 Key 已创建，但响应没有返回完整 Key，无法自动保存到本地。请到网站复制后手动添加。"
                .to_string(),
        );
    }
    let remote_key = parse_remote_key_payload(&station.id, &payload)
        .into_iter()
        .next()
        .unwrap_or_else(|| {
            remote_key_from_create_input(&station.id, &input, full_key_once.as_deref())
        });
    Ok(CreatedRemoteKey {
        remote_key,
        full_key_once,
        message: "Sub2API 远端 Key 已创建。".to_string(),
    })
}

fn resolve_sub2api_access_token(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
) -> Result<String, String> {
    let session = database.resolve_station_session_with_data_key(
        station.id.clone(),
        data_key,
        crate::services::database::now_millis_for_services() as i64,
    )?;
    if let Some(access_token) = session
        .access_token
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Ok(access_token);
    }
    if let Some(access_token) = login_and_store_access_token(database, data_key, station)? {
        return Ok(access_token);
    }
    Err(session.message.unwrap_or_else(|| {
        "Sub2API 远端 Key 管理需要 access token，请先导入手动会话或保存登录账号密码。".to_string()
    }))
}

fn effective_station_proxy(
    database: &AppDatabase,
    station: &Station,
) -> Result<ProxyConfig, String> {
    let settings = database.get_settings()?;
    Ok(resolve_proxy_config(
        &station.collector_proxy_mode,
        station.collector_proxy_url.clone(),
        &settings.collector_proxy_mode,
        settings.collector_proxy_url,
    ))
}

fn parse_remote_key_payload(station_id: &str, payload: &Value) -> Vec<RemoteStationKey> {
    remote_key_items(payload)
        .into_iter()
        .enumerate()
        .filter_map(|(index, value)| remote_key_from_value(station_id, value, index))
        .collect()
}

fn remote_key_items(payload: &Value) -> Vec<&Value> {
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

fn remote_key_from_value(
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
            .and_then(crate::services::remote_keys::api_key_fingerprint),
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
        collected_at: crate::services::database::now_millis_for_services().to_string(),
    })
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

fn remote_group_id_for_create(
    database: &AppDatabase,
    input: &CreateRemoteStationKeyInput,
) -> Result<Option<String>, String> {
    let Some(group_binding_id) = input
        .group_binding_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let bindings = database.list_station_group_bindings(input.station_id.clone())?;
    Ok(bindings
        .into_iter()
        .find(|binding| {
            binding.id == group_binding_id
                && binding.binding_kind == "station_group"
                && binding.binding_status != "disabled"
        })
        .and_then(|binding| binding.group_id_hash)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

fn sub2api_group_id_value(group_id: &str) -> Value {
    let trimmed = group_id.trim();
    if let Ok(numeric_id) = trimmed.parse::<i64>() {
        if numeric_id.to_string() == trimmed {
            return json!(numeric_id);
        }
    }
    json!(trimmed)
}

fn remote_key_from_create_input(
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
        api_key_fingerprint: full_key.and_then(crate::services::remote_keys::api_key_fingerprint),
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
        collected_at: crate::services::database::now_millis_for_services().to_string(),
    }
}

fn full_key_from_key_value(value: &Value) -> Option<String> {
    string_field(value, &["key", "api_key", "apiKey", "token"])
        .filter(|value| looks_like_full_api_key(value))
}

fn full_key_from_create_payload(payload: &Value) -> Option<String> {
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
    if trimmed.contains('*') || trimmed.contains("...") || trimmed.contains('…') {
        return false;
    }
    if lower.starts_with("sk-") && lower.contains("xxx") {
        return false;
    }
    true
}

fn routeable_keys_for_station(
    database: &AppDatabase,
    station_id: &str,
) -> Result<Vec<StationKey>, String> {
    database
        .list_station_keys(station_id.to_string())
        .map(|keys| {
            keys.into_iter()
                .filter(|key| key.enabled && key.api_key_present)
                .collect()
        })
}

pub fn collect(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    task: CollectorTask,
) -> Result<AdapterOutput, String> {
    match task {
        CollectorTask::Detect => Ok(AdapterOutput {
            adapter: "sub2api".to_string(),
            task,
            status: "success".to_string(),
            summary_json: json!({
                "adapter": "sub2api",
                "task": "detect",
                "message": "Sub2API adapter 已确认。",
                "endpointResults": [],
            }),
            normalized_json: json!({
                "adapter": "sub2api",
                "models": [],
            }),
            raw_json_redacted: None,
            error_code: None,
            error_message: None,
            facts: CollectorFacts::default(),
        }),
        CollectorTask::Balance => collect_balance(database, data_key, station_id),
        CollectorTask::Groups => collect_groups(database, data_key, station_id),
        CollectorTask::Models => unsupported_output(
            task,
            "unsupported_task",
            "Sub2API adapter 暂不支持模型列表采集。",
        ),
        CollectorTask::Full => {
            unsupported_output(task, "internal_task", "Full 采集由父任务拆分为子任务执行。")
        }
    }
}

fn unsupported_output(
    task: CollectorTask,
    code: &str,
    message: &str,
) -> Result<AdapterOutput, String> {
    Ok(AdapterOutput {
        adapter: "sub2api".to_string(),
        task,
        status: "manual_required".to_string(),
        summary_json: json!({
            "adapter": "sub2api",
            "task": task.as_str(),
            "message": message,
            "endpointResults": [],
        }),
        normalized_json: json!({ "models": [] }),
        raw_json_redacted: None,
        error_code: Some(code.to_string()),
        error_message: Some(message.to_string()),
        facts: CollectorFacts::default(),
    })
}

pub fn collect_groups(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
) -> Result<AdapterOutput, String> {
    let station = database.station_for_collector(station_id)?;
    let proxy = effective_station_proxy(database, &station)?;
    let session = database.resolve_station_session_with_data_key(
        station_id.to_string(),
        data_key,
        crate::services::database::now_millis_for_services() as i64,
    )?;
    let mut session_source = "reused_session";
    let mut access_token = match session.access_token {
        Some(access_token) => access_token,
        None => match login_and_store_access_token(database, data_key, &station)? {
            Some(access_token) => {
                session_source = "password_login";
                access_token
            }
            None => {
                return Ok(manual_session_required_output(session.message));
            }
        },
    };

    let available_url = build_management_url(&station.website_url, "/api/v1/groups/available")?;
    let rates_url = build_management_url(&station.website_url, "/api/v1/groups/rates")?;
    let policy = collector_request_policy();
    let budget = CollectionAttemptBudget::new(policy.task_budget);
    let mut auth_refresh_started = false;
    let mut auth_refresh = |_: &String, remaining: std::time::Duration| {
        if auth_refresh_started {
            return None;
        }
        auth_refresh_started = true;
        login_and_store_access_token_with_budget(database, data_key, &station, remaining)
            .ok()
            .flatten()
    };
    let mut available_request = |token: &String, timeout: std::time::Duration| {
        fetch_recoverable_json_with_bearer(&available_url, token, timeout, &proxy)
    };
    let available_execution = execute_json_request(
        "/api/v1/groups/available",
        access_token.clone(),
        Some(&mut auth_refresh),
        &mut available_request,
        &policy,
        &budget,
    );
    if let Some(refreshed) = available_execution.latest_credential.clone() {
        session_source = "auth_refresh";
        access_token = refreshed;
    }
    let mut rates_request = |token: &String, timeout: std::time::Duration| {
        fetch_recoverable_json_with_bearer(&rates_url, token, timeout, &proxy)
    };
    let rates_execution = execute_json_request(
        "/api/v1/groups/rates",
        access_token.clone(),
        Some(&mut auth_refresh),
        &mut rates_request,
        &policy,
        &budget,
    );
    let available_payload = available_execution
        .result
        .payload
        .clone()
        .unwrap_or(Value::Null);
    let rates_payload = rates_execution
        .result
        .payload
        .clone()
        .unwrap_or(Value::Null);
    let mut facts = parse_group_rate_facts(
        &station.id,
        &available_payload,
        &rates_payload,
        station.credit_per_cny,
    );
    let keys = routeable_keys_for_station(database, station_id)?;
    add_single_group_key_bindings(&mut facts, &keys);

    let endpoint_results = json!([
        available_execution.to_redacted_json(),
        rates_execution.to_redacted_json(),
    ]);
    let success_count = [
        available_execution.result.ok && available_execution.failure_kind.is_none(),
        rates_execution.result.ok && rates_execution.failure_kind.is_none(),
    ]
    .into_iter()
    .filter(|ok| *ok)
    .count();
    let status = match success_count {
        2 => "success",
        1 if !facts.groups.is_empty() => "partial",
        _ => "failed",
    };
    let group_count = facts.groups.len();
    let rate_count = facts.rates.len();

    Ok(AdapterOutput {
        adapter: "sub2api".to_string(),
        task: CollectorTask::Groups,
        status: status.to_string(),
        summary_json: json!({
            "adapter": "sub2api",
            "task": "groups",
            "sessionSource": session_source,
            "endpointResults": endpoint_results,
            "groups": group_count,
            "rates": rate_count,
        }),
        normalized_json: json!({ "groups": group_count, "rates": rate_count }),
        raw_json_redacted: Some(json!({ "endpointResults": endpoint_results })),
        error_code: if status == "failed" {
            Some("no_group_rate_facts".to_string())
        } else {
            None
        },
        error_message: if status == "failed" {
            Some("未采集到 Sub2API 分组或倍率。".to_string())
        } else {
            None
        },
        facts,
    })
}

fn manual_session_required_output(message: Option<String>) -> AdapterOutput {
    AdapterOutput {
        adapter: "sub2api".to_string(),
        task: CollectorTask::Groups,
        status: "manual_required".to_string(),
        summary_json: json!({
            "adapter": "sub2api",
            "task": "groups",
            "message": message.unwrap_or_else(|| "Sub2API 分组采集需要 access token。".to_string()),
        }),
        normalized_json: json!({ "groups": 0, "rates": 0 }),
        raw_json_redacted: None,
        error_code: Some("manual_session_required".to_string()),
        error_message: Some("Sub2API 分组采集需要 access token。".to_string()),
        facts: CollectorFacts::default(),
    }
}

fn login_and_store_access_token(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &crate::models::stations::Station,
) -> Result<Option<String>, String> {
    let credentials = database.get_station_credentials(station.id.clone())?;
    let Some(username) = credentials
        .login_username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if !credentials.password_present {
        return Ok(None);
    }
    let Some(password) =
        database.get_station_login_password_with_data_key(station.id.clone(), data_key)?
    else {
        return Ok(None);
    };
    let proxy = effective_station_proxy(database, station)?;
    let login = crate::services::collectors::sub2api::login_access_token_with_proxy(
        &station.website_url,
        username,
        &password,
        &proxy,
    )?;
    let Some(access_token) = login.access_token else {
        return Ok(None);
    };

    database.update_station_session_with_data_key(
        UpdateStationSessionInput {
            station_id: station.id.clone(),
            access_token: Some(access_token.clone()),
            refresh_token: None,
            cookie: None,
            newapi_user_id: credentials.newapi_user_id,
            token_expires_at: None,
        },
        data_key,
    )?;

    Ok(Some(access_token))
}

fn login_and_store_access_token_with_budget(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &crate::models::stations::Station,
    budget: std::time::Duration,
) -> Result<Option<String>, String> {
    let credentials = database.get_station_credentials(station.id.clone())?;
    let Some(username) = credentials
        .login_username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if !credentials.password_present {
        return Ok(None);
    }
    let Some(password) =
        database.get_station_login_password_with_data_key(station.id.clone(), data_key)?
    else {
        return Ok(None);
    };
    let proxy = effective_station_proxy(database, station)?;
    let login = crate::services::collectors::sub2api::login_access_token_with_budget_and_proxy(
        &station.website_url,
        username,
        &password,
        budget,
        &proxy,
    )?;
    let Some(access_token) = login.access_token else {
        return Ok(None);
    };

    database.update_station_session_with_data_key(
        UpdateStationSessionInput {
            station_id: station.id.clone(),
            access_token: Some(access_token.clone()),
            refresh_token: None,
            cookie: None,
            newapi_user_id: credentials.newapi_user_id,
            token_expires_at: None,
        },
        data_key,
    )?;

    Ok(Some(access_token))
}

fn collector_request_policy() -> RequestPolicy {
    RequestPolicy {
        max_attempts: 3,
        malformed_json_max_attempts: 2,
        task_budget: COLLECTOR_TASK_BUDGET,
        retry_delays: vec![
            std::time::Duration::from_millis(300),
            std::time::Duration::from_secs(1),
        ],
    }
}

#[derive(Debug, Clone)]
struct EndpointJsonResult {
    status: Option<u16>,
    ok: bool,
    payload: Option<Value>,
    error_message: Option<String>,
}

fn fetch_json_with_bearer(
    url: &str,
    access_token: &str,
    proxy: &ProxyConfig,
) -> EndpointJsonResult {
    let agent = match crate::services::outbound::credential_agent_builder_for_proxy(proxy) {
        Ok(builder) => builder.timeout(COLLECTOR_HTTP_TIMEOUT).build(),
        Err(error) => {
            return EndpointJsonResult {
                status: None,
                ok: false,
                payload: None,
                error_message: Some(crate::services::secrets::mask::redact_text(&error)),
            };
        }
    };
    let response = match agent
        .get(url)
        .timeout(COLLECTOR_HTTP_TIMEOUT)
        .set("Authorization", &format!("Bearer {access_token}"))
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => {
            return EndpointJsonResult {
                status: None,
                ok: false,
                payload: None,
                error_message: Some(crate::services::secrets::mask::redact_text(
                    &error.to_string(),
                )),
            };
        }
    };
    let status = response.status();
    let text = response.into_string().unwrap_or_default();
    let payload = serde_json::from_str::<Value>(&text).ok();

    EndpointJsonResult {
        status: Some(status),
        ok: (200..400).contains(&status),
        payload,
        error_message: None,
    }
}

fn fetch_recoverable_json_with_bearer(
    url: &str,
    access_token: &str,
    timeout: std::time::Duration,
    proxy: &ProxyConfig,
) -> RecoverableEndpointJsonResult {
    let started = std::time::Instant::now();
    let agent = match crate::services::outbound::credential_agent_builder_for_proxy(proxy) {
        Ok(builder) => builder.timeout(timeout.min(COLLECTOR_HTTP_TIMEOUT)).build(),
        Err(error) => {
            return RecoverableEndpointJsonResult {
                url: url.to_string(),
                status: None,
                ok: false,
                duration_ms: started.elapsed().as_millis() as i64,
                payload: None,
                error_message: Some(crate::services::secrets::mask::redact_text(&error)),
                retry_after: None,
            };
        }
    };
    let response = match agent
        .get(url)
        .timeout(timeout.min(COLLECTOR_HTTP_TIMEOUT))
        .set("Authorization", &format!("Bearer {access_token}"))
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => {
            return RecoverableEndpointJsonResult {
                url: url.to_string(),
                status: None,
                ok: false,
                duration_ms: started.elapsed().as_millis() as i64,
                payload: None,
                error_message: Some(crate::services::secrets::mask::redact_text(
                    &error.to_string(),
                )),
                retry_after: None,
            };
        }
    };
    let status = response.status();
    let retry_after = response
        .header("retry-after")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(std::time::Duration::from_secs);
    let text = response.into_string().unwrap_or_default();
    let payload = serde_json::from_str::<Value>(&text).ok();
    let ok = (200..400).contains(&status) && payload.is_some();
    let error_message = if ok {
        None
    } else {
        Some(crate::services::secrets::mask::redact_text(&text))
    };

    RecoverableEndpointJsonResult {
        url: url.to_string(),
        status: Some(status),
        ok,
        duration_ms: started.elapsed().as_millis() as i64,
        payload,
        error_message,
        retry_after,
    }
}

fn result_to_balance_endpoint_json(
    result: &RecoverableEndpointJsonResult,
    url: &str,
    station_key_id: &str,
) -> Value {
    json!({
        "endpoint": url,
        "url": result.url,
        "stationKeyId": station_key_id,
        "status": result.status,
        "durationMs": result.duration_ms,
        "ok": result.ok,
        "errorMessage": result.error_message,
    })
}

fn append_balance_endpoint_attempt(endpoint: &mut Value, result: &RecoverableEndpointJsonResult) {
    let attempt = json!({
        "url": result.url,
        "status": result.status,
        "durationMs": result.duration_ms,
        "ok": result.ok,
        "errorMessage": result.error_message,
    });
    if endpoint.get("attempts").is_none() {
        let first_attempt = json!({
            "url": endpoint.get("url").cloned().unwrap_or(Value::Null),
            "status": endpoint.get("status").cloned().unwrap_or(Value::Null),
            "durationMs": endpoint.get("durationMs").cloned().unwrap_or(Value::Null),
            "ok": endpoint.get("ok").cloned().unwrap_or(Value::Null),
            "errorMessage": endpoint.get("errorMessage").cloned().unwrap_or(Value::Null),
        });
        endpoint["attempts"] = json!([first_attempt]);
    }
    if let Some(attempts) = endpoint["attempts"].as_array_mut() {
        attempts.push(attempt);
        endpoint["attemptCount"] = json!(attempts.len());
    }
    endpoint["url"] = json!(result.url);
    endpoint["status"] = json!(result.status);
    endpoint["durationMs"] = json!(result.duration_ms);
    endpoint["ok"] = json!(result.ok);
    endpoint["errorMessage"] = json!(result.error_message);
    endpoint["recoveryActions"] = json!(["transient_retry"]);
}

fn balance_request_timeout(budget: &CollectionAttemptBudget) -> std::time::Duration {
    budget
        .remaining()
        .unwrap_or_else(|| std::time::Duration::from_millis(1))
        .min(COLLECTOR_HTTP_TIMEOUT)
        .max(std::time::Duration::from_millis(1))
}

fn post_json_with_bearer(
    url: &str,
    access_token: &str,
    body: &Value,
    proxy: &ProxyConfig,
) -> EndpointJsonResult {
    let agent = match crate::services::outbound::credential_agent_builder_for_proxy(proxy) {
        Ok(builder) => builder.timeout(COLLECTOR_HTTP_TIMEOUT).build(),
        Err(error) => {
            return EndpointJsonResult {
                status: None,
                ok: false,
                payload: None,
                error_message: Some(crate::services::secrets::mask::redact_text(&error)),
            };
        }
    };
    let response = match agent
        .post(url)
        .timeout(COLLECTOR_HTTP_TIMEOUT)
        .set("Authorization", &format!("Bearer {access_token}"))
        .set("Content-Type", "application/json")
        .send_string(&body.to_string())
    {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => {
            return EndpointJsonResult {
                status: None,
                ok: false,
                payload: None,
                error_message: Some(crate::services::secrets::mask::redact_text(
                    &error.to_string(),
                )),
            };
        }
    };
    let status = response.status();
    let text = response.into_string().unwrap_or_default();
    let payload = serde_json::from_str::<Value>(&text).ok();
    let error_message = if (200..400).contains(&status) {
        None
    } else {
        Some(crate::services::secrets::mask::redact_text(&text))
    };

    EndpointJsonResult {
        status: Some(status),
        ok: (200..400).contains(&status),
        payload,
        error_message,
    }
}

fn add_single_group_key_bindings(facts: &mut CollectorFacts, keys: &[StationKey]) {
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

pub fn collect_balance(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
) -> Result<AdapterOutput, String> {
    let station = database.station_for_collector(station_id)?;
    let proxy = effective_station_proxy(database, &station)?;
    let keys = routeable_keys_for_station(database, station_id)?;
    let url = build_api_url(&station.api_base_url, "/v1/usage")?;
    let mut facts = CollectorFacts::default();
    let mut endpoint_results = Vec::new();
    let policy = collector_request_policy();
    let budget = CollectionAttemptBudget::new(policy.task_budget);
    let mut transient_retry_keys: Vec<(StationKey, String, usize)> = Vec::new();

    for key in keys {
        let api_key = match database.resolve_station_key_secret_with_data_key(data_key, &key.id) {
            Ok(api_key) => api_key,
            Err(error) => {
                endpoint_results.push(json!({
                    "endpoint": url,
                    "stationKeyId": key.id,
                    "status": "secret_error",
                    "message": crate::services::secrets::mask::redact_text(&error),
                }));
                continue;
            }
        };
        let result = fetch_recoverable_json_with_bearer(
            &url,
            &api_key,
            balance_request_timeout(&budget),
            &proxy,
        );
        let mut redacted = result_to_balance_endpoint_json(&result, &url, &key.id);
        redacted["attemptCount"] = json!(1);
        endpoint_results.push(redacted);
        let endpoint_index = endpoint_results.len() - 1;
        if result.ok {
            let parsed = result.payload.clone().unwrap_or(Value::Null);
            facts.balances.push(parse_usage_balance(
                &station.id,
                Some(key.id),
                &parsed,
                station.credit_per_cny,
            ));
        } else if matches!(result.status, None | Some(408 | 429 | 500..=599)) {
            transient_retry_keys.push((key, api_key, endpoint_index));
        }
    }
    for _round in 0..2 {
        if transient_retry_keys.is_empty() {
            break;
        }
        let retrying = std::mem::take(&mut transient_retry_keys);
        for (key, api_key, endpoint_index) in retrying {
            if budget.remaining().is_none() {
                break;
            }
            let result = fetch_recoverable_json_with_bearer(
                &url,
                &api_key,
                balance_request_timeout(&budget),
                &proxy,
            );
            if let Some(endpoint) = endpoint_results.get_mut(endpoint_index) {
                append_balance_endpoint_attempt(endpoint, &result);
            }
            if result.ok {
                let parsed = result.payload.clone().unwrap_or(Value::Null);
                facts.balances.push(parse_usage_balance(
                    &station.id,
                    Some(key.id),
                    &parsed,
                    station.credit_per_cny,
                ));
            } else if matches!(result.status, None | Some(408 | 429 | 500..=599)) {
                transient_retry_keys.push((key, api_key, endpoint_index));
            }
        }
    }
    if facts.balances.is_empty() {
        if let Some(balance) = collect_account_balance_fallback(
            database,
            data_key,
            &station,
            &proxy,
            &mut endpoint_results,
            &budget,
            &policy,
        )? {
            facts.balances.push(balance);
        }
    }
    if !facts.balances.is_empty() {
        if let Some(stats) = collect_dashboard_usage_stats(
            database,
            data_key,
            &station,
            &proxy,
            &mut endpoint_results,
            &budget,
            &policy,
        )? {
            merge_dashboard_usage_stats(&mut facts.balances, &station.id, stats);
        }
    }

    let status = if facts.balances.is_empty() {
        "failed"
    } else {
        "success"
    };
    let balance_count = facts.balances.len();

    Ok(AdapterOutput {
        adapter: "sub2api".to_string(),
        task: CollectorTask::Balance,
        status: status.to_string(),
        summary_json: json!({
            "adapter": "sub2api",
            "task": "balance",
            "endpointResults": endpoint_results,
        }),
        normalized_json: json!({ "balances": balance_count }),
        raw_json_redacted: Some(json!({ "endpointResults": endpoint_results })),
        error_code: if balance_count == 0 {
            Some("no_balance_facts".to_string())
        } else {
            None
        },
        error_message: if balance_count == 0 {
            Some("未采集到 Sub2API usage 余额。".to_string())
        } else {
            None
        },
        facts,
    })
}

fn collect_account_balance_fallback(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &crate::models::stations::Station,
    proxy: &ProxyConfig,
    endpoint_results: &mut Vec<Value>,
    budget: &CollectionAttemptBudget,
    policy: &RequestPolicy,
) -> Result<Option<CollectedBalanceFact>, String> {
    let session = database.resolve_station_session_with_data_key(
        station.id.clone(),
        data_key,
        crate::services::database::now_millis_for_services() as i64,
    )?;
    let access_token = match session.access_token {
        Some(access_token) => access_token,
        None => {
            let Some(remaining) = budget.remaining() else {
                return Ok(None);
            };
            match login_and_store_access_token_with_budget(database, data_key, station, remaining)?
            {
                Some(access_token) => access_token,
                None => return Ok(None),
            }
        }
    };

    let mut access_token = access_token;
    let mut auth_refresh_started = false;
    let mut auth_refresh = |_: &String, remaining: std::time::Duration| {
        if auth_refresh_started {
            return None;
        }
        auth_refresh_started = true;
        login_and_store_access_token_with_budget(database, data_key, station, remaining)
            .ok()
            .flatten()
    };
    for path in ["/api/v1/user/profile", "/api/v1/auth/me"] {
        let url = build_management_url(&station.website_url, path)?;
        let mut request = |token: &String, timeout: std::time::Duration| {
            fetch_recoverable_json_with_bearer(&url, token, timeout, proxy)
        };
        let execution = execute_json_request(
            path,
            access_token.clone(),
            Some(&mut auth_refresh),
            &mut request,
            policy,
            budget,
        );
        if let Some(refreshed) = execution.latest_credential.clone() {
            access_token = refreshed;
        }
        let payload = execution.result.payload.clone().unwrap_or(Value::Null);
        endpoint_results.push(execution.to_redacted_json());
        if execution.failure_kind.is_some() || !execution.result.ok {
            continue;
        }
        if let Some(balance) = parse_account_balance(&station.id, &payload, station.credit_per_cny)
        {
            return Ok(Some(balance));
        }
    }

    Ok(None)
}

#[derive(Debug, Clone, Copy)]
struct DashboardUsageStats {
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
    fn has_any(self) -> bool {
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

    fn apply_to(self, balance: &mut CollectedBalanceFact) {
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

fn collect_dashboard_usage_stats(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &crate::models::stations::Station,
    proxy: &ProxyConfig,
    endpoint_results: &mut Vec<Value>,
    budget: &CollectionAttemptBudget,
    policy: &RequestPolicy,
) -> Result<Option<DashboardUsageStats>, String> {
    let session = database.resolve_station_session_with_data_key(
        station.id.clone(),
        data_key,
        crate::services::database::now_millis_for_services() as i64,
    )?;
    let access_token = match session.access_token {
        Some(access_token) => access_token,
        None => {
            let Some(remaining) = budget.remaining() else {
                return Ok(None);
            };
            match login_and_store_access_token_with_budget(database, data_key, station, remaining)?
            {
                Some(access_token) => access_token,
                None => return Ok(None),
            }
        }
    };

    let path = "/api/v1/usage/dashboard/stats";
    let url = build_management_url(&station.website_url, path)?;
    let mut auth_refresh_started = false;
    let mut auth_refresh = |_: &String, remaining: std::time::Duration| {
        if auth_refresh_started {
            return None;
        }
        auth_refresh_started = true;
        login_and_store_access_token_with_budget(database, data_key, station, remaining)
            .ok()
            .flatten()
    };
    let mut request = |token: &String, timeout: std::time::Duration| {
        fetch_recoverable_json_with_bearer(&url, token, timeout, proxy)
    };
    let execution = execute_json_request(
        path,
        access_token,
        Some(&mut auth_refresh),
        &mut request,
        policy,
        budget,
    );
    let payload = execution.result.payload.clone().unwrap_or(Value::Null);
    endpoint_results.push(execution.to_redacted_json());
    if execution.failure_kind.is_some() || !execution.result.ok {
        return Ok(None);
    }

    Ok(parse_dashboard_usage_stats(&payload))
}

fn parse_dashboard_usage_stats(payload: &Value) -> Option<DashboardUsageStats> {
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

fn merge_dashboard_usage_stats(
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
    .unwrap_or("CNY")
    .to_string();
    let credit_unit = shared_balance_text_value(
        key_balances
            .iter()
            .map(|balance| balance.credit_unit.as_deref()),
    )
    .map(ToString::to_string);
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

fn parse_account_balance(
    station_id: &str,
    payload: &Value,
    credit_per_cny: f64,
) -> Option<CollectedBalanceFact> {
    let value = payload
        .pointer("/data/balance")
        .and_then(Value::as_f64)
        .or_else(|| payload.get("balance").and_then(Value::as_f64))
        .or_else(|| {
            payload
                .pointer("/data/quota/remaining")
                .and_then(Value::as_f64)
        })
        .or_else(|| payload.pointer("/quota/remaining").and_then(Value::as_f64))?;
    let used = payload
        .pointer("/data/used")
        .and_then(Value::as_f64)
        .or_else(|| payload.pointer("/data/quota/used").and_then(Value::as_f64))
        .or_else(|| payload.get("used").and_then(Value::as_f64));
    let total = payload
        .pointer("/data/total")
        .and_then(Value::as_f64)
        .or_else(|| payload.pointer("/data/quota/total").and_then(Value::as_f64))
        .or_else(|| payload.get("total").and_then(Value::as_f64));
    let currency = payload
        .pointer("/data/currency")
        .and_then(Value::as_str)
        .or_else(|| payload.get("currency").and_then(Value::as_str))
        .unwrap_or("CNY")
        .to_string();

    Some(CollectedBalanceFact {
        station_id: station_id.to_string(),
        station_key_id: None,
        scope: "station".to_string(),
        value: normalize_credit_value(Some(value), credit_per_cny),
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
        currency,
        credit_unit: None,
        status: if value == 0.0 { "depleted" } else { "normal" }.to_string(),
        source: "sub2api_account_profile".to_string(),
        confidence: 0.85,
        collected_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{
            credentials::UpdateStationCredentialsInput, station_keys::CreateStationKeyInput,
            stations::CreateStationInput,
        },
        services::{database::AppDatabase, secrets::crypto::generate_data_key},
    };
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc, Arc, Mutex,
        },
        thread,
        time::Duration,
    };

    #[test]
    fn sub2api_usage_parses_remaining_from_nested_quota() {
        let fact = parse_usage_balance(
            "station-1",
            Some("key-1".to_string()),
            &json!({
                "quota": {
                    "remaining": 42.5,
                    "used": 10.0,
                    "total": 52.5,
                    "unit": "CNY"
                },
                "planName": "pro"
            }),
            1.0,
        );

        assert_eq!(fact.station_id, "station-1");
        assert_eq!(fact.station_key_id.as_deref(), Some("key-1"));
        assert_eq!(fact.value, Some(42.5));
        assert_eq!(fact.used_value, Some(10.0));
        assert_eq!(fact.total_value, Some(52.5));
        assert_eq!(fact.source, "sub2api_usage");
    }

    #[test]
    fn sub2api_usage_marks_zero_balance_depleted() {
        let fact = parse_usage_balance("station-1", None, &json!({ "remaining": 0.0 }), 1.0);
        assert_eq!(fact.status, "depleted");
    }

    #[test]
    fn sub2api_usage_normalizes_credit_balance_to_cny() {
        let fact = parse_usage_balance(
            "station-1",
            Some("key-1".to_string()),
            &json!({
                "quota": {
                    "remaining": 100.0,
                    "used": 25.0,
                    "total": 125.0
                }
            }),
            10.0,
        );

        assert_eq!(fact.value, Some(10.0));
        assert_eq!(fact.used_value, Some(2.5));
        assert_eq!(fact.total_value, Some(12.5));
        assert_eq!(fact.currency, "CNY");
    }

    #[test]
    fn sub2api_usage_captures_request_cost_and_token_totals() {
        let fact = parse_usage_balance(
            "station-1",
            Some("key-1".to_string()),
            &json!({
                "quota": {
                    "remaining": 100.0,
                    "used": 25.0,
                    "total": 125.0
                },
                "today_request_count": 18,
                "request_count": 240,
                "today_used_amount": 0.75,
                "today_base_consumption": 1.5,
                "used_amount": 9.5,
                "total_base_consumption": 19.0,
                "today_tokens": 12345,
                "total_tokens": 67890
            }),
            10.0,
        );

        assert_eq!(fact.today_request_count, Some(18));
        assert_eq!(fact.total_request_count, Some(240));
        assert_eq!(fact.today_consumption, Some(0.75));
        assert_eq!(fact.total_consumption, Some(9.5));
        assert_eq!(fact.today_base_consumption, Some(1.5));
        assert_eq!(fact.total_base_consumption, Some(19.0));
        assert_eq!(fact.today_token_count, Some(12345));
        assert_eq!(fact.total_token_count, Some(67890));
    }

    #[test]
    fn sub2api_groups_rates_join_by_group_id() {
        let available = json!({
            "data": [
                { "id": "default", "name": "Default", "platform": "anthropic", "rate_multiplier": 1.0 },
                { "id": "pro", "name": "Pro", "platform": "openai", "rate_multiplier": 1.5 }
            ]
        });
        let rates = json!({
            "data": {
                "default": 0.8,
                "pro": 1.2
            }
        });

        let facts = parse_group_rate_facts("station-1", &available, &rates, 1.0);

        assert!(facts
            .groups
            .iter()
            .any(|group| group.group_name == "Default"));
        assert!(facts.rates.iter().any(|rate| {
            rate.group_name == "Pro" && rate.effective_rate_multiplier == Some(1.2)
        }));
        let pro_rate = facts
            .rates
            .iter()
            .find(|rate| rate.group_name == "Pro")
            .expect("pro rate");
        assert_eq!(
            pro_rate
                .raw_json_redacted
                .as_ref()
                .and_then(|value| value.get("platform"))
                .and_then(|value| value.as_str()),
            Some("openai")
        );
    }

    #[test]
    fn parses_sub2api_remote_key_payload() {
        let keys = parse_remote_key_payload(
            "station-1",
            &json!({
                "data": {
                    "items": [
                        {
                            "id": "remote-123",
                            "name": "Claude pool",
                            "key": "sk-live-secret-abcdef",
                            "group": "pro",
                            "rate_multiplier": 0.8,
                            "created_at": "2026-07-01T12:00:00Z",
                            "last_used_at": "2026-07-02T12:00:00Z"
                        }
                    ]
                }
            }),
        );

        assert_eq!(keys.len(), 1);
        let key = keys.first().expect("remote key");
        assert_eq!(key.station_id, "station-1");
        assert_eq!(key.remote_key_name.as_deref(), Some("Claude pool"));
        assert_eq!(key.api_key_masked.as_deref(), Some("sk-...cdef"));
        assert!(key.api_key_fingerprint.is_some());
        assert_eq!(key.group_name.as_deref(), Some("pro"));
        assert_eq!(key.rate_multiplier, Some(0.8));
        assert_eq!(key.rate_source.as_deref(), Some("sub2api_keys"));
        assert_eq!(key.created_at.as_deref(), Some("2026-07-01T12:00:00Z"));
        assert_eq!(key.last_used_at.as_deref(), Some("2026-07-02T12:00:00Z"));
        assert_eq!(key.raw_source, "sub2api_keys");
    }

    #[test]
    fn remote_key_rejects_masked_and_redacted_full_key_values() {
        for value in [
            "sk-live...cdef",
            "sk-live****cdef",
            "sk-live…cdef",
            "[REDACTED]",
            "<redacted>",
            "redacted",
            "masked",
            "sk-live-xxx-cdef",
            "sk-live-XXXX-cdef",
        ] {
            let keys = parse_remote_key_payload(
                "station-1",
                &json!({
                    "data": [{
                        "id": format!("remote-{value}"),
                        "name": "Masked payload",
                        "key": value
                    }]
                }),
            );

            assert_eq!(keys.len(), 1, "payload should still be discovered");
            assert_eq!(
                keys[0].api_key_fingerprint, None,
                "{value} must not be fingerprinted as a full key"
            );
            assert_eq!(
                full_key_from_create_payload(&json!({ "data": { "key": value } })),
                None,
                "{value} must not be returned as a create full key"
            );
        }

        assert_eq!(
            full_key_from_create_payload(&json!({
                "data": { "api_key": "sk-live-secret-abcdef" }
            })),
            Some("sk-live-secret-abcdef".to_string())
        );
    }

    #[test]
    fn remote_key_discovery_id_is_stable_for_strong_identities() {
        let by_remote_id_first = parse_remote_key_payload(
            "station-1",
            &json!([
                { "id": "remote-123", "name": "Primary" },
                { "id": "remote-other", "name": "Other" }
            ]),
        );
        let by_remote_id_second = parse_remote_key_payload(
            "station-1",
            &json!([
                { "id": "remote-other", "name": "Other" },
                { "id": "remote-123", "name": "Primary" }
            ]),
        );
        assert_eq!(by_remote_id_first[0].id, by_remote_id_second[1].id);
        let expected_remote_id_hash = sha256_hex("remote-123".as_bytes());
        assert_eq!(
            by_remote_id_first[0].remote_key_id_hash.as_deref(),
            Some(expected_remote_id_hash.as_str())
        );

        let by_full_key_first = parse_remote_key_payload(
            "station-1",
            &json!([
                { "name": "Key A", "key": "sk-live-secret-abcdef" },
                { "name": "Other", "key": "sk-live-secret-other" }
            ]),
        );
        let by_full_key_second = parse_remote_key_payload(
            "station-1",
            &json!([
                { "name": "Other", "key": "sk-live-secret-other" },
                { "name": "Key A", "key": "sk-live-secret-abcdef" }
            ]),
        );
        assert_eq!(by_full_key_first[0].id, by_full_key_second[1].id);

        let by_masked_first = parse_remote_key_payload(
            "station-1",
            &json!([
                { "name": "Masked A", "masked_key": "sk-...cdef" },
                { "name": "Other", "masked_key": "sk-...ffff" }
            ]),
        );
        let by_masked_second = parse_remote_key_payload(
            "station-1",
            &json!([
                { "name": "Other", "masked_key": "sk-...ffff" },
                { "name": "Masked A", "masked_key": "sk-...cdef" }
            ]),
        );
        assert_eq!(by_masked_first[0].id, by_masked_second[1].id);
        assert_eq!(by_masked_first[0].remote_key_id_hash, None);
    }

    #[test]
    fn remote_key_group_hash_distinguishes_id_from_name_fallback() {
        let with_group_id = parse_remote_key_payload(
            "station-1",
            &json!({
                "id": "remote-with-group-id",
                "name": "With group id",
                "group_id": "gid-pro",
                "group_name": "Pro"
            }),
        );
        let name_only = parse_remote_key_payload(
            "station-1",
            &json!({
                "id": "remote-with-group-name",
                "name": "With group name",
                "group": "Pro"
            }),
        );

        assert_eq!(with_group_id[0].group_name.as_deref(), Some("Pro"));
        assert_eq!(
            with_group_id[0].group_id_hash.as_deref(),
            Some(stable_group_key_hash("station-1", "sub2api", Some("gid-pro"), "Pro").as_str())
        );
        assert_eq!(name_only[0].group_name.as_deref(), Some("Pro"));
        assert_eq!(
            name_only[0].group_id_hash.as_deref(),
            Some(stable_group_key_hash("station-1", "sub2api", None, "Pro").as_str())
        );
        assert_ne!(with_group_id[0].group_id_hash, name_only[0].group_id_hash);
    }

    #[test]
    fn create_remote_key_posts_selected_group_id_from_binding() {
        let server = TestCreateKeyServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station(CreateStationInput {
                name: "create remote key station".to_string(),
                station_type: "sub2api".to_string(),
                website_url: server.base_url.clone(),
                api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-station".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        database
            .update_station_session_with_data_key(
                UpdateStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: Some("remote-key-token".to_string()),
                    refresh_token: None,
                    cookie: None,
                    newapi_user_id: None,
                    token_expires_at: None,
                },
                &data_key,
            )
            .expect("session");
        let group = database
            .upsert_station_group_binding(
                crate::models::group_facts::UpsertStationGroupBindingInput {
                    station_id: station.id.clone(),
                    station_key_id: None,
                    binding_kind: crate::models::group_facts::BINDING_KIND_STATION_GROUP
                        .to_string(),
                    parent_group_binding_id: None,
                    group_key_hash: "collector-sub2api-pro".to_string(),
                    group_id_hash: Some("pro".to_string()),
                    group_name: "Pro".to_string(),
                    binding_status: crate::models::group_facts::BINDING_STATUS_AVAILABLE
                        .to_string(),
                    default_rate_multiplier: Some(1.2),
                    user_rate_multiplier: None,
                    effective_rate_multiplier: Some(1.2),
                    rate_source: Some("sub2api_groups_rates".to_string()),
                    confidence: 0.95,
                    last_seen_at: Some("1000".to_string()),
                    inferred_group_category: Some("gpt".to_string()),
                    group_category_override: None,
                    raw_json_redacted: None,
                },
            )
            .expect("group");

        let result = create_remote_key(
            &database,
            &data_key,
            CreateRemoteStationKeyInput {
                station_id: station.id,
                name: "Grouped remote key".to_string(),
                group_binding_id: Some(group.id),
                group_id_hash: Some("collector-sub2api-pro".to_string()),
                group_name: Some("Pro".to_string()),
            },
        )
        .expect("create remote key");
        let request = server.request_body();

        assert_eq!(
            result.full_key_once.as_deref(),
            Some("sk-created-secret-pro")
        );
        assert_eq!(request["name"], "Grouped remote key");
        assert_eq!(request["group"], "Pro");
        assert_eq!(request["group_id"], "pro");
    }

    #[test]
    fn create_remote_key_posts_numeric_group_id_as_number() {
        let server = TestCreateKeyServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station(CreateStationInput {
                name: "create remote key numeric station".to_string(),
                station_type: "sub2api".to_string(),
                website_url: server.base_url.clone(),
                api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-station".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        database
            .update_station_session_with_data_key(
                UpdateStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: Some("remote-key-token".to_string()),
                    refresh_token: None,
                    cookie: None,
                    newapi_user_id: None,
                    token_expires_at: None,
                },
                &data_key,
            )
            .expect("session");
        let group = database
            .upsert_station_group_binding(
                crate::models::group_facts::UpsertStationGroupBindingInput {
                    station_id: station.id.clone(),
                    station_key_id: None,
                    binding_kind: crate::models::group_facts::BINDING_KIND_STATION_GROUP
                        .to_string(),
                    parent_group_binding_id: None,
                    group_key_hash: "collector-sub2api-plus".to_string(),
                    group_id_hash: Some("8".to_string()),
                    group_name: "Plus".to_string(),
                    binding_status: crate::models::group_facts::BINDING_STATUS_AVAILABLE
                        .to_string(),
                    default_rate_multiplier: Some(0.035),
                    user_rate_multiplier: None,
                    effective_rate_multiplier: Some(0.035),
                    rate_source: Some("sub2api_groups_rates".to_string()),
                    confidence: 0.95,
                    last_seen_at: Some("1000".to_string()),
                    inferred_group_category: Some("gpt".to_string()),
                    group_category_override: None,
                    raw_json_redacted: None,
                },
            )
            .expect("group");

        create_remote_key(
            &database,
            &data_key,
            CreateRemoteStationKeyInput {
                station_id: station.id,
                name: "Numeric grouped remote key".to_string(),
                group_binding_id: Some(group.id),
                group_id_hash: Some("collector-sub2api-plus".to_string()),
                group_name: Some("Plus".to_string()),
            },
        )
        .expect("create remote key");
        let request = server.request_body();

        assert_eq!(request["name"], "Numeric grouped remote key");
        assert_eq!(request["group"], "Plus");
        assert_eq!(request["group_id"], json!(8));
    }

    #[test]
    fn sub2api_group_rates_keep_raw_multipliers_before_pricing_conversion() {
        let available = json!({
            "data": [
                { "id": "default", "name": "Default", "rate_multiplier": 1.0 }
            ]
        });
        let rates = json!({
            "data": {
                "default": 1.0
            }
        });

        let facts = parse_group_rate_facts("station-1", &available, &rates, 10.0);
        let rate = facts.rates.first().expect("rate");

        assert_eq!(rate.default_rate_multiplier, Some(1.0));
        assert_eq!(rate.user_rate_multiplier, Some(1.0));
        assert_eq!(rate.effective_rate_multiplier, Some(1.0));
    }

    #[test]
    fn sub2api_group_rates_parse_group_name_lists_and_group_ratio_maps() {
        let available = json!({
            "data": {
                "groups": ["default", "vip"]
            }
        });
        let rates = json!({
            "data": {
                "group_ratio": {
                    "default": 1.0,
                    "vip": 1.5
                },
                "model_ratio": {
                    "gpt-4o-mini": 0.7
                }
            }
        });

        let facts = parse_group_rate_facts("station-1", &available, &rates, 1.0);

        assert_eq!(facts.groups.len(), 2);
        assert!(facts.rates.iter().any(|rate| {
            rate.group_name == "vip" && rate.effective_rate_multiplier == Some(1.5)
        }));
        assert!(!facts
            .rates
            .iter()
            .any(|rate| rate.group_name == "gpt-4o-mini"));
    }

    #[test]
    fn sub2api_groups_logs_in_with_saved_password_when_access_token_missing() {
        let server = TestGroupServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station(CreateStationInput {
                name: "group login station".to_string(),
                station_type: "sub2api".to_string(),
                website_url: server.base_url.clone(),
                api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-station".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        database
            .update_station_credentials_with_data_key(
                UpdateStationCredentialsInput {
                    station_id: station.id.clone(),
                    login_username: Some("user@example.test".to_string()),
                    login_password: Some("correct-password".to_string()),
                    remember_password: true,
                },
                &data_key,
            )
            .expect("credentials");

        let output = collect_groups(&database, &data_key, &station.id).expect("groups");
        let session = database
            .resolve_station_session_with_data_key(station.id, &data_key, 100000)
            .expect("session");

        assert_eq!(output.status, "success");
        assert_eq!(output.facts.groups.len(), 2);
        assert!(output.facts.rates.iter().any(|rate| {
            rate.group_name == "Pro" && rate.effective_rate_multiplier == Some(1.2)
        }));
        assert_eq!(
            session.access_token.as_deref(),
            Some("collector-token-secret")
        );
        assert_eq!(
            output.summary_json["sessionSource"],
            json!("password_login")
        );
    }

    #[test]
    fn sub2api_groups_relogs_in_when_saved_access_token_is_rejected() {
        let server = TestGroupServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station(CreateStationInput {
                name: "stale token station".to_string(),
                station_type: "sub2api".to_string(),
                website_url: server.base_url.clone(),
                api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-station".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        database
            .update_station_credentials_with_data_key(
                UpdateStationCredentialsInput {
                    station_id: station.id.clone(),
                    login_username: Some("user@example.test".to_string()),
                    login_password: Some("correct-password".to_string()),
                    remember_password: true,
                },
                &data_key,
            )
            .expect("credentials");
        database
            .update_station_session_with_data_key(
                UpdateStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: Some("stale-token".to_string()),
                    refresh_token: None,
                    cookie: None,
                    newapi_user_id: None,
                    token_expires_at: None,
                },
                &data_key,
            )
            .expect("stale session");

        let output = collect_groups(&database, &data_key, &station.id).expect("groups");
        let session = database
            .resolve_station_session_with_data_key(station.id, &data_key, 100000)
            .expect("session");

        assert_eq!(output.status, "success");
        assert_eq!(output.facts.groups.len(), 2);
        assert_eq!(
            session.access_token.as_deref(),
            Some("collector-token-secret")
        );
    }

    #[test]
    fn sub2api_groups_marks_reused_session_in_summary() {
        let server = TestGroupServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station(CreateStationInput {
                name: "warm session station".to_string(),
                station_type: "sub2api".to_string(),
                website_url: server.base_url.clone(),
                api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-station".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        database
            .update_station_session_with_data_key(
                UpdateStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: Some("collector-token-secret".to_string()),
                    refresh_token: None,
                    cookie: None,
                    newapi_user_id: None,
                    token_expires_at: None,
                },
                &data_key,
            )
            .expect("session");

        let output = collect_groups(&database, &data_key, &station.id).expect("groups");

        assert_eq!(output.status, "success");
        assert_eq!(
            output.summary_json["sessionSource"],
            json!("reused_session")
        );
    }

    #[test]
    fn sub2api_groups_retries_transient_rate_endpoint_failure() {
        let server = FlakyGroupServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station(CreateStationInput {
                name: "flaky group station".to_string(),
                station_type: "sub2api".to_string(),
                website_url: server.base_url.clone(),
                api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-station".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        database
            .update_station_credentials_with_data_key(
                UpdateStationCredentialsInput {
                    station_id: station.id.clone(),
                    login_username: Some("user@example.test".to_string()),
                    login_password: Some("correct-password".to_string()),
                    remember_password: true,
                },
                &data_key,
            )
            .expect("credentials");

        let output = collect_groups(&database, &data_key, &station.id).expect("groups");

        assert_eq!(output.status, "success");
        assert!(output.facts.rates.iter().any(|rate| {
            rate.group_name == "Pro" && rate.effective_rate_multiplier == Some(1.2)
        }));
        assert_eq!(server.rate_request_count(), 2);
    }

    #[test]
    fn sub2api_groups_record_auth_then_transient_recovery() {
        let server = AuthThenTransientGroupServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station(CreateStationInput {
                name: "auth then transient group station".to_string(),
                station_type: "sub2api".to_string(),
                website_url: server.base_url.clone(),
                api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-station".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        database
            .update_station_credentials_with_data_key(
                UpdateStationCredentialsInput {
                    station_id: station.id.clone(),
                    login_username: Some("user@example.test".to_string()),
                    login_password: Some("correct-password".to_string()),
                    remember_password: true,
                },
                &data_key,
            )
            .expect("credentials");
        database
            .update_station_session_with_data_key(
                UpdateStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: Some("stale-token".to_string()),
                    refresh_token: None,
                    cookie: None,
                    newapi_user_id: None,
                    token_expires_at: None,
                },
                &data_key,
            )
            .expect("stale session");

        let output = collect_groups(&database, &data_key, &station.id).expect("groups");
        let endpoint = &output.summary_json["endpointResults"][0];

        assert_eq!(output.status, "success");
        assert_eq!(endpoint["ok"], true);
        assert_eq!(
            endpoint["recoveryActions"],
            json!(["auth_refresh", "transient_retry"])
        );
        assert_eq!(endpoint["attemptCount"], 3);
    }

    #[test]
    fn sub2api_balance_falls_back_to_account_profile_when_usage_is_unauthorized() {
        let server = TestBalanceFallbackServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "balance fallback station".to_string(),
                    station_type: "sub2api".to_string(),
                    website_url: server.base_url.clone(),
                    api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                    collector_proxy_mode: "inherit".to_string(),
                    collector_proxy_url: None,
                    api_key: "sk-invalid-for-usage".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        database
            .update_station_session_with_data_key(
                UpdateStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: Some("profile-token-secret".to_string()),
                    refresh_token: None,
                    cookie: None,
                    newapi_user_id: None,
                    token_expires_at: None,
                },
                &data_key,
            )
            .expect("session");

        let output = collect_balance(&database, &data_key, &station.id).expect("balance");

        assert_eq!(output.status, "success");
        assert_eq!(output.facts.balances.len(), 1);
        assert_eq!(output.facts.balances[0].station_key_id, None);
        assert_eq!(output.facts.balances[0].scope, "station");
        assert_eq!(output.facts.balances[0].value, Some(5.12962411));
        assert_eq!(output.facts.balances[0].source, "sub2api_account_profile");
    }

    #[test]
    fn sub2api_balance_refreshes_rejected_account_token() {
        let server = RefreshingBalanceFallbackServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "refresh balance fallback station".to_string(),
                    station_type: "sub2api".to_string(),
                    website_url: server.base_url.clone(),
                    api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                    collector_proxy_mode: "inherit".to_string(),
                    collector_proxy_url: None,
                    api_key: "sk-invalid-for-usage".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        database
            .update_station_credentials_with_data_key(
                UpdateStationCredentialsInput {
                    station_id: station.id.clone(),
                    login_username: Some("user@example.test".to_string()),
                    login_password: Some("correct-password".to_string()),
                    remember_password: true,
                },
                &data_key,
            )
            .expect("credentials");
        database
            .update_station_session_with_data_key(
                UpdateStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: Some("stale-profile-token".to_string()),
                    refresh_token: None,
                    cookie: None,
                    newapi_user_id: None,
                    token_expires_at: None,
                },
                &data_key,
            )
            .expect("session");

        let output = collect_balance(&database, &data_key, &station.id).expect("balance");
        let session = database
            .resolve_station_session_with_data_key(station.id, &data_key, 100000)
            .expect("session");
        let account_endpoint = &output.summary_json["endpointResults"][1];

        assert_eq!(output.status, "success");
        assert_eq!(output.facts.balances.len(), 1);
        assert_eq!(output.facts.balances[0].value, Some(42.0));
        assert_eq!(session.access_token.as_deref(), Some("fresh-profile-token"));
        assert_eq!(account_endpoint["recoveryActions"], json!(["auth_refresh"]));
    }

    #[test]
    fn sub2api_balance_collects_dashboard_usage_stats_with_account_token() {
        let server = BalanceDashboardStatsServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "dashboard stats balance station".to_string(),
                    station_type: "sub2api".to_string(),
                    website_url: server.base_url.clone(),
                    api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                    collector_proxy_mode: "inherit".to_string(),
                    collector_proxy_url: None,
                    api_key: "sk-dashboard-balance".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        database
            .update_station_session_with_data_key(
                UpdateStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: Some("dashboard-token-secret".to_string()),
                    refresh_token: None,
                    cookie: None,
                    newapi_user_id: None,
                    token_expires_at: None,
                },
                &data_key,
            )
            .expect("session");

        let output = collect_balance(&database, &data_key, &station.id).expect("balance");
        let station_balance = output
            .facts
            .balances
            .iter()
            .find(|balance| balance.scope == "station")
            .expect("station usage balance");

        assert_eq!(output.status, "success");
        assert_eq!(station_balance.value, Some(100.0));
        assert_eq!(station_balance.today_request_count, Some(12));
        assert_eq!(station_balance.total_request_count, Some(1200));
        assert_eq!(station_balance.today_consumption, Some(0.75));
        assert_eq!(station_balance.total_consumption, Some(18.5));
        assert_eq!(station_balance.today_token_count, Some(34567));
        assert_eq!(station_balance.total_token_count, Some(4567890));
        assert_eq!(station_balance.today_input_token_count, Some(30000));
        assert_eq!(station_balance.today_output_token_count, Some(4567));
        assert_eq!(station_balance.total_input_token_count, Some(4300000));
        assert_eq!(station_balance.total_output_token_count, Some(267890));
        assert!(output.summary_json["endpointResults"]
            .as_array()
            .expect("endpoint results")
            .iter()
            .any(|endpoint| endpoint["path"] == json!("/api/v1/usage/dashboard/stats")));
    }

    #[test]
    fn sub2api_balance_attempts_all_keys_before_transient_retries() {
        let server = FairBalanceServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "fair balance station".to_string(),
                    station_type: "sub2api".to_string(),
                    website_url: server.base_url.clone(),
                    api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                    collector_proxy_mode: "inherit".to_string(),
                    collector_proxy_url: None,
                    api_key: "sk-fallback".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        for (name, api_key) in [("key-a", "sk-key-a"), ("key-b", "sk-key-b")] {
            database
                .create_station_key_with_data_key(
                    CreateStationKeyInput {
                        station_id: station.id.clone(),
                        name: name.to_string(),
                        api_key: api_key.to_string(),
                        enabled: true,
                        priority: None,
                        max_concurrency: None,
                        load_factor: None,
                        schedulable: None,
                        group_name: None,
                        tier_label: None,
                        group_binding_id: None,
                        group_id_hash: None,
                        rate_multiplier: None,
                        manual_rate_multiplier: None,
                        rate_source: None,
                        balance_scope: None,
                        note: None,
                    },
                    &data_key,
                )
                .expect("station key");
        }

        let output = collect_balance(&database, &data_key, &station.id).expect("balance");

        assert_eq!(output.status, "success");
        assert_eq!(server.request_order(), vec!["A", "B", "A"]);
        let routeable_key_count = routeable_keys_for_station(&database, &station.id)
            .expect("routeable keys")
            .len();
        let endpoint_results = output.summary_json["endpointResults"]
            .as_array()
            .expect("endpoint results");
        assert_eq!(
            endpoint_results.len(),
            routeable_key_count,
            "transient retries should update the original key endpoint result instead of adding top-level rows"
        );
        let retried_endpoint = endpoint_results
            .iter()
            .find(|endpoint| endpoint["recoveryActions"] == json!(["transient_retry"]))
            .expect("retried endpoint");
        assert_eq!(retried_endpoint["attemptCount"], json!(2));
        assert_eq!(
            retried_endpoint["attempts"].as_array().map(Vec::len),
            Some(2)
        );
    }

    struct TestGroupServer {
        base_url: String,
    }

    struct FlakyGroupServer {
        base_url: String,
        rate_requests: Arc<AtomicUsize>,
    }

    struct AuthThenTransientGroupServer {
        base_url: String,
    }

    struct TestCreateKeyServer {
        base_url: String,
        request_rx: mpsc::Receiver<Value>,
    }

    struct TestBalanceFallbackServer {
        base_url: String,
    }

    struct RefreshingBalanceFallbackServer {
        base_url: String,
    }

    struct BalanceDashboardStatsServer {
        base_url: String,
    }

    struct FairBalanceServer {
        base_url: String,
        request_order: Arc<Mutex<Vec<String>>>,
    }

    impl TestGroupServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            thread::spawn(move || {
                for stream in listener.incoming().take(5).flatten() {
                    handle_group_test_request(stream);
                }
            });
            Self { base_url }
        }
    }

    impl FlakyGroupServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            let rate_requests = Arc::new(AtomicUsize::new(0));
            let handler_rate_requests = Arc::clone(&rate_requests);
            thread::spawn(move || {
                for stream in listener.incoming().take(6).flatten() {
                    handle_flaky_group_test_request(stream, Arc::clone(&handler_rate_requests));
                }
            });
            Self {
                base_url,
                rate_requests,
            }
        }

        fn rate_request_count(&self) -> usize {
            self.rate_requests.load(Ordering::Relaxed)
        }
    }

    impl AuthThenTransientGroupServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            let available_requests = Arc::new(AtomicUsize::new(0));
            let handler_available_requests = Arc::clone(&available_requests);
            thread::spawn(move || {
                for stream in listener.incoming().take(6).flatten() {
                    handle_auth_then_transient_group_request(
                        stream,
                        Arc::clone(&handler_available_requests),
                    );
                }
            });
            Self { base_url }
        }
    }

    impl TestCreateKeyServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            let (request_tx, request_rx) = mpsc::channel();
            thread::spawn(move || {
                if let Some(stream) = listener.incoming().take(1).flatten().next() {
                    handle_create_key_test_request(stream, request_tx);
                }
            });
            Self {
                base_url,
                request_rx,
            }
        }

        fn request_body(&self) -> Value {
            self.request_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("create request body")
        }
    }

    impl TestBalanceFallbackServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            thread::spawn(move || {
                for stream in listener.incoming().take(2).flatten() {
                    handle_balance_fallback_request(stream);
                }
            });
            Self { base_url }
        }
    }

    impl RefreshingBalanceFallbackServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            thread::spawn(move || {
                for stream in listener.incoming().take(4).flatten() {
                    handle_refreshing_balance_fallback_request(stream);
                }
            });
            Self { base_url }
        }
    }

    impl BalanceDashboardStatsServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            thread::spawn(move || {
                for stream in listener.incoming().take(2).flatten() {
                    handle_balance_dashboard_stats_request(stream);
                }
            });
            Self { base_url }
        }
    }

    impl FairBalanceServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            let request_order = Arc::new(Mutex::new(Vec::new()));
            let handler_request_order = Arc::clone(&request_order);
            thread::spawn(move || {
                for stream in listener.incoming().take(4).flatten() {
                    handle_fair_balance_request(stream, Arc::clone(&handler_request_order));
                }
            });
            Self {
                base_url,
                request_order,
            }
        }

        fn request_order(&self) -> Vec<&'static str> {
            self.request_order
                .lock()
                .map(|items| {
                    items
                        .iter()
                        .map(|item| if item == "sk-key-a" { "A" } else { "B" })
                        .collect()
                })
                .unwrap_or_default()
        }
    }

    fn handle_create_key_test_request(mut stream: TcpStream, request_tx: mpsc::Sender<Value>) {
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
        let authorized = request
            .to_lowercase()
            .contains("authorization: bearer remote-key-token");

        let (status, response) = if path == "/api/v1/keys" && authorized {
            let parsed = serde_json::from_str::<Value>(body).expect("create key json");
            request_tx.send(parsed).expect("send request body");
            (
                "200 OK",
                json!({
                    "data": {
                        "id": "created-remote-pro",
                        "name": "Grouped remote key",
                        "key": "sk-created-secret-pro",
                        "group_id": "pro",
                        "group_name": "Pro"
                    }
                }),
            )
        } else {
            ("401 Unauthorized", json!({ "message": "unauthorized" }))
        };
        let text = response.to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn handle_group_test_request(mut stream: TcpStream) {
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
        let authorized = request
            .to_lowercase()
            .contains("authorization: bearer collector-token-secret");

        let (status, response) = match path {
            "/api/v1/auth/login" => {
                let parsed = serde_json::from_str::<Value>(body).expect("login json");
                if parsed.get("password").and_then(Value::as_str) == Some("correct-password") {
                    (
                        "200 OK",
                        json!({ "data": { "access_token": "collector-token-secret" } }),
                    )
                } else {
                    (
                        "401 Unauthorized",
                        json!({ "message": "invalid credentials" }),
                    )
                }
            }
            "/api/v1/groups/available" if authorized => (
                "200 OK",
                json!({
                    "data": [
                        { "id": "default", "name": "Default", "rate_multiplier": 1.0 },
                        { "id": "pro", "name": "Pro", "rate_multiplier": 1.5 }
                    ]
                }),
            ),
            "/api/v1/groups/rates" if authorized => {
                ("200 OK", json!({ "data": { "default": 0.8, "pro": 1.2 } }))
            }
            _ => ("401 Unauthorized", json!({ "message": "unauthorized" })),
        };
        let text = response.to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn handle_flaky_group_test_request(mut stream: TcpStream, rate_requests: Arc<AtomicUsize>) {
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
        let authorized = request
            .to_lowercase()
            .contains("authorization: bearer flaky-collector-token");

        let (status, response) = match path {
            "/api/v1/auth/login" => {
                let parsed = serde_json::from_str::<Value>(body).expect("login json");
                if parsed.get("password").and_then(Value::as_str) == Some("correct-password") {
                    (
                        "200 OK",
                        json!({ "data": { "access_token": "flaky-collector-token" } }),
                    )
                } else {
                    (
                        "401 Unauthorized",
                        json!({ "message": "invalid credentials" }),
                    )
                }
            }
            "/api/v1/groups/available" if authorized => (
                "200 OK",
                json!({
                    "data": [
                        { "id": "default", "name": "Default" },
                        { "id": "pro", "name": "Pro" }
                    ]
                }),
            ),
            "/api/v1/groups/rates" if authorized => {
                let request_index = rate_requests.fetch_add(1, Ordering::Relaxed);
                if request_index == 0 {
                    (
                        "502 Bad Gateway",
                        json!({ "message": "temporary upstream failure" }),
                    )
                } else {
                    ("200 OK", json!({ "data": { "default": 0.8, "pro": 1.2 } }))
                }
            }
            _ => ("401 Unauthorized", json!({ "message": "unauthorized" })),
        };
        let text = response.to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn handle_auth_then_transient_group_request(
        mut stream: TcpStream,
        available_requests: Arc<AtomicUsize>,
    ) {
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
        let fresh_authorized = request
            .to_lowercase()
            .contains("authorization: bearer fresh-group-token");

        let (status, response) = match path {
            "/api/v1/auth/login" => {
                let parsed = serde_json::from_str::<Value>(body).expect("login json");
                if parsed.get("password").and_then(Value::as_str) == Some("correct-password") {
                    (
                        "200 OK",
                        json!({ "data": { "access_token": "fresh-group-token" } }),
                    )
                } else {
                    (
                        "401 Unauthorized",
                        json!({ "message": "invalid credentials" }),
                    )
                }
            }
            "/api/v1/groups/available" if fresh_authorized => {
                let request_index = available_requests.fetch_add(1, Ordering::Relaxed);
                if request_index == 0 {
                    (
                        "502 Bad Gateway",
                        json!({ "message": "temporary upstream failure" }),
                    )
                } else {
                    (
                        "200 OK",
                        json!({
                            "data": [
                                { "id": "default", "name": "Default", "rate_multiplier": 1.0 },
                                { "id": "pro", "name": "Pro", "rate_multiplier": 1.5 }
                            ]
                        }),
                    )
                }
            }
            "/api/v1/groups/rates" if fresh_authorized => {
                ("200 OK", json!({ "data": { "default": 0.8, "pro": 1.2 } }))
            }
            _ => ("401 Unauthorized", json!({ "message": "unauthorized" })),
        };
        let text = response.to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn handle_balance_fallback_request(mut stream: TcpStream) {
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let authorized = request
            .to_lowercase()
            .contains("authorization: bearer profile-token-secret");

        let (status, response) = match path {
            "/v1/usage" => ("401 Unauthorized", json!({ "message": "unauthorized" })),
            "/api/v1/user/profile" if authorized => (
                "200 OK",
                json!({
                    "data": {
                        "balance": 5.12962411
                    }
                }),
            ),
            _ => ("404 Not Found", json!({ "message": "not found" })),
        };
        let text = response.to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn handle_refreshing_balance_fallback_request(mut stream: TcpStream) {
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
        let fresh_authorized = request
            .to_lowercase()
            .contains("authorization: bearer fresh-profile-token");

        let (status, response) = match path {
            "/v1/usage" => ("401 Unauthorized", json!({ "message": "unauthorized" })),
            "/api/v1/auth/login" => {
                let parsed = serde_json::from_str::<Value>(body).expect("login json");
                if parsed.get("password").and_then(Value::as_str) == Some("correct-password") {
                    (
                        "200 OK",
                        json!({ "data": { "access_token": "fresh-profile-token" } }),
                    )
                } else {
                    (
                        "401 Unauthorized",
                        json!({ "message": "invalid credentials" }),
                    )
                }
            }
            "/api/v1/user/profile" if fresh_authorized => (
                "200 OK",
                json!({
                    "data": {
                        "balance": 42.0
                    }
                }),
            ),
            _ => ("401 Unauthorized", json!({ "message": "unauthorized" })),
        };
        let text = response.to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn handle_balance_dashboard_stats_request(mut stream: TcpStream) {
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let lower = request.to_lowercase();
        let key_authorized = lower.contains("authorization: bearer sk-dashboard-balance");
        let dashboard_authorized = lower.contains("authorization: bearer dashboard-token-secret");

        let (status, response) = match path {
            "/v1/usage" if key_authorized => (
                "200 OK",
                json!({
                    "remaining": 100.0
                }),
            ),
            "/api/v1/usage/dashboard/stats" if dashboard_authorized => (
                "200 OK",
                json!({
                    "data": {
                        "today_requests": 12,
                        "total_requests": 1200,
                        "today_actual_cost": 0.75,
                        "total_actual_cost": 18.5,
                        "today_tokens": 34567,
                        "total_tokens": 4567890,
                        "today_prompt_tokens": 30000,
                        "today_completion_tokens": 4567,
                        "prompt_tokens": 4300000,
                        "completion_tokens": 267890
                    }
                }),
            ),
            _ => ("401 Unauthorized", json!({ "message": "unauthorized" })),
        };
        let text = response.to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn handle_fair_balance_request(mut stream: TcpStream, request_order: Arc<Mutex<Vec<String>>>) {
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let lower = request.to_lowercase();
        let api_key = if lower.contains("authorization: bearer sk-key-a") {
            Some("sk-key-a")
        } else if lower.contains("authorization: bearer sk-key-b") {
            Some("sk-key-b")
        } else {
            None
        };
        if path == "/v1/usage" {
            if let Some(api_key) = api_key {
                if let Ok(mut order) = request_order.lock() {
                    order.push(api_key.to_string());
                }
            }
        }
        let attempts_for_key = api_key
            .and_then(|key| {
                request_order
                    .lock()
                    .ok()
                    .map(|order| order.iter().filter(|item| item.as_str() == key).count())
            })
            .unwrap_or(0);

        let (status, response) = match (path, api_key, attempts_for_key) {
            ("/v1/usage", Some("sk-key-a"), 1) => (
                "502 Bad Gateway",
                json!({ "message": "temporary upstream failure" }),
            ),
            ("/v1/usage", Some("sk-key-a"), _) => ("200 OK", json!({ "remaining": 11.0 })),
            ("/v1/usage", Some("sk-key-b"), _) => ("200 OK", json!({ "remaining": 22.0 })),
            _ => ("401 Unauthorized", json!({ "message": "unauthorized" })),
        };
        let text = response.to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 1024];
        let mut header_end = None;
        let mut content_length = 0_usize;

        loop {
            let read = stream.read(&mut buffer).expect("read request");
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            if header_end.is_none() {
                if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    header_end = Some(position + 4);
                    let headers = String::from_utf8_lossy(&bytes[..position]);
                    content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            if name.eq_ignore_ascii_case("content-length") {
                                value.trim().parse::<usize>().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                }
            }
            if let Some(header_end) = header_end {
                if bytes.len() >= header_end + content_length {
                    break;
                }
            }
        }

        String::from_utf8_lossy(&bytes).to_string()
    }
}
