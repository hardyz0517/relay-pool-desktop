use serde::{Deserialize, Serialize};

use super::registry::{IPC_BINDING_HASH, IPC_CONTRACT_VERSION};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeContractInfo {
    pub app_version: String,
    pub ipc_contract_version: u32,
    pub binding_hash: String,
    pub capabilities: Vec<RuntimeCapability>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCapability {
    RuntimeContract,
    DataRecovery,
    Settings,
    Stations,
    StationKeys,
    Collectors,
    Routing,
    Proxy,
    ChannelMonitoring,
    Pricing,
    ChangeEvents,
    Capture,
    TypedStreaming,
}

pub const RUNTIME_CONTRACT_TYPESCRIPT: &str = r#"export type RuntimeCapability =
  | "runtime_contract"
  | "data_recovery"
  | "settings"
  | "stations"
  | "station_keys"
  | "collectors"
  | "routing"
  | "proxy"
  | "channel_monitoring"
  | "pricing"
  | "change_events"
  | "capture"
  | "typed_streaming";

export type RuntimeContractInfo = {
  appVersion: string;
  ipcContractVersion: number;
  bindingHash: string;
  capabilities: RuntimeCapability[];
};"#;

pub fn current_runtime_contract() -> RuntimeContractInfo {
    RuntimeContractInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        ipc_contract_version: IPC_CONTRACT_VERSION,
        binding_hash: IPC_BINDING_HASH.to_string(),
        capabilities: vec![
            RuntimeCapability::RuntimeContract,
            RuntimeCapability::DataRecovery,
            RuntimeCapability::Settings,
            RuntimeCapability::Stations,
            RuntimeCapability::StationKeys,
            RuntimeCapability::Collectors,
            RuntimeCapability::Routing,
            RuntimeCapability::Proxy,
            RuntimeCapability::ChannelMonitoring,
            RuntimeCapability::Pricing,
            RuntimeCapability::ChangeEvents,
            RuntimeCapability::Capture,
            RuntimeCapability::TypedStreaming,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_contract_is_safe_and_uses_compiled_registry_identity() {
        let contract = current_runtime_contract();
        assert_eq!(contract.ipc_contract_version, IPC_CONTRACT_VERSION);
        assert_eq!(contract.binding_hash, IPC_BINDING_HASH);
        assert_eq!(contract.app_version, env!("CARGO_PKG_VERSION"));
        let serialized = serde_json::to_value(contract).expect("serialize contract");
        assert!(serialized["capabilities"].is_array());
        assert!(serialized["capabilities"]
            .as_array()
            .expect("capabilities")
            .iter()
            .all(|value| value.is_string()));
    }

    #[test]
    fn runtime_contract_does_not_include_runtime_or_secret_state() {
        let serialized = serde_json::to_string(&current_runtime_contract()).expect("serialize");
        assert!(!serialized.contains("sqlite"));
        assert!(!serialized.contains("token"));
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("\\"));
    }
}
