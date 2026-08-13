# Stage 6 Task 25.C Shard - NewAPI Remote-Key Driver Tests

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Move the NewAPI remote-key pagination, malformed pagination, partial-page failure, and create/reconcile/reveal behavior checks onto the NewAPI driver.
- Remove the corresponding legacy adapter-side remote-key tests after driver-side coverage is green.
- Do not modify Persistence V2 work.

## Changes

- Added driver-context async tests in `src-tauri/src/services/collectors/drivers/newapi/mod.rs` for:
  - paginating NewAPI token pages without fingerprinting masked keys,
  - rejecting missing pagination metadata,
  - failing before returning partial remote-key pages,
  - posting token creation, reconciling by name, and revealing the full key once.
- Removed the four corresponding fixture-backed tests from `src-tauri/src/services/collectors/adapters/newapi/mod.rs`.
- Extended `src-tauri/tests/provider_conformance.rs` outbound harness with a minimal local HTTP fixture client and redacted header storage so source-included driver HTTP tests execute instead of returning `RequestFailed`.

## RED Observations

- `cargo test --manifest-path src-tauri\Cargo.toml --lib newapi` initially failed because the new driver assertions assumed canonical header casing and the old sync client's transient retry count.
- The assertions now match the async driver contract: header checks are case-insensitive and the partial-page failure test verifies failure before partial return rather than old retry count.
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` initially failed because its stub `AsyncOutboundClient` always returned `RequestFailed`.
- The conformance harness now supports local HTTP fixture execution while keeping sensitive header debug output redacted.

## Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml` - passed
- `cargo test --manifest-path src-tauri\Cargo.toml --lib newapi` - 68 passed, 1 ignored
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` - 71 passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries` - 4 passed
- `cargo check --manifest-path src-tauri\Cargo.toml` - passed with existing warnings, including request-recovery visibility warnings from prior Sub2API driver-localization

## Boundary Notes

- Remaining `collectors/adapters/newapi/**` coverage is now mostly balance, dashboard, log, model/group output, live login, and legacy sync auth/client helpers.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/audits/persistence-v2-boundary-manifest.json`

## Follow-Up

- Migrate the remaining NewAPI balance/dashboard/log fixture tests to driver-context tests.
- Delete the legacy adapter auth/client helpers only after their useful behavior contracts have driver-side coverage or are proven obsolete.
- Task 25.A/25.B/25.D/25.E and the Stage 6 Gate are not claimed by this shard.
