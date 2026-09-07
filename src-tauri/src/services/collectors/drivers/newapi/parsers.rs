use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::models::station_published_status::{
    PublishedMonitorFact, PublishedMonitorIdentityKind, PublishedMonitorSampleFact,
    PublishedSampleOutcome, PublishedStatusBatch, PublishedStatusCompleteness,
    PublishedStatusSourceState, MAX_PUBLISHED_STATUS_LATENCY_MS, MAX_PUBLISHED_STATUS_MONITORS,
    MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL, MAX_PUBLISHED_STATUS_TIMESTAMP_MS,
    NEWAPI_PERF_METRICS_SOURCE_KIND,
};
use crate::services::collectors::facts::{
    normalize_group_description, CollectedBalanceFact, CollectedGroupFact, CollectedRateFact,
    CollectorFacts, NORMALIZED_BALANCE_CURRENCY,
};
use crate::services::group_categories::infer_group_category;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewApiEnvelopeError {
    pub message: String,
}

pub(crate) fn envelope_data(payload: &Value) -> Result<&Value, NewApiEnvelopeError> {
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
pub(crate) struct NewApiStatus {
    pub system_name: Option<String>,
    pub quota_per_unit: Option<f64>,
    pub quota_display_type: Option<String>,
}

pub(crate) fn parse_status(data: &Value) -> NewApiStatus {
    let quota_per_unit = parse_optional_f64(data.get("quota_per_unit"));
    NewApiStatus {
        system_name: data
            .get("system_name")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        quota_per_unit: quota_per_unit.filter(|value| *value > 0.0),
        quota_display_type: data
            .get("quota_display_type")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    }
}

pub(crate) fn parse_balance_fact(
    station_id: &str,
    data: &Value,
    quota_per_unit: Option<f64>,
    credit_per_cny: f64,
) -> CollectedBalanceFact {
    let remaining_units = quota_per_unit
        .zip(parse_optional_f64(data.get("quota")))
        .map(|(quota_per_unit, value)| value / quota_per_unit);
    let used_units = quota_per_unit
        .zip(parse_optional_f64(data.get("used_quota")))
        .map(|(quota_per_unit, value)| value / quota_per_unit);
    let remaining = apply_credit_per_cny(remaining_units, credit_per_cny);
    let used = apply_credit_per_cny(used_units, credit_per_cny);
    CollectedBalanceFact {
        station_id: station_id.to_string(),
        station_key_id: None,
        scope: "station".to_string(),
        balance_kind: "account_balance".to_string(),
        value: remaining,
        used_value: used,
        total_value: apply_credit_per_cny(
            remaining_units
                .zip(used_units)
                .map(|(left, right)| left + right),
            credit_per_cny,
        ),
        today_request_count: parse_i64_field(data, &["today_request_count"]),
        total_request_count: parse_i64_field(data, &["request_count"]),
        today_consumption: parse_f64_field(data, &["today_consumption"]),
        total_consumption: used_units,
        today_base_consumption: parse_f64_field(data, &["today_base_consumption"]),
        total_base_consumption: parse_f64_field(data, &["total_base_consumption"]),
        today_token_count: parse_i64_field(data, &["today_token_count"]),
        total_token_count: parse_i64_field(data, &["total_token_count"]),
        today_input_token_count: parse_i64_field(data, &["today_input_token_count"]),
        today_output_token_count: parse_i64_field(data, &["today_output_token_count"]),
        total_input_token_count: parse_i64_field(data, &["total_input_token_count"]),
        total_output_token_count: parse_i64_field(data, &["total_output_token_count"]),
        account_concurrency_limit: parse_i64_field(
            data,
            &[
                "concurrency_limit",
                "concurrent_limit",
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
        .filter(|value| *value > 0),
        currency: NORMALIZED_BALANCE_CURRENCY.to_string(),
        credit_unit: quota_per_unit.map(|value| format!("newapi_quota_{value}")),
        status: if remaining.is_some_and(|value| value <= 0.0) {
            "depleted"
        } else {
            "normal"
        }
        .to_string(),
        source: "newapi_user_self".to_string(),
        confidence: if quota_per_unit.is_some() { 0.95 } else { 0.9 },
        collected_at: None,
        evidence_confidence: if remaining.is_some() {
            "confirmed"
        } else {
            "unknown"
        }
        .to_string(),
        spendability_authority: if remaining.is_some() {
            "authoritative"
        } else {
            "unknown"
        }
        .to_string(),
    }
}

pub(crate) fn parse_group_facts(station_id: &str, data: &Value) -> CollectorFacts {
    let mut facts = CollectorFacts::default();

    for (group_name, value) in data.as_object().into_iter().flatten() {
        let group_key_hash =
            stable_group_key_hash(station_id, "newapi", Some(group_name), group_name);
        let rate = parse_optional_f64(value.get("ratio"));
        let description = normalize_group_description(value.get("desc"));
        let raw_json_redacted = crate::services::secrets::mask::redact_value(value);
        let inferred_group_category = infer_group_category(group_name, Some(&raw_json_redacted));
        facts.groups.push(CollectedGroupFact {
            station_id: station_id.to_string(),
            group_id: Some(group_name.clone()),
            group_key_hash: group_key_hash.clone(),
            group_name: group_name.clone(),
            description: description.clone(),
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
            description,
            default_rate_multiplier: None,
            user_rate_multiplier: None,
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

/// Adapts NewAPI's request-derived performance response into the shared
/// published-status fact envelope. NewAPI's actual grain is
/// `(model_name, group, bucket_ts)`; each model/group pair is therefore kept as
/// a distinct monitor identity and each returned bucket becomes one sample.
pub(crate) fn parse_perf_status_batch(
    station_id: &str,
    endpoint_revision: i64,
    payloads: impl IntoIterator<Item = Value>,
    collected_at_ms: i64,
) -> Result<PublishedStatusBatch, String> {
    let mut monitors = Vec::new();
    let mut truncated = false;
    for payload in payloads {
        let data = envelope_data(&payload).map_err(|error| error.message)?;
        let model_name = data
            .get("model_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "NewAPI performance response is missing model_name".to_string())?;
        let groups = data
            .get("groups")
            .and_then(Value::as_array)
            .ok_or_else(|| "NewAPI performance response is missing groups".to_string())?;
        for group in groups {
            if monitors.len() >= MAX_PUBLISHED_STATUS_MONITORS {
                truncated = true;
                break;
            }
            let group_name = group
                .get("group")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "NewAPI performance group is missing group".to_string())?;
            let series = group
                .get("series")
                .and_then(Value::as_array)
                .ok_or_else(|| "NewAPI performance group is missing series".to_string())?;
            let mut samples = series
                .iter()
                .filter_map(|point| {
                    let ts = point.get("ts").and_then(as_i64)?;
                    let checked_at_ms = ts.checked_mul(1_000)?;
                    if !(0..=MAX_PUBLISHED_STATUS_TIMESTAMP_MS).contains(&checked_at_ms) {
                        return None;
                    }
                    let success_rate = point.get("success_rate").and_then(as_f64);
                    let outcome = outcome_from_success_rate(success_rate);
                    Some(PublishedMonitorSampleFact {
                        model: model_name.to_string(),
                        outcome,
                        source_status: "success_rate_derived".to_string(),
                        latency_ms: positive_i64(point.get("avg_latency_ms").and_then(as_i64)),
                        ping_latency_ms: None,
                        ttft_ms: metric_i64(point.get("avg_ttft_ms").and_then(as_i64)),
                        tps: metric_f64(point.get("avg_tps").and_then(as_f64)),
                        success_rate_percent: bounded_success_rate(success_rate),
                        checked_at_ms,
                        safe_message: None,
                    })
                })
                .collect::<Vec<_>>();
            // The shared published-status contract retains the newest 60
            // buckets per monitor. NewAPI can return a minute-level series
            // for a 24-hour window (well above that bound), so trim oldest
            // buckets before validating the batch instead of rejecting an
            // otherwise valid response.
            samples.sort_by_key(|sample| sample.checked_at_ms);
            if samples.len() > MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL {
                let discard = samples.len() - MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL;
                samples.drain(0..discard);
            }
            let current_latency_ms = positive_i64(group.get("avg_latency_ms").and_then(as_i64));
            let current_ttft_ms = metric_i64(group.get("avg_ttft_ms").and_then(as_i64));
            let current_tps = metric_f64(group.get("avg_tps").and_then(as_f64));
            let current_success_rate_percent =
                bounded_success_rate(group.get("success_rate").and_then(as_f64));
            let latest_checked_at_ms = samples.iter().map(|sample| sample.checked_at_ms).max();
            let current_outcome =
                outcome_from_success_rate(group.get("success_rate").and_then(as_f64));
            let identity_seed = format!("{station_id}\n{model_name}\n{group_name}");
            let upstream_monitor_id = format!(
                "newapi:{}",
                format!("{:x}", Sha256::digest(identity_seed.as_bytes()))
            );
            monitors.push(PublishedMonitorFact {
                upstream_monitor_id,
                identity_kind: PublishedMonitorIdentityKind::DerivedFallback,
                name: format!("{model_name} · {group_name}"),
                provider: "newapi".to_string(),
                group_name: Some(group_name.to_string()),
                primary_model: model_name.to_string(),
                extra_models: Vec::new(),
                current_outcome,
                source_status: "success_rate_derived".to_string(),
                current_latency_ms,
                current_ping_latency_ms: None,
                current_ttft_ms,
                current_tps,
                current_success_rate_percent,
                upstream_checked_at_ms: latest_checked_at_ms,
                samples,
            });
        }
    }
    monitors.sort_by(|left, right| {
        left.primary_model
            .cmp(&right.primary_model)
            .then_with(|| left.group_name.cmp(&right.group_name))
    });
    let source_state = if monitors.is_empty() {
        PublishedStatusSourceState::Empty
    } else if truncated {
        PublishedStatusSourceState::Degraded
    } else {
        PublishedStatusSourceState::Available
    };
    let batch = PublishedStatusBatch {
        station_id: station_id.to_string(),
        endpoint_revision,
        source_kind: NEWAPI_PERF_METRICS_SOURCE_KIND.to_string(),
        source_state,
        completeness: if truncated {
            PublishedStatusCompleteness::Partial
        } else {
            PublishedStatusCompleteness::Complete
        },
        monitors,
        collected_at_ms,
        safe_error_kind: None,
    };
    batch.validate().map_err(|error| error.to_string())?;
    Ok(batch)
}

fn as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|v| i64::try_from(v).ok()))
}

fn as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

fn positive_i64(value: Option<i64>) -> Option<i64> {
    value.filter(|value| (1..=MAX_PUBLISHED_STATUS_LATENCY_MS).contains(value))
}

fn metric_i64(value: Option<i64>) -> Option<i64> {
    // NewAPI's aggregate helpers return zero when the metric has no samples
    // (for example, non-streaming requests have no TTFT). Treat that as
    // missing rather than presenting a fabricated zero measurement.
    value.filter(|value| (1..=MAX_PUBLISHED_STATUS_LATENCY_MS).contains(value))
}

fn metric_f64(value: Option<f64>) -> Option<f64> {
    // Likewise, avgTps returns zero when no output-token denominator exists.
    value.filter(|value| value.is_finite() && (f64::EPSILON..=1_000_000.0).contains(value))
}

fn bounded_success_rate(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && (0.0..=100.0).contains(value))
}

fn outcome_from_success_rate(rate: Option<f64>) -> PublishedSampleOutcome {
    match rate {
        Some(value) if value >= 100.0 => PublishedSampleOutcome::Available,
        Some(value) if value > 0.0 => PublishedSampleOutcome::Degraded,
        Some(_) => PublishedSampleOutcome::Unavailable,
        None => PublishedSampleOutcome::Unknown,
    }
}

fn apply_credit_per_cny(value: Option<f64>, credit_per_cny: f64) -> Option<f64> {
    value.map(|value| {
        let divisor = if credit_per_cny.is_finite() && credit_per_cny > 0.0 {
            credit_per_cny
        } else {
            1.0
        };
        value / divisor
    })
}

fn parse_optional_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
            .filter(|value| value.is_finite())
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
            .or_else(|| {
                value.as_f64().and_then(|value| {
                    (value.is_finite()
                        && value.fract() == 0.0
                        && value >= i64::MIN as f64
                        && value <= i64::MAX as f64)
                        .then_some(value as i64)
                })
            })
            .or_else(|| value.as_str()?.trim().parse::<i64>().ok())
    })
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
    use serde_json::json;

    #[test]
    fn envelope_requires_success_and_returns_data() {
        let payload = json!({"success": true, "message": "", "data": {"quota": 750000}});
        assert_eq!(envelope_data(&payload).expect("data")["quota"], 750000);
        let failed = json!({"success": false, "message": "not logged in", "data": null});
        assert_eq!(envelope_data(&failed).unwrap_err().message, "not logged in");
    }

    #[test]
    fn performance_response_preserves_model_group_and_bucket_dimensions() {
        let payload = json!({
            "success": true,
            "data": {
                "model_name": "gpt-test",
                "groups": [{
                    "group": "vip",
                    "avg_latency_ms": 1200,
                    "avg_ttft_ms": 400,
                    "success_rate": 95.0,
                    "avg_tps": 42.5,
                    "series": [
                        {"ts": 1700000000, "avg_latency_ms": 1000, "success_rate": 100.0, "avg_ttft_ms": 300, "avg_tps": 40.0},
                        {"ts": 1700003600, "avg_latency_ms": 1400, "success_rate": 90.0, "avg_ttft_ms": 500, "avg_tps": 45.0}
                    ]
                }]
            }
        });
        let batch = parse_perf_status_batch("station", 1, [payload], 1_700_000_400_000)
            .expect("performance payload parses");
        assert_eq!(batch.source_kind, NEWAPI_PERF_METRICS_SOURCE_KIND);
        assert_eq!(batch.monitors.len(), 1);
        let monitor = &batch.monitors[0];
        assert_eq!(monitor.primary_model, "gpt-test");
        assert_eq!(monitor.group_name.as_deref(), Some("vip"));
        assert_eq!(monitor.samples.len(), 2);
        assert_eq!(monitor.samples[0].checked_at_ms, 1_700_000_000_000);
        assert_eq!(monitor.samples[1].outcome, PublishedSampleOutcome::Degraded);
        assert_eq!(monitor.current_latency_ms, Some(1200));
        assert_eq!(monitor.current_ttft_ms, Some(400));
        assert_eq!(monitor.current_tps, Some(42.5));
        assert_eq!(monitor.current_success_rate_percent, Some(95.0));
        assert_eq!(monitor.current_ping_latency_ms, None);
        assert_eq!(monitor.samples[1].ttft_ms, Some(500));
        assert_eq!(monitor.samples[1].tps, Some(45.0));
        assert_eq!(monitor.samples[1].success_rate_percent, Some(90.0));
    }

    #[test]
    fn performance_series_retains_only_newest_shared_history_window() {
        let series = (0..65)
            .map(|index| {
                json!({
                    "ts": 1_700_000_000i64 + index,
                    "avg_latency_ms": 100,
                    "success_rate": 100.0,
                    "avg_ttft_ms": 20,
                    "avg_tps": 10.0
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "success": true,
            "data": {
                "model_name": "gpt-test",
                "groups": [{"group": "vip", "series": series}]
            }
        });

        let batch = parse_perf_status_batch("station", 1, [payload], 1_700_000_000_000)
            .expect("performance payload parses");
        let samples = &batch.monitors[0].samples;
        assert_eq!(samples.len(), MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL);
        assert_eq!(
            samples.first().map(|sample| sample.checked_at_ms),
            Some(1_700_000_005_000)
        );
        assert_eq!(
            samples.last().map(|sample| sample.checked_at_ms),
            Some(1_700_000_064_000)
        );
    }

    #[test]
    fn balance_uses_runtime_quota_per_unit() {
        let fact = parse_balance_fact(
            "station-1",
            &json!({"quota": 750000, "used_quota": 250000}),
            Some(250000.0),
            1.0,
        );
        assert_eq!(fact.value, Some(3.0));
        assert_eq!(fact.used_value, Some(1.0));
        assert_eq!(fact.total_value, Some(4.0));
        assert_eq!(fact.confidence, 0.95);
    }

    #[test]
    fn negative_remaining_quota_is_depleted() {
        let fact = parse_balance_fact(
            "station-1",
            &json!({"quota": -1.0, "used_quota": 2.0}),
            Some(1.0),
            1.0,
        );

        assert_eq!(fact.value, Some(-1.0));
        assert_eq!(fact.status, "depleted");
    }

    #[test]
    fn balance_quota_converts_to_usd_units() {
        let fact = parse_balance_fact(
            "station-1",
            &json!({
                "quota": 1000000.0,
                "used_quota": 500000.0,
                "group": "default"
            }),
            Some(500000.0),
            1.0,
        );

        assert_eq!(fact.value, Some(2.0));
        assert_eq!(fact.used_value, Some(1.0));
        assert_eq!(fact.total_value, Some(3.0));
        assert_eq!(fact.currency, "USD");
        assert_eq!(fact.source, "newapi_user_self");
    }

    #[test]
    fn balance_divides_quota_units_by_credit_per_cny() {
        let fact = parse_balance_fact(
            "station-1",
            &json!({"quota": 1000000.0, "used_quota": 500000.0}),
            Some(500000.0),
            10.0,
        );

        assert_eq!(fact.value, Some(2.0 / 10.0));
        assert_eq!(fact.used_value, Some(1.0 / 10.0));
        assert_eq!(fact.total_value, Some(3.0 / 10.0));
        assert_eq!(fact.total_consumption, Some(1.0));
    }

    #[test]
    fn invalid_credit_per_cny_keeps_quota_unit_balance() {
        let fact = parse_balance_fact(
            "station-1",
            &json!({"quota": 1000000.0, "used_quota": 500000.0}),
            Some(500000.0),
            0.0,
        );

        assert_eq!(fact.value, Some(2.0));
        assert_eq!(fact.used_value, Some(1.0));
        assert_eq!(fact.total_value, Some(3.0));
    }

    #[test]
    fn missing_quota_per_unit_does_not_guess_converted_balance() {
        let status = parse_status(&json!({}));
        assert_eq!(status.quota_per_unit, None);
        let fact = parse_balance_fact(
            "station-1",
            &json!({
                "quota": 1000000,
                "used_quota": 500000,
                "request_count": 12
            }),
            status.quota_per_unit,
            10.0,
        );
        assert_eq!(fact.value, None);
        assert_eq!(fact.used_value, None);
        assert_eq!(fact.total_value, None);
        assert_eq!(fact.total_consumption, None);
        assert_eq!(fact.total_request_count, Some(12));
        assert_eq!(fact.credit_unit, None);
    }

    #[test]
    fn status_rejects_non_finite_quota_per_unit() {
        assert_eq!(
            parse_status(&json!({"quota_per_unit": "NaN"})).quota_per_unit,
            None,
        );
        assert_eq!(
            parse_status(&json!({"quota_per_unit": "inf"})).quota_per_unit,
            None,
        );
    }

    #[test]
    fn balance_rejects_fractional_request_count() {
        let fact = parse_balance_fact(
            "station-1",
            &json!({"request_count": 1.4}),
            Some(500000.0),
            1.0,
        );

        assert_eq!(fact.total_request_count, None);
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
                "today_consumption": 1.25,
                "total_base_consumption": 39.75,
                "today_base_consumption": 2.5,
                "total_token_count": 987654,
                "today_token_count": 43210
            }),
            Some(250000.0),
            1.0,
        );

        assert_eq!(fact.today_request_count, Some(34));
        assert_eq!(fact.total_request_count, Some(1200));
        assert_eq!(fact.today_consumption, Some(1.25));
        assert_eq!(fact.total_consumption, Some(1.0));
        assert_eq!(fact.today_base_consumption, Some(2.5));
        assert_eq!(fact.total_base_consumption, Some(39.75));
        assert_eq!(fact.today_token_count, Some(43210));
        assert_eq!(fact.total_token_count, Some(987654));
        assert_eq!(fact.account_concurrency_limit, None);
    }

    #[test]
    fn balance_captures_account_concurrency_limit() {
        let fact = parse_balance_fact(
            "station-1",
            &json!({"quota": 750000, "concurrency_limit": 8}),
            Some(250000.0),
            1.0,
        );

        assert_eq!(fact.account_concurrency_limit, Some(8));
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
        assert!(facts.groups.iter().any(|group| {
            group.group_name == "default" && group.description.as_deref() == Some("Default")
        }));
        assert!(facts
            .groups
            .iter()
            .any(|group| group.group_name == "default"));
        assert!(facts
            .rates
            .iter()
            .any(|rate| { rate.group_name == "auto" && rate.effective_rate_multiplier.is_none() }));
        let default_rate = facts
            .rates
            .iter()
            .find(|rate| rate.group_name == "default")
            .expect("default rate");
        assert_eq!(default_rate.default_rate_multiplier, None);
        assert_eq!(default_rate.user_rate_multiplier, None);
        assert_eq!(default_rate.effective_rate_multiplier, Some(1.0));
        assert_eq!(default_rate.description.as_deref(), Some("Default"));
    }

    #[test]
    fn group_description_rejects_missing_non_string_and_oversized_values() {
        let facts = parse_group_facts(
            "station-1",
            &json!({
                "missing": {"ratio": 1.0},
                "number": {"desc": 42, "ratio": 1.0},
                "oversized": {"desc": "x".repeat(1025), "ratio": 1.0}
            }),
        );

        assert!(facts.groups.iter().all(|group| group.description.is_none()));
        assert!(facts.rates.iter().all(|rate| rate.description.is_none()));
    }

    #[test]
    fn group_map_parses_list_and_rate_fields() {
        let facts = parse_group_facts(
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
}
