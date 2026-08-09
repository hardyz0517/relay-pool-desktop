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
    #[expect(
        dead_code,
        reason = "contract=alerting.attention-constructor; owner=models/alerting; remove_when=all attention creation is store-owned"
    )]
    pub fn new(
        incident_id: impl Into<String>,
        episode_number: u32,
        now_ms: i64,
    ) -> Result<Self, String> {
        if episode_number == 0 || now_ms < 0 {
            return Err("episode_number must be positive and timestamp non-negative".to_string());
        }
        Ok(Self {
            incident_id: incident_id.into(),
            episode_number,
            seen_at_ms: None,
            snoozed_until_ms: None,
            updated_at_ms: now_ms,
        })
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.attention-mark-seen; owner=models/alerting; remove_when=attention mutations are fully routed through the command facade"
    )]
    pub fn mark_seen(&mut self, now_ms: i64) {
        self.seen_at_ms = Some(now_ms);
        self.updated_at_ms = now_ms;
    }

    pub fn snooze_until(&mut self, until_ms: i64, now_ms: i64) -> Result<(), String> {
        if until_ms <= now_ms {
            return Err("snooze deadline must be in the future".to_string());
        }
        self.snoozed_until_ms = Some(until_ms);
        self.updated_at_ms = now_ms;
        Ok(())
    }

    pub fn is_snoozed_at(&self, now_ms: i64) -> bool {
        self.snoozed_until_ms.is_some_and(|until| now_ms < until)
    }
}
