# Relay Pool Data Architecture Stage 6 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Key Pool, Edit Key, Add Provider, and group option helpers to consume shared current group facts while preserving selected `group_binding_id`, explicit clear behavior, and remote key group binding semantics.

**Architecture:** Stage 6 keeps UI layout and backend schema unchanged. It first updates stale source guards after Stage 5, then adds pure group option helpers that consume `StationGroupCurrentFact`, and finally migrates Key Pool/Add Provider consumers to those helpers rather than rebuilding group identity from compatibility fields in page-local code.

**Tech Stack:** TypeScript pure helpers, React view models/pages, Node source/behavior tests, Vite/TypeScript build.

---

## Entry Gate

- Worktree must be `D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`.
- Worktree branch must be `codex/data-architecture-stage0`.
- Read first:
  - `docs/superpowers/specs/2026-07-07-relay-pool-data-architecture-master-spec.md`
  - `docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-master-plan.md`
  - `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`
  - `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage5-progress.md`
  - this file
- Run drift intake:

```powershell
git status --short
git log --oneline -8
git -C D:\Dev\Projects\relay-pool-desktop status --short
git -C D:\Dev\Projects\relay-pool-desktop log --oneline -8
```

- If main checkout has uncommitted Key Pool, Edit Key, Add Provider, group facts, remote key, shared capability, runtime, or schema changes, stop and record an intake blocker.
- Stage 6 must not change database schema, remote secret handling, or page visual structure.

## Files

- Modify: `src/features/stations/groupOptionViewModels.ts`
- Modify: `src/features/stations/components/StationKeyRowsEditor.tsx`
- Modify: `src/features/stations/AddProviderPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/key-pool/EditKeyPage.tsx`
- Modify: `scripts/add-provider-key-groups.test.mjs`
- Create: `scripts/group-option-current-facts.test.mjs`
- Create: `scripts/key-group-selection-current-facts.test.mjs`
- Create: `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage6-progress.md`

## Task 1: Stage 6 Plan And Stale Guard Intake

**Files:**
- Create: `docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage6.md`
- Modify: `scripts/add-provider-key-groups.test.mjs`

- [ ] **Step 1: Commit this plan**

```powershell
$patterns = @('TB' + 'D', 'TO' + 'DO', 'lat' + 'er', 'fill' + ' in', 'Similar' + ' to')
Select-String -Path docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage6.md -Pattern $patterns -CaseSensitive
git add -- docs/superpowers/plans/2026-07-08-relay-pool-data-architecture-stage6.md
git diff --cached --name-only
git commit -m "docs: add data architecture stage6 plan"
```

Expected: no placeholder scan output; staged path is only the Stage 6 plan.

- [ ] **Step 2: Verify stale guard**

Run:

```powershell
node scripts/add-provider-key-groups.test.mjs
```

Expected: FAIL only on the old assertion requiring `stationDetailViewModels.ts` to include `dedupeStationGroupBindings` / `preferStationGroupBinding`.

- [ ] **Step 3: Update stale guard**

Replace the final station detail assertion in `scripts/add-provider-key-groups.test.mjs` with:

```js
assert.ok(
  stationDetailViewModelSource.includes("buildCurrentStationGroupFacts") &&
    stationDetailViewModelSource.includes("isDisplayableStationGroupCurrentFact") &&
    !stationDetailViewModelSource.includes("dedupeStationGroupBindings") &&
    !stationDetailViewModelSource.includes("preferStationGroupBinding"),
  "station detail group rows should consume shared current group facts instead of page-local duplicate binding merge logic",
);
```

- [ ] **Step 4: Verify and commit guard update**

```powershell
node scripts/add-provider-key-groups.test.mjs
git add -- scripts/add-provider-key-groups.test.mjs
git diff --cached --name-only
git commit -m "test: align add-provider guard with current group facts"
```

## Task 2: RED Test For Group Options From Current Facts

**Files:**
- Create: `scripts/group-option-current-facts.test.mjs`
- Modify: `src/features/stations/groupOptionViewModels.ts`

- [ ] **Step 1: Write failing test**

Create `scripts/group-option-current-facts.test.mjs` that imports `groupFacts.ts` and `groupOptionViewModels.ts` through temporary ESM files, then asserts:

```js
const currentFacts = buildCurrentStationGroupFacts({
  bindings: [
    binding({ id: "binding-current", groupName: "current", groupIdHash: "remote-current", effectiveRateMultiplier: 0.8 }),
    binding({ id: "binding-missing", groupName: "missing", bindingStatus: "missing", groupIdHash: "remote-missing", effectiveRateMultiplier: 0.1 }),
    binding({ id: "binding-legacy", groupName: "legacy", rateSource: "legacy_key_group", groupIdHash: "remote-legacy" }),
  ],
  rates: [],
});

const options = buildStationGroupOptionsFromCurrentFactsForSelect(currentFacts);
assert.deepEqual(options, [
  {
    value: "binding:binding-current",
    groupBindingId: "binding-current",
    groupIdHash: "remote-current",
    groupName: "current",
    rateMultiplier: 0.8,
    rateSource: "test",
    selectableForRemoteKey: true,
  },
]);
```

Also assert source guards:

```js
assert.ok(groupOptionSource.includes("buildStationGroupOptionsFromCurrentFacts"));
assert.ok(groupOptionSource.includes("isDisplayableStationGroupCurrentFact"));
```

- [ ] **Step 2: Verify RED**

```powershell
node scripts/group-option-current-facts.test.mjs
```

Expected: FAIL because `buildStationGroupOptionsFromCurrentFactsForSelect` is not exported.

- [ ] **Step 3: Commit RED**

```powershell
git add -- scripts/group-option-current-facts.test.mjs
git diff --cached --name-only
git commit -m "test: guard group options from current facts"
```

## Task 3: Implement Shared Group Option Helper

**Files:**
- Modify: `src/features/stations/groupOptionViewModels.ts`

- [ ] **Step 1: Add helper**

Import:

```ts
import {
  buildStationGroupOptionsFromCurrentFacts,
  isDisplayableStationGroupCurrentFact,
  type StationGroupCurrentFact,
} from "@/lib/projections/groupFacts";
```

Add:

```ts
export function buildStationGroupOptionsFromCurrentFactsForSelect(
  facts: StationGroupCurrentFact[],
) {
  return normalizeStationGroupOptions(
    buildStationGroupOptionsFromCurrentFacts(
      facts.filter(isDisplayableStationGroupCurrentFact),
    ),
  );
}
```

- [ ] **Step 2: Verify GREEN**

```powershell
node scripts/group-option-current-facts.test.mjs
node scripts/group-facts-projection.test.mjs
```

- [ ] **Step 3: Commit implementation**

```powershell
git add -- src/features/stations/groupOptionViewModels.ts
git diff --cached --name-only
git commit -m "refactor: build group options from current facts"
```

## Task 4: RED Test For Key Group Selection Boundaries

**Files:**
- Create: `scripts/key-group-selection-current-facts.test.mjs`

- [ ] **Step 1: Write source guard**

Create a Node script that reads:

- `src/features/key-pool/KeyPoolPage.tsx`
- `src/features/key-pool/EditKeyPage.tsx`
- `src/features/stations/AddProviderPage.tsx`
- `src/features/stations/components/StationKeyRowsEditor.tsx`
- `src/features/stations/groupOptionViewModels.ts`

Assert:

```js
assert.ok(groupOptionSource.includes("buildStationGroupOptionsFromCurrentFactsForSelect"));
assert.ok(keyPoolSource.includes("buildStationGroupOptionsFromCurrentFactsForSelect"));
assert.ok(editKeySource.includes("buildStationGroupOptionsFromCurrentFactsForSelect"));
assert.ok(addProviderSource.includes("buildStationGroupOptionsFromCurrentFactsForSelect"));
assert.ok(keyPoolSource.includes("KEEP_GROUP_BINDING_VALUE") && keyPoolSource.includes("CLEAR_GROUP_BINDING_VALUE"));
assert.ok(editKeySource.includes("KEEP_GROUP_BINDING_VALUE") && editKeySource.includes("CLEAR_GROUP_BINDING_VALUE"));
assert.ok(!stationKeyRowsEditorSource.includes("rateSource: null"));
```

- [ ] **Step 2: Verify RED**

```powershell
node scripts/key-group-selection-current-facts.test.mjs
```

Expected: FAIL because consumers have not migrated to the new helper.

- [ ] **Step 3: Commit RED**

```powershell
git add -- scripts/key-group-selection-current-facts.test.mjs
git diff --cached --name-only
git commit -m "test: guard key group current fact selection"
```

## Task 5: Migrate Consumers Conservatively

**Files:**
- Modify: `src/features/stations/components/StationKeyRowsEditor.tsx`
- Modify: `src/features/stations/AddProviderPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/key-pool/EditKeyPage.tsx`

- [ ] **Step 1: StationKeyRowsEditor**

Replace draft-created fallback option construction so `rateSource` is derived from row data when available, or omitted only through a typed helper. Keep `noGroupValue` as the only explicit clear path.

- [ ] **Step 2: AddProviderPage**

Where persisted bindings/rates are converted to dropdown options, build current facts first:

```ts
const currentGroupOptions = buildStationGroupOptionsFromCurrentFactsForSelect(
  buildCurrentStationGroupFacts({ bindings: groupBindings, rates: groupRates }),
);
```

Keep draft rows as editable overlay only; do not let draft rows replace persisted `groupBindingId` when a current fact exists.

- [ ] **Step 3: KeyPoolPage and EditKeyPage**

Use the same helper for station group option lists. Preserve:

```ts
KEEP_GROUP_BINDING_VALUE
CLEAR_GROUP_BINDING_VALUE
return { kind: "keep" as const }
return { kind: "clear" as const }
```

- [ ] **Step 4: Verify GREEN**

```powershell
node scripts/key-group-selection-current-facts.test.mjs
node scripts/group-option-current-facts.test.mjs
node scripts/add-provider-key-groups.test.mjs
node scripts/edit-key-page-flow.test.mjs
node scripts/shared-capabilities-contract.test.mjs
pnpm.cmd build
```

- [ ] **Step 5: Commit migration**

```powershell
git add -- src/features/stations/components/StationKeyRowsEditor.tsx src/features/stations/AddProviderPage.tsx src/features/key-pool/KeyPoolPage.tsx src/features/key-pool/EditKeyPage.tsx
git diff --cached --name-only
git commit -m "refactor: consume current group facts in key flows"
```

## Task 6: Stage 6 Audit And Handoff

**Files:**
- Create: `docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage6-progress.md`

- [ ] **Step 1: Run final verification**

```powershell
node scripts/group-option-current-facts.test.mjs
node scripts/key-group-selection-current-facts.test.mjs
node scripts/add-provider-key-groups.test.mjs
node scripts/edit-key-page-flow.test.mjs
node scripts/shared-capabilities-contract.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
pnpm.cmd build
git status --short
```

- [ ] **Step 2: Write and commit audit**

Document Stage 6 commits, drift, tests, and field conclusions. Then:

```powershell
git add -- docs/superpowers/audits/2026-07-08-relay-pool-data-architecture-stage6-progress.md
git diff --cached --name-only
git commit -m "docs: summarize data architecture stage6 progress"
```

- [ ] **Step 3: Update rolling heartbeat**

Use `automation_update` to record latest HEAD, verification, blockers, and next stage: Stage 7 runtime snapshot.

## Self Review

- Spec coverage: Stage 6 protects `group_binding_id`, explicit clear, remote key binding semantics, and current group fact option construction.
- Placeholder scan: use the command in Task 1 before committing the plan.
- Type consistency: `buildStationGroupOptionsFromCurrentFactsForSelect()` accepts `StationGroupCurrentFact[]` and returns existing `StationGroupOption[]`, so consumers do not need new UI types.
