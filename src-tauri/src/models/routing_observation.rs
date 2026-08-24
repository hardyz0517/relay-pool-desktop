use serde::{Deserialize, Serialize};

use crate::application::health_protection::HealthProtectionScope;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum ObservationSource {
    RealRequest,
    ActiveProbe,
    Administrative,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum TrafficEquivalence {
    ExactRequest,
    SameModelShape,
    EndpointOnly,
    Anonymous,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum ObservationOutcome {
    Success,
    CredentialFailure,
    EndpointFailure,
    ModelFailure,
    RateLimited,
    Timeout,
    Cancelled,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ObservationScope {
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub model: Option<String>,
    pub endpoint_revision: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ObservationOrder {
    pub producer_id: String,
    pub producer_sequence: u64,
    pub event_at_ms: i64,
    pub ingested_at_ms: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct RoutingObservation {
    pub id: String,
    pub order: ObservationOrder,
    pub scope: ObservationScope,
    pub source: ObservationSource,
    pub traffic_equivalence: TrafficEquivalence,
    pub outcome: ObservationOutcome,
    pub latency_ms: Option<u32>,
    pub evidence_mass_basis_points: u16,
    /// Present only when a real user request consumed an explicit durable
    /// Half-Open probe lease. The lease revision is a fence, not an identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_state_revision: Option<u64>,
    /// Exact scope used by a real-request Half-Open probe. The revision is
    /// only a fencing token; this field prevents endpoint probes from being
    /// accidentally interpreted as credential probes during ingestion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_scope: Option<HealthProtectionScope>,
}
impl RoutingObservation {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.id.is_empty()
            || self.order.producer_id.is_empty()
            || self.order.event_at_ms < 0
            || self.order.ingested_at_ms < 0
            || self.evidence_mass_basis_points > 10_000
            || self.latency_ms.is_some_and(|value| value > 3_600_000)
        {
            return Err("invalid routing observation");
        }
        if matches!(self.traffic_equivalence, TrafficEquivalence::Anonymous)
            && matches!(self.source, ObservationSource::ActiveProbe)
            && matches!(self.outcome, ObservationOutcome::Success)
        {
            return Err("anonymous probe cannot produce model quality success evidence");
        }
        Ok(())
    }
}
