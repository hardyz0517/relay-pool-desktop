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

    pub fn suppress(&mut self, reason: SuppressionReason, now_ms: i64) -> Result<(), String> {
        if self.status != DeliveryStatus::Scheduled {
            return Err("only scheduled delivery can be suppressed".to_string());
        }
        self.status = DeliveryStatus::Suppressed;
        self.suppressed_reason = Some(reason);
        self.updated_at_ms = now_ms;
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
