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
        CollectorTask::Balance | CollectorTask::Groups | CollectorTask::Models => {
            collect_balance_and_groups(database, data_key, station_id, task)
        }
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

fn build_balance_output(
    station_id: &str,
    data: &Value,
    status: &parsers::NewApiStatus,
    endpoint_result: Value,
) -> AdapterOutput {
    let mut facts = CollectorFacts::default();
    facts.balances.push(parsers::parse_balance_fact(
        station_id,
        data,
        status.quota_per_unit,
        status.used_fallback,
    ));
    let balance_count = facts.balances.len();
    AdapterOutput {
        adapter: "newapi".to_string(),
        task: CollectorTask::Balance,
        status: if balance_count == 0 {
            "partial"
        } else {
            "success"
        }
        .to_string(),
        summary_json: json!({
            "adapter": "newapi",
            "task": "balance",
            "quotaPerUnit": status.quota_per_unit,
            "quotaPerUnitFallback": status.used_fallback,
            "endpointResults": [endpoint_result],
        }),
        normalized_json: json!({
            "balanceCount": balance_count,
            "models": [],
        }),
        raw_json_redacted: None,
        error_code: (balance_count == 0).then(|| "empty_balance_facts".to_string()),
        error_message: (balance_count == 0)
            .then(|| "NewAPI balance response did not contain quota facts".to_string()),
        facts,
    }
}

fn build_groups_output(station_id: &str, data: &Value, endpoint_result: Value) -> AdapterOutput {
    let facts = parsers::parse_group_facts(station_id, data);
    let group_count = facts.groups.len();
    let rate_count = facts.rates.len();
    AdapterOutput {
        adapter: "newapi".to_string(),
        task: CollectorTask::Groups,
        status: if group_count == 0 {
            "partial"
        } else {
            "success"
        }
        .to_string(),
        summary_json: json!({
            "adapter": "newapi",
            "task": "groups",
            "endpointResults": [endpoint_result],
        }),
        normalized_json: json!({
            "groupCount": group_count,
            "rateCount": rate_count,
            "groups": facts.groups.iter().map(|group| json!({
                "groupId": group.group_id,
                "groupIdHash": group.group_key_hash,
                "groupName": group.group_name,
            })).collect::<Vec<_>>(),
            "rateMultipliers": facts.rates.iter().map(|rate| json!({
                "groupId": rate.group_id,
                "groupIdHash": rate.group_key_hash,
                "groupName": rate.group_name,
                "effectiveRateMultiplier": rate.effective_rate_multiplier,
            })).collect::<Vec<_>>(),
            "models": [],
        }),
        raw_json_redacted: None,
        error_code: (group_count == 0).then(|| "empty_group_facts".to_string()),
        error_message: (group_count == 0)
            .then(|| "NewAPI groups response did not contain group facts".to_string()),
        facts,
    }
}

fn build_models_output(station_id: &str, data: &Value, endpoint_result: Value) -> AdapterOutput {
    let mut facts = CollectorFacts::default();
    facts.models = parsers::parse_models(station_id, data);
    let model_names = facts
        .models
        .iter()
        .map(|model| model.model.clone())
        .collect::<Vec<_>>();
    AdapterOutput {
        adapter: "newapi".to_string(),
        task: CollectorTask::Models,
        status: if model_names.is_empty() {
            "partial"
        } else {
            "success"
        }
        .to_string(),
        summary_json: json!({
            "adapter": "newapi",
            "task": "models",
            "endpointResults": [endpoint_result],
        }),
        normalized_json: json!({
            "models": model_names,
        }),
        raw_json_redacted: None,
        error_code: facts
            .models
            .is_empty()
            .then(|| "empty_model_facts".to_string()),
        error_message: facts
            .models
            .is_empty()
            .then(|| "NewAPI models response did not contain model facts".to_string()),
        facts,
    }
}

pub fn collect_balance_and_groups(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    task: CollectorTask,
) -> Result<AdapterOutput, String> {
    collect_authenticated_task(database, data_key, station_id, task)
}

fn collect_authenticated_task(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    task: CollectorTask,
) -> Result<AdapterOutput, String> {
    let station = database.station_for_collector(station_id)?;
    if task == CollectorTask::Balance {
        let (status, _) = fetch_status(database, &station)?;
        let response = client::get_authenticated_json(
            database,
            data_key,
            &station,
            "/api/user/self",
            client::NewApiOperation::SelfInfo,
        )
        .map_err(newapi_request_error_message)?;
        return Ok(build_balance_output(
            station_id,
            &response.data,
            &status,
            response.endpoint_result,
        ));
    }
    if task == CollectorTask::Groups {
        let response = client::get_authenticated_json(
            database,
            data_key,
            &station,
            "/api/user/self/groups",
            client::NewApiOperation::Groups,
        )
        .map_err(newapi_request_error_message)?;
        return Ok(build_groups_output(
            station_id,
            &response.data,
            response.endpoint_result,
        ));
    }
    if task == CollectorTask::Models {
        let response = client::get_authenticated_json(
            database,
            data_key,
            &station,
            "/api/user/models",
            client::NewApiOperation::Models,
        )
        .map_err(newapi_request_error_message)?;
        return Ok(build_models_output(
            station_id,
            &response.data,
            response.endpoint_result,
        ));
    }
    unsupported_output(
        task,
        "unsupported_task",
        "NewAPI adapter does not run this task directly.",
    )
}

fn fetch_status(
    database: &AppDatabase,
    station: &Station,
) -> Result<(parsers::NewApiStatus, Value), String> {
    let settings = database.get_settings()?;
    let proxy = resolve_proxy_config(
        &station.collector_proxy_mode,
        station.collector_proxy_url.clone(),
        &settings.collector_proxy_mode,
        settings.collector_proxy_url,
    );
    let urls = collector_base_urls(&station.base_url);
    let url = join_url(&urls.management_base_url, "/api/status");
    let mut endpoint_results = Vec::new();
    let payload = get_newapi_public_json(&url, &proxy, &mut endpoint_results)?;
    let data = parsers::envelope_data(&payload).unwrap_or(&payload);
    let endpoint_result = endpoint_results.into_iter().next().unwrap_or_else(|| {
        json!({
            "url": url,
            "status": null,
            "ok": false,
        })
    });
    Ok((parsers::parse_status(data), endpoint_result))
}

fn newapi_request_error_message(error: client::NewApiRequestError) -> String {
    match error {
        client::NewApiRequestError::AuthRequired { message, .. }
        | client::NewApiRequestError::ManualRequired { message, .. }
        | client::NewApiRequestError::Transient { message, .. }
        | client::NewApiRequestError::OutcomeUnknown { message, .. }
        | client::NewApiRequestError::Permanent { message, .. } => message,
    }
}

fn get_newapi_public_json(
    url: &str,
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

    #[test]
    fn models_snapshot_keeps_top_level_models_contract() {
        let output = build_models_output(
            "station-1",
            parsers::envelope_data(
                &json!({"success": true, "data": ["gpt-4.1-mini", "claude-sonnet"]}),
            )
            .expect("model data"),
            json!({"path": "/api/user/models", "status": 200, "ok": true}),
        );
        assert_eq!(
            output.normalized_json["models"],
            json!(["gpt-4.1-mini", "claude-sonnet"])
        );
        assert_eq!(output.facts.models.len(), 2);
    }

    #[test]
    fn empty_successful_group_payload_is_partial() {
        let output = build_groups_output(
            "station-1",
            parsers::envelope_data(&json!({"success": true, "data": {}})).expect("group data"),
            json!({"path": "/api/user/self/groups", "status": 200, "ok": true}),
        );
        assert_eq!(output.status, "partial");
        assert_eq!(output.error_code.as_deref(), Some("empty_group_facts"));
    }
}
