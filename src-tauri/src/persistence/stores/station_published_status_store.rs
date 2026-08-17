use sqlx::{QueryBuilder, Row, Sqlite};

use crate::{
    models::station_published_status::{
        MAX_PUBLISHED_STATUS_MONITORS as DOMAIN_MAX_PUBLISHED_STATUS_MONITORS,
        MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL as DOMAIN_MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL,
    },
    persistence::{
        error::PersistenceError, read_session::ReadSession, write_session::WriteSession,
    },
};

pub(crate) const MAX_PUBLISHED_STATUS_MONITORS: u32 = DOMAIN_MAX_PUBLISHED_STATUS_MONITORS as u32;
pub(crate) const MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL: u32 =
    DOMAIN_MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL as u32;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StationPublishedStatusStore;

#[derive(Debug, Clone)]
pub(crate) struct PublishedStatusSourceWrite {
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) source_kind: String,
    pub(crate) source_state: String,
    pub(crate) last_attempt_at: String,
    pub(crate) last_success_at: Option<String>,
    pub(crate) last_complete_at: Option<String>,
    pub(crate) last_error_kind: Option<String>,
    /// `None` preserves the last complete inventory count for failed attempts.
    pub(crate) monitor_count: Option<i64>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PublishedMonitorWrite {
    pub(crate) id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) source_kind: String,
    pub(crate) upstream_monitor_id: String,
    pub(crate) identity_kind: String,
    pub(crate) name: String,
    pub(crate) provider: String,
    pub(crate) group_name: Option<String>,
    pub(crate) primary_model: String,
    /// A bounded, validated JSON string array produced by the domain layer.
    pub(crate) extra_models_json: String,
    pub(crate) current_outcome: String,
    pub(crate) source_status: String,
    pub(crate) current_latency_ms: Option<i64>,
    pub(crate) current_ping_latency_ms: Option<i64>,
    pub(crate) upstream_checked_at_ms: Option<i64>,
    pub(crate) last_seen_run_id: String,
    pub(crate) last_seen_at: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PublishedMonitorSampleWrite {
    pub(crate) id: String,
    pub(crate) monitor_id: String,
    pub(crate) model: String,
    pub(crate) checked_at_ms: i64,
    pub(crate) outcome: String,
    pub(crate) source_status: String,
    pub(crate) latency_ms: Option<i64>,
    pub(crate) ping_latency_ms: Option<i64>,
    pub(crate) safe_message: Option<String>,
    pub(crate) first_seen_run_id: String,
    pub(crate) last_seen_run_id: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PublishedStatusSourceRow {
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) source_kind: String,
    pub(crate) source_state: String,
    pub(crate) last_attempt_at: String,
    pub(crate) last_success_at: Option<String>,
    pub(crate) last_complete_at: Option<String>,
    pub(crate) last_error_kind: Option<String>,
    pub(crate) monitor_count: i64,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PublishedMonitorRow {
    pub(crate) id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) source_kind: String,
    pub(crate) upstream_monitor_id: String,
    pub(crate) identity_kind: String,
    pub(crate) name: String,
    pub(crate) provider: String,
    pub(crate) group_name: Option<String>,
    pub(crate) primary_model: String,
    pub(crate) extra_models_json: String,
    pub(crate) presence_status: String,
    pub(crate) current_outcome: String,
    pub(crate) source_status: String,
    pub(crate) current_latency_ms: Option<i64>,
    pub(crate) current_ping_latency_ms: Option<i64>,
    pub(crate) upstream_checked_at_ms: Option<i64>,
    pub(crate) last_seen_run_id: String,
    pub(crate) last_seen_at: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PublishedMonitorSampleRow {
    pub(crate) id: String,
    pub(crate) monitor_id: String,
    pub(crate) model: String,
    pub(crate) checked_at_ms: i64,
    pub(crate) outcome: String,
    pub(crate) source_status: String,
    pub(crate) latency_ms: Option<i64>,
    pub(crate) ping_latency_ms: Option<i64>,
    pub(crate) safe_message: Option<String>,
    pub(crate) first_seen_run_id: String,
    pub(crate) last_seen_run_id: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PublishedStatusWorkspaceRows {
    pub(crate) source: Option<PublishedStatusSourceRow>,
    pub(crate) monitors: Vec<PublishedMonitorRow>,
    pub(crate) samples: Vec<PublishedMonitorSampleRow>,
}

impl StationPublishedStatusStore {
    /// Upserts a source and verifies the endpoint revision once per apply transaction.
    pub(crate) async fn upsert_source(
        &self,
        write: &mut WriteSession,
        source: &PublishedStatusSourceWrite,
    ) -> Result<(), PersistenceError> {
        if source.endpoint_revision < 1
            || source.monitor_count.is_some_and(|count| {
                !(0..=i64::from(MAX_PUBLISHED_STATUS_MONITORS)).contains(&count)
            })
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        assert_station_endpoint_revision(write, &source.station_id, source.endpoint_revision)
            .await?;

        sqlx::query(
            r#"
            INSERT INTO station_published_status_sources (
                station_id, endpoint_revision, source_kind, source_state, last_attempt_at,
                last_success_at, last_complete_at, last_error_kind, monitor_count,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, COALESCE(?9, 0), ?10, ?11)
            ON CONFLICT(station_id, endpoint_revision, source_kind) DO UPDATE SET
                source_state = excluded.source_state,
                last_attempt_at = excluded.last_attempt_at,
                last_success_at = COALESCE(
                    excluded.last_success_at,
                    station_published_status_sources.last_success_at
                ),
                last_complete_at = COALESCE(
                    excluded.last_complete_at,
                    station_published_status_sources.last_complete_at
                ),
                last_error_kind = excluded.last_error_kind,
                monitor_count = CASE
                    WHEN ?12 THEN excluded.monitor_count
                    ELSE station_published_status_sources.monitor_count
                END,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&source.station_id)
        .bind(source.endpoint_revision)
        .bind(&source.source_kind)
        .bind(&source.source_state)
        .bind(&source.last_attempt_at)
        .bind(&source.last_success_at)
        .bind(&source.last_complete_at)
        .bind(&source.last_error_kind)
        .bind(source.monitor_count)
        .bind(&source.created_at)
        .bind(&source.updated_at)
        .bind(source.monitor_count.is_some())
        .execute(write.connection())
        .await?;
        Ok(())
    }

    /// Returns the durable monitor id because a retry may hit an existing identity.
    pub(crate) async fn upsert_monitor(
        &self,
        write: &mut WriteSession,
        monitor: &PublishedMonitorWrite,
    ) -> Result<String, PersistenceError> {
        sqlx::query(
            r#"
            INSERT INTO station_published_monitors (
                id, station_id, endpoint_revision, source_kind, upstream_monitor_id,
                identity_kind, name, provider, group_name, primary_model, extra_models_json,
                presence_status, current_outcome, source_status, current_latency_ms,
                current_ping_latency_ms, availability_7d_percent, upstream_checked_at,
                last_seen_run_id, last_seen_at, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                'current', ?12, ?13, ?14, ?15, NULL, ?16, ?17, ?18, ?19, ?20
            )
            ON CONFLICT(station_id, endpoint_revision, source_kind, upstream_monitor_id)
            DO UPDATE SET
                identity_kind = excluded.identity_kind,
                name = excluded.name,
                provider = excluded.provider,
                group_name = excluded.group_name,
                primary_model = excluded.primary_model,
                extra_models_json = excluded.extra_models_json,
                presence_status = 'current',
                current_outcome = excluded.current_outcome,
                source_status = excluded.source_status,
                current_latency_ms = excluded.current_latency_ms,
                current_ping_latency_ms = excluded.current_ping_latency_ms,
                availability_7d_percent = NULL,
                upstream_checked_at = excluded.upstream_checked_at,
                last_seen_run_id = excluded.last_seen_run_id,
                last_seen_at = excluded.last_seen_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&monitor.id)
        .bind(&monitor.station_id)
        .bind(monitor.endpoint_revision)
        .bind(&monitor.source_kind)
        .bind(&monitor.upstream_monitor_id)
        .bind(&monitor.identity_kind)
        .bind(&monitor.name)
        .bind(&monitor.provider)
        .bind(&monitor.group_name)
        .bind(&monitor.primary_model)
        .bind(&monitor.extra_models_json)
        .bind(&monitor.current_outcome)
        .bind(&monitor.source_status)
        .bind(monitor.current_latency_ms)
        .bind(monitor.current_ping_latency_ms)
        // Migration 41 deliberately stores timestamps as TEXT. Persist a
        // canonical decimal representation so the read path can decode both
        // existing and newly written facts consistently.
        .bind(
            monitor
                .upstream_checked_at_ms
                .map(|value| value.to_string()),
        )
        .bind(&monitor.last_seen_run_id)
        .bind(&monitor.last_seen_at)
        .bind(&monitor.created_at)
        .bind(&monitor.updated_at)
        .execute(write.connection())
        .await?;

        let id = sqlx::query_scalar::<_, String>(
            r#"
            SELECT id
            FROM station_published_monitors
            WHERE station_id = ?1
              AND endpoint_revision = ?2
              AND source_kind = ?3
              AND upstream_monitor_id = ?4
            "#,
        )
        .bind(&monitor.station_id)
        .bind(monitor.endpoint_revision)
        .bind(&monitor.source_kind)
        .bind(&monitor.upstream_monitor_id)
        .fetch_optional(write.connection())
        .await?
        .ok_or(PersistenceError::NotFound)?;
        Ok(id)
    }

    pub(crate) async fn upsert_sample(
        &self,
        write: &mut WriteSession,
        sample: &PublishedMonitorSampleWrite,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            r#"
            INSERT INTO station_published_monitor_samples (
                id, monitor_id, model, checked_at, outcome, source_status, latency_ms,
                ping_latency_ms, safe_message, first_seen_run_id, last_seen_run_id,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(monitor_id, model, checked_at) DO UPDATE SET
                outcome = excluded.outcome,
                source_status = excluded.source_status,
                latency_ms = excluded.latency_ms,
                ping_latency_ms = excluded.ping_latency_ms,
                safe_message = excluded.safe_message,
                last_seen_run_id = excluded.last_seen_run_id,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&sample.id)
        .bind(&sample.monitor_id)
        .bind(&sample.model)
        .bind(sample.checked_at_ms.to_string())
        .bind(&sample.outcome)
        .bind(&sample.source_status)
        .bind(sample.latency_ms)
        .bind(sample.ping_latency_ms)
        .bind(&sample.safe_message)
        .bind(&sample.first_seen_run_id)
        .bind(&sample.last_seen_run_id)
        .bind(&sample.created_at)
        .bind(&sample.updated_at)
        .execute(write.connection())
        .await?;
        Ok(())
    }

    /// Only complete inventories may mark omitted monitors missing.
    pub(crate) async fn mark_unseen_monitors_missing(
        &self,
        write: &mut WriteSession,
        station_id: &str,
        endpoint_revision: i64,
        source_kind: &str,
        seen_monitor_ids: &[String],
        updated_at: &str,
    ) -> Result<u64, PersistenceError> {
        let mut query = QueryBuilder::<Sqlite>::new(
            "UPDATE station_published_monitors SET presence_status = 'missing', updated_at = ",
        );
        query
            .push_bind(updated_at)
            .push(" WHERE station_id = ")
            .push_bind(station_id)
            .push(" AND endpoint_revision = ")
            .push_bind(endpoint_revision)
            .push(" AND source_kind = ")
            .push_bind(source_kind)
            .push(" AND presence_status = 'current'");
        if !seen_monitor_ids.is_empty() {
            query.push(" AND id NOT IN (");
            let mut ids = query.separated(", ");
            for monitor_id in seen_monitor_ids {
                ids.push_bind(monitor_id);
            }
            ids.push_unseparated(")");
        }
        let result = query.build().execute(write.connection()).await?;
        Ok(result.rows_affected())
    }

    /// Retains history in SQLite, rather than loading unbounded samples into Rust.
    pub(crate) async fn retain_active_samples(
        &self,
        write: &mut WriteSession,
        station_id: &str,
        endpoint_revision: i64,
        source_kind: &str,
    ) -> Result<u64, PersistenceError> {
        let result = sqlx::query(
            r#"
            DELETE FROM station_published_monitor_samples
            WHERE id IN (
                SELECT id
                FROM (
                    SELECT sample.id,
                           ROW_NUMBER() OVER (
                               PARTITION BY sample.monitor_id, sample.model
                               ORDER BY CAST(sample.checked_at AS INTEGER) DESC, sample.id DESC
                           ) AS row_number
                    FROM station_published_monitor_samples AS sample
                    JOIN station_published_monitors AS monitor
                      ON monitor.id = sample.monitor_id
                    WHERE monitor.station_id = ?1
                      AND monitor.endpoint_revision = ?2
                      AND monitor.source_kind = ?3
                      AND monitor.presence_status = 'current'
                )
                WHERE row_number > ?4
            )
            "#,
        )
        .bind(station_id)
        .bind(endpoint_revision)
        .bind(source_kind)
        .bind(i64::from(MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL))
        .execute(write.connection())
        .await?;
        Ok(result.rows_affected())
    }

    /// Drops display facts tied to superseded endpoints in the same apply
    /// transaction. Monitor deletion cascades to their samples.
    pub(crate) async fn purge_other_endpoint_revisions(
        &self,
        write: &mut WriteSession,
        station_id: &str,
        endpoint_revision: i64,
        source_kind: &str,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            r#"
            DELETE FROM station_published_monitors
            WHERE station_id = ?1
              AND source_kind = ?2
              AND endpoint_revision <> ?3
            "#,
        )
        .bind(station_id)
        .bind(source_kind)
        .bind(endpoint_revision)
        .execute(write.connection())
        .await?;
        sqlx::query(
            r#"
            DELETE FROM station_published_status_sources
            WHERE station_id = ?1
              AND source_kind = ?2
              AND endpoint_revision <> ?3
            "#,
        )
        .bind(station_id)
        .bind(source_kind)
        .bind(endpoint_revision)
        .execute(write.connection())
        .await?;
        Ok(())
    }

    /// Removes missing monitors after the caller's 30-day retention cutoff.
    pub(crate) async fn delete_missing_before(
        &self,
        write: &mut WriteSession,
        station_id: &str,
        endpoint_revision: i64,
        source_kind: &str,
        cutoff: &str,
    ) -> Result<u64, PersistenceError> {
        let result = sqlx::query(
            r#"
            DELETE FROM station_published_monitors
            WHERE station_id = ?1
              AND endpoint_revision = ?2
              AND source_kind = ?3
              AND presence_status = 'missing'
              AND updated_at < ?4
            "#,
        )
        .bind(station_id)
        .bind(endpoint_revision)
        .bind(source_kind)
        .bind(cutoff)
        .execute(write.connection())
        .await?;
        Ok(result.rows_affected())
    }

    /// Loads the whole workspace in three bounded queries, never one query per monitor.
    pub(crate) async fn load_workspace(
        &self,
        read: &mut ReadSession,
        station_id: &str,
        endpoint_revision: i64,
        source_kind: &str,
        monitor_limit: u32,
        samples_per_model_limit: u32,
    ) -> Result<PublishedStatusWorkspaceRows, PersistenceError> {
        let monitor_limit = monitor_limit.min(MAX_PUBLISHED_STATUS_MONITORS);
        let samples_per_model_limit =
            samples_per_model_limit.min(MAX_PUBLISHED_STATUS_SAMPLES_PER_MODEL);
        let source = sqlx::query(
            r#"
            SELECT station_id, endpoint_revision, source_kind, source_state, last_attempt_at,
                   last_success_at, last_complete_at, last_error_kind, monitor_count,
                   created_at, updated_at
            FROM station_published_status_sources
            WHERE station_id = ?1 AND endpoint_revision = ?2 AND source_kind = ?3
            "#,
        )
        .bind(station_id)
        .bind(endpoint_revision)
        .bind(source_kind)
        .fetch_optional(read.connection())
        .await?
        .map(row_to_source)
        .transpose()?;

        let monitors = sqlx::query(
            r#"
            SELECT id, station_id, endpoint_revision, source_kind, upstream_monitor_id,
                   identity_kind, name, provider, group_name, primary_model, extra_models_json,
                   presence_status, current_outcome, source_status, current_latency_ms,
                    current_ping_latency_ms, CAST(upstream_checked_at AS INTEGER) AS upstream_checked_at,
                   last_seen_run_id, last_seen_at, created_at, updated_at
            FROM station_published_monitors
            WHERE station_id = ?1
              AND endpoint_revision = ?2
              AND source_kind = ?3
              AND presence_status = 'current'
            ORDER BY provider COLLATE NOCASE ASC,
                     group_name COLLATE NOCASE ASC,
                     name COLLATE NOCASE ASC,
                     primary_model COLLATE NOCASE ASC,
                     upstream_monitor_id COLLATE NOCASE ASC,
                     id ASC
            LIMIT ?4
            "#,
        )
        .bind(station_id)
        .bind(endpoint_revision)
        .bind(source_kind)
        .bind(i64::from(monitor_limit))
        .fetch_all(read.connection())
        .await?
        .into_iter()
        .map(row_to_monitor)
        .collect::<Result<Vec<_>, _>>()?;

        let sample_limit = i64::from(monitor_limit) * i64::from(samples_per_model_limit);
        let samples = sqlx::query(
            r#"
            WITH selected_monitors AS (
                SELECT id, primary_model
                FROM station_published_monitors
                WHERE station_id = ?1
                  AND endpoint_revision = ?2
                  AND source_kind = ?3
                  AND presence_status = 'current'
                ORDER BY provider COLLATE NOCASE ASC,
                         group_name COLLATE NOCASE ASC,
                         name COLLATE NOCASE ASC,
                         primary_model COLLATE NOCASE ASC,
                         upstream_monitor_id COLLATE NOCASE ASC,
                         id ASC
                LIMIT ?4
            ), ranked_samples AS (
                SELECT sample.*,
                       ROW_NUMBER() OVER (
                           PARTITION BY sample.monitor_id, sample.model
                           ORDER BY CAST(sample.checked_at AS INTEGER) DESC, sample.id DESC
                       ) AS row_number
                FROM station_published_monitor_samples AS sample
                JOIN selected_monitors
                  ON selected_monitors.id = sample.monitor_id
                 AND selected_monitors.primary_model = sample.model
            )
            SELECT id, monitor_id, model, CAST(checked_at AS INTEGER) AS checked_at,
                   outcome, source_status, latency_ms,
                   ping_latency_ms, safe_message, first_seen_run_id, last_seen_run_id,
                   created_at, updated_at
            FROM ranked_samples
            WHERE row_number <= ?5
            ORDER BY monitor_id ASC, model ASC, CAST(checked_at AS INTEGER) ASC, id ASC
            LIMIT ?6
            "#,
        )
        .bind(station_id)
        .bind(endpoint_revision)
        .bind(source_kind)
        .bind(i64::from(monitor_limit))
        .bind(i64::from(samples_per_model_limit))
        .bind(sample_limit)
        .fetch_all(read.connection())
        .await?
        .into_iter()
        .map(row_to_sample)
        .collect::<Result<Vec<_>, _>>()?;

        Ok(PublishedStatusWorkspaceRows {
            source,
            monitors,
            samples,
        })
    }
}

async fn assert_station_endpoint_revision(
    write: &mut WriteSession,
    station_id: &str,
    expected_endpoint_revision: i64,
) -> Result<(), PersistenceError> {
    let revision =
        sqlx::query_scalar::<_, i64>("SELECT endpoint_revision FROM stations WHERE id = ?1")
            .bind(station_id)
            .fetch_optional(write.connection())
            .await?
            .ok_or(PersistenceError::NotFound)?;
    if revision != expected_endpoint_revision {
        return Err(PersistenceError::StaleRevision);
    }
    Ok(())
}

fn row_to_source(
    row: sqlx::sqlite::SqliteRow,
) -> Result<PublishedStatusSourceRow, PersistenceError> {
    Ok(PublishedStatusSourceRow {
        station_id: row.try_get("station_id")?,
        endpoint_revision: row.try_get("endpoint_revision")?,
        source_kind: row.try_get("source_kind")?,
        source_state: row.try_get("source_state")?,
        last_attempt_at: row.try_get("last_attempt_at")?,
        last_success_at: row.try_get("last_success_at")?,
        last_complete_at: row.try_get("last_complete_at")?,
        last_error_kind: row.try_get("last_error_kind")?,
        monitor_count: row.try_get("monitor_count")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_monitor(row: sqlx::sqlite::SqliteRow) -> Result<PublishedMonitorRow, PersistenceError> {
    Ok(PublishedMonitorRow {
        id: row.try_get("id")?,
        station_id: row.try_get("station_id")?,
        endpoint_revision: row.try_get("endpoint_revision")?,
        source_kind: row.try_get("source_kind")?,
        upstream_monitor_id: row.try_get("upstream_monitor_id")?,
        identity_kind: row.try_get("identity_kind")?,
        name: row.try_get("name")?,
        provider: row.try_get("provider")?,
        group_name: row.try_get("group_name")?,
        primary_model: row.try_get("primary_model")?,
        extra_models_json: row.try_get("extra_models_json")?,
        presence_status: row.try_get("presence_status")?,
        current_outcome: row.try_get("current_outcome")?,
        source_status: row.try_get("source_status")?,
        current_latency_ms: row.try_get("current_latency_ms")?,
        current_ping_latency_ms: row.try_get("current_ping_latency_ms")?,
        upstream_checked_at_ms: row.try_get("upstream_checked_at")?,
        last_seen_run_id: row.try_get("last_seen_run_id")?,
        last_seen_at: row.try_get("last_seen_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn row_to_sample(
    row: sqlx::sqlite::SqliteRow,
) -> Result<PublishedMonitorSampleRow, PersistenceError> {
    Ok(PublishedMonitorSampleRow {
        id: row.try_get("id")?,
        monitor_id: row.try_get("monitor_id")?,
        model: row.try_get("model")?,
        checked_at_ms: row.try_get("checked_at")?,
        outcome: row.try_get("outcome")?,
        source_status: row.try_get("source_status")?,
        latency_ms: row.try_get("latency_ms")?,
        ping_latency_ms: row.try_get("ping_latency_ms")?,
        safe_message: row.try_get("safe_message")?,
        first_seen_run_id: row.try_get("first_seen_run_id")?,
        last_seen_run_id: row.try_get("last_seen_run_id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use crate::persistence::runtime::PersistenceRuntime;

    use super::*;

    #[tokio::test]
    async fn station_published_status_store_retains_the_latest_sixty_samples() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("published-status.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize database");
        let store = StationPublishedStatusStore;

        let mut write = runtime.begin_write().await.expect("write session");
        seed_station(&mut write, "station-1", 1).await;
        store
            .upsert_source(&mut write, &source_write("station-1", "available", Some(1)))
            .await
            .expect("source");
        let monitor_id = store
            .upsert_monitor(&mut write, &monitor_write("station-1", "monitor-1"))
            .await
            .expect("monitor");
        for index in 0..=60 {
            let mut sample = sample_write(&monitor_id, 999 + index, &format!("sample-{index}"));
            if index == 0 {
                sample.outcome = "unavailable".into();
            }
            store
                .upsert_sample(&mut write, &sample)
                .await
                .expect("sample");
        }
        assert_eq!(
            store
                .retain_active_samples(&mut write, "station-1", 1, "sub2api_channel_monitors")
                .await
                .expect("retention"),
            1
        );
        write.commit().await.expect("commit");

        let mut read = runtime.begin_read().await.expect("read session");
        let workspace = store
            .load_workspace(
                &mut read,
                "station-1",
                1,
                "sub2api_channel_monitors",
                512,
                60,
            )
            .await
            .expect("workspace");
        assert_eq!(workspace.monitors.len(), 1);
        assert_eq!(workspace.samples.len(), 60);
        assert_eq!(workspace.samples[0].checked_at_ms, 1_000);
        assert_eq!(workspace.samples.last().unwrap().checked_at_ms, 1_059);
        assert!(workspace
            .samples
            .iter()
            .all(|sample| sample.outcome == "available"));
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn station_published_status_store_clears_legacy_upstream_availability_on_upsert() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("published-status.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize database");
        let store = StationPublishedStatusStore;

        let mut write = runtime.begin_write().await.expect("write session");
        seed_station(&mut write, "station-1", 1).await;
        let monitor_id = store
            .upsert_monitor(&mut write, &monitor_write("station-1", "monitor-1"))
            .await
            .expect("first monitor upsert");
        sqlx::query(
            "UPDATE station_published_monitors SET availability_7d_percent = 99.5 WHERE id = ?1",
        )
        .bind(&monitor_id)
        .execute(write.connection())
        .await
        .expect("seed legacy availability");

        let mut updated = monitor_write("station-1", "monitor-1");
        updated.name = "Updated fixture monitor".into();
        store
            .upsert_monitor(&mut write, &updated)
            .await
            .expect("updated monitor upsert");
        let availability = sqlx::query_scalar::<_, Option<f64>>(
            "SELECT availability_7d_percent FROM station_published_monitors WHERE id = ?1",
        )
        .bind(&monitor_id)
        .fetch_one(write.connection())
        .await
        .expect("legacy availability");
        assert_eq!(availability, None);

        write.commit().await.expect("commit");
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn station_published_status_store_failed_source_preserves_last_success() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("published-status.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize database");
        let store = StationPublishedStatusStore;

        let mut write = runtime.begin_write().await.expect("write session");
        seed_station(&mut write, "station-1", 1).await;
        store
            .upsert_source(&mut write, &source_write("station-1", "available", Some(1)))
            .await
            .expect("successful source");
        let mut failed = source_write("station-1", "failed", None);
        failed.last_attempt_at = "2026-08-16T01:00:00.000Z".into();
        failed.last_error_kind = Some("network_failed".into());
        failed.updated_at = failed.last_attempt_at.clone();
        store
            .upsert_source(&mut write, &failed)
            .await
            .expect("failed source");
        write.commit().await.expect("commit");

        let mut read = runtime.begin_read().await.expect("read session");
        let workspace = store
            .load_workspace(
                &mut read,
                "station-1",
                1,
                "sub2api_channel_monitors",
                512,
                60,
            )
            .await
            .expect("workspace");
        let source = workspace.source.expect("source row");
        assert_eq!(source.source_state, "failed");
        assert_eq!(source.monitor_count, 1);
        assert_eq!(
            source.last_success_at.as_deref(),
            Some("2026-08-16T00:00:00.000Z")
        );
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn station_published_status_workspace_excludes_missing_monitors() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("published-status.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize database");
        let store = StationPublishedStatusStore;

        let mut write = runtime.begin_write().await.expect("write session");
        seed_station(&mut write, "station-1", 1).await;
        store
            .upsert_source(&mut write, &source_write("station-1", "available", Some(1)))
            .await
            .expect("source");
        store
            .upsert_monitor(&mut write, &monitor_write("station-1", "monitor-1"))
            .await
            .expect("monitor");
        store
            .mark_unseen_monitors_missing(
                &mut write,
                "station-1",
                1,
                "sub2api_channel_monitors",
                &[],
                "2026-08-16T01:00:00.000Z",
            )
            .await
            .expect("mark missing");
        write.commit().await.expect("commit");

        let mut read = runtime.begin_read().await.expect("read session");
        let workspace = store
            .load_workspace(
                &mut read,
                "station-1",
                1,
                "sub2api_channel_monitors",
                512,
                60,
            )
            .await
            .expect("workspace");
        assert!(workspace.monitors.is_empty());
        assert!(workspace.samples.is_empty());
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn station_published_status_workspace_clamps_to_512_monitors_and_60_samples_each() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("published-status.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize database");
        let store = StationPublishedStatusStore;

        let mut write = runtime.begin_write().await.expect("write session");
        seed_station(&mut write, "station-1", 1).await;
        store
            .upsert_source(
                &mut write,
                &source_write("station-1", "available", Some(512)),
            )
            .await
            .expect("source");
        // Seed one monitor beyond the workspace limit and a full 60-sample
        // history for every monitor. The read path must still stay bounded.
        sqlx::query(
            r#"
            WITH RECURSIVE monitor_numbers(value) AS (
                SELECT 0
                UNION ALL
                SELECT value + 1 FROM monitor_numbers WHERE value < 512
            )
            INSERT INTO station_published_monitors (
                id, station_id, endpoint_revision, source_kind, upstream_monitor_id,
                identity_kind, name, provider, group_name, primary_model, extra_models_json,
                presence_status, current_outcome, source_status, current_latency_ms,
                current_ping_latency_ms, upstream_checked_at,
                last_seen_run_id, last_seen_at, created_at, updated_at
            )
            SELECT printf('monitor-%03d', value),
                   'station-1',
                   1,
                   'sub2api_channel_monitors',
                   printf('upstream-%03d', value),
                   'upstream_id',
                   printf('Fixture Monitor %03d', value),
                   'fixture',
                   NULL,
                   'fixture-model',
                   '[]',
                   'current',
                   'available',
                   'healthy',
                   1,
                   1,
                   value,
                   'run-1',
                   '0',
                   '0',
                   '0'
            FROM monitor_numbers
            "#,
        )
        .execute(write.connection())
        .await
        .expect("seed monitors");
        sqlx::query(
            r#"
            WITH RECURSIVE monitor_numbers(value) AS (
                SELECT 0
                UNION ALL
                SELECT value + 1 FROM monitor_numbers WHERE value < 512
            ), sample_numbers(value) AS (
                SELECT 0
                UNION ALL
                SELECT value + 1 FROM sample_numbers WHERE value < 59
            )
            INSERT INTO station_published_monitor_samples (
                id, monitor_id, model, checked_at, outcome, source_status, latency_ms,
                ping_latency_ms, safe_message, first_seen_run_id, last_seen_run_id,
                created_at, updated_at
            )
            SELECT printf('sample-%03d-%02d', monitor_numbers.value, sample_numbers.value),
                   printf('monitor-%03d', monitor_numbers.value),
                   'fixture-model',
                   sample_numbers.value,
                   'available',
                   'healthy',
                   1,
                   1,
                   NULL,
                   'run-1',
                   'run-1',
                   '0',
                   '0'
            FROM monitor_numbers
            CROSS JOIN sample_numbers
            "#,
        )
        .execute(write.connection())
        .await
        .expect("seed samples");
        write.commit().await.expect("commit");

        let mut read = runtime.begin_read().await.expect("read session");
        let workspace = store
            .load_workspace(
                &mut read,
                "station-1",
                1,
                "sub2api_channel_monitors",
                1_000,
                1_000,
            )
            .await
            .expect("bounded workspace");
        assert_eq!(workspace.monitors.len(), 512);
        assert_eq!(workspace.samples.len(), 512 * 60);
        assert_eq!(workspace.monitors[0].id, "monitor-000");
        assert_eq!(workspace.monitors.last().unwrap().id, "monitor-511");
        assert_eq!(workspace.samples[0].monitor_id, "monitor-000");
        assert_eq!(workspace.samples[0].checked_at_ms, 0);
        assert_eq!(workspace.samples.last().unwrap().monitor_id, "monitor-511");
        assert_eq!(workspace.samples.last().unwrap().checked_at_ms, 59);

        let narrow_workspace = store
            .load_workspace(
                &mut read,
                "station-1",
                1,
                "sub2api_channel_monitors",
                1,
                1_000,
            )
            .await
            .expect("narrow bounded workspace");
        assert_eq!(narrow_workspace.monitors.len(), 1);
        assert_eq!(narrow_workspace.samples.len(), 60);

        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn station_published_status_store_revision_fence_rejects_stale_writes() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("published-status.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize database");
        let store = StationPublishedStatusStore;
        let mut write = runtime.begin_write().await.expect("write session");
        seed_station(&mut write, "station-1", 2).await;
        let stale = source_write("station-1", "available", Some(0));
        assert!(matches!(
            store.upsert_source(&mut write, &stale).await,
            Err(PersistenceError::StaleRevision)
        ));
        drop(write);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn station_published_status_store_purges_superseded_endpoint_revisions() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("published-status.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize database");
        let store = StationPublishedStatusStore;

        let mut write = runtime.begin_write().await.expect("write session");
        seed_station(&mut write, "station-1", 1).await;
        store
            .upsert_source(&mut write, &source_write("station-1", "available", Some(1)))
            .await
            .expect("source revision one");
        let monitor_id = store
            .upsert_monitor(&mut write, &monitor_write("station-1", "monitor-1"))
            .await
            .expect("monitor revision one");
        store
            .upsert_sample(&mut write, &sample_write(&monitor_id, 1_000, "sample-1"))
            .await
            .expect("sample revision one");
        sqlx::query("UPDATE stations SET endpoint_revision = 2 WHERE id = 'station-1'")
            .execute(write.connection())
            .await
            .expect("advance endpoint revision");
        let mut current = source_write("station-1", "available", Some(0));
        current.endpoint_revision = 2;
        store
            .upsert_source(&mut write, &current)
            .await
            .expect("source revision two");
        store
            .purge_other_endpoint_revisions(&mut write, "station-1", 2, "sub2api_channel_monitors")
            .await
            .expect("purge superseded facts");
        let old_sources: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM station_published_status_sources WHERE endpoint_revision = 1",
        )
        .fetch_one(write.connection())
        .await
        .expect("old sources count");
        let old_monitors: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM station_published_monitors WHERE endpoint_revision = 1",
        )
        .fetch_one(write.connection())
        .await
        .expect("old monitors count");
        let samples: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM station_published_monitor_samples")
                .fetch_one(write.connection())
                .await
                .expect("samples count");
        assert_eq!(old_sources, 0);
        assert_eq!(old_monitors, 0);
        assert_eq!(samples, 0);
        write.commit().await.expect("commit");
        runtime.close().await.expect("close runtime");
    }

    async fn seed_station(write: &mut WriteSession, station_id: &str, endpoint_revision: i64) {
        sqlx::query(
            r#"
            INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, endpoint_revision,
                created_at, updated_at
            ) VALUES (?1, 'Fixture Station', 'sub2api', 'https://example.invalid',
                      'https://example.invalid/v1', ?2, '0', '0')
            "#,
        )
        .bind(station_id)
        .bind(endpoint_revision)
        .execute(write.connection())
        .await
        .expect("seed station");
    }

    fn source_write(
        station_id: &str,
        source_state: &str,
        monitor_count: Option<i64>,
    ) -> PublishedStatusSourceWrite {
        PublishedStatusSourceWrite {
            station_id: station_id.into(),
            endpoint_revision: 1,
            source_kind: "sub2api_channel_monitors".into(),
            source_state: source_state.into(),
            last_attempt_at: "2026-08-16T00:00:00.000Z".into(),
            last_success_at: (source_state == "available")
                .then(|| "2026-08-16T00:00:00.000Z".into()),
            last_complete_at: (source_state == "available")
                .then(|| "2026-08-16T00:00:00.000Z".into()),
            last_error_kind: None,
            monitor_count,
            created_at: "2026-08-16T00:00:00.000Z".into(),
            updated_at: "2026-08-16T00:00:00.000Z".into(),
        }
    }

    fn monitor_write(station_id: &str, id: &str) -> PublishedMonitorWrite {
        PublishedMonitorWrite {
            id: id.into(),
            station_id: station_id.into(),
            endpoint_revision: 1,
            source_kind: "sub2api_channel_monitors".into(),
            upstream_monitor_id: "upstream-monitor-1".into(),
            identity_kind: "upstream_id".into(),
            name: "Fixture Monitor".into(),
            provider: "fixture".into(),
            group_name: None,
            primary_model: "fixture-model".into(),
            extra_models_json: "[]".into(),
            current_outcome: "available".into(),
            source_status: "available".into(),
            current_latency_ms: Some(1),
            current_ping_latency_ms: Some(1),
            upstream_checked_at_ms: Some(1_700_000_000_000),
            last_seen_run_id: "run-1".into(),
            last_seen_at: "2026-08-16T00:00:00.000Z".into(),
            created_at: "2026-08-16T00:00:00.000Z".into(),
            updated_at: "2026-08-16T00:00:00.000Z".into(),
        }
    }

    fn sample_write(monitor_id: &str, second: u32, id: &str) -> PublishedMonitorSampleWrite {
        PublishedMonitorSampleWrite {
            id: id.into(),
            monitor_id: monitor_id.into(),
            model: "fixture-model".into(),
            checked_at_ms: i64::from(second),
            outcome: "available".into(),
            source_status: "available".into(),
            latency_ms: Some(1),
            ping_latency_ms: Some(1),
            safe_message: None,
            first_seen_run_id: "run-1".into(),
            last_seen_run_id: "run-1".into(),
            created_at: "2026-08-16T00:00:00.000Z".into(),
            updated_at: "2026-08-16T00:00:00.000Z".into(),
        }
    }
}
