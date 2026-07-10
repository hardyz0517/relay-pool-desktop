# Request Log Observability Table Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist reasoning effort, cache usage, first-token latency, and billing mode, then present request logs as a compact usage table without collecting IP addresses.

**Architecture:** Add a pure protocol-observation module beside the proxy runtime, keep network timing in the runtime, finalize cost and logging after response delivery, and isolate table presentation from page state. Existing request-log fields remain backward compatible and new database columns are nullable.

**Tech Stack:** Rust, serde_json, rusqlite, Tauri 2, React, TypeScript, Tailwind CSS, Node assertion scripts

---

### Task 1: Pure request and response observation

**Files:**
- Create: `src-tauri/src/services/proxy/observability.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`

- [ ] Write Rust tests for `reasoning.effort`, `reasoning_effort`, missing effort, Responses usage, Chat Completions usage, cache read/create fields, and SSE JSON split across arbitrary chunks.
- [ ] Run `cargo test services::proxy::observability --lib` and confirm RED because the module and types do not exist.
- [ ] Add `RequestObservation`, `ObservedUsage`, and a bounded `SseUsageObserver` that parses structured JSON only and never retains complete request/response bodies.
- [ ] Rerun the focused test and confirm all observation cases pass.

### Task 2: Request-log database contract

**Files:**
- Modify: `src-tauri/src/models/proxy.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] Add a failing database round-trip test using distinct sentinel values for `reasoning_effort`, cache creation/read tokens, `first_token_ms`, and `billing_mode`.
- [ ] Add a failing migration assertion that all five nullable columns exist after database initialization.
- [ ] Run the focused database tests and confirm failures are caused by the missing fields/columns.
- [ ] Extend `RequestLog` and `CreateRequestLogInput`, the base schema, a focused observability migration, INSERT parameters, row mapping, and the shared SELECT column constant.
- [ ] Rerun focused database tests and existing request-log cost snapshot tests.

### Task 3: Observable response delivery and unified settlement

**Files:**
- Modify: `src-tauri/src/models/pricing.rs`
- Modify: `src-tauri/src/services/pricing/mod.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Test: existing inline Rust test modules in those files

- [ ] Add failing tests proving the first streamed chunk records TTFT, split SSE usage reaches the final log input, non-stream JSON uses the same usage normalizer, and interrupted streams keep partial observations.
- [ ] Add failing pricing tests proving cache token fields survive into `RequestCostEstimate` and billing mode derives from the resolved pricing unit.
- [ ] Replace `std::io::copy` with an explicit fixed-size copy loop returning `ResponseWriteOutcome { first_token_ms, usage }`.
- [ ] Parse request observation once and carry reasoning effort into the final log draft.
- [ ] Add a usage-aware pricing entrypoint while retaining the existing wrapper for callers that only have prompt/output totals.
- [ ] Finalize request cost after response delivery when streaming usage becomes available; keep existing buffered behavior compatible.
- [ ] Run focused proxy and pricing tests until GREEN.

### Task 4: Compact request-log table

**Files:**
- Create: `src/features/logs/requestLogViewModels.ts`
- Create: `src/features/logs/RequestLogTable.tsx`
- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/lib/types/proxy.ts`
- Create: `scripts/request-log-observability-table.test.mjs`

- [ ] Write a failing Node contract test for the exact column labels, structured token/latency labels, nullable fallbacks, stable horizontal width, and absence of `IP`, `clientIp`, and `remoteAddr`.
- [ ] Run `node scripts/request-log-observability-table.test.mjs` and confirm RED on missing table/view-model files.
- [ ] Extend the TypeScript `RequestLog` contract and implement pure display mapping for reasoning, billing, Token, cost, latency, endpoint and time.
- [ ] Move the main table into `RequestLogTable`; keep filters and developer-only detail ownership in `LogsPage`.
- [ ] Preserve the current empty state, refresh, clear confirmation and developer-mode detail behavior.
- [ ] Rerun the new test plus `logs-developer-mode-details`, `logs-pricing-status-display`, and `log-query-service` scripts.

### Task 5: Boundary and regression audit

**Files:**
- Modify only files exposed by failing in-scope checks.

- [ ] Run `rg -n -i "client_ip|clientIp|remote_addr|remoteAddr|peer_addr" src src-tauri` and confirm no persisted/request-log IP field was introduced; the accepted socket peer value may only remain discarded.
- [ ] Run all request-log and proxy/pricing focused Rust tests.
- [ ] Run `cargo check --manifest-path src-tauri/Cargo.toml` and separate in-scope failures from unrelated existing workspace failures.
- [ ] Run `pnpm.cmd build` and `git diff --check`.
- [ ] Verify the request-log table at desktop and narrow viewport widths with the browser; confirm no incoherent overlap and horizontal scrolling where needed.
- [ ] Review the final diff for duplicated parsing, positional SQL drift, unbounded buffering, secret/IP capture, and accidental Dashboard changes.
