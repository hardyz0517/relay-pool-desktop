use super::fixed_point::BasisPoints;

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

pub(crate) fn cost_score(cost_basis_points: Option<u16>) -> Option<BasisPoints> {
    cost_basis_points.and_then(BasisPoints::new)
}

/// Returns the median of the positive, finite values in the supplied set.
///
/// The planner uses the same request-scoped candidate snapshot for both the
/// input set and each candidate score, so adding/removing a key or changing a
/// multiplier naturally produces a new reference on the next snapshot.
pub(crate) fn multiplier_median(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    let mut values = values
        .into_iter()
        .filter(|value| value.is_finite() && *value > 0.0)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 1 {
        Some(values[middle])
    } else {
        let lower = values[middle - 1];
        let upper = values[middle];
        Some(lower + (upper - lower) / 2.0)
    }
}

/// Converts a trusted effective key multiplier into the bounded cost score
/// defined by the routing pricing rule:
///
/// `S_cost = 100 / (1 + (m / (2r))^2)`
///
/// The return value is in basis points (`0..=10_000`). Invalid values or an
/// unavailable reference median make the cost factor unavailable instead of
/// inventing a fixed baseline.
pub(crate) fn cost_efficiency_from_multiplier(value: f64, reference_median: f64) -> Option<u16> {
    if !value.is_finite()
        || value <= 0.0
        || !reference_median.is_finite()
        || reference_median <= 0.0
    {
        return None;
    }
    let ratio = (value / reference_median) / 2.0;
    let denominator = ratio.mul_add(ratio, 1.0);
    if denominator.is_nan() {
        return None;
    }
    let score = (10_000.0 / denominator).floor();
    if !score.is_finite() {
        return Some(0);
    }
    Some(score.clamp(0.0, 10_000.0) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latency_is_bounded() {
        assert_eq!(responsiveness_score(Some(0), 100).get(), 10_000);
        assert_eq!(responsiveness_score(Some(100), 100).get(), 0);
    }

    #[test]
    fn unknown_cost_is_unavailable_instead_of_neutral_evidence() {
        assert_eq!(cost_score(None), None);
        assert_eq!(cost_score(Some(0)), Some(BasisPoints::ZERO));
    }

    #[test]
    fn median_ignores_invalid_values_and_averages_even_sets() {
        assert_eq!(multiplier_median([f64::NAN, -1.0, 0.04, 0.02]), Some(0.03));
        assert_eq!(multiplier_median([0.01, 0.02, 0.04]), Some(0.02));
        assert_eq!(multiplier_median([f64::INFINITY]), None);
    }

    #[test]
    fn median_reference_changes_when_the_participating_set_changes() {
        let before = multiplier_median([0.02, 0.04]).expect("reference median");
        let after = multiplier_median([0.02, 0.04, 0.2]).expect("reference median");
        assert_eq!(before, 0.03);
        assert_eq!(after, 0.04);
        assert_ne!(
            cost_efficiency_from_multiplier(0.02, before),
            cost_efficiency_from_multiplier(0.02, after)
        );
    }

    #[test]
    fn multiplier_proxy_uses_the_median_reference() {
        assert_eq!(cost_efficiency_from_multiplier(0.03, 0.03), Some(8_000));
        assert_eq!(cost_efficiency_from_multiplier(0.06, 0.03), Some(5_000));
        assert!(cost_efficiency_from_multiplier(0.015, 0.03).unwrap() > 8_000);
        assert_eq!(cost_efficiency_from_multiplier(f64::NAN, 0.03), None);
        assert_eq!(cost_efficiency_from_multiplier(0.03, 0.0), None);
    }
}
