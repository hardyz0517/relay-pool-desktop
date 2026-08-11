use crate::persistence::{
    error::PersistenceError, runtime::PersistenceHandle,
    stores::alerting::workspace::WorkspaceStore,
};
use crate::{
    application::alerting::{AlertingIngress, ObservationIngress},
    models::alerting::{AlertEventType, ConditionKey, ObservationKind, Severity},
    persistence::stores::alerting::{IncidentStore, LegacyCollectorFailureGroup},
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
    pub collector_failed_task_types: Vec<String>,
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
        let alerting = AlertingIngress::new(self.runtime.clone());
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    incident_store
                        .resolve_orphaned_station_incidents(write, now_ms)
                        .await?;
                    for group in incident_store
                        .legacy_collector_failure_groups(write)
                        .await?
                    {
                        alerting
                            .record_in_session(
                                write,
                                legacy_collector_failure_observation(&group, now_ms)?,
                            )
                            .await?;
                        incident_store
                            .resolve_legacy_collector_child_incidents(
                                write,
                                &group.station_id,
                                now_ms,
                            )
                            .await?;
                    }
                    Ok(())
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
    let collector_failed_task_types = collector_failed_task_types_from_summary(
        &row.event_type,
        &row.condition_key,
        &row.last_observation_summary_json,
    );
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
        collector_failed_task_types,
        resolved_at_ms: row.resolved_at_ms,
        updated_at_ms: row.updated_at_ms,
        seen_at_ms: row.seen_at_ms,
        snoozed_until_ms: row.snoozed_until_ms,
    }
}

fn collector_failed_task_types_from_summary(
    event_type: &str,
    condition_key: &str,
    summary_json: &str,
) -> Vec<String> {
    if event_type != AlertEventType::CollectorFailed.as_str() {
        return Vec::new();
    }

    let summary = serde_json::from_str::<serde_json::Value>(summary_json).ok();
    let reported = summary
        .as_ref()
        .and_then(|value| value.get("failedTaskTypes"))
        .and_then(serde_json::Value::as_array);
    let mut failed = reported
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect::<std::collections::BTreeSet<_>>();

    if failed.is_empty() {
        if let Some((_, task_type)) = condition_key.rsplit_once(":collector_failed:") {
            failed.insert(task_type);
        }
    }

    ["balance", "groups", "detect", "full"]
        .into_iter()
        .filter(|task_type| failed.contains(task_type))
        .map(str::to_string)
        .collect()
}

fn legacy_collector_failure_observation(
    group: &LegacyCollectorFailureGroup,
    now_ms: i64,
) -> Result<ObservationIngress, PersistenceError> {
    let condition_key =
        ConditionKey::new(format!("collector:{}:collector_failed", group.station_id))
            .map_err(PersistenceError::InvariantViolation)?;
    Ok(ObservationIngress {
        source_observation_key: format!(
            "collector:legacy_merge:{}:{}",
            group.station_id, group.last_seen_at_ms
        ),
        event_type: AlertEventType::CollectorFailed,
        condition_key,
        kind: ObservationKind::Abnormal,
        severity: Severity::Warning,
        object_type: "station".to_string(),
        object_id: Some(group.station_id.clone()),
        station_id: Some(group.station_id.clone()),
        station_key_id: None,
        source: "collector".to_string(),
        reason_code: Some("legacy_collector_failures_merged".to_string()),
        summary_json: serde_json::json!({
            "status": "failed",
            "failedTaskTypes": group.failed_task_types,
            "legacyMerged": true,
        })
        .to_string(),
        observed_at_ms: now_ms,
        fact_fresh_until_ms: now_ms.saturating_add(900_000),
    })
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{collector_failed_task_types_from_summary, ChangeCenterWorkspaceQuery};
    use crate::{
        application::{clock::SystemClock, ids::UuidV7Generator, stations::StationService},
        models::stations::CreateStationInput,
        persistence::{error::PersistenceError, runtime::PersistenceRuntime},
    };

    #[tokio::test]
    async fn listing_active_alerts_merges_legacy_collector_failures_by_station() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &temp.path().join("legacy-collector-failures.sqlite3"),
        )
        .await
        .expect("runtime");
        let station = StationService::new(
            runtime.handle(),
            Arc::new(SystemClock),
            Arc::new(UuidV7Generator),
        )
        .create(CreateStationInput {
            name: "Legacy collector failure fixture".to_string(),
            station_type: "sub2api".to_string(),
            website_url: "https://legacy-collector.example".to_string(),
            api_base_url: "https://legacy-collector.example/v1".to_string(),
            api_key: String::new(),
            collector_proxy_mode: "inherit".to_string(),
            collector_proxy_url: None,
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: None,
            collection_interval_minutes: 30,
            note: None,
        })
        .await
        .expect("station");
        let station_id = station.id.clone();

        runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    for (index, task_type) in ["full", "balance", "groups"].into_iter().enumerate()
                    {
                        let timestamp = 100 + index as i64;
                        sqlx::query(
                            "INSERT INTO change_incidents (
                                id, condition_key, event_type, lifecycle_state,
                                base_severity, severity, object_type, object_id, station_id,
                                lifecycle_policy_fingerprint, episode_number, first_seen_at_ms,
                                last_seen_at_ms, occurrence_count, episode_occurrence_count,
                                last_observation_summary_json, created_at_ms, updated_at_ms
                             ) VALUES (?1, ?2, 'collector_failed', 'open', 'warning', 'warning',
                                       'station', ?3, ?3, 'legacy-fixture', 1, ?4, ?4, 1, 1,
                                       '{}', ?4, ?4)",
                        )
                        .bind(format!("legacy-collector-{task_type}"))
                        .bind(format!(
                            "collector:{station_id}:collector_failed:{task_type}"
                        ))
                        .bind(&station_id)
                        .bind(timestamp)
                        .execute(write.connection())
                        .await?;
                    }
                    Ok::<(), PersistenceError>(())
                })
            })
            .await
            .expect("legacy incidents");

        let page = ChangeCenterWorkspaceQuery::new(runtime.handle())
            .list_current(None, None, Some("active"), None, 100)
            .await
            .expect("list current alerts");

        assert_eq!(page.active_count, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items[0].condition_key,
            format!("collector:{}:collector_failed", station.id)
        );
        assert_eq!(
            page.items[0].collector_failed_task_types,
            ["balance", "groups"]
        );

        let mut read = runtime.handle().begin_read().await.expect("read session");
        let legacy_active_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM change_incidents
             WHERE condition_key LIKE ?1 AND lifecycle_state != 'resolved'",
        )
        .bind(format!("collector:{}:collector_failed:%", station.id))
        .fetch_one(read.connection())
        .await
        .expect("legacy incident lifecycle");
        assert_eq!(legacy_active_count, 0);

        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[test]
    fn collector_failed_task_types_are_ordered_validated_and_legacy_compatible() {
        assert_eq!(
            collector_failed_task_types_from_summary(
                "collector_failed",
                "collector:station-1:collector_failed",
                r#"{"failedTaskTypes":["groups","unknown","balance","groups"]}"#,
            ),
            ["balance", "groups"]
        );
        assert_eq!(
            collector_failed_task_types_from_summary(
                "collector_failed",
                "collector:station-1:collector_failed:groups",
                "not-json",
            ),
            ["groups"]
        );
        assert!(collector_failed_task_types_from_summary(
            "rate_changed",
            "collector:station-1:collector_failed:groups",
            r#"{"failedTaskTypes":["groups"]}"#,
        )
        .is_empty());
    }
}
