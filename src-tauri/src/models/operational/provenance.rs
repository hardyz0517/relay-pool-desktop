use serde::{Deserialize, Serialize};

use super::identity::{EvidenceHash, RecordRevision, UnixMillis};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceCoverage {
    Complete,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceFreshness {
    Fresh,
    Stale,
    Expired,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceSource {
    ManualConfig,
    Collector,
    MonitoringProbe,
    RequestOutcome,
    SystemDefault,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactProvenance {
    source: EvidenceSource,
    record_revision: RecordRevision,
    observed_at: UnixMillis,
    freshness: EvidenceFreshness,
    evidence_hash: Option<EvidenceHash>,
}

impl FactProvenance {
    pub fn new(
        source: EvidenceSource,
        record_revision: RecordRevision,
        observed_at: UnixMillis,
        freshness: EvidenceFreshness,
    ) -> Self {
        Self {
            source,
            record_revision,
            observed_at,
            freshness,
            evidence_hash: None,
        }
    }

    pub fn with_hash(mut self, evidence_hash: EvidenceHash) -> Self {
        self.evidence_hash = Some(evidence_hash);
        self
    }

    pub fn source(&self) -> &EvidenceSource {
        &self.source
    }

    pub fn record_revision(&self) -> RecordRevision {
        self.record_revision
    }

    pub fn observed_at(&self) -> UnixMillis {
        self.observed_at
    }

    pub fn freshness(&self) -> EvidenceFreshness {
        self.freshness
    }

    pub fn evidence_hash(&self) -> Option<&EvidenceHash> {
        self.evidence_hash.as_ref()
    }
}
