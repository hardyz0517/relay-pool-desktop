use crate::{
    models::monitoring::{ClientProfileId, ProtocolKind},
    services::monitoring::profiles::{header, shape, ClientProfileDefinition},
};

pub fn gemini_cli_compat_v1() -> ClientProfileDefinition {
    ClientProfileDefinition {
        id: ClientProfileId::GeminiCliCompat,
        version: 1,
        enabled: true,
        supported_protocols: vec![ProtocolKind::GeminiNative],
        request: shape(
            "/v1beta/models/{model}:generateContent",
            vec![
                header("accept", "application/json"),
                header("content-type", "application/json"),
                header("user-agent", "relay-pool-gemini-cli-compat/1"),
                header("x-goog-api-client", "relay-pool-desktop/1"),
            ],
            &["generationConfig.maxOutputTokens", "stream"],
        ),
    }
}
