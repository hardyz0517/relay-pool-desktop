use std::sync::Arc;

use crate::{
    application::{app_services::AppServices, data_directory::DataDirectoryPort},
    persistence::runtime::PersistenceHandle,
    services::{pricing_catalog::StaticBuiltinModelBasePriceCatalog, secrets::vault::DataKeyVault},
};

pub(crate) fn compose_app_services(
    runtime: PersistenceHandle,
    data_key: [u8; 32],
    data_dir: String,
    pending_data_dir: Option<String>,
    data_directory_port: Arc<dyn DataDirectoryPort>,
) -> AppServices {
    AppServices::for_runtime(
        runtime,
        data_dir,
        pending_data_dir,
        data_directory_port,
        Arc::new(DataKeyVault::new(data_key)),
        Arc::new(StaticBuiltinModelBasePriceCatalog),
    )
}
