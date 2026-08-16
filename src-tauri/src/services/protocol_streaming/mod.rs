//! Incremental, bounded protocol-streaming primitives shared by outbound probes.
//!
//! This module owns SSE framing and OpenAI stream semantics only. Callers retain
//! ownership of HTTP classification, request profiles, persistence, and health
//! writeback decisions.

mod openai;
mod sse;

pub(crate) use openai::{
    parse_openai_responses_json, OpenAiChatReducer, OpenAiResponsesReducer, OpenAiStreamSummary,
    OpenAiUsage,
};
pub(crate) use sse::{SseDecoder, SseEvent, SseLimits, StreamError};
