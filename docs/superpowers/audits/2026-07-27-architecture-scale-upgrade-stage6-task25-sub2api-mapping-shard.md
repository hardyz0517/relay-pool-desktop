# Stage 6 Task 25.A/25.C Shard - Sub2API Mapping Adapter Deletion

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Delete the old `collectors::adapters::sub2api` module path.
- Keep the Sub2API driver production behavior routed through driver-local mapping/parser code.
- Replace the useful parser behavior coverage that was previously attached to the old adapter file.
- Do not modify Persistence V2 work.

## Changes

- Moved the retained Sub2API parser/mapping surface into `src-tauri/src/services/collectors/drivers/sub2api/mapping.rs`.
- Deleted `src-tauri/src/services/collectors/adapters/sub2api.rs` and removed its module export.
- Updated `drivers/sub2api/mod.rs` to call driver-local `mapping::*` functions instead of `adapters::sub2api::*`.
- Kept Sub2API remote-key capability construction in `remote_keys.rs` while removing the adapter dependency.
- Removed the obsolete `collectors::adapters::sub2api` fixture from `provider_conformance.rs`.
- Added driver-local mapping tests for:
  - nested remote-key payload parsing and secret masking,
  - masked full-key rejection,
  - group/rate fact parsing,
  - account profile and dashboard stats merge behavior.

## RED Observations

- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` initially failed because the source-included conformance harness still expected a `collectors::adapters::sub2api` fixture after the production driver started compiling `drivers::sub2api::mapping`.
- `cargo test --manifest-path src-tauri\Cargo.toml --lib sub2api::mapping` initially failed because new test fixtures asserted exact mask text and rate/profile shapes that did not match the existing parser contract.
- A later `provider_conformance` run failed because the conformance harness uses a local `mask_secret` stub; the test now asserts the stable security contract instead of exact mask formatting.

## Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml` - passed
- `cargo test --manifest-path src-tauri\Cargo.toml --lib sub2api::mapping` - 4 tests passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` - 54 tests passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries` - 4 tests passed
- `cargo check --manifest-path src-tauri\Cargo.toml` - passed with existing warnings plus request-recovery visibility warnings caused by the narrower driver-local compile surface
- `git diff --check` - passed

## Boundary Notes

- Source scan under `src-tauri/src` and `src-tauri/tests` is clear for:
  - `adapters::sub2api`
  - `collectors::adapters::sub2api`
  - `crate::services::collectors::adapters::sub2api`
- Remaining `sub2api` module declarations are the normal login support module and the provider driver module, not the deleted adapter path.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/superpowers/audits/persistence-v2-boundary-manifest.json`

## Follow-Up

- Task 25.A still needs additional provider physical closeout, especially the remaining NewAPI adapter/test compatibility surface.
- Task 25.E final graph and allowlist cleanup are not claimed by this shard.
