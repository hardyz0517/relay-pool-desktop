use crate::{
    application::{
        operational_facts::{
            balance_projector::{BalanceProjection, BalanceProjectionStatus},
            candidate_projector::{
                project_route_candidate, CandidateIdentityProjection,
                CandidateOperationalProjections, CapabilityProjectionSet, CapacityProjection,
                CapacityScope, CapacityScopeSnapshot, HealthProjectionSet,
                RouteCandidateProjection,
            },
            capability_projector::{
                CapabilityDecision, CapabilityFeature, CapabilityProjection, CapabilityProtocol,
                CapabilitySubject,
            },
            group_projector::{
                project_group, GroupProjection, GroupProjectionInput, GroupStatus, ProjectionTrace,
            },
            health_projector::{
                EffectiveHealthProjection, HealthAdmission, HealthProjectionTarget,
            },
            multiplier_projector::{
                project_multiplier, MultiplierEvidence, MultiplierEvidenceKind,
                MultiplierProjection, MultiplierProjectionInput, MultiplierResolutionStatus,
            },
            pricing_projector::{
                request_cost_comparison_context, PricingRouteKind, RequestCostComparisonContext,
                RoutingCostBasis,
            },
        },
        routing_engine::{
            admission::CandidateAdmissionProfile,
            capacity::ProviderAccountConstraint,
            request::{
                CanonicalRouteRequest, GroupFilterMode, OrderingProfile, RouteKind,
                RouteRequestClassifier, RouteRequestFacts, ValidatedLocalRouteSettings,
            },
            routing_health::health_is_blocked,
        },
    },
    models::{
        operational::{
            CapabilityVerdict, ModelName, PriceConfidence, RecordRevision, StationId, StationKeyId,
            UnixMillis,
        },
        pricing::ResolvedPricingContext,
        routing::{
            CanonicalRoutingCandidate, RoutingGroupFilter, RoutingPolicy, RuntimeRoutingSettings,
        },
    },
};

pub(crate) fn route_request_facts_for_read_model(
    settings: &RuntimeRoutingSettings,
    admitted_at_ms: i64,
) -> RouteRequestFacts {
    RouteRequestClassifier::classify(
        CanonicalRouteRequest {
            route_kind: RouteKind::Inference,
            requested_model: None,
            stream: false,
            uses_tools: false,
            uses_vision: false,
            uses_reasoning: false,
            untrusted_headers: Vec::new(),
        },
        validated_route_settings(settings),
        admitted_at_ms,
    )
}

pub(crate) fn validated_route_settings(
    settings: &RuntimeRoutingSettings,
) -> ValidatedLocalRouteSettings {
    ValidatedLocalRouteSettings {
        ordering_profile: ordering_profile(&settings.policy),
        max_rate_multiplier: settings.max_rate_multiplier,
        group_filter_mode: group_filter_mode(&settings.routing_group_scope),
        required_group_stable_key: required_group_stable_key(&settings.routing_group_scope),
        preferred_models: Vec::new(),
        required_tags: Vec::new(),
        allow_depleted_fallback: settings.allow_depleted_fallback,
        affinity_enabled: false,
    }
}

pub(crate) fn ordering_profile(policy: &RoutingPolicy) -> OrderingProfile {
    match policy {
        RoutingPolicy::CheapFirst | RoutingPolicy::CostStableFirst => OrderingProfile::CostFirst,
        RoutingPolicy::AutomaticBalanced
        | RoutingPolicy::PriorityFallback
        | RoutingPolicy::StableFirst
        | RoutingPolicy::BackupOnly => OrderingProfile::PriorityFirst,
    }
}

pub(crate) fn group_filter_mode(filter: &RoutingGroupFilter) -> GroupFilterMode {
    match filter {
        RoutingGroupFilter::AllGroups => GroupFilterMode::Any,
        RoutingGroupFilter::UngroupedOnly => GroupFilterMode::UngroupedOnly,
        RoutingGroupFilter::GroupBindingId(_)
        | RoutingGroupFilter::GroupIdHash(_)
        | RoutingGroupFilter::GroupType(_) => GroupFilterMode::Required,
    }
}

pub(crate) fn required_group_stable_key(filter: &RoutingGroupFilter) -> Option<String> {
    match filter {
        RoutingGroupFilter::GroupBindingId(id) => Some(format!("binding:{id}")),
        RoutingGroupFilter::GroupIdHash(hash) => Some(format!("group-id:{hash}")),
        RoutingGroupFilter::GroupType(group_type) => Some(format!(
            "group-type:{}",
            match group_type {
                crate::models::routing::PricingGroupType::Gpt => "gpt",
                crate::models::routing::PricingGroupType::Claude => "claude",
                crate::models::routing::PricingGroupType::Gemini => "gemini",
                crate::models::routing::PricingGroupType::Grok => "grok",
                crate::models::routing::PricingGroupType::ImageGeneration => "image_generation",
            }
        )),
        RoutingGroupFilter::AllGroups | RoutingGroupFilter::UngroupedOnly => None,
    }
}

#[cfg(test)]
pub(crate) fn admission_profile_from_runtime_candidate(
    candidate: &CanonicalRoutingCandidate,
) -> CandidateAdmissionProfile {
    CandidateAdmissionProfile {
        endpoint_revision: candidate.station_endpoint_revision,
        expected_credential_revision: 1,
        credential_revision: 1,
        durable_generation: 1,
        global_max_concurrency: 1024,
        station_account_max_concurrency: station_account_max_concurrency(candidate),
        station_key_max_concurrency: station_key_max_concurrency(candidate),
        provider_account_constraint: ProviderAccountConstraint::NotApplicable,
        half_open_probe_id: None,
    }
}

#[cfg(test)]
pub fn route_projection_from_runtime_candidate(
    request: &RouteRequestFacts,
    candidate: CanonicalRoutingCandidate,
) -> Result<RouteCandidateProjection, String> {
    route_projection_from_runtime_candidate_with_pricing(request, candidate, None)
}

pub(crate) fn route_projection_from_runtime_candidate_with_pricing(
    request: &RouteRequestFacts,
    candidate: CanonicalRoutingCandidate,
    request_pricing: Option<&ResolvedPricingContext>,
) -> Result<RouteCandidateProjection, String> {
    let now = UnixMillis::new(request.admitted_at_ms()).map_err(|error| error.to_string())?;
    let identity = CandidateIdentityProjection {
        station_key_id: candidate.station_key_id.clone(),
        station_id: candidate.station_id.clone(),
        endpoint_revision: candidate.station_endpoint_revision,
        sanitized_origin: candidate.sanitized_origin.clone(),
        credential_available: candidate.api_key_secret.is_some()
            || candidate
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
    };
    let economics = candidate.economic_snapshot.as_ref();
    let group = group_projection(economics, now);
    let multiplier = multiplier_projection(economics, now);
    let pricing = pricing_context_for_request(request, request_pricing, economics, &multiplier);
    let operational = CandidateOperationalProjections {
        identity,
        priority: candidate.routing_order.unwrap_or(candidate.priority),
        resolved_model: request.requested_model().map(ToString::to_string),
        group,
        multiplier,
        pricing,
        balance: balance_projection(candidate.balance_snapshot.as_ref(), now),
        capabilities: capability_projection_set(request, &candidate)?,
        health: health_projection_set(request, &candidate, now)?,
        capacity: capacity_projection(&candidate),
        backup_only: candidate.capabilities.only_use_as_backup,
        candidate_tags: candidate.capabilities.routing_tags.clone(),
        snapshot_id: format!("endpoint-revision-{}", candidate.station_endpoint_revision),
        fact_version_vector: format!(
            "endpoint:{};capabilities:{};health:{};balance:{}",
            candidate.station_endpoint_revision,
            candidate.capabilities.updated_at,
            candidate
                .health
                .as_ref()
                .map(|health| health.updated_at.as_str())
                .unwrap_or("missing"),
            candidate
                .balance_snapshot
                .as_ref()
                .and_then(|balance| balance.collected_at.as_deref())
                .unwrap_or("missing"),
        ),
    };
    Ok(project_route_candidate(request, operational))
}

fn group_projection(
    economics: Option<&crate::models::routing::RuntimeRoutingEconomicSnapshot>,
    now: UnixMillis,
) -> Option<GroupProjection> {
    let economics = economics?;
    project_group(GroupProjectionInput {
        group_binding_id: economics.group_binding_id.clone(),
        group_key_hash: economics.group_key_hash.clone(),
        group_id_hash: economics.group_id_hash.clone(),
        group_name: economics.group_name.clone(),
        status: group_status(economics),
        trace: ProjectionTrace::new(
            vec!["runtime_candidate_projection", "station_key_group"],
            PriceConfidence::new(
                economics
                    .group_confidence
                    .filter(|value| value.is_finite())
                    .unwrap_or(0.8)
                    .clamp(0.0, 1.0),
            )
            .expect("valid confidence"),
            now,
            "runtime_candidate_group",
            revision_refs(economics),
        ),
    })
}

fn group_status(economics: &crate::models::routing::RuntimeRoutingEconomicSnapshot) -> GroupStatus {
    match economics.group_status.as_deref().map(str::trim) {
        Some("disabled") => GroupStatus::Disabled,
        Some("missing") => GroupStatus::Missing,
        Some("manual_legacy") => GroupStatus::Legacy,
        Some("available") | Some("bound") => GroupStatus::Available,
        _ if economics.group_binding_id.is_some()
            || economics.group_key_hash.is_some()
            || economics.group_id_hash.is_some() =>
        {
            GroupStatus::Available
        }
        _ if economics.group_name.is_some() => GroupStatus::Legacy,
        _ => GroupStatus::Missing,
    }
}

fn multiplier_projection(
    economics: Option<&crate::models::routing::RuntimeRoutingEconomicSnapshot>,
    now: UnixMillis,
) -> MultiplierProjection {
    let Some(economics) = economics else {
        return missing_multiplier(now);
    };
    project_multiplier(MultiplierProjectionInput {
        disabled: false,
        ambiguous: false,
        manual_override: multiplier_evidence(
            MultiplierEvidenceKind::ManualOverride,
            economics.manual_rate_multiplier,
            economics,
            now,
        ),
        binding_latest_user: None,
        binding_latest_effective: multiplier_evidence(
            MultiplierEvidenceKind::BindingLatestEffective,
            economics.rate_multiplier,
            economics,
            now,
        ),
        current_user: None,
        current_effective: multiplier_evidence(
            MultiplierEvidenceKind::CurrentEffective,
            economics.rate_multiplier,
            economics,
            now,
        ),
        current_default: None,
        resolved_at: now,
    })
}

fn missing_multiplier(now: UnixMillis) -> MultiplierProjection {
    project_multiplier(MultiplierProjectionInput {
        disabled: false,
        ambiguous: false,
        manual_override: None,
        binding_latest_user: None,
        binding_latest_effective: None,
        current_user: None,
        current_effective: None,
        current_default: None,
        resolved_at: now,
    })
}

fn multiplier_evidence(
    kind: MultiplierEvidenceKind,
    value: Option<f64>,
    economics: &crate::models::routing::RuntimeRoutingEconomicSnapshot,
    _now: UnixMillis,
) -> Option<MultiplierEvidence> {
    let multiplier = crate::models::operational::RateMultiplier::new(value?).ok()?;
    Some(MultiplierEvidence {
        kind,
        multiplier,
        authoritative: true,
        fresh: true,
        revision: revision_refs(economics).into_iter().next()?,
    })
}

fn pricing_context_for_request(
    request: &RouteRequestFacts,
    request_pricing: Option<&ResolvedPricingContext>,
    economics: Option<&crate::models::routing::RuntimeRoutingEconomicSnapshot>,
    multiplier: &MultiplierProjection,
) -> RequestCostComparisonContext {
    let route_kind = match request.route_kind() {
        RouteKind::Inference => PricingRouteKind::Inference,
        RouteKind::ModelCatalog => PricingRouteKind::ModelCatalog,
    };
    if route_kind == PricingRouteKind::ModelCatalog {
        return RequestCostComparisonContext {
            route_kind,
            basis: RoutingCostBasis::NotApplicable,
            comparison_value: None,
            reason: Some("model_catalog_has_no_request_cost"),
            currency: None,
            unit: None,
            estimated_input_price: None,
            estimated_output_price: None,
            estimated_fixed_price: None,
            status_label: "not_applicable".to_string(),
            source_chain: Vec::new(),
            observed_at: None,
            confidence: None,
        };
    }
    let request_pricing = request_cost_comparison_context(route_kind, request_pricing);
    if request_pricing.basis != RoutingCostBasis::Unpriced {
        return request_pricing;
    }
    if multiplier.status == MultiplierResolutionStatus::Resolved {
        let multiplier_value = multiplier.multiplier.map(|value| value.get());
        return RequestCostComparisonContext {
            route_kind,
            basis: RoutingCostBasis::MultiplierProxy,
            comparison_value: multiplier_value,
            reason: Some("cost_first_multiplier_proxy"),
            currency: None,
            unit: Some("rate_multiplier".to_string()),
            estimated_input_price: None,
            estimated_output_price: None,
            estimated_fixed_price: None,
            status_label: "multiplier_proxy".to_string(),
            source_chain: multiplier_pricing_source_chain(multiplier, economics),
            observed_at: economics.and_then(|value| observed_at_for_multiplier(multiplier, value)),
            confidence: Some(multiplier.trace.confidence.get()),
        };
    }
    RequestCostComparisonContext {
        route_kind,
        basis: RoutingCostBasis::Unpriced,
        comparison_value: None,
        reason: Some("pricing_context_missing"),
        currency: None,
        unit: None,
        estimated_input_price: None,
        estimated_output_price: None,
        estimated_fixed_price: None,
        status_label: "unpriced".to_string(),
        source_chain: Vec::new(),
        observed_at: None,
        confidence: None,
    }
}

fn multiplier_pricing_source_chain(
    multiplier: &MultiplierProjection,
    economics: Option<&crate::models::routing::RuntimeRoutingEconomicSnapshot>,
) -> Vec<String> {
    let mut chain = vec!["runtime_candidate_economic_snapshot".to_string()];
    if let Some(kind) = multiplier.selected_kind {
        chain.push(multiplier_source_label(kind).to_string());
    } else {
        chain.push(multiplier.trace.reason.to_string());
    }
    if let Some(source) = economics
        .and_then(|value| value.rate_source.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        chain.push(format!("rate_source:{source}"));
    }
    chain
}

fn observed_at_for_multiplier(
    multiplier: &MultiplierProjection,
    economics: &crate::models::routing::RuntimeRoutingEconomicSnapshot,
) -> Option<String> {
    match multiplier.selected_kind {
        Some(MultiplierEvidenceKind::ManualOverride) => economics
            .manual_rate_updated_at
            .clone()
            .or_else(|| economics.rate_collected_at.clone())
            .or_else(|| economics.group_checked_at.clone()),
        Some(MultiplierEvidenceKind::BindingLatestEffective)
        | Some(MultiplierEvidenceKind::BindingLatestUser) => economics
            .rate_collected_at
            .clone()
            .or_else(|| economics.group_checked_at.clone())
            .or_else(|| economics.manual_rate_updated_at.clone()),
        Some(MultiplierEvidenceKind::CurrentEffective)
        | Some(MultiplierEvidenceKind::CurrentUser)
        | Some(MultiplierEvidenceKind::CurrentDefault) => economics
            .rate_collected_at
            .clone()
            .or_else(|| economics.key_updated_at.clone())
            .or_else(|| economics.group_checked_at.clone()),
        None => economics
            .rate_collected_at
            .clone()
            .or_else(|| economics.group_checked_at.clone()),
    }
}

fn multiplier_source_label(kind: MultiplierEvidenceKind) -> &'static str {
    match kind {
        MultiplierEvidenceKind::BindingLatestUser => "binding_latest_user",
        MultiplierEvidenceKind::BindingLatestEffective => "binding_latest_effective",
        MultiplierEvidenceKind::CurrentUser => "current_user",
        MultiplierEvidenceKind::CurrentEffective => "current_effective",
        MultiplierEvidenceKind::CurrentDefault => "current_default",
        MultiplierEvidenceKind::ManualOverride => "manual_override",
    }
}

fn revision_refs(
    _economics: &crate::models::routing::RuntimeRoutingEconomicSnapshot,
) -> Vec<RecordRevision> {
    // Runtime economic rows do not carry a domain revision. Timestamps are
    // freshness evidence only and must never become a revision substitute.
    Vec::new()
}

fn balance_projection(
    balance: Option<&crate::models::routing::RuntimeRoutingBalance>,
    now: UnixMillis,
) -> BalanceProjection {
    let (status, reason) = match balance {
        Some(balance) if balance.is_depleted() => (
            BalanceProjectionStatus::DepletedEmergency,
            "balance_depleted",
        ),
        Some(_) => (BalanceProjectionStatus::Healthy, "balance_healthy"),
        None => (BalanceProjectionStatus::Missing, "balance_missing"),
    };
    BalanceProjection {
        status,
        selected_scope: None,
        health_hint: crate::models::operational::HealthState::Unknown,
        trace: ProjectionTrace::new(
            vec!["runtime_candidate_projection"],
            PriceConfidence::new(if balance.is_some() { 0.8 } else { 0.0 })
                .expect("valid confidence"),
            now,
            reason,
            Vec::new(),
        ),
    }
}

fn capability_projection_set(
    request: &RouteRequestFacts,
    candidate: &CanonicalRoutingCandidate,
) -> Result<CapabilityProjectionSet, String> {
    Ok(CapabilityProjectionSet {
        protocol: capability_projection(
            protocol_subject(request),
            protocol_supported(request, candidate),
        ),
        model: capability_projection(
            CapabilitySubject::Model(request.requested_model().unwrap_or("*").to_string()),
            model_supported(request.requested_model(), candidate),
        ),
        stream: capability_projection(
            CapabilitySubject::Feature(CapabilityFeature::Stream),
            !request.stream() || candidate.capabilities.supports_stream,
        ),
        tools: capability_projection(
            CapabilitySubject::Feature(CapabilityFeature::Tools),
            !request.uses_tools() || candidate.capabilities.supports_tools,
        ),
        vision: capability_projection(
            CapabilitySubject::Feature(CapabilityFeature::Vision),
            !request.uses_vision() || candidate.capabilities.supports_vision,
        ),
        reasoning: capability_projection(
            CapabilitySubject::Feature(CapabilityFeature::Reasoning),
            !request.uses_reasoning() || candidate.capabilities.supports_reasoning,
        ),
    })
}

fn protocol_subject(request: &RouteRequestFacts) -> CapabilitySubject {
    match request.route_kind() {
        RouteKind::ModelCatalog => CapabilitySubject::Protocol(CapabilityProtocol::ChatCompletions),
        RouteKind::Inference => CapabilitySubject::Protocol(CapabilityProtocol::ChatCompletions),
    }
}

fn protocol_supported(request: &RouteRequestFacts, candidate: &CanonicalRoutingCandidate) -> bool {
    if matches!(request.route_kind(), RouteKind::ModelCatalog) {
        return true;
    }
    candidate.capabilities.supports_chat_completions
        || candidate.capabilities.supports_responses
        || candidate.capabilities.supports_embeddings
}

fn model_supported(model: Option<&str>, candidate: &CanonicalRoutingCandidate) -> bool {
    let Some(model) = model else {
        return true;
    };
    if candidate
        .capabilities
        .model_blocklist
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(model))
    {
        return false;
    }
    candidate.capabilities.model_allowlist.is_empty()
        || candidate
            .capabilities
            .model_allowlist
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(model))
}

fn capability_projection(subject: CapabilitySubject, supported: bool) -> CapabilityProjection {
    CapabilityProjection {
        subject,
        truth: if supported {
            CapabilityVerdict::Supported
        } else {
            CapabilityVerdict::Unsupported
        },
        decision: if supported {
            CapabilityDecision::Allow
        } else {
            CapabilityDecision::Reject
        },
        #[cfg(test)] winner: None,
        #[cfg(test)] overridden: Vec::new(),
        #[cfg(test)] conflict_reason: None,
        projector_version: crate::application::operational_facts::capability_projector::CAPABILITY_PROJECTOR_VERSION,
        reason_code: if supported { "capability_supported" } else { "capability_unsupported" },
        #[cfg(test)] source_refs: Vec::new(),
        #[cfg(test)] observed_at: None,
        #[cfg(test)] confidence: None,
    }
}

fn health_projection_set(
    request: &RouteRequestFacts,
    candidate: &CanonicalRoutingCandidate,
    now: UnixMillis,
) -> Result<HealthProjectionSet, String> {
    let station_id =
        StationId::new(candidate.station_id.clone()).map_err(|error| error.to_string())?;
    let station_key_id =
        StationKeyId::new(candidate.station_key_id.clone()).map_err(|error| error.to_string())?;
    let key_admission = if !candidate.schedulable {
        HealthAdmission::HardReject
    } else if health_is_blocked(candidate.health.as_ref(), now.get()) {
        HealthAdmission::SuppressDurableCooldown
    } else if candidate
        .health
        .as_ref()
        .is_some_and(|health| health.consecutive_failures > 0)
    {
        HealthAdmission::AdmitDegraded
    } else {
        HealthAdmission::Admit
    };
    Ok(HealthProjectionSet {
        station_key: effective_health(
            HealthProjectionTarget::StationKey(station_key_id.clone()),
            key_admission,
        ),
        station_account: effective_health(
            HealthProjectionTarget::StationAccount(station_id.clone()),
            HealthAdmission::Admit,
        ),
        endpoint: effective_health(
            HealthProjectionTarget::Endpoint(crate::models::operational::EndpointRef::new(
                station_id,
                crate::models::operational::EndpointId::new(format!(
                    "{}:endpoint",
                    candidate.station_id
                ))
                .map_err(|error| error.to_string())?,
                crate::models::operational::EndpointRevision::new(
                    candidate.station_endpoint_revision,
                )
                .map_err(|error| error.to_string())?,
            )),
            HealthAdmission::Admit,
        ),
        model: effective_health(
            HealthProjectionTarget::Model {
                station_key_id,
                model: ModelName::new(request.requested_model().unwrap_or("*"))
                    .map_err(|error| error.to_string())?,
            },
            HealthAdmission::Admit,
        ),
    })
}

fn effective_health(
    target: HealthProjectionTarget,
    admission: HealthAdmission,
) -> EffectiveHealthProjection {
    EffectiveHealthProjection {
        target,
        admission,
        reasons: vec![match admission {
            HealthAdmission::Admit => "runtime_candidate_admit",
            HealthAdmission::AdmitDegraded => "runtime_candidate_degraded",
            HealthAdmission::SuppressOrdinaryRuntime => "runtime_candidate_suppressed",
            HealthAdmission::SuppressDurableCooldown => "runtime_candidate_cooldown",
            HealthAdmission::HardReject => "runtime_candidate_hard_reject",
            HealthAdmission::Unknown => "runtime_candidate_unknown",
        }],
        runtime_overlay_applied: false,
        stale_runtime_overlay_ignored: false,
    }
}

fn capacity_projection(candidate: &CanonicalRoutingCandidate) -> CapacityProjection {
    let (scope, limit) = if uses_station_account_capacity(&candidate.station_type) {
        (
            CapacityScope::StationAccount,
            positive_u32(station_account_max_concurrency(candidate).into()),
        )
    } else {
        (
            CapacityScope::StationKey,
            positive_u32(candidate.max_concurrency),
        )
    };
    CapacityProjection {
        scopes: vec![CapacityScopeSnapshot {
            scope,
            limit,
            in_flight: candidate
                .load_factor
                .and_then(positive_u32)
                .unwrap_or_default(),
            available: true,
            source_revision: Some(candidate.station_endpoint_revision),
        }],
    }
}

fn uses_station_account_capacity(station_type: &str) -> bool {
    matches!(
        station_type.trim().to_ascii_lowercase().as_str(),
        "sub2api" | "newapi"
    )
}

fn station_account_max_concurrency(candidate: &CanonicalRoutingCandidate) -> u32 {
    match candidate.station_type.trim().to_ascii_lowercase().as_str() {
        "newapi" => 0,
        "sub2api" => candidate
            .station_account_concurrency_limit
            .and_then(positive_u32)
            .or_else(|| positive_u32(candidate.max_concurrency))
            .unwrap_or(1),
        _ => 0,
    }
}

#[cfg(test)]
fn station_key_max_concurrency(candidate: &CanonicalRoutingCandidate) -> u32 {
    if uses_station_account_capacity(&candidate.station_type) {
        0
    } else {
        positive_u32(candidate.max_concurrency).unwrap_or(1)
    }
}

fn positive_u32(value: i64) -> Option<u32> {
    u32::try_from(value).ok().filter(|value| *value > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        pricing::{PricingStatus, RequestKind, ResolvedPricingContext},
        proxy::UpstreamApiFormat,
        routing::{PricingGroupType, RuntimeRoutingEconomicSnapshot, StationKeyCapabilities},
    };

    #[test]
    fn sub2api_uses_collected_account_concurrency_as_shared_capacity() {
        let mut candidate = runtime_candidate(RuntimeRoutingEconomicSnapshot::default());
        candidate.station_type = "sub2api".to_string();
        candidate.max_concurrency = 3;
        candidate.station_account_concurrency_limit = Some(8);

        let profile = admission_profile_from_runtime_candidate(&candidate);
        let projection = capacity_projection(&candidate);

        assert_eq!(profile.station_account_max_concurrency, 8);
        assert_eq!(profile.station_key_max_concurrency, 0);
        assert_eq!(projection.scopes[0].scope, CapacityScope::StationAccount);
        assert_eq!(projection.scopes[0].limit, Some(8));
    }

    #[test]
    fn newapi_has_unlimited_station_and_key_capacity() {
        let mut candidate = runtime_candidate(RuntimeRoutingEconomicSnapshot::default());
        candidate.station_type = "newapi".to_string();
        candidate.max_concurrency = 3;

        let profile = admission_profile_from_runtime_candidate(&candidate);
        let projection = capacity_projection(&candidate);

        assert_eq!(profile.station_account_max_concurrency, 0);
        assert_eq!(profile.station_key_max_concurrency, 0);
        assert_eq!(projection.scopes[0].scope, CapacityScope::StationAccount);
        assert_eq!(projection.scopes[0].limit, None);
    }

    #[test]
    fn runtime_candidate_projection_does_not_use_timestamp_as_multiplier_revision() {
        let now_ms = 1_800_000_000_000;
        let settings = RuntimeRoutingSettings {
            policy: RoutingPolicy::CostStableFirst,
            max_rate_multiplier: Some(1.0),
            routing_group_scope: RoutingGroupFilter::GroupBindingId("binding-gpt".to_string()),
            scheduler_config: Default::default(),
            allow_depleted_fallback: false,
        };
        let request = route_request_facts_for_read_model(&settings, now_ms);
        let candidate = runtime_candidate(RuntimeRoutingEconomicSnapshot {
            group_binding_id: Some("binding-gpt".to_string()),
            group_key_hash: Some("hash-gpt".to_string()),
            group_id_hash: Some("gid-gpt".to_string()),
            group_name: Some("GPT Group".to_string()),
            group_status: Some("bound".to_string()),
            group_confidence: Some(0.91),
            group_checked_at: Some("2026-07-31T00:00:00Z".to_string()),
            rate_multiplier: Some(0.8),
            manual_rate_multiplier: Some(0.7),
            manual_rate_updated_at: Some("2026-07-31T01:00:00Z".to_string()),
            rate_source: Some("manual".to_string()),
            rate_collected_at: Some("2026-07-31T01:00:00Z".to_string()),
            key_updated_at: Some("2026-07-31T01:00:00Z".to_string()),
        });

        let projection =
            route_projection_from_runtime_candidate(&request, candidate).expect("projection");

        let group = projection.group.as_ref().expect("group projection");
        assert_eq!(group.stable_key, "binding:binding-gpt");
        assert_eq!(group.display_name, "GPT Group");
        assert!(projection.policy.group_matches);
        // RuntimeRoutingEconomicSnapshot carries freshness timestamps but no
        // domain revision. Without a real revision, multiplier evidence is
        // intentionally unavailable and must not enter routing economics.
        assert_eq!(projection.multiplier.multiplier, None);
        assert_eq!(projection.multiplier.selected_source, None);
        assert_eq!(
            projection.multiplier.status,
            MultiplierResolutionStatus::Missing
        );
        assert!(!projection.multiplier.ceiling_rejected);
        assert_eq!(projection.pricing.basis, RoutingCostBasis::Unpriced);
        assert_eq!(projection.pricing.comparison_value, None);
        assert_eq!(projection.pricing.unit, None);
        assert!(projection.pricing.source_chain.is_empty());
        assert_eq!(projection.pricing.observed_at, None);
    }

    #[test]
    fn runtime_candidate_projection_does_not_apply_ceiling_without_multiplier_revision() {
        let now_ms = 1_800_000_000_000;
        let settings = RuntimeRoutingSettings {
            policy: RoutingPolicy::CostStableFirst,
            max_rate_multiplier: Some(1.0),
            routing_group_scope: RoutingGroupFilter::AllGroups,
            scheduler_config: Default::default(),
            allow_depleted_fallback: false,
        };
        let request = route_request_facts_for_read_model(&settings, now_ms);
        let candidate = runtime_candidate(RuntimeRoutingEconomicSnapshot {
            group_binding_id: Some("binding-claude".to_string()),
            group_name: Some("Claude Group".to_string()),
            group_status: Some("available".to_string()),
            rate_multiplier: Some(1.25),
            key_updated_at: Some("2026-07-31T01:00:00Z".to_string()),
            ..RuntimeRoutingEconomicSnapshot::default()
        });

        let projection =
            route_projection_from_runtime_candidate(&request, candidate).expect("projection");

        // A timestamp-only economic row cannot establish authoritative
        // multiplier evidence, so the configured ceiling is not evaluated.
        assert_eq!(
            projection.multiplier.status,
            MultiplierResolutionStatus::Missing
        );
        assert_eq!(projection.multiplier.multiplier, None);
        assert!(!projection.multiplier.ceiling_rejected);
        assert!(!projection
            .hard_rejection_codes
            .contains(&"multiplier_ceiling"));
    }

    #[test]
    fn runtime_candidate_projection_uses_live_health_block_window() {
        let now_ms = 1_800_000_000_000;
        let settings = RuntimeRoutingSettings {
            policy: RoutingPolicy::PriorityFallback,
            max_rate_multiplier: None,
            routing_group_scope: RoutingGroupFilter::AllGroups,
            scheduler_config: Default::default(),
            allow_depleted_fallback: false,
        };
        let request = route_request_facts_for_read_model(&settings, now_ms);
        let mut candidate = runtime_candidate(RuntimeRoutingEconomicSnapshot::default());
        candidate.health = Some(crate::models::routing::StationKeyHealth {
            station_key_id: candidate.station_key_id.clone(),
            last_success_at: None,
            last_failure_at: None,
            consecutive_failures: 0,
            success_count: 1,
            failure_count: 1,
            avg_latency_ms: None,
            last_error_summary: None,
            cooldown_until: Some((now_ms - 1).to_string()),
            updated_at: "2026-07-31T00:00:00Z".to_string(),
        });

        let projection =
            route_projection_from_runtime_candidate(&request, candidate).expect("projection");

        assert_ne!(
            projection.health.station_key,
            HealthAdmission::SuppressDurableCooldown
        );
        assert!(!projection
            .hard_rejection_codes
            .contains(&"health_hard_reject"));
    }

    #[test]
    fn runtime_candidate_projection_prefers_exact_request_pricing_over_multiplier_proxy() {
        let now_ms = 1_800_000_000_000;
        let settings = RuntimeRoutingSettings {
            policy: RoutingPolicy::CostStableFirst,
            max_rate_multiplier: Some(2.0),
            routing_group_scope: RoutingGroupFilter::AllGroups,
            scheduler_config: Default::default(),
            allow_depleted_fallback: false,
        };
        let request = RouteRequestClassifier::classify(
            CanonicalRouteRequest {
                route_kind: RouteKind::Inference,
                requested_model: Some("gpt-5-mini".to_string()),
                stream: false,
                uses_tools: false,
                uses_vision: false,
                uses_reasoning: false,
                untrusted_headers: Vec::new(),
            },
            validated_route_settings(&settings),
            now_ms,
        );
        let candidate = runtime_candidate(RuntimeRoutingEconomicSnapshot {
            rate_multiplier: Some(0.8),
            rate_source: Some("collector".to_string()),
            rate_collected_at: Some("2026-07-31T00:00:00Z".to_string()),
            key_updated_at: Some("2026-07-31T00:00:00Z".to_string()),
            ..RuntimeRoutingEconomicSnapshot::default()
        });
        let pricing = ResolvedPricingContext {
            station_key_id: "key-1".to_string(),
            station_id: "station-1".to_string(),
            requested_model: "gpt-5-mini".to_string(),
            resolved_model: "gpt-5-mini".to_string(),
            request_kind: RequestKind::Text,
            group_binding_id: None,
            base_input_price: None,
            base_output_price: None,
            base_fixed_price: Some(0.5),
            currency: "USD".to_string(),
            unit: "per_1m_tokens".to_string(),
            base_price_source: Some("builtin".to_string()),
            effective_rate_multiplier: Some(0.8),
            rate_source: Some("collector".to_string()),
            rate_collected_at: Some("2026-07-31T02:00:00Z".to_string()),
            estimated_input_price: None,
            estimated_output_price: None,
            estimated_fixed_price: Some(0.5),
            pricing_status: PricingStatus::Priced,
            confidence: 0.95,
            source_chain: vec!["pricing_rule".to_string(), "model_base_price".to_string()],
            reason: None,
            resolved_at: "2026-07-31T02:00:00Z".to_string(),
        };

        let projection = route_projection_from_runtime_candidate_with_pricing(
            &request,
            candidate,
            Some(&pricing),
        )
        .expect("projection");

        assert_eq!(projection.pricing.basis, RoutingCostBasis::ExactPrice);
        assert_eq!(projection.pricing.comparison_value, Some(0.5));
        assert_eq!(projection.pricing.estimated_fixed_price, Some(0.5));
        assert_eq!(projection.pricing.currency.as_deref(), Some("USD"));
        assert_eq!(
            projection.pricing.source_chain,
            vec!["pricing_rule".to_string(), "model_base_price".to_string()]
        );
        assert_eq!(
            projection.pricing.observed_at.as_deref(),
            Some("2026-07-31T02:00:00Z")
        );
    }

    fn runtime_candidate(
        economic_snapshot: RuntimeRoutingEconomicSnapshot,
    ) -> CanonicalRoutingCandidate {
        CanonicalRoutingCandidate {
            station_key_id: "key-1".to_string(),
            station_id: "station-1".to_string(),
            station_type: "newapi".to_string(),
            station_account_concurrency_limit: None,
            station_endpoint_revision: 1,
            sanitized_origin: "https://station.example.test".to_string(),
            upstream_api_format: UpstreamApiFormat::CustomOpenAiCompatible,
            routing_order: None,
            priority: 10,
            max_concurrency: 4,
            load_factor: Some(0),
            schedulable: true,
            collector_proxy_mode: "inherit".to_string(),
            collector_proxy_url: None,
            station_name: "Station".to_string(),
            key_name: "Key".to_string(),
            capabilities: StationKeyCapabilities {
                station_key_id: "key-1".to_string(),
                supports_chat_completions: true,
                supports_responses: true,
                supports_embeddings: false,
                supports_stream: true,
                supports_tools: true,
                supports_vision: true,
                supports_reasoning: true,
                model_allowlist: Vec::new(),
                model_blocklist: Vec::new(),
                only_use_as_backup: false,
                preferred_models: Vec::new(),
                routing_tags: Vec::new(),
                updated_at: "2026-07-31T00:00:00Z".to_string(),
            },
            health: None,
            balance_snapshot: None,
            economic_snapshot: Some(economic_snapshot),
            api_key: Some("sk-test".to_string()),
            api_key_secret: None,
        }
    }
}
