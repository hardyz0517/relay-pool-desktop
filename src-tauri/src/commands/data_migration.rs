use serde_json::Value;
use tauri::State;

use crate::{
    application::data_migration::{PortableMigrationCommandError, PortableMigrationCommandFacade},
    commands::error,
    ipc::dto::{
        data_migration::{
            InspectPortableImportInputDto, PortableExportResultDto, PortableImportInspectionDto,
            PortableImportPrepareResultDto, PortableImportRecoveryStateDto,
            PortableMigrationCapabilityDto, PortableMigrationOperationDto,
            PortableMigrationOperationInputDto, PortableMigrationOperationStartedDto,
            PortableMigrationResultInputDto, PortablePathTokenDto, PreparePortableImportInputDto,
            StartPortableExportInputDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn get_portable_migration_capability(
    facade: State<'_, PortableMigrationCommandFacade>,
    input: Value,
) -> Result<PortableMigrationCapabilityDto, error::CommandError> {
    correlation::in_command_scope("get_portable_migration_capability", async {
        EmptyInputDto::parse(input)?;
        Ok(facade.capability())
    })
    .await
}

#[tauri::command]
pub async fn choose_portable_export_path(
    facade: State<'_, PortableMigrationCommandFacade>,
    input: Value,
) -> Result<Option<PortablePathTokenDto>, error::CommandError> {
    correlation::in_command_scope("choose_portable_export_path", async {
        EmptyInputDto::parse(input)?;
        facade
            .choose_export_path()
            .map_err(public_portable_migration_error)
    })
    .await
}

#[tauri::command]
pub async fn start_portable_export(
    facade: State<'_, PortableMigrationCommandFacade>,
    input: Value,
) -> Result<PortableMigrationOperationStartedDto, error::CommandError> {
    correlation::in_command_scope("start_portable_export", async {
        let input = StartPortableExportInputDto::parse(input)?;
        facade
            .start_portable_export(input)
            .map_err(public_portable_migration_error)
    })
    .await
}

#[tauri::command]
pub async fn get_portable_export_result(
    facade: State<'_, PortableMigrationCommandFacade>,
    input: Value,
) -> Result<PortableExportResultDto, error::CommandError> {
    correlation::in_command_scope("get_portable_export_result", async {
        let input = PortableMigrationResultInputDto::parse(input)?;
        facade
            .get_portable_export_result(input.resource_id)
            .map_err(public_portable_migration_error)
    })
    .await
}

#[tauri::command]
pub async fn choose_portable_import_file(
    facade: State<'_, PortableMigrationCommandFacade>,
    input: Value,
) -> Result<Option<PortablePathTokenDto>, error::CommandError> {
    correlation::in_command_scope("choose_portable_import_file", async {
        EmptyInputDto::parse(input)?;
        facade
            .choose_import_file()
            .map_err(public_portable_migration_error)
    })
    .await
}

#[tauri::command]
pub async fn start_portable_import_inspection(
    facade: State<'_, PortableMigrationCommandFacade>,
    input: Value,
) -> Result<PortableMigrationOperationStartedDto, error::CommandError> {
    correlation::in_command_scope("start_portable_import_inspection", async {
        let input = InspectPortableImportInputDto::parse(input)?;
        facade
            .start_portable_import_inspection(input)
            .map_err(public_portable_migration_error)
    })
    .await
}

#[tauri::command]
pub async fn get_portable_import_inspection(
    facade: State<'_, PortableMigrationCommandFacade>,
    input: Value,
) -> Result<PortableImportInspectionDto, error::CommandError> {
    correlation::in_command_scope("get_portable_import_inspection", async {
        let input = PortableMigrationResultInputDto::parse(input)?;
        facade
            .get_portable_import_inspection(input.resource_id)
            .map_err(public_portable_migration_error)
    })
    .await
}

#[tauri::command]
pub async fn start_portable_import_prepare(
    facade: State<'_, PortableMigrationCommandFacade>,
    input: Value,
) -> Result<PortableMigrationOperationStartedDto, error::CommandError> {
    correlation::in_command_scope("start_portable_import_prepare", async {
        let input = PreparePortableImportInputDto::parse(input)?;
        facade
            .start_portable_import_prepare(input)
            .map_err(public_portable_migration_error)
    })
    .await
}

#[tauri::command]
pub async fn get_portable_import_prepare_result(
    facade: State<'_, PortableMigrationCommandFacade>,
    input: Value,
) -> Result<PortableImportPrepareResultDto, error::CommandError> {
    correlation::in_command_scope("get_portable_import_prepare_result", async {
        let input = PortableMigrationResultInputDto::parse(input)?;
        facade
            .get_portable_import_prepare_result(input.resource_id)
            .map_err(public_portable_migration_error)
    })
    .await
}

#[tauri::command]
pub async fn get_portable_migration_operation(
    facade: State<'_, PortableMigrationCommandFacade>,
    input: Value,
) -> Result<PortableMigrationOperationDto, error::CommandError> {
    correlation::in_command_scope("get_portable_migration_operation", async {
        let input = PortableMigrationOperationInputDto::parse(input)?;
        facade
            .operation(input.operation_id())
            .map_err(public_portable_migration_error)
    })
    .await
}

#[tauri::command]
pub async fn get_portable_import_recovery_state(
    facade: State<'_, PortableMigrationCommandFacade>,
    input: Value,
) -> Result<PortableImportRecoveryStateDto, error::CommandError> {
    correlation::in_command_scope("get_portable_import_recovery_state", async {
        EmptyInputDto::parse(input)?;
        Ok(facade.recovery_state())
    })
    .await
}

pub(crate) fn public_portable_migration_error(
    error: PortableMigrationCommandError,
) -> error::CommandError {
    match error {
        PortableMigrationCommandError::FeatureUnavailable => error::CommandError::try_new(
            error::CommandErrorCode::Unsupported,
            "Portable migration is not enabled by the current security policy.",
            false,
            Some(error::PublicErrorDetails::Validation {
                fields: vec![error::PublicFieldError {
                    field: "feature".to_string(),
                    code: "feature_unavailable".to_string(),
                    message: "Portable migration is disabled.".to_string(),
                }],
            }),
            None,
        )
        .expect("portable migration feature-disabled error is bounded"),
        PortableMigrationCommandError::ResultUnknown => error::CommandError::from_work(
            error::WorkFailure::ResultUnknown,
        ),
        PortableMigrationCommandError::PathRejected
        | PortableMigrationCommandError::PathToken(_) => error::CommandError::try_new(
            error::CommandErrorCode::InvalidInput,
            "The selected portable migration path is invalid.",
            false,
            Some(error::PublicErrorDetails::Validation {
                fields: vec![error::PublicFieldError {
                    field: "pathToken".to_string(),
                    code: "invalid_path_token".to_string(),
                    message: "The selected path could not be approved.".to_string(),
                }],
            }),
            None,
        )
        .expect("portable migration path error is bounded"),
        PortableMigrationCommandError::Registry(error) => match error {
            crate::application::data_migration::registry::PortableMigrationRegistryError::Operation(
                error,
            ) => super::public_operation_registry_error(error),
            crate::application::data_migration::registry::PortableMigrationRegistryError::OwnerMismatch
            | crate::application::data_migration::registry::PortableMigrationRegistryError::NotFound => {
                error::CommandError::try_new(
                    error::CommandErrorCode::NotFound,
                    "The portable migration operation was not found.",
                    false,
                    None,
                    None,
                )
                .expect("portable operation not-found error is bounded")
            }
            crate::application::data_migration::registry::PortableMigrationRegistryError::CompletedResultMissing => {
                error::CommandError::from_work(error::WorkFailure::ResultUnknown)
            }
            crate::application::data_migration::registry::PortableMigrationRegistryError::IdempotencyConflict
            | crate::application::data_migration::registry::PortableMigrationRegistryError::PrepareAlreadyOwned => {
                error::CommandError::try_new(
                    error::CommandErrorCode::Conflict,
                    "A portable migration operation with different input already exists.",
                    false,
                    None,
                    None,
                )
                .expect("portable operation conflict error is bounded")
            }
            crate::application::data_migration::registry::PortableMigrationRegistryError::InvalidProgress => {
                error::CommandError::internal(None)
            }
        },
        PortableMigrationCommandError::ReadyServicesUnavailable => error::CommandError::try_new(
            error::CommandErrorCode::DataStoreUnavailable,
            "Portable migration is unavailable until the data store is ready.",
            true,
            None,
            None,
        )
        .expect("portable migration ready-services error is bounded"),
        PortableMigrationCommandError::Export(_) | PortableMigrationCommandError::Import(_) => {
            error::CommandError::from_work(error::WorkFailure::Internal)
        }
        PortableMigrationCommandError::IdempotencyConflict => error::CommandError::try_new(
            error::CommandErrorCode::Conflict,
            "A portable migration idempotency key is already bound to different input.",
            false,
            None,
            None,
        )
        .expect("portable migration idempotency conflict error is bounded"),
    }
}
