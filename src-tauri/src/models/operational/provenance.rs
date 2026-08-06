#[cfg(test)]
use serde::{Deserialize, Serialize};

#[cfg(test)]
use super::identity::{EvidenceHash, OperationalValidationError, RecordRevision, UnixMillis};

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceCoverage {
    Complete,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(test)]
pub enum EvidenceFreshness {
    Fresh,
    Stale,
    Expired,
    Unknown,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct EvidenceConfidence(f64);

#[cfg(test)]
impl EvidenceConfidence {
    #[cfg(test)]
    pub fn new(value: f64) -> Result<Self, OperationalValidationError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(OperationalValidationError::InvalidConfidence {
                field: "evidence",
                value,
            });
        }
        Ok(Self(value))
    }

    #[cfg(test)]
    pub fn get(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(test)]
pub enum EvidenceSource {
    ManualConfig,
    Collector,
    MonitoringProbe,
    RequestOutcome,
    SystemDefault,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(test)]
pub struct FactProvenance {
    source: EvidenceSource,
    record_revision: RecordRevision,
    observed_at: UnixMillis,
    freshness: EvidenceFreshness,
    evidence_hash: Option<EvidenceHash>,
}

#[cfg(test)]
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
