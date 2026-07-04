use std::collections::HashMap;

use serde_json::{json, Value};

use crate::{
    models::{credentials::UpdateStationSessionInput, station_keys::StationKey},
    services::{
        collectors::{
            adapters::{AdapterOutput, CollectorTask},
            facts::{CollectedBalanceFact, CollectedGroupFact, CollectedRateFact, CollectorFacts},
            url::{collector_base_urls, join_url},
        },
        database::AppDatabase,
    },
};

pub fn parse_usage_balance(
    station_id: &str,
    station_key_id: Option<String>,
    payload: &Value,
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
        value: remaining,
        used_value: used,
        total_value: total,
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

        facts.groups.push(CollectedGroupFact {
            station_id: station_id.to_string(),
            group_id: group_id.clone(),
            group_key_hash: group_key_hash.clone(),
            group_name: group.group_name.clone(),
            visibility: "available".to_string(),
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
            source: "sub2api_groups_rates".to_string(),
            confidence: if effective.is_some() { 0.9 } else { 0.6 },
            checked_at: None,
            raw_json_redacted: None,
        });
    }

    facts
}

fn collect_available_groups(payload: &Value) -> Vec<AvailableGroup> {
    group_items(payload)
        .into_iter()
        .filter_map(|value| {
            let group_id = string_field(value, &["id", "group_id", "groupId", "key"]);
            let group_name = string_field(value, &["name", "group_name", "groupName", "label"])
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

            for key in ["data", "rates", "groups", "items", "list"] {
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
    if let Some(items) = payload.as_array() {
        return items.iter().collect();
    }

    for key in [
        "data",
        "groups",
        "available_groups",
        "availableGroups",
        "items",
        "list",
    ] {
        if let Some(value) = payload.get(key) {
            if let Some(items) = value.as_array() {
                return items.iter().collect();
            }
        }
    }

    Vec::new()
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(*key))
        .find_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
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
    let session = database.resolve_station_session_with_data_key(
        station_id.to_string(),
        data_key,
        crate::services::database::now_millis_for_services() as i64,
    )?;
    let access_token = match session.access_token {
        Some(access_token) => access_token,
        None => match login_and_store_access_token(database, data_key, &station)? {
            Some(access_token) => access_token,
            None => {
                return Ok(manual_session_required_output(session.message));
            }
        },
    };

    let urls = collector_base_urls(&station.base_url);
    let available_url = join_url(&urls.management_base_url, "/api/v1/groups/available");
    let rates_url = join_url(&urls.management_base_url, "/api/v1/groups/rates");
    let available_result = fetch_json_with_bearer(&available_url, &access_token);
    let rates_result = fetch_json_with_bearer(&rates_url, &access_token);
    let available_payload = available_result.payload.clone().unwrap_or(Value::Null);
    let rates_payload = rates_result.payload.clone().unwrap_or(Value::Null);
    let mut facts = parse_group_rate_facts(&station.id, &available_payload, &rates_payload);
    let keys = routeable_keys_for_station(database, station_id)?;
    add_single_group_key_bindings(&mut facts, &keys);

    let endpoint_results = json!([
        available_result.to_redacted_json(),
        rates_result.to_redacted_json(),
    ]);
    let success_count = [available_result.ok, rates_result.ok]
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
    let login = crate::services::collectors::sub2api::login_access_token(
        &station.base_url,
        username,
        &password,
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

#[derive(Debug, Clone)]
struct EndpointJsonResult {
    url: String,
    status: Option<u16>,
    ok: bool,
    duration_ms: i64,
    payload: Option<Value>,
    error_message: Option<String>,
}

impl EndpointJsonResult {
    fn to_redacted_json(&self) -> Value {
        json!({
            "url": self.url,
            "status": self.status,
            "ok": self.ok,
            "durationMs": self.duration_ms,
            "errorMessage": self.error_message,
        })
    }
}

fn fetch_json_with_bearer(url: &str, access_token: &str) -> EndpointJsonResult {
    let started = std::time::Instant::now();
    let response = match ureq::get(url)
        .set("Authorization", &format!("Bearer {access_token}"))
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => {
            return EndpointJsonResult {
                url: url.to_string(),
                status: None,
                ok: false,
                duration_ms: started.elapsed().as_millis() as i64,
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
        url: url.to_string(),
        status: Some(status),
        ok: (200..400).contains(&status),
        duration_ms: started.elapsed().as_millis() as i64,
        payload,
        error_message: None,
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
    let keys = routeable_keys_for_station(database, station_id)?;
    let urls = collector_base_urls(&station.base_url);
    let url = join_url(&urls.upstream_api_base_url, "/usage");
    let mut facts = CollectorFacts::default();
    let mut endpoint_results = Vec::new();

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
        let started = std::time::Instant::now();
        let response = match ureq::get(&url)
            .set("Authorization", &format!("Bearer {api_key}"))
            .call()
        {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => response,
            Err(error) => {
                endpoint_results.push(json!({
                    "endpoint": url,
                    "stationKeyId": key.id,
                    "status": "network_error",
                    "message": crate::services::secrets::mask::redact_text(&error.to_string()),
                    "durationMs": started.elapsed().as_millis() as i64,
                }));
                continue;
            }
        };
        let status = response.status();
        let text = response.into_string().unwrap_or_default();
        let parsed = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
        endpoint_results.push(json!({
            "endpoint": url,
            "stationKeyId": key.id,
            "status": status,
            "durationMs": started.elapsed().as_millis() as i64,
            "ok": (200..400).contains(&status),
        }));
        if (200..400).contains(&status) {
            facts
                .balances
                .push(parse_usage_balance(&station.id, Some(key.id), &parsed));
        }
    }
    if facts.balances.is_empty() {
        if let Some(balance) =
            collect_account_balance_fallback(database, data_key, &station, &mut endpoint_results)?
        {
            facts.balances.push(balance);
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
    endpoint_results: &mut Vec<Value>,
) -> Result<Option<CollectedBalanceFact>, String> {
    let session = database.resolve_station_session_with_data_key(
        station.id.clone(),
        data_key,
        crate::services::database::now_millis_for_services() as i64,
    )?;
    let access_token = match session.access_token {
        Some(access_token) => access_token,
        None => match login_and_store_access_token(database, data_key, station)? {
            Some(access_token) => access_token,
            None => return Ok(None),
        },
    };

    let urls = collector_base_urls(&station.base_url);
    for path in ["/api/v1/user/profile", "/api/v1/auth/me"] {
        let url = join_url(&urls.management_base_url, path);
        let result = fetch_json_with_bearer(&url, &access_token);
        let payload = result.payload.clone().unwrap_or(Value::Null);
        endpoint_results.push(result.to_redacted_json());
        if !result.ok {
            continue;
        }
        if let Some(balance) = parse_account_balance(&station.id, &payload) {
            return Ok(Some(balance));
        }
    }

    Ok(None)
}

fn parse_account_balance(station_id: &str, payload: &Value) -> Option<CollectedBalanceFact> {
    let value = payload
        .pointer("/data/balance")
        .and_then(Value::as_f64)
        .or_else(|| payload.get("balance").and_then(Value::as_f64))
        .or_else(|| payload.pointer("/data/quota/remaining").and_then(Value::as_f64))
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
        value: Some(value),
        used_value: used,
        total_value: total,
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
        models::{credentials::UpdateStationCredentialsInput, stations::CreateStationInput},
        services::{database::AppDatabase, secrets::crypto::generate_data_key},
    };
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
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
        let fact = parse_usage_balance("station-1", None, &json!({ "remaining": 0.0 }));
        assert_eq!(fact.status, "depleted");
    }

    #[test]
    fn sub2api_groups_rates_join_by_group_id() {
        let available = json!({
            "data": [
                { "id": "default", "name": "Default", "rate_multiplier": 1.0 },
                { "id": "pro", "name": "Pro", "rate_multiplier": 1.5 }
            ]
        });
        let rates = json!({
            "data": {
                "default": 0.8,
                "pro": 1.2
            }
        });

        let facts = parse_group_rate_facts("station-1", &available, &rates);

        assert!(facts
            .groups
            .iter()
            .any(|group| group.group_name == "Default"));
        assert!(facts.rates.iter().any(|rate| {
            rate.group_name == "Pro" && rate.effective_rate_multiplier == Some(1.2)
        }));
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
                base_url: server.base_url,
                api_key: "sk-station".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
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
                    base_url: server.base_url,
                    api_key: "sk-invalid-for-usage".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
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

    struct TestGroupServer {
        base_url: String,
    }

    struct TestBalanceFallbackServer {
        base_url: String,
    }

    impl TestGroupServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            thread::spawn(move || {
                for stream in listener.incoming().take(4).flatten() {
                    handle_group_test_request(stream);
                }
            });
            Self { base_url }
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
