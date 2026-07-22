use std::{path::PathBuf, sync::Arc};

use crate::{
    application::{error::ApplicationError, settings::SettingsService},
    models::settings::AppSettings,
};

#[derive(Debug, Clone)]
pub(crate) struct DataDirectorySelection {
    pub(crate) active: String,
    pub(crate) pending: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DataDirectoryError {
    #[error("invalid data directory")]
    InvalidTarget,
    #[error("data directory I/O failed")]
    Io,
}

pub(crate) trait DataDirectoryPort: Send + Sync {
    fn select_pending(&self, target: PathBuf)
        -> Result<DataDirectorySelection, DataDirectoryError>;

    fn reset_to_default(&self) -> Result<DataDirectorySelection, DataDirectoryError>;
}

#[derive(Clone)]
pub(crate) struct DataDirectoryService {
    port: Arc<dyn DataDirectoryPort>,
    settings: Arc<SettingsService>,
}

impl DataDirectoryService {
    pub(crate) fn new(port: Arc<dyn DataDirectoryPort>, settings: Arc<SettingsService>) -> Self {
        Self { port, settings }
    }

    pub(crate) async fn select_pending(
        &self,
        target: PathBuf,
    ) -> Result<AppSettings, ApplicationError> {
        let port = self.port.clone();
        let selection = tokio::task::spawn_blocking(move || port.select_pending(target))
            .await
            .map_err(|_| ApplicationError::Internal)?
            .map_err(map_port_error)?;
        self.apply_selection(selection).await
    }

    pub(crate) async fn reset_to_default(&self) -> Result<AppSettings, ApplicationError> {
        let port = self.port.clone();
        let selection = tokio::task::spawn_blocking(move || port.reset_to_default())
            .await
            .map_err(|_| ApplicationError::Internal)?
            .map_err(map_port_error)?;
        self.apply_selection(selection).await
    }

    async fn apply_selection(
        &self,
        selection: DataDirectorySelection,
    ) -> Result<AppSettings, ApplicationError> {
        self.settings
            .set_data_directory_projection(selection.active, selection.pending)?;
        self.settings.load().await
    }
}

fn map_port_error(error: DataDirectoryError) -> ApplicationError {
    match error {
        DataDirectoryError::InvalidTarget => ApplicationError::ConstraintViolation,
        DataDirectoryError::Io => ApplicationError::IoFailed,
    }
}
