use serde_json::Value;
use tauri::State;

use crate::{
    application::{command_facades::ChangeEventsCommandFacade, pagination::PageLimit},
    commands::error,
    ipc::dto::{
        change_logs::{
            ChangeEventDto, ChangeEventIdInputDto, ChangeEventIdsInputDto,
            StationIdInputDto as ChangeLogStationIdInputDto, UpsertChangeEventInputDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn list_change_events(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<Vec<ChangeEventDto>, error::CommandError> {
    correlation::in_command_scope("list_change_events", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_change_events(None, PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn clear_change_events(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("clear_change_events", async {
        EmptyInputDto::parse(input)?;
        facade
            .clear_change_events()
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_change_events_for_station(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<Vec<ChangeEventDto>, error::CommandError> {
    correlation::in_command_scope("list_change_events_for_station", async {
        let input = ChangeLogStationIdInputDto::parse(input)?;
        facade
            .list_change_events(
                Some(&input.station_id),
                PageLimit::new(200).expect("bounded limit"),
            )
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_change_event(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<ChangeEventDto, error::CommandError> {
    correlation::in_command_scope("upsert_change_event", async {
        let input = UpsertChangeEventInputDto::parse(input)?.into_domain();
        facade
            .upsert_change_event(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn mark_change_event_read(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<ChangeEventDto, error::CommandError> {
    correlation::in_command_scope("mark_change_event_read", async {
        let input = ChangeEventIdInputDto::parse(input)?;
        facade
            .mark_change_event_read(input.id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn mark_change_events_read(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<Vec<ChangeEventDto>, error::CommandError> {
    correlation::in_command_scope("mark_change_events_read", async {
        let input = ChangeEventIdsInputDto::parse(input)?;
        facade
            .mark_change_events_read(input.ids)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn dismiss_change_event(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<ChangeEventDto, error::CommandError> {
    correlation::in_command_scope("dismiss_change_event", async {
        let input = ChangeEventIdInputDto::parse(input)?;
        facade
            .dismiss_change_event(input.id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn resolve_change_event(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<ChangeEventDto, error::CommandError> {
    correlation::in_command_scope("resolve_change_event", async {
        let input = ChangeEventIdInputDto::parse(input)?;
        facade
            .resolve_change_event(input.id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}
