use std::{sync::Arc, time::Duration};

use futures_util::{future::BoxFuture, FutureExt};
use serde_json::Value;
use tauri::Manager;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::{
    application::{
        collectors::{CaptureSnapshotRequest, CollectorService},
        credentials::CredentialService,
        error::ApplicationError,
        stations::StationService,
    },
    background_tasks::{BlockingExecutor, BlockingExecutorError},
    models::{
        capture::{CaptureSessionStatus, CapturedHttpEventInput},
        collector::CollectorRunResult,
        credentials::PersistStationSessionInput,
        stations::Station,
    },
    observability::correlation,
    outbound::{AsyncOutboundClient, ProxyPolicy, RequestBudget},
    services::{
        capture::{
            self,
            session::{CaptureCommit, CaptureSessionStore},
            web_authorization::VerifiedWebAuthorizationSession,
        },
        collectors::{
            contract::{
                AuthorizationRequest, AuthorizationStatus, CollectorContext, CredentialScope,
                CredentialSecret, CredentialSecretPurpose, DriverSecretAccessor,
                OpaqueCredentialHandle, ProviderAuthContext, ProviderEndpoints, ProviderKind,
                StationIdentity,
            },
            evidence::EndpointRole,
            failure::{DriverFailure, DriverFailureKind},
            orchestration::ProviderRegistry,
        },
        station_endpoints::url_belongs_to_base,
    },
};

#[derive(Debug)]
pub(crate) enum CaptureCommandError {
    Application(ApplicationError),
    Blocking(BlockingExecutorError),
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

impl From<BlockingExecutorError> for CaptureCommandError {
    fn from(error: BlockingExecutorError) -> Self {
        Self::Blocking(error)
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
    blocking: BlockingExecutor,
    outbound: AsyncOutboundClient,
    providers: Arc<ProviderRegistry>,
}

impl CaptureCommandFacade {
    pub(crate) fn new(
        stations: Arc<StationService>,
        credentials: Arc<CredentialService>,
        collectors: Arc<CollectorService>,
        sessions: CaptureSessionStore,
        blocking: BlockingExecutor,
        outbound: AsyncOutboundClient,
        providers: Arc<ProviderRegistry>,
    ) -> Self {
        Self {
            stations,
            credentials,
            collectors,
            sessions,
            blocking,
            outbound,
            providers,
        }
    }

    pub(crate) async fn start_capture_session(
        &self,
        app: tauri::AppHandle,
        station_id: String,
    ) -> Result<CaptureSessionStatus, CaptureCommandError> {
        let station = self.stations.station_for_capture(&station_id).await?;
        let credentials = self
            .credentials
            .get_station_credentials(station_id.clone())
            .await?;
        let login_password_secret = if credentials.password_present {
            self.credentials
                .get_station_login_password(station_id.clone())
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
        let target = CaptureSessionStartTarget {
            station,
            login_username: credentials.login_username,
            login_password,
        };
        let label = capture_window_label(&station_id);
        let endpoint_revision = target.station.endpoint_revision;
        let script = capture_script(
            &station_id,
            &label,
            target.login_username.as_deref(),
            target.login_password.as_ref().map(|value| value.as_str()),
        );
        self.open_capture_window(app, target, label.clone(), script)
            .await?;
        Ok(self.start_prepared_session(station_id, label, endpoint_revision)?)
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
        let cookie_header = read_capture_window_cookie_header(
            app,
            &self.blocking,
            &station_id,
            &station.website_url,
        )
        .await?;
        let verified = self
            .verify_newapi_web_authorization_session(&station, cookie_header, &candidate.user_id)
            .await?;
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

    async fn open_capture_window(
        &self,
        app: tauri::AppHandle,
        target: CaptureSessionStartTarget,
        label: String,
        script: String,
    ) -> Result<(), CaptureCommandError> {
        let label_for_start = label.clone();
        self.blocking
            .submit(
                "capture_window_open",
                None,
                current_correlation_id(),
                None,
                move |_| {
                    Ok(open_capture_window_blocking(
                        app,
                        target,
                        label_for_start,
                        script,
                    ))
                },
            )?
            .result()
            .await?
            .map_err(CaptureCommandError::Message)
    }

    async fn verify_newapi_web_authorization_session(
        &self,
        station: &Station,
        cookie_header: String,
        expected_user_id: &str,
    ) -> Result<VerifiedWebAuthorizationSession, CaptureCommandError> {
        let cookie_header = cookie_header.trim().to_string();
        if cookie_header.is_empty() {
            return Err(CaptureCommandError::Message(
                "Web authorization did not capture a usable Cookie header.".to_string(),
            ));
        }
        let expected_user_id = expected_user_id.trim().to_string();
        if expected_user_id.is_empty() {
            return Err(CaptureCommandError::Message(
                "Web authorization did not capture a usable user id.".to_string(),
            ));
        }

        let credential = OpaqueCredentialHandle {
            station_id: station.id.clone(),
            credential_revision: station.endpoint_revision,
            scope: CredentialScope::LoginSession,
        };
        let secret_accessor = WebAuthorizationSecretAccessor {
            expected: credential.clone(),
            cookie_header: cookie_header.clone(),
        };
        let context = CollectorContext {
            station: StationIdentity {
                station_id: station.id.clone(),
                endpoint_revision: station.endpoint_revision,
                provider: ProviderKind::NewApi,
            },
            endpoints: ProviderEndpoints {
                api_base_url: (!station.api_base_url.trim().is_empty())
                    .then_some(station.api_base_url.clone()),
                website_url: Some(station.website_url.clone()),
            },
            credential: credential.clone(),
            auth: Some(ProviderAuthContext::NewApi {
                user_id: expected_user_id.clone(),
                secret_purpose: CredentialSecretPurpose::SessionCookie,
            }),
            secrets: &secret_accessor,
            outbound: &self.outbound,
            proxy: ProxyPolicy::Direct,
            budget: RequestBudget::from_now(Duration::from_secs(20)),
            cancellation: CancellationToken::new(),
            correlation_id: current_correlation_id()
                .unwrap_or_else(|| "capture:web-authorization:newapi".to_string()),
        };
        let driver = self
            .providers
            .authorization(ProviderKind::NewApi)
            .map_err(capture_authorization_error)?;
        let output = driver
            .validate_authorization(
                &context,
                AuthorizationRequest {
                    station: context.station.clone(),
                    endpoints: context.endpoints.clone(),
                    credential,
                    endpoint_role: EndpointRole::Authorization,
                },
            )
            .await
            .map_err(capture_authorization_error)?;
        match output.status {
            AuthorizationStatus::Authorized => Ok(VerifiedWebAuthorizationSession::new(
                cookie_header,
                expected_user_id,
            )),
            AuthorizationStatus::ReauthorizationRequired => Err(CaptureCommandError::Message(
                "Web authorization session expired; please re-authorize in the login window."
                    .to_string(),
            )),
            AuthorizationStatus::Unsupported => Err(CaptureCommandError::Message(
                "NewAPI web authorization validation is not supported by this build.".to_string(),
            )),
        }
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

struct WebAuthorizationSecretAccessor {
    expected: OpaqueCredentialHandle,
    cookie_header: String,
}

impl DriverSecretAccessor for WebAuthorizationSecretAccessor {
    fn resolve_secret<'a>(
        &'a self,
        handle: &'a OpaqueCredentialHandle,
        purpose: CredentialSecretPurpose,
    ) -> BoxFuture<'a, Result<CredentialSecret, DriverFailure>> {
        async move {
            if purpose != CredentialSecretPurpose::SessionCookie || handle != &self.expected {
                return Err(DriverFailure::unsupported(
                    "web authorization cookie is not available to this driver context",
                ));
            }
            Ok(CredentialSecret::new(self.cookie_header.clone()))
        }
        .boxed()
    }
}

fn capture_authorization_error(error: DriverFailure) -> CaptureCommandError {
    let detail = error.sanitized_detail.as_deref().unwrap_or_default();
    match error.kind {
        DriverFailureKind::AuthRejected => CaptureCommandError::Message(
            "Web authorization session expired; please re-authorize in the login window."
                .to_string(),
        ),
        DriverFailureKind::MalformedPayload => CaptureCommandError::Message(
            "Web authorization self probe returned an invalid NewAPI user payload.".to_string(),
        ),
        DriverFailureKind::Unsupported | DriverFailureKind::InvalidRequest => {
            CaptureCommandError::Message(format!(
                "Web authorization validation is not available: {detail}"
            ))
        }
        DriverFailureKind::RateLimited
        | DriverFailureKind::Timeout
        | DriverFailureKind::BudgetExhausted
        | DriverFailureKind::Cancelled
        | DriverFailureKind::Transport
        | DriverFailureKind::ProviderUnavailable => {
            CaptureCommandError::Message(format!("Web authorization self probe failed: {detail}"))
        }
        DriverFailureKind::ResultUnknown | DriverFailureKind::Internal => {
            CaptureCommandError::Application(ApplicationError::Internal)
        }
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

fn current_correlation_id() -> Option<String> {
    correlation::current().map(|id| id.as_str().to_string())
}

fn open_capture_window_blocking(
    app: tauri::AppHandle,
    target: CaptureSessionStartTarget,
    label: String,
    script: String,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&label) {
        window
            .set_focus()
            .map_err(|error| format!("聚焦捕获窗口失败: {error}"))?;
    } else {
        tauri::WebviewWindowBuilder::new(
            &app,
            label.clone(),
            tauri::WebviewUrl::External(
                "about:blank"
                    .parse()
                    .map_err(|error| format!("捕获窗口初始化失败: {error}"))?,
            ),
        )
        .title(format!("网页登录 / 捕获 - {}", target.station.name))
        .inner_size(1100.0, 760.0)
        .initialization_script(&script)
        .build()
        .map_err(|error| format!("打开网页登录窗口失败: {error}"))?;
        if let Some(window) = app.get_webview_window(&label) {
            let target_url = target.station.website_url.clone();
            let target = target_url
                .parse()
                .map_err(|error| format!("Base URL 无法作为网页登录地址打开: {error}"))?;
            let navigator = window.clone();
            window
                .run_on_main_thread(move || {
                    let _ = navigator.navigate(target);
                })
                .map_err(|error| format!("安排捕获窗口导航失败: {error}"))?;
        }
    }
    Ok(())
}

fn capture_script(
    station_id: &str,
    window_label: &str,
    login_username: Option<&str>,
    login_password: Option<&str>,
) -> String {
    let login_username_json =
        serde_json::to_string(&login_username).unwrap_or_else(|_| "null".to_string());
    let login_password_json =
        serde_json::to_string(&login_password).unwrap_or_else(|_| "null".to_string());
    format!(
        r#"
(() => {{
  if (window.__relayPoolCaptureInstalled) return;
  window.__relayPoolCaptureInstalled = true;
  const stationId = {station_id:?};
  const sourceWindowId = {window_label:?};
  const loginUsername = {login_username_json};
  const loginPassword = {login_password_json};
  const limit = 4000;
  const invoke = (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)
    ? window.__TAURI_INTERNALS__.invoke
    : null;
  const pathFromUrl = (url) => {{
    try {{ return new URL(url, window.location.href).pathname || "/"; }}
    catch (_) {{ return "/"; }}
  }};
  const contentTypeOf = (headers) => {{
    try {{ return headers && headers.get ? (headers.get("content-type") || "") : ""; }}
    catch (_) {{ return ""; }}
  }};
  const tryFinishWebAuthorization = (status) => {{
    if (!invoke || !status || !status.webAuthorizationCandidate) return;
    if (window.__relayPoolAuthorizationFinishInFlight) return;
    window.__relayPoolAuthorizationFinishInFlight = true;
    invoke("finish_web_authorization_session", {{ stationId }})
      .catch(() => undefined)
      .finally(() => {{
        window.__relayPoolAuthorizationFinishInFlight = false;
      }});
  }};
  const send = (input) => {{
    if (!invoke) return;
    invoke("record_capture_event", {{ input }})
      .then(tryFinishWebAuthorization)
      .catch(() => undefined);
  }};
  const buildBase = (url, method, startedAt) => ({{
    stationId,
    sourceWindowId,
    pageUrl: window.location.href,
    requestUrl: String(new URL(url, window.location.href)),
    requestPath: pathFromUrl(url),
    method,
    startedAt,
  }});
  const setNativeValue = (element, value) => {{
    if (!element || value == null || element.value === value) return false;
    const prototype = Object.getPrototypeOf(element);
    const descriptor = prototype ? Object.getOwnPropertyDescriptor(prototype, "value") : null;
    if (descriptor && descriptor.set) descriptor.set.call(element, value);
    else element.value = value;
    element.dispatchEvent(new Event("input", {{ bubbles: true }}));
    element.dispatchEvent(new Event("change", {{ bubbles: true }}));
    return true;
  }};
  const candidateInput = (selectors) => {{
    for (const selector of selectors) {{
      const found = document.querySelector(selector);
      if (found && !found.disabled && !found.readOnly) return found;
    }}
    return null;
  }};
  const fillLoginForm = () => {{
    try {{
      setNativeValue(candidateInput([
        "input[type='email']",
        "input[name='email']",
        "input[name='username']",
        "input[name='user']",
        "input[autocomplete='username']",
        "input[placeholder*='邮箱']",
        "input[placeholder*='账号']",
        "input[placeholder*='email' i]",
      ]), loginUsername);
      setNativeValue(candidateInput([
        "input[type='password']",
        "input[name='password']",
        "input[autocomplete='current-password']",
        "input[placeholder*='密码']",
        "input[placeholder*='password' i]",
      ]), loginPassword);
      for (const checkbox of Array.from(document.querySelectorAll("input[type='checkbox']"))) {{
        const label = checkbox.closest("label") || (checkbox.id ? document.querySelector(`label[for="${{checkbox.id}}"]`) : null);
        const text = `${{checkbox.name || ""}} ${{checkbox.id || ""}} ${{label ? label.textContent || "" : ""}}`.toLowerCase();
        if (text.includes("agreement") || text.includes("attestation") || text.includes("region") || text.includes("大陆") || text.includes("中华人民共和国") || text.includes("独立陈述")) {{
          if (!checkbox.checked) {{
            checkbox.checked = true;
            checkbox.dispatchEvent(new Event("input", {{ bubbles: true }}));
            checkbox.dispatchEvent(new Event("change", {{ bubbles: true }}));
          }}
        }}
      }}
    }} catch (_) {{}}
  }};
  fillLoginForm();
  const fillTimer = window.setInterval(fillLoginForm, 800);
  window.setTimeout(() => window.clearInterval(fillTimer), 15000);
  try {{
    new MutationObserver(fillLoginForm).observe(document.documentElement, {{ childList: true, subtree: true }});
  }} catch (_) {{}}
  const originalFetch = window.fetch;
  window.fetch = async function(input, init) {{
    const url = typeof input === "string" ? input : (input && input.url) || String(input);
    const method = (init && init.method) || (input && input.method) || "GET";
    const startedAt = new Date().toISOString();
    const started = performance.now();
    try {{
      const response = await originalFetch.apply(this, arguments);
      const clone = response.clone();
      const contentType = contentTypeOf(response.headers);
      const base = buildBase(url, method, startedAt);
      if (contentType.includes("json")) {{
        clone.json().then((json) => send({{
          ...base,
          status: response.status,
          contentType,
          finishedAt: new Date().toISOString(),
          durationMs: Math.round(performance.now() - started),
          responseKind: "json",
          responseJson: json,
          responseSize: JSON.stringify(json).length,
        }})).catch(() => undefined);
      }} else {{
        clone.text().then((text) => send({{
          ...base,
          status: response.status,
          contentType,
          finishedAt: new Date().toISOString(),
          durationMs: Math.round(performance.now() - started),
          responseKind: contentType.includes("html") ? "html" : "text",
          responseText: text.slice(0, limit),
          responseSize: text.length,
        }})).catch(() => undefined);
      }}
      return response;
    }} catch (error) {{
      send({{
        ...buildBase(url, method, startedAt),
        finishedAt: new Date().toISOString(),
        durationMs: Math.round(performance.now() - started),
        responseKind: "error",
        errorMessage: error && error.message ? error.message : String(error),
      }});
      throw error;
    }}
  }};
  const originalOpen = XMLHttpRequest.prototype.open;
  const originalSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function(method, url) {{
    this.__relayPoolCapture = {{ method: method || "GET", url: String(url), startedAt: new Date().toISOString(), started: performance.now() }};
    return originalOpen.apply(this, arguments);
  }};
  XMLHttpRequest.prototype.send = function() {{
    this.addEventListener("loadend", function() {{
      const meta = this.__relayPoolCapture;
      if (!meta) return;
      const contentType = this.getResponseHeader("content-type") || "";
      let responseText = "";
      try {{ responseText = typeof this.responseText === "string" ? this.responseText : ""; }} catch (_) {{}}
      let responseJson = null;
      if (contentType.includes("json") && responseText) {{
        try {{ responseJson = JSON.parse(responseText); }} catch (_) {{}}
      }}
      send({{
        ...buildBase(meta.url, meta.method, meta.startedAt),
        status: this.status,
        contentType,
        finishedAt: new Date().toISOString(),
        durationMs: Math.round(performance.now() - meta.started),
        responseKind: responseJson ? "json" : (contentType.includes("html") ? "html" : "text"),
        responseJson,
        responseText: responseJson ? null : responseText.slice(0, limit),
        responseSize: responseText.length,
      }});
    }});
    return originalSend.apply(this, arguments);
  }};
}})();
"#
    )
}

async fn read_capture_window_cookie_header(
    app: tauri::AppHandle,
    blocking: &BlockingExecutor,
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

    let cookies = blocking
        .submit(
            "capture_window_cookie_read",
            None,
            current_correlation_id(),
            None,
            move |_| Ok(window.cookies_for_url(target)),
        )?
        .result()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_script_invokes_web_authorization_finish_after_candidate() {
        let script = capture_script("station-1", "capture-station-1", None, None);

        assert!(script.contains("finish_web_authorization_session"));
        assert!(script.contains("webAuthorizationCandidate"));
        assert!(script.contains("window.__relayPoolAuthorizationFinishInFlight"));
    }
}
