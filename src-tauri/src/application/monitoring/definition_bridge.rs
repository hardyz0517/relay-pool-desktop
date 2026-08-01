use crate::{
    application::error::ApplicationError,
    models::{
        monitoring::{
            ClientProfileId, ClientProfileRef, DefinitionRevision, HealthPolicy,
            HealthWritebackMode, ProtocolKind, RetryPolicy, RiskPolicy, SchedulePolicy,
            TargetScope,
        },
        proxy::UpstreamApiFormat,
    },
    persistence::stores::monitoring::definitions::MonitorDefinitionConfigRow,
};

use super::planner::{MonitorPlanningSnapshot, ProtocolSelection, TargetCapabilitySnapshot};
use crate::application::queries::routing_runtime::RoutingMonitoringTargetSnapshot;

pub(crate) fn planning_snapshot_from_config(
    row: MonitorDefinitionConfigRow,
) -> Result<MonitorPlanningSnapshot, ApplicationError> {
    let protocol_kind =
        ProtocolKind::from_str(&row.protocol_kind).ok_or(ApplicationError::ConstraintViolation)?;
    let client_profile_id = ClientProfileId::from_str(&row.client_profile_id)
        .ok_or(ApplicationError::ConstraintViolation)?;
    let health_writeback_mode = HealthWritebackMode::from_str(&row.health_writeback_mode)
        .ok_or(ApplicationError::ConstraintViolation)?;
    let fallback_models = serde_json::from_str::<Vec<String>>(&row.fallback_models_json)
        .map_err(|_| ApplicationError::ConstraintViolation)?;
    let target_scope = match row.target_type.as_str() {
        "station" => TargetScope::Station {
            station_id: row.station_id.clone(),
        },
        "station_key" => TargetScope::StationKey {
            station_id: row.station_id.clone(),
            station_key_id: row
                .station_key_id
                .clone()
                .ok_or(ApplicationError::ConstraintViolation)?,
        },
        _ => return Err(ApplicationError::ConstraintViolation),
    };

    Ok(MonitorPlanningSnapshot {
        id: row.id,
        revision: DefinitionRevision(
            u64::try_from(row.schedule_revision)
                .map_err(|_| ApplicationError::ConstraintViolation)?,
        ),
        target_scope,
        protocol_selection: ProtocolSelection::Explicit(protocol_kind),
        client_profile: ClientProfileRef::new(
            client_profile_id,
            u32::try_from(row.client_profile_version)
                .map_err(|_| ApplicationError::ConstraintViolation)?,
        )
        .map_err(|_| ApplicationError::ConstraintViolation)?,
        primary_model: row.primary_model,
        fallback_models,
        schedule_policy: SchedulePolicy::new(
            row.interval_seconds,
            row.jitter_seconds,
            row.execution_timeout_ms,
            row.attempt_timeout_ms,
            5_000,
        )
        .map_err(|_| ApplicationError::ConstraintViolation)?,
        retry_policy: RetryPolicy::new(
            u8::try_from(row.retry_max_attempts_per_model)
                .map_err(|_| ApplicationError::ConstraintViolation)?,
            u64::try_from(row.retry_initial_backoff_ms)
                .map_err(|_| ApplicationError::ConstraintViolation)?,
            u64::try_from(row.retry_max_backoff_ms)
                .map_err(|_| ApplicationError::ConstraintViolation)?,
        )
        .map_err(|_| ApplicationError::ConstraintViolation)?,
        risk_policy: RiskPolicy::new(
            u32::try_from(row.risk_daily_probe_budget)
                .map_err(|_| ApplicationError::ConstraintViolation)?,
        )
        .map_err(|_| ApplicationError::ConstraintViolation)?,
        health_policy: HealthPolicy::new(
            health_writeback_mode,
            u8::try_from(row.health_failure_threshold)
                .map_err(|_| ApplicationError::ConstraintViolation)?,
            u8::try_from(row.health_recovery_threshold)
                .map_err(|_| ApplicationError::ConstraintViolation)?,
        )
        .map_err(|_| ApplicationError::ConstraintViolation)?,
    })
}

pub(crate) fn target_snapshot_from_monitoring_target(
    candidate: &RoutingMonitoringTargetSnapshot,
) -> TargetCapabilitySnapshot {
    TargetCapabilitySnapshot {
        station_id: candidate.station_id.clone(),
        station_key_id: candidate.station_key_id.clone(),
        endpoint_revision: candidate.endpoint_revision,
        provider_protocol: protocol_from_upstream_format(&candidate.upstream_api_format),
        endpoint_protocol: protocol_from_capabilities(candidate),
    }
}

pub(crate) fn target_snapshots_for_scope(
    snapshot: &MonitorPlanningSnapshot,
    candidates: &[RoutingMonitoringTargetSnapshot],
) -> Vec<TargetCapabilitySnapshot> {
    candidates
        .iter()
        .filter(|candidate| match &snapshot.target_scope {
            TargetScope::Station { station_id } => candidate.station_id == *station_id,
            TargetScope::StationKey {
                station_id,
                station_key_id,
            } => candidate.station_id == *station_id && candidate.station_key_id == *station_key_id,
        })
        .map(target_snapshot_from_monitoring_target)
        .collect()
}

fn protocol_from_upstream_format(format: &UpstreamApiFormat) -> Option<ProtocolKind> {
    match format {
        UpstreamApiFormat::OpenAiChatCompletions => Some(ProtocolKind::OpenAiChat),
        UpstreamApiFormat::OpenAiResponses => Some(ProtocolKind::OpenAiResponses),
        UpstreamApiFormat::CustomOpenAiCompatible => Some(ProtocolKind::GenericOpenAi),
        UpstreamApiFormat::Auto => None,
    }
}

fn protocol_from_capabilities(candidate: &RoutingMonitoringTargetSnapshot) -> Option<ProtocolKind> {
    match (
        candidate.supports_chat_completions,
        candidate.supports_responses,
    ) {
        (true, false) => Some(ProtocolKind::OpenAiChat),
        (false, true) => Some(ProtocolKind::OpenAiResponses),
        _ => None,
    }
}
