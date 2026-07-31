use crate::{
    models::monitoring::{
        ClientProfileRef, DefinitionRevision, FailureKind, HealthPolicy, ProtocolKind, RetryPolicy,
        RiskPolicy, SchedulePolicy, TargetScope, TriggerKind,
    },
    services::monitoring::{
        adapters::protocol_auto::{resolve_protocol_auto, ProtocolCapabilityFacts},
        profiles::registry::BuiltinProfileRegistry,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProtocolSelection {
    Explicit(ProtocolKind),
    Auto,
}

#[derive(Debug, Clone)]
pub(crate) struct MonitorPlanningSnapshot {
    pub(crate) id: String,
    pub(crate) revision: DefinitionRevision,
    pub(crate) target_scope: TargetScope,
    pub(crate) protocol_selection: ProtocolSelection,
    pub(crate) client_profile: ClientProfileRef,
    pub(crate) primary_model: String,
    pub(crate) fallback_models: Vec<String>,
    pub(crate) schedule_policy: SchedulePolicy,
    pub(crate) retry_policy: RetryPolicy,
    pub(crate) risk_policy: RiskPolicy,
    pub(crate) health_policy: HealthPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TargetCapabilitySnapshot {
    pub(crate) station_id: String,
    pub(crate) station_key_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) provider_protocol: Option<ProtocolKind>,
    pub(crate) endpoint_protocol: Option<ProtocolKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbePlan {
    pub(crate) monitor_id: String,
    pub(crate) revision: DefinitionRevision,
    pub(crate) trigger_kind: TriggerKind,
    pub(crate) config_snapshot_hash: String,
    pub(crate) target_plans: Vec<ProbeTargetPlan>,
    pub(crate) model_plans: Vec<ProbeModelPlan>,
    pub(crate) schedule_policy: SchedulePolicy,
    pub(crate) retry_policy: RetryPolicy,
    pub(crate) health_policy: HealthPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeTargetPlan {
    pub(crate) station_id: String,
    pub(crate) station_key_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) protocol_kind: Option<ProtocolKind>,
    pub(crate) skip_failure_kind: Option<FailureKind>,
    pub(crate) client_profile: ClientProfileRef,
    pub(crate) request_profile_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeModelPlan {
    pub(crate) model: String,
    pub(crate) role: ProbeModelRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeModelRole {
    Primary,
    Fallback { index: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlanError {
    EmptyMonitorId,
    EmptyModel,
    NoTargets,
    TooManyTargets,
    ProfileRejected(String),
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ProbePlanner;

impl ProbePlanner {
    pub(crate) fn build_plan(
        &self,
        snapshot: MonitorPlanningSnapshot,
        targets: &[TargetCapabilitySnapshot],
        trigger_kind: TriggerKind,
    ) -> Result<ProbePlan, PlanError> {
        if snapshot.id.trim().is_empty() {
            return Err(PlanError::EmptyMonitorId);
        }
        if snapshot.primary_model.trim().is_empty() {
            return Err(PlanError::EmptyModel);
        }
        if targets.is_empty() {
            return Err(PlanError::NoTargets);
        }
        if targets.len() > 500 {
            return Err(PlanError::TooManyTargets);
        }

        let registry = BuiltinProfileRegistry::default();
        let mut target_plans = Vec::with_capacity(targets.len());
        let mut plan_hash_parts = vec![
            snapshot.id.clone(),
            snapshot.revision.0.to_string(),
            format!("{:?}", snapshot.target_scope),
            format!("{:?}", snapshot.protocol_selection),
            format!("{:?}", snapshot.client_profile.id),
            snapshot.client_profile.version.to_string(),
            snapshot.primary_model.clone(),
            snapshot.fallback_models.join(","),
            snapshot.risk_policy.max_daily_probe_attempts.to_string(),
            format!("{:?}", snapshot.health_policy.writeback_mode),
            snapshot.health_policy.failure_threshold.to_string(),
            snapshot.health_policy.recovery_threshold.to_string(),
        ];

        for target in targets {
            let resolution = match snapshot.protocol_selection {
                ProtocolSelection::Explicit(protocol_kind) => Some(protocol_kind),
                ProtocolSelection::Auto => {
                    let resolution = resolve_protocol_auto(ProtocolCapabilityFacts {
                        provider_protocol: target.provider_protocol,
                        endpoint_protocol: target.endpoint_protocol,
                    });
                    debug_assert_eq!(resolution.network_call_count, 0);
                    resolution.protocol_kind
                }
            };

            let (request_profile_hash, skip_failure_kind) = match resolution {
                Some(protocol_kind) => {
                    registry
                        .validate_execution_profile(
                            snapshot.client_profile.id,
                            snapshot.client_profile.version,
                            protocol_kind,
                        )
                        .map_err(PlanError::ProfileRejected)?;
                    let profile = registry
                        .get(snapshot.client_profile.id)
                        .ok_or_else(|| PlanError::ProfileRejected("profile missing".into()))?;
                    (Some(profile.profile_hash()), None)
                }
                None => (None, Some(FailureKind::NeedsConfiguration)),
            };

            plan_hash_parts.push(format!(
                "{}:{}:{:?}:{:?}",
                target.station_id, target.station_key_id, target.endpoint_revision, resolution
            ));
            target_plans.push(ProbeTargetPlan {
                station_id: target.station_id.clone(),
                station_key_id: target.station_key_id.clone(),
                endpoint_revision: target.endpoint_revision,
                protocol_kind: resolution,
                skip_failure_kind,
                client_profile: snapshot.client_profile.clone(),
                request_profile_hash,
            });
        }

        let mut model_plans = vec![ProbeModelPlan {
            model: snapshot.primary_model.trim().to_string(),
            role: ProbeModelRole::Primary,
        }];
        for (index, model) in snapshot.fallback_models.iter().enumerate() {
            let model = model.trim();
            if !model.is_empty() && model != snapshot.primary_model {
                model_plans.push(ProbeModelPlan {
                    model: model.to_string(),
                    role: ProbeModelRole::Fallback { index: index as u8 },
                });
            }
        }

        Ok(ProbePlan {
            monitor_id: snapshot.id,
            revision: snapshot.revision,
            trigger_kind,
            config_snapshot_hash: stable_plan_hash(&plan_hash_parts),
            target_plans,
            model_plans,
            schedule_policy: snapshot.schedule_policy,
            retry_policy: snapshot.retry_policy,
            health_policy: snapshot.health_policy,
        })
    }
}

fn stable_plan_hash(parts: &[String]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}
