//! Application control plane for routing-policy mutations.
//!
//! The routing aggregate owns validation and staged CAS. This coordinator
//! serializes all mutation sources; runtime publication is deliberately
//! deferred until the generation coordinator atomically activates the staged
//! policy, quality and circuit components.

use std::{path::PathBuf, sync::Arc};

use crate::{
    application::{error::ApplicationError, routing::RoutingService},
    models::{document_sync::TrustedDocumentSource, routing_policy::RoutingPolicyDocumentV3},
    persistence::{error::PersistenceError, stores::routing_policy_store::StoredRoutingPolicy},
    services::proxy::{
        limits::ProxyStartupResourceLimits, runtime::ProxyRuntimeState,
        transport_policy::TransportPolicySnapshot,
    },
};

#[derive(Clone)]
pub(crate) struct RoutingPolicyMutationCoordinator {
    routing: Arc<RoutingService>,
    proxy: Arc<ProxyRuntimeState>,
    mutation_gate: Arc<tokio::sync::Mutex<()>>,
}

impl RoutingPolicyMutationCoordinator {
    pub(crate) fn new(routing: Arc<RoutingService>, proxy: Arc<ProxyRuntimeState>) -> Self {
        Self {
            routing,
            proxy,
            mutation_gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Directory watched by the managed-document runner. The runner does not
    /// need to know how the persistence database is laid out.
    pub(crate) fn config_directory(&self) -> Option<PathBuf> {
        self.routing.routing_policy_config_directory()
    }

    pub(crate) async fn apply_ui(
        &self,
        document: RoutingPolicyDocumentV3,
    ) -> Result<StoredRoutingPolicy, ApplicationError> {
        self.apply(document, TrustedDocumentSource::ui()).await
    }

    async fn apply(
        &self,
        document: RoutingPolicyDocumentV3,
        source: TrustedDocumentSource,
    ) -> Result<StoredRoutingPolicy, ApplicationError> {
        let _gate = self.mutation_gate.lock().await;
        let stored = self
            .routing
            .apply_routing_policy_document_v3(document, source)
            .await?;
        Ok(stored)
    }

    /// Reconcile an external managed document and activate only when the CAS
    /// actually committed a newer policy. Invalid, unstable, or stale files
    /// remain diagnostics-only and never alter the active runtime snapshot.
    pub(crate) async fn reconcile_external(
        &self,
    ) -> Result<Option<StoredRoutingPolicy>, PersistenceError> {
        let _gate = self.mutation_gate.lock().await;
        let stored = self
            .routing
            .reconcile_external_routing_policy_document()
            .await?;
        Ok(stored)
    }

    pub(crate) async fn publish_active_policy(
        &self,
        stored: &StoredRoutingPolicy,
    ) -> Result<(), ApplicationError> {
        let policy = crate::application::routing::routing_policy_v3_from_stored(&stored.config)
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let snapshot = TransportPolicySnapshot::from_timeout_policy(
            &policy.timeout_policy,
            stored.revision,
            ProxyStartupResourceLimits::default().upstream_pool_idle_timeout,
        )
        .map_err(|_| ApplicationError::ConstraintViolation)?;
        self.proxy
            .publish_transport_policy(snapshot)
            .await
            .map_err(|_| ApplicationError::Unavailable)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ui_commit_stages_without_publishing_an_unqualified_runtime_policy() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = crate::persistence::runtime::PersistenceRuntime::initialize_new(
            &temp.path().join("routing.sqlite3"),
        )
        .await
        .expect("persistence runtime");
        let routing = Arc::new(RoutingService::new(runtime.handle()));
        let proxy = Arc::new(ProxyRuntimeState::for_tests());
        let coordinator = RoutingPolicyMutationCoordinator::new(routing.clone(), proxy.clone());
        let current = routing.load_routing_policy().await.expect("current policy");
        let mut policy =
            crate::application::routing::routing_policy_v3_from_stored(&current.config)
                .expect("v3 policy");
        policy.timeout_policy.connect_seconds = 3.0;
        let applied = coordinator
            .apply_ui(RoutingPolicyDocumentV3 {
                format_version:
                    crate::models::routing_policy::ROUTING_POLICY_DOCUMENT_FORMAT_VERSION,
                base_revision: current.revision,
                policy,
            })
            .await
            .expect("apply policy");

        assert_eq!(applied.status, "staged");
        let snapshot = proxy.transport_policy_snapshot();
        assert_ne!(snapshot.source_routing_policy_revision, applied.revision);
        assert_ne!(snapshot.connect_timeout, std::time::Duration::from_secs(3));
        runtime.close().await.expect("close persistence runtime");
    }
}
