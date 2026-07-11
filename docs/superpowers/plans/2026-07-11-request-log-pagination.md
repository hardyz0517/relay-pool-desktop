# Request Log Pagination Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a separated, compact pagination footer below the request-log table with 20, 50, and 100 row page sizes.

**Architecture:** `LogsPage` owns page state because it already owns filters and refresh lifecycle. A pure view-model helper slices and clamps records, while a focused presentational footer renders count, size, current page, and chevron controls.

**Tech Stack:** React 18, TypeScript, Tailwind CSS, Lucide React, Node assertion scripts

---

### Task 1: Pagination Contract

**Files:**
- Create: `scripts/request-log-pagination.test.mjs`
- Modify: `src/features/logs/requestLogViewModels.ts`

- [ ] **Step 1: Write the failing pagination test**

Create a script that imports a pure `paginateRequestLogs(items, page, pageSize)` helper and asserts a 20-row default slice, final-page ranges, empty ranges, and page clamping.

- [ ] **Step 2: Run test to verify it fails**

Run: `node scripts/request-log-pagination.test.mjs`
Expected: FAIL because `paginateRequestLogs` is not exported.

- [ ] **Step 3: Implement the pure helper**

Return `{ logs, page, pageSize, totalPages, startIndex, endIndex, totalCount }`, clamp page size to at least one, clamp page to `1..totalPages`, and return `0-0` for empty input.

- [ ] **Step 4: Run test to verify it passes**

Run: `node scripts/request-log-pagination.test.mjs`
Expected: PASS.

### Task 2: Page State And Footer

**Files:**
- Modify: `scripts/request-log-pagination.test.mjs`
- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/features/logs/RequestLogTable.tsx`

- [ ] **Step 1: Add failing source-contract assertions**

Assert that `LogsPage` owns `page` and `pageSize`, resets page for filters, refresh, clear, and size changes, passes only `pageInfo.logs` to the table, and renders a pagination footer separated with `mt-4`.

- [ ] **Step 2: Run test to verify it fails**

Run: `node scripts/request-log-pagination.test.mjs`
Expected: FAIL because page state and footer are absent.

- [ ] **Step 3: Implement page state and presentation**

Use page size options `[20, 50, 100]`, a native labelled select, Lucide chevron icon buttons with tooltips and disabled states, and a highlighted current-page cell. Keep selection scoped to the visible page.

- [ ] **Step 4: Run focused tests**

Run: `node scripts/request-log-pagination.test.mjs` and `node scripts/request-log-observability-table.test.mjs`.
Expected: both PASS.

### Task 3: Verification

**Files:**
- Verify all changed frontend and script files.

- [ ] **Step 1: Run related regression scripts**

Run the pagination, observability, pricing display, and channel-monitor request-log scripts.
Expected: all PASS.

- [ ] **Step 2: Run frontend build and diff checks**

Run: `pnpm.cmd build` and `git diff --check`.
Expected: exit code 0; the existing large-chunk warning is allowed.

- [ ] **Step 3: Inspect the local UI**

Open the request-log page at desktop and narrow widths. Confirm the footer is separated from the table, controls do not overlap, and disabled/focus states remain legible.
