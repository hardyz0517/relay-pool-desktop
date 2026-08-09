use crate::models::alerting::{AlertPolicy, Incident, IncidentObservation, StateTransition};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProjectionError {
    InvalidFreshness,
    ObservationOutOfOrder,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct IncidentProjector;

impl IncidentProjector {
    pub(crate) fn apply(
        &self,
        incident: &mut Incident,
        observation: &IncidentObservation,
        policy: &AlertPolicy,
    ) -> Result<StateTransition, ProjectionError> {
        if observation.fact_fresh_until_ms < observation.observed_at_ms {
            return Err(ProjectionError::InvalidFreshness);
        }
        if observation.observed_at_ms < incident.last_seen_at_ms {
            return Err(ProjectionError::ObservationOutOfOrder);
        }
        // Healthy observations after resolution are harmless replays. They do not
        // reopen an episode and the reducer returns `None` without changing state.
        Ok(incident.apply_observation(
            observation,
            policy.trigger_mode,
            policy.trigger_count,
            policy.trigger_duration_seconds,
            policy.recovery_mode,
            policy.recovery_count,
            policy.recovery_duration_seconds,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::alerting::{
        AlertEventType, ConditionKey, IncidentObservation, ObservationKind, Severity,
    };

    fn policy() -> AlertPolicy {
        let mut policy = AlertPolicy::system_default(Severity::Warning);
        policy.id = "test".to_string();
        policy
    }

    fn observation(at: i64, until: i64) -> IncidentObservation {
        IncidentObservation {
            source_observation_key: format!("obs-{at}"),
            event_type: AlertEventType::StationDown,
            condition_key: ConditionKey::new("station:1").unwrap(),
            kind: ObservationKind::Abnormal,
            severity: Severity::Warning,
            observed_at_ms: at,
            fact_fresh_until_ms: until,
            summary_json: "{}".to_string(),
        }
    }

    #[test]
    fn projector_rejects_invalid_or_stale_observations_before_state_mutation() {
        let mut incident = Incident::new(
            "incident-1",
            ConditionKey::new("station:1").unwrap(),
            AlertEventType::StationDown,
            Severity::Warning,
            100,
            "policy-v1",
        );
        let projector = IncidentProjector;
        assert_eq!(
            projector.apply(&mut incident, &observation(100, 99), &policy()),
            Err(ProjectionError::InvalidFreshness)
        );
        assert_eq!(incident.version, 0);
    }
}
