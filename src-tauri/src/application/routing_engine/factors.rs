use super::fixed_point::BasisPoints;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReliabilityEstimate {
    pub(crate) value: BasisPoints,
    pub(crate) sample_mass: u32,
    pub(crate) lower_bound: BasisPoints,
}

pub(crate) fn reliability_posterior(
    success_mass: u32,
    failure_mass: u32,
    alpha: u32,
    beta: u32,
) -> Option<ReliabilityEstimate> {
    if alpha == 0 || beta == 0 {
        return None;
    }
    let total = u64::from(success_mass)
        .checked_add(u64::from(failure_mass))?
        .checked_add(u64::from(alpha))?
        .checked_add(u64::from(beta))?;
    let success = u64::from(success_mass).checked_add(u64::from(alpha))?;
    let value = u16::try_from((success * 10_000 + total / 2) / total).ok()?;
    let lower = if success_mass.saturating_add(failure_mass) < 10_000 {
        0
    } else {
        value.saturating_sub(1_000)
    };
    Some(ReliabilityEstimate {
        value: BasisPoints::new(value)?,
        sample_mass: success_mass.saturating_add(failure_mass),
        lower_bound: BasisPoints::new(lower)?,
    })
}

pub(crate) fn responsiveness_score(latency_ms: Option<u32>, cap_ms: u32) -> BasisPoints {
    let Some(latency) = latency_ms else {
        return BasisPoints::new(5_000).unwrap();
    };
    if cap_ms == 0 {
        return BasisPoints::ZERO;
    }
    let clamped = latency.min(cap_ms);
    BasisPoints::new(((u64::from(cap_ms - clamped) * 10_000) / u64::from(cap_ms)) as u16)
        .unwrap_or(BasisPoints::ZERO)
}

pub(crate) fn cost_score(cost_basis_points: Option<u16>) -> BasisPoints {
    // Unknown pricing is neutral evidence, never a zero-cost advantage.
    BasisPoints::new(cost_basis_points.unwrap_or(5_000)).unwrap_or(BasisPoints::ZERO)
}

/// Converts a non-negative, like-for-like request price into the bounded
/// efficiency score consumed by the planner. The one-unit reference point is
/// frozen here rather than normalising against the current candidate set, so
/// adding or removing another key cannot change an existing key's score.
pub(crate) fn cost_efficiency_from_comparable_value(value: f64) -> Option<u16> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let micros = (value * 1_000_000.0).round();
    if micros > u64::MAX as f64 {
        return None;
    }
    let denominator = 1_000_000_u64.checked_add(micros as u64)?;
    Some(((10_000_u64 * 1_000_000_u64) / denominator) as u16)
}

/// Converts a trusted effective key multiplier into a stable cost proxy.
/// 1.0 is neutral (50%); lower multipliers score better and higher
/// multipliers score lower. This is a routing proxy, not a token-price
/// estimate.
pub(crate) fn cost_efficiency_from_multiplier(value: f64) -> Option<u16> {
    cost_efficiency_from_comparable_value(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_data_is_prior_not_perfect_health() {
        let estimate = reliability_posterior(0, 0, 2, 2).unwrap();
        assert_eq!(estimate.value.get(), 5_000);
        assert_eq!(estimate.lower_bound.get(), 0);
    }
    #[test]
    fn latency_is_bounded() {
        assert_eq!(responsiveness_score(Some(0), 100).get(), 10_000);
        assert_eq!(responsiveness_score(Some(100), 100).get(), 0);
    }

    #[test]
    fn unknown_cost_is_neutral_not_free() {
        assert_eq!(cost_score(None).get(), 5_000);
        assert_eq!(cost_score(Some(0)).get(), 0);
    }

    #[test]
    fn lower_comparable_cost_has_a_higher_fixed_score() {
        let cheap = cost_efficiency_from_comparable_value(0.1).unwrap();
        let expensive = cost_efficiency_from_comparable_value(10.0).unwrap();
        assert!(cheap > expensive);
        assert_eq!(cost_efficiency_from_comparable_value(0.0), Some(10_000));
    }

    #[test]
    fn multiplier_proxy_is_neutral_at_one_and_rewards_lower_rates() {
        assert_eq!(cost_efficiency_from_multiplier(1.0), Some(5_000));
        assert!(cost_efficiency_from_multiplier(0.075).unwrap() > 5_000);
        assert!(cost_efficiency_from_multiplier(2.0).unwrap() < 5_000);
        assert_eq!(cost_efficiency_from_multiplier(f64::NAN), None);
    }
}
