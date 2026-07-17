use bytes::Bytes;
use http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::Value;

use crate::models::{proxy::UpstreamApiFormat, routing::RouteEndpointKind};

use super::{
    adapters::responses::upstream_responses_path,
    error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
    request::CanonicalProxyRequest,
    responses_chat_fallback::{normalize_for_chat, responses_fallback_error_message},
    RouteCandidate,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EndpointAdapter {
    Models,
    Embeddings,
    ChatCompletions,
    Responses,
}

impl EndpointAdapter {
    pub(crate) fn for_endpoint(endpoint: &RouteEndpointKind) -> Self {
        match endpoint {
            RouteEndpointKind::Models => Self::Models,
            RouteEndpointKind::Embeddings => Self::Embeddings,
            RouteEndpointKind::ChatCompletions => Self::ChatCompletions,
            RouteEndpointKind::Responses => Self::Responses,
        }
    }

    pub(crate) fn prepare(
        self,
        request: &CanonicalProxyRequest,
        candidate: &RouteCandidate,
        mapped_model: Option<&str>,
    ) -> Result<PreparedUpstreamRequest, ProxyFailure> {
        match self {
            Self::Models => Ok(PreparedUpstreamRequest {
                method: Method::GET,
                path: "/v1/models".to_string(),
                headers: upstream_headers(&request.forwarded_headers, None),
                body: Bytes::new(),
                response_mode: ResponseMode::BufferedJson,
            }),
            Self::Embeddings => Ok(PreparedUpstreamRequest {
                method: Method::POST,
                path: "/v1/embeddings".to_string(),
                headers: upstream_headers(&request.forwarded_headers, Some("application/json")),
                body: rewrite_json_model(&request.body, mapped_model)?,
                response_mode: ResponseMode::BufferedJson,
            }),
            Self::ChatCompletions => Ok(PreparedUpstreamRequest {
                method: Method::POST,
                path: "/v1/chat/completions".to_string(),
                headers: upstream_headers(
                    &request.forwarded_headers,
                    Some(if request.stream {
                        "text/event-stream"
                    } else {
                        "application/json"
                    }),
                ),
                body: rewrite_json_model(&request.body, mapped_model)?,
                response_mode: if request.stream {
                    ResponseMode::StreamPassthrough
                } else {
                    ResponseMode::BufferedJson
                },
            }),
            Self::Responses => prepare_responses(request, candidate, mapped_model),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedUpstreamRequest {
    pub method: Method,
    pub path: String,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub response_mode: ResponseMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponseMode {
    BufferedJson,
    BufferedChatToResponses,
    StreamPassthrough,
}

pub(crate) fn response_headers_for_downstream(headers: &HeaderMap) -> HeaderMap {
    let mut output = HeaderMap::new();
    for name in [
        header::CONTENT_TYPE,
        header::CACHE_CONTROL,
        http::HeaderName::from_static("x-request-id"),
        http::HeaderName::from_static("openai-processing-ms"),
    ] {
        if let Some(value) = headers.get(&name) {
            output.insert(name, value.clone());
        }
    }
    output
}

fn prepare_responses(
    request: &CanonicalProxyRequest,
    candidate: &RouteCandidate,
    mapped_model: Option<&str>,
) -> Result<PreparedUpstreamRequest, ProxyFailure> {
    if matches!(
        candidate.upstream_api_format,
        UpstreamApiFormat::OpenAiChatCompletions
    ) {
        if request.stream {
            return Err(responses_chat_fallback_failure(
                "Responses-to-Chat fallback does not support streaming",
            ));
        }
        let body = parse_json_body(&request.body)?;
        let normalized = normalize_for_chat(&body).map_err(|error| {
            responses_chat_fallback_failure(&responses_fallback_error_message(&error))
        })?;
        return Ok(PreparedUpstreamRequest {
            method: Method::POST,
            path: "/v1/chat/completions".to_string(),
            headers: upstream_headers(&request.forwarded_headers, Some("application/json")),
            body: rewrite_json_value_model(normalized, mapped_model)?,
            response_mode: ResponseMode::BufferedChatToResponses,
        });
    }

    Ok(PreparedUpstreamRequest {
        method: Method::POST,
        path: upstream_responses_path(&candidate.upstream_api_format).to_string(),
        headers: upstream_headers(
            &request.forwarded_headers,
            Some(if request.stream {
                "text/event-stream"
            } else {
                "application/json"
            }),
        ),
        body: rewrite_json_model(&request.body, mapped_model)?,
        response_mode: if request.stream {
            ResponseMode::StreamPassthrough
        } else {
            ResponseMode::BufferedJson
        },
    })
}

fn upstream_headers(forwarded: &HeaderMap, accept: Option<&'static str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for name in [
        header::CONTENT_TYPE,
        http::HeaderName::from_static("openai-organization"),
        http::HeaderName::from_static("openai-project"),
        http::HeaderName::from_static("openai-beta"),
        http::HeaderName::from_static("idempotency-key"),
        header::USER_AGENT,
    ] {
        if let Some(value) = forwarded.get(&name) {
            headers.insert(name, value.clone());
        }
    }
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    if let Some(accept) = accept {
        headers.insert(header::ACCEPT, HeaderValue::from_static(accept));
    } else if let Some(value) = forwarded.get(header::ACCEPT) {
        headers.insert(header::ACCEPT, value.clone());
    }
    headers
}

fn rewrite_json_model(body: &Bytes, mapped_model: Option<&str>) -> Result<Bytes, ProxyFailure> {
    let value = parse_json_body(body)?;
    rewrite_json_value_model(value, mapped_model)
}

fn rewrite_json_value_model(
    mut value: Value,
    mapped_model: Option<&str>,
) -> Result<Bytes, ProxyFailure> {
    if let Some(mapped_model) = mapped_model {
        let Some(object) = value.as_object_mut() else {
            return Err(invalid_body_failure("request body must be a JSON object"));
        };
        object.insert("model".to_string(), Value::String(mapped_model.to_string()));
    }
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| invalid_body_failure(format!("serialize upstream body failed: {error}")))
}

fn parse_json_body(body: &Bytes) -> Result<Value, ProxyFailure> {
    serde_json::from_slice(body)
        .map_err(|error| invalid_body_failure(format!("request body must be valid JSON: {error}")))
}

fn invalid_body_failure(message: impl Into<String>) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::RequestBodyInvalid,
        FailureSource::Local,
        RetryClass::Never,
        StatusCode::BAD_REQUEST,
        message,
    )
}

fn responses_chat_fallback_failure(message: impl Into<String>) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::ResponsesChatFallbackIncompatible,
        FailureSource::Routing,
        RetryClass::Never,
        StatusCode::BAD_REQUEST,
        message,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{atomic::AtomicU32, Arc};

    use bytes::Bytes;
    use http::{header, HeaderMap, HeaderValue, Method};
    use serde_json::Value;
    use tokio::sync::Semaphore;

    use crate::{
        models::{proxy::UpstreamApiFormat, routing::RouteEndpointKind},
        services::proxy::{
            error::ProxyFailureCode,
            limits::{BodyBudget, RequestLease},
            request::{CanonicalProxyRequest, RequestRequirements},
            RouteCandidate,
        },
    };

    use super::{EndpointAdapter, ResponseMode};

    #[tokio::test]
    async fn endpoint_adapters_prepare_exact_paths_models_and_headers() {
        let request = canonical_request(
            RouteEndpointKind::Responses,
            br#"{"model":"client-model","input":"hi"}"#,
            false,
            forwarded_headers([
                ("accept", "text/event-stream"),
                ("openai-project", "proj-1"),
            ]),
        )
        .await;
        let candidate = candidate(UpstreamApiFormat::OpenAiResponses, "upstream-model");

        let prepared = EndpointAdapter::Responses
            .prepare(&request, &candidate, Some("upstream-model"))
            .expect("prepared");

        assert_eq!(prepared.method, Method::POST);
        assert_eq!(prepared.path, "/v1/responses");
        assert_eq!(
            serde_json::from_slice::<Value>(&prepared.body).unwrap()["model"],
            "upstream-model"
        );
        assert_eq!(
            prepared.headers.get(header::ACCEPT).unwrap(),
            "application/json"
        );
        assert_eq!(prepared.headers.get("openai-project").unwrap(), "proj-1");
        assert!(!prepared.headers.contains_key(header::AUTHORIZATION));
        assert_eq!(prepared.response_mode, ResponseMode::BufferedJson);
    }

    #[tokio::test]
    async fn endpoint_adapters_reject_unsupported_streaming_chat_bridge() {
        let request = canonical_request(
            RouteEndpointKind::Responses,
            br#"{"model":"gpt-test","input":"hi","stream":true}"#,
            true,
            HeaderMap::new(),
        )
        .await;
        let candidate = candidate(UpstreamApiFormat::OpenAiChatCompletions, "gpt-test");

        let failure = EndpointAdapter::Responses
            .prepare(&request, &candidate, Some("gpt-test"))
            .unwrap_err();

        assert_eq!(
            failure.code,
            ProxyFailureCode::ResponsesChatFallbackIncompatible
        );
    }

    #[tokio::test]
    async fn endpoint_adapters_cover_models_embeddings_chat_and_buffered_responses_bridge() {
        let models = canonical_request(
            RouteEndpointKind::Models,
            b"",
            false,
            forwarded_headers([("accept", "application/json")]),
        )
        .await;
        let embeddings = canonical_request(
            RouteEndpointKind::Embeddings,
            br#"{"model":"client-model","input":"hi"}"#,
            false,
            HeaderMap::new(),
        )
        .await;
        let chat = canonical_request(
            RouteEndpointKind::ChatCompletions,
            br#"{"model":"client-model","messages":[{"role":"user","content":"hi"}],"stream":true}"#,
            true,
            HeaderMap::new(),
        )
        .await;
        let responses = canonical_request(
            RouteEndpointKind::Responses,
            br#"{"model":"client-model","input":"hi"}"#,
            false,
            HeaderMap::new(),
        )
        .await;
        let direct = candidate(UpstreamApiFormat::Auto, "upstream-model");
        let chat_bridge = candidate(UpstreamApiFormat::OpenAiChatCompletions, "bridge-model");

        assert_eq!(
            EndpointAdapter::Models
                .prepare(&models, &direct, None)
                .unwrap()
                .path,
            "/v1/models"
        );
        let embeddings = EndpointAdapter::Embeddings
            .prepare(&embeddings, &direct, Some("upstream-model"))
            .unwrap();
        assert_eq!(embeddings.path, "/v1/embeddings");
        assert_eq!(
            serde_json::from_slice::<Value>(&embeddings.body).unwrap()["model"],
            "upstream-model"
        );
        let chat = EndpointAdapter::ChatCompletions
            .prepare(&chat, &direct, Some("upstream-model"))
            .unwrap();
        assert_eq!(chat.path, "/v1/chat/completions");
        assert_eq!(chat.response_mode, ResponseMode::StreamPassthrough);
        let bridged = EndpointAdapter::Responses
            .prepare(&responses, &chat_bridge, Some("bridge-model"))
            .unwrap();
        assert_eq!(bridged.path, "/v1/chat/completions");
        assert_eq!(
            serde_json::from_slice::<Value>(&bridged.body).unwrap()["messages"][0]["content"],
            "hi"
        );
        assert_eq!(bridged.response_mode, ResponseMode::BufferedChatToResponses);
    }

    #[test]
    fn endpoint_adapters_filter_upstream_response_headers() {
        let input = HeaderMap::from_iter([
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
            (header::SET_COOKIE, HeaderValue::from_static("secret=1")),
            (header::CONNECTION, HeaderValue::from_static("keep-alive")),
            (
                http::HeaderName::from_static("x-request-id"),
                HeaderValue::from_static("abc"),
            ),
        ]);

        let filtered = super::response_headers_for_downstream(&input);

        assert_eq!(
            filtered.get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(filtered.get("x-request-id").unwrap(), "abc");
        assert!(!filtered.contains_key(header::SET_COOKIE));
        assert!(!filtered.contains_key(header::CONNECTION));
    }

    async fn canonical_request(
        endpoint: RouteEndpointKind,
        body: &'static [u8],
        stream: bool,
        forwarded_headers: HeaderMap,
    ) -> CanonicalProxyRequest {
        let budget = BodyBudget::new(1024 * 1024);
        let body_budget = budget.acquire(body.len()).await.expect("budget");
        let permit = Arc::new(Semaphore::new(1))
            .try_acquire_owned()
            .expect("request permit");
        CanonicalProxyRequest::new(
            "req-test".to_string(),
            endpoint_path(&endpoint).to_string(),
            endpoint,
            Some("client-model".to_string()),
            stream,
            RequestRequirements::default(),
            Bytes::from_static(body),
            forwarded_headers,
            None,
            None,
            None,
            body_budget,
            RequestLease::new(permit, Arc::new(AtomicU32::new(0))),
        )
    }

    fn endpoint_path(endpoint: &RouteEndpointKind) -> &'static str {
        match endpoint {
            RouteEndpointKind::Models => "/v1/models",
            RouteEndpointKind::ChatCompletions => "/v1/chat/completions",
            RouteEndpointKind::Responses => "/v1/responses",
            RouteEndpointKind::Embeddings => "/v1/embeddings",
        }
    }

    fn forwarded_headers(
        headers: impl IntoIterator<Item = (&'static str, &'static str)>,
    ) -> HeaderMap {
        let mut output = HeaderMap::new();
        for (name, value) in headers {
            output.insert(
                http::HeaderName::from_static(name),
                HeaderValue::from_static(value),
            );
        }
        output.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer local"),
        );
        output
    }

    fn candidate(format: UpstreamApiFormat, mapped_model: &str) -> RouteCandidate {
        RouteCandidate {
            station_key_id: format!("key-{mapped_model}"),
            station_id: format!("station-{mapped_model}"),
            station_endpoint_revision: 1,
            upstream_base_url: "https://upstream.example/v1".to_string(),
            api_key: "sk-upstream".to_string(),
            collector_proxy_mode: "direct".to_string(),
            collector_proxy_url: None,
            upstream_api_format: format,
            priority: 0,
            max_concurrency: 0,
            load_factor: None,
            schedulable: true,
        }
    }
}
