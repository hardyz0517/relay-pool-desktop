use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::Instant,
};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    application::settings::generate_local_access_key,
    services::{
        portable_migration::{
            inspection_registry::{ImportInspectionId, ImportPreparationLease},
            schema_reader::{ordered_import_tables_v1, PortableReaderKind, PortableSchemaReader},
            target_writer::{
                validate_rebuilt_target_database, TrustedTableBatch, TrustedTargetWriter,
            },
            transform::{
                encrypted_secret_from_portable_row, portable_row_from_encrypted_secret,
                PortableRow, TransformOptions,
            },
            validate::PortableMigrationValidationError,
        },
        secrets::{
            rekey::{
                BufferedSecretRekeyWriter, SecretRekeyPolicy, SecretRekeyReport,
                SecretRekeyService, TransportSecretKey,
            },
            DeviceKeyResolver, CURRENT_SECRET_ENCRYPTION_VERSION,
        },
    },
};

use super::errors::DataMigrationImportError;

pub(crate) const REPLACE_CURRENT_CONFIRMATION: &str = "替换当前数据";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PortableImportMode {
    RestoreIntoEmpty,
    ReplaceCurrent,
}

#[derive(Debug, Clone)]
pub(crate) struct PortableImportPrepareRequest {
    pub(crate) inspected_import_id: ImportInspectionId,
    pub(crate) active_database_path: PathBuf,
    pub(crate) target_database_path: PathBuf,
    pub(crate) mode: PortableImportMode,
    pub(crate) confirmation_text: String,
    pub(crate) target_keys: DeviceKeyResolver,
    pub(crate) target_updated_at: String,
    pub(crate) now: Instant,
}

#[derive(Debug, Clone)]
pub(crate) struct PortableImportPrepareArtifact {
    pub(crate) target_database_path: PathBuf,
    pub(crate) target_sha256: String,
    pub(crate) target_key_id: String,
    pub(crate) row_counts: BTreeMap<String, usize>,
    pub(crate) rekey_report: SecretRekeyReport,
    pub(crate) mode: PortableImportMode,
}

pub(crate) fn validate_import_mode(
    mode: PortableImportMode,
    confirmation_text: &str,
) -> Result<(), DataMigrationImportError> {
    match mode {
        PortableImportMode::RestoreIntoEmpty if confirmation_text.is_empty() => Ok(()),
        PortableImportMode::ReplaceCurrent if confirmation_text == REPLACE_CURRENT_CONFIRMATION => {
            Ok(())
        }
        _ => Err(DataMigrationImportError::ConfirmationTextMismatch),
    }
}

pub(crate) async fn build_target_from_inspection(
    lease: &ImportPreparationLease,
    request: &PortableImportPrepareRequest,
) -> Result<PortableImportPrepareArtifact, DataMigrationImportError> {
    if lease.reader_kind != PortableReaderKind::V1EncryptedSecrets {
        return Err(DataMigrationImportError::Validation(
            PortableMigrationValidationError::UnsupportedSchema,
        ));
    }

    let include_history = lease
        .manifest
        .included_categories
        .iter()
        .any(|category| category == "history");
    let transport_key = lease.transport_key.with_bytes(|bytes| {
        TransportSecretKey::from_parts(lease.manifest.transport_key_id.clone(), *bytes)
    });
    let mut reader = PortableSchemaReader::open_v1(&lease.staging_path).await?;
    let mut batches = Vec::new();
    let mut rekey_report = None;

    for table_name in ordered_import_tables_v1() {
        let rows = reader
            .read_transformed_table(table_name, TransformOptions { include_history })
            .await?;
        let rows = if table_name == "settings" {
            with_target_local_defaults(rows, &request.target_updated_at)
        } else if table_name == "secrets" {
            let (rows, report) = rekey_secret_rows(rows, &transport_key, &request.target_keys)?;
            rekey_report = Some(report);
            rows
        } else {
            rows
        };
        batches.push(TrustedTableBatch {
            table_name: table_name.to_string(),
            rows,
        });
    }
    reader.close().await?;

    let target_existed_before = request.target_database_path.exists();
    let build_result = async {
        TrustedTargetWriter
            .rebuild_current_database(&request.target_database_path, &batches)
            .await?;

        let row_counts = validate_rebuilt_target_database(
            &request.target_database_path,
            request.target_keys.active_key_id().as_str(),
            transport_key.key_id(),
        )
        .await?;
        let target_sha256 = file_sha256_hex(&request.target_database_path)?;
        Ok::<_, DataMigrationImportError>((row_counts, target_sha256))
    }
    .await;
    let (row_counts, target_sha256) = match build_result {
        Ok(result) => result,
        Err(error) => {
            if !target_existed_before {
                cleanup_unpublished_target(&request.target_database_path);
            }
            return Err(error);
        }
    };

    Ok(PortableImportPrepareArtifact {
        target_database_path: request.target_database_path.clone(),
        target_sha256,
        target_key_id: request.target_keys.active_key_id().as_str().to_string(),
        row_counts,
        rekey_report: rekey_report.unwrap_or(SecretRekeyReport {
            from_key_id: transport_key.key_id().to_string(),
            to_key_id: request.target_keys.active_key_id().as_str().to_string(),
            included_rows: 0,
            dropped_rows: 0,
            reset_rows: 0,
            code: "ok",
        }),
        mode: request.mode,
    })
}

fn with_target_local_defaults(mut rows: Vec<PortableRow>, updated_at: &str) -> Vec<PortableRow> {
    rows.retain(|row| {
        !matches!(
            row.get("key").and_then(Value::as_str),
            Some("local_key" | "local_proxy_start_on_launch")
        )
    });
    rows.push(setting_row(
        "local_key",
        &generate_local_access_key(),
        updated_at,
    ));
    rows.push(setting_row(
        "local_proxy_start_on_launch",
        "false",
        updated_at,
    ));
    rows
}

fn setting_row(key: &str, value: &str, updated_at: &str) -> PortableRow {
    PortableRow::from([
        ("key".to_string(), Value::String(key.to_string())),
        ("value".to_string(), Value::String(value.to_string())),
        (
            "updated_at".to_string(),
            Value::String(updated_at.to_string()),
        ),
    ])
}

fn rekey_secret_rows(
    rows: Vec<PortableRow>,
    transport_key: &TransportSecretKey,
    target_keys: &DeviceKeyResolver,
) -> Result<(Vec<PortableRow>, SecretRekeyReport), DataMigrationImportError> {
    let mut templates = BTreeMap::new();
    let mut secrets = Vec::with_capacity(rows.len());
    for row in rows {
        let secret = encrypted_secret_from_portable_row(&row)
            .map_err(PortableMigrationValidationError::from)?;
        templates.insert(secret.id.clone(), row);
        secrets.push(secret);
    }

    let service = SecretRekeyService::new(
        transport_key.resolver(),
        target_keys.clone(),
        CURRENT_SECRET_ENCRYPTION_VERSION,
    );
    let mut writer = BufferedSecretRekeyWriter::create_new();
    let report = service.rekey(
        secrets,
        &SecretRekeyPolicy::include_all(),
        &mut writer,
        None,
    )?;

    let rows = writer
        .rows()
        .iter()
        .map(|secret| {
            let template = templates
                .get(&secret.id)
                .expect("rekey writer preserves source secret IDs");
            portable_row_from_encrypted_secret(template, secret)
        })
        .collect();
    Ok((rows, report))
}

fn file_sha256_hex(path: &Path) -> Result<String, DataMigrationImportError> {
    let mut file = File::open(path).map_err(|_| {
        DataMigrationImportError::Validation(PortableMigrationValidationError::OpenFailed)
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| {
            DataMigrationImportError::Validation(PortableMigrationValidationError::OpenFailed)
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn cleanup_unpublished_target(path: &Path) {
    let _ = fs::remove_file(path);
    for suffix in ["wal", "shm"] {
        let Ok(sidecar) = sqlite_sidecar_path(path, suffix) else {
            continue;
        };
        let _ = fs::remove_file(sidecar);
    }
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> Result<PathBuf, DataMigrationImportError> {
    let file_name = path
        .file_name()
        .ok_or(DataMigrationImportError::Validation(
            PortableMigrationValidationError::UnsupportedSchema,
        ))?;
    let mut sidecar_name = OsString::from(file_name);
    sidecar_name.push(format!("-{suffix}"));
    Ok(path.with_file_name(sidecar_name))
}
