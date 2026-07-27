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
            _request: OutboundRequest,
            _cancellation_token: CancellationToken,
        ) -> Result<OutboundResponse, OutboundFailure> {
            Err(OutboundFailure::new(OutboundFailureKind::RequestFailed))
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

    #[derive(Debug)]
    pub struct OutboundHeaders;

    impl OutboundHeaders {
        pub fn new() -> Self {
            Self
        }

        pub fn insert_sensitive(
            &mut self,
            _name: HeaderName,
            _value: SecretHeaderValue,
            _policy: &OutboundHeaderPolicy,
        ) -> Result<(), OutboundFailure> {
            Ok(())
        }

        pub fn insert_public(
            &mut self,
            _name: HeaderName,
            _value: HeaderValue,
            _policy: &OutboundHeaderPolicy,
        ) -> Result<(), OutboundFailure> {
            Ok(())
        }
    }

    pub struct SecretHeaderValue(String);

    impl SecretHeaderValue {
        pub fn new(value: impl Into<String>) -> Self {
            Self(value.into())
        }
    }
}

mod models {
    pub mod remote_keys {
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
                if trimmed.len() <= 8 {
                    return "[REDACTED]".to_string();
                }
                format!("{}********{}", &trimmed[..4], &trimmed[trimmed.len() - 4..])
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
            Ok(format!("{base}{path}"))
        }

        pub fn build_management_url(base_url: &str, path: &str) -> Result<String, String> {
            build_api_url(base_url, path)
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
        pub mod adapters {
            pub mod sub2api {
                use serde_json::Value;

                use crate::{
                    models::{
                        remote_keys::{
                            CreateRemoteStationKeyInput, RemoteKeyMatchStatus, RemoteStationKey,
                        },
                        station_keys::StationKey,
                    },
                    services::collectors::facts::{
                        CollectedBalanceFact, CollectedGroupFact, CollectorFacts,
                    },
                };

                #[derive(Debug, Clone, Copy)]
                pub struct DashboardUsageStats;

                pub fn parse_group_rate_facts(
                    station_id: &str,
                    available: &Value,
                    _rates: &Value,
                    _credit_per_cny: f64,
                ) -> CollectorFacts {
                    let mut facts = CollectorFacts::default();
                    if let Some(group_name) = available
                        .pointer("/groups/0")
                        .and_then(Value::as_str)
                        .or_else(|| available.pointer("/data/0/name").and_then(Value::as_str))
                    {
                        facts.groups.push(CollectedGroupFact {
                            station_id: station_id.to_string(),
                            group_id: Some(group_name.to_string()),
                            group_key_hash: format!("group:{group_name}"),
                            group_name: group_name.to_string(),
                            visibility: "available".to_string(),
                            inferred_group_category: None,
                            source: "sub2api_groups_available".to_string(),
                            confidence: 0.9,
                            raw_json_redacted: None,
                        });
                    }
                    facts
                }

                pub fn add_single_group_key_bindings(
                    _facts: &mut CollectorFacts,
                    _keys: &[StationKey],
                ) {
                }

                pub fn parse_usage_balance(
                    station_id: &str,
                    station_key_id: Option<String>,
                    _payload: &Value,
                    _credit_per_cny: f64,
                ) -> CollectedBalanceFact {
                    balance_fact(station_id, station_key_id, "station_key")
                }

                pub fn parse_account_balance(
                    station_id: &str,
                    _payload: &Value,
                    _credit_per_cny: f64,
                ) -> Option<CollectedBalanceFact> {
                    Some(balance_fact(station_id, None, "station"))
                }

                pub fn merge_account_profile_balance(
                    balances: &mut Vec<CollectedBalanceFact>,
                    profile_balance: CollectedBalanceFact,
                ) {
                    balances.push(profile_balance);
                }

                pub fn parse_dashboard_usage_stats(
                    _payload: &Value,
                ) -> Option<DashboardUsageStats> {
                    Some(DashboardUsageStats)
                }

                pub fn merge_dashboard_usage_stats(
                    _balances: &mut Vec<CollectedBalanceFact>,
                    _station_id: &str,
                    _stats: DashboardUsageStats,
                ) {
                }

                pub fn parse_remote_key_payload(
                    station_id: &str,
                    payload: &Value,
                ) -> Vec<RemoteStationKey> {
                    remote_key_items(payload)
                        .into_iter()
                        .enumerate()
                        .filter_map(|(index, value)| {
                            remote_key_from_value(station_id, value, index)
                        })
                        .collect()
                }

                pub fn remote_key_items(payload: &Value) -> Vec<&Value> {
                    if let Some(items) = payload.as_array() {
                        return items.iter().collect();
                    }
                    for pointer in [
                        "/data/items",
                        "/data/list",
                        "/data/keys",
                        "/data",
                        "/items",
                        "/list",
                        "/keys",
                    ] {
                        if let Some(items) = payload.pointer(pointer).and_then(Value::as_array) {
                            return items.iter().collect();
                        }
                    }
                    if payload.is_object() {
                        vec![payload]
                    } else {
                        Vec::new()
                    }
                }

                pub fn remote_key_from_value(
                    station_id: &str,
                    value: &Value,
                    index: usize,
                ) -> Option<RemoteStationKey> {
                    let name = value
                        .get("name")
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                    let full_key = full_key_from_key_value(value);
                    Some(RemoteStationKey {
                        id: format!(
                            "sub2api-remote-key-{}",
                            name.clone().unwrap_or_else(|| index.to_string())
                        ),
                        station_id: station_id.to_string(),
                        remote_key_id_hash: value
                            .get("id")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        remote_key_name: name,
                        api_key_masked: full_key
                            .as_deref()
                            .map(crate::services::secrets::mask::mask_secret),
                        api_key_fingerprint: None,
                        group_id_hash: None,
                        group_name: value
                            .get("group")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        tier_label: None,
                        rate_multiplier: None,
                        rate_source: Some("sub2api_keys".to_string()),
                        created_at: None,
                        last_used_at: None,
                        raw_source: "sub2api_keys".to_string(),
                        match_status: RemoteKeyMatchStatus::Unbound,
                        matched_station_key_id: None,
                        match_confidence: 0.0,
                        collected_at: "1".to_string(),
                    })
                }

                pub fn sub2api_group_id_value(group_id: &str) -> Value {
                    group_id
                        .parse::<i64>()
                        .map(Value::from)
                        .unwrap_or_else(|_| Value::from(group_id.to_string()))
                }

                pub fn remote_key_from_create_input(
                    station_id: &str,
                    input: &CreateRemoteStationKeyInput,
                    full_key: Option<&str>,
                ) -> RemoteStationKey {
                    RemoteStationKey {
                        id: format!("sub2api-remote-key-{}", input.name),
                        station_id: station_id.to_string(),
                        remote_key_id_hash: None,
                        remote_key_name: Some(input.name.clone()),
                        api_key_masked: full_key.map(crate::services::secrets::mask::mask_secret),
                        api_key_fingerprint: None,
                        group_id_hash: input.group_id_hash.clone(),
                        group_name: input.group_name.clone(),
                        tier_label: None,
                        rate_multiplier: None,
                        rate_source: Some("sub2api_keys".to_string()),
                        created_at: None,
                        last_used_at: None,
                        raw_source: "sub2api_keys".to_string(),
                        match_status: RemoteKeyMatchStatus::Unbound,
                        matched_station_key_id: None,
                        match_confidence: 0.0,
                        collected_at: "1".to_string(),
                    }
                }

                pub fn full_key_from_key_value(value: &Value) -> Option<String> {
                    value
                        .get("key")
                        .or_else(|| value.get("api_key"))
                        .or_else(|| value.get("apiKey"))
                        .or_else(|| value.get("token"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| value.len() >= 12 && !value.contains('*'))
                        .map(ToString::to_string)
                }

                pub fn full_key_from_create_payload(payload: &Value) -> Option<String> {
                    full_key_from_key_value(payload)
                        .or_else(|| {
                            payload
                                .pointer("/data/key")
                                .and_then(Value::as_str)
                                .map(ToString::to_string)
                        })
                        .or_else(|| {
                            payload
                                .pointer("/data/api_key")
                                .and_then(Value::as_str)
                                .map(ToString::to_string)
                        })
                        .or_else(|| {
                            payload
                                .pointer("/data/apiKey")
                                .and_then(Value::as_str)
                                .map(ToString::to_string)
                        })
                }

                fn balance_fact(
                    station_id: &str,
                    station_key_id: Option<String>,
                    scope: &str,
                ) -> CollectedBalanceFact {
                    CollectedBalanceFact {
                        station_id: station_id.to_string(),
                        station_key_id,
                        scope: scope.to_string(),
                        value: Some(1.0),
                        used_value: None,
                        total_value: None,
                        today_request_count: None,
                        total_request_count: None,
                        today_consumption: None,
                        total_consumption: None,
                        today_base_consumption: None,
                        total_base_consumption: None,
                        today_token_count: None,
                        total_token_count: None,
                        today_input_token_count: None,
                        today_output_token_count: None,
                        total_input_token_count: None,
                        total_output_token_count: None,
                        account_concurrency_limit: None,
                        currency: "CNY".to_string(),
                        credit_unit: None,
                        status: "normal".to_string(),
                        source: "sub2api_usage".to_string(),
                        confidence: 0.9,
                        collected_at: None,
                    }
                }
            }
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
    read_json(&root.join("docs/superpowers/audits/provider-capability-matrix.json"))
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
