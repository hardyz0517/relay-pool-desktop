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
) -> Result<PricingComparisonWorkspaceDto, error::CommandError> {
    correlation::in_command_scope("load_pricing_comparison_workspace", async {
        EmptyInputDto::parse(input)?;
        facade
            .load_pricing_comparison_workspace(PageLimit::new(500).expect("bounded limit"))
            .await
            .map(PricingComparisonWorkspaceDto::from)
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn load_pricing_group_monitor_status(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<
    crate::ipc::dto::pricing_reads::PricingGroupMonitorStatusWorkspaceDto,
    error::CommandError,
> {
    correlation::in_command_scope("load_pricing_group_monitor_status", async {
        let input =
            crate::ipc::dto::pricing_reads::PricingGroupMonitorStatusInputDto::parse(input)?
                .into_domain();
        facade
            .load_pricing_group_monitor_status(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}
