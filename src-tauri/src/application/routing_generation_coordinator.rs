use crate::{
    application::routing_generation::{
        validate_new_runtime_generation, RoutingGenerationIdentityError,
    },
    models::routing_generation::{
        NewRoutingRuntimeGeneration, RoutingGenerationFence, RoutingGenerationQualification,
        RoutingGenerationRegistrySnapshot,
    },
    persistence::{
        error::PersistenceError, runtime::PersistenceHandle,
        stores::routing_generation_store::RoutingGenerationStore,
    },
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum RoutingGenerationCoordinatorError {
    #[error("routing generation identity is invalid")]
    InvalidIdentity,
    #[error("routing generation policy fingerprint does not match staged policy")]
    PolicyFingerprintMismatch,
    #[error("routing generation registry is corrupt")]
    RegistryCorrupt,
    #[error("routing generation compare-and-swap conflict")]
    Conflict,
    #[error("routing generation has not passed activation qualification")]
    NotQualified,
    #[error("routing generation cutover is waiting for admitted attempts to finish")]
    CutoverBusy,
    #[error("routing generation persistence is unavailable")]
    Persistence(#[source] PersistenceError),
}

impl From<RoutingGenerationIdentityError> for RoutingGenerationCoordinatorError {
    fn from(_: RoutingGenerationIdentityError) -> Self {
        Self::InvalidIdentity
    }
}

impl From<PersistenceError> for RoutingGenerationCoordinatorError {
    fn from(error: PersistenceError) -> Self {
        match error {
            PersistenceError::InvariantViolation(detail)
                if detail.starts_with("routing_generation_registry_corrupt:") =>
            {
                Self::RegistryCorrupt
            }
            PersistenceError::InvariantViolation(detail)
                if detail == "routing_generation_not_qualified" =>
            {
                Self::NotQualified
            }
            PersistenceError::RevisionConflict(_) => Self::Conflict,
            error => Self::Persistence(error),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RoutingGenerationCoordinator {
    runtime: PersistenceHandle,
    store: RoutingGenerationStore,
}

impl RoutingGenerationCoordinator {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            store: RoutingGenerationStore,
        }
    }

    pub(crate) async fn inspect(
        &self,
    ) -> Result<RoutingGenerationRegistrySnapshot, RoutingGenerationCoordinatorError> {
        let mut read = self.runtime.begin_read().await?;
        self.store
            .load_registry_snapshot(read.connection())
            .await
            .map_err(Into::into)
    }

    /// Register a complete component tuple.  The row is first inserted as
    /// `building`; component metadata and canonical policy hashes are checked
    /// again in the same transaction before it becomes `ready`.
    pub(crate) async fn register_ready_generation(
        &self,
        generation: &NewRoutingRuntimeGeneration,
        now_ms: i64,
    ) -> Result<(), RoutingGenerationCoordinatorError> {
        validate_new_runtime_generation(generation, true)?;
        let mut write = self.runtime.begin_write().await?;
        let policy_match = self
            .store
            .policy_fingerprint_matches(write.connection(), generation)
            .await;
        if !policy_match? {
            return Err(RoutingGenerationCoordinatorError::PolicyFingerprintMismatch);
        }
        let insert_result = self
            .store
            .insert_building_runtime_generation(write.connection(), generation)
            .await;
        insert_result?;
        let ready_result = self
            .store
            .mark_runtime_generation_ready(
                write.connection(),
                &generation.runtime_generation_id,
                now_ms,
            )
            .await;
        ready_result?;
        write.commit().await?;
        Ok(())
    }

    pub(crate) async fn begin_cutover(
        &self,
        target_runtime_generation_id: &str,
        expected_active_runtime_generation_id: Option<&str>,
        now_ms: i64,
    ) -> Result<RoutingGenerationFence, RoutingGenerationCoordinatorError> {
        let mut write = self.runtime.begin_write().await?;
        let fence = self
            .store
            .begin_fence(
                write.connection(),
                target_runtime_generation_id,
                expected_active_runtime_generation_id,
                false,
                None,
                now_ms,
            )
            .await?;
        write.commit().await?;
        Ok(fence)
    }

    pub(crate) async fn record_qualification(
        &self,
        qualification: &RoutingGenerationQualification,
    ) -> Result<(), RoutingGenerationCoordinatorError> {
        let mut write = self.runtime.begin_write().await?;
        self.store
            .record_qualification(write.connection(), qualification)
            .await?;
        write.commit().await?;
        Ok(())
    }

    pub(crate) async fn complete_cutover(
        &self,
        fence: &RoutingGenerationFence,
        now_ms: i64,
    ) -> Result<(), RoutingGenerationCoordinatorError> {
        self.activate(fence, false, now_ms).await
    }

    pub(crate) async fn retarget_cutover(
        &self,
        fence: &RoutingGenerationFence,
        replacement_runtime_generation_id: &str,
        now_ms: i64,
    ) -> Result<RoutingGenerationFence, RoutingGenerationCoordinatorError> {
        let mut write = self.runtime.begin_write().await?;
        let replacement = self
            .store
            .retarget_fence(
                write.connection(),
                fence,
                replacement_runtime_generation_id,
                now_ms,
            )
            .await?;
        write.commit().await?;
        Ok(replacement)
    }

    pub(crate) async fn abort_cutover(
        &self,
        fence: &RoutingGenerationFence,
        reason_code: &str,
        now_ms: i64,
    ) -> Result<(), RoutingGenerationCoordinatorError> {
        let mut write = self.runtime.begin_write().await?;
        self.store
            .abort_fence(write.connection(), fence, false, reason_code, now_ms)
            .await?;
        write.commit().await?;
        Ok(())
    }

    /// Rollback never reconstructs a policy from legacy history.  The target
    /// must be a complete retired v3 runtime generation selected from the
    /// registry after its quality/circuit tail replay has been validated.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=v3-rollback-control-plane; owner=application/routing_generation_coordinator; remove_when=rollback is exposed through the production recovery command"
        )
    )]
    pub(crate) async fn begin_rollback(
        &self,
        target_runtime_generation_id: Option<&str>,
        expected_active_runtime_generation_id: &str,
        reason_code: &str,
        now_ms: i64,
    ) -> Result<RoutingGenerationFence, RoutingGenerationCoordinatorError> {
        let mut write = self.runtime.begin_write().await?;
        let target = match target_runtime_generation_id {
            Some(target) => target.to_string(),
            None => {
                self.store
                    .latest_rollback_candidate(write.connection())
                    .await?
                    .ok_or(RoutingGenerationCoordinatorError::Conflict)?
                    .runtime_generation_id
            }
        };
        let fence = self
            .store
            .begin_fence(
                write.connection(),
                &target,
                Some(expected_active_runtime_generation_id),
                true,
                Some(reason_code),
                now_ms,
            )
            .await?;
        write.commit().await?;
        Ok(fence)
    }

    pub(crate) async fn complete_rollback(
        &self,
        fence: &RoutingGenerationFence,
        now_ms: i64,
    ) -> Result<(), RoutingGenerationCoordinatorError> {
        self.activate(fence, true, now_ms).await
    }

    pub(crate) async fn abort_rollback(
        &self,
        fence: &RoutingGenerationFence,
        reason_code: &str,
        now_ms: i64,
    ) -> Result<(), RoutingGenerationCoordinatorError> {
        let mut write = self.runtime.begin_write().await?;
        self.store
            .abort_fence(write.connection(), fence, true, reason_code, now_ms)
            .await?;
        write.commit().await?;
        Ok(())
    }

    async fn activate(
        &self,
        fence: &RoutingGenerationFence,
        rollback: bool,
        now_ms: i64,
    ) -> Result<(), RoutingGenerationCoordinatorError> {
        let mut write = self.runtime.begin_write().await?;
        let pending = self
            .store
            .count_pending_admitted_attempts(write.connection())
            .await?;
        if pending != 0 {
            return Err(RoutingGenerationCoordinatorError::CutoverBusy);
        }
        let target = self
            .store
            .load_runtime_generation(write.connection(), &fence.target_runtime_generation_id)
            .await?
            .ok_or(RoutingGenerationCoordinatorError::Conflict)?;
        let staged = self
            .store
            .load_staged_policy_json(write.connection(), &target.policy_generation_id)
            .await?;
        let policy_hash = crate::application::routing_generation::canonical_json_sha256(&staged)?;
        if policy_hash != target.policy_input_hash || policy_hash != target.policy_content_hash {
            return Err(RoutingGenerationCoordinatorError::PolicyFingerprintMismatch);
        }
        self.store
            .activate_fenced_generation(write.connection(), fence, rollback, now_ms)
            .await?;
        write.commit().await?;
        Ok(())
    }
}
