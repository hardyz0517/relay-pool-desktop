use std::sync::Arc;

use crate::{
    application::{
        error::ApplicationError,
        queries::station_key_circuit_read::{
            CircuitPersistenceStatus, CircuitReadState, StationKeyCircuitReadSnapshot,
        },
        station_key_circuit::CircuitPersistenceGate,
    },
    models::station_keys::{
        KeyPoolCircuitPersistenceStatus, KeyPoolCircuitSnapshot, KeyPoolCircuitState, KeyPoolItem,
    },
    persistence::{runtime::PersistenceHandle, stores::credential_store::CredentialStore},
};

#[derive(Clone)]
pub(crate) struct KeyPoolQuery {
    runtime: PersistenceHandle,
    credentials: CredentialStore,
    circuit_persistence_gate: Arc<CircuitPersistenceGate>,
}
impl KeyPoolQuery {
    pub(crate) fn new(
        runtime: PersistenceHandle,
        circuit_persistence_gate: Arc<CircuitPersistenceGate>,
    ) -> Self {
        Self {
            runtime,
            credentials: CredentialStore,
            circuit_persistence_gate,
        }
    }
    pub(crate) async fn load_all(&self) -> Result<Vec<KeyPoolItem>, ApplicationError> {
        for attempt in 0..2 {
            let process_before = self.circuit_persistence_gate.snapshot();
            let mut read = self.runtime.begin_read().await?;
            let mut items = self.credentials.list_key_pool_items(&mut read).await?;
            let durable =
                crate::persistence::stores::station_key_circuit_store::StationKeyCircuitStore
                    .load_read_snapshot(read.connection())
                    .await?;
            let process_after = self.circuit_persistence_gate.snapshot();
            if process_before.revision != process_after.revision {
                if attempt == 0 {
                    continue;
                }
                return Err(ApplicationError::Unavailable);
            }
            let circuit = StationKeyCircuitReadSnapshot::project(
                chrono::Utc::now().timestamp_millis(),
                process_after,
                durable,
            );
            for item in &mut items {
                let fact = circuit.fact_for(&item.id, item.station_key_lifecycle_revision);
                item.circuit = Some(KeyPoolCircuitSnapshot {
                    state: match fact.state {
                        CircuitReadState::Closed => KeyPoolCircuitState::Closed,
                        CircuitReadState::Open => KeyPoolCircuitState::Open,
                        CircuitReadState::HalfOpen => KeyPoolCircuitState::HalfOpen,
                    },
                    state_revision: fact.state_revision,
                    policy_revision: fact.policy_revision,
                    consecutive_failures: fact.consecutive_failures,
                    reopen_level: fact.reopen_level,
                    cooldown_until_ms: fact.cooldown_until_ms,
                    half_open_lease_in_flight: fact.half_open_lease_in_flight,
                    persistence_status: match fact.persistence_status {
                        CircuitPersistenceStatus::Available => {
                            KeyPoolCircuitPersistenceStatus::Available
                        }
                        CircuitPersistenceStatus::Unavailable => {
                            KeyPoolCircuitPersistenceStatus::Unavailable
                        }
                    },
                });
            }
            return Ok(items);
        }
        Err(ApplicationError::Unavailable)
    }
}
