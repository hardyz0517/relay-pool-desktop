use sqlx::Row;

use crate::{
    models::change_events::ChangeEvent,
    persistence::{
        error::PersistenceError, read_session::ReadSession, write_session::WriteSession,
    },
};

#[derive(Debug, Clone)]
pub(crate) struct NewChangeEvent {
    pub id: String,
    pub severity: String,
    pub event_type: String,
    pub title: String,
    pub message: String,
    pub object_type: String,
    pub object_id: Option<String>,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub pricing_rule_id: Option<String>,
    pub request_log_id: Option<String>,
    pub old_value_json: Option<String>,
    pub new_value_json: Option<String>,
    pub impact_json: Option<String>,
    pub dedupe_key: String,
    pub source: String,
    pub now: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangeWriteResult {
    pub id: String,
    pub inserted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangeCursor {
    pub updated_at: String,
    pub id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ChangeEventPage {
    pub items: Vec<ChangeEvent>,
    pub next_cursor: Option<ChangeCursor>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ChangeStore;

impl ChangeStore {
    pub(crate) async fn upsert(
        &self,
        session: &mut WriteSession,
        event: &NewChangeEvent,
    ) -> Result<ChangeWriteResult, PersistenceError> {
        let inserted = sqlx::query(
            "INSERT INTO change_events (
                id, severity, event_type, status, title, message, object_type, object_id,
                station_id, station_key_id, pricing_rule_id, request_log_id, old_value_json,
                new_value_json, impact_json, dedupe_key, source, detected_at, created_at, updated_at
             ) VALUES (?1, ?2, ?3, 'unread', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                       ?13, ?14, ?15, ?16, ?17, ?17, ?17)
             ON CONFLICT(dedupe_key) DO UPDATE SET
                severity = excluded.severity,
                event_type = excluded.event_type,
                status = CASE
                    WHEN change_events.status IN ('read', 'dismissed') THEN change_events.status
                    ELSE 'unread' END,
                title = excluded.title,
                message = excluded.message,
                object_type = excluded.object_type,
                object_id = excluded.object_id,
                station_id = excluded.station_id,
                station_key_id = excluded.station_key_id,
                pricing_rule_id = excluded.pricing_rule_id,
                request_log_id = excluded.request_log_id,
                old_value_json = excluded.old_value_json,
                new_value_json = excluded.new_value_json,
                impact_json = excluded.impact_json,
                source = excluded.source,
                detected_at = excluded.detected_at,
                resolved_at = NULL,
                updated_at = excluded.updated_at",
        )
        .bind(&event.id)
        .bind(&event.severity)
        .bind(&event.event_type)
        .bind(&event.title)
        .bind(&event.message)
        .bind(&event.object_type)
        .bind(&event.object_id)
        .bind(&event.station_id)
        .bind(&event.station_key_id)
        .bind(&event.pricing_rule_id)
        .bind(&event.request_log_id)
        .bind(&event.old_value_json)
        .bind(&event.new_value_json)
        .bind(&event.impact_json)
        .bind(&event.dedupe_key)
        .bind(&event.source)
        .bind(&event.now)
        .execute(session.connection())
        .await?
        .rows_affected();
        let id =
            sqlx::query_scalar::<_, String>("SELECT id FROM change_events WHERE dedupe_key = ?1")
                .bind(&event.dedupe_key)
                .fetch_one(session.connection())
                .await?;
        let was_inserted = inserted == 1 && id == event.id;
        Ok(ChangeWriteResult {
            id,
            inserted: was_inserted,
        })
    }

    pub(crate) async fn resolve_by_dedupe_key(
        &self,
        session: &mut WriteSession,
        dedupe_key: &str,
        now: &str,
    ) -> Result<bool, PersistenceError> {
        let affected = sqlx::query(
            "UPDATE change_events SET status = 'resolved', resolved_at = ?1, updated_at = ?1
             WHERE dedupe_key = ?2 AND status NOT IN ('dismissed', 'resolved')",
        )
        .bind(now)
        .bind(dedupe_key)
        .execute(session.connection())
        .await?
        .rows_affected();
        Ok(affected > 0)
    }

    pub(crate) async fn list_page(
        &self,
        session: &mut ReadSession,
        station_id: Option<&str>,
        cursor: Option<&ChangeCursor>,
        limit: u32,
    ) -> Result<ChangeEventPage, PersistenceError> {
        let limit = limit.clamp(1, 200) as usize;
        let cursor_updated_at = cursor.map(|cursor| cursor.updated_at.as_str());
        let cursor_id = cursor.map(|cursor| cursor.id.as_str());
        let rows = sqlx::query(
            "SELECT e.id, e.severity, e.event_type, e.status, e.title, e.message,
                    e.object_type, e.object_id, e.station_id, s.name AS station_name,
                    e.station_key_id, e.pricing_rule_id, e.request_log_id,
                    e.old_value_json, e.new_value_json, e.impact_json, e.dedupe_key,
                    e.source, e.detected_at, e.resolved_at, e.created_at, e.updated_at
             FROM change_events e
             LEFT JOIN stations s ON s.id = e.station_id
             WHERE (?1 IS NULL OR e.station_id = ?1)
               AND (?2 IS NULL OR e.updated_at < ?2 OR (e.updated_at = ?2 AND e.id < ?3))
             ORDER BY e.updated_at DESC, e.id DESC
             LIMIT ?4",
        )
        .bind(station_id)
        .bind(cursor_updated_at)
        .bind(cursor_id)
        .bind((limit + 1) as i64)
        .fetch_all(session.connection())
        .await?;
        let has_more = rows.len() > limit;
        let mut items = rows.iter().map(row_to_change_event).collect::<Vec<_>>();
        items.truncate(limit);
        let next_cursor = has_more.then(|| {
            let event = items.last().expect("non-empty page with overflow");
            ChangeCursor {
                updated_at: event.updated_at.clone(),
                id: event.id.clone(),
            }
        });
        Ok(ChangeEventPage { items, next_cursor })
    }
}

fn row_to_change_event(row: &sqlx::sqlite::SqliteRow) -> ChangeEvent {
    ChangeEvent {
        id: row.get("id"),
        severity: row.get("severity"),
        event_type: row.get("event_type"),
        status: row.get("status"),
        title: row.get("title"),
        message: row.get("message"),
        object_type: row.get("object_type"),
        object_id: row.get("object_id"),
        station_id: row.get("station_id"),
        station_name: row.get("station_name"),
        station_key_id: row.get("station_key_id"),
        pricing_rule_id: row.get("pricing_rule_id"),
        request_log_id: row.get("request_log_id"),
        old_value_json: row.get("old_value_json"),
        new_value_json: row.get("new_value_json"),
        impact_json: row.get("impact_json"),
        dedupe_key: row.get("dedupe_key"),
        source: row.get("source"),
        detected_at: row.get("detected_at"),
        resolved_at: row.get("resolved_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
