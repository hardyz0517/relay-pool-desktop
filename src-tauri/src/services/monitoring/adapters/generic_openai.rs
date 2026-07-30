use crate::{
    models::monitoring::ProtocolKind,
    services::monitoring::{
        adapters::{
            contract::{ParsedProbeResponse, ProtocolAdapter, RequestDescriptor, ResponseLimits},
            openai_chat::parse_chat_response,
        },
        challenge::ChallengeValidator,
    },
};

#[derive(Debug, Clone)]
pub struct GenericOpenAiAdapter {
    stream: bool,
}

impl GenericOpenAiAdapter {
    pub fn new(stream: bool) -> Self {
        Self { stream }
    }
}

impl ProtocolAdapter for GenericOpenAiAdapter {
    fn protocol_kind(&self) -> ProtocolKind {
        ProtocolKind::GenericOpenAi
    }

    fn request_descriptor(&self) -> RequestDescriptor {
        RequestDescriptor {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            body: Vec::new(),
            stream: self.stream,
        }
    }

    fn parse_response(
        &self,
        http_status: u16,
        content_type: Option<&str>,
        body: &[u8],
        validator: &ChallengeValidator,
        limits: ResponseLimits,
    ) -> ParsedProbeResponse {
        parse_chat_response(
            ProtocolKind::GenericOpenAi,
            self.stream,
            http_status,
            content_type,
            body,
            validator,
            limits,
        )
    }
}
