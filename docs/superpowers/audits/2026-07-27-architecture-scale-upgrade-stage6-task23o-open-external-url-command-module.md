# Stage 6 Task 23.O Audit - Open External URL Command Module Split

Date: 2026-07-27

## Scope

- Move the `open_external_url` IPC command handler out of `src-tauri/src/commands/mod.rs` into the existing settings command module.
- Preserve public command name, DTO parsing, generated bindings, registry metadata and external URL validation behavior.
- Keep shared URL launcher helpers in `commands/mod.rs` because CCSwitch import still uses them.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `open_external_url` to `src-tauri/src/commands/settings.rs`.
- Updated `src-tauri/src/ipc/registry.rs` to point the command handler at its real module path, keeping the command id unchanged.
- Removed the `open_external_url` command body and no-longer-needed DTO import from `commands/mod.rs`.
- Existing validation and system URL launch helpers are reused from the parent command boundary.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml external_url --lib -- --nocapture` - 2 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, CCSwitch import, local-proxy, station-key, channel-monitoring, station-key connectivity, pricing rules/balances, collector and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
