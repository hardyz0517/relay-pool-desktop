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
            document_sync_store::StoredDocumentSync,
            routing_policy_store::StoredRoutingPolicy,
            routing_policy_v3_stage_upgrade::{self, StoredRoutingPolicyPublication},
        },
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoutingPolicyPublicationStatus {
    Staged,
    Ready,
    WaitingLatestInput,
    Failed,
    Active,
    Expired,
}

impl RoutingPolicyPublicationStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Staged => "staged",
            Self::Ready => "ready",
            Self::WaitingLatestInput => "waiting_latest_input",
            Self::Failed => "failed",
            Self::Active => "active",
            Self::Expired => "expired",
        }
    }

    pub(crate) const fn terminal(self) -> bool {
        matches!(self, Self::Failed | Self::Active | Self::Expired)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingPolicyPublication {
    pub(crate) revision: u64,
    pub(crate) policy_generation_id: Option<String>,
    pub(crate) status: RoutingPolicyPublicationStatus,
    pub(crate) failure_code: Option<&'static str>,
    pub(crate) activation_path: Option<&'static str>,
    pub(crate) runtime_status: Option<&'static str>,
    pub(crate) active_revision: Option<u64>,
    pub(crate) fallback_reason: Option<&'static str>,
    pub(crate) updated_at_ms: i64,
    pub(crate) terminal: bool,
}

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
        routing_policy_v3_stage_upgrade::load_effective_active_in(read.connection())
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

    pub(crate) async fn load_routing_policy_publication(
        &self,
        revision: u64,
        expected_policy_generation_id: Option<&str>,
    ) -> Result<RoutingPolicyPublication, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let stored = routing_policy_v3_stage_upgrade::load_publication_by_revision(
            read.connection(),
            revision,
        )
        .await
        .map_err(ApplicationError::from)?;
        let active_revision =
            routing_policy_v3_stage_upgrade::load_effective_active_in(read.connection())
                .await
                .map_err(ApplicationError::from)?
                .map(|policy| policy.revision);
        Ok(publication_from_stored(
            revision,
            expected_policy_generation_id,
            stored,
            active_revision,
        )?)
    }
}

fn publication_from_stored(
    revision: u64,
    expected_policy_generation_id: Option<&str>,
    stored: Option<StoredRoutingPolicyPublication>,
    active_revision: Option<u64>,
) -> Result<RoutingPolicyPublication, ApplicationError> {
    let Some(stored) = stored else {
        return Ok(expired_publication(
            revision,
            expected_policy_generation_id.map(str::to_owned),
            active_revision,
            0,
        ));
    };
    if expected_policy_generation_id.is_some_and(|expected| expected != stored.policy_generation_id)
    {
        return Ok(expired_publication(
            revision,
            expected_policy_generation_id.map(str::to_owned),
            active_revision,
            stored.policy_updated_at_ms,
        ));
    }

    let updated_at_ms = stored
        .runtime_updated_at_ms
        .map_or(stored.policy_updated_at_ms, |runtime| {
            runtime.max(stored.policy_updated_at_ms)
        });
    let (status, failure_code, fallback_reason) = match stored.policy_status.as_str() {
        "active" => (RoutingPolicyPublicationStatus::Active, None, None),
        "retired" => (RoutingPolicyPublicationStatus::Expired, None, None),
        "failed" => publication_failure(stored.policy_failure_code.as_deref()),
        "staged" | "ready" => match stored.runtime_status.as_deref() {
            None if stored.policy_status == "staged" => {
                (RoutingPolicyPublicationStatus::Staged, None, None)
            }
            None => (RoutingPolicyPublicationStatus::Ready, None, None),
            Some("building") => (RoutingPolicyPublicationStatus::Staged, None, None),
            Some("ready" | "cutover_fencing") => {
                (RoutingPolicyPublicationStatus::Ready, None, None)
            }
            Some("active") => (RoutingPolicyPublicationStatus::Active, None, None),
            Some("retired") => (RoutingPolicyPublicationStatus::Expired, None, None),
            Some("failed") => publication_failure(stored.runtime_failure_code.as_deref()),
            Some(_) => return Err(ApplicationError::Internal),
        },
        _ => return Err(ApplicationError::Internal),
    };
    let activation_path = (!matches!(
        status,
        RoutingPolicyPublicationStatus::Active | RoutingPolicyPublicationStatus::Expired
    ))
    .then_some("generation");
    Ok(RoutingPolicyPublication {
        revision: stored.revision,
        policy_generation_id: Some(stored.policy_generation_id),
        status,
        failure_code,
        activation_path,
        runtime_status: Some(status.as_str()),
        active_revision,
        fallback_reason,
        updated_at_ms,
        terminal: status.terminal(),
    })
}

fn expired_publication(
    revision: u64,
    policy_generation_id: Option<String>,
    active_revision: Option<u64>,
    updated_at_ms: i64,
) -> RoutingPolicyPublication {
    RoutingPolicyPublication {
        revision,
        policy_generation_id,
        status: RoutingPolicyPublicationStatus::Expired,
        failure_code: None,
        activation_path: None,
        runtime_status: Some(RoutingPolicyPublicationStatus::Expired.as_str()),
        active_revision,
        fallback_reason: None,
        updated_at_ms,
        terminal: true,
    }
}

fn publication_failure(
    code: Option<&str>,
) -> (
    RoutingPolicyPublicationStatus,
    Option<&'static str>,
    Option<&'static str>,
) {
    match code {
        Some("superseded_by_input_tail") => (
            RoutingPolicyPublicationStatus::WaitingLatestInput,
            None,
            Some("quality_tail"),
        ),
        Some("superseded_by_fence_tail") => (
            RoutingPolicyPublicationStatus::WaitingLatestInput,
            None,
            Some("fence_active"),
        ),
        _ => (
            RoutingPolicyPublicationStatus::Failed,
            Some(sanitize_failure_code(code)),
            None,
        ),
    }
}

fn sanitize_failure_code(code: Option<&str>) -> &'static str {
    match code {
        Some(
            "build_failed"
            | "generation_build_failed"
            | "quality_rebuild_failed"
            | "circuit_rebuild_failed",
        ) => "generation_build_failed",
        Some(
            "qualification_failed"
            | "generation_qualification_failed"
            | "comparison_failed"
            | "replay_failed",
        ) => "generation_qualification_failed",
        Some(
            "cutover_failed"
            | "generation_cutover_failed"
            | "cutover_timeout"
            | "cutover_cas_failed",
        ) => "generation_cutover_failed",
        _ => "generation_failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(policy_status: &str, runtime_status: Option<&str>) -> StoredRoutingPolicyPublication {
        StoredRoutingPolicyPublication {
            revision: 7,
            policy_generation_id: "pg1_fixture".into(),
            policy_status: policy_status.into(),
            policy_failure_code: None,
            policy_updated_at_ms: 10,
            runtime_status: runtime_status.map(str::to_owned),
            runtime_failure_code: None,
            runtime_updated_at_ms: runtime_status.map(|_| 20),
        }
    }

    #[test]
    fn publication_maps_durable_policy_and_runtime_states() {
        for (policy, runtime, expected, terminal) in [
            (
                "staged",
                None,
                RoutingPolicyPublicationStatus::Staged,
                false,
            ),
            (
                "staged",
                Some("building"),
                RoutingPolicyPublicationStatus::Staged,
                false,
            ),
            (
                "ready",
                Some("ready"),
                RoutingPolicyPublicationStatus::Ready,
                false,
            ),
            (
                "ready",
                Some("cutover_fencing"),
                RoutingPolicyPublicationStatus::Ready,
                false,
            ),
            (
                "ready",
                Some("active"),
                RoutingPolicyPublicationStatus::Active,
                true,
            ),
            (
                "active",
                Some("active"),
                RoutingPolicyPublicationStatus::Active,
                true,
            ),
            (
                "retired",
                Some("retired"),
                RoutingPolicyPublicationStatus::Expired,
                true,
            ),
        ] {
            let publication =
                publication_from_stored(7, None, Some(stored(policy, runtime)), Some(6))
                    .expect("valid publication state");
            assert_eq!(publication.status, expected);
            assert_eq!(publication.terminal, terminal);
            assert_eq!(publication.active_revision, Some(6));
            assert_eq!(publication.runtime_status, Some(expected.as_str()));
        }
    }

    #[test]
    fn superseded_generations_wait_for_latest_input_without_becoming_terminal_failures() {
        for (failure_code, fallback_reason) in [
            ("superseded_by_input_tail", "quality_tail"),
            ("superseded_by_fence_tail", "fence_active"),
        ] {
            let mut value = stored("ready", Some("failed"));
            value.runtime_failure_code = Some(failure_code.into());
            let publication = publication_from_stored(7, None, Some(value), Some(6))
                .expect("superseded publication state");
            assert_eq!(
                publication.status,
                RoutingPolicyPublicationStatus::WaitingLatestInput
            );
            assert_eq!(publication.failure_code, None);
            assert_eq!(publication.fallback_reason, Some(fallback_reason));
            assert_eq!(publication.activation_path, Some("generation"));
            assert!(!publication.terminal);
        }
    }

    #[test]
    fn failed_runtime_wins_over_ready_policy_and_sanitizes_failure_codes() {
        let mut arbitrary = stored("ready", Some("failed"));
        arbitrary.runtime_failure_code = Some("raw exception with sensitive detail".into());
        let generic = publication_from_stored(7, None, Some(arbitrary), Some(6))
            .expect("generic failed publication state");
        assert_eq!(generic.status, RoutingPolicyPublicationStatus::Failed);
        assert_eq!(generic.failure_code, Some("generation_failed"));
        assert_eq!(generic.fallback_reason, None);
        assert!(generic.terminal);
    }

    #[test]
    fn missing_or_mismatched_generation_is_terminally_expired() {
        let missing = publication_from_stored(7, Some("pg1_missing"), None, Some(6))
            .expect("missing publication is an expired result");
        assert_eq!(missing.status, RoutingPolicyPublicationStatus::Expired);
        assert_eq!(missing.policy_generation_id.as_deref(), Some("pg1_missing"));
        assert_eq!(missing.active_revision, Some(6));

        let mismatched =
            publication_from_stored(7, Some("pg1_old"), Some(stored("staged", None)), Some(6))
                .expect("mismatched publication is an expired result");
        assert_eq!(mismatched.status, RoutingPolicyPublicationStatus::Expired);
        assert_eq!(mismatched.policy_generation_id.as_deref(), Some("pg1_old"));
    }
}
