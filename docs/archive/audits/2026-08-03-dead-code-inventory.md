# Rust Dead Code Inventory

Date: 2026-08-03
Status: Tasks 0 through 8 completed for production dead-code policy; remaining warning noise is non-production/test-target cleanup

This audit is the deletion and wiring ledger for `docs/archive/plans/2026-08-03-dead-code-reliability-upgrade.md`.

## Baseline Context

- Branch: `master`
- Head: `706aed1a`
- Working tree: dirty before Task 0; pre-existing changes were preserved
- Normal `cargo check --lib`: passed, 64 warning groups
- `cargo check --all-targets`: passed, 267 dead_code groups across all targets
- Release `cargo check --release --lib`: passed, 64 warning groups
- Force-warn hidden diagnostics: passed, 529 workspace diagnostic groups / 525 unique identities

The force-warn run is filtered to repository Rust files. Dependency diagnostics are intentionally excluded because forcing a lint through `RUSTFLAGS` also affects dependency crates.

## Rules

Each candidate must close as one of:

- `wire`: production-reachable through a real owner and covered by behavior tests.
- `test-only`: available only under `#[cfg(test)]` or a narrowly justified `test-support` boundary.
- `delete`: removed together with wrappers, fixtures, and stale docs.
- `needs-decision`: temporarily unresolved; cannot remain at closeout.

`keep for future` is not an allowed state.

## Matrix Summary

| Matrix | Scope | dead_code warning groups | Notes |
|---|---|---:|---|
| default-lib | production library | 64 | `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` |
| all-targets | library, bins, unit tests, integration tests | 267 | includes test-only and integration-target diagnostics |
| release-lib | release production library | 64 | `cargo check --locked --manifest-path src-tauri/Cargo.toml --release --lib` |
| default-lib+force-warn | production library with `--force-warn dead_code` | 529 / 525 unique | repository-only diagnostics; exposes items hidden behind `allow(dead_code)` |

## Baseline Source Policy Summary

This table records the initial source-policy baseline captured before Task 7/8. The current enforced policy is recorded in "Task 8 Delta" and in the closeout document.

| Policy item | Count | Status |
|---|---:|---|
| blanket `allow(dead_code)` | 69 | baseline debt; closed by Task 7/8 |
| local `allow(dead_code)` | 11 | baseline debt; deleted or converted to audited expects by Task 7/8 |
| `expect(dead_code)` | 2 | baseline reasons did not yet meet the required ledger format; replaced by Task 7/8 audited expect policy |
| `cfg(test)` boundaries | 454 | informational; candidate helpers are handled by Task 1/6 |
| `test-support` mentions | 0 | no explicit feature boundary currently exists |

## Baseline Candidate Ledger

This was the initial candidate map. Final dispositions are recorded in the Task delta sections below; no row in this baseline table remains open at closeout.

| Area | Symbol or group | File | Current reachability | Decision | Owner | Verification |
|---|---|---|---|---|---|---|
| credentials/session | revision-aware session update and precise invalidation chain | `src-tauri/src/application/credentials.rs`, credential store, collector/provider ports | baseline: visible warning group; production caller status was unresolved before Task 3 | closed by Task 3: revision-aware persistence wired; stale invalidation chain deleted | credentials / collectors | credential, persistence session, redaction tests |
| data maintenance | recovery/admission/wrapper state | `src-tauri/src/application/data_maintenance.rs` | visible warning group; existing coordinator already owns main maintenance path | `delete` unless central admission contract is proven | data maintenance | data maintenance and migration fault tests |
| portable migration | no operation ID wrappers and allocation/test helpers | `src-tauri/src/application/data_migration/*`, `src-tauri/src/services/portable_migration/*` | baseline: visible warning group; canonical operation-ID paths already exist | closed by Task 6: no-ID wrappers deleted; allocation/test helpers isolated | portable migration | e2e, fault, malicious, recovery tests |
| routing/proxy | semantic failure bridge and affinity request fields | `src-tauri/src/application/routing_engine/*`, `src-tauri/src/services/proxy/*` | baseline: partly hidden by blanket allow; real hot path incomplete | closed by Task 2/7: success-only affinity wired; duplicate bridges deleted | routing / proxy | routing failure, runtime state, loopback, architecture tests |
| monitoring/schema/test helpers | convenience constructors, schema constant, Auto protocol branch | `src-tauri/src/application/monitoring/*`, `src-tauri/src/services/monitoring/*`, pricing monitoring DTO/query | baseline: visible warning group; some items likely test-only or delete | closed by Task 1/4/7: helpers isolated, schema wired, Auto deleted | monitoring / pricing monitoring | monitoring orchestrator, adapter contracts, contract tests |
| blanket allows | module-level and crate-level dead code masks | see script source policy output | baseline hidden debt | closed by Task 7/8: production blanket/local allows are 0 | module owners | `dead-code-inventory --mode ci`, Cargo checks |

## Task 1 Delta

Status: completed for low-risk helpers/constants.

| Item | Decision | Result |
|---|---|---|
| `ProbeTransportResult::available` | `test-only` | Removed from production impl and replaced by a local helper in `monitoring_orchestrator` integration tests. |
| `PRICING_GROUP_MONITORING_SCHEMA_VERSION` | `wire` | Query validation, workspace output, DTO validation, and DTO fixtures now use the model constant. |
| `resolver_from_parts` | `test-only` | Gated to tests; generation startup continues to use the real `prepare_generation_two_with_resolver` production entry. |
| no-resolver generation wrappers | `test-only`; Task 6 reviewed | Wrapped with `#[cfg(test)]` because production uses the resolver-aware startup path. |

Current normal `cargo check --lib` visible dead_code groups after Task 1: 59.

## Task 2 Delta

Status: completed for production `cargo check --lib` scope. Later Task 7/8 work removed the broader production blanket-allow masks and kept production dead-code diagnostics at zero.

| Item | Decision | Result |
|---|---|---|
| `RouteFailureKind::ModelUnavailable` and `classify_provider_semantic_signal` | `delete` | Removed the duplicate routing-failure bridge. Adapter-confirmed model-not-found tests now call the canonical `failure_from_provider_signal` owner directly. |
| `health_is_blocked` | `wire` | Runtime candidate projection now uses the shared helper, so expired cooldown timestamps no longer block an otherwise usable candidate and auth/balance hard failures share the same offline predicate. |
| `openai_error_semantic_signal` / `responses_error_semantic_signal` | `wire` | Upstream non-2xx responses are classified from adapter raw status/body into `ProviderErrorSemanticSignal`, then canonical failure, then `ProxyFailure`. 401/402/429/5xx and confirmed model/capability cases now produce typed proxy codes instead of the old catch-all. |
| `endpoint_adapter_error_semantic` | `wire` | Local adapter request/body transform failures now attach canonical semantic detail while preserving their existing public proxy code and message. |
| `ProxyFailure::from_public_error` and public-error mapping helpers | `wire` | Controller planning failures and upstream adapter failures now use the canonical public-error projection in production. |
| `RoutePlanningFailure` stale variants | `delete` | Removed planning variants with no production controller source (`ConfigRequired`, `PolicyRejected`, `EconomicsUnavailable`, `FactsUnavailable`, `LifecycleUnavailable`). Public error codes remain in the canonical failure layer, but the route planner no longer advertises impossible outputs. |
| `ProxyFailureCode::UpstreamHttpError` | `delete` | Removed the legacy catch-all after upstream HTTP failures were mapped to typed codes such as `upstream_rate_limited`, `upstream_unavailable`, and `upstream_authentication_failed`. Runtime and loopback tests were updated to assert the typed public codes. |
| `RetryPolicy::max_attempts` | `test-only` | Gated the duplicate helper to unit tests; production uses the controller's attempt budget directly. |
| `session_hash` / `previous_response_id` | `wire` | Execution now creates affinity lookups from these fields, enables affinity in route facts, and uses the existing planner affinity ranking when a bound station key is still eligible. |

Verification evidence:

- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`: passed.
- `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib`: passed with 0 visible dead_code warning groups.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib services::proxy::execution -- --nocapture`: 11 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_failure_contract -- --nocapture`: 19 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib runtime_candidate_projection_uses_live_health_block_window -- --nocapture`: 1 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib services::proxy::runtime::tests::v2_precommit_failure_finalizes_request_log_and_key_health -- --nocapture`: 1 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture`: 1 passed.
- `node scripts/routing-error-contract.test.mjs`: passed.

Notes:

- Unit-test builds still print all-target/test-only dead_code warnings from migration evidence helpers, preview simulation, dashboard rollup, and similar modules. These are deferred to Task 7 because production `cargo check --lib` is now clean and Task 7 owns all-target/blanket-allow cleanup.

## Task 4 Delta

Status: completed.

| Item | Decision | Result |
|---|---|---|
| `ProtocolSelection::Auto` | `delete` | Removed the planner branch and all Auto-only tests. Current monitoring definitions, DTOs, and the production bridge construct/use `Explicit`; no persisted or UI contract for Auto was found. |
| `protocol_auto` adapter module | `delete` | Removed the module export, helper implementation, capability-facts resolver, and stale test path declaration. |

Verification evidence:

- `rg -n 'protocol_auto|ProtocolSelection::Auto|resolve_protocol_auto|ProtocolCapabilityFacts|ProtocolAutoResolution' src-tauri` returned no matches.
- Monitoring architecture and adapter/orchestrator contract tests passed.
- Monitoring persistence tests passed.
- Normal `cargo check --lib` passed with 58 visible dead_code warning groups.

## Task 3 Delta

Status: completed for session update/delete split.

| Item | Decision | Result |
|---|---|---|
| `persist_station_session_if_revision` chain | `wire` | Kept as the canonical collector write-back path. NewAPI password login persists `PersistStationSessionInput` through `CollectorSourcePort::persist_station_session` with the expected endpoint revision. |
| `CredentialService::update_station_session_if_revision` | `delete` | Removed the stale `UpdateStationSessionInput` wrapper. The manual UI command still uses non-revision `update_station_session`; background collector writes use `persist_station_session_if_revision`. |
| `ProviderDraftService::update_session` | `delete` | Removed the matching draft-only wrapper; draft collection already uses `persist_session` with expected revision. |
| `CollectorSourcePort::update_station_session` | `delete` | Removed from the trait and both production/draft adapters because drivers do not call it. |
| precise session credential invalidation chain | `delete` | Removed `StationSessionCredentialKind`, application/provider-draft invalidation wrappers, trait method, and persistence invalidation helper. No command, driver, DTO, or product caller existed; `clear_station_credentials` remains the full credential reset path. |

Verification evidence:

- `rg -n 'StationSessionCredentialKind|invalidate_station_session_credential|invalidate_session_credential|CollectorSourcePort::update_station_session' src-tauri/src src-tauri/tests` returns no stale caller.
- `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib`: passed with 45 visible warning groups.
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`: passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_collectors -- --nocapture`: 51 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_sessions -- --nocapture`: 19 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test provider_conformance -- --nocapture`: 55 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib collectors -- --nocapture`: 78 passed.

## Task 5 Delta

Status: completed.

| Item | Decision | Result |
|---|---|---|
| `DataCommandAdmission` and `admit_command` | `delete` | No production command-dispatch caller or user-visible admission contract exists; persistence freeze remains the authoritative activation boundary. Removed the unused enum, method, and admission-only tests. |
| `DataMaintenanceState::Recovering` plus `enter_recovery`/`finish_recovery` | `delete` | Startup recovery completes before the ready runtime is composed, so the in-process coordinator has no recovery owner. Removed the unreachable state and transitions. |
| `MutationRejected` | `delete` | Only belonged to the removed admission layer and had no stable DTO mapping. |
| `freeze_for_activation` / `freeze_dependencies_for_activation` wrappers | `delete` | Production import activation already uses `freeze_dependencies_for_activation_except` plus `commit_activation_lease`; tests now exercise that canonical path directly. |
| `DataMaintenanceLease::activity` | `test-only` | Kept only for the coordinator unit assertion under `#[cfg(test)]`; it is absent from production builds. |

Verification evidence:

- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib data_maintenance -- --nocapture`: 5 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_e2e -- --nocapture`: 1 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_faults -- --nocapture`: 1 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_fault_matrix -- --nocapture`: 27 passed.
- `node scripts/portable-migration-startup-boundary.test.mjs`: passed.
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`: passed.
- Normal `cargo check --lib` passed with 53 visible dead_code warning groups.

## Task 6 Delta

Status: completed for production `cargo check --lib` scope. Later Task 7/8 work converted the remaining production debt into delete/test-only/contract-expect dispositions; residual warnings are test-target/source-included fixture noise.

| Item | Decision | Result |
|---|---|---|
| `DataMigrationExportService::export_portable_package` | `delete` | Removed the no-ID convenience entry; the command facade already supplies a durable export ID through `export_portable_package_with_export_id`. |
| `DataMigrationImportService::inspect_portable_package` | `delete` | Removed the no-ID convenience entry; inspection commands use `inspect_portable_package_with_inspection_id`. |
| `DataMigrationImportService::prepare_portable_import_for_activation` | `delete` | Removed the no-ID activation entry; command and tests use the import-ID-aware path, preserving exclusion of the current operation from drain. |
| `DataMigrationImportService::prepare_portable_import` | `test-only` | It is a pure target-rebuild helper used only by in-module tests; production commands use the activation-aware operation-ID path. Gated with `#[cfg(test)]`. |
| `supported-v1-reader-valid` fixture manifest hash | `wire` | Corrected the manifest to the SHA-256 of the already-tracked fixture file; no fixture bytes changed. |
| `PortablePackageExportArtifact`, `PortableExportArtifact`, `PortableImportPrepareArtifact`, `PortableImportActivationPrepareResult` evidence fields | `test-only` | Production terminal DTOs only consume export ID, restart flag, target path/hash/key, row counts, and package size. Package path, atomic publish evidence, manifests, self-test reports, rekey reports, import mode, backup path, and freeze evidence are now compiled only for unit-test evidence. |
| `read_framed_payload`, `decrypt_framed_payload`, `ParsedPortablePayload` | `test-only` | Production import uses bounded streaming `*_to_writer` paths. The allocation-style helpers remain available only for framing/envelope unit tests so large package inspection cannot accidentally route through an in-memory plaintext buffer. |
| `app_secret_binding_policy`, `validate_setting_key`, `validate_secret_selector`, catalog unknown-key error wrappers | `delete` | Transform code already consumes the canonical `setting_policy` / `secret_policy` owner and maps unknown entries through `TransformError`. Removed the duplicate wrapper layer and adjusted catalog tests to assert the canonical allowlists directly. |
| `DataCategory::LocalProxyAccessKey` | `delete` | It was never a table category. The manifest still excludes `"local_proxy_access_key"` by string contract; table occupancy categories only cover actual catalog tables. |
| `PortableActivationFault::Injected` | `test-only` | Fault injection is a test matrix mechanism. Production activation still uses the same `PortableActivationFaults` port with `NoPortableActivationFaults`, but the injected variant is not compiled into production. |
| `AgeEnvelopeErrorCode::Cancelled` and unused `AgeEnvelopeError::stable_code` wrapper | `delete` / `test-only` | Age envelope has no direct cancellation path; cancellation is owned by snapshot/export operation layers. Removed the stale code and kept `AgeEnvelopeError::code()` only for unit assertions. |
| `PortableSchemaFingerprint`, `trusted_schema_fingerprint_v1`, `occupancy_categories_v1`, `encode_hex` | `test-only` | Production reader validates live schema objects, compatibility, foreign keys, and fixed SELECTs. Fingerprint/occupancy helpers are catalog-drift probes used by unit tests only. |
| `reject_direct_sqlite_copy_source`, `PortableSnapshotError::UnsafeDirectCopy`, `sqlite_sidecar_path` | `test-only` | Production export always uses SQLite verified backup for consistent snapshots. The direct-copy guard remains only as a test proof that raw WAL sidecar copying is unsafe and must not become the production path. |
| `PortableActivationStartup` evidence fields and manual reason residue | `delete` | Removed unused `target_keys`, manual recovery operation/candidate evidence, stale manual reason variants, and `activation_journal_exists`. Startup reloads by `target_key_id`, and manual recovery UI state is derived from the journal/ready startup state instead of this internal evidence object. |
| `PortableMigrationBlockedReasonDto` and `PortableImportRecoveryReasonCodeDto` stale variants | `delete` | Removed blocked/recovery DTO variants with no production constructor and synced `data_migration.typescript.txt`. Current serialized outputs are unchanged; the public possible-output type is narrower. |
| `drain_deadline()` and `validate_regular_field_len()` helpers | `delete` | `drain_deadline_secs` and `max_regular_field_bytes` remain as limits data/DTO fields, but the unused helper methods and unreachable `RegularFieldTooLarge` variant were removed. Activation freeze currently uses the prepare deadline; regular field validation is not wired to production transform logic. |

Verification evidence:

- `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib`: passed with 16 visible warning groups; no remaining portable-migration/data-migration production dead_code warnings.
- Portable migration e2e/fault/malicious tests passed (1/1 each).
- Persistence upgrade recovery passed (24/24); generation upgrade unit suite passed (12/12).
- Portable migration unit suite passed (49/49).
- `portable-migration-boundary.test.mjs`: passed.
- `portable-migration-redaction.test.mjs`: passed.
- `portable-migration-fixture-matrix.test.mjs`: passed, 10 cases.
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`: passed.

Task 6 closeout:

- None for production `--lib`. Test-target-only evidence warnings remain visible under unit/integration builds and are tracked as non-production warning-noise cleanup, not release production dead-code debt.

## Task 7 Delta

Status: completed for production source policy. The first pass removed routing-kernel production blanket masks and kept production `cargo check --lib` at zero dead-code diagnostics; the continuation removed the remaining IPC/test masks or converted them into audited contract expects.

| Item | Decision | Result |
|---|---|---|
| routing-engine module blanket `allow(dead_code)` | `delete` / `test-only` | Removed blanket masks from `affinity`, `request`, `eligibility`, `planner`, `selector`, `controller`, `capacity`, and `runtime_metrics`. Exposed test helpers were either removed, wired, or gated with `#[cfg(test)]`. |
| operational target resolver blanket `allow(dead_code)` | `delete` | Removed the blanket mask from `application/operational_facts/target_resolver.rs`; the remaining production surface is reachable through runtime candidate projection and execution target resolution tests. |
| `RouteProgress::tighten_deadline` | `delete` | Removed the unused helper and updated route candidate projection test expectations to assert the current projection behavior directly. |
| `RoutePlanStratum::candidate_ids` and controller inspection helpers | `test-only` | Gated plan/controller inspection helpers and the `WaitWakeup` trace variant behind test cfg because production only consumes route decisions, not internal trace accessors. |
| `SelectedRoute.retry_permit` | `wire` | Kept the retry permit alive through `LeasedSelectedTarget` and `ExecutionTargetHandle` so retry budget leases now match the selected target lifecycle instead of being dropped at selection. |
| `ControllerDecision::Wait` / `Replan` fields | `wire` | Production execution now consumes non-selected controller decisions through `selected_route_or_failure`, preserving explicit failure mapping instead of relying on unused decision fields. |
| capacity gauges, provider-account evidence, wait tickets, retry-budget inspection | `test-only` | Gated diagnostic/test-only capacity state and evidence accessors while keeping production `NotApplicable` provider-account constraint behavior. |
| runtime outlier state | `test-only` | Isolated outlier accumulation state to tests. `RuntimeModelClass` remains production because affinity and route facts use it. |
| adapter capability signal surface | `delete` | Removed `services/proxy/adapters/capability.rs`, the chat/responses structural capability helpers, and the tests that only kept that experimental adapter surface alive. The current production proxy path uses `ProviderErrorSemanticSignal` and canonical request-finalization failure mapping instead. |
| command outbound failure mapping | `delete` | Removed `commands::error::OutboundFailure` and `CommandError::from_outbound` because no command or shared outbound client production caller existed. Kept the real `WorkFailure` and current `DriverFailure` command mappings. |
| local `allow(dead_code)` sweep | `delete` / `wire` / `contract-expect` | Removed stale local allows from the credential vault, monitoring runner, station collector runner, command error mappings, differential-test helper, and routing dual-terminal test stub. Converted `RouteCandidateEconomics` to a contract-scoped `expect` because those economics fields are serialized DTO output owned by routing workspace, not deletion candidates. |
| `capability_evidence` test crate mask | `delete` | Removed the crate-level blanket allow after deleting adapter-only stubs; added a subject coverage test so all remaining capability projector variants are intentionally exercised. |
| pricing service blanket `allow(dead_code)` | `delete` | Removed the blanket mask from `services/pricing`. Kept the production-used `pricing_context_from_pricing_parts` projection helper and deleted the uncalled cost-estimation, input-sanitization, and summary helpers plus their orphan `RequestCost*` / `RequestUsage` model types and tests. |
| monitoring service blanket `allow(dead_code)` | `delete` / `test-only` | Removed the blanket mask from `services/monitoring`. Deleted the unused `ProtocolAdapter::protocol_kind` method and the unconstructed stable-local-identity header variant; isolated generic JSON/SSE parser contract helpers, challenge test constructor, transport loopback constructor/metrics, response evidence, and executor debug-only output fields to test builds. Production monitoring continues through `ProbeExecutorTransport` and the typed probe executor path. |
| operational facts application/model blankets | `test-only` / `contract-expect` | Removed the blanket masks from `application/operational_facts` and `models/operational`. Isolated the unconnected operational fact assembler/reader/runtime-health-port and full operational-domain value-object aggregates to test builds. Kept production-used route projection value objects and marked reserved balance/capability/health/capacity/multiplier states as production-only contract expectations. Kept the `ExecutionTargetHandle` capacity lease as an RAII-owned field with an explicit production contract reason. |
| operational facts persistence store blanket | `test-only` | Removed the blanket mask from `persistence/stores/operational_facts` and gated the store module plus raw operational fact rows to test builds. Current production route projections use runtime candidate snapshots; the SQL operational-fact reader remains covered by `operational_fact_reader` as a store-shape contract. |
| request-finalization blankets | `test-only` / `contract-expect` | Removed the blanket masks from `failure`, `outcome`, `effect_planner`, and `outcome_orchestrator`. Kept the canonical failure/outcome/status taxonomies with production-only contract expectations, gated the unconnected effect planner and typed attempt-outcome aggregate to tests, and left the production cost snapshot/aggregate/orchestrator commit path active. |
| request lifecycle/routing decision persistence blankets | `wire` / `test-only` | Removed the blanket mask from startup lifecycle reconciliation; it is production-wired through `RequestFinalizationService` and produced no hidden warnings. Removed routing-decision store blankets and gated the unconnected writer/query/retention module to tests; `routing_decision_store` remains the durable store-shape contract until a production read/write owner is connected. |

Verification evidence:

- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`: passed.
- `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib`: passed with 0 dead-code warning groups.
- `node scripts/dead-code-inventory.mjs --mode verify --scope production`: passed with 0 default-lib diagnostics; blanket `allow(dead_code)` count reduced from 69 to 58 and local `allow(dead_code)` count reduced from 11 to 0.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity -- --nocapture`: 5 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity_faults -- --nocapture`: 4 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_planner_controller -- --nocapture`: 7 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test execution_target_resolver -- --nocapture`: 15 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test capability_evidence -- --nocapture`: 7 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib commands::error -- --nocapture`: 8 passed; unit-test target still reports deferred all-target warnings unrelated to command error mapping.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib services::pricing -- --nocapture`: 4 passed; unit-test target still reports deferred all-target warnings outside the pricing service production surface.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_adapter_contracts -- --nocapture`: 17 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_transport -- --nocapture`: 2 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_execution_integration -- --nocapture`: 17 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_domain -- --nocapture`: 6 passed; source-included test target still reports deferred test-fixture dead-code warnings.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --nocapture`: 6 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_economics_projectors -- --nocapture`: 8 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_health_projection -- --nocapture`: 5 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test capability_evidence -- --nocapture`: 7 passed; source-included `operational_model` warnings remain test-fixture cleanup work.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test route_candidate_projection -- --nocapture`: 4 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test execution_target_resolver -- --nocapture`: 15 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_pricing_projection -- --nocapture`: 12 passed.
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_read_models -- --nocapture`: 13 passed.
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`: passed after operational facts/model cleanup.
- `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib`: passed with 0 dead-code warning groups after operational facts/model cleanup.
- `node scripts/dead-code-inventory.mjs --mode verify --scope production`: passed with 0 default-lib diagnostics; source policy now shows 44 blanket allows, 0 local allows, and 2 single-line expects.

Task 7 closeout:

- Operational facts, request-finalization, request-lifecycle reconciliation, and routing-decision persistence blankets are cleared.
- IPC DTO/registry masks were converted to audited `expect(dead_code)` contracts with `contract=...`, `owner=...`, and `remove_when=...`.
- Integration test crate-level blanket masks were removed; remaining test-target warnings are source-included fixture noise and are tracked outside the production dead-code policy.
- Multi-line `#[expect(dead_code)]` reporting was fixed in Task 8; CI now counts and validates all 54 registered expects.

## Command Log

| Command | Exit code | Result |
|---|---:|---|
| `git status --short --branch` | 0 | captured before Task 0 |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | 64 visible warning groups |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --all-targets` | 0 | 267 workspace dead_code groups |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --release --lib` | 0 | 64 visible warning groups |
| `node scripts/dead-code-inventory.mjs --mode baseline` | 0 | 159 unique workspace identities across the three matrices |
| `node scripts/dead-code-inventory.mjs --mode force-warn` | 0 | 529 workspace groups / 525 unique identities |
| `git diff --check -- docs/archive/audits/2026-08-03-dead-code-inventory.md scripts/dead-code-inventory.mjs package.json` | 0 | clean; Git reports only its normal LF/CRLF normalization note for `package.json` |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_orchestrator -- --nocapture` | 0 | 11 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_adapter_contracts -- --nocapture` | 0 | 17 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_persistence -- --nocapture` | 0 | 10 passed |
| `node scripts/monitoring-architecture.test.mjs` | 0 | passed |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | 58 visible warning groups |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib data_maintenance -- --nocapture` | 0 | 5 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_e2e -- --nocapture` | 0 | 1 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_faults -- --nocapture` | 0 | 1 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_fault_matrix -- --nocapture` | 0 | 27 passed |
| `node scripts/portable-migration-startup-boundary.test.mjs` | 0 | passed |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 0 | passed |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | 53 visible warning groups |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | 51 visible warning groups |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_e2e -- --nocapture` | 0 | 1 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_faults -- --nocapture` | 0 | 1 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_malicious -- --nocapture` | 0 | 1 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_upgrade_recovery -- --nocapture` | 0 | 24 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib generation_upgrade -- --nocapture` | 0 | 12 passed |
| `node scripts/portable-migration-boundary.test.mjs` | 0 | passed |
| `node scripts/portable-migration-redaction.test.mjs` | 0 | scanned 38 files |
| `node scripts/portable-migration-fixture-matrix.test.mjs` | 0 | 10 cases after correcting manifest hash for tracked fixture bytes |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | 45 visible warning groups |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_collectors -- --nocapture` | 0 | 51 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_sessions -- --nocapture` | 0 | 19 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test provider_conformance -- --nocapture` | 0 | 55 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib collectors -- --nocapture` | 0 | 78 passed |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 0 | passed after Task 6 evidence/catalog cleanup |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | 42 visible warning groups after evidence-field isolation |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | 35 visible warning groups after allocation parser isolation and duplicate catalog wrapper deletion |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | 33 visible warning groups after non-table local proxy category and activation fault injection cleanup |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | 31 visible warning groups after age-envelope stale cancellation cleanup |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib portable_migration -- --nocapture` | 0 | 49 passed |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | 24 visible warning groups after schema/snapshot safety probes were isolated to tests |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib portable_migration -- --nocapture` | 0 | 49 passed after schema/snapshot test-only isolation |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 0 | passed after recovery/DTO/limits cleanup |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | 16 visible warning groups; remaining warnings are routing/proxy only |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib portable_migration -- --nocapture` | 0 | 49 passed after recovery/DTO/limits cleanup |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_e2e -- --nocapture` | 0 | 1 passed after recovery/DTO/limits cleanup |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_faults -- --nocapture` | 0 | 1 passed after recovery/DTO/limits cleanup |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_malicious -- --nocapture` | 0 | 1 passed after recovery/DTO/limits cleanup |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_upgrade_recovery -- --nocapture` | 0 | 24 passed after recovery/DTO/limits cleanup |
| `node scripts/portable-migration-boundary.test.mjs` | 0 | passed after recovery/DTO/limits cleanup |
| `node scripts/portable-migration-redaction.test.mjs` | 0 | scanned 38 files after recovery/DTO/limits cleanup |
| `node scripts/portable-migration-fixture-matrix.test.mjs` | 0 | 10 cases after recovery/DTO/limits cleanup |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 0 | passed after routing/proxy semantic and affinity cleanup |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | 0 visible dead_code warning groups after Task 2 |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib services::proxy::execution -- --nocapture` | 0 | 11 passed; unit-test target still reports deferred all-target warnings |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_failure_contract -- --nocapture` | 0 | 19 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib runtime_candidate_projection_uses_live_health_block_window -- --nocapture` | 0 | 1 passed; unit-test target still reports deferred all-target warnings |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib services::proxy::runtime::tests::v2_precommit_failure_finalizes_request_log_and_key_health -- --nocapture` | 0 | 1 passed; unit-test target still reports deferred all-target warnings |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture` | 0 | 1 passed |
| `node scripts/routing-error-contract.test.mjs` | 0 | passed |
| `git status --short --branch` | 0 | captured before continuing Task 7; working tree remained dirty with prior changes preserved |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 0 | passed after Task 7 first pass formatting |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed with 0 dead-code warning groups after Task 7 first pass |
| `node scripts/dead-code-inventory.mjs --mode verify --scope production` | 0 | 0 default-lib diagnostics; source policy shows 60 blanket allows, 11 local allows, 2 expects |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test capability_evidence -- --nocapture` | 0 | 6 passed after removing adapter capability signal surface; linker-only warnings from Windows build output |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib commands::error -- --nocapture` | 0 | 8 passed after deleting unused outbound command-error mapping; unit-test target still reports deferred all-target dead_code warnings |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 0 | passed after local allow sweep formatting |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed with 0 dead-code warning groups after local allow sweep |
| `node scripts/dead-code-inventory.mjs --mode verify --scope production` | 0 | 0 default-lib diagnostics; source policy shows 58 blanket allows, 0 local allows, 2 single-line expects |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test capability_evidence -- --nocapture` | 0 | 7 passed after removing the test crate blanket allow and adding subject coverage |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | temporarily exposed 18 pricing warnings after removing `services/pricing` blanket allow; used as RED evidence |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib services::pricing -- --nocapture` | 0 | 4 passed after deleting uncalled pricing cost/sanitize/summary surface; unit-test target still reports deferred all-target warnings |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 0 | passed after pricing cleanup formatting |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed with 0 dead-code warning groups after pricing cleanup |
| `node scripts/dead-code-inventory.mjs --mode verify --scope production` | 0 | 0 default-lib diagnostics; source policy shows 57 blanket allows, 0 local allows, 2 single-line expects |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | temporarily exposed 17 monitoring warnings after removing `services/monitoring` blanket allow; used as RED evidence |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_adapter_contracts -- --nocapture` | 0 | 17 passed after monitoring test-only/parser cleanup |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_transport -- --nocapture` | 0 | 2 passed after transport test-only cleanup |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_execution_integration -- --nocapture` | 0 | 17 passed after executor output/debug field isolation |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 0 | passed after monitoring cleanup formatting |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed with 0 dead-code warning groups after monitoring cleanup |
| `node scripts/dead-code-inventory.mjs --mode verify --scope production` | 0 | 0 default-lib diagnostics; source policy shows 56 blanket allows, 0 local allows, 2 single-line expects |
| `git status --short --branch` | 0 | captured before operational facts/model cleanup; existing dirty worktree preserved |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | temporarily exposed 53 operational-facts warnings after removing `application/operational_facts` blanket allow; used as RED evidence |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed with 0 dead-code warning groups after isolating operational facts assembler/reader/runtime-health-port and adding route projection contract expectations |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | temporarily exposed 62 model warnings after removing `models/operational` blanket allow; used as RED evidence |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed with 0 dead-code warning groups after moving full operational-domain aggregates to test builds |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_domain -- --nocapture` | 0 | 6 passed; source-included test target still reports deferred test-fixture warnings |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --nocapture` | 0 | 6 passed after keeping raw fact reader contracts test-only |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_economics_projectors -- --nocapture` | 0 | 8 passed after production-only expectation attrs were corrected |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_health_projection -- --nocapture` | 0 | 5 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test capability_evidence -- --nocapture` | 0 | 7 passed; source-included operational model warnings remain deferred test-fixture cleanup |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test route_candidate_projection -- --nocapture` | 0 | 4 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test execution_target_resolver -- --nocapture` | 0 | 15 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_pricing_projection -- --nocapture` | 0 | 12 passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_read_models -- --nocapture` | 0 | 13 passed |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 1 | reported rustfmt-only diffs after operational facts/model cleanup |
| `cargo fmt --manifest-path src-tauri/Cargo.toml` | 0 | formatted operational facts/model changes |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 0 | passed after formatting |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed with 0 dead-code warning groups after operational facts/model cleanup |
| `node scripts/dead-code-inventory.mjs --mode verify --scope production` | 0 | 0 default-lib diagnostics; source policy shows 54 blanket allows, 0 local allows, 2 single-line expects |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | temporarily exposed 10 operational fact store/raw-row warnings after removing the store module blanket allow; used as RED evidence |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed with 0 dead-code warning groups after gating operational fact store/raw rows to test builds |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --nocapture` | 0 | 6 passed after operational fact store/raw rows were gated to tests |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 0 | passed after operational fact store/raw-row cleanup |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed with 0 dead-code warning groups after operational fact store/raw-row cleanup |
| `node scripts/dead-code-inventory.mjs --mode verify --scope production` | 0 | 0 default-lib diagnostics; source policy shows 53 blanket allows, 0 local allows, 2 single-line expects |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | temporarily exposed 7 canonical failure taxonomy warnings after removing `request_finalization/failure` blanket allow; used as RED evidence |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed after converting reserved failure taxonomy states to production-only contract expectations |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | temporarily exposed 7 outcome warnings after removing `request_finalization/outcome` blanket allow; used as RED evidence |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed after gating test-only attempt outcome construction and retaining cost/outcome status contracts |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | temporarily exposed 12 effect-planner/attempt-outcome warnings after removing `request_finalization/effect_planner` blanket allow; used as RED evidence |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed after gating effect planner and typed attempt outcome aggregate to tests |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed after removing `request_finalization/outcome_orchestrator` blanket allow |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_failure_contract -- --nocapture` | 0 | 19 passed after request-finalization cleanup |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_outcome_domain -- --nocapture` | 0 | 12 passed after request-finalization cleanup |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 1 | reported rustfmt import-order diff after request-finalization cleanup |
| `cargo fmt --manifest-path src-tauri/Cargo.toml` | 0 | formatted request-finalization cleanup |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 0 | passed after request-finalization formatting |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed with 0 dead-code warning groups after request-finalization cleanup |
| `node scripts/dead-code-inventory.mjs --mode verify --scope production` | 0 | 0 default-lib diagnostics; source policy shows 49 blanket allows, 0 local allows, 2 single-line expects |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed after removing `request_lifecycle_reconciliation` blanket allow; no hidden dead-code warnings |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | temporarily exposed 30 routing decision store warnings after removing routing-decision persistence blanket allows; used as RED evidence |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed after gating unconnected routing-decision writer/query/retention store to tests |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_lifecycle_reconciliation -- --nocapture` | 0 | 2 passed after request lifecycle reconciliation cleanup |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_decision_store -- --nocapture` | 0 | 6 passed after routing-decision store test-only gating |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_outcome_persistence -- --nocapture` | 0 | 4 passed after routing-decision/request-lifecycle persistence cleanup |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 0 | passed after persistence blanket cleanup |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed with 0 dead-code warning groups after persistence blanket cleanup |
| `node scripts/dead-code-inventory.mjs --mode verify --scope production` | 0 | 0 default-lib diagnostics; source policy shows 44 blanket allows, 0 local allows, 2 single-line expects |

### Task 7 continuation: test masks, IPC contract expects, and CI policy

| Area | Disposition | Notes |
|---|---|---|
| integration test crate-level `#![allow(dead_code)]` masks | `delete` / `test-contract` / `fixture-shrink` | Removed the remaining test crate blanket masks from `routing_capacity`, `routing_capacity_faults`, `execution_target_resolver`, `route_candidate_projection`, `routing_runtime_state`, `routing_read_models`, `routing_planner_controller`, `hierarchical_route_planner`, `operational_economics_projectors`, `operational_pricing_projection`, and `operational_fact_reader`. Route/capacity/read-model files were made warning-clean with meaningful assertions or smaller fixtures. Operational source-included tests pass but still expose deferred fixture warnings because they include the full operational model/assembler surface. |
| high-confidence stale runtime state | `delete` | Removed `RuntimeDegradedReason::HalfOpenPending`; no production or test path constructed it. Added half-open failure coverage to keep the actual runtime state machine behavior protected. |
| IPC DTO/registry `cfg_attr(not(test), allow(dead_code))` | `contract-expect` | Replaced all remaining production IPC blanket allows with audited `expect(dead_code)` reasons that include `contract=...`, `owner=...`, and `remove_when=...`. `IPC_CONTRACT_VERSION` was found to be production-used and its unnecessary expect was removed. |
| `scripts/dead-code-inventory.mjs` source-policy scanner | `wire` | Upgraded the scanner to parse complete Rust attribute blocks so multi-line `expect(dead_code)` attributes are counted and reason-validated. CI mode now catches unregistered expects instead of silently missing rustfmt-expanded attributes. |

Current Task 7/8 validation:

| Command | Exit | Result |
|---|---:|---|
| `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | 0 | passed after test-mask and IPC expect formatting |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib` | 0 | passed with 0 production dead_code diagnostics and no unfulfilled lint expectations |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --release --lib` | 0 | passed with 0 production dead_code diagnostics in release profile |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml --all-targets` | 0 | passed; lib test target still generated 229 warnings from test-only/source-included surfaces |
| `node scripts/dead-code-inventory.mjs --mode ci --scope production` | 0 | passed: blanket `allow(dead_code)` = 0, local `allow(dead_code)` = 0, expect ledger = 54 with registered contract/owner/remove conditions |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity -- --nocapture` | 0 | 8 passed; warning-clean after wait-plan/retry-budget coverage |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity_faults -- --nocapture` | 0 | 5 passed; warning-clean after provider/runtime capacity assertions |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test execution_target_resolver -- --nocapture` | 0 | 15 passed; warning-clean after replacing whole capacity source include with a minimal RAII fixture |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_runtime_state -- --nocapture` | 0 | 7 passed; warning-clean after deleting stale `HalfOpenPending` and covering half-open failure |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_read_models -- --nocapture` | 0 | 16 passed; warning-clean after DTO/read-model contract coverage |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_planner_controller -- --nocapture` | 0 | 10 passed; warning-clean after controller lease/wait/retry/request assertions |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test hierarchical_route_planner -- --nocapture` | 0 | 6 passed; warning-clean after planner helper and pricing-basis coverage |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_pricing_projection -- --nocapture` | 0 | 14 passed; warning-clean after pricing mutation DTO and cost-basis label coverage |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_economics_projectors -- --nocapture` | 0 | 8 passed; still reports source-included operational model warnings, tracked as fixture cleanup rather than deletion candidates |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --nocapture` | 0 | 6 passed; still reports source-included operational model/assembler warnings, tracked as fixture cleanup rather than deletion candidates |

### Task 8 Delta: CI/release policy and closeout

Status: completed for production dead-code policy.

| Area | Disposition | Notes |
|---|---|---|
| policy fixtures | `wire` | Added `scripts/dead-code-inventory-policy.test.mjs` covering clean source, normal dead function failure, crate/module/local `allow(dead_code)` failure, unregistered `expect(dead_code)` failure, and `test-support` marker leakage failure. |
| shared verify entrypoint | `wire` | `scripts/verify.ps1` now runs the policy fixtures and production dead-code CI policy for `fast`, `full`, and `release`; `full`/`release` also include `cargo check --all-targets` and `cargo check --release --lib`. |
| release workflow | `wire` | `.github/workflows/release.yml` now calls `pnpm verify:release:prebundle` and `pnpm verify:release:postbundle`, so YAML no longer owns a duplicate release verification path or forwards a literal `--` into PowerShell. |
| bindings and contracts | `wire` | Regenerated IPC bindings/registry and verified `pnpm.cmd generate:bindings --check` plus `pnpm.cmd test:contracts`. |
| dependency lifecycle | `wire` | Architecture dependency lifecycle gate now keeps CI Rust pinned at `1.95.0` while accepting local reference `1.97.1` as a qualified version. |
| routing workspace contract | `wire` | Routing workspace query synchronization is owned by `src/lib/query/routingQuerySynchronization.ts`; contract tests now expect `RoutingPage` to call `refreshRoutingQueries(queryClient)`. |
| persistence architecture manifest | `wire` | Added the new real dependency edges and removed stale allowlisted edges; `persistence_architecture` passes 42/42. |
| closeout document | `wire` | Added `docs/archive/audits/2026-08-03-dead-code-closeout.md` with baseline/current counts, verification, remaining warning classes, release limitations, and follow-up guidance. |

Latest Task 8 validation:

| Command | Exit | Result |
|---|---:|---|
| `node scripts/dead-code-inventory-policy.test.mjs` | 0 | fixture policy suite passed |
| `node scripts/dead-code-inventory.mjs --mode ci --scope production` | 0 | default-lib dead_code diagnostics 0; blanket/local allow 0; registered expects 54; test-support leakage 0 |
| `node scripts/release-verification-entrypoint.test.mjs` | 0 | release entrypoint tests passed |
| `node scripts/updater-config.test.mjs` | 0 | updater/release workflow contract passed after switching to phase-specific release scripts |
| `node scripts/updater-timeout-recovery.test.mjs` | 0 | updater timeout recovery contract passed after switching to phase-specific release scripts |
| `pnpm.cmd generate:bindings --check` | 0 | generated bindings are current |
| `pnpm.cmd test:contracts` | 0 | contract tests passed |
| `node scripts/architecture/check-dependency-lifecycle.mjs` | 0 | dependency lifecycle gate passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_architecture` | 0 | 42 passed |
| `pnpm.cmd verify:fast` | 0 | passed; only Windows linker stdout warnings observed |
| `pnpm.cmd verify:full` with `CARGO_BUILD_JOBS=2` | 0 | passed on the latest closeout rerun, duration 333.73s, after one earlier high-concurrency Windows page-file/mmap failure (`os error 1455`) |
| `pnpm.cmd verify:release:prebundle` | 1 | expected local fail-fast at `Release version contract` because `RELAY_POOL_RELEASE_TAG` is unset and HEAD has no exact release tag; confirms the phase script no longer fails on PowerShell `--` forwarding |

Remaining dead-code handling notes:

- Production dead code is currently gated by CI policy: default lib diagnostics are 0, production blanket/local `allow(dead_code)` are 0, and all remaining expects are registered contracts.
- The largest remaining non-production noise is operational source-included test fixtures (`operational_economics_projectors`, `operational_fact_reader`, and previously observed `route_candidate_projection` / health projection style tests). These should be handled by splitting operational model fixture/support modules or narrower stubs, not by deleting IPC/domain contract types or reintroducing blanket allows.
- `allow(unused_imports)` remains a separate cleanup class in a few re-export-heavy modules/tests and is not counted as dead-code policy in this task.
