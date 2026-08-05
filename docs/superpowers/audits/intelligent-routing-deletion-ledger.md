# Intelligent Routing Deletion Ledger

Status: open until Task 20. Every entry below must become `deleted` or an exact historical-migration/red-fixture exception with evidence.

| Legacy owner | Current evidence | Final action | Task |
|---|---|---|---:|
| `RuntimeRoutingCandidate` | `src-tauri/src/models/routing.rs` | delete | 17 |
| `runtime_candidate_adapter` | `src-tauri/src/application/operational_facts/runtime_candidate_adapter.rs` | delete | 17 |
| candidate-slice planner/selector/controller | `src-tauri/src/application/routing_engine/{planner,selector,controller}.rs` | replace then delete | 17 |
| `SchedulerAdvancedSettings` and old settings literals | `src-tauri/src/models/{routing,settings}.rs` | migrate then delete | 18 |
| `LocalRoutingWorkspace` | backend, generated binding, API, query, and Routing page | migrate then delete | 17 |
| frontend group/pricing matcher and SHA-256 | `src/lib/projections/pricingGroupRefs.ts` | delete | 17 |
| derived `stations.status` / `station_keys.status` health writeback | health transition/store path | stop then drop | 6 / 18 |
| wide RoutingService and Store facade chains | routing application/service/query facades | collapse to owners | 17 |
| old architecture gates, fixtures, generated DTOs | `scripts/routing-*`, generated bridge | atomically replace | 18 |
| test-only production-equivalent capacity contracts | `application/routing_engine/capacity.rs` | implement or delete | 14 |
