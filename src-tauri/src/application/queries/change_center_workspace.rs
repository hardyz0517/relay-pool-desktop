use crate::persistence::{
    error::PersistenceError, runtime::PersistenceHandle,
    stores::alerting::workspace::WorkspaceStore,
};
use crate::{
    application::alerting::{
        AlertingIngress, AlertingReadModelUpdatePublisher, NoopAlertingReadModelUpdatePublisher,
        ObservationIngress,
    },
    models::alerting::{AlertEventType, ConditionKey, ObservationKind, Severity},
    persistence::stores::alerting::{IncidentStore, LegacyCollectorFailureGroup},
};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

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
    pub group_name: Option<String>,
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
    pub total_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActivityCursor {
    pub activity_at_ms: i64,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActivitySummary {
    pub record_type: String,
    pub id: String,
    pub event_type: String,
    pub severity: String,
    pub group_name: Option<String>,
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
    pub old_value_json: Option<String>,
    pub new_value_json: Option<String>,
    pub impact_json: Option<String>,
    pub collector_failed_task_types: Vec<String>,
    pub resolved_at_ms: Option<i64>,
    pub seen_at_ms: Option<i64>,
    pub snoozed_until_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub(crate) struct ActivityWorkspacePage {
    pub items: Vec<ActivitySummary>,
    pub next_cursor: Option<ActivityCursor>,
    pub active_count: i64,
    pub unseen_count: i64,
    pub total_count: i64,
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
    alerting_updates: Arc<dyn AlertingReadModelUpdatePublisher>,
}

impl ChangeCenterWorkspaceQuery {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=alerting.read-model-update-test-constructor; owner=application/queries/change_center_workspace; remove_when=all non-desktop compositions inject a read-model update publisher"
        )
    )]
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self::new_with_alerting_read_model_updates(
            runtime,
            Arc::new(NoopAlertingReadModelUpdatePublisher),
        )
    }

    pub(crate) fn new_with_alerting_read_model_updates(
        runtime: PersistenceHandle,
        alerting_updates: Arc<dyn AlertingReadModelUpdatePublisher>,
    ) -> Self {
        Self {
            runtime,
            alerting_updates,
        }
    }

    pub(crate) async fn list_current(
        &self,
        station_id: Option<&str>,
        severity: Option<&str>,
        lifecycle_state: Option<&str>,
        search: Option<&str>,
        cursor: Option<&IncidentCursor>,
        limit: u32,
    ) -> Result<IncidentWorkspacePage, PersistenceError> {
        self.prepare_workspace().await?;
        let mut read = self.runtime.begin_read().await?;
        let (rows, active_count, unseen_count) = WorkspaceStore
            .list_current(
                &mut read,
                station_id,
                severity,
                lifecycle_state,
                search,
                cursor.map(|value| (value.updated_at_ms, value.id.as_str())),
                limit,
            )
            .await?;
        let page_limit = limit.clamp(1, 200) as usize;
        let total_count = rows.first().map(|row| row.total_count).unwrap_or(0);
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
            total_count,
        })
    }

    pub(crate) async fn list_activity(
        &self,
        station_id: Option<&str>,
        severity: Option<&str>,
        record_type: Option<&str>,
        unread_only: bool,
        search: Option<&str>,
        cursor: Option<&ActivityCursor>,
        limit: u32,
    ) -> Result<ActivityWorkspacePage, PersistenceError> {
        self.prepare_workspace().await?;
        let mut read = self.runtime.begin_read().await?;
        let (rows, active_count, unseen_count) = WorkspaceStore
            .list_activity(
                &mut read,
                station_id,
                severity,
                record_type,
                unread_only,
                search,
                cursor.map(|value| (value.activity_at_ms, value.id.as_str())),
                limit,
            )
            .await?;
        let page_limit = limit.clamp(1, 200) as usize;
        let total_count = rows.first().map(|row| row.total_count).unwrap_or(0);
        let has_more = rows.len() > page_limit;
        let items = rows
            .into_iter()
            .take(page_limit)
            .map(activity_summary_from_row)
            .collect::<Vec<_>>();
        let next_cursor = has_more.then(|| {
            let last = items.last().expect("overflow page must contain an item");
            ActivityCursor {
                activity_at_ms: last.activity_at_ms,
                id: format!("{}:{}", last.record_type, last.id),
            }
        });
        Ok(ActivityWorkspacePage {
            items,
            next_cursor,
            active_count,
            unseen_count,
            total_count,
        })
    }

    async fn prepare_workspace(&self) -> Result<(), PersistenceError> {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| PersistenceError::ConstraintViolation)?
            .as_millis()
            .try_into()
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        let incident_store = IncidentStore;
        let alerting = AlertingIngress::new(self.runtime.clone());
        let changes = self
            .runtime
            .write(|write| {
                Box::pin(async move {
                    let mut changes = incident_store
                        .resolve_orphaned_station_incidents(write, now_ms)
                        .await?;
                    for group in incident_store
                        .legacy_collector_failure_groups(write)
                        .await?
                    {
                        changes += u64::from(
                            alerting
                                .record_in_session(
                                    write,
                                    legacy_collector_failure_observation(&group, now_ms)?,
                                )
                                .await?
                                .inserted,
                        );
                        changes += incident_store
                            .resolve_legacy_collector_child_incidents(
                                write,
                                &group.station_id,
                                now_ms,
                            )
                            .await?;
                    }
                    Ok(changes)
                })
            })
            .await?;
        if changes > 0 {
            self.alerting_updates.notify_after_commit();
        }
        Ok(())
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
    let group_name = group_name_from_summary(&row.last_observation_summary_json);
    IncidentSummary {
        id: row.id,
        condition_key: row.condition_key,
        event_type: row.event_type,
        lifecycle_state: row.lifecycle_state,
        severity: row.severity,
        group_name,
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

fn group_name_from_summary(summary_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(summary_json)
        .ok()
        .and_then(|value| {
            value
                .get("groupName")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value.len() <= 160)
}

fn activity_summary_from_row(
    row: crate::persistence::stores::alerting::workspace::WorkspaceActivityRow,
) -> ActivitySummary {
    let collector_failed_task_types = collector_failed_task_types_from_summary(
        &row.event_type,
        row.condition_key.as_deref().unwrap_or_default(),
        row.last_observation_summary_json.as_deref().unwrap_or("{}"),
    );
    let old_value_json = sanitized_activity_json(&row.event_type, row.old_value_json.as_deref());
    let new_value_json = sanitized_activity_json(
        &row.event_type,
        activity_json_with_group_rate(
            &row.event_type,
            row.new_value_json.as_deref(),
            row.group_effective_rate_multiplier,
        )
        .as_deref(),
    );
    let impact_json = sanitized_activity_json(&row.event_type, row.impact_json.as_deref());
    let group_name = row
        .last_observation_summary_json
        .as_deref()
        .and_then(group_name_from_summary)
        .or_else(|| {
            row.new_value_json
                .as_deref()
                .and_then(group_name_from_summary)
        });
    ActivitySummary {
        record_type: row.record_type,
        id: row.id,
        event_type: row.event_type,
        severity: row.severity,
        group_name,
        station_id: row.station_id,
        object_type: row.object_type,
        object_id: row.object_id,
        station_key_id: row.station_key_id,
        source: row.source,
        reason_code: row.reason_code,
        condition_key: row.condition_key,
        lifecycle_state: row.lifecycle_state,
        episode_number: row.episode_number,
        occurrence_count: row.occurrence_count,
        activity_at_ms: row.activity_at_ms,
        old_value_json,
        new_value_json,
        impact_json,
        collector_failed_task_types,
        resolved_at_ms: row.resolved_at_ms,
        seen_at_ms: row.seen_at_ms,
        snoozed_until_ms: row.snoozed_until_ms,
    }
}

fn sanitized_activity_json(event_type: &str, encoded: Option<&str>) -> Option<String> {
    const SAFE_KEYS: &[&str] = &[
        "groupName",
        "status",
        "groupKeyHash",
        "effectiveRateMultiplier",
        "oldEffectiveRateMultiplier",
        "newEffectiveRateMultiplier",
        "model",
        "modelName",
        "modelId",
        "price",
        "oldPrice",
        "newPrice",
        "rate",
        "oldRate",
        "newRate",
        "currency",
        "affectedKeyCount",
        "affectedRouteCount",
    ];
    let value = serde_json::from_str::<serde_json::Value>(encoded?).ok()?;
    let sanitized = match value {
        serde_json::Value::Object(values) => {
            let values = values
                .into_iter()
                .filter(|(key, _)| SAFE_KEYS.contains(&key.as_str()))
                .filter_map(|(key, value)| {
                    sanitized_activity_scalar(value).map(|value| (key, value))
                })
                .collect::<serde_json::Map<_, _>>();
            (!values.is_empty()).then_some(serde_json::Value::Object(values))?
        }
        value
            if matches!(
                event_type,
                "group_rate_changed"
                    | "rate_changed"
                    | "price_changed"
                    | "model_added"
                    | "model_removed"
            ) =>
        {
            sanitized_activity_scalar(value)?
        }
        _ => return None,
    };
    serde_json::to_string(&sanitized).ok()
}

fn activity_json_with_group_rate(
    event_type: &str,
    encoded: Option<&str>,
    effective_rate_multiplier: Option<f64>,
) -> Option<String> {
    if event_type != "group_added" || effective_rate_multiplier.is_none() {
        return encoded.map(str::to_string);
    }
    let mut values = match encoded.and_then(|value| serde_json::from_str(value).ok()) {
        Some(serde_json::Value::Object(values)) => values,
        _ => serde_json::Map::new(),
    };
    let should_fill = values
        .get("effectiveRateMultiplier")
        .is_none_or(serde_json::Value::is_null);
    if should_fill {
        values.insert(
            "effectiveRateMultiplier".to_string(),
            serde_json::json!(effective_rate_multiplier),
        );
    }
    serde_json::to_string(&serde_json::Value::Object(values)).ok()
}

fn sanitized_activity_scalar(value: serde_json::Value) -> Option<serde_json::Value> {
    match value {
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
            Some(value)
        }
        serde_json::Value::String(value) if value.len() <= 160 => {
            Some(serde_json::Value::String(value))
        }
        _ => None,
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

    use super::{
        activity_json_with_group_rate, collector_failed_task_types_from_summary,
        sanitized_activity_json, ChangeCenterWorkspaceQuery,
    };
    use crate::{
        application::{clock::SystemClock, ids::UuidV7Generator, stations::StationService},
        models::stations::CreateStationInput,
        persistence::{error::PersistenceError, runtime::PersistenceRuntime},
    };

    #[tokio::test]
    async fn activity_feed_lists_informational_changes_in_time_order() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("activity-feed.sqlite3"))
                .await
                .expect("runtime");

        runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    for (id, event_type, observed_at_ms, summary) in [
                        (
                            "change-1",
                            "group_added",
                            100_i64,
                            r#"{"groupName":"default","status":"available","effectiveRateMultiplier":0.07}"#,
                        ),
                        (
                            "change-2",
                            "group_rate_changed",
                            300_i64,
                            r#"{"groupName":"default","oldEffectiveRateMultiplier":1.0,"newEffectiveRateMultiplier":0.8}"#,
                        ),
                        (
                            "change-3",
                            "group_missing",
                            200_i64,
                            r#"{"groupName":"Claude Kiro 高速","status":"missing"}"#,
                        ),
                    ] {
                        sqlx::query(
                            "INSERT INTO change_event_occurrences (
                                id, source_observation_key, event_type, category,
                                observation_kind, severity, object_type, source,
                                reason_code, new_value_json, observed_at_ms, created_at_ms
                             ) VALUES (?1, ?2, ?3, 'audit_change', 'change', 'warning',
                                       'station_group_binding', 'collector', ?3, ?4, ?5, ?5)",
                        )
                        .bind(id)
                        .bind(format!("fixture:{id}"))
                        .bind(event_type)
                        .bind(summary)
                        .bind(observed_at_ms)
                        .execute(write.connection())
                        .await?;
                    }
                    Ok::<(), PersistenceError>(())
                })
            })
            .await
            .expect("activity fixtures");

        let query = ChangeCenterWorkspaceQuery::new(runtime.handle());
        let first_page = query
            .list_activity(None, None, None, false, None, None, 2)
            .await
            .expect("first activity page");
        assert_eq!(first_page.active_count, 0);
        assert_eq!(first_page.unseen_count, 3);
        assert_eq!(first_page.total_count, 3);
        assert_eq!(first_page.items.len(), 2);
        assert_eq!(first_page.items[0].record_type, "change");
        assert_eq!(first_page.items[0].event_type, "group_rate_changed");
        assert_eq!(first_page.items[0].severity, "info");
        assert_eq!(first_page.items[1].record_type, "change");
        assert_eq!(first_page.items[1].event_type, "group_missing");
        assert_eq!(first_page.items[1].severity, "info");

        let second_page = query
            .list_activity(
                None,
                None,
                None,
                false,
                None,
                first_page.next_cursor.as_ref(),
                2,
            )
            .await
            .expect("second activity page");
        assert_eq!(second_page.items.len(), 1);
        assert_eq!(second_page.total_count, 3);
        assert_eq!(second_page.items[0].event_type, "group_added");
        assert_eq!(
            second_page.items[0].new_value_json.as_deref(),
            Some(r#"{"effectiveRateMultiplier":0.07,"groupName":"default","status":"available"}"#)
        );
        assert!(second_page.next_cursor.is_none());

        let information_page = query
            .list_activity(None, None, Some("change"), false, None, None, 10)
            .await
            .expect("informational activity page");
        assert_eq!(information_page.items.len(), 3);
        assert!(information_page
            .items
            .iter()
            .all(|item| item.record_type == "change"));
        assert_eq!(information_page.unseen_count, 3);
        assert_eq!(information_page.total_count, 3);

        runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE change_event_occurrences SET seen_at_ms = 400 WHERE id = 'change-2'",
                    )
                    .execute(write.connection())
                    .await?;
                    Ok::<(), PersistenceError>(())
                })
            })
            .await
            .expect("mark informational fixture read");
        let unread_page = query
            .list_activity(None, None, None, true, None, None, 10)
            .await
            .expect("unread activity page");
        assert_eq!(unread_page.items.len(), 2);
        assert_eq!(unread_page.unseen_count, 2);
        assert!(unread_page
            .items
            .iter()
            .all(|item| item.seen_at_ms.is_none()));
        assert!(unread_page
            .items
            .iter()
            .all(|item| item.record_type == "change"));
        assert!(unread_page.items.iter().any(|item| item.id == "change-1"));

        runtime.close().await.expect("close runtime");
    }

    #[test]
    fn activity_json_keeps_new_group_effective_multiplier() {
        assert_eq!(
            sanitized_activity_json(
                "group_added",
                Some(
                    r#"{"groupName":"default","effectiveRateMultiplier":0.07,"secret":"redacted"}"#
                ),
            )
            .as_deref(),
            Some(r#"{"effectiveRateMultiplier":0.07,"groupName":"default"}"#)
        );
        assert_eq!(
            activity_json_with_group_rate(
                "group_added",
                Some(r#"{"groupName":"default","status":"available"}"#),
                Some(0.07),
            )
            .as_deref(),
            Some(r#"{"effectiveRateMultiplier":0.07,"groupName":"default","status":"available"}"#)
        );
    }

    #[tokio::test]
    async fn activity_feed_fills_missing_new_group_multiplier_from_binding() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("activity-group-rate.sqlite3"))
                .await
                .expect("runtime");
        let station = StationService::new(
            runtime.handle(),
            Arc::new(SystemClock),
            Arc::new(UuidV7Generator),
        )
        .create(CreateStationInput {
            name: "Group rate fixture".to_string(),
            station_type: "sub2api".to_string(),
            website_url: "https://group-rate.example".to_string(),
            api_base_url: "https://group-rate.example/v1".to_string(),
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
        runtime
            .handle()
            .write(|write| {
                let station_id = station.id.clone();
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO station_group_bindings (
                            id, station_id, binding_kind, group_key_hash, group_name,
                            binding_status, effective_rate_multiplier, confidence,
                            created_at, updated_at
                         ) VALUES ('binding-key-1', ?1, 'station_group', 'hash-1', 'default',
                                   'available', 0.07, 0.5, '0', '0')",
                    )
                    .bind(&station_id)
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        "INSERT INTO change_event_occurrences (
                            id, source_observation_key, event_type, category,
                            observation_kind, severity, object_type, object_id, station_id,
                            source, reason_code, new_value_json, observed_at_ms, created_at_ms
                         ) VALUES ('group-added-1', 'fixture:group-added-1', 'group_added',
                                   'audit_change', 'change', 'info', 'station_group_binding',
                                   'binding-key-1', ?1, 'collector', 'group_added',
                                   '{\"groupName\":\"default\"}', 100, 100)",
                    )
                    .bind(&station_id)
                    .execute(write.connection())
                    .await?;
                    Ok::<(), PersistenceError>(())
                })
            })
            .await
            .expect("activity fixtures");

        let page = ChangeCenterWorkspaceQuery::new(runtime.handle())
            .list_activity(None, None, None, false, None, None, 10)
            .await
            .expect("activity page");
        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items[0].new_value_json.as_deref(),
            Some(r#"{"effectiveRateMultiplier":0.07,"groupName":"default"}"#)
        );
        let searched_page = ChangeCenterWorkspaceQuery::new(runtime.handle())
            .list_activity(None, None, None, false, Some("rate fixture"), None, 10)
            .await
            .expect("searched activity page");
        assert_eq!(searched_page.total_count, 1);
        assert_eq!(searched_page.items.len(), 1);
        assert_eq!(searched_page.items[0].id, "group-added-1");
        let internal_key_search = ChangeCenterWorkspaceQuery::new(runtime.handle())
            .list_activity(None, None, None, false, Some("ke"), None, 10)
            .await
            .expect("internal key search");
        assert_eq!(internal_key_search.total_count, 0);
        assert!(internal_key_search.items.is_empty());
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn activity_cursor_does_not_skip_older_active_incidents() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("activity-cursor.sqlite3"))
                .await
                .expect("runtime");
        runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO change_incidents (
                            id, condition_key, event_type, lifecycle_state,
                            base_severity, severity, object_type,
                            lifecycle_policy_fingerprint, episode_number, first_seen_at_ms,
                            last_seen_at_ms, occurrence_count, episode_occurrence_count,
                            last_observation_summary_json, created_at_ms, updated_at_ms
                         ) VALUES ('older-active', 'fixture:older-active', 'collector_failed',
                                   'open', 'warning', 'warning', 'station', 'fixture', 1,
                                   100, 100, 1, 1, '{}', 100, 100)",
                    )
                    .execute(write.connection())
                    .await?;
                    for (id, observed_at_ms) in
                        [("newer-change", 300_i64), ("middle-change", 200_i64)]
                    {
                        sqlx::query(
                            "INSERT INTO change_event_occurrences (
                                id, source_observation_key, event_type, category,
                                observation_kind, severity, object_type, source,
                                observed_at_ms, created_at_ms
                             ) VALUES (?1, ?2, 'audit_change', 'audit_change', 'change',
                                       'info', 'global', 'fixture', ?3, ?3)",
                        )
                        .bind(id)
                        .bind(format!("fixture:{id}"))
                        .bind(observed_at_ms)
                        .execute(write.connection())
                        .await?;
                    }
                    Ok::<(), PersistenceError>(())
                })
            })
            .await
            .expect("activity cursor fixtures");

        let query = ChangeCenterWorkspaceQuery::new(runtime.handle());
        let first_page = query
            .list_activity(None, None, None, false, None, None, 2)
            .await
            .expect("first activity page");
        assert_eq!(first_page.active_count, 1);
        assert_eq!(first_page.total_count, 3);
        assert_eq!(
            first_page
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["newer-change", "middle-change"]
        );

        let second_page = query
            .list_activity(
                None,
                None,
                None,
                false,
                None,
                first_page.next_cursor.as_ref(),
                2,
            )
            .await
            .expect("second activity page");
        assert_eq!(second_page.total_count, 3);
        assert_eq!(
            second_page
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["older-active"]
        );
        assert!(second_page.next_cursor.is_none());

        runtime.close().await.expect("close runtime");
    }

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
            .list_current(None, None, Some("active"), None, None, 100)
            .await
            .expect("list current alerts");

        assert_eq!(page.active_count, 1);
        assert_eq!(page.total_count, 1);
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

    #[tokio::test]
    async fn missing_group_activity_is_information_not_an_active_problem() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temp.path().join("group-missing.sqlite3"))
                .await
                .expect("runtime");
        runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO change_event_occurrences (
                            id, source_observation_key, event_type, category,
                            observation_kind, severity, condition_key, object_type, source,
                            reason_code, new_value_json, observed_at_ms, created_at_ms
                         ) VALUES ('missing-group-1', 'fixture:group_missing', 'group_missing',
                                   'audit_change', 'change', 'info',
                                   'station_group:station-1:group-1', 'station_group_binding',
                                   'fixture', 'group_missing',
                                   '{\"groupName\":\"Claude Kiro 高速\",\"status\":\"missing\"}',
                                   200, 200)",
                    )
                    .execute(write.connection())
                    .await?;
                    Ok::<(), PersistenceError>(())
                })
            })
            .await
            .expect("missing group fixture");

        let query = ChangeCenterWorkspaceQuery::new(runtime.handle());
        let information_page = query
            .list_activity(None, Some("info"), Some("change"), false, None, None, 10)
            .await
            .expect("list missing group information");
        assert_eq!(information_page.items.len(), 1);
        assert_eq!(information_page.items[0].record_type, "change");
        assert_eq!(information_page.items[0].severity, "info");
        assert_eq!(
            information_page.items[0].group_name.as_deref(),
            Some("Claude Kiro 高速")
        );

        let active_page = query
            .list_current(None, None, Some("active"), None, None, 10)
            .await
            .expect("list active problems");
        assert!(active_page.items.is_empty());
        assert_eq!(active_page.active_count, 0);
        assert_eq!(active_page.total_count, 0);

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
