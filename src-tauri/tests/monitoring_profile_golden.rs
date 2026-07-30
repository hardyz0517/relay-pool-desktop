mod monitoring {
    #![allow(dead_code, unused_imports)]

    #[path = "../../src/models/monitoring/definition.rs"]
    pub mod definition;
    #[path = "../../src/models/monitoring/outcome.rs"]
    pub mod outcome;
    #[path = "../../src/models/monitoring/policy.rs"]
    pub mod policy;

    pub use definition::ClientProfileId;
    pub use outcome::ProtocolKind;
}

mod models {
    pub mod monitoring {
        pub use crate::monitoring::{ClientProfileId, ProtocolKind};
    }
}

#[path = "../src/services/monitoring/auth.rs"]
pub mod auth;
#[path = "../src/services/monitoring/profiles/claude_code.rs"]
pub mod claude_code;
#[path = "../src/services/monitoring/profiles/codex_cli.rs"]
pub mod codex_cli;
#[path = "../src/services/monitoring/profiles/gemini_cli.rs"]
pub mod gemini_cli;
#[path = "../src/services/monitoring/profiles/mod.rs"]
pub mod profiles_mod;
#[path = "../src/services/monitoring/profiles/registry.rs"]
pub mod registry;
#[path = "../src/services/monitoring/profiles/standard.rs"]
pub mod standard;

mod services {
    pub mod monitoring {
        pub use crate::auth;

        pub mod profiles {
            pub use crate::claude_code;
            pub use crate::codex_cli;
            pub use crate::gemini_cli;
            pub use crate::profiles_mod::*;
            pub use crate::registry;
            pub use crate::standard;
        }
    }
}

use models::monitoring::{ClientProfileId, ProtocolKind};
use services::monitoring::{
    auth::AuthBoundaryViolation,
    profiles::{
        registry::BuiltinProfileRegistry, ClientProfileDefinition, ClientProfileHeader,
        ClientProfileRequestShape, HeaderValue, ProfileAuthScheme,
    },
};

#[test]
fn profile_golden_definitions_match_versioned_request_images() {
    let registry = BuiltinProfileRegistry::default();
    for (profile_id, expected_json) in [
        (
            ClientProfileId::StandardApi,
            include_str!("fixtures/monitoring/profiles/standard_api_openai_chat.v1.json"),
        ),
        (
            ClientProfileId::CodexCliCompat,
            include_str!("fixtures/monitoring/profiles/codex_cli_compat.v2.json"),
        ),
        (
            ClientProfileId::ClaudeCodeCompat,
            include_str!("fixtures/monitoring/profiles/claude_code_compat.v2.json"),
        ),
        (
            ClientProfileId::GeminiCliCompat,
            include_str!("fixtures/monitoring/profiles/gemini_cli_compat.v2.json"),
        ),
    ] {
        let actual = registry
            .get(profile_id)
            .expect("builtin profile")
            .golden_summary();
        let expected: serde_json::Value =
            serde_json::from_str(expected_json).expect("expected golden json");
        assert_eq!(serde_json::to_value(actual).expect("actual json"), expected);
    }
}

#[test]
fn builtin_profile_headers_are_accepted_by_the_outbound_policy() {
    use http::HeaderName;
    use relay_pool_desktop_lib::outbound::OutboundHeaderPolicy;

    let registry = BuiltinProfileRegistry::default();
    let policy = OutboundHeaderPolicy::provider_default();

    for profile in registry.list().filter(|profile| profile.enabled) {
        let auth_name = HeaderName::from_bytes(profile.auth.header_name().as_bytes())
            .expect("valid profile auth header");
        assert!(
            policy.allows_sensitive(&auth_name),
            "profile {:?} declares auth header {} but the transport rejects it",
            profile.id,
            profile.auth.header_name()
        );

        for header in &profile.request.headers {
            let name =
                HeaderName::from_bytes(header.name.as_bytes()).expect("valid profile header");
            assert!(
                policy.allows_public(&name),
                "profile {:?} declares outbound header {} but the transport rejects it",
                profile.id,
                header.name
            );
        }
    }
}

#[test]
fn profile_boundaries_reject_auth_cookie_host_and_transport_overrides() {
    for forbidden_header in ["authorization", "x-api-key", "cookie", "host"] {
        let profile = ClientProfileDefinition {
            id: ClientProfileId::CodexCliCompat,
            version: 1,
            enabled: true,
            supported_protocols: vec![ProtocolKind::OpenAiChat],
            auth: ProfileAuthScheme::BearerAuthorization,
            request: ClientProfileRequestShape {
                method: "POST".to_string(),
                path: "{adapter_path}".to_string(),
                headers: vec![ClientProfileHeader {
                    name: forbidden_header.to_string(),
                    value: HeaderValue::Static("redacted-fixture-value".to_string()),
                }],
                body_defaults: vec![],
            },
        };
        assert_eq!(
            profile
                .validate_boundaries()
                .expect_err("boundary violation"),
            AuthBoundaryViolation::ForbiddenHeader(forbidden_header.to_string())
        );
    }
}

#[test]
fn profile_registry_exposes_disabled_grok_cli_placeholder_without_capability() {
    let registry = BuiltinProfileRegistry::default();
    let grok = registry
        .get(ClientProfileId::GrokCliCompat)
        .expect("grok placeholder exists");
    assert!(!grok.enabled);
    assert!(grok.supported_protocols.is_empty());
    assert!(!grok.supports_protocol(ProtocolKind::XaiGrok));
}

#[test]
fn profile_and_adapter_capabilities_must_match_before_execution() {
    let registry = BuiltinProfileRegistry::default();
    assert!(registry
        .validate_execution_profile(
            ClientProfileId::CodexCliCompat,
            2,
            ProtocolKind::OpenAiResponses
        )
        .is_ok());
    assert!(registry
        .validate_execution_profile(
            ClientProfileId::CodexCliCompat,
            2,
            ProtocolKind::AnthropicMessages
        )
        .is_err());
    assert!(registry
        .validate_execution_profile(ClientProfileId::GrokCliCompat, 1, ProtocolKind::XaiGrok)
        .is_err());
    assert!(registry
        .validate_execution_profile(
            ClientProfileId::CodexCliCompat,
            1,
            ProtocolKind::OpenAiResponses
        )
        .is_err());
}
