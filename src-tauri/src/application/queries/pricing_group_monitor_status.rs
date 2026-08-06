use std::{
    collections::BTreeSet,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    application::error::ApplicationError,
    models::pricing_group_monitoring::{
        CanonicalGroupRef, MatchKind, PRICING_GROUP_MONITORING_SCHEMA_VERSION,
        PricingGroupMonitorReducerInput, PricingGroupMonitorStatusInput,
        PricingGroupMonitorStatusWorkspace, PricingGroupMonitorSummary, ResolutionState,
        canonicalize_group_refs, group_refs_hash,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::monitoring::group_status_repository::{
            GroupStatusRows, MatchedKeyRow, PricingGroupMonitorStatusRepository,
        },
    },
};

#[derive(Clone)]
pub(crate) struct PricingGroupMonitorStatusQuery {
    runtime: PersistenceHandle,
    repository: PricingGroupMonitorStatusRepository,
}

impl PricingGroupMonitorStatusQuery {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            repository: PricingGroupMonitorStatusRepository,
        }
    }

    pub(crate) async fn load(
        &self,
        input: PricingGroupMonitorStatusInput,
    ) -> Result<PricingGroupMonitorStatusWorkspace, ApplicationError> {
        if input.schema_version != PRICING_GROUP_MONITORING_SCHEMA_VERSION
            || input.groups.len() > crate::models::pricing_group_monitoring::MAX_PRICING_GROUP_REFS
            || input.group_refs_hash.trim().is_empty()
        {
            return Err(ApplicationError::ConstraintViolation);
        }
        let refs = canonicalize_group_refs(&input.groups)
            .map_err(|_| ApplicationError::ConstraintViolation)?;
        let expected_hash =
            group_refs_hash(&input.groups).map_err(|_| ApplicationError::ConstraintViolation)?;
        if expected_hash != input.group_refs_hash {
            return Err(ApplicationError::ConstraintViolation);
        }

        let mut read = self.runtime.begin_read().await?;
        let rows = self.repository.load(&mut read, &input.groups).await?;
        let generated_at_ms = now_ms();
        let items = input
            .groups
            .iter()
            .map(|group| summary_for_group(group, &refs, &rows, generated_at_ms))
            .collect::<Vec<_>>();
        Ok(PricingGroupMonitorStatusWorkspace {
            schema_version: PRICING_GROUP_MONITORING_SCHEMA_VERSION,
            generated_at_ms,
            group_refs_hash: input.group_refs_hash,
            requested_group_count: input.groups.len() as u32,
            returned_group_count: items.len() as u32,
            omitted_group_count: 0,
            items,
        })
    }
}

fn summary_for_group(
    group: &CanonicalGroupRef,
    refs: &[String],
    rows: &GroupStatusRows,
    generated_at_ms: i64,
) -> PricingGroupMonitorSummary {
    let canonical_key = group.canonical_key().ok();
    let matched = canonical_key
        .as_deref()
        .map(|key| {
            rows.keys
                .iter()
                .filter(|row| row.group_ref_key == key)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let resolved = canonical_key.as_deref().is_some_and(|key| {
        rows.resolutions
            .iter()
            .any(|resolution| resolution.group_ref_key == key)
    });
    let resolution_state = if resolved {
        ResolutionState::Resolved
    } else {
        ResolutionState::Unresolved
    };
    let match_kind = match_kind_for(group, &matched, &rows.resolutions, canonical_key.as_deref());
    let keys = matched
        .iter()
        .map(|row| row.key.clone())
        .collect::<Vec<_>>();
    let station_monitors = rows
        .monitors
        .iter()
        .filter(|monitor| {
            // Repository scope is station-wide; keep the reducer input bounded to this station.
            monitor.station_id == group.station_id
                && (keys.iter().any(|key| {
                    monitor.target_type == "station"
                        || monitor.station_key_id.as_deref() == Some(key.id.as_str())
                }) || (keys.is_empty() && monitor.target_type == "station"))
        })
        .cloned()
        .collect::<Vec<_>>();
    let summary = crate::models::pricing_group_monitoring::reduce_pricing_group_monitor_summary(
        PricingGroupMonitorReducerInput {
            group_ref: group.clone(),
            match_kind,
            resolution_state,
            keys,
            monitors: station_monitors,
            target_results: rows.target_results.clone(),
            running: rows.running.clone(),
            generated_at_ms,
        },
    );
    if refs.is_empty() {
        return summary;
    }
    summary
}

fn match_kind_for(
    _group: &CanonicalGroupRef,
    matched: &[&MatchedKeyRow],
    resolutions: &[crate::persistence::stores::monitoring::group_status_repository::ResolvedGroupRefRow],
    canonical_key: Option<&str>,
) -> MatchKind {
    let kinds = matched
        .iter()
        .map(|row| row.match_kind.as_str())
        .collect::<BTreeSet<_>>();
    if kinds.contains("exact_binding") {
        MatchKind::ExactBinding
    } else if kinds.contains("parent_binding") {
        MatchKind::ParentBinding
    } else if kinds.contains("group_id_hash") {
        MatchKind::GroupIdHash
    } else if kinds.contains("group_key_hash") {
        MatchKind::GroupKeyHash
    } else if let Some(resolution) = canonical_key.and_then(|key| {
        resolutions
            .iter()
            .find(|resolution| resolution.group_ref_key == key)
    }) {
        match resolution.match_kind.as_str() {
            "exact_binding" => MatchKind::ExactBinding,
            "parent_binding" => MatchKind::ParentBinding,
            "group_id_hash" => MatchKind::GroupIdHash,
            "group_key_hash" => MatchKind::GroupKeyHash,
            _ => MatchKind::Unresolved,
        }
    } else {
        MatchKind::Unresolved
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::pricing_group_monitoring::DisplayState;

    #[test]
    fn unresolved_group_does_not_become_no_key() {
        let group = CanonicalGroupRef {
            station_id: "station-1".into(),
            group_binding_id: None,
            group_id_hash: None,
            group_key_hash: "".into(),
        };
        let summary = summary_for_group(
            &group,
            &[],
            &GroupStatusRows {
                resolutions: Vec::new(),
                keys: Vec::new(),
                monitors: Vec::new(),
                target_results: Vec::new(),
                running: Vec::new(),
            },
            1,
        );
        assert_eq!(summary.display_state, DisplayState::Unresolved);
    }
}
