use std::collections::HashSet;

use serde_json::Value;

use crate::services::collectors::facts::{
    CollectedBalanceFact, CollectedGroupFact, CollectedModelFact, CollectedRateFact, CollectorFacts,
};
use crate::services::group_categories::infer_group_category;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NewApiEnvelopeError {
    pub message: String,
}

pub(super) fn envelope_data(payload: &Value) -> Result<&Value, NewApiEnvelopeError> {
    if payload.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(NewApiEnvelopeError {
            message: payload
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("NewAPI request failed")
                .to_string(),
        });
    }
    payload.get("data").ok_or_else(|| NewApiEnvelopeError {
        message: "NewAPI response is missing data".to_string(),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NewApiStatus {
    pub system_name: Option<String>,
    pub quota_per_unit: f64,
    pub quota_display_type: Option<String>,
    pub used_fallback: bool,
}

pub(super) fn parse_status(data: &Value) -> NewApiStatus {
    let quota_per_unit = parse_optional_f64(data.get("quota_per_unit"));
    NewApiStatus {
        system_name: data
            .get("system_name")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        quota_per_unit: quota_per_unit
            .filter(|value| *value > 0.0)
            .unwrap_or(500000.0),
        quota_display_type: data
            .get("quota_display_type")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        used_fallback: quota_per_unit.map_or(true, |value| value <= 0.0),
    }
}

pub(super) fn parse_balance_fact(
    station_id: &str,
    data: &Value,
    quota_per_unit: f64,
    quota_per_unit_fallback: bool,
) -> CollectedBalanceFact {
    let remaining = parse_optional_f64(data.get("quota")).map(|value| value / quota_per_unit);
    let used = parse_optional_f64(data.get("used_quota")).map(|value| value / quota_per_unit);
    CollectedBalanceFact {
        station_id: station_id.to_string(),
        station_key_id: None,
        scope: "station".to_string(),
        value: remaining,
        used_value: used,
        total_value: remaining.zip(used).map(|(left, right)| left + right),
        today_request_count: parse_i64_field(
            data,
            &["today_request_count", "today_requests", "todayRequestCount", "todayRequests"],
        ),
        total_request_count: parse_i64_field(
            data,
            &[
                "total_request_count",
                "request_count",
                "totalRequests",
                "requestCount",
                "requests",
            ],
        ),
        today_consumption: parse_f64_field(
            data,
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
            data,
            &[
                "total_consumption",
                "used_amount",
                "totalUsedAmount",
                "totalConsumption",
                "consumption",
                "cost",
            ],
        ),
        today_token_count: parse_i64_field(
            data,
            &["today_token_count", "today_tokens", "todayTokenCount", "todayTokens"],
        ),
        total_token_count: parse_i64_field(
            data,
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
            data,
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
            data,
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
            data,
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
            data,
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
        currency: "USD".to_string(),
        credit_unit: Some(format!("newapi_quota_{quota_per_unit}")),
        status: if remaining == Some(0.0) {
            "depleted"
        } else {
            "normal"
        }
        .to_string(),
        source: "newapi_user_self".to_string(),
        confidence: if quota_per_unit_fallback { 0.75 } else { 0.95 },
        collected_at: None,
    }
}

pub(super) fn parse_group_facts(station_id: &str, data: &Value) -> CollectorFacts {
    let mut facts = CollectorFacts::default();

    for (group_name, value) in data.as_object().into_iter().flatten() {
        let group_key_hash =
            super::stable_group_key_hash(station_id, "newapi", Some(group_name), group_name);
        let rate = parse_optional_f64(value.get("ratio"));
        let raw_json_redacted = crate::services::secrets::mask::redact_value(value);
        let inferred_group_category = infer_group_category(group_name, Some(&raw_json_redacted));
        facts.groups.push(CollectedGroupFact {
            station_id: station_id.to_string(),
            group_id: Some(group_name.clone()),
            group_key_hash: group_key_hash.clone(),
            group_name: group_name.clone(),
            visibility: "available".to_string(),
            inferred_group_category: Some(inferred_group_category.clone()),
            source: "newapi_user_groups".to_string(),
            confidence: 0.9,
            raw_json_redacted: Some(raw_json_redacted),
        });
        facts.rates.push(CollectedRateFact {
            station_id: station_id.to_string(),
            station_key_id: None,
            group_id: Some(group_name.clone()),
            group_key_hash,
            group_name: group_name.clone(),
            default_rate_multiplier: rate,
            user_rate_multiplier: rate,
            effective_rate_multiplier: rate,
            inferred_group_category: Some(inferred_group_category),
            source: "newapi_user_groups".to_string(),
            confidence: if rate.is_some() { 0.9 } else { 0.65 },
            checked_at: None,
            raw_json_redacted: None,
        });
    }

    facts
}

pub(super) fn parse_models(station_id: &str, data: &Value) -> Vec<CollectedModelFact> {
    let mut seen = HashSet::new();
    data.as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let name = value
                .as_str()
                .or_else(|| {
                    ["id", "name", "model"]
                        .into_iter()
                        .find_map(|key| value.get(key).and_then(Value::as_str))
                })?
                .trim();
            if name.is_empty() || !seen.insert(name.to_string()) {
                return None;
            }
            Some(CollectedModelFact {
                station_id: station_id.to_string(),
                model: name.to_string(),
                available: true,
                source: "newapi_user_models".to_string(),
                confidence: 0.9,
            })
        })
        .collect()
}

fn parse_optional_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
    })
}

fn parse_f64_field(payload: &Value, names: &[&str]) -> Option<f64> {
    names
        .iter()
        .find_map(|name| parse_optional_f64(payload.get(*name)))
}

fn parse_i64_field(payload: &Value, names: &[&str]) -> Option<i64> {
    names
        .iter()
        .find_map(|name| parse_optional_i64(payload.get(*name)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn envelope_requires_success_and_returns_data() {
        let payload = json!({"success": true, "message": "", "data": {"quota": 750000}});
        assert_eq!(envelope_data(&payload).expect("data")["quota"], 750000);
        let failed = json!({"success": false, "message": "not logged in", "data": null});
        assert_eq!(envelope_data(&failed).unwrap_err().message, "not logged in");
    }

    #[test]
    fn balance_uses_runtime_quota_per_unit() {
        let fact = parse_balance_fact(
            "station-1",
            &json!({"quota": 750000, "used_quota": 250000}),
            250000.0,
            false,
        );
        assert_eq!(fact.value, Some(3.0));
        assert_eq!(fact.used_value, Some(1.0));
        assert_eq!(fact.total_value, Some(4.0));
        assert_eq!(fact.confidence, 0.95);
    }

    #[test]
    fn balance_captures_station_usage_totals() {
        let fact = parse_balance_fact(
            "station-1",
            &json!({
                "quota": 750000,
                "used_quota": 250000,
                "request_count": 1200,
                "today_request_count": 34,
                "used_amount": 19.875,
                "today_used_amount": 1.25,
                "total_tokens": 987654,
                "today_tokens": 43210
            }),
            250000.0,
            false,
        );

        assert_eq!(fact.today_request_count, Some(34));
        assert_eq!(fact.total_request_count, Some(1200));
        assert_eq!(fact.today_consumption, Some(1.25));
        assert_eq!(fact.total_consumption, Some(19.875));
        assert_eq!(fact.today_token_count, Some(43210));
        assert_eq!(fact.total_token_count, Some(987654));
    }

    #[test]
    fn group_map_preserves_names_and_non_numeric_rates() {
        let facts = parse_group_facts(
            "station-1",
            &json!({
                "default": {"desc": "Default", "ratio": 1.0},
                "auto": {"desc": "Automatic", "ratio": "auto"}
            }),
        );
        assert_eq!(facts.groups.len(), 2);
        assert!(facts
            .groups
            .iter()
            .any(|group| group.group_name == "default"));
        assert!(facts
            .rates
            .iter()
            .any(|rate| { rate.group_name == "auto" && rate.effective_rate_multiplier.is_none() }));
    }

    #[test]
    fn models_accept_strings_and_objects_without_duplicates() {
        let models = parse_models(
            "station-1",
            &json!(["gpt-4.1-mini", {"id": "claude-sonnet"}, {"name": "gpt-4.1-mini"}]),
        );
        assert_eq!(
            models
                .iter()
                .map(|model| model.model.as_str())
                .collect::<Vec<_>>(),
            vec!["gpt-4.1-mini", "claude-sonnet",]
        );
    }
}
