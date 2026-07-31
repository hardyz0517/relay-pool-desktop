use http::{header, HeaderValue, Method};
use serde_json::Value;
use std::time::{Duration, Instant};
use tauri::State;

use crate::{
    app_composition::ManagedWorkRuntime,
    application::{
        command_facades::{
            StationKeyConnectivityCommandError, StationKeyConnectivityCommandFacade,
            StationKeyConnectivityProbeTarget, StationKeyConnectivityResult,
            StationKeyModelDiscoveryResult,
        },
        connectivity_probe::{
            build_station_key_connectivity_probe_body, build_station_key_connectivity_probe_url,
            extract_station_key_connectivity_reply, model_ids_from_models_response,
            redact_connectivity_error, response_error_message,
            should_try_station_key_connectivity_chat_fallback,
            station_key_connectivity_model_candidates, station_key_connectivity_protocol_label,
            StationKeyConnectivityProbeKind, StationKeyConnectivityProbeResult,
            StationKeyConnectivityRequestMode, StationKeyConnectivityResponseMode,
            StationKeyConnectivitySseDecoder, DEFAULT_STATION_KEY_CONNECTIVITY_MODEL,
        },
    },
    background_tasks::{
        OperationContext, OperationFailureCode, OperationOwner, OperationRegistryError,
        OperationStartRequest, OperationTerminal,
    },
    commands::error,
    ipc::dto::{
        operations::{OperationIdInputDto, OperationStartedDto},
        routing_health_reads::RoutingStationKeyIdInputDto,
        station_keys::{
            StationKeyConnectivityInputDto, StationKeyConnectivityResultDto,
            StationKeyModelDiscoveryResultDto,
        },
    },
    models::{proxy::UpstreamApiFormat, routing::StationKeyCapabilities},
    observability::correlation,
    outbound::{
        AsyncOutboundClient, OutboundFailure, OutboundFailureKind, OutboundHeaderPolicy,
        OutboundHeaders, OutboundRequest, ProxyPolicy, RequestBudget, SecretHeaderValue,
    },
    services::{
        endpoint_ping::ping_station_endpoint, proxy::redact_error_message,
        station_endpoints::build_api_url,
    },
};

#[cfg(test)]
use crate::application::connectivity_probe::{
    run_station_key_connectivity_model_attempts, run_station_key_connectivity_single_model_probe,
};

#[cfg(test)]
use crate::application::connectivity_probe::STATION_KEY_CONNECTIVITY_SSE_PENDING_LIMIT;

#[cfg(test)]
use serde_json::json;

const STATION_KEY_CONNECTIVITY_MODEL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const STATION_KEY_CONNECTIVITY_PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const STATION_KEY_CONNECTIVITY_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);
const STATION_KEY_MODEL_DISCOVERY_OPERATION_TIMEOUT: Duration = Duration::from_secs(15);
const STATION_ENDPOINT_PING_TIMEOUT: Duration = Duration::from_secs(5);

fn command_application_error(
    error: crate::application::error::ApplicationError,
) -> error::CommandError {
    error::command_application_error(error)
}

fn station_key_connectivity_command_error(
    error: StationKeyConnectivityCommandError,
) -> error::CommandError {
    match error {
        StationKeyConnectivityCommandError::Application(error) => command_application_error(error),
        StationKeyConnectivityCommandError::Message(message) => message.into(),
    }
}

fn public_operation_registry_error(error: OperationRegistryError) -> error::CommandError {
    match error {
        OperationRegistryError::Overloaded => {
            error::CommandError::from_work(error::WorkFailure::Overloaded)
        }
        OperationRegistryError::Conflict { .. } => error::CommandError::try_new(
            error::CommandErrorCode::Conflict,
            "An operation with the same concurrency key is already running.",
            false,
            None,
            None,
        )
        .expect("operation conflict error is a bounded public contract"),
        OperationRegistryError::NotFound => error::CommandError::try_new(
            error::CommandErrorCode::NotFound,
            "The operation was not found.",
            false,
            None,
            None,
        )
        .expect("operation not-found error is a bounded public contract"),
        OperationRegistryError::Expired => error::CommandError::try_new(
            error::CommandErrorCode::NotFound,
            "The operation result has expired.",
            false,
            None,
            None,
        )
        .expect("operation expired error is a bounded public contract"),
        OperationRegistryError::AdmissionClosed => error::CommandError::try_new(
            error::CommandErrorCode::RuntimeUnavailable,
            "The desktop runtime is preparing data maintenance and is not accepting new operations.",
            true,
            None,
            None,
        )
        .expect("operation admission-closed error is a bounded public contract"),
        OperationRegistryError::InvalidSpec
        | OperationRegistryError::ProgressTooLarge { .. }
        | OperationRegistryError::TerminalAlreadyRecorded => error::CommandError::internal(None),
    }
}

#[tauri::command]
pub async fn start_station_key_connectivity_operation(
    facade: State<'_, StationKeyConnectivityCommandFacade>,
    runtime: State<'_, ManagedWorkRuntime>,
    input: Value,
) -> Result<OperationStartedDto, error::CommandError> {
    correlation::in_command_scope("start_station_key_connectivity_operation", async {
        let input = StationKeyConnectivityInputDto::parse(input)?;
        let station_key_id = input.station_key_id;
        let model = input.model;
        let target = facade
            .prepare_probe_target(station_key_id.clone())
            .await
            .map_err(station_key_connectivity_command_error)?;
        let station_id = target.key.station_id.clone();
        let endpoint_revision = target.key.station_endpoint_revision;
        let concurrency_key = format!("station-key-connectivity:{station_key_id}");
        let operation_station_key_id = station_key_id.clone();
        let facade = facade.inner().clone();
        let outbound = runtime.outbound.clone();
        let operation_id = runtime
            .operation
            .start(
                OperationStartRequest::new(
                    "station_key_connectivity",
                    OperationOwner::new("key-pool"),
                    move |context| {
                        Box::pin(async move {
                            run_station_key_connectivity_operation(
                                context,
                                facade,
                                outbound,
                                target,
                                operation_station_key_id,
                                station_id,
                                endpoint_revision,
                                model,
                            )
                            .await
                        })
                    },
                )
                .with_deadline(STATION_KEY_CONNECTIVITY_OPERATION_TIMEOUT)
                .with_concurrency_key(concurrency_key),
            )
            .map_err(public_operation_registry_error)?;
        Ok(OperationStartedDto::from(operation_id))
    })
    .await
}

#[tauri::command]
pub async fn get_station_key_connectivity_operation_result(
    facade: State<'_, StationKeyConnectivityCommandFacade>,
    runtime: State<'_, ManagedWorkRuntime>,
    input: Value,
) -> Result<StationKeyConnectivityResultDto, error::CommandError> {
    correlation::in_command_scope("get_station_key_connectivity_operation_result", async {
        let input = OperationIdInputDto::parse(input)?;
        let operation_id = input.operation_id();
        let snapshot = runtime
            .operation
            .status(operation_id)
            .map_err(public_operation_registry_error)?;
        if snapshot.kind != "station_key_connectivity" || snapshot.owner.feature != "key-pool" {
            return Err(error::CommandError::try_new(
                error::CommandErrorCode::NotFound,
                "The connectivity operation was not found.",
                false,
                None,
                None,
            )
            .expect("connectivity operation not-found error is bounded"));
        }
        if snapshot.terminal != Some(OperationTerminal::Completed) {
            return Err(error::CommandError::from_work(
                error::WorkFailure::ResultUnknown,
            ));
        }
        facade
            .get_result(operation_id)
            .map(StationKeyConnectivityResultDto::from)
            .ok_or_else(|| error::CommandError::from_work(error::WorkFailure::ResultUnknown))
    })
    .await
}

#[tauri::command]
pub async fn start_station_key_model_discovery_operation(
    facade: State<'_, StationKeyConnectivityCommandFacade>,
    runtime: State<'_, ManagedWorkRuntime>,
    input: Value,
) -> Result<OperationStartedDto, error::CommandError> {
    correlation::in_command_scope("start_station_key_model_discovery_operation", async {
        let input = RoutingStationKeyIdInputDto::parse(input)?;
        let station_key_id = input.station_key_id;
        let target = facade
            .prepare_probe_target(station_key_id.clone())
            .await
            .map_err(station_key_connectivity_command_error)?;
        let concurrency_key = format!("station-key-connectivity:{station_key_id}");
        let operation_station_key_id = station_key_id.clone();
        let facade = facade.inner().clone();
        let outbound = runtime.outbound.clone();
        let operation_id = runtime
            .operation
            .start(
                OperationStartRequest::new(
                    "station_key_model_discovery",
                    OperationOwner::new("key-pool"),
                    move |context| {
                        Box::pin(async move {
                            run_station_key_model_discovery_operation(
                                context,
                                facade,
                                outbound,
                                target,
                                operation_station_key_id,
                            )
                            .await
                        })
                    },
                )
                .with_deadline(STATION_KEY_MODEL_DISCOVERY_OPERATION_TIMEOUT)
                .with_concurrency_key(concurrency_key),
            )
            .map_err(public_operation_registry_error)?;
        Ok(OperationStartedDto::from(operation_id))
    })
    .await
}

#[tauri::command]
pub async fn get_station_key_model_discovery_operation_result(
    facade: State<'_, StationKeyConnectivityCommandFacade>,
    runtime: State<'_, ManagedWorkRuntime>,
    input: Value,
) -> Result<StationKeyModelDiscoveryResultDto, error::CommandError> {
    correlation::in_command_scope("get_station_key_model_discovery_operation_result", async {
        let input = OperationIdInputDto::parse(input)?;
        let operation_id = input.operation_id();
        let snapshot = runtime
            .operation
            .status(operation_id)
            .map_err(public_operation_registry_error)?;
        if snapshot.kind != "station_key_model_discovery" || snapshot.owner.feature != "key-pool" {
            return Err(error::CommandError::try_new(
                error::CommandErrorCode::NotFound,
                "The model discovery operation was not found.",
                false,
                None,
                None,
            )
            .expect("model discovery not-found error is bounded"));
        }
        if snapshot.terminal != Some(OperationTerminal::Completed) {
            return Err(error::CommandError::from_work(
                error::WorkFailure::ResultUnknown,
            ));
        }
        facade
            .get_model_discovery_result(operation_id)
            .map(StationKeyModelDiscoveryResultDto::from)
            .ok_or_else(|| error::CommandError::from_work(error::WorkFailure::ResultUnknown))
    })
    .await
}

async fn run_station_key_model_discovery_operation(
    context: OperationContext,
    facade: StationKeyConnectivityCommandFacade,
    outbound: AsyncOutboundClient,
    target: StationKeyConnectivityProbeTarget,
    station_key_id: String,
) -> OperationTerminal {
    let models = match discover_station_key_connectivity_models_outbound(
        &outbound,
        &target.key.station_api_base_url,
        target.api_key.as_str(),
        context.cancellation_token.clone(),
    )
    .await
    {
        Ok(models) => models,
        Err(terminal) => return terminal,
    };
    if context.cancellation_token.is_cancelled() {
        return OperationTerminal::Cancelled;
    }
    let _ = context.emit_progress(format!("discovered_models count={}", models.len()));
    facade.store_model_discovery_result(
        context.id,
        StationKeyModelDiscoveryResult {
            station_key_id,
            models,
        },
    );
    OperationTerminal::Completed
}

async fn run_station_key_connectivity_operation(
    context: OperationContext,
    facade: StationKeyConnectivityCommandFacade,
    outbound: AsyncOutboundClient,
    target: StationKeyConnectivityProbeTarget,
    station_key_id: String,
    station_id: String,
    endpoint_revision: i64,
    model: String,
) -> OperationTerminal {
    let ping_future = collect_station_endpoint_ping(
        &context,
        &facade,
        &outbound,
        station_id.clone(),
        endpoint_revision,
        target.key.station_api_base_url.clone(),
    );
    let model_probe =
        run_station_key_connectivity_prepared_outbound(&context, &outbound, &target, model);
    let (result, _) = tokio::join!(model_probe, ping_future);
    let result = match result {
        Ok(result) => result,
        Err(terminal) => return terminal,
    };
    if context.cancellation_token.is_cancelled() {
        return OperationTerminal::Cancelled;
    }
    let _ = context.emit_progress(format!(
        "completed ok={} status={} model={} mode={:?}",
        result.ok, result.status_code, result.model, result.response_mode
    ));
    facade.store_result(context.id, result.clone());
    context.enter_commit_barrier();
    match facade
        .record_station_key_connectivity(
            station_key_id,
            station_id,
            endpoint_revision,
            result.ok,
            result.duration_ms,
            result.message.clone(),
        )
        .await
    {
        Ok(()) => OperationTerminal::Completed,
        Err(_) => OperationTerminal::ResultUnknown,
    }
}

async fn collect_station_endpoint_ping(
    context: &OperationContext,
    facade: &StationKeyConnectivityCommandFacade,
    outbound: &AsyncOutboundClient,
    station_id: String,
    endpoint_revision: i64,
    base_url: String,
) {
    let probe = ping_station_endpoint(
        outbound,
        &base_url,
        STATION_ENDPOINT_PING_TIMEOUT,
        context.cancellation_token.clone(),
    )
    .await;
    if context.cancellation_token.is_cancelled() {
        return;
    }
    let checked_at = chrono::Utc::now().timestamp_millis().to_string();
    let _ = facade
        .record_station_endpoint_health(
            station_id,
            endpoint_revision,
            probe.status,
            probe.latency_ms,
            checked_at,
            probe.error_summary,
        )
        .await;
}

async fn run_station_key_connectivity_prepared_outbound(
    context: &OperationContext,
    outbound: &AsyncOutboundClient,
    target: &StationKeyConnectivityProbeTarget,
    model: String,
) -> Result<StationKeyConnectivityResult, OperationTerminal> {
    let upstream_api_format = match target.key.station_upstream_api_format.as_str() {
        "openai_chat_completions" => UpstreamApiFormat::OpenAiChatCompletions,
        "openai_responses" => UpstreamApiFormat::OpenAiResponses,
        "custom_openai_compatible" => UpstreamApiFormat::CustomOpenAiCompatible,
        _ => UpstreamApiFormat::Auto,
    };
    let discovered_models = discover_station_key_connectivity_models_outbound(
        outbound,
        &target.key.station_api_base_url,
        target.api_key.as_str(),
        context.cancellation_token.clone(),
    )
    .await
    .unwrap_or_default();
    let requested_model = model.trim().to_string();
    let candidates = station_key_connectivity_model_candidates(
        Some(&target.capabilities),
        Some(requested_model.as_str()),
        &discovered_models,
    );
    let mut last = None;
    for candidate in &candidates {
        if context.cancellation_token.is_cancelled() {
            return Err(OperationTerminal::Cancelled);
        }
        let result = run_station_key_connectivity_single_model_probe_outbound(
            context,
            outbound,
            &target.key.station_api_base_url,
            target.api_key.as_str(),
            candidate,
            &upstream_api_format,
            Some(&target.capabilities),
        )
        .await?;
        if result.ok {
            return Ok(StationKeyConnectivityResult {
                station_key_id: target.key.id.clone(),
                ok: result.ok,
                status_code: result.status_code,
                duration_ms: result.duration_ms,
                model: candidate.clone(),
                message: result.message,
                response_mode: result.response_mode,
                stream_fallback_reason: result.stream_fallback_reason,
            });
        }
        last = Some((candidate.clone(), result));
    }
    let (model, result) = last.unwrap_or_else(|| {
        (
            DEFAULT_STATION_KEY_CONNECTIVITY_MODEL.to_string(),
            StationKeyConnectivityProbeResult::failure(
                0,
                0,
                "connectivity probe did not run".to_string(),
            ),
        )
    });
    Ok(StationKeyConnectivityResult {
        station_key_id: target.key.id.clone(),
        ok: result.ok,
        status_code: result.status_code,
        duration_ms: result.duration_ms,
        model,
        message: result.message,
        response_mode: result.response_mode,
        stream_fallback_reason: result.stream_fallback_reason,
    })
}

async fn discover_station_key_connectivity_models_outbound(
    outbound: &AsyncOutboundClient,
    base_url: &str,
    api_key: &str,
    cancellation_token: tokio_util::sync::CancellationToken,
) -> Result<Vec<String>, OperationTerminal> {
    let url = build_api_url(base_url, "/v1/models").map_err(|_| OperationTerminal::Failed {
        code: OperationFailureCode::new("model-discovery-request-invalid"),
    })?;
    let response = outbound
        .execute(
            outbound_json_request(
                Method::GET,
                url,
                api_key,
                "application/json",
                Vec::new(),
                STATION_KEY_CONNECTIVITY_MODEL_DISCOVERY_TIMEOUT,
            )
            .map_err(outbound_failure_terminal_or_result)?,
            cancellation_token,
        )
        .await
        .map_err(outbound_failure_terminal_or_result)?;
    if !response.status.is_success() {
        return Err(OperationTerminal::Failed {
            code: OperationFailureCode::new("model-discovery-http"),
        });
    }
    let value =
        serde_json::from_slice::<Value>(&response.body).map_err(|_| OperationTerminal::Failed {
            code: OperationFailureCode::new("model-discovery-invalid-response"),
        })?;
    Ok(model_ids_from_models_response(&value))
}

async fn run_station_key_connectivity_single_model_probe_outbound(
    context: &OperationContext,
    outbound: &AsyncOutboundClient,
    base_url: &str,
    api_key: &str,
    model: &str,
    upstream_api_format: &UpstreamApiFormat,
    capabilities: Option<&StationKeyCapabilities>,
) -> Result<StationKeyConnectivityProbeResult, OperationTerminal> {
    let response_result = send_station_key_connectivity_probe_outbound(
        context,
        outbound,
        base_url,
        api_key,
        model,
        StationKeyConnectivityProbeKind::Responses,
    )
    .await?;
    if response_result.ok
        || !should_try_station_key_connectivity_chat_fallback(
            upstream_api_format,
            capabilities,
            response_result.status_code,
            &response_result.message,
        )
    {
        return Ok(response_result);
    }

    let chat_result = send_station_key_connectivity_probe_outbound(
        context,
        outbound,
        base_url,
        api_key,
        model,
        StationKeyConnectivityProbeKind::ChatCompletions,
    )
    .await?;
    let duration_ms = response_result
        .duration_ms
        .saturating_add(chat_result.duration_ms);
    if chat_result.ok {
        let mut chat_result = chat_result;
        chat_result.duration_ms = duration_ms;
        return Ok(chat_result);
    }

    Ok(StationKeyConnectivityProbeResult::failure(
        chat_result.status_code,
        duration_ms,
        format!(
            "Responses: {}; Chat Completions: {}",
            response_result.message, chat_result.message
        ),
    ))
}

async fn send_station_key_connectivity_probe_outbound(
    context: &OperationContext,
    outbound: &AsyncOutboundClient,
    base_url: &str,
    api_key: &str,
    model: &str,
    kind: StationKeyConnectivityProbeKind,
) -> Result<StationKeyConnectivityProbeResult, OperationTerminal> {
    let protocol = station_key_connectivity_protocol_label(kind);
    let _ = context.emit_progress(format!("attempt_started protocol={protocol} model={model}"));
    let stream_result = send_station_key_connectivity_stream_probe_outbound(
        context, outbound, base_url, api_key, model, kind,
    )
    .await?;
    if stream_result.ok {
        return Ok(stream_result.with_response_mode(StationKeyConnectivityResponseMode::Stream));
    }

    let fallback_reason = redact_connectivity_error(&stream_result.message);
    let _ = context.emit_progress(format!("fallback reason={fallback_reason}"));
    let fallback_result = send_station_key_connectivity_non_stream_probe_outbound(
        context, outbound, base_url, api_key, model, kind,
    )
    .await?;
    let duration_ms = stream_result
        .duration_ms
        .saturating_add(fallback_result.duration_ms);
    if fallback_result.ok {
        return Ok(StationKeyConnectivityProbeResult::success(
            fallback_result.status_code,
            duration_ms,
            fallback_result.message,
        )
        .with_response_mode(StationKeyConnectivityResponseMode::NonStreamFallback)
        .with_stream_fallback_reason(Some(fallback_reason)));
    }

    Ok(StationKeyConnectivityProbeResult::failure(
        fallback_result.status_code,
        duration_ms,
        format!(
            "Stream: {}; Non-stream fallback: {}",
            stream_result.message, fallback_result.message
        ),
    )
    .with_response_mode(StationKeyConnectivityResponseMode::NonStreamFallback)
    .with_stream_fallback_reason(Some(fallback_reason)))
}

async fn send_station_key_connectivity_non_stream_probe_outbound(
    context: &OperationContext,
    outbound: &AsyncOutboundClient,
    base_url: &str,
    api_key: &str,
    model: &str,
    kind: StationKeyConnectivityProbeKind,
) -> Result<StationKeyConnectivityProbeResult, OperationTerminal> {
    let url = match build_station_key_connectivity_probe_url(base_url, kind) {
        Ok(url) => url,
        Err(error) => {
            return Ok(StationKeyConnectivityProbeResult::failure(
                0,
                0,
                redact_error_message(&format!("API Base URL 无效: {error}")),
            ));
        }
    };
    let body = build_station_key_connectivity_probe_body(
        model,
        kind,
        StationKeyConnectivityRequestMode::NonStream,
    );
    let started = Instant::now();
    let response = outbound
        .execute(
            outbound_json_request(
                Method::POST,
                url,
                api_key,
                "application/json",
                serde_json::to_vec(&body).unwrap_or_default(),
                STATION_KEY_CONNECTIVITY_PROBE_TIMEOUT,
            )
            .map_err(|_| OperationTerminal::Failed {
                code: OperationFailureCode::new("connectivity-request-invalid"),
            })?,
            context.cancellation_token.clone(),
        )
        .await
        .map_err(outbound_failure_terminal_or_result)?;
    let duration_ms = elapsed_ms(started);
    let status_code = response.status.as_u16();
    let response_text = String::from_utf8_lossy(&response.body).to_string();
    if response.status.is_success() {
        let message =
            extract_station_key_connectivity_reply(&response_text, kind).unwrap_or_else(|| {
                match kind {
                    StationKeyConnectivityProbeKind::Responses => "Responses 连通正常".to_string(),
                    StationKeyConnectivityProbeKind::ChatCompletions => {
                        "Chat Completions 连通正常".to_string()
                    }
                }
            });
        return Ok(StationKeyConnectivityProbeResult::success(
            status_code,
            duration_ms,
            message,
        ));
    }
    Ok(StationKeyConnectivityProbeResult::failure(
        status_code,
        duration_ms,
        response_error_message(&response_text, status_code),
    ))
}

async fn send_station_key_connectivity_stream_probe_outbound(
    context: &OperationContext,
    outbound: &AsyncOutboundClient,
    base_url: &str,
    api_key: &str,
    model: &str,
    kind: StationKeyConnectivityProbeKind,
) -> Result<StationKeyConnectivityProbeResult, OperationTerminal> {
    let url = match build_station_key_connectivity_probe_url(base_url, kind) {
        Ok(url) => url,
        Err(error) => {
            return Ok(StationKeyConnectivityProbeResult::failure(
                0,
                0,
                redact_error_message(&format!("API Base URL 无效: {error}")),
            ));
        }
    };
    let request_body = build_station_key_connectivity_probe_body(
        model,
        kind,
        StationKeyConnectivityRequestMode::Stream,
    );
    let started = Instant::now();
    let mut response_body = Vec::new();
    let response = outbound
        .execute_stream(
            outbound_json_request(
                Method::POST,
                url,
                api_key,
                "text/event-stream",
                serde_json::to_vec(&request_body).unwrap_or_default(),
                STATION_KEY_CONNECTIVITY_PROBE_TIMEOUT,
            )
            .map_err(|_| OperationTerminal::Failed {
                code: OperationFailureCode::new("connectivity-request-invalid"),
            })?,
            context.cancellation_token.clone(),
            |chunk| {
                response_body.extend_from_slice(chunk);
                let _ = context.emit_progress("stream_chunk_received");
                Ok(())
            },
        )
        .await
        .map_err(outbound_failure_terminal_or_result)?;
    let duration_ms = elapsed_ms(started);
    let status_code = response.status.as_u16();
    let response_text = String::from_utf8_lossy(&response_body).to_string();
    if !response.status.is_success() {
        return Ok(StationKeyConnectivityProbeResult::failure(
            status_code,
            duration_ms,
            response_error_message(&response_text, status_code),
        ));
    }
    let content_type = response
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !content_type.contains("text/event-stream") {
        return Ok(StationKeyConnectivityProbeResult::failure(
            status_code,
            duration_ms,
            redact_connectivity_error(&format!(
                "Expected text/event-stream response, got {}",
                if content_type.is_empty() {
                    "missing content-type"
                } else {
                    content_type.as_str()
                }
            )),
        ));
    }
    let mut decoder = StationKeyConnectivitySseDecoder::new(kind);
    let deltas = match decoder.push(&response_body) {
        Ok(deltas) => deltas,
        Err(error) => {
            return Ok(StationKeyConnectivityProbeResult::failure(
                status_code,
                duration_ms,
                redact_connectivity_error(&error),
            ));
        }
    };
    for _ in deltas {
        let _ = context.emit_progress("stream_delta_received");
    }
    match decoder.finish() {
        Ok(message) if !message.trim().is_empty() => Ok(
            StationKeyConnectivityProbeResult::success(status_code, duration_ms, message),
        ),
        Ok(_) => Ok(StationKeyConnectivityProbeResult::success(
            status_code,
            duration_ms,
            match kind {
                StationKeyConnectivityProbeKind::Responses => {
                    "Responses streaming connected".to_string()
                }
                StationKeyConnectivityProbeKind::ChatCompletions => {
                    "Chat Completions streaming connected".to_string()
                }
            },
        )),
        Err(error) => Ok(StationKeyConnectivityProbeResult::failure(
            status_code,
            duration_ms,
            redact_connectivity_error(&error),
        )),
    }
}

fn outbound_json_request(
    method: Method,
    url: String,
    api_key: &str,
    accept: &'static str,
    body: Vec<u8>,
    timeout: Duration,
) -> Result<OutboundRequest, OutboundFailure> {
    let policy = OutboundHeaderPolicy::provider_default();
    let mut headers = OutboundHeaders::new();
    headers.insert_sensitive(
        header::AUTHORIZATION,
        SecretHeaderValue::new(format!("Bearer {api_key}")),
        &policy,
    )?;
    headers.insert_public(header::ACCEPT, HeaderValue::from_static(accept), &policy)?;
    if !body.is_empty() {
        headers.insert_public(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
            &policy,
        )?;
    }
    Ok(OutboundRequest {
        method,
        url,
        correlation_id: correlation::current_id_string(),
        headers,
        body,
        proxy: ProxyPolicy::Direct,
        budget: RequestBudget::from_now(timeout),
        retry_policy: Default::default(),
    })
}

fn outbound_failure_terminal_or_result(error: OutboundFailure) -> OperationTerminal {
    match error.kind {
        OutboundFailureKind::Cancelled => OperationTerminal::Cancelled,
        OutboundFailureKind::TotalTimeout | OutboundFailureKind::BudgetExhausted => {
            OperationTerminal::TimedOut
        }
        _ => OperationTerminal::Failed {
            code: OperationFailureCode::new("connectivity-transport"),
        },
    }
}

fn elapsed_ms(started: Instant) -> i64 {
    started.elapsed().as_millis().min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_key_connectivity_probe_uses_low_token_responses_request() {
        let body = build_station_key_connectivity_probe_body(
            "gpt-test",
            StationKeyConnectivityProbeKind::Responses,
            StationKeyConnectivityRequestMode::NonStream,
        );

        assert_eq!(body["model"], "gpt-test");
        assert_eq!(body["input"], "hi");
        assert_eq!(body["store"], false);
        assert_eq!(body["max_output_tokens"], 32);
    }

    #[test]
    fn station_key_connectivity_stream_bodies_request_streaming() {
        let responses = build_station_key_connectivity_probe_body(
            "gpt-test",
            StationKeyConnectivityProbeKind::Responses,
            StationKeyConnectivityRequestMode::Stream,
        );
        let chat = build_station_key_connectivity_probe_body(
            "gpt-test",
            StationKeyConnectivityProbeKind::ChatCompletions,
            StationKeyConnectivityRequestMode::Stream,
        );

        assert_eq!(responses["model"], "gpt-test");
        assert_eq!(responses["input"], "hi");
        assert_eq!(responses["stream"], true);
        assert_eq!(chat["model"], "gpt-test");
        assert_eq!(chat["messages"][0]["content"], "hi");
        assert_eq!(chat["stream"], true);
    }

    #[test]
    fn station_key_connectivity_responses_sse_decodes_split_deltas() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);

        assert!(decoder
            .push(br#"data: {"type":"response.output_text.delta","delta":"Hel"#)
            .unwrap()
            .is_empty());
        assert_eq!(decoder.push(br#"lo"}"#).unwrap(), Vec::<String>::new());
        assert_eq!(
            decoder
                .push(b"\n\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"!\"}\n\ndata: {\"type\":\"response.completed\"}\n\n")
                .unwrap(),
            vec!["Hello".to_string(), "!".to_string()]
        );
        assert_eq!(decoder.finish().unwrap(), "Hello!");
    }

    #[test]
    fn station_key_connectivity_responses_sse_accepts_done_sentinel() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);

        let deltas = decoder
            .push(
                b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hi\"}\n\ndata: [DONE]\n\n",
            )
            .unwrap();

        assert_eq!(deltas, vec!["Hi".to_string()]);
        assert_eq!(decoder.finish().unwrap(), "Hi");
    }

    #[test]
    fn station_key_connectivity_chat_sse_decodes_crlf_comments_and_done() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::ChatCompletions);

        let deltas = decoder
            .push(
                b": keep-alive\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n",
            )
            .unwrap();

        assert_eq!(deltas, vec!["Hi".to_string()]);
        assert_eq!(decoder.finish().unwrap(), "Hi");
    }

    #[test]
    fn station_key_connectivity_sse_rejects_malformed_json() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);

        let error = decoder
            .push(b"data: {not-json}\n\n")
            .expect_err("malformed SSE JSON should fail the stream attempt");

        assert!(error.contains("SSE"));
    }

    #[test]
    fn station_key_connectivity_sse_rejects_missing_terminal_signal() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);

        let deltas = decoder
            .push(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n")
            .unwrap();
        assert_eq!(deltas, vec!["partial".to_string()]);

        let error = decoder
            .finish()
            .expect_err("closing without response.completed should fail");

        assert!(error.contains("terminal"));
    }

    #[test]
    fn station_key_connectivity_sse_rejects_oversized_pending_data() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);
        let oversized = vec![b'a'; STATION_KEY_CONNECTIVITY_SSE_PENDING_LIMIT + 1];

        let error = decoder
            .push(&oversized)
            .expect_err("oversized pending data should fail");

        assert!(error.contains("too large"));
    }

    #[test]
    fn station_key_connectivity_extracts_responses_reply_text() {
        let value = json!({
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "Hi! What can I help you with?"
                }]
            }]
        });

        assert_eq!(
            extract_station_key_connectivity_reply(
                &value.to_string(),
                StationKeyConnectivityProbeKind::Responses
            ),
            Some("Hi! What can I help you with?".to_string())
        );
    }

    #[test]
    fn station_key_connectivity_extracts_chat_reply_text() {
        let value = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hi there"
                }
            }]
        });

        assert_eq!(
            extract_station_key_connectivity_reply(
                &value.to_string(),
                StationKeyConnectivityProbeKind::ChatCompletions
            ),
            Some("Hi there".to_string())
        );
    }

    #[test]
    fn station_key_connectivity_candidates_choose_lowest_allowed_model() {
        let capabilities = StationKeyCapabilities {
            station_key_id: "key-lowest".to_string(),
            supports_chat_completions: true,
            supports_responses: true,
            supports_embeddings: false,
            supports_stream: true,
            supports_tools: false,
            supports_vision: false,
            supports_reasoning: false,
            model_allowlist: vec![
                "gpt-4.1".to_string(),
                "gpt-4.1-mini".to_string(),
                "claude-sonnet-4".to_string(),
            ],
            model_blocklist: Vec::new(),
            preferred_models: vec!["gpt-4.1".to_string()],
            only_use_as_backup: false,
            routing_tags: Vec::new(),
            updated_at: "0".to_string(),
        };

        let candidates = station_key_connectivity_model_candidates(Some(&capabilities), None, &[]);

        assert_eq!(candidates[0], "gpt-4.1-mini");
        assert!(!candidates.contains(&"gpt-4o-mini".to_string()));
    }

    #[test]
    fn station_key_connectivity_probe_posts_to_responses_endpoint() {
        let url = build_station_key_connectivity_probe_url(
            "https://relay.example/v1",
            StationKeyConnectivityProbeKind::Responses,
        )
        .expect("build responses probe URL");

        assert_eq!(url, "https://relay.example/v1/responses");
    }

    #[test]
    fn station_key_connectivity_probe_uses_complete_api_namespace() {
        let url = build_station_key_connectivity_probe_url(
            "https://relay.example/api/v3",
            StationKeyConnectivityProbeKind::Responses,
        )
        .expect("build API namespace responses probe URL");

        assert_eq!(url, "https://relay.example/api/v3/responses");
    }

    #[test]
    fn station_key_connectivity_candidates_use_discovered_model_when_not_configured() {
        let discovered = vec!["claude-test".to_string()];
        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["claude-test"]);
    }

    #[test]
    fn station_key_connectivity_candidates_do_not_default_to_retired_gpt_4o_mini() {
        let candidates = station_key_connectivity_model_candidates(None, None, &[]);

        assert_eq!(candidates, vec!["gpt-4.1-mini"]);
    }

    #[test]
    fn station_key_connectivity_candidates_keep_fastest_discovered_models() {
        let discovered = vec![
            "codex-auto-review".to_string(),
            "gpt-5.4".to_string(),
            "gpt-5.5".to_string(),
        ];

        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["codex-auto-review", "gpt-5.4"]);
    }

    #[test]
    fn station_key_connectivity_candidates_are_capped_for_interactive_tests() {
        let discovered = vec![
            "gpt-4.1".to_string(),
            "gpt-4.1-mini".to_string(),
            "gpt-4.1-nano".to_string(),
            "gpt-5.4".to_string(),
        ];

        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["gpt-4.1-nano", "gpt-4.1-mini"]);
    }

    #[test]
    fn station_key_connectivity_candidates_sort_discovered_models_by_lowest_cost() {
        let discovered = vec![
            "gpt-4.1".to_string(),
            "gpt-4.1-mini".to_string(),
            "gpt-4.1-nano".to_string(),
        ];

        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["gpt-4.1-nano", "gpt-4.1-mini"]);
    }

    #[test]
    fn station_key_connectivity_attempts_next_model_after_503() {
        let candidates = vec!["codex-auto-review".to_string(), "gpt-5.4".to_string()];
        let mut attempted = Vec::new();

        let (model, result) =
            run_station_key_connectivity_model_attempts(&candidates, |candidate| {
                attempted.push(candidate.to_string());
                if candidate == "gpt-5.4" {
                    StationKeyConnectivityProbeResult::success(
                        200,
                        42,
                        "Chat Completions 连通正常".to_string(),
                    )
                } else {
                    StationKeyConnectivityProbeResult::failure(
                        503,
                        12,
                        "Service temporarily unavailable".to_string(),
                    )
                }
            });

        assert_eq!(attempted, vec!["codex-auto-review", "gpt-5.4"]);
        assert_eq!(model, "gpt-5.4");
        assert!(result.ok);
    }

    #[test]
    fn station_key_connectivity_attempts_next_model_after_responses_and_chat_fail() {
        let candidates = vec!["codex-auto-review".to_string(), "gpt-5.4".to_string()];
        let mut attempted = Vec::new();

        let (model, result) =
            run_station_key_connectivity_model_attempts(&candidates, |candidate| {
                run_station_key_connectivity_single_model_probe(
                    &UpstreamApiFormat::Auto,
                    None,
                    |kind| {
                        attempted.push((candidate.to_string(), kind));
                        match (candidate, kind) {
                            ("gpt-5.4", StationKeyConnectivityProbeKind::ChatCompletions) => {
                                StationKeyConnectivityProbeResult::success(
                                    200,
                                    11,
                                    "Chat Completions 连通正常".to_string(),
                                )
                            }
                            _ => StationKeyConnectivityProbeResult::failure(
                                503,
                                7,
                                "Service temporarily unavailable".to_string(),
                            ),
                        }
                    },
                )
            });

        assert_eq!(
            attempted,
            vec![
                (
                    "codex-auto-review".to_string(),
                    StationKeyConnectivityProbeKind::Responses,
                ),
                (
                    "codex-auto-review".to_string(),
                    StationKeyConnectivityProbeKind::ChatCompletions,
                ),
                (
                    "gpt-5.4".to_string(),
                    StationKeyConnectivityProbeKind::Responses,
                ),
                (
                    "gpt-5.4".to_string(),
                    StationKeyConnectivityProbeKind::ChatCompletions,
                ),
            ]
        );
        assert_eq!(model, "gpt-5.4");
        assert!(result.ok);
        assert_eq!(result.status_code, 200);
        assert_eq!(result.duration_ms, 18);
        assert_eq!(result.message, "Chat Completions 连通正常");
    }

    #[test]
    fn station_key_connectivity_network_error_does_not_switch_protocol() {
        let candidates = vec!["gpt-4.1-mini".to_string()];
        let mut attempted = Vec::new();

        let (_model, result) =
            run_station_key_connectivity_model_attempts(&candidates, |candidate| {
                run_station_key_connectivity_single_model_probe(
                    &UpstreamApiFormat::Auto,
                    None,
                    |kind| {
                        attempted.push((candidate.to_string(), kind));
                        match kind {
                            StationKeyConnectivityProbeKind::Responses => {
                                StationKeyConnectivityProbeResult::failure(
                                    0,
                                    9,
                                    "Network Error".to_string(),
                                )
                            }
                            StationKeyConnectivityProbeKind::ChatCompletions => {
                                StationKeyConnectivityProbeResult::success(
                                    200,
                                    13,
                                    "Chat Completions 连通正常".to_string(),
                                )
                            }
                        }
                    },
                )
            });

        assert_eq!(
            attempted,
            vec![(
                "gpt-4.1-mini".to_string(),
                StationKeyConnectivityProbeKind::Responses,
            )]
        );
        assert!(!result.ok);
        assert_eq!(result.status_code, 0);
    }

    #[test]
    fn station_key_connectivity_parse_body_400_can_switch_protocol() {
        let candidates = vec!["codex-auto-review".to_string()];
        let mut attempted = Vec::new();

        let (_model, result) =
            run_station_key_connectivity_model_attempts(&candidates, |candidate| {
                run_station_key_connectivity_single_model_probe(
                    &UpstreamApiFormat::CustomOpenAiCompatible,
                    None,
                    |kind| {
                        attempted.push((candidate.to_string(), kind));
                        match kind {
                            StationKeyConnectivityProbeKind::Responses => {
                                StationKeyConnectivityProbeResult::failure(
                                    400,
                                    17,
                                    "Failed to parse request body".to_string(),
                                )
                            }
                            StationKeyConnectivityProbeKind::ChatCompletions => {
                                StationKeyConnectivityProbeResult::success(
                                    200,
                                    23,
                                    "Chat Completions connected".to_string(),
                                )
                            }
                        }
                    },
                )
            });

        assert_eq!(
            attempted,
            vec![
                (
                    "codex-auto-review".to_string(),
                    StationKeyConnectivityProbeKind::Responses,
                ),
                (
                    "codex-auto-review".to_string(),
                    StationKeyConnectivityProbeKind::ChatCompletions,
                ),
            ]
        );
        assert!(result.ok);
        assert_eq!(result.status_code, 200);
        assert_eq!(result.duration_ms, 40);
    }

    #[test]
    fn station_key_connectivity_plain_400_does_not_switch_protocol() {
        assert!(!should_try_station_key_connectivity_chat_fallback(
            &UpstreamApiFormat::CustomOpenAiCompatible,
            None,
            400,
            "The provider rejected the current credentials.",
        ));
    }

    #[test]
    fn station_key_connectivity_chat_probe_uses_low_token_request() {
        let body = build_station_key_connectivity_probe_body(
            "claude-test",
            StationKeyConnectivityProbeKind::ChatCompletions,
            StationKeyConnectivityRequestMode::NonStream,
        );

        assert_eq!(body["model"], "claude-test");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert_eq!(body["stream"], false);
        assert_eq!(body["max_tokens"], 32);
    }

    #[test]
    fn station_key_connectivity_auto_format_can_fallback_to_chat_on_503() {
        assert!(should_try_station_key_connectivity_chat_fallback(
            &UpstreamApiFormat::Auto,
            None,
            503,
            "Service temporarily unavailable",
        ));
    }
}
