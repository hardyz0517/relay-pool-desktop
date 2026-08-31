use std::time::SystemTime;

use serde_json::{json, Value};

use crate::application::request_finalization::failure::{
    CapabilityApplicabilitySet, EvidenceConfidence as CanonicalEvidenceConfidence,
    ProviderErrorSemanticSignal, ProviderProtocolKind,
};

use super::{
    error_envelope::{
        BodyCapture, EnvelopeShape, ErrorEnvelopeInput, ErrorTypeKey, FailureTransport,
    },
    error_rules::{
        collect_upstream_failure_evidence_for_profile, EvidenceConfidence, ProviderRuleProfile,
        SemanticCandidate, UpstreamFailureEvidence,
    },
};

pub fn generate_response_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}",
        crate::services::time::now_millis_for_services()
    )
}

pub fn extract_choice_text(value: &Value) -> Option<String> {
    value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub fn wrap_chat_response_as_responses(value: Value, fallback_model: Option<&str>) -> Value {
    let content = extract_choice_text(&value).unwrap_or_default();
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .or(fallback_model)
        .unwrap_or("unknown-model");
    let created = value
        .get("created")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| (crate::services::time::now_millis_for_services() / 1000) as i64);
    let usage = value.get("usage").cloned().unwrap_or(Value::Null);

    json!({
        "id": value.get("id").cloned().unwrap_or_else(|| Value::String(generate_response_id("response"))),
        "object": "response",
        "created": created,
        "model": model,
        "output": [{
            "id": generate_response_id("output"),
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "output_text",
                "text": content,
            }],
        }],
        "output_text": content,
        "usage": usage,
    })
}

pub(crate) fn openai_error_semantic_signal(
    status: u16,
    body: Option<&Value>,
    station_key_id: &str,
    station_id: &str,
    endpoint_revision: i64,
    model: Option<&str>,
    applicability: CapabilityApplicabilitySet,
) -> ProviderErrorSemanticSignal {
    let encoded = body.and_then(|body| serde_json::to_vec(body).ok());
    openai_error_semantic_signal_from_capture(
        status,
        encoded.as_deref().map(BodyCapture::Complete),
        encoded.as_ref().map(|_| "application/json"),
        None,
        station_key_id,
        station_id,
        endpoint_revision,
        model,
        applicability,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn openai_error_semantic_signal_from_capture(
    status: u16,
    body: Option<BodyCapture<'_>>,
    content_type: Option<&str>,
    retry_after: Option<&str>,
    station_key_id: &str,
    station_id: &str,
    endpoint_revision: i64,
    model: Option<&str>,
    applicability: CapabilityApplicabilitySet,
) -> ProviderErrorSemanticSignal {
    openai_error_semantic_signal_from_capture_for_profile(
        status,
        body,
        content_type,
        retry_after,
        station_key_id,
        station_id,
        endpoint_revision,
        model,
        applicability,
        ProviderRuleProfile::GenericOpenAiCompatibleV1,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn openai_error_semantic_signal_from_capture_for_profile(
    status: u16,
    body: Option<BodyCapture<'_>>,
    content_type: Option<&str>,
    retry_after: Option<&str>,
    station_key_id: &str,
    station_id: &str,
    endpoint_revision: i64,
    model: Option<&str>,
    applicability: CapabilityApplicabilitySet,
    rule_profile: ProviderRuleProfile,
    group_binding_id: Option<&str>,
) -> ProviderErrorSemanticSignal {
    let evidence = collect_upstream_failure_evidence_for_profile(
        ErrorEnvelopeInput {
            status,
            transport: FailureTransport::Http,
            content_type,
            body: body.unwrap_or(BodyCapture::Complete(&[])),
            retry_after,
            received_at: SystemTime::now(),
        },
        rule_profile,
    );
    openai_semantic_signal_from_evidence(
        &evidence,
        station_key_id,
        station_id,
        endpoint_revision,
        model,
        applicability,
        group_binding_id,
    )
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) fn openai_sse_error_semantic_signal_from_capture(
    transport: FailureTransport,
    body: BodyCapture<'_>,
    retry_after: Option<&str>,
    station_key_id: &str,
    station_id: &str,
    endpoint_revision: i64,
    model: Option<&str>,
    applicability: CapabilityApplicabilitySet,
) -> ProviderErrorSemanticSignal {
    openai_sse_error_semantic_signal_from_capture_for_profile(
        transport,
        body,
        retry_after,
        station_key_id,
        station_id,
        endpoint_revision,
        model,
        applicability,
        ProviderRuleProfile::GenericOpenAiCompatibleV1,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn openai_sse_error_semantic_signal_from_capture_for_profile(
    transport: FailureTransport,
    body: BodyCapture<'_>,
    retry_after: Option<&str>,
    station_key_id: &str,
    station_id: &str,
    endpoint_revision: i64,
    model: Option<&str>,
    applicability: CapabilityApplicabilitySet,
    rule_profile: ProviderRuleProfile,
    group_binding_id: Option<&str>,
) -> ProviderErrorSemanticSignal {
    debug_assert!(transport.is_sse_failure());
    let evidence = collect_upstream_failure_evidence_for_profile(
        ErrorEnvelopeInput {
            // SSE failures arrive after a successful HTTP handshake. The typed
            // transport plus the envelope, rather than a fabricated HTTP status,
            // is the authoritative protocol evidence.
            status: 200,
            transport,
            content_type: Some("application/json"),
            body,
            retry_after,
            received_at: SystemTime::now(),
        },
        rule_profile,
    );
    openai_semantic_signal_from_evidence(
        &evidence,
        station_key_id,
        station_id,
        endpoint_revision,
        model,
        applicability,
        group_binding_id,
    )
}

/// The sole adapter from normalized HTTP/SSE evidence into the application
/// semantic vocabulary. Consumers must not re-read status/code/message after
/// this boundary.
pub(crate) fn openai_semantic_signal_from_evidence(
    evidence: &UpstreamFailureEvidence,
    station_key_id: &str,
    station_id: &str,
    endpoint_revision: i64,
    model: Option<&str>,
    applicability: CapabilityApplicabilitySet,
    group_binding_id: Option<&str>,
) -> ProviderErrorSemanticSignal {
    let confirmed = evidence.confidence == EvidenceConfidence::Confirmed;

    if confirmed
        && evidence
            .semantic_candidates
            .contains(&SemanticCandidate::GroupSubscriptionInvalid)
    {
        return group_binding_id.map_or(
            ProviderErrorSemanticSignal::GenericStatus {
                status: evidence.status,
                confidence: canonical_confidence(evidence.confidence),
            },
            |group_binding_id| ProviderErrorSemanticSignal::ConfirmedGroupSubscriptionInvalid {
                station_id: station_id.to_string(),
                group_binding_id: group_binding_id.to_string(),
            },
        );
    }
    if confirmed
        && evidence
            .semantic_candidates
            .contains(&SemanticCandidate::Authentication)
    {
        return ProviderErrorSemanticSignal::ConfirmedAuthentication {
            station_key_id: station_key_id.to_string(),
        };
    }
    if confirmed
        && evidence
            .semantic_candidates
            .contains(&SemanticCandidate::InsufficientQuota)
    {
        // The current application vocabulary has no quota target. RateLimited
        // is intentionally the non-destructive fallback until Task 3 adds the
        // typed quota signal; it must never become a credential hard-fail.
        return ProviderErrorSemanticSignal::RateLimited {
            station_id: station_id.to_string(),
            retry_after_ms: evidence.retry_after_ms,
        };
    }
    if confirmed
        && evidence
            .semantic_candidates
            .contains(&SemanticCandidate::ModelUnavailable)
        && applicability.permits_model_not_found_learning()
    {
        return ProviderErrorSemanticSignal::ConfirmedModelNotFound {
            station_key_id: station_key_id.to_string(),
            model: model.unwrap_or("unknown").to_string(),
        };
    }
    if confirmed
        && evidence
            .semantic_candidates
            .contains(&SemanticCandidate::ProviderCapacity)
    {
        return ProviderErrorSemanticSignal::Overloaded;
    }
    if evidence
        .semantic_candidates
        .contains(&SemanticCandidate::RelayAuthenticationOverloaded)
    {
        return ProviderErrorSemanticSignal::Overloaded;
    }
    if evidence
        .semantic_candidates
        .contains(&SemanticCandidate::RuntimeConcurrencyLimited)
        || evidence
            .semantic_candidates
            .contains(&SemanticCandidate::RateLimited)
    {
        return ProviderErrorSemanticSignal::RateLimited {
            station_id: station_id.to_string(),
            retry_after_ms: evidence.retry_after_ms,
        };
    }
    if matches!(evidence.status, 405 | 501)
        && !matches!(evidence.envelope, EnvelopeShape::None)
        && matches!(evidence.error_type, ErrorTypeKey::InvalidRequestError)
    {
        return ProviderErrorSemanticSignal::ConfirmedCapabilityMismatch {
            protocol: ProviderProtocolKind::Unknown,
        };
    }
    if evidence
        .semantic_candidates
        .contains(&SemanticCandidate::PayloadTooLarge)
        || evidence
            .semantic_candidates
            .contains(&SemanticCandidate::ProviderRequestRejected)
    {
        return ProviderErrorSemanticSignal::BadRequest;
    }
    if evidence
        .semantic_candidates
        .contains(&SemanticCandidate::ProviderServerFailure)
        || (500..=599).contains(&evidence.status)
    {
        return ProviderErrorSemanticSignal::ServerError {
            station_id: station_id.to_string(),
            endpoint_revision,
        };
    }
    ProviderErrorSemanticSignal::GenericStatus {
        status: evidence.status,
        confidence: canonical_confidence(evidence.confidence),
    }
}

fn canonical_confidence(confidence: EvidenceConfidence) -> CanonicalEvidenceConfidence {
    match confidence {
        EvidenceConfidence::Confirmed => CanonicalEvidenceConfidence::Confirmed,
        EvidenceConfidence::Probable => CanonicalEvidenceConfidence::Probable,
        EvidenceConfidence::Unknown => CanonicalEvidenceConfidence::Unknown,
        EvidenceConfidence::Conflicting => CanonicalEvidenceConfidence::Conflicting,
    }
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use serde_json::json;

    use super::{
        openai_error_semantic_signal, openai_error_semantic_signal_from_capture,
        openai_error_semantic_signal_from_capture_for_profile,
        openai_semantic_signal_from_evidence, openai_sse_error_semantic_signal_from_capture,
        openai_sse_error_semantic_signal_from_capture_for_profile,
    };
    use crate::application::request_finalization::failure::{
        CapabilityApplicabilitySet, EvidenceConfidence as CanonicalEvidenceConfidence,
        ProviderErrorSemanticSignal,
    };
    use crate::services::proxy::adapters::{
        error_envelope::{BodyCapture, ErrorEnvelopeInput, FailureTransport},
        error_rules::{collect_upstream_failure_evidence_for_profile, ProviderRuleProfile},
    };

    #[test]
    fn ordinary_429_remains_rate_limited() {
        let body = json!({"error": {"message": "Too many requests"}});
        let signal = openai_error_semantic_signal(
            429,
            Some(&body),
            "key-test",
            "station-test",
            1,
            Some("gpt-test"),
            CapabilityApplicabilitySet::UnknownModelCatalog,
        );
        assert!(matches!(
            signal,
            ProviderErrorSemanticSignal::RateLimited { .. }
        ));
    }

    #[test]
    fn generic_529_is_not_globally_treated_as_capacity() {
        let signal = openai_error_semantic_signal(
            529,
            None,
            "key-test",
            "station-test",
            1,
            Some("gpt-test"),
            CapabilityApplicabilitySet::UnknownModelCatalog,
        );
        assert_eq!(
            signal,
            ProviderErrorSemanticSignal::ServerError {
                station_id: "station-test".to_string(),
                endpoint_revision: 1,
            }
        );
    }

    #[test]
    fn trusted_model_not_found_is_typed_but_untrusted_404_is_not() {
        let body = json!({"error": {"code": "model_not_found"}});
        let trusted = openai_error_semantic_signal(
            404,
            Some(&body),
            "key-test",
            "station-test",
            1,
            Some("gpt-test"),
            CapabilityApplicabilitySet::ConfirmedModelCatalog,
        );
        assert!(matches!(
            trusted,
            ProviderErrorSemanticSignal::ConfirmedModelNotFound { .. }
        ));

        let untrusted = openai_error_semantic_signal(
            404,
            Some(&body),
            "key-test",
            "station-test",
            1,
            Some("gpt-test"),
            CapabilityApplicabilitySet::UnknownModelCatalog,
        );
        assert_eq!(
            untrusted,
            ProviderErrorSemanticSignal::GenericStatus {
                status: 404,
                confidence: CanonicalEvidenceConfidence::Confirmed,
            }
        );
    }

    #[test]
    fn upstream_request_rejections_are_typed_separately_from_local_parse_errors() {
        for status in [400, 409, 422] {
            let signal = openai_error_semantic_signal(
                status,
                None,
                "key-test",
                "station-test",
                1,
                Some("gpt-test"),
                CapabilityApplicabilitySet::UnknownModelCatalog,
            );
            assert_eq!(signal, ProviderErrorSemanticSignal::BadRequest);
        }
    }

    #[test]
    fn capability_status_requires_a_protocol_error_envelope() {
        let trusted = openai_error_semantic_signal(
            405,
            Some(&json!({"error": {"type": "invalid_request_error"}})),
            "key-test",
            "station-test",
            1,
            Some("gpt-test"),
            CapabilityApplicabilitySet::UnknownModelCatalog,
        );
        assert!(matches!(
            trusted,
            ProviderErrorSemanticSignal::ConfirmedCapabilityMismatch { .. }
        ));

        let unknown = openai_error_semantic_signal(
            405,
            None,
            "key-test",
            "station-test",
            1,
            Some("gpt-test"),
            CapabilityApplicabilitySet::UnknownModelCatalog,
        );
        assert_eq!(
            unknown,
            ProviderErrorSemanticSignal::GenericStatus {
                status: 405,
                confidence: CanonicalEvidenceConfidence::Unknown,
            }
        );
    }

    #[test]
    fn confirmed_capacity_is_an_ordinary_key_overload_signal() {
        let evidence = collect_upstream_failure_evidence_for_profile(
            ErrorEnvelopeInput {
                status: 400,
                transport: FailureTransport::Http,
                content_type: Some("application/json"),
                body: BodyCapture::Complete(
                    br#"{"error":{"message":"Selected model is at capacity. Please try again later."}}"#,
                ),
                retry_after: Some("2"),
                received_at: UNIX_EPOCH,
            },
            ProviderRuleProfile::NativeOpenAiV1,
        );
        let signal = openai_semantic_signal_from_evidence(
            &evidence,
            "key-test",
            "station-test",
            1,
            Some("gpt-test"),
            CapabilityApplicabilitySet::UnknownModelCatalog,
            None,
        );
        assert_eq!(signal, ProviderErrorSemanticSignal::Overloaded);
    }

    #[test]
    fn conflicting_server_status_and_credential_code_never_blocks_the_key() {
        let signal = openai_error_semantic_signal(
            500,
            Some(&json!({"error": {"code": "invalid_api_key"}})),
            "key-test",
            "station-test",
            7,
            Some("gpt-test"),
            CapabilityApplicabilitySet::UnknownModelCatalog,
        );
        assert_eq!(
            signal,
            ProviderErrorSemanticSignal::ServerError {
                station_id: "station-test".to_string(),
                endpoint_revision: 7,
            }
        );
    }

    #[test]
    fn bounded_raw_capture_preserves_retry_after_without_reparsing_headers_downstream() {
        let signal = openai_error_semantic_signal_from_capture(
            429,
            Some(BodyCapture::Complete(
                br#"{"error":{"type":"rate_limit_error","message":"retry"}}"#,
            )),
            Some("application/json"),
            Some("17"),
            "key-test",
            "station-test",
            1,
            Some("gpt-test"),
            CapabilityApplicabilitySet::UnknownModelCatalog,
        );
        assert_eq!(
            signal,
            ProviderErrorSemanticSignal::RateLimited {
                station_id: "station-test".to_string(),
                retry_after_ms: Some(17_000),
            }
        );
    }

    #[test]
    fn sse_capacity_and_rate_limit_use_the_same_evidence_adapter_as_http() {
        let capacity = openai_sse_error_semantic_signal_from_capture_for_profile(
            FailureTransport::ResponsesSseFailure,
            BodyCapture::Complete(
                br#"{"type":"response.failed","response":{"error":{"code":"slow_down","message":"Please retry later."}}}"#,
            ),
            None,
            "key-test",
            "station-test",
            1,
            Some("gpt-test"),
            CapabilityApplicabilitySet::ConfirmedModelCatalog,
            ProviderRuleProfile::Sub2ApiV1,
            None,
        );
        assert_eq!(capacity, ProviderErrorSemanticSignal::Overloaded);

        let rate = openai_sse_error_semantic_signal_from_capture(
            FailureTransport::ChatSseError,
            BodyCapture::Complete(
                br#"{"error":{"type":"rate_limit_error","code":"rate_limit_exceeded","message":"retry"}}"#,
            ),
            Some("3"),
            "key-test",
            "station-test",
            1,
            Some("gpt-test"),
            CapabilityApplicabilitySet::ConfirmedModelCatalog,
        );
        assert_eq!(
            rate,
            ProviderErrorSemanticSignal::RateLimited {
                station_id: "station-test".to_string(),
                retry_after_ms: Some(3_000),
            }
        );
    }

    #[test]
    fn sub2api_group_subscription_error_requires_binding_identity() {
        for code in [
            "GROUP_DELETED",
            "GROUP_DISABLED",
            "GROUP_NOT_ALLOWED",
            "SUBSCRIPTION_NOT_FOUND",
        ] {
            let encoded = format!(r#"{{"error":{{"code":"{code}"}}}}"#);
            let typed = openai_error_semantic_signal_from_capture_for_profile(
                403,
                Some(BodyCapture::Complete(encoded.as_bytes())),
                Some("application/json"),
                None,
                "key-test",
                "station-test",
                1,
                Some("gpt-test"),
                CapabilityApplicabilitySet::ConfirmedModelCatalog,
                ProviderRuleProfile::Sub2ApiV1,
                Some("group-binding-test"),
            );
            assert_eq!(
                typed,
                ProviderErrorSemanticSignal::ConfirmedGroupSubscriptionInvalid {
                    station_id: "station-test".to_string(),
                    group_binding_id: "group-binding-test".to_string(),
                }
            );
        }

        let body = BodyCapture::Complete(
            br#"{"error":{"code":"SUBSCRIPTION_NOT_FOUND","message":"No active subscription found for this group"}}"#,
        );
        let neutral = openai_error_semantic_signal_from_capture_for_profile(
            403,
            Some(body),
            Some("application/json"),
            None,
            "key-test",
            "station-test",
            1,
            Some("gpt-test"),
            CapabilityApplicabilitySet::ConfirmedModelCatalog,
            ProviderRuleProfile::Sub2ApiV1,
            None,
        );
        assert_eq!(
            neutral,
            ProviderErrorSemanticSignal::GenericStatus {
                status: 403,
                confidence: CanonicalEvidenceConfidence::Confirmed,
            }
        );
    }
}
