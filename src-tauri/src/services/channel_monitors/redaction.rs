use serde_json::{Map, Value};

const REDACTED: &str = "[REDACTED]";
const MAX_TEXT_LEN: usize = 500;
const SENSITIVE_KEYS: [&str; 12] = [
    "authorization",
    "cookie",
    "set-cookie",
    "api_key",
    "apikey",
    "access_token",
    "refresh_token",
    "token",
    "secret",
    "session",
    "password",
    "key",
];

pub fn redact_monitor_text(input: &str) -> String {
    truncate_text(&redact_secret_tokens(input))
}

pub fn redact_monitor_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(redact_object(map)),
        Value::Array(items) => Value::Array(items.iter().map(redact_monitor_json).collect()),
        Value::String(text) => Value::String(redact_monitor_text(text)),
        _ => value.clone(),
    }
}

fn redact_object(map: &Map<String, Value>) -> Map<String, Value> {
    map.iter()
        .map(|(key, value)| {
            if is_sensitive_key(key) {
                (key.clone(), Value::String(REDACTED.to_string()))
            } else {
                (key.clone(), redact_monitor_json(value))
            }
        })
        .collect()
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase();
    SENSITIVE_KEYS.iter().any(|item| normalized == *item)
}

fn redact_secret_tokens(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut token = String::new();
    let mut redact_next = false;

    for ch in input.chars() {
        if ch.is_whitespace() {
            flush_token(&mut output, &mut token, &mut redact_next);
            output.push(ch);
        } else {
            token.push(ch);
        }
    }
    flush_token(&mut output, &mut token, &mut redact_next);

    output
}

fn flush_token(output: &mut String, token: &mut String, redact_next: &mut bool) {
    if token.is_empty() {
        return;
    }

    let lower = token
        .trim_matches(|ch: char| ch.is_ascii_punctuation())
        .to_ascii_lowercase();
    if lower == "bearer" {
        output.push_str(token);
        *redact_next = true;
    } else if *redact_next || is_secret_like_token(&lower) {
        output.push_str(REDACTED);
        *redact_next = false;
    } else {
        output.push_str(token);
    }
    token.clear();
}

fn is_secret_like_token(token: &str) -> bool {
    token.starts_with("sk-") && token.len() >= 12
}

fn truncate_text(input: &str) -> String {
    if input.chars().count() <= MAX_TEXT_LEN {
        return input.to_string();
    }

    let mut output: String = input.chars().take(MAX_TEXT_LEN).collect();
    output.push_str("...");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recursively_redacts_secret_json_fields() {
        let value = json!({
            "authorization": "Bearer sk-live-secret",
            "nested": {
                "api_key": "sk-nested-secret",
                "items": [
                    { "refresh_token": "refresh-secret" },
                    { "safe": "visible" }
                ]
            }
        });

        let redacted = redact_monitor_json(&value);
        let text = redacted.to_string();

        assert_eq!(redacted["authorization"], "[REDACTED]");
        assert_eq!(redacted["nested"]["api_key"], "[REDACTED]");
        assert_eq!(
            redacted["nested"]["items"][0]["refresh_token"],
            "[REDACTED]"
        );
        assert_eq!(redacted["nested"]["items"][1]["safe"], "visible");
        assert!(!text.contains("sk-live-secret"));
        assert!(!text.contains("refresh-secret"));
    }

    #[test]
    fn redacts_bearer_and_sk_like_text_and_truncates() {
        let long_tail = "x".repeat(700);
        let input = format!(
            "error Bearer sk-bearer-secret-123 and direct sk-direct-secret-456 {long_tail}"
        );

        let redacted = redact_monitor_text(&input);

        assert!(!redacted.contains("sk-bearer-secret-123"));
        assert!(!redacted.contains("sk-direct-secret-456"));
        assert!(redacted.matches("[REDACTED]").count() >= 2);
        assert!(redacted.len() <= 520);
    }
}
