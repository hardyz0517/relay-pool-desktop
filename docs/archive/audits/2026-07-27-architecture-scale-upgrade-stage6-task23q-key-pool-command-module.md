# Stage 6 Task 23.Q Audit - Key Pool Command Module Split

Date: 2026-07-27

## Scope

- Move station-key, key-pool and remote-key IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata, `KeyPoolCommandFacade` behavior and `RemoteKeysCommandFacade` behavior.
- Keep station-key connectivity probe/operation commands in `commands/mod.rs` for a later shard because they carry substantial outbound helper logic.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/key_pool.rs` for:
  - `list_station_keys`
  - `create_station_key`
  - `update_station_key`
  - `save_station_key_with_defaults`
  - `update_station_key_group_binding`
  - `delete_station_key`
  - `reorder_station_keys`
  - `get_remote_key_capability`
  - `list_remote_station_keys`
  - `scan_remote_station_keys`
  - `create_remote_station_key`
  - `create_local_station_key_from_remote`
  - `bind_remote_station_key`
  - `unbind_remote_station_key`
  - `list_key_pool_items`
  - `reorder_key_pool`
  - `get_station_key_capabilities`
  - `update_station_key_capabilities`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed key-pool/remote-key command bodies and no-longer-needed facade/DTO imports from `commands/mod.rs`.
- Moved the remote-key public error mapping into the new module with the same public machine classification, and updated the existing unit-test reference.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml key_pool --lib -- --nocapture` - 0 matched
- `cargo test --locked --manifest-path src-tauri\Cargo.toml station_key --lib -- --nocapture` - 36 passed
- `cargo test --locked --manifest-path src-tauri\Cargo.toml remote_key --lib -- --nocapture` - 15 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, CCSwitch import, local-proxy, station-key connectivity, pricing rules/balances, collector and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
