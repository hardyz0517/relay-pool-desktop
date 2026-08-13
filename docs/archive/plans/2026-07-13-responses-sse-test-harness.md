# Responses SSE Test Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the Windows socket-reset race in the Responses SSE proxy test without changing production proxy behavior.

**Architecture:** Keep the existing fake upstream and response assertions, but replace its header-only socket read with the module's existing `read_http_request` parser. The parser consumes the complete `Content-Length` body before the fake upstream responds and closes the connection.

**Tech Stack:** Rust, Tauri 2, `std::net::TcpListener`, Cargo test

---

## File Structure

- `src-tauri/src/services/proxy/runtime.rs`: Contains the production HTTP parser and the failing in-module Responses SSE test fixture. Only the test fixture will change.

### Task 1: Make the Responses SSE fake upstream consume complete requests

**Files:**
- Modify: `src-tauri/src/services/proxy/runtime.rs:3692`
- Test: `src-tauri/src/services/proxy/runtime.rs:3688`

- [ ] **Step 1: Verify the existing regression test is RED**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::proxy::runtime::tests::forward_responses_request_streams_with_sse_accept_header -- --exact --nocapture
```

Expected: FAIL at the status assertion with `left: 502` and `right: 200`.

- [ ] **Step 2: Replace the header-only read with the complete parser**

Inside the fake upstream loop, replace the manual buffer, deadline, and `server_stream.read(...)` loop with:

```rust
let upstream_request =
    read_http_request(&mut server_stream).expect("read complete upstream request");
let accepts_sse = upstream_request
    .headers
    .get("accept")
    .is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"));
```

Then replace:

```rust
if !request_text.contains("accept: text/event-stream") {
```

with:

```rust
if !accepts_sse {
```

Keep the existing JSON and SSE responses and the two-request upper bound unchanged.

- [ ] **Step 3: Run the focused test and verify GREEN**

Run the Step 1 command again.

Expected: PASS with `1 passed; 0 failed`.

- [ ] **Step 4: Repeat the focused test to rule out packet-timing dependence**

Run:

```powershell
1..5 | ForEach-Object {
  cargo test --manifest-path src-tauri/Cargo.toml services::proxy::runtime::tests::forward_responses_request_streams_with_sse_accept_header -- --exact
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
```

Expected: all five invocations PASS.

- [ ] **Step 5: Run the complete project verification**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
pnpm build
```

Expected: the complete Rust suite has zero failures, Cargo check and format check exit successfully, and the TypeScript/Vite build succeeds.

- [ ] **Step 6: Confirm production code is unchanged**

Run:

```powershell
git diff --unified=20 -- src-tauri/src/services/proxy/runtime.rs
```

Expected: the diff is confined to `forward_responses_request_streams_with_sse_accept_header` inside the `#[cfg(test)]` module.

- [ ] **Step 7: Commit the test harness fix**

```powershell
git add -- src-tauri/src/services/proxy/runtime.rs
git commit -m "test: stabilize responses SSE upstream fixture"
```

### Task 2: Finish the feature branch

**Files:**
- Review: commits after `7d082dc`

- [ ] **Step 1: Confirm the worktree is clean and review the commit range**

Run:

```powershell
git status --short
git log --oneline 7d082dc..HEAD
git diff --check 7d082dc...HEAD
```

Expected: the worktree is clean, the commit range contains only the tray fix and approved test-harness work, and the diff check exits successfully.

- [ ] **Step 2: Enter branch completion workflow**

Use `superpowers:finishing-a-development-branch`. Because the user authorized this test fix to enable completion, present the required merge, PR, keep, and discard options only after the complete test suite passes.
