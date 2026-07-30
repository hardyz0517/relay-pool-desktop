#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetentionPolicy {
    pub(crate) raw_target_age_ms: i64,
    pub(crate) per_monitor_delete_limit: u32,
    pub(crate) global_delete_limit: u32,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            raw_target_age_ms: 30 * 24 * 60 * 60 * 1_000,
            per_monitor_delete_limit: 1_000,
            global_delete_limit: 10_000,
        }
    }
}

impl RetentionPolicy {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.raw_target_age_ms <= 0 {
            return Err("raw_target_age_ms must be positive");
        }
        if self.per_monitor_delete_limit == 0 || self.global_delete_limit == 0 {
            return Err("retention delete limits must be positive");
        }
        if self.per_monitor_delete_limit > self.global_delete_limit {
            return Err("per_monitor_delete_limit cannot exceed global_delete_limit");
        }
        Ok(())
    }

    pub(crate) fn cutoff_ms(&self, now_ms: i64) -> i64 {
        now_ms.saturating_sub(self.raw_target_age_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetentionRunLimits {
    pub(crate) max_rows: u32,
    pub(crate) max_runtime_ms: u64,
}

impl Default for RetentionRunLimits {
    fn default() -> Self {
        Self {
            max_rows: 2_000,
            max_runtime_ms: 250,
        }
    }
}

impl RetentionRunLimits {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.max_rows == 0 {
            return Err("max_rows must be positive");
        }
        if self.max_runtime_ms == 0 {
            return Err("max_runtime_ms must be positive");
        }
        Ok(())
    }
}
