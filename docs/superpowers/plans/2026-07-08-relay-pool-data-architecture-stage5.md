# Relay Pool Data Architecture Stage 5 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move station detail group rows, station asset rate chips, and station balance display to shared current projections while preserving the existing page layouts.

**Architecture:** Stage 5 extends the Stage 3 group projection with display-safe current group helpers, adds a pure station balance projection, then migrates `stationDetailViewModels.ts` and `stationAssetViewModels.ts` to consume these shared facts. Query loading stays unchanged; this stage only changes view-model data construction and focused tests.

**Tech Stack:** TypeScript pure projections, React view models, Node script tests, Vite/TypeScript build.

---

## Entry Gate

- Worktree must be `D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`.
- Worktree branch must be `codex/data-architecture-stage0`.
- Read first:
  - `docs/superpowers/specs/2026-07-07-relay-pool-data-architecture-master-spec.md`
  - `docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-master-plan.md`
  - `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`
  - `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage4-progress.md`
  - this file
- Run drift intake:

```powershell
git status --short
git log --oneline -8
git -C D:\Dev\Projects\relay-pool-desktop status --short
git -C D:\Dev\Projects\relay-pool-desktop log --oneline -8
```

- If main checkout has uncommitted station detail, station asset, group facts, balance, collector, runtime, or schema changes, stop and record an intake blocker.
- If main checkout only has committed changes, merge `master`, then rerun:

```powershell
node scripts/station-detail-group-source.test.mjs
node scripts/station-assets-current-projections.test.mjs
node scripts/station-current-balance-projection.test.mjs
node scripts/group-facts-projection.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
pnpm.cmd build
```

## Files

- Create: `src/lib/projections/balanceFacts.ts`
- Create: `scripts/station-current-balance-projection.test.mjs`
- Create: `scripts/station-assets-current-projections.test.mjs`
- Modify: `src/lib/projections/groupFacts.ts`
- Modify: `scripts/group-facts-projection.test.mjs`
- Modify: `src/features/stations/stationDetailViewModels.ts`
- Modify: `src/features/stations/stationAssetViewModels.ts`
- Create: `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage5-progress.md`
- Do not modify: `src/features/stations/StationDetailPage.tsx`, `src/features/stations/StationsPage.tsx` visual structure unless a source guard proves an import path must be updated.
- Do not modify Rust schema or database code in Stage 5.

## Projection Contracts

### `src/lib/projections/balanceFacts.ts`

```ts
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { Station } from "@/lib/types/stations";

export type StationBalanceCurrentFact = {
  stationId: string;
  snapshotId: string | null;
  value: number | null;
  currency: string;
  lowBalanceThreshold: number | null;
  status: string | null;
  source: "balance_snapshot" | "station_cache" | "missing";
  sourceLabel: string;
  updatedAt: string | null;
  collectedAt: string | null;
  sourceSnapshot: BalanceSnapshot | null;
};

export function buildCurrentStationBalanceFacts(input: {
  stations: Station[];
  balances: BalanceSnapshot[];
}): Map<string, StationBalanceCurrentFact>;

export function currentStationBalanceFor(input: {
  station: Station;
  balances: BalanceSnapshot[];
}): StationBalanceCurrentFact;
```

Rules:

- Station-scope `BalanceSnapshot` is authoritative for current station balance.
- Station-key snapshots must not override station-scope snapshots, even if newer.
- If no station-scope snapshot exists, fallback to station compatibility cache: `station.balanceCny`, `station.lowBalanceThresholdCny`, `station.lastCheckedAt`.
- If both snapshot and cache are absent, return a `missing` fact with `value: null`.
- Do not write database fields or alter schema.

### `src/lib/projections/groupFacts.ts`

Add this helper without changing the existing `StationGroupCurrentFact` shape:

```ts
export function isDisplayableStationGroupCurrentFact(fact: StationGroupCurrentFact) {
  return (
    fact.bindingKind === "station_group" &&
    fact.available &&
    fact.bindingStatus !== "manual_legacy" &&
    fact.sourceBinding?.rateSource !== "legacy_key_group"
  );
}
```

Rules:

- `buildCurrentStationGroupFacts()` remains the only source of group identity and rate fallback.
- Station detail and station asset chips must both use `isDisplayableStationGroupCurrentFact()` so they agree on missing, disabled, and legacy groups.
- Missing/disabled facts remain preserved by the projection but are not displayed as current group rows/chips.

## Task 1: Stage 5 Plan Commit

**Files:**
- Create: `docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage5.md`

- [ ] **Step 1: Review the plan file**

Run:

```powershell
$patterns = @('TB' + 'D', 'TO' + 'DO', 'lat' + 'er', 'fill' + ' in', 'Similar' + ' to')
Select-String -Path docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage5.md -Pattern $patterns -CaseSensitive
```

Expected: no output.

- [ ] **Step 2: Commit the plan**

```powershell
git add -- docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage5.md
git diff --cached --name-only
git commit -m "docs: add data architecture stage5 plan"
```

Expected staged path:

```text
docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage5.md
```

## Task 2: RED Test For Current Station Balance Projection

**Files:**
- Create: `scripts/station-current-balance-projection.test.mjs`

- [ ] **Step 1: Write the failing test**

Create `scripts/station-current-balance-projection.test.mjs`:

```js
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import ts from "typescript";

async function importBalanceProjection() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-balance-projection-"));
  const outputPath = join(tempRoot, "balanceFacts.mjs");
  const source = await readFile("src/lib/projections/balanceFacts.ts", "utf8");
  const output = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      verbatimModuleSyntax: true,
    },
  }).outputText;
  await writeFile(outputPath, output, "utf8");
  return import(`file://${outputPath.replaceAll("\\", "/")}`);
}

const { buildCurrentStationBalanceFacts, currentStationBalanceFor } = await importBalanceProjection();

const stations = [
  station({ id: "station-a", balanceCny: 99, lowBalanceThresholdCny: 10, lastCheckedAt: "2026-07-08T01:00:00.000Z" }),
  station({ id: "station-b", balanceCny: 6, lowBalanceThresholdCny: 8, lastCheckedAt: "2026-07-08T02:00:00.000Z" }),
  station({ id: "station-c", balanceCny: null, lowBalanceThresholdCny: null, lastCheckedAt: null }),
];

const facts = buildCurrentStationBalanceFacts({
  stations,
  balances: [
    balance({ id: "key-newer", stationId: "station-a", stationKeyId: "key-a", scope: "station_key", value: 100, updatedAt: "2026-07-08T04:00:00.000Z" }),
    balance({ id: "station-old", stationId: "station-a", scope: "station", value: 12, status: "normal", updatedAt: "2026-07-08T03:00:00.000Z" }),
    balance({ id: "station-new", stationId: "station-a", scope: "station", value: 13, status: "low", lowBalanceThreshold: 20, source: "station_balance", updatedAt: "2026-07-08T05:00:00.000Z", collectedAt: "2026-07-08T05:00:01.000Z" }),
  ],
});

assert.deepEqual(
  Array.from(facts.values()).map((fact) => ({
    stationId: fact.stationId,
    snapshotId: fact.snapshotId,
    value: fact.value,
    lowBalanceThreshold: fact.lowBalanceThreshold,
    status: fact.status,
    source: fact.source,
    updatedAt: fact.updatedAt,
    collectedAt: fact.collectedAt,
  })),
  [
    {
      stationId: "station-a",
      snapshotId: "station-new",
      value: 13,
      lowBalanceThreshold: 20,
      status: "low",
      source: "balance_snapshot",
      updatedAt: "2026-07-08T05:00:00.000Z",
      collectedAt: "2026-07-08T05:00:01.000Z",
    },
    {
      stationId: "station-b",
      snapshotId: null,
      value: 6,
      lowBalanceThreshold: 8,
      status: "low",
      source: "station_cache",
      updatedAt: "2026-07-08T02:00:00.000Z",
      collectedAt: null,
    },
    {
      stationId: "station-c",
      snapshotId: null,
      value: null,
      lowBalanceThreshold: null,
      status: null,
      source: "missing",
      updatedAt: null,
      collectedAt: null,
    },
  ],
  "current balance facts should prefer latest station-scope snapshots, ignore station-key snapshots, and fallback to station cache",
);

assert.equal(
  currentStationBalanceFor({ station: stations[1], balances: [] }).source,
  "station_cache",
  "single-station helper should use the same fallback rule",
);

const projectionSource = await readFile("src/lib/projections/balanceFacts.ts", "utf8");
assert.ok(
  !projectionSource.includes("invoke<") && !projectionSource.includes("listBalanceSnapshots"),
  "balance projection should stay pure and must not call Tauri or query services",
);

function station(overrides) {
  return {
    id: "station",
    name: "Station",
    stationType: "sub2api",
    baseUrl: "https://station.example.test",
    apiKeyMasked: "sk-...",
    apiKeyPresent: true,
    keyCount: 1,
    enabled: true,
    priority: 0,
    creditPerCny: 10,
    balanceRaw: null,
    balanceCny: null,
    lowBalanceThresholdCny: null,
    collectionIntervalMinutes: 5,
    status: "healthy",
    latencyMs: null,
    lastCheckedAt: null,
    lastPricingFetchedAt: null,
    note: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}

function balance(overrides) {
  return {
    id: "balance",
    stationId: "station",
    stationKeyId: null,
    scope: "station",
    value: 1,
    currency: "CNY",
    creditUnit: null,
    usedValue: null,
    totalValue: null,
    lowBalanceThreshold: null,
    status: "normal",
    source: "station_balance",
    confidence: 1,
    collectedAt: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
node scripts/station-current-balance-projection.test.mjs
```

Expected: FAIL with `ENOENT` for `src/lib/projections/balanceFacts.ts`.

- [ ] **Step 3: Commit RED test**

```powershell
git add -- scripts/station-current-balance-projection.test.mjs
git diff --cached --name-only
git commit -m "test: guard current station balance projection"
```

## Task 3: Implement Current Station Balance Projection

**Files:**
- Create: `src/lib/projections/balanceFacts.ts`

- [ ] **Step 1: Create projection implementation**

Create `src/lib/projections/balanceFacts.ts`:

```ts
import { toTimestampMillis } from "@/lib/time";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { Station } from "@/lib/types/stations";

export type StationBalanceCurrentFact = {
  stationId: string;
  snapshotId: string | null;
  value: number | null;
  currency: string;
  lowBalanceThreshold: number | null;
  status: string | null;
  source: "balance_snapshot" | "station_cache" | "missing";
  sourceLabel: string;
  updatedAt: string | null;
  collectedAt: string | null;
  sourceSnapshot: BalanceSnapshot | null;
};

export function buildCurrentStationBalanceFacts(input: {
  stations: Station[];
  balances: BalanceSnapshot[];
}): Map<string, StationBalanceCurrentFact> {
  const latestStationBalances = latestStationBalanceSnapshots(input.balances);
  return new Map(
    input.stations.map((station) => [
      station.id,
      factForStation(station, latestStationBalances.get(station.id) ?? null),
    ]),
  );
}

export function currentStationBalanceFor(input: {
  station: Station;
  balances: BalanceSnapshot[];
}): StationBalanceCurrentFact {
  return factForStation(
    input.station,
    latestStationBalanceSnapshots(input.balances).get(input.station.id) ?? null,
  );
}

function latestStationBalanceSnapshots(balances: BalanceSnapshot[]) {
  const latest = new Map<string, BalanceSnapshot>();
  for (const balance of balances) {
    if (balance.scope !== "station") {
      continue;
    }
    const current = latest.get(balance.stationId);
    if (!current || toTime(balance.updatedAt) > toTime(current.updatedAt)) {
      latest.set(balance.stationId, balance);
    }
  }
  return latest;
}

function factForStation(
  station: Station,
  snapshot: BalanceSnapshot | null,
): StationBalanceCurrentFact {
  if (snapshot) {
    return {
      stationId: station.id,
      snapshotId: snapshot.id,
      value: snapshot.value,
      currency: snapshot.currency || "CNY",
      lowBalanceThreshold: snapshot.lowBalanceThreshold,
      status: snapshot.status,
      source: "balance_snapshot",
      sourceLabel: snapshot.source,
      updatedAt: snapshot.updatedAt,
      collectedAt: snapshot.collectedAt,
      sourceSnapshot: snapshot,
    };
  }

  if (
    typeof station.balanceCny === "number" ||
    typeof station.lowBalanceThresholdCny === "number" ||
    station.lastCheckedAt
  ) {
    return {
      stationId: station.id,
      snapshotId: null,
      value: finiteOrNull(station.balanceCny),
      currency: "CNY",
      lowBalanceThreshold: finiteOrNull(station.lowBalanceThresholdCny),
      status: balanceStatusFor(station.balanceCny, station.lowBalanceThresholdCny),
      source: "station_cache",
      sourceLabel: "station_config",
      updatedAt: station.lastCheckedAt,
      collectedAt: null,
      sourceSnapshot: null,
    };
  }

  return {
    stationId: station.id,
    snapshotId: null,
    value: null,
    currency: "CNY",
    lowBalanceThreshold: null,
    status: null,
    source: "missing",
    sourceLabel: "missing",
    updatedAt: null,
    collectedAt: null,
    sourceSnapshot: null,
  };
}

function balanceStatusFor(value: number | null, threshold: number | null) {
  if (value == null || !Number.isFinite(value)) {
    return null;
  }
  if (value <= 0) {
    return "depleted";
  }
  if (threshold != null && Number.isFinite(threshold) && value <= threshold) {
    return "low";
  }
  return "normal";
}

function finiteOrNull(value: number | null) {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function toTime(value: string | null) {
  if (!value) {
    return 0;
  }
  const time = toTimestampMillis(value);
  return Number.isNaN(time) ? 0 : time;
}
```

- [ ] **Step 2: Verify GREEN**

Run:

```powershell
node scripts/station-current-balance-projection.test.mjs
```

Expected: exit 0.

- [ ] **Step 3: Commit implementation**

```powershell
git add -- src/lib/projections/balanceFacts.ts
git diff --cached --name-only
git commit -m "refactor: add current station balance projection"
```

## Task 4: RED Test For Shared Station Group Display Facts

**Files:**
- Modify: `scripts/group-facts-projection.test.mjs`

- [ ] **Step 1: Add failing assertion**

Append this assertion near the end of `scripts/group-facts-projection.test.mjs`:

```js
const { isDisplayableStationGroupCurrentFact } = await importGroupProjection();
const displayFacts = buildCurrentStationGroupFacts({
  bindings: [
    binding({ id: "display", groupName: "display", bindingStatus: "available", rateSource: "sub2api_groups_rates" }),
    binding({ id: "missing", groupName: "missing", bindingStatus: "missing", rateSource: "sub2api_groups_rates" }),
    binding({ id: "disabled", groupName: "disabled", bindingStatus: "disabled", rateSource: "sub2api_groups_rates" }),
    binding({ id: "legacy", groupName: "legacy", bindingStatus: "manual_legacy", rateSource: "manual_legacy" }),
    binding({ id: "legacy-key", groupName: "legacy key", bindingStatus: "available", rateSource: "legacy_key_group" }),
  ],
  rates: [],
});

assert.deepEqual(
  displayFacts.filter(isDisplayableStationGroupCurrentFact).map((fact) => fact.groupName),
  ["display"],
  "displayable station group current facts should exclude missing, disabled, manual legacy, and legacy key group rows",
);
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
node scripts/group-facts-projection.test.mjs
```

Expected: FAIL because `isDisplayableStationGroupCurrentFact` is not exported.

- [ ] **Step 3: Commit RED test**

```powershell
git add -- scripts/group-facts-projection.test.mjs
git diff --cached --name-only
git commit -m "test: guard displayable group current facts"
```

## Task 5: Implement Shared Station Group Display Helper

**Files:**
- Modify: `src/lib/projections/groupFacts.ts`

- [ ] **Step 1: Add display helper**

Add this export after `buildStationGroupOptionsFromCurrentFacts()`:

```ts
export function isDisplayableStationGroupCurrentFact(fact: StationGroupCurrentFact) {
  return (
    fact.bindingKind === "station_group" &&
    fact.available &&
    fact.bindingStatus !== "manual_legacy" &&
    fact.sourceBinding?.rateSource !== "legacy_key_group"
  );
}
```

- [ ] **Step 2: Verify GREEN**

Run:

```powershell
node scripts/group-facts-projection.test.mjs
```

Expected: exit 0.

- [ ] **Step 3: Commit implementation**

```powershell
git add -- src/lib/projections/groupFacts.ts
git diff --cached --name-only
git commit -m "refactor: add displayable group current fact guard"
```

## Task 6: RED Test For Station Detail And Asset Projection Consumption

**Files:**
- Create: `scripts/station-assets-current-projections.test.mjs`
- Modify: `scripts/station-detail-group-source.test.mjs`

- [ ] **Step 1: Extend station detail source guard**

In `scripts/station-detail-group-source.test.mjs`, update the loader to transpile `groupFacts.ts` into a temporary file and replace the `@/lib/projections/groupFacts` import in `stationDetailViewModels.ts`. Then add these source assertions:

```js
const detailSource = await readFile("src/features/stations/stationDetailViewModels.ts", "utf8");
assert.ok(
  detailSource.includes("buildCurrentStationGroupFacts") &&
    detailSource.includes("isDisplayableStationGroupCurrentFact"),
  "station detail group rows should consume shared current group projection facts",
);
assert.ok(
  !detailSource.includes("function dedupeStationGroupBindings(") &&
    !detailSource.includes("function preferStationGroupBinding(") &&
    !detailSource.includes("function stationGroupBindingScore("),
  "station detail should not keep page-local station group de-duplication after Stage 5",
);
```

- [ ] **Step 2: Write station asset RED test**

Create `scripts/station-assets-current-projections.test.mjs`:

```js
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import ts from "typescript";

async function transpileTsFile(sourcePath, outputPath, replacements = []) {
  let source = await readFile(sourcePath, "utf8");
  for (const [from, to] of replacements) {
    source = source.replaceAll(from, to);
  }
  const output = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      verbatimModuleSyntax: true,
    },
  }).outputText;
  await writeFile(outputPath, output, "utf8");
}

async function importStationAssetViewModels() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-station-assets-"));
  const groupFactsPath = join(tempRoot, "groupFacts.mjs");
  const balanceFactsPath = join(tempRoot, "balanceFacts.mjs");
  const assetPath = join(tempRoot, "stationAssetViewModels.mjs");
  await transpileTsFile("src/lib/projections/groupFacts.ts", groupFactsPath);
  await transpileTsFile("src/lib/projections/balanceFacts.ts", balanceFactsPath, [
    ['@/lib/time', './time.mjs'],
  ]);
  await writeFile(
    join(tempRoot, "time.mjs"),
    "export function toTimestampMillis(value) { return Date.parse(value); }",
    "utf8",
  );
  await transpileTsFile("src/features/stations/stationAssetViewModels.ts", assetPath, [
    ['@/lib/projections/groupFacts', './groupFacts.mjs'],
    ['@/lib/projections/balanceFacts', './balanceFacts.mjs'],
    ['@/lib/time', './time.mjs'],
  ]);
  return import(`file://${assetPath.replaceAll("\\", "/")}`);
}

const { buildStationAssetRows, formatStationBalance } = await importStationAssetViewModels();

const rows = buildStationAssetRows({
  stations: [
    station({ id: "station-a", name: "Station A", balanceCny: 99, lowBalanceThresholdCny: 10 }),
    station({ id: "station-b", name: "Station B", balanceCny: 6, lowBalanceThresholdCny: 8 }),
  ],
  keysByStation: new Map([["station-a", [stationKey({ id: "key-a", stationId: "station-a" })]]]),
  balances: [
    balance({ id: "key-newer", stationId: "station-a", stationKeyId: "key-a", scope: "station_key", value: 100, updatedAt: "2026-07-08T05:00:00.000Z" }),
    balance({ id: "station-current", stationId: "station-a", scope: "station", value: 13, status: "low", updatedAt: "2026-07-08T04:00:00.000Z" }),
  ],
  snapshotsByStation: new Map(),
  groupBindingsByStation: new Map([
    [
      "station-a",
      [
        binding({ id: "binding-current", stationId: "station-a", groupName: "current", effectiveRateMultiplier: 0.8 }),
        binding({ id: "binding-missing", stationId: "station-a", groupName: "missing", bindingStatus: "missing", effectiveRateMultiplier: 0.1 }),
      ],
    ],
  ]),
  groupRatesByStation: new Map([
    [
      "station-a",
      [
        rate({ id: "rate-current", stationId: "station-a", groupBindingId: "binding-current", groupName: "current", effectiveRateMultiplier: 0.7, checkedAt: "2026-07-08T04:00:00.000Z" }),
      ],
    ],
  ]),
  changes: [],
});

assert.deepEqual(
  rows[0].rateChips.map((chip) => ({ label: chip.label, value: chip.value, tone: chip.tone })),
  [{ label: "current", value: "0.80x", tone: "good" }],
  "station assets should build rate chips from shared current group facts and hide missing groups",
);
assert.equal(formatStationBalance(rows[0]), "CNY 13.00", "station asset balance should prefer station-scope current balance");
assert.equal(formatStationBalance(rows[1]), "CNY 6.00", "station asset balance should fallback to station cache");

const assetSource = await readFile("src/features/stations/stationAssetViewModels.ts", "utf8");
assert.ok(
  assetSource.includes("buildCurrentStationGroupFacts") &&
    assetSource.includes("isDisplayableStationGroupCurrentFact") &&
    assetSource.includes("buildCurrentStationBalanceFacts"),
  "station asset rows should consume shared group and balance projections",
);

function station(overrides) {
  return {
    id: "station",
    name: "Station",
    stationType: "sub2api",
    baseUrl: "https://station.example.test",
    apiKeyMasked: "sk-...",
    apiKeyPresent: true,
    keyCount: 1,
    enabled: true,
    priority: 0,
    creditPerCny: 10,
    balanceRaw: null,
    balanceCny: null,
    lowBalanceThresholdCny: null,
    collectionIntervalMinutes: 5,
    status: "healthy",
    latencyMs: null,
    lastCheckedAt: null,
    lastPricingFetchedAt: null,
    note: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}

function stationKey(overrides) {
  return {
    id: "key",
    stationId: "station",
    name: "Key",
    apiKeyMasked: "sk-...",
    apiKeyPresent: true,
    enabled: true,
    priority: 0,
    groupBindingId: null,
    groupIdHash: null,
    groupName: null,
    tierLabel: null,
    rateMultiplier: null,
    rateSource: null,
    rateCollectedAt: null,
    balanceScope: null,
    status: "healthy",
    lastCheckedAt: null,
    lastUsedAt: null,
    note: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}

function binding(overrides) {
  return {
    id: "binding",
    stationId: "station",
    stationKeyId: null,
    bindingKind: "station_group",
    parentGroupBindingId: null,
    groupKeyHash: "group-key",
    groupIdHash: null,
    groupName: "group",
    bindingStatus: "available",
    defaultRateMultiplier: null,
    userRateMultiplier: null,
    effectiveRateMultiplier: null,
    rateSource: "sub2api_groups_rates",
    confidence: 1,
    lastSeenAt: null,
    lastCheckedAt: null,
    lastRateChangedAt: null,
    rawJsonRedacted: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}

function rate(overrides) {
  return {
    id: "rate",
    stationId: "station",
    stationKeyId: null,
    groupBindingId: null,
    bindingKind: "station_group",
    groupKeyHash: "group-key",
    groupName: "group",
    defaultRateMultiplier: null,
    userRateMultiplier: null,
    effectiveRateMultiplier: null,
    source: "sub2api_groups_rates",
    confidence: 1,
    rawJsonRedacted: null,
    checkedAt: "2026-07-08T00:00:00.000Z",
    createdAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}

function balance(overrides) {
  return {
    id: "balance",
    stationId: "station",
    stationKeyId: null,
    scope: "station",
    value: 1,
    currency: "CNY",
    creditUnit: null,
    usedValue: null,
    totalValue: null,
    lowBalanceThreshold: null,
    status: "normal",
    source: "station_balance",
    confidence: 1,
    collectedAt: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}
```

- [ ] **Step 3: Verify RED**

Run:

```powershell
node scripts/station-detail-group-source.test.mjs
node scripts/station-assets-current-projections.test.mjs
```

Expected:

- `station-detail-group-source.test.mjs` fails because the view model still keeps page-local group de-duplication.
- `station-assets-current-projections.test.mjs` fails because the view model has no `groupRatesByStation` input and does not consume shared projections.

- [ ] **Step 4: Commit RED tests**

```powershell
git add -- scripts/station-detail-group-source.test.mjs scripts/station-assets-current-projections.test.mjs
git diff --cached --name-only
git commit -m "test: guard station asset current projections"
```

## Task 7: Migrate Station Detail View Model To Current Projections

**Files:**
- Modify: `src/features/stations/stationDetailViewModels.ts`

- [ ] **Step 1: Replace group row construction**

In `src/features/stations/stationDetailViewModels.ts`, import:

```ts
import {
  buildCurrentStationGroupFacts,
  isDisplayableStationGroupCurrentFact,
  type StationGroupCurrentFact,
} from "@/lib/projections/groupFacts";
import { currentStationBalanceFor, type StationBalanceCurrentFact } from "@/lib/projections/balanceFacts";
```

Replace `buildBalanceCards(station, balances)` internals so it first calls:

```ts
const currentBalance = currentStationBalanceFor({ station, balances });
```

Then use `currentBalance.value`, `currentBalance.currency`, `currentBalance.lowBalanceThreshold`, `currentBalance.status`, `currentBalance.updatedAt`, and `currentBalance.collectedAt`.

Replace `buildGroupRows()` with:

```ts
export function buildGroupRows(
  bindings: StationGroupBinding[],
  rates: GroupRateRecord[],
): StationDetailGroupRow[] {
  return buildCurrentStationGroupFacts({ bindings, rates })
    .filter(isDisplayableStationGroupCurrentFact)
    .map(groupRowFromCurrentFact);
}

function groupRowFromCurrentFact(fact: StationGroupCurrentFact): StationDetailGroupRow {
  const defaultRate = fact.sourceRate?.defaultRateMultiplier ?? fact.sourceBinding?.defaultRateMultiplier ?? null;
  const userRate = fact.sourceRate?.userRateMultiplier ?? fact.sourceBinding?.userRateMultiplier ?? null;
  const warning = groupWarningForFact(fact);
  return {
    id: fact.groupBindingId ?? fact.identityKey,
    groupName: fact.groupName || "未命名分组",
    rawJsonRedacted: fact.sourceRate?.rawJsonRedacted ?? fact.sourceBinding?.rawJsonRedacted ?? null,
    effectiveRate: formatRate(fact.rateMultiplier, "未确定"),
    defaultRate: formatRate(defaultRate),
    userRate: formatRate(userRate, "未覆盖"),
    bindingStatus: formatBindingStatusLabel(fact.bindingStatus),
    sourceLabel: formatRateSourceLabel(fact.rateSource ?? "binding"),
    lastChecked: formatDetailDate(fact.rateCheckedAt ?? fact.sourceBinding?.updatedAt ?? null),
    tone: warning ? "warning" : "good",
    warning,
  };
}

function groupWarningForFact(fact: StationGroupCurrentFact) {
  if (fact.rateMultiplier == null || !Number.isFinite(fact.rateMultiplier)) {
    return "缺少倍率";
  }
  if (fact.rateMultiplier === 0) {
    return "倍率为 0";
  }
  return null;
}
```

Remove these page-local helpers after the replacement:

```text
dedupeStationGroupBindings
preferStationGroupBinding
stationGroupBindingScore
normalizeGroupName
```

- [ ] **Step 2: Verify station detail GREEN**

Run:

```powershell
node scripts/station-detail-group-source.test.mjs
node scripts/group-facts-projection.test.mjs
node scripts/station-current-balance-projection.test.mjs
```

Expected: all exit 0.

- [ ] **Step 3: Commit station detail migration**

```powershell
git add -- src/features/stations/stationDetailViewModels.ts
git diff --cached --name-only
git commit -m "refactor: consume current facts in station detail"
```

## Task 8: Migrate Station Asset View Model To Current Projections

**Files:**
- Modify: `src/features/stations/stationAssetViewModels.ts`

- [ ] **Step 1: Update input and row type**

In `src/features/stations/stationAssetViewModels.ts`, import:

```ts
import {
  buildCurrentStationGroupFacts,
  isDisplayableStationGroupCurrentFact,
} from "@/lib/projections/groupFacts";
import {
  buildCurrentStationBalanceFacts,
  type StationBalanceCurrentFact,
} from "@/lib/projections/balanceFacts";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
```

Add to `StationAssetRow`:

```ts
currentBalance: StationBalanceCurrentFact;
```

Add to `buildStationAssetRows()` input:

```ts
groupRatesByStation?: Map<string, GroupRateRecord[]>;
```

Inside `buildStationAssetRows()`, replace `latestBalanceByStation` with:

```ts
const currentBalancesByStation = buildCurrentStationBalanceFacts({ stations, balances });
```

Then set each row:

```ts
const currentBalance = currentBalancesByStation.get(station.id) ?? null;
const groupBindings = groupBindingsByStation.get(station.id) ?? [];
const groupRates = groupRatesByStation?.get(station.id) ?? [];
```

Return:

```ts
latestBalance: currentBalance?.sourceSnapshot ?? null,
currentBalance: currentBalance ?? buildCurrentStationBalanceFacts({ stations: [station], balances: [] }).get(station.id)!,
rateChips: rateChipsForStation(groupBindings, groupRates, snapshotsByStation.get(station.id) ?? null),
```

- [ ] **Step 2: Replace rate chip construction**

Replace `rateChipsForStation()` and `rateChipsFromBindings()` with:

```ts
function rateChipsForStation(
  bindings: StationGroupBinding[],
  rates: GroupRateRecord[],
  snapshot: CollectorSnapshot | null,
): RateChip[] {
  const currentFactChips = rateChipsFromCurrentFacts(bindings, rates);
  return currentFactChips.length > 0 ? currentFactChips : extractRateChips(snapshot);
}

export function rateChipsFromCurrentFacts(
  bindings: StationGroupBinding[],
  rates: GroupRateRecord[],
): RateChip[] {
  return buildCurrentStationGroupFacts({ bindings, rates })
    .filter(isDisplayableStationGroupCurrentFact)
    .slice(0, 3)
    .map((fact) => ({
      label: fact.groupName,
      value: typeof fact.rateMultiplier === "number" ? `${fact.rateMultiplier.toFixed(2)}x` : "-",
      tone: fact.rateMultiplier == null ? "warning" : fact.rateMultiplier < 1 ? "good" : "neutral",
    }));
}
```

Update `formatStationBalance(row)`:

```ts
export function formatStationBalance(row: StationAssetRow) {
  const value = row.currentBalance.value;
  if (value == null) {
    return "未采集";
  }
  return `${row.currentBalance.currency} ${value.toFixed(2)}`;
}
```

- [ ] **Step 3: Verify station asset GREEN**

Run:

```powershell
node scripts/station-assets-current-projections.test.mjs
node scripts/station-asset-loading-boundary.test.mjs
node scripts/station-asset-selection.test.mjs
```

Expected: all exit 0.

- [ ] **Step 4: Commit station asset migration**

```powershell
git add -- src/features/stations/stationAssetViewModels.ts
git diff --cached --name-only
git commit -m "refactor: consume current facts in station assets"
```

## Task 9: Stage 5 Verification And Audit

**Files:**
- Create: `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage5-progress.md`

- [ ] **Step 1: Run full Stage 5 verification**

Run:

```powershell
node scripts/station-current-balance-projection.test.mjs
node scripts/group-facts-projection.test.mjs
node scripts/station-detail-group-source.test.mjs
node scripts/station-assets-current-projections.test.mjs
node scripts/station-asset-loading-boundary.test.mjs
node scripts/station-asset-selection.test.mjs
node scripts/test-dashboard-balance-summary.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
pnpm.cmd build
git status --short
```

Expected:

- Node scripts exit 0.
- `pnpm.cmd build` exits 0 with only the existing Vite chunk-size warning.
- `git status --short` is clean before writing the audit.

- [ ] **Step 2: Write progress audit**

Create `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage5-progress.md`:

```md
# Relay Pool 数据架构 Stage 5 进度审计

日期：2026-07-08

## 范围

Stage 5 迁移站点详情 group rows、站点资产 rate chips、站点余额展示到 shared current projections。页面视觉结构不变，不删除字段，不改 schema。

## 已完成

- `src/lib/projections/balanceFacts.ts`
- `buildCurrentStationBalanceFacts()`
- `currentStationBalanceFor()`
- `isDisplayableStationGroupCurrentFact()`
- `stationDetailViewModels.ts` 消费 current group facts 和 current balance facts
- `stationAssetViewModels.ts` 消费 current group facts 和 current balance facts

## 字段审计

- 本阶段未新增 schema 字段。
- `stations.balance_cny` 和 `stations.low_balance_threshold_cny` 仍是 compatibility cache，仅在没有 station-scope balance snapshot 时 fallback。
- `group_rate_records` 仍是 evidence/history，不直接复活 missing/disabled group。
- 字段归属清单无需新增 `unknown pending audit` 行。

## 验证

记录完整命令和结果。

## 下一步

进入 Stage 6：Key Pool 与 Add Provider 迁移。先做 drift intake，再创建 Stage 6 计划。
```

- [ ] **Step 3: Commit audit**

```powershell
git add -- docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage5-progress.md
git diff --cached --name-only
git commit -m "docs: summarize data architecture stage5 progress"
```

- [ ] **Step 4: Update rolling heartbeat**

Use `automation_update` for `relay-pool` to record:

- Latest worktree HEAD.
- Stage 5 commits.
- Verification results.
- Next stage: Stage 6 Key Pool / Add Provider migration.
- Current blocker if any.

## Self Review

- Spec coverage: Stage 5 moves station detail groups, station asset chips, and balance display to current projections without changing schema or page layout.
- Placeholder scan: this plan avoids open placeholders and gives exact commands, expected failures, and commit paths.
- Type consistency: `StationBalanceCurrentFact`, `StationGroupCurrentFact`, `groupRatesByStation`, and `currentBalance` are introduced before consumers use them.
