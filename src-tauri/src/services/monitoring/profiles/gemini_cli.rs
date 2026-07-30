use crate::{
    models::monitoring::{ClientProfileId, ProtocolKind},
    services::monitoring::profiles::{
        header, model_template_header, shape, ClientProfileDefinition, ProfileAuthScheme,
    },
};

pub fn gemini_cli_compat_v2() -> ClientProfileDefinition {
    ClientProfileDefinition {
        id: ClientProfileId::GeminiCliCompat,
        version: 2,
        enabled: true,
        supported_protocols: vec![ProtocolKind::GeminiNative],
        auth: ProfileAuthScheme::ApiKeyHeader {
            name: "x-goog-api-key".to_string(),
        },
        request: shape(
            "/v1beta/models/{model}:generateContent",
            vec![
                header("accept", "application/json"),
                header("content-type", "application/json"),
                model_template_header("user-agent", "GeminiCLI/0.53.0/{model} (win32; x64; cli)"),
            ],
            &[
                "contents",
                "generationConfig.temperature",
                "generationConfig.maxOutputTokens",
                "generationConfig.thinkingConfig.thinkingBudget",
            ],
        ),
    }
}
