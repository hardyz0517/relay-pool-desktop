use std::{collections::HashMap, ops::Not};

use http::{header::AUTHORIZATION, HeaderMap};
use subtle::ConstantTimeEq;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthDecision {
    Missing,
    Invalid,
    Accepted,
}

impl AuthDecision {
    pub fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted)
    }
}

impl Not for AuthDecision {
    type Output = bool;

    fn not(self) -> Self::Output {
        !self.is_accepted()
    }
}

pub trait AuthorizationHeaders {
    fn authorization_value(&self) -> Option<&str>;
}

impl AuthorizationHeaders for HeaderMap {
    fn authorization_value(&self) -> Option<&str> {
        self.get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
    }
}

impl AuthorizationHeaders for HashMap<String, String> {
    fn authorization_value(&self) -> Option<&str> {
        self.get("authorization").map(String::as_str)
    }
}

pub fn authorize_headers(
    headers: &impl AuthorizationHeaders,
    configured_key: &str,
) -> AuthDecision {
    let Some(value) = headers.authorization_value() else {
        return AuthDecision::Missing;
    };
    let Some(token) = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
    else {
        return AuthDecision::Invalid;
    };
    let left = token.trim().as_bytes();
    let right = configured_key.trim().as_bytes();
    if left.len() == right.len() && bool::from(left.ct_eq(right)) {
        AuthDecision::Accepted
    } else {
        AuthDecision::Invalid
    }
}

pub fn allowed_origin(origin: &str) -> Option<&str> {
    let origin = origin.trim();
    let loopback = origin == "http://localhost"
        || origin.starts_with("http://localhost:")
        || origin == "http://127.0.0.1"
        || origin.starts_with("http://127.0.0.1:");
    loopback.then_some(origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_auth_requires_exact_configured_key() {
        let headers = HashMap::from([(
            "authorization".to_string(),
            "Bearer relay-local-secret".to_string(),
        )]);
        assert_eq!(
            authorize_headers(&headers, "relay-local-secret"),
            AuthDecision::Accepted
        );
        assert_eq!(
            authorize_headers(&headers, "relay-local-other"),
            AuthDecision::Invalid
        );
        assert_eq!(
            authorize_headers(&HashMap::new(), "relay-local-secret"),
            AuthDecision::Missing
        );
    }

    #[test]
    fn bearer_auth_accepts_http_header_map() {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, "Bearer relay-local-secret".parse().unwrap());
        assert_eq!(
            authorize_headers(&headers, "relay-local-secret"),
            AuthDecision::Accepted
        );
    }

    #[test]
    fn cors_allows_loopback_origins_only() {
        assert_eq!(
            allowed_origin("http://127.0.0.1:3000"),
            Some("http://127.0.0.1:3000")
        );
        assert_eq!(
            allowed_origin("http://localhost:5173"),
            Some("http://localhost:5173")
        );
        assert_eq!(allowed_origin("https://attacker.example"), None);
    }
}
