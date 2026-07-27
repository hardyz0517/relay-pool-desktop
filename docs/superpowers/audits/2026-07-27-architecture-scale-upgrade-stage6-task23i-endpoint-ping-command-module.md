# Stage 6 Task 23.I Audit - Endpoint Ping Command Module Split

Date: 2026-07-27

## Scope

- Move the endpoint ping IPC command handler out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command name, DTO parsing, generated bindings, registry metadata and `RoutingCommandFacade` behavior.
- Do not modify Persistence V2.

## Implementation Notes

- Added `src-tauri/src/commands/endpoint_ping.rs` for `ping_station_endpoint`.
- Updated `src-tauri/src/ipc/registry.rs` to point the command handler at its real module path, keeping the command id unchanged.
- Removed the endpoint ping command body and no-longer-needed result DTO import from `commands/mod.rs`.
- Existing endpoint ping public error mapping is reused from the parent command boundary.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml endpoint_ping --lib -- --nocapture` - 6 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, external/local-proxy, station-key, channel-monitoring, operations, routing/pricing, collector and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
