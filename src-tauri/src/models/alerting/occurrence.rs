use super::event::{AlertEventType, ConditionKey, EventCategory, ObservationKind, Severity};

/// Immutable observation persisted before any incident projection is changed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    dead_code,
    reason = "contract=alerting.immutable-occurrence; owner=models/alerting; remove_when=typed persistence boundary is retired"
)]
pub struct EventOccurrence {
    pub id: String,
    pub source_observation_key: String,
    pub event_type: AlertEventType,
    pub category: EventCategory,
    pub observation_kind: ObservationKind,
    pub severity: Severity,
    pub condition_key: Option<ConditionKey>,
    pub incident_id: Option<String>,
    pub episode_number: Option<u32>,
    pub object_type: String,
    pub object_id: Option<String>,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub pricing_rule_id: Option<String>,
    pub request_log_id: Option<String>,
    pub source: String,
    pub reason_code: Option<String>,
    pub old_value_json: Option<String>,
    pub new_value_json: Option<String>,
    pub impact_json: Option<String>,
    pub observed_at_ms: i64,
    pub created_at_ms: i64,
}

impl EventOccurrence {
    #[expect(
        dead_code,
        reason = "contract=alerting.occurrence-validation; owner=models/alerting; remove_when=import and persistence adapters validate at DTO boundary"
    )]
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() || self.source_observation_key.is_empty() {
            return Err("occurrence id and source_observation_key are required".to_string());
        }
        if self.object_type.is_empty() || self.source.is_empty() {
            return Err("occurrence object_type and source are required".to_string());
        }
        if self.observed_at_ms < 0 || self.created_at_ms < 0 {
            return Err("occurrence timestamps must be non-negative".to_string());
        }
        for (name, value) in [
            ("old_value_json", self.old_value_json.as_deref()),
            ("new_value_json", self.new_value_json.as_deref()),
            ("impact_json", self.impact_json.as_deref()),
        ] {
            if let Some(value) = value {
                serde_json::from_str::<serde_json::Value>(value)
                    .map_err(|_| format!("{name} must contain valid JSON"))?;
            }
        }
        Ok(())
    }
}
