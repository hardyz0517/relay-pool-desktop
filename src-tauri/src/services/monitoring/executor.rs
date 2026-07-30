use std::time::Instant;

use http::header;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::{
    models::monitoring::{FailureKind, ProbeOutcome, ProtocolKind, SemanticConfidence},
    outbound::SecretHeaderValue,
    services::monitoring::{
        adapters::{
            anthropic_messages::AnthropicMessagesAdapter, contract::ProtocolAdapter,
            gemini_native::GeminiNativeAdapter, generic_openai::GenericOpenAiAdapter,
            openai_chat::OpenAiChatAdapter, openai_responses::OpenAiResponsesAdapter,
            xai_grok::XaiGrokAdapter,
        },
        challenge::ChallengeValidator,
        profiles::{registry::BuiltinProfileRegistry, HeaderValue},
        transport::{
            MonitoringAuthHeader, MonitoringTransport, MonitoringTransportError,
            MonitoringTransportRequest,
        },
    },
};

#[derive(Debug, Clone)]
pub struct ProbeExecutionInput {
    pub station_key_id: String,
    pub endpoint_revision: i64,
    pub protocol_kind: ProtocolKind,
    pub client_profile_id: crate::models::monitoring::ClientProfileId,
    pub model: String,
    pub prompt: String,
    pub validator: ChallengeValidator,
    pub deadline_at: Instant,
    pub stream: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedProbeSecret {
    pub value: String,
    pub endpoint_revision: i64,
}

pub trait ProbeSecretResolver {
    fn resolve_station_key_secret(
        &self,
        station_key_id: &str,
    ) -> Result<ResolvedProbeSecret, FailureKind>;

    fn current_endpoint_revision(&self, station_key_id: &str) -> Result<i64, FailureKind>;
}

#[derive(Debug)]
pub struct ProbeExecutionOutput {
    pub outcome: ProbeOutcome,
    pub failure_kind: Option<FailureKind>,
    pub retryable: bool,
    pub latency_ms: u64,
    pub http_status: Option<u16>,
    pub response_model: Option<String>,
    pub semantic_confidence: SemanticConfidence,
    pub request_profile_hash: String,
    pub response_bytes: usize,
    pub output_bytes: usize,
    pub debug_summary: ProbeExecutionDebugSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeExecutionDebugSummary {
    pub method: String,
    pub relative_path: String,
    pub protocol_kind: ProtocolKind,
    pub request_profile_hash: String,
    pub header_names: Vec<String>,
    pub response_bytes: usize,
    pub output_bytes: usize,
    pub error_summary: Option<String>,
}

pub struct ProbeExecutor<R> {
    transport: MonitoringTransport,
    secret_resolver: R,
    profiles: BuiltinProfileRegistry,
}

impl<R> ProbeExecutor<R>
where
    R: ProbeSecretResolver,
{
    pub fn new(transport: MonitoringTransport, secret_resolver: R) -> Self {
        Self {
            transport,
            secret_resolver,
            profiles: BuiltinProfileRegistry::default(),
        }
    }

    pub async fn execute(
        &self,
        input: ProbeExecutionInput,
        cancellation_token: CancellationToken,
    ) -> ProbeExecutionOutput {
        let started = Instant::now();
        let profile = match self.profiles.get(input.client_profile_id) {
            Some(profile) if profile.supports_protocol(input.protocol_kind) => profile,
            _ => {
                return failure_output(
                    input.protocol_kind,
                    None,
                    FailureKind::NeedsConfiguration,
                    started,
                    String::new(),
                    None,
                )
            }
        };
        let request_profile_hash = profile.profile_hash();
        let secret = match self
            .secret_resolver
            .resolve_station_key_secret(&input.station_key_id)
        {
            Ok(secret) if secret.endpoint_revision == input.endpoint_revision => secret,
            Ok(_) => {
                return failure_output(
                    input.protocol_kind,
                    None,
                    FailureKind::Interrupted,
                    started,
                    request_profile_hash,
                    Some("endpoint_revision_changed_before_send".to_string()),
                )
            }
            Err(failure_kind) => {
                return failure_output(
                    input.protocol_kind,
                    None,
                    failure_kind,
                    started,
                    request_profile_hash,
                    Some("secret_resolve_failed".to_string()),
                )
            }
        };

        let adapter = adapter_for(input.protocol_kind, input.stream);
        let mut descriptor = adapter.request_descriptor();
        descriptor.body = request_body(
            input.protocol_kind,
            &input.model,
            &input.prompt,
            input.stream,
        );
        let public_headers = profile_public_headers(profile);
        let request = MonitoringTransportRequest {
            descriptor,
            public_headers,
            auth_header: Some(MonitoringAuthHeader {
                name: header::AUTHORIZATION.as_str().to_string(),
                value: SecretHeaderValue::new(format!("Bearer {}", secret.value)),
            }),
            request_deadline: input.deadline_at,
        };
        let transport_response = if input.stream {
            self.transport
                .execute_streaming(request, cancellation_token)
                .await
        } else {
            self.transport
                .execute_buffered(request, cancellation_token)
                .await
        };
        let transport_response = match transport_response {
            Ok(response) => response,
            Err(error) => {
                return transport_failure_output(
                    input.protocol_kind,
                    error,
                    started,
                    request_profile_hash,
                )
            }
        };

        match self
            .secret_resolver
            .current_endpoint_revision(&input.station_key_id)
        {
            Ok(revision) if revision == input.endpoint_revision => {}
            _ => {
                return failure_output(
                    input.protocol_kind,
                    Some(transport_response.http_status),
                    FailureKind::Interrupted,
                    started,
                    request_profile_hash,
                    Some("endpoint_revision_changed_before_writeback".to_string()),
                )
            }
        }

        let parsed = adapter.parse_response(
            transport_response.http_status,
            transport_response.content_type.as_deref(),
            &transport_response.body,
            &input.validator,
            Default::default(),
        );
        ProbeExecutionOutput {
            outcome: parsed.outcome,
            failure_kind: parsed.failure_kind,
            retryable: retryable(parsed.failure_kind),
            latency_ms: transport_response.total_latency_ms,
            http_status: parsed.http_status,
            response_model: parsed.model,
            semantic_confidence: SemanticConfidence::ProtocolValidated,
            request_profile_hash: request_profile_hash.clone(),
            response_bytes: parsed.response_bytes,
            output_bytes: parsed.output_bytes,
            debug_summary: ProbeExecutionDebugSummary {
                method: transport_response.evidence.method,
                relative_path: transport_response.evidence.relative_path,
                protocol_kind: input.protocol_kind,
                request_profile_hash,
                header_names: transport_response.evidence.header_names,
                response_bytes: parsed.response_bytes,
                output_bytes: parsed.output_bytes,
                error_summary: parsed.failure_kind.map(|kind| format!("{kind:?}")),
            },
        }
    }
}

fn adapter_for(protocol_kind: ProtocolKind, stream: bool) -> Box<dyn ProtocolAdapter> {
    match protocol_kind {
        ProtocolKind::OpenAiChat => Box::new(OpenAiChatAdapter::new(stream)),
        ProtocolKind::OpenAiResponses => Box::new(OpenAiResponsesAdapter::new(stream)),
        ProtocolKind::AnthropicMessages => Box::new(AnthropicMessagesAdapter::new(stream)),
        ProtocolKind::GeminiNative => Box::new(GeminiNativeAdapter::new(stream)),
        ProtocolKind::XaiGrok => Box::new(XaiGrokAdapter::new(stream)),
        ProtocolKind::GenericOpenAi => Box::new(GenericOpenAiAdapter::new(stream)),
    }
}

fn request_body(protocol_kind: ProtocolKind, model: &str, prompt: &str, stream: bool) -> Vec<u8> {
    let value = match protocol_kind {
        ProtocolKind::OpenAiResponses => json!({
            "model": model,
            "input": prompt,
            "max_output_tokens": 16,
            "stream": stream
        }),
        ProtocolKind::AnthropicMessages => json!({
            "model": model,
            "max_tokens": 16,
            "stream": stream,
            "messages": [{"role": "user", "content": prompt}]
        }),
        ProtocolKind::GeminiNative => json!({
            "contents": [{"role": "user", "parts": [{"text": prompt}]}]
        }),
        ProtocolKind::OpenAiChat | ProtocolKind::GenericOpenAi | ProtocolKind::XaiGrok => json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 16,
            "stream": stream
        }),
    };
    serde_json::to_vec(&value).expect("monitoring request body serializes")
}

fn profile_public_headers(
    profile: &crate::services::monitoring::profiles::ClientProfileDefinition,
) -> Vec<(String, String)> {
    profile
        .request
        .headers
        .iter()
        .filter_map(|header| match &header.value {
            HeaderValue::Static(value) => Some((header.name.clone(), value.clone())),
            HeaderValue::StableLocalIdentity { scope } => {
                Some((header.name.clone(), format!("relay-pool-desktop:{scope}")))
            }
        })
        .collect()
}

fn retryable(failure_kind: Option<FailureKind>) -> bool {
    matches!(
        failure_kind,
        Some(
            FailureKind::RateLimit
                | FailureKind::ServerError
                | FailureKind::Network
                | FailureKind::Timeout
        )
    )
}

fn transport_failure_output(
    protocol_kind: ProtocolKind,
    error: MonitoringTransportError,
    started: Instant,
    request_profile_hash: String,
) -> ProbeExecutionOutput {
    failure_output(
        protocol_kind,
        None,
        error.failure_kind,
        started,
        request_profile_hash,
        Some(format!("{:?}", error.kind)),
    )
}

fn failure_output(
    protocol_kind: ProtocolKind,
    http_status: Option<u16>,
    failure_kind: FailureKind,
    started: Instant,
    request_profile_hash: String,
    error_summary: Option<String>,
) -> ProbeExecutionOutput {
    ProbeExecutionOutput {
        outcome: ProbeOutcome::Unavailable,
        failure_kind: Some(failure_kind),
        retryable: retryable(Some(failure_kind)),
        latency_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        http_status,
        response_model: None,
        semantic_confidence: SemanticConfidence::ProtocolValidated,
        request_profile_hash: request_profile_hash.clone(),
        response_bytes: 0,
        output_bytes: 0,
        debug_summary: ProbeExecutionDebugSummary {
            method: "POST".to_string(),
            relative_path: String::new(),
            protocol_kind,
            request_profile_hash,
            header_names: Vec::new(),
            response_bytes: 0,
            output_bytes: 0,
            error_summary,
        },
    }
}
