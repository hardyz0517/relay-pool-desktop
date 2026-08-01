use std::sync::Arc;

use tokio::runtime::Handle;

use crate::{
    application::{
        app_services::AppServices,
        command_facades::{
            CaptureCommandFacade, ChangeEventsCommandFacade, ChannelMonitoringCommandFacade,
            ChannelStatusCommandFacade, CollectorMetadataCommandFacade, CredentialsCommandFacade,
            DataDirectoryCommandFacade, KeyPoolCommandFacade, LocalProxyCommandFacade,
            PricingCommandFacade, ProviderDraftCommandFacade, RemoteKeysCommandFacade,
            RequestLogsCommandFacade, RoutingCommandFacade, SettingsStationsCommandFacade,
            StationCollectionCommandFacade, StationKeyConnectivityCommandFacade,
        },
        data_directory::DataDirectoryPort,
    },
    background_tasks::{
        BlockingExecutor, BlockingExecutorConfig, OperationRegistry, OperationRegistryConfig,
        TaskSupervisor,
    },
    outbound::{AsyncOutboundClient, AsyncOutboundClientConfig},
    persistence::runtime::PersistenceHandle,
    runtime_composition::{RuntimeCompositionError, WorkRuntimeBundle},
    services::{
        collectors::{
            drivers::{static_provider_entries, REQUIRED_PROVIDER_KINDS},
            orchestration::{ProviderRegistry, ProviderRegistryError},
        },
        monitoring::runner::MonitoringRunner,
        pricing_catalog::StaticBuiltinModelBasePriceCatalog,
        proxy::runtime::ProxyRuntimeState,
        secrets::vault::DataKeyVault,
    },
    TrayBehaviorState,
};

pub(crate) type ManagedWorkRuntime =
    WorkRuntimeBundle<TaskSupervisor, BlockingExecutor, AsyncOutboundClient, OperationRegistry>;

#[derive(Clone, Debug)]
pub(crate) struct WorkRuntimeConfig {
    pub blocking: BlockingExecutorConfig,
    pub outbound: AsyncOutboundClientConfig,
    pub operation: OperationRegistryConfig,
}

impl WorkRuntimeConfig {
    pub(crate) fn architecture_budget() -> Self {
        Self {
            blocking: BlockingExecutorConfig::architecture_budget(),
            outbound: AsyncOutboundClientConfig::architecture_budget(),
            operation: OperationRegistryConfig::architecture_budget(),
        }
    }
}

pub(crate) fn compose_work_runtime(
    config: WorkRuntimeConfig,
    spawn_handle: Handle,
) -> Result<ManagedWorkRuntime, RuntimeCompositionError> {
    validate_work_runtime_config(&config)?;
    Ok(WorkRuntimeBundle::new(
        TaskSupervisor::with_spawn_handle(spawn_handle),
        BlockingExecutor::new(config.blocking),
        AsyncOutboundClient::new(config.outbound),
        OperationRegistry::new(config.operation),
    ))
}

fn validate_work_runtime_config(config: &WorkRuntimeConfig) -> Result<(), RuntimeCompositionError> {
    if config.blocking.max_running == 0
        || config.blocking.queue_capacity == 0
        || config.blocking.queue_timeout.is_zero()
        || config.blocking.default_execution_timeout.is_zero()
        || config.outbound.max_attempts == 0
        || config.outbound.success_body_max_bytes == 0
        || config.outbound.error_body_max_bytes == 0
        || config.outbound.timeouts.connect_timeout.is_zero()
        || config.outbound.timeouts.first_byte_timeout.is_zero()
        || config.outbound.timeouts.body_read_timeout.is_zero()
        || config.outbound.timeouts.total_timeout.is_zero()
        || config.operation.max_running_global == 0
        || config.operation.max_running_per_concurrency_key == 0
        || config.operation.progress_ring_entries_per_operation == 0
        || config.operation.progress_entry_max_bytes == 0
        || config.operation.terminal_max_entries == 0
        || config.operation.default_deadline.is_zero()
    {
        return Err(RuntimeCompositionError::WorkRuntimeConfiguration);
    }
    Ok(())
}

pub(crate) fn compose_provider_registry() -> Result<ProviderRegistry, ProviderRegistryError> {
    ProviderRegistry::new(static_provider_entries(), REQUIRED_PROVIDER_KINDS)
}

pub(crate) fn compose_app_services(
    runtime: PersistenceHandle,
    data_key: [u8; 32],
    data_dir: String,
    pending_data_dir: Option<String>,
    data_directory_port: Arc<dyn DataDirectoryPort>,
    blocking: BlockingExecutor,
) -> AppServices {
    AppServices::for_runtime(
        runtime,
        data_dir,
        pending_data_dir,
        data_directory_port,
        blocking,
        Arc::new(DataKeyVault::new(data_key)),
        Arc::new(StaticBuiltinModelBasePriceCatalog),
    )
}

pub(crate) fn compose_settings_stations_command_facade(
    services: &AppServices,
    tray_behavior: Arc<TrayBehaviorState>,
) -> SettingsStationsCommandFacade {
    SettingsStationsCommandFacade::new(
        Arc::clone(&services.stations),
        Arc::clone(&services.settings),
        tray_behavior,
    )
}

pub(crate) fn compose_key_pool_command_facade(services: &AppServices) -> KeyPoolCommandFacade {
    KeyPoolCommandFacade::new(Arc::clone(&services.credentials))
}

pub(crate) fn compose_remote_keys_command_facade(
    services: &AppServices,
    blocking: BlockingExecutor,
    outbound: AsyncOutboundClient,
    providers: Arc<ProviderRegistry>,
    data_key: [u8; 32],
) -> RemoteKeysCommandFacade {
    RemoteKeysCommandFacade::new(
        Arc::clone(&services.collectors),
        Arc::clone(&services.credentials),
        Arc::clone(&services.settings),
        blocking,
        outbound,
        providers,
        data_key,
    )
}

pub(crate) fn compose_routing_command_facade(
    services: &AppServices,
    outbound: AsyncOutboundClient,
) -> RoutingCommandFacade {
    RoutingCommandFacade::new(
        Arc::clone(&services.routing),
        Arc::clone(&services.request_logs),
        outbound,
    )
}

pub(crate) fn compose_request_logs_command_facade(
    services: &AppServices,
) -> RequestLogsCommandFacade {
    RequestLogsCommandFacade::new(Arc::clone(&services.request_logs))
}

pub(crate) fn compose_channel_monitoring_command_facade(
    services: &AppServices,
    runner: Arc<MonitoringRunner>,
) -> ChannelMonitoringCommandFacade {
    ChannelMonitoringCommandFacade::new(Arc::clone(&services.monitoring), runner)
}

pub(crate) fn compose_channel_status_command_facade(
    services: &AppServices,
) -> ChannelStatusCommandFacade {
    ChannelStatusCommandFacade::new(Arc::clone(&services.channel_status))
}

pub(crate) fn compose_collector_metadata_command_facade(
    services: &AppServices,
) -> CollectorMetadataCommandFacade {
    CollectorMetadataCommandFacade::new(Arc::clone(&services.collectors))
}

pub(crate) fn compose_station_collection_command_facade(
    services: &AppServices,
    blocking: BlockingExecutor,
    outbound: AsyncOutboundClient,
    providers: Arc<ProviderRegistry>,
    data_key: [u8; 32],
) -> StationCollectionCommandFacade {
    StationCollectionCommandFacade::new(
        Arc::clone(&services.collectors),
        Arc::clone(&services.credentials),
        Arc::clone(&services.settings),
        blocking,
        outbound,
        providers,
        data_key,
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        app_composition::{compose_provider_registry, compose_work_runtime, WorkRuntimeConfig},
        background_tasks::{BlockingExecutorConfig, OperationRegistryConfig, TaskId},
        outbound::{AsyncOutboundClientConfig, OutboundHeaderPolicy, TimeoutPolicy},
        runtime_composition::RuntimeCompositionError,
        services::collectors::{contract::ProviderKind, failure::DriverFailureKind},
    };

    #[test]
    fn work_runtime_composition_uses_architecture_budgets() {
        let tokio_runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let runtime = compose_work_runtime(
            WorkRuntimeConfig::architecture_budget(),
            tokio_runtime.handle().clone(),
        )
        .expect("work runtime");

        assert_eq!(runtime.blocking.metrics().queued, 0);
        assert_eq!(runtime.outbound.metrics().pool_size, 0);
        assert_eq!(runtime.operation.metrics().running, 0);
        assert!(runtime.supervisor.status(&TaskId::from("missing")).is_err());
    }

    #[test]
    fn work_runtime_composition_rejects_invalid_dependency_budget_before_construction() {
        let config = WorkRuntimeConfig {
            blocking: BlockingExecutorConfig {
                max_running: 0,
                queue_capacity: 16,
                queue_timeout: Duration::from_millis(2_000),
                default_execution_timeout: Duration::from_millis(30_000),
            },
            outbound: AsyncOutboundClientConfig {
                timeouts: TimeoutPolicy::provider_default(),
                header_policy: OutboundHeaderPolicy::provider_default(),
                success_body_max_bytes: 8_388_608,
                error_body_max_bytes: 65_536,
                max_attempts: 2,
                redirect_max_hops: 5,
                https_downgrade_allowed: false,
            },
            operation: OperationRegistryConfig::architecture_budget(),
        };

        let tokio_runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let error = match compose_work_runtime(config, tokio_runtime.handle().clone()) {
            Ok(_) => panic!("invalid budget must fail closed"),
            Err(error) => error,
        };
        assert_eq!(error, RuntimeCompositionError::WorkRuntimeConfiguration);
    }

    #[test]
    fn work_runtime_composition_rejects_invalid_operation_budget_before_construction() {
        let config = WorkRuntimeConfig {
            blocking: BlockingExecutorConfig::architecture_budget(),
            outbound: AsyncOutboundClientConfig::architecture_budget(),
            operation: OperationRegistryConfig {
                max_running_global: 0,
                ..OperationRegistryConfig::architecture_budget()
            },
        };

        let tokio_runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let error = match compose_work_runtime(config, tokio_runtime.handle().clone()) {
            Ok(_) => panic!("invalid operation budget must fail closed"),
            Err(error) => error,
        };
        assert_eq!(error, RuntimeCompositionError::WorkRuntimeConfiguration);
    }

    #[test]
    fn provider_registry_composition_registers_every_known_provider() {
        let registry = compose_provider_registry().expect("provider registry");

        assert_eq!(registry.len(), 3);
        assert_eq!(
            registry
                .descriptor(ProviderKind::Sub2Api)
                .expect("sub2api descriptor")
                .display_name,
            "Sub2API"
        );
        assert_eq!(
            registry
                .descriptor(ProviderKind::NewApi)
                .expect("newapi descriptor")
                .display_name,
            "NewAPI"
        );
        assert_eq!(
            registry
                .descriptor(ProviderKind::OpenAiCompatible)
                .expect("openai-compatible descriptor")
                .display_name,
            "OpenAI-compatible"
        );
    }

    #[test]
    fn provider_registry_composition_registers_openai_reference_collector_only() {
        let registry = compose_provider_registry().expect("provider registry");

        assert!(registry.collector(ProviderKind::OpenAiCompatible).is_ok());
        let failure = match registry.remote_key(ProviderKind::OpenAiCompatible) {
            Ok(_) => panic!("OpenAI-compatible has no remote-key capability"),
            Err(failure) => failure,
        };

        assert_eq!(failure.kind, DriverFailureKind::Unsupported);
    }
}

pub(crate) fn compose_station_key_connectivity_command_facade(
    services: &AppServices,
) -> StationKeyConnectivityCommandFacade {
    StationKeyConnectivityCommandFacade::new(
        Arc::clone(&services.credentials),
        Arc::clone(&services.routing),
    )
}

pub(crate) fn compose_capture_command_facade(
    services: &AppServices,
    sessions: crate::services::capture::session::CaptureSessionStore,
    outbound: AsyncOutboundClient,
    providers: Arc<ProviderRegistry>,
) -> CaptureCommandFacade {
    CaptureCommandFacade::new(
        Arc::clone(&services.stations),
        Arc::clone(&services.credentials),
        Arc::clone(&services.provider_drafts),
        Arc::clone(&services.collectors),
        sessions,
        outbound,
        providers,
    )
}

pub(crate) fn compose_pricing_command_facade(services: &AppServices) -> PricingCommandFacade {
    PricingCommandFacade::new(
        Arc::clone(&services.pricing),
        Arc::clone(&services.pricing_comparison),
    )
}

pub(crate) fn compose_provider_draft_command_facade(
    services: &AppServices,
    blocking: BlockingExecutor,
    outbound: AsyncOutboundClient,
    providers: Arc<ProviderRegistry>,
    data_key: [u8; 32],
) -> ProviderDraftCommandFacade {
    ProviderDraftCommandFacade::new(
        Arc::clone(&services.provider_drafts),
        Arc::clone(&services.settings),
        blocking,
        outbound,
        providers,
        data_key,
    )
}

pub(crate) fn compose_change_events_command_facade(
    services: &AppServices,
) -> ChangeEventsCommandFacade {
    ChangeEventsCommandFacade::new(Arc::clone(&services.changes))
}

pub(crate) fn compose_credentials_command_facade(
    services: &AppServices,
) -> CredentialsCommandFacade {
    CredentialsCommandFacade::new(Arc::clone(&services.credentials))
}

pub(crate) fn compose_data_directory_command_facade(
    services: &AppServices,
    blocking: BlockingExecutor,
) -> DataDirectoryCommandFacade {
    DataDirectoryCommandFacade::new(
        Arc::clone(&services.data_directory),
        Arc::clone(&services.settings),
        blocking,
    )
}

pub(crate) fn compose_local_proxy_command_facade(
    services: &AppServices,
    proxy: Arc<ProxyRuntimeState>,
) -> LocalProxyCommandFacade {
    LocalProxyCommandFacade::new(
        Arc::clone(&services.settings),
        Arc::clone(&services.routing),
        Arc::clone(&services.credentials),
        Arc::clone(&services.request_logs),
        Arc::clone(&services.request_finalization),
        proxy,
    )
}
