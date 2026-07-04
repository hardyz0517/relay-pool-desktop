use serde_json::{Map, Value};

const REDACTED: &str = "[REDACTED]";
const MAX_TEXT_LEN: usize = 500;
const SENSITIVE_KEY_HINTS: [&str; 16] = [
    "authorization",
    "cookie",
    "set-cookie",
    "api_key",
    "apikey",
    "access_key",
    "accesskey",
    "access_token",
    "accesstoken",
    "refresh_token",
    "refreshtoken",
    "token",
    "secret",
    "session",
    "password",
    "credential",
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
    let normalized = normalize_key(key);
    if normalized == "key" {
        return true;
    }
    SENSITIVE_KEY_HINTS
        .iter()
        .map(|item| normalize_key(item))
        .any(|hint| normalized.contains(&hint))
}

fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
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

    if *redact_next {
        output.push_str(REDACTED);
        *redact_next = false;
        token.clear();
        return;
    }

    if let Some(redacted_assignment) = redact_assignment_token(token, redact_next) {
        output.push_str(&redacted_assignment);
        token.clear();
        return;
    }

    let lower = token
        .trim_matches(is_boundary_punctuation)
        .to_ascii_lowercase();
    if lower == "bearer" {
        output.push_str(token);
        *redact_next = true;
    } else if is_secret_like_token(&lower) {
        output.push_str(REDACTED);
    } else {
        output.push_str(token);
    }
    token.clear();
}

fn redact_assignment_token(token: &str, redact_next: &mut bool) -> Option<String> {
    let (delimiter_index, delimiter) = token
        .char_indices()
        .find(|(_, ch)| *ch == '=' || *ch == ':')?;
    let key = token[..delimiter_index].trim_matches(is_boundary_punctuation);
    if !is_sensitive_key(key) {
        return None;
    }

    let value_start = delimiter_index + delimiter.len_utf8();
    let value = token[value_start..].trim_start_matches(is_boundary_punctuation);
    if value.is_empty() {
        *redact_next = true;
    }

    Some(format!(
        "{}{}{}",
        &token[..delimiter_index],
        delimiter,
        REDACTED
    ))
}

fn is_boundary_punctuation(ch: char) -> bool {
    ch.is_ascii_punctuation() && ch != '_' && ch != '-'
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
    fn redacts_secret_key_name_variants_without_redacting_safe_fields() {
        let value = json!({
            "accessToken": "access-secret",
            "refreshToken": "refresh-secret",
            "secretKey": "secret-key",
            "x-api-key": "api-secret",
            "credentials": {
                "password": "nested-password"
            },
            "model": "gpt-4o-mini",
            "ok": true
        });

        let redacted = redact_monitor_json(&value);
        let text = redacted.to_string();

        assert_eq!(redacted["accessToken"], "[REDACTED]");
        assert_eq!(redacted["refreshToken"], "[REDACTED]");
        assert_eq!(redacted["secretKey"], "[REDACTED]");
        assert_eq!(redacted["x-api-key"], "[REDACTED]");
        assert_eq!(redacted["credentials"], "[REDACTED]");
        assert_eq!(redacted["model"], "gpt-4o-mini");
        assert_eq!(redacted["ok"], true);
        assert!(!text.contains("access-secret"));
        assert!(!text.contains("api-secret"));
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

    #[test]
    fn redacts_secret_assignment_patterns_in_text() {
        let input = r#"api_key=plain-secret session=abc accessToken":"access-secret refresh_token: refresh-secret"#;

        let redacted = redact_monitor_text(input);

        assert!(!redacted.contains("plain-secret"));
        assert!(!redacted.contains("session=abc"));
        assert!(!redacted.contains("access-secret"));
        assert!(!redacted.contains("refresh-secret"));
        assert!(redacted.matches("[REDACTED]").count() >= 4);
    }
}
