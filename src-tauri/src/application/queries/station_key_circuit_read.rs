//! Versioned read-only projection of the mutable station-key circuit.
//!
//! Proxy admission remains the state owner. This module only combines the
//! durable reducer rows with the process-local and durable persistence gates;
//! it never creates state rows, leases, or reducer events.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    application::station_key_circuit::{
        CircuitPersistenceGateSnapshot, StationKeyCircuitState, StationKeyCircuitStatus,
    },
    persistence::stores::station_key_circuit_store::StationKeyCircuitDurableReadSnapshot,
};

pub(crate) const STATION_KEY_CIRCUIT_READ_MODEL_VERSION: &str = "station_key_circuit_read_model_v1";
pub(crate) const STATION_KEY_CIRCUIT_SOURCE_SCHEMA_VERSION: u32 = 68;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CircuitReadModelStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CircuitPersistenceStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CircuitReadState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CircuitReadSnapshotRevision {
    pub(crate) process_gate_revision: u64,
    pub(crate) persistence_health_revision: u64,
    pub(crate) state_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationKeyCircuitReadFact {
    pub(crate) station_key_id: String,
    pub(crate) lifecycle_revision: u64,
    pub(crate) state: CircuitReadState,
    pub(crate) state_revision: Option<u64>,
    pub(crate) policy_revision: Option<u64>,
    pub(crate) consecutive_failures: u16,
    pub(crate) reopen_level: u32,
    pub(crate) cooldown_until_ms: Option<u64>,
    pub(crate) half_open_lease_in_flight: bool,
    pub(crate) half_open_lease_expires_at_ms: Option<u64>,
    pub(crate) recovery_successes: Option<u16>,
    pub(crate) persistence_status: CircuitPersistenceStatus,
    pub(crate) state_row_present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StationKeyCircuitReadSnapshot {
    pub(crate) status_version: &'static str,
    pub(crate) source_schema_version: u32,
    pub(crate) generated_at_ms: i64,
    pub(crate) read_model_status: CircuitReadModelStatus,
    pub(crate) read_model_code: Option<String>,
    pub(crate) revision: CircuitReadSnapshotRevision,
    pub(crate) circuits: Vec<StationKeyCircuitReadFact>,
    #[serde(skip)]
    process_gate: CircuitPersistenceGateSnapshot,
    #[serde(skip)]
    durable: StationKeyCircuitDurableReadSnapshot,
}

impl StationKeyCircuitReadSnapshot {
    pub(crate) fn project(
        generated_at_ms: i64,
        process_gate: CircuitPersistenceGateSnapshot,
        durable: StationKeyCircuitDurableReadSnapshot,
    ) -> Self {
        let global_unavailable = process_gate.global_unavailable
            || durable.persistence_gate_active(
                crate::persistence::stores::station_key_circuit_store::SHARED_CIRCUIT_PERSISTENCE_GATE_KEY,
                crate::persistence::stores::station_key_circuit_store::SHARED_CIRCUIT_PERSISTENCE_GATE_REVISION,
            );
        let mut circuits = durable
            .statuses
            .iter()
            .map(|status| {
                project_status(
                    status,
                    process_gate.is_active(&status.station_key_id, status.lifecycle_revision)
                        || durable.persistence_gate_active(
                            &status.station_key_id,
                            status.lifecycle_revision,
                        ),
                )
            })
            .collect::<Vec<_>>();
        for gate in &durable.persistence_gates {
            if gate.station_key_id
                == crate::persistence::stores::station_key_circuit_store::SHARED_CIRCUIT_PERSISTENCE_GATE_KEY
            {
                continue;
            }
            if circuits.iter().any(|fact| {
                fact.station_key_id == gate.station_key_id
                    && fact.lifecycle_revision == gate.lifecycle_revision
            }) {
                continue;
            }
            circuits.push(default_fact(
                gate.station_key_id.clone(),
                gate.lifecycle_revision,
                CircuitPersistenceStatus::Unavailable,
            ));
        }
        circuits.sort_by(|left, right| {
            left.station_key_id
                .cmp(&right.station_key_id)
                .then_with(|| left.lifecycle_revision.cmp(&right.lifecycle_revision))
        });
        let revision = CircuitReadSnapshotRevision {
            process_gate_revision: process_gate.revision,
            persistence_health_revision: durable.persistence_health_revision,
            state_fingerprint: state_fingerprint(&circuits, &durable),
        };
        Self {
            status_version: STATION_KEY_CIRCUIT_READ_MODEL_VERSION,
            source_schema_version: STATION_KEY_CIRCUIT_SOURCE_SCHEMA_VERSION,
            generated_at_ms: generated_at_ms.max(0),
            read_model_status: if global_unavailable {
                CircuitReadModelStatus::Unavailable
            } else {
                CircuitReadModelStatus::Available
            },
            read_model_code: global_unavailable
                .then(|| "circuit_persistence_unavailable".to_string()),
            revision,
            circuits,
            process_gate,
            durable,
        }
    }

    pub(crate) fn fact_for(
        &self,
        station_key_id: &str,
        lifecycle_revision: u64,
    ) -> StationKeyCircuitReadFact {
        if let Some(fact) = self.circuits.iter().find(|fact| {
            fact.station_key_id == station_key_id && fact.lifecycle_revision == lifecycle_revision
        }) {
            return fact.clone();
        }
        let unavailable = self
            .process_gate
            .is_active(station_key_id, lifecycle_revision)
            || self
                .durable
                .persistence_gate_active(station_key_id, lifecycle_revision);
        default_fact(
            station_key_id.to_string(),
            lifecycle_revision,
            if unavailable {
                CircuitPersistenceStatus::Unavailable
            } else {
                CircuitPersistenceStatus::Available
            },
        )
    }
}

fn project_status(
    status: &StationKeyCircuitStatus,
    persistence_unavailable: bool,
) -> StationKeyCircuitReadFact {
    let persistence_status = if persistence_unavailable {
        CircuitPersistenceStatus::Unavailable
    } else {
        CircuitPersistenceStatus::Available
    };
    match &status.state {
        StationKeyCircuitState::Closed {
            state_revision,
            consecutive_failures,
            reopen_level,
        } => StationKeyCircuitReadFact {
            station_key_id: status.station_key_id.clone(),
            lifecycle_revision: status.lifecycle_revision,
            state: CircuitReadState::Closed,
            state_revision: Some(*state_revision),
            policy_revision: Some(status.policy_revision),
            consecutive_failures: *consecutive_failures,
            reopen_level: *reopen_level,
            cooldown_until_ms: None,
            half_open_lease_in_flight: false,
            half_open_lease_expires_at_ms: None,
            recovery_successes: None,
            persistence_status,
            state_row_present: true,
        },
        StationKeyCircuitState::Open {
            state_revision,
            cooldown_until_ms,
            consecutive_failures,
            reopen_level,
            ..
        } => StationKeyCircuitReadFact {
            station_key_id: status.station_key_id.clone(),
            lifecycle_revision: status.lifecycle_revision,
            state: CircuitReadState::Open,
            state_revision: Some(*state_revision),
            policy_revision: Some(status.policy_revision),
            consecutive_failures: *consecutive_failures,
            reopen_level: *reopen_level,
            cooldown_until_ms: Some(*cooldown_until_ms),
            half_open_lease_in_flight: false,
            half_open_lease_expires_at_ms: None,
            recovery_successes: None,
            persistence_status,
            state_row_present: true,
        },
        StationKeyCircuitState::HalfOpen {
            state_revision,
            lease_id,
            lease_expires_at_ms,
            recovery_successes,
            reopen_level,
            ..
        } => StationKeyCircuitReadFact {
            station_key_id: status.station_key_id.clone(),
            lifecycle_revision: status.lifecycle_revision,
            state: CircuitReadState::HalfOpen,
            state_revision: Some(*state_revision),
            policy_revision: Some(status.policy_revision),
            consecutive_failures: 0,
            reopen_level: *reopen_level,
            cooldown_until_ms: None,
            half_open_lease_in_flight: lease_id.is_some(),
            half_open_lease_expires_at_ms: *lease_expires_at_ms,
            recovery_successes: Some(*recovery_successes),
            persistence_status,
            state_row_present: true,
        },
    }
}

fn default_fact(
    station_key_id: String,
    lifecycle_revision: u64,
    persistence_status: CircuitPersistenceStatus,
) -> StationKeyCircuitReadFact {
    StationKeyCircuitReadFact {
        station_key_id,
        lifecycle_revision,
        state: CircuitReadState::Closed,
        state_revision: None,
        policy_revision: None,
        consecutive_failures: 0,
        reopen_level: 0,
        cooldown_until_ms: None,
        half_open_lease_in_flight: false,
        half_open_lease_expires_at_ms: None,
        recovery_successes: None,
        persistence_status,
        state_row_present: false,
    }
}

fn state_fingerprint(
    circuits: &[StationKeyCircuitReadFact],
    durable: &StationKeyCircuitDurableReadSnapshot,
) -> String {
    let mut digest = Sha256::new();
    for fact in circuits {
        digest.update(fact.station_key_id.as_bytes());
        digest.update(fact.lifecycle_revision.to_le_bytes());
        digest.update(fact.state_revision.unwrap_or_default().to_le_bytes());
        digest.update(fact.policy_revision.unwrap_or_default().to_le_bytes());
        digest.update([fact.state as u8, fact.persistence_status as u8]);
    }
    for gate in &durable.persistence_gates {
        digest.update(gate.station_key_id.as_bytes());
        digest.update(gate.lifecycle_revision.to_le_bytes());
        digest.update(gate.updated_at_ms.to_le_bytes());
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn missing_state_projects_as_default_closed_without_creating_a_row() {
        let snapshot = StationKeyCircuitReadSnapshot::project(
            10,
            CircuitPersistenceGateSnapshot {
                global_unavailable: false,
                station_keys: BTreeSet::new(),
                revision: 3,
            },
            StationKeyCircuitDurableReadSnapshot {
                statuses: Vec::new(),
                persistence_gates: Vec::new(),
                persistence_health_revision: 4,
            },
        );
        let fact = snapshot.fact_for("key-new", 7);
        assert_eq!(fact.state, CircuitReadState::Closed);
        assert_eq!(fact.state_revision, None);
        assert!(!fact.state_row_present);
        assert_eq!(fact.persistence_status, CircuitPersistenceStatus::Available);
    }

    #[test]
    fn process_gate_overrides_a_durable_closed_state() {
        let snapshot = StationKeyCircuitReadSnapshot::project(
            10,
            CircuitPersistenceGateSnapshot {
                global_unavailable: false,
                station_keys: BTreeSet::from([("key-a".to_string(), 2)]),
                revision: 1,
            },
            StationKeyCircuitDurableReadSnapshot {
                statuses: vec![StationKeyCircuitStatus {
                    station_key_id: "key-a".to_string(),
                    lifecycle_revision: 2,
                    policy_revision: 5,
                    lease_policy: None,
                    state: StationKeyCircuitState::Closed {
                        state_revision: 9,
                        consecutive_failures: 0,
                        reopen_level: 0,
                    },
                }],
                persistence_gates: Vec::new(),
                persistence_health_revision: 0,
            },
        );
        assert_eq!(
            snapshot.fact_for("key-a", 2).persistence_status,
            CircuitPersistenceStatus::Unavailable
        );
    }
}
