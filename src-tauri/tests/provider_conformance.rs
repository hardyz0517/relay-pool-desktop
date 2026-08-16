#![allow(
    dead_code,
    reason = "Provider conformance harness compiles contract source without production drivers"
)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

mod outbound {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    use bytes::Bytes;
    use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
    use tokio_util::sync::CancellationToken;

    #[derive(Clone)]
    pub struct AsyncOutboundClient;

    #[derive(Clone, Debug)]
    pub struct AsyncOutboundClientConfig;

    impl AsyncOutboundClientConfig {
        pub fn architecture_budget() -> Self {
            Self
        }
    }

    impl AsyncOutboundClient {
        pub fn new(_config: AsyncOutboundClientConfig) -> Self {
            Self
        }
    }

    impl AsyncOutboundClient {
        pub async fn execute(
            &self,
            request: OutboundRequest,
            _cancellation_token: CancellationToken,
        ) -> Result<OutboundResponse, OutboundFailure> {
            execute_local_http(request)
                .map_err(|_| OutboundFailure::new(OutboundFailureKind::RequestFailed))
        }
    }

    #[derive(Clone)]
    pub enum ProxyPolicy {
        Direct,
        System,
    }

    #[derive(Clone, Copy, Debug)]
    pub struct RequestBudget {
        deadline: Instant,
    }

    impl RequestBudget {
        pub fn from_now(duration: Duration) -> Self {
            Self {
                deadline: Instant::now() + duration,
            }
        }

        pub fn remaining(&self) -> Option<Duration> {
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            (!remaining.is_zero()).then_some(remaining)
        }
    }

    pub struct OutboundRequest {
        pub method: Method,
        pub url: String,
        pub correlation_id: Option<String>,
        pub headers: OutboundHeaders,
        pub body: Vec<u8>,
        pub proxy: ProxyPolicy,
        pub budget: RequestBudget,
        pub retry_policy: OutboundRetryPolicy,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum OutboundRetryPolicy {
        #[default]
        StatusRetry,
        Never,
    }

    pub struct OutboundResponse {
        pub status: StatusCode,
        pub headers: HeaderMap,
        pub body: Bytes,
        pub evidence: OutboundEvidence,
    }

    pub struct OutboundEvidence {
        pub final_url: String,
        pub retry_after: Option<Duration>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct OutboundFailure {
        pub kind: OutboundFailureKind,
    }

    impl OutboundFailure {
        pub fn new(kind: OutboundFailureKind) -> Self {
            Self { kind }
        }
    }

    impl std::fmt::Display for OutboundFailure {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "{:?}", self.kind)
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum OutboundFailureKind {
        InvalidUrl,
        InvalidHeader,
        HeaderNotAllowed(String),
        ProxyPolicy,
        TransportPolicy,
        ConnectTimeout,
        FirstByteTimeout,
        BodyTimeout,
        TotalTimeout,
        BudgetExhausted,
        Cancelled,
        BodyLimitExceeded { limit_bytes: usize },
        RedirectBlocked,
        RedirectLoop,
        RedirectLimitExceeded,
        RetryAfterExceedsBudget,
        RequestFailed,
    }

    pub struct OutboundHeaderPolicy;

    impl OutboundHeaderPolicy {
        pub fn provider_default() -> Self {
            Self
        }
    }

    pub struct OutboundHeaders {
        entries: Vec<(String, String, bool)>,
    }

    impl std::fmt::Debug for OutboundHeaders {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let entries = self
                .entries
                .iter()
                .map(|(name, value, sensitive)| {
                    if *sensitive {
                        (name.as_str(), "<redacted>")
                    } else {
                        (name.as_str(), value.as_str())
                    }
                })
                .collect::<Vec<_>>();
            formatter.debug_list().entries(entries).finish()
        }
    }

    impl OutboundHeaders {
        pub fn new() -> Self {
            Self {
                entries: Vec::new(),
            }
        }

        pub fn insert_sensitive(
            &mut self,
            name: HeaderName,
            value: SecretHeaderValue,
            _policy: &OutboundHeaderPolicy,
        ) -> Result<(), OutboundFailure> {
            self.entries
                .push((name.as_str().to_string(), value.expose().to_string(), true));
            Ok(())
        }

        pub fn insert_public(
            &mut self,
            name: HeaderName,
            value: HeaderValue,
            _policy: &OutboundHeaderPolicy,
        ) -> Result<(), OutboundFailure> {
            let value = value
                .to_str()
                .map_err(|_| OutboundFailure::new(OutboundFailureKind::InvalidHeader))?;
            self.entries
                .push((name.as_str().to_string(), value.to_string(), false));
            Ok(())
        }

        pub fn materialize(
            &self,
            _policy: &OutboundHeaderPolicy,
        ) -> Result<HeaderMap, OutboundFailure> {
            let mut headers = HeaderMap::new();
            for (name, value, _) in &self.entries {
                let name = HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| OutboundFailure::new(OutboundFailureKind::InvalidHeader))?;
                let value = HeaderValue::from_str(value)
                    .map_err(|_| OutboundFailure::new(OutboundFailureKind::InvalidHeader))?;
                headers.insert(name, value);
            }
            Ok(headers)
        }
    }

    pub struct SecretHeaderValue(String);

    impl SecretHeaderValue {
        pub fn new(value: impl Into<String>) -> Self {
            Self(value.into())
        }

        fn expose(&self) -> &str {
            self.0.as_str()
        }
    }

    fn execute_local_http(request: OutboundRequest) -> Result<OutboundResponse, String> {
        let (host, port, path) = parse_http_url(&request.url)?;
        let mut stream =
            TcpStream::connect((host.as_str(), port)).map_err(|error| error.to_string())?;
        if let Some(timeout) = request.budget.remaining() {
            stream
                .set_read_timeout(Some(timeout))
                .map_err(|error| error.to_string())?;
            stream
                .set_write_timeout(Some(timeout))
                .map_err(|error| error.to_string())?;
        }
        let mut bytes = Vec::new();
        write!(
            bytes,
            "{} {} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n",
            request.method.as_str(),
            path
        )
        .map_err(|error| error.to_string())?;
        let has_content_length = request
            .headers
            .entries
            .iter()
            .any(|(name, _, _)| name.eq_ignore_ascii_case("content-length"));
        for (name, value, _) in &request.headers.entries {
            write!(bytes, "{name}: {value}\r\n").map_err(|error| error.to_string())?;
        }
        if !request.body.is_empty() && !has_content_length {
            write!(bytes, "Content-Length: {}\r\n", request.body.len())
                .map_err(|error| error.to_string())?;
        }
        bytes.extend_from_slice(b"\r\n");
        bytes.extend_from_slice(&request.body);
        stream
            .write_all(&bytes)
            .map_err(|error| error.to_string())?;

        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .map_err(|error| error.to_string())?;
        let header_end = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or_else(|| "missing response header end".to_string())?;
        let header_text =
            std::str::from_utf8(&response[..header_end]).map_err(|error| error.to_string())?;
        let mut response_headers = HeaderMap::new();
        let status = header_text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|status| status.parse::<u16>().ok())
            .ok_or_else(|| "missing response status".to_string())?;
        for line in header_text.lines().skip(1) {
            let Some((name, value)) = line.split_once(':') else {
                return Err("malformed response header".to_string());
            };
            let name = HeaderName::from_bytes(name.trim().as_bytes())
                .map_err(|error| error.to_string())?;
            let value = HeaderValue::from_str(value.trim()).map_err(|error| error.to_string())?;
            response_headers.append(name, value);
        }
        Ok(OutboundResponse {
            status: StatusCode::from_u16(status).map_err(|error| error.to_string())?,
            headers: response_headers,
            body: Bytes::copy_from_slice(&response[(header_end + 4)..]),
            evidence: OutboundEvidence {
                final_url: request.url,
                retry_after: None,
            },
        })
    }

    fn parse_http_url(url: &str) -> Result<(String, u16, String), String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| "only http fixture URLs are supported".to_string())?;
        let (authority, path) = rest
            .split_once('/')
            .map(|(authority, path)| (authority, format!("/{path}")))
            .unwrap_or((rest, "/".to_string()));
        let (host, port) = authority
            .rsplit_once(':')
            .ok_or_else(|| "fixture URL is missing port".to_string())?;
        let port = port
            .parse::<u16>()
            .map_err(|error| format!("invalid fixture port: {error}"))?;
        Ok((host.to_string(), port, path))
    }
}

mod models {
    pub mod credentials {
        #[derive(Debug, Clone)]
        pub struct PersistStationSessionInput {
            pub station_id: String,
            pub access_token: Option<String>,
            pub refresh_token: Option<String>,
            pub cookie: Option<String>,
            pub newapi_user_id: Option<String>,
            pub token_expires_at: Option<i64>,
            pub session_expires_at: Option<i64>,
            pub session_source: String,
        }
    }

    pub mod remote_keys {
        pub fn api_key_fingerprint(secret: &str) -> Option<String> {
            crate::services::remote_keys::api_key_fingerprint(secret)
        }

        #[derive(Debug, Clone, PartialEq)]
        pub struct RemoteStationKey {
            pub id: String,
            pub station_id: String,
            pub remote_key_id_hash: Option<String>,
            pub remote_key_name: Option<String>,
            pub api_key_masked: Option<String>,
            pub api_key_fingerprint: Option<String>,
            pub group_id_hash: Option<String>,
            pub group_name: Option<String>,
            pub tier_label: Option<String>,
            pub rate_multiplier: Option<f64>,
            pub rate_source: Option<String>,
            pub created_at: Option<String>,
            pub last_used_at: Option<String>,
            pub raw_source: String,
            pub match_status: RemoteKeyMatchStatus,
            pub matched_station_key_id: Option<String>,
            pub match_confidence: f64,
            pub collected_at: String,
        }

        #[derive(Debug, Clone, PartialEq)]
        pub struct CreateRemoteStationKeyInput {
            pub station_id: String,
            pub name: String,
            pub group_binding_id: Option<String>,
            pub group_id_hash: Option<String>,
            pub group_name: Option<String>,
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum RemoteKeyMatchStatus {
            Matched,
            Possible,
            Unbound,
        }
    }

    pub mod station_keys {
        #[derive(Debug, Clone)]
        pub struct StationKey {
            pub id: String,
            pub station_id: String,
            pub name: String,
            pub api_key_masked: String,
            pub api_key_present: bool,
            pub enabled: bool,
            pub priority: i64,
            pub max_concurrency: i64,
            pub load_factor: Option<i64>,
            pub schedulable: bool,
            pub group_name: Option<String>,
            pub tier_label: Option<String>,
            pub group_binding_id: Option<String>,
            pub group_id_hash: Option<String>,
            pub rate_multiplier: Option<f64>,
            pub manual_rate_multiplier: Option<f64>,
            pub manual_rate_updated_at: Option<String>,
            pub rate_source: Option<String>,
            pub rate_collected_at: Option<String>,
            pub balance_scope: Option<String>,
            pub status: String,
            pub last_checked_at: Option<String>,
            pub last_used_at: Option<String>,
            pub note: Option<String>,
            pub created_at: String,
            pub updated_at: String,
        }
    }

    pub mod stations {
        #[derive(Debug, Clone)]
        pub struct Station {
            pub id: String,
            pub website_url: String,
            pub endpoint_revision: i64,
        }
    }
}

mod services {
    pub mod secrets {
        pub mod mask {
            use serde_json::Value;

            pub fn redact_text(text: &str) -> String {
                text.split_whitespace()
                    .map(|segment| {
                        let lower = segment.to_lowercase();
                        if segment.len() > 18
                            && (lower.starts_with("sk-")
                                || lower.contains("api_key=")
                                || lower.contains("token=")
                                || lower.contains("authorization")
                                || lower.contains("password=")
                                || lower.contains("session="))
                        {
                            "[REDACTED]".to_string()
                        } else {
                            segment.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            }

            pub fn redact_value(value: &Value) -> Value {
                match value {
                    Value::Object(map) => Value::Object(
                        map.iter()
                            .map(|(key, child)| {
                                let lower = key.to_lowercase();
                                let redacted = if [
                                    "api_key",
                                    "key",
                                    "token",
                                    "authorization",
                                    "cookie",
                                    "password",
                                    "secret",
                                    "session",
                                    "credential",
                                ]
                                .iter()
                                .any(|hint| lower.contains(hint))
                                {
                                    Value::String("[REDACTED]".to_string())
                                } else {
                                    redact_value(child)
                                };
                                (key.clone(), redacted)
                            })
                            .collect(),
                    ),
                    Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
                    Value::String(text) if text.len() > 18 && text.starts_with("sk-") => {
                        Value::String("[REDACTED]".to_string())
                    }
                    _ => value.clone(),
                }
            }

            pub fn mask_secret(secret: &str) -> String {
                let trimmed = secret.trim();
                if trimmed.is_empty() {
                    return "未设置".to_string();
                }
                if trimmed.chars().count() <= 8 {
                    return "****".to_string();
                }
                let prefix: String = trimmed.chars().take(4).collect();
                let suffix: String = trimmed
                    .chars()
                    .rev()
                    .take(4)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                format!("{prefix}********{suffix}")
            }
        }
    }

    pub mod remote_keys {
        pub fn api_key_fingerprint(secret: &str) -> Option<String> {
            (!secret.trim().is_empty()).then(|| format!("fingerprint:{}", secret.len()))
        }
    }

    pub mod time {
        pub fn now_millis_for_services() -> u64 {
            1
        }
    }

    pub mod station_endpoints {
        pub fn build_api_url(base_url: &str, path: &str) -> Result<String, String> {
            let base = base_url.trim_end_matches('/');
            if !path.starts_with('/') || path.contains("://") {
                return Err("invalid path".to_string());
            }
            let resource = path.strip_prefix("/v1/").unwrap_or(path);
            Ok(format!("{base}/{resource}"))
        }

        pub fn build_management_url(base_url: &str, path: &str) -> Result<String, String> {
            let base = base_url.trim_end_matches('/');
            if !path.starts_with('/') || path.contains("://") {
                return Err("invalid path".to_string());
            }
            Ok(format!("{base}/{}", path.trim_start_matches('/')))
        }
    }

    pub mod group_categories {
        pub fn infer_group_category(
            group_name: &str,
            _raw_json_redacted: Option<&serde_json::Value>,
        ) -> String {
            group_name.trim().to_lowercase()
        }
    }

    pub mod collectors {
        pub trait CollectorSourcePort {
            fn persist_station_session<'a>(
                &'a self,
                input: crate::models::credentials::PersistStationSessionInput,
                expected_endpoint_revision: i64,
            ) -> futures_util::future::BoxFuture<'a, Result<(), String>>;
        }

        pub mod facts {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/services/collectors/facts.rs"
            ));
        }

        pub mod evidence {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/services/collectors/evidence.rs"
            ));
        }

        pub mod failure {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/services/collectors/failure.rs"
            ));
        }

        pub mod contract {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/services/collectors/contract.rs"
            ));
        }

        pub mod drivers {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/services/collectors/drivers/mod.rs"
            ));
        }

        pub mod orchestration {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/services/collectors/orchestration.rs"
            ));
        }

        pub mod manual_authorization {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/services/collectors/manual_authorization.rs"
            ));
        }
    }
}

use services::collectors::{
    contract::{ProviderDescriptor, ProviderKind},
    drivers::{static_provider_entries, REQUIRED_PROVIDER_KINDS},
    failure::DriverFailureKind,
    orchestration::ProviderRegistry,
};

const REQUIRED_SUPPORTED_SCENARIOS: [&str; 11] = [
    "success",
    "partial",
    "auth_failure",
    "rate_limit",
    "server_failure",
    "malformed",
    "unknown_shape",
    "cancel",
    "budget_exhaustion",
    "stale_endpoint_revision",
    "redaction",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapabilityMatrix {
    schema_version: u32,
    providers: Vec<ProviderMatrix>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderMatrix {
    kind: String,
    display_name: String,
    station_types: Vec<String>,
    capabilities: BTreeMap<String, CapabilityMatrixEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapabilityMatrixEntry {
    status: String,
    declared_in_registry: bool,
    fixture_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureManifest {
    schema_version: u32,
    cases: Vec<FixtureCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureCase {
    id: String,
    provider_kind: String,
    capability: String,
    scenario: String,
    endpoint_role: String,
    request_schema: serde_json::Value,
    response_schema: serde_json::Value,
    redaction: RedactionFixture,
    source: FixtureSource,
    expected: FixtureExpected,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RedactionFixture {
    status: String,
    secret_canaries: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureSource {
    kind: String,
    path: String,
    provenance: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureExpected {
    registry_status: String,
    failure_kind: String,
    canonical_facts: Option<serde_json::Value>,
}

#[test]
fn provider_capability_matrix_matches_static_registry() {
    let root = repo_root();
    let matrix = read_matrix(&root);
    assert_eq!(matrix.schema_version, 1);
    let registry = ProviderRegistry::new(static_provider_entries(), REQUIRED_PROVIDER_KINDS)
        .expect("registry");
    let providers = matrix
        .providers
        .iter()
        .map(|provider| (provider.kind.as_str(), provider))
        .collect::<BTreeMap<_, _>>();

    for kind in REQUIRED_PROVIDER_KINDS {
        let descriptor = registry.descriptor(*kind).expect("registered descriptor");
        let matrix_provider = providers
            .get(kind.as_str())
            .unwrap_or_else(|| panic!("matrix missing provider {}", kind.as_str()));
        assert_descriptor_matches_matrix(descriptor, matrix_provider);
        assert_capability_matches_matrix(
            &registry,
            *kind,
            "collector",
            descriptor.capabilities.collector.is_some(),
            matrix_provider.capabilities.get("collector"),
        );
        assert_capability_matches_matrix(
            &registry,
            *kind,
            "remote_key",
            descriptor.capabilities.remote_key.is_some(),
            matrix_provider.capabilities.get("remote_key"),
        );
        assert_capability_matches_matrix(
            &registry,
            *kind,
            "authorization",
            descriptor.capabilities.authorization.is_some(),
            matrix_provider.capabilities.get("authorization"),
        );
    }
    assert_eq!(
        providers.len(),
        REQUIRED_PROVIDER_KINDS.len(),
        "matrix must not declare unregistered provider kinds"
    );
}

#[test]
fn provider_fixtures_are_complete_for_declared_matrix() {
    let root = repo_root();
    let matrix = read_matrix(&root);
    let manifest = read_fixture_manifest(&root);
    assert_eq!(manifest.schema_version, 1);
    let fixture_ids = manifest
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();

    for case in &manifest.cases {
        assert_fixture_case_is_structured(case);
    }

    for provider in &matrix.providers {
        for (capability, entry) in &provider.capabilities {
            assert!(
                matches!(entry.status.as_str(), "unsupported" | "supported"),
                "invalid capability status for {} {}",
                provider.kind,
                capability
            );
            assert!(
                !entry.fixture_ids.is_empty(),
                "matrix entry {} {} must cite at least one fixture",
                provider.kind,
                capability
            );
            for fixture_id in &entry.fixture_ids {
                let fixture = fixture_ids
                    .get(fixture_id.as_str())
                    .unwrap_or_else(|| panic!("matrix cites missing fixture {fixture_id}"));
                assert_eq!(fixture.provider_kind, provider.kind);
                assert_eq!(fixture.capability, *capability);
            }
            if entry.status == "supported" {
                let scenarios = entry
                    .fixture_ids
                    .iter()
                    .map(|fixture_id| fixture_ids[fixture_id.as_str()].scenario.as_str())
                    .collect::<BTreeSet<_>>();
                for scenario in REQUIRED_SUPPORTED_SCENARIOS {
                    assert!(
                        scenarios.contains(scenario),
                        "supported capability {} {} missing {scenario} fixture",
                        provider.kind,
                        capability
                    );
                }
            } else {
                assert!(
                    entry.fixture_ids.iter().any(|fixture_id| {
                        fixture_ids[fixture_id.as_str()].scenario == "unsupported"
                    }),
                    "unsupported capability {} {} must have an unsupported fixture",
                    provider.kind,
                    capability
                );
            }
        }
    }
}

fn assert_descriptor_matches_matrix(descriptor: &ProviderDescriptor, provider: &ProviderMatrix) {
    assert_eq!(provider.display_name, descriptor.display_name);
    assert_eq!(
        provider.station_types,
        descriptor
            .station_types
            .iter()
            .map(|station_type| station_type.to_string())
            .collect::<Vec<_>>()
    );
}

fn assert_capability_matches_matrix(
    registry: &ProviderRegistry,
    kind: ProviderKind,
    capability: &str,
    declared: bool,
    entry: Option<&CapabilityMatrixEntry>,
) {
    let entry = entry.unwrap_or_else(|| {
        panic!(
            "matrix missing capability {capability} for provider {}",
            kind.as_str()
        )
    });
    assert_eq!(
        entry.declared_in_registry,
        declared,
        "matrix declaration drift for {} {capability}",
        kind.as_str()
    );
    if declared {
        assert_eq!(entry.status, "supported");
        return;
    }

    assert_eq!(entry.status, "unsupported");
    let failure = match capability {
        "collector" => registry.collector(kind).map(|_| ()),
        "remote_key" => registry.remote_key(kind).map(|_| ()),
        "authorization" => registry.authorization(kind).map(|_| ()),
        other => panic!("unsupported capability key {other}"),
    }
    .expect_err("unsupported matrix capability must fail as typed Unsupported");
    assert_eq!(failure.kind, DriverFailureKind::Unsupported);
}

fn assert_fixture_case_is_structured(case: &FixtureCase) {
    assert!(!case.id.trim().is_empty(), "fixture id is required");
    assert!(
        ["sub2api", "newapi", "openai-compatible"].contains(&case.provider_kind.as_str()),
        "fixture {} has unknown provider kind",
        case.id
    );
    assert!(
        ["collector", "remote_key", "authorization"].contains(&case.capability.as_str()),
        "fixture {} has unknown capability",
        case.id
    );
    assert!(
        [
            "api_base",
            "website",
            "balance",
            "groups",
            "models",
            "remote_keys",
            "authorization",
            "unknown"
        ]
        .contains(&case.endpoint_role.as_str()),
        "fixture {} has unknown endpoint role",
        case.id
    );
    assert!(
        case.request_schema.is_object(),
        "fixture {} must describe request schema",
        case.id
    );
    assert!(
        case.response_schema.is_object(),
        "fixture {} must describe response schema",
        case.id
    );
    assert!(
        matches!(
            case.redaction.status.as_str(),
            "not_applicable" | "redacted" | "verified"
        ),
        "fixture {} has invalid redaction status",
        case.id
    );
    assert!(
        case.redaction
            .secret_canaries
            .iter()
            .all(|canary| !canary.trim().is_empty()),
        "fixture {} has blank secret canary",
        case.id
    );
    assert!(
        !case.source.kind.trim().is_empty()
            && !case.source.path.trim().is_empty()
            && !case.source.provenance.trim().is_empty(),
        "fixture {} must include source provenance",
        case.id
    );
    assert_eq!(
        case.expected.registry_status,
        if case.scenario == "unsupported" {
            "unsupported"
        } else {
            "supported"
        },
        "fixture {} expected registry status does not match scenario",
        case.id
    );
    if case.scenario == "unsupported" {
        assert_eq!(case.expected.failure_kind, "unsupported");
        assert!(case.expected.canonical_facts.is_none());
    }
}

fn read_matrix(root: &Path) -> CapabilityMatrix {
    read_json(&root.join("docs/audits/provider-capability-matrix.json"))
}

fn read_fixture_manifest(root: &Path) -> FixtureManifest {
    read_json(&root.join("src-tauri/tests/fixtures/providers/manifest.json"))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&content)
        .unwrap_or_else(|error| panic!("invalid JSON {}: {error}", path.display()))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}
