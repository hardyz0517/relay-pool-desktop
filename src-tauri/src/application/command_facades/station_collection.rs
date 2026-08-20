use std::{future::Future, sync::Arc};

use serde_json::json;

use crate::{
    application::{
        collectors::{CaptureSnapshotRequest, CollectorService},
        credentials::CredentialService,
        error::ApplicationError,
        settings::SettingsService,
    },
    background_tasks::{BlockingExecutor, BlockingExecutorError},
    models::collector::{CollectorEvent, CollectorRunResult},
    observability::correlation,
    outbound::AsyncOutboundClient,
    services::{
        collectors::{self, output::CollectorTask, V2CollectorSourceAdapter},
        remote_keys::RemoteKeyOperationError,
        station_collection_coordinator::{
            StationCollectionAdmissionError, StationCollectionCoordinator,
        },
    },
};

use super::remote_keys::RemoteKeysCommandFacade;

const REMOTE_KEY_REFRESH_EVENT: &str = "remote_keys";

#[derive(Debug)]
pub(crate) enum StationCollectionCommandError {
    Admission(StationCollectionAdmissionError),
    Prepare(ApplicationError),
    Apply(ApplicationError),
    Blocking(BlockingExecutorError),
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
        }
    }

    pub(crate) async fn run_station_collection(
        &self,
        station_id: String,
        task: CollectorTask,
    ) -> Result<CollectorRunResult, StationCollectionCommandError> {
        let station_id_for_lease = station_id.clone();
        run_with_station_collection_lease(
            &self.station_collection_coordinator,
            &station_id_for_lease,
            || self.run_station_collection_inner(station_id, task),
        )
        .await
    }

    /// Scan recharge pages through the same station lease and credential
    /// source as balance/group collection. The browser result is persisted as
    /// a collector snapshot so the UI never invents an entry from a guessed
    /// path.
    pub(crate) async fn scan_station_recharge(
        &self,
        app: &tauri::AppHandle,
        station_id: String,
    ) -> Result<CollectorRunResult, StationCollectionCommandError> {
        // Background balance/group collection may already hold this station's
        // lease. Recharge discovery is user initiated and short-lived, so wait
        // briefly for that same lease instead of failing immediately with a
        // generic IPC conflict.
        let _lease =
            acquire_recharge_station_lease(&self.station_collection_coordinator, &station_id)
                .await
                .map_err(StationCollectionCommandError::Admission)?;
        self.scan_station_recharge_inner(app, station_id).await
    }

    async fn scan_station_recharge_inner(
        &self,
        app: &tauri::AppHandle,
        station_id: String,
    ) -> Result<CollectorRunResult, StationCollectionCommandError> {
        let source = self.source();
        let station = self
            .collectors
            .station_for_collection(&station_id)
            .await
            .map_err(|_| StationCollectionCommandError::Prepare(ApplicationError::Internal))?;
        let session = match tokio::time::timeout(
            std::time::Duration::from_secs(25),
            collectors::resolve_station_session_for_browser(
                &source,
                &self.outbound,
                station_id.clone(),
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
        let session_usable = recharge_session_is_usable(&session);
        // A recharge page can be public even when the station has no saved
        // session (for example a public `/custom/...` shop). Do not turn the
        // absence of credentials into an authorization prompt before opening
        // the page. If the rendered page actually redirects to login, the
        // browser probe will return `login_required` and preserve that signal.
        let browser_session = if session_usable {
            crate::commands::browser_transport::RechargeSession {
                cookie: session.cookie.as_deref(),
                access_token: session.access_token.as_deref(),
                refresh_token: session.refresh_token.as_deref(),
                newapi_user_id: session.newapi_user_id.as_deref(),
            }
        } else {
            crate::commands::browser_transport::RechargeSession::default()
        };

        let probe = crate::commands::browser_transport::scan_recharge_page(
            app,
            &station.website_url,
            &station.station_type,
            browser_session,
        )
        .await;
        match probe {
            Ok(probe) => {
                let status = match probe.status.as_str() {
                    "success" => "success",
                    "login_required" => "manual_required",
                    "not_found" | "no_match" => "partial",
                    _ => "failed",
                };
                let message = match probe.status.as_str() {
                    "login_required" => Some("站点页面要求登录，请先完成浏览器授权。".to_string()),
                    "not_found" => Some("页面明确返回 404，未生成充值入口。".to_string()),
                    "no_match" => Some(if session_usable {
                        "已打开登录后的页面，但未发现可确认的充值入口。".to_string()
                    } else {
                        "已打开站点页面，但未发现可确认的充值入口。".to_string()
                    }),
                    _ => None,
                };
                self.record_recharge_snapshot(
                    station.id,
                    station.endpoint_revision,
                    status,
                    json!({
                        "collector": "recharge",
                        "status": probe.status,
                        "currentUrl": probe.current_url,
                        "title": probe.title,
                        "provider": probe.provider,
                        "paymentMethods": probe.payment_methods,
                        "protectedCandidates": probe.protected_candidates,
                        "candidateDiagnostics": probe.candidate_diagnostics,
                        "evidence": probe.evidence,
                        "scan": {
                            "phase": "completed",
                            "sessionMode": if session_usable { "authenticated" } else { "public_fallback" },
                            "candidateCount": probe.candidates_scanned,
                            "entryCount": probe.entries.len()
                        }
                    }),
                    json!({ "entries": probe.entries }),
                    message,
                    probe.entries.len() as i64,
                )
                .await
            }
            Err(error) => {
                let message = if error.is_timeout() {
                    error.recharge_message()
                } else if matches!(
                    error.recharge_diagnostic()["kind"].as_str(),
                    Some("cross_origin_redirect")
                ) {
                    error.recharge_message()
                } else {
                    "充值页面采集失败，请检查站点地址和登录状态。".to_string()
                };
                self.record_recharge_snapshot(
                    station.id,
                    station.endpoint_revision,
                    "failed",
                    json!({
                        "collector": "recharge",
                        "status": "error",
                        "scan": error.recharge_diagnostic()
                    }),
                    json!({ "entries": [] }),
                    Some(message),
                    0,
                )
                .await
            }
        }
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
        let prepared = self
            .blocking
            .submit(
                "station_collection_prepare",
                None,
                current_correlation_id(),
                None,
                move |_| {
                    Ok(collectors::prepare_station_collection_route_v2(
                        &source, station_id, task,
                    ))
                },
            )
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
        let prepared = self
            .blocking
            .submit(
                "station_login_prepare",
                None,
                current_correlation_id(),
                None,
                move |_| {
                    Ok(collectors::prepare_station_login_probe_v2(
                        &source, station_id,
                    ))
                },
            )
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

fn recharge_session_is_usable(session: &crate::models::credentials::ResolvedSession) -> bool {
    // `resolve_station_session` deliberately returns Ready for sessions that
    // can still be refreshed through the normal collector (for example an
    // expired access token plus a refresh token). A hidden WebView cannot
    // perform that application-level recovery safely, so only a session with
    // no recovery message and a directly injectable secret may start a scan.
    session.message.is_none()
        && session
            .newapi_user_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        && (session
            .cookie
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            || session
                .access_token
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()))
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
        assert!(!recharge_session_is_usable(&session));

        session.access_token = Some("fixture-access".to_string());
        session.newapi_user_id = Some("42".to_string());
        session.message = None;
        assert!(recharge_session_is_usable(&session));
        session.message = Some("refresh required".to_string());
        assert!(!recharge_session_is_usable(&session));
        session.access_token = None;
        session.message = None;
        session.cookie = Some("session=fixture".to_string());
        assert!(recharge_session_is_usable(&session));
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
