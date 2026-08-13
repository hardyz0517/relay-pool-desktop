# Stage 5 Task 22.B Audit - Station-Key Connectivity Command Async Outbound Cutover

Date: 2026-07-27

## Scope

- Cut over the legacy `test_station_key_connectivity` command from direct `spawn_blocking` plus `ureq` transport to the shared `AsyncOutboundClient`.
- Preserve the command's existing IPC shape, immediate result return, progress channel events and connectivity record commit behavior.
- Keep the operation-backed `start_station_key_connectivity_operation` path unchanged.
- Confirm channel monitor probe was already cut over in `S4-T17A3-channel-monitor-probe-async-outbound`; no channel monitor code changed in this shard.
- Keep updater manifest inspection and final production `ureq` cleanup for later Task 22 shards.
- Keep Persistence V2 untouched.

## Implementation Notes

- `StationKeyConnectivityCommandFacade` now receives the shared outbound client from the composition root, keeping `test_station_key_connectivity` at a single Tauri `State` boundary.
- Removed the command-level `tauri::async_runtime::spawn_blocking` wrapper and the old blocking helper.
- Replaced legacy `ureq::post` probe requests with `AsyncOutboundClient::execute` and `execute_stream` using the existing `outbound_json_request` helper.
- Replaced legacy `/v1/models` discovery through `ureq::get` with the existing async outbound discovery helper.
- Streaming probes still parse SSE chunks with `StationKeyConnectivitySseDecoder` and emit `Delta` progress events through the original command channel.
- The old synchronous orchestration helpers in `application::connectivity_probe` are now test-only; production connectivity command paths use async outbound.
- Removed the `test_station_key_connectivity` direct `spawn_blocking` exception from the architecture boundary manifest.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml --check`
- `cargo test --locked --manifest-path src-tauri\Cargo.toml station_key_connectivity --lib -- --nocapture` - 27 passed
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib`
- `cargo test --locked --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries -- --nocapture` - 4 passed
- `pnpm generate:bindings --check`
- `node scripts\architecture\check-command-state-boundaries.mjs`
- JSON parse: `architecture-scale-boundary-manifest.json`
- Literal scan: `src-tauri/src/commands/mod.rs` no longer contains connectivity `ureq`, legacy blocking helper, response pair helper or model-discovery `ureq` helper.
- Persistence V2 zero diff: `git diff -- src-tauri/src/persistence src-tauri/migrations docs/audits/persistence-v2-boundary-manifest.json`

## Known Follow-Up

- Task 22 still owns updater direct HTTP inspection and final production `ureq` removal.
- Remaining legacy provider adapter dead code is intentionally left for Task 25/26 cleanup after all cutovers are complete.
- Stage 5 Gate is not claimed by this shard.
