use crate::{
    models::monitoring::{ClientProfileId, ProtocolKind},
    services::monitoring::profiles::{header, shape, ClientProfileDefinition},
};

pub fn claude_code_compat_v1() -> ClientProfileDefinition {
    ClientProfileDefinition {
        id: ClientProfileId::ClaudeCodeCompat,
        version: 1,
        enabled: true,
        supported_protocols: vec![ProtocolKind::AnthropicMessages],
        request: shape(
            "/v1/messages",
            vec![
                header("accept", "application/json"),
                header("anthropic-version", "2023-06-01"),
                header("content-type", "application/json"),
                header("user-agent", "relay-pool-claude-code-compat/1"),
            ],
            &["max_tokens", "stream"],
        ),
    }
}
