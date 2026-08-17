use std::sync::Arc;

use super::{
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
        key_pool::KeyPoolQuery, pricing_comparison::PricingComparisonQuery,
        pricing_group_monitor_status::PricingGroupMonitorStatusQuery,
        station_assets::StationAssetsQuery, station_published_status::StationPublishedStatusQuery,
    },
    request_finalization::RequestFinalizationService,
    request_logs::RequestLogService,
    routing::RoutingService,
    settings::SettingsService,
    station_capacity_domains::StationCapacityDomainService,
    stations::StationService,
};
use crate::background_tasks::BlockingExecutor;

#[derive(Clone)]
pub(crate) struct AppServices {
    pub(crate) stations: Arc<StationService>,
    pub(crate) station_capacity_domains: Arc<StationCapacityDomainService>,
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
    pub(crate) pricing_group_monitor_status: Arc<PricingGroupMonitorStatusQuery>,
    pub(crate) station_assets: Arc<StationAssetsQuery>,
    pub(crate) station_published_status: Arc<StationPublishedStatusQuery>,
    pub(crate) key_pool: Arc<KeyPoolQuery>,
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
            Arc::new(StationCapacityDomainService::new(
                runtime.clone(),
                clock.clone(),
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
            Arc::new(PricingGroupMonitorStatusQuery::new(runtime.clone())),
            Arc::new(StationAssetsQuery::new(runtime.clone())),
            Arc::new(StationPublishedStatusQuery::new(
                runtime.clone(),
                clock.clone(),
            )),
            Arc::new(KeyPoolQuery::new(runtime.clone())),
            Arc::new(DashboardMetricsQuery::new(runtime.clone(), clock.clone())),
            settings,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        stations: Arc<StationService>,
        station_capacity_domains: Arc<StationCapacityDomainService>,
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
        pricing_group_monitor_status: Arc<PricingGroupMonitorStatusQuery>,
        station_assets: Arc<StationAssetsQuery>,
        station_published_status: Arc<StationPublishedStatusQuery>,
        key_pool: Arc<KeyPoolQuery>,
        dashboard_metrics: Arc<DashboardMetricsQuery>,
        settings: Arc<SettingsService>,
    ) -> Self {
        Self {
            stations,
            station_capacity_domains,
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
            pricing_group_monitor_status,
            station_assets,
            station_published_status,
            key_pool,
            dashboard_metrics,
            settings,
        }
    }
}
