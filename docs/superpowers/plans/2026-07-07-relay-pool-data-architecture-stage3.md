# Relay Pool Data Architecture Stage 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a shared current-group projection that centralizes station group identity, rate fallback, and missing/disabled handling without migrating page consumers yet.

**Architecture:** Stage 3 creates a pure TypeScript projection module under `src/lib/projections/` and source/behavior tests under `scripts/`. It must not delete fields, change schema, or consume main checkout dirty changes implicitly. Page migrations stay for later stages after this projection is verified.

**Tech Stack:** React/Tauri app, TypeScript pure functions, Node script tests, Vite/TypeScript build.

---

## Hard Entry Gate

Do not execute code tasks until this gate passes:

- Main checkout dirty changes touching pricing, station detail, station group visual metadata, collector apply, or `src/lib/types/groupFacts.ts` have been committed and merged into this worktree, or the user explicitly approves a patch-based intake for exact paths.
- After intake, rerun Stage 0/1/2 guards before editing production code.
- If `src/lib/types/groupFacts.ts` changed, update this plan's type references before writing tests.

Required intake commands:

```powershell
git branch --show-current
git status --short
git log --oneline -8
git -C D:\Dev\Projects\relay-pool-desktop status --short
git -C D:\Dev\Projects\relay-pool-desktop log --oneline -8
```

Expected before implementation:

- Worktree branch is `codex/data-architecture-stage0`.
- Worktree status is clean.
- Main checkout has no uncommitted high-overlap dirty files, or the user has explicitly approved exact patch intake.

## Files

- Create: `src/lib/projections/groupFacts.ts`
- Create: `scripts/group-facts-projection.test.mjs`
- Modify: `scripts/data-architecture-field-ownership.test.mjs` only if the new projection path must be allowlisted for compatibility field read-through.
- Modify: `docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage3-progress.md`
- Do not modify page consumers in Stage 3.
- Do not modify Rust schema or database code in Stage 3.

## Projection Contract

Create this public API:

```ts
import type { GroupRateRecord, StationGroupBinding, StationGroupOption } from "@/lib/types/groupFacts";

export type StationGroupCurrentFact = {
  identityKey: string;
  groupBindingId: string | null;
  stationId: string;
  stationKeyId: string | null;
  bindingKind: string;
  groupKeyHash: string | null;
  groupIdHash: string | null;
  groupName: string;
  bindingStatus: string;
  available: boolean;
  rateMultiplier: number | null;
  rateSource: string | null;
  rateEvidenceId: string | null;
  rateCheckedAt: string | null;
  sourceBinding: StationGroupBinding | null;
  sourceRate: GroupRateRecord | null;
};

export function buildCurrentStationGroupFacts(input: {
  bindings: StationGroupBinding[];
  rates: GroupRateRecord[];
}): StationGroupCurrentFact[];

export function latestGroupRatesByBindingOrHash(
  rates: GroupRateRecord[],
): Map<string, GroupRateRecord>;

export function buildStationGroupOptionsFromCurrentFacts(
  facts: StationGroupCurrentFact[],
): StationGroupOption[];
```

Rules:

- Identity fallback order: `group_binding_id` -> `group_key_hash` -> `group_id_hash` -> normalized `group_name`.
- `group_key_hash` and `group_id_hash` are not interchangeable.
- Rate fallback order:
  1. Binding `user_rate_multiplier`
  2. Binding `effective_rate_multiplier`
  3. Latest rate `user_rate_multiplier`
  4. Latest rate `effective_rate_multiplier`
  5. Binding `default_rate_multiplier`
  6. Latest rate `default_rate_multiplier`
  7. `null`
- `missing` and `disabled` bindings stay unavailable even when historical rates exist.
- A binding and a rate record with the same current identity must produce one current fact, not two.
- Multiple true current groups with the same display name must remain separate when their durable identity differs.

## Task 1: RED Test For Current Group Identity And Rate Fallback

**Files:**
- Create: `scripts/group-facts-projection.test.mjs`

- [ ] **Step 1: Write the failing test**

Create `scripts/group-facts-projection.test.mjs`:

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

const {
  buildCurrentStationGroupFacts,
  buildStationGroupOptionsFromCurrentFacts,
  latestGroupRatesByBindingOrHash,
} = await importTsModule("src/lib/projections/groupFacts.ts");

const bindings = [
  binding({
    id: "binding-current",
    stationId: "station-a",
    groupKeyHash: "local-current",
    groupIdHash: "remote-current",
    groupName: "default",
    userRateMultiplier: null,
    effectiveRateMultiplier: 0.8,
    defaultRateMultiplier: 1,
    bindingStatus: "available",
  }),
  binding({
    id: "binding-same-name-a",
    stationId: "station-a",
    groupKeyHash: "local-a",
    groupIdHash: "same-remote",
    groupName: "shared name",
    userRateMultiplier: 0.5,
    effectiveRateMultiplier: 0.7,
    defaultRateMultiplier: 1,
    bindingStatus: "available",
  }),
  binding({
    id: "binding-same-name-b",
    stationId: "station-a",
    groupKeyHash: "local-b",
    groupIdHash: "same-remote",
    groupName: "shared name",
    userRateMultiplier: null,
    effectiveRateMultiplier: 0.6,
    defaultRateMultiplier: 1,
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
];

const rates = [
  rate({
    id: "rate-current-newer",
    stationId: "station-a",
    groupBindingId: "binding-current",
    groupKeyHash: "local-current",
    groupName: "default",
    userRateMultiplier: 0.75,
    effectiveRateMultiplier: 0.75,
    defaultRateMultiplier: 1,
    checkedAt: "2026-07-07T02:00:00.000Z",
  }),
  rate({
    id: "rate-current-shadow",
    stationId: "station-a",
    groupBindingId: null,
    groupKeyHash: "local-current",
    groupName: "default",
    userRateMultiplier: 0.7,
    effectiveRateMultiplier: 0.7,
    defaultRateMultiplier: 1,
    checkedAt: "2026-07-07T03:00:00.000Z",
  }),
  rate({
    id: "rate-missing-history",
    stationId: "station-a",
    groupBindingId: "binding-missing",
    groupKeyHash: "local-missing",
    groupName: "missing group",
    userRateMultiplier: 0.01,
    effectiveRateMultiplier: 0.01,
    defaultRateMultiplier: 1,
    checkedAt: "2026-07-07T04:00:00.000Z",
  }),
];

const latestRates = latestGroupRatesByBindingOrHash(rates);
assert.equal(latestRates.get("binding:binding-current")?.id, "rate-current-newer");
assert.equal(latestRates.get("group-key:local-current")?.id, "rate-current-shadow");

const facts = buildCurrentStationGroupFacts({ bindings, rates });

assert.deepEqual(
  facts.map((fact) => ({
    identityKey: fact.identityKey,
    groupBindingId: fact.groupBindingId,
    groupName: fact.groupName,
    rateMultiplier: fact.rateMultiplier,
    available: fact.available,
    rateEvidenceId: fact.rateEvidenceId,
  })),
  [
    {
      identityKey: "binding:binding-current",
      groupBindingId: "binding-current",
      groupName: "default",
      rateMultiplier: 0.8,
      available: true,
      rateEvidenceId: "rate-current-newer",
    },
    {
      identityKey: "binding:binding-same-name-a",
      groupBindingId: "binding-same-name-a",
      groupName: "shared name",
      rateMultiplier: 0.5,
      available: true,
      rateEvidenceId: null,
    },
    {
      identityKey: "binding:binding-same-name-b",
      groupBindingId: "binding-same-name-b",
      groupName: "shared name",
      rateMultiplier: 0.6,
      available: true,
      rateEvidenceId: null,
    },
    {
      identityKey: "binding:binding-missing",
      groupBindingId: "binding-missing",
      groupName: "missing group",
      rateMultiplier: 0.01,
      available: false,
      rateEvidenceId: "rate-missing-history",
    },
  ],
  "current facts should preserve durable identities, use rate fallback, and not revive missing groups",
);

const options = buildStationGroupOptionsFromCurrentFacts(facts);
assert.deepEqual(
  options.map((option) => ({
    value: option.value,
    groupBindingId: option.groupBindingId,
    groupName: option.groupName,
    rateMultiplier: option.rateMultiplier,
    selectableForRemoteKey: option.selectableForRemoteKey,
  })),
  [
    {
      value: "binding:binding-current",
      groupBindingId: "binding-current",
      groupName: "default",
      rateMultiplier: 0.8,
      selectableForRemoteKey: true,
    },
    {
      value: "binding:binding-same-name-a",
      groupBindingId: "binding-same-name-a",
      groupName: "shared name",
      rateMultiplier: 0.5,
      selectableForRemoteKey: true,
    },
    {
      value: "binding:binding-same-name-b",
      groupBindingId: "binding-same-name-b",
      groupName: "shared name",
      rateMultiplier: 0.6,
      selectableForRemoteKey: true,
    },
  ],
  "group options should include only available current facts and keep duplicate display names distinct",
);

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
    createdAt: "2026-07-07T00:00:00.000Z",
    updatedAt: "2026-07-07T00:00:00.000Z",
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
    checkedAt: "2026-07-07T00:00:00.000Z",
    createdAt: "2026-07-07T00:00:00.000Z",
    ...overrides,
  };
}
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
node scripts/group-facts-projection.test.mjs
```

Expected: FAIL with `ENOENT` for `src/lib/projections/groupFacts.ts`.

- [ ] **Step 3: Commit RED test**

```powershell
git add -- scripts/group-facts-projection.test.mjs
git commit -m "test: guard current group projection"
```

## Task 2: Implement Pure Group Projection

**Files:**
- Create: `src/lib/projections/groupFacts.ts`

- [ ] **Step 1: Create the projection module**

Create `src/lib/projections/groupFacts.ts`:

```ts
import type { GroupRateRecord, StationGroupBinding, StationGroupOption } from "@/lib/types/groupFacts";

export type StationGroupCurrentFact = {
  identityKey: string;
  groupBindingId: string | null;
  stationId: string;
  stationKeyId: string | null;
  bindingKind: string;
  groupKeyHash: string | null;
  groupIdHash: string | null;
  groupName: string;
  bindingStatus: string;
  available: boolean;
  rateMultiplier: number | null;
  rateSource: string | null;
  rateEvidenceId: string | null;
  rateCheckedAt: string | null;
  sourceBinding: StationGroupBinding | null;
  sourceRate: GroupRateRecord | null;
};

export function buildCurrentStationGroupFacts(input: {
  bindings: StationGroupBinding[];
  rates: GroupRateRecord[];
}): StationGroupCurrentFact[] {
  const latestRates = latestGroupRatesByBindingOrHash(input.rates);
  const consumedRateIds = new Set<string>();
  const facts = input.bindings.map((binding) => {
    const identityKey = identityKeyForBinding(binding);
    const latestRate = latestRates.get(`binding:${binding.id}`) ??
      latestRates.get(`group-key:${binding.groupKeyHash}`) ??
      null;
    if (latestRate) {
      consumedRateIds.add(latestRate.id);
    }
    return factFromBinding(binding, latestRate, identityKey);
  });

  for (const rate of input.rates) {
    if (consumedRateIds.has(rate.id)) {
      continue;
    }
    const identityKey = identityKeyForRate(rate);
    if (facts.some((fact) => fact.identityKey === identityKey)) {
      continue;
    }
    facts.push(factFromRate(rate, identityKey));
  }

  return facts;
}

export function latestGroupRatesByBindingOrHash(
  rates: GroupRateRecord[],
): Map<string, GroupRateRecord> {
  const latest = new Map<string, GroupRateRecord>();
  for (const rate of rates) {
    const keys = [
      rate.groupBindingId ? `binding:${rate.groupBindingId}` : null,
      rate.groupKeyHash ? `group-key:${rate.groupKeyHash}` : null,
      normalizedName(rate.groupName) ? `group-name:${normalizedName(rate.groupName)}` : null,
    ].filter((key): key is string => Boolean(key));
    for (const key of keys) {
      const existing = latest.get(key);
      if (!existing || Date.parse(rate.checkedAt) >= Date.parse(existing.checkedAt)) {
        latest.set(key, rate);
      }
    }
  }
  return latest;
}

export function buildStationGroupOptionsFromCurrentFacts(
  facts: StationGroupCurrentFact[],
): StationGroupOption[] {
  return facts
    .filter((fact) => fact.available)
    .map((fact) => ({
      value: fact.groupBindingId ? `binding:${fact.groupBindingId}` : fact.identityKey,
      groupBindingId: fact.groupBindingId,
      groupIdHash: fact.groupIdHash,
      groupName: fact.groupName,
      rateMultiplier: fact.rateMultiplier,
      rateSource: fact.rateSource,
      selectableForRemoteKey: Boolean(fact.groupIdHash),
    }));
}

function factFromBinding(
  binding: StationGroupBinding,
  latestRate: GroupRateRecord | null,
  identityKey: string,
): StationGroupCurrentFact {
  return {
    identityKey,
    groupBindingId: binding.id,
    stationId: binding.stationId,
    stationKeyId: binding.stationKeyId,
    bindingKind: binding.bindingKind,
    groupKeyHash: binding.groupKeyHash,
    groupIdHash: binding.groupIdHash,
    groupName: binding.groupName,
    bindingStatus: binding.bindingStatus,
    available: binding.bindingStatus !== "missing" && binding.bindingStatus !== "disabled",
    rateMultiplier: firstNumber(
      binding.userRateMultiplier,
      binding.effectiveRateMultiplier,
      latestRate?.userRateMultiplier,
      latestRate?.effectiveRateMultiplier,
      binding.defaultRateMultiplier,
      latestRate?.defaultRateMultiplier,
    ),
    rateSource: binding.rateSource ?? latestRate?.source ?? null,
    rateEvidenceId: latestRate?.id ?? null,
    rateCheckedAt: latestRate?.checkedAt ?? binding.lastCheckedAt,
    sourceBinding: binding,
    sourceRate: latestRate,
  };
}

function factFromRate(rate: GroupRateRecord, identityKey: string): StationGroupCurrentFact {
  return {
    identityKey,
    groupBindingId: rate.groupBindingId,
    stationId: rate.stationId,
    stationKeyId: rate.stationKeyId,
    bindingKind: rate.bindingKind,
    groupKeyHash: rate.groupKeyHash,
    groupIdHash: null,
    groupName: rate.groupName,
    bindingStatus: "rate_only",
    available: true,
    rateMultiplier: firstNumber(
      rate.userRateMultiplier,
      rate.effectiveRateMultiplier,
      rate.defaultRateMultiplier,
    ),
    rateSource: rate.source,
    rateEvidenceId: rate.id,
    rateCheckedAt: rate.checkedAt,
    sourceBinding: null,
    sourceRate: rate,
  };
}

function identityKeyForBinding(binding: StationGroupBinding) {
  if (binding.id) return `binding:${binding.id}`;
  if (binding.groupKeyHash) return `group-key:${binding.groupKeyHash}`;
  if (binding.groupIdHash) return `group-id:${binding.groupIdHash}`;
  return `group-name:${normalizedName(binding.groupName)}`;
}

function identityKeyForRate(rate: GroupRateRecord) {
  if (rate.groupBindingId) return `binding:${rate.groupBindingId}`;
  if (rate.groupKeyHash) return `group-key:${rate.groupKeyHash}`;
  return `group-name:${normalizedName(rate.groupName)}`;
}

function firstNumber(...values: Array<number | null | undefined>) {
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
node scripts/group-facts-projection.test.mjs
```

Expected: exit 0.

- [ ] **Step 3: Run boundary guards**

Run:

```powershell
node scripts/data-architecture-field-ownership.test.mjs
node scripts/pricing-comparison-view-model.test.mjs
pnpm.cmd build
```

Expected:

- `data-architecture-field-ownership` exit 0.
- `pricing-comparison-view-model` exit 0.
- `pnpm.cmd build` exit 0 with only the existing Vite chunk-size warning.

- [ ] **Step 4: Commit implementation**

```powershell
git add -- src/lib/projections/groupFacts.ts
git commit -m "refactor: add current group projection"
```

## Task 3: Source Boundary Guard For Projection Consumers

**Files:**
- Modify: `scripts/group-facts-projection.test.mjs`
- Modify: `docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage3-progress.md`

- [ ] **Step 1: Add source guard assertions**

Append this to `scripts/group-facts-projection.test.mjs`:

```js
const projectionSource = await readFile("src/lib/projections/groupFacts.ts", "utf8");
assert.ok(
  !projectionSource.includes("from \"@/features/") &&
    !projectionSource.includes("invoke<") &&
    !projectionSource.includes("getLocalAccessKey") &&
    !projectionSource.includes("upsertStationGroupBinding"),
  "group projection should stay pure and must not import feature modules, call Tauri, read secrets, or write bindings",
);
assert.ok(
  projectionSource.includes("binding.userRateMultiplier") &&
    projectionSource.includes("binding.effectiveRateMultiplier") &&
    projectionSource.includes("latestRate?.userRateMultiplier") &&
    projectionSource.includes("latestRate?.effectiveRateMultiplier") &&
    projectionSource.includes("binding.defaultRateMultiplier") &&
    projectionSource.includes("latestRate?.defaultRateMultiplier"),
  "group projection should encode the documented rate fallback order",
);
```

- [ ] **Step 2: Run test**

Run:

```powershell
node scripts/group-facts-projection.test.mjs
```

Expected: exit 0.

- [ ] **Step 3: Write Stage 3 progress audit**

Create `docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage3-progress.md`:

```md
# Relay Pool 数据架构 Stage 3 进度审计

日期：2026-07-07

## 范围

Stage 3 当前只建立 `src/lib/projections/groupFacts.ts` 纯函数投影，不迁移页面消费者，不删除字段，不改 schema。

## 已完成

- `buildCurrentStationGroupFacts()`
- `latestGroupRatesByBindingOrHash()`
- `buildStationGroupOptionsFromCurrentFacts()`
- `scripts/group-facts-projection.test.mjs`

## 验证

```powershell
node scripts/group-facts-projection.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/pricing-comparison-view-model.test.mjs
pnpm.cmd build
```

## Drift

记录执行时的工作树 HEAD、主 checkout HEAD、主 checkout dirty paths。若主 checkout 仍有未提交 pricing / station / group facts 改动，本文必须写明未接入。
```

- [ ] **Step 4: Commit guards and docs**

```powershell
git add -- scripts/group-facts-projection.test.mjs docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage3-progress.md
git commit -m "test: guard current group projection boundaries"
```

## Self Review

- Spec coverage: covers identity fallback, rate fallback, duplicate binding/rate dedupe, same-name distinct groups, missing not revived, and pure projection boundary.
- Placeholder scan: the plan contains concrete commands, code snippets, and expected results for each step.
- Type consistency: uses existing `StationGroupBinding`, `GroupRateRecord`, and `StationGroupOption` from `src/lib/types/groupFacts.ts`; re-check after main checkout dirty changes are merged.
