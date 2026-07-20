use bytes::Bytes;
use futures_util::stream::BoxStream;
use http::{HeaderMap, StatusCode};

use crate::models::routing::{RouteEndpointKind, RoutingGroupFilter};

use super::{
    lifecycle::{request::RequestContextSnapshot, writer::RequestTerminalReservation},
    limits::{BodyBudgetLease, RequestLease},
};

pub type ByteStream =
    BoxStream<'static, Result<Bytes, crate::services::proxy::error::ProxyFailure>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestRequirements {
    pub uses_tools: bool,
    pub uses_vision: bool,
    pub uses_reasoning: bool,
    pub routing_group_filter: RoutingGroupFilter,
}

impl Default for RequestRequirements {
    fn default() -> Self {
        Self {
            uses_tools: false,
            uses_vision: false,
            uses_reasoning: false,
            routing_group_filter: RoutingGroupFilter::default(),
        }
    }
}

pub struct CanonicalProxyRequest {
    pub request_id: String,
    pub local_path: String,
    pub endpoint: RouteEndpointKind,
    pub model: Option<String>,
    pub stream: bool,
    pub reasoning_effort: Option<String>,
    pub requirements: RequestRequirements,
    pub body: Bytes,
    pub forwarded_headers: HeaderMap,
    pub idempotency_key: Option<String>,
    pub session_hash: Option<String>,
    pub previous_response_id: Option<String>,
    lifecycle_admission: Option<RequestLifecycleAdmission>,
    _body_budget: BodyBudgetLease,
    request_lease: Option<RequestLease>,
}

pub(crate) struct RequestLifecycleAdmission {
    pub(crate) context: RequestContextSnapshot,
    pub(crate) terminal: RequestTerminalReservation,
}

impl CanonicalProxyRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: String,
        local_path: String,
        endpoint: RouteEndpointKind,
        model: Option<String>,
        stream: bool,
        reasoning_effort: Option<String>,
        requirements: RequestRequirements,
        body: Bytes,
        forwarded_headers: HeaderMap,
        idempotency_key: Option<String>,
        session_hash: Option<String>,
        previous_response_id: Option<String>,
        lifecycle_admission: Option<RequestLifecycleAdmission>,
        body_budget: BodyBudgetLease,
        request_lease: RequestLease,
    ) -> Self {
        Self {
            request_id,
            local_path,
            endpoint,
            model,
            stream,
            reasoning_effort,
            requirements,
            body,
            forwarded_headers,
            idempotency_key,
            session_hash,
            previous_response_id,
            lifecycle_admission,
            _body_budget: body_budget,
            request_lease: Some(request_lease),
        }
    }

    pub(crate) fn take_lifecycle_admission(&mut self) -> Option<RequestLifecycleAdmission> {
        self.lifecycle_admission.take()
    }

    pub(crate) fn take_request_lease(&mut self) -> Option<RequestLease> {
        self.request_lease.take()
    }
}

pub enum ProxyResponsePayload {
    Buffered(Bytes),
    Stream(ByteStream),
}

pub struct ProxyHttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub payload: ProxyResponsePayload,
}
