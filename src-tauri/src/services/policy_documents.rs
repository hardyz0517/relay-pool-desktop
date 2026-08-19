//! Shared file-side control plane for versioned policy documents.
//!
//! SQLite remains the active source of truth.  This module owns only the
//! managed file boundary: strict JSON decoding helpers, stable reads, canonical
//! materialization, and a single coordinator guard used by both document
//! kinds.  It intentionally does not contain policy or mapping validation.

use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex, OnceLock},
    time::{Duration, SystemTime},
};

use serde::{
    de::{self, DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor},
    Deserializer, Serialize,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::models::document_sync::DocumentKind;
use crate::services::data_store::atomic_file::{
    ApprovedLeaf, AtomicFileError, AtomicJournalPort, LocalAtomicFileAdapter,
};

pub(crate) const MAX_DOCUMENT_BYTES: usize = 64 * 1024;
pub(crate) const STABLE_READ_DELAY: Duration = Duration::from_millis(150);
const CONFIG_DIRECTORY: &str = "config";

#[derive(Debug, Error)]
pub(crate) enum PolicyDocumentError {
    #[error("document is missing")]
    Missing,
    #[error("document is too large")]
    TooLarge,
    #[error("document read is unstable")]
    Unstable,
    #[error("document path is invalid")]
    InvalidPath,
    #[error("document JSON is invalid: {0}")]
    InvalidJson(String),
    #[error("document contains a duplicate object key")]
    DuplicateKey,
    #[error("document materialization failed: {0}")]
    Io(#[from] io::Error),
    #[error("document serialization failed: {0}")]
    Serialization(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StableDocument {
    pub(crate) bytes: Vec<u8>,
    pub(crate) digest: String,
    pub(crate) identity: FileIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileIdentity {
    pub(crate) length: u64,
    pub(crate) modified_unix_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReconciliationState {
    Missing,
    Stable,
    Changed,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileObservation {
    pub(crate) kind: DocumentKind,
    pub(crate) state: ReconciliationState,
    pub(crate) digest: Option<String>,
    pub(crate) identity: Option<FileIdentity>,
}

#[derive(Debug, Clone)]
pub(crate) struct DocumentFileStore {
    root: PathBuf,
    stable_read_delay: Duration,
}

impl DocumentFileStore {
    pub(crate) fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            stable_read_delay: STABLE_READ_DELAY,
        }
    }

    #[cfg(test)]
    fn with_stable_read_delay(mut self, delay: Duration) -> Self {
        self.stable_read_delay = delay;
        self
    }

    pub(crate) fn directory(&self) -> PathBuf {
        self.root.join(CONFIG_DIRECTORY)
    }

    #[cfg(test)]
    pub(crate) fn path(&self, kind: DocumentKind) -> Result<PathBuf, PolicyDocumentError> {
        let directory = self.directory();
        let path = directory.join(kind.file_name());
        if path.parent() != Some(directory.as_path()) || path.file_name().is_none() {
            return Err(PolicyDocumentError::InvalidPath);
        }
        Ok(path)
    }

    fn approved_leaf(&self, kind: DocumentKind) -> Result<ApprovedLeaf, PolicyDocumentError> {
        let directory = self.directory();
        let metadata = match fs::symlink_metadata(&directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(PolicyDocumentError::Missing)
            }
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_dir() {
            return Err(PolicyDocumentError::InvalidPath);
        }
        ApprovedLeaf::approve(directory, kind.file_name()).map_err(map_atomic_error)
    }

    pub(crate) fn read_once(
        &self,
        kind: DocumentKind,
    ) -> Result<StableDocument, PolicyDocumentError> {
        let leaf = self.approved_leaf(kind)?;
        let metadata = leaf
            .target_metadata()
            .map_err(map_atomic_error)?
            .ok_or(PolicyDocumentError::Missing)?;
        let length = usize::try_from(metadata.len()).map_err(|_| PolicyDocumentError::TooLarge)?;
        if length > MAX_DOCUMENT_BYTES {
            return Err(PolicyDocumentError::TooLarge);
        }
        let mut file = File::open(leaf.path())?;
        let mut bytes = Vec::with_capacity(length);
        file.read_to_end(&mut bytes)?;
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(PolicyDocumentError::TooLarge);
        }
        Ok(StableDocument {
            digest: digest_hex(&bytes),
            identity: FileIdentity {
                length: bytes.len() as u64,
                modified_unix_ms: metadata.modified().ok().and_then(unix_ms),
            },
            bytes,
        })
    }

    pub(crate) async fn read_stable(
        &self,
        kind: DocumentKind,
    ) -> Result<StableDocument, PolicyDocumentError> {
        let first = self.read_once(kind)?;
        if !self.stable_read_delay.is_zero() {
            tokio::time::sleep(self.stable_read_delay).await;
        }
        let second = self.read_once(kind)?;
        if first.identity != second.identity || first.digest != second.digest {
            return Err(PolicyDocumentError::Unstable);
        }
        Ok(second)
    }

    pub(crate) fn materialize(
        &self,
        kind: DocumentKind,
        canonical_bytes: &[u8],
    ) -> Result<String, PolicyDocumentError> {
        if canonical_bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(PolicyDocumentError::TooLarge);
        }
        let directory = self.directory();
        ensure_directory(&directory)?;
        let target = self.approved_leaf(kind)?;
        let readback = LocalAtomicFileAdapter
            .publish_and_readback(canonical_bytes, &target)
            .map_err(map_atomic_error)?;
        if readback != canonical_bytes {
            return Err(PolicyDocumentError::Unstable);
        }
        Ok(digest_hex(&readback))
    }
}

/// Serializes all read/materialize/reconcile operations for a process.  The
/// database CAS remains the cross-process fence; this guard prevents two
/// watcher/UI operations from interleaving file reads and replaces locally.
#[derive(Debug, Clone)]
pub(crate) struct PolicyDocumentCoordinator {
    files: Arc<DocumentFileStore>,
    guard: Arc<Mutex<()>>,
}

static SHARED_COORDINATORS: OnceLock<StdMutex<HashMap<PathBuf, PolicyDocumentCoordinator>>> =
    OnceLock::new();

impl PolicyDocumentCoordinator {
    pub(crate) fn new(files: DocumentFileStore) -> Self {
        Self {
            files: Arc::new(files),
            guard: Arc::new(Mutex::new(())),
        }
    }

    /// Return the process-wide coordinator for a data root. Both routing
    /// documents use this factory so a file watcher and a UI mutation cannot
    /// interleave their local read/compare/materialize sequence. SQLite CAS
    /// remains the cross-process correctness fence.
    pub(crate) fn shared(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let registry = SHARED_COORDINATORS.get_or_init(|| StdMutex::new(HashMap::new()));
        let mut coordinators = registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        coordinators
            .entry(root.clone())
            .or_insert_with(|| Self::new(DocumentFileStore::new(root)))
            .clone()
    }

    pub(crate) fn files(&self) -> &DocumentFileStore {
        &self.files
    }

    /// Acquire the process-wide document operation guard for an application
    /// operation that must combine a database revision check with file
    /// materialization. Callers must use `files()` directly while holding the
    /// returned guard.
    pub(crate) async fn acquire_operation_guard(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.guard.clone().lock_owned().await
    }

    pub(crate) async fn read_stable(
        &self,
        kind: DocumentKind,
    ) -> Result<StableDocument, PolicyDocumentError> {
        let _guard = self.guard.lock().await;
        self.files.read_stable(kind).await
    }

    pub(crate) async fn reconcile(
        &self,
        kind: DocumentKind,
        expected_digest: Option<&str>,
    ) -> FileObservation {
        let _guard = self.guard.lock().await;
        match self.files.read_once(kind) {
            Ok(observed) => FileObservation {
                kind,
                state: if expected_digest.is_some_and(|digest| digest == observed.digest) {
                    ReconciliationState::Stable
                } else {
                    ReconciliationState::Changed
                },
                digest: Some(observed.digest),
                identity: Some(observed.identity),
            },
            Err(PolicyDocumentError::Missing) => FileObservation {
                kind,
                state: ReconciliationState::Missing,
                digest: None,
                identity: None,
            },
            Err(_) => FileObservation {
                kind,
                state: ReconciliationState::Unavailable,
                digest: None,
                identity: None,
            },
        }
    }
}

pub(crate) fn decode_strict_json<T: DeserializeOwned>(
    bytes: &[u8],
) -> Result<T, PolicyDocumentError> {
    let payload = strip_utf8_bom(bytes);
    if payload.len() > MAX_DOCUMENT_BYTES {
        return Err(PolicyDocumentError::TooLarge);
    }
    let mut deserializer = serde_json::Deserializer::from_slice(payload);
    let value = deserializer
        .deserialize_any(StrictValueVisitor)
        .map_err(|error| {
            if error.to_string().contains("duplicate object key") {
                PolicyDocumentError::DuplicateKey
            } else {
                PolicyDocumentError::InvalidJson(error.to_string())
            }
        })?;
    deserializer
        .end()
        .map_err(|error| PolicyDocumentError::InvalidJson(error.to_string()))?;
    serde_json::from_value(value)
        .map_err(|error| PolicyDocumentError::InvalidJson(error.to_string()))
}

pub(crate) fn canonical_json<T: Serialize>(document: &T) -> Result<Vec<u8>, PolicyDocumentError> {
    let bytes = serde_json::to_vec(document)
        .map_err(|error| PolicyDocumentError::Serialization(error.to_string()))?;
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(PolicyDocumentError::TooLarge);
    }
    Ok(bytes)
}

fn strip_utf8_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)
}

fn digest_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn unix_ms(value: SystemTime) -> Option<u128> {
    value
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

fn ensure_directory(directory: &Path) -> Result<(), PolicyDocumentError> {
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(PolicyDocumentError::InvalidPath),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(directory)?;
            let metadata = fs::symlink_metadata(directory)?;
            if metadata.is_dir() {
                Ok(())
            } else {
                Err(PolicyDocumentError::InvalidPath)
            }
        }
        Err(error) => Err(error.into()),
    }
}

fn map_atomic_error(error: AtomicFileError) -> PolicyDocumentError {
    match error {
        AtomicFileError::PathRejected => PolicyDocumentError::InvalidPath,
        AtomicFileError::Missing => PolicyDocumentError::Missing,
        AtomicFileError::AlreadyExists => PolicyDocumentError::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "document target already exists",
        )),
        AtomicFileError::Io(error) => PolicyDocumentError::Io(error),
        AtomicFileError::Identity(error) => PolicyDocumentError::Io(io::Error::other(error)),
    }
}

struct StrictValueSeed;

impl<'de> DeserializeSeed<'de> for StrictValueSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictValueVisitor)
    }
}

struct StrictValueVisitor;

impl<'de> Visitor<'de> for StrictValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| de::Error::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(StrictValueSeed)? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate object key"));
            }
            values.insert(key, map.next_value_seed(StrictValueSeed)?);
        }
        let object = values.into_iter().collect::<Map<String, Value>>();
        Ok(Value::Object(object))
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Duration};

    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    #[serde(deny_unknown_fields)]
    struct FixtureDocument {
        format_version: u16,
        base_revision: u64,
    }

    #[test]
    fn strict_json_rejects_duplicate_and_unknown_fields_and_accepts_bom() {
        let error = decode_strict_json::<FixtureDocument>(
            br#"{"format_version":1,"format_version":1,"base_revision":1}"#,
        )
        .expect_err("duplicate key must fail closed");
        assert!(matches!(error, PolicyDocumentError::DuplicateKey));
        let error = decode_strict_json::<FixtureDocument>(
            br#"{"format_version":1,"base_revision":1,"extra":true}"#,
        )
        .expect_err("unknown field must fail closed");
        assert!(matches!(error, PolicyDocumentError::InvalidJson(_)));
        let document = decode_strict_json::<FixtureDocument>(&[
            0xEF, 0xBB, 0xBF, b'{', b'"', b'f', b'o', b'r', b'm', b'a', b't', b'_', b'v', b'e',
            b'r', b's', b'i', b'o', b'n', b'"', b':', b'1', b',', b'"', b'b', b'a', b's', b'e',
            b'_', b'r', b'e', b'v', b'i', b's', b'i', b'o', b'n', b'"', b':', b'1', b'}',
        ])
        .expect("BOM is allowed");
        assert_eq!(
            document,
            FixtureDocument {
                format_version: 1,
                base_revision: 1
            }
        );
    }

    #[tokio::test]
    async fn coordinator_materializes_both_document_kinds_and_reconciles_digest() {
        let root = tempfile::tempdir().expect("tempdir");
        let files = DocumentFileStore::new(root.path()).with_stable_read_delay(Duration::ZERO);
        let coordinator = PolicyDocumentCoordinator::new(files);
        let routing = br#"{"formatVersion":1}"#;
        let mapping = br#"{"formatVersion":1,"rules":[]}"#;
        let routing_digest = coordinator
            .files()
            .materialize(DocumentKind::RoutingPolicy, routing)
            .expect("routing materialize");
        let mapping_digest = coordinator
            .files()
            .materialize(DocumentKind::ModelMapping, mapping)
            .expect("mapping materialize");
        assert_ne!(routing_digest, mapping_digest);
        let observed = coordinator
            .reconcile(DocumentKind::ModelMapping, Some(&mapping_digest))
            .await;
        assert_eq!(observed.state, ReconciliationState::Stable);
        assert_eq!(observed.kind, DocumentKind::ModelMapping);
        let stable = coordinator
            .read_stable(DocumentKind::RoutingPolicy)
            .await
            .expect("stable routing document");
        assert_eq!(stable.bytes, routing);
        assert_eq!(stable.digest, routing_digest);
        assert_eq!(
            coordinator
                .files()
                .path(DocumentKind::ModelMapping)
                .unwrap()
                .file_name()
                .unwrap(),
            "model-mapping.json"
        );
    }

    #[tokio::test]
    async fn reconciliation_distinguishes_changed_and_unavailable_without_returning_file_bytes() {
        let root = tempfile::tempdir().expect("tempdir");
        let coordinator = PolicyDocumentCoordinator::new(
            DocumentFileStore::new(root.path()).with_stable_read_delay(Duration::ZERO),
        );
        let initial = br#"{"formatVersion":1,"rules":[]}"#;
        let expected_digest = coordinator
            .files()
            .materialize(DocumentKind::ModelMapping, initial)
            .expect("materialize mapping");
        let path = coordinator
            .files()
            .path(DocumentKind::ModelMapping)
            .expect("mapping path");

        fs::write(&path, br#"{"formatVersion":1,"rules":["external"]}"#).expect("external edit");
        let changed = coordinator
            .reconcile(DocumentKind::ModelMapping, Some(&expected_digest))
            .await;
        assert_eq!(changed.state, ReconciliationState::Changed);
        assert!(changed.digest.is_some());
        assert_ne!(changed.digest.as_deref(), Some(expected_digest.as_str()));
        assert!(changed.identity.is_some());

        // A directory at the managed leaf is an unavailable file boundary,
        // not an external document that the application may parse or apply.
        fs::remove_file(&path).expect("remove edited file");
        fs::create_dir(&path).expect("replace file with directory");
        let unavailable = coordinator
            .reconcile(DocumentKind::ModelMapping, Some(&expected_digest))
            .await;
        assert_eq!(unavailable.state, ReconciliationState::Unavailable);
        assert!(unavailable.digest.is_none());
        assert!(unavailable.identity.is_none());
    }

    #[tokio::test]
    async fn stable_read_fails_closed_when_external_writer_changes_bytes_mid_read() {
        let root = tempfile::tempdir().expect("tempdir");
        let files =
            DocumentFileStore::new(root.path()).with_stable_read_delay(Duration::from_millis(40));
        files
            .materialize(DocumentKind::ModelMapping, br#"{"formatVersion":1}"#)
            .expect("initial materialize");
        let path = files
            .path(DocumentKind::ModelMapping)
            .expect("mapping path");
        let reader = files.clone();
        let task =
            tokio::spawn(async move { reader.read_stable(DocumentKind::ModelMapping).await });
        tokio::time::sleep(Duration::from_millis(5)).await;
        fs::write(&path, br#"{"formatVersion":1,"changed":true}"#).expect("external write");

        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("stable reader completes")
            .expect("stable reader task");
        assert!(matches!(result, Err(PolicyDocumentError::Unstable)));
    }

    #[test]
    fn materialization_rejects_unavailable_config_root_and_oversized_payload() {
        let root = tempfile::tempdir().expect("tempdir");
        fs::write(root.path().join(CONFIG_DIRECTORY), b"not a directory").expect("config blocker");
        let store = DocumentFileStore::new(root.path());
        let path_error = store
            .materialize(DocumentKind::ModelMapping, br#"{"formatVersion":1}"#)
            .expect_err("config file must block materialization");
        assert!(matches!(path_error, PolicyDocumentError::InvalidPath));

        let oversized = vec![b' '; MAX_DOCUMENT_BYTES + 1];
        let json_error = decode_strict_json::<FixtureDocument>(&oversized)
            .expect_err("oversized document must fail closed");
        assert!(matches!(json_error, PolicyDocumentError::TooLarge));
    }

    #[test]
    fn managed_target_directory_fails_closed() {
        let root = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(root.path().join("config").join("model-mapping.json"))
            .expect("target directory");
        let store = DocumentFileStore::new(root.path());
        let error = store
            .read_once(DocumentKind::ModelMapping)
            .expect_err("document target directory must be rejected");
        assert!(matches!(error, PolicyDocumentError::InvalidPath));
    }

    #[cfg(unix)]
    #[test]
    fn managed_symlink_target_and_config_directory_fail_closed() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("tempdir");
        let config = root.path().join("config");
        fs::create_dir(&config).expect("config directory");
        let outside = root.path().join("outside.json");
        fs::write(&outside, br#"{"formatVersion":1}"#).expect("outside file");
        symlink(&outside, config.join("model-mapping.json")).expect("target symlink");
        let store = DocumentFileStore::new(root.path());
        let target_error = store
            .read_once(DocumentKind::ModelMapping)
            .expect_err("document target symlink must be rejected");
        assert!(matches!(target_error, PolicyDocumentError::InvalidPath));

        let root_with_link = tempfile::tempdir().expect("linked root");
        let real_config = root_with_link.path().join("real-config");
        fs::create_dir(&real_config).expect("real config");
        symlink(&real_config, root_with_link.path().join("config")).expect("config symlink");
        let parent_error = DocumentFileStore::new(root_with_link.path())
            .read_once(DocumentKind::ModelMapping)
            .expect_err("config directory symlink must be rejected");
        assert!(matches!(parent_error, PolicyDocumentError::InvalidPath));
    }
}
