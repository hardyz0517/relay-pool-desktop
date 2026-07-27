# Stage 6 Task 23.L Audit - Operation Status Command Module Split

Date: 2026-07-27

## Scope

- Move operation status/cancel IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata and `ManagedWorkRuntime` behavior.
- Keep station-key connectivity operation startup and helper logic in `commands/mod.rs` for a later, larger shard.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/operations.rs` for:
  - `get_operation_status`
  - `cancel_operation`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed operation status/cancel command bodies and no-longer-needed operation DTO imports from `commands/mod.rs`.
- Existing operation registry error mapping remains in the parent command boundary because `start_station_key_connectivity_operation` still uses it.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml operation_status --lib -- --nocapture` - 0 matched
- `cargo test --locked --manifest-path src-tauri\Cargo.toml operation --lib -- --nocapture` - 15 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, external/local-proxy, station-key, channel-monitoring, station-key connectivity, pricing, collector and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
