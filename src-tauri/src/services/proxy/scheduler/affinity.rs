use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AffinityKind {
    PreviousResponse,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffinityHit {
    pub kind: AffinityKind,
    pub station_key_id: String,
}

#[derive(Debug, Default)]
pub struct AffinityStore {
    session_entries: HashMap<ScopedAffinityKey, AffinityEntry>,
    response_entries: HashMap<ScopedAffinityKey, AffinityEntry>,
}

impl AffinityStore {
    pub fn bind_session(
        &mut self,
        routing_group_scope: &str,
        session_hash: &str,
        station_key_id: &str,
        now_ms: i64,
        ttl_seconds: i64,
    ) {
        bind_entry(
            &mut self.session_entries,
            routing_group_scope,
            session_hash,
            station_key_id,
            now_ms,
            ttl_seconds,
        );
    }

    pub fn bind_response(
        &mut self,
        routing_group_scope: &str,
        response_id: &str,
        station_key_id: &str,
        now_ms: i64,
        ttl_seconds: i64,
    ) {
        bind_entry(
            &mut self.response_entries,
            routing_group_scope,
            response_id,
            station_key_id,
            now_ms,
            ttl_seconds,
        );
    }

    pub fn lookup_session(
        &mut self,
        routing_group_scope: &str,
        session_hash: &str,
        now_ms: i64,
    ) -> Option<String> {
        lookup_entry(&mut self.session_entries, routing_group_scope, session_hash, now_ms)
    }

    pub fn lookup_response(
        &mut self,
        routing_group_scope: &str,
        response_id: &str,
        now_ms: i64,
    ) -> Option<String> {
        lookup_entry(
            &mut self.response_entries,
            routing_group_scope,
            response_id,
            now_ms,
        )
    }

    pub fn resolve(
        &mut self,
        routing_group_scope: &str,
        previous_response_id: Option<&str>,
        session_hash: Option<&str>,
        now_ms: i64,
    ) -> Option<AffinityHit> {
        if let Some(response_id) = previous_response_id {
            if let Some(station_key_id) =
                self.lookup_response(routing_group_scope, response_id, now_ms)
            {
                return Some(AffinityHit {
                    kind: AffinityKind::PreviousResponse,
                    station_key_id,
                });
            }
        }

        if let Some(session_hash) = session_hash {
            if let Some(station_key_id) =
                self.lookup_session(routing_group_scope, session_hash, now_ms)
            {
                return Some(AffinityHit {
                    kind: AffinityKind::Session,
                    station_key_id,
                });
            }
        }

        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ScopedAffinityKey {
    routing_group_scope: String,
    value: String,
}

impl ScopedAffinityKey {
    fn new(routing_group_scope: &str, value: &str) -> Option<Self> {
        if routing_group_scope.is_empty() || value.is_empty() {
            return None;
        }

        Some(Self {
            routing_group_scope: routing_group_scope.to_string(),
            value: value.to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AffinityEntry {
    station_key_id: String,
    expires_at_ms: i64,
}

fn bind_entry(
    entries: &mut HashMap<ScopedAffinityKey, AffinityEntry>,
    routing_group_scope: &str,
    value: &str,
    station_key_id: &str,
    now_ms: i64,
    ttl_seconds: i64,
) {
    if station_key_id.is_empty() || ttl_seconds <= 0 {
        return;
    }

    let Some(key) = ScopedAffinityKey::new(routing_group_scope, value) else {
        return;
    };

    entries.insert(
        key,
        AffinityEntry {
            station_key_id: station_key_id.to_string(),
            expires_at_ms: now_ms.saturating_add(ttl_seconds.saturating_mul(1_000)),
        },
    );
}

fn lookup_entry(
    entries: &mut HashMap<ScopedAffinityKey, AffinityEntry>,
    routing_group_scope: &str,
    value: &str,
    now_ms: i64,
) -> Option<String> {
    let key = ScopedAffinityKey::new(routing_group_scope, value)?;
    let entry = entries.get(&key)?;
    if entry.expires_at_ms <= now_ms {
        entries.remove(&key);
        return None;
    }

    Some(entry.station_key_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_affinity_is_scoped_by_routing_group() {
        let mut store = AffinityStore::default();

        store.bind_session("group-a", "session-1", "key-a", 1_000, 60);

        assert_eq!(
            store.lookup_session("group-a", "session-1", 1_001),
            Some("key-a".to_string())
        );
        assert_eq!(store.lookup_session("group-b", "session-1", 1_001), None);
    }

    #[test]
    fn previous_response_affinity_takes_precedence_over_session() {
        let mut store = AffinityStore::default();

        store.bind_session("group-a", "session-1", "key-session", 1_000, 60);
        store.bind_response("group-a", "resp-1", "key-response", 1_000, 60);

        assert_eq!(
            store.resolve("group-a", Some("resp-1"), Some("session-1"), 1_001),
            Some(AffinityHit {
                kind: AffinityKind::PreviousResponse,
                station_key_id: "key-response".to_string(),
            })
        );
    }

    #[test]
    fn expired_affinity_entries_return_none() {
        let mut store = AffinityStore::default();

        store.bind_session("group-a", "session-1", "key-a", 1_000, 1);
        store.bind_response("group-a", "resp-1", "key-b", 1_000, 1);

        assert_eq!(
            store.lookup_session("group-a", "session-1", 1_999),
            Some("key-a".to_string())
        );
        assert_eq!(store.lookup_session("group-a", "session-1", 2_000), None);
        assert_eq!(store.lookup_response("group-a", "resp-1", 2_000), None);
    }

    #[test]
    fn empty_scope_session_and_response_are_ignored() {
        let mut store = AffinityStore::default();

        store.bind_session("", "session-1", "key-a", 1_000, 60);
        store.bind_session("group-a", "", "key-a", 1_000, 60);
        store.bind_response("", "resp-1", "key-b", 1_000, 60);
        store.bind_response("group-a", "", "key-b", 1_000, 60);

        assert_eq!(store.lookup_session("", "session-1", 1_001), None);
        assert_eq!(store.lookup_session("group-a", "", 1_001), None);
        assert_eq!(store.lookup_response("", "resp-1", 1_001), None);
        assert_eq!(store.lookup_response("group-a", "", 1_001), None);
        assert_eq!(store.resolve("group-a", Some(""), Some(""), 1_001), None);
    }

    #[test]
    fn empty_previous_response_id_is_ignored_before_session_fallback() {
        let mut store = AffinityStore::default();

        store.bind_session("group-a", "session-1", "key-session", 1_000, 60);

        assert_eq!(
            store.resolve("group-a", Some(""), Some("session-1"), 1_001),
            Some(AffinityHit {
                kind: AffinityKind::Session,
                station_key_id: "key-session".to_string(),
            })
        );
    }
}
