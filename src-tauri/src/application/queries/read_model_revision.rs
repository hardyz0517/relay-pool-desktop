use std::sync::OnceLock;

use tokio::sync::broadcast;

use crate::persistence::{
    error::PersistenceError, stores::asset_revision_store::AssetRevisionStore, ReadSession,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DomainRevisionNotice {
    pub(crate) mutation_id: String,
    pub(crate) affected_scopes: Vec<String>,
    pub(crate) revision_vector: Vec<(String, i64)>,
}

const REVISION_NOTICE_CAPACITY: usize = 128;

static REVISION_NOTICES: OnceLock<broadcast::Sender<DomainRevisionNotice>> = OnceLock::new();

fn revision_notice_sender() -> &'static broadcast::Sender<DomainRevisionNotice> {
    REVISION_NOTICES.get_or_init(|| {
        let (sender, _) = broadcast::channel(REVISION_NOTICE_CAPACITY);
        sender
    })
}

/// Subscribe to low-latency revision hints for read-model consumers.
///
/// Notices are deliberately best-effort. Consumers must re-read their source
/// aggregate and compare the returned revision before using cached data.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=read-model.domain-revision-notice; owner=frontend/bridge subscription; remove_when=frontend bridge consumes typed revision notices"
    )
)]
pub(crate) fn subscribe_domain_revision_notices() -> broadcast::Receiver<DomainRevisionNotice> {
    revision_notice_sender().subscribe()
}

/// Publish a committed mutation after its transaction has been closed. A
/// receiver that has fallen behind is expected to perform a normal revision
/// reconciliation rather than relying on every individual notice.
pub(crate) fn publish_domain_revision_notice(notice: DomainRevisionNotice) {
    let _ = revision_notice_sender().send(notice);
}

impl DomainRevisionNotice {
    pub(crate) fn for_scope(scope: impl Into<String>, revision: i64) -> Self {
        let scope = scope.into();
        Self {
            mutation_id: uuid::Uuid::now_v7().to_string(),
            affected_scopes: vec![scope.clone()],
            revision_vector: vec![(scope, revision)],
        }
    }
}

/// Read-model responses carry the highest durable source revision observed by
/// the query. It is computed from the same read transaction as the rows.
pub(crate) async fn load_asset_revision(read: &mut ReadSession) -> Result<i64, PersistenceError> {
    AssetRevisionStore.load(read.connection()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_notice_is_bounded_and_identifies_scope() {
        let mut receiver = subscribe_domain_revision_notices();
        let notice = DomainRevisionNotice::for_scope("model_mapping", 7);
        publish_domain_revision_notice(notice.clone());
        let received = receiver.try_recv().expect("revision notice");
        assert_eq!(received.affected_scopes, vec!["model_mapping"]);
        assert_eq!(received.revision_vector, vec![("model_mapping".into(), 7)]);
        assert!(!received.mutation_id.is_empty());
    }
}
