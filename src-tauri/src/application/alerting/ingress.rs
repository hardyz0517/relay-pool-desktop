use crate::{
    models::alerting::{
        resolve_policy, AlertEventType, ConditionKey, EventCategory, Incident, IncidentObservation,
        ObservationKind, PolicyMatchContext, Severity, StateTransition,
    },
    persistence::{
        error::PersistenceError,
        runtime::PersistenceHandle,
        stores::alerting::{
            AlertingSettingsStore, AttentionKey, AttentionStore, DeliveryStore, IncidentSnapshot,
            IncidentStore, OccurrenceInsert, OccurrenceStore, PolicyStore,
        },
        WriteSession,
    },
};

use super::{delivery_planner::DeliveryPlanner, policy_service::AlertingSettings};

#[derive(Debug, Clone)]
pub(crate) struct ObservationIngress {
    pub source_observation_key: String,
    pub event_type: AlertEventType,
    pub condition_key: ConditionKey,
    pub kind: ObservationKind,
    pub severity: Severity,
    pub object_type: String,
    pub object_id: Option<String>,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub source: String,
    pub reason_code: Option<String>,
    pub summary_json: String,
    pub observed_at_ms: i64,
    pub fact_fresh_until_ms: i64,
}

impl ObservationIngress {
    fn validate(&self) -> Result<(), PersistenceError> {
        if self.source_observation_key.is_empty()
            || self.source_observation_key.len() > 200
            || !self
                .source_observation_key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
            || self.object_type.is_empty()
            || self.object_type.len() > 100
            || self.source.is_empty()
            || self.source.len() > 100
            || self.observed_at_ms < 0
            || self.fact_fresh_until_ms < self.observed_at_ms
            || !is_redacted_json(&self.summary_json)
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        Ok(())
    }
}

fn is_redacted_json(value: &str) -> bool {
    if value.len() > 16 * 1024 {
        return false;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(value) else {
        return false;
    };
    fn walk(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(map) => map.iter().all(|(key, value)| {
                let key = key.to_ascii_lowercase();
                ![
                    "token",
                    "secret",
                    "authorization",
                    "password",
                    "cookie",
                    "api_key",
                ]
                .iter()
                .any(|needle| key.contains(needle))
                    && walk(value)
            }),
            serde_json::Value::Array(items) => items.iter().all(walk),
            serde_json::Value::String(value) => {
                !value.contains("://") && !value.to_ascii_lowercase().contains("bearer ")
            }
            _ => true,
        }
    }
    walk(&value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IngressResult {
    pub inserted: bool,
}

#[derive(Clone)]
pub(crate) struct AlertingIngress {
    runtime: PersistenceHandle,
}

impl AlertingIngress {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self { runtime }
    }

    pub(crate) async fn record(
        &self,
        input: ObservationIngress,
    ) -> Result<IngressResult, PersistenceError> {
        // External/IPC writers are gated until the durable startup upgrade has
        // completed. Authoritative producer transactions use
        // `record_in_session` after startup and remain atomic with their fact
        // writes; the upgrade rebuild itself also uses that lower-level entry.
        crate::services::data_store::alerting_upgrade::assert_alerting_writer_ready(&self.runtime)
            .await?;
        input.validate()?;
        let mut write = self.runtime.begin_write().await?;
        let result = self.record_in_session(&mut write, input).await?;
        write.commit().await?;
        Ok(result)
    }

    /// Project an observation into an already-open authoritative write
    /// transaction. Producers use this to keep their fact write and the
    /// alerting projection atomic; it never opens or commits a nested
    /// transaction.
    pub(crate) async fn record_in_session(
        &self,
        write: &mut WriteSession,
        input: ObservationIngress,
    ) -> Result<IngressResult, PersistenceError> {
        input.validate()?;
        let store = OccurrenceStore;
        let id = format!("occurrence-{}", input.source_observation_key);
        let condition_key = input.condition_key.clone();
        let event_type = input.event_type;
        let kind = input.kind;
        let severity = input.severity;
        let observed_at_ms = input.observed_at_ms;
        let fresh_until_ms = input.fact_fresh_until_ms;
        let source_observation_key = input.source_observation_key.clone();
        let occurrence = OccurrenceInsert {
            id,
            source_observation_key: input.source_observation_key,
            event_type: input.event_type,
            category: if input.kind == ObservationKind::Change {
                EventCategory::AuditChange
            } else {
                EventCategory::ConditionObservation
            },
            observation_kind: input.kind,
            severity: input.severity,
            condition_key: Some(input.condition_key),
            object_type: input.object_type,
            object_id: input.object_id,
            station_id: input.station_id,
            station_key_id: input.station_key_id,
            source: input.source,
            reason_code: input.reason_code,
            old_value_json: None,
            new_value_json: Some(input.summary_json),
            impact_json: None,
            observed_at_ms: input.observed_at_ms,
            created_at_ms: input.observed_at_ms,
        };
        let result = store.insert_ignore(write, &occurrence).await?;
        if !result.inserted {
            return Ok(IngressResult { inserted: false });
        }

        // Changes are append-only audit facts. Only condition observations
        // enter the incident state machine.
        if kind != ObservationKind::Change {
            let incident_store = IncidentStore;
            let policies = PolicyStore.list_for_write(write).await?;
            let policy_context = PolicyMatchContext {
                event_type,
                base_severity: severity,
                station_id: occurrence.station_id.as_deref(),
                station_key_id: occurrence.station_key_id.as_deref(),
            };
            let policy = resolve_policy(&policies, &policy_context);
            let settings = load_settings_for_write(write).await?;
            let mut snapshot = incident_store
                .load_for_write(write, condition_key.as_str())
                .await?
                .unwrap_or_else(|| IncidentSnapshot {
                    incident: Incident::new(
                        format!("incident-{}", condition_key.as_str()),
                        condition_key.clone(),
                        event_type,
                        severity,
                        observed_at_ms,
                        format!("{}:{}", policy.id, policy.revision),
                    ),
                });
            let expected_version = snapshot.incident.version;
            snapshot.incident.object_type = occurrence.object_type.clone();
            snapshot.incident.object_id = occurrence.object_id.clone();
            snapshot.incident.station_id = occurrence.station_id.clone();
            snapshot.incident.station_key_id = occurrence.station_key_id.clone();
            // `system_default` is an immutable virtual profile, not a row in
            // alert_policies. Keep its fingerprint/revision in the incident
            // snapshot, but never write a dangling foreign-key reference.
            let persisted_policy_id = (policy.id != "system_default").then(|| policy.id.clone());
            snapshot.incident.policy_id = persisted_policy_id;
            snapshot.incident.policy_revision =
                (policy.id != "system_default").then_some(policy.revision);
            snapshot.incident.lifecycle_policy_fingerprint =
                format!("{}:{}", policy.id, policy.revision);
            snapshot.incident.severity = policy.effective_severity(severity);
            let observation = IncidentObservation {
                source_observation_key,
                event_type,
                condition_key: condition_key.clone(),
                kind,
                severity,
                observed_at_ms,
                fact_fresh_until_ms: fresh_until_ms,
                summary_json: occurrence
                    .new_value_json
                    .clone()
                    .unwrap_or_else(|| "{}".to_string()),
            };
            let transition = super::incident_projector::IncidentProjector
                .apply(&mut snapshot.incident, &observation, &policy)
                .map_err(|error| match error {
                    super::incident_projector::ProjectionError::InvalidFreshness => {
                        PersistenceError::ConstraintViolation
                    }
                    super::incident_projector::ProjectionError::ObservationOutOfOrder => {
                        PersistenceError::ConstraintViolation
                    }
                })?;
            /*
             * The reducer owns lifecycle state; policy/settings only decide
             * when and where a notification is recorded. This keeps a
             * disabled channel from changing the incident itself.
             */
            if expected_version == 0 && snapshot.incident.occurrence_count == 0 {
                // A first healthy observation still creates a durable
                // resolved episode so future abnormal observations are
                // correctly treated as a reopen.
                snapshot.incident.version = 1;
            }
            if expected_version == 0 {
                incident_store.insert_snapshot(write, &snapshot).await?;
            } else {
                incident_store
                    .update_snapshot_cas(write, &snapshot, expected_version)
                    .await?;
            }
            AttentionStore
                .ensure(
                    write,
                    AttentionKey {
                        incident_id: &snapshot.incident.id,
                        episode_number: snapshot.incident.episode_number as i64,
                    },
                    observed_at_ms,
                )
                .await?;
            incident_store
                .link_occurrence(
                    write,
                    &occurrence.id,
                    &snapshot.incident.id,
                    snapshot.incident.episode_number as i64,
                )
                .await?;

            if transition != StateTransition::None
                && !(transition == StateTransition::Resolved
                    && snapshot.incident.episode_occurrence_count == 0)
            {
                let attention = None;
                for planned in DeliveryPlanner.plan_transition(
                    transition,
                    &snapshot.incident,
                    &policy,
                    &settings,
                    attention,
                    observed_at_ms,
                ) {
                    let mut delivery = planned.delivery;
                    if let Some(reason) = planned.suppression_reason {
                        delivery
                            .suppress(reason, observed_at_ms)
                            .map_err(PersistenceError::InvariantViolation)?;
                    }
                    DeliveryStore.schedule(write, &delivery).await?;
                }
            }
        }
        Ok(IngressResult { inserted: true })
    }
}

async fn load_settings_for_write(
    write: &mut WriteSession,
) -> Result<AlertingSettings, PersistenceError> {
    let value = AlertingSettingsStore.load_json_for_write(write).await?;
    let settings = value
        .map(|value| serde_json::from_str::<AlertingSettings>(&value))
        .transpose()
        .map_err(|_| PersistenceError::InvariantViolation("invalid alerting settings".into()))?
        .unwrap_or_default();
    settings.validate()?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingress_rejects_unbounded_or_stale_payloads() {
        let mut input = ObservationIngress {
            source_observation_key: "obs-1".to_string(),
            event_type: AlertEventType::StationDown,
            condition_key: ConditionKey::new("station:1").unwrap(),
            kind: ObservationKind::Abnormal,
            severity: Severity::Critical,
            object_type: "station".to_string(),
            object_id: Some("station-1".to_string()),
            station_id: Some("station-1".to_string()),
            station_key_id: None,
            source: "fixture".to_string(),
            reason_code: None,
            summary_json: "{}".to_string(),
            observed_at_ms: 10,
            fact_fresh_until_ms: 20,
        };
        assert!(input.validate().is_ok());
        input.fact_fresh_until_ms = 9;
        assert!(input.validate().is_err());
        input.fact_fresh_until_ms = 20;
        input.summary_json = r#"{"authorization":"Bearer secret"}"#.to_string();
        assert!(input.validate().is_err());
    }
}
