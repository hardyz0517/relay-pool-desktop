use serde::{Deserialize, Serialize};

use super::event::{AlertEventType, ConditionKey, ObservationKind, Severity};
use super::policy::{RecoveryMode, TriggerMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Pending,
    Open,
    Recovering,
    Resolved,
}

impl LifecycleState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Open => "open",
            Self::Recovering => "recovering",
            Self::Resolved => "resolved",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentObservation {
    pub source_observation_key: String,
    pub event_type: AlertEventType,
    pub condition_key: ConditionKey,
    pub kind: ObservationKind,
    pub severity: Severity,
    pub observed_at_ms: i64,
    pub fact_fresh_until_ms: i64,
    pub summary_json: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateTransition {
    None,
    Pending,
    Opened,
    Recovering,
    Resolved,
    #[expect(
        dead_code,
        reason = "contract=alerting.reopened-transition; owner=models/alerting; remove_when=post-resolution episodes no longer reopen"
    )]
    Reopened,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incident {
    pub id: String,
    pub condition_key: ConditionKey,
    pub event_type: AlertEventType,
    pub lifecycle_state: LifecycleState,
    pub base_severity: Severity,
    pub severity: Severity,
    pub object_type: String,
    pub object_id: Option<String>,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub policy_id: Option<String>,
    pub policy_revision: Option<u64>,
    pub lifecycle_policy_fingerprint: String,
    pub episode_number: u32,
    pub first_seen_at_ms: i64,
    pub last_seen_at_ms: i64,
    pub opened_at_ms: Option<i64>,
    pub recovering_at_ms: Option<i64>,
    pub resolved_at_ms: Option<i64>,
    pub occurrence_count: u64,
    pub episode_occurrence_count: u64,
    pub consecutive_abnormal_count: u32,
    pub consecutive_healthy_count: u32,
    pub pending_since_ms: Option<i64>,
    pub healthy_since_ms: Option<i64>,
    pub last_observation_id: Option<String>,
    pub last_observation_summary_json: String,
    pub fact_fresh_until_ms: Option<i64>,
    pub next_state_evaluation_at_ms: Option<i64>,
    pub last_notification_at_ms: Option<i64>,
    pub next_notification_at_ms: Option<i64>,
    pub version: u64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl Incident {
    pub fn new(
        id: impl Into<String>,
        condition_key: ConditionKey,
        event_type: AlertEventType,
        base_severity: Severity,
        now_ms: i64,
        lifecycle_policy_fingerprint: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            condition_key,
            event_type,
            lifecycle_state: LifecycleState::Pending,
            base_severity,
            severity: base_severity,
            object_type: "unknown".to_string(),
            object_id: None,
            station_id: None,
            station_key_id: None,
            policy_id: None,
            policy_revision: None,
            lifecycle_policy_fingerprint: lifecycle_policy_fingerprint.into(),
            episode_number: 1,
            first_seen_at_ms: now_ms,
            last_seen_at_ms: now_ms,
            opened_at_ms: None,
            recovering_at_ms: None,
            resolved_at_ms: None,
            occurrence_count: 0,
            episode_occurrence_count: 0,
            consecutive_abnormal_count: 0,
            consecutive_healthy_count: 0,
            pending_since_ms: Some(now_ms),
            healthy_since_ms: None,
            last_observation_id: None,
            last_observation_summary_json: "{}".to_string(),
            fact_fresh_until_ms: None,
            next_state_evaluation_at_ms: None,
            last_notification_at_ms: None,
            next_notification_at_ms: None,
            version: 0,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        }
    }

    pub fn apply_observation(
        &mut self,
        observation: &IncidentObservation,
        trigger_mode: TriggerMode,
        trigger_count: Option<u32>,
        trigger_duration_seconds: Option<u64>,
        recovery_mode: RecoveryMode,
        recovery_count: Option<u32>,
        recovery_duration_seconds: Option<u64>,
    ) -> StateTransition {
        // Invalid or out-of-order facts must never mutate lifecycle state. The
        // application projector returns a typed error for diagnostics; this
        // domain-facing convenience method remains total for replay callers.
        if observation.fact_fresh_until_ms < observation.observed_at_ms
            || observation.observed_at_ms < self.last_seen_at_ms
        {
            return StateTransition::None;
        }
        self.version = self.version.saturating_add(1);
        self.updated_at_ms = observation.observed_at_ms;
        self.last_seen_at_ms = observation.observed_at_ms;
        self.last_observation_id = Some(observation.source_observation_key.clone());
        self.last_observation_summary_json = observation.summary_json.clone();
        self.fact_fresh_until_ms = Some(observation.fact_fresh_until_ms);
        self.severity = observation.severity;

        match observation.kind {
            ObservationKind::Abnormal => {
                self.occurrence_count = self.occurrence_count.saturating_add(1);
                self.episode_occurrence_count = self.episode_occurrence_count.saturating_add(1);
                self.consecutive_abnormal_count = self.consecutive_abnormal_count.saturating_add(1);
                self.consecutive_healthy_count = 0;
                self.healthy_since_ms = None;
                self.next_state_evaluation_at_ms = None;

                if self.lifecycle_state == LifecycleState::Resolved {
                    self.episode_number = self.episode_number.saturating_add(1);
                    self.episode_occurrence_count = 1;
                    self.consecutive_abnormal_count = 1;
                    self.pending_since_ms = Some(observation.observed_at_ms);
                    self.opened_at_ms = None;
                    self.recovering_at_ms = None;
                    self.resolved_at_ms = None;
                    self.lifecycle_state = LifecycleState::Pending;
                    return self.maybe_open(
                        observation.observed_at_ms,
                        trigger_mode,
                        trigger_count,
                        trigger_duration_seconds,
                    );
                }

                if self.lifecycle_state == LifecycleState::Recovering {
                    self.lifecycle_state = LifecycleState::Open;
                    self.recovering_at_ms = None;
                    return StateTransition::Opened;
                }

                self.maybe_open(
                    observation.observed_at_ms,
                    trigger_mode,
                    trigger_count,
                    trigger_duration_seconds,
                )
            }
            ObservationKind::Healthy => {
                self.consecutive_abnormal_count = 0;
                self.consecutive_healthy_count = self.consecutive_healthy_count.saturating_add(1);
                self.pending_since_ms = None;
                self.next_state_evaluation_at_ms = None;
                if self.lifecycle_state == LifecycleState::Pending {
                    self.lifecycle_state = LifecycleState::Resolved;
                    self.resolved_at_ms = Some(observation.observed_at_ms);
                    return StateTransition::Resolved;
                }
                if self.lifecycle_state == LifecycleState::Open {
                    self.lifecycle_state = LifecycleState::Recovering;
                    self.recovering_at_ms = Some(observation.observed_at_ms);
                    self.healthy_since_ms = Some(observation.observed_at_ms);
                    if recovery_mode == RecoveryMode::HealthyDuration {
                        self.next_state_evaluation_at_ms = self
                            .healthy_since_ms
                            .zip(recovery_duration_seconds)
                            .map(|(since, duration)| {
                                since.saturating_add(duration.saturating_mul(1000) as i64)
                            });
                    }
                    if recovery_satisfied(
                        recovery_mode,
                        self.consecutive_healthy_count,
                        self.healthy_since_ms,
                        observation.observed_at_ms,
                        recovery_count,
                        recovery_duration_seconds,
                    ) {
                        self.lifecycle_state = LifecycleState::Resolved;
                        self.resolved_at_ms = Some(observation.observed_at_ms);
                        self.recovering_at_ms = None;
                        self.next_state_evaluation_at_ms = None;
                        return StateTransition::Resolved;
                    }
                    return StateTransition::Recovering;
                }
                if self.lifecycle_state == LifecycleState::Recovering
                    && recovery_satisfied(
                        recovery_mode,
                        self.consecutive_healthy_count,
                        self.healthy_since_ms,
                        observation.observed_at_ms,
                        recovery_count,
                        recovery_duration_seconds,
                    )
                {
                    self.lifecycle_state = LifecycleState::Resolved;
                    self.resolved_at_ms = Some(observation.observed_at_ms);
                    self.recovering_at_ms = None;
                    return StateTransition::Resolved;
                }
                if self.lifecycle_state == LifecycleState::Recovering
                    && recovery_mode == RecoveryMode::HealthyDuration
                {
                    self.next_state_evaluation_at_ms = self
                        .healthy_since_ms
                        .zip(recovery_duration_seconds)
                        .map(|(since, duration)| {
                            since.saturating_add(duration.saturating_mul(1000) as i64)
                        });
                }
                StateTransition::None
            }
            ObservationKind::Change => StateTransition::None,
        }
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.incident-deadline; owner=models/alerting; remove_when=scheduled projector is retired"
    )]
    pub fn evaluate_due(
        &mut self,
        now_ms: i64,
        trigger_mode: TriggerMode,
        trigger_duration_seconds: Option<u64>,
        recovery_mode: RecoveryMode,
        recovery_duration_seconds: Option<u64>,
    ) -> StateTransition {
        let Some(deadline) = self.next_state_evaluation_at_ms else {
            return StateTransition::None;
        };
        if now_ms < deadline || self.fact_fresh_until_ms.is_some_and(|until| now_ms > until) {
            return StateTransition::None;
        }
        match self.lifecycle_state {
            LifecycleState::Pending if trigger_mode == TriggerMode::ActiveDuration => {
                if self.pending_since_ms.is_some()
                    && trigger_duration_seconds.is_some()
                    && self.next_state_evaluation_at_ms == Some(deadline)
                {
                    self.lifecycle_state = LifecycleState::Open;
                    self.opened_at_ms = Some(now_ms);
                    self.next_state_evaluation_at_ms = None;
                    return StateTransition::Opened;
                }
            }
            LifecycleState::Recovering if recovery_mode == RecoveryMode::HealthyDuration => {
                if self.healthy_since_ms.is_some()
                    && recovery_duration_seconds.is_some()
                    && self.next_state_evaluation_at_ms == Some(deadline)
                {
                    self.lifecycle_state = LifecycleState::Resolved;
                    self.resolved_at_ms = Some(now_ms);
                    self.recovering_at_ms = None;
                    self.next_state_evaluation_at_ms = None;
                    return StateTransition::Resolved;
                }
            }
            _ => {}
        }
        StateTransition::None
    }

    fn maybe_open(
        &mut self,
        observed_at_ms: i64,
        mode: TriggerMode,
        count: Option<u32>,
        duration_seconds: Option<u64>,
    ) -> StateTransition {
        let satisfied = match mode {
            TriggerMode::Immediate => true,
            TriggerMode::ConsecutiveOccurrences => {
                self.consecutive_abnormal_count >= count.unwrap_or(1)
            }
            TriggerMode::ActiveDuration => {
                self.next_state_evaluation_at_ms =
                    self.pending_since_ms
                        .zip(duration_seconds)
                        .map(|(since, duration)| {
                            since.saturating_add(duration.saturating_mul(1000) as i64)
                        });
                false
            }
        };
        if satisfied {
            self.lifecycle_state = LifecycleState::Open;
            self.opened_at_ms = Some(observed_at_ms);
            self.pending_since_ms = None;
            self.next_state_evaluation_at_ms = None;
            return StateTransition::Opened;
        }
        StateTransition::Pending
    }
}

fn recovery_satisfied(
    mode: RecoveryMode,
    healthy_count: u32,
    healthy_since_ms: Option<i64>,
    observed_at_ms: i64,
    count: Option<u32>,
    duration_seconds: Option<u64>,
) -> bool {
    match mode {
        RecoveryMode::ConsecutiveHealthy => healthy_count >= count.unwrap_or(1),
        RecoveryMode::HealthyDuration => {
            healthy_since_ms
                .zip(duration_seconds)
                .is_some_and(|(since, duration)| {
                    observed_at_ms.saturating_sub(since) >= duration.saturating_mul(1000) as i64
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(kind: ObservationKind, at: i64) -> IncidentObservation {
        IncidentObservation {
            source_observation_key: format!("obs-{at}"),
            event_type: AlertEventType::StationDown,
            condition_key: ConditionKey::new("station:1").unwrap(),
            kind,
            severity: Severity::Critical,
            observed_at_ms: at,
            fact_fresh_until_ms: at + 10_000,
            summary_json: "{}".to_string(),
        }
    }

    #[test]
    fn immediate_incident_recovers_and_reopens_new_episode() {
        let mut incident = Incident::new(
            "incident-1",
            ConditionKey::new("station:1").unwrap(),
            AlertEventType::StationDown,
            Severity::Critical,
            0,
            "p1",
        );
        assert_eq!(
            incident.apply_observation(
                &observation(ObservationKind::Abnormal, 1),
                TriggerMode::Immediate,
                None,
                None,
                RecoveryMode::ConsecutiveHealthy,
                Some(2),
                None,
            ),
            StateTransition::Opened
        );
        assert_eq!(
            incident.apply_observation(
                &observation(ObservationKind::Healthy, 2),
                TriggerMode::Immediate,
                None,
                None,
                RecoveryMode::ConsecutiveHealthy,
                Some(2),
                None,
            ),
            StateTransition::Recovering
        );
        assert_eq!(
            incident.apply_observation(
                &observation(ObservationKind::Healthy, 3),
                TriggerMode::Immediate,
                None,
                None,
                RecoveryMode::ConsecutiveHealthy,
                Some(2),
                None,
            ),
            StateTransition::Resolved
        );
        assert_eq!(
            incident.apply_observation(
                &observation(ObservationKind::Abnormal, 4),
                TriggerMode::Immediate,
                None,
                None,
                RecoveryMode::ConsecutiveHealthy,
                Some(2),
                None,
            ),
            StateTransition::Opened
        );
        assert_eq!(incident.episode_number, 2);
    }

    #[test]
    fn one_healthy_observation_resolves_when_recovery_count_is_one() {
        let mut incident = Incident::new(
            "incident-1",
            ConditionKey::new("station:1").unwrap(),
            AlertEventType::StationDown,
            Severity::Critical,
            0,
            "p1",
        );
        assert_eq!(
            incident.apply_observation(
                &observation(ObservationKind::Abnormal, 1),
                TriggerMode::Immediate,
                None,
                None,
                RecoveryMode::ConsecutiveHealthy,
                Some(1),
                None,
            ),
            StateTransition::Opened
        );
        assert_eq!(
            incident.apply_observation(
                &observation(ObservationKind::Healthy, 2),
                TriggerMode::Immediate,
                None,
                None,
                RecoveryMode::ConsecutiveHealthy,
                Some(1),
                None,
            ),
            StateTransition::Resolved
        );
        assert_eq!(incident.lifecycle_state, LifecycleState::Resolved);
    }

    #[test]
    fn duration_trigger_does_not_open_stale_fact() {
        let mut incident = Incident::new(
            "incident-1",
            ConditionKey::new("station:1").unwrap(),
            AlertEventType::StationDown,
            Severity::Critical,
            0,
            "p1",
        );
        incident.apply_observation(
            &observation(ObservationKind::Abnormal, 1),
            TriggerMode::ActiveDuration,
            None,
            Some(2),
            RecoveryMode::ConsecutiveHealthy,
            None,
            None,
        );
        assert_eq!(
            incident.evaluate_due(
                12_000,
                TriggerMode::ActiveDuration,
                Some(2),
                RecoveryMode::ConsecutiveHealthy,
                None,
            ),
            StateTransition::None
        );
    }

    #[test]
    fn healthy_duration_uses_persistent_deadline_and_freshness() {
        let mut incident = Incident::new(
            "incident-1",
            ConditionKey::new("station:1").unwrap(),
            AlertEventType::StationDown,
            Severity::Critical,
            0,
            "p1",
        );
        incident.apply_observation(
            &observation(ObservationKind::Abnormal, 1),
            TriggerMode::Immediate,
            None,
            None,
            RecoveryMode::HealthyDuration,
            None,
            Some(2),
        );
        assert_eq!(
            incident.apply_observation(
                &observation(ObservationKind::Healthy, 2),
                TriggerMode::Immediate,
                None,
                None,
                RecoveryMode::HealthyDuration,
                None,
                Some(2),
            ),
            StateTransition::Recovering
        );
        assert_eq!(
            incident.evaluate_due(
                2_002,
                TriggerMode::Immediate,
                None,
                RecoveryMode::HealthyDuration,
                Some(2),
            ),
            StateTransition::Resolved
        );
    }

    #[test]
    fn stale_observation_is_ignored_without_advancing_version() {
        let mut incident = Incident::new(
            "incident-1",
            ConditionKey::new("station:1").unwrap(),
            AlertEventType::StationDown,
            Severity::Critical,
            0,
            "p1",
        );
        let mut stale = observation(ObservationKind::Abnormal, 1);
        stale.fact_fresh_until_ms = 0;
        assert_eq!(
            incident.apply_observation(
                &stale,
                TriggerMode::Immediate,
                None,
                None,
                RecoveryMode::ConsecutiveHealthy,
                Some(1),
                None,
            ),
            StateTransition::None
        );
        assert_eq!(incident.version, 0);
    }
}
