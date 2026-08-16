use std::{
    collections::BTreeMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures_util::future::BoxFuture;
use tokio_util::sync::CancellationToken;

use crate::{
    application::{
        monitoring::{
            orchestrator::{ProbeTransport, ProbeTransportRequest, ProbeTransportResult},
            planner::ProbePlan,
        },
        queries::routing_runtime::RoutingMonitoringTargetSnapshot,
    },
    models::monitoring::{ClientProfileId, FailureKind, ProtocolKind},
    outbound::{AsyncOutboundClient, ProxyPolicy},
    services::monitoring::{
        challenge::ProbeChallenge,
        executor::{ProbeExecutionInput, ProbeExecutor, ProbeSecretResolver, ResolvedProbeSecret},
        transport::{MonitoringTransport, MonitoringTransportConfig},
    },
};

#[cfg(test)]
use crate::outbound::TimeoutPolicy;

#[derive(Debug, Clone)]
pub(crate) struct ProbeTargetEndpoint {
    pub(crate) station_key_id: String,
    pub(crate) base_url: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) secret: String,
    pub(crate) protocol_kind: ProtocolKind,
    pub(crate) client_profile_id: ClientProfileId,
    pub(crate) client_profile_version: u32,
}

pub(crate) struct ProbeExecutorTransport {
    client: AsyncOutboundClient,
    cancellation_token: CancellationToken,
    targets: BTreeMap<String, ProbeTargetEndpoint>,
}

impl ProbeExecutorTransport {
    pub(crate) fn new(
        client: AsyncOutboundClient,
        cancellation_token: CancellationToken,
        endpoints: impl IntoIterator<Item = ProbeTargetEndpoint>,
    ) -> Self {
        Self {
            client,
            cancellation_token,
            targets: endpoints
                .into_iter()
                .map(|endpoint| (endpoint.station_key_id.clone(), endpoint))
                .collect(),
        }
    }

    pub(crate) fn endpoints_from_plan(
        plan: &ProbePlan,
        candidates: &[RoutingMonitoringTargetSnapshot],
        secrets: &BTreeMap<String, String>,
    ) -> Vec<ProbeTargetEndpoint> {
        plan.target_plans
            .iter()
            .filter_map(|target| {
                let protocol_kind = target.protocol_kind?;
                let candidate = candidates
                    .iter()
                    .find(|candidate| candidate.station_key_id == target.station_key_id)?;
                let secret = secrets.get(&target.station_key_id)?;
                Some(ProbeTargetEndpoint {
                    station_key_id: target.station_key_id.clone(),
                    base_url: candidate.api_base_url.clone(),
                    endpoint_revision: target.endpoint_revision,
                    secret: secret.clone(),
                    protocol_kind,
                    client_profile_id: target.client_profile.id,
                    client_profile_version: target.client_profile.version,
                })
            })
            .collect()
    }
}

impl ProbeTransport for ProbeExecutorTransport {
    fn send(&mut self, request: ProbeTransportRequest) -> BoxFuture<'_, ProbeTransportResult> {
        let Some(endpoint) = self.targets.get(&request.station_key_id).cloned() else {
            return Box::pin(async {
                ProbeTransportResult::failure(FailureKind::NeedsConfiguration, false, None, 0)
            });
        };
        let client = self.client.clone();
        let cancellation_token = self.cancellation_token.clone();
        Box::pin(async move {
            let now_ms = now_ms();
            let remaining_ms = request.deadline_at_ms.saturating_sub(now_ms).max(1) as u64;
            let challenge = ProbeChallenge::generate_arithmetic();
            let snapshot = challenge.snapshot();
            let endpoint_revision = endpoint.endpoint_revision;
            let protocol_kind = endpoint.protocol_kind;
            let client_profile_id = endpoint.client_profile_id;
            let client_profile_version = endpoint.client_profile_version;
            let transport = MonitoringTransport::from_client(
                client,
                MonitoringTransportConfig {
                    base_url: endpoint.base_url.clone(),
                    proxy: ProxyPolicy::Direct,
                    #[cfg(test)]
                    timeouts: TimeoutPolicy {
                        connect_timeout: Duration::from_secs(10),
                        first_byte_timeout: Duration::from_secs(30),
                        body_read_timeout: Duration::from_secs(30),
                        total_timeout: Duration::from_millis(remaining_ms),
                    },
                    #[cfg(test)]
                    success_body_max_bytes: 2 * 1024 * 1024,
                    #[cfg(test)]
                    error_body_max_bytes: 8 * 1024,
                    #[cfg(test)]
                    redirect_max_hops: 2,
                },
            );
            let executor = ProbeExecutor::new(transport, StaticProbeSecretResolver { endpoint });
            let output = executor
                .execute(
                    ProbeExecutionInput {
                        station_key_id: request.station_key_id,
                        endpoint_revision,
                        protocol_kind,
                        client_profile_id,
                        client_profile_version,
                        model: request.model,
                        prompt: snapshot.prompt,
                        validator: challenge.validator(),
                        deadline_at: Instant::now() + Duration::from_millis(remaining_ms),
                        stream: true,
                    },
                    cancellation_token,
                )
                .await;
            ProbeTransportResult {
                outcome: output.outcome,
                failure_kind: output.failure_kind,
                retryable: output.retryable,
                retry_after_ms: None,
                latency_ms: output.latency_ms,
                semantic_confidence: output.semantic_confidence,
                error_summary: output.error_summary,
            }
        })
    }
}

struct StaticProbeSecretResolver {
    endpoint: ProbeTargetEndpoint,
}

impl ProbeSecretResolver for StaticProbeSecretResolver {
    fn resolve_station_key_secret(
        &self,
        station_key_id: &str,
    ) -> Result<ResolvedProbeSecret, FailureKind> {
        if station_key_id != self.endpoint.station_key_id {
            return Err(FailureKind::NeedsConfiguration);
        }
        Ok(ResolvedProbeSecret {
            value: self.endpoint.secret.clone(),
            endpoint_revision: self.endpoint.endpoint_revision,
        })
    }

    fn current_endpoint_revision(&self, station_key_id: &str) -> Result<i64, FailureKind> {
        if station_key_id != self.endpoint.station_key_id {
            return Err(FailureKind::NeedsConfiguration);
        }
        Ok(self.endpoint.endpoint_revision)
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
