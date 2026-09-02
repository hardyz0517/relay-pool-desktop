//! Typed impact classification for V3 routing policy changes.
//!
//! This module is the single owner of the policy fields that affect runtime
//! generation components.  Keep the comparison explicit: when a new public
//! V3 field is added, it must be assigned to one of the projections below and
//! the exhaustive tests should be updated before it can enter the fast path.

use crate::models::routing_policy::RoutingPolicyConfigV3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoutingPolicyImpactLevel {
    Noop,
    PolicyOnlyFastCandidate,
    CircuitRebuild,
    QualityRebuild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RoutingPolicyImpact {
    pub(crate) level: RoutingPolicyImpactLevel,
    quality_changed: bool,
    circuit_changed: bool,
    pub(crate) transport_changed: bool,
}

impl RoutingPolicyImpact {
    pub(crate) fn compare(active: &RoutingPolicyConfigV3, target: &RoutingPolicyConfigV3) -> Self {
        assert_all_fields_classified(active);
        let quality_changed = active.reliability_source_weights
            != target.reliability_source_weights
            || active.reliability_sampling != target.reliability_sampling;
        let circuit_changed = active.retry.consecutive_failure_threshold
            != target.retry.consecutive_failure_threshold
            || active.circuit_breaker != target.circuit_breaker;
        let transport_changed = active.timeout_policy != target.timeout_policy
            || active.outbound_proxy_mode != target.outbound_proxy_mode
            || active.outbound_proxy_url != target.outbound_proxy_url;

        let policy_changed = active.version != target.version
            || active.reliability_weight != target.reliability_weight
            || active.responsiveness_weight != target.responsiveness_weight
            || active.cost_weight != target.cost_weight
            || active.preference_weight != target.preference_weight
            || active.allow_depleted_fallback != target.allow_depleted_fallback
            || active.affinity_enabled != target.affinity_enabled
            || active.affinity_ttl_seconds != target.affinity_ttl_seconds
            || active.max_rate_multiplier != target.max_rate_multiplier
            || active.routing_group_filter != target.routing_group_filter
            || active.retry.max_retry_count != target.retry.max_retry_count;

        let level = if quality_changed {
            RoutingPolicyImpactLevel::QualityRebuild
        } else if circuit_changed {
            RoutingPolicyImpactLevel::CircuitRebuild
        } else if policy_changed || transport_changed {
            RoutingPolicyImpactLevel::PolicyOnlyFastCandidate
        } else {
            RoutingPolicyImpactLevel::Noop
        };

        Self {
            level,
            quality_changed,
            circuit_changed,
            transport_changed,
        }
    }

    pub(crate) const fn is_policy_only_fast_candidate(self) -> bool {
        matches!(
            self.level,
            RoutingPolicyImpactLevel::PolicyOnlyFastCandidate
        ) && !self.transport_changed
    }

    pub(crate) const fn quality_rebuild(self) -> bool {
        self.quality_changed
    }

    pub(crate) const fn circuit_rebuild(self) -> bool {
        self.circuit_changed
    }
}

/// Compile-time guard for the impact owner. Adding a public V3 field must
/// update this exhaustive pattern before the policy can compile again.
fn assert_all_fields_classified(policy: &RoutingPolicyConfigV3) {
    let RoutingPolicyConfigV3 {
        version: _,
        reliability_weight: _,
        responsiveness_weight: _,
        cost_weight: _,
        preference_weight: _,
        allow_depleted_fallback: _,
        affinity_enabled: _,
        affinity_ttl_seconds: _,
        max_rate_multiplier: _,
        routing_group_filter: _,
        outbound_proxy_mode: _,
        outbound_proxy_url: _,
        reliability_source_weights:
            crate::models::routing_policy::ReliabilitySourceWeightsV3 {
                real_traffic_percent: _,
                monitoring_percent: _,
            },
        reliability_sampling:
            crate::models::routing_policy::ReliabilitySamplingPolicyV3 {
                historical_minimum_samples: _,
                recent_minimum_samples: _,
                optimistic_reliability_percent: _,
                optimistic_latency_ms: _,
            },
        retry:
            crate::models::routing_policy::RetryPolicyV3 {
                version: _,
                max_retry_count: _,
                consecutive_failure_threshold: _,
            },
        circuit_breaker:
            crate::models::routing_policy::CircuitBreakerPolicyV3 {
                version: _,
                recovery_success_threshold: _,
                recovery_wait_seconds: _,
            },
        timeout_policy:
            crate::models::routing_policy::TimeoutPolicyV2 {
                version: _,
                connect_seconds: _,
                first_byte_seconds: _,
                precommit_seconds: _,
                buffered_execution_seconds: _,
                stream_idle_seconds: _,
            },
    } = policy;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(mutator: impl FnOnce(&mut RoutingPolicyConfigV3)) -> RoutingPolicyImpact {
        let active = RoutingPolicyConfigV3::default();
        let mut target = active.clone();
        mutator(&mut target);
        RoutingPolicyImpact::compare(&active, &target)
    }

    #[test]
    fn policy_only_fields_are_fast_candidates() {
        // Use a concrete filter change rather than relying on enum defaults.
        let mut target = RoutingPolicyConfigV3::default();
        target.routing_group_filter =
            crate::models::routing::RoutingGroupFilter::GroupBindingId("group".to_string());
        let impact = RoutingPolicyImpact::compare(&RoutingPolicyConfigV3::default(), &target);
        assert_eq!(
            impact.level,
            RoutingPolicyImpactLevel::PolicyOnlyFastCandidate
        );
        assert!(impact.is_policy_only_fast_candidate());
        assert!(!impact.quality_rebuild());
        assert!(!impact.circuit_rebuild());
    }

    #[test]
    fn every_non_transport_runtime_field_is_classified_as_a_fast_candidate() {
        use crate::models::routing_policy::RoutingGroupFilter;

        // Keep this list explicit and reviewable.  Adding a new public V3
        // field requires adding it here (and assigning it in `compare`) before
        // it can accidentally enter the fast path.
        let mut cases: Vec<(&str, Box<dyn FnOnce(&mut RoutingPolicyConfigV3)>)> = vec![
            (
                "reliability_weight",
                Box::new(|policy| {
                    policy.reliability_weight = policy.reliability_weight.saturating_sub(100)
                }),
            ),
            (
                "responsiveness_weight",
                Box::new(|policy| {
                    policy.responsiveness_weight = policy.responsiveness_weight.saturating_sub(100)
                }),
            ),
            (
                "cost_weight",
                Box::new(|policy| policy.cost_weight = policy.cost_weight.saturating_sub(100)),
            ),
            (
                "preference_weight",
                Box::new(|policy| {
                    policy.preference_weight = policy.preference_weight.saturating_add(100)
                }),
            ),
            (
                "allow_depleted_fallback",
                Box::new(|policy| policy.allow_depleted_fallback = !policy.allow_depleted_fallback),
            ),
            (
                "affinity_enabled",
                Box::new(|policy| policy.affinity_enabled = !policy.affinity_enabled),
            ),
            (
                "affinity_ttl_seconds",
                Box::new(|policy| {
                    policy.affinity_ttl_seconds = policy.affinity_ttl_seconds.saturating_add(1)
                }),
            ),
            (
                "max_rate_multiplier",
                Box::new(|policy| {
                    policy.max_rate_multiplier = Some(
                        policy
                            .max_rate_multiplier
                            .unwrap_or(1.0)
                            .mul_add(0.1, policy.max_rate_multiplier.unwrap_or(1.0)),
                    )
                }),
            ),
            (
                "routing_group_filter",
                Box::new(|policy| {
                    policy.routing_group_filter =
                        RoutingGroupFilter::GroupBindingId("test-group-binding".to_string())
                }),
            ),
            (
                "retry.max_retry_count",
                Box::new(|policy| {
                    policy.retry.max_retry_count = policy.retry.max_retry_count.saturating_add(1)
                }),
            ),
        ];

        for (field, mutate) in cases.drain(..) {
            let impact = classify(mutate);
            assert_eq!(
                impact.level,
                RoutingPolicyImpactLevel::PolicyOnlyFastCandidate,
                "{field} must remain on the policy-only lane"
            );
            assert!(
                impact.is_policy_only_fast_candidate(),
                "{field} must be eligible for the initial fast path"
            );
            assert!(!impact.transport_changed, "{field} is not transport state");
            assert!(!impact.quality_rebuild(), "{field} must not replay quality");
            assert!(!impact.circuit_rebuild(), "{field} must not replay circuit");
        }
    }

    #[test]
    fn quality_and_circuit_fields_never_enter_fast_path() {
        let quality = classify(|policy| {
            policy.reliability_sampling.recent_minimum_samples = policy
                .reliability_sampling
                .recent_minimum_samples
                .saturating_add(1)
        });
        assert_eq!(quality.level, RoutingPolicyImpactLevel::QualityRebuild);
        assert!(!quality.is_policy_only_fast_candidate());

        let circuit = classify(|policy| {
            policy.retry.consecutive_failure_threshold =
                policy.retry.consecutive_failure_threshold.saturating_add(1)
        });
        assert_eq!(circuit.level, RoutingPolicyImpactLevel::CircuitRebuild);
        assert!(!circuit.is_policy_only_fast_candidate());

        let mut combined = RoutingPolicyConfigV3::default();
        combined.reliability_sampling.recent_minimum_samples += 1;
        combined.retry.consecutive_failure_threshold += 1;
        let combined_impact =
            RoutingPolicyImpact::compare(&RoutingPolicyConfigV3::default(), &combined);
        assert_eq!(
            combined_impact.level,
            RoutingPolicyImpactLevel::QualityRebuild
        );
        assert!(combined_impact.quality_rebuild());
        assert!(combined_impact.circuit_rebuild());
        assert!(!combined_impact.is_policy_only_fast_candidate());
    }

    #[test]
    fn transport_is_not_enabled_on_initial_fast_path() {
        let mut target = RoutingPolicyConfigV3::default();
        target.timeout_policy.connect_seconds += 1.0;
        let impact = RoutingPolicyImpact::compare(&RoutingPolicyConfigV3::default(), &target);
        assert_eq!(
            impact.level,
            RoutingPolicyImpactLevel::PolicyOnlyFastCandidate
        );
        assert!(impact.transport_changed);
        assert!(!impact.is_policy_only_fast_candidate());
    }

    #[test]
    fn unchanged_policy_is_noop() {
        let policy = RoutingPolicyConfigV3::default();
        let impact = RoutingPolicyImpact::compare(&policy, &policy);
        assert_eq!(impact.level, RoutingPolicyImpactLevel::Noop);
        assert!(!impact.is_policy_only_fast_candidate());
    }
}
