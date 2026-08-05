use crate::{
    application::routing_engine::{
        algorithm_profile::DispatchAlgorithmProfile,
        planning_snapshot::{CandidateSnapshot, PlanningSnapshot, RuntimeOverlaySnapshot},
    },
    models::routing_policy::RoutingPolicyConfigV1,
    persistence::{stores::operational_facts::OperationalFactStore, ReadSession},
};

use super::{
    reader::{OperationalFactReadError, OperationalFactReader, OperationalFactSource},
    OperationalFactReadOptions,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum PlanningSnapshotBuildError {
    #[error("operational facts unavailable: {0}")]
    Facts(#[from] OperationalFactReadError),
    #[error("planning snapshot is invalid: {0}")]
    Invalid(&'static str),
}

/// Builds the immutable durable half of a routing plan from one caller-owned
/// read transaction. Runtime capacity and circuit state are supplied explicitly
/// as an overlay so this builder never opens a second transaction.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PlanningSnapshotBuilder;

impl PlanningSnapshotBuilder {
    pub(crate) async fn build(
        &self,
        read: &mut ReadSession,
        options: &OperationalFactReadOptions,
        policy: RoutingPolicyConfigV1,
        profile: DispatchAlgorithmProfile,
        runtime: RuntimeOverlaySnapshot,
    ) -> Result<PlanningSnapshot, PlanningSnapshotBuildError> {
        let reader = OperationalFactReader::new(OperationalFactStore);
        let facts = reader.load_bundle(read, options).await?;
        let durable_revision = [
            facts.version_vector().max_station_revision(),
            facts.version_vector().max_key_revision(),
            facts.version_vector().max_settings_revision(),
            facts.version_vector().max_alias_revision(),
        ]
        .into_iter()
        .max()
        .unwrap_or(1) as u64;
        let candidates = facts
            .candidates()
            .iter()
            .map(|candidate| CandidateSnapshot {
                station_key_id: candidate.station_key_id().as_str().to_string(),
                station_id: candidate.station_id().as_str().to_string(),
                endpoint_revision: candidate.endpoint().endpoint_ref().revision().get(),
                credential_available: candidate.credential().available(),
                capability_basis_points: 10_000,
                // No monitor row is neutral prior evidence, not a failed key.
                reliability_basis_points: 5_000,
                responsiveness_basis_points: 5_000,
                cost_basis_points: None,
                preference_basis_points: 0,
                failure_domains: vec![
                    format!("station:{}", candidate.station_id().as_str()),
                    format!("key:{}", candidate.station_key_id().as_str()),
                ],
            })
            .collect();
        let snapshot = PlanningSnapshot {
            snapshot_id: facts.snapshot_id().as_str().to_string(),
            durable_revision,
            policy,
            profile,
            candidates,
            runtime,
        };
        snapshot
            .validate()
            .map_err(PlanningSnapshotBuildError::Invalid)?;
        Ok(snapshot)
    }
}

// Keep the source bound visible at the composition boundary. This prevents a
// future caller from silently replacing the transactional fact source with a
// page-specific query facade.
fn _source_contract<S: OperationalFactSource>() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_type_is_bound_to_the_operational_source() {
        _source_contract::<OperationalFactStore>();
    }
}
