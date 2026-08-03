use crate::models::monitoring::FailureKind;

pub fn classify_http_status(status: u16) -> Option<FailureKind> {
    match status {
        200..=299 => None,
        400 | 422 => Some(FailureKind::InvalidRequest),
        401 | 403 => Some(FailureKind::Auth),
        429 => Some(FailureKind::RateLimit),
        500..=599 => Some(FailureKind::ServerError),
        400..=499 => Some(FailureKind::ClientError),
        _ => Some(FailureKind::ProtocolMismatch),
    }
}

#[cfg(test)]
pub fn parse_json_probe_response(
    protocol_kind: ProtocolKind,
    status: u16,
    content_type: Option<&str>,
    body: &[u8],
    validator: &ChallengeValidator,
    limits: ResponseLimits,
) -> ParsedProbeResponse {
    if body.len() > limits.max_response_bytes {
        return ParsedProbeResponse::unavailable(
            protocol_kind,
            Some(status),
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    if let Some(failure_kind) = classify_http_status(status) {
        return ParsedProbeResponse::unavailable(
            protocol_kind,
            Some(status),
            failure_kind,
            body.len(),
        );
    }
    if body.is_empty() {
        return ParsedProbeResponse::unavailable(
            protocol_kind,
            Some(status),
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    if !content_type
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("json")
    {
        return ParsedProbeResponse::unavailable(
            protocol_kind,
            Some(status),
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    let value = match serde_json::from_slice::<Value>(body) {
        Ok(value) => value,
        Err(_) => {
            return ParsedProbeResponse::unavailable(
                protocol_kind,
                Some(status),
                FailureKind::ProtocolMismatch,
                body.len(),
            );
        }
    };
    if value.get("error").is_some() {
        return ParsedProbeResponse::unavailable(
            protocol_kind,
            Some(status),
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    let output_text = extract_text_fields(&value);
    validate_output_text(
        protocol_kind,
        Some(status),
        output_text,
        body.len(),
        validator,
        limits,
    )
}

#[cfg(test)]
use crate::{
    models::monitoring::ProtocolKind,
    services::monitoring::{
        adapters::contract::{
            extract_text_fields, validate_output_text, ParsedProbeResponse, ResponseLimits,
        },
        challenge::ChallengeValidator,
    },
};
#[cfg(test)]
use serde_json::Value;

#[cfg(test)]
mod tests {
    use crate::{
        models::monitoring::{FailureKind, ProbeOutcome, ProtocolKind},
        services::monitoring::{
            adapters::{contract::ResponseLimits, http_mapping::parse_json_probe_response},
            challenge::ChallengeValidator,
        },
    };

    #[test]
    fn services_monitoring_http_200_error_json_fails_closed() {
        let parsed = parse_json_probe_response(
            ProtocolKind::OpenAiChat,
            200,
            Some("application/json"),
            br#"{"error":{"message":"bad"}}"#,
            &ChallengeValidator::from_expected_answer_for_tests("RP_ANSWER=42"),
            ResponseLimits::default(),
        );

        assert_eq!(parsed.outcome, ProbeOutcome::Unavailable);
        assert_eq!(parsed.failure_kind, Some(FailureKind::ProtocolMismatch));
    }
}
