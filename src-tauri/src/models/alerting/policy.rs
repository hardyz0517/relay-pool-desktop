use serde::{Deserialize, Serialize};

use super::event::{event_definition, AlertEventType, EventCategory, Severity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    Global,
    EventType,
    Station,
    StationKey,
}

impl ScopeKind {
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "global" => Self::Global,
            "event_type" => Self::EventType,
            "station" => Self::Station,
            "station_key" => Self::StationKey,
            _ => return None,
        })
    }

    pub fn rank(self) -> u8 {
        match self {
            Self::Global => 0,
            Self::EventType => 1,
            Self::Station => 2,
            Self::StationKey => 3,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::EventType => "event_type",
            Self::Station => "station",
            Self::StationKey => "station_key",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerMode {
    Immediate,
    ConsecutiveOccurrences,
    ActiveDuration,
}

impl TriggerMode {
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "immediate" => Self::Immediate,
            "consecutive_occurrences" => Self::ConsecutiveOccurrences,
            "active_duration" => Self::ActiveDuration,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::ConsecutiveOccurrences => "consecutive_occurrences",
            Self::ActiveDuration => "active_duration",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryMode {
    ConsecutiveHealthy,
    HealthyDuration,
}

impl RecoveryMode {
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "consecutive_healthy" => Self::ConsecutiveHealthy,
            "healthy_duration" => Self::HealthyDuration,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConsecutiveHealthy => "consecutive_healthy",
            Self::HealthyDuration => "healthy_duration",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    Never,
    Interval,
    SeverityEscalation,
    IntervalAndEscalation,
}

impl RepeatMode {
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "never" => Self::Never,
            "interval" => Self::Interval,
            "severity_escalation" => Self::SeverityEscalation,
            "interval_and_escalation" => Self::IntervalAndEscalation,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Interval => "interval",
            Self::SeverityEscalation => "severity_escalation",
            Self::IntervalAndEscalation => "interval_and_escalation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuietHoursPolicy {
    Inherit,
    Respect,
    BypassForCritical,
}

impl QuietHoursPolicy {
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "inherit" => Self::Inherit,
            "respect" => Self::Respect,
            "bypass_for_critical" => Self::BypassForCritical,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::Respect => "respect",
            Self::BypassForCritical => "bypass_for_critical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyState {
    Active,
    Disabled,
    Orphaned,
    Tombstone,
}

impl PolicyState {
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "active" => Self::Active,
            "disabled" => Self::Disabled,
            "orphaned" => Self::Orphaned,
            "tombstone" => Self::Tombstone,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Orphaned => "orphaned",
            Self::Tombstone => "tombstone",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyMatchContext<'a> {
    pub event_type: AlertEventType,
    pub base_severity: Severity,
    pub station_id: Option<&'a str>,
    pub station_key_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertPolicy {
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

impl AlertPolicy {
    #[cfg(test)]
    pub fn system_default(base_severity: Severity) -> Self {
        Self::system_default_for_event(None, base_severity)
    }

    pub fn system_default_for_event(
        event_type: Option<AlertEventType>,
        base_severity: Severity,
    ) -> Self {
        let audit = event_type
            .and_then(event_definition)
            .is_some_and(|definition| definition.category == EventCategory::AuditChange);
        let immediate = audit
            || event_type == Some(AlertEventType::KeyInvalid)
            || (event_type.is_none() && base_severity == Severity::Critical);
        let recovery_count = Some(1);
        Self {
            id: "system_default".to_string(),
            name: "System default".to_string(),
            enabled: true,
            state: PolicyState::Active,
            scope_kind: ScopeKind::Global,
            event_type: None,
            station_id: None,
            station_key_id: None,
            minimum_severity: None,
            severity_offset: 0,
            trigger_mode: if immediate {
                TriggerMode::Immediate
            } else {
                TriggerMode::ConsecutiveOccurrences
            },
            trigger_count: (!immediate).then_some(
                if event_type == Some(AlertEventType::CollectorFailed) {
                    3
                } else {
                    2
                },
            ),
            trigger_duration_seconds: None,
            recovery_mode: RecoveryMode::ConsecutiveHealthy,
            recovery_count,
            recovery_duration_seconds: None,
            in_app_enabled: true,
            desktop_enabled: false,
            repeat_mode: RepeatMode::Never,
            repeat_interval_seconds: None,
            cooldown_seconds: 30 * 60,
            recovery_notification_enabled: true,
            quiet_hours_policy: QuietHoursPolicy::Inherit,
            priority: u32::MAX,
            revision: 1,
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() || self.id.len() > 128 {
            return Err("policy id must be 1..=128 bytes".to_string());
        }
        if self.name.is_empty() || self.name.len() > 200 {
            return Err("policy name must be 1..=200 bytes".to_string());
        }
        if !(-1..=1).contains(&self.severity_offset) {
            return Err("severity_offset must be -1, 0 or 1".to_string());
        }
        if self.revision == 0 || self.created_at_ms < 0 || self.updated_at_ms < 0 {
            return Err("policy revision and timestamps must be positive".to_string());
        }
        if self.station_key_id.is_some()
            && self.station_id.is_none()
            && self.scope_kind == ScopeKind::StationKey
        {
            // A key-only rule is valid; an optional station id is only an ownership check.
        }
        match self.scope_kind {
            ScopeKind::Global if self.station_id.is_some() || self.station_key_id.is_some() => {
                return Err("global policy cannot specify station ids".to_string())
            }
            ScopeKind::EventType if self.event_type.is_none() => {
                return Err("event_type policy requires event_type".to_string())
            }
            ScopeKind::EventType if self.station_id.is_some() || self.station_key_id.is_some() => {
                return Err("event_type policy cannot specify station ids".to_string())
            }
            ScopeKind::Station if self.station_id.is_none() || self.station_key_id.is_some() => {
                return Err("station policy requires only station_id".to_string())
            }
            ScopeKind::StationKey if self.station_key_id.is_none() => {
                return Err("station_key policy requires station_key_id".to_string())
            }
            _ => {}
        }
        if let Some(count) = self.trigger_count {
            if !(1..=100).contains(&count) {
                return Err("trigger_count must be between 1 and 100".to_string());
            }
        }
        if let Some(seconds) = self.trigger_duration_seconds {
            if !(60..=30 * 24 * 60 * 60).contains(&seconds) {
                return Err("trigger_duration_seconds is outside supported range".to_string());
            }
        }
        if let Some(count) = self.recovery_count {
            if !(1..=100).contains(&count) {
                return Err("recovery_count must be between 1 and 100".to_string());
            }
        }
        if let Some(seconds) = self.recovery_duration_seconds {
            if !(60..=30 * 24 * 60 * 60).contains(&seconds) {
                return Err("recovery_duration_seconds is outside supported range".to_string());
            }
        }
        if self.trigger_mode == TriggerMode::ConsecutiveOccurrences && self.trigger_count.is_none()
            || self.trigger_mode == TriggerMode::ActiveDuration
                && self.trigger_duration_seconds.is_none()
            || self.trigger_mode == TriggerMode::Immediate
                && (self.trigger_count.is_some() || self.trigger_duration_seconds.is_some())
        {
            return Err("trigger mode and trigger parameters do not match".to_string());
        }
        if self.recovery_mode == RecoveryMode::ConsecutiveHealthy && self.recovery_count.is_none()
            || self.recovery_mode == RecoveryMode::HealthyDuration
                && self.recovery_duration_seconds.is_none()
        {
            return Err("recovery mode and recovery parameters do not match".to_string());
        }
        if matches!(
            self.repeat_mode,
            RepeatMode::Interval | RepeatMode::IntervalAndEscalation
        ) && self.repeat_interval_seconds.is_none()
        {
            return Err("interval repeat mode requires repeat_interval_seconds".to_string());
        }
        if self
            .repeat_interval_seconds
            .is_some_and(|seconds| seconds == 0)
        {
            return Err("repeat_interval_seconds must be positive".to_string());
        }
        Ok(())
    }

    pub fn matches(&self, context: &PolicyMatchContext<'_>) -> bool {
        self.enabled
            && self.state == PolicyState::Active
            && self
                .event_type
                .is_none_or(|value| value == context.event_type)
            && self
                .minimum_severity
                .is_none_or(|minimum| context.base_severity.rank() >= minimum.rank())
            && self
                .station_id
                .as_deref()
                .is_none_or(|id| Some(id) == context.station_id)
            && self
                .station_key_id
                .as_deref()
                .is_none_or(|id| Some(id) == context.station_key_id)
            && match self.scope_kind {
                ScopeKind::Global => true,
                ScopeKind::EventType => self.event_type == Some(context.event_type),
                ScopeKind::Station => self.station_id.as_deref() == context.station_id,
                ScopeKind::StationKey => self.station_key_id.as_deref() == context.station_key_id,
            }
    }

    pub fn resolution_key(&self) -> (u8, u8, u8, u32, i64, &str) {
        (
            self.scope_kind.rank(),
            u8::from(self.event_type.is_some()),
            self.minimum_severity.map_or(0, Severity::rank),
            self.priority,
            self.created_at_ms,
            &self.id,
        )
    }

    pub fn effective_severity(&self, base: Severity) -> Severity {
        base.apply_offset(self.severity_offset)
    }
}

pub fn resolve_policy<'a>(
    policies: impl IntoIterator<Item = &'a AlertPolicy>,
    context: &PolicyMatchContext<'_>,
) -> AlertPolicy {
    policies
        .into_iter()
        .filter(|policy| policy.matches(context))
        .min_by_key(|policy| {
            let key = policy.resolution_key();
            (
                std::cmp::Reverse(key.0),
                std::cmp::Reverse(key.1),
                std::cmp::Reverse(key.2),
                key.3,
                key.4,
                key.5,
            )
        })
        .cloned()
        .unwrap_or_else(|| {
            AlertPolicy::system_default_for_event(Some(context.event_type), context.base_severity)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(scope_kind: ScopeKind, event_type: Option<AlertEventType>) -> AlertPolicy {
        let mut policy = AlertPolicy::system_default(Severity::Warning);
        policy.id = format!("{scope_kind:?}");
        policy.scope_kind = scope_kind;
        policy.event_type = event_type;
        policy.station_id = (scope_kind == ScopeKind::Station).then(|| "station-1".to_string());
        policy.station_key_id = (scope_kind == ScopeKind::StationKey).then(|| "key-1".to_string());
        if scope_kind == ScopeKind::EventType {
            policy.trigger_count = Some(2);
        }
        policy
    }

    #[test]
    fn resolution_prefers_specific_scope_and_falls_back_to_system_default() {
        let global = policy(ScopeKind::Global, None);
        let event = policy(ScopeKind::EventType, Some(AlertEventType::StationDown));
        let station = policy(ScopeKind::Station, None);
        let key = policy(ScopeKind::StationKey, None);
        let context = PolicyMatchContext {
            event_type: AlertEventType::StationDown,
            base_severity: Severity::Critical,
            station_id: Some("station-1"),
            station_key_id: Some("key-1"),
        };
        assert_eq!(
            resolve_policy([&global, &event, &station, &key], &context).id,
            "StationKey"
        );
        let unmatched = PolicyMatchContext {
            station_id: None,
            station_key_id: None,
            ..context
        };
        assert_eq!(resolve_policy([&global], &unmatched).id, "Global");
        assert_eq!(resolve_policy([], &context).id, "system_default");
    }

    #[test]
    fn validation_rejects_mismatched_trigger_parameters() {
        let mut policy = AlertPolicy::system_default(Severity::Warning);
        policy.id = "custom".to_string();
        policy.trigger_mode = TriggerMode::Immediate;
        policy.trigger_count = Some(2);
        assert!(policy.validate().is_err());
    }

    #[test]
    fn system_default_is_valid_for_all_base_severities() {
        for severity in [Severity::Info, Severity::Warning, Severity::Critical] {
            assert!(AlertPolicy::system_default(severity).validate().is_ok());
        }
    }

    #[test]
    fn event_defaults_match_condition_and_audit_semantics() {
        let station_down = AlertPolicy::system_default_for_event(
            Some(AlertEventType::StationDown),
            Severity::Critical,
        );
        assert_eq!(
            station_down.trigger_mode,
            TriggerMode::ConsecutiveOccurrences
        );
        assert_eq!(station_down.trigger_count, Some(2));
        assert_eq!(station_down.recovery_count, Some(1));

        let balance_depleted = AlertPolicy::system_default_for_event(
            Some(AlertEventType::BalanceDepleted),
            Severity::Critical,
        );
        assert_eq!(
            balance_depleted.trigger_mode,
            TriggerMode::ConsecutiveOccurrences
        );
        assert_eq!(balance_depleted.trigger_count, Some(2));

        let collector_failed = AlertPolicy::system_default_for_event(
            Some(AlertEventType::CollectorFailed),
            Severity::Warning,
        );
        assert_eq!(collector_failed.trigger_count, Some(3));

        let key_invalid = AlertPolicy::system_default_for_event(
            Some(AlertEventType::KeyInvalid),
            Severity::Critical,
        );
        assert_eq!(key_invalid.trigger_mode, TriggerMode::Immediate);
        assert_eq!(key_invalid.trigger_count, None);
        assert_eq!(key_invalid.recovery_count, Some(1));

        let audit = AlertPolicy::system_default_for_event(
            Some(AlertEventType::PriceChanged),
            Severity::Info,
        );
        assert_eq!(audit.trigger_mode, TriggerMode::Immediate);
        assert_eq!(audit.trigger_count, None);
        assert_eq!(audit.recovery_count, Some(1));
    }

    #[test]
    fn station_key_policy_cannot_cross_station_ownership_boundary() {
        let mut policy = AlertPolicy::system_default(Severity::Warning);
        policy.id = "key-policy".to_string();
        policy.scope_kind = ScopeKind::StationKey;
        policy.station_key_id = Some("key-1".to_string());
        policy.station_id = Some("station-1".to_string());
        let matching = PolicyMatchContext {
            event_type: AlertEventType::StationDown,
            base_severity: Severity::Warning,
            station_id: Some("station-1"),
            station_key_id: Some("key-1"),
        };
        assert!(policy.matches(&matching));
        let foreign = PolicyMatchContext {
            station_id: Some("station-2"),
            ..matching
        };
        assert!(!policy.matches(&foreign));
    }

    #[test]
    fn policy_serialization_uses_frontend_camel_case_names() {
        let policy = AlertPolicy::system_default(Severity::Warning);
        let value = serde_json::to_value(policy).expect("serialize policy");
        assert!(value.get("scopeKind").is_some());
        assert!(value.get("triggerMode").is_some());
        assert!(value.get("scope_kind").is_none());
    }
}
