use std::sync::Arc;

use crate::{
    application::{
        clock::{Clock, SystemClock},
        settings::SettingsService,
    },
    persistence::runtime::PersistenceHandle,
};

pub(crate) fn settings_service_for_v2_tests(
    runtime: PersistenceHandle,
    data_dir: String,
    pending_data_dir: Option<String>,
) -> SettingsService {
    SettingsService::new(
        runtime,
        Arc::new(SystemClock) as Arc<dyn Clock>,
        data_dir,
        pending_data_dir,
    )
}
