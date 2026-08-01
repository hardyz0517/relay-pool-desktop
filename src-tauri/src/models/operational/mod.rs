#![allow(unused_imports)]
#![allow(dead_code)]

pub mod capability;
pub mod economics;
pub mod health;
pub mod identity;
pub mod provenance;
pub(crate) mod raw_facts;

use serde::{Deserialize, Serialize};

pub use capability::{
    CapabilityDimension, CapabilityEvidence, CapabilityVerdict, RequestModelCapabilityAssessment,
    StationKeyCapabilityFacts,
};
pub use economics::{
    BalanceFacts, BalanceScope, CurrencyCode, EconomicsValidationError, Money, MoneyAmount,
    PriceConfidence, PricingUnit, RateMultiplier, RequestCostBasis, RequestPricingAssessment,
};
pub use health::{
    EndpointHealthFact, EndpointHealthTarget, HealthFact, HealthState, ModelHealthFact,
    ModelHealthTarget, StationAccountHealthFact, StationAccountHealthTarget, StationKeyHealthFact,
    StationKeyHealthTarget,
};
pub use identity::{
    EndpointFacts, EndpointId, EndpointRef, EndpointRevision, EvidenceHash, ModelName,
    OperationalValidationError, OutboundPolicyRef, RecordRevision, SanitizedOrigin,
    StationAccountRef, StationId, StationKeyId, UnixMillis,
};
pub use provenance::{
    EvidenceConfidence, EvidenceCoverage, EvidenceFreshness, EvidenceSource, FactProvenance,
};
pub(crate) use raw_facts::{
    OperationalFactReadOptions, RawOperationalCandidateRow, RawOperationalFactRows,
    RawOperationalModelAliasRow, RawOperationalSettingRow, MAX_OPERATIONAL_CANDIDATES,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
