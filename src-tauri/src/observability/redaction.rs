#![allow(
    dead_code,
    reason = "Task 18A freezes the redaction contract before production diagnostics are wired to it"
)]

use url::Url;

pub(crate) const DEFAULT_TEXT_PREVIEW_BYTES: usize = 512;
const REDACTED: &str = "[REDACTED]";
const TRUNCATED: &str = "[TRUNCATED]";

const SENSITIVE_MARKERS: &[&str] = &[
    "api_key",
    "apikey",
    "authorization",
    "bearer ",
    "cookie",
    "new-api-user",
    "password",
    "refresh_token",
    "set-cookie",
    "sk-",
    "token",
];

pub(crate) fn redact_text_preview(input: &str) -> String {
    redact_text_preview_with_limit(input, DEFAULT_TEXT_PREVIEW_BYTES)
}

pub(crate) fn redact_text_preview_with_limit(input: &str, max_bytes: usize) -> String {
    if contains_sensitive_marker(input) {
        return REDACTED.to_string();
    }
    bounded_prefix(input, max_bytes)
}

pub(crate) fn redact_url_preview(input: &str) -> String {
    let Ok(mut url) = Url::parse(input) else {
        return redact_text_preview(input);
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    url.set_path("/path-redacted");
    url.to_string()
}

fn contains_sensitive_marker(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    SENSITIVE_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

fn bounded_prefix(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }
    let mut output = String::new();
    for character in input.chars() {
        if output.len() + character.len_utf8() > max_bytes {
            break;
        }
        output.push(character);
    }
    output.push_str(TRUNCATED);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_redaction_removes_secret_canaries_and_bounds_preview() {
        assert_eq!(
            redact_text_preview("Authorization: Bearer sk-secret"),
            REDACTED
        );

        let preview = redact_text_preview_with_limit("abcdef", 3);
        assert_eq!(preview, "abc[TRUNCATED]");
    }

    #[test]
    fn url_redaction_strips_credentials_query_fragment_and_path() {
        let redacted =
            redact_url_preview("https://user:pass@example.test/path/to/item?token=secret#frag");

        assert_eq!(redacted, "https://example.test/path-redacted");
    }
}
