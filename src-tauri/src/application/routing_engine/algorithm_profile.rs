pub(crate) const DISPATCH_ALGORITHM_VERSION: u16 = 2;
pub(crate) const AFFINITY_BONUS_CAP_BASIS_POINTS: u16 = 150;
pub(crate) const AFFINITY_HYSTERESIS_MARGIN_BASIS_POINTS: u16 = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DispatchAlgorithmProfile {
    pub(crate) version: u16,
    pub(crate) latency_cap_ms: u32,
    pub(crate) affinity_bonus_cap_basis_points: u16,
    pub(crate) affinity_hysteresis_margin_basis_points: u16,
}
impl Default for DispatchAlgorithmProfile {
    fn default() -> Self {
        Self {
            version: DISPATCH_ALGORITHM_VERSION,
            latency_cap_ms: 120_000,
            affinity_bonus_cap_basis_points: AFFINITY_BONUS_CAP_BASIS_POINTS,
            affinity_hysteresis_margin_basis_points: AFFINITY_HYSTERESIS_MARGIN_BASIS_POINTS,
        }
    }
}
impl DispatchAlgorithmProfile {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.version != DISPATCH_ALGORITHM_VERSION
            || self.latency_cap_ms == 0
            || self.affinity_bonus_cap_basis_points == 0
            || self.affinity_bonus_cap_basis_points > 10_000
            || self.affinity_hysteresis_margin_basis_points == 0
            || self.affinity_hysteresis_margin_basis_points > 10_000
        {
            return Err("invalid dispatch algorithm profile");
        }
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn canonical_version(&self) -> String {
        format!("routing-profile-v{}", self.version)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_profile_is_complete() {
        let profile = DispatchAlgorithmProfile::default();
        assert!(profile.validate().is_ok());
        assert_eq!(profile.canonical_version(), "routing-profile-v2");
        assert_eq!(profile.affinity_bonus_cap_basis_points, 150);
        assert_eq!(profile.affinity_hysteresis_margin_basis_points, 1_000);
    }
}
