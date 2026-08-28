use std::future::Future;
use std::time::Instant;

use sha2::{Digest, Sha256};
use tracing::Instrument;

use crate::observability::runtime::{InteractionId, StableEventCode};
use crate::observability::runtime_context::{
    IpcRuntimeContextV1, RuntimeContextRegistry, ValidatedRuntimeContext,
};

pub(crate) const CORRELATION_ID_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CorrelationId(String);

tokio::task_local! {
    static CURRENT_CORRELATION_ID: CorrelationId;
    static CURRENT_INTERACTION_ID: Option<InteractionId>;
}

impl CorrelationId {
    pub(crate) fn new() -> Self {
        let value = uuid::Uuid::now_v7().simple().to_string();
        debug_assert_eq!(value.len(), CORRELATION_ID_BYTES);
        Self(value)
    }

    pub(crate) fn for_proxy_request(request_id: &str) -> Self {
        Self::from_stable_parts("proxy.request", request_id)
    }

    fn from_stable_parts(scope: &str, value: &str) -> Self {
        let digest = Sha256::digest([scope.as_bytes(), b"\0", value.as_bytes()].concat());
        let value = format!("{digest:x}");
        let value = value[..CORRELATION_ID_BYTES].to_string();
        debug_assert_eq!(value.len(), CORRELATION_ID_BYTES);
        Self(value)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) fn current() -> Option<CorrelationId> {
    CURRENT_CORRELATION_ID.try_with(Clone::clone).ok()
}

pub(crate) fn current_id_string() -> Option<String> {
    current().map(|id| id.as_str().to_string())
}

pub(crate) fn current_interaction() -> Option<InteractionId> {
    CURRENT_INTERACTION_ID.try_with(Clone::clone).ok().flatten()
}

#[cfg(test)]
pub(crate) fn current_interaction_id_string() -> Option<String> {
    current_interaction().map(|id| id.as_str().to_owned())
}

pub(crate) fn current_or_new() -> CorrelationId {
    current().unwrap_or_else(CorrelationId::new)
}

pub(crate) async fn in_scope<T>(
    span_name: &'static str,
    correlation_id: CorrelationId,
    future: impl Future<Output = T>,
) -> T {
    let interaction_id = current_interaction();
    in_scope_with_interaction(span_name, correlation_id, interaction_id, future).await
}

/// Enters a work scope with an explicitly captured interaction context.
///
/// Tokio task-local values are scoped to the task that polls a future; they
/// are not inherited by a newly spawned task. Child work therefore captures
/// the interaction at its admission boundary and passes it here explicitly.
/// Keeping this API separate from `in_scope` makes accidental ambient
/// propagation at independent scheduler boundaries visible to callers.
pub(crate) async fn in_scope_with_interaction<T>(
    span_name: &'static str,
    correlation_id: CorrelationId,
    interaction_id: Option<InteractionId>,
    future: impl Future<Output = T>,
) -> T {
    StableEventCode::new(span_name).expect("work span scope must be a stable public code");
    let span = tracing::info_span!(
        "work.scope",
        scope = span_name,
        correlation_id = correlation_id.as_str()
    );
    CURRENT_CORRELATION_ID
        .scope(correlation_id, async move {
            CURRENT_INTERACTION_ID
                .scope(interaction_id, future.instrument(span))
                .await
        })
        .await
}

pub(crate) fn with_scope<T>(
    span_name: &'static str,
    correlation_id: CorrelationId,
    operation: impl FnOnce() -> T,
) -> T {
    StableEventCode::new(span_name).expect("work span scope must be a stable public code");
    let span = tracing::info_span!(
        "work.scope",
        scope = span_name,
        correlation_id = correlation_id.as_str()
    );
    let _entered = span.enter();
    CURRENT_CORRELATION_ID.sync_scope(correlation_id, || {
        let interaction_id = current_interaction();
        CURRENT_INTERACTION_ID.sync_scope(interaction_id, operation)
    })
}

#[cfg(test)]
pub(crate) async fn in_command_scope<T>(
    command: &'static str,
    future: impl Future<Output = T>,
) -> T {
    in_command_scope_with_interaction(command, None, future).await
}

/// The command-boundary entry point for frontend runtime metadata.
///
/// Tauri receives the metadata as an opaque JSON value on purpose: malformed
/// capability input must not prevent the business command from running. The
/// value is parsed and admitted only in this function. Every rejection is
/// reduced to a fixed diagnostic code and the command continues with a null
/// interaction id; rejected metadata is never copied into a span or event.
pub(crate) async fn in_command_scope_with_runtime_context<T>(
    command: &'static str,
    registry: &RuntimeContextRegistry,
    runtime_context: Option<serde_json::Value>,
    future: impl Future<Output = T>,
) -> T {
    let validated = runtime_context.and_then(|value| {
        let parsed = serde_json::from_value::<IpcRuntimeContextV1>(value).ok();
        let Some(parsed) = parsed else {
            crate::observability::runtime::bootstrap::emit_rate_limited(
                crate::ipc::runtime_events::runtime_context_invalid(),
            );
            return None;
        };
        match registry.validate(Some(&parsed), Instant::now()) {
            Ok(validated) => Some(validated),
            Err(_) => {
                crate::observability::runtime::bootstrap::emit_rate_limited(
                    crate::ipc::runtime_events::runtime_context_invalid(),
                );
                None
            }
        }
    });
    in_command_scope_with_interaction(command, validated, future).await
}

/// Command boundary helper used by the IPC adapter once runtime context is
/// available. Invalid context is intentionally handled by the caller; this
/// helper only propagates an already validated value and never changes
/// correlation identity semantics.
pub(crate) async fn in_command_scope_with_interaction<T>(
    command: &'static str,
    runtime_context: Option<ValidatedRuntimeContext>,
    future: impl Future<Output = T>,
) -> T {
    StableEventCode::from_command_name(command)
        .expect("command span scope must be a public command identifier");
    let correlation_id = CorrelationId::new();
    let interaction_id = runtime_context.and_then(|context| context.interaction_id);
    let span = tracing::info_span!(
        "ipc.command",
        command,
        correlation_id = correlation_id.as_str(),
        interaction_id = interaction_id
            .as_ref()
            .map(InteractionId::as_str)
            .unwrap_or("null")
    );
    CURRENT_CORRELATION_ID
        .scope(correlation_id, async move {
            CURRENT_INTERACTION_ID
                .scope(interaction_id, future.instrument(span))
                .await
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::runtime_context::RuntimeContextRegistry;

    async fn application_boundary() -> (CorrelationId, CorrelationId) {
        let application_id = current().expect("application receives command correlation");
        tokio::task::yield_now().await;
        let outbound_id = current().expect("outbound receives application correlation");
        (application_id, outbound_id)
    }

    #[tokio::test]
    async fn command_application_and_outbound_share_one_bounded_identifier() {
        let (command_id, application_id, outbound_id) =
            in_command_scope("fixture_command", async {
                let command_id = current().expect("command correlation");
                let (application_id, outbound_id) = application_boundary().await;
                (command_id, application_id, outbound_id)
            })
            .await;

        assert_eq!(command_id, application_id);
        assert_eq!(application_id, outbound_id);
        assert_eq!(command_id.as_str().len(), CORRELATION_ID_BYTES);
        assert!(command_id
            .as_str()
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()));
        assert!(current().is_none(), "command scope must not leak");
    }

    #[tokio::test]
    async fn command_scope_with_validated_interaction_propagates_without_leaking() {
        let registry = RuntimeContextRegistry::new();
        let raw = IpcRuntimeContextV1 {
            context_session_id: registry.context_session_id().to_owned(),
            interaction_id: Some("int_0123456789abcdef0123456789abcdef".to_owned()),
        };
        let context = registry
            .validate(Some(&raw), std::time::Instant::now())
            .expect("fixture interaction validates");
        let observed = in_command_scope_with_interaction("fixture_command", Some(context), async {
            current_interaction_id_string()
        })
        .await;

        assert_eq!(
            observed.as_deref(),
            Some("int_0123456789abcdef0123456789abcdef")
        );
        assert!(
            current_interaction().is_none(),
            "interaction scope must not leak"
        );
    }

    #[tokio::test]
    async fn invalid_runtime_context_is_dropped_without_changing_command_result() {
        let registry = RuntimeContextRegistry::new();
        let observed = in_command_scope_with_runtime_context(
            "fixture_command",
            &registry,
            Some(serde_json::json!({
                "contextSessionId": "ctx_invalid",
                "interactionId": "int_invalid"
            })),
            async { (current_interaction_id_string(), 42u8) },
        )
        .await;
        assert_eq!(observed, (None, 42));
        assert!(
            current_interaction().is_none(),
            "interaction scope must not leak"
        );
    }

    #[tokio::test]
    async fn valid_runtime_context_reaches_child_scope() {
        let registry = RuntimeContextRegistry::new();
        let session = registry.context_session_id().to_owned();
        let observed = in_command_scope_with_runtime_context(
            "fixture_command",
            &registry,
            Some(serde_json::json!({
                "contextSessionId": session,
                "interactionId": "int_0123456789abcdef0123456789abcdef"
            })),
            async { current_interaction_id_string() },
        )
        .await;
        assert_eq!(
            observed.as_deref(),
            Some("int_0123456789abcdef0123456789abcdef")
        );
    }

    #[tokio::test]
    async fn command_scope_accepts_secret_domain_terms_in_static_command_names() {
        let observed = in_command_scope("upsert_common_login_password", async {
            current().expect("command correlation")
        })
        .await;

        assert_eq!(observed.as_str().len(), CORRELATION_ID_BYTES);
        assert!(current().is_none(), "command scope must not leak");
    }

    #[tokio::test]
    async fn explicit_work_scope_preserves_parent_correlation_across_spawn() {
        let (parent_id, child_id) = in_command_scope("fixture_command", async {
            let parent_id = current().expect("parent correlation");
            let inherited = current_or_new();
            let child = tokio::spawn(async move {
                in_scope("task.run", inherited, async {
                    current().expect("spawned work correlation")
                })
                .await
            })
            .await
            .expect("spawned work joins");
            (parent_id, child)
        })
        .await;

        assert_eq!(parent_id, child_id);
        assert!(current().is_none(), "work scope must not leak");
    }

    #[tokio::test]
    async fn explicitly_captured_interaction_reaches_spawned_work_scope() {
        let registry = RuntimeContextRegistry::new();
        let observed = in_command_scope_with_runtime_context(
            "fixture_command",
            &registry,
            Some(serde_json::json!({
                "contextSessionId": registry.context_session_id(),
                "interactionId": "int_0123456789abcdef0123456789abcdef"
            })),
            async {
                let parent_interaction = current_interaction();
                let parent_correlation = current().expect("parent correlation");
                let expected_interaction = parent_interaction.clone();
                let child = tokio::spawn(async move {
                    in_scope_with_interaction(
                        "task.run",
                        parent_correlation,
                        parent_interaction,
                        async { (current_id_string(), current_interaction_id_string()) },
                    )
                    .await
                })
                .await
                .expect("spawned work joins");
                (child, expected_interaction.map(|id| id.as_str().to_owned()))
            },
        )
        .await;

        assert_eq!(
            observed.0 .1.as_deref(),
            Some("int_0123456789abcdef0123456789abcdef")
        );
        assert_eq!(observed.0 .0.as_deref().map(str::len), Some(32));
        assert_eq!(observed.0 .1, observed.1);
    }

    #[test]
    fn proxy_request_correlation_is_deterministic_bounded_and_redacted() {
        let request_id = "req_0198108c8411_00003039_0000000000000001";
        let first = CorrelationId::for_proxy_request(request_id);
        let second = CorrelationId::for_proxy_request(request_id);

        assert_eq!(first, second);
        assert_eq!(first.as_str().len(), CORRELATION_ID_BYTES);
        assert!(first.as_str().bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first.as_str(), request_id);
        assert!(
            !first.as_str().contains("req_"),
            "public request ids must not be copied into correlation labels"
        );
    }

    #[test]
    fn synchronous_scope_sets_correlation_without_leaking() {
        let correlation_id = CorrelationId::for_proxy_request("req_fixture");

        let observed = with_scope("proxy.request.body", correlation_id.clone(), || {
            current().expect("synchronous body poll receives correlation")
        });

        assert_eq!(observed, correlation_id);
        assert!(current().is_none(), "synchronous scope must not leak");
    }

    #[tokio::test]
    #[should_panic(expected = "command span scope must be a public command identifier")]
    async fn command_scope_rejects_secret_or_url_shaped_span_fields() {
        in_command_scope("https://example.test/v1?token=secret", async {}).await;
    }

    #[test]
    #[should_panic(expected = "work span scope must be a stable public code")]
    fn work_scope_rejects_secret_or_path_shaped_span_fields() {
        let correlation_id = CorrelationId::for_proxy_request("req_fixture");
        with_scope("C:\\Users\\cpp_s\\relay-pool.db", correlation_id, || ());
    }
}
