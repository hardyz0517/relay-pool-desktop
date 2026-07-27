# Stage 6 Task 25.C Shard - NewAPI Groups and Models Driver Tests

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Move NewAPI quota, groups, models, and empty-group behavior checks onto the NewAPI driver/parser side.
- Remove the corresponding legacy adapter-side tests once driver/parser coverage is green.
- Do not modify Persistence V2 work.

## Changes

- Added driver parser coverage for:
  - NewAPI quota conversion to USD units,
  - group/rate map parsing for default and VIP groups.
- Added driver-context async coverage for:
  - empty successful groups payload returning `DriverOutputStatus::Partial`,
  - top-level NewAPI models payload producing model facts.
- Removed the matching legacy adapter-side tests from `src-tauri/src/services/collectors/adapters/newapi/mod.rs`.

## Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml` - passed
- `cargo test --manifest-path src-tauri\Cargo.toml --lib newapi` - 68 passed, 1 ignored
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` - 75 passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries` - 4 passed
- `cargo check --manifest-path src-tauri\Cargo.toml` - passed with existing warnings, including request-recovery visibility warnings from prior Sub2API driver-localization

## Boundary Notes

- Remaining `collectors/adapters/newapi/**` coverage is mostly balance, dashboard, log, live login, and legacy sync auth/client helper behavior.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/superpowers/audits/persistence-v2-boundary-manifest.json`

## Follow-Up

- Migrate the remaining NewAPI balance/dashboard/log fixture tests to driver-context tests.
- Delete legacy sync auth/client helpers after their useful behavior contracts have driver-side coverage or are proven obsolete.
- Task 25.A/25.B/25.D/25.E and the Stage 6 Gate are not claimed by this shard.
