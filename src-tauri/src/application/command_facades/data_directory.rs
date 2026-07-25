use std::{path::PathBuf, sync::Arc};

use crate::{
    application::{
        data_directory::DataDirectoryService, error::ApplicationError, settings::SettingsService,
    },
    models::settings::AppSettings,
};

#[derive(Clone)]
pub(crate) struct DataDirectoryCommandFacade {
    data_directory: Arc<DataDirectoryService>,
    settings: Arc<SettingsService>,
}

impl DataDirectoryCommandFacade {
    pub(crate) fn new(
        data_directory: Arc<DataDirectoryService>,
        settings: Arc<SettingsService>,
    ) -> Self {
        Self {
            data_directory,
            settings,
        }
    }

    pub(crate) async fn choose_data_dir(
        &self,
        selected: Option<PathBuf>,
    ) -> Result<AppSettings, ApplicationError> {
        match selected {
            Some(data_dir) => self.data_directory.select_pending(data_dir).await,
            None => self.settings.load().await,
        }
    }

    pub(crate) async fn reset_data_dir(&self) -> Result<AppSettings, ApplicationError> {
        self.data_directory.reset_to_default().await
    }
}
