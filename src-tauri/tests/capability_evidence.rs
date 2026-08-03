use serde_json::Value;

#[path = "../src/models/operational/mod.rs"]
mod operational_model;

mod models {
    pub(crate) mod operational {
        pub(crate) use crate::operational_model::*;
    }
}

#[path = "../src/application/operational_facts/capability_projector.rs"]
mod capability_projector;

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
fn known_protocol_and_feature_subjects_default_to_allow_when_no_evidence_exists() {
    let subjects = [
        CapabilitySubject::Protocol(CapabilityProtocol::ChatCompletions),
        CapabilitySubject::Protocol(CapabilityProtocol::Embeddings),
        CapabilitySubject::Feature(CapabilityFeature::Stream),
        CapabilitySubject::Feature(CapabilityFeature::Reasoning),
    ];

    for subject in subjects {
        let projection = project_capability(subject, &[], default_policy(), now());
        assert_eq!(projection.truth, CapabilityVerdict::Unknown);
        assert_eq!(projection.decision, CapabilityDecision::Allow);
        assert!(projection.winner.is_none());
    }
}
