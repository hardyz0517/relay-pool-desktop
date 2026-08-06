use sha2::{Digest, Sha256};

use super::{fixed_point::BasisPoints, tiers::AvailabilityTier};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DispatchCandidate {
    pub(crate) id: String,
    pub(crate) utility: BasisPoints,
    pub(crate) tier: AvailabilityTier,
    pub(crate) failure_domains: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DispatchDecision {
    pub(crate) selected_id: String,
    pub(crate) band_size: usize,
    pub(crate) explored: bool,
    pub(crate) seed_commitment: String,
}

pub(crate) fn weighted_rendezvous(
    candidates: &[DispatchCandidate],
    seed: &[u8],
    band_basis_points: u16,
) -> Option<DispatchDecision> {
    if candidates.is_empty() {
        return None;
    }
    let best_utility = candidates
        .iter()
        .map(|candidate| candidate.utility.get())
        .max()
        .expect("not empty");
    let band = candidates
        .iter()
        .filter(|candidate| {
            best_utility.saturating_sub(candidate.utility.get()) <= band_basis_points
        })
        .collect::<Vec<_>>();
    let mut ranked = band
        .iter()
        .map(|candidate| {
            let candidate = *candidate;
            let mut hasher = Sha256::new();
            hasher.update(seed);
            hasher.update(candidate.id.as_bytes());
            let digest = hasher.finalize();
            let hash = u64::from_be_bytes(digest[0..8].try_into().expect("hash width"));
            let weight = u64::from(candidate.utility.get().max(1));
            let rank = u128::from(hash).saturating_mul(u128::from(weight));
            (rank, candidate)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.id.cmp(&right.1.id))
    });
    let best = ranked[0].1;
    let band_size = band.len();
    let commitment = Sha256::digest(seed)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Some(DispatchDecision {
        selected_id: best.id.clone(),
        band_size,
        explored: false,
        seed_commitment: commitment,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, utility: u16) -> DispatchCandidate {
        DispatchCandidate {
            id: id.to_string(),
            utility: BasisPoints::new(utility).expect("valid utility"),
            tier: AvailabilityTier::Primary,
            failure_domains: Vec::new(),
        }
    }

    #[test]
    fn rendezvous_never_dispatches_outside_the_utility_band() {
        let candidates = [candidate("best", 10_000), candidate("outside", 8_000)];
        for round in 0_u64..64 {
            let decision = weighted_rendezvous(&candidates, &round.to_be_bytes(), 500)
                .expect("non-empty dispatch");
            assert_eq!(decision.selected_id, "best");
            assert_eq!(decision.band_size, 1);
        }
    }
}
