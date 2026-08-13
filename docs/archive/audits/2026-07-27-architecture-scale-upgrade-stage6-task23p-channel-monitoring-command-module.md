# Stage 6 Task 23.P Audit - Channel Monitoring Command Module Split

Date: 2026-07-27

## Scope

- Move channel-monitoring IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata and `ChannelMonitoringCommandFacade` behavior.
- Keep channel status commands in their existing module and keep unrelated operation/connectivity helpers in `commands/mod.rs`.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/channel_monitoring.rs` for:
  - `list_channel_monitors`
  - `list_channel_monitor_summaries`
  - `create_channel_monitor`
  - `update_channel_monitor`
  - `delete_channel_monitor`
  - `list_channel_monitor_runs`
  - `list_channel_monitor_templates`
  - `create_channel_monitor_template`
  - `update_channel_monitor_template`
  - `duplicate_channel_monitor_template`
  - `delete_channel_monitor_template`
  - `run_channel_monitor_now`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed channel-monitoring command bodies and no-longer-needed facade/DTO imports from `commands/mod.rs`.
- Moved the `run_channel_monitor_now` public error mapping into the new module and updated its existing unit-test reference.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml channel_monitor --lib -- --nocapture` - 40 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, CCSwitch import, local-proxy, station-key, station-key connectivity, pricing rules/balances, collector and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
