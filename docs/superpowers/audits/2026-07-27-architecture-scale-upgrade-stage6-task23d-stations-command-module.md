# Stage 6 Task 23.D Audit - Stations Command Module Split

Date: 2026-07-27

## Scope

- Move station CRUD/reorder IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata and `SettingsStationsCommandFacade` behavior.
- Do not modify Persistence V2.

## Implementation Notes

- Added `src-tauri/src/commands/stations.rs` for:
  - `list_stations`
  - `create_station`
  - `update_station`
  - `delete_station`
  - `reorder_stations`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed station command bodies and no-longer-needed station input/output DTO imports from `commands/mod.rs`.
- The nearby data-store helpers and `is_supported_database_file` remain in `commands/mod.rs`; moving data-store commands is a separate shard.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml --check`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml stations --lib -- --nocapture` - 4 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store, external/local-proxy, logs, station-key, channel-monitoring, operations, routing/pricing, collector and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
