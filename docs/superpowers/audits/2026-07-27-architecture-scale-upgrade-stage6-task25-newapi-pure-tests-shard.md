# Stage 6 Task 25.C Shard - NewAPI Pure Driver Test Replacement

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Move a first slice of NewAPI pure mapping, dashboard total, and usage merge behavior checks onto the NewAPI driver module.
- Preserve the existing adapter-side compatibility tests until the remaining behavior coverage is migrated.
- Do not modify Persistence V2 work.

## Changes

- Added driver-local tests in `src-tauri/src/services/collectors/drivers/newapi/mod.rs` for:
  - standard dashboard usage array shape,
  - exact dashboard total target matching,
  - missing metric propagation during dashboard total merge,
  - usage stat merge behavior that removes unverified self-usage fields,
  - empty optional usage merge cleanup,
  - integer metric parsing that rejects fractional values.

## Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml -- --check` - passed
- `cargo test --manifest-path src-tauri\Cargo.toml --lib newapi` - 74 passed, 1 ignored
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` - 61 passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries` - 4 passed
- `cargo check --manifest-path src-tauri\Cargo.toml` - passed with existing warnings, including request-recovery visibility warnings from prior Sub2API driver-localization
- `git diff --check` - passed

## Boundary Notes

- The broader `collectors/adapters/newapi/**` test compatibility tree still exists and remains a Task 25 follow-up.
- This shard only adds replacement tests; it does not delete the legacy NewAPI adapter path.
- Lightweight protected-path check found no modified Persistence V2 paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/superpowers/audits/persistence-v2-boundary-manifest.json`

## Follow-Up

- Continue migrating NewAPI adapter behavior tests in smaller slices before deleting `collectors/adapters/newapi/**`.
- Task 25.A/25.B/25.D/25.E and the Stage 6 Gate are not claimed by this shard.
