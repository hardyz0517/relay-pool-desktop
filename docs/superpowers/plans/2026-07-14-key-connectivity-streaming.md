# Key Connectivity Streaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the station-key connectivity test request streaming first, display real deltas, and retry the same endpoint once with a non-streaming request when streaming fails.

**Architecture:** The frontend creates a request-scoped Tauri `Channel` and passes it into the existing command. Rust probes each model/protocol in the existing order, using a streaming HTTP attempt first and a same-endpoint non-streaming fallback second. A small Rust SSE decoder owns protocol parsing, terminal-signal validation, and bounded buffering.

**Tech Stack:** Tauri 2 IPC `Channel`, Rust `ureq` + `serde_json`, React/TypeScript, Node source-contract tests, Cargo unit tests.

---

## File Structure

- Modify `scripts/key-pool-connectivity-dialog.test.mjs`: source-contract assertions for Channel usage, typed progress events, stream-first request bodies, fallback metadata, and no fake interval.
- Modify `src/lib/types/stationKeys.ts`: add `StationKeyConnectivityTestEvent`, `StationKeyConnectivityResponseMode`, `responseMode`, and `streamFallbackReason`.
- Modify `src/lib/api/stationKeys.ts`: create a Tauri `Channel`, pass it to `test_station_key_connectivity`, and expose an optional `onEvent` callback.
- Modify `src/features/key-pool/KeyPoolPage.tsx`: maintain per-run streaming text/status, ignore stale events, clear partial text on fallback, and pass progress handling to the API.
- Modify `src-tauri/src/commands/mod.rs`: add Channel payload structs, stream/non-stream body mode, SSE decoder, stream-first probe flow, fallback metadata, and unit tests.

---

### Task 1: Frontend And Contract RED

**Files:**
- Modify: `scripts/key-pool-connectivity-dialog.test.mjs`
- Modify: `src/lib/types/stationKeys.ts`
- Modify: `src/lib/api/stationKeys.ts`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Update the Node contract test first**

Assert that `src/lib/api/stationKeys.ts` imports `Channel`, creates `new Channel<StationKeyConnectivityTestEvent>()`, assigns `channel.onmessage`, and invokes `test_station_key_connectivity` with `{ stationKeyId, model, progress: channel }`.

Assert that `src/features/key-pool/KeyPoolPage.tsx` handles `attemptStarted`, `delta`, and `fallback`, tracks `connectivityRunTokenRef`, clears partial output on fallback, and still has no `window.setInterval`.

Assert that `src-tauri/src/commands/mod.rs` accepts a `Channel<StationKeyConnectivityTestEvent>`, has `StationKeyConnectivityResponseMode`, adds `response_mode` and `stream_fallback_reason`, uses stream and non-stream body modes, sets `Accept` to `text/event-stream` for stream and `application/json` for fallback, and contains an SSE decoder.

- [ ] **Step 2: Run the focused Node contract and verify RED**

Run: `node scripts/key-pool-connectivity-dialog.test.mjs`

Expected: FAIL because the API does not create a Channel, Rust command has no progress channel, and fallback metadata does not exist.

---

### Task 2: Rust Unit RED

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Add Rust tests before implementation**

Add tests covering:

```rust
#[test]
fn station_key_connectivity_stream_bodies_request_streaming() {
    let responses = build_station_key_connectivity_probe_body(
        "gpt-test",
        StationKeyConnectivityProbeKind::Responses,
        StationKeyConnectivityRequestMode::Stream,
    );
    let chat = build_station_key_connectivity_probe_body(
        "gpt-test",
        StationKeyConnectivityProbeKind::ChatCompletions,
        StationKeyConnectivityRequestMode::Stream,
    );

    assert_eq!(responses["stream"], true);
    assert_eq!(chat["stream"], true);
}
```

Also add decoder tests for Responses split chunks, Chat CRLF comments plus `[DONE]`, malformed JSON, missing terminal signal, and oversized pending data.

Add orchestration tests proving a stream success does not retry non-stream, and a stream failure retries the same protocol once as non-stream before existing protocol/model fallback continues.

- [ ] **Step 2: Run focused Cargo tests and verify RED**

Run: `cargo test station_key_connectivity --lib`

Expected: FAIL because request mode, decoder, response mode, and fallback orchestration are not implemented.

---

### Task 3: Rust Streaming Backend GREEN

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Add serializable event and response-mode types**

Add:

```rust
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum StationKeyConnectivityResponseMode {
    Stream,
    NonStreamFallback,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum StationKeyConnectivityTestEvent {
    AttemptStarted { model: String, protocol: String },
    Delta { text: String },
    Fallback { reason: String },
}
```

Extend result structs with `response_mode` and `stream_fallback_reason`.

- [ ] **Step 2: Add request mode and body builder support**

Change `build_station_key_connectivity_probe_body(model, kind)` to `build_station_key_connectivity_probe_body(model, kind, mode)`.

For `Stream`, set `"stream": true` for both protocols. For `NonStream`, preserve current non-stream fields and `store: false`.

- [ ] **Step 3: Add the SSE decoder**

Create `StationKeyConnectivitySseDecoder` with:

- `push(&mut self, chunk: &[u8]) -> Result<Vec<String>, String>`
- `finish(self) -> Result<String, String>`
- bounded pending buffer
- Responses support for `response.output_text.delta` and `response.completed`
- Chat support for `choices[0].delta.content` and `[DONE]`

- [ ] **Step 4: Implement stream-first probe flow**

Refactor `send_station_key_connectivity_probe` into a wrapper that:

1. emits `AttemptStarted`;
2. sends streaming request with `Accept: text/event-stream`;
3. validates 2xx and content type;
4. reads chunks, decodes deltas, emits `Delta`;
5. returns `response_mode: Stream` on terminal success;
6. on any stream failure, emits `Fallback`, then sends one non-stream request with `Accept: application/json`;
7. returns `response_mode: NonStreamFallback` and `stream_fallback_reason` when fallback succeeds.

- [ ] **Step 5: Run focused Cargo tests and verify GREEN**

Run: `cargo test station_key_connectivity --lib`

Expected: PASS.

---

### Task 4: TypeScript Channel And Dialog GREEN

**Files:**
- Modify: `src/lib/types/stationKeys.ts`
- Modify: `src/lib/api/stationKeys.ts`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`

- [ ] **Step 1: Add TypeScript event/result contract**

Add:

```ts
export type StationKeyConnectivityResponseMode = "stream" | "non_stream_fallback";

export type StationKeyConnectivityTestEvent =
  | { type: "attemptStarted"; model: string; protocol: string }
  | { type: "delta"; text: string }
  | { type: "fallback"; reason: string };
```

Extend `StationKeyConnectivityTestResult` with `responseMode` and `streamFallbackReason`.

- [ ] **Step 2: Add request-scoped Channel to the API wrapper**

Change `testStationKeyConnectivity(stationKeyId, model)` to accept an optional `{ onEvent }` option. Create `const progress = new Channel<StationKeyConnectivityTestEvent>()`, assign `progress.onmessage`, and pass `progress` in the invoke payload.

- [ ] **Step 3: Wire dialog streaming state**

In `KeyPoolPage.tsx`, add run-token and streaming status state. During `handleRunConnectivityTest`, increment the token and handle:

- `attemptStarted`: clear response text and show the current protocol/model attempt;
- `delta`: append real text;
- `fallback`: clear partial text and store the redacted fallback reason.

Invalidate the token on close so late events cannot mutate a new dialog session.

- [ ] **Step 4: Run the focused Node contract and verify GREEN**

Run: `node scripts/key-pool-connectivity-dialog.test.mjs`

Expected: PASS.

---

### Task 5: Full Verification

**Files:**
- Verify all touched files.

- [ ] **Step 1: Focused backend tests**

Run: `cargo test station_key_connectivity --lib`

Expected: PASS.

- [ ] **Step 2: Existing focused chat-probe test**

Run: `cargo test station_key_connectivity_chat_probe_uses_low_token_request --lib`

Expected: PASS.

- [ ] **Step 3: Focused frontend contract**

Run: `node scripts/key-pool-connectivity-dialog.test.mjs`

Expected: PASS.

- [ ] **Step 4: Frontend build**

Run: `pnpm.cmd build`

Expected: PASS. If `dist/assets` is transiently locked, rerun once before diagnosing code.

- [ ] **Step 5: Rust check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 6: Diff audit**

Run: `git diff -- src-tauri/src/commands/mod.rs src/lib/api/stationKeys.ts src/lib/types/stationKeys.ts src/features/key-pool/KeyPoolPage.tsx scripts/key-pool-connectivity-dialog.test.mjs docs/superpowers/plans/2026-07-14-key-connectivity-streaming.md`

Expected: Only key-connectivity streaming implementation and tests are changed.

---

## Self-Review

- Spec coverage: stream-first, same-endpoint non-stream fallback, typed Channel events, frontend stale-run guard, response mode metadata, duration including body reads, redacted fallback reasons, bounded SSE decoder, and existing protocol/model fallback are represented.
- Placeholder scan: no TBD/TODO/fill-in-later steps remain.
- Type consistency: `StationKeyConnectivityTestEvent`, `StationKeyConnectivityResponseMode`, `responseMode`, and `streamFallbackReason` names match across Rust serde and TypeScript.
