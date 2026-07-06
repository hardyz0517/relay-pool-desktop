# Relay Pool Shared Capabilities Refactor Design

Date: 2026-07-06

## Purpose

Relay Pool Desktop currently has several user operations implemented more than once across pages. The most important examples are station key save flows, station group option handling, and channel monitor run loading. This creates maintenance risk because a bug fix or behavior change can land in one page while another page keeps the old workflow.

The refactor should make mature engineering boundaries explicit:

- Rust services own business workflows and persisted facts.
- Tauri commands expose coarse-grained capabilities for complete user operations.
- TypeScript API modules are thin clients with preview fallback where needed.
- React pages express UI intent and do not assemble multi-step business flows.

The work must be comprehensive in direction but incremental in execution. Each stage should be small enough to verify and revert independently.

## Current Problems

### Station Key Save Flow Duplication

`AddKeyPage`, `EditKeyPage`, and `KeyPoolPage` each compose station key save behavior from lower-level calls such as create/update key, update group binding, and update default routing capabilities. This causes two risks:

- Clearing an existing group binding can be skipped by page-local conditional logic.
- Default capability flags are repeated in multiple pages and can drift.

### Station Group Option Duplication

Group option logic is spread across `CreateRemoteKeyDialog`, `StationKeyRowsEditor`, `AddProviderPage`, and key-pool forms. Components currently duplicate matching, dedupe, option value, and multiplier formatting rules. Some selectors use index values while others use compound strings. This is fragile for synced remote groups because remote key creation must preserve the binding identity that resolves to the upstream group id.

### Channel Monitor Run Loading Duplication

`ChannelMonitoringTab` and `ChannelStatusTab` both load monitors and then fetch runs per monitor. Their failure semantics differ: one keeps per-monitor failure state, while the other silently falls back to empty runs. This should be a shared capability.

### Low-Risk Utility Duplication

Many pages define local `readError` helpers and multiplier/rate formatters. These are not the core architecture problem, but they should be cleaned up after the business capabilities are centralized.

## Target Architecture

### Backend Services

Rust services should provide complete operations for the workflows that must stay consistent:

- Save a station key with default routing capability and group binding handling.
- List normalized station group options for UI selection and remote key creation.
- List channel monitor summaries with recent run data and per-monitor load status.

The service layer should contain the business rules. Tauri command functions should remain thin wrappers around those services.

### TypeScript API Layer

`src/lib/api/*` modules should expose the same coarse-grained capabilities to React:

- `saveStationKeyWithDefaults(input)`
- `listStationGroupOptions(stationId)`
- `listChannelMonitorSummaries()`

The API layer may keep browser-preview fallback behavior, but it should not become a second business-rule implementation.

### React Pages

Pages should submit intent and render results:

- Key pages submit a station key form to one save API.
- Group selection components consume normalized group options.
- Channel tabs consume monitor summaries.

Pages should no longer coordinate multi-step persistence flows for these operations.

## Proposed Capabilities

### `save_station_key_with_defaults`

Add a Rust service and command that handles both create and update.

Input shape:

- `mode`: `create` or `update`
- `id`: required for update
- `station_id`
- key metadata: name, api key, enabled, priority, status, tier label, note
- group selection action:
  - `keep`: keep the existing group binding and group-derived fields, only valid for update
  - `clear`: remove the existing group binding, group id hash, group name, multiplier, and group-derived rate source
  - `set`: apply the selected `group_binding_id`; `group_id_hash` and `group_name` may be included only as display or fallback context
- station-key metadata already supported by the existing station key model, including `balance_scope`
- nested `capabilities` object is accepted only when the caller is intentionally editing routing capability fields; routine create/edit flows should omit it and let the backend apply defaults

Rules:

- Create and update use the same public command.
- Group binding changes must use the explicit group selection action. Do not rely on plain `Option<String>` null handling, because Rust/Tauri deserialization cannot distinguish omitted fields from `null` without an explicit action or patch type.
- Default routing capability is written in one backend place.
- Routine key create/edit flows write default routing capability from one backend helper. The defaults match the current product decision: all supported flags default to true, model allow/block/preferred lists default to empty, `only_use_as_backup` defaults to false, and routing tags default to empty.
- Advanced routing edits remain a separate capability concern. If a future caller passes the nested `capabilities` object, the backend should update `station_key_capabilities` explicitly; otherwise it should preserve existing capabilities on update and create defaults on create.
- If base key save succeeds but follow-up persistence fails, return a clear error that names the failed part.
- Existing lower-level commands remain available during migration. After all named consumers migrate, each lower-level command must be classified as either still-public API or internal-only support API before any removal.

Initial consumers:

- `AddKeyPage`
- `EditKeyPage`
- `KeyPoolPage`

Follow-up consumers:

- Station dialog flows in `StationsPage`
- Key save paths inside `AddProviderPage` if they represent the same user operation

### `list_station_group_options`

Add a Rust service and command that returns normalized group options for a station.

Output shape:

- `group_binding_id`
- `group_id_hash`
- `group_name`
- `rate_multiplier`
- `rate_source`
- `selectable_for_remote_key`

Rules:

- Build options from saved `station_group_bindings` and latest relevant `group_rate_records`.
- Dedupe and match by this priority: `group_binding_id`, then `group_id_hash`, then normalized `group_name`.
- Prefer saved binding identity over scan-only display data.
- Remote-key creation should prefer `group_binding_id` so the backend can resolve the real upstream group id when available.
- Frontend option values must be stable and not index based.

Initial consumers:

- `CreateRemoteKeyDialog`
- `StationKeyRowsEditor`
- `AddKeyPage`
- `EditKeyPage`
- `KeyPoolPage`

Follow-up consumers:

- `AddProviderPage`
- Station detail group/key editors

### `list_channel_monitor_summaries`

Add a Rust service and command that returns channel monitor data ready for both channel pages.

Output shape:

- `monitor`
- `recent_runs`
- `runs_load_status`: `ok` or `failed`
- `latest_run`: latest run when available, otherwise `null`

Rules:

- One backend capability owns how many recent runs are loaded.
- Per-monitor run load failure should be represented explicitly.
- A single monitor run-load failure should not blank the whole page.
- `ChannelMonitoringTab` and `ChannelStatusTab` consume the same summary API.

Initial consumers:

- `ChannelMonitoringTab`
- `ChannelStatusTab`

### Shared Frontend Utilities

After the business capabilities are migrated, add small shared utilities:

- `src/lib/errors.ts`: `readError(error: unknown): string`
- `src/lib/formatters.ts`: rate and multiplier formatting

These utilities should remain simple formatting/parsing helpers. They should not absorb business workflows.

## Incremental Delivery Plan

### Stage 0: Baseline Behavior Tests

Before changing behavior, add or tighten focused tests that describe the current desired behavior:

- Creating a key applies default capabilities.
- Editing a key preserves or changes group identity correctly.
- Remote key creation preserves group binding identity.
- Channel monitor run load failure is visible and consistent.

This stage should not change production behavior. If clearing an existing group binding is not currently supported, Stage 0 should document that as the known defect and add a pending or expected-failure test case. Stage 1 turns that case green through the new explicit group selection action.

### Stage 1: Backend Station Key Save Capability

Add `save_station_key_with_defaults` in Rust and expose it through Tauri. Add service tests for create, update, clear binding, and default capability. Add the TypeScript API wrapper without migrating all pages at once.

### Stage 2: Migrate Key Save Consumers

Migrate `AddKeyPage`, `EditKeyPage`, and `KeyPoolPage` one at a time. After each page migration, run targeted tests. Once all three are migrated, add a negative-proof test that these pages no longer call the old multi-step save sequence directly.

### Stage 3: Backend Station Group Options

Add `list_station_group_options` and tests for dedupe, match priority, latest multiplier selection, and remote-key selectability. Keep the existing lower-level group APIs during migration.

### Stage 4: Migrate Group Option Consumers

Migrate group option consumers in small batches:

1. `CreateRemoteKeyDialog`
2. `StationKeyRowsEditor`
3. key-pool add/edit forms
4. `AddProviderPage` and station editor flows

Each batch should verify that remote group identity is preserved.

### Stage 5: Backend Channel Monitor Summaries

Add `list_channel_monitor_summaries` and service tests for normal runs and per-monitor run load failure.

### Stage 6: Migrate Channel Tabs

Migrate `ChannelMonitoringTab` and `ChannelStatusTab` to the summary API. Add a negative-proof test that both tabs no longer issue page-local per-monitor run loading.

### Stage 7: Low-Risk Utility Cleanup

Replace duplicated `readError` and multiplier/rate formatters with shared utilities. Do this only after the main business workflows are stable.

## Verification

Each implementation stage should run the smallest relevant checks first, then broader checks as needed.

Required checks by stage:

- Frontend-only page migrations: targeted script tests and `pnpm.cmd build`.
- Rust service or command changes: targeted Rust tests where available and `cargo check --manifest-path .\src-tauri\Cargo.toml`.
- Shared business behavior changes: both targeted script tests and Rust checks.

Important negative-proof checks:

- Key pages no longer call `updateStationKeyCapabilities` directly for default capability persistence.
- Key pages no longer compose save plus group-binding update as a page-local workflow.
- Channel tabs no longer call `listChannelMonitorRuns(monitor.id)` in page-local loops.
- Group option components no longer define unstable index-based option values.

## Risk Controls

- Keep old lower-level commands while migrating consumers.
- Migrate one consumer group at a time.
- Use exact-path staging only.
- Do not mix unrelated dirty worktree changes into these commits.
- Treat existing icons, output files, and unrelated modified files as out of scope.
- If a stage becomes too large, split it into backend command, TS API wrapper, page migration, and test commits.

## Non-Goals

- Do not redesign the UI layout.
- Do not add new account, payment, cloud sync, marketplace, or team features.
- Do not replace the collector or router architecture.
- Do not remove existing lower-level APIs until all consumers have been migrated and verified.
- Do not change remote Sub2API semantics beyond preserving the correct group identity.

## Acceptance Criteria

The refactor is complete when:

- Saving a Station Key has one primary public capability.
- Listing selectable station groups has one primary public capability.
- Loading channel monitor run summaries has one primary public capability.
- The pages named in this spec no longer duplicate business workflow orchestration.
- Existing remote group identity behavior remains intact.
- Clearing a station key group binding is supported and tested.
- Relevant frontend and Rust checks pass.
- Work is delivered in small, reviewable commits with explicit staged paths.
