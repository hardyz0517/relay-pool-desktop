use std::sync::Arc;

use crate::{
    application::{
        clock::Clock, error::ApplicationError, monitoring::queries::ChannelStatusReadModelQuery,
    },
    models::monitoring::{
        ChannelMonitorAttemptHistoryInput, ChannelMonitorAttemptPage,
        ChannelMonitorExecutionDetail, ChannelMonitorExecutionIdInput,
        ChannelMonitorExecutionListInput, ChannelMonitorExecutionPage, ChannelStatusWorkspaceInput,
        ChannelStatusWorkspaceV2, MonitoringCapabilityCatalog, MonitoringClientProfileCapability,
        MonitoringProtocolCapability, ProtocolKind,
    },
    persistence::runtime::PersistenceHandle,
    services::monitoring::profiles::registry::BuiltinProfileRegistry,
};

#[derive(Clone)]
pub(crate) struct ChannelStatusQuery {
    read_model: ChannelStatusReadModelQuery,
}

impl ChannelStatusQuery {
    pub(crate) fn new(runtime: PersistenceHandle, clock: Arc<dyn Clock>) -> Self {
        Self {
            read_model: ChannelStatusReadModelQuery::new(runtime, clock),
        }
    }

    pub(crate) async fn load_workspace(
        &self,
        input: ChannelStatusWorkspaceInput,
    ) -> Result<ChannelStatusWorkspaceV2, ApplicationError> {
        self.read_model.load_workspace(input).await
    }

    pub(crate) async fn list_executions(
        &self,
        input: ChannelMonitorExecutionListInput,
    ) -> Result<ChannelMonitorExecutionPage, ApplicationError> {
        self.read_model.list_executions(input).await
    }

    pub(crate) async fn get_execution(
        &self,
        input: ChannelMonitorExecutionIdInput,
    ) -> Result<ChannelMonitorExecutionDetail, ApplicationError> {
        self.read_model.get_execution(input).await
    }

    pub(crate) async fn list_attempt_history(
        &self,
        input: ChannelMonitorAttemptHistoryInput,
    ) -> Result<ChannelMonitorAttemptPage, ApplicationError> {
        self.read_model.list_attempt_history(input).await
    }

    pub(crate) async fn list_monitoring_capabilities(
        &self,
    ) -> Result<MonitoringCapabilityCatalog, ApplicationError> {
        let protocols = [
            ProtocolKind::OpenAiChat,
            ProtocolKind::OpenAiResponses,
            ProtocolKind::AnthropicMessages,
            ProtocolKind::GeminiNative,
            ProtocolKind::XaiGrok,
            ProtocolKind::GenericOpenAi,
        ]
        .into_iter()
        .map(|protocol| MonitoringProtocolCapability {
            id: protocol.as_str().to_string(),
            enabled: true,
            streaming: !matches!(protocol, ProtocolKind::GenericOpenAi),
        })
        .collect();
        let registry = BuiltinProfileRegistry::default();
        let profiles = registry
            .list()
            .map(|profile| {
                let summary = profile.golden_summary();
                MonitoringClientProfileCapability {
                    id: summary.id.as_str().to_string(),
                    version: summary.version,
                    enabled: summary.enabled,
                    cli_compat: !matches!(
                        summary.id,
                        crate::models::monitoring::ClientProfileId::StandardApi
                    ),
                    supported_protocols: summary
                        .supported_protocols
                        .into_iter()
                        .map(|protocol| protocol.as_str().to_string())
                        .collect(),
                    method: summary.method,
                    path: summary.path,
                    header_names: summary.header_names,
                    body_defaults: summary.body_defaults,
                    profile_hash: summary.profile_hash,
                }
            })
            .collect();
        Ok(MonitoringCapabilityCatalog {
            protocols,
            profiles,
        })
    }
}
