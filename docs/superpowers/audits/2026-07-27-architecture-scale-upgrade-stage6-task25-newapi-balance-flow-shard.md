# Stage 6 Task 25.C Shard - NewAPI Balance Flow Driver Tests

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Move the remaining NewAPI full balance-flow behavior checks from the legacy adapter test surface to the NewAPI driver.
- Keep the ignored live-login smoke test and legacy sync auth/client helper tests in place until their closeout path is decided.
- Do not modify Persistence V2 work.

## Changes

- Added driver-context async `CollectorTaskKind::Balance` tests in `src-tauri/src/services/collectors/drivers/newapi/mod.rs` for:
  - request count, cost, and total token collection from usage logs,
  - token display mode not treating `used_quota` as token count,
  - recent dashboard windows not being accepted as all-time totals,
  - partial dashboard total token rows being rejected for total token output,
  - zero self `used_quota` preventing stale dashboard totals from being trusted,
  - empty log token counts leaving input, output, and total token facts unknown.
- Added a test-only driver helper that builds the fixture-backed `CollectorContext` and runs `NewApiCollectorDriver.collect(... Balance)`.
- Removed the six corresponding sync adapter balance tests from `src-tauri/src/services/collectors/adapters/newapi/mod.rs`.

## Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml` - passed
- `cargo test --manifest-path src-tauri\Cargo.toml --lib newapi` - 68 passed, 1 ignored
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` - 89 passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries` - 4 passed

## Boundary Notes

- Remaining `collectors/adapters/newapi/**` coverage is now the ignored live login test plus legacy sync `auth.rs` and `client.rs` helper tests.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/superpowers/audits/persistence-v2-boundary-manifest.json`

## Follow-Up

- Decide whether the ignored live NewAPI password-login smoke belongs in a driver-side authorization/live harness, a documented manual qualification step, or deletion.
- Replace or retire the remaining legacy sync `auth.rs` and `client.rs` helper tests before deleting the adapter compatibility island.
- Task 25.A/25.B/25.D/25.E and the Stage 6 Gate are not claimed by this shard.
