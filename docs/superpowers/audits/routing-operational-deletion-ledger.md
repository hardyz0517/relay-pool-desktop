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
| Full URL route/log fields | `RuntimeRoutingCandidate.upstream_base_url`, request log `upstream_base_url`, `ProxyFailureContext.candidate_upstream_base_url` | Full endpoint URLs can leak local/private provider data | Task 25 | Legacy read/sanitizer migration only |
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
- Earlier smoke run on 2026-07-31 passed the command chain and report shape, but correctly set `deletionApproved = false` because the worktree was dirty and the run was not a clean 60-minute observation.
- Task 28 may delete debug legacy only after the separate local observation and reset/reimport preconditions in the plan are met.
- Any new temporary adapter must add owner, consumer, expiry task, and forbidden scopes to this ledger in the same commit.
