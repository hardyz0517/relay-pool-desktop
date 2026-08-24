//! Read-only access to the persisted routing policy and its managed-document
//! synchronization metadata.
//!
//! Policy writes and runtime activation belong to
//! [`RoutingPolicyMutationCoordinator`]. This service intentionally exposes
//! only the two durable reads needed by command and proxy-startup callers.

use crate::{
    application::error::ApplicationError,
    models::document_sync::ROUTING_POLICY_DOCUMENT_KIND,
    persistence::{
        runtime::PersistenceHandle,
        stores::{
            document_sync_store::StoredDocumentSync, routing_policy_store::StoredRoutingPolicy,
        },
    },
};

#[derive(Clone)]
pub(crate) struct RoutingPolicyReadService {
    runtime: PersistenceHandle,
}

impl RoutingPolicyReadService {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self { runtime }
    }

    pub(crate) async fn load_routing_policy(
        &self,
    ) -> Result<StoredRoutingPolicy, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        crate::persistence::stores::routing_policy_store::RoutingPolicyStore
            .load(read.connection())
            .await
            .map_err(ApplicationError::from)?
            .ok_or(ApplicationError::NotFound)
    }

    pub(crate) async fn load_routing_policy_document_sync(
        &self,
    ) -> Result<Option<StoredDocumentSync>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        crate::persistence::stores::document_sync_store::DocumentSyncStore
            .load(read.connection(), ROUTING_POLICY_DOCUMENT_KIND)
            .await
            .map_err(ApplicationError::from)
    }
}
