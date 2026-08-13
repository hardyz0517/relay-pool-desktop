use serde_json::Value;
use tauri::{AppHandle, State};
use tauri_plugin_notification::NotificationExt;

use crate::services::alerting::{
    DesktopNotificationAdapter, DesktopNotificationError, DesktopNotificationPayload,
    TauriDesktopNotificationAdapter,
};
use crate::{
    application::command_facades::{parse_observation, AlertingCommandFacade},
    commands::error,
    ipc::dto::alerting::{
        AlertPolicyDeleteInputDto, AlertPolicyDto, AlertPolicyInputDto, AlertingActivityInputDto,
        AlertingActivityPageDto, AlertingClearInputDto, AlertingClearRecordScope,
        AlertingClearScope, AlertingCurrentInputDto, AlertingDeliveryPageDto,
        AlertingHistoryInputDto, AlertingIncidentInputDto, AlertingIncidentPageDto,
        AlertingIncidentSummaryDto, AlertingMarkAllSeenInputDto, AlertingMarkSeenInputDto,
        AlertingNotificationTestInputDto, AlertingObservationInputDto, AlertingOccurrencePageDto,
        AlertingSettingsDto, AlertingSettingsInputDto, AlertingSnoozeInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn list_alert_policies(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<Vec<AlertPolicyDto>, error::CommandError> {
    correlation::in_command_scope("list_alert_policies", async {
        crate::ipc::dto::EmptyInputDto::parse(input)?;
        facade
            .list_policies()
            .await
            .map(|policies| policies.into_iter().map(Into::into).collect())
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_alert_policy(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<AlertPolicyDto, error::CommandError> {
    correlation::in_command_scope("upsert_alert_policy", async {
        let input = AlertPolicyInputDto::parse(input)?;
        let (policy, expected_revision) = input
            .into_domain(now_ms())
            .map_err(|_| invalid_alerting_input())?;
        facade
            .save_policy(policy, expected_revision)
            .await
            .map(Into::into)
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_alert_policy(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_alert_policy", async {
        let input = AlertPolicyDeleteInputDto::parse(input)?;
        facade
            .delete_policy(&input.id, input.expected_revision, now_ms())
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_alerting_settings(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<AlertingSettingsDto, error::CommandError> {
    correlation::in_command_scope("get_alerting_settings", async {
        crate::ipc::dto::EmptyInputDto::parse(input)?;
        facade
            .load_settings()
            .await
            .map(Into::into)
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_alerting_settings(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<AlertingSettingsDto, error::CommandError> {
    correlation::in_command_scope("update_alerting_settings", async {
        let input = AlertingSettingsInputDto::parse(input)?;
        let expected_revision = input.expected_revision.ok_or_else(invalid_alerting_input)?;
        let settings = input
            .into_domain(expected_revision)
            .map_err(|_| invalid_alerting_input())?;
        facade
            .update_settings(settings, expected_revision, now_ms())
            .await
            .map(Into::into)
            .map_err(super::public_command_application_error)
    })
    .await
}

fn invalid_alerting_input() -> error::CommandError {
    error::CommandError::try_new(
        error::CommandErrorCode::InvalidInput,
        "The alerting input is invalid.",
        false,
        None,
        None,
    )
    .expect("bounded alerting validation error")
}

#[tauri::command]
pub async fn list_alerting_incidents(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<AlertingIncidentPageDto, error::CommandError> {
    correlation::in_command_scope("list_alerting_incidents", async {
        let input = AlertingCurrentInputDto::parse(input)?;
        let cursor = input.cursor.map(|value| {
            crate::application::queries::change_center_workspace::IncidentCursor {
                updated_at_ms: value.updated_at_ms,
                id: value.id,
            }
        });
        facade
            .list_current(
                input.station_id.as_deref(),
                input.severity.as_deref(),
                input.lifecycle_state.as_deref(),
                cursor.as_ref(),
                input.limit.unwrap_or(50),
            )
            .await
            .map(Into::into)
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_alerting_activity(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<AlertingActivityPageDto, error::CommandError> {
    correlation::in_command_scope("list_alerting_activity", async {
        let input = AlertingActivityInputDto::parse(input)?;
        let cursor = input.cursor.map(|value| {
            crate::application::queries::change_center_workspace::ActivityCursor {
                activity_at_ms: value.updated_at_ms,
                id: value.id,
            }
        });
        facade
            .list_activity(
                input.station_id.as_deref(),
                input.severity.as_deref(),
                input.record_type.map(|value| value.as_str()),
                input.unread_only,
                cursor.as_ref(),
                input.limit.unwrap_or(50),
            )
            .await
            .map(Into::into)
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_alerting_incident(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<AlertingIncidentSummaryDto, error::CommandError> {
    correlation::in_command_scope("get_alerting_incident", async {
        let input = AlertingIncidentInputDto::parse(input)?;
        facade
            .get_incident_detail(&input.incident_id, input.episode_number)
            .await
            .map_err(super::public_command_application_error)?
            .map(Into::into)
            .ok_or_else(|| {
                error::CommandError::try_new(
                    error::CommandErrorCode::NotFound,
                    "The alerting incident was not found.",
                    false,
                    None,
                    None,
                )
                .expect("bounded not-found error")
            })
    })
    .await
}

#[tauri::command]
pub async fn list_alerting_occurrences(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<AlertingOccurrencePageDto, error::CommandError> {
    correlation::in_command_scope("list_alerting_occurrences", async {
        let input = AlertingHistoryInputDto::parse(input)?;
        let cursor = input.cursor.map(|value| {
            crate::application::queries::change_center_workspace::OccurrenceCursor {
                observed_at_ms: value.updated_at_ms,
                id: value.id,
            }
        });
        facade
            .list_occurrences(
                &input.incident_id,
                input.episode_number,
                cursor.as_ref(),
                input.limit.unwrap_or(50),
            )
            .await
            .map(Into::into)
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_alerting_deliveries(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<AlertingDeliveryPageDto, error::CommandError> {
    correlation::in_command_scope("list_alerting_deliveries", async {
        let input = AlertingHistoryInputDto::parse(input)?;
        let cursor = input.cursor.map(|value| {
            crate::application::queries::change_center_workspace::DeliveryCursor {
                created_at_ms: value.updated_at_ms,
                id: value.id,
            }
        });
        facade
            .list_deliveries(
                &input.incident_id,
                input.episode_number,
                cursor.as_ref(),
                input.limit.unwrap_or(50),
            )
            .await
            .map(Into::into)
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn record_alerting_observation(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<bool, error::CommandError> {
    correlation::in_command_scope("record_alerting_observation", async {
        let input = AlertingObservationInputDto::parse(input)?;
        let observation = parse_observation(
            input.source_observation_key,
            input.event_type,
            input.condition_key,
            input.kind,
            input.severity,
            input.object_type,
            input.object_id,
            input.station_id,
            input.station_key_id,
            input.source,
            input.reason_code,
            input.summary_json,
            input.observed_at_ms,
            input.fact_fresh_until_ms,
        )
        .map_err(super::public_command_application_error)?;
        facade
            .record_observation(observation)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn mark_alerting_seen(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("mark_alerting_seen", async {
        let input = AlertingMarkSeenInputDto::parse(input)?;
        match input
            .record_type
            .unwrap_or(crate::ipc::dto::alerting::AlertingActivityRecordType::Incident)
        {
            crate::ipc::dto::alerting::AlertingActivityRecordType::Incident => {
                facade
                    .mark_seen(
                        input.incident_id.as_deref().expect("validated incident id"),
                        input.episode_number.expect("validated episode"),
                        now_ms(),
                    )
                    .await
            }
            crate::ipc::dto::alerting::AlertingActivityRecordType::Change => {
                facade
                    .mark_information_seen(
                        input.activity_id.as_deref().expect("validated activity id"),
                        now_ms(),
                    )
                    .await
            }
        }
        .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn mark_all_alerting_seen(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<u64, error::CommandError> {
    correlation::in_command_scope("mark_all_alerting_seen", async {
        let input = AlertingMarkAllSeenInputDto::parse(input)?;
        let record_scope = input
            .record_scope
            .unwrap_or(AlertingClearRecordScope::Incidents);
        facade
            .mark_all_seen(
                input.station_id,
                input.severity,
                matches!(
                    record_scope,
                    AlertingClearRecordScope::Incidents | AlertingClearRecordScope::All
                ),
                matches!(
                    record_scope,
                    AlertingClearRecordScope::Information | AlertingClearRecordScope::All
                ),
                now_ms(),
            )
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn resolve_all_alerting_incidents(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<u64, error::CommandError> {
    correlation::in_command_scope("resolve_all_alerting_incidents", async {
        let input = AlertingMarkAllSeenInputDto::parse(input)?;
        facade
            .resolve_all_active(input.station_id, input.severity, now_ms())
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn clear_alerting_incidents(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<u64, error::CommandError> {
    correlation::in_command_scope("clear_alerting_incidents", async {
        let input = AlertingClearInputDto::parse(input)?;
        let record_scope = input
            .record_scope
            .unwrap_or(AlertingClearRecordScope::Incidents);
        facade
            .clear_activity(
                input.station_id,
                input.severity,
                input.lifecycle_state.map(AlertingClearScope::as_str),
                matches!(
                    record_scope,
                    AlertingClearRecordScope::Incidents | AlertingClearRecordScope::All
                ),
                matches!(
                    record_scope,
                    AlertingClearRecordScope::Information | AlertingClearRecordScope::All
                ),
            )
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn snooze_alerting_incident(
    facade: State<'_, AlertingCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("snooze_alerting_incident", async {
        let input = AlertingSnoozeInputDto::parse(input)?;
        facade
            .snooze(
                &input.incident_id,
                input.episode_number,
                input.until_ms,
                now_ms(),
            )
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn test_alerting_notification(
    app: AppHandle,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("test_alerting_notification", async {
        let channel = AlertingNotificationTestInputDto::parse(input)?.channel;
        match channel.as_str() {
            "in_app" => Ok(()),
            "desktop" => {
                let adapter = TauriDesktopNotificationAdapter::new(app);
                let payload = DesktopNotificationPayload::new(
                    "test-notification",
                    "test-incident",
                    1,
                    "test",
                    "relaypool://changes?source=test_notification",
                )
                .map_err(|_| invalid_alerting_input())?;
                adapter.send(&payload).map_err(public_notification_error)
            }
            _ => Err(invalid_alerting_input()),
        }
    })
    .await
}

fn public_notification_error(error: DesktopNotificationError) -> error::CommandError {
    let (code, message, retryable) = match error {
        DesktopNotificationError::PermissionDenied => (
            error::CommandErrorCode::PermissionDenied,
            "Desktop notification permission is denied.",
            false,
        ),
        DesktopNotificationError::Unavailable => (
            error::CommandErrorCode::Unsupported,
            "Desktop notifications are unavailable on this runtime.",
            false,
        ),
        DesktopNotificationError::Transient => (
            error::CommandErrorCode::ExternalUnavailable,
            "The desktop notification could not be delivered.",
            true,
        ),
        DesktopNotificationError::InvalidPayload | DesktopNotificationError::InvalidField(_) => (
            error::CommandErrorCode::InvalidInput,
            "The notification payload is invalid.",
            false,
        ),
    };
    error::CommandError::try_new(code, message, retryable, None, None)
        .expect("bounded notification command error")
}

/// Requesting permission is an explicit user action from the settings page.
/// Startup and background delivery never invoke this command implicitly.
#[tauri::command]
pub async fn request_desktop_notification_permission(
    app: AppHandle,
    input: Value,
) -> Result<String, error::CommandError> {
    correlation::in_command_scope("request_desktop_notification_permission", async {
        crate::ipc::dto::EmptyInputDto::parse(input)?;
        let state = app
            .notification()
            .request_permission()
            .map_err(|_| notification_unavailable())?;
        match state {
            tauri::plugin::PermissionState::Granted => Ok("allowed".to_string()),
            tauri::plugin::PermissionState::Denied
            | tauri::plugin::PermissionState::Prompt
            | tauri::plugin::PermissionState::PromptWithRationale => {
                Err(notification_permission_denied())
            }
        }
    })
    .await
}

#[tauri::command]
pub async fn get_desktop_notification_permission(
    app: AppHandle,
    input: Value,
) -> Result<String, error::CommandError> {
    correlation::in_command_scope("get_desktop_notification_permission", async {
        crate::ipc::dto::EmptyInputDto::parse(input)?;
        let adapter = TauriDesktopNotificationAdapter::new(app);
        Ok(adapter.permission_state().as_str().to_string())
    })
    .await
}

fn notification_permission_denied() -> error::CommandError {
    error::CommandError::try_new(
        error::CommandErrorCode::PermissionDenied,
        "Desktop notification permission is denied.",
        false,
        None,
        None,
    )
    .expect("bounded notification permission error")
}

fn notification_unavailable() -> error::CommandError {
    error::CommandError::try_new(
        error::CommandErrorCode::Unsupported,
        "Desktop notifications are unavailable on this runtime.",
        false,
        None,
        None,
    )
    .expect("bounded notification availability error")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}
