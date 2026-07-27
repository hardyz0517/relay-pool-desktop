use serde_json::Value;
use tauri::State;

use crate::{
    app_composition::ManagedWorkRuntime,
    commands::error,
    ipc::{
        dto::{runtime_status::RuntimeStatusDto, settings::AppStatusDto, EmptyInputDto},
        runtime_contract::{current_runtime_contract, RuntimeContractInfo},
    },
    models::AppStatus,
    observability::correlation,
};

#[tauri::command]
pub async fn app_status(input: Value) -> Result<AppStatusDto, error::CommandError> {
    correlation::in_command_scope("app_status", async {
        EmptyInputDto::parse(input)?;
        Ok(AppStatus::default())
    })
    .await
}

/// Returns only the immutable build/IPC identity needed before normal app queries.
#[tauri::command]
pub async fn get_runtime_contract_info(
    input: Value,
) -> Result<RuntimeContractInfo, error::CommandError> {
    correlation::in_command_scope("get_runtime_contract_info", async {
        EmptyInputDto::parse(input)?;
        Ok(current_runtime_contract())
    })
    .await
}

#[tauri::command]
pub async fn get_runtime_status(
    runtime: State<'_, ManagedWorkRuntime>,
    input: Value,
) -> Result<RuntimeStatusDto, error::CommandError> {
    correlation::in_command_scope("get_runtime_status", async {
        EmptyInputDto::parse(input)?;
        Ok(RuntimeStatusDto::from(
            runtime
                .supervisor
                .statuses()
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
        ))
    })
    .await
}
