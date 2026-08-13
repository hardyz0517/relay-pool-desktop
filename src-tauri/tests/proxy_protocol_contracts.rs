mod services {
    pub(crate) mod proxy {
        #[path = "../../../src/services/proxy/diagnostic_memory.rs"]
        pub(crate) mod diagnostic_memory;
        #[path = "../../../src/services/proxy/protocol/mod.rs"]
        pub(crate) mod protocol;
    }
}

use bytes::Bytes;
use services::proxy::diagnostic_memory::{
    DiagnosticMemoryBudget, DEFAULT_DIAGNOSTIC_MEMORY_LIMIT_BYTES,
};
use services::proxy::protocol::{
    chat_sse::ChatSseMachine, responses_sse::ResponsesSseMachine, BootstrapDisposition,
    CompletionPolicy, DownstreamTransform, ProtocolEventKind, ProtocolMachine, ProtocolTerminal,
    ResponsePlan, SseBootstrapMachine, TransportMode, UpstreamProtocol, MAX_PROTOCOL_EVENT_BYTES,
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

    let progress = machine
        .observe_chunk(&Bytes::from_static(
            b"data: {\"type\":\"response.output_text.delta\"}\n\n",
        ))
        .expect("delta");
    assert_eq!(progress.terminal(), None);

    assert_eq!(
        machine.finish_eof().expect("clean eof"),
        ProtocolTerminal::Incomplete
    );
}

#[test]
fn responses_sse_terminal_event_completes_protocol() {
    let mut machine = ResponsesSseMachine::new();
    let progress = machine
        .observe_chunk(&Bytes::from_static(
            b"data: {\"type\":\"response.completed\"}\n\n",
        ))
        .expect("completed");
    assert_eq!(progress.terminal(), Some(ProtocolTerminal::Completed));
    assert_eq!(
        machine.finish_eof().expect("terminal eof"),
        ProtocolTerminal::Completed
    );
}

#[test]
fn responses_sse_failed_terminal_event_is_not_success() {
    let mut machine = ResponsesSseMachine::new();
    let progress = machine
        .observe_chunk(&Bytes::from_static(
            b"data: {\"type\":\"response.failed\"}\n\n",
        ))
        .expect("failed");
    assert_eq!(progress.terminal(), Some(ProtocolTerminal::Failed));
    assert_eq!(
        machine.finish_eof().expect("terminal eof"),
        ProtocolTerminal::Failed
    );
}

#[test]
fn chat_sse_done_sentinel_is_the_only_clean_stream_success() {
    let mut missing_done = ChatSseMachine::new();
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
    let progress = with_done
        .observe_chunk(&Bytes::from_static(b"data: [DONE]\n\n"))
        .expect("done");
    assert_eq!(progress.terminal(), Some(ProtocolTerminal::Completed));
}

#[test]
fn responses_capacity_error_split_across_chunks_stays_precommit() {
    let mut bootstrap = SseBootstrapMachine::new(Box::new(ResponsesSseMachine::new()));
    assert_eq!(
        bootstrap
            .observe_chunk(&Bytes::from_static(
                b"event: error\r\ndata: {\"type\":\"error\",\"error\":{\"code\":\"server_is_over"
            ))
            .expect("partial event"),
        BootstrapDisposition::Pending
    );
    let result = bootstrap
        .observe_chunk(&Bytes::from_static(b"loaded\"}}\r\n\r\n"))
        .expect("completed error event");
    assert!(matches!(
        result,
        BootstrapDisposition::PrecommitTerminal {
            terminal: ProtocolTerminal::Failed,
            ..
        }
    ));
}

#[test]
fn every_two_chunk_boundary_keeps_first_sse_error_precommit_and_releases_buffers() {
    let cases: [(&str, &[u8]); 2] = [
        (
            "responses",
            b"event: error\r\ndata: {\"type\":\"error\",\"error\":{\"code\":\"server_is_overloaded\"}}\r\n\r\n",
        ),
        (
            "chat",
            b"event: error\ndata: {\"error\":{\"code\":\"server_error\"}}\n\n",
        ),
    ];

    for (protocol, event) in cases {
        for split_at in 0..=event.len() {
            let mut bootstrap = SseBootstrapMachine::new(machine_for(protocol));
            let first = bootstrap
                .observe_chunk(&Bytes::copy_from_slice(&event[..split_at]))
                .expect("first chunk");
            let disposition = if matches!(first, BootstrapDisposition::Pending) {
                bootstrap
                    .observe_chunk(&Bytes::copy_from_slice(&event[split_at..]))
                    .expect("second chunk")
            } else {
                first
            };
            assert!(
                matches!(
                    disposition,
                    BootstrapDisposition::PrecommitTerminal {
                        terminal: ProtocolTerminal::Failed,
                        ..
                    }
                ),
                "{protocol} split {split_at} must remain retry-eligible before output commits"
            );
            assert_eq!(
                bootstrap.retained_bytes(),
                0,
                "{protocol} split {split_at} must release precommit bytes"
            );
            assert!(matches!(
                bootstrap.finish_eof().expect("terminal EOF"),
                BootstrapDisposition::PrecommitTerminal {
                    terminal: ProtocolTerminal::Failed,
                    ..
                }
            ));
        }
    }

    fn machine_for(protocol: &str) -> Box<dyn ProtocolMachine> {
        match protocol {
            "responses" => Box::new(ResponsesSseMachine::new()),
            "chat" => Box::new(ChatSseMachine::new()),
            _ => unreachable!("test protocol"),
        }
    }
}

#[test]
fn responses_created_is_control_but_text_delta_commits_and_flushes_in_order() {
    let mut bootstrap = SseBootstrapMachine::new(Box::new(ResponsesSseMachine::new()));
    assert_eq!(
        bootstrap
            .observe_chunk(&Bytes::from_static(
                b"event: response.created\ndata: {\"type\":\"response.created\"}\n\n"
            ))
            .expect("control event"),
        BootstrapDisposition::Pending
    );

    let disposition = bootstrap
        .observe_chunk(&Bytes::from_static(
            b"event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n",
        ))
        .expect("semantic event");
    let BootstrapDisposition::Emit { events, terminal } = disposition else {
        panic!("semantic output must commit");
    };
    assert_eq!(events.len(), 2);
    assert!(String::from_utf8_lossy(&events[0]).contains("response.created"));
    assert!(String::from_utf8_lossy(&events[1]).contains("response.output_text.delta"));
    assert_eq!(terminal, None);
}

#[test]
fn content_then_error_in_one_chunk_commits_before_the_error() {
    let mut bootstrap = SseBootstrapMachine::new(Box::new(ResponsesSseMachine::new()));
    let disposition = bootstrap
        .observe_chunk(&Bytes::from_static(
            b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\nevent: error\ndata: {\"type\":\"error\",\"error\":{\"code\":\"server_error\"}}\n\n",
        ))
        .expect("ordered events");
    let BootstrapDisposition::Emit { events, terminal } = disposition else {
        panic!("content commits the stream before the following error");
    };
    assert_eq!(events.len(), 2);
    assert_eq!(terminal, Some(ProtocolTerminal::Failed));
}

#[test]
fn empty_completed_response_is_a_successful_commit() {
    let mut bootstrap = SseBootstrapMachine::new(Box::new(ResponsesSseMachine::new()));
    let disposition = bootstrap
        .observe_chunk(&Bytes::from_static(
            b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"output\":[]}}\n\n",
        ))
        .expect("completed event");
    assert!(matches!(
        disposition,
        BootstrapDisposition::Emit {
            terminal: Some(ProtocolTerminal::Completed),
            ..
        }
    ));
}

#[test]
fn unknown_valid_event_conservatively_commits_and_is_preserved() {
    let mut bootstrap = SseBootstrapMachine::new(Box::new(ResponsesSseMachine::new()));
    let disposition = bootstrap
        .observe_chunk(&Bytes::from_static(
            b"event: response.future.delta\ndata: {\"type\":\"response.future.delta\",\"value\":1}\n\n",
        ))
        .expect("unknown valid event");
    let BootstrapDisposition::Emit { events, terminal } = disposition else {
        panic!("unknown valid event must commit");
    };
    assert_eq!(events.len(), 1);
    assert!(String::from_utf8_lossy(&events[0]).contains("response.future.delta"));
    assert_eq!(terminal, None);
}

#[test]
fn chat_role_control_then_error_remains_precommit() {
    let mut bootstrap = SseBootstrapMachine::new(Box::new(ChatSseMachine::new()));
    let disposition = bootstrap
        .observe_chunk(&Bytes::from_static(
            b"data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\nevent: error\ndata: {\"error\":{\"code\":\"server_is_overloaded\"}}\n\n",
        ))
        .expect("control then error");
    assert!(matches!(
        disposition,
        BootstrapDisposition::PrecommitTerminal {
            terminal: ProtocolTerminal::Failed,
            ..
        }
    ));
}

#[test]
fn protocol_event_and_bootstrap_limits_are_hard_bounds() {
    let mut event_machine = ResponsesSseMachine::new();
    let oversized = Bytes::from(vec![b'x'; MAX_PROTOCOL_EVENT_BYTES + 1]);
    let error = event_machine
        .observe_chunk(&oversized)
        .expect_err("oversized event must fail");
    assert_eq!(error.code, "protocol_event_too_large");

    let mut bootstrap = SseBootstrapMachine::new(Box::new(ResponsesSseMachine::new()));
    let control = format!(
        "event: response.created\ndata: {{\"type\":\"response.created\",\"padding\":\"{}\"}}\n\n",
        "x".repeat(130 * 1024)
    );
    assert_eq!(
        bootstrap
            .observe_chunk(&Bytes::from(control.clone()))
            .expect("first control"),
        BootstrapDisposition::Pending
    );
    let error = bootstrap
        .observe_chunk(&Bytes::from(control))
        .expect_err("bootstrap aggregate must be bounded");
    assert_eq!(error.code, "protocol_bootstrap_too_large");
    assert_eq!(
        bootstrap.retained_bytes(),
        0,
        "failure releases retained bytes"
    );
}

#[test]
fn protocol_progress_exposes_each_complete_event_in_order() {
    let mut machine = ResponsesSseMachine::new();
    let progress = machine
        .observe_chunk(&Bytes::from_static(
            b": heartbeat\n\nevent: response.created\ndata: {\"type\":\"response.created\"}\n\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"x\"}\n\n",
        ))
        .expect("events");
    let events = progress.into_events();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].kind, ProtocolEventKind::Heartbeat);
    assert_eq!(events[1].kind, ProtocolEventKind::Control);
    assert_eq!(events[2].kind, ProtocolEventKind::Semantic);
}

#[test]
fn eof_after_only_control_events_is_precommit_incomplete_and_releases_buffer() {
    let mut bootstrap = SseBootstrapMachine::for_completion_policy_with_diagnostic_memory(
        CompletionPolicy::ResponsesTerminalEvent,
        DiagnosticMemoryBudget::new(DEFAULT_DIAGNOSTIC_MEMORY_LIMIT_BYTES),
    )
    .expect("responses bootstrap")
    .expect("responses bootstrap machine");
    assert_eq!(
        bootstrap
            .observe_chunk(&Bytes::from_static(
                b"event: response.created\ndata: {\"type\":\"response.created\"}\n\n",
            ))
            .expect("control"),
        BootstrapDisposition::Pending
    );
    assert!(bootstrap.retained_bytes() > 0);
    assert!(matches!(
        bootstrap.finish_eof().expect("clean eof"),
        BootstrapDisposition::PrecommitTerminal {
            terminal: ProtocolTerminal::Incomplete,
            ..
        }
    ));
    assert_eq!(bootstrap.retained_bytes(), 0);
}

#[test]
fn malformed_precommit_event_releases_buffered_controls() {
    let mut bootstrap = SseBootstrapMachine::new(Box::new(ResponsesSseMachine::new()));
    bootstrap
        .observe_chunk(&Bytes::from_static(
            b"data: {\"type\":\"response.created\"}\n\n",
        ))
        .expect("control");
    let error = bootstrap
        .observe_chunk(&Bytes::from_static(b"data: {not-json}\n\n"))
        .expect_err("malformed event");
    assert_eq!(error.code, "malformed_protocol_event");
    assert_eq!(bootstrap.retained_bytes(), 0);
}

#[test]
fn event_limit_includes_the_sse_delimiter() {
    let mut machine = ResponsesSseMachine::new();
    let mut event = vec![b'x'; MAX_PROTOCOL_EVENT_BYTES - 1];
    event.extend_from_slice(b"\n\n");
    let error = machine
        .observe_chunk(&Bytes::from(event))
        .expect_err("delimiter bytes count toward the limit");
    assert_eq!(error.code, "protocol_event_too_large");
    assert_eq!(machine.retained_bytes(), 0);
}

#[test]
fn chat_content_then_error_in_one_chunk_commits_before_the_error() {
    let mut bootstrap = SseBootstrapMachine::new(Box::new(ChatSseMachine::new()));
    let disposition = bootstrap
        .observe_chunk(&Bytes::from_static(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\nevent: error\ndata: {\"error\":{\"code\":\"server_error\"}}\n\n",
        ))
        .expect("ordered chat events");
    let BootstrapDisposition::Emit { events, terminal } = disposition else {
        panic!("chat content commits before its following error");
    };
    assert_eq!(events.len(), 2);
    assert_eq!(terminal, Some(ProtocolTerminal::Failed));
}

#[test]
fn named_heartbeat_with_data_does_not_commit_and_responses_done_completes() {
    let mut bootstrap = SseBootstrapMachine::new(Box::new(ResponsesSseMachine::new()));
    assert_eq!(
        bootstrap
            .observe_chunk(&Bytes::from_static(
                b"event: ping\ndata: {\"timestamp\":1}\n\n",
            ))
            .expect("heartbeat"),
        BootstrapDisposition::Pending
    );
    assert!(matches!(
        bootstrap
            .observe_chunk(&Bytes::from_static(b"data: [DONE]\n\n"))
            .expect("done"),
        BootstrapDisposition::Emit {
            terminal: Some(ProtocolTerminal::Completed),
            ..
        }
    ));
}
