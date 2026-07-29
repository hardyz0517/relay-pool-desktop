#![allow(
    dead_code,
    reason = "Task 12 publishes path token infrastructure before Task 13+ wire the import/export IPC flows"
)]

use std::{
    collections::{HashMap, VecDeque},
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{self, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rand::{rngs::OsRng, RngCore};

use crate::services::data_store::{
    atomic_file::{ApprovedLeaf, AtomicFileError, PublishMode},
    file_identity::{identity_for_file, identity_for_path, FileIdentity, FileIdentityError},
};

const TOKEN_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_TOKENS_PER_KIND: usize = 64;

#[derive(Clone, Debug)]
pub(crate) struct PathTokenRegistry {
    inner: Arc<Mutex<PathTokenRegistryInner>>,
    config: PathTokenRegistryConfig,
}

impl PathTokenRegistry {
    pub(crate) fn new() -> Self {
        Self::with_config(PathTokenRegistryConfig::default())
    }

    pub(crate) fn with_config(config: PathTokenRegistryConfig) -> Self {
        assert!(!config.ttl.is_zero(), "path token TTL must be positive");
        assert!(
            config.max_per_kind > 0,
            "path token capacity must be positive"
        );
        let mut process_nonce = [0_u8; 16];
        OsRng.fill_bytes(&mut process_nonce);
        Self {
            inner: Arc::new(Mutex::new(PathTokenRegistryInner {
                process_nonce,
                import: TokenBucket::default(),
                export: TokenBucket::default(),
            })),
            config,
        }
    }

    pub(crate) fn approve_import_path(
        &self,
        path: impl AsRef<Path>,
        now: Instant,
    ) -> Result<ImportPathToken, PathTokenError> {
        let path = path.as_ref();
        let canonical_path = path
            .canonicalize()
            .map_err(|_| PathTokenError::OpenFailed)?;
        let mut file =
            open_import_guard(&canonical_path).map_err(|_| PathTokenError::OpenFailed)?;
        let identity = identity_for_file(&mut file)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|_| PathTokenError::OpenFailed)?;

        let mut inner = self.inner.lock().expect("path token registry mutex");
        let id = PathTokenId::new(PathTokenKind::Import, inner.process_nonce);
        gc_bucket(&mut inner.import, now, self.config.ttl);
        evict_to_capacity(
            &mut inner.import,
            self.config.max_per_kind.saturating_sub(1),
        );
        inner.import.entries.insert(
            id.clone(),
            ImportPathEntry {
                file: Some(file),
                canonical_path,
                identity: identity.clone(),
                expires_at: now + self.config.ttl,
                consumed: false,
            },
        );
        inner.import.order.push_back(id.clone());
        Ok(ImportPathToken { id, identity })
    }

    pub(crate) fn approve_export_path(
        &self,
        parent: impl AsRef<Path>,
        leaf: impl Into<OsString>,
        overwrite_existing: bool,
        now: Instant,
    ) -> Result<ExportPathToken, PathTokenError> {
        let approved_leaf = ApprovedLeaf::approve(parent.as_ref(), leaf.into())?;
        let parent_guard =
            open_parent_guard(parent.as_ref()).map_err(|_| PathTokenError::OpenFailed)?;
        let target_state = match (approved_leaf.path().exists(), overwrite_existing) {
            (false, _) => ExportTargetState::Absent,
            (true, true) => ExportTargetState::Existing(identity_for_path(&approved_leaf.path())?),
            (true, false) => return Err(PathTokenError::AlreadyExists),
        };
        let mode = if overwrite_existing {
            PublishMode::ReplaceExisting
        } else {
            PublishMode::CreateNew
        };

        let mut inner = self.inner.lock().expect("path token registry mutex");
        let id = PathTokenId::new(PathTokenKind::Export, inner.process_nonce);
        gc_bucket(&mut inner.export, now, self.config.ttl);
        evict_to_capacity(
            &mut inner.export,
            self.config.max_per_kind.saturating_sub(1),
        );
        inner.export.entries.insert(
            id.clone(),
            ExportPathEntry {
                parent_guard: Some(parent_guard),
                approved_leaf: approved_leaf.clone(),
                target_state: target_state.clone(),
                mode,
                expires_at: now + self.config.ttl,
                consumed: false,
            },
        );
        inner.export.order.push_back(id.clone());
        Ok(ExportPathToken {
            id,
            approved_path: approved_leaf.path(),
            target_state,
            mode,
        })
    }

    pub(crate) fn consume_import(
        &self,
        token: &PathTokenId,
        now: Instant,
    ) -> Result<ImportPathLease, PathTokenError> {
        if token.kind != PathTokenKind::Import {
            return Err(PathTokenError::TypeMismatch);
        }
        let mut inner = self.inner.lock().expect("path token registry mutex");
        validate_nonce(token, inner.process_nonce)?;
        let entry = inner
            .import
            .entries
            .get_mut(token)
            .ok_or(PathTokenError::NotFound)?;
        if entry.consumed {
            return Err(PathTokenError::Consumed);
        }
        if now >= entry.expires_at {
            entry.consumed = true;
            entry.file.take();
            return Err(PathTokenError::Expired);
        }
        let mut file = entry.file.take().ok_or(PathTokenError::Consumed)?;
        let current_identity = identity_for_file(&mut file)?;
        if current_identity != entry.identity {
            entry.consumed = true;
            return Err(PathTokenError::SelectedFileChanged);
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|_| PathTokenError::OpenFailed)?;
        entry.consumed = true;
        Ok(ImportPathLease {
            token_id: token.clone(),
            file,
            canonical_path: entry.canonical_path.clone(),
            identity: entry.identity.clone(),
        })
    }

    pub(crate) fn consume_export(
        &self,
        token: &PathTokenId,
        now: Instant,
    ) -> Result<ExportPathLease, PathTokenError> {
        if token.kind != PathTokenKind::Export {
            return Err(PathTokenError::TypeMismatch);
        }
        let mut inner = self.inner.lock().expect("path token registry mutex");
        validate_nonce(token, inner.process_nonce)?;
        let entry = inner
            .export
            .entries
            .get_mut(token)
            .ok_or(PathTokenError::NotFound)?;
        if entry.consumed {
            return Err(PathTokenError::Consumed);
        }
        if now >= entry.expires_at {
            entry.consumed = true;
            entry.parent_guard.take();
            return Err(PathTokenError::Expired);
        }
        verify_target_state(&entry.approved_leaf, &entry.target_state)?;
        entry.consumed = true;
        Ok(ExportPathLease {
            token_id: token.clone(),
            _parent_guard: entry.parent_guard.take().ok_or(PathTokenError::Consumed)?,
            approved_leaf: entry.approved_leaf.clone(),
            target_state: entry.target_state.clone(),
            mode: entry.mode,
        })
    }

    pub(crate) fn gc(&self, now: Instant) {
        let mut inner = self.inner.lock().expect("path token registry mutex");
        gc_bucket(&mut inner.import, now, self.config.ttl);
        gc_bucket(&mut inner.export, now, self.config.ttl);
    }

    #[cfg(test)]
    fn process_nonce_for_test(&self) -> [u8; 16] {
        self.inner
            .lock()
            .expect("path token registry mutex")
            .process_nonce
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PathTokenRegistryConfig {
    pub(crate) ttl: Duration,
    pub(crate) max_per_kind: usize,
}

impl Default for PathTokenRegistryConfig {
    fn default() -> Self {
        Self {
            ttl: TOKEN_TTL,
            max_per_kind: MAX_TOKENS_PER_KIND,
        }
    }
}

#[derive(Debug)]
struct PathTokenRegistryInner {
    process_nonce: [u8; 16],
    import: TokenBucket<ImportPathEntry>,
    export: TokenBucket<ExportPathEntry>,
}

#[derive(Debug)]
struct TokenBucket<T> {
    entries: HashMap<PathTokenId, T>,
    order: VecDeque<PathTokenId>,
}

impl<T> Default for TokenBucket<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }
}

#[derive(Debug)]
struct ImportPathEntry {
    file: Option<File>,
    canonical_path: PathBuf,
    identity: FileIdentity,
    expires_at: Instant,
    consumed: bool,
}

#[derive(Debug)]
struct ExportPathEntry {
    parent_guard: Option<File>,
    approved_leaf: ApprovedLeaf,
    target_state: ExportTargetState,
    mode: PublishMode,
    expires_at: Instant,
    consumed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportPathToken {
    pub(crate) id: PathTokenId,
    pub(crate) identity: FileIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportPathToken {
    pub(crate) id: PathTokenId,
    pub(crate) approved_path: PathBuf,
    pub(crate) target_state: ExportTargetState,
    pub(crate) mode: PublishMode,
}

#[derive(Debug)]
pub(crate) struct ImportPathLease {
    pub(crate) token_id: PathTokenId,
    pub(crate) file: File,
    pub(crate) canonical_path: PathBuf,
    pub(crate) identity: FileIdentity,
}

#[derive(Debug)]
pub(crate) struct ExportPathLease {
    pub(crate) token_id: PathTokenId,
    _parent_guard: File,
    pub(crate) approved_leaf: ApprovedLeaf,
    pub(crate) target_state: ExportTargetState,
    pub(crate) mode: PublishMode,
}

impl ExportPathLease {
    pub(crate) fn approved_leaf(&self) -> &ApprovedLeaf {
        &self.approved_leaf
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExportTargetState {
    Absent,
    Existing(FileIdentity),
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct PathTokenId {
    kind: PathTokenKind,
    process_nonce: [u8; 16],
    value: String,
}

impl PathTokenId {
    fn new(kind: PathTokenKind, process_nonce: [u8; 16]) -> Self {
        Self {
            kind,
            process_nonce,
            value: uuid::Uuid::now_v7().to_string(),
        }
    }

    pub(crate) fn kind(&self) -> PathTokenKind {
        self.kind
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.value
    }

    #[cfg(test)]
    fn with_process_nonce_for_test(mut self, process_nonce: [u8; 16]) -> Self {
        self.process_nonce = process_nonce;
        self
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum PathTokenKind {
    Import,
    Export,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum PathTokenError {
    #[error("path token was not created by this process")]
    ProcessMismatch,
    #[error("path token type does not match this operation")]
    TypeMismatch,
    #[error("path token not found")]
    NotFound,
    #[error("path token expired")]
    Expired,
    #[error("path token was already consumed")]
    Consumed,
    #[error("selected file changed after approval")]
    SelectedFileChanged,
    #[error("selected target already exists")]
    AlreadyExists,
    #[error("selected path was rejected")]
    PathRejected,
    #[error("selected file could not be opened")]
    OpenFailed,
    #[error("selected file identity could not be read")]
    IdentityFailed,
}

impl From<AtomicFileError> for PathTokenError {
    fn from(error: AtomicFileError) -> Self {
        match error {
            AtomicFileError::AlreadyExists => Self::AlreadyExists,
            AtomicFileError::Identity(_) => Self::IdentityFailed,
            AtomicFileError::PathRejected | AtomicFileError::Missing | AtomicFileError::Io(_) => {
                Self::PathRejected
            }
        }
    }
}

impl From<FileIdentityError> for PathTokenError {
    fn from(_error: FileIdentityError) -> Self {
        Self::IdentityFailed
    }
}

fn validate_nonce(token: &PathTokenId, process_nonce: [u8; 16]) -> Result<(), PathTokenError> {
    if token.process_nonce != process_nonce {
        Err(PathTokenError::ProcessMismatch)
    } else {
        Ok(())
    }
}

fn verify_target_state(
    approved_leaf: &ApprovedLeaf,
    expected: &ExportTargetState,
) -> Result<(), PathTokenError> {
    match expected {
        ExportTargetState::Absent => {
            if approved_leaf.path().exists() {
                Err(PathTokenError::SelectedFileChanged)
            } else {
                Ok(())
            }
        }
        ExportTargetState::Existing(identity) => {
            if !approved_leaf.path().exists() {
                return Err(PathTokenError::SelectedFileChanged);
            }
            let current = identity_for_path(&approved_leaf.path())?;
            if &current == identity {
                Ok(())
            } else {
                Err(PathTokenError::SelectedFileChanged)
            }
        }
    }
}

fn gc_bucket<T: TokenEntry>(bucket: &mut TokenBucket<T>, now: Instant, ttl: Duration) {
    let expired = bucket
        .entries
        .iter()
        .filter_map(|(id, entry)| {
            let _ttl_documents_v1_contract = ttl;
            (entry.consumed() || now >= entry.expires_at()).then_some(id.clone())
        })
        .collect::<Vec<_>>();
    for id in expired {
        bucket.entries.remove(&id);
    }
    bucket.order.retain(|id| bucket.entries.contains_key(id));
}

fn evict_to_capacity<T>(bucket: &mut TokenBucket<T>, max_len: usize) {
    while bucket.entries.len() > max_len {
        if let Some(id) = bucket.order.pop_front() {
            bucket.entries.remove(&id);
        } else {
            break;
        }
    }
}

trait TokenEntry {
    fn expires_at(&self) -> Instant;
    fn consumed(&self) -> bool;
}

impl TokenEntry for ImportPathEntry {
    fn expires_at(&self) -> Instant {
        self.expires_at
    }

    fn consumed(&self) -> bool {
        self.consumed
    }
}

impl TokenEntry for ExportPathEntry {
    fn expires_at(&self) -> Instant {
        self.expires_at
    }

    fn consumed(&self) -> bool {
        self.consumed
    }
}

#[cfg(windows)]
fn open_import_guard(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
}

#[cfg(not(windows))]
fn open_import_guard(path: &Path) -> io::Result<File> {
    OpenOptions::new().read(true).open(path)
}

#[cfg(windows)]
fn open_parent_guard(parent: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ};

    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(parent)
}

#[cfg(not(windows))]
fn open_parent_guard(parent: &Path) -> io::Result<File> {
    File::open(parent)
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    #[test]
    fn import_token_is_one_time_and_returns_original_read_handle() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("portable.rpd-move");
        std::fs::write(&path, b"portable-package").expect("write");
        let registry = PathTokenRegistry::new();
        let now = Instant::now();
        let token = registry.approve_import_path(&path, now).expect("approve");

        let mut lease = registry.consume_import(&token.id, now).expect("consume");
        let mut bytes = Vec::new();
        lease
            .file
            .read_to_end(&mut bytes)
            .expect("read held handle");

        assert_eq!(bytes, b"portable-package");
        assert_eq!(lease.identity, token.identity);
        assert_eq!(
            registry.consume_import(&token.id, now).unwrap_err(),
            PathTokenError::Consumed
        );
    }

    #[test]
    fn token_ttl_capacity_type_and_process_nonce_are_fail_closed() {
        let root = tempfile::tempdir().expect("tempdir");
        let registry = PathTokenRegistry::with_config(PathTokenRegistryConfig {
            ttl: Duration::from_secs(600),
            max_per_kind: 2,
        });
        let now = Instant::now();
        let first_path = root.path().join("first.rpd-move");
        let second_path = root.path().join("second.rpd-move");
        let third_path = root.path().join("third.rpd-move");
        for path in [&first_path, &second_path, &third_path] {
            std::fs::write(path, b"x").expect("write");
        }
        let expired = registry
            .approve_import_path(&first_path, now)
            .expect("approve expired");
        assert_eq!(
            registry
                .consume_import(&expired.id, now + Duration::from_secs(600))
                .unwrap_err(),
            PathTokenError::Expired
        );

        let export = registry
            .approve_export_path(root.path(), "out.rpd-move", false, now)
            .expect("approve export");
        assert_eq!(
            registry.consume_import(&export.id, now).unwrap_err(),
            PathTokenError::TypeMismatch
        );

        let first = registry
            .approve_import_path(&first_path, now)
            .expect("first");
        let _second = registry
            .approve_import_path(&second_path, now)
            .expect("second");
        let _third = registry
            .approve_import_path(&third_path, now)
            .expect("third");
        assert_eq!(
            registry.consume_import(&first.id, now).unwrap_err(),
            PathTokenError::NotFound
        );

        let other_registry = PathTokenRegistry::new();
        let other_nonce = other_registry.process_nonce_for_test();
        let wrong_process = export.id.clone().with_process_nonce_for_test(other_nonce);
        assert_eq!(
            registry.consume_export(&wrong_process, now).unwrap_err(),
            PathTokenError::ProcessMismatch
        );
    }

    #[test]
    fn export_token_detects_selected_target_changes_before_publish() {
        let root = tempfile::tempdir().expect("tempdir");
        let registry = PathTokenRegistry::new();
        let now = Instant::now();

        let absent = registry
            .approve_export_path(root.path(), "new.rpd-move", false, now)
            .expect("approve absent");
        std::fs::write(root.path().join("new.rpd-move"), b"late").expect("late");
        assert_eq!(
            registry.consume_export(&absent.id, now).unwrap_err(),
            PathTokenError::SelectedFileChanged
        );

        let existing_path = root.path().join("existing.rpd-move");
        std::fs::write(&existing_path, b"before").expect("before");
        let existing = registry
            .approve_export_path(root.path(), "existing.rpd-move", true, now)
            .expect("approve existing");
        std::fs::write(&existing_path, b"after").expect("after");
        assert_eq!(
            registry.consume_export(&existing.id, now).unwrap_err(),
            PathTokenError::SelectedFileChanged
        );

        let stable_path = root.path().join("stable.rpd-move");
        std::fs::write(&stable_path, b"stable").expect("stable");
        let stable = registry
            .approve_export_path(root.path(), "stable.rpd-move", true, now)
            .expect("approve stable");
        let lease = registry.consume_export(&stable.id, now).expect("consume");
        assert_eq!(lease.approved_leaf().path(), stable_path);
        assert_eq!(lease.mode, PublishMode::ReplaceExisting);
    }
}
