use std::sync::Arc;

use crate::{
    application::{error::ApplicationError, monitoring::MonitoringService, pagination::PageLimit},
    models::{
        channel_monitors::{
            ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun,
            CreateChannelMonitorInput, CreateChannelMonitorTemplateInput,
            UpdateChannelMonitorInput, UpdateChannelMonitorTemplateInput,
        },
        shared_capabilities::ChannelMonitorSummary,
    },
    services::channel_monitors::ChannelMonitorRunnerPort,
};

#[derive(Clone)]
pub(crate) struct ChannelMonitoringCommandFacade {
    monitoring: Arc<MonitoringService>,
    runner: Arc<dyn ChannelMonitorRunnerPort>,
}

impl ChannelMonitoringCommandFacade {
    pub(crate) fn new(
        monitoring: Arc<MonitoringService>,
        runner: Arc<dyn ChannelMonitorRunnerPort>,
    ) -> Self {
        Self { monitoring, runner }
    }

    pub(crate) async fn list_channel_monitors(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ChannelMonitor>, ApplicationError> {
        self.monitoring.list_monitors(limit).await
    }

    pub(crate) async fn list_channel_monitor_summaries(
        &self,
        run_since: Option<&str>,
        run_limit: Option<usize>,
    ) -> Result<Vec<ChannelMonitorSummary>, ApplicationError> {
        self.monitoring
            .list_channel_monitor_summaries(run_since, run_limit)
            .await
    }

    pub(crate) async fn create_channel_monitor(
        &self,
        input: CreateChannelMonitorInput,
    ) -> Result<ChannelMonitor, ApplicationError> {
        self.monitoring.create_monitor(input).await
    }

    pub(crate) async fn update_channel_monitor(
        &self,
        input: UpdateChannelMonitorInput,
    ) -> Result<ChannelMonitor, ApplicationError> {
        self.monitoring.update_monitor(input).await
    }

    pub(crate) async fn delete_channel_monitor(&self, id: String) -> Result<(), ApplicationError> {
        self.monitoring.delete_monitor(id).await
    }

    pub(crate) async fn list_channel_monitor_runs(
        &self,
        monitor_id: &str,
        limit: PageLimit,
    ) -> Result<Vec<ChannelMonitorRun>, ApplicationError> {
        self.monitoring
            .list_run_page(monitor_id, None, limit)
            .await
            .map(|page| page.items)
    }

    pub(crate) async fn list_channel_monitor_templates(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ChannelMonitorRequestTemplate>, ApplicationError> {
        self.monitoring.list_templates(limit).await
    }

    pub(crate) async fn create_channel_monitor_template(
        &self,
        input: CreateChannelMonitorTemplateInput,
    ) -> Result<ChannelMonitorRequestTemplate, ApplicationError> {
        self.monitoring.create_template(input).await
    }

    pub(crate) async fn update_channel_monitor_template(
        &self,
        input: UpdateChannelMonitorTemplateInput,
    ) -> Result<ChannelMonitorRequestTemplate, ApplicationError> {
        self.monitoring.update_template(input).await
    }

    pub(crate) async fn duplicate_channel_monitor_template(
        &self,
        id: String,
    ) -> Result<ChannelMonitorRequestTemplate, ApplicationError> {
        self.monitoring.duplicate_template(id).await
    }

    pub(crate) async fn delete_channel_monitor_template(
        &self,
        id: String,
    ) -> Result<(), ApplicationError> {
        self.monitoring.delete_template(id).await
    }

    pub(crate) async fn run_channel_monitor_now(
        &self,
        monitor_id: String,
    ) -> Result<Vec<ChannelMonitorRun>, String> {
        self.runner.run_monitor(monitor_id).await
    }
}
