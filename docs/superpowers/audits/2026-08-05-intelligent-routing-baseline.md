# Intelligent Routing Upgrade Baseline

Status: superseded by the completed 2026-08-05 intelligent routing upgrade; retained as start-of-work evidence
Captured: 2026-08-05
Worktree: `E:\Dev\Projects\relay-pool-desktop-claude-audit`
Branch: `codex/claude-audit`
Start HEAD: `b45bbeb42fe7439712f8baacc9d9b593329d5808`

## Environment

- Rust: `cargo 1.97.1`
- Node: `v24.18.0`
- pnpm: `11.9.0`
- Current migration maximum: `0023_dashboard_request_metrics_rollups.sql`
- Baseline `cargo check --locked --manifest-path src-tauri/Cargo.toml --lib`: passed in 58.46s.

## Production Call Graph

```text
Proxy startup
  -> V2RoutingRepository
  -> RoutingService.load_workspace_projection_candidates_with_request_pricing
  -> runtime_candidate_adapter
  -> RouteCandidateProjection
  -> legacy planner/controller/capacity
  -> ExecutionTargetResolver and credential resolution
  -> protocol attempt and RequestFinalization
  -> health transition, request outcome, trace/read models
```

The graph is the baseline being replaced. It is not an approved target topology.

## Confirmed Baseline Debt

| Evidence | Current behavior | Final action |
|---|---|---|
| `application/operational_facts/assembler.rs` | derives revisions from timestamps and falls back to `1` | replace in Tasks 2 and 4 |
| `persistence/stores/mod.rs` | `operational_facts` and `routing_decisions` are test-gated | productionize in Task 2 |
| `application/operational_facts/queries.rs` | loads capability/health/balance/pricing rows and discards them | replace with typed snapshot facts in Task 4 |
| `application/routing_engine/{planner,selector,controller}.rs` | planner accepts projected candidate slices | replace with PlanningSnapshot pipeline in Tasks 3 and 11-15 |
| `models/routing.rs` | owns RuntimeRoutingCandidate and SchedulerAdvancedSettings | delete in Tasks 17-18 |
| `features/routing` and `lib/*/localRouting*` | LocalRoutingWorkspace is a second routing page truth | migrate in Task 16 and delete in Task 17 |
| `lib/projections/pricingGroupRefs.ts` | frontend produces canonical pricing-group hash | server-owned join in Task 8, delete frontend algorithm in Task 17 |
| `health_transitions.rs` and `health_observation_store.rs` | health derives a status string and writes it to station keys | stop writeback in Task 6, drop schema in Task 18 |
| `application/queries/dashboard_metrics.rs` | query path can invoke rollup repair | move repair to runner in Task 9 |

## Temporary Boundary

The sole temporary boundary is `intelligent_routing_qualification`.

- Owner: Tasks 3-15
- Scope: test/support composition only
- Delete by: Task 17
- Production reachability: forbidden

No compatibility flag, second selector, second writer, or generic temporary exception is authorized.

## External Reference Boundary

AgentGate, Sub2API, and other external projects informed the design review only. This implementation uses no copied routing implementation and takes no external runtime dependency.
