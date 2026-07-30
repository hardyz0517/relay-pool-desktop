use sqlx::{Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MonitoringBudgetRepository;

impl MonitoringBudgetRepository {
    pub(crate) async fn reserve_attempts(
        &self,
        connection: &mut SqliteConnection,
        id: &str,
        monitor_id: &str,
        station_key_id: Option<&str>,
        window_start_ms: i64,
        window_end_ms: i64,
        amount: i64,
        limit: i64,
        now_ms: i64,
    ) -> Result<bool, PersistenceError> {
        let existing = sqlx::query(
            r#"
            SELECT id, attempt_count
            FROM channel_monitor_probe_budget_usage
            WHERE monitor_id = ?1
              AND (station_key_id IS ?2 OR station_key_id = ?2)
              AND budget_window_start_ms = ?3
            "#,
        )
        .bind(monitor_id)
        .bind(station_key_id)
        .bind(window_start_ms)
        .fetch_optional(&mut *connection)
        .await?;

        if let Some(row) = existing {
            let existing_id = row.get::<String, _>("id");
            if existing_id == id {
                return Ok(true);
            }
            let current = row.get::<i64, _>("attempt_count");
            if current + amount > limit {
                return Ok(false);
            }
            sqlx::query(
                r#"
                UPDATE channel_monitor_probe_budget_usage
                SET attempt_count = ?1, updated_at_ms = ?2
                WHERE id = ?3
                "#,
            )
            .bind(current + amount)
            .bind(now_ms)
            .bind(existing_id)
            .execute(&mut *connection)
            .await?;
        } else {
            if amount > limit {
                return Ok(false);
            }
            sqlx::query(
                r#"
                INSERT INTO channel_monitor_probe_budget_usage (
                    id, monitor_id, station_key_id, budget_window_start_ms,
                    budget_window_end_ms, attempt_count, updated_at_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )
            .bind(id)
            .bind(monitor_id)
            .bind(station_key_id)
            .bind(window_start_ms)
            .bind(window_end_ms)
            .bind(amount)
            .bind(now_ms)
            .execute(&mut *connection)
            .await?;
        }
        Ok(true)
    }
}
