use std::sync::Arc;

use crate::{
    application::{
        alerting::policy_service::{AlertingSettings, PolicyService},
        alerting::{AlertingIngress, AlertingReadModelUpdatePublisher, ObservationIngress},
        error::ApplicationError,
        queries::change_center_workspace::{
            ActivityCursor, ActivityWorkspacePage, ChangeCenterWorkspaceQuery, DeliveryCursor,
            DeliveryHistoryPage, IncidentCursor, IncidentSummary, IncidentWorkspacePage,
            OccurrenceCursor, OccurrenceHistoryPage,
        },
    },
    models::alerting::{AlertEventType, ConditionKey, ObservationKind, Severity},
    persistence::{runtime::PersistenceHandle, stores::alerting::AttentionKey},
};

#[derive(Clone)]
pub(crate) struct AlertingCommandFacade {
    runtime: PersistenceHandle,
    workspace: ChangeCenterWorkspaceQuery,
    ingress: AlertingIngress,
    policy_service: PolicyService,
    alerting_updates: Arc<dyn AlertingReadModelUpdatePublisher>,
}

impl AlertingCommandFacade {
    pub(crate) fn new(
        runtime: PersistenceHandle,
        alerting_updates: Arc<dyn AlertingReadModelUpdatePublisher>,
    ) -> Self {
        Self {
            workspace: ChangeCenterWorkspaceQuery::new_with_alerting_read_model_updates(
                runtime.clone(),
                Arc::clone(&alerting_updates),
            ),
            ingress: AlertingIngress::new(runtime.clone()),
            policy_service: PolicyService::new(runtime.clone()),
            runtime,
            alerting_updates,
        }
    }

    pub(crate) async fn list_policies(
        &self,
    ) -> Result<Vec<crate::models::alerting::AlertPolicy>, ApplicationError> {
        self.policy_service
            .list_policies()
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn save_policy(
        &self,
        policy: crate::models::alerting::AlertPolicy,
        expected_revision: Option<u64>,
    ) -> Result<crate::models::alerting::AlertPolicy, ApplicationError> {
        self.policy_service
            .save_policy(policy.clone(), expected_revision)
            .await
            .map_err(ApplicationError::from)?;
        self.policy_service
            .get_policy(&policy.id)
            .await
            .map_err(ApplicationError::from)?
            .ok_or(ApplicationError::NotFound)
    }

    pub(crate) async fn delete_policy(
        &self,
        id: &str,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<(), ApplicationError> {
        self.policy_service
            .set_policy_state(
                id,
                crate::models::alerting::PolicyState::Tombstone,
                expected_revision,
                now_ms,
            )
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn load_settings(&self) -> Result<AlertingSettings, ApplicationError> {
        self.policy_service
            .load_settings()
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update_settings(
        &self,
        settings: AlertingSettings,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<AlertingSettings, ApplicationError> {
        self.policy_service
            .update_settings(settings, expected_revision, now_ms)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_current(
        &self,
        station_id: Option<&str>,
        severity: Option<&str>,
        lifecycle_state: Option<&str>,
        search: Option<&str>,
        cursor: Option<&IncidentCursor>,
        limit: u32,
    ) -> Result<IncidentWorkspacePage, ApplicationError> {
        self.workspace
            .list_current(station_id, severity, lifecycle_state, search, cursor, limit)
            .await
            .map_err(Into::into)
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
    ) -> Result<ActivityWorkspacePage, ApplicationError> {
        self.workspace
            .list_activity(
                station_id,
                severity,
                record_type,
                unread_only,
                search,
                cursor,
                limit,
            )
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn get_incident_detail(
        &self,
        incident_id: &str,
        episode_number: i64,
    ) -> Result<Option<IncidentSummary>, ApplicationError> {
        self.workspace
            .get_incident_detail(incident_id, episode_number)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_occurrences(
        &self,
        incident_id: &str,
        episode_number: i64,
        cursor: Option<&OccurrenceCursor>,
        limit: u32,
    ) -> Result<OccurrenceHistoryPage, ApplicationError> {
        self.workspace
            .list_occurrences(incident_id, episode_number, cursor, limit)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_deliveries(
        &self,
        incident_id: &str,
        episode_number: i64,
        cursor: Option<&DeliveryCursor>,
        limit: u32,
    ) -> Result<DeliveryHistoryPage, ApplicationError> {
        self.workspace
            .list_deliveries(incident_id, episode_number, cursor, limit)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn record_observation(
        &self,
        input: ObservationIngress,
    ) -> Result<bool, ApplicationError> {
        let inserted = self.ingress.record(input).await?.inserted;
        if inserted {
            self.alerting_updates.notify_after_commit();
        }
        Ok(inserted)
    }

    pub(crate) async fn mark_seen(
        &self,
        incident_id: &str,
        episode_number: i64,
        now_ms: i64,
    ) -> Result<(), ApplicationError> {
        let incident_id = incident_id.to_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    crate::persistence::stores::alerting::AttentionStore
                        .ensure(
                            write,
                            AttentionKey {
                                incident_id: &incident_id,
                                episode_number,
                            },
                            now_ms,
                        )
                        .await?;
                    crate::persistence::stores::alerting::AttentionStore
                        .mark_seen(
                            write,
                            AttentionKey {
                                incident_id: &incident_id,
                                episode_number,
                            },
                            now_ms,
                        )
                        .await
                })
            })
            .await
            .map_err(ApplicationError::from)?;
        self.alerting_updates.notify_after_commit();
        Ok(())
    }

    pub(crate) async fn mark_information_seen(
        &self,
        activity_id: &str,
        now_ms: i64,
    ) -> Result<(), ApplicationError> {
        let activity_id = activity_id.to_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    let affected = crate::persistence::stores::alerting::OccurrenceStore
                        .mark_informational_change_seen(write, &activity_id, now_ms)
                        .await?;
                    if affected == 1 {
                        Ok(())
                    } else {
                        Err(crate::persistence::error::PersistenceError::NotFound)
                    }
                })
            })
            .await
            .map_err(ApplicationError::from)?;
        self.alerting_updates.notify_after_commit();
        Ok(())
    }

    pub(crate) async fn mark_all_seen(
        &self,
        station_id: Option<String>,
        severity: Option<Severity>,
        mark_incidents: bool,
        mark_information: bool,
        now_ms: i64,
    ) -> Result<u64, ApplicationError> {
        let marked = self
            .runtime
            .write(|write| {
                Box::pin(async move {
                    let incidents = if mark_incidents {
                        crate::persistence::stores::alerting::AttentionStore
                            .mark_all_seen(write, station_id.as_deref(), severity, now_ms)
                            .await?
                    } else {
                        0
                    };
                    let information = if mark_information {
                        crate::persistence::stores::alerting::OccurrenceStore
                            .mark_all_informational_changes_seen(
                                write,
                                station_id.as_deref(),
                                severity,
                                now_ms,
                            )
                            .await?
                    } else {
                        0
                    };
                    Ok(incidents + information)
                })
            })
            .await
            .map_err(ApplicationError::from)?;
        if marked > 0 {
            self.alerting_updates.notify_after_commit();
        }
        Ok(marked)
    }

    pub(crate) async fn resolve_all_active(
        &self,
        station_id: Option<String>,
        severity: Option<Severity>,
        now_ms: i64,
    ) -> Result<u64, ApplicationError> {
        let incident_store = crate::persistence::stores::alerting::IncidentStore;
        let resolved = self
            .runtime
            .write(|write| {
                Box::pin(async move {
                    incident_store
                        .resolve_all_active(write, station_id.as_deref(), severity, now_ms)
                        .await
                })
            })
            .await
            .map_err(ApplicationError::from)?;
        if resolved > 0 {
            self.alerting_updates.notify_after_commit();
        }
        Ok(resolved)
    }

    pub(crate) async fn clear_activity(
        &self,
        station_id: Option<String>,
        severity: Option<Severity>,
        lifecycle_state: Option<&str>,
        clear_incidents: bool,
        clear_information: bool,
    ) -> Result<u64, ApplicationError> {
        let incident_store = crate::persistence::stores::alerting::IncidentStore;
        let occurrence_store = crate::persistence::stores::alerting::OccurrenceStore;
        let lifecycle_state = lifecycle_state.map(str::to_owned);
        let cleared = self
            .runtime
            .write(|write| {
                Box::pin(async move {
                    let cleared_incidents = if clear_incidents {
                        incident_store
                            .clear(
                                write,
                                station_id.as_deref(),
                                severity,
                                lifecycle_state.as_deref(),
                            )
                            .await?
                    } else {
                        0
                    };
                    let cleared_information = if clear_information {
                        occurrence_store
                            .clear_informational_changes(
                                write,
                                station_id.as_deref(),
                                severity,
                                lifecycle_state.as_deref() == Some("unread"),
                            )
                            .await?
                    } else {
                        0
                    };
                    Ok(cleared_incidents + cleared_information)
                })
            })
            .await
            .map_err(ApplicationError::from)?;
        if cleared > 0 {
            self.alerting_updates.notify_after_commit();
        }
        Ok(cleared)
    }

    pub(crate) async fn snooze(
        &self,
        incident_id: &str,
        episode_number: i64,
        until_ms: i64,
        now_ms: i64,
    ) -> Result<(), ApplicationError> {
        if until_ms <= now_ms {
            return Err(ApplicationError::ConstraintViolation);
        }
        let incident_id = incident_id.to_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    crate::persistence::stores::alerting::AttentionStore
                        .ensure(
                            write,
                            AttentionKey {
                                incident_id: &incident_id,
                                episode_number,
                            },
                            now_ms,
                        )
                        .await?;
                    crate::persistence::stores::alerting::AttentionStore
                        .snooze_until(
                            write,
                            AttentionKey {
                                incident_id: &incident_id,
                                episode_number,
                            },
                            until_ms,
                            now_ms,
                        )
                        .await
                })
            })
            .await
            .map_err(ApplicationError::from)?;
        self.alerting_updates.notify_after_commit();
        Ok(())
    }
}

pub(crate) fn parse_observation(
    source_observation_key: String,
    event_type: String,
    condition_key: String,
    kind: String,
    severity: String,
    object_type: String,
    object_id: Option<String>,
    station_id: Option<String>,
    station_key_id: Option<String>,
    source: String,
    reason_code: Option<String>,
    summary_json: String,
    observed_at_ms: i64,
    fact_fresh_until_ms: i64,
) -> Result<ObservationIngress, ApplicationError> {
    let event_type =
        AlertEventType::from_str(&event_type).ok_or(ApplicationError::ConstraintViolation)?;
    let condition_key =
        ConditionKey::new(condition_key).map_err(|_| ApplicationError::ConstraintViolation)?;
    let kind = match kind.as_str() {
        "abnormal" => ObservationKind::Abnormal,
        "healthy" => ObservationKind::Healthy,
        "change" => ObservationKind::Change,
        _ => return Err(ApplicationError::ConstraintViolation),
    };
    let severity = Severity::from_str(&severity).ok_or(ApplicationError::ConstraintViolation)?;
    Ok(ObservationIngress {
        source_observation_key,
        event_type,
        condition_key,
        kind,
        severity,
        object_type,
        object_id,
        station_id,
        station_key_id,
        source,
        reason_code,
        summary_json,
        observed_at_ms,
        fact_fresh_until_ms,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use crate::{
        application::alerting::AlertingReadModelUpdatePublisher,
        models::alerting::{AlertEventType, ConditionKey, ObservationKind, Severity},
        persistence::runtime::PersistenceRuntime,
    };

    use super::{AlertingCommandFacade, ObservationIngress};

    #[derive(Default)]
    struct RecordingAlertingUpdates {
        notifications: AtomicUsize,
    }

    impl RecordingAlertingUpdates {
        fn count(&self) -> usize {
            self.notifications.load(Ordering::SeqCst)
        }
    }

    impl AlertingReadModelUpdatePublisher for RecordingAlertingUpdates {
        fn notify_after_commit(&self) {
            self.notifications.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn change_observation(source_observation_key: &str) -> ObservationIngress {
        ObservationIngress {
            source_observation_key: source_observation_key.to_string(),
            event_type: AlertEventType::AuditChange,
            condition_key: ConditionKey::new("global:fixture:audit-change".to_string())
                .expect("fixture condition key"),
            kind: ObservationKind::Change,
            severity: Severity::Info,
            object_type: "global".to_string(),
            object_id: None,
            station_id: None,
            station_key_id: None,
            source: "fixture".to_string(),
            reason_code: Some("fixture_change".to_string()),
            summary_json: "{}".to_string(),
            observed_at_ms: 100,
            fact_fresh_until_ms: 100,
        }
    }

    #[tokio::test]
    async fn publishes_after_a_committed_read_model_change_but_not_for_a_duplicate() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&temporary.path().join("alerting.sqlite3"))
                .await
                .expect("runtime");
        crate::services::data_store::alerting_upgrade::run_durable_upgrade(&runtime.handle(), 100)
            .await
            .expect("complete alerting upgrade");
        let updates = Arc::new(RecordingAlertingUpdates::default());
        let facade = AlertingCommandFacade::new(runtime.handle(), updates.clone());

        assert!(facade
            .record_observation(change_observation("fixture-change-1"))
            .await
            .expect("record change"));
        assert_eq!(updates.count(), 1);

        assert!(!facade
            .record_observation(change_observation("fixture-change-1"))
            .await
            .expect("repeat change"));
        assert_eq!(updates.count(), 1);

        runtime.close().await.expect("close runtime");
    }
}
