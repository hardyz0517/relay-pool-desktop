use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{
    background_tasks::{
        BoxOperationFuture, OperationContext, OperationId, OperationOwner, OperationRegistry,
        OperationRegistryError, OperationStartRequest, OperationState, OperationTerminal,
    },
    services::portable_migration::limits::PortableMigrationLimitsV1,
};

const OWNER_FEATURE: &str = "portable-data-migration";
const RESULT_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_TERMINAL_RESULTS: usize = 64;
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(250);
const PROGRESS_MIN_PERCENT_DELTA: u8 = 1;

#[derive(Clone)]
pub(crate) struct PortableMigrationOperationRegistry {
    operations: OperationRegistry,
    limits: PortableMigrationLimitsV1,
    inner: Arc<Mutex<PortableMigrationOperationRegistryInner>>,
}

impl PortableMigrationOperationRegistry {
    pub(crate) fn new(operations: OperationRegistry) -> Self {
        let limits = PortableMigrationLimitsV1::CURRENT;
        let config = PortableMigrationRegistryConfig::default();
        assert!(
            !config.terminal_result_ttl.is_zero(),
            "terminal result TTL must be positive"
        );
        assert!(
            config.terminal_result_max_entries > 0,
            "terminal result capacity must be positive"
        );
        Self {
            operations,
            limits,
            inner: Arc::new(Mutex::new(PortableMigrationOperationRegistryInner {
                config,
                typed_slots: HashMap::new(),
                results: HashMap::new(),
                result_order: VecDeque::new(),
            })),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_config(
        operations: OperationRegistry,
        limits: PortableMigrationLimitsV1,
        config: PortableMigrationRegistryConfig,
    ) -> Self {
        assert!(
            !config.terminal_result_ttl.is_zero(),
            "terminal result TTL must be positive"
        );
        assert!(
            config.terminal_result_max_entries > 0,
            "terminal result capacity must be positive"
        );
        Self {
            operations,
            limits,
            inner: Arc::new(Mutex::new(PortableMigrationOperationRegistryInner {
                config,
                typed_slots: HashMap::new(),
                results: HashMap::new(),
                result_order: VecDeque::new(),
            })),
        }
    }

    pub(crate) fn start_portable_operation<F>(
        &self,
        kind: PortableOperationKind,
        concurrency_key: Option<String>,
        body: F,
    ) -> Result<OperationId, PortableMigrationRegistryError>
    where
        F: FnOnce(OperationContext) -> BoxOperationFuture + Send + 'static,
    {
        let mut request = OperationStartRequest::new(
            kind.operation_kind(),
            OperationOwner::new(OWNER_FEATURE),
            body,
        )
        .with_deadline(kind.deadline(self.limits));
        if let Some(key) = concurrency_key {
            request = request.with_concurrency_key(key);
        }
        let id = self.operations.start(request)?;
        let mut inner = self
            .inner
            .lock()
            .expect("portable operation registry mutex");
        inner.typed_slots.insert(
            id,
            TypedOperationSlot {
                kind,
                progress: VecDeque::new(),
                last_progress: None,
            },
        );
        Ok(id)
    }

    pub(crate) fn emit_progress_at(
        &self,
        id: OperationId,
        progress: PortableMigrationProgress,
        now: Instant,
    ) -> Result<bool, PortableMigrationRegistryError> {
        let encoded = serde_json::to_string(&progress)
            .map_err(|_| PortableMigrationRegistryError::InvalidProgress)?;
        let mut inner = self
            .inner
            .lock()
            .expect("portable operation registry mutex");
        let slot = inner
            .typed_slots
            .get_mut(&id)
            .ok_or(PortableMigrationRegistryError::NotFound)?;
        if !should_emit(slot.last_progress.as_ref(), &progress, now) {
            return Ok(false);
        }
        self.operations.progress(id, encoded)?;
        if slot.progress.len() == 128 {
            slot.progress.pop_front();
        }
        slot.progress.push_back(progress.clone());
        slot.last_progress = Some(LastPortableProgress { progress, at: now });
        Ok(true)
    }

    pub(crate) fn record_terminal_result_at(
        &self,
        id: OperationId,
        result: PortableMigrationTerminalResult,
        now: Instant,
    ) -> Result<(), PortableMigrationRegistryError> {
        let mut inner = self
            .inner
            .lock()
            .expect("portable operation registry mutex");
        gc_results_locked(&mut inner, now);
        if !inner.typed_slots.contains_key(&id) {
            return Err(PortableMigrationRegistryError::NotFound);
        }
        inner.results.insert(
            id,
            TerminalResultEntry {
                result,
                recorded_at: now,
            },
        );
        inner.result_order.push_back(id);
        while inner.result_order.len() > inner.config.terminal_result_max_entries {
            if let Some(expired) = inner.result_order.pop_front() {
                inner.results.remove(&expired);
            }
        }
        Ok(())
    }

    pub(crate) fn get_portable_migration_operation(
        &self,
        id: OperationId,
        now: Instant,
    ) -> Result<PortableMigrationOperationSnapshot, PortableMigrationRegistryError> {
        let operation = self.operations.status(id)?;
        if operation.owner.feature != OWNER_FEATURE {
            return Err(PortableMigrationRegistryError::OwnerMismatch);
        }
        let mut inner = self
            .inner
            .lock()
            .expect("portable operation registry mutex");
        gc_results_locked(&mut inner, now);
        let typed = inner
            .typed_slots
            .get(&id)
            .ok_or(PortableMigrationRegistryError::OwnerMismatch)?;
        let terminal = match operation.terminal.clone() {
            Some(OperationTerminal::Completed) => {
                match inner.results.get(&id).map(|entry| entry.result.clone()) {
                    Some(result) => Some(PortableMigrationTerminal::Completed { result }),
                    None => Some(PortableMigrationTerminal::ResultUnknown),
                }
            }
            Some(OperationTerminal::Failed { code }) => Some(PortableMigrationTerminal::Failed {
                code: code.as_str().to_string(),
            }),
            Some(OperationTerminal::Cancelled) => Some(PortableMigrationTerminal::Cancelled),
            Some(OperationTerminal::TimedOut) => Some(PortableMigrationTerminal::TimedOut),
            Some(OperationTerminal::ResultUnknown) => {
                Some(PortableMigrationTerminal::ResultUnknown)
            }
            None => None,
        };
        Ok(PortableMigrationOperationSnapshot {
            operation_id: id,
            kind: typed.kind,
            state: PortableMigrationOperationState::from(&operation.state),
            deadline: operation.deadline,
            progress: typed.progress.iter().cloned().collect(),
            terminal,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PortableMigrationRegistryConfig {
    pub(crate) terminal_result_ttl: Duration,
    pub(crate) terminal_result_max_entries: usize,
}

impl Default for PortableMigrationRegistryConfig {
    fn default() -> Self {
        Self {
            terminal_result_ttl: RESULT_TTL,
            terminal_result_max_entries: MAX_TERMINAL_RESULTS,
        }
    }
}

#[derive(Debug)]
struct PortableMigrationOperationRegistryInner {
    config: PortableMigrationRegistryConfig,
    typed_slots: HashMap<OperationId, TypedOperationSlot>,
    results: HashMap<OperationId, TerminalResultEntry>,
    result_order: VecDeque<OperationId>,
}

#[derive(Debug)]
struct TypedOperationSlot {
    kind: PortableOperationKind,
    progress: VecDeque<PortableMigrationProgress>,
    last_progress: Option<LastPortableProgress>,
}

#[derive(Debug)]
struct LastPortableProgress {
    progress: PortableMigrationProgress,
    at: Instant,
}

#[derive(Debug)]
struct TerminalResultEntry {
    result: PortableMigrationTerminalResult,
    recorded_at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PortableOperationKind {
    ExportPackage,
    InspectPackage,
    PrepareImport,
}

impl PortableOperationKind {
    fn operation_kind(self) -> &'static str {
        match self {
            Self::ExportPackage => "portable_migration.export_package",
            Self::InspectPackage => "portable_migration.inspect_package",
            Self::PrepareImport => "portable_migration.prepare_import",
        }
    }

    fn deadline(self, limits: PortableMigrationLimitsV1) -> Duration {
        match self {
            Self::ExportPackage => limits.export_deadline(),
            Self::InspectPackage => limits.inspection_deadline(),
            Self::PrepareImport => limits.prepare_deadline(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub(crate) enum PortableMigrationProgress {
    Queued,
    KdfStarted,
    KdfFinished,
    ReadingPackage { percent: u8, bytes_read: u64 },
    WritingDatabase { percent: u8, rows_written: u64 },
    PublishingPackage { percent: u8, bytes_written: u64 },
    VerifyingPackage,
}

impl PortableMigrationProgress {
    fn phase_key(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::KdfStarted => "kdf_started",
            Self::KdfFinished => "kdf_finished",
            Self::ReadingPackage { .. } => "reading_package",
            Self::WritingDatabase { .. } => "writing_database",
            Self::PublishingPackage { .. } => "publishing_package",
            Self::VerifyingPackage => "verifying_package",
        }
    }

    fn percent(&self) -> Option<u8> {
        match self {
            Self::ReadingPackage { percent, .. }
            | Self::WritingDatabase { percent, .. }
            | Self::PublishingPackage { percent, .. } => Some(*percent),
            Self::Queued | Self::KdfStarted | Self::KdfFinished | Self::VerifyingPackage => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub(crate) enum PortableMigrationTerminalResult {
    ExportedPackage {
        export_id: String,
        package_size_bytes: u64,
    },
    InspectedPackage {
        export_id: String,
        source_platform: String,
        included_categories: Vec<String>,
        sqlite_size_bytes: u64,
    },
    PreparedImport {
        export_id: String,
        target_rows: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "terminal", rename_all = "snake_case")]
pub(crate) enum PortableMigrationTerminal {
    Completed {
        result: PortableMigrationTerminalResult,
    },
    Failed {
        code: String,
    },
    Cancelled,
    TimedOut,
    ResultUnknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PortableMigrationOperationState {
    Running,
    Stopping,
    Terminal,
}

impl From<&OperationState> for PortableMigrationOperationState {
    fn from(state: &OperationState) -> Self {
        match state {
            OperationState::Running => Self::Running,
            OperationState::Stopping => Self::Stopping,
            OperationState::Terminal { .. } => Self::Terminal,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PortableMigrationOperationSnapshot {
    pub(crate) operation_id: OperationId,
    pub(crate) kind: PortableOperationKind,
    pub(crate) state: PortableMigrationOperationState,
    pub(crate) deadline: Duration,
    pub(crate) progress: Vec<PortableMigrationProgress>,
    pub(crate) terminal: Option<PortableMigrationTerminal>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum PortableMigrationRegistryError {
    #[error("operation registry error")]
    Operation(#[from] OperationRegistryError),
    #[error("operation owner mismatch")]
    OwnerMismatch,
    #[error("operation not found")]
    NotFound,
    #[error("portable migration progress is invalid")]
    InvalidProgress,
}

fn should_emit(
    last: Option<&LastPortableProgress>,
    next: &PortableMigrationProgress,
    now: Instant,
) -> bool {
    let Some(last) = last else {
        return true;
    };
    if last.progress.phase_key() != next.phase_key() {
        return true;
    }
    if matches!(
        next,
        PortableMigrationProgress::KdfStarted | PortableMigrationProgress::KdfFinished
    ) {
        return true;
    }
    match (last.progress.percent(), next.percent()) {
        (Some(previous), Some(current)) => {
            now.duration_since(last.at) >= PROGRESS_MIN_INTERVAL
                && current.saturating_sub(previous) >= PROGRESS_MIN_PERCENT_DELTA
        }
        _ => false,
    }
}

fn gc_results_locked(inner: &mut PortableMigrationOperationRegistryInner, now: Instant) {
    let ttl = inner.config.terminal_result_ttl;
    let expired = inner
        .results
        .iter()
        .filter_map(|(id, entry)| (now.duration_since(entry.recorded_at) >= ttl).then_some(*id))
        .collect::<Vec<_>>();
    for id in expired {
        inner.results.remove(&id);
    }
    inner
        .result_order
        .retain(|id| inner.results.contains_key(id));
}

#[cfg(test)]
mod tests {
    use std::future;

    use static_assertions::assert_impl_all;

    use super::*;
    use crate::background_tasks::OperationRegistryConfig;

    #[tokio::test(start_paused = true)]
    async fn portable_operations_use_explicit_deadlines_by_kind() {
        let facade = facade();
        let now = Instant::now();

        let export = facade
            .start_portable_operation(PortableOperationKind::ExportPackage, None, pending_body)
            .expect("export");
        let inspect = facade
            .start_portable_operation(PortableOperationKind::InspectPackage, None, pending_body)
            .expect("inspect");
        let prepare = facade
            .start_portable_operation(PortableOperationKind::PrepareImport, None, pending_body)
            .expect("prepare");

        assert_eq!(
            facade
                .get_portable_migration_operation(export, now)
                .expect("export status")
                .deadline,
            Duration::from_secs(2 * 60 * 60)
        );
        assert_eq!(
            facade
                .get_portable_migration_operation(inspect, now)
                .expect("inspect status")
                .deadline,
            Duration::from_secs(30 * 60)
        );
        assert_eq!(
            facade
                .get_portable_migration_operation(prepare, now)
                .expect("prepare status")
                .deadline,
            Duration::from_secs(2 * 60 * 60)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn progress_is_typed_and_throttled_without_free_text_or_fake_kdf_percent() {
        let facade = facade();
        let id = facade
            .start_portable_operation(PortableOperationKind::InspectPackage, None, pending_body)
            .expect("start");
        let now = Instant::now();

        assert!(facade
            .emit_progress_at(
                id,
                PortableMigrationProgress::ReadingPackage {
                    percent: 1,
                    bytes_read: 10
                },
                now,
            )
            .expect("first"));
        assert!(!facade
            .emit_progress_at(
                id,
                PortableMigrationProgress::ReadingPackage {
                    percent: 2,
                    bytes_read: 20
                },
                now + Duration::from_millis(249),
            )
            .expect("too soon"));
        assert!(!facade
            .emit_progress_at(
                id,
                PortableMigrationProgress::ReadingPackage {
                    percent: 1,
                    bytes_read: 30
                },
                now + Duration::from_millis(250),
            )
            .expect("too small"));
        assert!(facade
            .emit_progress_at(
                id,
                PortableMigrationProgress::ReadingPackage {
                    percent: 2,
                    bytes_read: 40
                },
                now + Duration::from_millis(250),
            )
            .expect("enough"));
        assert!(facade
            .emit_progress_at(id, PortableMigrationProgress::KdfStarted, now,)
            .expect("kdf start"));
        assert!(facade
            .emit_progress_at(id, PortableMigrationProgress::KdfFinished, now,)
            .expect("kdf finish"));

        let snapshot = facade
            .get_portable_migration_operation(id, now)
            .expect("snapshot");
        assert_eq!(snapshot.progress.len(), 4);
        assert_impl_all!(PortableMigrationProgress: Serialize);
        assert!(matches!(
            snapshot.progress[2],
            PortableMigrationProgress::KdfStarted
        ));
        assert!(snapshot.progress[2].percent().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn terminal_result_ttl_capacity_and_missing_result_are_checked() {
        let facade = PortableMigrationOperationRegistry::with_config(
            OperationRegistry::new(OperationRegistryConfig::architecture_budget()),
            PortableMigrationLimitsV1::CURRENT,
            PortableMigrationRegistryConfig {
                terminal_result_ttl: Duration::from_secs(30 * 60),
                terminal_result_max_entries: 2,
            },
        );
        let now = Instant::now();
        let completed = facade
            .start_portable_operation(PortableOperationKind::ExportPackage, None, completed_body)
            .expect("completed");
        tokio::task::yield_now().await;
        assert!(matches!(
            facade
                .get_portable_migration_operation(completed, now)
                .expect("completed without typed result")
                .terminal,
            Some(PortableMigrationTerminal::ResultUnknown)
        ));

        facade
            .record_terminal_result_at(
                completed,
                PortableMigrationTerminalResult::ExportedPackage {
                    export_id: "export-1".to_string(),
                    package_size_bytes: 10,
                },
                now,
            )
            .expect("record result");
        let snapshot = facade
            .get_portable_migration_operation(completed, now)
            .expect("completed result");
        assert!(matches!(
            snapshot.terminal,
            Some(PortableMigrationTerminal::Completed { .. })
        ));

        let second = facade
            .start_portable_operation(PortableOperationKind::InspectPackage, None, completed_body)
            .expect("second");
        let third = facade
            .start_portable_operation(PortableOperationKind::PrepareImport, None, completed_body)
            .expect("third");
        tokio::task::yield_now().await;
        facade
            .record_terminal_result_at(
                second,
                PortableMigrationTerminalResult::InspectedPackage {
                    export_id: "export-2".to_string(),
                    source_platform: "windows".to_string(),
                    included_categories: vec!["core_data".to_string()],
                    sqlite_size_bytes: 20,
                },
                now,
            )
            .expect("second result");
        facade
            .record_terminal_result_at(
                third,
                PortableMigrationTerminalResult::PreparedImport {
                    export_id: "export-3".to_string(),
                    target_rows: 3,
                },
                now,
            )
            .expect("third result");
        assert_eq!(
            facade
                .get_portable_migration_operation(completed, now)
                .expect("evicted completed result")
                .terminal,
            Some(PortableMigrationTerminal::ResultUnknown)
        );
        assert_eq!(
            facade
                .get_portable_migration_operation(second, now + Duration::from_secs(30 * 60))
                .expect("expired completed result")
                .terminal,
            Some(PortableMigrationTerminal::ResultUnknown)
        );
    }

    fn facade() -> PortableMigrationOperationRegistry {
        PortableMigrationOperationRegistry::new(OperationRegistry::new(
            OperationRegistryConfig::architecture_budget(),
        ))
    }

    fn pending_body(_context: OperationContext) -> BoxOperationFuture {
        Box::pin(future::pending())
    }

    fn completed_body(_context: OperationContext) -> BoxOperationFuture {
        Box::pin(async { OperationTerminal::Completed })
    }
}
