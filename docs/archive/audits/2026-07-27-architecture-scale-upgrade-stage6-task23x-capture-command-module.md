# Stage 6 Task 23.X Audit - Capture Command Module Split

Date: 2026-07-27

## Scope

- Move capture/session IPC command handlers out of `src-tauri/src/commands/mod.rs` into a focused command module.
- Preserve public command names, DTO parsing, capture session store behavior, capture facade calls, capture window label construction and web authorization capture script behavior.
- Preserve registry metadata and generated TypeScript surface.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/capture.rs` for:
  - `start_capture_session`
  - `get_capture_session_status`
  - `record_capture_event`
  - `clear_capture_session`
  - `close_capture_session`
  - `finish_capture_session`
  - `finish_web_authorization_session`
  - capture-only helper/test coverage for request-origin checks, authorization candidate extraction and capture script contents
- Updated `src-tauri/src/ipc/registry.rs` command handlers to point to `commands::capture::*` while preserving command ids and contract metadata.
- Removed capture command bodies, capture helper functions and capture-only tests from `commands/mod.rs`.
- Kept shared work/error conversion helpers in `commands/mod.rs` for sibling command modules.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml capture --lib -- --nocapture` - 36 passed; includes an expected invalid UTF-8 panic message from a passing fixture test
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving station-key connectivity command bodies/helpers.
- `commands/mod.rs` still contains station-key connectivity command logic; Stage 6 Gate is not claimed by this shard.
