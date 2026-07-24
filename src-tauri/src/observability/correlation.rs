use std::future::Future;

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

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) fn current() -> Option<CorrelationId> {
    CURRENT_CORRELATION_ID.try_with(Clone::clone).ok()
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
}
