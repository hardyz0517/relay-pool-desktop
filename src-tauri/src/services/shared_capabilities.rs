use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::{
    models::{
        channel_monitors::ChannelMonitor,
        group_facts::{
            GroupRateRecord, StationGroupBinding, UpdateStationKeyGroupBindingInput,
            BINDING_KIND_STATION_GROUP, BINDING_STATUS_DISABLED, BINDING_STATUS_MANUAL_LEGACY,
        },
        routing::{StationKeyCapabilities, UpdateStationKeyCapabilitiesInput},
        shared_capabilities::{
            ChannelMonitorRunsLoadStatus, ChannelMonitorSummary, SaveStationKeyMode,
            SaveStationKeyWithDefaultsInput, SaveStationKeyWithDefaultsResult, StationGroupOption,
            StationKeyGroupSelectionKind,
        },
        station_keys::{CreateStationKeyInput, StationKey, UpdateStationKeyInput},
    },
    services::{
        database::{self, AppDatabase},
        group_categories::{infer_group_category, normalize_group_category},
    },
};

pub fn default_station_key_capabilities_input(
    station_key_id: String,
) -> UpdateStationKeyCapabilitiesInput {
    UpdateStationKeyCapabilitiesInput {
        station_key_id,
        supports_chat_completions: true,
        supports_responses: true,
        supports_embeddings: true,
        supports_stream: true,
        supports_tools: true,
        supports_vision: true,
        supports_reasoning: true,
        model_allowlist: Vec::new(),
        model_blocklist: Vec::new(),
        preferred_models: Vec::new(),
        only_use_as_backup: false,
        routing_tags: Vec::new(),
    }
}

pub fn save_station_key_with_defaults_in_connection(
    connection: &Connection,
    data_key: &[u8; 32],
    input: SaveStationKeyWithDefaultsInput,
) -> Result<SaveStationKeyWithDefaultsResult, String> {
    validate_group_selection(&input)?;

    let mode = input.mode.clone();
    let mut station_key = match mode {
        SaveStationKeyMode::Create => create_station_key(connection, data_key, &input)?,
        SaveStationKeyMode::Update => update_station_key(connection, data_key, &input)?,
    };

    station_key = match input.group_selection.kind {
        StationKeyGroupSelectionKind::Keep => station_key,
        StationKeyGroupSelectionKind::Clear => {
            database::clear_station_key_group_binding_in_connection(connection, &station_key.id)?
        }
        StationKeyGroupSelectionKind::Set => {
            let group_binding_id = input
                .group_selection
                .group_binding_id
                .clone()
                .expect("validated group binding id");
            database::update_station_key_group_binding_in_connection(
                connection,
                UpdateStationKeyGroupBindingInput {
                    station_key_id: station_key.id.clone(),
                    group_binding_id,
                },
            )?
        }
    };

    let capabilities = match (mode, input.capabilities) {
        (SaveStationKeyMode::Create, capabilities) => {
            persist_capabilities(connection, station_key.id.clone(), capabilities)?
        }
        (SaveStationKeyMode::Update, Some(capabilities)) => {
            persist_capabilities(connection, station_key.id.clone(), Some(capabilities))?
        }
        (SaveStationKeyMode::Update, None) => {
            database::station_key_capabilities_by_id(connection, &station_key.id)?
        }
    };

    Ok(SaveStationKeyWithDefaultsResult {
        station_key,
        capabilities,
        message: "Station Key 已保存".to_string(),
    })
}

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

pub fn channel_monitor_summaries_from_database(
    database: &AppDatabase,
    monitors: Vec<ChannelMonitor>,
    run_since: Option<&str>,
    run_limit: Option<usize>,
) -> Vec<ChannelMonitorSummary> {
    monitors
        .into_iter()
        .map(|monitor| {
            match database.list_channel_monitor_runs_for_summary(
                monitor.id.clone(),
                run_since,
                run_limit,
            ) {
                Ok(runs) => {
                    let recent_runs = runs;
                    let latest_run = recent_runs.first().cloned();
                    ChannelMonitorSummary {
                        monitor,
                        recent_runs,
                        runs_load_status: ChannelMonitorRunsLoadStatus::Ok,
                        latest_run,
                    }
                }
                Err(_) => ChannelMonitorSummary {
                    monitor,
                    recent_runs: Vec::new(),
                    runs_load_status: ChannelMonitorRunsLoadStatus::Failed,
                    latest_run: None,
                },
            }
        })
        .collect()
}

#[allow(dead_code)]
fn _keep_capability_type_referenced(_: Option<StationKeyCapabilities>) {}

fn validate_group_selection(input: &SaveStationKeyWithDefaultsInput) -> Result<(), String> {
    match input.group_selection.kind {
        StationKeyGroupSelectionKind::Keep if input.mode == SaveStationKeyMode::Create => {
            Err("创建 Station Key 时不能保留现有分组".to_string())
        }
        StationKeyGroupSelectionKind::Set => {
            let has_binding = input
                .group_selection
                .group_binding_id
                .as_deref()
                .is_some_and(|id| !id.trim().is_empty());
            if has_binding {
                Ok(())
            } else {
                Err("请选择要绑定的分组".to_string())
            }
        }
        StationKeyGroupSelectionKind::Keep | StationKeyGroupSelectionKind::Clear => Ok(()),
    }
}

fn create_station_key(
    connection: &Connection,
    data_key: &[u8; 32],
    input: &SaveStationKeyWithDefaultsInput,
) -> Result<StationKey, String> {
    let api_key = input
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|api_key| !api_key.is_empty())
        .ok_or_else(|| "创建 Station Key 时 API Key 不能为空".to_string())?;
    let created = database::create_station_key_in_connection_with_data_key(
        connection,
        CreateStationKeyInput {
            station_id: input.station_id.clone(),
            name: input.name.clone(),
            api_key: api_key.to_string(),
            enabled: input.enabled,
            priority: input.priority,
            max_concurrency: None,
            load_factor: None,
            schedulable: None,
            group_name: None,
            tier_label: input.tier_label.clone(),
            group_binding_id: None,
            group_id_hash: None,
            rate_multiplier: None,
            manual_rate_multiplier: None,
            rate_source: None,
            balance_scope: input.balance_scope.clone(),
            note: input.note.clone(),
        },
        Some(data_key),
    )?;

    if let Some(status) = input.status.clone() {
        database::update_station_key_in_connection_with_data_key(
            connection,
            UpdateStationKeyInput {
                id: created.id.clone(),
                station_id: created.station_id.clone(),
                name: created.name.clone(),
                api_key: None,
                enabled: created.enabled,
                priority: created.priority,
                max_concurrency: created.max_concurrency,
                load_factor: created.load_factor,
                schedulable: created.schedulable,
                group_name: created.group_name.clone(),
                tier_label: created.tier_label.clone(),
                group_binding_id: created.group_binding_id.clone(),
                group_id_hash: created.group_id_hash.clone(),
                rate_multiplier: created.rate_multiplier,
                manual_rate_multiplier: None,
                rate_source: created.rate_source.clone(),
                balance_scope: created.balance_scope.clone(),
                status,
                note: created.note.clone(),
            },
            Some(data_key),
        )
    } else {
        Ok(created)
    }
}

fn update_station_key(
    connection: &Connection,
    data_key: &[u8; 32],
    input: &SaveStationKeyWithDefaultsInput,
) -> Result<StationKey, String> {
    let id = input
        .id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "更新 Station Key 时 ID 不能为空".to_string())?;
    let existing = database::station_key_by_id(connection, id)?;

    database::update_station_key_in_connection_with_data_key(
        connection,
        UpdateStationKeyInput {
            id: existing.id.clone(),
            station_id: input.station_id.clone(),
            name: input.name.clone(),
            api_key: input.api_key.clone(),
            enabled: input.enabled,
            priority: input.priority.unwrap_or(existing.priority),
            max_concurrency: existing.max_concurrency,
            load_factor: existing.load_factor,
            schedulable: existing.schedulable,
            group_name: existing.group_name.clone(),
            tier_label: input.tier_label.clone(),
            group_binding_id: existing.group_binding_id.clone(),
            group_id_hash: existing.group_id_hash.clone(),
            rate_multiplier: existing.rate_multiplier,
            manual_rate_multiplier: None,
            rate_source: existing.rate_source.clone(),
            balance_scope: input.balance_scope.clone().or(existing.balance_scope),
            status: input.status.clone().unwrap_or(existing.status),
            note: input.note.clone(),
        },
        Some(data_key),
    )
}

fn persist_capabilities(
    connection: &Connection,
    station_key_id: String,
    capabilities: Option<UpdateStationKeyCapabilitiesInput>,
) -> Result<StationKeyCapabilities, String> {
    let mut input = capabilities
        .unwrap_or_else(|| default_station_key_capabilities_input(station_key_id.clone()));
    input.station_key_id = station_key_id;
    database::update_station_key_capabilities_in_connection(connection, input)
        .map_err(|error| format!("保存 Key 默认能力失败: {error}"))
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
    fn default_capability_flags_are_all_enabled() {
        let input = default_station_key_capabilities_input("key-1".to_string());

        assert!(input.supports_chat_completions);
        assert!(input.supports_responses);
        assert!(input.supports_embeddings);
        assert!(input.supports_stream);
        assert!(input.supports_tools);
        assert!(input.supports_vision);
        assert!(input.supports_reasoning);
        assert!(!input.only_use_as_backup);
        assert!(input.model_allowlist.is_empty());
        assert!(input.model_blocklist.is_empty());
        assert!(input.preferred_models.is_empty());
        assert!(input.routing_tags.is_empty());
    }

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
