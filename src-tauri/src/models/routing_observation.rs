use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum ObservationSource { RealRequest, ActiveProbe, Administrative }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum TrafficEquivalence { ExactRequest, SameModelShape, EndpointOnly, Anonymous }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum ObservationOutcome { Success, CredentialFailure, EndpointFailure, ModelFailure, RateLimited, Timeout, Cancelled, Unknown }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ObservationScope { pub station_id: Option<String>, pub station_key_id: Option<String>, pub model: Option<String>, pub endpoint_revision: Option<i64> }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ObservationOrder { pub producer_id: String, pub producer_sequence: u64, pub event_at_ms: i64, pub ingested_at_ms: i64 }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct RoutingObservation { pub id: String, pub order: ObservationOrder, pub scope: ObservationScope, pub source: ObservationSource, pub traffic_equivalence: TrafficEquivalence, pub outcome: ObservationOutcome, pub latency_ms: Option<u32>, pub evidence_mass_basis_points: u16 }
impl RoutingObservation { pub(crate) fn validate(&self) -> Result<(), &'static str> { if self.id.is_empty() || self.order.producer_id.is_empty() || self.order.event_at_ms < 0 || self.order.ingested_at_ms < 0 || self.evidence_mass_basis_points > 10_000 || self.latency_ms.is_some_and(|value| value > 3_600_000) { return Err("invalid routing observation"); } if matches!(self.traffic_equivalence, TrafficEquivalence::Anonymous) && matches!(self.source, ObservationSource::ActiveProbe) && matches!(self.outcome, ObservationOutcome::Success) { return Err("anonymous probe cannot produce model quality success evidence"); } Ok(()) } }
