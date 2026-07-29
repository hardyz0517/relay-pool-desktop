# Stage 6 Task 25.A/C Shard - NewAPI Adapter Island Closeout

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Delete the remaining test-only NewAPI adapter compatibility island after the useful parser, auth, remote-key, usage-helper, group/model, and balance-flow behavior checks were moved to driver-side coverage.
- Remove the collector module export for the now-empty adapter namespace.
- Do not modify Persistence V2 work.

## Changes

- Deleted `src-tauri/src/services/collectors/adapters/mod.rs`.
- Deleted the remaining NewAPI adapter files:
  - `src-tauri/src/services/collectors/adapters/newapi/mod.rs`,
  - `src-tauri/src/services/collectors/adapters/newapi/auth.rs`,
  - `src-tauri/src/services/collectors/adapters/newapi/client.rs`.
- Removed `pub mod adapters;` from `src-tauri/src/services/collectors/mod.rs`.
- Updated the `CollectorSourcePort` comment from legacy adapter wording to provider collection driver wording.

## Evidence

- Source scan for `collectors::adapters`, `adapters::newapi`, `mod adapters`, and `pub mod adapters` in `src-tauri/src` and `src-tauri/tests` is clear except for the unrelated `services::proxy::adapters` namespace.
- `cargo fmt --manifest-path src-tauri\Cargo.toml` - passed
- `cargo test --manifest-path src-tauri\Cargo.toml --lib newapi` - 62 passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` - 89 passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries` - 4 passed
- `cargo check --manifest-path src-tauri\Cargo.toml` - passed with existing warnings

## Boundary Notes

- The NewAPI collector, authorization, remote-key, parser, and fixture coverage now lives under `src-tauri/src/services/collectors/drivers/newapi/**`.
- The deleted adapter files were already outside the production module graph before this shard because `src-tauri/src/services/collectors/adapters/mod.rs` exposed NewAPI only behind `#[cfg(test)]`.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/superpowers/audits/persistence-v2-boundary-manifest.json`

## Follow-Up

- Continue Task 25.B/25.D/25.E: clear the remaining non-provider legacy paths, artifact policy items, and final graph/allowlist inventory.
- Task 25 and the Stage 6 Gate are not claimed by this shard.
