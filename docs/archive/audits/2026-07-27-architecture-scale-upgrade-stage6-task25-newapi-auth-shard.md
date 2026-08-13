# Stage 6 Task 25.A/25.C Shard - NewAPI Auth Test Edge Cutover

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Cut the remaining `collectors/mod.rs` test-only NewAPI login edge from `adapters::newapi` to the NewAPI driver auth module.
- Keep behavior unchanged for saved-credential login probes and station login test fixtures.
- Do not modify Persistence V2 work.

## Changes

- Added NewAPI password-login probe helpers to `src-tauri/src/services/collectors/drivers/newapi/auth.rs`.
- Updated test-only login preparation in `src-tauri/src/services/collectors/mod.rs` to call `drivers::newapi::auth`.
- Extended `provider_conformance.rs` with the minimal model/port harness needed to compile the driver-local test helper.
- Added driver-local coverage for Set-Cookie normalization.

## RED Observations

- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` initially failed because the source-included driver auth module now compiled the test-only login helper, while the harness did not define the minimal `Station`, `PersistStationSessionInput`, or `CollectorSourcePort` shapes.
- The fix kept the edge in the driver auth module and added only the minimal harness definitions required for conformance compilation.

## Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml` - passed
- `cargo test --manifest-path src-tauri\Cargo.toml --lib newapi::auth` - 6 tests passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` - 55 tests passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries` - 4 tests passed
- `cargo check --manifest-path src-tauri\Cargo.toml` - passed with existing warnings plus request-recovery visibility warnings from prior Sub2API driver-localization
- `git diff --check` - passed

## Boundary Notes

- Source scan for `adapters::newapi`, `collectors::adapters::newapi`, and `crate::services::collectors::adapters::newapi` in the touched login/harness files is clear.
- The broader `collectors/adapters/newapi/**` test compatibility tree still exists and remains a Task 25 follow-up.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/audits/persistence-v2-boundary-manifest.json`
