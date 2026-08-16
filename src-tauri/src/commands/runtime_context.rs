use tauri::State;

use crate::{commands::error::CommandError, ipc::dto::runtime_context::RuntimeContextRegistry};

/// Bootstrap-only capability handshake for the frontend IPC adapter.
/// The capability is process-local and is never written to runtime events.
#[tauri::command]
pub async fn initialize_runtime_context(
    registry: State<'_, RuntimeContextRegistry>,
) -> Result<String, CommandError> {
    Ok(registry.context_session_id().to_owned())
}
