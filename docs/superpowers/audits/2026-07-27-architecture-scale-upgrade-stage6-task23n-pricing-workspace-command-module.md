# Stage 6 Task 23.N Audit - Pricing Workspace Command Module Split

Date: 2026-07-27

## Scope

- Move the pricing comparison workspace IPC command handler out of `src-tauri/src/commands/mod.rs` into a focused command module.
- Preserve public command name, DTO parsing, generated bindings, registry metadata and `PricingCommandFacade` behavior.
- Keep pricing rule, model base price and balance snapshot commands in `commands/mod.rs` for later shards.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/pricing_workspace.rs` for `load_pricing_comparison_workspace`.
- Updated `src-tauri/src/ipc/registry.rs` to point the command handler at its real module path, keeping the command id unchanged.
- Removed the pricing workspace command body and no-longer-needed workspace DTO import from `commands/mod.rs`.
- Existing public command error mapping is reused from the parent command boundary.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml pricing_comparison --lib -- --nocapture` - 0 matched
- `cargo test --locked --manifest-path src-tauri\Cargo.toml pricing --lib -- --nocapture` - 25 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, external/local-proxy, station-key, channel-monitoring, station-key connectivity, pricing rules/balances, collector and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
