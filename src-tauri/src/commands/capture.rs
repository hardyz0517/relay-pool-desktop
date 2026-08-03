use serde_json::Value;
use tauri::{Manager, State};

use crate::{
    application::command_facades::{
        CaptureCommandError, CaptureCommandFacade, CaptureSessionStartPlan,
    },
    commands::error,
    ipc::dto::provider_drafts::{ProviderDraftIdInputDto, ProviderDraftPreviewDto},
    ipc::dto::station_collector_operations::{
        CaptureSessionStatusDto, CaptureStationIdInputDto, CapturedHttpEventInputDto,
        CollectorRunResultDto,
    },
    observability::correlation,
    services::capture as service_capture,
};

fn capture_command_error(error: CaptureCommandError) -> error::CommandError {
    match error {
        CaptureCommandError::Application(error) => error::command_application_error(error),
        CaptureCommandError::Blocking(error) => super::public_blocking_executor_error(error),
        CaptureCommandError::Message(message) => message.into(),
    }
}

#[tauri::command]
pub async fn start_capture_session(
    app: tauri::AppHandle,
    facade: State<'_, CaptureCommandFacade>,
    input: Value,
) -> Result<CaptureSessionStatusDto, error::CommandError> {
    correlation::in_command_scope("start_capture_session", async {
        let input = CaptureStationIdInputDto::parse(input)?;
        let plan = facade
            .start_capture_session(input.station_id)
            .await
            .map_err(capture_command_error)?;
        open_capture_window(app, &plan).map_err(capture_command_error)?;
        let web_authorization_cookie_url = plan.target.station.website_url.clone();
        facade
            .start_prepared_session(
                plan.station_id,
                plan.label,
                plan.endpoint_revision,
                web_authorization_cookie_url,
            )
            .map_err(CaptureCommandError::Message)
            .map_err(capture_command_error)
    })
    .await
}

#[tauri::command]
pub async fn start_provider_draft_authorization(
    app: tauri::AppHandle,
    facade: State<'_, CaptureCommandFacade>,
    input: Value,
) -> Result<CaptureSessionStatusDto, error::CommandError> {
    correlation::in_command_scope("start_provider_draft_authorization", async {
        let input = ProviderDraftIdInputDto::parse(input)?;
        let plan = facade
            .start_provider_draft_authorization(input.draft_id)
            .await
            .map_err(capture_command_error)?;
        open_capture_window(app, &plan).map_err(capture_command_error)?;
        let web_authorization_cookie_url = plan.target.station.website_url.clone();
        facade
            .start_prepared_session(
                plan.station_id,
                plan.label,
                plan.endpoint_revision,
                web_authorization_cookie_url,
            )
            .map_err(CaptureCommandError::Message)
            .map_err(capture_command_error)
    })
    .await
}

#[tauri::command]
pub async fn get_capture_session_status(
    sessions: State<'_, service_capture::session::CaptureSessionStore>,
    input: Value,
) -> Result<CaptureSessionStatusDto, error::CommandError> {
    correlation::in_command_scope("get_capture_session_status", async {
        let input = CaptureStationIdInputDto::parse(input)?;
        Ok(sessions.status(&input.station_id)?)
    })
    .await
}

#[tauri::command]
pub async fn record_capture_event(
    facade: State<'_, CaptureCommandFacade>,
    input: Value,
) -> Result<CaptureSessionStatusDto, error::CommandError> {
    correlation::in_command_scope("record_capture_event", async {
        let input = CapturedHttpEventInputDto::parse(input)?.into_domain();
        facade
            .record_capture_event(input)
            .await
            .map_err(capture_command_error)
    })
    .await
}

#[tauri::command]
pub async fn clear_capture_session(
    sessions: State<'_, service_capture::session::CaptureSessionStore>,
    input: Value,
) -> Result<CaptureSessionStatusDto, error::CommandError> {
    correlation::in_command_scope("clear_capture_session", async {
        let input = CaptureStationIdInputDto::parse(input)?;
        Ok(sessions.clear(&input.station_id)?)
    })
    .await
}

#[tauri::command]
pub async fn close_capture_session(
    app: tauri::AppHandle,
    sessions: State<'_, service_capture::session::CaptureSessionStore>,
    input: Value,
) -> Result<CaptureSessionStatusDto, error::CommandError> {
    correlation::in_command_scope("close_capture_session", async {
        let input = CaptureStationIdInputDto::parse(input)?;
        let label = capture_window_label(&input.station_id);
        if let Some(window) = app.get_webview_window(&label) {
            window
                .close()
                .map_err(|error| format!("关闭网页登录窗口失败: {error}"))?;
        }
        Ok(sessions.clear(&input.station_id)?)
    })
    .await
}

#[tauri::command]
pub async fn finish_capture_session(
    facade: State<'_, CaptureCommandFacade>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("finish_capture_session", async {
        let input = CaptureStationIdInputDto::parse(input)?;
        facade
            .finish_capture_session(input.station_id)
            .await
            .map_err(capture_command_error)
    })
    .await
}

#[tauri::command]
pub async fn finish_web_authorization_session(
    app: tauri::AppHandle,
    facade: State<'_, CaptureCommandFacade>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("finish_web_authorization_session", async {
        let input = CaptureStationIdInputDto::parse(input)?;
        let cookie_url = facade
            .web_authorization_cookie_url(&input.station_id)
            .await
            .map_err(CaptureCommandError::Message)
            .map_err(capture_command_error)?;
        let cookie_header = read_capture_window_cookies(app, &input.station_id, &cookie_url)
            .map_err(capture_command_error)?;
        facade
            .finish_web_authorization_session(input.station_id, cookie_header)
            .await
            .map_err(capture_command_error)
    })
    .await
}

#[tauri::command]
pub async fn finish_provider_draft_authorization_session(
    app: tauri::AppHandle,
    facade: State<'_, CaptureCommandFacade>,
    input: Value,
) -> Result<ProviderDraftPreviewDto, error::CommandError> {
    correlation::in_command_scope("finish_provider_draft_authorization_session", async {
        let input = ProviderDraftIdInputDto::parse(input)?;
        let cookie_url = facade
            .web_authorization_cookie_url(&input.draft_id)
            .await
            .map_err(CaptureCommandError::Message)
            .map_err(capture_command_error)?;
        let cookie_header = read_capture_window_cookies(app, &input.draft_id, &cookie_url)
            .map_err(capture_command_error)?;
        facade
            .finish_provider_draft_authorization_session(input.draft_id, cookie_header)
            .await
            .map_err(capture_command_error)
    })
    .await
}

fn capture_window_label(station_id: &str) -> String {
    format!(
        "capture-{}",
        station_id.replace(|character: char| !character.is_ascii_alphanumeric(), "-")
    )
}

fn open_capture_window(
    app: tauri::AppHandle,
    plan: &CaptureSessionStartPlan,
) -> Result<(), CaptureCommandError> {
    if let Some(window) = app.get_webview_window(&plan.label) {
        window.set_focus().map_err(|error| {
            CaptureCommandError::Message(format!("Failed to focus capture window: {error}"))
        })?;
    } else {
        tauri::WebviewWindowBuilder::new(
            &app,
            plan.label.clone(),
            tauri::WebviewUrl::External("about:blank".parse().map_err(|error| {
                CaptureCommandError::Message(format!(
                    "Failed to initialize capture window: {error}"
                ))
            })?),
        )
        .title(format!("Web authorization - {}", plan.target.station.name))
        .inner_size(1100.0, 760.0)
        .initialization_script(&plan.script)
        .build()
        .map_err(|error| {
            CaptureCommandError::Message(format!("Failed to open capture window: {error}"))
        })?;
        if let Some(window) = app.get_webview_window(&plan.label) {
            let target = plan.target.station.website_url.parse().map_err(|error| {
                CaptureCommandError::Message(format!(
                    "Station website URL cannot be opened for authorization: {error}"
                ))
            })?;
            let navigator = window.clone();
            window
                .run_on_main_thread(move || {
                    let _ = navigator.navigate(target);
                })
                .map_err(|error| {
                    CaptureCommandError::Message(format!(
                        "Failed to schedule capture window navigation: {error}"
                    ))
                })?;
        }
    }
    Ok(())
}

fn read_capture_window_cookies(
    app: tauri::AppHandle,
    owner_id: &str,
    website_url: &str,
) -> Result<String, CaptureCommandError> {
    let label = capture_window_label(owner_id);
    let window = app.get_webview_window(&label).ok_or_else(|| {
        CaptureCommandError::Message(
            "Capture authorization window is not available; reopen it and retry.".to_string(),
        )
    })?;
    let target = tauri::Url::parse(website_url).map_err(|error| {
        CaptureCommandError::Message(format!(
            "Station website URL cannot be used for cookie lookup: {error}"
        ))
    })?;
    let cookies = window.cookies_for_url(target).map_err(|error| {
        CaptureCommandError::Message(format!(
            "Reading capture authorization cookies failed: {error}"
        ))
    })?;
    let pairs = cookies
        .into_iter()
        .map(|cookie| (cookie.name().to_string(), cookie.value().to_string()))
        .collect::<Vec<_>>();
    service_capture::web_authorization::build_cookie_header_from_pairs(&pairs).ok_or_else(|| {
        CaptureCommandError::Message(
            "Capture authorization did not provide usable cookies; finish login in the capture window and retry."
                .to_string(),
        )
    })
}

#[cfg(test)]
fn capture_request_belongs_to_station(
    station_website_url: &str,
    station_api_base_url: &str,
    request_url: &str,
) -> bool {
    [station_website_url, station_api_base_url]
        .into_iter()
        .any(|base_url| {
            crate::services::station_endpoints::url_belongs_to_base(request_url, base_url)
        })
}

#[cfg(test)]
fn web_authorization_candidate_user_id_from_input(
    input: &crate::models::capture::CapturedHttpEventInput,
) -> Option<String> {
    let fallback_path;
    let request_path = if let Some(path) = input.request_path.as_deref() {
        path
    } else {
        fallback_path = path_from_request_url(&input.request_url);
        &fallback_path
    };
    if !service_capture::web_authorization::is_newapi_completion_candidate(
        request_path,
        input.status,
        input.response_json.as_ref(),
    ) {
        return None;
    }
    input
        .response_json
        .as_ref()
        .and_then(service_capture::web_authorization::extract_verified_user_id)
}

#[cfg(test)]
fn path_from_request_url(url: &str) -> String {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let path = without_scheme
        .find('/')
        .map(|index| &without_scheme[index..])
        .unwrap_or("/");
    path.split(['?', '#']).next().unwrap_or("/").to_string()
}

#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::capture::CapturedHttpEventInput;
    use serde_json::json;

    #[test]
    fn capture_request_belongs_to_management_base_when_station_url_uses_v1() {
        assert!(capture_request_belongs_to_station(
            "https://relay.example.com",
            "https://relay.example.com/v1",
            "https://relay.example.com/api/v1/auth/login"
        ));
    }

    #[test]
    fn capture_request_rejects_other_station_origins() {
        assert!(!capture_request_belongs_to_station(
            "https://relay.example.com",
            "https://relay.example.com/v1",
            "https://other.example.com/api/v1/auth/login"
        ));
    }

    #[test]
    fn capture_accepts_configured_origins_and_rejects_lookalikes() {
        assert!(capture_request_belongs_to_station(
            "https://console.example:443",
            "https://api.example/v1",
            "https://console.example/api/user/self",
        ));
        assert!(capture_request_belongs_to_station(
            "https://console.example",
            "https://api.example/v1",
            "https://api.example/v1/models",
        ));
        assert!(!capture_request_belongs_to_station(
            "https://console.example",
            "https://api.example/v1",
            "https://console.example.evil.test/api/user/self",
        ));
    }

    #[test]
    fn captured_newapi_self_event_marks_web_authorization_candidate() {
        let input = CapturedHttpEventInput {
            station_id: "station-1".to_string(),
            source_window_id: "capture-station-1".to_string(),
            page_url: "https://relay.example/console".to_string(),
            request_url: "https://relay.example/api/user/self".to_string(),
            request_path: Some("/api/user/self".to_string()),
            method: "GET".to_string(),
            status: Some(200),
            content_type: Some("application/json".to_string()),
            started_at: None,
            finished_at: None,
            duration_ms: None,
            response_kind: Some("json".to_string()),
            response_size: None,
            response_json: Some(json!({ "success": true, "data": { "id": 42 } })),
            response_text: None,
            error_message: None,
        };

        assert_eq!(
            web_authorization_candidate_user_id_from_input(&input).as_deref(),
            Some("42")
        );
    }

    #[test]
    fn capture_script_invokes_web_authorization_finish_after_candidate() {
        let script = capture_script("station-1", "capture-station-1", None, None);

        assert!(script.contains("finish_web_authorization_session"));
        assert!(script.contains("webAuthorizationCandidate"));
        assert!(script.contains("__relayPoolAuthorizationFinishInFlight"));
        assert!(script.contains("input[placeholder*='邮箱']"));
        assert!(script.contains("input[placeholder*='账号']"));
        assert!(script.contains("input[placeholder*='密码']"));
        assert!(script.contains("text.includes(\"中华人民共和国\")"));
    }
}
