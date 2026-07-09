use crate::models::routing::RouteEndpointKind;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub const PROBE_CONFIRMATION_TTL_MS: i64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeCacheKey {
    pub station_key_id: String,
    pub endpoint: RouteEndpointKind,
    pub mapped_model: Option<String>,
}

impl ProbeCacheKey {
    pub fn new(
        station_key_id: impl Into<String>,
        endpoint: RouteEndpointKind,
        mapped_model: Option<&str>,
    ) -> Self {
        Self {
            station_key_id: station_key_id.into(),
            endpoint,
            mapped_model: mapped_model.map(ToString::to_string),
        }
    }
}

impl Hash for ProbeCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.station_key_id.hash(state);
        route_endpoint_hash_tag(&self.endpoint).hash(state);
        self.mapped_model.hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeEligibility {
    Ready,
    Depleted,
    Cooling,
    AuthError,
    ManualDisabled,
}

impl ProbeEligibility {
    fn can_probe(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeCandidate {
    pub station_key_id: String,
    pub endpoint: RouteEndpointKind,
    pub mapped_model: Option<String>,
    pub eligibility: ProbeEligibility,
}

impl ProbeCandidate {
    pub fn new(
        station_key_id: impl Into<String>,
        endpoint: RouteEndpointKind,
        mapped_model: Option<&str>,
        eligibility: ProbeEligibility,
    ) -> Self {
        Self {
            station_key_id: station_key_id.into(),
            endpoint,
            mapped_model: mapped_model.map(ToString::to_string),
            eligibility,
        }
    }

    pub fn cache_key(&self) -> ProbeCacheKey {
        ProbeCacheKey {
            station_key_id: self.station_key_id.clone(),
            endpoint: self.endpoint.clone(),
            mapped_model: self.mapped_model.clone(),
        }
    }
}

#[derive(Debug)]
pub struct ProbeConfirmationCache {
    ttl_ms: i64,
    passed_until_ms: HashMap<ProbeCacheKey, i64>,
}

impl Default for ProbeConfirmationCache {
    fn default() -> Self {
        Self::new(PROBE_CONFIRMATION_TTL_MS)
    }
}

impl ProbeConfirmationCache {
    pub fn new(ttl_ms: i64) -> Self {
        Self {
            ttl_ms,
            passed_until_ms: HashMap::new(),
        }
    }

    pub fn should_probe(&mut self, key: &ProbeCacheKey, now_ms: i64) -> bool {
        !self.should_skip_probe(key, now_ms)
    }

    pub fn record_pass(&mut self, key: ProbeCacheKey, now_ms: i64) {
        self.passed_until_ms.insert(key, now_ms + self.ttl_ms);
    }

    pub fn should_skip_probe(&mut self, key: &ProbeCacheKey, now_ms: i64) -> bool {
        let Some(expires_at_ms) = self.passed_until_ms.get(key).copied() else {
            return false;
        };
        if expires_at_ms <= now_ms {
            self.passed_until_ms.remove(key);
            return false;
        }
        true
    }

    pub fn next_probe_candidate(
        &mut self,
        candidates: &[ProbeCandidate],
        now_ms: i64,
    ) -> Option<ProbeCandidate> {
        candidates
            .iter()
            .find(|candidate| {
                candidate.eligibility.can_probe()
                    && self.should_probe(&candidate.cache_key(), now_ms)
            })
            .cloned()
    }
}

fn route_endpoint_hash_tag(endpoint: &RouteEndpointKind) -> u8 {
    match endpoint {
        RouteEndpointKind::Models => 0,
        RouteEndpointKind::ChatCompletions => 1,
        RouteEndpointKind::Responses => 2,
        RouteEndpointKind::Embeddings => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_probe_skips_depleted_cooling_and_auth_error_keys() {
        let mut cache = ProbeConfirmationCache::default();
        let candidates = vec![
            ProbeCandidate::new(
                "depleted",
                RouteEndpointKind::ChatCompletions,
                Some("gpt-5.4"),
                ProbeEligibility::Depleted,
            ),
            ProbeCandidate::new(
                "cooling",
                RouteEndpointKind::ChatCompletions,
                Some("gpt-5.4"),
                ProbeEligibility::Cooling,
            ),
            ProbeCandidate::new(
                "auth",
                RouteEndpointKind::ChatCompletions,
                Some("gpt-5.4"),
                ProbeEligibility::AuthError,
            ),
            ProbeCandidate::new(
                "manual",
                RouteEndpointKind::ChatCompletions,
                Some("gpt-5.4"),
                ProbeEligibility::ManualDisabled,
            ),
            ProbeCandidate::new(
                "ready",
                RouteEndpointKind::ChatCompletions,
                Some("gpt-5.4"),
                ProbeEligibility::Ready,
            ),
        ];

        let selected = cache
            .next_probe_candidate(&candidates, 1_000)
            .expect("ready candidate");

        assert_eq!(selected.station_key_id, "ready");
    }

    #[test]
    fn probe_cache_deduplicates_burst_for_same_key_endpoint_model() {
        let mut cache = ProbeConfirmationCache::new(30_000);
        let key = ProbeCacheKey::new("key-a", RouteEndpointKind::Responses, Some("gpt-5.4"));

        assert!(cache.should_probe(&key, 1_000));
        cache.record_pass(key.clone(), 1_000);

        assert!(cache.should_skip_probe(&key, 1_001));
        assert!(!cache.should_probe(&key, 1_001));
    }

    #[test]
    fn probe_cache_expires_after_ttl() {
        let mut cache = ProbeConfirmationCache::new(30_000);
        let key = ProbeCacheKey::new("key-a", RouteEndpointKind::Responses, Some("gpt-5.4"));

        cache.record_pass(key.clone(), 1_000);

        assert!(cache.should_skip_probe(&key, 30_999));
        assert!(cache.should_probe(&key, 31_000));
        assert!(!cache.passed_until_ms.contains_key(&key));
    }
}
