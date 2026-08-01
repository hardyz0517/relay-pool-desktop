use serde_json::{json, Value};

use crate::application::request_finalization::failure::{
    CapabilityApplicabilitySet, ProviderErrorSemanticSignal,
};

use super::capability::{
    AdapterCapabilityFeature, AdapterCapabilityProtocol, AdapterCapabilitySignal,
    AdapterCapabilitySubject, AdapterCapabilityVerdict,
};

pub fn generate_response_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}",
        crate::services::time::now_millis_for_services()
    )
}

pub fn extract_choice_text(value: &Value) -> Option<String> {
    value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub fn wrap_chat_response_as_responses(value: Value, fallback_model: Option<&str>) -> Value {
    let content = extract_choice_text(&value).unwrap_or_default();
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .or(fallback_model)
        .unwrap_or("unknown-model");
    let created = value
        .get("created")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| (crate::services::time::now_millis_for_services() / 1000) as i64);
    let usage = value.get("usage").cloned().unwrap_or(Value::Null);

    json!({
        "id": value.get("id").cloned().unwrap_or_else(|| Value::String(generate_response_id("response"))),
        "object": "response",
        "created": created,
        "model": model,
        "output": [{
            "id": generate_response_id("output"),
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "output_text",
                "text": content,
            }],
        }],
        "output_text": content,
        "usage": usage,
    })
}

#[allow(dead_code)]
pub(crate) fn chat_completions_capability_signals() -> Vec<AdapterCapabilitySignal> {
    vec![
        AdapterCapabilitySignal::structural(
            AdapterCapabilitySubject::Protocol(AdapterCapabilityProtocol::ChatCompletions),
            AdapterCapabilityVerdict::Supported,
            "chat_completions_wire_protocol",
        ),
        AdapterCapabilitySignal::structural(
            AdapterCapabilitySubject::Protocol(AdapterCapabilityProtocol::Responses),
            AdapterCapabilityVerdict::Unsupported,
            "chat_adapter_does_not_execute_responses_protocol",
        ),
        AdapterCapabilitySignal::structural(
            AdapterCapabilitySubject::Feature(AdapterCapabilityFeature::Stream),
            AdapterCapabilityVerdict::Supported,
            "chat_streaming_supported_by_wire_protocol",
        ),
        AdapterCapabilitySignal::structural(
            AdapterCapabilitySubject::Feature(AdapterCapabilityFeature::Tools),
            AdapterCapabilityVerdict::Supported,
            "chat_tools_are_openai_compatible",
        ),
    ]
}

pub(crate) fn openai_error_semantic_signal(
    status: u16,
    body: Option<&Value>,
    station_key_id: &str,
    station_id: &str,
    model: Option<&str>,
    applicability: CapabilityApplicabilitySet,
) -> ProviderErrorSemanticSignal {
    let code = body.and_then(openai_error_code).unwrap_or_default();
    match status {
        401 => ProviderErrorSemanticSignal::ConfirmedAuthentication {
            station_key_id: station_key_id.to_string(),
        },
        403 if matches!(
            code,
            "invalid_api_key" | "invalid_api_key_format" | "authentication_error"
        ) =>
        {
            ProviderErrorSemanticSignal::ConfirmedAuthentication {
                station_key_id: station_key_id.to_string(),
            }
        }
        402 => ProviderErrorSemanticSignal::ConfirmedInsufficientBalance {
            station_id: station_id.to_string(),
        },
        404 if applicability.permits_model_not_found_learning()
            && matches!(code, "model_not_found" | "model_not_available") =>
        {
            ProviderErrorSemanticSignal::ConfirmedModelNotFound {
                station_key_id: station_key_id.to_string(),
                model: model.unwrap_or("unknown").to_string(),
            }
        }
        429 => ProviderErrorSemanticSignal::RateLimited {
            station_id: station_id.to_string(),
            retry_after_ms: None,
        },
        400 | 409 | 422 => ProviderErrorSemanticSignal::BadRequest,
        500..=599 => ProviderErrorSemanticSignal::ServerError {
            station_id: station_id.to_string(),
            endpoint_revision: 0,
        },
        _ => ProviderErrorSemanticSignal::GenericStatus { status },
    }
}

fn openai_error_code(body: &Value) -> Option<&str> {
    let error = body.get("error").unwrap_or(body);
    error
        .get("code")
        .and_then(Value::as_str)
        .or_else(|| error.get("type").and_then(Value::as_str))
}
