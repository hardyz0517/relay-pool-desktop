#[cfg(test)]
use crate::models::proxy::ProxyLifecycle;
use crate::models::proxy::ProxyStatus;

use super::TypeDescriptor;

pub type ProxyStatusDto = ProxyStatus;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-dto-type-descriptor; owner=ipc; remove_when=descriptor is registered in production binding export"
    )
)]
pub const PROXY_WORKSPACE_READS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "ProxyWorkspaceReadsDto",
    typescript: include_str!("proxy_workspace_reads.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<serde_json::Value> {
    let status = fixture_status();
    vec![
        serde_json::json!({"command":"get_proxy_status","input":{},"output":status}),
        serde_json::json!({"command":"start_local_proxy","input":{},"output":fixture_status()}),
    ]
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
