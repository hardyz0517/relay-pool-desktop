use semver::Version;
use serde::Serialize;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub(crate) mod runtime_events;

use super::outbound::current_system_proxy_url;
use crate::outbound::{
    AsyncOutboundClient, OutboundFailure, OutboundHeaderPolicy, OutboundHeaders, OutboundRequest,
    OutboundRetryPolicy, ProxyPolicy, RequestBudget,
};
use http::{header, HeaderValue, Method};

const UPDATE_MANIFEST_URL: &str =
    "https://github.com/hardyz0517/relay-pool-desktop/releases/latest/download/latest.json";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterNetworkConfig {
    pub proxy_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishedVersionRelation {
    CurrentOrOlder,
    Newer,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedUpdateInspection {
    pub relation: PublishedVersionRelation,
    pub version: String,
    pub notes: Option<String>,
}

pub fn network_config() -> UpdaterNetworkConfig {
    UpdaterNetworkConfig {
        proxy_url: current_system_proxy_url(),
    }
}

pub async fn inspect_latest_update_manifest(
    outbound: &AsyncOutboundClient,
    current_version: &str,
) -> Result<PublishedUpdateInspection, String> {
    inspect_update_manifest_at(outbound, current_version, UPDATE_MANIFEST_URL).await
}

/// Keep the production endpoint fixed while letting the owner test its actual
/// outbound/parser path against a local server.
async fn inspect_update_manifest_at(
    outbound: &AsyncOutboundClient,
    current_version: &str,
    manifest_url: &str,
) -> Result<PublishedUpdateInspection, String> {
    let response = match outbound
        .execute(
            updater_manifest_request(manifest_url, Duration::from_secs(10))
                .map_err(|error| format!("Failed to build updater manifest request: {error}"))?,
            CancellationToken::new(),
        )
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return Err(format!("Failed to read updater latest.json: {error}"));
        }
    };
    if !response.status.is_success() {
        return Err(format!(
            "Failed to read updater latest.json: HTTP {}",
            response.status.as_u16()
        ));
    }
    let body = String::from_utf8(response.body.to_vec())
        .map_err(|error| format!("Failed to read updater latest.json body: {error}"))?;
    inspect_manifest_body(&body, current_version)
}

fn updater_manifest_request(
    manifest_url: &str,
    timeout: Duration,
) -> Result<OutboundRequest, OutboundFailure> {
    let policy = OutboundHeaderPolicy::provider_default();
    let mut headers = OutboundHeaders::new();
    headers.insert_public(
        header::ACCEPT,
        HeaderValue::from_static("application/json"),
        &policy,
    )?;
    Ok(OutboundRequest {
        method: Method::GET,
        url: manifest_url.to_string(),
        correlation_id: None,
        headers,
        body: Vec::new(),
        proxy: ProxyPolicy::System,
        budget: RequestBudget::from_now(timeout),
        retry_policy: OutboundRetryPolicy::Never,
    })
}

fn inspect_manifest_body(
    body: &str,
    current_version: &str,
) -> Result<PublishedUpdateInspection, String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|error| format!("Invalid updater manifest JSON: {error}"))?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Updater manifest does not contain a version".to_string())?;
    let published = Version::parse(normalize_version(version))
        .map_err(|error| format!("Invalid published updater version: {error}"))?;
    let current = Version::parse(normalize_version(current_version))
        .map_err(|error| format!("Invalid current application version: {error}"))?;

    Ok(PublishedUpdateInspection {
        relation: if published > current {
            PublishedVersionRelation::Newer
        } else {
            PublishedVersionRelation::CurrentOrOlder
        },
        version: version.to_string(),
        notes: value
            .get("notes")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|notes| !notes.is_empty())
            .map(str::to_string),
    })
}

fn normalize_version(value: &str) -> &str {
    let value = value.trim();
    value
        .strip_prefix('v')
        .or_else(|| value.strip_prefix('V'))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::{
        inspect_manifest_body, inspect_update_manifest_at, updater_manifest_request,
        PublishedVersionRelation, UPDATE_MANIFEST_URL,
    };
    use crate::outbound::{OutboundRetryPolicy, ProxyPolicy};
    use http::Method;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::Arc,
        time::Duration,
    };

    #[test]
    fn updater_manifest_request_uses_shared_outbound_policy() {
        let request =
            updater_manifest_request(UPDATE_MANIFEST_URL, Duration::from_secs(10)).unwrap();

        assert_eq!(request.method, Method::GET);
        assert_eq!(request.url, UPDATE_MANIFEST_URL);
        assert_eq!(request.proxy, ProxyPolicy::System);
        assert_eq!(request.retry_policy, OutboundRetryPolicy::Never);
        assert!(request.body.is_empty());
        assert!(request
            .headers
            .redaction()
            .public
            .iter()
            .any(|header| header == "accept"));
        assert!(request.budget.remaining().is_some());
    }

    #[test]
    fn classifies_equal_and_older_manifests_as_current_or_older() {
        assert_eq!(
            inspect_manifest_body(r#"{"version":"0.2.2","notes":""}"#, "0.2.2")
                .unwrap()
                .relation,
            PublishedVersionRelation::CurrentOrOlder,
        );
        assert_eq!(
            inspect_manifest_body(r#"{"version":"0.2.1"}"#, "0.2.2")
                .unwrap()
                .relation,
            PublishedVersionRelation::CurrentOrOlder,
        );
    }

    #[test]
    fn classifies_newer_and_prefixed_versions_with_semver_rules() {
        assert_eq!(
            inspect_manifest_body(r#"{"version":"v0.2.3"}"#, "V0.2.2")
                .unwrap()
                .relation,
            PublishedVersionRelation::Newer,
        );
        assert_eq!(
            inspect_manifest_body(r#"{"version":"0.2.3-beta.1"}"#, "0.2.3")
                .unwrap()
                .relation,
            PublishedVersionRelation::CurrentOrOlder,
        );
    }

    #[test]
    fn normalizes_empty_release_notes_to_none() {
        let inspection =
            inspect_manifest_body(r#"{"version":"0.2.3","notes":""}"#, "0.2.2").unwrap();

        assert_eq!(inspection.notes, None);
    }

    #[test]
    fn rejects_missing_or_invalid_manifest_versions() {
        assert!(inspect_manifest_body("{}", "0.2.2").is_err());
        assert!(inspect_manifest_body(r#"{"version":"not-semver"}"#, "0.2.2").is_err());
        assert!(inspect_manifest_body(r#"{"version":"0.2.3"}"#, "not-semver").is_err());
    }

    #[tokio::test]
    async fn loopback_malformed_manifest_failure_publishes_final_jsonl_event() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback updater");
        let address = listener.local_addr().expect("loopback updater address");
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept updater request");
            let mut request = [0_u8; 4096];
            assert!(stream.read(&mut request).expect("read updater request") > 0);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 16\r\nconnection: close\r\n\r\n{not-json-canary",
                )
                .expect("write malformed updater manifest");
        });
        let root = tempfile::tempdir().expect("runtime root");
        let service = Arc::new(crate::observability::runtime::RuntimeLogService::open(
            root.path(),
        ));
        let outbound = crate::outbound::AsyncOutboundClient::new(
            crate::outbound::AsyncOutboundClientConfig::architecture_budget(),
        );

        crate::observability::runtime::bootstrap::with_test_service(
            Arc::clone(&service),
            || async {
                let result = inspect_update_manifest_at(
                    &outbound,
                    "0.0.0",
                    &format!("http://{address}/latest.json"),
                )
                .await;
                assert!(crate::observability::runtime::bootstrap::record_failure(
                    crate::services::updater::runtime_events::manifest_inspect_failed(),
                    result,
                )
                .is_err());
            },
        )
        .await;
        worker.join().expect("updater loopback joins");
        service.flush();

        let page = crate::observability::runtime::RuntimeLogReader::new(root.path()).read_page(
            0,
            50,
            1024 * 1024,
        );
        assert!(page.issues.is_empty(), "reader issues: {:?}", page.issues);
        let raw = page
            .lines
            .iter()
            .map(|line| line.as_bytes())
            .collect::<Vec<_>>();
        assert!(raw.iter().any(|line| line
            .windows(31)
            .any(|window| window == b"updater.manifest.inspect_failed")));
        assert!(!raw
            .iter()
            .any(|line| line.windows(15).any(|window| window == b"not-json-canary")));
    }

    #[tokio::test]
    async fn loopback_disconnect_manifest_failure_publishes_final_jsonl_event() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback updater");
        let address = listener.local_addr().expect("loopback updater address");
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept updater request");
            let mut request = [0_u8; 4096];
            assert!(stream.read(&mut request).expect("read updater request") > 0);
            // Drop the accepted connection before response headers. This is a
            // deterministic provider/network disconnect, not a DNS fixture.
        });
        let root = tempfile::tempdir().expect("runtime root");
        let service = Arc::new(crate::observability::runtime::RuntimeLogService::open(
            root.path(),
        ));
        let outbound = crate::outbound::AsyncOutboundClient::new(
            crate::outbound::AsyncOutboundClientConfig::architecture_budget(),
        );

        crate::observability::runtime::bootstrap::with_test_service(
            Arc::clone(&service),
            || async {
                let result = inspect_update_manifest_at(
                    &outbound,
                    "0.0.0",
                    &format!("http://{address}/latest.json"),
                )
                .await;
                assert!(crate::observability::runtime::bootstrap::record_failure(
                    crate::services::updater::runtime_events::manifest_inspect_failed(),
                    result,
                )
                .is_err());
            },
        )
        .await;
        worker.join().expect("updater loopback joins");
        service.flush();

        let page = crate::observability::runtime::RuntimeLogReader::new(root.path()).read_page(
            0,
            50,
            1024 * 1024,
        );
        assert!(page.issues.is_empty(), "reader issues: {:?}", page.issues);
        assert!(page.lines.iter().any(|line| {
            serde_json::from_slice::<crate::observability::runtime::RuntimeEvent>(line.as_bytes())
                .ok()
                .is_some_and(|event| event.event_code.as_str() == "updater.manifest.inspect_failed")
        }));
    }
}
