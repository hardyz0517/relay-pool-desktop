# Stage 6 Task 23.H Audit - Credentials Command Module Split

Date: 2026-07-27

## Scope

- Move credential IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata and `CredentialsCommandFacade` behavior.
- Do not modify Persistence V2.

## Implementation Notes

- Added `src-tauri/src/commands/credentials.rs` for:
  - `get_station_credentials`
  - `update_station_credentials`
  - `update_station_session`
  - `clear_station_credentials`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed credential command bodies and no-longer-needed facade/DTO imports from `commands/mod.rs`.
- `StationIdInputDto` remains imported by `commands/mod.rs` because station-key command handlers still use it.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml credentials --lib -- --nocapture` - 4 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, external/local-proxy, station-key, channel-monitoring, operations, routing/pricing, collector and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
