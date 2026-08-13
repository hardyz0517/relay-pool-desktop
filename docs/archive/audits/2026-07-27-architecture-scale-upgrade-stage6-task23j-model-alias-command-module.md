# Stage 6 Task 23.J Audit - Model Alias Command Module Split

Date: 2026-07-27

## Scope

- Move model-alias IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata and `RoutingCommandFacade` behavior.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/model_aliases.rs` for:
  - `list_model_aliases`
  - `upsert_model_alias`
  - `delete_model_alias`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed model-alias command bodies and no-longer-needed model-alias DTO imports from `commands/mod.rs`.
- Existing public command error mapping is reused from the parent command boundary.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml model_alias --lib -- --nocapture` - 1 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, external/local-proxy, station-key, channel-monitoring, operations, routing/pricing, collector and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
