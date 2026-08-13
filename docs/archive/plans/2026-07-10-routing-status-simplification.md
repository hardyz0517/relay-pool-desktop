# Routing Status Simplification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the troubleshooting and latest-route-explanation sections, then align the two routing summary cards at their top edge.

**Architecture:** Keep the routing workspace and backend decision data unchanged. Remove only the status-page presentation and its now-dead navigation prop chain, while adding an explicit top-alignment constraint to the existing two-column grid.

**Tech Stack:** React 18, TypeScript, Tailwind CSS, Node regression scripts, Vite.

---

### Task 1: Lock the simplified status-page contract

**Files:**
- Modify: `scripts/local-routing-explanation.test.mjs`

- [ ] **Step 1: Replace the old positive UI assertions with removal and alignment assertions**

The script must assert that `LocalRoutingStatusTab.tsx` excludes `排障入口`, `最近一次路由解释`, `RouteExplanationPanel`, and `onOpenPage`; that `RoutingPage.tsx` and `App.tsx` exclude the old `onOpenPage` prop chain; and that the overview grid contains `items-start`.

- [ ] **Step 2: Run the regression script and verify RED**

Run: `node scripts/local-routing-explanation.test.mjs`

Expected: FAIL because the old sections and navigation prop chain still exist, and the overview grid does not yet contain `items-start`.

### Task 2: Remove the obsolete UI and align cards

**Files:**
- Modify: `src/features/routing/LocalRoutingStatusTab.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/app/App.tsx`
- Delete: `src/features/routing/RouteExplanationPanel.tsx`

- [ ] **Step 1: Simplify `LocalRoutingStatusTab`**

Remove the `Activity`, `ScrollText`, `Button`, `AppRouteId`, and `RouteExplanationPanel` imports; remove `LocalRoutingLinkedPage`, `onOpenPage`, and `openLinkedPage`; remove both obsolete JSX sections; and change the summary grid to:

```tsx
<div className="grid items-start gap-3 lg:grid-cols-[minmax(0,1.15fr)_minmax(280px,0.85fr)]">
```

- [ ] **Step 2: Remove the dead prop chain**

Render `RoutingPage` without `onOpenPage` in `App.tsx`, make `RoutingPage` parameterless, remove the navigation types, and render:

```tsx
<LocalRoutingStatusTab loading={loading} workspace={workspace} />
```

- [ ] **Step 3: Delete the unused explanation component**

Delete `src/features/routing/RouteExplanationPanel.tsx`. Do not change `LocalRoutingWorkspace`, query services, backend decision records, or request logs.

- [ ] **Step 4: Run focused and production verification**

Run:

```powershell
node scripts\local-routing-explanation.test.mjs
node scripts\local-routing-page-layout.test.mjs
pnpm.cmd build
git diff --check -- src/app/App.tsx src/features/routing/LocalRoutingStatusTab.tsx src/features/routing/RoutingPage.tsx scripts/local-routing-explanation.test.mjs
```

Expected: both scripts pass, TypeScript/Vite build exits 0, and `git diff --check` reports no errors.
