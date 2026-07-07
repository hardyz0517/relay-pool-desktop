# Relay Pool Data Architecture Stage 4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move pricing candidate construction into a shared projection layer while preserving the current pricing page UI and model/group matching behavior.

**Architecture:** Stage 4 builds `src/lib/projections/pricingFacts.ts` on top of Stage 3 `groupFacts.ts`. The feature view model keeps catalog filtering, model matching, row sorting, and UI-facing labels at first, but no longer owns binding/rate de-duplication or raw current group fallback. `pricingRules` become an explicit projection input with a conservative fallback role only when current group facts have no multiplier.

**Tech Stack:** TypeScript pure projections, React view model, Node script tests, Vite/TypeScript build.

---

## Entry Gate

- Worktree must be `D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`.
- Worktree branch must be `codex/data-architecture-stage0`.
- Read first:
  - `docs/superpowers/specs/2026-07-07-relay-pool-data-architecture-master-spec.md`
  - `docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-master-plan.md`
  - `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`
  - `docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage3-progress.md`
  - this file
- Run drift intake:

```powershell
git status --short
git log --oneline -8
git -C D:\Dev\Projects\relay-pool-desktop status --short
git -C D:\Dev\Projects\relay-pool-desktop log --oneline -8
```

- If main checkout has new uncommitted pricing, group facts, station detail, collector, runtime, or schema changes, stop and record an intake blocker.
- If main checkout only has committed changes, merge `master`, then rerun:

```powershell
node scripts/group-facts-projection.test.mjs
node scripts/pricing-comparison-view-model.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
pnpm.cmd build
```

## Files

- Create: `src/lib/projections/pricingFacts.ts`
- Create: `scripts/pricing-facts-projection.test.mjs`
- Modify: `src/features/pricing/pricingComparisonViewModel.ts`
- Modify: `scripts/pricing-comparison-view-model.test.mjs`
- Modify: `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage4-progress.md`
- Do not modify: `src/features/pricing/PricingPage.tsx` visual structure unless a source guard fails because of the view model import path.
- Do not modify Rust schema or database code in Stage 4.

## Projection Contract

Create `src/lib/projections/pricingFacts.ts` with this public API:

```ts
import type { PricingRule } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationKey } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import type { StationGroupCurrentFact } from "@/lib/projections/groupFacts";

export type PricingGroupCandidate = {
  identityKey: string;
  station: Station;
  stationKeyId: string | null;
  stationKeyName: string | null;
  groupBindingId: string | null;
  groupRateRecordId: string | null;
  groupKeyHash: string;
  groupIdHash: string | null;
  groupName: string;
  groupRawJsonRedacted: Record<string, unknown> | null;
  groupMultiplier: number | null;
  pricingRuleId: string | null;
  source: string;
  checkedAt: string | null;
  currentFact: StationGroupCurrentFact;
};

export function buildPricingGroupCandidates(input: {
  stations: Station[];
  stationKeys?: StationKey[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  pricingRules: PricingRule[];
}): PricingGroupCandidate[];
```

Rules:

- Use `buildCurrentStationGroupFacts()` as the only source of current group identity and rate fallback.
- Only available `station_group` facts produce pricing candidates.
- Preserve `rate_only` candidates when they are available and have a known station.
- `pricingRules` are indexed, not ignored. A matching enabled rule may provide `groupMultiplier` only when `currentFact.rateMultiplier` is `null` and `rule.rateMultiplier` is finite.
- A matching pricing rule is selected by `stationId`, then `groupBindingId` when present, then normalized `groupName`, and must be `enabled`.
- Do not use `pricingRules.inputPrice` or `pricingRules.outputPrice` in Stage 4; page estimates still use official catalog price multiplied by current effective multiplier.

## Task 1: RED Test For Pricing Projection Candidates

**Files:**
- Create: `scripts/pricing-facts-projection.test.mjs`

- [ ] **Step 1: Write the failing test**

Create `scripts/pricing-facts-projection.test.mjs`:

```js
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

async function importTsModule(path) {
  const source = await readFile(path, "utf8");
  const output = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      verbatimModuleSyntax: true,
    },
  }).outputText;
  return import(`data:text/javascript;base64,${Buffer.from(output, "utf8").toString("base64")}`);
}

const { buildPricingGroupCandidates } = await importTsModule("src/lib/projections/pricingFacts.ts");

const candidates = buildPricingGroupCandidates({
  stations: [station("station-a", "Station A", 10)],
  stationKeys: [stationKey("station-a", "key-a", "Key A")],
  groupBindings: [
    binding({
      id: "binding-current",
      stationId: "station-a",
      stationKeyId: "key-a",
      groupKeyHash: "local-current",
      groupIdHash: "remote-current",
      groupName: "default",
      userRateMultiplier: null,
      effectiveRateMultiplier: 0.8,
      defaultRateMultiplier: 1,
      bindingStatus: "available",
    }),
    binding({
      id: "binding-rule-fallback",
      stationId: "station-a",
      groupKeyHash: "local-rule",
      groupIdHash: "remote-rule",
      groupName: "rule only",
      userRateMultiplier: null,
      effectiveRateMultiplier: null,
      defaultRateMultiplier: null,
      bindingStatus: "available",
    }),
    binding({
      id: "binding-missing",
      stationId: "station-a",
      groupKeyHash: "local-missing",
      groupIdHash: "remote-missing",
      groupName: "missing group",
      userRateMultiplier: null,
      effectiveRateMultiplier: null,
      defaultRateMultiplier: 1,
      bindingStatus: "missing",
    }),
  ],
  groupRates: [
    rate({
      id: "rate-current",
      stationId: "station-a",
      groupBindingId: "binding-current",
      groupKeyHash: "local-current",
      groupName: "default",
      effectiveRateMultiplier: 0.7,
      checkedAt: "2026-07-08T01:00:00.000Z",
    }),
    rate({
      id: "rate-shadow",
      stationId: "station-a",
      groupBindingId: null,
      groupKeyHash: "local-current",
      groupName: "default",
      effectiveRateMultiplier: 0.7,
      checkedAt: "2026-07-08T02:00:00.000Z",
    }),
    rate({
      id: "rate-missing",
      stationId: "station-a",
      groupBindingId: "binding-missing",
      groupKeyHash: "local-missing",
      groupName: "missing group",
      effectiveRateMultiplier: 0.01,
      checkedAt: "2026-07-08T03:00:00.000Z",
    }),
  ],
  pricingRules: [
    pricingRule({
      id: "rule-fallback",
      stationId: "station-a",
      groupBindingId: "binding-rule-fallback",
      groupName: "rule only",
      model: "gpt-5-mini",
      rateMultiplier: 0.42,
      enabled: true,
    }),
  ],
});

assert.deepEqual(
  candidates.map((candidate) => ({
    identityKey: candidate.identityKey,
    stationName: candidate.station.name,
    stationKeyName: candidate.stationKeyName,
    groupBindingId: candidate.groupBindingId,
    groupRateRecordId: candidate.groupRateRecordId,
    groupName: candidate.groupName,
    groupMultiplier: candidate.groupMultiplier,
    pricingRuleId: candidate.pricingRuleId,
  })),
  [
    {
      identityKey: "binding:binding-current",
      stationName: "Station A",
      stationKeyName: "Key A",
      groupBindingId: "binding-current",
      groupRateRecordId: "rate-current",
      groupName: "default",
      groupMultiplier: 0.8,
      pricingRuleId: null,
    },
    {
      identityKey: "binding:binding-rule-fallback",
      stationName: "Station A",
      stationKeyName: null,
      groupBindingId: "binding-rule-fallback",
      groupRateRecordId: null,
      groupName: "rule only",
      groupMultiplier: 0.42,
      pricingRuleId: "rule-fallback",
    },
  ],
  "pricing candidates should reuse current group facts, suppress shadow rates, hide missing groups, and use pricingRules only as multiplier fallback",
);

const projectionSource = await readFile("src/lib/projections/pricingFacts.ts", "utf8");
assert.ok(
  projectionSource.includes("buildCurrentStationGroupFacts"),
  "pricing projection should consume the shared current group projection",
);
assert.ok(
  !projectionSource.includes("from \"@/features/") &&
    !projectionSource.includes("invoke<") &&
    !projectionSource.includes("getLocalAccessKey") &&
    !projectionSource.includes("upsertPricingRule"),
  "pricing projection should stay pure and must not import feature modules, call Tauri, read secrets, or write pricing rules",
);

function station(id, name, creditPerCny) {
  return {
    id,
    name,
    stationType: "sub2api",
    baseUrl: `https://${id}.example.test`,
    apiKeyMasked: "sk-...",
    apiKeyPresent: true,
    keyCount: 1,
    enabled: true,
    priority: 0,
    creditPerCny,
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
  };
}

function stationKey(stationId, id, name) {
  return {
    id,
    stationId,
    name,
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
    rateSource: "test",
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
    source: "test",
    confidence: 1,
    rawJsonRedacted: null,
    checkedAt: "2026-07-08T00:00:00.000Z",
    createdAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}

function pricingRule(overrides) {
  return {
    id: "rule",
    stationId: "station",
    stationKeyId: null,
    groupBindingId: null,
    groupName: null,
    tierLabel: null,
    model: "gpt-5-mini",
    inputPrice: null,
    outputPrice: null,
    fixedPrice: null,
    rateMultiplier: null,
    currency: "CNY",
    unit: "multiplier",
    priceType: "rate_multiplier",
    basePriceSource: null,
    normalizationStatus: "normalized",
    source: "test",
    confidence: 1,
    enabled: true,
    note: null,
    collectedAt: "2026-07-08T00:00:00.000Z",
    validFrom: null,
    validUntil: null,
    createdAt: "2026-07-08T00:00:00.000Z",
    updatedAt: "2026-07-08T00:00:00.000Z",
    ...overrides,
  };
}
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
node scripts/pricing-facts-projection.test.mjs
```

Expected: FAIL with `ENOENT` for `src/lib/projections/pricingFacts.ts`.

- [ ] **Step 3: Commit RED test**

```powershell
git add -- scripts/pricing-facts-projection.test.mjs
git commit -m "test: guard pricing projection candidates"
```

## Task 2: Implement Pricing Projection Candidate Builder

**Files:**
- Create: `src/lib/projections/pricingFacts.ts`

- [ ] **Step 1: Create projection implementation**

Create `src/lib/projections/pricingFacts.ts`:

```ts
import { buildCurrentStationGroupFacts, type StationGroupCurrentFact } from "@/lib/projections/groupFacts";
import type { PricingRule } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationKey } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";

export type PricingGroupCandidate = {
  identityKey: string;
  station: Station;
  stationKeyId: string | null;
  stationKeyName: string | null;
  groupBindingId: string | null;
  groupRateRecordId: string | null;
  groupKeyHash: string;
  groupIdHash: string | null;
  groupName: string;
  groupRawJsonRedacted: Record<string, unknown> | null;
  groupMultiplier: number | null;
  pricingRuleId: string | null;
  source: string;
  checkedAt: string | null;
  currentFact: StationGroupCurrentFact;
};

export function buildPricingGroupCandidates(input: {
  stations: Station[];
  stationKeys?: StationKey[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  pricingRules: PricingRule[];
}): PricingGroupCandidate[] {
  const stationsById = new Map(input.stations.map((station) => [station.id, station]));
  const stationKeyNameById = new Map((input.stationKeys ?? []).map((key) => [key.id, key.name]));
  const pricingRules = input.pricingRules.filter((rule) => rule.enabled);

  return buildCurrentStationGroupFacts({
    bindings: input.groupBindings,
    rates: input.groupRates,
  })
    .filter((fact) => fact.available && fact.bindingKind === "station_group")
    .flatMap((fact) => {
      const station = stationsById.get(fact.stationId);
      if (!station) {
        return [];
      }
      const matchingRule = firstMatchingPricingRule(fact, pricingRules);
      const ruleMultiplier = firstFiniteNumber(matchingRule?.rateMultiplier);
      const groupMultiplier = fact.rateMultiplier ?? ruleMultiplier;
      const stationKeyId = fact.stationKeyId ?? null;
      return [
        {
          identityKey: fact.identityKey,
          station,
          stationKeyId,
          stationKeyName: stationKeyId ? stationKeyNameById.get(stationKeyId) ?? null : null,
          groupBindingId: fact.groupBindingId,
          groupRateRecordId: fact.rateEvidenceId,
          groupKeyHash: fact.groupKeyHash ?? "",
          groupIdHash: fact.groupIdHash,
          groupName: fact.groupName,
          groupRawJsonRedacted: fact.sourceRate?.rawJsonRedacted ?? fact.sourceBinding?.rawJsonRedacted ?? null,
          groupMultiplier,
          pricingRuleId: fact.rateMultiplier === null ? matchingRule?.id ?? null : null,
          source: fact.rateSource ?? matchingRule?.source ?? "station_group_current_fact",
          checkedAt: fact.rateCheckedAt ?? matchingRule?.collectedAt ?? null,
          currentFact: fact,
        },
      ];
    });
}

function firstMatchingPricingRule(
  fact: StationGroupCurrentFact,
  pricingRules: PricingRule[],
) {
  return pricingRules.find((rule) => {
    if (rule.stationId !== fact.stationId) {
      return false;
    }
    if (fact.groupBindingId && rule.groupBindingId) {
      return rule.groupBindingId === fact.groupBindingId;
    }
    if (rule.groupName && normalizedName(rule.groupName) === normalizedName(fact.groupName)) {
      return true;
    }
    return false;
  }) ?? null;
}

function firstFiniteNumber(...values: Array<number | null | undefined>) {
  for (const value of values) {
    if (typeof value === "number" && Number.isFinite(value)) {
      return value;
    }
  }
  return null;
}

function normalizedName(value: string) {
  return value.trim().toLowerCase();
}
```

- [ ] **Step 2: Verify GREEN**

Run:

```powershell
node scripts/pricing-facts-projection.test.mjs
```

Expected: exit 0.

- [ ] **Step 3: Commit implementation**

```powershell
git add -- src/lib/projections/pricingFacts.ts
git commit -m "refactor: add pricing projection candidates"
```

## Task 3: Migrate Pricing View Model To Projection Candidates

**Files:**
- Modify: `src/features/pricing/pricingComparisonViewModel.ts`
- Modify: `scripts/pricing-comparison-view-model.test.mjs`

- [ ] **Step 1: Add RED source guard to pricing comparison test**

Append to `scripts/pricing-comparison-view-model.test.mjs` near the existing `pageSource` source assertions:

```js
const viewModelSource = await readFile("src/features/pricing/pricingComparisonViewModel.ts", "utf8");
assert.ok(
  viewModelSource.includes("buildPricingGroupCandidates"),
  "pricing comparison view model should consume shared pricing projection candidates",
);
assert.ok(
  !viewModelSource.includes("void input.pricingRules"),
  "pricing comparison view model must pass pricingRules into the shared projection instead of ignoring them",
);
assert.ok(
  !viewModelSource.includes("function bindingCandidate(") &&
    !viewModelSource.includes("function rateCandidate(") &&
    !viewModelSource.includes("function isRateForBinding(") &&
    !viewModelSource.includes("function latestRatesByStationGroup("),
  "pricing comparison view model should not keep page-local group/rate projection helpers after Stage 4",
);
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
node scripts/pricing-comparison-view-model.test.mjs
```

Expected: FAIL on `buildPricingGroupCandidates` or `void input.pricingRules`.

- [ ] **Step 3: Migrate view model**

In `src/features/pricing/pricingComparisonViewModel.ts`:

1. Import the projection:

```ts
import {
  buildPricingGroupCandidates,
  type PricingGroupCandidate,
} from "../../lib/projections/pricingFacts";
```

2. Replace `type GroupCandidate = { ... }` with:

```ts
type GroupCandidate = PricingGroupCandidate;
```

3. Remove `void input.pricingRules;`.

4. In `buildPricingComparisonViewModel()`, create candidates once:

```ts
  const pricingCandidates = buildPricingGroupCandidates({
    stations: input.stations,
    stationKeys: input.stationKeys,
    groupBindings: input.groupBindings,
    groupRates: input.groupRates,
    pricingRules: input.pricingRules,
  });
```

5. Change `buildRowsForModel()` signature to accept `pricingCandidates: PricingGroupCandidate[]` instead of `groupBindings`, `groupRates`, `stationsById`, and `stationKeyNameById`.

6. Replace its body with:

```ts
function buildRowsForModel(
  model: PricingComparisonCatalogEntry,
  pricingCandidates: PricingGroupCandidate[],
  evidenceByStationModel: Map<string, PricingEvidenceStatus>,
) {
  return pricingCandidates
    .filter((candidate) => groupCandidateMatchesModel(candidate, model))
    .map((candidate) => createRowFromCandidate(model, candidate, evidenceByStationModel))
    .sort(compareRows);
}
```

7. Add this matcher:

```ts
function groupCandidateMatchesModel(
  candidate: PricingGroupCandidate,
  model: PricingComparisonCatalogEntry,
) {
  const platform = groupPlatformFromRawJson(candidate.groupRawJsonRedacted);
  if (platform) {
    return platformMatchesProvider(platform, model.provider);
  }
  const groupType = candidate.groupIdHash?.trim() ?? "";
  if (groupType) {
    return groupTypeMatchesModel(candidate.station.id, groupType, candidate.groupName, model);
  }
  return legacyGroupTextMatchesModel(
    [candidate.groupName, candidate.source, searchableJsonText(candidate.groupRawJsonRedacted)].join(" "),
    model.groupMatchers,
  );
}
```

8. Remove these page-local helpers:

```ts
bindingCandidate
rateCandidate
isStationGroupBinding
isStationGroupRate
isRateForBinding
selectRateForBinding
isRateBackedByActiveGroup
latestRatesByStationGroup
```

9. Keep `groupPlatformFromRawJson`, `platformMatchesProvider`, `groupTypeMatchesModel`, `legacyGroupTextMatchesModel`, `searchableJsonText`, and sorting/metrics helpers in the view model for now. They are model-classification and UI view-model concerns, not current fact ownership.

- [ ] **Step 4: Verify GREEN**

Run:

```powershell
node scripts/pricing-comparison-view-model.test.mjs
node scripts/pricing-facts-projection.test.mjs
node scripts/group-facts-projection.test.mjs
```

Expected: all exit 0.

- [ ] **Step 5: Run boundary checks and build**

Run:

```powershell
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
pnpm.cmd build
```

Expected:

- Node scripts exit 0.
- `pnpm.cmd build` exit 0 with only the existing Vite chunk-size warning.

- [ ] **Step 6: Commit migration**

```powershell
git add -- src/features/pricing/pricingComparisonViewModel.ts scripts/pricing-comparison-view-model.test.mjs
git commit -m "refactor: consume pricing projection candidates"
```

## Task 4: Stage 4 Progress Audit And Rolling Handoff

**Files:**
- Create: `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage4-progress.md`

- [ ] **Step 1: Write progress audit**

Create `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage4-progress.md`:

```md
# Relay Pool 数据架构 Stage 4 进度审计

日期：2026-07-08

## 范围

Stage 4 把价格候选构建迁到 `src/lib/projections/pricingFacts.ts`，价格页视觉结构不变，不删除字段，不改 schema。

## 已完成

- `buildPricingGroupCandidates()`
- `scripts/pricing-facts-projection.test.mjs`
- `src/features/pricing/pricingComparisonViewModel.ts` 消费 shared pricing projection candidates

## pricingRules 处理

- `pricingRules` 不再被 `void input.pricingRules` 忽略。
- Stage 4 仅允许 enabled matching rule 在 current group fact 没有倍率时提供 `rateMultiplier` fallback。
- Stage 4 不使用 `inputPrice` / `outputPrice` 覆盖官方目录价格。

## 验证

```powershell
node scripts/pricing-facts-projection.test.mjs
node scripts/pricing-comparison-view-model.test.mjs
node scripts/group-facts-projection.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
pnpm.cmd build
```

## Drift

记录执行时的工作树 HEAD、主 checkout HEAD、主 checkout dirty paths。若主 checkout 出现未提交 pricing / group facts / collector 改动，本文必须写明未接入。
```

- [ ] **Step 2: Run final verification**

Run:

```powershell
node scripts/pricing-facts-projection.test.mjs
node scripts/pricing-comparison-view-model.test.mjs
node scripts/group-facts-projection.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
pnpm.cmd build
git status --short
```

Expected:

- All commands exit 0 except `git status --short`, which should only show the new audit file before commit.
- Build may keep the existing Vite chunk-size warning.

- [ ] **Step 3: Commit audit**

```powershell
git add -- docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage4-progress.md
git commit -m "docs: summarize data architecture stage4 progress"
```

- [ ] **Step 4: Update rolling heartbeat**

Use `automation_update` for `relay-pool` to record:

- Latest worktree HEAD.
- Stage 4 commits.
- Verification results.
- Next stage: Stage 5 station detail / station assets migration.
- Current blocker if any.

## Self Review

- Spec coverage: Stage 4 moves current pricing candidate construction behind shared projection, reuses Stage 3 current group facts, keeps pricing page visual structure, and removes `void input.pricingRules`.
- Placeholder scan: no TBD / TODO / later placeholders.
- Type consistency: `PricingGroupCandidate` intentionally mirrors the old `GroupCandidate` shape and adds `identityKey`, `groupIdHash`, `pricingRuleId`, and `currentFact`.
