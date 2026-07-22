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

pub fn mask_secret(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "未设置".to_string();
    }
    if trimmed.chars().count() <= 8 {
        return "****".to_string();
    }
    let prefix: String = trimmed.chars().take(3).collect();
    let suffix: String = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
}

pub fn redact_text(text: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        if let Ok(serialized) = serde_json::to_string(&redact_value(&value)) {
            return serialized;
        }
    }

    text.split_whitespace()
        .map(|segment| {
            if looks_like_secret(segment) || segment_has_secret_assignment(segment) {
                "[REDACTED]".to_string()
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn redact_text_preview(text: &str, limit: usize) -> String {
    let limited: String = text.chars().take(limit).collect();
    let redacted = redact_text(&limited);
    if text.chars().count() > limit {
        format!("{redacted}\n... truncated")
    } else {
        redacted
    }
}

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
        Value::String(text) if looks_like_secret(text) => Value::String("[REDACTED]".to_string()),
        _ => value.clone(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_secret_keeps_prefix_and_suffix_only() {
        let masked = mask_secret("sk-p8-secret-plaintext-canary");

        assert_eq!(masked, "sk-...nary");
        assert!(!masked.contains("secret-plaintext"));
    }

    #[test]
    fn redact_text_removes_bearer_token() {
        let redacted = redact_text("Authorization: Bearer sk-p8-secret-plaintext-canary");

        assert!(redacted.contains("[REDACTED]"));
        assert!(!redacted.contains("sk-p8-secret-plaintext-canary"));
    }

    #[test]
    fn redact_value_removes_nested_cookie() {
        let value = serde_json::json!({
            "headers": {
                "cookie": "rpd_session=p8-cookie-canary"
            },
            "model": "gpt-4o-mini"
        });

        let redacted = redact_value(&value);

        assert_eq!(redacted["headers"]["cookie"], "[REDACTED]");
        assert_eq!(redacted["model"], "gpt-4o-mini");
    }

    #[test]
    fn redact_text_parses_json_payloads() {
        let redacted = redact_text(
            r#"{"authorization":"Bearer sk-p8-secret-plaintext-canary","model":"gpt-4o-mini"}"#,
        );

        assert!(redacted.contains("[REDACTED]"));
        assert!(redacted.contains("gpt-4o-mini"));
        assert!(!redacted.contains("sk-p8-secret-plaintext-canary"));
    }
}
