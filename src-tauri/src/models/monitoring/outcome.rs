use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolKind {
    OpenAiChat,
    OpenAiResponses,
    AnthropicMessages,
    GeminiNative,
    XaiGrok,
    GenericOpenAi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeOutcome {
    Available,
    Degraded,
    Unavailable,
    Skipped,
}

impl ProbeOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
            Self::Skipped => "skipped",
        }
    }

    pub fn contributes_to_availability_denominator(self) -> bool {
        !matches!(self, Self::Skipped)
    }

    pub fn is_route_available(self) -> bool {
        matches!(self, Self::Available | Self::Degraded)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Auth,
    RateLimit,
    ServerError,
    ClientError,
    InvalidRequest,
    Network,
    Timeout,
    ProtocolMismatch,
    ContentMismatch,
    SlowLatency,
    RecoveredAfterRetry,
    NeedsConfiguration,
    BudgetExceeded,
    Cancelled,
    Interrupted,
    Internal,
    LegacyHttpOnly,
}

impl FailureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::RateLimit => "rate_limit",
            Self::ServerError => "server_error",
            Self::ClientError => "client_error",
            Self::InvalidRequest => "invalid_request",
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::ProtocolMismatch => "protocol_mismatch",
            Self::ContentMismatch => "content_mismatch",
            Self::SlowLatency => "slow_latency",
            Self::RecoveredAfterRetry => "recovered_after_retry",
            Self::NeedsConfiguration => "needs_configuration",
            Self::BudgetExceeded => "budget_exceeded",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
            Self::Internal => "internal",
            Self::LegacyHttpOnly => "legacy_http_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticConfidence {
    ProtocolValidated,
    LegacyHttpOnly,
}

impl SemanticConfidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProtocolValidated => "protocol_validated",
            Self::LegacyHttpOnly => "legacy_http_only",
        }
    }

    pub fn allows_authoritative_health_writeback(self) -> bool {
        matches!(self, Self::ProtocolValidated)
    }
}

impl ProtocolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiChat => "open_ai_chat",
            Self::OpenAiResponses => "open_ai_responses",
            Self::AnthropicMessages => "anthropic_messages",
            Self::GeminiNative => "gemini_native",
            Self::XaiGrok => "xai_grok",
            Self::GenericOpenAi => "generic_open_ai",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "open_ai_chat" => Some(Self::OpenAiChat),
            "open_ai_responses" => Some(Self::OpenAiResponses),
            "anthropic_messages" => Some(Self::AnthropicMessages),
            "gemini_native" => Some(Self::GeminiNative),
            "xai_grok" => Some(Self::XaiGrok),
            "generic_open_ai" => Some(Self::GenericOpenAi),
            _ => None,
        }
    }
}
