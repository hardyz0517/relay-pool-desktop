use std::{sync::Arc, time::Duration};

use tokio_util::sync::CancellationToken;

use crate::{
    application::{
        error::ApplicationError,
        model_mapping_service::ModelMappingService,
        queries::{
            request_decision_trace::{
                RecentRouteDecisionsInput, RecentRouteDecisionsPage, RequestDecisionTrace,
            },
            routing_runtime::{RoutingRuntimeActivity, RoutingRuntimeOverlay},
            routing_workspace::{RoutingWorkspaceSnapshot, RoutingWorkspaceSnapshotInput},
        },
        routing_diagnostics_reader::RoutingDiagnosticsReader,
        routing_endpoint_ports::{
            RoutingEndpointHealthStatus, RoutingEndpointHealthWrite,
            RoutingEndpointHealthWritePort, RoutingEndpointTargetReadPort,
        },
        routing_policy_control_plane::RoutingPolicyMutationCoordinator,
        routing_policy_read::RoutingPolicyReadService,
        routing_read_ports::{
            RoutingCircuitStatusReadPort, RoutingProtectionReadPort, RoutingRuntimeOverlayReadPort,
            RoutingSimulationReadPort, RoutingWorkspaceReadPort,
        },
    },
    models::{
        document_sync::TrustedDocumentSource,
        pricing::BalanceSnapshot,
        routing::{ModelAlias, RouteSimulationInput, RouteSimulationResult},
        stations::{EndpointPingResult, StationEndpointHealth},
    },
    outbound::AsyncOutboundClient,
    services::{
        endpoint_ping::ping_station_endpoint as probe_station_endpoint,
        proxy::runtime::ProxyRuntimeState, time::now_millis_for_services,
    },
};

#[derive(Debug)]
pub(crate) enum EndpointPingCommandError {
    Application(ApplicationError),
    ResultUnknown,
}

impl From<ApplicationError> for EndpointPingCommandError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

#[derive(Clone)]
pub(crate) struct RoutingCommandFacade {
    workspace_read: Arc<dyn RoutingWorkspaceReadPort>,
    runtime_overlay_read: Arc<dyn RoutingRuntimeOverlayReadPort>,
    protection_read: Arc<dyn RoutingProtectionReadPort>,
    circuit_status_read: Arc<dyn RoutingCircuitStatusReadPort>,
    simulation_read: Arc<dyn RoutingSimulationReadPort>,
    endpoint_targets: Arc<dyn RoutingEndpointTargetReadPort>,
    endpoint_health: Arc<dyn RoutingEndpointHealthWritePort>,
    routing_policy_read: Arc<RoutingPolicyReadService>,
    model_mapping: Arc<ModelMappingService>,
    routing_diagnostics: Arc<RoutingDiagnosticsReader>,
    policy_mutations: Arc<RoutingPolicyMutationCoordinator>,
    outbound: AsyncOutboundClient,
    proxy: Arc<ProxyRuntimeState>,
}

impl RoutingCommandFacade {
    pub(crate) async fn apply_model_mapping_document(
        &self,
        document: crate::models::model_mapping::ModelMappingDocumentV1,
        source: TrustedDocumentSource,
    ) -> Result<crate::models::model_mapping::ModelMappingDocumentV1, ApplicationError> {
        self.model_mapping.apply_document(document, source).await
    }

    pub(crate) async fn restore_model_mapping_document(
        &self,
        document: crate::models::model_mapping::ModelMappingDocumentV1,
        expected_revision: u64,
    ) -> Result<crate::models::model_mapping::ModelMappingDocumentV1, ApplicationError> {
        self.model_mapping
            .restore_document(document, expected_revision)
            .await
    }

    pub(crate) async fn load_model_mapping_history_document(
        &self,
        revision: u64,
    ) -> Result<Option<String>, ApplicationError> {
        self.model_mapping.load_history_document(revision).await
    }

    pub(crate) async fn list_model_mapping_legacy_reviews(
        &self,
    ) -> Result<
        Vec<crate::persistence::stores::model_mapping_store::StoredLegacyModelAliasReview>,
        ApplicationError,
    > {
        self.model_mapping.list_legacy_reviews().await
    }

    pub(crate) async fn reconcile_model_mapping_document_sync(
        &self,
    ) -> Result<crate::application::model_mapping::ModelMappingDocumentSyncSnapshot, ApplicationError>
    {
        self.model_mapping.reconcile_document_sync().await
    }

    pub(crate) fn new(
        workspace_read: Arc<dyn RoutingWorkspaceReadPort>,
        runtime_overlay_read: Arc<dyn RoutingRuntimeOverlayReadPort>,
        protection_read: Arc<dyn RoutingProtectionReadPort>,
        circuit_status_read: Arc<dyn RoutingCircuitStatusReadPort>,
        simulation_read: Arc<dyn RoutingSimulationReadPort>,
        endpoint_targets: Arc<dyn RoutingEndpointTargetReadPort>,
        endpoint_health: Arc<dyn RoutingEndpointHealthWritePort>,
        routing_policy_read: Arc<RoutingPolicyReadService>,
        model_mapping: Arc<ModelMappingService>,
        routing_diagnostics: Arc<RoutingDiagnosticsReader>,
        policy_mutations: Arc<RoutingPolicyMutationCoordinator>,
        outbound: AsyncOutboundClient,
        proxy: Arc<ProxyRuntimeState>,
    ) -> Self {
        Self {
            workspace_read,
            runtime_overlay_read,
            protection_read,
            circuit_status_read,
            simulation_read,
            endpoint_targets,
            endpoint_health,
            routing_policy_read,
            model_mapping,
            routing_diagnostics,
            policy_mutations,
            outbound,
            proxy,
        }
    }

    pub(crate) async fn list_model_aliases(&self) -> Result<Vec<ModelAlias>, ApplicationError> {
        self.routing_diagnostics.list_model_aliases().await
    }

    pub(crate) async fn load_routing_policy(
        &self,
    ) -> Result<
        crate::persistence::stores::routing_policy_store::StoredRoutingPolicy,
        ApplicationError,
    > {
        self.routing_policy_read.load_routing_policy().await
    }

    pub(crate) async fn load_routing_policy_document_sync(
        &self,
    ) -> Result<
        Option<crate::persistence::stores::document_sync_store::StoredDocumentSync>,
        ApplicationError,
    > {
        self.routing_policy_read
            .load_routing_policy_document_sync()
            .await
    }

    pub(crate) async fn load_routing_policy_publication(
        &self,
        revision: u64,
        policy_generation_id: Option<&str>,
    ) -> Result<crate::application::routing_policy_read::RoutingPolicyPublication, ApplicationError>
    {
        let mut publication = self
            .routing_policy_read
            .load_routing_policy_publication(revision, policy_generation_id)
            .await?;
        // A durable generation may be active while the local proxy is stopped
        // (for example after a save followed by an application restart).  Do
        // not report that as live runtime activation until both runtime
        // components are present and accepting requests.
        let runtime_available = self
            .proxy
            .publication_availability()
            .await
            .can_receive_publication();
        if should_report_persisted_only(runtime_available, publication.status) {
            publication.activation_path = Some("persisted_only");
            publication.fallback_reason = Some("runtime_unavailable");
        }
        Ok(publication)
    }

    pub(crate) async fn get_routing_protection_status(
        &self,
    ) -> Result<
        crate::application::queries::routing_protection::RoutingProtectionStatus,
        ApplicationError,
    > {
        let now_ms = now_millis_for_services().min(i64::MAX as u128) as i64;
        let mut status = self.protection_read.read_protection_status(now_ms).await?;
        status.timeouts = Some(self.get_proxy_timeout_facts());
        Ok(status)
    }

    pub(crate) async fn get_routing_circuit_status(
        &self,
    ) -> Result<
        crate::application::queries::station_key_circuit_read::StationKeyCircuitReadSnapshot,
        ApplicationError,
    > {
        let now_ms = now_millis_for_services().min(i64::MAX as u128) as i64;
        self.circuit_status_read.read_circuit_status(now_ms).await
    }

    pub(crate) fn get_proxy_timeout_facts(
        &self,
    ) -> crate::application::queries::routing_protection::ProxyTimeoutFacts {
        let transport_policy = self.proxy.transport_policy_snapshot();
        crate::application::queries::routing_protection::ProxyTimeoutFacts {
            connect_seconds: transport_policy.connect_timeout.as_secs_f64(),
            first_byte_seconds: transport_policy.first_byte_timeout.as_secs_f64(),
            precommit_seconds: transport_policy.request_deadline.as_secs_f64(),
            buffered_execution_seconds: transport_policy.buffered_execution_timeout.as_secs_f64(),
            stream_idle_seconds: transport_policy.stream_idle_timeout.as_secs_f64(),
            owner: "transport_policy_store".to_string(),
        }
    }

    pub(crate) async fn apply_routing_policy_document_v3(
        &self,
        document: crate::models::routing_policy::RoutingPolicyDocumentV3,
    ) -> Result<
        crate::persistence::stores::routing_policy_store::StoredRoutingPolicy,
        ApplicationError,
    > {
        self.policy_mutations.apply_ui(document).await
    }

    pub(crate) async fn list_station_endpoint_health(
        &self,
    ) -> Result<Vec<StationEndpointHealth>, ApplicationError> {
        self.routing_diagnostics
            .list_station_endpoint_health()
            .await
    }

    pub(crate) async fn load_routing_workspace_snapshot(
        &self,
        input: RoutingWorkspaceSnapshotInput,
    ) -> Result<RoutingWorkspaceSnapshot, ApplicationError> {
        self.workspace_read.read_workspace_snapshot(input).await
    }

    pub(crate) async fn load_routing_runtime_overlay(
        &self,
    ) -> Result<RoutingRuntimeOverlay, ApplicationError> {
        let proxy: Arc<dyn RoutingRuntimeActivity> = self.proxy.clone();
        self.runtime_overlay_read.read_runtime_overlay(proxy).await
    }

    pub(crate) async fn list_recent_route_decisions(
        &self,
        input: RecentRouteDecisionsInput,
    ) -> Result<RecentRouteDecisionsPage, ApplicationError> {
        self.routing_diagnostics
            .list_recent_route_decisions(input)
            .await
    }

    pub(crate) async fn get_request_decision_trace(
        &self,
        request_log_id: String,
    ) -> Result<RequestDecisionTrace, ApplicationError> {
        // Durable terminal facts survive restart. A retained runtime trace is
        // supplemental diagnostics, never an alternative source of truth.
        if let Ok(trace) = self
            .routing_diagnostics
            .get_request_decision_trace(request_log_id.clone())
            .await
        {
            if let Some(runtime) = self.proxy.decision_trace_for_request(&request_log_id).await {
                return Ok(
                    crate::application::queries::request_decision_trace::append_runtime_trace(
                        trace, runtime,
                    ),
                );
            }
            return Ok(trace);
        }
        if let Some(trace) = self.proxy.decision_trace_for_request(&request_log_id).await {
            return Ok(
                crate::application::queries::request_decision_trace::decision_trace_from_runtime(
                    trace,
                ),
            );
        }
        self.routing_diagnostics
            .get_request_decision_trace(request_log_id)
            .await
    }

    pub(crate) async fn simulate_route(
        &self,
        input: RouteSimulationInput,
    ) -> Result<RouteSimulationResult, ApplicationError> {
        self.simulation_read.read_simulation(input).await
    }

    pub(crate) async fn list_balance_snapshots_for_station(
        &self,
        station_id: &str,
    ) -> Result<Vec<BalanceSnapshot>, ApplicationError> {
        self.routing_diagnostics
            .list_balance_snapshots_for_station(station_id)
            .await
    }

    pub(crate) async fn ping_station_endpoint(
        &self,
        station_id: String,
    ) -> Result<EndpointPingResult, EndpointPingCommandError> {
        let target = self
            .endpoint_targets
            .read_endpoint_probe_target(&station_id)
            .await?;
        let checked_at = now_millis_for_services().to_string();
        let api_base_url = target.api_base_url.clone();
        let probe = probe_station_endpoint(
            &self.outbound,
            &api_base_url,
            Duration::from_secs(5),
            CancellationToken::new(),
        )
        .await;
        let status = RoutingEndpointHealthStatus::parse(&probe.status)?;
        let health = self
            .endpoint_health
            .write_endpoint_health(RoutingEndpointHealthWrite {
                station_id: target.station_id,
                expected_endpoint_revision: target.endpoint_revision,
                status,
                latency_ms: probe.latency_ms,
                checked_at: checked_at.clone(),
                error_summary: probe.error_summary,
            })
            .await
            .map_err(endpoint_ping_write_error)?;
        Ok(EndpointPingResult {
            station_id: health.station_id,
            ok: probe.ok,
            status: health.status,
            latency_ms: health.latency_ms,
            checked_at: health.checked_at.unwrap_or(checked_at),
            error_summary: health.error_summary,
        })
    }
}

fn endpoint_ping_write_error(error: ApplicationError) -> EndpointPingCommandError {
    match error {
        // The transaction may have committed before the client observed its
        // result.  This is the only condition represented by ResultUnknown;
        // stale/unavailable/constraint failures remain actionable categories.
        ApplicationError::CommitOutcomeUnknown => EndpointPingCommandError::ResultUnknown,
        error => EndpointPingCommandError::Application(error),
    }
}

fn should_report_persisted_only(
    runtime_available: bool,
    status: crate::application::routing_policy_read::RoutingPolicyPublicationStatus,
) -> bool {
    !runtime_available
        && !matches!(
            status,
            crate::application::routing_policy_read::RoutingPolicyPublicationStatus::Failed
                | crate::application::routing_policy_read::RoutingPolicyPublicationStatus::Expired
        )
}

#[cfg(test)]
mod tests {
    use super::{
        endpoint_ping_write_error, should_report_persisted_only, EndpointPingCommandError,
    };
    use crate::application::{
        error::ApplicationError, routing_policy_read::RoutingPolicyPublicationStatus,
    };

    #[test]
    fn stopped_runtime_never_masks_failed_or_expired_publications() {
        for status in [
            RoutingPolicyPublicationStatus::Staged,
            RoutingPolicyPublicationStatus::Ready,
            RoutingPolicyPublicationStatus::WaitingLatestInput,
            RoutingPolicyPublicationStatus::Active,
        ] {
            assert!(should_report_persisted_only(false, status));
            assert!(!should_report_persisted_only(true, status));
        }
        for status in [
            RoutingPolicyPublicationStatus::Failed,
            RoutingPolicyPublicationStatus::Expired,
        ] {
            assert!(!should_report_persisted_only(false, status));
        }
    }

    #[test]
    fn endpoint_ping_preserves_stale_and_unavailable_errors() {
        assert!(matches!(
            endpoint_ping_write_error(ApplicationError::StaleRevision),
            EndpointPingCommandError::Application(ApplicationError::StaleRevision)
        ));
        assert!(matches!(
            endpoint_ping_write_error(ApplicationError::Unavailable),
            EndpointPingCommandError::Application(ApplicationError::Unavailable)
        ));
        assert!(matches!(
            endpoint_ping_write_error(ApplicationError::CommitOutcomeUnknown),
            EndpointPingCommandError::ResultUnknown
        ));
    }
}
