use std::future::Future;

use sha2::{Digest, Sha256};
use tracing::Instrument;

pub(crate) const CORRELATION_ID_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CorrelationId(String);

tokio::task_local! {
    static CURRENT_CORRELATION_ID: CorrelationId;
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

pub(crate) fn current_or_new() -> CorrelationId {
    current().unwrap_or_else(CorrelationId::new)
}

pub(crate) async fn in_scope<T>(
    span_name: &'static str,
    correlation_id: CorrelationId,
    future: impl Future<Output = T>,
) -> T {
    let span = tracing::info_span!(
        "work.scope",
        scope = span_name,
        correlation_id = correlation_id.as_str()
    );
    CURRENT_CORRELATION_ID
        .scope(correlation_id, future.instrument(span))
        .await
}

pub(crate) fn with_scope<T>(
    span_name: &'static str,
    correlation_id: CorrelationId,
    operation: impl FnOnce() -> T,
) -> T {
    let span = tracing::info_span!(
        "work.scope",
        scope = span_name,
        correlation_id = correlation_id.as_str()
    );
    let _entered = span.enter();
    CURRENT_CORRELATION_ID.sync_scope(correlation_id, operation)
}

pub(crate) async fn in_command_scope<T>(
    command: &'static str,
    future: impl Future<Output = T>,
) -> T {
    let correlation_id = CorrelationId::new();
    let span = tracing::info_span!(
        "ipc.command",
        command,
        correlation_id = correlation_id.as_str()
    );
    CURRENT_CORRELATION_ID
        .scope(correlation_id, future.instrument(span))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
