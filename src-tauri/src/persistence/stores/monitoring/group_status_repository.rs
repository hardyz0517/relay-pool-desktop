use sqlx::{Row, SqliteConnection};

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    models::pricing_group_monitoring::{
        CanonicalGroupRef, LatestOutcome, MonitorSnapshot, RunningSnapshot, StationKeySnapshot,
        TargetResultSnapshot,
    },
    persistence::{error::PersistenceError, ReadSession},
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PricingGroupMonitorStatusRepository;

#[cfg(test)]
static QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone)]
pub(crate) struct GroupStatusRows {
    pub(crate) resolutions: Vec<ResolvedGroupRefRow>,
    pub(crate) keys: Vec<MatchedKeyRow>,
    pub(crate) monitors: Vec<MonitorSnapshot>,
    pub(crate) target_results: Vec<TargetResultSnapshot>,
    pub(crate) running: Vec<RunningSnapshot>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedGroupRefRow {
    pub(crate) group_ref_key: String,
    pub(crate) match_kind: String,
}

#[derive(Debug, Clone)]
pub(crate) struct MatchedKeyRow {
    pub(crate) group_ref_key: String,
    pub(crate) match_kind: String,
    pub(crate) key: StationKeySnapshot,
}

impl PricingGroupMonitorStatusRepository {
    #[cfg(test)]
    pub(crate) fn reset_query_count() {
        QUERY_COUNT.store(0, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn query_count() -> usize {
        QUERY_COUNT.load(Ordering::SeqCst)
    }

    pub(crate) async fn load(
        &self,
        read: &mut ReadSession,
        groups: &[CanonicalGroupRef],
    ) -> Result<GroupStatusRows, PersistenceError> {
        if groups.is_empty() {
            return Ok(GroupStatusRows {
                resolutions: Vec::new(),
                keys: Vec::new(),
                monitors: Vec::new(),
                target_results: Vec::new(),
                running: Vec::new(),
            });
        }
        let encoded_groups = groups
            .iter()
            .map(|group| {
                serde_json::json!({
                    "stationId": group.station_id,
                    "groupBindingId": group.group_binding_id,
                    "groupIdHash": group.group_id_hash,
                    "groupKeyHash": group.group_key_hash,
                    "canonicalKey": group.canonical_key().ok(),
                })
            })
            .collect::<Vec<_>>();
        let encoded = serde_json::to_string(&encoded_groups)
            .map_err(|_| PersistenceError::InvariantViolation("group refs encode".into()))?;
        self.load_connection(read.connection(), groups, &encoded)
            .await
    }

    pub(crate) async fn load_connection(
        &self,
        connection: &mut SqliteConnection,
        groups: &[CanonicalGroupRef],
        encoded: &str,
    ) -> Result<GroupStatusRows, PersistenceError> {
        let resolutions = self.load_resolutions(connection, encoded).await?;
        let keys = self.load_keys(connection, encoded).await?;
        let station_ids: Vec<String> = groups
            .iter()
            .map(|group| group.station_id.trim().to_owned())
            .collect();
        let station_ids_json = serde_json::to_string(&station_ids)
            .map_err(|_| PersistenceError::InvariantViolation("station ids encode".into()))?;
        let monitors = self.load_monitors(connection, &station_ids_json).await?;
        let monitor_ids: Vec<String> = monitors.iter().map(|monitor| monitor.id.clone()).collect();
        let monitor_ids_json = serde_json::to_string(&monitor_ids)
            .map_err(|_| PersistenceError::InvariantViolation("monitor ids encode".into()))?;
        let target_results = self
            .load_latest_results(connection, &monitor_ids_json)
            .await?;
        let running = self.load_running(connection, &monitor_ids_json).await?;
        Ok(GroupStatusRows {
            resolutions,
            keys,
            monitors,
            target_results,
            running,
        })
    }

    async fn load_resolutions(
        &self,
        connection: &mut SqliteConnection,
        encoded_groups: &str,
    ) -> Result<Vec<ResolvedGroupRefRow>, PersistenceError> {
        #[cfg(test)]
        QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
        let rows = sqlx::query(
            r#"
            WITH requested AS (
                SELECT
                    json_extract(value, '$.stationId') AS station_id,
                    json_extract(value, '$.groupBindingId') AS group_binding_id,
                    json_extract(value, '$.groupIdHash') AS group_id_hash,
                    json_extract(value, '$.groupKeyHash') AS group_key_hash,
                    json_extract(value, '$.canonicalKey') AS group_ref_key
                FROM json_each(?1)
            )
            SELECT r.group_ref_key,
                   CASE
                       WHEN r.group_binding_id IS NOT NULL THEN 'exact_binding'
                       WHEN r.group_id_hash IS NOT NULL THEN 'group_id_hash'
                       ELSE 'group_key_hash'
                   END AS match_kind
            FROM requested r
            WHERE
                (r.group_binding_id IS NOT NULL AND EXISTS (
                    SELECT 1
                    FROM station_group_bindings b
                    WHERE b.id = r.group_binding_id
                      AND b.station_id = r.station_id
                      AND b.binding_kind = 'station_group'
                ))
                OR
                (r.group_binding_id IS NULL AND r.group_id_hash IS NOT NULL AND 1 = (
                    SELECT COUNT(*)
                    FROM station_group_bindings b
                    WHERE b.station_id = r.station_id
                      AND b.binding_kind = 'station_group'
                      AND b.group_id_hash = r.group_id_hash
                ))
                OR
                (r.group_binding_id IS NULL AND r.group_id_hash IS NULL AND r.group_key_hash IS NOT NULL AND 1 = (
                    SELECT COUNT(*)
                    FROM station_group_bindings b
                    WHERE b.station_id = r.station_id
                      AND b.binding_kind = 'station_group'
                      AND b.group_key_hash = r.group_key_hash
                ))
            "#,
        )
        .bind(encoded_groups)
        .fetch_all(&mut *connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| ResolvedGroupRefRow {
                group_ref_key: row.get("group_ref_key"),
                match_kind: row.get("match_kind"),
            })
            .collect())
    }

    async fn load_keys(
        &self,
        connection: &mut SqliteConnection,
        encoded_groups: &str,
    ) -> Result<Vec<MatchedKeyRow>, PersistenceError> {
        #[cfg(test)]
        QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
        let rows = sqlx::query(
            r#"
            WITH requested AS (
                SELECT
                    json_extract(value, '$.stationId') AS station_id,
                    json_extract(value, '$.groupBindingId') AS group_binding_id,
                    json_extract(value, '$.groupIdHash') AS group_id_hash,
                    json_extract(value, '$.groupKeyHash') AS group_key_hash,
                    json_extract(value, '$.canonicalKey') AS group_ref_key
                FROM json_each(?1)
            )
            SELECT
                r.group_ref_key,
                CASE
                    WHEN r.group_binding_id IS NOT NULL AND k.group_binding_id = r.group_binding_id
                        THEN 'exact_binding'
                    WHEN r.group_binding_id IS NOT NULL AND gb.parent_group_binding_id = r.group_binding_id
                        THEN 'parent_binding'
                    WHEN r.group_id_hash IS NOT NULL THEN 'group_id_hash'
                    ELSE 'group_key_hash'
                END AS match_kind,
                k.id,
                k.priority,
                CAST(k.created_at AS INTEGER) AS created_at_ms,
                k.group_binding_id,
                k.group_id_hash,
                k.enabled,
                CASE WHEN k.api_key_secret_id IS NOT NULL OR trim(k.api_key) <> '' THEN 1 ELSE 0 END AS credentialed
            FROM requested r
            JOIN station_keys k
              ON k.station_id = r.station_id
            LEFT JOIN station_group_bindings gb ON gb.id = k.group_binding_id
            WHERE (
                (r.group_binding_id IS NOT NULL
                    AND (k.group_binding_id = r.group_binding_id
                         OR gb.parent_group_binding_id = r.group_binding_id))
                OR (r.group_binding_id IS NULL
                    AND r.group_id_hash IS NOT NULL
                    AND (
                        k.group_id_hash = r.group_id_hash
                        OR gb.group_id_hash = r.group_id_hash
                    ))
                OR (r.group_binding_id IS NULL
                    AND r.group_id_hash IS NULL
                    AND r.group_key_hash IS NOT NULL
                    AND gb.group_key_hash = r.group_key_hash)
            )
            ORDER BY r.group_ref_key ASC, k.priority ASC, created_at_ms ASC, k.id ASC
            "#,
        )
        .bind(encoded_groups)
        .fetch_all(&mut *connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| MatchedKeyRow {
                group_ref_key: row.get("group_ref_key"),
                match_kind: row.get("match_kind"),
                key: StationKeySnapshot {
                    id: row.get("id"),
                    priority: row.get("priority"),
                    created_at_ms: row.get("created_at_ms"),
                    group_binding_id: row.get("group_binding_id"),
                    group_id_hash: row.get("group_id_hash"),
                    enabled: row.get::<i64, _>("enabled") != 0,
                    credentialed: row.get::<i64, _>("credentialed") != 0,
                },
            })
            .collect())
    }

    async fn load_monitors(
        &self,
        connection: &mut SqliteConnection,
        station_ids: &str,
    ) -> Result<Vec<MonitorSnapshot>, PersistenceError> {
        #[cfg(test)]
        QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
        let rows = sqlx::query(
            r#"
            SELECT m.id, m.station_id, CAST(m.created_at AS INTEGER) AS created_at_ms,
                   m.target_type, m.station_key_id, m.enabled
            FROM channel_monitors m
            JOIN json_each(?1) station_ids
              ON m.station_id = station_ids.value
            ORDER BY m.created_at ASC, m.id ASC
            "#,
        )
        .bind(station_ids)
        .fetch_all(&mut *connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| MonitorSnapshot {
                id: row.get("id"),
                station_id: row.get("station_id"),
                created_at_ms: row.get("created_at_ms"),
                target_type: row.get("target_type"),
                station_key_id: row.get("station_key_id"),
                enabled: row.get::<i64, _>("enabled") != 0,
            })
            .collect())
    }

    async fn load_latest_results(
        &self,
        connection: &mut SqliteConnection,
        monitor_ids: &str,
    ) -> Result<Vec<TargetResultSnapshot>, PersistenceError> {
        #[cfg(test)]
        QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
        let rows = sqlx::query(
            r#"
            WITH ranked AS (
                SELECT tr.id, tr.monitor_id, tr.station_key_id,
                       tr.terminal_outcome, tr.terminal_failure_kind,
                       tr.terminal_reason, tr.finished_at_ms, tr.latency_ms,
                       ROW_NUMBER() OVER (
                           PARTITION BY tr.monitor_id, tr.station_key_id
                           ORDER BY tr.finished_at_ms DESC, tr.id DESC
                       ) AS row_number
                FROM channel_monitor_target_results tr
                JOIN json_each(?1) monitor_ids
                  ON tr.monitor_id = monitor_ids.value
                WHERE tr.finished_at_ms IS NOT NULL
            )
            SELECT id, monitor_id, station_key_id, terminal_outcome,
                   terminal_failure_kind, terminal_reason, finished_at_ms, latency_ms
            FROM ranked
            WHERE row_number = 1
            ORDER BY monitor_id ASC, station_key_id ASC, finished_at_ms DESC, id DESC
            "#,
        )
        .bind(monitor_ids)
        .fetch_all(&mut *connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| TargetResultSnapshot {
                id: row.get("id"),
                monitor_id: row.get("monitor_id"),
                station_key_id: row.get("station_key_id"),
                terminal_outcome: LatestOutcome::from_probe_outcome(
                    row.get::<String, _>("terminal_outcome").as_str(),
                ),
                failure_kind: row.get("terminal_failure_kind"),
                terminal_reason: row.get("terminal_reason"),
                finished_at_ms: row.get("finished_at_ms"),
                latency_ms: row.get("latency_ms"),
            })
            .collect())
    }

    async fn load_running(
        &self,
        connection: &mut SqliteConnection,
        monitor_ids: &str,
    ) -> Result<Vec<RunningSnapshot>, PersistenceError> {
        #[cfg(test)]
        QUERY_COUNT.fetch_add(1, Ordering::SeqCst);
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT e.monitor_id
            FROM channel_monitor_executions e
            JOIN json_each(?1) monitor_ids
              ON e.monitor_id = monitor_ids.value
            WHERE e.status IN ('queued', 'running')
            "#,
        )
        .bind(monitor_ids)
        .fetch_all(&mut *connection)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| RunningSnapshot {
                monitor_id: row.get("monitor_id"),
                station_key_id: None,
            })
            .collect())
    }
}
