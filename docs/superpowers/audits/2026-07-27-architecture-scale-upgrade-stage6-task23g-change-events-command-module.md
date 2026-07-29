# Stage 6 Task 23.G Audit - Change Events Command Module Split

Date: 2026-07-27

## Scope

- Move change-event IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata and `ChangeEventsCommandFacade` behavior.
- Do not modify Persistence V2.

## Implementation Notes

- Added `src-tauri/src/commands/change_events.rs` for:
  - `list_change_events`
  - `clear_change_events`
  - `list_change_events_for_station`
  - `upsert_change_event`
  - `mark_change_event_read`
  - `mark_change_events_read`
  - `dismiss_change_event`
  - `resolve_change_event`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed change-event command bodies and no-longer-needed facade/DTO imports from `commands/mod.rs`.
- Existing read caps (`PageLimit::new(200)`) and mutation behavior are preserved.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml change_event --lib -- --nocapture` - 0 matched; lib test cfg compiled
- `cargo test --locked --manifest-path src-tauri\Cargo.toml change_events --lib -- --nocapture` - 0 matched; lib test cfg compiled
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, external/local-proxy, station-key, channel-monitoring, operations, routing/pricing, collector and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
