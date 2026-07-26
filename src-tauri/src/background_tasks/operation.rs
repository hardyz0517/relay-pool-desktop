use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub type BoxOperationFuture = Pin<Box<dyn Future<Output = OperationTerminal> + Send>>;

#[derive(Clone, Debug)]
pub struct OperationRegistryConfig {
    pub max_running_global: usize,
    pub max_running_per_concurrency_key: usize,
    pub progress_ring_entries_per_operation: usize,
    pub progress_entry_max_bytes: usize,
    pub terminal_ttl: Duration,
    pub terminal_max_entries: usize,
    pub expired_tombstone_ttl: Duration,
    pub default_deadline: Duration,
}

impl OperationRegistryConfig {
    pub fn architecture_budget() -> Self {
        Self {
            max_running_global: 8,
            max_running_per_concurrency_key: 1,
            progress_ring_entries_per_operation: 128,
            progress_entry_max_bytes: 8_192,
            terminal_ttl: Duration::from_millis(900_000),
            terminal_max_entries: 256,
            expired_tombstone_ttl: Duration::from_millis(3_600_000),
            default_deadline: Duration::from_millis(30_000),
        }
    }
}

#[derive(Clone)]
pub struct OperationRegistry {
    inner: Arc<Mutex<OperationRegistryInner>>,
    next_id: Arc<AtomicU64>,
    config: OperationRegistryConfig,
}

#[derive(Default)]
struct OperationRegistryInner {
    slots: BTreeMap<OperationId, OperationSlot>,
    running_count: usize,
    running_by_concurrency_key: HashMap<String, usize>,
    terminal_order: VecDeque<OperationId>,
    expired_tombstones: BTreeMap<OperationId, Instant>,
}

struct OperationSlot {
    spec: RunningOperationSpec,
    token: CancellationToken,
    commit_barrier: Arc<AtomicBool>,
    cancel_requested: bool,
    started_at: Instant,
    state: OperationState,
    progress: VecDeque<OperationProgress>,
    next_progress_sequence: u64,
    join: Option<JoinHandle<()>>,
}

#[derive(Clone, Debug)]
struct RunningOperationSpec {
    kind: String,
    owner: OperationOwner,
    deadline: Duration,
    concurrency_key: Option<String>,
    cancellation: CancellationPolicy,
}

impl OperationRegistry {
    pub fn new(config: OperationRegistryConfig) -> Self {
        assert!(
            config.max_running_global > 0,
            "max_running_global must be positive"
        );
        assert!(
            config.max_running_per_concurrency_key > 0,
            "max_running_per_concurrency_key must be positive"
        );
        assert!(
            config.progress_ring_entries_per_operation > 0,
            "progress ring capacity must be positive"
        );
        assert!(
            config.progress_entry_max_bytes > 0,
            "progress entry byte limit must be positive"
        );
        assert!(
            config.terminal_max_entries > 0,
            "terminal capacity must be positive"
        );
        assert!(
            !config.default_deadline.is_zero(),
            "default operation deadline must be positive"
        );
        Self {
            inner: Arc::new(Mutex::new(OperationRegistryInner::default())),
            next_id: Arc::new(AtomicU64::new(1)),
            config,
        }
    }

    pub fn start<F>(
        &self,
        request: OperationStartRequest<F>,
    ) -> Result<OperationId, OperationRegistryError>
    where
        F: FnOnce(OperationContext) -> BoxOperationFuture + Send + 'static,
    {
        let id = OperationId(self.next_id.fetch_add(1, Ordering::SeqCst));
        let spec = RunningOperationSpec {
            kind: request.kind,
            owner: request.owner,
            deadline: request.deadline.unwrap_or(self.config.default_deadline),
            concurrency_key: request.concurrency_key,
            cancellation: request.cancellation,
        };
        if spec.deadline.is_zero() {
            return Err(OperationRegistryError::InvalidSpec);
        }
        let token = CancellationToken::new();
        let commit_barrier = Arc::new(AtomicBool::new(false));
        {
            let mut inner = self.inner.lock().expect("operation registry mutex");
            if inner.running_count >= self.config.max_running_global {
                return Err(OperationRegistryError::Overloaded);
            }
            if let Some(key) = &spec.concurrency_key {
                let running = inner
                    .running_by_concurrency_key
                    .get(key)
                    .copied()
                    .unwrap_or(0);
                if running >= self.config.max_running_per_concurrency_key {
                    return Err(OperationRegistryError::Conflict {
                        concurrency_key: key.clone(),
                    });
                }
            }
            inner.running_count += 1;
            if let Some(key) = &spec.concurrency_key {
                *inner
                    .running_by_concurrency_key
                    .entry(key.clone())
                    .or_insert(0) += 1;
            }
            inner.slots.insert(
                id,
                OperationSlot {
                    spec: spec.clone(),
                    token: token.clone(),
                    commit_barrier: Arc::clone(&commit_barrier),
                    cancel_requested: false,
                    started_at: Instant::now(),
                    state: OperationState::Running,
                    progress: VecDeque::new(),
                    next_progress_sequence: 1,
                    join: None,
                },
            );
        }

        let registry = self.clone();
        let context = OperationContext {
            id,
            kind: spec.kind.clone(),
            owner: spec.owner.clone(),
            cancellation_token: token.clone(),
            commit_barrier,
            registry: self.clone(),
        };
        let join = tokio::spawn(async move {
            let terminal = tokio::time::timeout(spec.deadline, (request.body)(context))
                .await
                .unwrap_or(OperationTerminal::TimedOut);
            registry.finish(id, terminal);
        });
        let mut inner = self.inner.lock().expect("operation registry mutex");
        if let Some(slot) = inner.slots.get_mut(&id) {
            slot.join = Some(join);
        }
        Ok(id)
    }

    pub fn progress(
        &self,
        id: OperationId,
        message: impl Into<String>,
    ) -> Result<(), OperationRegistryError> {
        let message = message.into();
        if message.len() > self.config.progress_entry_max_bytes {
            return Err(OperationRegistryError::ProgressTooLarge {
                limit_bytes: self.config.progress_entry_max_bytes,
            });
        }
        let mut inner = self.inner.lock().expect("operation registry mutex");
        let slot = inner
            .slots
            .get_mut(&id)
            .ok_or(OperationRegistryError::NotFound)?;
        if !matches!(
            slot.state,
            OperationState::Running | OperationState::Stopping
        ) {
            return Err(OperationRegistryError::TerminalAlreadyRecorded);
        }
        let progress = OperationProgress {
            id,
            sequence: slot.next_progress_sequence,
            message,
        };
        slot.next_progress_sequence += 1;
        if slot.progress.len() == self.config.progress_ring_entries_per_operation {
            slot.progress.pop_front();
        }
        slot.progress.push_back(progress);
        Ok(())
    }

    pub async fn cancel(
        &self,
        id: OperationId,
        wait: Duration,
    ) -> Result<OperationCancelOutcome, OperationRegistryError> {
        let join = {
            let mut inner = self.inner.lock().expect("operation registry mutex");
            if !inner.slots.contains_key(&id) {
                return Err(self.not_found_or_expired_locked(&inner, id));
            }
            let slot = inner.slots.get_mut(&id).expect("slot checked");
            if !matches!(
                slot.state,
                OperationState::Running | OperationState::Stopping
            ) {
                return Ok(OperationCancelOutcome::AlreadyTerminal {
                    terminal: slot.terminal().expect("terminal state"),
                });
            }
            slot.cancel_requested = true;
            slot.state = OperationState::Stopping;
            match slot.spec.cancellation {
                CancellationPolicy::Cooperative => slot.token.cancel(),
                CancellationPolicy::Detach => {}
            }
            slot.join.take()
        };

        let Some(join) = join else {
            return Ok(OperationCancelOutcome::StillStopping);
        };
        match tokio::time::timeout(wait, join).await {
            Ok(Ok(())) => {
                let terminal = self
                    .status(id)?
                    .terminal
                    .expect("terminal after cancel wait");
                Ok(OperationCancelOutcome::Stopped { terminal })
            }
            Ok(Err(_)) => {
                self.finish(
                    id,
                    OperationTerminal::Failed {
                        code: OperationFailureCode::new("operation-panicked"),
                    },
                );
                let terminal = self.status(id)?.terminal.expect("panic terminal");
                Ok(OperationCancelOutcome::Stopped { terminal })
            }
            Err(_) => Ok(OperationCancelOutcome::StillStopping),
        }
    }

    pub fn detach(
        &self,
        id: OperationId,
    ) -> Result<OperationDetachOutcome, OperationRegistryError> {
        let mut inner = self.inner.lock().expect("operation registry mutex");
        if !inner.slots.contains_key(&id) {
            return Err(self.not_found_or_expired_locked(&inner, id));
        }
        let slot = inner.slots.get_mut(&id).expect("slot checked");
        match slot.spec.cancellation {
            CancellationPolicy::Cooperative => {
                slot.cancel_requested = true;
                slot.state = OperationState::Stopping;
                slot.token.cancel();
                Ok(OperationDetachOutcome::CancelRequested)
            }
            CancellationPolicy::Detach => Ok(OperationDetachOutcome::Detached),
        }
    }

    pub fn status(&self, id: OperationId) -> Result<OperationSnapshot, OperationRegistryError> {
        let inner = self.inner.lock().expect("operation registry mutex");
        let Some(slot) = inner.slots.get(&id) else {
            return Err(self.not_found_or_expired_locked(&inner, id));
        };
        Ok(OperationSnapshot {
            id,
            kind: slot.spec.kind.clone(),
            owner: slot.spec.owner.clone(),
            state: slot.state.clone(),
            started_at: slot.started_at,
            progress: slot.progress.iter().cloned().collect(),
            terminal: slot.terminal(),
        })
    }

    pub fn metrics(&self) -> OperationRegistryMetrics {
        let inner = self.inner.lock().expect("operation registry mutex");
        OperationRegistryMetrics {
            running: inner.running_count,
            stored: inner.slots.len(),
            terminal: inner
                .slots
                .values()
                .filter(|slot| slot.terminal().is_some())
                .count(),
            expired_tombstones: inner.expired_tombstones.len(),
        }
    }

    pub fn gc(&self, now: Instant) {
        let mut inner = self.inner.lock().expect("operation registry mutex");
        let terminal_ids = inner
            .slots
            .iter()
            .filter_map(|(id, slot)| {
                let OperationState::Terminal { recorded_at, .. } = slot.state else {
                    return None;
                };
                (now.duration_since(recorded_at) >= self.config.terminal_ttl).then_some(*id)
            })
            .collect::<Vec<_>>();
        for id in terminal_ids {
            inner.slots.remove(&id);
            inner.expired_tombstones.insert(id, now);
        }
        inner.expired_tombstones.retain(|_, recorded_at| {
            now.duration_since(*recorded_at) < self.config.expired_tombstone_ttl
        });
    }

    fn finish(&self, id: OperationId, mut terminal: OperationTerminal) {
        let mut inner = self.inner.lock().expect("operation registry mutex");
        let Some(slot) = inner.slots.get_mut(&id) else {
            return;
        };
        if let OperationState::Terminal { .. } = slot.state {
            return;
        }
        if slot.cancel_requested
            && slot.commit_barrier.load(Ordering::SeqCst)
            && terminal == OperationTerminal::Cancelled
        {
            terminal = OperationTerminal::ResultUnknown;
        }
        if slot.cancel_requested && terminal == OperationTerminal::TimedOut {
            terminal = OperationTerminal::ResultUnknown;
        }
        let concurrency_key = slot.spec.concurrency_key.clone();
        slot.state = OperationState::Terminal {
            terminal: terminal.clone(),
            recorded_at: Instant::now(),
        };
        slot.join = None;
        inner.running_count = inner.running_count.saturating_sub(1);
        if let Some(key) = &concurrency_key {
            if let Some(count) = inner.running_by_concurrency_key.get_mut(key) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    inner.running_by_concurrency_key.remove(key);
                }
            }
        }
        inner.terminal_order.push_back(id);
        while inner.terminal_order.len() > self.config.terminal_max_entries {
            if let Some(expired) = inner.terminal_order.pop_front() {
                inner.slots.remove(&expired);
                inner.expired_tombstones.insert(expired, Instant::now());
            }
        }
    }

    fn not_found_or_expired_locked(
        &self,
        inner: &OperationRegistryInner,
        id: OperationId,
    ) -> OperationRegistryError {
        if inner.expired_tombstones.contains_key(&id) {
            OperationRegistryError::Expired
        } else {
            OperationRegistryError::NotFound
        }
    }
}

pub struct OperationStartRequest<F>
where
    F: FnOnce(OperationContext) -> BoxOperationFuture + Send + 'static,
{
    pub kind: String,
    pub owner: OperationOwner,
    pub deadline: Option<Duration>,
    pub concurrency_key: Option<String>,
    pub cancellation: CancellationPolicy,
    body: F,
}

impl<F> OperationStartRequest<F>
where
    F: FnOnce(OperationContext) -> BoxOperationFuture + Send + 'static,
{
    pub fn new(kind: impl Into<String>, owner: OperationOwner, body: F) -> Self {
        Self {
            kind: kind.into(),
            owner,
            deadline: None,
            concurrency_key: None,
            cancellation: CancellationPolicy::Cooperative,
            body,
        }
    }

    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = Some(deadline);
        self
    }

    pub fn with_concurrency_key(mut self, key: impl Into<String>) -> Self {
        self.concurrency_key = Some(key.into());
        self
    }

    pub fn with_cancellation_policy(mut self, cancellation: CancellationPolicy) -> Self {
        self.cancellation = cancellation;
        self
    }
}

#[derive(Clone)]
pub struct OperationContext {
    pub id: OperationId,
    pub kind: String,
    pub owner: OperationOwner,
    pub cancellation_token: CancellationToken,
    commit_barrier: Arc<AtomicBool>,
    registry: OperationRegistry,
}

impl OperationContext {
    pub fn emit_progress(&self, message: impl Into<String>) -> Result<(), OperationRegistryError> {
        self.registry.progress(self.id, message)
    }

    pub fn enter_commit_barrier(&self) {
        self.commit_barrier.store(true, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationId(u64);

impl OperationId {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationOwner {
    pub feature: String,
}

impl OperationOwner {
    pub fn new(feature: impl Into<String>) -> Self {
        Self {
            feature: feature.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationState {
    Running,
    Stopping,
    Terminal {
        terminal: OperationTerminal,
        recorded_at: Instant,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationTerminal {
    Completed,
    Failed { code: OperationFailureCode },
    Cancelled,
    TimedOut,
    ResultUnknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationFailureCode(String);

impl OperationFailureCode {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancellationPolicy {
    Cooperative,
    Detach,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationProgress {
    pub id: OperationId,
    pub sequence: u64,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct OperationSnapshot {
    pub id: OperationId,
    pub kind: String,
    pub owner: OperationOwner,
    pub state: OperationState,
    pub started_at: Instant,
    pub progress: Vec<OperationProgress>,
    pub terminal: Option<OperationTerminal>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationCancelOutcome {
    Stopped { terminal: OperationTerminal },
    StillStopping,
    AlreadyTerminal { terminal: OperationTerminal },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationDetachOutcome {
    CancelRequested,
    Detached,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OperationRegistryMetrics {
    pub running: usize,
    pub stored: usize,
    pub terminal: usize,
    pub expired_tombstones: usize,
}

impl OperationSlot {
    fn terminal(&self) -> Option<OperationTerminal> {
        match &self.state {
            OperationState::Terminal { terminal, .. } => Some(terminal.clone()),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum OperationRegistryError {
    Overloaded,
    Conflict { concurrency_key: String },
    InvalidSpec,
    NotFound,
    Expired,
    ProgressTooLarge { limit_bytes: usize },
    TerminalAlreadyRecorded,
}

impl std::fmt::Display for OperationRegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overloaded => formatter.write_str("operation registry is overloaded"),
            Self::Conflict { concurrency_key } => {
                write!(
                    formatter,
                    "operation concurrency key is running: {concurrency_key}"
                )
            }
            Self::InvalidSpec => formatter.write_str("operation spec is invalid"),
            Self::NotFound => formatter.write_str("operation not found"),
            Self::Expired => formatter.write_str("operation expired"),
            Self::ProgressTooLarge { limit_bytes } => {
                write!(formatter, "operation progress exceeds {limit_bytes} bytes")
            }
            Self::TerminalAlreadyRecorded => {
                formatter.write_str("operation terminal is already recorded")
            }
        }
    }
}

impl std::error::Error for OperationRegistryError {}
