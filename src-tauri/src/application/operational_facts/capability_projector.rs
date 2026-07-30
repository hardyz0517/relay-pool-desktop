use std::cmp::Reverse;

use crate::models::operational::{
    CapabilityVerdict, EndpointRevision, EvidenceConfidence, EvidenceCoverage, UnixMillis,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CapabilityProtocol {
    ChatCompletions,
    Responses,
    Embeddings,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CapabilityFeature {
    Stream,
    Tools,
    Vision,
    Reasoning,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CapabilitySubject {
    Protocol(CapabilityProtocol),
    Feature(CapabilityFeature),
    Model(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CapabilityEvidenceSource {
    AdapterStructure,
    UserBlock,
    AdapterSemantic,
    SuccessfulRequest,
    CollectorInventory,
    UserAllow,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CanonicalCapabilityEvidence {
    pub(crate) id: String,
    pub(crate) subject: CapabilitySubject,
    pub(crate) verdict: CapabilityVerdict,
    pub(crate) source: CapabilityEvidenceSource,
    pub(crate) coverage: EvidenceCoverage,
    pub(crate) observed_at: UnixMillis,
    pub(crate) endpoint_revision: EndpointRevision,
    pub(crate) confidence: EvidenceConfidence,
    pub(crate) expires_at: Option<UnixMillis>,
}

impl CanonicalCapabilityEvidence {
    pub(crate) fn is_expired(&self, now: UnixMillis) -> bool {
        self.expires_at
            .map(|expires_at| expires_at.get() <= now.get())
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityDecision {
    Allow,
    Reject,
    RequireStrictConfirmation,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CapabilityProjectionPolicy {
    pub(crate) strict_unknown: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CapabilityProjection {
    pub(crate) subject: CapabilitySubject,
    pub(crate) truth: CapabilityVerdict,
    pub(crate) decision: CapabilityDecision,
    pub(crate) winner: Option<CanonicalCapabilityEvidence>,
    pub(crate) overridden: Vec<CanonicalCapabilityEvidence>,
    pub(crate) conflict_reason: Option<&'static str>,
}

pub(crate) fn project_capability(
    subject: CapabilitySubject,
    evidence: &[CanonicalCapabilityEvidence],
    policy: CapabilityProjectionPolicy,
    now: UnixMillis,
) -> CapabilityProjection {
    let mut relevant = evidence
        .iter()
        .filter(|item| item.subject == subject)
        .filter(|item| !item.is_expired(now))
        .filter_map(normalize_evidence_for_reduction)
        .collect::<Vec<_>>();

    relevant.sort_by_key(|item| {
        (
            precedence(item),
            Reverse(item.endpoint_revision.get()),
            Reverse(item.observed_at.get()),
            item.source,
            item.id.clone(),
        )
    });

    let winner = relevant.first().cloned();
    let overridden = relevant.iter().skip(1).cloned().collect::<Vec<_>>();
    let truth = winner
        .as_ref()
        .map(|item| item.verdict)
        .unwrap_or(CapabilityVerdict::Unknown);
    let decision = match truth {
        CapabilityVerdict::Supported => CapabilityDecision::Allow,
        CapabilityVerdict::Unsupported => CapabilityDecision::Reject,
        CapabilityVerdict::Unknown if policy.strict_unknown => {
            CapabilityDecision::RequireStrictConfirmation
        }
        CapabilityVerdict::Unknown => CapabilityDecision::Allow,
    };
    let conflict_reason = conflict_reason(winner.as_ref(), &overridden);

    CapabilityProjection {
        subject,
        truth,
        decision,
        winner,
        overridden,
        conflict_reason,
    }
}

fn normalize_evidence_for_reduction(
    evidence: &CanonicalCapabilityEvidence,
) -> Option<CanonicalCapabilityEvidence> {
    match (evidence.source, evidence.verdict, evidence.coverage) {
        (
            CapabilityEvidenceSource::CollectorInventory,
            CapabilityVerdict::Unsupported,
            EvidenceCoverage::Complete,
        ) => Some(evidence.clone()),
        (
            CapabilityEvidenceSource::CollectorInventory,
            CapabilityVerdict::Unsupported,
            EvidenceCoverage::Partial | EvidenceCoverage::Unknown,
        ) => None,
        (_, CapabilityVerdict::Unknown, _) => None,
        _ => Some(evidence.clone()),
    }
}

fn precedence(evidence: &CanonicalCapabilityEvidence) -> u8 {
    match (evidence.source, evidence.verdict) {
        (CapabilityEvidenceSource::AdapterStructure, CapabilityVerdict::Unsupported) => 0,
        (CapabilityEvidenceSource::UserBlock, CapabilityVerdict::Unsupported) => 1,
        (CapabilityEvidenceSource::AdapterSemantic, CapabilityVerdict::Unsupported) => 2,
        (CapabilityEvidenceSource::SuccessfulRequest, CapabilityVerdict::Supported) => 3,
        (CapabilityEvidenceSource::CollectorInventory, CapabilityVerdict::Unsupported) => 4,
        (CapabilityEvidenceSource::UserAllow, CapabilityVerdict::Supported) => 5,
        (CapabilityEvidenceSource::CollectorInventory, CapabilityVerdict::Supported) => 6,
        (CapabilityEvidenceSource::AdapterStructure, CapabilityVerdict::Supported) => 7,
        _ => 8,
    }
}

fn conflict_reason(
    winner: Option<&CanonicalCapabilityEvidence>,
    overridden: &[CanonicalCapabilityEvidence],
) -> Option<&'static str> {
    let winner = winner?;
    if overridden.is_empty() {
        return None;
    }
    if overridden.iter().any(|item| {
        item.verdict != winner.verdict && item.endpoint_revision == winner.endpoint_revision
    }) {
        Some("same_revision_conflict_resolved_by_precedence")
    } else {
        Some("lower_precedence_evidence_overridden")
    }
}
