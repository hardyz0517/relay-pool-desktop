# Change Center Exchange Ratio Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Change Center multiplier rows use each station's current exchange ratio exactly once.

**Architecture:** Keep collector facts and change-event JSON as raw upstream values. Pass the station exchange-ratio map into the existing Change Center view-model options and normalize only at the display projection boundary with the shared formatter helper.

**Tech Stack:** React, TypeScript, esbuild-powered Node regression scripts, pnpm/Vite.

---

### Task 1: Lock the display contract with a failing regression

**Files:**
- Modify: `scripts/change-center-mark-read.test.mjs`

- [ ] **Step 1: Add a rate-change fixture with a non-default exchange ratio**

Pass `stationCreditPerCnyById: new Map([["station-rate", 2]])` for raw values `1.4` and `1.8`, then assert the title and diff contain `0.7` and `0.9`. Build the same event twice and assert the second projection is still `0.9`, not `0.45`.

- [ ] **Step 2: Cover group-added, group-missing, and fallback behavior**

Add focused assertions showing raw `1.8` becomes `0.9` for added/missing rows, while ratio `1` and invalid/missing ratios preserve raw values.

- [ ] **Step 3: Run the focused test and verify RED**

Run: `node scripts/change-center-mark-read.test.mjs`

Expected: assertion failure because the view model still renders raw `1.4` / `1.8`.

### Task 2: Normalize once in the Change Center projection

**Files:**
- Modify: `src/features/changes/changeEventViewModels.ts`
- Modify: `src/features/changes/ChangeCenterPage.tsx`

- [ ] **Step 1: Extend the view-model input**

Add `stationCreditPerCnyById?: Map<string, number> | Record<string, number>` to `ChangeEventListOptions`. Import `effectiveRateMultiplierForCredit` and add a small helper that looks up `event.stationId`, then converts a raw multiplier once.

- [ ] **Step 2: Apply the helper to all multiplier row types**

Use the helper for both sides of `rate_changed` and for the selected multiplier in `group_added` and `group_missing`. Leave `readMultiplier` as a raw JSON reader so storage semantics remain explicit.

- [ ] **Step 3: Supply the current ratio map from the page**

Build `stationCreditPerCnyById` from the existing `stationsQuery.data` and pass it alongside `stationNamesById` to filtering and row rendering. Do not transform the events themselves.

- [ ] **Step 4: Run the focused test and verify GREEN**

Run: `node scripts/change-center-mark-read.test.mjs`

Expected: all assertions pass.

### Task 3: Verify the complete frontend boundary

**Files:**
- Verify only.

- [ ] **Step 1: Run TypeScript checking**

Run: `pnpm.cmd exec tsc --noEmit`

Expected: exit code `0`.

- [ ] **Step 2: Run the production frontend build**

Run: `pnpm.cmd build`

Expected: Vite build exits with code `0`.

- [ ] **Step 3: Check patch hygiene and exact scope**

Run: `git diff --check` and inspect `git status --short`.

Expected: no whitespace errors; only the two docs, the focused regression, and the two Change Center frontend files belong to this task. Existing unrelated dirty files remain untouched.
