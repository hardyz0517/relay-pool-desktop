use crate::{
    models::monitoring::{ClientProfileId, ProtocolKind},
    services::monitoring::profiles::{
        header, request_value_header, shape, ClientProfileDefinition, ProfileAuthScheme,
        RequestValueKind,
    },
};

pub fn claude_code_compat_v2() -> ClientProfileDefinition {
    ClientProfileDefinition {
        id: ClientProfileId::ClaudeCodeCompat,
        version: 2,
        enabled: true,
        supported_protocols: vec![ProtocolKind::AnthropicMessages],
        auth: ProfileAuthScheme::BearerAuthorization,
        request: shape(
            "/v1/messages",
            vec![
                header("accept", "application/json"),
                header("anthropic-beta", "claude-code-20250219"),
                header("anthropic-version", "2023-06-01"),
                header("content-type", "application/json"),
                header("user-agent", "claude-cli/2.1.220 (external, cli)"),
                header("x-app", "cli"),
                request_value_header("x-claude-code-session-id", RequestValueKind::SessionId),
                request_value_header("x-client-request-id", RequestValueKind::RequestId),
            ],
            &[
                "max_tokens",
                "metadata.user_id",
                "system",
                "messages",
                "tools",
                "stream",
            ],
        ),
    }
}
