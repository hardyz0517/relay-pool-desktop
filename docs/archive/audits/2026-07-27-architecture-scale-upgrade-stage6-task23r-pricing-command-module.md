# Stage 6 Task 23.R Audit - Pricing Command Module Split

Date: 2026-07-27

## Scope

- Move pricing rule, model base price, pricing context and balance snapshot IPC command handlers out of `src-tauri/src/commands/mod.rs` into a domain command module.
- Preserve public command names, DTO parsing, generated bindings, registry metadata, `PricingCommandFacade` behavior and the station-scoped balance snapshot read through `RoutingCommandFacade`.
- Keep the pricing comparison workspace command in its existing `pricing_workspace` module.
- Persistence V2 is outside this shard's implementation scope.

## Implementation Notes

- Added `src-tauri/src/commands/pricing.rs` for:
  - `list_pricing_rules`
  - `list_model_base_prices`
  - `upsert_model_base_price`
  - `reset_model_base_prices_to_builtins`
  - `upsert_pricing_rule`
  - `delete_pricing_rule`
  - `resolve_station_key_pricing_context`
  - `list_balance_snapshots`
  - `list_current_station_balance_snapshots`
  - `list_balance_snapshots_for_station`
  - `upsert_balance_snapshot`
- Updated `src-tauri/src/ipc/registry.rs` to point each command handler at its real module path, keeping command ids unchanged.
- Removed pricing command bodies and no-longer-needed facade/DTO imports from `commands/mod.rs`.
- Existing public command error mapping is reused from the parent command boundary.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `cargo test --locked --manifest-path src-tauri\Cargo.toml pricing --lib -- --nocapture` - 25 passed
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic

## Known Follow-Up

- Continue Task 23 by moving remaining command domains one shard at a time.
- `commands/mod.rs` still contains data-store startup/recovery, CCSwitch import, local-proxy, station-key connectivity, collector metadata/collection and capture command bodies/helpers; Stage 6 Gate is not claimed by this shard.
