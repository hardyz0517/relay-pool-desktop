#![allow(dead_code)]

use std::collections::BTreeMap;

use serde_json::Value;
use sqlx::SqliteConnection;

use crate::persistence::error::PersistenceError;

use super::MAX_ROUTE_CANDIDATE_DECISION_DETAILS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RoutingTraceStatus {
    Complete,
    TraceIncomplete,
}

impl RoutingTraceStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::TraceIncomplete => "trace_incomplete",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoutingDecisionWrite {
    pub(crate) decision_id: String,
    pub(crate) request_id: String,
    pub(crate) decided_at_ms: i64,
    pub(crate) ordering_profile: String,
    pub(crate) selected_station_key_id: Option<String>,
    pub(crate) selected_station_id: Option<String>,
    pub(crate) selected_endpoint_revision: Option<i64>,
    pub(crate) candidate_count: u32,
    pub(crate) rejection_counts: BTreeMap<String, u32>,
    pub(crate) snapshot_id: String,
    pub(crate) fact_version_vector: String,
    pub(crate) planner_version: String,
    pub(crate) projector_version: String,
    pub(crate) runtime_overlay_revision: u64,
    pub(crate) trace_status: RoutingTraceStatus,
    pub(crate) candidates: Vec<RouteCandidateDecisionWrite>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RouteCandidateDecisionWrite {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) selected: bool,
    pub(crate) attempted: bool,
    pub(crate) primary_rejection_representative: bool,
    pub(crate) availability_tier: String,
    pub(crate) hard_rejection_code: Option<String>,
    pub(crate) hard_rejection_gate: Option<String>,
    pub(crate) priority: i64,
    pub(crate) cost_basis: String,
    pub(crate) cost_currency: Option<String>,
    pub(crate) cost_unit: Option<String>,
    pub(crate) cost_comparison_value: Option<f64>,
    pub(crate) snapshot_id: String,
    pub(crate) fact_version_vector: String,
    pub(crate) evidence: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingDecisionWriteOutcome {
    pub(crate) candidate_detail_count: usize,
    pub(crate) candidate_detail_truncated: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingDecisionWriter;

impl RoutingDecisionWriter {
    pub(crate) async fn upsert_decision(
        &self,
        connection: &mut SqliteConnection,
        decision: &RoutingDecisionWrite,
        now_ms: i64,
    ) -> Result<RoutingDecisionWriteOutcome, PersistenceError> {
        validate_decision(decision)?;
        let retained = retained_candidates(&decision.candidates);
        let truncated = retained.len() < decision.candidates.len();
        let rejection_counts_json = serde_json::to_string(&decision.rejection_counts)
            .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
        let runtime_overlay_revision =
            i64::try_from(decision.runtime_overlay_revision).map_err(|_| {
                PersistenceError::InvariantViolation("runtime revision overflow".into())
            })?;

        sqlx::query(
            r#"
            INSERT INTO route_decisions (
                id, request_id, decided_at_ms, ordering_profile,
                selected_station_key_id, selected_station_id, selected_endpoint_revision,
                candidate_count, candidate_detail_count, candidate_detail_truncated,
                rejection_counts_json, snapshot_id, fact_version_vector,
                planner_version, projector_version, runtime_overlay_revision,
                trace_status, created_at_ms, updated_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
            ON CONFLICT(request_id) DO UPDATE SET
                decided_at_ms = excluded.decided_at_ms,
                ordering_profile = excluded.ordering_profile,
                selected_station_key_id = excluded.selected_station_key_id,
                selected_station_id = excluded.selected_station_id,
                selected_endpoint_revision = excluded.selected_endpoint_revision,
                candidate_count = excluded.candidate_count,
                candidate_detail_count = excluded.candidate_detail_count,
                candidate_detail_truncated = excluded.candidate_detail_truncated,
                rejection_counts_json = excluded.rejection_counts_json,
                snapshot_id = excluded.snapshot_id,
                fact_version_vector = excluded.fact_version_vector,
                planner_version = excluded.planner_version,
                projector_version = excluded.projector_version,
                runtime_overlay_revision = excluded.runtime_overlay_revision,
                trace_status = excluded.trace_status,
                updated_at_ms = excluded.updated_at_ms
            "#,
        )
        .bind(&decision.decision_id)
        .bind(&decision.request_id)
        .bind(decision.decided_at_ms)
        .bind(&decision.ordering_profile)
        .bind(decision.selected_station_key_id.as_deref())
        .bind(decision.selected_station_id.as_deref())
        .bind(decision.selected_endpoint_revision)
        .bind(i64::from(decision.candidate_count))
        .bind(i64::try_from(retained.len()).unwrap_or(i64::MAX))
        .bind(i64::from(truncated as u8))
        .bind(rejection_counts_json)
        .bind(&decision.snapshot_id)
        .bind(&decision.fact_version_vector)
        .bind(&decision.planner_version)
        .bind(&decision.projector_version)
        .bind(runtime_overlay_revision)
        .bind(decision.trace_status.as_str())
        .bind(now_ms)
        .bind(now_ms)
        .execute(&mut *connection)
        .await?;

        sqlx::query("DELETE FROM route_candidate_decisions WHERE decision_id = ?1")
            .bind(&decision.decision_id)
            .execute(&mut *connection)
            .await?;

        for (index, candidate) in retained.iter().enumerate() {
            let evidence_json = serde_json::to_string(&candidate.evidence)
                .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
            validate_safe_text(&evidence_json)?;
            sqlx::query(
                r#"
                INSERT INTO route_candidate_decisions (
                    id, decision_id, request_id, station_key_id, station_id, endpoint_revision,
                    selected, attempted, retained_reason, availability_tier,
                    hard_rejection_code, hard_rejection_gate, priority, cost_basis,
                    cost_currency, cost_unit, cost_comparison_value, snapshot_id,
                    fact_version_vector, evidence_json, created_at_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
                "#,
            )
            .bind(format!("{}:{index:04}", decision.decision_id))
            .bind(&decision.decision_id)
            .bind(&decision.request_id)
            .bind(&candidate.station_key_id)
            .bind(&candidate.station_id)
            .bind(candidate.endpoint_revision)
            .bind(i64::from(candidate.selected as u8))
            .bind(i64::from(candidate.attempted as u8))
            .bind(retained_reason(candidate))
            .bind(&candidate.availability_tier)
            .bind(candidate.hard_rejection_code.as_deref())
            .bind(candidate.hard_rejection_gate.as_deref())
            .bind(candidate.priority)
            .bind(&candidate.cost_basis)
            .bind(candidate.cost_currency.as_deref())
            .bind(candidate.cost_unit.as_deref())
            .bind(candidate.cost_comparison_value)
            .bind(&candidate.snapshot_id)
            .bind(&candidate.fact_version_vector)
            .bind(evidence_json)
            .bind(now_ms)
            .execute(&mut *connection)
            .await?;
        }

        Ok(RoutingDecisionWriteOutcome {
            candidate_detail_count: retained.len(),
            candidate_detail_truncated: truncated,
        })
    }
}

fn validate_decision(decision: &RoutingDecisionWrite) -> Result<(), PersistenceError> {
    for value in [
        decision.decision_id.as_str(),
        decision.request_id.as_str(),
        decision.ordering_profile.as_str(),
        decision.snapshot_id.as_str(),
        decision.fact_version_vector.as_str(),
        decision.planner_version.as_str(),
        decision.projector_version.as_str(),
    ] {
        validate_safe_text(value)?;
    }
    for candidate in &decision.candidates {
        for value in [
            candidate.station_key_id.as_str(),
            candidate.station_id.as_str(),
            candidate.availability_tier.as_str(),
            candidate.cost_basis.as_str(),
            candidate.snapshot_id.as_str(),
            candidate.fact_version_vector.as_str(),
        ] {
            validate_safe_text(value)?;
        }
        if let Some(value) = candidate.hard_rejection_code.as_deref() {
            validate_safe_text(value)?;
        }
        if let Some(value) = candidate.hard_rejection_gate.as_deref() {
            validate_safe_text(value)?;
        }
        if let Some(value) = candidate.cost_currency.as_deref() {
            validate_safe_text(value)?;
        }
        if let Some(value) = candidate.cost_unit.as_deref() {
            validate_safe_text(value)?;
        }
    }
    Ok(())
}

fn validate_safe_text(value: &str) -> Result<(), PersistenceError> {
    let lower = value.to_ascii_lowercase();
    let forbidden = [
        "authorization",
        "bearer ",
        "sk-",
        "api_key",
        "cookie",
        "http://",
        "https://",
        "?",
        "prompt",
        "response_body",
    ];
    if forbidden.iter().any(|needle| lower.contains(needle)) {
        return Err(PersistenceError::InvariantViolation(
            "routing decision trace contains unsafe high-cardinality or secret-shaped text".into(),
        ));
    }
    Ok(())
}

fn retained_candidates(
    candidates: &[RouteCandidateDecisionWrite],
) -> Vec<&RouteCandidateDecisionWrite> {
    let mut ordered = candidates.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|candidate| {
        (
            retention_rank(candidate),
            candidate.priority,
            candidate.station_key_id.clone(),
        )
    });
    ordered
        .into_iter()
        .take(MAX_ROUTE_CANDIDATE_DECISION_DETAILS)
        .collect()
}

fn retention_rank(candidate: &RouteCandidateDecisionWrite) -> i64 {
    if candidate.selected {
        0
    } else if candidate.attempted {
        1
    } else if candidate.primary_rejection_representative {
        2
    } else {
        3
    }
}

fn retained_reason(candidate: &RouteCandidateDecisionWrite) -> &'static str {
    if candidate.selected {
        "selected"
    } else if candidate.attempted {
        "attempted"
    } else if candidate.primary_rejection_representative {
        "primary_rejection_representative"
    } else {
        "bounded_sample"
    }
}
