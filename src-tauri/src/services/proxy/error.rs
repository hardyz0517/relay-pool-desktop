use http::StatusCode;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureSource {
    Local,
    Routing,
    Upstream,
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "reserved by the downstream-disconnect failure contract"
        )
    )]
    Downstream,
    Internal,
}

impl FailureSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Routing => "routing",
            Self::Upstream => "upstream",
            Self::Downstream => "downstream",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    Never,
    BeforeOutput,
    AfterCommitStop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyFailureCode {
    LocalProxyBusy,
    LocalProxyMemoryBusy,
    #[expect(
        dead_code,
        reason = "reserved by the local proxy request-header failure contract"
    )]
    RequestHeaderTimeout,
    #[expect(
        dead_code,
        reason = "reserved by the local proxy request-header failure contract"
    )]
    RequestHeaderTooLarge,
    RequestBodyTimeout,
    RequestBodyTooLarge,
    RequestBodyInvalid,
    LocalAuthMissing,
    LocalAuthInvalid,
    RouteNoCandidate,
    RouteWaitTimeout,
    UpstreamConnectFailed,
    UpstreamFirstByteTimeout,
    UpstreamHttpError,
    UpstreamStreamFailed,
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "reserved by the downstream-disconnect failure contract"
        )
    )]
    DownstreamDisconnected,
    ResponsesChatFallbackIncompatible,
    #[expect(
        dead_code,
        reason = "reserved by the application-update admission contract"
    )]
    ApplicationUpdateInProgress,
    InternalProxyError,
}

impl ProxyFailureCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalProxyBusy => "local_proxy_busy",
            Self::LocalProxyMemoryBusy => "local_proxy_memory_busy",
            Self::RequestHeaderTimeout => "request_header_timeout",
            Self::RequestHeaderTooLarge => "request_header_too_large",
            Self::RequestBodyTimeout => "request_body_timeout",
            Self::RequestBodyTooLarge => "request_body_too_large",
            Self::RequestBodyInvalid => "request_body_invalid",
            Self::LocalAuthMissing => "local_auth_missing",
            Self::LocalAuthInvalid => "local_auth_invalid",
            Self::RouteNoCandidate => "route_no_candidate",
            Self::RouteWaitTimeout => "route_wait_timeout",
            Self::UpstreamConnectFailed => "upstream_connect_failed",
            Self::UpstreamFirstByteTimeout => "upstream_first_byte_timeout",
            Self::UpstreamHttpError => "upstream_http_error",
            Self::UpstreamStreamFailed => "upstream_stream_failed",
            Self::DownstreamDisconnected => "downstream_disconnected",
            Self::ResponsesChatFallbackIncompatible => "responses_chat_fallback_incompatible",
            Self::ApplicationUpdateInProgress => "application_update_in_progress",
            Self::InternalProxyError => "internal_proxy_error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyFailure {
    pub code: ProxyFailureCode,
    pub source: FailureSource,
    pub retry_class: RetryClass,
    pub http_status: StatusCode,
    pub public_message: String,
    pub internal_detail: Option<String>,
    context: Option<Box<ProxyFailureContext>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProxyFailureContext {
    pub candidate_id: Option<String>,
    pub candidate_station_id: Option<String>,
    pub candidate_upstream_base_url: Option<String>,
    pub attempt_count: Option<i64>,
    pub route_policy: Option<String>,
}

impl ProxyFailure {
    pub fn new(
        code: ProxyFailureCode,
        source: FailureSource,
        retry_class: RetryClass,
        http_status: StatusCode,
        public_message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            source,
            retry_class,
            http_status,
            public_message: public_message.into(),
            internal_detail: None,
            context: None,
        }
    }

    pub(crate) fn context_mut(&mut self) -> &mut ProxyFailureContext {
        self.context
            .get_or_insert_with(|| Box::new(ProxyFailureContext::default()))
    }

    pub(crate) fn candidate_id(&self) -> Option<&str> {
        self.context
            .as_deref()
            .and_then(|context| context.candidate_id.as_deref())
    }

    pub(crate) fn candidate_station_id(&self) -> Option<&str> {
        self.context
            .as_deref()
            .and_then(|context| context.candidate_station_id.as_deref())
    }

    pub(crate) fn candidate_upstream_base_url(&self) -> Option<&str> {
        self.context
            .as_deref()
            .and_then(|context| context.candidate_upstream_base_url.as_deref())
    }

    pub(crate) fn attempt_count(&self) -> Option<i64> {
        self.context
            .as_deref()
            .and_then(|context| context.attempt_count)
    }

    pub(crate) fn route_policy(&self) -> Option<&str> {
        self.context
            .as_deref()
            .and_then(|context| context.route_policy.as_deref())
    }

    pub fn into_response(self) -> (StatusCode, Value) {
        let message = crate::services::secrets::mask::redact_text(&self.public_message);
        (
            self.http_status,
            serde_json::json!({
                "error": {
                    "message": message,
                    "type": "relay_pool_error",
                    "param": Value::Null,
                    "code": self.code.as_str(),
                }
            }),
        )
    }
}
