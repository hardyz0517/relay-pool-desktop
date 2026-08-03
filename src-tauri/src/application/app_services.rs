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
    provider_drafts::ProviderDraftService,
    queries::{
        channel_status::ChannelStatusQuery, dashboard_metrics::DashboardMetricsQuery,
        pricing_comparison::PricingComparisonQuery,
    },
    request_finalization::RequestFinalizationService,
    request_logs::RequestLogService,
    routing::RoutingService,
    settings::SettingsService,
    stations::StationService,
};
use crate::background_tasks::BlockingExecutor;

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
    pub(crate) provider_drafts: Arc<ProviderDraftService>,
    pub(crate) channel_status: Arc<ChannelStatusQuery>,
    pub(crate) pricing_comparison: Arc<PricingComparisonQuery>,
    pub(crate) dashboard_metrics: Arc<DashboardMetricsQuery>,
    pub(crate) settings: Arc<SettingsService>,
}

impl AppServices {
    pub(crate) fn for_runtime(
        runtime: crate::persistence::runtime::PersistenceHandle,
        data_dir: String,
        pending_data_dir: Option<String>,
        data_directory_port: Arc<dyn DataDirectoryPort>,
        blocking: BlockingExecutor,
        credential_vault: Arc<dyn CredentialVault>,
        builtin_price_catalog: Arc<dyn BuiltinModelBasePriceCatalog>,
    ) -> Self {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(UuidV7Generator);
        let settings = Arc::new(SettingsService::new(
            runtime.clone(),
            clock.clone(),
            ids.clone(),
            credential_vault.clone(),
            data_dir,
            pending_data_dir,
        ));
        let data_directory = Arc::new(DataDirectoryService::new(
            data_directory_port,
            settings.clone(),
            blocking,
        ));
        let provider_drafts = Arc::new(ProviderDraftService::new(
            runtime.clone(),
            credential_vault.clone(),
            clock.clone(),
            ids.clone(),
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
            provider_drafts,
            Arc::new(ChannelStatusQuery::new(runtime.clone(), clock.clone())),
            Arc::new(PricingComparisonQuery::new(runtime.clone())),
            Arc::new(DashboardMetricsQuery::new(runtime.clone(), clock.clone())),
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
        provider_drafts: Arc<ProviderDraftService>,
        channel_status: Arc<ChannelStatusQuery>,
        pricing_comparison: Arc<PricingComparisonQuery>,
        dashboard_metrics: Arc<DashboardMetricsQuery>,
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
            provider_drafts,
            channel_status,
            pricing_comparison,
            dashboard_metrics,
            settings,
        }
    }
}
