use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum CapacityConstraintKey {
    HalfOpen(String),
    Global,
    StationAccount(String),
    #[cfg(test)]
    ProviderAccount(String),
    StationKey(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProviderAccountConstraint {
    #[cfg(test)]
    Trusted {
        provider_account_id: String,
        max_concurrency: u32,
    },
    #[cfg(test)]
    EvidenceGap {
        reason: &'static str,
    },
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompositeCapacityRequest {
    pub(crate) station_id: String,
    pub(crate) station_key_id: String,
    pub(crate) half_open_probe_id: Option<String>,
    pub(crate) global_max_concurrency: u32,
    pub(crate) station_account_max_concurrency: u32,
    pub(crate) station_key_max_concurrency: u32,
    pub(crate) provider_account_constraint: ProviderAccountConstraint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CapacityEvidenceGap {
    pub(crate) constraint: &'static str,
    pub(crate) reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CapacityAcquireFailure {
    ConstraintUnavailable {
        constraint: CapacityConstraintKey,
        in_flight: u32,
        max_concurrency: u32,
        evidence_gaps: Vec<CapacityEvidenceGap>,
    },
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct CapacityGauge {
    pub(crate) active: u32,
    pub(crate) waiting: u32,
    pub(crate) load_denominator: u32,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct CompositeCapacityRegistry {
    shared: Arc<Mutex<CapacityState>>,
}

impl CompositeCapacityRegistry {
    pub(crate) fn active_for(&self, constraint: &CapacityConstraintKey) -> u32 {
        self.shared
            .lock()
            .expect("capacity registry poisoned")
            .counters
            .get(constraint)
            .map(|counter| counter.active)
            .unwrap_or_default()
    }

    pub(crate) fn active_for_station_key(&self, station_key_id: &str) -> u32 {
        self.active_for(&CapacityConstraintKey::StationKey(
            station_key_id.to_string(),
        ))
    }

    pub(crate) fn try_acquire(
        &self,
        request: CompositeCapacityRequest,
    ) -> Result<CapacityLease, CapacityAcquireFailure> {
        let mut state = self.shared.lock().expect("capacity registry poisoned");
        let mut acquired = Vec::new();
        let evidence_gaps = provider_evidence_gaps(&request.provider_account_constraint);

        let ordered_constraints = ordered_constraints(&request);
        for (constraint, max_concurrency) in ordered_constraints {
            let max_concurrency = state.effective_max(&constraint, max_concurrency);
            let in_flight = {
                let counter = state.counters.entry(constraint.clone()).or_default();
                counter.active
            };
            if max_concurrency > 0 && in_flight >= max_concurrency {
                rollback(&mut state, &acquired);
                return Err(CapacityAcquireFailure::ConstraintUnavailable {
                    constraint,
                    in_flight,
                    max_concurrency,
                    evidence_gaps,
                });
            }
            let counter = state.counters.entry(constraint.clone()).or_default();
            counter.active = counter.active.saturating_add(1);
            counter.load_denominator = effective_load_denominator(max_concurrency, 0);
            acquired.push(constraint);
        }

        Ok(CapacityLease {
            shared: Some(Arc::clone(&self.shared)),
            constraints: acquired,
            released: false,
            _evidence_gaps: evidence_gaps,
        })
    }

    #[cfg(test)]
    pub(crate) fn gauge(&self, constraint: &CapacityConstraintKey) -> CapacityGauge {
        let state = self.shared.lock().expect("capacity registry poisoned");
        state
            .counters
            .get(constraint)
            .map(CapacityCounter::public)
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn set_runtime_max(&self, constraint: CapacityConstraintKey, max_concurrency: u32) {
        let mut state = self.shared.lock().expect("capacity registry poisoned");
        state
            .runtime_max_overrides
            .insert(constraint.clone(), max_concurrency);
        state
            .counters
            .entry(constraint)
            .or_default()
            .load_denominator = max_concurrency;
    }

    pub(crate) fn try_enter_wait(
        &self,
        constraint: CapacityConstraintKey,
        max_waiters: u32,
        now_ms: i64,
        deadline_ms: i64,
    ) -> Result<CapacityWaitPermit, CapacityWaitMiss> {
        if max_waiters == 0 || deadline_ms <= now_ms {
            return Err(CapacityWaitMiss::NotAdmitted);
        }
        let mut state = self.shared.lock().expect("capacity registry poisoned");
        let waiting = state
            .counters
            .entry(constraint.clone())
            .or_default()
            .waiting;
        if waiting >= max_waiters {
            return Err(CapacityWaitMiss::QueueFull);
        }
        let ticket = state.next_wait_ticket;
        state.next_wait_ticket = state.next_wait_ticket.saturating_add(1);
        let counter = state.counters.entry(constraint.clone()).or_default();
        counter.waiting = counter.waiting.saturating_add(1);
        counter.waiters.push_back(ticket);
        Ok(CapacityWaitPermit {
            shared: Some(Arc::clone(&self.shared)),
            constraint,
            ticket,
            released: false,
        })
    }
}

fn ordered_constraints(request: &CompositeCapacityRequest) -> Vec<(CapacityConstraintKey, u32)> {
    let mut constraints = Vec::new();
    if let Some(half_open_probe_id) = &request.half_open_probe_id {
        constraints.push((
            CapacityConstraintKey::HalfOpen(half_open_probe_id.clone()),
            1,
        ));
    }
    constraints.push((
        CapacityConstraintKey::Global,
        request.global_max_concurrency,
    ));
    constraints.push((
        CapacityConstraintKey::StationAccount(request.station_id.clone()),
        request.station_account_max_concurrency,
    ));
    #[cfg(test)]
    if let ProviderAccountConstraint::Trusted {
        provider_account_id,
        max_concurrency,
    } = &request.provider_account_constraint
    {
        constraints.push((
            CapacityConstraintKey::ProviderAccount(provider_account_id.clone()),
            *max_concurrency,
        ));
    }
    constraints.push((
        CapacityConstraintKey::StationKey(request.station_key_id.clone()),
        request.station_key_max_concurrency,
    ));
    constraints
}

fn rollback(state: &mut CapacityState, acquired: &[CapacityConstraintKey]) {
    for constraint in acquired.iter().rev() {
        if let Some(counter) = state.counters.get_mut(constraint) {
            counter.active = counter.active.saturating_sub(1);
        }
    }
}

#[derive(Debug, Default)]
struct CapacityState {
    counters: BTreeMap<CapacityConstraintKey, CapacityCounter>,
    runtime_max_overrides: BTreeMap<CapacityConstraintKey, u32>,
    next_wait_ticket: u64,
}

impl CapacityState {
    fn effective_max(&self, constraint: &CapacityConstraintKey, requested_max: u32) -> u32 {
        let Some(runtime_max) = self.runtime_max_overrides.get(constraint).copied() else {
            return requested_max;
        };
        if requested_max == 0 {
            runtime_max
        } else if runtime_max == 0 {
            requested_max
        } else {
            requested_max.min(runtime_max)
        }
    }
}

#[derive(Debug, Default)]
struct CapacityCounter {
    active: u32,
    waiting: u32,
    load_denominator: u32,
    waiters: VecDeque<u64>,
}

impl CapacityCounter {
    #[cfg(test)]
    fn public(&self) -> CapacityGauge {
        CapacityGauge {
            active: self.active,
            waiting: self.waiting,
            load_denominator: self.load_denominator,
        }
    }
}

#[derive(Debug)]
pub(crate) struct CapacityLease {
    shared: Option<Arc<Mutex<CapacityState>>>,
    constraints: Vec<CapacityConstraintKey>,
    released: bool,
    _evidence_gaps: Vec<CapacityEvidenceGap>,
}

#[cfg(test)]
fn provider_evidence_gaps(constraint: &ProviderAccountConstraint) -> Vec<CapacityEvidenceGap> {
    if let ProviderAccountConstraint::EvidenceGap { reason } = constraint {
        vec![CapacityEvidenceGap {
            constraint: "provider_account",
            reason,
        }]
    } else {
        Vec::new()
    }
}

#[cfg(not(test))]
fn provider_evidence_gaps(_constraint: &ProviderAccountConstraint) -> Vec<CapacityEvidenceGap> {
    Vec::new()
}

impl CapacityLease {
    #[cfg(test)]
    pub(crate) fn constraints(&self) -> &[CapacityConstraintKey] {
        &self.constraints
    }

    #[cfg(test)]
    pub(crate) fn evidence_gaps(&self) -> &[CapacityEvidenceGap] {
        &self._evidence_gaps
    }

    pub(crate) fn release(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        let Some(shared) = &self.shared else {
            return;
        };
        let mut state = shared.lock().expect("capacity registry poisoned");
        rollback(&mut state, &self.constraints);
    }
}

impl Drop for CapacityLease {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapacityWaitMiss {
    NotAdmitted,
    QueueFull,
}

#[derive(Debug)]
pub(crate) struct CapacityWaitPermit {
    shared: Option<Arc<Mutex<CapacityState>>>,
    constraint: CapacityConstraintKey,
    ticket: u64,
    released: bool,
}

impl CapacityWaitPermit {
    #[cfg(test)]
    pub(crate) fn ticket(&self) -> u64 {
        self.ticket
    }

    pub(crate) fn release(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        let Some(shared) = &self.shared else {
            return;
        };
        let mut state = shared.lock().expect("capacity registry poisoned");
        if let Some(counter) = state.counters.get_mut(&self.constraint) {
            counter.waiting = counter.waiting.saturating_sub(1);
            if let Some(position) = counter
                .waiters
                .iter()
                .position(|ticket| *ticket == self.ticket)
            {
                counter.waiters.remove(position);
            }
        }
    }
}

impl Drop for CapacityWaitPermit {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CapacityMissObservation {
    pub(crate) constraint: CapacityConstraintKey,
    pub(crate) waitable: bool,
    pub(crate) in_flight: u32,
    pub(crate) max_concurrency: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PlanningRoundCapacityState {
    pub(crate) unavailable_this_pass: Vec<CapacityMissObservation>,
    pub(crate) wait_observations: Vec<CapacityMissObservation>,
}

impl PlanningRoundCapacityState {
    pub(crate) fn record_miss(&mut self, observation: CapacityMissObservation) {
        if observation.waitable {
            self.wait_observations.push(observation.clone());
        }
        self.unavailable_this_pass.push(observation);
    }

    pub(crate) fn clear(&mut self) {
        self.unavailable_this_pass.clear();
        self.wait_observations.clear();
    }

    pub(crate) fn build_wait_plan(
        &self,
        now_ms: i64,
        deadline_ms: i64,
        max_waiters: u32,
    ) -> Result<CapacityWaitPlan, CapacityWaitMiss> {
        if deadline_ms <= now_ms || max_waiters == 0 {
            return Err(CapacityWaitMiss::NotAdmitted);
        }
        let Some(observation) = self.wait_observations.first() else {
            return Err(CapacityWaitMiss::NotAdmitted);
        };
        Ok(CapacityWaitPlan {
            constraint: observation.constraint.clone(),
            max_waiters,
            timeout_ms: deadline_ms.saturating_sub(now_ms),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CapacityWaitPlan {
    pub(crate) constraint: CapacityConstraintKey,
    pub(crate) max_waiters: u32,
    pub(crate) timeout_ms: i64,
}

pub(crate) fn effective_load_denominator(max_concurrency: u32, load_factor: u32) -> u32 {
    if load_factor > 0 {
        load_factor
    } else if max_concurrency > 0 {
        max_concurrency
    } else {
        1
    }
}
