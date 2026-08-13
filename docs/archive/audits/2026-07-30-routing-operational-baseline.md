# Routing Operational Upgrade Task 0 Baseline

Status: Task 0 baseline and contracts frozen for `codex/routing-operational-upgrade`
Date: 2026-07-30
Scope: routing facts, scheduler, proxy execution, request lifecycle, pricing, monitoring integration, security contracts, and deletion inventory.

## Branch And Workspace State

Baseline branch:

```text
## codex/routing-operational-upgrade
?? docs/archive/plans/2026-07-30-routing-operational-unification-upgrade.md
?? docs/specs/2026-07-30-routing-operational-unification-upgrade-spec.md
```

Baseline commit:

```text
ef2d381 test: update monitoring architecture checks
297c332 feat: refresh monitoring frontend flows
4e51bf6 feat: migrate key connectivity checks to operations
6f570a7 feat: upgrade monitoring execution pipeline
aa56a76 feat: update persistence for remote keys and monitoring
```

Migration tail:

```text
0006_collectors_changes.sql
0007_pricing_monitoring.sql
0008_legacy_parity.sql
0009_provider_drafts.sql
0010_status_monitoring_v2.sql
0011_remote_key_one_to_one.sql
0012_seed_builtin_monitor_templates.sql
0013_remote_key_discovery_order.sql
0014_monitor_profile_v2.sql
0015_monitor_probe_timeout_defaults.sql
```

Task 0 found no user-owned dirty hunks in this worktree before implementation. The original project worktree was not used for edits.

## Monitoring Baseline Decision

Status monitoring V2 is treated as frozen enough for Task 1 because the branch head already contains the monitoring cutover and `docs/audits/status-monitoring-qualification.md` records local deterministic qualification on 2026-07-30.

This is not a release qualification. Live provider probes, signed install, upgrade, sleep/resume, and signed updater gates still require explicit authorization and stay as release gates. Routing must integrate monitoring only through shared operational facts and narrow health/target ports; it must not import monitoring scheduler, transport, or read-model internals.

## Current Production Call Graph

```text
local HTTP ingress
  -> CanonicalProxyRequest with RequestLease and lifecycle admission
  -> V2ProxyExecutor
  -> ExecutionEngine
  -> V2RoutingRepository
  -> RoutingService
  -> RoutingStore::load_runtime_candidates
  -> RuntimeRoutingCandidate
  -> rich_route_candidate_from_v2 decrypts/preloads API key
  -> route_request builds RouteRequest
  -> router::select_route_candidates_with_scheduler
  -> SchedulerRuntimeState::schedule / legacy routing_policy
  -> ordered RichRouteCandidate list
  -> UpstreamAttemptExecutor / upstream.rs
  -> response_body lifecycle finalization
  -> RequestFinalizationService
  -> RequestLogStore and StationKey health transition
```

Observed issues in this graph:

- Candidate DTO still carries full `upstream_base_url`, `api_key`, and encrypted secret material.
- V2 repository resolves credential before a real production capacity lease exists.
- Scheduler records capacity as `acquired_simulated` and releases the guard immediately.
- Fallback is driven from a static accepted candidate order rather than per-attempt replan.
- Request finalization currently writes attempt journal and Key health, but pricing settlement, scoped endpoint/account/capability effects, decision trace, and success-only affinity are not unified consumers.
- Monitoring still consumes `RuntimeRoutingCandidate` for endpoint ping target assembly.

## Baseline Symbol Scan

Command:

```powershell
rg -n "acquired_simulated|report_result|bind_session|ordered.*candidate|upstream_base_url|RuntimeRoutingCandidate|InternalProxyError|cheap_first" src-tauri/src src scripts
```

Important hits:

| Concern | Evidence | Task owner |
|---|---|---|
| simulated capacity | `src-tauri/src/application/routing_engine/scheduler/mod.rs` | Task 15/22 |
| test-only feedback facade | `scheduler::report_result`, `scheduler::bind_session` behind `#[cfg(test)]` | Task 14/19/22 |
| credential/full URL candidate | `src-tauri/src/models/routing.rs::RuntimeRoutingCandidate` | Task 3/17/25 |
| monitoring depends on routing candidate DTO | `src-tauri/src/services/monitoring/runner.rs`, `orchestrator_transport.rs`, `application/monitoring/definition_bridge.rs` | Task 3/6/24 |
| request log full upstream URL | `request_log_write.rs`, `request_log_store.rs`, `request_lifecycle/request.rs` | Task 25 |
| public error flattening | `ProxyFailureCode::InternalProxyError` and string context | Task 18 |
| legacy cheap-first score | `routing_policy.rs::cheap_first_score` uses `input + output` when fixed price is absent | Task 7/12 |
| frontend pricing/group matching | `src/lib/projections/pricingFacts.ts` | Task 9/23/24 |

## Related Contract Conflicts

| Conflict | Selected contract | Owner | Required sync |
|---|---|---|---|
| Monitoring can use routing candidate DTO, but routing spec requires shared fact/target ports only. | Routing operational spec and status monitoring V2 target/observation ports. | Task 3/6 | Replace imports with shared endpoint/key operational facts; update monitoring architecture gate. |
| Request lifecycle has leases and writer, but production outcome consumers are partial. | Request lifecycle remains the terminal writer; Task 19 adds typed AttemptOutcome/RequestOutcome and fixed consumers. | Task 19/20 | Convert `request_finalization.rs` into module, keep writer permits, add cost/effect/decision consumers. |
| Pricing resolver exists but router/proxy and frontend do independent matching. | `PricingProjector`/`CostCalculator` become the single pricing semantics owner. | Task 7/19/23 | Route snapshot assembler and UI read models consume projector output; frontend matcher deleted. |
| Persistence V2 scans index for committed artifacts and currently caught old local paths. | Artifact scan remains authoritative. | Task 0 | Desensitized previous monitoring baseline and synchronized capture command contract. |
| Existing `cheap_first` product enum conflicts with hierarchical `CostFirst`. | Legacy policies require migration readiness before production cutover. | Task 10/11/12/22 | Add migration UI and fail-closed behavior; do not silently map missing multiplier ceiling. |
| Debug legacy proxy runtime may remain by `PROJECT_PLAN.md`, while spec deletes default-v2 legacy paths. | Keep debug runtime only as process-start full old owner; default v2 must be single owner. | Task 22/24/28 | Deletion ledger and architecture gates distinguish default-v2 deletion from later debug runtime removal. |

## External Reference Record

All external projects are references for behavior and architecture only. No source code, data structure, selector implementation, UI component, or license-covered core implementation may be copied.

| Project | Reviewed commit | License observed | Learn | Do not learn |
|---|---|---|---|---|
| Sub2API | `5a6143097db142b72a6fc848c214e97214470bdd` | LGPL-3.0 | multiple facts feeding one scheduler, priority/load/LRU layering, acquire-fail alternative selection, concurrency wait, model capability vs transient overload, single pricing resolver | fat Account object, arbitrary Extra JSON escape hatches, Redis/outbox/distributed epochs, session binding before durable success |
| claude-code-hub | `595a7d988a91c730ed63a791b4a92acb5a0e9c41` | MIT | fixed request snapshot, provider/endpoint availability separation, stream terminal before settlement/binding, decision-chain UI | thousand-line selector, per-candidate async DB/Redis checks, SaaS/Postgres/Redis/Bull complexity |
| LiteLLM | `71b825a7f0549fd9a297f7926fc5990c11323d92` | MIT for non-enterprise tree at reviewed license | provider abstraction and typed fallback concepts | broad provider fallback fields, adaptive/bandit router, callback side effects as core truth |
| Envoy | `7b7415d2609f5ecdc27ee0f351542fc842c1bf14` | Apache-2.0 | retry budget, outlier detection principles, active recovery | distributed control plane and cluster-scale complexity |
| HAProxy | `9afec06e0eb477e29b7eeaf9eb8b5039ca4a470a` | GPL-2.0-family license file at reviewed commit | maxconn/maxqueue, redispatch, fall/rise, slow-start principles | code, GPL-covered implementation details, config-language shape |

## Baseline Verification

| Command | Result | Notes |
|---|---:|---|
| `cargo test --locked --manifest-path src-tauri/Cargo.toml application::routing_engine -- --nocapture` | passed | 94 filtered routing tests passed. Does not prove production composition. |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml services::proxy -- --nocapture` | passed | 92 filtered proxy tests passed. Expected injected panic output appears in fault tests. |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml request_lifecycle -- --nocapture` | passed | Request lifecycle and import capability filters passed. |
| `pnpm.cmd exec tsc --noEmit` | passed | TypeScript check passed. |
| `cargo check --locked --manifest-path src-tauri/Cargo.toml` | passed | Rust check passed with existing dead-code warnings. |
| `pnpm.cmd test:contracts` | passed after Task 0 baseline fixes | Initially failed on stale capture command contract, old monitoring baseline local paths, and a SQLx metadata script that scanned test-only setup SQL as production. Task 0 corrected all three and reran successfully after explicit staging because the artifact scanner reads the Git index. |

## Task 0 Fixes Included

- Updated `scripts/manual-authorization-capability.test.mjs` so it matches the existing security gate that already allows `finish_provider_draft_authorization_session` in capture windows.
- Redacted local absolute paths from `docs/archive/audits/2026-07-29-status-monitoring-baseline.md`.
- Updated `scripts/sqlx-offline-metadata.test.mjs` so the runtime-query ban applies to production source before `#[cfg(test)]`, while critical production queries still require SQLx offline metadata.

These are baseline hygiene fixes required for reproducible contract runs. They do not change routing behavior.

## Exit Decision

Task 1 may start only after this Task 0 commit is created and `pnpm.cmd test:contracts` passes against the staged index. The remaining implementation blockers are deliberate Task owners, not untriaged baseline unknowns.
