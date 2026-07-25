use std::sync::Arc;

use crate::{
    application::{
        app_services::AppServices,
        command_facades::{
            ChangeEventsCommandFacade, ChannelMonitoringCommandFacade, ChannelStatusCommandFacade,
            CredentialsCommandFacade, KeyPoolCommandFacade, PricingCommandFacade,
            RequestLogsCommandFacade, RoutingCommandFacade, SettingsStationsCommandFacade,
        },
        data_directory::DataDirectoryPort,
    },
    persistence::runtime::PersistenceHandle,
    services::{pricing_catalog::StaticBuiltinModelBasePriceCatalog, secrets::vault::DataKeyVault},
    TrayBehaviorState,
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

pub(crate) fn compose_settings_stations_command_facade(
    services: &AppServices,
    tray_behavior: Arc<TrayBehaviorState>,
) -> SettingsStationsCommandFacade {
    SettingsStationsCommandFacade::new(
        Arc::clone(&services.stations),
        Arc::clone(&services.settings),
        tray_behavior,
    )
}

pub(crate) fn compose_key_pool_command_facade(services: &AppServices) -> KeyPoolCommandFacade {
    KeyPoolCommandFacade::new(Arc::clone(&services.credentials))
}

pub(crate) fn compose_routing_command_facade(services: &AppServices) -> RoutingCommandFacade {
    RoutingCommandFacade::new(Arc::clone(&services.routing))
}

pub(crate) fn compose_request_logs_command_facade(
    services: &AppServices,
) -> RequestLogsCommandFacade {
    RequestLogsCommandFacade::new(Arc::clone(&services.request_logs))
}

pub(crate) fn compose_channel_monitoring_command_facade(
    services: &AppServices,
) -> ChannelMonitoringCommandFacade {
    ChannelMonitoringCommandFacade::new(Arc::clone(&services.monitoring))
}

pub(crate) fn compose_channel_status_command_facade(
    services: &AppServices,
) -> ChannelStatusCommandFacade {
    ChannelStatusCommandFacade::new(Arc::clone(&services.channel_status))
}

pub(crate) fn compose_pricing_command_facade(services: &AppServices) -> PricingCommandFacade {
    PricingCommandFacade::new(
        Arc::clone(&services.pricing),
        Arc::clone(&services.pricing_comparison),
    )
}

pub(crate) fn compose_change_events_command_facade(
    services: &AppServices,
) -> ChangeEventsCommandFacade {
    ChangeEventsCommandFacade::new(Arc::clone(&services.changes))
}

pub(crate) fn compose_credentials_command_facade(
    services: &AppServices,
) -> CredentialsCommandFacade {
    CredentialsCommandFacade::new(Arc::clone(&services.credentials))
}
