use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DefinitionRevision(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetScope {
    Station {
        station_id: String,
    },
    StationKey {
        station_id: String,
        station_key_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientProfileId {
    StandardApi,
    CodexCliCompat,
    ClaudeCodeCompat,
    GeminiCliCompat,
    GrokCliCompat,
}

impl ClientProfileId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StandardApi => "standard_api",
            Self::CodexCliCompat => "codex_cli_compat",
            Self::ClaudeCodeCompat => "claude_code_compat",
            Self::GeminiCliCompat => "gemini_cli_compat",
            Self::GrokCliCompat => "grok_cli_compat",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "standard_api" => Some(Self::StandardApi),
            "codex_cli_compat" => Some(Self::CodexCliCompat),
            "claude_code_compat" => Some(Self::ClaudeCodeCompat),
            "gemini_cli_compat" => Some(Self::GeminiCliCompat),
            "grok_cli_compat" => Some(Self::GrokCliCompat),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientProfileRef {
    pub id: ClientProfileId,
    pub version: u32,
}

impl ClientProfileRef {
    pub fn new(id: ClientProfileId, version: u32) -> Result<Self, String> {
        if version == 0 {
            return Err("client_profile_version must be positive".to_string());
        }
        Ok(Self { id, version })
    }
}
