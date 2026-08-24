//! Process-local routing runtime composition.
//!
//! `RoutingRuntimeState` is intentionally a small composition root. Mutable
//! state with a distinct owner or lifecycle lives in `activity`,
//! `capacity_retry`, or `diagnostics`; this type only creates and holds those
//! owners alongside the process identity/revision overlay.

mod activity;
mod capacity_retry;
mod diagnostics;

#[cfg(test)]
pub(crate) use activity::ActivityLease as RoutingLease;
use activity::ActivityState;
use capacity_retry::{CapacityRetryRegistry, CapacityRetryRuntime};
use diagnostics::{DiagnosticMemoryBudget, DiagnosticsState};

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use rand::{rngs::OsRng, RngCore};

use crate::{
    application::routing_engine::capacity::CompositeCapacityRegistry,
    application::routing_engine::planning_snapshot::RuntimeOverlaySnapshot,
};

/// Runtime-owned mutable state for one proxy process instance. Durable facts
/// and policy never live here; they are captured into a PlanningSnapshot.
#[derive(Debug)]
pub(crate) struct RoutingRuntimeState {
    instance_id: String,
    runtime_revision: AtomicU64,
    candidate_set_revision: AtomicU64,
    max_concurrency: u32,
    root_seed: [u8; 32],
    activity: ActivityState,
    capacity_retry: CapacityRetryRuntime,
    diagnostics: DiagnosticsState,
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
            activity: ActivityState::new(),
            capacity_retry: CapacityRetryRuntime::new(max_concurrency, exploration_budget),
            diagnostics: DiagnosticsState::new(),
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
            in_flight: self.activity.in_flight(),
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

    pub(crate) fn retry_budget(
        &self,
    ) -> crate::application::routing_engine::capacity::RetryBudgetRegistry {
        self.capacity_retry.retry_budget()
    }

    pub(crate) fn exploration_budget(
        &self,
    ) -> crate::application::routing_engine::exploration::ExplorationBudgetRegistry {
        self.capacity_retry.exploration_budget()
    }

    pub(crate) fn capacity_retry_registry(&self) -> CapacityRetryRegistry {
        self.capacity_retry.capacity_retry_registry()
    }

    pub(crate) fn diagnostic_memory_budget(&self) -> DiagnosticMemoryBudget {
        self.diagnostics.diagnostic_memory_budget()
    }

    /// Appends one completed request trace to the process-local bounded ring.
    /// The command facade may expose an individual, already-redacted trace
    /// through the typed decision-trace IPC read model.
    pub(crate) fn record_decision_trace(
        &self,
        trace: crate::observability::decision_trace::RequestDecisionTraceV1,
    ) {
        self.diagnostics.record_decision_trace(trace);
    }

    #[cfg(test)]
    pub(crate) fn classification_metrics_snapshot(
        &self,
    ) -> crate::observability::metrics::MetricSnapshot {
        self.diagnostics.classification_metrics_snapshot()
    }

    pub(crate) fn decision_trace_snapshot(
        &self,
    ) -> Vec<crate::observability::decision_trace::RequestDecisionTraceV1> {
        self.diagnostics.decision_trace_snapshot()
    }

    pub fn capacity_registry(&self) -> Arc<CompositeCapacityRegistry> {
        self.activity.capacity_registry()
    }

    pub(crate) fn active_for_station(
        &self,
        station_type: &str,
        station_id: &str,
        station_key_id: &str,
    ) -> i64 {
        self.activity
            .active_for_station(station_type, station_id, station_key_id)
    }

    pub(crate) fn active_for_station_key(&self, station_key_id: &str) -> i64 {
        self.activity.active_for_station_key(station_key_id)
    }

    #[cfg(test)]
    pub(crate) fn acquire(
        &self,
        request: crate::application::routing_engine::capacity::CompositeCapacityRequest,
    ) -> Result<
        RoutingLease<'_>,
        crate::application::routing_engine::capacity::CapacityAcquireFailure,
    > {
        self.activity.acquire(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::routing_engine::capacity::{
        CompositeCapacityRequest, ProviderAccountConstraint,
    };
    use crate::observability::{
        decision_trace::{DecisionTraceBuilder, DecisionTraceEvent, DecisionTraceEventKind},
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
}
