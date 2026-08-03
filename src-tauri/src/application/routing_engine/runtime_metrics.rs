#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[cfg(test)]
const RUNTIME_OUTLIER_POLICY_VERSION: &str = "runtime_outlier_policy_v1";
const MODEL_CLASS_MAX_LEN: usize = 64;

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RuntimeEndpointKind {
    ChatCompletions,
    Responses,
    ModelCatalog,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RuntimeModelClass {
    Named(String),
    Other,
}

impl RuntimeModelClass {
    pub(crate) fn normalize(model: Option<&str>) -> Self {
        let Some(model) = model else {
            return Self::Other;
        };
        let normalized = model.trim().to_ascii_lowercase();
        if normalized.is_empty() || normalized.len() > MODEL_CLASS_MAX_LEN {
            return Self::Other;
        }
        if !normalized.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        }) {
            return Self::Other;
        }
        Self::Named(normalized)
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RuntimeMetricKey {
    pub(crate) station_key_id: String,
    pub(crate) endpoint_kind: RuntimeEndpointKind,
    pub(crate) model_class: RuntimeModelClass,
    pub(crate) endpoint_revision: i64,
    pub(crate) credential_revision: i64,
}

#[cfg(test)]
impl RuntimeMetricKey {
    pub(crate) fn new(
        station_key_id: impl Into<String>,
        endpoint_kind: RuntimeEndpointKind,
        model: Option<&str>,
        endpoint_revision: i64,
        credential_revision: i64,
    ) -> Self {
        Self {
            station_key_id: station_key_id.into(),
            endpoint_kind,
            model_class: RuntimeModelClass::normalize(model),
            endpoint_revision,
            credential_revision,
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeOutlierPolicyV1 {
    pub(crate) version: &'static str,
    pub(crate) failure_window_max_samples: usize,
    pub(crate) failure_window_ms: i64,
    pub(crate) failure_window_min_samples: usize,
    pub(crate) failure_threshold_percent: u8,
    pub(crate) base_cooldown_ms: i64,
    pub(crate) max_cooldown_ms: i64,
    pub(crate) retry_after_min_ms: i64,
    pub(crate) retry_after_max_ms: i64,
    pub(crate) max_passive_ejection_percent: u8,
    pub(crate) half_open_successes_to_recover: u8,
    pub(crate) slow_start_ms: i64,
    pub(crate) max_entries: usize,
    pub(crate) entry_ttl_ms: i64,
}

#[cfg(test)]
impl Default for RuntimeOutlierPolicyV1 {
    fn default() -> Self {
        Self {
            version: RUNTIME_OUTLIER_POLICY_VERSION,
            failure_window_max_samples: 20,
            failure_window_ms: 5 * 60 * 1_000,
            failure_window_min_samples: 5,
            failure_threshold_percent: 60,
            base_cooldown_ms: 30_000,
            max_cooldown_ms: 15 * 60 * 1_000,
            retry_after_min_ms: 1_000,
            retry_after_max_ms: 60 * 60 * 1_000,
            max_passive_ejection_percent: 50,
            half_open_successes_to_recover: 2,
            slow_start_ms: 60_000,
            max_entries: 1_024,
            entry_ttl_ms: 30 * 60 * 1_000,
        }
    }
}

#[cfg(test)]
impl RuntimeOutlierPolicyV1 {
    pub(crate) fn validate(&self) -> Result<(), RuntimePolicyError> {
        if self.failure_window_max_samples != 20
            || self.failure_window_ms != 5 * 60 * 1_000
            || self.failure_window_min_samples != 5
            || self.failure_threshold_percent != 60
            || self.base_cooldown_ms != 30_000
            || self.max_cooldown_ms != 15 * 60 * 1_000
            || self.retry_after_min_ms != 1_000
            || self.retry_after_max_ms != 60 * 60 * 1_000
            || self.max_passive_ejection_percent != 50
            || self.half_open_successes_to_recover != 2
            || self.slow_start_ms != 60_000
        {
            return Err(RuntimePolicyError::InvalidV1Contract);
        }
        if self.max_entries == 0 || self.entry_ttl_ms <= 0 {
            return Err(RuntimePolicyError::InvalidBounds);
        }
        Ok(())
    }

    fn cooldown_for_ejection(&self, ejection_count: u32) -> i64 {
        let multiplier_shift = ejection_count.saturating_sub(1).min(10);
        self.base_cooldown_ms
            .saturating_mul(1_i64 << multiplier_shift)
            .min(self.max_cooldown_ms)
    }

    fn clamp_retry_after(&self, retry_after_ms: i64) -> Option<i64> {
        if retry_after_ms <= 0 {
            return None;
        }
        Some(retry_after_ms.clamp(self.retry_after_min_ms, self.retry_after_max_ms))
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimePolicyError {
    InvalidV1Contract,
    InvalidBounds,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeFailureKind {
    Ordinary,
    RateLimited { retry_after_ms: Option<i64> },
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeAttemptObservation {
    Success,
    Failure(RuntimeFailureKind),
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeFeedbackOutcome {
    pub(crate) applied: bool,
    pub(crate) policy_version: &'static str,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RuntimeOverlaySnapshot {
    pub(crate) policy_version: &'static str,
    pub(crate) entries: BTreeMap<RuntimeMetricKey, RuntimeCandidateOverlay>,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RuntimeCandidateOverlay {
    pub(crate) admission: RuntimeAdmission,
    pub(crate) failure_rate: f64,
    pub(crate) slow_start_penalty: f64,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeAdmission {
    Available,
    Degraded { reason: RuntimeDegradedReason },
    Suppressed { until_ms: i64 },
    HalfOpen { successes: u8 },
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeDegradedReason {
    MaxPassiveEjectionProtected,
    SingleCandidateOutlierProtected,
    SlowStart,
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct RuntimeRouteState {
    policy: RuntimeOutlierPolicyV1,
    entries: BTreeMap<RuntimeMetricKey, RuntimeMetricEntry>,
    applied_attempt_ids: BTreeSet<String>,
}

#[cfg(test)]
impl RuntimeRouteState {
    pub(crate) fn new(policy: RuntimeOutlierPolicyV1) -> Result<Self, RuntimePolicyError> {
        policy.validate()?;
        Ok(Self {
            policy,
            entries: BTreeMap::new(),
            applied_attempt_ids: BTreeSet::new(),
        })
    }

    pub(crate) fn policy_version(&self) -> &'static str {
        self.policy.version
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn has_entry(&self, key: &RuntimeMetricKey) -> bool {
        self.entries.contains_key(key)
    }

    pub(crate) fn report_attempt(
        &mut self,
        attempt_id: impl Into<String>,
        key: RuntimeMetricKey,
        observation: RuntimeAttemptObservation,
        now_ms: i64,
    ) -> RuntimeFeedbackOutcome {
        let attempt_id = attempt_id.into();
        if !self.applied_attempt_ids.insert(attempt_id) {
            return RuntimeFeedbackOutcome {
                applied: false,
                policy_version: self.policy.version,
            };
        }
        self.cleanup(now_ms);
        let policy = self.policy.clone();
        let entry = self.entries.entry(key).or_default();
        entry.last_touched_ms = now_ms;
        entry.record_observation(&policy, observation, now_ms);
        self.enforce_bounds();
        RuntimeFeedbackOutcome {
            applied: true,
            policy_version: self.policy.version,
        }
    }

    pub(crate) fn retain_live_revisions(&mut self, live_keys: &[RuntimeMetricKey]) -> usize {
        let live = live_keys.iter().collect::<BTreeSet<_>>();
        let before = self.entries.len();
        self.entries.retain(|key, _| live.contains(key));
        before.saturating_sub(self.entries.len())
    }

    pub(crate) fn cleanup(&mut self, now_ms: i64) -> usize {
        let before = self.entries.len();
        let ttl_ms = self.policy.entry_ttl_ms;
        self.entries
            .retain(|_, entry| now_ms.saturating_sub(entry.last_touched_ms) <= ttl_ms);
        before.saturating_sub(self.entries.len())
    }

    pub(crate) fn snapshot_overlay(
        &mut self,
        keys: &[RuntimeMetricKey],
        now_ms: i64,
    ) -> RuntimeOverlaySnapshot {
        self.cleanup(now_ms);
        let raw_suppressed = keys
            .iter()
            .filter(|key| {
                self.entries
                    .get(*key)
                    .is_some_and(|entry| entry.is_suppressed(now_ms))
            })
            .cloned()
            .collect::<Vec<_>>();
        let max_suppressed = keys
            .len()
            .saturating_mul(usize::from(self.policy.max_passive_ejection_percent))
            / 100;
        let allowed_suppressed = raw_suppressed
            .iter()
            .take(max_suppressed)
            .cloned()
            .collect::<BTreeSet<_>>();

        let mut entries = BTreeMap::new();
        for key in keys {
            let Some(entry) = self.entries.get(key) else {
                entries.insert(
                    key.clone(),
                    RuntimeCandidateOverlay {
                        admission: RuntimeAdmission::Available,
                        failure_rate: 0.0,
                        slow_start_penalty: 0.0,
                    },
                );
                continue;
            };

            let admission = if entry.is_suppressed(now_ms) {
                if keys.len() == 1 {
                    RuntimeAdmission::Degraded {
                        reason: RuntimeDegradedReason::SingleCandidateOutlierProtected,
                    }
                } else if allowed_suppressed.contains(key) {
                    RuntimeAdmission::Suppressed {
                        until_ms: entry.cooldown_until_ms.unwrap_or(now_ms),
                    }
                } else {
                    RuntimeAdmission::Degraded {
                        reason: RuntimeDegradedReason::MaxPassiveEjectionProtected,
                    }
                }
            } else if entry.needs_half_open_probe(now_ms) {
                RuntimeAdmission::HalfOpen {
                    successes: entry.half_open_successes,
                }
            } else if entry
                .slow_start_until_ms
                .is_some_and(|until| until > now_ms)
            {
                RuntimeAdmission::Degraded {
                    reason: RuntimeDegradedReason::SlowStart,
                }
            } else {
                RuntimeAdmission::Available
            };

            entries.insert(
                key.clone(),
                RuntimeCandidateOverlay {
                    admission,
                    failure_rate: entry.failure_rate(now_ms, self.policy.failure_window_ms),
                    slow_start_penalty: entry.slow_start_penalty(now_ms, self.policy.slow_start_ms),
                },
            );
        }

        RuntimeOverlaySnapshot {
            policy_version: self.policy.version,
            entries,
        }
    }

    pub(crate) fn try_acquire_half_open_probe<'a>(
        &'a mut self,
        key: &RuntimeMetricKey,
        now_ms: i64,
    ) -> Option<HalfOpenProbePermit<'a>> {
        let entry = self.entries.get_mut(key)?;
        if !entry.needs_half_open_probe(now_ms) || entry.half_open_in_flight {
            return None;
        }
        entry.half_open_in_flight = true;
        Some(HalfOpenProbePermit {
            state: self,
            key: key.clone(),
            completed: false,
        })
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

    fn record_half_open_success(&mut self, key: &RuntimeMetricKey, now_ms: i64) {
        let Some(entry) = self.entries.get_mut(key) else {
            return;
        };
        entry.half_open_in_flight = false;
        entry.last_touched_ms = now_ms;
        entry.half_open_successes = entry.half_open_successes.saturating_add(1);
        if entry.half_open_successes >= self.policy.half_open_successes_to_recover {
            entry.cooldown_until_ms = None;
            entry.ejection_count = 0;
            entry.half_open_successes = 0;
            entry.slow_start_until_ms = Some(now_ms.saturating_add(self.policy.slow_start_ms));
        }
    }

    fn record_half_open_failure(&mut self, key: &RuntimeMetricKey, now_ms: i64) {
        let Some(entry) = self.entries.get_mut(key) else {
            return;
        };
        entry.half_open_in_flight = false;
        entry.last_touched_ms = now_ms;
        entry.half_open_successes = 0;
        entry.eject(&self.policy, None, now_ms);
    }

    fn release_half_open(&mut self, key: &RuntimeMetricKey) {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.half_open_in_flight = false;
        }
    }
}

#[cfg(test)]
impl Default for RuntimeRouteState {
    fn default() -> Self {
        Self::new(RuntimeOutlierPolicyV1::default()).expect("default runtime policy is valid")
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct HalfOpenProbePermit<'a> {
    state: &'a mut RuntimeRouteState,
    key: RuntimeMetricKey,
    completed: bool,
}

#[cfg(test)]
impl HalfOpenProbePermit<'_> {
    pub(crate) fn record_success(mut self, now_ms: i64) {
        self.state
            .record_half_open_success(&self.key.clone(), now_ms);
        self.completed = true;
    }

    pub(crate) fn record_failure(mut self, now_ms: i64) {
        self.state
            .record_half_open_failure(&self.key.clone(), now_ms);
        self.completed = true;
    }
}

#[cfg(test)]
impl Drop for HalfOpenProbePermit<'_> {
    fn drop(&mut self) {
        if !self.completed {
            self.state.release_half_open(&self.key.clone());
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Default)]
struct RuntimeMetricEntry {
    failure_window: VecDeque<RuntimeOutcomeSample>,
    cooldown_until_ms: Option<i64>,
    ejection_count: u32,
    half_open_in_flight: bool,
    half_open_successes: u8,
    slow_start_until_ms: Option<i64>,
    last_touched_ms: i64,
}

#[cfg(test)]
impl RuntimeMetricEntry {
    fn record_observation(
        &mut self,
        policy: &RuntimeOutlierPolicyV1,
        observation: RuntimeAttemptObservation,
        now_ms: i64,
    ) {
        self.failure_window.push_back(RuntimeOutcomeSample {
            failed: matches!(observation, RuntimeAttemptObservation::Failure(_)),
            observed_at_ms: now_ms,
        });
        self.trim_window(policy, now_ms);

        match observation {
            RuntimeAttemptObservation::Success => {
                if self.cooldown_until_ms.is_none() {
                    self.ejection_count = 0;
                    self.half_open_successes = 0;
                }
            }
            RuntimeAttemptObservation::Failure(RuntimeFailureKind::RateLimited {
                retry_after_ms,
            }) => {
                self.half_open_successes = 0;
                let retry_after_ms =
                    retry_after_ms.and_then(|value| policy.clamp_retry_after(value));
                self.eject(policy, retry_after_ms, now_ms);
            }
            RuntimeAttemptObservation::Failure(RuntimeFailureKind::Ordinary) => {
                self.half_open_successes = 0;
                if self.should_eject(policy, now_ms) {
                    self.eject(policy, None, now_ms);
                }
            }
        }
    }

    fn trim_window(&mut self, policy: &RuntimeOutlierPolicyV1, now_ms: i64) {
        while self.failure_window.len() > policy.failure_window_max_samples {
            self.failure_window.pop_front();
        }
        while self.failure_window.front().is_some_and(|sample| {
            now_ms.saturating_sub(sample.observed_at_ms) > policy.failure_window_ms
        }) {
            self.failure_window.pop_front();
        }
    }

    fn should_eject(&mut self, policy: &RuntimeOutlierPolicyV1, now_ms: i64) -> bool {
        self.trim_window(policy, now_ms);
        if self.failure_window.len() < policy.failure_window_min_samples {
            return false;
        }
        self.failure_rate(now_ms, policy.failure_window_ms)
            >= f64::from(policy.failure_threshold_percent) / 100.0
    }

    fn failure_rate(&self, now_ms: i64, window_ms: i64) -> f64 {
        let samples = self
            .failure_window
            .iter()
            .filter(|sample| now_ms.saturating_sub(sample.observed_at_ms) <= window_ms)
            .collect::<Vec<_>>();
        if samples.is_empty() {
            return 0.0;
        }
        let failures = samples.iter().filter(|sample| sample.failed).count();
        failures as f64 / samples.len() as f64
    }

    fn eject(&mut self, policy: &RuntimeOutlierPolicyV1, retry_after_ms: Option<i64>, now_ms: i64) {
        self.ejection_count = self.ejection_count.saturating_add(1);
        let cooldown_ms =
            retry_after_ms.unwrap_or_else(|| policy.cooldown_for_ejection(self.ejection_count));
        self.cooldown_until_ms = Some(now_ms.saturating_add(cooldown_ms));
        self.slow_start_until_ms = None;
    }

    fn is_suppressed(&self, now_ms: i64) -> bool {
        self.cooldown_until_ms.is_some_and(|until| until > now_ms)
    }

    fn needs_half_open_probe(&self, now_ms: i64) -> bool {
        self.cooldown_until_ms.is_some_and(|until| until <= now_ms)
            && self.half_open_successes
                < RuntimeOutlierPolicyV1::default().half_open_successes_to_recover
    }

    fn slow_start_penalty(&self, now_ms: i64, slow_start_ms: i64) -> f64 {
        let Some(until_ms) = self.slow_start_until_ms else {
            return 0.0;
        };
        if until_ms <= now_ms || slow_start_ms <= 0 {
            return 0.0;
        }
        let remaining = until_ms.saturating_sub(now_ms) as f64;
        (remaining / slow_start_ms as f64).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct RuntimeOutcomeSample {
    failed: bool,
    observed_at_ms: i64,
}

#[cfg(test)]
pub(crate) fn parse_retry_after_ms(value: Option<&str>) -> Option<i64> {
    let value = value?.trim();
    if value.is_empty() || value.starts_with('-') {
        return None;
    }
    let seconds = value.parse::<i64>().ok()?;
    if seconds <= 0 {
        return None;
    }
    Some(seconds.saturating_mul(1_000))
}
