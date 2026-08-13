# Stage 6 Task 25.C Shard - NewAPI Parser Contract Cutover

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Move another pure NewAPI parser contract slice onto the NewAPI driver.
- Remove adapter-side duplicate pure tests once driver-side coverage exists.
- Do not modify Persistence V2 work.

## Changes

- Added driver-local tests in `src-tauri/src/services/collectors/drivers/newapi/mod.rs` for:
  - rejecting non-standard token wrappers and alias fields,
  - rejecting failed NewAPI envelope payloads before status parsing.
- Removed adapter-side duplicates now covered by driver-local tests:
  - dashboard usage item shape,
  - dashboard total exact target match,
  - dashboard total missing metric propagation,
  - usage merge cleanup,
  - empty usage merge cleanup,
  - integer metric fractional rejection,
  - token parser wrapper/alias rejection,
  - failed status envelope rejection.

## RED Observations

- `cargo test --manifest-path src-tauri\Cargo.toml --lib newapi` initially failed because the migrated status test referenced the old adapter wrapper `parse_status_payload`.
- The driver separates envelope validation and status parsing, so the test now asserts the same failed-envelope contract through `parsers::envelope_data`.

## Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml` - passed
- `cargo test --manifest-path src-tauri\Cargo.toml --lib newapi` - 68 passed, 1 ignored
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` - 63 passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries` - 4 passed
- `cargo check --manifest-path src-tauri\Cargo.toml` - passed with existing warnings, including request-recovery visibility warnings from prior Sub2API driver-localization

## Boundary Notes

- The broader `collectors/adapters/newapi/**` compatibility tree still exists and remains a Task 25 follow-up.
- HTTP fixture-backed NewAPI balance, dashboard, log, pagination, create, and reveal tests remain in the adapter island until a driver-context harness replaces them.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/audits/persistence-v2-boundary-manifest.json`

## Follow-Up

- Continue migrating fixture-backed NewAPI adapter tests in smaller slices.
- Task 25.A/25.B/25.D/25.E and the Stage 6 Gate are not claimed by this shard.
