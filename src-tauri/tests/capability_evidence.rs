#![allow(dead_code)]

use serde_json::Value;

#[path = "../src/models/operational/mod.rs"]
mod operational_model;

mod models {
    pub(crate) mod operational {
        pub(crate) use crate::operational_model::*;
    }
}

mod application {
    pub(crate) mod request_finalization {
        pub(crate) mod failure {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub(crate) enum CapabilityApplicabilitySet {
                ConfirmedModelCatalog,
                UnknownModelCatalog,
                PositiveCapabilityEvidence,
                LoadEvidenceGap,
                RequestPolicyOnly,
            }

            impl CapabilityApplicabilitySet {
                pub(crate) fn permits_model_not_found_learning(self) -> bool {
                    matches!(self, Self::ConfirmedModelCatalog)
                }
            }

            #[derive(Debug, Clone, PartialEq, Eq)]
            pub(crate) enum ProviderErrorSemanticSignal {
                ConfirmedAuthentication {
                    station_key_id: String,
                },
                ConfirmedInsufficientBalance {
                    station_id: String,
                },
                ConfirmedModelNotFound {
                    station_key_id: String,
                    model: String,
                },
                RateLimited {
                    station_id: String,
                    retry_after_ms: Option<i64>,
                },
                BadRequest,
                ServerError {
                    station_id: String,
                    endpoint_revision: i64,
                },
                GenericStatus {
                    status: u16,
                },
            }
        }
    }
}

#[path = "../src/services/proxy/adapters/capability.rs"]
mod capability;
#[path = "../src/application/operational_facts/capability_projector.rs"]
mod capability_projector;
#[path = "../src/services/proxy/adapters/openai.rs"]
mod openai_adapter;

mod services {
    pub(crate) mod time {
        pub(crate) fn now_millis_for_services() -> i64 {
            1_000
        }
    }
}

use capability::{
    model_signal_from_http_status, AdapterCapabilityProtocol, AdapterCapabilitySignal,
    AdapterCapabilitySubject, AdapterCapabilityVerdict, AdapterHttpCapabilityProfile,
};
use capability_projector::{
    project_capability, CanonicalCapabilityEvidence, CapabilityDecision, CapabilityEvidenceSource,
    CapabilityFeature, CapabilityProjectionPolicy, CapabilityProtocol, CapabilitySubject,
};
use operational_model::{
    CapabilityVerdict, EndpointRevision, EvidenceConfidence, EvidenceCoverage, UnixMillis,
};

fn now() -> UnixMillis {
    UnixMillis::new(1_000).expect("now")
}

fn revision(value: i64) -> EndpointRevision {
    EndpointRevision::new(value).expect("revision")
}

fn evidence(
    id: &str,
    subject: CapabilitySubject,
    verdict: CapabilityVerdict,
    source: CapabilityEvidenceSource,
    coverage: EvidenceCoverage,
) -> CanonicalCapabilityEvidence {
    CanonicalCapabilityEvidence {
        id: id.to_string(),
        subject,
        verdict,
        source,
        coverage,
        observed_at: now(),
        endpoint_revision: revision(1),
        confidence: EvidenceConfidence::new(1.0).expect("confidence"),
        expires_at: None,
    }
}

fn default_policy() -> CapabilityProjectionPolicy {
    CapabilityProjectionPolicy {
        strict_unknown: false,
    }
}

#[test]
fn precedence_fixture_documents_canonical_policy() {
    let fixture: Value =
        serde_json::from_slice(include_bytes!("fixtures/capability/precedence.v1.json"))
            .expect("fixture");

    assert_eq!(fixture["schemaVersion"], 1);
    assert!(fixture["policy"]
        .as_str()
        .expect("policy")
        .contains("adapter_structure > user_block"));
}

#[test]
fn adapter_structural_unsupported_is_not_overridden_by_user_allow_or_alias() {
    let subject = CapabilitySubject::Protocol(CapabilityProtocol::Responses);
    let projection = project_capability(
        subject.clone(),
        &[
            evidence(
                "adapter-unsupported",
                subject.clone(),
                CapabilityVerdict::Unsupported,
                CapabilityEvidenceSource::AdapterStructure,
                EvidenceCoverage::Complete,
            ),
            evidence(
                "user-allow",
                subject.clone(),
                CapabilityVerdict::Supported,
                CapabilityEvidenceSource::UserAllow,
                EvidenceCoverage::Complete,
            ),
        ],
        default_policy(),
        now(),
    );

    assert_eq!(projection.truth, CapabilityVerdict::Unsupported);
    assert_eq!(projection.decision, CapabilityDecision::Reject);
    assert_eq!(
        projection.winner.expect("winner").source,
        CapabilityEvidenceSource::AdapterStructure
    );
}

#[test]
fn user_block_wins_over_scoped_allow_and_collector_positive() {
    let subject = CapabilitySubject::Feature(CapabilityFeature::Tools);
    let projection = project_capability(
        subject.clone(),
        &[
            evidence(
                "allow",
                subject.clone(),
                CapabilityVerdict::Supported,
                CapabilityEvidenceSource::UserAllow,
                EvidenceCoverage::Complete,
            ),
            evidence(
                "collector",
                subject.clone(),
                CapabilityVerdict::Supported,
                CapabilityEvidenceSource::CollectorInventory,
                EvidenceCoverage::Complete,
            ),
            evidence(
                "block",
                subject.clone(),
                CapabilityVerdict::Unsupported,
                CapabilityEvidenceSource::UserBlock,
                EvidenceCoverage::Complete,
            ),
        ],
        default_policy(),
        now(),
    );

    assert_eq!(projection.truth, CapabilityVerdict::Unsupported);
    assert_eq!(
        projection.winner.expect("winner").source,
        CapabilityEvidenceSource::UserBlock
    );
}

#[test]
fn complete_inventory_missing_model_can_be_negative_but_partial_inventory_cannot() {
    let subject = CapabilitySubject::Model("gpt-4.1".to_string());
    let partial_projection = project_capability(
        subject.clone(),
        &[evidence(
            "partial-missing",
            subject.clone(),
            CapabilityVerdict::Unsupported,
            CapabilityEvidenceSource::CollectorInventory,
            EvidenceCoverage::Partial,
        )],
        default_policy(),
        now(),
    );
    assert_eq!(partial_projection.truth, CapabilityVerdict::Unknown);
    assert_eq!(partial_projection.decision, CapabilityDecision::Allow);

    let complete_projection = project_capability(
        subject.clone(),
        &[evidence(
            "complete-missing",
            subject.clone(),
            CapabilityVerdict::Unsupported,
            CapabilityEvidenceSource::CollectorInventory,
            EvidenceCoverage::Complete,
        )],
        default_policy(),
        now(),
    );
    assert_eq!(complete_projection.truth, CapabilityVerdict::Unsupported);
    assert_eq!(complete_projection.decision, CapabilityDecision::Reject);
}

#[test]
fn rate_limit_overload_and_generic_403_404_do_not_write_model_unsupported() {
    for status in [429, 503] {
        let signal = model_signal_from_http_status(
            AdapterHttpCapabilityProfile::OpenAiKnown,
            "gpt-4.1",
            status,
        );
        assert_eq!(signal.verdict, AdapterCapabilityVerdict::Neutral);
    }

    for status in [403, 404] {
        let signal = model_signal_from_http_status(
            AdapterHttpCapabilityProfile::GenericOpenAiCompatible,
            "gpt-4.1",
            status,
        );
        assert_eq!(signal.verdict, AdapterCapabilityVerdict::Uncertain);
    }
}

#[test]
fn same_revision_conflicts_are_resolved_by_stable_precedence_not_input_order() {
    let subject = CapabilitySubject::Model("gpt-4.1".to_string());
    let success = evidence(
        "success",
        subject.clone(),
        CapabilityVerdict::Supported,
        CapabilityEvidenceSource::SuccessfulRequest,
        EvidenceCoverage::Complete,
    );
    let semantic_negative = evidence(
        "adapter-negative",
        subject.clone(),
        CapabilityVerdict::Unsupported,
        CapabilityEvidenceSource::AdapterSemantic,
        EvidenceCoverage::Complete,
    );
    let collector_positive = evidence(
        "collector-positive",
        subject.clone(),
        CapabilityVerdict::Supported,
        CapabilityEvidenceSource::CollectorInventory,
        EvidenceCoverage::Complete,
    );

    let forward = project_capability(
        subject.clone(),
        &[
            success.clone(),
            collector_positive.clone(),
            semantic_negative.clone(),
        ],
        default_policy(),
        now(),
    );
    let reverse = project_capability(
        subject.clone(),
        &[collector_positive, semantic_negative, success],
        default_policy(),
        now(),
    );

    assert_eq!(forward.truth, CapabilityVerdict::Unsupported);
    assert_eq!(reverse.truth, CapabilityVerdict::Unsupported);
    assert_eq!(
        forward.conflict_reason,
        Some("same_revision_conflict_resolved_by_precedence")
    );
    assert_eq!(
        reverse.winner.expect("winner").source,
        CapabilityEvidenceSource::AdapterSemantic
    );
}

#[test]
fn strict_policy_turns_unknown_into_hard_admission_without_rewriting_truth() {
    let subject = CapabilitySubject::Feature(CapabilityFeature::Vision);
    let projection = project_capability(
        subject,
        &[],
        CapabilityProjectionPolicy {
            strict_unknown: true,
        },
        now(),
    );

    assert_eq!(projection.truth, CapabilityVerdict::Unknown);
    assert_eq!(
        projection.decision,
        CapabilityDecision::RequireStrictConfirmation
    );
    assert!(projection.winner.is_none());
}

#[test]
fn openai_chat_adapter_returns_explicit_structural_capability_signals() {
    let signals = openai_adapter::chat_completions_capability_signals();

    assert!(signals.iter().any(|signal| {
        signal.subject
            == AdapterCapabilitySubject::Protocol(AdapterCapabilityProtocol::ChatCompletions)
            && signal.verdict == AdapterCapabilityVerdict::Supported
    }));
    assert!(signals.iter().any(|signal| {
        signal.subject == AdapterCapabilitySubject::Protocol(AdapterCapabilityProtocol::Responses)
            && signal.verdict == AdapterCapabilityVerdict::Unsupported
    }));
}

#[test]
fn adapter_signal_type_is_provider_neutral() {
    let signal = AdapterCapabilitySignal::semantic(
        AdapterCapabilitySubject::Model {
            model: "gpt-4.1".to_string(),
        },
        AdapterCapabilityVerdict::Unsupported,
        "fixture",
    );

    assert_eq!(signal.reason, "fixture");
}
