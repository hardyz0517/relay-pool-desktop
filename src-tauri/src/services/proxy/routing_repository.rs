use std::collections::BTreeMap;

use crate::{
    application::{
        credentials::SecretRef,
        operational_facts::{
            candidate_projector::RouteCandidateProjection,
            runtime_candidate_adapter::{
                admission_profile_from_runtime_candidate, route_projection_from_runtime_candidate,
            },
            target_resolver::ExecutionTargetRef,
        },
        routing::RoutingService,
        routing_engine::{controller::CandidateAdmissionProfile, request::RouteRequestFacts},
    },
    models::{pricing::BalanceSnapshot, routing::RuntimeRoutingSettings},
    persistence::stores::routing_store::OperationalExecutionTargetRefRow,
};

pub(crate) type RoutingExecutionSettings = RuntimeRoutingSettings;

#[cfg(test)]
pub(crate) use crate::application::operational_facts::runtime_candidate_adapter::{
    admission_profile_from_runtime_candidate as admission_profile_from_candidate,
    route_projection_from_runtime_candidate as route_projection_from_runtime,
};

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
                    let profile = admission_profile_from_runtime_candidate(&candidate);
                    profiles.insert(candidate.station_key_id.clone(), profile);
                    route_projection_from_runtime_candidate(&request, candidate)
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(OperationalRouteSnapshot {
                candidates: projected,
                targets,
                profiles,
                snapshot_id: format!(
                    "runtime-candidates-{}",
                    chrono::Utc::now().timestamp_millis()
                ),
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
