//! Routing policy admission and compilation.
//!
//! The compiler deliberately accepts only the versioned, complete policy
//! configuration. Legacy strategy names are handled by the migration boundary
//! in `legacy_mapping`; they are never parsed by the runtime compiler.

#[cfg(test)]
use serde_json::Value;
use thiserror::Error;

use crate::{
    models::{
        routing::RoutingPolicy as LegacyRoutingPolicy, routing_policy::RoutingPolicyConfigV1,
    },
    persistence::stores::routing_policy_store::StoredRoutingPolicy,
};

use super::routing_engine::fixed_point::BasisPoints;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoutingPolicyStatus {
    Active,
    ConfigurationRequired,
    Invalid,
}

impl RoutingPolicyStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::ConfigurationRequired => "routing_configuration_required",
            Self::Invalid => "invalid",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoutingPolicyAggregate {
    pub(crate) config: RoutingPolicyConfigV1,
    pub(crate) revision: u64,
    pub(crate) policy_version: String,
    pub(crate) system_version: String,
    pub(crate) status: RoutingPolicyStatus,
    pub(crate) updated_at_ms: i64,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum RoutingPolicyCompileError {
    #[error("routing policy JSON must be an object")]
    #[cfg(test)]
    NotAnObject,
    #[error("routing policy JSON is invalid: {0}")]
    InvalidConfig(String),
    #[error("unsupported routing policy version {0}")]
    UnknownVersion(u16),
    #[error("routing policy is not admitted: {0}")]
    NotAdmitted(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledRoutingPolicy {
    pub(crate) source_revision: u64,
    pub(crate) policy_version: String,
    pub(crate) system_version: String,
    pub(crate) reliability_weight: BasisPoints,
    pub(crate) responsiveness_weight: BasisPoints,
    pub(crate) cost_weight: BasisPoints,
    pub(crate) preference_weight: BasisPoints,
    pub(crate) max_candidates: u16,
    pub(crate) exploration_share_basis_points: BasisPoints,
    pub(crate) allow_depleted_fallback: bool,
    pub(crate) affinity_enabled: bool,
    pub(crate) affinity_ttl_seconds: u32,
}

impl RoutingPolicyAggregate {
    pub(crate) fn from_stored(
        stored: StoredRoutingPolicy,
    ) -> Result<Self, RoutingPolicyCompileError> {
        let config = serde_json::from_value::<RoutingPolicyConfigV1>(stored.config)
            .map_err(|error| RoutingPolicyCompileError::InvalidConfig(error.to_string()))?;
        let status = match stored.status.as_str() {
            "active" => RoutingPolicyStatus::Active,
            "routing_configuration_required" => RoutingPolicyStatus::ConfigurationRequired,
            "invalid" => RoutingPolicyStatus::Invalid,
            other => {
                return Err(RoutingPolicyCompileError::InvalidConfig(format!(
                    "unknown status {other}"
                )))
            }
        };
        Ok(Self {
            config,
            revision: stored.revision,
            policy_version: stored.policy_version,
            system_version: stored.system_version,
            status,
            updated_at_ms: stored.updated_at_ms,
        })
    }

    pub(crate) fn compile(&self) -> Result<CompiledRoutingPolicy, RoutingPolicyCompileError> {
        if self.revision == 0 {
            return Err(RoutingPolicyCompileError::NotAdmitted(
                "revision_unavailable",
            ));
        }
        if self.status != RoutingPolicyStatus::Active {
            return Err(RoutingPolicyCompileError::NotAdmitted(self.status.as_str()));
        }
        compile_config(
            &self.config,
            self.revision,
            &self.policy_version,
            &self.system_version,
        )
    }
}

/// Compile a complete V1 config. This is also used by draft simulation and
/// therefore has no database, settings, secret, or runtime dependencies.
pub(crate) fn compile_config(
    config: &RoutingPolicyConfigV1,
    source_revision: u64,
    policy_version: &str,
    system_version: &str,
) -> Result<CompiledRoutingPolicy, RoutingPolicyCompileError> {
    if config.version != 1 {
        return Err(RoutingPolicyCompileError::UnknownVersion(config.version));
    }
    config
        .validate()
        .map_err(|error| RoutingPolicyCompileError::InvalidConfig(error.to_string()))?;
    if source_revision == 0 || policy_version.is_empty() || system_version.is_empty() {
        return Err(RoutingPolicyCompileError::NotAdmitted(
            "missing_policy_identity",
        ));
    }
    let bp = |value| BasisPoints::new(value).expect("validated basis-point field");
    Ok(CompiledRoutingPolicy {
        source_revision,
        policy_version: policy_version.to_owned(),
        system_version: system_version.to_owned(),
        reliability_weight: bp(config.reliability_weight),
        responsiveness_weight: bp(config.responsiveness_weight),
        cost_weight: bp(config.cost_weight),
        preference_weight: bp(config.preference_weight),
        max_candidates: config.max_candidates,
        exploration_share_basis_points: bp(config.exploration_share_basis_points),
        allow_depleted_fallback: config.allow_depleted_fallback,
        affinity_enabled: config.affinity_enabled,
        affinity_ttl_seconds: config.affinity_ttl_seconds,
    })
}

/// Parse only the canonical V1 object. Legacy strategy names are handled by
/// the migration boundary and are not accepted here.
#[cfg(test)]
pub(crate) fn compile_json(
    config: &Value,
    source_revision: u64,
    policy_version: &str,
    system_version: &str,
) -> Result<CompiledRoutingPolicy, RoutingPolicyCompileError> {
    if !config.is_object() {
        return Err(RoutingPolicyCompileError::NotAnObject);
    }
    let typed = serde_json::from_value::<RoutingPolicyConfigV1>(config.clone())
        .map_err(|error| RoutingPolicyCompileError::InvalidConfig(error.to_string()))?;
    compile_config(&typed, source_revision, policy_version, system_version)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LegacyPolicyMapping {
    pub(crate) legacy: LegacyRoutingPolicy,
    pub(crate) preset: RoutingPolicyConfigV1,
    pub(crate) preserved_fields: &'static [&'static str],
    pub(crate) lost_semantics: &'static str,
    pub(crate) routing_configuration_required: bool,
    pub(crate) reason: &'static str,
}

/// Explicit migration matrix for all six V1 strategy values. Each result is
/// intentionally distinct, even where the old selector happened to share an
/// ordering implementation, so semantic loss cannot be hidden by a merge.
pub(crate) fn legacy_policy_mapping(policy: LegacyRoutingPolicy) -> LegacyPolicyMapping {
    let mut preset = RoutingPolicyConfigV1::default();
    let (preserved, lost, required, reason) = match policy {
        LegacyRoutingPolicy::AutomaticBalanced => (
            &[
                "weights",
                "max_candidates",
                "exploration_share",
                "fallback",
                "affinity",
            ][..],
            "none; automatic balancing is represented by the default factor weights",
            false,
            "directly representable by V1",
        ),
        LegacyRoutingPolicy::PriorityFallback => {
            preset.reliability_weight = 5_000;
            preset.responsiveness_weight = 2_500;
            preset.cost_weight = 1_000;
            preset.preference_weight = 1_500;
            (
                &["priority ordering", "fallback", "weights"][..],
                "legacy priority tie-break is not retained",
                false,
                "priority becomes preference/reliability weighting",
            )
        }
        LegacyRoutingPolicy::StableFirst => {
            preset.reliability_weight = 5_000;
            preset.responsiveness_weight = 2_500;
            preset.cost_weight = 1_000;
            preset.preference_weight = 1_500;
            preset.affinity_enabled = true;
            (
                &["weights", "affinity"][..],
                "legacy stable queue order is not retained",
                false,
                "stability is represented by affinity and reliability",
            )
        }
        LegacyRoutingPolicy::BackupOnly => {
            preset.reliability_weight = 3_500;
            preset.responsiveness_weight = 2_000;
            preset.cost_weight = 1_500;
            preset.preference_weight = 3_000;
            (
                &["weights"][..],
                "backup-only tier semantics require candidate role metadata",
                true,
                "V1 cannot infer backup role from a policy name",
            )
        }
        LegacyRoutingPolicy::CheapFirst => {
            preset.reliability_weight = 2_000;
            preset.responsiveness_weight = 1_000;
            preset.cost_weight = 6_000;
            preset.preference_weight = 1_000;
            (
                &["weights", "max_candidates"][..],
                "unbounded legacy price comparator is not retained",
                false,
                "cost preference becomes a bounded factor",
            )
        }
        LegacyRoutingPolicy::CostStableFirst => {
            preset.reliability_weight = 2_500;
            preset.responsiveness_weight = 1_500;
            preset.cost_weight = 4_500;
            preset.preference_weight = 1_500;
            preset.affinity_enabled = true;
            (
                &["weights", "affinity"][..],
                "legacy cost/stability tie-break order is not retained",
                false,
                "cost and stability become independent factors",
            )
        }
    };
    LegacyPolicyMapping {
        legacy: policy,
        preset,
        preserved_fields: preserved,
        lost_semantics: lost,
        routing_configuration_required: required,
        reason,
    }
}

#[cfg(test)]
pub(crate) fn legacy_policy_mappings() -> [LegacyPolicyMapping; 6] {
    [
        LegacyRoutingPolicy::AutomaticBalanced,
        LegacyRoutingPolicy::PriorityFallback,
        LegacyRoutingPolicy::StableFirst,
        LegacyRoutingPolicy::BackupOnly,
        LegacyRoutingPolicy::CheapFirst,
        LegacyRoutingPolicy::CostStableFirst,
    ]
    .map(legacy_policy_mapping)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiler_rejects_unknown_version_and_partial_identity() {
        let mut config = RoutingPolicyConfigV1::default();
        config.version = 2;
        assert!(matches!(
            compile_config(&config, 1, "v1", "system"),
            Err(RoutingPolicyCompileError::UnknownVersion(2))
        ));
        assert!(matches!(
            compile_config(&RoutingPolicyConfigV1::default(), 0, "v1", "system"),
            Err(RoutingPolicyCompileError::NotAdmitted(
                "missing_policy_identity"
            ))
        ));
    }

    #[test]
    fn six_legacy_values_have_explicit_non_silent_matrix() {
        let mappings = legacy_policy_mappings();
        assert_eq!(mappings.len(), 6);
        assert!(mappings
            .iter()
            .all(|mapping| !mapping.reason.is_empty() && !mapping.lost_semantics.is_empty()));
        assert!(mappings
            .iter()
            .any(|mapping| mapping.routing_configuration_required));
        for (index, left) in mappings.iter().enumerate() {
            assert!(mappings[index + 1..]
                .iter()
                .all(|right| left.preset != right.preset));
        }
    }

    #[test]
    fn compiler_never_reads_legacy_settings() {
        let value = serde_json::json!({"legacy_strategy": "cheap_first"});
        assert!(matches!(
            compile_json(&value, 1, "v1", "system"),
            Err(RoutingPolicyCompileError::InvalidConfig(_))
        ));
    }
}
