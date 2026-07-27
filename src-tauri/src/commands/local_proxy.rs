use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tauri::State;

use crate::{
    application::command_facades::LocalProxyCommandFacade,
    commands::error,
    ipc::dto::{
        proxy_workspace_reads::{LocalRoutingWorkspaceDto, ProxyStatusDto},
        routing_mutations::ReorderLocalRoutingKeysInputDto,
        EmptyInputDto,
    },
    observability::correlation,
    services::proxy::runtime::ProxyRuntimeState,
};

#[tauri::command]
pub async fn get_proxy_status(
    facade: State<'_, LocalProxyCommandFacade>,
    input: Value,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope("get_proxy_status", async {
        EmptyInputDto::parse(input)?;
        facade
            .get_proxy_status()
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn load_local_routing_workspace(
    facade: State<'_, LocalProxyCommandFacade>,
    input: Value,
) -> Result<LocalRoutingWorkspaceDto, error::CommandError> {
    correlation::in_command_scope("load_local_routing_workspace", async {
        EmptyInputDto::parse(input)?;
        facade
            .load_local_routing_workspace()
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn reorder_local_routing_keys(
    facade: State<'_, LocalProxyCommandFacade>,
    input: Value,
) -> Result<LocalRoutingWorkspaceDto, error::CommandError> {
    correlation::in_command_scope("reorder_local_routing_keys", async {
        let input = ReorderLocalRoutingKeysInputDto::parse(input)?;
        facade
            .reorder_local_routing_keys(input.station_key_ids)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn start_local_proxy(
    facade: State<'_, LocalProxyCommandFacade>,
    input: Value,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope("start_local_proxy", async {
        EmptyInputDto::parse(input)?;
        facade
            .start_local_proxy()
            .await
            .map_err(super::public_local_proxy_error)
    })
    .await
}

#[tauri::command]
pub async fn stop_local_proxy(
    facade: State<'_, LocalProxyCommandFacade>,
    input: Value,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope("stop_local_proxy", async {
        EmptyInputDto::parse(input)?;
        facade
            .stop_local_proxy()
            .await
            .map_err(super::public_local_proxy_error)
    })
    .await
}

#[tauri::command]
pub async fn cleanup_before_update(
    facade: State<'_, LocalProxyCommandFacade>,
    input: Value,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope("cleanup_before_update", async {
        EmptyInputDto::parse(input)?;
        facade
            .cleanup_before_update()
            .await
            .map_err(super::public_local_proxy_error)
    })
    .await
}

#[tauri::command]
pub async fn prepare_local_proxy_for_update(
    proxy: State<'_, Arc<ProxyRuntimeState>>,
    input: Value,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope("prepare_local_proxy_for_update", async {
        EmptyInputDto::parse(input)?;
        Ok(proxy.prepare_for_update(Duration::from_secs(30)).await?)
    })
    .await
}

#[tauri::command]
pub async fn restart_local_proxy(
    facade: State<'_, LocalProxyCommandFacade>,
    input: Value,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope("restart_local_proxy", async {
        EmptyInputDto::parse(input)?;
        facade
            .restart_local_proxy()
            .await
            .map_err(super::public_local_proxy_error)
    })
    .await
}
