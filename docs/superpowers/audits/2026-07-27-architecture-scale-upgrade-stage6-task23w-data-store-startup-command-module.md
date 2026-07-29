# Stage 6 Task 23.W Audit - Data Store Startup Command Module Split

Date: 2026-07-27

## Scope

- Move data-store startup and recovery IPC command handlers out of `src-tauri/src/commands/mod.rs` into a focused command module.
- Preserve public command names, DTO parsing, recovery view mapping, located-candidate evidence registry behavior, data-store backup/diagnostic launch behavior and command registry metadata.
- Update application state registration to manage the moved `LocatedDataStoreCandidates` type through the new module path.
- Persistence V2 implementation and migrations are outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/data_store_startup.rs` for:
  - `LocatedDataStoreCandidates`
  - `get_data_store_startup_state`
  - `refresh_data_store_candidates`
  - `locate_data_store_candidate`
  - `activate_data_store_candidate`
  - `create_new_data_store`
  - `open_data_store_backup_dir`
  - `export_data_store_diagnostic`
- Kept `commands/data_recovery.rs` as the recovery-only DTO/authorization mapping boundary.
- Moved data-store-only helpers (`data_store_updated_at`, supported database filename check and path launcher) with their only command callers.
- Updated `src-tauri/src/ipc/registry.rs` command handlers to point to `commands::data_store_startup::*` while preserving command ids.
- Updated `src-tauri/src/lib.rs` state registration to manage `commands::data_store_startup::LocatedDataStoreCandidates`.
- Moved the existing located-candidate registry unit tests with the state owner.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml located_candidate --lib -- --nocapture` - 2 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic after rerunning a prior no-output timeout

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains station-key connectivity and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
