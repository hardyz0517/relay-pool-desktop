use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    models::monitoring::{ClientProfileId, ProtocolKind},
    services::monitoring::profiles::RequestValueKind,
};

const MINIMAL_PROBE_INSTRUCTIONS: &str = "Follow the input exactly.";
const CODEX_PROBE_INSTRUCTIONS: &str = "You are Codex, based on GPT-5. You are running as a coding agent in the Codex CLI on a user's computer.";
const CLAUDE_CODE_PROBE_INSTRUCTIONS: &str =
    "You are Claude Code, Anthropic's official CLI for Claude.";

#[derive(Debug, Clone)]
pub struct ProbeRequestContext {
    session_id: String,
    request_id: String,
    device_id: String,
    account_uuid: String,
}

impl ProbeRequestContext {
    pub fn new(station_key_id: &str) -> Self {
        Self {
            session_id: Uuid::now_v7().to_string(),
            request_id: Uuid::now_v7().to_string(),
            device_id: stable_hex_identity("monitoring-device", station_key_id),
            account_uuid: stable_uuid_identity("monitoring-account", station_key_id),
        }
    }

    pub fn request_value(&self, kind: RequestValueKind) -> &str {
        match kind {
            RequestValueKind::SessionId => &self.session_id,
            RequestValueKind::RequestId => &self.request_id,
        }
    }

    fn claude_metadata_user_id(&self) -> String {
        serde_json::to_string(&json!({
            "device_id": self.device_id,
            "account_uuid": self.account_uuid,
            "session_id": self.session_id,
        }))
        .expect("Claude metadata identity serializes")
    }
}

fn stable_hex_identity(namespace: &str, station_key_id: &str) -> String {
    let digest = Sha256::digest(format!("{namespace}:{station_key_id}").as_bytes());
    format!("{digest:x}")
}

fn stable_uuid_identity(namespace: &str, station_key_id: &str) -> String {
    let digest = Sha256::digest(format!("{namespace}:{station_key_id}").as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes).to_string()
}

pub fn resolve_probe_request_path(path_template: &str, model: &str) -> String {
    if !path_template.contains("{model}") {
        return path_template.to_string();
    }

    let mut url = reqwest::Url::parse("http://monitoring.local/")
        .expect("static monitoring path base URL is valid");
    url.path_segments_mut()
        .expect("HTTP URL supports path segments")
        .push(model);
    let encoded_model = url.path().trim_start_matches('/');
    path_template.replace("{model}", encoded_model)
}

pub fn build_probe_request_body(
    protocol_kind: ProtocolKind,
    client_profile_id: ClientProfileId,
    model: &str,
    prompt: &str,
    stream: bool,
    context: &ProbeRequestContext,
) -> Vec<u8> {
    let value = match (protocol_kind, client_profile_id) {
        (ProtocolKind::OpenAiResponses, ClientProfileId::CodexCliCompat) => json!({
            "model": model,
            "instructions": CODEX_PROBE_INSTRUCTIONS,
            "input": [{
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": prompt}]
            }],
            "tools": [],
            "tool_choice": "auto",
            "parallel_tool_calls": false,
            "reasoning": {"effort": "low", "summary": "auto"},
            "store": false,
            "stream": stream
        }),
        (ProtocolKind::OpenAiResponses, _) => json!({
            "model": model,
            "instructions": MINIMAL_PROBE_INSTRUCTIONS,
            "input": prompt,
            "max_output_tokens": 16,
            "store": false,
            "stream": stream
        }),
        (ProtocolKind::AnthropicMessages, ClientProfileId::ClaudeCodeCompat) => json!({
            "model": model,
            "max_tokens": 20,
            "stream": stream,
            "metadata": {"user_id": context.claude_metadata_user_id()},
            "system": [{"type": "text", "text": CLAUDE_CODE_PROBE_INSTRUCTIONS}],
            "messages": [{
                "role": "user",
                "content": [{"type": "text", "text": prompt}]
            }],
            "tools": []
        }),
        (ProtocolKind::AnthropicMessages, _) => json!({
            "model": model,
            "max_tokens": 16,
            "stream": stream,
            "messages": [{"role": "user", "content": prompt}]
        }),
        (ProtocolKind::GeminiNative, ClientProfileId::GeminiCliCompat) => json!({
            "contents": [{"role": "user", "parts": [{"text": prompt}]}],
            "generationConfig": {
                "temperature": 0,
                "maxOutputTokens": 256,
                "thinkingConfig": {"thinkingBudget": 0}
            }
        }),
        (ProtocolKind::GeminiNative, _) => json!({
            "contents": [{"role": "user", "parts": [{"text": prompt}]}],
            "generationConfig": {"maxOutputTokens": 16}
        }),
        (ProtocolKind::OpenAiChat | ProtocolKind::GenericOpenAi, _) => json!({
            "model": model,
            "messages": [
                {"role": "system", "content": MINIMAL_PROBE_INSTRUCTIONS},
                {"role": "user", "content": prompt}
            ],
            "max_tokens": 16,
            "stream": stream
        }),
        (ProtocolKind::XaiGrok, _) => json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 16,
            "stream": stream
        }),
    };
    serde_json::to_vec(&value).expect("monitoring request body serializes")
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{
        build_probe_request_body, resolve_probe_request_path, ProbeRequestContext,
        CLAUDE_CODE_PROBE_INSTRUCTIONS, CODEX_PROBE_INSTRUCTIONS, MINIMAL_PROBE_INSTRUCTIONS,
    };
    use crate::models::monitoring::{ClientProfileId, ProtocolKind};

    fn body(protocol: ProtocolKind, profile: ClientProfileId) -> Value {
        serde_json::from_slice(&build_probe_request_body(
            protocol,
            profile,
            "probe-model",
            "Reply exactly RP_ANSWER=42",
            true,
            &ProbeRequestContext::new("station-key-test"),
        ))
        .expect("valid request JSON")
    }

    #[test]
    fn standard_responses_probe_is_minimal_and_not_stored() {
        let value = body(ProtocolKind::OpenAiResponses, ClientProfileId::StandardApi);

        assert_eq!(value["instructions"], MINIMAL_PROBE_INSTRUCTIONS);
        assert_eq!(value["input"], "Reply exactly RP_ANSWER=42");
        assert_eq!(value["max_output_tokens"], 16);
        assert_eq!(value["store"], false);
        assert_eq!(value["stream"], true);
    }

    #[test]
    fn model_path_placeholder_is_resolved_as_one_encoded_segment() {
        assert_eq!(
            resolve_probe_request_path(
                "/v1beta/models/{model}:generateContent",
                "models/gemini test"
            ),
            "/v1beta/models/models%2Fgemini%20test:generateContent"
        );
        assert_eq!(
            resolve_probe_request_path("/v1/responses", "unused"),
            "/v1/responses"
        );
    }

    #[test]
    fn codex_profile_uses_the_codex_responses_request_shape() {
        let value = body(
            ProtocolKind::OpenAiResponses,
            ClientProfileId::CodexCliCompat,
        );

        assert_eq!(value["instructions"], CODEX_PROBE_INSTRUCTIONS);
        assert_eq!(value["input"][0]["type"], "message");
        assert_eq!(value["input"][0]["role"], "user");
        assert_eq!(value["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(
            value["input"][0]["content"][0]["text"],
            "Reply exactly RP_ANSWER=42"
        );
        assert_eq!(value["tools"], serde_json::json!([]));
        assert_eq!(value["tool_choice"], "auto");
        assert_eq!(value["parallel_tool_calls"], false);
        assert_eq!(value["reasoning"]["effort"], "low");
        assert_eq!(value["reasoning"]["summary"], "auto");
        assert_eq!(value["store"], false);
        assert!(value.get("max_output_tokens").is_none());
    }

    #[test]
    fn chat_compatible_probes_supply_minimal_system_instructions() {
        for protocol in [ProtocolKind::OpenAiChat, ProtocolKind::GenericOpenAi] {
            let value = body(protocol, ClientProfileId::StandardApi);

            assert_eq!(value["messages"][0]["role"], "system");
            assert_eq!(value["messages"][0]["content"], MINIMAL_PROBE_INSTRUCTIONS);
            assert_eq!(value["messages"][1]["role"], "user");
            assert_eq!(value["max_tokens"], 16);
        }
    }

    #[test]
    fn gemini_probe_caps_output_without_requesting_cached_content() {
        let value = body(ProtocolKind::GeminiNative, ClientProfileId::GeminiCliCompat);

        assert_eq!(value["generationConfig"]["temperature"], 0);
        assert_eq!(value["generationConfig"]["maxOutputTokens"], 256);
        assert_eq!(
            value["generationConfig"]["thinkingConfig"]["thinkingBudget"],
            0
        );
        assert!(value.get("cachedContent").is_none());
        assert!(value.get("systemInstruction").is_none());
    }

    #[test]
    fn claude_code_profile_supplies_required_identity_shape_without_caching() {
        let value = body(
            ProtocolKind::AnthropicMessages,
            ClientProfileId::ClaudeCodeCompat,
        );
        let user_id: Value = serde_json::from_str(
            value["metadata"]["user_id"]
                .as_str()
                .expect("metadata user id string"),
        )
        .expect("metadata user id JSON");

        assert_eq!(value["system"][0]["text"], CLAUDE_CODE_PROBE_INSTRUCTIONS);
        assert_eq!(value["messages"][0]["content"][0]["type"], "text");
        assert_eq!(value["tools"], serde_json::json!([]));
        assert_eq!(value["max_tokens"], 20);
        assert_eq!(user_id["device_id"].as_str().map(str::len), Some(64));
        assert!(
            uuid::Uuid::parse_str(user_id["account_uuid"].as_str().unwrap_or_default()).is_ok()
        );
        assert!(uuid::Uuid::parse_str(user_id["session_id"].as_str().unwrap_or_default()).is_ok());
        assert!(!value.to_string().contains("station-key-test"));
        assert!(value.get("cache_control").is_none());
    }

    #[test]
    fn anthropic_and_grok_probes_do_not_request_prompt_caching() {
        for protocol in [ProtocolKind::AnthropicMessages, ProtocolKind::XaiGrok] {
            let value = body(protocol, ClientProfileId::StandardApi);

            assert!(value.get("cache_control").is_none());
            assert!(value.get("cachedContent").is_none());
            assert!(value.get("prompt_cache_key").is_none());
        }
    }
}
