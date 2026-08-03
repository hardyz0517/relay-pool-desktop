use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PRICING_GROUP_MONITORING_SCHEMA_VERSION: u32 = 1;
pub const MAX_PRICING_GROUP_REFS: usize = 500;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PricingGroupMonitorStatusInput {
    pub schema_version: u32,
    pub group_refs_hash: String,
    pub groups: Vec<CanonicalGroupRef>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingGroupMonitorStatusWorkspace {
    pub schema_version: u32,
    pub generated_at_ms: i64,
    pub group_refs_hash: String,
    pub requested_group_count: u32,
    pub returned_group_count: u32,
    pub omitted_group_count: u32,
    pub items: Vec<PricingGroupMonitorSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalGroupRef {
    pub station_id: String,
    pub group_binding_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_key_hash: String,
}

impl CanonicalGroupRef {
    pub fn canonical_key(&self) -> Result<String, PricingGroupMonitoringInputError> {
        let station_id = normalized_required(&self.station_id, "stationId")?;
        if let Some(binding_id) = normalized_optional(self.group_binding_id.as_deref()) {
            return Ok(format!("station:{station_id}:binding:{binding_id}"));
        }
        if let Some(group_id_hash) = normalized_optional(self.group_id_hash.as_deref()) {
            return Ok(format!("station:{station_id}:group-id:{group_id_hash}"));
        }
        if let Some(group_key_hash) = normalized_optional(Some(self.group_key_hash.as_str())) {
            return Ok(format!("station:{station_id}:group-key:{group_key_hash}"));
        }
        Err(PricingGroupMonitoringInputError::UnresolvedGroup)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PricingGroupMonitoringInputError {
    EmptyStationId,
    UnresolvedGroup,
    DuplicateGroupRef(String),
    TooManyGroupRefs(usize),
}

impl std::fmt::Display for PricingGroupMonitoringInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyStationId => write!(f, "stationId must not be empty"),
            Self::UnresolvedGroup => write!(f, "group reference cannot be resolved"),
            Self::DuplicateGroupRef(value) => write!(f, "duplicate group reference: {value}"),
            Self::TooManyGroupRefs(value) => {
                write!(
                    f,
                    "group reference count exceeds {MAX_PRICING_GROUP_REFS}: {value}"
                )
            }
        }
    }
}

impl std::error::Error for PricingGroupMonitoringInputError {}

pub fn canonicalize_group_refs(
    groups: &[CanonicalGroupRef],
) -> Result<Vec<String>, PricingGroupMonitoringInputError> {
    if groups.len() > MAX_PRICING_GROUP_REFS {
        return Err(PricingGroupMonitoringInputError::TooManyGroupRefs(
            groups.len(),
        ));
    }
    let mut refs = Vec::with_capacity(groups.len());
    for group in groups {
        refs.push(group.canonical_key()?);
    }
    refs.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    let mut unique = Vec::with_capacity(refs.len());
    for reference in refs {
        if unique.last() == Some(&reference) {
            return Err(PricingGroupMonitoringInputError::DuplicateGroupRef(
                reference,
            ));
        }
        unique.push(reference);
    }
    Ok(unique)
}

pub fn group_refs_hash(
    groups: &[CanonicalGroupRef],
) -> Result<String, PricingGroupMonitoringInputError> {
    let refs = canonicalize_group_refs(groups)?;
    Ok(format!("{:x}", Sha256::digest(refs.join("\n").as_bytes())))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    ExactBinding,
    ParentBinding,
    GroupIdHash,
    GroupKeyHash,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionState {
    Resolved,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LatestOutcome {
    Available,
    Degraded,
    Unavailable,
    Skipped,
    Missing,
}

impl LatestOutcome {
    pub fn from_probe_outcome(value: &str) -> Self {
        match value {
            "available" => Self::Available,
            "degraded" => Self::Degraded,
            "unavailable" => Self::Unavailable,
            "skipped" => Self::Skipped,
            _ => Self::Missing,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayState {
    Unresolved,
    NoKey,
    Unmonitored,
    Running,
    Untested,
    Available,
    Degraded,
    Unavailable,
    Skipped,
    UnavailableData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationKeySnapshot {
    pub id: String,
    pub priority: i64,
    pub created_at_ms: i64,
    pub group_binding_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub enabled: bool,
    pub credentialed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorSnapshot {
    pub id: String,
    pub station_id: String,
    pub created_at_ms: i64,
    pub target_type: String,
    pub station_key_id: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetResultSnapshot {
    pub id: String,
    pub monitor_id: String,
    pub station_key_id: Option<String>,
    pub terminal_outcome: LatestOutcome,
    pub failure_kind: Option<String>,
    pub terminal_reason: Option<String>,
    pub finished_at_ms: Option<i64>,
    pub latency_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningSnapshot {
    pub monitor_id: String,
    pub station_key_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricingGroupMonitorReducerInput {
    pub group_ref: CanonicalGroupRef,
    pub match_kind: MatchKind,
    pub resolution_state: ResolutionState,
    pub keys: Vec<StationKeySnapshot>,
    pub monitors: Vec<MonitorSnapshot>,
    pub target_results: Vec<TargetResultSnapshot>,
    pub running: Vec<RunningSnapshot>,
    pub generated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingGroupMonitorSummary {
    pub station_id: String,
    pub group_binding_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_key_hash: String,
    pub match_kind: MatchKind,
    pub resolution_state: ResolutionState,
    pub has_bound_key: bool,
    pub bound_key_count: u32,
    pub enabled_key_count: u32,
    pub credentialed_key_count: u32,
    pub enabled_monitor_definition_count: u32,
    pub monitored_key_count: u32,
    pub tested_key_count: u32,
    pub representative_key_id: Option<String>,
    pub representative_monitor_id: Option<String>,
    pub latest_target_result_id: Option<String>,
    pub latest_outcome: LatestOutcome,
    pub latest_failure_kind: Option<String>,
    pub latest_terminal_reason: Option<String>,
    pub running: bool,
    pub checked_at_ms: Option<i64>,
    pub latency_ms: Option<i64>,
    pub generated_at_ms: i64,
    pub display_state: DisplayState,
}

pub fn reduce_pricing_group_monitor_summary(
    input: PricingGroupMonitorReducerInput,
) -> PricingGroupMonitorSummary {
    let group_ref = input.group_ref;
    let unresolved = input.resolution_state == ResolutionState::Unresolved
        || input.match_kind == MatchKind::Unresolved;
    let mut keys = input.keys;
    keys.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then(left.created_at_ms.cmp(&right.created_at_ms))
            .then(left.id.cmp(&right.id))
    });
    let representative_key = keys.first();
    let enabled_monitors: Vec<&MonitorSnapshot> = input
        .monitors
        .iter()
        .filter(|monitor| monitor.enabled)
        .collect();
    let enabled_monitor_definition_count = enabled_monitors.len() as u32;
    let monitored_key_ids: BTreeSet<String> = enabled_monitors
        .iter()
        .flat_map(|monitor| {
            if monitor.target_type == "station" {
                keys.iter().map(|key| key.id.clone()).collect::<Vec<_>>()
            } else {
                monitor.station_key_id.iter().cloned().collect::<Vec<_>>()
            }
        })
        .filter(|key_id| keys.iter().any(|key| &key.id == key_id))
        .collect();
    let monitored_key_count = monitored_key_ids.len() as u32;

    let representative_monitor = representative_key.and_then(|key| {
        enabled_monitors
            .iter()
            .copied()
            .filter(|monitor| {
                monitor.target_type == "station"
                    || monitor.station_key_id.as_deref() == Some(key.id.as_str())
            })
            .min_by(|left, right| {
                left.created_at_ms
                    .cmp(&right.created_at_ms)
                    .then(left.id.cmp(&right.id))
            })
    });
    let representative_monitor_id = representative_monitor.map(|monitor| monitor.id.clone());

    let latest = representative_monitor.and_then(|monitor| {
        input
            .target_results
            .iter()
            .filter(|result| {
                result.monitor_id == monitor.id
                    && match (&result.station_key_id, monitor.target_type.as_str()) {
                        (Some(key_id), _) => representative_key
                            .map(|key| key.id == *key_id)
                            .unwrap_or(false),
                        (None, _) => false,
                    }
            })
            .max_by(|left, right| {
                left.finished_at_ms
                    .cmp(&right.finished_at_ms)
                    .then(left.id.cmp(&right.id))
            })
    });
    let running = representative_monitor
        .map(|monitor| {
            input.running.iter().any(|running| {
                running.monitor_id == monitor.id
                    && (running.station_key_id.is_none()
                        || running.station_key_id == representative_key.map(|key| key.id.clone()))
            })
        })
        .unwrap_or(false);

    let display_state = if unresolved {
        DisplayState::Unresolved
    } else if keys.is_empty() {
        DisplayState::NoKey
    } else if enabled_monitor_definition_count == 0 {
        DisplayState::Unmonitored
    } else if running {
        DisplayState::Running
    } else {
        match latest.map(|result| result.terminal_outcome) {
            None | Some(LatestOutcome::Missing) => DisplayState::Untested,
            Some(LatestOutcome::Available) => DisplayState::Available,
            Some(LatestOutcome::Degraded) => DisplayState::Degraded,
            Some(LatestOutcome::Unavailable) => DisplayState::Unavailable,
            Some(LatestOutcome::Skipped) => DisplayState::Skipped,
        }
    };

    let tested_key_ids: BTreeSet<String> = input
        .target_results
        .iter()
        .filter_map(|result| result.station_key_id.clone())
        .filter(|key_id| keys.iter().any(|key| &key.id == key_id))
        .collect();

    PricingGroupMonitorSummary {
        station_id: group_ref.station_id,
        group_binding_id: group_ref.group_binding_id,
        group_id_hash: group_ref.group_id_hash,
        group_key_hash: group_ref.group_key_hash,
        match_kind: input.match_kind,
        resolution_state: input.resolution_state,
        has_bound_key: !keys.is_empty(),
        bound_key_count: keys.len() as u32,
        enabled_key_count: keys.iter().filter(|key| key.enabled).count() as u32,
        credentialed_key_count: keys.iter().filter(|key| key.credentialed).count() as u32,
        enabled_monitor_definition_count,
        monitored_key_count,
        tested_key_count: tested_key_ids.len() as u32,
        representative_key_id: representative_key.map(|key| key.id.clone()),
        representative_monitor_id,
        latest_target_result_id: latest.map(|result| result.id.clone()),
        latest_outcome: latest
            .map(|result| result.terminal_outcome)
            .unwrap_or(LatestOutcome::Missing),
        latest_failure_kind: latest.and_then(|result| result.failure_kind.clone()),
        latest_terminal_reason: latest.and_then(|result| result.terminal_reason.clone()),
        running,
        checked_at_ms: latest.and_then(|result| result.finished_at_ms),
        latency_ms: latest.and_then(|result| result.latency_ms),
        generated_at_ms: input.generated_at_ms,
        display_state,
    }
}

fn normalized_required(
    value: &str,
    field: &str,
) -> Result<String, PricingGroupMonitoringInputError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(if field == "stationId" {
            PricingGroupMonitoringInputError::EmptyStationId
        } else {
            PricingGroupMonitoringInputError::UnresolvedGroup
        });
    }
    Ok(value.to_owned())
}

fn normalized_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group_ref() -> CanonicalGroupRef {
        CanonicalGroupRef {
            station_id: "station-1".into(),
            group_binding_id: Some("binding-1".into()),
            group_id_hash: Some("group-id-1".into()),
            group_key_hash: "group-key-1".into(),
        }
    }

    #[test]
    fn canonical_ref_prefers_binding_then_group_id_then_group_key() {
        assert_eq!(
            group_ref().canonical_key().unwrap(),
            "station:station-1:binding:binding-1"
        );
        let mut value = group_ref();
        value.group_binding_id = None;
        assert_eq!(
            value.canonical_key().unwrap(),
            "station:station-1:group-id:group-id-1"
        );
        value.group_id_hash = None;
        assert_eq!(
            value.canonical_key().unwrap(),
            "station:station-1:group-key:group-key-1"
        );
    }

    #[test]
    fn canonical_refs_sort_by_utf8_and_reject_duplicates() {
        let mut first = group_ref();
        first.group_binding_id = Some("b".into());
        let mut second = group_ref();
        second.group_binding_id = Some("a".into());
        assert_eq!(
            canonicalize_group_refs(&[first.clone(), second.clone()]).unwrap(),
            vec![
                "station:station-1:binding:a".to_string(),
                "station:station-1:binding:b".to_string()
            ]
        );
        assert!(matches!(
            canonicalize_group_refs(&[first.clone(), first]),
            Err(PricingGroupMonitoringInputError::DuplicateGroupRef(_))
        ));
    }

    #[test]
    fn representative_monitor_does_not_choose_later_success() {
        let summary = reduce_pricing_group_monitor_summary(PricingGroupMonitorReducerInput {
            group_ref: group_ref(),
            match_kind: MatchKind::ExactBinding,
            resolution_state: ResolutionState::Resolved,
            keys: vec![StationKeySnapshot {
                id: "key-1".into(),
                priority: 1,
                created_at_ms: 1,
                group_binding_id: Some("binding-1".into()),
                group_id_hash: None,
                enabled: true,
                credentialed: true,
            }],
            monitors: vec![
                MonitorSnapshot {
                    id: "monitor-1".into(),
                    station_id: "station-1".into(),
                    created_at_ms: 1,
                    target_type: "station_key".into(),
                    station_key_id: Some("key-1".into()),
                    enabled: true,
                },
                MonitorSnapshot {
                    id: "monitor-2".into(),
                    station_id: "station-1".into(),
                    created_at_ms: 2,
                    target_type: "station_key".into(),
                    station_key_id: Some("key-1".into()),
                    enabled: true,
                },
            ],
            target_results: vec![TargetResultSnapshot {
                id: "result-2".into(),
                monitor_id: "monitor-2".into(),
                station_key_id: Some("key-1".into()),
                terminal_outcome: LatestOutcome::Available,
                failure_kind: None,
                terminal_reason: None,
                finished_at_ms: Some(2),
                latency_ms: Some(10),
            }],
            running: Vec::new(),
            generated_at_ms: 3,
        });
        assert_eq!(
            summary.representative_monitor_id.as_deref(),
            Some("monitor-1")
        );
        assert_eq!(summary.display_state, DisplayState::Untested);
    }

    #[test]
    fn running_overlays_without_replacing_latest_terminal_result() {
        let mut input = PricingGroupMonitorReducerInput {
            group_ref: group_ref(),
            match_kind: MatchKind::ExactBinding,
            resolution_state: ResolutionState::Resolved,
            keys: vec![StationKeySnapshot {
                id: "key-1".into(),
                priority: 1,
                created_at_ms: 1,
                group_binding_id: Some("binding-1".into()),
                group_id_hash: None,
                enabled: true,
                credentialed: true,
            }],
            monitors: vec![MonitorSnapshot {
                id: "monitor-1".into(),
                station_id: "station-1".into(),
                created_at_ms: 1,
                target_type: "station_key".into(),
                station_key_id: Some("key-1".into()),
                enabled: true,
            }],
            target_results: vec![TargetResultSnapshot {
                id: "result-1".into(),
                monitor_id: "monitor-1".into(),
                station_key_id: Some("key-1".into()),
                terminal_outcome: LatestOutcome::Available,
                failure_kind: None,
                terminal_reason: None,
                finished_at_ms: Some(2),
                latency_ms: Some(10),
            }],
            running: Vec::new(),
            generated_at_ms: 3,
        };
        input.running.push(RunningSnapshot {
            monitor_id: "monitor-1".into(),
            station_key_id: Some("key-1".into()),
        });
        let summary = reduce_pricing_group_monitor_summary(input);
        assert!(summary.running);
        assert_eq!(summary.latest_outcome, LatestOutcome::Available);
        assert_eq!(summary.display_state, DisplayState::Running);
    }
}
