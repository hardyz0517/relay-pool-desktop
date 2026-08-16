use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::{
    fs,
    io::Write,
    time::{Duration, Instant},
};

use super::clock::ClockGuard;
use super::descriptor::EventDescriptor;
use super::event::{Component, EventLevel, EventOutcome, RuntimeDetail, RuntimeEvent};
use super::lease::RuntimeLogLease;
use super::recovery::{recover_partials, RecoveryConfig, RecoveryReport};
use super::retention::{retain, RetentionConfig, RetentionReport};
use super::sink::{RuntimeLogWriter, DEFAULT_MAX_SEGMENT_BYTES};
use super::subject::{SessionId, StableEventCode};

const RUNTIME_LOG_QUEUE_CAPACITY: usize = 256;
const RUNTIME_LOG_NORMAL_QUEUE_CAPACITY: usize = 224;
const RUNTIME_LOG_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);
const LEASE_RETRY_INITIAL: Duration = Duration::from_millis(100);
const LEASE_RETRY_MAX: Duration = Duration::from_secs(5);

enum WriterCommand {
    Append {
        line: Vec<u8>,
        at_ms: i64,
        allow_day_rotation: bool,
    },
    Flush(mpsc::Sender<Result<(), ()>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeLogState {
    Ready,
    Degraded,
}

#[derive(Debug)]
struct Inner {
    session_id: SessionId,
    sequence: u64,
    writer: Option<RuntimeLogWriter>,
    lease: Option<RuntimeLogLease>,
    state: RuntimeLogState,
    clock: ClockGuard,
    dropped: u64,
    rejected: u64,
    clock_adjustment_reported: bool,
    flush_timed_out: bool,
    last_sink_error_code: Option<&'static str>,
    recovery: RecoveryReport,
    retention: RetentionReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeLogSnapshot {
    pub(crate) state: RuntimeLogState,
    pub(crate) dropped_count: u64,
    pub(crate) rejected_count: u64,
    pub(crate) last_sink_error_code: Option<&'static str>,
    pub(crate) recovery: RecoveryReport,
    pub(crate) retention: RetentionReport,
    pub(crate) clock_stable: bool,
}

#[derive(Debug)]
struct QueueBudget {
    queued: AtomicUsize,
}

impl QueueBudget {
    fn try_reserve(&self, priority: bool) -> bool {
        let limit = if priority {
            RUNTIME_LOG_QUEUE_CAPACITY
        } else {
            RUNTIME_LOG_NORMAL_QUEUE_CAPACITY
        };
        let mut current = self.queued.load(Ordering::Acquire);
        loop {
            if current >= limit {
                return false;
            }
            match self.queued.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(observed) => current = observed,
            }
        }
    }

    fn release(&self) {
        self.queued.fetch_sub(1, Ordering::AcqRel);
    }

    #[cfg(test)]
    fn depth(&self) -> usize {
        self.queued.load(Ordering::Acquire)
    }
}

/// The only production entry point for durable runtime events.
#[derive(Debug, Clone)]
pub(crate) struct RuntimeLogService {
    root: PathBuf,
    inner: Arc<Mutex<Inner>>,
    queue: mpsc::SyncSender<WriterCommand>,
    queue_budget: Arc<QueueBudget>,
}

impl RuntimeLogService {
    pub(crate) fn open(root: impl AsRef<Path>) -> Self {
        Self::open_with_segment_bytes(root, DEFAULT_MAX_SEGMENT_BYTES)
    }

    #[cfg(all(feature = "runtime-logging-windows-smoke", debug_assertions))]
    pub(crate) fn open_for_runtime_logging_smoke(root: impl AsRef<Path>) -> Self {
        // Keep the packaged smoke quick while exercising the same atomic
        // segment publication path used in production.
        Self::open_with_segment_bytes(root, 4096)
    }

    fn open_with_segment_bytes(root: impl AsRef<Path>, max_segment_bytes: u64) -> Self {
        let root = root.as_ref().to_path_buf();
        let lease = RuntimeLogLease::try_acquire(&root).ok();
        let writer = lease
            .as_ref()
            .map(|lease| RuntimeLogWriter::open(lease, max_segment_bytes));
        let state = if writer.is_some() {
            RuntimeLogState::Ready
        } else {
            RuntimeLogState::Degraded
        };
        let (queue, receiver) = mpsc::sync_channel(RUNTIME_LOG_QUEUE_CAPACITY);
        let queue_budget = Arc::new(QueueBudget {
            queued: AtomicUsize::new(0),
        });
        let inner = Arc::new(Mutex::new(Inner {
            session_id: SessionId::new(),
            sequence: 0,
            writer,
            lease,
            state,
            clock: ClockGuard::default(),
            dropped: 0,
            rejected: 0,
            clock_adjustment_reported: false,
            flush_timed_out: false,
            last_sink_error_code: None,
            recovery: RecoveryReport::default(),
            retention: RetentionReport::default(),
        }));
        let worker_inner = Arc::clone(&inner);
        let worker_queue_budget = Arc::clone(&queue_budget);
        let worker_root = root.clone();
        if std::thread::Builder::new()
            .name("runtime-log-writer".to_owned())
            .spawn(move || writer_worker(worker_root, worker_inner, receiver, worker_queue_budget))
            .is_err()
        {
            if let Ok(mut guard) = inner.lock() {
                guard.state = RuntimeLogState::Degraded;
                guard.last_sink_error_code = Some("runtime.sink.worker_unavailable");
            }
        }
        Self {
            root,
            inner,
            queue,
            queue_budget,
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Runs bounded startup maintenance only while this process owns the
    /// installation-wide runtime-log lease. The active writer is excluded
    /// from retention; malformed and unknown files remain untouched.
    pub(crate) fn startup_maintenance(&self) -> (RecoveryReport, RetentionReport) {
        let Ok(mut inner) = self.inner.lock() else {
            return (RecoveryReport::default(), RetentionReport::default());
        };
        let clock_observation = inner.clock.sample_now();
        let clock_stable = inner.clock.is_stable()
            && clock_observation.adjustment == super::clock::ClockAdjustment::None;
        let Some(lease) = inner.lease.as_ref() else {
            return (RecoveryReport::default(), RetentionReport::default());
        };
        let _ = self.publish_manifest_snapshot();
        let recovery = recover_partials(&self.root, lease, RecoveryConfig::default());
        let active_paths = inner
            .writer
            .as_ref()
            .and_then(|writer| writer.active_path().map(Path::to_path_buf))
            .into_iter()
            .collect::<Vec<_>>();
        let retention = retain(
            &self.root,
            &active_paths,
            unix_ms().max(0) as u128,
            RetentionConfig {
                clock_stable,
                ..RetentionConfig::default()
            },
        );
        inner.recovery = recovery;
        inner.retention = retention;
        if retention.delete_failures > 0 {
            inner.last_sink_error_code = Some("runtime.log_retention.degraded");
        }
        (recovery, retention)
    }

    fn publish_manifest_snapshot(&self) -> std::io::Result<()> {
        let manifest = super::catalog::Catalog::build(
            super::catalog::OWNER_EVENT_DESCRIPTOR_SLICES,
        )
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "runtime manifest"))?;
        let bytes = serde_json::to_vec_pretty(&manifest).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "runtime manifest")
        })?;
        let partial = self.root.join("manifest.json.partial");
        let published = self.root.join("manifest.json");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        let mut current_bytes = bytes;
        current_bytes.push(b'\n');
        if let Ok(existing) = fs::read(&published) {
            if existing == current_bytes {
                let _ = fs::remove_file(&partial);
                return Ok(());
            }
        }

        // Windows does not replace an existing file with rename. Preserve one
        // previous snapshot, move the old current out of the way, and restore
        // it if publishing the new snapshot fails. The manifest is auxiliary;
        // a failed rotation must never prevent the application from starting.
        let previous = self.root.join("manifest.previous.json");
        let had_published = published.exists();
        if had_published {
            let _ = fs::remove_file(&previous);
            fs::rename(&published, &previous)?;
        }
        if let Err(error) = fs::rename(&partial, &published) {
            if had_published {
                let _ = fs::rename(&previous, &published);
            }
            return Err(error);
        }
        Ok(())
    }

    pub(crate) fn state(&self) -> RuntimeLogState {
        self.inner
            .lock()
            .map(|inner| inner.state)
            .unwrap_or(RuntimeLogState::Degraded)
    }

    pub(crate) fn queue_counters(&self) -> (u64, u64) {
        self.inner
            .lock()
            .map(|inner| (inner.dropped, inner.rejected))
            .unwrap_or((0, 0))
    }

    pub(crate) fn snapshot(&self) -> RuntimeLogSnapshot {
        self.inner
            .lock()
            .map(|inner| RuntimeLogSnapshot {
                state: inner.state,
                dropped_count: inner.dropped,
                rejected_count: inner.rejected,
                last_sink_error_code: inner.last_sink_error_code,
                recovery: inner.recovery,
                retention: inner.retention,
                clock_stable: inner.clock.is_stable(),
            })
            .unwrap_or(RuntimeLogSnapshot {
                state: RuntimeLogState::Degraded,
                dropped_count: 0,
                rejected_count: 0,
                last_sink_error_code: Some("runtime.sink.state_unavailable"),
                recovery: RecoveryReport::default(),
                retention: RetentionReport::default(),
                clock_stable: false,
            })
    }

    #[cfg(test)]
    fn inject_clock_adjustment(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.clock.sample_at(1_000, 1);
            inner.clock.sample_at(900, 2);
        }
    }

    /// Persist an event through its compiled descriptor. Production callers
    /// cannot select component or severity independently of the catalog.
    pub(crate) fn record_descriptor(
        &self,
        descriptor: &'static EventDescriptor,
        outcome: EventOutcome,
        detail: RuntimeDetail,
    ) {
        self.record_parts(
            descriptor.code,
            descriptor.component,
            descriptor.level,
            outcome,
            detail,
        );
    }

    #[cfg(test)]
    pub(crate) fn record(
        &self,
        code: &'static str,
        component: Component,
        level: EventLevel,
        outcome: EventOutcome,
        detail: RuntimeDetail,
    ) {
        self.record_parts(code, component, level, outcome, detail);
    }

    fn record_parts(
        &self,
        code: &'static str,
        component: Component,
        level: EventLevel,
        outcome: EventOutcome,
        detail: RuntimeDetail,
    ) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        inner.sequence = inner.sequence.saturating_add(1);
        let Ok(event_code) = StableEventCode::new(code) else {
            inner.state = RuntimeLogState::Degraded;
            inner.last_sink_error_code = Some("runtime.sink.invalid_event_code");
            return;
        };
        let error = (outcome == EventOutcome::Error).then(|| {
            super::error::RuntimeError::new(
                StableEventCode::new("runtime").expect("static runtime domain"),
                StableEventCode::new("internal").expect("static runtime error"),
                false,
                super::error::DataDisposition::Redacted,
            )
        });
        let correlation_id = current_correlation_id();
        let interaction_id = current_interaction_id();
        let clock_observation = inner.clock.sample_now();
        let at_ms = clock_observation.at_ms;
        let clock_adjustment = (clock_observation.adjustment
            != super::clock::ClockAdjustment::None
            && !inner.clock_adjustment_reported)
            .then_some(clock_observation.adjustment);
        let clock_stable = inner.clock.is_stable()
            && clock_observation.adjustment == super::clock::ClockAdjustment::None;
        let Ok(event) = RuntimeEvent::new(
            at_ms,
            inner.sequence,
            level,
            event_code,
            component,
            outcome,
            inner.session_id.clone(),
            correlation_id.clone(),
            interaction_id.clone(),
            None,
            None,
            None,
            error,
            detail,
        ) else {
            inner.state = RuntimeLogState::Degraded;
            inner.last_sink_error_code = Some("runtime.sink.event_validation_failed");
            return;
        };
        // The catalog is the producer contract, not just a reader hint. Do
        // this before queueing so incompatible events never become durable
        // data that diagnostics cannot explain.
        if !super::catalog::Catalog::accepts_event(&event) {
            inner.rejected = inner.rejected.saturating_add(1);
            inner.state = RuntimeLogState::Degraded;
            inner.last_sink_error_code = Some("runtime.sink.event_contract_rejected");
            return;
        }
        let Ok(line) = event.to_json_line() else {
            inner.state = RuntimeLogState::Degraded;
            inner.last_sink_error_code = Some("runtime.sink.serialization_failed");
            return;
        };
        if inner.state != RuntimeLogState::Ready {
            return;
        }
        let priority = matches!(level, EventLevel::Error | EventLevel::Warn);
        if !self.queue_budget.try_reserve(priority) {
            inner.dropped = inner.dropped.saturating_add(1);
            return;
        }
        let accepted = match self.queue.try_send(WriterCommand::Append {
            line: line.trim_end().as_bytes().to_vec(),
            at_ms,
            allow_day_rotation: clock_stable,
        }) {
            Ok(()) => true,
            Err(mpsc::TrySendError::Full(_)) => {
                self.queue_budget.release();
                inner.dropped = inner.dropped.saturating_add(1);
                false
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.queue_budget.release();
                inner.rejected = inner.rejected.saturating_add(1);
                inner.state = RuntimeLogState::Degraded;
                inner.last_sink_error_code = Some("runtime.sink.queue_disconnected");
                false
            }
        };
        if !accepted {
            return;
        }

        // Clock anomalies are recorded once per process with a closed enum
        // detail. This is deliberately an independent queue item rather than
        // a recursive call through the service, so a wall-clock fault cannot
        // deadlock the producer boundary.
        let Some(adjustment) = clock_adjustment else {
            return;
        };
        let clock_sequence = inner.sequence.saturating_add(1);
        let Ok(clock_event) = RuntimeEvent::new(
            at_ms,
            clock_sequence,
            EventLevel::Warn,
            StableEventCode::new("runtime.clock.wall_adjusted")
                .expect("static clock adjustment event code"),
            Component::Runtime,
            EventOutcome::Degraded,
            inner.session_id.clone(),
            correlation_id,
            interaction_id,
            None,
            None,
            None,
            None,
            RuntimeDetail::Clock { adjustment },
        ) else {
            inner.state = RuntimeLogState::Degraded;
            inner.last_sink_error_code = Some("runtime.sink.clock_event_validation_failed");
            return;
        };
        if !super::catalog::Catalog::accepts_event(&clock_event) {
            inner.rejected = inner.rejected.saturating_add(1);
            inner.state = RuntimeLogState::Degraded;
            inner.last_sink_error_code = Some("runtime.sink.clock_event_contract_rejected");
            return;
        }
        let Ok(clock_line) = clock_event.to_json_line() else {
            inner.state = RuntimeLogState::Degraded;
            inner.last_sink_error_code = Some("runtime.sink.clock_event_serialization_failed");
            return;
        };
        if !self.queue_budget.try_reserve(true) {
            inner.dropped = inner.dropped.saturating_add(1);
            return;
        }
        match self.queue.try_send(WriterCommand::Append {
            line: clock_line.trim_end().as_bytes().to_vec(),
            at_ms,
            allow_day_rotation: false,
        }) {
            Ok(()) => {
                inner.sequence = clock_sequence;
                inner.clock_adjustment_reported = true;
            }
            Err(mpsc::TrySendError::Full(_)) => {
                self.queue_budget.release();
                inner.dropped = inner.dropped.saturating_add(1);
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.queue_budget.release();
                inner.rejected = inner.rejected.saturating_add(1);
                inner.state = RuntimeLogState::Degraded;
                inner.last_sink_error_code = Some("runtime.sink.queue_disconnected");
            }
        }
    }

    pub(crate) fn flush(&self) {
        let (reply, receiver) = mpsc::channel();
        let deadline = Instant::now() + RUNTIME_LOG_FLUSH_TIMEOUT;
        let mut command = WriterCommand::Flush(reply);
        let sent = loop {
            match self.queue.try_send(command) {
                Ok(()) => break true,
                Err(mpsc::TrySendError::Disconnected(_)) => break false,
                Err(mpsc::TrySendError::Full(returned)) => {
                    command = returned;
                    if Instant::now() >= deadline {
                        break false;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        };
        let completed = sent
            && receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .ok()
                .and_then(Result::ok)
                .is_some();
        if !completed {
            if let Ok(mut inner) = self.inner.lock() {
                inner.state = RuntimeLogState::Degraded;
                inner.flush_timed_out = true;
                inner.last_sink_error_code = Some("runtime.sink.flush_timeout");
            }
        }
    }
}

fn writer_worker(
    root: PathBuf,
    inner: Arc<Mutex<Inner>>,
    receiver: mpsc::Receiver<WriterCommand>,
    queue_budget: Arc<QueueBudget>,
) {
    let mut retry_delay = LEASE_RETRY_INITIAL;
    let mut next_retry_at = Instant::now();
    loop {
        let command = match receiver.recv_timeout(retry_delay) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if Instant::now() >= next_retry_at {
                    if restore_lease(&root, &inner) {
                        retry_delay = LEASE_RETRY_INITIAL;
                    } else {
                        retry_delay = (retry_delay * 2).min(LEASE_RETRY_MAX);
                    }
                    next_retry_at = Instant::now() + retry_delay;
                }
                continue;
            }
        };
        match command {
            WriterCommand::Append {
                line,
                at_ms,
                allow_day_rotation,
            } => {
                queue_budget.release();
                let Ok(mut guard) = inner.lock() else {
                    continue;
                };
                let Some(mut writer) = guard.writer.take() else {
                    continue;
                };
                drop(guard);

                // Never hold the service state mutex across synchronous file I/O.
                let result = writer.append_json_line_at(&line, at_ms, allow_day_rotation);
                if let Ok(mut guard) = inner.lock() {
                    if result.is_err() {
                        guard.state = RuntimeLogState::Degraded;
                        guard.last_sink_error_code = Some("runtime.sink.write_failed");
                        // A failed writer may retain an active partial or a
                        // broken handle. Drop both so the bounded lease retry
                        // path can reacquire a clean writer later.
                        guard.writer = None;
                        guard.lease = None;
                    } else {
                        guard.writer = Some(writer);
                    }
                }
            }
            WriterCommand::Flush(reply) => {
                let writer = inner.lock().ok().and_then(|mut guard| guard.writer.take());
                let result = match writer {
                    Some(mut writer) => {
                        let result = writer.flush_and_publish().map_err(|_| ());
                        if let Ok(mut guard) = inner.lock() {
                            if result.is_err() {
                                guard.state = RuntimeLogState::Degraded;
                                guard.last_sink_error_code = Some("runtime.sink.flush_failed");
                                guard.writer = None;
                                guard.lease = None;
                            } else {
                                guard.writer = Some(writer);
                                if guard.flush_timed_out {
                                    guard.state = RuntimeLogState::Ready;
                                    guard.flush_timed_out = false;
                                }
                            }
                        }
                        result
                    }
                    None => Ok(()),
                };
                let _ = reply.send(result);
            }
        }
    }
}

fn restore_lease(root: &Path, inner: &Arc<Mutex<Inner>>) -> bool {
    if inner
        .lock()
        .ok()
        .is_none_or(|guard| guard.lease.is_some() || guard.writer.is_some())
    {
        return false;
    }
    let Ok(lease) = RuntimeLogLease::try_acquire(root) else {
        return false;
    };

    // Recovery is performed before making the new writer visible. It is
    // bounded by RecoveryConfig and never emits a dynamic retry diagnostic.
    let _ = recover_partials(root, &lease, RecoveryConfig::default());
    let writer = RuntimeLogWriter::open(&lease, DEFAULT_MAX_SEGMENT_BYTES);
    let Ok(mut guard) = inner.lock() else {
        return false;
    };
    if guard.lease.is_some() || guard.writer.is_some() {
        return false;
    }
    guard.writer = Some(writer);
    guard.lease = Some(lease);
    guard.state = RuntimeLogState::Ready;
    true
}

fn unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

fn current_correlation_id() -> Option<super::subject::CorrelationIdRef> {
    crate::observability::correlation::current_id_string()
        .and_then(|value| super::subject::CorrelationIdRef::from_public(&value).ok())
}

fn current_interaction_id() -> Option<super::subject::InteractionId> {
    crate::observability::correlation::current_interaction()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::correlation;
    use crate::observability::runtime::{RuntimeEvent, RuntimeLogReader};
    use crate::observability::runtime_context::RuntimeContextRegistry;
    use std::process::{Child, Command};

    const CHILD_ROOT: &str = "RELAY_POOL_RUNTIME_SERVICE_CHILD_ROOT";
    const CHILD_READY: &str = "RELAY_POOL_RUNTIME_SERVICE_CHILD_READY";
    const CHILD_RELEASE: &str = "RELAY_POOL_RUNTIME_SERVICE_CHILD_RELEASE";

    #[test]
    fn records_are_drained_by_the_writer_worker_on_flush() {
        let root = tempfile::tempdir().expect("runtime root");
        let service = RuntimeLogService::open(root.path());
        service.record(
            "runtime.log_event.dropped",
            Component::Runtime,
            EventLevel::Warn,
            EventOutcome::Ok,
            RuntimeDetail::None,
        );
        service.flush();

        let published = std::fs::read_dir(root.path())
            .expect("runtime directory")
            .flatten()
            .filter(|entry| {
                entry
                    .path()
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".jsonl"))
            })
            .count();
        assert_eq!(published, 1);
    }

    #[tokio::test]
    async fn spawned_runtime_event_preserves_explicit_interaction_and_correlation() {
        let root = tempfile::tempdir().expect("runtime root");
        let service = RuntimeLogService::open(root.path());
        let _ = service.startup_maintenance();
        let context_registry = RuntimeContextRegistry::new();
        let observed = correlation::in_command_scope_with_runtime_context(
            "fixture_command",
            &context_registry,
            Some(serde_json::json!({
                "contextSessionId": context_registry.context_session_id(),
                "interactionId": "int_0123456789abcdef0123456789abcdef"
            })),
            async {
                let parent_correlation_id = correlation::current_or_new();
                let parent_correlation = parent_correlation_id.as_str().to_owned();
                let interaction = correlation::current_interaction();
                let child_service = service.clone();
                tokio::spawn(async move {
                    correlation::in_scope_with_interaction(
                        "task.run",
                        parent_correlation_id,
                        interaction,
                        async move {
                            child_service.record(
                                "runtime.log_event.dropped",
                                Component::Runtime,
                                EventLevel::Warn,
                                EventOutcome::Ok,
                                RuntimeDetail::None,
                            );
                        },
                    )
                    .await;
                })
                .await
                .expect("child joins");
                parent_correlation
            },
        )
        .await;
        service.flush();

        let page = RuntimeLogReader::new(root.path()).read_page(0, 200, 1024 * 1024);
        let event = page
            .lines
            .iter()
            .map(|line| serde_json::from_slice::<RuntimeEvent>(line.as_bytes()))
            .find_map(Result::ok)
            .expect("child event is readable");
        assert_eq!(
            event.correlation_id.as_ref().map(|id| id.as_str()),
            Some(observed.as_str())
        );
        assert_eq!(
            event.interaction_id.as_ref().map(|id| id.as_str()),
            Some("int_0123456789abcdef0123456789abcdef")
        );
    }

    #[test]
    fn queue_budget_reserves_priority_capacity_for_warn_and_error() {
        let budget = QueueBudget {
            queued: AtomicUsize::new(0),
        };
        for _ in 0..RUNTIME_LOG_NORMAL_QUEUE_CAPACITY {
            assert!(budget.try_reserve(false));
        }
        assert!(!budget.try_reserve(false));
        assert!(budget.try_reserve(true));
        assert_eq!(budget.depth(), RUNTIME_LOG_NORMAL_QUEUE_CAPACITY + 1);
        budget.release();
        assert_eq!(budget.depth(), RUNTIME_LOG_NORMAL_QUEUE_CAPACITY);
    }

    #[test]
    fn startup_publishes_current_manifest_and_preserves_previous_snapshot() {
        let root = tempfile::tempdir().expect("runtime root");
        std::fs::write(root.path().join("manifest.json"), b"previous").expect("old snapshot");
        let service = RuntimeLogService::open(root.path());
        let _ = service.startup_maintenance();

        assert_eq!(
            std::fs::read(root.path().join("manifest.previous.json")).expect("previous"),
            b"previous"
        );
        let current = std::fs::read(root.path().join("manifest.json")).expect("current");
        assert!(
            crate::observability::runtime::catalog::Catalog::validate_snapshot(&current).is_some()
        );
    }

    #[test]
    fn startup_pauses_age_retention_until_clock_observation_window_completes() {
        let root = tempfile::tempdir().expect("runtime root");
        let service = RuntimeLogService::open(root.path());
        let (_, retention) = service.startup_maintenance();

        assert!(retention.age_deletion_paused);
        assert!(!service.snapshot().clock_stable);
    }

    #[test]
    fn degraded_service_recovers_after_runtime_lease_is_released() {
        let root = tempfile::tempdir().expect("runtime root");
        let owner = RuntimeLogService::open(root.path());
        assert_eq!(owner.state(), RuntimeLogState::Ready);

        let contender = RuntimeLogService::open(root.path());
        assert_eq!(contender.state(), RuntimeLogState::Degraded);
        drop(owner);

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && contender.state() != RuntimeLogState::Ready {
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(contender.state(), RuntimeLogState::Ready);
        contender.record(
            "runtime.log_event.dropped",
            Component::Runtime,
            EventLevel::Warn,
            EventOutcome::Ok,
            RuntimeDetail::None,
        );
        contender.flush();
        assert!(std::fs::read_dir(root.path())
            .expect("runtime directory")
            .flatten()
            .any(|entry| entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "jsonl")));
    }

    #[test]
    fn clock_observation_after_lease_recovery_is_published_once() {
        let root = tempfile::tempdir().expect("runtime root");
        let owner = RuntimeLogService::open(root.path());
        let contender = RuntimeLogService::open(root.path());
        assert_eq!(contender.state(), RuntimeLogState::Degraded);

        // Simulate a wall-clock discontinuity while this service is waiting
        // for the installation-wide runtime-log lease.
        contender.inject_clock_adjustment();
        drop(owner);

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && contender.state() != RuntimeLogState::Ready {
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(contender.state(), RuntimeLogState::Ready);
        contender.record(
            "runtime.log_event.dropped",
            Component::Runtime,
            EventLevel::Warn,
            EventOutcome::Ok,
            RuntimeDetail::None,
        );
        contender.flush();

        let page = RuntimeLogReader::new(root.path()).read_page(0, 200, 1024 * 1024);
        let clock_events = page
            .lines
            .iter()
            .filter_map(|line| serde_json::from_slice::<RuntimeEvent>(line.as_bytes()).ok())
            .filter(|event| event.event_code.as_str() == "runtime.clock.wall_adjusted")
            .collect::<Vec<_>>();
        assert_eq!(clock_events.len(), 1);
    }

    #[test]
    fn unavailable_log_directory_degrades_without_blocking_record_or_flush() {
        let root = tempfile::tempdir().expect("parent directory");
        let blocked_path = root.path().join("not-a-directory");
        std::fs::write(&blocked_path, b"fixture").expect("blocking file");
        let service = RuntimeLogService::open(&blocked_path);
        assert_eq!(service.state(), RuntimeLogState::Degraded);
        service.record(
            "runtime.log_event.dropped",
            Component::Runtime,
            EventLevel::Warn,
            EventOutcome::Ok,
            RuntimeDetail::None,
        );
        service.flush();
        assert_eq!(service.state(), RuntimeLogState::Degraded);
        assert_eq!(service.queue_counters(), (0, 0));
    }

    #[test]
    fn rejects_events_that_do_not_match_the_catalog_contract() {
        let root = tempfile::tempdir().expect("runtime root");
        let service = RuntimeLogService::open(root.path());
        service.record(
            "runtime.log_event.dropped",
            Component::App,
            EventLevel::Warn,
            EventOutcome::Ok,
            RuntimeDetail::None,
        );
        service.flush();

        assert_eq!(service.state(), RuntimeLogState::Degraded);
        assert_eq!(service.queue_counters(), (0, 1));
        assert!(!std::fs::read_dir(root.path())
            .expect("runtime directory")
            .flatten()
            .any(|entry| entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "jsonl")));
    }

    #[test]
    fn records_clock_adjustment_once_with_closed_detail() {
        let root = tempfile::tempdir().expect("runtime root");
        let service = RuntimeLogService::open(root.path());
        service.inject_clock_adjustment();
        service.record(
            "runtime.log_event.dropped",
            Component::Runtime,
            EventLevel::Warn,
            EventOutcome::Ok,
            RuntimeDetail::None,
        );
        service.record(
            "runtime.log_event.dropped",
            Component::Runtime,
            EventLevel::Warn,
            EventOutcome::Ok,
            RuntimeDetail::None,
        );
        service.flush();

        let page = RuntimeLogReader::new(root.path()).read_page(0, 200, 1024 * 1024);
        let clock_events = page
            .lines
            .iter()
            .filter_map(|line| serde_json::from_slice::<RuntimeEvent>(line.as_bytes()).ok())
            .filter(|event| event.event_code.as_str() == "runtime.clock.wall_adjusted")
            .collect::<Vec<_>>();
        assert_eq!(clock_events.len(), 1);
        assert!(matches!(
            clock_events[0].detail,
            RuntimeDetail::Clock { adjustment }
                if adjustment != super::super::clock::ClockAdjustment::None
        ));
    }

    #[test]
    fn writer_lease_is_exclusive_across_process_restart_and_recovers() {
        if std::env::var_os(CHILD_ROOT).is_some() {
            return;
        }

        let root = tempfile::tempdir().expect("runtime root");
        let ready = root.path().join("child.ready");
        let release = root.path().join("child.release");
        let mut child = spawn_service_child(root.path(), &ready, &release);
        wait_for_child_marker(&ready, &mut child);

        let contender = RuntimeLogService::open(root.path());
        assert_eq!(contender.state(), RuntimeLogState::Degraded);
        contender.record(
            "runtime.log_event.dropped",
            Component::Runtime,
            EventLevel::Warn,
            EventOutcome::Ok,
            RuntimeDetail::None,
        );
        contender.flush();
        assert_eq!(contender.queue_counters(), (0, 0));

        std::fs::write(&release, b"release\n").expect("release child");
        let status = child.wait().expect("wait child");
        assert!(status.success(), "service child exited with {status}");

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && contender.state() != RuntimeLogState::Ready {
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(contender.state(), RuntimeLogState::Ready);
        contender.record(
            "runtime.log_event.dropped",
            Component::Runtime,
            EventLevel::Warn,
            EventOutcome::Ok,
            RuntimeDetail::None,
        );
        contender.flush();

        let page = RuntimeLogReader::new(root.path()).read_page(0, 200, 1024 * 1024);
        assert!(page
            .lines
            .iter()
            .filter_map(|line| serde_json::from_slice::<RuntimeEvent>(line.as_bytes()).ok())
            .any(|event| event.event_code.as_str() == "runtime.log_event.dropped"));
    }

    #[test]
    fn writer_service_child_holds_until_parent_releases() {
        let Some(root) = std::env::var_os(CHILD_ROOT) else {
            return;
        };
        let ready = std::env::var_os(CHILD_READY).expect("child ready path");
        let release = std::env::var_os(CHILD_RELEASE).expect("child release path");
        let root = PathBuf::from(root);
        let service = RuntimeLogService::open(&root);
        assert_eq!(service.state(), RuntimeLogState::Ready);
        service.startup_maintenance();
        service.record(
            "runtime.log_event.dropped",
            Component::Runtime,
            EventLevel::Warn,
            EventOutcome::Ok,
            RuntimeDetail::None,
        );
        service.flush();
        std::fs::write(ready, b"ready\n").expect("child ready");
        for _ in 0..600 {
            if Path::new(&release).exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("parent did not release service child");
    }

    fn spawn_service_child(root: &Path, ready: &Path, release: &Path) -> Child {
        Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "observability::runtime::service::tests::writer_service_child_holds_until_parent_releases",
                "--nocapture",
            ])
            .env(CHILD_ROOT, root)
            .env(CHILD_READY, ready)
            .env(CHILD_RELEASE, release)
            .spawn()
            .expect("spawn service child")
    }

    fn wait_for_child_marker(marker: &Path, child: &mut Child) {
        for _ in 0..200 {
            if marker.exists() {
                return;
            }
            if let Some(status) = child.try_wait().expect("poll child") {
                panic!("service child exited before readiness: {status}");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        panic!("service child did not become ready");
    }
}
