//! Station-key scoped v3 circuit reducer.
//!
//! This module deliberately contains no request routing or persistence I/O. It
//! is the single state-transition owner that the durable store and request
//! admission layer can call. The store is responsible for serializing calls
//! with a state revision/CAS; this reducer makes the transition rules explicit
//! and deterministic for both production and replay tests.

use std::{
    collections::BTreeSet,
    sync::{Arc, RwLock},
};

#[derive(Debug, Default)]
struct CircuitPersistenceGateState {
    global_unavailable: bool,
    station_keys: BTreeSet<(String, u64)>,
    revision: u64,
}

/// Process-local fail-closed fence shared by request finalization and routing
/// admission. Durable gate rows survive restarts; this fence covers the period
/// in which the circuit store itself is unavailable and cannot persist one.
#[derive(Debug, Default)]
pub(crate) struct CircuitPersistenceGate {
    state: RwLock<CircuitPersistenceGateState>,
}

impl CircuitPersistenceGate {
    pub(crate) fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) fn is_active(&self, station_key_id: &str, lifecycle_revision: u64) -> bool {
        let state = self
            .state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.global_unavailable
            || state
                .station_keys
                .contains(&(station_key_id.to_string(), lifecycle_revision))
    }

    pub(crate) fn mark_station_key(&self, station_key_id: &str, lifecycle_revision: u64) {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .station_keys
            .insert((station_key_id.to_string(), lifecycle_revision));
        state.revision = state.revision.saturating_add(1);
    }

    pub(crate) fn mark_global_unavailable(&self) {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.global_unavailable = true;
        state.revision = state.revision.saturating_add(1);
    }

    pub(crate) fn revision(&self) -> u64 {
        self.state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .revision
    }

    /// Only the supervised health-check owner may call this method. A failure
    /// opened while the check was in flight advances the revision and keeps
    /// the process-local fence closed.
    pub(crate) fn clear_if_unchanged(&self, expected_revision: u64) -> bool {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.revision != expected_revision {
            return false;
        }
        state.global_unavailable = false;
        state.station_keys.clear();
        state.revision = state.revision.saturating_add(1);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StationKeyCircuitConfig {
    pub(crate) policy_revision: u64,
    pub(crate) consecutive_failure_threshold: u16,
    pub(crate) recovery_success_threshold: u16,
    pub(crate) recovery_wait_ms: u64,
    pub(crate) max_cooldown_ms: u64,
}

impl StationKeyCircuitConfig {
    pub(crate) fn validate(self) -> Result<Self, CircuitError> {
        if self.policy_revision == 0
            || self.consecutive_failure_threshold == 0
            || self.recovery_success_threshold == 0
            || self.recovery_wait_ms == 0
            || self.max_cooldown_ms < self.recovery_wait_ms
        {
            return Err(CircuitError::InvalidConfig);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StationKeyCircuitLeasePolicy {
    pub(crate) policy_revision: u64,
    pub(crate) recovery_success_threshold: u16,
    pub(crate) recovery_wait_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StationKeyCircuitState {
    Closed {
        state_revision: u64,
        consecutive_failures: u16,
        reopen_level: u32,
    },
    Open {
        state_revision: u64,
        opened_at_ms: u64,
        cooldown_until_ms: u64,
        consecutive_failures: u16,
        reopen_level: u32,
    },
    HalfOpen {
        state_revision: u64,
        lease_id: Option<String>,
        lease_revision: u64,
        lease_expires_at_ms: Option<u64>,
        recovery_successes: u16,
        reopen_level: u32,
    },
}

impl Default for StationKeyCircuitState {
    fn default() -> Self {
        Self::Closed {
            state_revision: 1,
            consecutive_failures: 0,
            reopen_level: 0,
        }
    }
}

/// Application-facing result returned by the durable circuit admission port.
/// The persistence adapter owns storage and CAS details, while callers only
/// need the decision and its revision fences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CircuitAdmissionResult {
    AllowedClosed {
        state_revision: u64,
    },
    AllowedHalfOpen {
        state_revision: u64,
        lease_revision: u64,
    },
    DeniedOpenCooldown,
    DeniedHalfOpenLease,
    DeniedScoreGate,
    DeniedGenerationFence,
    DeniedStaleGeneration,
    DeniedLateAfterFinalization,
    DeniedPersistenceUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StationKeyCircuitStatus {
    pub(crate) station_key_id: String,
    pub(crate) lifecycle_revision: u64,
    pub(crate) policy_revision: u64,
    pub(crate) lease_policy: Option<StationKeyCircuitLeasePolicy>,
    pub(crate) state: StationKeyCircuitState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CircuitAdmission {
    AllowedClosed,
    AllowedHalfOpen,
    DeniedOpenCooldown,
    DeniedHalfOpenLease,
    DeniedScoreGate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CircuitTransition {
    Observed,
    Opened,
    Reopened,
    RecoverySucceeded,
    Closed,
    IgnoredLateResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CircuitError {
    InvalidConfig,
    InvalidTime,
    InvalidLease,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StationKeyCircuit {
    config: StationKeyCircuitConfig,
    policy_revision: u64,
    lease_policy: Option<StationKeyCircuitLeasePolicy>,
    state: StationKeyCircuitState,
}

impl StationKeyCircuit {
    pub(crate) fn new(config: StationKeyCircuitConfig) -> Result<Self, CircuitError> {
        Ok(Self {
            config: config.validate()?,
            policy_revision: config.policy_revision,
            lease_policy: None,
            state: StationKeyCircuitState::default(),
        })
    }

    pub(crate) fn from_state(
        config: StationKeyCircuitConfig,
        policy_revision: u64,
        lease_policy: Option<StationKeyCircuitLeasePolicy>,
        state: StationKeyCircuitState,
    ) -> Result<Self, CircuitError> {
        if policy_revision == 0 {
            return Err(CircuitError::InvalidConfig);
        }
        let circuit = Self {
            config: config.validate()?,
            policy_revision,
            lease_policy,
            state,
        };
        circuit.validate_state()?;
        Ok(circuit)
    }

    pub(crate) fn state(&self) -> &StationKeyCircuitState {
        &self.state
    }

    pub(crate) fn policy_revision(&self) -> u64 {
        self.policy_revision
    }

    pub(crate) fn lease_policy(&self) -> Option<StationKeyCircuitLeasePolicy> {
        self.lease_policy
    }

    pub(crate) fn admit(
        &mut self,
        now_ms: u64,
        deadline_at_ms: u64,
        score_gate_passed: bool,
        lease_id: impl Into<String>,
    ) -> Result<CircuitAdmission, CircuitError> {
        if deadline_at_ms < now_ms {
            return Err(CircuitError::InvalidTime);
        }
        let lease_id = lease_id.into();
        if lease_id.is_empty() {
            return Err(CircuitError::InvalidLease);
        }
        match self.state.clone() {
            StationKeyCircuitState::Closed {
                state_revision,
                consecutive_failures,
                reopen_level,
            } => {
                if consecutive_failures >= self.config.consecutive_failure_threshold {
                    self.policy_revision = self.config.policy_revision;
                    self.lease_policy = None;
                    self.open_with_policy(
                        now_ms,
                        consecutive_failures,
                        reopen_level,
                        self.config.policy_revision,
                        self.config.recovery_wait_ms,
                    );
                    return Ok(CircuitAdmission::DeniedOpenCooldown);
                }
                if self.policy_revision != self.config.policy_revision {
                    self.policy_revision = self.config.policy_revision;
                    self.state = StationKeyCircuitState::Closed {
                        state_revision: state_revision.saturating_add(1).max(1),
                        consecutive_failures,
                        reopen_level,
                    };
                }
                Ok(CircuitAdmission::AllowedClosed)
            }
            StationKeyCircuitState::Open {
                cooldown_until_ms,
                reopen_level,
                state_revision,
                ..
            } => {
                if self.policy_revision != self.config.policy_revision {
                    self.policy_revision = self.config.policy_revision;
                    self.state = match self.state.clone() {
                        StationKeyCircuitState::Open {
                            opened_at_ms,
                            cooldown_until_ms,
                            consecutive_failures,
                            reopen_level,
                            ..
                        } => StationKeyCircuitState::Open {
                            state_revision: state_revision.saturating_add(1).max(1),
                            opened_at_ms,
                            cooldown_until_ms,
                            consecutive_failures,
                            reopen_level,
                        },
                        _ => unreachable!(),
                    };
                }
                if cooldown_until_ms > now_ms {
                    return Ok(CircuitAdmission::DeniedOpenCooldown);
                }
                if !score_gate_passed {
                    return Ok(CircuitAdmission::DeniedScoreGate);
                }
                let state_revision = match &self.state {
                    StationKeyCircuitState::Open { state_revision, .. } => *state_revision,
                    _ => unreachable!(),
                }
                .saturating_add(1)
                .max(1);
                self.policy_revision = self.config.policy_revision;
                self.lease_policy = Some(StationKeyCircuitLeasePolicy {
                    policy_revision: self.config.policy_revision,
                    recovery_success_threshold: self.config.recovery_success_threshold,
                    recovery_wait_ms: self.config.recovery_wait_ms,
                });
                self.state = StationKeyCircuitState::HalfOpen {
                    state_revision,
                    lease_id: Some(lease_id),
                    lease_revision: state_revision,
                    lease_expires_at_ms: Some(deadline_at_ms),
                    recovery_successes: 0,
                    reopen_level,
                };
                Ok(CircuitAdmission::AllowedHalfOpen)
            }
            StationKeyCircuitState::HalfOpen {
                lease_id: Some(_), ..
            } => Ok(CircuitAdmission::DeniedHalfOpenLease),
            StationKeyCircuitState::HalfOpen {
                state_revision,
                recovery_successes,
                reopen_level,
                ..
            } => {
                if !score_gate_passed {
                    Ok(CircuitAdmission::DeniedScoreGate)
                } else {
                    self.policy_revision = self.config.policy_revision;
                    self.lease_policy = Some(StationKeyCircuitLeasePolicy {
                        policy_revision: self.config.policy_revision,
                        recovery_success_threshold: self.config.recovery_success_threshold,
                        recovery_wait_ms: self.config.recovery_wait_ms,
                    });
                    self.state = StationKeyCircuitState::HalfOpen {
                        state_revision: state_revision.saturating_add(1).max(1),
                        lease_id: Some(lease_id),
                        lease_revision: state_revision.saturating_add(1).max(1),
                        lease_expires_at_ms: Some(deadline_at_ms),
                        recovery_successes,
                        reopen_level,
                    };
                    Ok(CircuitAdmission::AllowedHalfOpen)
                }
            }
        }
    }

    pub(crate) fn finish(
        &mut self,
        now_ms: u64,
        lease_id: Option<&str>,
        success: bool,
        boundary_crossed: bool,
    ) -> Result<CircuitTransition, CircuitError> {
        let state = self.state.clone();
        match state {
            StationKeyCircuitState::Closed {
                state_revision,
                consecutive_failures,
                reopen_level,
            } => {
                if !boundary_crossed {
                    return Ok(CircuitTransition::Observed);
                }
                if success {
                    self.policy_revision = self.config.policy_revision;
                    self.lease_policy = None;
                    self.state = StationKeyCircuitState::Closed {
                        state_revision: state_revision.saturating_add(1).max(1),
                        consecutive_failures: 0,
                        reopen_level,
                    };
                    Ok(CircuitTransition::Observed)
                } else {
                    let failures = consecutive_failures.saturating_add(1);
                    if failures < self.config.consecutive_failure_threshold {
                        self.policy_revision = self.config.policy_revision;
                        self.lease_policy = None;
                        self.state = StationKeyCircuitState::Closed {
                            state_revision: state_revision.saturating_add(1).max(1),
                            consecutive_failures: failures,
                            reopen_level,
                        };
                        Ok(CircuitTransition::Observed)
                    } else {
                        self.open_with_policy(
                            now_ms,
                            failures,
                            reopen_level,
                            self.config.policy_revision,
                            self.config.recovery_wait_ms,
                        );
                        Ok(CircuitTransition::Opened)
                    }
                }
            }
            StationKeyCircuitState::Open { .. } => Ok(CircuitTransition::IgnoredLateResult),
            StationKeyCircuitState::HalfOpen {
                state_revision,
                lease_id: active_lease,
                lease_revision: _,
                lease_expires_at_ms,
                recovery_successes,
                reopen_level,
            } => {
                let Some(expected) = active_lease.as_deref() else {
                    return Ok(CircuitTransition::IgnoredLateResult);
                };
                if lease_id != Some(expected) {
                    return Ok(CircuitTransition::IgnoredLateResult);
                }
                if !boundary_crossed {
                    self.lease_policy = None;
                    self.state = StationKeyCircuitState::HalfOpen {
                        state_revision,
                        lease_id: None,
                        lease_revision: state_revision,
                        lease_expires_at_ms: None,
                        recovery_successes,
                        reopen_level,
                    };
                    return Ok(CircuitTransition::Observed);
                }
                if let Some(expires) = lease_expires_at_ms {
                    // The lease is valid strictly before its deadline.  The
                    // durable reaper uses the same `<= now` boundary, so a
                    // result observed exactly at expiry cannot close the
                    // circuit after the lease has ended.
                    if now_ms >= expires {
                        let lease_policy = self.active_lease_policy()?;
                        self.open_with_policy(
                            now_ms,
                            0,
                            reopen_level,
                            lease_policy.policy_revision,
                            lease_policy.recovery_wait_ms,
                        );
                        return Ok(CircuitTransition::Reopened);
                    }
                }
                let lease_policy = self.active_lease_policy()?;
                if success {
                    let successes = recovery_successes.saturating_add(1);
                    if successes >= lease_policy.recovery_success_threshold {
                        self.policy_revision = lease_policy.policy_revision;
                        self.lease_policy = None;
                        self.state = StationKeyCircuitState::Closed {
                            state_revision: state_revision.saturating_add(1).max(1),
                            consecutive_failures: 0,
                            reopen_level: 0,
                        };
                        Ok(CircuitTransition::Closed)
                    } else {
                        self.policy_revision = lease_policy.policy_revision;
                        self.lease_policy = None;
                        self.state = StationKeyCircuitState::HalfOpen {
                            state_revision: state_revision.saturating_add(1).max(1),
                            lease_id: None,
                            lease_revision: state_revision.saturating_add(1).max(1),
                            lease_expires_at_ms: None,
                            recovery_successes: successes,
                            reopen_level,
                        };
                        Ok(CircuitTransition::RecoverySucceeded)
                    }
                } else {
                    self.open_with_policy(
                        now_ms,
                        0,
                        reopen_level,
                        lease_policy.policy_revision,
                        lease_policy.recovery_wait_ms,
                    );
                    Ok(CircuitTransition::Reopened)
                }
            }
        }
    }

    pub(crate) fn reap_expired_lease(
        &mut self,
        now_ms: u64,
        lease_id: &str,
    ) -> Result<bool, CircuitError> {
        let StationKeyCircuitState::HalfOpen {
            lease_id: Some(active),
            lease_expires_at_ms: Some(expires),
            reopen_level,
            ..
        } = self.state.clone()
        else {
            return Ok(false);
        };
        if active != lease_id || now_ms < expires {
            return Ok(false);
        }
        let lease_policy = self.active_lease_policy()?;
        self.open_with_policy(
            now_ms,
            0,
            reopen_level,
            lease_policy.policy_revision,
            lease_policy.recovery_wait_ms,
        );
        Ok(true)
    }

    fn active_lease_policy(&self) -> Result<StationKeyCircuitLeasePolicy, CircuitError> {
        self.lease_policy.ok_or(CircuitError::InvalidLease)
    }

    fn open_with_policy(
        &mut self,
        now_ms: u64,
        consecutive_failures: u16,
        previous_level: u32,
        policy_revision: u64,
        recovery_wait_ms: u64,
    ) {
        let reopen_level = previous_level.saturating_add(1).max(1);
        let exponent = reopen_level.saturating_sub(1).min(63);
        let multiplier = 1_u64.checked_shl(exponent).unwrap_or(u64::MAX);
        let cooldown = recovery_wait_ms
            .saturating_mul(multiplier)
            .min(self.config.max_cooldown_ms);
        let state_revision = match self.state {
            StationKeyCircuitState::Closed { state_revision, .. }
            | StationKeyCircuitState::Open { state_revision, .. }
            | StationKeyCircuitState::HalfOpen { state_revision, .. } => {
                state_revision.saturating_add(1).max(1)
            }
        };
        self.policy_revision = policy_revision;
        self.lease_policy = None;
        self.state = StationKeyCircuitState::Open {
            state_revision,
            opened_at_ms: now_ms,
            cooldown_until_ms: now_ms.saturating_add(cooldown),
            consecutive_failures,
            reopen_level,
        };
    }

    fn validate_state(&self) -> Result<(), CircuitError> {
        let has_active_lease = matches!(
            self.state,
            StationKeyCircuitState::HalfOpen {
                lease_id: Some(_),
                ..
            }
        );
        if has_active_lease != self.lease_policy.is_some()
            || self.lease_policy.is_some_and(|policy| {
                policy.policy_revision == 0
                    || policy.recovery_success_threshold == 0
                    || policy.recovery_wait_ms == 0
            })
        {
            return Err(CircuitError::InvalidLease);
        }
        match &self.state {
            StationKeyCircuitState::Open {
                cooldown_until_ms,
                opened_at_ms,
                ..
            } if cooldown_until_ms < opened_at_ms => Err(CircuitError::InvalidTime),
            StationKeyCircuitState::HalfOpen {
                lease_id,
                lease_expires_at_ms,
                ..
            } if lease_id.is_some() != lease_expires_at_ms.is_some() => {
                Err(CircuitError::InvalidLease)
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circuit() -> StationKeyCircuit {
        StationKeyCircuit::new(StationKeyCircuitConfig {
            policy_revision: 1,
            consecutive_failure_threshold: 3,
            recovery_success_threshold: 2,
            recovery_wait_ms: 10,
            max_cooldown_ms: 1_000,
        })
        .unwrap()
    }

    #[test]
    fn persistence_gate_only_clears_an_unchanged_health_check_snapshot() {
        let gate = CircuitPersistenceGate::default();
        gate.mark_station_key("key-a", 1);
        let health_check_revision = gate.revision();

        gate.mark_station_key("key-b", 1);
        assert!(!gate.clear_if_unchanged(health_check_revision));
        assert!(gate.is_active("key-a", 1));
        assert!(gate.is_active("key-b", 1));

        let retry_revision = gate.revision();
        assert!(gate.clear_if_unchanged(retry_revision));
        assert!(!gate.is_active("key-a", 1));
        assert!(!gate.is_active("key-b", 1));
    }

    #[test]
    fn global_persistence_gate_blocks_every_station_key() {
        let gate = CircuitPersistenceGate::default();
        gate.mark_global_unavailable();

        assert!(gate.is_active("key-a", 1));
        assert!(gate.is_active("key-b", 42));
    }

    #[test]
    fn consecutive_failures_open_and_cooldown_grows() {
        let mut circuit = circuit();
        for index in 0..2 {
            assert_eq!(
                circuit.finish(index, None, false, true).unwrap(),
                CircuitTransition::Observed
            );
        }
        assert_eq!(
            circuit.finish(2, None, false, true).unwrap(),
            CircuitTransition::Opened
        );
        let StationKeyCircuitState::Open {
            cooldown_until_ms,
            reopen_level,
            ..
        } = circuit.state()
        else {
            panic!("expected open");
        };
        assert_eq!(*reopen_level, 1);
        assert_eq!(*cooldown_until_ms, 12);
    }

    #[test]
    fn half_open_has_one_lease_and_requires_two_successes() {
        let mut circuit = circuit();
        for index in 0..3 {
            circuit.finish(index, None, false, true).unwrap();
        }
        assert_eq!(
            circuit.admit(12, 100, true, "lease-a").unwrap(),
            CircuitAdmission::AllowedHalfOpen
        );
        assert_eq!(
            circuit.admit(12, 100, true, "lease-b").unwrap(),
            CircuitAdmission::DeniedHalfOpenLease
        );
        assert_eq!(
            circuit.finish(20, Some("lease-a"), true, true).unwrap(),
            CircuitTransition::RecoverySucceeded
        );
        circuit.admit(20, 100, true, "lease-c").unwrap();
        assert_eq!(
            circuit.finish(21, Some("lease-c"), true, true).unwrap(),
            CircuitTransition::Closed
        );
        assert!(matches!(
            circuit.state(),
            StationKeyCircuitState::Closed { .. }
        ));
    }

    #[test]
    fn stale_result_cannot_close_a_new_open_cycle() {
        let mut circuit = circuit();
        for index in 0..3 {
            circuit.finish(index, None, false, true).unwrap();
        }
        circuit.admit(12, 100, true, "lease-a").unwrap();
        circuit.finish(20, Some("lease-a"), false, true).unwrap();
        assert_eq!(
            circuit.finish(21, Some("lease-a"), true, true).unwrap(),
            CircuitTransition::IgnoredLateResult
        );
        assert!(matches!(
            circuit.state(),
            StationKeyCircuitState::Open { .. }
        ));
    }

    #[test]
    fn expired_lease_reopens_once() {
        let mut circuit = circuit();
        for index in 0..3 {
            circuit.finish(index, None, false, true).unwrap();
        }
        circuit.admit(12, 20, true, "lease-a").unwrap();
        assert!(circuit.reap_expired_lease(21, "lease-a").unwrap());
        assert!(!circuit.reap_expired_lease(22, "lease-a").unwrap());
    }

    #[test]
    fn lease_reaper_treats_the_exact_deadline_as_expired() {
        let mut circuit = circuit();
        for index in 0..3 {
            circuit.finish(index, None, false, true).unwrap();
        }
        circuit.admit(12, 20, true, "lease-a").unwrap();
        assert!(circuit.reap_expired_lease(20, "lease-a").unwrap());
        assert!(!circuit.reap_expired_lease(20, "lease-a").unwrap());
    }

    #[test]
    fn result_at_lease_deadline_cannot_count_as_recovery_success() {
        let mut circuit = circuit();
        for index in 0..3 {
            circuit.finish(index, None, false, true).unwrap();
        }
        circuit.admit(12, 20, true, "lease-a").unwrap();
        assert_eq!(
            circuit.finish(20, Some("lease-a"), true, true).unwrap(),
            CircuitTransition::Reopened
        );
        assert!(matches!(
            circuit.state(),
            StationKeyCircuitState::Open { .. }
        ));
    }
}
