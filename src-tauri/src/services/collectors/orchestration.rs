use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::Arc,
};

use futures_util::{
    future::{BoxFuture, Shared},
    FutureExt,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::outbound::RequestBudget;
use crate::services::collectors::{
    contract::{
        AuthRefreshKey, AuthRefreshOutcome, AuthorizationDriver, CollectorDriver, ProviderEntry,
        ProviderKind, RemoteKeyDriver,
    },
    failure::DriverFailure,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCapability {
    Collector,
    RemoteKey,
    Authorization,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderRegistryError {
    #[error("duplicate provider registration: {0}")]
    DuplicateProvider(ProviderKind),
    #[error("missing provider registration: {0}")]
    MissingProvider(ProviderKind),
    #[error("provider descriptor/capability mismatch for {kind}: {capability:?}")]
    CapabilityMismatch {
        kind: ProviderKind,
        capability: ProviderCapability,
    },
}

pub struct ProviderRegistry {
    entries: HashMap<ProviderKind, ProviderEntry>,
}

impl ProviderRegistry {
    pub fn new(
        entries: Vec<ProviderEntry>,
        required_kinds: &[ProviderKind],
    ) -> Result<Self, ProviderRegistryError> {
        let mut registered = HashSet::new();
        let mut mapped = HashMap::new();
        for entry in entries {
            validate_entry(&entry)?;
            let kind = entry.descriptor.kind;
            if !registered.insert(kind) {
                return Err(ProviderRegistryError::DuplicateProvider(kind));
            }
            mapped.insert(kind, entry);
        }
        for required in required_kinds {
            if !mapped.contains_key(required) {
                return Err(ProviderRegistryError::MissingProvider(*required));
            }
        }
        Ok(Self { entries: mapped })
    }

    pub fn descriptor(
        &self,
        kind: ProviderKind,
    ) -> Result<&crate::services::collectors::contract::ProviderDescriptor, DriverFailure> {
        self.entries
            .get(&kind)
            .map(|entry| &entry.descriptor)
            .ok_or_else(|| DriverFailure::unsupported(format!("provider {kind} is not registered")))
    }

    pub fn collector(&self, kind: ProviderKind) -> Result<&dyn CollectorDriver, DriverFailure> {
        let entry = self.entry(kind)?;
        entry.collector.as_deref().ok_or_else(|| {
            DriverFailure::unsupported(format!("provider {kind} has no collector capability"))
        })
    }

    pub fn remote_key(&self, kind: ProviderKind) -> Result<&dyn RemoteKeyDriver, DriverFailure> {
        let entry = self.entry(kind)?;
        entry.remote_key.as_deref().ok_or_else(|| {
            DriverFailure::unsupported(format!("provider {kind} has no remote-key capability"))
        })
    }

    pub fn authorization(
        &self,
        kind: ProviderKind,
    ) -> Result<&dyn AuthorizationDriver, DriverFailure> {
        let entry = self.entry(kind)?;
        entry.authorization.as_deref().ok_or_else(|| {
            DriverFailure::unsupported(format!("provider {kind} has no authorization capability"))
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether a registered provider owns the station type and
    /// explicitly declares the requested collector task.
    pub fn supports_collector_task_for_station_type(
        &self,
        station_type: &str,
        task: crate::services::collectors::contract::CollectorTaskKind,
    ) -> bool {
        self.entries.values().any(|entry| {
            entry
                .descriptor
                .station_types
                .contains(&station_type.trim())
                && entry
                    .descriptor
                    .capabilities
                    .collector
                    .as_ref()
                    .is_some_and(|capability| capability.supported_tasks.contains(&task))
        })
    }

    fn entry(&self, kind: ProviderKind) -> Result<&ProviderEntry, DriverFailure> {
        self.entries
            .get(&kind)
            .ok_or_else(|| DriverFailure::unsupported(format!("provider {kind} is not registered")))
    }
}

fn validate_entry(entry: &ProviderEntry) -> Result<(), ProviderRegistryError> {
    let kind = entry.descriptor.kind;
    validate_capability(
        kind,
        ProviderCapability::Collector,
        entry.descriptor.capabilities.collector.is_some(),
        entry
            .collector
            .as_ref()
            .map(Arc::as_ref)
            .map(CollectorDriver::kind),
    )?;
    validate_capability(
        kind,
        ProviderCapability::RemoteKey,
        entry.descriptor.capabilities.remote_key.is_some(),
        entry
            .remote_key
            .as_ref()
            .map(Arc::as_ref)
            .map(RemoteKeyDriver::kind),
    )?;
    validate_capability(
        kind,
        ProviderCapability::Authorization,
        entry.descriptor.capabilities.authorization.is_some(),
        entry
            .authorization
            .as_ref()
            .map(Arc::as_ref)
            .map(AuthorizationDriver::kind),
    )
}

fn validate_capability(
    kind: ProviderKind,
    capability: ProviderCapability,
    declared: bool,
    driver_kind: Option<ProviderKind>,
) -> Result<(), ProviderRegistryError> {
    match (declared, driver_kind) {
        (false, None) => Ok(()),
        (true, Some(observed)) if observed == kind => Ok(()),
        _ => Err(ProviderRegistryError::CapabilityMismatch { kind, capability }),
    }
}

type SharedRefresh = Shared<BoxFuture<'static, Result<AuthRefreshOutcome, DriverFailure>>>;

#[derive(Clone, Default)]
pub struct AuthRefreshSingleFlight {
    in_flight: Arc<Mutex<HashMap<AuthRefreshKey, SharedRefresh>>>,
}

impl AuthRefreshSingleFlight {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn refresh<F, Fut>(
        &self,
        key: AuthRefreshKey,
        budget: RequestBudget,
        cancellation: CancellationToken,
        refresh: F,
    ) -> Result<AuthRefreshOutcome, DriverFailure>
    where
        F: FnOnce(AuthRefreshKey, RequestBudget, CancellationToken) -> Fut,
        Fut: Future<Output = Result<AuthRefreshOutcome, DriverFailure>> + Send + 'static,
    {
        let shared = {
            let mut in_flight = self.in_flight.lock().await;
            if let Some(shared) = in_flight.get(&key) {
                shared.clone()
            } else {
                let refresh_key = key.clone();
                let shared = refresh(refresh_key, budget, cancellation).boxed().shared();
                in_flight.insert(key.clone(), shared.clone());
                shared
            }
        };

        let result = shared.await;
        let mut in_flight = self.in_flight.lock().await;
        in_flight.remove(&key);
        result
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };

    use super::*;
    use crate::services::collectors::contract::{
        CredentialScope, DriverCapabilities, ProviderDescriptor,
    };

    fn descriptor(kind: ProviderKind) -> ProviderDescriptor {
        ProviderDescriptor {
            kind,
            display_name: kind.as_str(),
            station_types: &[],
            capabilities: DriverCapabilities::none(),
        }
    }

    #[test]
    fn registry_rejects_duplicate_kind() {
        let error = match ProviderRegistry::new(
            vec![
                ProviderEntry::unsupported(descriptor(ProviderKind::Sub2Api)),
                ProviderEntry::unsupported(descriptor(ProviderKind::Sub2Api)),
            ],
            &[ProviderKind::Sub2Api],
        ) {
            Ok(_) => panic!("duplicate provider kind must fail closed"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ProviderRegistryError::DuplicateProvider(ProviderKind::Sub2Api)
        );
    }

    #[test]
    fn registry_rejects_missing_required_kind() {
        let error = match ProviderRegistry::new(
            vec![ProviderEntry::unsupported(descriptor(
                ProviderKind::Sub2Api,
            ))],
            &[ProviderKind::Sub2Api, ProviderKind::NewApi],
        ) {
            Ok(_) => panic!("missing provider kind must fail closed"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ProviderRegistryError::MissingProvider(ProviderKind::NewApi)
        );
    }

    #[test]
    fn missing_capability_returns_typed_unsupported() {
        let registry = ProviderRegistry::new(
            vec![ProviderEntry::unsupported(descriptor(
                ProviderKind::Sub2Api,
            ))],
            &[ProviderKind::Sub2Api],
        )
        .expect("registry");

        let failure = match registry.collector(ProviderKind::Sub2Api) {
            Ok(_) => panic!("collector capability is intentionally absent in 19.A"),
            Err(failure) => failure,
        };

        assert_eq!(
            failure.kind,
            crate::services::collectors::failure::DriverFailureKind::Unsupported
        );
    }

    fn auth_key(revision: i64) -> AuthRefreshKey {
        AuthRefreshKey {
            provider: ProviderKind::NewApi,
            station_id: "station-1".to_string(),
            endpoint_revision: 7,
            credential_revision: revision,
            scope: CredentialScope::LoginSession,
        }
    }

    #[tokio::test]
    async fn auth_refresh_single_flight_runs_one_side_effect_per_revision() {
        let single_flight = AuthRefreshSingleFlight::new();
        let runs = Arc::new(AtomicUsize::new(0));
        let key = auth_key(11);
        let first_single_flight = single_flight.clone();
        let first_runs = Arc::clone(&runs);
        let second_runs = Arc::clone(&runs);
        let first_key = key.clone();
        let first = tokio::spawn(async move {
            first_single_flight
                .refresh(
                    first_key,
                    RequestBudget::from_now(Duration::from_secs(1)),
                    CancellationToken::new(),
                    move |key, _budget, _cancellation| async move {
                        first_runs.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        Ok(AuthRefreshOutcome {
                            credential_revision: key.credential_revision + 1,
                        })
                    },
                )
                .await
        });
        tokio::time::sleep(Duration::from_millis(1)).await;
        let second = single_flight
            .refresh(
                key,
                RequestBudget::from_now(Duration::from_secs(1)),
                CancellationToken::new(),
                move |_key, _budget, _cancellation| async move {
                    second_runs.fetch_add(1, Ordering::SeqCst);
                    Ok(AuthRefreshOutcome {
                        credential_revision: 99,
                    })
                },
            )
            .await;
        let first = first.await.expect("first task");

        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(first.expect("first refresh").credential_revision, 12);
        assert_eq!(second.expect("second waiter").credential_revision, 12);
    }

    #[tokio::test]
    async fn auth_refresh_single_flight_scopes_by_credential_revision() {
        let single_flight = AuthRefreshSingleFlight::new();
        let runs = Arc::new(AtomicUsize::new(0));
        let first_runs = Arc::clone(&runs);
        let second_runs = Arc::clone(&runs);

        let first = single_flight.refresh(
            auth_key(11),
            RequestBudget::from_now(Duration::from_secs(1)),
            CancellationToken::new(),
            move |key, _budget, _cancellation| async move {
                first_runs.fetch_add(1, Ordering::SeqCst);
                Ok(AuthRefreshOutcome {
                    credential_revision: key.credential_revision + 1,
                })
            },
        );
        let second = single_flight.refresh(
            auth_key(12),
            RequestBudget::from_now(Duration::from_secs(1)),
            CancellationToken::new(),
            move |key, _budget, _cancellation| async move {
                second_runs.fetch_add(1, Ordering::SeqCst);
                Ok(AuthRefreshOutcome {
                    credential_revision: key.credential_revision + 1,
                })
            },
        );

        let (first, second) = tokio::join!(first, second);

        assert_eq!(runs.load(Ordering::SeqCst), 2);
        assert_eq!(first.expect("first refresh").credential_revision, 12);
        assert_eq!(second.expect("second refresh").credential_revision, 13);
    }

    #[tokio::test]
    async fn auth_refresh_waiter_cancellation_does_not_start_second_refresh() {
        let single_flight = AuthRefreshSingleFlight::new();
        let runs = Arc::new(AtomicUsize::new(0));
        let key = auth_key(11);
        let impatient_single_flight = single_flight.clone();
        let impatient_runs = Arc::clone(&runs);
        let patient_runs = Arc::clone(&runs);
        let impatient_key = key.clone();
        let impatient = tokio::spawn(async move {
            tokio::time::timeout(
                Duration::from_millis(5),
                impatient_single_flight.refresh(
                    impatient_key,
                    RequestBudget::from_now(Duration::from_secs(1)),
                    CancellationToken::new(),
                    move |key, _budget, _cancellation| async move {
                        impatient_runs.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        Ok(AuthRefreshOutcome {
                            credential_revision: key.credential_revision + 1,
                        })
                    },
                ),
            )
            .await
        });
        tokio::time::sleep(Duration::from_millis(1)).await;
        let patient = single_flight
            .refresh(
                key,
                RequestBudget::from_now(Duration::from_secs(1)),
                CancellationToken::new(),
                move |_key, _budget, _cancellation| async move {
                    patient_runs.fetch_add(1, Ordering::SeqCst);
                    Ok(AuthRefreshOutcome {
                        credential_revision: 99,
                    })
                },
            )
            .await;
        let impatient = impatient.await.expect("impatient task");

        assert!(impatient.is_err());
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        assert_eq!(patient.expect("patient waiter").credential_revision, 12);
    }
}
