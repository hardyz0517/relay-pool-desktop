use std::{fs, path::PathBuf};

use crate::application::data_directory::{
    DataDirectoryError, DataDirectoryPort, DataDirectorySelection,
};

use super::relocation::{write_active_data_dir_selection, write_relocation_intent};

pub(crate) struct FileDataDirectoryPort {
    default_data_dir: PathBuf,
    active_data_dir: PathBuf,
}

impl FileDataDirectoryPort {
    pub(crate) fn new(default_data_dir: PathBuf, active_data_dir: PathBuf) -> Self {
        Self {
            default_data_dir,
            active_data_dir,
        }
    }

    fn selection(&self, pending: PathBuf) -> DataDirectorySelection {
        DataDirectorySelection {
            active: self.active_data_dir.display().to_string(),
            pending: Some(pending.display().to_string()),
        }
    }
}

impl DataDirectoryPort for FileDataDirectoryPort {
    fn select_pending(
        &self,
        target: PathBuf,
    ) -> Result<DataDirectorySelection, DataDirectoryError> {
        if target.as_os_str().is_empty() {
            return Err(DataDirectoryError::InvalidTarget);
        }
        fs::create_dir_all(&target).map_err(|_| DataDirectoryError::Io)?;
        write_relocation_intent(&self.default_data_dir, &self.active_data_dir, &target)
            .map_err(|_| DataDirectoryError::Io)?;
        Ok(self.selection(target))
    }

    fn reset_to_default(&self) -> Result<DataDirectorySelection, DataDirectoryError> {
        fs::create_dir_all(&self.default_data_dir).map_err(|_| DataDirectoryError::Io)?;
        if self.active_data_dir == self.default_data_dir {
            write_active_data_dir_selection(&self.default_data_dir, &self.default_data_dir)
                .map_err(|_| DataDirectoryError::Io)?;
        } else {
            write_relocation_intent(
                &self.default_data_dir,
                &self.active_data_dir,
                &self.default_data_dir,
            )
            .map_err(|_| DataDirectoryError::Io)?;
        }
        Ok(self.selection(self.default_data_dir.clone()))
    }
}
