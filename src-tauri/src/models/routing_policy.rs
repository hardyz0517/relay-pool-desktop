use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RoutingPolicyConfigV1 {
    pub version: u16,
    pub reliability_weight: u16,
    pub responsiveness_weight: u16,
    pub cost_weight: u16,
    pub preference_weight: u16,
    pub max_candidates: u16,
    pub exploration_share_basis_points: u16,
    pub allow_depleted_fallback: bool,
    pub affinity_enabled: bool,
    pub affinity_ttl_seconds: u32,
}

impl Default for RoutingPolicyConfigV1 {
    fn default() -> Self {
        Self { version: 1, reliability_weight: 4_000, responsiveness_weight: 2_500, cost_weight: 2_000, preference_weight: 1_500, max_candidates: 64, exploration_share_basis_points: 500, allow_depleted_fallback: false, affinity_enabled: false, affinity_ttl_seconds: 300 }
    }
}

impl RoutingPolicyConfigV1 {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let total = u32::from(self.reliability_weight) + u32::from(self.responsiveness_weight) + u32::from(self.cost_weight) + u32::from(self.preference_weight);
        if self.version != 1 || self.max_candidates == 0 || self.max_candidates > 1_024 || total != 10_000 || self.exploration_share_basis_points > 2_000 || (self.affinity_enabled && !(1..=86_400).contains(&self.affinity_ttl_seconds)) { return Err("invalid routing policy"); }
        Ok(())
    }
}
