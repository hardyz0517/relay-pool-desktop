#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AvailabilityTier {
    Primary,
    ConfiguredBackup,
    DepletedEmergency,
}

pub(crate) fn classify_tier(
    enabled: bool,
    healthy: bool,
    depleted: bool,
    allow_depleted: bool,
) -> Option<AvailabilityTier> {
    if !enabled || !healthy {
        return None;
    }
    if depleted {
        return allow_depleted.then_some(AvailabilityTier::DepletedEmergency);
    } else {
        Some(AvailabilityTier::Primary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depleted_candidates_are_emergency_only_when_policy_allows_them() {
        assert_eq!(classify_tier(true, true, true, false), None);
        assert_eq!(
            classify_tier(true, true, true, true),
            Some(AvailabilityTier::DepletedEmergency)
        );
    }
}
