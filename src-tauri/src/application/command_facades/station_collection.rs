use std::{future::Future, sync::Arc};

use crate::{
    application::{
        collectors::{CaptureSnapshotRequest, CollectorService},
        credentials::CredentialService,
        error::ApplicationError,
        settings::SettingsService,
    },
    background_tasks::{BlockingExecutor, BlockingExecutorError},
    models::station_redemption::StationRedemptionResult,
    models::{
        collector::{CollectorEvent, CollectorRunResult},
        credentials::{PersistStationSessionInput, ResolvedSession, SessionResolveStatus},
    },
    observability::correlation,
    outbound::{AsyncOutboundClient, RequestBudget},
    services::{
        collectors::{self, output::CollectorTask, V2CollectorSourceAdapter},
        remote_keys::RemoteKeyOperationError,
        station_collection_coordinator::{
            StationCollectionAdmissionError, StationCollectionCoordinator,
        },
        station_collection_feedback::StationCollectionFeedback,
    },
};

use super::remote_keys::RemoteKeysCommandFacade;

const REMOTE_KEY_REFRESH_EVENT: &str = "remote_keys";

#[derive(Debug)]
pub(crate) enum StationCollectionCommandError {
    Admission(StationCollectionAdmissionError),
    Scheduled,
    Prepare(ApplicationError),
    Apply(ApplicationError),
    Blocking(BlockingExecutorError),
}

#[derive(Debug)]
pub(crate) struct RechargeScanRequest {
    pub(crate) website_url: String,
    pub(crate) station_type: String,
    pub(crate) session_usable: bool,
    pub(crate) cookie: Option<String>,
    pub(crate) access_token: Option<String>,
    pub(crate) refresh_token: Option<String>,
    pub(crate) newapi_user_id: Option<String>,
}

#[derive(Debug)]
pub(crate) struct RechargeScanCapture {
    pub(crate) status: String,
    pub(crate) summary_json: serde_json::Value,
    pub(crate) normalized_json: serde_json::Value,
    pub(crate) error_message: Option<String>,
    pub(crate) event_count: i64,
}

#[derive(Clone)]
pub(crate) struct StationCollectionCommandFacade {
    collectors: Arc<CollectorService>,
    credentials: Arc<CredentialService>,
    settings: Arc<SettingsService>,
    blocking: BlockingExecutor,
    outbound: AsyncOutboundClient,
    providers: Arc<collectors::orchestration::ProviderRegistry>,
    remote_keys: RemoteKeysCommandFacade,
    station_collection_coordinator: StationCollectionCoordinator,
    station_collection_feedback: StationCollectionFeedback,
}

impl StationCollectionCommandFacade {
    pub(crate) fn new(
        collectors: Arc<CollectorService>,
        credentials: Arc<CredentialService>,
        settings: Arc<SettingsService>,
        blocking: BlockingExecutor,
        outbound: AsyncOutboundClient,
        providers: Arc<collectors::orchestration::ProviderRegistry>,
        station_collection_coordinator: StationCollectionCoordinator,
        station_collection_feedback: StationCollectionFeedback,
    ) -> Self {
        let remote_keys = RemoteKeysCommandFacade::new(
            Arc::clone(&collectors),
            Arc::clone(&credentials),
            Arc::clone(&settings),
            blocking.clone(),
            outbound.clone(),
            Arc::clone(&providers),
        );
        Self {
            collectors,
            credentials,
            settings,
            blocking,
            outbound,
            providers,
            remote_keys,
            station_collection_coordinator,
            station_collection_feedback,
        }
    }

    pub(crate) async fn run_station_collection(
        &self,
        station_id: String,
        task: CollectorTask,
    ) -> Result<CollectorRunResult, StationCollectionCommandError> {
        if let Some(result) = self
            .station_collection_feedback
            .wait_for_scheduled_result(&station_id)
            .await
        {
            return result.map_err(|_| StationCollectionCommandError::Scheduled);
        }
        let station_id_for_lease = station_id.clone();
        let _lease = match self
            .station_collection_coordinator
            .try_acquire(&station_id_for_lease)
        {
            Ok(lease) => lease,
            Err(StationCollectionAdmissionError::AlreadyRunning) => {
                if let Some(result) = self
                    .station_collection_feedback
                    .wait_for_scheduled_result(&station_id_for_lease)
                    .await
                {
                    return result.map_err(|_| StationCollectionCommandError::Scheduled);
                }
                return Err(StationCollectionCommandError::Admission(
                    StationCollectionAdmissionError::AlreadyRunning,
                ));
            }
            Err(error) => return Err(StationCollectionCommandError::Admission(error)),
        };
        self.run_station_collection_inner(station_id, task).await
    }

    /// Scan recharge pages through the same station lease and credential
    /// source as balance/group collection. The browser result is persisted as
    /// a collector snapshot so the UI never invents an entry from a guessed
    /// path.
    pub(crate) async fn scan_station_recharge<F, Fut>(
        &self,
        station_id: String,
        scan: F,
    ) -> Result<CollectorRunResult, StationCollectionCommandError>
    where
        F: FnOnce(RechargeScanRequest) -> Fut,
        Fut: Future<Output = RechargeScanCapture>,
    {
        // Background balance/group collection may already hold this station's
        // lease. Recharge discovery is user initiated and short-lived, so wait
        // briefly for that same lease instead of failing immediately with a
        // generic IPC conflict.
        let _lease =
            acquire_recharge_station_lease(&self.station_collection_coordinator, &station_id)
                .await
                .map_err(StationCollectionCommandError::Admission)?;
        self.scan_station_recharge_inner(station_id, scan).await
    }

    pub(crate) async fn redeem_station_code(
        &self,
        station_id: String,
        code: String,
    ) -> Result<StationRedemptionResult, StationCollectionCommandError> {
        let _lease =
            acquire_recharge_station_lease(&self.station_collection_coordinator, &station_id)
                .await
                .map_err(StationCollectionCommandError::Admission)?;
        self.redeem_station_code_inner(station_id, code).await
    }

    async fn redeem_station_code_inner(
        &self,
        station_id: String,
        code: String,
    ) -> Result<StationRedemptionResult, StationCollectionCommandError> {
        let station = self
            .collectors
            .station_for_collection(&station_id)
            .await
            .map_err(|_| StationCollectionCommandError::Prepare(ApplicationError::Internal))?;
        let settings = self
            .settings
            .load()
            .await
            .map_err(|_| StationCollectionCommandError::Prepare(ApplicationError::Internal))?;
        let budget = RequestBudget::from_now(std::time::Duration::from_secs(u64::from(
            settings.collector_timeout_seconds,
        )));
        let cancellation = tokio_util::sync::CancellationToken::new();
        let correlation_id = current_correlation_id();
        let code = code.trim().to_string();
        let credentials = self
            .credentials
            .get_station_credentials(station_id.clone())
            .await
            .map_err(|_| StationCollectionCommandError::Prepare(ApplicationError::Internal))?;
        let source = self.source();
        let Some(session) = run_with_redemption_budget(
            budget,
            collectors::resolve_station_session_for_operation(
                &source,
                &self.outbound,
                station_id.clone(),
                collectors::StationSessionResolveMode::ReuseUsable,
                cancellation.clone(),
                correlation_id.clone(),
            ),
        )
        .await
        else {
            return Ok(crate::services::station_redemption::timeout_result(
                &station.station_type,
            ));
        };
        let session = session.map_err(StationCollectionCommandError::Prepare)?;
        let proxy_config = crate::services::outbound::resolve_proxy_config(
            &station.collector_proxy_mode,
            station.collector_proxy_url.clone(),
            &settings.collector_proxy_mode,
            settings.collector_proxy_url,
        );
        let proxy = crate::services::outbound::proxy_policy_from_mode(
            &proxy_config.mode,
            proxy_config.url.as_deref(),
        )
        .map_err(|_| StationCollectionCommandError::Prepare(ApplicationError::Internal))?;

        let attempt = crate::services::station_redemption::redeem_station_code(
            &self.outbound,
            &station,
            &session,
            &code,
            credentials.session_user_agent.as_deref(),
            proxy.clone(),
            budget,
            cancellation.clone(),
            correlation_id.clone(),
        )
        .await;
        if !attempt.authentication_rejected || !station.station_type.eq_ignore_ascii_case("sub2api")
        {
            return Ok(attempt.result);
        }
        let mut last_attempt = attempt;
        if let Some(saved_refresh_token) = session
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if let Some(refreshed) = crate::services::station_redemption::refresh_sub2api_session(
                &self.outbound,
                &station,
                saved_refresh_token,
                session.cookie.as_deref(),
                credentials.session_user_agent.as_deref(),
                proxy.clone(),
                budget,
                cancellation.clone(),
                correlation_id.clone(),
            )
            .await
            {
                let refreshed_session = ResolvedSession {
                    status: SessionResolveStatus::Ready,
                    access_token: Some(refreshed.access_token.clone()),
                    refresh_token: Some(
                        refreshed
                            .refresh_token
                            .clone()
                            .unwrap_or_else(|| saved_refresh_token.to_string()),
                    ),
                    cookie: refreshed.cookie.clone(),
                    newapi_user_id: session.newapi_user_id.clone(),
                    message: None,
                };
                let Some(persisted) = run_with_redemption_budget(
                    budget,
                    self.credentials.persist_station_session_if_revision(
                        PersistStationSessionInput {
                            station_id: station.id.clone(),
                            access_token: refreshed_session.access_token.clone(),
                            refresh_token: refreshed_session.refresh_token.clone(),
                            cookie: refreshed_session.cookie.clone(),
                            newapi_user_id: refreshed_session.newapi_user_id.clone(),
                            token_expires_at: refreshed.token_expires_at.clone(),
                            session_expires_at: refreshed.token_expires_at,
                            session_source: "refresh_token".to_string(),
                            session_user_agent: credentials.session_user_agent.clone(),
                        },
                        station.endpoint_revision,
                    ),
                )
                .await
                else {
                    return Ok(crate::services::station_redemption::timeout_result(
                        &station.station_type,
                    ));
                };
                persisted.map_err(|_| {
                    StationCollectionCommandError::Prepare(ApplicationError::Internal)
                })?;
                last_attempt = crate::services::station_redemption::redeem_station_code(
                    &self.outbound,
                    &station,
                    &refreshed_session,
                    &code,
                    credentials.session_user_agent.as_deref(),
                    proxy.clone(),
                    budget,
                    cancellation.clone(),
                    correlation_id.clone(),
                )
                .await;
                if !last_attempt.authentication_rejected {
                    return Ok(last_attempt.result);
                }
            }
        }

        if budget.remaining().is_none() {
            return Ok(crate::services::station_redemption::timeout_result(
                &station.station_type,
            ));
        }
        let Some(password_session) = run_with_redemption_budget(
            budget,
            collectors::resolve_station_session_for_operation(
                &source,
                &self.outbound,
                station_id,
                collectors::StationSessionResolveMode::ForcePasswordLogin,
                cancellation.clone(),
                correlation_id.clone(),
            ),
        )
        .await
        else {
            return Ok(crate::services::station_redemption::timeout_result(
                &station.station_type,
            ));
        };
        let password_session = password_session.map_err(StationCollectionCommandError::Prepare)?;
        if !redemption_session_has_authentication(&password_session) {
            return Ok(last_attempt.result);
        }

        Ok(crate::services::station_redemption::redeem_station_code(
            &self.outbound,
            &station,
            &password_session,
            &code,
            credentials.session_user_agent.as_deref(),
            proxy,
            budget,
            cancellation,
            correlation_id,
        )
        .await
        .result)
    }

    async fn scan_station_recharge_inner<F, Fut>(
        &self,
        station_id: String,
        scan: F,
    ) -> Result<CollectorRunResult, StationCollectionCommandError>
    where
        F: FnOnce(RechargeScanRequest) -> Fut,
        Fut: Future<Output = RechargeScanCapture>,
    {
        let source = self.source();
        let station = self
            .collectors
            .station_for_collection(&station_id)
            .await
            .map_err(|_| StationCollectionCommandError::Prepare(ApplicationError::Internal))?;
        let session = match tokio::time::timeout(
            std::time::Duration::from_secs(25),
            collectors::resolve_station_session_for_operation(
                &source,
                &self.outbound,
                station_id.clone(),
                collectors::StationSessionResolveMode::ReuseUsable,
                tokio_util::sync::CancellationToken::new(),
                current_correlation_id(),
            ),
        )
        .await
        {
            Ok(result) => result.map_err(StationCollectionCommandError::Prepare)?,
            Err(_) => crate::models::credentials::ResolvedSession::manual_required(
                "session resolve timed out; public recharge scan fallback",
            ),
        };
        let session_usable = recharge_session_is_usable(&station.station_type, &session);
        // A recharge page can be public even when the station has no saved
        // session (for example a public `/custom/...` shop). Do not turn the
        // absence of credentials into an authorization prompt before opening
        // the page. If the rendered page actually redirects to login, the
        // browser probe will return `login_required` and preserve that signal.
        let capture = scan(RechargeScanRequest {
            website_url: station.website_url,
            station_type: station.station_type,
            session_usable,
            cookie: session_usable.then_some(session.cookie).flatten(),
            access_token: session_usable.then_some(session.access_token).flatten(),
            refresh_token: session_usable.then_some(session.refresh_token).flatten(),
            newapi_user_id: session_usable.then_some(session.newapi_user_id).flatten(),
        })
        .await;
        self.record_recharge_snapshot(
            station.id,
            station.endpoint_revision,
            &capture.status,
            capture.summary_json,
            capture.normalized_json,
            capture.error_message,
            capture.event_count,
        )
        .await
    }

    async fn record_recharge_snapshot(
        &self,
        station_id: String,
        endpoint_revision: i64,
        status: &str,
        summary_json: serde_json::Value,
        normalized_json: serde_json::Value,
        error_message: Option<String>,
        event_count: i64,
    ) -> Result<CollectorRunResult, StationCollectionCommandError> {
        self.collectors
            .record_capture_snapshot(CaptureSnapshotRequest {
                station_id,
                endpoint_revision,
                task_type: "recharge".to_string(),
                status: status.to_string(),
                summary_json,
                normalized_json,
                raw_json_redacted: None,
                error_message,
                event_count,
            })
            .await
            .map_err(StationCollectionCommandError::Prepare)
    }

    async fn run_station_collection_inner(
        &self,
        station_id: String,
        task: CollectorTask,
    ) -> Result<CollectorRunResult, StationCollectionCommandError> {
        let station_id_for_remote_keys = station_id.clone();
        let source = self.source();
        let cancellation_token = tokio_util::sync::CancellationToken::new();
        let prepared = self
            .blocking
            .submit_wait_for_capacity(
                "station_collection_prepare",
                None,
                current_correlation_id(),
                None,
                &cancellation_token,
                move |_| {
                    Ok(collectors::prepare_station_collection_route_v2(
                        &source, station_id, task,
                    ))
                },
            )
            .await
            .map_err(StationCollectionCommandError::Blocking)?
            .result()
            .await
            .map_err(StationCollectionCommandError::Blocking)?
            .map_err(StationCollectionCommandError::Prepare)?;
        let prepared = match prepared {
            collectors::PreparedStationCollectionRoute::Sub2Api(prepared) => {
                collectors::finish_sub2api_collection_v2(
                    self.providers.as_ref(),
                    &self.outbound,
                    prepared,
                    tokio_util::sync::CancellationToken::new(),
                    current_correlation_id(),
                )
                .await
                .map_err(StationCollectionCommandError::Prepare)?
            }
            collectors::PreparedStationCollectionRoute::NewApi(prepared) => {
                let source = self.source();
                collectors::finish_newapi_collection_v2(
                    &source,
                    self.providers.as_ref(),
                    &self.outbound,
                    prepared,
                    tokio_util::sync::CancellationToken::new(),
                    current_correlation_id(),
                )
                .await
                .map_err(StationCollectionCommandError::Prepare)?
            }
        };
        let result = self.apply_prepared_collection(prepared).await?;
        Ok(append_remote_key_refresh_event(task, result, async {
            self.remote_keys
                .scan_remote_station_keys(station_id_for_remote_keys)
                .await
                .map(|scan| scan.message)
        })
        .await)
    }

    pub(crate) async fn test_station_login(
        &self,
        station_id: String,
    ) -> Result<CollectorRunResult, StationCollectionCommandError> {
        let station_id_for_lease = station_id.clone();
        run_with_station_collection_lease(
            &self.station_collection_coordinator,
            &station_id_for_lease,
            || self.test_station_login_inner(station_id),
        )
        .await
    }

    async fn test_station_login_inner(
        &self,
        station_id: String,
    ) -> Result<CollectorRunResult, StationCollectionCommandError> {
        let source = self.source();
        let cancellation_token = tokio_util::sync::CancellationToken::new();
        let prepared = self
            .blocking
            .submit_wait_for_capacity(
                "station_login_prepare",
                None,
                current_correlation_id(),
                None,
                &cancellation_token,
                move |_| {
                    Ok(collectors::prepare_station_login_probe_v2(
                        &source, station_id,
                    ))
                },
            )
            .await
            .map_err(StationCollectionCommandError::Blocking)?
            .result()
            .await
            .map_err(StationCollectionCommandError::Blocking)?
            .map_err(StationCollectionCommandError::Prepare)?;
        let source = self.source();
        let prepared = collectors::finish_station_login_probe_v2(
            &source,
            &self.outbound,
            prepared,
            tokio_util::sync::CancellationToken::new(),
            current_correlation_id(),
        )
        .await
        .map_err(StationCollectionCommandError::Prepare)?;
        self.apply_prepared_collection(prepared).await
    }

    fn source(&self) -> V2CollectorSourceAdapter {
        V2CollectorSourceAdapter::new(
            Arc::clone(&self.collectors),
            Arc::clone(&self.credentials),
            Arc::clone(&self.settings),
        )
    }

    async fn apply_prepared_collection(
        &self,
        prepared: collectors::PreparedStationCollection,
    ) -> Result<CollectorRunResult, StationCollectionCommandError> {
        let apply = collectors::apply::V2CollectorApplyAdapter::new((*self.collectors).clone());
        collectors::apply_prepared_station_collection_v2(&self.collectors, &apply, prepared)
            .await
            .map_err(StationCollectionCommandError::Apply)
    }
}

fn recharge_session_is_usable(
    station_type: &str,
    session: &crate::models::credentials::ResolvedSession,
) -> bool {
    // `resolve_station_session` deliberately returns Ready for sessions that
    // can still be refreshed through the normal collector (for example an
    // expired access token plus a refresh token). A hidden WebView cannot
    // perform that application-level recovery safely, so only a session with
    // no recovery message and a directly injectable secret may start a scan.
    let has_cookie = session
        .cookie
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let has_access_token = session
        .access_token
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let provider_identity_is_usable = !station_type.eq_ignore_ascii_case("newapi")
        || session
            .newapi_user_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
    (station_type.eq_ignore_ascii_case("sub2api") && has_cookie && !has_access_token)
        || (session.message.is_none()
            && provider_identity_is_usable
            && (has_cookie || has_access_token))
}

fn redemption_session_has_authentication(session: &ResolvedSession) -> bool {
    session
        .access_token
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        || session
            .cookie
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
}

async fn run_with_redemption_budget<T, F>(budget: RequestBudget, future: F) -> Option<T>
where
    F: Future<Output = T>,
{
    let remaining = budget.remaining()?;
    tokio::time::timeout(remaining, future).await.ok()
}

async fn run_with_station_collection_lease<T, F, Fut>(
    coordinator: &StationCollectionCoordinator,
    station_id: &str,
    operation: F,
) -> Result<T, StationCollectionCommandError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, StationCollectionCommandError>>,
{
    let _lease = coordinator
        .try_acquire(station_id)
        .map_err(StationCollectionCommandError::Admission)?;
    operation().await
}

async fn acquire_recharge_station_lease(
    coordinator: &StationCollectionCoordinator,
    station_id: &str,
) -> Result<
    crate::services::station_collection_coordinator::StationCollectionLease,
    StationCollectionAdmissionError,
> {
    const WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(25);
    const RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);
    let deadline = tokio::time::Instant::now() + WAIT_TIMEOUT;
    loop {
        match coordinator.try_acquire(station_id) {
            Ok(lease) => return Ok(lease),
            Err(StationCollectionAdmissionError::AlreadyRunning)
            | Err(StationCollectionAdmissionError::AtCapacity) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(StationCollectionAdmissionError::AlreadyRunning);
                }
                tokio::time::sleep(RETRY_INTERVAL).await;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn append_remote_key_refresh_event<F>(
    task: CollectorTask,
    mut result: CollectorRunResult,
    refresh: F,
) -> CollectorRunResult
where
    F: Future<Output = Result<String, RemoteKeyOperationError>>,
{
    if !collectors::should_refresh_remote_keys_after_collection(
        task,
        result.snapshot.status.as_str(),
    ) {
        return result;
    }

    let event = match refresh.await {
        Ok(message) => CollectorEvent {
            event_type: REMOTE_KEY_REFRESH_EVENT.to_string(),
            message,
            status: "success".to_string(),
        },
        Err(error) => CollectorEvent {
            event_type: REMOTE_KEY_REFRESH_EVENT.to_string(),
            message: remote_key_refresh_error_message(&error),
            status: "failed".to_string(),
        },
    };
    result.events.push(event);
    result
}

fn remote_key_refresh_error_message(error: &RemoteKeyOperationError) -> String {
    match error {
        RemoteKeyOperationError::Unsupported => "当前站点不支持远端密钥扫描。".to_string(),
        RemoteKeyOperationError::UnsupportedWithDetail(detail) => detail.clone(),
        RemoteKeyOperationError::ExternalUnavailable(_) => "远端密钥接口暂时不可用。".to_string(),
        RemoteKeyOperationError::ExternalUnavailableWithDetail { detail, .. } => detail.clone(),
        RemoteKeyOperationError::ResultUnknown => "远端密钥扫描结果无法确认。".to_string(),
        RemoteKeyOperationError::Conflict => "站点配置已变化，请重新采集。".to_string(),
        RemoteKeyOperationError::Application(_) | RemoteKeyOperationError::Internal => {
            "远端密钥刷新失败。".to_string()
        }
    }
}

fn current_correlation_id() -> Option<String> {
    correlation::current().map(|id| id.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::{future::pending, num::NonZeroUsize};

    use serde_json::json;
    use tokio::sync::oneshot;

    use super::*;
    use crate::{
        models::collector::CollectorSnapshot,
        models::credentials::{ResolvedSession, SessionResolveStatus},
        services::station_collection_coordinator::{
            StationCollectionAdmissionError, StationCollectionCoordinator,
        },
    };

    fn result(status: &str) -> CollectorRunResult {
        CollectorRunResult {
            snapshot: CollectorSnapshot {
                id: "snapshot-1".to_string(),
                station_id: "station-1".to_string(),
                endpoint_revision: 1,
                source: "fixture".to_string(),
                status: status.to_string(),
                fetched_at: "1700000000000".to_string(),
                summary_json: json!({}),
                normalized_json: json!({}),
                raw_json_redacted: None,
                error_message: None,
                created_at: "1700000000000".to_string(),
            },
            events: Vec::new(),
        }
    }

    #[test]
    fn recharge_session_requires_directly_injectable_authentication() {
        let mut session = ResolvedSession {
            status: SessionResolveStatus::Ready,
            access_token: None,
            refresh_token: Some("fixture-refresh".to_string()),
            cookie: None,
            newapi_user_id: None,
            message: Some("session refresh or login is required".to_string()),
        };
        assert!(!recharge_session_is_usable("sub2api", &session));

        session.access_token = Some("fixture-access".to_string());
        session.message = None;
        assert!(recharge_session_is_usable("sub2api", &session));
        assert!(!recharge_session_is_usable("newapi", &session));
        session.newapi_user_id = Some("42".to_string());
        assert!(recharge_session_is_usable("newapi", &session));
        session.message = Some("refresh required".to_string());
        assert!(!recharge_session_is_usable("sub2api", &session));
        session.access_token = None;
        session.message = None;
        session.cookie = Some("session=fixture".to_string());
        assert!(recharge_session_is_usable("sub2api", &session));
    }

    #[tokio::test]
    async fn successful_group_collection_refreshes_remote_keys() {
        let refreshed = Arc::new(AtomicBool::new(false));
        let refreshed_by_scan = Arc::clone(&refreshed);
        let result =
            append_remote_key_refresh_event(CollectorTask::Groups, result("success"), async move {
                refreshed_by_scan.store(true, Ordering::SeqCst);
                Ok("remote keys refreshed".to_string())
            })
            .await;

        assert!(refreshed.load(Ordering::SeqCst));
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].event_type, REMOTE_KEY_REFRESH_EVENT);
        assert_eq!(result.events[0].status, "success");
    }

    #[tokio::test]
    async fn unrelated_or_failed_collection_does_not_refresh_remote_keys() {
        for (task, status) in [
            (CollectorTask::Balance, "success"),
            (CollectorTask::Groups, "failed"),
        ] {
            let refreshed = Arc::new(AtomicBool::new(false));
            let refreshed_by_scan = Arc::clone(&refreshed);
            let result = append_remote_key_refresh_event(task, result(status), async move {
                refreshed_by_scan.store(true, Ordering::SeqCst);
                Ok("unexpected".to_string())
            })
            .await;

            assert!(!refreshed.load(Ordering::SeqCst));
            assert!(result.events.is_empty());
        }
    }

    #[tokio::test]
    async fn remote_key_refresh_failure_is_reported_without_discarding_collection() {
        let result =
            append_remote_key_refresh_event(CollectorTask::Full, result("partial"), async {
                Err(RemoteKeyOperationError::ExternalUnavailable(
                    crate::services::remote_keys::RemoteKeyExternalFailureReason::ProviderUnavailable,
                ))
            })
            .await;

        assert_eq!(result.snapshot.status, "partial");
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].event_type, REMOTE_KEY_REFRESH_EVENT);
        assert_eq!(result.events[0].status, "failed");
        assert_eq!(result.events[0].message, "远端密钥接口暂时不可用。");
    }

    #[tokio::test]
    async fn remote_key_refresh_preserves_safe_external_failure_detail() {
        let result =
            append_remote_key_refresh_event(CollectorTask::Full, result("partial"), async {
                Err(RemoteKeyOperationError::ExternalUnavailableWithDetail {
                    reason: crate::services::remote_keys::RemoteKeyExternalFailureReason::AuthenticationRejected,
                    detail: "Sub2API remote-key list request was rejected (HTTP 403)".to_string(),
                })
            })
            .await;

        assert_eq!(result.events.len(), 1);
        assert_eq!(
            result.events[0].message,
            "Sub2API remote-key list request was rejected (HTTP 403)"
        );
    }

    #[tokio::test]
    async fn admission_failure_does_not_poll_manual_operation() {
        let coordinator =
            StationCollectionCoordinator::new(NonZeroUsize::new(1).expect("non-zero limit"));
        let _held = coordinator.try_acquire("station-1").expect("station held");
        let ran = Arc::new(AtomicBool::new(false));
        let ran_by_operation = Arc::clone(&ran);

        let result = run_with_station_collection_lease(&coordinator, "station-1", || async move {
            ran_by_operation.store(true, Ordering::SeqCst);
            Ok::<_, StationCollectionCommandError>(())
        })
        .await;

        assert!(matches!(
            result,
            Err(StationCollectionCommandError::Admission(
                StationCollectionAdmissionError::AlreadyRunning
            ))
        ));
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn manual_lease_releases_after_failure_and_rejects_capacity_without_polling() {
        let coordinator =
            StationCollectionCoordinator::new(NonZeroUsize::new(1).expect("non-zero limit"));
        let failed = run_with_station_collection_lease(&coordinator, "station-1", || async {
            Err::<(), _>(StationCollectionCommandError::Prepare(
                ApplicationError::Internal,
            ))
        })
        .await;
        assert!(matches!(
            failed,
            Err(StationCollectionCommandError::Prepare(
                ApplicationError::Internal
            ))
        ));
        assert!(coordinator.try_acquire("station-1").is_ok());

        let _held = coordinator.try_acquire("station-1").expect("station held");
        let ran = Arc::new(AtomicBool::new(false));
        let ran_by_operation = Arc::clone(&ran);
        let rejected =
            run_with_station_collection_lease(&coordinator, "station-2", || async move {
                ran_by_operation.store(true, Ordering::SeqCst);
                Ok::<_, StationCollectionCommandError>(())
            })
            .await;

        assert!(matches!(
            rejected,
            Err(StationCollectionCommandError::Admission(
                StationCollectionAdmissionError::AtCapacity
            ))
        ));
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn manual_lease_excludes_same_station_until_operation_completes_or_is_aborted() {
        let coordinator = Arc::new(StationCollectionCoordinator::new(
            NonZeroUsize::new(1).expect("non-zero limit"),
        ));
        let (started_sender, started_receiver) = oneshot::channel();
        let (release_sender, release_receiver) = oneshot::channel();
        let running_coordinator = Arc::clone(&coordinator);
        let running = tokio::spawn(async move {
            run_with_station_collection_lease(&running_coordinator, "station-1", || async move {
                started_sender
                    .send(())
                    .expect("operation start is observed");
                release_receiver.await.expect("operation is released");
                Ok::<_, StationCollectionCommandError>(())
            })
            .await
        });
        started_receiver.await.expect("operation starts");
        assert!(matches!(
            coordinator.try_acquire("station-1"),
            Err(StationCollectionAdmissionError::AlreadyRunning)
        ));
        release_sender.send(()).expect("release operation");
        running
            .await
            .expect("operation joins")
            .expect("operation succeeds");
        assert!(coordinator.try_acquire("station-1").is_ok());

        let (abort_started_sender, abort_started_receiver) = oneshot::channel();
        let abort_coordinator = Arc::clone(&coordinator);
        let aborted = tokio::spawn(async move {
            run_with_station_collection_lease(&abort_coordinator, "station-2", || async move {
                abort_started_sender
                    .send(())
                    .expect("abort operation start is observed");
                pending::<Result<(), StationCollectionCommandError>>().await
            })
            .await
        });
        abort_started_receiver
            .await
            .expect("abort operation starts");
        aborted.abort();
        assert!(aborted.await.expect_err("operation aborts").is_cancelled());
        assert!(coordinator.try_acquire("station-2").is_ok());
    }
}
