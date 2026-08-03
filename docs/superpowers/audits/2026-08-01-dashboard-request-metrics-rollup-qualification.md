# Dashboard Request Metrics Rollup Qualification

Status: passed

Date: 2026-08-02

Branch: `codex/dashboard-request-metrics-read-model`

Worktree: `E:\Dev\Projects\relay-pool-desktop-dashboard-metrics`

## Scope

This audit closes the rollup branch triggered by the direct-aggregation failure recorded in `2026-08-01-dashboard-request-metrics-baseline.md`.

Implemented rollup changes:

- Added schema 23 rollup tables for request metric counts, cost classifications, and per-currency cost totals.
- Maintained rollups from `RequestLogStore::start_request`, `RequestLogStore::finish_request`, `RequestLogStore::clear`, and `RequestOutcomeStore::insert_request_cost_aggregate`.
- Kept rollups rebuildable: `DashboardMetricsQuery` checks whether rollup request count matches canonical request logs and repairs from durable facts before reading.
- Used pure `UPDATE` for negative deltas and `UPSERT` for positive deltas, avoiding SQLite check-constraint failures on decrement paths.
- Kept portable migration fail-closed behavior by treating Dashboard rollups and helper indexes as derived objects rather than portable user facts.
- Removed the discovered persistence dependency cycle: `dashboard_metrics_rollup -> request_outcome_store -> dashboard_metrics_rollup`. `RequestCostAggregateWrite` now lives in the neutral `request_cost_write` write-model module; the old outcome-store export remains as a compatibility facade.

## Performance qualification

Command:

```powershell
python scripts/dashboard_metrics_perf_probe.py --rows 100000 500000 --warm-samples 10 --cold-samples 3 --writer-samples 50
```

Result:

| Rows | Live warm p95 | Cumulative warm p95 | Writer p95 regression | Writer busy | Reader busy | Gate |
|---:|---:|---:|---:|---:|---:|---|
| 100,000 | 0.302 ms | 0.076 ms | -38.349% | 0 | 0 | Passed |
| 500,000 | 0.734 ms | 0.115 ms | 6.156% | 0 | 0 | Passed |

`EXPLAIN QUERY PLAN` evidence:

- Period rollup: `SEARCH dashboard_request_metric_rollups USING INDEX idx_dashboard_request_metric_rollups_range`.
- Cost count rollup: `SEARCH dashboard_request_cost_rollups USING INDEX idx_dashboard_request_cost_rollups_range`.
- Cost total rollup: `SEARCH dashboard_request_cost_totals_rollups USING INDEX idx_dashboard_request_cost_totals_rollups_range`.

## Verification commands

Passed:

- `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`
- `cargo check --locked --manifest-path src-tauri/Cargo.toml`
- `pnpm exec tsc --noEmit`
- `pnpm exec vitest run src/features/dashboard/dashboardRequestMetricsViewModel.test.ts`
- `pnpm test` — 75 files / 254 tests passed
- `pnpm lint` — exited 0 with existing warnings only
- `pnpm test:contracts`
- `pnpm build`
- `node scripts/dashboard-performance-metrics.test.mjs; node scripts/dashboard-request-count-source.test.mjs; node scripts/dashboard-query-service.test.mjs; node scripts/dashboard-request-cost-format.test.mjs; node scripts/dashboard-recent-usage-layout.test.mjs; node scripts/dashboard-token-value-color.test.mjs; node scripts/dashboard-recent-usage-key-label.test.mjs; node scripts/dashboard-station-usage-cards.test.mjs; node scripts/query-services-boundary.test.mjs`
- `python -m py_compile scripts/dashboard_metrics_perf_probe.py`
- `git diff --check` — exited 0; Git reported line-ending normalization warnings only
- `cargo test --locked --manifest-path src-tauri/Cargo.toml persistence::stores::request_log_store::v2_tests::request_terminal_uses_compare_and_set -- --nocapture`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml schema_21_quarantines_removed_collector_providers_without_deleting_assets -- --nocapture`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml schema_reader::tests::reader_accepts_current_schema_and_reads_with_fixed_selects -- --nocapture`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml schema_reader::tests::reader_rejects_unknown_schema_objects_columns_and_spoofed_versions -- --nocapture`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml portable_export_package_writes_self_verified_age_file -- --nocapture`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml trusted_schema_fingerprint_matches_fixture -- --nocapture`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml station_endpoint_change_is_atomic_and_matches_v1_contract_boundary -- --nocapture`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml services::data_store::generation_upgrade::tests::schema_16_generation_two_runs_secret_baseline_before_opening_runtime -- --nocapture`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_outcome_persistence`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_architecture persistence_v2_dependency_edges_match_the_boundary_manifest`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_collectors stale_revision_and_unsupported_model_events_have_no_side_effects -- --nocapture`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_runtime readable_but_not_writable_opens_in_inspection_only_mode -- --nocapture`

Full cargo run:

- The final `cargo test --locked --manifest-path src-tauri/Cargo.toml` run executed 757 lib tests: 756 passed; 1 failed due to a transient Windows temp SQLite WAL file lock (`os error 32`) in `schema_16_generation_two_runs_secret_baseline_before_opening_runtime`.
- The failing WAL test passed immediately when rerun in isolation. The direct-module `routing_outcome_persistence` test also passed after its test-only module boundary was updated for `request_cost_write`, and the persistence dependency-manifest test passed after the cycle repair.
- During the full-suite follow-up, two stale test contracts were corrected: collector model facts are intentionally not persisted for unsupported adapters, and the inspection-only fixture now explicitly constructs the intended `0.3.1` binary compatibility instead of using the current `0.4.0` package binary. Both tests passed in isolation and in the subsequent full run before the unrelated WAL lock.

No plan-scope verification remains intentionally skipped. The only non-green signal observed was the one full-cargo transient Windows WAL file-lock failure noted above, and the same test passed on immediate rerun.
