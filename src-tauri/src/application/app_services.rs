use std::sync::Arc;

use super::{
    changes::ChangeService,
    clock::{Clock, SystemClock},
    collectors::CollectorService,
    credentials::{CredentialService, CredentialVault},
    data_directory::{DataDirectoryPort, DataDirectoryService},
    ids::{IdGenerator, UuidV7Generator},
    monitoring::MonitoringService,
    pricing::{BuiltinModelBasePriceCatalog, PricingService},
    queries::{channel_status::ChannelStatusQuery, pricing_comparison::PricingComparisonQuery},
    request_finalization::RequestFinalizationService,
    request_logs::RequestLogService,
    routing::RoutingService,
    settings::SettingsService,
    stations::StationService,
};

#[derive(Clone)]
pub(crate) struct AppServices {
    pub(crate) stations: Arc<StationService>,
    pub(crate) changes: Arc<ChangeService>,
    pub(crate) data_directory: Arc<DataDirectoryService>,
    pub(crate) credentials: Arc<CredentialService>,
    pub(crate) collectors: Arc<CollectorService>,
    pub(crate) routing: Arc<RoutingService>,
    pub(crate) request_finalization: Arc<RequestFinalizationService>,
    pub(crate) request_logs: Arc<RequestLogService>,
    pub(crate) monitoring: Arc<MonitoringService>,
    pub(crate) pricing: Arc<PricingService>,
    pub(crate) channel_status: Arc<ChannelStatusQuery>,
    pub(crate) pricing_comparison: Arc<PricingComparisonQuery>,
    pub(crate) settings: Arc<SettingsService>,
}

impl AppServices {
    pub(crate) fn for_runtime(
        runtime: crate::persistence::runtime::PersistenceHandle,
        data_dir: String,
        pending_data_dir: Option<String>,
        data_directory_port: Arc<dyn DataDirectoryPort>,
        credential_vault: Arc<dyn CredentialVault>,
        builtin_price_catalog: Arc<dyn BuiltinModelBasePriceCatalog>,
    ) -> Self {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(UuidV7Generator);
        let settings = Arc::new(SettingsService::new(
            runtime.clone(),
            clock.clone(),
            data_dir,
            pending_data_dir,
        ));
        let data_directory = Arc::new(DataDirectoryService::new(
            data_directory_port,
            settings.clone(),
        ));
        Self::new(
            Arc::new(StationService::new(
                runtime.clone(),
                clock.clone(),
                ids.clone(),
            )),
            Arc::new(ChangeService::new(
                runtime.clone(),
                clock.clone(),
                ids.clone(),
            )),
            data_directory,
            Arc::new(CredentialService::new(
                runtime.clone(),
                credential_vault,
                clock.clone(),
                ids.clone(),
            )),
            Arc::new(CollectorService::new(
                runtime.clone(),
                clock.clone(),
                ids.clone(),
            )),
            Arc::new(RoutingService::new(runtime.clone())),
            Arc::new(RequestFinalizationService::new(runtime.clone())),
            Arc::new(RequestLogService::new(runtime.clone())),
            Arc::new(MonitoringService::new(
                runtime.clone(),
                clock.clone(),
                ids.clone(),
            )),
            Arc::new(PricingService::new(
                runtime.clone(),
                clock.clone(),
                ids,
                builtin_price_catalog,
            )),
            Arc::new(ChannelStatusQuery::new(runtime.clone(), clock.clone())),
            Arc::new(PricingComparisonQuery::new(runtime.clone())),
            settings,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        stations: Arc<StationService>,
        changes: Arc<ChangeService>,
        data_directory: Arc<DataDirectoryService>,
        credentials: Arc<CredentialService>,
        collectors: Arc<CollectorService>,
        routing: Arc<RoutingService>,
        request_finalization: Arc<RequestFinalizationService>,
        request_logs: Arc<RequestLogService>,
        monitoring: Arc<MonitoringService>,
        pricing: Arc<PricingService>,
        channel_status: Arc<ChannelStatusQuery>,
        pricing_comparison: Arc<PricingComparisonQuery>,
        settings: Arc<SettingsService>,
    ) -> Self {
        Self {
            stations,
            changes,
            data_directory,
            credentials,
            collectors,
            routing,
            request_finalization,
            request_logs,
            monitoring,
            pricing,
            channel_status,
            pricing_comparison,
            settings,
        }
    }
}
