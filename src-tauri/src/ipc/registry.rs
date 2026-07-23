use serde::Serialize;
#[cfg(test)]
use sha2::{Digest, Sha256};

#[cfg(test)]
use super::dto::REGISTERED_TYPES;

#[cfg_attr(not(test), allow(dead_code))]
pub const GENERATOR_VERSION: u32 = 1;
#[cfg_attr(not(test), allow(dead_code))]
pub const IPC_CONTRACT_VERSION: u32 = 1;

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
            app_status => $crate::commands::app_status,
            get_data_store_startup_state => $crate::commands::get_data_store_startup_state,
            refresh_data_store_candidates => $crate::commands::refresh_data_store_candidates,
            locate_data_store_candidate => $crate::commands::locate_data_store_candidate,
            activate_data_store_candidate => $crate::commands::activate_data_store_candidate,
            create_new_data_store => $crate::commands::create_new_data_store,
            open_data_store_backup_dir => $crate::commands::open_data_store_backup_dir,
            export_data_store_diagnostic => $crate::commands::export_data_store_diagnostic,
            list_stations => $crate::commands::list_stations,
            create_station => $crate::commands::create_station,
            update_station => $crate::commands::update_station,
            delete_station => $crate::commands::delete_station,
            reorder_stations => $crate::commands::reorder_stations,
            get_settings => $crate::commands::get_settings,
            get_local_access_key => $crate::commands::get_local_access_key,
            update_local_access_key => $crate::commands::update_local_access_key,
            import_relay_pool_to_ccswitch => $crate::commands::import_relay_pool_to_ccswitch,
            open_external_url => $crate::commands::open_external_url,
            updater_network_config => $crate::commands::updater_network_config,
            inspect_latest_update_manifest => $crate::commands::inspect_latest_update_manifest,
            update_settings => $crate::commands::update_settings,
            choose_data_dir => $crate::commands::choose_data_dir,
            reset_data_dir => $crate::commands::reset_data_dir,
            get_proxy_status => $crate::commands::get_proxy_status,
            load_local_routing_workspace => $crate::commands::load_local_routing_workspace,
            reorder_local_routing_keys => $crate::commands::reorder_local_routing_keys,
            start_local_proxy => $crate::commands::start_local_proxy,
            stop_local_proxy => $crate::commands::stop_local_proxy,
            cleanup_before_update => $crate::commands::cleanup_before_update,
            prepare_local_proxy_for_update => $crate::commands::prepare_local_proxy_for_update,
            restart_local_proxy => $crate::commands::restart_local_proxy,
            list_request_logs => $crate::commands::list_request_logs,
            clear_request_logs => $crate::commands::clear_request_logs,
            list_station_keys => $crate::commands::list_station_keys,
            create_station_key => $crate::commands::create_station_key,
            update_station_key => $crate::commands::update_station_key,
            save_station_key_with_defaults => $crate::commands::save_station_key_with_defaults,
            update_station_key_group_binding => $crate::commands::update_station_key_group_binding,
            delete_station_key => $crate::commands::delete_station_key,
            reorder_station_keys => $crate::commands::reorder_station_keys,
            get_remote_key_capability => $crate::commands::get_remote_key_capability,
            list_remote_station_keys => $crate::commands::list_remote_station_keys,
            scan_remote_station_keys => $crate::commands::scan_remote_station_keys,
            create_remote_station_key => $crate::commands::create_remote_station_key,
            create_local_station_key_from_remote => $crate::commands::create_local_station_key_from_remote,
            bind_remote_station_key => $crate::commands::bind_remote_station_key,
            unbind_remote_station_key => $crate::commands::unbind_remote_station_key,
            list_key_pool_items => $crate::commands::list_key_pool_items,
            reorder_key_pool => $crate::commands::reorder_key_pool,
            get_station_key_capabilities => $crate::commands::get_station_key_capabilities,
            update_station_key_capabilities => $crate::commands::update_station_key_capabilities,
            list_model_aliases => $crate::commands::list_model_aliases,
            upsert_model_alias => $crate::commands::upsert_model_alias,
            delete_model_alias => $crate::commands::delete_model_alias,
            list_station_key_health => $crate::commands::list_station_key_health,
            list_station_endpoint_health => $crate::commands::list_station_endpoint_health,
            list_channel_monitors => $crate::commands::list_channel_monitors,
            list_channel_monitor_summaries => $crate::commands::list_channel_monitor_summaries,
            list_channel_status_summaries => $crate::commands::list_channel_status_summaries,
            load_channel_status_workspace => $crate::commands::load_channel_status_workspace,
            load_pricing_comparison_workspace => $crate::commands::load_pricing_comparison_workspace,
            create_channel_monitor => $crate::commands::create_channel_monitor,
            update_channel_monitor => $crate::commands::update_channel_monitor,
            delete_channel_monitor => $crate::commands::delete_channel_monitor,
            list_channel_monitor_runs => $crate::commands::list_channel_monitor_runs,
            list_channel_monitor_templates => $crate::commands::list_channel_monitor_templates,
            create_channel_monitor_template => $crate::commands::create_channel_monitor_template,
            update_channel_monitor_template => $crate::commands::update_channel_monitor_template,
            duplicate_channel_monitor_template => $crate::commands::duplicate_channel_monitor_template,
            delete_channel_monitor_template => $crate::commands::delete_channel_monitor_template,
            run_channel_monitor_now => $crate::commands::run_channel_monitor_now,
            get_station_key_health => $crate::commands::get_station_key_health,
            ping_station_endpoint => $crate::commands::ping_station_endpoint,
            test_station_key_connectivity => $crate::commands::test_station_key_connectivity,
            simulate_route => $crate::commands::simulate_route,
            list_pricing_rules => $crate::commands::list_pricing_rules,
            list_model_base_prices => $crate::commands::list_model_base_prices,
            upsert_model_base_price => $crate::commands::upsert_model_base_price,
            reset_model_base_prices_to_builtins => $crate::commands::reset_model_base_prices_to_builtins,
            upsert_pricing_rule => $crate::commands::upsert_pricing_rule,
            delete_pricing_rule => $crate::commands::delete_pricing_rule,
            resolve_station_key_pricing_context => $crate::commands::resolve_station_key_pricing_context,
            list_balance_snapshots => $crate::commands::list_balance_snapshots,
            list_current_station_balance_snapshots => $crate::commands::list_current_station_balance_snapshots,
            list_balance_snapshots_for_station => $crate::commands::list_balance_snapshots_for_station,
            upsert_balance_snapshot => $crate::commands::upsert_balance_snapshot,
            list_station_group_bindings => $crate::commands::list_station_group_bindings,
            list_station_group_options => $crate::commands::list_station_group_options,
            upsert_station_group_binding => $crate::commands::upsert_station_group_binding,
            list_group_rate_records => $crate::commands::list_group_rate_records,
            list_collector_runs => $crate::commands::list_collector_runs,
            list_change_events => $crate::commands::list_change_events,
            clear_change_events => $crate::commands::clear_change_events,
            list_change_events_for_station => $crate::commands::list_change_events_for_station,
            upsert_change_event => $crate::commands::upsert_change_event,
            mark_change_event_read => $crate::commands::mark_change_event_read,
            mark_change_events_read => $crate::commands::mark_change_events_read,
            dismiss_change_event => $crate::commands::dismiss_change_event,
            resolve_change_event => $crate::commands::resolve_change_event,
            get_station_credentials => $crate::commands::get_station_credentials,
            update_station_credentials => $crate::commands::update_station_credentials,
            update_station_session => $crate::commands::update_station_session,
            clear_station_credentials => $crate::commands::clear_station_credentials,
            detect_station_info => $crate::commands::detect_station_info,
            collect_station_info => $crate::commands::collect_station_info,
            collect_station_task => $crate::commands::collect_station_task,
            test_station_login => $crate::commands::test_station_login,
            test_station_login_input => $crate::commands::test_station_login_input,
            detect_sub2api_station => $crate::commands::detect_sub2api_station,
            collect_sub2api_station => $crate::commands::collect_sub2api_station,
            list_collector_snapshots => $crate::commands::list_collector_snapshots,
            get_latest_collector_snapshot => $crate::commands::get_latest_collector_snapshot,
            start_capture_session => $crate::commands::start_capture_session,
            get_capture_session_status => $crate::commands::get_capture_session_status,
            record_capture_event => $crate::commands::record_capture_event,
            finish_capture_session => $crate::commands::finish_capture_session,
            finish_web_authorization_session => $crate::commands::finish_web_authorization_session,
            clear_capture_session => $crate::commands::clear_capture_session,
            close_capture_session => $crate::commands::close_capture_session,
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
}

#[cfg(test)]
#[derive(Serialize)]
struct RegistryEvidence<'a> {
    kind: &'static str,
    serialization_fixture_hash: &'a str,
}

#[cfg(test)]
fn command_schema(name: &str) -> (&'static str, &'static str, &'static str) {
    match name {
        "get_settings" => ("unit", "SettingsDto", "legacy_string_error_v0"),
        "list_stations" => ("unit", "Vec<StationDto>", "legacy_string_error_v0"),
        _ => (
            "legacy_unmigrated_input",
            "legacy_unmigrated_output",
            "legacy_string_error_v0",
        ),
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
            let (input, output, error) = command_schema(name);
            serde_json::json!({"name": name, "input": input, "output": output, "error": error})
        }).collect::<Vec<_>>(),
        "types": types,
        "streaming_surfaces": STREAMING_SURFACES,
    }))
    .expect("canonical IPC contract must serialize")
}

#[cfg(test)]
fn pilot_serialization_fixture() -> String {
    let settings = crate::models::settings::AppSettings {
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
    };
    let value = serde_json::json!({
        "schemaVersion": 1,
        "commands": [
            {"command": "get_settings", "input": {}, "output": super::dto::SettingsDto::from(settings)},
            {"command": "list_stations", "input": {}, "output": [super::dto::stations::fixture()]},
        ]
    });
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
    format!(
        "// @generated by repository IPC generator. Do not edit.\n// generator version: {GENERATOR_VERSION}\n// IPC contract version: {IPC_CONTRACT_VERSION}\n// canonical hash: {contract_hash}\n\nimport {{ invoke }} from \"@tauri-apps/api/core\";\n\n{types}\n\nexport const IPC_CONTRACT_VERSION = {IPC_CONTRACT_VERSION} as const;\nexport const IPC_BINDING_HASH = \"{contract_hash}\" as const;\n\nexport function getSettings(): Promise<SettingsDto> {{\n  return invoke<SettingsDto>(\"get_settings\");\n}}\n\nexport function listStations(): Promise<StationDto[]> {{\n  return invoke<StationDto[]>(\"list_stations\");\n}}\n\nexport type StreamingSubscription = {{ close(): void }};\n\nexport interface TypedStreamingAdapter<Event> {{\n  readonly eventSchemaVersion: number;\n  open(onEvent: (event: Event) => void): StreamingSubscription;\n}}\n"
    )
}

#[cfg(test)]
fn render_registry(contract_hash: &str, fixture_hash: &str) -> String {
    let mut commands = COMMANDS
        .iter()
        .map(|command| {
            let (input, output, error) = command_schema(command.name);
            RegistryCommand {
                name: command.name,
                transport: TransportKind::Ordinary,
                input_schema_hash: sha256(input),
                output_schema_hash: sha256(output),
                error_schema_hash: sha256(error),
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
    }

    #[test]
    fn emit_repository_bindings() {
        let output_dir = PathBuf::from(
            std::env::var_os("RELAY_POOL_BINDINGS_OUT")
                .expect("RELAY_POOL_BINDINGS_OUT is required for generator invocation"),
        );
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
            output_dir.join("command-registry.json"),
            render_registry(&contract_hash, &fixture_hash),
        )
        .expect("command registry must be written");
        fs::write(output_dir.join("pilot-serialization.json"), fixture)
            .expect("serialization fixture must be written");
    }
}
