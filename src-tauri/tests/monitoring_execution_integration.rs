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

fn input(endpoint_revision: i64, deadline_at: Instant) -> ProbeExecutionInput {
    ProbeExecutionInput {
        station_key_id: "key-1".to_string(),
        endpoint_revision,
        protocol_kind: ProtocolKind::OpenAiResponses,
        client_profile_id: ClientProfileId::StandardApi,
        model: "gpt-primary".to_string(),
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
