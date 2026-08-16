use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde_json::Value;
use tauri::{AppHandle, WebviewUrl, WebviewWindow};

use crate::services::remote_keys::{RemoteKeyExternalFailureReason, RemoteKeyOperationError};

const PAGE_READY_TIMEOUT: Duration = Duration::from_secs(12);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(35);
const EVAL_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const MAX_CALLBACK_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserTransportFailureKind {
    InvalidWebsiteUrl,
    WindowUnavailable,
    NavigationTimeout,
    ScriptUnavailable,
    RequestTimeout,
    AuthenticationRejected,
    Rejected,
    MalformedPayload,
    ResponseTooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BrowserTransportError {
    kind: BrowserTransportFailureKind,
    status: Option<u16>,
}

impl BrowserTransportError {
    fn new(kind: BrowserTransportFailureKind) -> Self {
        Self { kind, status: None }
    }

    fn rejected(status: Option<u16>) -> Self {
        let kind = if matches!(status, Some(401 | 403)) {
            BrowserTransportFailureKind::AuthenticationRejected
        } else {
            BrowserTransportFailureKind::Rejected
        };
        Self { kind, status }
    }

    pub(crate) fn into_remote_key_error(self) -> RemoteKeyOperationError {
        let reason = match self.kind {
            BrowserTransportFailureKind::RequestTimeout => RemoteKeyExternalFailureReason::TimedOut,
            BrowserTransportFailureKind::AuthenticationRejected => {
                RemoteKeyExternalFailureReason::AuthenticationRejected
            }
            BrowserTransportFailureKind::MalformedPayload
            | BrowserTransportFailureKind::ResponseTooLarge => {
                RemoteKeyExternalFailureReason::MalformedPayload
            }
            BrowserTransportFailureKind::InvalidWebsiteUrl
            | BrowserTransportFailureKind::WindowUnavailable
            | BrowserTransportFailureKind::NavigationTimeout
            | BrowserTransportFailureKind::ScriptUnavailable
            | BrowserTransportFailureKind::Rejected => {
                RemoteKeyExternalFailureReason::ProviderUnavailable
            }
        };
        let detail = match (self.kind, self.status) {
            (BrowserTransportFailureKind::InvalidWebsiteUrl, _) => {
                "The station website URL cannot be used for browser-assisted key discovery."
                    .to_string()
            }
            (BrowserTransportFailureKind::WindowUnavailable, _) => {
                "The browser-assisted key request window could not be created.".to_string()
            }
            (BrowserTransportFailureKind::NavigationTimeout, _) => {
                "The browser-assisted key request could not reach the station origin.".to_string()
            }
            (BrowserTransportFailureKind::ScriptUnavailable, _) => {
                "The station page did not accept the browser-assisted key request.".to_string()
            }
            (BrowserTransportFailureKind::RequestTimeout, _) => {
                "The browser-assisted key request timed out.".to_string()
            }
            (BrowserTransportFailureKind::AuthenticationRejected, Some(status)) => format!(
                "The browser-assisted key request was rejected (HTTP {status}); re-authorize the station session."
            ),
            (BrowserTransportFailureKind::AuthenticationRejected, None) => {
                "The browser-assisted key request rejected the saved station session.".to_string()
            }
            (BrowserTransportFailureKind::Rejected, Some(status)) => format!(
                "The browser-assisted key request failed (HTTP {status})."
            ),
            (BrowserTransportFailureKind::Rejected, None) => {
                "The browser-assisted key request failed.".to_string()
            }
            (BrowserTransportFailureKind::MalformedPayload, _) => {
                "The browser-assisted key response was not a valid Sub2API key list.".to_string()
            }
            (BrowserTransportFailureKind::ResponseTooLarge, _) => {
                "The browser-assisted key response exceeded the local safety limit.".to_string()
            }
        };
        RemoteKeyOperationError::ExternalUnavailableWithDetail { reason, detail }
    }
}

pub(crate) async fn fetch_sub2api_remote_key_list(
    app: &AppHandle,
    website_url: &str,
    access_token: Option<&str>,
) -> Result<Value, BrowserTransportError> {
    let target = tauri::Url::parse(website_url)
        .ok()
        .filter(valid_browser_target)
        .ok_or_else(|| {
            BrowserTransportError::new(BrowserTransportFailureKind::InvalidWebsiteUrl)
        })?;
    // Use the shared WebView profile so browser cookies/storage remain available,
    // but never reuse the capture window: its fetch hook would route the full
    // key-list response through the ordinary capture IPC DTO before redaction.
    let window = tauri::WebviewWindowBuilder::new(
        app,
        format!("browser-key-fetch-{}", uuid::Uuid::now_v7()),
        WebviewUrl::External(target.clone()),
    )
    .title("Provider key request")
    .visible(false)
    .skip_taskbar(true)
    .build()
    .map_err(|_| BrowserTransportError::new(BrowserTransportFailureKind::WindowUnavailable))?;

    let result = fetch_from_window(&window, &target, access_token).await;
    let _ = window.close();
    result
}

async fn fetch_from_window(
    window: &WebviewWindow,
    target: &tauri::Url,
    access_token: Option<&str>,
) -> Result<Value, BrowserTransportError> {
    wait_for_same_origin_document(window, target).await?;
    let request_id = uuid::Uuid::now_v7().to_string();
    let script = browser_key_list_script(&request_id, access_token);
    let started = evaluate_json(window, script, EVAL_TIMEOUT).await?;
    if started.get("started").and_then(Value::as_bool) != Some(true) {
        return Err(BrowserTransportError::new(
            BrowserTransportFailureKind::ScriptUnavailable,
        ));
    }

    let request_id_json = serde_json::to_string(&request_id)
        .map_err(|_| BrowserTransportError::new(BrowserTransportFailureKind::ScriptUnavailable))?;
    let poll_script = format!(
        r#"(() => {{
  const bridge = window.__relayPoolNativeKeyBridge;
  return bridge && typeof bridge.take === "function" ? bridge.take({request_id_json}) : null;
}})()"#
    );
    let deadline = Instant::now() + REQUEST_TIMEOUT;
    loop {
        if Instant::now() >= deadline {
            return Err(BrowserTransportError::new(
                BrowserTransportFailureKind::RequestTimeout,
            ));
        }
        match evaluate_json(window, poll_script.clone(), EVAL_TIMEOUT).await {
            Ok(Value::Null) => {}
            Ok(result) => return browser_result_payload(result),
            Err(error) if error.kind == BrowserTransportFailureKind::MalformedPayload => {}
            Err(error) => return Err(error),
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn wait_for_same_origin_document(
    window: &WebviewWindow,
    target: &tauri::Url,
) -> Result<(), BrowserTransportError> {
    let deadline = Instant::now() + PAGE_READY_TIMEOUT;
    while Instant::now() < deadline {
        let at_target_origin = window
            .url()
            .ok()
            .is_some_and(|current| same_origin(&current, target));
        if at_target_origin {
            let readiness = evaluate_json(
                window,
                "({ readyState: document.readyState, origin: window.location.origin })",
                EVAL_TIMEOUT,
            )
            .await;
            if readiness.ok().is_some_and(|value| {
                matches!(
                    value.get("readyState").and_then(Value::as_str),
                    Some("interactive" | "complete")
                )
            }) {
                return Ok(());
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    Err(BrowserTransportError::new(
        BrowserTransportFailureKind::NavigationTimeout,
    ))
}

async fn evaluate_json(
    window: &WebviewWindow,
    script: impl Into<String>,
    timeout: Duration,
) -> Result<Value, BrowserTransportError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = Arc::new(Mutex::new(Some(sender)));
    window
        .eval_with_callback(script, move |raw| {
            if let Ok(mut sender) = sender.lock() {
                if let Some(sender) = sender.take() {
                    let _ = sender.send(raw);
                }
            }
        })
        .map_err(|_| BrowserTransportError::new(BrowserTransportFailureKind::ScriptUnavailable))?;
    let raw = tokio::time::timeout(timeout, receiver)
        .await
        .map_err(|_| BrowserTransportError::new(BrowserTransportFailureKind::ScriptUnavailable))?
        .map_err(|_| BrowserTransportError::new(BrowserTransportFailureKind::ScriptUnavailable))?;
    if raw.len() > MAX_CALLBACK_BYTES {
        return Err(BrowserTransportError::new(
            BrowserTransportFailureKind::ResponseTooLarge,
        ));
    }
    serde_json::from_str(&raw)
        .map_err(|_| BrowserTransportError::new(BrowserTransportFailureKind::MalformedPayload))
}

fn browser_result_payload(result: Value) -> Result<Value, BrowserTransportError> {
    if result.get("ok").and_then(Value::as_bool) == Some(true) {
        return result.get("payload").cloned().ok_or_else(|| {
            BrowserTransportError::new(BrowserTransportFailureKind::MalformedPayload)
        });
    }
    let status = result
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok());
    match result.get("kind").and_then(Value::as_str) {
        Some("timeout") => Err(BrowserTransportError::new(
            BrowserTransportFailureKind::RequestTimeout,
        )),
        Some("invalid_json" | "malformed_payload") => Err(BrowserTransportError::new(
            BrowserTransportFailureKind::MalformedPayload,
        )),
        Some("response_too_large") => Err(BrowserTransportError::new(
            BrowserTransportFailureKind::ResponseTooLarge,
        )),
        _ => Err(BrowserTransportError::rejected(status)),
    }
}

fn valid_browser_target(url: &tauri::Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
}

fn same_origin(left: &tauri::Url, right: &tauri::Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn browser_key_list_script(request_id: &str, access_token: Option<&str>) -> String {
    let request_id = serde_json::to_string(request_id).unwrap_or_else(|_| "null".to_string());
    let access_token = serde_json::to_string(&access_token).unwrap_or_else(|_| "null".to_string());
    format!(
        r#"(() => {{
  if (!window.__relayPoolNativeKeyBridge) {{
    const results = new Map();
    Object.defineProperty(window, "__relayPoolNativeKeyBridge", {{
      configurable: true,
      enumerable: false,
      value: {{
        put: (id, result) => results.set(id, result),
        take: (id) => {{
          const result = results.get(id) || null;
          if (result) results.delete(id);
          return result;
        }},
      }},
    }});
  }}
  const requestId = {request_id};
  const suppliedToken = {access_token};
  const bridge = window.__relayPoolNativeKeyBridge;
  const scalarNumber = (value) => {{
    const parsed = typeof value === "number" ? value : Number.parseInt(String(value), 10);
    return Number.isFinite(parsed) && parsed >= 0 ? parsed : null;
  }};
  const tokenFromValue = (value, keyHint = "", depth = 0) => {{
    if (depth > 4 || value == null) return null;
    if (typeof value === "string") {{
      const text = value.trim();
      if (!text) return null;
      const normalized = String(keyHint).replace(/[-_]/g, "").toLowerCase();
      if (["accesstoken", "authtoken", "token", "jwt"].includes(normalized)
          || (text.length > 40 && text.split(".").length === 3)) return text;
      try {{ return tokenFromValue(JSON.parse(text), keyHint, depth + 1); }} catch (_) {{ return null; }}
    }}
    if (Array.isArray(value)) {{
      return value.map((item) => tokenFromValue(item, keyHint, depth + 1)).find(Boolean) || null;
    }}
    if (typeof value === "object") {{
      for (const [key, child] of Object.entries(value)) {{
        const found = tokenFromValue(child, key, depth + 1);
        if (found) return found;
      }}
    }}
    return null;
  }};
  const storedToken = () => {{
    for (const storage of [window.localStorage, window.sessionStorage]) {{
      try {{
        for (let index = 0; index < storage.length; index += 1) {{
          const key = storage.key(index);
          if (!key) continue;
          const found = tokenFromValue(storage.getItem(key), key);
          if (found) return found;
        }}
      }} catch (_) {{}}
    }}
    return null;
  }};
  void (async () => {{
    const allItems = [];
    const timezone = (() => {{
      try {{ return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC"; }} catch (_) {{ return "UTC"; }}
    }})();
    const token = suppliedToken || storedToken();
    const headers = {{
      "Accept": "application/json, text/plain, */*",
      "Accept-Language": navigator.language || "en",
      "Content-Type": "application/json",
      "X-User-UI-Request": "1",
    }};
    if (token) headers.Authorization = `Bearer ${{token}}`;
    try {{
      for (let page = 1; page <= 10000; page += 1) {{
        const endpoint = new URL("/api/v1/keys", window.location.origin);
        endpoint.searchParams.set("page", String(page));
        endpoint.searchParams.set("page_size", "20");
        endpoint.searchParams.set("sort_by", "created_at");
        endpoint.searchParams.set("sort_order", "desc");
        endpoint.searchParams.set("timezone", timezone);
        const controller = new AbortController();
        const timer = window.setTimeout(() => controller.abort(), 15000);
        let response;
        try {{
          response = await window.fetch(endpoint.toString(), {{
            method: "GET",
            credentials: "include",
            cache: "no-store",
            headers,
            signal: controller.signal,
          }});
        }} finally {{
          window.clearTimeout(timer);
        }}
        const contentType = (response.headers.get("content-type") || "").toLowerCase();
        if (!response.ok) {{
          bridge.put(requestId, {{
            ok: false,
            status: response.status,
            kind: contentType.includes("html") ? "html" : "rejected",
          }});
          return;
        }}
        const text = await response.text();
        if (text.length > 1048576) {{
          bridge.put(requestId, {{ ok: false, status: response.status, kind: "response_too_large" }});
          return;
        }}
        let payload;
        try {{ payload = JSON.parse(text); }}
        catch (_) {{
          bridge.put(requestId, {{ ok: false, status: response.status, kind: "invalid_json" }});
          return;
        }}
        const data = payload && payload.data;
        const items = data && Array.isArray(data.items) ? data.items : null;
        if (!items) {{
          bridge.put(requestId, {{ ok: false, status: response.status, kind: "malformed_payload" }});
          return;
        }}
        allItems.push(...items);
        const total = scalarNumber(data.total);
        const pages = scalarNumber(data.pages);
        const pageSize = scalarNumber(data.page_size) || 20;
        const complete = total !== null
          ? allItems.length >= total
          : items.length === 0 || items.length < pageSize || (pages !== null && page >= pages);
        if (complete) {{
          if (total !== null && allItems.length !== total) {{
            bridge.put(requestId, {{ ok: false, status: response.status, kind: "malformed_payload" }});
            return;
          }}
          bridge.put(requestId, {{ ok: true, payload: {{ data: {{ items: allItems }} }} }});
          return;
        }}
      }}
      bridge.put(requestId, {{ ok: false, kind: "response_too_large" }});
    }} catch (error) {{
      bridge.put(requestId, {{
        ok: false,
        kind: error && error.name === "AbortError" ? "timeout" : "request_failed",
      }});
    }}
  }})();
  return {{ started: true }};
}})()"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_key_script_is_origin_relative_and_uses_browser_timezone() {
        let script = browser_key_list_script("request-1", Some("fixture-token"));

        assert!(script.contains("new URL(\"/api/v1/keys\", window.location.origin)"));
        assert!(script.contains("Intl.DateTimeFormat().resolvedOptions().timeZone"));
        assert!(script.contains("credentials: \"include\""));
    }

    #[test]
    fn browser_result_never_uses_a_response_body_as_an_error() {
        let error = browser_result_payload(serde_json::json!({
            "ok": false,
            "status": 403,
            "kind": "html",
            "body": "fixture-sensitive-body"
        }))
        .expect_err("403 should be rejected");

        assert_eq!(
            error.kind,
            BrowserTransportFailureKind::AuthenticationRejected
        );
        assert_eq!(error.status, Some(403));
        let mapped = error.into_remote_key_error();
        assert!(matches!(
            mapped,
            RemoteKeyOperationError::ExternalUnavailableWithDetail { detail, .. }
                if !detail.contains("fixture-sensitive-body")
        ));
    }
}
