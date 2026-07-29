use std::sync::Arc;

use crate::{
    application::{
        collectors::CollectorService, credentials::CredentialService, error::ApplicationError,
        settings::SettingsService,
    },
    background_tasks::{BlockingExecutor, BlockingExecutorError},
    models::collector::CollectorRunResult,
    observability::correlation,
    outbound::AsyncOutboundClient,
    services::collectors::{self, output::CollectorTask, V2CollectorSourceAdapter},
};

#[derive(Debug)]
pub(crate) enum StationCollectionCommandError {
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
}

impl StationCollectionCommandFacade {
    pub(crate) fn new(
        collectors: Arc<CollectorService>,
        credentials: Arc<CredentialService>,
        settings: Arc<SettingsService>,
        blocking: BlockingExecutor,
        outbound: AsyncOutboundClient,
        providers: Arc<collectors::orchestration::ProviderRegistry>,
    ) -> Self {
        Self {
            collectors,
            credentials,
            settings,
            blocking,
            outbound,
            providers,
        }
    }

    pub(crate) async fn run_station_collection(
        &self,
        station_id: String,
        task: CollectorTask,
    ) -> Result<CollectorRunResult, StationCollectionCommandError> {
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
            collectors::PreparedStationCollectionRoute::OpenAiCompatible(prepared) => {
                collectors::finish_openai_compatible_collection_v2(
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
                collectors::finish_newapi_collection_v2(
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
        self.apply_prepared_collection(prepared).await
    }

    pub(crate) async fn test_station_login(
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

fn current_correlation_id() -> Option<String> {
    correlation::current().map(|id| id.as_str().to_string())
}
