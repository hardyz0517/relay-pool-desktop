use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

/// Whether the producer supplied a trustworthy event timestamp.  The
/// projector only admits `Valid` events into a time window; the other values
/// remain useful for retry/circuit decisions and audit but must never be
/// silently repaired with an ingestion timestamp.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum EventTimeStatus {
    Valid,
    Missing,
    Invalid,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum ResponseOrigin {
    Upstream,
    Relay,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum FailureAttribution {
    Key,
    Local,
    Client,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum RecoveryOrigin {
    #[default]
    Normal,
    CrashRecovery,
    LeaseReaper,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum ObservationRetryDisposition {
    #[default]
    End,
    RetryableBeforeCommit,
    StopRequest,
}

impl Default for EventTimeStatus {
    fn default() -> Self {
        Self::Valid
    }
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
    /// Opaque, non-sensitive commitment to the protocol, client profile,
    /// model and request shape that produced this observation. Active probes
    /// may influence quality only when this identity is present and valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparability_key: Option<String>,
    /// Stable request/probe lifecycle identity used to collapse retries into
    /// one quality sample.  This is deliberately separate from `id`, which
    /// identifies one attempt event.
    #[serde(default)]
    pub correlation_id: String,
    /// Attempt ordinal within the correlation cluster.
    #[serde(default)]
    pub attempt_index: u16,
    /// Current station-key binding lifecycle.  A changed binding must not
    /// inherit quality from the previous object.
    #[serde(default)]
    pub station_key_lifecycle_revision: u64,
    /// Durable lifecycle metadata needed for deterministic final-attempt
    /// selection during projection.
    #[serde(default)]
    pub cluster_finalized: bool,
    #[serde(default)]
    pub cluster_expected_attempt_count: u16,
    /// True only after the request crossed the outbound boundary.
    #[serde(default = "default_boundary_crossed")]
    pub boundary_crossed: bool,
    #[serde(default)]
    pub event_time_status: EventTimeStatus,
    /// Canonical response provenance and attribution are classified once at
    /// the outbound/lifecycle boundary. Projectors must not reconstruct them
    /// from `outcome` or an HTTP status string.
    #[serde(default)]
    pub response_origin: ResponseOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    #[serde(default)]
    pub failure_attribution: FailureAttribution,
    #[serde(default)]
    pub recovery_origin: RecoveryOrigin,
    #[serde(default)]
    pub retry_disposition: ObservationRetryDisposition,
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

fn default_boundary_crossed() -> bool {
    true
}

impl RoutingObservation {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.id.is_empty()
            || self.order.producer_id.is_empty()
            || self.order.event_at_ms < 0
            || self.order.ingested_at_ms < 0
            || self.evidence_mass_basis_points > 10_000
            || self.latency_ms.is_some_and(|value| value > 3_600_000)
            || self.correlation_id.len() > 192
            || self.correlation_id.chars().any(char::is_control)
            || self.failure_code.as_deref().is_some_and(|value| {
                value.is_empty() || value.len() > 96 || value.chars().any(char::is_control)
            })
            || self
                .comparability_key
                .as_deref()
                .is_some_and(|value| !is_valid_comparability_key(value))
        {
            return Err("invalid routing observation");
        }
        if matches!(self.source, ObservationSource::ActiveProbe)
            && matches!(
                self.traffic_equivalence,
                TrafficEquivalence::ExactRequest | TrafficEquivalence::SameModelShape
            )
            && self.comparability_key.is_none()
        {
            return Err("comparable active probe requires comparability identity");
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

pub(crate) fn is_valid_comparability_key(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("cmp:v1:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Builds the opaque identity used to prove that active-monitoring evidence
/// exercised the same protocol, client contract, model and request shape as
/// a real routed request. Inputs are canonical labels only; request bodies,
/// endpoint URLs and credentials must never be included.
pub(crate) fn routing_comparability_key_v1(
    protocol: &str,
    client_profile_id: &str,
    client_profile_version: u32,
    effective_model: &str,
    request_profile_hash: &str,
) -> Option<String> {
    let protocol = nonempty_comparability_part(protocol)?;
    let client_profile_id = nonempty_comparability_part(client_profile_id)?;
    let effective_model = nonempty_comparability_part(effective_model)?;
    let request_profile_hash = nonempty_comparability_part(request_profile_hash)?;
    if client_profile_version == 0 {
        return None;
    }

    let mut hasher = Sha256::new();
    let profile_version = client_profile_version.to_string();
    for part in [
        "relay-pool:routing-comparability:v1",
        protocol,
        client_profile_id,
        profile_version.as_str(),
        effective_model,
        request_profile_hash,
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    Some(format!("cmp:v1:{:x}", hasher.finalize()))
}

fn nonempty_comparability_part(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty() && !value.chars().any(char::is_control)).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::{is_valid_comparability_key, routing_comparability_key_v1};

    #[test]
    fn comparability_commitment_is_deterministic_and_shape_sensitive() {
        let first = routing_comparability_key_v1(
            "open_ai_responses",
            "standard_api",
            1,
            "gpt-test",
            "profile-hash",
        )
        .expect("valid commitment");
        let replay = routing_comparability_key_v1(
            "open_ai_responses",
            "standard_api",
            1,
            "gpt-test",
            "profile-hash",
        )
        .expect("valid commitment");
        let different_protocol = routing_comparability_key_v1(
            "open_ai_chat",
            "standard_api",
            1,
            "gpt-test",
            "profile-hash",
        )
        .expect("valid commitment");

        assert_eq!(first, replay);
        assert_ne!(first, different_protocol);
        assert!(is_valid_comparability_key(&first));
    }

    #[test]
    fn comparability_commitment_rejects_incomplete_identity() {
        assert!(routing_comparability_key_v1(
            "open_ai_responses",
            "standard_api",
            0,
            "gpt-test",
            "profile-hash",
        )
        .is_none());
        assert!(routing_comparability_key_v1(
            "open_ai_responses",
            "standard_api",
            1,
            " ",
            "profile-hash",
        )
        .is_none());
    }
}
