use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use tokio_util::sync::CancellationToken;

pub type BoxTaskFuture = Pin<Box<dyn Future<Output = Result<(), TaskFailure>> + Send + 'static>>;
pub type TaskBody = Arc<dyn Fn(TaskRunContext) -> BoxTaskFuture + Send + Sync + 'static>;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskId(String);

impl TaskId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for TaskId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskRunId(pub u64);

#[derive(Clone)]
pub struct TaskSpec {
    pub id: TaskId,
    pub kind: String,
    pub concurrency_key: Option<String>,
    pub restart_policy: RestartPolicy,
    pub shutdown_timeout: Duration,
    pub body: TaskBody,
}

impl TaskSpec {
    pub fn new(
        id: impl Into<TaskId>,
        kind: impl Into<String>,
        body: impl Fn(TaskRunContext) -> BoxTaskFuture + Send + Sync + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            concurrency_key: None,
            restart_policy: RestartPolicy::never(),
            shutdown_timeout: Duration::from_secs(5),
            body: Arc::new(body),
        }
    }

    pub fn with_concurrency_key(mut self, concurrency_key: impl Into<String>) -> Self {
        self.concurrency_key = Some(concurrency_key.into());
        self
    }

    pub fn with_restart_policy(mut self, restart_policy: RestartPolicy) -> Self {
        self.restart_policy = restart_policy;
        self
    }

    pub fn with_shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.shutdown_timeout = timeout;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestartPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub jitter_seed: u64,
}

impl RestartPolicy {
    pub fn never() -> Self {
        Self {
            max_retries: 0,
            base_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
            jitter_seed: 0,
        }
    }

    pub fn transient(max_retries: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self {
            max_retries,
            base_delay,
            max_delay,
            jitter_seed: 0x5EED,
        }
    }

    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if self.max_retries == 0 || self.base_delay.is_zero() {
            return Duration::ZERO;
        }
        let exponent = attempt.saturating_sub(1).min(16);
        let multiplier = 1_u128 << exponent;
        let base_ms = self.base_delay.as_millis().saturating_mul(multiplier);
        let jitter_ms = deterministic_jitter_ms(self.jitter_seed, attempt);
        let capped_ms = base_ms
            .saturating_add(u128::from(jitter_ms))
            .min(self.max_delay.as_millis());
        Duration::from_millis(capped_ms as u64)
    }
}

fn deterministic_jitter_ms(seed: u64, attempt: u32) -> u64 {
    if seed == 0 {
        return 0;
    }
    let mixed = seed ^ u64::from(attempt).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    mixed % 11
}

#[derive(Clone)]
pub struct TaskRunContext {
    pub task_id: TaskId,
    pub run_id: TaskRunId,
    pub correlation_id: String,
    pub cancellation_token: CancellationToken,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskFailure {
    pub class: RestartClass,
    pub code: String,
}

impl TaskFailure {
    pub fn new(class: RestartClass, code: impl Into<String>) -> Self {
        Self {
            class,
            code: code.into(),
        }
    }

    pub fn transient(code: impl Into<String>) -> Self {
        Self::new(RestartClass::Transient, code)
    }

    pub fn configuration(code: impl Into<String>) -> Self {
        Self::new(RestartClass::Configuration, code)
    }

    pub fn cancelled() -> Self {
        Self::new(RestartClass::Cancelled, "cancelled")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartClass {
    Transient,
    Configuration,
    Authentication,
    Invariant,
    Cancelled,
}
