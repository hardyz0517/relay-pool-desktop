use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CapacitySnapshot {
    pub in_flight: u64,
    pub waiting: u64,
}

#[derive(Debug, Default)]
pub struct CapacityRegistry {
    shared: Arc<CapacityShared>,
}

impl CapacityRegistry {
    pub fn try_acquire(&self, key: impl Into<String>, max_concurrency: i64) -> CapacityGuard {
        let key = key.into();
        let mut states = self
            .shared
            .states
            .lock()
            .expect("capacity registry poisoned");
        let capacity = states.entry(key.clone()).or_default();
        if max_concurrency > 0 && capacity.in_flight >= max_concurrency as u64 {
            return CapacityGuard::rejected(key);
        }

        capacity.in_flight += 1;
        CapacityGuard::new_acquired(Arc::clone(&self.shared), key)
    }

    pub fn try_enter_wait(&self, key: impl Into<String>, max_waiting: u64) -> WaitingPermit {
        let key = key.into();
        let mut states = self
            .shared
            .states
            .lock()
            .expect("capacity registry poisoned");
        let capacity = states.entry(key.clone()).or_default();
        if capacity.waiting >= max_waiting {
            return WaitingPermit::rejected(key);
        }

        capacity.waiting += 1;
        WaitingPermit::new_admitted(Arc::clone(&self.shared), key)
    }

    pub fn wait_acquire(
        &self,
        key: impl Into<String>,
        max_concurrency: i64,
        max_waiting: u64,
        timeout: Duration,
    ) -> CapacityWaitResult {
        let key = key.into();
        let deadline = Instant::now() + timeout;
        let mut states = self
            .shared
            .states
            .lock()
            .expect("capacity registry poisoned");
        let capacity = states.entry(key.clone()).or_default();
        if max_waiting == 0 || capacity.waiting >= max_waiting {
            return CapacityWaitResult::QueueFull;
        }
        capacity.waiting += 1;

        loop {
            let capacity = states.entry(key.clone()).or_default();
            if max_concurrency <= 0 || capacity.in_flight < max_concurrency as u64 {
                capacity.waiting = capacity.waiting.saturating_sub(1);
                capacity.in_flight += 1;
                drop(states);
                return CapacityWaitResult::Acquired(CapacityGuard::new_acquired(
                    Arc::clone(&self.shared),
                    key,
                ));
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                capacity.waiting = capacity.waiting.saturating_sub(1);
                return CapacityWaitResult::TimedOut;
            }
            let (next_states, _) = self
                .shared
                .changed
                .wait_timeout(states, remaining)
                .expect("capacity registry poisoned");
            states = next_states;
        }
    }

    pub fn in_flight(&self, key: &str) -> u64 {
        self.snapshot(key).in_flight
    }

    pub fn waiting(&self, key: &str) -> u64 {
        self.snapshot(key).waiting
    }

    pub fn snapshot(&self, key: &str) -> CapacitySnapshot {
        let states = self
            .shared
            .states
            .lock()
            .expect("capacity registry poisoned");
        states
            .get(key)
            .map(|capacity| CapacitySnapshot {
                in_flight: capacity.in_flight,
                waiting: capacity.waiting,
            })
            .unwrap_or_default()
    }
}

pub fn effective_load_capacity(max_concurrency: i64, load_factor: i64) -> u64 {
    if load_factor > 0 {
        load_factor as u64
    } else if max_concurrency > 0 {
        max_concurrency as u64
    } else {
        1
    }
}

#[derive(Debug)]
pub enum CapacityWaitResult {
    Acquired(CapacityGuard),
    QueueFull,
    TimedOut,
}

#[derive(Debug, Default)]
struct CapacityShared {
    states: Mutex<HashMap<String, CapacityState>>,
    changed: Condvar,
}

#[derive(Debug, Default)]
struct CapacityState {
    in_flight: u64,
    waiting: u64,
}

#[derive(Debug)]
pub struct CapacityGuard {
    shared: Option<Arc<CapacityShared>>,
    key: String,
    acquired: bool,
    released: bool,
}

impl CapacityGuard {
    fn new_acquired(shared: Arc<CapacityShared>, key: String) -> Self {
        Self {
            shared: Some(shared),
            key,
            acquired: true,
            released: false,
        }
    }

    fn rejected(key: String) -> Self {
        Self {
            shared: None,
            key,
            acquired: false,
            released: true,
        }
    }

    pub fn acquired(&self) -> bool {
        self.acquired
    }

    pub fn release(&mut self) {
        if !self.acquired || self.released {
            return;
        }
        self.released = true;
        if let Some(shared) = &self.shared {
            let mut states = shared.states.lock().expect("capacity registry poisoned");
            if let Some(capacity) = states.get_mut(&self.key) {
                capacity.in_flight = capacity.in_flight.saturating_sub(1);
            }
            drop(states);
            shared.changed.notify_all();
        }
    }
}

impl Drop for CapacityGuard {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Debug)]
pub struct WaitingPermit {
    shared: Option<Arc<CapacityShared>>,
    key: String,
    admitted: bool,
    released: bool,
}

impl WaitingPermit {
    fn new_admitted(shared: Arc<CapacityShared>, key: String) -> Self {
        Self {
            shared: Some(shared),
            key,
            admitted: true,
            released: false,
        }
    }

    fn rejected(key: String) -> Self {
        Self {
            shared: None,
            key,
            admitted: false,
            released: true,
        }
    }

    pub fn admitted(&self) -> bool {
        self.admitted
    }

    pub fn release(&mut self) {
        if !self.admitted || self.released {
            return;
        }
        self.released = true;
        if let Some(shared) = &self.shared {
            let mut states = shared.states.lock().expect("capacity registry poisoned");
            if let Some(capacity) = states.get_mut(&self.key) {
                capacity.waiting = capacity.waiting.saturating_sub(1);
            }
            drop(states);
            shared.changed.notify_all();
        }
    }
}

impl Drop for WaitingPermit {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn zero_max_concurrency_is_unlimited_and_capacity_defaults_to_one() {
        let registry = CapacityRegistry::default();

        let first = registry.try_acquire("key-a", 0);
        let second = registry.try_acquire("key-a", 0);

        assert!(first.acquired());
        assert!(second.acquired());
        assert_eq!(registry.in_flight("key-a"), 2);
        assert_eq!(effective_load_capacity(0, 0), 1);
    }

    #[test]
    fn positive_max_concurrency_blocks_when_full() {
        let registry = CapacityRegistry::default();

        let first = registry.try_acquire("key-a", 1);
        let second = registry.try_acquire("key-a", 1);

        assert!(first.acquired());
        assert!(!second.acquired());
        assert_eq!(registry.in_flight("key-a"), 1);
    }

    #[test]
    fn release_guard_decrements_once_even_when_released_twice_and_dropped() {
        let registry = CapacityRegistry::default();
        let mut guard = registry.try_acquire("key-a", 1);

        assert!(guard.acquired());
        assert_eq!(registry.in_flight("key-a"), 1);

        guard.release();
        guard.release();
        drop(guard);

        assert_eq!(registry.in_flight("key-a"), 0);
    }

    #[test]
    fn waiting_admits_until_max_then_rejects_and_drop_decrements() {
        let registry = CapacityRegistry::default();

        let first = registry.try_enter_wait("key-a", 1);
        let second = registry.try_enter_wait("key-a", 1);

        assert!(first.admitted());
        assert!(!second.admitted());
        assert_eq!(registry.waiting("key-a"), 1);

        drop(first);

        assert_eq!(registry.waiting("key-a"), 0);
    }

    #[test]
    fn positive_load_factor_takes_precedence_over_max_concurrency() {
        assert_eq!(effective_load_capacity(3, 9), 9);
        assert_eq!(effective_load_capacity(3, 0), 3);
        assert_eq!(effective_load_capacity(0, -1), 1);
    }

    #[test]
    fn bounded_wait_acquires_after_release_and_cleans_waiter() {
        let registry = Arc::new(CapacityRegistry::default());
        let first = registry.try_acquire("key-a", 1);
        let waiter_registry = Arc::clone(&registry);
        let waiter = std::thread::spawn(move || {
            waiter_registry.wait_acquire("key-a", 1, 1, Duration::from_secs(1))
        });
        let deadline = Instant::now() + Duration::from_secs(1);
        while registry.waiting("key-a") != 1 && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(registry.waiting("key-a"), 1);
        drop(first);
        assert!(matches!(
            waiter.join().unwrap(),
            CapacityWaitResult::Acquired(_)
        ));
        assert_eq!(registry.waiting("key-a"), 0);
    }

    #[test]
    fn bounded_wait_reports_queue_full_and_timeout() {
        let registry = Arc::new(CapacityRegistry::default());
        let _active = registry.try_acquire("key-a", 1);
        let admitted = registry.try_enter_wait("key-a", 1);
        assert!(matches!(
            registry.wait_acquire("key-a", 1, 1, Duration::from_millis(5)),
            CapacityWaitResult::QueueFull
        ));
        drop(admitted);
        assert!(matches!(
            registry.wait_acquire("key-a", 1, 1, Duration::from_millis(5)),
            CapacityWaitResult::TimedOut
        ));
        assert_eq!(registry.waiting("key-a"), 0);
    }
}
