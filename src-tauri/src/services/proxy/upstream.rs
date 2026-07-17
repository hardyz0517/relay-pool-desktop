use std::{collections::HashMap, fmt, sync::Arc};

use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt};
use http::{header, HeaderMap, HeaderValue, StatusCode};
use tokio::sync::RwLock;

use crate::services::station_endpoints::build_api_url;

use super::{
    endpoint_adapter::{PreparedUpstreamRequest, ResponseMode},
    error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
    limits::ProxyServerLimits,
    redact_error_message,
    request::ByteStream,
    RouteCandidate,
};

#[derive(Clone)]
pub(crate) struct UpstreamClientPool {
    direct: Arc<reqwest::Client>,
    proxied: Arc<RwLock<HashMap<ProxyRoute, Arc<reqwest::Client>>>>,
    limits: ProxyServerLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ProxyRoute {
    Direct,
    Http(String),
    Socks(String),
}

pub(crate) enum UpstreamAttempt {
    Buffered {
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
    },
    Stream {
        status: StatusCode,
        headers: HeaderMap,
        chunks: ByteStream,
    },
}

impl fmt::Debug for UpstreamAttempt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Buffered {
                status,
                headers,
                body,
            } => formatter
                .debug_struct("Buffered")
                .field("status", status)
                .field("headers", headers)
                .field("body_len", &body.len())
                .finish(),
            Self::Stream {
                status, headers, ..
            } => formatter
                .debug_struct("Stream")
                .field("status", status)
                .field("headers", headers)
                .finish(),
        }
    }
}

impl UpstreamClientPool {
    pub(crate) fn new(limits: ProxyServerLimits) -> Result<Self, ProxyFailure> {
        Ok(Self {
            direct: Arc::new(build_client(&ProxyRoute::Direct, &limits)?),
            proxied: Arc::new(RwLock::new(HashMap::new())),
            limits,
        })
    }

    pub(crate) async fn client(
        &self,
        route: &ProxyRoute,
    ) -> Result<Arc<reqwest::Client>, ProxyFailure> {
        if matches!(route, ProxyRoute::Direct) {
            return Ok(Arc::clone(&self.direct));
        }

        if let Some(client) = self.proxied.read().await.get(route).cloned() {
            return Ok(client);
        }

        let mut clients = self.proxied.write().await;
        if let Some(client) = clients.get(route).cloned() {
            return Ok(client);
        }
        let client = Arc::new(build_client(route, &self.limits)?);
        clients.insert(route.clone(), Arc::clone(&client));
        Ok(client)
    }

    pub(crate) async fn send(
        &self,
        prepared: PreparedUpstreamRequest,
        candidate: &RouteCandidate,
    ) -> Result<UpstreamAttempt, ProxyFailure> {
        let route = ProxyRoute::from_candidate_parts(
            &candidate.collector_proxy_mode,
            candidate.collector_proxy_url.as_deref(),
        )?;
        let url = build_api_url(&candidate.upstream_base_url, &prepared.path)
            .map_err(internal_proxy_failure)?;
        let client = self.client(&route).await?;
        let method = reqwest::Method::from_bytes(prepared.method.as_str().as_bytes())
            .map_err(|error| internal_proxy_failure(format!("invalid upstream method: {error}")))?;
        let mut request = client.request(method, url);
        for (name, value) in prepared.headers.iter() {
            request = request.header(name.as_str(), value.clone());
        }
        request = request.header(
            header::AUTHORIZATION.as_str(),
            HeaderValue::from_str(&format!("Bearer {}", candidate.api_key)).map_err(|error| {
                internal_proxy_failure(format!("invalid upstream authorization header: {error}"))
            })?,
        );
        if !prepared.body.is_empty() {
            request = request.body(prepared.body.clone());
        }

        let response = request.send().await.map_err(upstream_connect_failure)?;
        let status = response.status();
        let headers = response.headers().clone();
        match prepared.response_mode {
            ResponseMode::StreamPassthrough if status.is_success() => {
                let chunks = response
                    .bytes_stream()
                    .map_err(upstream_stream_failure)
                    .boxed();
                Ok(UpstreamAttempt::Stream {
                    status,
                    headers,
                    chunks,
                })
            }
            ResponseMode::BufferedJson
            | ResponseMode::BufferedChatToResponses
            | ResponseMode::StreamPassthrough => {
                let body = response.bytes().await.map_err(upstream_connect_failure)?;
                Ok(UpstreamAttempt::Buffered {
                    status,
                    headers,
                    body,
                })
            }
        }
    }
}

impl ProxyRoute {
    pub(crate) fn from_candidate_parts(
        mode: &str,
        url: Option<&str>,
    ) -> Result<Self, ProxyFailure> {
        match mode.trim().to_ascii_lowercase().as_str() {
            "" | "direct" | "inherit" => Ok(Self::Direct),
            "http" | "http_proxy" | "https" | "https_proxy" => {
                let url = required_proxy_url(url)?;
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err(invalid_proxy_failure(
                        "HTTP proxy route requires http(s) URL",
                    ));
                }
                Ok(Self::Http(url.to_string()))
            }
            "socks" | "socks5" | "socks_proxy" => {
                let url = required_proxy_url(url)?;
                if !(url.starts_with("socks5://") || url.starts_with("socks5h://")) {
                    return Err(invalid_proxy_failure(
                        "SOCKS proxy route requires socks5 URL",
                    ));
                }
                Ok(Self::Socks(url.to_string()))
            }
            other => Err(invalid_proxy_failure(format!(
                "unsupported upstream proxy mode: {other}"
            ))),
        }
    }
}

fn build_client(
    route: &ProxyRoute,
    limits: &ProxyServerLimits,
) -> Result<reqwest::Client, ProxyFailure> {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(limits.upstream_connect_timeout)
        .pool_idle_timeout(Some(limits.stream_idle_timeout));
    match route {
        ProxyRoute::Direct => {}
        ProxyRoute::Http(url) | ProxyRoute::Socks(url) => {
            let proxy = reqwest::Proxy::all(url).map_err(|error| {
                invalid_proxy_failure(format!("invalid upstream proxy URL: {error}"))
            })?;
            builder = builder.proxy(proxy);
        }
    }
    builder
        .build()
        .map_err(|error| internal_proxy_failure(format!("build upstream client failed: {error}")))
}

fn required_proxy_url(url: Option<&str>) -> Result<&str, ProxyFailure> {
    let Some(url) = url.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(invalid_proxy_failure("proxy URL is required"));
    };
    Ok(url)
}

fn upstream_connect_failure(error: reqwest::Error) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::UpstreamConnectFailed,
        FailureSource::Upstream,
        RetryClass::BeforeOutput,
        StatusCode::BAD_GATEWAY,
        redact_error_message(&format!("upstream request failed: {error}")),
    )
}

fn upstream_stream_failure(error: reqwest::Error) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::UpstreamStreamFailed,
        FailureSource::Upstream,
        RetryClass::AfterCommitStop,
        StatusCode::BAD_GATEWAY,
        redact_error_message(&format!("upstream stream failed: {error}")),
    )
}

fn invalid_proxy_failure(message: impl Into<String>) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::InternalProxyError,
        FailureSource::Internal,
        RetryClass::Never,
        StatusCode::BAD_GATEWAY,
        message,
    )
}

fn internal_proxy_failure(message: impl Into<String>) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::InternalProxyError,
        FailureSource::Internal,
        RetryClass::Never,
        StatusCode::INTERNAL_SERVER_ERROR,
        message,
    )
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use bytes::Bytes;
    use http::{HeaderMap, Method, StatusCode};

    use crate::{
        models::proxy::UpstreamApiFormat,
        services::proxy::{
            endpoint_adapter::{PreparedUpstreamRequest, ResponseMode},
            error::ProxyFailureCode,
            limits::ProxyServerLimits,
            test_support::{LoopbackUpstream, ScriptedResponse},
            RouteCandidate,
        },
    };

    use super::{ProxyRoute, UpstreamAttempt, UpstreamClientPool};

    #[tokio::test]
    async fn upstream_transport_reuses_clients_and_never_follows_redirects() {
        let pool = UpstreamClientPool::new(test_limits()).expect("pool");
        assert!(Arc::ptr_eq(
            &pool
                .client(&ProxyRoute::Direct)
                .await
                .expect("direct client"),
            &pool
                .client(&ProxyRoute::Direct)
                .await
                .expect("direct client")
        ));
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Redirect {
            location: "https://other.example/secret".to_string(),
        }]);

        let outcome = pool
            .send(
                prepared_request("/v1/models"),
                &test_candidate(&upstream.base_url),
            )
            .await
            .expect("upstream outcome");

        assert_eq!(outcome.status(), StatusCode::FOUND);
    }

    #[tokio::test]
    async fn upstream_transport_classifies_connect_timeout_and_http_status() {
        let pool = UpstreamClientPool::new(short_limits()).expect("pool");
        let connect = pool
            .send(
                prepared_request("/v1/models"),
                &test_candidate("http://127.0.0.1:9"),
            )
            .await
            .expect_err("connect failure");
        assert_eq!(connect.code, ProxyFailureCode::UpstreamConnectFailed);

        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Status {
            status: 429,
            reason: "Too Many Requests",
        }]);
        let status = pool
            .send(
                prepared_request("/v1/models"),
                &test_candidate(&upstream.base_url),
            )
            .await
            .expect("http status response");

        assert_eq!(status.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn upstream_transport_validates_http_and_socks_proxy_urls() {
        assert_eq!(
            ProxyRoute::from_candidate_parts("direct", None).expect("direct"),
            ProxyRoute::Direct
        );
        assert_eq!(
            ProxyRoute::from_candidate_parts("http", Some("http://127.0.0.1:8888"))
                .expect("http proxy"),
            ProxyRoute::Http("http://127.0.0.1:8888".to_string())
        );
        assert_eq!(
            ProxyRoute::from_candidate_parts("socks", Some("socks5://127.0.0.1:1080"))
                .expect("socks proxy"),
            ProxyRoute::Socks("socks5://127.0.0.1:1080".to_string())
        );
        assert!(ProxyRoute::from_candidate_parts("http", Some("socks5://127.0.0.1:1080")).is_err());
        assert!(ProxyRoute::from_candidate_parts("socks", Some("http://127.0.0.1:8888")).is_err());
    }

    fn prepared_request(path: &str) -> PreparedUpstreamRequest {
        PreparedUpstreamRequest {
            method: Method::GET,
            path: path.to_string(),
            headers: HeaderMap::new(),
            body: Bytes::new(),
            response_mode: ResponseMode::BufferedJson,
        }
    }

    fn test_candidate(upstream_base_url: &str) -> RouteCandidate {
        RouteCandidate {
            station_key_id: "key-test".to_string(),
            station_id: "station-test".to_string(),
            station_endpoint_revision: 1,
            upstream_base_url: upstream_base_url.to_string(),
            api_key: "sk-upstream-test".to_string(),
            collector_proxy_mode: "direct".to_string(),
            collector_proxy_url: None,
            upstream_api_format: UpstreamApiFormat::Auto,
            priority: 0,
            max_concurrency: 0,
            load_factor: None,
            schedulable: true,
        }
    }

    fn test_limits() -> ProxyServerLimits {
        ProxyServerLimits::default()
    }

    fn short_limits() -> ProxyServerLimits {
        ProxyServerLimits {
            upstream_connect_timeout: Duration::from_millis(50),
            ..ProxyServerLimits::default()
        }
    }

    impl UpstreamAttempt {
        fn status(&self) -> StatusCode {
            match self {
                Self::Buffered { status, .. } | Self::Stream { status, .. } => *status,
            }
        }
    }
}
