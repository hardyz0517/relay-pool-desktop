use std::{path::PathBuf, sync::Arc};

use crate::{
    application::{error::ApplicationError, settings::SettingsService},
    background_tasks::{BlockingExecutor, BlockingExecutorError},
    models::settings::AppSettings,
    observability::correlation,
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
    blocking: BlockingExecutor,
}

impl DataDirectoryService {
    pub(crate) fn new(
        port: Arc<dyn DataDirectoryPort>,
        settings: Arc<SettingsService>,
        blocking: BlockingExecutor,
    ) -> Self {
        Self {
            port,
            settings,
            blocking,
        }
    }

    pub(crate) async fn select_pending(
        &self,
        target: PathBuf,
    ) -> Result<AppSettings, ApplicationError> {
        let port = self.port.clone();
        let selection = self
            .blocking
            .submit(
                "data_directory_select_pending",
                None,
                current_correlation_id(),
                None,
                move |_| Ok(port.select_pending(target)),
            )
            .map_err(map_blocking_error)?
            .result()
            .await
            .map_err(map_blocking_error)?
            .map_err(map_port_error)?;
        self.apply_selection(selection).await
    }

    pub(crate) async fn reset_to_default(&self) -> Result<AppSettings, ApplicationError> {
        let port = self.port.clone();
        let selection = self
            .blocking
            .submit(
                "data_directory_reset_to_default",
                None,
                current_correlation_id(),
                None,
                move |_| Ok(port.reset_to_default()),
            )
            .map_err(map_blocking_error)?
            .result()
            .await
            .map_err(map_blocking_error)?
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

fn map_blocking_error(error: BlockingExecutorError) -> ApplicationError {
    match error {
        BlockingExecutorError::QueueFull | BlockingExecutorError::QueueTimeout => {
            ApplicationError::Unavailable
        }
        BlockingExecutorError::ExecutionTimeout
        | BlockingExecutorError::CancelledBeforeStart
        | BlockingExecutorError::CancelledLateResultDiscarded
        | BlockingExecutorError::Closed
        | BlockingExecutorError::Panicked
        | BlockingExecutorError::JobFailed { .. }
        | BlockingExecutorError::ShutdownTimeout { .. } => ApplicationError::Internal,
    }
}

fn current_correlation_id() -> Option<String> {
    correlation::current().map(|id| id.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_errors_keep_existing_application_categories() {
        assert!(matches!(
            map_port_error(DataDirectoryError::InvalidTarget),
            ApplicationError::ConstraintViolation
        ));
        assert!(matches!(
            map_port_error(DataDirectoryError::Io),
            ApplicationError::IoFailed
        ));
    }

    #[test]
    fn blocking_executor_errors_map_to_stable_application_categories() {
        assert!(matches!(
            map_blocking_error(BlockingExecutorError::QueueFull),
            ApplicationError::Unavailable
        ));
        assert!(matches!(
            map_blocking_error(BlockingExecutorError::QueueTimeout),
            ApplicationError::Unavailable
        ));
        assert!(matches!(
            map_blocking_error(BlockingExecutorError::ExecutionTimeout),
            ApplicationError::Internal
        ));
        assert!(matches!(
            map_blocking_error(BlockingExecutorError::CancelledBeforeStart),
            ApplicationError::Internal
        ));
    }
}
