# Stage 6 Task 23.C Audit - Settings Command Module Split

Date: 2026-07-27

## Scope

- Move settings IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata and settings facade behavior.
- Update the command state boundary gate so future Task 23 shards can validate command handlers at their real module paths.
- Do not modify Persistence V2.

## Implementation Notes

- Added `src-tauri/src/commands/settings.rs` for:
  - `get_settings`
  - `get_local_access_key`
  - `update_local_access_key`
  - `update_settings`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed settings command bodies and no-longer-needed input DTO imports from `commands/mod.rs`.
- `choose_data_dir` and `reset_data_dir` still return `SettingsDto`, so that output DTO remains imported by `commands/mod.rs`.
- `scripts/architecture/check-command-state-boundaries.mjs` now reads the compiled IPC registry mapping and validates migrated command signatures in the handler source file rather than assuming every command lives in `commands/mod.rs`.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml --check`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml settings --lib -- --nocapture` - 14 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store, stations, external/local-proxy, logs, station-key, channel-monitoring, operations, routing/pricing, collector and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
