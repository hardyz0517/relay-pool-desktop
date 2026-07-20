use std::sync::Arc;

use super::{
    collectors::CollectorService, credentials::CredentialService, monitoring::MonitoringService,
    pricing::PricingService, request_finalization::RequestFinalizationService,
    routing::RoutingService, settings::SettingsService, stations::StationService,
};

#[derive(Clone)]
pub(crate) struct AppServices {
    pub(crate) stations: Arc<StationService>,
    pub(crate) credentials: Arc<CredentialService>,
    pub(crate) collectors: Arc<CollectorService>,
    pub(crate) routing: Arc<RoutingService>,
    pub(crate) request_finalization: Arc<RequestFinalizationService>,
    pub(crate) monitoring: Arc<MonitoringService>,
    pub(crate) pricing: Arc<PricingService>,
    pub(crate) settings: Arc<SettingsService>,
}

impl AppServices {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        stations: Arc<StationService>,
        credentials: Arc<CredentialService>,
        collectors: Arc<CollectorService>,
        routing: Arc<RoutingService>,
        request_finalization: Arc<RequestFinalizationService>,
        monitoring: Arc<MonitoringService>,
        pricing: Arc<PricingService>,
        settings: Arc<SettingsService>,
    ) -> Self {
        Self {
            stations,
            credentials,
            collectors,
            routing,
            request_finalization,
            monitoring,
            pricing,
            settings,
        }
    }
}
