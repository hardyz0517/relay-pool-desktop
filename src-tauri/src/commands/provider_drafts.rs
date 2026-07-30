use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::{ProviderDraftCommandError, ProviderDraftCommandFacade},
    commands::error,
    ipc::dto::{
        provider_drafts::{
            CollectProviderDraftPreviewInputDto, CommitProviderDraftInputDto,
            CreateProviderDraftInputDto, PatchProviderDraftInputDto, ProviderDraftDto,
            ProviderDraftIdInputDto, ProviderDraftPreviewDto,
        },
        stations::StationDto,
    },
    models::remote_keys::RemoteKeyScanResult,
    observability::correlation,
};

fn command_error(error: ProviderDraftCommandError) -> error::CommandError {
    match error {
        ProviderDraftCommandError::Application(error) => {
            super::public_command_application_error(error)
        }
        ProviderDraftCommandError::Blocking(error) => super::public_blocking_executor_error(error),
        ProviderDraftCommandError::Remote(error) => super::key_pool::public_remote_key_error(error),
    }
}

#[tauri::command]
pub async fn create_or_resume_provider_draft(
    facade: State<'_, ProviderDraftCommandFacade>,
    input: Value,
) -> Result<ProviderDraftDto, error::CommandError> {
    correlation::in_command_scope("create_or_resume_provider_draft", async {
        facade
            .create_or_resume(CreateProviderDraftInputDto::parse(input)?)
            .await
            .map_err(command_error)
    })
    .await
}

#[tauri::command]
pub async fn get_provider_draft(
    facade: State<'_, ProviderDraftCommandFacade>,
    input: Value,
) -> Result<ProviderDraftDto, error::CommandError> {
    correlation::in_command_scope("get_provider_draft", async {
        let input = ProviderDraftIdInputDto::parse(input)?;
        facade.get(input.draft_id).await.map_err(command_error)
    })
    .await
}

#[tauri::command]
pub async fn patch_provider_draft(
    facade: State<'_, ProviderDraftCommandFacade>,
    input: Value,
) -> Result<ProviderDraftDto, error::CommandError> {
    correlation::in_command_scope("patch_provider_draft", async {
        facade
            .patch(PatchProviderDraftInputDto::parse(input)?)
            .await
            .map_err(command_error)
    })
    .await
}

#[tauri::command]
pub async fn discard_provider_draft(
    facade: State<'_, ProviderDraftCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("discard_provider_draft", async {
        let input = ProviderDraftIdInputDto::parse(input)?;
        facade.discard(input.draft_id).await.map_err(command_error)
    })
    .await
}

#[tauri::command]
pub async fn collect_provider_draft_preview(
    facade: State<'_, ProviderDraftCommandFacade>,
    input: Value,
) -> Result<ProviderDraftPreviewDto, error::CommandError> {
    correlation::in_command_scope("collect_provider_draft_preview", async {
        let (draft_id, task) = CollectProviderDraftPreviewInputDto::parse(input)?;
        facade
            .collect_preview(draft_id, task)
            .await
            .map_err(command_error)
    })
    .await
}

#[tauri::command]
pub async fn scan_provider_draft_remote_keys(
    facade: State<'_, ProviderDraftCommandFacade>,
    input: Value,
) -> Result<RemoteKeyScanResult, error::CommandError> {
    correlation::in_command_scope("scan_provider_draft_remote_keys", async {
        let input = ProviderDraftIdInputDto::parse(input)?;
        facade
            .scan_remote_keys(input.draft_id)
            .await
            .map_err(command_error)
    })
    .await
}

#[tauri::command]
pub async fn commit_provider_draft(
    facade: State<'_, ProviderDraftCommandFacade>,
    input: Value,
) -> Result<StationDto, error::CommandError> {
    correlation::in_command_scope("commit_provider_draft", async {
        facade
            .commit(CommitProviderDraftInputDto::parse(input)?)
            .await
            .map(StationDto::from)
            .map_err(command_error)
    })
    .await
}
