use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentAttention {
    pub incident_id: String,
    pub episode_number: u32,
    pub seen_at_ms: Option<i64>,
    pub snoozed_until_ms: Option<i64>,
    pub updated_at_ms: i64,
}

impl IncidentAttention {
    pub fn is_snoozed_at(&self, now_ms: i64) -> bool {
        self.snoozed_until_ms.is_some_and(|until| now_ms < until)
    }
}
