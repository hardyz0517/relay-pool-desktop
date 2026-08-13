# Stage 6 Task 23.A Audit - Updater Command Module Split

Date: 2026-07-27

## Scope

- Move updater IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata and facade/service behavior.
- Do not modify Persistence V2.

## Implementation Notes

- Added `src-tauri/src/commands/updater.rs` for:
  - `updater_network_config`
  - `inspect_latest_update_manifest`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping the command ids unchanged.
- Removed updater DTO/service imports and command bodies from `commands/mod.rs`.
- No updater service, DTO shape, frontend binding, ACL or runtime behavior was changed.

## Deterministic Evidence

- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic
- `cargo test --locked --manifest-path src-tauri\Cargo.toml updater --lib -- --nocapture` - 8 passed

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains command bodies and helpers outside updater; Stage 6 Gate is not claimed by this shard.
