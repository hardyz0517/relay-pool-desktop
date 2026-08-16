use super::catalog::{Catalog, CatalogError, OWNER_EVENT_DESCRIPTOR_SLICES};
use super::clock::{ClockAdjustment, ClockGuard, MonotonicTimer};
use super::descriptor::{
    EventDescriptor, Lifecycle, SamplingPolicy, CORE_DETAILS, CORE_OUTCOMES, CORE_SUBJECTS,
};
use super::error::{DataDisposition, RuntimeError};
use super::event::{
    Component, EventLevel, EventOutcome, RuntimeDetail, RuntimeEvent, MAX_SERIALIZED_EVENT_BYTES,
};
use super::subject::{InteractionId, StableEventCode};

#[test]
fn runtime_event_is_typed_bounded_and_serializes_as_one_line() {
    let event = RuntimeEvent::new(
        1_723_600_000_000,
        7,
        EventLevel::Error,
        StableEventCode::new("ipc.command.failed").expect("stable code"),
        Component::Ipc,
        EventOutcome::Error,
        super::subject::SessionId::new(),
        None,
        Some(InteractionId::new()),
        None,
        None,
        Some(super::clock::Elapsed::from_duration(
            std::time::Duration::from_millis(12),
        )),
        Some(RuntimeError::new(
            StableEventCode::new("ipc").expect("domain"),
            StableEventCode::new("command_failed").expect("error code"),
            true,
            DataDisposition::Redacted,
        )),
        RuntimeDetail::Redacted {
            reason: super::event::RedactionReason::UnknownError,
        },
    )
    .expect("valid event");

    let line = event.to_json_line().expect("json line");
    assert!(line.ends_with('\n'));
    assert!(line.len() <= MAX_SERIALIZED_EVENT_BYTES);
    let round_trip: RuntimeEvent = serde_json::from_str(&line).expect("round trip");
    assert_eq!(round_trip.sequence, 7);
}

#[test]
fn runtime_event_rejects_untyped_error_and_invalid_retry_detail() {
    assert!(RuntimeEvent::new(
        1,
        1,
        EventLevel::Info,
        StableEventCode::new("ipc.command.completed").expect("stable code"),
        Component::Ipc,
        EventOutcome::Ok,
        super::subject::SessionId::new(),
        None,
        None,
        None,
        None,
        None,
        Some(RuntimeError::new(
            StableEventCode::new("ipc").unwrap(),
            StableEventCode::new("unexpected").unwrap(),
            false,
            DataDisposition::Redacted,
        )),
        RuntimeDetail::None,
    )
    .is_err());

    assert!(RuntimeEvent::new(
        1,
        1,
        EventLevel::Warn,
        StableEventCode::new("ipc.command.retrying").unwrap(),
        Component::Ipc,
        EventOutcome::Degraded,
        super::subject::SessionId::new(),
        None,
        None,
        None,
        None,
        None,
        None,
        RuntimeDetail::Retry {
            attempt: 0,
            max_attempts: 3,
        },
    )
    .is_err());
}

#[test]
fn catalog_rejects_event_with_descriptor_level_mismatch() {
    let event = RuntimeEvent::new(
        1,
        1,
        EventLevel::Info,
        StableEventCode::new("runtime.log_event.dropped").expect("stable code"),
        Component::Runtime,
        EventOutcome::Ok,
        super::subject::SessionId::new(),
        None,
        None,
        None,
        None,
        None,
        None,
        RuntimeDetail::None,
    )
    .expect("valid event");
    assert!(!Catalog::accepts_event(&event));

    let matching = RuntimeEvent::new(
        1,
        2,
        EventLevel::Warn,
        StableEventCode::new("runtime.log_event.dropped").expect("stable code"),
        Component::Runtime,
        EventOutcome::Ok,
        super::subject::SessionId::new(),
        None,
        None,
        None,
        None,
        None,
        None,
        RuntimeDetail::None,
    )
    .expect("valid event");
    assert!(Catalog::accepts_event(&matching));
}

#[test]
fn interaction_ids_are_anonymous_and_clock_uses_monotonic_deltas() {
    let id = InteractionId::new();
    assert!(id.as_str().starts_with("int_"));
    assert!(!id.as_str().contains("https"));

    let mut clock = ClockGuard::new(100);
    assert_eq!(clock.sample_at(1_000, 10).adjustment, ClockAdjustment::None);
    assert_eq!(
        clock.sample_at(950, 20).adjustment,
        ClockAdjustment::Rollback
    );
    assert_eq!(
        clock.sample_at(2_000, 30).adjustment,
        ClockAdjustment::ForwardJump
    );

    let timer = MonotonicTimer::start();
    assert!(timer.elapsed().as_millis() < 1_000);
}

#[test]
fn clock_adjustment_recovers_after_monotonic_observation_window() {
    let mut clock = ClockGuard::new(100);
    assert!(clock.is_stable());
    assert_eq!(clock.sample_at(1_000, 10).adjustment, ClockAdjustment::None);
    assert!(!clock.is_stable());
    assert_eq!(
        clock.sample_at(1_010, 30_010).adjustment,
        ClockAdjustment::None
    );
    assert!(clock.is_stable());
    assert_eq!(
        clock.sample_at(800, 30_020).adjustment,
        ClockAdjustment::Rollback
    );
    assert!(!clock.is_stable());
    assert_eq!(
        clock.sample_at(900, 30_030).adjustment,
        ClockAdjustment::None
    );
    assert!(!clock.is_stable());
    assert_eq!(
        clock.sample_at(1_900, 60_030).adjustment,
        ClockAdjustment::None
    );
    assert!(clock.is_stable());
}

#[test]
fn catalog_manifest_is_globally_unique_and_hashed() {
    let manifest = Catalog::build(OWNER_EVENT_DESCRIPTOR_SLICES).expect("catalog");
    let descriptor_count = OWNER_EVENT_DESCRIPTOR_SLICES
        .iter()
        .map(|slice| slice.len())
        .sum::<usize>();
    assert_eq!(manifest.manifest_version, 1);
    assert_eq!(manifest.events.len(), descriptor_count);
    assert_eq!(manifest.manifest_id.len(), 64);
    assert!(!CORE_OUTCOMES.is_empty());
    assert!(!CORE_DETAILS.is_empty());
    assert!(!CORE_SUBJECTS.is_empty());
}

const DUPLICATE: &[EventDescriptor] = &[EventDescriptor {
    code: "runtime.duplicate",
    owner: "fixture.owner",
    event_schema_version: 1,
    detail_schema_version: 1,
    component: Component::Runtime,
    level: EventLevel::Info,
    outcomes: CORE_OUTCOMES,
    details: CORE_DETAILS,
    subjects: CORE_SUBJECTS,
    sampling: SamplingPolicy::Default,
    support_bundle: false,
    message_key: "runtime.duplicate",
    lifecycle: Lifecycle::Active,
}];

#[test]
fn catalog_rejects_global_code_collisions() {
    assert_eq!(
        Catalog::build(&[DUPLICATE, DUPLICATE]),
        Err(CatalogError::DuplicateCode)
    );
}

#[test]
fn catalog_rejects_unknown_replacements() {
    const DEPRECATED: &[EventDescriptor] = &[EventDescriptor {
        code: "runtime.old",
        owner: "fixture.owner",
        event_schema_version: 1,
        detail_schema_version: 1,
        component: Component::Runtime,
        level: EventLevel::Info,
        outcomes: CORE_OUTCOMES,
        details: CORE_DETAILS,
        subjects: CORE_SUBJECTS,
        sampling: SamplingPolicy::Default,
        support_bundle: false,
        message_key: "runtime.old",
        lifecycle: Lifecycle::Deprecated {
            replaced_by: "runtime.missing",
            sunset_version: 2,
        },
    }];

    assert_eq!(
        Catalog::build(&[DEPRECATED]),
        Err(CatalogError::UnknownReplacement)
    );
}

#[test]
fn catalog_owner_slices_preserve_the_aggregate_manifest() {
    let manifest = Catalog::build(OWNER_EVENT_DESCRIPTOR_SLICES).expect("owner slices");
    let expected_codes = OWNER_EVENT_DESCRIPTOR_SLICES
        .iter()
        .flat_map(|slice| slice.iter().map(|descriptor| descriptor.code))
        .collect::<std::collections::BTreeSet<_>>();
    let manifest_codes = manifest
        .events
        .iter()
        .map(|event| event.code.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(manifest_codes, expected_codes);
    assert_eq!(manifest.events.len(), expected_codes.len());
    assert!(manifest
        .events
        .iter()
        .any(|event| event.owner == "proxy.runtime"));
    assert!(manifest
        .events
        .iter()
        .any(|event| event.owner == "updater.manifests"));
}

#[test]
fn invalid_runtime_context_is_catalogued_as_rate_limited() {
    let manifest = Catalog::build(OWNER_EVENT_DESCRIPTOR_SLICES).expect("core runtime catalog");
    let descriptor = manifest
        .events
        .iter()
        .find(|event| event.code == "ipc.runtime_context.invalid")
        .expect("invalid context descriptor");
    assert_eq!(descriptor.sampling, SamplingPolicy::RateLimited);
    assert_eq!(descriptor.owner, "ipc.runtime_context");
}

#[test]
fn producer_codes_are_catalogued_with_safe_classification() {
    let manifest = Catalog::build(OWNER_EVENT_DESCRIPTOR_SLICES).expect("runtime catalog");
    let expected = [
        (
            "proxy.lifecycle.start_started",
            Component::Proxy,
            EventLevel::Info,
        ),
        (
            "proxy.lifecycle.start_succeeded",
            Component::Proxy,
            EventLevel::Info,
        ),
        (
            "proxy.lifecycle.start_failed",
            Component::Proxy,
            EventLevel::Warn,
        ),
        (
            "proxy.lifecycle.stop_succeeded",
            Component::Proxy,
            EventLevel::Info,
        ),
        (
            "proxy.lifecycle.stop_failed",
            Component::Proxy,
            EventLevel::Warn,
        ),
        (
            "proxy.lifecycle.drain_succeeded",
            Component::Proxy,
            EventLevel::Info,
        ),
        (
            "proxy.lifecycle.drain_failed",
            Component::Proxy,
            EventLevel::Warn,
        ),
        (
            "proxy.lifecycle.drain_timeout",
            Component::Proxy,
            EventLevel::Warn,
        ),
        (
            "collector.station.failed",
            Component::Collector,
            EventLevel::Warn,
        ),
        (
            "collector.station.cancelled",
            Component::Collector,
            EventLevel::Info,
        ),
        (
            "monitoring.runner.failed",
            Component::Monitoring,
            EventLevel::Warn,
        ),
        (
            "monitoring.runner.cancelled",
            Component::Monitoring,
            EventLevel::Info,
        ),
        (
            "monitoring.maintenance.failed",
            Component::Monitoring,
            EventLevel::Warn,
        ),
        (
            "monitoring.maintenance.cancelled",
            Component::Monitoring,
            EventLevel::Info,
        ),
        (
            "migration.portable.export_failed",
            Component::Migration,
            EventLevel::Warn,
        ),
        (
            "migration.portable.inspect_failed",
            Component::Migration,
            EventLevel::Warn,
        ),
        (
            "migration.portable.prepare_failed",
            Component::Migration,
            EventLevel::Warn,
        ),
        (
            "updater.manifest.inspect_failed",
            Component::Migration,
            EventLevel::Warn,
        ),
    ];
    for (code, component, level) in expected {
        let descriptor = manifest
            .events
            .iter()
            .find(|event| event.code == code)
            .unwrap_or_else(|| panic!("missing producer descriptor: {code}"));
        assert_eq!(descriptor.component, component, "component for {code}");
        assert_eq!(descriptor.level, level, "level for {code}");
        assert!(
            descriptor.support_bundle,
            "support bundle permission for {code}"
        );
    }
}
