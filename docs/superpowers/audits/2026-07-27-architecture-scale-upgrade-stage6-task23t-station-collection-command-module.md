# Stage 6 Task 23.T Audit - Station Collection Command Module Split

Date: 2026-07-27

## Scope

- Move station collection and login probe IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata, `StationCollectionCommandFacade` behavior and async login probe behavior.
- Keep collector metadata commands in their existing module and keep capture/session commands in `commands/mod.rs` for later shards.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/station_collection.rs` for:
  - `detect_sub2api_station`
  - `collect_sub2api_station`
  - `detect_station_info`
  - `collect_station_info`
  - `collect_station_task`
  - `test_station_login`
  - `test_station_login_input`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed station collection/login command bodies and no-longer-needed facade/DTO imports from `commands/mod.rs`.
- Moved station collection/login public error mapping into the new module while continuing to reuse parent correlation and blocking-executor helpers.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml station_collector --lib -- --nocapture` - 10 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, CCSwitch import, local-proxy, station-key connectivity and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
