use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde_json::Value;
use tauri::{webview::Cookie, AppHandle, WebviewUrl, WebviewWindow};

use crate::services::remote_keys::{RemoteKeyExternalFailureReason, RemoteKeyOperationError};

const PAGE_READY_TIMEOUT: Duration = Duration::from_secs(12);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(35);
const EVAL_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const MAX_CALLBACK_BYTES: usize = 4 * 1024 * 1024;
const RECHARGE_PAGE_SETTLE: Duration = Duration::from_millis(180);
// SPA route guards often redirect a custom menu route to /login shortly after
// the initial document is ready. Do not follow a configured/sidebar candidate
// until that first route decision has had a short, bounded settle window.
const RECHARGE_ROUTE_SETTLE: Duration = Duration::from_millis(700);
// Recharge discovery is an interactive operation. Keep the whole bounded
// graph walk comfortably below the frontend timeout; the candidate order
// favors configured/sidebar entries over generic page links.
const RECHARGE_SCAN_TIMEOUT: Duration = Duration::from_secs(15);
const RECHARGE_INITIAL_READY_TIMEOUT: Duration = Duration::from_secs(5);
const RECHARGE_CANDIDATE_READY_TIMEOUT: Duration = Duration::from_secs(3);
const RECHARGE_MAX_CANDIDATES: usize = 3;
const RECHARGE_PROBE_INTERVAL: Duration = Duration::from_millis(180);
// External stores can serve an immediately-complete HTML shell and render
// their product list only after a follow-up request. The two known Cloudcat
// shops take roughly seven seconds on a cold load, so retain a small margin
// while keeping the enclosing scan below the frontend's 18-second deadline.
const RECHARGE_PROBE_TIMEOUT: Duration = Duration::from_secs(9);

/// Credentials used only to bootstrap the temporary recharge WebView. The
/// values are never included in a collector snapshot or diagnostic message.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RechargeSession<'a> {
    pub cookie: Option<&'a str>,
    pub access_token: Option<&'a str>,
    pub refresh_token: Option<&'a str>,
    pub newapi_user_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RechargeDocumentState {
    url: String,
    document_id: Option<String>,
    time_origin: Option<String>,
    route_version: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RechargePageEntry {
    pub url: String,
    pub label: String,
    pub provider: Option<String>,
    pub payment_methods: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RechargeCandidate {
    url: String,
    label: String,
    #[serde(default = "default_recharge_candidate_priority")]
    priority: u8,
}

fn default_recharge_candidate_priority() -> u8 {
    99
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RechargePageProbe {
    pub status: String,
    pub current_url: String,
    pub title: String,
    pub provider: Option<String>,
    pub payment_methods: Vec<String>,
    pub entries: Vec<RechargePageEntry>,
    pub candidates: Vec<RechargeCandidate>,
    pub protected_candidates: Vec<String>,
    #[serde(default)]
    pub candidate_diagnostics: Vec<RechargeCandidateDiagnostic>,
    pub evidence: Vec<String>,
    #[serde(default)]
    pub candidates_scanned: usize,
    #[serde(default)]
    pub loading: bool,
}

/// Candidate-level evidence is persisted only after URL sanitization. It is
/// intentionally separate from `RechargeCandidate`, whose URL is kept in
/// memory for the duration of navigation so authenticated custom-page links
/// do not lose their transient token.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RechargeCandidateDiagnostic {
    pub url: String,
    pub label: String,
    pub status: String,
    pub current_url: String,
    pub evidence: Vec<String>,
    pub provider: Option<String>,
    pub payment_methods: Vec<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RechargeScanPhase {
    InitialNavigation,
    InitialDocumentReady,
    InitialScan,
    CandidateNavigation,
    CandidateDocumentReady,
    CandidateScan,
    OverallTimeout,
}

impl RechargeScanPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::InitialNavigation => "initial_navigation",
            Self::InitialDocumentReady => "initial_document_ready",
            Self::InitialScan => "initial_scan",
            Self::CandidateNavigation => "candidate_navigation",
            Self::CandidateDocumentReady => "candidate_document_ready",
            Self::CandidateScan => "candidate_scan",
            Self::OverallTimeout => "timed_out",
        }
    }
}

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
    CrossOriginRedirect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowserTransportError {
    kind: BrowserTransportFailureKind,
    status: Option<u16>,
    recharge_phase: Option<RechargeScanPhase>,
    recharge_url: Option<String>,
    recharge_payload_shape: Option<String>,
}

impl BrowserTransportError {
    fn new(kind: BrowserTransportFailureKind) -> Self {
        Self {
            kind,
            status: None,
            recharge_phase: None,
            recharge_url: None,
            recharge_payload_shape: None,
        }
    }

    fn rejected(status: Option<u16>) -> Self {
        let kind = if matches!(status, Some(401 | 403)) {
            BrowserTransportFailureKind::AuthenticationRejected
        } else {
            BrowserTransportFailureKind::Rejected
        };
        Self {
            kind,
            status,
            recharge_phase: None,
            recharge_url: None,
            recharge_payload_shape: None,
        }
    }

    pub(crate) fn is_timeout(&self) -> bool {
        matches!(
            self.kind,
            BrowserTransportFailureKind::NavigationTimeout
                | BrowserTransportFailureKind::RequestTimeout
        )
    }

    fn with_recharge_context(
        mut self,
        phase: RechargeScanPhase,
        current_url: Option<&tauri::Url>,
    ) -> Self {
        self.recharge_phase = Some(phase);
        self.recharge_url = current_url.map(|url| sanitize_recharge_url(url.as_str()));
        self
    }

    fn with_recharge_payload_shape(mut self, shape: impl Into<String>) -> Self {
        self.recharge_payload_shape = Some(shape.into());
        self
    }

    pub(crate) fn recharge_diagnostic(&self) -> serde_json::Value {
        serde_json::json!({
            "phase": self.recharge_phase.map(RechargeScanPhase::as_str).unwrap_or("unknown"),
            "kind": match self.kind {
                BrowserTransportFailureKind::NavigationTimeout => "navigation_timeout",
                BrowserTransportFailureKind::CrossOriginRedirect => "cross_origin_redirect",
                BrowserTransportFailureKind::ScriptUnavailable => "script_unavailable",
                BrowserTransportFailureKind::WindowUnavailable => "window_unavailable",
                BrowserTransportFailureKind::InvalidWebsiteUrl => "invalid_website_url",
                BrowserTransportFailureKind::MalformedPayload => "malformed_payload",
                BrowserTransportFailureKind::ResponseTooLarge => "response_too_large",
                BrowserTransportFailureKind::RequestTimeout => "request_timeout",
                BrowserTransportFailureKind::AuthenticationRejected => "authentication_rejected",
                BrowserTransportFailureKind::Rejected => "rejected",
            },
            "url": self.recharge_url,
            "payloadShape": self.recharge_payload_shape,
        })
    }

    pub(crate) fn recharge_message(&self) -> String {
        match self.kind {
            BrowserTransportFailureKind::CrossOriginRedirect => {
                "充值页面跳转到了外部站点，已停止扫描。".to_string()
            }
            BrowserTransportFailureKind::NavigationTimeout => format!(
                "充值扫描在 {} 阶段等待页面完成时超时。",
                self.recharge_phase
                    .map(RechargeScanPhase::as_str)
                    .unwrap_or("navigation")
            ),
            BrowserTransportFailureKind::ScriptUnavailable => format!(
                "充值扫描在 {} 阶段无法读取页面。",
                self.recharge_phase
                    .map(RechargeScanPhase::as_str)
                    .unwrap_or("page_script")
            ),
            _ => "充值页面采集失败，请检查站点地址和登录状态。".to_string(),
        }
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
            | BrowserTransportFailureKind::CrossOriginRedirect
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
            (BrowserTransportFailureKind::CrossOriginRedirect, _) => {
                "The browser-assisted key request was redirected outside the station origin."
                    .to_string()
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

/// Inspect a rendered station page in the shared WebView profile.
///
/// This reads visible DOM text, real anchors, and an explicit inline app
/// configuration object when a provider publishes one. It never inspects
/// external script bundles, whose route strings would make SPA fallbacks look
/// like valid purchase pages.
pub(crate) async fn scan_recharge_page(
    app: &AppHandle,
    website_url: &str,
    station_type: &str,
    session: RechargeSession<'_>,
) -> Result<RechargePageProbe, BrowserTransportError> {
    let target = tauri::Url::parse(website_url)
        .ok()
        .filter(valid_browser_target)
        .ok_or_else(|| {
            BrowserTransportError::new(BrowserTransportFailureKind::InvalidWebsiteUrl)
        })?;
    let window = tauri::WebviewWindowBuilder::new(
        app,
        format!("browser-recharge-scan-{}", uuid::Uuid::now_v7()),
        WebviewUrl::External(tauri::Url::parse("about:blank").map_err(|_| {
            BrowserTransportError::new(BrowserTransportFailureKind::WindowUnavailable)
        })?),
    )
    .title("Station recharge scan")
    .visible(false)
    .skip_taskbar(true)
    .initialization_script(recharge_initialization_script_for_station_type(
        &target,
        session,
        station_type,
    ))
    .build()
    .map_err(|_| BrowserTransportError::new(BrowserTransportFailureKind::WindowUnavailable))?;

    let cookies = recharge_session_cookies(session.cookie, &target);
    let navigation_target = target.clone();
    let navigation_window = window.clone();
    let (initialization_sender, initialization_receiver) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        // Cookie injection is best effort. WebView2/WebKit can reject an
        // individual domain or cookie attribute while navigation itself is
        // still valid; let the page report `login_required` instead of
        // turning that into a generic window failure.
        for cookie in cookies {
            let _ = navigation_window.set_cookie(cookie);
        }
        let navigated = navigation_window.navigate(navigation_target).is_ok();
        let _ = initialization_sender.send(navigated);
    })
    .map_err(|_| {
        BrowserTransportError::new(BrowserTransportFailureKind::WindowUnavailable)
            .with_recharge_context(RechargeScanPhase::InitialNavigation, None)
    })?;
    let initialized = tokio::time::timeout(EVAL_TIMEOUT, initialization_receiver)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or(false);
    if !initialized {
        let _ = window.close();
        return Err(
            BrowserTransportError::new(BrowserTransportFailureKind::WindowUnavailable)
                .with_recharge_context(RechargeScanPhase::InitialNavigation, None),
        );
    }

    let result = match tokio::time::timeout(RECHARGE_SCAN_TIMEOUT, async {
        wait_for_recharge_document(
            &window,
            &target,
            RECHARGE_INITIAL_READY_TIMEOUT,
            RechargeScanPhase::InitialDocumentReady,
            None,
            false,
        )
        .await?;
        let initial_value = evaluate_recharge_value(
            &window,
            RECHARGE_PROBE_TIMEOUT,
            RechargeScanPhase::InitialScan,
        )
        .await
        .map_err(|error| {
            let current = window.url().ok();
            error.with_recharge_context(RechargeScanPhase::InitialScan, current.as_ref())
        })?;
        let mut previous_document = recharge_document_state_from_value(&initial_value)
            .or_else(|| recharge_document_state_fallback(&window));
        let mut probe = scan_recharge_document(
            &window,
            initial_value,
            RECHARGE_PROBE_TIMEOUT,
            RechargeScanPhase::InitialScan,
        )
        .await?;
        let mut queue = VecDeque::from(std::mem::take(&mut probe.candidates));
        let mut deferred_custom_url = None;
        let mut deferred_custom_entries =
            if is_custom_recharge_wrapper_url(&probe.current_url, &target)
                && has_external_recharge_candidate(&queue, &target)
            {
                deferred_custom_url = Some(probe.current_url.clone());
                std::mem::take(&mut probe.entries)
            } else {
                Vec::new()
            };
        let mut seen_candidates = HashSet::new();
        if let Some(initial_url) = window.url().ok() {
            seen_candidates.insert(sanitize_recharge_url(initial_url.as_str()));
        }
        let mut scanned = 0usize;

        // A login page is a terminal state. It is important not to follow
        // links found in a public shell and accidentally expose an entry that
        // the authenticated station UI did not actually make available.
        while probe.status != "login_required"
            && probe.status != "not_found"
            && probe.entries.is_empty()
            && scanned < RECHARGE_MAX_CANDIDATES
        {
            let Some(candidate) = queue.pop_front() else {
                break;
            };
            // Keep the original candidate URL for transient navigation. Only
            // the identity used for deduplication and diagnostics is sanitized.
            let Some(candidate_url) = normalize_recharge_candidate_url(&candidate.url) else {
                continue;
            };
            let Some(candidate_identity) = sanitize_recharge_candidate_url(&candidate_url) else {
                continue;
            };
            if !seen_candidates.insert(candidate_identity.clone()) {
                continue;
            }
            let candidate_target = tauri::Url::parse(&candidate_url).map_err(|_| {
                BrowserTransportError::new(BrowserTransportFailureKind::InvalidWebsiteUrl)
                    .with_recharge_context(
                        RechargeScanPhase::CandidateNavigation,
                        window.url().ok().as_ref(),
                    )
            })?;
            scanned += 1;
            let navigation = serde_json::to_string(&candidate_url).map_err(|_| {
                BrowserTransportError::new(BrowserTransportFailureKind::ScriptUnavailable)
                    .with_recharge_context(
                        RechargeScanPhase::CandidateNavigation,
                        window.url().ok().as_ref(),
                    )
            })?;
            let current = window.url().ok();
            window
                .eval(format!(
                    "(() => {{ window.location.assign({navigation}); }})()"
                ))
                .map_err(|_| {
                    BrowserTransportError::new(BrowserTransportFailureKind::ScriptUnavailable)
                        .with_recharge_context(
                            RechargeScanPhase::CandidateNavigation,
                            current.as_ref(),
                        )
                })?;
            if let Err(error) = wait_for_recharge_document(
                &window,
                &candidate_target,
                RECHARGE_CANDIDATE_READY_TIMEOUT,
                RechargeScanPhase::CandidateDocumentReady,
                previous_document.as_ref(),
                true,
            )
            .await
            {
                probe
                    .candidate_diagnostics
                    .push(RechargeCandidateDiagnostic {
                        url: candidate_identity,
                        label: candidate.label.clone(),
                        status: "navigation_failed".to_string(),
                        current_url: window
                            .url()
                            .ok()
                            .map(|url| sanitize_recharge_url(url.as_str()))
                            .unwrap_or_default(),
                        evidence: Vec::new(),
                        provider: None,
                        payment_methods: Vec::new(),
                        reason: Some(recharge_failure_kind(error.kind).to_string()),
                    });
                if matches!(error.kind, BrowserTransportFailureKind::CrossOriginRedirect)
                    || error.is_timeout()
                {
                    // An invalid or stale candidate must not hold the whole
                    // scan hostage; the remaining candidates still have value.
                    continue;
                }
                return Err(error);
            }
            let candidate_value = match evaluate_recharge_value(
                &window,
                RECHARGE_PROBE_TIMEOUT,
                RechargeScanPhase::CandidateScan,
            )
            .await
            {
                Ok(value) => value,
                Err(error) => {
                    probe
                        .candidate_diagnostics
                        .push(RechargeCandidateDiagnostic {
                            url: candidate_identity,
                            label: candidate.label.clone(),
                            status: "probe_failed".to_string(),
                            current_url: window
                                .url()
                                .ok()
                                .map(|url| sanitize_recharge_url(url.as_str()))
                                .unwrap_or_default(),
                            evidence: Vec::new(),
                            provider: None,
                            payment_methods: Vec::new(),
                            reason: Some(recharge_failure_kind(error.kind).to_string()),
                        });
                    continue;
                }
            };
            previous_document = recharge_document_state_from_value(&candidate_value)
                .or_else(|| recharge_document_state_fallback(&window));
            let candidate_probe = match scan_recharge_document(
                &window,
                candidate_value,
                RECHARGE_PROBE_TIMEOUT,
                RechargeScanPhase::CandidateScan,
            )
            .await
            {
                Ok(probe) => probe,
                Err(error) => {
                    probe
                        .candidate_diagnostics
                        .push(RechargeCandidateDiagnostic {
                            url: candidate_identity,
                            label: candidate.label.clone(),
                            status: "probe_failed".to_string(),
                            current_url: window
                                .url()
                                .ok()
                                .map(|url| sanitize_recharge_url(url.as_str()))
                                .unwrap_or_default(),
                            evidence: Vec::new(),
                            provider: None,
                            payment_methods: Vec::new(),
                            reason: Some(recharge_failure_kind(error.kind).to_string()),
                        });
                    continue;
                }
            };
            probe
                .candidate_diagnostics
                .push(RechargeCandidateDiagnostic {
                    url: candidate_identity,
                    label: candidate.label.clone(),
                    status: candidate_probe.status.clone(),
                    current_url: sanitize_recharge_url(&candidate_probe.current_url),
                    evidence: candidate_probe.evidence.clone(),
                    provider: candidate_probe.provider.clone(),
                    payment_methods: candidate_probe.payment_methods.clone(),
                    reason: None,
                });
            let candidate_queue = VecDeque::from(candidate_probe.candidates.clone());
            let candidate_is_custom_wrapper = candidate_probe.status == "success"
                && is_custom_recharge_wrapper_url(&candidate_probe.current_url, &target)
                && has_external_recharge_candidate(&candidate_queue, &target);
            if candidate_is_custom_wrapper {
                // A custom route is only a station-owned wrapper around the
                // actual shop. Hold it as a fallback while its external
                // candidate is validated; a successful external page will
                // replace this deferred entry below.
                if !candidate_probe.entries.is_empty() {
                    deferred_custom_url = Some(candidate_probe.current_url.clone());
                    deferred_custom_entries = candidate_probe.entries.clone();
                }
            } else if candidate_probe.status == "success" {
                for mut entry in candidate_probe.entries {
                    if !candidate.label.trim().is_empty() {
                        entry.label = candidate.label.clone();
                    }
                    probe.entries.push(entry);
                }
                probe
                    .payment_methods
                    .extend(candidate_probe.payment_methods);
                probe.payment_methods.sort();
                probe.payment_methods.dedup();
                if probe.provider.is_none() {
                    probe.provider = candidate_probe.provider;
                }
                probe.evidence.extend(candidate_probe.evidence);
                probe.current_url = candidate_probe.current_url;
                probe.loading = candidate_probe.loading;
                // A confirmed product/payment page is the terminal discovery
                // result. Its payment methods and products are already part
                // of the page evidence; probing unrelated fallback URLs after
                // this point can turn a valid result into an overall timeout.
                break;
            } else if candidate_probe.status == "login_required" {
                probe
                    .protected_candidates
                    .push(sanitize_recharge_url(&candidate_url));
            }
            for nested in candidate_probe.candidates {
                if let Some(nested_url) = normalize_recharge_candidate_url(&nested.url) {
                    let Some(nested_identity) = sanitize_recharge_candidate_url(&nested_url) else {
                        continue;
                    };
                    if !seen_candidates.contains(&nested_identity) {
                        queue.push_back(RechargeCandidate {
                            url: nested_url,
                            label: nested.label,
                            priority: 20,
                        });
                    }
                }
            }
            probe.current_url = candidate_probe.current_url;
            probe.loading = candidate_probe.loading;
        }
        if probe.entries.is_empty() && !deferred_custom_entries.is_empty() {
            // Preserve the authenticated custom route as a fallback when all
            // external candidates fail validation or navigation.
            probe.entries = deferred_custom_entries;
            if let Some(custom_url) = deferred_custom_url {
                probe.current_url = custom_url;
            }
        }
        probe.candidates_scanned = scanned;
        for candidate in &mut probe.candidates {
            candidate.url = sanitize_recharge_url(&candidate.url);
        }
        probe
            .candidates
            .retain(|candidate| !candidate.url.is_empty());
        for entry in &mut probe.entries {
            entry.url = sanitize_recharge_url(&entry.url);
        }
        probe.current_url = sanitize_recharge_url(&probe.current_url);
        probe
            .entries
            .sort_by(|left, right| left.url.cmp(&right.url));
        probe.entries.dedup_by(|left, right| left.url == right.url);
        probe.protected_candidates.sort();
        probe.protected_candidates.dedup();
        probe
            .candidate_diagnostics
            .sort_by(|left, right| left.url.cmp(&right.url));
        probe.evidence.sort();
        probe.evidence.dedup();
        if probe.status == "no_match" && !probe.entries.is_empty() {
            probe.status = "success".to_string();
        } else if probe.status == "no_match" && !probe.protected_candidates.is_empty() {
            probe.status = "login_required".to_string();
        }
        Ok(probe)
    })
    .await
    {
        Ok(result) => result,
        Err(_) => {
            let current = window.url().ok();
            Err(
                BrowserTransportError::new(BrowserTransportFailureKind::NavigationTimeout)
                    .with_recharge_context(RechargeScanPhase::OverallTimeout, current.as_ref()),
            )
        }
    };
    let _ = window.close();
    result
}

/// Read a rendered document until its app shell has settled or a terminal
/// result is visible. Provider pages such as external shops commonly return a
/// tiny loading shell first and populate products/payment controls from a
/// client-side request; a single DOM read would incorrectly report no_match.
async fn scan_recharge_document(
    window: &WebviewWindow,
    initial_value: Value,
    timeout: Duration,
    phase: RechargeScanPhase,
) -> Result<RechargePageProbe, BrowserTransportError> {
    let deadline = Instant::now() + timeout;
    let mut value = initial_value;
    let mut first_read = true;
    loop {
        if first_read {
            first_read = false;
            tokio::time::sleep(RECHARGE_PAGE_SETTLE).await;
        }
        let probe = match recharge_probe_from_value(&value) {
            Ok(probe) => probe,
            Err(_shape) if Instant::now() < deadline => {
                // WebView2 can briefly report an undefined or string-wrapped
                // result while a SPA replaces its document. Treat that as a
                // transient read failure and retry inside the existing probe
                // budget instead of failing the whole collection.
                tokio::time::sleep(RECHARGE_PROBE_INTERVAL).await;
                let remaining = deadline.saturating_duration_since(Instant::now());
                value = evaluate_recharge_value(window, remaining.min(EVAL_TIMEOUT), phase)
                    .await
                    .map_err(|error| {
                        let current = window.url().ok();
                        error.with_recharge_context(phase, current.as_ref())
                    })?;
                continue;
            }
            Err(shape) => {
                return Err(BrowserTransportError::new(
                    BrowserTransportFailureKind::MalformedPayload,
                )
                .with_recharge_context(phase, window.url().ok().as_ref())
                .with_recharge_payload_shape(shape));
            }
        };
        let terminal = matches!(
            probe.status.as_str(),
            "success" | "login_required" | "not_found"
        ) || !probe.loading;
        if terminal || Instant::now() >= deadline {
            return Ok(probe);
        }
        tokio::time::sleep(RECHARGE_PROBE_INTERVAL).await;
        value = evaluate_json(window, recharge_page_scan_script(), EVAL_TIMEOUT)
            .await
            .map_err(|error| {
                let current = window.url().ok();
                error.with_recharge_context(phase, current.as_ref())
            })?;
    }
}

/// Evaluate the page probe within a bounded retry window. Navigation and SPA
/// route replacement can make one callback transiently unavailable; retries
/// are deliberately local to the probe and never extend the overall scan
/// deadline.
async fn evaluate_recharge_value(
    window: &WebviewWindow,
    timeout: Duration,
    phase: RechargeScanPhase,
) -> Result<Value, BrowserTransportError> {
    let deadline = Instant::now() + timeout;
    let mut last_error = None;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(last_error.unwrap_or_else(|| {
                BrowserTransportError::new(BrowserTransportFailureKind::MalformedPayload)
                    .with_recharge_context(phase, window.url().ok().as_ref())
            }));
        }
        let script = format!("JSON.stringify({})", recharge_page_scan_script());
        match evaluate_json(window, script, remaining.min(EVAL_TIMEOUT)).await {
            Ok(value) => return Ok(value),
            Err(error)
                if matches!(
                    error.kind,
                    BrowserTransportFailureKind::MalformedPayload
                        | BrowserTransportFailureKind::ScriptUnavailable
                ) && remaining > RECHARGE_PROBE_INTERVAL =>
            {
                last_error = Some(error);
                tokio::time::sleep(RECHARGE_PROBE_INTERVAL).await;
            }
            Err(error) => return Err(error),
        }
    }
}

fn recharge_probe_from_value(value: &Value) -> Result<RechargePageProbe, String> {
    let normalized = match value {
        Value::String(raw) => {
            // Some WebView2 versions serialize an object-valued eval result as
            // a JSON string. Decode that one extra layer, but do not accept
            // arbitrary text as a probe.
            serde_json::from_str::<Value>(raw).map_err(|_| "string".to_string())?
        }
        other => other.clone(),
    };
    let object = normalized
        .as_object()
        .ok_or_else(|| recharge_value_shape(&normalized))?;
    let text = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let entries = object
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.as_object()?;
            let url = entry.get("url")?.as_str()?.to_string();
            let label = entry
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("充值入口")
                .to_string();
            let provider = entry
                .get("provider")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            let payment_methods = entry
                .get("paymentMethods")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default();
            Some(RechargePageEntry {
                url,
                label,
                provider,
                payment_methods,
            })
        })
        .collect();
    let candidates = object
        .get("candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|candidate| {
            let candidate = candidate.as_object()?;
            Some(RechargeCandidate {
                url: candidate.get("url")?.as_str()?.to_string(),
                label: candidate
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("充值入口")
                    .to_string(),
                priority: candidate
                    .get("priority")
                    .and_then(Value::as_u64)
                    .and_then(|value| u8::try_from(value).ok())
                    .unwrap_or_else(default_recharge_candidate_priority),
            })
        })
        .collect();
    let string_array = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    Ok(RechargePageProbe {
        status: text("status"),
        current_url: text("currentUrl"),
        title: text("title"),
        provider: object
            .get("provider")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        payment_methods: string_array("paymentMethods"),
        entries,
        candidates,
        protected_candidates: string_array("protectedCandidates"),
        candidate_diagnostics: object
            .get("candidateDiagnostics")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|diagnostic| serde_json::from_value(diagnostic.clone()).ok())
            .collect(),
        evidence: string_array("evidence"),
        candidates_scanned: object
            .get("candidatesScanned")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or_default(),
        loading: object
            .get("loading")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn recharge_value_shape(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            keys.truncate(24);
            format!("object:{}", keys.join(","))
        }
        Value::Array(_) => "array".to_string(),
        Value::Null => "null".to_string(),
        Value::Bool(_) => "bool".to_string(),
        Value::Number(_) => "number".to_string(),
        Value::String(_) => "string".to_string(),
    }
}

fn normalize_recharge_candidate_url(value: &str) -> Option<String> {
    let url = tauri::Url::parse(value).ok()?;
    valid_browser_target(&url).then_some(url.to_string())
}

fn sanitize_recharge_candidate_url(value: &str) -> Option<String> {
    let sanitized = sanitize_recharge_url(value);
    (!sanitized.is_empty()).then_some(sanitized)
}

fn is_custom_recharge_wrapper_url(value: &str, target: &tauri::Url) -> bool {
    let Ok(url) = tauri::Url::parse(value) else {
        return false;
    };
    same_recharge_site(&url, target)
        && url
            .path()
            .trim_matches('/')
            .split('/')
            .next()
            .is_some_and(|segment| segment.eq_ignore_ascii_case("custom"))
}

fn has_external_recharge_candidate(
    candidates: &VecDeque<RechargeCandidate>,
    target: &tauri::Url,
) -> bool {
    candidates.iter().any(|candidate| {
        tauri::Url::parse(&candidate.url)
            .ok()
            .is_some_and(|url| valid_browser_target(&url) && !same_recharge_site(&url, target))
    })
}

fn recharge_failure_kind(kind: BrowserTransportFailureKind) -> &'static str {
    match kind {
        BrowserTransportFailureKind::InvalidWebsiteUrl => "invalid_url",
        BrowserTransportFailureKind::WindowUnavailable => "window_unavailable",
        BrowserTransportFailureKind::NavigationTimeout => "navigation_timeout",
        BrowserTransportFailureKind::ScriptUnavailable => "script_unavailable",
        BrowserTransportFailureKind::RequestTimeout => "request_timeout",
        BrowserTransportFailureKind::AuthenticationRejected => "authentication_rejected",
        BrowserTransportFailureKind::Rejected => "rejected",
        BrowserTransportFailureKind::MalformedPayload => "malformed_payload",
        BrowserTransportFailureKind::ResponseTooLarge => "response_too_large",
        BrowserTransportFailureKind::CrossOriginRedirect => "cross_origin_redirect",
    }
}

fn recharge_session_cookies(
    session_cookie: Option<&str>,
    target: &tauri::Url,
) -> Vec<Cookie<'static>> {
    let Some(header) = session_cookie else {
        return Vec::new();
    };
    let Some(host) = target.host_str() else {
        return Vec::new();
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let canonical_host = host.strip_prefix("www.").unwrap_or(&host);
    let mut cookie_domains = vec![host.clone()];
    if canonical_host != host {
        cookie_domains.push(canonical_host.to_string());
    } else if canonical_host.contains('.')
        && canonical_host.parse::<std::net::IpAddr>().is_err()
        && canonical_host != "localhost"
    {
        cookie_domains.push(format!("www.{canonical_host}"));
    }
    let secure = target.scheme() == "https";
    header
        .split(';')
        .flat_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            let name = name.trim();
            let value = value.trim();
            if name.is_empty() || value.is_empty() {
                return Some(Vec::new());
            }
            // __Host- cookies are deliberately host-only and must not carry a
            // Domain attribute. All other captured pairs are scoped to this
            // station origin instead of being shared with another site.
            let domains = if name.starts_with("__Host-") {
                vec![host.clone()]
            } else {
                cookie_domains.clone()
            };
            Some(
                domains
                    .into_iter()
                    .map(|domain| {
                        let mut builder = Cookie::build((name.to_string(), value.to_string()))
                            .path("/")
                            .secure(secure);
                        if !name.starts_with("__Host-") {
                            builder = builder.domain(domain);
                        }
                        builder.build()
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect()
}

fn recharge_page_scan_script() -> String {
    r#"(() => {
  const clean = (value, limit = 240) => String(value || '').replace(/\s+/g, ' ').trim().slice(0, limit);
  const visible = (element, limit = 12000) => {
    if (!element) return '';
    try {
      const style = window.getComputedStyle(element);
      const opacity = String(style.opacity || '').trim();
      if (style.display === 'none' || style.visibility === 'hidden' || (opacity && Number(opacity) === 0)) return '';
      if (element.getAttribute && element.getAttribute('aria-hidden') === 'true') return '';
    } catch (_) {}
    // A hidden native WebView can report zero-sized layout boxes even though
    // it has a fully rendered document. Style/ARIA checks still exclude pages
    // that intentionally hide an element without losing real sidebar links.
    return clean(element.innerText || element.textContent, limit);
  };
  const body = visible(document.body, 12000);
  const title = clean(document.title, 160);
  const currentUrl = String(window.location.href);
  const attributes = Array.from(document.querySelectorAll('[alt],[aria-label],[title]'))
    .map((element) => clean(element.getAttribute('alt') || element.getAttribute('aria-label') || element.getAttribute('title')))
    .filter(Boolean)
    .join(' ')
    .slice(0, 12000);
  const surface = `${title} ${body} ${attributes}`;
  const visibleNormalized = surface.toLowerCase();
  const rechargeLabelLike = /(充值|购买额度|余额充值|充值码|卡密|兑换码|支付|商城|商品|套餐|recharge|top[ -]?up|purchase|billing|payment|checkout|shop|store|buy(?:\s+credits|\s+now)?)/i;
  const rechargeUrlLike = /(?:充值|购买|卡密|兑换|recharge|top[\/_-]?up|purchase|billing|payment|checkout|shop|store|custom)/i;
  // Navigation labels such as “价格” or “兑换码购买” are not proof that the
  // current document is a payment page. Require an explicit payment/product
  // marker, or a price together with product context, before recording the
  // current URL as an entry.
  const pageProofLike = /(支付方式|充值金额|充值订单|付款|订单金额|购买套餐|立即购买|选择商品|商品列表|商品详情|前往充值商店|充值商店|充值(?:\/|或)?订阅|payment method|billing details|order total|checkout|buy now|add to cart|recharge(?:\s+store)?|alipay|支付宝|wechat pay|微信支付|usdt|trc20|erc20)/i;
  const priceLike = /(?:¥|￥|\$)\s*\d|(?:\d+(?:\.\d+)?)\s*(?:元|rmb|usd|刀)/i;
  const productContextLike = /(商品|套餐|余额|充值|购买|卡密|兑换码|payment|billing|checkout|shop|store|product|order)/i;
  const pageRechargeLike = pageProofLike.test(surface)
    || (priceLike.test(surface) && productContextLike.test(surface));
  const routeRechargeLike = /\/(?:purchase|recharge|top[\/_-]?up|billing|checkout|custom)(?:[/?#]|$)/i.test(currentUrl)
    && pageRechargeLike;
  const routeCustomLike = /\/custom(?:[/?#]|$)/i.test(currentUrl);
  const stationType = String(window.__relayPoolRechargeStationType || '').trim().toLowerCase();
  const normalizeHost = (value) => String(value || '').toLowerCase().replace(/^www\./, '').replace(/\.$/, '');
  const stationHost = normalizeHost(window.__relayPoolRechargeStationHost);
  const stationSite = Boolean(stationHost) && normalizeHost(window.location.hostname) === stationHost;
  // The authenticated shell can contain purchase/payment copy in its
  // navigation or announcements. It is not itself a recharge destination;
  // only an explicit payment route, a custom route, or an external shop may
  // be recorded as the current page.
  const stationShellRouteLike = stationSite
    && /^(?:\/$|\/home(?:[/?#]|$)|\/dashboard(?:[/?#]|$)|\/index\.html(?:[/?#]|$))/i.test(window.location.pathname);
  const loginRouteLike = /\/(?:login|signin|sign-in)(?:[/?#]|$)/i.test(currentUrl);
  const loginControls = Array.from(document.querySelectorAll('input[type="password"],input[name*="password" i],form[action*="login" i]'))
    .some((element) => Boolean(visible(element)));
  const loginLike = loginRouteLike || (loginControls && /(登录|登陆|验证码|密码|login|sign[ -]?in|log[ -]?in)/i.test(visibleNormalized));
  const notFoundLike = /(?:404|not found|page not found|页面不存在|找不到页面|访问出错|域名已迁移|页面已迁移|moved permanently)/i.test(visibleNormalized);
  const inlineConfigScript = Array.from(document.scripts).find((script) => {
    if (script.src) return false;
    return /^window\.__APP_CONFIG__\s*=/.test((script.textContent || '').trim());
  });
  let appConfig = window.__APP_CONFIG__ || window.__relayPoolRechargeSettings || null;
  if (!appConfig && inlineConfigScript) {
    try {
      const text = (inlineConfigScript.textContent || '').trim();
      const assignment = text.match(/^window\.__APP_CONFIG__\s*=\s*/);
      const json = assignment ? text.slice(assignment[0].length).replace(/;\s*$/, '') : '';
      appConfig = JSON.parse(json);
    } catch (_) {}
  }
  // Sub2API/NewAPI normally injects __APP_CONFIG__, but route guards may
  // fetch /settings/public after the first document paint. Kick off the same
  // same-origin request when the object is not available yet and let the
  // bounded probe loop pick up the result on a later pass. Do this only on
  // the station page, not after following an external shop candidate.
  const settingsAttempts = Number(window.__relayPoolRechargeSettingsAttempts || 0);
  if (!appConfig && stationSite && !routeCustomLike
      && !window.__relayPoolRechargeSettingsPending && settingsAttempts < 2) {
    try {
      window.__relayPoolRechargeSettingsPending = true;
      window.__relayPoolRechargeSettingsAttempts = settingsAttempts + 1;
      const settingsUrl = new URL('/api/v1/settings/public', currentUrl).toString();
      const controller = new AbortController();
      const timer = window.setTimeout(() => controller.abort(), 2500);
      window.fetch(settingsUrl, {
        credentials: 'include',
        cache: 'no-store',
        headers: { 'Accept': 'application/json', 'X-User-UI-Request': '1' },
        signal: controller.signal,
      })
        .then((response) => response.ok ? response.json() : null)
        .then((payload) => {
          const value = payload && typeof payload === 'object' && payload.data && typeof payload.data === 'object'
            ? payload.data
            : payload;
          if (value && typeof value === 'object') window.__relayPoolRechargeSettings = value;
        })
        .catch(() => {})
        .finally(() => {
          window.clearTimeout(timer);
          window.__relayPoolRechargeSettingsPending = false;
        });
    } catch (_) {
      window.__relayPoolRechargeSettingsPending = false;
    }
  }
  appConfig = appConfig || window.__relayPoolRechargeSettings || null;
  const settingsPending = window.__relayPoolRechargeSettingsPending === true;
  const paymentMatchers = [
    ['alipay', /支付宝|alipay/i],
    ['wechat', /微信支付|微信|wechat/i],
    ['bank', /银行卡|银行转账|bank(?: card| transfer)?/i],
    ['usdt', /usdt|数字货币|crypto|trc20|erc20/i],
  ];
  const paymentMethods = paymentMatchers.filter(([, matcher]) => matcher.test(surface)).map(([name]) => name);
  const providerSurface = `${visibleNormalized} ${currentUrl.toLowerCase()}`;
  const provider = /链动小铺|链动|liandong|liandongshop|(?:^|[./])ldxp\.cn(?:[/:/?#]|$)/i.test(providerSurface)
    ? 'liandong'
    : /云猫|yuncat|cloudcat|(?:^|[./])catfk\.com(?:[/:/?#]|$)/i.test(providerSurface)
      ? 'cloudcat'
      : null;
  const configuredCandidates = [];
  const configValue = (...keys) => {
    const roots = [
      appConfig,
      appConfig && appConfig.data,
      appConfig && appConfig.settings,
      appConfig && appConfig.publicSettings,
      appConfig && appConfig.public_settings,
      appConfig && appConfig.data && appConfig.data.settings,
      appConfig && appConfig.data && appConfig.data.config,
      appConfig && appConfig.config,
    ];
    for (const root of roots) {
      if (!root || typeof root !== 'object') continue;
      for (const key of keys) {
        if (Object.prototype.hasOwnProperty.call(root, key)) return root[key];
      }
    }
    return null;
  };
  const hasConfigKey = (...keys) => {
    const roots = [
      appConfig,
      appConfig && appConfig.data,
      appConfig && appConfig.settings,
      appConfig && appConfig.publicSettings,
      appConfig && appConfig.public_settings,
      appConfig && appConfig.data && appConfig.data.settings,
      appConfig && appConfig.data && appConfig.data.config,
      appConfig && appConfig.config,
    ];
    return roots.some((root) => root && typeof root === 'object'
      && keys.some((key) => Object.prototype.hasOwnProperty.call(root, key)));
  };
  const normalizeUrl = (rawValue) => {
    if (typeof rawValue !== 'string' || !rawValue.trim()) return null;
    try {
      const url = new URL(rawValue.trim(), currentUrl);
      if (!/^https?:$/i.test(url.protocol) || url.username || url.password) return null;
      // Keep ordinary SPA hash routes (for example `#/purchase`) intact.
      // Drop a fragment only when it carries the same credential material we
      // already remove from query strings before returning it to the UI.
      const sensitiveHashKeyLike = /^(?:token|access[_-]?token|refresh[_-]?token|auth(?:orization)?|auth[_-]?token|session(?:[_-]?id)?|cookie|password|secret|code)$/i;
      const hashContainsSensitiveKey = (hash) => {
        const rawHash = String(hash || '').replace(/^#/, '');
        if (!rawHash) return false;
        const query = rawHash.includes('?') ? rawHash.slice(rawHash.indexOf('?') + 1) : rawHash;
        return query.split('&').some((part) => {
          const rawKey = part.split('=', 1)[0].trim();
          if (!rawKey) return false;
          let key = rawKey;
          try { key = decodeURIComponent(rawKey.replace(/\+/g, ' ')); } catch (_) {}
          return sensitiveHashKeyLike.test(key);
        });
      };
      if (hashContainsSensitiveKey(url.hash)) url.hash = '';
      return url.toString();
    } catch (_) {
      return null;
    }
  };
  const addConfigured = (rawValue, label, priority = 20) => {
    const url = normalizeUrl(rawValue);
    if (!url) return;
    configuredCandidates.push({ url, label: clean(label) || '充值入口', priority });
  };
  if (configValue('purchase_subscription_enabled', 'purchaseSubscriptionEnabled') === true) {
    addConfigured(configValue('purchase_subscription_url', 'purchaseSubscriptionUrl'), '订阅购买', 5);
  }
  // Sub2API/NewAPI publish the authenticated purchase route even when the
  // external shop URL is not included in public settings. Only enqueue this
  // one contractual route after the rendered shell proves it is a known
  // station application. The first shell paint can arrive before sidebar or
  // recharge text, so do not make route discovery depend on that transient
  // text. The destination is still scanned for visible payment evidence and
  // a rendered 404 is rejected.
  if (stationSite && !routeCustomLike && stationShellRouteLike
      && hasConfigKey('purchase_subscription_enabled', 'purchaseSubscriptionEnabled', 'payment_enabled', 'paymentEnabled')) {
    addConfigured(new URL('/purchase', currentUrl).toString(), '订阅购买', 11);
  }
  // NewAPI exposes its authenticated recharge page at `/wallet`. Keep this
  // route type-scoped so Sub2API stations are not probed with a NewAPI path.
  // The destination still has to render payment/product evidence before it
  // can become a normalized entry.
  if (stationType === 'newapi' && stationSite && !routeCustomLike && stationShellRouteLike) {
    addConfigured(new URL('/wallet', currentUrl).toString(), '钱包充值', 11);
  }
  addConfigured(configValue('purchase_url', 'purchaseUrl'), '购买额度', 8);
  addConfigured(configValue('recharge_url', 'rechargeUrl'), '充值入口', 8);
  addConfigured(configValue('balance_low_notify_recharge_url', 'balanceLowNotifyRechargeUrl'), '低余额充值', 30);
  let menuItems = configValue('custom_menu_items', 'customMenuItems');
  if (typeof menuItems === 'string') {
    try { menuItems = JSON.parse(menuItems); } catch (_) { menuItems = []; }
  }
  if (Array.isArray(menuItems)) {
    menuItems.forEach((item) => {
      if (!item || typeof item !== 'object') return;
      const visibility = clean(item.visibility || item.visible_to || item.visibleTo || item.scope || item.audience).toLowerCase();
      if (item.visible === false || ['admin', 'administrator', 'system', 'hidden', 'private', 'false', '0'].includes(visibility)) return;
      const label = clean(item.label || item.label_zh || item.labelZh || item.name || item.displayName || item.display_name || item.title || item.caption || item.text);
      const rawValue = item.url || item.href || item.link || item.path || item.route || item.target || item.external_url || item.externalUrl;
      if (!rechargeLabelLike.test(`${label} ${rawValue || ''}`)
          && !rechargeUrlLike.test(String(rawValue || ''))) return;
      // Sub2API/NewAPI custom menu items are rendered through an authenticated
      // `/custom/{id}` route before they load the external shop in an iframe.
      // Derive that route only from the published menu id; it is not a guessed
      // URL and keeps the station login/session in the navigation chain.
      const itemId = item.id || item.key || item.menu_id || item.menuId;
      if (typeof itemId === 'string' && itemId.trim()) {
        addConfigured(`/custom/${encodeURIComponent(itemId.trim())}`, label || '自定义充值', 9);
      }
      addConfigured(rawValue, label || '自定义充值', 10);
    });
  }
  const inlineNavigationTarget = (element) => {
    const handler = element.getAttribute && element.getAttribute('onclick');
    if (!handler) return '';
    const matched = handler.match(/(?:(?:window\.)?location(?:\.(?:href|assign|replace))?|window\.open)\s*(?:=|\()\s*(['"])(https?:\/\/[^'"\s]+|\/[^'"\s]+)\1/i);
    return matched ? matched[2] : '';
  };
  const meaningfulTarget = (...values) => values
    .map((value) => typeof value === 'string' ? value.trim() : '')
    .find((value) => value && !/^(?:#|javascript:|about:blank$|void\s*\(\s*0\s*\)\s*;?$)/i.test(value)) || '';
  const elementTarget = (element) => {
    const parentAnchor = element.closest && element.closest('a[href]');
    return meaningfulTarget(
      element.getAttribute('href'),
      element.getAttribute('data-href'),
      element.getAttribute('data-url'),
      element.getAttribute('data-link'),
      element.getAttribute('data-to'),
      element.getAttribute('data-route'),
      element.getAttribute('data-path'),
      element.getAttribute('data-target'),
      element.getAttribute('data-lazy-href'),
      element.getAttribute('data-recharge-url'),
      element.getAttribute('data-payment-url'),
      element.getAttribute('data-shop-url'),
      element.getAttribute('to'),
      element.getAttribute('formaction'),
      parentAnchor && parentAnchor.getAttribute('href'),
      inlineNavigationTarget(element),
    );
  };
  const anchors = Array.from(document.querySelectorAll('a[href],[role="link"],button,[role="button"],[data-href],[data-url],[data-link],[data-to],[data-route],[data-path],[data-target],[data-lazy-href],[data-recharge-url],[data-payment-url],[data-shop-url],[to],[onclick]'))
    .map((element) => {
      const rawHref = elementTarget(element);
      const label = clean(visible(element, 480) || element.getAttribute('aria-label') || element.getAttribute('title'));
      if (!rawHref || !rechargeLabelLike.test(`${label} ${rawHref}`)
          && !rechargeUrlLike.test(rawHref)) return null;
      const url = normalizeUrl(rawHref);
      if (!url) return null;
      return { url, label: label || '充值入口', priority: 20 };
    })
    .filter(Boolean);
  // Some providers route a sidebar item to /custom/<id> and render the actual
  // shop in an iframe. The iframe source is the concrete navigation target;
  // it is still only returned after the external document proves it is a
  // purchase/product page.
  const embeddedCandidates = Array.from(document.querySelectorAll('iframe,frame'))
    .map((frame) => {
      const rawSrc = meaningfulTarget(
        frame.getAttribute('src'),
        frame.getAttribute('data-src'),
        frame.getAttribute('data-url'),
        frame.getAttribute('data-href'),
        frame.getAttribute('data-link'),
        frame.getAttribute('data-lazy-src'),
      );
      const frameOwner = frame.closest && frame.closest('a,button,[role="link"],[role="button"]');
      const surrounding = clean(`${frame.getAttribute('title') || ''} ${frame.getAttribute('aria-label') || ''} ${frameOwner ? visible(frameOwner, 720) : ''}`);
      const embeddedUrlLike = /\/(?:purchase|recharge|top[\/_-]?up|billing|checkout|shop)(?:[/?#]|$)/i.test(rawSrc);
      // Cloudcat exposes a technical anti-blacklist iframe from this path.
      // It is not a product page, even when the surrounding custom route is
      // otherwise eligible for embedded-shop discovery.
      const technicalIframeLike = /(?:^|[/?#])buyerBlackIframe(?:[/?#]|$)/i.test(rawSrc);
      if (!rawSrc || technicalIframeLike || (!routeCustomLike && !rechargeLabelLike.test(surrounding)
          && !embeddedUrlLike)) return null;
      const url = normalizeUrl(rawSrc);
      if (!url) return null;
      return { url, label: clean(frame.getAttribute('title') || title || '自定义充值'), priority: 12 };
    })
    .filter(Boolean);
  const entries = [];
  const seen = new Set();
  const candidateMap = new Map();
  const addCandidate = (candidate) => {
    if (!candidate || !candidate.url) return;
    const existing = candidateMap.get(candidate.url);
    if (!existing || Number(candidate.priority || 99) < Number(existing.priority || 99)) {
      candidateMap.set(candidate.url, {
        url: candidate.url,
        label: clean(candidate.label) || '充值入口',
        priority: Number(candidate.priority || 99),
      });
    }
  };
  const add = (url, label) => {
    if (!url || seen.has(url)) return;
    seen.add(url);
    entries.push({ url, label: clean(label) || '充值入口', provider, paymentMethods: [...paymentMethods] });
  };
  configuredCandidates.forEach(addCandidate);
  anchors.forEach(addCandidate);
  embeddedCandidates.forEach(addCandidate);
  const candidates = Array.from(candidateMap.values())
    .sort((left, right) => left.priority - right.priority || left.url.localeCompare(right.url))
    .slice(0, 12)
    .map(({ url, label, priority }) => ({ url, label, priority }));
  const routeSettled = window.__relayPoolRechargeRouteSettled === true;
  const stationRoutePending = stationSite && !routeSettled && !loginLike && !notFoundLike;
  // A configured/sidebar/iframe target is useful only after the station SPA
  // has made its first route decision. Without this guard, public inline
  // config on /custom/<id> can be followed before the app redirects to login.
  const hasCandidate = candidates.length > 0;
  const baseLoading = Array.from(document.querySelectorAll('.page-loading,.loading,[aria-busy="true"]'))
    .some((element) => Boolean(visible(element)))
    || (body.length < 120 && /(加载中|正在加载|loading|please wait)/i.test(visibleNormalized))
    || stationRoutePending
    || (stationSite && !routeCustomLike && settingsPending && !hasCandidate)
    // External shops often replace an empty document after their first
    // client-side request. Wait for that truly empty shell, but do not make a
    // short, already-rendered product page depend on station app config.
    || (body.length < 24 && !stationSite && !routeCustomLike && !loginLike && !notFoundLike && !appConfig && !hasCandidate)
    || (stationSite && !routeCustomLike && !appConfig && body.length < 160 && !loginLike && !notFoundLike && !hasCandidate);
  // A dashboard often has a "充值" navigation item. That is only a
  // candidate, not proof that the current document is the recharge page.
  // Require payment/product/order evidence before recording the current URL.
  if ((pageRechargeLike || routeRechargeLike)
      && !stationShellRouteLike
      && !loginLike && !notFoundLike && !baseLoading) add(currentUrl, title || '充值页面');
  // Custom page shells can report `complete` before their embedded shop is
  // attached. Give that specific, evidence-free state the regular bounded
  // probe window instead of declaring a false negative on the first paint.
  const loading = baseLoading || (routeCustomLike && !loginLike && !notFoundLike
    && !pageRechargeLike && candidates.length === 0);
  const status = loginLike ? 'login_required' : notFoundLike ? 'not_found' : entries.length ? 'success' : 'no_match';
  const evidence = [];
  if (rechargeLabelLike.test(surface)) evidence.push('visible_recharge_text');
  if (pageRechargeLike) evidence.push('visible_payment_or_product');
  if (anchors.length) evidence.push('rendered_recharge_link');
  if (embeddedCandidates.length) evidence.push('embedded_recharge_link');
  if (configuredCandidates.length) evidence.push('configured_recharge_link');
  if (paymentMethods.length) evidence.push('visible_payment_method');
  if (provider) evidence.push(`provider:${provider}`);
  return {
    status,
    currentUrl,
    title,
    provider,
    paymentMethods,
    entries,
    candidates,
    protectedCandidates: [],
    evidence,
    loading,
    documentId: window.__relayPoolRechargeDocumentId || null,
    timeOrigin: window.performance && window.performance.timeOrigin ? String(window.performance.timeOrigin) : null,
    routeVersion: window.__relayPoolRechargeRouteVersion || null,
  };
})()"#.to_string()
}

fn sanitize_recharge_url(value: &str) -> String {
    let Ok(mut url) = tauri::Url::parse(value) else {
        return String::new();
    };
    let has_sensitive_query = url
        .query_pairs()
        .any(|(key, _)| is_sensitive_recharge_key(&key));
    if has_sensitive_query {
        url.set_query(None);
    }
    if url
        .fragment()
        .is_some_and(recharge_fragment_has_sensitive_key)
    {
        url.set_fragment(None);
    }
    url.to_string()
}

fn is_sensitive_recharge_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "token"
            | "access_token"
            | "accesstoken"
            | "refresh_token"
            | "refreshtoken"
            | "auth"
            | "authorization"
            | "auth_token"
            | "session"
            | "session_id"
            | "sessionid"
            | "cookie"
            | "password"
            | "secret"
            | "code"
    )
}

fn recharge_fragment_has_sensitive_key(fragment: &str) -> bool {
    let raw = fragment.strip_prefix('?').unwrap_or(fragment);
    let query = raw.split_once('?').map(|(_, query)| query).unwrap_or(raw);
    query.split('&').any(|part| {
        let key = url::form_urlencoded::parse(part.as_bytes())
            .next()
            .map(|(key, _)| key.into_owned())
            .unwrap_or_default();
        !key.is_empty() && is_sensitive_recharge_key(&key)
    })
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

async fn wait_for_recharge_document(
    window: &WebviewWindow,
    target: &tauri::Url,
    timeout: Duration,
    phase: RechargeScanPhase,
    previous: Option<&RechargeDocumentState>,
    allow_external_candidate: bool,
) -> Result<(), BrowserTransportError> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let current = window.url().ok();
        match current.as_ref() {
            Some(url) if url.scheme() == "about" => {}
            Some(url)
                if same_recharge_site(url, target)
                    || (allow_external_candidate && valid_browser_target(url)) =>
            {
                let readiness = evaluate_json(
                    window,
                    "({ readyState: document.readyState, href: String(window.location.href), documentId: window.__relayPoolRechargeDocumentId || null, timeOrigin: window.performance && window.performance.timeOrigin ? String(window.performance.timeOrigin) : null, routeVersion: window.__relayPoolRechargeRouteVersion || null })",
                    EVAL_TIMEOUT,
                )
                .await;
                match readiness {
                    Ok(value) => {
                        let ready = matches!(
                            value.get("readyState").and_then(Value::as_str),
                            Some("interactive" | "complete")
                        );
                        let state =
                            recharge_document_state_from_value(&value).unwrap_or_else(|| {
                                RechargeDocumentState {
                                    url: url.to_string(),
                                    document_id: None,
                                    time_origin: None,
                                    route_version: None,
                                }
                            });
                        if ready
                            && previous
                                .is_none_or(|previous| recharge_document_changed(previous, &state))
                        {
                            return Ok(());
                        }
                    }
                    Err(error) => {
                        return Err(error.with_recharge_context(phase, current.as_ref()));
                    }
                }
            }
            Some(url) => {
                return Err(BrowserTransportError::new(
                    BrowserTransportFailureKind::CrossOriginRedirect,
                )
                .with_recharge_context(phase, Some(url)));
            }
            None => {}
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    let current = window.url().ok();
    Err(
        BrowserTransportError::new(BrowserTransportFailureKind::NavigationTimeout)
            .with_recharge_context(phase, current.as_ref()),
    )
}

fn recharge_document_state_from_value(value: &Value) -> Option<RechargeDocumentState> {
    let url = value
        .get("currentUrl")
        .or_else(|| value.get("href"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())?
        .to_string();
    let optional_string = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.is_empty())
    };
    Some(RechargeDocumentState {
        url,
        document_id: optional_string("documentId"),
        time_origin: optional_string("timeOrigin"),
        route_version: optional_string("routeVersion"),
    })
}

fn recharge_document_state_fallback(window: &WebviewWindow) -> Option<RechargeDocumentState> {
    Some(RechargeDocumentState {
        url: window.url().ok()?.to_string(),
        document_id: None,
        time_origin: None,
        route_version: None,
    })
}

fn recharge_document_changed(
    previous: &RechargeDocumentState,
    current: &RechargeDocumentState,
) -> bool {
    // The initialization script gives each document and client-side route a
    // generation. Prefer those generations over URL changes because a browser
    // can update location before the old document has actually been unloaded.
    let mut has_generation = false;
    let mut incomplete_generation = false;
    if let (Some(previous), Some(current)) =
        (previous.document_id.as_ref(), current.document_id.as_ref())
    {
        has_generation = true;
        if previous != current {
            return true;
        }
    } else if previous.document_id.is_some() != current.document_id.is_some() {
        incomplete_generation = true;
    }
    if let (Some(previous), Some(current)) =
        (previous.time_origin.as_ref(), current.time_origin.as_ref())
    {
        has_generation = true;
        if previous != current {
            return true;
        }
    } else if previous.time_origin.is_some() != current.time_origin.is_some() {
        incomplete_generation = true;
    }
    if let (Some(previous), Some(current)) = (
        previous.route_version.as_ref(),
        current.route_version.as_ref(),
    ) {
        has_generation = true;
        if previous != current {
            return true;
        }
    } else if previous.route_version.is_some() != current.route_version.is_some() {
        incomplete_generation = true;
    }
    if has_generation && !incomplete_generation {
        return false;
    }
    // A partial generation is not enough to prove that navigation stayed in
    // the same document. Fall back to the URL so candidate navigation cannot
    // wait until timeout merely because one optional marker was unavailable.
    previous.url != current.url
}

fn recharge_initialization_script(target: &tauri::Url, session: RechargeSession<'_>) -> String {
    let host = target
        .host_str()
        .map(|value| {
            value
                .trim_end_matches('.')
                .trim_start_matches("www.")
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    let host = serde_json::to_string(&host).unwrap_or_else(|_| "\"\"".to_string());
    let scheme = serde_json::to_string(&format!("{}:", target.scheme()))
        .unwrap_or_else(|_| "\"https:\"".to_string());
    let port = target
        .port_or_known_default()
        .map(|value| value.to_string())
        .unwrap_or_default();
    let port = serde_json::to_string(&port).unwrap_or_else(|_| "\"\"".to_string());
    let route_settle_ms = RECHARGE_ROUTE_SETTLE.as_millis();
    let access_token =
        serde_json::to_string(&session.access_token).unwrap_or_else(|_| "null".to_string());
    let refresh_token =
        serde_json::to_string(&session.refresh_token).unwrap_or_else(|_| "null".to_string());
    let user_id =
        serde_json::to_string(&session.newapi_user_id).unwrap_or_else(|_| "null".to_string());

    format!(
        r#"(() => {{
  try {{
    const random = globalThis.crypto && typeof globalThis.crypto.randomUUID === 'function'
      ? globalThis.crypto.randomUUID()
      : `${{Date.now()}}-${{Math.random()}}`;
    Object.defineProperty(window, '__relayPoolRechargeDocumentId', {{
      configurable: true,
      value: random,
    }});
  }} catch (_) {{}}
  try {{
    const routeVersion = `${{Date.now()}}-${{Math.random()}}`;
    Object.defineProperty(window, '__relayPoolRechargeRouteVersion', {{ configurable: true, writable: true, value: routeVersion }});
    const markRoutePending = () => {{
      window.__relayPoolRechargeRouteSettled = false;
      if (window.__relayPoolRechargeRouteSettleTimer) window.clearTimeout(window.__relayPoolRechargeRouteSettleTimer);
      window.__relayPoolRechargeRouteSettleTimer = window.setTimeout(() => {{
        window.__relayPoolRechargeRouteSettled = true;
      }}, {route_settle_ms});
    }};
    const markRouteChanged = () => {{
      window.__relayPoolRechargeRouteVersion = `${{routeVersion}}-${{Date.now()}}-${{Math.random()}}`;
      markRoutePending();
    }};
    markRoutePending();
    for (const method of ['pushState', 'replaceState']) {{
      const original = window.history[method];
      if (typeof original !== 'function') continue;
      window.history[method] = function() {{
        const result = original.apply(this, arguments);
        markRouteChanged();
        return result;
      }};
    }}
    window.addEventListener('popstate', markRouteChanged);
    window.addEventListener('hashchange', markRouteChanged);
  }} catch (_) {{}}
  const targetHost = {host};
  const targetScheme = {scheme};
  const targetPort = {port};
  const userId = {user_id};
  try {{
    Object.defineProperty(window, '__relayPoolRechargeStationHost', {{
      configurable: true,
      value: targetHost,
    }});
  }} catch (_) {{}}
  const normalizeHost = (value) => String(value || '').toLowerCase().replace(/^www\./, '').replace(/\.$/, '');
  const effectivePort = (location) => location.port || (location.protocol === 'https:' ? '443' : '80');
  const sameSite = normalizeHost(location.hostname) === targetHost;
  const sameScheme = location.protocol === targetScheme && effectivePort(location) === targetPort;
  const secureUpgrade = targetScheme === 'http:'
    && targetPort === '80'
    && location.protocol === 'https:'
    && effectivePort(location) === '443';
  if (!sameSite || (!sameScheme && !secureUpgrade)) return;
  const accessToken = {access_token};
  const refreshToken = {refresh_token};
  const write = (storage, key, value) => {{
    if (!value || !storage) return;
    try {{ storage.setItem(key, value); }} catch (_) {{}}
  }};
  if (typeof accessToken === 'string' && accessToken.trim()) {{
    for (const storage of [window.localStorage, window.sessionStorage]) {{
      for (const key of ['auth_token', 'access_token', 'accessToken', 'token']) write(storage, key, accessToken);
      if (typeof userId === 'string' && userId.trim()) {{
        write(storage, 'auth_user', JSON.stringify({{ id: userId }}));
      }}
    }}
  }}
  if (typeof refreshToken === 'string' && refreshToken.trim()) {{
    for (const storage of [window.localStorage, window.sessionStorage]) {{
      for (const key of ['refresh_token', 'refreshToken']) write(storage, key, refreshToken);
    }}
  }}
}})()"#
    )
}

fn recharge_initialization_script_for_station_type(
    target: &tauri::Url,
    session: RechargeSession<'_>,
    station_type: &str,
) -> String {
    let station_type =
        serde_json::to_string(station_type.trim()).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        "{};try {{ Object.defineProperty(window, '__relayPoolRechargeStationType', {{ configurable: true, value: {} }}); }} catch (_) {{}}",
        recharge_initialization_script(target, session),
        station_type
    )
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
    serde_json::from_str(&raw).map_err(|_| {
        BrowserTransportError::new(BrowserTransportFailureKind::MalformedPayload)
            .with_recharge_payload_shape(callback_payload_shape(&raw))
    })
}

fn callback_payload_shape(raw: &str) -> String {
    let trimmed = raw.trim();
    let kind = if trimmed.is_empty() {
        "empty"
    } else if trimmed.starts_with("undefined") {
        "undefined"
    } else if trimmed.starts_with("<") {
        "html"
    } else if trimmed.starts_with('{') {
        "object_like"
    } else if trimmed.starts_with('[') {
        "array_like"
    } else if trimmed.starts_with('"') {
        "string_like"
    } else {
        "other"
    };
    format!("callback:{kind}:{}bytes", trimmed.len())
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

fn same_recharge_site(left: &tauri::Url, right: &tauri::Url) -> bool {
    fn normalized_host(url: &tauri::Url) -> Option<String> {
        url.host_str().map(|host| {
            host.trim_end_matches('.')
                .trim_start_matches("www.")
                .to_ascii_lowercase()
        })
    }

    let same_scheme = left.scheme() == right.scheme();
    // `left` is the current browser URL and `right` is the requested target
    // in the readiness check below. A current HTTPS page may be the result of
    // an HTTP target upgrading; the reverse is a downgrade and is rejected.
    let safe_upgrade = left.scheme() == "https" && right.scheme() == "http";
    let compatible_ports = if same_scheme {
        left.port_or_known_default() == right.port_or_known_default()
    } else if safe_upgrade {
        // Many station and shop links start on HTTP and immediately upgrade
        // to HTTPS. Accept only the two default ports for that transition;
        // never follow a downgrade or a cross-port redirect.
        left.port_or_known_default() == Some(443) && right.port_or_known_default() == Some(80)
    } else {
        false
    };
    compatible_ports && normalized_host(left) == normalized_host(right)
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
    fn recharge_script_uses_visible_dom_and_does_not_inspect_source_bundles() {
        let script = recharge_page_scan_script();

        assert!(script.contains("document.body"));
        assert!(script.contains("innerText"));
        assert!(script.contains("candidates"));
        assert!(script.contains("pageRechargeLike"));
        assert!(script.contains("window.__APP_CONFIG__"));
        assert!(script.contains("window\\.__APP_CONFIG__\\s*="));
        assert!(script.contains("JSON.parse(json)"));
        assert!(script.contains("purchase_subscription_url"));
        assert!(script.contains("new URL('/purchase', currentUrl).toString()"));
        assert!(script.contains("hasConfigKey('purchase_subscription_enabled'"));
        assert!(script.contains("stationSite && !routeCustomLike && stationShellRouteLike"));
        assert!(script.contains("stationType === 'newapi'"));
        assert!(script.contains("new URL('/wallet', currentUrl).toString()"));
        assert!(script.contains("custom_menu_items"));
        assert!(script.contains("/custom/${encodeURIComponent(itemId.trim())}"));
        assert!(script.contains("balance_low_notify_recharge_url"));
        assert!(script.contains("data-url"));
        assert!(script.contains("data-route"));
        assert!(script.contains("data-recharge-url"));
        assert!(script.contains("data-payment-url"));
        assert!(script.contains("querySelectorAll('iframe,frame')"));
        assert!(script.contains("data-lazy-src"));
        assert!(script.contains("buyerBlackIframe"));
        assert!(script.contains("sensitiveHashKeyLike"));
        assert!(script.contains("configured_recharge_link"));
        assert!(script.contains("embedded_recharge_link"));
        assert!(script.contains("protectedCandidates"));
        assert!(script.contains("__relayPoolRechargeSettingsAttempts"));
        assert!(script.contains("pageProofLike"));
        assert!(script.contains("前往充值商店"));
        assert!(script.contains("充值(?:\\/|或)?订阅"));
        assert!(script.contains("meaningfulTarget"));
        assert!(!script.contains("rechargeLike.test(visibleNormalized) && !loginLike"));
        assert!(!script.contains("document.documentElement.innerHTML"));
    }

    #[test]
    fn recharge_script_initializes_config_before_loading_state() {
        let script = recharge_page_scan_script();
        let config = script
            .find("let appConfig =")
            .expect("config initialization is present");
        let candidates = script
            .find("const hasCandidate = candidates.length > 0")
            .expect("candidate readiness is present");
        let base_loading = script
            .find("const baseLoading =")
            .expect("base loading state is present");
        let loading = script
            .find("const loading =")
            .expect("loading state is present");

        assert!(config < loading);
        assert!(candidates < base_loading);
        assert!(base_loading < loading);
        assert!(script.contains("settingsPending && !hasCandidate"));
        assert!(script.contains("!appConfig && !hasCandidate"));
        assert!(script.contains("stationRoutePending"));
    }

    #[test]
    fn recharge_script_declares_station_site_before_shell_route_check() {
        let script = recharge_page_scan_script();
        let station_site = script
            .find("const stationSite =")
            .expect("station site detection is present");
        let shell_route = script
            .find("const stationShellRouteLike = stationSite")
            .expect("shell route detection is present");

        assert!(station_site < shell_route);
    }

    #[test]
    fn recharge_probe_accepts_string_wrapped_json_and_defaults_optional_fields() {
        let wrapped = Value::String(
            serde_json::json!({
                "status": "no_match",
                "currentUrl": "https://example.test/home",
                "entries": [],
                "loading": false,
            })
            .to_string(),
        );
        let probe = recharge_probe_from_value(&wrapped).expect("wrapped probe should parse");
        assert_eq!(probe.status, "no_match");
        assert_eq!(probe.current_url, "https://example.test/home");
        assert!(probe.entries.is_empty());
        assert!(!probe.loading);
    }

    #[test]
    fn recharge_candidates_accept_explicit_external_http_links_but_reject_unsafe_urls() {
        assert_eq!(
            normalize_recharge_candidate_url("https://catfk.com/shop/pikaqiu#checkout"),
            Some("https://catfk.com/shop/pikaqiu#checkout".to_string())
        );
        assert_eq!(
            normalize_recharge_candidate_url("https://example.test/#/purchase"),
            Some("https://example.test/#/purchase".to_string())
        );
        assert_eq!(
            normalize_recharge_candidate_url("https://example.test/#/purchase?token=fixture"),
            Some("https://example.test/#/purchase?token=fixture".to_string())
        );
        assert_eq!(
            sanitize_recharge_candidate_url("https://example.test/#/purchase?token=fixture"),
            Some("https://example.test/".to_string())
        );
        assert!(normalize_recharge_candidate_url("javascript:alert(1)").is_none());
        assert!(normalize_recharge_candidate_url("https://user:pass@example.test/pay").is_none());
    }

    #[test]
    fn custom_wrapper_is_deferred_only_when_an_external_recharge_candidate_exists() {
        let station = tauri::Url::parse("https://relay.example/").unwrap();
        let custom_url = "https://relay.example/custom/25adabf01283c4a2";
        let purchase_url = "https://relay.example/purchase";
        let external_shop = RechargeCandidate {
            url: "https://catfk.com/shop/jianshang".to_string(),
            label: "兑换码购买".to_string(),
            priority: 12,
        };

        assert!(is_custom_recharge_wrapper_url(custom_url, &station));
        assert!(!is_custom_recharge_wrapper_url(purchase_url, &station));
        assert!(has_external_recharge_candidate(
            &VecDeque::from([external_shop.clone()]),
            &station,
        ));
        assert!(!has_external_recharge_candidate(
            &VecDeque::from([RechargeCandidate {
                url: purchase_url.to_string(),
                ..external_shop
            }]),
            &station,
        ));
    }

    #[test]
    fn recharge_result_urls_drop_sensitive_query_material() {
        assert_eq!(
            sanitize_recharge_url("https://example.test/purchase?token=fixture&amount=10#pay"),
            "https://example.test/purchase#pay"
        );
        assert_eq!(
            sanitize_recharge_url("https://example.test/purchase?amount=10#pay"),
            "https://example.test/purchase?amount=10#pay"
        );
        assert_eq!(
            sanitize_recharge_url("https://example.test/purchase?accessToken=fixture&amount=10"),
            "https://example.test/purchase"
        );
        assert_eq!(
            sanitize_recharge_url("https://example.test/#/purchase"),
            "https://example.test/#/purchase"
        );
        assert_eq!(
            sanitize_recharge_url("https://example.test/#/purchase?session=fixture"),
            "https://example.test/"
        );
    }

    #[test]
    fn recharge_session_cookie_header_is_scoped_to_station_origin() {
        let target = tauri::Url::parse("https://www.example.test/home").unwrap();
        let cookies = recharge_session_cookies(
            Some("session=fixture-session; __Host-auth=fixture-auth; ignored"),
            &target,
        );

        assert_eq!(cookies.len(), 3);
        assert_eq!(cookies[0].name(), "session");
        assert_eq!(cookies[0].domain(), Some("www.example.test"));
        assert_eq!(cookies[0].path(), Some("/"));
        assert_eq!(cookies[0].http_only(), None);
        assert_eq!(cookies[1].name(), "session");
        assert_eq!(cookies[1].domain(), Some("example.test"));
        assert_eq!(cookies[2].name(), "__Host-auth");
        assert_eq!(cookies[2].domain(), None);
        assert_eq!(cookies[2].secure(), Some(true));
    }

    #[test]
    fn recharge_session_cookie_header_covers_apex_to_www_redirects() {
        let target = tauri::Url::parse("https://example.test/home").unwrap();
        let cookies = recharge_session_cookies(Some("session=fixture-session"), &target);

        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].domain(), Some("example.test"));
        assert_eq!(cookies[1].domain(), Some("www.example.test"));
    }

    #[test]
    fn recharge_document_wait_requires_a_new_document_identity() {
        let previous = RechargeDocumentState {
            url: "https://example.test/home".to_string(),
            document_id: Some("document-1".to_string()),
            time_origin: Some("1".to_string()),
            route_version: Some("route-1".to_string()),
        };
        let same_document = RechargeDocumentState {
            url: "https://example.test/purchase".to_string(),
            document_id: Some("document-1".to_string()),
            time_origin: Some("1".to_string()),
            route_version: Some("route-1".to_string()),
        };
        let new_document = RechargeDocumentState {
            url: "https://example.test/login?redirect=/purchase".to_string(),
            document_id: Some("document-2".to_string()),
            time_origin: Some("2".to_string()),
            route_version: Some("route-2".to_string()),
        };

        assert!(!recharge_document_changed(&previous, &same_document));
        assert!(recharge_document_changed(&previous, &new_document));

        let spa_route = RechargeDocumentState {
            route_version: Some("route-2".to_string()),
            ..same_document
        };
        assert!(recharge_document_changed(&previous, &spa_route));

        let partial_generation = RechargeDocumentState {
            url: "https://example.test/login?redirect=/purchase".to_string(),
            document_id: None,
            time_origin: Some("1".to_string()),
            route_version: Some("route-1".to_string()),
        };
        assert!(recharge_document_changed(&previous, &partial_generation));
    }

    #[test]
    fn recharge_initialization_script_scopes_session_storage_to_station_site() {
        let target = tauri::Url::parse("https://www.example.test/home").unwrap();
        let script = recharge_initialization_script(
            &target,
            RechargeSession {
                cookie: Some("session=fixture-cookie"),
                access_token: Some("fixture-access-token"),
                refresh_token: Some("fixture-refresh-token"),
                newapi_user_id: Some("42"),
            },
        );

        assert!(script.contains("__relayPoolRechargeDocumentId"));
        assert!(script.contains("auth_token"));
        assert!(script.contains("refresh_token"));
        assert!(script.contains("auth_user"));
        assert!(script.contains("42"));
        assert!(script.contains("targetHost"));
        assert!(script.contains("__relayPoolRechargeStationHost"));
        assert!(script.contains("example.test"));
        assert!(script.contains("secureUpgrade"));
        assert!(script.contains("sameSite"));
        assert!(script.contains("hashchange"));
        assert!(script.contains("__relayPoolRechargeRouteSettled"));
        assert!(!script.contains("fixture-cookie"));
        assert!(script.contains("fixture-access-token"));
    }

    #[test]
    fn recharge_initialization_script_carries_station_type_without_credentials() {
        let target = tauri::Url::parse("https://example.test/home").unwrap();
        let script = recharge_initialization_script_for_station_type(
            &target,
            RechargeSession {
                access_token: Some("fixture-access-token"),
                ..RechargeSession::default()
            },
            "newapi",
        );

        assert!(script.contains("__relayPoolRechargeStationType"));
        assert!(script.contains("newapi"));
        assert!(script.contains("fixture-access-token"));
    }

    #[test]
    fn recharge_site_allows_www_alias_but_not_cross_site_or_port_change() {
        let apex = tauri::Url::parse("https://example.test/purchase").unwrap();
        let www = tauri::Url::parse("https://www.example.test/login").unwrap();
        let other = tauri::Url::parse("https://payments.example.test/purchase").unwrap();
        let other_port = tauri::Url::parse("https://www.example.test:8443/purchase").unwrap();
        let current_https = tauri::Url::parse("https://example.test/purchase").unwrap();
        let requested_http = tauri::Url::parse("http://example.test/purchase").unwrap();
        let current_http = tauri::Url::parse("http://example.test/purchase").unwrap();
        let requested_https = tauri::Url::parse("https://example.test/purchase").unwrap();

        assert!(same_recharge_site(&apex, &www));
        assert!(!same_recharge_site(&apex, &other));
        assert!(!same_recharge_site(&www, &other_port));
        assert!(same_recharge_site(&current_https, &requested_http));
        assert!(!same_recharge_site(&current_http, &requested_https));
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
