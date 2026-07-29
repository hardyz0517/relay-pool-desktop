use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crate::services::{
    data_store::atomic_file::{
        create_new_file, ApprovedLeaf, AtomicFileError, AtomicFilePublishPort, PublishEvidence,
        PublishMode,
    },
    secrets::{
        rekey::{
            BufferedSecretRekeyWriter, SecretRekeyPolicy, SecretRekeyService, TransportSecretKey,
        },
        CURRENT_SECRET_ENCRYPTION_VERSION,
    },
};

use super::{
    age_envelope::{
        decrypt_framed_payload_to_writer, encrypt_framed_payload, AgeEnvelopeError,
        AgeEnvelopeOptions,
    },
    format::{ParsedPortablePayloadInfo, PortableMigrationManifest, TransportKeyMaterial},
    schema_reader::{ordered_import_tables_v1, PortableSchemaReader},
    transform::{encrypted_secret_from_portable_row, TransformOptions},
    validate::{validate_closed_sqlite_database, PortableMigrationValidationError},
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum PortablePackageStagingError {
    #[error("portable migration package envelope failed")]
    Envelope(#[from] AgeEnvelopeError),
    #[error("portable migration package validation failed")]
    Validation(#[from] PortableMigrationValidationError),
    #[error("portable migration package secret self-test failed")]
    SecretSelfTest,
    #[error("portable migration package atomic publish failed")]
    Atomic(#[from] AtomicFileError),
    #[error("portable migration package I/O failed")]
    Io,
    #[error("portable migration package cleanup failed")]
    CleanupFailed,
}

pub(crate) type PortablePackageStagingResult<T> = Result<T, PortablePackageStagingError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PortablePackageSelfTestReport {
    pub(crate) manifest: PortableMigrationManifest,
    pub(crate) sqlite_sha256: [u8; 32],
    pub(crate) row_counts: BTreeMap<String, u64>,
}

pub(crate) fn partial_path_for_target(
    target: &ApprovedLeaf,
    export_id: &str,
) -> PortablePackageStagingResult<PathBuf> {
    let target_path = target.path();
    let parent = target_path
        .parent()
        .ok_or(PortablePackageStagingError::Io)?;
    let leaf = target_path
        .file_name()
        .ok_or(PortablePackageStagingError::Io)?;
    let mut partial_leaf = OsString::from(".");
    partial_leaf.push(leaf);
    partial_leaf.push(format!(".{export_id}.partial"));
    Ok(parent.join(partial_leaf))
}

pub(crate) fn write_encrypted_partial<R: Read>(
    target: &ApprovedLeaf,
    export_id: &str,
    passphrase: &str,
    manifest: &PortableMigrationManifest,
    transport_key: &TransportKeyMaterial,
    sqlite_reader: R,
    expected_record_count_keys: &[&str],
    options: AgeEnvelopeOptions,
) -> PortablePackageStagingResult<PathBuf> {
    let partial = partial_path_for_target(target, export_id)?;
    let mut file = create_new_file(&partial)?;
    let result = encrypt_framed_payload(
        &mut file,
        passphrase,
        manifest,
        transport_key,
        sqlite_reader,
        expected_record_count_keys,
        options,
    );
    match result {
        Ok(_) => {
            file.flush().map_err(|_| PortablePackageStagingError::Io)?;
            file.sync_all()
                .map_err(|_| PortablePackageStagingError::Io)?;
            Ok(partial)
        }
        Err(error) => {
            drop(file);
            remove_file_if_exists(&partial)?;
            Err(error.into())
        }
    }
}

pub(crate) async fn self_test_encrypted_package(
    package_path: &Path,
    scratch_directory: &Path,
    passphrase: &str,
    expected_record_count_keys: &[&str],
    options: AgeEnvelopeOptions,
) -> PortablePackageStagingResult<PortablePackageSelfTestReport> {
    fs::create_dir_all(scratch_directory).map_err(|_| PortablePackageStagingError::Io)?;
    let scratch_path = scratch_directory.join(format!(
        "portable-package-self-test-{}.sqlite3",
        uuid::Uuid::now_v7()
    ));

    let result = async {
        let mut scratch = create_new_file(&scratch_path)?;
        let file = File::open(package_path).map_err(|_| PortablePackageStagingError::Io)?;
        let parsed = decrypt_framed_payload_to_writer(
            file,
            passphrase,
            expected_record_count_keys,
            options,
            &mut scratch,
        )?;
        scratch
            .sync_all()
            .map_err(|_| PortablePackageStagingError::Io)?;
        drop(scratch);

        validate_closed_sqlite_database(&scratch_path).await?;
        let include_history = parsed
            .manifest
            .included_categories
            .iter()
            .any(|category| category == "history");
        let row_counts = read_and_verify_rows(
            &scratch_path,
            &parsed.manifest.record_counts,
            include_history,
        )
        .await?;
        verify_secret_rows_decrypt_with_transport_key(&scratch_path, &parsed).await?;

        Ok(PortablePackageSelfTestReport {
            manifest: parsed.manifest,
            sqlite_sha256: parsed.sqlite_sha256,
            row_counts,
        })
    }
    .await;

    let cleanup = remove_file_if_exists(&scratch_path);
    match (result, cleanup) {
        (Ok(report), Ok(())) => Ok(report),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

pub(crate) fn publish_verified_partial(
    publisher: &impl AtomicFilePublishPort,
    partial_path: &Path,
    target: &ApprovedLeaf,
    mode: PublishMode,
) -> PortablePackageStagingResult<PublishEvidence> {
    publisher
        .publish(partial_path, target, mode)
        .map_err(PortablePackageStagingError::from)
}

pub(crate) fn remove_file_if_exists(path: &Path) -> PortablePackageStagingResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(PortablePackageStagingError::CleanupFailed),
    }
}

async fn read_and_verify_rows(
    sqlite_path: &Path,
    expected: &BTreeMap<String, u64>,
    include_history: bool,
) -> PortablePackageStagingResult<BTreeMap<String, u64>> {
    let mut reader = PortableSchemaReader::open_v1(sqlite_path).await?;
    let mut actual = BTreeMap::new();
    let options = TransformOptions { include_history };
    for table_name in ordered_import_tables_v1() {
        let rows = reader.read_transformed_table(table_name, options).await?;
        actual.insert(table_name.to_string(), rows.len() as u64);
    }
    reader.close().await?;
    if &actual != expected {
        return Err(PortablePackageStagingError::Validation(
            PortableMigrationValidationError::UnsupportedSchema,
        ));
    }
    Ok(actual)
}

async fn verify_secret_rows_decrypt_with_transport_key(
    sqlite_path: &Path,
    parsed: &ParsedPortablePayloadInfo,
) -> PortablePackageStagingResult<()> {
    let mut reader = PortableSchemaReader::open_v1(sqlite_path).await?;
    let rows = reader
        .read_transformed_table(
            "secrets",
            TransformOptions {
                include_history: true,
            },
        )
        .await?;
    reader.close().await?;

    let secrets = rows
        .iter()
        .map(encrypted_secret_from_portable_row)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| PortablePackageStagingError::SecretSelfTest)?;
    let source = parsed.transport_key.with_bytes(|bytes| {
        TransportSecretKey::from_parts(parsed.manifest.transport_key_id.clone(), *bytes)
    });
    let target = TransportSecretKey::generate();
    let verifier = SecretRekeyService::new(
        source.resolver(),
        target.resolver(),
        CURRENT_SECRET_ENCRYPTION_VERSION,
    );
    let mut writer = BufferedSecretRekeyWriter::create_new();
    verifier
        .rekey(
            secrets,
            &SecretRekeyPolicy::include_all(),
            &mut writer,
            None,
        )
        .map_err(|_| PortablePackageStagingError::SecretSelfTest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, io::Cursor};

    use base64::{engine::general_purpose, Engine as _};
    use sha2::{Digest, Sha256};

    use crate::services::{
        data_store::atomic_file::{ApprovedLeaf, LocalAtomicFileAdapter, PublishMode},
        portable_migration::{
            age_envelope::AgeEnvelopeOptions,
            format::{PortableMigrationManifest, TransportKeyMaterial},
        },
    };

    use super::{partial_path_for_target, publish_verified_partial, write_encrypted_partial};

    const FIXTURE_KEYS: [&str; 2] = ["station_keys", "stations"];

    #[test]
    fn partial_path_is_hidden_sibling_named_by_export_id() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = ApprovedLeaf::approve(root.path(), "export.rpd-move").expect("approve");

        let partial = partial_path_for_target(&target, "018f7f9a-1111-7000-8000-000000000001")
            .expect("partial");

        assert_eq!(
            partial.file_name().and_then(|value| value.to_str()),
            Some(".export.rpd-move.018f7f9a-1111-7000-8000-000000000001.partial")
        );
        assert_eq!(partial.parent(), Some(root.path()));
    }

    #[test]
    fn partial_publish_preserves_existing_target_without_replace_mode() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = ApprovedLeaf::approve(root.path(), "export.rpd-move").expect("approve");
        std::fs::write(target.path(), b"old").expect("old target");
        let partial = write_encrypted_partial(
            &target,
            "018f7f9a-1111-7000-8000-000000000001",
            "passphrase",
            &manifest(b"SQLite fixture"),
            &TransportKeyMaterial::from_bytes([3; 32]),
            Cursor::new(b"SQLite fixture"),
            &FIXTURE_KEYS,
            AgeEnvelopeOptions::TEST_FAST,
        )
        .expect("partial");

        let error = publish_verified_partial(
            &LocalAtomicFileAdapter,
            &partial,
            &target,
            PublishMode::CreateNew,
        )
        .expect_err("must reject unapproved overwrite");

        assert!(matches!(
            error,
            super::PortablePackageStagingError::Atomic(
                crate::services::data_store::atomic_file::AtomicFileError::AlreadyExists
            )
        ));
        assert_eq!(std::fs::read(target.path()).expect("read"), b"old");
    }

    fn manifest(sqlite: &[u8]) -> PortableMigrationManifest {
        let mut record_counts = BTreeMap::new();
        record_counts.insert("station_keys".to_string(), 0);
        record_counts.insert("stations".to_string(), 0);
        PortableMigrationManifest {
            format: "relay-pool-portable-migration".to_string(),
            format_version: 1,
            export_id: "018f7f9a-1111-7000-8000-000000000001".to_string(),
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
            included_categories: vec!["core_data".to_string(), "history".to_string()],
            excluded_categories: vec![
                "session_credentials".to_string(),
                "local_proxy_access_key".to_string(),
                "device_runtime_state".to_string(),
                "provider_drafts".to_string(),
            ],
            record_counts,
            sqlite_size_bytes: sqlite.len() as u64,
            sqlite_sha256: general_purpose::STANDARD.encode(Sha256::digest(sqlite)),
        }
    }
}
