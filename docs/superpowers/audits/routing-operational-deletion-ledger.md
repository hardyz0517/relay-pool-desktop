# Routing Operational Deletion Ledger

Status: Task 0 initial ledger
Date: 2026-07-30

Each entry must either be deleted by its owner task or converted into a documented, isolated compatibility projection. No entry may become a permanent second production path.

| Entry | Evidence | Problem | Delete/adapt owner | Last allowed scope |
|---|---|---|---|---|
| Legacy weighted score path | `src-tauri/src/application/routing_engine/routing_policy.rs`, `scheduler/scoring.rs`, `scheduler/selection.rs` | Multi-factor score and TopK weighted order conflict with sealed hierarchical kernel | Task 12/22 | Legacy policy migration preview and non-production fixtures |
| `cheap_first_score` input/output addition | `routing_policy.rs::estimated_cost` | Adds input and output token unit prices without a request-time scalar basis | Task 7/12 | Existing legacy behavior before migration readiness |
| Simulated capacity | `scheduler/mod.rs` writes `acquired_simulated` after immediate release | Candidate can be selected without owning executable capacity | Task 15/22 | Pure preview explanation only, never SelectedRoute |
| Slot unavailable in ordered IDs | `scheduler/mod.rs` pushes IDs before simulated capacity result | Unavailable candidate remains executable intent | Task 15/16/22 | RoutePlan unavailable intent only |
| Test-only scheduler feedback facade | `SchedulerRuntimeState::report_result`, `bind_session` behind `#[cfg(test)]` | Production cannot use same runtime feedback and affinity contract as tests | Task 14/19/22 | Unit tests until outcome consumers exist |
| Static fallback order | `ExecutionEngine` consumes accepted candidate list | Runtime health/capacity changes and real attempt outcomes do not trigger proper replan | Task 16/21/22 | Legacy production before atomic cutover |
| Credential-bearing candidate | `RuntimeRoutingCandidate.api_key`, `api_key_secret`, `rich_route_candidate_from_v2` | Credentials resolved before lease and can cross DTO/log/debug boundaries | Task 17 | Existing v2 until target resolver cutover |
| Full URL route/log fields | `RuntimeRoutingCandidate.upstream_base_url`, request log `upstream_base_url`, `ProxyFailureContext.candidate_upstream_base_url` | Full endpoint URLs can leak local/private provider data | Task 25 | Legacy read/sanitizer migration only |
| Monitoring candidate DTO dependency | `monitoring/runner.rs`, `monitoring/orchestrator_transport.rs`, `application/monitoring/definition_bridge.rs` | Dependency direction is reversed; monitoring consumes routing candidate instead of shared facts | Task 3/6/24 | Transitional read-only adapter with owner and single consumer |
| Frontend pricing/group matcher | `src/lib/projections/pricingFacts.ts` | UI can disagree with backend route/pricing semantics | Task 9/23/24 | Display-only until backend read model lands |
| Arbitrary string planner errors | routing string errors and `InternalProxyError` flattening | Public client cannot distinguish config, capacity, capability, transient, and internal errors | Task 18/22 | Existing proxy error mapping before typed taxonomy |
| Legacy policy config values | routing settings enum values | Missing multiplier ceiling cannot be silently upgraded | Task 10/11/22 | Pre-migration readiness UI and rollback window |
| Debug legacy runtime | `RELAY_POOL_PROXY_RUNTIME=legacy` policy in `PROJECT_PLAN.md` | Long-term second owner risk if it leaks into UI or automatic fallback | Task 28 | Process-start full old owner only after default-v2 release observation |

Deletion gate:

- Task 24 must prove default-v2 has no second selector, capacity, pricing, feedback, or frontend truth path.
- Task 28 may delete debug legacy only after the separate observation and rollback preconditions in the plan are met.
- Any new temporary adapter must add owner, consumer, expiry task, and forbidden scopes to this ledger in the same commit.
