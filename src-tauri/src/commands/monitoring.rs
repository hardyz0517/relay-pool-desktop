use serde::Serialize;

use crate::{
    application::{
        error::ApplicationError, monitoring::MonitoringService, pagination::PageLimit,
        queries::channel_status::ChannelStatusQuery,
    },
    models::{
        channel_monitors::{
            ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun,
            ChannelMonitorRunCursor, ChannelMonitorRunPage, CreateChannelMonitorInput,
            CreateChannelMonitorRunInput, CreateChannelMonitorTemplateInput,
            UpdateChannelMonitorInput, UpdateChannelMonitorTemplateInput,
        },
        shared_capabilities::ChannelStatusSummary,
    },
};

const DEFAULT_PAGE_LIMIT: u32 = 200;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MonitoringCommandError {
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
}

pub(crate) async fn list_channel_monitor_templates(
    service: &MonitoringService,
    limit: Option<u32>,
) -> Result<Vec<ChannelMonitorRequestTemplate>, MonitoringCommandError> {
    service
        .list_templates(page_limit(limit)?)
        .await
        .map_err(monitoring_command_error)
}

pub(crate) async fn create_channel_monitor_template(
    service: &MonitoringService,
    input: CreateChannelMonitorTemplateInput,
) -> Result<ChannelMonitorRequestTemplate, MonitoringCommandError> {
    service
        .create_template(input)
        .await
        .map_err(monitoring_command_error)
}

pub(crate) async fn update_channel_monitor_template(
    service: &MonitoringService,
    input: UpdateChannelMonitorTemplateInput,
) -> Result<ChannelMonitorRequestTemplate, MonitoringCommandError> {
    service
        .update_template(input)
        .await
        .map_err(monitoring_command_error)
}

pub(crate) async fn delete_channel_monitor_template(
    service: &MonitoringService,
    id: String,
) -> Result<(), MonitoringCommandError> {
    service
        .delete_template(id)
        .await
        .map_err(monitoring_command_error)
}

pub(crate) async fn list_channel_monitors(
    service: &MonitoringService,
    limit: Option<u32>,
) -> Result<Vec<ChannelMonitor>, MonitoringCommandError> {
    service
        .list_monitors(page_limit(limit)?)
        .await
        .map_err(monitoring_command_error)
}

pub(crate) async fn create_channel_monitor(
    service: &MonitoringService,
    input: CreateChannelMonitorInput,
) -> Result<ChannelMonitor, MonitoringCommandError> {
    service
        .create_monitor(input)
        .await
        .map_err(monitoring_command_error)
}

pub(crate) async fn update_channel_monitor(
    service: &MonitoringService,
    input: UpdateChannelMonitorInput,
) -> Result<ChannelMonitor, MonitoringCommandError> {
    service
        .update_monitor(input)
        .await
        .map_err(monitoring_command_error)
}

pub(crate) async fn delete_channel_monitor(
    service: &MonitoringService,
    id: String,
) -> Result<(), MonitoringCommandError> {
    service
        .delete_monitor(id)
        .await
        .map_err(monitoring_command_error)
}

pub(crate) async fn list_channel_monitor_runs(
    service: &MonitoringService,
    monitor_id: String,
    cursor: Option<ChannelMonitorRunCursor>,
    limit: Option<u32>,
) -> Result<ChannelMonitorRunPage, MonitoringCommandError> {
    service
        .list_run_page(&monitor_id, cursor.as_ref(), page_limit(limit)?)
        .await
        .map_err(monitoring_command_error)
}

pub(crate) async fn record_channel_monitor_run(
    service: &MonitoringService,
    input: CreateChannelMonitorRunInput,
) -> Result<ChannelMonitorRun, MonitoringCommandError> {
    service
        .record_run(input)
        .await
        .map_err(monitoring_command_error)
}

pub(crate) async fn load_channel_status_summaries(
    query: &ChannelStatusQuery,
    limit: Option<u32>,
) -> Result<Vec<ChannelStatusSummary>, MonitoringCommandError> {
    query
        .load(page_limit(limit)?)
        .await
        .map_err(monitoring_command_error)
}

fn page_limit(value: Option<u32>) -> Result<PageLimit, MonitoringCommandError> {
    PageLimit::new(value.unwrap_or(DEFAULT_PAGE_LIMIT)).map_err(monitoring_command_error)
}

fn monitoring_command_error(error: ApplicationError) -> MonitoringCommandError {
    match error {
        ApplicationError::NotFound => MonitoringCommandError {
            code: "not_found",
            message: "not found",
        },
        ApplicationError::ConstraintViolation | ApplicationError::Conflict => {
            MonitoringCommandError {
                code: "invalid_request",
                message: "invalid request",
            }
        }
        ApplicationError::Busy => MonitoringCommandError {
            code: "busy",
            message: "resource busy",
        },
        ApplicationError::Unavailable => MonitoringCommandError {
            code: "unavailable",
            message: "persistence unavailable",
        },
        _ => MonitoringCommandError {
            code: "internal",
            message: "internal failure",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unbounded_command_page() {
        assert_eq!(
            page_limit(Some(0)).expect_err("zero").code,
            "invalid_request"
        );
        assert_eq!(
            page_limit(Some(501)).expect_err("oversized").code,
            "invalid_request"
        );
    }
}
