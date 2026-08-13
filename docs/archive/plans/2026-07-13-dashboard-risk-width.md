# Dashboard Current Risk Width Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep all four current-risk cards visible in one row while preventing long risk details from widening the dashboard beyond the window.

**Architecture:** Preserve the existing dashboard component structure and Tailwind styling. Add shrink constraints only along the current-risk section's grid/flex chain, and lock the summary grid to four equal columns.

**Tech Stack:** React 18, TypeScript, Tailwind CSS 3, Node.js assertion scripts, Vite

---

### Task 1: Add the width regression contract

**Files:**
- Modify: `scripts/dashboard-section-spacing.test.mjs`
- Test: `scripts/dashboard-section-spacing.test.mjs`

- [ ] **Step 1: Write the failing assertions**

Require the current-risk section, fixed four-column grid, detail list, and local `ObjectRow` usage to carry `min-w-0`:

```javascript
assert.match(
  dashboardSource,
  /<section className="grid min-w-0 gap-3">[\s\S]*?<div className="grid min-w-0 grid-cols-4 gap-3">/,
  "current risk should keep four shrinkable columns without widening the page",
);

assert.match(
  dashboardSource,
  /activeRiskEvents\.length === 0[\s\S]*?<div className="grid min-w-0 gap-2">[\s\S]*?<ObjectRow[\s\S]*?className="min-w-0"/,
  "current risk detail rows should shrink and truncate inside the visible page width",
);
```

- [ ] **Step 2: Run the focused test and verify RED**

Run: `node scripts/dashboard-section-spacing.test.mjs`

Expected: FAIL because the current-risk section still uses `grid gap-3`, the summary grid still uses `grid-cols-2 sm:grid-cols-4`, and the detail row has no local `min-w-0` class.

### Task 2: Make the current-risk width chain shrinkable

**Files:**
- Modify: `src/features/dashboard/DashboardPage.tsx`
- Test: `scripts/dashboard-section-spacing.test.mjs`

- [ ] **Step 1: Implement the minimal layout change**

Use these classes in the current-risk section:

```tsx
<section className="grid min-w-0 gap-3">
  {/* header */}
  <div className="grid min-w-0 grid-cols-4 gap-3">
    {/* four DashboardMetricTile instances */}
  </div>
  {/* empty state or details */}
  <div className="grid min-w-0 gap-2">
    <ObjectRow className="min-w-0" />
  </div>
</section>
```

Keep the existing `min-w-0` on `DashboardMetricTile`, so each grid item and its text column can shrink below intrinsic content width.

- [ ] **Step 2: Run the focused test and verify GREEN**

Run: `node scripts/dashboard-section-spacing.test.mjs`

Expected: PASS with no output.

- [ ] **Step 3: Run frontend build verification**

Run: `pnpm build`

Expected: TypeScript `--noEmit` and Vite production build both complete successfully.

- [ ] **Step 4: Verify narrow desktop layout in a real browser**

At 760px and 1050px viewport widths, measure the active page layer and current-risk section:

```javascript
layer.scrollWidth === layer.clientWidth
section.getBoundingClientRect().right <= document.documentElement.clientWidth
cards.length === 4
new Set(cards.map((card) => card.getBoundingClientRect().top)).size === 1
```

Expected: no horizontal overflow and all four risk cards remain on the same row.

- [ ] **Step 5: Review the final scoped diff**

Run: `git diff --check -- scripts/dashboard-section-spacing.test.mjs src/features/dashboard/DashboardPage.tsx`

Expected: no whitespace errors; unrelated working-tree changes remain untouched.
