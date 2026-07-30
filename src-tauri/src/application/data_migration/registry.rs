#![allow(
    dead_code,
    reason = "Task 12 publishes typed portable migration registry infrastructure before Task 13+ wire the import/export IPC flows"
)]

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    background_tasks::{
        BoxOperationFuture, OperationContext, OperationId, OperationOwner, OperationRegistry,
        OperationRegistryError, OperationStartRequest, OperationState, OperationTerminal,
    },
    services::{
        data_store::file_identity::FileIdentity,
        portable_migration::limits::PortableMigrationLimitsV1,
    },
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
        Self::with_config(
            operations,
            PortableMigrationLimitsV1::CURRENT,
            PortableMigrationRegistryConfig::default(),
        )
    }

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
        let mut process_hmac_key = [0_u8; 32];
        OsRng.fill_bytes(&mut process_hmac_key);
        Self {
            operations,
            limits,
            inner: Arc::new(Mutex::new(PortableMigrationOperationRegistryInner {
                process_hmac_key,
                config,
                typed_slots: HashMap::new(),
                results: HashMap::new(),
                result_order: VecDeque::new(),
                idempotency: HashMap::new(),
                prepare_owners: HashSet::new(),
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

    pub(crate) fn digest(
        &self,
        input: &PortableIdempotencyDigestInput<'_>,
    ) -> PortableIdempotencyDigest {
        let inner = self
            .inner
            .lock()
            .expect("portable operation registry mutex");
        let passphrase_hmac = keyed_hmac(&inner.process_hmac_key, input.passphrase.as_bytes());
        let mut data = Vec::new();
        data.extend_from_slice(input.kind.operation_kind().as_bytes());
        data.extend_from_slice(b"\0identity\0");
        update_identity(&mut data, input.identity);
        data.extend_from_slice(b"\0options\0");
        input.options.update_digest_input(&mut data);
        data.extend_from_slice(b"\0passphrase-hmac\0");
        data.extend_from_slice(&passphrase_hmac);
        PortableIdempotencyDigest(keyed_hmac(&inner.process_hmac_key, &data))
    }

    pub(crate) fn reserve_idempotency(
        &self,
        key: impl Into<String>,
        digest: PortableIdempotencyDigest,
        operation_id: OperationId,
    ) -> Result<IdempotencyReservation, PortableMigrationRegistryError> {
        let key = key.into();
        let mut inner = self
            .inner
            .lock()
            .expect("portable operation registry mutex");
        match inner.idempotency.get(&key) {
            Some(binding) if binding.digest == digest => Ok(IdempotencyReservation::Existing {
                operation_id: binding.operation_id,
            }),
            Some(_) => Err(PortableMigrationRegistryError::IdempotencyConflict),
            None => {
                inner.idempotency.insert(
                    key,
                    IdempotencyBinding {
                        digest,
                        operation_id,
                    },
                );
                Ok(IdempotencyReservation::Reserved)
            }
        }
    }

    pub(crate) fn try_claim_prepare_owner(
        &self,
        digest: PortableIdempotencyDigest,
    ) -> Result<PrepareOwnerGuard, PortableMigrationRegistryError> {
        let mut inner = self
            .inner
            .lock()
            .expect("portable operation registry mutex");
        if !inner.prepare_owners.insert(digest) {
            return Err(PortableMigrationRegistryError::PrepareAlreadyOwned);
        }
        Ok(PrepareOwnerGuard {
            digest,
            registry: Arc::clone(&self.inner),
            released: false,
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
    process_hmac_key: [u8; 32],
    config: PortableMigrationRegistryConfig,
    typed_slots: HashMap<OperationId, TypedOperationSlot>,
    results: HashMap<OperationId, TerminalResultEntry>,
    result_order: VecDeque<OperationId>,
    idempotency: HashMap<String, IdempotencyBinding>,
    prepare_owners: HashSet<PortableIdempotencyDigest>,
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

#[derive(Clone, Debug)]
struct IdempotencyBinding {
    digest: PortableIdempotencyDigest,
    operation_id: OperationId,
}

#[derive(Debug)]
pub(crate) struct PrepareOwnerGuard {
    digest: PortableIdempotencyDigest,
    registry: Arc<Mutex<PortableMigrationOperationRegistryInner>>,
    released: bool,
}

impl PrepareOwnerGuard {
    pub(crate) fn release(mut self) {
        self.release_inner();
    }

    fn release_inner(&mut self) {
        if self.released {
            return;
        }
        self.registry
            .lock()
            .expect("portable operation registry mutex")
            .prepare_owners
            .remove(&self.digest);
        self.released = true;
    }
}

impl Drop for PrepareOwnerGuard {
    fn drop(&mut self) {
        self.release_inner();
    }
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

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) struct PortableIdempotencyDigest([u8; 32]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum IdempotencyReservation {
    Reserved,
    Existing { operation_id: OperationId },
}

pub(crate) struct PortableIdempotencyDigestInput<'a> {
    pub(crate) kind: PortableOperationKind,
    pub(crate) identity: &'a FileIdentity,
    pub(crate) options: PortableCommandOptions,
    pub(crate) passphrase: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PortableCommandOptions {
    Export {
        include_history: bool,
        overwrite_existing: bool,
        confirmation_matched: bool,
    },
    Inspect,
    PrepareImport {
        mode: PortableImportMode,
    },
}

impl PortableCommandOptions {
    fn update_digest_input(&self, data: &mut Vec<u8>) {
        match self {
            Self::Export {
                include_history,
                overwrite_existing,
                confirmation_matched,
            } => {
                data.extend_from_slice(b"export");
                data.extend_from_slice(&[*include_history as u8]);
                data.extend_from_slice(&[*overwrite_existing as u8]);
                data.extend_from_slice(&[*confirmation_matched as u8]);
            }
            Self::Inspect => data.extend_from_slice(b"inspect"),
            Self::PrepareImport { mode } => {
                data.extend_from_slice(b"prepare_import");
                data.extend_from_slice(mode.as_bytes());
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PortableImportMode {
    Merge,
    Replace,
}

impl PortableImportMode {
    fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Merge => b"merge",
            Self::Replace => b"replace",
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum PortableMigrationRegistryError {
    #[error("operation registry error")]
    Operation(#[from] OperationRegistryError),
    #[error("operation owner mismatch")]
    OwnerMismatch,
    #[error("operation not found")]
    NotFound,
    #[error("operation completed but terminal result is missing")]
    CompletedResultMissing,
    #[error("idempotency key is already bound to different input")]
    IdempotencyConflict,
    #[error("prepare operation is already owned")]
    PrepareAlreadyOwned,
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

fn keyed_hmac(key: &[u8; 32], value: &[u8]) -> [u8; 32] {
    hmac_sha256(key, value)
}

fn hmac_sha256(key: &[u8], value: &[u8]) -> [u8; 32] {
    let mut ipad = [0x36_u8; 64];
    let mut opad = [0x5c_u8; 64];
    let key_digest;
    let normalized_key = if key.len() > 64 {
        key_digest = Sha256::digest(key);
        key_digest.as_slice()
    } else {
        key
    };
    for (index, byte) in normalized_key.iter().enumerate() {
        ipad[index] ^= byte;
        opad[index] ^= byte;
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(value);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    outer.finalize().into()
}

fn update_identity(data: &mut Vec<u8>, identity: &FileIdentity) {
    data.extend_from_slice(&identity.volume_serial.unwrap_or(0).to_be_bytes());
    data.extend_from_slice(&[identity.volume_serial.is_some() as u8]);
    data.extend_from_slice(&identity.file_id.unwrap_or(0).to_be_bytes());
    data.extend_from_slice(&[identity.file_id.is_some() as u8]);
    data.extend_from_slice(&identity.length.to_be_bytes());
    data.extend_from_slice(identity.sha256.as_bytes());
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

    #[test]
    fn idempotency_digest_includes_kind_identity_options_and_passphrase_hmac() {
        let facade = facade();
        let identity = identity("file");
        let export = PortableIdempotencyDigestInput {
            kind: PortableOperationKind::ExportPackage,
            identity: &identity,
            options: PortableCommandOptions::Export {
                include_history: true,
                overwrite_existing: false,
                confirmation_matched: true,
            },
            passphrase: "secret",
        };
        let same = facade.digest(&export);
        let different_kind = facade.digest(&PortableIdempotencyDigestInput {
            kind: PortableOperationKind::InspectPackage,
            identity: &identity,
            options: PortableCommandOptions::Inspect,
            passphrase: "secret",
        });
        let different_passphrase = facade.digest(&PortableIdempotencyDigestInput {
            kind: PortableOperationKind::ExportPackage,
            identity: &identity,
            options: PortableCommandOptions::Export {
                include_history: true,
                overwrite_existing: false,
                confirmation_matched: true,
            },
            passphrase: "other",
        });

        assert_eq!(same, facade.digest(&export));
        assert_ne!(same, different_kind);
        assert_ne!(same, different_passphrase);

        let operation_id = OperationId::from_u64(7).expect("operation id");
        assert_eq!(
            facade
                .reserve_idempotency("idem", same, operation_id)
                .expect("reserve"),
            IdempotencyReservation::Reserved
        );
        assert_eq!(
            facade
                .reserve_idempotency("idem", same, operation_id)
                .expect("same binding"),
            IdempotencyReservation::Existing { operation_id }
        );
        assert_eq!(
            facade
                .reserve_idempotency("idem", different_kind, operation_id)
                .unwrap_err(),
            PortableMigrationRegistryError::IdempotencyConflict
        );
    }

    #[test]
    fn concurrent_prepare_claim_has_exactly_one_owner() {
        let facade = facade();
        let digest = facade.digest(&PortableIdempotencyDigestInput {
            kind: PortableOperationKind::PrepareImport,
            identity: &identity("prepare"),
            options: PortableCommandOptions::PrepareImport {
                mode: PortableImportMode::Merge,
            },
            passphrase: "secret",
        });

        let first = facade.try_claim_prepare_owner(digest).expect("first owner");
        assert_eq!(
            facade.try_claim_prepare_owner(digest).unwrap_err(),
            PortableMigrationRegistryError::PrepareAlreadyOwned
        );
        first.release();
        let second = facade
            .try_claim_prepare_owner(digest)
            .expect("released owner");
        drop(second);
    }

    #[test]
    fn local_hmac_sha256_matches_rfc4231_vector() {
        let key = [0x0b_u8; 20];
        let digest = hmac_sha256(&key, b"Hi There");

        assert_eq!(
            hex(&digest),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
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

    fn identity(seed: &str) -> FileIdentity {
        FileIdentity {
            volume_serial: None,
            file_id: None,
            length: seed.len() as u64,
            sha256: format!("{:x}", sha2::Sha256::digest(seed.as_bytes())),
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
