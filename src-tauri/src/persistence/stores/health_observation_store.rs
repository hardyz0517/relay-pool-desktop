use futures_util::future::{BoxFuture, FutureExt};
use sqlx::{Row, SqliteConnection};

use crate::{
    models::health::{HealthObservation, StationKeyHealthSnapshot},
    persistence::error::PersistenceError,
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HealthObservationStore;

pub(crate) trait HealthObservationWrite {
    fn assert_station_key_revision<'a>(
        &'a mut self,
        station_key_id: &'a str,
        endpoint_revision: i64,
    ) -> BoxFuture<'a, Result<(), PersistenceError>>;

    fn insert_observation_once<'a>(
        &'a mut self,
        observation: &'a HealthObservation,
        writeback_decision: &'a str,
    ) -> BoxFuture<'a, Result<bool, PersistenceError>>;

    fn load_station_key_health<'a>(
        &'a mut self,
        station_key_id: &'a str,
        now_ms: i64,
    ) -> BoxFuture<'a, Result<StationKeyHealthSnapshot, PersistenceError>>;

    fn upsert_station_key_health<'a>(
        &'a mut self,
        snapshot: &'a StationKeyHealthSnapshot,
    ) -> BoxFuture<'a, Result<(), PersistenceError>>;

    fn update_station_key_status<'a>(
        &'a mut self,
        station_key_id: &'a str,
        status: &'a str,
        now_ms: i64,
    ) -> BoxFuture<'a, Result<(), PersistenceError>>;
}

impl HealthObservationWrite for SqliteConnection {
    fn assert_station_key_revision<'a>(
        &'a mut self,
        station_key_id: &'a str,
        endpoint_revision: i64,
    ) -> BoxFuture<'a, Result<(), PersistenceError>> {
        async move {
            HealthObservationStore
                .assert_station_key_revision(self, station_key_id, endpoint_revision)
                .await
        }
        .boxed()
    }

    fn insert_observation_once<'a>(
        &'a mut self,
        observation: &'a HealthObservation,
        writeback_decision: &'a str,
    ) -> BoxFuture<'a, Result<bool, PersistenceError>> {
        async move {
            HealthObservationStore
                .insert_observation_once(self, observation, writeback_decision)
                .await
        }
        .boxed()
    }

    fn load_station_key_health<'a>(
        &'a mut self,
        station_key_id: &'a str,
        now_ms: i64,
    ) -> BoxFuture<'a, Result<StationKeyHealthSnapshot, PersistenceError>> {
        async move {
            HealthObservationStore
                .load_station_key_health(self, station_key_id, now_ms)
                .await
        }
        .boxed()
    }

    fn upsert_station_key_health<'a>(
        &'a mut self,
        snapshot: &'a StationKeyHealthSnapshot,
    ) -> BoxFuture<'a, Result<(), PersistenceError>> {
        async move {
            HealthObservationStore
                .upsert_station_key_health(self, snapshot)
                .await
        }
        .boxed()
    }

    fn update_station_key_status<'a>(
        &'a mut self,
        station_key_id: &'a str,
        status: &'a str,
        now_ms: i64,
    ) -> BoxFuture<'a, Result<(), PersistenceError>> {
        async move {
            HealthObservationStore
                .update_station_key_status(self, station_key_id, status, now_ms)
                .await
        }
        .boxed()
    }
}

impl HealthObservationWrite for sqlx::Transaction<'_, sqlx::Sqlite> {
    fn assert_station_key_revision<'a>(
        &'a mut self,
        station_key_id: &'a str,
        endpoint_revision: i64,
    ) -> BoxFuture<'a, Result<(), PersistenceError>> {
        async move {
            HealthObservationStore
                .assert_station_key_revision(&mut *self, station_key_id, endpoint_revision)
                .await
        }
        .boxed()
    }

    fn insert_observation_once<'a>(
        &'a mut self,
        observation: &'a HealthObservation,
        writeback_decision: &'a str,
    ) -> BoxFuture<'a, Result<bool, PersistenceError>> {
        async move {
            HealthObservationStore
                .insert_observation_once(&mut *self, observation, writeback_decision)
                .await
        }
        .boxed()
    }

    fn load_station_key_health<'a>(
        &'a mut self,
        station_key_id: &'a str,
        now_ms: i64,
    ) -> BoxFuture<'a, Result<StationKeyHealthSnapshot, PersistenceError>> {
        async move {
            HealthObservationStore
                .load_station_key_health(&mut *self, station_key_id, now_ms)
                .await
        }
        .boxed()
    }

    fn upsert_station_key_health<'a>(
        &'a mut self,
        snapshot: &'a StationKeyHealthSnapshot,
    ) -> BoxFuture<'a, Result<(), PersistenceError>> {
        async move {
            HealthObservationStore
                .upsert_station_key_health(&mut *self, snapshot)
                .await
        }
        .boxed()
    }

    fn update_station_key_status<'a>(
        &'a mut self,
        station_key_id: &'a str,
        status: &'a str,
        now_ms: i64,
    ) -> BoxFuture<'a, Result<(), PersistenceError>> {
        async move {
            HealthObservationStore
                .update_station_key_status(&mut *self, station_key_id, status, now_ms)
                .await
        }
        .boxed()
    }
}

impl HealthObservationStore {
    pub(crate) async fn assert_station_key_revision(
        &self,
        connection: &mut SqliteConnection,
        station_key_id: &str,
        endpoint_revision: i64,
    ) -> Result<(), PersistenceError> {
        let exists = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM station_keys k
                JOIN stations s ON s.id = k.station_id
                WHERE k.id = ?1 AND s.endpoint_revision = ?2
            )
            "#,
        )
        .bind(station_key_id)
        .bind(endpoint_revision)
        .fetch_one(connection)
        .await?;
        if exists == 0 {
            return Err(PersistenceError::NotFound);
        }
        Ok(())
    }

    pub(crate) async fn insert_observation_once(
        &self,
        connection: &mut SqliteConnection,
        observation: &HealthObservation,
        writeback_decision: &str,
    ) -> Result<bool, PersistenceError> {
        let inserted = sqlx::query(
            r#"
            INSERT OR IGNORE INTO station_key_health_observations (
                id, station_key_id, target_result_id, source, source_event_id,
                observed_at_ms, endpoint_revision, outcome, failure_kind, latency_ms,
                retry_after_ms, traffic_equivalence, error_summary,
                writeback_decision, created_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
        )
        .bind(&observation.id)
        .bind(&observation.station_key_id)
        .bind(&observation.target_result_id)
        .bind(observation.source.as_str())
        .bind(&observation.source_event_id)
        .bind(observation.observed_at_ms)
        .bind(observation.endpoint_revision)
        .bind(observation.outcome.as_str())
        .bind(&observation.failure_kind)
        .bind(observation.latency_ms)
        .bind(observation.retry_after_ms)
        .bind(observation.traffic_equivalence.as_str())
        .bind(&observation.error_summary)
        .bind(writeback_decision)
        .bind(observation.observed_at_ms)
        .execute(connection)
        .await?
        .rows_affected();
        Ok(inserted > 0)
    }

    pub(crate) async fn load_station_key_health(
        &self,
        connection: &mut SqliteConnection,
        station_key_id: &str,
        now_ms: i64,
    ) -> Result<StationKeyHealthSnapshot, PersistenceError> {
        let row = sqlx::query(
            r#"
            SELECT station_key_id, endpoint_revision, last_success_at, last_failure_at,
                   consecutive_failures, success_count, failure_count, total_duration_ms,
                   avg_latency_ms, last_error_summary, cooldown_until, updated_at
            FROM station_key_health
            WHERE station_key_id = ?1
            "#,
        )
        .bind(station_key_id)
        .fetch_optional(connection)
        .await?;
        Ok(row
            .map(row_to_health_snapshot)
            .unwrap_or_else(|| StationKeyHealthSnapshot::empty(station_key_id, 1, now_ms)))
    }

    pub(crate) async fn upsert_station_key_health(
        &self,
        connection: &mut SqliteConnection,
        snapshot: &StationKeyHealthSnapshot,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            r#"
            INSERT INTO station_key_health (
                station_key_id, endpoint_revision, last_success_at, last_failure_at,
                consecutive_failures, success_count, failure_count, total_duration_ms,
                avg_latency_ms, last_error_summary, cooldown_until, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(station_key_id) DO UPDATE SET
                endpoint_revision = excluded.endpoint_revision,
                last_success_at = excluded.last_success_at,
                last_failure_at = excluded.last_failure_at,
                consecutive_failures = excluded.consecutive_failures,
                success_count = excluded.success_count,
                failure_count = excluded.failure_count,
                total_duration_ms = excluded.total_duration_ms,
                avg_latency_ms = excluded.avg_latency_ms,
                last_error_summary = excluded.last_error_summary,
                cooldown_until = excluded.cooldown_until,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&snapshot.station_key_id)
        .bind(snapshot.endpoint_revision)
        .bind(&snapshot.last_success_at)
        .bind(&snapshot.last_failure_at)
        .bind(snapshot.consecutive_failures)
        .bind(snapshot.success_count)
        .bind(snapshot.failure_count)
        .bind(snapshot.total_duration_ms)
        .bind(snapshot.avg_latency_ms)
        .bind(&snapshot.last_error_summary)
        .bind(&snapshot.cooldown_until)
        .bind(&snapshot.updated_at)
        .execute(connection)
        .await?;
        Ok(())
    }

    pub(crate) async fn update_station_key_status(
        &self,
        connection: &mut SqliteConnection,
        station_key_id: &str,
        status: &str,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            r#"
            UPDATE station_keys
            SET status = ?1, last_checked_at = ?2, updated_at = ?2
            WHERE id = ?3
            "#,
        )
        .bind(status)
        .bind(now_ms.to_string())
        .bind(station_key_id)
        .execute(connection)
        .await?;
        Ok(())
    }
}

fn row_to_health_snapshot(row: sqlx::sqlite::SqliteRow) -> StationKeyHealthSnapshot {
    StationKeyHealthSnapshot {
        station_key_id: row.get("station_key_id"),
        endpoint_revision: row.get("endpoint_revision"),
        last_success_at: row.get("last_success_at"),
        last_failure_at: row.get("last_failure_at"),
        consecutive_failures: row.get("consecutive_failures"),
        success_count: row.get("success_count"),
        failure_count: row.get("failure_count"),
        total_duration_ms: row.get("total_duration_ms"),
        avg_latency_ms: row.get("avg_latency_ms"),
        last_error_summary: row.get("last_error_summary"),
        cooldown_until: row.get("cooldown_until"),
        updated_at: row.get("updated_at"),
    }
}
