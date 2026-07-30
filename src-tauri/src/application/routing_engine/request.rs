#![allow(dead_code)]

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RouteKind {
    Inference,
    ModelCatalog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrderingProfile {
    PriorityFirst,
    CostFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupFilterMode {
    Any,
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CanonicalRouteRequest {
    pub(crate) route_kind: RouteKind,
    pub(crate) requested_model: Option<String>,
    pub(crate) stream: bool,
    pub(crate) uses_tools: bool,
    pub(crate) uses_vision: bool,
    pub(crate) uses_reasoning: bool,
    pub(crate) untrusted_headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ValidatedLocalRouteSettings {
    pub(crate) ordering_profile: OrderingProfile,
    pub(crate) max_rate_multiplier: Option<f64>,
    pub(crate) group_filter_mode: GroupFilterMode,
    pub(crate) required_group_stable_key: Option<String>,
    pub(crate) preferred_models: Vec<String>,
    pub(crate) required_tags: Vec<String>,
    pub(crate) allow_depleted_fallback: bool,
    pub(crate) affinity_enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RouteRequestFacts {
    route_kind: RouteKind,
    requested_model: Option<String>,
    stream: bool,
    uses_tools: bool,
    uses_vision: bool,
    uses_reasoning: bool,
    ordering_profile: OrderingProfile,
    max_rate_multiplier: Option<f64>,
    group_filter_mode: GroupFilterMode,
    required_group_stable_key: Option<String>,
    preferred_models: Vec<String>,
    required_tags: Vec<String>,
    allow_depleted_fallback: bool,
    affinity_enabled: bool,
    admitted_at_ms: i64,
}

impl RouteRequestFacts {
    pub(crate) fn route_kind(&self) -> RouteKind {
        self.route_kind
    }

    pub(crate) fn requested_model(&self) -> Option<&str> {
        self.requested_model.as_deref()
    }

    pub(crate) fn stream(&self) -> bool {
        self.stream
    }

    pub(crate) fn uses_tools(&self) -> bool {
        self.uses_tools
    }

    pub(crate) fn uses_vision(&self) -> bool {
        self.uses_vision
    }

    pub(crate) fn uses_reasoning(&self) -> bool {
        self.uses_reasoning
    }

    pub(crate) fn ordering_profile(&self) -> OrderingProfile {
        self.ordering_profile
    }

    pub(crate) fn max_rate_multiplier(&self) -> Option<f64> {
        self.max_rate_multiplier
    }

    pub(crate) fn group_filter_mode(&self) -> GroupFilterMode {
        self.group_filter_mode
    }

    pub(crate) fn required_group_stable_key(&self) -> Option<&str> {
        self.required_group_stable_key.as_deref()
    }

    pub(crate) fn preferred_models(&self) -> &[String] {
        &self.preferred_models
    }

    pub(crate) fn required_tags(&self) -> &[String] {
        &self.required_tags
    }

    pub(crate) fn allow_depleted_fallback(&self) -> bool {
        self.allow_depleted_fallback
    }

    pub(crate) fn affinity_enabled(&self) -> bool {
        self.affinity_enabled
    }

    pub(crate) fn admitted_at_ms(&self) -> i64 {
        self.admitted_at_ms
    }
}

pub(crate) struct RouteRequestClassifier;

impl RouteRequestClassifier {
    pub(crate) fn classify(
        request: CanonicalRouteRequest,
        settings: ValidatedLocalRouteSettings,
        admitted_at_ms: i64,
    ) -> RouteRequestFacts {
        RouteRequestFacts {
            route_kind: request.route_kind,
            requested_model: request.requested_model,
            stream: request.stream,
            uses_tools: request.uses_tools,
            uses_vision: request.uses_vision,
            uses_reasoning: request.uses_reasoning,
            ordering_profile: settings.ordering_profile,
            max_rate_multiplier: settings.max_rate_multiplier,
            group_filter_mode: settings.group_filter_mode,
            required_group_stable_key: settings.required_group_stable_key,
            preferred_models: settings.preferred_models,
            required_tags: settings.required_tags,
            allow_depleted_fallback: settings.allow_depleted_fallback,
            affinity_enabled: settings.affinity_enabled,
            admitted_at_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RouteProgress {
    ordinal: u32,
    actual_attempt_exclusions: BTreeSet<String>,
    deadline_ms: i64,
    attempt_count: u32,
    snapshot_rebuild_count: u32,
    runtime_rebuild_count: u32,
}

impl RouteProgress {
    pub(crate) fn new(deadline_ms: i64) -> Self {
        Self {
            ordinal: 0,
            actual_attempt_exclusions: BTreeSet::new(),
            deadline_ms,
            attempt_count: 0,
            snapshot_rebuild_count: 0,
            runtime_rebuild_count: 0,
        }
    }

    pub(crate) fn record_actual_attempt(&mut self, station_key_id: impl Into<String>) {
        self.ordinal = self.ordinal.saturating_add(1);
        self.attempt_count = self.attempt_count.saturating_add(1);
        self.actual_attempt_exclusions.insert(station_key_id.into());
    }

    pub(crate) fn tighten_deadline(&mut self, deadline_ms: i64) -> bool {
        if deadline_ms <= self.deadline_ms {
            self.deadline_ms = deadline_ms;
            true
        } else {
            false
        }
    }

    pub(crate) fn record_snapshot_rebuild(&mut self) {
        self.snapshot_rebuild_count = self.snapshot_rebuild_count.saturating_add(1);
    }

    pub(crate) fn record_runtime_rebuild(&mut self) {
        self.runtime_rebuild_count = self.runtime_rebuild_count.saturating_add(1);
    }

    pub(crate) fn view(&self) -> RouteProgressView {
        RouteProgressView {
            ordinal: self.ordinal,
            actual_attempt_exclusions: self.actual_attempt_exclusions.clone(),
            deadline_ms: self.deadline_ms,
            attempt_count: self.attempt_count,
            snapshot_rebuild_count: self.snapshot_rebuild_count,
            runtime_rebuild_count: self.runtime_rebuild_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RouteProgressView {
    pub(crate) ordinal: u32,
    pub(crate) actual_attempt_exclusions: BTreeSet<String>,
    pub(crate) deadline_ms: i64,
    pub(crate) attempt_count: u32,
    pub(crate) snapshot_rebuild_count: u32,
    pub(crate) runtime_rebuild_count: u32,
}

impl RouteProgressView {
    pub(crate) fn excludes_station_key(&self, station_key_id: &str) -> bool {
        self.actual_attempt_exclusions.contains(station_key_id)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlanningRoundContext {
    pub(crate) request: RouteRequestFacts,
    pub(crate) progress: RouteProgressView,
    pub(crate) snapshot_id: String,
    pub(crate) runtime_overlay_revision: u64,
}
