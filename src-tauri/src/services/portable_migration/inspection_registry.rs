#![allow(
    dead_code,
    reason = "Task 12 publishes import inspection registry infrastructure before Task 13 wires the importer flow"
)]

use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rand::{rngs::OsRng, RngCore};

use crate::services::{
    data_store::file_identity::FileIdentity,
    portable_migration::{
        format::{PortableMigrationManifest, TransportKeyMaterial},
        schema_reader::PortableReaderKind,
    },
};

const INSPECTION_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_INSPECTIONS: usize = 64;

#[derive(Clone, Debug)]
pub(crate) struct ImportInspectionRegistry {
    inner: Arc<Mutex<ImportInspectionRegistryInner>>,
    config: ImportInspectionRegistryConfig,
}

impl ImportInspectionRegistry {
    pub(crate) fn new() -> Self {
        Self::with_config(ImportInspectionRegistryConfig::default())
    }

    pub(crate) fn with_config(config: ImportInspectionRegistryConfig) -> Self {
        assert!(
            !config.ttl.is_zero(),
            "inspection registry TTL must be positive"
        );
        assert!(
            config.max_entries > 0,
            "inspection registry capacity must be positive"
        );
        let mut process_nonce = [0_u8; 16];
        OsRng.fill_bytes(&mut process_nonce);
        Self {
            inner: Arc::new(Mutex::new(ImportInspectionRegistryInner {
                process_nonce,
                entries: HashMap::new(),
                order: VecDeque::new(),
            })),
            config,
        }
    }

    pub(crate) fn register(
        &self,
        lease: ImportPreparationLease,
        summary: ImportInspectionSummary,
        now: Instant,
    ) -> ImportInspectionHandle {
        let mut inner = self.inner.lock().expect("inspection registry mutex");
        gc_locked(&mut inner, now, self.config.ttl);
        while inner.entries.len() >= self.config.max_entries {
            if let Some(id) = inner.order.pop_front() {
                inner.entries.remove(&id);
            } else {
                break;
            }
        }
        let id = ImportInspectionId::new(inner.process_nonce);
        inner.entries.insert(
            id.clone(),
            ImportInspectionEntry {
                lease: Some(lease),
                summary: summary.clone(),
                expires_at: now + self.config.ttl,
                consumed: false,
            },
        );
        inner.order.push_back(id.clone());
        ImportInspectionHandle { id, summary }
    }

    pub(crate) fn consume(
        &self,
        id: &ImportInspectionId,
        now: Instant,
    ) -> Result<ImportPreparationLease, ImportInspectionError> {
        let mut inner = self.inner.lock().expect("inspection registry mutex");
        validate_nonce(id, inner.process_nonce)?;
        let entry = inner
            .entries
            .get_mut(id)
            .ok_or(ImportInspectionError::NotFound)?;
        if entry.consumed {
            return Err(ImportInspectionError::Consumed);
        }
        if now >= entry.expires_at {
            entry.consumed = true;
            entry.lease.take();
            return Err(ImportInspectionError::Expired);
        }
        entry.consumed = true;
        entry.lease.take().ok_or(ImportInspectionError::Consumed)
    }

    pub(crate) fn summary(
        &self,
        id: &ImportInspectionId,
        now: Instant,
    ) -> Result<ImportInspectionSummary, ImportInspectionError> {
        let inner = self.inner.lock().expect("inspection registry mutex");
        validate_nonce(id, inner.process_nonce)?;
        let entry = inner
            .entries
            .get(id)
            .ok_or(ImportInspectionError::NotFound)?;
        if entry.consumed {
            return Err(ImportInspectionError::Consumed);
        }
        if now >= entry.expires_at {
            return Err(ImportInspectionError::Expired);
        }
        Ok(entry.summary.clone())
    }

    pub(crate) fn gc(&self, now: Instant) {
        let mut inner = self.inner.lock().expect("inspection registry mutex");
        gc_locked(&mut inner, now, self.config.ttl);
    }

    #[cfg(test)]
    fn process_nonce_for_test(&self) -> [u8; 16] {
        self.inner
            .lock()
            .expect("inspection registry mutex")
            .process_nonce
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ImportInspectionRegistryConfig {
    pub(crate) ttl: Duration,
    pub(crate) max_entries: usize,
}

impl Default for ImportInspectionRegistryConfig {
    fn default() -> Self {
        Self {
            ttl: INSPECTION_TTL,
            max_entries: MAX_INSPECTIONS,
        }
    }
}

#[derive(Debug)]
struct ImportInspectionRegistryInner {
    process_nonce: [u8; 16],
    entries: HashMap<ImportInspectionId, ImportInspectionEntry>,
    order: VecDeque<ImportInspectionId>,
}

#[derive(Debug)]
struct ImportInspectionEntry {
    lease: Option<ImportPreparationLease>,
    summary: ImportInspectionSummary,
    expires_at: Instant,
    consumed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportInspectionHandle {
    pub(crate) id: ImportInspectionId,
    pub(crate) summary: ImportInspectionSummary,
}

#[derive(Debug)]
pub(crate) struct ImportPreparationLease {
    pub(crate) source_identity: FileIdentity,
    pub(crate) staging_path: PathBuf,
    pub(crate) staging_identity: FileIdentity,
    pub(crate) reader_kind: PortableReaderKind,
    pub(crate) manifest: PortableMigrationManifest,
    pub(crate) sqlite_sha256: [u8; 32],
    pub(crate) transport_key: TransportKeyMaterial,
}

impl Drop for ImportPreparationLease {
    fn drop(&mut self) {
        if !is_import_staging_sqlite(&self.staging_path) {
            return;
        }
        let _ = fs::remove_file(&self.staging_path);
        if let Some(parent) = self.staging_path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportInspectionSummary {
    pub(crate) export_id: String,
    pub(crate) created_at: String,
    pub(crate) source_app_version: String,
    pub(crate) source_platform: String,
    pub(crate) included_categories: Vec<String>,
    pub(crate) include_history: bool,
    pub(crate) record_counts: Vec<(String, u64)>,
    pub(crate) sqlite_size_bytes: u64,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct ImportInspectionId {
    process_nonce: [u8; 16],
    value: String,
}

impl ImportInspectionId {
    fn new(process_nonce: [u8; 16]) -> Self {
        Self {
            process_nonce,
            value: uuid::Uuid::now_v7().to_string(),
        }
    }

    #[cfg(test)]
    fn with_process_nonce_for_test(mut self, process_nonce: [u8; 16]) -> Self {
        self.process_nonce = process_nonce;
        self
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum ImportInspectionError {
    #[error("inspection handle was not created by this process")]
    ProcessMismatch,
    #[error("inspection result not found")]
    NotFound,
    #[error("inspection result expired")]
    Expired,
    #[error("inspection result was already consumed")]
    Consumed,
}

fn validate_nonce(
    id: &ImportInspectionId,
    process_nonce: [u8; 16],
) -> Result<(), ImportInspectionError> {
    if id.process_nonce != process_nonce {
        Err(ImportInspectionError::ProcessMismatch)
    } else {
        Ok(())
    }
}

fn gc_locked(inner: &mut ImportInspectionRegistryInner, now: Instant, ttl: Duration) {
    let expired = inner
        .entries
        .iter()
        .filter_map(|(id, entry)| {
            let _ttl_documents_v1_contract = ttl;
            (entry.consumed || now >= entry.expires_at).then_some(id.clone())
        })
        .collect::<Vec<_>>();
    for id in expired {
        inner.entries.remove(&id);
    }
    inner.order.retain(|id| inner.entries.contains_key(id));
}

fn is_import_staging_sqlite(path: &std::path::Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if file_name != "portable.sqlite3" {
        return false;
    }
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("portable-import-inspection-"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use base64::{engine::general_purpose, Engine as _};
    use sha2::{Digest, Sha256};
    use static_assertions::assert_not_impl_any;

    use super::*;

    assert_not_impl_any!(ImportPreparationLease: Clone, Copy, serde::Serialize);

    #[test]
    fn inspection_result_is_consumed_once_by_moving_non_clone_lease() {
        let registry = ImportInspectionRegistry::new();
        let now = Instant::now();
        let handle = registry.register(lease([7; 32]), summary("one"), now);

        let lease = registry.consume(&handle.id, now).expect("consume");
        lease
            .transport_key
            .with_bytes(|bytes| assert_eq!(bytes, &[7; 32]));
        assert_eq!(
            registry.consume(&handle.id, now).unwrap_err(),
            ImportInspectionError::Consumed
        );
    }

    #[test]
    fn inspection_registry_ttl_capacity_process_nonce_and_gc_are_fail_closed() {
        let registry = ImportInspectionRegistry::with_config(ImportInspectionRegistryConfig {
            ttl: Duration::from_secs(600),
            max_entries: 2,
        });
        let now = Instant::now();
        let expired = registry.register(lease([1; 32]), summary("expired"), now);
        assert_eq!(
            registry
                .consume(&expired.id, now + Duration::from_secs(600))
                .unwrap_err(),
            ImportInspectionError::Expired
        );
        registry.gc(now + Duration::from_secs(601));
        assert_eq!(
            registry.consume(&expired.id, now).unwrap_err(),
            ImportInspectionError::NotFound
        );

        let first = registry.register(lease([2; 32]), summary("first"), now);
        let _second = registry.register(lease([3; 32]), summary("second"), now);
        let _third = registry.register(lease([4; 32]), summary("third"), now);
        assert_eq!(
            registry.consume(&first.id, now).unwrap_err(),
            ImportInspectionError::NotFound
        );

        let other = ImportInspectionRegistry::new();
        let wrong_process = first
            .id
            .clone()
            .with_process_nonce_for_test(other.process_nonce_for_test());
        assert_eq!(
            registry.consume(&wrong_process, now).unwrap_err(),
            ImportInspectionError::ProcessMismatch
        );
    }

    #[test]
    fn summary_is_allowlist_and_does_not_expose_transport_key() {
        let registry = ImportInspectionRegistry::new();
        let now = Instant::now();
        let handle = registry.register(lease([9; 32]), summary("safe"), now);

        let debug = format!("{:?}", registry.summary(&handle.id, now));

        assert!(!debug.contains("TransportKeyMaterial"));
        assert!(!debug.contains("transport_key"));
    }

    #[test]
    fn expired_preparation_lease_removes_only_owned_staging_file() {
        let directory = tempfile::tempdir().expect("tempdir");
        let operation_dir = directory.path().join(format!(
            "portable-import-inspection-{}",
            uuid::Uuid::now_v7()
        ));
        std::fs::create_dir(&operation_dir).expect("operation dir");
        let staging_path = operation_dir.join("portable.sqlite3");
        std::fs::write(&staging_path, b"staged").expect("staged file");

        let registry = ImportInspectionRegistry::with_config(ImportInspectionRegistryConfig {
            ttl: Duration::from_secs(1),
            max_entries: 2,
        });
        let now = Instant::now();
        let handle = registry.register(
            lease_with_path([4; 32], staging_path.clone()),
            summary("cleanup"),
            now,
        );

        assert_eq!(
            registry
                .consume(&handle.id, now + Duration::from_secs(1))
                .unwrap_err(),
            ImportInspectionError::Expired
        );
        assert!(!staging_path.exists());
        assert!(!operation_dir.exists());

        let unrelated_path = directory.path().join("active.sqlite3");
        std::fs::write(&unrelated_path, b"active").expect("unrelated file");
        drop(lease_with_path([5; 32], unrelated_path.clone()));
        assert!(unrelated_path.exists());
    }

    fn lease(key: [u8; 32]) -> ImportPreparationLease {
        lease_with_path(key, std::path::PathBuf::from("staging.sqlite3"))
    }

    fn lease_with_path(key: [u8; 32], staging_path: PathBuf) -> ImportPreparationLease {
        let manifest = manifest_struct("018f7f9a-1111-7000-8000-000000000001");
        ImportPreparationLease {
            source_identity: identity("source"),
            staging_path,
            staging_identity: identity("staging"),
            reader_kind: PortableReaderKind::V1EncryptedSecrets,
            manifest,
            sqlite_sha256: [5; 32],
            transport_key: TransportKeyMaterial::from_bytes(key),
        }
    }

    fn summary(export_id_suffix: &str) -> ImportInspectionSummary {
        ImportInspectionSummary {
            export_id: export_id_suffix.to_string(),
            created_at: "2026-07-29T00:00:00Z".to_string(),
            source_app_version: "0.3.3".to_string(),
            source_platform: "windows".to_string(),
            included_categories: vec!["core_data".to_string()],
            include_history: false,
            record_counts: vec![("stations".to_string(), 1)],
            sqlite_size_bytes: 64,
        }
    }

    fn identity(seed: &str) -> FileIdentity {
        FileIdentity {
            volume_serial: None,
            file_id: None,
            length: seed.len() as u64,
            sha256: format!("{:x}", Sha256::digest(seed.as_bytes())),
        }
    }

    fn manifest_struct(export_id: &str) -> PortableMigrationManifest {
        let mut record_counts = BTreeMap::new();
        record_counts.insert("station_keys".to_string(), 0);
        record_counts.insert("stations".to_string(), 0);
        PortableMigrationManifest {
            format: "relay-pool-portable-migration".to_string(),
            format_version: 1,
            export_id: export_id.to_string(),
            created_at: "2026-07-29T00:00:00Z".to_string(),
            source_app_version: "0.3.3".to_string(),
            source_platform: "windows".to_string(),
            database_generation: 2,
            database_schema_version: 10,
            portable_schema_profile: "encrypted-secrets-v1".to_string(),
            minimum_importer_version: "0.3.3".to_string(),
            transport_key_id: "transport:018f7f9a-1111-7000-8000-000000000002".to_string(),
            encryption_version: 1,
            export_policy_version: 1,
            required_features: vec![],
            extensions: serde_json::json!({}),
            included_categories: vec!["core_data".to_string()],
            excluded_categories: vec![
                "history".to_string(),
                "session_credentials".to_string(),
                "local_proxy_access_key".to_string(),
                "device_runtime_state".to_string(),
                "provider_drafts".to_string(),
            ],
            record_counts,
            sqlite_size_bytes: 0,
            sqlite_sha256: general_purpose::STANDARD.encode(Sha256::digest([])),
        }
    }
}
