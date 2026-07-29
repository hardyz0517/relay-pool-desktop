# Stage 6 Task 23.M Audit - Channel Status Command Module Split

Date: 2026-07-27

## Scope

- Move channel-status IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata and `ChannelStatusCommandFacade` behavior.
- Keep channel monitor mutation/template/run commands in `commands/mod.rs` for later shards.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/channel_status.rs` for:
  - `list_channel_status_summaries`
  - `load_channel_status_workspace`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed channel-status command bodies and no-longer-needed channel-status facade/DTO imports from `commands/mod.rs`.
- Existing public command error mapping is reused from the parent command boundary.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml channel_status --lib -- --nocapture` - 0 matched
- `cargo test --locked --manifest-path src-tauri\Cargo.toml channel_monitor_operations --lib -- --nocapture` - 2 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, external/local-proxy, station-key, channel-monitoring, station-key connectivity, pricing, collector and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
