# Relay Pool Data Architecture Stage 7 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a pure, testable runtime route snapshot projection that compiles stable proxy input without reading UI view models or exposing plaintext secrets.

**Architecture:** Stage 7 adds a TypeScript projection boundary first, then protects it with source guards. The snapshot keeps secret references and routing evidence only; Rust proxy secret material remains outside this stage.

**Tech Stack:** TypeScript pure projection, Node ESM tests, existing pricing/group/balance projections, Vite/TypeScript build.

---

## Entry Gate

- Worktree must be `D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`.
- Branch must be `codex/data-architecture-stage0`.
- Read first:
  - `docs/superpowers/specs/2026-07-07-relay-pool-data-architecture-master-spec.md`
  - `docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-master-plan.md`
  - `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`
  - `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage6-progress.md`
  - this file
- Run drift intake:

```powershell
git status --short
git log --oneline -8
git -C D:\Dev\Projects\relay-pool-desktop status --short
git -C D:\Dev\Projects\relay-pool-desktop log --oneline -8
```

- Stop if main checkout has uncommitted runtime, proxy, secret, station key, group binding, pricing, balance, or schema changes.

## Files

- Create: `src/lib/projections/runtimeSnapshot.ts`
- Create: `scripts/runtime-snapshot-projection.test.mjs`
- Create: `scripts/runtime-snapshot-boundary.test.mjs`
- Create: `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage7-progress.md`
- Modify only if needed for final boundary inventory: `scripts/data-architecture-field-ownership.test.mjs`

## Runtime Snapshot Shape

`src/lib/projections/runtimeSnapshot.ts` must export:

```ts
export type RuntimeRouteSecretRef = {
  kind: "station_key_secret";
  stationKeyId: string;
  present: boolean;
  masked: string | null;
};

export type RuntimeRouteSnapshotCandidate = {
  stationKeyId: string;
  stationId: string;
  stationName: string;
  keyName: string;
  enabled: boolean;
  priority: number;
  upstreamBaseUrl: string;
  upstreamApiFormat: string;
  secretRef: RuntimeRouteSecretRef;
  groupBindingId: string | null;
  groupIdentityKey: string | null;
  rateMultiplier: number | null;
  rateSource: string | null;
  modelPolicy: {
    allowlist: string[];
    blocklist: string[];
    preferredModels: string[];
    onlyUseAsBackup: boolean;
    routingTags: string[];
  };
  pricingStatus: {
    pricingRuleId: string | null;
    priceConfidence: number | null;
    source: string | null;
  };
  balanceStatus: {
    status: string | null;
    value: number | null;
    currency: string;
    scope: string | null;
    collectedAt: string | null;
  };
  healthStatus: {
    consecutiveFailures: number;
    successCount: number;
    failureCount: number;
    cooldownUntil: string | null;
    lastErrorSummary: string | null;
  };
  evidence: {
    groupFactIdentity: string | null;
    groupRateRecordId: string | null;
    balanceSnapshotId: string | null;
    capabilityUpdatedAt: string | null;
    healthUpdatedAt: string | null;
  };
};

export type RuntimeRouteSnapshot = {
  snapshotId: string;
  generatedAt: string;
  version: 1;
  candidates: RuntimeRouteSnapshotCandidate[];
};
```

## Task 1: Commit Stage 7 Plan

**Files:**
- Create: `docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage7.md`

- [ ] **Step 1: Scan for placeholders**

```powershell
$patterns = @('TB' + 'D', 'TO' + 'DO', 'lat' + 'er', 'fill' + ' in', 'Similar' + ' to')
Select-String -Path docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage7.md -Pattern $patterns -CaseSensitive
```

Expected: no output.

- [ ] **Step 2: Commit this plan**

```powershell
git add -- docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage7.md
git diff --cached --name-only
git commit -m "docs: add data architecture stage7 plan"
```

Expected staged path: `docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage7.md`.

## Task 2: RED Runtime Snapshot Projection Test

**Files:**
- Create: `scripts/runtime-snapshot-projection.test.mjs`

- [ ] **Step 1: Write failing behavior test**

Create `scripts/runtime-snapshot-projection.test.mjs` with an ESM transpile harness like the existing projection tests. The test must:

```js
const snapshot = buildRuntimeRouteSnapshot({
  generatedAt: "2026-07-08T00:00:00.000Z",
  stations: [station({ id: "station-a", name: "Alpha", baseUrl: "https://alpha.example/v1" })],
  stationKeys: [
    key({
      id: "key-a",
      stationId: "station-a",
      name: "primary",
      apiKeyMasked: "sk-...masked",
      apiKeyPresent: true,
      enabled: true,
      priority: 5,
      groupBindingId: "binding-a",
    }),
    key({
      id: "key-disabled",
      stationId: "station-a",
      name: "disabled",
      apiKeyMasked: "sk-disabled",
      apiKeyPresent: true,
      enabled: false,
      priority: 1,
    }),
  ],
  capabilities: [
    capability({
      stationKeyId: "key-a",
      modelAllowlist: ["gpt-4.1"],
      preferredModels: ["gpt-4.1-mini"],
      onlyUseAsBackup: false,
      updatedAt: "2026-07-08T00:01:00.000Z",
    }),
  ],
  health: [
    health({
      stationKeyId: "key-a",
      consecutiveFailures: 2,
      cooldownUntil: "2026-07-08T00:10:00.000Z",
      updatedAt: "2026-07-08T00:02:00.000Z",
    }),
  ],
  groupBindings: [
    binding({
      id: "binding-a",
      stationId: "station-a",
      groupName: "vip",
      effectiveRateMultiplier: 0.75,
      rateSource: "collector",
    }),
  ],
  groupRates: [],
  pricingRules: [
    pricingRule({
      id: "rule-a",
      stationId: "station-a",
      groupBindingId: "binding-a",
      rateMultiplier: 0.8,
      confidence: 0.9,
      source: "manual",
    }),
  ],
  balances: [
    balance({
      id: "balance-a",
      stationId: "station-a",
      scope: "station",
      value: 42,
      currency: "CNY",
      status: "normal",
      collectedAt: "2026-07-08T00:03:00.000Z",
    }),
  ],
});

assert.equal(snapshot.version, 1);
assert.equal(snapshot.snapshotId, "runtime-route-2026-07-08T00:00:00.000Z");
assert.deepEqual(snapshot.candidates.map((candidate) => candidate.stationKeyId), ["key-a"]);
assert.equal(snapshot.candidates[0].secretRef.kind, "station_key_secret");
assert.equal(snapshot.candidates[0].secretRef.present, true);
assert.equal(snapshot.candidates[0].secretRef.masked, "sk-...masked");
assert.equal(JSON.stringify(snapshot).includes("sk-live-plaintext"), false);
assert.equal(snapshot.candidates[0].groupBindingId, "binding-a");
assert.equal(snapshot.candidates[0].rateMultiplier, 0.75);
assert.equal(snapshot.candidates[0].rateSource, "collector");
assert.equal(snapshot.candidates[0].modelPolicy.allowlist[0], "gpt-4.1");
assert.equal(snapshot.candidates[0].pricingStatus.pricingRuleId, null);
assert.equal(snapshot.candidates[0].balanceStatus.value, 42);
assert.equal(snapshot.candidates[0].healthStatus.cooldownUntil, "2026-07-08T00:10:00.000Z");
assert.equal(snapshot.candidates[0].evidence.groupFactIdentity, "binding:binding-a");
assert.equal(snapshot.candidates[0].evidence.balanceSnapshotId, "balance-a");
```

- [ ] **Step 2: Verify RED**

```powershell
node scripts/runtime-snapshot-projection.test.mjs
```

Expected: FAIL because `src/lib/projections/runtimeSnapshot.ts` does not exist or does not export `buildRuntimeRouteSnapshot`.

- [ ] **Step 3: Commit RED**

```powershell
git add -- scripts/runtime-snapshot-projection.test.mjs
git diff --cached --name-only
git commit -m "test: guard runtime route snapshot projection"
```

## Task 3: Implement Pure Runtime Snapshot Projection

**Files:**
- Create: `src/lib/projections/runtimeSnapshot.ts`

- [ ] **Step 1: Implement minimal projection**

Create `src/lib/projections/runtimeSnapshot.ts`. It must import only from `@/lib/projections/*` and `@/lib/types/*`, not from `@/features/*`, `@/lib/api/*`, Tauri, or secret APIs.

Implementation rules:

- Build current group facts with `buildCurrentStationGroupFacts({ bindings: groupBindings, rates: groupRates })`.
- Build station balance facts with `buildCurrentStationBalanceFacts({ stations, balances })`.
- Use `buildPricingGroupCandidates()` only for pricing evidence; do not let pricing rule multiplier override a current group fact multiplier.
- Exclude disabled station keys and keys with `apiKeyPresent === false`.
- Candidate `secretRef` uses `stationKeyId`, `apiKeyPresent`, and `apiKeyMasked`; no `apiKey` input property exists.
- Sort candidates by `priority`, then `stationKeyId`.

- [ ] **Step 2: Verify GREEN**

```powershell
node scripts/runtime-snapshot-projection.test.mjs
node scripts/group-facts-projection.test.mjs
node scripts/pricing-facts-projection.test.mjs
node scripts/station-current-balance-projection.test.mjs
```

Expected: all exit 0.

- [ ] **Step 3: Commit implementation**

```powershell
git add -- src/lib/projections/runtimeSnapshot.ts
git diff --cached --name-only
git commit -m "refactor: add runtime route snapshot projection"
```

## Task 4: Runtime Snapshot Boundary Guard

**Files:**
- Create: `scripts/runtime-snapshot-boundary.test.mjs`

- [ ] **Step 1: Write source guard**

Create `scripts/runtime-snapshot-boundary.test.mjs`:

```js
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/lib/projections/runtimeSnapshot.ts", "utf8");

assert.ok(source.includes("buildRuntimeRouteSnapshot"));
assert.ok(source.includes("buildCurrentStationGroupFacts"));
assert.ok(source.includes("buildCurrentStationBalanceFacts"));
assert.ok(source.includes("buildPricingGroupCandidates"));
assert.ok(source.includes("secretRef"));
assert.ok(!source.includes("@/features/"));
assert.ok(!source.includes("@/lib/api/"));
assert.ok(!source.includes("@tauri-apps/api"));
assert.ok(!source.includes("apiKey:"));
assert.ok(!source.includes(".apiKey"));
assert.ok(!source.includes("listStation"));
assert.ok(!source.includes("invoke<"));
```

- [ ] **Step 2: Verify guard**

```powershell
node scripts/runtime-snapshot-boundary.test.mjs
```

Expected: exit 0 after Task 3.

- [ ] **Step 3: Commit guard**

```powershell
git add -- scripts/runtime-snapshot-boundary.test.mjs
git diff --cached --name-only
git commit -m "test: guard runtime snapshot projection boundary"
```

## Task 5: Stage 7 Final Verification And Audit

**Files:**
- Create: `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage7-progress.md`

- [ ] **Step 1: Run final verification**

```powershell
node scripts/runtime-snapshot-projection.test.mjs
node scripts/runtime-snapshot-boundary.test.mjs
node scripts/group-facts-projection.test.mjs
node scripts/pricing-facts-projection.test.mjs
node scripts/station-current-balance-projection.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
pnpm.cmd build
git status --short
```

Expected: all commands exit 0; `pnpm.cmd build` may keep the existing Vite chunk-size warning.

- [ ] **Step 2: Write and commit audit**

Document Stage 7 commits, drift intake, no schema changes, no plaintext secret in snapshot, runtime boundary status, verification results, and next stage: Stage 8 compatibility field review.

```powershell
git add -- docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage7-progress.md
git diff --cached --name-only
git commit -m "docs: summarize data architecture stage7 progress"
```

- [ ] **Step 3: Update rolling heartbeat**

Use `automation_update` to record latest HEAD, Stage 7 verification, blockers, and next stage: Stage 8 compatibility field review.

## Self Review

- Spec coverage: plan creates runtime snapshot candidates with group binding id, effective multiplier/source, model policy, pricing status, balance status, health/cooldown evidence, and secret references.
- Boundary coverage: pure projection guard prevents imports from features, Tauri, API modules, and plaintext `apiKey` access.
- Schema safety: no database schema, Rust route candidate, or secret storage changes are part of Stage 7.
