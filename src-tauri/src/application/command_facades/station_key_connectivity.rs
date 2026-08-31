use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use zeroize::Zeroizing;

use crate::{
    application::{
        collectors::CollectorService,
        connectivity_probe::{
            StationKeyConnectivityClientProfile, StationKeyConnectivityProbeKind,
            StationKeyConnectivityResponseMode,
        },
        credentials::CredentialService,
        error::ApplicationError,
        routing::RoutingService,
        settings::SettingsService,
    },
    background_tasks::OperationId,
    models::{routing::StationKeyCapabilities, station_keys::KeyPoolItem},
    outbound::ProxyPolicy,
};

const CONNECTIVITY_RESULT_TTL: Duration = Duration::from_secs(30 * 60);
const CONNECTIVITY_RESULT_CAPACITY: usize = 64;

#[derive(Debug)]
pub(crate) enum StationKeyConnectivityCommandError {
    Application(ApplicationError),
    Message(String),
}

impl From<ApplicationError> for StationKeyConnectivityCommandError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

pub(crate) struct StationKeyConnectivityProbeTarget {
    pub(crate) key: KeyPoolItem,
    pub(crate) api_key: Zeroizing<String>,
    pub(crate) capabilities: StationKeyCapabilities,
    pub(crate) proxy: ProxyPolicy,
}

#[derive(Clone, Debug)]
pub(crate) struct StationKeyConnectivityResult {
    pub(crate) station_key_id: String,
    pub(crate) ok: bool,
    pub(crate) status_code: u16,
    pub(crate) duration_ms: i64,
    pub(crate) model: String,
    pub(crate) message: String,
    pub(crate) validated_protocol: StationKeyConnectivityProbeKind,
    pub(crate) client_profile: StationKeyConnectivityClientProfile,
    pub(crate) response_mode: StationKeyConnectivityResponseMode,
    pub(crate) stream_fallback_reason: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct StationKeyModelDiscoveryResult {
    pub(crate) station_key_id: String,
    pub(crate) models: Vec<String>,
}

#[derive(Clone)]
struct StationKeyConnectivityResultStore {
    inner: Arc<Mutex<StationKeyConnectivityResultStoreInner>>,
    ttl: Duration,
    capacity: usize,
}

#[derive(Clone)]
struct StationKeyModelDiscoveryResultStore {
    inner: Arc<Mutex<StationKeyModelDiscoveryResultStoreInner>>,
    ttl: Duration,
    capacity: usize,
}

#[derive(Default)]
struct StationKeyConnectivityResultStoreInner {
    entries: BTreeMap<OperationId, StationKeyConnectivityResultEntry>,
    insertion_order: VecDeque<OperationId>,
}

#[derive(Default)]
struct StationKeyModelDiscoveryResultStoreInner {
    entries: BTreeMap<OperationId, StationKeyModelDiscoveryResultEntry>,
    insertion_order: VecDeque<OperationId>,
}

struct StationKeyConnectivityResultEntry {
    result: StationKeyConnectivityResult,
    recorded_at: Instant,
}

struct StationKeyModelDiscoveryResultEntry {
    result: StationKeyModelDiscoveryResult,
    recorded_at: Instant,
}

impl Default for StationKeyConnectivityResultStore {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(StationKeyConnectivityResultStoreInner::default())),
            ttl: CONNECTIVITY_RESULT_TTL,
            capacity: CONNECTIVITY_RESULT_CAPACITY,
        }
    }
}

impl Default for StationKeyModelDiscoveryResultStore {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(
                StationKeyModelDiscoveryResultStoreInner::default(),
            )),
            ttl: CONNECTIVITY_RESULT_TTL,
            capacity: CONNECTIVITY_RESULT_CAPACITY,
        }
    }
}

impl StationKeyConnectivityResultStore {
    fn insert(&self, operation_id: OperationId, result: StationKeyConnectivityResult) {
        let now = Instant::now();
        let mut inner = self.inner.lock().expect("connectivity result store mutex");
        self.prune_locked(&mut inner, now);
        if inner.entries.contains_key(&operation_id) {
            inner
                .insertion_order
                .retain(|stored_id| *stored_id != operation_id);
        }
        inner.entries.insert(
            operation_id,
            StationKeyConnectivityResultEntry {
                result,
                recorded_at: now,
            },
        );
        inner.insertion_order.push_back(operation_id);
        while inner.entries.len() > self.capacity {
            if let Some(oldest_id) = inner.insertion_order.pop_front() {
                inner.entries.remove(&oldest_id);
            }
        }
    }

    fn get(&self, operation_id: OperationId) -> Option<StationKeyConnectivityResult> {
        let mut inner = self.inner.lock().expect("connectivity result store mutex");
        self.prune_locked(&mut inner, Instant::now());
        inner
            .entries
            .get(&operation_id)
            .map(|entry| entry.result.clone())
    }

    fn prune_locked(&self, inner: &mut StationKeyConnectivityResultStoreInner, now: Instant) {
        while let Some(operation_id) = inner.insertion_order.front().copied() {
            let expired = inner
                .entries
                .get(&operation_id)
                .is_none_or(|entry| now.duration_since(entry.recorded_at) >= self.ttl);
            if !expired {
                break;
            }
            inner.insertion_order.pop_front();
            inner.entries.remove(&operation_id);
        }
    }
}

impl StationKeyModelDiscoveryResultStore {
    fn insert(&self, operation_id: OperationId, result: StationKeyModelDiscoveryResult) {
        let now = Instant::now();
        let mut inner = self
            .inner
            .lock()
            .expect("model discovery result store mutex");
        self.prune_locked(&mut inner, now);
        if inner.entries.contains_key(&operation_id) {
            inner
                .insertion_order
                .retain(|stored_id| *stored_id != operation_id);
        }
        inner.entries.insert(
            operation_id,
            StationKeyModelDiscoveryResultEntry {
                result,
                recorded_at: now,
            },
        );
        inner.insertion_order.push_back(operation_id);
        while inner.entries.len() > self.capacity {
            if let Some(oldest_id) = inner.insertion_order.pop_front() {
                inner.entries.remove(&oldest_id);
            }
        }
    }

    fn get(&self, operation_id: OperationId) -> Option<StationKeyModelDiscoveryResult> {
        let mut inner = self
            .inner
            .lock()
            .expect("model discovery result store mutex");
        self.prune_locked(&mut inner, Instant::now());
        inner
            .entries
            .get(&operation_id)
            .map(|entry| entry.result.clone())
    }

    fn prune_locked(&self, inner: &mut StationKeyModelDiscoveryResultStoreInner, now: Instant) {
        while let Some(operation_id) = inner.insertion_order.front().copied() {
            let expired = inner
                .entries
                .get(&operation_id)
                .is_none_or(|entry| now.duration_since(entry.recorded_at) >= self.ttl);
            if !expired {
                break;
            }
            inner.insertion_order.pop_front();
            inner.entries.remove(&operation_id);
        }
    }
}

#[derive(Clone)]
pub(crate) struct StationKeyConnectivityCommandFacade {
    collectors: Arc<CollectorService>,
    credentials: Arc<CredentialService>,
    routing: Arc<RoutingService>,
    settings: Arc<SettingsService>,
    results: StationKeyConnectivityResultStore,
    model_discovery_results: StationKeyModelDiscoveryResultStore,
}

impl StationKeyConnectivityCommandFacade {
    pub(crate) fn new(
        collectors: Arc<CollectorService>,
        credentials: Arc<CredentialService>,
        routing: Arc<RoutingService>,
        settings: Arc<SettingsService>,
    ) -> Self {
        Self {
            collectors,
            credentials,
            routing,
            settings,
            results: StationKeyConnectivityResultStore::default(),
            model_discovery_results: StationKeyModelDiscoveryResultStore::default(),
        }
    }

    pub(crate) fn store_result(
        &self,
        operation_id: OperationId,
        result: StationKeyConnectivityResult,
    ) {
        self.results.insert(operation_id, result);
    }

    pub(crate) fn get_result(
        &self,
        operation_id: OperationId,
    ) -> Option<StationKeyConnectivityResult> {
        self.results.get(operation_id)
    }

    pub(crate) fn store_model_discovery_result(
        &self,
        operation_id: OperationId,
        result: StationKeyModelDiscoveryResult,
    ) {
        self.model_discovery_results.insert(operation_id, result);
    }

    pub(crate) fn get_model_discovery_result(
        &self,
        operation_id: OperationId,
    ) -> Option<StationKeyModelDiscoveryResult> {
        self.model_discovery_results.get(operation_id)
    }

    pub(crate) async fn prepare_probe_target(
        &self,
        station_key_id: String,
    ) -> Result<StationKeyConnectivityProbeTarget, StationKeyConnectivityCommandError> {
        let key = self
            .credentials
            .list_key_pool_items()
            .await?
            .into_iter()
            .find(|item| item.id == station_key_id)
            .ok_or_else(|| {
                StationKeyConnectivityCommandError::Message(
                    "Station Key does not exist".to_string(),
                )
            })?;
        if !key.api_key_present {
            return Err(StationKeyConnectivityCommandError::Message(
                "Station Key does not have a saved API key".to_string(),
            ));
        }
        let (station, settings) = tokio::try_join!(
            self.collectors.station_for_collection(&key.station_id),
            self.settings.load(),
        )?;
        let proxy_config = crate::services::outbound::resolve_proxy_config(
            &station.collector_proxy_mode,
            station.collector_proxy_url,
            &settings.collector_proxy_mode,
            settings.collector_proxy_url,
        );
        let proxy = crate::services::outbound::proxy_policy_from_mode(
            &proxy_config.mode,
            proxy_config.url.as_deref(),
        )
        .map_err(|_| {
            StationKeyConnectivityCommandError::Message(
                "Station proxy configuration is invalid".to_string(),
            )
        })?;
        let secret = self
            .credentials
            .resolve_station_key_secret(station_key_id.clone())
            .await?;
        let api_key = String::from_utf8(secret.as_bytes().to_vec())
            .map(Zeroizing::new)
            .map_err(|_| {
                StationKeyConnectivityCommandError::Message(
                    "Station Key API key is not valid UTF-8".to_string(),
                )
            })?;
        let capabilities = self
            .credentials
            .get_station_key_capabilities(station_key_id)
            .await?;
        Ok(StationKeyConnectivityProbeTarget {
            key,
            api_key,
            capabilities,
            proxy,
        })
    }

    pub(crate) fn record_station_endpoint_health(
        &self,
        station_id: String,
        endpoint_revision: i64,
        status: String,
        latency_ms: Option<i64>,
        checked_at: String,
        error_summary: Option<String>,
    ) -> impl std::future::Future<Output = Result<(), ApplicationError>> + Send + '_ {
        async move {
            self.routing
                .record_station_endpoint_health(
                    station_id,
                    endpoint_revision,
                    status,
                    latency_ms,
                    checked_at,
                    error_summary,
                )
                .await
                .map(|_| ())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(station_key_id: &str) -> StationKeyConnectivityResult {
        StationKeyConnectivityResult {
            station_key_id: station_key_id.to_string(),
            ok: true,
            status_code: 200,
            duration_ms: 42,
            model: "gpt-test".to_string(),
            message: "ok".to_string(),
            validated_protocol: StationKeyConnectivityProbeKind::Responses,
            client_profile: StationKeyConnectivityClientProfile::StandardApi,
            response_mode: StationKeyConnectivityResponseMode::Stream,
            stream_fallback_reason: None,
        }
    }

    #[test]
    fn result_store_evicts_the_oldest_entry_at_capacity() {
        let store = StationKeyConnectivityResultStore {
            inner: Arc::new(Mutex::new(StationKeyConnectivityResultStoreInner::default())),
            ttl: Duration::from_secs(60),
            capacity: 2,
        };
        let first = OperationId::from_u64(1).unwrap();
        let second = OperationId::from_u64(2).unwrap();
        let third = OperationId::from_u64(3).unwrap();

        store.insert(first, result("key-1"));
        store.insert(second, result("key-2"));
        store.insert(third, result("key-3"));

        assert!(store.get(first).is_none());
        assert_eq!(store.get(second).unwrap().station_key_id, "key-2");
        assert_eq!(store.get(third).unwrap().station_key_id, "key-3");
    }

    #[test]
    fn result_store_drops_expired_entries_on_read() {
        let operation_id = OperationId::from_u64(1).unwrap();
        let mut inner = StationKeyConnectivityResultStoreInner::default();
        inner.entries.insert(
            operation_id,
            StationKeyConnectivityResultEntry {
                result: result("key-1"),
                recorded_at: Instant::now() - Duration::from_secs(2),
            },
        );
        inner.insertion_order.push_back(operation_id);
        let store = StationKeyConnectivityResultStore {
            inner: Arc::new(Mutex::new(inner)),
            ttl: Duration::from_secs(1),
            capacity: 2,
        };

        assert!(store.get(operation_id).is_none());
    }
}
