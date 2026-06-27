pub mod redaction;
pub mod session;

use serde_json::{json, Value};

use crate::models::capture::{CapturedHttpEvent, CapturedHttpEventInput};

pub fn sanitize_event(input: CapturedHttpEventInput) -> CapturedHttpEvent {
    let request_path = input
        .request_path
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| path_from_url(&input.request_url));
    let content_type = input.content_type.unwrap_or_default();
    let response_kind = input
        .response_kind
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| infer_response_kind(&content_type, input.response_json.is_some()));
    let response_json_redacted = input.response_json.as_ref().map(redaction::redact_value);
    let response_text_preview_redacted = input
        .response_text
        .as_deref()
        .map(redaction::redact_text_preview);
    let response_size = input.response_size.unwrap_or_else(|| {
        response_json_redacted
            .as_ref()
            .map(|value| value.to_string().len() as i64)
            .or_else(|| response_text_preview_redacted.as_ref().map(|value| value.len() as i64))
            .unwrap_or(0)
    });
    let classification = classify_event(&request_path, input.status, &content_type);

    CapturedHttpEvent {
        id: format!("capture-{}", crate::services::database::now_millis_for_services()),
        station_id: input.station_id,
        source_window_id: input.source_window_id,
        page_url: input.page_url,
        request_url: input.request_url,
        request_path,
        method: input.method.to_uppercase(),
        status: input.status,
        content_type,
        started_at: input.started_at,
        finished_at: input.finished_at,
        duration_ms: input.duration_ms,
        response_kind,
        response_size,
        response_json_redacted,
        response_text_preview_redacted,
        classification,
        confidence: 0.5,
        error_message: input.error_message.map(short_error),
    }
}

pub fn summarize_events(events: &[CapturedHttpEvent]) -> (Value, Value, Value) {
    let endpoints: Vec<Value> = events
        .iter()
        .map(|event| {
            json!({
                "url": event.request_url,
                "path": event.request_path,
                "method": event.method,
                "status": event.status,
                "classification": event.classification,
                "confidence": event.confidence,
            })
        })
        .collect();
    let matched_fields = extract_fields(events);
    let recognized_field_count = matched_fields.len();
    let pending_confirmations: Vec<Value> = matched_fields
        .iter()
        .filter(|field| {
            field
                .get("confidence")
                .and_then(Value::as_f64)
                .map(|confidence| confidence < 0.8)
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    let balance = first_field_value(&matched_fields, "balance");
    let groups = values_for_category(&matched_fields, "group");
    let rates = values_for_category(&matched_fields, "rate");
    let keys = values_for_category(&matched_fields, "key_metadata");
    let models = values_for_category(&matched_fields, "model");
    let status = if recognized_field_count > 0 {
        if pending_confirmations.is_empty() {
            "success"
        } else {
            "needs_confirmation"
        }
    } else if events.is_empty() {
        "manual_required"
    } else {
        "partial"
    };
    let summary = json!({
        "mode": "webview-capture",
        "adapter": "WebView Capture",
        "detectedType": "Captured",
        "conclusion": if recognized_field_count > 0 { "已捕获" } else { "等待捕获" },
        "message": if recognized_field_count > 0 {
            format!("捕获到 {} 个接口，识别到 {} 个候选字段。", events.len(), recognized_field_count)
        } else {
            "网页登录窗口已打开，请登录并浏览后台页面后点击完成采集。".to_string()
        },
        "capture": {
            "endpointCount": events.len(),
            "recognizedFieldCount": recognized_field_count,
            "pendingConfirmationCount": pending_confirmations.len(),
        },
        "recognized": {
            "balanceLabel": balance.clone().unwrap_or(Value::String("未识别".to_string())),
            "groupCount": groups.len(),
            "rateCount": rates.len(),
            "keyCount": keys.len(),
            "modelCount": models.len(),
            "matchedFieldCount": recognized_field_count,
        },
        "endpointResults": events.iter().map(|event| json!({
            "path": event.request_path,
            "result": endpoint_result_label(event.status),
            "detail": endpoint_detail(event),
            "statusCode": event.status,
        })).collect::<Vec<_>>(),
        "webviewRequired": false,
        "rawPreviewAvailable": true,
    });
    let normalized = json!({
        "stationId": events.first().map(|event| event.station_id.clone()),
        "adapter": "webview-capture",
        "status": status,
        "balance": balance.unwrap_or(Value::Null),
        "groups": groups,
        "rateMultipliers": rates,
        "keys": keys,
        "models": models,
        "matchedFields": matched_fields,
        "detectedEndpoints": endpoints,
        "pendingConfirmations": pending_confirmations,
        "confidenceSummary": {
            "recognizedFieldCount": recognized_field_count,
        },
    });
    let raw = json!({
        "events": events.iter().map(event_preview).collect::<Vec<_>>(),
    });
    (summary, normalized, raw)
}

pub fn event_field_counts(events: &[CapturedHttpEvent]) -> (usize, usize) {
    let fields = extract_fields(events);
    let pending = fields
        .iter()
        .filter(|field| {
            field
                .get("confidence")
                .and_then(Value::as_f64)
                .map(|confidence| confidence < 0.8)
                .unwrap_or(false)
        })
        .count();
    (fields.len(), pending)
}

fn extract_fields(events: &[CapturedHttpEvent]) -> Vec<Value> {
    let mut output = Vec::new();
    for event in events {
        if let Some(json) = &event.response_json_redacted {
            collect_json_matches(json, "$", event, &mut output);
        }
    }
    output
}

fn collect_json_matches(
    value: &Value,
    path: &str,
    event: &CapturedHttpEvent,
    output: &mut Vec<Value>,
) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let next_path = format!("{path}.{key}");
                if let Some(category) = category_for_key(key) {
                    output.push(json!({
                        "category": category,
                        "label": key,
                        "value": child,
                        "sourceUrl": event.request_url,
                        "sourcePath": event.request_path,
                        "jsonPath": next_path,
                        "confidence": confidence_for_match(category, &event.request_path),
                        "evidencePreview": preview_value(child),
                    }));
                }
                collect_json_matches(child, &next_path, event, output);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                collect_json_matches(child, &format!("{path}[{index}]"), event, output);
            }
        }
        _ => {}
    }
}

fn category_for_key(key: &str) -> Option<&'static str> {
    let lower = key.to_lowercase();
    if matches_any(&lower, &["balance", "quota", "credit", "amount", "remain", "remaining"]) {
        return Some("balance");
    }
    if matches_any(&lower, &["group", "group_id", "group_name", "usable_group"]) {
        return Some("group");
    }
    if matches_any(&lower, &["rate_multiplier", "ratio", "multiplier", "model_ratio", "completion_ratio"]) {
        return Some("rate");
    }
    if matches_any(&lower, &["api_key", "apikey", "custom_key", "token", "access_token", "sk"]) {
        return Some("key_metadata");
    }
    if matches_any(&lower, &["model", "model_name", "model_id", "models", "owned_by"]) {
        return Some("model");
    }
    if matches_any(&lower, &["usage", "used", "prompt_tokens", "completion_tokens", "total_tokens", "request_count"]) {
        return Some("usage");
    }
    None
}

fn confidence_for_match(category: &str, path: &str) -> f64 {
    let lower_path = path.to_lowercase();
    match category {
        "balance" if lower_path.contains("user") || lower_path.contains("profile") => 0.85,
        "key_metadata" if lower_path.contains("key") => 0.85,
        "rate" if lower_path.contains("rate") || lower_path.contains("ratio") || lower_path.contains("pricing") => 0.8,
        "model" if lower_path.contains("model") || lower_path.contains("channel") => 0.8,
        _ => 0.65,
    }
}

fn first_field_value(fields: &[Value], category: &str) -> Option<Value> {
    fields.iter().find_map(|field| {
        let field_category = field.get("category").and_then(Value::as_str)?;
        if field_category == category {
            field.get("value").cloned()
        } else {
            None
        }
    })
}

fn values_for_category(fields: &[Value], category: &str) -> Vec<Value> {
    fields
        .iter()
        .filter_map(|field| {
            let field_category = field.get("category").and_then(Value::as_str)?;
            if field_category == category {
                field.get("value").cloned()
            } else {
                None
            }
        })
        .collect()
}

fn event_preview(event: &CapturedHttpEvent) -> Value {
    json!({
        "path": event.request_path,
        "method": event.method,
        "status": event.status,
        "contentType": event.content_type,
        "classification": event.classification,
        "responseJsonRedacted": event.response_json_redacted,
        "responseTextPreviewRedacted": event.response_text_preview_redacted,
        "errorMessage": event.error_message,
    })
}

fn endpoint_result_label(status: Option<i64>) -> &'static str {
    match status {
        Some(200..=299) => "成功",
        Some(300..=399) => "重定向",
        Some(401 | 403) => "需要登录",
        Some(404) => "404",
        Some(429) => "限流",
        Some(500..=599) => "站点异常",
        Some(_) => "已捕获",
        None => "请求失败",
    }
}

fn endpoint_detail(event: &CapturedHttpEvent) -> String {
    if let Some(error) = &event.error_message {
        return error.clone();
    }
    if event.response_json_redacted.is_some() {
        "捕获到 JSON 响应".to_string()
    } else if event.content_type.contains("html") {
        "捕获到页面响应".to_string()
    } else {
        "已捕获接口响应".to_string()
    }
}

fn classify_event(path: &str, status: Option<i64>, content_type: &str) -> String {
    if matches!(status, Some(401 | 403)) {
        return "auth_required".to_string();
    }
    let lower = path.to_lowercase();
    if lower.contains("key") {
        "keys".to_string()
    } else if lower.contains("pricing") || lower.contains("ratio") || lower.contains("rate") {
        "pricing".to_string()
    } else if lower.contains("user") || lower.contains("profile") || lower.contains("me") {
        "account".to_string()
    } else if lower.contains("model") || lower.contains("channel") {
        "models".to_string()
    } else if content_type.contains("json") {
        "json".to_string()
    } else {
        "other".to_string()
    }
}

fn infer_response_kind(content_type: &str, has_json: bool) -> String {
    if has_json || content_type.contains("json") {
        "json".to_string()
    } else if content_type.contains("html") {
        "html".to_string()
    } else {
        "text".to_string()
    }
}

fn path_from_url(url: &str) -> String {
    let without_scheme = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url);
    let path = without_scheme
        .find('/')
        .map(|index| &without_scheme[index..])
        .unwrap_or("/");
    path.split(['?', '#']).next().unwrap_or("/").to_string()
}

fn preview_value(value: &Value) -> String {
    let text = match value {
        Value::String(text) => text.clone(),
        _ => value.to_string(),
    };
    text.chars().take(160).collect()
}

fn matches_any(value: &str, hints: &[&str]) -> bool {
    hints.iter().any(|hint| value.contains(hint))
}

fn short_error(error: String) -> String {
    if error.len() > 160 {
        format!("{}...", error.chars().take(160).collect::<String>())
    } else {
        error
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{
        models::capture::CapturedHttpEventInput,
        services::capture::{sanitize_event, summarize_events},
    };

    #[test]
    fn sanitize_event_redacts_secret_json_fields() {
        let event = sanitize_event(CapturedHttpEventInput {
            station_id: "station-1".to_string(),
            source_window_id: "window-1".to_string(),
            page_url: "https://relay.example/dashboard".to_string(),
            request_url: "https://relay.example/api/user/profile".to_string(),
            request_path: None,
            method: "get".to_string(),
            status: Some(200),
            content_type: Some("application/json".to_string()),
            started_at: None,
            finished_at: None,
            duration_ms: Some(15),
            response_kind: None,
            response_size: None,
            response_json: Some(json!({
                "balance": 12.5,
                "api_key": "sk-secret-value-that-must-not-leak",
                "nested": { "authorization": "Bearer secret-token" }
            })),
            response_text: None,
            error_message: None,
        });

        let redacted = event.response_json_redacted.unwrap();
        assert_eq!(redacted["balance"], json!(12.5));
        assert_eq!(redacted["api_key"], json!("[REDACTED]"));
        assert_eq!(redacted["nested"]["authorization"], json!("[REDACTED]"));
        assert_eq!(event.request_path, "/api/user/profile");
        assert_eq!(event.classification, "account");
    }

    #[test]
    fn sanitize_event_truncates_and_redacts_text_preview() {
        let sensitive = format!("token={}{}", "a".repeat(80), "x".repeat(5000));
        let event = sanitize_event(CapturedHttpEventInput {
            station_id: "station-1".to_string(),
            source_window_id: "window-1".to_string(),
            page_url: "https://relay.example".to_string(),
            request_url: "https://relay.example/api/error".to_string(),
            request_path: Some("/api/error".to_string()),
            method: "post".to_string(),
            status: Some(500),
            content_type: Some("text/plain".to_string()),
            started_at: None,
            finished_at: None,
            duration_ms: None,
            response_kind: None,
            response_size: None,
            response_json: None,
            response_text: Some(sensitive),
            error_message: Some("very long error".repeat(20)),
        });

        let preview = event.response_text_preview_redacted.unwrap();
        assert!(preview.len() <= 4_020);
        assert!(preview.contains("[REDACTED]"));
        assert!(!preview.contains("aaaaaaaaaaaaaaaaaaaa"));
        assert_eq!(event.method, "POST");
        assert_eq!(event.classification, "other");
        assert!(event.error_message.unwrap().len() <= 163);
    }

    #[test]
    fn summarize_events_extracts_business_fields_with_confidence() {
        let event = sanitize_event(CapturedHttpEventInput {
            station_id: "station-1".to_string(),
            source_window_id: "window-1".to_string(),
            page_url: "https://relay.example/dashboard".to_string(),
            request_url: "https://relay.example/api/ratio_config".to_string(),
            request_path: None,
            method: "GET".to_string(),
            status: Some(200),
            content_type: Some("application/json".to_string()),
            started_at: None,
            finished_at: None,
            duration_ms: None,
            response_kind: None,
            response_size: None,
            response_json: Some(json!({
                "data": {
                    "group_name": "default",
                    "model_ratio": { "gpt-4o-mini": 0.5 },
                    "models": ["gpt-4o-mini"]
                }
            })),
            response_text: None,
            error_message: None,
        });
        let (summary, normalized, raw) = summarize_events(&[event]);

        assert_eq!(summary["capture"]["endpointCount"], json!(1));
        assert_eq!(summary["recognized"]["groupCount"], json!(1));
        assert_eq!(summary["recognized"]["rateCount"], json!(1));
        assert_eq!(summary["recognized"]["modelCount"], json!(1));
        assert_eq!(normalized["status"], json!("needs_confirmation"));
        assert_eq!(raw["events"][0]["path"], json!("/api/ratio_config"));
    }
}
