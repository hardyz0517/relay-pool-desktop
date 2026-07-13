mod auth;
mod client;
mod parsers;
#[cfg(test)]
mod test_support;

use serde_json::{json, Value};

use crate::models::{
    remote_keys::{
        CreateRemoteStationKeyInput, RemoteKeyCapability, RemoteKeyMatchStatus, RemoteStationKey,
    },
    stations::Station,
};
use crate::services::{
    collectors::{
        adapters::{AdapterOutput, CollectorTask, CreatedRemoteKey},
        facts::{CollectedBalanceFact, CollectorFacts},
        url::join_url,
    },
    database::AppDatabase,
    outbound::{agent_builder_for_proxy, resolve_proxy_config, ProxyConfig},
};

const COLLECTOR_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const NEWAPI_REMOTE_KEY_PAGE_SIZE: usize = 100;
const NEWAPI_LOG_PAGE_SIZE: usize = 100;
const NEWAPI_LOG_MAX_PAGES: usize = 100;
const NEWAPI_LOG_TYPE_CONSUME: i64 = 2;

pub(crate) fn login_with_password(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
    login_username: &str,
    login_password: &str,
) -> Result<auth::NewApiLoginProbeOutcome, String> {
    auth::login_with_password(database, data_key, station, login_username, login_password)
}

pub(crate) fn test_login_credentials(
    base_url: &str,
    login_username: &str,
    login_password: &str,
) -> Result<auth::NewApiLoginProbeOutcome, String> {
    auth::test_login_credentials(base_url, login_username, login_password)
}

fn parse_newapi_balance(station_id: &str, payload: &Value) -> CollectedBalanceFact {
    parsers::parse_balance_fact(station_id, payload, 500000.0, true)
}

fn parse_newapi_group_facts(station_id: &str, payload: &Value) -> CollectorFacts {
    parsers::parse_group_facts(station_id, payload)
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
    let tokens = fetch_newapi_token_items(database, data_key, &station)?;
    Ok(parse_remote_key_items(&station.id, &tokens))
}

pub fn scan_remote_key_full_secret(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    remote_key_id: &str,
) -> Result<(RemoteStationKey, String), String> {
    let station = database.station_for_collector(station_id)?;
    let tokens = fetch_newapi_token_items(database, data_key, &station)?;
    for (index, value) in tokens.iter().enumerate() {
        let Some(remote_key) = remote_key_from_value(&station.id, value, index) else {
            continue;
        };
        if remote_key.id != remote_key_id {
            continue;
        }
        return reveal_full_key_for_token_value(database, data_key, &station, value, index);
    }

    Err("远端 Key 已不存在，无法创建本地 Key。".to_string())
}

pub fn create_remote_key(
    database: &AppDatabase,
    data_key: &[u8; 32],
    input: CreateRemoteStationKeyInput,
) -> Result<CreatedRemoteKey, String> {
    let station = database.station_for_collector(&input.station_id)?;
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
    client::post_authenticated_json(
        database,
        data_key,
        &station,
        "/api/token/",
        client::NewApiOperation::CreateToken,
        body,
    )
    .map_err(newapi_request_error_message)?;

    let tokens = fetch_newapi_token_items(database, data_key, &station)?;
    for (index, value) in tokens.iter().enumerate() {
        if !created_token_matches(value, &input.name) {
            continue;
        }
        let (remote_key, full_key_once) =
            reveal_full_key_for_token_value(database, data_key, &station, value, index)?;
        return Ok(CreatedRemoteKey {
            remote_key,
            full_key_once: Some(full_key_once),
            message: "NewAPI 远端 Key 已创建。".to_string(),
        });
    }

    Err(
        "NewAPI 远端 Key 已创建，但未能在列表中对账找到新 Key。请扫描远端 Key 后手动同步。"
            .to_string(),
    )
}

fn fetch_newapi_token_items(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
) -> Result<Vec<Value>, String> {
    let mut page = 1_usize;
    let mut items = Vec::new();
    loop {
        let response = client::get_authenticated_json(
            database,
            data_key,
            station,
            &format!("/api/token/?p={page}&page_size={NEWAPI_REMOTE_KEY_PAGE_SIZE}"),
            client::NewApiOperation::ListTokens,
        )
        .map_err(newapi_request_error_message)?;
        let page_items = remote_key_items(&response.data)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let page_size =
            page_size_from_payload(&response.data).unwrap_or(NEWAPI_REMOTE_KEY_PAGE_SIZE);
        let total = total_from_payload(&response.data);
        let page_item_count = page_items.len();
        items.extend(page_items);
        if total.is_some_and(|total| items.len() >= total) {
            break;
        }
        if page_item_count == 0 || page_item_count < page_size {
            break;
        }
        page += 1;
        if page > 1000 {
            return Err("NewAPI 远端 Key 分页超过安全上限，已停止扫描。".to_string());
        }
    }
    Ok(items)
}

fn parse_remote_key_items(station_id: &str, items: &[Value]) -> Vec<RemoteStationKey> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, value)| remote_key_from_value(station_id, value, index))
        .collect()
}

fn created_token_matches(value: &Value, expected_name: &str) -> bool {
    let expected_name = expected_name.trim();
    string_field(value, &["name", "key_name", "keyName", "label", "remark"])
        .as_deref()
        .map(str::trim)
        .is_some_and(|name| !expected_name.is_empty() && name == expected_name)
}

fn reveal_full_key_for_token_value(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
    value: &Value,
    index: usize,
) -> Result<(RemoteStationKey, String), String> {
    let mut remote_key = remote_key_from_value(&station.id, value, index)
        .ok_or_else(|| "NewAPI 远端 Key 响应缺少可识别身份。".to_string())?;
    let token_id = string_field(value, &["id", "token_id", "tokenId"])
        .ok_or_else(|| "NewAPI 远端 Key 缺少 token id，无法读取完整 Key。".to_string())?;
    let response = client::post_authenticated_json(
        database,
        data_key,
        station,
        &format!("/api/token/{token_id}/key"),
        client::NewApiOperation::RevealToken,
        json!({}),
    )
    .map_err(newapi_request_error_message)?;
    let full_key = full_key_from_reveal_payload(&response.data).ok_or_else(|| {
        "NewAPI reveal 响应没有返回完整 Key，无法自动保存到本地。请到网站复制后手动补全。"
            .to_string()
    })?;
    remote_key.api_key_masked = Some(crate::services::secrets::mask::mask_secret(&full_key));
    remote_key.api_key_fingerprint = crate::services::remote_keys::api_key_fingerprint(&full_key);
    Ok((remote_key, full_key))
}

fn remote_key_items(payload: &Value) -> Vec<&Value> {
    if let Some(items) = payload.as_array() {
        return items.iter().collect();
    }
    for pointer in [
        "/items",
        "/list",
        "/tokens",
        "/keys",
        "/data/items",
        "/data/list",
        "/data/tokens",
        "/data/keys",
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
    let remote_key_id = string_field(value, &["id", "token_id", "tokenId"]);
    let name = string_field(value, &["name", "key_name", "keyName", "label", "remark"]);
    let key_value = string_field(value, &["key", "api_key", "apiKey", "token"]);
    let full_key = key_value
        .as_deref()
        .filter(|value| looks_like_full_api_key(value))
        .map(ToString::to_string);
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
    })
    .or_else(|| key_value.filter(|value| !looks_like_full_api_key(value)));
    let (identity_kind, identity, include_index) = remote_key_identity(
        remote_key_id.as_deref(),
        full_key.as_deref(),
        masked.as_deref(),
        name.as_deref(),
    )?;
    let group_name = string_field(value, &["group", "group_name", "groupName", "group_label"]);
    let group_id_hash = group_name
        .as_deref()
        .map(|group| stable_group_key_hash(station_id, "newapi", Some(group), group));
    let identity_seed = if include_index {
        format!("{station_id}:{identity_kind}:{identity}:{index}")
    } else {
        format!("{station_id}:{identity_kind}:{identity}")
    };

    Some(RemoteStationKey {
        id: format!(
            "newapi-remote-key-{}",
            &sha256_hex(identity_seed.as_bytes())[..16]
        ),
        station_id: station_id.to_string(),
        remote_key_id_hash: remote_key_id
            .as_deref()
            .map(|value| sha256_hex(value.as_bytes())),
        remote_key_name: name,
        api_key_masked: masked,
        api_key_fingerprint: full_key
            .as_deref()
            .and_then(crate::services::remote_keys::api_key_fingerprint),
        group_id_hash,
        group_name,
        tier_label: None,
        rate_multiplier: None,
        rate_source: Some("newapi_tokens".to_string()),
        created_at: string_field(
            value,
            &["created_at", "createdAt", "created", "created_time"],
        ),
        last_used_at: string_field(
            value,
            &["last_used_at", "lastUsedAt", "last_used", "accessed_time"],
        ),
        raw_source: "newapi_tokens".to_string(),
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

fn full_key_from_reveal_payload(payload: &Value) -> Option<String> {
    string_field(payload, &["key", "api_key", "apiKey", "token"])
        .filter(|value| looks_like_full_api_key(value))
        .or_else(|| {
            payload
                .pointer("/data/key")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| looks_like_full_api_key(value))
                .map(ToString::to_string)
        })
}

fn page_size_from_payload(payload: &Value) -> Option<usize> {
    payload
        .get("page_size")
        .or_else(|| payload.get("pageSize"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
}

fn total_from_payload(payload: &Value) -> Option<usize> {
    payload
        .get("total")
        .or_else(|| payload.get("count"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
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

pub fn collect(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    task: CollectorTask,
) -> Result<AdapterOutput, String> {
    match task {
        CollectorTask::Detect => Ok(AdapterOutput {
            adapter: "newapi".to_string(),
            task,
            status: "success".to_string(),
            summary_json: json!({
                "adapter": "newapi",
                "task": "detect",
                "message": "NewAPI adapter 已确认。",
                "endpointResults": [],
            }),
            normalized_json: json!({
                "adapter": "newapi",
                "models": [],
            }),
            raw_json_redacted: None,
            error_code: None,
            error_message: None,
            facts: CollectorFacts::default(),
        }),
        CollectorTask::Balance | CollectorTask::Groups | CollectorTask::Models => {
            collect_balance_and_groups(database, data_key, station_id, task)
        }
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
        adapter: "newapi".to_string(),
        task,
        status: "manual_required".to_string(),
        summary_json: json!({
            "adapter": "newapi",
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

fn build_balance_output(
    station_id: &str,
    data: &Value,
    status: &parsers::NewApiStatus,
    endpoint_results: Vec<Value>,
) -> AdapterOutput {
    let mut facts = CollectorFacts::default();
    facts.balances.push(parsers::parse_balance_fact(
        station_id,
        data,
        status.quota_per_unit,
        status.used_fallback,
    ));
    let balance_count = facts.balances.len();
    AdapterOutput {
        adapter: "newapi".to_string(),
        task: CollectorTask::Balance,
        status: if balance_count == 0 {
            "partial"
        } else {
            "success"
        }
        .to_string(),
        summary_json: json!({
            "adapter": "newapi",
            "task": "balance",
            "quotaPerUnit": status.quota_per_unit,
            "quotaPerUnitFallback": status.used_fallback,
            "endpointResults": endpoint_results,
        }),
        normalized_json: json!({
            "balanceCount": balance_count,
            "models": [],
        }),
        raw_json_redacted: None,
        error_code: (balance_count == 0).then(|| "empty_balance_facts".to_string()),
        error_message: (balance_count == 0)
            .then(|| "NewAPI balance response did not contain quota facts".to_string()),
        facts,
    }
}

fn build_groups_output(station_id: &str, data: &Value, endpoint_result: Value) -> AdapterOutput {
    let facts = parsers::parse_group_facts(station_id, data);
    let group_count = facts.groups.len();
    let rate_count = facts.rates.len();
    AdapterOutput {
        adapter: "newapi".to_string(),
        task: CollectorTask::Groups,
        status: if group_count == 0 {
            "partial"
        } else {
            "success"
        }
        .to_string(),
        summary_json: json!({
            "adapter": "newapi",
            "task": "groups",
            "endpointResults": [endpoint_result],
        }),
        normalized_json: json!({
            "groupCount": group_count,
            "rateCount": rate_count,
            "groups": facts.groups.iter().map(|group| json!({
                "groupId": group.group_id,
                "groupIdHash": group.group_key_hash,
                "groupName": group.group_name,
            })).collect::<Vec<_>>(),
            "rateMultipliers": facts.rates.iter().map(|rate| json!({
                "groupId": rate.group_id,
                "groupIdHash": rate.group_key_hash,
                "groupName": rate.group_name,
                "effectiveRateMultiplier": rate.effective_rate_multiplier,
            })).collect::<Vec<_>>(),
            "models": [],
        }),
        raw_json_redacted: None,
        error_code: (group_count == 0).then(|| "empty_group_facts".to_string()),
        error_message: (group_count == 0)
            .then(|| "NewAPI groups response did not contain group facts".to_string()),
        facts,
    }
}

fn build_models_output(station_id: &str, data: &Value, endpoint_result: Value) -> AdapterOutput {
    let mut facts = CollectorFacts::default();
    facts.models = parsers::parse_models(station_id, data);
    let model_names = facts
        .models
        .iter()
        .map(|model| model.model.clone())
        .collect::<Vec<_>>();
    AdapterOutput {
        adapter: "newapi".to_string(),
        task: CollectorTask::Models,
        status: if model_names.is_empty() {
            "partial"
        } else {
            "success"
        }
        .to_string(),
        summary_json: json!({
            "adapter": "newapi",
            "task": "models",
            "endpointResults": [endpoint_result],
        }),
        normalized_json: json!({
            "models": model_names,
        }),
        raw_json_redacted: None,
        error_code: facts
            .models
            .is_empty()
            .then(|| "empty_model_facts".to_string()),
        error_message: facts
            .models
            .is_empty()
            .then(|| "NewAPI models response did not contain model facts".to_string()),
        facts,
    }
}

pub fn collect_balance_and_groups(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    task: CollectorTask,
) -> Result<AdapterOutput, String> {
    collect_authenticated_task(database, data_key, station_id, task)
}

fn collect_authenticated_task(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    task: CollectorTask,
) -> Result<AdapterOutput, String> {
    let station = database.station_for_collector(station_id)?;
    if task == CollectorTask::Balance {
        let (status, status_endpoint_result) = fetch_status(database, &station)?;
        let response = client::get_authenticated_json(
            database,
            data_key,
            &station,
            "/api/user/self",
            client::NewApiOperation::SelfInfo,
        )
        .map_err(newapi_request_error_message)?;
        let (usage_stats, usage_endpoint_results) =
            collect_usage_stats(database, data_key, &station, status.quota_per_unit);
        let mut balance_data = response.data;
        if let Some(usage_stats) = usage_stats {
            merge_usage_stats_into_balance_data(&mut balance_data, usage_stats);
        }
        let mut endpoint_results = vec![status_endpoint_result, response.endpoint_result];
        endpoint_results.extend(usage_endpoint_results);
        return Ok(build_balance_output(
            station_id,
            &balance_data,
            &status,
            endpoint_results,
        ));
    }
    if task == CollectorTask::Groups {
        let response = client::get_authenticated_json(
            database,
            data_key,
            &station,
            "/api/user/self/groups",
            client::NewApiOperation::Groups,
        )
        .map_err(newapi_request_error_message)?;
        return Ok(build_groups_output(
            station_id,
            &response.data,
            response.endpoint_result,
        ));
    }
    if task == CollectorTask::Models {
        let response = client::get_authenticated_json(
            database,
            data_key,
            &station,
            "/api/user/models",
            client::NewApiOperation::Models,
        )
        .map_err(newapi_request_error_message)?;
        return Ok(build_models_output(
            station_id,
            &response.data,
            response.endpoint_result,
        ));
    }
    unsupported_output(
        task,
        "unsupported_task",
        "NewAPI adapter does not run this task directly.",
    )
}

#[derive(Debug, Clone, Default)]
struct NewApiUsageStats {
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

#[derive(Debug, Clone, Default)]
struct NewApiLogStatWindow {
    request_count: Option<i64>,
    token_count: Option<i64>,
    consumption: Option<f64>,
    base_consumption: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct NewApiLogWindow {
    request_count: Option<i64>,
    input_token_count: Option<i64>,
    output_token_count: Option<i64>,
}

impl NewApiUsageStats {
    fn has_any(&self) -> bool {
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
}

fn collect_usage_stats(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
    quota_per_unit: f64,
) -> (Option<NewApiUsageStats>, Vec<Value>) {
    let now = unix_now_seconds();
    let today_start = local_today_start_timestamp(now);
    let mut endpoint_results = Vec::new();

    let today_stat = collect_log_stat_window(
        database,
        data_key,
        station,
        today_start,
        now,
        quota_per_unit,
    )
    .map_endpoint_result(&mut endpoint_results);
    let today_logs = collect_log_window(database, data_key, station, today_start, now)
        .map_endpoint_results(&mut endpoint_results);
    let total_stat = collect_log_stat_window(database, data_key, station, 0, now, quota_per_unit)
        .map_endpoint_result(&mut endpoint_results);
    let total_logs = collect_log_window(database, data_key, station, 0, now)
        .map_endpoint_results(&mut endpoint_results);

    let today_split_token_count = today_logs
        .as_ref()
        .and_then(|logs| logs.input_token_count.zip(logs.output_token_count))
        .map(|(input, output)| input + output);
    let total_split_token_count = total_logs
        .as_ref()
        .and_then(|logs| logs.input_token_count.zip(logs.output_token_count))
        .map(|(input, output)| input + output);

    let stats = NewApiUsageStats {
        today_request_count: today_logs
            .as_ref()
            .and_then(|logs| logs.request_count)
            .or_else(|| today_stat.as_ref().and_then(|stat| stat.request_count)),
        total_request_count: total_logs
            .as_ref()
            .and_then(|logs| logs.request_count)
            .or_else(|| total_stat.as_ref().and_then(|stat| stat.request_count)),
        today_consumption: today_stat.as_ref().and_then(|stat| stat.consumption),
        total_consumption: total_stat.as_ref().and_then(|stat| stat.consumption),
        today_base_consumption: today_stat.as_ref().and_then(|stat| stat.base_consumption),
        total_base_consumption: total_stat.as_ref().and_then(|stat| stat.base_consumption),
        today_token_count: today_stat
            .as_ref()
            .and_then(|stat| stat.token_count)
            .or(today_split_token_count),
        total_token_count: total_stat
            .as_ref()
            .and_then(|stat| stat.token_count)
            .or(total_split_token_count),
        today_input_token_count: today_logs.as_ref().and_then(|logs| logs.input_token_count),
        today_output_token_count: today_logs.as_ref().and_then(|logs| logs.output_token_count),
        total_input_token_count: total_logs.as_ref().and_then(|logs| logs.input_token_count),
        total_output_token_count: total_logs.as_ref().and_then(|logs| logs.output_token_count),
    };

    (stats.has_any().then_some(stats), endpoint_results)
}

trait UsageCollectionResultExt<T> {
    fn map_endpoint_result(self, endpoint_results: &mut Vec<Value>) -> Option<T>;
    fn map_endpoint_results(self, endpoint_results: &mut Vec<Value>) -> Option<T>;
}

impl<T> UsageCollectionResultExt<T> for Result<(T, Vec<Value>), String> {
    fn map_endpoint_result(self, endpoint_results: &mut Vec<Value>) -> Option<T> {
        self.map_endpoint_results(endpoint_results)
    }

    fn map_endpoint_results(self, endpoint_results: &mut Vec<Value>) -> Option<T> {
        match self {
            Ok((value, mut results)) => {
                endpoint_results.append(&mut results);
                Some(value)
            }
            Err(_) => None,
        }
    }
}

fn collect_log_stat_window(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
    start_timestamp: i64,
    end_timestamp: i64,
    quota_per_unit: f64,
) -> Result<(NewApiLogStatWindow, Vec<Value>), String> {
    let path = newapi_log_stat_path(start_timestamp, end_timestamp);
    let response = client::get_authenticated_json(
        database,
        data_key,
        station,
        &path,
        client::NewApiOperation::LogStat,
    )
    .map_err(newapi_request_error_message)?;
    let quota = numeric_f64_field(&response.data, &["quota"]).unwrap_or(0.0);
    let base_consumption = numeric_f64_field(
        &response.data,
        &[
            "base_consumption",
            "base_used_amount",
            "base_cost",
            "baseConsumption",
            "baseUsedAmount",
            "baseCost",
        ],
    )
    .or_else(|| {
        numeric_f64_field(
            &response.data,
            &[
                "base_quota",
                "base_used_quota",
                "baseQuota",
                "baseUsedQuota",
            ],
        )
        .map(|value| value / quota_per_unit)
    });
    Ok((
        NewApiLogStatWindow {
            request_count: numeric_i64_field(&response.data, &["rpm", "request_count", "count"]),
            token_count: numeric_i64_field(&response.data, &["tpm", "token_count", "token_used"]),
            consumption: Some(quota / quota_per_unit),
            base_consumption,
        },
        vec![response.endpoint_result],
    ))
}

fn collect_log_window(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
    start_timestamp: i64,
    end_timestamp: i64,
) -> Result<(NewApiLogWindow, Vec<Value>), String> {
    let mut page = 1_usize;
    let mut total = None;
    let mut fetched = 0_usize;
    let mut input_tokens = 0_i64;
    let mut output_tokens = 0_i64;
    let mut saw_token_count = false;
    let mut endpoint_results = Vec::new();

    loop {
        let path = newapi_log_page_path(page, start_timestamp, end_timestamp);
        let response = client::get_authenticated_json(
            database,
            data_key,
            station,
            &path,
            client::NewApiOperation::ListLogs,
        )
        .map_err(newapi_request_error_message)?;
        endpoint_results.push(response.endpoint_result);
        total = total.or_else(|| numeric_usize_field(&response.data, &["total"]));
        let items = response
            .data
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for item in &items {
            let prompt_tokens = numeric_i64_field(
                item,
                &[
                    "prompt_tokens",
                    "input_tokens",
                    "promptTokens",
                    "inputTokens",
                ],
            )
            .unwrap_or(0);
            let completion_tokens = numeric_i64_field(
                item,
                &[
                    "completion_tokens",
                    "output_tokens",
                    "completionTokens",
                    "outputTokens",
                ],
            )
            .unwrap_or(0);
            if prompt_tokens != 0 || completion_tokens != 0 {
                saw_token_count = true;
            }
            input_tokens += prompt_tokens;
            output_tokens += completion_tokens;
        }

        fetched += items.len();
        if items.is_empty()
            || total.is_some_and(|total| fetched >= total)
            || page >= NEWAPI_LOG_MAX_PAGES
        {
            break;
        }
        page += 1;
    }

    Ok((
        NewApiLogWindow {
            request_count: total.and_then(|value| i64::try_from(value).ok()),
            input_token_count: saw_token_count.then_some(input_tokens),
            output_token_count: saw_token_count.then_some(output_tokens),
        },
        endpoint_results,
    ))
}

fn merge_usage_stats_into_balance_data(data: &mut Value, stats: NewApiUsageStats) {
    let Some(object) = data.as_object_mut() else {
        return;
    };
    insert_missing_i64(object, "today_request_count", stats.today_request_count);
    insert_missing_i64_with_aliases(
        object,
        "total_request_count",
        &[
            "total_request_count",
            "request_count",
            "totalRequests",
            "requestCount",
            "requests",
        ],
        stats.total_request_count,
    );
    insert_missing_f64(object, "today_consumption", stats.today_consumption);
    insert_missing_f64(object, "total_consumption", stats.total_consumption);
    insert_missing_f64_with_aliases(
        object,
        "today_base_consumption",
        &[
            "today_base_consumption",
            "today_base_used_amount",
            "today_base_cost",
            "todayBaseConsumption",
            "todayBaseUsedAmount",
            "todayBaseCost",
            "today_quota_consumption",
        ],
        stats.today_base_consumption,
    );
    insert_missing_f64_with_aliases(
        object,
        "total_base_consumption",
        &[
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
        ],
        stats.total_base_consumption,
    );
    insert_missing_i64(object, "today_token_count", stats.today_token_count);
    insert_missing_i64(object, "total_token_count", stats.total_token_count);
    insert_missing_i64(
        object,
        "today_input_token_count",
        stats.today_input_token_count,
    );
    insert_missing_i64(
        object,
        "today_output_token_count",
        stats.today_output_token_count,
    );
    insert_missing_i64(
        object,
        "total_input_token_count",
        stats.total_input_token_count,
    );
    insert_missing_i64(
        object,
        "total_output_token_count",
        stats.total_output_token_count,
    );
}

fn insert_missing_i64(object: &mut serde_json::Map<String, Value>, key: &str, value: Option<i64>) {
    if !object.contains_key(key) {
        if let Some(value) = value {
            object.insert(key.to_string(), json!(value));
        }
    }
}

fn insert_missing_i64_with_aliases(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
    aliases: &[&str],
    value: Option<i64>,
) {
    if aliases.iter().any(|alias| object.contains_key(*alias)) {
        return;
    }
    insert_missing_i64(object, key, value);
}

fn insert_missing_f64(object: &mut serde_json::Map<String, Value>, key: &str, value: Option<f64>) {
    if !object.contains_key(key) {
        if let Some(value) = value {
            object.insert(key.to_string(), json!(value));
        }
    }
}

fn insert_missing_f64_with_aliases(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
    aliases: &[&str],
    value: Option<f64>,
) {
    if aliases.iter().any(|alias| object.contains_key(*alias)) {
        return;
    }
    insert_missing_f64(object, key, value);
}

fn newapi_log_stat_path(start_timestamp: i64, end_timestamp: i64) -> String {
    format!(
        "/api/log/self/stat?type={NEWAPI_LOG_TYPE_CONSUME}&token_name=&model_name=&start_timestamp={start_timestamp}&end_timestamp={end_timestamp}&group="
    )
}

fn newapi_log_page_path(page: usize, start_timestamp: i64, end_timestamp: i64) -> String {
    format!(
        "/api/log/self?p={page}&page_size={NEWAPI_LOG_PAGE_SIZE}&type={NEWAPI_LOG_TYPE_CONSUME}&token_name=&model_name=&start_timestamp={start_timestamp}&end_timestamp={end_timestamp}&group=&request_id="
    )
}

fn unix_now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn local_today_start_timestamp(fallback_now: i64) -> i64 {
    let now = chrono::Local::now();
    let Some(midnight) = now.date_naive().and_hms_opt(0, 0, 0) else {
        return fallback_now;
    };
    midnight
        .and_local_timezone(chrono::Local)
        .earliest()
        .map(|value| value.timestamp())
        .unwrap_or(fallback_now)
}

fn numeric_f64_field(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(|item| item.as_f64().or_else(|| item.as_str()?.trim().parse().ok()))
    })
}

fn numeric_i64_field(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| {
            item.as_i64()
                .or_else(|| item.as_u64().and_then(|value| i64::try_from(value).ok()))
                .or_else(|| item.as_f64().map(|value| value.round() as i64))
                .or_else(|| item.as_str()?.trim().parse().ok())
        })
    })
}

fn numeric_usize_field(value: &Value, keys: &[&str]) -> Option<usize> {
    numeric_i64_field(value, keys).and_then(|value| usize::try_from(value).ok())
}

fn fetch_status(
    database: &AppDatabase,
    station: &Station,
) -> Result<(parsers::NewApiStatus, Value), String> {
    let settings = database.get_settings()?;
    let proxy = resolve_proxy_config(
        &station.collector_proxy_mode,
        station.collector_proxy_url.clone(),
        &settings.collector_proxy_mode,
        settings.collector_proxy_url,
    );
    let url = join_url(&station.website_url, "/api/status");
    let mut endpoint_results = Vec::new();
    let payload = get_newapi_public_json(&url, &proxy, &mut endpoint_results)?;
    let data = parsers::envelope_data(&payload).unwrap_or(&payload);
    let endpoint_result = endpoint_results.into_iter().next().unwrap_or_else(|| {
        json!({
            "url": url,
            "status": null,
            "ok": false,
        })
    });
    Ok((parsers::parse_status(data), endpoint_result))
}

fn newapi_request_error_message(error: client::NewApiRequestError) -> String {
    match error {
        client::NewApiRequestError::AuthRequired { message, .. }
        | client::NewApiRequestError::ManualRequired { message, .. }
        | client::NewApiRequestError::Transient { message, .. }
        | client::NewApiRequestError::OutcomeUnknown { message, .. }
        | client::NewApiRequestError::Permanent { message, .. } => message,
    }
}

fn get_newapi_public_json(
    url: &str,
    proxy: &ProxyConfig,
    endpoint_results: &mut Vec<Value>,
) -> Result<Value, String> {
    let started = std::time::Instant::now();
    let agent = match agent_builder_for_proxy(proxy) {
        Ok(builder) => builder.timeout(COLLECTOR_HTTP_TIMEOUT).build(),
        Err(error) => {
            let message = crate::services::secrets::mask::redact_text(&error);
            endpoint_results.push(json!({
                "url": url,
                "status": null,
                "ok": false,
                "durationMs": started.elapsed().as_millis() as i64,
                "errorMessage": message,
            }));
            return Err(message);
        }
    };
    let response = match agent
        .get(url)
        .timeout(COLLECTOR_HTTP_TIMEOUT)
        .set("Content-Type", "application/json")
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => {
            let message = crate::services::secrets::mask::redact_text(&error.to_string());
            endpoint_results.push(json!({
                "url": url,
                "status": null,
                "ok": false,
                "durationMs": started.elapsed().as_millis() as i64,
                "errorMessage": message,
            }));
            return Err(message);
        }
    };
    let status = response.status();
    let text = response.into_string().unwrap_or_default();
    endpoint_results.push(json!({
        "url": url,
        "status": status,
        "ok": (200..400).contains(&status),
        "durationMs": started.elapsed().as_millis() as i64,
    }));
    let payload = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
    if (200..400).contains(&status) {
        Ok(payload)
    } else {
        Err(crate::services::secrets::mask::redact_text(&text))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{credentials::UpdateStationSessionInput, stations::CreateStationInput},
        services::{
            collectors::adapters::newapi::test_support::{json_response, TestHttpServer},
            database::AppDatabase,
            secrets::crypto::generate_data_key,
        },
    };
    use serde_json::json;

    #[test]
    #[ignore = "requires RELAY_POOL_LIVE_NEWAPI_RUN=1 plus live NewAPI credentials in env"]
    fn live_newapi_password_login_and_collects_core_facts_from_env() {
        if std::env::var("RELAY_POOL_LIVE_NEWAPI_RUN").as_deref() != Ok("1") {
            return;
        }
        let base_url = std::env::var("RELAY_POOL_LIVE_NEWAPI_BASE_URL")
            .expect("RELAY_POOL_LIVE_NEWAPI_BASE_URL");
        let username = std::env::var("RELAY_POOL_LIVE_NEWAPI_USERNAME")
            .expect("RELAY_POOL_LIVE_NEWAPI_USERNAME");
        let password = std::env::var("RELAY_POOL_LIVE_NEWAPI_PASSWORD")
            .expect("RELAY_POOL_LIVE_NEWAPI_PASSWORD");
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station(CreateStationInput {
                name: "live newapi smoke".to_string(),
                station_type: "newapi".to_string(),
                website_url: base_url.to_string(),
                api_base_url: format!("{}/v1", base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: String::new(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");

        let login = login_with_password(&database, &data_key, &station, &username, &password)
            .expect("login");
        assert!(
            login.cookie_present,
            "NewAPI live login did not return a usable cookie session"
        );

        for task in [
            CollectorTask::Balance,
            CollectorTask::Groups,
            CollectorTask::Models,
        ] {
            let output = collect(&database, &data_key, &station.id, task).expect("collect task");
            assert!(
                output.status == "success" || output.status == "partial",
                "live NewAPI task returned unexpected status"
            );
        }
    }

    #[test]
    fn newapi_quota_converts_to_usd_units() {
        let fact = parse_newapi_balance(
            "station-1",
            &json!({
                "quota": 1000000.0,
                "used_quota": 500000.0,
                "group": "default"
            }),
        );

        assert_eq!(fact.value, Some(2.0));
        assert_eq!(fact.used_value, Some(1.0));
        assert_eq!(fact.total_value, Some(3.0));
        assert_eq!(fact.currency, "USD");
        assert_eq!(fact.source, "newapi_user_self");
    }

    #[test]
    fn newapi_balance_collects_usage_logs_for_request_cost_and_tokens() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota_per_unit": 500000 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "quota": 1000000,
                        "used_quota": 500000,
                        "request_count": 1200
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 375000, "rpm": 2, "tpm": 49567 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 2,
                        "items": [
                            { "prompt_tokens": 30000, "completion_tokens": 4567 },
                            { "prompt_tokens": 10000, "completion_tokens": 5000 }
                        ]
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 9250000, "rpm": 3, "tpm": 422890 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 3,
                        "items": [
                            { "prompt_tokens": 30000, "completion_tokens": 4567 },
                            { "prompt_tokens": 10000, "completion_tokens": 5000 },
                            { "prompt_tokens": 250000, "completion_tokens": 123323 }
                        ]
                    }
                }),
            )),
        ]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let output = collect(&database, &data_key, &station.id, CollectorTask::Balance)
            .expect("balance collect");
        let requests = server.finish();
        let balance = output.facts.balances.first().expect("balance fact");

        assert_eq!(output.status, "success");
        assert_eq!(balance.today_request_count, Some(2));
        assert_eq!(balance.total_request_count, Some(1200));
        assert_eq!(balance.today_consumption, Some(0.75));
        assert_eq!(balance.total_consumption, Some(18.5));
        assert_eq!(balance.today_input_token_count, Some(40000));
        assert_eq!(balance.today_output_token_count, Some(9567));
        assert_eq!(balance.today_token_count, Some(49567));
        assert_eq!(balance.total_input_token_count, Some(290000));
        assert_eq!(balance.total_output_token_count, Some(132890));
        assert_eq!(balance.total_token_count, Some(422890));
        assert!(requests
            .iter()
            .any(|request| request.starts_with("GET /api/log/self/stat?type=2&")));
        assert!(requests
            .iter()
            .any(|request| request.starts_with("GET /api/log/self?p=1&page_size=100&type=2&")));
    }

    #[test]
    fn newapi_balance_collects_total_tokens_from_stat_when_logs_have_no_split() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota_per_unit": 500000 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "quota": 1000000,
                        "used_quota": 0,
                        "request_count": 0
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 0, "rpm": 0, "tpm": 54321 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 0,
                        "items": []
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 0, "rpm": 0, "tpm": 987654 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 0,
                        "items": []
                    }
                }),
            )),
        ]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let output = collect(&database, &data_key, &station.id, CollectorTask::Balance)
            .expect("balance collect");
        let balance = output.facts.balances.first().expect("balance fact");

        assert_eq!(output.status, "success");
        assert_eq!(balance.today_request_count, Some(0));
        assert_eq!(balance.today_consumption, Some(0.0));
        assert_eq!(balance.today_input_token_count, None);
        assert_eq!(balance.today_output_token_count, None);
        assert_eq!(balance.today_token_count, Some(54321));
        assert_eq!(balance.total_request_count, Some(0));
        assert_eq!(balance.total_consumption, Some(0.0));
        assert_eq!(balance.total_input_token_count, None);
        assert_eq!(balance.total_output_token_count, None);
        assert_eq!(balance.total_token_count, Some(987654));
    }

    #[test]
    fn newapi_groups_parse_list_and_rate_fields() {
        let facts = parse_newapi_group_facts(
            "station-1",
            &json!({
                "default": { "desc": "Default", "ratio": 1.0 },
                "vip": { "desc": "VIP", "ratio": 0.8 }
            }),
        );

        assert!(facts
            .groups
            .iter()
            .any(|group| group.group_name == "default"));
        assert!(facts.rates.iter().any(|rate| {
            rate.group_name == "vip" && rate.effective_rate_multiplier == Some(0.8)
        }));
    }

    #[test]
    fn models_snapshot_keeps_top_level_models_contract() {
        let output = build_models_output(
            "station-1",
            parsers::envelope_data(
                &json!({"success": true, "data": ["gpt-4.1-mini", "claude-sonnet"]}),
            )
            .expect("model data"),
            json!({"path": "/api/user/models", "status": 200, "ok": true}),
        );
        assert_eq!(
            output.normalized_json["models"],
            json!(["gpt-4.1-mini", "claude-sonnet"])
        );
        assert_eq!(output.facts.models.len(), 2);
    }

    #[test]
    fn empty_successful_group_payload_is_partial() {
        let output = build_groups_output(
            "station-1",
            parsers::envelope_data(&json!({"success": true, "data": {}})).expect("group data"),
            json!({"path": "/api/user/self/groups", "status": 200, "ok": true}),
        );
        assert_eq!(output.status, "partial");
        assert_eq!(output.error_code.as_deref(), Some("empty_group_facts"));
    }

    #[test]
    fn scan_remote_keys_paginates_newapi_tokens_without_full_secret() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 2,
                        "total": 3,
                        "items": [
                            {
                                "id": 101,
                                "name": "primary",
                                "key": "sk-abc**********7890",
                                "group": "default",
                                "created_time": 1760000000,
                                "accessed_time": 1760000100
                            },
                            {
                                "id": 102,
                                "name": "secondary",
                                "key": "sk-def**********4567",
                                "group": "vip"
                            }
                        ]
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 2,
                        "page_size": 2,
                        "total": 3,
                        "items": [
                            {
                                "id": 103,
                                "name": "third",
                                "key": "sk-ghi**********1234",
                                "group": "vip"
                            }
                        ]
                    }
                }),
            )),
        ]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let keys = scan_remote_keys(&database, &data_key, &station.id).expect("scan keys");
        let requests = server.finish();

        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0].remote_key_name.as_deref(), Some("primary"));
        assert_eq!(
            keys[0].api_key_masked.as_deref(),
            Some("sk-abc**********7890")
        );
        assert_eq!(keys[0].api_key_fingerprint, None);
        assert_eq!(keys[0].group_name.as_deref(), Some("default"));
        assert_eq!(keys[0].raw_source, "newapi_tokens");
        assert!(requests[0].starts_with("GET /api/token/?p=1&page_size=100 "));
        assert!(requests[1].starts_with("GET /api/token/?p=2&page_size=100 "));
        assert!(requests[0].contains("New-Api-User: 42"));
    }

    #[test]
    fn scan_remote_keys_errors_before_returning_partial_pages() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 2,
                        "total": 3,
                        "items": [
                            { "id": 101, "name": "primary", "key": "sk-abc**********7890" },
                            { "id": 102, "name": "secondary", "key": "sk-def**********4567" }
                        ]
                    }
                }),
            )),
            Some(json_response(
                502,
                json!({"success": false, "message": "bad gateway"}),
            )),
            Some(json_response(
                502,
                json!({"success": false, "message": "bad gateway"}),
            )),
        ]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let error = scan_remote_keys(&database, &data_key, &station.id).unwrap_err();
        let requests = server.finish();

        assert!(!error.trim().is_empty());
        assert_eq!(requests.len(), 3);
    }

    #[test]
    fn create_remote_key_posts_token_then_reconciles_and_reveals_secret() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(200, json!({"success": true, "message": ""}))),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 1,
                        "items": [{
                            "id": 301,
                            "name": "relay-created",
                            "key": "sk-crt**********f260",
                            "group": "vip"
                        }]
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "key": "sk-created-secret-f260" }
                }),
            )),
        ]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let created = create_remote_key(
            &database,
            &data_key,
            CreateRemoteStationKeyInput {
                station_id: station.id.clone(),
                name: "relay-created".to_string(),
                group_binding_id: None,
                group_id_hash: None,
                group_name: Some("vip".to_string()),
            },
        )
        .expect("created remote key");
        let requests = server.finish();

        assert_eq!(
            created.remote_key.remote_key_name.as_deref(),
            Some("relay-created")
        );
        assert_eq!(created.remote_key.group_name.as_deref(), Some("vip"));
        assert_eq!(
            created.full_key_once.as_deref(),
            Some("sk-created-secret-f260")
        );
        assert!(requests[0].starts_with("POST /api/token/ "));
        assert!(requests[0].contains("\"name\":\"relay-created\""));
        assert!(requests[0].contains("\"group\":\"vip\""));
        assert!(requests[1].starts_with("GET /api/token/?p=1&page_size=100 "));
        assert!(requests[2].starts_with("POST /api/token/301/key "));
    }

    fn test_station(database: &AppDatabase, base_url: &str) -> Station {
        database
            .create_station(CreateStationInput {
                name: "newapi station".to_string(),
                station_type: "newapi".to_string(),
                website_url: base_url.to_string(),
                api_base_url: format!("{}/v1", base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: String::new(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station")
    }

    fn persist_access_token_session(database: &AppDatabase, data_key: &[u8; 32], station_id: &str) {
        database
            .update_station_session_with_data_key(
                UpdateStationSessionInput {
                    station_id: station_id.to_string(),
                    access_token: Some("newapi-access-token".to_string()),
                    refresh_token: None,
                    cookie: None,
                    newapi_user_id: Some("42".to_string()),
                    token_expires_at: None,
                },
                data_key,
            )
            .expect("session");
    }
}
