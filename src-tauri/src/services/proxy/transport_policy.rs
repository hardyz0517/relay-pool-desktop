use std::{
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use crate::models::routing_policy::TimeoutPolicyV2;

/// Immutable request transport facts compiled from one persisted routing
/// policy revision. A request owns an Arc of this value for its entire
/// lifecycle; publishing a newer value cannot change an in-flight request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransportPolicySnapshot {
    pub(crate) source_routing_policy_revision: u64,
    pub(crate) version: u16,
    pub(crate) connect_timeout: Duration,
    pub(crate) first_byte_timeout: Duration,
    pub(crate) buffered_execution_timeout: Duration,
    pub(crate) stream_idle_timeout: Duration,
    pub(crate) request_deadline: Duration,
    pub(crate) upstream_pool_idle_timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransportPolicyValidationError {
    UnsupportedVersion,
    ZeroTimeout(&'static str),
    InvalidRevision,
}

impl TransportPolicySnapshot {
    pub(crate) const VERSION: u16 = 2;

    pub(crate) fn from_timeout_policy(
        policy: &TimeoutPolicyV2,
        source_routing_policy_revision: u64,
        upstream_pool_idle_timeout: Duration,
    ) -> Result<Self, TransportPolicyValidationError> {
        let snapshot = Self {
            source_routing_policy_revision,
            version: Self::VERSION,
            connect_timeout: Duration::from_millis(policy.connect_millis()),
            first_byte_timeout: Duration::from_millis(policy.first_byte_millis()),
            buffered_execution_timeout: Duration::from_millis(policy.buffered_execution_millis()),
            stream_idle_timeout: Duration::from_millis(policy.stream_idle_millis()),
            request_deadline: Duration::from_millis(policy.precommit_millis()),
            upstream_pool_idle_timeout,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    #[cfg(test)]
    pub(crate) fn from_limits(
        limits: &crate::services::proxy::limits::ProxyStartupResourceLimits,
    ) -> Result<Self, TransportPolicyValidationError> {
        let mut snapshot = Self::default();
        snapshot.upstream_pool_idle_timeout = limits.upstream_pool_idle_timeout;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub(crate) fn validate(&self) -> Result<(), TransportPolicyValidationError> {
        if self.version != Self::VERSION {
            return Err(TransportPolicyValidationError::UnsupportedVersion);
        }
        if self.source_routing_policy_revision == 0 {
            return Err(TransportPolicyValidationError::InvalidRevision);
        }
        for (name, value) in [
            ("connect_timeout", self.connect_timeout),
            ("first_byte_timeout", self.first_byte_timeout),
            (
                "buffered_execution_timeout",
                self.buffered_execution_timeout,
            ),
            ("stream_idle_timeout", self.stream_idle_timeout),
            ("request_deadline", self.request_deadline),
            (
                "upstream_pool_idle_timeout",
                self.upstream_pool_idle_timeout,
            ),
        ] {
            if value.is_zero() {
                return Err(TransportPolicyValidationError::ZeroTimeout(name));
            }
        }
        Ok(())
    }

    pub(crate) fn remaining_request_deadline(&self, started: Instant) -> Option<Duration> {
        self.request_deadline.checked_sub(started.elapsed())
    }

    /// Values that affect reqwest client construction. Stream idle is a
    /// response-body deadline and intentionally does not rotate the pool.
    pub(crate) fn client_config_fingerprint(&self) -> UpstreamClientConfigFingerprint {
        UpstreamClientConfigFingerprint {
            connect_timeout: self.connect_timeout,
            pool_idle_timeout: self.upstream_pool_idle_timeout,
        }
    }
}

impl Default for TransportPolicySnapshot {
    fn default() -> Self {
        Self {
            source_routing_policy_revision: 1,
            version: Self::VERSION,
            connect_timeout: Duration::from_secs(10),
            first_byte_timeout: Duration::from_secs(30),
            buffered_execution_timeout: Duration::from_secs(300),
            stream_idle_timeout: Duration::from_secs(90),
            request_deadline: Duration::from_secs(60),
            upstream_pool_idle_timeout: Duration::from_secs(90),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct UpstreamClientConfigFingerprint {
    pub(crate) connect_timeout: Duration,
    pub(crate) pool_idle_timeout: Duration,
}

/// Process-local current transport policy. The read lock is held only while
/// cloning an Arc, never across request execution or network I/O.
#[derive(Clone)]
pub(crate) struct TransportPolicyStore {
    current: Arc<RwLock<Arc<TransportPolicySnapshot>>>,
}

impl TransportPolicyStore {
    pub(crate) fn new(
        initial: TransportPolicySnapshot,
    ) -> Result<Self, TransportPolicyValidationError> {
        initial.validate()?;
        Ok(Self {
            current: Arc::new(RwLock::new(Arc::new(initial))),
        })
    }

    pub(crate) fn load(&self) -> Arc<TransportPolicySnapshot> {
        self.current
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub(crate) fn publish_if_newer(
        &self,
        snapshot: TransportPolicySnapshot,
    ) -> Result<bool, TransportPolicyValidationError> {
        snapshot.validate()?;
        let mut current = self
            .current
            .write()
            .unwrap_or_else(|error| error.into_inner());
        if snapshot.source_routing_policy_revision <= current.source_routing_policy_revision {
            return Ok(false);
        }
        *current = Arc::new(snapshot);
        Ok(true)
    }

    /// Replace the active snapshot during a stopped-to-starting lifecycle
    /// transition. The lifecycle mutex guarantees no request can observe a
    /// partially started generation.
    pub(crate) fn install(
        &self,
        snapshot: TransportPolicySnapshot,
    ) -> Result<(), TransportPolicyValidationError> {
        snapshot.validate()?;
        let mut current = self
            .current
            .write()
            .unwrap_or_else(|error| error.into_inner());
        *current = Arc::new(snapshot);
        Ok(())
    }
}

impl Default for TransportPolicyStore {
    fn default() -> Self {
        Self::new(TransportPolicySnapshot::default()).expect("default transport policy is valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_timeout_policy_without_startup_resource_limits() {
        let snapshot = TransportPolicySnapshot::from_timeout_policy(
            &TimeoutPolicyV2::default(),
            7,
            Duration::from_secs(90),
        )
        .expect("valid policy");
        assert_eq!(snapshot.source_routing_policy_revision, 7);
        assert_eq!(snapshot.connect_timeout, Duration::from_secs(10));
        assert_eq!(snapshot.first_byte_timeout, Duration::from_secs(30));
        assert_eq!(snapshot.request_deadline, Duration::from_secs(60));
        assert_eq!(
            snapshot.client_config_fingerprint().pool_idle_timeout,
            Duration::from_secs(90)
        );
    }

    #[test]
    fn store_rejects_stale_revisions_and_keeps_newest_snapshot() {
        let store = TransportPolicyStore::default();
        let mut newer = (*store.load()).clone();
        newer.source_routing_policy_revision = 2;
        newer.connect_timeout = Duration::from_secs(3);
        assert!(store.publish_if_newer(newer).expect("publish"));

        let mut stale = (*store.load()).clone();
        stale.source_routing_policy_revision = 1;
        stale.connect_timeout = Duration::from_secs(1);
        assert!(!store.publish_if_newer(stale).expect("stale publish"));
        assert_eq!(store.load().connect_timeout, Duration::from_secs(3));
    }

    #[test]
    fn stream_idle_does_not_change_client_fingerprint() {
        let mut first = TransportPolicySnapshot::default();
        let mut second = first.clone();
        second.stream_idle_timeout = Duration::from_secs(4);
        assert_eq!(
            first.client_config_fingerprint(),
            second.client_config_fingerprint()
        );
        first.connect_timeout = Duration::from_secs(4);
        assert_ne!(
            first.client_config_fingerprint(),
            second.client_config_fingerprint()
        );
    }
}
