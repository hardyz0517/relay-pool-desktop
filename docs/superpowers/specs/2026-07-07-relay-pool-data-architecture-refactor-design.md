# Relay Pool Data Architecture Refactor Design

Date: 2026-07-07

## Purpose

Relay Pool Desktop is moving from a prototype-style local desktop tool toward a mature local AI gateway control plane. The current codebase already has many good building blocks: stations, station keys, group bindings, group rate history, balance snapshots, pricing rules, change events, collector runs, request logs, route candidates, and local proxy runtime state. The problem is that several pages still assemble these facts independently, and some fields act as both current state, historical evidence, and display cache.

This design defines a non-breaking refactor that gives every fact a clear owner, introduces shared query and projection interfaces, removes repeated page-local joins, and protects existing user-visible behavior while the app matures.

The first rule is safety: do not delete fields or merge similar fields until their semantics, writers, readers, migration path, and regression coverage are explicit.

## Reference Projects And Lessons

These projects are not templates to copy. They are reference points for mature boundaries.

- LiteLLM Proxy: virtual keys govern model access and spend, while routing/load balancing happens through model groups and deployments. Lesson: local entry keys, upstream provider secrets, model access, and spend facts should not be one object.
  - https://docs.litellm.ai/docs/proxy/virtual_keys
  - https://docs.litellm.ai/docs/proxy/load_balancing
- Portkey AI Gateway: gateway configs are composable JSON policies for routing, fallback, retries, caching, and load balancing. Lesson: runtime routing policy should be compiled from control-plane facts instead of recreated in each UI page.
  - https://portkey.ai/docs/product/ai-gateway/configs
  - https://portkey.ai/docs/product/ai-gateway/fallbacks
- Envoy AI Gateway: separates control plane and data plane. Lesson: Relay Pool's Tauri/React management surface should be treated as control plane, and the local OpenAI-compatible proxy should consume a stable runtime snapshot.
  - https://aigateway.envoyproxy.io/docs/concepts/architecture/
- Apache APISIX: supports control-plane/data-plane and standalone file/API-driven modes with hot configuration updates and version/digest checks. Lesson: local runtime config should be versioned and atomically replaceable.
  - https://apisix.apache.org/docs/apisix/deployment-modes/
- Kong Gateway: separates route, service, upstream, consumer, and plugin entities. Lesson: routing target, upstream station/key, caller identity, and policy should remain separate concepts.
  - https://developer.konghq.com/gateway/entities/route/
- TensorZero: combines gateway, observability, evaluation, optimization, and experimentation as related but separate subsystems. Lesson: request logs and collector evidence should feed analysis without becoming current UI state directly.
  - https://github.com/tensorzero/tensorzero
- Bifrost AI Gateway: emphasizes virtual keys, weighted key distribution, model filtering, load balancing, and failover. Lesson: key governance and upstream key inventory need explicit boundaries.
  - https://docs.getbifrost.ai/overview
- Open WebUI extensibility: separates extension mechanisms from the core UI model. Lesson: future provider-specific behaviors should plug into adapter/projection boundaries instead of branching through pages.
  - https://docs.openwebui.com/features/extensibility/

## Current Audit Summary

### Duplicated Data Loading

Multiple pages independently compose similar calls:

- `PricingPage`: stations, station keys, station group bindings, group rate records, pricing rules.
- `StationDetailPage`: stations, credentials, station keys, group bindings, rate records, collector runs, latest snapshot, balance snapshots, change events.
- `StationsPage`: station list plus enrichment calls for balances, changes, snapshots, keys, group facts, collector runs.
- `AddProviderPage`: stations, credentials, keys, group bindings, rate records, remote key capability, remote keys.
- `KeyPoolPage`: stations, key pool items, monitors, templates, group options.

This causes loading behavior, partial failure behavior, stale-data behavior, and timeout behavior to drift.

### Duplicated Group And Rate Projection

Group/rate logic is repeated in several shapes:

- `AddProviderPage` converts bindings and latest rates into drafts, dedupes group rows, and merges saved options.
- `stationDetailViewModels.ts` dedupes station group bindings by group name and chooses a preferred binding.
- `pricingComparisonViewModel.ts` builds pricing candidates from bindings and standalone rate records.
- `shared_capabilities.rs` already has `station_group_options_from_facts`, but only group-option consumers use it.

The durable direction exists, but it is too narrow. The app needs one current-fact projection layer, not several page-specific approximations.

### Duplicated Utility Code

There are already shared helpers such as `src/lib/errors.ts` and `src/lib/formatters.ts`, but pages still define local `readError`, `formatRate`, `formatMultiplier`, `formatTime`, `toTime`, and status label/tone helpers. This is low-level duplication, but it creates polish drift and makes refactors harder.

### Repeated Preview Fallback Semantics

Many `src/lib/api/*.ts` files have local `isInvokeUnavailable` and memory fallback behavior. Browser preview fallback is useful, but it should not become a second business implementation. It should be centralized and explicitly marked as UI-preview behavior.

### Field Ambiguity

Several fields look similar but are not equivalent:

- `group_binding_id`: durable local row identity.
- `group_key_hash`: local stable identity used for dedupe and binding lookup.
- `group_id_hash`: redacted upstream group id identity when available.
- `group_name`: display name and sometimes legacy matching fallback.
- `rate_multiplier` on station keys: compatibility/current display cache, not the authoritative multiplier source.
- `effective_rate_multiplier` on group bindings: current projection of the best-known multiplier.
- `group_rate_records`: historical evidence and change detection, not the only current state.

No refactor may collapse these fields without a written migration and tests proving the distinction is preserved where needed.

## Target Architecture

Relay Pool should be modeled as a local AI gateway with these layers.

### Control Plane

The control plane is the Tauri app, database, collectors, settings, and React UI. It owns configuration, credentials, station assets, collected facts, normalized projections, and user workflows.

### Data Plane

The data plane is the local OpenAI-compatible proxy. It should not reconstruct UI joins or query many tables at request time. It should consume a compiled runtime snapshot and emit request logs/health facts.

### Canonical Facts

Canonical facts are persisted business objects that own the core identity and relationships:

- `stations`
- `station_keys`
- `station_group_bindings`
- `pricing_rules`
- `station_key_capabilities`
- `routing policies and aliases`
- `secrets`
- `settings`

Canonical facts may reference evidence but should not depend on page-local joins.

### Evidence And History

Evidence/history tables preserve observations:

- `group_rate_records`
- `balance_snapshots`
- `collector_runs`
- `collector_snapshots`
- `station_key_health`
- `station_endpoint_health`
- `request_logs`
- `change_events`
- `remote_station_keys`

Evidence is read by projection services. Pages should not parse raw evidence as the primary source of current state unless no projection exists yet.

### Current Projections

Current projections are derived read models. They answer "what should the UI or runtime treat as current right now?"

Required projections:

- `StationCurrentSummary`
- `StationGroupCurrentFact`
- `StationKeyCurrentFact`
- `StationBalanceCurrentFact`
- `PricingComparisonCandidate`
- `RouteCandidate`
- `RuntimeRouteSnapshot`

Projection functions should be pure where possible and tested independently. Tauri commands can expose coarse-grained projection bundles once the shape is stable.

### Compatibility Fields

Compatibility fields stay in the schema until all consumers have migrated and old database states are safe:

- `station_keys.group_name`
- `station_keys.group_id_hash`
- `station_keys.rate_multiplier`
- `station_keys.rate_source`
- `station_keys.rate_collected_at`
- `stations.balance_raw`
- `stations.balance_cny`
- `stations.last_pricing_fetched_at`

During this design, these fields are not deleted. They are classified as compatibility/cache fields with explicit writer rules and read restrictions.

## Field Ownership Decisions

### Station

`stations` owns station identity, upstream endpoint, account-level settings, enabled state, priority, recharge ratio (`credit_per_cny`), and coarse status.

`stations.balance_raw` and `stations.balance_cny` remain compatibility fields. New current balance UI should prefer `StationBalanceCurrentFact`, which is derived from latest station-scope `balance_snapshots`, with station fields as fallback only.

`stations.last_pricing_fetched_at` remains a coarse collection timestamp. It must not be used as the only source for group freshness once group/rate projections exist.

### Station Key

`station_keys` owns routeable local key identity, parent station, enabled state, priority, status, routing capability reference, and optional group binding reference.

`station_keys.group_binding_id` is the primary current group selection reference for the key.

`station_keys.group_name`, `group_id_hash`, `rate_multiplier`, `rate_source`, and `rate_collected_at` are compatibility/display caches. New features should read these through `StationKeyCurrentFact`, not directly, unless editing the compatibility fields themselves.

### Station Group Binding

`station_group_bindings` owns current local group identity and binding status.

For `binding_kind = station_group`, it represents a currently known station-level group.

For `binding_kind = key_binding`, it represents a key-to-group binding or legacy/manual relationship.

The binding row may carry current multiplier fields because it is the current projection anchor. These fields should be updated by collector/application services, not by page-local logic.

### Group Rate Record

`group_rate_records` owns historical multiplier observations. It should support audit, change detection, and projection refresh, but UI pages should not independently choose "latest" records. Shared projection code should do that.

### Pricing Rule

`pricing_rules` owns normalized model pricing facts and manual/collected pricing overrides. It does not own group membership. Pricing comparison should join pricing rules with group projections through a shared pricing candidate builder.

### Balance Snapshot

`balance_snapshots` owns balance evidence. Current balance is the latest relevant projection by station/key/scope, not a field copied independently into every page.

### Change Event

`change_events` owns user-facing event history and unread state. It should not be the source of object state, only a notification/audit companion to facts.

### Collector Snapshot

`collector_snapshots` stays useful for debugging and redacted source inspection. Product pages should not parse `normalized_json` as their primary source once fact tables/projections cover the same data.

## Shared Interfaces

### Query Services

Add shared TypeScript query modules first, backed by existing API calls. These do not change persisted behavior.

Recommended files:

- `src/lib/queries/stationQueries.ts`
- `src/lib/queries/pricingQueries.ts`
- `src/lib/queries/dashboardQueries.ts`
- `src/lib/queries/keyPoolQueries.ts`

Initial functions:

- `loadStationFactBundle(stationId)`
- `loadStationDetailBundle(stationId)`
- `loadAllStationPricingFacts()`
- `loadStationAssetWorkspace()`
- `loadKeyPoolWorkspace()`

Rules:

- Query services own `Promise.all` composition and partial failure policy.
- Pages own loading state and user intent, not data choreography.
- Query services return raw facts plus projection-ready grouping maps when useful.
- Start in TypeScript to avoid Rust API churn. Promote stable bundles to Tauri commands only after consumers converge.

### Projection Services

Add shared projection modules with pure functions.

Recommended files:

- `src/lib/projections/groupFacts.ts`
- `src/lib/projections/balanceFacts.ts`
- `src/lib/projections/pricingFacts.ts`
- `src/lib/projections/stationFacts.ts`
- `src/lib/projections/runtimeSnapshot.ts`

Initial functions:

- `buildCurrentStationGroupFacts(bindings, rates)`
- `latestGroupRatesByBindingOrHash(rates)`
- `buildStationGroupOptionsFromCurrentFacts(groupFacts)`
- `buildStationKeyCurrentFacts(keys, groupFacts)`
- `buildCurrentStationBalances(stations, balances)`
- `buildPricingCandidates(models, stations, groupFacts, pricingRules, modelEvidence)`
- `buildRuntimeRouteSnapshot(settings, aliases, keys, capabilities, health, groupFacts, pricingRules, balances)`

Rules:

- Projection functions should be deterministic and side-effect free.
- Every projection must document fallback order.
- Projection types should expose `source` and `evidence` fields so the UI can explain where values came from.
- Page-specific view models can still exist, but they should consume projections rather than raw fact tables.

### Backend Shared Capabilities

Keep and expand Rust shared services for operations that must be transactionally correct:

- `save_station_key_with_defaults`
- `list_station_group_options`
- `list_channel_monitor_summaries`
- future `compile_runtime_route_snapshot`

Use backend services for persistence and operations. Use TypeScript projection services first for read-side convergence, then move stable projections backend-side only when needed for runtime or performance.

### API Fallback Wrapper

Introduce one helper for preview fallback:

- `invokeOrFallback(command, args, fallback)`
- `isInvokeUnavailable(error)`

Rules:

- API modules may provide preview fallback data.
- Fallback code must not introduce different business rules from real Tauri commands.
- Complex fallback behavior should call the same projection utilities used by real data.

### Shared Formatting And Labels

Consolidate:

- `readError`
- `toTime`
- `formatDateTime`
- `formatRelativeTime`
- `formatMoney`
- `formatRate`
- `formatMultiplier`
- `stationKeyStatusLabel`
- `collectorRunStatusLabel`
- `groupBindingStatusLabel`
- `balanceStatusLabel`
- status-to-tone maps

Rules:

- Formatters should not contain business joining logic.
- Status labels should be centralized enough to avoid UI copy drift, but feature-specific wording can stay local when truly different.

## Non-Breaking Migration Plan

### Stage 0: Baseline And Safety Net

No production behavior changes.

Add or tighten tests for current desired behavior:

- Price page does not show duplicate rows for the same current group identity.
- Price page still supports multiple legitimate groups under one station/model.
- Station detail shows current group status and latest multiplier consistently.
- Station asset list balance uses latest station-scope balance snapshot before station fallback.
- Key pool edit preserves current group binding unless the user explicitly clears or changes it.
- Add provider remote group sync preserves `group_binding_id` and upstream group identity.
- Local proxy route candidate still exposes group binding, multiplier, pricing status, and health facts.

Also add a field ownership test/document check that fails when new direct readers of compatibility fields are added outside approved modules.

### Stage 1: Utility Deduplication

Low-risk cleanup:

- Replace page-local `readError` with `src/lib/errors.ts`.
- Move common time parsing to `src/lib/time.ts`.
- Move common multiplier/money formatting to `src/lib/formatters.ts`.
- Move status labels/tone maps to `src/lib/statusLabels.ts` where shared.

No behavior change is intended. Keep output strings stable unless an existing string is clearly broken by mojibake and the file already uses Chinese copy.

### Stage 2: Query Service Layer

Add TypeScript query services that wrap existing API calls:

- Pricing page uses `loadAllStationPricingFacts()`.
- Station detail uses `loadStationDetailBundle(stationId)`.
- Station asset page uses `loadStationAssetWorkspace()`.
- Add provider edit flow uses `loadProviderEditBundle(stationId)`.

This reduces repeated `Promise.all` without changing fact semantics.

### Stage 3: Current Group Projection

Add `buildCurrentStationGroupFacts` and related tests.

Fallback order:

1. Binding identity: `group_binding_id`.
2. Group key: `group_key_hash`.
3. Upstream group id hash: `group_id_hash`.
4. Normalized group name only as legacy fallback.

Multiplier order:

1. Binding `user_rate_multiplier`.
2. Binding `effective_rate_multiplier`.
3. Latest rate `user_rate_multiplier`.
4. Latest rate `effective_rate_multiplier`.
5. Binding `default_rate_multiplier`.
6. Latest rate `default_rate_multiplier`.
7. `null`.

Status order:

1. Binding status from `station_group_bindings`.
2. Missing/disabled state must not be hidden by older rate records.
3. Rate history can explain freshness but cannot resurrect a missing binding as available.

This stage must not delete or rewrite existing data.

### Stage 4: Pricing Projection And Page Migration

Move pricing candidate construction from page-specific matching into `pricingFacts.ts`.

Rules:

- A row identity must include model id plus current group identity.
- Same current group should not appear twice because both binding and rate record matched.
- Multiple legitimate groups under one station/model remain visible.
- Station-specific provider mapping must not be hardcoded inside the page. If a mapping is needed, it belongs in provider metadata or group projection evidence.
- `pricingRules` should be used for model evidence and overrides, not ignored.

Migration:

1. Build pricing candidates from projections.
2. Adapt `PricingPage` to render existing UI from candidates.
3. Remove page-local group/rate matching after tests pass.

### Stage 5: Station Detail And Asset Migration

Move station detail group rows and station asset chips to current projections.

Rules:

- Station detail and asset list must agree about current group count, missing groups, and displayed multiplier.
- Snapshot parsing remains only a fallback for old databases with no fact rows.
- Missing group warnings must remain visible.

### Stage 6: Key Pool And Add Provider Migration

Move group selection and draft merging to shared group option/current fact utilities.

Rules:

- Creating or editing a key must preserve `group_binding_id` when selected.
- Clearing a group must be explicit.
- Existing key compatibility fields should be refreshed through backend workflows, not page-local copies.
- Remote key creation must still resolve the real upstream group id server-side.

### Stage 7: Runtime Snapshot

Compile local proxy runtime input from facts and projections.

Output shape should include:

- snapshot id/version
- generated timestamp
- station key candidates
- station id and base URL
- secret references, not plaintext secrets
- enabled status and priority
- group binding id
- effective multiplier and source
- model allow/block/preferred lists
- pricing rule reference or pricing status
- balance status
- health/cooldown state
- route policy data

Rules:

- Proxy runtime reads the snapshot or a snapshot-producing service, not UI view models.
- Runtime snapshot compilation should be testable without launching the proxy.
- Request logs store the route decision evidence used at request time.

### Stage 8: Compatibility Field Review

Only after consumers migrate:

- Generate a reader/writer inventory for compatibility fields.
- Mark fields as active, compatibility cache, deprecated, or removable.
- Add migration notes for any future removal.
- Do not remove a field while old database states or runtime paths still require it.

## Regression Protection

### Required Test Types

- Pure projection tests for group facts, balance facts, pricing candidates, and runtime snapshot compilation.
- Page-source negative-proof tests that old duplicate logic is no longer present.
- Focused Node script tests for frontend view models.
- Rust unit tests for transactional capabilities.
- `pnpm.cmd build` after frontend migrations.
- `cargo check --manifest-path .\src-tauri\Cargo.toml` after Rust/service changes.

### Golden Behavior Cases

The following cases must be locked before or during migration:

- A group discovered from `/groups/available` and `/groups/rates` is one current group, not two rows.
- Two distinct groups with the same display name but different upstream ids are not incorrectly merged unless only legacy name data exists.
- A missing group remains missing even if old rate history exists.
- A key bound to a group keeps its binding after editing unrelated key fields.
- A user can explicitly clear a key group binding.
- Browser preview can render the pages, but preview fallback does not define business truth.
- Local proxy route candidates are unchanged unless a stage explicitly updates runtime snapshot semantics.

## Review And Rollback Strategy

Each stage should be independently reviewable and reversible.

Rules:

- Use exact-path staging only.
- Do not mix implementation with unrelated dirty files.
- Keep old commands and fields while consumers migrate.
- Prefer additive modules first, then migrate readers.
- Do not change schema constraints until tests and projections prove the intended semantics.
- If a migration breaks a page, revert that page migration while keeping the additive projection module if it is independently tested.

## Non-Goals

- Do not redesign the app UI.
- Do not remove current fields in the first implementation cycle.
- Do not replace the database.
- Do not move to a server/SaaS architecture.
- Do not add account, team, payment, cloud sync, or marketplace features.
- Do not copy implementation from AGPL/LGPL projects.
- Do not change upstream Sub2API/NewAPI semantics except through explicit adapter fixes.

## Acceptance Criteria

The architecture refactor is complete when:

- Pages consume shared query services instead of duplicating broad `Promise.all` call graphs.
- Current group/rate/balance/pricing state is produced by shared projection utilities.
- Price page, station detail, station asset list, key pool, and add-provider flows agree on current group identity and multiplier.
- Existing capabilities still work: remote key sync, group binding, key editing, price comparison, balance display, change center, route simulation, and local proxy routing.
- Compatibility fields are documented with reader/writer ownership and are not casually consumed by new code.
- Obsolete page-local duplicate logic and stale tests are removed or quarantined.
- Verification includes targeted frontend tests, targeted Rust tests when needed, and build/check commands appropriate to the touched layers.

