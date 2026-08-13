# Stage 6 Task 23.S Audit - Collector Metadata Command Module Split

Date: 2026-07-27

## Scope

- Move collector metadata/read-model IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata and `CollectorMetadataCommandFacade` behavior.
- Keep station collection execution, login probes and capture commands in `commands/mod.rs` for later shards.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/collector_metadata.rs` for:
  - `list_station_group_bindings`
  - `list_station_group_options`
  - `upsert_station_group_binding`
  - `list_group_rate_records`
  - `list_collector_runs`
  - `list_collector_snapshots`
  - `get_latest_collector_snapshot`
  - `list_latest_collector_snapshots`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed collector metadata command bodies and no-longer-needed facade/DTO imports from `commands/mod.rs`.
- Existing public command error mapping is reused from the parent command boundary.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml collector_facts --lib -- --nocapture` - 2 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, CCSwitch import, local-proxy, station-key connectivity, station collection/login and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
