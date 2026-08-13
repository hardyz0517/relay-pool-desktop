# Stage 6 Task 25.C Shard - NewAPI Usage Helper Driver Tests

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Move NewAPI usage-window, log-stat, dashboard-window, and dashboard-total helper behavior checks onto the NewAPI driver side.
- Remove the corresponding legacy adapter-side helper tests once driver-side coverage is green.
- Do not modify Persistence V2 work.

## Changes

- Added driver-context async tests in `src-tauri/src/services/collectors/drivers/newapi/mod.rs` for:
  - truncated log windows keeping exact request and token totals unknown,
  - missing token fields preserving unknown token totals,
  - malformed log pagination producing `DriverFailureKind::MalformedPayload`,
  - log-stat windows not guessing unavailable consumption fields,
  - dashboard windows not summing partial token rows,
  - negative token values keeping token totals unknown,
  - dashboard total backfill searching past empty recent windows.
- Removed the eight corresponding fixture-backed tests from `src-tauri/src/services/collectors/adapters/newapi/mod.rs`.

## Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml` - passed
- `cargo test --manifest-path src-tauri\Cargo.toml --lib newapi` - 68 passed, 1 ignored
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` - 83 passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries` - 4 passed
- `cargo check --manifest-path src-tauri\Cargo.toml` - passed with existing warnings

## Boundary Notes

- Remaining `collectors/adapters/newapi/**` coverage is mostly full balance-flow tests, the ignored live login test, and legacy sync auth/client helper behavior.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/audits/persistence-v2-boundary-manifest.json`

## Follow-Up

- Migrate the remaining NewAPI full balance-flow tests to driver-context `NewApiCollectorDriver.collect(... Balance)` coverage.
- Delete legacy sync auth/client helpers only after their useful behavior contracts have driver-side coverage or are proven obsolete.
- Task 25.A/25.B/25.D/25.E and the Stage 6 Gate are not claimed by this shard.
