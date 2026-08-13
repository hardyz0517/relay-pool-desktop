# Channel Monitor Stream Observability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make monitor request logs record truthful group, stream, reasoning, Token, and latency data, then render compact Token and latency cells.

**Architecture:** Keep monitor probes pinned directly to the selected upstream Key. Derive request metadata during template rendering, observe streaming SSE responses incrementally with the shared proxy usage parser, persist the resulting facts, and render them through focused request-log cells.

**Tech Stack:** Rust, rusqlite, ureq, serde_json, React, TypeScript, Tailwind CSS, lucide-react, Node.js contract tests

---

### Task 1: Render Truthful Monitor Request Metadata

**Files:**
- Modify: `src-tauri/src/services/channel_monitors/templates.rs`
- Modify: `src-tauri/src/services/database.rs`

- [x] **Step 1: Add failing renderer and built-in-template tests**

Extend the renderer test to assert that a body containing `"stream": "{{stream}}"` and `"reasoning": { "effort": "minimal" }` produces:

```rust
assert!(rendered.stream);
assert_eq!(rendered.reasoning_effort.as_deref(), Some("minimal"));
```

Extend the built-in seed test to assert all four template bodies have `stream == "{{stream}}"` and both Responses bodies have `reasoning.effort == "minimal"`.

- [x] **Step 2: Verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml channel_monitors::templates --lib`

Expected: compilation or assertion failure because `RenderedMonitorRequest` has no metadata fields and built-ins still seed `false`.

- [x] **Step 3: Implement metadata projection and seed changes**

Add these fields:

```rust
pub struct RenderedMonitorRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub stream: bool,
    pub reasoning_effort: Option<String>,
}
```

After rendering `body_value`, derive `stream` from `/stream` and derive reasoning via `RequestObservation::from_json(&body_value)`. Change built-in bodies to use `"stream": "{{stream}}"`; add nested minimal reasoning to both Responses bodies.

- [x] **Step 4: Verify GREEN**

Run the focused template test command again. Expected: PASS.

### Task 2: Observe Streaming Monitor Responses

**Files:**
- Modify: `src-tauri/src/services/channel_monitors/probe.rs`
- Reuse: `src-tauri/src/services/proxy/observability.rs`

- [x] **Step 1: Add a failing staged-SSE probe test**

Serve a chunked `text/event-stream` response whose first chunk arrives before the final `response.completed` event. Assert:

```rust
assert!(result.first_token_ms.is_some());
assert_eq!(result.usage.as_ref().and_then(|usage| usage.input_tokens), Some(9));
assert_eq!(result.usage.as_ref().and_then(|usage| usage.output_tokens), Some(4));
assert_eq!(result.usage.as_ref().and_then(|usage| usage.cache_read_tokens), Some(3));
```

- [x] **Step 2: Verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml channel_monitors::probe --lib`

Expected: failure because the probe buffers JSON and exposes neither first-token timing nor cache-aware usage.

- [x] **Step 3: Implement incremental stream observation**

Extend `MonitorProbeResult` with `first_token_ms`. Extend `MonitorProbeUsage` with cache fields or replace it with the shared `ObservedUsage` shape. For `request.stream`, read chunks incrementally, set first-token timing on the first non-empty chunk, feed `SseUsageObserver`, and retain a bounded diagnostic excerpt. Preserve the buffered JSON path for non-stream requests.

- [x] **Step 4: Verify GREEN**

Run the focused probe tests again. Expected: PASS, including existing redaction and request-boundary tests.

### Task 3: Persist Monitor Facts

**Files:**
- Modify: `src-tauri/src/services/channel_monitors/mod.rs`

- [x] **Step 1: Add a failing monitor-log integration assertion**

Update the existing successful monitor test to bind its Key and assert the saved request log contains:

```rust
assert_eq!(log.group_binding_id, key.group_binding_id);
assert!(log.stream);
assert_eq!(log.reasoning_effort.as_deref(), Some("minimal"));
assert!(log.first_token_ms.is_some());
```

- [x] **Step 2: Verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml services::channel_monitors --lib`

Expected: assertion failure because the monitor context and inserted log still hardcode false/none and derive group from economics.

- [x] **Step 3: Wire actual request and probe metadata**

Set the built-in monitor context to `stream: true`. Persist:

```rust
stream: request.stream,
reasoning_effort: request.reasoning_effort.clone(),
first_token_ms: result.first_token_ms,
group_binding_id: target.group_binding_id.clone(),
```

Pass cache-read and cache-creation Token values into `CreateRequestLogInput` and retain the existing pricing calculation.

- [x] **Step 4: Verify GREEN**

Run the focused channel-monitor tests again. Expected: PASS.

### Task 4: Upgrade Request Log Cells

**Files:**
- Modify: `src/features/logs/RequestLogTable.tsx`
- Modify: `src/features/logs/requestLogViewModels.ts`
- Modify: `scripts/request-log-observability-table.test.mjs`

- [x] **Step 1: Add failing frontend contract assertions**

Require the table to import lucide `ArrowDown`, `ArrowUp`, and `Database`, render dedicated `TokenUsageCell` and `LatencyCell` components, use a teal vertical latency bar, and resolve the group label through a helper that prefers `KeyPoolItem.groupName`.

- [x] **Step 2: Verify RED**

Run: `node scripts/request-log-observability-table.test.mjs`

Expected: FAIL because the table still uses generic `Breakdown` cells and raw binding IDs.

- [x] **Step 3: Implement group and compact cells**

Add `formatGroupName(log, keyById)` to prefer the current Key's `groupName`, then persisted binding ID, then `未分组`. Replace the Token renderer with fixed-size colored icons and compact counts. Replace the latency renderer with a stable teal bar and aligned `首字` / `总耗时` rows.

- [x] **Step 4: Verify GREEN**

Run the focused Node test and `node scripts/logs-pricing-status-display.test.mjs`. Expected: PASS.

### Task 5: Full Verification

**Files:**
- Verify all modified production and test files

- [x] **Step 1: Run Rust verification**

Run: `cargo test --manifest-path src-tauri/Cargo.toml services::channel_monitors --lib`

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: both exit successfully.

- [x] **Step 2: Run frontend verification**

Run: `node scripts/request-log-observability-table.test.mjs`

Run: `pnpm.cmd build`

Expected: all commands exit successfully.

- [x] **Step 3: Inspect the local desktop UI**

Run the current-source Tauri development app, generate or wait for a monitor row, and verify the group name, stream label, reasoning effort, Token icons, and latency bar at desktop width. Confirm no cell overlaps and historical rows retain sensible fallbacks.

- [x] **Step 4: Audit scope**

Run: `git diff --check` and `git status --short`.

Expected: no whitespace errors; unrelated Dashboard changes remain preserved and distinguishable from this task.
