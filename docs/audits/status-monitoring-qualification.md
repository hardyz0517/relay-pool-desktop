# Status Monitoring V2 Qualification

Status: local deterministic qualification passed on `codex/status-monitoring-refactor`; live provider and signed release qualification require explicit authorization/secrets.

Generated for branch: `codex/status-monitoring-refactor`

## Automated evidence

The following checks are designed to be safe for local CI/dev execution and do not use provider secrets:

- `cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_faults -- --nocapture`
  - transaction rollback covers attempt append, target finalization, health observation, execution finalization, rollup dirty range marking, and budget reservation.
  - commit-outcome-unknown replay is idempotent for monitor budget reservations and health observations.
  - invalid target ownership is a permanent finalization fault and leaves no partial target or fake execution summary.
  - hard-kill/startup recovery changes queued/running executions to `interrupted`, does not synthesize failed attempts, does not replay network work, and does not refund already reserved probe budget.
- `cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_concurrency -- --nocapture`
  - 100 due monitors are handled with bounded scheduler queue pressure and no catch-up storm.
  - same monitor/station-key manual storms single-flight to one execution.
  - 100 unique-key executions drain under global/station/key permit limits with no deadlock or leaked active executions.
- Existing protocol/orchestrator evidence:
  - `monitoring_adapter_contracts`
  - `monitoring_profile_golden`
  - `monitoring_transport`
  - `monitoring_orchestrator`
  - These cover provider adapter request shapes, profile versioning, semantic validation, retry/fallback recording, and fake-200/content mismatch rejection.
- Existing persistence/read evidence:
  - `monitoring_persistence`
  - `monitoring_write_path`
  - `monitoring_scheduler`
  - `monitoring_read_model`
  - `monitoring_buckets_retention`
  - These cover V2 fact persistence, nearest-due scheduling, backend-derived recent/bucket windows, retention repair, and the read-only legacy observation boundary.

## Scripts

- `scripts/run-monitoring-soak.ps1`
  - Runs deterministic local monitoring suites repeatedly for the requested duration and writes `docs/audits/status-monitoring-soak-latest.json`.
  - Use `-Quick` for a short smoke run.
- `scripts/verify-monitoring-live.ps1`
  - Fails closed unless `-AuthorizeLiveProviderProbe` is provided.
  - Requires provider secrets from environment/SecretManager and never prints secret values.
  - Produces a sanitized authorization/evidence checklist JSON.
- `scripts/verify-monitoring-db.ps1`
  - Read-only SQLite verification for V2 tables, stale running executions, legacy `channel_monitor_runs` row presence, and disallowed legacy-http-only authoritative writeback.
- `scripts/verify-monitoring-read-model-performance.ps1`
  - Seeds a fresh migrated SQLite database with a large deterministic monitoring fixture and verifies the workspace read model plus scheduler lag against release-gate thresholds.

## Latest local validation

Run date: 2026-07-30 Asia/Shanghai.

- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` passed.
- `pnpm.cmd architecture:fixtures`, `architecture:typescript`, `architecture:commands`, and `architecture:security` passed.
- `pnpm.cmd test:contracts` passed.
- `pnpm.cmd test` passed: 54 files / 184 tests.
- `pnpm.cmd lint` passed with 0 errors and 79 warnings.
- `pnpm.cmd build` passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --nocapture` passed: 659 tests.
- Monitoring integration suites passed:
  - `monitoring_domain`: 7 tests.
  - `monitoring_adapter_contracts`: 18 tests.
  - `monitoring_profile_golden`: 4 tests.
  - `monitoring_migration`: 3 tests.
  - `monitoring_persistence`: 10 tests.
  - `monitoring_orchestrator`: 11 tests.
  - `monitoring_transport`: 2 tests.
  - `monitoring_scheduler`: 6 tests.
  - `station_key_health_transitions`: 8 tests.
  - `monitoring_buckets_retention`: 8 tests.
  - `monitoring_read_model`: 3 tests.
  - `monitoring_execution_integration`: 6 tests.
  - `monitoring_faults`: 7 tests.
  - `monitoring_concurrency`: 2 tests.
- `cargo check --manifest-path src-tauri/Cargo.toml` passed.
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-monitoring-soak.ps1 -DurationMinutes 1 -Quick` passed and wrote `docs/audits/status-monitoring-soak-latest.json`.
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-monitoring-soak.ps1 -DurationMinutes 60 -OutputPath docs/audits/status-monitoring-soak-60m-latest.json` passed and wrote `docs/audits/status-monitoring-soak-60m-latest.json`.
  - Started: 2026-07-30T01:26:35.9157847+08:00.
  - Finished: 2026-07-30T02:26:38.8829128+08:00.
  - Iterations: 985.
  - Failures: 0.
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-monitoring-soak.ps1 -DurationMinutes 60 -ReleaseBuild -MixedProviderWorkload -OutputPath docs/audits/status-monitoring-soak-release-mixed-60m-latest.json` passed and wrote `docs/audits/status-monitoring-soak-release-mixed-60m-latest.json`.
  - Started: 2026-07-30T02:32:28.0905333+08:00.
  - Finished: 2026-07-30T03:32:29.3902925+08:00.
  - Build profile: release.
  - Workload: mixed provider / stream / retry / fallback / missing.
  - Iterations: 513.
  - Failures: 0.
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-monitoring-db.ps1 ...` passed against a fresh migrated v10 temporary database and wrote `docs/audits/status-monitoring-db-latest.json`.
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-monitoring-read-model-performance.ps1 -MonitorRows 500 -TargetResults 500000 -Attempts 100000 -Samples 20 -WorkspaceP95LimitMs 250 -SchedulerLagP95LimitMs 2000` passed and wrote `docs/audits/status-monitoring-read-model-performance-latest.json`.
  - Fixture: 500 workspace rows, 500,000 `channel_monitor_target_results`, 100,000 attempts.
  - Workspace p95: 134.93 ms against the 250 ms gate.
  - Scheduler lag p95: 16 ms against the 2,000 ms gate.
  - Recent target-result query plan uses `idx_channel_monitor_target_results_monitor_station_finished`.
- `scripts/verify-monitoring-live.ps1` was checked without authorization and failed closed as designed.
- `pnpm.cmd tauri:build` completed Vite, release Rust compilation, application build, and NSIS bundle generation, then failed the final signing step because `TAURI_SIGNING_PRIVATE_KEY` is not set while a public key is configured. The unsigned local bundle path was generated under `src-tauri/target/release/bundle/nsis/`, which remains ignored build output.

## Live-provider matrix

Live probes are intentionally not run by default. They require account-owner authorization because even low-frequency synthetic probes can affect upstream risk systems.

Required live matrix after authorization:

- OpenAI standard profile.
- Anthropic standard profile.
- Gemini standard profile.
- xAI/Grok standard profile.
- Generic OpenAI-compatible standard profile.
- Codex CLI compatibility profile: low-frequency acceptance and semantic-content verification.
- Claude Code compatibility profile: low-frequency acceptance and semantic-content verification.
- Gemini CLI compatibility profile: low-frequency acceptance and semantic-content verification.

`grok_cli_compat` remains disabled until separately verified; it must not be enabled by this qualification run.

For each live execution, evidence must include:

- user-visible execution id/status/latency;
- runtime sanitized diagnostics;
- `channel_monitor_executions`;
- `channel_monitor_target_results`;
- `channel_monitor_attempts`;
- `station_key_health_observations`.

## Long-running and release qualification

The following are release gates, not default dev-turn checks:

- signed updater artifact generation with `TAURI_SIGNING_PRIVATE_KEY`;
- fresh signed Windows install;
- upgrade from previous formal release;
- quit/restart;
- sleep/resume.

Until those are run, the implementation can be considered locally qualified but not release-qualified.
