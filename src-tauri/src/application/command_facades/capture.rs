use std::{sync::Arc, time::Duration};

use futures_util::{future::BoxFuture, FutureExt};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::{
    application::{
        collectors::{CaptureSnapshotRequest, CollectorService},
        credentials::CredentialService,
        error::ApplicationError,
        provider_drafts::ProviderDraftService,
        stations::StationService,
    },
    background_tasks::BlockingExecutorError,
    models::{
        capture::{CaptureSessionStatus, CapturedHttpEventInput},
        collector::CollectorRunResult,
        credentials::PersistStationSessionInput,
        provider_drafts::{ProviderDraftPayload, ProviderDraftPreview},
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

pub(crate) struct CaptureSessionStartPlan {
    pub(crate) station_id: String,
    pub(crate) label: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) script: String,
    pub(crate) target: CaptureSessionStartTarget,
}

#[derive(Clone)]
pub(crate) struct CaptureCommandFacade {
    stations: Arc<StationService>,
    credentials: Arc<CredentialService>,
    drafts: Arc<ProviderDraftService>,
    collectors: Arc<CollectorService>,
    sessions: CaptureSessionStore,
    outbound: AsyncOutboundClient,
    providers: Arc<ProviderRegistry>,
}

impl CaptureCommandFacade {
    pub(crate) fn new(
        stations: Arc<StationService>,
        credentials: Arc<CredentialService>,
        drafts: Arc<ProviderDraftService>,
        collectors: Arc<CollectorService>,
        sessions: CaptureSessionStore,
        outbound: AsyncOutboundClient,
        providers: Arc<ProviderRegistry>,
    ) -> Self {
        Self {
            stations,
            credentials,
            drafts,
            collectors,
            sessions,
            outbound,
            providers,
        }
    }

    pub(crate) async fn start_capture_session(
        &self,
        station_id: String,
    ) -> Result<CaptureSessionStartPlan, CaptureCommandError> {
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
            "finish_web_authorization_session",
            "stationId",
        );
        Ok(CaptureSessionStartPlan {
            station_id,
            label,
            endpoint_revision,
            script,
            target,
        })
    }

    pub(crate) async fn start_provider_draft_authorization(
        &self,
        draft_id: String,
    ) -> Result<CaptureSessionStartPlan, CaptureCommandError> {
        let station = self.drafts.station_projection(&draft_id).await?;
        let credentials = self.drafts.credentials_projection(&draft_id).await?;
        let login_password = self
            .drafts
            .login_password(&draft_id)
            .await?
            .map(Zeroizing::new);
        let target = CaptureSessionStartTarget {
            station,
            login_username: credentials.login_username,
            login_password,
        };
        let label = capture_window_label(&draft_id);
        let endpoint_revision = target.station.endpoint_revision;
        let script = capture_script(
            &draft_id,
            &label,
            target.login_username.as_deref(),
            target.login_password.as_ref().map(|value| value.as_str()),
            "finish_provider_draft_authorization_session",
            "draftId",
        );
        Ok(CaptureSessionStartPlan {
            station_id: draft_id,
            label,
            endpoint_revision,
            script,
            target,
        })
    }

    pub(crate) async fn record_capture_event(
        &self,
        input: CapturedHttpEventInput,
    ) -> Result<CaptureSessionStatus, CaptureCommandError> {
        let owner = self.capture_owner(&input.station_id).await?;
        let station = owner.station();
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
            match owner {
                CaptureOwner::Station(_) => {
                    self.credentials
                        .persist_station_session_if_revision(session, receipt.endpoint_revision)
                        .await?;
                }
                CaptureOwner::Draft { .. } => {
                    self.drafts
                        .persist_session(session, receipt.endpoint_revision)
                        .await?;
                }
            }
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
        station_id: String,
        cookie_header: String,
    ) -> Result<CollectorRunResult, CaptureCommandError> {
        self.finish_web_authorization_session_with_cookie(station_id, cookie_header)
            .await
    }

    async fn finish_web_authorization_session_with_cookie(
        &self,
        station_id: String,
        cookie_header: String,
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

    pub(crate) async fn finish_provider_draft_authorization_session(
        &self,
        draft_id: String,
        cookie_header: String,
    ) -> Result<ProviderDraftPreview, CaptureCommandError> {
        self.finish_provider_draft_authorization_session_with_cookie(draft_id, cookie_header)
            .await
    }

    async fn finish_provider_draft_authorization_session_with_cookie(
        &self,
        draft_id: String,
        cookie_header: String,
    ) -> Result<ProviderDraftPreview, CaptureCommandError> {
        let owner = self.capture_owner(&draft_id).await?;
        let CaptureOwner::Draft { station, payload } = owner else {
            return Err(CaptureCommandError::Message(
                "provider draft authorization requires an active draft".to_string(),
            ));
        };
        let candidate = self
            .sessions
            .web_authorization_candidate(&draft_id)?
            .ok_or_else(|| {
                CaptureCommandError::Message(
                    "Web authorization has not captured a user identity yet.".to_string(),
                )
            })?;
        let verified = self
            .verify_newapi_web_authorization_session(&station, cookie_header, &candidate.user_id)
            .await?;
        let commit = self
            .sessions
            .begin_web_authorization_commit(&draft_id, &candidate)?;
        let result = self
            .persist_provider_draft_authorization_inner(&draft_id, &payload, verified, &commit)
            .await;
        match result {
            Ok(preview) => {
                self.sessions.complete_commit(&draft_id, &commit)?;
                Ok(preview)
            }
            Err(error) => Err(abort_capture_commit(
                &self.sessions,
                &draft_id,
                &commit,
                error,
            )),
        }
    }

    pub(crate) async fn web_authorization_cookie_url(
        &self,
        station_id: &str,
    ) -> Result<String, String> {
        self.sessions.web_authorization_cookie_url(station_id)
    }

    pub(crate) fn start_prepared_session(
        &self,
        station_id: String,
        label: String,
        endpoint_revision: i64,
        web_authorization_cookie_url: String,
    ) -> Result<CaptureSessionStatus, String> {
        self.sessions.start(
            station_id,
            label,
            endpoint_revision,
            web_authorization_cookie_url,
        )
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

    async fn persist_provider_draft_authorization_inner(
        &self,
        draft_id: &str,
        payload: &ProviderDraftPayload,
        verified: VerifiedWebAuthorizationSession,
        commit: &CaptureCommit,
    ) -> Result<ProviderDraftPreview, CaptureCommandError> {
        self.drafts
            .persist_session(
                PersistStationSessionInput {
                    station_id: draft_id.to_string(),
                    access_token: None,
                    refresh_token: None,
                    cookie: Some(verified.cookie_header),
                    newapi_user_id: Some(verified.newapi_user_id),
                    token_expires_at: None,
                    session_expires_at: None,
                    session_source: verified.session_source,
                },
                commit.endpoint_revision,
            )
            .await?;

        let (mut summary, normalized, _) = capture::summarize_events(&commit.events);
        if let Some(summary) = summary.as_object_mut() {
            summary.insert(
                "webAuthorization".to_string(),
                capture::web_authorization_summary("success", Some("web_authorization"), true),
            );
            summary.insert("normalized".to_string(), normalized);
        }
        let preview = ProviderDraftPreview {
            draft_id: draft_id.to_string(),
            kind: "capture".to_string(),
            runtime_fingerprint: ProviderDraftService::runtime_fingerprint(payload),
            status: "success".to_string(),
            groups: Vec::new(),
            models: Vec::new(),
            balance: None,
            summary_json: summary,
            collected_at: chrono::Utc::now().timestamp_millis().to_string(),
        };
        self.drafts.store_preview(preview).await.map_err(Into::into)
    }

    async fn capture_owner(&self, owner_id: &str) -> Result<CaptureOwner, CaptureCommandError> {
        match self.stations.station_for_capture(owner_id).await {
            Ok(station) => Ok(CaptureOwner::Station(station)),
            Err(ApplicationError::NotFound) => {
                let draft = self.drafts.get(owner_id.to_string()).await?;
                if draft.state != "active" {
                    return Err(ApplicationError::NotFound.into());
                }
                let payload = draft.payload.clone();
                let station = self.drafts.station_projection(owner_id).await?;
                Ok(CaptureOwner::Draft { station, payload })
            }
            Err(error) => Err(error.into()),
        }
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

enum CaptureOwner {
    Station(Station),
    Draft {
        station: Station,
        payload: ProviderDraftPayload,
    },
}

impl CaptureOwner {
    fn station(&self) -> &Station {
        match self {
            Self::Station(station) | Self::Draft { station, .. } => station,
        }
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

fn capture_script(
    station_id: &str,
    window_label: &str,
    login_username: Option<&str>,
    login_password: Option<&str>,
    finish_authorization_command: &str,
    finish_authorization_input_key: &str,
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
    invoke({finish_authorization_command:?}, {{ [{finish_authorization_input_key:?}]: stationId }})
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
        let script = capture_script(
            "station-1",
            "capture-station-1",
            None,
            None,
            "finish_web_authorization_session",
            "stationId",
        );

        assert!(script.contains("finish_web_authorization_session"));
        assert!(script.contains("webAuthorizationCandidate"));
        assert!(script.contains("window.__relayPoolAuthorizationFinishInFlight"));
    }
}
