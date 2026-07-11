use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StickyKind {
    PreviousResponse,
    Session,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoredCandidate {
    pub station_key_id: String,
    pub priority: i64,
    pub score: f64,
    pub load_rate: f64,
    pub waiting: u64,
    pub sticky_kind: Option<StickyKind>,
}

pub fn top_k_candidates(candidates: &[ScoredCandidate], top_k: usize) -> Vec<ScoredCandidate> {
    let mut sorted = candidates.to_vec();
    sorted.sort_by(compare_candidates);
    sorted.truncate(top_k.min(sorted.len()));
    sorted
}

pub fn top_k_weights(top_k: &[ScoredCandidate]) -> Vec<f64> {
    let min_score = top_k
        .iter()
        .map(|candidate| candidate.score)
        .fold(f64::INFINITY, f64::min);
    if !min_score.is_finite() {
        return Vec::new();
    }

    top_k
        .iter()
        .map(|candidate| (candidate.score - min_score) + 1.0)
        .collect()
}

pub fn weighted_order_without_replacement(
    candidates: &[ScoredCandidate],
    seed: u64,
) -> Vec<ScoredCandidate> {
    let mut remaining = candidates.to_vec();
    let mut rng = DeterministicRng::new(seed);
    let mut ordered = Vec::with_capacity(remaining.len());

    while !remaining.is_empty() {
        let weights = top_k_weights(&remaining);
        let total_weight: f64 = weights.iter().sum();
        let selected_index = if total_weight > 0.0 && total_weight.is_finite() {
            let mut target = rng.next_unit_f64() * total_weight;
            weights
                .iter()
                .position(|weight| {
                    target -= *weight;
                    target <= 0.0
                })
                .unwrap_or(weights.len() - 1)
        } else {
            0
        };
        ordered.push(remaining.remove(selected_index));
    }

    ordered
}

pub fn move_sticky_candidate_to_front(candidates: &mut Vec<ScoredCandidate>) {
    let Some(index) = candidates
        .iter()
        .position(|candidate| candidate.sticky_kind == Some(StickyKind::PreviousResponse))
        .or_else(|| {
            candidates
                .iter()
                .position(|candidate| candidate.sticky_kind == Some(StickyKind::Session))
        })
    else {
        return;
    };

    if index > 0 {
        let candidate = candidates.remove(index);
        candidates.insert(0, candidate);
    }
}

fn compare_candidates(left: &ScoredCandidate, right: &ScoredCandidate) -> Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.priority.cmp(&right.priority))
        .then_with(|| left.load_rate.total_cmp(&right.load_rate))
        .then_with(|| left.waiting.cmp(&right.waiting))
        .then_with(|| left.station_key_id.cmp(&right.station_key_id))
}

struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn next_unit_f64(&mut self) -> f64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let value = self.state >> 11;
        value as f64 / ((1_u64 << 53) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_k_tie_order_matches_scheduler_spec() {
        let candidates = vec![
            scored("key-c", 1, 10.0, 0.2, 1),
            scored("key-b", 0, 10.0, 0.8, 0),
            scored("key-a", 0, 10.0, 0.2, 2),
            scored("key-d", 0, 10.0, 0.2, 1),
            scored("key-e", 0, 11.0, 1.0, 9),
        ];

        let selected = top_k_candidates(&candidates, 4);
        let keys: Vec<_> = selected
            .iter()
            .map(|candidate| candidate.station_key_id.as_str())
            .collect();

        assert_eq!(keys, vec!["key-e", "key-d", "key-a", "key-b"]);
    }

    #[test]
    fn top_k_weights_use_score_delta_from_minimum_plus_one() {
        let candidates = vec![
            scored("key-a", 0, 8.5, 0.0, 0),
            scored("key-b", 0, 10.0, 0.0, 0),
            scored("key-c", 0, 9.0, 0.0, 0),
        ];

        let top_k = top_k_candidates(&candidates, 3);
        let weights = top_k_weights(&top_k);

        assert_eq!(weights, vec![2.5, 1.5, 1.0]);
    }

    #[test]
    fn weighted_order_is_stable_for_same_seed_and_contains_no_duplicates() {
        let candidates = vec![
            scored("key-a", 0, 8.0, 0.0, 0),
            scored("key-b", 0, 9.0, 0.0, 0),
            scored("key-c", 0, 10.0, 0.0, 0),
        ];

        let first = weighted_order_without_replacement(&candidates, 42);
        let second = weighted_order_without_replacement(&candidates, 42);

        assert_eq!(first, second);
        let mut keys: Vec<_> = first
            .iter()
            .map(|candidate| candidate.station_key_id.as_str())
            .collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), first.len());
    }

    #[test]
    fn previous_response_sticky_moves_before_session_sticky() {
        let mut top_k = vec![
            scored("key-normal", 0, 10.0, 0.0, 0),
            scored_with_sticky("key-session", 0, 9.0, 0.0, 0, StickyKind::Session),
            scored_with_sticky("key-response", 0, 8.0, 0.0, 0, StickyKind::PreviousResponse),
        ];

        move_sticky_candidate_to_front(&mut top_k);

        assert_eq!(top_k[0].station_key_id, "key-response");
    }

    #[test]
    fn sticky_outside_top_k_cannot_be_moved_in() {
        let candidates = vec![
            scored("key-a", 0, 10.0, 0.0, 0),
            scored("key-b", 0, 9.0, 0.0, 0),
            scored_with_sticky("key-sticky", 0, 1.0, 0.0, 0, StickyKind::PreviousResponse),
        ];
        let mut top_k = top_k_candidates(&candidates, 2);

        move_sticky_candidate_to_front(&mut top_k);

        let keys: Vec<_> = top_k
            .iter()
            .map(|candidate| candidate.station_key_id.as_str())
            .collect();
        assert_eq!(keys, vec!["key-a", "key-b"]);
    }

    fn scored(
        station_key_id: &str,
        priority: i64,
        score: f64,
        load_rate: f64,
        waiting: u64,
    ) -> ScoredCandidate {
        ScoredCandidate {
            station_key_id: station_key_id.to_string(),
            priority,
            score,
            load_rate,
            waiting,
            sticky_kind: None,
        }
    }

    fn scored_with_sticky(
        station_key_id: &str,
        priority: i64,
        score: f64,
        load_rate: f64,
        waiting: u64,
        sticky_kind: StickyKind,
    ) -> ScoredCandidate {
        ScoredCandidate {
            sticky_kind: Some(sticky_kind),
            ..scored(station_key_id, priority, score, load_rate, waiting)
        }
    }
}
