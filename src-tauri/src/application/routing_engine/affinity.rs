use std::collections::BTreeMap;

use super::runtime_metrics::RuntimeModelClass;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum AffinityKind {
    PreviousResponse,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AffinityPolicy {
    pub(crate) max_entries: usize,
    pub(crate) ttl_ms: i64,
}

impl Default for AffinityPolicy {
    fn default() -> Self {
        Self {
            max_entries: 1_024,
            ttl_ms: 30 * 60 * 1_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct AffinityLookup {
    pub(crate) kind: AffinityKind,
    pub(crate) routing_group_scope: String,
    pub(crate) value: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) model_class: RuntimeModelClass,
}

impl AffinityLookup {
    pub(crate) fn new(
        kind: AffinityKind,
        routing_group_scope: impl Into<String>,
        value: impl Into<String>,
        endpoint_revision: i64,
        model: Option<&str>,
    ) -> Self {
        Self {
            kind,
            routing_group_scope: routing_group_scope.into(),
            value: value.into(),
            endpoint_revision,
            model_class: RuntimeModelClass::normalize(model),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AffinityHit {
    pub(crate) station_key_id: String,
    pub(crate) bound_at_ms: i64,
    pub(crate) expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AffinityMiss {
    InvalidInput,
    NotFound,
    Expired,
    GroupScopeMismatch,
    EndpointRevisionMismatch,
    ModelMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AffinityBindError {
    InvalidInput,
    InvalidTtl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AffinityEntry {
    station_key_id: String,
    bound_at_ms: i64,
    expires_at_ms: i64,
    last_touched_ms: i64,
}

#[derive(Debug)]
pub(crate) struct AffinityRegistry {
    policy: AffinityPolicy,
    entries: BTreeMap<AffinityLookup, AffinityEntry>,
}

impl AffinityRegistry {
    pub(crate) fn new(policy: AffinityPolicy) -> Self {
        Self {
            policy,
            entries: BTreeMap::new(),
        }
    }

    pub(crate) fn bind(
        &mut self,
        lookup: AffinityLookup,
        station_key_id: impl Into<String>,
        now_ms: i64,
        ttl_ms: i64,
    ) -> Result<(), AffinityBindError> {
        validate_lookup(&lookup)?;
        let station_key_id = station_key_id.into();
        if station_key_id.is_empty() {
            return Err(AffinityBindError::InvalidInput);
        }
        if ttl_ms <= 0 {
            return Err(AffinityBindError::InvalidTtl);
        }
        let ttl_ms = ttl_ms.min(self.policy.ttl_ms);
        self.cleanup(now_ms);
        self.entries.insert(
            lookup,
            AffinityEntry {
                station_key_id,
                bound_at_ms: now_ms,
                expires_at_ms: now_ms.saturating_add(ttl_ms),
                last_touched_ms: now_ms,
            },
        );
        self.enforce_bounds();
        Ok(())
    }

    pub(crate) fn lookup(
        &mut self,
        lookup: &AffinityLookup,
        now_ms: i64,
    ) -> Result<AffinityHit, AffinityMiss> {
        validate_lookup(lookup).map_err(|_| AffinityMiss::InvalidInput)?;
        if let Some(entry) = self.entries.get(lookup) {
            if entry.expires_at_ms <= now_ms {
                self.entries.remove(lookup);
                return Err(AffinityMiss::Expired);
            }
            return Ok(AffinityHit {
                station_key_id: entry.station_key_id.clone(),
                bound_at_ms: entry.bound_at_ms,
                expires_at_ms: entry.expires_at_ms,
            });
        }
        self.classify_mismatch(lookup, now_ms)
    }

    pub(crate) fn cleanup(&mut self, now_ms: i64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|_, entry| {
            entry.expires_at_ms > now_ms
                && now_ms.saturating_sub(entry.last_touched_ms) <= self.policy.ttl_ms
        });
        before.saturating_sub(self.entries.len())
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    fn classify_mismatch(
        &mut self,
        lookup: &AffinityLookup,
        now_ms: i64,
    ) -> Result<AffinityHit, AffinityMiss> {
        let mut expired = Vec::new();
        let mut group_mismatch = false;
        let mut revision_mismatch = false;
        let mut model_mismatch = false;

        for (key, entry) in &self.entries {
            if key.kind != lookup.kind || key.value != lookup.value {
                continue;
            }
            if entry.expires_at_ms <= now_ms {
                expired.push(key.clone());
                continue;
            }
            if key.routing_group_scope != lookup.routing_group_scope {
                group_mismatch = true;
            } else if key.endpoint_revision != lookup.endpoint_revision {
                revision_mismatch = true;
            } else if key.model_class != lookup.model_class {
                model_mismatch = true;
            }
        }
        for key in expired {
            self.entries.remove(&key);
        }
        if group_mismatch {
            Err(AffinityMiss::GroupScopeMismatch)
        } else if revision_mismatch {
            Err(AffinityMiss::EndpointRevisionMismatch)
        } else if model_mismatch {
            Err(AffinityMiss::ModelMismatch)
        } else {
            Err(AffinityMiss::NotFound)
        }
    }

    fn enforce_bounds(&mut self) {
        while self.entries.len() > self.policy.max_entries {
            let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(key, entry)| (entry.last_touched_ms, *key))
                .map(|(key, _)| key.clone())
            else {
                return;
            };
            self.entries.remove(&oldest_key);
        }
    }
}

impl Default for AffinityRegistry {
    fn default() -> Self {
        Self::new(AffinityPolicy::default())
    }
}

fn validate_lookup(lookup: &AffinityLookup) -> Result<(), AffinityBindError> {
    if lookup.routing_group_scope.is_empty() || lookup.value.is_empty() {
        return Err(AffinityBindError::InvalidInput);
    }
    Ok(())
}
