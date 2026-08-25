use super::error_envelope::{
    parse_error_envelope, BodyKind, EnvelopeShape, ErrorCodeKey, ErrorEnvelopeInput, ErrorTypeKey,
    EvidenceFlags, FailureTransport, MessageSignature, ParsedErrorEnvelope,
    ENVELOPE_PROFILE_VERSION,
};

pub(crate) const ERROR_RULE_SET_VERSION: &str = "openai-compatible-errors-v1";
pub(crate) const MESSAGE_SIGNATURE_REGISTRY_VERSION: &str = "sub2api-signatures-v1";

/// Immutable provider semantics selected when an execution target is resolved.
/// A provider-shaped string is never sufficient to opt a generic gateway into
/// provider-specific retry or durable-health behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderRuleProfile {
    GenericOpenAiCompatibleV1,
    NativeOpenAiV1,
    Sub2ApiV1,
}

impl ProviderRuleProfile {
    fn permits_openai_capacity_signature(self) -> bool {
        matches!(self, Self::NativeOpenAiV1 | Self::Sub2ApiV1)
    }

    fn permits_server_is_overloaded(self) -> bool {
        matches!(self, Self::NativeOpenAiV1 | Self::Sub2ApiV1)
    }

    fn permits_slow_down(self) -> bool {
        matches!(self, Self::Sub2ApiV1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvidenceConfidence {
    Confirmed,
    Probable,
    Unknown,
    Conflicting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConflictReasonCode {
    CredentialCodeOnUpstreamServerFailure,
    CapacityCodeOnAuthenticationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SemanticCandidate {
    Authentication,
    PermissionDenied,
    InsufficientQuota,
    ModelUnavailable,
    RateLimited,
    ProviderCapacity,
    ProviderServerFailure,
    ProviderRequestRejected,
    RelayAuthenticationOverloaded,
    RuntimeConcurrencyLimited,
    Redirect,
    OutboundProxyAuthentication,
    RequestTimeout,
    TooEarly,
    PayloadTooLarge,
    ClientClosedRequest,
    GroupSubscriptionInvalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpstreamFailureEvidence {
    pub profile_version: &'static str,
    pub rule_set_version: &'static str,
    pub message_registry_version: &'static str,
    pub status: u16,
    pub transport: FailureTransport,
    pub body_kind: BodyKind,
    pub envelope: EnvelopeShape,
    pub code: ErrorCodeKey,
    pub error_type: ErrorTypeKey,
    pub message_signature: Option<MessageSignature>,
    pub retry_after_ms: Option<i64>,
    pub confidence: EvidenceConfidence,
    pub conflict_reason: Option<ConflictReasonCode>,
    pub semantic_candidates: Vec<SemanticCandidate>,
    pub flags: EvidenceFlags,
}

#[cfg(test)]
pub(crate) fn collect_upstream_failure_evidence(
    input: ErrorEnvelopeInput<'_>,
) -> UpstreamFailureEvidence {
    collect_upstream_failure_evidence_for_profile(
        input,
        ProviderRuleProfile::GenericOpenAiCompatibleV1,
    )
}

pub(crate) fn collect_upstream_failure_evidence_for_profile(
    input: ErrorEnvelopeInput<'_>,
    profile: ProviderRuleProfile,
) -> UpstreamFailureEvidence {
    classify(parse_error_envelope(input), profile)
}

fn classify(parsed: ParsedErrorEnvelope, profile: ProviderRuleProfile) -> UpstreamFailureEvidence {
    let mut candidates = Vec::with_capacity(2);
    let mut confidence = EvidenceConfidence::Unknown;
    let mut conflict_reason = None;

    if matches!(
        parsed.body_kind,
        BodyKind::ErrorBodyTooLarge | BodyKind::JsonTooDeep | BodyKind::JsonTooComplex
    ) {
        return evidence(parsed, candidates, confidence, conflict_reason);
    }

    match parsed.status {
        300..=399 => push_unique(&mut candidates, SemanticCandidate::Redirect),
        401 => push_unique(&mut candidates, SemanticCandidate::Authentication),
        403 => push_unique(&mut candidates, SemanticCandidate::PermissionDenied),
        402 => push_unique(&mut candidates, SemanticCandidate::InsufficientQuota),
        407 => push_unique(
            &mut candidates,
            SemanticCandidate::OutboundProxyAuthentication,
        ),
        408 => push_unique(&mut candidates, SemanticCandidate::RequestTimeout),
        425 => push_unique(&mut candidates, SemanticCandidate::TooEarly),
        413 => push_unique(&mut candidates, SemanticCandidate::PayloadTooLarge),
        429 => push_unique(&mut candidates, SemanticCandidate::RateLimited),
        499 => push_unique(&mut candidates, SemanticCandidate::ClientClosedRequest),
        400 | 409 | 422 => push_unique(&mut candidates, SemanticCandidate::ProviderRequestRejected),
        500..=599 => push_unique(&mut candidates, SemanticCandidate::ProviderServerFailure),
        _ => {}
    }

    let credential_code = matches!(
        parsed.code,
        ErrorCodeKey::InvalidApiKey
            | ErrorCodeKey::InvalidApiKeyFormat
            | ErrorCodeKey::AuthenticationError
            | ErrorCodeKey::AuthenticationFailed
    );
    let capacity_code = (profile.permits_server_is_overloaded()
        && matches!(parsed.code, ErrorCodeKey::ServerIsOverloaded))
        || (profile.permits_slow_down() && matches!(parsed.code, ErrorCodeKey::SlowDown))
        || (profile.permits_openai_capacity_signature()
            && matches!(
                parsed.message_signature,
                Some(MessageSignature::OpenAiModelAtCapacityV1)
            ));

    if (500..=599).contains(&parsed.status) && credential_code {
        confidence = EvidenceConfidence::Conflicting;
        conflict_reason = Some(ConflictReasonCode::CredentialCodeOnUpstreamServerFailure);
    } else if matches!(parsed.status, 401 | 403) && capacity_code {
        confidence = EvidenceConfidence::Conflicting;
        conflict_reason = Some(ConflictReasonCode::CapacityCodeOnAuthenticationStatus);
    } else if capacity_code {
        candidates.retain(|candidate| {
            !matches!(
                candidate,
                SemanticCandidate::ProviderRequestRejected
                    | SemanticCandidate::ProviderServerFailure
            )
        });
        push_unique(&mut candidates, SemanticCandidate::ProviderCapacity);
        confidence = EvidenceConfidence::Confirmed;
    } else {
        let trusted_sse_failure =
            parsed.transport.is_sse_failure() && !matches!(parsed.shape, EnvelopeShape::None);
        match parsed.code {
            ErrorCodeKey::InvalidApiKey
            | ErrorCodeKey::InvalidApiKeyFormat
            | ErrorCodeKey::AuthenticationError
            | ErrorCodeKey::AuthenticationFailed
                if matches!(parsed.status, 401 | 403) || trusted_sse_failure =>
            {
                candidates.clear();
                candidates.push(SemanticCandidate::Authentication);
                confidence = EvidenceConfidence::Confirmed;
            }
            ErrorCodeKey::PermissionDenied if parsed.status == 403 || trusted_sse_failure => {
                confidence = EvidenceConfidence::Confirmed;
            }
            ErrorCodeKey::InsufficientQuota
                if matches!(parsed.status, 402 | 429)
                    || trusted_sse_failure
                    || parsed.flags.error_on_success_status =>
            {
                candidates.clear();
                candidates.push(SemanticCandidate::InsufficientQuota);
                confidence = EvidenceConfidence::Confirmed;
            }
            ErrorCodeKey::ModelNotFound | ErrorCodeKey::ModelNotAvailable
                if matches!(parsed.status, 400 | 404) || trusted_sse_failure =>
            {
                candidates.clear();
                candidates.push(SemanticCandidate::ModelUnavailable);
                confidence = EvidenceConfidence::Confirmed;
            }
            ErrorCodeKey::RateLimitExceeded if parsed.status == 429 || trusted_sse_failure => {
                candidates.retain(|candidate| {
                    !matches!(candidate, SemanticCandidate::ProviderServerFailure)
                });
                push_unique(&mut candidates, SemanticCandidate::RateLimited);
                confidence = EvidenceConfidence::Confirmed;
            }
            ErrorCodeKey::ApiKeyAuthOverloaded
                if matches!(parsed.status, 429 | 503) || trusted_sse_failure =>
            {
                candidates.clear();
                candidates.push(SemanticCandidate::RelayAuthenticationOverloaded);
                confidence = EvidenceConfidence::Confirmed;
            }
            ErrorCodeKey::ConcurrencyLimitExceeded
                if parsed.status == 429 || trusted_sse_failure =>
            {
                candidates.clear();
                candidates.push(SemanticCandidate::RuntimeConcurrencyLimited);
                confidence = EvidenceConfidence::Confirmed;
            }
            ErrorCodeKey::GroupDeleted
            | ErrorCodeKey::GroupDisabled
            | ErrorCodeKey::GroupNotAllowed
            | ErrorCodeKey::SubscriptionNotFound
                if profile == ProviderRuleProfile::Sub2ApiV1
                    && (parsed.status == 403 || trusted_sse_failure) =>
            {
                candidates.clear();
                candidates.push(SemanticCandidate::GroupSubscriptionInvalid);
                confidence = EvidenceConfidence::Confirmed;
            }
            ErrorCodeKey::ServerError | ErrorCodeKey::UpstreamError
                if (500..=599).contains(&parsed.status) || parsed.flags.error_on_success_status =>
            {
                push_unique(&mut candidates, SemanticCandidate::ProviderServerFailure);
                confidence = EvidenceConfidence::Confirmed;
            }
            _ if !candidates.is_empty() => confidence = EvidenceConfidence::Probable,
            _ => {}
        }
    }

    evidence(parsed, candidates, confidence, conflict_reason)
}

fn evidence(
    parsed: ParsedErrorEnvelope,
    semantic_candidates: Vec<SemanticCandidate>,
    confidence: EvidenceConfidence,
    conflict_reason: Option<ConflictReasonCode>,
) -> UpstreamFailureEvidence {
    UpstreamFailureEvidence {
        profile_version: ENVELOPE_PROFILE_VERSION,
        rule_set_version: ERROR_RULE_SET_VERSION,
        message_registry_version: MESSAGE_SIGNATURE_REGISTRY_VERSION,
        status: parsed.status,
        transport: parsed.transport,
        body_kind: parsed.body_kind,
        envelope: parsed.shape,
        code: parsed.code,
        error_type: parsed.error_type,
        message_signature: parsed.message_signature,
        retry_after_ms: parsed.retry_after_ms,
        confidence,
        conflict_reason,
        semantic_candidates,
        flags: parsed.flags,
    }
}

fn push_unique(candidates: &mut Vec<SemanticCandidate>, candidate: SemanticCandidate) {
    if !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::proxy::adapters::error_envelope::MAX_ERROR_BODY_BYTES;
    use crate::services::proxy::adapters::error_envelope::{BodyCapture, ErrorEnvelopeInput};
    use std::time::UNIX_EPOCH;

    fn evidence(status: u16, body: &[u8]) -> UpstreamFailureEvidence {
        collect_upstream_failure_evidence(ErrorEnvelopeInput {
            status,
            transport: FailureTransport::Http,
            content_type: Some("application/json"),
            body: BodyCapture::Complete(body),
            retry_after: None,
            received_at: UNIX_EPOCH,
        })
    }

    #[test]
    fn five_xx_credential_code_is_conflicting_not_a_key_failure() {
        let evidence = evidence(500, br#"{"error":{"code":"invalid_api_key"}}"#);
        assert_eq!(evidence.confidence, EvidenceConfidence::Conflicting);
        assert_eq!(
            evidence.conflict_reason,
            Some(ConflictReasonCode::CredentialCodeOnUpstreamServerFailure)
        );
        assert_eq!(
            evidence.semantic_candidates,
            vec![SemanticCandidate::ProviderServerFailure]
        );
    }

    #[test]
    fn valid_two_xx_error_envelope_is_not_success() {
        let evidence = evidence(
            200,
            br#"{"error":{"code":"server_error","message":"retry"}}"#,
        );
        assert!(evidence.flags.error_on_success_status);
        assert_eq!(evidence.confidence, EvidenceConfidence::Confirmed);
        assert_eq!(
            evidence.semantic_candidates,
            vec![SemanticCandidate::ProviderServerFailure]
        );
    }

    #[test]
    fn over_limit_body_has_no_semantic_or_durable_candidate() {
        let too_large = collect_upstream_failure_evidence(ErrorEnvelopeInput {
            status: 400,
            transport: FailureTransport::Http,
            content_type: Some("application/json"),
            body: BodyCapture::Complete(&vec![b'x'; MAX_ERROR_BODY_BYTES + 1]),
            retry_after: None,
            received_at: UNIX_EPOCH,
        });
        assert_eq!(too_large.body_kind, BodyKind::ErrorBodyTooLarge);
        assert_eq!(too_large.confidence, EvidenceConfidence::Unknown);
        assert!(too_large.semantic_candidates.is_empty());
    }

    #[test]
    fn provider_shaped_capacity_tokens_are_closed_for_generic_gateways() {
        for (status, body) in [
            (
                503,
                br#"{"error":{"code":"server_is_overloaded"}}"#.as_slice(),
            ),
            (429, br#"{"error":{"code":"slow_down"}}"#.as_slice()),
            (529, br#"{"error":{"type":"overloaded_error"}}"#.as_slice()),
        ] {
            let generic = collect_upstream_failure_evidence_for_profile(
                ErrorEnvelopeInput {
                    status,
                    transport: FailureTransport::Http,
                    content_type: Some("application/json"),
                    body: BodyCapture::Complete(body),
                    retry_after: None,
                    received_at: UNIX_EPOCH,
                },
                ProviderRuleProfile::GenericOpenAiCompatibleV1,
            );
            assert!(!generic
                .semantic_candidates
                .contains(&SemanticCandidate::ProviderCapacity));
            assert_ne!(generic.confidence, EvidenceConfidence::Confirmed);
        }
    }

    #[test]
    fn profile_capacity_rules_are_explicit_and_versioned() {
        let classified = |profile, status, body: &'static [u8]| {
            collect_upstream_failure_evidence_for_profile(
                ErrorEnvelopeInput {
                    status,
                    transport: FailureTransport::Http,
                    content_type: Some("application/json"),
                    body: BodyCapture::Complete(body),
                    retry_after: None,
                    received_at: UNIX_EPOCH,
                },
                profile,
            )
        };
        for evidence in [
            classified(
                ProviderRuleProfile::Sub2ApiV1,
                503,
                br#"{"error":{"code":"server_is_overloaded"}}"#,
            ),
            classified(
                ProviderRuleProfile::Sub2ApiV1,
                429,
                br#"{"error":{"code":"slow_down"}}"#,
            ),
        ] {
            assert_eq!(evidence.confidence, EvidenceConfidence::Confirmed);
            assert!(evidence
                .semantic_candidates
                .contains(&SemanticCandidate::ProviderCapacity));
        }
    }
}
