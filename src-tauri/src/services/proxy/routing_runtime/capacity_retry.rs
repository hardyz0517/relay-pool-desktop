//! Process-local capacity and retry controls.
//!
//! The registries in this module are deliberately ephemeral. They protect a
//! running proxy process and are not a source of durable health or policy.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};

use crate::application::routing_engine::{
    capacity::RetryBudgetRegistry, exploration::ExplorationBudgetRegistry,
    failure_domains::CapacityDomainCommitment,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CapacityRetryProfileV1 {
    pub(crate) cooldown_ms: i64,
    pub(crate) domain_active_limit: u32,
    pub(crate) global_active_limit: u32,
    pub(crate) domain_waiter_limit: u32,
    pub(crate) global_waiter_limit: u32,
}

impl Default for CapacityRetryProfileV1 {
    fn default() -> Self {
        Self {
            cooldown_ms: 2_000,
            domain_active_limit: 2,
            global_active_limit: 8,
            domain_waiter_limit: 32,
            global_waiter_limit: 128,
        }
    }
}

impl CapacityRetryProfileV1 {
    pub(crate) const VERSION: &'static str = "capacity_retry_v1";

    pub(crate) fn deterministic_equal_jitter_ms(
        self,
        logical_request_identity: &[u8],
        retry_ordinal: u8,
    ) -> Option<u64> {
        use sha2::{Digest, Sha256};

        let cap = match retry_ordinal {
            1 => 250_u64,
            2 => 1_000_u64,
            _ => return None,
        };
        let mut digest = Sha256::new();
        digest.update(Self::VERSION.as_bytes());
        digest.update([retry_ordinal]);
        digest.update(logical_request_identity);
        let bytes = digest.finalize();
        let sample = u64::from_be_bytes(bytes[..8].try_into().expect("sha256 prefix"));
        let half = cap / 2;
        Some(half + sample % (cap - half + 1))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CapacityRetryRegistry {
    profile: CapacityRetryProfileV1,
    shared: Arc<Mutex<CapacityRetryRegistryState>>,
}

#[derive(Debug, Default)]
struct CapacityRetryRegistryState {
    active_global: u32,
    active_by_domain: BTreeMap<CapacityDomainCommitment, u32>,
    cooldown_by_domain: BTreeMap<CapacityDomainCommitment, DomainCooldown>,
    next_ticket: u64,
    global_waiters: VecDeque<u64>,
    domain_waiters: BTreeMap<CapacityDomainCommitment, VecDeque<u64>>,
    waiter_domains: BTreeMap<u64, CapacityDomainCommitment>,
}

#[derive(Debug, Clone, Copy)]
struct DomainCooldown {
    open_until_ms: i64,
    half_open_active: bool,
}

impl CapacityRetryRegistry {
    pub(crate) fn new(profile: CapacityRetryProfileV1) -> Self {
        Self {
            profile,
            shared: Arc::new(Mutex::new(CapacityRetryRegistryState::default())),
        }
    }

    #[cfg(test)]
    pub(crate) fn try_acquire(
        &self,
        domain: CapacityDomainCommitment,
        now_ms: i64,
    ) -> Result<CapacityRetryPermit, CapacityRetryAdmissionMiss> {
        let mut state = self
            .shared
            .lock()
            .expect("capacity retry registry poisoned");
        acquire_locked(&mut state, self.profile, domain.clone(), now_ms).map(|half_open| {
            CapacityRetryPermit {
                registry: self.clone(),
                domain,
                half_open,
                released: false,
            }
        })
    }

    pub(crate) fn deterministic_equal_jitter_ms(
        &self,
        logical_request_identity: &[u8],
        retry_ordinal: u8,
    ) -> Option<u64> {
        self.profile
            .deterministic_equal_jitter_ms(logical_request_identity, retry_ordinal)
    }

    pub(crate) fn register_waiter(
        &self,
        domain: CapacityDomainCommitment,
    ) -> Result<CapacityRetryWaiter, CapacityRetryAdmissionMiss> {
        let mut state = self
            .shared
            .lock()
            .expect("capacity retry registry poisoned");
        let domain_waiters = state.domain_waiters.get(&domain).map_or(0, VecDeque::len);
        if state.global_waiters.len() >= self.profile.global_waiter_limit as usize {
            return Err(CapacityRetryAdmissionMiss::GlobalWaitersFull);
        }
        if domain_waiters >= self.profile.domain_waiter_limit as usize {
            return Err(CapacityRetryAdmissionMiss::DomainWaitersFull);
        }
        let ticket = state.next_ticket;
        state.next_ticket = state.next_ticket.wrapping_add(1);
        state.global_waiters.push_back(ticket);
        state
            .domain_waiters
            .entry(domain.clone())
            .or_default()
            .push_back(ticket);
        state.waiter_domains.insert(ticket, domain.clone());
        Ok(CapacityRetryWaiter {
            registry: self.clone(),
            domain,
            ticket: Some(ticket),
        })
    }

    pub(crate) fn record_capacity_exhausted(&self, domain: CapacityDomainCommitment, now_ms: i64) {
        let mut state = self
            .shared
            .lock()
            .expect("capacity retry registry poisoned");
        state.cooldown_by_domain.insert(
            domain,
            DomainCooldown {
                open_until_ms: now_ms.saturating_add(self.profile.cooldown_ms),
                half_open_active: false,
            },
        );
    }

    pub(crate) fn protection_facts(
        &self,
        now_ms: i64,
    ) -> Vec<crate::application::queries::routing_protection::CapacityProtectionFact> {
        let state = self
            .shared
            .lock()
            .expect("capacity retry registry poisoned");
        state
            .cooldown_by_domain
            .iter()
            .map(|(domain, cooldown)| {
                let state = if cooldown.half_open_active {
                    "half_open"
                } else if now_ms < cooldown.open_until_ms {
                    "open"
                } else {
                    // Keep an expired cooldown visible until admission turns
                    // it into a half-open probe.
                    "open"
                };
                crate::application::queries::routing_protection::CapacityProtectionFact {
                    scope: format!(
                        "capacity_domain:v{}:{}",
                        domain.schema_version, domain.digest_hex
                    ),
                    state: state.to_string(),
                    cooldown_until_ms: Some(cooldown.open_until_ms),
                    recent_failure_code: Some("capacity_exhausted".to_string()),
                    updated_at_ms: Some(now_ms),
                }
            })
            .collect()
    }
}

fn acquire_locked(
    state: &mut CapacityRetryRegistryState,
    profile: CapacityRetryProfileV1,
    domain: CapacityDomainCommitment,
    now_ms: i64,
) -> Result<bool, CapacityRetryAdmissionMiss> {
    let half_open = match state.cooldown_by_domain.get_mut(&domain) {
        Some(cooldown) if now_ms < cooldown.open_until_ms => {
            return Err(CapacityRetryAdmissionMiss::CooldownOpen {
                retry_after_ms: cooldown.open_until_ms.saturating_sub(now_ms),
            });
        }
        Some(cooldown) if cooldown.half_open_active => {
            return Err(CapacityRetryAdmissionMiss::HalfOpenProbeActive);
        }
        Some(cooldown) => {
            cooldown.half_open_active = true;
            true
        }
        None => false,
    };
    let domain_active = state.active_by_domain.get(&domain).copied().unwrap_or(0);
    if state.active_global >= profile.global_active_limit {
        if half_open {
            state
                .cooldown_by_domain
                .get_mut(&domain)
                .expect("cooldown exists")
                .half_open_active = false;
        }
        return Err(CapacityRetryAdmissionMiss::GlobalActiveFull);
    }
    if domain_active >= profile.domain_active_limit {
        if half_open {
            state
                .cooldown_by_domain
                .get_mut(&domain)
                .expect("cooldown exists")
                .half_open_active = false;
        }
        return Err(CapacityRetryAdmissionMiss::DomainActiveFull);
    }
    state.active_global = state.active_global.saturating_add(1);
    *state.active_by_domain.entry(domain).or_default() += 1;
    Ok(half_open)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapacityRetryAdmissionMiss {
    GlobalActiveFull,
    DomainActiveFull,
    GlobalWaitersFull,
    DomainWaitersFull,
    NotQueueHead,
    CooldownOpen { retry_after_ms: i64 },
    HalfOpenProbeActive,
}

#[derive(Debug)]
pub(crate) struct CapacityRetryWaiter {
    registry: CapacityRetryRegistry,
    domain: CapacityDomainCommitment,
    ticket: Option<u64>,
}

impl CapacityRetryWaiter {
    pub(crate) fn try_promote(
        &mut self,
        now_ms: i64,
    ) -> Result<CapacityRetryPermit, CapacityRetryAdmissionMiss> {
        let ticket = self.ticket.expect("waiter already promoted or cancelled");
        let mut state = self
            .registry
            .shared
            .lock()
            .expect("capacity retry registry poisoned");
        let global_head = state.global_waiters.front().copied();
        let domain_head = state
            .domain_waiters
            .get(&self.domain)
            .and_then(|waiters| waiters.front().copied());
        if global_head != Some(ticket) || domain_head != Some(ticket) {
            return Err(CapacityRetryAdmissionMiss::NotQueueHead);
        }
        let half_open = acquire_locked(
            &mut state,
            self.registry.profile,
            self.domain.clone(),
            now_ms,
        )?;
        remove_waiter_locked(&mut state, ticket, &self.domain);
        self.ticket = None;
        Ok(CapacityRetryPermit {
            registry: self.registry.clone(),
            domain: self.domain.clone(),
            half_open,
            released: false,
        })
    }
}

impl Drop for CapacityRetryWaiter {
    fn drop(&mut self) {
        let Some(ticket) = self.ticket.take() else {
            return;
        };
        let mut state = self
            .registry
            .shared
            .lock()
            .expect("capacity retry registry poisoned");
        remove_waiter_locked(&mut state, ticket, &self.domain);
    }
}

fn remove_waiter_locked(
    state: &mut CapacityRetryRegistryState,
    ticket: u64,
    domain: &CapacityDomainCommitment,
) {
    state
        .global_waiters
        .retain(|candidate| *candidate != ticket);
    if let Some(waiters) = state.domain_waiters.get_mut(domain) {
        waiters.retain(|candidate| *candidate != ticket);
        if waiters.is_empty() {
            state.domain_waiters.remove(domain);
        }
    }
    state.waiter_domains.remove(&ticket);
}

#[derive(Debug)]
pub(crate) struct CapacityRetryPermit {
    registry: CapacityRetryRegistry,
    domain: CapacityDomainCommitment,
    half_open: bool,
    released: bool,
}

impl CapacityRetryPermit {
    pub(crate) fn complete_success(mut self) {
        self.release(Some(true), None);
    }

    pub(crate) fn complete_capacity_failure(mut self, now_ms: i64) {
        self.release(Some(false), Some(now_ms));
    }

    fn release(&mut self, success: Option<bool>, now_ms: Option<i64>) {
        if self.released {
            return;
        }
        self.released = true;
        let mut state = self
            .registry
            .shared
            .lock()
            .expect("capacity retry registry poisoned");
        state.active_global = state.active_global.saturating_sub(1);
        if let Some(active) = state.active_by_domain.get_mut(&self.domain) {
            *active = active.saturating_sub(1);
            if *active == 0 {
                state.active_by_domain.remove(&self.domain);
            }
        }
        if self.half_open {
            match success {
                Some(true) => {
                    state.cooldown_by_domain.remove(&self.domain);
                }
                Some(false) => {
                    state.cooldown_by_domain.insert(
                        self.domain.clone(),
                        DomainCooldown {
                            open_until_ms: now_ms
                                .unwrap_or_default()
                                .saturating_add(self.registry.profile.cooldown_ms),
                            half_open_active: false,
                        },
                    );
                }
                None => {
                    if let Some(cooldown) = state.cooldown_by_domain.get_mut(&self.domain) {
                        cooldown.half_open_active = false;
                    }
                }
            }
        }
    }
}

impl Drop for CapacityRetryPermit {
    fn drop(&mut self) {
        self.release(None, None);
    }
}

/// Capacity-related process state. All fields are discarded with the proxy
/// runtime and have no durable-health responsibilities.
#[derive(Debug)]
pub(crate) struct CapacityRetryRuntime {
    retry_budget: RetryBudgetRegistry,
    exploration_budget: ExplorationBudgetRegistry,
    capacity_retry: CapacityRetryRegistry,
}

impl CapacityRetryRuntime {
    pub(crate) fn new(max_concurrency: u32, exploration_budget: u32) -> Self {
        Self {
            retry_budget: RetryBudgetRegistry::new(max_concurrency.max(1)),
            exploration_budget: ExplorationBudgetRegistry::new(exploration_budget),
            capacity_retry: CapacityRetryRegistry::new(CapacityRetryProfileV1::default()),
        }
    }

    pub(crate) fn retry_budget(&self) -> RetryBudgetRegistry {
        self.retry_budget.clone()
    }

    pub(crate) fn exploration_budget(&self) -> ExplorationBudgetRegistry {
        self.exploration_budget.clone()
    }

    pub(crate) fn capacity_retry_registry(&self) -> CapacityRetryRegistry {
        self.capacity_retry.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::routing_engine::failure_domains::ProviderCapacityDomain;

    fn domain(name: &str) -> CapacityDomainCommitment {
        ProviderCapacityDomain::from_trusted_identity("openai", name, None, None)
            .expect("trusted domain")
            .commitment()
    }

    #[test]
    fn profile_jitter_is_deterministic_and_bounded() {
        let profile = CapacityRetryProfileV1::default();
        let first = profile
            .deterministic_equal_jitter_ms(b"request-1", 1)
            .expect("first retry");
        assert_eq!(
            Some(first),
            profile.deterministic_equal_jitter_ms(b"request-1", 1)
        );
        assert!((125..=250).contains(&first));
        assert!((500..=1_000).contains(
            &profile
                .deterministic_equal_jitter_ms(b"request-1", 2)
                .expect("second retry")
        ));
        assert_eq!(profile.deterministic_equal_jitter_ms(b"request-1", 3), None);
    }

    #[test]
    fn half_open_probe_drop_releases_ownership_without_clearing_cooldown() {
        let registry = CapacityRetryRegistry::new(CapacityRetryProfileV1::default());
        let capacity_domain = domain("gpt-5");
        registry.record_capacity_exhausted(capacity_domain.clone(), 1_000);
        let abandoned = registry
            .try_acquire(capacity_domain.clone(), 3_000)
            .expect("half-open probe");
        drop(abandoned);
        registry
            .try_acquire(capacity_domain.clone(), 3_000)
            .expect("replacement probe after cancellation")
            .complete_capacity_failure(3_000);
        assert_eq!(
            registry
                .try_acquire(capacity_domain, 4_999)
                .expect_err("cooldown reopens after failed probe"),
            CapacityRetryAdmissionMiss::CooldownOpen { retry_after_ms: 1 }
        );
    }

    #[test]
    fn waiter_drop_releases_global_and_domain_queue_capacity() {
        let registry = CapacityRetryRegistry::new(CapacityRetryProfileV1 {
            global_waiter_limit: 1,
            domain_waiter_limit: 1,
            ..CapacityRetryProfileV1::default()
        });
        let capacity_domain = domain("gpt-5");
        let waiter = registry
            .register_waiter(capacity_domain.clone())
            .expect("first waiter");
        assert_eq!(
            registry
                .register_waiter(capacity_domain.clone())
                .expect_err("global queue cap"),
            CapacityRetryAdmissionMiss::GlobalWaitersFull
        );
        drop(waiter);
        assert!(registry.register_waiter(capacity_domain).is_ok());
    }
}
