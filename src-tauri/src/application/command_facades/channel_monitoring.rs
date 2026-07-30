use std::sync::Arc;

use crate::{
    application::{error::ApplicationError, monitoring::MonitoringService, pagination::PageLimit},
    models::{
        channel_monitors::{
            ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun,
            CreateChannelMonitorInput, CreateChannelMonitorTemplateInput,
            UpdateChannelMonitorInput, UpdateChannelMonitorTemplateInput,
        },
        monitoring::{CancelChannelMonitorExecutionReceipt, RunChannelMonitorReceipt},
    },
    services::monitoring::runner::MonitoringRunner,
};

#[derive(Clone)]
pub(crate) struct ChannelMonitoringCommandFacade {
    monitoring: Arc<MonitoringService>,
    runner: Arc<MonitoringRunner>,
}

impl ChannelMonitoringCommandFacade {
    pub(crate) fn new(monitoring: Arc<MonitoringService>, runner: Arc<MonitoringRunner>) -> Self {
        Self { monitoring, runner }
    }

    pub(crate) async fn list_channel_monitors(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ChannelMonitor>, ApplicationError> {
        self.monitoring.list_monitors(limit).await
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
            .list_legacy_run_page(monitor_id, None, limit)
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
        trigger_request_id: Option<String>,
    ) -> Result<RunChannelMonitorReceipt, String> {
        let trigger_request_id =
            trigger_request_id.unwrap_or_else(|| format!("manual:{}", uuid::Uuid::now_v7()));
        self.runner
            .enqueue_manual(monitor_id, trigger_request_id)
            .await
    }

    pub(crate) async fn cancel_channel_monitor_execution(
        &self,
        execution_id: String,
    ) -> Result<CancelChannelMonitorExecutionReceipt, ApplicationError> {
        let live_cancelled = self.runner.cancel_live(&execution_id);
        match self.monitoring.cancel_execution(execution_id.clone()).await {
            Ok(mut receipt) => {
                receipt.cancelled = receipt.cancelled || live_cancelled;
                if live_cancelled && receipt.status != "cancelled" {
                    receipt.status = "cancelling".to_string();
                }
                Ok(receipt)
            }
            Err(_error) if live_cancelled => Ok(CancelChannelMonitorExecutionReceipt {
                execution_id,
                status: "cancelling".to_string(),
                cancelled: true,
            }),
            Err(error) => Err(error),
        }
    }
}
