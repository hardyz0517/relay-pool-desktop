#[cfg(test)]
mod persistence_v1_performance_probe {
    use super::*;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    const RELEASE_COMMIT: &str = "54751559aed8f3f7c159e322bc7bbcc71d993204";
    const RELEASED_FIXTURE_SHA256: &str =
        "ad1f159cd6feabbb7d9bb4d6a37bf4fbc979f98eab03a42f402eb6fa863f34c9";
    const SAMPLE_COUNT: usize = 40;
    const STARTUP_SAMPLE_COUNT: usize = 15;

    #[test]
    fn reconstructed_v031_baseline() {
        let released_fixture = required_path("PERSISTENCE_V1_RELEASED_FIXTURE");
        let output = required_path("PERSISTENCE_V1_BASELINE_OUTPUT");
        let probe_sha256 = std::env::var("PERSISTENCE_V1_BENCHMARK_PROBE_SHA256")
            .expect("benchmark probe SHA-256");
        let environment: serde_json::Value = serde_json::from_str(
            &std::env::var("PERSISTENCE_V1_ENVIRONMENT_JSON").expect("environment JSON"),
        )
        .expect("valid environment JSON");

        assert_eq!(sha256_file(&released_fixture), RELEASED_FIXTURE_SHA256);
        assert!(
            probe_sha256.len() == 64 && probe_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        );

        let root = unique_root();
        let data_dir = root.join("data");
        fs::create_dir_all(&data_dir).expect("create reconstructed V1 data directory");
        let database_path = data_dir.join(DATABASE_FILE);
        fs::copy(&released_fixture, &database_path).expect("copy released V1 fixture");
        seed_normalized_workload(&database_path);
        let derived_fixture_sha256 = sha256_file(&database_path);

        let startup_without_migration = sample_without_warmup(STARTUP_SAMPLE_COUNT, || {
            let started = Instant::now();
            let database =
                AppDatabase::initialize_existing_at(data_dir.clone(), data_dir.clone(), None)
                    .expect("open current v0.3.1 database");
            let elapsed = started.elapsed();
            drop(database);
            elapsed
        });

        let database =
            AppDatabase::initialize_existing_at(data_dir.clone(), data_dir.clone(), None)
                .expect("open V1 hot-read fixture");
        let hot_request_logs = sample(SAMPLE_COUNT, || {
            let started = Instant::now();
            let rows = database.list_request_logs().expect("V1 request logs");
            assert_eq!(rows.len(), 500);
            started.elapsed()
        });
        let hot_change_events = sample(SAMPLE_COUNT, || {
            let started = Instant::now();
            let mut rows = normalized_change_event_page(&database);
            assert_eq!(rows.len(), 201, "normalized V1 query must fetch limit + 1");
            rows.truncate(200);
            assert_eq!(rows.len(), 200);
            started.elapsed()
        });
        drop(database);

        let report = json!({
            "schemaVersion": 1,
            "baselineKind": "reconstructed-v0.3.1-source-baseline",
            "provenance": {
                "releaseCommit": RELEASE_COMMIT,
                "releasedFixtureSha256": RELEASED_FIXTURE_SHA256,
                "derivedFixtureSha256": derived_fixture_sha256,
                "benchmarkProbeSha256": probe_sha256,
            },
            "build": {
                "profile": "release",
                "locked": true,
                "source": "detached-v0.3.1-tag-worktree-with-appended-test-probe",
            },
            "environment": environment,
            "standardFixture": {
                "stations": 100,
                "stationKeys": 1_000,
                "requestLogs": 10_000,
                "changeEvents": 100_000,
            },
            "workloads": {
                "requestLogs": {
                    "rows": 500,
                    "projection": "v0.3.1-production-full-row-representative-economics-attempt-model",
                },
                "changeEvents": {
                    "queryLimit": 201,
                    "returnedRows": 200,
                    "projection": "v0.3.1-production-full-row-representative-associated-fields",
                    "contract": "normalized-first-page-not-v0.3.1-public-api",
                },
                "startup": { "migrationsIncluded": false },
            },
            "metrics": {
                "hotRequestLogs": { "samplesNs": duration_ns(&hot_request_logs) },
                "hotChangeEventsFirstPage": { "samplesNs": duration_ns(&hot_change_events) },
                "startupWithoutMigration": { "samplesNs": duration_ns(&startup_without_migration) },
            },
        });
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).expect("create baseline output directory");
        }
        fs::write(
            &output,
            serde_json::to_vec_pretty(&report).expect("serialize V1 baseline"),
        )
        .expect("write reconstructed V1 baseline");
        fs::remove_dir_all(&root).expect("remove reconstructed V1 fixture directory");
    }

    fn required_path(name: &str) -> PathBuf {
        PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("missing {name}")))
    }

    fn unique_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "relay-pool-v031-performance-{}-{nonce}",
            std::process::id()
        ))
    }

    fn sample(count: usize, mut operation: impl FnMut() -> Duration) -> Vec<Duration> {
        operation();
        (0..count).map(|_| operation()).collect()
    }

    fn sample_without_warmup(
        count: usize,
        mut operation: impl FnMut() -> Duration,
    ) -> Vec<Duration> {
        (0..count).map(|_| operation()).collect()
    }

    fn duration_ns(samples: &[Duration]) -> Vec<u64> {
        samples
            .iter()
            .map(|sample| u64::try_from(sample.as_nanos()).expect("duration fits u64"))
            .collect()
    }

    fn sha256_file(path: &Path) -> String {
        let bytes = fs::read(path).expect("read SHA-256 input");
        format!("{:x}", Sha256::digest(bytes))
    }

    fn normalized_change_event_page(database: &AppDatabase) -> Vec<ChangeEvent> {
        let connection = database.connection().expect("V1 connection");
        let mut statement = connection
            .prepare(
                "SELECT change_events.id, change_events.severity, change_events.event_type, change_events.status,
                        change_events.title, change_events.message, change_events.object_type, change_events.object_id,
                        change_events.station_id, stations.name AS station_name,
                        change_events.station_key_id, change_events.pricing_rule_id, change_events.request_log_id,
                        change_events.old_value_json, change_events.new_value_json, change_events.impact_json,
                        change_events.dedupe_key, change_events.source,
                        change_events.detected_at, change_events.resolved_at, change_events.created_at, change_events.updated_at
                   FROM change_events
                   LEFT JOIN stations ON stations.id = change_events.station_id
                  WHERE NOT (
                        change_events.event_type IN ('model_added', 'model_removed')
                        AND COALESCE(stations.station_type, '') = 'newapi'
                     )
                  ORDER BY change_events.updated_at DESC, change_events.detected_at DESC
                  LIMIT 201",
            )
            .expect("prepare normalized V1 change-event page");
        statement
            .query_map([], row_to_change_event)
            .expect("query normalized V1 change-event page")
            .collect::<Result<Vec<_>, _>>()
            .expect("map normalized V1 change-event page")
    }

    fn seed_normalized_workload(database_path: &Path) {
        let connection =
            Connection::open(database_path).expect("open copied V1 fixture for seeding");
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;
                BEGIN IMMEDIATE;
                DELETE FROM request_logs;
                DELETE FROM change_events;
                DELETE FROM pricing_rules;
                DELETE FROM station_keys;
                DELETE FROM stations;

                WITH digits(n) AS (VALUES(0),(1),(2),(3),(4),(5),(6),(7),(8),(9))
                INSERT INTO stations (
                    id, name, station_type, website_url, api_base_url, api_key,
                    enabled, priority, created_at, updated_at
                )
                SELECT printf('station-%03d', a.n * 10 + b.n),
                       printf('Station %03d', a.n * 10 + b.n),
                       'openai_compatible', 'https://example.invalid',
                       'https://example.invalid/v1', '', 1, a.n * 10 + b.n,
                       '2026-07-20T00:00:00Z', '2026-07-20T00:00:00Z'
                FROM digits a CROSS JOIN digits b;

                WITH digits(n) AS (VALUES(0),(1),(2),(3),(4),(5),(6),(7),(8),(9))
                INSERT INTO station_keys (
                    id, station_id, name, api_key, enabled, priority, routing_order,
                    created_at, updated_at
                )
                SELECT printf('key-%04d', a.n * 100 + b.n * 10 + c.n),
                       printf('station-%03d', (a.n * 100 + b.n * 10 + c.n) % 100),
                       printf('Key %04d', a.n * 100 + b.n * 10 + c.n), '', 1,
                       a.n * 100 + b.n * 10 + c.n,
                       a.n * 100 + b.n * 10 + c.n,
                       '2026-07-20T00:00:00Z', '2026-07-20T00:00:00Z'
                FROM digits a CROSS JOIN digits b CROSS JOIN digits c;

                INSERT INTO pricing_rules (
                    id, station_id, model, input_price, output_price, fixed_price,
                    currency, unit, price_type, source, confidence, enabled,
                    collected_at, created_at, updated_at
                ) VALUES (
                    'pricing-qualification', 'station-000', 'gpt-qualification',
                    1.25, 5.0, 0.01, 'CNY', '1M tokens', 'token',
                    'qualification', 1.0, 1, '2026-07-20T00:00:00Z',
                    '2026-07-20T00:00:00Z', '2026-07-20T00:00:00Z'
                );

                WITH digits(n) AS (VALUES(0),(1),(2),(3),(4),(5),(6),(7),(8),(9))
                INSERT INTO request_logs (
                    id, request_id, started_at, finished_at, duration_ms, method, path,
                    model, stream, status, lifecycle_status, station_key_id, station_id,
                    upstream_base_url, fallback_count, route_policy, route_reason,
                    rejected_candidates_json, body_bytes, attempt_count, route_wait_ms,
                    upstream_headers_ms, attempts_json, completion_source, prompt_tokens,
                    completion_tokens, total_tokens, cache_creation_tokens, cache_read_tokens,
                    reasoning_effort, first_token_ms, billing_mode, estimated_input_cost,
                    estimated_output_cost, estimated_total_cost, base_input_cost,
                    base_output_cost, base_fixed_cost, base_total_cost, cost_currency,
                    pricing_rule_id, pricing_source, cost_status, group_binding_id,
                    normalization_status, balance_scope, economic_context_json, created_at
                )
                SELECT printf('request-log-%05d', a.n * 1000 + b.n * 100 + c.n * 10 + d.n),
                       printf('request-%05d', a.n * 1000 + b.n * 100 + c.n * 10 + d.n),
                       '2026-07-20T00:00:00Z', '2026-07-20T00:00:01Z', 1000,
                       'POST', '/v1/responses', 'gpt-qualification', 1, 'success', 'completed',
                       printf('key-%04d', (a.n * 1000 + b.n * 100 + c.n * 10 + d.n) % 1000),
                       printf('station-%03d', (a.n * 1000 + b.n * 100 + c.n * 10 + d.n) % 100),
                       'https://example.invalid/v1', 0, 'cost_stable_first',
                       'qualification representative route', '[]', 2048, 1, 2, 40,
                       '[{"ordinal":0,"terminal":"success"}]', 'body_completed',
                       1000, 200, 1200, 50, 100, 'medium', 75,
                       'estimated', 0.00125, 0.001, 0.00225, 0.001, 0.0008,
                       0.0001, 0.0019, 'CNY', 'pricing-qualification', 'qualification',
                       'complete', 'group-qualification', 'normalized', 'station_key',
                       '{"rateMultiplier":1.1,"source":"qualification"}',
                       printf('%05d', a.n * 1000 + b.n * 100 + c.n * 10 + d.n)
                FROM digits a CROSS JOIN digits b CROSS JOIN digits c CROSS JOIN digits d;

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
                FROM digits a CROSS JOIN digits b CROSS JOIN digits c CROSS JOIN digits d CROSS JOIN digits e;
                COMMIT;
                PRAGMA wal_checkpoint(TRUNCATE);
                VACUUM;
                "#,
            )
            .expect("seed normalized V1 workload");
        for (table, expected) in [
            ("stations", 100_i64),
            ("station_keys", 1_000_i64),
            ("request_logs", 10_000_i64),
            ("change_events", 100_000_i64),
        ] {
            let actual: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("read normalized V1 workload count");
            assert_eq!(actual, expected, "normalized V1 workload table {table}");
        }
    }
}
