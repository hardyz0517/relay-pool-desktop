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
