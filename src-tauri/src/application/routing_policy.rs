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
        routing::RoutingPolicy as LegacyRoutingPolicy,
        routing_policy::{
            RetryFailoverPolicyV2, RoutingPolicyConfigV1, RoutingPolicyConfigV2,
            RoutingPolicyFieldValidationError, MAX_CAPACITY_RETRY_WAIT_BUDGET_SECONDS_HARD_CAP,
            MAX_SAME_TARGET_CAPACITY_RETRIES_HARD_CAP, MAX_TOTAL_ATTEMPTS_HARD_CAP,
        },
    },
    persistence::stores::routing_policy_store::StoredRoutingPolicy,
};

use super::routing_engine::fixed_point::BasisPoints;

const MAX_CAPACITY_RETRY_WAIT_BUDGET_MILLIS_HARD_CAP: u64 =
    (MAX_CAPACITY_RETRY_WAIT_BUDGET_SECONDS_HARD_CAP * 1_000.0) as u64;

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
    /// Canonical active domain shape. V1 is accepted only by
    /// `RoutingPolicyConfigV2::from_stored_value` at this boundary.
    pub(crate) policy: RoutingPolicyConfigV2,
    pub(crate) revision: u64,
    pub(crate) policy_version: String,
    pub(crate) system_version: String,
    pub(crate) status: RoutingPolicyStatus,
    pub(crate) updated_at_ms: i64,
}

/// The single request-local attempt budget emitted by the policy compiler.
/// `max_total_attempts` includes the initial outbound attempt.  Consumers must
/// pass this immutable value through admission, execution, capacity retry and
/// trace rather than reading policy settings or defaults independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttemptBudgetProfileV1 {
    pub(crate) policy_revision: u64,
    pub(crate) max_total_attempts: u32,
    pub(crate) max_same_target_capacity_retries: u32,
    pub(crate) capacity_retry_wait_budget_ms: u64,
    pub(crate) allow_cross_capacity_domain_fallback: bool,
}

impl AttemptBudgetProfileV1 {
    pub(crate) fn from_policy(
        policy_revision: u64,
        retry_failover: &RetryFailoverPolicyV2,
    ) -> Result<Self, RoutingPolicyCompileError> {
        if policy_revision == 0 {
            return Err(RoutingPolicyCompileError::NotAdmitted(
                "revision_unavailable",
            ));
        }
        retry_failover
            .validate()
            .map_err(RoutingPolicyCompileError::InvalidField)?;
        let profile = Self {
            policy_revision,
            max_total_attempts: u32::from(retry_failover.max_total_attempts),
            max_same_target_capacity_retries: u32::from(
                retry_failover.max_same_target_capacity_retries,
            ),
            capacity_retry_wait_budget_ms: retry_failover.capacity_retry_wait_budget_millis(),
            allow_cross_capacity_domain_fallback: retry_failover
                .allow_cross_capacity_domain_fallback,
        };
        profile.validate()?;
        Ok(profile)
    }

    pub(crate) fn validate(&self) -> Result<(), RoutingPolicyCompileError> {
        if self.policy_revision == 0 {
            return Err(RoutingPolicyCompileError::NotAdmitted(
                "revision_unavailable",
            ));
        }
        if !(1..=u32::from(MAX_TOTAL_ATTEMPTS_HARD_CAP)).contains(&self.max_total_attempts) {
            return Err(RoutingPolicyCompileError::InvalidField(
                RoutingPolicyFieldValidationError {
                    field: "retryFailover.maxTotalAttempts",
                    code: "out_of_range",
                    message_key: "routing.retryFailover.maxTotalAttempts.range",
                },
            ));
        }
        if self.max_same_target_capacity_retries
            > u32::from(MAX_SAME_TARGET_CAPACITY_RETRIES_HARD_CAP)
        {
            return Err(RoutingPolicyCompileError::InvalidField(
                RoutingPolicyFieldValidationError {
                    field: "retryFailover.maxSameTargetCapacityRetries",
                    code: "out_of_range",
                    message_key: "routing.retryFailover.maxSameTargetCapacityRetries.range",
                },
            ));
        }
        if self.max_same_target_capacity_retries >= self.max_total_attempts {
            return Err(RoutingPolicyCompileError::InvalidField(
                RoutingPolicyFieldValidationError {
                    field: "retryFailover.maxSameTargetCapacityRetries",
                    code: "must_be_less_than_max_total_attempts",
                    message_key: "routing.retryFailover.maxSameTargetCapacityRetries.lessThanTotal",
                },
            ));
        }
        if self.capacity_retry_wait_budget_ms > MAX_CAPACITY_RETRY_WAIT_BUDGET_MILLIS_HARD_CAP {
            return Err(RoutingPolicyCompileError::InvalidField(
                RoutingPolicyFieldValidationError {
                    field: "retryFailover.capacityRetryWaitBudgetSeconds",
                    code: "out_of_range",
                    message_key: "routing.retryFailover.capacityRetryWaitBudgetSeconds.range",
                },
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum RoutingPolicyCompileError {
    #[error("routing policy JSON must be an object")]
    #[cfg(test)]
    NotAnObject,
    #[error("routing policy JSON is invalid: {0}")]
    InvalidConfig(String),
    #[error("routing policy field {0:?} is invalid")]
    InvalidField(RoutingPolicyFieldValidationError),
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
    pub(crate) attempt_budget: AttemptBudgetProfileV1,
    pub(crate) protection_profile: crate::application::health_protection::HealthProtectionProfileV1,
    pub(crate) protection_enabled: bool,
}

impl RoutingPolicyAggregate {
    pub(crate) fn from_stored(
        stored: StoredRoutingPolicy,
    ) -> Result<Self, RoutingPolicyCompileError> {
        let policy = RoutingPolicyConfigV2::from_stored_value(&stored.config)
            .map_err(|error| RoutingPolicyCompileError::InvalidField(error))?;
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
            policy,
            revision: stored.revision,
            policy_version: stored.policy_version,
            system_version: stored.system_version,
            status,
            updated_at_ms: stored.updated_at_ms,
        })
    }

    /// Compile the active aggregate through the V2 domain.  The aggregate is
    /// still represented by V1 until the storage cutover lands; this method is
    /// the explicit migration boundary and is the path new consumers should
    /// adopt.
    pub(crate) fn compile_v2(&self) -> Result<CompiledRoutingPolicy, RoutingPolicyCompileError> {
        if self.revision == 0 {
            return Err(RoutingPolicyCompileError::NotAdmitted(
                "revision_unavailable",
            ));
        }
        if self.status != RoutingPolicyStatus::Active {
            return Err(RoutingPolicyCompileError::NotAdmitted(self.status.as_str()));
        }
        compile_config_v2(
            &self.policy,
            self.revision,
            &self.policy_version,
            &self.system_version,
        )
    }
}

/// Compile a legacy V1 config for migration and regression fixtures. Production
/// callers compile the canonical V2 aggregate instead.
#[cfg(test)]
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
    compile_config_with_retry(
        config,
        &RetryFailoverPolicyV2::default(),
        source_revision,
        policy_version,
        system_version,
    )
}

#[cfg(test)]
pub(crate) fn compile_config_with_retry(
    config: &RoutingPolicyConfigV1,
    retry_failover: &RetryFailoverPolicyV2,
    source_revision: u64,
    policy_version: &str,
    system_version: &str,
) -> Result<CompiledRoutingPolicy, RoutingPolicyCompileError> {
    config
        .validate()
        .map_err(|error| RoutingPolicyCompileError::InvalidConfig(error.to_string()))?;
    let mut upgraded =
        RoutingPolicyConfigV2::from_v1(config).map_err(RoutingPolicyCompileError::InvalidField)?;
    upgraded.retry_failover = retry_failover.clone();
    compile_config_v2(&upgraded, source_revision, policy_version, system_version)
}

/// Compile a fully upgraded V2 policy.  This is deliberately independent of
/// storage, settings and runtime state so drafts and migration tests exercise
/// the same validation and profile generation as production.
pub(crate) fn compile_config_v2(
    config: &RoutingPolicyConfigV2,
    source_revision: u64,
    policy_version: &str,
    system_version: &str,
) -> Result<CompiledRoutingPolicy, RoutingPolicyCompileError> {
    if config.version != 2 {
        return Err(RoutingPolicyCompileError::UnknownVersion(config.version));
    }
    config
        .validate()
        .map_err(RoutingPolicyCompileError::InvalidField)?;
    if source_revision == 0 || policy_version.is_empty() || system_version.is_empty() {
        return Err(RoutingPolicyCompileError::NotAdmitted(
            "missing_policy_identity",
        ));
    }
    let bp = |value| BasisPoints::new(value).expect("validated basis-point field");
    let attempt_budget =
        AttemptBudgetProfileV1::from_policy(source_revision, &config.retry_failover)?;
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
        attempt_budget,
        protection_profile:
            crate::application::health_protection::HealthProtectionProfileV1::from_policy_config(
                &config.protection_profile,
            )
            .map_err(|_| {
                RoutingPolicyCompileError::InvalidField(RoutingPolicyFieldValidationError {
                    field: "protectionProfile",
                    code: "invalid_profile",
                    message_key: "routing.protectionProfile.invalid",
                })
            })?,
        protection_enabled: config.protection_profile.enabled,
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

#[cfg(test)]
pub(crate) fn compile_json_v2(
    config: &Value,
    source_revision: u64,
    policy_version: &str,
    system_version: &str,
) -> Result<CompiledRoutingPolicy, RoutingPolicyCompileError> {
    if !config.is_object() {
        return Err(RoutingPolicyCompileError::NotAnObject);
    }
    let typed = serde_json::from_value::<RoutingPolicyConfigV2>(config.clone())
        .map_err(|error| RoutingPolicyCompileError::InvalidConfig(error.to_string()))?;
    compile_config_v2(&typed, source_revision, policy_version, system_version)
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
    fn attempt_budget_profile_compiles_from_v2_and_carries_revision() {
        let config = RoutingPolicyConfigV2::default();
        let compiled =
            compile_config_v2(&config, 42, "v2", "system").expect("default v2 policy compiles");
        assert_eq!(compiled.attempt_budget.policy_revision, 42);
        assert_eq!(compiled.attempt_budget.max_total_attempts, 4);
        assert_eq!(compiled.attempt_budget.max_same_target_capacity_retries, 2);
        assert_eq!(compiled.attempt_budget.capacity_retry_wait_budget_ms, 2_000);
        assert!(compiled.attempt_budget.allow_cross_capacity_domain_fallback);
    }

    #[test]
    fn v1_compiler_is_baseline_equivalent_v2_upgrade_boundary() {
        let v1 = RoutingPolicyConfigV1::default();
        let compiled = compile_config(&v1, 9, "v1", "system").expect("default v1 compiles");
        let upgraded = RoutingPolicyConfigV2::from_v1(&v1).expect("v1 upgrades");
        let v2 = compile_config_v2(&upgraded, 9, "v2", "system").expect("upgraded v2 compiles");
        assert_eq!(compiled.attempt_budget, v2.attempt_budget);
        assert_eq!(compiled.reliability_weight, v2.reliability_weight);
        assert_eq!(compiled.max_candidates, v2.max_candidates);
    }

    #[test]
    fn compiler_rejects_retry_failover_before_any_runtime_profile_is_built() {
        let mut config = RoutingPolicyConfigV2::default();
        config.retry_failover.max_total_attempts = 1;
        config.retry_failover.max_same_target_capacity_retries = 1;
        let error = compile_config_v2(&config, 1, "v2", "system")
            .expect_err("same-target retry cannot consume the initial attempt");
        assert!(matches!(
            error,
            RoutingPolicyCompileError::InvalidField(error)
                if error.field == "retryFailover.maxSameTargetCapacityRetries"
        ));

        config = RoutingPolicyConfigV2::default();
        config.retry_failover.max_total_attempts = MAX_TOTAL_ATTEMPTS_HARD_CAP + 1;
        let error = compile_config_v2(&config, 1, "v2", "system")
            .expect_err("compiler must enforce platform hard cap");
        assert!(matches!(
            error,
            RoutingPolicyCompileError::InvalidField(error)
                if error.field == "retryFailover.maxTotalAttempts"
        ));
    }

    #[test]
    fn compile_json_v2_requires_nested_retry_failover_and_rejects_unknown_fields() {
        let mut json =
            serde_json::to_value(RoutingPolicyConfigV2::default()).expect("serialize v2 config");
        json["unknown"] = serde_json::json!(true);
        assert!(matches!(
            compile_json_v2(&json, 1, "v2", "system"),
            Err(RoutingPolicyCompileError::InvalidConfig(_))
        ));

        let mut missing =
            serde_json::to_value(RoutingPolicyConfigV2::default()).expect("serialize v2 config");
        missing
            .as_object_mut()
            .expect("config object")
            .remove("retryFailover");
        assert!(matches!(
            compile_json_v2(&missing, 1, "v2", "system"),
            Err(RoutingPolicyCompileError::InvalidConfig(_))
        ));
    }

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
