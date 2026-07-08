# Relay Pool Data Architecture Stage 8 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Review compatibility fields after stages 0-7 and lock the conclusion that no field is safe to remove in this pass.

**Architecture:** Stage 8 is documentation and guard work only. It updates the field ownership ledger with a compatibility review section, adds a source guard for the review conclusion, and writes the final stage audit.

**Tech Stack:** Markdown audit/ledger, Node source guard, existing field ownership and query boundary scripts.

---

## Entry Gate

- Worktree must be `D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`.
- Branch must be `codex/data-architecture-stage0`.
- Read first:
  - `docs/superpowers/specs/2026-07-07-relay-pool-data-architecture-master-spec.md`
  - `docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-master-plan.md`
  - `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`
  - `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage7-progress.md`
  - this file
- Run drift intake:

```powershell
git status --short
git log --oneline -8
git -C D:\Dev\Projects\relay-pool-desktop status --short
git -C D:\Dev\Projects\relay-pool-desktop log --oneline -8
```

- Stop if main checkout has uncommitted schema, database, runtime, proxy, station key, group binding, pricing, balance, or secret changes.

## Files

- Modify: `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`
- Create: `scripts/compatibility-field-review.test.mjs`
- Create: `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage8-progress.md`

## Task 1: Commit Stage 8 Plan

**Files:**
- Create: `docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage8.md`

- [ ] **Step 1: Scan for placeholders**

```powershell
$patterns = @('TB' + 'D', 'TO' + 'DO', 'lat' + 'er', 'fill' + ' in', 'Similar' + ' to')
Select-String -Path docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage8.md -Pattern $patterns -CaseSensitive
```

Expected: no output.

- [ ] **Step 2: Commit this plan**

```powershell
git add -- docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage8.md
git diff --cached --name-only
git commit -m "docs: add data architecture stage8 plan"
```

Expected staged path: `docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage8.md`.

## Task 2: RED Compatibility Review Guard

**Files:**
- Create: `scripts/compatibility-field-review.test.mjs`

- [ ] **Step 1: Write failing review guard**

Create `scripts/compatibility-field-review.test.mjs`:

```js
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const ledger = await readFile("docs/superpowers/audits/relay-pool-field-ownership-ledger.md", "utf8");

assert.ok(ledger.includes("## Stage 8 兼容字段复查结论"));
assert.ok(ledger.includes("本轮无 removable candidate 字段"));
assert.ok(ledger.includes("`station_keys.group_name`") && ledger.includes("compatibility cache"));
assert.ok(ledger.includes("`station_keys.group_id_hash`") && ledger.includes("compatibility cache"));
assert.ok(ledger.includes("`station_keys.rate_multiplier`") && ledger.includes("compatibility cache"));
assert.ok(ledger.includes("`stations.balance_raw`") && ledger.includes("compatibility cache"));
assert.ok(ledger.includes("`stations.balance_cny`") && ledger.includes("compatibility cache"));
assert.ok(ledger.includes("`stations.last_pricing_fetched_at`") && ledger.includes("compatibility cache"));
assert.ok(!ledger.includes("| `station_keys.group_name` | removable candidate |"));
assert.ok(!ledger.includes("| `station_keys.group_id_hash` | removable candidate |"));
assert.ok(!ledger.includes("| `station_keys.rate_multiplier` | removable candidate |"));
assert.ok(!ledger.includes("| `stations.balance_raw` | removable candidate |"));
assert.ok(!ledger.includes("| `stations.balance_cny` | removable candidate |"));
assert.ok(!ledger.includes("| `stations.last_pricing_fetched_at` | removable candidate |"));
```

- [ ] **Step 2: Verify RED**

```powershell
node scripts/compatibility-field-review.test.mjs
```

Expected: FAIL because the Stage 8 review section has not been added to the ledger.

- [ ] **Step 3: Commit RED**

```powershell
git add -- scripts/compatibility-field-review.test.mjs
git diff --cached --name-only
git commit -m "test: guard compatibility field review"
```

## Task 3: Update Field Ownership Ledger

**Files:**
- Modify: `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`

- [ ] **Step 1: Add Stage 8 review section**

Append a section titled `## Stage 8 兼容字段复查结论` with these conclusions:

- 本轮无 removable candidate 字段。
- `station_keys.group_name` remains compatibility cache for display and legacy fallback.
- `station_keys.group_id_hash` remains compatibility cache for remote identity metadata and remote key workflows.
- `station_keys.rate_multiplier`, `station_keys.rate_source`, and `station_keys.rate_collected_at` remain compatibility cache for old database and preview fallback.
- `stations.balance_raw` and `stations.balance_cny` remain compatibility cache for no station-scope snapshot fallback.
- `stations.last_pricing_fetched_at` remains compatibility cache for coarse collector freshness diagnostics.
- Runtime snapshot does not consume UI view model and does not carry plaintext secret.
- No schema migration or field deletion is approved by Stage 8.

- [ ] **Step 2: Verify GREEN**

```powershell
node scripts/compatibility-field-review.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
```

Expected: both exit 0.

- [ ] **Step 3: Commit ledger update**

```powershell
git add -- docs/superpowers/audits/relay-pool-field-ownership-ledger.md
git diff --cached --name-only
git commit -m "docs: review compatibility field ownership"
```

## Task 4: Stage 8 Final Verification And Audit

**Files:**
- Create: `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage8-progress.md`

- [ ] **Step 1: Run final verification**

```powershell
node scripts/compatibility-field-review.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
node scripts/runtime-snapshot-boundary.test.mjs
node scripts/runtime-snapshot-projection.test.mjs
pnpm.cmd build
git status --short
```

Expected: all commands exit 0; `pnpm.cmd build` may keep the existing Vite chunk-size warning.

- [ ] **Step 2: Write and commit audit**

Document Stage 8 commits, drift intake, compatibility field conclusions, verification results, and whole-upgrade status.

```powershell
git add -- docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage8-progress.md
git diff --cached --name-only
git commit -m "docs: summarize data architecture stage8 progress"
```

- [ ] **Step 3: Update rolling heartbeat**

Use `automation_update` to record latest HEAD, Stage 8 verification, blockers, and whole-upgrade closeout status.

## Self Review

- Spec coverage: Stage 8 performs compatibility field review without deleting fields or changing schema.
- Guard coverage: source guard confirms core compatibility fields are not marked removable.
- Runtime safety: Stage 8 preserves the Stage 7 no-plaintext-secret snapshot boundary.
