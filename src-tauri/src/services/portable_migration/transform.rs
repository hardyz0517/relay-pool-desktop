use std::collections::BTreeMap;

use base64::{engine::general_purpose, Engine as _};
use serde_json::Value;

use crate::models::secrets::{SecretRecordSelector, VersionedEncryptedSecret};

use super::{
    catalog::{
        field_rule, secret_policy, setting_policy, FieldTransform, SecretPolicy, SettingPolicy,
        TablePolicy,
    },
    format::PortableFormatError,
    limits::PortableMigrationLimitsV1,
};

pub(crate) type PortableRow = BTreeMap<String, Value>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct TransformOptions {
    pub(crate) include_history: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RowTransform {
    Keep(PortableRow),
    Omit { reason: OmitReason },
    Rebuild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OmitReason {
    OptionalHistoryDisabled,
    ExcludedByPolicy,
    ResetByTarget,
    RegeneratedOnTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum TransformError {
    #[error("portable migration transform references an unknown table")]
    UnknownTable(String),
    #[error("portable migration row contains an undeclared column")]
    UnknownColumn { table: String, column: String },
    #[error("portable migration row contains an undeclared setting")]
    UnknownSetting(String),
    #[error("portable migration row contains an undeclared secret selector")]
    UnknownSecretSelector { scope: String, kind: String },
    #[error("portable migration row is missing a required secret field")]
    MissingSecretField(&'static str),
    #[error("portable migration row contains invalid binary secret material")]
    InvalidSecretBinary,
    #[error("portable migration row contains invalid secret encryption version")]
    InvalidSecretEncryptionVersion,
    #[error("portable migration row contains legacy plaintext secret material")]
    PlaintextSecretResidue { table: String, column: String },
    #[error("portable migration JSON field is invalid")]
    InvalidJson { table: String, column: String },
    #[error("portable migration field exceeds v1 limits")]
    LimitExceeded,
    #[error("portable migration canary scan found sensitive residue")]
    SensitiveResidue,
    #[error("portable migration secret rekey output does not match the source rows")]
    SecretRekeyOutputMismatch,
}

const PORTABLE_BINARY_PREFIX: &str = "base64:";

pub(crate) fn portable_binary_value(bytes: &[u8]) -> Value {
    Value::String(format!(
        "{PORTABLE_BINARY_PREFIX}{}",
        general_purpose::STANDARD.encode(bytes)
    ))
}

pub(crate) fn portable_binary_bytes(value: &Value) -> Result<Vec<u8>, TransformError> {
    let text = value.as_str().ok_or(TransformError::InvalidSecretBinary)?;
    let encoded = text
        .strip_prefix(PORTABLE_BINARY_PREFIX)
        .ok_or(TransformError::InvalidSecretBinary)?;
    general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| TransformError::InvalidSecretBinary)
}

pub(crate) fn encrypted_secret_from_portable_row(
    row: &PortableRow,
) -> Result<VersionedEncryptedSecret, TransformError> {
    let id = required_text(row, "id")?;
    let scope = required_text(row, "scope")?;
    let owner_id = required_text(row, "owner_id")?;
    let kind = required_text(row, "kind")?;
    let key_id = required_text(row, "key_id")?;
    let encryption_version = required_text(row, "encryption_version")?
        .parse::<u16>()
        .map_err(|_| TransformError::InvalidSecretEncryptionVersion)?;
    let value_hash = required_text(row, "value_hash")?;
    let ciphertext = portable_binary_bytes(
        row.get("ciphertext")
            .ok_or(TransformError::MissingSecretField("ciphertext"))?,
    )?;
    let nonce = portable_binary_bytes(
        row.get("nonce")
            .ok_or(TransformError::MissingSecretField("nonce"))?,
    )?;
    Ok(VersionedEncryptedSecret {
        id,
        selector: SecretRecordSelector::new(scope, owner_id, kind),
        key_id,
        encryption_version,
        ciphertext,
        nonce,
        value_hash,
    })
}

pub(crate) fn portable_row_from_encrypted_secret(
    template: &PortableRow,
    secret: &VersionedEncryptedSecret,
) -> PortableRow {
    let mut row = template.clone();
    row.insert("id".into(), Value::String(secret.id.clone()));
    row.insert("scope".into(), Value::String(secret.selector.scope.clone()));
    row.insert(
        "owner_id".into(),
        Value::String(secret.selector.owner_id.clone()),
    );
    row.insert("kind".into(), Value::String(secret.selector.kind.clone()));
    row.insert("key_id".into(), Value::String(secret.key_id.clone()));
    row.insert(
        "encryption_version".into(),
        Value::String(secret.encryption_version.to_string()),
    );
    row.insert(
        "value_hash".into(),
        Value::String(secret.value_hash.clone()),
    );
    row.insert(
        "ciphertext".into(),
        portable_binary_value(&secret.ciphertext),
    );
    row.insert("nonce".into(), portable_binary_value(&secret.nonce));
    row
}

pub(crate) fn transform_row(
    table_name: &str,
    row: &PortableRow,
    options: TransformOptions,
    limits: PortableMigrationLimitsV1,
) -> Result<RowTransform, TransformError> {
    let table = super::catalog::table_catalog(table_name)
        .ok_or_else(|| TransformError::UnknownTable(table_name.to_string()))?;
    validate_known_columns(table_name, row, table.columns)?;

    match table.policy {
        TablePolicy::InternalRebuild | TablePolicy::Reset => return Ok(RowTransform::Rebuild),
        TablePolicy::Exclude => {
            return Ok(RowTransform::Omit {
                reason: OmitReason::ExcludedByPolicy,
            })
        }
        TablePolicy::OptionalHistory if !options.include_history => {
            return Ok(RowTransform::Omit {
                reason: OmitReason::OptionalHistoryDisabled,
            })
        }
        TablePolicy::Include | TablePolicy::IncludeWithTransform | TablePolicy::OptionalHistory => {
        }
    }

    match table_name {
        "settings" => transform_setting_row(row, limits),
        "secrets" => transform_secret_row(row),
        "app_secret_bindings" => transform_secret_binding_row(row),
        "channel_monitor_request_templates" => transform_channel_monitor_template_row(row, limits),
        _ => transform_declared_fields(table_name, row, limits),
    }
}

pub(crate) fn scan_for_sensitive_residue(bytes: &[u8]) -> Result<(), TransformError> {
    let text = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    let canaries = [
        "sk-p8-secret-plaintext-canary",
        "sk-validation-canary",
        "password-plaintext-canary",
        "session=secret-canary",
        "bearer sk-",
        "authorization:",
        "refresh_token=",
        "access_token=",
        "api_key=",
        "cookie:",
    ];
    if canaries.iter().any(|canary| text.contains(canary)) {
        return Err(TransformError::SensitiveResidue);
    }
    Ok(())
}

fn transform_setting_row(
    row: &PortableRow,
    limits: PortableMigrationLimitsV1,
) -> Result<RowTransform, TransformError> {
    let key = text_field(row, "key").ok_or_else(|| TransformError::UnknownSetting("".into()))?;
    match setting_policy(key).ok_or_else(|| TransformError::UnknownSetting(key.to_string()))? {
        SettingPolicy::Include => transform_declared_fields("settings", row, limits),
        SettingPolicy::IncludeWithTransform => {
            let mut next = row.clone();
            if let Some(value) = next.get_mut("value") {
                *value = redact_json_text_value("settings", "value", value, limits)?;
            }
            Ok(RowTransform::Keep(next))
        }
        SettingPolicy::Reset => Ok(RowTransform::Omit {
            reason: OmitReason::RegeneratedOnTarget,
        }),
    }
}

fn transform_secret_row(row: &PortableRow) -> Result<RowTransform, TransformError> {
    let scope = text_field(row, "scope").unwrap_or_default();
    let kind = text_field(row, "kind").unwrap_or_default();
    match secret_policy(scope, kind).ok_or_else(|| TransformError::UnknownSecretSelector {
        scope: scope.to_string(),
        kind: kind.to_string(),
    })? {
        SecretPolicy::IncludeAndRekey | SecretPolicy::IncludeWhenRememberedAndRekey => {
            Ok(RowTransform::Keep(row.clone()))
        }
        SecretPolicy::DeleteAndResetReference => Ok(RowTransform::Omit {
            reason: OmitReason::ResetByTarget,
        }),
        SecretPolicy::ExcludeAndRegenerate => Ok(RowTransform::Omit {
            reason: OmitReason::RegeneratedOnTarget,
        }),
    }
}

fn transform_secret_binding_row(row: &PortableRow) -> Result<RowTransform, TransformError> {
    let scope = text_field(row, "binding_scope").unwrap_or_default();
    let kind = text_field(row, "binding_kind").unwrap_or_default();
    match secret_policy(scope, kind).ok_or_else(|| TransformError::UnknownSecretSelector {
        scope: scope.to_string(),
        kind: kind.to_string(),
    })? {
        SecretPolicy::IncludeAndRekey | SecretPolicy::IncludeWhenRememberedAndRekey => {
            Ok(RowTransform::Keep(row.clone()))
        }
        SecretPolicy::DeleteAndResetReference => Ok(RowTransform::Omit {
            reason: OmitReason::ResetByTarget,
        }),
        SecretPolicy::ExcludeAndRegenerate => Ok(RowTransform::Omit {
            reason: OmitReason::RegeneratedOnTarget,
        }),
    }
}

fn transform_channel_monitor_template_row(
    row: &PortableRow,
    limits: PortableMigrationLimitsV1,
) -> Result<RowTransform, TransformError> {
    if sqlite_truthy(text_field(row, "built_in")) {
        return Ok(RowTransform::Omit {
            reason: OmitReason::RegeneratedOnTarget,
        });
    }
    transform_declared_fields("channel_monitor_request_templates", row, limits)
}

fn sqlite_truthy(value: Option<&str>) -> bool {
    matches!(value, Some("1" | "true" | "TRUE"))
}

fn transform_declared_fields(
    table_name: &str,
    row: &PortableRow,
    limits: PortableMigrationLimitsV1,
) -> Result<RowTransform, TransformError> {
    let mut next = row.clone();
    for (column, value) in row {
        match field_rule(table_name, column).unwrap_or(FieldTransform::Copy) {
            FieldTransform::Copy
            | FieldTransform::SecretReference
            | FieldTransform::ReencryptSecret => validate_value_size(value, limits)?,
            FieldTransform::RequireEmpty => {
                if !value_is_empty(value) {
                    return Err(TransformError::PlaintextSecretResidue {
                        table: table_name.to_string(),
                        column: column.to_string(),
                    });
                }
            }
            FieldTransform::ResetNull | FieldTransform::Exclude => {
                next.insert(column.clone(), Value::Null);
            }
            FieldTransform::ResetText(value) => {
                next.insert(column.clone(), Value::String(value.to_string()));
            }
            FieldTransform::RedactText => {
                next.insert(column.clone(), redact_text_value(value, limits)?);
            }
            FieldTransform::RedactJson => {
                next.insert(
                    column.clone(),
                    redact_json_text_value(table_name, column, value, limits)?,
                );
            }
            FieldTransform::BoundedJson => {
                validate_json_text_value(table_name, column, value, limits)?
            }
        }
    }

    if table_name == "channel_monitors" {
        for runtime_column in [
            "last_run_at",
            "last_run_id",
            "next_run_at",
            "last_status",
            "last_error_message",
        ] {
            next.insert(runtime_column.to_string(), Value::Null);
        }
    }
    if table_name == "station_group_bindings" {
        for runtime_column in [
            "last_seen_at",
            "last_checked_at",
            "last_rate_changed_at",
            "last_seen_run_id",
        ] {
            next.insert(runtime_column.to_string(), Value::Null);
        }
    }
    if table_name == "station_credentials" {
        for runtime_column in [
            "login_error",
            "last_login_at",
            "session_expires_at",
            "access_token_secret_id",
            "refresh_token_secret_id",
            "cookie_secret_id",
            "newapi_user_id",
            "token_expires_at",
            "token_refreshed_at",
        ] {
            next.insert(runtime_column.to_string(), Value::Null);
        }
        next.insert("login_status".into(), Value::String("unknown".into()));
        next.insert("session_status".into(), Value::String("none".into()));
        next.insert("session_source".into(), Value::String("none".into()));
    }

    Ok(RowTransform::Keep(next))
}

fn validate_known_columns(
    table_name: &str,
    row: &PortableRow,
    declared_columns: &[&str],
) -> Result<(), TransformError> {
    for column in row.keys() {
        if !declared_columns.contains(&column.as_str()) {
            return Err(TransformError::UnknownColumn {
                table: table_name.to_string(),
                column: column.to_string(),
            });
        }
    }
    Ok(())
}

fn redact_text_value(
    value: &Value,
    limits: PortableMigrationLimitsV1,
) -> Result<Value, TransformError> {
    validate_value_size(value, limits)?;
    Ok(match value {
        Value::String(text) => Value::String(crate::models::secrets::redact_text(text)),
        Value::Null => Value::Null,
        other => Value::String(crate::models::secrets::redact_text(&other.to_string())),
    })
}

fn redact_json_text_value(
    table: &str,
    column: &str,
    value: &Value,
    limits: PortableMigrationLimitsV1,
) -> Result<Value, TransformError> {
    validate_value_size(value, limits)?;
    let Some(text) = value.as_str() else {
        return Ok(Value::Null);
    };
    if text.trim().is_empty() {
        return Ok(Value::String(text.to_string()));
    }
    let parsed = parse_json_text(table, column, text)?;
    validate_json_depth_for_transform(&parsed, limits)?;
    let redacted = crate::models::secrets::redact_value(&parsed);
    serde_json::to_string(&redacted)
        .map(Value::String)
        .map_err(|_| TransformError::InvalidJson {
            table: table.to_string(),
            column: column.to_string(),
        })
}

fn validate_json_text_value(
    table: &str,
    column: &str,
    value: &Value,
    limits: PortableMigrationLimitsV1,
) -> Result<(), TransformError> {
    validate_value_size(value, limits)?;
    let Some(text) = value.as_str() else {
        return Ok(());
    };
    if text.trim().is_empty() {
        return Ok(());
    }
    let parsed = parse_json_text(table, column, text)?;
    validate_json_depth_for_transform(&parsed, limits)
}

fn parse_json_text(table: &str, column: &str, text: &str) -> Result<Value, TransformError> {
    serde_json::from_str(text).map_err(|_| TransformError::InvalidJson {
        table: table.to_string(),
        column: column.to_string(),
    })
}

fn validate_json_depth_for_transform(
    value: &Value,
    limits: PortableMigrationLimitsV1,
) -> Result<(), TransformError> {
    fn walk(value: &Value, depth: usize, max_depth: usize) -> Result<(), TransformError> {
        if depth > max_depth {
            return Err(TransformError::LimitExceeded);
        }
        match value {
            Value::Array(values) => {
                for value in values {
                    walk(value, depth + 1, max_depth)?;
                }
            }
            Value::Object(values) => {
                for value in values.values() {
                    walk(value, depth + 1, max_depth)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    walk(value, 0, limits.max_json_depth)
}

fn validate_value_size(
    value: &Value,
    limits: PortableMigrationLimitsV1,
) -> Result<(), TransformError> {
    let len = match value {
        Value::String(text) => text.len(),
        other => other.to_string().len(),
    };
    limits
        .validate_large_redacted_json_field_len(len)
        .map_err(|_| TransformError::LimitExceeded)
}

fn value_is_empty(value: &Value) -> bool {
    matches!(value, Value::Null) || value.as_str().is_some_and(str::is_empty)
}

fn text_field<'a>(row: &'a PortableRow, column: &str) -> Option<&'a str> {
    row.get(column).and_then(Value::as_str)
}

fn required_text(row: &PortableRow, column: &'static str) -> Result<String, TransformError> {
    text_field(row, column)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or(TransformError::MissingSecretField(column))
}

impl From<PortableFormatError> for TransformError {
    fn from(_: PortableFormatError) -> Self {
        TransformError::LimitExceeded
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn transform_resets_device_runtime_and_legacy_plaintext_fields() {
        let row = PortableRow::from([
            ("id".into(), json!("key-1")),
            ("station_id".into(), json!("station-1")),
            ("api_key".into(), json!("")),
            ("api_key_secret_id".into(), json!("secret-1")),
            ("status".into(), json!("healthy")),
            ("last_checked_at".into(), json!("2026-07-29T00:00:00Z")),
        ]);

        let transformed = transform_row(
            "station_keys",
            &row,
            TransformOptions::default(),
            PortableMigrationLimitsV1::CURRENT,
        )
        .expect("transform");
        let RowTransform::Keep(row) = transformed else {
            panic!("expected kept row");
        };
        assert_eq!(row["api_key"], "");
        assert_eq!(row["status"], "unchecked");
        assert!(row["last_checked_at"].is_null());

        let mut bad = row.clone();
        bad.insert("api_key".into(), json!("sk-p8-secret-plaintext-canary"));
        assert!(matches!(
            transform_row(
                "station_keys",
                &bad,
                TransformOptions::default(),
                PortableMigrationLimitsV1::CURRENT
            ),
            Err(TransformError::PlaintextSecretResidue { .. })
        ));
    }

    #[test]
    fn settings_and_secret_selectors_are_fail_closed() {
        let unknown_setting = PortableRow::from([
            ("key".into(), json!("future_setting")),
            ("value".into(), json!("1")),
        ]);
        assert!(matches!(
            transform_row(
                "settings",
                &unknown_setting,
                TransformOptions::default(),
                PortableMigrationLimitsV1::CURRENT
            ),
            Err(TransformError::UnknownSetting(_))
        ));

        let local_key = PortableRow::from([
            ("key".into(), json!("local_key")),
            ("value".into(), json!("sk-local-plaintext-canary")),
        ]);
        assert_eq!(
            transform_row(
                "settings",
                &local_key,
                TransformOptions::default(),
                PortableMigrationLimitsV1::CURRENT
            )
            .unwrap(),
            RowTransform::Omit {
                reason: OmitReason::RegeneratedOnTarget
            }
        );

        let unknown_secret = PortableRow::from([
            ("scope".into(), json!("station_key")),
            ("kind".into(), json!("future_secret")),
        ]);
        assert!(matches!(
            transform_row(
                "secrets",
                &unknown_secret,
                TransformOptions::default(),
                PortableMigrationLimitsV1::CURRENT
            ),
            Err(TransformError::UnknownSecretSelector { .. })
        ));
    }

    #[test]
    fn optional_history_is_omitted_by_default_and_redacted_when_enabled() {
        let row = PortableRow::from([
            ("id".into(), json!("request-log-1")),
            ("request_id".into(), json!("req-1")),
            (
                "error_message".into(),
                json!("Authorization: Bearer sk-p8-secret-plaintext-canary"),
            ),
            (
                "rejected_candidates_json".into(),
                json!(r#"[{"api_key":"sk-p8-secret-plaintext-canary","model":"gpt"}]"#),
            ),
        ]);

        assert_eq!(
            transform_row(
                "request_logs",
                &row,
                TransformOptions::default(),
                PortableMigrationLimitsV1::CURRENT
            )
            .unwrap(),
            RowTransform::Omit {
                reason: OmitReason::OptionalHistoryDisabled
            }
        );

        let RowTransform::Keep(redacted) = transform_row(
            "request_logs",
            &row,
            TransformOptions {
                include_history: true,
            },
            PortableMigrationLimitsV1::CURRENT,
        )
        .unwrap() else {
            panic!("history row should be kept");
        };
        let serialized = serde_json::to_string(&redacted).unwrap();
        assert!(!serialized.contains("sk-p8-secret-plaintext-canary"));
        assert!(serialized.contains("[REDACTED]"));
    }

    #[test]
    fn builtin_monitor_templates_are_regenerated_on_target() {
        let built_in = PortableRow::from([
            ("id".into(), json!("builtin-chat-completions")),
            ("request_body_json".into(), json!(r#"{"messages":[]}"#)),
            ("built_in".into(), json!("1")),
        ]);
        assert_eq!(
            transform_row(
                "channel_monitor_request_templates",
                &built_in,
                TransformOptions::default(),
                PortableMigrationLimitsV1::CURRENT
            )
            .unwrap(),
            RowTransform::Omit {
                reason: OmitReason::RegeneratedOnTarget
            }
        );

        let custom = PortableRow::from([
            ("id".into(), json!("custom-template")),
            (
                "request_body_json".into(),
                json!(r#"{"api_key":"redacted"}"#),
            ),
            ("built_in".into(), json!("0")),
        ]);
        let RowTransform::Keep(custom) = transform_row(
            "channel_monitor_request_templates",
            &custom,
            TransformOptions::default(),
            PortableMigrationLimitsV1::CURRENT,
        )
        .unwrap() else {
            panic!("custom monitor template should be migrated");
        };
        assert_eq!(custom["built_in"], "0");
    }

    #[test]
    fn existing_monitor_rows_are_classified_without_preserving_runtime_status() {
        let row = PortableRow::from([
            ("id".into(), json!("monitor-1")),
            ("fallback_models_json".into(), json!("[]")),
            ("last_run_at".into(), json!("1")),
            ("last_run_id".into(), json!("run-1")),
            ("next_run_at".into(), json!("2")),
            ("last_status".into(), json!("failed")),
            ("last_error_message".into(), json!("bad cookie: secret")),
        ]);

        let RowTransform::Keep(row) = transform_row(
            "channel_monitors",
            &row,
            TransformOptions::default(),
            PortableMigrationLimitsV1::CURRENT,
        )
        .unwrap() else {
            panic!("monitor config should be kept");
        };
        for column in [
            "last_run_at",
            "last_run_id",
            "next_run_at",
            "last_status",
            "last_error_message",
        ] {
            assert!(row[column].is_null(), "{column} should be reset");
        }
    }

    #[test]
    fn canary_scanner_rejects_sensitive_residue() {
        scan_for_sensitive_residue(b"ordinary portable bytes").expect("clean bytes");
        assert_eq!(
            scan_for_sensitive_residue(b"contains sk-p8-secret-plaintext-canary").unwrap_err(),
            TransformError::SensitiveResidue
        );
    }

    #[test]
    fn encrypted_secret_rows_use_tagged_base64_for_blob_fields() {
        let row = PortableRow::from([
            ("id".into(), json!("secret-1")),
            ("scope".into(), json!("station_key")),
            ("owner_id".into(), json!("key-1")),
            ("kind".into(), json!("api_key")),
            ("masked_value".into(), json!("sk-...nary")),
            ("ciphertext".into(), portable_binary_value(&[1, 2, 3])),
            ("nonce".into(), portable_binary_value(&[4, 5, 6])),
            ("created_at".into(), json!("1")),
            ("updated_at".into(), json!("2")),
            ("key_id".into(), json!("source-key")),
            ("encryption_version".into(), json!("1")),
            ("value_hash".into(), json!("hash")),
        ]);

        let secret = encrypted_secret_from_portable_row(&row).expect("secret row");
        assert_eq!(secret.ciphertext, vec![1, 2, 3]);
        assert_eq!(secret.nonce, vec![4, 5, 6]);

        let next = portable_row_from_encrypted_secret(&row, &secret);
        assert_eq!(next["ciphertext"], portable_binary_value(&[1, 2, 3]));
        assert!(matches!(
            portable_binary_bytes(&json!("not-base64")),
            Err(TransformError::InvalidSecretBinary)
        ));
    }
}
