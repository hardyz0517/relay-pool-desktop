mod auth;
mod client;
mod parsers;
#[cfg(test)]
mod test_support;

use serde_json::{json, Value};

use crate::models::{
    remote_keys::{CreateRemoteStationKeyInput, RemoteKeyCapability, RemoteStationKey},
    stations::Station,
};
use crate::services::{
    collectors::{
        adapters::{AdapterOutput, CollectorTask, CreatedRemoteKey},
        facts::{CollectedBalanceFact, CollectorFacts},
        url::{collector_base_urls, join_url},
    },
    database::AppDatabase,
    outbound::{agent_builder_for_proxy, resolve_proxy_config, ProxyConfig},
};

const NEWAPI_REMOTE_KEY_UNSUPPORTED: &str =
    "NewAPI 远端 Key 列表/创建接口尚未适配；当前仅支持读取账号分组信息。";
const COLLECTOR_HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

fn parse_newapi_balance(station_id: &str, payload: &Value) -> CollectedBalanceFact {
    parsers::parse_balance_fact(station_id, payload, 500000.0, true)
}

fn parse_newapi_group_facts(station_id: &str, payload: &Value) -> CollectorFacts {
    parsers::parse_group_facts(station_id, payload)
}

pub fn remote_key_capability(station: &Station) -> Result<RemoteKeyCapability, String> {
    Ok(RemoteKeyCapability {
        station_id: station.id.clone(),
        station_type: station.station_type.trim().to_string(),
        can_list_remote_keys: false,
        can_create_remote_key: false,
        can_read_groups: true,
        requires_manual_session: true,
        unsupported_reason: Some(NEWAPI_REMOTE_KEY_UNSUPPORTED.to_string()),
    })
}

pub fn scan_remote_keys(
    _database: &AppDatabase,
    _data_key: &[u8; 32],
    _station_id: &str,
) -> Result<Vec<RemoteStationKey>, String> {
    Ok(Vec::new())
}

pub fn create_remote_key(
    _database: &AppDatabase,
    _data_key: &[u8; 32],
    _input: CreateRemoteStationKeyInput,
) -> Result<CreatedRemoteKey, String> {
    Err(NEWAPI_REMOTE_KEY_UNSUPPORTED.to_string())
}

pub fn collect(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    task: CollectorTask,
) -> Result<AdapterOutput, String> {
    match task {
        CollectorTask::Detect => Ok(AdapterOutput {
            adapter: "newapi".to_string(),
            task,
            status: "success".to_string(),
            summary_json: json!({
                "adapter": "newapi",
                "task": "detect",
                "message": "NewAPI adapter 已确认。",
                "endpointResults": [],
            }),
            normalized_json: json!({
                "adapter": "newapi",
                "models": [],
            }),
            raw_json_redacted: None,
            error_code: None,
            error_message: None,
            facts: CollectorFacts::default(),
        }),
        CollectorTask::Balance | CollectorTask::Groups => {
            collect_balance_and_groups(database, data_key, station_id, task)
        }
        CollectorTask::Models => unsupported_output(
            task,
            "unsupported_task",
            "NewAPI adapter 暂不支持模型列表采集。",
        ),
        CollectorTask::Full => {
            unsupported_output(task, "internal_task", "Full 采集由父任务拆分为子任务执行。")
        }
    }
}

fn unsupported_output(
    task: CollectorTask,
    code: &str,
    message: &str,
) -> Result<AdapterOutput, String> {
    Ok(AdapterOutput {
        adapter: "newapi".to_string(),
        task,
        status: "manual_required".to_string(),
        summary_json: json!({
            "adapter": "newapi",
            "task": task.as_str(),
            "message": message,
            "endpointResults": [],
        }),
        normalized_json: json!({ "models": [] }),
        raw_json_redacted: None,
        error_code: Some(code.to_string()),
        error_message: Some(message.to_string()),
        facts: CollectorFacts::default(),
    })
}

pub fn collect_balance_and_groups(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    task: CollectorTask,
) -> Result<AdapterOutput, String> {
    let station = database.station_for_collector(station_id)?;
    let settings = database.get_settings()?;
    let proxy = resolve_proxy_config(
        &station.collector_proxy_mode,
        station.collector_proxy_url.clone(),
        &settings.collector_proxy_mode,
        settings.collector_proxy_url,
    );
    let credentials = database.get_station_credentials(station_id.to_string())?;
    let Some(user_id) = credentials.newapi_user_id.clone() else {
        return manual_required_output(
            "newapi",
            task,
            "newapi_user_id_required",
            "NewAPI 采集需要 User ID。",
        );
    };
    let session = database.resolve_station_session_with_data_key(
        station_id.to_string(),
        data_key,
        crate::services::database::now_millis_for_services() as i64,
    )?;
    let Some(access_token) = session.access_token else {
        return manual_required_output(
            "newapi",
            task,
            "manual_session_required",
            "NewAPI 采集需要 access token。",
        );
    };

    let urls = collector_base_urls(&station.base_url);
    let mut facts = CollectorFacts::default();
    let mut endpoint_results = Vec::new();

    if matches!(task, CollectorTask::Balance | CollectorTask::Full) {
        let url = join_url(&urls.management_base_url, "/api/user/self");
        let payload =
            get_newapi_json(&url, &access_token, &user_id, &proxy, &mut endpoint_results)?;
        let data = parsers::envelope_data(&payload).map_err(|error| error.message)?;
        facts.balances.push(parse_newapi_balance(station_id, data));
    }
    if matches!(task, CollectorTask::Groups | CollectorTask::Full) {
        let url = join_url(&urls.management_base_url, "/api/user/self/groups");
        let payload =
            get_newapi_json(&url, &access_token, &user_id, &proxy, &mut endpoint_results)?;
        let data = parsers::envelope_data(&payload).map_err(|error| error.message)?;
        let group_facts = parse_newapi_group_facts(station_id, data);
        facts.groups.extend(group_facts.groups);
        facts.rates.extend(group_facts.rates);
    }

    Ok(AdapterOutput {
        adapter: "newapi".to_string(),
        task,
        status: "success".to_string(),
        summary_json: json!({
            "adapter": "newapi",
            "task": task.as_str(),
            "endpointResults": endpoint_results,
        }),
        normalized_json: json!({
            "balanceCount": facts.balances.len(),
            "groupCount": facts.groups.len(),
            "rateCount": facts.rates.len(),
        }),
        raw_json_redacted: Some(json!({ "endpointResults": endpoint_results })),
        error_code: None,
        error_message: None,
        facts,
    })
}

fn get_newapi_json(
    url: &str,
    access_token: &str,
    user_id: &str,
    proxy: &ProxyConfig,
    endpoint_results: &mut Vec<Value>,
) -> Result<Value, String> {
    let started = std::time::Instant::now();
    let agent = match agent_builder_for_proxy(proxy) {
        Ok(builder) => builder.timeout(COLLECTOR_HTTP_TIMEOUT).build(),
        Err(error) => {
            let message = crate::services::secrets::mask::redact_text(&error);
            endpoint_results.push(json!({
                "url": url,
                "status": null,
                "ok": false,
                "durationMs": started.elapsed().as_millis() as i64,
                "errorMessage": message,
            }));
            return Err(message);
        }
    };
    let response = match agent
        .get(url)
        .timeout(COLLECTOR_HTTP_TIMEOUT)
        .set("Authorization", &format!("Bearer {access_token}"))
        .set("New-Api-User", user_id)
        .set("Content-Type", "application/json")
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => {
            let message = crate::services::secrets::mask::redact_text(&error.to_string());
            endpoint_results.push(json!({
                "url": url,
                "status": null,
                "ok": false,
                "durationMs": started.elapsed().as_millis() as i64,
                "errorMessage": message,
            }));
            return Err(message);
        }
    };
    let status = response.status();
    let text = response.into_string().unwrap_or_default();
    endpoint_results.push(json!({
        "url": url,
        "status": status,
        "ok": (200..400).contains(&status),
        "durationMs": started.elapsed().as_millis() as i64,
    }));
    let payload = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
    if (200..400).contains(&status) {
        Ok(payload)
    } else {
        Err(crate::services::secrets::mask::redact_text(&text))
    }
}

fn manual_required_output(
    adapter: &str,
    task: CollectorTask,
    code: &str,
    message: &str,
) -> Result<AdapterOutput, String> {
    Ok(AdapterOutput {
        adapter: adapter.to_string(),
        task,
        status: "manual_required".to_string(),
        summary_json: json!({ "adapter": adapter, "task": task.as_str(), "message": message }),
        normalized_json: json!({ "balanceCount": 0, "groupCount": 0, "rateCount": 0 }),
        raw_json_redacted: None,
        error_code: Some(code.to_string()),
        error_message: Some(message.to_string()),
        facts: CollectorFacts::default(),
    })
}

fn stable_group_key_hash(
    station_id: &str,
    adapter: &str,
    group_id: Option<&str>,
    group_name: &str,
) -> String {
    let adapter = adapter.trim().to_lowercase();
    let source = if let Some(group_id) = group_id.filter(|value| !value.trim().is_empty()) {
        format!("id:{adapter}:{}", group_id.trim())
    } else {
        format!(
            "name:{}:{}:{}",
            station_id,
            adapter,
            group_name.trim().to_lowercase()
        )
    };
    sha256_hex(source.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn newapi_quota_converts_to_usd_units() {
        let fact = parse_newapi_balance(
            "station-1",
            &json!({
                "quota": 1000000.0,
                "used_quota": 500000.0,
                "group": "default"
            }),
        );

        assert_eq!(fact.value, Some(2.0));
        assert_eq!(fact.used_value, Some(1.0));
        assert_eq!(fact.total_value, Some(3.0));
        assert_eq!(fact.currency, "USD");
        assert_eq!(fact.source, "newapi_user_self");
    }

    #[test]
    fn newapi_groups_parse_list_and_rate_fields() {
        let facts = parse_newapi_group_facts(
            "station-1",
            &json!({
                "default": { "desc": "Default", "ratio": 1.0 },
                "vip": { "desc": "VIP", "ratio": 0.8 }
            }),
        );

        assert!(facts
            .groups
            .iter()
            .any(|group| group.group_name == "default"));
        assert!(facts.rates.iter().any(|rate| {
            rate.group_name == "vip" && rate.effective_rate_multiplier == Some(0.8)
        }));
    }
}
