pub mod capture;
pub mod collector;
pub mod credentials;
pub mod proxy;
pub mod routing;
pub mod settings;
pub mod station_keys;
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
