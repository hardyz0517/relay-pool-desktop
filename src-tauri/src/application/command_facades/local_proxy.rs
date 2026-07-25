use std::sync::Arc;

use crate::{
    application::{
        error::ApplicationError, request_finalization::RequestFinalizationService,
        request_logs::RequestLogService, routing::RoutingService, settings::SettingsService,
    },
    models::proxy::ProxyStatus,
    services::proxy::{
        lifecycle::ports::RequestLifecycleStore,
        routing_repository::{RoutingRepository, V2RoutingRepository},
        routing_types::LocalRoutingWorkspace,
        runtime::{ProxyRuntimeState, ProxyStartConfig},
    },
};

#[derive(Debug)]
pub(crate) enum LocalProxyCommandError {
    Application(ApplicationError),
    Runtime,
}

impl From<ApplicationError> for LocalProxyCommandError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

impl From<String> for LocalProxyCommandError {
    fn from(_error: String) -> Self {
        Self::Runtime
    }
}

#[derive(Clone)]
pub(crate) struct LocalProxyCommandFacade {
    settings: Arc<SettingsService>,
    routing: Arc<RoutingService>,
    request_logs: Arc<RequestLogService>,
    request_finalization: Arc<RequestFinalizationService>,
    proxy: Arc<ProxyRuntimeState>,
    data_key: [u8; 32],
}

impl LocalProxyCommandFacade {
    pub(crate) fn new(
        settings: Arc<SettingsService>,
        routing: Arc<RoutingService>,
        request_logs: Arc<RequestLogService>,
        request_finalization: Arc<RequestFinalizationService>,
        proxy: Arc<ProxyRuntimeState>,
        data_key: [u8; 32],
    ) -> Self {
        Self {
            settings,
            routing,
            request_logs,
            request_finalization,
            proxy,
            data_key,
        }
    }

    pub(crate) async fn get_proxy_status(&self) -> Result<ProxyStatus, ApplicationError> {
        let settings = self.settings.load().await?;
        Ok(self.proxy.status(settings.local_proxy_port))
    }

    pub(crate) async fn load_local_routing_workspace(
        &self,
    ) -> Result<LocalRoutingWorkspace, ApplicationError> {
        self.load_workspace().await
    }

    pub(crate) async fn reorder_local_routing_keys(
        &self,
        station_key_ids: Vec<String>,
    ) -> Result<LocalRoutingWorkspace, ApplicationError> {
        self.routing
            .reorder_local_routing_keys(station_key_ids)
            .await?;
        self.load_workspace().await
    }

    pub(crate) async fn start_local_proxy(&self) -> Result<ProxyStatus, LocalProxyCommandError> {
        let (settings, config) = self.proxy_start_config().await?;
        let status = self.proxy.start(config).await?;
        if let Err(error) = self.settings.set_local_proxy_start_on_launch(true).await {
            let _ = self.proxy.stop(status.port).await;
            return Err(error.into());
        }
        Ok(self.proxy.status(settings.local_proxy_port))
    }

    pub(crate) async fn stop_local_proxy(&self) -> Result<ProxyStatus, LocalProxyCommandError> {
        let settings = self.settings.load().await?;
        let status = self.proxy.stop(settings.local_proxy_port).await?;
        self.settings.set_local_proxy_start_on_launch(false).await?;
        Ok(status)
    }

    pub(crate) async fn cleanup_before_update(
        &self,
    ) -> Result<ProxyStatus, LocalProxyCommandError> {
        let settings = self.settings.load().await?;
        self.proxy
            .cleanup_before_update(settings.local_proxy_port)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn restart_local_proxy(&self) -> Result<ProxyStatus, LocalProxyCommandError> {
        let (settings, config) = self.proxy_start_config().await?;
        let status = self.proxy.restart(config).await?;
        if let Err(error) = self.settings.set_local_proxy_start_on_launch(true).await {
            let _ = self.proxy.stop(status.port).await;
            return Err(error.into());
        }
        Ok(self.proxy.status(settings.local_proxy_port))
    }

    async fn load_workspace(&self) -> Result<LocalRoutingWorkspace, ApplicationError> {
        let settings = self.settings.load().await?;
        let request_logs = self
            .request_logs
            .list_recent(
                crate::application::pagination::PageLimit::new(500).expect("bounded limit"),
            )
            .await?;
        let proxy_status = self.proxy.status(settings.local_proxy_port);
        self.routing
            .load_local_routing_workspace(settings, request_logs, proxy_status)
            .await
    }

    async fn proxy_start_config(
        &self,
    ) -> Result<(crate::models::settings::AppSettings, ProxyStartConfig), ApplicationError> {
        let settings = self.settings.load().await?;
        let local_access_key = self.settings.ensure_local_access_key().await?;
        let routing_repository: Arc<dyn RoutingRepository> = Arc::new(V2RoutingRepository::new(
            self.routing.as_ref().clone(),
            self.data_key,
        ));
        let lifecycle_store: Arc<dyn RequestLifecycleStore> = self.request_finalization.clone();
        let config = ProxyStartConfig::new_v2(
            routing_repository,
            lifecycle_store,
            local_access_key,
            settings.local_proxy_port,
        );
        Ok((settings, config))
    }
}
