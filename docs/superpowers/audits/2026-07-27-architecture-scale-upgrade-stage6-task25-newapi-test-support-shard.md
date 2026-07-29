# Stage 6 Task 25.C Shard - NewAPI Test Support Relocation

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Move the NewAPI HTTP fixture helper out of the legacy adapter tree and into the NewAPI driver test surface.
- Keep the remaining legacy adapter behavior tests compiling while they are migrated in later shards.
- Do not modify Persistence V2 work.

## Changes

- Moved `src-tauri/src/services/collectors/adapters/newapi/test_support.rs` to `src-tauri/src/services/collectors/drivers/newapi/test_support.rs`.
- Added a `#[cfg(test)]` driver-local `test_support` module declaration.
- Updated legacy NewAPI adapter tests and client tests to import the fixture helper from `drivers::newapi::test_support`.
- Widened the fixture server and JSON response helper visibility to `pub(crate)` for the transitional test-only call sites.

## Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml` - passed
- `cargo test --manifest-path src-tauri\Cargo.toml --lib newapi` - 68 passed, 1 ignored
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` - 67 passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries` - 4 passed
- `cargo check --manifest-path src-tauri\Cargo.toml` - passed with existing warnings, including request-recovery visibility warnings from prior Sub2API driver-localization

## Boundary Notes

- The moved helper remains test-only and does not add a production driver dependency.
- The broader `collectors/adapters/newapi/**` compatibility tree still exists and remains a Task 25 follow-up.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/superpowers/audits/persistence-v2-boundary-manifest.json`

## Follow-Up

- Use the driver-local fixture helper to migrate the remaining HTTP fixture-backed NewAPI adapter tests.
- Task 25.A/25.B/25.D/25.E and the Stage 6 Gate are not claimed by this shard.
