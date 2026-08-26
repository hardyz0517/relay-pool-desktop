use serde::Serialize;
#[cfg(test)]
use sha2::{Digest, Sha256};

#[cfg(test)]
use super::dto::REGISTERED_TYPES;
#[cfg(test)]
use super::runtime_contract::RUNTIME_CONTRACT_TYPESCRIPT;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-registry-document; owner=ipc; remove_when=registry document is exported in production binding generation"
    )
)]
pub const GENERATOR_VERSION: u32 = 1;
pub const IPC_CONTRACT_VERSION: u32 = 1;
// Updated by `pnpm generate:bindings` whenever the compiled command/type contract changes.
pub const IPC_BINDING_HASH: &str =
    "eb586692e080dee356c9ed5a1388d279bb35df884049ac8a1f57bf5682f6bc89";

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-registry-document; owner=ipc; remove_when=registry document is exported in production binding generation"
    )
)]
#[derive(Debug, Clone, Copy)]
pub struct CommandDescriptor {
    pub name: &'static str,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-registry-document; owner=ipc; remove_when=registry document is exported in production binding generation"
    )
)]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    Ordinary,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-registry-document; owner=ipc; remove_when=registry document is exported in production binding generation"
    )
)]
#[derive(Debug, Clone, Copy, Serialize)]
pub struct StreamingSurface {
    pub command: &'static str,
    pub event: &'static str,
    pub event_schema_version: u32,
    pub transport: TransportKind,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-registry-document; owner=ipc; remove_when=registry document is exported in production binding generation"
    )
)]
pub const STREAMING_SURFACES: &[StreamingSurface] = &[];

#[macro_export]
macro_rules! ipc_command_registry {
    ($consumer:ident) => {
        $consumer! {
            app_status => $crate::commands::runtime::app_status,
            get_runtime_contract_info => $crate::commands::runtime::get_runtime_contract_info,
            get_runtime_status => $crate::commands::runtime::get_runtime_status,
            restart_application => $crate::commands::runtime::restart_application,
            initialize_runtime_context => $crate::commands::runtime_context::initialize_runtime_context,
            read_runtime_diagnostics => $crate::commands::runtime_diagnostics::read_runtime_diagnostics,
            export_runtime_support_bundle => $crate::commands::runtime_diagnostics::export_runtime_support_bundle,
            open_runtime_log_directory => $crate::commands::runtime_diagnostics::open_runtime_log_directory,
            open_runtime_log_file => $crate::commands::runtime_diagnostics::open_runtime_log_file,
            record_frontend_boundary_failure => $crate::commands::runtime_diagnostics::record_frontend_boundary_failure,
            get_data_store_startup_state => $crate::commands::data_store_startup::get_data_store_startup_state,
            refresh_data_store_candidates => $crate::commands::data_store_startup::refresh_data_store_candidates,
            locate_data_store_candidate => $crate::commands::data_store_startup::locate_data_store_candidate,
            activate_data_store_candidate => $crate::commands::data_store_startup::activate_data_store_candidate,
            create_new_data_store => $crate::commands::data_store_startup::create_new_data_store,
            open_data_store_backup_dir => $crate::commands::data_store_startup::open_data_store_backup_dir,
            export_data_store_diagnostic => $crate::commands::data_store_startup::export_data_store_diagnostic,
            get_portable_migration_capability => $crate::commands::data_migration::get_portable_migration_capability,
            choose_portable_export_path => $crate::commands::data_migration::choose_portable_export_path,
            start_portable_export => $crate::commands::data_migration::start_portable_export,
            get_portable_export_result => $crate::commands::data_migration::get_portable_export_result,
            choose_portable_import_file => $crate::commands::data_migration::choose_portable_import_file,
            start_portable_import_inspection => $crate::commands::data_migration::start_portable_import_inspection,
            get_portable_import_inspection => $crate::commands::data_migration::get_portable_import_inspection,
            start_portable_import_prepare => $crate::commands::data_migration::start_portable_import_prepare,
            get_portable_import_prepare_result => $crate::commands::data_migration::get_portable_import_prepare_result,
            get_portable_migration_operation => $crate::commands::data_migration::get_portable_migration_operation,
            get_portable_import_recovery_state => $crate::commands::data_migration::get_portable_import_recovery_state,
            list_stations => $crate::commands::stations::list_stations,
            create_station => $crate::commands::stations::create_station,
            update_station => $crate::commands::stations::update_station,
            delete_station => $crate::commands::stations::delete_station,
            reorder_stations => $crate::commands::stations::reorder_stations,
            get_station_capacity_domain => $crate::commands::stations::get_station_capacity_domain,
            upsert_station_capacity_domain => $crate::commands::stations::upsert_station_capacity_domain,
            clear_station_capacity_domain => $crate::commands::stations::clear_station_capacity_domain,
            create_or_resume_provider_draft => $crate::commands::provider_drafts::create_or_resume_provider_draft,
            get_provider_draft => $crate::commands::provider_drafts::get_provider_draft,
            patch_provider_draft => $crate::commands::provider_drafts::patch_provider_draft,
            discard_provider_draft => $crate::commands::provider_drafts::discard_provider_draft,
            collect_provider_draft_preview => $crate::commands::provider_drafts::collect_provider_draft_preview,
            scan_provider_draft_remote_keys => $crate::commands::provider_drafts::scan_provider_draft_remote_keys,
            commit_provider_draft => $crate::commands::provider_drafts::commit_provider_draft,
            get_settings => $crate::commands::settings::get_settings,
            get_local_access_key => $crate::commands::settings::get_local_access_key,
            update_local_access_key => $crate::commands::settings::update_local_access_key,
            import_relay_pool_to_ccswitch => $crate::commands::ccswitch_import::import_relay_pool_to_ccswitch,
            open_external_url => $crate::commands::settings::open_external_url,
            updater_network_config => $crate::commands::updater::updater_network_config,
            inspect_latest_update_manifest => $crate::commands::updater::inspect_latest_update_manifest,
            update_settings => $crate::commands::settings::update_settings,
            choose_data_dir => $crate::commands::data_directory::choose_data_dir,
            reset_data_dir => $crate::commands::data_directory::reset_data_dir,
            get_proxy_status => $crate::commands::local_proxy::get_proxy_status,
            start_local_proxy => $crate::commands::local_proxy::start_local_proxy,
            stop_local_proxy => $crate::commands::local_proxy::stop_local_proxy,
            cleanup_before_update => $crate::commands::local_proxy::cleanup_before_update,
            prepare_local_proxy_for_update => $crate::commands::local_proxy::prepare_local_proxy_for_update,
            restart_local_proxy => $crate::commands::local_proxy::restart_local_proxy,
            list_request_logs => $crate::commands::request_logs::list_request_logs,
            clear_request_logs => $crate::commands::request_logs::clear_request_logs,
            load_dashboard_live_request_metrics => $crate::commands::dashboard::load_dashboard_live_request_metrics,
            load_dashboard_cumulative_request_metrics => $crate::commands::dashboard::load_dashboard_cumulative_request_metrics,
            list_station_keys => $crate::commands::key_pool::list_station_keys,
            create_station_key => $crate::commands::key_pool::create_station_key,
            update_station_key => $crate::commands::key_pool::update_station_key,
            save_station_key_with_defaults => $crate::commands::key_pool::save_station_key_with_defaults,
            update_station_key_group_binding => $crate::commands::key_pool::update_station_key_group_binding,
            delete_station_key => $crate::commands::key_pool::delete_station_key,
            reorder_station_keys => $crate::commands::key_pool::reorder_station_keys,
            get_remote_key_capability => $crate::commands::key_pool::get_remote_key_capability,
            list_remote_station_keys => $crate::commands::key_pool::list_remote_station_keys,
            scan_remote_station_keys => $crate::commands::key_pool::scan_remote_station_keys,
            create_remote_station_key => $crate::commands::key_pool::create_remote_station_key,
            create_local_station_key_from_remote => $crate::commands::key_pool::create_local_station_key_from_remote,
            delete_remote_station_key => $crate::commands::key_pool::delete_remote_station_key,
            bind_remote_station_key => $crate::commands::key_pool::bind_remote_station_key,
            unbind_remote_station_key => $crate::commands::key_pool::unbind_remote_station_key,
            list_key_pool_items => $crate::commands::key_pool::list_key_pool_items,
            reorder_key_pool => $crate::commands::key_pool::reorder_key_pool,
            get_station_key_capabilities => $crate::commands::key_pool::get_station_key_capabilities,
            update_station_key_capabilities => $crate::commands::key_pool::update_station_key_capabilities,
            list_model_aliases => $crate::commands::model_aliases::list_model_aliases,
            upsert_model_alias => $crate::commands::model_aliases::upsert_model_alias,
            delete_model_alias => $crate::commands::model_aliases::delete_model_alias,
            get_model_mapping_workspace => $crate::commands::model_mapping::get_model_mapping_workspace,
            get_model_mapping_document => $crate::commands::model_mapping::get_model_mapping_document,
            validate_model_mapping_document => $crate::commands::model_mapping::validate_model_mapping_document,
            apply_model_mapping_document => $crate::commands::model_mapping::apply_model_mapping_document,
            restore_model_mapping_revision => $crate::commands::model_mapping::restore_model_mapping_revision,
            simulate_model_mapping => $crate::commands::model_mapping::simulate_model_mapping,
            resolve_request_mapping_trace => $crate::commands::model_mapping::resolve_request_mapping_trace,
            list_station_key_health => $crate::commands::routing_health::list_station_key_health,
            get_routing_protection_status => $crate::commands::routing_health::get_routing_protection_status,
            load_routing_policy => $crate::commands::routing_health::load_routing_policy,
            apply_routing_policy_document => $crate::commands::routing_health::apply_routing_policy_document,
            list_station_endpoint_health => $crate::commands::routing_health::list_station_endpoint_health,
            load_routing_workspace_snapshot => $crate::commands::routing_health::load_routing_workspace_snapshot,
            load_routing_runtime_overlay => $crate::commands::routing_health::load_routing_runtime_overlay,
            list_recent_route_decisions => $crate::commands::routing_health::list_recent_route_decisions,
            list_error_rate_history => $crate::commands::routing_health::list_error_rate_history,
            get_station_key_operational_detail => $crate::commands::routing_health::get_station_key_operational_detail,
            get_request_decision_trace => $crate::commands::routing_health::get_request_decision_trace,
            list_channel_monitors => $crate::commands::channel_monitoring::list_channel_monitors,
            load_channel_status_workspace => $crate::commands::channel_status::load_channel_status_workspace,
            list_channel_monitor_executions => $crate::commands::channel_status::list_channel_monitor_executions,
            get_channel_monitor_execution => $crate::commands::channel_status::get_channel_monitor_execution,
            list_channel_monitor_attempts => $crate::commands::channel_status::list_channel_monitor_attempts,
            list_monitoring_capabilities => $crate::commands::channel_status::list_monitoring_capabilities,
            get_station_published_status_workspace => $crate::commands::station_published_status::get_station_published_status_workspace,
            load_pricing_comparison_workspace => $crate::commands::pricing_workspace::load_pricing_comparison_workspace,
            load_pricing_group_monitor_status => $crate::commands::pricing_workspace::load_pricing_group_monitor_status,
            create_channel_monitor => $crate::commands::channel_monitoring::create_channel_monitor,
            update_channel_monitor => $crate::commands::channel_monitoring::update_channel_monitor,
            delete_channel_monitor => $crate::commands::channel_monitoring::delete_channel_monitor,
            list_channel_monitor_templates => $crate::commands::channel_monitoring::list_channel_monitor_templates,
            create_channel_monitor_template => $crate::commands::channel_monitoring::create_channel_monitor_template,
            update_channel_monitor_template => $crate::commands::channel_monitoring::update_channel_monitor_template,
            duplicate_channel_monitor_template => $crate::commands::channel_monitoring::duplicate_channel_monitor_template,
            delete_channel_monitor_template => $crate::commands::channel_monitoring::delete_channel_monitor_template,
            run_channel_monitor_now => $crate::commands::channel_monitoring::run_channel_monitor_now,
            cancel_channel_monitor_execution => $crate::commands::channel_monitoring::cancel_channel_monitor_execution,
            get_station_key_health => $crate::commands::routing_health::get_station_key_health,
            get_operation_status => $crate::commands::operations::get_operation_status,
            cancel_operation => $crate::commands::operations::cancel_operation,
            start_station_key_connectivity_operation => $crate::commands::station_key_connectivity::start_station_key_connectivity_operation,
            get_station_key_connectivity_operation_result => $crate::commands::station_key_connectivity::get_station_key_connectivity_operation_result,
            start_station_key_model_discovery_operation => $crate::commands::station_key_connectivity::start_station_key_model_discovery_operation,
            get_station_key_model_discovery_operation_result => $crate::commands::station_key_connectivity::get_station_key_model_discovery_operation_result,
            ping_station_endpoint => $crate::commands::endpoint_ping::ping_station_endpoint,
            simulate_route => $crate::commands::routing_health::simulate_route,
            list_model_base_prices => $crate::commands::pricing::list_model_base_prices,
            list_model_price_sync_catalog => $crate::commands::pricing::list_model_price_sync_catalog,
            upsert_model_base_price => $crate::commands::pricing::upsert_model_base_price,
            delete_model_base_price => $crate::commands::pricing::delete_model_base_price,
            reset_model_base_prices_to_builtins => $crate::commands::pricing::reset_model_base_prices_to_builtins,
            get_model_price_sync_state => $crate::commands::pricing::get_model_price_sync_state,
            save_model_price_sync_config => $crate::commands::pricing::save_model_price_sync_config,
            sync_model_prices => $crate::commands::pricing::sync_model_prices,
            reload_model_price_catalog => $crate::commands::pricing::reload_model_price_catalog,
            open_model_price_catalog_directory => $crate::commands::pricing::open_model_price_catalog_directory,
            resolve_station_key_pricing_context => $crate::commands::pricing::resolve_station_key_pricing_context,
            list_balance_snapshots => $crate::commands::pricing::list_balance_snapshots,
            list_current_station_balance_snapshots => $crate::commands::pricing::list_current_station_balance_snapshots,
            list_balance_snapshots_for_station => $crate::commands::pricing::list_balance_snapshots_for_station,
            upsert_balance_snapshot => $crate::commands::pricing::upsert_balance_snapshot,
            list_station_group_bindings => $crate::commands::collector_metadata::list_station_group_bindings,
            list_station_group_options => $crate::commands::collector_metadata::list_station_group_options,
            upsert_station_group_binding => $crate::commands::collector_metadata::upsert_station_group_binding,
            list_group_rate_records => $crate::commands::collector_metadata::list_group_rate_records,
            list_collector_runs => $crate::commands::collector_metadata::list_collector_runs,
            list_alerting_activity => $crate::commands::alerting::list_alerting_activity,
            list_alerting_incidents => $crate::commands::alerting::list_alerting_incidents,
            get_alerting_incident => $crate::commands::alerting::get_alerting_incident,
            list_alerting_occurrences => $crate::commands::alerting::list_alerting_occurrences,
            list_alerting_deliveries => $crate::commands::alerting::list_alerting_deliveries,
            list_alert_policies => $crate::commands::alerting::list_alert_policies,
            upsert_alert_policy => $crate::commands::alerting::upsert_alert_policy,
            delete_alert_policy => $crate::commands::alerting::delete_alert_policy,
            get_alerting_settings => $crate::commands::alerting::get_alerting_settings,
            update_alerting_settings => $crate::commands::alerting::update_alerting_settings,
            record_alerting_observation => $crate::commands::alerting::record_alerting_observation,
            mark_alerting_seen => $crate::commands::alerting::mark_alerting_seen,
            mark_all_alerting_seen => $crate::commands::alerting::mark_all_alerting_seen,
            resolve_all_alerting_incidents => $crate::commands::alerting::resolve_all_alerting_incidents,
            clear_alerting_incidents => $crate::commands::alerting::clear_alerting_incidents,
            snooze_alerting_incident => $crate::commands::alerting::snooze_alerting_incident,
            test_alerting_notification => $crate::commands::alerting::test_alerting_notification,
            request_desktop_notification_permission => $crate::commands::alerting::request_desktop_notification_permission,
            get_desktop_notification_permission => $crate::commands::alerting::get_desktop_notification_permission,
            get_station_credentials => $crate::commands::credentials::get_station_credentials,
            update_station_credentials => $crate::commands::credentials::update_station_credentials,
            update_station_session => $crate::commands::credentials::update_station_session,
            clear_station_credentials => $crate::commands::credentials::clear_station_credentials,
            list_common_login_options => $crate::commands::credentials::list_common_login_options,
            upsert_common_login_email => $crate::commands::credentials::upsert_common_login_email,
            delete_common_login_email => $crate::commands::credentials::delete_common_login_email,
            upsert_common_login_password => $crate::commands::credentials::upsert_common_login_password,
            delete_common_login_password => $crate::commands::credentials::delete_common_login_password,
            get_common_login_password => $crate::commands::credentials::get_common_login_password,
            detect_station_info => $crate::commands::station_collection::detect_station_info,
            collect_station_info => $crate::commands::station_collection::collect_station_info,
            collect_station_task => $crate::commands::station_collection::collect_station_task,
            scan_station_recharge => $crate::commands::station_collection::scan_station_recharge,
            test_station_login => $crate::commands::station_collection::test_station_login,
            test_station_login_input => $crate::commands::station_collection::test_station_login_input,
            detect_sub2api_station => $crate::commands::station_collection::detect_sub2api_station,
            collect_sub2api_station => $crate::commands::station_collection::collect_sub2api_station,
            list_collector_snapshots => $crate::commands::collector_metadata::list_collector_snapshots,
            get_latest_collector_snapshot => $crate::commands::collector_metadata::get_latest_collector_snapshot,
            list_latest_collector_snapshots => $crate::commands::collector_metadata::list_latest_collector_snapshots,
            start_capture_session => $crate::commands::capture::start_capture_session,
            start_provider_draft_authorization => $crate::commands::capture::start_provider_draft_authorization,
            get_capture_session_status => $crate::commands::capture::get_capture_session_status,
            record_capture_event => $crate::commands::capture::record_capture_event,
            finish_capture_session => $crate::commands::capture::finish_capture_session,
            finish_web_authorization_session => $crate::commands::capture::finish_web_authorization_session,
            finish_provider_draft_authorization_session => $crate::commands::capture::finish_provider_draft_authorization_session,
            clear_capture_session => $crate::commands::capture::clear_capture_session,
            close_capture_session => $crate::commands::capture::close_capture_session,
        }
    };
}

macro_rules! compile_descriptors {
    ($( $name:ident => $handler:path, )*) => {
        #[cfg_attr(not(test), expect(dead_code, reason = "contract=ipc-registry-document; owner=ipc; remove_when=registry document is exported in production binding generation"))]
        pub const COMMANDS: &[CommandDescriptor] = &[
            $(CommandDescriptor { name: stringify!($name) },)*
        ];
    };
}

crate::ipc_command_registry!(compile_descriptors);

#[cfg(test)]
#[derive(Serialize)]
struct RegistryDocument<'a> {
    schema_version: u32,
    generator_version: u32,
    ipc_contract_version: u32,
    contract_hash: &'a str,
    commands: Vec<RegistryCommand<'a>>,
    streaming_surfaces: &'static [StreamingSurface],
    evidence: RegistryEvidence<'a>,
}

#[cfg(test)]
#[derive(Serialize)]
struct RegistryCommand<'a> {
    name: &'a str,
    transport: TransportKind,
    input_schema_hash: String,
    output_schema_hash: String,
    error_schema_hash: String,
    mutation_kind: &'static str,
    transport_retry: bool,
    result_unknown: bool,
    runtime_validation: &'static str,
}

#[cfg(test)]
#[derive(Serialize)]
struct RegistryEvidence<'a> {
    kind: &'static str,
    serialization_fixture_hash: &'a str,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
struct CommandContract {
    input: &'static str,
    output: &'static str,
    error: &'static str,
    mutation_kind: &'static str,
    transport_retry: bool,
    result_unknown: bool,
    runtime_validation: &'static str,
}

#[cfg(test)]
fn command_contract(name: &str) -> CommandContract {
    match name {
        "app_status" => migrated_read("EmptyInputDto", "AppStatusDto"),
        "restart_application" => migrated_mutation("EmptyInputDto", "unit", "non_idempotent", true),
        "get_runtime_status" => migrated_read("EmptyInputDto", "RuntimeStatusDto"),
        "read_runtime_diagnostics" => CommandContract {
            runtime_validation: "developer_mode_gate",
            ..migrated_read("RuntimeDiagnosticsQueryDto", "RuntimeDiagnosticsPageDto")
        },
        "export_runtime_support_bundle" => CommandContract {
            runtime_validation: "developer_mode_gate",
            ..migrated_mutation(
                "EmptyInputDto",
                "Option<RuntimeSupportBundleResultDto>",
                "non_idempotent",
                true,
            )
        },
        "open_runtime_log_directory" | "open_runtime_log_file" => {
            migrated_mutation("EmptyInputDto", "unit", "idempotent", false)
        }
        "record_frontend_boundary_failure" => CommandContract {
            runtime_validation: "rust_dto_pre_application",
            ..migrated_mutation("EmptyInputDto", "unit", "idempotent", false)
        },
        "get_settings" => migrated_read("EmptyInputDto", "SettingsDto"),
        "get_local_access_key" => migrated_read("EmptyInputDto", "String"),
        "update_local_access_key" => migrated_mutation(
            "UpdateLocalAccessKeyInputDto",
            "SettingsDto",
            "idempotent",
            false,
        ),
        "import_relay_pool_to_ccswitch" => migrated_mutation(
            "EmptyInputDto",
            "CcswitchImportResultDto",
            "non_idempotent",
            true,
        ),
        "open_external_url" => {
            migrated_mutation("OpenExternalUrlInputDto", "unit", "non_idempotent", true)
        }
        "list_stations" => migrated_read("EmptyInputDto", "Vec<StationDto>"),
        "update_settings" => {
            migrated_mutation("UpdateSettingsInputDto", "SettingsDto", "idempotent", false)
        }
        "create_station" => migrated_mutation(
            "CreateStationInputDto",
            "StationDto",
            "non_idempotent",
            true,
        ),
        "update_station" => {
            migrated_mutation("UpdateStationInputDto", "StationDto", "idempotent", false)
        }
        "delete_station" => migrated_mutation("DeleteStationInputDto", "unit", "idempotent", false),
        "reorder_stations" => migrated_mutation(
            "ReorderStationsInputDto",
            "Vec<StationDto>",
            "idempotent",
            false,
        ),
        "get_station_capacity_domain" => migrated_read(
            "StationCapacityDomainQueryInputDto",
            "Option<StationCapacityDomainDto>",
        ),
        "upsert_station_capacity_domain" => migrated_mutation(
            "UpsertStationCapacityDomainInputDto",
            "StationCapacityDomainDto",
            "idempotent",
            false,
        ),
        "clear_station_capacity_domain" => migrated_mutation(
            "ClearStationCapacityDomainInputDto",
            "()",
            "idempotent",
            false,
        ),
        "list_station_keys" => migrated_read("StationIdInputDto", "Vec<StationKeyDto>"),
        "get_remote_key_capability" => migrated_read("StationIdInputDto", "RemoteKeyCapabilityDto"),
        "list_remote_station_keys" => {
            migrated_read("StationIdInputDto", "Vec<RemoteStationKeyDto>")
        }
        "scan_remote_station_keys" => migrated_mutation(
            "StationIdInputDto",
            "RemoteKeyScanResultDto",
            "idempotent",
            false,
        ),
        "list_key_pool_items" => migrated_read("EmptyInputDto", "Vec<KeyPoolItemDto>"),
        "get_station_credentials" => migrated_read("StationIdInputDto", "StationCredentialsDto"),
        "list_common_login_options" => migrated_read("EmptyInputDto", "CommonLoginOptionsDto"),
        "get_common_login_password" => migrated_read("CommonLoginIdInputDto", "String"),
        "upsert_common_login_email" => migrated_mutation(
            "UpsertCommonLoginEmailInputDto",
            "CommonLoginEmailDto",
            "non_idempotent",
            true,
        ),
        "upsert_common_login_password" => migrated_mutation(
            "UpsertCommonLoginPasswordInputDto",
            "CommonLoginPasswordDto",
            "non_idempotent",
            true,
        ),
        "create_or_resume_provider_draft" => migrated_mutation(
            "CreateProviderDraftInputDto",
            "ProviderDraftDto",
            "idempotent",
            false,
        ),
        "get_provider_draft" => migrated_read("ProviderDraftIdInputDto", "ProviderDraftDto"),
        "patch_provider_draft" => migrated_mutation(
            "PatchProviderDraftInputDto",
            "ProviderDraftDto",
            "idempotent",
            false,
        ),
        "discard_provider_draft" => {
            migrated_mutation("ProviderDraftIdInputDto", "unit", "idempotent", false)
        }
        "collect_provider_draft_preview" => migrated_mutation(
            "CollectProviderDraftPreviewInputDto",
            "ProviderDraftPreviewDto",
            "idempotent",
            false,
        ),
        "scan_provider_draft_remote_keys" => migrated_mutation(
            "ProviderDraftIdInputDto",
            "RemoteKeyScanResultDto",
            "idempotent",
            false,
        ),
        "commit_provider_draft" => migrated_mutation(
            "CommitProviderDraftInputDto",
            "StationDto",
            "idempotent",
            false,
        ),
        "delete_common_login_email" | "delete_common_login_password" => {
            migrated_mutation("CommonLoginIdInputDto", "unit", "idempotent", false)
        }
        "create_station_key" => migrated_mutation(
            "CreateStationKeyInputDto",
            "StationKeyDto",
            "non_idempotent",
            true,
        ),
        "update_station_key" => migrated_mutation(
            "UpdateStationKeyInputDto",
            "StationKeyDto",
            "idempotent",
            false,
        ),
        "save_station_key_with_defaults" => migrated_mutation(
            "SaveStationKeyWithDefaultsInputDto",
            "SaveStationKeyWithDefaultsResultDto",
            "non_idempotent",
            true,
        ),
        "update_station_key_group_binding" => migrated_mutation(
            "UpdateStationKeyGroupBindingInputDto",
            "StationKeyDto",
            "idempotent",
            false,
        ),
        "delete_station_key" => {
            migrated_mutation("StationKeyIdInputDto", "unit", "idempotent", false)
        }
        "reorder_station_keys" => migrated_mutation(
            "ReorderStationKeysInputDto",
            "Vec<StationKeyDto>",
            "idempotent",
            false,
        ),
        "create_remote_station_key" => migrated_mutation(
            "CreateRemoteStationKeyInputDto",
            "CreateRemoteStationKeyResultDto",
            "non_idempotent",
            true,
        ),
        "create_local_station_key_from_remote" => migrated_mutation(
            "RemoteStationKeyInputDto",
            "CreateLocalStationKeyFromRemoteResultDto",
            "non_idempotent",
            true,
        ),
        "delete_remote_station_key" => migrated_mutation(
            "RemoteStationKeyInputDto",
            "DeleteRemoteStationKeyResultDto",
            "idempotent",
            false,
        ),
        "bind_remote_station_key" => migrated_mutation(
            "BindRemoteStationKeyInputDto",
            "Vec<RemoteStationKeyDto>",
            "idempotent",
            false,
        ),
        "unbind_remote_station_key" => migrated_mutation(
            "RemoteStationKeyInputDto",
            "Vec<RemoteStationKeyDto>",
            "idempotent",
            false,
        ),
        "reorder_key_pool" => migrated_mutation(
            "ReorderKeyPoolInputDto",
            "Vec<KeyPoolItemDto>",
            "idempotent",
            false,
        ),
        "update_station_credentials" => migrated_mutation(
            "UpdateStationCredentialsInputDto",
            "StationCredentialsDto",
            "idempotent",
            false,
        ),
        "update_station_session" => migrated_mutation(
            "UpdateStationSessionInputDto",
            "StationCredentialsDto",
            "idempotent",
            false,
        ),
        "clear_station_credentials" => migrated_mutation(
            "StationIdInputDto",
            "StationCredentialsDto",
            "idempotent",
            false,
        ),
        "list_request_logs" => migrated_read("EmptyInputDto", "Vec<RequestLogDto>"),
        "clear_request_logs" => migrated_mutation("EmptyInputDto", "unit", "idempotent", false),
        "load_dashboard_live_request_metrics" => migrated_read(
            "DashboardRequestMetricsInputDto",
            "DashboardLiveRequestMetricsSnapshotDto",
        ),
        "load_dashboard_cumulative_request_metrics" => migrated_read(
            "EmptyInputDto",
            "DashboardCumulativeRequestMetricsSnapshotDto",
        ),
        "list_alerting_activity" => {
            migrated_read("AlertingActivityInputDto", "AlertingActivityPageDto")
        }
        "list_alerting_incidents" => {
            migrated_read("AlertingCurrentInputDto", "AlertingIncidentPageDto")
        }
        "get_alerting_incident" => {
            migrated_read("AlertingIncidentInputDto", "AlertingIncidentSummaryDto")
        }
        "list_alerting_occurrences" => {
            migrated_read("AlertingHistoryInputDto", "AlertingOccurrencePageDto")
        }
        "list_alerting_deliveries" => {
            migrated_read("AlertingHistoryInputDto", "AlertingDeliveryPageDto")
        }
        "list_alert_policies" => migrated_read("EmptyInputDto", "Vec<AlertPolicyDto>"),
        "upsert_alert_policy" => {
            migrated_mutation("AlertPolicyInputDto", "AlertPolicyDto", "idempotent", false)
        }
        "delete_alert_policy" => {
            migrated_mutation("AlertPolicyDeleteInputDto", "unit", "idempotent", false)
        }
        "get_alerting_settings" => migrated_read("EmptyInputDto", "AlertingSettingsDto"),
        "update_alerting_settings" => migrated_mutation(
            "AlertingSettingsInputDto",
            "AlertingSettingsDto",
            "idempotent",
            false,
        ),
        "record_alerting_observation" => {
            migrated_mutation("AlertingObservationInputDto", "bool", "idempotent", false)
        }
        "mark_alerting_seen" => {
            migrated_mutation("AlertingMarkSeenInputDto", "unit", "idempotent", false)
        }
        "mark_all_alerting_seen" => {
            migrated_mutation("AlertingMarkAllSeenInputDto", "u64", "idempotent", false)
        }
        "resolve_all_alerting_incidents" => {
            migrated_mutation("AlertingMarkAllSeenInputDto", "u64", "idempotent", false)
        }
        "clear_alerting_incidents" => {
            migrated_mutation("AlertingClearInputDto", "u64", "idempotent", false)
        }
        "snooze_alerting_incident" => {
            migrated_mutation("AlertingSnoozeInputDto", "unit", "idempotent", false)
        }
        "test_alerting_notification" => migrated_mutation(
            "AlertingNotificationTestInputDto",
            "unit",
            "idempotent",
            false,
        ),
        "request_desktop_notification_permission" => {
            migrated_mutation("EmptyInputDto", "string", "idempotent", false)
        }
        "get_desktop_notification_permission" => migrated_read("EmptyInputDto", "string"),
        "list_balance_snapshots" | "list_current_station_balance_snapshots" => {
            migrated_read("EmptyInputDto", "Vec<BalanceSnapshotDto>")
        }
        "list_balance_snapshots_for_station" => {
            migrated_read("CollectorStationIdInputDto", "Vec<BalanceSnapshotDto>")
        }
        "upsert_balance_snapshot" => migrated_mutation(
            "UpsertBalanceSnapshotInputDto",
            "BalanceSnapshotDto",
            "idempotent",
            false,
        ),
        "list_station_group_bindings" => {
            migrated_read("CollectorStationIdInputDto", "Vec<StationGroupBindingDto>")
        }
        "list_station_group_options" => {
            migrated_read("CollectorStationIdInputDto", "Vec<StationGroupOptionDto>")
        }
        "upsert_station_group_binding" => migrated_mutation(
            "UpsertStationGroupBindingInputDto",
            "StationGroupBindingDto",
            "idempotent",
            false,
        ),
        "list_group_rate_records" => {
            migrated_read("CollectorStationIdInputDto", "Vec<GroupRateRecordDto>")
        }
        "list_collector_runs" => {
            migrated_read("CollectorStationIdInputDto", "Vec<CollectorRunDto>")
        }
        "list_collector_snapshots" => {
            migrated_read("CollectorStationIdInputDto", "Vec<CollectorSnapshotDto>")
        }
        "get_latest_collector_snapshot" => {
            migrated_read("CollectorStationIdInputDto", "Option<CollectorSnapshotDto>")
        }
        "list_latest_collector_snapshots" => {
            migrated_read("CollectorStationIdsInputDto", "Vec<CollectorSnapshotDto>")
        }
        "list_channel_monitors" => migrated_read("EmptyInputDto", "Vec<ChannelMonitorDto>"),
        "list_channel_monitor_templates" => {
            migrated_read("EmptyInputDto", "Vec<ChannelMonitorRequestTemplateDto>")
        }
        "create_channel_monitor" => migrated_mutation(
            "CreateChannelMonitorInputDto",
            "ChannelMonitorDto",
            "non_idempotent",
            true,
        ),
        "update_channel_monitor" => migrated_mutation(
            "UpdateChannelMonitorInputDto",
            "ChannelMonitorDto",
            "idempotent",
            false,
        ),
        "delete_channel_monitor" => migrated_mutation(
            "ChannelMonitorMutationIdInputDto",
            "unit",
            "idempotent",
            false,
        ),
        "create_channel_monitor_template" => migrated_mutation(
            "CreateChannelMonitorTemplateInputDto",
            "ChannelMonitorRequestTemplateDto",
            "non_idempotent",
            true,
        ),
        "update_channel_monitor_template" => migrated_mutation(
            "UpdateChannelMonitorTemplateInputDto",
            "ChannelMonitorRequestTemplateDto",
            "idempotent",
            false,
        ),
        "duplicate_channel_monitor_template" => migrated_mutation(
            "ChannelMonitorMutationIdInputDto",
            "ChannelMonitorRequestTemplateDto",
            "non_idempotent",
            true,
        ),
        "delete_channel_monitor_template" => migrated_mutation(
            "ChannelMonitorMutationIdInputDto",
            "unit",
            "idempotent",
            false,
        ),
        "load_channel_status_workspace" => migrated_read(
            "ChannelStatusWorkspaceInputDto",
            "ChannelStatusWorkspaceDto",
        ),
        "get_station_published_status_workspace" => migrated_read(
            "StationPublishedStatusWorkspaceInputDto",
            "StationPublishedStatusWorkspaceDto",
        ),
        "list_channel_monitor_executions" => migrated_read(
            "ChannelMonitorExecutionListInputDto",
            "ChannelMonitorExecutionPageDto",
        ),
        "get_channel_monitor_execution" => migrated_read(
            "ChannelMonitorExecutionIdInputDto",
            "ChannelMonitorExecutionDetailDto",
        ),
        "list_channel_monitor_attempts" => migrated_read(
            "ChannelMonitorAttemptHistoryInputDto",
            "ChannelMonitorAttemptPageDto",
        ),
        "list_monitoring_capabilities" => {
            migrated_read("EmptyInputDto", "MonitoringCapabilityCatalogDto")
        }
        "run_channel_monitor_now" => migrated_mutation(
            "RunChannelMonitorNowInputDto",
            "RunChannelMonitorReceiptDto",
            "non_idempotent",
            true,
        ),
        "cancel_channel_monitor_execution" => migrated_mutation(
            "CancelChannelMonitorExecutionInputDto",
            "CancelChannelMonitorExecutionReceiptDto",
            "idempotent",
            false,
        ),
        "detect_sub2api_station"
        | "collect_sub2api_station"
        | "detect_station_info"
        | "collect_station_info"
        | "scan_station_recharge"
        | "test_station_login" => migrated_mutation(
            "CollectorStationIdInputDto",
            "CollectorRunResultDto",
            "non_idempotent",
            true,
        ),
        "collect_station_task" => migrated_mutation(
            "StationCollectorTaskInputDto",
            "CollectorRunResultDto",
            "non_idempotent",
            true,
        ),
        "test_station_login_input" => {
            migrated_read("StationLoginTestInputDto", "StationLoginTestResultDto")
        }
        "start_capture_session" => migrated_mutation(
            "CaptureStationIdInputDto",
            "CaptureSessionStatusDto",
            "non_idempotent",
            true,
        ),
        "start_provider_draft_authorization" => migrated_mutation(
            "ProviderDraftIdInputDto",
            "CaptureSessionStatusDto",
            "non_idempotent",
            true,
        ),
        "finish_capture_session" | "finish_web_authorization_session" => migrated_mutation(
            "CaptureStationIdInputDto",
            "CollectorRunResultDto",
            "non_idempotent",
            true,
        ),
        "finish_provider_draft_authorization_session" => migrated_mutation(
            "ProviderDraftIdInputDto",
            "ProviderDraftPreviewDto",
            "non_idempotent",
            true,
        ),
        "get_capture_session_status" => {
            migrated_read("CaptureStationIdInputDto", "CaptureSessionStatusDto")
        }
        "record_capture_event" => migrated_mutation(
            "CapturedHttpEventInputDto",
            "CaptureSessionStatusDto",
            "non_idempotent",
            true,
        ),
        "clear_capture_session" | "close_capture_session" => migrated_mutation(
            "CaptureStationIdInputDto",
            "CaptureSessionStatusDto",
            "idempotent",
            false,
        ),
        "get_station_key_capabilities" => {
            migrated_read("RoutingStationKeyIdInputDto", "StationKeyCapabilitiesDto")
        }
        "list_model_aliases" => migrated_read("EmptyInputDto", "Vec<ModelAliasDto>"),
        "update_station_key_capabilities" => migrated_mutation(
            "UpdateStationKeyCapabilitiesInputDto",
            "StationKeyCapabilitiesDto",
            "idempotent",
            false,
        ),
        "upsert_model_alias" => migrated_mutation(
            "UpsertModelAliasInputDto",
            "ModelAliasDto",
            "idempotent",
            false,
        ),
        "delete_model_alias" => {
            migrated_mutation("DeleteModelAliasInputDto", "unit", "idempotent", false)
        }
        "get_model_mapping_workspace" => migrated_read("EmptyInputDto", "ModelMappingWorkspaceDto"),
        "get_model_mapping_document" => migrated_read("EmptyInputDto", "ModelMappingDocumentDto"),
        "validate_model_mapping_document" => migrated_read(
            "ValidateModelMappingDocumentInputDto",
            "ModelMappingValidationResultDto",
        ),
        "apply_model_mapping_document" => migrated_mutation(
            "ApplyModelMappingDocumentInputDto",
            "ModelMappingWorkspaceDto",
            "idempotent",
            false,
        ),
        "restore_model_mapping_revision" => migrated_mutation(
            "RestoreModelMappingRevisionInputDto",
            "ModelMappingWorkspaceDto",
            "idempotent",
            false,
        ),
        "simulate_model_mapping" => migrated_read(
            "SimulateModelMappingInputDto",
            "ModelMappingSimulationResultDto",
        ),
        "resolve_request_mapping_trace" => {
            migrated_read("ResolveRequestMappingTraceInputDto", "ModelMappingTraceDto")
        }
        "list_station_key_health" => migrated_read("EmptyInputDto", "Vec<StationKeyHealthDto>"),
        "get_routing_protection_status" => migrated_read(
            "RoutingProtectionStatusInputDto",
            "RoutingProtectionStatusDto",
        ),
        "load_routing_policy" => migrated_read("EmptyInputDto", "RoutingPolicySnapshotDto"),
        "apply_routing_policy_document" => migrated_mutation(
            "ApplyRoutingPolicyDocumentInputDto",
            "RoutingPolicySnapshotDto",
            "idempotent",
            false,
        ),
        "list_station_endpoint_health" => {
            migrated_read("EmptyInputDto", "Vec<StationEndpointHealthDto>")
        }
        "load_routing_workspace_snapshot" => migrated_read(
            "RoutingWorkspaceSnapshotInputDto",
            "RoutingWorkspaceSnapshotDto",
        ),
        "load_routing_runtime_overlay" => {
            migrated_read("EmptyInputDto", "RoutingRuntimeOverlayDto")
        }
        "list_recent_route_decisions" => migrated_read(
            "RecentRouteDecisionsInputDto",
            "RecentRouteDecisionsPageDto",
        ),
        "list_error_rate_history" => {
            migrated_read("ErrorRateHistoryInputDto", "ErrorRateHistoryPageDto")
        }
        "get_station_key_operational_detail" => migrated_read(
            "StationKeyOperationalDetailInputDto",
            "StationKeyOperationalDetailDto",
        ),
        "get_request_decision_trace" => {
            migrated_read("RequestDecisionTraceInputDto", "RequestDecisionTraceDto")
        }
        "get_station_key_health" => {
            migrated_read("RoutingStationKeyIdInputDto", "StationKeyHealthDto")
        }
        "get_operation_status" => migrated_read("OperationIdInputDto", "OperationSnapshotDto"),
        "cancel_operation" => migrated_mutation(
            "CancelOperationInputDto",
            "CancelOperationOutcomeDto",
            "idempotent",
            false,
        ),
        "start_station_key_connectivity_operation" => migrated_mutation(
            "StationKeyConnectivityInputDto",
            "OperationStartedDto",
            "non_idempotent",
            true,
        ),
        "get_station_key_connectivity_operation_result" => {
            migrated_read("OperationIdInputDto", "StationKeyConnectivityResultDto")
        }
        "start_station_key_model_discovery_operation" => migrated_mutation(
            "RoutingStationKeyIdInputDto",
            "OperationStartedDto",
            "non_idempotent",
            true,
        ),
        "get_station_key_model_discovery_operation_result" => {
            migrated_read("OperationIdInputDto", "StationKeyModelDiscoveryResultDto")
        }
        "ping_station_endpoint" => migrated_mutation(
            "StationIdInputDto",
            "EndpointPingResultDto",
            "non_idempotent",
            true,
        ),
        "simulate_route" => migrated_read("RouteSimulationInputDto", "RouteSimulationResultDto"),
        "list_model_base_prices" => migrated_read("EmptyInputDto", "Vec<ModelBasePriceDto>"),
        "list_model_price_sync_catalog" => {
            migrated_read("EmptyInputDto", "Vec<ModelPriceCatalogEntryDto>")
        }
        "get_model_price_sync_state" => migrated_read("EmptyInputDto", "ModelPriceSyncStateDto"),
        "save_model_price_sync_config" => migrated_mutation(
            "SaveModelPriceSyncConfigInputDto",
            "ModelPriceSyncStateDto",
            "idempotent",
            false,
        ),
        "sync_model_prices" => migrated_mutation(
            "SyncModelPricesInputDto",
            "ModelPriceSyncResultDto",
            "non_idempotent",
            true,
        ),
        "reload_model_price_catalog" => migrated_mutation(
            "EmptyInputDto",
            "ModelPriceSyncStateDto",
            "idempotent",
            false,
        ),
        "open_model_price_catalog_directory" => {
            migrated_mutation("EmptyInputDto", "unit", "idempotent", false)
        }
        "resolve_station_key_pricing_context" => {
            migrated_read("PricingContextInputDto", "ResolvedPricingContextDto")
        }
        "load_pricing_comparison_workspace" => {
            migrated_read("EmptyInputDto", "PricingComparisonWorkspaceDto")
        }
        "load_pricing_group_monitor_status" => migrated_read(
            "PricingGroupMonitorStatusInputDto",
            "PricingGroupMonitorStatusWorkspaceDto",
        ),
        "upsert_model_base_price" => migrated_mutation(
            "UpsertModelBasePriceInputDto",
            "ModelBasePriceDto",
            "idempotent",
            false,
        ),
        "delete_model_base_price" => {
            migrated_mutation("ModelBasePriceIdInputDto", "unit", "idempotent", false)
        }
        "reset_model_base_prices_to_builtins" => migrated_mutation(
            "EmptyInputDto",
            "Vec<ModelBasePriceDto>",
            "idempotent",
            false,
        ),
        "get_proxy_status" => migrated_read("EmptyInputDto", "ProxyStatusDto"),
        "start_local_proxy" => {
            migrated_mutation("EmptyInputDto", "ProxyStatusDto", "idempotent", false)
        }
        "stop_local_proxy" => {
            migrated_mutation("EmptyInputDto", "ProxyStatusDto", "idempotent", false)
        }
        "restart_local_proxy" => {
            migrated_mutation("EmptyInputDto", "ProxyStatusDto", "non_idempotent", true)
        }
        "get_data_store_startup_state" => migrated_read("EmptyInputDto", "DataStoreStartupViewDto"),
        "refresh_data_store_candidates" => migrated_mutation(
            "EmptyInputDto",
            "DataStoreStartupViewDto",
            "idempotent",
            false,
        ),
        "locate_data_store_candidate" => migrated_mutation(
            "EmptyInputDto",
            "Option<DataStoreCandidateViewDto>",
            "non_idempotent",
            true,
        ),
        "activate_data_store_candidate" => migrated_mutation(
            "ActivateDataStoreCandidateInputDto",
            "ActivationResultDto",
            "non_idempotent",
            true,
        ),
        "create_new_data_store" => migrated_mutation(
            "CreateNewDataStoreInputDto",
            "ActivationResultDto",
            "non_idempotent",
            true,
        ),
        "open_data_store_backup_dir" => {
            migrated_mutation("EmptyInputDto", "unit", "non_idempotent", true)
        }
        "export_data_store_diagnostic" => {
            migrated_mutation("EmptyInputDto", "Option<String>", "non_idempotent", true)
        }
        "get_portable_migration_capability" => {
            migrated_read("EmptyInputDto", "PortableMigrationCapabilityDto")
        }
        "choose_portable_export_path" | "choose_portable_import_file" => CommandContract {
            input: "EmptyInputDto",
            output: "Option<PortablePathTokenDto>",
            error: "CommandError",
            mutation_kind: "maintenance_read",
            transport_retry: false,
            result_unknown: false,
            runtime_validation: "rust_dto_pre_application",
        },
        "start_portable_export" => CommandContract {
            input: "StartPortableExportInputDto",
            output: "PortableMigrationOperationStartedDto",
            error: "CommandError",
            mutation_kind: "maintenance_read",
            transport_retry: false,
            result_unknown: true,
            runtime_validation: "rust_dto_pre_application",
        },
        "get_portable_export_result" => {
            migrated_read("PortableMigrationResultInputDto", "PortableExportResultDto")
        }
        "start_portable_import_inspection" => CommandContract {
            input: "InspectPortableImportInputDto",
            output: "PortableMigrationOperationStartedDto",
            error: "CommandError",
            mutation_kind: "maintenance_read",
            transport_retry: false,
            result_unknown: true,
            runtime_validation: "rust_dto_pre_application",
        },
        "get_portable_import_inspection" => migrated_read(
            "PortableMigrationResultInputDto",
            "PortableImportInspectionDto",
        ),
        "start_portable_import_prepare" => CommandContract {
            input: "PreparePortableImportInputDto",
            output: "PortableMigrationOperationStartedDto",
            error: "CommandError",
            mutation_kind: "maintenance_activity",
            transport_retry: false,
            result_unknown: true,
            runtime_validation: "rust_dto_pre_application",
        },
        "get_portable_import_prepare_result" => migrated_read(
            "PortableMigrationResultInputDto",
            "PortableImportPrepareResultDto",
        ),
        "get_portable_migration_operation" => migrated_read(
            "PortableMigrationOperationInputDto",
            "PortableMigrationOperationDto",
        ),
        "get_portable_import_recovery_state" => {
            migrated_read("EmptyInputDto", "PortableImportRecoveryStateDto")
        }
        "choose_data_dir" | "reset_data_dir" => {
            migrated_mutation("EmptyInputDto", "SettingsDto", "non_idempotent", true)
        }
        "cleanup_before_update" => {
            migrated_mutation("EmptyInputDto", "ProxyStatusDto", "idempotent", false)
        }
        "prepare_local_proxy_for_update" => {
            migrated_mutation("EmptyInputDto", "ProxyStatusDto", "idempotent", false)
        }
        "updater_network_config" => migrated_read("EmptyInputDto", "UpdaterNetworkConfigDto"),
        "inspect_latest_update_manifest" => migrated_read(
            "PublishedUpdateInspectionInputDto",
            "PublishedUpdateInspectionDto",
        ),
        "get_runtime_contract_info" => migrated_read("EmptyInputDto", "RuntimeContractInfo"),
        _ => legacy_declared("legacy_unmigrated_input", "legacy_unmigrated_output"),
    }
}

#[cfg(test)]
const fn migrated_read(input: &'static str, output: &'static str) -> CommandContract {
    CommandContract {
        input,
        output,
        error: "CommandError",
        mutation_kind: "read",
        transport_retry: false,
        result_unknown: false,
        runtime_validation: "rust_dto_pre_application",
    }
}

#[cfg(test)]
const fn migrated_mutation(
    input: &'static str,
    output: &'static str,
    mutation_kind: &'static str,
    result_unknown: bool,
) -> CommandContract {
    CommandContract {
        input,
        output,
        error: "CommandError",
        mutation_kind,
        transport_retry: false,
        result_unknown,
        runtime_validation: "rust_dto_pre_application",
    }
}

#[cfg(test)]
const fn legacy_declared(input: &'static str, output: &'static str) -> CommandContract {
    CommandContract {
        input,
        output,
        error: "CommandError",
        mutation_kind: "legacy_unclassified",
        transport_retry: false,
        result_unknown: false,
        runtime_validation: "legacy_unmigrated",
    }
}

#[cfg(test)]
fn sha256(value: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(value.as_ref()))
}

#[cfg(test)]
fn canonical_contract() -> String {
    let mut commands = COMMANDS
        .iter()
        .map(|command| command.name)
        .collect::<Vec<_>>();
    commands.sort_unstable();
    let types = REGISTERED_TYPES
        .iter()
        .map(|descriptor| (descriptor.name, descriptor.typescript))
        .collect::<Vec<_>>();
    serde_json::to_string(&serde_json::json!({
        "generator_version": GENERATOR_VERSION,
        "ipc_contract_version": IPC_CONTRACT_VERSION,
        "commands": commands.iter().map(|name| {
            let contract = command_contract(name);
            serde_json::json!({"name": name, "input": contract.input, "output": contract.output, "error": contract.error, "mutation_kind": contract.mutation_kind, "transport_retry": contract.transport_retry, "result_unknown": contract.result_unknown, "runtime_validation": contract.runtime_validation})
        }).collect::<Vec<_>>(),
        "types": types,
        "streaming_surfaces": STREAMING_SURFACES,
    }))
    .expect("canonical IPC contract must serialize")
}

#[cfg(test)]
fn pilot_serialization_fixture() -> String {
    let settings = super::dto::SettingsDto::from(crate::models::settings::AppSettings {
        local_proxy_port: 8787,
        local_proxy_start_on_launch: false,
        local_key_masked: "sk-fixture-...redacted".into(),
        routing_policy_name: "automatic_balanced".into(),
        collector_proxy_mode: "direct".into(),
        collector_proxy_url: None,
        max_rate_multiplier: None,
        routing_group_scope: Default::default(),
        scheduler_config: Default::default(),
        low_balance_threshold_cny: 15.0,
        collector_interval_minutes: 30,
        balance_interval_minutes: 5,
        group_rate_interval_minutes: 20,
        published_status_interval_minutes: 5,
        pricing_refresh_interval_minutes: 60,
        collector_timeout_seconds: 15,
        collector_max_concurrency: 3,
        allow_depleted_fallback: false,
        developer_mode_enabled: false,
        show_decision_explanation: false,
        tray_behavior: "close_to_tray".into(),
        data_dir: "fixture-data-dir-redacted".into(),
        pending_data_dir: None,
        data_dir_change_requires_restart: false,
    });
    let update_settings = super::dto::settings::UpdateSettingsInputDto::parse(serde_json::json!({
        "localProxyPort": 8787, "defaultRoutingStrategy": "automatic_balanced",
        "collectorProxyMode": "direct", "collectorProxyUrl": null,
        "maxRateMultiplier": null, "defaultRoutingGroupFilter": "all_groups",
        "schedulerAdvancedSettings": null, "lowBalanceThresholdCny": 15.0,
        "collectorIntervalMinutes": 30, "balanceIntervalMinutes": 5,
        "groupRateIntervalMinutes": 20, "publishedStatusIntervalMinutes": 5,
        "pricingRefreshIntervalMinutes": 60,
        "collectorTimeoutSeconds": 15,
        "collectorMaxConcurrency": 3, "allowDepletedFallback": false,
        "developerModeEnabled": false
    }))
    .expect("settings fixture input");
    let create_station = super::dto::stations::CreateStationInputDto::parse(serde_json::json!({
        "name": "Fixture Station", "stationType": "newapi",
        "websiteUrl": "https://provider.invalid", "apiBaseUrl": "https://provider.invalid/v1",
        "apiKey": "", "collectorProxyMode": "inherit", "collectorProxyUrl": null,
        "enabled": true, "creditPerCny": 1.0, "lowBalanceThresholdCny": 15.0,
        "collectionIntervalMinutes": 5, "note": null
    }))
    .expect("create fixture input");
    let mut update_station =
        serde_json::to_value(&create_station).expect("create fixture serialization");
    update_station["id"] = serde_json::json!("station-fixture");
    update_station["apiKey"] = serde_json::Value::Null;
    let update_station = super::dto::stations::UpdateStationInputDto::parse(update_station)
        .expect("update fixture input");
    let delete_station = super::dto::stations::DeleteStationInputDto::parse(
        serde_json::json!({"id": "station-fixture"}),
    )
    .expect("delete fixture input");
    let reorder_stations = super::dto::stations::ReorderStationsInputDto::parse(
        serde_json::json!({"stationIds": ["station-fixture"]}),
    )
    .expect("reorder fixture input");
    let station = super::dto::stations::fixture();
    let mut commands = vec![
        serde_json::json!({"command": "get_settings", "input": {}, "output": settings.clone()}),
        serde_json::json!({"command": "list_stations", "input": {}, "output": [station.clone()]}),
        serde_json::json!({"command": "update_settings", "input": update_settings, "output": settings.clone()}),
        serde_json::json!({"command": "create_station", "input": create_station, "output": station.clone()}),
        serde_json::json!({"command": "update_station", "input": update_station, "output": station.clone()}),
        serde_json::json!({"command": "delete_station", "input": delete_station, "output": null}),
        serde_json::json!({"command": "reorder_stations", "input": reorder_stations, "output": [station]}),
    ];
    commands.extend(super::dto::station_keys::serialization_fixtures());
    commands.extend(super::dto::request_logs::serialization_fixtures());
    commands.extend(super::dto::collector_facts::serialization_fixtures());
    commands.extend(super::dto::channel_monitor_reads::serialization_fixtures());
    commands.extend(super::dto::channel_monitor_mutations::serialization_fixtures());
    commands.extend(super::dto::channel_monitor_operations::serialization_fixtures());
    commands.extend(super::dto::station_collector_operations::serialization_fixtures());
    commands.extend(super::dto::station_published_status::serialization_fixtures());
    commands.extend(super::dto::routing_health_reads::serialization_fixtures());
    commands.extend(super::dto::routing_mutations::serialization_fixtures());
    commands.extend(super::dto::pricing_reads::serialization_fixtures());
    commands.extend(super::dto::pricing_mutations::serialization_fixtures());
    commands.extend(super::dto::proxy_workspace_reads::serialization_fixtures());
    commands.extend(super::dto::dashboard_reads::serialization_fixtures());
    commands.extend(super::dto::provider_drafts::serialization_fixtures());
    let value = serde_json::json!({"schemaVersion": 1, "commands": commands});
    format!(
        "{}\n",
        serde_json::to_string_pretty(&value).expect("fixture must serialize")
    )
}

#[cfg(test)]
fn render_typescript(contract_hash: &str) -> String {
    let types = REGISTERED_TYPES
        .iter()
        .map(|descriptor| descriptor.typescript)
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut command_names = COMMANDS
        .iter()
        .map(|command| command.name)
        .collect::<Vec<_>>();
    command_names.sort_unstable();
    let command_union = command_names
        .iter()
        .map(|name| format!("  | \"{name}\""))
        .collect::<Vec<_>>()
        .join("\n");
    let source = format!(
        "// @generated by repository IPC generator. Do not edit.\n// generator version: {GENERATOR_VERSION}\n// IPC contract version: {IPC_CONTRACT_VERSION}\n// canonical hash: {contract_hash}\n\nimport {{ invoke }} from \"@/lib/bridge/transport\";\n\n{types}\n\nexport type IpcCommand =\n{command_union};\n\nexport const IPC_CONTRACT_VERSION = {IPC_CONTRACT_VERSION} as const;\nexport const IPC_BINDING_HASH = \"{contract_hash}\" as const;\n\nexport function invokeCommand<T>(command: IpcCommand, args?: Record<string, unknown>): Promise<T> {{\n  return invoke<T>(command, args);\n}}\n\nexport function getSettings(input: EmptyInputDto = {{}}): Promise<SettingsDto> {{\n  return invokeCommand<SettingsDto>(\"get_settings\", {{ input }});\n}}\n\nexport function listStations(input: EmptyInputDto = {{}}): Promise<StationDto[]> {{\n  return invokeCommand<StationDto[]>(\"list_stations\", {{ input }});\n}}\n\nexport function updateSettings(input: UpdateSettingsInputDto): Promise<SettingsDto> {{\n  return invokeCommand<SettingsDto>(\"update_settings\", {{ input }});\n}}\n\nexport function createStation(input: CreateStationInputDto): Promise<StationDto> {{\n  return invokeCommand<StationDto>(\"create_station\", {{ input }});\n}}\n\nexport function updateStation(input: UpdateStationInputDto): Promise<StationDto> {{\n  return invokeCommand<StationDto>(\"update_station\", {{ input }});\n}}\n\nexport function deleteStation(input: DeleteStationInputDto): Promise<void> {{\n  return invokeCommand<void>(\"delete_station\", {{ input }});\n}}\n\nexport function reorderStations(input: ReorderStationsInputDto): Promise<StationDto[]> {{\n  return invokeCommand<StationDto[]>(\"reorder_stations\", {{ input }});\n}}\n\nexport function listStationKeys(input: StationIdInputDto): Promise<StationKeyDto[]> {{\n  return invokeCommand<StationKeyDto[]>(\"list_station_keys\", {{ input }});\n}}\n\nexport function createStationKey(input: CreateStationKeyInputDto): Promise<StationKeyDto> {{\n  return invokeNonIdempotent<StationKeyDto>(\"create_station_key\", {{ input }});\n}}\n\nexport function updateStationKey(input: UpdateStationKeyInputDto): Promise<StationKeyDto> {{\n  return invokeCommand<StationKeyDto>(\"update_station_key\", {{ input }});\n}}\n\nexport function saveStationKeyWithDefaults(input: SaveStationKeyWithDefaultsInputDto): Promise<SaveStationKeyWithDefaultsResultDto> {{\n  return invokeNonIdempotent<SaveStationKeyWithDefaultsResultDto>(\"save_station_key_with_defaults\", {{ input }});\n}}\n\nexport function updateStationKeyGroupBinding(input: UpdateStationKeyGroupBindingInputDto): Promise<StationKeyDto> {{\n  return invokeCommand<StationKeyDto>(\"update_station_key_group_binding\", {{ input }});\n}}\n\nexport function deleteStationKey(input: StationKeyIdInputDto): Promise<void> {{\n  return invokeCommand<void>(\"delete_station_key\", {{ input }});\n}}\n\nexport function reorderStationKeys(input: ReorderStationKeysInputDto): Promise<StationKeyDto[]> {{\n  return invokeCommand<StationKeyDto[]>(\"reorder_station_keys\", {{ input }});\n}}\n\nexport function getRemoteKeyCapability(input: StationIdInputDto): Promise<RemoteKeyCapabilityDto> {{\n  return invokeCommand<RemoteKeyCapabilityDto>(\"get_remote_key_capability\", {{ input }});\n}}\n\nexport function listRemoteStationKeys(input: StationIdInputDto): Promise<RemoteStationKeyDto[]> {{\n  return invokeCommand<RemoteStationKeyDto[]>(\"list_remote_station_keys\", {{ input }});\n}}\n\nexport function scanRemoteStationKeys(input: StationIdInputDto): Promise<RemoteKeyScanResultDto> {{\n  return invokeCommand<RemoteKeyScanResultDto>(\"scan_remote_station_keys\", {{ input }});\n}}\n\nexport function createRemoteStationKey(input: CreateRemoteStationKeyInputDto): Promise<CreateRemoteStationKeyResultDto> {{\n  return invokeNonIdempotent<CreateRemoteStationKeyResultDto>(\"create_remote_station_key\", {{ input }});\n}}\n\nexport function createLocalStationKeyFromRemote(input: RemoteStationKeyInputDto): Promise<CreateLocalStationKeyFromRemoteResultDto> {{\n  return invokeNonIdempotent<CreateLocalStationKeyFromRemoteResultDto>(\"create_local_station_key_from_remote\", {{ input }});\n}}\n\nexport function bindRemoteStationKey(input: BindRemoteStationKeyInputDto): Promise<RemoteStationKeyDto[]> {{\n  return invokeCommand<RemoteStationKeyDto[]>(\"bind_remote_station_key\", {{ input }});\n}}\n\nexport function unbindRemoteStationKey(input: RemoteStationKeyInputDto): Promise<RemoteStationKeyDto[]> {{\n  return invokeCommand<RemoteStationKeyDto[]>(\"unbind_remote_station_key\", {{ input }});\n}}\n\nexport function listKeyPoolItems(input: EmptyInputDto = {{}}): Promise<KeyPoolItemDto[]> {{\n  return invokeCommand<KeyPoolItemDto[]>(\"list_key_pool_items\", {{ input }});\n}}\n\nexport function reorderKeyPool(input: ReorderKeyPoolInputDto): Promise<KeyPoolItemDto[]> {{\n  return invokeCommand<KeyPoolItemDto[]>(\"reorder_key_pool\", {{ input }});\n}}\n\nexport function getStationCredentials(input: StationIdInputDto): Promise<StationCredentialsDto> {{\n  return invokeCommand<StationCredentialsDto>(\"get_station_credentials\", {{ input }});\n}}\n\nexport function updateStationCredentials(input: UpdateStationCredentialsInputDto): Promise<StationCredentialsDto> {{\n  return invokeCommand<StationCredentialsDto>(\"update_station_credentials\", {{ input }});\n}}\n\nexport function updateStationSession(input: UpdateStationSessionInputDto): Promise<StationCredentialsDto> {{\n  return invokeCommand<StationCredentialsDto>(\"update_station_session\", {{ input }});\n}}\n\nexport function clearStationCredentials(input: StationIdInputDto): Promise<StationCredentialsDto> {{\n  return invokeCommand<StationCredentialsDto>(\"clear_station_credentials\", {{ input }});\n}}\n\nexport function getRuntimeContractInfo(): Promise<RuntimeContractInfo> {{\n  return invokeCommand<RuntimeContractInfo>(\"get_runtime_contract_info\");\n}}\n\nexport type StreamingSubscription = {{ close(): void }};\n\nexport interface TypedStreamingAdapter<Event> {{\n  readonly eventSchemaVersion: number;\n  open(onEvent: (event: Event) => void): StreamingSubscription;\n}}\n"
    );
    let mut source = source
        .replace(
            r#"import { invoke } from "@/lib/bridge/transport";"#,
            r#"import { invoke, invokeNonIdempotent } from "@/lib/bridge/transport";"#,
        )
        .replace(
            r#"return invokeCommand<StationDto>("create_station", { input });"#,
            r#"return invokeNonIdempotent<StationDto>("create_station", { input });"#,
        )
        .replace(
            r#"export function listStationKeys(input: StationIdInputDto): Promise<StationKeyDto[]> {"#,
            r#"export function getStationCapacityDomain(input: StationCapacityDomainQueryInputDto): Promise<StationCapacityDomainDto | null> {
  return invokeCommand<StationCapacityDomainDto | null>("get_station_capacity_domain", { input });
}

export function upsertStationCapacityDomain(input: UpsertStationCapacityDomainInputDto): Promise<StationCapacityDomainDto> {
  return invokeCommand<StationCapacityDomainDto>("upsert_station_capacity_domain", { input });
}

export function clearStationCapacityDomain(input: ClearStationCapacityDomainInputDto): Promise<void> {
  return invokeCommand<void>("clear_station_capacity_domain", { input });
}

export function listStationKeys(input: StationIdInputDto): Promise<StationKeyDto[]> {"#,
        )
        .replace(
            r#"export function listModelAliases(input: EmptyInputDto = {}): Promise<ModelAliasDto[]> {
  return invokeCommand<ModelAliasDto[]>("list_model_aliases", { input });
}"#,
            r#"export function getModelMappingWorkspace(input: EmptyInputDto = {}): Promise<ModelMappingWorkspaceDto> {
  return invokeCommand<ModelMappingWorkspaceDto>("get_model_mapping_workspace", { input });
}

export function getModelMappingDocument(input: EmptyInputDto = {}): Promise<ModelMappingDocumentDto> {
  return invokeCommand<ModelMappingDocumentDto>("get_model_mapping_document", { input });
}

export function validateModelMappingDocument(input: ValidateModelMappingDocumentInputDto): Promise<ModelMappingValidationResultDto> {
  return invokeCommand<ModelMappingValidationResultDto>("validate_model_mapping_document", { input });
}

export function applyModelMappingDocument(input: ApplyModelMappingDocumentInputDto): Promise<ModelMappingWorkspaceDto> {
  return invokeCommand<ModelMappingWorkspaceDto>("apply_model_mapping_document", { input });
}

export function restoreModelMappingRevision(input: RestoreModelMappingRevisionInputDto): Promise<ModelMappingWorkspaceDto> {
  return invokeCommand<ModelMappingWorkspaceDto>("restore_model_mapping_revision", { input });
}

export function simulateModelMapping(input: SimulateModelMappingInputDto): Promise<ModelMappingSimulationResultDto> {
  return invokeCommand<ModelMappingSimulationResultDto>("simulate_model_mapping", { input });
}

export function resolveRequestMappingTrace(input: ResolveRequestMappingTraceInputDto): Promise<ModelMappingTraceDto> {
  return invokeCommand<ModelMappingTraceDto>("resolve_request_mapping_trace", { input });
}

export function listModelAliases(input: EmptyInputDto = {}): Promise<ModelAliasDto[]> {
  return invokeCommand<ModelAliasDto[]>("list_model_aliases", { input });
}"#,
        )
        .replace(
            r#"export function listStationKeys(input: StationIdInputDto): Promise<StationKeyDto[]> {"#,
            r#"export function createOrResumeProviderDraft(input: CreateProviderDraftInputDto): Promise<ProviderDraftDto> {
  return invokeCommand<ProviderDraftDto>("create_or_resume_provider_draft", { input });
}

export function getProviderDraft(input: ProviderDraftIdInputDto): Promise<ProviderDraftDto> {
  return invokeCommand<ProviderDraftDto>("get_provider_draft", { input });
}

export function patchProviderDraft(input: PatchProviderDraftInputDto): Promise<ProviderDraftDto> {
  return invokeCommand<ProviderDraftDto>("patch_provider_draft", { input });
}

export function discardProviderDraft(input: ProviderDraftIdInputDto): Promise<void> {
  return invokeCommand<void>("discard_provider_draft", { input });
}

export function collectProviderDraftPreview(input: CollectProviderDraftPreviewInputDto): Promise<ProviderDraftPreviewDto> {
  return invokeCommand<ProviderDraftPreviewDto>("collect_provider_draft_preview", { input });
}

export function scanProviderDraftRemoteKeys(input: ProviderDraftIdInputDto): Promise<RemoteKeyScanResultDto> {
  return invokeCommand<RemoteKeyScanResultDto>("scan_provider_draft_remote_keys", { input });
}

export function commitProviderDraft(input: CommitProviderDraftInputDto): Promise<StationDto> {
  return invokeCommand<StationDto>("commit_provider_draft", { input });
}

export function listStationKeys(input: StationIdInputDto): Promise<StationKeyDto[]> {"#,
        )
        .replace(
            r#"export function getSettings(input: EmptyInputDto = {}): Promise<SettingsDto> {
  return invokeCommand<SettingsDto>("get_settings", { input });
}"#,
r#"export function appStatus(input: EmptyInputDto = {}): Promise<AppStatusDto> {
  return invokeCommand<AppStatusDto>("app_status", { input });
}

export function getRuntimeStatus(input: EmptyInputDto = {}): Promise<RuntimeStatusDto> {
  return invokeCommand<RuntimeStatusDto>("get_runtime_status", { input });
}

export function restartApplication(input: EmptyInputDto = {}): Promise<void> {
  return invokeNonIdempotent<void>("restart_application", { input });
}

export function getSettings(input: EmptyInputDto = {}): Promise<SettingsDto> {
  return invokeCommand<SettingsDto>("get_settings", { input });
}

export function getLocalAccessKey(input: EmptyInputDto = {}): Promise<string> {
  return invokeCommand<string>("get_local_access_key", { input });
}

export function updateLocalAccessKey(input: UpdateLocalAccessKeyInputDto): Promise<SettingsDto> {
  return invokeCommand<SettingsDto>("update_local_access_key", { input });
}

export function importRelayPoolToCcswitch(input: EmptyInputDto = {}): Promise<CcswitchImportResultDto> {
  return invokeNonIdempotent<CcswitchImportResultDto>("import_relay_pool_to_ccswitch", { input });
}

export function openExternalUrl(input: OpenExternalUrlInputDto): Promise<void> {
  return invokeNonIdempotent<void>("open_external_url", { input });
}"#,
        )
        .replace(
            "export function getRuntimeContractInfo(): Promise<RuntimeContractInfo>",
            r#"export function listCommonLoginOptions(input: EmptyInputDto = {}): Promise<CommonLoginOptionsDto> {
  return invokeCommand<CommonLoginOptionsDto>("list_common_login_options", { input });
}

export function upsertCommonLoginEmail(input: UpsertCommonLoginEmailInputDto): Promise<CommonLoginEmailDto> {
  return invokeNonIdempotent<CommonLoginEmailDto>("upsert_common_login_email", { input });
}

export function deleteCommonLoginEmail(input: CommonLoginIdInputDto): Promise<void> {
  return invokeCommand<void>("delete_common_login_email", { input });
}

export function upsertCommonLoginPassword(input: UpsertCommonLoginPasswordInputDto): Promise<CommonLoginPasswordDto> {
  return invokeNonIdempotent<CommonLoginPasswordDto>("upsert_common_login_password", { input });
}

export function deleteCommonLoginPassword(input: CommonLoginIdInputDto): Promise<void> {
  return invokeCommand<void>("delete_common_login_password", { input });
}

export function getCommonLoginPassword(input: CommonLoginIdInputDto): Promise<string> {
  return invokeCommand<string>("get_common_login_password", { input });
}

export function listRequestLogs(input: EmptyInputDto = {}): Promise<RequestLogDto[]> {
  return invokeCommand<RequestLogDto[]>("list_request_logs", { input });
}

export function clearRequestLogs(input: EmptyInputDto = {}): Promise<void> {
  return invokeCommand<void>("clear_request_logs", { input });
}

export function loadDashboardLiveRequestMetrics(input: DashboardRequestMetricsInputDto): Promise<DashboardLiveRequestMetricsSnapshotDto> {
  return invokeCommand<DashboardLiveRequestMetricsSnapshotDto>("load_dashboard_live_request_metrics", { input });
}

export function loadDashboardCumulativeRequestMetrics(input: EmptyInputDto = {}): Promise<DashboardCumulativeRequestMetricsSnapshotDto> {
  return invokeCommand<DashboardCumulativeRequestMetricsSnapshotDto>("load_dashboard_cumulative_request_metrics", { input });
}

export function listBalanceSnapshots(input: EmptyInputDto = {}): Promise<BalanceSnapshotDto[]> {
  return invokeCommand<BalanceSnapshotDto[]>("list_balance_snapshots", { input });
}

export function listCurrentStationBalanceSnapshots(input: EmptyInputDto = {}): Promise<BalanceSnapshotDto[]> {
  return invokeCommand<BalanceSnapshotDto[]>("list_current_station_balance_snapshots", { input });
}

export function listBalanceSnapshotsForStation(input: CollectorStationIdInputDto): Promise<BalanceSnapshotDto[]> {
  return invokeCommand<BalanceSnapshotDto[]>("list_balance_snapshots_for_station", { input });
}

export function upsertBalanceSnapshot(input: UpsertBalanceSnapshotInputDto): Promise<BalanceSnapshotDto> {
  return invokeCommand<BalanceSnapshotDto>("upsert_balance_snapshot", { input });
}

export function listStationGroupBindings(input: CollectorStationIdInputDto): Promise<StationGroupBindingDto[]> {
  return invokeCommand<StationGroupBindingDto[]>("list_station_group_bindings", { input });
}

export function listStationGroupOptions(input: CollectorStationIdInputDto): Promise<StationGroupOptionDto[]> {
  return invokeCommand<StationGroupOptionDto[]>("list_station_group_options", { input });
}

export function upsertStationGroupBinding(input: UpsertStationGroupBindingInputDto): Promise<StationGroupBindingDto> {
  return invokeCommand<StationGroupBindingDto>("upsert_station_group_binding", { input });
}

export function listGroupRateRecords(input: CollectorStationIdInputDto): Promise<GroupRateRecordDto[]> {
  return invokeCommand<GroupRateRecordDto[]>("list_group_rate_records", { input });
}

export function listCollectorRuns(input: CollectorStationIdInputDto): Promise<CollectorRunDto[]> {
  return invokeCommand<CollectorRunDto[]>("list_collector_runs", { input });
}

export function listCollectorSnapshots(input: CollectorStationIdInputDto): Promise<CollectorSnapshotDto[]> {
  return invokeCommand<CollectorSnapshotDto[]>("list_collector_snapshots", { input });
}

export function getLatestCollectorSnapshot(input: CollectorStationIdInputDto): Promise<CollectorSnapshotDto | null> {
  return invokeCommand<CollectorSnapshotDto | null>("get_latest_collector_snapshot", { input });
}

export function listLatestCollectorSnapshots(input: CollectorStationIdsInputDto): Promise<CollectorSnapshotDto[]> {
  return invokeCommand<CollectorSnapshotDto[]>("list_latest_collector_snapshots", { input });
}

export function listChannelMonitors(input: EmptyInputDto = {}): Promise<ChannelMonitorDto[]> {
  return invokeCommand<ChannelMonitorDto[]>("list_channel_monitors", { input });
}

export function listChannelMonitorTemplates(input: EmptyInputDto = {}): Promise<ChannelMonitorRequestTemplateDto[]> {
  return invokeCommand<ChannelMonitorRequestTemplateDto[]>("list_channel_monitor_templates", { input });
}

export function createChannelMonitor(input: CreateChannelMonitorInputDto): Promise<ChannelMonitorDto> {
  return invokeNonIdempotent<ChannelMonitorDto>("create_channel_monitor", { input });
}

export function updateChannelMonitor(input: UpdateChannelMonitorInputDto): Promise<ChannelMonitorDto> {
  return invokeCommand<ChannelMonitorDto>("update_channel_monitor", { input });
}

export function deleteChannelMonitor(input: ChannelMonitorMutationIdInputDto): Promise<void> {
  return invokeCommand<void>("delete_channel_monitor", { input });
}

export function createChannelMonitorTemplate(input: CreateChannelMonitorTemplateInputDto): Promise<ChannelMonitorRequestTemplateDto> {
  return invokeNonIdempotent<ChannelMonitorRequestTemplateDto>("create_channel_monitor_template", { input });
}

export function updateChannelMonitorTemplate(input: UpdateChannelMonitorTemplateInputDto): Promise<ChannelMonitorRequestTemplateDto> {
  return invokeCommand<ChannelMonitorRequestTemplateDto>("update_channel_monitor_template", { input });
}

export function duplicateChannelMonitorTemplate(input: ChannelMonitorMutationIdInputDto): Promise<ChannelMonitorRequestTemplateDto> {
  return invokeNonIdempotent<ChannelMonitorRequestTemplateDto>("duplicate_channel_monitor_template", { input });
}

export function deleteChannelMonitorTemplate(input: ChannelMonitorMutationIdInputDto): Promise<void> {
  return invokeCommand<void>("delete_channel_monitor_template", { input });
}

export function loadChannelStatusWorkspace(input: ChannelStatusWorkspaceInputDto = {}): Promise<ChannelStatusWorkspaceDto> {
  return invokeCommand<ChannelStatusWorkspaceDto>("load_channel_status_workspace", { input });
}

export function getStationPublishedStatusWorkspace(input: StationPublishedStatusWorkspaceInputDto): Promise<StationPublishedStatusWorkspaceDto> {
  return invokeCommand<StationPublishedStatusWorkspaceDto>("get_station_published_status_workspace", { input });
}

export function listChannelMonitorExecutions(input: ChannelMonitorExecutionListInputDto = {}): Promise<ChannelMonitorExecutionPageDto> {
  return invokeCommand<ChannelMonitorExecutionPageDto>("list_channel_monitor_executions", { input });
}

export function getChannelMonitorExecution(input: ChannelMonitorExecutionIdInputDto): Promise<ChannelMonitorExecutionDetailDto> {
  return invokeCommand<ChannelMonitorExecutionDetailDto>("get_channel_monitor_execution", { input });
}

export function listChannelMonitorAttempts(input: ChannelMonitorAttemptHistoryInputDto): Promise<ChannelMonitorAttemptPageDto> {
  return invokeCommand<ChannelMonitorAttemptPageDto>("list_channel_monitor_attempts", { input });
}

export function listMonitoringCapabilities(input: EmptyInputDto = {}): Promise<MonitoringCapabilityCatalogDto> {
  return invokeCommand<MonitoringCapabilityCatalogDto>("list_monitoring_capabilities", { input });
}

export function runChannelMonitorNow(input: RunChannelMonitorNowInputDto): Promise<RunChannelMonitorReceiptDto> {
  return invokeNonIdempotent<RunChannelMonitorReceiptDto>("run_channel_monitor_now", { input });
}

export function cancelChannelMonitorExecution(input: CancelChannelMonitorExecutionInputDto): Promise<CancelChannelMonitorExecutionReceiptDto> {
  return invokeCommand<CancelChannelMonitorExecutionReceiptDto>("cancel_channel_monitor_execution", { input });
}

export function detectSub2apiStation(input: CollectorStationIdInputDto): Promise<CollectorRunResultDto> {
  return invokeNonIdempotent<CollectorRunResultDto>("detect_sub2api_station", { input });
}

export function collectSub2apiStation(input: CollectorStationIdInputDto): Promise<CollectorRunResultDto> {
  return invokeNonIdempotent<CollectorRunResultDto>("collect_sub2api_station", { input });
}

export function detectStationInfo(input: CollectorStationIdInputDto): Promise<CollectorRunResultDto> {
  return invokeNonIdempotent<CollectorRunResultDto>("detect_station_info", { input });
}

export function collectStationInfo(input: CollectorStationIdInputDto): Promise<CollectorRunResultDto> {
  return invokeNonIdempotent<CollectorRunResultDto>("collect_station_info", { input });
}

export function collectStationTask(input: StationCollectorTaskInputDto): Promise<CollectorRunResultDto> {
  return invokeNonIdempotent<CollectorRunResultDto>("collect_station_task", { input });
}

export function scanStationRecharge(input: CollectorStationIdInputDto): Promise<CollectorRunResultDto> {
  return invokeNonIdempotent<CollectorRunResultDto>("scan_station_recharge", { input });
}

export function testStationLogin(input: CollectorStationIdInputDto): Promise<CollectorRunResultDto> {
  return invokeNonIdempotent<CollectorRunResultDto>("test_station_login", { input });
}

export function testStationLoginInput(input: StationLoginTestInputDto): Promise<StationLoginTestResultDto> {
  return invokeCommand<StationLoginTestResultDto>("test_station_login_input", { input });
}

export function startCaptureSession(input: CaptureStationIdInputDto): Promise<CaptureSessionStatusDto> {
  return invokeNonIdempotent<CaptureSessionStatusDto>("start_capture_session", { input });
}

export function startProviderDraftAuthorization(input: ProviderDraftIdInputDto): Promise<CaptureSessionStatusDto> {
  return invokeNonIdempotent<CaptureSessionStatusDto>("start_provider_draft_authorization", { input });
}

export function getCaptureSessionStatus(input: CaptureStationIdInputDto): Promise<CaptureSessionStatusDto> {
  return invokeCommand<CaptureSessionStatusDto>("get_capture_session_status", { input });
}

export function recordCaptureEvent(input: CapturedHttpEventInputDto): Promise<CaptureSessionStatusDto> {
  return invokeNonIdempotent<CaptureSessionStatusDto>("record_capture_event", { input });
}

export function finishCaptureSession(input: CaptureStationIdInputDto): Promise<CollectorRunResultDto> {
  return invokeNonIdempotent<CollectorRunResultDto>("finish_capture_session", { input });
}

export function finishWebAuthorizationSession(input: CaptureStationIdInputDto): Promise<CollectorRunResultDto> {
  return invokeNonIdempotent<CollectorRunResultDto>("finish_web_authorization_session", { input });
}

export function finishProviderDraftAuthorizationSession(input: ProviderDraftIdInputDto): Promise<ProviderDraftPreviewDto> {
  return invokeNonIdempotent<ProviderDraftPreviewDto>("finish_provider_draft_authorization_session", { input });
}

export function clearCaptureSession(input: CaptureStationIdInputDto): Promise<CaptureSessionStatusDto> {
  return invokeCommand<CaptureSessionStatusDto>("clear_capture_session", { input });
}

export function closeCaptureSession(input: CaptureStationIdInputDto): Promise<CaptureSessionStatusDto> {
  return invokeCommand<CaptureSessionStatusDto>("close_capture_session", { input });
}

export function getStationKeyCapabilities(input: RoutingStationKeyIdInputDto): Promise<StationKeyCapabilitiesDto> {
  return invokeCommand<StationKeyCapabilitiesDto>("get_station_key_capabilities", { input });
}

export function listModelAliases(input: EmptyInputDto = {}): Promise<ModelAliasDto[]> {
  return invokeCommand<ModelAliasDto[]>("list_model_aliases", { input });
}

export function updateStationKeyCapabilities(input: UpdateStationKeyCapabilitiesInputDto): Promise<StationKeyCapabilitiesDto> {
  return invokeCommand<StationKeyCapabilitiesDto>("update_station_key_capabilities", { input });
}

export function upsertModelAlias(input: UpsertModelAliasInputDto): Promise<ModelAliasDto> {
  return invokeCommand<ModelAliasDto>("upsert_model_alias", { input });
}

export function deleteModelAlias(input: DeleteModelAliasInputDto): Promise<void> {
  return invokeCommand<void>("delete_model_alias", { input });
}

export function listStationKeyHealth(input: EmptyInputDto = {}): Promise<StationKeyHealthDto[]> {
  return invokeCommand<StationKeyHealthDto[]>("list_station_key_health", { input });
}

export function getRoutingProtectionStatus(input: RoutingProtectionStatusInputDto = {}): Promise<RoutingProtectionStatusDto> {
  return invokeCommand<RoutingProtectionStatusDto>("get_routing_protection_status", { input });
}

export function loadRoutingPolicy(input: EmptyInputDto = {}): Promise<RoutingPolicySnapshotDto> {
  return invokeCommand<RoutingPolicySnapshotDto>("load_routing_policy", { input });
}

export function applyRoutingPolicyDocument(input: ApplyRoutingPolicyDocumentInputDto): Promise<RoutingPolicySnapshotDto> {
  return invokeCommand<RoutingPolicySnapshotDto>("apply_routing_policy_document", { input });
}

export function listStationEndpointHealth(input: EmptyInputDto = {}): Promise<StationEndpointHealthDto[]> {
  return invokeCommand<StationEndpointHealthDto[]>("list_station_endpoint_health", { input });
}

export function loadRoutingWorkspaceSnapshot(input: RoutingWorkspaceSnapshotInputDto = {}): Promise<RoutingWorkspaceSnapshotDto> {
  return invokeCommand<RoutingWorkspaceSnapshotDto>("load_routing_workspace_snapshot", { input });
}

export function loadRoutingRuntimeOverlay(input: EmptyInputDto = {}): Promise<RoutingRuntimeOverlayDto> {
  return invokeCommand<RoutingRuntimeOverlayDto>("load_routing_runtime_overlay", { input });
}

export function listRecentRouteDecisions(input: RecentRouteDecisionsInputDto = {}): Promise<RecentRouteDecisionsPageDto> {
  return invokeCommand<RecentRouteDecisionsPageDto>("list_recent_route_decisions", { input });
}

export function listErrorRateHistory(input: ErrorRateHistoryInputDto = {}): Promise<ErrorRateHistoryPageDto> {
  return invokeCommand<ErrorRateHistoryPageDto>("list_error_rate_history", { input });
}

export function getStationKeyOperationalDetail(input: StationKeyOperationalDetailInputDto): Promise<StationKeyOperationalDetailDto> {
  return invokeCommand<StationKeyOperationalDetailDto>("get_station_key_operational_detail", { input });
}

export function getRequestDecisionTrace(input: RequestDecisionTraceInputDto): Promise<RequestDecisionTraceDto> {
  return invokeCommand<RequestDecisionTraceDto>("get_request_decision_trace", { input });
}

export function getStationKeyHealth(input: RoutingStationKeyIdInputDto): Promise<StationKeyHealthDto> {
  return invokeCommand<StationKeyHealthDto>("get_station_key_health", { input });
}

export function getOperationStatus(input: OperationIdInputDto): Promise<OperationSnapshotDto> {
  return invokeCommand<OperationSnapshotDto>("get_operation_status", { input });
}

export function cancelOperation(input: CancelOperationInputDto): Promise<CancelOperationOutcomeDto> {
  return invokeCommand<CancelOperationOutcomeDto>("cancel_operation", { input });
}

export function startStationKeyConnectivityOperation(input: StationKeyConnectivityInputDto): Promise<OperationStartedDto> {
  return invokeNonIdempotent<OperationStartedDto>("start_station_key_connectivity_operation", { input });
}

export function getStationKeyConnectivityOperationResult(input: OperationIdInputDto): Promise<StationKeyConnectivityResultDto> {
  return invokeCommand<StationKeyConnectivityResultDto>("get_station_key_connectivity_operation_result", { input });
}

export function startStationKeyModelDiscoveryOperation(input: RoutingStationKeyIdInputDto): Promise<OperationStartedDto> {
  return invokeNonIdempotent<OperationStartedDto>("start_station_key_model_discovery_operation", { input });
}

export function getStationKeyModelDiscoveryOperationResult(input: OperationIdInputDto): Promise<StationKeyModelDiscoveryResultDto> {
  return invokeCommand<StationKeyModelDiscoveryResultDto>("get_station_key_model_discovery_operation_result", { input });
}

export function simulateRoute(input: RouteSimulationInputDto): Promise<RouteSimulationResultDto> {
  return invokeCommand<RouteSimulationResultDto>("simulate_route", { input });
}

export function listModelBasePrices(input: EmptyInputDto = {}): Promise<ModelBasePriceDto[]> {
  return invokeCommand<ModelBasePriceDto[]>("list_model_base_prices", { input });
}

export function listModelPriceSyncCatalog(input: EmptyInputDto = {}): Promise<ModelPriceCatalogEntryDto[]> {
  return invokeCommand<ModelPriceCatalogEntryDto[]>("list_model_price_sync_catalog", { input });
}

export function getModelPriceSyncState(input: EmptyInputDto = {}): Promise<ModelPriceSyncStateDto> {
  return invokeCommand<ModelPriceSyncStateDto>("get_model_price_sync_state", { input });
}

export function saveModelPriceSyncConfig(input: SaveModelPriceSyncConfigInputDto): Promise<ModelPriceSyncStateDto> {
  return invokeCommand<ModelPriceSyncStateDto>("save_model_price_sync_config", { input });
}

export function syncModelPrices(input: SyncModelPricesInputDto): Promise<ModelPriceSyncResultDto> {
  return invokeNonIdempotent<ModelPriceSyncResultDto>("sync_model_prices", { input });
}

export function reloadModelPriceCatalog(input: EmptyInputDto = {}): Promise<ModelPriceSyncStateDto> {
  return invokeCommand<ModelPriceSyncStateDto>("reload_model_price_catalog", { input });
}

export function openModelPriceCatalogDirectory(input: EmptyInputDto = {}): Promise<void> {
  return invokeCommand<void>("open_model_price_catalog_directory", { input });
}

export function resolveStationKeyPricingContext(input: PricingContextInputDto): Promise<ResolvedPricingContextDto> {
  return invokeCommand<ResolvedPricingContextDto>("resolve_station_key_pricing_context", { input });
}

export function loadPricingComparisonWorkspace(input: EmptyInputDto = {}): Promise<PricingComparisonWorkspaceDto> {
  return invokeCommand<PricingComparisonWorkspaceDto>("load_pricing_comparison_workspace", { input });
}

export function loadPricingGroupMonitorStatus(input: PricingGroupMonitorStatusInputDto): Promise<PricingGroupMonitorStatusWorkspaceDto> {
  return invokeCommand<PricingGroupMonitorStatusWorkspaceDto>("load_pricing_group_monitor_status", { input });
}

export function upsertModelBasePrice(input: UpsertModelBasePriceInputDto): Promise<ModelBasePriceDto> {
  return invokeCommand<ModelBasePriceDto>("upsert_model_base_price", { input });
}

export function deleteModelBasePrice(input: ModelBasePriceIdInputDto): Promise<void> {
  return invokeCommand<void>("delete_model_base_price", { input });
}

export function resetModelBasePricesToBuiltins(input: EmptyInputDto = {}): Promise<ModelBasePriceDto[]> {
  return invokeCommand<ModelBasePriceDto[]>("reset_model_base_prices_to_builtins", { input });
}

export function getProxyStatus(input: EmptyInputDto = {}): Promise<ProxyStatusDto> {
  return invokeCommand<ProxyStatusDto>("get_proxy_status", { input });
}

export function pingStationEndpoint(input: StationIdInputDto): Promise<EndpointPingResultDto> {
  return invokeNonIdempotent<EndpointPingResultDto>("ping_station_endpoint", { input });
}

export function startLocalProxy(input: EmptyInputDto = {}): Promise<ProxyStatusDto> {
  return invokeCommand<ProxyStatusDto>("start_local_proxy", { input });
}

export function stopLocalProxy(input: EmptyInputDto = {}): Promise<ProxyStatusDto> {
  return invokeCommand<ProxyStatusDto>("stop_local_proxy", { input });
}

export function restartLocalProxy(input: EmptyInputDto = {}): Promise<ProxyStatusDto> {
  return invokeNonIdempotent<ProxyStatusDto>("restart_local_proxy", { input });
}

export function getDataStoreStartupState(input: EmptyInputDto = {}): Promise<DataStoreStartupViewDto> {
  return invokeCommand<DataStoreStartupViewDto>("get_data_store_startup_state", { input });
}

export function refreshDataStoreCandidates(input: EmptyInputDto = {}): Promise<DataStoreStartupViewDto> {
  return invokeCommand<DataStoreStartupViewDto>("refresh_data_store_candidates", { input });
}

export function locateDataStoreCandidate(input: EmptyInputDto = {}): Promise<DataStoreCandidateViewDto | null> {
  return invokeNonIdempotent<DataStoreCandidateViewDto | null>("locate_data_store_candidate", { input });
}

export function activateDataStoreCandidate(input: ActivateDataStoreCandidateInputDto): Promise<ActivationResultDto> {
  return invokeNonIdempotent<ActivationResultDto>("activate_data_store_candidate", { input });
}

export function createNewDataStore(input: CreateNewDataStoreInputDto): Promise<ActivationResultDto> {
  return invokeNonIdempotent<ActivationResultDto>("create_new_data_store", { input });
}

export function openDataStoreBackupDir(input: EmptyInputDto = {}): Promise<void> {
  return invokeNonIdempotent<void>("open_data_store_backup_dir", { input });
}

export function exportDataStoreDiagnostic(input: EmptyInputDto = {}): Promise<string | null> {
  return invokeNonIdempotent<string | null>("export_data_store_diagnostic", { input });
}

export function getPortableMigrationCapability(input: EmptyInputDto = {}): Promise<PortableMigrationCapabilityDto> {
  return invokeCommand<PortableMigrationCapabilityDto>("get_portable_migration_capability", { input });
}

export function choosePortableExportPath(input: EmptyInputDto = {}): Promise<PortablePathTokenDto | null> {
  return invokeNonIdempotent<PortablePathTokenDto | null>("choose_portable_export_path", { input });
}

export function startPortableExport(input: StartPortableExportInputDto): Promise<PortableMigrationOperationStartedDto> {
  return invokeNonIdempotent<PortableMigrationOperationStartedDto>("start_portable_export", { input });
}

export function getPortableExportResult(input: PortableMigrationResultInputDto): Promise<PortableExportResultDto> {
  return invokeCommand<PortableExportResultDto>("get_portable_export_result", { input });
}

export function choosePortableImportFile(input: EmptyInputDto = {}): Promise<PortablePathTokenDto | null> {
  return invokeNonIdempotent<PortablePathTokenDto | null>("choose_portable_import_file", { input });
}

export function startPortableImportInspection(input: InspectPortableImportInputDto): Promise<PortableMigrationOperationStartedDto> {
  return invokeNonIdempotent<PortableMigrationOperationStartedDto>("start_portable_import_inspection", { input });
}

export function getPortableImportInspection(input: PortableMigrationResultInputDto): Promise<PortableImportInspectionDto> {
  return invokeCommand<PortableImportInspectionDto>("get_portable_import_inspection", { input });
}

export function startPortableImportPrepare(input: PreparePortableImportInputDto): Promise<PortableMigrationOperationStartedDto> {
  return invokeNonIdempotent<PortableMigrationOperationStartedDto>("start_portable_import_prepare", { input });
}

export function getPortableImportPrepareResult(input: PortableMigrationResultInputDto): Promise<PortableImportPrepareResultDto> {
  return invokeCommand<PortableImportPrepareResultDto>("get_portable_import_prepare_result", { input });
}

export function getPortableMigrationOperation(input: PortableMigrationOperationInputDto): Promise<PortableMigrationOperationDto> {
  return invokeCommand<PortableMigrationOperationDto>("get_portable_migration_operation", { input });
}

export function getPortableImportRecoveryState(input: EmptyInputDto = {}): Promise<PortableImportRecoveryStateDto> {
  return invokeCommand<PortableImportRecoveryStateDto>("get_portable_import_recovery_state", { input });
}

export function chooseDataDir(input: EmptyInputDto = {}): Promise<SettingsDto> {
  return invokeNonIdempotent<SettingsDto>("choose_data_dir", { input });
}

export function resetDataDir(input: EmptyInputDto = {}): Promise<SettingsDto> {
  return invokeNonIdempotent<SettingsDto>("reset_data_dir", { input });
}

export function cleanupBeforeUpdate(input: EmptyInputDto = {}): Promise<ProxyStatusDto> {
  return invokeCommand<ProxyStatusDto>("cleanup_before_update", { input });
}

export function prepareLocalProxyForUpdate(input: EmptyInputDto = {}): Promise<ProxyStatusDto> {
  return invokeCommand<ProxyStatusDto>("prepare_local_proxy_for_update", { input });
}

export function updaterNetworkConfig(input: EmptyInputDto = {}): Promise<UpdaterNetworkConfigDto> {
  return invokeCommand<UpdaterNetworkConfigDto>("updater_network_config", { input });
}

export function inspectLatestUpdateManifest(input: PublishedUpdateInspectionInputDto): Promise<PublishedUpdateInspectionDto> {
  return invokeCommand<PublishedUpdateInspectionDto>("inspect_latest_update_manifest", { input });
}

export function getRuntimeContractInfo(): Promise<RuntimeContractInfo>"#,
        )
        .replace(
            "export function bindRemoteStationKey(input: BindRemoteStationKeyInputDto)",
            r#"export function deleteRemoteStationKey(input: RemoteStationKeyInputDto): Promise<DeleteRemoteStationKeyResultDto> {
  return invokeCommand<DeleteRemoteStationKeyResultDto>("delete_remote_station_key", { input });
}

export function bindRemoteStationKey(input: BindRemoteStationKeyInputDto)"#,
        )
        .replace(
            "export function getRuntimeContractInfo(): Promise<RuntimeContractInfo>",
            r#"export function listAlertPolicies(input: EmptyInputDto = {}): Promise<AlertPolicyDto[]> {
  return invokeCommand<AlertPolicyDto[]>("list_alert_policies", { input });
}

export function upsertAlertPolicy(input: AlertPolicyInputDto): Promise<AlertPolicyDto> {
  return invokeCommand<AlertPolicyDto>("upsert_alert_policy", { input });
}

export function deleteAlertPolicy(input: AlertPolicyDeleteInputDto): Promise<void> {
  return invokeCommand<void>("delete_alert_policy", { input });
}

export function getAlertingSettings(input: EmptyInputDto = {}): Promise<AlertingSettingsDto> {
  return invokeCommand<AlertingSettingsDto>("get_alerting_settings", { input });
}

export function updateAlertingSettings(input: AlertingSettingsInputDto): Promise<AlertingSettingsDto> {
  return invokeCommand<AlertingSettingsDto>("update_alerting_settings", { input });
}

export function listAlertingIncidents(input: AlertingCurrentInputDto = {}): Promise<AlertingIncidentPageDto> {
  return invokeCommand<AlertingIncidentPageDto>("list_alerting_incidents", { input });
}

export function listAlertingActivity(input: AlertingActivityInputDto = {}): Promise<AlertingActivityPageDto> {
  return invokeCommand<AlertingActivityPageDto>("list_alerting_activity", { input });
}

export function getAlertingIncident(input: AlertingIncidentInputDto): Promise<AlertingIncidentSummaryDto> {
  return invokeCommand<AlertingIncidentSummaryDto>("get_alerting_incident", { input });
}

export function listAlertingOccurrences(input: AlertingHistoryInputDto): Promise<AlertingOccurrencePageDto> {
  return invokeCommand<AlertingOccurrencePageDto>("list_alerting_occurrences", { input });
}

export function listAlertingDeliveries(input: AlertingHistoryInputDto): Promise<AlertingDeliveryPageDto> {
  return invokeCommand<AlertingDeliveryPageDto>("list_alerting_deliveries", { input });
}

export function markAlertingSeen(input: AlertingMarkSeenInputDto): Promise<void> {
  return invokeCommand<void>("mark_alerting_seen", { input });
}

export function markAllAlertingSeen(input: AlertingMarkAllSeenInputDto = {}): Promise<number> {
  return invokeCommand<number>("mark_all_alerting_seen", { input });
}

export function resolveAllAlertingIncidents(input: AlertingMarkAllSeenInputDto = {}): Promise<number> {
  return invokeCommand<number>("resolve_all_alerting_incidents", { input });
}

export function clearAlertingIncidents(input: AlertingClearInputDto = {}): Promise<number> {
  return invokeCommand<number>("clear_alerting_incidents", { input });
}

export function snoozeAlertingIncident(input: AlertingSnoozeInputDto): Promise<void> {
  return invokeCommand<void>("snooze_alerting_incident", { input });
}

export function requestDesktopNotificationPermission(input: EmptyInputDto = {}): Promise<string> {
  return invokeCommand<string>("request_desktop_notification_permission", { input });
}

export function getDesktopNotificationPermission(input: EmptyInputDto = {}): Promise<string> {
  return invokeCommand<string>("get_desktop_notification_permission", { input });
}

export function getRuntimeContractInfo(): Promise<RuntimeContractInfo>"#,
        )
        .replace(
            r#"export function getRuntimeContractInfo(): Promise<RuntimeContractInfo> {
  return invokeCommand<RuntimeContractInfo>("get_runtime_contract_info");
}"#,
            r#"export function initializeRuntimeContext(input: EmptyInputDto = {}): Promise<string> {
  return invokeCommand<string>("initialize_runtime_context", { input });
}

export function recordFrontendBoundaryFailure(input: EmptyInputDto = {}): Promise<void> {
  return invokeCommand<void>("record_frontend_boundary_failure", { input });
}

export function readRuntimeDiagnostics(input: RuntimeDiagnosticsQueryDto = {}): Promise<RuntimeDiagnosticsPageDto> {
  return invokeCommand<RuntimeDiagnosticsPageDto>("read_runtime_diagnostics", { input });
}

export function exportRuntimeSupportBundle(input: EmptyInputDto = {}): Promise<RuntimeSupportBundleResultDto | null> {
  return invokeCommand<RuntimeSupportBundleResultDto | null>("export_runtime_support_bundle", { input });
}

export function openRuntimeLogDirectory(input: EmptyInputDto = {}): Promise<void> {
  return invokeCommand<void>("open_runtime_log_directory", { input });
}

export function openRuntimeLogFile(input: EmptyInputDto = {}): Promise<void> {
  return invokeCommand<void>("open_runtime_log_file", { input });
}

export function getRuntimeContractInfo(input: EmptyInputDto = {}): Promise<RuntimeContractInfo> {
  return invokeCommand<RuntimeContractInfo>("get_runtime_contract_info", { input });
}"#,
        );
    if !source.contains("export function getModelMappingWorkspace") {
        source = source.replace(
            r#"export function listModelAliases(input: EmptyInputDto = {}): Promise<ModelAliasDto[]> {"#,
            r#"export function getModelMappingWorkspace(input: EmptyInputDto = {}): Promise<ModelMappingWorkspaceDto> {
  return invokeCommand<ModelMappingWorkspaceDto>("get_model_mapping_workspace", { input });
}

export function getModelMappingDocument(input: EmptyInputDto = {}): Promise<ModelMappingDocumentDto> {
  return invokeCommand<ModelMappingDocumentDto>("get_model_mapping_document", { input });
}

export function validateModelMappingDocument(input: ValidateModelMappingDocumentInputDto): Promise<ModelMappingValidationResultDto> {
  return invokeCommand<ModelMappingValidationResultDto>("validate_model_mapping_document", { input });
}

export function applyModelMappingDocument(input: ApplyModelMappingDocumentInputDto): Promise<ModelMappingWorkspaceDto> {
  return invokeCommand<ModelMappingWorkspaceDto>("apply_model_mapping_document", { input });
}

export function restoreModelMappingRevision(input: RestoreModelMappingRevisionInputDto): Promise<ModelMappingWorkspaceDto> {
  return invokeCommand<ModelMappingWorkspaceDto>("restore_model_mapping_revision", { input });
}

export function simulateModelMapping(input: SimulateModelMappingInputDto): Promise<ModelMappingSimulationResultDto> {
  return invokeCommand<ModelMappingSimulationResultDto>("simulate_model_mapping", { input });
}

export function resolveRequestMappingTrace(input: ResolveRequestMappingTraceInputDto): Promise<ModelMappingTraceDto> {
  return invokeCommand<ModelMappingTraceDto>("resolve_request_mapping_trace", { input });
}

export function listModelAliases(input: EmptyInputDto = {}): Promise<ModelAliasDto[]> {"#,
        );
    }
    source
}

#[cfg(test)]
fn render_contract_typescript(contract_hash: &str) -> String {
    format!(
        "// @generated by repository IPC generator. Do not edit.\n// generator version: {GENERATOR_VERSION}\n// IPC contract version: {IPC_CONTRACT_VERSION}\n// canonical hash: {contract_hash}\n\n{RUNTIME_CONTRACT_TYPESCRIPT}\n\nexport const IPC_CONTRACT_VERSION = {IPC_CONTRACT_VERSION} as const;\nexport const IPC_BINDING_HASH = \"{contract_hash}\" as const;\n"
    )
}

#[cfg(test)]
fn render_registry(contract_hash: &str, fixture_hash: &str) -> String {
    let mut commands = COMMANDS
        .iter()
        .map(|command| {
            let contract = command_contract(command.name);
            RegistryCommand {
                name: command.name,
                transport: TransportKind::Ordinary,
                input_schema_hash: sha256(contract.input),
                output_schema_hash: sha256(contract.output),
                error_schema_hash: sha256(contract.error),
                mutation_kind: contract.mutation_kind,
                transport_retry: contract.transport_retry,
                result_unknown: contract.result_unknown,
                runtime_validation: contract.runtime_validation,
            }
        })
        .collect::<Vec<_>>();
    commands.sort_unstable_by_key(|command| command.name);
    let document = RegistryDocument {
        schema_version: 1,
        generator_version: GENERATOR_VERSION,
        ipc_contract_version: IPC_CONTRACT_VERSION,
        contract_hash,
        commands,
        streaming_surfaces: STREAMING_SURFACES,
        evidence: RegistryEvidence {
            kind: "compiled-rust-registry",
            serialization_fixture_hash: fixture_hash,
        },
    };
    format!(
        "{}\n",
        serde_json::to_string_pretty(&document).expect("registry must serialize")
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    #[test]
    fn pilot_serialization_matches_golden() {
        assert_eq!(
            pilot_serialization_fixture(),
            include_str!("dto/fixtures/pilot-serialization.json").replace("\r\n", "\n")
        );
    }

    #[test]
    fn compiled_registry_has_unique_command_identities() {
        let mut names = COMMANDS
            .iter()
            .map(|command| command.name)
            .collect::<Vec<_>>();
        let original_len = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), original_len);
        assert!(names.contains(&"get_settings"));
        assert!(names.contains(&"list_stations"));
        assert!(names.contains(&"get_runtime_contract_info"));
    }

    #[test]
    fn runtime_binding_hash_matches_the_compiled_contract() {
        assert_eq!(sha256(canonical_contract()), IPC_BINDING_HASH);
    }

    #[test]
    fn migrated_settings_station_commands_have_closed_schemas_and_no_transport_retry() {
        for name in [
            "get_runtime_contract_info",
            "get_settings",
            "list_stations",
            "update_settings",
            "create_station",
            "update_station",
            "delete_station",
            "reorder_stations",
        ] {
            let contract = command_contract(name);
            assert!(!contract.input.starts_with("legacy_"), "{name}");
            assert!(!contract.output.starts_with("legacy_"), "{name}");
            assert_eq!(
                contract.runtime_validation, "rust_dto_pre_application",
                "{name}"
            );
            assert!(!contract.transport_retry, "{name}");
        }
        assert!(command_contract("create_station").result_unknown);
    }

    #[test]
    fn station_key_ordinary_commands_have_closed_schemas_and_frozen_mutation_semantics() {
        let non_idempotent = [
            "create_local_station_key_from_remote",
            "create_remote_station_key",
            "create_station_key",
            "save_station_key_with_defaults",
        ];
        for name in [
            "bind_remote_station_key",
            "clear_station_credentials",
            "create_local_station_key_from_remote",
            "create_remote_station_key",
            "create_station_key",
            "delete_station_key",
            "get_remote_key_capability",
            "get_station_credentials",
            "list_key_pool_items",
            "list_remote_station_keys",
            "list_station_keys",
            "reorder_key_pool",
            "reorder_station_keys",
            "save_station_key_with_defaults",
            "scan_remote_station_keys",
            "unbind_remote_station_key",
            "update_station_credentials",
            "update_station_key",
            "update_station_key_group_binding",
            "update_station_session",
        ] {
            let contract = command_contract(name);
            assert!(!contract.input.starts_with("legacy_"), "{name}");
            assert!(!contract.output.starts_with("legacy_"), "{name}");
            assert_eq!(
                contract.runtime_validation, "rust_dto_pre_application",
                "{name}"
            );
            assert!(!contract.transport_retry, "{name}");
            assert_eq!(
                contract.result_unknown,
                non_idempotent.contains(&name),
                "{name}"
            );
        }
        assert_eq!(
            command_contract("scan_remote_station_keys").mutation_kind,
            "idempotent"
        );
    }

    #[test]
    fn request_log_commands_have_closed_schemas_and_frozen_mutation_semantics() {
        for name in ["clear_request_logs", "list_request_logs"] {
            let contract = command_contract(name);
            assert!(!contract.input.starts_with("legacy_"), "{name}");
            assert!(!contract.output.starts_with("legacy_"), "{name}");
            assert_eq!(
                contract.runtime_validation, "rust_dto_pre_application",
                "{name}"
            );
            assert!(!contract.transport_retry, "{name}");
            assert!(!contract.result_unknown, "{name}");
        }
    }

    #[test]
    fn collector_facts_snapshot_commands_have_closed_schemas_and_frozen_mutation_semantics() {
        for name in [
            "get_latest_collector_snapshot",
            "list_balance_snapshots",
            "list_balance_snapshots_for_station",
            "list_collector_runs",
            "list_collector_snapshots",
            "list_current_station_balance_snapshots",
            "list_group_rate_records",
            "list_station_group_bindings",
            "list_station_group_options",
            "upsert_balance_snapshot",
            "upsert_station_group_binding",
        ] {
            let contract = command_contract(name);
            assert!(!contract.input.starts_with("legacy_"), "{name}");
            assert!(!contract.output.starts_with("legacy_"), "{name}");
            assert_eq!(
                contract.runtime_validation, "rust_dto_pre_application",
                "{name}"
            );
            assert!(!contract.transport_retry, "{name}");
            assert!(!contract.result_unknown, "{name}");
        }
        for name in ["upsert_balance_snapshot", "upsert_station_group_binding"] {
            assert_eq!(command_contract(name).mutation_kind, "idempotent", "{name}");
        }
    }

    #[test]
    fn channel_monitor_read_commands_have_closed_schemas() {
        for name in [
            "get_channel_monitor_execution",
            "list_channel_monitor_attempts",
            "list_channel_monitor_executions",
            "list_channel_monitor_templates",
            "list_channel_monitors",
            "list_monitoring_capabilities",
        ] {
            let contract = command_contract(name);
            assert!(!contract.input.starts_with("legacy_"), "{name}");
            assert!(!contract.output.starts_with("legacy_"), "{name}");
            assert_eq!(contract.mutation_kind, "read", "{name}");
            assert_eq!(
                contract.runtime_validation, "rust_dto_pre_application",
                "{name}"
            );
            assert!(!contract.transport_retry, "{name}");
            assert!(!contract.result_unknown, "{name}");
        }
    }

    #[test]
    fn channel_monitor_mutations_have_closed_schemas_and_frozen_semantics() {
        let non_idempotent = [
            "create_channel_monitor",
            "create_channel_monitor_template",
            "duplicate_channel_monitor_template",
        ];
        for name in [
            "create_channel_monitor",
            "update_channel_monitor",
            "delete_channel_monitor",
            "create_channel_monitor_template",
            "update_channel_monitor_template",
            "duplicate_channel_monitor_template",
            "delete_channel_monitor_template",
        ] {
            let contract = command_contract(name);
            assert!(!contract.input.starts_with("legacy_"), "{name}");
            assert!(!contract.output.starts_with("legacy_"), "{name}");
            assert_eq!(
                contract.runtime_validation, "rust_dto_pre_application",
                "{name}"
            );
            assert!(!contract.transport_retry, "{name}");
            assert_eq!(
                contract.result_unknown,
                non_idempotent.contains(&name),
                "{name}"
            );
            assert_eq!(
                contract.mutation_kind,
                if non_idempotent.contains(&name) {
                    "non_idempotent"
                } else {
                    "idempotent"
                },
                "{name}"
            );
        }
    }

    #[test]
    fn channel_monitor_operations_have_closed_schemas_and_frozen_semantics() {
        let workspace = command_contract("load_channel_status_workspace");
        assert_eq!(workspace.input, "ChannelStatusWorkspaceInputDto");
        assert_eq!(workspace.output, "ChannelStatusWorkspaceDto");
        assert_eq!(workspace.mutation_kind, "read");
        assert_eq!(workspace.runtime_validation, "rust_dto_pre_application");
        assert!(!workspace.transport_retry);
        assert!(!workspace.result_unknown);

        let run_now = command_contract("run_channel_monitor_now");
        assert_eq!(run_now.input, "RunChannelMonitorNowInputDto");
        assert_eq!(run_now.output, "RunChannelMonitorReceiptDto");
        assert_eq!(run_now.mutation_kind, "non_idempotent");
        assert_eq!(run_now.runtime_validation, "rust_dto_pre_application");
        assert!(!run_now.transport_retry);
        assert!(run_now.result_unknown);

        let cancel = command_contract("cancel_channel_monitor_execution");
        assert_eq!(cancel.input, "CancelChannelMonitorExecutionInputDto");
        assert_eq!(cancel.output, "CancelChannelMonitorExecutionReceiptDto");
        assert_eq!(cancel.mutation_kind, "idempotent");
        assert_eq!(cancel.runtime_validation, "rust_dto_pre_application");
        assert!(!cancel.transport_retry);
        assert!(!cancel.result_unknown);
    }

    #[test]
    fn station_collector_operations_have_closed_schemas_and_frozen_semantics() {
        for name in [
            "detect_sub2api_station",
            "collect_sub2api_station",
            "detect_station_info",
            "collect_station_info",
            "collect_station_task",
            "scan_station_recharge",
            "test_station_login",
        ] {
            let contract = command_contract(name);
            assert!(!contract.input.starts_with("legacy_"), "{name}");
            assert_eq!(contract.output, "CollectorRunResultDto", "{name}");
            assert_eq!(contract.mutation_kind, "non_idempotent", "{name}");
            assert_eq!(
                contract.runtime_validation, "rust_dto_pre_application",
                "{name}"
            );
            assert!(!contract.transport_retry, "{name}");
            assert!(contract.result_unknown, "{name}");
        }

        let login_input = command_contract("test_station_login_input");
        assert_eq!(login_input.input, "StationLoginTestInputDto");
        assert_eq!(login_input.output, "StationLoginTestResultDto");
        assert_eq!(login_input.mutation_kind, "read");
        assert_eq!(login_input.runtime_validation, "rust_dto_pre_application");
        assert!(!login_input.transport_retry);
        assert!(!login_input.result_unknown);
    }

    #[test]
    fn routing_health_reads_have_closed_schemas_and_read_semantics() {
        for (name, input, output) in [
            (
                "get_station_key_capabilities",
                "RoutingStationKeyIdInputDto",
                "StationKeyCapabilitiesDto",
            ),
            ("list_model_aliases", "EmptyInputDto", "Vec<ModelAliasDto>"),
            (
                "list_station_key_health",
                "EmptyInputDto",
                "Vec<StationKeyHealthDto>",
            ),
            (
                "get_routing_protection_status",
                "RoutingProtectionStatusInputDto",
                "RoutingProtectionStatusDto",
            ),
            (
                "list_station_endpoint_health",
                "EmptyInputDto",
                "Vec<StationEndpointHealthDto>",
            ),
            (
                "load_routing_workspace_snapshot",
                "RoutingWorkspaceSnapshotInputDto",
                "RoutingWorkspaceSnapshotDto",
            ),
            (
                "load_routing_runtime_overlay",
                "EmptyInputDto",
                "RoutingRuntimeOverlayDto",
            ),
            (
                "list_recent_route_decisions",
                "RecentRouteDecisionsInputDto",
                "RecentRouteDecisionsPageDto",
            ),
            (
                "get_station_key_operational_detail",
                "StationKeyOperationalDetailInputDto",
                "StationKeyOperationalDetailDto",
            ),
            (
                "get_request_decision_trace",
                "RequestDecisionTraceInputDto",
                "RequestDecisionTraceDto",
            ),
            (
                "get_station_key_health",
                "RoutingStationKeyIdInputDto",
                "StationKeyHealthDto",
            ),
            (
                "simulate_route",
                "RouteSimulationInputDto",
                "RouteSimulationResultDto",
            ),
        ] {
            let contract = command_contract(name);
            assert_eq!(contract.input, input, "{name}");
            assert_eq!(contract.output, output, "{name}");
            assert_eq!(contract.mutation_kind, "read", "{name}");
            assert_eq!(
                contract.runtime_validation, "rust_dto_pre_application",
                "{name}"
            );
            assert!(!contract.transport_retry, "{name}");
            assert!(!contract.result_unknown, "{name}");
        }
    }

    #[test]
    fn routing_mutations_have_closed_schemas_and_idempotent_semantics() {
        for (name, input, output) in [
            (
                "update_station_key_capabilities",
                "UpdateStationKeyCapabilitiesInputDto",
                "StationKeyCapabilitiesDto",
            ),
            (
                "upsert_model_alias",
                "UpsertModelAliasInputDto",
                "ModelAliasDto",
            ),
            ("delete_model_alias", "DeleteModelAliasInputDto", "unit"),
        ] {
            let contract = command_contract(name);
            assert_eq!(contract.input, input, "{name}");
            assert_eq!(contract.output, output, "{name}");
            assert_eq!(contract.mutation_kind, "idempotent", "{name}");
            assert_eq!(
                contract.runtime_validation, "rust_dto_pre_application",
                "{name}"
            );
            assert!(!contract.transport_retry, "{name}");
            assert!(!contract.result_unknown, "{name}");
        }
    }

    #[test]
    fn pricing_reads_have_closed_schemas_and_read_semantics() {
        for (name, input, output) in [
            (
                "list_model_base_prices",
                "EmptyInputDto",
                "Vec<ModelBasePriceDto>",
            ),
            (
                "resolve_station_key_pricing_context",
                "PricingContextInputDto",
                "ResolvedPricingContextDto",
            ),
            (
                "load_pricing_comparison_workspace",
                "EmptyInputDto",
                "PricingComparisonWorkspaceDto",
            ),
            (
                "load_pricing_group_monitor_status",
                "PricingGroupMonitorStatusInputDto",
                "PricingGroupMonitorStatusWorkspaceDto",
            ),
        ] {
            let contract = command_contract(name);
            assert_eq!(contract.input, input, "{name}");
            assert_eq!(contract.output, output, "{name}");
            assert_eq!(contract.mutation_kind, "read", "{name}");
            assert_eq!(
                contract.runtime_validation, "rust_dto_pre_application",
                "{name}"
            );
            assert!(!contract.transport_retry, "{name}");
            assert!(!contract.result_unknown, "{name}");
        }
    }

    #[test]
    fn pricing_mutations_have_closed_schemas_and_idempotent_semantics() {
        for (name, input, output) in [
            (
                "upsert_model_base_price",
                "UpsertModelBasePriceInputDto",
                "ModelBasePriceDto",
            ),
            (
                "reset_model_base_prices_to_builtins",
                "EmptyInputDto",
                "Vec<ModelBasePriceDto>",
            ),
        ] {
            let contract = command_contract(name);
            assert_eq!(contract.input, input, "{name}");
            assert_eq!(contract.output, output, "{name}");
            assert_eq!(contract.mutation_kind, "idempotent", "{name}");
            assert_eq!(
                contract.runtime_validation, "rust_dto_pre_application",
                "{name}"
            );
            assert!(!contract.transport_retry, "{name}");
            assert!(!contract.result_unknown, "{name}");
        }
    }

    #[test]
    fn proxy_workspace_reads_have_closed_schemas_and_read_semantics() {
        for (name, output) in [("get_proxy_status", "ProxyStatusDto")] {
            let contract = command_contract(name);
            assert_eq!(contract.input, "EmptyInputDto", "{name}");
            assert_eq!(contract.output, output, "{name}");
            assert_eq!(contract.mutation_kind, "read", "{name}");
            assert_eq!(
                contract.runtime_validation, "rust_dto_pre_application",
                "{name}"
            );
            assert!(!contract.transport_retry, "{name}");
            assert!(!contract.result_unknown, "{name}");
        }
    }

    #[test]
    fn endpoint_ping_has_closed_schemas_and_non_idempotent_semantics() {
        let contract = command_contract("ping_station_endpoint");
        assert_eq!(contract.input, "StationIdInputDto");
        assert_eq!(contract.output, "EndpointPingResultDto");
        assert_eq!(contract.mutation_kind, "non_idempotent");
        assert_eq!(contract.runtime_validation, "rust_dto_pre_application");
        assert!(!contract.transport_retry);
        assert!(contract.result_unknown);
    }

    #[test]
    fn operation_commands_have_closed_schemas_and_status_cancel_semantics() {
        let status = command_contract("get_operation_status");
        assert_eq!(status.input, "OperationIdInputDto");
        assert_eq!(status.output, "OperationSnapshotDto");
        assert_eq!(status.mutation_kind, "read");
        assert_eq!(status.runtime_validation, "rust_dto_pre_application");
        assert!(!status.transport_retry);
        assert!(!status.result_unknown);

        let cancel = command_contract("cancel_operation");
        assert_eq!(cancel.input, "CancelOperationInputDto");
        assert_eq!(cancel.output, "CancelOperationOutcomeDto");
        assert_eq!(cancel.mutation_kind, "idempotent");
        assert_eq!(cancel.runtime_validation, "rust_dto_pre_application");
        assert!(!cancel.transport_retry);
        assert!(!cancel.result_unknown);

        let start_connectivity = command_contract("start_station_key_connectivity_operation");
        assert_eq!(start_connectivity.input, "StationKeyConnectivityInputDto");
        assert_eq!(start_connectivity.output, "OperationStartedDto");
        assert_eq!(start_connectivity.mutation_kind, "non_idempotent");
        assert_eq!(
            start_connectivity.runtime_validation,
            "rust_dto_pre_application"
        );
        assert!(!start_connectivity.transport_retry);
        assert!(start_connectivity.result_unknown);

        let connectivity_result = command_contract("get_station_key_connectivity_operation_result");
        assert_eq!(connectivity_result.input, "OperationIdInputDto");
        assert_eq!(
            connectivity_result.output,
            "StationKeyConnectivityResultDto"
        );
        assert_eq!(connectivity_result.mutation_kind, "read");
        assert_eq!(
            connectivity_result.runtime_validation,
            "rust_dto_pre_application"
        );
        assert!(!connectivity_result.transport_retry);
        assert!(!connectivity_result.result_unknown);
    }

    #[test]
    fn proxy_start_has_a_closed_schema_and_idempotent_semantics() {
        let contract = command_contract("start_local_proxy");
        assert_eq!(contract.input, "EmptyInputDto");
        assert_eq!(contract.output, "ProxyStatusDto");
        assert_eq!(contract.mutation_kind, "idempotent");
        assert_eq!(contract.runtime_validation, "rust_dto_pre_application");
        assert!(!contract.transport_retry);
        assert!(!contract.result_unknown);
    }

    #[test]
    fn proxy_stop_has_a_closed_schema_and_idempotent_semantics() {
        let contract = command_contract("stop_local_proxy");
        assert_eq!(contract.input, "EmptyInputDto");
        assert_eq!(contract.output, "ProxyStatusDto");
        assert_eq!(contract.mutation_kind, "idempotent");
        assert_eq!(contract.runtime_validation, "rust_dto_pre_application");
        assert!(!contract.transport_retry);
        assert!(!contract.result_unknown);
    }

    #[test]
    fn proxy_restart_has_a_closed_schema_and_non_idempotent_semantics() {
        let contract = command_contract("restart_local_proxy");
        assert_eq!(contract.input, "EmptyInputDto");
        assert_eq!(contract.output, "ProxyStatusDto");
        assert_eq!(contract.mutation_kind, "non_idempotent");
        assert_eq!(contract.runtime_validation, "rust_dto_pre_application");
        assert!(!contract.transport_retry);
        assert!(contract.result_unknown);
    }

    #[test]
    fn updater_backend_reads_have_closed_schemas() {
        let network = command_contract("updater_network_config");
        assert_eq!(network.input, "EmptyInputDto");
        assert_eq!(network.output, "UpdaterNetworkConfigDto");
        assert_eq!(network.mutation_kind, "read");
        assert_eq!(network.runtime_validation, "rust_dto_pre_application");
        assert!(!network.transport_retry);
        assert!(!network.result_unknown);

        let manifest = command_contract("inspect_latest_update_manifest");
        assert_eq!(manifest.input, "PublishedUpdateInspectionInputDto");
        assert_eq!(manifest.output, "PublishedUpdateInspectionDto");
        assert_eq!(manifest.mutation_kind, "read");
        assert_eq!(manifest.runtime_validation, "rust_dto_pre_application");
        assert!(!manifest.transport_retry);
        assert!(!manifest.result_unknown);
    }

    #[test]
    fn application_restart_is_a_non_idempotent_lifecycle_command() {
        let contract = command_contract("restart_application");
        assert_eq!(contract.input, "EmptyInputDto");
        assert_eq!(contract.output, "unit");
        assert_eq!(contract.mutation_kind, "non_idempotent");
        assert_eq!(contract.runtime_validation, "rust_dto_pre_application");
        assert!(!contract.transport_retry);
        assert!(contract.result_unknown);
    }

    #[test]
    fn runtime_diagnostics_surface_requires_a_developer_gate() {
        let status = command_contract("get_runtime_status");
        assert_eq!(status.input, "EmptyInputDto");
        assert_eq!(status.output, "RuntimeStatusDto");
        assert_eq!(status.mutation_kind, "read");
        assert_eq!(status.runtime_validation, "rust_dto_pre_application");
        assert!(!status.transport_retry);
        assert!(!status.result_unknown);

        let mut diagnostics_commands = 0;
        for command in COMMANDS {
            let contract = command_contract(command.name);
            assert!(
                !matches!(
                    contract.output,
                    "RuntimeDiagnostics"
                        | "RuntimeDiagnosticsDto"
                        | "RuntimeDiagnosticsSnapshot"
                        | "MetricSnapshot"
                ),
                "{} exposes full runtime diagnostics as an IPC output",
                command.name
            );
            let diagnostics_like = command.name.contains("runtime_diagnostic")
                || command.name.contains("runtime_diagnostics")
                || command.name.contains("support_bundle")
                || command.name == "record_frontend_boundary_failure";
            if diagnostics_like {
                diagnostics_commands += 1;
                assert!(
                    matches!(
                        command.name,
                        "read_runtime_diagnostics"
                            | "export_runtime_support_bundle"
                            | "record_frontend_boundary_failure"
                    ),
                    "unexpected runtime diagnostics command: {}",
                    command.name
                );
                if matches!(
                    command.name,
                    "read_runtime_diagnostics" | "export_runtime_support_bundle"
                ) {
                    assert_eq!(
                        contract.runtime_validation, "developer_mode_gate",
                        "{} must enforce developer mode before reading/exporting diagnostics",
                        command.name
                    );
                } else {
                    assert_eq!(contract.runtime_validation, "rust_dto_pre_application");
                }
            }
        }
        assert_eq!(diagnostics_commands, 3);
    }

    #[test]
    fn generated_bindings_use_the_common_transport_and_dedicated_wrappers() {
        let source = render_typescript("fixture-hash");
        assert!(source.contains("@/lib/bridge/transport"));
        assert!(!source.contains("@tauri-apps/api/core"));
        assert!(source.contains(r#"invokeNonIdempotent<StationDto>("create_station""#));
        assert!(source.contains(
            r#"function getRuntimeContractInfo(input: EmptyInputDto = {}): Promise<RuntimeContractInfo>"#
        ));
        assert!(source.contains(
            r#"function getRuntimeStatus(input: EmptyInputDto = {}): Promise<RuntimeStatusDto>"#
        ));
        assert!(source.contains("export type OperationEventDto = {"));
        for wrapper in [
            "updateSettings",
            "createStation",
            "updateStation",
            "deleteStation",
            "reorderStations",
        ] {
            assert!(
                source.contains(&format!("function {wrapper}(")),
                "{wrapper}"
            );
        }
        for wrapper in [
            "bindRemoteStationKey",
            "clearStationCredentials",
            "createLocalStationKeyFromRemote",
            "createRemoteStationKey",
            "createStationKey",
            "deleteStationKey",
            "getRemoteKeyCapability",
            "getStationCredentials",
            "listKeyPoolItems",
            "listRemoteStationKeys",
            "listStationKeys",
            "reorderKeyPool",
            "reorderStationKeys",
            "saveStationKeyWithDefaults",
            "scanRemoteStationKeys",
            "unbindRemoteStationKey",
            "updateStationCredentials",
            "updateStationKey",
            "updateStationKeyGroupBinding",
            "updateStationSession",
        ] {
            assert!(
                source.contains(&format!("function {wrapper}(")),
                "{wrapper}"
            );
        }
        for wrapper in [
            "getLatestCollectorSnapshot",
            "listBalanceSnapshots",
            "listBalanceSnapshotsForStation",
            "listCollectorRuns",
            "listCollectorSnapshots",
            "listCurrentStationBalanceSnapshots",
            "listGroupRateRecords",
            "listStationGroupBindings",
            "listStationGroupOptions",
            "upsertBalanceSnapshot",
            "upsertStationGroupBinding",
        ] {
            assert!(
                source.contains(&format!("function {wrapper}(")),
                "{wrapper}"
            );
        }
        for wrapper in [
            "listChannelMonitorTemplates",
            "listChannelMonitors",
            "listChannelMonitorExecutions",
            "getChannelMonitorExecution",
            "listChannelMonitorAttempts",
            "listMonitoringCapabilities",
            "createChannelMonitor",
            "updateChannelMonitor",
            "deleteChannelMonitor",
            "createChannelMonitorTemplate",
            "updateChannelMonitorTemplate",
            "duplicateChannelMonitorTemplate",
            "deleteChannelMonitorTemplate",
            "cancelChannelMonitorExecution",
        ] {
            assert!(
                source.contains(&format!("function {wrapper}(")),
                "{wrapper}"
            );
        }
        for (command, output) in [
            ("create_channel_monitor", "ChannelMonitorDto"),
            (
                "create_channel_monitor_template",
                "ChannelMonitorRequestTemplateDto",
            ),
            (
                "duplicate_channel_monitor_template",
                "ChannelMonitorRequestTemplateDto",
            ),
            ("run_channel_monitor_now", "RunChannelMonitorReceiptDto"),
            ("detect_sub2api_station", "CollectorRunResultDto"),
            ("collect_sub2api_station", "CollectorRunResultDto"),
            ("detect_station_info", "CollectorRunResultDto"),
            ("collect_station_info", "CollectorRunResultDto"),
            ("scan_station_recharge", "CollectorRunResultDto"),
            ("collect_station_task", "CollectorRunResultDto"),
            ("test_station_login", "CollectorRunResultDto"),
        ] {
            assert!(
                source.contains(&format!("invokeNonIdempotent<{output}>(\"{command}\"")),
                "{command}"
            );
        }
        assert!(source.contains("function loadChannelStatusWorkspace("));
        assert!(source.contains("function testStationLoginInput("));
        for function in [
            "resolveAllAlertingIncidents",
            "clearAlertingIncidents",
            "getStationKeyCapabilities",
            "listModelAliases",
            "listStationKeyHealth",
            "listStationEndpointHealth",
            "getStationKeyHealth",
            "simulateRoute",
        ] {
            assert!(
                source.contains(&format!("function {function}(")),
                "{function}"
            );
        }
        for function in [
            "updateStationKeyCapabilities",
            "upsertModelAlias",
            "deleteModelAlias",
        ] {
            assert!(
                source.contains(&format!("function {function}(")),
                "{function}"
            );
        }
        for function in ["upsertModelBasePrice", "resetModelBasePricesToBuiltins"] {
            assert!(
                source.contains(&format!("function {function}(")),
                "{function}"
            );
        }
    }

    #[test]
    fn emit_repository_bindings() {
        let Some(output_dir) = std::env::var_os("RELAY_POOL_BINDINGS_OUT") else {
            return;
        };
        let output_dir = PathBuf::from(output_dir);
        fs::create_dir_all(&output_dir).expect("generator output directory must be created");
        let canonical = canonical_contract();
        let contract_hash = sha256(canonical);
        assert_eq!(
            contract_hash, IPC_BINDING_HASH,
            "runtime binding hash is stale; update IPC_BINDING_HASH"
        );
        let fixture = pilot_serialization_fixture();
        let fixture_hash = sha256(fixture.as_bytes());
        fs::write(
            output_dir.join("generated.ts"),
            render_typescript(&contract_hash),
        )
        .expect("TypeScript binding must be written");
        fs::write(
            output_dir.join("contract.ts"),
            render_contract_typescript(&contract_hash),
        )
        .expect("TypeScript contract must be written");
        fs::write(
            output_dir.join("command-registry.json"),
            render_registry(&contract_hash, &fixture_hash),
        )
        .expect("command registry must be written");
        fs::write(output_dir.join("pilot-serialization.json"), fixture)
            .expect("serialization fixture must be written");
    }
}
