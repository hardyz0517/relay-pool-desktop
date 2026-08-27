use std::{
    collections::HashSet,
    fmt,
    time::{Duration, Instant},
};

use http::{HeaderMap, HeaderName, HeaderValue};
use zeroize::Zeroize;

use crate::outbound::error::{OutboundFailure, OutboundFailureKind};

#[derive(Clone, Copy, Debug)]
pub struct RequestBudget {
    deadline: Instant,
}

impl RequestBudget {
    pub fn from_now(duration: Duration) -> Self {
        Self {
            deadline: Instant::now() + duration,
        }
    }

    pub fn from_deadline(deadline: Instant) -> Self {
        Self { deadline }
    }

    pub fn remaining(&self) -> Option<Duration> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            None
        } else {
            Some(remaining)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeoutPolicy {
    pub connect_timeout: Duration,
    pub first_byte_timeout: Duration,
    pub body_read_timeout: Duration,
    pub total_timeout: Duration,
}

impl TimeoutPolicy {
    pub fn provider_default() -> Self {
        Self {
            connect_timeout: Duration::from_millis(5_000),
            first_byte_timeout: Duration::from_millis(10_000),
            body_read_timeout: Duration::from_millis(10_000),
            total_timeout: Duration::from_millis(30_000),
        }
    }
}

#[derive(Clone, Debug)]
pub struct OutboundHeaderPolicy {
    public_headers: HashSet<HeaderName>,
    sensitive_headers: HashSet<HeaderName>,
}

impl OutboundHeaderPolicy {
    pub fn provider_default() -> Self {
        Self::new(
            [
                "accept",
                "accept-language",
                "if-none-match",
                "content-type",
                "user-agent",
                "x-request-id",
                "idempotency-key",
                "openai-organization",
                "openai-project",
                "openai-beta",
                "anthropic-beta",
                "anthropic-version",
                "new-api-user",
                "x-user-ui-request",
                "x-app",
                "x-claude-code-session-id",
                "x-client-request-id",
                "x-goog-api-client",
            ],
            [
                "authorization",
                "cookie",
                "proxy-authorization",
                "x-goog-api-key",
            ],
        )
    }

    pub fn new<const P: usize, const S: usize>(
        public_headers: [&'static str; P],
        sensitive_headers: [&'static str; S],
    ) -> Self {
        Self {
            public_headers: public_headers
                .into_iter()
                .map(HeaderName::from_static)
                .collect(),
            sensitive_headers: sensitive_headers
                .into_iter()
                .map(HeaderName::from_static)
                .collect(),
        }
    }

    pub fn allows_public(&self, name: &HeaderName) -> bool {
        self.public_headers.contains(name)
    }

    pub fn allows_sensitive(&self, name: &HeaderName) -> bool {
        self.sensitive_headers.contains(name)
    }
}

pub struct OutboundHeaders {
    public: HeaderMap,
    sensitive: Vec<(HeaderName, SecretHeaderValue)>,
}

impl OutboundHeaders {
    pub fn new() -> Self {
        Self {
            public: HeaderMap::new(),
            sensitive: Vec::new(),
        }
    }

    pub fn insert_public(
        &mut self,
        name: HeaderName,
        value: HeaderValue,
        policy: &OutboundHeaderPolicy,
    ) -> Result<(), OutboundFailure> {
        if !policy.allows_public(&name) {
            return Err(OutboundFailure::new(OutboundFailureKind::HeaderNotAllowed(
                name.to_string(),
            )));
        }
        self.public.insert(name, value);
        Ok(())
    }

    pub fn insert_sensitive(
        &mut self,
        name: HeaderName,
        value: SecretHeaderValue,
        policy: &OutboundHeaderPolicy,
    ) -> Result<(), OutboundFailure> {
        if !policy.allows_sensitive(&name) {
            return Err(OutboundFailure::new(OutboundFailureKind::HeaderNotAllowed(
                name.to_string(),
            )));
        }
        self.sensitive.push((name, value));
        Ok(())
    }

    pub(crate) fn materialize(
        &self,
        policy: &OutboundHeaderPolicy,
    ) -> Result<HeaderMap, OutboundFailure> {
        self.materialize_for_redirect(policy, true)
    }

    pub(crate) fn materialize_for_redirect(
        &self,
        policy: &OutboundHeaderPolicy,
        preserve_sensitive: bool,
    ) -> Result<HeaderMap, OutboundFailure> {
        let mut headers = HeaderMap::new();
        for (name, value) in self.public.iter() {
            if !policy.allows_public(name) {
                return Err(OutboundFailure::new(OutboundFailureKind::HeaderNotAllowed(
                    name.to_string(),
                )));
            }
            headers.insert(name, value.clone());
        }
        if preserve_sensitive {
            for (name, value) in &self.sensitive {
                if !policy.allows_sensitive(name) {
                    return Err(OutboundFailure::new(OutboundFailureKind::HeaderNotAllowed(
                        name.to_string(),
                    )));
                }
                let header_value = HeaderValue::from_str(value.expose())
                    .map_err(|_| OutboundFailure::new(OutboundFailureKind::InvalidHeader))?;
                headers.insert(name, header_value);
            }
        }
        Ok(headers)
    }

    pub fn redaction(&self) -> HeaderRedaction {
        let public = self.public.keys().map(HeaderName::to_string).collect();
        let sensitive = self
            .sensitive
            .iter()
            .map(|(name, _)| format!("{name}: <redacted>"))
            .collect();
        HeaderRedaction { public, sensitive }
    }
}

impl Default for OutboundHeaders {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for OutboundHeaders {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OutboundHeaders")
            .field("public", &self.public.keys().collect::<Vec<_>>())
            .field(
                "sensitive",
                &self
                    .sensitive
                    .iter()
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderRedaction {
    pub public: Vec<String>,
    pub sensitive: Vec<String>,
}

#[derive(PartialEq, Eq)]
pub struct SecretHeaderValue {
    value: String,
}

impl SecretHeaderValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    pub(crate) fn expose(&self) -> &str {
        &self.value
    }
}

impl Clone for SecretHeaderValue {
    fn clone(&self) -> Self {
        Self::new(self.value.clone())
    }
}

impl fmt::Debug for SecretHeaderValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

impl Drop for SecretHeaderValue {
    fn drop(&mut self) {
        self.value.zeroize();
    }
}
