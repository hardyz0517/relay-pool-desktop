# Stage 5 Task 22.C Audit - Updater Manifest Async Outbound Cutover

Date: 2026-07-27

## Scope

- Cut over updater latest manifest inspection from `spawn_blocking` plus legacy `ureq` transport to the shared `AsyncOutboundClient`.
- Preserve updater network config inspection, manifest JSON parsing and published-version relation behavior.
- Keep final production `ureq` dependency cleanup for the next Task 22 shard.
- Keep Persistence V2 untouched.

## Implementation Notes

- `services::updater::inspect_latest_update_manifest` is now async and accepts the shared outbound client.
- Updater manifest inspection builds a typed outbound `GET` request with `ProxyPolicy::System`, `Accept: application/json`, a 10 second budget, empty body and no status retry.
- `commands::inspect_latest_update_manifest` now awaits the async updater service through the managed work runtime outbound client instead of entering `tauri::async_runtime::spawn_blocking`.
- Added a focused unit test that locks the updater manifest request policy.
- Removed the updater direct `spawn_blocking` exception from the architecture boundary manifest.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo fmt --manifest-path src-tauri\Cargo.toml --check`
- `cargo test --locked --manifest-path src-tauri\Cargo.toml updater --lib -- --nocapture` - 8 passed
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib`
- `node scripts\architecture\check-command-state-boundaries.mjs`
- `cargo test --locked --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries -- --nocapture` - 4 passed
- `pnpm generate:bindings --check`
- JSON parse: `architecture-scale-boundary-manifest.json`
- Literal scan: updater service and command no longer contain updater `ureq`, legacy outbound agent construction or updater manifest `spawn_blocking`.
- Persistence V2 zero diff: `git diff -- src-tauri/src/persistence src-tauri/migrations docs/superpowers/audits/persistence-v2-boundary-manifest.json`

## Known Follow-Up

- Task 22 still owns final production `ureq` removal and legacy outbound/dead adapter cleanup.
- Remaining legacy provider adapter dead code is intentionally left for Task 25/26 cleanup after all cutovers are complete.
- Stage 5 Gate is not claimed by this shard.
