use crate::observability::runtime::{
    descriptor::{standard_descriptor, EventDescriptor},
    event::{Component, EventLevel},
};
use crate::services::remote_keys::{RemoteKeyExternalFailureReason, RemoteKeyOperationError};

pub(crate) const EVENT_DESCRIPTORS: &[EventDescriptor] = &[
    standard_descriptor(
        "frontend.shell",
        "frontend.boundary.failed",
        Component::Frontend,
        EventLevel::Error,
    ),
    standard_descriptor(
        "key_pool.remote_keys",
        "collector.remote_key_scan.completed",
        Component::Collector,
        EventLevel::Info,
    ),
    standard_descriptor(
        "key_pool.remote_keys",
        "collector.remote_key_scan.failed.credentials_unavailable",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "key_pool.remote_keys",
        "collector.remote_key_scan.failed.authentication_rejected",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "key_pool.remote_keys",
        "collector.remote_key_scan.failed.rate_limited",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "key_pool.remote_keys",
        "collector.remote_key_scan.failed.timed_out",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "key_pool.remote_keys",
        "collector.remote_key_scan.failed.budget_exhausted",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "key_pool.remote_keys",
        "collector.remote_key_scan.failed.cancelled",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "key_pool.remote_keys",
        "collector.remote_key_scan.failed.transport",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "key_pool.remote_keys",
        "collector.remote_key_scan.failed.malformed_payload",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "key_pool.remote_keys",
        "collector.remote_key_scan.failed.provider_unavailable",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "key_pool.remote_keys",
        "collector.remote_key_scan.failed.other",
        Component::Collector,
        EventLevel::Error,
    ),
    standard_descriptor(
        "key_pool.remote_keys",
        "collector.remote_key_scan.failed.browser_context_required",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "station.capture",
        "collector.web_session.candidate_detected",
        Component::Collector,
        EventLevel::Info,
    ),
    standard_descriptor(
        "station.capture",
        "collector.web_session.browser_state_read_succeeded",
        Component::Collector,
        EventLevel::Info,
    ),
    standard_descriptor(
        "station.capture",
        "collector.web_session.browser_state_read_failed",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "station.capture",
        "collector.web_session.verification_succeeded",
        Component::Collector,
        EventLevel::Info,
    ),
    standard_descriptor(
        "station.capture",
        "collector.web_session.verification_failed",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "station.capture",
        "collector.web_session.persistence_succeeded",
        Component::Collector,
        EventLevel::Info,
    ),
    standard_descriptor(
        "station.capture",
        "collector.web_session.persistence_failed",
        Component::Collector,
        EventLevel::Error,
    ),
    standard_descriptor(
        "station.capture",
        "collector.web_session.completed",
        Component::Collector,
        EventLevel::Info,
    ),
];

pub(crate) fn frontend_boundary_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[0]
}

pub(crate) fn remote_key_scan_completed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[1]
}

pub(crate) fn remote_key_scan_failed(error: &RemoteKeyOperationError) -> &'static EventDescriptor {
    let reason = match error {
        RemoteKeyOperationError::ExternalUnavailable(reason)
        | RemoteKeyOperationError::ExternalUnavailableWithDetail { reason, .. } => Some(*reason),
        _ => None,
    };
    match reason {
        Some(RemoteKeyExternalFailureReason::CredentialsUnavailable) => &EVENT_DESCRIPTORS[2],
        Some(RemoteKeyExternalFailureReason::AuthenticationRejected) => &EVENT_DESCRIPTORS[3],
        Some(RemoteKeyExternalFailureReason::BrowserContextRequired) => &EVENT_DESCRIPTORS[12],
        Some(RemoteKeyExternalFailureReason::RateLimited) => &EVENT_DESCRIPTORS[4],
        Some(RemoteKeyExternalFailureReason::TimedOut) => &EVENT_DESCRIPTORS[5],
        Some(RemoteKeyExternalFailureReason::BudgetExhausted) => &EVENT_DESCRIPTORS[6],
        Some(RemoteKeyExternalFailureReason::Cancelled) => &EVENT_DESCRIPTORS[7],
        Some(RemoteKeyExternalFailureReason::Transport) => &EVENT_DESCRIPTORS[8],
        Some(RemoteKeyExternalFailureReason::MalformedPayload) => &EVENT_DESCRIPTORS[9],
        Some(RemoteKeyExternalFailureReason::ProviderUnavailable) => &EVENT_DESCRIPTORS[10],
        None => &EVENT_DESCRIPTORS[11],
    }
}

pub(crate) fn web_authorization_candidate_detected() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[13]
}

pub(crate) fn web_authorization_cookie_read_succeeded() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[14]
}

pub(crate) fn web_authorization_cookie_read_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[15]
}

pub(crate) fn web_authorization_verification_succeeded() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[16]
}

pub(crate) fn web_authorization_verification_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[17]
}

pub(crate) fn web_authorization_persistence_succeeded() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[18]
}

pub(crate) fn web_authorization_persistence_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[19]
}

pub(crate) fn web_authorization_completed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[20]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_key_scan_failure_uses_a_stable_safe_reason_code() {
        let descriptor = remote_key_scan_failed(&RemoteKeyOperationError::ExternalUnavailable(
            RemoteKeyExternalFailureReason::TimedOut,
        ));

        assert_eq!(
            descriptor.code,
            "collector.remote_key_scan.failed.timed_out"
        );
    }

    #[test]
    fn web_authorization_events_use_stage_only_safe_codes() {
        assert_eq!(
            web_authorization_cookie_read_failed().code,
            "collector.web_session.browser_state_read_failed"
        );
        assert_eq!(
            web_authorization_persistence_succeeded().code,
            "collector.web_session.persistence_succeeded"
        );
    }
}
