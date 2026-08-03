use serde::{Deserialize, Serialize};

#[cfg(test)]
use super::{
    identity::ModelName,
    provenance::{EvidenceCoverage, FactProvenance},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityVerdict {
    Supported,
    Unsupported,
    Unknown,
}

impl CapabilityVerdict {
    #[cfg(test)]
    pub fn is_supported(self) -> bool {
        matches!(self, Self::Supported)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(test)]
pub enum CapabilityDimension {
    Tools,
    Vision,
    Reasoning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(test)]
pub struct CapabilityEvidence {
    dimension: CapabilityDimension,
    verdict: CapabilityVerdict,
    coverage: EvidenceCoverage,
    provenance: FactProvenance,
}

#[cfg(test)]
impl CapabilityEvidence {
    pub fn new(
        dimension: CapabilityDimension,
        verdict: CapabilityVerdict,
        coverage: EvidenceCoverage,
        provenance: FactProvenance,
    ) -> Self {
        Self {
            dimension,
            verdict,
            coverage,
            provenance,
        }
    }

    pub fn dimension(&self) -> CapabilityDimension {
        self.dimension
    }

    pub fn verdict(&self) -> CapabilityVerdict {
        self.verdict
    }

    pub fn coverage(&self) -> EvidenceCoverage {
        self.coverage
    }

    pub fn provenance(&self) -> &FactProvenance {
        &self.provenance
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(test)]
pub struct StationKeyCapabilityFacts {
    tools: CapabilityEvidence,
    vision: CapabilityEvidence,
    reasoning: CapabilityEvidence,
}

#[cfg(test)]
impl StationKeyCapabilityFacts {
    pub fn new(
        tools: CapabilityEvidence,
        vision: CapabilityEvidence,
        reasoning: CapabilityEvidence,
    ) -> Self {
        Self {
            tools,
            vision,
            reasoning,
        }
    }

    pub fn tools(&self) -> &CapabilityEvidence {
        &self.tools
    }

    pub fn vision(&self) -> &CapabilityEvidence {
        &self.vision
    }

    pub fn reasoning(&self) -> &CapabilityEvidence {
        &self.reasoning
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(test)]
pub struct RequestModelCapabilityAssessment {
    model: ModelName,
    verdict: CapabilityVerdict,
    coverage: EvidenceCoverage,
    provenance: FactProvenance,
}

#[cfg(test)]
impl RequestModelCapabilityAssessment {
    pub fn new(
        model: ModelName,
        verdict: CapabilityVerdict,
        coverage: EvidenceCoverage,
        provenance: FactProvenance,
    ) -> Self {
        Self {
            model,
            verdict,
            coverage,
            provenance,
        }
    }

    pub fn model(&self) -> &ModelName {
        &self.model
    }

    pub fn verdict(&self) -> CapabilityVerdict {
        self.verdict
    }

    pub fn coverage(&self) -> EvidenceCoverage {
        self.coverage
    }

    pub fn provenance(&self) -> &FactProvenance {
        &self.provenance
    }
}
