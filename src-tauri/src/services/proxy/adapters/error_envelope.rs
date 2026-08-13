use std::time::SystemTime;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::Value;

use crate::services::proxy::diagnostic_memory::{
    json_complexity, JsonComplexity, MAX_DIAGNOSTIC_JSON_BYTES, MAX_DIAGNOSTIC_JSON_DEPTH,
};

pub(crate) const MAX_ERROR_BODY_BYTES: usize = MAX_DIAGNOSTIC_JSON_BYTES;
pub(crate) const MAX_MESSAGE_SCAN_BYTES: usize = 16 * 1024;
pub(crate) const MAX_JSON_DEPTH: usize = MAX_DIAGNOSTIC_JSON_DEPTH;
pub(crate) const MAX_RETRY_AFTER_MS: i64 = 7 * 24 * 60 * 60 * 1_000;
pub(crate) const ENVELOPE_PROFILE_VERSION: &str = "openai-envelope-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailureTransport {
    Http,
    ChatSseError,
    ResponsesSseFailure,
}

impl FailureTransport {
    pub(super) fn is_sse_failure(self) -> bool {
        matches!(self, Self::ChatSseError | Self::ResponsesSseFailure)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodyCapture<'a> {
    Complete(&'a [u8]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodyKind {
    Empty,
    Json,
    NonJson,
    Html,
    ErrorBodyTooLarge,
    JsonTooDeep,
    JsonTooComplex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnvelopeShape {
    None,
    NestedError,
    TopLevelError,
    ResponsesFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorCodeKey {
    Missing,
    Unknown,
    Numeric(u16),
    ServerError,
    ServerIsOverloaded,
    SlowDown,
    InvalidApiKey,
    InvalidApiKeyFormat,
    AuthenticationError,
    AuthenticationFailed,
    PermissionDenied,
    InsufficientQuota,
    ModelNotFound,
    ModelNotAvailable,
    RateLimitExceeded,
    UpstreamError,
    ApiKeyAuthOverloaded,
    ConcurrencyLimitExceeded,
    GroupDeleted,
    GroupDisabled,
    GroupNotAllowed,
    SubscriptionNotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorTypeKey {
    Missing,
    Unknown,
    Error,
    ResponseFailed,
    ServerError,
    UpstreamError,
    OverloadedError,
    AuthenticationError,
    InvalidRequestError,
    RateLimitError,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct EvidenceFlags {
    pub message_scan_truncated: bool,
    pub unexpected_content_type: bool,
    pub error_on_success_status: bool,
    pub code_was_numeric: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedErrorEnvelope {
    pub status: u16,
    pub transport: FailureTransport,
    pub body_kind: BodyKind,
    pub shape: EnvelopeShape,
    pub code: ErrorCodeKey,
    pub error_type: ErrorTypeKey,
    pub retry_after_ms: Option<i64>,
    pub flags: EvidenceFlags,
    pub message_signature: Option<MessageSignature>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageSignature {
    OpenAiModelAtCapacityV1,
}

pub(crate) struct ErrorEnvelopeInput<'a> {
    pub status: u16,
    pub transport: FailureTransport,
    pub content_type: Option<&'a str>,
    pub body: BodyCapture<'a>,
    pub retry_after: Option<&'a str>,
    pub received_at: SystemTime,
}

pub(super) fn parse_error_envelope(input: ErrorEnvelopeInput<'_>) -> ParsedErrorEnvelope {
    let mut parsed = ParsedErrorEnvelope {
        status: input.status,
        transport: input.transport,
        body_kind: BodyKind::Empty,
        shape: EnvelopeShape::None,
        code: ErrorCodeKey::Missing,
        error_type: ErrorTypeKey::Missing,
        retry_after_ms: parse_retry_after_ms(input.retry_after, input.received_at),
        flags: EvidenceFlags::default(),
        message_signature: None,
    };

    let body = match input.body {
        BodyCapture::Complete(body) if body.len() > MAX_ERROR_BODY_BYTES => {
            parsed.body_kind = BodyKind::ErrorBodyTooLarge;
            return parsed;
        }
        BodyCapture::Complete(body) => body,
    };

    if body.is_empty() {
        return parsed;
    }

    parsed.flags.unexpected_content_type = input
        .content_type
        .is_some_and(|value| !is_json_content_type(value));

    match json_complexity(body) {
        JsonComplexity::WithinLimit => {}
        JsonComplexity::TooDeep => {
            parsed.body_kind = BodyKind::JsonTooDeep;
            return parsed;
        }
        JsonComplexity::TooComplex => {
            parsed.body_kind = BodyKind::JsonTooComplex;
            return parsed;
        }
    }

    let value = match serde_json::from_slice::<Value>(body) {
        Ok(value) => value,
        Err(_) => {
            parsed.body_kind = if looks_like_html(body) {
                BodyKind::Html
            } else {
                BodyKind::NonJson
            };
            return parsed;
        }
    };
    parsed.body_kind = BodyKind::Json;
    if json_depth(&value, 1) > MAX_JSON_DEPTH {
        parsed.body_kind = BodyKind::JsonTooDeep;
        return parsed;
    }

    let (shape, error) = locate_error_object(&value);
    parsed.shape = shape;
    parsed.flags.error_on_success_status =
        (200..300).contains(&input.status) && !matches!(shape, EnvelopeShape::None);

    let Some(error) = error else {
        return parsed;
    };
    parsed.code = normalize_code(error.get("code"), &mut parsed.flags);
    parsed.error_type = normalize_type(error.get("type"));
    if matches!(parsed.error_type, ErrorTypeKey::Missing)
        && matches!(shape, EnvelopeShape::ResponsesFailed)
    {
        parsed.error_type = ErrorTypeKey::ResponseFailed;
    }
    if let Some(message) = error.get("message").and_then(Value::as_str) {
        let (scan, truncated) = utf8_prefix(message, MAX_MESSAGE_SCAN_BYTES);
        parsed.flags.message_scan_truncated = truncated;
        parsed.message_signature =
            match_message_signature(input.status, input.transport, shape, scan);
    }
    parsed
}

/// Versioned capacity message signature matching. Only upstream responses
/// that already carried an envelope and a plausible status are scanned, and
/// the scan is bounded by the parser's UTF-8-safe message prefix.
fn match_message_signature(
    status: u16,
    transport: FailureTransport,
    shape: EnvelopeShape,
    message: &str,
) -> Option<MessageSignature> {
    if matches!(shape, EnvelopeShape::None) || !(status == 400 || transport.is_sse_failure()) {
        return None;
    }
    const OPENAI_CAPACITY_PREFIX: &str = "selected model is at capacity";
    message
        .get(..OPENAI_CAPACITY_PREFIX.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(OPENAI_CAPACITY_PREFIX))
        .map(|_| MessageSignature::OpenAiModelAtCapacityV1)
}

fn locate_error_object(value: &Value) -> (EnvelopeShape, Option<&serde_json::Map<String, Value>>) {
    let Some(root) = value.as_object() else {
        return (EnvelopeShape::None, None);
    };
    let root_type = root.get("type").and_then(Value::as_str);
    let response = root.get("response").and_then(Value::as_object);
    let responses_failed = root_type
        .is_some_and(|value| value.eq_ignore_ascii_case("response.failed"))
        || root
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|value| value.eq_ignore_ascii_case("failed"))
        || response
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str)
            .is_some_and(|value| value.eq_ignore_ascii_case("failed"));

    if responses_failed {
        let error = response
            .and_then(|value| value.get("error"))
            .and_then(Value::as_object)
            .or_else(|| root.get("error").and_then(Value::as_object));
        return (EnvelopeShape::ResponsesFailed, error.or(Some(root)));
    }
    if let Some(error) = root.get("error").and_then(Value::as_object) {
        return (EnvelopeShape::NestedError, Some(error));
    }
    let has_error_field = ["code", "message"]
        .iter()
        .any(|key| root.contains_key(*key));
    if has_error_field {
        return (EnvelopeShape::TopLevelError, Some(root));
    }
    (EnvelopeShape::None, None)
}

fn normalize_code(value: Option<&Value>, flags: &mut EvidenceFlags) -> ErrorCodeKey {
    let Some(value) = value else {
        return ErrorCodeKey::Missing;
    };
    if let Some(number) = value.as_u64().and_then(|number| u16::try_from(number).ok()) {
        flags.code_was_numeric = true;
        return ErrorCodeKey::Numeric(number);
    }
    let Some(value) = value.as_str() else {
        return ErrorCodeKey::Unknown;
    };
    if value.bytes().all(|byte| byte.is_ascii_digit()) {
        if let Ok(number) = value.parse::<u16>() {
            flags.code_was_numeric = true;
            return ErrorCodeKey::Numeric(number);
        }
    }
    match_known_code(value).unwrap_or(ErrorCodeKey::Unknown)
}

fn match_known_code(value: &str) -> Option<ErrorCodeKey> {
    let key = comparison_key(value)?;
    Some(match key.as_str() {
        "server_error" => ErrorCodeKey::ServerError,
        "server_is_overloaded" => ErrorCodeKey::ServerIsOverloaded,
        "slow_down" => ErrorCodeKey::SlowDown,
        "invalid_api_key" => ErrorCodeKey::InvalidApiKey,
        "invalid_api_key_format" => ErrorCodeKey::InvalidApiKeyFormat,
        "authentication_error" => ErrorCodeKey::AuthenticationError,
        "authentication_failed" => ErrorCodeKey::AuthenticationFailed,
        "permission_denied" => ErrorCodeKey::PermissionDenied,
        "insufficient_quota" => ErrorCodeKey::InsufficientQuota,
        "model_not_found" => ErrorCodeKey::ModelNotFound,
        "model_not_available" => ErrorCodeKey::ModelNotAvailable,
        "rate_limit_exceeded" => ErrorCodeKey::RateLimitExceeded,
        "upstream_error" => ErrorCodeKey::UpstreamError,
        "api_key_auth_overloaded" => ErrorCodeKey::ApiKeyAuthOverloaded,
        "concurrency_limit_exceeded" => ErrorCodeKey::ConcurrencyLimitExceeded,
        "group_deleted" => ErrorCodeKey::GroupDeleted,
        "group_disabled" => ErrorCodeKey::GroupDisabled,
        "group_not_allowed" => ErrorCodeKey::GroupNotAllowed,
        "subscription_not_found" => ErrorCodeKey::SubscriptionNotFound,
        _ => return None,
    })
}

fn normalize_type(value: Option<&Value>) -> ErrorTypeKey {
    let Some(value) = value.and_then(Value::as_str) else {
        return if value.is_some() {
            ErrorTypeKey::Unknown
        } else {
            ErrorTypeKey::Missing
        };
    };
    let Some(key) = comparison_key(value) else {
        return ErrorTypeKey::Unknown;
    };
    match key.as_str() {
        "error" => ErrorTypeKey::Error,
        "response.failed" => ErrorTypeKey::ResponseFailed,
        "server_error" => ErrorTypeKey::ServerError,
        "upstream_error" => ErrorTypeKey::UpstreamError,
        "overloaded_error" => ErrorTypeKey::OverloadedError,
        "authentication_error" => ErrorTypeKey::AuthenticationError,
        "invalid_request_error" => ErrorTypeKey::InvalidRequestError,
        "rate_limit_error" => ErrorTypeKey::RateLimitError,
        _ => ErrorTypeKey::Unknown,
    }
}

fn comparison_key(value: &str) -> Option<String> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return None;
    }
    Some(value.to_ascii_lowercase().replace('-', "_"))
}

fn is_json_content_type(value: &str) -> bool {
    let media_type = value.split(';').next().unwrap_or(value).trim();
    media_type.eq_ignore_ascii_case("application/json")
        || media_type.eq_ignore_ascii_case("text/json")
        || media_type.to_ascii_lowercase().ends_with("+json")
        || media_type.eq_ignore_ascii_case("text/event-stream")
}

fn looks_like_html(body: &[u8]) -> bool {
    let prefix = &body[..body.len().min(256)];
    let prefix = String::from_utf8_lossy(prefix)
        .trim_start()
        .to_ascii_lowercase();
    prefix.starts_with("<!doctype html") || prefix.starts_with("<html")
}

fn json_depth(value: &Value, depth: usize) -> usize {
    match value {
        Value::Array(values) => values
            .iter()
            .map(|value| json_depth(value, depth + 1))
            .max()
            .unwrap_or(depth),
        Value::Object(values) => values
            .values()
            .map(|value| json_depth(value, depth + 1))
            .max()
            .unwrap_or(depth),
        _ => depth,
    }
}

fn utf8_prefix(value: &str, limit: usize) -> (&str, bool) {
    if value.len() <= limit {
        return (value, false);
    }
    let mut boundary = limit;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    (&value[..boundary], true)
}

pub(crate) fn parse_retry_after_ms(value: Option<&str>, received_at: SystemTime) -> Option<i64> {
    let value = value?.trim();
    if let Ok(seconds) = value.parse::<i64>() {
        return (seconds > 0).then(|| seconds.saturating_mul(1_000).min(MAX_RETRY_AFTER_MS));
    }
    let retry_at = parse_http_date(value)?;
    let received_at: DateTime<Utc> = received_at.into();
    let delay = retry_at
        .signed_duration_since(received_at)
        .num_milliseconds();
    (delay > 0).then_some(delay.min(MAX_RETRY_AFTER_MS))
}

fn parse_http_date(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(value) = DateTime::parse_from_rfc2822(value) {
        return Some(value.with_timezone(&Utc));
    }
    // HTTP permits two obsolete wire formats for compatibility with old intermediaries.
    for format in ["%A, %d-%b-%y %H:%M:%S GMT", "%a %b %e %H:%M:%S %Y"] {
        if let Ok(value) = NaiveDateTime::parse_from_str(value, format) {
            return Some(value.and_utc());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::proxy::adapters::error_rules::{
        collect_upstream_failure_evidence_for_profile, EvidenceConfidence, ProviderRuleProfile,
        SemanticCandidate,
    };
    use std::time::{Duration, UNIX_EPOCH};

    fn parse(
        status: u16,
        content_type: Option<&str>,
        body: &[u8],
    ) -> super::super::error_rules::UpstreamFailureEvidence {
        collect_upstream_failure_evidence_for_profile(
            ErrorEnvelopeInput {
                status,
                transport: FailureTransport::Http,
                content_type,
                body: BodyCapture::Complete(body),
                retry_after: None,
                received_at: UNIX_EPOCH,
            },
            ProviderRuleProfile::Sub2ApiV1,
        )
    }

    #[test]
    fn parser_table_covers_envelopes_codes_content_and_status_boundaries() {
        struct Case<'a> {
            status: u16,
            content_type: Option<&'a str>,
            body: &'a [u8],
            shape: EnvelopeShape,
            kind: BodyKind,
            candidate: SemanticCandidate,
        }
        let cases = [
            Case { status: 400, content_type: Some("application/json"), body: br#"{"error":{"code":"SERVER_IS_OVERLOADED","type":"server_error","message":"busy"}}"#, shape: EnvelopeShape::NestedError, kind: BodyKind::Json, candidate: SemanticCandidate::ProviderCapacity },
            Case { status: 200, content_type: Some("text/event-stream"), body: br#"{"type":"response.failed","response":{"error":{"code":"server_error","message":"retry"}}}"#, shape: EnvelopeShape::ResponsesFailed, kind: BodyKind::Json, candidate: SemanticCandidate::ProviderServerFailure },
            Case { status: 503, content_type: Some("application/json"), body: br#"{"code":529,"message":"busy"}"#, shape: EnvelopeShape::TopLevelError, kind: BodyKind::Json, candidate: SemanticCandidate::ProviderServerFailure },
            Case { status: 529, content_type: None, body: b"", shape: EnvelopeShape::None, kind: BodyKind::Empty, candidate: SemanticCandidate::ProviderServerFailure },
            Case { status: 407, content_type: Some("text/html"), body: b"<html>proxy auth</html>", shape: EnvelopeShape::None, kind: BodyKind::Html, candidate: SemanticCandidate::OutboundProxyAuthentication },
            Case { status: 413, content_type: None, body: b"not json", shape: EnvelopeShape::None, kind: BodyKind::NonJson, candidate: SemanticCandidate::PayloadTooLarge },
            Case { status: 499, content_type: None, body: b"", shape: EnvelopeShape::None, kind: BodyKind::Empty, candidate: SemanticCandidate::ClientClosedRequest },
            Case { status: 302, content_type: None, body: b"", shape: EnvelopeShape::None, kind: BodyKind::Empty, candidate: SemanticCandidate::Redirect },
        ];
        for case in cases {
            let evidence = parse(case.status, case.content_type, case.body);
            assert_eq!(evidence.envelope, case.shape, "status {}", case.status);
            assert_eq!(evidence.body_kind, case.kind, "status {}", case.status);
            assert!(
                evidence.semantic_candidates.contains(&case.candidate),
                "status {} candidates {:?}",
                case.status,
                evidence.semantic_candidates
            );
        }
    }

    #[test]
    fn capacity_message_is_guarded_and_utf8_scan_is_bounded() {
        let suffix = "界".repeat(MAX_MESSAGE_SCAN_BYTES);
        let body =
            format!(r#"{{"error":{{"message":"Selected model is at capacity. {suffix}"}}}}"#);
        let evidence = parse(400, Some("application/json"), body.as_bytes());
        assert_eq!(
            evidence.message_signature,
            Some(MessageSignature::OpenAiModelAtCapacityV1)
        );
        assert!(evidence.flags.message_scan_truncated);
        assert_eq!(evidence.confidence, EvidenceConfidence::Confirmed);

        let wrong_status = parse(
            401,
            Some("application/json"),
            br#"{"error":{"message":"Selected model is at capacity"}}"#,
        );
        assert_eq!(wrong_status.message_signature, None);
        assert!(!wrong_status
            .semantic_candidates
            .contains(&SemanticCandidate::ProviderCapacity));

        let sse = collect_upstream_failure_evidence_for_profile(
            ErrorEnvelopeInput {
                status: 200,
                transport: FailureTransport::ResponsesSseFailure,
                content_type: Some("text/event-stream"),
                body: BodyCapture::Complete(
                    br#"{"type":"response.failed","response":{"error":{"message":"Selected model is at capacity"}}}"#,
                ),
                retry_after: None,
                received_at: UNIX_EPOCH,
            },
            ProviderRuleProfile::NativeOpenAiV1,
        );
        assert_eq!(
            sse.message_signature,
            Some(MessageSignature::OpenAiModelAtCapacityV1)
        );
        assert!(sse
            .semantic_candidates
            .contains(&SemanticCandidate::ProviderCapacity));
    }

    #[test]
    fn missing_fields_and_wrong_content_type_stay_closed() {
        let evidence = parse(400, Some("text/plain"), br#"{"error":{}}"#);
        assert_eq!(evidence.envelope, EnvelopeShape::NestedError);
        assert_eq!(evidence.code, ErrorCodeKey::Missing);
        assert_eq!(evidence.error_type, ErrorTypeKey::Missing);
        assert!(evidence.flags.unexpected_content_type);

        let no_error = parse(200, Some("application/json"), br#"{"id":"response-ok"}"#);
        assert_eq!(no_error.envelope, EnvelopeShape::None);
        assert_eq!(no_error.confidence, EvidenceConfidence::Unknown);
        assert!(no_error.semantic_candidates.is_empty());
    }

    #[test]
    fn arbitrary_code_message_authorization_and_secret_canary_never_escape_evidence() {
        let canary = "sk-test-super-secret-canary";
        let body = format!(
            r#"{{"error":{{"code":"{canary}","type":"Bearer","message":"Authorization: Bearer {canary}"}}}}"#
        );
        let evidence = parse(500, Some("application/json"), body.as_bytes());
        let debug = format!("{evidence:?}");
        assert!(!debug.contains(canary));
        assert!(!debug.contains("Authorization"));
        assert_eq!(evidence.code, ErrorCodeKey::Unknown);
        assert_eq!(evidence.error_type, ErrorTypeKey::Unknown);
        assert_eq!(evidence.message_signature, None);
    }

    #[test]
    fn depth_and_size_limits_cannot_create_durable_semantics() {
        let deep = format!(
            "{}0{}",
            "[".repeat(MAX_JSON_DEPTH + 1),
            "]".repeat(MAX_JSON_DEPTH + 1)
        );
        let evidence = parse(500, Some("application/json"), deep.as_bytes());
        assert_eq!(evidence.body_kind, BodyKind::JsonTooDeep);
        assert_eq!(evidence.code, ErrorCodeKey::Missing);

        let oversized = vec![b'x'; MAX_ERROR_BODY_BYTES + 1];
        let evidence = parse(500, None, &oversized);
        assert_eq!(evidence.body_kind, BodyKind::ErrorBodyTooLarge);
        assert_eq!(evidence.confidence, EvidenceConfidence::Unknown);
        assert!(evidence.semantic_candidates.is_empty());
    }

    #[test]
    fn invalid_utf8_and_structural_node_limit_remain_non_semantic() {
        let invalid_utf8 = b"{\"error\":{\"code\":\"server_error\",\"message\":\""
            .iter()
            .copied()
            .chain([0xff, 34, 125, 125])
            .collect::<Vec<_>>();
        let evidence = parse(500, Some("application/json"), &invalid_utf8);
        assert_eq!(evidence.body_kind, BodyKind::NonJson);
        assert_eq!(evidence.envelope, EnvelopeShape::None);
        assert_eq!(evidence.code, ErrorCodeKey::Missing);
        assert_eq!(evidence.message_signature, None);
        assert_eq!(evidence.confidence, EvidenceConfidence::Probable);

        let too_many_nodes = format!(
            "[{}]",
            std::iter::repeat_n(
                "0",
                crate::services::proxy::diagnostic_memory::MAX_DIAGNOSTIC_JSON_NODES
            )
            .collect::<Vec<_>>()
            .join(",")
        );
        let evidence = parse(500, Some("application/json"), too_many_nodes.as_bytes());
        assert_eq!(evidence.body_kind, BodyKind::JsonTooComplex);
        assert_eq!(evidence.envelope, EnvelopeShape::None);
        assert_eq!(evidence.code, ErrorCodeKey::Missing);
        assert_eq!(evidence.message_signature, None);
        assert_eq!(evidence.confidence, EvidenceConfidence::Unknown);
    }

    #[test]
    fn retry_after_supports_delta_and_http_date_with_absolute_cap() {
        let received = UNIX_EPOCH + Duration::from_secs(784_111_777);
        assert_eq!(parse_retry_after_ms(Some("12"), received), Some(12_000));
        assert_eq!(
            parse_retry_after_ms(Some("999999999"), received),
            Some(MAX_RETRY_AFTER_MS)
        );
        assert_eq!(
            parse_retry_after_ms(Some("Sun, 06 Nov 1994 08:49:49 GMT"), received),
            Some(12_000)
        );
        assert_eq!(
            parse_retry_after_ms(Some("Sunday, 06-Nov-94 08:49:49 GMT"), received),
            Some(12_000)
        );
        assert_eq!(
            parse_retry_after_ms(Some("Sun Nov  6 08:49:49 1994"), received),
            Some(12_000)
        );
        assert_eq!(parse_retry_after_ms(Some("0"), received), None);
        assert_eq!(parse_retry_after_ms(Some("invalid"), received), None);
    }

    #[test]
    fn bounded_parser_does_not_panic_for_deterministic_arbitrary_bytes() {
        let mut state = 0x9e37_79b9_u32;
        for len in 0..2048usize {
            let mut body = vec![0; len];
            for byte in &mut body {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                *byte = state as u8;
            }
            let evidence = parse(500, Some("application/octet-stream"), &body);
            assert!(evidence.semantic_candidates.len() <= 2);
        }
    }
}
