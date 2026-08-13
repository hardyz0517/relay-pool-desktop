# Dashboard Cost Comparison Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show today's and cumulative charged cost beside the corresponding 1x base cost, including explicit `$0.0000` zero values.

**Architecture:** Extend the existing per-currency dashboard cost aggregation with a base-cost total. Keep presentation formatting inside `DashboardPage.tsx` so the summary module remains a data-only unit.

**Tech Stack:** React, TypeScript, Node.js assertion scripts

---

### Task 1: Aggregate and display paired costs

**Files:**
- Modify: `src/features/dashboard/requestCostSummary.ts`
- Modify: `src/features/dashboard/DashboardPage.tsx`
- Test: `scripts/dashboard-request-cost-summary.test.mjs`

- [x] Add assertions that today's and cumulative totals include `baseTotalCost`, and that the card renders paired zero-aware cost values with the `总计:` label.
- [x] Run `node scripts/dashboard-request-cost-summary.test.mjs` and confirm the new assertions fail because base totals and paired formatting are absent.
- [x] Extend `DashboardCostTotal` and its aggregation to sum finite `baseTotalCost` values in the same currency row.
- [x] Format each row as `<actual> / <base>`, default an empty summary to `$0.0000 / $0.0000`, and use the same formatter for today's value and cumulative detail.
- [x] Re-run the focused script, then run `pnpm build` and confirm both pass.
