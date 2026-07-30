use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MonitoringMaintenanceConfig {
    pub(crate) startup_delay_ms: u64,
    pub(crate) startup_jitter_ms: u64,
    pub(crate) interval_ms: u64,
    pub(crate) row_budget: u32,
    pub(crate) time_budget_ms: u64,
}

impl Default for MonitoringMaintenanceConfig {
    fn default() -> Self {
        Self {
            startup_delay_ms: 5_000,
            startup_jitter_ms: 30_000,
            interval_ms: 15 * 60 * 1_000,
            row_budget: 2_000,
            time_budget_ms: 250,
        }
    }
}

impl MonitoringMaintenanceConfig {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.interval_ms == 0 {
            return Err("interval_ms must be positive");
        }
        if self.row_budget == 0 {
            return Err("row_budget must be positive");
        }
        if self.time_budget_ms == 0 {
            return Err("time_budget_ms must be positive");
        }
        Ok(())
    }

    pub(crate) fn deterministic_startup_delay(&self, installation_hash: u64) -> Duration {
        let jitter = if self.startup_jitter_ms == 0 {
            0
        } else {
            installation_hash % self.startup_jitter_ms.saturating_add(1)
        };
        Duration::from_millis(self.startup_delay_ms.saturating_add(jitter))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MonitoringMaintenanceState {
    running: Arc<AtomicBool>,
}

impl Default for MonitoringMaintenanceState {
    fn default() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl MonitoringMaintenanceState {
    pub(crate) fn try_begin_cycle(&self) -> Option<MonitoringMaintenanceCycleGuard> {
        self.running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| MonitoringMaintenanceCycleGuard {
                running: Arc::clone(&self.running),
                started_at: Instant::now(),
            })
    }
}

#[derive(Debug)]
pub(crate) struct MonitoringMaintenanceCycleGuard {
    running: Arc<AtomicBool>,
    started_at: Instant,
}

impl MonitoringMaintenanceCycleGuard {
    pub(crate) fn should_continue(
        &self,
        cancellation: &CancellationToken,
        processed_rows: u32,
        config: &MonitoringMaintenanceConfig,
    ) -> bool {
        !cancellation.is_cancelled()
            && processed_rows < config.row_budget
            && self.started_at.elapsed() < Duration::from_millis(config.time_budget_ms)
    }
}

impl Drop for MonitoringMaintenanceCycleGuard {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}
