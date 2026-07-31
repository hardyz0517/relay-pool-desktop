use std::sync::Arc;

use crate::{
    application::{
        error::ApplicationError, provider_drafts::ProviderDraftService, settings::SettingsService,
    },
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
    Remote(RemoteKeyOperationError),
}

impl From<ApplicationError> for ProviderDraftCommandError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
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
    outbound: AsyncOutboundClient,
    providers: Arc<ProviderRegistry>,
}

impl ProviderDraftCommandFacade {
    pub(crate) fn new(
        drafts: Arc<ProviderDraftService>,
        settings: Arc<SettingsService>,
        outbound: AsyncOutboundClient,
        providers: Arc<ProviderRegistry>,
    ) -> Self {
        Self {
            drafts,
            settings,
            outbound,
            providers,
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
        let source = self.source(draft_id.clone());
        let prepared = collectors::prepare_station_collection_route_v2(&source, draft_id, task)?;
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
        let source = self.source(draft_id.clone());
        let newapi =
            remote_keys::prepare_newapi_remote_key_driver_context_v2(&source, draft_id.clone())?;
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

        let source = self.source(draft_id.clone());
        let sub2api =
            remote_keys::prepare_sub2api_remote_key_driver_context_v2(&source, draft_id.clone())?;
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
}

#[derive(Clone)]
struct ProviderDraftCollectorSource {
    drafts: Arc<ProviderDraftService>,
    settings: Arc<SettingsService>,
    draft_id: String,
}

impl CollectorSourcePort for ProviderDraftCollectorSource {
    fn station_for_collector(&self, station_id: &str) -> Result<Station, String> {
        block_on_draft_source(self.drafts.station_projection(station_id)).map_err(app_error)
    }

    fn get_settings(&self) -> Result<AppSettings, String> {
        block_on_draft_source(self.settings.load()).map_err(app_error)
    }

    fn list_station_keys(&self, station_id: String) -> Result<Vec<StationKey>, String> {
        block_on_draft_source(self.drafts.list_keys(&station_id)).map_err(app_error)
    }

    fn resolve_station_key_secret(&self, station_key_id: &str) -> Result<String, String> {
        block_on_draft_source(self.drafts.key_secret(&self.draft_id, station_key_id))
            .map_err(app_error)
    }

    fn get_station_credentials(&self, station_id: String) -> Result<StationCredentials, String> {
        block_on_draft_source(self.drafts.credentials_projection(&station_id)).map_err(app_error)
    }

    fn get_station_login_password(&self, station_id: String) -> Result<Option<String>, String> {
        block_on_draft_source(self.drafts.login_password(&station_id)).map_err(app_error)
    }

    fn resolve_station_session(
        &self,
        station_id: String,
        _now_ms: i64,
    ) -> Result<ResolvedSession, String> {
        block_on_draft_source(self.drafts.resolve_session(&station_id)).map_err(app_error)
    }

    fn update_station_session(
        &self,
        input: UpdateStationSessionInput,
        expected_revision: i64,
    ) -> Result<StationCredentials, String> {
        block_on_draft_source(self.drafts.update_session(input, expected_revision))
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
        block_on_draft_source(self.drafts.invalidate_session_credential(station_id, kind))
            .map_err(app_error)
    }

    fn list_station_group_bindings(
        &self,
        station_id: String,
    ) -> Result<Vec<StationGroupBinding>, String> {
        block_on_draft_source(self.drafts.list_groups(&station_id)).map_err(app_error)
    }
}

fn block_on_draft_source<F: std::future::Future>(future: F) -> F::Output {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
}

fn current_correlation_id() -> Option<String> {
    correlation::current().map(|id| id.as_str().to_string())
}

fn app_error(error: ApplicationError) -> String {
    error.to_string()
}
