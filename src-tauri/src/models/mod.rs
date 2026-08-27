pub(crate) mod alerting;
pub mod capture;
pub mod channel_monitors;
pub mod collector;
pub mod collector_runs;
pub mod credentials;
pub mod dashboard_metrics;
pub(crate) mod document_sync;
pub mod group_facts;
pub(crate) mod health;
pub(crate) mod model_mapping;
pub mod monitoring;
pub mod operational;
pub mod pricing;
pub mod pricing_group_monitoring;
pub mod provider_drafts;
pub mod proxy;
pub mod remote_keys;
pub mod routing;
pub(crate) mod routing_observation;
pub(crate) mod routing_policy;
pub(crate) mod routing_read_models;
pub mod secrets;
pub mod settings;
pub mod shared_capabilities;
pub mod station_capacity_domains;
pub mod station_endpoints;
pub mod station_keys;
pub mod station_published_status;
pub mod station_redemption;
pub mod stations;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AppStatus {
    pub proxy_running: bool,
    pub local_base_url: String,
}

impl Default for AppStatus {
    fn default() -> Self {
        Self {
            proxy_running: false,
            local_base_url: "http://127.0.0.1:8787/v1".to_string(),
        }
    }
}
