#![allow(dead_code)]

use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

use super::{ROUTING_DECISION_RETENTION_MAX_AGE_DAYS, ROUTING_DECISION_RETENTION_MAX_COUNT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingDecisionRetentionOutcome {
    pub(crate) deleted_decisions: u32,
    pub(crate) deleted_candidate_details: u32,
    pub(crate) has_more: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingDecisionRetention;

impl RoutingDecisionRetention {
    pub(crate) async fn enforce(
        &self,
        connection: &mut SqliteConnection,
        now_ms: i64,
        batch_size: u32,
    ) -> Result<RoutingDecisionRetentionOutcome, PersistenceError> {
        let batch_size = batch_size.clamp(1, 500);
        let cutoff_ms = now_ms - ROUTING_DECISION_RETENTION_MAX_AGE_DAYS * 86_400_000;
        let mut ids = old_decision_ids(connection, cutoff_ms, batch_size).await?;
        if ids.len() < batch_size as usize {
            let remaining = batch_size as usize - ids.len();
            ids.extend(over_count_decision_ids(connection, remaining as u32).await?);
        }
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return Ok(RoutingDecisionRetentionOutcome {
                deleted_decisions: 0,
                deleted_candidate_details: 0,
                has_more: false,
            });
        }

        let deleted_candidate_details = delete_candidate_details(connection, &ids).await?;
        let deleted_decisions = delete_decisions(connection, &ids).await?;
        let has_more = !old_decision_ids(connection, cutoff_ms, 1).await?.is_empty()
            || !over_count_decision_ids(connection, 1).await?.is_empty();

        Ok(RoutingDecisionRetentionOutcome {
            deleted_decisions,
            deleted_candidate_details,
            has_more,
        })
    }
}

async fn old_decision_ids(
    connection: &mut SqliteConnection,
    cutoff_ms: i64,
    limit: u32,
) -> Result<Vec<String>, PersistenceError> {
    let rows = sqlx::query(
        r#"
        SELECT id
        FROM route_decisions
        WHERE decided_at_ms < ?1
        ORDER BY decided_at_ms ASC, id ASC
        LIMIT ?2
        "#,
    )
    .bind(cutoff_ms)
    .bind(i64::from(limit))
    .fetch_all(connection)
    .await?;
    Ok(rows.into_iter().map(|row| row.get("id")).collect())
}

async fn over_count_decision_ids(
    connection: &mut SqliteConnection,
    limit: u32,
) -> Result<Vec<String>, PersistenceError> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM route_decisions")
        .fetch_one(&mut *connection)
        .await?;
    let excess = total - i64::from(ROUTING_DECISION_RETENTION_MAX_COUNT);
    if excess <= 0 || limit == 0 {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        r#"
        SELECT id
        FROM route_decisions
        ORDER BY decided_at_ms ASC, id ASC
        LIMIT ?1
        "#,
    )
    .bind(excess.min(i64::from(limit)))
    .fetch_all(connection)
    .await?;
    Ok(rows.into_iter().map(|row| row.get("id")).collect())
}

async fn delete_candidate_details(
    connection: &mut SqliteConnection,
    ids: &[String],
) -> Result<u32, PersistenceError> {
    let mut deleted = 0_u32;
    for id in ids {
        let rows = sqlx::query("DELETE FROM route_candidate_decisions WHERE decision_id = ?1")
            .bind(id)
            .execute(&mut *connection)
            .await?
            .rows_affected();
        deleted = deleted.saturating_add(rows.min(u64::from(u32::MAX)) as u32);
    }
    Ok(deleted)
}

async fn delete_decisions(
    connection: &mut SqliteConnection,
    ids: &[String],
) -> Result<u32, PersistenceError> {
    let mut deleted = 0_u32;
    for id in ids {
        let rows = sqlx::query("DELETE FROM route_decisions WHERE id = ?1")
            .bind(id)
            .execute(&mut *connection)
            .await?
            .rows_affected();
        deleted = deleted.saturating_add(rows.min(u64::from(u32::MAX)) as u32);
    }
    Ok(deleted)
}
