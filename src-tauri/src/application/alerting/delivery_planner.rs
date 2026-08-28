use sha2::{Digest, Sha256};

use crate::models::alerting::{
    AlertPolicy, DeliveryKind, Incident, IncidentAttention, NotificationChannel,
    NotificationDelivery, Severity, StateTransition, SuppressionReason,
};

use super::policy_service::AlertingSettings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedDelivery {
    pub delivery: NotificationDelivery,
    pub suppression_reason: Option<SuppressionReason>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DeliveryPlanner;

impl DeliveryPlanner {
    pub(crate) fn plan_transition(
        &self,
        transition: StateTransition,
        incident: &Incident,
        policy: &AlertPolicy,
        settings: &AlertingSettings,
        attention: Option<&IncidentAttention>,
        now_ms: i64,
    ) -> Vec<PlannedDelivery> {
        let kind = match transition {
            StateTransition::Opened | StateTransition::Reopened => DeliveryKind::Opened,
            StateTransition::Resolved => {
                if !policy.recovery_notification_enabled {
                    return Vec::new();
                }
                DeliveryKind::Recovered
            }
            StateTransition::None | StateTransition::Pending | StateTransition::Recovering => {
                return Vec::new()
            }
        };
        self.plan_kind(kind, incident, policy, settings, attention, now_ms, 1)
    }

    fn plan_kind(
        &self,
        kind: DeliveryKind,
        incident: &Incident,
        policy: &AlertPolicy,
        settings: &AlertingSettings,
        attention: Option<&IncidentAttention>,
        now_ms: i64,
        sequence: u64,
    ) -> Vec<PlannedDelivery> {
        let snapshot = serde_json::to_string(policy).unwrap_or_else(|_| "{}".to_string());
        [NotificationChannel::InApp, NotificationChannel::Desktop]
            .into_iter()
            .map(|channel| {
                let suppression_reason = suppression_reason(
                    incident,
                    policy,
                    settings,
                    attention,
                    channel,
                    now_ms,
                    self.channel_is_policy_enabled(policy, channel),
                );
                let delivery = make_delivery(
                    incident,
                    policy,
                    channel,
                    kind,
                    sequence,
                    now_ms,
                    snapshot.clone(),
                );
                PlannedDelivery {
                    delivery,
                    suppression_reason,
                }
            })
            .collect()
    }

    fn channel_is_policy_enabled(
        &self,
        policy: &AlertPolicy,
        channel: NotificationChannel,
    ) -> bool {
        match channel {
            NotificationChannel::InApp => policy.in_app_enabled,
            NotificationChannel::Desktop => policy.desktop_enabled,
        }
    }
}

fn suppression_reason(
    incident: &Incident,
    policy: &AlertPolicy,
    settings: &AlertingSettings,
    attention: Option<&IncidentAttention>,
    channel: NotificationChannel,
    now_ms: i64,
    policy_channel_enabled: bool,
) -> Option<SuppressionReason> {
    if !policy.enabled || policy.state != crate::models::alerting::PolicyState::Active {
        return Some(SuppressionReason::PolicyMuted);
    }
    if !settings.alerting_enabled {
        return Some(SuppressionReason::GlobalDisabled);
    }
    if !policy_channel_enabled {
        return Some(SuppressionReason::ChannelDisabled);
    }
    let global_channel_enabled = match channel {
        NotificationChannel::InApp => settings.in_app_enabled,
        NotificationChannel::Desktop => settings.desktop_enabled,
    };
    if !global_channel_enabled {
        return Some(SuppressionReason::ChannelDisabled);
    }
    if settings.is_paused_at(now_ms) {
        return Some(SuppressionReason::GlobalPause);
    }
    if attention.is_some_and(|value| value.is_snoozed_at(now_ms)) {
        return Some(SuppressionReason::IncidentSnoozed);
    }
    let quiet_allowed = match policy.quiet_hours_policy {
        crate::models::alerting::QuietHoursPolicy::Respect => true,
        crate::models::alerting::QuietHoursPolicy::BypassForCritical => {
            incident.severity != Severity::Critical
        }
        crate::models::alerting::QuietHoursPolicy::Inherit => {
            !(incident.severity == Severity::Critical && settings.critical_bypasses_quiet_hours)
        }
    };
    if quiet_allowed && settings.is_quiet_at(now_ms) {
        return Some(SuppressionReason::QuietHours);
    }
    None
}

fn make_delivery(
    incident: &Incident,
    policy: &AlertPolicy,
    channel: NotificationChannel,
    kind: DeliveryKind,
    sequence: u64,
    now_ms: i64,
    snapshot: String,
) -> NotificationDelivery {
    let key = crate::models::alerting::make_delivery_key(
        &incident.id,
        incident.episode_number,
        channel,
        kind,
        sequence,
    );
    let id = format!("delivery-{}", short_hash(&key));
    let mut delivery = NotificationDelivery::new(
        id,
        incident.id.clone(),
        incident.episode_number,
        sequence,
        channel,
        kind,
        now_ms,
        snapshot,
    )
    .expect("planner emits valid delivery values");
    // The system default is a virtual profile. Its full snapshot is retained
    // for auditability, while the nullable FK only points at stored policies.
    if policy.id != "system_default" {
        delivery.policy_id = Some(policy.id.clone());
        delivery.policy_revision = Some(policy.revision);
    }
    delivery
}

fn short_hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::alerting::{AlertEventType, ConditionKey, LifecycleState};

    fn incident() -> Incident {
        let mut incident = Incident::new(
            "incident-1",
            ConditionKey::new("station:1").unwrap(),
            AlertEventType::StationDown,
            Severity::Critical,
            0,
            "fingerprint",
        );
        incident.lifecycle_state = LifecycleState::Open;
        incident
    }

    #[test]
    fn opened_transition_is_audited_even_when_channel_is_disabled() {
        let mut settings = AlertingSettings::default();
        settings.desktop_enabled = false;
        let mut policy = AlertPolicy::system_default(Severity::Critical);
        policy.desktop_enabled = true;
        let plans = DeliveryPlanner.plan_transition(
            StateTransition::Opened,
            &incident(),
            &policy,
            &settings,
            None,
            0,
        );
        assert_eq!(plans.len(), 2);
        assert!(plans.iter().any(|plan| {
            plan.delivery.channel == NotificationChannel::Desktop
                && plan.suppression_reason == Some(SuppressionReason::ChannelDisabled)
        }));
    }

    #[test]
    fn critical_default_can_bypass_quiet_hours() {
        let settings = AlertingSettings {
            quiet_hours_enabled: true,
            quiet_hours_start_local: Some("00:00".into()),
            quiet_hours_end_local: Some("23:59".into()),
            ..Default::default()
        };
        let mut policy = AlertPolicy::system_default(Severity::Critical);
        policy.desktop_enabled = true;
        let plans = DeliveryPlanner.plan_transition(
            StateTransition::Opened,
            &incident(),
            &policy,
            &settings,
            None,
            0,
        );
        assert!(plans.iter().any(|plan| {
            plan.delivery.channel == NotificationChannel::InApp && plan.suppression_reason.is_none()
        }));
        assert!(plans.iter().any(|plan| {
            plan.delivery.channel == NotificationChannel::Desktop
                && plan.suppression_reason == Some(SuppressionReason::ChannelDisabled)
        }));
    }
}
