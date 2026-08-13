# Station Group Issue Details Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the station list report only missing or disabled groups that still affect enabled keys, and expose concrete reasons from the `分组异常` tag on hover and keyboard focus.

**Architecture:** Keep collector storage untouched and derive current operational issues in `stationAssetViewModels.ts`. Project a structured reason list into `StationAssetRow`, reuse `StationIssueTag.title` for complete Chinese details, and render a local accessible tooltip around tags that have details.

**Tech Stack:** React, TypeScript, Tailwind CSS, Node assertion regression scripts, Vite.

---

### Task 1: Derive current group issues from enabled-key dependencies

**Files:**
- Modify: `scripts/station-list-risk-tags.test.mjs`
- Modify: `src/features/stations/stationAssetViewModels.ts`

- [ ] **Step 1: Extend the test harness with real row building**

Import `buildStationAssetRows` from the transpiled view model and add complete fixture builders for `StationKey` and `StationGroupBinding`. Add a helper that builds one station row from bindings and keys so the test exercises the real projection rather than manually assigning `groupIssueCount`.

- [ ] **Step 2: Write failing group-issue regressions**

Add assertions proving:

```javascript
assert.equal(groupIssueTagFor({
  bindings: [missingGroup({ groupName: "Plus" }), availableGroup({ groupName: " plus " })],
  keys: [enabledKey({ groupName: "Plus" })],
}), undefined);

assert.equal(groupIssueTagFor({
  bindings: [missingGroup({ groupName: "Legacy" })],
  keys: [],
}), undefined);

assert.equal(groupIssueTagFor({
  bindings: [missingGroup({ id: "missing-pro", groupName: "Pro" })],
  keys: [enabledKey({ groupBindingId: "missing-pro", groupName: "Pro", name: "生产 Key" })],
})?.title, "分组「Pro」已下架，但仍被启用 Key「生产 Key」使用。");
```

Cover disabled-key-only references, exact binding-id matching, group-id-hash matching, normalized-name matching, and duplicate historical bindings coalescing into one reason.

- [ ] **Step 3: Run the focused test and verify RED**

Run: `node scripts/station-list-risk-tags.test.mjs`

Expected: FAIL because `buildStationAssetRows` still counts every `missing` / `disabled` station-group binding and does not project reason details.

- [ ] **Step 4: Implement the minimal group-issue projection**

In `stationAssetViewModels.ts`:

```typescript
export type StationGroupIssueReason = {
  bindingId: string;
  groupName: string;
  bindingStatus: "missing" | "disabled";
  affectedKeyCount: number;
  affectedKeyNames: string[];
  message: string;
};
```

Add a pure helper that:

1. builds normalized names for current `available` station groups;
2. skips historical bindings shadowed by a current same-name group;
3. matches only enabled keys by binding id, non-empty group-id hash, or normalized name;
4. coalesces duplicate historical bindings by normalized group name and status;
5. builds bounded, secret-safe Chinese messages from key display names.

Set `groupIssueReasons` on each `StationAssetRow`, derive `groupIssueCount` from its length, and pass the joined reason messages to `createStationIssueTag("group_issue", title)`.

- [ ] **Step 5: Run the focused test and verify GREEN**

Run: `node scripts/station-list-risk-tags.test.mjs`

Expected: PASS with the new suppression, dependency, deduplication, and message assertions.

### Task 2: Render accessible hover and focus details

**Files:**
- Modify: `scripts/station-list-risk-tags.test.mjs`
- Modify: `src/features/stations/StationsPage.tsx`

- [ ] **Step 1: Write failing tooltip source-contract assertions**

Require the station row source to contain a dedicated issue-tag renderer with:

```tsx
tabIndex={tag.title ? 0 : undefined}
aria-describedby={tag.title ? tooltipId : undefined}
role="tooltip"
group-hover/tag:visible
group-focus/tag:visible
```

Also require the tag's native `title` to use the concrete detail string.

- [ ] **Step 2: Run the focused test and verify RED**

Run: `node scripts/station-list-risk-tags.test.mjs`

Expected: FAIL because the page currently renders a plain `<span title=...>` without a hover/focus detail layer.

- [ ] **Step 3: Implement a local `StationIssueTagBadge` component**

Render tags with stable positioning and no row layout shift. For detailed tags, use a relatively positioned focusable wrapper, a visible focus ring, native title fallback, `aria-describedby`, and an absolutely positioned tooltip using existing surface, border, shadow, foreground, and muted-foreground tokens. Preserve the current badge colors and size.

- [ ] **Step 4: Run the focused test and verify GREEN**

Run: `node scripts/station-list-risk-tags.test.mjs`

Expected: PASS.

### Task 3: Verify the completed frontend change

**Files:**
- Verify: `scripts/station-list-risk-tags.test.mjs`
- Verify: `src/features/stations/stationAssetViewModels.ts`
- Verify: `src/features/stations/StationsPage.tsx`

- [ ] **Step 1: Run focused and adjacent station projection regressions**

Run:

```powershell
node scripts/station-list-risk-tags.test.mjs
node scripts/station-assets-current-projections.test.mjs
```

Expected: both commands exit 0.

- [ ] **Step 2: Run TypeScript checking**

Run: `pnpm.cmd exec tsc --noEmit`

Expected: exit 0 with no TypeScript errors.

- [ ] **Step 3: Run the Vite production build**

Run: `pnpm.cmd build`

Expected: exit 0 and a completed Vite build. If the known transient `EPERM` lock on `dist/assets` occurs, rerun the exact command once before diagnosing source code.

- [ ] **Step 4: Verify the visible station row**

Launch or reuse the source app, open the station list, and verify:

- harmless historical groups no longer produce `分组异常`;
- a fixture or existing station with an actively referenced missing group still shows the tag;
- hover and keyboard focus expose the same concrete reason;
- the tooltip stays inside the light desktop-tool visual language and does not move the row.

- [ ] **Step 5: Inspect final scope**

Run:

```powershell
git diff --check -- scripts/station-list-risk-tags.test.mjs src/features/stations/stationAssetViewModels.ts src/features/stations/StationsPage.tsx
git status --short
```

Expected: no whitespace errors; only task files plus pre-existing user changes are present. Do not stage or commit unless explicitly requested.
