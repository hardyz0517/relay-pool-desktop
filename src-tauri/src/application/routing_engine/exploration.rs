use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

#[derive(Debug, Clone)]
pub(crate) struct ExplorationBudgetRegistry {
    remaining: Arc<AtomicU32>,
    capacity: u32,
}
impl ExplorationBudgetRegistry {
    pub(crate) fn new(capacity: u32) -> Self {
        Self {
            remaining: Arc::new(AtomicU32::new(capacity)),
            capacity,
        }
    }
    pub(crate) fn reserve(&self) -> bool {
        self.remaining
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_sub(1)
            })
            .is_ok()
    }
    pub(crate) fn release(&self) {
        self.remaining
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                Some(value.saturating_add(1).min(self.capacity))
            })
            .ok();
    }
    pub(crate) fn remaining(&self) -> u32 {
        self.remaining.load(Ordering::Acquire)
    }
}

pub(crate) fn derive_seed(root: &[u8], domain: &str, round: u64) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    hasher.update(root);
    hasher.update(domain.as_bytes());
    hasher.update(round.to_be_bytes());
    hasher.finalize().into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplorationLane {
    Exploit,
    Explore,
}

/// Reserves the global budget before entering the unknown lane. A request
/// that loses the deterministic share gate never consumes budget.
pub(crate) fn choose_lane(
    seed: &[u8; 32],
    share_basis_points: u16,
    has_unknown_candidate: bool,
    budget: &ExplorationBudgetRegistry,
) -> ExplorationLane {
    if !has_unknown_candidate || share_basis_points == 0 {
        return ExplorationLane::Exploit;
    }
    let draw = u16::from_be_bytes([seed[0], seed[1]]) % 10_000;
    if draw >= share_basis_points || !budget.reserve() {
        ExplorationLane::Exploit
    } else {
        ExplorationLane::Explore
    }
}

/// Exhaustive bounded simulation uses this closed form as the starvation
/// contract: every block of `ceil(10000/share)` eligible rounds has at least
/// one exploration opportunity, subject to the finite budget.
pub(crate) fn starvation_bound(share_basis_points: u16) -> Option<u64> {
    if share_basis_points == 0 {
        None
    } else {
        Some((10_000_u64 + u64::from(share_basis_points) - 1) / u64::from(share_basis_points))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reservation_is_atomic_and_bounded() {
        let registry = ExplorationBudgetRegistry::new(1);
        assert!(registry.reserve());
        assert!(!registry.reserve());
        registry.release();
        assert_eq!(registry.remaining(), 1);
    }

    #[test]
    fn exploration_has_a_finite_starvation_bound() {
        assert_eq!(starvation_bound(500), Some(20));
        assert_eq!(starvation_bound(0), None);
    }
}
