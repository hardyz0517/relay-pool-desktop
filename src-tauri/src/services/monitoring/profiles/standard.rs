use crate::{
    models::monitoring::{ClientProfileId, ProtocolKind},
    services::monitoring::profiles::{header, shape, ClientProfileDefinition, ProfileAuthScheme},
};

pub fn standard_api_v1() -> ClientProfileDefinition {
    ClientProfileDefinition {
        id: ClientProfileId::StandardApi,
        version: 1,
        enabled: true,
        supported_protocols: vec![
            ProtocolKind::OpenAiChat,
            ProtocolKind::OpenAiResponses,
            ProtocolKind::AnthropicMessages,
            ProtocolKind::GeminiNative,
            ProtocolKind::XaiGrok,
            ProtocolKind::GenericOpenAi,
        ],
        auth: ProfileAuthScheme::BearerAuthorization,
        request: shape(
            "{adapter_path}",
            vec![
                header("accept", "application/json"),
                header("content-type", "application/json"),
            ],
            &["max_tokens", "stream"],
        ),
    }
}
