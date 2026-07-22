use std::collections::{HashMap, HashSet};

use crate::{
    models::{
        group_facts::{
            GroupRateRecord, StationGroupBinding, BINDING_KIND_STATION_GROUP,
            BINDING_STATUS_DISABLED, BINDING_STATUS_MANUAL_LEGACY,
        },
        shared_capabilities::StationGroupOption,
    },
    services::group_categories::{infer_group_category, normalize_group_category},
};

pub fn station_group_options_from_facts(
    bindings: Vec<StationGroupBinding>,
    rates: Vec<GroupRateRecord>,
) -> Vec<StationGroupOption> {
    let latest_rates = latest_rates_by_binding_or_hash(rates);
    let mut seen_values = HashSet::new();
    let mut options = bindings
        .into_iter()
        .filter(is_selectable_station_group_binding)
        .filter_map(|binding| {
            let rate = latest_rates
                .get(&rate_key_for_binding_id(&binding.id))
                .or_else(|| latest_rates.get(&rate_key_for_group_hash(&binding.group_key_hash)));
            let value = format!("binding:{}", binding.id);
            if !seen_values.insert(value.clone()) {
                return None;
            }
            let group_id_hash = binding
                .group_id_hash
                .clone()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| Some(binding.group_key_hash.clone()));
            let rate_multiplier = binding
                .effective_rate_multiplier
                .or(binding.default_rate_multiplier)
                .or_else(|| rate.and_then(|record| record.effective_rate_multiplier))
                .or_else(|| rate.and_then(|record| record.default_rate_multiplier));
            let rate_source = binding
                .rate_source
                .clone()
                .or_else(|| rate.map(|record| record.source.clone()));
            let selectable_for_remote_key = binding
                .group_id_hash
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty());
            let inferred_group_category = normalize_group_category(
                binding
                    .inferred_group_category
                    .as_deref()
                    .or_else(|| rate.and_then(|record| record.inferred_group_category.as_deref())),
            )
            .unwrap_or_else(|| {
                infer_group_category(
                    &binding.group_name,
                    rate.and_then(|record| record.raw_json_redacted.as_ref())
                        .or(binding.raw_json_redacted.as_ref()),
                )
            });
            let group_category_override =
                normalize_group_category(binding.group_category_override.as_deref());
            let effective_group_category = group_category_override
                .clone()
                .unwrap_or_else(|| inferred_group_category.clone());

            Some(StationGroupOption {
                value,
                group_binding_id: Some(binding.id),
                group_id_hash,
                group_name: binding.group_name,
                rate_multiplier,
                inferred_group_category: Some(inferred_group_category),
                group_category_override,
                effective_group_category,
                rate_source,
                selectable_for_remote_key,
            })
        })
        .collect::<Vec<_>>();

    options.sort_by(|left, right| {
        left.group_name
            .to_lowercase()
            .cmp(&right.group_name.to_lowercase())
            .then_with(|| left.value.cmp(&right.value))
    });
    options
}

fn latest_rates_by_binding_or_hash(
    rates: Vec<GroupRateRecord>,
) -> HashMap<String, GroupRateRecord> {
    let mut latest = HashMap::new();
    for rate in rates
        .into_iter()
        .filter(|rate| rate.binding_kind == BINDING_KIND_STATION_GROUP)
    {
        if let Some(group_binding_id) = rate.group_binding_id.as_deref() {
            latest
                .entry(rate_key_for_binding_id(group_binding_id))
                .or_insert_with(|| rate.clone());
        }
        latest
            .entry(rate_key_for_group_hash(&rate.group_key_hash))
            .or_insert(rate);
    }
    latest
}

fn is_selectable_station_group_binding(binding: &StationGroupBinding) -> bool {
    binding.binding_kind == BINDING_KIND_STATION_GROUP
        && binding.binding_status != BINDING_STATUS_DISABLED
        && binding.binding_status != BINDING_STATUS_MANUAL_LEGACY
        && binding.rate_source.as_deref() != Some("legacy_key_group")
}

fn rate_key_for_binding_id(id: &str) -> String {
    format!("binding:{id}")
}

fn rate_key_for_group_hash(group_key_hash: &str) -> String {
    format!("group:{group_key_hash}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::group_facts::{
        GroupRateRecord, StationGroupBinding, BINDING_KIND_STATION_GROUP, BINDING_STATUS_AVAILABLE,
    };

    #[test]
    fn station_group_options_prefer_binding_identity() {
        let options = station_group_options_from_facts(
            vec![binding(
                "binding-1",
                "hash-from-binding",
                Some("remote-group"),
            )],
            vec![rate("rate-1", Some("binding-1"), "hash-from-rate")],
        );

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].value, "binding:binding-1");
        assert_eq!(options[0].group_binding_id.as_deref(), Some("binding-1"));
        assert_eq!(options[0].group_id_hash.as_deref(), Some("remote-group"));
        assert_eq!(options[0].rate_multiplier, Some(0.8));
        assert_eq!(options[0].rate_source.as_deref(), Some("binding_api"));
        assert!(options[0].selectable_for_remote_key);
    }

    #[test]
    fn station_group_options_omit_legacy_key_group_rate_source() {
        let mut legacy_binding = binding("binding-legacy", "hash-legacy", Some("remote-legacy"));
        legacy_binding.rate_source = Some("legacy_key_group".to_string());

        let options = station_group_options_from_facts(vec![legacy_binding], Vec::new());

        assert!(options.is_empty());
    }

    fn binding(id: &str, group_key_hash: &str, group_id_hash: Option<&str>) -> StationGroupBinding {
        StationGroupBinding {
            id: id.to_string(),
            station_id: "station-1".to_string(),
            station_key_id: None,
            binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
            parent_group_binding_id: None,
            group_key_hash: group_key_hash.to_string(),
            group_id_hash: group_id_hash.map(ToString::to_string),
            group_name: "Default".to_string(),
            binding_status: BINDING_STATUS_AVAILABLE.to_string(),
            default_rate_multiplier: Some(1.0),
            user_rate_multiplier: None,
            effective_rate_multiplier: Some(0.8),
            rate_source: Some("binding_api".to_string()),
            confidence: 1.0,
            last_seen_at: Some("1000".to_string()),
            last_checked_at: Some("1000".to_string()),
            last_rate_changed_at: None,
            inferred_group_category: Some("gpt".to_string()),
            group_category_override: None,
            raw_json_redacted: None,
            created_at: "1000".to_string(),
            updated_at: "1000".to_string(),
        }
    }

    fn rate(id: &str, group_binding_id: Option<&str>, group_key_hash: &str) -> GroupRateRecord {
        GroupRateRecord {
            id: id.to_string(),
            station_id: "station-1".to_string(),
            station_key_id: None,
            group_binding_id: group_binding_id.map(ToString::to_string),
            binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
            group_key_hash: group_key_hash.to_string(),
            group_name: "Default".to_string(),
            default_rate_multiplier: Some(1.2),
            user_rate_multiplier: None,
            effective_rate_multiplier: Some(1.1),
            source: "rate_api".to_string(),
            confidence: 1.0,
            inferred_group_category: Some("gpt".to_string()),
            raw_json_redacted: None,
            checked_at: "1000".to_string(),
            created_at: "1000".to_string(),
        }
    }
}
