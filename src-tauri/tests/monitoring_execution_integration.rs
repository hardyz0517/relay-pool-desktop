use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use tokio_util::sync::CancellationToken;

#[path = "../src/models/monitoring/outcome.rs"]
pub mod model_outcome;

mod models {
    pub mod monitoring {
        use serde::{Deserialize, Serialize};

        pub use crate::model_outcome::{
            FailureKind, ProbeOutcome, ProtocolKind, SemanticConfidence,
        };

        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum ClientProfileId {
            StandardApi,
            CodexCliCompat,
            ClaudeCodeCompat,
            GeminiCliCompat,
            GrokCliCompat,
        }
    }
}

mod outbound {
    pub use relay_pool_desktop_lib::outbound::*;
}

#[path = "../src/services/monitoring/adapters/anthropic_messages.rs"]
pub mod anthropic_messages;
#[path = "../src/services/monitoring/challenge.rs"]
pub mod challenge;
#[path = "../src/services/monitoring/adapters/contract.rs"]
pub mod contract;
#[path = "../src/services/monitoring/executor.rs"]
pub mod executor;
#[path = "../src/services/monitoring/adapters/gemini_native.rs"]
pub mod gemini_native;
#[path = "../src/services/monitoring/adapters/generic_openai.rs"]
pub mod generic_openai;
#[path = "../src/services/monitoring/adapters/http_mapping.rs"]
pub mod http_mapping;
#[path = "../src/services/monitoring/auth.rs"]
pub mod monitoring_auth;
#[path = "../src/services/monitoring/adapters/openai_chat.rs"]
pub mod openai_chat;
#[path = "../src/services/monitoring/adapters/openai_responses.rs"]
pub mod openai_responses;
#[path = "../src/services/monitoring/profiles/mod.rs"]
pub mod profiles;
#[path = "../src/services/monitoring/request_shape.rs"]
pub mod request_shape;
#[path = "../src/services/monitoring/adapters/sse.rs"]
pub mod sse;
#[path = "../src/services/monitoring/transport.rs"]
pub mod transport;
#[path = "../src/services/monitoring/adapters/xai_grok.rs"]
pub mod xai_grok;

mod services {
    pub mod monitoring {
        pub mod adapters {
            pub mod anthropic_messages {
                pub use crate::anthropic_messages::*;
            }
            pub mod contract {
                pub use crate::contract::*;
            }
            pub mod gemini_native {
                pub use crate::gemini_native::*;
            }
            pub mod generic_openai {
                pub use crate::generic_openai::*;
            }
            pub mod http_mapping {
                pub use crate::http_mapping::*;
            }
            pub mod openai_chat {
                pub use crate::openai_chat::*;
            }
            pub mod openai_responses {
                pub use crate::openai_responses::*;
            }
            pub mod sse {
                pub use crate::sse::*;
            }
            pub mod xai_grok {
                pub use crate::xai_grok::*;
            }
        }
        pub mod auth {
            pub use crate::monitoring_auth::*;
        }
        pub mod challenge {
            pub use crate::challenge::*;
        }
        pub mod profiles {
            pub use crate::profiles::*;
        }
        pub mod request_shape {
            pub use crate::request_shape::*;
        }
        pub mod transport {
            pub use crate::transport::*;
        }
    }
}

use challenge::ChallengeValidator;
use executor::{ProbeExecutionInput, ProbeExecutor, ProbeSecretResolver, ResolvedProbeSecret};
use models::monitoring::{ClientProfileId, FailureKind, ProbeOutcome, ProtocolKind};
use transport::{MonitoringTransport, MonitoringTransportConfig};

type BoxServerFuture = Pin<Box<dyn Future<Output = String> + Send>>;

#[derive(Clone)]
struct TestServer {
    url: String,
    received: Arc<Mutex<Vec<String>>>,
}

async fn spawn_server(
    handler: impl Fn(String) -> BoxServerFuture + Send + Sync + 'static,
) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind server");
    let addr = listener.local_addr().expect("server address");
    let received = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(handler);
    let server_received = Arc::clone(&received);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let handler = Arc::clone(&handler);
            let server_received = Arc::clone(&server_received);
            tokio::spawn(async move {
                let mut buffer = [0_u8; 8192];
                let size = stream.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..size]).to_string();
                server_received
                    .lock()
                    .expect("received lock")
                    .push(request.clone());
                let response = handler(request).await;
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    TestServer {
        url: format!("http://{addr}"),
        received,
    }
}

#[tokio::test]
async fn loopback_executor_sends_secret_late_parses_response_and_redacts_debug() {
    let canary = "rp-canary-secret-executor";
    let server = spawn_server(|request| {
        Box::pin(async move {
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer rp-canary-secret-executor"));
            assert!(request.contains("/v1/responses"));
            response(
                "200 OK",
                &[("content-type", "application/json")],
                r#"{
                    "status":"completed",
                    "model":"gpt-primary",
                    "output":[{"content":[{"type":"output_text","text":"RP_ANSWER=42"}]}],
                    "usage":{"input_tokens":9,"output_tokens":3,"total_tokens":12}
                }"#,
            )
        })
    })
    .await;
    let executor = ProbeExecutor::new(
        MonitoringTransport::new(MonitoringTransportConfig::loopback_test(&server.url)),
        FakeSecretResolver {
            secret: canary.to_string(),
            revision: 3,
            current_revision: 3,
        },
    );

    let output = executor
        .execute(
            input(3, Instant::now() + Duration::from_secs(2)),
            CancellationToken::new(),
        )
        .await;

    assert_eq!(output.outcome, ProbeOutcome::Available);
    assert_eq!(output.failure_kind, None);
    assert_eq!(output.http_status, Some(200));
    assert_eq!(output.response_model, Some("gpt-primary".to_string()));
    assert!(output.output_bytes > 0);
    let debug = format!("{:?}", output.debug_summary);
    assert!(!debug.contains(canary));
    assert!(debug.contains("/v1/responses"));
    assert!(server
        .received
        .lock()
        .expect("received")
        .iter()
        .any(|request| request.contains(canary)));
}

#[tokio::test]
async fn codex_profile_reaches_upstream_with_a_codex_responses_request() {
    let canary = "rp-canary-secret-codex-profile";
    let server = spawn_server(|request| {
        Box::pin(async move {
            let (head, body) = request.split_once("\r\n\r\n").expect("HTTP request body");
            let lower_head = head.to_ascii_lowercase();
            assert!(head.starts_with("POST /v1/responses HTTP/1.1"));
            assert!(lower_head.contains("authorization: bearer rp-canary-secret-codex-profile"));
            assert!(lower_head.contains("openai-beta: responses=experimental"));
            assert!(lower_head.contains("user-agent: codex_cli_rs/0.146.0"));
            assert!(!lower_head.contains("x-stainless-"));

            let value: serde_json::Value = serde_json::from_str(body).expect("Codex request JSON");
            assert!(value["instructions"]
                .as_str()
                .is_some_and(|instructions| instructions.len() < 200));
            assert_eq!(value["input"][0]["type"], "message");
            assert_eq!(value["input"][0]["content"][0]["type"], "input_text");
            assert_eq!(value["tools"], serde_json::json!([]));
            assert_eq!(value["reasoning"]["effort"], "low");
            assert_eq!(value["store"], false);

            response(
                "200 OK",
                &[("content-type", "application/json")],
                r#"{
                    "status":"completed",
                    "model":"gpt-primary",
                    "output":[{"content":[{"type":"output_text","text":"RP_ANSWER=42"}]}]
                }"#,
            )
        })
    })
    .await;
    let executor = ProbeExecutor::new(
        MonitoringTransport::new(MonitoringTransportConfig::loopback_test(&server.url)),
        FakeSecretResolver {
            secret: canary.to_string(),
            revision: 3,
            current_revision: 3,
        },
    );

    let output = executor
        .execute(
            input_with_profile(
                3,
                Instant::now() + Duration::from_secs(2),
                ClientProfileId::CodexCliCompat,
            ),
            CancellationToken::new(),
        )
        .await;

    assert_eq!(output.outcome, ProbeOutcome::Available);
    assert_eq!(output.failure_kind, None);
    assert_eq!(output.http_status, Some(200));
    assert!(server
        .received
        .lock()
        .expect("received")
        .iter()
        .any(|request| request.contains("codex_cli_rs/0.146.0")));
}

#[tokio::test]
async fn claude_code_profile_reaches_upstream_with_required_cli_identity() {
    let canary = "rp-canary-secret-claude-profile";
    let server = spawn_server(|request| {
        Box::pin(async move {
            let (head, body) = request.split_once("\r\n\r\n").expect("HTTP request body");
            let lower_head = head.to_ascii_lowercase();
            assert!(head.starts_with("POST /v1/messages HTTP/1.1"));
            assert!(lower_head.contains("authorization: bearer rp-canary-secret-claude-profile"));
            assert!(lower_head.contains("user-agent: claude-cli/2.1.220 (external, cli)"));
            assert!(lower_head.contains("anthropic-beta: claude-code-20250219"));
            assert!(lower_head.contains("anthropic-version: 2023-06-01"));
            assert!(lower_head.contains("x-app: cli"));
            assert!(lower_head.contains("x-claude-code-session-id:"));
            assert!(lower_head.contains("x-client-request-id:"));

            let value: serde_json::Value =
                serde_json::from_str(body).expect("Claude Code request JSON");
            let metadata: serde_json::Value = serde_json::from_str(
                value["metadata"]["user_id"]
                    .as_str()
                    .expect("metadata user id"),
            )
            .expect("metadata user id JSON");
            assert_eq!(
                value["system"][0]["text"],
                "You are Claude Code, Anthropic's official CLI for Claude."
            );
            assert_eq!(value["messages"][0]["content"][0]["type"], "text");
            assert_eq!(value["tools"], serde_json::json!([]));
            assert_eq!(value["max_tokens"], 20);
            assert!(metadata["device_id"]
                .as_str()
                .is_some_and(|id| id.len() == 64));
            assert!(metadata["account_uuid"].as_str().is_some());
            assert!(metadata["session_id"].as_str().is_some());

            response(
                "200 OK",
                &[("content-type", "application/json")],
                r#"{
                    "id":"msg_probe",
                    "type":"message",
                    "role":"assistant",
                    "model":"claude-probe",
                    "content":[{"type":"text","text":"RP_ANSWER=42"}],
                    "stop_reason":"end_turn",
                    "usage":{"input_tokens":12,"output_tokens":4}
                }"#,
            )
        })
    })
    .await;
    let executor = ProbeExecutor::new(
        MonitoringTransport::new(MonitoringTransportConfig::loopback_test(&server.url)),
        FakeSecretResolver {
            secret: canary.to_string(),
            revision: 3,
            current_revision: 3,
        },
    );

    let output = executor
        .execute(
            input_for(
                3,
                Instant::now() + Duration::from_secs(2),
                ProtocolKind::AnthropicMessages,
                ClientProfileId::ClaudeCodeCompat,
                "claude-probe",
            ),
            CancellationToken::new(),
        )
        .await;

    assert_eq!(output.outcome, ProbeOutcome::Available);
    assert_eq!(output.failure_kind, None);
    assert!(!format!("{:?}", output.debug_summary).contains(canary));
}

#[tokio::test]
async fn gemini_cli_profile_uses_api_key_auth_and_resolves_the_model_path() {
    let canary = "rp-canary-secret-gemini-profile";
    let server = spawn_server(|request| {
        Box::pin(async move {
            let (head, body) = request.split_once("\r\n\r\n").expect("HTTP request body");
            let lower_head = head.to_ascii_lowercase();
            assert!(head.starts_with(
                "POST /v1beta/models/gemini-2.5-flash:generateContent HTTP/1.1"
            ));
            assert!(lower_head.contains("x-goog-api-key: rp-canary-secret-gemini-profile"));
            assert!(!lower_head.contains("authorization:"));
            assert!(lower_head.contains(
                "user-agent: geminicli/0.53.0/gemini-2.5-flash (win32; x64; cli)"
            ));

            let value: serde_json::Value =
                serde_json::from_str(body).expect("Gemini CLI request JSON");
            assert_eq!(value["generationConfig"]["temperature"], 0);
            assert_eq!(value["generationConfig"]["maxOutputTokens"], 256);
            assert_eq!(
                value["generationConfig"]["thinkingConfig"]["thinkingBudget"],
                0
            );
            assert!(value.get("cachedContent").is_none());

            response(
                "200 OK",
                &[("content-type", "application/json")],
                r#"{
                    "candidates":[{
                        "content":{"role":"model","parts":[{"text":"RP_ANSWER=42"}]},
                        "finishReason":"STOP"
                    }],
                    "modelVersion":"gemini-2.5-flash",
                    "usageMetadata":{"promptTokenCount":9,"candidatesTokenCount":4,"totalTokenCount":13}
                }"#,
            )
        })
    })
    .await;
    let executor = ProbeExecutor::new(
        MonitoringTransport::new(MonitoringTransportConfig::loopback_test(&server.url)),
        FakeSecretResolver {
            secret: canary.to_string(),
            revision: 3,
            current_revision: 3,
        },
    );

    let output = executor
        .execute(
            input_for(
                3,
                Instant::now() + Duration::from_secs(2),
                ProtocolKind::GeminiNative,
                ClientProfileId::GeminiCliCompat,
                "gemini-2.5-flash",
            ),
            CancellationToken::new(),
        )
        .await;

    assert_eq!(output.outcome, ProbeOutcome::Available);
    assert_eq!(output.failure_kind, None);
    assert!(!format!("{:?}", output.debug_summary).contains(canary));
}

#[tokio::test]
async fn executor_fails_closed_when_endpoint_revision_changes_before_writeback() {
    let server = spawn_server(|_| {
        Box::pin(async {
            response(
                "200 OK",
                &[("content-type", "application/json")],
                r#"{"status":"completed","output":[{"content":[{"type":"output_text","text":"RP_ANSWER=42"}]}]}"#,
            )
        })
    })
    .await;
    let executor = ProbeExecutor::new(
        MonitoringTransport::new(MonitoringTransportConfig::loopback_test(&server.url)),
        FakeSecretResolver {
            secret: "rp-canary-secret-revision".to_string(),
            revision: 3,
            current_revision: 4,
        },
    );

    let output = executor
        .execute(
            input(3, Instant::now() + Duration::from_secs(2)),
            CancellationToken::new(),
        )
        .await;

    assert_eq!(output.outcome, ProbeOutcome::Unavailable);
    assert_eq!(output.failure_kind, Some(FailureKind::Interrupted));
    assert!(format!("{:?}", output.debug_summary)
        .contains("endpoint_revision_changed_before_writeback"));
}

#[tokio::test]
async fn executor_rejects_an_outdated_profile_version_before_sending() {
    let executor = ProbeExecutor::new(
        MonitoringTransport::new(MonitoringTransportConfig::loopback_test(
            "http://127.0.0.1:9",
        )),
        FakeSecretResolver {
            secret: "must-not-be-sent".to_string(),
            revision: 3,
            current_revision: 3,
        },
    );
    let mut stale_input = input_with_profile(
        3,
        Instant::now() + Duration::from_secs(2),
        ClientProfileId::CodexCliCompat,
    );
    stale_input.client_profile_version = 1;

    let output = executor
        .execute(stale_input, CancellationToken::new())
        .await;

    assert_eq!(output.outcome, ProbeOutcome::Unavailable);
    assert_eq!(output.failure_kind, Some(FailureKind::NeedsConfiguration));
    assert!(output.request_profile_hash.is_empty());
}

fn input(endpoint_revision: i64, deadline_at: Instant) -> ProbeExecutionInput {
    input_with_profile(endpoint_revision, deadline_at, ClientProfileId::StandardApi)
}

fn input_with_profile(
    endpoint_revision: i64,
    deadline_at: Instant,
    client_profile_id: ClientProfileId,
) -> ProbeExecutionInput {
    input_for(
        endpoint_revision,
        deadline_at,
        ProtocolKind::OpenAiResponses,
        client_profile_id,
        "gpt-primary",
    )
}

fn input_for(
    endpoint_revision: i64,
    deadline_at: Instant,
    protocol_kind: ProtocolKind,
    client_profile_id: ClientProfileId,
    model: &str,
) -> ProbeExecutionInput {
    let client_profile_version = match client_profile_id {
        ClientProfileId::StandardApi | ClientProfileId::GrokCliCompat => 1,
        ClientProfileId::CodexCliCompat
        | ClientProfileId::ClaudeCodeCompat
        | ClientProfileId::GeminiCliCompat => 2,
    };
    ProbeExecutionInput {
        station_key_id: "key-1".to_string(),
        endpoint_revision,
        protocol_kind,
        client_profile_id,
        client_profile_version,
        model: model.to_string(),
        prompt: "Compute 20 + 22. Reply only RP_ANSWER=42".to_string(),
        validator: ChallengeValidator::from_expected_answer_for_tests("RP_ANSWER=42"),
        deadline_at,
        stream: false,
    }
}

fn response(status: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut output = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        output.push_str(name);
        output.push_str(": ");
        output.push_str(value);
        output.push_str("\r\n");
    }
    output.push_str("\r\n");
    output.push_str(body);
    output
}

struct FakeSecretResolver {
    secret: String,
    revision: i64,
    current_revision: i64,
}

impl ProbeSecretResolver for FakeSecretResolver {
    fn resolve_station_key_secret(
        &self,
        _station_key_id: &str,
    ) -> Result<ResolvedProbeSecret, FailureKind> {
        Ok(ResolvedProbeSecret {
            value: self.secret.clone(),
            endpoint_revision: self.revision,
        })
    }

    fn current_endpoint_revision(&self, _station_key_id: &str) -> Result<i64, FailureKind> {
        Ok(self.current_revision)
    }
}
