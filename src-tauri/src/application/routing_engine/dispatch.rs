use sha2::{Digest, Sha256};

use super::{fixed_point::BasisPoints, tiers::AvailabilityTier};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DispatchCandidate { pub(crate) id: String, pub(crate) utility: BasisPoints, pub(crate) tier: AvailabilityTier, pub(crate) failure_domains: Vec<String> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DispatchDecision { pub(crate) selected_id: String, pub(crate) band_size: usize, pub(crate) explored: bool, pub(crate) seed_commitment: String }

pub(crate) fn weighted_rendezvous(candidates: &[DispatchCandidate], seed: &[u8], band_basis_points: u16) -> Option<DispatchDecision> {
    if candidates.is_empty() { return None; }
    let mut ranked = candidates.iter().map(|candidate| { let mut hasher = Sha256::new(); hasher.update(seed); hasher.update(candidate.id.as_bytes()); let digest = hasher.finalize(); let hash = u64::from_be_bytes(digest[0..8].try_into().expect("hash width")); let weight = u64::from(candidate.utility.get().max(1)); let rank = u128::from(hash).saturating_mul(u128::from(weight)); (rank, candidate) }).collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.id.cmp(&right.1.id)));
    let best = ranked[0].1;
    let band_size = ranked.iter().filter(|(_, candidate)| best.utility.get().saturating_sub(candidate.utility.get()) <= band_basis_points).count().max(1);
    let commitment = Sha256::digest(seed)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Some(DispatchDecision { selected_id: best.id.clone(), band_size, explored: false, seed_commitment: commitment })
}
