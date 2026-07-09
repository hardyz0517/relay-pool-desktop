use crate::models::routing::RouteEndpointKind;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub const ROUTE_AFFINITY_TTL_MS: i64 = 10 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteAffinityKey {
    pub endpoint: RouteEndpointKind,
    pub model: Option<String>,
}

impl RouteAffinityKey {
    pub fn new(endpoint: RouteEndpointKind, model: Option<&str>) -> Self {
        Self {
            endpoint,
            model: model.map(ToString::to_string),
        }
    }
}

impl Hash for RouteAffinityKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        route_endpoint_hash_tag(&self.endpoint).hash(state);
        self.model.hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteAffinityValue {
    pub station_key_id: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Default)]
pub struct RouteAffinityStore {
    pub entries: HashMap<RouteAffinityKey, RouteAffinityValue>,
}

impl RouteAffinityStore {
    pub fn record_success(
        &mut self,
        endpoint: RouteEndpointKind,
        model: Option<&str>,
        station_key_id: &str,
        now_ms: i64,
    ) {
        if matches!(endpoint, RouteEndpointKind::Models) {
            return;
        }

        self.entries.insert(
            RouteAffinityKey::new(endpoint, model),
            RouteAffinityValue {
                station_key_id: station_key_id.to_string(),
                expires_at_ms: now_ms + ROUTE_AFFINITY_TTL_MS,
            },
        );
    }

    pub fn lookup(
        &mut self,
        endpoint: RouteEndpointKind,
        model: Option<&str>,
        now_ms: i64,
    ) -> Option<String> {
        if matches!(endpoint, RouteEndpointKind::Models) {
            return None;
        }

        let key = RouteAffinityKey::new(endpoint, model);
        let value = self.entries.get(&key)?;
        if value.expires_at_ms <= now_ms {
            self.entries.remove(&key);
            return None;
        }

        Some(value.station_key_id.clone())
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
    fn affinity_key_uses_endpoint_and_model() {
        let key_a = RouteAffinityKey::new(RouteEndpointKind::ChatCompletions, Some("gpt-4o-mini"));
        let key_b = RouteAffinityKey::new(RouteEndpointKind::Responses, Some("gpt-4o-mini"));
        let key_c = RouteAffinityKey::new(RouteEndpointKind::ChatCompletions, Some("gpt-4o"));

        assert_ne!(key_a, key_b);
        assert_ne!(key_a, key_c);
    }

    #[test]
    fn models_endpoint_does_not_update_affinity() {
        let mut store = RouteAffinityStore::default();

        store.record_success(RouteEndpointKind::Models, None, "key-a", 1_000);

        assert!(store
            .lookup(RouteEndpointKind::Models, None, 1_001)
            .is_none());
    }

    #[test]
    fn affinity_lookup_returns_unexpired_key() {
        let mut store = RouteAffinityStore::default();

        store.record_success(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-4o-mini"),
            "key-a",
            1_000,
        );

        assert_eq!(
            store.lookup(
                RouteEndpointKind::ChatCompletions,
                Some("gpt-4o-mini"),
                1_001,
            ),
            Some("key-a".to_string())
        );
    }

    #[test]
    fn affinity_lookup_expires_old_key() {
        let mut store = RouteAffinityStore::default();

        store.record_success(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-4o-mini"),
            "key-a",
            1_000,
        );

        assert!(store
            .lookup(
                RouteEndpointKind::ChatCompletions,
                Some("gpt-4o-mini"),
                1_000 + ROUTE_AFFINITY_TTL_MS + 1,
            )
            .is_none());
    }
}
