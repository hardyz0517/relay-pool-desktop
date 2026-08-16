#[path = "../src/models/monitoring/outcome.rs"]
pub mod mon_outcome;

mod models {
    pub mod monitoring {
        pub use crate::mon_outcome::{FailureKind, ProbeOutcome, ProtocolKind};
    }
}

#[path = "../src/services/monitoring/adapters/anthropic_messages.rs"]
pub mod anthropic_messages;
#[path = "../src/services/monitoring/challenge.rs"]
pub mod challenge;
#[path = "../src/services/monitoring/adapters/contract.rs"]
pub mod contract;
#[path = "../src/services/monitoring/adapters/gemini_native.rs"]
pub mod gemini_native;
#[path = "../src/services/monitoring/adapters/generic_openai.rs"]
pub mod generic_openai;
#[path = "../src/services/monitoring/adapters/http_mapping.rs"]
pub mod http_mapping;
#[path = "../src/services/monitoring/adapters/openai_chat.rs"]
pub mod openai_chat;
#[path = "../src/services/monitoring/adapters/openai_responses.rs"]
pub mod openai_responses;
#[path = "../src/services/monitoring/adapters/openai_stream.rs"]
pub mod openai_stream;
#[path = "../src/services/protocol_streaming/sse.rs"]
pub mod protocol_sse;
pub(crate) use protocol_sse::{SseDecoder, SseEvent, SseLimits, StreamError};
#[path = "../src/services/protocol_streaming/openai.rs"]
pub mod protocol_openai;
#[path = "../src/services/monitoring/adapters/sse.rs"]
pub mod sse;
#[path = "../src/services/monitoring/adapters/xai_grok.rs"]
pub mod xai_grok;

mod services {
    pub mod monitoring {
        pub use crate::challenge;

        pub mod adapters {
            #[allow(unused_imports)]
            pub use crate::anthropic_messages;
            pub use crate::contract;
            #[allow(unused_imports)]
            pub use crate::gemini_native;
            #[allow(unused_imports)]
            pub use crate::generic_openai;
            pub use crate::http_mapping;
            pub use crate::openai_chat;
            #[allow(unused_imports)]
            pub use crate::openai_responses;
            pub(crate) use crate::openai_stream;
            pub use crate::sse;
            #[allow(unused_imports)]
            pub use crate::xai_grok;
        }
    }
    pub(crate) mod protocol_streaming {
        pub(crate) use crate::{
            protocol_openai::{
                parse_openai_responses_json, OpenAiChatReducer, OpenAiResponsesReducer,
                OpenAiStreamSummary, OpenAiUsage,
            },
            SseDecoder, SseEvent, SseLimits, StreamError,
        };
    }
}

use models::monitoring::{FailureKind, ProbeOutcome, ProtocolKind};
use services::monitoring::adapters::contract::ProtocolAdapter;
use services::monitoring::{
    adapters::{contract::ResponseLimits, http_mapping::parse_json_probe_response, sse::SseParser},
    challenge::{ChallengeValidator, ProbeChallenge},
};

fn validator() -> ChallengeValidator {
    ChallengeValidator::from_expected_answer_for_tests("RP_ANSWER=42")
}

#[test]
fn common_challenge_uses_random_tokens_and_does_not_serialize_expected_answer() {
    let first = ProbeChallenge::generate_arithmetic();
    let second = ProbeChallenge::generate_arithmetic();

    assert_ne!(first.snapshot().token, second.snapshot().token);
    let serialized = serde_json::to_string(&first.snapshot()).expect("snapshot json");
    assert!(serialized.contains("prompt"));
    assert!(serialized.contains("token"));
    assert!(!serialized.contains("expected"));
    assert!(!serialized.contains("answer_hash"));
}

#[test]
fn common_http_200_html_error_json_and_empty_body_fail_closed() {
    let cases = [
        (
            "text/html",
            br#"<html>ok</html>"#.as_slice(),
            FailureKind::ProtocolMismatch,
        ),
        (
            "application/json",
            br#"{"error":{"message":"nope"}}"#.as_slice(),
            FailureKind::ProtocolMismatch,
        ),
        (
            "application/json",
            b"".as_slice(),
            FailureKind::ProtocolMismatch,
        ),
        (
            "application/json",
            br#"{"content":"wrong"}"#.as_slice(),
            FailureKind::ContentMismatch,
        ),
    ];

    for (content_type, body, expected_failure) in cases {
        let parsed = parse_json_probe_response(
            ProtocolKind::OpenAiChat,
            200,
            Some(content_type),
            body,
            &validator(),
            ResponseLimits::default(),
        );
        assert_eq!(parsed.outcome, ProbeOutcome::Unavailable);
        assert_eq!(parsed.failure_kind, Some(expected_failure));
    }
}

#[test]
fn common_http_status_mapping_classifies_4xx_429_and_5xx() {
    let cases = [
        (401, FailureKind::Auth),
        (403, FailureKind::Auth),
        (429, FailureKind::RateLimit),
        (400, FailureKind::InvalidRequest),
        (422, FailureKind::InvalidRequest),
        (404, FailureKind::ClientError),
        (500, FailureKind::ServerError),
    ];

    for (status, expected_failure) in cases {
        let parsed = parse_json_probe_response(
            ProtocolKind::OpenAiChat,
            status,
            Some("application/json"),
            br#"{"error":{"message":"failed"}}"#,
            &validator(),
            ResponseLimits::default(),
        );
        assert_eq!(parsed.outcome, ProbeOutcome::Unavailable);
        assert_eq!(parsed.failure_kind, Some(expected_failure));
    }
}

#[test]
fn common_json_success_requires_extracted_output_and_validator_hit() {
    let parsed = parse_json_probe_response(
        ProtocolKind::OpenAiChat,
        200,
        Some("application/json"),
        br#"{"choices":[{"message":{"content":"RP_ANSWER=42"}}]}"#,
        &validator(),
        ResponseLimits::default(),
    );

    assert_eq!(parsed.outcome, ProbeOutcome::Available);
    assert_eq!(parsed.output_text.as_deref(), Some("RP_ANSWER=42"));

    let explained = parse_json_probe_response(
        ProtocolKind::OpenAiChat,
        200,
        Some("application/json"),
        br#"{"choices":[{"message":{"content":"The result is `RP_ANSWER=42`."}}]}"#,
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(explained.outcome, ProbeOutcome::Available);

    let mismatch = parse_json_probe_response(
        ProtocolKind::OpenAiChat,
        200,
        Some("application/json"),
        br#"{"choices":[{"message":{"content":"RP_ANSWER=41"}}]}"#,
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(mismatch.failure_kind, Some(FailureKind::ContentMismatch));
    assert_eq!(mismatch.output_bytes, "RP_ANSWER=41".len());
    assert!(mismatch.output_text.is_none());
}

#[test]
fn common_sse_chunk_boundaries_crlf_lf_and_utf8_are_stable() {
    let body = "data: {\"delta\":\"RP_ANSWER=\"}\r\n\r\ndata: {\"delta\":\"42\"}\n\ndata: {\"delta\":\"雪\"}\n\ndata: [DONE]\n\n";
    let expected = parse_sse(body.as_bytes(), body.len() + 32);

    for split in 1..body.len() {
        let mut parser = SseParser::new(
            ProtocolKind::OpenAiChat,
            ResponseLimits {
                max_response_bytes: body.len() + 32,
                max_output_bytes: 128,
                max_sse_events: 16,
            },
        );
        parser.push(&body.as_bytes()[..split]).ok();
        parser.push(&body.as_bytes()[split..]).ok();
        let parsed = parser.finish(&ChallengeValidator::from_expected_answer_for_tests(
            "RP_ANSWER=42雪",
        ));
        assert_eq!(parsed, expected, "split={split}");
    }
}

#[test]
fn common_sse_premature_eof_error_event_and_limits_fail_closed() {
    let mut premature = SseParser::new(ProtocolKind::OpenAiChat, ResponseLimits::default());
    premature
        .push(br#"data: {"delta":"RP_ANSWER=42"}"#)
        .expect("chunk");
    let parsed = premature.finish(&validator());
    assert_eq!(parsed.outcome, ProbeOutcome::Unavailable);
    assert_eq!(parsed.failure_kind, Some(FailureKind::ProtocolMismatch));

    let mut error_event = SseParser::new(ProtocolKind::OpenAiChat, ResponseLimits::default());
    assert_eq!(
        error_event.push(br#"data: {"error":{"message":"bad"}}"#),
        Ok(Vec::new())
    );
    assert_eq!(
        error_event.push(b"\n\n").expect_err("protocol error"),
        FailureKind::ProtocolMismatch
    );

    let mut limited = SseParser::new(
        ProtocolKind::OpenAiChat,
        ResponseLimits {
            max_response_bytes: 8,
            max_output_bytes: 8,
            max_sse_events: 1,
        },
    );
    assert_eq!(
        limited
            .push(br#"data: {"delta":"too much"}"#)
            .expect_err("limit"),
        FailureKind::ProtocolMismatch
    );
}

fn parse_sse(
    body: &[u8],
    max_response_bytes: usize,
) -> services::monitoring::adapters::contract::ParsedProbeResponse {
    let mut parser = SseParser::new(
        ProtocolKind::OpenAiChat,
        ResponseLimits {
            max_response_bytes,
            max_output_bytes: 128,
            max_sse_events: 16,
        },
    );
    for chunk in body.chunks(3) {
        parser.push(chunk).expect("chunk");
    }
    parser.finish(&ChallengeValidator::from_expected_answer_for_tests(
        "RP_ANSWER=42雪",
    ))
}

#[test]
fn openai_chat_json_success_extracts_model_usage_and_rejects_fake_200() {
    let adapter = openai_chat::OpenAiChatAdapter::new(false);
    let parsed = adapter.parse_response(
        200,
        Some("application/json"),
        include_bytes!("fixtures/monitoring/openai_chat/chat_success.json"),
        &validator(),
        ResponseLimits::default(),
    );

    assert_eq!(parsed.protocol_kind, ProtocolKind::OpenAiChat);
    assert_eq!(parsed.outcome, ProbeOutcome::Available);
    assert_eq!(parsed.model.as_deref(), Some("gpt-4.1-mini"));
    assert_eq!(
        parsed.usage.as_ref().and_then(|usage| usage.total_tokens),
        Some(11)
    );

    let fake = adapter.parse_response(
        200,
        Some("application/json"),
        include_bytes!("fixtures/monitoring/openai_chat/chat_no_content.json"),
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(fake.outcome, ProbeOutcome::Unavailable);
    assert_eq!(fake.failure_kind, Some(FailureKind::EmptyResponse));
}

#[test]
fn openai_chat_stream_requires_delta_content_and_done() {
    let adapter = openai_chat::OpenAiChatAdapter::new(true);
    let parsed = adapter.parse_response(
        200,
        Some("text/event-stream"),
        include_bytes!("fixtures/monitoring/openai_chat/chat_stream_success.sse"),
        &validator(),
        ResponseLimits::default(),
    );

    assert_eq!(parsed.outcome, ProbeOutcome::Available);
    assert_eq!(parsed.output_text.as_deref(), Some("RP_ANSWER=42"));
    assert_eq!(parsed.model.as_deref(), Some("gpt-4.1-mini"));

    let no_content = adapter.parse_response(
        200,
        Some("text/event-stream"),
        include_bytes!("fixtures/monitoring/openai_chat/chat_stream_no_content.sse"),
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(no_content.outcome, ProbeOutcome::Unavailable);
    assert_eq!(no_content.failure_kind, Some(FailureKind::EmptyResponse));

    let error = adapter.parse_response(
        200,
        Some("text/event-stream"),
        include_bytes!("fixtures/monitoring/openai_chat/chat_stream_error.sse"),
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(error.failure_kind, Some(FailureKind::ProtocolMismatch));
}

#[test]
fn openai_responses_json_and_stream_terminal_semantics_are_distinct() {
    let json_adapter = openai_responses::OpenAiResponsesAdapter::new(false);
    let parsed = json_adapter.parse_response(
        200,
        Some("application/json"),
        include_bytes!("fixtures/monitoring/openai_responses/responses_completed.json"),
        &validator(),
        ResponseLimits::default(),
    );

    assert_eq!(parsed.protocol_kind, ProtocolKind::OpenAiResponses);
    assert_eq!(parsed.outcome, ProbeOutcome::Available);
    assert_eq!(parsed.model.as_deref(), Some("gpt-4.1-mini"));
    assert_eq!(
        parsed.usage.as_ref().and_then(|usage| usage.total_tokens),
        Some(11)
    );

    for fixture in [
        include_bytes!("fixtures/monitoring/openai_responses/responses_failed.json").as_slice(),
        include_bytes!("fixtures/monitoring/openai_responses/responses_incomplete.json").as_slice(),
    ] {
        let failed = json_adapter.parse_response(
            200,
            Some("application/json"),
            fixture,
            &validator(),
            ResponseLimits::default(),
        );
        assert_eq!(failed.outcome, ProbeOutcome::Unavailable);
        assert_eq!(failed.failure_kind, Some(FailureKind::ProtocolMismatch));
    }

    let stream_adapter = openai_responses::OpenAiResponsesAdapter::new(true);
    let stream = stream_adapter.parse_response(
        200,
        Some("text/event-stream"),
        include_bytes!("fixtures/monitoring/openai_responses/responses_stream_completed.sse"),
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(stream.outcome, ProbeOutcome::Available);
    assert_eq!(stream.output_text.as_deref(), Some("RP_ANSWER=42"));
}
#[test]
fn generic_openai_only_accepts_minimal_chat_compatible_intersection() {
    let adapter = generic_openai::GenericOpenAiAdapter::new(false);
    let chat = adapter.parse_response(
        200,
        Some("application/json"),
        include_bytes!("fixtures/monitoring/generic_openai/minimal_chat_success.json"),
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(chat.protocol_kind, ProtocolKind::GenericOpenAi);
    assert_eq!(chat.outcome, ProbeOutcome::Available);

    let responses_shape = adapter.parse_response(
        200,
        Some("application/json"),
        include_bytes!("fixtures/monitoring/openai_responses/responses_completed.json"),
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(responses_shape.outcome, ProbeOutcome::Unavailable);
    assert_eq!(
        responses_shape.failure_kind,
        Some(FailureKind::ProtocolMismatch)
    );
}

#[test]
fn xai_grok_uses_distinct_adapter_even_for_chat_like_wire_shape() {
    let adapter = xai_grok::XaiGrokAdapter::new(false);
    let descriptor = adapter.request_descriptor();
    assert_eq!(descriptor.path, "/v1/chat/completions");

    let parsed = adapter.parse_response(
        200,
        Some("application/json"),
        include_bytes!("fixtures/monitoring/xai_grok/grok_chat_success.json"),
        &validator(),
        ResponseLimits::default(),
    );

    assert_eq!(parsed.protocol_kind, ProtocolKind::XaiGrok);
    assert_eq!(parsed.outcome, ProbeOutcome::Available);
    assert_eq!(parsed.model.as_deref(), Some("grok-4"));
}

#[test]
fn anthropic_messages_json_and_stream_require_message_terminal_semantics() {
    let json_adapter = anthropic_messages::AnthropicMessagesAdapter::new(false);
    let descriptor = json_adapter.request_descriptor();
    assert_eq!(descriptor.path, "/v1/messages");

    let parsed = json_adapter.parse_response(
        200,
        Some("application/json"),
        include_bytes!("fixtures/monitoring/anthropic_messages/message_success.json"),
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(parsed.protocol_kind, ProtocolKind::AnthropicMessages);
    assert_eq!(parsed.outcome, ProbeOutcome::Available);
    assert_eq!(parsed.model.as_deref(), Some("claude-3-5-sonnet-20241022"));
    assert_eq!(
        parsed.usage.as_ref().and_then(|usage| usage.total_tokens),
        Some(11)
    );
    assert_eq!(
        parsed
            .usage
            .as_ref()
            .and_then(|usage| usage.cache_creation_tokens),
        Some(1)
    );
    assert_eq!(
        parsed
            .usage
            .as_ref()
            .and_then(|usage| usage.cache_read_tokens),
        Some(2)
    );

    let error = json_adapter.parse_response(
        200,
        Some("application/json"),
        include_bytes!("fixtures/monitoring/anthropic_messages/message_error.json"),
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(error.outcome, ProbeOutcome::Unavailable);
    assert_eq!(error.failure_kind, Some(FailureKind::ProtocolMismatch));

    let stream_adapter = anthropic_messages::AnthropicMessagesAdapter::new(true);
    let stream = stream_adapter.parse_response(
        200,
        Some("text/event-stream"),
        include_bytes!("fixtures/monitoring/anthropic_messages/message_stream_success.sse"),
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(stream.outcome, ProbeOutcome::Available);
    assert_eq!(stream.output_text.as_deref(), Some("RP_ANSWER=42"));

    let stream_error = stream_adapter.parse_response(
        200,
        Some("text/event-stream"),
        include_bytes!("fixtures/monitoring/anthropic_messages/message_stream_error.sse"),
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(stream_error.outcome, ProbeOutcome::Unavailable);
    assert_eq!(
        stream_error.failure_kind,
        Some(FailureKind::ProtocolMismatch)
    );
}

#[test]
fn gemini_native_json_and_stream_distinguish_stop_from_safety_or_api_error() {
    let json_adapter = gemini_native::GeminiNativeAdapter::new(false);
    let descriptor = json_adapter.request_descriptor();
    assert_eq!(descriptor.path, "/v1beta/models/{model}:generateContent");

    let parsed = json_adapter.parse_response(
        200,
        Some("application/json"),
        include_bytes!("fixtures/monitoring/gemini_native/generate_content_success.json"),
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(parsed.protocol_kind, ProtocolKind::GeminiNative);
    assert_eq!(parsed.outcome, ProbeOutcome::Available);
    assert_eq!(parsed.model.as_deref(), Some("gemini-2.5-flash"));
    assert_eq!(
        parsed.usage.as_ref().and_then(|usage| usage.total_tokens),
        Some(11)
    );

    for fixture in [
        include_bytes!("fixtures/monitoring/gemini_native/generate_content_blocked.json")
            .as_slice(),
        include_bytes!("fixtures/monitoring/gemini_native/generate_content_error.json").as_slice(),
    ] {
        let failed = json_adapter.parse_response(
            200,
            Some("application/json"),
            fixture,
            &validator(),
            ResponseLimits::default(),
        );
        assert_eq!(failed.outcome, ProbeOutcome::Unavailable);
        assert_eq!(failed.failure_kind, Some(FailureKind::ProtocolMismatch));
    }

    let stream_adapter = gemini_native::GeminiNativeAdapter::new(true);
    let stream = stream_adapter.parse_response(
        200,
        Some("text/event-stream"),
        include_bytes!("fixtures/monitoring/gemini_native/stream_generate_content_success.sse"),
        &validator(),
        ResponseLimits::default(),
    );
    assert_eq!(stream.outcome, ProbeOutcome::Available);
    assert_eq!(stream.output_text.as_deref(), Some("RP_ANSWER=42"));
}
