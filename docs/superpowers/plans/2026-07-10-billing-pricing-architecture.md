# Relay Pool Billing And Pricing Architecture Implementation Plan

> For implementation workers: execute task by task. Use TDD for behavior changes. Do not implement in a dirty or conflicted checkout. Keep commits small and stage exact paths only.

**Goal:** Implement the unified billing and pricing architecture from `docs/superpowers/specs/2026-07-10-billing-pricing-architecture-design.md`.

**Architecture:** Rust owns pricing truth through a resolver and calculator. Proxy runtime, channel monitoring, request log writes, request log reads, dashboard totals, and logs UI consume that shared backend result instead of recalculating cost independently.

**Non-negotiable constraints:**

- Do not add accounts, payments, subscriptions, cloud sync, team permissions, or a full financial ledger.
- Do not put specific third-party project names into new billing architecture code, tests, schema, comments, or UI copy.
- Do not use balance normalization values as group multipliers.
- Do not let manual pricing rules for one model price another model.
- Do not mix currencies in dashboard totals.
- Do not use `git add .`, `git add -A`, or `git commit -a`.

---

## Entry Gate

- Start from a clean checkout or an isolated worktree.
- Stop if `git status --short -- .` shows unrelated staged paths, unresolved conflicts, or local-routing/schema/runtime work owned by another session.
- Read first:
  - `docs/PROJECT_PLAN.md`
  - `docs/superpowers/specs/2026-07-10-billing-pricing-architecture-design.md`
  - `src-tauri/src/models/pricing.rs`
  - `src-tauri/src/services/pricing/mod.rs`
  - `src-tauri/src/services/database.rs`
  - `src-tauri/src/services/channel_monitors/mod.rs`
  - `src-tauri/src/services/proxy/runtime.rs`
  - `src/features/dashboard/DashboardPage.tsx`
  - `src/features/logs/LogsPage.tsx`

Run:

```powershell
git status --short -- .
git diff --cached --name-only
git log --oneline -8
```

Expected before implementation: no unrelated staged paths and no unmerged files.

If the spec edge-case clarification is still uncommitted, commit or intentionally include it before implementation:

```powershell
git add -- docs/superpowers/specs/2026-07-10-billing-pricing-architecture-design.md
git diff --cached --name-only
git commit -m "docs: clarify billing pricing spec edge cases"
```

Expected staged path: `docs/superpowers/specs/2026-07-10-billing-pricing-architecture-design.md`.

---

## Task 1: Commit This Plan

**Files:**

- Create: `docs/superpowers/plans/2026-07-10-billing-pricing-architecture.md`

- [ ] Step 1: Scan for incomplete markers and forbidden architecture-source terms.

```powershell
$patterns = @('TB' + 'D', 'TO' + 'DO', 'FIX' + 'ME')
Select-String -Path docs/superpowers/plans/2026-07-10-billing-pricing-architecture.md -Pattern $patterns -CaseSensitive
```

Expected: no output.

- [ ] Step 2: Commit the plan only.

```powershell
git add -- docs/superpowers/plans/2026-07-10-billing-pricing-architecture.md
git diff --cached --name-only
git commit -m "docs: add billing pricing architecture plan"
```

Expected staged path: `docs/superpowers/plans/2026-07-10-billing-pricing-architecture.md`.

---

## Task 2: RED Resolver And Calculator Contract Tests

**Files:**

- Modify: `src-tauri/src/services/pricing/mod.rs`
- Optionally create: `src-tauri/src/services/pricing/resolver.rs`
- Optionally create: `src-tauri/src/services/pricing/calculator.rs`
- Modify: `src-tauri/src/models/pricing.rs`

- [ ] Step 1: Add failing Rust tests for resolver priority.

Required RED cases:

- base price exists and no manual rule exists -> status is not `unpriced`.
- station key has active group binding and current multiplier -> final prices equal base price times multiplier.
- station key expects a group multiplier but no current multiplier fact exists -> status is `missing_rate`.
- manual rule for a different model is ignored.
- missing model base price with multiplier present -> status is `missing_model_price`.
- fixed price enters the unified cost breakdown.

Keep test names neutral, for example:

- `pricing_resolver_uses_base_price_without_manual_rule`
- `pricing_resolver_applies_bound_group_multiplier`
- `pricing_resolver_marks_missing_expected_rate`
- `pricing_resolver_ignores_manual_rule_for_other_model`
- `cost_calculator_includes_fixed_price`

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml pricing_resolver_
cargo test --manifest-path .\src-tauri\Cargo.toml cost_calculator_
```

Expected: fail for missing types/functions, not because of unrelated compile errors.

- [ ] Step 2: Commit RED tests if the project convention for this slice allows RED commits. Otherwise keep them unstaged and proceed to implementation.

---

## Task 3: Implement Domain DTOs And Cost Calculator

**Files:**

- Modify: `src-tauri/src/models/pricing.rs`
- Modify: `src-tauri/src/services/pricing/mod.rs`
- Create if helpful: `src-tauri/src/services/pricing/calculator.rs`

- [ ] Step 1: Add DTOs.

Add neutral Rust DTOs with Serde camelCase output where they cross Tauri/API boundaries:

- `PricingStatus`
- `RequestKind`
- `ResolvedPricingContext`
- `RequestUsage`
- `RequestCostBreakdown`

Status string values must match the spec:

- `priced`
- `base_price_only`
- `missing_rate`
- `missing_model_price`
- `unpriced`
- `unsupported_billing_mode`
- `legacy_estimate`

- [ ] Step 2: Implement `calculate_request_cost(context, usage)`.

Initial calculator behavior:

- token input cost = `input_tokens * estimated_input_price`
- token output cost = `output_tokens * estimated_output_price`
- fixed cost = `estimated_fixed_price` when present
- total cost = input + output + fixed
- preserve context currency
- preserve context pricing status
- return no numeric total for statuses that cannot price the request

- [ ] Step 3: Run focused tests.

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml cost_calculator_
```

- [ ] Step 4: Commit calculator foundation.

```powershell
git add -- src-tauri/src/models/pricing.rs src-tauri/src/services/pricing/mod.rs src-tauri/src/services/pricing/calculator.rs
git diff --cached --name-only
git commit -m "feat: add unified request cost calculator"
```

Adjust staged paths if `calculator.rs` is not created.

---

## Task 4: Add Narrow Database Pricing Helpers

**Files:**

- Modify: `src-tauri/src/services/database.rs`
- Optionally create: `src-tauri/src/services/pricing/repository.rs`

- [ ] Step 1: Add or expose narrow read helpers.

The resolver needs helpers that return small typed records, not page-shaped view models:

- station key pricing subject:
  - station key id
  - station id
  - group binding id
  - cached key multiplier fields for compatibility fallback only
- exact manual pricing override candidates for station/key/group/model
- model base price by requested or normalized model
- current group multiplier fact by group binding

- [ ] Step 2: Preserve compatibility wrappers.

Existing public methods such as `route_candidate_economics` and `route_candidate_economics_for_model` should remain available during migration, but their internals should become thin adapters over the new resolver once the resolver exists.

- [ ] Step 3: Add RED/GREEN helper tests where the current bug can be reproduced.

Required cases:

- exact model override wins.
- another model's override is ignored.
- group multiplier fact can be resolved from the key's binding.
- model base price can price the request without a manual rule.

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml route_candidate_economics_
```

- [ ] Step 4: Commit database helper slice.

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/services/pricing/repository.rs
git diff --cached --name-only
git commit -m "feat: add pricing resolver database helpers"
```

Adjust staged paths if `repository.rs` is not created.

---

## Task 5: Implement Pricing Resolver

**Files:**

- Modify: `src-tauri/src/services/pricing/mod.rs`
- Create if helpful: `src-tauri/src/services/pricing/resolver.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] Step 1: Implement `resolve_pricing_context`.

Input:

- `station_key_id`
- `requested_model`
- `request_kind`

Priority:

1. exact manual override matching scope and requested model
2. explicit tested model-pattern override only if implemented
3. key-bound group multiplier times model base price
4. current station/group multiplier times model base price
5. model base price at `1.0x`, as `base_price_only` or `missing_rate`
6. `missing_model_price` or `unpriced` with reason

- [ ] Step 2: Produce source chain.

The source chain should explain:

- model price source
- manual override id when used
- group binding id when used
- group rate evidence id or source when used
- fallback reason for `base_price_only` or `missing_rate`

- [ ] Step 3: Route legacy economics through the resolver.

Map `ResolvedPricingContext` back into the existing `RouteCandidateEconomics` fields so route selection and existing UI keep working during migration.

- [ ] Step 4: Run focused tests.

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml pricing_resolver_
cargo test --manifest-path .\src-tauri\Cargo.toml route_candidate_economics_
```

- [ ] Step 5: Commit resolver.

```powershell
git add -- src-tauri/src/services/pricing/mod.rs src-tauri/src/services/pricing/resolver.rs src-tauri/src/services/database.rs
git diff --cached --name-only
git commit -m "feat: resolve request pricing context"
```

Adjust staged paths if `resolver.rs` is not created.

---

## Task 6: Migrate Channel Monitor Cost Calculation

**Files:**

- Modify: `src-tauri/src/services/channel_monitors/mod.rs`
- Modify tests in the same file or related monitor tests

- [ ] Step 1: Add RED monitor test.

Required monitor behavior:

- monitor request with base price but no manual rule writes a priced or base-price-only cost, not unpriced.
- monitor request with group multiplier uses the same cost as the resolver/calculator.
- fixed price is included if present.

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml channel_monitor
```

- [ ] Step 2: Replace monitor-local formula.

Remove monitor-local input/output multiplication and call:

- `resolve_pricing_context`
- `calculate_request_cost`

- [ ] Step 3: Store context snapshot.

Use existing `economic_context_json` first unless a migration is explicitly approved. The snapshot should include serialized pricing context and cost breakdown.

- [ ] Step 4: Run focused tests.

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml channel_monitor
cargo test --manifest-path .\src-tauri\Cargo.toml pricing_resolver_
```

- [ ] Step 5: Commit monitor migration.

```powershell
git add -- src-tauri/src/services/channel_monitors/mod.rs
git diff --cached --name-only
git commit -m "feat: use unified pricing for channel monitors"
```

---

## Task 7: Migrate Proxy Runtime Cost Calculation

**Files:**

- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify related proxy runtime tests

- [ ] Step 1: Add RED proxy test.

Required proxy behavior:

- proxy request cost equals monitor cost for the same context and usage.
- base price without manual rule does not produce true unpriced.
- wrong-model manual rule is ignored.

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml proxy
```

- [ ] Step 2: Replace proxy-local formula.

Proxy runtime should call the shared resolver and calculator after route selection and usage capture.

- [ ] Step 3: Keep route candidate economics compatible.

Route candidate explanation JSON can still expose estimated prices, but those fields should derive from `ResolvedPricingContext`.

- [ ] Step 4: Run focused tests.

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml proxy
cargo test --manifest-path .\src-tauri\Cargo.toml pricing_resolver_
```

- [ ] Step 5: Commit proxy migration.

```powershell
git add -- src-tauri/src/services/proxy/runtime.rs
git diff --cached --name-only
git commit -m "feat: use unified pricing for proxy requests"
```

---

## Task 8: Stabilize Request Log Snapshot Semantics

**Files:**

- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/models/proxy.rs` if request log DTO fields need status/source additions
- Modify: `src/lib/types/proxy.ts` if Tauri output changes

- [ ] Step 1: Add RED request log tests.

Required behavior:

- logs with saved cost snapshots are returned unchanged after prices change.
- old logs without snapshots are marked `legacy_estimate` when backfilled.
- read-side backfill does not overwrite request-time snapshot status.

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml request_log
```

- [ ] Step 2: Update insert and read behavior.

Rules:

- Request-time writes persist `pricing_status`, cost fields, currency, pricing source, and serialized context.
- `request_log_with_estimated_cost` must not recompute rows that already have a snapshot.
- legacy rows may be estimated for display, but must be marked `legacy_estimate`.

- [ ] Step 3: Run focused tests.

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml request_log
cargo test --manifest-path .\src-tauri\Cargo.toml pricing_resolver_
```

- [ ] Step 4: Commit request log semantics.

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/models/proxy.rs src/lib/types/proxy.ts
git diff --cached --name-only
git commit -m "feat: persist request pricing snapshots"
```

Adjust staged paths based on actual files changed.

---

## Task 9: Dashboard Currency Grouping And Status Display

**Files:**

- Modify: `src/features/dashboard/DashboardPage.tsx`
- Optionally create: `src/features/dashboard/requestCostSummary.ts`
- Create or modify focused dashboard script tests

- [ ] Step 1: Add RED TypeScript/Node test.

Required behavior:

- USD and CNY totals are not summed together.
- `base_price_only` is displayed differently from `unpriced`.
- unpriced/unsupported rows are counted separately.
- legacy estimates are labelled separately.

Suggested test file:

- `scripts/dashboard-request-cost-summary.test.mjs`

Run:

```powershell
node scripts\dashboard-request-cost-summary.test.mjs
```

- [ ] Step 2: Extract a pure summary helper.

Move cost grouping out of the component into a testable helper:

- input: request logs
- output: totals by currency, unpriced count, unsupported count, legacy estimate count, display rows

- [ ] Step 3: Wire dashboard UI.

Dashboard should render grouped cost totals and status labels without changing the overall local-tool layout.

- [ ] Step 4: Run checks.

```powershell
node scripts\dashboard-request-cost-summary.test.mjs
pnpm.cmd exec tsc --noEmit
```

- [ ] Step 5: Commit dashboard migration.

```powershell
git add -- src/features/dashboard/DashboardPage.tsx src/features/dashboard/requestCostSummary.ts scripts/dashboard-request-cost-summary.test.mjs
git diff --cached --name-only
git commit -m "feat: group dashboard request costs by currency"
```

Adjust staged paths based on actual files changed.

---

## Task 10: Logs UI Pricing Status

**Files:**

- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/lib/types/proxy.ts`
- Optionally create a focused script test

- [ ] Step 1: Add RED test or source guard.

Required behavior:

- logs show cost status labels.
- `missing_model_price`, `unpriced`, and `unsupported_billing_mode` are distinct.
- cost formatting does not hide missing status behind a plain dash.

- [ ] Step 2: Update logs rendering.

Display:

- numeric cost when present
- currency when present
- pricing status label
- source chain or reason where the existing layout can support it

- [ ] Step 3: Run checks.

```powershell
pnpm.cmd exec tsc --noEmit
pnpm.cmd build
```

- [ ] Step 4: Commit logs UI.

```powershell
git add -- src/features/logs/LogsPage.tsx src/lib/types/proxy.ts
git diff --cached --name-only
git commit -m "feat: show request pricing status in logs"
```

Adjust staged paths based on actual files changed.

---

## Task 11: Pricing Diagnostics Command

**Files:**

- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/services/pricing/mod.rs`
- Modify: `src/lib/api/economics.ts` or create a neutral API module if needed
- Optionally modify pricing page diagnostics

- [ ] Step 1: Add a backend diagnostic command.

Add a command such as:

- `resolve_station_key_pricing_context(station_key_id, requested_model, request_kind)`

It should return the same `ResolvedPricingContext` used by runtime paths.

- [ ] Step 2: Add tests.

Required behavior:

- command returns source chain.
- command returns `missing_rate` when a bound group lacks a current multiplier.
- command returns `base_price_only` when no multiplier is expected.

- [ ] Step 3: Wire only diagnostic consumers.

Do not make diagnostics the runtime source of truth. Runtime uses the service directly; UI uses the command for explanation.

- [ ] Step 4: Run checks.

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml pricing_resolver_
pnpm.cmd exec tsc --noEmit
```

- [ ] Step 5: Commit diagnostics.

```powershell
git add -- src-tauri/src/commands/mod.rs src-tauri/src/services/pricing/mod.rs src/lib/api/economics.ts
git diff --cached --name-only
git commit -m "feat: expose pricing context diagnostics"
```

Adjust staged paths based on actual files changed.

---

## Task 12: End-To-End Regression Sweep

**Files:**

- Modify only tests or docs if gaps are found

- [ ] Step 1: Run focused Rust test set.

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml pricing_resolver_
cargo test --manifest-path .\src-tauri\Cargo.toml cost_calculator_
cargo test --manifest-path .\src-tauri\Cargo.toml route_candidate_economics_
cargo test --manifest-path .\src-tauri\Cargo.toml request_log
cargo test --manifest-path .\src-tauri\Cargo.toml channel_monitor
cargo test --manifest-path .\src-tauri\Cargo.toml proxy
```

If shared target-dir locks appear, use a separate target dir:

```powershell
$env:CARGO_TARGET_DIR = Join-Path $env:TEMP "relay-pool-billing-target"
cargo test --manifest-path .\src-tauri\Cargo.toml pricing_resolver_
```

- [ ] Step 2: Run frontend checks.

```powershell
node scripts\dashboard-request-cost-summary.test.mjs
pnpm.cmd exec tsc --noEmit
pnpm.cmd build
```

- [ ] Step 3: Run formatting and cargo check.

```powershell
cargo fmt --manifest-path .\src-tauri\Cargo.toml --check
cargo check --manifest-path .\src-tauri\Cargo.toml
```

- [ ] Step 4: Run source guards.

Search for duplicated formulas and forbidden architecture-source naming in new billing surfaces:

```powershell
rg -n "estimated_input_price.*prompt_tokens|estimated_output_price.*completion_tokens|estimatedTotalCost.*reduce|pricing_rule_id.*model" src-tauri src scripts
```

During manual review, search new billing surfaces for prohibited provider- or project-specific source names without committing those names into source files, tests, schema, comments, or UI copy.

- [ ] Step 5: Final status.

```powershell
git status --short -- .
git diff --cached --name-only
git log --oneline -8
```

Expected:

- no staged paths unless preparing a final commit
- no unrelated dirty files introduced by this work
- all commits are focused and exact-path staged

---

## Implementation Order Summary

1. Plan and clean entry gate.
2. Resolver/calculator RED tests.
3. DTOs and calculator.
4. Database helper layer.
5. Resolver implementation and compatibility economics adapter.
6. Channel monitor migration.
7. Proxy runtime migration.
8. Request log snapshot semantics.
9. Dashboard currency grouping and status display.
10. Logs UI status display.
11. Pricing diagnostics command.
12. Full regression sweep.

Do not proceed to UI migration before resolver, calculator, monitor, proxy, and request log snapshot semantics are green.
