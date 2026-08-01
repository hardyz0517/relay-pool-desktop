#![allow(dead_code)]

use serde_json::Value;
use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingDecisionCursor {
    pub(crate) decided_at_ms: i64,
    pub(crate) id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoutingDecisionSummaryRow {
    pub(crate) id: String,
    pub(crate) request_id: String,
    pub(crate) decided_at_ms: i64,
    pub(crate) ordering_profile: String,
    pub(crate) selected_station_key_id: Option<String>,
    pub(crate) candidate_count: u32,
    pub(crate) candidate_detail_count: u32,
    pub(crate) candidate_detail_truncated: bool,
    pub(crate) rejection_counts: Value,
    pub(crate) trace_status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoutingDecisionPage {
    pub(crate) rows: Vec<RoutingDecisionSummaryRow>,
    pub(crate) next_cursor: Option<RoutingDecisionCursor>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RouteCandidateDecisionRow {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) selected: bool,
    pub(crate) attempted: bool,
    pub(crate) retained_reason: String,
    pub(crate) hard_rejection_code: Option<String>,
    pub(crate) cost_basis: String,
    pub(crate) evidence: Value,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingDecisionQueries;

impl RoutingDecisionQueries {
    pub(crate) async fn list_decisions(
        &self,
        connection: &mut SqliteConnection,
        cursor: Option<&RoutingDecisionCursor>,
        limit: u32,
    ) -> Result<RoutingDecisionPage, PersistenceError> {
        let limit = limit.clamp(1, 500);
        let fetch_limit = i64::from(limit) + 1;
        let rows = if let Some(cursor) = cursor {
            sqlx::query(
                r#"
                SELECT id, request_id, decided_at_ms, ordering_profile,
                       selected_station_key_id, candidate_count, candidate_detail_count,
                       candidate_detail_truncated, rejection_counts_json, trace_status
                FROM route_decisions
                WHERE decided_at_ms < ?1 OR (decided_at_ms = ?1 AND id < ?2)
                ORDER BY decided_at_ms DESC, id DESC
                LIMIT ?3
                "#,
            )
            .bind(cursor.decided_at_ms)
            .bind(&cursor.id)
            .bind(fetch_limit)
            .fetch_all(&mut *connection)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, request_id, decided_at_ms, ordering_profile,
                       selected_station_key_id, candidate_count, candidate_detail_count,
                       candidate_detail_truncated, rejection_counts_json, trace_status
                FROM route_decisions
                ORDER BY decided_at_ms DESC, id DESC
                LIMIT ?1
                "#,
            )
            .bind(fetch_limit)
            .fetch_all(&mut *connection)
            .await?
        };

        let has_next = rows.len() > limit as usize;
        let rows = rows.into_iter().take(limit as usize).collect::<Vec<_>>();
        let next_cursor = if has_next {
            rows.last().map(|row| RoutingDecisionCursor {
                decided_at_ms: row.get("decided_at_ms"),
                id: row.get("id"),
            })
        } else {
            None
        };
        let rows = rows
            .into_iter()
            .map(summary_from_row)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(RoutingDecisionPage { rows, next_cursor })
    }

    pub(crate) async fn list_candidate_details(
        &self,
        connection: &mut SqliteConnection,
        decision_id: &str,
        limit: u32,
    ) -> Result<Vec<RouteCandidateDecisionRow>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT station_key_id, station_id, selected, attempted, retained_reason,
                   hard_rejection_code, cost_basis, evidence_json
            FROM route_candidate_decisions
            WHERE decision_id = ?1
            ORDER BY selected DESC, attempted DESC, station_key_id ASC
            LIMIT ?2
            "#,
        )
        .bind(decision_id)
        .bind(i64::from(limit.clamp(1, 500)))
        .fetch_all(connection)
        .await?;

        rows.into_iter().map(candidate_from_row).collect()
    }
}

fn summary_from_row(
    row: sqlx::sqlite::SqliteRow,
) -> Result<RoutingDecisionSummaryRow, PersistenceError> {
    let rejection_counts_json: String = row.get("rejection_counts_json");
    let rejection_counts = serde_json::from_str(&rejection_counts_json)
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    Ok(RoutingDecisionSummaryRow {
        id: row.get("id"),
        request_id: row.get("request_id"),
        decided_at_ms: row.get("decided_at_ms"),
        ordering_profile: row.get("ordering_profile"),
        selected_station_key_id: row.get("selected_station_key_id"),
        candidate_count: row.get::<i64, _>("candidate_count") as u32,
        candidate_detail_count: row.get::<i64, _>("candidate_detail_count") as u32,
        candidate_detail_truncated: row.get::<i64, _>("candidate_detail_truncated") != 0,
        rejection_counts,
        trace_status: row.get("trace_status"),
    })
}

fn candidate_from_row(
    row: sqlx::sqlite::SqliteRow,
) -> Result<RouteCandidateDecisionRow, PersistenceError> {
    let evidence_json: String = row.get("evidence_json");
    let evidence = serde_json::from_str(&evidence_json)
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    Ok(RouteCandidateDecisionRow {
        station_key_id: row.get("station_key_id"),
        station_id: row.get("station_id"),
        selected: row.get::<i64, _>("selected") != 0,
        attempted: row.get::<i64, _>("attempted") != 0,
        retained_reason: row.get("retained_reason"),
        hard_rejection_code: row.get("hard_rejection_code"),
        cost_basis: row.get("cost_basis"),
        evidence,
    })
}
