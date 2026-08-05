use super::fixed_point::BasisPoints;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DispatchAlgorithmProfile {
    pub(crate) version: u16,
    pub(crate) reliability_prior_alpha: u32,
    pub(crate) reliability_prior_beta: u32,
    pub(crate) reliability_decay_basis_points_per_hour: u16,
    pub(crate) minimum_sample_mass_basis_points: u32,
    pub(crate) exploit_band_basis_points: u16,
    pub(crate) exploration_share_basis_points: u16,
    pub(crate) latency_cap_ms: u32,
    pub(crate) seed_domain: &'static str,
}
impl Default for DispatchAlgorithmProfile {
    fn default() -> Self {
        Self {
            version: 1,
            reliability_prior_alpha: 2,
            reliability_prior_beta: 2,
            reliability_decay_basis_points_per_hour: 9_500,
            minimum_sample_mass_basis_points: 10_000,
            exploit_band_basis_points: 500,
            exploration_share_basis_points: 500,
            latency_cap_ms: 120_000,
            seed_domain: "relay-pool-routing/v1",
        }
    }
}
impl DispatchAlgorithmProfile {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1
            || self.reliability_prior_alpha == 0
            || self.reliability_prior_beta == 0
            || self.reliability_decay_basis_points_per_hour == 0
            || self.reliability_decay_basis_points_per_hour > 10_000
            || self.minimum_sample_mass_basis_points == 0
            || self.exploit_band_basis_points > 5_000
            || self.exploration_share_basis_points > 2_000
            || self.latency_cap_ms == 0
            || self.seed_domain.is_empty()
        {
            return Err("invalid dispatch algorithm profile");
        }
        Ok(())
    }
    pub(crate) fn canonical_version(&self) -> String {
        format!("routing-profile-v{}", self.version)
    }
    pub(crate) fn basis_points(value: u16) -> Option<BasisPoints> {
        BasisPoints::new(value)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_profile_is_complete() {
        let profile = DispatchAlgorithmProfile::default();
        assert!(profile.validate().is_ok());
        assert_eq!(profile.canonical_version(), "routing-profile-v1");
    }
}
