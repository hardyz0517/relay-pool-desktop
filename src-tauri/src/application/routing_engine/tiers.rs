#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AvailabilityTier { Primary, Backup, Emergency }

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TieredCandidate { pub(crate) station_key_id: String, pub(crate) tier: AvailabilityTier, pub(crate) rejection_reason: Option<&'static str> }

pub(crate) fn classify_tier(enabled: bool, healthy: bool, depleted: bool, allow_depleted: bool) -> Option<AvailabilityTier> { if !enabled || !healthy { return None; } if depleted && !allow_depleted { return Some(AvailabilityTier::Emergency); } if depleted { Some(AvailabilityTier::Backup) } else { Some(AvailabilityTier::Primary) } }
