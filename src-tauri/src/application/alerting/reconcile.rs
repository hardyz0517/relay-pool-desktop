use std::sync::Arc;

use crate::{
    models::alerting::{
        AlertEventType, AlertPolicy, Incident, LifecycleState, PolicyMatchContext, Severity,
        SuppressionReason,
    },
    persistence::{
        error::PersistenceError,
        runtime::PersistenceHandle,
        stores::alerting::{DeliveryStore, IncidentSnapshot, IncidentStore},
    },
};

use super::{
    policy_resolver::PolicyResolver, policy_service::AlertingSettings,
    AlertingReadModelUpdatePublisher,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReconcileCursor {
    pub updated_at_ms: i64,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReconcilePage {
    pub examined: u32,
    pub updated: u32,
    pub suppressed: u64,
    pub next_cursor: Option<ReconcileCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReconcileLimits {
    pub page_size: u32,
    pub suppression_limit_per_incident: u32,
}

impl Default for ReconcileLimits {
    fn default() -> Self {
        Self {
            page_size: 100,
            suppression_limit_per_incident: 100,
        }
    }
}

#[derive(Clone)]
pub(crate) struct AlertingReconciler {
    runtime: PersistenceHandle,
    incident_store: IncidentStore,
    delivery_store: DeliveryStore,
    resolver: PolicyResolver,
    limits: ReconcileLimits,
    alerting_updates: Arc<dyn AlertingReadModelUpdatePublisher>,
}

impl AlertingReconciler {
    pub(crate) fn new(
        runtime: PersistenceHandle,
        alerting_updates: Arc<dyn AlertingReadModelUpdatePublisher>,
    ) -> Self {
        Self {
            runtime,
            incident_store: IncidentStore,
            delivery_store: DeliveryStore,
            resolver: PolicyResolver,
            limits: ReconcileLimits::default(),
            alerting_updates,
        }
    }

    #[expect(
        dead_code,
        reason = "contract=alerting.reconcile-limits; owner=application/alerting; remove_when=reconcile limits are fixed by runtime composition"
    )]
    pub(crate) fn with_limits(mut self, limits: ReconcileLimits) -> Self {
        self.limits = ReconcileLimits {
            page_size: limits.page_size.clamp(1, 500),
            suppression_limit_per_incident: limits.suppression_limit_per_incident.clamp(1, 500),
        };
        self
    }

    pub(crate) async fn reconcile_page(
        &self,
        cursor: Option<ReconcileCursor>,
        policies: &[AlertPolicy],
        settings: &AlertingSettings,
        now_ms: i64,
    ) -> Result<ReconcilePage, PersistenceError> {
        if now_ms < 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let db_cursor = cursor
            .as_ref()
            .map(|value| (value.updated_at_ms, value.id.clone()));
        let snapshots = {
            let mut read = self.runtime.begin_read().await?;
            self.incident_store
                .list_active_page(&mut read, db_cursor, self.limits.page_size)
                .await?
        };
        let examined = snapshots.len() as u32;
        let mut updated = 0;
        let mut suppressed = 0;
        let mut last_cursor = None;
        for snapshot in snapshots {
            let incident = snapshot.incident;
            let base_severity =
                canonical_base_severity(incident.event_type, incident.base_severity);
            last_cursor = Some(ReconcileCursor {
                updated_at_ms: incident.updated_at_ms,
                id: incident.id.clone(),
            });
            let context = PolicyMatchContext {
                event_type: incident.event_type,
                base_severity,
                station_id: incident.station_id.as_deref(),
                station_key_id: incident.station_key_id.as_deref(),
            };
            let resolved = self.resolver.resolve(policies, &context);
            let fingerprint_changed = incident.lifecycle_policy_fingerprint != resolved.fingerprint;
            let mut next = incident.clone();
            next.base_severity = base_severity;
            next.policy_id = Some(resolved.policy.id.clone());
            next.policy_revision = Some(resolved.policy.revision);
            next.severity = resolved.effective_severity;
            next.lifecycle_policy_fingerprint = resolved.fingerprint;
            if fingerprint_changed {
                reset_evaluation_epoch(&mut next, now_ms);
            }
            let should_update = fingerprint_changed
                || next.policy_id != incident.policy_id
                || next.policy_revision != incident.policy_revision
                || next.base_severity != incident.base_severity
                || next.severity != incident.severity;
            if !should_update {
                continue;
            }
            next.version = incident.version.saturating_add(1);
            next.updated_at_ms = now_ms.max(incident.updated_at_ms);
            let incident_id = incident.id.clone();
            let expected_version = incident.version;
            let store = self.incident_store;
            let delivery_store = self.delivery_store;
            let suppression_limit = self.limits.suppression_limit_per_incident;
            // A policy fingerprint change invalidates every not-yet-claimed
            // delivery created from the previous snapshot.  The next
            // observation/deadline may create a replacement; already claimed
            // or delivered rows are intentionally untouched by the SQL WHERE.
            let reason = if fingerprint_changed {
                Some(SuppressionReason::PolicyMuted)
            } else {
                suppression_reason(settings, &resolved.policy, now_ms)
            };
            let suppressed_for_incident = self
                .runtime
                .write(|write| {
                    Box::pin(async move {
                        store
                            .update_snapshot_cas(
                                write,
                                &IncidentSnapshot { incident: next },
                                expected_version,
                            )
                            .await?;
                        if let Some(reason) = reason {
                            delivery_store
                                .suppress_scheduled_for_incident(
                                    write,
                                    &incident_id,
                                    incident.episode_number,
                                    reason,
                                    now_ms,
                                    suppression_limit,
                                )
                                .await
                        } else {
                            Ok(0)
                        }
                    })
                })
                .await?;
            suppressed += suppressed_for_incident;
            updated += 1;
        }
        let next_cursor = if examined == self.limits.page_size {
            last_cursor
        } else {
            None
        };
        if updated > 0 || suppressed > 0 {
            self.alerting_updates.notify_after_commit();
        }
        Ok(ReconcilePage {
            examined,
            updated,
            suppressed,
            next_cursor,
        })
    }
}

fn reset_evaluation_epoch(incident: &mut Incident, now_ms: i64) {
    match incident.lifecycle_state {
        LifecycleState::Pending => {
            incident.pending_since_ms = Some(now_ms);
            incident.consecutive_abnormal_count = 0;
            incident.next_state_evaluation_at_ms = None;
        }
        LifecycleState::Recovering => {
            incident.healthy_since_ms = None;
            incident.consecutive_healthy_count = 0;
            incident.next_state_evaluation_at_ms = None;
        }
        LifecycleState::Open | LifecycleState::Resolved => {
            // An open episode is not closed by editing a rule. A resolved
            // episode is not replayed by a rule edit.
        }
    }
}

fn canonical_base_severity(event_type: AlertEventType, persisted: Severity) -> Severity {
    match event_type {
        AlertEventType::GroupMissing => Severity::Info,
        _ => persisted,
    }
}

fn suppression_reason(
    settings: &AlertingSettings,
    policy: &AlertPolicy,
    now_ms: i64,
) -> Option<SuppressionReason> {
    if !policy.enabled || policy.state != crate::models::alerting::PolicyState::Active {
        return Some(SuppressionReason::PolicyMuted);
    }
    if !settings.alerting_enabled {
        return Some(SuppressionReason::GlobalDisabled);
    }
    if settings.is_paused_at(now_ms) {
        return Some(SuppressionReason::GlobalPause);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::alerting::{AlertEventType, ConditionKey, Severity};

    #[test]
    fn policy_change_starts_new_epoch_only_for_pending_or_recovering() {
        let mut pending = Incident::new(
            "i",
            ConditionKey::new("station:1").unwrap(),
            AlertEventType::StationDown,
            Severity::Warning,
            10,
            "old",
        );
        reset_evaluation_epoch(&mut pending, 100);
        assert_eq!(pending.pending_since_ms, Some(100));
        assert_eq!(pending.consecutive_abnormal_count, 0);
    }

    #[test]
    fn disabled_global_settings_suppress_future_delivery_only() {
        let mut settings = AlertingSettings::default();
        settings.alerting_enabled = false;
        let policy = AlertPolicy::system_default(Severity::Warning);
        assert_eq!(
            suppression_reason(&settings, &policy, 0),
            Some(SuppressionReason::GlobalDisabled)
        );
    }

    #[test]
    fn group_missing_reconciles_legacy_warning_to_info() {
        assert_eq!(
            canonical_base_severity(AlertEventType::GroupMissing, Severity::Warning),
            Severity::Info
        );
        assert_eq!(
            canonical_base_severity(AlertEventType::StationDown, Severity::Critical),
            Severity::Critical
        );
    }
}
