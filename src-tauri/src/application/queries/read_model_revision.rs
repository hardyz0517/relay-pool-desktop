use crate::persistence::{error::PersistenceError, stores::asset_revision_store::AssetRevisionStore, ReadSession};

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "contract=read-model.domain-revision-notice; owner=application/queries/read_model_revision; remove_when=all mutation facades emit typed revision notices")
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DomainRevisionNotice {
    pub(crate) mutation_id: String,
    pub(crate) affected_scopes: Vec<String>,
    pub(crate) revision_vector: Vec<(String, i64)>,
}

/// Read-model responses carry the highest durable source revision observed by
/// the query. It is computed from the same read transaction as the rows.
pub(crate) async fn load_asset_revision(read: &mut ReadSession) -> Result<i64, PersistenceError> {
    AssetRevisionStore.load(read.connection()).await
}
