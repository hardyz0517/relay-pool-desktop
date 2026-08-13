# Status Monitoring V2 Migration Manifest

Status: Task 6 initial schema/backfill verified
Date: 2026-07-29
Migration: `src-tauri/src/persistence/migrations/0010_status_monitoring_v2.sql`

## Version

- Previous schema: 8
- New schema: 9
- `current_binary_compatibility()` updated to readable `1..=9`, writable `{9}`.
- The next available migration number in this worktree was `0009`; no provider-drafts migration exists in this worktree.

## Channel monitor definition evolution

`channel_monitors` now has V2 definition/scheduling fields:

- `protocol_kind`
- `client_profile_id`
- `client_profile_version`
- `primary_model`
- `fallback_models_v2_json`
- retry policy fields
- risk daily probe budget
- health writeback policy fields
- `attempt_timeout_ms`
- `execution_timeout_ms`
- `schedule_revision`
- integer `next_due_at_ms`

Legacy `fallback_models_json[0]` is backfilled to `primary_model`; distinct remaining values are backfilled to `fallback_models_v2_json` with a maximum of 3 entries. Legacy `next_run_at` is copied into `next_due_at_ms`.

## New tables

- `channel_monitor_executions`
- `channel_monitor_attempts`
- `channel_monitor_target_results`
- `channel_monitor_bucket_rollups`
- `channel_monitor_rollup_dirty_ranges`
- `station_key_health_observations`
- `channel_monitor_probe_budget_usage`

## Legacy import behavior

Each legacy `channel_monitor_runs` row is imported as:

- one `channel_monitor_executions` row with `trigger_kind='legacy_import'`;
- one `channel_monitor_attempts` row with `request_profile_hash='legacy-http-only'`;
- one `channel_monitor_target_results` row with `semantic_confidence='legacy_http_only'`;
- one dirty rollup range.

Legacy rows are intentionally not imported into `station_key_health_observations`, because HTTP-only historical results are display evidence, not authoritative health observations.

## Verified gates

- `cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_migration -- --nocapture` => passed, 3 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --test persistence_pricing_monitoring -- --nocapture` => passed, 4 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade -- --nocapture` => passed, 22 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade_recovery -- --nocapture` => passed, 15 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --test persistence_runtime -- --nocapture` => passed, 15 tests.
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` => passed.
- `cargo check --manifest-path src-tauri/Cargo.toml` => passed.

## Known follow-up into Task 7+

- Repository ports and transaction boundaries are not implemented yet.
- Production write path still uses the legacy monitoring store until Task 7+ moves consumers.
- Rollup rebuild implementation is deferred to the repository/read-model tasks; migration only creates rollup storage and dirty ranges.
