use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts_per_model: u8,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl RetryPolicy {
    pub fn new(
        max_attempts_per_model: u8,
        base_delay_ms: u64,
        max_delay_ms: u64,
    ) -> Result<Self, String> {
        if !(1..=3).contains(&max_attempts_per_model) {
            return Err("max_attempts_per_model must be between 1 and 3".to_string());
        }
        if max_delay_ms < base_delay_ms {
            return Err("max_delay_ms must be >= base_delay_ms".to_string());
        }
        Ok(Self {
            max_attempts_per_model,
            base_delay_ms,
            max_delay_ms,
        })
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts_per_model: 1,
            base_delay_ms: 200,
            max_delay_ms: 2_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskPolicy {
    pub max_daily_probe_attempts: u32,
    pub require_manual_confirmation_for_high_frequency: bool,
}

impl RiskPolicy {
    pub fn new(max_daily_probe_attempts: u32) -> Result<Self, String> {
        if max_daily_probe_attempts == 0 {
            return Err("max_daily_probe_attempts must be positive".to_string());
        }
        Ok(Self {
            max_daily_probe_attempts,
            require_manual_confirmation_for_high_frequency: true,
        })
    }
}

impl Default for RiskPolicy {
    fn default() -> Self {
        Self {
            max_daily_probe_attempts: 500,
            require_manual_confirmation_for_high_frequency: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthWritebackMode {
    Disabled,
    ObserveOnly,
    Authoritative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthPolicy {
    pub writeback_mode: HealthWritebackMode,
    pub failure_threshold: u8,
    pub recovery_threshold: u8,
}

impl HealthPolicy {
    pub fn new(
        writeback_mode: HealthWritebackMode,
        failure_threshold: u8,
        recovery_threshold: u8,
    ) -> Result<Self, String> {
        if failure_threshold == 0 || recovery_threshold == 0 {
            return Err("health thresholds must be positive".to_string());
        }
        Ok(Self {
            writeback_mode,
            failure_threshold,
            recovery_threshold,
        })
    }
}

impl HealthWritebackMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::ObserveOnly => "observe_only",
            Self::Authoritative => "authoritative",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "disabled" => Some(Self::Disabled),
            "observe_only" => Some(Self::ObserveOnly),
            "authoritative" => Some(Self::Authoritative),
            _ => None,
        }
    }
}

impl Default for HealthPolicy {
    fn default() -> Self {
        Self {
            writeback_mode: HealthWritebackMode::ObserveOnly,
            failure_threshold: 2,
            recovery_threshold: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulePolicy {
    pub interval_seconds: u64,
    pub jitter_seconds: u64,
    pub execution_timeout_ms: u64,
    pub attempt_timeout_ms: u64,
    pub slow_latency_threshold_ms: u64,
}

impl SchedulePolicy {
    pub fn new(
        interval_seconds: i64,
        jitter_seconds: i64,
        execution_timeout_ms: i64,
        attempt_timeout_ms: i64,
        slow_latency_threshold_ms: i64,
    ) -> Result<Self, String> {
        if interval_seconds <= 0 {
            return Err("interval_seconds must be positive".to_string());
        }
        if jitter_seconds < 0 {
            return Err("jitter_seconds must be non-negative".to_string());
        }
        if execution_timeout_ms <= 0 || attempt_timeout_ms <= 0 {
            return Err("timeouts must be positive".to_string());
        }
        if slow_latency_threshold_ms <= 0 {
            return Err("slow_latency_threshold_ms must be positive".to_string());
        }
        let interval_seconds = interval_seconds as u64;
        let jitter_seconds = jitter_seconds as u64;
        if jitter_seconds > 600 {
            return Err("jitter_seconds must be <= 600".to_string());
        }
        if jitter_seconds.saturating_mul(4) > interval_seconds {
            return Err("jitter_seconds must be <= 25% of interval_seconds".to_string());
        }
        let execution_timeout_ms = execution_timeout_ms as u64;
        let attempt_timeout_ms = attempt_timeout_ms as u64;
        if attempt_timeout_ms >= execution_timeout_ms {
            return Err("attempt_timeout_ms must be less than execution_timeout_ms".to_string());
        }
        Ok(Self {
            interval_seconds,
            jitter_seconds,
            execution_timeout_ms,
            attempt_timeout_ms,
            slow_latency_threshold_ms: slow_latency_threshold_ms as u64,
        })
    }
}

impl Default for SchedulePolicy {
    fn default() -> Self {
        Self {
            interval_seconds: 300,
            jitter_seconds: 30,
            execution_timeout_ms: 30_000,
            attempt_timeout_ms: 10_000,
            slow_latency_threshold_ms: 5_000,
        }
    }
}
