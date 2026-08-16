use serde::{Deserialize, Serialize};

use super::TypeDescriptor;
use crate::observability::runtime::{
    catalog::ManifestSource, Component, EventLevel, EventOutcome, RuntimeDetail,
};

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RuntimeDiagnosticsQueryDto {
    #[serde(default)]
    pub segment_index: usize,
    #[serde(default)]
    pub line_index: usize,
    #[serde(default)]
    pub level: Option<EventLevel>,
    #[serde(default)]
    pub component: Option<Component>,
    #[serde(default)]
    pub event_code: Option<String>,
    #[serde(default)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub interaction_id: Option<String>,
}

impl RuntimeDiagnosticsQueryDto {
    pub(crate) fn parse(
        value: serde_json::Value,
    ) -> Result<Self, crate::commands::error::CommandError> {
        serde_json::from_value(value).map_err(|_| {
            crate::ipc::dto::invalid_input(
                "input",
                "invalid_shape",
                "The runtime diagnostics input is invalid.",
            )
        })
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeEventDto {
    pub schema_version: u16,
    pub at_ms: i64,
    pub sequence: u64,
    pub level: EventLevel,
    pub event_code: String,
    pub component: Component,
    pub outcome: EventOutcome,
    pub session_id: String,
    pub correlation_id: Option<String>,
    pub interaction_id: Option<String>,
    pub operation_id: Option<String>,
    pub duration_ms: Option<u64>,
    pub detail: RuntimeDetail,
    pub error_code: Option<String>,
    pub message_key: String,
    pub deprecated_replaced_by: Option<String>,
    pub manifest_source: ManifestSource,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeDiagnosticsPageDto {
    pub events: Vec<RuntimeEventDto>,
    pub next_segment_index: Option<usize>,
    pub next_line_index: Option<usize>,
    pub issue_count: u32,
    pub sink_degraded: bool,
    pub dropped_count: u64,
    pub rejected_count: u64,
    pub last_sink_error_code: Option<String>,
    pub clock_stable: bool,
    pub recovery_examined: usize,
    pub recovery_recovered: usize,
    pub recovery_skipped: usize,
    pub retention_considered: usize,
    pub retention_deleted: usize,
    pub retention_skipped_unknown: usize,
    pub retention_delete_failures: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeSupportBundleResultDto {
    pub event_count: u32,
    pub issue_count: u32,
}

pub(crate) const RUNTIME_DIAGNOSTICS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "RuntimeDiagnosticsQueryDto",
    typescript: include_str!("runtime_diagnostics.typescript.txt"),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_contract_rejects_paths_regex_and_server_limit_overrides() {
        let unknown = serde_json::json!({
            "path": "C:\\Users\\secret",
            "regex": ".*",
            "maxLines": 1,
            "maxBytes": 1
        });
        assert!(serde_json::from_value::<RuntimeDiagnosticsQueryDto>(unknown).is_err());

        let exact = serde_json::json!({
            "segmentIndex": 2,
            "lineIndex": 7,
            "level": "warn",
            "component": "runtime",
            "eventCode": "runtime.log_event.dropped",
            "correlationId": "cor_0123456789abcdef0123456789abcdef",
            "interactionId": "int_0123456789abcdef0123456789abcdef"
        });
        let query = serde_json::from_value::<RuntimeDiagnosticsQueryDto>(exact)
            .expect("supported exact filters");
        assert_eq!(query.segment_index, 2);
        assert_eq!(query.line_index, 7);
        assert_eq!(
            query.event_code.as_deref(),
            Some("runtime.log_event.dropped")
        );
    }

    #[test]
    fn support_bundle_result_never_serializes_a_destination_path() {
        let result = RuntimeSupportBundleResultDto {
            event_count: 3,
            issue_count: 1,
        };

        let serialized = serde_json::to_value(result).expect("support bundle result serialization");
        assert_eq!(
            serialized,
            serde_json::json!({ "eventCount": 3, "issueCount": 1 })
        );
        assert!(serialized.get("path").is_none());
    }
}
