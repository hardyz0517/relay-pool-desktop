use std::{sync::Arc, time::Duration};

use tokio_util::sync::CancellationToken;

use crate::{
    application::{
        error::ApplicationError,
        model_mapping_service::ModelMappingService,
        queries::{
            operational_detail::StationKeyOperationalDetail,
            request_decision_trace::{
                RecentRouteDecisionsInput, RecentRouteDecisionsPage, RequestDecisionTrace,
            },
            routing_runtime::{RoutingRuntimeActivity, RoutingRuntimeOverlay},
            routing_workspace::{RoutingWorkspaceSnapshot, RoutingWorkspaceSnapshotInput},
        },
        routing::RoutingService,
        routing_diagnostics_reader::RoutingDiagnosticsReader,
        routing_policy_control_plane::RoutingPolicyMutationCoordinator,
        routing_policy_read::RoutingPolicyReadService,
    },
    models::{
        document_sync::TrustedDocumentSource,
        pricing::BalanceSnapshot,
        routing::{ModelAlias, RouteSimulationInput, RouteSimulationResult, StationKeyHealth},
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
    routing: Arc<RoutingService>,
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
        routing: Arc<RoutingService>,
        routing_policy_read: Arc<RoutingPolicyReadService>,
        model_mapping: Arc<ModelMappingService>,
        routing_diagnostics: Arc<RoutingDiagnosticsReader>,
        policy_mutations: Arc<RoutingPolicyMutationCoordinator>,
        outbound: AsyncOutboundClient,
        proxy: Arc<ProxyRuntimeState>,
    ) -> Self {
        Self {
            routing,
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

    pub(crate) async fn get_routing_protection_status(
        &self,
        requested_model: Option<&str>,
    ) -> Result<
        crate::application::queries::routing_protection::RoutingProtectionStatus,
        ApplicationError,
    > {
        let now_ms = now_millis_for_services().min(i64::MAX as u128) as i64;
        let capacity = self.proxy.capacity_protection_facts(now_ms).await;
        let mut status = self
            .routing
            .get_routing_protection_status(
                now_ms,
                capacity.as_deref().unwrap_or(&[]),
                capacity.is_some(),
                requested_model,
            )
            .await?;
        let transport_policy = self.proxy.transport_policy_snapshot();
        status.timeouts = Some(
            crate::application::queries::routing_protection::ProxyTimeoutFacts {
                connect_seconds: transport_policy.connect_timeout.as_secs_f64(),
                first_byte_seconds: transport_policy.first_byte_timeout.as_secs_f64(),
                precommit_seconds: transport_policy.request_deadline.as_secs_f64(),
                buffered_execution_seconds: transport_policy
                    .buffered_execution_timeout
                    .as_secs_f64(),
                stream_idle_seconds: transport_policy.stream_idle_timeout.as_secs_f64(),
                owner: "transport_policy_store".to_string(),
            },
        );
        Ok(status)
    }

    pub(crate) async fn apply_routing_policy_document_v2(
        &self,
        document: crate::models::routing_policy::RoutingPolicyDocumentV2,
    ) -> Result<
        crate::persistence::stores::routing_policy_store::StoredRoutingPolicy,
        ApplicationError,
    > {
        self.policy_mutations.apply_ui(document).await
    }

    pub(crate) async fn list_station_key_health(
        &self,
    ) -> Result<Vec<StationKeyHealth>, ApplicationError> {
        self.routing_diagnostics.list_station_key_health().await
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
        self.routing.load_routing_workspace_snapshot(input).await
    }

    pub(crate) async fn load_routing_runtime_overlay(
        &self,
    ) -> Result<RoutingRuntimeOverlay, ApplicationError> {
        let proxy: Arc<dyn RoutingRuntimeActivity> = self.proxy.clone();
        self.routing.load_routing_runtime_overlay(proxy).await
    }

    pub(crate) async fn list_recent_route_decisions(
        &self,
        input: RecentRouteDecisionsInput,
    ) -> Result<RecentRouteDecisionsPage, ApplicationError> {
        self.routing_diagnostics
            .list_recent_route_decisions(input)
            .await
    }

    pub(crate) async fn list_error_rate_history(
        &self,
        before_ms: Option<i64>,
        limit: usize,
    ) -> Result<crate::application::error_rate_protection::ErrorRateHistoryPageV1, ApplicationError>
    {
        self.routing_diagnostics
            .list_error_rate_history(before_ms, limit)
            .await
    }

    pub(crate) async fn get_station_key_operational_detail(
        &self,
        station_key_id: String,
    ) -> Result<StationKeyOperationalDetail, ApplicationError> {
        self.routing
            .get_station_key_operational_detail(station_key_id)
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
        self.routing.simulate_route(input).await
    }

    pub(crate) async fn list_balance_snapshots_for_station(
        &self,
        station_id: &str,
    ) -> Result<Vec<BalanceSnapshot>, ApplicationError> {
        self.routing_diagnostics
            .list_balance_snapshots_for_station(station_id)
            .await
    }

    pub(crate) async fn get_station_key_health(
        &self,
        station_key_id: String,
    ) -> Result<StationKeyHealth, ApplicationError> {
        self.routing_diagnostics
            .station_key_health_by_id(&station_key_id)
            .await
    }

    pub(crate) async fn ping_station_endpoint(
        &self,
        station_id: String,
    ) -> Result<EndpointPingResult, EndpointPingCommandError> {
        let target = self
            .routing
            .station_endpoint_probe_target(&station_id)
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
        let health = self
            .routing
            .record_station_endpoint_health(
                target.station_id,
                target.endpoint_revision,
                probe.status,
                probe.latency_ms,
                checked_at.clone(),
                probe.error_summary,
            )
            .await
            .map_err(|_| EndpointPingCommandError::ResultUnknown)?;
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
