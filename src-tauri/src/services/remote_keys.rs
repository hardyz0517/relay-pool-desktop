use sha2::{Digest, Sha256};

use crate::{
    models::{
        remote_keys::{
            BindRemoteStationKeyInput, CreateRemoteStationKeyInput, CreateRemoteStationKeyResult,
            RemoteKeyCapability, RemoteKeyScanResult, RemoteStationKey,
        },
        station_keys::CreateStationKeyInput,
    },
    services::database::{now_millis_for_services, AppDatabase},
};

const REMOTE_KEY_ADAPTER_PENDING_MESSAGE: &str =
    "远端 Key 管理适配器尚未接入，请等待 Task 4 完成后再执行该操作。";

pub fn remote_key_capability(
    database: &AppDatabase,
    station_id: String,
) -> Result<RemoteKeyCapability, String> {
    let station = database.station_for_collector(&station_id)?;
    let station_type = station.station_type.trim().to_string();
    let supported = matches!(station_type.as_str(), "sub2api" | "newapi");

    Ok(RemoteKeyCapability {
        station_id,
        station_type: station_type.clone(),
        can_list_remote_keys: supported,
        can_create_remote_key: supported,
        can_read_groups: supported,
        requires_manual_session: supported,
        unsupported_reason: if supported {
            None
        } else {
            Some(format!(
                "暂不支持 {station_type} 类型中转站的远端 Key 管理。"
            ))
        },
    })
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

    let (remote_key, full_key) =
        create_remote_key_with_adapter(database, data_key, input, &capability.station_type)?;
    save_created_remote_key(database, data_key, remote_key, full_key)
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
        "sub2api" | "newapi" => {
            let _ = (database, data_key, station_id);
            Err(format!(
                "{} 当前中转站类型：{}。",
                REMOTE_KEY_ADAPTER_PENDING_MESSAGE, station_type
            ))
        }
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
) -> Result<(RemoteStationKey, String), String> {
    match station_type {
        "sub2api" | "newapi" => {
            let _ = (database, data_key, input);
            Err(format!(
                "{} 当前中转站类型：{}。",
                REMOTE_KEY_ADAPTER_PENDING_MESSAGE, station_type
            ))
        }
        _ => Err(format!(
            "暂不支持 {station_type} 类型中转站的远端 Key 创建。"
        )),
    }
}

fn save_created_remote_key(
    database: &AppDatabase,
    data_key: &[u8; 32],
    remote_key: RemoteStationKey,
    full_key: String,
) -> Result<CreateRemoteStationKeyResult, String> {
    if full_key.trim().is_empty() {
        return Err("远端站点未返回完整 Key，无法保存到本地 Station Key。".to_string());
    }

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
            group_name: remote_key.group_name.clone(),
            tier_label: remote_key.tier_label.clone(),
            group_binding_id: None,
            group_id_hash: remote_key.group_id_hash.clone(),
            rate_multiplier: remote_key.rate_multiplier,
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

    let mut remote_keys =
        database.list_remote_station_keys(synced_remote_key.station_id.clone())?;
    remote_keys.retain(|key| key.id != synced_remote_key.id);
    remote_keys.push(synced_remote_key.clone());
    let saved_remote_keys =
        database.replace_remote_station_keys(synced_remote_key.station_id.clone(), remote_keys)?;
    let remote_key = saved_remote_keys
        .into_iter()
        .find(|key| key.id == synced_remote_key.id)
        .unwrap_or(synced_remote_key);

    Ok(CreateRemoteStationKeyResult {
        remote_key,
        station_key,
        full_key_once: Some(full_key),
        message: "远端 Key 已创建，并已保存为启用的本地 Station Key。".to_string(),
    })
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
    if remote_fingerprint.is_some() && remote_fingerprint == local_fingerprint {
        return 1.0;
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
    use crate::{models::stations::CreateStationInput, services::database::AppDatabase};

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
                base_url: "https://relay.example".to_string(),
                api_key: "sk-test".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                note: None,
            })
            .expect("station")
    }
}
