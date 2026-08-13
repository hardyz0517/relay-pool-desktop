# Stage 6 Task 25.A Shard - NewAPI Auth Physical Closeout

Date: 2026-07-27

## Scope

- Continue Task 25.A provider physical closeout without claiming Task 25 or Stage 6 Gate completion.
- Move NewAPI collection auth-context preparation out of the legacy adapter production surface.
- Keep the legacy NewAPI adapter available only for unit-test parser/client fixtures while production code uses the NewAPI driver module.

## Changes

- Added `src-tauri/src/services/collectors/drivers/newapi/auth.rs`.
  - Owns `PreparedNewApiAuthKind`, `PreparedNewApiAuthContext`, `NewApiResolvedSession`, `NewApiAuthSessionSource`, and the access-token/cookie selection rule.
  - Avoids app-model dependencies so `provider_conformance` can compile provider drivers through its standalone harness.
- Updated `src-tauri/src/services/collectors/mod.rs`.
  - Adapts `CollectorSourcePort` to `drivers::newapi::auth::NewApiAuthSessionSource`.
  - Calls `drivers::newapi::auth::prepare_collector_auth_context` for production NewAPI collection prep.
- Updated `src-tauri/src/services/collectors/adapters/mod.rs`.
  - Makes `adapters::newapi` `#[cfg(test)]`, removing the legacy NewAPI adapter module from the production module graph.
- Updated `src-tauri/src/services/collectors/adapters/newapi/mod.rs`.
  - Deleted the production `prepare_collector_auth_context` compatibility facade and its prepared auth types.
  - Deleted unused NewAPI `remote_key_capability`; current remote-key capability handling is driver/service-owned.

## Boundary Notes

- Production NewAPI collection auth no longer depends on `collectors::adapters::newapi`.
- Remaining `adapters::newapi` references are in test-only collector login fixtures or inside the legacy adapter test module itself.
- `adapters::sub2api` and `adapters::request_recovery` remain in production use and are not part of this shard.
- No Stage 6 Gate is claimed. Task 25.B-E and the remaining provider closeout work are still open.

## Evidence

- `cargo fmt` - passed
- `cargo check` - passed with existing dead-code warnings
- `cargo test --lib services::collectors::drivers::newapi::auth` - 3 tests passed
- `cargo test --lib services::collectors::drivers::newapi` - 19 tests passed
- `cargo test --lib services::collectors::adapters::newapi::tests::newapi_quota_converts_to_usd_units` - 1 test passed
- `cargo test --test provider_conformance` - 34 tests passed

## Verification Notes

- `cargo test services::collectors::drivers::newapi` was attempted before narrowing to `--lib`, but timed out while Cargo enumerated/build-locked multiple test binaries. It is not counted as passing evidence.
- `git diff --name-only` before writing this audit showed only collector/provider files; no Persistence V2 paths were touched in this shard.
