use std::collections::HashMap;

use serde_json::{json, Value};

use crate::{
    models::station_keys::StationKey,
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
    let Some(access_token) = session.access_token else {
        return Ok(AdapterOutput {
            adapter: "sub2api".to_string(),
            task: CollectorTask::Groups,
            status: "manual_required".to_string(),
            summary_json: json!({
                "adapter": "sub2api",
                "task": "groups",
                "message": session.message.unwrap_or_else(|| "Sub2API 分组采集需要 access token。".to_string()),
            }),
            normalized_json: json!({ "groups": 0, "rates": 0 }),
            raw_json_redacted: None,
            error_code: Some("manual_session_required".to_string()),
            error_message: Some("Sub2API 分组采集需要 access token。".to_string()),
            facts: CollectorFacts::default(),
        });
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
