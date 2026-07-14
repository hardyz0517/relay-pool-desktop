use sha2::{Digest, Sha256};

use crate::{
    models::{
        group_facts::{GroupRateRecord, StationGroupBinding},
        remote_keys::{
            BindRemoteStationKeyInput, CreateLocalStationKeyFromRemoteResult,
            CreateRemoteStationKeyInput, CreateRemoteStationKeyResult, RemoteKeyCapability,
            RemoteKeyScanResult, RemoteStationKey,
        },
        station_keys::{CreateStationKeyInput, StationKey, UpdateStationKeyInput},
    },
    services::{
        collectors::adapters::{self, CreatedRemoteKey},
        database::{now_millis_for_services, AppDatabase},
    },
};

pub fn remote_key_capability(
    database: &AppDatabase,
    station_id: String,
) -> Result<RemoteKeyCapability, String> {
    let station = database.station_for_collector(&station_id)?;
    let station_type = station.station_type.trim().to_string();
    match station_type.as_str() {
        "sub2api" => adapters::sub2api::remote_key_capability(&station),
        "newapi" => adapters::newapi::remote_key_capability(&station),
        _ => Ok(RemoteKeyCapability {
            station_id,
            station_type: station_type.clone(),
            can_list_remote_keys: false,
            can_create_remote_key: false,
            can_read_groups: false,
            requires_manual_session: false,
            unsupported_reason: Some(format!(
                "暂不支持 {station_type} 类型中转站的远端 Key 管理。"
            )),
        }),
    }
}

pub fn list_remote_keys(
    database: &AppDatabase,
    station_id: String,
) -> Result<Vec<RemoteStationKey>, String> {
    database.list_remote_station_keys(station_id)
}

pub fn scan_remote_keys(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: String,
) -> Result<RemoteKeyScanResult, String> {
    let capability = remote_key_capability(database, station_id.clone())?;
    if !capability.can_list_remote_keys {
        return Ok(RemoteKeyScanResult {
            station_id,
            capability: capability.clone(),
            keys: database.list_remote_station_keys(capability.station_id)?,
            synced_station_key_ids: Vec::new(),
            message: capability
                .unsupported_reason
                .clone()
                .unwrap_or_else(|| "该中转站暂不支持远端 Key 扫描。".to_string()),
        });
    }

    let discovered_keys =
        scan_remote_keys_with_adapter(database, data_key, &station_id, &capability.station_type)?;
    let discovered_keys =
        enrich_remote_key_discoveries(database, data_key, &station_id, discovered_keys)?;
    let keys = database.replace_remote_station_keys(station_id.clone(), discovered_keys)?;
    let synced_station_key_ids = keys
        .iter()
        .filter_map(|key| key.matched_station_key_id.clone())
        .collect::<Vec<_>>();

    Ok(RemoteKeyScanResult {
        station_id,
        capability,
        message: format!("远端 Key 扫描完成，已同步 {} 条发现。", keys.len()),
        keys,
        synced_station_key_ids,
    })
}

pub fn create_remote_key(
    database: &AppDatabase,
    data_key: &[u8; 32],
    input: CreateRemoteStationKeyInput,
) -> Result<CreateRemoteStationKeyResult, String> {
    let capability = remote_key_capability(database, input.station_id.clone())?;
    if !capability.can_create_remote_key {
        return Err(capability
            .unsupported_reason
            .unwrap_or_else(|| "该中转站暂不支持创建远端 Key。".to_string()));
    }

    let CreatedRemoteKey {
        remote_key,
        full_key_once,
        message,
    } = create_remote_key_with_adapter(database, data_key, input, &capability.station_type)?;
    let full_key = full_key_once
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "远端站点未返回完整 Key，无法保存到本地 Station Key。".to_string())?;
    save_created_remote_key(
        database,
        data_key,
        remote_key,
        full_key,
        message,
        capability.station_type != "newapi",
    )
}

pub fn create_local_key_from_remote_key(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: String,
    remote_key_id: String,
) -> Result<CreateLocalStationKeyFromRemoteResult, String> {
    let capability = remote_key_capability(database, station_id.clone())?;
    if !capability.can_list_remote_keys {
        return Err(capability
            .unsupported_reason
            .unwrap_or_else(|| "该中转站暂不支持远端 Key 扫描。".to_string()));
    }

    let (remote_key, full_key) = remote_key_full_secret_with_adapter(
        database,
        data_key,
        &station_id,
        &remote_key_id,
        &capability.station_type,
    )?;
    let result = save_created_remote_key(
        database,
        data_key,
        remote_key,
        full_key,
        "远端 Key 已同步为本地 Station Key。".to_string(),
        false,
    )?;
    Ok(CreateLocalStationKeyFromRemoteResult {
        remote_key: result.remote_key,
        station_key: result.station_key,
        message: "远端 Key 已保存为本地 Station Key。".to_string(),
    })
}

pub fn bind_remote_key(
    database: &AppDatabase,
    input: BindRemoteStationKeyInput,
) -> Result<Vec<RemoteStationKey>, String> {
    database.bind_remote_station_key(input.remote_key_id, input.station_key_id)
}

fn scan_remote_keys_with_adapter(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    station_type: &str,
) -> Result<Vec<RemoteStationKey>, String> {
    match station_type {
        "sub2api" => adapters::sub2api::scan_remote_keys(database, data_key, station_id),
        "newapi" => adapters::newapi::scan_remote_keys(database, data_key, station_id),
        _ => Err(format!(
            "暂不支持 {station_type} 类型中转站的远端 Key 扫描。"
        )),
    }
}

fn create_remote_key_with_adapter(
    database: &AppDatabase,
    data_key: &[u8; 32],
    input: CreateRemoteStationKeyInput,
    station_type: &str,
) -> Result<CreatedRemoteKey, String> {
    match station_type {
        "sub2api" => adapters::sub2api::create_remote_key(database, data_key, input),
        "newapi" => adapters::newapi::create_remote_key(database, data_key, input),
        _ => Err(format!(
            "暂不支持 {station_type} 类型中转站的远端 Key 创建。"
        )),
    }
}

fn remote_key_full_secret_with_adapter(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    remote_key_id: &str,
    station_type: &str,
) -> Result<(RemoteStationKey, String), String> {
    match station_type {
        "sub2api" => adapters::sub2api::scan_remote_key_full_secret(
            database,
            data_key,
            station_id,
            remote_key_id,
        ),
        "newapi" => adapters::newapi::scan_remote_key_full_secret(
            database,
            data_key,
            station_id,
            remote_key_id,
        ),
        _ => Err(format!(
            "暂不支持 {station_type} 类型中转站从远端发现同步本地 Key。"
        )),
    }
}

fn save_created_remote_key(
    database: &AppDatabase,
    data_key: &[u8; 32],
    remote_key: RemoteStationKey,
    full_key: String,
    adapter_message: String,
    expose_full_key_once: bool,
) -> Result<CreateRemoteStationKeyResult, String> {
    if full_key.trim().is_empty() {
        return Err("远端站点未返回完整 Key，无法保存到本地 Station Key。".to_string());
    }

    let station_id = remote_key.station_id.clone();
    let mut enriched_keys =
        enrich_remote_key_discoveries(database, data_key, &station_id, vec![remote_key])?;
    let remote_key = enriched_keys
        .pop()
        .ok_or_else(|| "远端 Key 创建结果为空，无法同步。".to_string())?;

    if let Some(station_key_id) = remote_key.matched_station_key_id.as_deref() {
        let station_key = database
            .list_station_keys(remote_key.station_id.clone())?
            .into_iter()
            .find(|key| key.id == station_key_id)
            .ok_or_else(|| "已匹配的本地 Station Key 不存在，无法同步远端 Key。".to_string())?;
        let remote_key = save_remote_key_discovery(database, remote_key)?;
        return Ok(CreateRemoteStationKeyResult {
            remote_key,
            station_key,
            full_key_once: expose_full_key_once.then(|| full_key.clone()),
            message: if adapter_message.trim().is_empty() {
                "远端 Key 已创建，并已关联到已有本地 Station Key。".to_string()
            } else {
                format!("{adapter_message} 已关联到已有本地 Station Key。")
            },
        });
    }

    let group_binding_id = find_group_binding_for_remote_key(database, &remote_key)?
        .map(|binding| binding.id)
        .filter(|id| !id.trim().is_empty());
    let station_key = database.create_station_key_with_data_key(
        CreateStationKeyInput {
            station_id: remote_key.station_id.clone(),
            name: remote_key
                .remote_key_name
                .clone()
                .unwrap_or_else(|| "远端 Key".to_string()),
            api_key: full_key.trim().to_string(),
            enabled: true,
            priority: None,
            max_concurrency: None,
            load_factor: None,
            schedulable: None,
            group_name: remote_key.group_name.clone(),
            tier_label: remote_key.tier_label.clone(),
            group_binding_id,
            group_id_hash: remote_key.group_id_hash.clone(),
            rate_multiplier: remote_key.rate_multiplier,
            manual_rate_multiplier: None,
            rate_source: remote_key.rate_source.clone(),
            balance_scope: None,
            note: Some("由远端站点创建并同步。".to_string()),
        },
        data_key,
    )?;
    let mut synced_remote_key = remote_key;
    synced_remote_key.matched_station_key_id = Some(station_key.id.clone());
    synced_remote_key.match_status = crate::models::remote_keys::RemoteKeyMatchStatus::Matched;
    synced_remote_key.match_confidence = 1.0;
    synced_remote_key.collected_at = now_millis_for_services().to_string();

    let remote_key = save_remote_key_discovery(database, synced_remote_key)?;

    Ok(CreateRemoteStationKeyResult {
        remote_key,
        station_key,
        full_key_once: expose_full_key_once.then_some(full_key),
        message: if adapter_message.trim().is_empty() {
            "远端 Key 已创建，并已保存为启用的本地 Station Key。".to_string()
        } else {
            format!("{adapter_message} 已保存为启用的本地 Station Key。")
        },
    })
}

fn enrich_remote_key_discoveries(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    keys: Vec<RemoteStationKey>,
) -> Result<Vec<RemoteStationKey>, String> {
    let group_bindings = database.list_station_group_bindings(station_id.to_string())?;
    let group_rates = database.list_group_rate_records(station_id.to_string())?;
    let local_candidates = local_station_key_candidates(database, data_key, station_id)?;

    let mut enriched = Vec::with_capacity(keys.len());
    for mut key in keys {
        apply_group_metadata(&mut key, &group_bindings, &group_rates);
        if let Some((local_key, confidence)) = best_local_key_match(&key, &local_candidates) {
            if confidence >= 0.8 {
                key.matched_station_key_id = Some(local_key.key.id.clone());
                key.match_confidence = confidence;
                key.match_status = if confidence >= 0.9 {
                    crate::models::remote_keys::RemoteKeyMatchStatus::Matched
                } else {
                    crate::models::remote_keys::RemoteKeyMatchStatus::Possible
                };
                sync_station_key_metadata(database, data_key, &local_key.key, &key)?;
            }
        }
        enriched.push(key);
    }

    Ok(enriched)
}

#[derive(Debug, Clone)]
struct LocalStationKeyCandidate {
    key: StationKey,
    full_key: Option<String>,
    fingerprint: Option<String>,
}

fn local_station_key_candidates(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
) -> Result<Vec<LocalStationKeyCandidate>, String> {
    let keys = database.list_station_keys(station_id.to_string())?;
    Ok(keys
        .into_iter()
        .map(|key| {
            let full_key = if key.api_key_present {
                database
                    .resolve_station_key_secret_with_data_key(data_key, &key.id)
                    .ok()
            } else {
                None
            };
            let fingerprint = full_key.as_deref().and_then(api_key_fingerprint);
            LocalStationKeyCandidate {
                key,
                full_key,
                fingerprint,
            }
        })
        .collect())
}

fn best_local_key_match<'a>(
    remote_key: &RemoteStationKey,
    local_candidates: &'a [LocalStationKeyCandidate],
) -> Option<(&'a LocalStationKeyCandidate, f64)> {
    local_candidates
        .iter()
        .map(|candidate| {
            let same_group = remote_key
                .group_id_hash
                .as_deref()
                .zip(candidate.key.group_id_hash.as_deref())
                .map(|(remote, local)| remote == local)
                .unwrap_or(false)
                || names_match(
                    remote_key.group_name.as_deref(),
                    candidate.key.group_name.as_deref(),
                );
            let same_name = names_match(
                remote_key.remote_key_name.as_deref(),
                Some(candidate.key.name.as_str()),
            );
            let confidence = remote_key_confidence(
                remote_key.api_key_fingerprint.as_deref(),
                candidate.fingerprint.as_deref(),
                remote_key.api_key_masked.as_deref(),
                candidate.full_key.as_deref(),
                same_group,
                same_name,
            );
            (candidate, confidence)
        })
        .max_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|(_, confidence)| *confidence > 0.0)
}

fn apply_group_metadata(
    remote_key: &mut RemoteStationKey,
    group_bindings: &[StationGroupBinding],
    group_rates: &[GroupRateRecord],
) {
    let Some(binding) = matching_group_binding(remote_key, group_bindings) else {
        return;
    };
    let latest_rate = latest_group_rate(binding, group_rates);

    remote_key.group_id_hash = Some(binding.group_key_hash.clone());
    remote_key.group_name = Some(binding.group_name.clone());
    if remote_key.rate_multiplier.is_none() {
        remote_key.rate_multiplier = latest_rate
            .and_then(effective_rate_from_record)
            .or_else(|| effective_rate_from_binding(binding));
    }
    if remote_key.rate_source.as_deref() == Some("sub2api_keys")
        || remote_key
            .rate_source
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        remote_key.rate_source = latest_rate
            .map(|rate| rate.source.clone())
            .or_else(|| binding.rate_source.clone())
            .or_else(|| Some("station_group_binding".to_string()));
    }
}

fn matching_group_binding<'a>(
    remote_key: &RemoteStationKey,
    group_bindings: &'a [StationGroupBinding],
) -> Option<&'a StationGroupBinding> {
    group_bindings
        .iter()
        .filter(|binding| {
            binding.binding_kind == "station_group" && binding.binding_status != "disabled"
        })
        .find(|binding| {
            remote_key
                .group_id_hash
                .as_deref()
                .map(|remote_group| {
                    remote_group == binding.group_key_hash
                        || binding.group_id_hash.as_deref() == Some(remote_group)
                })
                .unwrap_or(false)
                || names_match(
                    remote_key.group_name.as_deref(),
                    Some(binding.group_name.as_str()),
                )
        })
}

fn latest_group_rate<'a>(
    binding: &StationGroupBinding,
    group_rates: &'a [GroupRateRecord],
) -> Option<&'a GroupRateRecord> {
    group_rates.iter().find(|rate| {
        rate.binding_kind == "station_group"
            && (rate.group_binding_id.as_deref() == Some(binding.id.as_str())
                || rate.group_key_hash == binding.group_key_hash
                || normalized_text(&rate.group_name) == normalized_text(&binding.group_name))
    })
}

fn effective_rate_from_binding(binding: &StationGroupBinding) -> Option<f64> {
    binding
        .user_rate_multiplier
        .or(binding.effective_rate_multiplier)
        .or(binding.default_rate_multiplier)
}

fn effective_rate_from_record(record: &GroupRateRecord) -> Option<f64> {
    record
        .user_rate_multiplier
        .or(record.effective_rate_multiplier)
        .or(record.default_rate_multiplier)
}

fn sync_station_key_metadata(
    database: &AppDatabase,
    data_key: &[u8; 32],
    local_key: &StationKey,
    remote_key: &RemoteStationKey,
) -> Result<(), String> {
    let group_binding_id = find_group_binding_for_remote_key(database, remote_key)?
        .map(|binding| binding.id)
        .or_else(|| local_key.group_binding_id.clone());
    database.update_station_key_with_data_key(
        UpdateStationKeyInput {
            id: local_key.id.clone(),
            station_id: local_key.station_id.clone(),
            name: local_key.name.clone(),
            api_key: None,
            enabled: local_key.enabled,
            priority: local_key.priority,
            max_concurrency: local_key.max_concurrency,
            load_factor: local_key.load_factor,
            schedulable: local_key.schedulable,
            group_name: remote_key
                .group_name
                .clone()
                .or_else(|| local_key.group_name.clone()),
            tier_label: remote_key
                .tier_label
                .clone()
                .or_else(|| local_key.tier_label.clone()),
            group_binding_id,
            group_id_hash: remote_key
                .group_id_hash
                .clone()
                .or_else(|| local_key.group_id_hash.clone()),
            rate_multiplier: remote_key.rate_multiplier.or(local_key.rate_multiplier),
            manual_rate_multiplier: None,
            rate_source: remote_key
                .rate_source
                .clone()
                .or_else(|| local_key.rate_source.clone()),
            balance_scope: local_key.balance_scope.clone(),
            status: local_key.status.clone(),
            note: local_key.note.clone(),
        },
        data_key,
    )?;
    Ok(())
}

fn find_group_binding_for_remote_key(
    database: &AppDatabase,
    remote_key: &RemoteStationKey,
) -> Result<Option<StationGroupBinding>, String> {
    let bindings = database.list_station_group_bindings(remote_key.station_id.clone())?;
    Ok(matching_group_binding(remote_key, &bindings).cloned())
}

fn save_remote_key_discovery(
    database: &AppDatabase,
    remote_key: RemoteStationKey,
) -> Result<RemoteStationKey, String> {
    let mut remote_keys = database.list_remote_station_keys(remote_key.station_id.clone())?;
    remote_keys.retain(|key| key.id != remote_key.id);
    remote_keys.push(remote_key.clone());
    let saved_remote_keys =
        database.replace_remote_station_keys(remote_key.station_id.clone(), remote_keys)?;
    Ok(saved_remote_keys
        .into_iter()
        .find(|key| key.id == remote_key.id)
        .unwrap_or(remote_key))
}

fn names_match(left: Option<&str>, right: Option<&str>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => normalized_text(left) == normalized_text(right),
        _ => false,
    }
}

fn normalized_text(value: &str) -> String {
    value.trim().to_lowercase()
}

pub fn api_key_fingerprint(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(trimmed.as_bytes());
    Some(format!("{:x}", hasher.finalize()))
}

pub fn visible_mask_parts(masked: &str) -> Option<(String, String)> {
    let trimmed = masked.trim();
    let (prefix, suffix) = trimmed
        .split_once("****")
        .or_else(|| trimmed.split_once("..."))?;
    let prefix = prefix.trim().to_string();
    let suffix = suffix.trim().to_string();
    if prefix.len() < 3 || suffix.len() < 3 {
        return None;
    }
    Some((prefix, suffix))
}

pub fn masked_key_matches_full(masked: &str, full_key: &str) -> bool {
    visible_mask_parts(masked)
        .map(|(prefix, suffix)| full_key.starts_with(&prefix) && full_key.ends_with(&suffix))
        .unwrap_or(false)
}

pub fn remote_key_confidence(
    remote_fingerprint: Option<&str>,
    local_fingerprint: Option<&str>,
    remote_masked: Option<&str>,
    local_full_key: Option<&str>,
    same_group: bool,
    same_name: bool,
) -> f64 {
    if let Some(remote_fingerprint) = remote_fingerprint {
        return if Some(remote_fingerprint) == local_fingerprint {
            1.0
        } else {
            0.0
        };
    }
    if let (Some(masked), Some(full_key)) = (remote_masked, local_full_key) {
        if masked_key_matches_full(masked, full_key) {
            return if same_group || same_name { 0.92 } else { 0.82 };
        }
    }
    match (same_group, same_name) {
        (true, true) => 0.72,
        (true, false) | (false, true) => 0.55,
        (false, false) => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{
            credentials::UpdateStationSessionInput,
            group_facts::{
                InsertGroupRateRecordInput, UpsertStationGroupBindingInput,
                BINDING_KIND_STATION_GROUP, BINDING_STATUS_AVAILABLE,
            },
            station_keys::CreateStationKeyInput,
            station_keys::UpdateStationKeyInput,
            stations::CreateStationInput,
        },
        services::{database::AppDatabase, secrets::crypto::generate_data_key},
    };
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn fingerprints_identical_keys_consistently() {
        assert_eq!(api_key_fingerprint("sk-a"), api_key_fingerprint("sk-a"));
        assert_ne!(api_key_fingerprint("sk-a"), api_key_fingerprint("sk-b"));
        assert_eq!(api_key_fingerprint("   "), None);
    }

    #[test]
    fn masked_key_match_requires_visible_prefix_and_suffix() {
        assert!(masked_key_matches_full(
            "sk-live****cdef",
            "sk-live-123-cdef"
        ));
        assert!(masked_key_matches_full(
            "sk-live-...cdef",
            "sk-live-123-cdef"
        ));
        assert!(!masked_key_matches_full(
            "sk-live****zzzz",
            "sk-live-123-cdef"
        ));
        assert!(!masked_key_matches_full(
            "sk-live-...zzzz",
            "sk-live-123-cdef"
        ));
        assert!(!masked_key_matches_full("sk****ef", "sk-live-123-cdef"));
        assert!(!masked_key_matches_full("sk-...ef", "sk-live-123-cdef"));
    }

    #[test]
    fn confidence_separates_high_and_possible_matches() {
        let fp = api_key_fingerprint("sk-live-123-cdef");
        assert_eq!(
            remote_key_confidence(fp.as_deref(), fp.as_deref(), None, None, false, false),
            1.0
        );
        assert!(
            remote_key_confidence(
                None,
                None,
                Some("sk-live****cdef"),
                Some("sk-live-123-cdef"),
                true,
                false
            ) >= 0.9
        );
        assert!(remote_key_confidence(None, None, None, None, true, true) < 0.8);
    }

    #[test]
    fn discovered_remote_keys_are_enriched_and_matched_to_local_keys() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = test_station(&database, "sub2api");
        let local_key = database
            .list_station_keys(station.id.clone())
            .expect("station keys")
            .remove(0);
        database
            .update_station_key_with_data_key(
                UpdateStationKeyInput {
                    id: local_key.id.clone(),
                    station_id: station.id.clone(),
                    name: "Local Pro Key".to_string(),
                    api_key: Some("sk-live-secret-abcdef".to_string()),
                    enabled: true,
                    priority: local_key.priority,
                    max_concurrency: 3,
                    load_factor: None,
                    schedulable: true,
                    group_name: None,
                    tier_label: None,
                    group_binding_id: None,
                    group_id_hash: None,
                    rate_multiplier: None,
                    manual_rate_multiplier: None,
                    rate_source: None,
                    balance_scope: local_key.balance_scope,
                    status: local_key.status,
                    note: local_key.note,
                },
                &data_key,
            )
            .expect("update local key");
        let group = database
            .upsert_station_group_binding(UpsertStationGroupBindingInput {
                station_id: station.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: "collector-sub2api-pro".to_string(),
                group_id_hash: Some("pro".to_string()),
                group_name: "Pro".to_string(),
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: Some(1.2),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(1.2),
                rate_source: Some("sub2api_groups_rates".to_string()),
                confidence: 0.95,
                last_seen_at: Some("1000".to_string()),
                inferred_group_category: Some("gpt".to_string()),
                group_category_override: None,
                raw_json_redacted: None,
            })
            .expect("group");
        database
            .upsert_group_rate_record_if_changed(InsertGroupRateRecordInput {
                station_id: station.id.clone(),
                station_key_id: None,
                group_binding_id: Some(group.id.clone()),
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                group_key_hash: group.group_key_hash.clone(),
                group_name: "Pro".to_string(),
                default_rate_multiplier: Some(1.2),
                user_rate_multiplier: None,
                effective_rate_multiplier: Some(1.2),
                source: "sub2api_groups_rates".to_string(),
                confidence: 0.95,
                inferred_group_category: Some(
                    group
                        .inferred_group_category
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                ),
                raw_json_redacted: None,
                checked_at: "2000".to_string(),
            })
            .expect("rate");

        let enriched = enrich_remote_key_discoveries(
            &database,
            &data_key,
            &station.id,
            vec![RemoteStationKey {
                id: "remote-pro-key".to_string(),
                station_id: station.id.clone(),
                remote_key_id_hash: Some("remote-id-hash".to_string()),
                remote_key_name: Some("Local Pro Key".to_string()),
                api_key_masked: Some("sk-live****cdef".to_string()),
                api_key_fingerprint: None,
                group_id_hash: Some(group.group_key_hash.clone()),
                group_name: None,
                tier_label: None,
                rate_multiplier: None,
                rate_source: Some("sub2api_keys".to_string()),
                created_at: None,
                last_used_at: None,
                raw_source: "sub2api_keys".to_string(),
                match_status: crate::models::remote_keys::RemoteKeyMatchStatus::Unbound,
                matched_station_key_id: None,
                match_confidence: 0.0,
                collected_at: "3000".to_string(),
            }],
        )
        .expect("enriched");

        assert_eq!(enriched.len(), 1);
        let remote_key = &enriched[0];
        assert_eq!(
            remote_key.match_status,
            crate::models::remote_keys::RemoteKeyMatchStatus::Matched
        );
        assert_eq!(
            remote_key.matched_station_key_id.as_deref(),
            Some(local_key.id.as_str())
        );
        assert!(remote_key.match_confidence >= 0.9);
        assert_eq!(remote_key.group_name.as_deref(), Some("Pro"));
        assert_eq!(remote_key.rate_multiplier, Some(1.2));
        assert_eq!(
            remote_key.rate_source.as_deref(),
            Some("sub2api_groups_rates")
        );
    }

    #[test]
    fn local_key_from_remote_discovery_uses_full_secret_not_mask_placeholder() {
        let full_key = "sk-real-secret-f260";
        let server = TestRemoteKeyServer::start(full_key, 2);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station(CreateStationInput {
                name: "remote key station".to_string(),
                station_type: "sub2api".to_string(),
                website_url: server.base_url.clone(),
                api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: String::new(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        database
            .update_station_session_with_data_key(
                UpdateStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: Some("remote-key-token".to_string()),
                    refresh_token: None,
                    cookie: None,
                    newapi_user_id: None,
                    token_expires_at: None,
                },
                &data_key,
            )
            .expect("session");

        let scan = scan_remote_keys(&database, &data_key, station.id.clone()).expect("scan");
        let remote_key = scan.keys.first().expect("remote key");
        let masked = remote_key.api_key_masked.clone().expect("masked key");
        let placeholder_key = database
            .create_station_key_with_data_key(
                CreateStationKeyInput {
                    station_id: station.id.clone(),
                    name: "masked placeholder".to_string(),
                    api_key: masked,
                    enabled: true,
                    priority: None,
                    max_concurrency: None,
                    load_factor: None,
                    schedulable: None,
                    group_name: remote_key.group_name.clone(),
                    tier_label: None,
                    group_binding_id: None,
                    group_id_hash: remote_key.group_id_hash.clone(),
                    rate_multiplier: None,
                    manual_rate_multiplier: None,
                    rate_source: None,
                    balance_scope: Some("station_key".to_string()),
                    note: Some("旧版开关生成的遮罩占位 Key".to_string()),
                },
                &data_key,
            )
            .expect("placeholder key");

        let result = create_local_key_from_remote_key(
            &database,
            &data_key,
            station.id.clone(),
            remote_key.id.clone(),
        )
        .expect("create local key from remote");
        let saved_secret = database
            .resolve_station_key_secret_with_data_key(&data_key, &result.station_key.id)
            .expect("saved secret");

        assert_eq!(saved_secret, full_key);
        assert_ne!(result.station_key.id, placeholder_key.id);
        assert_eq!(
            result.remote_key.matched_station_key_id.as_deref(),
            Some(result.station_key.id.as_str())
        );
    }

    #[test]
    fn newapi_local_key_from_remote_discovery_reveals_full_secret_explicitly() {
        let full_key = "sk-newapi-real-secret-f260";
        let server = TestNewApiRemoteKeyServer::start(full_key, 4);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station(CreateStationInput {
                name: "newapi remote key station".to_string(),
                station_type: "newapi".to_string(),
                website_url: server.base_url.clone(),
                api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: String::new(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        database
            .update_station_session_with_data_key(
                UpdateStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: Some("newapi-remote-key-token".to_string()),
                    refresh_token: None,
                    cookie: None,
                    newapi_user_id: Some("42".to_string()),
                    token_expires_at: None,
                },
                &data_key,
            )
            .expect("session");

        let scan = scan_remote_keys(&database, &data_key, station.id.clone()).expect("scan");
        let remote_key = scan.keys.first().expect("remote key");
        assert_eq!(
            remote_key.api_key_masked.as_deref(),
            Some("sk-new**********f260")
        );
        assert_eq!(remote_key.api_key_fingerprint, None);

        let result = create_local_key_from_remote_key(
            &database,
            &data_key,
            station.id.clone(),
            remote_key.id.clone(),
        )
        .expect("create local key from newapi remote");
        let saved_secret = database
            .resolve_station_key_secret_with_data_key(&data_key, &result.station_key.id)
            .expect("saved secret");

        assert_eq!(saved_secret, full_key);
        assert_eq!(
            result.remote_key.matched_station_key_id.as_deref(),
            Some(result.station_key.id.as_str())
        );
    }

    #[test]
    fn newapi_remote_key_create_saves_secret_without_returning_full_key_once() {
        let full_key = "sk-newapi-created-secret-f260";
        let server = TestNewApiCreateKeyServer::start(full_key, 4);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station(CreateStationInput {
                name: "newapi create key station".to_string(),
                station_type: "newapi".to_string(),
                website_url: server.base_url.clone(),
                api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: String::new(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        database
            .update_station_session_with_data_key(
                UpdateStationSessionInput {
                    station_id: station.id.clone(),
                    access_token: Some("newapi-create-token".to_string()),
                    refresh_token: None,
                    cookie: None,
                    newapi_user_id: Some("42".to_string()),
                    token_expires_at: None,
                },
                &data_key,
            )
            .expect("session");

        let result = create_remote_key(
            &database,
            &data_key,
            CreateRemoteStationKeyInput {
                station_id: station.id.clone(),
                name: "relay-created".to_string(),
                group_binding_id: None,
                group_id_hash: None,
                group_name: Some("vip".to_string()),
            },
        )
        .expect("create newapi remote key");
        let saved_secret = database
            .resolve_station_key_secret_with_data_key(&data_key, &result.station_key.id)
            .expect("saved secret");

        assert_eq!(saved_secret, full_key);
        assert_eq!(result.full_key_once, None);
        assert_eq!(
            result.remote_key.matched_station_key_id.as_deref(),
            Some(result.station_key.id.as_str())
        );
    }

    #[test]
    fn capability_allows_remote_key_actions_for_supported_station_types() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "sub2api");

        let capability = remote_key_capability(&database, station.id).expect("capability");

        assert_eq!(capability.station_type, "sub2api");
        assert!(capability.can_list_remote_keys);
        assert!(capability.can_create_remote_key);
        assert!(capability.can_read_groups);
        assert!(capability.requires_manual_session);
        assert_eq!(capability.unsupported_reason, None);
    }

    #[test]
    fn capability_advertises_newapi_remote_key_actions() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "newapi");

        let capability = remote_key_capability(&database, station.id).expect("capability");

        assert_eq!(capability.station_type, "newapi");
        assert!(capability.can_list_remote_keys);
        assert!(capability.can_create_remote_key);
        assert!(capability.can_read_groups);
        assert!(capability.requires_manual_session);
        assert_eq!(capability.unsupported_reason, None);
    }

    #[test]
    fn capability_rejects_unsupported_station_types_without_actions() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = test_station(&database, "custom");

        let capability = remote_key_capability(&database, station.id).expect("capability");

        assert_eq!(capability.station_type, "custom");
        assert!(!capability.can_list_remote_keys);
        assert!(!capability.can_create_remote_key);
        assert!(!capability.can_read_groups);
        assert!(!capability.requires_manual_session);
        assert!(capability
            .unsupported_reason
            .as_deref()
            .unwrap_or_default()
            .contains("暂不支持"));
    }

    fn test_station(
        database: &AppDatabase,
        station_type: &str,
    ) -> crate::models::stations::Station {
        database
            .create_station(CreateStationInput {
                name: format!("{station_type} station"),
                station_type: station_type.to_string(),
                website_url: "https://relay.example".to_string(),
                api_base_url: "https://relay.example/v1".to_string(),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-test".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station")
    }

    struct TestRemoteKeyServer {
        base_url: String,
    }

    impl TestRemoteKeyServer {
        fn start(full_key: &'static str, requests: usize) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            thread::spawn(move || {
                for stream in listener.incoming().take(requests).flatten() {
                    let mut stream = stream;
                    let mut buffer = [0_u8; 2048];
                    let read = stream.read(&mut buffer).expect("read request");
                    let request = String::from_utf8_lossy(&buffer[..read]);
                    let authorized = request
                        .to_lowercase()
                        .contains("authorization: bearer remote-key-token");
                    let (status, body) = if authorized && request.starts_with("GET /api/v1/keys") {
                        (
                            "200 OK",
                            serde_json::json!({
                                "data": {
                                    "items": [{
                                        "id": "remote-real-key",
                                        "name": "123",
                                        "key": full_key,
                                        "group": "codex-0号池",
                                        "rate_multiplier": 0.01
                                    }]
                                }
                            })
                            .to_string(),
                        )
                    } else {
                        (
                            "401 Unauthorized",
                            serde_json::json!({ "message": "unauthorized" }).to_string(),
                        )
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream
                        .write_all(response.as_bytes())
                        .expect("write response");
                }
            });
            Self { base_url }
        }
    }

    struct TestNewApiRemoteKeyServer {
        base_url: String,
    }

    impl TestNewApiRemoteKeyServer {
        fn start(full_key: &'static str, requests: usize) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            thread::spawn(move || {
                for stream in listener.incoming().take(requests).flatten() {
                    let mut stream = stream;
                    let mut buffer = [0_u8; 4096];
                    let read = stream.read(&mut buffer).expect("read request");
                    let request = String::from_utf8_lossy(&buffer[..read]);
                    let authorized = request
                        .to_lowercase()
                        .contains("authorization: bearer newapi-remote-key-token")
                        && request.contains("New-Api-User: 42");
                    let (status, body) = if authorized && request.starts_with("GET /api/token/") {
                        (
                            "200 OK",
                            serde_json::json!({
                                "success": true,
                                "data": {
                                    "page": 1,
                                    "page_size": 100,
                                    "total": 1,
                                    "items": [{
                                        "id": 201,
                                        "name": "NewAPI primary",
                                        "key": "sk-new**********f260",
                                        "group": "default"
                                    }]
                                }
                            })
                            .to_string(),
                        )
                    } else if authorized && request.starts_with("POST /api/token/201/key") {
                        (
                            "200 OK",
                            serde_json::json!({
                                "success": true,
                                "data": { "key": full_key }
                            })
                            .to_string(),
                        )
                    } else {
                        (
                            "401 Unauthorized",
                            serde_json::json!({ "message": "unauthorized" }).to_string(),
                        )
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream
                        .write_all(response.as_bytes())
                        .expect("write response");
                }
            });
            Self { base_url }
        }
    }

    struct TestNewApiCreateKeyServer {
        base_url: String,
    }

    impl TestNewApiCreateKeyServer {
        fn start(full_key: &'static str, requests: usize) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            thread::spawn(move || {
                for stream in listener.incoming().take(requests).flatten() {
                    let mut stream = stream;
                    let mut buffer = [0_u8; 4096];
                    let read = stream.read(&mut buffer).expect("read request");
                    let request = String::from_utf8_lossy(&buffer[..read]);
                    let authorized = request
                        .to_lowercase()
                        .contains("authorization: bearer newapi-create-token")
                        && request.contains("New-Api-User: 42");
                    let (status, body) = if authorized && request.starts_with("POST /api/token/ ") {
                        (
                            "200 OK",
                            serde_json::json!({"success": true, "message": ""}).to_string(),
                        )
                    } else if authorized && request.starts_with("GET /api/token/") {
                        (
                            "200 OK",
                            serde_json::json!({
                                "success": true,
                                "data": {
                                    "page": 1,
                                    "page_size": 100,
                                    "total": 1,
                                    "items": [{
                                        "id": 401,
                                        "name": "relay-created",
                                        "key": "sk-new**********f260",
                                        "group": "vip"
                                    }]
                                }
                            })
                            .to_string(),
                        )
                    } else if authorized && request.starts_with("POST /api/token/401/key") {
                        (
                            "200 OK",
                            serde_json::json!({
                                "success": true,
                                "data": { "key": full_key }
                            })
                            .to_string(),
                        )
                    } else {
                        (
                            "401 Unauthorized",
                            serde_json::json!({ "message": "unauthorized" }).to_string(),
                        )
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream
                        .write_all(response.as_bytes())
                        .expect("write response");
                }
            });
            Self { base_url }
        }
    }
}
