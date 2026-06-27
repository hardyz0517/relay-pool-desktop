use serde_json::{Map, Value};

const SECRET_HINTS: [&str; 12] = [
    "api_key",
    "apikey",
    "key",
    "token",
    "access_token",
    "refresh_token",
    "authorization",
    "cookie",
    "password",
    "secret",
    "session",
    "credential",
];

const TEXT_PREVIEW_LIMIT: usize = 4_000;

pub fn redact_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut next = Map::new();
            for (key, child) in map {
                if is_secret_key(key) {
                    next.insert(key.clone(), Value::String("[REDACTED]".to_string()));
                } else {
                    next.insert(key.clone(), redact_value(child));
                }
            }
            Value::Object(next)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        Value::String(text) if looks_like_secret(text) => {
            Value::String("[REDACTED]".to_string())
        }
        _ => value.clone(),
    }
}

pub fn redact_text_preview(text: &str) -> String {
    let limited: String = text.chars().take(TEXT_PREVIEW_LIMIT).collect();
    let redacted = redact_secret_patterns(&limited);
    if text.chars().count() > TEXT_PREVIEW_LIMIT {
        format!("{redacted}\n... 已截断")
    } else {
        redacted
    }
}

fn redact_secret_patterns(text: &str) -> String {
    let mut output = Vec::new();
    for segment in text.split_whitespace() {
        if looks_like_secret(segment) || segment_has_secret_assignment(segment) {
            output.push("[REDACTED]".to_string());
        } else {
            output.push(segment.to_string());
        }
    }
    output.join(" ")
}

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    SECRET_HINTS.iter().any(|hint| lower.contains(hint))
}

fn looks_like_secret(value: &str) -> bool {
    let lower = value.to_lowercase();
    value.len() > 18
        && (lower.starts_with("sk-")
            || lower.starts_with("bearer ")
            || lower.contains("authorization")
            || lower.contains("token=")
            || lower.contains("session=")
            || lower.contains("api_key=")
            || lower.contains("password="))
}

fn segment_has_secret_assignment(value: &str) -> bool {
    let lower = value.to_lowercase();
    SECRET_HINTS
        .iter()
        .any(|hint| lower.contains(&format!("{hint}=")) || lower.contains(&format!("{hint}:")))
}
