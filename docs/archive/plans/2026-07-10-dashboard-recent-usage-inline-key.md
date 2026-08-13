# Dashboard Recent Usage Inline Key Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Display the station key name inline after the model name in each Dashboard recent-usage row.

**Architecture:** Keep the existing request-log data projection and right-aligned cost/token column unchanged. Replace the left content column's separate model and key rows with one truncation-safe flex row, while retaining the timestamp as the second row.

**Tech Stack:** React, TypeScript, Tailwind CSS, Node.js source-contract regression scripts

---

### Task 1: Move the key label beside the model

**Files:**
- Modify: `scripts/dashboard-recent-usage-key-label.test.mjs`
- Modify: `src/features/dashboard/DashboardPage.tsx`

- [x] **Step 1: Write the failing layout assertion**

Require a `flex min-w-0 items-baseline gap-2` title row containing both `{request.model ?? request.path}` and `Key：{requestKeyName}`, followed by the existing timestamp row.

- [x] **Step 2: Run the focused test and verify RED**

Run: `node scripts/dashboard-recent-usage-key-label.test.mjs`

Expected: FAIL because the key label is still rendered in its own third row.

- [x] **Step 3: Implement the inline title row**

Use a truncation-safe flex row where the model is primary text and the key is muted secondary text with a bounded width. Do not change the right-side cost/token column.

- [x] **Step 4: Run focused and layout tests**

Run: `node scripts/dashboard-recent-usage-key-label.test.mjs` and `node scripts/dashboard-recent-usage-layout.test.mjs`

Expected: both PASS.

- [x] **Step 5: Verify the frontend build**

Run: `pnpm.cmd build`

Expected: TypeScript and Vite build complete successfully.
