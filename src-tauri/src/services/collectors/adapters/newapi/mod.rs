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
    },
    database::AppDatabase,
    outbound::{credential_agent_builder_for_proxy, resolve_proxy_config, ProxyConfig},
    station_endpoints::build_management_url,
};

const COLLECTOR_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const NEWAPI_REMOTE_KEY_PAGE_SIZE: usize = 100;
const NEWAPI_LOG_PAGE_SIZE: usize = 100;
const NEWAPI_LOG_MAX_PAGES: usize = 100;
const NEWAPI_LOG_TYPE_CONSUME: i64 = 2;
const NEWAPI_DASHBOARD_MAX_WINDOW_SECONDS: i64 = 30 * 24 * 60 * 60;
const NEWAPI_DASHBOARD_TOTAL_START_TIMESTAMP: i64 = 0;
const NEWAPI_DASHBOARD_TOTAL_MAX_WINDOWS: usize = 240;

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
    parsers::parse_balance_fact(station_id, payload, Some(500000.0))
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
    let mut expected_total = None;
    loop {
        let response = client::get_authenticated_json(
            database,
            data_key,
            station,
            &format!("/api/token/?p={page}&page_size={NEWAPI_REMOTE_KEY_PAGE_SIZE}"),
            client::NewApiOperation::ListTokens,
        )
        .map_err(newapi_request_error_message)?;
        let response_page = numeric_usize_field(&response.data, &["page"])
            .filter(|value| *value == page)
            .ok_or_else(|| "NewAPI token pagination is missing a valid page number".to_string())?;
        let page_size = page_size_from_payload(&response.data)
            .ok_or_else(|| "NewAPI token pagination is missing page_size".to_string())?;
        let total = total_from_payload(&response.data)
            .ok_or_else(|| "NewAPI token pagination is missing total".to_string())?;
        if expected_total.is_some_and(|expected| expected != total) {
            return Err("NewAPI token pagination total changed between pages".to_string());
        }
        expected_total = Some(total);
        let page_items = remote_key_items(&response.data)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let page_item_count = page_items.len();
        if page_item_count > page_size {
            return Err("NewAPI token pagination returned more items than page_size".to_string());
        }
        items.extend(page_items);
        if items.len() > total {
            return Err("NewAPI token pagination returned more items than total".to_string());
        }
        if items.len() == total {
            break;
        }
        if page_item_count == 0 || page_item_count < page_size {
            return Err("NewAPI token pagination ended before reaching total".to_string());
        }
        page = response_page.saturating_add(1);
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
    string_field(value, "name")
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
    let token_id = numeric_i64_field(value, &["id"])
        .filter(|value| *value > 0)
        .map(|value| value.to_string())
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
    payload
        .get("items")
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn remote_key_from_value(
    station_id: &str,
    value: &Value,
    index: usize,
) -> Option<RemoteStationKey> {
    let remote_key_id = numeric_i64_field(value, &["id"])
        .filter(|value| *value > 0)
        .map(|value| value.to_string());
    let name = string_field(value, "name");
    let key_value = string_field(value, "key");
    let full_key = key_value
        .as_deref()
        .filter(|value| looks_like_full_api_key(value))
        .map(ToString::to_string);
    let masked = full_key
        .as_deref()
        .map(crate::services::secrets::mask::mask_secret)
        .or_else(|| {
            key_value
                .clone()
                .filter(|value| !looks_like_full_api_key(value))
        });
    let (identity_kind, identity, include_index) = remote_key_identity(
        remote_key_id.as_deref(),
        full_key.as_deref(),
        masked.as_deref(),
        name.as_deref(),
    )?;
    let group_name = string_field(value, "group");
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
        created_at: numeric_i64_field(value, &["created_time"]).map(|value| value.to_string()),
        last_used_at: numeric_i64_field(value, &["accessed_time"]).map(|value| value.to_string()),
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
    string_field(payload, "key").filter(|value| looks_like_full_api_key(value))
}

fn page_size_from_payload(payload: &Value) -> Option<usize> {
    payload
        .get("page_size")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
}

fn total_from_payload(payload: &Value) -> Option<usize> {
    payload
        .get("total")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .map(ToString::to_string)
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
            "quotaPerUnitAvailable": status.quota_per_unit.is_some(),
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
        let (usage_stats, usage_endpoint_results) = collect_usage_stats(
            database,
            data_key,
            &station,
            &response.data,
            status.quota_per_unit,
        );
        let mut balance_data = response.data;
        merge_optional_usage_stats_into_balance_data(&mut balance_data, usage_stats);
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
    consumption: Option<f64>,
    base_consumption: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct NewApiLogWindow {
    request_count: Option<i64>,
    input_token_count: Option<i64>,
    output_token_count: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct NewApiDashboardUsageWindow {
    request_count: Option<i64>,
    token_count: Option<i64>,
    quota: Option<i64>,
    consumption: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct NewApiDashboardTotalTarget {
    request_count: i64,
    quota: i64,
}

impl NewApiDashboardUsageWindow {
    fn add(&mut self, other: NewApiDashboardUsageWindow) {
        if !self.has_any() {
            self.request_count = other.request_count;
            self.token_count = other.token_count;
            self.quota = other.quota;
            self.consumption = other.consumption;
            return;
        }
        self.request_count = checked_sum_i64(self.request_count, other.request_count);
        self.token_count = checked_sum_i64(self.token_count, other.token_count);
        self.quota = checked_sum_i64(self.quota, other.quota);
        self.consumption = checked_sum_f64(self.consumption, other.consumption);
    }

    fn has_any(&self) -> bool {
        self.request_count.is_some()
            || self.token_count.is_some()
            || self.quota.is_some()
            || self.consumption.is_some()
    }
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
    self_data: &Value,
    quota_per_unit: Option<f64>,
) -> (Option<NewApiUsageStats>, Vec<Value>) {
    let now = unix_now_seconds();
    let today_start = local_today_start_timestamp(now);
    let mut endpoint_results = Vec::new();

    let today_dashboard = collect_dashboard_usage_window(
        database,
        data_key,
        station,
        today_start,
        now,
        quota_per_unit,
    )
    .map_endpoint_result(&mut endpoint_results);
    let total_dashboard =
        collect_dashboard_usage_total(database, data_key, station, self_data, now, quota_per_unit)
            .map_endpoint_results(&mut endpoint_results);
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
    let self_request_count = numeric_i64_field(self_data, &["request_count"]);
    let total_split_token_count = total_logs
        .as_ref()
        .filter(|logs| logs.request_count.is_some() && logs.request_count == self_request_count)
        .and_then(|logs| logs.input_token_count.zip(logs.output_token_count))
        .map(|(input, output)| input + output);

    let stats = NewApiUsageStats {
        today_request_count: today_dashboard
            .as_ref()
            .and_then(|dashboard| dashboard.request_count)
            .or_else(|| today_logs.as_ref().and_then(|logs| logs.request_count)),
        total_request_count: None,
        today_consumption: today_dashboard
            .as_ref()
            .and_then(|dashboard| dashboard.consumption)
            .or_else(|| today_stat.as_ref().and_then(|stat| stat.consumption)),
        total_consumption: total_dashboard
            .as_ref()
            .and_then(|dashboard| dashboard.consumption)
            .or_else(|| total_stat.as_ref().and_then(|stat| stat.consumption)),
        today_base_consumption: today_stat.as_ref().and_then(|stat| stat.base_consumption),
        total_base_consumption: total_stat.as_ref().and_then(|stat| stat.base_consumption),
        today_token_count: today_dashboard
            .as_ref()
            .and_then(|dashboard| dashboard.token_count)
            .or(today_split_token_count),
        total_token_count: total_dashboard
            .as_ref()
            .and_then(|dashboard| dashboard.token_count)
            .or(total_split_token_count),
        today_input_token_count: None,
        today_output_token_count: None,
        total_input_token_count: None,
        total_output_token_count: None,
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
    quota_per_unit: Option<f64>,
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
    let consumption = quota_per_unit
        .zip(numeric_f64_field(&response.data, &["quota"]))
        .map(|(quota_per_unit, quota)| quota / quota_per_unit);
    Ok((
        NewApiLogStatWindow {
            consumption,
            base_consumption: None,
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
    let mut saw_incomplete_token_fields = false;
    let mut endpoint_results = Vec::new();
    let mut completed_window = false;

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
        let response_page = numeric_usize_field(&response.data, &["page"])
            .filter(|value| *value == page)
            .ok_or_else(|| "NewAPI log pagination is missing a valid page number".to_string())?;
        let page_size = numeric_usize_field(&response.data, &["page_size"])
            .filter(|value| *value > 0)
            .ok_or_else(|| "NewAPI log pagination is missing page_size".to_string())?;
        let response_total = numeric_usize_field(&response.data, &["total"])
            .ok_or_else(|| "NewAPI log pagination is missing total".to_string())?;
        if total.is_some_and(|expected| expected != response_total) {
            return Err("NewAPI log pagination total changed between pages".to_string());
        }
        total = Some(response_total);
        let items = response
            .data
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| "NewAPI log pagination is missing items".to_string())?;
        if items.len() > page_size {
            return Err("NewAPI log pagination returned more items than page_size".to_string());
        }
        for item in &items {
            let prompt_tokens = numeric_i64_field(item, &["prompt_tokens"]);
            let completion_tokens = numeric_i64_field(item, &["completion_tokens"]);
            match (
                prompt_tokens.filter(|value| *value >= 0),
                completion_tokens.filter(|value| *value >= 0),
            ) {
                (Some(prompt_tokens), Some(completion_tokens)) => {
                    if let (Some(next_input), Some(next_output)) = (
                        input_tokens.checked_add(prompt_tokens),
                        output_tokens.checked_add(completion_tokens),
                    ) {
                        saw_token_count = true;
                        input_tokens = next_input;
                        output_tokens = next_output;
                    } else {
                        saw_incomplete_token_fields = true;
                    }
                }
                _ => saw_incomplete_token_fields = true,
            }
        }

        fetched = fetched
            .checked_add(items.len())
            .ok_or_else(|| "NewAPI log pagination count overflowed".to_string())?;
        if response_total >= NEWAPI_LOG_PAGE_SIZE * NEWAPI_LOG_MAX_PAGES {
            break;
        }
        if fetched > response_total {
            return Err("NewAPI log pagination returned more items than total".to_string());
        }
        if fetched == response_total {
            completed_window = true;
            break;
        }
        if items.len() < page_size {
            return Err("NewAPI log pagination ended before reaching total".to_string());
        }
        if page >= NEWAPI_LOG_MAX_PAGES {
            break;
        }
        page = response_page.saturating_add(1);
    }

    Ok((
        NewApiLogWindow {
            request_count: completed_window
                .then(|| total.and_then(|value| i64::try_from(value).ok()))
                .flatten(),
            input_token_count: (saw_token_count
                && !saw_incomplete_token_fields
                && completed_window)
                .then_some(input_tokens),
            output_token_count: (saw_token_count
                && !saw_incomplete_token_fields
                && completed_window)
                .then_some(output_tokens),
        },
        endpoint_results,
    ))
}

fn collect_dashboard_usage_window(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
    start_timestamp: i64,
    end_timestamp: i64,
    quota_per_unit: Option<f64>,
) -> Result<(NewApiDashboardUsageWindow, Vec<Value>), String> {
    let path = newapi_dashboard_data_path(start_timestamp, end_timestamp);
    let response = client::get_authenticated_json(
        database,
        data_key,
        station,
        &path,
        client::NewApiOperation::DashboardData,
    )
    .map_err(newapi_request_error_message)?;

    let mut request_count = 0_i64;
    let mut token_count = 0_i64;
    let mut quota = 0_i64;
    let mut saw_request_count = false;
    let mut saw_token_count = false;
    let mut saw_quota = false;
    let mut request_count_complete = true;
    let mut token_count_complete = true;
    let mut quota_complete = true;

    for item in dashboard_usage_items(&response.data) {
        match numeric_i64_field(item, &["count"]).filter(|value| *value >= 0) {
            Some(value) => match request_count.checked_add(value) {
                Some(next) => {
                    request_count = next;
                    saw_request_count = true;
                }
                None => request_count_complete = false,
            },
            None => request_count_complete = false,
        }
        match numeric_i64_field(item, &["token_used"]).filter(|value| *value >= 0) {
            Some(value) => match token_count.checked_add(value) {
                Some(next) => {
                    token_count = next;
                    saw_token_count = true;
                }
                None => token_count_complete = false,
            },
            None => token_count_complete = false,
        }
        match numeric_i64_field(item, &["quota"]) {
            Some(value) => match quota.checked_add(value) {
                Some(next) => {
                    quota = next;
                    saw_quota = true;
                }
                None => quota_complete = false,
            },
            None => quota_complete = false,
        }
    }
    let request_count = (saw_request_count && request_count_complete).then_some(request_count);
    let token_count = (saw_token_count && token_count_complete).then_some(token_count);
    let quota = (saw_quota && quota_complete).then_some(quota);

    Ok((
        NewApiDashboardUsageWindow {
            request_count,
            token_count,
            quota,
            consumption: quota_per_unit
                .zip(quota)
                .map(|(quota_per_unit, quota)| quota as f64 / quota_per_unit),
        },
        vec![response.endpoint_result],
    ))
}

fn collect_dashboard_usage_total(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
    self_data: &Value,
    now: i64,
    quota_per_unit: Option<f64>,
) -> Result<(NewApiDashboardUsageWindow, Vec<Value>), String> {
    let target = dashboard_total_target(self_data);
    collect_dashboard_usage_total_backwards(
        database,
        data_key,
        station,
        now,
        quota_per_unit,
        target,
    )
}

fn collect_dashboard_usage_total_backwards(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &Station,
    now: i64,
    quota_per_unit: Option<f64>,
    target: Option<NewApiDashboardTotalTarget>,
) -> Result<(NewApiDashboardUsageWindow, Vec<Value>), String> {
    let Some(target) = target else {
        return Err("NewAPI dashboard total requires used_quota and request_count".to_string());
    };

    let mut end_timestamp = now;
    let mut total = NewApiDashboardUsageWindow::default();
    let mut endpoint_results = Vec::new();
    let mut collected_any = false;

    for _ in 0..NEWAPI_DASHBOARD_TOTAL_MAX_WINDOWS {
        let start_timestamp = end_timestamp
            .saturating_sub(NEWAPI_DASHBOARD_MAX_WINDOW_SECONDS - 1)
            .max(NEWAPI_DASHBOARD_TOTAL_START_TIMESTAMP);
        let (window, mut results) = collect_dashboard_usage_window(
            database,
            data_key,
            station,
            start_timestamp,
            end_timestamp,
            quota_per_unit,
        )?;
        let window_has_any = window.has_any();
        if window_has_any {
            collected_any = true;
            total.add(window);
        } else if target.request_count == 0 && target.quota == 0 {
            return Err("NewAPI dashboard data response did not contain usage facts".to_string());
        }
        endpoint_results.append(&mut results);
        if dashboard_total_matches_target(&total, target) {
            return Ok((total, endpoint_results));
        }
        if start_timestamp <= NEWAPI_DASHBOARD_TOTAL_START_TIMESTAMP {
            break;
        }
        end_timestamp = start_timestamp.saturating_sub(1);
    }

    Err(if collected_any {
        "NewAPI dashboard total response did not cover all-time usage".to_string()
    } else {
        "NewAPI dashboard data response did not contain usage facts".to_string()
    })
}

fn dashboard_total_matches_target(
    total: &NewApiDashboardUsageWindow,
    target: NewApiDashboardTotalTarget,
) -> bool {
    total.quota == Some(target.quota) && total.request_count == Some(target.request_count)
}

fn dashboard_total_target(self_data: &Value) -> Option<NewApiDashboardTotalTarget> {
    Some(NewApiDashboardTotalTarget {
        request_count: numeric_i64_field(self_data, &["request_count"])
            .filter(|value| *value >= 0)?,
        quota: numeric_i64_field(self_data, &["used_quota"]).filter(|value| *value >= 0)?,
    })
}

fn dashboard_usage_items(payload: &Value) -> Vec<&Value> {
    payload
        .as_array()
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn merge_usage_stats_into_balance_data(data: &mut Value, stats: NewApiUsageStats) {
    let Some(object) = data.as_object_mut() else {
        return;
    };
    for key in [
        "today_request_count",
        "today_consumption",
        "today_base_consumption",
        "total_base_consumption",
        "today_token_count",
        "total_token_count",
        "today_input_token_count",
        "today_output_token_count",
        "total_input_token_count",
        "total_output_token_count",
    ] {
        object.remove(key);
    }
    insert_i64(object, "today_request_count", stats.today_request_count);
    insert_f64(object, "today_consumption", stats.today_consumption);
    insert_f64(
        object,
        "today_base_consumption",
        stats.today_base_consumption,
    );
    insert_f64(
        object,
        "total_base_consumption",
        stats.total_base_consumption,
    );
    insert_i64(object, "today_token_count", stats.today_token_count);
    insert_i64(object, "total_token_count", stats.total_token_count);
    insert_i64(
        object,
        "today_input_token_count",
        stats.today_input_token_count,
    );
    insert_i64(
        object,
        "today_output_token_count",
        stats.today_output_token_count,
    );
    insert_i64(
        object,
        "total_input_token_count",
        stats.total_input_token_count,
    );
    insert_i64(
        object,
        "total_output_token_count",
        stats.total_output_token_count,
    );
}

fn merge_optional_usage_stats_into_balance_data(data: &mut Value, stats: Option<NewApiUsageStats>) {
    merge_usage_stats_into_balance_data(data, stats.unwrap_or_default());
}

fn insert_i64(object: &mut serde_json::Map<String, Value>, key: &str, value: Option<i64>) {
    if let Some(value) = value {
        object.insert(key.to_string(), json!(value));
    }
}

fn insert_f64(object: &mut serde_json::Map<String, Value>, key: &str, value: Option<f64>) {
    if let Some(value) = value {
        object.insert(key.to_string(), json!(value));
    }
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

fn newapi_dashboard_data_path(start_timestamp: i64, end_timestamp: i64) -> String {
    format!(
        "/api/data/self?start_timestamp={start_timestamp}&end_timestamp={end_timestamp}&default_time=hour"
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
            .filter(|value| value.is_finite())
    })
}

fn numeric_i64_field(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| {
            item.as_i64()
                .or_else(|| item.as_u64().and_then(|value| i64::try_from(value).ok()))
                .or_else(|| {
                    item.as_f64().and_then(|value| {
                        (value.is_finite()
                            && value.fract() == 0.0
                            && value >= i64::MIN as f64
                            && value <= i64::MAX as f64)
                            .then_some(value as i64)
                    })
                })
                .or_else(|| item.as_str()?.trim().parse().ok())
        })
    })
}

fn numeric_usize_field(value: &Value, keys: &[&str]) -> Option<usize> {
    numeric_i64_field(value, keys).and_then(|value| usize::try_from(value).ok())
}

fn checked_sum_i64(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    left.zip(right)
        .and_then(|(left, right)| left.checked_add(right))
}

fn checked_sum_f64(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    left.zip(right)
        .map(|(left, right)| left + right)
        .filter(|value| value.is_finite())
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
    let url = build_management_url(&station.website_url, "/api/status")?;
    let mut endpoint_results = Vec::new();
    let payload = get_newapi_public_json(&url, &proxy, &mut endpoint_results)?;
    let status = parse_status_payload(&payload)?;
    let endpoint_result = endpoint_results.into_iter().next().unwrap_or_else(|| {
        json!({
            "url": url,
            "status": null,
            "ok": false,
        })
    });
    Ok((status, endpoint_result))
}

fn parse_status_payload(payload: &Value) -> Result<parsers::NewApiStatus, String> {
    parsers::envelope_data(payload)
        .map(parsers::parse_status)
        .map_err(|error| error.message)
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
    let agent = match credential_agent_builder_for_proxy(proxy) {
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
    fn newapi_balance_collects_usage_logs_for_request_count_cost_and_total_tokens() {
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
                        "used_quota": 9250000,
                        "request_count": 1200
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 2, "quota": 375000, "token_used": 49567 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 1200, "quota": 9250000, "token_used": 422890 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 375000, "rpm": 2, "tpm": 0 }
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
                    "data": { "quota": 9250000, "rpm": 3, "tpm": 0 }
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
        assert_eq!(balance.today_token_count, Some(49567));
        assert_eq!(balance.today_input_token_count, None);
        assert_eq!(balance.today_output_token_count, None);
        assert_eq!(balance.total_token_count, Some(422890));
        assert_eq!(balance.total_input_token_count, None);
        assert_eq!(balance.total_output_token_count, None);
        assert!(requests
            .iter()
            .any(|request| request.starts_with("GET /api/log/self/stat?type=2&")));
        assert!(requests
            .iter()
            .any(|request| request.starts_with("GET /api/log/self?p=1&page_size=100&type=2&")));
    }

    #[test]
    fn newapi_balance_does_not_treat_used_quota_as_tokens_in_token_display_mode() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "quota_per_unit": 500000,
                        "quota_display_type": "TOKENS"
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "quota": 1000000,
                        "used_quota": 9250000,
                        "request_count": 1200
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 2, "quota": 375000, "token_used": 175200000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 1200, "quota": 9250000, "token_used": 470000000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 375000, "rpm": 2, "tpm": 0 }
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
                            { "prompt_tokens": 100000000, "completion_tokens": 50000000 },
                            { "prompt_tokens": 20000000, "completion_tokens": 5200000 }
                        ]
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 9250000, "rpm": 3, "tpm": 0 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 10000,
                        "items": [
                            { "prompt_tokens": 100000000, "completion_tokens": 50000000 },
                            { "prompt_tokens": 20000000, "completion_tokens": 5200000 }
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
        assert_eq!(balance.today_token_count, Some(175200000));
        assert_eq!(balance.total_token_count, Some(470000000));
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.starts_with("GET /api/data/self?"))
                .count(),
            2
        );
        assert!(requests
            .iter()
            .any(|request| request.starts_with("GET /api/data/self?")));
    }

    #[test]
    fn truncated_log_window_does_not_report_exact_request_count() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": {
                    "page": 1,
                    "page_size": 100,
                    "total": 10000,
                    "items": [
                        { "prompt_tokens": 10, "completion_tokens": 5 }
                    ]
                }
            }),
        ))]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let (window, _) =
            collect_log_window(&database, &data_key, &station, 0, 1).expect("log window");
        server.finish();

        assert_eq!(window.request_count, None);
        assert_eq!(window.input_token_count, None);
        assert_eq!(window.output_token_count, None);
    }

    #[test]
    fn log_window_with_missing_token_field_keeps_token_totals_unknown() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": {
                    "page": 1,
                    "page_size": 100,
                    "total": 1,
                    "items": [
                        { "prompt_tokens": 10 }
                    ]
                }
            }),
        ))]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let (window, _) =
            collect_log_window(&database, &data_key, &station, 0, 1).expect("log window");
        server.finish();

        assert_eq!(window.request_count, Some(1));
        assert_eq!(window.input_token_count, None);
        assert_eq!(window.output_token_count, None);
    }

    #[test]
    fn log_window_rejects_missing_standard_total() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": {
                    "page": 1,
                    "page_size": 100,
                    "items": [
                        { "prompt_tokens": 10, "completion_tokens": 5 }
                    ]
                }
            }),
        ))]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let error = collect_log_window(&database, &data_key, &station, 0, 1).unwrap_err();
        server.finish();

        assert!(error.contains("pagination"));
    }

    #[test]
    fn log_stat_without_quota_keeps_consumption_unknown() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": { "rpm": 1, "tpm": 25 }
            }),
        ))]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let (window, _) =
            collect_log_stat_window(&database, &data_key, &station, 0, 1, Some(500000.0))
                .expect("log stat window");
        server.finish();

        assert_eq!(window.consumption, None);
    }

    #[test]
    fn log_stat_does_not_guess_nonstandard_base_consumption() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": { "quota": 500000, "base_cost": 9.5 }
            }),
        ))]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let (window, _) =
            collect_log_stat_window(&database, &data_key, &station, 0, 1, Some(500000.0))
                .expect("log stat window");
        server.finish();

        assert_eq!(window.consumption, Some(1.0));
        assert_eq!(window.base_consumption, None);
    }

    #[test]
    fn dashboard_usage_items_require_standard_array_shape() {
        assert_eq!(dashboard_usage_items(&json!([{ "count": 1 }])).len(), 1);
        assert!(dashboard_usage_items(&json!({
            "items": [{ "count": 1 }]
        }))
        .is_empty());
        assert!(dashboard_usage_items(&json!({ "count": 1 })).is_empty());
    }

    #[test]
    fn dashboard_window_does_not_sum_partial_rows() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": [
                    { "count": 2, "quota": 300000, "token_used": 100 },
                    { "count": 3, "quota": 400000 }
                ]
            }),
        ))]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let (window, _) =
            collect_dashboard_usage_window(&database, &data_key, &station, 0, 1, Some(500000.0))
                .expect("dashboard window");
        server.finish();

        assert_eq!(window.request_count, Some(5));
        assert_eq!(window.quota, Some(700000));
        assert_eq!(window.consumption, Some(1.4));
        assert_eq!(window.token_count, None);
    }

    #[test]
    fn log_window_rejects_negative_token_values() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": {
                    "page": 1,
                    "page_size": 100,
                    "total": 1,
                    "items": [
                        { "prompt_tokens": -1, "completion_tokens": 5 }
                    ]
                }
            }),
        ))]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let (window, _) =
            collect_log_window(&database, &data_key, &station, 0, 1).expect("log window");
        server.finish();

        assert_eq!(window.input_token_count, None);
        assert_eq!(window.output_token_count, None);
    }

    #[test]
    fn dashboard_total_requires_exact_raw_quota_match() {
        let target = NewApiDashboardTotalTarget {
            request_count: 1200,
            quota: 9_250_000,
        };
        assert!(dashboard_total_matches_target(
            &NewApiDashboardUsageWindow {
                request_count: Some(1200),
                quota: Some(9_250_000),
                ..Default::default()
            },
            target,
        ));
        assert!(!dashboard_total_matches_target(
            &NewApiDashboardUsageWindow {
                request_count: Some(1199),
                quota: Some(9_250_000),
                ..Default::default()
            },
            target,
        ));
        assert!(!dashboard_total_matches_target(
            &NewApiDashboardUsageWindow {
                request_count: Some(1200),
                quota: Some(9_249_999),
                ..Default::default()
            },
            target,
        ));
    }

    #[test]
    fn dashboard_total_searches_past_empty_recent_windows() {
        let server = TestHttpServer::sequence(vec![
            Some(json_response(200, json!({ "success": true, "data": [] }))),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 12, "quota": 900000, "token_used": 456789 }
                    ]
                }),
            )),
        ]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let (total, _) = collect_dashboard_usage_total_backwards(
            &database,
            &data_key,
            &station,
            unix_now_seconds(),
            Some(500000.0),
            Some(NewApiDashboardTotalTarget {
                request_count: 12,
                quota: 900000,
            }),
        )
        .expect("dashboard total");
        server.finish();

        assert_eq!(total.request_count, Some(12));
        assert_eq!(total.quota, Some(900000));
        assert_eq!(total.token_count, Some(456789));
    }

    #[test]
    fn dashboard_total_merge_propagates_missing_window_metrics() {
        let mut total = NewApiDashboardUsageWindow::default();
        total.add(NewApiDashboardUsageWindow {
            request_count: Some(2),
            token_count: Some(100),
            quota: Some(300000),
            consumption: Some(0.6),
        });
        total.add(NewApiDashboardUsageWindow {
            request_count: Some(3),
            token_count: None,
            quota: Some(400000),
            consumption: Some(0.8),
        });

        assert_eq!(total.request_count, Some(5));
        assert_eq!(total.quota, Some(700000));
        assert_eq!(total.consumption, Some(1.4));
        assert_eq!(total.token_count, None);
    }

    #[test]
    fn usage_merge_removes_unverified_self_usage_fields() {
        let mut data = json!({
            "request_count": 12,
            "today_request_count": 999,
            "today_consumption": 999.0,
            "today_token_count": 999,
            "total_token_count": 999,
            "today_base_consumption": 999.0,
            "total_base_consumption": 999.0
        });

        merge_usage_stats_into_balance_data(
            &mut data,
            NewApiUsageStats {
                today_request_count: Some(2),
                today_consumption: Some(0.75),
                today_token_count: Some(123),
                ..Default::default()
            },
        );

        assert_eq!(data["request_count"], 12);
        assert_eq!(data["today_request_count"], 2);
        assert_eq!(data["today_consumption"], 0.75);
        assert_eq!(data["today_token_count"], 123);
        assert!(data.get("total_token_count").is_none());
        assert!(data.get("today_base_consumption").is_none());
        assert!(data.get("total_base_consumption").is_none());
    }

    #[test]
    fn empty_usage_merge_still_removes_unverified_self_usage_fields() {
        let mut data = json!({
            "request_count": 12,
            "today_token_count": 999,
            "total_token_count": 999
        });

        merge_optional_usage_stats_into_balance_data(&mut data, None);

        assert_eq!(data["request_count"], 12);
        assert!(data.get("today_token_count").is_none());
        assert!(data.get("total_token_count").is_none());
    }

    #[test]
    fn integer_metrics_reject_fractional_values() {
        assert_eq!(
            numeric_i64_field(&json!({ "count": 1.4 }), &["count"]),
            None
        );
        assert_eq!(
            numeric_i64_field(&json!({ "count": 2.0 }), &["count"]),
            Some(2)
        );
        assert_eq!(
            numeric_i64_field(&json!({ "count": "3" }), &["count"]),
            Some(3)
        );
    }

    #[test]
    fn newapi_balance_does_not_treat_recent_dashboard_window_as_total() {
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
                        "used_quota": 1000000,
                        "request_count": 1200
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 2, "quota": 300000, "token_used": 111000000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": false,
                    "message": "时间跨度不能超过 1 个月"
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 3, "quota": 300000, "token_used": 111000000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 7, "quota": 700000, "token_used": 359000000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 300000, "rpm": 2, "tpm": 999999 }
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
                    "data": { "quota": 1000000, "rpm": 2, "tpm": 888888 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 10000,
                        "items": [
                            { "prompt_tokens": 111000000, "completion_tokens": 0 }
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
        assert_eq!(balance.today_token_count, Some(111000000));
        assert_eq!(balance.total_token_count, None);
        assert_eq!(balance.total_consumption, Some(2.0));
        let dashboard_requests = requests
            .iter()
            .filter(|request| request.starts_with("GET /api/data/self?"))
            .count();
        assert!(dashboard_requests >= 2);
    }

    #[test]
    fn newapi_balance_rejects_partial_dashboard_total_token_data() {
        let created_at = unix_now_seconds().saturating_sub(3600);
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
                        "used_quota": 1000000,
                        "created_at": created_at,
                        "request_count": 1200
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 2, "quota": 300000, "token_used": 111000000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 2, "quota": 300000, "token_used": 111000000 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 300000, "rpm": 2, "tpm": 999999 }
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
                    "data": { "quota": 300000, "rpm": 2, "tpm": 888888 }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": {
                        "page": 1,
                        "page_size": 100,
                        "total": 10000,
                        "items": [
                            { "prompt_tokens": 111000000, "completion_tokens": 0 }
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
        let balance = output.facts.balances.first().expect("balance fact");

        assert_eq!(output.status, "success");
        assert_eq!(balance.today_token_count, Some(111000000));
        assert_eq!(balance.total_request_count, Some(1200));
        assert_eq!(balance.total_consumption, Some(2.0));
        assert_eq!(balance.total_token_count, None);
    }

    #[test]
    fn newapi_balance_rejects_dashboard_total_when_self_used_quota_is_zero() {
        let created_at = unix_now_seconds().saturating_sub(3600);
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
                        "created_at": created_at,
                        "request_count": 0
                    }
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": []
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": []
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": []
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": [
                        { "count": 4, "quota": 500000, "token_used": 123456789 }
                    ]
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": { "quota": 0, "rpm": 0, "tpm": 0 }
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
                    "data": { "quota": 0, "rpm": 0, "tpm": 0 }
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
        assert_eq!(balance.total_consumption, Some(0.0));
        assert_eq!(balance.total_request_count, Some(0));
        assert_eq!(balance.total_token_count, None);
    }

    #[test]
    fn newapi_balance_leaves_tokens_unknown_when_logs_have_no_token_counts() {
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
                    "data": []
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": []
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
        assert_eq!(balance.today_token_count, None);
        assert_eq!(balance.total_request_count, Some(0));
        assert_eq!(balance.total_consumption, Some(0.0));
        assert_eq!(balance.total_input_token_count, None);
        assert_eq!(balance.total_output_token_count, None);
        assert_eq!(balance.total_token_count, None);
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
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": []
                }),
            )),
            Some(json_response(
                200,
                json!({
                    "success": true,
                    "data": []
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
    fn token_parsers_reject_nonstandard_wrappers_and_aliases() {
        assert!(remote_key_items(&json!({
            "tokens": [{ "id": 1, "name": "wrong-wrapper" }]
        }))
        .is_empty());
        assert!(remote_key_from_value(
            "station-1",
            &json!({
                "tokenId": 1,
                "keyName": "wrong-alias",
                "apiKey": "sk-abc**********7890"
            }),
            0,
        )
        .is_none());
        assert_eq!(
            full_key_from_reveal_payload(&json!({
                "data": { "key": "sk-nested-secret-value" }
            })),
            None,
        );
    }

    #[test]
    fn status_payload_rejects_failed_envelope() {
        let error = parse_status_payload(&json!({
            "success": false,
            "message": "status unavailable",
            "quota_per_unit": 500000
        }))
        .unwrap_err();

        assert_eq!(error, "status unavailable");
    }

    #[test]
    fn token_scan_rejects_missing_pagination_metadata() {
        let server = TestHttpServer::sequence(vec![Some(json_response(
            200,
            json!({
                "success": true,
                "data": {
                    "page": 1,
                    "page_size": 100,
                    "items": [
                        { "id": 101, "name": "primary", "key": "sk-abc**********7890" }
                    ]
                }
            }),
        ))]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, &server.base_url);
        persist_access_token_session(&database, &data_key, &station.id);

        let error = scan_remote_keys(&database, &data_key, &station.id).unwrap_err();
        server.finish();

        assert!(error.contains("pagination"));
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
