use crate::{
    models::monitoring::{ClientProfileId, ProtocolKind},
    services::monitoring::profiles::{header, shape, ClientProfileDefinition, ProfileAuthScheme},
};

pub fn codex_cli_compat_v2() -> ClientProfileDefinition {
    ClientProfileDefinition {
        id: ClientProfileId::CodexCliCompat,
        version: 2,
        enabled: true,
        supported_protocols: vec![ProtocolKind::OpenAiResponses],
        auth: ProfileAuthScheme::BearerAuthorization,
        request: shape(
            "/v1/responses",
            vec![
                header("accept", "application/json"),
                header("content-type", "application/json"),
                header("openai-beta", "responses=experimental"),
                header("user-agent", "codex_cli_rs/0.146.0"),
            ],
            &[
                "instructions",
                "input",
                "tools",
                "tool_choice",
                "parallel_tool_calls",
                "reasoning.effort",
                "reasoning.summary",
                "store",
                "stream",
            ],
        ),
    }
}
