use std::{collections::BTreeMap, sync::Arc};

use crate::{
    application::{
        error::ApplicationError, provider_drafts::ProviderDraftService, settings::SettingsService,
    },
    background_tasks::{BlockingExecutor, BlockingExecutorError},
    models::{
        credentials::{
            PersistStationSessionInput, ResolvedSession, StationCredentials,
            StationSessionCredentialKind, UpdateStationSessionInput,
        },
        group_facts::StationGroupBinding,
        provider_drafts::{
            CommitProviderDraftInput, CreateProviderDraftInput, PatchProviderDraftInput,
            ProviderDraft, ProviderDraftPreview,
        },
        remote_keys::RemoteKeyScanResult,
        settings::AppSettings,
        station_keys::StationKey,
        stations::Station,
    },
    observability::correlation,
    outbound::AsyncOutboundClient,
    services::{
        collectors::{
            self, orchestration::ProviderRegistry, output::CollectorTask, CollectorSourcePort,
        },
        remote_keys::{self, RemoteKeyOperationError},
    },
};

#[derive(Debug)]
pub(crate) enum ProviderDraftCommandError {
    Application(ApplicationError),
    Blocking(BlockingExecutorError),
    Remote(RemoteKeyOperationError),
}

impl From<ApplicationError> for ProviderDraftCommandError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

impl From<BlockingExecutorError> for ProviderDraftCommandError {
    fn from(error: BlockingExecutorError) -> Self {
        Self::Blocking(error)
    }
}

impl From<RemoteKeyOperationError> for ProviderDraftCommandError {
    fn from(error: RemoteKeyOperationError) -> Self {
        Self::Remote(error)
    }
}

#[derive(Clone)]
pub(crate) struct ProviderDraftCommandFacade {
    drafts: Arc<ProviderDraftService>,
    settings: Arc<SettingsService>,
    blocking: BlockingExecutor,
    outbound: AsyncOutboundClient,
    providers: Arc<ProviderRegistry>,
    data_key: [u8; 32],
}

impl ProviderDraftCommandFacade {
    pub(crate) fn new(
        drafts: Arc<ProviderDraftService>,
        settings: Arc<SettingsService>,
        blocking: BlockingExecutor,
        outbound: AsyncOutboundClient,
        providers: Arc<ProviderRegistry>,
        data_key: [u8; 32],
    ) -> Self {
        Self {
            drafts,
            settings,
            blocking,
            outbound,
            providers,
            data_key,
        }
    }

    pub(crate) async fn create_or_resume(
        &self,
        input: CreateProviderDraftInput,
    ) -> Result<ProviderDraft, ProviderDraftCommandError> {
        self.drafts
            .create_or_resume(input)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn get(
        &self,
        draft_id: String,
    ) -> Result<ProviderDraft, ProviderDraftCommandError> {
        self.drafts.get(draft_id).await.map_err(Into::into)
    }

    pub(crate) async fn patch(
        &self,
        input: PatchProviderDraftInput,
    ) -> Result<ProviderDraft, ProviderDraftCommandError> {
        self.drafts.patch(input).await.map_err(Into::into)
    }

    pub(crate) async fn discard(&self, draft_id: String) -> Result<(), ProviderDraftCommandError> {
        self.drafts.discard(draft_id).await.map_err(Into::into)
    }

    pub(crate) async fn commit(
        &self,
        input: CommitProviderDraftInput,
    ) -> Result<Station, ProviderDraftCommandError> {
        self.drafts.commit(input).await.map_err(Into::into)
    }

    pub(crate) async fn collect_preview(
        &self,
        draft_id: String,
        task: CollectorTask,
    ) -> Result<ProviderDraftPreview, ProviderDraftCommandError> {
        let draft = self.drafts.get(draft_id.clone()).await?;
        let fingerprint = ProviderDraftService::runtime_fingerprint(&draft.payload);
        let finish_draft_id = draft_id.clone();
        let source = self.snapshot_source(draft_id.clone()).await?;
        let data_key = self.data_key;
        let prepared = super::draft_jobs::prepare_collection_plan(
            &self.blocking,
            source,
            data_key,
            draft_id,
            task,
        )
        .await??;
        let prepared = match prepared {
            collectors::PreparedStationCollectionRoute::Sub2Api(prepared) => {
                collectors::finish_sub2api_collection_v2(
                    self.providers.as_ref(),
                    &self.outbound,
                    prepared,
                    tokio_util::sync::CancellationToken::new(),
                    current_correlation_id(),
                )
                .await?
            }
            collectors::PreparedStationCollectionRoute::OpenAiCompatible(prepared) => {
                collectors::finish_openai_compatible_collection_v2(
                    self.providers.as_ref(),
                    &self.outbound,
                    prepared,
                    tokio_util::sync::CancellationToken::new(),
                    current_correlation_id(),
                )
                .await?
            }
            collectors::PreparedStationCollectionRoute::NewApi(prepared) => {
                let source = self.source(finish_draft_id);
                collectors::finish_newapi_collection_v2(
                    &source,
                    self.providers.as_ref(),
                    &self.outbound,
                    prepared,
                    tokio_util::sync::CancellationToken::new(),
                    current_correlation_id(),
                )
                .await?
            }
        };
        let preview = collectors::provider_draft_preview_from_prepared(
            prepared,
            fingerprint,
            chrono::Utc::now().timestamp_millis().to_string(),
        );
        self.drafts.store_preview(preview).await.map_err(Into::into)
    }

    pub(crate) async fn scan_remote_keys(
        &self,
        draft_id: String,
    ) -> Result<RemoteKeyScanResult, ProviderDraftCommandError> {
        let prepare_source = self.snapshot_source(draft_id.clone()).await?;
        let data_key = self.data_key;
        let newapi = super::draft_jobs::prepare_newapi_key_scan_plan(
            &self.blocking,
            prepare_source,
            data_key,
            draft_id.clone(),
        )
        .await??;
        if let Some(prepared) = newapi {
            let prepared = remote_keys::prepare_newapi_remote_key_scan_v2(
                self.providers.as_ref(),
                &self.outbound,
                prepared,
                tokio_util::sync::CancellationToken::new(),
                current_correlation_id(),
            )
            .await?;
            return remote_keys::preview_remote_key_scan_v2(prepared).map_err(Into::into);
        }

        let source = self.snapshot_source(draft_id.clone()).await?;
        let data_key = self.data_key;
        let sub2api = super::draft_jobs::prepare_sub2api_key_scan_plan(
            &self.blocking,
            source,
            data_key,
            draft_id.clone(),
        )
        .await??;
        if let Some(prepared) = sub2api {
            let prepared = remote_keys::prepare_sub2api_remote_key_scan_v2(
                self.providers.as_ref(),
                &self.outbound,
                prepared,
                tokio_util::sync::CancellationToken::new(),
                current_correlation_id(),
            )
            .await?;
            return remote_keys::preview_remote_key_scan_v2(prepared).map_err(Into::into);
        }
        Err(RemoteKeyOperationError::Unsupported.into())
    }

    fn source(&self, draft_id: String) -> ProviderDraftCollectorSource {
        ProviderDraftCollectorSource {
            drafts: Arc::clone(&self.drafts),
            settings: Arc::clone(&self.settings),
            draft_id,
        }
    }

    async fn snapshot_source(
        &self,
        draft_id: String,
    ) -> Result<DraftSnapshotCollectorSource, ApplicationError> {
        let station = self.drafts.station_projection(&draft_id).await?;
        let settings = self.settings.load().await.map_err(app_error);
        let keys = self.drafts.list_keys(&draft_id).await.map_err(app_error);
        let mut key_secrets = BTreeMap::new();
        if let Ok(keys) = &keys {
            for key in keys.iter().filter(|key| key.api_key_present) {
                key_secrets.insert(
                    key.id.clone(),
                    self.drafts
                        .key_secret(&draft_id, &key.id)
                        .await
                        .map_err(app_error),
                );
            }
        }
        let credentials = self
            .drafts
            .credentials_projection(&draft_id)
            .await
            .map_err(app_error);
        let login_password = self
            .drafts
            .login_password(&draft_id)
            .await
            .map_err(app_error);
        let session = self
            .drafts
            .resolve_session(&draft_id)
            .await
            .map_err(app_error);
        let groups = self.drafts.list_groups(&draft_id).await.map_err(app_error);
        Ok(DraftSnapshotCollectorSource {
            station,
            settings,
            keys,
            key_secrets,
            credentials,
            login_password,
            session,
            groups,
        })
    }
}

struct DraftSnapshotCollectorSource {
    station: Station,
    settings: Result<AppSettings, String>,
    keys: Result<Vec<StationKey>, String>,
    key_secrets: BTreeMap<String, Result<String, String>>,
    credentials: Result<StationCredentials, String>,
    login_password: Result<Option<String>, String>,
    session: Result<ResolvedSession, String>,
    groups: Result<Vec<StationGroupBinding>, String>,
}

impl CollectorSourcePort for DraftSnapshotCollectorSource {
    fn station_for_collector(&self, _station_id: &str) -> Result<Station, String> {
        Ok(self.station.clone())
    }

    fn get_settings(&self) -> Result<AppSettings, String> {
        self.settings.clone()
    }

    fn list_station_keys(&self, _station_id: String) -> Result<Vec<StationKey>, String> {
        self.keys.clone()
    }

    fn resolve_station_key_secret_with_data_key(
        &self,
        _data_key: &[u8; 32],
        station_key_id: &str,
    ) -> Result<String, String> {
        self.key_secrets
            .get(station_key_id)
            .cloned()
            .unwrap_or_else(|| Err("station key secret is not available".to_string()))
    }

    fn get_station_credentials(&self, _station_id: String) -> Result<StationCredentials, String> {
        self.credentials.clone()
    }

    fn get_station_login_password_with_data_key(
        &self,
        _station_id: String,
        _data_key: &[u8; 32],
    ) -> Result<Option<String>, String> {
        self.login_password.clone()
    }

    fn resolve_station_session_with_data_key(
        &self,
        _station_id: String,
        _data_key: &[u8; 32],
        _now_ms: i64,
    ) -> Result<ResolvedSession, String> {
        self.session.clone()
    }

    fn update_station_session_with_data_key(
        &self,
        _input: UpdateStationSessionInput,
        _data_key: &[u8; 32],
        _expected_revision: i64,
    ) -> Result<StationCredentials, String> {
        Err("draft snapshot source is read-only".to_string())
    }

    fn persist_station_session<'a>(
        &'a self,
        _input: PersistStationSessionInput,
        _expected_revision: i64,
    ) -> futures_util::future::BoxFuture<'a, Result<StationCredentials, String>> {
        Box::pin(async { Err("draft snapshot source is read-only".to_string()) })
    }

    fn invalidate_station_session_credential(
        &self,
        _station_id: &str,
        _kind: StationSessionCredentialKind,
    ) -> Result<(), String> {
        Err("draft snapshot source is read-only".to_string())
    }

    fn list_station_group_bindings(
        &self,
        _station_id: String,
    ) -> Result<Vec<StationGroupBinding>, String> {
        self.groups.clone()
    }
}

#[derive(Clone)]
struct ProviderDraftCollectorSource {
    drafts: Arc<ProviderDraftService>,
    settings: Arc<SettingsService>,
    draft_id: String,
}

impl CollectorSourcePort for ProviderDraftCollectorSource {
    fn station_for_collector(&self, station_id: &str) -> Result<Station, String> {
        tauri::async_runtime::block_on(self.drafts.station_projection(station_id))
            .map_err(app_error)
    }

    fn get_settings(&self) -> Result<AppSettings, String> {
        tauri::async_runtime::block_on(self.settings.load()).map_err(app_error)
    }

    fn list_station_keys(&self, station_id: String) -> Result<Vec<StationKey>, String> {
        tauri::async_runtime::block_on(self.drafts.list_keys(&station_id)).map_err(app_error)
    }

    fn resolve_station_key_secret_with_data_key(
        &self,
        _data_key: &[u8; 32],
        station_key_id: &str,
    ) -> Result<String, String> {
        tauri::async_runtime::block_on(self.drafts.key_secret(&self.draft_id, station_key_id))
            .map_err(app_error)
    }

    fn get_station_credentials(&self, station_id: String) -> Result<StationCredentials, String> {
        tauri::async_runtime::block_on(self.drafts.credentials_projection(&station_id))
            .map_err(app_error)
    }

    fn get_station_login_password_with_data_key(
        &self,
        station_id: String,
        _data_key: &[u8; 32],
    ) -> Result<Option<String>, String> {
        tauri::async_runtime::block_on(self.drafts.login_password(&station_id)).map_err(app_error)
    }

    fn resolve_station_session_with_data_key(
        &self,
        station_id: String,
        _data_key: &[u8; 32],
        _now_ms: i64,
    ) -> Result<ResolvedSession, String> {
        tauri::async_runtime::block_on(self.drafts.resolve_session(&station_id)).map_err(app_error)
    }

    fn update_station_session_with_data_key(
        &self,
        input: UpdateStationSessionInput,
        _data_key: &[u8; 32],
        expected_revision: i64,
    ) -> Result<StationCredentials, String> {
        tauri::async_runtime::block_on(self.drafts.update_session(input, expected_revision))
            .map_err(app_error)
    }

    fn persist_station_session<'a>(
        &'a self,
        input: PersistStationSessionInput,
        expected_revision: i64,
    ) -> futures_util::future::BoxFuture<'a, Result<StationCredentials, String>> {
        let drafts = Arc::clone(&self.drafts);
        Box::pin(async move {
            drafts
                .persist_session(input, expected_revision)
                .await
                .map_err(app_error)
        })
    }

    fn invalidate_station_session_credential(
        &self,
        station_id: &str,
        kind: StationSessionCredentialKind,
    ) -> Result<(), String> {
        tauri::async_runtime::block_on(self.drafts.invalidate_session_credential(station_id, kind))
            .map_err(app_error)
    }

    fn list_station_group_bindings(
        &self,
        station_id: String,
    ) -> Result<Vec<StationGroupBinding>, String> {
        tauri::async_runtime::block_on(self.drafts.list_groups(&station_id)).map_err(app_error)
    }
}

fn current_correlation_id() -> Option<String> {
    correlation::current().map(|id| id.as_str().to_string())
}

fn app_error(error: ApplicationError) -> String {
    error.to_string()
}
