use super::fixed_point::BasisPoints;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReliabilityEstimate { pub(crate) value: BasisPoints, pub(crate) sample_mass: u32, pub(crate) lower_bound: BasisPoints }

pub(crate) fn reliability_posterior(success_mass: u32, failure_mass: u32, alpha: u32, beta: u32) -> Option<ReliabilityEstimate> {
    if alpha == 0 || beta == 0 { return None; }
    let total = u64::from(success_mass).checked_add(u64::from(failure_mass))?.checked_add(u64::from(alpha))?.checked_add(u64::from(beta))?;
    let success = u64::from(success_mass).checked_add(u64::from(alpha))?;
    let value = u16::try_from((success * 10_000 + total / 2) / total).ok()?;
    let lower = if success_mass.saturating_add(failure_mass) < 10_000 { 0 } else { value.saturating_sub(1_000) };
    Some(ReliabilityEstimate { value: BasisPoints::new(value)?, sample_mass: success_mass.saturating_add(failure_mass), lower_bound: BasisPoints::new(lower)? })
}

pub(crate) fn responsiveness_score(latency_ms: Option<u32>, cap_ms: u32) -> BasisPoints {
    let Some(latency) = latency_ms else { return BasisPoints::new(5_000).unwrap(); };
    if cap_ms == 0 { return BasisPoints::ZERO; }
    let clamped = latency.min(cap_ms);
    BasisPoints::new(((u64::from(cap_ms - clamped) * 10_000) / u64::from(cap_ms)) as u16).unwrap_or(BasisPoints::ZERO)
}

pub(crate) fn cost_score(cost_basis_points: Option<u16>) -> BasisPoints { BasisPoints::new(cost_basis_points.unwrap_or(0)).unwrap_or(BasisPoints::ZERO) }

#[cfg(test)]
mod tests { use super::*; #[test] fn no_data_is_prior_not_perfect_health() { let estimate = reliability_posterior(0, 0, 2, 2).unwrap(); assert_eq!(estimate.value.get(), 5_000); assert_eq!(estimate.lower_bound.get(), 0); } #[test] fn latency_is_bounded() { assert_eq!(responsiveness_score(Some(0), 100).get(), 10_000); assert_eq!(responsiveness_score(Some(100), 100).get(), 0); } }
