use crate::{
    application::{
        alerting::policy_service::{AlertingSettings, PolicyService},
        alerting::{AlertingIngress, ObservationIngress},
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
}

impl AlertingCommandFacade {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            workspace: ChangeCenterWorkspaceQuery::new(runtime.clone()),
            ingress: AlertingIngress::new(runtime.clone()),
            policy_service: PolicyService::new(runtime.clone()),
            runtime,
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
        cursor: Option<&IncidentCursor>,
        limit: u32,
    ) -> Result<IncidentWorkspacePage, ApplicationError> {
        self.workspace
            .list_current(station_id, severity, lifecycle_state, cursor, limit)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_activity(
        &self,
        station_id: Option<&str>,
        severity: Option<&str>,
        cursor: Option<&ActivityCursor>,
        limit: u32,
    ) -> Result<ActivityWorkspacePage, ApplicationError> {
        self.workspace
            .list_activity(station_id, severity, cursor, limit)
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
        Ok(self.ingress.record(input).await?.inserted)
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
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn mark_all_seen(
        &self,
        station_id: Option<String>,
        severity: Option<Severity>,
        now_ms: i64,
    ) -> Result<u64, ApplicationError> {
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    crate::persistence::stores::alerting::AttentionStore
                        .mark_all_seen(write, station_id.as_deref(), severity, now_ms)
                        .await
                })
            })
            .await
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn resolve_all_active(
        &self,
        station_id: Option<String>,
        severity: Option<Severity>,
        now_ms: i64,
    ) -> Result<u64, ApplicationError> {
        let incident_store = crate::persistence::stores::alerting::IncidentStore;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    incident_store
                        .resolve_all_active(write, station_id.as_deref(), severity, now_ms)
                        .await
                })
            })
            .await
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn clear_incidents(
        &self,
        station_id: Option<String>,
        severity: Option<Severity>,
        lifecycle_state: Option<&str>,
    ) -> Result<u64, ApplicationError> {
        let incident_store = crate::persistence::stores::alerting::IncidentStore;
        let lifecycle_state = lifecycle_state.map(str::to_owned);
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    incident_store
                        .clear(
                            write,
                            station_id.as_deref(),
                            severity,
                            lifecycle_state.as_deref(),
                        )
                        .await
                })
            })
            .await
            .map_err(ApplicationError::from)
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
            .map_err(ApplicationError::from)
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
