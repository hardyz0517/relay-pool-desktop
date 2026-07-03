pub mod adapters;
pub mod facts;
pub mod sub2api;
pub mod url;

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
    data_key: &[u8; 32],
    station_id: String,
) -> Result<CollectorRunResult, String> {
    sub2api::collect_login_state(database, data_key, station_id)
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
            station.name,
            "missing_credentials",
            "未填写登录账号或密码，无法测试登录。",
            "缺少账号或密码，先补齐再测试登录。",
        ));
    }

    let password = database.get_station_login_password_with_data_key(station_id.clone(), data_key)?;
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
    let login_attempt =
        sub2api::test_login_credentials(&station.base_url, &login_username, &login_password)?;
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
        if login_succeeded { None } else { Some(diagnosis) },
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

fn build_status_result(
    station_id: String,
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
        models::{
            credentials::UpdateStationCredentialsInput,
            stations::CreateStationInput,
        },
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
                base_url: server.base_url.clone(),
                api_key: "sk-test-routing".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
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
        assert_eq!(server.last_login_password(), Some("correct-password".to_string()));
        assert_eq!(
            result
                .snapshot
                .summary_json
                .get("loginRequired")
                .and_then(Value::as_bool),
            Some(false),
        );
    }

    struct TestLoginServer {
        base_url: String,
        last_login_password: Arc<Mutex<Option<String>>>,
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
            self.last_login_password.lock().ok().and_then(|value| value.clone())
        }
    }

    fn handle_login_request(mut stream: TcpStream, last_login_password: Arc<Mutex<Option<String>>>) {
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
        stream.write_all(response.as_bytes()).expect("write response");
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
