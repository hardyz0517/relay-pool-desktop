use serde_json::Value;

#[path = "../src/models/operational/mod.rs"]
mod operational_model;

mod models {
    pub(crate) mod operational {
        pub(crate) use crate::operational_model::*;
    }
    pub(crate) mod routing {
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
        #[serde(rename_all = "camelCase")]
        pub(crate) struct RuntimeRoutingBalance {
            pub(crate) scope: String,
            pub(crate) value: Option<f64>,
            pub(crate) currency: String,
            pub(crate) low_balance_threshold: Option<f64>,
            pub(crate) status: String,
            pub(crate) collected_at: Option<String>,
        }

        impl RuntimeRoutingBalance {
            pub(crate) fn is_depleted(&self) -> bool {
                self.value.is_some_and(|value| value <= 0.0)
                    || self.value.is_none()
                        && matches!(
                            self.status.trim().to_ascii_lowercase().as_str(),
                            "depleted" | "exhausted" | "empty"
                        )
            }

            pub(crate) fn has_explicit_status(&self) -> bool {
                matches!(
                    self.status.trim().to_ascii_lowercase().as_str(),
                    "normal"
                        | "available"
                        | "usable"
                        | "low"
                        | "warning"
                        | "depleted"
                        | "exhausted"
                        | "empty"
                )
            }
        }
    }
}

mod operational_facts {
    #[path = "../../src/application/operational_facts/balance_projector.rs"]
    pub(crate) mod balance_projector;
    #[path = "../../src/application/operational_facts/capability_projector.rs"]
    pub(crate) mod capability_projector;
    #[path = "../../src/application/operational_facts/group_projector.rs"]
    pub(crate) mod group_projector;
    #[path = "../../src/application/operational_facts/multiplier_projector.rs"]
    pub(crate) mod multiplier_projector;
    pub(crate) mod pricing_projector {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum PricingVerdict {
            Exact,
            MultiplierProxy,
            Unpriced,
            NotApplicable,
            Ambiguous,
            Stale,
            Invalid,
        }
    }
    #[path = "../../src/application/operational_facts/asset_status_projector.rs"]
    pub(crate) mod asset_status_projector;
}

use operational_facts::{
    asset_status_projector::{project_asset_status, AssetStatus, AssetStatusInput},
    balance_projector::{
        project_balance, BalanceAmount, BalanceEvidenceStatus, BalanceObservation,
        BalanceProjectionStatus,
    },
    capability_projector::{
        project_capability, CanonicalCapabilityEvidence, CapabilityDecision,
        CapabilityEvidenceSource, CapabilityFeature, CapabilityProjectionPolicy, CapabilitySubject,
    },
    group_projector::{
        reduce_group, GroupProjectionInput, GroupStatus, GroupVerdict, ProjectionTrace,
    },
    multiplier_projector::{
        project_multiplier, MultiplierEvidence, MultiplierEvidenceKind, MultiplierProjectionInput,
        MultiplierResolutionStatus,
    },
    pricing_projector::PricingVerdict,
};
use operational_model::{
    BalanceScope, CapabilityVerdict, EndpointRevision, EvidenceConfidence, EvidenceCoverage,
    PriceConfidence, RateMultiplier, RecordRevision, UnixMillis,
};

#[test]
fn versioned_fixture_freezes_shared_projector_contract() {
    let fixture: Value = serde_json::from_slice(include_bytes!(
        "fixtures/intelligent_routing/projectors/v1.json"
    ))
    .expect("fixture");
    assert_eq!(fixture["schemaVersion"], 1);
    assert_eq!(
        fixture["projectorVersions"]["assetStatus"],
        "asset_status_rollup_v1"
    );
    assert_eq!(fixture["assetStatusIsUiOnly"], true);
    for state in fixture["unknownStates"].as_array().expect("states") {
        assert!(state.is_string());
    }
}

fn now() -> UnixMillis {
    UnixMillis::new(1_000).expect("now")
}
fn rev(value: i64) -> RecordRevision {
    RecordRevision::new(value).expect("revision")
}
fn trace() -> ProjectionTrace {
    ProjectionTrace::new(
        vec!["fixture"],
        PriceConfidence::new(0.9).expect("confidence"),
        now(),
        "fixture",
        vec![rev(1)],
    )
}

#[test]
fn every_reducer_emits_version_reason_and_source_reference() {
    let group = reduce_group(GroupProjectionInput {
        group_binding_id: Some("binding-a".into()),
        group_key_hash: None,
        group_id_hash: None,
        group_name: Some("Shared".into()),
        group_category: None,
        status: GroupStatus::Available,
        trace: trace(),
    });
    assert_eq!(group.verdict, GroupVerdict::Available);
    assert_eq!(group.trace.projector_version, "group_identity_v1");
    assert_eq!(group.trace.reason, "fixture");
    assert!(!group.trace.source_refs.is_empty());

    let balance = project_balance(
        Some(BalanceObservation {
            scope: BalanceScope::StationKey,
            status: BalanceEvidenceStatus::Available,
            balance: Some(BalanceAmount::new(100.0, "USD").expect("balance")),
            low_balance_threshold: Some(BalanceAmount::new(10.0, "USD").expect("threshold")),
            authoritative: true,
            fresh: true,
            revision: Some(rev(2)),
        }),
        None,
        now(),
    );
    assert_eq!(balance.trace.projector_version, "balance_scope_v1");
    assert!(!balance.trace.source_refs.is_empty());

    let multiplier = project_multiplier(MultiplierProjectionInput {
        disabled: false,
        ambiguous: false,
        manual_override: Some(MultiplierEvidence {
            kind: MultiplierEvidenceKind::ManualOverride,
            multiplier: RateMultiplier::new(1.0).expect("multiplier"),
            authoritative: true,
            fresh: true,
            revision: rev(3),
        }),
        binding_latest_user: None,
        binding_latest_effective: None,
        current_user: None,
        current_effective: None,
        current_default: None,
        resolved_at: now(),
    });
    assert_eq!(multiplier.trace.projector_version, "rate_precedence_v1");
    assert!(!multiplier.trace.source_refs.is_empty());

    let subject = CapabilitySubject::Feature(CapabilityFeature::Stream);
    let capability = project_capability(
        subject.clone(),
        &[],
        CapabilityProjectionPolicy {
            strict_unknown: true,
        },
        now(),
    );
    assert_eq!(
        capability.decision,
        CapabilityDecision::RequireStrictConfirmation
    );
    assert_eq!(capability.projector_version, "capability_evidence_v1");
    assert_eq!(capability.reason_code, "capability_unknown_strict");

    let asset = project_asset_status(AssetStatusInput {
        group: GroupVerdict::Available,
        pricing: PricingVerdict::Exact,
        balance: BalanceProjectionStatus::Healthy,
        capability: CapabilityDecision::Allow,
        multiplier: MultiplierResolutionStatus::Resolved,
        observed_at: Some(now()),
    });
    assert_eq!(asset.status, AssetStatus::Healthy);
    assert_eq!(asset.projector_version, "asset_status_rollup_v1");
    assert!(!asset.source_refs.is_empty());
}

#[test]
fn capability_output_tracks_evidence_identity_and_observation() {
    let subject = CapabilitySubject::Model("gpt-5-mini".into());
    let evidence = CanonicalCapabilityEvidence {
        id: "evidence-1".into(),
        subject: subject.clone(),
        verdict: CapabilityVerdict::Supported,
        source: CapabilityEvidenceSource::SuccessfulRequest,
        coverage: EvidenceCoverage::Complete,
        observed_at: now(),
        endpoint_revision: EndpointRevision::new(1).expect("endpoint revision"),
        confidence: EvidenceConfidence::new(0.8).expect("confidence"),
        expires_at: None,
    };
    let projection = project_capability(
        subject,
        &[evidence],
        CapabilityProjectionPolicy {
            strict_unknown: true,
        },
        now(),
    );
    assert_eq!(projection.reason_code, "capability_supported");
    assert_eq!(projection.source_refs, vec!["evidence-1"]);
    assert_eq!(projection.observed_at, Some(now()));
}
