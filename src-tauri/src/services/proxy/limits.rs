use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const BODY_BUDGET_UNIT_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyServerLimits {
    pub max_connections: usize,
    pub max_in_flight_requests: usize,
    pub max_header_bytes: usize,
    pub max_body_bytes: usize,
    pub max_buffered_body_bytes: usize,
    pub header_timeout: Duration,
    pub body_timeout: Duration,
    pub upstream_connect_timeout: Duration,
    pub upstream_first_byte_timeout: Duration,
    pub precommit_timeout: Duration,
    pub buffered_execution_timeout: Duration,
    pub stream_idle_timeout: Duration,
    pub shutdown_timeout: Duration,
}

impl Default for ProxyServerLimits {
    fn default() -> Self {
        Self {
            max_connections: 64,
            max_in_flight_requests: 32,
            max_header_bytes: 64 * 1024,
            max_body_bytes: 32 * 1024 * 1024,
            max_buffered_body_bytes: 128 * 1024 * 1024,
            header_timeout: Duration::from_secs(10),
            body_timeout: Duration::from_secs(30),
            upstream_connect_timeout: Duration::from_secs(10),
            upstream_first_byte_timeout: Duration::from_secs(30),
            precommit_timeout: Duration::from_secs(60),
            buffered_execution_timeout: Duration::from_secs(300),
            stream_idle_timeout: Duration::from_secs(90),
            shutdown_timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BodyBudget {
    capacity_bytes: usize,
    semaphore: Arc<Semaphore>,
}

impl BodyBudget {
    pub fn new(capacity_bytes: usize) -> Self {
        let permits = bytes_to_permits(capacity_bytes);
        Self {
            capacity_bytes: permits * BODY_BUDGET_UNIT_BYTES,
            semaphore: Arc::new(Semaphore::new(permits)),
        }
    }

    pub async fn acquire(&self, bytes: usize) -> Result<BodyBudgetLease, BodyBudgetError> {
        let permits = bytes_to_permits(bytes);
        if permits > self.semaphore.available_permits() {
            return Err(BodyBudgetError::InsufficientCapacity);
        }
        let permit = Arc::clone(&self.semaphore)
            .try_acquire_many_owned(permits as u32)
            .map_err(|_| BodyBudgetError::InsufficientCapacity)?;
        Ok(BodyBudgetLease {
            _permit: Arc::new(permit),
        })
    }

    pub fn available_bytes(&self) -> usize {
        self.semaphore.available_permits() * BODY_BUDGET_UNIT_BYTES
    }

    pub fn capacity_bytes(&self) -> usize {
        self.capacity_bytes
    }
}

#[derive(Debug, Clone)]
pub struct BodyBudgetLease {
    _permit: Arc<OwnedSemaphorePermit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyBudgetError {
    InsufficientCapacity,
}

#[derive(Debug)]
pub struct RequestLease {
    _permit: OwnedSemaphorePermit,
    active_requests: Arc<AtomicU32>,
}

impl RequestLease {
    pub fn new(permit: OwnedSemaphorePermit, active_requests: Arc<AtomicU32>) -> Self {
        active_requests.fetch_add(1, Ordering::Relaxed);
        Self {
            _permit: permit,
            active_requests,
        }
    }
}

impl Drop for RequestLease {
    fn drop(&mut self) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
    }
}

fn bytes_to_permits(bytes: usize) -> usize {
    if bytes == 0 {
        0
    } else {
        bytes.div_ceil(BODY_BUDGET_UNIT_BYTES)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{BodyBudget, ProxyServerLimits};

    #[tokio::test]
    async fn body_budget_holds_bytes_until_last_request_owner_drops() {
        let budget = BodyBudget::new(4 * 1024);
        let lease = budget.acquire(3072).await.expect("lease");
        let clone = lease.clone();
        drop(lease);
        assert_eq!(budget.available_bytes(), 1024);
        drop(clone);
        assert_eq!(budget.available_bytes(), 4096);
    }

    #[test]
    fn proxy_server_limits_match_the_approved_budget() {
        let limits = ProxyServerLimits::default();
        assert_eq!(limits.max_connections, 64);
        assert_eq!(limits.max_in_flight_requests, 32);
        assert_eq!(limits.max_header_bytes, 64 * 1024);
        assert_eq!(limits.max_body_bytes, 32 * 1024 * 1024);
        assert_eq!(limits.max_buffered_body_bytes, 128 * 1024 * 1024);
        assert_eq!(limits.header_timeout, Duration::from_secs(10));
        assert_eq!(limits.body_timeout, Duration::from_secs(30));
        assert_eq!(limits.upstream_connect_timeout, Duration::from_secs(10));
        assert_eq!(limits.upstream_first_byte_timeout, Duration::from_secs(30));
        assert_eq!(limits.precommit_timeout, Duration::from_secs(60));
        assert_eq!(limits.buffered_execution_timeout, Duration::from_secs(300));
        assert_eq!(limits.stream_idle_timeout, Duration::from_secs(90));
        assert_eq!(limits.shutdown_timeout, Duration::from_secs(30));
    }
}
