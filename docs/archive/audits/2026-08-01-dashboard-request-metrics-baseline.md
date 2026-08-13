# Dashboard Request Metrics Baseline

Status: baseline + direct-aggregation failure evidence; superseded by rollup qualification

Date: 2026-08-01

Branch: `codex/dashboard-request-metrics-read-model`

Baseline revision: `f403f8d3d07ad5da1b00e8bca28c2290775cc2eb`

## Existing Ownership

- `list_request_logs` is capped at 500 rows by the command adapter.
- `DashboardPage.tsx` scans that bounded array for today counts, token totals, average duration, RPM, TPM, and cost summaries.
- Request detail and the five-row recent usage list are valid bounded-log consumers and remain in scope.
- Canonical request start/finalization writes already exist in `RequestLogStore`.
- Request-level actual cost is owned by `routing_request_cost_aggregates`; legacy estimated/base cost fields are not authoritative cumulative facts.
- Latest schema at baseline is 21. Migration 22 is therefore the next available number.

## Frozen Decisions

- Live recent/today metrics use a bounded backend read model and refresh every two seconds only while the proxy is running.
- Cumulative metrics use a separate backend read model, load on page entry, and refresh at most every 30 seconds while running.
- Each snapshot is internally consistent. Live and cumulative snapshots do not claim a shared capture time.
- Request-level `usage_status` is persisted. Ambiguous historical rows remain `unknown_legacy`.
- Malformed historical timestamps remain null, are excluded from metric ranges, and are reported without blocking startup.
- Corrupt cost aggregates produce partial totals with `cost_totals_complete = false`; raw JSON is never returned.
- Dashboard displays authoritative actual cost only. The legacy base-cost comparison is removed.
- Direct indexed aggregation was the first implementation. It failed the measured performance gates and therefore triggered the rollup upgrade branch.

## Before Evidence

With 501 requests inside five minutes, the current page can receive at most 500 rows and therefore cannot display more than 100 RPM. The upgraded persistence behavior tests use 501 and 3,000 rows to prove the bounded-page dependency is gone.

## Closeout

Implemented on branch `codex/dashboard-request-metrics-read-model` in worktree `E:\Dev\Projects\relay-pool-desktop-dashboard-metrics`.

Revision at qualification start: `f403f8d3d07ad5da1b00e8bca28c2290775cc2eb`; working tree is intentionally dirty with the plan implementation.

Implemented evidence:

- Dashboard request metric cards consume backend live/cumulative snapshots, not `RequestLog[]` reductions.
- `requestLogs.slice(0, 5)` remains only for recent usage.
- Base-cost comparison was removed from Dashboard request cost display; recent usage shows authoritative actual cost only.
- Live/cumulative snapshots keep independent `captured_at_ms`; fact windows use exclusive end bounds.
- Cost aggregation reads request-level `routing_request_cost_aggregates`; single-currency totals use its structure-preserving compatibility projection, while mixed/incomplete rows keep JSON degradation handling.
- `loadDashboardWorkspace` dead composite query service was deleted after Dashboard moved to explicit query options.

Behavior verified:

- `cargo test --locked --manifest-path src-tauri/Cargo.toml dashboard_metrics_read::tests` — passed, including boundary/status coverage, corrupt cost degradation, late cost convergence, interrupted/failed terminal duration, clear-log cascade, and timestamp index explain coverage.
- Dashboard/source architecture script bundle — passed:
  - `scripts/dashboard-request-cost-format.test.mjs`
  - `scripts/dashboard-recent-usage-layout.test.mjs`
  - `scripts/dashboard-request-count-source.test.mjs`
  - `scripts/dashboard-performance-metrics.test.mjs`
  - `scripts/dashboard-token-value-color.test.mjs`
  - `scripts/dashboard-recent-usage-key-label.test.mjs`
  - `scripts/dashboard-shared-query.test.mjs`
  - `scripts/dashboard-query-service.test.mjs`
  - `scripts/dashboard-station-usage-cards.test.mjs`
  - `scripts/query-services-boundary.test.mjs`
- `pnpm exec tsc --noEmit` — passed.

Performance qualification:

Command:

```powershell
python scripts/dashboard_metrics_perf_probe.py --rows 100000 500000 --warm-samples 10 --cold-samples 3 --writer-samples 50
```

Result:

| Rows | Live warm p95 | Cumulative warm p95 | Writer p95 regression | SQLite busy | Gate |
|---:|---:|---:|---:|---:|---|
| 100,000 | 591.248 ms | 746.764 ms | -22.886% | 0 | Failed primary gate |
| 500,000 | 2870.616 ms | 2962.371 ms | 17.612% | 0 | Failed extended gate |

`EXPLAIN QUERY PLAN` evidence:

- Period range: `SEARCH request_logs USING COVERING INDEX idx_request_logs_dashboard_metrics_range (received_at_ms>? AND received_at_ms<?)`.
- Cost counts: `SEARCH l USING COVERING INDEX idx_request_logs_received_at (received_at_ms>? AND received_at_ms<?)` plus request aggregate primary-key lookup.

Qualification decision:

- Direct indexed aggregation is semantically correct but does not meet the plan’s primary performance gate on this SQLite qualification probe.
- The failed probe is recorded as the trigger for the plan’s rollup-upgrade branch; the threshold was not relaxed.
- This baseline audit is superseded for merge qualification by `docs/archive/audits/2026-08-01-dashboard-request-metrics-rollup-qualification.md`.

Previously not yet re-run after the direct-aggregation qualification attempt:

- `pnpm test`
- `pnpm build`
- `pnpm test:contracts`
- `pnpm lint`
- `cargo check --locked --manifest-path src-tauri/Cargo.toml`

The rollup qualification audit records the final rerun set for this branch.
