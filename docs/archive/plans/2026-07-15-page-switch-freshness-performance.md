# Page Switch Freshness and Performance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove page-activation query amplification while retaining immediate cache display and bounded background freshness.

**Architecture:** Add backend page workspaces for cross-table snapshots, make React Query the single frontend owner of those snapshots, memoize repeated legacy pricing lookups within one log read, and persist Change Center reads in one transaction. Existing fallbacks remain available for browser-only development.

**Tech Stack:** Tauri 2, Rust, rusqlite, React 18, TypeScript, TanStack React Query, Vitest, Node contract tests.

---

### Task 1: Channel Status Single Query Source

**Files:**
- Modify: `src-tauri/src/models/shared_capabilities.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/queries/channelQueries.ts`
- Modify: `src/lib/query/resourceQueries.ts`
- Modify: `src/features/channels/ChannelStatusTab.tsx`
- Test: `scripts/page-switch-data-path-performance.test.mjs`

- [ ] Write a failing contract test requiring one backend `load_channel_status_workspace` invocation and forbidding an activation-owned manual workspace load.
- [ ] Run `node scripts/page-switch-data-path-performance.test.mjs` and confirm it fails on the missing command and duplicate refresh path.
- [ ] Add `ChannelStatusWorkspace`, load it under one database guard, expose the Tauri command, and make the query loader prefer that command with the current Promise-based fallback.
- [ ] Make `ChannelStatusTab` consume query data directly, poll only while active, and use explicit refetch for manual/monitor refresh.
- [ ] Re-run the contract and focused channel tests.

### Task 2: Pricing Comparison Workspace

**Files:**
- Modify: `src-tauri/src/models/shared_capabilities.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/lib/queries/pricingQueries.ts`
- Modify: `src/lib/query/queryKeys.ts`
- Modify: `src/lib/query/resourceQueries.ts`
- Modify: `src/features/pricing/PricingPage.tsx`
- Test: `scripts/page-switch-data-path-performance.test.mjs`

- [ ] Extend the failing contract to require one `load_pricing_comparison_workspace` invocation and reject station-mapped binding/rate/key calls in `PricingPage`.
- [ ] Run the contract and confirm the pricing assertions fail.
- [ ] Add all-station read helpers and return one consistent pricing workspace from Rust.
- [ ] Add the frontend query/fallback and switch `PricingPage` to cached data plus background refetch.
- [ ] Re-run pricing projection and page contract tests.

### Task 3: Request Log Pricing Lookup Reuse

**Files:**
- Modify: `src-tauri/src/services/database.rs`

- [ ] Add a Rust unit test with multiple legacy logs sharing a key/model and a counting resolver; require one economics lookup and equivalent enriched outputs.
- [ ] Run the focused test and confirm it fails because no memoized list enrichment boundary exists.
- [ ] Add per-list `(station_key_id, trimmed_model)` memoization without persisting or sharing contexts across snapshots.
- [ ] Re-run focused request-log pricing tests.

### Task 4: Atomic Change Center Read Persistence

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/api/changeEvents.ts`
- Modify: `src/features/changes/changeEventViewModels.ts`
- Modify: `src/components/shell/AppShell.tsx`
- Modify: `src/features/changes/ChangeCenterPage.tsx`
- Modify: `scripts/change-center-mark-read.test.mjs`

- [ ] Change the regression contract to require one batch callback receiving all captured unread IDs.
- [ ] Run the change-center test and confirm it fails against the per-ID API.
- [ ] Add a transactional backend batch command that updates only still-unread rows and returns current state.
- [ ] Add the frontend API fallback, centralize entry persistence in `AppShell`, and retain optimistic cache updates with error invalidation.
- [ ] Re-run change-center tests, including concurrent resolved/dismissed behavior.

### Task 5: Full Verification

**Files:**
- Verify all files above without unrelated formatting or staging.

- [ ] Run the focused Node and Vitest suites for the four pages.
- [ ] Run focused Rust tests for request-log enrichment, pricing/channel workspaces, and batch reads.
- [ ] Run `pnpm build`.
- [ ] Run `cargo check --manifest-path src-tauri/Cargo.toml`.
- [ ] Run the navigation browser benchmark with realistic list sizes and inspect long tasks/hidden queries.
- [ ] Launch the source desktop app and switch repeatedly across Change Center, Channel Status, Usage Records, and Pricing Comparison while the proxy is active.
