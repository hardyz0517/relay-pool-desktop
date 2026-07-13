#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyConfig {
    pub mode: String,
    pub url: Option<String>,
}

impl ProxyConfig {
    pub fn direct() -> Self {
        Self {
            mode: "direct".to_string(),
            url: None,
        }
    }
}

pub fn normalize_proxy_mode(value: &str, allow_inherit: bool) -> String {
    match value.trim() {
        "direct" => "direct".to_string(),
        "system" => "system".to_string(),
        "manual" => "manual".to_string(),
        "inherit" if allow_inherit => "inherit".to_string(),
        _ if allow_inherit => "inherit".to_string(),
        _ => "direct".to_string(),
    }
}

pub fn normalize_proxy_url(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

pub fn resolve_proxy_config(
    station_mode: &str,
    station_url: Option<String>,
    global_mode: &str,
    global_url: Option<String>,
) -> ProxyConfig {
    let station_mode = normalize_proxy_mode(station_mode, true);
    if station_mode != "inherit" {
        return ProxyConfig {
            mode: station_mode,
            url: normalize_proxy_url(station_url),
        };
    }
    ProxyConfig {
        mode: normalize_proxy_mode(global_mode, false),
        url: normalize_proxy_url(global_url),
    }
}

pub fn agent_builder_for_proxy(proxy: &ProxyConfig) -> Result<ureq::AgentBuilder, String> {
    let builder = ureq::AgentBuilder::new();
    match proxy.mode.as_str() {
        "direct" => Ok(builder.try_proxy_from_env(false)),
        "system" => match current_system_proxy_url() {
            Some(url) => {
                let proxy = ureq::Proxy::new(&url).map_err(|error| {
                    format!(
                        "系统采集代理地址无效: {}",
                        crate::services::secrets::mask::redact_text(&error.to_string())
                    )
                })?;
                Ok(builder.proxy(proxy))
            }
            None => Ok(builder.try_proxy_from_env(true)),
        },
        "manual" => {
            let Some(url) = proxy
                .url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                return Err("手动采集代理地址不能为空".to_string());
            };
            let proxy = ureq::Proxy::new(url).map_err(|error| {
                format!(
                    "采集代理地址无效: {}",
                    crate::services::secrets::mask::redact_text(&error.to_string())
                )
            })?;
            Ok(builder.proxy(proxy))
        }
        _ => Ok(builder.try_proxy_from_env(false)),
    }
}

pub(crate) fn current_system_proxy_url() -> Option<String> {
    current_windows_system_proxy_url()
}

#[cfg(windows)]
fn current_windows_system_proxy_url() -> Option<String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    let internet_settings = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    let proxy_enabled: u32 = internet_settings.get_value("ProxyEnable").unwrap_or(0);
    if proxy_enabled == 0 {
        return None;
    }
    let proxy_server: String = internet_settings.get_value("ProxyServer").ok()?;
    proxy_url_from_windows_proxy_server(&proxy_server)
}

#[cfg(not(windows))]
fn current_windows_system_proxy_url() -> Option<String> {
    None
}

fn proxy_url_from_windows_proxy_server(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('=') {
        let mut http_candidate = None;
        for item in trimmed.split(';') {
            let Some((scheme, address)) = item.split_once('=') else {
                continue;
            };
            let normalized = normalize_proxy_address(address)?;
            match scheme.trim().to_ascii_lowercase().as_str() {
                "https" => return Some(normalized),
                "http" => http_candidate = Some(normalized),
                _ => {}
            }
        }
        return http_candidate;
    }
    normalize_proxy_address(trimmed)
}

fn normalize_proxy_address(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_matches('"').trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains("://") {
        Some(trimmed.to_string())
    } else {
        Some(format!("http://{trimmed}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_inherit_uses_global_proxy_config() {
        let proxy = resolve_proxy_config(
            "inherit",
            None,
            "manual",
            Some("http://127.0.0.1:7890".to_string()),
        );

        assert_eq!(proxy.mode, "manual");
        assert_eq!(proxy.url.as_deref(), Some("http://127.0.0.1:7890"));
    }

    #[test]
    fn station_direct_overrides_global_manual_proxy() {
        let proxy = resolve_proxy_config(
            "direct",
            None,
            "manual",
            Some("http://127.0.0.1:7890".to_string()),
        );

        assert_eq!(proxy, ProxyConfig::direct());
    }

    #[test]
    fn parses_windows_system_proxy_server_default_port() {
        assert_eq!(
            proxy_url_from_windows_proxy_server("127.0.0.1:7890"),
            Some("http://127.0.0.1:7890".to_string())
        );
    }

    #[test]
    fn parses_windows_system_proxy_server_https_mapping_first() {
        assert_eq!(
            proxy_url_from_windows_proxy_server("http=127.0.0.1:8080;https=127.0.0.1:7890"),
            Some("http://127.0.0.1:7890".to_string())
        );
    }
}
