//! Production health-protection domain primitives.
//!
//! This module deliberately owns neither SQLite writes nor request execution.
//! It is the single state-transition owner for a future cross-request health
//! protection path. Callers persist [`HealthProtectionSnapshotV1`] through the
//! existing observation transaction before publishing a durable status. The
//! capacity retry registry remains a separate process-local mechanism and must
//! not be fed into this reducer as a durable verdict.

use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const HEALTH_PROTECTION_VERSION: &str = "health_protection_v1";
const MAX_SCOPE_BYTES: usize = 192;
const MAX_OBSERVATION_ID_BYTES: usize = 160;
const MAX_PROFILE_SAMPLES: usize = 256;
const MAX_PROFILE_ENTRIES: usize = 4_096;
const MAX_PROFILE_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000;
const MAX_PROFILE_COOLDOWN_MS: i64 = 24 * 60 * 60 * 1_000;

fn default_health_protection_enabled() -> bool {
    true
}

/// The persistence kind is part of the read model, not a hint for callers to
/// merge otherwise independent mechanisms. `RuntimeOutlier` is reserved for
/// process-local overlays; durable admission is always backed by the
/// observation/reducer snapshot and never by the test-only outlier model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HealthProtectionPersistenceKind {
    Durable,
    RuntimeOutlier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HealthProtectionPreset {
    Conservative,
    Balanced,
    Aggressive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthProtectionProfileV1 {
    pub(crate) version: String,
    /// Policy-owned activation switch. The reducer keeps its state when the
    /// switch is turned off, but planner admission and observation ingestion
    /// must not consume it until re-enabled.
    #[serde(default = "default_health_protection_enabled")]
    pub(crate) enabled: bool,
    pub(crate) preset: HealthProtectionPreset,
    pub(crate) window_max_samples: usize,
    pub(crate) window_ms: i64,
    pub(crate) min_samples: usize,
    pub(crate) failure_threshold_percent: u8,
    pub(crate) base_cooldown_ms: i64,
    pub(crate) max_cooldown_ms: i64,
    pub(crate) half_open_successes_to_close: u8,
    pub(crate) max_entries: usize,
}

impl HealthProtectionProfileV1 {
    pub(crate) fn from_policy_config(
        config: &crate::models::routing_policy::ProtectionProfileConfigV2,
    ) -> Result<Self, HealthProtectionError> {
        config
            .validate()
            .map_err(|_| HealthProtectionError::InvalidProfile)?;
        let mut profile = Self::for_preset(HealthProtectionPreset::Balanced);
        profile.window_max_samples = usize::from(config.window_max_samples);
        profile.window_ms = config.window_millis();
        profile.min_samples = usize::from(config.min_samples);
        profile.failure_threshold_percent = config.failure_threshold_percent;
        profile.half_open_successes_to_close = config.half_open_successes_to_close;
        profile.enabled = config.enabled;
        profile.validate()?;
        Ok(profile)
    }

    pub(crate) fn for_preset(preset: HealthProtectionPreset) -> Self {
        let (
            window_max_samples,
            min_samples,
            threshold,
            base_cooldown_ms,
            max_cooldown_ms,
            half_open_successes_to_close,
        ) = match preset {
            // Cooldown and entry caps remain system-owned safety limits. The
            // policy may tune only the bounded observation parameters below.
            HealthProtectionPreset::Conservative => (32, 5, 50, 60_000, 15 * 60 * 1_000, 3),
            HealthProtectionPreset::Balanced => (64, 5, 60, 30_000, 15 * 60 * 1_000, 2),
            HealthProtectionPreset::Aggressive => (128, 8, 75, 15_000, 5 * 60 * 1_000, 1),
        };
        Self {
            version: HEALTH_PROTECTION_VERSION.to_string(),
            enabled: true,
            preset,
            window_max_samples,
            window_ms: 5 * 60 * 1_000,
            min_samples,
            failure_threshold_percent: threshold,
            base_cooldown_ms,
            max_cooldown_ms,
            half_open_successes_to_close,
            max_entries: 1_024,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), HealthProtectionError> {
        if self.version != HEALTH_PROTECTION_VERSION {
            return Err(HealthProtectionError::UnsupportedVersion);
        }
        if self.window_max_samples == 0 || self.window_max_samples > MAX_PROFILE_SAMPLES {
            return Err(HealthProtectionError::InvalidProfile);
        }
        if self.window_ms <= 0 || self.window_ms > MAX_PROFILE_WINDOW_MS {
            return Err(HealthProtectionError::InvalidProfile);
        }
        if self.min_samples == 0 || self.min_samples > self.window_max_samples {
            return Err(HealthProtectionError::InvalidProfile);
        }
        if !(1..=100).contains(&self.failure_threshold_percent) {
            return Err(HealthProtectionError::InvalidProfile);
        }
        if self.base_cooldown_ms <= 0
            || self.max_cooldown_ms < self.base_cooldown_ms
            || self.max_cooldown_ms > MAX_PROFILE_COOLDOWN_MS
        {
            return Err(HealthProtectionError::InvalidProfile);
        }
        if self.half_open_successes_to_close == 0 || self.max_entries == 0 {
            return Err(HealthProtectionError::InvalidProfile);
        }
        if self.max_entries > MAX_PROFILE_ENTRIES {
            return Err(HealthProtectionError::InvalidProfile);
        }
        Ok(())
    }

    fn cooldown_for_open_count(&self, open_count: u32) -> i64 {
        let shift = open_count.saturating_sub(1).min(10);
        self.base_cooldown_ms
            .saturating_mul(1_i64 << shift)
            .min(self.max_cooldown_ms)
    }
}

impl Default for HealthProtectionProfileV1 {
    fn default() -> Self {
        Self::for_preset(HealthProtectionPreset::Balanced)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HealthProtectionScopeKind {
    Credential,
    Account,
    Group,
    Endpoint,
    Model,
    CapacityDomain,
}

/// Canonical identity used to derive the opaque key shared by durable health
/// persistence and application-side reducer scopes. Keep this tuple-shaped
/// encoding stable because existing SQLite rows retain the resulting key.
pub(crate) struct DurableHealthScopeCommitmentInput<'identity> {
    pub(crate) scope_kind: &'identity str,
    pub(crate) station_id: &'identity str,
    pub(crate) station_key_id: Option<&'identity str>,
    pub(crate) group_binding_id: Option<&'identity str>,
    pub(crate) resolved_model_commitment: Option<&'identity str>,
    pub(crate) credential_revision: Option<i64>,
    pub(crate) account_revision: Option<i64>,
    pub(crate) group_revision: Option<i64>,
    pub(crate) endpoint_revision: Option<i64>,
    pub(crate) model_alias_revision: Option<i64>,
}

pub(crate) fn durable_health_scope_commitment(
    input: DurableHealthScopeCommitmentInput<'_>,
) -> Option<String> {
    let canonical = serde_json::to_vec(&(
        input.scope_kind,
        input.station_id,
        input.station_key_id,
        input.group_binding_id,
        input.resolved_model_commitment,
        input.credential_revision,
        input.account_revision,
        input.group_revision,
        input.endpoint_revision,
        input.model_alias_revision,
    ))
    .ok()?;
    Some(format!(
        "{}:v1:{:x}",
        input.scope_kind,
        Sha256::digest(canonical)
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthProtectionScope {
    pub(crate) kind: HealthProtectionScopeKind,
    /// A commitment, never a raw endpoint, account, credential or request ID.
    pub(crate) commitment: String,
}

impl HealthProtectionScope {
    pub(crate) fn new(
        kind: HealthProtectionScopeKind,
        commitment: impl Into<String>,
    ) -> Result<Self, HealthProtectionError> {
        let commitment = commitment.into();
        let scope = Self { kind, commitment };
        scope.validate()?;
        Ok(scope)
    }

    fn validate(&self) -> Result<(), HealthProtectionError> {
        if self.commitment.is_empty()
            || self.commitment.len() > MAX_SCOPE_BYTES
            || !self.commitment.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            })
        {
            return Err(HealthProtectionError::InvalidScope);
        }
        Ok(())
    }

    /// Hashes an untrusted source into a bounded commitment. This is useful at
    /// an adapter boundary, where the source may contain a provider URL or a
    /// local identifier that must never be emitted in status or metrics.
    pub(crate) fn from_untrusted(kind: HealthProtectionScopeKind, value: &str) -> Self {
        let digest = Sha256::digest(value.as_bytes());
        let commitment = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        Self { kind, commitment }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HealthProtectionFailureCode {
    ConnectFailure,
    FirstByteTimeout,
    Upstream5xx,
    RateLimited,
    CapacityExhausted,
    EndpointUnavailable,
    Unknown,
}

impl HealthProtectionFailureCode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ConnectFailure => "connect_failure",
            Self::FirstByteTimeout => "first_byte_timeout",
            Self::Upstream5xx => "upstream_5xx",
            Self::RateLimited => "rate_limited",
            Self::CapacityExhausted => "capacity_exhausted",
            Self::EndpointUnavailable => "endpoint_unavailable",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HealthProtectionObservationOutcome {
    Success,
    Failure(HealthProtectionFailureCode),
}

impl HealthProtectionObservationOutcome {
    pub(crate) fn is_failure(self) -> bool {
        matches!(self, Self::Failure(_))
    }

    pub(crate) fn failure_code(self) -> Option<HealthProtectionFailureCode> {
        match self {
            Self::Failure(code) => Some(code),
            Self::Success => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthProtectionObservation {
    pub(crate) id: String,
    pub(crate) scope: HealthProtectionScope,
    pub(crate) observed_at_ms: i64,
    pub(crate) outcome: HealthProtectionObservationOutcome,
    pub(crate) probe: bool,
    /// The revision returned by `begin_probe`; this prevents a late result
    /// from closing a newer half-open cycle.
    pub(crate) probe_state_revision: Option<u64>,
    pub(crate) retry_after_ms: Option<i64>,
}

impl HealthProtectionObservation {
    fn validate(&self) -> Result<(), HealthProtectionError> {
        if self.id.is_empty()
            || self.id.len() > MAX_OBSERVATION_ID_BYTES
            || self.id.chars().any(char::is_control)
            || self.observed_at_ms < 0
        {
            return Err(HealthProtectionError::InvalidObservation);
        }
        self.scope.validate()?;
        if self
            .retry_after_ms
            .is_some_and(|value| !(0..=MAX_PROFILE_COOLDOWN_MS).contains(&value))
        {
            return Err(HealthProtectionError::InvalidObservation);
        }
        if self.probe != self.probe_state_revision.is_some()
            || self
                .probe_state_revision
                .is_some_and(|revision| revision == 0)
        {
            return Err(HealthProtectionError::InvalidObservation);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HealthProtectionState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HealthProtectionTransition {
    IgnoredDuplicate,
    Observed,
    Opened,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-health-transition-reference; owner=application/health_protection; remove_when=legacy scoped health reducer is deleted"
        )
    )]
    ProbeSucceeded,
    Closed,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-health-transition-reference; owner=application/health_protection; remove_when=legacy scoped health reducer is deleted"
        )
    )]
    Reopened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HealthProtectionError {
    UnsupportedVersion,
    InvalidProfile,
    InvalidScope,
    InvalidObservation,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-health-scope-error; owner=application/health_protection; remove_when=legacy scoped health adapter is retired"
        )
    )]
    ScopeNotFound,
    ProbeNotAllowed,
    ProbeAlreadyInFlight,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-health-stale-result-reference; owner=application/health_protection; remove_when=legacy scoped health reducer is deleted"
        )
    )]
    StaleProbe,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-health-stale-result-reference; owner=application/health_protection; remove_when=legacy scoped health reducer is deleted"
        )
    )]
    StaleObservation,
    EntryLimit,
    SnapshotInvalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowSample {
    observed_at_ms: i64,
    failed: bool,
    failure_code: Option<HealthProtectionFailureCode>,
}

/// A bounded sliding window. IDs are retained separately for bounded
/// idempotency; they are never exposed in the status projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthObservationWindowV1 {
    samples: VecDeque<WindowSample>,
    seen_ids: VecDeque<String>,
}

impl HealthObservationWindowV1 {
    fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            seen_ids: VecDeque::new(),
        }
    }

    fn record(
        &mut self,
        observation: &HealthProtectionObservation,
        profile: &HealthProtectionProfileV1,
    ) -> bool {
        if self.seen_ids.iter().any(|id| id == &observation.id) {
            return false;
        }
        self.seen_ids.push_back(observation.id.clone());
        while self.seen_ids.len() > profile.window_max_samples.saturating_mul(4).max(16) {
            self.seen_ids.pop_front();
        }
        self.samples.push_back(WindowSample {
            observed_at_ms: observation.observed_at_ms,
            failed: observation.outcome.is_failure(),
            failure_code: observation.outcome.failure_code(),
        });
        self.trim(profile, observation.observed_at_ms);
        true
    }

    fn trim(&mut self, profile: &HealthProtectionProfileV1, now_ms: i64) {
        while self.samples.len() > profile.window_max_samples {
            self.samples.pop_front();
        }
        while self
            .samples
            .front()
            .is_some_and(|sample| now_ms.saturating_sub(sample.observed_at_ms) > profile.window_ms)
        {
            self.samples.pop_front();
        }
    }

    fn summary(&mut self, profile: &HealthProtectionProfileV1, now_ms: i64) -> WindowSummary {
        self.trim(profile, now_ms);
        let sample_count = self.samples.len();
        let failure_count = self.samples.iter().filter(|sample| sample.failed).count();
        WindowSummary {
            sample_count,
            failure_count,
            failure_rate_percent: if sample_count == 0 {
                0
            } else {
                ((failure_count * 100) / sample_count).min(100) as u8
            },
            last_failure_code: self
                .samples
                .iter()
                .rev()
                .find_map(|sample| sample.failure_code),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowSummary {
    sample_count: usize,
    failure_count: usize,
    failure_rate_percent: u8,
    last_failure_code: Option<HealthProtectionFailureCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HealthProtectionEntrySnapshotV1 {
    scope: HealthProtectionScope,
    state: HealthProtectionState,
    state_revision: u64,
    opened_at_ms: Option<i64>,
    cooldown_until_ms: Option<i64>,
    half_open_successes: u8,
    half_open_probe_in_flight: bool,
    open_count: u32,
    updated_at_ms: i64,
    window: HealthObservationWindowV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthProtectionSnapshotV1 {
    pub(crate) version: String,
    pub(crate) persistence_kind: HealthProtectionPersistenceKind,
    pub(crate) generated_at_ms: i64,
    entries: Vec<HealthProtectionEntrySnapshotV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HealthProtectionStatus {
    pub(crate) version: String,
    pub(crate) scope: HealthProtectionScope,
    pub(crate) state: HealthProtectionState,
    pub(crate) persistence_kind: HealthProtectionPersistenceKind,
    pub(crate) state_revision: u64,
    pub(crate) opened_at_ms: Option<i64>,
    pub(crate) cooldown_until_ms: Option<i64>,
    pub(crate) cooldown_remaining_ms: Option<i64>,
    /// True only after an explicit durable probe lease has been reserved.
    /// Half-Open without this fence remains suppressed by admission.
    pub(crate) half_open_probe_in_flight: bool,
    pub(crate) recent_failure_code: Option<HealthProtectionFailureCode>,
    pub(crate) sample_count: usize,
    pub(crate) failure_rate_percent: u8,
    pub(crate) updated_at_ms: i64,
    pub(crate) detail_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HealthProtectionProbe {
    pub(crate) scope: HealthProtectionScope,
    pub(crate) state_revision: u64,
}

/// Controls how the planner treats a durable scope that is eligible for a
/// Half-Open probe. The first pass may retain an expired Open scope so the
/// execution owner can atomically reserve a lease. If that reservation races
/// with another request, the follow-up pass must be strict and suppress the
/// stale discovery candidate instead of reusing the old snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HealthProbeAdmissionMode {
    Normal,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-health-lease-race-mode; owner=application/health_protection; remove_when=v3 station-key circuit owns lease-race admission"
        )
    )]
    StrictAfterLeaseRace,
}

#[derive(Debug)]
pub(crate) struct HealthProtectionReducer {
    profile: HealthProtectionProfileV1,
    persistence_kind: HealthProtectionPersistenceKind,
    entries: BTreeMap<HealthProtectionScope, HealthProtectionEntrySnapshotV1>,
}

impl HealthProtectionReducer {
    pub(crate) fn new(
        profile: HealthProtectionProfileV1,
        persistence_kind: HealthProtectionPersistenceKind,
    ) -> Result<Self, HealthProtectionError> {
        profile.validate()?;
        Ok(Self {
            profile,
            persistence_kind,
            entries: BTreeMap::new(),
        })
    }

    pub(crate) fn profile(&self) -> &HealthProtectionProfileV1 {
        &self.profile
    }

    /// Apply a validated profile revision without discarding observed state.
    /// Shrinking a window only trims evidence; cooldown and probe fences are
    /// retained, so changing settings cannot accidentally reopen a scope.
    pub(crate) fn reconfigure(
        &mut self,
        profile: HealthProtectionProfileV1,
    ) -> Result<(), HealthProtectionError> {
        profile.validate()?;
        self.profile = profile;
        for entry in self.entries.values_mut() {
            entry.window.trim(&self.profile, entry.updated_at_ms.max(0));
        }
        Ok(())
    }

    pub(crate) const fn persistence_kind(&self) -> HealthProtectionPersistenceKind {
        self.persistence_kind
    }

    /// Applies an already classified durable verdict without creating a
    /// second classifier. Scoped health persistence is the source of truth
    /// for these explicit decisions; this adapter only keeps the richer
    /// Closed/Open/Half-Open state and bounded window in sync.
    pub(crate) fn apply_durable_verdict(
        &mut self,
        observation: HealthProtectionObservation,
        opens_protection: bool,
    ) -> Result<HealthProtectionTransition, HealthProtectionError> {
        observation.validate()?;
        if observation.probe {
            return Err(HealthProtectionError::InvalidObservation);
        }
        let scope = observation.scope.clone();
        self.evict_entries(observation.observed_at_ms);
        let profile = self.profile.clone();
        if !self.entries.contains_key(&scope) && self.entries.len() >= profile.max_entries {
            self.evict_oldest_closed();
            if self.entries.len() >= profile.max_entries {
                return Err(HealthProtectionError::EntryLimit);
            }
        }
        let entry =
            self.entries
                .entry(scope.clone())
                .or_insert_with(|| HealthProtectionEntrySnapshotV1 {
                    scope: scope.clone(),
                    state: HealthProtectionState::Closed,
                    state_revision: 1,
                    opened_at_ms: None,
                    cooldown_until_ms: None,
                    half_open_successes: 0,
                    half_open_probe_in_flight: false,
                    open_count: 0,
                    updated_at_ms: observation.observed_at_ms,
                    window: HealthObservationWindowV1::new(),
                });
        if entry.window.seen_ids.iter().any(|id| id == &observation.id) {
            return Ok(HealthProtectionTransition::IgnoredDuplicate);
        }
        if entry.state == HealthProtectionState::HalfOpen {
            // A durable verdict is authoritative and therefore cancels a
            // pending probe rather than racing it with a second transition.
            entry.half_open_probe_in_flight = false;
        }
        if !opens_protection {
            entry.window.record(&observation, &profile);
            entry.state = HealthProtectionState::Closed;
            entry.state_revision = entry.state_revision.saturating_add(1).max(1);
            entry.opened_at_ms = None;
            entry.cooldown_until_ms = None;
            entry.half_open_successes = 0;
            entry.open_count = 0;
            entry.updated_at_ms = observation.observed_at_ms;
            return Ok(HealthProtectionTransition::Closed);
        }

        entry.window.record(&observation, &profile);
        Self::open_entry(
            &profile,
            entry,
            observation.observed_at_ms,
            observation.retry_after_ms,
        );
        Ok(HealthProtectionTransition::Opened)
    }

    /// Applies an explicit durable recovery. A recovery observation is a
    /// typed verdict removal, so it is allowed to close an Open state without
    /// pretending that an untracked request was a Half-Open probe.
    pub(crate) fn apply_durable_recovery(
        &mut self,
        scope: HealthProtectionScope,
        observation_id: String,
        observed_at_ms: i64,
    ) -> Result<HealthProtectionTransition, HealthProtectionError> {
        if observation_id.is_empty()
            || observation_id.len() > MAX_OBSERVATION_ID_BYTES
            || observation_id.chars().any(char::is_control)
            || observed_at_ms < 0
        {
            return Err(HealthProtectionError::InvalidObservation);
        }
        scope.validate()?;
        let profile = self.profile.clone();
        let Some(entry) = self.entries.get_mut(&scope) else {
            return Ok(HealthProtectionTransition::Observed);
        };
        if entry.window.seen_ids.iter().any(|id| id == &observation_id) {
            return Ok(HealthProtectionTransition::IgnoredDuplicate);
        }
        let recovery = HealthProtectionObservation {
            id: observation_id,
            scope,
            observed_at_ms,
            outcome: HealthProtectionObservationOutcome::Success,
            probe: false,
            probe_state_revision: None,
            retry_after_ms: None,
        };
        entry.window.record(&recovery, &profile);
        let was_open = entry.state != HealthProtectionState::Closed;
        entry.state = HealthProtectionState::Closed;
        entry.state_revision = entry.state_revision.saturating_add(1).max(1);
        entry.opened_at_ms = None;
        entry.cooldown_until_ms = None;
        entry.half_open_probe_in_flight = false;
        entry.half_open_successes = 0;
        entry.open_count = 0;
        entry.updated_at_ms = observed_at_ms;
        Ok(if was_open {
            HealthProtectionTransition::Closed
        } else {
            HealthProtectionTransition::Observed
        })
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-health-observation-reference; owner=application/health_protection; remove_when=legacy scoped health reducer is deleted"
        )
    )]
    pub(crate) fn observe(
        &mut self,
        observation: HealthProtectionObservation,
    ) -> Result<HealthProtectionTransition, HealthProtectionError> {
        observation.validate()?;
        self.evict_entries(observation.observed_at_ms);
        let scope = observation.scope.clone();
        if scope.kind == HealthProtectionScopeKind::CapacityDomain
            && self.persistence_kind == HealthProtectionPersistenceKind::Durable
        {
            return Err(HealthProtectionError::InvalidScope);
        }
        let profile = self.profile.clone();
        if !self.entries.contains_key(&scope) && self.entries.len() >= profile.max_entries {
            self.evict_oldest_closed();
            if self.entries.len() >= profile.max_entries {
                return Err(HealthProtectionError::EntryLimit);
            }
        }
        let entry =
            self.entries
                .entry(scope.clone())
                .or_insert_with(|| HealthProtectionEntrySnapshotV1 {
                    scope: scope.clone(),
                    state: HealthProtectionState::Closed,
                    state_revision: 1,
                    opened_at_ms: None,
                    cooldown_until_ms: None,
                    half_open_successes: 0,
                    half_open_probe_in_flight: false,
                    open_count: 0,
                    updated_at_ms: observation.observed_at_ms,
                    window: HealthObservationWindowV1::new(),
                });

        if entry.window.seen_ids.iter().any(|id| id == &observation.id) {
            return Ok(HealthProtectionTransition::IgnoredDuplicate);
        }
        if observation.probe {
            if entry.state != HealthProtectionState::HalfOpen
                || !entry.half_open_probe_in_flight
                || entry.state_revision != observation.probe_state_revision.expect("validated")
            {
                return Err(HealthProtectionError::StaleProbe);
            }
        }
        if !observation.probe && observation.observed_at_ms < entry.updated_at_ms {
            return Err(HealthProtectionError::StaleObservation);
        }

        if entry.state == HealthProtectionState::Open {
            return Err(HealthProtectionError::ProbeNotAllowed);
        }

        if entry.state == HealthProtectionState::HalfOpen && !observation.probe {
            return Err(HealthProtectionError::ProbeNotAllowed);
        }
        if entry.state == HealthProtectionState::HalfOpen && !entry.half_open_probe_in_flight {
            return Err(HealthProtectionError::ProbeNotAllowed);
        }
        if observation.probe && entry.state != HealthProtectionState::HalfOpen {
            return Err(HealthProtectionError::ProbeNotAllowed);
        }

        if !entry.window.record(&observation, &profile) {
            return Ok(HealthProtectionTransition::IgnoredDuplicate);
        }
        entry.updated_at_ms = observation.observed_at_ms;

        if entry.state == HealthProtectionState::HalfOpen {
            entry.half_open_probe_in_flight = false;
            match observation.outcome {
                HealthProtectionObservationOutcome::Success => {
                    entry.half_open_successes = entry.half_open_successes.saturating_add(1);
                    if entry.half_open_successes >= profile.half_open_successes_to_close {
                        entry.state = HealthProtectionState::Closed;
                        entry.state_revision = entry.state_revision.saturating_add(1).max(1);
                        entry.opened_at_ms = None;
                        entry.cooldown_until_ms = None;
                        entry.open_count = 0;
                        entry.half_open_successes = 0;
                        entry.window = HealthObservationWindowV1::new();
                        return Ok(HealthProtectionTransition::Closed);
                    }
                    return Ok(HealthProtectionTransition::ProbeSucceeded);
                }
                HealthProtectionObservationOutcome::Failure(code) => {
                    Self::open_entry(
                        &profile,
                        entry,
                        observation.observed_at_ms,
                        observation.retry_after_ms,
                    );
                    let _ = code;
                    return Ok(HealthProtectionTransition::Reopened);
                }
            }
        }

        let summary = entry.window.summary(&profile, observation.observed_at_ms);
        if entry.state == HealthProtectionState::Closed
            && summary.sample_count >= profile.min_samples
            && summary.failure_rate_percent >= profile.failure_threshold_percent
        {
            Self::open_entry(
                &profile,
                entry,
                observation.observed_at_ms,
                observation.retry_after_ms,
            );
            return Ok(HealthProtectionTransition::Opened);
        }
        Ok(HealthProtectionTransition::Observed)
    }

    /// Reserves the only probe allowed for a scope in Half-Open. The caller
    /// must submit a `probe: true` observation carrying the returned token.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=legacy-health-probe; owner=application/health_protection; remove_when=v3 station-key circuit owns durable Half-Open leases"
        )
    )]
    pub(crate) fn begin_probe(
        &mut self,
        scope: &HealthProtectionScope,
        now_ms: i64,
    ) -> Result<HealthProtectionProbe, HealthProtectionError> {
        if now_ms < 0 {
            return Err(HealthProtectionError::InvalidObservation);
        }
        let entry = self
            .entries
            .get_mut(scope)
            .ok_or(HealthProtectionError::ScopeNotFound)?;
        if entry.half_open_probe_in_flight {
            return Err(HealthProtectionError::ProbeAlreadyInFlight);
        }
        let eligible = match entry.state {
            HealthProtectionState::Open => {
                entry.cooldown_until_ms.is_some_and(|until| until <= now_ms)
            }
            // Half-Open may require multiple successful probes, but only one
            // can be in flight at a time.
            HealthProtectionState::HalfOpen => !entry.half_open_probe_in_flight,
            HealthProtectionState::Closed => false,
        };
        if !eligible {
            return Err(HealthProtectionError::ProbeNotAllowed);
        }
        if entry.state == HealthProtectionState::Open {
            entry.state = HealthProtectionState::HalfOpen;
            entry.state_revision = entry.state_revision.saturating_add(1).max(1);
            entry.half_open_successes = 0;
        } else {
            // A multi-probe recovery keeps its successful probe count while
            // issuing a fresh fence for the next single in-flight probe.
            entry.state_revision = entry.state_revision.saturating_add(1).max(1);
        }
        entry.half_open_probe_in_flight = true;
        entry.updated_at_ms = now_ms;
        Ok(HealthProtectionProbe {
            scope: scope.clone(),
            state_revision: entry.state_revision,
        })
    }

    /// Cancels a reserved probe without recording a health sample. This is
    /// used when request admission or target resolution stops before the
    /// outbound attempt boundary. The revision fence makes cancellation
    /// idempotent and prevents an old request from clearing a newer probe.
    pub(crate) fn cancel_probe(
        &mut self,
        probe: HealthProtectionProbe,
        now_ms: i64,
    ) -> Result<bool, HealthProtectionError> {
        if now_ms < 0 {
            return Err(HealthProtectionError::InvalidObservation);
        }
        let Some(entry) = self.entries.get_mut(&probe.scope) else {
            return Ok(false);
        };
        if entry.state != HealthProtectionState::HalfOpen
            || !entry.half_open_probe_in_flight
            || entry.state_revision != probe.state_revision
        {
            return Ok(false);
        }
        entry.half_open_probe_in_flight = false;
        entry.state_revision = entry.state_revision.saturating_add(1).max(1);
        entry.updated_at_ms = now_ms;
        Ok(true)
    }

    #[cfg(test)]
    pub(crate) fn probe_is_current(&self, probe: HealthProtectionProbe) -> bool {
        self.entries.get(&probe.scope).is_some_and(|entry| {
            entry.state == HealthProtectionState::HalfOpen
                && entry.half_open_probe_in_flight
                && entry.state_revision == probe.state_revision
        })
    }

    #[cfg(test)]
    pub(crate) fn status(
        &mut self,
        scope: &HealthProtectionScope,
        now_ms: i64,
    ) -> Result<Option<HealthProtectionStatus>, HealthProtectionError> {
        if now_ms < 0 {
            return Err(HealthProtectionError::InvalidObservation);
        }
        let Some(entry) = self.entries.get_mut(scope) else {
            return Ok(None);
        };
        let profile = self.profile.clone();
        let summary = entry.window.summary(&profile, now_ms);
        Ok(Some(status_from_entry(
            entry,
            self.persistence_kind,
            summary,
            now_ms,
        )))
    }

    pub(crate) fn statuses(
        &mut self,
        now_ms: i64,
    ) -> Result<Vec<HealthProtectionStatus>, HealthProtectionError> {
        if now_ms < 0 {
            return Err(HealthProtectionError::InvalidObservation);
        }
        self.evict_entries(now_ms);
        let profile = self.profile.clone();
        let mut statuses = Vec::with_capacity(self.entries.len());
        for entry in self.entries.values_mut() {
            let summary = entry.window.summary(&profile, now_ms);
            statuses.push(status_from_entry(
                entry,
                self.persistence_kind,
                summary,
                now_ms,
            ));
        }
        Ok(statuses)
    }

    pub(crate) fn snapshot(&self, generated_at_ms: i64) -> HealthProtectionSnapshotV1 {
        HealthProtectionSnapshotV1 {
            version: HEALTH_PROTECTION_VERSION.to_string(),
            persistence_kind: self.persistence_kind,
            generated_at_ms: generated_at_ms.max(0),
            entries: self.entries.values().cloned().collect(),
        }
    }

    pub(crate) fn restore(
        profile: HealthProtectionProfileV1,
        snapshot: HealthProtectionSnapshotV1,
    ) -> Result<Self, HealthProtectionError> {
        profile.validate()?;
        if snapshot.version != HEALTH_PROTECTION_VERSION
            || snapshot.generated_at_ms < 0
            || snapshot.entries.len() > profile.max_entries
        {
            return Err(HealthProtectionError::SnapshotInvalid);
        }
        let mut entries = BTreeMap::new();
        for entry in snapshot.entries {
            validate_entry_snapshot(&profile, &entry)?;
            if snapshot.persistence_kind == HealthProtectionPersistenceKind::Durable
                && entry.scope.kind == HealthProtectionScopeKind::CapacityDomain
            {
                return Err(HealthProtectionError::SnapshotInvalid);
            }
            if entries.insert(entry.scope.clone(), entry).is_some() {
                return Err(HealthProtectionError::SnapshotInvalid);
            }
        }
        Ok(Self {
            profile,
            persistence_kind: snapshot.persistence_kind,
            entries,
        })
    }

    fn open_entry(
        profile: &HealthProtectionProfileV1,
        entry: &mut HealthProtectionEntrySnapshotV1,
        now_ms: i64,
        retry_after_ms: Option<i64>,
    ) {
        entry.state = HealthProtectionState::Open;
        entry.state_revision = entry.state_revision.saturating_add(1).max(1);
        entry.opened_at_ms = Some(now_ms);
        entry.open_count = entry.open_count.saturating_add(1);
        let cooldown = retry_after_ms
            .filter(|value| *value > 0)
            .unwrap_or_else(|| profile.cooldown_for_open_count(entry.open_count))
            .clamp(1, profile.max_cooldown_ms);
        entry.cooldown_until_ms = Some(now_ms.saturating_add(cooldown));
        entry.half_open_successes = 0;
        entry.half_open_probe_in_flight = false;
        entry.updated_at_ms = now_ms;
    }

    fn evict_entries(&mut self, _now_ms: i64) {
        if self.entries.len() <= self.profile.max_entries {
            return;
        }
        while self.entries.len() > self.profile.max_entries {
            if !self.evict_oldest_closed() {
                break;
            }
        }
    }

    fn evict_oldest_closed(&mut self) -> bool {
        let mut candidates = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.state == HealthProtectionState::Closed)
            .map(|(scope, entry)| (entry.updated_at_ms, scope.clone()))
            .collect::<Vec<_>>();
        candidates.sort();
        let Some((_, scope)) = candidates.first() else {
            return false;
        };
        self.entries.remove(scope);
        true
    }
}

fn status_from_entry(
    entry: &mut HealthProtectionEntrySnapshotV1,
    persistence_kind: HealthProtectionPersistenceKind,
    summary: WindowSummary,
    now_ms: i64,
) -> HealthProtectionStatus {
    HealthProtectionStatus {
        version: HEALTH_PROTECTION_VERSION.to_string(),
        scope: entry.scope.clone(),
        state: entry.state,
        persistence_kind,
        state_revision: entry.state_revision,
        opened_at_ms: entry.opened_at_ms,
        cooldown_until_ms: entry.cooldown_until_ms,
        cooldown_remaining_ms: entry
            .cooldown_until_ms
            .map(|until| until.saturating_sub(now_ms).max(0)),
        half_open_probe_in_flight: entry.half_open_probe_in_flight,
        recent_failure_code: summary.last_failure_code,
        sample_count: summary.sample_count,
        failure_rate_percent: summary.failure_rate_percent,
        updated_at_ms: entry.updated_at_ms,
        detail_available: true,
    }
}

fn validate_entry_snapshot(
    profile: &HealthProtectionProfileV1,
    entry: &HealthProtectionEntrySnapshotV1,
) -> Result<(), HealthProtectionError> {
    entry.scope.validate()?;
    if entry.scope.commitment.is_empty()
        || entry.scope.commitment.len() > MAX_SCOPE_BYTES
        || entry.state_revision == 0
        || entry.updated_at_ms < 0
        || entry.opened_at_ms.is_some_and(|value| value < 0)
        || entry.cooldown_until_ms.is_some_and(|value| value < 0)
        || entry.window.samples.len() > profile.window_max_samples
    {
        return Err(HealthProtectionError::SnapshotInvalid);
    }
    if entry.state == HealthProtectionState::Closed
        && (entry.opened_at_ms.is_some()
            || entry.cooldown_until_ms.is_some()
            || entry.half_open_probe_in_flight
            || entry.half_open_successes != 0)
    {
        return Err(HealthProtectionError::SnapshotInvalid);
    }
    if entry.state == HealthProtectionState::Open
        && (entry.opened_at_ms.is_none()
            || entry.cooldown_until_ms.is_none()
            || entry.half_open_probe_in_flight
            || entry.half_open_successes != 0)
    {
        return Err(HealthProtectionError::SnapshotInvalid);
    }
    if entry.state == HealthProtectionState::HalfOpen
        && (entry.opened_at_ms.is_none()
            || entry.cooldown_until_ms.is_none()
            || entry.half_open_successes > profile.half_open_successes_to_close)
    {
        return Err(HealthProtectionError::SnapshotInvalid);
    }
    if entry.window.seen_ids.len() > profile.window_max_samples.saturating_mul(4).max(16) {
        return Err(HealthProtectionError::SnapshotInvalid);
    }
    if entry.window.seen_ids.iter().any(|id| {
        id.is_empty() || id.len() > MAX_OBSERVATION_ID_BYTES || id.chars().any(char::is_control)
    }) || entry
        .window
        .seen_ids
        .iter()
        .zip(entry.window.seen_ids.iter().skip(1))
        .any(|(left, right)| left == right)
        || entry.window.samples.iter().any(|sample| {
            sample.observed_at_ms < 0 || sample.failed != sample.failure_code.is_some()
        })
    {
        return Err(HealthProtectionError::SnapshotInvalid);
    }
    Ok(())
}

/// Convert untrusted failure text to a stable, low-cardinality reason. The
/// reducer intentionally accepts only this closed mapping at the adapter edge.
pub(crate) fn failure_code_from_label(label: &str) -> HealthProtectionFailureCode {
    match label.trim().to_ascii_lowercase().as_str() {
        "connect_failure" | "connection_failure" | "connect_timeout" => {
            HealthProtectionFailureCode::ConnectFailure
        }
        "first_byte_timeout" | "ttft_timeout" => HealthProtectionFailureCode::FirstByteTimeout,
        "upstream_5xx" | "server_error" | "http_5xx" => HealthProtectionFailureCode::Upstream5xx,
        "rate_limited" | "rate_limit" | "http_429" => HealthProtectionFailureCode::RateLimited,
        "capacity_exhausted" | "capacity" => HealthProtectionFailureCode::CapacityExhausted,
        "endpoint_unavailable" | "endpoint_failure" => {
            HealthProtectionFailureCode::EndpointUnavailable
        }
        _ => HealthProtectionFailureCode::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> HealthProtectionScope {
        HealthProtectionScope::new(HealthProtectionScopeKind::Endpoint, "endpoint:v1:test")
            .expect("scope")
    }

    fn observation(
        id: &str,
        at_ms: i64,
        outcome: HealthProtectionObservationOutcome,
    ) -> HealthProtectionObservation {
        HealthProtectionObservation {
            id: id.to_string(),
            scope: scope(),
            observed_at_ms: at_ms,
            outcome,
            probe: false,
            probe_state_revision: None,
            retry_after_ms: None,
        }
    }

    fn fast_profile() -> HealthProtectionProfileV1 {
        HealthProtectionProfileV1 {
            version: HEALTH_PROTECTION_VERSION.to_string(),
            preset: HealthProtectionPreset::Balanced,
            enabled: true,
            window_max_samples: 4,
            window_ms: 1_000,
            min_samples: 2,
            failure_threshold_percent: 50,
            base_cooldown_ms: 100,
            max_cooldown_ms: 500,
            half_open_successes_to_close: 2,
            max_entries: 4,
        }
    }

    #[test]
    fn profile_rejects_unbounded_values() {
        let mut profile = fast_profile();
        profile.window_max_samples = MAX_PROFILE_SAMPLES + 1;
        assert_eq!(
            profile.validate(),
            Err(HealthProtectionError::InvalidProfile)
        );

        let mut profile = fast_profile();
        profile.failure_threshold_percent = 0;
        assert_eq!(
            profile.validate(),
            Err(HealthProtectionError::InvalidProfile)
        );
    }

    #[test]
    fn threshold_opens_and_cooldown_enters_single_half_open_probe() {
        let mut reducer =
            HealthProtectionReducer::new(fast_profile(), HealthProtectionPersistenceKind::Durable)
                .expect("reducer");
        assert_eq!(
            reducer
                .observe(observation(
                    "one",
                    0,
                    HealthProtectionObservationOutcome::Failure(
                        HealthProtectionFailureCode::Upstream5xx,
                    ),
                ))
                .expect("first observation"),
            HealthProtectionTransition::Observed
        );
        assert_eq!(
            reducer
                .observe(observation(
                    "two",
                    1,
                    HealthProtectionObservationOutcome::Failure(
                        HealthProtectionFailureCode::Upstream5xx,
                    ),
                ))
                .expect("second observation"),
            HealthProtectionTransition::Opened
        );
        let status = reducer
            .status(&scope(), 50)
            .expect("status")
            .expect("entry");
        assert_eq!(status.state, HealthProtectionState::Open);
        assert_eq!(status.cooldown_remaining_ms, Some(51));
        let probe = reducer.begin_probe(&scope(), 101).expect("probe");
        assert!(reducer.probe_is_current(probe));
        assert_eq!(
            reducer.begin_probe(&scope(), 101),
            Err(HealthProtectionError::ProbeAlreadyInFlight)
        );
    }

    #[test]
    fn cancelling_probe_releases_in_flight_and_invalidates_late_result() {
        let mut reducer =
            HealthProtectionReducer::new(fast_profile(), HealthProtectionPersistenceKind::Durable)
                .expect("reducer");
        for (id, at_ms) in [("one", 0), ("two", 1)] {
            reducer
                .observe(observation(
                    id,
                    at_ms,
                    HealthProtectionObservationOutcome::Failure(
                        HealthProtectionFailureCode::Upstream5xx,
                    ),
                ))
                .expect("open");
        }
        let probe = reducer.begin_probe(&scope(), 101).expect("probe");
        assert!(reducer.cancel_probe(probe.clone(), 102).expect("cancel"));
        assert!(!reducer.probe_is_current(probe.clone()));
        assert!(!reducer.cancel_probe(probe, 103).expect("duplicate cancel"));
        let status = reducer
            .status(&scope(), 103)
            .expect("status")
            .expect("entry");
        assert_eq!(status.state, HealthProtectionState::HalfOpen);
        assert!(!status.half_open_probe_in_flight);
    }

    #[test]
    fn reconfigure_trims_window_without_resetting_open_or_probe_state() {
        let mut reducer =
            HealthProtectionReducer::new(fast_profile(), HealthProtectionPersistenceKind::Durable)
                .expect("reducer");
        for (id, at_ms) in [("one", 0), ("two", 1)] {
            reducer
                .observe(observation(
                    id,
                    at_ms,
                    HealthProtectionObservationOutcome::Failure(
                        HealthProtectionFailureCode::Upstream5xx,
                    ),
                ))
                .expect("open protection");
        }
        let before = reducer
            .status(&scope(), 50)
            .expect("status")
            .expect("entry");
        assert_eq!(before.state, HealthProtectionState::Open);
        let probe = reducer.begin_probe(&scope(), 101).expect("probe");

        let mut updated = fast_profile();
        updated.window_max_samples = 2;
        updated.window_ms = 500;
        reducer.reconfigure(updated.clone()).expect("reconfigure");

        assert_eq!(reducer.profile(), &updated);
        let after = reducer
            .status(&scope(), 101)
            .expect("status")
            .expect("entry");
        assert_eq!(after.state, HealthProtectionState::HalfOpen);
        assert!(after.half_open_probe_in_flight);
        assert_eq!(after.cooldown_until_ms, before.cooldown_until_ms);
        assert!(reducer.probe_is_current(probe));
    }

    #[test]
    fn probe_successes_close_and_failure_reopens_with_retry_after() {
        let mut reducer = HealthProtectionReducer::new(
            fast_profile(),
            HealthProtectionPersistenceKind::RuntimeOutlier,
        )
        .expect("reducer");
        for (id, at_ms) in [("one", 0), ("two", 1)] {
            reducer
                .observe(observation(
                    id,
                    at_ms,
                    HealthProtectionObservationOutcome::Failure(
                        HealthProtectionFailureCode::ConnectFailure,
                    ),
                ))
                .expect("open");
        }
        let probe = reducer.begin_probe(&scope(), 101).expect("probe");
        let mut failed_probe = observation(
            "probe-failure",
            101,
            HealthProtectionObservationOutcome::Failure(HealthProtectionFailureCode::RateLimited),
        );
        failed_probe.probe = true;
        failed_probe.probe_state_revision = Some(probe.state_revision);
        failed_probe.retry_after_ms = Some(250);
        assert_eq!(
            reducer.observe(failed_probe).expect("reopen"),
            HealthProtectionTransition::Reopened
        );
        let status = reducer
            .status(&scope(), 101)
            .expect("status")
            .expect("entry");
        assert_eq!(status.state, HealthProtectionState::Open);
        assert_eq!(status.cooldown_until_ms, Some(351));

        let probe = reducer.begin_probe(&scope(), 351).expect("second probe");
        let mut success = observation(
            "probe-success-1",
            351,
            HealthProtectionObservationOutcome::Success,
        );
        success.probe = true;
        success.probe_state_revision = Some(probe.state_revision);
        assert_eq!(
            reducer.observe(success).expect("first success"),
            HealthProtectionTransition::ProbeSucceeded
        );
        let probe = reducer.begin_probe(&scope(), 352).expect("follow-up probe");
        let mut success = observation(
            "probe-success-2",
            352,
            HealthProtectionObservationOutcome::Success,
        );
        success.probe = true;
        success.probe_state_revision = Some(probe.state_revision);
        assert_eq!(
            reducer.observe(success).expect("second success"),
            HealthProtectionTransition::Closed
        );
        let status = reducer
            .status(&scope(), 352)
            .expect("status")
            .expect("entry");
        assert_eq!(status.state, HealthProtectionState::Closed);
    }

    #[test]
    fn duplicate_observation_is_idempotent_and_window_is_bounded() {
        let mut reducer =
            HealthProtectionReducer::new(fast_profile(), HealthProtectionPersistenceKind::Durable)
                .expect("reducer");
        let first = observation("same", 0, HealthProtectionObservationOutcome::Success);
        assert_eq!(
            reducer.observe(first.clone()).expect("first"),
            HealthProtectionTransition::Observed
        );
        assert_eq!(
            reducer.observe(first).expect("duplicate"),
            HealthProtectionTransition::IgnoredDuplicate
        );
        for index in 0..12 {
            reducer
                .observe(observation(
                    &format!("id-{index}"),
                    index + 2,
                    HealthProtectionObservationOutcome::Success,
                ))
                .expect("sample");
        }
        let status = reducer
            .status(&scope(), 13)
            .expect("status")
            .expect("entry");
        assert!(status.sample_count <= 4);
    }

    #[test]
    fn snapshot_round_trip_preserves_state_and_rejects_bad_versions() {
        let profile = fast_profile();
        let mut reducer =
            HealthProtectionReducer::new(profile.clone(), HealthProtectionPersistenceKind::Durable)
                .expect("reducer");
        reducer
            .observe(observation(
                "one",
                1,
                HealthProtectionObservationOutcome::Success,
            ))
            .expect("observation");
        let snapshot = reducer.snapshot(10);
        let encoded = serde_json::to_vec(&snapshot).expect("encode");
        let decoded: HealthProtectionSnapshotV1 = serde_json::from_slice(&encoded).expect("decode");
        let mut restored = HealthProtectionReducer::restore(profile, decoded).expect("restore");
        assert_eq!(
            restored.status(&scope(), 10).expect("status"),
            reducer.status(&scope(), 10).expect("status")
        );

        let mut bad = snapshot;
        bad.version = "health_protection_v99".to_string();
        assert!(matches!(
            HealthProtectionReducer::restore(HealthProtectionProfileV1::default(), bad),
            Err(HealthProtectionError::SnapshotInvalid)
        ));
    }

    #[test]
    fn untrusted_scope_and_failure_labels_are_low_cardinality() {
        let scope = HealthProtectionScope::from_untrusted(
            HealthProtectionScopeKind::Endpoint,
            "https://provider.example/v1/key=secret",
        );
        assert_eq!(scope.commitment.len(), 64);
        assert!(!scope.commitment.contains("secret"));
        assert_eq!(
            failure_code_from_label("HTTP_429"),
            HealthProtectionFailureCode::RateLimited
        );
        assert_eq!(
            failure_code_from_label("raw response: api-key=secret"),
            HealthProtectionFailureCode::Unknown
        );
        assert_eq!(
            HealthProtectionFailureCode::RateLimited.as_str(),
            "rate_limited"
        );
    }

    #[test]
    fn late_probe_result_cannot_close_a_newer_cycle() {
        let mut reducer = HealthProtectionReducer::new(
            fast_profile(),
            HealthProtectionPersistenceKind::RuntimeOutlier,
        )
        .expect("reducer");
        for (id, at_ms) in [("one", 0), ("two", 1)] {
            reducer
                .observe(observation(
                    id,
                    at_ms,
                    HealthProtectionObservationOutcome::Failure(
                        HealthProtectionFailureCode::EndpointUnavailable,
                    ),
                ))
                .expect("open");
        }
        let first_probe = reducer.begin_probe(&scope(), 101).expect("first probe");
        let mut first_result =
            observation("late", 101, HealthProtectionObservationOutcome::Success);
        first_result.probe = true;
        first_result.probe_state_revision = Some(first_probe.state_revision + 1);
        assert_eq!(
            reducer.observe(first_result),
            Err(HealthProtectionError::StaleProbe)
        );
        assert!(reducer.probe_is_current(first_probe));
    }

    #[test]
    fn cancelled_probe_releases_half_open_slot_and_fences_late_result() {
        let mut reducer =
            HealthProtectionReducer::new(fast_profile(), HealthProtectionPersistenceKind::Durable)
                .expect("reducer");
        for (id, at_ms) in [("one", 0), ("two", 1)] {
            reducer
                .observe(observation(
                    id,
                    at_ms,
                    HealthProtectionObservationOutcome::Failure(
                        HealthProtectionFailureCode::EndpointUnavailable,
                    ),
                ))
                .expect("open");
        }
        let probe = reducer.begin_probe(&scope(), 101).expect("probe");
        assert!(reducer.cancel_probe(probe.clone(), 102).expect("cancel"));
        assert!(!reducer.probe_is_current(probe.clone()));
        let status = reducer
            .status(&scope(), 102)
            .expect("status")
            .expect("entry");
        assert_eq!(status.state, HealthProtectionState::HalfOpen);
        assert!(!status.half_open_probe_in_flight);
        assert!(!reducer
            .cancel_probe(probe.clone(), 103)
            .expect("idempotent cancel"));

        let mut late = observation(
            "late-cancelled",
            103,
            HealthProtectionObservationOutcome::Success,
        );
        late.probe = true;
        late.probe_state_revision = Some(probe.state_revision);
        assert_eq!(
            reducer.observe(late),
            Err(HealthProtectionError::StaleProbe)
        );
        let next = reducer.begin_probe(&scope(), 103).expect("next probe");
        assert!(next.state_revision > probe.state_revision);
    }

    #[test]
    fn durable_reducer_rejects_capacity_scope_and_keeps_entry_bound() {
        let mut reducer =
            HealthProtectionReducer::new(fast_profile(), HealthProtectionPersistenceKind::Durable)
                .expect("reducer");
        let capacity_scope = HealthProtectionScope::new(
            HealthProtectionScopeKind::CapacityDomain,
            "capacity:v1:test",
        )
        .expect("scope");
        let mut capacity_observation = observation(
            "capacity",
            0,
            HealthProtectionObservationOutcome::Failure(
                HealthProtectionFailureCode::CapacityExhausted,
            ),
        );
        capacity_observation.scope = capacity_scope;
        assert_eq!(
            reducer.observe(capacity_observation),
            Err(HealthProtectionError::InvalidScope)
        );

        for index in 0..fast_profile().max_entries {
            let unique_scope = HealthProtectionScope::new(
                HealthProtectionScopeKind::Endpoint,
                format!("endpoint:v1:{index}"),
            )
            .expect("scope");
            let mut sample = observation(
                &format!("sample-{index}"),
                index as i64,
                HealthProtectionObservationOutcome::Success,
            );
            sample.scope = unique_scope;
            reducer.observe(sample).expect("bounded sample");
        }
        let mut extra = observation(
            "sample-extra",
            100,
            HealthProtectionObservationOutcome::Success,
        );
        extra.scope =
            HealthProtectionScope::new(HealthProtectionScopeKind::Endpoint, "endpoint:v1:extra")
                .expect("scope");
        reducer.observe(extra).expect("old closed entry is evicted");
        assert_eq!(reducer.statuses(100).expect("statuses").len(), 4);
    }
}
