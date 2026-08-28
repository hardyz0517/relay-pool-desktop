//! Bounded error-rate observation adapter for the routing health reducer.
//!
//! This module is intentionally an adapter, not a second breaker.  It accepts
//! the already validated `RoutingObservation` taxonomy, turns only a small,
//! allow-listed subset into `HealthProtectionObservation`, and keeps a safe
//! diagnostic history. The feature is disabled by default. When explicitly
//! enabled by an internal composition fixture, reducer status is consumed by
//! the planner through the typed admission bridge below. Pool ejection is
//! represented by durable Open suppression; Half-Open admission requires an
//! explicit reducer probe lease. This adapter never starts an outbound call.

use std::sync::{Arc, Mutex};

#[cfg(test)]
use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    application::health_protection::{
        durable_health_scope_commitment, failure_code_from_label,
        DurableHealthScopeCommitmentInput, HealthProtectionFailureCode,
        HealthProtectionObservation, HealthProtectionObservationOutcome, HealthProtectionProbe,
        HealthProtectionScope, HealthProtectionScopeKind, HealthProtectionState,
        HealthProtectionStatus,
    },
    models::routing_observation::{
        ObservationOutcome, ObservationSource, RoutingObservation, TrafficEquivalence,
    },
};

#[cfg(test)]
use crate::application::health_protection::HealthProtectionSnapshotV1;

pub(crate) const ERROR_RATE_PROTECTION_VERSION: &str = "error_rate_protection_v1";
pub(crate) const ERROR_RATE_HISTORY_VERSION: &str = "error_rate_history_v1";
pub(crate) const DEFAULT_ERROR_RATE_PROTECTION_ENABLED: bool = false;

const DEFAULT_HISTORY_MAX_EVENTS: usize = 512;
const MAX_HISTORY_EVENTS: usize = 4_096;
const MAX_HISTORY_RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1_000;
#[cfg(test)]
const MAX_HISTORY_SCOPE_BYTES: usize = 192;
#[cfg(test)]
const MAX_FAILURE_CODE_COUNTS: usize = 8;

/// The planner-facing switch is intentionally narrower than the history
/// configuration. It is produced by the application-owned service and cannot
/// be inferred from a request or from a persisted reducer row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ErrorRateAdmissionConfigV1 {
    pub(crate) enabled: bool,
    pub(crate) probe: Option<HealthProtectionProbe>,
}

impl ErrorRateAdmissionConfigV1 {
    pub(crate) const fn disabled() -> Self {
        Self {
            enabled: false,
            probe: None,
        }
    }

    #[cfg(test)]
    pub(crate) const fn enabled() -> Self {
        Self {
            enabled: true,
            probe: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_probe(mut self, probe: HealthProtectionProbe) -> Self {
        self.probe = Some(probe);
        self
    }
}

/// Typed result of matching one candidate scope against the durable reducer.
/// Missing/Closed scopes are admissible; Open scopes are ejected. Half-Open
/// scopes remain suppressed unless the caller supplies a matching lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorRateScopedVerdict {
    Admitted,
    Suppressed(HealthProtectionState),
}

impl ErrorRateScopedVerdict {
    pub(crate) const fn is_admitted(self) -> bool {
        matches!(self, Self::Admitted)
    }
}

/// Match a planner-owned opaque scope against reducer statuses without
/// exposing raw provider/key/model identifiers to the status surface.
/// Multiple rows for a scope are reduced conservatively: any open state wins.
#[cfg(test)]
pub(crate) fn scoped_admission_verdict(
    statuses: &[HealthProtectionStatus],
    scope: &HealthProtectionScope,
) -> ErrorRateScopedVerdict {
    scoped_admission_verdict_with_probe(statuses, scope, None)
}

pub(crate) fn scoped_admission_verdict_with_probe(
    statuses: &[HealthProtectionStatus],
    scope: &HealthProtectionScope,
    probe: Option<&HealthProtectionProbe>,
) -> ErrorRateScopedVerdict {
    statuses
        .iter()
        .filter(|status| {
            status.scope == *scope
                && status.persistence_kind
                    == crate::application::health_protection::HealthProtectionPersistenceKind::Durable
        })
        .map(|status| match status.state {
            HealthProtectionState::Closed => ErrorRateScopedVerdict::Admitted,
            HealthProtectionState::Open => ErrorRateScopedVerdict::Suppressed(status.state),
            HealthProtectionState::HalfOpen => {
                if status.half_open_probe_in_flight
                    && probe.is_some_and(|probe| {
                        probe.scope == status.scope
                            && probe.state_revision == status.state_revision
                    })
                {
                    ErrorRateScopedVerdict::Admitted
                } else {
                    ErrorRateScopedVerdict::Suppressed(status.state)
                }
            }
        })
        .find(|verdict| !verdict.is_admitted())
        .unwrap_or(ErrorRateScopedVerdict::Admitted)
}

/// Admission used by the first planning pass to discover a candidate that can
/// receive a real Half-Open probe. An expired Open entry has no lease yet, so
/// ordinary admission would suppress it before the execution coordinator had
/// a chance to reserve one. The coordinator still performs the atomic
/// revision-fenced reservation and replans with the returned fence before any
/// outbound attempt is started. Half-Open entries and Open entries with
/// remaining cooldown stay suppressed.
pub(crate) fn scoped_admission_verdict_for_probe_candidate(
    statuses: &[HealthProtectionStatus],
    scope: &HealthProtectionScope,
) -> ErrorRateScopedVerdict {
    statuses
        .iter()
        .filter(|status| {
            status.scope == *scope
                && status.persistence_kind
                    == crate::application::health_protection::HealthProtectionPersistenceKind::Durable
        })
        .map(|status| match status.state {
            HealthProtectionState::Closed => ErrorRateScopedVerdict::Admitted,
            HealthProtectionState::Open
                if status.cooldown_remaining_ms == Some(0) => ErrorRateScopedVerdict::Admitted,
            HealthProtectionState::Open => ErrorRateScopedVerdict::Suppressed(status.state),
            HealthProtectionState::HalfOpen => ErrorRateScopedVerdict::Suppressed(status.state),
        })
        .find(|verdict| !verdict.is_admitted())
        .unwrap_or(ErrorRateScopedVerdict::Admitted)
}

/// Build the same one-way commitment used by observation ingestion. Keeping
/// this constructor in the adapter prevents planner code from reproducing the
/// hashing algorithm or accidentally comparing raw secrets.
pub(crate) fn admission_scope(
    kind: HealthProtectionScopeKind,
    value: &str,
) -> HealthProtectionScope {
    HealthProtectionScope::from_untrusted(kind, value)
}

/// Build the endpoint commitment from the same canonical tuple everywhere.
/// Raw station IDs and endpoint revisions never leave this adapter.
pub(crate) fn endpoint_health_scope(
    station_id: &str,
    endpoint_revision: i64,
) -> Option<HealthProtectionScope> {
    if station_id.trim().is_empty() || endpoint_revision <= 0 {
        return None;
    }
    let commitment = durable_health_scope_commitment(DurableHealthScopeCommitmentInput {
        scope_kind: "station_endpoint",
        station_id,
        station_key_id: None,
        group_binding_id: None,
        resolved_model_commitment: None,
        credential_revision: None,
        account_revision: None,
        group_revision: None,
        endpoint_revision: Some(endpoint_revision),
        model_alias_revision: None,
    })?;
    HealthProtectionScope::new(HealthProtectionScopeKind::Endpoint, commitment).ok()
}

/// The scope used by the production error-rate probe bridge for one immutable
/// routing candidate.  The current bridge deliberately stops at Credential;
/// Endpoint remains a durable health scope for observation attribution and
/// read models, but its Half-Open resolver is a later, separately-scoped
/// capability. Keeping this resolver here makes planner and proxy execution
/// compare the exact same opaque value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateHealthScopes {
    pub(crate) credential: HealthProtectionScope,
}

impl CandidateHealthScopes {
    pub(crate) fn iter(&self) -> impl Iterator<Item = &HealthProtectionScope> {
        std::iter::once(&self.credential)
    }

    pub(crate) fn contains(&self, scope: &HealthProtectionScope) -> bool {
        self.credential == *scope
    }
}

/// Resolve the production probe scope for a candidate from one immutable
/// identity snapshot. Endpoint revisions are intentionally not resolved here;
/// endpoint-scoped durable verdicts are handled by the independent health
/// attribution/read-model path until an endpoint probe resolver is introduced.
pub(crate) fn candidate_health_scopes(
    station_id: &str,
    station_key_id: &str,
    _endpoint_revision: i64,
) -> Option<CandidateHealthScopes> {
    if station_id.trim().is_empty() || station_key_id.trim().is_empty() {
        return None;
    }
    Some(CandidateHealthScopes {
        credential: admission_scope(HealthProtectionScopeKind::Credential, station_key_id),
    })
}

/// Settings owned by this adapter.  Thresholds and cooldowns remain owned by
/// `HealthProtectionProfileV1`; this type only controls whether the adapter is
/// allowed to feed observations and how much diagnostic data it retains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ErrorRateProtectionConfigV1 {
    pub(crate) version: String,
    pub(crate) enabled: bool,
    pub(crate) history_retention_ms: i64,
    pub(crate) history_max_events: usize,
}

impl Default for ErrorRateProtectionConfigV1 {
    fn default() -> Self {
        Self {
            version: ERROR_RATE_PROTECTION_VERSION.to_string(),
            enabled: DEFAULT_ERROR_RATE_PROTECTION_ENABLED,
            history_retention_ms: 24 * 60 * 60 * 1_000,
            history_max_events: DEFAULT_HISTORY_MAX_EVENTS,
        }
    }
}

impl ErrorRateProtectionConfigV1 {
    pub(crate) fn validate(&self) -> Result<(), ErrorRateProtectionError> {
        if self.version != ERROR_RATE_PROTECTION_VERSION
            || self.history_retention_ms <= 0
            || self.history_retention_ms > MAX_HISTORY_RETENTION_MS
            || self.history_max_events == 0
            || self.history_max_events > MAX_HISTORY_EVENTS
        {
            return Err(ErrorRateProtectionError::InvalidConfig);
        }
        Ok(())
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorRateObservationDisposition {
    Disabled,
    IgnoredAdministrative,
    IgnoredAnonymous,
    IgnoredCredentialFailure,
    IgnoredModelFailure,
    IgnoredCancelled,
    Recorded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorRateProtectionError {
    InvalidConfig,
    InvalidObservation,
    #[cfg(test)]
    InvalidSnapshot,
    #[cfg(test)]
    DuplicateObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErrorRateHistoryOutcome {
    Success,
    Failure,
}

/// A diagnostic event deliberately excludes request IDs, station/key IDs,
/// URLs, model names and provider text.  The scope is a one-way commitment
/// generated at the adapter boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ErrorRateHistoryEventV1 {
    pub(crate) observed_at_ms: i64,
    pub(crate) scope_kind: HealthProtectionScopeKind,
    pub(crate) scope_commitment: String,
    pub(crate) outcome: ErrorRateHistoryOutcome,
    pub(crate) failure_code: Option<HealthProtectionFailureCode>,
    pub(crate) sample_count: usize,
    pub(crate) failure_count: usize,
    pub(crate) failure_rate_percent: u8,
    pub(crate) transition: Option<HealthProtectionTransitionCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HealthProtectionTransitionCode {
    IgnoredDuplicate,
    Observed,
    Opened,
    ProbeSucceeded,
    Closed,
    Reopened,
}

pub(crate) const fn transition_code(
    transition: crate::application::health_protection::HealthProtectionTransition,
) -> HealthProtectionTransitionCode {
    use crate::application::health_protection::HealthProtectionTransition as T;
    match transition {
        T::IgnoredDuplicate => HealthProtectionTransitionCode::IgnoredDuplicate,
        T::Observed => HealthProtectionTransitionCode::Observed,
        T::Opened => HealthProtectionTransitionCode::Opened,
        T::ProbeSucceeded => HealthProtectionTransitionCode::ProbeSucceeded,
        T::Closed => HealthProtectionTransitionCode::Closed,
        T::Reopened => HealthProtectionTransitionCode::Reopened,
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ErrorRateHistorySnapshotV1 {
    pub(crate) version: String,
    pub(crate) generated_at_ms: i64,
    pub(crate) enabled: bool,
    pub(crate) dropped_events: u64,
    pub(crate) events: Vec<ErrorRateHistoryEventV1>,
    /// Test-only compatibility snapshot. Production reducer state is persisted
    /// by the observation transaction and read through the protection status
    /// projector; this bounded adapter snapshot is not an admission input.
    pub(crate) reducer_snapshot: Option<HealthProtectionSnapshotV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ErrorRateHistoryPageV1 {
    pub(crate) version: String,
    pub(crate) enabled: bool,
    pub(crate) detail_available: bool,
    pub(crate) events: Vec<ErrorRateHistoryEventV1>,
    pub(crate) next_before_ms: Option<i64>,
    pub(crate) dropped_events: u64,
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct StoredHistoryEvent {
    id: String,
    event: ErrorRateHistoryEventV1,
}

/// Safe, bounded history and reducer input adapter. Production history and
/// reducer snapshots are committed by the observation transaction. This type
/// deliberately does not own a reducer; `HealthProtectionReducer` remains the
/// only state-transition owner, and its status is currently diagnostic-only
/// for routing admission.
#[derive(Debug)]
pub(crate) struct ErrorRateProtectionAdapter {
    config: ErrorRateProtectionConfigV1,
    #[cfg(test)]
    events: VecDeque<StoredHistoryEvent>,
    #[cfg(test)]
    seen_ids: VecDeque<String>,
    #[cfg(test)]
    dropped_events: u64,
}

/// Application-scoped port used by observation ingestion and read queries.
/// Production history is owned by `RoutingErrorRateHistoryStore`; the adapter
/// only classifies observations and builds reducer/history inputs. The bounded
/// in-memory history helpers below are test-only compatibility coverage.
#[derive(Clone, Debug)]
pub(crate) struct ErrorRateProtectionService {
    adapter: Arc<Mutex<ErrorRateProtectionAdapter>>,
}

impl Default for ErrorRateProtectionService {
    fn default() -> Self {
        Self::disabled()
    }
}

impl ErrorRateProtectionService {
    pub(crate) fn disabled() -> Self {
        Self::from_adapter(
            ErrorRateProtectionAdapter::disabled().expect("default error rate config"),
        )
    }

    pub(crate) fn from_adapter(adapter: ErrorRateProtectionAdapter) -> Self {
        Self {
            adapter: Arc::new(Mutex::new(adapter)),
        }
    }

    /// Refresh only the planner/observation enable switch from the committed
    /// routing policy. Thresholds remain owned by the durable reducer profile
    /// loaded from that same policy; no caller can inject a partial profile.
    pub(crate) fn set_enabled(&self, enabled: bool) {
        if let Ok(mut adapter) = self.adapter.lock() {
            adapter.config.enabled = enabled;
        }
    }

    pub(crate) fn config(&self) -> ErrorRateProtectionConfigV1 {
        self.adapter
            .lock()
            .map(|adapter| adapter.config().clone())
            .unwrap_or_default()
    }

    pub(crate) fn admission_config(&self) -> ErrorRateAdmissionConfigV1 {
        self.adapter
            .lock()
            .map(|adapter| ErrorRateAdmissionConfigV1 {
                enabled: adapter.config().enabled,
                probe: None,
            })
            .unwrap_or_else(|_| ErrorRateAdmissionConfigV1::disabled())
    }

    /// The routing policy owns whether error-rate protection is active.  The
    /// service keeps its bounded adapter settings (history limits and version)
    /// while this method projects the policy switch at the application
    /// boundary.  This avoids a second mutable configuration owner.
    pub(crate) fn admission_config_for_policy(&self, enabled: bool) -> ErrorRateAdmissionConfigV1 {
        let mut config = self.admission_config();
        config.enabled = enabled;
        config
    }

    pub(crate) fn config_for_policy(&self, enabled: bool) -> ErrorRateProtectionConfigV1 {
        let mut config = self.config();
        config.enabled = enabled;
        config
    }

    /// Returns the bounded, canonical reducer input for a real request. The
    /// caller must pass it to the single `HealthProtectionReducer` owner and
    /// supply a probe fence when handling synthetic probes.
    #[cfg(test)]
    pub(crate) fn health_observation(
        &self,
        observation: &RoutingObservation,
    ) -> Option<HealthProtectionObservation> {
        self.health_observation_for_policy(observation, self.config().enabled)
    }

    pub(crate) fn health_observation_for_policy(
        &self,
        observation: &RoutingObservation,
        enabled: bool,
    ) -> Option<HealthProtectionObservation> {
        let adapter = self.adapter.lock().ok()?;
        adapter
            .health_observation_with_enabled(observation, enabled)
            .ok()
            .flatten()
    }

    /// Builds a durable history event without mutating process-local state.
    /// Persistence owns the transaction and supplies the reducer transition.
    #[cfg(test)]
    pub(crate) fn history_event_seed(
        &self,
        observation: &RoutingObservation,
        transition: Option<HealthProtectionTransitionCode>,
    ) -> Option<ErrorRateHistoryEventV1> {
        self.history_event_seed_for_policy(observation, transition, self.config().enabled)
    }

    pub(crate) fn history_event_seed_for_policy(
        &self,
        observation: &RoutingObservation,
        transition: Option<HealthProtectionTransitionCode>,
        enabled: bool,
    ) -> Option<ErrorRateHistoryEventV1> {
        let adapter = self.adapter.lock().ok()?;
        adapter
            .history_event_seed_with_enabled(observation, transition, enabled)
            .ok()
            .flatten()
    }
}

impl ErrorRateProtectionAdapter {
    pub(crate) fn disabled() -> Result<Self, ErrorRateProtectionError> {
        Self::new(ErrorRateProtectionConfigV1::default())
    }

    pub(crate) fn new(
        config: ErrorRateProtectionConfigV1,
    ) -> Result<Self, ErrorRateProtectionError> {
        config.validate()?;
        Ok(Self {
            config,
            #[cfg(test)]
            events: VecDeque::new(),
            #[cfg(test)]
            seen_ids: VecDeque::new(),
            #[cfg(test)]
            dropped_events: 0,
        })
    }

    pub(crate) fn config(&self) -> &ErrorRateProtectionConfigV1 {
        &self.config
    }

    #[cfg(test)]
    pub(crate) fn observe(
        &mut self,
        observation: &RoutingObservation,
    ) -> Result<ErrorRateObservationDisposition, ErrorRateProtectionError> {
        observation
            .validate()
            .map_err(|_| ErrorRateProtectionError::InvalidObservation)?;
        if !self.config.enabled {
            return Ok(ErrorRateObservationDisposition::Disabled);
        }
        let Some(classified) = classify_observation(observation) else {
            return Ok(ignored_disposition(observation));
        };
        if self.seen_ids.iter().any(|id| id == &observation.id) {
            return Err(ErrorRateProtectionError::DuplicateObservation);
        }
        self.seen_ids.push_back(observation.id.clone());
        while self.seen_ids.len() > self.config.history_max_events.saturating_mul(2).max(16) {
            self.seen_ids.pop_front();
        }

        let scope = scope_for_observation(observation);
        self.events.push_back(StoredHistoryEvent {
            id: observation.id.clone(),
            event: ErrorRateHistoryEventV1 {
                observed_at_ms: observation.order.event_at_ms,
                scope_kind: scope.kind,
                scope_commitment: history_scope_commitment(&scope),
                outcome: if outcome_is_failure(classified.outcome) {
                    ErrorRateHistoryOutcome::Failure
                } else {
                    ErrorRateHistoryOutcome::Success
                },
                failure_code: outcome_failure_code(classified.outcome),
                sample_count: 0,
                failure_count: 0,
                failure_rate_percent: 0,
                transition: None,
            },
        });
        self.trim(observation.order.event_at_ms);
        let summary = self
            .events
            .iter()
            .filter(|event| {
                event.event.scope_kind == scope.kind
                    && event.event.scope_commitment == scope.commitment
            })
            .fold((0_usize, 0_usize), |(samples, failures), event| {
                (
                    samples.saturating_add(1),
                    failures.saturating_add(matches!(
                        event.event.outcome,
                        ErrorRateHistoryOutcome::Failure
                    ) as usize),
                )
            });
        if let Some(last) = self.events.back_mut() {
            // The just-recorded event is always retained unless the configured
            // history budget is zero (which validation rejects).
            if last.id == observation.id {
                last.event.sample_count = summary.0;
                last.event.failure_count = summary.1;
                last.event.failure_rate_percent = if summary.0 == 0 {
                    0
                } else {
                    ((summary.1 * 100) / summary.0).min(100) as u8
                };
            }
        }
        Ok(ErrorRateObservationDisposition::Recorded)
    }

    #[cfg(test)]
    pub(crate) fn health_observation(
        &self,
        observation: &RoutingObservation,
    ) -> Result<Option<HealthProtectionObservation>, ErrorRateProtectionError> {
        self.health_observation_with_enabled(observation, self.config.enabled)
    }

    pub(crate) fn health_observation_with_enabled(
        &self,
        observation: &RoutingObservation,
        enabled: bool,
    ) -> Result<Option<HealthProtectionObservation>, ErrorRateProtectionError> {
        observation
            .validate()
            .map_err(|_| ErrorRateProtectionError::InvalidObservation)?;
        if !enabled {
            return Ok(None);
        }
        let probe_state_revision = match observation.source {
            ObservationSource::ActiveProbe | ObservationSource::RealRequest => {
                observation.probe_state_revision
            }
            _ => None,
        };
        // Status-monitoring probes do not carry a durable protection fence and
        // must not be mistaken for a Half-Open request probe. Ordinary real
        // requests remain valid error-rate samples; only a real request with a
        // revision fence is classified as a Half-Open probe.
        if matches!(observation.source, ObservationSource::ActiveProbe)
            && probe_state_revision.is_none()
        {
            return Ok(None);
        }
        let Some(classified) = classify_observation(observation) else {
            return Ok(None);
        };
        let scope = scope_for_observation(observation);
        Ok(Some(HealthProtectionObservation {
            id: observation.id.clone(),
            scope,
            observed_at_ms: observation.order.event_at_ms,
            outcome: classified.outcome,
            probe: probe_state_revision.is_some(),
            probe_state_revision,
            retry_after_ms: None,
        }))
    }

    #[cfg(test)]
    pub(crate) fn history_event_seed(
        &self,
        observation: &RoutingObservation,
        transition: Option<HealthProtectionTransitionCode>,
    ) -> Result<Option<ErrorRateHistoryEventV1>, ErrorRateProtectionError> {
        self.history_event_seed_with_enabled(observation, transition, self.config.enabled)
    }

    pub(crate) fn history_event_seed_with_enabled(
        &self,
        observation: &RoutingObservation,
        transition: Option<HealthProtectionTransitionCode>,
        enabled: bool,
    ) -> Result<Option<ErrorRateHistoryEventV1>, ErrorRateProtectionError> {
        observation
            .validate()
            .map_err(|_| ErrorRateProtectionError::InvalidObservation)?;
        if !enabled
            || (matches!(observation.source, ObservationSource::ActiveProbe)
                && observation.probe_state_revision.is_some())
        {
            return Ok(None);
        }
        let Some(classified) = classify_observation(observation) else {
            return Ok(None);
        };
        let scope = scope_for_observation(observation);
        let failure = outcome_is_failure(classified.outcome);
        Ok(Some(ErrorRateHistoryEventV1 {
            observed_at_ms: observation.order.event_at_ms,
            scope_kind: scope.kind,
            scope_commitment: history_scope_commitment(&scope),
            outcome: if failure {
                ErrorRateHistoryOutcome::Failure
            } else {
                ErrorRateHistoryOutcome::Success
            },
            failure_code: outcome_failure_code(classified.outcome),
            sample_count: 1,
            failure_count: usize::from(failure),
            failure_rate_percent: if failure { 100 } else { 0 },
            transition,
        }))
    }

    #[cfg(test)]
    pub(crate) fn history_page(
        &mut self,
        before_ms: Option<i64>,
        limit: usize,
        now_ms: i64,
    ) -> Result<ErrorRateHistoryPageV1, ErrorRateProtectionError> {
        if now_ms < 0 {
            return Err(ErrorRateProtectionError::InvalidObservation);
        }
        self.trim(now_ms);
        let limit = limit.clamp(1, self.config.history_max_events);
        let mut events = self
            .events
            .iter()
            .rev()
            .filter(|stored| before_ms.is_none_or(|before| stored.event.observed_at_ms < before))
            .take(limit)
            .map(|stored| stored.event.clone())
            .collect::<Vec<_>>();
        events.reverse();
        let next_before_ms = events.first().map(|event| event.observed_at_ms);
        Ok(ErrorRateHistoryPageV1 {
            version: ERROR_RATE_HISTORY_VERSION.to_string(),
            enabled: self.config.enabled,
            detail_available: self.config.enabled && !events.is_empty(),
            events,
            next_before_ms,
            dropped_events: self.dropped_events,
        })
    }

    #[cfg(test)]
    pub(crate) fn snapshot(
        &mut self,
        generated_at_ms: i64,
    ) -> Result<ErrorRateHistorySnapshotV1, ErrorRateProtectionError> {
        if generated_at_ms < 0 {
            return Err(ErrorRateProtectionError::InvalidSnapshot);
        }
        self.trim(generated_at_ms);
        Ok(ErrorRateHistorySnapshotV1 {
            version: ERROR_RATE_HISTORY_VERSION.to_string(),
            generated_at_ms,
            enabled: self.config.enabled,
            dropped_events: self.dropped_events,
            events: self
                .events
                .iter()
                .map(|stored| stored.event.clone())
                .collect(),
            reducer_snapshot: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn restore(
        config: ErrorRateProtectionConfigV1,
        snapshot: ErrorRateHistorySnapshotV1,
    ) -> Result<Self, ErrorRateProtectionError> {
        config.validate()?;
        if snapshot.version != ERROR_RATE_HISTORY_VERSION
            || snapshot.generated_at_ms < 0
            || snapshot.events.len() > config.history_max_events
        {
            return Err(ErrorRateProtectionError::InvalidSnapshot);
        }
        let mut adapter = Self::new(config)?;
        if snapshot.reducer_snapshot.is_some() {
            return Err(ErrorRateProtectionError::InvalidSnapshot);
        }
        let mut previous_at = 0_i64;
        for (index, event) in snapshot.events.into_iter().enumerate() {
            validate_history_event(&event)?;
            if index > 0 && event.observed_at_ms < previous_at {
                return Err(ErrorRateProtectionError::InvalidSnapshot);
            }
            previous_at = event.observed_at_ms;
            let id = format!("restored-{index}");
            adapter.seen_ids.push_back(id.clone());
            adapter.events.push_back(StoredHistoryEvent { id, event });
        }
        adapter.dropped_events = snapshot.dropped_events;
        adapter.trim(snapshot.generated_at_ms);
        Ok(adapter)
    }

    #[cfg(test)]
    fn trim(&mut self, now_ms: i64) {
        while self.events.len() > self.config.history_max_events {
            self.events.pop_front();
            self.dropped_events = self.dropped_events.saturating_add(1);
        }
        while self.events.front().is_some_and(|event| {
            now_ms.saturating_sub(event.event.observed_at_ms) > self.config.history_retention_ms
        }) {
            self.events.pop_front();
            self.dropped_events = self.dropped_events.saturating_add(1);
        }
    }
}

fn history_scope_commitment(scope: &HealthProtectionScope) -> String {
    if scope.commitment.len() == 64
        && scope
            .commitment
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return scope.commitment.clone();
    }
    format!("{:x}", Sha256::digest(scope.commitment.as_bytes()))
}

#[derive(Debug, Clone, Copy)]
struct ClassifiedObservation {
    outcome: HealthProtectionObservationOutcome,
}

fn outcome_is_failure(outcome: HealthProtectionObservationOutcome) -> bool {
    matches!(outcome, HealthProtectionObservationOutcome::Failure(_))
}

fn outcome_failure_code(
    outcome: HealthProtectionObservationOutcome,
) -> Option<HealthProtectionFailureCode> {
    match outcome {
        HealthProtectionObservationOutcome::Failure(code) => Some(code),
        HealthProtectionObservationOutcome::Success => None,
    }
}

fn classify_observation(observation: &RoutingObservation) -> Option<ClassifiedObservation> {
    if matches!(observation.source, ObservationSource::Administrative)
        || matches!(
            observation.traffic_equivalence,
            TrafficEquivalence::Anonymous
        )
    {
        return None;
    }
    let outcome = match observation.outcome {
        ObservationOutcome::Success => HealthProtectionObservationOutcome::Success,
        ObservationOutcome::EndpointFailure => HealthProtectionObservationOutcome::Failure(
            HealthProtectionFailureCode::EndpointUnavailable,
        ),
        ObservationOutcome::RateLimited => {
            HealthProtectionObservationOutcome::Failure(HealthProtectionFailureCode::RateLimited)
        }
        ObservationOutcome::Timeout => HealthProtectionObservationOutcome::Failure(
            HealthProtectionFailureCode::FirstByteTimeout,
        ),
        ObservationOutcome::Unknown => {
            HealthProtectionObservationOutcome::Failure(failure_code_from_label("unknown"))
        }
        ObservationOutcome::CredentialFailure => return None,
        ObservationOutcome::ModelFailure => return None,
        ObservationOutcome::Cancelled => return None,
    };
    Some(ClassifiedObservation { outcome })
}

#[cfg(test)]
fn ignored_disposition(observation: &RoutingObservation) -> ErrorRateObservationDisposition {
    if matches!(observation.source, ObservationSource::Administrative) {
        ErrorRateObservationDisposition::IgnoredAdministrative
    } else if matches!(
        observation.traffic_equivalence,
        TrafficEquivalence::Anonymous
    ) {
        ErrorRateObservationDisposition::IgnoredAnonymous
    } else {
        match observation.outcome {
            ObservationOutcome::CredentialFailure => {
                ErrorRateObservationDisposition::IgnoredCredentialFailure
            }
            ObservationOutcome::ModelFailure => {
                ErrorRateObservationDisposition::IgnoredModelFailure
            }
            ObservationOutcome::Cancelled => ErrorRateObservationDisposition::IgnoredCancelled,
            _ => ErrorRateObservationDisposition::IgnoredAdministrative,
        }
    }
}

fn scope_for_observation(observation: &RoutingObservation) -> HealthProtectionScope {
    // A leased real-request probe carries its exact durable scope through the
    // lifecycle. Never infer Credential from station_key_id when the probe was
    // admitted against an Endpoint (or another future) scope.
    if let Some(scope) = observation.probe_scope.clone() {
        return scope;
    }
    let (kind, value) = if let Some(value) = observation.scope.station_key_id.as_deref() {
        (HealthProtectionScopeKind::Credential, value)
    } else if let Some(value) = observation.scope.model.as_deref() {
        (HealthProtectionScopeKind::Model, value)
    } else if let (Some(station_id), Some(endpoint_revision)) = (
        observation.scope.station_id.as_deref(),
        observation.scope.endpoint_revision,
    ) {
        return endpoint_health_scope(station_id, endpoint_revision).unwrap_or_else(|| {
            HealthProtectionScope::from_untrusted(HealthProtectionScopeKind::Endpoint, station_id)
        });
    } else {
        (HealthProtectionScopeKind::Endpoint, "global")
    };
    HealthProtectionScope::from_untrusted(kind, value)
}

#[cfg(test)]
fn validate_history_event(event: &ErrorRateHistoryEventV1) -> Result<(), ErrorRateProtectionError> {
    if event.observed_at_ms < 0
        || event.scope_commitment.is_empty()
        || event.scope_commitment.len() > MAX_HISTORY_SCOPE_BYTES
        || !event
            .scope_commitment
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || event.sample_count == 0 && event.failure_count != 0
        || event.failure_count > event.sample_count
        || event.failure_rate_percent > 100
    {
        return Err(ErrorRateProtectionError::InvalidSnapshot);
    }
    Ok(())
}

/// A compact low-cardinality aggregate for a scope.  This is useful for a UI
/// summary and keeps failure-code labels finite even when an upstream changes
/// its error text.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ErrorRateScopeSummaryV1 {
    pub(crate) scope_kind: HealthProtectionScopeKind,
    pub(crate) scope_commitment: String,
    pub(crate) sample_count: usize,
    pub(crate) failure_count: usize,
    pub(crate) failure_rate_percent: u8,
    pub(crate) failure_code_counts: Vec<(HealthProtectionFailureCode, usize)>,
}

#[cfg(test)]
pub(crate) fn aggregate_scope_summary(
    events: &[ErrorRateHistoryEventV1],
) -> Vec<ErrorRateScopeSummaryV1> {
    let mut summaries: BTreeMap<(HealthProtectionScopeKind, String), ErrorRateScopeSummaryV1> =
        BTreeMap::new();
    for event in events {
        let key = (event.scope_kind, event.scope_commitment.clone());
        let summary = summaries
            .entry(key)
            .or_insert_with(|| ErrorRateScopeSummaryV1 {
                scope_kind: event.scope_kind,
                scope_commitment: event.scope_commitment.clone(),
                sample_count: 0,
                failure_count: 0,
                failure_rate_percent: 0,
                failure_code_counts: Vec::new(),
            });
        summary.sample_count = summary.sample_count.saturating_add(1);
        if matches!(event.outcome, ErrorRateHistoryOutcome::Failure) {
            summary.failure_count = summary.failure_count.saturating_add(1);
            if let Some(code) = event.failure_code {
                if let Some((_, count)) = summary
                    .failure_code_counts
                    .iter_mut()
                    .find(|(existing, _)| *existing == code)
                {
                    *count = count.saturating_add(1);
                } else if summary.failure_code_counts.len() < MAX_FAILURE_CODE_COUNTS {
                    summary.failure_code_counts.push((code, 1));
                }
            }
        }
    }
    for summary in summaries.values_mut() {
        summary.failure_rate_percent = if summary.sample_count == 0 {
            0
        } else {
            ((summary.failure_count * 100) / summary.sample_count).min(100) as u8
        };
    }
    summaries.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(
        id: &str,
        at_ms: i64,
        outcome: ObservationOutcome,
        source: ObservationSource,
        traffic: TrafficEquivalence,
    ) -> RoutingObservation {
        RoutingObservation {
            id: id.to_string(),
            order: crate::models::routing_observation::ObservationOrder {
                producer_id: "test-producer".to_string(),
                producer_sequence: at_ms as u64 + 1,
                event_at_ms: at_ms,
                ingested_at_ms: at_ms,
            },
            scope: crate::models::routing_observation::ObservationScope {
                station_id: Some("https://provider.invalid/api?token=secret".to_string()),
                station_key_id: Some("key-secret".to_string()),
                model: Some("gpt-secret".to_string()),
                endpoint_revision: Some(1),
            },
            source,
            traffic_equivalence: traffic,
            outcome,
            latency_ms: Some(10),
            evidence_mass_basis_points: 10_000,
            probe_scope: None,
            probe_state_revision: None,
        }
    }

    #[test]
    fn disabled_by_default_does_not_feed_reducer_or_retain_history() {
        let mut adapter = ErrorRateProtectionAdapter::disabled().expect("adapter");
        let result = adapter
            .observe(&observation(
                "one",
                10,
                ObservationOutcome::EndpointFailure,
                ObservationSource::RealRequest,
                TrafficEquivalence::ExactRequest,
            ))
            .expect("observe");
        assert_eq!(result, ErrorRateObservationDisposition::Disabled);
        let page = adapter.history_page(None, 20, 10).expect("history");
        assert!(!page.enabled);
        assert!(page.events.is_empty());
        assert!(!page.detail_available);
    }

    #[test]
    fn endpoint_observation_uses_revision_fenced_durable_commitment() {
        let observation = RoutingObservation {
            id: "endpoint-only".to_string(),
            order: crate::models::routing_observation::ObservationOrder {
                producer_id: "test".to_string(),
                producer_sequence: 1,
                event_at_ms: 1,
                ingested_at_ms: 1,
            },
            scope: crate::models::routing_observation::ObservationScope {
                station_id: Some("station-1".to_string()),
                station_key_id: None,
                model: None,
                endpoint_revision: Some(7),
            },
            source: ObservationSource::RealRequest,
            traffic_equivalence: TrafficEquivalence::EndpointOnly,
            outcome: ObservationOutcome::EndpointFailure,
            latency_ms: None,
            evidence_mass_basis_points: 10_000,
            probe_scope: None,
            probe_state_revision: None,
        };
        let scope = scope_for_observation(&observation);
        let durable = crate::persistence::stores::routing_health_verdict_store::ScopedHealthSubject::endpoint(
            "station-1", 7,
        )
        .expect("endpoint subject");
        assert_eq!(scope.kind, HealthProtectionScopeKind::Endpoint);
        assert_eq!(scope.commitment, durable.scope());
        assert!(!scope.commitment.contains("station-1"));
    }

    #[test]
    fn candidate_probe_bridge_is_credential_only_until_endpoint_resolver_exists() {
        let scopes = candidate_health_scopes("station-1", "credential-1", 7)
            .expect("credential probe scope");
        let collected = scopes.iter().cloned().collect::<Vec<_>>();
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].kind, HealthProtectionScopeKind::Credential);
        assert!(scopes.contains(&collected[0]));
        assert!(!scopes
            .contains(&endpoint_health_scope("station-1", 7).expect("endpoint durable scope")));
    }

    #[test]
    fn classification_is_low_cardinality_and_excludes_sensitive_scope_values() {
        let config = ErrorRateProtectionConfigV1 {
            enabled: true,
            history_max_events: 10,
            history_retention_ms: 1_000,
            ..Default::default()
        };
        let mut adapter = ErrorRateProtectionAdapter::new(config).expect("adapter");
        adapter
            .observe(&observation(
                "one",
                10,
                ObservationOutcome::Success,
                ObservationSource::RealRequest,
                TrafficEquivalence::ExactRequest,
            ))
            .expect("observe");
        let page = adapter.history_page(None, 20, 10).expect("history");
        let encoded = serde_json::to_string(&page).expect("serialize");
        assert!(!encoded.contains("provider.invalid"));
        assert!(!encoded.contains("token=secret"));
        assert!(!encoded.contains("key-secret"));
        assert!(!encoded.contains("gpt-secret"));
        assert_eq!(page.events[0].failure_code, None);
        assert!(page.events[0]
            .scope_commitment
            .chars()
            .all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn only_controlled_failures_are_recorded_for_reducer_input_port() {
        let config = ErrorRateProtectionConfigV1 {
            enabled: true,
            history_max_events: 10,
            history_retention_ms: 1_000,
            ..Default::default()
        };
        let mut adapter = ErrorRateProtectionAdapter::new(config).expect("adapter");
        assert_eq!(
            adapter
                .observe(&observation(
                    "auth",
                    1,
                    ObservationOutcome::CredentialFailure,
                    ObservationSource::RealRequest,
                    TrafficEquivalence::ExactRequest,
                ))
                .expect("observe"),
            ErrorRateObservationDisposition::IgnoredCredentialFailure
        );
        assert_eq!(
            adapter
                .observe(&observation(
                    "a",
                    10,
                    ObservationOutcome::EndpointFailure,
                    ObservationSource::RealRequest,
                    TrafficEquivalence::ExactRequest,
                ))
                .expect("observe"),
            ErrorRateObservationDisposition::Recorded
        );
        assert_eq!(
            adapter
                .observe(&observation(
                    "b",
                    11,
                    ObservationOutcome::EndpointFailure,
                    ObservationSource::RealRequest,
                    TrafficEquivalence::SameModelShape,
                ))
                .expect("observe"),
            ErrorRateObservationDisposition::Recorded
        );
        let page = adapter.history_page(None, 10, 11).expect("history");
        assert_eq!(page.events.len(), 2);
        assert_eq!(page.events.last().and_then(|e| e.transition), None);
        assert_eq!(
            page.events.last().map(|e| e.failure_rate_percent),
            Some(100)
        );
    }

    #[test]
    fn reducer_input_port_returns_canonical_scope_and_failure_code_without_transition() {
        let config = ErrorRateProtectionConfigV1 {
            enabled: true,
            ..Default::default()
        };
        let adapter = ErrorRateProtectionAdapter::new(config).expect("adapter");
        let input = adapter
            .health_observation(&observation(
                "timeout",
                20,
                ObservationOutcome::Timeout,
                ObservationSource::RealRequest,
                TrafficEquivalence::ExactRequest,
            ))
            .expect("input result")
            .expect("canonical input");
        assert!(!input.probe);
        assert_eq!(input.scope.kind, HealthProtectionScopeKind::Credential);
        assert_eq!(
            input.outcome,
            HealthProtectionObservationOutcome::Failure(
                HealthProtectionFailureCode::FirstByteTimeout
            )
        );
        assert!(input
            .scope
            .commitment
            .chars()
            .all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn real_request_probe_revision_is_consumed_as_a_fenced_probe() {
        let config = ErrorRateProtectionConfigV1 {
            enabled: true,
            ..Default::default()
        };
        let mut probe_observation = observation(
            "real-probe",
            30,
            ObservationOutcome::Success,
            ObservationSource::RealRequest,
            TrafficEquivalence::ExactRequest,
        );
        probe_observation.probe_state_revision = Some(7);
        let adapter = ErrorRateProtectionAdapter::new(config).expect("adapter");
        let input = adapter
            .health_observation(&probe_observation)
            .expect("probe input result")
            .expect("real request probe input");
        assert!(input.probe);
        assert_eq!(input.probe_state_revision, Some(7));
        assert_eq!(input.outcome, HealthProtectionObservationOutcome::Success);
    }

    #[test]
    fn typed_probe_scope_is_hashed_for_legacy_bounded_history_storage() {
        let config = ErrorRateProtectionConfigV1 {
            enabled: true,
            ..Default::default()
        };
        let mut probe_observation = observation(
            "endpoint-probe-history",
            32,
            ObservationOutcome::EndpointFailure,
            ObservationSource::RealRequest,
            TrafficEquivalence::ExactRequest,
        );
        probe_observation.probe_scope =
            Some(endpoint_health_scope("station-1", 1).expect("endpoint scope"));
        probe_observation.probe_state_revision = Some(3);
        let adapter = ErrorRateProtectionAdapter::new(config).expect("adapter");
        let event = adapter
            .history_event_seed(&probe_observation, None)
            .expect("history seed")
            .expect("typed probe history");
        assert_eq!(event.scope_kind, HealthProtectionScopeKind::Endpoint);
        assert_eq!(event.scope_commitment.len(), 64);
        assert!(event
            .scope_commitment
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn unfenced_active_probe_is_not_a_reducer_input() {
        let config = ErrorRateProtectionConfigV1 {
            enabled: true,
            ..Default::default()
        };
        let mut active_probe = observation(
            "status-probe",
            31,
            ObservationOutcome::Success,
            ObservationSource::ActiveProbe,
            TrafficEquivalence::ExactRequest,
        );
        active_probe.probe_state_revision = None;
        let adapter = ErrorRateProtectionAdapter::new(config).expect("adapter");
        assert!(adapter
            .health_observation(&active_probe)
            .expect("observation")
            .is_none());
    }

    #[test]
    fn scoped_admission_bridge_is_fail_closed_for_open_and_half_open() {
        let scope = admission_scope(HealthProtectionScopeKind::Credential, "key-a");
        let status = |state| HealthProtectionStatus {
            version: crate::application::health_protection::HEALTH_PROTECTION_VERSION.to_string(),
            scope: scope.clone(),
            state,
            persistence_kind:
                crate::application::health_protection::HealthProtectionPersistenceKind::Durable,
            state_revision: 1,
            opened_at_ms: Some(10),
            cooldown_until_ms: Some(100),
            cooldown_remaining_ms: Some(90),
            half_open_probe_in_flight: state == HealthProtectionState::HalfOpen,
            recent_failure_code: Some(HealthProtectionFailureCode::EndpointUnavailable),
            sample_count: 5,
            failure_rate_percent: 100,
            updated_at_ms: 10,
            detail_available: true,
        };
        assert_eq!(
            scoped_admission_verdict(&[status(HealthProtectionState::Open)], &scope),
            ErrorRateScopedVerdict::Suppressed(HealthProtectionState::Open)
        );
        assert_eq!(
            scoped_admission_verdict(&[status(HealthProtectionState::HalfOpen)], &scope),
            ErrorRateScopedVerdict::Suppressed(HealthProtectionState::HalfOpen)
        );
        assert_eq!(
            scoped_admission_verdict(&[status(HealthProtectionState::Closed)], &scope),
            ErrorRateScopedVerdict::Admitted
        );
        let probe = HealthProtectionProbe {
            scope: scope.clone(),
            state_revision: 1,
        };
        assert_eq!(
            scoped_admission_verdict_with_probe(
                &[status(HealthProtectionState::HalfOpen)],
                &scope,
                Some(&probe),
            ),
            ErrorRateScopedVerdict::Admitted
        );
    }

    #[test]
    fn probe_candidate_admission_only_releases_expired_open_scope() {
        let scope = admission_scope(HealthProtectionScopeKind::Credential, "key-a");
        let status = |state, remaining| HealthProtectionStatus {
            version: crate::application::health_protection::HEALTH_PROTECTION_VERSION.to_string(),
            scope: scope.clone(),
            state,
            persistence_kind:
                crate::application::health_protection::HealthProtectionPersistenceKind::Durable,
            state_revision: 2,
            opened_at_ms: Some(10),
            cooldown_until_ms: Some(100),
            cooldown_remaining_ms: remaining,
            half_open_probe_in_flight: state == HealthProtectionState::HalfOpen,
            recent_failure_code: Some(HealthProtectionFailureCode::EndpointUnavailable),
            sample_count: 5,
            failure_rate_percent: 100,
            updated_at_ms: 10,
            detail_available: true,
        };
        assert_eq!(
            scoped_admission_verdict_for_probe_candidate(
                &[status(HealthProtectionState::Open, Some(0))],
                &scope,
            ),
            ErrorRateScopedVerdict::Admitted
        );
        assert_eq!(
            scoped_admission_verdict_for_probe_candidate(
                &[status(HealthProtectionState::Open, Some(1))],
                &scope,
            ),
            ErrorRateScopedVerdict::Suppressed(HealthProtectionState::Open)
        );
        assert_eq!(
            scoped_admission_verdict_for_probe_candidate(
                &[status(HealthProtectionState::HalfOpen, Some(0))],
                &scope,
            ),
            ErrorRateScopedVerdict::Suppressed(HealthProtectionState::HalfOpen)
        );
    }

    #[test]
    fn scoped_admission_bridge_never_matches_raw_or_different_scope() {
        let canonical = admission_scope(HealthProtectionScopeKind::Credential, "key-a");
        let other = admission_scope(HealthProtectionScopeKind::Credential, "key-b");
        let status = HealthProtectionStatus {
            version: crate::application::health_protection::HEALTH_PROTECTION_VERSION.to_string(),
            scope: other,
            state: HealthProtectionState::Open,
            persistence_kind:
                crate::application::health_protection::HealthProtectionPersistenceKind::Durable,
            state_revision: 1,
            opened_at_ms: Some(10),
            cooldown_until_ms: Some(100),
            cooldown_remaining_ms: Some(90),
            half_open_probe_in_flight: false,
            recent_failure_code: Some(HealthProtectionFailureCode::EndpointUnavailable),
            sample_count: 5,
            failure_rate_percent: 100,
            updated_at_ms: 10,
            detail_available: true,
        };
        assert_eq!(
            scoped_admission_verdict(&[status], &canonical),
            ErrorRateScopedVerdict::Admitted,
            "a different commitment must not suppress this candidate"
        );
        assert_ne!(canonical.commitment, "key-a");
    }

    #[test]
    fn scoped_admission_bridge_does_not_turn_runtime_overlay_into_durable_error_rate() {
        let scope = admission_scope(HealthProtectionScopeKind::Credential, "key-a");
        let status = HealthProtectionStatus {
            version: crate::application::health_protection::HEALTH_PROTECTION_VERSION.to_string(),
            scope: scope.clone(),
            state: HealthProtectionState::Open,
            persistence_kind:
                crate::application::health_protection::HealthProtectionPersistenceKind::RuntimeOutlier,
            state_revision: 1,
            opened_at_ms: Some(10),
            cooldown_until_ms: Some(100),
            cooldown_remaining_ms: Some(90),
            half_open_probe_in_flight: false,
            recent_failure_code: Some(HealthProtectionFailureCode::EndpointUnavailable),
            sample_count: 5,
            failure_rate_percent: 100,
            updated_at_ms: 10,
            detail_available: true,
        };
        assert_eq!(
            scoped_admission_verdict(&[status], &scope),
            ErrorRateScopedVerdict::Admitted,
            "runtime overlays have a separate capacity/ejection owner"
        );
    }

    #[test]
    fn history_retention_and_page_cursor_are_bounded_and_deterministic() {
        let config = ErrorRateProtectionConfigV1 {
            enabled: true,
            history_max_events: 2,
            history_retention_ms: 10,
            ..Default::default()
        };
        let mut adapter = ErrorRateProtectionAdapter::new(config).expect("adapter");
        for (index, at_ms) in [0, 5, 20].into_iter().enumerate() {
            adapter
                .observe(&observation(
                    &format!("id-{index}"),
                    at_ms,
                    ObservationOutcome::Success,
                    ObservationSource::RealRequest,
                    TrafficEquivalence::ExactRequest,
                ))
                .expect("observe");
        }
        let page = adapter.history_page(None, 1, 20).expect("history");
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].observed_at_ms, 20);
        assert!(page.next_before_ms.is_some());
        assert!(page.dropped_events >= 1);
    }

    #[test]
    fn snapshot_restore_preserves_safe_history_and_rejects_invalid_scope() {
        let config = ErrorRateProtectionConfigV1 {
            enabled: true,
            history_max_events: 10,
            history_retention_ms: 1_000,
            ..Default::default()
        };
        let mut adapter = ErrorRateProtectionAdapter::new(config.clone()).expect("adapter");
        adapter
            .observe(&observation(
                "one",
                10,
                ObservationOutcome::Timeout,
                ObservationSource::RealRequest,
                TrafficEquivalence::ExactRequest,
            ))
            .expect("observe");
        let snapshot = adapter.snapshot(10).expect("snapshot");
        let mut restored = ErrorRateProtectionAdapter::restore(config, snapshot).expect("restore");
        let restored_page = restored.history_page(None, 10, 10).expect("history");
        assert_eq!(restored_page.events.len(), 1);
        let mut invalid = restored.snapshot(10).expect("snapshot");
        invalid.events[0].scope_commitment = "https://secret.invalid".to_string();
        assert!(matches!(
            ErrorRateProtectionAdapter::restore(
                ErrorRateProtectionConfigV1 {
                    enabled: true,
                    history_max_events: 10,
                    history_retention_ms: 1_000,
                    ..Default::default()
                },
                invalid,
            ),
            Err(ErrorRateProtectionError::InvalidSnapshot)
        ));
    }

    #[test]
    fn aggregate_summary_has_finite_failure_code_cardinality() {
        let events = (0..100)
            .map(|index| ErrorRateHistoryEventV1 {
                observed_at_ms: index,
                scope_kind: HealthProtectionScopeKind::Endpoint,
                scope_commitment: "a".repeat(64),
                outcome: ErrorRateHistoryOutcome::Failure,
                failure_code: Some(HealthProtectionFailureCode::Unknown),
                sample_count: 1,
                failure_count: 1,
                failure_rate_percent: 100,
                transition: None,
            })
            .collect::<Vec<_>>();
        let summaries = aggregate_scope_summary(&events);
        assert_eq!(summaries.len(), 1);
        assert_eq!(
            summaries[0].failure_code_counts,
            vec![(HealthProtectionFailureCode::Unknown, 100)]
        );
    }
}
