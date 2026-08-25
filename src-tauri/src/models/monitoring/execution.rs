use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

use super::{FailureKind, ProbeOutcome, ProtocolKind, SemanticConfidence};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AttemptOrdinal(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptRole {
    Primary,
    Fallback { index: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    Scheduled,
    Manual,
    StartupRecovery,
    LegacyImport,
}

impl TriggerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Manual => "manual",
            Self::StartupRecovery => "startup_recovery",
            Self::LegacyImport => "legacy_import",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorExecutionStatus {
    Queued,
    Running,
    Completed,
    Partial,
    Cancelled,
    Skipped,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeAttempt {
    pub id: String,
    pub station_key_id: String,
    pub model: String,
    pub role: AttemptRole,
    pub ordinal: AttemptOrdinal,
    pub outcome: ProbeOutcome,
    pub failure_kind: Option<FailureKind>,
    pub latency_ms: Option<u64>,
    pub semantic_confidence: SemanticConfidence,
}

impl ProbeAttempt {
    pub fn new(
        id: impl Into<String>,
        station_key_id: impl Into<String>,
        model: impl Into<String>,
        role: AttemptRole,
        ordinal: AttemptOrdinal,
        outcome: ProbeOutcome,
        failure_kind: Option<FailureKind>,
    ) -> Result<Self, String> {
        let id = non_empty(id.into(), "attempt_id")?;
        let station_key_id = non_empty(station_key_id.into(), "station_key_id")?;
        let model = non_empty(model.into(), "model")?;
        if matches!(outcome, ProbeOutcome::Available) && failure_kind.is_some() {
            return Err("available attempt must not include failure_kind".to_string());
        }
        if matches!(outcome, ProbeOutcome::Unavailable | ProbeOutcome::Degraded)
            && failure_kind.is_none()
        {
            return Err("non-available attempt requires failure_kind".to_string());
        }
        Ok(Self {
            id,
            station_key_id,
            model,
            role,
            ordinal,
            outcome,
            failure_kind,
            latency_ms: None,
            semantic_confidence: SemanticConfidence::ProtocolValidated,
        })
    }

    pub fn legacy_http_only(mut self) -> Self {
        self.semantic_confidence = SemanticConfidence::LegacyHttpOnly;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorTargetResult {
    pub execution_id: String,
    pub station_id: String,
    pub station_key_id: String,
    pub terminal_outcome: ProbeOutcome,
    pub terminal_failure_kind: Option<FailureKind>,
    pub decisive_attempt_id: Option<String>,
    pub requested_model: Option<String>,
    pub effective_model: Option<String>,
    pub used_fallback: bool,
    pub attempt_count: u32,
    pub protocol_kind: ProtocolKind,
    pub semantic_confidence: SemanticConfidence,
}

impl MonitorTargetResult {
    pub fn availability_eligible(&self) -> bool {
        !matches!(
            self.terminal_failure_kind,
            Some(FailureKind::BudgetExceeded)
        )
    }

    pub fn latency_eligible(&self) -> bool {
        self.availability_eligible()
    }

    pub fn from_attempts(
        execution_id: impl Into<String>,
        station_id: impl Into<String>,
        station_key_id: impl Into<String>,
        protocol_kind: ProtocolKind,
        attempts: &[ProbeAttempt],
    ) -> Result<Self, String> {
        let execution_id = non_empty(execution_id.into(), "execution_id")?;
        let station_id = non_empty(station_id.into(), "station_id")?;
        let station_key_id = non_empty(station_key_id.into(), "station_key_id")?;
        if attempts.is_empty() {
            return Err("zero attempts require explicit skipped target result".to_string());
        }
        let mut attempt_ids = HashSet::new();
        for attempt in attempts {
            if attempt.station_key_id != station_key_id {
                return Err("attempt station_key_id must match target".to_string());
            }
            if !attempt_ids.insert(attempt.id.as_str()) {
                return Err("attempt ids must be unique within target result".to_string());
            }
        }

        let decisive = attempts
            .iter()
            .find(|attempt| matches!(attempt.outcome, ProbeOutcome::Available))
            .or_else(|| {
                attempts
                    .iter()
                    .find(|attempt| matches!(attempt.outcome, ProbeOutcome::Degraded))
            })
            .or_else(|| {
                attempts
                    .iter()
                    .rev()
                    .find(|attempt| !matches!(attempt.outcome, ProbeOutcome::Skipped))
            })
            .ok_or_else(|| "non-skipped decisive attempt required".to_string())?;

        let used_fallback = attempts
            .iter()
            .any(|attempt| matches!(attempt.role, AttemptRole::Fallback { .. }));
        let recovered_after_retry = decisive.outcome.is_route_available()
            && (used_fallback || decisive.ordinal.0 > 0 || attempts.len() > 1);
        let terminal_outcome = if recovered_after_retry {
            ProbeOutcome::Degraded
        } else {
            decisive.outcome
        };
        let terminal_failure_kind = if recovered_after_retry {
            Some(FailureKind::RecoveredAfterRetry)
        } else {
            decisive.failure_kind
        };

        Ok(Self {
            execution_id,
            station_id,
            station_key_id,
            terminal_outcome,
            terminal_failure_kind,
            decisive_attempt_id: Some(decisive.id.clone()),
            requested_model: attempts.first().map(|attempt| attempt.model.clone()),
            effective_model: Some(decisive.model.clone()),
            used_fallback,
            attempt_count: attempts.len() as u32,
            protocol_kind,
            semantic_confidence: decisive.semantic_confidence,
        })
    }

    pub fn skipped(
        execution_id: impl Into<String>,
        station_id: impl Into<String>,
        station_key_id: impl Into<String>,
        protocol_kind: ProtocolKind,
        failure_kind: FailureKind,
    ) -> Result<Self, String> {
        Ok(Self {
            execution_id: non_empty(execution_id.into(), "execution_id")?,
            station_id: non_empty(station_id.into(), "station_id")?,
            station_key_id: non_empty(station_key_id.into(), "station_key_id")?,
            terminal_outcome: ProbeOutcome::Skipped,
            terminal_failure_kind: Some(failure_kind),
            decisive_attempt_id: None,
            requested_model: None,
            effective_model: None,
            used_fallback: false,
            attempt_count: 0,
            protocol_kind,
            semantic_confidence: SemanticConfidence::ProtocolValidated,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AvailabilitySummary {
    pub denominator: u32,
    pub route_available_count: u32,
    pub availability_percent: Option<f64>,
}

impl AvailabilitySummary {
    pub fn from_target_results(results: &[MonitorTargetResult]) -> Self {
        let denominator = results
            .iter()
            .filter(|result| {
                result.availability_eligible()
                    && result
                        .terminal_outcome
                        .contributes_to_availability_denominator()
            })
            .count() as u32;
        let route_available_count = results
            .iter()
            .filter(|result| result.terminal_outcome.is_route_available())
            .count() as u32;
        Self {
            denominator,
            route_available_count,
            availability_percent: (denominator > 0)
                .then(|| route_available_count as f64 * 100.0 / denominator as f64),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub execution_id: String,
    pub status: MonitorExecutionStatus,
    pub target_count: u32,
    pub available_count: u32,
    pub degraded_count: u32,
    pub unavailable_count: u32,
    pub skipped_count: u32,
    pub summary_outcome: ProbeOutcome,
    pub summary_failure_kind: Option<FailureKind>,
}

impl ExecutionSummary {
    pub fn from_target_results(
        execution_id: impl Into<String>,
        expected_target_count: u32,
        results: &[MonitorTargetResult],
    ) -> Result<Self, String> {
        let execution_id = non_empty(execution_id.into(), "execution_id")?;
        let mut keys = BTreeSet::new();
        for result in results {
            if result.execution_id != execution_id {
                return Err("target result execution_id must match summary".to_string());
            }
            if !keys.insert(result.station_key_id.as_str()) {
                return Err("one target result per station key per execution".to_string());
            }
        }
        let mut available_count = 0;
        let mut degraded_count = 0;
        let mut unavailable_count = 0;
        let mut skipped_count = 0;
        let mut first_failure = None;
        for result in results {
            match result.terminal_outcome {
                ProbeOutcome::Available => available_count += 1,
                ProbeOutcome::Degraded => degraded_count += 1,
                ProbeOutcome::Unavailable => {
                    unavailable_count += 1;
                    first_failure = first_failure.or(result.terminal_failure_kind);
                }
                ProbeOutcome::Skipped => skipped_count += 1,
            }
        }
        let status = if results.len() as u32 == expected_target_count {
            MonitorExecutionStatus::Completed
        } else {
            MonitorExecutionStatus::Partial
        };
        let summary_outcome = if unavailable_count > 0 {
            ProbeOutcome::Unavailable
        } else if degraded_count > 0 {
            ProbeOutcome::Degraded
        } else if available_count > 0 {
            ProbeOutcome::Available
        } else {
            ProbeOutcome::Skipped
        };
        Ok(Self {
            execution_id,
            status,
            target_count: expected_target_count,
            available_count,
            degraded_count,
            unavailable_count,
            skipped_count,
            summary_outcome,
            summary_failure_kind: first_failure,
        })
    }
}

fn non_empty(value: String, field: &str) -> Result<String, String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(format!("{field} must be non-empty"))
    } else {
        Ok(value)
    }
}
