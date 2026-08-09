//! Platform notification boundary for alerting delivery.
//!
//! The delivery ledger owns retries and idempotency.  This module owns only
//! the platform side effect and deliberately accepts a small, already
//! redacted payload.  It must never inspect an incident, station or secret
//! store directly.

use std::fmt;

use tauri::{AppHandle, Runtime};
use tauri_plugin_notification::{NotificationExt, PermissionState};

/// The three states exposed to settings and the delivery planner.  `Unknown`
/// is used when the plugin cannot be initialized or queried on a runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DesktopNotificationPermission {
    Allowed,
    Denied,
    Unavailable,
}

impl DesktopNotificationPermission {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Only non-sensitive, bounded fields cross the platform boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DesktopNotificationPayload {
    pub delivery_id: String,
    pub incident_id: String,
    pub episode_number: u32,
    pub delivery_kind: String,
    pub title: String,
    pub body: String,
    pub deep_link: String,
}

impl DesktopNotificationPayload {
    pub(crate) fn new(
        delivery_id: impl Into<String>,
        incident_id: impl Into<String>,
        episode_number: u32,
        delivery_kind: impl Into<String>,
        deep_link: impl Into<String>,
    ) -> Result<Self, DesktopNotificationError> {
        let delivery_id = bounded_text(delivery_id.into(), 128, "delivery_id")?;
        let incident_id = bounded_text(incident_id.into(), 128, "incident_id")?;
        let delivery_kind = bounded_text(delivery_kind.into(), 32, "delivery_kind")?;
        let deep_link = bounded_text(deep_link.into(), 512, "deep_link")?;
        if episode_number == 0 {
            return Err(DesktopNotificationError::InvalidPayload);
        }
        let body = format!("Alert {incident_id} requires attention (episode {episode_number}).");
        Ok(Self {
            delivery_id,
            incident_id,
            episode_number,
            title: "Relay Pool alert".to_string(),
            body,
            delivery_kind,
            deep_link,
        })
    }
}

fn bounded_text(
    value: String,
    max_bytes: usize,
    field: &'static str,
) -> Result<String, DesktopNotificationError> {
    if value.chars().any(char::is_control) {
        return Err(DesktopNotificationError::InvalidField(field));
    }
    let value = value.trim();
    if value.is_empty() || value.len() > max_bytes {
        return Err(DesktopNotificationError::InvalidField(field));
    }
    Ok(value.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DesktopNotificationError {
    PermissionDenied,
    Unavailable,
    Transient,
    InvalidPayload,
    InvalidField(&'static str),
}

impl DesktopNotificationError {
    pub(crate) fn error_code(&self) -> &'static str {
        match self {
            Self::PermissionDenied => "permission_denied",
            Self::Unavailable => "notification_unsupported",
            Self::Transient => "delivery_adapter_failed",
            Self::InvalidPayload | Self::InvalidField(_) => "invalid_notification_payload",
        }
    }

    pub(crate) fn is_retryable(&self) -> bool {
        matches!(self, Self::Transient)
    }
}

impl fmt::Display for DesktopNotificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.error_code())
    }
}

/// Injectable boundary used by the supervised delivery worker.
pub(crate) trait DesktopNotificationAdapter: Send + Sync {
    fn permission_state(&self) -> DesktopNotificationPermission;
    #[expect(
        dead_code,
        reason = "contract=alerting.desktop-permission-request; owner=services/alerting; remove_when=permission requests are handled outside the adapter"
    )]
    fn request_permission(&self) -> DesktopNotificationPermission;
    fn send(&self, payload: &DesktopNotificationPayload) -> Result<(), DesktopNotificationError>;
}

/// Official Tauri notification plugin adapter.  `show()` is intentionally
/// called outside a database transaction; the ledger lease handles the crash
/// boundary where the OS outcome cannot be confirmed.
#[derive(Clone)]
pub(crate) struct TauriDesktopNotificationAdapter<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> TauriDesktopNotificationAdapter<R> {
    pub(crate) fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }

    fn map_permission(
        state: Result<PermissionState, tauri_plugin_notification::Error>,
    ) -> DesktopNotificationPermission {
        match state {
            Ok(PermissionState::Granted) => DesktopNotificationPermission::Allowed,
            Ok(
                PermissionState::Denied
                | PermissionState::Prompt
                | PermissionState::PromptWithRationale,
            ) => DesktopNotificationPermission::Denied,
            Err(_) => DesktopNotificationPermission::Unavailable,
        }
    }
}

impl<R: Runtime> DesktopNotificationAdapter for TauriDesktopNotificationAdapter<R> {
    fn permission_state(&self) -> DesktopNotificationPermission {
        Self::map_permission(self.app.notification().permission_state())
    }

    fn request_permission(&self) -> DesktopNotificationPermission {
        Self::map_permission(self.app.notification().request_permission())
    }

    fn send(&self, payload: &DesktopNotificationPayload) -> Result<(), DesktopNotificationError> {
        match self.permission_state() {
            DesktopNotificationPermission::Allowed => {}
            DesktopNotificationPermission::Denied => {
                return Err(DesktopNotificationError::PermissionDenied)
            }
            DesktopNotificationPermission::Unavailable => {
                return Err(DesktopNotificationError::Unavailable)
            }
        }

        self.app
            .notification()
            .builder()
            .id(stable_notification_id(&payload.delivery_id))
            .title(payload.title.clone())
            .body(payload.body.clone())
            // The official desktop plugin does not expose a native click
            // callback, but preserving this extra keeps the payload contract
            // ready for supported platform action listeners and test fakes.
            .extra("deep_link", payload.deep_link.clone())
            .extra("delivery_id", payload.delivery_id.clone())
            .extra("incident_id", payload.incident_id.clone())
            .extra("episode_number", payload.episode_number)
            .extra("delivery_kind", payload.delivery_kind.clone())
            .auto_cancel()
            .show()
            .map_err(|_| DesktopNotificationError::Transient)
    }
}

fn stable_notification_id(delivery_id: &str) -> i32 {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(delivery_id.as_bytes());
    let value = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]);
    (value & 0x7fff_ffff) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_is_bounded_and_contains_only_deep_link_metadata() {
        let payload = DesktopNotificationPayload::new(
            "delivery-1",
            "incident-1",
            2,
            "opened",
            "relaypool://change-center?incident_id=incident-1&episode=2",
        )
        .unwrap();
        assert_eq!(payload.title, "Relay Pool alert");
        assert!(payload.body.contains("episode 2"));
        assert!(payload.deep_link.starts_with("relaypool://"));
    }

    #[test]
    fn payload_rejects_controls_and_unbounded_values() {
        assert_eq!(
            DesktopNotificationPayload::new("delivery\n", "incident", 1, "opened", "relaypool://x"),
            Err(DesktopNotificationError::InvalidField("delivery_id"))
        );
        assert_eq!(
            DesktopNotificationPayload::new("delivery", "incident", 0, "opened", "relaypool://x"),
            Err(DesktopNotificationError::InvalidPayload)
        );
    }

    #[test]
    fn error_codes_keep_platform_details_stable_and_redacted() {
        assert_eq!(
            DesktopNotificationError::PermissionDenied.error_code(),
            "permission_denied"
        );
        assert_eq!(
            DesktopNotificationError::Unavailable.error_code(),
            "notification_unsupported"
        );
        assert!(DesktopNotificationError::Transient.is_retryable());
        assert!(!DesktopNotificationError::PermissionDenied.is_retryable());
    }

    #[test]
    fn notification_id_is_stable_and_non_negative() {
        assert_eq!(
            stable_notification_id("delivery-1"),
            stable_notification_id("delivery-1")
        );
        assert!(stable_notification_id("delivery-1") >= 0);
    }
}
