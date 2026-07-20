mod services {
    pub(crate) mod proxy {
        #[path = "../../../src/services/proxy/protocol/mod.rs"]
        pub(crate) mod protocol;
    }
}

use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use services::proxy::protocol::{
    chat_sse::ChatSseMachine, responses_sse::ResponsesSseMachine, ProtocolMachine,
    ProtocolProgress, ProtocolTerminal,
};

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
