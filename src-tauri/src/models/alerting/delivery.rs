use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannel {
    InApp,
    Desktop,
}

impl NotificationChannel {
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "in_app" => Self::InApp,
            "desktop" => Self::Desktop,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::InApp => "in_app",
            Self::Desktop => "desktop",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryKind {
    Opened,
    Repeated,
    Escalated,
    Recovered,
    Test,
}

impl DeliveryKind {
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "opened" => Self::Opened,
            "repeated" => Self::Repeated,
            "escalated" => Self::Escalated,
            "recovered" => Self::Recovered,
            "test" => Self::Test,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Opened => "opened",
            Self::Repeated => "repeated",
            Self::Escalated => "escalated",
            Self::Recovered => "recovered",
            Self::Test => "test",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Scheduled,
    Claimed,
    Delivered,
    Suppressed,
    Failed,
    OutcomeUnknown,
}

impl DeliveryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Claimed => "claimed",
            Self::Delivered => "delivered",
            Self::Suppressed => "suppressed",
            Self::Failed => "failed",
            Self::OutcomeUnknown => "outcome_unknown",
        }
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.delivery-status-parser; owner=models/alerting; remove_when=recovery and import adapters are retired"
    )]
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "scheduled" => Self::Scheduled,
            "claimed" => Self::Claimed,
            "delivered" => Self::Delivered,
            "suppressed" => Self::Suppressed,
            "failed" => Self::Failed,
            "outcome_unknown" => Self::OutcomeUnknown,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionReason {
    GlobalDisabled,
    ChannelDisabled,
    PermissionDenied,
    QuietHours,
    GlobalPause,
    IncidentSnoozed,
    Cooldown,
    RepeatDisabled,
    PolicyMuted,
    StaleEpisode,
}

impl SuppressionReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GlobalDisabled => "global_disabled",
            Self::ChannelDisabled => "channel_disabled",
            Self::PermissionDenied => "permission_denied",
            Self::QuietHours => "quiet_hours",
            Self::GlobalPause => "global_pause",
            Self::IncidentSnoozed => "incident_snoozed",
            Self::Cooldown => "cooldown",
            Self::RepeatDisabled => "repeat_disabled",
            Self::PolicyMuted => "policy_muted",
            Self::StaleEpisode => "stale_episode",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationDelivery {
    pub id: String,
    pub delivery_key: String,
    pub incident_id: String,
    pub episode_number: u32,
    pub delivery_sequence: u64,
    pub policy_id: Option<String>,
    pub policy_revision: Option<u64>,
    pub policy_snapshot_json: String,
    pub channel: NotificationChannel,
    pub delivery_kind: DeliveryKind,
    pub status: DeliveryStatus,
    pub scheduled_at_ms: i64,
    pub claim_token: Option<String>,
    pub claimed_at_ms: Option<i64>,
    pub lease_expires_at_ms: Option<i64>,
    pub attempt_count: u32,
    pub attempted_at_ms: Option<i64>,
    pub outcome_unknown_at_ms: Option<i64>,
    pub retry_not_before_ms: Option<i64>,
    pub delivered_at_ms: Option<i64>,
    pub suppressed_reason: Option<SuppressionReason>,
    pub error_code: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl NotificationDelivery {
    pub fn new(
        id: impl Into<String>,
        incident_id: impl Into<String>,
        episode_number: u32,
        delivery_sequence: u64,
        channel: NotificationChannel,
        delivery_kind: DeliveryKind,
        scheduled_at_ms: i64,
        policy_snapshot_json: String,
    ) -> Result<Self, String> {
        if episode_number == 0 || delivery_sequence == 0 {
            return Err("episode_number and delivery_sequence must be positive".to_string());
        }
        if serde_json::from_str::<serde_json::Value>(&policy_snapshot_json).is_err() {
            return Err("policy_snapshot_json must contain valid JSON".to_string());
        }
        let incident_id = incident_id.into();
        let delivery_key = make_delivery_key(
            &incident_id,
            episode_number,
            channel,
            delivery_kind,
            delivery_sequence,
        );
        Ok(Self {
            id: id.into(),
            delivery_key,
            incident_id,
            episode_number,
            delivery_sequence,
            policy_id: None,
            policy_revision: None,
            policy_snapshot_json,
            channel,
            delivery_kind,
            status: DeliveryStatus::Scheduled,
            scheduled_at_ms,
            claim_token: None,
            claimed_at_ms: None,
            lease_expires_at_ms: None,
            attempt_count: 0,
            attempted_at_ms: None,
            outcome_unknown_at_ms: None,
            retry_not_before_ms: None,
            delivered_at_ms: None,
            suppressed_reason: None,
            error_code: None,
            created_at_ms: scheduled_at_ms,
            updated_at_ms: scheduled_at_ms,
        })
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.delivery-claim-transition; owner=models/alerting; remove_when=delivery claim state is persisted without a domain transition helper"
    )]
    pub fn claim(
        &mut self,
        token: impl Into<String>,
        now_ms: i64,
        lease_ms: i64,
    ) -> Result<(), String> {
        if !matches!(
            self.status,
            DeliveryStatus::Scheduled | DeliveryStatus::OutcomeUnknown
        ) {
            return Err("only scheduled or outcome_unknown delivery can be claimed".to_string());
        }
        if lease_ms <= 0 {
            return Err("lease_ms must be positive".to_string());
        }
        if self.status == DeliveryStatus::Scheduled && self.scheduled_at_ms > now_ms {
            return Err("delivery is not due yet".to_string());
        }
        if self.status == DeliveryStatus::OutcomeUnknown
            && self.retry_not_before_ms.is_some_and(|due| due > now_ms)
        {
            return Err("delivery retry is not due yet".to_string());
        }
        self.claim_token = Some(token.into());
        self.claimed_at_ms = Some(now_ms);
        self.lease_expires_at_ms = Some(now_ms.saturating_add(lease_ms));
        self.attempt_count = self.attempt_count.saturating_add(1);
        self.attempted_at_ms = Some(now_ms);
        self.retry_not_before_ms = None;
        self.status = DeliveryStatus::Claimed;
        self.updated_at_ms = now_ms;
        Ok(())
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.delivery-mark-delivered; owner=models/alerting; remove_when=delivery completion is represented only by persistence state"
    )]
    pub fn mark_delivered(&mut self, token: &str, now_ms: i64) -> Result<(), String> {
        self.require_claim(token)?;
        self.status = DeliveryStatus::Delivered;
        self.delivered_at_ms = Some(now_ms);
        self.claim_token = None;
        self.lease_expires_at_ms = None;
        self.updated_at_ms = now_ms;
        Ok(())
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.delivery-mark-failed; owner=models/alerting; remove_when=delivery failure transitions are represented only by the persistence worker"
    )]
    pub fn mark_failed(
        &mut self,
        token: &str,
        error_code: impl Into<String>,
        now_ms: i64,
    ) -> Result<(), String> {
        self.require_claim(token)?;
        self.status = DeliveryStatus::Failed;
        self.error_code = Some(error_code.into());
        self.claim_token = None;
        self.lease_expires_at_ms = None;
        self.updated_at_ms = now_ms;
        Ok(())
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.delivery-expire-claim; owner=models/alerting; remove_when=claim lease expiry is represented only by the persistence worker"
    )]
    pub fn expire_claim(&mut self, now_ms: i64, retry_not_before_ms: Option<i64>) -> bool {
        if self.status != DeliveryStatus::Claimed
            || self
                .lease_expires_at_ms
                .is_none_or(|expires| now_ms < expires)
        {
            return false;
        }
        self.status = DeliveryStatus::OutcomeUnknown;
        self.outcome_unknown_at_ms = Some(now_ms);
        self.retry_not_before_ms = retry_not_before_ms;
        self.updated_at_ms = now_ms;
        true
    }

    pub fn suppress(&mut self, reason: SuppressionReason, now_ms: i64) -> Result<(), String> {
        if self.status != DeliveryStatus::Scheduled {
            return Err("only scheduled delivery can be suppressed".to_string());
        }
        self.status = DeliveryStatus::Suppressed;
        self.suppressed_reason = Some(reason);
        self.updated_at_ms = now_ms;
        Ok(())
    }

    fn require_claim(&self, token: &str) -> Result<(), String> {
        if self.status != DeliveryStatus::Claimed || self.claim_token.as_deref() != Some(token) {
            return Err("delivery claim token does not own this delivery".to_string());
        }
        Ok(())
    }
}

pub fn make_delivery_key(
    incident_id: &str,
    episode_number: u32,
    channel: NotificationChannel,
    delivery_kind: DeliveryKind,
    delivery_sequence: u64,
) -> String {
    format!(
        "{incident_id}:{episode_number}:{}:{}:{delivery_sequence}",
        channel.as_str(),
        delivery_kind.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_requires_same_token_and_expiry_is_outcome_unknown() {
        let mut delivery = NotificationDelivery::new(
            "delivery-1",
            "incident-1",
            1,
            1,
            NotificationChannel::Desktop,
            DeliveryKind::Opened,
            10,
            "{}".to_string(),
        )
        .unwrap();
        delivery.claim("token-1", 10, 100).unwrap();
        assert!(delivery.mark_delivered("wrong", 20).is_err());
        assert!(delivery.expire_claim(111, Some(200)));
        assert_eq!(delivery.status, DeliveryStatus::OutcomeUnknown);
        delivery.claim("token-2", 201, 100).unwrap();
        delivery.mark_delivered("token-2", 202).unwrap();
        assert_eq!(delivery.status, DeliveryStatus::Delivered);
    }

    #[test]
    fn claim_does_not_bypass_scheduled_or_retry_deadlines() {
        let mut scheduled = NotificationDelivery::new(
            "delivery-2",
            "incident-1",
            1,
            1,
            NotificationChannel::Desktop,
            DeliveryKind::Repeated,
            100,
            "{}".to_string(),
        )
        .unwrap();
        assert!(scheduled.claim("token", 99, 10).is_err());
        scheduled.claim("token", 100, 10).unwrap();
        assert!(scheduled.expire_claim(111, Some(200)));
        assert!(scheduled.claim("retry", 199, 10).is_err());
        scheduled.claim("retry", 200, 10).unwrap();
    }
}
