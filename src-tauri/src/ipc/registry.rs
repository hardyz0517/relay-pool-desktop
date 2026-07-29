use serde::Serialize;
#[cfg(test)]
use sha2::{Digest, Sha256};

#[cfg(test)]
use super::dto::REGISTERED_TYPES;
#[cfg(test)]
use super::runtime_contract::RUNTIME_CONTRACT_TYPESCRIPT;

#[cfg_attr(not(test), allow(dead_code))]
pub const GENERATOR_VERSION: u32 = 1;
#[cfg_attr(not(test), allow(dead_code))]
pub const IPC_CONTRACT_VERSION: u32 = 1;
// Updated by `pnpm generate:bindings` whenever the compiled command/type contract changes.
pub const IPC_BINDING_HASH: &str =
    "51309a9ca7e305d0940ea679d4907970b0a0cc025f2f9e536187cdd240709571";

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
pub struct CommandDescriptor {
    pub name: &'static str,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    Ordinary,
    Channel,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, Serialize)]
pub struct StreamingSurface {
    pub command: &'static str,
    pub event: &'static str,
    pub event_schema_version: u32,
    pub transport: TransportKind,
}

#[cfg_attr(not(test), allow(dead_code))]
pub const STREAMING_SURFACES: &[StreamingSurface] = &[StreamingSurface {
    command: "test_station_key_connectivity",
    event: "StationKeyConnectivityTestEvent",
    event_schema_version: 1,
    transport: TransportKind::Channel,
}];

#[macro_export]
macro_rules! ipc_command_registry {
    ($consumer:ident) => {
        $consumer! {
            app_status => $crate::commands::runtime::app_status,
            get_runtime_contract_info => $crate::commands::runtime::get_runtime_contract_info,
            get_runtime_status => $crate::commands::runtime::get_runtime_status,
            get_data_store_startup_state => $crate::commands::data_store_startup::get_data_store_startup_state,
            refresh_data_store_candidates => $crate::commands::data_store_startup::refresh_data_store_candidates,
            locate_data_store_candidate => $crate::commands::data_store_startup::locate_data_store_candidate,
            activate_data_store_candidate => $crate::commands::data_store_startup::activate_data_store_candidate,
            create_new_data_store => $crate::commands::data_store_startup::create_new_data_store,
            open_data_store_backup_dir => $crate::commands::data_store_startup::open_data_store_backup_dir,
            export_data_store_diagnostic => $crate::commands::data_store_startup::export_data_store_diagnostic,
            list_stations => $crate::commands::stations::list_stations,
            create_station => $crate::commands::stations::create_station,
            update_station => $crate::commands::stations::update_station,
            delete_station => $crate::commands::stations::delete_station,
            reorder_stations => $crate::commands::stations::reorder_stations,
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
            load_local_routing_workspace => $crate::commands::local_proxy::load_local_routing_workspace,
            reorder_local_routing_keys => $crate::commands::local_proxy::reorder_local_routing_keys,
            start_local_proxy => $crate::commands::local_proxy::start_local_proxy,
            stop_local_proxy => $crate::commands::local_proxy::stop_local_proxy,
            cleanup_before_update => $crate::commands::local_proxy::cleanup_before_update,
            prepare_local_proxy_for_update => $crate::commands::local_proxy::prepare_local_proxy_for_update,
            restart_local_proxy => $crate::commands::local_proxy::restart_local_proxy,
            list_request_logs => $crate::commands::request_logs::list_request_logs,
            clear_request_logs => $crate::commands::request_logs::clear_request_logs,
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
            list_station_key_health => $crate::commands::routing_health::list_station_key_health,
            list_station_endpoint_health => $crate::commands::routing_health::list_station_endpoint_health,
            list_channel_monitors => $crate::commands::channel_monitoring::list_channel_monitors,
            list_channel_monitor_summaries => $crate::commands::channel_monitoring::list_channel_monitor_summaries,
            list_channel_status_summaries => $crate::commands::channel_status::list_channel_status_summaries,
            load_channel_status_workspace => $crate::commands::channel_status::load_channel_status_workspace,
            load_pricing_comparison_workspace => $crate::commands::pricing_workspace::load_pricing_comparison_workspace,
            create_channel_monitor => $crate::commands::channel_monitoring::create_channel_monitor,
            update_channel_monitor => $crate::commands::channel_monitoring::update_channel_monitor,
            delete_channel_monitor => $crate::commands::channel_monitoring::delete_channel_monitor,
            list_channel_monitor_runs => $crate::commands::channel_monitoring::list_channel_monitor_runs,
            list_channel_monitor_templates => $crate::commands::channel_monitoring::list_channel_monitor_templates,
            create_channel_monitor_template => $crate::commands::channel_monitoring::create_channel_monitor_template,
            update_channel_monitor_template => $crate::commands::channel_monitoring::update_channel_monitor_template,
            duplicate_channel_monitor_template => $crate::commands::channel_monitoring::duplicate_channel_monitor_template,
            delete_channel_monitor_template => $crate::commands::channel_monitoring::delete_channel_monitor_template,
            run_channel_monitor_now => $crate::commands::channel_monitoring::run_channel_monitor_now,
            get_station_key_health => $crate::commands::routing_health::get_station_key_health,
            get_operation_status => $crate::commands::operations::get_operation_status,
            cancel_operation => $crate::commands::operations::cancel_operation,
            start_station_key_connectivity_operation => $crate::commands::station_key_connectivity::start_station_key_connectivity_operation,
            ping_station_endpoint => $crate::commands::endpoint_ping::ping_station_endpoint,
            test_station_key_connectivity => $crate::commands::station_key_connectivity::test_station_key_connectivity,
            simulate_route => $crate::commands::routing_health::simulate_route,
            list_pricing_rules => $crate::commands::pricing::list_pricing_rules,
            list_model_base_prices => $crate::commands::pricing::list_model_base_prices,
            upsert_model_base_price => $crate::commands::pricing::upsert_model_base_price,
            reset_model_base_prices_to_builtins => $crate::commands::pricing::reset_model_base_prices_to_builtins,
            upsert_pricing_rule => $crate::commands::pricing::upsert_pricing_rule,
            delete_pricing_rule => $crate::commands::pricing::delete_pricing_rule,
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
            list_change_events => $crate::commands::change_events::list_change_events,
            clear_change_events => $crate::commands::change_events::clear_change_events,
            list_change_events_for_station => $crate::commands::change_events::list_change_events_for_station,
            upsert_change_event => $crate::commands::change_events::upsert_change_event,
            mark_change_event_read => $crate::commands::change_events::mark_change_event_read,
            mark_change_events_read => $crate::commands::change_events::mark_change_events_read,
            dismiss_change_event => $crate::commands::change_events::dismiss_change_event,
            resolve_change_event => $crate::commands::change_events::resolve_change_event,
            get_station_credentials => $crate::commands::credentials::get_station_credentials,
            update_station_credentials => $crate::commands::credentials::update_station_credentials,
            update_station_session => $crate::commands::credentials::update_station_session,
            clear_station_credentials => $crate::commands::credentials::clear_station_credentials,
            list_common_login_profiles => $crate::commands::credentials::list_common_login_profiles,
            upsert_common_login_profile => $crate::commands::credentials::upsert_common_login_profile,
            delete_common_login_profile => $crate::commands::credentials::delete_common_login_profile,
            get_common_login_profile_password => $crate::commands::credentials::get_common_login_profile_password,
            detect_station_info => $crate::commands::station_collection::detect_station_info,
            collect_station_info => $crate::commands::station_collection::collect_station_info,
            collect_station_task => $crate::commands::station_collection::collect_station_task,
            test_station_login => $crate::commands::station_collection::test_station_login,
            test_station_login_input => $crate::commands::station_collection::test_station_login_input,
            detect_sub2api_station => $crate::commands::station_collection::detect_sub2api_station,
            collect_sub2api_station => $crate::commands::station_collection::collect_sub2api_station,
            list_collector_snapshots => $crate::commands::collector_metadata::list_collector_snapshots,
            get_latest_collector_snapshot => $crate::commands::collector_metadata::get_latest_collector_snapshot,
            list_latest_collector_snapshots => $crate::commands::collector_metadata::list_latest_collector_snapshots,
            start_capture_session => $crate::commands::capture::start_capture_session,
            get_capture_session_status => $crate::commands::capture::get_capture_session_status,
            record_capture_event => $crate::commands::capture::record_capture_event,
            finish_capture_session => $crate::commands::capture::finish_capture_session,
            finish_web_authorization_session => $crate::commands::capture::finish_web_authorization_session,
            clear_capture_session => $crate::commands::capture::clear_capture_session,
            close_capture_session => $crate::commands::capture::close_capture_session,
        }
    };
}

macro_rules! compile_descriptors {
    ($( $name:ident => $handler:path, )*) => {
        #[cfg_attr(not(test), allow(dead_code))]
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
        "get_runtime_status" => migrated_read("EmptyInputDto", "RuntimeStatusDto"),
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
        "list_common_login_profiles" => {
            migrated_read("EmptyInputDto", "Vec<CommonLoginProfileDto>")
        }
        "get_common_login_profile_password" => {
            migrated_read("CommonLoginProfileIdInputDto", "String")
        }
        "upsert_common_login_profile" => migrated_mutation(
            "UpsertCommonLoginProfileInputDto",
            "CommonLoginProfileDto",
            "non_idempotent",
            true,
        ),
        "delete_common_login_profile" => {
            migrated_mutation("CommonLoginProfileIdInputDto", "unit", "idempotent", false)
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
        "list_change_events" => migrated_read("EmptyInputDto", "Vec<ChangeEventDto>"),
        "clear_change_events" => migrated_mutation("EmptyInputDto", "unit", "idempotent", false),
        "list_change_events_for_station" => {
            migrated_read("ChangeLogStationIdInputDto", "Vec<ChangeEventDto>")
        }
        "upsert_change_event" => migrated_mutation(
            "UpsertChangeEventInputDto",
            "ChangeEventDto",
            "idempotent",
            false,
        ),
        "mark_change_event_read" | "dismiss_change_event" | "resolve_change_event" => {
            migrated_mutation(
                "ChangeEventIdInputDto",
                "ChangeEventDto",
                "idempotent",
                false,
            )
        }
        "mark_change_events_read" => migrated_mutation(
            "ChangeEventIdsInputDto",
            "Vec<ChangeEventDto>",
            "idempotent",
            false,
        ),
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
        "list_channel_monitor_summaries" => migrated_read(
            "ChannelMonitorSummaryInputDto",
            "Vec<ChannelMonitorSummaryDto>",
        ),
        "list_channel_status_summaries" => {
            migrated_read("EmptyInputDto", "Vec<ChannelStatusSummaryDto>")
        }
        "list_channel_monitor_runs" => {
            migrated_read("ChannelMonitorIdInputDto", "Vec<ChannelMonitorRunDto>")
        }
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
        "load_channel_status_workspace" => {
            migrated_read("EmptyInputDto", "ChannelStatusWorkspaceDto")
        }
        "run_channel_monitor_now" => migrated_mutation(
            "ChannelMonitorIdInputDto",
            "Vec<ChannelMonitorRunDto>",
            "non_idempotent",
            true,
        ),
        "detect_sub2api_station"
        | "collect_sub2api_station"
        | "detect_station_info"
        | "collect_station_info"
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
        "finish_capture_session" | "finish_web_authorization_session" => migrated_mutation(
            "CaptureStationIdInputDto",
            "CollectorRunResultDto",
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
        "list_station_key_health" => migrated_read("EmptyInputDto", "Vec<StationKeyHealthDto>"),
        "list_station_endpoint_health" => {
            migrated_read("EmptyInputDto", "Vec<StationEndpointHealthDto>")
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
        "ping_station_endpoint" => migrated_mutation(
            "StationIdInputDto",
            "EndpointPingResultDto",
            "non_idempotent",
            true,
        ),
        "simulate_route" => migrated_read("RouteSimulationInputDto", "RouteSimulationResultDto"),
        "list_pricing_rules" => migrated_read("EmptyInputDto", "Vec<PricingRuleDto>"),
        "list_model_base_prices" => migrated_read("EmptyInputDto", "Vec<ModelBasePriceDto>"),
        "resolve_station_key_pricing_context" => {
            migrated_read("PricingContextInputDto", "ResolvedPricingContextDto")
        }
        "load_pricing_comparison_workspace" => {
            migrated_read("EmptyInputDto", "PricingComparisonWorkspaceDto")
        }
        "upsert_model_base_price" => migrated_mutation(
            "UpsertModelBasePriceInputDto",
            "ModelBasePriceDto",
            "idempotent",
            false,
        ),
        "reset_model_base_prices_to_builtins" => migrated_mutation(
            "EmptyInputDto",
            "Vec<ModelBasePriceDto>",
            "idempotent",
            false,
        ),
        "upsert_pricing_rule" => migrated_mutation(
            "UpsertPricingRuleInputDto",
            "PricingRuleDto",
            "idempotent",
            false,
        ),
        "delete_pricing_rule" => {
            migrated_mutation("PricingRuleIdInputDto", "unit", "idempotent", false)
        }
        "get_proxy_status" => migrated_read("EmptyInputDto", "ProxyStatusDto"),
        "load_local_routing_workspace" => {
            migrated_read("EmptyInputDto", "LocalRoutingWorkspaceDto")
        }
        "reorder_local_routing_keys" => migrated_mutation(
            "ReorderLocalRoutingKeysInputDto",
            "LocalRoutingWorkspaceDto",
            "idempotent",
            false,
        ),
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
        "test_station_key_connectivity" => migrated_mutation(
            "StationKeyConnectivityInputDto",
            "StationKeyConnectivityTestResult",
            "non_idempotent",
            true,
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
        default_routing_strategy: "automatic_balanced".into(),
        collector_proxy_mode: "direct".into(),
        collector_proxy_url: None,
        max_rate_multiplier: None,
        default_routing_group_filter: Default::default(),
        scheduler_advanced_settings: Default::default(),
        low_balance_threshold_cny: 15.0,
        collector_interval_minutes: 30,
        balance_interval_minutes: 5,
        group_rate_interval_minutes: 20,
        model_list_interval_minutes: 60,
        pricing_refresh_interval_minutes: 60,
        collector_timeout_seconds: 15,
        collector_max_concurrency: 3,
        allow_depleted_fallback: false,
        developer_mode_enabled: false,
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
        "groupRateIntervalMinutes": 20, "modelListIntervalMinutes": 60,
        "pricingRefreshIntervalMinutes": 60, "collectorTimeoutSeconds": 15,
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
        serde_json::json!({"command": "update_settings", "input": update_settings, "output": settings}),
        serde_json::json!({"command": "create_station", "input": create_station, "output": station.clone()}),
        serde_json::json!({"command": "update_station", "input": update_station, "output": station.clone()}),
        serde_json::json!({"command": "delete_station", "input": delete_station, "output": null}),
        serde_json::json!({"command": "reorder_stations", "input": reorder_stations, "output": [station]}),
    ];
    commands.extend(super::dto::station_keys::serialization_fixtures());
    commands.extend(super::dto::change_logs::serialization_fixtures());
    commands.extend(super::dto::collector_facts::serialization_fixtures());
    commands.extend(super::dto::channel_monitor_reads::serialization_fixtures());
    commands.extend(super::dto::channel_monitor_mutations::serialization_fixtures());
    commands.extend(super::dto::channel_monitor_operations::serialization_fixtures());
    commands.extend(super::dto::station_collector_operations::serialization_fixtures());
    commands.extend(super::dto::routing_health_reads::serialization_fixtures());
    commands.extend(super::dto::routing_mutations::serialization_fixtures());
    commands.extend(super::dto::pricing_reads::serialization_fixtures());
    commands.extend(super::dto::pricing_mutations::serialization_fixtures());
    commands.extend(super::dto::proxy_workspace_reads::serialization_fixtures());
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
    source
        .replace(
            r#"import { invoke } from "@/lib/bridge/transport";"#,
            r#"import { invoke, invokeNonIdempotent } from "@/lib/bridge/transport";"#,
        )
        .replace(
            r#"return invokeCommand<StationDto>("create_station", { input });"#,
            r#"return invokeNonIdempotent<StationDto>("create_station", { input });"#,
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
            r#"export function listCommonLoginProfiles(input: EmptyInputDto = {}): Promise<CommonLoginProfileDto[]> {
  return invokeCommand<CommonLoginProfileDto[]>("list_common_login_profiles", { input });
}

export function upsertCommonLoginProfile(input: UpsertCommonLoginProfileInputDto): Promise<CommonLoginProfileDto> {
  return invokeNonIdempotent<CommonLoginProfileDto>("upsert_common_login_profile", { input });
}

export function deleteCommonLoginProfile(input: CommonLoginProfileIdInputDto): Promise<void> {
  return invokeCommand<void>("delete_common_login_profile", { input });
}

export function getCommonLoginProfilePassword(input: CommonLoginProfileIdInputDto): Promise<string> {
  return invokeCommand<string>("get_common_login_profile_password", { input });
}

export function listRequestLogs(input: EmptyInputDto = {}): Promise<RequestLogDto[]> {
  return invokeCommand<RequestLogDto[]>("list_request_logs", { input });
}

export function clearRequestLogs(input: EmptyInputDto = {}): Promise<void> {
  return invokeCommand<void>("clear_request_logs", { input });
}

export function listChangeEvents(input: EmptyInputDto = {}): Promise<ChangeEventDto[]> {
  return invokeCommand<ChangeEventDto[]>("list_change_events", { input });
}

export function clearChangeEvents(input: EmptyInputDto = {}): Promise<void> {
  return invokeCommand<void>("clear_change_events", { input });
}

export function listChangeEventsForStation(input: ChangeLogStationIdInputDto): Promise<ChangeEventDto[]> {
  return invokeCommand<ChangeEventDto[]>("list_change_events_for_station", { input });
}

export function upsertChangeEvent(input: UpsertChangeEventInputDto): Promise<ChangeEventDto> {
  return invokeCommand<ChangeEventDto>("upsert_change_event", { input });
}

export function markChangeEventRead(input: ChangeEventIdInputDto): Promise<ChangeEventDto> {
  return invokeCommand<ChangeEventDto>("mark_change_event_read", { input });
}

export function markChangeEventsRead(input: ChangeEventIdsInputDto): Promise<ChangeEventDto[]> {
  return invokeCommand<ChangeEventDto[]>("mark_change_events_read", { input });
}

export function dismissChangeEvent(input: ChangeEventIdInputDto): Promise<ChangeEventDto> {
  return invokeCommand<ChangeEventDto>("dismiss_change_event", { input });
}

export function resolveChangeEvent(input: ChangeEventIdInputDto): Promise<ChangeEventDto> {
  return invokeCommand<ChangeEventDto>("resolve_change_event", { input });
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

export function listChannelMonitorSummaries(input: ChannelMonitorSummaryInputDto): Promise<ChannelMonitorSummaryDto[]> {
  return invokeCommand<ChannelMonitorSummaryDto[]>("list_channel_monitor_summaries", { input });
}

export function listChannelStatusSummaries(input: EmptyInputDto = {}): Promise<ChannelStatusSummaryDto[]> {
  return invokeCommand<ChannelStatusSummaryDto[]>("list_channel_status_summaries", { input });
}

export function listChannelMonitorRuns(input: ChannelMonitorIdInputDto): Promise<ChannelMonitorRunDto[]> {
  return invokeCommand<ChannelMonitorRunDto[]>("list_channel_monitor_runs", { input });
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

export function loadChannelStatusWorkspace(input: EmptyInputDto = {}): Promise<ChannelStatusWorkspaceDto> {
  return invokeCommand<ChannelStatusWorkspaceDto>("load_channel_status_workspace", { input });
}

export function runChannelMonitorNow(input: ChannelMonitorIdInputDto): Promise<ChannelMonitorRunDto[]> {
  return invokeNonIdempotent<ChannelMonitorRunDto[]>("run_channel_monitor_now", { input });
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

export function testStationLogin(input: CollectorStationIdInputDto): Promise<CollectorRunResultDto> {
  return invokeNonIdempotent<CollectorRunResultDto>("test_station_login", { input });
}

export function testStationLoginInput(input: StationLoginTestInputDto): Promise<StationLoginTestResultDto> {
  return invokeCommand<StationLoginTestResultDto>("test_station_login_input", { input });
}

export function startCaptureSession(input: CaptureStationIdInputDto): Promise<CaptureSessionStatusDto> {
  return invokeNonIdempotent<CaptureSessionStatusDto>("start_capture_session", { input });
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

export function listStationEndpointHealth(input: EmptyInputDto = {}): Promise<StationEndpointHealthDto[]> {
  return invokeCommand<StationEndpointHealthDto[]>("list_station_endpoint_health", { input });
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

export function simulateRoute(input: RouteSimulationInputDto): Promise<RouteSimulationResultDto> {
  return invokeCommand<RouteSimulationResultDto>("simulate_route", { input });
}

export function listPricingRules(input: EmptyInputDto = {}): Promise<PricingRuleDto[]> {
  return invokeCommand<PricingRuleDto[]>("list_pricing_rules", { input });
}

export function listModelBasePrices(input: EmptyInputDto = {}): Promise<ModelBasePriceDto[]> {
  return invokeCommand<ModelBasePriceDto[]>("list_model_base_prices", { input });
}

export function resolveStationKeyPricingContext(input: PricingContextInputDto): Promise<ResolvedPricingContextDto> {
  return invokeCommand<ResolvedPricingContextDto>("resolve_station_key_pricing_context", { input });
}

export function loadPricingComparisonWorkspace(input: EmptyInputDto = {}): Promise<PricingComparisonWorkspaceDto> {
  return invokeCommand<PricingComparisonWorkspaceDto>("load_pricing_comparison_workspace", { input });
}

export function upsertModelBasePrice(input: UpsertModelBasePriceInputDto): Promise<ModelBasePriceDto> {
  return invokeCommand<ModelBasePriceDto>("upsert_model_base_price", { input });
}

export function resetModelBasePricesToBuiltins(input: EmptyInputDto = {}): Promise<ModelBasePriceDto[]> {
  return invokeCommand<ModelBasePriceDto[]>("reset_model_base_prices_to_builtins", { input });
}

export function upsertPricingRule(input: UpsertPricingRuleInputDto): Promise<PricingRuleDto> {
  return invokeCommand<PricingRuleDto>("upsert_pricing_rule", { input });
}

export function deletePricingRule(input: PricingRuleIdInputDto): Promise<void> {
  return invokeCommand<void>("delete_pricing_rule", { input });
}

export function getProxyStatus(input: EmptyInputDto = {}): Promise<ProxyStatusDto> {
  return invokeCommand<ProxyStatusDto>("get_proxy_status", { input });
}

export function loadLocalRoutingWorkspace(input: EmptyInputDto = {}): Promise<LocalRoutingWorkspaceDto> {
  return invokeCommand<LocalRoutingWorkspaceDto>("load_local_routing_workspace", { input });
}

export function reorderLocalRoutingKeys(input: ReorderLocalRoutingKeysInputDto): Promise<LocalRoutingWorkspaceDto> {
  return invokeCommand<LocalRoutingWorkspaceDto>("reorder_local_routing_keys", { input });
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
            r#"export function getRuntimeContractInfo(): Promise<RuntimeContractInfo> {
  return invokeCommand<RuntimeContractInfo>("get_runtime_contract_info");
}"#,
            r#"export function getRuntimeContractInfo(input: EmptyInputDto = {}): Promise<RuntimeContractInfo> {
  return invokeCommand<RuntimeContractInfo>("get_runtime_contract_info", { input });
}"#,
        )
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
    fn changes_logs_commands_have_closed_schemas_and_frozen_mutation_semantics() {
        for name in [
            "clear_change_events",
            "clear_request_logs",
            "dismiss_change_event",
            "list_change_events",
            "list_change_events_for_station",
            "list_request_logs",
            "mark_change_event_read",
            "mark_change_events_read",
            "resolve_change_event",
            "upsert_change_event",
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
        assert_eq!(
            command_contract("upsert_change_event").mutation_kind,
            "idempotent"
        );
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
            "list_channel_monitor_runs",
            "list_channel_monitor_summaries",
            "list_channel_monitor_templates",
            "list_channel_monitors",
            "list_channel_status_summaries",
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
        assert_eq!(workspace.input, "EmptyInputDto");
        assert_eq!(workspace.output, "ChannelStatusWorkspaceDto");
        assert_eq!(workspace.mutation_kind, "read");
        assert_eq!(workspace.runtime_validation, "rust_dto_pre_application");
        assert!(!workspace.transport_retry);
        assert!(!workspace.result_unknown);

        let run_now = command_contract("run_channel_monitor_now");
        assert_eq!(run_now.input, "ChannelMonitorIdInputDto");
        assert_eq!(run_now.output, "Vec<ChannelMonitorRunDto>");
        assert_eq!(run_now.mutation_kind, "non_idempotent");
        assert_eq!(run_now.runtime_validation, "rust_dto_pre_application");
        assert!(!run_now.transport_retry);
        assert!(run_now.result_unknown);
    }

    #[test]
    fn station_collector_operations_have_closed_schemas_and_frozen_semantics() {
        for name in [
            "detect_sub2api_station",
            "collect_sub2api_station",
            "detect_station_info",
            "collect_station_info",
            "collect_station_task",
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
                "list_station_endpoint_health",
                "EmptyInputDto",
                "Vec<StationEndpointHealthDto>",
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
            ("list_pricing_rules", "EmptyInputDto", "Vec<PricingRuleDto>"),
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
            (
                "upsert_pricing_rule",
                "UpsertPricingRuleInputDto",
                "PricingRuleDto",
            ),
            ("delete_pricing_rule", "PricingRuleIdInputDto", "unit"),
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
        for (name, output) in [
            ("get_proxy_status", "ProxyStatusDto"),
            ("load_local_routing_workspace", "LocalRoutingWorkspaceDto"),
        ] {
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
    fn local_routing_reorder_has_a_closed_schema_and_idempotent_semantics() {
        let contract = command_contract("reorder_local_routing_keys");
        assert_eq!(contract.input, "ReorderLocalRoutingKeysInputDto");
        assert_eq!(contract.output, "LocalRoutingWorkspaceDto");
        assert_eq!(contract.mutation_kind, "idempotent");
        assert_eq!(contract.runtime_validation, "rust_dto_pre_application");
        assert!(!contract.transport_retry);
        assert!(!contract.result_unknown);
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
    fn runtime_status_is_the_only_public_runtime_diagnostics_surface() {
        let status = command_contract("get_runtime_status");
        assert_eq!(status.input, "EmptyInputDto");
        assert_eq!(status.output, "RuntimeStatusDto");
        assert_eq!(status.mutation_kind, "read");
        assert_eq!(status.runtime_validation, "rust_dto_pre_application");
        assert!(!status.transport_retry);
        assert!(!status.result_unknown);

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
            assert!(
                !command.name.contains("runtime_diagnostic")
                    && !command.name.contains("runtime_diagnostics"),
                "{} exposes a runtime diagnostics IPC command without a developer-mode gate",
                command.name
            );
        }
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
            "listChannelMonitorRuns",
            "listChannelMonitorSummaries",
            "listChannelMonitorTemplates",
            "listChannelMonitors",
            "listChannelStatusSummaries",
            "createChannelMonitor",
            "updateChannelMonitor",
            "deleteChannelMonitor",
            "createChannelMonitorTemplate",
            "updateChannelMonitorTemplate",
            "duplicateChannelMonitorTemplate",
            "deleteChannelMonitorTemplate",
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
            ("run_channel_monitor_now", "ChannelMonitorRunDto[]"),
            ("detect_sub2api_station", "CollectorRunResultDto"),
            ("collect_sub2api_station", "CollectorRunResultDto"),
            ("detect_station_info", "CollectorRunResultDto"),
            ("collect_station_info", "CollectorRunResultDto"),
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
        for function in [
            "upsertModelBasePrice",
            "resetModelBasePricesToBuiltins",
            "upsertPricingRule",
            "deletePricingRule",
        ] {
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
