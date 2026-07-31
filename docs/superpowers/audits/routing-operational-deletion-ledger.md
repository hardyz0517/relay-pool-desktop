# Routing Operational Deletion Ledger

Status: Task 24 default-v2 single-owner cutover applied; clean 60-minute pre-deletion approval passed; Task 26 final qualification must rerun after deletion/cutover commits
Date: 2026-07-31

Each entry must either be deleted by its owner task or converted into a documented, isolated compatibility projection. No entry may become a permanent second production path.

| Entry | Evidence | Problem | Delete/adapt owner | Last allowed scope |
|---|---|---|---|---|
| Legacy weighted score path | `src-tauri/src/application/routing_engine/routing_policy.rs`, `scheduler/scoring.rs`, `scheduler/selection.rs` | Multi-factor score and TopK weighted order conflict with sealed hierarchical kernel | Task 12/22 | Legacy policy migration preview and non-production fixtures |
| `cheap_first_score` input/output addition | `routing_policy.rs::estimated_cost` | Adds input and output token unit prices without a request-time scalar basis | Task 7/12 | Existing legacy behavior before migration readiness |
| Simulated capacity | legacy scheduler test/preview capacity snapshot only; `acquired_simulated` production symbol removed | Candidate can be selected without owning executable capacity | Task 15/22/24 | Isolated legacy preview/test only; default-v2 production uses `CompositeCapacityRegistry` |
| Slot unavailable in ordered IDs | legacy scheduler test/preview capacity snapshot only; `slot_unavailable` production symbol removed | Unavailable candidate remains executable intent | Task 15/16/22/24 | RoutePlan unavailable intent only; never default-v2 execution |
| Test-only scheduler feedback facade | `SchedulerRuntimeState::report_result`, `SchedulerRuntimeState::bind_session`, `AffinityStore::bind_session`, `RuntimeMetricsRegistry::report_result` behind `#[cfg(test)]` | Production cannot use same runtime feedback and affinity contract as tests | Task 14/19/22 | Unit tests until outcome consumers exist |
| Static fallback order | old `ExecutionEngine` accepted-list traversal | Runtime health/capacity changes and real attempt outcomes do not trigger proper replan | Task 16/21/22/24 | Removed from default-v2 production; fallback is controller-driven replan |
| Credential-bearing route DTOs | `RuntimeRoutingCandidate.api_key`, `api_key_secret` compatibility fields; `RouteCandidate.api_key` removed | Credentials resolved before lease/target resolution and can cross DTO/log/debug boundaries | Task 17/24/25 | Runtime candidate compatibility/read-model bridge only; default-v2 target credentials resolve after controller selection through `ExecutionCredentialResolver` |
| Full URL route/log fields | `RuntimeRoutingCandidate.upstream_base_url`, request log `upstream_base_url`, `ProxyFailureContext.candidate_upstream_base_url` | Full endpoint URLs can leak local/private provider data | Task 25 | Historical request-log sanitizer migration and read-only compatibility only; new writes remain NULL |
| Monitoring candidate DTO dependency | `monitoring/runner.rs`, `monitoring/orchestrator_transport.rs`, `application/monitoring/definition_bridge.rs` | Dependency direction is reversed; monitoring consumes routing candidate instead of shared facts | Task 3/6/24 | Transitional read-only adapter with owner and single consumer |
| Frontend pricing/group matcher | `src/lib/projections/pricingFacts.ts`, `src/lib/projections/groupFacts.ts` marked `RPD_ROUTING_BOUNDARY:display-only-routing-truth-compat` | UI can disagree with backend route/pricing semantics | Task 9/23/24 | Display-only UI compatibility until backend read model lands; not a production routing truth owner |
| Arbitrary string planner errors | routing string errors and `InternalProxyError` flattening | Public client cannot distinguish config, capacity, capability, transient, and internal errors | Task 18/22 | Existing proxy error mapping before typed taxonomy |
| Legacy policy config values | routing settings enum values | Missing multiplier ceiling cannot be silently upgraded | Task 10/11/22 | Pre-migration readiness UI and development reset/reimport window |
| Old request-coupled response finalizer | `response_body.rs::LifecycleFinalizationLease`, `ProxyStartConfig::with_legacy_request_coupled_finalization` | Attempt terminal and request terminal can be sent from one finalizer without an explicit durable attempt ack barrier | Task 28 | Removed from default-v2 production config in Task 22; remaining use must be explicit debug/test-only isolation and never share a request with dual-terminal finalization |
| Debug legacy runtime | `RELAY_POOL_PROXY_RUNTIME=legacy` policy in `PROJECT_PLAN.md` | Long-term second owner risk if it leaks into UI or automatic fallback | Task 28 | Process-start full old owner only until default-v2 local observation proves reset/reimport recovery |

Deletion gate:

- Task 24 must prove default-v2 has no second selector, capacity, pricing, feedback, or frontend truth path.
- Task 24 pre-deletion approval was produced by a clean, non-smoke run of
  `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-task24-predeletion-gate.ps1 -DurationMinutes 60`.
  The runner executes production composition, stream/drop lifecycle faults, redaction contracts and the Task 21 loopback soak, then writes ignored evidence under
  `output/routing-operational/qualification/task24-predeletion/`.
  The approved 2026-07-31 run started at `2026-07-31T15:39:56.9784240+08:00`, finished at `2026-07-31T16:40:05.1488150+08:00`, ran at commit `42367bf0398e00afd1ded4c149212ff00a02a970`, had `worktreeCleanAtStart = true`, and wrote `deletionApproved = true` in `task24-predeletion-gate-latest.json`.
- Task 24 cutover commit `9b3b3ce` switched default-v2 execution to `OperationalRouteSnapshot` + `RouteAdmissionController` + late `ExecutionTargetResolver`/`ExecutionCredentialResolver`, added `scripts/routing-single-owner.test.mjs`, removed `EndpointAdapter::prepare(RouteCandidate)`, and isolated frontend matcher code as display-only compatibility.
- Post-cutover target-branch checks on 2026-07-31 passed:
  `node scripts/routing-single-owner.test.mjs`,
  `node scripts/routing-operational-architecture.test.mjs`,
  `node scripts/local-proxy-v2-boundary.test.mjs`,
  `cargo check --locked --manifest-path src-tauri/Cargo.toml`.
- Task 25 sanitized historical `request_logs.upstream_base_url` through schema 18:
  `0018_request_log_url_sanitizer.sql` adds bounded progress state;
  the persistence upgrade coordinator runs a pre-18 byte-level scrub before schema-upgrade backup creation, then runs the schema-18 progress sanitizer before runtime ready;
  `PersistenceRuntime::open_current` refuses schema >=18 databases whose sanitizer progress is incomplete or whose legacy full URL rows remain.
  The sanitizer reads old values as bytes, treats invalid UTF-8 as `redacted_unparseable`, uses `url::Url` for structured parsing, clears the original column to NULL, and performs WAL truncate + `VACUUM` + WAL truncate after completion.
- Task 25 target-branch checks on 2026-07-31 passed:
  `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_url_sanitizer_migration -- --nocapture`,
  `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_security_boundaries -- --nocapture`,
  `node scripts/local-routing-redaction.test.mjs`,
  `pnpm.cmd verify:persistence-artifacts`,
  `pnpm.cmd architecture:security`,
  `cargo check --locked --manifest-path src-tauri/Cargo.toml`,
  `pnpm.cmd test:contracts`,
  `pnpm.cmd build:demo`.
  The schema17 fixture seeds a URL canary plus invalid UTF-8 and scans active DB, WAL, SHM and the app-generated schema backup for the canary after upgrade.
- Task 25 also fixed the architecture security build-entry gate by keeping `src/demo.tsx` isolated from desktop backend/generated transport imports; this was required for the plan's security command to pass and does not change production entry behavior.
- Earlier smoke run on 2026-07-31 passed the command chain and report shape, but correctly set `deletionApproved = false` because the worktree was dirty and the run was not a clean 60-minute observation.
- Task 28 may delete debug legacy only after the separate local observation and reset/reimport preconditions in the plan are met.
- Any new temporary adapter must add owner, consumer, expiry task, and forbidden scopes to this ledger in the same commit.

Task 28 debug legacy runtime deletion ticket:

- Status: registered only; code deletion is not approved yet.
- Owner: Task 28.
- Created: 2026-07-31 on `codex/routing-operational-upgrade`.
- Current automated evidence:
  - Task 27 local deterministic self-check runner `scripts/run-routing-operational-local-self-check.ps1` exists and writes ignored evidence under `output/routing-operational/qualification/local-self-check/`.
  - Clean HEAD run at commit `6d55d33de469e9c4d78988380b6c53b8d72929cb` reported `schemaVersion = 1`, `kind = routing-operational-local-self-check`, `totalSteps = 11`, `failures = 0`, `worktreeCleanAtStart = true`, and `worktreeCleanAtFinish = true`.
  - The runner reuses existing suites for known-schema import, upgrade recovery, fresh generation-two config, sanitizer resume/startup readiness, startup lifecycle reconciliation, configured routing fields, catalog decision/cost persistence, redaction boundaries, and Task 26 self-check wiring.
- Missing preconditions before physical deletion:
  - authorized real OpenAI-compatible client smoke for buffered, streaming, cancel, model listing, fallback, and stable error body;
  - authorized low-frequency real provider semantic fixture for auth/model errors where adapter evidence exists;
  - CCSwitch fixed-local-entry cooperation check;
  - Windows sleep/resume and UI timeline versus SQLite journal/decision/health/cost reconciliation;
  - explicit confirmation that no P0/P1 remains in default-v2 observation.
- Future deletion/adaptation scope, once the missing preconditions are satisfied:
  - remove `ProxyFinalizationMode::LegacyRequestCoupled` and `ProxyStartConfig::with_legacy_request_coupled_finalization` from `src-tauri/src/services/proxy/runtime.rs`;
  - remove runtime branches in `src-tauri/src/services/proxy/runtime.rs` that dispatch to request-coupled finalization instead of dual-terminal finalization;
  - remove or rewrite tests that only prove legacy request-coupled finalization, keeping dual-terminal lifecycle tests;
  - update `scripts/local-proxy-v2-boundary.test.mjs`, `scripts/routing-single-owner.test.mjs`, `docs/PROJECT_PLAN.md`, and this ledger so no debug legacy process-start owner is advertised as a supported recovery path.
- Forbidden deletion shortcut: do not remove only the env/documentation string or enum name while leaving an unreachable request-coupled finalization branch behind.
- Supported recovery after deletion: stop admission, reset local data, reimport config, or reconfigure with the current dev binary. Old binary rollback remains outside the development-phase contract.
