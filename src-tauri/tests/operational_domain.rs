#[path = "../src/models/operational/mod.rs"]
mod operational;

use operational::{
    BalanceFacts, BalanceScope, CapabilityDimension, CapabilityEvidence, CapabilityVerdict,
    CurrencyCode, EndpointFacts, EndpointHealthFact, EndpointHealthTarget, EndpointId, EndpointRef,
    EndpointRevision, EvidenceCoverage, EvidenceFreshness, EvidenceSource, FactProvenance,
    HealthFact, HealthState, ModelHealthFact, ModelHealthTarget, ModelName, Money, MoneyAmount,
    OutboundPolicyRef, PriceConfidence, PricingUnit, RateMultiplier, RecordRevision,
    RequestCostBasis, RequestModelCapabilityAssessment, RequestPricingAssessment, SanitizedOrigin,
    StationAccountHealthFact, StationAccountHealthTarget, StationAccountRef, StationId,
    StationKeyCapabilityFacts, StationKeyHealthFact, StationKeyHealthTarget, StationKeyId,
    StationKeyOperationalFacts, UnixMillis,
};

fn station_id() -> StationId {
    StationId::new("station-a").expect("valid station")
}

fn station_key_id() -> StationKeyId {
    StationKeyId::new("key-a").expect("valid key")
}

fn provenance() -> FactProvenance {
    FactProvenance::new(
        EvidenceSource::Collector,
        RecordRevision::new(1).expect("valid revision"),
        UnixMillis::new(1_000).expect("valid timestamp"),
        EvidenceFreshness::Fresh,
    )
}

fn endpoint_ref() -> EndpointRef {
    EndpointRef::new(
        station_id(),
        EndpointId::new("endpoint-a").expect("valid endpoint"),
        EndpointRevision::new(2).expect("valid endpoint revision"),
    )
}

#[test]
fn validated_primitives_reject_invalid_values() {
    assert!(StationId::new(" ").is_err());
    assert!(StationKeyId::new("").is_err());
    assert!(EndpointRevision::new(0).is_err());
    assert!(RecordRevision::new(-1).is_err());
    assert!(UnixMillis::new(-1).is_err());

    assert!(CurrencyCode::new("XXX").is_err());
    assert!(PricingUnit::try_from("character").is_err());
    assert!(MoneyAmount::new(-0.01).is_err());
    assert!(MoneyAmount::new(f64::NAN).is_err());
    assert!(MoneyAmount::new(f64::INFINITY).is_err());
    assert!(RateMultiplier::new(0.0).is_err());
    assert!(RateMultiplier::new(f64::NEG_INFINITY).is_err());
    assert!(PriceConfidence::new(1.1).is_err());
}

#[test]
fn capability_and_coverage_are_explicit_tri_state_values() {
    let supported = CapabilityEvidence::new(
        CapabilityDimension::Tools,
        CapabilityVerdict::Supported,
        EvidenceCoverage::Complete,
        provenance(),
    );
    let unsupported = CapabilityEvidence::new(
        CapabilityDimension::Vision,
        CapabilityVerdict::Unsupported,
        EvidenceCoverage::Partial,
        provenance(),
    );
    let unknown = CapabilityEvidence::new(
        CapabilityDimension::Reasoning,
        CapabilityVerdict::Unknown,
        EvidenceCoverage::Unknown,
        provenance(),
    );

    assert_eq!(supported.verdict(), CapabilityVerdict::Supported);
    assert_eq!(unsupported.verdict(), CapabilityVerdict::Unsupported);
    assert_eq!(unknown.verdict(), CapabilityVerdict::Unknown);
    assert_eq!(unknown.coverage(), EvidenceCoverage::Unknown);
    assert!(!unknown.verdict().is_supported());
}

#[test]
fn health_targets_are_typed_by_scope_not_collapsed_to_bool() {
    let station_key_health: StationKeyHealthFact = HealthFact::new(
        StationKeyHealthTarget::new(station_key_id()),
        HealthState::Available,
        provenance(),
    );
    let station_account_health: StationAccountHealthFact = HealthFact::new(
        StationAccountHealthTarget::new(station_id()),
        HealthState::Degraded,
        provenance(),
    );
    let endpoint_health: EndpointHealthFact = HealthFact::new(
        EndpointHealthTarget::new(endpoint_ref()),
        HealthState::Unavailable,
        provenance(),
    );
    let model_health: ModelHealthFact = HealthFact::new(
        ModelHealthTarget::new(station_key_id(), ModelName::new("gpt-4.1").expect("valid model")),
        HealthState::Unknown,
        provenance(),
    );

    assert_eq!(station_key_health.state(), HealthState::Available);
    assert_eq!(station_account_health.state(), HealthState::Degraded);
    assert_eq!(endpoint_health.state(), HealthState::Unavailable);
    assert_eq!(model_health.state(), HealthState::Unknown);
}

#[test]
fn endpoint_facts_store_only_safe_target_references() {
    assert!(SanitizedOrigin::from_endpoint_url("https://user:pass@example.com/v1").is_err());

    let origin =
        SanitizedOrigin::from_endpoint_url("https://api.example.com:8443/v1/chat?debug=leak-canary")
            .expect("sanitized origin");
    assert_eq!(origin.as_str(), "https://api.example.com:8443");

    let endpoint = EndpointFacts::new(
        endpoint_ref(),
        origin,
        OutboundPolicyRef::new("policy-direct").expect("valid policy"),
    );
    let debug = format!("{endpoint:?}");
    assert!(!debug.contains("debug=leak-canary"));
    assert!(!debug.contains("user:pass"));
    assert_eq!(endpoint.endpoint_ref().revision().get(), 2);
}

#[test]
fn station_account_ref_is_the_station_identity_not_a_second_account_root() {
    let station = station_id();
    let account = StationAccountRef::new(station.clone());

    assert_eq!(account.station_id(), &station);
}

#[test]
fn station_key_operational_facts_exclude_request_specific_model_and_pricing_verdicts() {
    let complete = EvidenceCoverage::Complete;
    let capabilities = StationKeyCapabilityFacts::new(
        CapabilityEvidence::new(
            CapabilityDimension::Tools,
            CapabilityVerdict::Supported,
            complete,
            provenance(),
        ),
        CapabilityEvidence::new(
            CapabilityDimension::Vision,
            CapabilityVerdict::Unknown,
            EvidenceCoverage::Unknown,
            provenance(),
        ),
        CapabilityEvidence::new(
            CapabilityDimension::Reasoning,
            CapabilityVerdict::Unsupported,
            EvidenceCoverage::Partial,
            provenance(),
        ),
    );
    let usd = CurrencyCode::new("USD").expect("known currency");
    let balance = BalanceFacts::new(
        Money::new(MoneyAmount::new(10.0).expect("valid money"), usd.clone()),
        Money::new(MoneyAmount::new(1.0).expect("valid money"), usd),
        BalanceScope::StationKey,
        provenance(),
    );
    let key_health = HealthFact::new(
        StationKeyHealthTarget::new(station_key_id()),
        HealthState::Available,
        provenance(),
    );
    let account_health = HealthFact::new(
        StationAccountHealthTarget::new(station_id()),
        HealthState::Available,
        provenance(),
    );
    let endpoint_health = HealthFact::new(
        EndpointHealthTarget::new(endpoint_ref()),
        HealthState::Available,
        provenance(),
    );
    let facts = StationKeyOperationalFacts::new(
        station_key_id(),
        station_id(),
        StationAccountRef::new(station_id()),
        EndpointFacts::new(
            endpoint_ref(),
            SanitizedOrigin::from_endpoint_url("https://api.example.com/v1").expect("origin"),
            OutboundPolicyRef::new("policy-direct").expect("policy"),
        ),
        capabilities,
        balance,
        key_health,
        account_health,
        endpoint_health,
    );

    assert_eq!(facts.station_key_id().as_str(), "key-a");
    assert_eq!(facts.endpoint().sanitized_origin().as_str(), "https://api.example.com");

    let model_assessment = RequestModelCapabilityAssessment::new(
        ModelName::new("gpt-4.1").expect("model"),
        CapabilityVerdict::Supported,
        EvidenceCoverage::Partial,
        provenance(),
    );
    let pricing_assessment = RequestPricingAssessment::new(
        RequestCostBasis::MultiplierProxy,
        PricingUnit::InputToken,
        RateMultiplier::new(1.25).expect("valid multiplier"),
        PriceConfidence::new(0.8).expect("valid confidence"),
        provenance(),
    );

    assert_eq!(model_assessment.verdict(), CapabilityVerdict::Supported);
    assert_eq!(pricing_assessment.basis(), RequestCostBasis::MultiplierProxy);
}
