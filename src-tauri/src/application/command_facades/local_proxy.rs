use std::sync::Arc;

use crate::{
    application::{
        credentials::CredentialService, error::ApplicationError,
        request_finalization::RequestFinalizationService,
        request_lifecycle::ports::LifecycleWriteError, routing::RoutingService,
        routing_execution_reader::RoutingExecutionReader,
        routing_policy_read::RoutingPolicyReadService, settings::SettingsService,
    },
    models::proxy::ProxyStatus,
    services::proxy::{
        lifecycle::ports::RequestLifecycleStore,
        limits::ProxyStartupResourceLimits,
        routing_repository::{RoutingExecutionRepository, RoutingRepository},
        runtime::{ProxyRuntimeState, ProxyStartConfig},
        transport_policy::TransportPolicySnapshot,
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

pub(crate) struct CcswitchImportProxyTarget {
    pub(crate) local_access_key: String,
    pub(crate) proxy_status: ProxyStatus,
}

#[derive(Clone)]
pub(crate) struct LocalProxyCommandFacade {
    settings: Arc<SettingsService>,
    routing: Arc<RoutingService>,
    routing_policy_read: Arc<RoutingPolicyReadService>,
    credentials: Arc<CredentialService>,
    request_finalization: Arc<RequestFinalizationService>,
    proxy: Arc<ProxyRuntimeState>,
}

impl LocalProxyCommandFacade {
    pub(crate) fn new(
        settings: Arc<SettingsService>,
        routing: Arc<RoutingService>,
        routing_policy_read: Arc<RoutingPolicyReadService>,
        credentials: Arc<CredentialService>,
        request_finalization: Arc<RequestFinalizationService>,
        proxy: Arc<ProxyRuntimeState>,
    ) -> Self {
        Self {
            settings,
            routing,
            routing_policy_read,
            credentials,
            request_finalization,
            proxy,
        }
    }

    pub(crate) async fn get_proxy_status(&self) -> Result<ProxyStatus, ApplicationError> {
        let settings = self.settings.load().await?;
        Ok(self.proxy.status(settings.local_proxy_port))
    }

    pub(crate) async fn start_local_proxy(&self) -> Result<ProxyStatus, LocalProxyCommandError> {
        let (settings, _local_access_key, config) = self.proxy_start_config().await?;
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
        let (settings, _local_access_key, config) = self.proxy_start_config().await?;
        let status = self.proxy.restart(config).await?;
        if let Err(error) = self.settings.set_local_proxy_start_on_launch(true).await {
            let _ = self.proxy.stop(status.port).await;
            return Err(error.into());
        }
        Ok(self.proxy.status(settings.local_proxy_port))
    }

    pub(crate) async fn import_relay_pool_to_ccswitch(
        &self,
    ) -> Result<CcswitchImportProxyTarget, LocalProxyCommandError> {
        let (_settings, local_access_key, config) = self.proxy_start_config().await?;
        let proxy_status = self.proxy.start(config).await?;
        Ok(CcswitchImportProxyTarget {
            local_access_key,
            proxy_status,
        })
    }

    async fn proxy_start_config(
        &self,
    ) -> Result<
        (
            crate::models::settings::AppSettings,
            String,
            ProxyStartConfig,
        ),
        LocalProxyCommandError,
    > {
        let settings = self.settings.load().await?;
        let local_access_key = self.settings.ensure_local_access_key().await?;
        self.request_finalization
            .reconcile_startup_interrupted_request_lifecycle()
            .await
            .map_err(local_proxy_startup_reconciliation_error)?;
        let execution_reader = Arc::new(RoutingExecutionReader::new(self.routing.clone()));
        let routing_repository: Arc<dyn RoutingRepository> =
            Arc::new(RoutingExecutionRepository::new(execution_reader));
        let stored_policy = self.routing_policy_read.load_routing_policy().await?;
        let policy = crate::models::routing_policy::RoutingPolicyConfigV2::from_stored_value(
            &stored_policy.config,
        )
        .map_err(|_| ApplicationError::ConstraintViolation)?;
        let lifecycle_store: Arc<dyn RequestLifecycleStore> = self.request_finalization.clone();
        let transport_policy = TransportPolicySnapshot::from_timeout_policy(
            &policy.timeout_policy,
            stored_policy.revision,
            ProxyStartupResourceLimits::default().upstream_pool_idle_timeout,
        )
        .map_err(|_| ApplicationError::ConstraintViolation)?;
        let config = ProxyStartConfig::new_v2(
            routing_repository,
            self.credentials.clone(),
            lifecycle_store,
            local_access_key.clone(),
            settings.local_proxy_port,
        )
        .with_transport_policy(transport_policy);
        Ok((settings, local_access_key, config))
    }
}

fn local_proxy_startup_reconciliation_error(error: LifecycleWriteError) -> LocalProxyCommandError {
    match error {
        LifecycleWriteError::DatabaseBusy => {
            LocalProxyCommandError::Application(ApplicationError::Unavailable)
        }
        LifecycleWriteError::Unavailable(_) => {
            LocalProxyCommandError::Application(ApplicationError::Unavailable)
        }
        LifecycleWriteError::CommitOutcomeUnknown(_) => {
            LocalProxyCommandError::Application(ApplicationError::CommitOutcomeUnknown)
        }
    }
}
