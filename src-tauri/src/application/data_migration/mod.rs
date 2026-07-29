pub(crate) mod errors;
pub(crate) mod export_service;
pub(crate) mod import_occupancy;
pub(crate) mod import_prepare;
pub(crate) mod import_service;
pub(crate) mod registry;

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::{
    background_tasks::OperationRegistry,
    ipc::dto::data_migration::{
        PortableImportRecoveryReasonCodeDto, PortableImportRecoveryStateDto,
        PortableMigrationBlockedReasonDto, PortableMigrationCapabilityDto,
        PortableMigrationLimitsDto, PortableMigrationOperationDto,
        PortableMigrationOperationStartedDto, PortablePathTokenDto,
    },
    services::portable_migration::{
        activation_journal::{read_journal, PortableActivationPhase},
        limits::PortableMigrationLimitsV1,
        path_tokens::{PathTokenError, PathTokenRegistry},
    },
};

use self::registry::{PortableMigrationOperationRegistry, PortableMigrationRegistryError};

const SECURITY_POLICY_APPROVED: bool = false;
const PATH_TOKEN_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone)]
pub(crate) struct PortableMigrationCommandFacade {
    config_dir: PathBuf,
    default_data_dir: PathBuf,
    path_tokens: PathTokenRegistry,
    operations: PortableMigrationOperationRegistry,
    limits: PortableMigrationLimitsV1,
}

impl PortableMigrationCommandFacade {
    pub(crate) fn new(
        config_dir: PathBuf,
        default_data_dir: PathBuf,
        operation_registry: OperationRegistry,
    ) -> Self {
        Self {
            config_dir,
            default_data_dir,
            path_tokens: PathTokenRegistry::new(),
            operations: PortableMigrationOperationRegistry::new(operation_registry),
            limits: PortableMigrationLimitsV1::CURRENT,
        }
    }

    pub(crate) fn capability(&self) -> PortableMigrationCapabilityDto {
        let mut blocked_reasons = Vec::new();
        if !SECURITY_POLICY_APPROVED {
            blocked_reasons.push(PortableMigrationBlockedReasonDto::SecurityPolicyNotApproved);
        }
        if !cfg!(target_os = "windows") {
            blocked_reasons.push(PortableMigrationBlockedReasonDto::UnsupportedPlatform);
        }
        if !self.default_data_dir.exists() {
            blocked_reasons.push(PortableMigrationBlockedReasonDto::DataStoreNotWritable);
        }
        blocked_reasons.dedup();
        PortableMigrationCapabilityDto {
            enabled: blocked_reasons.is_empty(),
            blocked_reasons,
            supported_format: "relay-pool-portable-migration".to_string(),
            supported_profile: "portable-migration-v1".to_string(),
            current_schema_profile: "relay-pool-desktop-v10".to_string(),
            history_supported: true,
            limits: PortableMigrationLimitsDto::from(self.limits),
        }
    }

    pub(crate) fn choose_export_path(
        &self,
    ) -> Result<Option<PortablePathTokenDto>, PortableMigrationCommandError> {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Relay Pool migration", &["rpd-move"])
            .set_file_name("relay-pool-data.rpd-move")
            .save_file()
        else {
            return Ok(None);
        };
        let parent = path
            .parent()
            .ok_or(PortableMigrationCommandError::PathRejected)?;
        let leaf = path
            .file_name()
            .ok_or(PortableMigrationCommandError::PathRejected)?
            .to_os_string();
        let token = self
            .path_tokens
            .approve_export_path(parent, leaf, true, Instant::now())?;
        Ok(Some(PortablePathTokenDto {
            path_token: token.id.as_str().to_string(),
            expires_in_ms: PATH_TOKEN_TTL.as_millis().try_into().unwrap_or(u64::MAX),
        }))
    }

    pub(crate) fn choose_import_file(
        &self,
    ) -> Result<Option<PortablePathTokenDto>, PortableMigrationCommandError> {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Relay Pool migration", &["rpd-move"])
            .pick_file()
        else {
            return Ok(None);
        };
        let token = self.path_tokens.approve_import_path(path, Instant::now())?;
        Ok(Some(PortablePathTokenDto {
            path_token: token.id.as_str().to_string(),
            expires_in_ms: PATH_TOKEN_TTL.as_millis().try_into().unwrap_or(u64::MAX),
        }))
    }

    pub(crate) fn start_disabled(
        &self,
    ) -> Result<PortableMigrationOperationStartedDto, PortableMigrationCommandError> {
        Err(PortableMigrationCommandError::FeatureUnavailable)
    }

    pub(crate) fn result_disabled<T>(&self) -> Result<T, PortableMigrationCommandError> {
        Err(PortableMigrationCommandError::ResultUnknown)
    }

    pub(crate) fn operation(
        &self,
        operation_id: crate::background_tasks::OperationId,
    ) -> Result<PortableMigrationOperationDto, PortableMigrationCommandError> {
        self.operations
            .get_portable_migration_operation(operation_id, Instant::now())
            .map(PortableMigrationOperationDto::from)
            .map_err(PortableMigrationCommandError::Registry)
    }

    pub(crate) fn recovery_state(&self) -> PortableImportRecoveryStateDto {
        match read_journal(&self.config_dir) {
            Ok(Some(journal)) => {
                let import_id = journal.payload.operation_id;
                match journal.payload.phase {
                    PortableActivationPhase::Prepared
                    | PortableActivationPhase::ActivationStarted
                    | PortableActivationPhase::ReplacementCommitted
                    | PortableActivationPhase::ActivatedValidated => {
                        PortableImportRecoveryStateDto::ActivationPending { import_id }
                    }
                    PortableActivationPhase::RolledBack => {
                        PortableImportRecoveryStateDto::RolledBack {
                            import_id,
                            reason_code:
                                PortableImportRecoveryReasonCodeDto::ActivationValidationFailed,
                        }
                    }
                    PortableActivationPhase::ManualRecoveryRequired => {
                        PortableImportRecoveryStateDto::ManualRecoveryRequired {
                            import_id: Some(import_id),
                            reason_code:
                                PortableImportRecoveryReasonCodeDto::ArtifactIdentityMismatch,
                        }
                    }
                    PortableActivationPhase::RollbackStarted => {
                        PortableImportRecoveryStateDto::ManualRecoveryRequired {
                            import_id: Some(import_id),
                            reason_code:
                                PortableImportRecoveryReasonCodeDto::RollbackValidationFailed,
                        }
                    }
                    PortableActivationPhase::Completed => {
                        PortableImportRecoveryStateDto::Activated { import_id }
                    }
                }
            }
            Ok(None) => PortableImportRecoveryStateDto::None,
            Err(_) => PortableImportRecoveryStateDto::ManualRecoveryRequired {
                import_id: None,
                reason_code: PortableImportRecoveryReasonCodeDto::JournalInvalid,
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PortableMigrationCommandError {
    #[error("portable migration is disabled by policy")]
    FeatureUnavailable,
    #[error("portable migration result is unavailable")]
    ResultUnknown,
    #[error("portable migration path was rejected")]
    PathRejected,
    #[error("portable migration path token failed")]
    PathToken(#[from] PathTokenError),
    #[error("portable migration operation registry failed")]
    Registry(#[from] PortableMigrationRegistryError),
}
