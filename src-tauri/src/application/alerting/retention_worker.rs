use crate::persistence::{
    error::PersistenceError,
    runtime::PersistenceHandle,
    stores::alerting::{DeliveryStore, OccurrenceStore},
};

/// Bounded, restart-safe retention pass. Current incidents are never deleted;
/// only resolved episodes and append-only history/delivery rows are eligible.
#[derive(Clone)]
pub(crate) struct AlertingRetentionWorker {
    runtime: PersistenceHandle,
}

impl AlertingRetentionWorker {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self { runtime }
    }

    pub(crate) async fn run_once(
        &self,
        now_ms: i64,
        occurrence_retention_days: u32,
        delivery_retention_days: u32,
        batch_size: u32,
    ) -> Result<RetentionReport, PersistenceError> {
        if now_ms < 0 || occurrence_retention_days == 0 || delivery_retention_days == 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
        let batch = i64::from(batch_size.clamp(1, 10_000));
        let occurrence_cutoff =
            now_ms.saturating_sub(i64::from(occurrence_retention_days).saturating_mul(86_400_000));
        let delivery_cutoff =
            now_ms.saturating_sub(i64::from(delivery_retention_days).saturating_mul(86_400_000));
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    let deliveries = DeliveryStore
                        .delete_terminal_before(write, delivery_cutoff, batch as u32)
                        .await?;
                    let occurrences = OccurrenceStore
                        .delete_retained_before(write, occurrence_cutoff, batch as u32)
                        .await?;
                    Ok(RetentionReport {
                        occurrences_deleted: occurrences,
                        deliveries_deleted: deliveries,
                    })
                })
            })
            .await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RetentionReport {
    pub occurrences_deleted: u64,
    pub deliveries_deleted: u64,
}
