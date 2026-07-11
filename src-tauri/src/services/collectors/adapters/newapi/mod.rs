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
        url::{collector_base_urls, join_url},
    },
    database::AppDatabase,
    outbound::{agent_builder_for_proxy, resolve_proxy_config, ProxyConfig},
};

const COLLECTOR_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const NEWAPI_REMOTE_KEY_PAGE_SIZE: usize = 100;

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
    endpoint_result: Value,
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
            "endpointResults": [endpoint_result],
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
        let (status, _) = fetch_status(database, &station)?;
        let response = client::get_authenticated_json(
            database,
            data_key,
            &station,
            "/api/user/self",
            client::NewApiOperation::SelfInfo,
        )
        .map_err(newapi_request_error_message)?;
        return Ok(build_balance_output(
            station_id,
            &response.data,
            &status,
            response.endpoint_result,
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
    let urls = collector_base_urls(&station.base_url);
    let url = join_url(&urls.management_base_url, "/api/status");
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
                base_url,
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
                base_url: base_url.to_string(),
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
