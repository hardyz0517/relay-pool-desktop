use serde::{Deserialize, Serialize};

#[cfg(test)]
use super::{
    identity::{EndpointRef, ModelName, StationId, StationKeyId},
    provenance::FactProvenance,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthState {
    Available,
    Degraded,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(test)]
pub struct StationKeyHealthTarget {
    station_key_id: StationKeyId,
}

#[cfg(test)]
impl StationKeyHealthTarget {
    pub fn new(station_key_id: StationKeyId) -> Self {
        Self { station_key_id }
    }

    pub fn station_key_id(&self) -> &StationKeyId {
        &self.station_key_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(test)]
pub struct StationAccountHealthTarget {
    station_id: StationId,
}

#[cfg(test)]
impl StationAccountHealthTarget {
    pub fn new(station_id: StationId) -> Self {
        Self { station_id }
    }

    pub fn station_id(&self) -> &StationId {
        &self.station_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(test)]
pub struct EndpointHealthTarget {
    endpoint_ref: EndpointRef,
}

#[cfg(test)]
impl EndpointHealthTarget {
    pub fn new(endpoint_ref: EndpointRef) -> Self {
        Self { endpoint_ref }
    }

    pub fn endpoint_ref(&self) -> &EndpointRef {
        &self.endpoint_ref
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(test)]
pub struct ModelHealthTarget {
    station_key_id: StationKeyId,
    model: ModelName,
}

#[cfg(test)]
impl ModelHealthTarget {
    pub fn new(station_key_id: StationKeyId, model: ModelName) -> Self {
        Self {
            station_key_id,
            model,
        }
    }

    pub fn station_key_id(&self) -> &StationKeyId {
        &self.station_key_id
    }

    pub fn model(&self) -> &ModelName {
        &self.model
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(test)]
pub struct HealthFact<T> {
    target: T,
    state: HealthState,
    provenance: FactProvenance,
}

#[cfg(test)]
impl<T> HealthFact<T> {
    pub fn new(target: T, state: HealthState, provenance: FactProvenance) -> Self {
        Self {
            target,
            state,
            provenance,
        }
    }

    pub fn target(&self) -> &T {
        &self.target
    }

    pub fn state(&self) -> HealthState {
        self.state
    }

    pub fn provenance(&self) -> &FactProvenance {
        &self.provenance
    }
}

#[cfg(test)]
pub type StationKeyHealthFact = HealthFact<StationKeyHealthTarget>;
#[cfg(test)]
pub type StationAccountHealthFact = HealthFact<StationAccountHealthTarget>;
#[cfg(test)]
pub type EndpointHealthFact = HealthFact<EndpointHealthTarget>;
#[cfg(test)]
pub type ModelHealthFact = HealthFact<ModelHealthTarget>;
