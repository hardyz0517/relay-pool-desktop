# Model Mapping Control-Plane Gap

Status: Phase 1, Phase 2, bounded Phase 3 and shared control-plane implementation
qualification record; this is not a release or live-provider qualification.
Captured: 2026-08-18. Evidence is limited to source inspection and synthetic
fixtures; no local database, credentials, URLs, request bodies, or real model
data are included.

## Finding

The mapping runtime and routing-policy configuration now use the shared
document-kind projection and the production resolver/planner path. The
remaining gap is qualification and legacy compatibility cleanup, not the
unified document-apply boundary:

| Contract | Current evidence | Remaining impact |
| --- | --- | --- |
| `document_kind`-partitioned sync state and complete apply | Migration `0044_document_sync_foundation.sql` and `DocumentSyncStore` maintain separate coalescing rows for `routing_policy` and `model_mapping`; `apply_routing_policy_document` and `apply_model_mapping_document` both use complete-document CAS and the shared materializer | The compatibility `update_routing_policy` facade remains, but delegates to the complete apply service; legacy alias mutation symbols remain read-only/unsupported until retirement |
| Shared file watcher and digest reconciliation | `services::policy_documents::PolicyDocumentCoordinator` is started from the composition root; one native `notify` watcher coalesces events for 750 ms, immediately reconciles on watcher error/overflow, attempts watcher rebuild, and a 30-second digest reconciliation covers both document kinds | Restart/overflow code is implemented and locally tested; release qualification for those failure paths remains |
| Shared materialization and revision notice | Strict JSON, stable reads, guarded external-file checks, atomic materialization and durable sync status are implemented; stale external documents are rejected when their base revision is not the active DB revision; canonical routing-policy apply publishes after-commit revision notices | Legacy compatibility mutation paths still need notice coverage/retirement evidence; routing-policy history has no provenance column, so source is not persisted as history audit data |
| Trusted source context | UI, file-watch, restore, migration, startup and system adapters use the typed `TrustedDocumentSource`; IPC payloads cannot supply arbitrary provenance strings | The type boundary is complete. `TrustedDocumentSource` is not a substitute for a routing-policy history provenance column; no claim is made that source has been persisted as audit history |

## Implemented runtime boundary

The active production path now includes:

- exact, default and bounded glob matching with overlap diagnostics;
- Profile / Binding lookup with Key > Station > Profile default precedence;
- bounded `CandidateModelVariant` expansion, rank-aware planner/admission and
  retry identity;
- native-model capability identity and explicit fallback triggers;
- Phase 2 Routing UI for rules, Profiles, Bindings, fallback chains and legacy
  migration review;
- native watcher coalescing and 30-second reconciliation through the shared
  policy-document coordinator.
- complete routing-policy document apply through `baseRevision` CAS, with the
  legacy field-level facade delegating to the same service;
- typed trusted source adapters at the application boundary, without exposing
  source selection to IPC callers.

Mapping persistence remains SQLite-CAS backed, with rule-scoped revision
metadata and stale external-document rejection. The legacy alias table and
compatibility bridge are retained for migration/audit review only; production
planning explicitly disables legacy alias reads and public legacy writes return
`Unsupported`.

## Remaining control-plane gaps

1. Add notice coverage for any remaining legacy compatibility mutation path and
   keep its behavior from forking active routing truth; finish watcher
   restart/overflow release qualification.
2. Keep the legacy alias table and generated compatibility surface read-only
   until the migration review and rollback-observation window closes; only then
   decide its retirement in a separate migration.
3. Complete release-machine and live-provider qualification. Local unit,
   migration, frontend and build checks do not prove provider behavior.
4. Keep IPC workspace/validation responses bounded as the document and legacy
   review datasets evolve. The current command boundary caps known model
   options, legacy reviews and compiler diagnostics; any future mapping read
   model must preserve equivalent hard limits and expose truncation explicitly
   if the UI needs to distinguish a partial result.

These gaps do not block manual verification of fixed, Profile/Binding, fallback
or bounded-glob mappings. They do block release qualification and retirement of
the legacy alias schema until legacy mutation notices, watcher failure-path
evidence, routing-policy history provenance decisions, and operational evidence
are closed.
