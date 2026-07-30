use std::sync::Arc;

use crate::{
    application::{
        error::ApplicationError, pagination::PageLimit, queries::channel_status::ChannelStatusQuery,
    },
    models::{
        monitoring::{
            ChannelMonitorAttemptHistoryInput, ChannelMonitorAttemptPage,
            ChannelMonitorExecutionDetail, ChannelMonitorExecutionIdInput,
            ChannelMonitorExecutionListInput, ChannelMonitorExecutionPage,
            ChannelStatusWorkspaceInput, ChannelStatusWorkspaceV2, MonitoringCapabilityCatalog,
        },
        shared_capabilities::ChannelStatusSummary,
    },
};

#[derive(Clone)]
pub(crate) struct ChannelStatusCommandFacade {
    channel_status: Arc<ChannelStatusQuery>,
}

impl ChannelStatusCommandFacade {
    pub(crate) fn new(channel_status: Arc<ChannelStatusQuery>) -> Self {
        Self { channel_status }
    }

    pub(crate) async fn list_channel_status_summaries(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ChannelStatusSummary>, ApplicationError> {
        self.channel_status.load(limit).await
    }

    pub(crate) async fn load_channel_status_workspace(
        &self,
        input: ChannelStatusWorkspaceInput,
    ) -> Result<ChannelStatusWorkspaceV2, ApplicationError> {
        self.channel_status.load_workspace(input).await
    }

    pub(crate) async fn list_channel_monitor_executions(
        &self,
        input: ChannelMonitorExecutionListInput,
    ) -> Result<ChannelMonitorExecutionPage, ApplicationError> {
        self.channel_status.list_executions(input).await
    }

    pub(crate) async fn get_channel_monitor_execution(
        &self,
        input: ChannelMonitorExecutionIdInput,
    ) -> Result<ChannelMonitorExecutionDetail, ApplicationError> {
        self.channel_status.get_execution(input).await
    }

    pub(crate) async fn list_channel_monitor_attempts(
        &self,
        input: ChannelMonitorAttemptHistoryInput,
    ) -> Result<ChannelMonitorAttemptPage, ApplicationError> {
        self.channel_status.list_attempt_history(input).await
    }

    pub(crate) async fn list_monitoring_capabilities(
        &self,
    ) -> Result<MonitoringCapabilityCatalog, ApplicationError> {
        self.channel_status.list_monitoring_capabilities().await
    }
}
