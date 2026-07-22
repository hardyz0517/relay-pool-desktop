use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::{
    application::credentials::CredentialService,
    models::{
        group_facts::{GroupRateRecord, StationGroupBinding},
        remote_keys::{
            CreateLocalStationKeyFromRemoteResult, CreateRemoteStationKeyInput,
            CreateRemoteStationKeyResult, RemoteKeyCapability, RemoteKeyScanResult,
            RemoteStationKey,
        },
        station_keys::{StationKey, UpdateStationKeyInput},
    },
    services::collectors::{
        adapters::{self, CreatedRemoteKey},
        CollectorSourcePort,
    },
};

// V2CollectorSourceAdapter resolves secrets through CredentialService; provider
// adapters retain this argument only for the temporary legacy port implementation.
const V2_UNUSED_DATA_KEY: [u8; 32] = [0; 32];

pub(crate) enum PreparedRemoteKeyScan {
    Unsupported {
        station_id: String,
        capability: RemoteKeyCapability,
    },
    Discovered {
        station_id: String,
        expected_endpoint_revision: i64,
        capability: RemoteKeyCapability,
        keys: Vec<RemoteStationKey>,
        station_key_updates: Vec<UpdateStationKeyInput>,
    },
}
pub(crate) struct PreparedRemoteKeySave {
    remote_key: RemoteStationKey,
    expected_endpoint_revision: i64,
    matched_station_key_update: Option<UpdateStationKeyInput>,
    new_group_binding_id: Option<String>,
    full_key: String,
    adapter_message: String,
    expose_full_key_once: bool,
    matched_existing: bool,
}

pub(crate) fn prepare_remote_key_scan_v2(
    source: &dyn CollectorSourcePort,
    station_id: String,
) -> Result<PreparedRemoteKeyScan, String> {
    let (capability, expected_endpoint_revision) =
        remote_key_capability_from_source(source, station_id.clone())?;
    if !capability.can_list_remote_keys {
        return Ok(PreparedRemoteKeyScan::Unsupported {
            station_id,
            capability,
        });
    }
    let discovered = scan_remote_keys_with_source(source, &station_id, &capability.station_type)?;
    let (keys, station_key_updates) =
        enrich_remote_key_discoveries_from_source(source, &station_id, discovered)?;
    ensure_source_endpoint_revision(source, &station_id, expected_endpoint_revision)?;
    Ok(PreparedRemoteKeyScan::Discovered {
        station_id,
        expected_endpoint_revision,
        capability,
        keys,
        station_key_updates,
    })
}

pub(crate) async fn finish_remote_key_scan_v2(
    credentials: &CredentialService,
    prepared: PreparedRemoteKeyScan,
) -> Result<RemoteKeyScanResult, String> {
    match prepared {
        PreparedRemoteKeyScan::Unsupported {
            station_id,
            capability,
        } => {
            let keys = credentials
                .list_remote_station_keys(station_id.clone())
                .await
                .map_err(|error| error.to_string())?;
            Ok(RemoteKeyScanResult {
                station_id,
                capability: capability.clone(),
                keys,
                synced_station_key_ids: Vec::new(),
                message: capability
                    .unsupported_reason
                    .clone()
                    .unwrap_or_else(|| "该中转站暂不支持远端 Key 扫描。".to_string()),
            })
        }
        PreparedRemoteKeyScan::Discovered {
            station_id,
            expected_endpoint_revision,
            capability,
            keys,
            station_key_updates,
        } => {
            let keys = credentials
                .replace_remote_station_keys_and_metadata(
                    station_id.clone(),
                    expected_endpoint_revision,
                    keys,
                    station_key_updates,
                )
                .await
                .map_err(|error| error.to_string())?;
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
    }
}

pub(crate) fn prepare_remote_key_creation_v2(
    source: &dyn CollectorSourcePort,
    input: CreateRemoteStationKeyInput,
) -> Result<PreparedRemoteKeySave, String> {
    let (capability, expected_endpoint_revision) =
        remote_key_capability_from_source(source, input.station_id.clone())?;
    if !capability.can_create_remote_key {
        return Err(capability
            .unsupported_reason
            .unwrap_or_else(|| "该中转站暂不支持创建远端 Key。".to_string()));
    }
    let CreatedRemoteKey {
        remote_key,
        full_key_once,
        message,
    } = create_remote_key_with_source(source, input, &capability.station_type)?;
    let full_key = full_key_once
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "远端站点未返回完整 Key，无法保存到本地 Station Key。".to_string())?;
    prepare_remote_key_save_from_source(
        source,
        remote_key,
        full_key,
        message,
        capability.station_type != "newapi",
        expected_endpoint_revision,
    )
}

pub(crate) fn prepare_local_key_from_remote_v2(
    source: &dyn CollectorSourcePort,
    station_id: String,
    remote_key_id: String,
) -> Result<PreparedRemoteKeySave, String> {
    let (capability, expected_endpoint_revision) =
        remote_key_capability_from_source(source, station_id.clone())?;
    if !capability.can_list_remote_keys {
        return Err(capability
            .unsupported_reason
            .unwrap_or_else(|| "该中转站暂不支持远端 Key 扫描。".to_string()));
    }
    let (remote_key, full_key) = remote_key_full_secret_with_source(
        source,
        &station_id,
        &remote_key_id,
        &capability.station_type,
    )?;
    prepare_remote_key_save_from_source(
        source,
        remote_key,
        full_key,
        "远端 Key 已同步为本地 Station Key。".to_string(),
        false,
        expected_endpoint_revision,
    )
}

pub(crate) async fn finish_remote_key_creation_v2(
    credentials: &CredentialService,
    prepared: PreparedRemoteKeySave,
) -> Result<CreateRemoteStationKeyResult, String> {
    let PreparedRemoteKeySave {
        remote_key,
        expected_endpoint_revision,
        matched_station_key_update,
        new_group_binding_id,
        full_key,
        adapter_message,
        expose_full_key_once,
        matched_existing,
    } = prepared;
    let response_key = expose_full_key_once.then(|| full_key.clone());
    let (remote_key, station_key) = credentials
        .save_remote_station_key_with_local(
            remote_key,
            expected_endpoint_revision,
            matched_station_key_update,
            new_group_binding_id,
            full_key,
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(CreateRemoteStationKeyResult {
        remote_key,
        station_key,
        full_key_once: response_key,
        message: remote_key_save_message(&adapter_message, matched_existing),
    })
}

pub(crate) async fn finish_local_key_from_remote_v2(
    credentials: &CredentialService,
    prepared: PreparedRemoteKeySave,
) -> Result<CreateLocalStationKeyFromRemoteResult, String> {
    let result = finish_remote_key_creation_v2(credentials, prepared).await?;
    Ok(CreateLocalStationKeyFromRemoteResult {
        remote_key: result.remote_key,
        station_key: result.station_key,
        message: "远端 Key 已保存为本地 Station Key。".to_string(),
    })
}

fn remote_key_capability_from_source(
    source: &dyn CollectorSourcePort,
    station_id: String,
) -> Result<(RemoteKeyCapability, i64), String> {
    let station = source.station_for_collector(&station_id)?;
    let endpoint_revision = station.endpoint_revision;
    let station_type = station.station_type.trim().to_string();
    let capability = match station_type.as_str() {
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
    }?;
    Ok((capability, endpoint_revision))
}

fn ensure_source_endpoint_revision(
    source: &dyn CollectorSourcePort,
    station_id: &str,
    expected_endpoint_revision: i64,
) -> Result<(), String> {
    let current = source.station_for_collector(station_id)?;
    if current.endpoint_revision != expected_endpoint_revision {
        return Err("station endpoint changed while remote key operation was running".to_string());
    }
    Ok(())
}

fn scan_remote_keys_with_source(
    source: &dyn CollectorSourcePort,
    station_id: &str,
    station_type: &str,
) -> Result<Vec<RemoteStationKey>, String> {
    match station_type {
        "sub2api" => adapters::sub2api::scan_remote_keys(source, &V2_UNUSED_DATA_KEY, station_id),
        "newapi" => adapters::newapi::scan_remote_keys(source, &V2_UNUSED_DATA_KEY, station_id),
        _ => Err(format!(
            "暂不支持 {station_type} 类型中转站的远端 Key 扫描。"
        )),
    }
}

fn create_remote_key_with_source(
    source: &dyn CollectorSourcePort,
    input: CreateRemoteStationKeyInput,
    station_type: &str,
) -> Result<CreatedRemoteKey, String> {
    match station_type {
        "sub2api" => adapters::sub2api::create_remote_key(source, &V2_UNUSED_DATA_KEY, input),
        "newapi" => adapters::newapi::create_remote_key(source, &V2_UNUSED_DATA_KEY, input),
        _ => Err(format!(
            "暂不支持 {station_type} 类型中转站的远端 Key 创建。"
        )),
    }
}

fn remote_key_full_secret_with_source(
    source: &dyn CollectorSourcePort,
    station_id: &str,
    remote_key_id: &str,
    station_type: &str,
) -> Result<(RemoteStationKey, String), String> {
    match station_type {
        "sub2api" => adapters::sub2api::scan_remote_key_full_secret(
            source,
            &V2_UNUSED_DATA_KEY,
            station_id,
            remote_key_id,
        ),
        "newapi" => adapters::newapi::scan_remote_key_full_secret(
            source,
            &V2_UNUSED_DATA_KEY,
            station_id,
            remote_key_id,
        ),
        _ => Err(format!(
            "暂不支持 {station_type} 类型中转站从远端发现同步本地 Key。"
        )),
    }
}

fn prepare_remote_key_save_from_source(
    source: &dyn CollectorSourcePort,
    remote_key: RemoteStationKey,
    full_key: String,
    adapter_message: String,
    expose_full_key_once: bool,
    expected_endpoint_revision: i64,
) -> Result<PreparedRemoteKeySave, String> {
    if full_key.trim().is_empty() {
        return Err("远端站点未返回完整 Key，无法保存到本地 Station Key。".to_string());
    }
    let station_id = remote_key.station_id.clone();
    let bindings = source.list_station_group_bindings(station_id.clone())?;
    let (mut remote_keys, mut station_key_updates) =
        enrich_remote_key_discoveries_from_parts(source, &station_id, &bindings, vec![remote_key])?;
    let remote_key = remote_keys
        .pop()
        .ok_or_else(|| "远端 Key 创建结果为空，无法同步。".to_string())?;
    let matched_station_key_update = station_key_updates.pop();
    let matched_existing = matched_station_key_update.is_some();
    let new_group_binding_id = (!matched_existing)
        .then(|| matching_group_binding(&remote_key, &bindings))
        .flatten()
        .map(|binding| binding.id.clone())
        .filter(|id| !id.trim().is_empty());
    ensure_source_endpoint_revision(source, &station_id, expected_endpoint_revision)?;
    Ok(PreparedRemoteKeySave {
        remote_key,
        expected_endpoint_revision,
        matched_station_key_update,
        new_group_binding_id,
        full_key,
        adapter_message,
        expose_full_key_once,
        matched_existing,
    })
}

fn enrich_remote_key_discoveries_from_source(
    source: &dyn CollectorSourcePort,
    station_id: &str,
    keys: Vec<RemoteStationKey>,
) -> Result<(Vec<RemoteStationKey>, Vec<UpdateStationKeyInput>), String> {
    let bindings = source.list_station_group_bindings(station_id.to_string())?;
    enrich_remote_key_discoveries_from_parts(source, station_id, &bindings, keys)
}

fn enrich_remote_key_discoveries_from_parts(
    source: &dyn CollectorSourcePort,
    station_id: &str,
    bindings: &[StationGroupBinding],
    keys: Vec<RemoteStationKey>,
) -> Result<(Vec<RemoteStationKey>, Vec<UpdateStationKeyInput>), String> {
    let local_candidates = local_station_key_candidates_from_source(source, station_id)?;
    let mut updates = BTreeMap::<String, (f64, UpdateStationKeyInput)>::new();
    let mut enriched = Vec::with_capacity(keys.len());
    for mut key in keys {
        apply_group_metadata(&mut key, bindings, &[]);
        if let Some((local_key, confidence)) = best_local_key_match(&key, &local_candidates) {
            if confidence >= 0.8 {
                key.matched_station_key_id = Some(local_key.key.id.clone());
                key.match_confidence = confidence;
                key.match_status = if confidence >= 0.9 {
                    crate::models::remote_keys::RemoteKeyMatchStatus::Matched
                } else {
                    crate::models::remote_keys::RemoteKeyMatchStatus::Possible
                };
                let update = station_key_metadata_update(&local_key.key, &key, bindings);
                let replace = updates
                    .get(&local_key.key.id)
                    .map(|(current, _)| confidence > *current)
                    .unwrap_or(true);
                if replace {
                    updates.insert(local_key.key.id.clone(), (confidence, update));
                }
            }
        }
        enriched.push(key);
    }
    Ok((
        enriched,
        updates.into_values().map(|(_, update)| update).collect(),
    ))
}

fn local_station_key_candidates_from_source(
    source: &dyn CollectorSourcePort,
    station_id: &str,
) -> Result<Vec<LocalStationKeyCandidate>, String> {
    let keys = source.list_station_keys(station_id.to_string())?;
    Ok(keys
        .into_iter()
        .map(|key| {
            let full_key = if key.api_key_present {
                source
                    .resolve_station_key_secret_with_data_key(&V2_UNUSED_DATA_KEY, &key.id)
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

fn station_key_metadata_update(
    local_key: &StationKey,
    remote_key: &RemoteStationKey,
    bindings: &[StationGroupBinding],
) -> UpdateStationKeyInput {
    let group_binding_id = matching_group_binding(remote_key, bindings)
        .map(|binding| binding.id.clone())
        .or_else(|| local_key.group_binding_id.clone());
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
    }
}

fn remote_key_save_message(adapter_message: &str, matched_existing: bool) -> String {
    match (adapter_message.trim().is_empty(), matched_existing) {
        (true, true) => "远端 Key 已创建，并已关联到已有本地 Station Key。".to_string(),
        (false, true) => format!("{adapter_message} 已关联到已有本地 Station Key。"),
        (true, false) => "远端 Key 已创建，并已保存为启用的本地 Station Key。".to_string(),
        (false, false) => format!("{adapter_message} 已保存为启用的本地 Station Key。"),
    }
}

#[derive(Debug, Clone)]
struct LocalStationKeyCandidate {
    key: StationKey,
    full_key: Option<String>,
    fingerprint: Option<String>,
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

    #[test]
    fn fingerprints_are_stable_without_exposing_secrets() {
        assert_eq!(api_key_fingerprint("sk-a"), api_key_fingerprint("sk-a"));
        assert_ne!(api_key_fingerprint("sk-a"), api_key_fingerprint("sk-b"));
        assert_eq!(api_key_fingerprint("   "), None);
    }

    #[test]
    fn masked_key_matching_requires_meaningful_visible_parts() {
        assert!(masked_key_matches_full(
            "sk-live****cdef",
            "sk-live-123-cdef",
        ));
        assert!(!masked_key_matches_full("sk****ef", "sk-live-123-cdef"));
        assert!(!masked_key_matches_full("not-masked", "sk-live-123-cdef"));
    }

    #[test]
    fn confidence_never_accepts_name_and_group_only_as_a_secret_match() {
        assert!(remote_key_confidence(None, None, None, None, true, true) < 0.8);
        let fingerprint = api_key_fingerprint("sk-live-123-cdef");
        assert_eq!(
            remote_key_confidence(
                fingerprint.as_deref(),
                fingerprint.as_deref(),
                None,
                None,
                false,
                false,
            ),
            1.0,
        );
    }
}
