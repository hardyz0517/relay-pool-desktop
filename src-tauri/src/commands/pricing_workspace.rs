use serde_json::Value;
use tauri::State;

use crate::{
    application::{command_facades::PricingCommandFacade, pagination::PageLimit},
    commands::error,
    ipc::dto::{pricing_reads::PricingComparisonWorkspaceDto, EmptyInputDto},
    observability::correlation,
};

#[tauri::command]
pub async fn load_pricing_comparison_workspace(
    facade: State<'_, PricingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<PricingComparisonWorkspaceDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "load_pricing_comparison_workspace",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .load_pricing_comparison_workspace(PageLimit::new(500).expect("bounded limit"))
                .await
                .map(PricingComparisonWorkspaceDto::from)
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn load_pricing_group_monitor_status(
    facade: State<'_, PricingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<
    crate::ipc::dto::pricing_reads::PricingGroupMonitorStatusWorkspaceDto,
    error::CommandError,
> {
    correlation::in_command_scope_with_runtime_context(
        "load_pricing_group_monitor_status",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input =
                crate::ipc::dto::pricing_reads::PricingGroupMonitorStatusInputDto::parse(input)?
                    .into_domain();
            facade
                .load_pricing_group_monitor_status(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}
