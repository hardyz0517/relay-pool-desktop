# Stage 6 Task 23.B Audit - Runtime Command Module Split

Date: 2026-07-27

## Scope

- Move runtime/status IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata and runtime read behavior.
- Do not modify Persistence V2.

## Implementation Notes

- Added `src-tauri/src/commands/runtime.rs` for:
  - `app_status`
  - `get_runtime_contract_info`
  - `get_runtime_status`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed runtime DTO/model/contract imports and command bodies from `commands/mod.rs`.
- No runtime contract payload, public status projection, ACL, frontend binding or background task behavior was changed.

## Deterministic Evidence

- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml runtime_status --lib -- --nocapture` - 2 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store, stations/settings, proxy, logs, station-key, channel-monitoring, operations, routing/pricing, collector and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
