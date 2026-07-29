#[cfg(test)]
use crate::models::proxy::ProxyLifecycle;
use crate::models::proxy::ProxyStatus;

use super::TypeDescriptor;

pub type ProxyStatusDto = ProxyStatus;
pub type LocalRoutingWorkspaceDto =
    crate::application::routing_engine::routing_types::LocalRoutingWorkspace;

#[cfg_attr(not(test), allow(dead_code))]
pub const PROXY_WORKSPACE_READS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "ProxyWorkspaceReadsDto",
    typescript: include_str!("proxy_workspace_reads.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<serde_json::Value> {
    let status = fixture_status();
    let workspace = fixture_workspace();
    vec![
        serde_json::json!({"command":"get_proxy_status","input":{},"output":status}),
        serde_json::json!({"command":"load_local_routing_workspace","input":{},"output":workspace}),
        serde_json::json!({"command":"start_local_proxy","input":{},"output":fixture_status()}),
    ]
}

#[cfg(test)]
pub(crate) fn fixture_workspace() -> serde_json::Value {
    let status = fixture_status();
    serde_json::json!({
        "proxyStatus": status.clone(),
        "settings": {
            "enabled": true,
            "bindAddr": "127.0.0.1",
            "port": 8787,
            "endpoint": "chat_completions",
            "policy": "automatic_balanced",
            "maxRateMultiplier": 2.0,
            "routingGroupFilter": "all_groups",
            "fallbackEnabled": true,
            "previewKind": "baseline_eligibility"
        },
        "summary": {
            "candidateCount": 0,
            "previewEligibleCandidateCount": 0,
            "previewExcludedCandidateCount": 0,
            "cooldownCandidateCount": 0,
            "lastDecisionAt": null
        },
        "candidates": [],
        "latestDecision": null,
        "recentEvents": []
    })
}

#[cfg(test)]
fn fixture_status() -> ProxyStatus {
    ProxyStatus {
        running: true,
        lifecycle: ProxyLifecycle::Running,
        bind_addr: "127.0.0.1".into(),
        port: 8787,
        started_at: Some("1700000000000".into()),
        last_error: None,
        active_requests: 1,
        request_count: 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_status_serializes_the_closed_public_shape() {
        let value = serde_json::to_value(ProxyStatusDto {
            running: false,
            lifecycle: ProxyLifecycle::Stopped,
            bind_addr: "127.0.0.1".into(),
            port: 8787,
            started_at: None,
            last_error: None,
            active_requests: 0,
            request_count: 0,
        })
        .expect("proxy status fixture");

        assert_eq!(value["lifecycle"], "stopped");
        assert_eq!(value["bindAddr"], "127.0.0.1");
        assert!(value.get("bind_addr").is_none());
    }
}
