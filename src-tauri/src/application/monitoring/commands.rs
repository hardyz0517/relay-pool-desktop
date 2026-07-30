use crate::models::monitoring::TriggerKind;

use super::planner::{MonitorPlanningSnapshot, TargetCapabilitySnapshot};

#[derive(Debug, Clone)]
pub(crate) struct MonitorExecutionRequest {
    pub(crate) trigger_kind: TriggerKind,
    pub(crate) manual_idempotency_key: Option<String>,
    pub(crate) snapshot: MonitorPlanningSnapshot,
    pub(crate) targets: Vec<TargetCapabilitySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MonitorExecutionReceipt {
    pub(crate) execution_id: String,
    pub(crate) reused_existing: bool,
}
