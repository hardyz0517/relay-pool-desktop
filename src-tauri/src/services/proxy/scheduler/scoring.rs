use crate::models::routing::SchedulerAdvancedSettings;

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreInput {
    pub station_key_id: String,
    pub priority: i64,
    pub effective_multiplier: f64,
    pub in_flight: u64,
    pub effective_capacity: u64,
    pub waiting: u64,
    pub error_rate_ewma: f64,
    pub ttft_ms: Option<f64>,
    pub quota_headroom: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreFactors {
    pub multiplier: f64,
    pub priority: f64,
    pub load: f64,
    pub queue: f64,
    pub error_rate: f64,
    pub ttft: f64,
    pub quota_headroom: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandidateScoreBreakdown {
    pub station_key_id: String,
    pub priority: i64,
    pub score: f64,
    pub load_rate: f64,
    pub waiting: u64,
    pub factors: ScoreFactors,
}

pub fn score_candidates(
    inputs: &[ScoreInput],
    weights: &SchedulerAdvancedSettings,
) -> Vec<CandidateScoreBreakdown> {
    let multipliers: Vec<_> = inputs
        .iter()
        .map(|candidate| candidate.effective_multiplier)
        .collect();
    let priorities: Vec<_> = inputs.iter().map(|candidate| candidate.priority).collect();
    let waiting: Vec<_> = inputs.iter().map(|candidate| candidate.waiting).collect();
    let error_rates: Vec<_> = inputs
        .iter()
        .map(|candidate| candidate.error_rate_ewma)
        .collect();
    let ttfts: Vec<_> = inputs.iter().map(|candidate| candidate.ttft_ms).collect();

    let multiplier_factors = multiplier_factors(&multipliers);
    let priority_factors = priority_factors(&priorities);
    let queue_factors = queue_factors(&waiting);
    let error_factors = error_factors(&error_rates);
    let ttft_factors = ttft_factors(&ttfts);

    inputs
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let load_rate = load_rate(candidate.in_flight, candidate.effective_capacity);
            let factors = ScoreFactors {
                multiplier: multiplier_factors[index],
                priority: priority_factors[index],
                load: 1.0 - load_rate,
                queue: queue_factors[index],
                error_rate: error_factors[index],
                ttft: ttft_factors[index],
                quota_headroom: clamp01(candidate.quota_headroom),
            };
            let score = factors.multiplier * weights.multiplier
                + factors.priority * weights.priority
                + factors.load * weights.load
                + factors.queue * weights.queue
                + factors.error_rate * weights.error_rate
                + factors.ttft * weights.ttft
                + factors.quota_headroom * weights.quota_headroom;

            CandidateScoreBreakdown {
                station_key_id: candidate.station_key_id.clone(),
                priority: candidate.priority,
                score,
                load_rate,
                waiting: candidate.waiting,
                factors,
            }
        })
        .collect()
}

pub fn multiplier_factors(values: &[f64]) -> Vec<f64> {
    lower_is_better_factors(values, 1.0)
}

pub fn priority_factors(values: &[i64]) -> Vec<f64> {
    let values: Vec<_> = values.iter().map(|value| *value as f64).collect();
    lower_is_better_factors(&values, 1.0)
}

pub fn queue_factors(waiting: &[u64]) -> Vec<f64> {
    let max_waiting = waiting.iter().copied().max().unwrap_or(0);
    if max_waiting == 0 {
        return vec![1.0; waiting.len()];
    }

    waiting
        .iter()
        .map(|value| 1.0 - clamp01(*value as f64 / max_waiting as f64))
        .collect()
}

pub fn error_factors(error_rate_ewmas: &[f64]) -> Vec<f64> {
    error_rate_ewmas
        .iter()
        .map(|value| {
            if value.is_finite() {
                1.0 - clamp01(*value)
            } else {
                0.0
            }
        })
        .collect()
}

pub fn ttft_factors(ttfts: &[Option<f64>]) -> Vec<f64> {
    let present: Vec<_> = ttfts
        .iter()
        .flatten()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    if present.is_empty() {
        return vec![0.5; ttfts.len()];
    }

    let present_factors = lower_is_better_factors(&present, 0.5);
    let mut present_index = 0;
    ttfts
        .iter()
        .map(|value| match value {
            Some(value) if value.is_finite() => {
                let factor = present_factors[present_index];
                present_index += 1;
                factor
            }
            Some(_) => 0.0,
            None => 0.5,
        })
        .collect()
}

pub fn load_rate(in_flight: u64, effective_capacity: u64) -> f64 {
    if effective_capacity == 0 {
        return 1.0;
    }
    clamp01(in_flight as f64 / effective_capacity as f64)
}

fn lower_is_better_factors(values: &[f64], equal_factor: f64) -> Vec<f64> {
    if values.is_empty() {
        return Vec::new();
    }

    let finite_values: Vec<_> = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    if finite_values.is_empty() {
        return vec![0.0; values.len()];
    }

    let min = finite_values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = finite_values
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    if range == 0.0 {
        return values
            .iter()
            .map(|value| if value.is_finite() { equal_factor } else { 0.0 })
            .collect();
    }

    values
        .iter()
        .map(|value| {
            if value.is_finite() {
                clamp01((max - *value) / range)
            } else {
                0.0
            }
        })
        .collect()
}

fn clamp01(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    value.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::routing::SchedulerAdvancedSettings;

    #[test]
    fn multiplier_factors_scale_lower_values_higher() {
        let factors = multiplier_factors(&[0.5, 1.0, 1.5]);

        assert_eq!(factors, vec![1.0, 0.5, 0.0]);
    }

    #[test]
    fn missing_ttft_is_neutral() {
        let factors = ttft_factors(&[Some(100.0), None, Some(300.0)]);

        assert_eq!(factors[1], 0.5);
    }

    #[test]
    fn equal_present_ttft_is_neutral() {
        let factors = ttft_factors(&[Some(200.0), Some(200.0)]);

        assert_eq!(factors, vec![0.5, 0.5]);
    }

    #[test]
    fn priority_factors_scale_lower_values_higher() {
        let factors = priority_factors(&[0, 10, 20]);

        assert_eq!(factors, vec![1.0, 0.5, 0.0]);
    }

    #[test]
    fn queue_factor_is_one_when_every_candidate_has_zero_waiting() {
        let factors = queue_factors(&[0, 0, 0]);

        assert_eq!(factors, vec![1.0, 1.0, 1.0]);
    }

    #[test]
    fn error_factor_clamps_to_zero_one_range() {
        let factors = error_factors(&[-0.25, 0.25, 1.5]);

        assert_eq!(factors, vec![1.0, 0.75, 0.0]);
    }

    #[test]
    fn multiplier_factor_does_not_reward_nan_or_infinity() {
        let factors = multiplier_factors(&[0.5, f64::NAN, f64::INFINITY, f64::NEG_INFINITY]);

        assert_eq!(factors, vec![1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn error_factor_does_not_reward_nan() {
        let factors = error_factors(&[0.0, f64::NAN]);

        assert_eq!(factors, vec![1.0, 0.0]);
    }

    #[test]
    fn ttft_factor_does_not_reward_explicit_non_finite_values() {
        let factors = ttft_factors(&[Some(100.0), None, Some(f64::NAN), Some(f64::INFINITY)]);

        assert_eq!(factors, vec![0.5, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn score_candidates_keeps_explainable_factor_breakdown() {
        let inputs = vec![
            ScoreInput {
                station_key_id: "key-fast".to_string(),
                priority: 0,
                effective_multiplier: 0.5,
                in_flight: 0,
                effective_capacity: 10,
                waiting: 0,
                error_rate_ewma: 0.0,
                ttft_ms: Some(100.0),
                quota_headroom: 1.0,
            },
            ScoreInput {
                station_key_id: "key-slow".to_string(),
                priority: 10,
                effective_multiplier: 1.5,
                in_flight: 10,
                effective_capacity: 10,
                waiting: 5,
                error_rate_ewma: 1.0,
                ttft_ms: Some(300.0),
                quota_headroom: 0.0,
            },
        ];
        let weights = SchedulerAdvancedSettings::default();

        let scored = score_candidates(&inputs, &weights);

        assert_eq!(scored[0].station_key_id, "key-fast");
        assert!(scored[0].score > scored[1].score);
        assert_eq!(scored[0].factors.multiplier, 1.0);
        assert_eq!(scored[1].factors.load, 0.0);
    }
}
