use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    application::data_migration::registry::{
        PortableMigrationOperationSnapshot, PortableMigrationOperationState,
        PortableMigrationProgress, PortableMigrationTerminal, PortableMigrationTerminalResult,
        PortableOperationKind,
    },
    background_tasks::OperationId,
    commands::error::CommandError,
    services::portable_migration::limits::PortableMigrationLimitsV1,
};

use super::{invalid_input, TypeDescriptor};

const MAX_TOKEN_BYTES: usize = 128;
const MAX_PASSPHRASE_BYTES: usize = 1024;
const MAX_CONFIRMATION_BYTES: usize = 128;
const REPLACE_CURRENT_CONFIRMATION: &str = "替换当前数据";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortableMigrationCapabilityDto {
    pub enabled: bool,
    pub blocked_reasons: Vec<PortableMigrationBlockedReasonDto>,
    pub supported_format: String,
    pub supported_profile: String,
    pub current_schema_profile: String,
    pub history_supported: bool,
    pub limits: PortableMigrationLimitsDto,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortableMigrationBlockedReasonDto {
    SecurityPolicyNotApproved,
    UnsupportedPlatform,
    SecurityBaselineIncomplete,
    CredentialStoreKeyMissing,
    CredentialStoreUnavailable,
    DataStoreNotWritable,
    MaintenanceInProgress,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortableMigrationLimitsDto {
    pub max_age_file_bytes: u64,
    pub max_sqlite_bytes: u64,
    pub max_rows_per_table: u64,
    pub max_total_user_table_rows: u64,
    pub max_json_depth: usize,
    pub max_regular_field_bytes: usize,
    pub max_large_redacted_json_field_bytes: usize,
    pub max_passphrase_utf8_bytes: usize,
    pub export_deadline_ms: u64,
    pub inspection_deadline_ms: u64,
    pub prepare_deadline_ms: u64,
}

impl From<PortableMigrationLimitsV1> for PortableMigrationLimitsDto {
    fn from(limits: PortableMigrationLimitsV1) -> Self {
        Self {
            max_age_file_bytes: limits.max_age_file_bytes,
            max_sqlite_bytes: limits.max_sqlite_bytes,
            max_rows_per_table: limits.max_rows_per_table,
            max_total_user_table_rows: limits.max_total_user_table_rows,
            max_json_depth: limits.max_json_depth,
            max_regular_field_bytes: limits.max_regular_field_bytes,
            max_large_redacted_json_field_bytes: limits.max_large_redacted_json_field_bytes,
            max_passphrase_utf8_bytes: limits.max_passphrase_utf8_bytes,
            export_deadline_ms: millis(limits.export_deadline()),
            inspection_deadline_ms: millis(limits.inspection_deadline()),
            prepare_deadline_ms: millis(limits.prepare_deadline()),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortablePathTokenDto {
    pub path_token: String,
    pub expires_in_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableExportOptionsDto {
    pub include_history: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartPortableExportInputDto {
    pub output_path_token: String,
    pub passphrase: String,
    pub passphrase_confirmation: String,
    pub options: PortableExportOptionsDto,
    pub idempotency_key: String,
}

impl StartPortableExportInputDto {
    pub fn parse(value: Value) -> Result<Self, CommandError> {
        let input: Self = parse_value(value)?;
        validate_token("outputPathToken", &input.output_path_token)?;
        validate_passphrase_pair(&input.passphrase, &input.passphrase_confirmation)?;
        validate_uuid_v7("idempotencyKey", &input.idempotency_key)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectPortableImportInputDto {
    pub input_path_token: String,
    pub passphrase: String,
    pub idempotency_key: String,
}

impl InspectPortableImportInputDto {
    pub fn parse(value: Value) -> Result<Self, CommandError> {
        let input: Self = parse_value(value)?;
        validate_token("inputPathToken", &input.input_path_token)?;
        validate_passphrase("passphrase", &input.passphrase)?;
        validate_uuid_v7("idempotencyKey", &input.idempotency_key)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreparePortableImportInputDto {
    pub inspected_import_id: String,
    pub mode: PortableImportModeDto,
    pub confirmation_text: String,
    pub idempotency_key: String,
}

impl PreparePortableImportInputDto {
    pub fn parse(value: Value) -> Result<Self, CommandError> {
        let input: Self = parse_value(value)?;
        validate_uuid_v7("inspectedImportId", &input.inspected_import_id)?;
        validate_uuid_v7("idempotencyKey", &input.idempotency_key)?;
        match input.mode {
            PortableImportModeDto::RestoreIntoEmpty if input.confirmation_text.is_empty() => {}
            PortableImportModeDto::ReplaceCurrent
                if input.confirmation_text == REPLACE_CURRENT_CONFIRMATION => {}
            _ => {
                return Err(invalid_input(
                    "confirmationText",
                    "confirmation_mismatch",
                    "The import confirmation text does not match the selected mode.",
                ));
            }
        }
        if input.confirmation_text.len() > MAX_CONFIRMATION_BYTES {
            return Err(invalid_input(
                "confirmationText",
                "too_large",
                "The import confirmation text is too large.",
            ));
        }
        Ok(input)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PortableImportModeDto {
    RestoreIntoEmpty,
    ReplaceCurrent,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortableMigrationOperationStartedDto {
    pub operation_id: String,
    pub resource_id: String,
    pub resource_kind: PortableMigrationResourceKindDto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PortableMigrationResourceKindDto {
    Export,
    Inspection,
    Import,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableMigrationResultInputDto {
    pub resource_id: String,
}

impl PortableMigrationResultInputDto {
    pub fn parse(value: Value) -> Result<Self, CommandError> {
        let input: Self = parse_value(value)?;
        validate_uuid_v7("resourceId", &input.resource_id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableMigrationOperationInputDto {
    pub operation_id: String,
}

impl PortableMigrationOperationInputDto {
    pub fn parse(value: Value) -> Result<Self, CommandError> {
        let input: Self = parse_value(value)?;
        parse_operation_id("operationId", &input.operation_id)?;
        Ok(input)
    }

    pub fn operation_id(&self) -> OperationId {
        parse_operation_id("operationId", &self.operation_id)
            .expect("PortableMigrationOperationInputDto is validated during parse")
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortableMigrationOperationDto {
    pub operation_id: String,
    pub kind: PortableMigrationOperationKindDto,
    pub state: PortableMigrationOperationStateDto,
    pub deadline_ms: u64,
    pub progress: Vec<PortableMigrationProgressDto>,
    pub terminal: Option<PortableMigrationTerminalDto>,
}

impl From<PortableMigrationOperationSnapshot> for PortableMigrationOperationDto {
    fn from(snapshot: PortableMigrationOperationSnapshot) -> Self {
        Self {
            operation_id: snapshot.operation_id.as_u64().to_string(),
            kind: PortableMigrationOperationKindDto::from(snapshot.kind),
            state: PortableMigrationOperationStateDto::from(snapshot.state),
            deadline_ms: millis(snapshot.deadline),
            progress: snapshot
                .progress
                .into_iter()
                .map(PortableMigrationProgressDto::from)
                .collect(),
            terminal: snapshot.terminal.map(PortableMigrationTerminalDto::from),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortableMigrationOperationKindDto {
    ExportPackage,
    InspectPackage,
    PrepareImport,
}

impl From<PortableOperationKind> for PortableMigrationOperationKindDto {
    fn from(kind: PortableOperationKind) -> Self {
        match kind {
            PortableOperationKind::ExportPackage => Self::ExportPackage,
            PortableOperationKind::InspectPackage => Self::InspectPackage,
            PortableOperationKind::PrepareImport => Self::PrepareImport,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortableMigrationOperationStateDto {
    Running,
    Stopping,
    Terminal,
}

impl From<PortableMigrationOperationState> for PortableMigrationOperationStateDto {
    fn from(state: PortableMigrationOperationState) -> Self {
        match state {
            PortableMigrationOperationState::Running => Self::Running,
            PortableMigrationOperationState::Stopping => Self::Stopping,
            PortableMigrationOperationState::Terminal => Self::Terminal,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum PortableMigrationProgressDto {
    Queued,
    KdfStarted,
    KdfFinished,
    ReadingPackage { percent: u8, bytes_read: u64 },
    WritingDatabase { percent: u8, rows_written: u64 },
    PublishingPackage { percent: u8, bytes_written: u64 },
    VerifyingPackage,
}

impl From<PortableMigrationProgress> for PortableMigrationProgressDto {
    fn from(progress: PortableMigrationProgress) -> Self {
        match progress {
            PortableMigrationProgress::Queued => Self::Queued,
            PortableMigrationProgress::KdfStarted => Self::KdfStarted,
            PortableMigrationProgress::KdfFinished => Self::KdfFinished,
            PortableMigrationProgress::ReadingPackage {
                percent,
                bytes_read,
            } => Self::ReadingPackage {
                percent,
                bytes_read,
            },
            PortableMigrationProgress::WritingDatabase {
                percent,
                rows_written,
            } => Self::WritingDatabase {
                percent,
                rows_written,
            },
            PortableMigrationProgress::PublishingPackage {
                percent,
                bytes_written,
            } => Self::PublishingPackage {
                percent,
                bytes_written,
            },
            PortableMigrationProgress::VerifyingPackage => Self::VerifyingPackage,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "terminal", rename_all = "snake_case")]
pub enum PortableMigrationTerminalDto {
    Completed {
        result: PortableMigrationTerminalResultDto,
    },
    Failed {
        code: String,
    },
    Cancelled,
    TimedOut,
    ResultUnknown,
}

impl From<PortableMigrationTerminal> for PortableMigrationTerminalDto {
    fn from(terminal: PortableMigrationTerminal) -> Self {
        match terminal {
            PortableMigrationTerminal::Completed { result } => Self::Completed {
                result: PortableMigrationTerminalResultDto::from(result),
            },
            PortableMigrationTerminal::Failed { code } => Self::Failed { code },
            PortableMigrationTerminal::Cancelled => Self::Cancelled,
            PortableMigrationTerminal::TimedOut => Self::TimedOut,
            PortableMigrationTerminal::ResultUnknown => Self::ResultUnknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum PortableMigrationTerminalResultDto {
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

impl From<PortableMigrationTerminalResult> for PortableMigrationTerminalResultDto {
    fn from(result: PortableMigrationTerminalResult) -> Self {
        match result {
            PortableMigrationTerminalResult::ExportedPackage {
                export_id,
                package_size_bytes,
            } => Self::ExportedPackage {
                export_id,
                package_size_bytes,
            },
            PortableMigrationTerminalResult::InspectedPackage {
                export_id,
                source_platform,
                included_categories,
                sqlite_size_bytes,
            } => Self::InspectedPackage {
                export_id,
                source_platform,
                included_categories,
                sqlite_size_bytes,
            },
            PortableMigrationTerminalResult::PreparedImport {
                export_id,
                target_rows,
            } => Self::PreparedImport {
                export_id,
                target_rows,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortableExportResultDto {
    pub export_id: String,
    pub package_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortableImportInspectionDto {
    pub inspection_id: String,
    pub export_id: String,
    pub source_platform: String,
    pub included_categories: Vec<String>,
    pub include_history: bool,
    pub sqlite_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortableImportPrepareResultDto {
    pub import_id: String,
    pub restart_required: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PortableImportRecoveryStateDto {
    None,
    ActivationPending {
        import_id: String,
    },
    Activated {
        import_id: String,
    },
    RolledBack {
        import_id: String,
        reason_code: PortableImportRecoveryReasonCodeDto,
    },
    ManualRecoveryRequired {
        import_id: Option<String>,
        reason_code: PortableImportRecoveryReasonCodeDto,
    },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortableImportRecoveryReasonCodeDto {
    ActivationValidationFailed,
    AtomicReplaceFailed,
    JournalInvalid,
    ArtifactIdentityMismatch,
    RollbackValidationFailed,
}

pub const DATA_MIGRATION_TYPE: TypeDescriptor = TypeDescriptor {
    name: "DataMigration",
    typescript: include_str!("data_migration.typescript.txt"),
};

fn parse_value<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, CommandError> {
    serde_json::from_value(value).map_err(|_| {
        invalid_input(
            "input",
            "invalid_shape",
            "The command input shape is invalid.",
        )
    })
}

fn validate_token(field: &'static str, value: &str) -> Result<(), CommandError> {
    if value.is_empty() || value.len() > MAX_TOKEN_BYTES || value.chars().any(char::is_control) {
        return Err(invalid_input(
            field,
            "invalid_token",
            "The path token is invalid.",
        ));
    }
    Ok(())
}

fn validate_passphrase_pair(passphrase: &str, confirmation: &str) -> Result<(), CommandError> {
    validate_passphrase("passphrase", passphrase)?;
    validate_passphrase("passphraseConfirmation", confirmation)?;
    if passphrase != confirmation {
        return Err(invalid_input(
            "passphraseConfirmation",
            "confirmation_mismatch",
            "The migration passphrase confirmation does not match.",
        ));
    }
    Ok(())
}

fn validate_passphrase(field: &'static str, value: &str) -> Result<(), CommandError> {
    if value.is_empty() || value.len() > MAX_PASSPHRASE_BYTES || value.chars().any(char::is_control)
    {
        return Err(invalid_input(
            field,
            "invalid_secret",
            "The migration passphrase is invalid.",
        ));
    }
    Ok(())
}

fn validate_uuid_v7(field: &'static str, value: &str) -> Result<(), CommandError> {
    let id = uuid::Uuid::parse_str(value).map_err(|_| {
        invalid_input(
            field,
            "invalid_uuid_v7",
            "The identifier must be a canonical UUIDv7.",
        )
    })?;
    if id.get_version_num() != 7 || id.to_string() != value {
        return Err(invalid_input(
            field,
            "invalid_uuid_v7",
            "The identifier must be a canonical UUIDv7.",
        ));
    }
    Ok(())
}

fn parse_operation_id(field: &'static str, value: &str) -> Result<OperationId, CommandError> {
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_input(
            field,
            "invalid_operation_id",
            "The operation identifier is invalid.",
        ));
    }
    let id = value.parse::<u64>().map_err(|_| {
        invalid_input(
            field,
            "invalid_operation_id",
            "The operation identifier is invalid.",
        )
    })?;
    OperationId::from_u64(id).ok_or_else(|| {
        invalid_input(
            field,
            "invalid_operation_id",
            "The operation identifier is invalid.",
        )
    })
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_export_rejects_unknown_fields_and_password_mismatch() {
        let id = uuid::Uuid::now_v7().to_string();
        let error = StartPortableExportInputDto::parse(serde_json::json!({
            "outputPathToken": "token",
            "passphrase": "RPD_TEST_PASSWORD_CANARY",
            "passphraseConfirmation": "different",
            "options": { "includeHistory": false },
            "idempotencyKey": id,
            "unexpected": true
        }))
        .expect_err("unknown field");
        assert_eq!(
            error.code,
            crate::commands::error::CommandErrorCode::InvalidInput
        );

        let id = uuid::Uuid::now_v7().to_string();
        let error = StartPortableExportInputDto::parse(serde_json::json!({
            "outputPathToken": "token",
            "passphrase": "RPD_TEST_PASSWORD_CANARY",
            "passphraseConfirmation": "different",
            "options": { "includeHistory": false },
            "idempotencyKey": id
        }))
        .expect_err("mismatch");
        let serialized = serde_json::to_string(&error).expect("error json");
        assert!(!serialized.contains("RPD_TEST_PASSWORD_CANARY"));
    }

    #[test]
    fn prepare_confirmation_is_mode_exact_and_utf8_sensitive() {
        let id = uuid::Uuid::now_v7().to_string();
        let inspected = uuid::Uuid::now_v7().to_string();
        PreparePortableImportInputDto::parse(serde_json::json!({
            "inspectedImportId": inspected,
            "mode": "replaceCurrent",
            "confirmationText": "替换当前数据",
            "idempotencyKey": id
        }))
        .expect("exact replace confirmation");

        let id = uuid::Uuid::now_v7().to_string();
        let inspected = uuid::Uuid::now_v7().to_string();
        assert!(PreparePortableImportInputDto::parse(serde_json::json!({
            "inspectedImportId": inspected,
            "mode": "replaceCurrent",
            "confirmationText": " 替换当前数据",
            "idempotencyKey": id
        }))
        .is_err());

        let id = uuid::Uuid::now_v7().to_string();
        let inspected = uuid::Uuid::now_v7().to_string();
        PreparePortableImportInputDto::parse(serde_json::json!({
            "inspectedImportId": inspected,
            "mode": "restoreIntoEmpty",
            "confirmationText": "",
            "idempotencyKey": id
        }))
        .expect("empty restore confirmation");
    }

    #[test]
    fn ids_are_closed_over_uuid_v7_and_positive_operation_ids() {
        assert!(PortableMigrationResultInputDto::parse(serde_json::json!({
            "resourceId": uuid::Uuid::now_v7().to_string()
        }))
        .is_ok());
        assert!(PortableMigrationResultInputDto::parse(serde_json::json!({
            "resourceId": uuid::Uuid::new_v4().to_string()
        }))
        .is_err());
        assert!(
            PortableMigrationOperationInputDto::parse(serde_json::json!({
                "operationId": "1"
            }))
            .is_ok()
        );
        assert!(
            PortableMigrationOperationInputDto::parse(serde_json::json!({
                "operationId": "000"
            }))
            .is_err()
        );
    }
}
