use serde::{Deserialize, Serialize};

use crate::application::alerting::policy_service::AlertingSettings;
use crate::application::queries::change_center_workspace::{
    DeliveryCursor, DeliveryHistoryPage, DeliverySummary, IncidentCursor, IncidentSummary,
    IncidentWorkspacePage, OccurrenceCursor, OccurrenceHistoryPage, OccurrenceSummary,
};
use crate::models::alerting::{
    AlertEventType, AlertPolicy, PolicyState, QuietHoursPolicy, RecoveryMode, RepeatMode,
    ScopeKind, Severity, TriggerMode,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertPolicyDto {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub state: PolicyState,
    pub scope_kind: ScopeKind,
    pub event_type: Option<AlertEventType>,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub minimum_severity: Option<Severity>,
    pub severity_offset: i8,
    pub trigger_mode: TriggerMode,
    pub trigger_count: Option<u32>,
    pub trigger_duration_seconds: Option<u64>,
    pub recovery_mode: RecoveryMode,
    pub recovery_count: Option<u32>,
    pub recovery_duration_seconds: Option<u64>,
    pub in_app_enabled: bool,
    pub desktop_enabled: bool,
    pub repeat_mode: RepeatMode,
    pub repeat_interval_seconds: Option<u64>,
    pub cooldown_seconds: u64,
    pub recovery_notification_enabled: bool,
    pub quiet_hours_policy: QuietHoursPolicy,
    pub priority: u32,
    pub revision: u64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertPolicyInputDto {
    pub id: Option<String>,
    pub name: String,
    pub enabled: bool,
    pub state: PolicyState,
    pub scope_kind: ScopeKind,
    pub event_type: Option<AlertEventType>,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub minimum_severity: Option<Severity>,
    pub severity_offset: i8,
    pub trigger_mode: TriggerMode,
    pub trigger_count: Option<u32>,
    pub trigger_duration_seconds: Option<u64>,
    pub recovery_mode: RecoveryMode,
    pub recovery_count: Option<u32>,
    pub recovery_duration_seconds: Option<u64>,
    pub in_app_enabled: bool,
    pub desktop_enabled: bool,
    pub repeat_mode: RepeatMode,
    pub repeat_interval_seconds: Option<u64>,
    pub cooldown_seconds: u64,
    pub recovery_notification_enabled: bool,
    pub quiet_hours_policy: QuietHoursPolicy,
    pub priority: u32,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertPolicyDeleteInputDto {
    pub id: String,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertingSettingsDto {
    #[serde(rename = "enabled")]
    pub alerting_enabled: bool,
    pub in_app_enabled: bool,
    pub desktop_enabled: bool,
    pub paused: bool,
    pub global_pause_until_ms: Option<i64>,
    pub quiet_hours_enabled: bool,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
    pub quiet_hours_timezone: String,
    pub critical_bypasses_quiet_hours: bool,
    pub history_retention_days: u32,
    pub delivery_retention_days: u32,
    pub revision: u64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertingSettingsInputDto {
    #[serde(rename = "enabled")]
    pub alerting_enabled: bool,
    pub in_app_enabled: bool,
    pub desktop_enabled: bool,
    pub paused: bool,
    pub global_pause_until_ms: Option<i64>,
    pub quiet_hours_enabled: bool,
    pub quiet_hours_start: Option<String>,
    pub quiet_hours_end: Option<String>,
    pub quiet_hours_timezone: String,
    pub critical_bypasses_quiet_hours: bool,
    pub history_retention_days: u32,
    pub delivery_retention_days: u32,
    #[serde(default)]
    pub expected_revision: Option<u64>,
}

impl From<AlertPolicy> for AlertPolicyDto {
    fn from(value: AlertPolicy) -> Self {
        Self {
            id: value.id,
            name: value.name,
            enabled: value.enabled,
            state: value.state,
            scope_kind: value.scope_kind,
            event_type: value.event_type,
            station_id: value.station_id,
            station_key_id: value.station_key_id,
            minimum_severity: value.minimum_severity,
            severity_offset: value.severity_offset,
            trigger_mode: value.trigger_mode,
            trigger_count: value.trigger_count,
            trigger_duration_seconds: value.trigger_duration_seconds,
            recovery_mode: value.recovery_mode,
            recovery_count: value.recovery_count,
            recovery_duration_seconds: value.recovery_duration_seconds,
            in_app_enabled: value.in_app_enabled,
            desktop_enabled: value.desktop_enabled,
            repeat_mode: value.repeat_mode,
            repeat_interval_seconds: value.repeat_interval_seconds,
            cooldown_seconds: value.cooldown_seconds,
            recovery_notification_enabled: value.recovery_notification_enabled,
            quiet_hours_policy: value.quiet_hours_policy,
            priority: value.priority,
            revision: value.revision,
            created_at_ms: value.created_at_ms,
            updated_at_ms: value.updated_at_ms,
        }
    }
}

impl AlertPolicyInputDto {
    pub(crate) fn parse(
        value: serde_json::Value,
    ) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| invalid())
    }

    pub(crate) fn into_domain(self, now_ms: i64) -> Result<(AlertPolicy, Option<u64>), String> {
        let id = self
            .id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("policy-{}", uuid::Uuid::now_v7()));
        let expected_revision = self.expected_revision;
        let policy = AlertPolicy {
            id,
            name: self.name,
            enabled: self.enabled,
            state: self.state,
            scope_kind: self.scope_kind,
            event_type: self.event_type,
            station_id: self.station_id,
            station_key_id: self.station_key_id,
            minimum_severity: self.minimum_severity,
            severity_offset: self.severity_offset,
            trigger_mode: self.trigger_mode,
            trigger_count: self.trigger_count,
            trigger_duration_seconds: self.trigger_duration_seconds,
            recovery_mode: self.recovery_mode,
            recovery_count: self.recovery_count,
            recovery_duration_seconds: self.recovery_duration_seconds,
            in_app_enabled: self.in_app_enabled,
            desktop_enabled: self.desktop_enabled,
            repeat_mode: self.repeat_mode,
            repeat_interval_seconds: self.repeat_interval_seconds,
            cooldown_seconds: self.cooldown_seconds,
            recovery_notification_enabled: self.recovery_notification_enabled,
            quiet_hours_policy: self.quiet_hours_policy,
            priority: self.priority,
            revision: expected_revision.map_or(1, |value| value.saturating_add(1)),
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        };
        policy.validate()?;
        Ok((policy, expected_revision))
    }
}

impl AlertPolicyDeleteInputDto {
    pub(crate) fn parse(
        value: serde_json::Value,
    ) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| invalid())
    }
}

impl From<AlertingSettings> for AlertingSettingsDto {
    fn from(value: AlertingSettings) -> Self {
        Self {
            alerting_enabled: value.alerting_enabled,
            in_app_enabled: value.in_app_enabled,
            desktop_enabled: value.desktop_enabled,
            paused: value.paused,
            global_pause_until_ms: value.global_pause_until_ms,
            quiet_hours_enabled: value.quiet_hours_enabled,
            quiet_hours_start: value.quiet_hours_start_local,
            quiet_hours_end: value.quiet_hours_end_local,
            quiet_hours_timezone: value.quiet_hours_time_zone,
            critical_bypasses_quiet_hours: value.critical_bypasses_quiet_hours,
            history_retention_days: value.history_retention_days,
            delivery_retention_days: value.delivery_retention_days,
            revision: value.revision,
            updated_at_ms: value.updated_at_ms,
        }
    }
}

impl AlertingSettingsInputDto {
    pub(crate) fn parse(
        value: serde_json::Value,
    ) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| invalid())
    }

    pub(crate) fn into_domain(
        self,
        expected_revision: u64,
    ) -> Result<AlertingSettings, crate::persistence::error::PersistenceError> {
        Ok(AlertingSettings {
            revision: expected_revision.saturating_add(1),
            alerting_enabled: self.alerting_enabled,
            in_app_enabled: self.in_app_enabled,
            desktop_enabled: self.desktop_enabled,
            paused: self.paused,
            global_pause_until_ms: self.global_pause_until_ms,
            quiet_hours_enabled: self.quiet_hours_enabled,
            quiet_hours_start_local: self.quiet_hours_start,
            quiet_hours_end_local: self.quiet_hours_end,
            quiet_hours_time_zone: self.quiet_hours_timezone,
            critical_bypasses_quiet_hours: self.critical_bypasses_quiet_hours,
            history_retention_days: self.history_retention_days,
            delivery_retention_days: self.delivery_retention_days,
            updated_at_ms: 0,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertingCurrentInputDto {
    pub station_id: Option<String>,
    pub severity: Option<String>,
    pub lifecycle_state: Option<String>,
    pub cursor: Option<AlertingCursorDto>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertingCursorDto {
    pub updated_at_ms: i64,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertingIncidentInputDto {
    pub incident_id: String,
    pub episode_number: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertingHistoryInputDto {
    pub incident_id: String,
    pub episode_number: i64,
    pub cursor: Option<AlertingCursorDto>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertingAttentionInputDto {
    pub incident_id: String,
    pub episode_number: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertingMarkAllSeenInputDto {
    pub station_id: Option<String>,
    pub severity: Option<Severity>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AlertingClearScope {
    Active,
    Unread,
    Resolved,
}

impl AlertingClearScope {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Unread => "unread",
            Self::Resolved => "resolved",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertingClearInputDto {
    pub station_id: Option<String>,
    pub severity: Option<Severity>,
    pub lifecycle_state: Option<AlertingClearScope>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertingSnoozeInputDto {
    pub incident_id: String,
    pub episode_number: i64,
    pub until_ms: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertingNotificationTestInputDto {
    pub channel: String,
}

impl AlertingNotificationTestInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| invalid())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AlertingObservationInputDto {
    pub source_observation_key: String,
    pub event_type: String,
    pub condition_key: String,
    pub kind: String,
    pub severity: String,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AlertingIncidentSummaryDto {
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AlertingCursorOutputDto {
    pub updated_at_ms: i64,
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AlertingIncidentPageDto {
    pub items: Vec<AlertingIncidentSummaryDto>,
    pub next_cursor: Option<AlertingCursorOutputDto>,
    pub active_count: i64,
    pub unseen_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AlertingOccurrenceDto {
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AlertingDeliveryDto {
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AlertingOccurrencePageDto {
    pub items: Vec<AlertingOccurrenceDto>,
    pub next_cursor: Option<AlertingCursorOutputDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AlertingDeliveryPageDto {
    pub items: Vec<AlertingDeliveryDto>,
    pub next_cursor: Option<AlertingCursorOutputDto>,
}

impl AlertingCurrentInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| {
            crate::commands::error::CommandError::try_new(
                crate::commands::error::CommandErrorCode::InvalidInput,
                "The alerting query is invalid.",
                false,
                None,
                None,
            )
            .expect("bounded alerting validation error")
        })
    }
}

impl AlertingAttentionInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| invalid())
    }
}

impl AlertingMarkAllSeenInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| invalid())
    }
}

impl AlertingClearInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| invalid())
    }
}

impl AlertingSnoozeInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| invalid())
    }
}

impl AlertingObservationInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| invalid())
    }
}

impl AlertingIncidentInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| invalid())?;
        validate_incident_key(&input.incident_id, input.episode_number)?;
        Ok(input)
    }
}

impl AlertingHistoryInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| invalid())?;
        validate_incident_key(&input.incident_id, input.episode_number)?;
        if input.limit.is_some_and(|value| value == 0 || value > 200) {
            return Err(invalid());
        }
        Ok(input)
    }
}

fn validate_incident_key(
    incident_id: &str,
    episode_number: i64,
) -> Result<(), crate::commands::error::CommandError> {
    if incident_id.trim().is_empty()
        || incident_id.len() > 200
        || episode_number <= 0
        || episode_number > i64::from(u32::MAX)
    {
        return Err(invalid());
    }
    Ok(())
}

fn invalid() -> crate::commands::error::CommandError {
    crate::commands::error::CommandError::try_new(
        crate::commands::error::CommandErrorCode::InvalidInput,
        "The alerting input is invalid.",
        false,
        None,
        None,
    )
    .expect("bounded alerting validation error")
}

impl From<IncidentWorkspacePage> for AlertingIncidentPageDto {
    fn from(page: IncidentWorkspacePage) -> Self {
        Self {
            items: page.items.into_iter().map(Into::into).collect(),
            next_cursor: page.next_cursor.map(Into::into),
            active_count: page.active_count,
            unseen_count: page.unseen_count,
        }
    }
}

impl From<IncidentSummary> for AlertingIncidentSummaryDto {
    fn from(item: IncidentSummary) -> Self {
        Self {
            id: item.id,
            condition_key: item.condition_key,
            event_type: item.event_type,
            lifecycle_state: item.lifecycle_state,
            severity: item.severity,
            station_id: item.station_id,
            episode_number: item.episode_number,
            occurrence_count: item.occurrence_count,
            last_seen_at_ms: item.last_seen_at_ms,
            collector_failed_task_types: item.collector_failed_task_types,
            resolved_at_ms: item.resolved_at_ms,
            updated_at_ms: item.updated_at_ms,
            seen_at_ms: item.seen_at_ms,
            snoozed_until_ms: item.snoozed_until_ms,
        }
    }
}

impl From<IncidentCursor> for AlertingCursorOutputDto {
    fn from(cursor: IncidentCursor) -> Self {
        Self {
            updated_at_ms: cursor.updated_at_ms,
            id: cursor.id,
        }
    }
}

impl From<OccurrenceCursor> for AlertingCursorOutputDto {
    fn from(cursor: OccurrenceCursor) -> Self {
        Self {
            updated_at_ms: cursor.observed_at_ms,
            id: cursor.id,
        }
    }
}

impl From<DeliveryCursor> for AlertingCursorOutputDto {
    fn from(cursor: DeliveryCursor) -> Self {
        Self {
            updated_at_ms: cursor.created_at_ms,
            id: cursor.id,
        }
    }
}

impl From<OccurrenceSummary> for AlertingOccurrenceDto {
    fn from(item: OccurrenceSummary) -> Self {
        Self {
            id: item.id,
            source_observation_key: item.source_observation_key,
            event_type: item.event_type,
            observation_kind: item.observation_kind,
            severity: item.severity,
            reason_code: item.reason_code,
            source: item.source,
            object_type: item.object_type,
            object_id: item.object_id,
            station_id: item.station_id,
            station_key_id: item.station_key_id,
            observed_at_ms: item.observed_at_ms,
        }
    }
}

impl From<DeliverySummary> for AlertingDeliveryDto {
    fn from(item: DeliverySummary) -> Self {
        Self {
            id: item.id,
            delivery_key: item.delivery_key,
            channel: item.channel,
            delivery_kind: item.delivery_kind,
            status: item.status,
            scheduled_at_ms: item.scheduled_at_ms,
            attempt_count: item.attempt_count,
            delivered_at_ms: item.delivered_at_ms,
            suppressed_reason: item.suppressed_reason,
            error_code: item.error_code,
            created_at_ms: item.created_at_ms,
            updated_at_ms: item.updated_at_ms,
        }
    }
}

impl From<OccurrenceHistoryPage> for AlertingOccurrencePageDto {
    fn from(page: OccurrenceHistoryPage) -> Self {
        Self {
            items: page.items.into_iter().map(Into::into).collect(),
            next_cursor: page.next_cursor.map(Into::into),
        }
    }
}

impl From<DeliveryHistoryPage> for AlertingDeliveryPageDto {
    fn from(page: DeliveryHistoryPage) -> Self {
        Self {
            items: page.items.into_iter().map(Into::into).collect(),
            next_cursor: page.next_cursor.map(Into::into),
        }
    }
}

pub const ALERTING_TYPE: super::TypeDescriptor = super::TypeDescriptor {
    name: "AlertingIncidentPageDto",
    typescript: r#"export type AlertSeverity = "info" | "warning" | "critical";
export type AlertEventType =
  | "group_missing" | "key_group_unresolved" | "balance_low" | "balance_depleted"
  | "price_expired" | "key_invalid" | "collector_failed" | "station_down"
  | "route_impacted" | "group_added" | "rate_changed" | "group_rate_changed"
  | "price_changed" | "model_added" | "model_removed" | "audit_change";
export type AlertScope = "global" | "event_type" | "station" | "station_key";
export type AlertPolicyState = "active" | "disabled" | "orphaned" | "tombstone";
export type AlertTriggerMode = "immediate" | "consecutive_occurrences" | "active_duration";
export type AlertRecoveryMode = "consecutive_healthy" | "healthy_duration";
export type AlertRepeatMode = "never" | "interval" | "severity_escalation" | "interval_and_escalation";
export type AlertQuietHoursPolicy = "inherit" | "respect" | "bypass_for_critical";
export type AlertPolicyDto = {
  id: string; name: string; enabled: boolean; state: AlertPolicyState;
  scopeKind: AlertScope; eventType: AlertEventType | null;
  stationId: string | null; stationKeyId: string | null;
  minimumSeverity: AlertSeverity | null; severityOffset: -1 | 0 | 1;
  triggerMode: AlertTriggerMode; triggerCount: number | null;
  triggerDurationSeconds: number | null; recoveryMode: AlertRecoveryMode;
  recoveryCount: number | null; recoveryDurationSeconds: number | null;
  inAppEnabled: boolean; desktopEnabled: boolean; repeatMode: AlertRepeatMode;
  repeatIntervalSeconds: number | null; cooldownSeconds: number;
  recoveryNotificationEnabled: boolean; quietHoursPolicy: AlertQuietHoursPolicy;
  priority: number; revision: number; createdAtMs: number; updatedAtMs: number;
};

export type AlertPolicyInputDto = Omit<AlertPolicyDto, "revision" | "createdAtMs" | "updatedAtMs"> & {
  id?: string; expectedRevision?: number;
};

export type AlertPolicyDeleteInputDto = { id: string; expectedRevision: number };
export type AlertingSettingsDto = {
  enabled: boolean; inAppEnabled: boolean; desktopEnabled: boolean; paused: boolean;
  globalPauseUntilMs: number | null; quietHoursEnabled: boolean;
  quietHoursStart: string | null; quietHoursEnd: string | null;
  quietHoursTimezone: string; criticalBypassesQuietHours: boolean;
  historyRetentionDays: number; deliveryRetentionDays: number;
  revision: number; updatedAtMs: number;
};
export type AlertingSettingsInputDto = Omit<AlertingSettingsDto, "revision" | "updatedAtMs"> & {
  expectedRevision?: number;
};
export type AlertingCurrentInputDto = {
  stationId?: string | null; severity?: AlertSeverity | null;
  lifecycleState?: string | null; cursor?: AlertingCursorDto | null; limit?: number;
};
export type AlertingAttentionInputDto = { incidentId: string; episodeNumber: number };
export type AlertingMarkAllSeenInputDto = { stationId?: string | null; severity?: AlertSeverity | null };
export type AlertingClearScope = "active" | "unread" | "resolved";
export type AlertingClearInputDto = {
  stationId?: string | null; severity?: AlertSeverity | null;
  lifecycleState?: AlertingClearScope | null;
};
export type AlertingSnoozeInputDto = AlertingAttentionInputDto & { untilMs: number };
export type AlertingObservationInputDto = {
  sourceObservationKey: string; eventType: string; conditionKey: string;
  kind: string; severity: string; objectType: string; objectId?: string | null;
  stationId?: string | null; stationKeyId?: string | null; source: string;
  reasonCode?: string | null; summaryJson: string; observedAtMs: number;
  factFreshUntilMs: number;
};
export type AlertingIncidentSummaryDto = {
  id: string; conditionKey: string; eventType: string; lifecycleState: string;
  severity: string; stationId: string | null; episodeNumber: number;
  occurrenceCount: number; lastSeenAtMs: number; collectorFailedTaskTypes: string[];
  resolvedAtMs: number | null;
  updatedAtMs: number; seenAtMs: number | null; snoozedUntilMs: number | null;
};
export type AlertingCursorDto = { updatedAtMs: number; id: string };
export type AlertingIncidentPageDto = {
  items: AlertingIncidentSummaryDto[]; nextCursor: AlertingCursorDto | null;
  activeCount: number; unseenCount: number;
};
export type AlertingIncidentInputDto = { incidentId: string; episodeNumber: number };
export type AlertingHistoryInputDto = AlertingIncidentInputDto & { cursor?: AlertingCursorDto | null; limit?: number };
export type AlertingOccurrenceDto = {
  id: string; sourceObservationKey: string; eventType: string; observationKind: string;
  severity: string; reasonCode: string | null; source: string; objectType: string;
  objectId: string | null; stationId: string | null; stationKeyId: string | null; observedAtMs: number;
};
export type AlertingDeliveryDto = {
  id: string; deliveryKey: string; channel: string; deliveryKind: string; status: string;
  scheduledAtMs: number; attemptCount: number; deliveredAtMs: number | null;
  suppressedReason: string | null; errorCode: string | null; createdAtMs: number; updatedAtMs: number;
};
export type AlertingOccurrencePageDto = { items: AlertingOccurrenceDto[]; nextCursor: AlertingCursorDto | null };
export type AlertingDeliveryPageDto = { items: AlertingDeliveryDto[]; nextCursor: AlertingCursorDto | null };"#,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_inputs_reject_unbounded_or_invalid_keys() {
        assert!(AlertingIncidentInputDto::parse(serde_json::json!({
            "incidentId": "", "episodeNumber": 1
        }))
        .is_err());
        assert!(AlertingHistoryInputDto::parse(serde_json::json!({
            "incidentId": "incident-1", "episodeNumber": 0
        }))
        .is_err());
        assert!(AlertingHistoryInputDto::parse(serde_json::json!({
            "incidentId": "incident-1", "episodeNumber": 1, "limit": 201
        }))
        .is_err());
        assert!(AlertingHistoryInputDto::parse(serde_json::json!({
            "incidentId": "incident-1", "episodeNumber": 1, "limit": 20
        }))
        .is_ok());
    }
}
