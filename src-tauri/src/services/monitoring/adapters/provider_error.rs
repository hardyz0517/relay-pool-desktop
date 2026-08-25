use std::time::UNIX_EPOCH;

use crate::{
    models::monitoring::{FailureKind, ProtocolKind},
    services::proxy::adapters::{
        error_envelope::{BodyCapture, ErrorEnvelopeInput, FailureTransport},
        error_rules::{
            collect_upstream_failure_evidence_for_profile, ProviderRuleProfile, SemanticCandidate,
        },
    },
};

/// Classifies a bounded provider error envelope before the normal HTTP status
/// mapping runs.  A successful HTTP status is not proof of a successful probe:
/// several gateways return an error envelope with status 200, and streaming
/// protocols can emit the same envelope after headers have been received.
pub(crate) fn classify_provider_error(
    protocol: ProtocolKind,
    status: u16,
    content_type: Option<&str>,
    body: &[u8],
    transport: FailureTransport,
) -> Option<FailureKind> {
    let profile = match protocol {
        ProtocolKind::OpenAiChat | ProtocolKind::OpenAiResponses => {
            ProviderRuleProfile::GenericOpenAiCompatibleV1
        }
        _ => ProviderRuleProfile::GenericOpenAiCompatibleV1,
    };
    let evidence = collect_upstream_failure_evidence_for_profile(
        ErrorEnvelopeInput {
            status,
            transport,
            content_type,
            body: BodyCapture::Complete(body),
            retry_after: None,
            received_at: UNIX_EPOCH,
        },
        profile,
    );
    if !matches!(
        evidence.confidence,
        crate::services::proxy::adapters::error_rules::EvidenceConfidence::Confirmed
    ) {
        return None;
    }
    if evidence
        .semantic_candidates
        .contains(&SemanticCandidate::InsufficientQuota)
    {
        return Some(FailureKind::BudgetExceeded);
    }
    if evidence
        .semantic_candidates
        .contains(&SemanticCandidate::GroupSubscriptionInvalid)
    {
        return Some(FailureKind::BudgetExceeded);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_http_200_insufficient_quota_to_business_failure() {
        assert_eq!(
            classify_provider_error(
                ProtocolKind::OpenAiChat,
                200,
                Some("application/json"),
                br#"{"error":{"code":"insufficient_quota","type":"insufficient_quota"}}"#,
                FailureTransport::Http,
            ),
            Some(FailureKind::BudgetExceeded)
        );
    }

    #[test]
    fn leaves_unknown_success_envelope_unclassified() {
        assert_eq!(
            classify_provider_error(
                ProtocolKind::OpenAiResponses,
                200,
                Some("application/json"),
                br#"{"error":{"code":"vendor_specific"}}"#,
                FailureTransport::Http,
            ),
            None
        );
    }
}
