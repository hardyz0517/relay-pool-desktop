use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use rand::{rngs::OsRng, RngCore};

pub(crate) use super::diagnostic_memory::{
    DiagnosticMemoryBudget, DEFAULT_DIAGNOSTIC_MEMORY_LIMIT_BYTES,
};
#[cfg(test)]
use crate::observability::metrics::MetricSnapshot;

#[cfg(test)]
use crate::application::routing_engine::capacity::{CapacityLease, CompositeCapacityRequest};
use crate::{
    application::routing_engine::{
        capacity::{CapacityConstraintKey, CompositeCapacityRegistry, RetryBudgetRegistry},
        exploration::ExplorationBudgetRegistry,
        failure_domains::CapacityDomainCommitment,
        planning_snapshot::RuntimeOverlaySnapshot,
    },
    observability::{
        decision_trace::{DecisionTraceEventKind, DecisionTraceRing, RequestDecisionTraceV1},
        metrics::{
            ClassificationMetricLabel, LocalMetricBuffer, MetricEvent, MetricKind, MetricLabel,
        },
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CapacityRetryProfileV1 {
    pub(crate) same_target_retries: u8,
    pub(crate) max_upstream_attempts: u8,
    pub(crate) total_wait_budget_ms: i64,
    pub(crate) cooldown_ms: i64,
    pub(crate) domain_active_limit: u32,
    pub(crate) global_active_limit: u32,
    pub(crate) domain_waiter_limit: u32,
    pub(crate) global_waiter_limit: u32,
}

impl Default for CapacityRetryProfileV1 {
    fn default() -> Self {
        Self {
            same_target_retries: 2,
            max_upstream_attempts: 4,
            total_wait_budget_ms: 2_000,
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
    shared: Arc<Mutex<CapacityRetryState>>,
}

#[derive(Debug, Default)]
struct CapacityRetryState {
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
    fn new(profile: CapacityRetryProfileV1) -> Self {
        Self {
            profile,
            shared: Arc::new(Mutex::new(CapacityRetryState::default())),
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

    pub(crate) fn same_target_retries(&self) -> u8 {
        self.profile.same_target_retries
    }

    pub(crate) fn total_wait_budget_ms(&self) -> u64 {
        self.profile.total_wait_budget_ms.max(0) as u64
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
}

fn acquire_locked(
    state: &mut CapacityRetryState,
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
    state: &mut CapacityRetryState,
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

/// Runtime-owned mutable state for one proxy process instance. Durable facts
/// and policy never live here; they are captured into a PlanningSnapshot.
#[derive(Debug)]
pub(crate) struct RoutingRuntimeState {
    instance_id: String,
    runtime_revision: AtomicU64,
    candidate_set_revision: AtomicU64,
    max_concurrency: u32,
    root_seed: [u8; 32],
    in_flight: AtomicU64,
    capacity: Arc<CompositeCapacityRegistry>,
    retry_budget: RetryBudgetRegistry,
    exploration_budget: ExplorationBudgetRegistry,
    capacity_retry: CapacityRetryRegistry,
    diagnostic_memory: DiagnosticMemoryBudget,
    decision_traces: Arc<Mutex<DecisionTraceRing>>,
    classification_metrics: Arc<Mutex<LocalMetricBuffer>>,
}

impl RoutingRuntimeState {
    pub(crate) fn new(max_concurrency: u32, exploration_budget: u32) -> Self {
        let mut root_seed = [0_u8; 32];
        OsRng.fill_bytes(&mut root_seed);
        let capacity_retry_profile = CapacityRetryProfileV1::default();
        Self {
            instance_id: format!("proxy-runtime:{}", uuid::Uuid::now_v7()),
            runtime_revision: AtomicU64::new(1),
            candidate_set_revision: AtomicU64::new(1),
            max_concurrency,
            root_seed,
            in_flight: AtomicU64::new(0),
            capacity: Arc::new(CompositeCapacityRegistry::default()),
            retry_budget: RetryBudgetRegistry::new(max_concurrency.max(1)),
            exploration_budget: ExplorationBudgetRegistry::new(exploration_budget),
            capacity_retry: CapacityRetryRegistry::new(capacity_retry_profile),
            diagnostic_memory: DiagnosticMemoryBudget::new(DEFAULT_DIAGNOSTIC_MEMORY_LIMIT_BYTES),
            decision_traces: Arc::new(Mutex::new(DecisionTraceRing::new())),
            classification_metrics: Arc::new(Mutex::new(
                LocalMetricBuffer::new(2_048).expect("non-zero routing metric capacity"),
            )),
        }
    }

    #[cfg(test)]
    pub(crate) fn instance_id(&self) -> &str {
        &self.instance_id
    }

    pub(crate) fn snapshot(&self) -> RuntimeOverlaySnapshot {
        RuntimeOverlaySnapshot {
            runtime_instance_id: self.instance_id.clone(),
            runtime_revision: self.runtime_revision.load(Ordering::Acquire),
            candidate_set_revision: self.candidate_set_revision.load(Ordering::Acquire),
            in_flight: self
                .in_flight
                .load(Ordering::Acquire)
                .min(u64::from(u32::MAX)) as u32,
            max_concurrency: self.max_concurrency,
            affinity_station_key_id: None,
        }
    }

    /// Signals that process-local routing state changed and a currently
    /// planning request must rebuild its immutable view before another
    /// admission decision.
    pub(crate) fn mark_runtime_changed(&self) -> u64 {
        self.candidate_set_revision.fetch_add(1, Ordering::AcqRel);
        self.runtime_revision.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub(crate) fn root_seed(&self) -> [u8; 32] {
        self.root_seed
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

    pub(crate) fn diagnostic_memory_budget(&self) -> DiagnosticMemoryBudget {
        self.diagnostic_memory.clone()
    }

    /// Appends one completed request trace to the process-local bounded ring.
    /// The command facade may expose an individual, already-redacted trace
    /// through the typed decision-trace IPC read model. The ring itself is
    /// never persisted and remains bounded by DecisionTraceProfileV1.
    pub(crate) fn record_decision_trace(&self, trace: RequestDecisionTraceV1) {
        self.record_classification_metrics(&trace);
        self.decision_traces
            .lock()
            .expect("decision trace ring poisoned")
            .push(trace);
    }

    fn record_classification_metrics(&self, trace: &RequestDecisionTraceV1) {
        let Ok(mut metrics) = self.classification_metrics.lock() else {
            return;
        };
        for event in &trace.events {
            let label = match event.kind {
                DecisionTraceEventKind::AttemptStart => ClassificationMetricLabel::AttemptStart,
                DecisionTraceEventKind::CanonicalFailure => {
                    ClassificationMetricLabel::CanonicalFailure
                }
                DecisionTraceEventKind::SameTargetRetry => {
                    ClassificationMetricLabel::SameTargetRetry
                }
                DecisionTraceEventKind::SameDomainFallbackSuppressed => {
                    ClassificationMetricLabel::SameDomainSuppressed
                }
                DecisionTraceEventKind::CrossDomainFallback => {
                    ClassificationMetricLabel::CrossDomainFallback
                }
                DecisionTraceEventKind::CommittedStop => ClassificationMetricLabel::CommittedStop,
                DecisionTraceEventKind::SseErrorBeforeSemanticCommit => {
                    ClassificationMetricLabel::SsePrecommitError
                }
                DecisionTraceEventKind::Saturation => ClassificationMetricLabel::Saturation,
                DecisionTraceEventKind::FailClosed => ClassificationMetricLabel::FailClosed,
                DecisionTraceEventKind::ProfileVersionMismatch => {
                    ClassificationMetricLabel::ProfileMismatch
                }
                DecisionTraceEventKind::TraceTruncated => ClassificationMetricLabel::Truncated,
                DecisionTraceEventKind::RequestTerminal => {
                    ClassificationMetricLabel::RequestTerminal
                }
            };
            // The classification label is a closed enum. Do not attach trace
            // code, request identity, station/key, model, or provider data.
            if let Ok(metric) = MetricEvent::new(
                MetricKind::Classification,
                1,
                vec![MetricLabel::Classification(label)],
            ) {
                metrics.record(metric);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn classification_metrics_snapshot(&self) -> MetricSnapshot {
        self.classification_metrics
            .lock()
            .expect("classification metric buffer poisoned")
            .snapshot()
    }

    pub(crate) fn decision_trace_snapshot(&self) -> Vec<RequestDecisionTraceV1> {
        self.decision_traces
            .lock()
            .expect("decision trace ring poisoned")
            .traces()
            .cloned()
            .collect()
    }

    pub fn capacity_registry(&self) -> Arc<CompositeCapacityRegistry> {
        Arc::clone(&self.capacity)
    }

    pub(crate) fn active_for_station(
        &self,
        station_type: &str,
        station_id: &str,
        station_key_id: &str,
    ) -> i64 {
        let constraint = if matches!(
            station_type.trim().to_ascii_lowercase().as_str(),
            "sub2api" | "newapi"
        ) {
            CapacityConstraintKey::StationAccount(station_id.to_string())
        } else {
            CapacityConstraintKey::StationKey(station_key_id.to_string())
        };
        i64::from(self.capacity.active_for(&constraint))
    }

    pub(crate) fn active_for_station_key(&self, station_key_id: &str) -> i64 {
        i64::from(self.capacity.active_for_station_key(station_key_id))
    }

    #[cfg(test)]
    pub(crate) fn acquire(
        &self,
        request: CompositeCapacityRequest,
    ) -> Result<
        RoutingLease<'_>,
        crate::application::routing_engine::capacity::CapacityAcquireFailure,
    > {
        let lease = self.capacity.try_acquire(request)?;
        self.in_flight.fetch_add(1, Ordering::AcqRel);
        Ok(RoutingLease {
            runtime: self,
            lease: Some(lease),
        })
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct RoutingLease<'a> {
    runtime: &'a RoutingRuntimeState,
    lease: Option<CapacityLease>,
}

#[cfg(test)]
impl RoutingLease<'_> {
    pub(crate) fn release(&mut self) {
        if let Some(mut lease) = self.lease.take() {
            lease.release();
            self.runtime.in_flight.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

#[cfg(test)]
impl Drop for RoutingLease<'_> {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::routing_engine::{
        capacity::{CompositeCapacityRequest, ProviderAccountConstraint},
        failure_domains::ProviderCapacityDomain,
    };
    use crate::observability::{
        decision_trace::{DecisionTraceBuilder, DecisionTraceEvent},
        metrics::{ClassificationMetricLabel, MetricKind, MetricLabel},
    };

    fn request(id: &str) -> CompositeCapacityRequest {
        CompositeCapacityRequest {
            station_id: "station".into(),
            station_key_id: id.into(),
            half_open_probe_id: None,
            global_max_concurrency: 1,
            station_account_max_concurrency: 1,
            station_key_max_concurrency: 1,
            provider_account_constraint: ProviderAccountConstraint::NotApplicable,
        }
    }

    #[test]
    fn restart_uses_a_new_identity_and_old_lease_cannot_touch_new_state() {
        let first = RoutingRuntimeState::new(1, 1);
        let first_id = first.instance_id().to_string();
        let mut lease = first.acquire(request("key-1")).expect("lease");
        let second = RoutingRuntimeState::new(1, 1);
        assert_ne!(first_id, second.instance_id());
        lease.release();
        assert_eq!(second.snapshot().in_flight, 0);
    }

    #[test]
    fn station_key_activity_is_not_confused_with_shared_station_account_activity() {
        let runtime = RoutingRuntimeState::new(10, 1);
        let mut first_request = request("key-1");
        first_request.global_max_concurrency = 10;
        first_request.station_account_max_concurrency = 10;
        let mut second_request = request("key-2");
        second_request.global_max_concurrency = 10;
        second_request.station_account_max_concurrency = 10;

        let _first = runtime.acquire(first_request).expect("first key lease");
        let _second = runtime.acquire(second_request).expect("second key lease");

        assert_eq!(runtime.active_for_station("sub2api", "station", "key-1"), 2);
        assert_eq!(runtime.active_for_station_key("key-1"), 1);
        assert_eq!(runtime.active_for_station_key("key-2"), 1);
    }

    #[test]
    fn completed_trace_records_only_closed_classification_metrics() {
        let runtime = RoutingRuntimeState::new(1, 1);
        let mut trace = DecisionTraceBuilder::new("request-not-a-metric-label").expect("trace");
        trace
            .record(
                DecisionTraceEvent::new(
                    DecisionTraceEventKind::SameDomainFallbackSuppressed,
                    "capacity_same_domain_fallback_suppressed",
                    2,
                    None,
                )
                .expect("event"),
            )
            .expect("record event");

        runtime.record_decision_trace(trace.finish());

        let metrics = runtime.classification_metrics_snapshot();
        assert_eq!(metrics.events.len(), 1);
        assert_eq!(metrics.events[0].kind, MetricKind::Classification);
        assert_eq!(
            metrics.events[0].labels,
            vec![MetricLabel::Classification(
                ClassificationMetricLabel::SameDomainSuppressed
            )]
        );
    }

    fn domain(name: &str) -> CapacityDomainCommitment {
        ProviderCapacityDomain::from_trusted_identity("openai", name, None, None)
            .expect("trusted domain")
            .commitment()
    }

    #[test]
    fn capacity_retry_profile_uses_non_zero_deterministic_equal_jitter() {
        let profile = CapacityRetryProfileV1::default();
        let first = profile
            .deterministic_equal_jitter_ms(b"request-1", 1)
            .expect("first retry");
        let repeated = profile
            .deterministic_equal_jitter_ms(b"request-1", 1)
            .expect("same retry");
        let second = profile
            .deterministic_equal_jitter_ms(b"request-1", 2)
            .expect("second retry");

        assert_eq!(first, repeated);
        assert!((125..=250).contains(&first));
        assert!((500..=1_000).contains(&second));
        assert_eq!(profile.deterministic_equal_jitter_ms(b"request-1", 3), None);
    }

    #[test]
    fn capacity_retry_registry_enforces_domain_and_global_limits() {
        let registry = CapacityRetryRegistry::new(CapacityRetryProfileV1 {
            domain_active_limit: 1,
            global_active_limit: 2,
            ..CapacityRetryProfileV1::default()
        });
        let first = registry.try_acquire(domain("gpt-5"), 0).expect("first");
        assert_eq!(
            registry
                .try_acquire(domain("gpt-5"), 0)
                .expect_err("domain limit"),
            CapacityRetryAdmissionMiss::DomainActiveFull
        );
        let second = registry
            .try_acquire(domain("gpt-4.1"), 0)
            .expect("second domain");
        assert_eq!(
            registry
                .try_acquire(domain("o3"), 0)
                .expect_err("global limit"),
            CapacityRetryAdmissionMiss::GlobalActiveFull
        );
        drop((first, second));
        assert!(registry.try_acquire(domain("gpt-5"), 0).is_ok());
    }

    #[test]
    fn capacity_retry_registry_bounds_one_hundred_concurrent_requests_and_releases_them() {
        use std::{
            sync::{
                atomic::{AtomicUsize, Ordering},
                Arc, Barrier,
            },
            thread,
        };

        const WORKERS: usize = 100;
        let registry = CapacityRetryRegistry::new(CapacityRetryProfileV1 {
            domain_active_limit: 3,
            global_active_limit: 5,
            ..CapacityRetryProfileV1::default()
        });
        let rendezvous = Arc::new(Barrier::new(WORKERS + 1));
        let admitted_gpt_5 = Arc::new(AtomicUsize::new(0));
        let admitted_gpt_4_1 = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::with_capacity(WORKERS);

        for worker in 0..WORKERS {
            let registry = registry.clone();
            let rendezvous = Arc::clone(&rendezvous);
            let admitted_gpt_5 = Arc::clone(&admitted_gpt_5);
            let admitted_gpt_4_1 = Arc::clone(&admitted_gpt_4_1);
            workers.push(thread::spawn(move || {
                let is_gpt_5 = worker % 2 == 0;
                let permit = registry
                    .try_acquire(domain(if is_gpt_5 { "gpt-5" } else { "gpt-4.1" }), 0)
                    .ok();
                if permit.is_some() {
                    (if is_gpt_5 {
                        admitted_gpt_5
                    } else {
                        admitted_gpt_4_1
                    })
                    .fetch_add(1, Ordering::SeqCst);
                }

                // Keep every admitted permit live while the whole concurrent
                // cohort has attempted admission, then model cancellation / shutdown
                // by dropping it as the task exits.
                rendezvous.wait();
                rendezvous.wait();
                permit.is_some()
            }));
        }

        rendezvous.wait();
        let gpt_5 = admitted_gpt_5.load(Ordering::SeqCst);
        let gpt_4_1 = admitted_gpt_4_1.load(Ordering::SeqCst);
        assert_eq!(gpt_5 + gpt_4_1, 5, "global active capacity is bounded");
        assert!(gpt_5 <= 3, "gpt-5 domain capacity is bounded");
        assert!(gpt_4_1 <= 3, "gpt-4.1 domain capacity is bounded");

        rendezvous.wait();
        let admitted = workers
            .into_iter()
            .map(|worker| worker.join().expect("capacity worker"))
            .filter(|admitted| *admitted)
            .count();
        assert_eq!(admitted, 5);
        assert!(
            registry.try_acquire(domain("gpt-5"), 0).is_ok(),
            "all dropped permits must be released"
        );
    }

    #[test]
    fn cooldown_allows_only_one_half_open_probe_and_reopens_on_failure() {
        let registry = CapacityRetryRegistry::new(CapacityRetryProfileV1::default());
        let capacity_domain = domain("gpt-5");
        registry.record_capacity_exhausted(capacity_domain.clone(), 1_000);
        assert_eq!(
            registry
                .try_acquire(capacity_domain.clone(), 2_999)
                .expect_err("still open"),
            CapacityRetryAdmissionMiss::CooldownOpen { retry_after_ms: 1 }
        );
        let probe = registry
            .try_acquire(capacity_domain.clone(), 3_000)
            .expect("half-open probe");
        assert_eq!(
            registry
                .try_acquire(capacity_domain.clone(), 3_000)
                .expect_err("one probe"),
            CapacityRetryAdmissionMiss::HalfOpenProbeActive
        );
        probe.complete_capacity_failure(3_000);
        assert_eq!(
            registry
                .try_acquire(capacity_domain.clone(), 4_999)
                .expect_err("reopened"),
            CapacityRetryAdmissionMiss::CooldownOpen { retry_after_ms: 1 }
        );
        registry
            .try_acquire(capacity_domain.clone(), 5_000)
            .expect("next probe")
            .complete_success();
        assert!(registry.try_acquire(capacity_domain, 5_000).is_ok());
    }

    #[test]
    fn cancelled_half_open_probe_releases_its_admission_without_changing_cooldown() {
        let registry = CapacityRetryRegistry::new(CapacityRetryProfileV1::default());
        let capacity_domain = domain("gpt-5");
        registry.record_capacity_exhausted(capacity_domain.clone(), 1_000);

        let abandoned_probe = registry
            .try_acquire(capacity_domain.clone(), 3_000)
            .expect("half-open probe");
        drop(abandoned_probe);

        // A cancelled request must not strand the half-open ownership. The
        // original cooldown remains authoritative until a probe reports an
        // actual success or capacity failure.
        drop(
            registry
                .try_acquire(capacity_domain, 3_000)
                .expect("replacement probe after cancellation"),
        );
    }

    #[test]
    fn waiter_admission_is_fifo_and_cancel_safe() {
        let registry = CapacityRetryRegistry::new(CapacityRetryProfileV1::default());
        let capacity_domain = domain("gpt-5");
        let mut first = registry
            .register_waiter(capacity_domain.clone())
            .expect("first waiter");
        let mut second = registry
            .register_waiter(capacity_domain.clone())
            .expect("second waiter");
        assert_eq!(
            second.try_promote(0).expect_err("not head"),
            CapacityRetryAdmissionMiss::NotQueueHead
        );
        let permit = first.try_promote(0).expect("head promotes");
        drop(permit);
        assert!(second.try_promote(0).is_ok());

        let cancelled_domain = domain("gpt-4.1");
        let cancelled = registry
            .register_waiter(cancelled_domain.clone())
            .expect("cancelled waiter");
        let mut next = registry
            .register_waiter(cancelled_domain)
            .expect("next waiter");
        drop(cancelled);
        assert!(next.try_promote(0).is_ok());
    }

    #[test]
    fn one_hundred_waiters_respect_queue_cap_and_cancel_cleanup() {
        use std::{
            sync::{
                atomic::{AtomicUsize, Ordering},
                Arc, Barrier,
            },
            thread,
        };

        const WORKERS: usize = 100;
        const WAITERS: u32 = 16;
        let registry = CapacityRetryRegistry::new(CapacityRetryProfileV1 {
            domain_waiter_limit: WAITERS,
            global_waiter_limit: WAITERS,
            ..CapacityRetryProfileV1::default()
        });
        let start = Arc::new(Barrier::new(WORKERS + 1));
        let finish = Arc::new(Barrier::new(WORKERS + 1));
        let admitted = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::with_capacity(WORKERS);

        for _ in 0..WORKERS {
            let registry = registry.clone();
            let start = Arc::clone(&start);
            let finish = Arc::clone(&finish);
            let admitted = Arc::clone(&admitted);
            workers.push(thread::spawn(move || {
                start.wait();
                let waiter = registry.register_waiter(domain("gpt-5")).ok();
                if waiter.is_some() {
                    admitted.fetch_add(1, Ordering::SeqCst);
                }
                // Dropping the waiter models cancellation or runtime shutdown.
                finish.wait();
                drop(waiter);
            }));
        }

        start.wait();
        finish.wait();
        for worker in workers {
            worker.join().expect("waiter worker");
        }
        assert_eq!(
            admitted.load(Ordering::SeqCst),
            WAITERS as usize,
            "the shared waiter queue must reject excess concurrent requests"
        );

        // Every admitted ticket was dropped, so a fresh full queue can be
        // registered. This catches cancellation paths that strand tickets.
        let mut replacement = Vec::with_capacity(WAITERS as usize);
        for _ in 0..WAITERS {
            replacement.push(
                registry
                    .register_waiter(domain("gpt-5"))
                    .expect("cancelled waiters must release queue capacity"),
            );
        }
        assert_eq!(
            registry
                .register_waiter(domain("gpt-5"))
                .expect_err("queue cap"),
            CapacityRetryAdmissionMiss::GlobalWaitersFull
        );
        drop(replacement);
    }
}
