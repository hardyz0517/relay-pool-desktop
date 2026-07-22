mod services {
    pub(crate) mod proxy {
        #[path = "../../../src/services/proxy/protocol/mod.rs"]
        pub(crate) mod protocol;
    }
}

use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use services::proxy::protocol::{
    chat_sse::ChatSseMachine, responses_sse::ResponsesSseMachine, CompletionPolicy,
    DownstreamTransform, ProtocolMachine, ProtocolProgress, ProtocolTerminal, ResponsePlan,
    TransportMode, UpstreamProtocol,
};

#[test]
fn response_plan_contract_covers_every_supported_protocol_shape() {
    let upstream_protocols = [
        UpstreamProtocol::ResponsesJson,
        UpstreamProtocol::ResponsesSse,
        UpstreamProtocol::ChatCompletionsJson,
        UpstreamProtocol::ChatCompletionsSse,
        UpstreamProtocol::EmbeddingsJson,
        UpstreamProtocol::ModelsJson,
    ];
    let completion_policies = [
        CompletionPolicy::ValidatedJsonBody,
        CompletionPolicy::ResponsesTerminalEvent,
        CompletionPolicy::ChatDoneSentinel,
    ];

    let plans = upstream_protocols
        .into_iter()
        .zip(completion_policies.into_iter().cycle())
        .enumerate()
        .map(
            |(index, (upstream_protocol, completion_policy))| ResponsePlan {
                transport: if index % 2 == 0 {
                    TransportMode::Buffered
                } else {
                    TransportMode::Streaming
                },
                upstream_protocol,
                downstream_transform: if index % 2 == 0 {
                    DownstreamTransform::Passthrough
                } else {
                    DownstreamTransform::ChatToResponses
                },
                completion_policy,
            },
        )
        .collect::<Vec<_>>();

    assert_eq!(plans.len(), 6);
    assert!(plans.iter().any(|plan| {
        plan.transport == TransportMode::Streaming
            && plan.downstream_transform == DownstreamTransform::ChatToResponses
    }));
    assert!(plans
        .iter()
        .any(|plan| plan.upstream_protocol == UpstreamProtocol::ModelsJson));
    assert!(plans
        .iter()
        .any(|plan| { plan.completion_policy == CompletionPolicy::ResponsesTerminalEvent }));
}

#[test]
fn responses_sse_eof_without_terminal_is_explicitly_incomplete() {
    let mut machine = ResponsesSseMachine::new();
    machine
        .observe_headers(StatusCode::OK, &HeaderMap::new())
        .expect("headers");

    assert_eq!(
        machine
            .observe_chunk(&Bytes::from_static(
                b"data: {\"type\":\"response.output_text.delta\"}\n\n"
            ))
            .expect("delta"),
        ProtocolProgress::Observed
    );

    assert_eq!(
        machine.finish_eof().expect("clean eof"),
        ProtocolTerminal::Incomplete
    );
}

#[test]
fn responses_sse_terminal_event_completes_protocol() {
    let mut machine = ResponsesSseMachine::new();
    machine
        .observe_headers(StatusCode::OK, &HeaderMap::new())
        .expect("headers");
    assert_eq!(
        machine
            .observe_chunk(&Bytes::from_static(
                b"data: {\"type\":\"response.completed\"}\n\n"
            ))
            .expect("completed"),
        ProtocolProgress::Terminal(ProtocolTerminal::Completed)
    );
    assert_eq!(
        machine.finish_eof().expect("terminal eof"),
        ProtocolTerminal::Completed
    );
}

#[test]
fn chat_sse_done_sentinel_is_the_only_clean_stream_success() {
    let mut missing_done = ChatSseMachine::new();
    missing_done
        .observe_headers(StatusCode::OK, &HeaderMap::new())
        .expect("headers");
    missing_done
        .observe_chunk(&Bytes::from_static(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
        ))
        .expect("delta");
    assert_eq!(
        missing_done.finish_eof().expect("clean eof"),
        ProtocolTerminal::Incomplete
    );

    let mut with_done = ChatSseMachine::new();
    with_done
        .observe_headers(StatusCode::OK, &HeaderMap::new())
        .expect("headers");
    assert_eq!(
        with_done
            .observe_chunk(&Bytes::from_static(b"data: [DONE]\n\n"))
            .expect("done"),
        ProtocolProgress::Terminal(ProtocolTerminal::Completed)
    );
}
