# Stage 6 Task 25.B/E Shard - Provider Dispatcher Legacy Deletion

Date: 2026-07-27

## Scope

- Continue Task 25 without claiming Task 25 or Stage 6 Gate completion.
- Remove the remaining collector route `Legacy` variants and their dead synchronous fallback prepare path.
- Replace station collection route dispatch from adapter-name string `if` chains with closed `ProviderKind` enum matching.
- Keep Sub2API, NewAPI, and OpenAI-compatible driver preparation behavior unchanged.

## Changes

- Updated `prepare_station_collection_route_v2` and `prepare_station_task_route_v2`.
  - Station type is now normalized through `provider_kind_for_station_type`.
  - Provider dispatch is a closed `ProviderKind` match.
  - Unsupported station types still fail with `ApplicationError::ConstraintViolation`.
- Deleted unused route variants.
  - `PreparedStationCollectionRoute::Legacy`
  - `PreparedStationTaskRoute::Legacy`
- Deleted unused legacy synchronous fallback helpers.
  - `prepare_station_task_v2`
  - `prepare_station_collection_v2`
  - `dispatch_adapter_output`
  - `failed_adapter_output`
- Updated the command facade and scheduled station collector match arms to handle only provider driver routes.
- Changed `full_child_tasks` to accept `ProviderKind` instead of adapter strings.

## Boundary Notes

- Literal scan found no remaining references to the deleted legacy route/prepare/dispatch names in `src-tauri/src` or `src-tauri/tests`.
- The existing `custom` station type compatibility remains mapped to `ProviderKind::OpenAiCompatible`; `ProviderKind::parse` remains closed and does not map custom directly.
- Task 25 remains open for the larger physical provider-module decomposition and request-recovery adapter cleanup.

## Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml` - passed
- `cargo check --manifest-path src-tauri\Cargo.toml` - passed with existing 12 warnings
- `cargo test --manifest-path src-tauri\Cargo.toml --test provider_conformance` - 34 tests passed
- `cargo test --manifest-path src-tauri\Cargo.toml --lib station_collector` - 10 tests passed
- `cargo test --manifest-path src-tauri\Cargo.toml --lib full_tasks_are_bounded_by_provider_capability` - 1 test passed
- `cargo test --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries` - 4 tests passed
- `git diff --check` - passed

## Verification Notes

- An earlier parallel provider-conformance run timed out while Cargo locks were still active and is not counted as evidence; provider conformance was rerun serially and passed.
- This shard did not touch Persistence V2 protected paths:
  - `src-tauri/src/persistence`
  - `src-tauri/migrations`
  - `docs/audits/persistence-v2-boundary-manifest.json`
