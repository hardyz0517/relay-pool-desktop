# Stage 6 Task 25.A Shard - Collector Output Type Closeout

Date: 2026-07-27

## Scope

- Continue Task 25.A provider physical closeout without claiming Task 25 or Stage 6 Gate completion.
- Move common collector output/task types out of `collectors/adapters/mod.rs`.
- Keep provider adapter modules focused on adapter/provider implementation instead of acting as a shared type bucket.

## Changes

- Added `src-tauri/src/services/collectors/output.rs`.
  - Owns `CollectorTask`, `AdapterOutput`, and `CreatedRemoteKey`.
- Updated `src-tauri/src/services/collectors/mod.rs`.
  - Publishes `output` as the collector output/task contract module.
  - Uses local `CollectorTask`/`AdapterOutput` imports instead of `adapters::*` public types.
- Updated station collection command/facade/runner/apply/remote-key/provider adapter callers.
  - Public task routing now references `collectors::output::CollectorTask`.
  - V2 apply and provider legacy fixtures import `output::{AdapterOutput, CollectorTask, CreatedRemoteKey}`.
- Updated `src-tauri/src/services/collectors/adapters/mod.rs`.
  - Removed shared type definitions from the adapter module.

## Boundary Notes

- `adapters/mod.rs` now only declares provider/legacy adapter implementation modules: test-only NewAPI, request recovery, and Sub2API.
- `rg` found no remaining `collectors::adapters::CollectorTask`, `adapters::CollectorTask`, `adapters::AdapterOutput`, or `adapters::CreatedRemoteKey` references after migration.
- `adapters::sub2api` and `adapters::request_recovery` are still production paths and remain open for later Task 25 provider closeout.
- No Stage 6 Gate is claimed. Task 25.B-E remain open.

## Evidence

- `cargo fmt` - passed
- `cargo check` - passed with existing dead-code warnings
- `cargo test --lib services::collectors::collector_apply` - 1 test passed
- `cargo test --lib services::station_collectors` - 6 tests passed
- `cargo test --lib services::collectors::adapters::newapi::tests::newapi_quota_converts_to_usd_units` - 1 test passed
- `cargo test --lib services::collectors::adapters::sub2api::tests::sub2api_usage_parses_remaining_from_nested_quota` - 1 test passed

## Verification Notes

- Initial parallel `cargo test --lib services::collectors::collector_apply` / `services::station_collectors` surfaced a test-only NewAPI import that production `cargo check` could not see. The import was fixed and the tests were rerun serially.
- `git status --short --branch` before writing this audit showed only collector/provider/command files plus the new output module; no Persistence V2 paths were touched in this shard.
