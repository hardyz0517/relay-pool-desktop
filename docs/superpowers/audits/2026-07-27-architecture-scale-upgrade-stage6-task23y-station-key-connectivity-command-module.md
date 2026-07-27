# Stage 6 Task 23.Y Audit - Station Key Connectivity Command Module Split

Date: 2026-07-27

## Scope

- Move station-key connectivity IPC command handlers out of `src-tauri/src/commands/mod.rs` into a focused command module.
- Preserve public command names, DTO parsing, operation registration behavior, streaming event envelope schema, outbound probe behavior, model discovery/candidate selection and connectivity result recording.
- Preserve registry metadata, generated TypeScript surface and channel streaming contract.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/station_key_connectivity.rs` for:
  - `start_station_key_connectivity_operation`
  - `test_station_key_connectivity`
  - `StationKeyConnectivityTestResult`
  - `StationKeyConnectivityTestEvent`
  - streaming progress/event helpers
  - outbound discovery/probe helpers
  - station-key connectivity unit tests
- Updated `src-tauri/src/ipc/registry.rs` command handlers to point to `commands::station_key_connectivity::*` while preserving command ids and contract metadata.
- Removed station-key connectivity command bodies, helper functions and tests from `commands/mod.rs`.
- Left `public_operation_registry_error` in `commands/mod.rs` because `commands/operations.rs` also uses it as a shared public work-error mapping.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml station_key_connectivity --lib -- --nocapture` - 27 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Run Task 23 / Stage 6 closeout gates now that command bodies are physically split.
- Stage 6 Gate is not claimed by this shard alone.
