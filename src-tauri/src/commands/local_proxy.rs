use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tauri::State;

use crate::{
    application::command_facades::LocalProxyCommandFacade,
    commands::error,
    ipc::dto::{proxy_workspace_reads::ProxyStatusDto, EmptyInputDto},
    observability::correlation,
    services::proxy::runtime::ProxyRuntimeState,
};

#[tauri::command]
pub async fn get_proxy_status(
    facade: State<'_, LocalProxyCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_proxy_status",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .get_proxy_status()
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn start_local_proxy(
    facade: State<'_, LocalProxyCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "start_local_proxy",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .start_local_proxy()
                .await
                .map_err(super::public_local_proxy_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn stop_local_proxy(
    facade: State<'_, LocalProxyCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "stop_local_proxy",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .stop_local_proxy()
                .await
                .map_err(super::public_local_proxy_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn cleanup_before_update(
    facade: State<'_, LocalProxyCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "cleanup_before_update",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .cleanup_before_update()
                .await
                .map_err(super::public_local_proxy_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn prepare_local_proxy_for_update(
    proxy: State<'_, Arc<ProxyRuntimeState>>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "prepare_local_proxy_for_update",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            Ok(proxy.prepare_for_update(Duration::from_secs(30)).await?)
        },
    )
    .await
}

#[tauri::command]
pub async fn restart_local_proxy(
    facade: State<'_, LocalProxyCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "restart_local_proxy",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .restart_local_proxy()
                .await
                .map_err(super::public_local_proxy_error)
        },
    )
    .await
}
