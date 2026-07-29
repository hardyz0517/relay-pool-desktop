use serde_json::Value;
use tauri::State;

use crate::{
    app_composition::ManagedWorkRuntime,
    commands::error,
    ipc::dto::{
        updater_data_recovery::{
            PublishedUpdateInspectionDto, PublishedUpdateInspectionInputDto,
            UpdaterNetworkConfigDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
    services::updater,
};

#[tauri::command]
pub async fn updater_network_config(
    input: Value,
) -> Result<UpdaterNetworkConfigDto, error::CommandError> {
    correlation::in_command_scope("updater_network_config", async {
        EmptyInputDto::parse(input)?;
        Ok(updater::network_config())
    })
    .await
}

#[tauri::command]
pub async fn inspect_latest_update_manifest(
    runtime: State<'_, ManagedWorkRuntime>,
    input: Value,
) -> Result<PublishedUpdateInspectionDto, error::CommandError> {
    correlation::in_command_scope("inspect_latest_update_manifest", async {
        let input = PublishedUpdateInspectionInputDto::parse(input)?;
        Ok(
            updater::inspect_latest_update_manifest(&runtime.outbound, &input.current_version)
                .await?,
        )
    })
    .await
}
