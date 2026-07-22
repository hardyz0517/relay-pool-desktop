pub mod adapters;
pub(crate) mod collector_apply;
pub mod facts;
pub mod sub2api;

// Preserve the crate-local composition path while the V2 apply boundary is
// owned by the collector consumer rather than a legacy persistence module.
pub(crate) mod apply {
    pub(crate) use super::collector_apply::{CollectorApplyPort, V2CollectorApplyAdapter};
}
use std::sync::Arc;

use serde_json::{json, Value};

use crate::{
    application::{
        collectors::{CollectorApplyOutcome, CollectorService},
        credentials::CredentialService,
        error::ApplicationError,
        settings::SettingsService,
    },
    models::{
        collector::{CollectorRunResult, StationLoginTestInput, StationLoginTestResult},
        credentials::{
            PersistStationSessionInput, ResolvedSession, StationCredentials,
            StationSessionCredentialKind, UpdateStationSessionInput,
        },
        group_facts::StationGroupBinding,
        settings::AppSettings,
        station_keys::StationKey,
        stations::Station,
    },
};

use collector_apply::CollectorApplyPort;

/// Consumer-owned read/write boundary required by provider HTTP adapters.
///
/// Production composition supplies this port from catalog, settings, and
/// credential application services.
pub(crate) trait CollectorSourcePort: Send + Sync {
    fn station_for_collector(&self, station_id: &str) -> Result<Station, String>;
    fn get_settings(&self) -> Result<AppSettings, String>;
    fn list_station_keys(&self, station_id: String) -> Result<Vec<StationKey>, String>;
    fn resolve_station_key_secret_with_data_key(
        &self,
        data_key: &[u8; 32],
        station_key_id: &str,
    ) -> Result<String, String>;
    fn get_station_credentials(&self, station_id: String) -> Result<StationCredentials, String>;
    fn get_station_login_password_with_data_key(
        &self,
        station_id: String,
        data_key: &[u8; 32],
    ) -> Result<Option<String>, String>;
    fn resolve_station_session_with_data_key(
        &self,
        station_id: String,
        data_key: &[u8; 32],
        now_ms: i64,
    ) -> Result<ResolvedSession, String>;
    fn update_station_session_with_data_key(
        &self,
        input: UpdateStationSessionInput,
        data_key: &[u8; 32],
        expected_revision: i64,
    ) -> Result<StationCredentials, String>;
    fn persist_station_session_with_data_key(
        &self,
        input: PersistStationSessionInput,
        data_key: &[u8; 32],
        expected_revision: i64,
    ) -> Result<StationCredentials, String>;
    fn invalidate_station_session_credential(
        &self,
        station_id: &str,
        kind: StationSessionCredentialKind,
    ) -> Result<(), String>;
    fn list_station_group_bindings(
        &self,
        station_id: String,
    ) -> Result<Vec<StationGroupBinding>, String>;
}

#[derive(Clone)]
pub(crate) struct V2CollectorSourceAdapter {
    collectors: Arc<CollectorService>,
    credentials: Arc<CredentialService>,
    settings: Arc<SettingsService>,
}

impl V2CollectorSourceAdapter {
    pub(crate) fn new(
        collectors: Arc<CollectorService>,
        credentials: Arc<CredentialService>,
        settings: Arc<SettingsService>,
    ) -> Self {
        Self {
            collectors,
            credentials,
            settings,
        }
    }
}

impl CollectorSourcePort for V2CollectorSourceAdapter {
    fn station_for_collector(&self, station_id: &str) -> Result<Station, String> {
        tauri::async_runtime::block_on(self.collectors.station_for_collection(station_id))
            .map_err(application_error)
    }

    fn get_settings(&self) -> Result<AppSettings, String> {
        tauri::async_runtime::block_on(self.settings.load()).map_err(application_error)
    }

    fn list_station_keys(&self, station_id: String) -> Result<Vec<StationKey>, String> {
        tauri::async_runtime::block_on(self.credentials.list_station_keys(station_id))
            .map_err(application_error)
    }

    fn resolve_station_key_secret_with_data_key(
        &self,
        _data_key: &[u8; 32],
        station_key_id: &str,
    ) -> Result<String, String> {
        let secret = tauri::async_runtime::block_on(
            self.credentials
                .resolve_station_key_secret(station_key_id.to_string()),
        )
        .map_err(application_error)?;
        String::from_utf8(secret.as_bytes().to_vec())
            .map_err(|_| "station key secret is not valid UTF-8".to_string())
    }

    fn get_station_credentials(&self, station_id: String) -> Result<StationCredentials, String> {
        tauri::async_runtime::block_on(self.credentials.get_station_credentials(station_id))
            .map_err(application_error)
    }

    fn get_station_login_password_with_data_key(
        &self,
        station_id: String,
        _data_key: &[u8; 32],
    ) -> Result<Option<String>, String> {
        let secret =
            tauri::async_runtime::block_on(self.credentials.get_station_login_password(station_id))
                .map_err(application_error)?;
        secret
            .map(|secret| {
                String::from_utf8(secret.as_bytes().to_vec())
                    .map_err(|_| "station login password is not valid UTF-8".to_string())
            })
            .transpose()
    }

    fn resolve_station_session_with_data_key(
        &self,
        station_id: String,
        _data_key: &[u8; 32],
        now_ms: i64,
    ) -> Result<ResolvedSession, String> {
        tauri::async_runtime::block_on(self.credentials.resolve_station_session(station_id, now_ms))
            .map_err(application_error)
    }

    fn update_station_session_with_data_key(
        &self,
        input: UpdateStationSessionInput,
        _data_key: &[u8; 32],
        expected_revision: i64,
    ) -> Result<StationCredentials, String> {
        tauri::async_runtime::block_on(
            self.credentials
                .update_station_session_if_revision(input, expected_revision),
        )
        .map_err(application_error)
    }

    fn persist_station_session_with_data_key(
        &self,
        input: PersistStationSessionInput,
        _data_key: &[u8; 32],
        expected_revision: i64,
    ) -> Result<StationCredentials, String> {
        tauri::async_runtime::block_on(
            self.credentials
                .persist_station_session_if_revision(input, expected_revision),
        )
        .map_err(application_error)
    }

    fn invalidate_station_session_credential(
        &self,
        station_id: &str,
        kind: StationSessionCredentialKind,
    ) -> Result<(), String> {
        tauri::async_runtime::block_on(
            self.credentials
                .invalidate_station_session_credential(station_id.to_string(), kind),
        )
        .map_err(application_error)
    }

    fn list_station_group_bindings(
        &self,
        station_id: String,
    ) -> Result<Vec<StationGroupBinding>, String> {
        tauri::async_runtime::block_on(self.collectors.list_station_group_bindings(&station_id))
            .map_err(application_error)
    }
}

fn application_error(error: ApplicationError) -> String {
    error.to_string()
}

struct LoginTestAttempt {
    token_present: bool,
    login_message: Option<String>,
    manual_required: Option<String>,
}

pub(crate) fn prepare_station_task_v2(
    source: &dyn CollectorSourcePort,
    data_key: &[u8; 32],
    station_id: String,
    task: adapters::CollectorTask,
) -> Result<(String, i64, adapters::AdapterOutput), ApplicationError> {
    if task == adapters::CollectorTask::Full {
        return Err(ApplicationError::ConstraintViolation);
    }
    let station = source
        .station_for_collector(&station_id)
        .map_err(|_| ApplicationError::Internal)?;
    let adapter = adapter_name_for_station_type(&station.station_type)
        .map_err(|_| ApplicationError::ConstraintViolation)?;
    let output = dispatch_adapter_output(source, data_key, &station_id, adapter, task);
    Ok((station_id, station.endpoint_revision, output))
}

pub(crate) async fn apply_prepared_station_task_v2(
    port: &dyn CollectorApplyPort,
    station_id: String,
    endpoint_revision: i64,
    output: adapters::AdapterOutput,
) -> Result<CollectorApplyOutcome, ApplicationError> {
    collector_apply::apply_station_output_v2(port, station_id, endpoint_revision, None, output)
        .await
}

#[derive(Debug)]
pub(crate) struct PreparedStationCollection {
    station_id: String,
    endpoint_revision: i64,
    adapter: String,
    task: adapters::CollectorTask,
    outputs: Vec<adapters::AdapterOutput>,
    enabled_key_count: usize,
}

/// Performs all source reads and provider HTTP calls before the async apply phase.
pub(crate) fn prepare_station_collection_v2(
    source: &dyn CollectorSourcePort,
    data_key: &[u8; 32],
    station_id: String,
    task: adapters::CollectorTask,
) -> Result<PreparedStationCollection, ApplicationError> {
    let station = source
        .station_for_collector(&station_id)
        .map_err(|_| ApplicationError::Internal)?;
    let adapter = adapter_name_for_station_type(&station.station_type)
        .map_err(|_| ApplicationError::ConstraintViolation)?
        .to_string();
    let tasks = if task == adapters::CollectorTask::Full {
        full_child_tasks(&adapter)
    } else {
        vec![task]
    };
    if tasks.is_empty() || tasks.len() > 3 {
        return Err(ApplicationError::ConstraintViolation);
    }
    let outputs = tasks
        .into_iter()
        .map(|child_task| {
            dispatch_adapter_output(source, data_key, &station_id, &adapter, child_task)
        })
        .collect();
    let enabled_key_count = if task == adapters::CollectorTask::Full {
        source
            .list_station_keys(station_id.clone())
            .map_err(|_| ApplicationError::Internal)?
            .into_iter()
            .filter(|key| key.enabled)
            .count()
    } else {
        0
    };
    Ok(PreparedStationCollection {
        station_id,
        endpoint_revision: station.endpoint_revision,
        adapter,
        task,
        outputs,
        enabled_key_count,
    })
}

/// Applies a prepared task through V2 and returns a complete, bounded read model.
pub(crate) async fn apply_prepared_station_collection_v2(
    service: &CollectorService,
    port: &dyn CollectorApplyPort,
    prepared: PreparedStationCollection,
) -> Result<CollectorRunResult, ApplicationError> {
    if prepared.task != adapters::CollectorTask::Full {
        let output = prepared
            .outputs
            .into_iter()
            .next()
            .ok_or(ApplicationError::ConstraintViolation)?;
        let task_type = if output.adapter == "login-state" {
            "login-test".to_string()
        } else {
            output.task.as_str().to_string()
        };
        let outcome = collector_apply::apply_station_output_v2(
            port,
            prepared.station_id,
            prepared.endpoint_revision,
            None,
            output,
        )
        .await?;
        return service.result_for_apply(&outcome, &task_type).await;
    }

    apply_prepared_full_collection_v2(service, port, prepared).await
}

pub(crate) fn prepare_station_login_test_v2(
    source: &dyn CollectorSourcePort,
    data_key: &[u8; 32],
    station_id: String,
) -> Result<PreparedStationCollection, ApplicationError> {
    let station = source
        .station_for_collector(&station_id)
        .map_err(|_| ApplicationError::Internal)?;
    let credentials = source
        .get_station_credentials(station_id.clone())
        .map_err(|_| ApplicationError::Internal)?;
    let username = credentials.login_username.clone().unwrap_or_default();
    let password = source
        .get_station_login_password_with_data_key(station_id.clone(), data_key)
        .map_err(|_| ApplicationError::Internal)?;

    let (status, message, diagnosis, token_present) =
        if !has_login_credentials(&credentials.login_username, credentials.password_present)
            || password
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            (
                "manual_required".to_string(),
                "Saved login credentials are incomplete.".to_string(),
                "Save both the login username and password before testing login.".to_string(),
                false,
            )
        } else {
            let password_value = password.as_deref().unwrap_or_default();
            let attempt = if station.station_type.trim() == "newapi" {
                let outcome = adapters::newapi::login_with_password(
                    source,
                    data_key,
                    &station,
                    &username,
                    password_value,
                )
                .map_err(|_| ApplicationError::Internal)?;
                LoginTestAttempt {
                    token_present: outcome.cookie_present,
                    login_message: outcome.login_message,
                    manual_required: outcome.manual_required,
                }
            } else {
                let outcome = sub2api::test_login_credentials(
                    &station.website_url,
                    &username,
                    password_value,
                )
                .map_err(|_| ApplicationError::Internal)?;
                LoginTestAttempt {
                    token_present: outcome.token_present,
                    login_message: outcome.login_message,
                    manual_required: outcome.manual_required,
                }
            };
            let status = if attempt.token_present {
                "success"
            } else {
                "manual_required"
            };
            (
                status.to_string(),
                attempt
                    .login_message
                    .unwrap_or_else(|| "Login test completed.".to_string()),
                attempt.manual_required.unwrap_or_else(|| {
                    "The login endpoint returned a usable session credential.".to_string()
                }),
                attempt.token_present,
            )
        };
    let output = adapters::AdapterOutput {
        adapter: "login-state".to_string(),
        task: adapters::CollectorTask::Detect,
        status: status.clone(),
        facts: facts::CollectorFacts::default(),
        summary_json: json!({
            "mode": "login-state",
            "adapter": "Login State Adapter",
            "detectedType": "Login State",
            "conclusion": if token_present { "Login succeeded" } else { "Action required" },
            "message": message,
            "login": {
                "usernamePresent": !username.trim().is_empty(),
                "passwordPresent": password.as_deref().is_some_and(|value| !value.trim().is_empty()),
                "status": credentials.login_status,
            },
            "loginRequired": !token_present,
            "diagnosis": diagnosis,
            "endpointResults": [],
            "recognized": {
                "balanceLabel": Value::Null,
                "groupCount": 0,
                "rateCount": 0,
                "keyCount": 0,
                "matchedFieldCount": 0,
            },
            "stationName": station.name,
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
            "stationName": station.name,
            "loginUsernamePresent": !username.trim().is_empty(),
            "loginPasswordPresent": password.as_deref().is_some_and(|value| !value.trim().is_empty()),
        })),
        error_code: (!token_present).then(|| "login_action_required".to_string()),
        error_message: (!token_present).then_some(diagnosis),
    };
    Ok(PreparedStationCollection {
        station_id,
        endpoint_revision: station.endpoint_revision,
        adapter: "login-state".to_string(),
        task: adapters::CollectorTask::Detect,
        outputs: vec![output],
        enabled_key_count: 0,
    })
}

async fn apply_prepared_full_collection_v2(
    service: &CollectorService,
    port: &dyn CollectorApplyPort,
    prepared: PreparedStationCollection,
) -> Result<CollectorRunResult, ApplicationError> {
    let full_output = aggregate_full_output_v2(&prepared);
    let parent_outcome = collector_apply::apply_station_output_v2(
        port,
        prepared.station_id.clone(),
        prepared.endpoint_revision,
        None,
        full_output,
    )
    .await?;
    let mut events = Vec::with_capacity(prepared.outputs.len() + 1);
    for output in &prepared.outputs {
        let outcome = collector_apply::apply_station_output_v2(
            port,
            prepared.station_id.clone(),
            prepared.endpoint_revision,
            Some(parent_outcome.run_id.clone()),
            output.clone(),
        )
        .await?;
        let result = service
            .result_for_apply(&outcome, output.task.as_str())
            .await?;
        events.extend(result.events);
    }
    let parent_result = service.result_for_apply(&parent_outcome, "full").await?;
    events.extend(parent_result.events);
    Ok(CollectorRunResult {
        snapshot: parent_result.snapshot,
        events,
    })
}

fn aggregate_full_output_v2(prepared: &PreparedStationCollection) -> adapters::AdapterOutput {
    let status = aggregate_full_output_status(&prepared.outputs);
    let endpoint_results = prepared
        .outputs
        .iter()
        .flat_map(|output| {
            output
                .summary_json
                .get("endpointResults")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    let child_runs = prepared
        .outputs
        .iter()
        .map(|output| {
            json!({
                "task": output.task.as_str(),
                "status": output.status,
            })
        })
        .collect::<Vec<_>>();
    let business =
        full_business_summary_from_outputs(&prepared.outputs, prepared.enabled_key_count);
    let models = prepared
        .outputs
        .iter()
        .flat_map(|output| output.facts.models.iter().cloned())
        .collect::<Vec<_>>();
    let error_message =
        (status == "failed").then(|| "all full collector child tasks failed".to_string());

    adapters::AdapterOutput {
        adapter: prepared.adapter.clone(),
        task: adapters::CollectorTask::Full,
        status: status.clone(),
        facts: facts::CollectorFacts {
            models,
            ..facts::CollectorFacts::default()
        },
        summary_json: json!({
            "adapter": prepared.adapter,
            "task": "full",
            "conclusion": conclusion_for_full_status(&status),
            "message": full_summary_message(&business),
            "childRuns": child_runs,
            "endpointResults": endpoint_results,
            "recognized": {
                "balanceLabel": business.balance_label,
                "groupCount": business.groups.len(),
                "rateCount": business.rate_multipliers.len(),
                "keyCount": business.key_count,
                "matchedFieldCount": business.matched_field_count(),
            },
        }),
        normalized_json: json!({
            "balance": business.balance_value,
            "balanceLabel": business.balance_label,
            "groups": business.groups,
            "rateMultipliers": business.rate_multipliers,
            "models": business.models,
            "childRuns": child_runs,
        }),
        raw_json_redacted: None,
        error_code: (status == "failed").then(|| "all_child_tasks_failed".to_string()),
        error_message,
    }
}

fn aggregate_full_output_status(outputs: &[adapters::AdapterOutput]) -> String {
    let success = outputs
        .iter()
        .filter(|output| output.status == "success")
        .count();
    let partial = outputs
        .iter()
        .filter(|output| output.status == "partial")
        .count();
    let manual = outputs
        .iter()
        .filter(|output| output.status == "manual_required")
        .count();
    if success == outputs.len() {
        "success".to_string()
    } else if success > 0 || partial > 0 {
        "partial".to_string()
    } else if manual == outputs.len() {
        "manual_required".to_string()
    } else {
        "failed".to_string()
    }
}

fn full_business_summary_from_outputs(
    outputs: &[adapters::AdapterOutput],
    key_count: usize,
) -> FullBusinessSummary {
    let balances = outputs.iter().flat_map(|output| &output.facts.balances);
    let latest_balance = balances
        .filter(|balance| balance.value.is_some())
        .max_by_key(|balance| balance.scope == "station");
    let balance_value = latest_balance
        .and_then(|balance| balance.value)
        .map(Value::from)
        .unwrap_or(Value::Null);
    let balance_label = latest_balance.and_then(|balance| {
        balance
            .value
            .map(|value| format_balance_label(value, &balance.currency))
    });
    let groups = outputs
        .iter()
        .flat_map(|output| &output.facts.groups)
        .map(|group| {
            json!({
                "groupName": group.group_name,
                "status": group.visibility,
                "source": group.source,
            })
        })
        .collect();
    let rate_multipliers = outputs
        .iter()
        .flat_map(|output| &output.facts.rates)
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
        .collect();
    let models = outputs
        .iter()
        .flat_map(|output| &output.facts.models)
        .filter(|model| model.available)
        .map(|model| model.model.clone())
        .collect();
    FullBusinessSummary {
        balance_value,
        balance_label,
        groups,
        rate_multipliers,
        models,
        key_count,
    }
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
    source: &dyn CollectorSourcePort,
    data_key: &[u8; 32],
    station_id: &str,
    adapter: &str,
    task: adapters::CollectorTask,
) -> adapters::AdapterOutput {
    let result = match adapter {
        "sub2api" => adapters::sub2api::collect(source, data_key, station_id, task),
        "newapi" => adapters::newapi::collect(source, data_key, station_id, task),
        "openai-compatible" => {
            adapters::openai_compatible::collect(source, data_key, station_id, task)
        }
        _ => unreachable!("adapter is validated before dispatch"),
    };

    result.unwrap_or_else(|error| failed_adapter_output(adapter, task, error))
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

    #[test]
    fn login_requires_username_and_password() {
        assert!(!has_login_credentials(&None, false));
        assert!(!has_login_credentials(
            &Some("user@example.com".to_string()),
            false,
        ));
        assert!(!has_login_credentials(&None, true));
        assert!(has_login_credentials(
            &Some("user@example.com".to_string()),
            true,
        ));
    }

    #[test]
    fn full_tasks_are_bounded_by_provider_capability() {
        assert_eq!(
            full_child_tasks("newapi"),
            vec![
                adapters::CollectorTask::Balance,
                adapters::CollectorTask::Groups,
                adapters::CollectorTask::Models,
            ],
        );
        assert_eq!(
            full_child_tasks("openai-compatible"),
            vec![adapters::CollectorTask::Models],
        );
    }
}
