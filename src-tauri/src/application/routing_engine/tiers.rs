#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AvailabilityTier {
    Primary,
    Backup,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TieredCandidate {
    pub(crate) station_key_id: String,
    pub(crate) tier: AvailabilityTier,
    pub(crate) rejection_reason: Option<&'static str>,
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
        return allow_depleted.then_some(AvailabilityTier::Backup);
    } else {
        Some(AvailabilityTier::Primary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depleted_candidates_are_backup_only_when_policy_allows_them() {
        assert_eq!(classify_tier(true, true, true, false), None);
        assert_eq!(
            classify_tier(true, true, true, true),
            Some(AvailabilityTier::Backup)
        );
    }
}
