use futures_util::future::{BoxFuture, FutureExt};
use http::{header, HeaderValue, Method, StatusCode};
use serde_json::{json, Value};

use crate::{
    outbound::{
        OutboundFailureKind, OutboundHeaderPolicy, OutboundHeaders, OutboundRequest, RequestBudget,
        SecretHeaderValue,
    },
    services::{
        collectors::{
            contract::{
                CollectorContext, CollectorDriver, CollectorTaskKind, CredentialSecretPurpose,
                DriverOutput, DriverOutputStatus, ProviderKind, RedactedDiagnostics,
            },
            evidence::{redact_text, redact_value, EndpointEvidence, EndpointRole},
            facts::{CollectedModelFact, CollectorFacts},
            failure::{
                AuthEffect, DriverFailure, DriverFailureKind, FailedEndpoint, RetryDisposition,
            },
        },
        station_endpoints::build_api_url,
    },
};

pub const SUPPORTED_COLLECTOR_TASKS: &[CollectorTaskKind] =
    &[CollectorTaskKind::Detect, CollectorTaskKind::Models];

pub struct OpenAiCompatibleCollectorDriver;

impl CollectorDriver for OpenAiCompatibleCollectorDriver {
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAiCompatible
    }

    fn collect<'a>(
        &'a self,
        context: &'a CollectorContext<'a>,
        task: CollectorTaskKind,
    ) -> BoxFuture<'a, Result<DriverOutput, DriverFailure>> {
        async move {
            match task {
                CollectorTaskKind::Detect | CollectorTaskKind::Models => {
                    collect_models(context).await
                }
                CollectorTaskKind::Balance
                | CollectorTaskKind::Groups
                | CollectorTaskKind::Full => Err(DriverFailure::unsupported(format!(
                    "OpenAI-compatible collector does not support {task:?}"
                ))),
            }
        }
        .boxed()
    }
}

async fn collect_models(context: &CollectorContext<'_>) -> Result<DriverOutput, DriverFailure> {
    let Some(api_base_url) = context.endpoints.api_base_url.as_deref() else {
        return Err(invalid_request("OpenAI-compatible API base URL is missing"));
    };
    let url = build_api_url(api_base_url, "/v1/models")
        .map_err(|error| invalid_request(redact_text(&error)))?;
    let api_key = context
        .secrets
        .resolve_secret(
            &context.credential,
            CredentialSecretPurpose::AuthorizationHeader,
        )
        .await?;
    let response = context
        .outbound
        .execute(
            build_models_request(
                &url,
                api_key.expose(),
                context.proxy.clone(),
                context.budget,
                Some(context.correlation_id.clone()),
            )?,
            context.cancellation.clone(),
        )
        .await
        .map_err(|failure| driver_failure_from_outbound(EndpointRole::Models, failure))?;
    let endpoint = EndpointEvidence::new(
        EndpointRole::Models,
        "GET",
        Some(response.evidence.final_url.clone()),
        Some(response.status.as_u16()),
        None,
    );
    let payload = serde_json::from_slice::<Value>(&response.body).unwrap_or(Value::Null);
    if !response.status.is_success() {
        return Err(http_failure(response.status, payload, endpoint));
    }

    let models = parse_openai_models(&context.station.station_id, &payload);
    let model_names = models
        .iter()
        .map(|model| model.model.clone())
        .collect::<Vec<_>>();
    Ok(DriverOutput {
        facts: CollectorFacts {
            models,
            ..CollectorFacts::default()
        },
        evidence: vec![endpoint],
        status: if model_names.is_empty() {
            DriverOutputStatus::Partial
        } else {
            DriverOutputStatus::Success
        },
        diagnostics: RedactedDiagnostics {
            summary: Some(
                json!({
                    "modelCount": model_names.len(),
                    "models": model_names,
                })
                .to_string(),
            ),
            raw_json_redacted: Some(redact_value(&payload)),
        },
    })
}

fn build_models_request(
    url: &str,
    api_key: &str,
    proxy: crate::outbound::ProxyPolicy,
    budget: RequestBudget,
    correlation_id: Option<String>,
) -> Result<OutboundRequest, DriverFailure> {
    let policy = OutboundHeaderPolicy::provider_default();
    let mut headers = OutboundHeaders::new();
    headers
        .insert_sensitive(
            header::AUTHORIZATION,
            SecretHeaderValue::new(format!("Bearer {api_key}")),
            &policy,
        )
        .map_err(|failure| driver_failure_from_outbound(EndpointRole::Models, failure))?;
    headers
        .insert_public(
            header::ACCEPT,
            HeaderValue::from_static("application/json"),
            &policy,
        )
        .map_err(|failure| driver_failure_from_outbound(EndpointRole::Models, failure))?;
    Ok(OutboundRequest {
        method: Method::GET,
        url: url.to_string(),
        correlation_id,
        headers,
        body: Vec::new(),
        proxy,
        budget,
    })
}

pub fn parse_openai_models(station_id: &str, payload: &Value) -> Vec<CollectedModelFact> {
    payload
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(|model| CollectedModelFact {
            station_id: station_id.to_string(),
            model: model.to_string(),
            available: true,
            source: "openai_models".to_string(),
            confidence: 0.9,
        })
        .collect()
}

fn invalid_request(detail: impl Into<String>) -> DriverFailure {
    DriverFailure {
        kind: DriverFailureKind::InvalidRequest,
        retry: RetryDisposition::Never,
        auth_effect: AuthEffect::None,
        endpoint: None,
        evidence: crate::services::collectors::evidence::EvidenceSet::empty(),
        sanitized_detail: Some(redact_text(&detail.into())),
    }
}

fn http_failure(status: StatusCode, payload: Value, endpoint: EndpointEvidence) -> DriverFailure {
    let status_code = status.as_u16();
    let retry =
        if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::SERVICE_UNAVAILABLE {
            RetryDisposition::WithinBudget
        } else {
            RetryDisposition::Never
        };
    let (kind, auth_effect) = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => (
            DriverFailureKind::AuthRejected,
            AuthEffect::InvalidateCredential,
        ),
        StatusCode::TOO_MANY_REQUESTS => (DriverFailureKind::RateLimited, AuthEffect::None),
        status if status.is_server_error() => {
            (DriverFailureKind::ProviderUnavailable, AuthEffect::None)
        }
        _ => (DriverFailureKind::Transport, AuthEffect::None),
    };
    DriverFailure {
        kind,
        retry,
        auth_effect,
        endpoint: Some(FailedEndpoint {
            role: EndpointRole::Models,
            status_code: Some(status_code),
        }),
        evidence: crate::services::collectors::evidence::EvidenceSet::new([endpoint]),
        sanitized_detail: Some(redact_text(&payload.to_string())),
    }
}

fn driver_failure_from_outbound(
    role: EndpointRole,
    failure: crate::outbound::OutboundFailure,
) -> DriverFailure {
    let kind = match failure.kind {
        OutboundFailureKind::BudgetExhausted => DriverFailureKind::BudgetExhausted,
        OutboundFailureKind::Cancelled => DriverFailureKind::Cancelled,
        OutboundFailureKind::ConnectTimeout
        | OutboundFailureKind::FirstByteTimeout
        | OutboundFailureKind::BodyTimeout
        | OutboundFailureKind::TotalTimeout => DriverFailureKind::Timeout,
        OutboundFailureKind::InvalidUrl
        | OutboundFailureKind::InvalidHeader
        | OutboundFailureKind::HeaderNotAllowed(_)
        | OutboundFailureKind::ProxyPolicy
        | OutboundFailureKind::TransportPolicy
        | OutboundFailureKind::RedirectBlocked
        | OutboundFailureKind::RedirectLoop
        | OutboundFailureKind::RedirectLimitExceeded
        | OutboundFailureKind::RetryAfterExceedsBudget => DriverFailureKind::InvalidRequest,
        OutboundFailureKind::BodyLimitExceeded { .. } => DriverFailureKind::MalformedPayload,
        OutboundFailureKind::RequestFailed => DriverFailureKind::Transport,
    };
    DriverFailure {
        kind,
        retry: RetryDisposition::Never,
        auth_effect: AuthEffect::None,
        endpoint: Some(FailedEndpoint {
            role,
            status_code: None,
        }),
        evidence: crate::services::collectors::evidence::EvidenceSet::empty(),
        sanitized_detail: Some(redact_text(&failure.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn openai_models_parser_reads_data_ids() {
        let models = parse_openai_models(
            "station-1",
            &json!({
                "data": [
                    { "id": "gpt-4o-mini" },
                    { "id": "text-embedding-3-small" },
                    { "id": "" },
                    { "object": "model" }
                ]
            }),
        );

        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|model| model.model == "gpt-4o-mini"));
        assert!(models
            .iter()
            .all(|model| model.station_id == "station-1" && model.available));
    }

    #[test]
    fn openai_http_status_maps_auth_rate_and_server_failures() {
        let unauthorized = http_failure(
            StatusCode::UNAUTHORIZED,
            json!({"error": {"message": "bad key sk-p8-secret-plaintext-canary"}}),
            EndpointEvidence::new(EndpointRole::Models, "GET", None, Some(401), None),
        );
        assert_eq!(unauthorized.kind, DriverFailureKind::AuthRejected);
        assert_eq!(unauthorized.auth_effect, AuthEffect::InvalidateCredential);
        assert!(!unauthorized
            .sanitized_detail
            .as_deref()
            .unwrap_or_default()
            .contains("sk-p8-secret-plaintext-canary"));

        let rate_limited = http_failure(
            StatusCode::TOO_MANY_REQUESTS,
            json!({"error": {"message": "rate"}}),
            EndpointEvidence::new(EndpointRole::Models, "GET", None, Some(429), None),
        );
        assert_eq!(rate_limited.kind, DriverFailureKind::RateLimited);
        assert_eq!(rate_limited.retry, RetryDisposition::WithinBudget);

        let server = http_failure(
            StatusCode::BAD_GATEWAY,
            json!({"error": {"message": "upstream"}}),
            EndpointEvidence::new(EndpointRole::Models, "GET", None, Some(502), None),
        );
        assert_eq!(server.kind, DriverFailureKind::ProviderUnavailable);
    }
}
