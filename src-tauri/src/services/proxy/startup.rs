use std::sync::Arc;

use crate::{
    application::app_services::AppServices,
    models::proxy::ProxyStatus,
    services::proxy::{
        lifecycle::ports::RequestLifecycleStore,
        routing_repository::{RoutingRepository, V2RoutingRepository},
    },
};

use super::runtime::{ProxyRuntimeState, ProxyStartConfig};

pub(crate) async fn start_from_v2_persisted_settings(
    services: &AppServices,
    data_key: [u8; 32],
    proxy: &ProxyRuntimeState,
) -> Result<ProxyStatus, String> {
    let settings = services
        .settings
        .load()
        .await
        .map_err(|error| error.to_string())?;
    let local_access_key = services
        .settings
        .ensure_local_access_key()
        .await
        .map_err(|error| error.to_string())?;
    proxy
        .start(config_from_v2_services(
            services,
            data_key,
            local_access_key,
            settings.local_proxy_port,
        ))
        .await
}

pub(crate) fn config_from_v2_services(
    services: &AppServices,
    data_key: [u8; 32],
    local_access_key: String,
    port: u16,
) -> ProxyStartConfig {
    let routing_repository: Arc<dyn RoutingRepository> = Arc::new(V2RoutingRepository::new(
        services.routing.as_ref().clone(),
        data_key,
    ));
    let lifecycle_store: Arc<dyn RequestLifecycleStore> = services.request_finalization.clone();
    ProxyStartConfig::new_v2(routing_repository, lifecycle_store, local_access_key, port)
}

#[cfg(test)]
mod tests {
    use crate::{
        models::settings::UpdateSettingsInput,
        services::proxy::{runtime::ProxyRuntimeState, test_support::V2ProxyTestFixture},
    };

    use super::*;

    #[tokio::test]
    async fn persisted_settings_start_uses_configured_proxy_port() {
        let fixture = V2ProxyTestFixture::new().await;
        let port = next_free_port().await;
        update_proxy_port(&fixture.services, port).await;
        let runtime = ProxyRuntimeState::for_tests();

        let status =
            start_from_v2_persisted_settings(&fixture.services, fixture.data_key, &runtime)
                .await
                .expect("start proxy");

        assert!(status.running);
        assert_eq!(status.port, port);
        runtime.stop(port).await.expect("stop proxy");
    }

    async fn next_free_port() -> u16 {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind free port");
        listener.local_addr().expect("local address").port()
    }

    async fn update_proxy_port(services: &AppServices, port: u16) {
        let settings = services.settings.load().await.expect("settings");
        services
            .settings
            .update(UpdateSettingsInput {
                local_proxy_port: port,
                default_routing_strategy: settings.default_routing_strategy,
                collector_proxy_mode: settings.collector_proxy_mode,
                collector_proxy_url: settings.collector_proxy_url,
                max_rate_multiplier: Some(settings.max_rate_multiplier),
                default_routing_group_filter: Some(settings.default_routing_group_filter),
                scheduler_advanced_settings: Some(settings.scheduler_advanced_settings),
                low_balance_threshold_cny: settings.low_balance_threshold_cny,
                collector_interval_minutes: settings.collector_interval_minutes,
                balance_interval_minutes: settings.balance_interval_minutes,
                group_rate_interval_minutes: settings.group_rate_interval_minutes,
                model_list_interval_minutes: settings.model_list_interval_minutes,
                pricing_refresh_interval_minutes: settings.pricing_refresh_interval_minutes,
                collector_timeout_seconds: settings.collector_timeout_seconds,
                collector_max_concurrency: settings.collector_max_concurrency,
                allow_depleted_fallback: settings.allow_depleted_fallback,
                developer_mode_enabled: settings.developer_mode_enabled,
                tray_behavior: Some(settings.tray_behavior),
            })
            .await
            .expect("update proxy port");
    }
}
