pub mod capability;
pub mod economics;
pub mod health;
pub mod identity;
pub mod provenance;
pub(crate) mod raw_facts;

#[cfg(test)]
use serde::{Deserialize, Serialize};

pub use capability::CapabilityVerdict;
#[cfg(test)]
pub use capability::{
    CapabilityDimension, CapabilityEvidence, RequestModelCapabilityAssessment,
    StationKeyCapabilityFacts,
};
#[cfg(test)]
pub use economics::{
    BalanceFacts, CurrencyCode, Money, MoneyAmount, PricingUnit, RequestCostBasis,
    RequestPricingAssessment,
};
pub use economics::{BalanceScope, PriceConfidence, RateMultiplier};
#[cfg(test)]
pub(crate) use economics::EconomicsValidationError;
#[cfg(not(test))]
pub use health::HealthState;
#[cfg(test)]
pub use health::{
    EndpointHealthFact, EndpointHealthTarget, HealthFact, HealthState, ModelHealthFact,
    ModelHealthTarget, StationAccountHealthFact, StationAccountHealthTarget, StationKeyHealthFact,
    StationKeyHealthTarget,
};
pub use identity::SanitizedOrigin;
pub use identity::{EndpointFacts, OutboundPolicyRef};
#[cfg(test)]
pub use identity::{EvidenceHash, StationAccountRef};
pub use identity::{
    EndpointId, EndpointRef, EndpointRevision, ModelName,
    RecordRevision, StationId, StationKeyId,
};
pub use identity::{OperationalValidationError, UnixMillis};
#[cfg(test)]
pub use provenance::{EvidenceConfidence, EvidenceCoverage};
#[cfg(test)]
pub use provenance::{EvidenceFreshness, EvidenceSource, FactProvenance};
pub(crate) use raw_facts::MAX_OPERATIONAL_CANDIDATES;
pub(crate) use raw_facts::{
    OperationalFactReadOptions, RawOperationalCandidateRow, RawOperationalFactRows,
    RawOperationalModelAliasRow, RawOperationalSettingRow,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg(test)]
pub struct StationKeyOperationalFacts {
    station_key_id: StationKeyId,
    station_id: StationId,
    station_account: StationAccountRef,
    endpoint: EndpointFacts,
    capability: StationKeyCapabilityFacts,
    balance: BalanceFacts,
    key_health: StationKeyHealthFact,
    station_account_health: StationAccountHealthFact,
    endpoint_health: EndpointHealthFact,
}

#[cfg(test)]
impl StationKeyOperationalFacts {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        station_key_id: StationKeyId,
        station_id: StationId,
        station_account: StationAccountRef,
        endpoint: EndpointFacts,
        capability: StationKeyCapabilityFacts,
        balance: BalanceFacts,
        key_health: StationKeyHealthFact,
        station_account_health: StationAccountHealthFact,
        endpoint_health: EndpointHealthFact,
    ) -> Self {
        Self {
            station_key_id,
            station_id,
            station_account,
            endpoint,
            capability,
            balance,
            key_health,
            station_account_health,
            endpoint_health,
        }
    }

    pub fn station_key_id(&self) -> &StationKeyId {
        &self.station_key_id
    }

    pub fn station_id(&self) -> &StationId {
        &self.station_id
    }

    pub fn station_account(&self) -> &StationAccountRef {
        &self.station_account
    }

    pub fn endpoint(&self) -> &EndpointFacts {
        &self.endpoint
    }

    pub fn capability(&self) -> &StationKeyCapabilityFacts {
        &self.capability
    }

    pub fn balance(&self) -> &BalanceFacts {
        &self.balance
    }

    pub fn key_health(&self) -> &StationKeyHealthFact {
        &self.key_health
    }

    pub fn station_account_health(&self) -> &StationAccountHealthFact {
        &self.station_account_health
    }

    pub fn endpoint_health(&self) -> &EndpointHealthFact {
        &self.endpoint_health
    }
}
