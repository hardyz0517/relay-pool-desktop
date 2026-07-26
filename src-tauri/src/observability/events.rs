#![allow(
    dead_code,
    reason = "Task 18L freezes structured event fields before production recorders are wired to all call sites"
)]

use sha2::{Digest, Sha256};

const MAX_STABLE_CODE_BYTES: usize = 64;
const REDACTED_RESOURCE_HASH_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StructuredEvent {
    pub(crate) code: StableEventCode,
    pub(crate) kind: StructuredEventKind,
    pub(crate) duration_ms: u64,
    pub(crate) result: StructuredEventResult,
    pub(crate) resource_id: Option<RedactedResourceId>,
}

impl StructuredEvent {
    pub(crate) fn new(
        code: impl AsRef<str>,
        kind: StructuredEventKind,
        duration_ms: u64,
        result: StructuredEventResult,
        resource: Option<(&str, &str)>,
    ) -> Result<Self, StructuredEventError> {
        Ok(Self {
            code: StableEventCode::new(code.as_ref())?,
            kind,
            duration_ms,
            result,
            resource_id: resource
                .map(|(scope, raw)| RedactedResourceId::from_raw(scope, raw))
                .transpose()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructuredEventKind {
    BlockingJob,
    IpcCommand,
    Operation,
    OutboundRequest,
    TaskRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructuredEventResult {
    Cancelled,
    Error,
    Ok,
    Overloaded,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StableEventCode(String);

impl StableEventCode {
    pub(crate) fn new(value: &str) -> Result<Self, StructuredEventError> {
        if !is_stable_token(value) {
            return Err(StructuredEventError::InvalidStableCode);
        }
        Ok(Self(value.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RedactedResourceId(String);

impl RedactedResourceId {
    pub(crate) fn from_raw(scope: &str, raw: &str) -> Result<Self, StructuredEventError> {
        if !is_stable_token(scope) {
            return Err(StructuredEventError::InvalidResourceScope);
        }
        let digest = Sha256::digest([scope.as_bytes(), b"\0", raw.as_bytes()].concat());
        let hash = format!("{digest:x}");
        Ok(Self(format!(
            "res_{}",
            &hash[..REDACTED_RESOURCE_HASH_BYTES]
        )))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructuredEventError {
    InvalidResourceScope,
    InvalidStableCode,
}

fn is_stable_token(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_STABLE_CODE_BYTES {
        return false;
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    !value.contains("://")
        && !value.contains('?')
        && !value.contains('=')
        && !value.contains('\\')
        && !value.contains('/')
        && !lower.contains("authorization")
        && !lower.contains("bearer")
        && !lower.contains("cookie")
        && !lower.contains("password")
        && !lower.contains("sk-")
        && !lower.contains("token")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_event_accepts_only_stable_public_fields() {
        let event = StructuredEvent::new(
            "proxy.request.timeout",
            StructuredEventKind::OutboundRequest,
            1500,
            StructuredEventResult::Timeout,
            Some((
                "station",
                "https://user:pass@example.test/v1/chat?authorization=Bearer%20sk-secret",
            )),
        )
        .expect("stable event");

        assert_eq!(event.code.as_str(), "proxy.request.timeout");
        assert_eq!(event.duration_ms, 1500);
        assert_eq!(event.result, StructuredEventResult::Timeout);
        let resource = event.resource_id.expect("redacted resource");
        assert!(resource.as_str().starts_with("res_"));
        assert_eq!(
            resource.as_str().len(),
            "res_".len() + REDACTED_RESOURCE_HASH_BYTES
        );
        assert!(!resource.as_str().contains("example.test"));
        assert!(!resource.as_str().contains("sk-secret"));
    }

    #[test]
    fn structured_event_rejects_secret_url_query_and_path_shaped_codes() {
        for code in [
            "https://example.test/v1?token=secret",
            "command.authorization",
            "provider/sk-secret",
            "C:\\Users\\cpp_s\\relay-pool.db",
            "prompt=response",
            "MixedCase",
        ] {
            assert_eq!(
                StructuredEvent::new(
                    code,
                    StructuredEventKind::IpcCommand,
                    1,
                    StructuredEventResult::Error,
                    None,
                ),
                Err(StructuredEventError::InvalidStableCode)
            );
        }
    }

    #[test]
    fn structured_event_debug_never_contains_raw_resource_material() {
        let raw = "Authorization: Bearer sk-secret cookie=session C:\\Users\\cpp_s\\relay-pool.db";
        let event = StructuredEvent::new(
            "command.error",
            StructuredEventKind::IpcCommand,
            10,
            StructuredEventResult::Error,
            Some(("command", raw)),
        )
        .expect("stable event");
        let debug = format!("{event:?}");

        for forbidden in [
            "Authorization",
            "Bearer",
            "cookie=session",
            "relay-pool.db",
            "sk-secret",
        ] {
            assert!(!debug.contains(forbidden), "{forbidden}");
        }
    }
}
