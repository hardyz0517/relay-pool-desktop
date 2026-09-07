//! Read-only projection for routing protection facts.
//!
//! Durable health verdicts, legacy health snapshots, and process-local
//! capacity protection are intentionally represented as separate persistence
//! kinds. This keeps a runtime capacity cooldown from being presented as a
//! durable circuit breaker and gives callers an explicit unavailable state.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::application::queries::station_key_circuit_read::{
    CircuitPersistenceStatus, CircuitReadModelStatus, CircuitReadSnapshotRevision,
    CircuitReadState, StationKeyCircuitReadSnapshot,
};

pub(crate) const ROUTING_PROTECTION_STATUS_VERSION: &str = "routing_protection_status_v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProtectionPersistenceKind {
    Durable,
    LegacyCompatibility,
    RuntimeCapacity,
    StationKeyCircuit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProtectionState {
    NoProtection,
    Closed,
    Degraded,
    Cooldown,
    Blocked,
    Open,
    HalfOpen,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProtectionReadModelStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProtectionDiagnosticReason {
    CapacityExhausted,
    CapacityStateUnavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProtectionStatusEntry {
    /// This is a non-secret commitment or a bounded compatibility hash. It
    /// never contains an endpoint, credential, request, or account value.
    pub(crate) scope: String,
    pub(crate) scope_kind: Option<String>,
    pub(crate) state: ProtectionState,
    /// Stable UI explanation key. The public `Degraded` state also carries
    /// whether it came from reducer `Closed` monitoring or an actual degraded
    /// verdict; consumers must not infer that distinction from `state` alone.
    pub(crate) explanation_key: String,
    pub(crate) persistence_kind: Option<ProtectionPersistenceKind>,
    pub(crate) cooldown_until_ms: Option<i64>,
    pub(crate) cooldown_remaining_ms: Option<i64>,
    pub(crate) recent_failure_code: Option<String>,
    pub(crate) diagnostic_reason: Option<ProtectionDiagnosticReason>,
    pub(crate) updated_at_ms: Option<i64>,
    pub(crate) detail_available: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingProtectionStatus {
    pub(crate) status_version: &'static str,
    pub(crate) generated_at_ms: i64,
    pub(crate) entries: Vec<ProtectionStatusEntry>,
    pub(crate) read_model_status: ProtectionReadModelStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) circuit_revision: Option<CircuitReadSnapshotRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) read_model_code: Option<String>,
    /// Effective proxy timeout facts. These are read-only runtime facts and
    /// intentionally do not belong to the editable routing policy document.
    pub(crate) timeouts: Option<ProxyTimeoutFacts>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProxyTimeoutFacts {
    pub(crate) connect_seconds: f64,
    pub(crate) first_byte_seconds: f64,
    pub(crate) precommit_seconds: f64,
    pub(crate) buffered_execution_seconds: f64,
    pub(crate) stream_idle_seconds: f64,
    pub(crate) owner: String,
}

/// The runtime registry is process-local. The caller supplies a deliberately
/// narrow fact instead of exposing registry locks or mutable state to queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CapacityProtectionFact {
    pub(crate) scope: String,
    pub(crate) state: String,
    pub(crate) cooldown_until_ms: Option<i64>,
    pub(crate) recent_failure_code: Option<String>,
    pub(crate) updated_at_ms: Option<i64>,
}

/// Compatibility protection projection backed exclusively by the v3
/// station-key circuit. Transport timeouts remain attached by the command
/// facade until their dedicated command is introduced.
pub(crate) fn project_routing_protection_status_from_circuit(
    circuit: &StationKeyCircuitReadSnapshot,
    capacity: &[CapacityProtectionFact],
    runtime_capacity_available: bool,
) -> RoutingProtectionStatus {
    let generated_at_ms = circuit.generated_at_ms;
    let mut entries = circuit
        .circuits
        .iter()
        .map(|fact| {
            let (state, explanation_key) = if fact.persistence_status
                == CircuitPersistenceStatus::Unavailable
            {
                (
                    ProtectionState::Unavailable,
                    "routing.protection.unavailable",
                )
            } else {
                match fact.state {
                    CircuitReadState::Closed => {
                        (ProtectionState::Closed, "routing.protection.closed")
                    }
                    CircuitReadState::Open => (ProtectionState::Open, "routing.protection.open"),
                    CircuitReadState::HalfOpen => {
                        (ProtectionState::HalfOpen, "routing.protection.half_open")
                    }
                }
            };
            ProtectionStatusEntry {
                scope: bounded_scope(&fact.station_key_id, "station_key"),
                scope_kind: Some("station_key".to_string()),
                state,
                explanation_key: explanation_key.to_string(),
                persistence_kind: Some(ProtectionPersistenceKind::StationKeyCircuit),
                cooldown_until_ms: fact
                    .cooldown_until_ms
                    .and_then(|value| i64::try_from(value).ok()),
                cooldown_remaining_ms: fact.cooldown_until_ms.and_then(|value| {
                    i64::try_from(value)
                        .ok()
                        .map(|until| until.saturating_sub(generated_at_ms).max(0))
                }),
                recent_failure_code: None,
                diagnostic_reason: None,
                updated_at_ms: None,
                detail_available: fact.persistence_status == CircuitPersistenceStatus::Available,
            }
        })
        .collect::<Vec<_>>();
    if runtime_capacity_available {
        entries.extend(
            capacity
                .iter()
                .map(|fact| capacity_entry(fact, generated_at_ms)),
        );
    } else {
        entries.push(ProtectionStatusEntry {
            scope: "runtime_capacity".to_string(),
            scope_kind: Some("local_capacity".to_string()),
            state: ProtectionState::Unavailable,
            explanation_key: "routing.protection.unavailable".to_string(),
            persistence_kind: Some(ProtectionPersistenceKind::RuntimeCapacity),
            cooldown_until_ms: None,
            cooldown_remaining_ms: None,
            recent_failure_code: None,
            diagnostic_reason: Some(ProtectionDiagnosticReason::CapacityStateUnavailable),
            updated_at_ms: None,
            detail_available: false,
        });
    }
    entries.sort_by(|left, right| left.scope.cmp(&right.scope));
    if entries.is_empty() {
        entries.push(ProtectionStatusEntry {
            scope: "routing".to_string(),
            scope_kind: None,
            state: ProtectionState::NoProtection,
            explanation_key: "routing.protection.none_active".to_string(),
            persistence_kind: None,
            cooldown_until_ms: None,
            cooldown_remaining_ms: None,
            recent_failure_code: None,
            diagnostic_reason: None,
            updated_at_ms: Some(generated_at_ms),
            detail_available: true,
        });
    }
    RoutingProtectionStatus {
        status_version: ROUTING_PROTECTION_STATUS_VERSION,
        generated_at_ms,
        entries,
        read_model_status: match circuit.read_model_status {
            CircuitReadModelStatus::Available => ProtectionReadModelStatus::Available,
            CircuitReadModelStatus::Unavailable => ProtectionReadModelStatus::Unavailable,
        },
        circuit_revision: Some(circuit.revision.clone()),
        read_model_code: circuit.read_model_code.clone(),
        timeouts: None,
    }
}

fn capacity_entry(fact: &CapacityProtectionFact, generated_at_ms: i64) -> ProtectionStatusEntry {
    let normalized_state = fact.state.trim().to_ascii_lowercase();
    let (state, detail_available) = match normalized_state.as_str() {
        "open" => (ProtectionState::Open, true),
        "half_open" | "half-open" => (ProtectionState::HalfOpen, true),
        _ => (ProtectionState::Unavailable, false),
    };
    let diagnostic_reason = match normalized_state.as_str() {
        "open" | "exhausted" | "capacity_exhausted" => {
            Some(ProtectionDiagnosticReason::CapacityExhausted)
        }
        "unavailable" | "state_unavailable" | "capacity_state_unavailable" => {
            Some(ProtectionDiagnosticReason::CapacityStateUnavailable)
        }
        _ => None,
    };
    ProtectionStatusEntry {
        scope: bounded_scope(&fact.scope, "capacity"),
        scope_kind: Some("local_capacity".to_string()),
        state,
        explanation_key: match state {
            ProtectionState::Open => "routing.protection.open",
            ProtectionState::HalfOpen => "routing.protection.half_open",
            _ => "routing.protection.unavailable",
        }
        .to_string(),
        persistence_kind: Some(ProtectionPersistenceKind::RuntimeCapacity),
        cooldown_until_ms: fact.cooldown_until_ms,
        cooldown_remaining_ms: remaining_ms(fact.cooldown_until_ms, generated_at_ms),
        recent_failure_code: fact
            .recent_failure_code
            .as_deref()
            .map(|code| bounded_code(code, "capacity_failure")),
        diagnostic_reason,
        updated_at_ms: fact.updated_at_ms.filter(|value| *value >= 0),
        detail_available,
    }
}

fn remaining_ms(until_ms: Option<i64>, now_ms: i64) -> Option<i64> {
    until_ms.map(|until| until.saturating_sub(now_ms).max(0))
}

fn bounded_scope(value: &str, prefix: &str) -> String {
    let value = value.trim();
    if value.len() <= 128
        && !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.'))
    {
        return value.to_string();
    }
    format!("{prefix}:v1:{}", digest_hex(value.as_bytes()))
}

fn bounded_code(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.len() <= 96
        && !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        value.to_string()
    } else {
        fallback.to_string()
    }
}

fn digest_hex(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod v3_tests {
    use super::*;
    use crate::{
        application::{
            queries::station_key_circuit_read::StationKeyCircuitReadSnapshot,
            station_key_circuit::{
                CircuitPersistenceGateSnapshot, StationKeyCircuitState, StationKeyCircuitStatus,
            },
        },
        persistence::stores::station_key_circuit_store::StationKeyCircuitDurableReadSnapshot,
    };

    #[test]
    fn protection_adapter_projects_only_station_key_circuit_facts() {
        let circuit = StationKeyCircuitReadSnapshot::project(
            100,
            CircuitPersistenceGateSnapshot::default(),
            StationKeyCircuitDurableReadSnapshot {
                statuses: vec![StationKeyCircuitStatus {
                    station_key_id: "key-a".to_string(),
                    lifecycle_revision: 2,
                    policy_revision: 7,
                    lease_policy: None,
                    state: StationKeyCircuitState::Open {
                        state_revision: 3,
                        opened_at_ms: 10,
                        cooldown_until_ms: 500,
                        consecutive_failures: 4,
                        reopen_level: 1,
                    },
                }],
                persistence_gates: Vec::new(),
                persistence_health_revision: 9,
            },
        );
        let status = project_routing_protection_status_from_circuit(&circuit, &[], true);
        assert_eq!(status.status_version, "routing_protection_status_v2");
        assert_eq!(status.entries.len(), 1);
        assert_eq!(status.entries[0].state, ProtectionState::Open);
        assert_eq!(status.entries[0].scope_kind.as_deref(), Some("station_key"));
        assert_eq!(
            status.entries[0].persistence_kind,
            Some(ProtectionPersistenceKind::StationKeyCircuit)
        );
        assert_eq!(status.entries[0].cooldown_remaining_ms, Some(400));
        assert_eq!(
            status
                .circuit_revision
                .as_ref()
                .map(|revision| revision.persistence_health_revision),
            Some(9)
        );
    }
}
