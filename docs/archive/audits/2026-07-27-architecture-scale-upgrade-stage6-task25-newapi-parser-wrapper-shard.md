# Stage 6 Task 25.C Shard - NewAPI Parser Wrapper Deletion

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Delete the legacy NewAPI adapter parser wrapper after the parser implementation already lives in the driver module.
- Keep remaining legacy adapter behavior tests compiling while they are migrated in later shards.
- Do not modify Persistence V2 work.

## Changes

- Deleted `src-tauri/src/services/collectors/adapters/newapi/parsers.rs`.
- Updated the remaining legacy NewAPI adapter auth, client, and behavior tests to import `drivers::newapi::parsers` directly.
- Kept parser behavior coverage in `src-tauri/src/services/collectors/drivers/newapi/parsers.rs` and driver-local tests.

## Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml` - passed
- `cargo test --manifest-path src-tauri\Cargo.toml --lib newapi` - 68 passed, 1 ignored
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` - 67 passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries` - 4 passed
- `cargo check --manifest-path src-tauri\Cargo.toml` - passed with existing warnings, including request-recovery visibility warnings from prior Sub2API driver-localization

## Boundary Notes

- Remaining `collectors/adapters/newapi/**` files are now the legacy sync auth/client helpers and the fixture-backed behavior tests.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/audits/persistence-v2-boundary-manifest.json`

## Follow-Up

- Migrate the remaining NewAPI fixture-backed behavior tests to driver-context tests.
- Remove legacy sync auth/client helpers only after their useful behavior contracts have driver-side coverage.
- Task 25.A/25.B/25.D/25.E and the Stage 6 Gate are not claimed by this shard.
