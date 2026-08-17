use std::collections::{BTreeSet, HashSet};

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::models::{
    secrets::redact_text,
    station_published_status::{
        derived_monitor_identity, PublishedMonitorFact, PublishedMonitorIdentityKind,
        PublishedMonitorSampleFact, PublishedSampleOutcome, PublishedStatusBatch,
        PublishedStatusCompleteness, PublishedStatusSourceState, MAX_PUBLISHED_STATUS_EXTRA_MODELS,
        MAX_PUBLISHED_STATUS_GROUP_BYTES, MAX_PUBLISHED_STATUS_LATENCY_MS,
        MAX_PUBLISHED_STATUS_MODEL_BYTES, MAX_PUBLISHED_STATUS_MONITORS,
        MAX_PUBLISHED_STATUS_MONITOR_ID_BYTES, MAX_PUBLISHED_STATUS_NAME_BYTES,
        MAX_PUBLISHED_STATUS_PROVIDER_BYTES, MAX_PUBLISHED_STATUS_SAFE_MESSAGE_BYTES,
        MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL, MAX_PUBLISHED_STATUS_SOURCE_STATUS_BYTES,
        MAX_PUBLISHED_STATUS_STATION_ID_BYTES, MAX_PUBLISHED_STATUS_TIMELINE_INPUT,
        MAX_PUBLISHED_STATUS_TIMESTAMP_MS, STATION_PUBLISHED_STATUS_SOURCE_KIND,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishedStatusParseErrorKind {
    InvalidEnvelope,
    InvalidStation,
    TooManyMonitors,
    NoValidMonitors,
    InvalidCanonicalFact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedStatusParseError {
    pub kind: PublishedStatusParseErrorKind,
    safe_detail: Option<String>,
}

impl std::fmt::Display for PublishedStatusParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self.kind {
            PublishedStatusParseErrorKind::InvalidEnvelope => {
                "Sub2API channel monitor response did not match the supported envelope"
            }
            PublishedStatusParseErrorKind::InvalidStation => "published-status input is invalid",
            PublishedStatusParseErrorKind::TooManyMonitors => {
                "Sub2API channel monitor response exceeds the supported monitor limit"
            }
            PublishedStatusParseErrorKind::NoValidMonitors => {
                "Sub2API channel monitor response contains no valid monitor records"
            }
            PublishedStatusParseErrorKind::InvalidCanonicalFact => {
                "Sub2API channel monitor response could not be normalized safely"
            }
        };
        formatter.write_str(message)?;
        if let Some(detail) = &self.safe_detail {
            write!(formatter, "; {detail}")?;
        }
        Ok(())
    }
}

impl std::error::Error for PublishedStatusParseError {}

/// Maps the status vocabulary used by released Sub2API deployments. Unknown
/// and null statuses intentionally remain unknown rather than being
/// interpreted as a local or upstream failure.
pub fn map_sub2api_published_status(value: Option<&str>) -> PublishedSampleOutcome {
    match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("healthy" | "operational" | "available" | "success" | "ok") => {
            PublishedSampleOutcome::Available
        }
        Some("degraded" | "warning") => PublishedSampleOutcome::Degraded,
        Some("failed" | "error" | "unavailable" | "down") => PublishedSampleOutcome::Unavailable,
        _ => PublishedSampleOutcome::Unknown,
    }
}

/// Parses a successful `GET /api/v1/channel-monitors` envelope into bounded,
/// provider-independent facts. Transport and HTTP failure classification stay
/// in the collector driver so this function has no network side effects.
pub fn parse_channel_monitors_payload(
    station_id: &str,
    endpoint_revision: i64,
    collected_at_ms: i64,
    payload: &Value,
) -> Result<PublishedStatusBatch, PublishedStatusParseError> {
    let station_id = bounded_required(station_id, MAX_PUBLISHED_STATUS_STATION_ID_BYTES)
        .ok_or(parse_error(PublishedStatusParseErrorKind::InvalidStation))?;
    if endpoint_revision < 1 || collected_at_ms < 0 {
        return Err(parse_error(PublishedStatusParseErrorKind::InvalidStation));
    }
    validate_success_envelope(payload)?;
    let items = payload
        .get("data")
        .and_then(Value::as_object)
        .and_then(|data| data.get("items"))
        .and_then(Value::as_array)
        .ok_or(parse_error(PublishedStatusParseErrorKind::InvalidEnvelope))?;
    if items.len() > MAX_PUBLISHED_STATUS_MONITORS {
        return Err(parse_error(PublishedStatusParseErrorKind::TooManyMonitors));
    }
    if items.is_empty() {
        return Ok(PublishedStatusBatch {
            station_id,
            endpoint_revision,
            source_kind: STATION_PUBLISHED_STATUS_SOURCE_KIND.to_string(),
            source_state: PublishedStatusSourceState::Empty,
            completeness: PublishedStatusCompleteness::Complete,
            monitors: Vec::new(),
            collected_at_ms,
            safe_error_kind: None,
        });
    }

    let mut monitors = Vec::with_capacity(items.len());
    let mut identities = HashSet::with_capacity(items.len());
    let mut partial = false;
    for item in items {
        let parsed = match parse_monitor(item) {
            Ok(parsed) => parsed,
            Err(()) => {
                partial = true;
                continue;
            }
        };
        if !identities.insert(parsed.fact.upstream_monitor_id.clone()) {
            partial = true;
            continue;
        }
        partial |= parsed.partial;
        monitors.push(parsed.fact);
    }
    if monitors.is_empty() {
        return Err(parse_error_with_safe_detail(
            PublishedStatusParseErrorKind::NoValidMonitors,
            monitor_item_schema_summary(items),
        ));
    }

    let batch = PublishedStatusBatch {
        station_id,
        endpoint_revision,
        source_kind: STATION_PUBLISHED_STATUS_SOURCE_KIND.to_string(),
        source_state: partial
            .then_some(PublishedStatusSourceState::Degraded)
            .unwrap_or(PublishedStatusSourceState::Available),
        completeness: partial
            .then_some(PublishedStatusCompleteness::Partial)
            .unwrap_or(PublishedStatusCompleteness::Complete),
        monitors,
        collected_at_ms,
        safe_error_kind: None,
    };
    batch
        .validate()
        .map_err(|_| parse_error(PublishedStatusParseErrorKind::InvalidCanonicalFact))?;
    Ok(batch)
}

struct ParsedMonitor {
    fact: PublishedMonitorFact,
    partial: bool,
}

fn parse_monitor(value: &Value) -> Result<ParsedMonitor, ()> {
    let item = value.as_object().ok_or(())?;
    let name = bounded_required(
        item.get("name").and_then(Value::as_str).unwrap_or_default(),
        MAX_PUBLISHED_STATUS_NAME_BYTES,
    )
    .ok_or(())?;
    let provider = bounded_required(
        item.get("provider")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        MAX_PUBLISHED_STATUS_PROVIDER_BYTES,
    )
    .ok_or(())?;
    let primary_model = bounded_required(
        item.get("primary_model")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        MAX_PUBLISHED_STATUS_MODEL_BYTES,
    )
    .ok_or(())?;
    let group_name = optional_bounded_text(
        item.get("group_name").and_then(Value::as_str),
        MAX_PUBLISHED_STATUS_GROUP_BYTES,
    )
    .ok_or(())?;
    let (upstream_monitor_id, identity_kind) = match optional_bounded_text(
        item.get("id").and_then(Value::as_str),
        MAX_PUBLISHED_STATUS_MONITOR_ID_BYTES,
    ) {
        Some(Some(id)) => (id, PublishedMonitorIdentityKind::UpstreamId),
        Some(None) => (
            derived_monitor_identity(&name, &provider, group_name.as_deref(), &primary_model),
            PublishedMonitorIdentityKind::DerivedFallback,
        ),
        None => return Err(()),
    };

    let (source_status, current_outcome, mut partial) =
        parse_status(item.get("primary_status").and_then(Value::as_str));
    let (current_latency_ms, valid_latency) = optional_latency(item.get("primary_latency_ms"));
    let (current_ping_latency_ms, valid_ping) =
        optional_latency(item.get("primary_ping_latency_ms"));
    partial |= !valid_latency || !valid_ping;

    let (extra_models, valid_extra_models) = parse_extra_models(item.get("extra_models"));
    partial |= !valid_extra_models;
    let (samples, valid_timeline, timeline_partial) =
        parse_timeline(item.get("timeline"), &primary_model);
    if !valid_timeline {
        return Err(());
    }
    partial |= timeline_partial;
    let upstream_checked_at_ms = samples.last().map(|sample| sample.checked_at_ms);

    let fact = PublishedMonitorFact {
        upstream_monitor_id,
        identity_kind,
        name,
        provider,
        group_name,
        primary_model,
        extra_models,
        current_outcome,
        source_status,
        current_latency_ms,
        current_ping_latency_ms,
        upstream_checked_at_ms,
        samples,
    };
    fact.validate().map_err(|_| ())?;
    Ok(ParsedMonitor { fact, partial })
}

fn parse_timeline(
    value: Option<&Value>,
    primary_model: &str,
) -> (Vec<PublishedMonitorSampleFact>, bool, bool) {
    let Some(items) = value.and_then(Value::as_array) else {
        return (Vec::new(), false, true);
    };
    if items.len() > MAX_PUBLISHED_STATUS_TIMELINE_INPUT {
        return (Vec::new(), false, true);
    }

    let mut parsed = Vec::with_capacity(items.len());
    let mut partial = false;
    for item in items {
        match parse_sample(item, primary_model) {
            Some((sample, sample_partial)) => {
                if sample.model == primary_model {
                    parsed.push(sample);
                    partial |= sample_partial;
                } else {
                    partial = true;
                }
            }
            None => partial = true,
        }
    }
    if !items.is_empty() && parsed.is_empty() {
        return (Vec::new(), false, true);
    }

    parsed.sort_by(sample_sort_key);
    let mut normalized = Vec::with_capacity(parsed.len());
    let mut cursor = 0;
    while cursor < parsed.len() {
        let checked_at_ms = parsed[cursor].checked_at_ms;
        let model = parsed[cursor].model.clone();
        let mut end = cursor + 1;
        while end < parsed.len()
            && parsed[end].checked_at_ms == checked_at_ms
            && parsed[end].model == model
        {
            end += 1;
        }
        if end - cursor > 1 {
            let chosen = parsed[cursor].clone();
            if parsed[cursor + 1..end]
                .iter()
                .any(|candidate| candidate != &chosen)
            {
                partial = true;
            }
            normalized.push(chosen);
        } else {
            normalized.push(parsed[cursor].clone());
        }
        cursor = end;
    }

    if normalized.len() > MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL {
        let discard = normalized.len() - MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL;
        normalized.drain(0..discard);
    }
    (normalized, true, partial)
}

fn parse_sample(value: &Value, fallback_model: &str) -> Option<(PublishedMonitorSampleFact, bool)> {
    let item = value.as_object()?;
    let model = item
        .get("model")
        .and_then(Value::as_str)
        .and_then(|value| bounded_required(value, MAX_PUBLISHED_STATUS_MODEL_BYTES))
        .or_else(|| bounded_required(fallback_model, MAX_PUBLISHED_STATUS_MODEL_BYTES))?;
    let checked_at_ms = parse_checked_at(item.get("checked_at"))?;
    let (source_status, outcome, mut partial) =
        parse_status(item.get("status").and_then(Value::as_str));
    let (latency_ms, valid_latency) = optional_latency(item.get("latency_ms"));
    let (ping_latency_ms, valid_ping) = optional_latency(item.get("ping_latency_ms"));
    partial |= !valid_latency || !valid_ping;
    let (safe_message, valid_message) = optional_safe_message(item.get("message"));
    partial |= !valid_message;
    Some((
        PublishedMonitorSampleFact {
            model,
            outcome,
            source_status,
            latency_ms,
            ping_latency_ms,
            checked_at_ms,
            safe_message,
        },
        partial,
    ))
}

fn parse_status(value: Option<&str>) -> (String, PublishedSampleOutcome, bool) {
    let source_status = value
        .and_then(|value| bounded_required(value, MAX_PUBLISHED_STATUS_SOURCE_STATUS_BYTES))
        .unwrap_or_else(|| "unknown".to_string());
    let outcome = map_sub2api_published_status(value);
    let partial = outcome == PublishedSampleOutcome::Unknown;
    (source_status, outcome, partial)
}

fn parse_extra_models(value: Option<&Value>) -> (Vec<String>, bool) {
    let Some(items) = value else {
        return (Vec::new(), true);
    };
    let Some(items) = items.as_array() else {
        return (Vec::new(), false);
    };
    if items.len() > MAX_PUBLISHED_STATUS_EXTRA_MODELS {
        return (Vec::new(), false);
    }
    let mut models = BTreeSet::new();
    let mut valid = true;
    for item in items {
        let Some(model) = item
            .as_str()
            .and_then(|value| bounded_required(value, MAX_PUBLISHED_STATUS_MODEL_BYTES))
        else {
            valid = false;
            continue;
        };
        if !models.insert(model) {
            valid = false;
        }
    }
    (models.into_iter().collect(), valid)
}

fn optional_latency(value: Option<&Value>) -> (Option<i64>, bool) {
    match value {
        None | Some(Value::Null) => (None, true),
        Some(value) => match value.as_i64() {
            Some(value) if (0..=MAX_PUBLISHED_STATUS_LATENCY_MS).contains(&value) => {
                (Some(value), true)
            }
            _ => (None, false),
        },
    }
}

fn optional_safe_message(value: Option<&Value>) -> (Option<String>, bool) {
    match value {
        None | Some(Value::Null) => (None, true),
        Some(Value::String(value)) => match sanitize_safe_message(value) {
            Some(value) => (Some(value), true),
            None => (None, false),
        },
        Some(_) => (None, false),
    }
}

fn sanitize_safe_message(value: &str) -> Option<String> {
    let redacted = redact_text(value);
    let without_controls = redacted
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    let trimmed = without_controls.trim();
    (!trimmed.is_empty()).then(|| truncate_utf8(trimmed, MAX_PUBLISHED_STATUS_SAFE_MESSAGE_BYTES))
}

fn parse_checked_at(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(value) => value
            .as_i64()
            .filter(|value| (0..=MAX_PUBLISHED_STATUS_TIMESTAMP_MS).contains(value)),
        Value::String(value) => value
            .parse::<i64>()
            .ok()
            .filter(|value| (0..=MAX_PUBLISHED_STATUS_TIMESTAMP_MS).contains(value))
            .or_else(|| {
                DateTime::parse_from_rfc3339(value)
                    .ok()
                    .map(|value| value.with_timezone(&Utc).timestamp_millis())
                    .filter(|value| (0..=MAX_PUBLISHED_STATUS_TIMESTAMP_MS).contains(value))
            }),
        _ => None,
    }
}

fn bounded_required(value: &str, max_bytes: usize) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty() && trimmed.len() <= max_bytes && !trimmed.chars().any(char::is_control))
        .then(|| trimmed.to_string())
}

fn optional_bounded_text(value: Option<&str>, max_bytes: usize) -> Option<Option<String>> {
    match value {
        None => Some(None),
        Some(value) if value.trim().is_empty() => Some(None),
        Some(value) => bounded_required(value, max_bytes).map(Some),
    }
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn sample_sort_key(
    left: &PublishedMonitorSampleFact,
    right: &PublishedMonitorSampleFact,
) -> std::cmp::Ordering {
    left.checked_at_ms
        .cmp(&right.checked_at_ms)
        .then_with(|| left.model.cmp(&right.model))
        .then_with(|| left.outcome.cmp(&right.outcome))
        .then_with(|| left.source_status.cmp(&right.source_status))
        .then_with(|| left.latency_ms.cmp(&right.latency_ms))
        .then_with(|| left.ping_latency_ms.cmp(&right.ping_latency_ms))
        .then_with(|| left.safe_message.cmp(&right.safe_message))
}

fn validate_success_envelope(payload: &Value) -> Result<(), PublishedStatusParseError> {
    let code = payload
        .as_object()
        .and_then(|payload| payload.get("code"))
        .and_then(Value::as_i64)
        .ok_or(parse_error(PublishedStatusParseErrorKind::InvalidEnvelope))?;
    (code == 0)
        .then_some(())
        .ok_or(parse_error(PublishedStatusParseErrorKind::InvalidEnvelope))
}

fn parse_error(kind: PublishedStatusParseErrorKind) -> PublishedStatusParseError {
    PublishedStatusParseError {
        kind,
        safe_detail: None,
    }
}

fn parse_error_with_safe_detail(
    kind: PublishedStatusParseErrorKind,
    safe_detail: Option<String>,
) -> PublishedStatusParseError {
    PublishedStatusParseError { kind, safe_detail }
}

/// Returns only a short schema fingerprint. It deliberately never includes
/// values from the station response, because collector errors are persisted.
fn monitor_item_schema_summary(items: &[Value]) -> Option<String> {
    const MAX_ITEMS: usize = 3;
    const MAX_FIELDS: usize = 16;
    const MAX_FIELD_NAME_BYTES: usize = 48;
    const MAX_DETAIL_BYTES: usize = 480;

    let mut fields = BTreeSet::new();
    for item in items.iter().take(MAX_ITEMS) {
        let Some(object) = item.as_object() else {
            continue;
        };
        for (name, value) in object {
            if fields.len() == MAX_FIELDS {
                break;
            }
            let name = sanitize_schema_field_name(name, MAX_FIELD_NAME_BYTES)?;
            fields.insert(format!("{name}:{}", json_value_schema(value)));
        }
    }
    if fields.is_empty() {
        return Some(format!(
            "items={} but the first records are not objects",
            items.len()
        ));
    }

    let detail = format!(
        "items={}; observed fields (names/types only): {}",
        items.len(),
        fields.into_iter().collect::<Vec<_>>().join(", ")
    );
    Some(truncate_utf8(&detail, MAX_DETAIL_BYTES))
}

fn sanitize_schema_field_name(value: &str, max_bytes: usize) -> Option<String> {
    let normalized = value
        .chars()
        .map(|character| {
            (character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.'))
                .then_some(character)
                .unwrap_or('_')
        })
        .collect::<String>();
    (!normalized.is_empty()).then(|| truncate_utf8(&normalized, max_bytes))
}

fn json_value_schema(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "bool".to_string(),
        Value::Number(_) => "number".to_string(),
        Value::String(_) => "string".to_string(),
        Value::Array(items) => {
            let mut element_shapes = BTreeSet::new();
            for item in items.iter().take(3) {
                element_shapes.insert(json_value_schema(item));
            }
            if element_shapes.is_empty() {
                "array[]".to_string()
            } else {
                format!(
                    "array[{}]",
                    element_shapes.into_iter().collect::<Vec<_>>().join("|")
                )
            }
        }
        Value::Object(object) => {
            let names = object
                .keys()
                .take(12)
                .filter_map(|name| sanitize_schema_field_name(name, 32))
                .collect::<Vec<_>>();
            if names.is_empty() {
                "object{}".to_string()
            } else {
                format!("object{{{}}}", names.join(","))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::station_published_status::{
        PublishedMonitorIdentityKind, PublishedSampleOutcome, PublishedStatusCompleteness,
        PublishedStatusSourceState,
    };

    fn fixture(name: &str) -> Value {
        let source = match name {
            "complete-60" => include_str!("fixtures/published_status/complete-60.json"),
            "normalization-boundaries" => {
                include_str!("fixtures/published_status/normalization-boundaries.json")
            }
            "empty-success" => include_str!("fixtures/published_status/empty-success.json"),
            "unknown-and-nullable" => {
                include_str!("fixtures/published_status/unknown-and-nullable.json")
            }
            "partial-malformed-item" => {
                include_str!("fixtures/published_status/partial-malformed-item.json")
            }
            "malformed-envelope" => {
                include_str!("fixtures/published_status/malformed-envelope.json")
            }
            _ => panic!("unknown fixture: {name}"),
        };
        serde_json::from_str(source).expect("fixture JSON")
    }

    fn parse(name: &str) -> PublishedStatusBatch {
        parse_channel_monitors_payload("station-fixture", 7, 1_786_000_000_000, &fixture(name))
            .expect("parse fixture")
    }

    #[test]
    fn maps_only_documented_sub2api_statuses() {
        assert_eq!(
            map_sub2api_published_status(Some("healthy")),
            PublishedSampleOutcome::Available
        );
        assert_eq!(
            map_sub2api_published_status(Some("degraded")),
            PublishedSampleOutcome::Degraded
        );
        assert_eq!(
            map_sub2api_published_status(Some("failed")),
            PublishedSampleOutcome::Unavailable
        );
        assert_eq!(
            map_sub2api_published_status(Some("healthy-ish")),
            PublishedSampleOutcome::Unknown
        );
        assert_eq!(
            map_sub2api_published_status(Some("operational")),
            PublishedSampleOutcome::Available
        );
        assert_eq!(
            map_sub2api_published_status(Some("ERROR")),
            PublishedSampleOutcome::Unavailable
        );
        assert_eq!(
            map_sub2api_published_status(None),
            PublishedSampleOutcome::Unknown
        );
    }

    #[test]
    fn complete_fixture_produces_exactly_sixty_canonical_samples() {
        let batch = parse("complete-60");

        assert_eq!(batch.source_state, PublishedStatusSourceState::Available);
        assert_eq!(batch.completeness, PublishedStatusCompleteness::Complete);
        assert_eq!(batch.monitors.len(), 1);
        assert_eq!(batch.monitors[0].samples.len(), 60);
        assert!(batch.monitors[0]
            .samples
            .windows(2)
            .all(|pair| pair[0].checked_at_ms < pair[1].checked_at_ms));
        assert_eq!(
            batch.monitors[0].identity_kind,
            PublishedMonitorIdentityKind::UpstreamId
        );
    }

    #[test]
    fn normalization_sorts_deduplicates_and_keeps_the_latest_sixty_samples() {
        let batch = parse("normalization-boundaries");
        let over_limit = batch
            .monitors
            .iter()
            .find(|monitor| monitor.upstream_monitor_id == "monitor-over-limit")
            .expect("over-limit monitor");
        let duplicate = batch
            .monitors
            .iter()
            .find(|monitor| monitor.upstream_monitor_id == "monitor-unordered-duplicates")
            .expect("duplicate monitor");

        assert_eq!(batch.completeness, PublishedStatusCompleteness::Partial);
        assert_eq!(over_limit.samples.len(), 60);
        assert_eq!(duplicate.samples.len(), 3);
        assert!(duplicate
            .samples
            .windows(2)
            .all(|pair| pair[0].checked_at_ms < pair[1].checked_at_ms));
        assert_eq!(
            duplicate.samples[1].outcome,
            PublishedSampleOutcome::Degraded
        );
    }

    #[test]
    fn empty_success_is_distinct_from_malformed_envelope() {
        let empty = parse("empty-success");
        let malformed = parse_channel_monitors_payload(
            "station-fixture",
            7,
            1_786_000_000_000,
            &fixture("malformed-envelope"),
        )
        .expect_err("malformed envelope");

        assert_eq!(empty.source_state, PublishedStatusSourceState::Empty);
        assert_eq!(empty.completeness, PublishedStatusCompleteness::Complete);
        assert!(empty.monitors.is_empty());
        assert_eq!(
            malformed.kind,
            PublishedStatusParseErrorKind::InvalidEnvelope
        );
    }

    #[test]
    fn unknown_nullable_and_invalid_monitor_items_produce_partial_facts() {
        let unknown = parse("unknown-and-nullable");
        let partial = parse("partial-malformed-item");

        assert_eq!(unknown.completeness, PublishedStatusCompleteness::Partial);
        assert_eq!(
            unknown.monitors[0].current_outcome,
            PublishedSampleOutcome::Unknown
        );
        assert_eq!(partial.completeness, PublishedStatusCompleteness::Partial);
        assert_eq!(partial.monitors.len(), 1);
        assert_eq!(
            partial.monitors[0].upstream_monitor_id,
            "monitor-valid-among-malformed"
        );
    }

    #[test]
    fn missing_upstream_id_has_a_stable_derived_identity() {
        let payload = serde_json::json!({
            "code": 0,
            "data": { "items": [{
                "name": "Fallback monitor",
                "provider": "openai",
                "group_name": "default",
                "primary_model": "gpt-fixture",
                "primary_status": "healthy",
                "timeline": []
            }]}
        });

        let first =
            parse_channel_monitors_payload("station-fixture", 7, 1, &payload).expect("first parse");
        let second = parse_channel_monitors_payload("station-fixture", 7, 1, &payload)
            .expect("second parse");

        assert_eq!(
            first.monitors[0].identity_kind,
            PublishedMonitorIdentityKind::DerivedFallback
        );
        assert_eq!(
            first.monitors[0].upstream_monitor_id,
            second.monitors[0].upstream_monitor_id
        );
    }

    #[test]
    fn invalid_monitor_error_contains_only_a_bounded_schema_summary() {
        let payload = serde_json::json!({
            "code": 0,
            "data": { "items": [{
                "monitor label": "do-not-persist-this-value",
                "credential": "sk-p8-secret-plaintext-canary",
                "records": []
            }]}
        });

        let error = parse_channel_monitors_payload("station-fixture", 7, 1, &payload)
            .expect_err("invalid monitor item");
        let message = error.to_string();

        assert_eq!(error.kind, PublishedStatusParseErrorKind::NoValidMonitors);
        assert!(message.contains("credential:string"));
        assert!(message.contains("monitor_label:string"));
        assert!(message.contains("records:array[]"));
        assert!(!message.contains("do-not-persist-this-value"));
        assert!(!message.contains("sk-p8-secret-plaintext-canary"));
        assert!(message.len() <= 600);
    }

    #[test]
    fn timeline_only_keeps_samples_for_the_primary_model() {
        let payload = serde_json::json!({
            "code": 0,
            "data": { "items": [{
                "id": "monitor-primary-model",
                "name": "Primary model monitor",
                "provider": "openai",
                "primary_model": "gpt-primary",
                "primary_status": "healthy",
                "extra_models": ["gpt-extra"],
                "timeline": [
                    {
                        "model": "gpt-extra",
                        "status": "healthy",
                        "checked_at": "2026-08-15T00:00:00Z"
                    },
                    {
                        "model": "gpt-primary",
                        "status": "degraded",
                        "checked_at": "2026-08-15T00:01:00Z"
                    }
                ]
            }]}
        });

        let batch = parse_channel_monitors_payload("station-fixture", 7, 1, &payload)
            .expect("parse primary-model timeline");

        assert_eq!(batch.completeness, PublishedStatusCompleteness::Partial);
        assert_eq!(batch.monitors[0].samples.len(), 1);
        assert_eq!(batch.monitors[0].samples[0].model, "gpt-primary");
    }

    #[test]
    fn timeline_without_model_uses_the_monitor_primary_model() {
        let payload = serde_json::json!({
            "code": 0,
            "data": { "items": [{
                "id": "monitor-timeline-model-fallback",
                "name": "Timeline model fallback",
                "provider": "openai",
                "primary_model": "gpt-primary",
                "primary_status": "healthy",
                "timeline": [{
                    "status": "healthy",
                    "latency_ms": 120,
                    "ping_latency_ms": 20,
                    "checked_at": "2026-08-15T00:00:00Z"
                }]
            }]}
        });

        let batch = parse_channel_monitors_payload("station-fixture", 7, 1, &payload)
            .expect("parse timeline without explicit model");

        assert_eq!(batch.monitors[0].samples.len(), 1);
        assert_eq!(batch.monitors[0].samples[0].model, "gpt-primary");
        assert_eq!(batch.completeness, PublishedStatusCompleteness::Complete);
    }

    #[test]
    fn ignores_upstream_seven_day_availability_without_degrading_the_batch() {
        let payload = serde_json::json!({
            "code": 0,
            "data": { "items": [{
                "id": "monitor-ignored-availability",
                "name": "Ignored availability monitor",
                "provider": "openai",
                "primary_model": "gpt-primary",
                "primary_status": "healthy",
                "availability_7d": "not-a-percent",
                "timeline": [{
                    "status": "healthy",
                    "checked_at": "2026-08-15T00:00:00Z"
                }]
            }]}
        });

        let batch = parse_channel_monitors_payload("station-fixture", 7, 1, &payload)
            .expect("parse payload with ignored upstream availability");

        assert_eq!(batch.completeness, PublishedStatusCompleteness::Complete);
    }

    #[test]
    fn unsafe_message_is_redacted_bounded_and_control_free() {
        let canary = "sk-p8-secret-plaintext-canary";
        let payload = serde_json::json!({
            "code": 0,
            "data": { "items": [{
                "id": "monitor-message",
                "name": "Message monitor",
                "provider": "openai",
                "primary_model": "gpt-fixture",
                "primary_status": "healthy",
                "timeline": [{
                    "model": "gpt-fixture",
                    "status": "healthy",
                    "checked_at": "2026-08-15T00:00:00Z",
                    "message": format!("Authorization: Bearer {canary}\n{}", "x".repeat(800))
                }]
            }]}
        });
        let batch = parse_channel_monitors_payload("station-fixture", 7, 1, &payload)
            .expect("parse payload");
        let message = batch.monitors[0].samples[0]
            .safe_message
            .as_deref()
            .expect("safe message");

        assert!(!message.contains(canary));
        assert!(!message.chars().any(char::is_control));
        assert!(message.len() <= MAX_PUBLISHED_STATUS_SAFE_MESSAGE_BYTES);
    }
}
