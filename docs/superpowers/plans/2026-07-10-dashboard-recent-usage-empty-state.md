# Dashboard Recent Usage Empty State Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show the approved empty state in the dashboard's recent-usage section only after workspace data has loaded successfully with no request logs.

**Architecture:** Keep the change local to `DashboardPage`: add an explicit successful-load flag next to the existing dashboard state, then branch the recent-usage content between the empty presentation and the unchanged request rows. Lock the behavior with the existing source-contract regression script before changing production code.

**Tech Stack:** React 18, TypeScript, Tailwind CSS, Lucide React, Node.js assertion scripts, Vite

---

### Task 1: Lock the empty-state contract

**Files:**
- Modify: `scripts/dashboard-recent-usage-layout.test.mjs`

- [x] **Step 1: Write the failing regression assertion**

Add assertions requiring `Inbox`, the approved Chinese copy, `dashboardLoaded && requestLogs.length === 0`, and a minimum-height centered empty-state container.

- [x] **Step 2: Run the focused test and verify RED**

Run: `node scripts/dashboard-recent-usage-layout.test.mjs`

Expected: FAIL because `DashboardPage.tsx` does not yet contain the empty-state branch.

### Task 2: Implement the dashboard empty state

**Files:**
- Modify: `src/features/dashboard/DashboardPage.tsx`

- [x] **Step 1: Add the successful-load state**

Import `Inbox`, initialize `dashboardLoaded` to `false`, and set it to `true` only after `loadDashboardWorkspace()` has populated all dashboard state successfully.

- [x] **Step 2: Render the approved empty branch**

Inside the existing recent-usage section, render a centered, stable-height empty state when `dashboardLoaded && requestLogs.length === 0`. Include `Inbox`, `暂无使用记录`, and `开始使用 API 后，您的使用历史将显示在这里。`; otherwise retain the existing five-row mapping unchanged.

- [x] **Step 3: Run the focused test and verify GREEN**

Run: `node scripts/dashboard-recent-usage-layout.test.mjs`

Expected: exit code 0 with no output.

### Task 3: Verify the front end

**Files:**
- Verify: `src/features/dashboard/DashboardPage.tsx`
- Verify: `scripts/dashboard-recent-usage-layout.test.mjs`

- [x] **Step 1: Run related dashboard scripts**

Run: `node scripts/dashboard-recent-usage-layout.test.mjs` and the other `scripts/dashboard-*.test.mjs` scripts.

Expected: all commands exit 0.

- [x] **Step 2: Run the production front-end build**

Run: `pnpm.cmd build`

Expected: TypeScript and Vite complete with exit code 0.

- [x] **Step 3: Review the scoped diff**

Run: `git diff -- src/features/dashboard/DashboardPage.tsx scripts/dashboard-recent-usage-layout.test.mjs docs/superpowers/specs/2026-07-10-dashboard-recent-usage-empty-state-design.md docs/superpowers/plans/2026-07-10-dashboard-recent-usage-empty-state.md`

Expected: only the approved empty state, its test, and documentation appear. Do not commit or push unless explicitly requested.
