//! Application control plane for routing-policy mutations.
//!
//! The routing aggregate owns validation and staged CAS. This coordinator
//! serializes all mutation sources. Runtime publication is normally deferred
//! until the generation coordinator activates a staged generation; policy-only
//! changes may use the coordinator's bounded fast lane when all reuse checks
//! pass, and otherwise remain staged for the supervised generation runner.

use std::{path::PathBuf, sync::Arc};

use futures_util::future::BoxFuture;

use crate::{
    application::{error::ApplicationError, routing::RoutingService},
    models::{document_sync::TrustedDocumentSource, routing_policy::RoutingPolicyDocumentV3},
    persistence::runtime::PersistenceHandle,
    persistence::{error::PersistenceError, stores::routing_policy_store::StoredRoutingPolicy},
    services::proxy::{
        limits::ProxyStartupResourceLimits, runtime::ProxyRuntimeState,
        transport_policy::TransportPolicySnapshot,
    },
};

/// Outcome of the bounded interactive activation lane.
///
/// `NotApplicable` is deliberately not an error. It means the candidate is
/// still safe to process through the supervised generation runner (for
/// example, a CAS race or an incomplete build). Persistence failures and
/// corrupted generation evidence must never be downgraded to this outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FastActivationResult {
    Activated,
    NotApplicable { reason: &'static str },
}

/// Narrow application port for the optional interactive generation fast lane.
///
/// The application mutation coordinator owns policy validation and mutation
/// serialization; generation materialization remains a background-task
/// concern. Keeping this callback at the boundary avoids an application ->
/// background_tasks dependency (and the resulting module cycle) while still
/// allowing composition to wire the production implementation.
pub(crate) trait RoutingPolicyFastActivationPort: Send + Sync {
    fn try_fast_activate_for_policy_revision(
        &self,
        runtime: PersistenceHandle,
        policy_revision: u64,
    ) -> BoxFuture<'static, Result<FastActivationResult, PersistenceError>>;
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=routing-policy.fast-activation-test-default; owner=application/routing_policy_control_plane; remove_when=all isolated coordinator tests inject an explicit fast-activation port"
    )
)]
struct NoopRoutingPolicyFastActivationPort;

impl RoutingPolicyFastActivationPort for NoopRoutingPolicyFastActivationPort {
    fn try_fast_activate_for_policy_revision(
        &self,
        _runtime: PersistenceHandle,
        _policy_revision: u64,
    ) -> BoxFuture<'static, Result<FastActivationResult, PersistenceError>> {
        Box::pin(async {
            Ok(FastActivationResult::NotApplicable {
                reason: "fast_activation_unavailable",
            })
        })
    }
}

#[derive(Clone)]
pub(crate) struct RoutingPolicyMutationCoordinator {
    routing: Arc<RoutingService>,
    proxy: Arc<ProxyRuntimeState>,
    fast_activation: Arc<dyn RoutingPolicyFastActivationPort>,
    mutation_gate: Arc<tokio::sync::Mutex<()>>,
    generation_gate: Arc<tokio::sync::Mutex<()>>,
}

impl RoutingPolicyMutationCoordinator {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=routing-policy.fast-activation-compat-constructor; owner=application/routing_policy_control_plane; remove_when=all compositions and tests use new_with_fast_activation"
        )
    )]
    pub(crate) fn new(routing: Arc<RoutingService>, proxy: Arc<ProxyRuntimeState>) -> Self {
        Self::new_with_fast_activation(
            routing,
            proxy,
            Arc::new(NoopRoutingPolicyFastActivationPort),
        )
    }

    pub(crate) fn new_with_fast_activation(
        routing: Arc<RoutingService>,
        proxy: Arc<ProxyRuntimeState>,
        fast_activation: Arc<dyn RoutingPolicyFastActivationPort>,
    ) -> Self {
        Self {
            routing,
            proxy,
            fast_activation,
            mutation_gate: Arc::new(tokio::sync::Mutex::new(())),
            generation_gate: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Directory watched by the managed-document runner. The runner does not
    /// need to know how the persistence database is laid out.
    pub(crate) fn config_directory(&self) -> Option<PathBuf> {
        self.routing.routing_policy_config_directory()
    }

    pub(crate) async fn lock_mutation(&self) -> tokio::sync::OwnedMutexGuard<()> {
        Arc::clone(&self.mutation_gate).lock_owned().await
    }

    pub(crate) async fn lock_generation_lane(&self) -> tokio::sync::OwnedMutexGuard<()> {
        Arc::clone(&self.generation_gate).lock_owned().await
    }

    pub(crate) fn try_lock_generation_lane(&self) -> Option<tokio::sync::OwnedMutexGuard<()>> {
        Arc::clone(&self.generation_gate).try_lock_owned().ok()
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
        let _gate = self.lock_mutation().await;
        let stored = self
            .routing
            .apply_routing_policy_document_v3(document, source)
            .await?;
        let availability = self.proxy.publication_availability().await;
        let fast_activation = if availability.can_receive_publication() {
            let Some(_generation_gate) = self.try_lock_generation_lane() else {
                return Ok(stored);
            };
            self.fast_activation
                .try_fast_activate_for_policy_revision(
                    self.routing.persistence_handle(),
                    stored.revision,
                )
                .await?
        } else {
            FastActivationResult::NotApplicable {
                reason: "runtime_unavailable",
            }
        };
        if fast_activation == FastActivationResult::Activated {
            // Return the durable active row so callers can render the
            // activation result without waiting for the polling loop.
            let active = self.routing.load_routing_policy().await?;
            // Publish the same revision to the process-local transport store
            // before returning.  Persistence activation alone would leave
            // newly admitted requests using the previous timeout snapshot
            // until the supervised runner's next tick.
            self.publish_active_policy(&active).await?;
            crate::application::routing::sync_routing_policy_file(
                self.routing.persistence_handle(),
                &active,
                true,
            )
            .await?;
            return Ok(active);
        }
        Ok(stored)
    }

    /// Reconcile an external managed document and activate only when the CAS
    /// actually committed a newer policy. Invalid, unstable, or stale files
    /// remain diagnostics-only and never alter the active runtime snapshot.
    pub(crate) async fn reconcile_external(
        &self,
    ) -> Result<Option<StoredRoutingPolicy>, PersistenceError> {
        let _gate = self.lock_mutation().await;
        let stored = self
            .routing
            .reconcile_external_routing_policy_document()
            .await?;
        let fast_activation = if stored.is_some() {
            let availability = self.proxy.publication_availability().await;
            if availability.can_receive_publication() {
                let Some(_generation_gate) = self.try_lock_generation_lane() else {
                    return Ok(stored);
                };
                self.fast_activation
                    .try_fast_activate_for_policy_revision(
                        self.routing.persistence_handle(),
                        stored
                            .as_ref()
                            .expect("stored policy change is present")
                            .revision,
                    )
                    .await?
            } else {
                FastActivationResult::NotApplicable {
                    reason: "runtime_unavailable",
                }
            }
        } else {
            FastActivationResult::NotApplicable {
                reason: "no_policy_change",
            }
        };
        if fast_activation == FastActivationResult::Activated {
            let active = self
                .routing
                .load_routing_policy()
                .await
                .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
            self.publish_active_policy(&active)
                .await
                .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
            return Ok(Some(active));
        }
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
    use tokio_util::sync::CancellationToken;

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

    #[tokio::test]
    async fn stopped_proxy_persists_policy_only_change_without_fast_activation() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = crate::persistence::runtime::PersistenceRuntime::initialize_new(
            &temp.path().join("routing-stopped.sqlite3"),
        )
        .await
        .expect("persistence runtime");
        let handle = runtime.handle();
        let routing = Arc::new(RoutingService::new(handle.clone()));
        let seeded = routing.load_routing_policy().await.expect("seeded policy");
        let mut baseline_policy =
            crate::application::routing::routing_policy_v3_from_stored(&seeded.config)
                .expect("seeded V3 policy");
        baseline_policy.routing_group_filter =
            crate::models::routing::RoutingGroupFilter::GroupBindingId("group-a".into());
        routing
            .apply_routing_policy_document_v3(
                RoutingPolicyDocumentV3 {
                    format_version:
                        crate::models::routing_policy::ROUTING_POLICY_DOCUMENT_FORMAT_VERSION,
                    base_revision: seeded.revision,
                    policy: baseline_policy,
                },
                TrustedDocumentSource::ui(),
            )
            .await
            .expect("stage baseline policy");
        let cancellation = CancellationToken::new();
        crate::background_tasks::routing_generation_cutover_runner::build_ready_once(
            &handle,
            &cancellation,
        )
        .await
        .expect("build initial generation")
        .expect("initial generation");
        crate::background_tasks::routing_generation_cutover_runner::qualify_and_activate_once(
            &handle,
            &cancellation,
        )
        .await
        .expect("activate initial generation")
        .expect("activated initial generation");

        let proxy = Arc::new(ProxyRuntimeState::for_tests());
        let coordinator = RoutingPolicyMutationCoordinator::new(routing.clone(), proxy);
        let current = routing.load_routing_policy().await.expect("current policy");
        let mut policy =
            crate::application::routing::routing_policy_v3_from_stored(&current.config)
                .expect("v3 policy");
        policy.routing_group_filter = crate::models::routing::RoutingGroupFilter::AllGroups;

        let applied = coordinator
            .apply_ui(RoutingPolicyDocumentV3 {
                format_version:
                    crate::models::routing_policy::ROUTING_POLICY_DOCUMENT_FORMAT_VERSION,
                base_revision: current.revision,
                policy,
            })
            .await
            .expect("persist policy-only change");

        assert_eq!(applied.status, "staged");
        assert_eq!(
            routing
                .load_routing_policy()
                .await
                .expect("active policy")
                .revision,
            current.revision
        );
        runtime.close().await.expect("close persistence runtime");
    }

    #[tokio::test]
    async fn publishes_active_transport_policy_immediately() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = crate::persistence::runtime::PersistenceRuntime::initialize_new(
            &temp.path().join("routing-publication.sqlite3"),
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
        let published = StoredRoutingPolicy {
            config: serde_json::to_value(&policy).expect("serialize policy"),
            revision: current.revision.saturating_add(1),
            policy_version: current.policy_version,
            system_version: current.system_version,
            status: "active".to_string(),
            updated_at_ms: current.updated_at_ms,
        };

        coordinator
            .publish_active_policy(&published)
            .await
            .expect("publish active policy");

        let snapshot = proxy.transport_policy_snapshot();
        assert_eq!(snapshot.source_routing_policy_revision, published.revision);
        assert_eq!(snapshot.connect_timeout, std::time::Duration::from_secs(3));
        runtime.close().await.expect("close persistence runtime");
    }
}
