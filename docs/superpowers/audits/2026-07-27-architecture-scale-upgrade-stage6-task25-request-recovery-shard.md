# Stage 6 Task 25.A Shard - Sub2API Request Recovery Physical Closeout

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Move Sub2API request recovery support out of the legacy `adapters` module tree.
- Keep request recovery behavior and tests unchanged while narrowing the physical adapter surface.
- Do not modify Persistence V2 work.

## Changes

- Moved request recovery support:
  - from `src-tauri/src/services/collectors/adapters/request_recovery.rs`
  - to `src-tauri/src/services/collectors/drivers/sub2api/request_recovery.rs`
- Removed `request_recovery` from `collectors::adapters`.
- Added `request_recovery` as a Sub2API driver-local module.
- Updated the legacy Sub2API adapter import to use `drivers::sub2api::request_recovery`.

## Boundary Notes

- Source scan under `src-tauri/src` and `src-tauri/tests` is clear for:
  - `adapters::request_recovery`
  - `collectors::adapters::request_recovery`
- Historical plan/audit documents still mention the old path as prior-state evidence; this shard does not rewrite old audit history.
- `adapters::sub2api` remains a production/test compatibility surface and still needs later Task 25 physical decomposition.

## Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml` - passed
- `cargo check --manifest-path src-tauri\Cargo.toml` - passed with existing 12 warnings
- `cargo test --manifest-path src-tauri\Cargo.toml --lib request_recovery` - 16 tests passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` - 50 tests passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries` - 4 tests passed
- `git diff --check` - passed

## Verification Notes

- Earlier parallel Cargo checks timed out while competing for Cargo locks and are not counted as evidence; the same checks were rerun serially and passed.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/superpowers/audits/persistence-v2-boundary-manifest.json`
