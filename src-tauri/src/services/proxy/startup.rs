use std::sync::Arc;

use crate::{
    application::{app_services::AppServices, routing_execution_reader::RoutingExecutionReader},
    models::proxy::ProxyStatus,
    services::proxy::{
        lifecycle::ports::RequestLifecycleStore,
        routing_repository::{RoutingExecutionRepository, RoutingRepository},
        transport_policy::TransportPolicySnapshot,
    },
};

use super::runtime::{ProxyRuntimeState, ProxyStartConfig};

pub(crate) async fn start_from_v2_persisted_settings(
    services: &AppServices,
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
    services
        .request_finalization
        .reconcile_terminal_outbox()
        .await
        .map_err(|error| format!("startup terminal outbox reconciliation failed: {error:?}"))?;
    services
        .request_finalization
        .reconcile_startup_interrupted_request_lifecycle()
        .await
        .map_err(|error| format!("startup request lifecycle reconciliation failed: {error:?}"))?;
    let stored_policy = services
        .routing_policy_read
        .load_routing_policy()
        .await
        .map_err(|error| format!("startup routing policy load failed: {error:?}"))?;
    let policy = crate::models::routing_policy::RoutingPolicyConfigV2::from_stored_value(
        &stored_policy.config,
    )
    .map_err(|error| format!("startup routing policy invalid: {error:?}"))?;
    let transport_policy = TransportPolicySnapshot::from_timeout_policy(
        &policy.timeout_policy,
        stored_policy.revision,
        super::limits::ProxyStartupResourceLimits::default().upstream_pool_idle_timeout,
    )
    .map_err(|error| format!("startup transport policy invalid: {error:?}"))?;
    proxy
        .start(
            config_from_v2_services(services, local_access_key, settings.local_proxy_port)
                .with_transport_policy(transport_policy),
        )
        .await
}

pub(crate) fn config_from_v2_services(
    services: &AppServices,
    local_access_key: String,
    port: u16,
) -> ProxyStartConfig {
    let execution_reader = Arc::new(RoutingExecutionReader::new(services.routing.clone()));
    let routing_repository: Arc<dyn RoutingRepository> =
        Arc::new(RoutingExecutionRepository::new(execution_reader));
    let lifecycle_store: Arc<dyn RequestLifecycleStore> = services.request_finalization.clone();
    ProxyStartConfig::new_v2(
        routing_repository,
        services.credentials.clone(),
        lifecycle_store,
        local_access_key,
        port,
    )
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

        let status = start_from_v2_persisted_settings(&fixture.services, &runtime)
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
                collector_proxy_mode: settings.collector_proxy_mode,
                collector_proxy_url: settings.collector_proxy_url,
                low_balance_threshold_cny: settings.low_balance_threshold_cny,
                collector_interval_minutes: settings.collector_interval_minutes,
                balance_interval_minutes: settings.balance_interval_minutes,
                group_rate_interval_minutes: settings.group_rate_interval_minutes,
                published_status_interval_minutes: settings.published_status_interval_minutes,
                pricing_refresh_interval_minutes: settings.pricing_refresh_interval_minutes,
                collector_timeout_seconds: settings.collector_timeout_seconds,
                collector_max_concurrency: settings.collector_max_concurrency,
                developer_mode_enabled: settings.developer_mode_enabled,
                show_decision_explanation: settings.show_decision_explanation,
                tray_behavior: Some(settings.tray_behavior),
            })
            .await
            .expect("update proxy port");
    }
}
