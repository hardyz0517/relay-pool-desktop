# PageForm Sticky Footer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep every `PageForm` footer fully visible inside the page scroll container at all scroll positions.

**Architecture:** The fix stays in the shared `PageForm` component. A focused source-contract test protects the sticky footer class and confirms the known form surfaces continue to consume `PageForm`.

**Tech Stack:** React, TypeScript, Tailwind CSS, Node script tests, Vite build.

---

### Task 1: Regression Contract

**Files:**
- Create: `scripts/page-form-sticky-footer.test.mjs`

- [ ] **Step 1: Write the failing test**

Create a Node script that reads `src/components/ui/PageForm.tsx`, extracts the footer class, requires `sticky bottom-0`, rejects `bottom-[calc(var(--shell-page-gap)*-1)]`, and checks current provider/key/channel monitor form files reference `PageForm`.

- [ ] **Step 2: Run test to verify it fails**

Run: `node scripts/page-form-sticky-footer.test.mjs`
Expected: FAIL because the current footer still uses the negative bottom offset.

### Task 2: Shared Component Fix

**Files:**
- Modify: `src/components/ui/PageForm.tsx`

- [ ] **Step 1: Write minimal implementation**

Change only the sticky footer bottom constraint from `bottom-[calc(var(--shell-page-gap)*-1)]` to `bottom-0`. Keep the full-width gutter compensation, border, background, blur, padding, and wrapping behavior unchanged.

- [ ] **Step 2: Run focused test to verify it passes**

Run: `node scripts/page-form-sticky-footer.test.mjs`
Expected: PASS with `page form sticky footer contract ok`.

### Task 3: Verification

**Files:**
- Modify: `scripts/edit-key-page-flow.test.mjs` if its route-active assertion is stale relative to `src/app/pageTransitionPolicy.ts`.

- [ ] **Step 1: Run related script tests**

Run:
`node scripts/page-scaffold-sticky-header.test.mjs`
`node scripts/edit-key-page-flow.test.mjs`
`node scripts/channel-monitoring-layout.test.mjs`
`node scripts/page-form-sticky-footer.test.mjs`

Expected: all exit 0.

- [ ] **Step 2: Update stale adjacent contracts if necessary**

If `scripts/edit-key-page-flow.test.mjs` fails only because it expects the old inline `pageId === "addKey" || pageId === "editKey"` shape, update it to verify the current architecture instead: `App.tsx` should pass `resolveActiveShellRouteId(...)` into `AppShell`, and `src/app/pageTransitionPolicy.ts` should map both `addKey` and `editKey` to `parentRouteId: "keyPool"`.

- [ ] **Step 3: Run build check**

Run: `pnpm build`
Expected: TypeScript and Vite build exit 0.

- [ ] **Step 4: Browser verification**

Launch the current-source app through Vite/Tauri or a Vite dev server, inspect a scrollable `PageForm` at top, middle, and bottom positions, and confirm the footer remains fully visible and stable.
