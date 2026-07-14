pub mod adapters;
pub mod apply;
pub mod facts;
pub mod session;
pub mod sub2api;

use serde_json::{json, Value};

use crate::{
    models::{
        collector::{
            CollectorEvent, CollectorRunResult, StationLoginTestInput, StationLoginTestResult,
        },
        collector_runs::{CreateCollectorRunInput, FinishCollectorRunInput},
    },
    services::database::AppDatabase,
    services::remote_keys,
};

struct LoginTestAttempt {
    token_present: bool,
    login_message: Option<String>,
    manual_required: Option<String>,
}

pub fn detect_station_info(
    database: &AppDatabase,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    sub2api::detect_station(database, station_id)
}

pub fn collect_station_info(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: String,
) -> Result<CollectorRunResult, String> {
    collect_station_task(
        database,
        data_key,
        station_id,
        adapters::CollectorTask::Full,
    )
}

pub fn collect_station_task(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: String,
    task: adapters::CollectorTask,
) -> Result<CollectorRunResult, String> {
    let station = database.station_for_collector(&station_id)?;
    let adapter = adapter_name_for_station_type(&station.station_type)?;
    let endpoint_revision = station.endpoint_revision;
    if task == adapters::CollectorTask::Full {
        return collect_full_station_task(
            database,
            data_key,
            station_id,
            adapter,
            endpoint_revision,
        );
    }

    let output = dispatch_adapter_output(database, data_key, &station_id, adapter, task);
    let applied =
        apply::apply_adapter_output(database, &station_id, endpoint_revision, None, output)?;
    let mut result = applied.result;
    if task == adapters::CollectorTask::Groups {
        append_remote_key_refresh_event(database, data_key, &station_id, &mut result.events);
    }
    Ok(result)
}

fn collect_full_station_task(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: String,
    adapter: &str,
    endpoint_revision: i64,
) -> Result<CollectorRunResult, String> {
    let parent_run = database.create_collector_run_for_revision(
        CreateCollectorRunInput {
            station_id: station_id.clone(),
            parent_run_id: None,
            adapter: adapter.to_string(),
            task_type: adapters::CollectorTask::Full.as_str().to_string(),
        },
        endpoint_revision,
    )?;

    let mut child_results = Vec::new();
    let mut events = Vec::new();
    for child_task in full_child_tasks(adapter) {
        let output = dispatch_adapter_output(database, data_key, &station_id, adapter, child_task);
        let applied = apply::apply_adapter_output(
            database,
            &station_id,
            endpoint_revision,
            Some(parent_run.id.clone()),
            output,
        )?;
        events.extend(applied.result.events.clone());
        if child_task == adapters::CollectorTask::Groups {
            append_remote_key_refresh_event(database, data_key, &station_id, &mut events);
        }
        child_results.push((applied.result.snapshot, applied.run));
    }

    let endpoint_count = child_results
        .iter()
        .map(|(_, run)| run.endpoint_count)
        .sum::<i64>();
    let success_count = child_results
        .iter()
        .map(|(_, run)| run.success_count)
        .sum::<i64>();
    let failure_count = child_results
        .iter()
        .map(|(_, run)| run.failure_count)
        .sum::<i64>();
    let manual_action_required = child_results
        .iter()
        .any(|(_, run)| run.manual_action_required);
    let status = aggregate_full_status(&child_results);
    let child_snapshots = child_results
        .iter()
        .map(|(snapshot, run)| {
            json!({
                "snapshotId": snapshot.id,
                "runId": run.id,
                "task": run.task_type,
                "status": run.status,
            })
        })
        .collect::<Vec<_>>();
    let child_runs = child_results
        .iter()
        .map(|(_, run)| {
            json!({
                "id": run.id,
                "task": run.task_type,
                "status": run.status,
                "snapshotId": run.snapshot_id,
            })
        })
        .collect::<Vec<_>>();
    let model_names = child_results
        .iter()
        .flat_map(|(snapshot, _)| {
            snapshot
                .normalized_json
                .get("models")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|model| model.as_str().map(ToString::to_string))
        .collect::<Vec<_>>();

    let business_summary = full_business_summary(database, &station_id, model_names)?;
    let snapshot = database.insert_collector_snapshot(
        &station_id,
        &format!("{adapter}-full"),
        &status,
        json!({
            "adapter": adapter,
            "task": "full",
            "conclusion": conclusion_for_full_status(&status),
            "message": full_summary_message(&business_summary),
            "childRuns": child_runs,
            "endpointCount": endpoint_count,
            "successCount": success_count,
            "failureCount": failure_count,
            "recognized": {
                "balanceLabel": business_summary.balance_label,
                "groupCount": business_summary.groups.len(),
                "rateCount": business_summary.rate_multipliers.len(),
                "keyCount": business_summary.key_count,
                "matchedFieldCount": business_summary.matched_field_count(),
            },
        }),
        json!({
            "balance": business_summary.balance_value,
            "balanceLabel": business_summary.balance_label,
            "groups": business_summary.groups,
            "rateMultipliers": business_summary.rate_multipliers,
            "models": business_summary.models,
            "childSnapshots": child_snapshots,
        }),
        None,
        None,
    )?;
    let finished_parent = database.finish_collector_run(FinishCollectorRunInput {
        id: parent_run.id,
        status: status.clone(),
        endpoint_count,
        success_count,
        failure_count,
        manual_action_required,
        error_code: if status == "failed" {
            Some("all_child_tasks_failed".to_string())
        } else {
            None
        },
        error_message: if status == "failed" {
            Some("Full 采集的所有子任务都失败。".to_string())
        } else {
            None
        },
        snapshot_id: Some(snapshot.id.clone()),
    })?;
    events.push(CollectorEvent {
        event_type: "full".to_string(),
        message: format!("Full 采集完成：{}", finished_parent.status),
        status,
    });

    Ok(CollectorRunResult { snapshot, events })
}

#[derive(Debug, Clone)]
struct FullBusinessSummary {
    balance_value: Value,
    balance_label: Option<String>,
    groups: Vec<Value>,
    rate_multipliers: Vec<Value>,
    models: Vec<String>,
    key_count: usize,
}

impl FullBusinessSummary {
    fn matched_field_count(&self) -> usize {
        usize::from(self.balance_label.is_some())
            + self.groups.len()
            + self.rate_multipliers.len()
            + self.models.len()
    }
}

fn full_business_summary(
    database: &AppDatabase,
    station_id: &str,
    models: Vec<String>,
) -> Result<FullBusinessSummary, String> {
    let latest_balance = database
        .list_balance_snapshots()?
        .into_iter()
        .filter(|snapshot| snapshot.station_id == station_id)
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at));
    let balance_value = latest_balance
        .as_ref()
        .and_then(|snapshot| snapshot.value)
        .map(Value::from)
        .unwrap_or(Value::Null);
    let balance_label = latest_balance.as_ref().and_then(|snapshot| {
        snapshot
            .value
            .map(|value| format_balance_label(value, &snapshot.currency))
    });

    let groups = database
        .list_station_group_bindings(station_id.to_string())?
        .into_iter()
        .filter(|binding| {
            binding.binding_kind == "station_group" && binding.binding_status != "missing"
        })
        .map(|binding| {
            json!({
                "id": binding.id,
                "groupName": binding.group_name,
                "status": binding.binding_status,
                "defaultRateMultiplier": binding.default_rate_multiplier,
                "userRateMultiplier": binding.user_rate_multiplier,
                "effectiveRateMultiplier": binding.effective_rate_multiplier,
                "source": binding.rate_source,
                "lastCheckedAt": binding.last_checked_at,
            })
        })
        .collect::<Vec<_>>();

    let rate_multipliers = database
        .list_group_rate_records(station_id.to_string())?
        .into_iter()
        .filter_map(|rate| {
            rate.effective_rate_multiplier.map(|multiplier| {
                json!({
                    "groupName": rate.group_name,
                    "multiplier": multiplier,
                    "defaultRateMultiplier": rate.default_rate_multiplier,
                    "userRateMultiplier": rate.user_rate_multiplier,
                    "source": rate.source,
                    "checkedAt": rate.checked_at,
                })
            })
        })
        .collect::<Vec<_>>();

    let key_count = database
        .list_station_keys(station_id.to_string())?
        .into_iter()
        .filter(|key| key.enabled)
        .count();

    Ok(FullBusinessSummary {
        balance_value,
        balance_label,
        groups,
        rate_multipliers,
        models,
        key_count,
    })
}

fn conclusion_for_full_status(status: &str) -> &'static str {
    match status {
        "success" => "已采集",
        "partial" => "部分采集",
        "manual_required" => "需要登录",
        "failed" => "失败",
        _ => "已检查",
    }
}

fn full_summary_message(summary: &FullBusinessSummary) -> String {
    let mut parts = Vec::new();
    if summary.balance_label.is_some() {
        parts.push("余额");
    }
    if !summary.groups.is_empty() {
        parts.push("分组");
    }
    if !summary.rate_multipliers.is_empty() {
        parts.push("倍率");
    }
    if !summary.models.is_empty() {
        parts.push("模型");
    }

    if parts.is_empty() {
        "Full 采集已完成，但暂未识别到可展示的业务字段。".to_string()
    } else {
        format!("Full 采集已识别{}。", parts.join("、"))
    }
}

fn format_balance_label(value: f64, currency: &str) -> String {
    let mut amount = format!("{value:.6}");
    while amount.contains('.') && amount.ends_with('0') {
        amount.pop();
    }
    if amount.ends_with('.') {
        amount.pop();
    }
    let currency = currency.trim();
    if currency.is_empty() {
        amount
    } else {
        format!("{amount} {currency}")
    }
}

fn dispatch_adapter_output(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    adapter: &str,
    task: adapters::CollectorTask,
) -> adapters::AdapterOutput {
    let result = match adapter {
        "sub2api" => adapters::sub2api::collect(database, data_key, station_id, task),
        "newapi" => adapters::newapi::collect(database, data_key, station_id, task),
        "openai-compatible" => {
            adapters::openai_compatible::collect(database, data_key, station_id, task)
        }
        _ => unreachable!("adapter is validated before dispatch"),
    };

    result.unwrap_or_else(|error| failed_adapter_output(adapter, task, error))
}

fn append_remote_key_refresh_event(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    events: &mut Vec<CollectorEvent>,
) {
    match remote_keys::scan_remote_keys(database, data_key, station_id.to_string()) {
        Ok(scan) => {
            if scan.capability.can_list_remote_keys {
                events.push(CollectorEvent {
                    event_type: "remote_keys".to_string(),
                    message: scan.message,
                    status: "success".to_string(),
                });
            }
        }
        Err(error) => {
            events.push(CollectorEvent {
                event_type: "remote_keys".to_string(),
                message: format!(
                    "远端 Key 刷新失败：{}",
                    crate::services::secrets::mask::redact_text(&error)
                ),
                status: "failed".to_string(),
            });
        }
    }
}

fn failed_adapter_output(
    adapter: &str,
    task: adapters::CollectorTask,
    error: String,
) -> adapters::AdapterOutput {
    let message = crate::services::secrets::mask::redact_text(&error);
    adapters::AdapterOutput {
        adapter: adapter.to_string(),
        task,
        status: "failed".to_string(),
        facts: facts::CollectorFacts::default(),
        summary_json: json!({
            "adapter": adapter,
            "task": task.as_str(),
            "message": message,
            "endpointResults": [],
        }),
        normalized_json: json!({ "models": [] }),
        raw_json_redacted: None,
        error_code: Some("adapter_error".to_string()),
        error_message: Some(message),
    }
}

fn adapter_name_for_station_type(station_type: &str) -> Result<&'static str, String> {
    match station_type.trim() {
        "sub2api" => Ok("sub2api"),
        "newapi" => Ok("newapi"),
        "openai-compatible" | "openai_compatible" | "custom" => Ok("openai-compatible"),
        other => Err(format!("不支持的站点类型: {other}")),
    }
}

fn full_child_tasks(adapter: &str) -> Vec<adapters::CollectorTask> {
    match adapter {
        "newapi" => vec![
            adapters::CollectorTask::Balance,
            adapters::CollectorTask::Groups,
            adapters::CollectorTask::Models,
        ],
        "sub2api" => vec![
            adapters::CollectorTask::Balance,
            adapters::CollectorTask::Groups,
        ],
        "openai-compatible" => vec![adapters::CollectorTask::Models],
        _ => Vec::new(),
    }
}

fn aggregate_full_status(
    child_results: &[(
        crate::models::collector::CollectorSnapshot,
        crate::models::collector_runs::CollectorRun,
    )],
) -> String {
    if child_results.is_empty() {
        return "failed".to_string();
    }
    let success = child_results
        .iter()
        .filter(|(_, run)| run.status == "success")
        .count();
    let partial = child_results
        .iter()
        .filter(|(_, run)| run.status == "partial")
        .count();
    let manual = child_results
        .iter()
        .filter(|(_, run)| run.status == "manual_required")
        .count();

    if success == child_results.len() {
        "success".to_string()
    } else if success > 0 || partial > 0 {
        "partial".to_string()
    } else if manual == child_results.len() {
        "manual_required".to_string()
    } else {
        "failed".to_string()
    }
}

pub fn test_station_login(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: String,
) -> Result<CollectorRunResult, String> {
    let station = database.station_for_collector(&station_id)?;
    let credentials = database.get_station_credentials(station_id.clone())?;

    if !has_login_credentials(&credentials.login_username, credentials.password_present) {
        return Ok(build_status_result(
            station_id,
            station.endpoint_revision,
            station.name,
            "missing_credentials",
            "未填写登录账号或密码，无法测试登录。",
            "缺少账号或密码，先补齐再测试登录。",
        ));
    }

    let password =
        database.get_station_login_password_with_data_key(station_id.clone(), data_key)?;
    let Some(login_password) = password else {
        return Ok(build_status_result(
            station_id,
            station.endpoint_revision,
            station.name,
            "missing_credentials",
            "未保存登录密码，无法测试登录。",
            "未保存登录密码，先在编辑站点里勾选记住密码并保存。",
        ));
    };

    let login_username = credentials.login_username.clone().unwrap_or_default();
    let station_type = station.station_type.trim();
    let login_attempt = match station_type {
        "newapi" => {
            let attempt = adapters::newapi::login_with_password(
                database,
                data_key,
                &station,
                &login_username,
                &login_password,
            )?;
            LoginTestAttempt {
                token_present: attempt.cookie_present,
                login_message: attempt.login_message,
                manual_required: attempt.manual_required,
            }
        }
        _ => {
            let attempt = sub2api::test_login_credentials(
                &station.website_url,
                &login_username,
                &login_password,
            )?;
            LoginTestAttempt {
                token_present: attempt.token_present,
                login_message: attempt.login_message,
                manual_required: attempt.manual_required,
            }
        }
    };
    let login_succeeded = login_attempt.token_present;
    let conclusion = if login_succeeded {
        "登录成功"
    } else {
        "需要处理"
    };
    let message = login_attempt
        .login_message
        .clone()
        .unwrap_or_else(|| "登录测试已完成。".to_string());
    let diagnosis = login_attempt
        .manual_required
        .clone()
        .unwrap_or_else(|| "登录接口已返回可用 token。".to_string());
    let status = if login_succeeded {
        "success"
    } else {
        "manual_required"
    };
    let summary = json!({
        "mode": "login-state",
        "adapter": "Login State Adapter",
        "detectedType": "Login State",
        "conclusion": conclusion,
        "message": message,
        "login": {
            "usernamePresent": !login_username.trim().is_empty(),
            "passwordPresent": !login_password.trim().is_empty(),
            "status": credentials.login_status,
        },
        "loginRequired": !login_succeeded,
        "nextStep": if login_succeeded {
            "登录测试成功。点击采集信息会继续读取余额、分组和倍率。"
        } else {
            "请检查账号密码、验证码、2FA 或站点登录接口字段。"
        },
        "diagnosis": diagnosis,
        "endpointResults": [],
        "recognized": {
            "balanceLabel": "未识别",
            "groupCount": 0,
            "rateCount": 0,
            "keyCount": 0,
            "matchedFieldCount": 0,
        },
        "webviewRequired": true,
        "webviewNote": "网页登录捕获仍是高级兜底功能。",
        "stationName": station.name,
    });
    let normalized = json!({
        "stationId": station_id,
        "adapter": "login-state",
        "status": status,
        "balance": Value::Null,
        "groups": [],
        "rateMultipliers": [],
        "keys": [],
        "models": [],
        "matchedFields": [],
        "detectedEndpoints": [],
        "pendingConfirmations": [],
        "confidenceSummary": { "recognizedFieldCount": 0 },
    });
    let snapshot = database.insert_collector_snapshot(
        &station_id,
        "login-state-test",
        status,
        summary,
        normalized,
        Some(json!({
            "stationName": station.name,
            "loginUsernamePresent": !login_username.trim().is_empty(),
            "loginPasswordPresent": !login_password.trim().is_empty(),
            "note": "测试登录只验证登录接口，不执行余额、分组和倍率采集。",
        })),
        if login_succeeded {
            None
        } else {
            Some(diagnosis)
        },
    )?;

    Ok(CollectorRunResult {
        snapshot,
        events: vec![crate::models::collector::CollectorEvent {
            event_type: "login-test".to_string(),
            message: message.clone(),
            status: status.to_string(),
        }],
    })
}

pub fn test_station_login_input(
    input: StationLoginTestInput,
) -> Result<StationLoginTestResult, String> {
    let website_url = input.website_url.trim();
    let login_username = input.login_username.trim();
    let login_password = input.login_password.trim();

    if website_url.is_empty() {
        return Ok(StationLoginTestResult {
            status: "missing_base_url".to_string(),
            message: "请先填写基础地址。".to_string(),
            diagnosis: None,
            token_present: false,
        });
    }
    if login_username.is_empty() || login_password.is_empty() {
        return Ok(StationLoginTestResult {
            status: "missing_credentials".to_string(),
            message: "请先填写登录用户名和密码。".to_string(),
            diagnosis: None,
            token_present: false,
        });
    }

    let station_type = input.station_type.as_deref().unwrap_or("sub2api").trim();
    let login_attempt = match station_type {
        "newapi" => {
            let attempt = adapters::newapi::test_login_credentials(
                website_url,
                login_username,
                login_password,
            )?;
            LoginTestAttempt {
                token_present: attempt.cookie_present,
                login_message: attempt.login_message,
                manual_required: attempt.manual_required,
            }
        }
        _ => {
            let attempt =
                sub2api::test_login_credentials(website_url, login_username, login_password)?;
            LoginTestAttempt {
                token_present: attempt.token_present,
                login_message: attempt.login_message,
                manual_required: attempt.manual_required,
            }
        }
    };
    let token_present = login_attempt.token_present;
    Ok(StationLoginTestResult {
        status: if token_present {
            "success"
        } else {
            "manual_required"
        }
        .to_string(),
        message: login_attempt
            .login_message
            .unwrap_or_else(|| "连通性测试已完成。".to_string()),
        diagnosis: login_attempt.manual_required.or_else(|| {
            if token_present {
                Some("登录接口返回可用 token。".to_string())
            } else {
                None
            }
        }),
        token_present,
    })
}

fn build_status_result(
    station_id: String,
    endpoint_revision: i64,
    station_name: String,
    status: &str,
    conclusion: &str,
    message: &str,
) -> CollectorRunResult {
    let snapshot = crate::models::collector::CollectorSnapshot {
        id: format!(
            "snapshot-{}",
            crate::services::database::now_millis_for_services()
        ),
        station_id: station_id.clone(),
        endpoint_revision,
        source: "login-state-collect".to_string(),
        status: status.to_string(),
        fetched_at: crate::services::database::now_millis_for_services().to_string(),
        summary_json: json!({
            "mode": "login-state",
            "adapter": "Login State Adapter",
            "detectedType": "Login State",
            "conclusion": conclusion,
            "message": message,
            "endpointResults": [],
            "recognized": {
                "balanceLabel": "未识别",
                "groupCount": 0,
                "rateCount": 0,
                "keyCount": 0,
                "matchedFieldCount": 0,
            },
            "webviewRequired": true,
            "webviewNote": "网页登录捕获仍是高级兜底功能。",
            "stationName": station_name,
        }),
        normalized_json: json!({
            "stationId": station_id,
            "adapter": "login-state",
            "status": status,
            "balance": Value::Null,
            "groups": [],
            "rateMultipliers": [],
            "keys": [],
            "models": [],
            "matchedFields": [],
            "detectedEndpoints": [],
            "pendingConfirmations": [],
            "confidenceSummary": { "recognizedFieldCount": 0 },
        }),
        raw_json_redacted: Some(json!({
            "stationName": station_name,
            "message": message,
        })),
        error_message: Some(message.to_string()),
        created_at: crate::services::database::now_millis_for_services().to_string(),
    };

    CollectorRunResult {
        snapshot,
        events: vec![crate::models::collector::CollectorEvent {
            event_type: "login-state".to_string(),
            message: message.to_string(),
            status: status.to_string(),
        }],
    }
}

fn has_login_credentials(username: &Option<String>, password_present: bool) -> bool {
    username
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
        && password_present
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{credentials::UpdateStationCredentialsInput, stations::CreateStationInput},
        services::{database::AppDatabase, secrets::crypto::generate_data_key},
    };
    use serde_json::Value;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{Arc, Mutex},
        thread,
    };

    #[test]
    fn login_requires_username_and_password() {
        assert!(!has_login_credentials(&None, false));
        assert!(!has_login_credentials(
            &Some("user@example.com".to_string()),
            false
        ));
        assert!(!has_login_credentials(&None, true));
        assert!(has_login_credentials(
            &Some("user@example.com".to_string()),
            true
        ));
    }

    #[test]
    fn test_station_login_uses_saved_password() {
        let server = TestLoginServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = database
            .create_station(CreateStationInput {
                name: "login test".to_string(),
                station_type: "sub2api".to_string(),
                website_url: server.base_url.clone(),
                api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-test-routing".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        let data_key = generate_data_key();
        database
            .update_station_credentials_with_data_key(
                UpdateStationCredentialsInput {
                    station_id: station.id.clone(),
                    login_username: Some("user@example.test".to_string()),
                    login_password: Some("correct-password".to_string()),
                    remember_password: true,
                },
                &data_key,
            )
            .expect("credentials");

        let result = test_station_login(&database, &data_key, station.id).expect("login test");

        assert_eq!(result.snapshot.status, "success");
        assert_eq!(
            server.last_login_password(),
            Some("correct-password".to_string())
        );
        assert_eq!(
            result
                .snapshot
                .summary_json
                .get("loginRequired")
                .and_then(Value::as_bool),
            Some(false),
        );
    }

    #[test]
    fn test_station_login_dispatches_newapi_password_login() {
        let server = TestNewApiLoginServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = database
            .create_station(CreateStationInput {
                name: "newapi login test".to_string(),
                station_type: "newapi".to_string(),
                website_url: server.base_url.clone(),
                api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-test-routing".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        let data_key = generate_data_key();
        database
            .update_station_credentials_with_data_key(
                UpdateStationCredentialsInput {
                    station_id: station.id.clone(),
                    login_username: Some("user@example.test".to_string()),
                    login_password: Some("correct-password".to_string()),
                    remember_password: true,
                },
                &data_key,
            )
            .expect("credentials");

        let result =
            test_station_login(&database, &data_key, station.id.clone()).expect("login test");
        let session = database
            .resolve_station_session_with_data_key(station.id.clone(), &data_key, 100_000)
            .expect("session");

        assert_eq!(result.snapshot.status, "success");
        assert_eq!(
            server.last_login_password(),
            Some("correct-password".to_string())
        );
        assert_eq!(session.newapi_user_id.as_deref(), Some("42"));
        assert_eq!(
            session.status,
            crate::services::collectors::session::SessionResolveStatus::Ready
        );
        assert_eq!(session.cookie.as_deref(), Some("session=newapi-login"));
    }

    #[test]
    fn test_station_login_input_dispatches_newapi_when_station_type_is_present() {
        let server = TestNewApiLoginServer::start();

        let result = test_station_login_input(StationLoginTestInput {
            station_type: Some("newapi".to_string()),
            website_url: server.base_url.clone(),
            login_username: "user@example.test".to_string(),
            login_password: "correct-password".to_string(),
        })
        .expect("login input test");

        assert_eq!(result.status, "success");
        assert_eq!(
            server.last_login_password(),
            Some("correct-password".to_string())
        );
        assert!(result.token_present);
    }

    #[test]
    fn full_snapshot_summarizes_child_business_facts() {
        let server = TestFullServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "full collect".to_string(),
                    station_type: "sub2api".to_string(),
                    website_url: server.base_url.clone(),
                    api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                    collector_proxy_mode: "inherit".to_string(),
                    collector_proxy_url: None,
                    api_key: "sk-route-key".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        database
            .update_station_credentials_with_data_key(
                UpdateStationCredentialsInput {
                    station_id: station.id.clone(),
                    login_username: Some("user@example.test".to_string()),
                    login_password: Some("correct-password".to_string()),
                    remember_password: true,
                },
                &data_key,
            )
            .expect("credentials");

        let result = collect_station_task(
            &database,
            &data_key,
            station.id,
            adapters::CollectorTask::Full,
        )
        .expect("full collect");
        let summary = &result.snapshot.summary_json;
        let normalized = &result.snapshot.normalized_json;

        assert_eq!(result.snapshot.status, "success");
        assert_eq!(
            summary
                .pointer("/recognized/balanceLabel")
                .and_then(Value::as_str),
            Some("42.5 CNY")
        );
        assert_eq!(
            summary
                .pointer("/recognized/groupCount")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            summary
                .pointer("/recognized/rateCount")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            normalized
                .get("groups")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            normalized
                .get("rateMultipliers")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
    }

    #[test]
    fn newapi_full_child_tasks_include_models_after_groups() {
        assert_eq!(
            full_child_tasks("newapi"),
            vec![
                adapters::CollectorTask::Balance,
                adapters::CollectorTask::Groups,
                adapters::CollectorTask::Models,
            ]
        );
    }

    #[test]
    fn group_collection_refreshes_remote_key_discoveries() {
        let server = TestGroupsAndKeysServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "groups and keys collect".to_string(),
                    station_type: "sub2api".to_string(),
                    website_url: server.base_url.clone(),
                    api_base_url: format!("{}/v1", server.base_url.trim_end_matches('/')),
                    collector_proxy_mode: "inherit".to_string(),
                    collector_proxy_url: None,
                    api_key: "sk-route-key".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        database
            .update_station_credentials_with_data_key(
                UpdateStationCredentialsInput {
                    station_id: station.id.clone(),
                    login_username: Some("user@example.test".to_string()),
                    login_password: Some("correct-password".to_string()),
                    remember_password: true,
                },
                &data_key,
            )
            .expect("credentials");

        let result = collect_station_task(
            &database,
            &data_key,
            station.id.clone(),
            adapters::CollectorTask::Groups,
        )
        .expect("group collect");
        let remote_keys = database
            .list_remote_station_keys(station.id)
            .expect("remote keys");

        assert_eq!(result.snapshot.status, "success");
        assert_eq!(remote_keys.len(), 1);
        assert_eq!(
            remote_keys[0].remote_key_name.as_deref(),
            Some("Auto Pro Key")
        );
        assert_eq!(remote_keys[0].group_name.as_deref(), Some("Pro"));
        assert_eq!(remote_keys[0].rate_multiplier, Some(1.25));
        assert_eq!(
            remote_keys[0].rate_source.as_deref(),
            Some("sub2api_groups_rates")
        );
    }

    struct TestLoginServer {
        base_url: String,
        last_login_password: Arc<Mutex<Option<String>>>,
    }

    struct TestNewApiLoginServer {
        base_url: String,
        last_login_password: Arc<Mutex<Option<String>>>,
    }

    struct TestFullServer {
        base_url: String,
    }

    struct TestGroupsAndKeysServer {
        base_url: String,
    }

    impl TestLoginServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            let last_login_password = Arc::new(Mutex::new(None));
            let captured_password = Arc::clone(&last_login_password);

            thread::spawn(move || {
                for stream in listener.incoming().take(3).flatten() {
                    handle_login_request(stream, Arc::clone(&captured_password));
                }
            });

            Self {
                base_url,
                last_login_password,
            }
        }

        fn last_login_password(&self) -> Option<String> {
            self.last_login_password
                .lock()
                .ok()
                .and_then(|value| value.clone())
        }
    }

    impl TestNewApiLoginServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            let last_login_password = Arc::new(Mutex::new(None));
            let captured_password = Arc::clone(&last_login_password);

            thread::spawn(move || {
                for stream in listener.incoming().take(3).flatten() {
                    handle_newapi_login_request(stream, Arc::clone(&captured_password));
                }
            });

            Self {
                base_url,
                last_login_password,
            }
        }

        fn last_login_password(&self) -> Option<String> {
            self.last_login_password
                .lock()
                .ok()
                .and_then(|value| value.clone())
        }
    }

    impl TestFullServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            thread::spawn(move || {
                for stream in listener.incoming().take(5).flatten() {
                    handle_full_request(stream);
                }
            });
            Self { base_url }
        }
    }

    impl TestGroupsAndKeysServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            thread::spawn(move || {
                for stream in listener.incoming().take(4).flatten() {
                    handle_groups_and_keys_request(stream);
                }
            });
            Self { base_url }
        }
    }

    fn handle_login_request(
        mut stream: TcpStream,
        last_login_password: Arc<Mutex<Option<String>>>,
    ) {
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let body = request.split("\r\n\r\n").nth(1).unwrap_or("");

        let (status, response) = if path == "/api/v1/auth/login" {
            let parsed = serde_json::from_str::<Value>(body).expect("login json");
            let password = parsed
                .get("password")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if let Ok(mut captured) = last_login_password.lock() {
                *captured = Some(password.clone());
            }
            if password == "correct-password" {
                (
                    "200 OK",
                    json!({ "data": { "access_token": "login-test-token-secret" } }),
                )
            } else {
                (
                    "401 Unauthorized",
                    json!({ "message": "invalid credentials" }),
                )
            }
        } else {
            ("404 Not Found", json!({ "message": "not found" }))
        };

        let text = response.to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn handle_newapi_login_request(
        mut stream: TcpStream,
        last_login_password: Arc<Mutex<Option<String>>>,
    ) {
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let body = request.split("\r\n\r\n").nth(1).unwrap_or("");

        let (status, response, set_cookie) = if path == "/api/user/login" {
            let parsed = serde_json::from_str::<Value>(body).expect("login json");
            let password = parsed
                .get("password")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if let Ok(mut captured) = last_login_password.lock() {
                *captured = Some(password.clone());
            }
            if password == "correct-password" {
                (
                    "200 OK",
                    json!({ "success": true, "data": { "id": 42 } }),
                    Some("session=newapi-login; Path=/; HttpOnly"),
                )
            } else {
                (
                    "401 Unauthorized",
                    json!({ "message": "invalid credentials" }),
                    None,
                )
            }
        } else {
            ("404 Not Found", json!({ "message": "not found" }), None)
        };

        let text = response.to_string();
        let cookie_header = set_cookie
            .map(|cookie| format!("Set-Cookie: {cookie}\r\n"))
            .unwrap_or_default();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{cookie_header}Content-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn handle_full_request(mut stream: TcpStream) {
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
        let authorized = request
            .to_lowercase()
            .contains("authorization: bearer full-collector-token");

        let (status, response) = match path {
            "/v1/usage" => (
                "200 OK",
                json!({
                    "quota": {
                        "remaining": 42.5,
                        "used": 7.5,
                        "total": 50.0,
                        "unit": "CNY"
                    }
                }),
            ),
            "/api/v1/auth/login" => {
                let parsed = serde_json::from_str::<Value>(body).expect("login json");
                if parsed.get("password").and_then(Value::as_str) == Some("correct-password") {
                    (
                        "200 OK",
                        json!({ "data": { "access_token": "full-collector-token" } }),
                    )
                } else {
                    (
                        "401 Unauthorized",
                        json!({ "message": "invalid credentials" }),
                    )
                }
            }
            "/api/v1/groups/available" if authorized => (
                "200 OK",
                json!({
                    "data": [
                        { "id": "default", "name": "Default", "rate_multiplier": 1.0 },
                        { "id": "pro", "name": "Pro", "rate_multiplier": 1.5 }
                    ]
                }),
            ),
            "/api/v1/groups/rates" if authorized => {
                ("200 OK", json!({ "data": { "default": 0.8, "pro": 1.2 } }))
            }
            "/api/v1/usage/dashboard/stats" if authorized => (
                "200 OK",
                json!({
                    "data": {
                        "today_requests": 12,
                        "total_requests": 1200,
                        "today_actual_cost": 0.75,
                        "total_actual_cost": 18.5,
                        "today_tokens": 34567,
                        "total_tokens": 4567890,
                        "today_prompt_tokens": 30000,
                        "today_completion_tokens": 4567,
                        "prompt_tokens": 4300000,
                        "completion_tokens": 267890
                    }
                }),
            ),
            _ => ("404 Not Found", json!({ "message": "not found" })),
        };
        let text = response.to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn handle_groups_and_keys_request(mut stream: TcpStream) {
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
        let authorized = request
            .to_lowercase()
            .contains("authorization: bearer groups-and-keys-token");

        let (status, response) = match path {
            "/api/v1/auth/login" => {
                let parsed = serde_json::from_str::<Value>(body).expect("login json");
                if parsed.get("password").and_then(Value::as_str) == Some("correct-password") {
                    (
                        "200 OK",
                        json!({ "data": { "access_token": "groups-and-keys-token" } }),
                    )
                } else {
                    (
                        "401 Unauthorized",
                        json!({ "message": "invalid credentials" }),
                    )
                }
            }
            "/api/v1/groups/available" if authorized => (
                "200 OK",
                json!({
                    "data": [
                        { "id": "pro", "name": "Pro", "rate_multiplier": 1.5 }
                    ]
                }),
            ),
            "/api/v1/groups/rates" if authorized => ("200 OK", json!({ "data": { "pro": 1.25 } })),
            "/api/v1/keys?page=1&page_size=100" if authorized => (
                "200 OK",
                json!({
                    "data": {
                        "items": [{
                            "id": "remote-pro-key",
                            "name": "Auto Pro Key",
                            "masked_key": "sk-auto****7890",
                            "group_id": "pro"
                        }]
                    }
                }),
            ),
            _ => ("404 Not Found", json!({ "message": "not found" })),
        };
        let text = response.to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 1024];
        let mut header_end = None;
        let mut content_length = 0_usize;

        loop {
            let read = stream.read(&mut buffer).expect("read request");
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            if header_end.is_none() {
                if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    header_end = Some(position + 4);
                    let headers = String::from_utf8_lossy(&bytes[..position]);
                    content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            if name.eq_ignore_ascii_case("content-length") {
                                value.trim().parse::<usize>().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                }
            }
            if let Some(end) = header_end {
                if bytes.len().saturating_sub(end) >= content_length {
                    break;
                }
            }
        }

        String::from_utf8_lossy(&bytes).to_string()
    }
}
