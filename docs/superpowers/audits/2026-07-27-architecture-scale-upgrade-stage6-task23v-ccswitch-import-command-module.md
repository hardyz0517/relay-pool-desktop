# Stage 6 Task 23.V Audit - CCSwitch Import Command Module Split

Date: 2026-07-27

## Scope

- Move the CCSwitch import IPC command handler out of `src-tauri/src/commands/mod.rs` into a focused command module.
- Preserve public command name, DTO parsing, generated bindings, registry metadata, `LocalProxyCommandFacade::import_relay_pool_to_ccswitch` behavior and CCSwitch deeplink shape.
- Keep shared system URL launch helpers in `commands/mod.rs` because settings/data-store commands still reuse them.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/ccswitch_import.rs` for:
  - `import_relay_pool_to_ccswitch`
  - `prepare_ccswitch_import`
  - `build_ccswitch_provider_deeplink`
  - `encode_query_param`
- Updated `src-tauri/src/ipc/registry.rs` to point the command handler at its real module path, keeping the command id unchanged.
- Removed CCSwitch import command and deeplink helper bodies from `commands/mod.rs`.
- Updated existing CCSwitch unit tests to reference the moved helper functions through the new module.
- Preserved the existing CCSwitch provider query shape, including `model=gpt-5.4` for Codex imports, `configFormat=json`, usage metadata and enabled state.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml ccswitch --lib -- --nocapture` - 3 passed after rerunning without cargo lock contention
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, station-key connectivity and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
