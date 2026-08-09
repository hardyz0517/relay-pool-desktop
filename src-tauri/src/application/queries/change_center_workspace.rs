use crate::persistence::stores::alerting::IncidentStore;
use crate::persistence::{
    error::PersistenceError, runtime::PersistenceHandle,
    stores::alerting::workspace::WorkspaceStore,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IncidentCursor {
    pub updated_at_ms: i64,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IncidentSummary {
    pub id: String,
    pub condition_key: String,
    pub event_type: String,
    pub lifecycle_state: String,
    pub severity: String,
    pub station_id: Option<String>,
    pub episode_number: i64,
    pub occurrence_count: i64,
    pub last_seen_at_ms: i64,
    pub resolved_at_ms: Option<i64>,
    pub updated_at_ms: i64,
    pub seen_at_ms: Option<i64>,
    pub snoozed_until_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub(crate) struct IncidentWorkspacePage {
    pub items: Vec<IncidentSummary>,
    pub next_cursor: Option<IncidentCursor>,
    pub active_count: i64,
    pub unseen_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OccurrenceCursor {
    pub observed_at_ms: i64,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OccurrenceSummary {
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
pub(crate) struct OccurrenceHistoryPage {
    pub items: Vec<OccurrenceSummary>,
    pub next_cursor: Option<OccurrenceCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeliveryCursor {
    pub created_at_ms: i64,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeliverySummary {
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

#[derive(Debug, Clone)]
pub(crate) struct DeliveryHistoryPage {
    pub items: Vec<DeliverySummary>,
    pub next_cursor: Option<DeliveryCursor>,
}

#[derive(Clone)]
pub(crate) struct ChangeCenterWorkspaceQuery {
    runtime: PersistenceHandle,
}

impl ChangeCenterWorkspaceQuery {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self { runtime }
    }

    pub(crate) async fn list_current(
        &self,
        station_id: Option<&str>,
        severity: Option<&str>,
        lifecycle_state: Option<&str>,
        cursor: Option<&IncidentCursor>,
        limit: u32,
    ) -> Result<IncidentWorkspacePage, PersistenceError> {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| PersistenceError::ConstraintViolation)?
            .as_millis()
            .try_into()
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        let incident_store = IncidentStore;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    incident_store
                        .resolve_orphaned_station_incidents(write, now_ms)
                        .await
                        .map(|_| ())
                })
            })
            .await?;
        let mut read = self.runtime.begin_read().await?;
        let (rows, active_count, unseen_count) = WorkspaceStore
            .list_current(
                &mut read,
                station_id,
                severity,
                lifecycle_state,
                cursor.map(|value| (value.updated_at_ms, value.id.as_str())),
                limit,
            )
            .await?;
        let page_limit = limit.clamp(1, 200) as usize;
        let has_more = rows.len() > page_limit;
        let items = rows
            .into_iter()
            .take(page_limit)
            .map(incident_summary_from_row)
            .collect::<Vec<_>>();
        let next_cursor = has_more.then(|| {
            let last = items.last().expect("overflow page must contain an item");
            IncidentCursor {
                updated_at_ms: last.updated_at_ms,
                id: last.id.clone(),
            }
        });
        Ok(IncidentWorkspacePage {
            items,
            next_cursor,
            active_count,
            unseen_count,
        })
    }

    pub(crate) async fn get_incident_detail(
        &self,
        incident_id: &str,
        episode_number: i64,
    ) -> Result<Option<IncidentSummary>, PersistenceError> {
        let mut read = self.runtime.begin_read().await?;
        Ok(WorkspaceStore
            .get_incident_detail(&mut read, incident_id, episode_number)
            .await?
            .map(incident_summary_from_row))
    }

    pub(crate) async fn list_occurrences(
        &self,
        incident_id: &str,
        episode_number: i64,
        cursor: Option<&OccurrenceCursor>,
        limit: u32,
    ) -> Result<OccurrenceHistoryPage, PersistenceError> {
        let mut read = self.runtime.begin_read().await?;
        let limit = limit.clamp(1, 200);
        let rows = WorkspaceStore
            .list_occurrences(
                &mut read,
                incident_id,
                episode_number,
                cursor.map(|value| (value.observed_at_ms, value.id.as_str())),
                limit,
            )
            .await?;
        let has_more = rows.len() > limit as usize;
        let items = rows
            .into_iter()
            .take(limit as usize)
            .map(occurrence_summary_from_row)
            .collect::<Vec<_>>();
        let next_cursor = has_more.then(|| {
            let last = items.last().expect("overflow page must contain an item");
            OccurrenceCursor {
                observed_at_ms: last.observed_at_ms,
                id: last.id.clone(),
            }
        });
        Ok(OccurrenceHistoryPage { items, next_cursor })
    }

    pub(crate) async fn list_deliveries(
        &self,
        incident_id: &str,
        episode_number: i64,
        cursor: Option<&DeliveryCursor>,
        limit: u32,
    ) -> Result<DeliveryHistoryPage, PersistenceError> {
        let mut read = self.runtime.begin_read().await?;
        let limit = limit.clamp(1, 200);
        let rows = WorkspaceStore
            .list_deliveries(
                &mut read,
                incident_id,
                episode_number,
                cursor.map(|value| (value.created_at_ms, value.id.as_str())),
                limit,
            )
            .await?;
        let has_more = rows.len() > limit as usize;
        let items = rows
            .into_iter()
            .take(limit as usize)
            .map(delivery_summary_from_row)
            .collect::<Vec<_>>();
        let next_cursor = has_more.then(|| {
            let last = items.last().expect("overflow page must contain an item");
            DeliveryCursor {
                created_at_ms: last.created_at_ms,
                id: last.id.clone(),
            }
        });
        Ok(DeliveryHistoryPage { items, next_cursor })
    }
}

fn incident_summary_from_row(
    row: crate::persistence::stores::alerting::workspace::WorkspaceIncidentRow,
) -> IncidentSummary {
    IncidentSummary {
        id: row.id,
        condition_key: row.condition_key,
        event_type: row.event_type,
        lifecycle_state: row.lifecycle_state,
        severity: row.severity,
        station_id: row.station_id,
        episode_number: row.episode_number,
        occurrence_count: row.occurrence_count,
        last_seen_at_ms: row.last_seen_at_ms,
        resolved_at_ms: row.resolved_at_ms,
        updated_at_ms: row.updated_at_ms,
        seen_at_ms: row.seen_at_ms,
        snoozed_until_ms: row.snoozed_until_ms,
    }
}

fn occurrence_summary_from_row(
    row: crate::persistence::stores::alerting::workspace::WorkspaceOccurrenceRow,
) -> OccurrenceSummary {
    OccurrenceSummary {
        id: row.id,
        source_observation_key: row.source_observation_key,
        event_type: row.event_type,
        observation_kind: row.observation_kind,
        severity: row.severity,
        reason_code: row.reason_code,
        source: row.source,
        object_type: row.object_type,
        object_id: row.object_id,
        station_id: row.station_id,
        station_key_id: row.station_key_id,
        observed_at_ms: row.observed_at_ms,
    }
}

fn delivery_summary_from_row(
    row: crate::persistence::stores::alerting::workspace::WorkspaceDeliveryRow,
) -> DeliverySummary {
    DeliverySummary {
        id: row.id,
        delivery_key: row.delivery_key,
        channel: row.channel,
        delivery_kind: row.delivery_kind,
        status: row.status,
        scheduled_at_ms: row.scheduled_at_ms,
        attempt_count: row.attempt_count,
        delivered_at_ms: row.delivered_at_ms,
        suppressed_reason: row.suppressed_reason,
        error_code: row.error_code,
        created_at_ms: row.created_at_ms,
        updated_at_ms: row.updated_at_ms,
    }
}
