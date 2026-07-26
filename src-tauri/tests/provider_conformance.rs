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

    #[derive(Clone)]
    pub struct AsyncOutboundClient;

    #[derive(Clone)]
    pub struct ProxyPolicy;

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
        }
    }

    pub mod collectors {
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
    drivers::{stage19a_static_entries, REQUIRED_PROVIDER_KINDS},
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
    let registry = ProviderRegistry::new(stage19a_static_entries(), REQUIRED_PROVIDER_KINDS)
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
