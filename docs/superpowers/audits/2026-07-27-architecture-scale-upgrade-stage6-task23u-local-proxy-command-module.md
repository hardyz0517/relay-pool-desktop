# Stage 6 Task 23.U Audit - Local Proxy Command Module Split

Date: 2026-07-27

## Scope

- Move local proxy IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata, `LocalProxyCommandFacade` behavior and `ProxyRuntimeState::prepare_for_update` behavior.
- Keep CCSwitch import/deeplink command logic in `commands/mod.rs` because it still shares URL-launch and provider-deeplink helpers.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/local_proxy.rs` for:
  - `get_proxy_status`
  - `load_local_routing_workspace`
  - `reorder_local_routing_keys`
  - `start_local_proxy`
  - `stop_local_proxy`
  - `cleanup_before_update`
  - `prepare_local_proxy_for_update`
  - `restart_local_proxy`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed local proxy command bodies and no-longer-needed DTO/runtime imports from `commands/mod.rs`.
- Existing public local proxy error mapping remains in the parent command boundary because `import_relay_pool_to_ccswitch` still uses it.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml local_proxy --lib -- --nocapture` - 0 matched
- `cargo test --locked --manifest-path src-tauri\Cargo.toml proxy --lib -- --nocapture` - 104 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, CCSwitch import, station-key connectivity and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
