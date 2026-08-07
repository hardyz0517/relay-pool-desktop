use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use rand::{rngs::OsRng, RngCore};

use crate::application::routing_engine::{
    capacity::{CapacityConstraintKey, CompositeCapacityRegistry, RetryBudgetRegistry},
    exploration::ExplorationBudgetRegistry,
    planning_snapshot::RuntimeOverlaySnapshot,
};
#[cfg(test)]
use crate::application::routing_engine::capacity::{CapacityLease, CompositeCapacityRequest};

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
}

impl RoutingRuntimeState {
    pub(crate) fn new(max_concurrency: u32, exploration_budget: u32) -> Self {
        let mut root_seed = [0_u8; 32];
        OsRng.fill_bytes(&mut root_seed);
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

    pub fn capacity_registry(&self) -> Arc<CompositeCapacityRegistry> {
        Arc::clone(&self.capacity)
    }

    pub(crate) fn active_for_station(&self, station_type: &str, station_id: &str, station_key_id: &str) -> i64 {
        let constraint = if matches!(station_type.trim().to_ascii_lowercase().as_str(), "sub2api" | "newapi") {
            CapacityConstraintKey::StationAccount(station_id.to_string())
        } else {
            CapacityConstraintKey::StationKey(station_key_id.to_string())
        };
        i64::from(self.capacity.active_for(&constraint))
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
    use crate::application::routing_engine::capacity::{
        CompositeCapacityRequest, ProviderAccountConstraint,
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
}
