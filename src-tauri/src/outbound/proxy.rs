use std::fmt;

use url::Url;

use crate::outbound::error::{OutboundFailure, OutboundFailureKind};
use crate::outbound::policy::SecretHeaderValue;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProxyPolicy {
    Direct,
    System,
    Manual(ManualProxy),
}

impl ProxyPolicy {
    pub fn pool_key(&self) -> TransportPoolKey {
        match self {
            Self::Direct => TransportPoolKey::Direct,
            Self::System => TransportPoolKey::System,
            Self::Manual(proxy) => TransportPoolKey::Manual {
                scheme: proxy.scheme,
                endpoint: proxy.endpoint.clone(),
            },
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ManualProxy {
    pub scheme: ProxyScheme,
    pub endpoint: String,
    pub credentials: Option<ProxyCredentials>,
}

impl ManualProxy {
    pub fn parse(endpoint: impl Into<String>) -> Result<Self, OutboundFailure> {
        Self::parse_with_credentials(endpoint, None, None)
    }

    pub fn parse_with_credentials(
        endpoint: impl Into<String>,
        username: Option<String>,
        password: Option<SecretHeaderValue>,
    ) -> Result<Self, OutboundFailure> {
        let endpoint = endpoint.into();
        if contains_control(&endpoint) {
            return Err(OutboundFailure::new(OutboundFailureKind::ProxyPolicy));
        }
        let url = Url::parse(&endpoint)
            .map_err(|_| OutboundFailure::new(OutboundFailureKind::ProxyPolicy))?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err(OutboundFailure::new(OutboundFailureKind::ProxyPolicy));
        }
        let scheme = ProxyScheme::from_url_scheme(url.scheme())?;
        let credentials = match (username, password) {
            (Some(username), Some(password)) => Some(ProxyCredentials { username, password }),
            (None, None) => None,
            _ => return Err(OutboundFailure::new(OutboundFailureKind::ProxyPolicy)),
        };
        Ok(Self {
            scheme,
            endpoint,
            credentials,
        })
    }
}

impl fmt::Debug for ManualProxy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManualProxy")
            .field("scheme", &self.scheme)
            .field("endpoint", &self.endpoint)
            .field(
                "credentials",
                &self.credentials.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProxyCredentials {
    pub username: String,
    pub password: SecretHeaderValue,
}

impl fmt::Debug for ProxyCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProxyCredentials")
            .field("username", &"<redacted>")
            .field("password", &self.password)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum ProxyScheme {
    Http,
    Https,
    Socks5,
    Socks5h,
}

impl ProxyScheme {
    fn from_url_scheme(scheme: &str) -> Result<Self, OutboundFailure> {
        match scheme.to_ascii_lowercase().as_str() {
            "http" => Ok(Self::Http),
            "https" => Ok(Self::Https),
            "socks5" => Ok(Self::Socks5),
            "socks5h" => Ok(Self::Socks5h),
            _ => Err(OutboundFailure::new(OutboundFailureKind::ProxyPolicy)),
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum TransportPoolKey {
    Direct,
    System,
    Manual {
        scheme: ProxyScheme,
        endpoint: String,
    },
}

fn contains_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}
