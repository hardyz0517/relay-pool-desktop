#[cfg(test)]
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
    let assignment_redacted = redact_sensitive_assignments(input);
    truncate_text(&redact_secret_tokens(&assignment_redacted))
}

#[cfg(test)]
pub fn redact_monitor_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(redact_object(map)),
        Value::Array(items) => Value::Array(items.iter().map(redact_monitor_json).collect()),
        Value::String(text) => Value::String(redact_monitor_text(text)),
        _ => value.clone(),
    }
}

#[cfg(test)]
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

fn is_boundary_punctuation(ch: char) -> bool {
    ch.is_ascii_punctuation() && ch != '_' && ch != '-'
}

fn redact_sensitive_assignments(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut last_written = 0;
    let mut index = 0;

    while index < chars.len() {
        if chars[index] != ':' && chars[index] != '=' {
            index += 1;
            continue;
        }

        let Some(key) = assignment_key_before(&chars, index) else {
            index += 1;
            continue;
        };
        if !is_sensitive_key(&key) {
            index += 1;
            continue;
        }

        let (value_start, value_end) = assignment_value_range(&chars, index + 1, &key);
        push_chars(&mut output, &chars[last_written..value_start]);
        output.push_str(REDACTED);
        last_written = value_end;
        index = value_end;
    }

    push_chars(&mut output, &chars[last_written..]);
    output
}

fn assignment_key_before(chars: &[char], delimiter_index: usize) -> Option<String> {
    let mut end = delimiter_index;
    while end > 0 && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    if end == 0 {
        return None;
    }

    if chars[end - 1] == '"' || chars[end - 1] == '\'' {
        let quote = chars[end - 1];
        let key_end = end - 1;
        let mut start_quote = key_end;
        while start_quote > 0 {
            start_quote -= 1;
            if chars[start_quote] == quote && !is_escaped(chars, start_quote) {
                let key: String = chars[start_quote + 1..key_end].iter().collect();
                return Some(key);
            }
        }
        return unquoted_key_before(chars, key_end);
    }

    unquoted_key_before(chars, end)
}

fn unquoted_key_before(chars: &[char], key_end: usize) -> Option<String> {
    let mut key_start = key_end;
    while key_start > 0 && is_key_char(chars[key_start - 1]) {
        key_start -= 1;
    }
    if key_start == key_end {
        return None;
    }
    Some(chars[key_start..key_end].iter().collect())
}

fn assignment_value_range(chars: &[char], mut value_start: usize, key: &str) -> (usize, usize) {
    while value_start < chars.len() && chars[value_start].is_whitespace() {
        value_start += 1;
    }

    if value_start < chars.len() && (chars[value_start] == '"' || chars[value_start] == '\'') {
        let quote = chars[value_start];
        let content_start = value_start + 1;
        let mut value_end = content_start;
        while value_end < chars.len() {
            if chars[value_end] == quote && !is_escaped(chars, value_end) {
                return (content_start, value_end);
            }
            value_end += 1;
        }
        let fallback_end = unclosed_quoted_value_boundary(chars, content_start);
        return (content_start, fallback_end);
    }

    if is_header_style_secret_key(key) {
        return (value_start, header_style_value_end(chars, value_start));
    }

    let mut value_end = value_start;
    while value_end < chars.len() && !is_unquoted_value_boundary(chars[value_end]) {
        value_end += 1;
    }
    (value_start, value_end)
}

fn is_key_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn is_unquoted_value_boundary(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, ',' | '}' | ']' | ';' | '&')
}

fn is_header_style_secret_key(key: &str) -> bool {
    matches!(
        normalize_key(key).as_str(),
        "authorization" | "cookie" | "setcookie"
    )
}

fn header_style_value_end(chars: &[char], start: usize) -> usize {
    let mut value_end = start;
    while value_end < chars.len() && chars[value_end] != '\r' && chars[value_end] != '\n' {
        value_end += 1;
    }
    value_end
}

fn unclosed_quoted_value_boundary(chars: &[char], start: usize) -> usize {
    let mut value_end = start;
    while value_end < chars.len() && !is_unquoted_value_boundary(chars[value_end]) {
        value_end += 1;
    }
    value_end
}

fn is_escaped(chars: &[char], index: usize) -> bool {
    let mut backslash_count = 0;
    let mut cursor = index;
    while cursor > 0 && chars[cursor - 1] == '\\' {
        backslash_count += 1;
        cursor -= 1;
    }
    backslash_count % 2 == 1
}

fn push_chars(output: &mut String, chars: &[char]) {
    output.extend(chars.iter());
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

    #[test]
    fn redacts_compact_invalid_json_secret_text() {
        let input = r#"{"error":"bad","accessToken":"secret""#;

        let redacted = redact_monitor_text(input);

        assert!(redacted.contains(r#""error":"bad""#));
        assert!(!redacted.contains("secret"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_compact_json_like_secret_inside_context() {
        let input = r#"prefix {"ok":false,"api_key":"plain-secret"} suffix"#;

        let redacted = redact_monitor_text(input);

        assert!(redacted.contains("prefix"));
        assert!(redacted.contains(r#""ok":false"#));
        assert!(redacted.contains("suffix"));
        assert!(!redacted.contains("plain-secret"));
    }

    #[test]
    fn redacts_comma_delimited_secret_assignments() {
        let input = "session=abc,accessToken=secret";

        let redacted = redact_monitor_text(input);

        assert!(!redacted.contains("abc"));
        assert!(!redacted.contains("secret"));
        assert!(redacted.matches("[REDACTED]").count() >= 2);
    }

    #[test]
    fn redacts_authorization_header_value_with_spaces() {
        let input = "authorization: Bearer opaque-token";

        let redacted = redact_monitor_text(input);

        assert!(redacted.contains("authorization: "));
        assert!(!redacted.contains("Bearer"));
        assert!(!redacted.contains("opaque-token"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_case_insensitive_authorization_header_value() {
        let input = "Authorization: Bearer ghp_example";

        let redacted = redact_monitor_text(input);

        assert!(redacted.contains("Authorization: "));
        assert!(!redacted.contains("ghp_example"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_cookie_header_value_with_semicolon_segments() {
        let input = "cookie: a=b; c=d";

        let redacted = redact_monitor_text(input);

        assert!(redacted.contains("cookie: "));
        assert!(!redacted.contains("a=b"));
        assert!(!redacted.contains("c=d"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_secret_values_in_url_query_text() {
        let input = "https://x.test/path?accessToken=secret&session=abc";

        let redacted = redact_monitor_text(input);

        assert!(redacted.contains("https://x.test/path?"));
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("session=abc"));
        assert!(!redacted.contains("=abc"));
        assert!(redacted.matches("[REDACTED]").count() >= 2);
    }
}
