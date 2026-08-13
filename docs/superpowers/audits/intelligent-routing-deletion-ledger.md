# Intelligent Routing Deletion Ledger

Status: locally qualified; release evidence pending. The ledger is authoritative
for the remaining cutover work. No unapproved temporary or legacy production
owner remains. The explicitly listed `active boundary` entries are current
canonical contracts, not compatibility paths; they stay until their owning
domain is replaced and must not be treated as release blockers by themselves.

| Legacy owner | Evidence after cutover | Final disposition | Task |
|---|---|---|---:|
| `CanonicalRoutingCandidate` | `src-tauri/src/models/routing.rs` and canonical workspace read model | `active boundary`: retained as a credential-safe operational read fact for workspace/detail/simulation reads; production proxy execution consumes `PlanningSnapshot` plus `RoutePlanCandidate` | 16 / 17 |
| `candidate_projection` | `src-tauri/src/application/operational_facts/candidate_projection.rs` | `active boundary`: pure projection retained only for operational detail and simulation compatibility; workspace rows are built directly from canonical candidates | 17 |
| candidate-slice planner/selector/controller | `src-tauri/src/application/routing_engine/intelligent_planner.rs` and `admission.rs` | `deleted`: `planner_legacy.rs`, `selector.rs`, `controller.rs`, `routing_snapshot.rs`, `routing_types.rs` and `planner_contract_gate.rs` are absent from production | 17 |
| `DispatchAlgorithmSettings` and old settings literals | policy compiler and migration 0025/0026 | `historical migration`: old rows are classified/reset during import; runtime reads only the versioned policy/profile | 18 |
| `LocalRoutingWorkspace` | `src/features/routing/` and `src/lib/queries/` | `deleted`: old API client, query family, types and backend chain are absent; `RoutingWorkspaceSnapshot` is canonical | 17 |
| frontend group/pricing matcher and SHA-256 | `src/lib/projections/` and routing architecture fixtures | `red fixture`: old matcher patterns exist only in negative regression fixtures or display-only historical evidence, never as routing authority | 17 |
| derived `stations.status` / `station_keys.status` health writeback | health transitions and asset-status projector | `active boundary`: administrative asset state remains UI-owned; derived health is projected from observations and is not written back | 6 / 18 |
| `station_endpoint_health` / `station_key_health` tables | migration 0026 and portable migration fixtures | `deleted`: dropped from the current schema/catalog; replacement snapshots are projection-owned and rebuildable from observations | 18 |
| wide RoutingService and Store facade chains | `node scripts/query-services-boundary.test.mjs` and `routing-single-owner.test.mjs` | `deleted`: pass-through owners were removed; policy, facts, observations, quality, workspace and trace each have one query/write owner | 17 |
| old architecture gates, fixtures, generated DTOs | `node scripts/routing-cutover-schema.test.mjs` and binding check | `deleted`: obsolete positive gates, fixtures and generated symbols are absent; red fixtures are explicitly retained as regression evidence | 18 |
| test-only production-equivalent capacity contracts | `src-tauri/src/services/proxy/routing_runtime.rs` and capacity tests | `deleted`: test-only production-equivalent contract removed; tests exercise the same runtime registry, lease and budget types used by production | 14 |
| upstream status classifier in `routing_failure::classify_route_failure` / `should_fallback` | versioned upstream `CanonicalOutcome` rule set | `deleted`: `should_fallback` has zero production references; `classify_route_failure` survives only as a planning-only helper with zero upstream-outcome consumers (contract gate `upstream-error-contract.test.mjs` green) | 9 (closed 2026-08-12) |
| `RetryPolicy::decide` HTTP status switch | canonical effect plan retry disposition and replay-safety gate | `deleted`: no `RetryPolicy::decide` or status switch remains; execution consumes `CanonicalOutcome.retry` plus the replay-safety gate (contract gate green) | 9 (closed 2026-08-12) |
| Execution `attempt_failure_kind` / `health_effect` reconstruction | lossless canonical attempt/effect plan | `deleted`: execution no longer reconstructs attempt kind or health from `ProxyFailure`/HTTP status; effects come from `classified_attempt_failure_from_canonical` (contract gate green) | 9 (closed 2026-08-12) |
| OpenAI/Responses duplicate code and message/status inference | versioned typed provider rule sets in `services/proxy/error_rules.rs` | `deleted`: adapters parse bounded evidence only; classification is owned by `error_envelope`/`error_rules` + one `CanonicalOutcome` classifier (contract gate green) | 9 (closed 2026-08-12) |
| `ProxyFailure::from_public_error` as retry/health input | OpenAI-compatible public error adapter as terminal consumer only | `deleted`: `from_public_error` is used only inside `from_canonical` as a terminal projection; retry/health/capability decisions read `CanonicalFailure` (contract gate green) | 9 (closed 2026-08-12) |
| scoped failure effect written as current Station Key health | scoped routing health verdict projector/read model | `deleted`: `apply_durable_attempt_effect` writes typed scopes/dimensions into `routing_health_observations`/`routing_health_verdicts`; legacy station-key writeback is gone | 6 / 9 (closed 2026-08-12) |

Disposition vocabulary is intentionally limited to `deleted`, `historical
migration`, `red fixture` and `active boundary`. Any new compatibility item must
be added here with one of those four dispositions before it can enter the code.
An `active boundary` entry must name its canonical owner and consumer; it is not
an excuse to keep the removed routing selector, workspace, or wrapper chain.
