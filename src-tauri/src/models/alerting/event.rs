use serde::{Deserialize, Serialize};

/// Stable, redacted identity for an observation stream.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConditionKey(String);

impl ConditionKey {
    pub const MAX_LEN: usize = 200;

    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.is_empty() || value.len() > Self::MAX_LEN {
            return Err(format!("condition key must be 1..={} bytes", Self::MAX_LEN));
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
        {
            return Err("condition key contains unsupported characters".to_string());
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ConditionKey {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ConditionKey> for String {
    fn from(value: ConditionKey) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    pub fn rank(self) -> u8 {
        match self {
            Self::Info => 1,
            Self::Warning => 2,
            Self::Critical => 3,
        }
    }

    pub fn apply_offset(self, offset: i8) -> Self {
        let rank = (self.rank() as i8 + offset).clamp(1, 3);
        match rank {
            1 => Self::Info,
            2 => Self::Warning,
            _ => Self::Critical,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    AuditChange,
    ConditionObservation,
}

impl EventCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AuditChange => "audit_change",
            Self::ConditionObservation => "condition_observation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    Abnormal,
    Healthy,
    Change,
}

impl ObservationKind {
    #[expect(
        dead_code,
        reason = "contract=alerting.observation-kind-serializer; owner=models/alerting; remove_when=persistence adapters no longer serialize kinds"
    )]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Abnormal => "abnormal",
            Self::Healthy => "healthy",
            Self::Change => "change",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertEventType {
    GroupMissing,
    KeyGroupUnresolved,
    BalanceLow,
    BalanceDepleted,
    PriceExpired,
    KeyInvalid,
    CollectorFailed,
    StationDown,
    RouteImpacted,
    GroupAdded,
    RateChanged,
    GroupRateChanged,
    PriceChanged,
    ModelAdded,
    ModelRemoved,
    AuditChange,
}

impl AlertEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GroupMissing => "group_missing",
            Self::KeyGroupUnresolved => "key_group_unresolved",
            Self::BalanceLow => "balance_low",
            Self::BalanceDepleted => "balance_depleted",
            Self::PriceExpired => "price_expired",
            Self::KeyInvalid => "key_invalid",
            Self::CollectorFailed => "collector_failed",
            Self::StationDown => "station_down",
            Self::RouteImpacted => "route_impacted",
            Self::GroupAdded => "group_added",
            Self::RateChanged => "rate_changed",
            Self::GroupRateChanged => "group_rate_changed",
            Self::PriceChanged => "price_changed",
            Self::ModelAdded => "model_added",
            Self::ModelRemoved => "model_removed",
            Self::AuditChange => "audit_change",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "group_missing" => Self::GroupMissing,
            "key_group_unresolved" => Self::KeyGroupUnresolved,
            "balance_low" => Self::BalanceLow,
            "balance_depleted" => Self::BalanceDepleted,
            "price_expired" => Self::PriceExpired,
            "key_invalid" => Self::KeyInvalid,
            "collector_failed" => Self::CollectorFailed,
            "station_down" => Self::StationDown,
            "route_impacted" => Self::RouteImpacted,
            "group_added" => Self::GroupAdded,
            "rate_changed" => Self::RateChanged,
            "group_rate_changed" => Self::GroupRateChanged,
            "price_changed" => Self::PriceChanged,
            "model_added" => Self::ModelAdded,
            "model_removed" => Self::ModelRemoved,
            "audit_change" => Self::AuditChange,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryOwner {
    GroupBinding,
    BalanceProjection,
    PricingProjection,
    StationKeyHealth,
    CollectorTask,
    EndpointHealth,
    RoutingProjection,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventDefinition {
    pub event_type: AlertEventType,
    pub category: EventCategory,
    pub observation_kind: ObservationKind,
    pub base_severity: Severity,
    pub recovery_owner: RecoveryOwner,
    pub user_overridable: bool,
    pub fact_freshness_seconds: u32,
}

impl EventDefinition {
    pub const fn state(
        event_type: AlertEventType,
        base_severity: Severity,
        recovery_owner: RecoveryOwner,
        fact_freshness_seconds: u32,
    ) -> Self {
        Self {
            event_type,
            category: EventCategory::ConditionObservation,
            observation_kind: ObservationKind::Abnormal,
            base_severity,
            recovery_owner,
            user_overridable: true,
            fact_freshness_seconds,
        }
    }

    pub const fn audit(event_type: AlertEventType, base_severity: Severity) -> Self {
        Self {
            event_type,
            category: EventCategory::AuditChange,
            observation_kind: ObservationKind::Change,
            base_severity,
            recovery_owner: RecoveryOwner::None,
            user_overridable: true,
            fact_freshness_seconds: 0,
        }
    }
}

pub fn event_registry() -> &'static [EventDefinition] {
    static REGISTRY: [EventDefinition; 16] = [
        EventDefinition::audit(AlertEventType::GroupMissing, Severity::Info),
        EventDefinition::state(
            AlertEventType::KeyGroupUnresolved,
            Severity::Warning,
            RecoveryOwner::GroupBinding,
            900,
        ),
        EventDefinition::state(
            AlertEventType::BalanceLow,
            Severity::Warning,
            RecoveryOwner::BalanceProjection,
            900,
        ),
        EventDefinition::state(
            AlertEventType::BalanceDepleted,
            Severity::Critical,
            RecoveryOwner::BalanceProjection,
            900,
        ),
        EventDefinition::state(
            AlertEventType::PriceExpired,
            Severity::Warning,
            RecoveryOwner::PricingProjection,
            900,
        ),
        EventDefinition::state(
            AlertEventType::KeyInvalid,
            Severity::Critical,
            RecoveryOwner::StationKeyHealth,
            300,
        ),
        EventDefinition::state(
            AlertEventType::CollectorFailed,
            Severity::Warning,
            RecoveryOwner::CollectorTask,
            900,
        ),
        EventDefinition::state(
            AlertEventType::StationDown,
            Severity::Critical,
            RecoveryOwner::EndpointHealth,
            300,
        ),
        EventDefinition::state(
            AlertEventType::RouteImpacted,
            Severity::Warning,
            RecoveryOwner::RoutingProjection,
            300,
        ),
        EventDefinition::audit(AlertEventType::GroupAdded, Severity::Info),
        EventDefinition::audit(AlertEventType::RateChanged, Severity::Info),
        EventDefinition::audit(AlertEventType::GroupRateChanged, Severity::Info),
        EventDefinition::audit(AlertEventType::PriceChanged, Severity::Info),
        EventDefinition::audit(AlertEventType::ModelAdded, Severity::Info),
        EventDefinition::audit(AlertEventType::ModelRemoved, Severity::Info),
        EventDefinition::audit(AlertEventType::AuditChange, Severity::Info),
    ];
    &REGISTRY
}

pub fn event_definition(event_type: AlertEventType) -> Option<&'static EventDefinition> {
    event_registry()
        .iter()
        .find(|definition| definition.event_type == event_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_recovery_contract_for_every_condition() {
        assert_eq!(event_registry().len(), 16);
        assert!(event_registry()
            .iter()
            .filter(|entry| entry.category == EventCategory::ConditionObservation)
            .all(|entry| entry.recovery_owner != RecoveryOwner::None));
        assert!(event_registry()
            .iter()
            .filter(|entry| entry.category == EventCategory::AuditChange)
            .all(|entry| entry.observation_kind == ObservationKind::Change));
    }

    #[test]
    fn group_missing_is_an_informational_audit_event() {
        let definition =
            event_definition(AlertEventType::GroupMissing).expect("group missing definition");
        assert_eq!(definition.category, EventCategory::AuditChange);
        assert_eq!(definition.observation_kind, ObservationKind::Change);
        assert_eq!(definition.base_severity, Severity::Info);
        assert_eq!(definition.recovery_owner, RecoveryOwner::None);
    }

    #[test]
    fn condition_key_is_bounded_and_redacted() {
        assert!(ConditionKey::new("station:key:abc").is_ok());
        assert!(ConditionKey::new("https://example.test").is_err());
        assert!(ConditionKey::new("x".repeat(ConditionKey::MAX_LEN + 1)).is_err());
    }
}
