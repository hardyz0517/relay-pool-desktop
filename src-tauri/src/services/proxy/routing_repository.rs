use std::collections::BTreeMap;

use crate::{
    application::{
        credentials::SecretRef,
        operational_facts::{
            balance_projector::{BalanceProjection, BalanceProjectionStatus},
            capability_projector::{
                CapabilityDecision, CapabilityFeature, CapabilityProjection, CapabilityProtocol,
                CapabilitySubject,
            },
            candidate_projector::{
                project_route_candidate, CandidateIdentityProjection,
                CandidateOperationalProjections, CapabilityProjectionSet, CapacityProjection,
                CapacityScope, CapacityScopeSnapshot,
                HealthProjectionSet, RouteCandidateProjection,
            },
            group_projector::ProjectionTrace,
            health_projector::{
                EffectiveHealthProjection, HealthAdmission, HealthProjectionTarget,
            },
            multiplier_projector::{MultiplierProjection, MultiplierResolutionStatus},
            pricing_projector::{
                RequestCostComparisonContext, RoutingCostBasis, PricingRouteKind,
            },
            target_resolver::ExecutionTargetRef,
        },
        routing::RoutingService,
        routing_engine::{
            controller::{CandidateAdmissionProfile},
            capacity::ProviderAccountConstraint,
            request::RouteRequestFacts,
        },
    },
    models::operational::{
        CapabilityVerdict, ModelName, PriceConfidence, StationId, StationKeyId, UnixMillis,
    },
    models::{
        pricing::BalanceSnapshot,
        routing::{RuntimeRoutingCandidate, RuntimeRoutingSettings},
    },
    persistence::stores::routing_store::OperationalExecutionTargetRefRow,
};

pub(crate) type RoutingExecutionSettings = RuntimeRoutingSettings;

#[derive(Debug, Clone)]
pub(crate) struct OperationalRouteSnapshot {
    pub(crate) candidates: Vec<RouteCandidateProjection>,
    pub(crate) targets: BTreeMap<String, ExecutionTargetRef>,
    pub(crate) profiles: BTreeMap<String, CandidateAdmissionProfile>,
    pub(crate) snapshot_id: String,
    pub(crate) runtime_overlay_revision: u64,
    pub(crate) durable_generation: u64,
}

#[derive(Clone)]
pub(crate) struct V2RoutingRepository {
    routing: RoutingService,
    data_key: [u8; 32],
}

impl V2RoutingRepository {
    pub(crate) fn new(routing: RoutingService, data_key: [u8; 32]) -> Self {
        Self { routing, data_key }
    }
}

pub(crate) trait RoutingRepository: Send + Sync {
    fn load_model_alias_pairs(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<(String, String)>, String>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn load_execution_settings(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<RoutingExecutionSettings, String>> {
        Box::pin(async { Ok(RoutingExecutionSettings::default()) })
    }

    fn load_balance_snapshots(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<BalanceSnapshot>, String>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn load_operational_route_snapshot(
        &self,
        request: RouteRequestFacts,
    ) -> futures_util::future::BoxFuture<'static, Result<OperationalRouteSnapshot, String>>;
}

pub(crate) trait OperationalExecutionTargetRepository: Send + Sync {
    fn load_execution_target_refs(
        &self,
        station_key_ids: Vec<String>,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<OperationalExecutionTargetRefRow>, String>,
    >;
}

impl RoutingRepository for V2RoutingRepository {
    fn load_model_alias_pairs(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<(String, String)>, String>> {
        let routing = self.routing.clone();
        Box::pin(async move {
            routing
                .list_model_alias_pairs()
                .await
                .map_err(|error| format!("load V2 model aliases failed: {error}"))
        })
    }

    fn load_execution_settings(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<RoutingExecutionSettings, String>> {
        let routing = self.routing.clone();
        Box::pin(async move {
            routing
                .load_execution_settings()
                .await
                .map_err(|error| format!("load V2 routing execution settings failed: {error}"))
        })
    }

    fn load_balance_snapshots(
        &self,
    ) -> futures_util::future::BoxFuture<'static, Result<Vec<BalanceSnapshot>, String>> {
        let routing = self.routing.clone();
        Box::pin(async move {
            routing
                .list_balance_snapshots()
                .await
                .map_err(|error| format!("load V2 balance snapshots failed: {error}"))
        })
    }

    fn load_operational_route_snapshot(
        &self,
        request: RouteRequestFacts,
    ) -> futures_util::future::BoxFuture<'static, Result<OperationalRouteSnapshot, String>> {
        let routing = self.routing.clone();
        Box::pin(async move {
            let candidates = routing
                .load_runtime_candidates()
                .await
                .map_err(|error| format!("load V2 route candidates failed: {error}"))?;
            let station_key_ids = candidates
                .iter()
                .map(|candidate| candidate.station_key_id.clone())
                .collect::<Vec<_>>();
            let target_rows = routing
                .load_operational_execution_target_refs(station_key_ids)
                .await
                .map_err(|error| format!("load operational target refs failed: {error}"))?;
            let targets = target_rows
                .into_iter()
                .map(|row| {
                    let target = execution_target_ref_from_row(row)?;
                    Ok((target.station_key_id.clone(), target))
                })
                .collect::<Result<BTreeMap<_, _>, String>>()?;
            let mut profiles = BTreeMap::new();
            let projected = candidates
                .into_iter()
                .map(|candidate| {
                    let profile = admission_profile_from_candidate(&candidate);
                    profiles.insert(candidate.station_key_id.clone(), profile);
                    route_projection_from_runtime(&request, candidate)
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(OperationalRouteSnapshot {
                candidates: projected,
                targets,
                profiles,
                snapshot_id: format!("runtime-candidates-{}", chrono::Utc::now().timestamp_millis()),
                runtime_overlay_revision: 1,
                durable_generation: 1,
            })
        })
    }
}

impl OperationalExecutionTargetRepository for V2RoutingRepository {
    fn load_execution_target_refs(
        &self,
        station_key_ids: Vec<String>,
    ) -> futures_util::future::BoxFuture<
        'static,
        Result<Vec<OperationalExecutionTargetRefRow>, String>,
    > {
        let routing = self.routing.clone();
        Box::pin(async move {
            routing
                .load_operational_execution_target_refs(station_key_ids)
                .await
                .map_err(|error| format!("load operational execution target refs failed: {error}"))
        })
    }
}

fn execution_target_ref_from_row(
    row: OperationalExecutionTargetRefRow,
) -> Result<ExecutionTargetRef, String> {
    let api_key_secret_ref = match (
        row.api_key_secret_id,
        row.api_key_secret_scope,
        row.api_key_secret_owner_id,
        row.api_key_secret_kind,
    ) {
        (Some(id), Some(scope), Some(owner_id), Some(kind)) => Some(SecretRef {
            id,
            scope,
            owner_id,
            kind,
        }),
        (None, None, None, None) => None,
        _ => return Err("incomplete station key secret ref".to_string()),
    };
    Ok(ExecutionTargetRef {
        station_key_id: row.station_key_id,
        station_id: row.station_id,
        endpoint_revision: row.endpoint_revision,
        api_base_url: row.api_base_url,
        upstream_api_format: row.upstream_api_format,
        collector_proxy_mode: row.collector_proxy_mode,
        collector_proxy_url: row.collector_proxy_url,
        enabled: row.key_enabled && row.station_enabled,
        api_key_secret_ref,
        inline_api_key_present: row.inline_api_key_present,
    })
}

pub(crate) fn admission_profile_from_candidate(
    candidate: &RuntimeRoutingCandidate,
) -> CandidateAdmissionProfile {
    CandidateAdmissionProfile {
        endpoint_revision: candidate.station_endpoint_revision,
        expected_credential_revision: 1,
        credential_revision: 1,
        durable_generation: 1,
        global_max_concurrency: 1024,
        station_account_max_concurrency: 1024,
        station_key_max_concurrency: positive_u32(candidate.max_concurrency).unwrap_or(1),
        provider_account_constraint: ProviderAccountConstraint::NotApplicable,
        half_open_probe_id: None,
    }
}

pub(crate) fn route_projection_from_runtime(
    request: &RouteRequestFacts,
    candidate: RuntimeRoutingCandidate,
) -> Result<RouteCandidateProjection, String> {
    let now = UnixMillis::new(request.admitted_at_ms())
        .or_else(|_| UnixMillis::new(0))
        .map_err(|error| error.to_string())?;
    let identity = CandidateIdentityProjection {
        station_key_id: candidate.station_key_id.clone(),
        station_id: candidate.station_id.clone(),
        endpoint_revision: candidate.station_endpoint_revision,
        sanitized_origin: sanitized_origin(&candidate.upstream_base_url),
        credential_available: candidate.api_key_secret.is_some()
            || candidate
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
    };
    let operational = CandidateOperationalProjections {
        identity,
        priority: candidate.routing_order.unwrap_or(candidate.priority),
        resolved_model: request.requested_model().map(ToString::to_string),
        group: None,
        multiplier: missing_multiplier(now),
        pricing: unpriced_context(request),
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

fn sanitized_origin(api_base_url: &str) -> String {
    crate::models::station_endpoints::sanitized_api_base_url_for_trace(api_base_url)
}

fn missing_multiplier(now: UnixMillis) -> MultiplierProjection {
    MultiplierProjection {
        multiplier: None,
        status: MultiplierResolutionStatus::Missing,
        selected_kind: None,
        trace: ProjectionTrace::new(
            vec!["runtime_candidate_projection"],
            PriceConfidence::new(0.0).expect("valid confidence"),
            now,
            "multiplier_missing",
            Vec::new(),
        ),
    }
}

fn unpriced_context(request: &RouteRequestFacts) -> RequestCostComparisonContext {
    let route_kind = match request.route_kind() {
        crate::application::routing_engine::request::RouteKind::Inference => PricingRouteKind::Inference,
        crate::application::routing_engine::request::RouteKind::ModelCatalog => {
            PricingRouteKind::ModelCatalog
        }
    };
    if route_kind == PricingRouteKind::ModelCatalog {
        return RequestCostComparisonContext {
            route_kind,
            basis: RoutingCostBasis::NotApplicable,
            comparison_value: None,
            reason: Some("model_catalog_has_no_request_cost"),
            currency: None,
            unit: None,
            source_chain: Vec::new(),
            observed_at: None,
            confidence: None,
        };
    }
    RequestCostComparisonContext {
        route_kind,
        basis: RoutingCostBasis::Unpriced,
        comparison_value: None,
        reason: Some("pricing_context_missing"),
        currency: None,
        unit: None,
        source_chain: Vec::new(),
        observed_at: None,
        confidence: None,
    }
}

fn balance_projection(
    balance: Option<&crate::models::routing::RuntimeRoutingBalance>,
    now: UnixMillis,
) -> BalanceProjection {
    let (status, reason) = match balance.map(|balance| balance.status.as_str()) {
        Some("depleted") | Some("low") => (BalanceProjectionStatus::DepletedEmergency, "balance_depleted"),
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
    candidate: &RuntimeRoutingCandidate,
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
        crate::application::routing_engine::request::RouteKind::ModelCatalog => {
            CapabilitySubject::Protocol(CapabilityProtocol::ChatCompletions)
        }
        crate::application::routing_engine::request::RouteKind::Inference => {
            CapabilitySubject::Protocol(CapabilityProtocol::ChatCompletions)
        }
    }
}

fn protocol_supported(request: &RouteRequestFacts, candidate: &RuntimeRoutingCandidate) -> bool {
    if matches!(
        request.route_kind(),
        crate::application::routing_engine::request::RouteKind::ModelCatalog
    ) {
        return true;
    }
    candidate.capabilities.supports_chat_completions
        || candidate.capabilities.supports_responses
        || candidate.capabilities.supports_embeddings
}

fn model_supported(model: Option<&str>, candidate: &RuntimeRoutingCandidate) -> bool {
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
        winner: None,
        overridden: Vec::new(),
        conflict_reason: None,
    }
}

fn health_projection_set(
    request: &RouteRequestFacts,
    candidate: &RuntimeRoutingCandidate,
    now: UnixMillis,
) -> Result<HealthProjectionSet, String> {
    let station_id = StationId::new(candidate.station_id.clone()).map_err(|error| error.to_string())?;
    let station_key_id =
        StationKeyId::new(candidate.station_key_id.clone()).map_err(|error| error.to_string())?;
    let key_admission = if !candidate.schedulable {
        HealthAdmission::HardReject
    } else if candidate.health.as_ref().and_then(|health| health.cooldown_until.as_ref()).is_some()
    {
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
            now,
        ),
        station_account: effective_health(
            HealthProjectionTarget::StationAccount(station_id.clone()),
            HealthAdmission::Admit,
            now,
        ),
        endpoint: effective_health(
            HealthProjectionTarget::Endpoint(crate::models::operational::EndpointRef::new(
                station_id,
                crate::models::operational::EndpointId::new(format!(
                    "{}:endpoint",
                    candidate.station_id
                ))
                .map_err(|error| error.to_string())?,
                crate::models::operational::EndpointRevision::new(candidate.station_endpoint_revision)
                    .map_err(|error| error.to_string())?,
            )),
            HealthAdmission::Admit,
            now,
        ),
        model: effective_health(
            HealthProjectionTarget::Model {
                station_key_id,
                model: ModelName::new(request.requested_model().unwrap_or("*"))
                    .map_err(|error| error.to_string())?,
            },
            HealthAdmission::Admit,
            now,
        ),
    })
}

fn effective_health(
    target: HealthProjectionTarget,
    admission: HealthAdmission,
    _now: UnixMillis,
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

fn capacity_projection(candidate: &RuntimeRoutingCandidate) -> CapacityProjection {
    let limit = positive_u32(candidate.max_concurrency);
    CapacityProjection {
        scopes: vec![CapacityScopeSnapshot {
            scope: CapacityScope::StationKey,
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

fn positive_u32(value: i64) -> Option<u32> {
    u32::try_from(value).ok().filter(|value| *value > 0)
}
