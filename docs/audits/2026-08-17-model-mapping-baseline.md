# Model Mapping Phase 1 Baseline

Status: Model mapping implementation audit; not a release qualification.
Captured: 2026-08-18
Worktree: `E:\Dev\Projects\relay-pool-desktop`
Migration maximum at capture: `0046_model_mapping_rejection_metadata.sql`

This record uses source counts and synthetic fixtures only. It does not include
local database contents, real model names, endpoint URLs, credentials, or
request bodies.

## Production Call Graph

```text
proxy ingress
  -> model_mapping::resolve_request (one active runtime snapshot)
  -> RouteRequestFacts.with_model_mapping(revision, fence)
  -> RoutingService.load_intelligent_planning_snapshot
       -> OperationalFactReadOptions::without_legacy_aliases
       -> PlanningSnapshotBuilder
  -> admission / target resolver
  -> endpoint adapter with the frozen mapped model
```

`/models`, `/usage`, and other mapping-bypass endpoints do not run inference
mapping. A mapping rejection returns a typed proxy failure before ordinary
candidate selection.

## Legacy Alias Inventory

The following counts are `rg` source-line counts at capture time, not runtime
invocation counts:

| Symbol family | Count | Finding |
|---|---:|---|
| `load_model_alias_pairs` | 2 | Trait and implementation only; no call site |
| `mapped_model` | 84 | Legacy helper plus tests/compatibility fields; no proxy call |
| `ModelAliasFact` | 6 | Fact/read-model compatibility and tests; not used by resolver |
| `model_alias_revision` | 67 | Historical lifecycle/candidate provenance fields remain |
| `resolved_upstream_model` | 42 | Candidate/attempt compatibility projection remains |
| `upsert_model_alias` | 19 | IPC/bridge/facade/store symbols remain; public write now returns `Unsupported` |
| `delete_model_alias` | 15 | IPC/bridge/facade/store symbols remain; public write now returns `Unsupported` |
| `list_model_aliases` | 18 | Read-only migration/audit compatibility remains |

The production planning read explicitly disables the legacy alias SQL query.
Operational-facts tests and migration/audit readers retain an opt-in legacy
read so old schema fixtures remain verifiable. The `model_aliases` table and
its migration input are not deleted in Phase 1.

## Known Gaps And Qualification Residuals

- The shared control plane has `document_kind`-partitioned sync state, strict
  JSON/stable reads, atomic materialization, a native `notify` watcher, 750 ms
  event coalescing, immediate reconciliation/rebuild on watcher errors or
  overflow, and a 30-second digest reconciliation task. Routing-policy writes
  use `apply_routing_policy_document` with document `baseRevision` CAS; the
  legacy update facade delegates to that service.
- The frontend now exposes Profile, Binding, fallback-chain and legacy review
  workflows in addition to exact/fixed rules. Full history/restore read models
  and a unified routing-policy document editor remain outside this hand-test
  surface.
- UI, file-watch, restore, migration, startup and system paths now attach a
  typed `TrustedDocumentSource` at the service boundary; IPC cannot choose an
  arbitrary source. Routing-policy history has no provenance column, so this is
  an authorization/type boundary rather than persisted source audit data.
  Legacy compatibility mutation notice coverage and watcher restart/overflow
  release qualification remain open.
- Legacy generated bridge types, store methods, lifecycle fields, and test
  helpers remain until the post-observation deletion decision.
- Runtime resolution fence is carried into request facts and revision identity;
  the opaque fence is not independently validated by `PlanningSnapshot` yet.
- Migration creates `model_mapping_rule:<id>` revision rows, while full
  document replacement must keep those object-scoped rows synchronized with
  the active rule set. Treat stale or missing rule-scoped rows as a persistence
  follow-up; the aggregate revision alone is not evidence that object fences
  are correct.
- Phase 2 runtime now expands Profile/Binding targets into bounded
  `CandidateModelVariant` values, carries variant identity through planner,
  admission, retry progress and endpoint model rewriting, and keeps capacity
  keyed to the underlying Key/account. Model-level capability verdicts are
  scoped by native model identity and fallback triggers are rank-aware. Glob
  matching is compiled with bounded wildcard semantics and overlap diagnostics.

These gaps do not block manual verification of fixed, Profile/Binding or
fallback mappings. They do block release qualification and retirement of the
legacy alias table until legacy mutation notice coverage, watcher failure-path
evidence, portable migration evidence and the deletion ledger are closed.
