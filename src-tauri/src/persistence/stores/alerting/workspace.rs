use sqlx::Row;

use crate::persistence::{error::PersistenceError, ReadSession};

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceIncidentRow {
    pub id: String,
    pub condition_key: String,
    pub event_type: String,
    pub lifecycle_state: String,
    pub severity: String,
    pub station_id: Option<String>,
    pub episode_number: i64,
    pub occurrence_count: i64,
    pub last_seen_at_ms: i64,
    pub last_observation_summary_json: String,
    pub resolved_at_ms: Option<i64>,
    pub updated_at_ms: i64,
    pub seen_at_ms: Option<i64>,
    pub snoozed_until_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceOccurrenceRow {
    pub id: String,
    pub source_observation_key: String,
    pub event_type: String,
    pub observation_kind: String,
    pub severity: String,
    pub reason_code: Option<String>,
    pub source: String,
    pub object_type: String,
    pub object_id: Option<String>,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub observed_at_ms: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceActivityRow {
    pub record_type: String,
    pub id: String,
    pub event_type: String,
    pub severity: String,
    pub station_id: Option<String>,
    pub object_type: Option<String>,
    pub object_id: Option<String>,
    pub station_key_id: Option<String>,
    pub source: Option<String>,
    pub reason_code: Option<String>,
    pub condition_key: Option<String>,
    pub lifecycle_state: Option<String>,
    pub episode_number: Option<i64>,
    pub occurrence_count: Option<i64>,
    pub activity_at_ms: i64,
    pub last_observation_summary_json: Option<String>,
    pub old_value_json: Option<String>,
    pub new_value_json: Option<String>,
    pub impact_json: Option<String>,
    pub resolved_at_ms: Option<i64>,
    pub seen_at_ms: Option<i64>,
    pub snoozed_until_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceDeliveryRow {
    pub id: String,
    pub delivery_key: String,
    pub channel: String,
    pub delivery_kind: String,
    pub status: String,
    pub scheduled_at_ms: i64,
    pub attempt_count: i64,
    pub delivered_at_ms: Option<i64>,
    pub suppressed_reason: Option<String>,
    pub error_code: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct WorkspaceStore;

impl WorkspaceStore {
    pub(crate) async fn list_activity(
        &self,
        session: &mut ReadSession,
        station_id: Option<&str>,
        severity: Option<&str>,
        record_type: Option<&str>,
        unread_only: bool,
        cursor: Option<(i64, &str)>,
        limit: u32,
    ) -> Result<(Vec<WorkspaceActivityRow>, i64, i64), PersistenceError> {
        let limit = i64::from(limit.clamp(1, 200));
        let rows = sqlx::query(
            "WITH activity AS (
                SELECT 'incident' AS record_type, i.id AS id,
                       'incident:' || i.id AS activity_key,
                       i.event_type AS event_type, i.severity AS severity,
                       i.station_id AS station_id, i.object_type AS object_type,
                       i.object_id AS object_id, i.station_key_id AS station_key_id,
                       NULL AS source, NULL AS reason_code,
                       i.condition_key AS condition_key,
                       i.lifecycle_state AS lifecycle_state,
                       i.episode_number AS episode_number,
                       i.occurrence_count AS occurrence_count,
                       i.updated_at_ms AS activity_at_ms,
                       i.last_observation_summary_json AS last_observation_summary_json,
                       NULL AS old_value_json, NULL AS new_value_json, NULL AS impact_json,
                       i.resolved_at_ms AS resolved_at_ms,
                       a.seen_at_ms AS seen_at_ms,
                       a.snoozed_until_ms AS snoozed_until_ms
                FROM change_incidents i
                LEFT JOIN incident_attention a
                  ON a.incident_id = i.id AND a.episode_number = i.episode_number

                UNION ALL

                SELECT 'change' AS record_type, o.id AS id,
                       'change:' || o.id AS activity_key,
                       o.event_type AS event_type, 'info' AS severity,
                       o.station_id AS station_id, o.object_type AS object_type,
                       o.object_id AS object_id, o.station_key_id AS station_key_id,
                       o.source AS source, o.reason_code AS reason_code,
                       o.condition_key AS condition_key,
                       NULL AS lifecycle_state, NULL AS episode_number,
                       NULL AS occurrence_count, o.observed_at_ms AS activity_at_ms,
                       NULL AS last_observation_summary_json,
                       o.old_value_json AS old_value_json,
                       o.new_value_json AS new_value_json,
                       o.impact_json AS impact_json,
                       NULL AS resolved_at_ms, o.seen_at_ms AS seen_at_ms,
                       NULL AS snoozed_until_ms
                FROM change_event_occurrences o
                WHERE o.incident_id IS NULL AND o.category = 'audit_change'
            )
            SELECT record_type, id, activity_key, event_type, severity, station_id,
                   object_type, object_id, station_key_id, source, reason_code,
                   condition_key, lifecycle_state, episode_number, occurrence_count,
                   activity_at_ms, last_observation_summary_json, old_value_json,
                   new_value_json, impact_json, resolved_at_ms, seen_at_ms,
                   snoozed_until_ms
            FROM activity
            WHERE (?1 IS NULL OR station_id = ?1)
              AND (?2 IS NULL OR severity = ?2)
              AND (?3 IS NULL OR record_type = ?3)
              AND (?4 = 0 OR (seen_at_ms IS NULL AND (
                    record_type = 'change'
                    OR lifecycle_state IN ('pending', 'open', 'recovering')
                  )))
              AND (?5 IS NULL OR activity_at_ms < ?5
                   OR (activity_at_ms = ?5 AND activity_key < ?6))
            ORDER BY activity_at_ms DESC, activity_key DESC
            LIMIT ?7",
        )
        .bind(station_id)
        .bind(severity)
        .bind(record_type)
        .bind(unread_only)
        .bind(cursor.map(|value| value.0))
        .bind(cursor.map(|value| value.1))
        .bind(limit + 1)
        .fetch_all(session.connection())
        .await?;
        let rows = rows
            .into_iter()
            .map(row_to_activity)
            .collect::<Result<Vec<_>, _>>()?;
        let active_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM change_incidents
             WHERE lifecycle_state IN ('pending', 'open', 'recovering')
               AND (?1 IS NULL OR station_id = ?1)
               AND (?2 IS NULL OR severity = ?2)",
        )
        .bind(station_id)
        .bind(severity)
        .fetch_one(session.connection())
        .await?;
        let unseen_count = sqlx::query_scalar::<_, i64>(
            "SELECT
                (SELECT COUNT(*) FROM change_incidents i
                 LEFT JOIN incident_attention a
                   ON a.incident_id = i.id AND a.episode_number = i.episode_number
                 WHERE i.lifecycle_state IN ('pending', 'open', 'recovering')
                   AND i.severity IN ('warning', 'critical')
                   AND a.seen_at_ms IS NULL
                   AND (?1 IS NULL OR i.station_id = ?1)
                   AND (?2 IS NULL OR i.severity = ?2)
                   AND (?3 IS NULL OR ?3 = 'incident'))
                +
                (SELECT COUNT(*) FROM change_event_occurrences o
                 WHERE o.incident_id IS NULL AND o.category = 'audit_change'
                   AND o.seen_at_ms IS NULL
                   AND (?1 IS NULL OR o.station_id = ?1)
                   AND (?2 IS NULL OR ?2 = 'info')
                   AND (?3 IS NULL OR ?3 = 'change'))",
        )
        .bind(station_id)
        .bind(severity)
        .bind(record_type)
        .fetch_one(session.connection())
        .await?;
        Ok((rows, active_count, unseen_count))
    }

    pub(crate) async fn list_current(
        &self,
        session: &mut ReadSession,
        station_id: Option<&str>,
        severity: Option<&str>,
        lifecycle_state: Option<&str>,
        cursor: Option<(i64, &str)>,
        limit: u32,
    ) -> Result<(Vec<WorkspaceIncidentRow>, i64, i64), PersistenceError> {
        let limit = i64::from(limit.clamp(1, 200));
        let rows = sqlx::query(
            "SELECT i.id, i.condition_key, i.event_type, i.lifecycle_state, i.severity,
                    i.station_id, i.episode_number, i.occurrence_count, i.last_seen_at_ms,
                    i.last_observation_summary_json, i.resolved_at_ms, i.updated_at_ms,
                    a.seen_at_ms, a.snoozed_until_ms
             FROM change_incidents i
             LEFT JOIN incident_attention a
               ON a.incident_id = i.id AND a.episode_number = i.episode_number
             WHERE (?1 IS NULL OR i.station_id = ?1)
               AND (?2 IS NULL OR i.severity = ?2)
               AND (
                    ?3 IS NULL
                    OR (?3 = 'active' AND i.lifecycle_state IN ('pending', 'open', 'recovering'))
                    OR (?3 = 'unread' AND i.lifecycle_state IN ('pending', 'open', 'recovering') AND a.seen_at_ms IS NULL)
                    OR i.lifecycle_state = ?3
               )
               AND (?4 IS NULL OR i.updated_at_ms < ?4
                    OR (i.updated_at_ms = ?4 AND i.id < ?5))
             ORDER BY i.updated_at_ms DESC, i.id DESC
             LIMIT ?6",
        )
        .bind(station_id)
        .bind(severity)
        .bind(lifecycle_state)
        .bind(cursor.map(|value| value.0))
        .bind(cursor.map(|value| value.1))
        .bind(limit + 1)
        .fetch_all(session.connection())
        .await?;
        let rows = rows
            .into_iter()
            .map(row_to_incident)
            .collect::<Result<Vec<_>, _>>()?;
        let active_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM change_incidents
             WHERE lifecycle_state IN ('pending', 'open', 'recovering')
               AND (?1 IS NULL OR station_id = ?1)
               AND (?2 IS NULL OR severity = ?2)",
        )
        .bind(station_id)
        .bind(severity)
        .fetch_one(session.connection())
        .await?;
        let unseen_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM change_incidents i
             LEFT JOIN incident_attention a
               ON a.incident_id = i.id AND a.episode_number = i.episode_number
             WHERE i.lifecycle_state IN ('pending', 'open', 'recovering')
               AND i.severity IN ('warning', 'critical')
               AND a.seen_at_ms IS NULL
               AND (?1 IS NULL OR i.station_id = ?1)
               AND (?2 IS NULL OR i.severity = ?2)",
        )
        .bind(station_id)
        .bind(severity)
        .fetch_one(session.connection())
        .await?;
        Ok((rows, active_count, unseen_count))
    }

    pub(crate) async fn get_incident_detail(
        &self,
        session: &mut ReadSession,
        incident_id: &str,
        episode_number: i64,
    ) -> Result<Option<WorkspaceIncidentRow>, PersistenceError> {
        let row = sqlx::query(
            "SELECT i.id, i.condition_key, i.event_type, i.lifecycle_state, i.severity,
                    i.station_id, i.episode_number, i.occurrence_count, i.last_seen_at_ms,
                    i.last_observation_summary_json, i.resolved_at_ms, i.updated_at_ms,
                    a.seen_at_ms, a.snoozed_until_ms
             FROM change_incidents i
             LEFT JOIN incident_attention a
               ON a.incident_id = i.id AND a.episode_number = i.episode_number
             WHERE i.id = ?1 AND i.episode_number = ?2",
        )
        .bind(incident_id)
        .bind(episode_number)
        .fetch_optional(session.connection())
        .await?;
        row.map(row_to_incident).transpose()
    }

    pub(crate) async fn list_occurrences(
        &self,
        session: &mut ReadSession,
        incident_id: &str,
        episode_number: i64,
        cursor: Option<(i64, &str)>,
        limit: u32,
    ) -> Result<Vec<WorkspaceOccurrenceRow>, PersistenceError> {
        let limit = i64::from(limit.clamp(1, 200));
        let rows = sqlx::query(
            "SELECT id, source_observation_key, event_type, observation_kind, severity,
                    reason_code, source, object_type, object_id, station_id, station_key_id,
                    observed_at_ms
             FROM change_event_occurrences
             WHERE incident_id = ?1 AND episode_number = ?2
               AND (?3 IS NULL OR observed_at_ms < ?3
                    OR (observed_at_ms = ?3 AND id < ?4))
             ORDER BY observed_at_ms DESC, id DESC LIMIT ?5",
        )
        .bind(incident_id)
        .bind(episode_number)
        .bind(cursor.map(|value| value.0))
        .bind(cursor.map(|value| value.1))
        .bind(limit + 1)
        .fetch_all(session.connection())
        .await?;
        rows.into_iter().map(row_to_occurrence).collect()
    }

    pub(crate) async fn list_deliveries(
        &self,
        session: &mut ReadSession,
        incident_id: &str,
        episode_number: i64,
        cursor: Option<(i64, &str)>,
        limit: u32,
    ) -> Result<Vec<WorkspaceDeliveryRow>, PersistenceError> {
        let limit = i64::from(limit.clamp(1, 200));
        let rows = sqlx::query(
            "SELECT id, delivery_key, channel, delivery_kind, status, scheduled_at_ms,
                    attempt_count, delivered_at_ms, suppressed_reason, error_code,
                    created_at_ms, updated_at_ms
             FROM notification_deliveries
             WHERE incident_id = ?1 AND episode_number = ?2
               AND (?3 IS NULL OR created_at_ms < ?3
                    OR (created_at_ms = ?3 AND id < ?4))
             ORDER BY created_at_ms DESC, id DESC LIMIT ?5",
        )
        .bind(incident_id)
        .bind(episode_number)
        .bind(cursor.map(|value| value.0))
        .bind(cursor.map(|value| value.1))
        .bind(limit + 1)
        .fetch_all(session.connection())
        .await?;
        rows.into_iter().map(row_to_delivery).collect()
    }
}

fn row_to_activity(row: sqlx::sqlite::SqliteRow) -> Result<WorkspaceActivityRow, PersistenceError> {
    Ok(WorkspaceActivityRow {
        record_type: row.try_get("record_type")?,
        id: row.try_get("id")?,
        event_type: row.try_get("event_type")?,
        severity: row.try_get("severity")?,
        station_id: row.try_get("station_id")?,
        object_type: row.try_get("object_type")?,
        object_id: row.try_get("object_id")?,
        station_key_id: row.try_get("station_key_id")?,
        source: row.try_get("source")?,
        reason_code: row.try_get("reason_code")?,
        condition_key: row.try_get("condition_key")?,
        lifecycle_state: row.try_get("lifecycle_state")?,
        episode_number: row.try_get("episode_number")?,
        occurrence_count: row.try_get("occurrence_count")?,
        activity_at_ms: row.try_get("activity_at_ms")?,
        last_observation_summary_json: row.try_get("last_observation_summary_json")?,
        old_value_json: row.try_get("old_value_json")?,
        new_value_json: row.try_get("new_value_json")?,
        impact_json: row.try_get("impact_json")?,
        resolved_at_ms: row.try_get("resolved_at_ms")?,
        seen_at_ms: row.try_get("seen_at_ms")?,
        snoozed_until_ms: row.try_get("snoozed_until_ms")?,
    })
}

fn row_to_incident(row: sqlx::sqlite::SqliteRow) -> Result<WorkspaceIncidentRow, PersistenceError> {
    Ok(WorkspaceIncidentRow {
        id: row.try_get("id")?,
        condition_key: row.try_get("condition_key")?,
        event_type: row.try_get("event_type")?,
        lifecycle_state: row.try_get("lifecycle_state")?,
        severity: row.try_get("severity")?,
        station_id: row.try_get("station_id")?,
        episode_number: row.try_get("episode_number")?,
        occurrence_count: row.try_get("occurrence_count")?,
        last_seen_at_ms: row.try_get("last_seen_at_ms")?,
        last_observation_summary_json: row.try_get("last_observation_summary_json")?,
        resolved_at_ms: row.try_get("resolved_at_ms")?,
        updated_at_ms: row.try_get("updated_at_ms")?,
        seen_at_ms: row.try_get("seen_at_ms")?,
        snoozed_until_ms: row.try_get("snoozed_until_ms")?,
    })
}

fn row_to_occurrence(
    row: sqlx::sqlite::SqliteRow,
) -> Result<WorkspaceOccurrenceRow, PersistenceError> {
    Ok(WorkspaceOccurrenceRow {
        id: row.try_get("id")?,
        source_observation_key: row.try_get("source_observation_key")?,
        event_type: row.try_get("event_type")?,
        observation_kind: row.try_get("observation_kind")?,
        severity: row.try_get("severity")?,
        reason_code: row.try_get("reason_code")?,
        source: row.try_get("source")?,
        object_type: row.try_get("object_type")?,
        object_id: row.try_get("object_id")?,
        station_id: row.try_get("station_id")?,
        station_key_id: row.try_get("station_key_id")?,
        observed_at_ms: row.try_get("observed_at_ms")?,
    })
}

fn row_to_delivery(row: sqlx::sqlite::SqliteRow) -> Result<WorkspaceDeliveryRow, PersistenceError> {
    Ok(WorkspaceDeliveryRow {
        id: row.try_get("id")?,
        delivery_key: row.try_get("delivery_key")?,
        channel: row.try_get("channel")?,
        delivery_kind: row.try_get("delivery_kind")?,
        status: row.try_get("status")?,
        scheduled_at_ms: row.try_get("scheduled_at_ms")?,
        attempt_count: row.try_get("attempt_count")?,
        delivered_at_ms: row.try_get("delivered_at_ms")?,
        suppressed_reason: row.try_get("suppressed_reason")?,
        error_code: row.try_get("error_code")?,
        created_at_ms: row.try_get("created_at_ms")?,
        updated_at_ms: row.try_get("updated_at_ms")?,
    })
}
