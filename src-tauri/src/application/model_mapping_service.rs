//! Application owner for model-mapping persistence and document lifecycle.
//!
//! Model mapping is used by routing, but its document/CAS lifecycle is not a
//! routing-planner concern. Keeping this narrow service separate prevents the
//! command facade from reaching into the broad routing aggregate for mapping
//! writes and history reads.

use crate::{
    application::{error::ApplicationError, model_mapping::ModelMappingDocumentSyncSnapshot},
    models::{document_sync::TrustedDocumentSource, model_mapping::ModelMappingDocumentV1},
    persistence::{
        runtime::PersistenceHandle,
        stores::model_mapping_store::{ModelMappingStore, StoredLegacyModelAliasReview},
    },
};

#[derive(Clone)]
pub(crate) struct ModelMappingService {
    runtime: PersistenceHandle,
}

impl ModelMappingService {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self { runtime }
    }

    pub(crate) async fn apply_document(
        &self,
        document: ModelMappingDocumentV1,
        source: TrustedDocumentSource,
    ) -> Result<ModelMappingDocumentV1, ApplicationError> {
        crate::application::model_mapping::persist_document(self.runtime.clone(), document, source)
            .await
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn restore_document(
        &self,
        document: ModelMappingDocumentV1,
        expected_revision: u64,
    ) -> Result<ModelMappingDocumentV1, ApplicationError> {
        crate::application::model_mapping::persist_document_at_revision(
            self.runtime.clone(),
            document,
            expected_revision,
            TrustedDocumentSource::history_restore(),
        )
        .await
        .map_err(ApplicationError::from)
    }

    pub(crate) async fn load_history_document(
        &self,
        revision: u64,
    ) -> Result<Option<String>, ApplicationError> {
        let revision =
            i64::try_from(revision).map_err(|_| ApplicationError::ConstraintViolation)?;
        let mut read = self.runtime.begin_read().await?;
        ModelMappingStore
            .load_history_revision(read.connection(), revision)
            .await
            .map(|item| item.map(|item| item.document_json))
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn list_legacy_reviews(
        &self,
    ) -> Result<Vec<StoredLegacyModelAliasReview>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        ModelMappingStore
            .list_legacy_reviews(read.connection())
            .await
            .map_err(ApplicationError::from)
    }

    pub(crate) async fn reconcile_document_sync(
        &self,
    ) -> Result<ModelMappingDocumentSyncSnapshot, ApplicationError> {
        crate::application::model_mapping::reconcile_model_mapping_document_sync(
            self.runtime.clone(),
        )
        .await
        .map_err(ApplicationError::from)
    }
}
