pub use crate::models::proxy::{normalize_proxy_mode, normalize_proxy_url};
use crate::outbound::{ManualProxy, ProxyPolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyConfig {
    pub mode: String,
    pub url: Option<String>,
}

impl ProxyConfig {
    #[cfg(test)]
    pub fn direct() -> Self {
        Self {
            mode: "direct".to_string(),
            url: None,
        }
    }
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

/// Converts a concrete network proxy setting into the outbound transport policy.
/// The global setting is always concrete; monitor/station overrides should
/// resolve inheritance before calling this helper.
pub(crate) fn proxy_policy_from_mode(mode: &str, url: Option<&str>) -> Result<ProxyPolicy, String> {
    match normalize_proxy_mode(mode, false).as_str() {
        "direct" => Ok(ProxyPolicy::Direct),
        "system" => Ok(ProxyPolicy::System),
        "manual" => {
            let endpoint = normalize_proxy_url(url.map(str::to_owned))
                .ok_or_else(|| "manual proxy address is required".to_string())?;
            ManualProxy::parse(endpoint)
                .map(ProxyPolicy::Manual)
                .map_err(|_| "invalid manual proxy address".to_string())
        }
        _ => Ok(ProxyPolicy::Direct),
    }
}

/// Resolves the three-level proxy precedence used by local routing:
/// station override -> local routing override -> global network setting.
/// Each layer may use `inherit`, while the global setting is always concrete.
pub fn resolve_routing_proxy_config(
    station_mode: &str,
    station_url: Option<String>,
    local_mode: &str,
    local_url: Option<String>,
    global_mode: &str,
    global_url: Option<String>,
) -> ProxyConfig {
    let local_mode = normalize_proxy_mode(local_mode, true);
    let local = if local_mode == "inherit" {
        ProxyConfig {
            mode: normalize_proxy_mode(global_mode, false),
            url: normalize_proxy_url(global_url),
        }
    } else {
        ProxyConfig {
            mode: local_mode,
            url: normalize_proxy_url(local_url),
        }
    };
    resolve_proxy_config(station_mode, station_url, &local.mode, local.url)
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
    fn routing_proxy_inheritance_resolves_global_for_station_and_local_defaults() {
        let proxy = resolve_routing_proxy_config(
            "inherit",
            None,
            "inherit",
            None,
            "manual",
            Some("http://127.0.0.1:7890".to_string()),
        );

        assert_eq!(proxy.mode, "manual");
        assert_eq!(proxy.url.as_deref(), Some("http://127.0.0.1:7890"));
    }

    #[test]
    fn routing_proxy_explicit_direct_overrides_inherited_global_proxy() {
        let proxy = resolve_routing_proxy_config(
            "inherit",
            None,
            "direct",
            None,
            "manual",
            Some("http://127.0.0.1:7890".to_string()),
        );

        assert_eq!(proxy, ProxyConfig::direct());
    }

    #[test]
    fn concrete_proxy_modes_convert_to_outbound_policies() {
        assert_eq!(
            proxy_policy_from_mode("direct", None).expect("direct proxy policy"),
            ProxyPolicy::Direct
        );
        assert_eq!(
            proxy_policy_from_mode("system", None).expect("system proxy policy"),
            ProxyPolicy::System
        );
        assert_eq!(
            proxy_policy_from_mode("manual", Some("http://127.0.0.1:7890"))
                .expect("manual proxy policy")
                .pool_key(),
            ProxyPolicy::Manual(ManualProxy::parse("http://127.0.0.1:7890").expect("manual proxy"))
                .pool_key()
        );
    }

    #[test]
    fn manual_proxy_policy_requires_a_valid_endpoint() {
        assert!(proxy_policy_from_mode("manual", None).is_err());
        assert!(proxy_policy_from_mode("manual", Some("http://user:pass@127.0.0.1:7890")).is_err());
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
