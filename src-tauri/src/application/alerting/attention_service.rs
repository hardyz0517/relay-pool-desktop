use crate::models::alerting::IncidentAttention;

#[derive(Debug, Default, Clone, Copy)]
#[expect(
    dead_code,
    reason = "contract=alerting.attention-service; owner=application/alerting; remove_when=attention mutations are fully routed through the worker"
)]
pub(crate) struct AttentionService;

impl AttentionService {
    #[expect(
        dead_code,
        reason = "contract=alerting.attention-mark-seen; owner=application/alerting; remove_when=attention mutations are fully routed through the command facade"
    )]
    pub(crate) fn mark_seen(&self, attention: &mut IncidentAttention, now_ms: i64) {
        attention.mark_seen(now_ms);
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.attention-snooze; owner=application/alerting; remove_when=attention mutations are fully routed through the command facade"
    )]
    pub(crate) fn snooze_until(
        &self,
        attention: &mut IncidentAttention,
        until_ms: i64,
        now_ms: i64,
    ) -> Result<(), String> {
        attention.snooze_until(until_ms, now_ms)
    }
}
