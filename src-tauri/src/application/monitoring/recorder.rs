use crate::models::monitoring::{FailureKind, ProbeOutcome, ProtocolKind, SemanticConfidence};

use super::planner::ProbePlan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MonitorExecutionReceipt {
    pub(crate) execution_id: String,
    pub(crate) reused_existing: bool,
}

pub(crate) trait MonitoringRecorder {
    fn find_manual_execution(&self, idempotency_key: &str) -> Option<MonitorExecutionReceipt>;
    fn begin_execution(
        &mut self,
        execution_id: String,
        plan: &ProbePlan,
        manual_idempotency_key: Option<&str>,
        started_at_ms: i64,
    ) -> MonitorExecutionReceipt;
    fn append_attempt(&mut self, attempt: RecordedAttempt);
    fn finalize_target(&mut self, result: RecordedTargetResult);
    fn finalize_execution(&mut self, summary: RecordedExecutionSummary);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecordedAttempt {
    pub(crate) execution_id: String,
    pub(crate) station_key_id: String,
    pub(crate) model: String,
    pub(crate) model_index: u8,
    pub(crate) attempt_number: u8,
    pub(crate) started_at_ms: i64,
    pub(crate) finished_at_ms: i64,
    pub(crate) outcome: ProbeOutcome,
    pub(crate) failure_kind: Option<FailureKind>,
    pub(crate) retryable: bool,
    pub(crate) semantic_confidence: SemanticConfidence,
    /// A closed, safe diagnostic code propagated from the probe implementation.
    pub(crate) error_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecordedTargetResult {
    pub(crate) execution_id: String,
    pub(crate) station_id: String,
    pub(crate) station_key_id: String,
    pub(crate) terminal_outcome: ProbeOutcome,
    pub(crate) terminal_failure_kind: Option<FailureKind>,
    pub(crate) decisive_attempt_id: Option<String>,
    pub(crate) requested_model: Option<String>,
    pub(crate) effective_model: Option<String>,
    pub(crate) used_fallback: bool,
    pub(crate) attempt_count: u32,
    pub(crate) protocol_kind: Option<ProtocolKind>,
    pub(crate) request_profile_hash: Option<String>,
    pub(crate) endpoint_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecordedExecutionSummary {
    pub(crate) execution_id: String,
    pub(crate) target_count: u32,
    pub(crate) available_count: u32,
    pub(crate) degraded_count: u32,
    pub(crate) unavailable_count: u32,
    pub(crate) skipped_count: u32,
    pub(crate) summary_outcome: ProbeOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BufferedExecution {
    pub(crate) execution_id: String,
    pub(crate) plan: ProbePlan,
    pub(crate) manual_idempotency_key: Option<String>,
    pub(crate) started_at_ms: i64,
    pub(crate) attempts: Vec<RecordedAttempt>,
    pub(crate) targets: Vec<RecordedTargetResult>,
    pub(crate) summary: Option<RecordedExecutionSummary>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BufferedMonitoringRecorder {
    existing_manual: std::collections::BTreeMap<String, MonitorExecutionReceipt>,
    execution: Option<BufferedExecution>,
}

impl BufferedMonitoringRecorder {
    pub(crate) fn into_execution(self) -> Option<BufferedExecution> {
        self.execution
    }
}

impl MonitoringRecorder for BufferedMonitoringRecorder {
    fn find_manual_execution(&self, idempotency_key: &str) -> Option<MonitorExecutionReceipt> {
        self.existing_manual.get(idempotency_key).cloned()
    }

    fn begin_execution(
        &mut self,
        execution_id: String,
        plan: &ProbePlan,
        manual_idempotency_key: Option<&str>,
        started_at_ms: i64,
    ) -> MonitorExecutionReceipt {
        self.execution = Some(BufferedExecution {
            execution_id: execution_id.clone(),
            plan: plan.clone(),
            manual_idempotency_key: manual_idempotency_key.map(ToOwned::to_owned),
            started_at_ms,
            attempts: Vec::new(),
            targets: Vec::new(),
            summary: None,
        });
        MonitorExecutionReceipt {
            execution_id,
            reused_existing: false,
        }
    }

    fn append_attempt(&mut self, attempt: RecordedAttempt) {
        if let Some(execution) = &mut self.execution {
            execution.attempts.push(attempt);
        }
    }

    fn finalize_target(&mut self, result: RecordedTargetResult) {
        if let Some(execution) = &mut self.execution {
            execution.targets.push(result);
        }
    }

    fn finalize_execution(&mut self, summary: RecordedExecutionSummary) {
        if let Some(execution) = &mut self.execution {
            execution.summary = Some(summary);
        }
    }
}
