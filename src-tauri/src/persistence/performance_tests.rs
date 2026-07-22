use crate::{
    application::{
        changes::ChangeService,
        clock::SystemClock,
        ids::UuidV7Generator,
        pagination::PageLimit,
        request_finalization::RequestFinalizationService,
        request_lifecycle::{
            delivery::DeliveryTerminal,
            ports::RequestLifecycleStore,
            request::{
                FinalRequestRecord, RequestCompletion, RequestContextSnapshot,
                RequestLogAnnotations, RequestTerminal, RequestTerminalSnapshot,
            },
        },
        request_logs::RequestLogService,
        routing::RoutingService,
    },
    persistence::{
        runtime::{PersistenceHandle, PersistenceRuntime},
        write_coordinator::WriteCoordinatorSnapshot,
    },
};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[cfg(windows)]
use windows_sys::Win32::System::{
    ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX},
    Threading::GetCurrentProcess,
};

const STATION_COUNT: i64 = 100;
const STATION_KEY_COUNT: i64 = 1_000;
const REQUEST_LOG_COUNT: i64 = 10_000;
const EVIDENCE_ROW_COUNT: i64 = 100_000;
const SAMPLE_COUNT: usize = 40;
const STARTUP_SAMPLE_COUNT: usize = 15;

const ROUTING_P95_LIMIT: Duration = Duration::from_millis(50);
const WRITE_P95_LIMIT: Duration = Duration::from_millis(100);
const POOL_ACQUIRE_P95_LIMIT: Duration = Duration::from_millis(20);
const PRIVATE_USAGE_LIMIT_BYTES: u64 = 256 * 1024 * 1024;
static NEXT_MEASUREMENT_ID: AtomicU64 = AtomicU64::new(1);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn standard_fixture_meets_approved_absolute_performance_gates() {
    #[cfg(windows)]
    let memory_sampler =
        PrivateUsageSampler::start().expect("start standard-suite private usage sampler");
    let fixture = StandardFixture::create().await;
    let runtime = PersistenceRuntime::open_current(&fixture.path)
        .await
        .expect("open standard fixture");
    assert_standard_counts(&runtime.handle()).await;
    let routing = RoutingService::new(runtime.handle());
    let request_logs = RequestLogService::new(runtime.handle());
    let changes = ChangeService::new(
        runtime.handle(),
        Arc::new(SystemClock),
        Arc::new(UuidV7Generator),
    );

    let routing = sample(SAMPLE_COUNT, || async {
        let started = Instant::now();
        let candidate_count = routing
            .load_runtime_candidates()
            .await
            .expect("routing candidates");
        assert_eq!(candidate_count.len() as i64, STATION_KEY_COUNT);
        started.elapsed()
    })
    .await;

    let hot_request_logs = sample(SAMPLE_COUNT, || async {
        let started = Instant::now();
        let row_count = request_logs
            .list_recent(PageLimit::new(500).expect("bounded request-log page"))
            .await
            .expect("hot production request-log page");
        assert_eq!(row_count.len(), 500);
        started.elapsed()
    })
    .await;

    let hot_change_events_first_page = sample(SAMPLE_COUNT, || async {
        let started = Instant::now();
        let item_count = changes
            .list(
                None,
                PageLimit::new(200).expect("bounded change-event page"),
            )
            .await
            .expect("hot production change-event page");
        assert_eq!(item_count.len(), 200);
        started.elapsed()
    })
    .await;

    let pool_acquire = sample(SAMPLE_COUNT, || async {
        let started = Instant::now();
        let mut read = runtime.begin_read().await.expect("pool acquire");
        let _: i64 = sqlx::query_scalar("SELECT 1")
            .fetch_one(read.connection())
            .await
            .expect("pool probe");
        started.elapsed()
    })
    .await;

    let ordinary_write = sample(SAMPLE_COUNT, || {
        let handle = runtime.handle();
        async move { measured_ordinary_write(&handle).await }
    })
    .await;

    assert_p95("routing candidate load", &routing, ROUTING_P95_LIMIT);
    assert_p95(
        "ordinary write transaction",
        &ordinary_write,
        WRITE_P95_LIMIT,
    );
    assert_p95("pool acquire", &pool_acquire, POOL_ACQUIRE_P95_LIMIT);

    let write_queue = runtime.handle().write_metrics();
    runtime.close().await.expect("close persistence runtime");

    #[cfg(windows)]
    let memory_observation = memory_sampler
        .finish()
        .expect("finish standard-suite private usage sampler");
    #[cfg(windows)]
    assert_standard_observations(
        memory_observation.peak_private_usage_delta_bytes,
        write_queue,
    );
    #[cfg(windows)]
    let memory = memory_observation.to_json();
    #[cfg(not(windows))]
    let memory = json!({
        "status": "unsupported",
        "metric": "PROCESS_MEMORY_COUNTERS_EX.PrivateUsage",
        "sampleIntervalMs": 10,
        "limitBytes": PRIVATE_USAGE_LIMIT_BYTES,
    });

    let report = qualification_report(
        "standard",
        &[
            ("routingCandidateLoad", &routing),
            ("hotRequestLogs", &hot_request_logs),
            ("hotChangeEventsFirstPage", &hot_change_events_first_page),
            ("ordinaryWrite", &ordinary_write),
            ("poolAcquire", &pool_acquire),
        ],
        qualification_provenance(),
        qualification_environment(),
        memory,
        write_queue,
        json!({}),
    );
    println!("PERSISTENCE_QUALIFICATION {report}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn startup_and_finalization_under_bounded_concurrent_reads_are_measured() {
    let fixture = StandardFixture::create().await;

    let mut startup = Vec::with_capacity(STARTUP_SAMPLE_COUNT);
    for _ in 0..STARTUP_SAMPLE_COUNT {
        let started = Instant::now();
        let runtime = PersistenceRuntime::open_current(&fixture.path)
            .await
            .expect("startup without migration");
        startup.push(started.elapsed());
        runtime.close().await.expect("close persistence runtime");
    }

    let runtime = PersistenceRuntime::open_current(&fixture.path)
        .await
        .expect("open concurrent fixture");
    let handle = runtime.handle();
    let changes = ChangeService::new(
        runtime.handle(),
        Arc::new(SystemClock),
        Arc::new(UuidV7Generator),
    );
    let reader_count = 3_usize;
    let iterations_per_reader = 25_usize;
    let mut readers = Vec::with_capacity(reader_count);
    for _ in 0..reader_count {
        let changes = changes.clone();
        readers.push(tokio::spawn(async move {
            for _ in 0..iterations_per_reader {
                let page = changes
                    .list(None, PageLimit::new(200).expect("bounded concurrent page"))
                    .await
                    .expect("bounded concurrent read");
                assert_eq!(page.len(), 200);
            }
        }));
    }
    assert_eq!(readers.len(), reader_count, "reader task fan-out changed");

    let finalization = sample(SAMPLE_COUNT, || {
        let handle = handle.clone();
        async move { measured_request_log_finalization(&handle).await }
    })
    .await;
    for reader in readers {
        reader.await.expect("bounded reader task joined");
    }

    assert_p95(
        "finalization write under concurrent reads",
        &finalization,
        WRITE_P95_LIMIT,
    );
    let write_queue = runtime.handle().write_metrics();
    runtime.close().await.expect("close persistence runtime");
    assert_standard_observations(0, write_queue);
    let report = startup_qualification_report(&startup, &finalization);
    println!("PERSISTENCE_QUALIFICATION {report}");
}

async fn measured_ordinary_write(handle: &PersistenceHandle) -> Duration {
    let id = next_measurement_id("ordinary");
    let started = Instant::now();
    handle
        .write(move |write| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO persistence_qualification_writes (id, created_at) VALUES (?1, ?2)",
                )
                .bind(id)
                .bind("2026-07-20T00:00:00Z")
                .execute(write.connection())
                .await?;
                Ok(())
            })
        })
        .await
        .expect("measured write");
    started.elapsed()
}

async fn measured_request_log_finalization(handle: &PersistenceHandle) -> Duration {
    let id = next_measurement_id("finalization");
    let finalization = RequestFinalizationService::new(handle.clone());
    finalization
        .start_request(
            crate::application::request_lifecycle::request::RequestStartRecord {
                context: RequestContextSnapshot {
                    request_id: id.clone(),
                    method: "POST".to_string(),
                    local_path: "/v1/responses".to_string(),
                    endpoint: "/v1/responses".to_string(),
                    received_at_ms: 1,
                },
            },
        )
        .await
        .expect("admit measured request-log finalization");
    let started = Instant::now();
    finalization
        .finish_request(FinalRequestRecord {
            context: RequestContextSnapshot {
                request_id: id,
                method: "POST".to_string(),
                local_path: "/v1/responses".to_string(),
                endpoint: "/v1/responses".to_string(),
                received_at_ms: 1,
            },
            terminal: RequestTerminalSnapshot {
                terminal: RequestTerminal::Completed(RequestCompletion {
                    protocol_completed: true,
                    attempt_id: None,
                }),
                delivery: DeliveryTerminal::BodyCompleted,
            },
            selected_attempt_id: None,
            attempt_count: 0,
            fallback_count: 0,
            annotations: RequestLogAnnotations::default(),
        })
        .await
        .expect("measured request-log finalization");
    started.elapsed()
}

async fn assert_standard_counts(handle: &PersistenceHandle) {
    let mut read = handle.begin_read().await.expect("fixture count read");
    for (table, expected) in [
        ("stations", STATION_COUNT),
        ("station_keys", STATION_KEY_COUNT),
        ("request_logs", REQUEST_LOG_COUNT),
        ("change_events", EVIDENCE_ROW_COUNT),
    ] {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let actual: i64 = sqlx::query_scalar(&sql)
            .fetch_one(read.connection())
            .await
            .expect("fixture row count");
        assert_eq!(actual, expected, "standard fixture table {table}");
    }
}

async fn sample<F, Fut>(count: usize, mut operation: F) -> Vec<Duration>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Duration>,
{
    operation().await;
    let mut samples = Vec::with_capacity(count);
    for _ in 0..count {
        samples.push(operation().await);
    }
    samples
}

fn assert_p95(name: &str, samples: &[Duration], limit: Duration) {
    // Absolute timing gates require the controlled release-profile qualification run.
    // Debug timings remain emitted for engineering diagnostics but are not release evidence.
    if cfg!(debug_assertions) {
        return;
    }
    let actual = percentile(samples, 0.95);
    assert!(
        actual < limit,
        "{name} p95 {:?} must remain below approved limit {:?}",
        actual,
        limit
    );
}

fn percentile(samples: &[Duration], quantile: f64) -> Duration {
    assert!(!samples.is_empty());
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = ((sorted.len() as f64 * quantile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[rank]
}

fn relative_percentile_gate(baseline: &[Duration], current: &[Duration], quantile: f64) -> bool {
    let baseline_ns = percentile(baseline, quantile).as_nanos();
    let current_ns = percentile(current, quantile).as_nanos();
    current_ns.saturating_mul(10) <= baseline_ns.saturating_mul(11)
}

fn qualification_report(
    suite: &str,
    metrics: &[(&str, &Vec<Duration>)],
    provenance: serde_json::Value,
    environment: serde_json::Value,
    memory: serde_json::Value,
    write_queue: WriteCoordinatorSnapshot,
    baseline_metrics: serde_json::Value,
) -> serde_json::Value {
    let metric_values = metrics
        .iter()
        .map(|(name, samples)| {
            (
                (*name).to_string(),
                json!({
                    "medianNs": percentile(samples, 0.50).as_nanos(),
                    "p95Ns": percentile(samples, 0.95).as_nanos(),
                    "samplesNs": samples.iter().map(Duration::as_nanos).collect::<Vec<_>>(),
                    "medianMs": duration_ms(percentile(samples, 0.50)),
                    "p95Ms": duration_ms(percentile(samples, 0.95)),
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    json!({
        "schemaVersion": 1,
        "suite": suite,
        "fixture": {
            "stations": STATION_COUNT,
            "stationKeys": STATION_KEY_COUNT,
            "requestLogs": REQUEST_LOG_COUNT,
            "changeEvents": EVIDENCE_ROW_COUNT,
        },
        "workloads": {
            "requestLogs": {
                "rows": 500,
                "projection": "production-request-log-service-full-row-projection",
            },
            "changeEvents": {
                "queryLimit": 201,
                "returnedRows": 200,
                "projection": "production-change-service-first-page-projection",
            },
            "startup": { "migrationsIncluded": false },
        },
        "provenance": provenance,
        "environment": environment,
        "memory": memory,
        "queues": {
            "qualificationScope": "write-coordinator-only-not-all-queues",
            "writeCoordinator": {
                "currentDepth": write_queue.current_queue_depth,
                "peakDepth": write_queue.peak_queue_depth,
                "acquiredWrites": write_queue.acquired_writes,
                "totalWaitMicros": write_queue.total_queue_wait_micros,
                "committedWrites": write_queue.committed_writes,
                "rolledBackWrites": write_queue.rolled_back_writes,
            },
            "finalizationService": {
                "coverage": "production-request-finalization-service-terminal-transition",
                "snapshot": serde_json::Value::Null,
            },
        },
        "baselineMetrics": baseline_metrics,
        "metrics": metric_values,
    })
}

fn assert_standard_observations(
    private_usage_delta_bytes: u64,
    write_queue: WriteCoordinatorSnapshot,
) {
    assert!(
        private_usage_delta_bytes <= PRIVATE_USAGE_LIMIT_BYTES,
        "PrivateUsage peak delta {private_usage_delta_bytes} must remain at or below {PRIVATE_USAGE_LIMIT_BYTES} bytes"
    );
    assert_eq!(
        write_queue.current_queue_depth, 0,
        "write coordinator queue must drain before qualification reporting"
    );
    assert_eq!(
        write_queue.acquired_writes,
        write_queue.committed_writes + write_queue.rolled_back_writes,
        "every acquired write must have a terminal coordinator outcome"
    );
}

fn startup_qualification_report(
    startup: &[Duration],
    finalization: &[Duration],
) -> serde_json::Value {
    json!({
        "schemaVersion": 1,
        "suite": "startup-and-concurrent-finalization",
        "provenance": qualification_provenance(),
        "environment": qualification_environment(),
        "workloads": { "startup": { "migrationsIncluded": false } },
        "metrics": {
            "startupWithoutMigration": metric_report(startup),
            "finalization": {
                "coverage": "production-request-finalization-service-terminal-transition",
                "medianNs": percentile(finalization, 0.50).as_nanos(),
                "p95Ns": percentile(finalization, 0.95).as_nanos(),
                "samplesNs": finalization.iter().map(Duration::as_nanos).collect::<Vec<_>>(),
            },
        },
    })
}

fn metric_report(samples: &[Duration]) -> serde_json::Value {
    json!({
        "medianNs": percentile(samples, 0.50).as_nanos(),
        "p95Ns": percentile(samples, 0.95).as_nanos(),
        "samplesNs": samples.iter().map(Duration::as_nanos).collect::<Vec<_>>(),
        "medianMs": duration_ms(percentile(samples, 0.50)),
        "p95Ms": duration_ms(percentile(samples, 0.95)),
    })
}

fn qualification_provenance() -> serde_json::Value {
    let parsed = std::env::var("PERSISTENCE_QUALIFICATION_PROVENANCE_JSON")
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());
    if let Some(mut provenance) = parsed {
        provenance["build"]["profile"] = json!(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        });
        provenance
    } else {
        json!({
            "v2Commit": "unavailable-debug-run",
            "build": {
                "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
                "locked": false,
            },
            "worktreeSnapshot": { "kind": "unqualified-debug-run" },
        })
    }
}

fn qualification_environment() -> serde_json::Value {
    let parsed = std::env::var("PERSISTENCE_QUALIFICATION_ENVIRONMENT_JSON")
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());
    if let Some(mut environment) = parsed {
        environment["debugAssertions"] = json!(cfg!(debug_assertions));
        environment
    } else {
        json!({
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "debugAssertions": cfg!(debug_assertions),
            "availableParallelism": std::thread::available_parallelism().map(usize::from).ok(),
        })
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

#[test]
fn relative_gate_rejects_more_than_ten_percent_regression() {
    let baseline = [
        Duration::from_nanos(100),
        Duration::from_nanos(100),
        Duration::from_nanos(100),
    ];
    let exactly_ten_percent = [
        Duration::from_nanos(110),
        Duration::from_nanos(110),
        Duration::from_nanos(110),
    ];
    let above_ten_percent = [
        Duration::from_nanos(111),
        Duration::from_nanos(111),
        Duration::from_nanos(111),
    ];

    assert!(relative_percentile_gate(
        &baseline,
        &exactly_ten_percent,
        0.95,
    ));
    assert!(!relative_percentile_gate(
        &baseline,
        &above_ten_percent,
        0.95,
    ));
    assert!(relative_percentile_gate(
        &baseline,
        &exactly_ten_percent,
        0.50,
    ));
    assert!(!relative_percentile_gate(
        &baseline,
        &above_ten_percent,
        0.50,
    ));
}

#[test]
fn qualification_report_requires_memory_queue_and_environment() {
    let samples = vec![Duration::from_nanos(100), Duration::from_nanos(110)];
    let queue = WriteCoordinatorSnapshot {
        current_queue_depth: 0,
        peak_queue_depth: 2,
        acquired_writes: 4,
        total_queue_wait_micros: 5,
        committed_writes: 3,
        rolled_back_writes: 1,
    };
    let report = qualification_report(
        "standard",
        &[("hotRequestLogs", &samples)],
        json!({ "baselineKind": "reconstructed-v0.3.1-source-baseline" }),
        json!({ "cpuModel": "contract CPU", "logicalProcessors": 16 }),
        json!({
            "metric": "PROCESS_MEMORY_COUNTERS_EX.PrivateUsage",
            "sampleIntervalMs": 10,
            "baselinePrivateUsageBytes": 100,
            "peakPrivateUsageBytes": 200,
            "peakPrivateUsageDeltaBytes": 100,
            "limitBytes": 268435456_u64,
        }),
        queue,
        json!({ "hotRequestLogs": { "p95Ns": 100 } }),
    );

    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["suite"], "standard");
    assert_eq!(
        report["provenance"]["baselineKind"],
        "reconstructed-v0.3.1-source-baseline"
    );
    assert_eq!(report["environment"]["cpuModel"], "contract CPU");
    assert_eq!(
        report["memory"]["metric"],
        "PROCESS_MEMORY_COUNTERS_EX.PrivateUsage"
    );
    assert_eq!(report["memory"]["limitBytes"], 268435456_u64);
    assert_eq!(report["workloads"]["requestLogs"]["rows"], 500);
    assert_eq!(report["workloads"]["changeEvents"]["returnedRows"], 200);
    assert_eq!(
        report["queues"]["qualificationScope"],
        "write-coordinator-only-not-all-queues"
    );
    assert_eq!(report["queues"]["writeCoordinator"]["currentDepth"], 0);
    assert_eq!(report["queues"]["writeCoordinator"]["peakDepth"], 2);
    assert_eq!(
        report["queues"]["finalizationService"]["coverage"],
        "production-request-finalization-service-terminal-transition"
    );
    assert!(report["queues"]["finalizationService"]["snapshot"].is_null());
    assert_eq!(report["baselineMetrics"]["hotRequestLogs"]["p95Ns"], 100);
    assert!(report["metrics"]["hotRequestLogs"]["samplesNs"].is_array());
}

#[test]
fn standard_observation_gate_rejects_memory_or_queue_contract_violations() {
    let healthy_queue = WriteCoordinatorSnapshot {
        current_queue_depth: 0,
        peak_queue_depth: 2,
        acquired_writes: 4,
        total_queue_wait_micros: 5,
        committed_writes: 4,
        rolled_back_writes: 0,
    };
    assert_standard_observations(PRIVATE_USAGE_LIMIT_BYTES, healthy_queue);

    let leaked_queue = WriteCoordinatorSnapshot {
        current_queue_depth: 1,
        peak_queue_depth: healthy_queue.peak_queue_depth,
        acquired_writes: healthy_queue.acquired_writes,
        total_queue_wait_micros: healthy_queue.total_queue_wait_micros,
        committed_writes: healthy_queue.committed_writes,
        rolled_back_writes: healthy_queue.rolled_back_writes,
    };
    assert!(std::panic::catch_unwind(|| {
        assert_standard_observations(PRIVATE_USAGE_LIMIT_BYTES, leaked_queue)
    })
    .is_err());
    assert!(std::panic::catch_unwind(|| {
        assert_standard_observations(PRIVATE_USAGE_LIMIT_BYTES + 1, healthy_queue)
    })
    .is_err());
}

#[test]
fn startup_report_retains_raw_nanosecond_samples_and_release_schema() {
    let startup = vec![Duration::from_nanos(100); STARTUP_SAMPLE_COUNT];
    let finalization = vec![Duration::from_nanos(200); SAMPLE_COUNT];
    let report = startup_qualification_report(&startup, &finalization);

    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["suite"], "startup-and-concurrent-finalization");
    assert_eq!(
        report["metrics"]["startupWithoutMigration"]["samplesNs"]
            .as_array()
            .map(Vec::len),
        Some(STARTUP_SAMPLE_COUNT)
    );
    assert_eq!(
        report["metrics"]["finalization"]["samplesNs"]
            .as_array()
            .map(Vec::len),
        Some(SAMPLE_COUNT)
    );
    assert_eq!(
        report["metrics"]["finalization"]["coverage"],
        "production-request-finalization-service-terminal-transition"
    );
    assert_eq!(report["workloads"]["startup"]["migrationsIncluded"], false);
}

#[test]
fn mock_wrapper_contract_emits_reports_from_rust_builders() {
    let hot_request_logs = (100..140).map(Duration::from_nanos).collect::<Vec<_>>();
    let hot_change_events = (100..140).map(Duration::from_nanos).collect::<Vec<_>>();
    let startup = (100..115).map(Duration::from_nanos).collect::<Vec<_>>();
    let finalization = (200..240).map(Duration::from_nanos).collect::<Vec<_>>();
    let provenance = json!({
        "v2Commit": "dddddddddddddddddddddddddddddddddddddddd",
        "build": { "profile": "release", "locked": true },
        "measurementStartedAtUtc": "2026-07-21T00:20:00.0000000Z",
        "measurementCompletedAtUtc": "2026-07-21T00:30:00.0000000Z",
        "worktreeSnapshot": {
            "kind": "hashed-dirty-worktree",
            "trackedDiffSha256": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
            "untrackedContentSha256": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        },
    });
    let environment = json!({
        "cpuModel": "contract CPU",
        "logicalProcessors": 16,
        "installedMemoryBytes": 16_000_000_000_u64,
        "windowsCaption": "contract Windows",
        "windowsVersion": "10.0.1",
        "windowsBuild": "1",
        "activePowerScheme": "contract power scheme",
        "rustcVersion": "rustc contract",
        "cargoVersion": "cargo contract",
        "gitHead": "dddddddddddddddddddddddddddddddddddddddd",
        "worktreeDirty": true,
        "worktreeStatusSha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "antivirusProducts": [],
        "defenderRealTimeProtection": "unavailable",
        "windowsSearchService": "unavailable",
        "debugAssertions": false,
    });
    let write_queue = WriteCoordinatorSnapshot {
        current_queue_depth: 0,
        peak_queue_depth: 1,
        acquired_writes: 41,
        total_queue_wait_micros: 2,
        committed_writes: 41,
        rolled_back_writes: 0,
    };
    let standard = qualification_report(
        "standard",
        &[
            ("hotRequestLogs", &hot_request_logs),
            ("hotChangeEventsFirstPage", &hot_change_events),
        ],
        provenance.clone(),
        environment.clone(),
        json!({
            "metric": "PROCESS_MEMORY_COUNTERS_EX.PrivateUsage",
            "sampleIntervalMs": 10,
            "sampleCount": 4,
            "baselinePrivateUsageBytes": 100,
            "peakPrivateUsageBytes": 200,
            "peakPrivateUsageDeltaBytes": 100,
            "limitBytes": PRIVATE_USAGE_LIMIT_BYTES,
        }),
        write_queue,
        json!({}),
    );
    let mut startup_report = startup_qualification_report(&startup, &finalization);
    startup_report["provenance"] = provenance;
    startup_report["environment"] = environment;

    println!("PERSISTENCE_QUALIFICATION {standard}");
    println!("PERSISTENCE_QUALIFICATION {startup_report}");
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy)]
struct ProcessMemorySample {
    private_usage_bytes: u64,
    working_set_bytes: u64,
    peak_working_set_bytes: u64,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy)]
struct PrivateUsageObservation {
    sample_count: u64,
    baseline_private_usage_bytes: u64,
    peak_private_usage_bytes: u64,
    peak_private_usage_delta_bytes: u64,
    current_working_set_bytes: u64,
    peak_working_set_bytes: u64,
}

#[cfg(windows)]
impl PrivateUsageObservation {
    fn to_json(self) -> serde_json::Value {
        json!({
            "metric": "PROCESS_MEMORY_COUNTERS_EX.PrivateUsage",
            "sampleIntervalMs": 10,
            "sampleCount": self.sample_count,
            "baselinePrivateUsageBytes": self.baseline_private_usage_bytes,
            "peakPrivateUsageBytes": self.peak_private_usage_bytes,
            "peakPrivateUsageDeltaBytes": self.peak_private_usage_delta_bytes,
            "limitBytes": PRIVATE_USAGE_LIMIT_BYTES,
            "currentWorkingSetBytes": self.current_working_set_bytes,
            "peakWorkingSetBytes": self.peak_working_set_bytes,
        })
    }
}

#[cfg(windows)]
struct PrivateUsageSampler {
    baseline: ProcessMemorySample,
    samples: Arc<Mutex<Vec<ProcessMemorySample>>>,
    stop: Arc<AtomicBool>,
    join: JoinHandle<()>,
}

#[cfg(windows)]
impl PrivateUsageSampler {
    fn start() -> Result<Self, String> {
        let baseline = process_memory_sample()?;
        let samples = Arc::new(Mutex::new(vec![baseline]));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_samples = Arc::clone(&samples);
        let thread_stop = Arc::clone(&stop);
        let join = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(10));
                if let Ok(sample) = process_memory_sample() {
                    thread_samples
                        .lock()
                        .expect("memory samples mutex")
                        .push(sample);
                }
            }
        });
        Ok(Self {
            baseline,
            samples,
            stop,
            join,
        })
    }

    fn finish(self) -> Result<PrivateUsageObservation, String> {
        self.stop.store(true, Ordering::Release);
        self.join
            .join()
            .map_err(|_| "private usage sampler thread panicked".to_string())?;
        let final_sample = process_memory_sample()?;
        let mut samples = self
            .samples
            .lock()
            .map_err(|_| "private usage samples mutex poisoned".to_string())?;
        samples.push(final_sample);
        let peak_private_usage_bytes = samples
            .iter()
            .map(|sample| sample.private_usage_bytes)
            .max()
            .unwrap_or(self.baseline.private_usage_bytes);
        let peak_working_set_bytes = samples
            .iter()
            .map(|sample| sample.peak_working_set_bytes)
            .max()
            .unwrap_or(self.baseline.peak_working_set_bytes);
        Ok(PrivateUsageObservation {
            sample_count: samples.len() as u64,
            baseline_private_usage_bytes: self.baseline.private_usage_bytes,
            peak_private_usage_bytes,
            peak_private_usage_delta_bytes: peak_private_usage_bytes
                .saturating_sub(self.baseline.private_usage_bytes),
            current_working_set_bytes: final_sample.working_set_bytes,
            peak_working_set_bytes,
        })
    }
}

#[cfg(windows)]
fn process_memory_sample() -> Result<ProcessMemorySample, String> {
    let mut counters = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..PROCESS_MEMORY_COUNTERS_EX::default()
    };
    let succeeded = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>(),
            counters.cb,
        )
    };
    if succeeded == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(ProcessMemorySample {
        private_usage_bytes: counters.PrivateUsage as u64,
        working_set_bytes: counters.WorkingSetSize as u64,
        peak_working_set_bytes: counters.PeakWorkingSetSize as u64,
    })
}

#[cfg(windows)]
#[test]
fn windows_private_usage_sample_is_available() {
    let sample = process_memory_sample().expect("read current process memory counters");
    assert!(sample.private_usage_bytes > 0);
    assert!(sample.working_set_bytes > 0);
    assert!(sample.peak_working_set_bytes >= sample.working_set_bytes);
}

#[cfg(windows)]
#[test]
fn windows_private_usage_sampler_retains_ten_millisecond_observation_contract() {
    let sampler = PrivateUsageSampler::start().expect("start private usage sampler");
    std::thread::sleep(Duration::from_millis(25));
    let observation = sampler.finish().expect("finish private usage sampler");

    assert!(observation.sample_count >= 2);
    assert!(observation.peak_private_usage_bytes >= observation.baseline_private_usage_bytes);
    assert_eq!(
        observation.peak_private_usage_delta_bytes,
        observation
            .peak_private_usage_bytes
            .saturating_sub(observation.baseline_private_usage_bytes)
    );
    let json = observation.to_json();
    assert_eq!(json["metric"], "PROCESS_MEMORY_COUNTERS_EX.PrivateUsage");
    assert_eq!(json["sampleIntervalMs"], 10);
    assert_eq!(json["limitBytes"], 268435456_u64);
    assert_eq!(json["sampleCount"], observation.sample_count);
}

#[tokio::test]
async fn queue_observations_return_to_zero_and_are_serialized() {
    let root = tempfile::tempdir().expect("queue observation root");
    let path = root.path().join("queue-observation.sqlite3");
    let runtime = PersistenceRuntime::initialize_new(&path)
        .await
        .expect("initialize queue observation runtime");
    let handle = runtime.handle();
    let before = handle.write_metrics();
    handle
        .write(|write| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO settings (key, value, updated_at) VALUES ('queue-contract', 'ok', '1')",
                )
                .execute(write.connection())
                .await?;
                Ok(())
            })
        })
        .await
        .expect("queue observation write");
    let after = handle.write_metrics();

    assert_eq!(before.current_queue_depth, 0);
    assert_eq!(after.current_queue_depth, 0);
    assert_eq!(after.acquired_writes, before.acquired_writes + 1);
    assert_eq!(after.committed_writes, before.committed_writes + 1);
    let samples = vec![Duration::from_nanos(1)];
    let report = qualification_report(
        "queue-contract",
        &[("ordinaryWrite", &samples)],
        json!({ "baselineKind": "reconstructed-v0.3.1-source-baseline" }),
        json!({ "cpuModel": "contract" }),
        json!({
            "metric": "PROCESS_MEMORY_COUNTERS_EX.PrivateUsage",
            "sampleIntervalMs": 10,
            "baselinePrivateUsageBytes": 1,
            "peakPrivateUsageBytes": 1,
            "peakPrivateUsageDeltaBytes": 0,
            "limitBytes": 268435456_u64,
        }),
        after,
        json!({}),
    );
    assert_eq!(
        report["queues"]["writeCoordinator"]["currentDepth"],
        after.current_queue_depth
    );
    assert_eq!(
        report["queues"]["writeCoordinator"]["committedWrites"],
        after.committed_writes
    );
    runtime
        .close()
        .await
        .expect("close queue observation runtime");
}

fn next_measurement_id(prefix: &str) -> String {
    let sequence = NEXT_MEASUREMENT_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{sequence:08}")
}

struct StandardFixture {
    _root: tempfile::TempDir,
    path: PathBuf,
}

impl StandardFixture {
    async fn create() -> Self {
        let root = tempfile::tempdir().expect("performance fixture root");
        let path = root.path().join("relay-pool-v2.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("initialize V2 fixture");
        seed_standard_fixture(&runtime.handle()).await;
        runtime.close().await.expect("close persistence runtime");
        Self { _root: root, path }
    }
}

async fn seed_standard_fixture(handle: &PersistenceHandle) {
    handle
        .write(|write| {
            Box::pin(async move {
                for statement in fixture_statements() {
                    sqlx::query(statement).execute(write.connection()).await?;
                }
                Ok(())
            })
        })
        .await
        .expect("seed standard performance fixture");
}

fn fixture_statements() -> [&'static str; 7] {
    [
        r#"
        WITH digits(n) AS (VALUES(0),(1),(2),(3),(4),(5),(6),(7),(8),(9))
        INSERT INTO stations (
            id, name, station_type, website_url, api_base_url, enabled, priority,
            created_at, updated_at
        )
        SELECT printf('station-%03d', a.n * 10 + b.n),
               printf('Station %03d', a.n * 10 + b.n),
               'openai_compatible', 'https://example.invalid', 'https://example.invalid/v1',
               1, a.n * 10 + b.n, '2026-07-20T00:00:00Z', '2026-07-20T00:00:00Z'
        FROM digits a CROSS JOIN digits b
        "#,
        r#"
        WITH digits(n) AS (VALUES(0),(1),(2),(3),(4),(5),(6),(7),(8),(9))
        INSERT INTO secrets (
            id, scope, owner_id, kind, masked_value, ciphertext, nonce, created_at, updated_at
        )
        SELECT printf('secret-%04d', a.n * 100 + b.n * 10 + c.n),
               'station_key', printf('key-%04d', a.n * 100 + b.n * 10 + c.n),
               'api_key', '****test', X'01020304', X'010203040506070809101112',
               '2026-07-20T00:00:00Z', '2026-07-20T00:00:00Z'
        FROM digits a CROSS JOIN digits b CROSS JOIN digits c
        "#,
        r#"
        WITH digits(n) AS (VALUES(0),(1),(2),(3),(4),(5),(6),(7),(8),(9))
        INSERT INTO station_keys (
            id, station_id, name, api_key_secret_id, enabled, priority, routing_order,
            created_at, updated_at
        )
        SELECT printf('key-%04d', a.n * 100 + b.n * 10 + c.n),
               printf('station-%03d', (a.n * 100 + b.n * 10 + c.n) % 100),
               printf('Key %04d', a.n * 100 + b.n * 10 + c.n),
               printf('secret-%04d', a.n * 100 + b.n * 10 + c.n), 1,
               a.n * 100 + b.n * 10 + c.n,
               a.n * 100 + b.n * 10 + c.n,
               '2026-07-20T00:00:00Z', '2026-07-20T00:00:00Z'
        FROM digits a CROSS JOIN digits b CROSS JOIN digits c
        "#,
        r#"
        INSERT INTO pricing_rules (
            id, station_id, station_key_id, model, input_price, output_price,
            fixed_price, rate_multiplier, currency, unit, price_type,
            base_price_source, normalization_status, source, confidence, enabled,
            collected_at, valid_from, created_at, updated_at
        ) VALUES (
            'pricing-qualification', 'station-000', 'key-0000', 'gpt-qualification',
            1.25, 5.0, 0.01, 1.1, 'CNY', '1M tokens', 'token',
            'qualification-fixture', 'normalized', 'qualification', 1.0, 1,
            '2026-07-20T00:00:00Z', '2026-07-20T00:00:00Z',
            '2026-07-20T00:00:00Z', '2026-07-20T00:00:00Z'
        )
        "#,
        r#"
        WITH digits(n) AS (VALUES(0),(1),(2),(3),(4),(5),(6),(7),(8),(9))
        INSERT INTO request_logs (
            id, request_id, started_at, finished_at, duration_ms, method, path,
            endpoint, model, stream, status, lifecycle_status, station_key_id,
            station_id, upstream_base_url, fallback_count, route_policy, route_reason,
            rejected_candidates_json, body_bytes, attempt_count, route_wait_ms,
            upstream_headers_ms, attempts_json, completion_source, prompt_tokens,
            completion_tokens, total_tokens, cache_creation_tokens, cache_read_tokens,
            reasoning_effort, first_token_ms, terminal_kind, terminal_code,
            terminal_detail, protocol_completed, delivery_terminal,
            selected_attempt_ordinal, terminal_at_ms, billing_mode,
            estimated_input_cost, estimated_output_cost, estimated_total_cost,
            base_input_cost, base_output_cost, base_fixed_cost, base_total_cost,
            cost_currency, pricing_rule_id, pricing_source, cost_status,
            group_binding_id, normalization_status, balance_scope,
            economic_context_json, created_at
        )
        SELECT printf('request-log-%05d', a.n * 1000 + b.n * 100 + c.n * 10 + d.n),
               printf('request-%05d', a.n * 1000 + b.n * 100 + c.n * 10 + d.n),
               '2026-07-20T00:00:00Z', '2026-07-20T00:00:01Z', 1000,
               'POST', '/v1/responses', '/v1/responses', 'gpt-qualification', 1,
               'success', 'completed',
               printf('key-%04d', (a.n * 1000 + b.n * 100 + c.n * 10 + d.n) % 1000),
               printf('station-%03d', (a.n * 1000 + b.n * 100 + c.n * 10 + d.n) % 100),
               'https://example.invalid/v1', 0, 'cost_stable_first',
               'qualification representative route', '[]', 2048, 1, 2, 40,
               '[{"ordinal":0,"terminal":"success"}]', 'body_completed',
               1000, 200, 1200, 50, 100, 'medium', 75,
               'success', 'ok', 'qualification terminal', 1, 'body_completed', 0, 1000,
               'estimated', 0.00125, 0.001, 0.00225, 0.001, 0.0008, 0.0001,
               0.0019, 'CNY', 'pricing-qualification', 'qualification', 'complete',
               'group-qualification', 'normalized', 'station_key',
               '{"rateMultiplier":1.1,"source":"qualification"}',
               printf('%05d', a.n * 1000 + b.n * 100 + c.n * 10 + d.n)
        FROM digits a CROSS JOIN digits b CROSS JOIN digits c CROSS JOIN digits d
        "#,
        r#"
        WITH digits(n) AS (VALUES(0),(1),(2),(3),(4),(5),(6),(7),(8),(9))
        INSERT INTO change_events (
            id, severity, event_type, status, title, message, object_type,
            object_id, station_id, station_key_id, pricing_rule_id, request_log_id,
            old_value_json, new_value_json, impact_json, dedupe_key, source,
            detected_at, resolved_at, created_at, updated_at
        )
        SELECT printf('evidence-%06d', a.n * 10000 + b.n * 1000 + c.n * 100 + d.n * 10 + e.n),
               'info', 'qualification_evidence', 'unread', 'Qualification evidence',
               'Redacted synthetic evidence', 'persistence_fixture',
               printf('object-%06d', a.n * 10000 + b.n * 1000 + c.n * 100 + d.n * 10 + e.n),
               printf('station-%03d', (a.n * 10000 + b.n * 1000 + c.n * 100 + d.n * 10 + e.n) % 100),
               printf('key-%04d', (a.n * 10000 + b.n * 1000 + c.n * 100 + d.n * 10 + e.n) % 1000),
               'pricing-qualification',
               printf('request-log-%05d', (a.n * 10000 + b.n * 1000 + c.n * 100 + d.n * 10 + e.n) % 10000),
               '{"status":"before"}', '{"status":"after"}',
               '{"scope":"qualification","material":true}',
               printf('evidence-dedupe-%06d', a.n * 10000 + b.n * 1000 + c.n * 100 + d.n * 10 + e.n),
               'qualification', '2026-07-20T00:00:00Z',
               NULL,
               printf('%06d', a.n * 10000 + b.n * 1000 + c.n * 100 + d.n * 10 + e.n),
               printf('%06d', a.n * 10000 + b.n * 1000 + c.n * 100 + d.n * 10 + e.n)
        FROM digits a CROSS JOIN digits b CROSS JOIN digits c CROSS JOIN digits d CROSS JOIN digits e
        "#,
        r#"
        CREATE TABLE persistence_qualification_writes (
            id TEXT PRIMARY KEY,
            created_at TEXT NOT NULL
        )
        "#,
    ]
}
