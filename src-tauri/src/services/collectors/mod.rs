pub mod sub2api;

use serde_json::{json, Value};

use crate::{models::collector::CollectorRunResult, services::database::AppDatabase};

pub fn detect_station_info(
    database: &AppDatabase,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    sub2api::detect_station(database, station_id)
}

pub fn collect_station_info(
    database: &AppDatabase,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    sub2api::collect_login_state(database, station_id)
}

pub fn test_station_login(
    database: &AppDatabase,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    let station = database.station_for_collector(&station_id)?;
    let credentials = database.get_station_credentials(station_id.clone())?;

    if !has_login_credentials(&credentials.login_username, credentials.password_present) {
        return Ok(build_status_result(
            station_id,
            station.name,
            "missing_credentials",
            "未填写登录账号或密码，无法测试登录。",
            "缺少账号或密码，先补齐再测试登录。",
        ));
    }

    let password = database.get_station_login_password(station_id.clone())?;
    let Some(login_password) = password else {
        return Ok(build_status_result(
            station_id,
            station.name,
            "missing_credentials",
            "未保存登录密码，无法测试登录。",
            "未保存登录密码，先在编辑站点里勾选记住密码并保存。",
        ));
    };

    let login_username = credentials.login_username.clone().unwrap_or_default();
    let summary = json!({
        "mode": "login-state",
        "adapter": "Login State Adapter",
        "detectedType": "Login State",
        "conclusion": "需要登录",
        "message": "已读取账号密码，准备尝试登录。",
        "login": {
            "usernamePresent": !login_username.trim().is_empty(),
            "passwordPresent": !login_password.trim().is_empty(),
            "status": credentials.login_status,
        },
        "loginRequired": true,
        "nextStep": "如果登录接口可用，主流程会继续尝试读取登录态。",
        "diagnosis": "测试登录只验证账号密码是否可用，不会执行后续接口采集。",
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
        "status": "manual_required",
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
        "manual_required",
        summary,
        normalized,
        Some(json!({
            "stationName": station.name,
            "loginUsernamePresent": !login_username.trim().is_empty(),
            "loginPasswordPresent": !login_password.trim().is_empty(),
            "note": "测试登录仅记录登录准备状态，不执行完整采集。",
        })),
        Some("登录测试已完成，后续登录态采集待接入真实站点接口。".to_string()),
    )?;

    Ok(CollectorRunResult {
        snapshot,
        events: vec![crate::models::collector::CollectorEvent {
            event_type: "login-test".to_string(),
            message: "登录测试已完成。".to_string(),
            status: "manual_required".to_string(),
        }],
    })
}

fn build_status_result(
    station_id: String,
    station_name: String,
    status: &str,
    conclusion: &str,
    message: &str,
) -> CollectorRunResult {
    let snapshot = crate::models::collector::CollectorSnapshot {
        id: format!("snapshot-{}", crate::services::database::now_millis_for_services()),
        station_id: station_id.clone(),
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
    use super::has_login_credentials;

    #[test]
    fn login_requires_username_and_password() {
        assert!(!has_login_credentials(&None, false));
        assert!(!has_login_credentials(&Some("user@example.com".to_string()), false));
        assert!(!has_login_credentials(&None, true));
        assert!(has_login_credentials(
            &Some("user@example.com".to_string()),
            true
        ));
    }
}
