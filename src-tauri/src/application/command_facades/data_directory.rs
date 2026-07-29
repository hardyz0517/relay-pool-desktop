use std::{path::PathBuf, sync::Arc};

use crate::{
    application::{
        data_directory::DataDirectoryService, error::ApplicationError, settings::SettingsService,
    },
    background_tasks::{BlockingExecutor, BlockingExecutorError},
    models::settings::AppSettings,
    observability::correlation,
};

#[derive(Clone)]
pub(crate) struct DataDirectoryCommandFacade {
    data_directory: Arc<DataDirectoryService>,
    settings: Arc<SettingsService>,
    blocking: BlockingExecutor,
}

impl DataDirectoryCommandFacade {
    pub(crate) fn new(
        data_directory: Arc<DataDirectoryService>,
        settings: Arc<SettingsService>,
        blocking: BlockingExecutor,
    ) -> Self {
        Self {
            data_directory,
            settings,
            blocking,
        }
    }

    pub(crate) async fn choose_data_dir(&self) -> Result<AppSettings, DataDirectoryCommandError> {
        let selected = self
            .pick_folder()
            .await
            .map_err(DataDirectoryCommandError::Blocking)?;
        match selected {
            Some(data_dir) => self
                .data_directory
                .select_pending(data_dir)
                .await
                .map_err(DataDirectoryCommandError::Application),
            None => self
                .settings
                .load()
                .await
                .map_err(DataDirectoryCommandError::Application),
        }
    }

    pub(crate) async fn reset_data_dir(&self) -> Result<AppSettings, ApplicationError> {
        self.data_directory.reset_to_default().await
    }

    async fn pick_folder(&self) -> Result<Option<PathBuf>, BlockingExecutorError> {
        self.blocking
            .submit(
                "data_directory_choose_folder_dialog",
                None,
                current_correlation_id(),
                None,
                |_| Ok(rfd::FileDialog::new().pick_folder()),
            )?
            .result()
            .await
    }
}

fn current_correlation_id() -> Option<String> {
    correlation::current().map(|id| id.as_str().to_string())
}

#[derive(Debug)]
pub(crate) enum DataDirectoryCommandError {
    Application(ApplicationError),
    Blocking(BlockingExecutorError),
}
