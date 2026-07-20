use crate::{models::proxy::ProxyStatus, services::database::AppDatabase};

use super::runtime::{ProxyRuntimeState, ProxyStartConfig};

pub async fn start_from_persisted_settings(
    database: &AppDatabase,
    data_key: [u8; 32],
    proxy: &ProxyRuntimeState,
) -> Result<ProxyStatus, String> {
    let settings = database.get_settings()?;
    database.migrate_plaintext_secrets(&data_key)?;
    proxy
        .start(ProxyStartConfig::new(
            database.clone(),
            data_key,
            settings.local_proxy_port,
        ))
        .await
}

#[cfg(test)]
mod tests {
    use crate::{
        models::settings::UpdateSettingsInput,
        services::{database::AppDatabase, proxy::runtime::ProxyRuntimeState},
    };

    use super::*;

    #[tokio::test]
    async fn persisted_settings_start_uses_configured_proxy_port() {
        let database = AppDatabase::new_temp_file_for_tests("startup").expect("database");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        let port = next_free_port().await;
        update_proxy_port(&database, port);
        let runtime = ProxyRuntimeState::for_tests();

        let status = start_from_persisted_settings(&database, data_key, &runtime)
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

    fn update_proxy_port(database: &AppDatabase, port: u16) {
        let settings = database.get_settings().expect("settings");
        database
            .update_settings(UpdateSettingsInput {
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
            })
            .expect("update proxy port");
    }
}
