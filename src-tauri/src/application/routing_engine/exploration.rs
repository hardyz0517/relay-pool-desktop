use std::sync::{atomic::{AtomicU32, Ordering}, Arc};

#[derive(Debug, Clone)]
pub(crate) struct ExplorationBudgetRegistry { remaining: Arc<AtomicU32>, capacity: u32 }
impl ExplorationBudgetRegistry { pub(crate) fn new(capacity: u32) -> Self { Self { remaining: Arc::new(AtomicU32::new(capacity)), capacity } } pub(crate) fn reserve(&self) -> bool { self.remaining.fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| value.checked_sub(1)).is_ok() } pub(crate) fn release(&self) { self.remaining.fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| Some(value.saturating_add(1).min(self.capacity))).ok(); } pub(crate) fn remaining(&self) -> u32 { self.remaining.load(Ordering::Acquire) } }

pub(crate) fn derive_seed(root: &[u8], domain: &str, round: u64) -> [u8; 32] { let mut hasher = sha2::Sha256::new(); use sha2::Digest; hasher.update(root); hasher.update(domain.as_bytes()); hasher.update(round.to_be_bytes()); hasher.finalize().into() }

#[cfg(test)]
mod tests { use super::*; #[test] fn reservation_is_atomic_and_bounded() { let registry = ExplorationBudgetRegistry::new(1); assert!(registry.reserve()); assert!(!registry.reserve()); registry.release(); assert_eq!(registry.remaining(), 1); } }
