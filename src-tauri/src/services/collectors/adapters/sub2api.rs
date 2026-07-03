use serde_json::{json, Value};

use crate::{
    models::station_keys::StationKey,
    services::{
        collectors::{
            adapters::{AdapterOutput, CollectorTask},
            facts::{CollectedBalanceFact, CollectorFacts},
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

fn routeable_keys_for_station(
    database: &AppDatabase,
    station_id: &str,
) -> Result<Vec<StationKey>, String> {
    database.list_station_keys(station_id.to_string()).map(|keys| {
        keys.into_iter()
            .filter(|key| key.enabled && key.api_key_present)
            .collect()
    })
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
        let api_key =
            match database.resolve_station_key_secret_with_data_key(data_key, &key.id) {
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
}
