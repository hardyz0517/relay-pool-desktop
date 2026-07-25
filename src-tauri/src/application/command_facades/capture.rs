use std::{sync::Arc, time::Duration};

use serde_json::Value;
use tauri::Manager;
use zeroize::Zeroizing;

use crate::{
    application::{
        collectors::{CaptureSnapshotRequest, CollectorService},
        credentials::CredentialService,
        error::ApplicationError,
        stations::StationService,
    },
    models::{
        capture::{CaptureSessionStatus, CapturedHttpEventInput},
        collector::CollectorRunResult,
        credentials::PersistStationSessionInput,
        stations::Station,
    },
    services::{
        capture::{
            self,
            session::{CaptureCommit, CaptureSessionStore},
            web_authorization::VerifiedWebAuthorizationSession,
        },
        station_endpoints::url_belongs_to_base,
    },
};

#[derive(Debug)]
pub(crate) enum CaptureCommandError {
    Application(ApplicationError),
    Message(String),
}

impl From<ApplicationError> for CaptureCommandError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

impl From<String> for CaptureCommandError {
    fn from(error: String) -> Self {
        Self::Message(error)
    }
}

pub(crate) struct CaptureSessionStartTarget {
    pub(crate) station: Station,
    pub(crate) login_username: Option<String>,
    pub(crate) login_password: Option<Zeroizing<String>>,
}

#[derive(Clone)]
pub(crate) struct CaptureCommandFacade {
    stations: Arc<StationService>,
    credentials: Arc<CredentialService>,
    collectors: Arc<CollectorService>,
    sessions: CaptureSessionStore,
}

impl CaptureCommandFacade {
    pub(crate) fn new(
        stations: Arc<StationService>,
        credentials: Arc<CredentialService>,
        collectors: Arc<CollectorService>,
        sessions: CaptureSessionStore,
    ) -> Self {
        Self {
            stations,
            credentials,
            collectors,
            sessions,
        }
    }

    pub(crate) async fn start_capture_session(
        &self,
        station_id: String,
    ) -> Result<CaptureSessionStartTarget, CaptureCommandError> {
        let station = self.stations.station_for_capture(&station_id).await?;
        let credentials = self
            .credentials
            .get_station_credentials(station_id.clone())
            .await?;
        let login_password_secret = if credentials.password_present {
            self.credentials
                .get_station_login_password(station_id)
                .await?
        } else {
            None
        };
        let login_password = login_password_secret
            .as_ref()
            .map(|secret| {
                std::str::from_utf8(secret.as_bytes())
                    .map(|value| Zeroizing::new(value.to_string()))
                    .map_err(|_| "stored station login password is not valid UTF-8".to_string())
            })
            .transpose()?;
        Ok(CaptureSessionStartTarget {
            station,
            login_username: credentials.login_username,
            login_password,
        })
    }

    pub(crate) async fn record_capture_event(
        &self,
        input: CapturedHttpEventInput,
    ) -> Result<CaptureSessionStatus, CaptureCommandError> {
        let station = self.stations.station_for_capture(&input.station_id).await?;
        if !capture_request_belongs_to_station(
            &station.website_url,
            &station.api_base_url,
            &input.request_url,
        ) {
            return Err(CaptureCommandError::Message(
                "捕获事件不属于当前站点 Base URL，已拒绝。".to_string(),
            ));
        }
        let web_authorization_user_id = web_authorization_candidate_user_id_from_input(&input);
        let captured_credentials = capture::extract_session_credentials(&input);
        let station_id = input.station_id.clone();
        let event = capture::sanitize_event(input);
        let receipt = self
            .sessions
            .push_event(&station_id, event, web_authorization_user_id)?;
        if let Some(session) = captured_credentials {
            self.credentials
                .persist_station_session_if_revision(session, receipt.endpoint_revision)
                .await?;
        }
        Ok(receipt.status)
    }

    pub(crate) async fn finish_capture_session(
        &self,
        station_id: String,
    ) -> Result<CollectorRunResult, CaptureCommandError> {
        let commit = self.sessions.begin_finish(&station_id)?;
        let result = self
            .finish_capture_session_with_events_inner(&station_id, &commit, None)
            .await
            .map_err(CaptureCommandError::Application);
        match result {
            Ok(result) => {
                self.sessions.complete_commit(&station_id, &commit)?;
                Ok(result)
            }
            Err(error) => Err(abort_capture_commit(
                &self.sessions,
                &station_id,
                &commit,
                error,
            )),
        }
    }

    pub(crate) async fn finish_web_authorization_session(
        &self,
        app: tauri::AppHandle,
        station_id: String,
    ) -> Result<CollectorRunResult, CaptureCommandError> {
        let station = self.stations.station_for_capture(&station_id).await?;
        let candidate = self
            .sessions
            .web_authorization_candidate(&station_id)?
            .ok_or_else(|| {
                CaptureCommandError::Message(
                    "网页登录授权尚未捕获到用户身份，请在授权窗口完成登录后重试。".to_string(),
                )
            })?;
        let cookie_header =
            read_capture_window_cookie_header(app, &station_id, &station.website_url).await?;
        let verified = capture::web_authorization::verify_newapi_cookie_session(
            &station.website_url,
            &cookie_header,
            &candidate.user_id,
            Duration::from_secs(20),
        )?;
        let commit = self
            .sessions
            .begin_web_authorization_commit(&station_id, &candidate)?;
        let persist_result = self
            .persist_web_authorization_session_inner(
                station_id.clone(),
                verified,
                commit.endpoint_revision,
            )
            .await
            .map_err(CaptureCommandError::Application);
        if let Err(error) = persist_result {
            return Err(abort_capture_commit(
                &self.sessions,
                &station_id,
                &commit,
                error,
            ));
        }

        let result = self
            .finish_capture_session_with_events_inner(
                &station_id,
                &commit,
                Some(capture::web_authorization_summary(
                    "success",
                    Some("web_authorization"),
                    true,
                )),
            )
            .await
            .map_err(CaptureCommandError::Application);
        match result {
            Ok(result) => {
                self.sessions.complete_commit(&station_id, &commit)?;
                Ok(result)
            }
            Err(error) => Err(abort_capture_commit(
                &self.sessions,
                &station_id,
                &commit,
                error,
            )),
        }
    }

    pub(crate) fn start_prepared_session(
        &self,
        station_id: String,
        label: String,
        endpoint_revision: i64,
    ) -> Result<CaptureSessionStatus, String> {
        self.sessions.start(station_id, label, endpoint_revision)
    }

    async fn persist_web_authorization_session_inner(
        &self,
        station_id: String,
        verified: VerifiedWebAuthorizationSession,
        endpoint_revision: i64,
    ) -> Result<(), ApplicationError> {
        self.credentials
            .persist_station_session_if_revision(
                PersistStationSessionInput {
                    station_id,
                    access_token: None,
                    refresh_token: None,
                    cookie: Some(verified.cookie_header),
                    newapi_user_id: Some(verified.newapi_user_id),
                    token_expires_at: None,
                    session_expires_at: None,
                    session_source: verified.session_source,
                },
                endpoint_revision,
            )
            .await?;
        Ok(())
    }

    async fn finish_capture_session_with_events_inner(
        &self,
        station_id: &str,
        commit: &CaptureCommit,
        web_authorization_summary: Option<Value>,
    ) -> Result<CollectorRunResult, ApplicationError> {
        let events = &commit.events;
        let (mut summary, normalized, raw) = capture::summarize_events(events);
        if let Some(web_authorization_summary) = web_authorization_summary {
            if let Some(summary) = summary.as_object_mut() {
                summary.insert("webAuthorization".to_string(), web_authorization_summary);
            }
        }
        let status = normalized
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("partial")
            .to_string();
        let error_message = if events.is_empty() {
            Some("未捕获到后台接口响应，请确认已在网页登录窗口完成登录并打开后台页面。".to_string())
        } else {
            None
        };
        self.collectors
            .record_capture_snapshot(CaptureSnapshotRequest {
                station_id: station_id.to_string(),
                endpoint_revision: commit.endpoint_revision,
                status,
                summary_json: summary,
                normalized_json: normalized,
                raw_json_redacted: Some(raw),
                error_message,
                event_count: events.len() as i64,
            })
            .await
    }
}

fn abort_capture_commit(
    sessions: &CaptureSessionStore,
    station_id: &str,
    commit: &CaptureCommit,
    persistence_error: CaptureCommandError,
) -> CaptureCommandError {
    match sessions.abort_commit(station_id, commit) {
        Ok(()) => persistence_error,
        Err(_) => CaptureCommandError::Application(ApplicationError::Internal),
    }
}

fn capture_window_label(station_id: &str) -> String {
    format!(
        "capture-{}",
        station_id.replace(|character: char| !character.is_ascii_alphanumeric(), "-")
    )
}

async fn read_capture_window_cookie_header(
    app: tauri::AppHandle,
    station_id: &str,
    station_website_url: &str,
) -> Result<String, CaptureCommandError> {
    let label = capture_window_label(station_id);
    let window = app.get_webview_window(&label).ok_or_else(|| {
        CaptureCommandError::Message("网页登录授权窗口不存在，请重新打开授权窗口。".to_string())
    })?;
    let target = tauri::Url::parse(station_website_url).map_err(|error| {
        CaptureCommandError::Message(format!("站点管理地址无法用于读取 Cookie: {error}"))
    })?;

    let cookies = tauri::async_runtime::spawn_blocking(move || window.cookies_for_url(target))
        .await
        .map_err(|error| {
            CaptureCommandError::Message(format!("读取网页登录授权 Cookie 任务失败: {error}"))
        })?
        .map_err(|error| {
            CaptureCommandError::Message(format!("读取网页登录授权 Cookie 失败: {error}"))
        })?;

    let pairs = cookies
        .into_iter()
        .map(|cookie| (cookie.name().to_string(), cookie.value().to_string()))
        .collect::<Vec<_>>();
    Ok(
        capture::web_authorization::build_cookie_header_from_pairs(&pairs).ok_or_else(|| {
            CaptureCommandError::Message(
                "网页登录授权未捕获到可用 Cookie，请确认已在授权窗口完成登录。".to_string(),
            )
        })?,
    )
}

fn capture_request_belongs_to_station(
    station_website_url: &str,
    station_api_base_url: &str,
    request_url: &str,
) -> bool {
    [station_website_url, station_api_base_url]
        .into_iter()
        .any(|base_url| url_belongs_to_base(request_url, base_url))
}

fn web_authorization_candidate_user_id_from_input(
    input: &CapturedHttpEventInput,
) -> Option<String> {
    let fallback_path;
    let request_path = if let Some(path) = input.request_path.as_deref() {
        path
    } else {
        fallback_path = path_from_request_url(&input.request_url);
        &fallback_path
    };
    if !capture::web_authorization::is_newapi_completion_candidate(
        request_path,
        input.status,
        input.response_json.as_ref(),
    ) {
        return None;
    }
    input
        .response_json
        .as_ref()
        .and_then(capture::web_authorization::extract_verified_user_id)
}

fn path_from_request_url(url: &str) -> String {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let path = without_scheme
        .find('/')
        .map(|index| &without_scheme[index..])
        .unwrap_or("/");
    path.split(['?', '#']).next().unwrap_or("/").to_string()
}
