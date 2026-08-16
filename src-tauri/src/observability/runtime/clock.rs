use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) const DEFAULT_CLOCK_JUMP_THRESHOLD_MS: u64 = 5 * 60 * 1000;
pub(crate) const DEFAULT_CLOCK_STABILITY_OBSERVATION_WINDOW_MS: u64 = 30 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClockAdjustment {
    None,
    Rollback,
    ForwardJump,
    NonMonotonic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClockObservation {
    pub(crate) at_ms: i64,
    pub(crate) monotonic_ms: u64,
    pub(crate) wall_delta_ms: Option<i64>,
    pub(crate) monotonic_delta_ms: Option<u64>,
    pub(crate) adjustment: ClockAdjustment,
}

#[derive(Debug)]
pub(crate) struct ClockGuard {
    anchor: Instant,
    previous_wall_ms: Option<i64>,
    previous_monotonic_ms: Option<u64>,
    jump_threshold_ms: u64,
    unstable: bool,
    unstable_since_monotonic_ms: Option<u64>,
    observing_since_monotonic_ms: Option<u64>,
    observation_window_ms: u64,
}

impl Default for ClockGuard {
    fn default() -> Self {
        Self::new(DEFAULT_CLOCK_JUMP_THRESHOLD_MS)
    }
}

impl ClockGuard {
    pub(crate) fn new(jump_threshold_ms: u64) -> Self {
        Self {
            anchor: Instant::now(),
            previous_wall_ms: None,
            previous_monotonic_ms: None,
            jump_threshold_ms,
            unstable: false,
            unstable_since_monotonic_ms: None,
            observing_since_monotonic_ms: None,
            observation_window_ms: DEFAULT_CLOCK_STABILITY_OBSERVATION_WINDOW_MS,
        }
    }

    pub(crate) fn is_stable(&self) -> bool {
        !self.unstable && self.observing_since_monotonic_ms.is_none()
    }

    pub(crate) fn sample_now(&mut self) -> ClockObservation {
        let wall_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(i64::MAX as u128) as i64;
        let monotonic_ms = self.anchor.elapsed().as_millis().min(u64::MAX as u128) as u64;
        self.sample_at(wall_ms, monotonic_ms)
    }

    pub(crate) fn sample_at(&mut self, wall_ms: i64, monotonic_ms: u64) -> ClockObservation {
        let wall_delta_ms = self.previous_wall_ms.map(|previous| wall_ms - previous);
        let monotonic_delta_ms = self
            .previous_monotonic_ms
            .map(|previous| monotonic_ms.saturating_sub(previous));
        let adjustment = match (wall_delta_ms, monotonic_delta_ms) {
            (Some(wall_delta), Some(_mono_delta)) if wall_delta < 0 => ClockAdjustment::Rollback,
            (Some(wall_delta), Some(mono_delta))
                if wall_delta.saturating_sub(mono_delta as i64) > self.jump_threshold_ms as i64 =>
            {
                ClockAdjustment::ForwardJump
            }
            // Both clocks are sampled at millisecond precision. A small wall
            // delta with a zero monotonic delta is therefore normal around a
            // tick boundary; require a materially large discrepancy before
            // classifying it as non-monotonic.
            (Some(wall_delta), Some(mono_delta)) if wall_delta > 1_000 && mono_delta == 0 => {
                ClockAdjustment::NonMonotonic
            }
            _ => ClockAdjustment::None,
        };
        if adjustment != ClockAdjustment::None {
            self.unstable = true;
            self.unstable_since_monotonic_ms = Some(monotonic_ms);
            self.observing_since_monotonic_ms = None;
        } else if self.previous_monotonic_ms.is_none() {
            // The first sample after acquiring the lease establishes the
            // wall/monotonic baseline. Do not permit age-based retention
            // until a full monotonic observation window has elapsed.
            self.observing_since_monotonic_ms = Some(monotonic_ms);
        } else if self.unstable {
            let recovered = self.unstable_since_monotonic_ms.is_some_and(|started| {
                monotonic_ms.saturating_sub(started) >= self.observation_window_ms
            });
            if recovered {
                self.unstable = false;
                self.unstable_since_monotonic_ms = None;
            }
        } else if self.observing_since_monotonic_ms.is_some_and(|started| {
            monotonic_ms.saturating_sub(started) >= self.observation_window_ms
        }) {
            self.observing_since_monotonic_ms = None;
        }
        self.previous_wall_ms = Some(wall_ms);
        self.previous_monotonic_ms = Some(monotonic_ms);
        ClockObservation {
            at_ms: wall_ms,
            monotonic_ms,
            wall_delta_ms,
            monotonic_delta_ms,
            adjustment,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Elapsed(Duration);

impl Elapsed {
    #[cfg(test)]
    pub(crate) fn from_duration(duration: Duration) -> Self {
        Self(duration)
    }

    pub(crate) fn as_millis(self) -> u64 {
        self.0.as_millis().min(u64::MAX as u128) as u64
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct MonotonicTimer(Instant);

#[cfg(test)]
impl MonotonicTimer {
    pub(crate) fn start() -> Self {
        Self(Instant::now())
    }

    pub(crate) fn elapsed(&self) -> Elapsed {
        Elapsed(self.0.elapsed())
    }
}
