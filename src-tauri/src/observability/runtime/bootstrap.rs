#[cfg(test)]
use std::future::Future;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex, OnceLock,
};
use std::time::Instant;

use super::{descriptor::EventDescriptor, EventOutcome, RuntimeDetail, RuntimeLogService};

static SERVICE: OnceLock<Arc<RuntimeLogService>> = OnceLock::new();
static PENDING_INSTALLATION_LEASE_EVENTS: OnceLock<Mutex<Vec<InstallationLeaseEvent>>> =
    OnceLock::new();
#[cfg(test)]
static TEST_SERVICE: OnceLock<Mutex<Option<Arc<RuntimeLogService>>>> = OnceLock::new();
#[cfg(test)]
static TEST_SERVICE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static RATE_LIMIT_CLOCK: OnceLock<Instant> = OnceLock::new();
static LAST_RATE_LIMITED_EVENT_MS: AtomicU64 = AtomicU64::new(0);
const RATE_LIMIT_WINDOW_MS: u64 = 1_000;
const MAX_PENDING_INSTALLATION_LEASE_EVENTS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstallationLeaseEvent {
    Acquired,
    Contended,
    AcquireFailed,
    Released,
    ReleaseFailed,
}

impl InstallationLeaseEvent {
    fn descriptor(
        self,
    ) -> (
        &'static EventDescriptor,
        EventOutcome,
        super::event::LeaseState,
    ) {
        match self {
            Self::Acquired => (
                crate::persistence::runtime_events::installation_lease_acquired(),
                EventOutcome::Ok,
                super::event::LeaseState::Acquired,
            ),
            Self::Contended => (
                crate::persistence::runtime_events::installation_lease_contended(),
                EventOutcome::Overloaded,
                super::event::LeaseState::Unavailable,
            ),
            Self::AcquireFailed => (
                crate::persistence::runtime_events::installation_lease_acquire_failed(),
                EventOutcome::Error,
                super::event::LeaseState::Unavailable,
            ),
            Self::Released => (
                crate::persistence::runtime_events::installation_lease_released(),
                EventOutcome::Ok,
                super::event::LeaseState::Released,
            ),
            Self::ReleaseFailed => (
                crate::persistence::runtime_events::installation_lease_release_failed(),
                EventOutcome::Error,
                super::event::LeaseState::Released,
            ),
        }
    }
}

/// Installs the process-local runtime sink after the application data root is
/// known. Installation is intentionally one-way: bootstrap paths can safely
/// emit before this point, while later fixed-code paths become durable events.
pub(crate) fn install(service: Arc<RuntimeLogService>) {
    if SERVICE.set(Arc::clone(&service)).is_ok() {
        drain_pending_installation_lease_events(&service);
    }
}

pub(crate) fn emit_installation_lease_event(event: InstallationLeaseEvent) {
    if let Some(service) = current_service() {
        record_installation_lease_event(&service, event);
        return;
    }
    let pending = PENDING_INSTALLATION_LEASE_EVENTS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut events) = pending.lock() {
        if events.len() < MAX_PENDING_INSTALLATION_LEASE_EVENTS {
            events.push(event);
            drop(events);
            // Installation can race the initial service lookup. Re-check
            // after enqueueing so an event cannot remain stranded forever.
            if let Some(service) = current_service() {
                drain_pending_installation_lease_events(&service);
            }
            return;
        }
    }
    if let Some(service) = current_service() {
        drain_pending_installation_lease_events(&service);
        record_installation_lease_event(&service, event);
        return;
    }
    // The event code is fixed and contains no external data. Keep the
    // pre-install fallback visible without blocking startup on diagnostics.
    eprintln!("{}", event.descriptor().0.code);
}

fn drain_pending_installation_lease_events(service: &RuntimeLogService) {
    let pending = PENDING_INSTALLATION_LEASE_EVENTS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .map(|mut events| std::mem::take(&mut *events))
        .unwrap_or_default();
    for event in pending {
        record_installation_lease_event(service, event);
    }
}

fn record_installation_lease_event(service: &RuntimeLogService, event: InstallationLeaseEvent) {
    let (descriptor, outcome, state) = event.descriptor();
    service.record_descriptor(descriptor, outcome, RuntimeDetail::Lease { state });
}

/// Fixed-code fallback used before the runtime log service is available.
///
/// This is intentionally the only non-service stderr path. It never formats
/// external errors, paths, identifiers, or environment values.
pub(crate) fn emit(descriptor: &'static EventDescriptor) {
    if let Some(service) = current_service() {
        emit_to(&service, descriptor);
        return;
    }
    eprintln!("{}", descriptor.code);
}

/// Emit the logger's fixed-code fallback without routing through the service.
///
/// This is reserved for bootstrap/runtime-sink failures where the normal JSONL
/// path may be unavailable. The descriptor code is static and contains no
/// caller-controlled data.
pub(crate) fn emit_fixed_stderr(descriptor: &'static EventDescriptor) {
    eprintln!("{}", descriptor.code);
}

fn current_service() -> Option<Arc<RuntimeLogService>> {
    #[cfg(test)]
    if let Some(service) = TEST_SERVICE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|service| service.clone())
    {
        return Some(service);
    }
    SERVICE.get().cloned()
}

/// Runs a producer through the same bootstrap adapter while routing its
/// fixed-code events to an isolated runtime service. This is test-only so
/// production retains one installation-wide service and no alternate sink.
#[cfg(test)]
pub(crate) async fn with_test_service<F, Fut, T>(service: Arc<RuntimeLogService>, operation: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    let _guard = TEST_SERVICE_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let slot = TEST_SERVICE.get_or_init(|| Mutex::new(None));
    let previous = slot
        .lock()
        .expect("test runtime service slot")
        .replace(service);
    let _reset = TestServiceReset { previous };
    operation().await
}

#[cfg(test)]
struct TestServiceReset {
    previous: Option<Arc<RuntimeLogService>>,
}

#[cfg(test)]
impl Drop for TestServiceReset {
    fn drop(&mut self) {
        if let Some(slot) = TEST_SERVICE.get() {
            if let Ok(mut current) = slot.lock() {
                *current = self.previous.take();
            }
        }
    }
}

fn emit_to(service: &RuntimeLogService, descriptor: &'static EventDescriptor) {
    service.record_descriptor(
        descriptor,
        default_outcome(descriptor),
        RuntimeDetail::Redacted {
            reason: super::event::RedactionReason::UnknownError,
        },
    );
}

/// Emits a fixed-code event at most once per process-local window. This is
/// used for malformed boundary metadata, where the business command must
/// continue but an untrusted caller must not be able to fill the runtime log.
pub(crate) fn emit_rate_limited(descriptor: &'static EventDescriptor) {
    // Isolated test services must not contend on the process-wide production
    // rate-limit window with unrelated tests running in parallel.
    #[cfg(test)]
    if TEST_SERVICE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .is_some_and(|service| service.is_some())
    {
        emit(descriptor);
        return;
    }

    let clock = RATE_LIMIT_CLOCK.get_or_init(Instant::now);
    let now_ms = clock.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let mut previous = LAST_RATE_LIMITED_EVENT_MS.load(Ordering::Relaxed);
    loop {
        if previous != 0 && now_ms.saturating_sub(previous) < RATE_LIMIT_WINDOW_MS {
            return;
        }
        match LAST_RATE_LIMITED_EVENT_MS.compare_exchange_weak(
            previous,
            now_ms,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                emit(descriptor);
                return;
            }
            Err(observed) => previous = observed,
        }
    }
}

#[cfg(test)]
pub(crate) fn reset_rate_limit_for_tests() {
    LAST_RATE_LIMITED_EVENT_MS.store(0, Ordering::Release);
}

/// Records a fixed-code failure while preserving the original result. This is
/// used at application boundaries where the public error contract must remain
/// unchanged; the error value is deliberately never inspected or formatted.
pub(crate) fn record_failure<T, E>(
    descriptor: &'static EventDescriptor,
    result: Result<T, E>,
) -> Result<T, E> {
    if result.is_err() {
        emit(descriptor);
    }
    result
}

fn default_outcome(descriptor: &super::descriptor::EventDescriptor) -> EventOutcome {
    descriptor
        .outcomes
        .iter()
        .copied()
        .find(|outcome| *outcome == EventOutcome::Degraded)
        .or_else(|| descriptor.outcomes.first().copied())
        .expect("runtime event descriptor has at least one outcome")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::observability::runtime::{
        catalog::{Catalog, OWNER_EVENT_DESCRIPTOR_SLICES},
        event::{DetailKind, RuntimeEvent},
        reader::RuntimeLogReader,
        service::RuntimeLogState,
        Component, EventLevel,
    };

    #[test]
    fn producer_domains_keep_catalog_component_mapping() {
        assert_eq!(
            Catalog::descriptor("proxy.lifecycle.start_failed").component,
            Component::Proxy
        );
        assert_eq!(
            Catalog::descriptor("collector.station.failed").component,
            Component::Collector
        );
        assert_eq!(
            Catalog::descriptor("monitoring.runner.failed").component,
            Component::Monitoring
        );
        assert_eq!(
            Catalog::descriptor("migration.portable.import_failed").component,
            Component::Migration
        );
        assert_eq!(
            Catalog::descriptor("updater.manifest.inspect_failed").component,
            Component::Migration
        );
        assert_eq!(
            Catalog::descriptor("monitoring.runner.degraded").level,
            EventLevel::Warn
        );
        assert_eq!(
            Catalog::descriptor("monitoring.runner.failed").level,
            EventLevel::Warn
        );
        assert_eq!(
            Catalog::descriptor("frontend.boundary.failed").level,
            EventLevel::Error
        );
        assert_eq!(
            default_outcome(Catalog::descriptor("monitoring.runner.degraded")),
            EventOutcome::Degraded
        );
        assert_eq!(
            Catalog::descriptor("monitoring.runner.cancelled").level,
            EventLevel::Info
        );
        assert_eq!(
            Catalog::descriptor("monitoring.runner.cancelled").outcomes,
            &[
                EventOutcome::Ok,
                EventOutcome::Error,
                EventOutcome::Cancelled,
                EventOutcome::Timeout,
                EventOutcome::Overloaded,
                EventOutcome::Degraded
            ]
        );
    }

    #[test]
    fn injected_producer_events_publish_typed_jsonl_without_dynamic_payloads() {
        let root = tempfile::tempdir().expect("runtime root");
        let service = RuntimeLogService::open(root.path());
        assert_eq!(service.state(), RuntimeLogState::Ready);

        let producer_slices = [
            crate::services::proxy::runtime_events::EVENT_DESCRIPTORS,
            crate::services::station_collectors::runtime_events::EVENT_DESCRIPTORS,
            crate::services::monitoring::runtime_events::EVENT_DESCRIPTORS,
            crate::services::portable_migration::runtime_events::EVENT_DESCRIPTORS,
            crate::services::updater::runtime_events::EVENT_DESCRIPTORS,
        ];
        let expected = producer_slices
            .iter()
            .flat_map(|slice| slice.iter().map(|descriptor| descriptor.code))
            .collect::<BTreeSet<_>>();
        for descriptor in producer_slices.iter().flat_map(|slice| slice.iter()) {
            emit_to(&service, descriptor);
        }
        service.flush();

        let reader = RuntimeLogReader::new(root.path());
        let page = reader.read_page(0, 200, 1024 * 1024);
        assert!(page.issues.is_empty(), "reader issues: {:?}", page.issues);
        let mut observed = BTreeSet::new();
        for line in page.lines {
            let event: RuntimeEvent =
                serde_json::from_slice(line.as_bytes()).expect("typed runtime event");
            assert!(Catalog::accepts_event(&event), "uncatalogued event");
            assert_eq!(event.detail.kind(), DetailKind::Redacted);
            let encoded = String::from_utf8_lossy(line.as_bytes());
            assert!(!encoded.contains("sk-secret"));
            assert!(!encoded.contains("Authorization"));
            observed.insert(event.event_code.as_str().to_owned());
        }
        assert_eq!(observed.len(), expected.len());
        assert_eq!(
            observed.into_iter().collect::<BTreeSet<_>>(),
            expected.into_iter().map(str::to_owned).collect()
        );

        // The producer adapter remains non-blocking while another process owns
        // the installation lease; after release, the same injected event is
        // durably published by the bounded retry worker.
        let contender = RuntimeLogService::open(root.path());
        assert_eq!(contender.state(), RuntimeLogState::Degraded);
        drop(service);
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && contender.state() != RuntimeLogState::Ready {
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(contender.state(), RuntimeLogState::Ready);
        emit_to(
            &contender,
            crate::services::proxy::runtime_events::lifecycle_start_failed(),
        );
        contender.flush();
        let recovered = RuntimeLogReader::new(root.path()).read_page(0, 200, 1024 * 1024);
        assert!(recovered
            .lines
            .iter()
            .any(|line| String::from_utf8_lossy(line.as_bytes())
                .contains("proxy.lifecycle.start_failed")));

        // Keep the full owner aggregate in scope so this test also fails if a
        // producer descriptor is accidentally removed from the catalog.
        assert!(OWNER_EVENT_DESCRIPTOR_SLICES
            .iter()
            .flat_map(|slice| slice.iter())
            .any(|descriptor| descriptor.code == "proxy.lifecycle.start_failed"));
    }

    #[tokio::test]
    async fn fixed_failure_adapter_publishes_migration_and_updater_jsonl_events() {
        let root = tempfile::tempdir().expect("runtime root");
        let service = Arc::new(RuntimeLogService::open(root.path()));
        with_test_service(Arc::clone(&service), || async {
            let migration: Result<(), &str> = Err("fixture migration failure sk-secret");
            let updater: Result<(), &str> = Err("fixture updater failure Authorization");
            assert!(record_failure(
                crate::services::portable_migration::runtime_events::inspect_failed(),
                migration,
            )
            .is_err());
            assert!(record_failure(
                crate::services::updater::runtime_events::manifest_inspect_failed(),
                updater,
            )
            .is_err());
        })
        .await;
        service.flush();

        let page = RuntimeLogReader::new(root.path()).read_page(0, 50, 1024 * 1024);
        assert!(page.issues.is_empty(), "reader issues: {:?}", page.issues);
        let events = page
            .lines
            .iter()
            .filter_map(|line| serde_json::from_slice::<RuntimeEvent>(line.as_bytes()).ok())
            .collect::<Vec<_>>();
        assert!(events.iter().any(|event| {
            event.event_code.as_str() == "migration.portable.inspect_failed"
                && event.component == Component::Migration
        }));
        assert!(events.iter().any(|event| {
            event.event_code.as_str() == "updater.manifest.inspect_failed"
                && event.component == Component::Migration
        }));
        let raw = page
            .lines
            .iter()
            .map(|line| line.as_bytes())
            .collect::<Vec<_>>();
        assert!(!raw.iter().any(|line| {
            line.windows(9).any(|window| window == b"sk-secret")
                || line.windows(13).any(|window| window == b"Authorization")
        }));
    }
}
