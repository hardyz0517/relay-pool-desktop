# NewAPI Strict Collection Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every NewAPI balance, usage, token, request-count, and group-rate fact traceable to an authoritative QuantumNous/new-api source without guessed or truncated values.

**Architecture:** Keep the existing NewAPI adapter boundary, but make status conversion explicitly optional and select each usage metric by endpoint semantics. Preserve `None` through facts, persistence, and presentation when conversion or full coverage cannot be proved.

**Tech Stack:** Rust, serde_json, reqwest-based collector client, SQLite, React/TypeScript view models.

---

### Task 1: Remove quota-to-token conflation

**Files:**
- Modify: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`

- [x] Add a regression where status is `TOKENS`, `used_quota` differs from dashboard `token_used`, and assert total token comes from the dashboard only.
- [x] Run `cargo test -p relay-pool-desktop newapi_balance_does_not_treat_used_quota_as_tokens -- --nocapture` and confirm it fails against the current dirty implementation.
- [x] Remove `apply_direct_total_tokens_for_token_display`, the `total_tokens_are_direct` branch, and its call site.
- [x] Re-run the targeted test and confirm it passes.

### Task 2: Reject incomplete log facts and missing stat quota

**Files:**
- Modify: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`

- [x] Add tests proving a 10000-row capped log response cannot produce an exact request count and a stat response without `quota` cannot produce zero consumption.
- [x] Run both tests and confirm the expected failures.
- [x] Return log `request_count` only when `completed_window` is true, and map stat consumption only when a valid `quota` exists.
- [x] Re-run both tests and confirm they pass.

### Task 3: Make quota conversion strict

**Files:**
- Modify: `src-tauri/src/services/collectors/adapters/newapi/parsers.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`

- [x] Add parser and adapter tests where `quota_per_unit` is absent or non-positive and assert converted balance/consumption values remain unknown.
- [x] Run the tests and confirm they fail because the current parser substitutes 500000.
- [x] Change `NewApiStatus.quota_per_unit` to `Option<f64>` and only collect converted monetary metrics when it is positive.
- [x] Keep account `request_count` and direct dashboard token/count facts available even when monetary conversion is unavailable.
- [x] Re-run the targeted tests and confirm they pass.

### Task 4: Correct group ratio semantics

**Files:**
- Modify: `src-tauri/src/services/collectors/adapters/newapi/parsers.rs`

- [x] Add a parser test asserting `/api/user/self/groups.ratio` populates only `effective_rate_multiplier`.
- [x] Run the test and confirm it fails because the current parser duplicates the value into three fields.
- [x] Set `default_rate_multiplier` and `user_rate_multiplier` to `None`.
- [x] Re-run the parser test and confirm it passes.

### Task 5: Final source and regression audit

**Files:**
- Verify: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`
- Verify: `src-tauri/src/services/collectors/adapters/newapi/parsers.rs`
- Verify: `src-tauri/src/services/collectors/apply.rs`
- Verify: `src/features/stations/stationDetailViewModels.ts`

- [x] Search the final adapter for `tpm`, `used_quota` token mappings, `unwrap_or(0.0)` on remote usage fields, and unchecked log totals.
- [x] Run `cargo test -p relay-pool-desktop services::collectors::adapters::newapi -- --nocapture`.
- [x] Run `cargo fmt --check`, `git diff --check`, and `cargo check -p relay-pool-desktop`.
- [x] Inspect the final task-only diff and document any source limitation that necessarily remains.

No commit or push step is included because this task does not authorize VCS publication.

### Final audit additions

- [x] Continue all-time dashboard search past empty recent windows because current `/api/user/self` does not expose account creation time.
- [x] Propagate missing metrics across dashboard windows instead of summing only the windows that happened to provide a field.
- [x] Reject partial dashboard rows, incomplete log token fields, negative token values, non-finite conversion values, and malformed pagination metadata.
- [x] Restrict `/api/user/models`, `/api/token/`, token reveal, and `/api/status` parsing to the response structures proven by the pinned upstream source.
