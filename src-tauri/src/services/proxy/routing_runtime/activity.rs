//! Process-local request activity and admission capacity.
//!
//! The composite capacity registry is the source for active station/key
//! counts. It is intentionally kept here with the request activity counter:
//! both describe only the currently running proxy instance and are released
//! through the existing lease RAII path.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use crate::application::routing_engine::capacity::CompositeCapacityRegistry;
#[cfg(test)]
use crate::application::routing_engine::capacity::{CapacityLease, CompositeCapacityRequest};

#[derive(Debug)]
pub(crate) struct ActivityState {
    in_flight: AtomicU64,
    capacity: Arc<CompositeCapacityRegistry>,
}

impl ActivityState {
    pub(crate) fn new() -> Self {
        Self {
            in_flight: AtomicU64::new(0),
            capacity: Arc::new(CompositeCapacityRegistry::default()),
        }
    }

    pub(crate) fn in_flight(&self) -> u32 {
        self.in_flight
            .load(Ordering::Acquire)
            .min(u64::from(u32::MAX)) as u32
    }

    pub(crate) fn capacity_registry(&self) -> Arc<CompositeCapacityRegistry> {
        Arc::clone(&self.capacity)
    }

    pub(crate) fn active_for_station(
        &self,
        station_type: &str,
        station_id: &str,
        station_key_id: &str,
    ) -> i64 {
        let constraint = if matches!(
            station_type.trim().to_ascii_lowercase().as_str(),
            "sub2api" | "newapi"
        ) {
            crate::application::routing_engine::capacity::CapacityConstraintKey::StationAccount(
                station_id.to_string(),
            )
        } else {
            crate::application::routing_engine::capacity::CapacityConstraintKey::StationKey(
                station_key_id.to_string(),
            )
        };
        i64::from(self.capacity.active_for(&constraint))
    }

    pub(crate) fn active_for_station_key(&self, station_key_id: &str) -> i64 {
        i64::from(self.capacity.active_for_station_key(station_key_id))
    }

    #[cfg(test)]
    pub(crate) fn acquire(
        &self,
        request: CompositeCapacityRequest,
    ) -> Result<
        ActivityLease<'_>,
        crate::application::routing_engine::capacity::CapacityAcquireFailure,
    > {
        let lease = self.capacity.try_acquire(request)?;
        self.in_flight.fetch_add(1, Ordering::AcqRel);
        Ok(ActivityLease {
            activity: self,
            lease: Some(lease),
        })
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct ActivityLease<'a> {
    activity: &'a ActivityState,
    lease: Option<CapacityLease>,
}

#[cfg(test)]
impl ActivityLease<'_> {
    pub(crate) fn release(&mut self) {
        if let Some(mut lease) = self.lease.take() {
            lease.release();
            self.activity.in_flight.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

#[cfg(test)]
impl Drop for ActivityLease<'_> {
    fn drop(&mut self) {
        self.release();
    }
}
