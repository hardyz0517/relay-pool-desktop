use crate::{
    models::monitoring::{ClientProfileId, ProtocolKind},
    services::monitoring::profiles::{header, shape, ClientProfileDefinition},
};

pub fn codex_cli_compat_v1() -> ClientProfileDefinition {
    ClientProfileDefinition {
        id: ClientProfileId::CodexCliCompat,
        version: 1,
        enabled: true,
        supported_protocols: vec![
            ProtocolKind::OpenAiChat,
            ProtocolKind::OpenAiResponses,
            ProtocolKind::GenericOpenAi,
        ],
        request: shape(
            "{adapter_path}",
            vec![
                header("accept", "application/json"),
                header("content-type", "application/json"),
                header("openai-beta", "responses=v1"),
                header("user-agent", "relay-pool-codex-cli-compat/1"),
                header("x-stainless-arch", "unknown"),
                header("x-stainless-lang", "rust"),
                header("x-stainless-os", "unknown"),
            ],
            &["max_tokens", "stream"],
        ),
    }
}
