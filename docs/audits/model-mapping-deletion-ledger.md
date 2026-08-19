# Model Mapping Legacy Deletion Ledger

Status: Phase 1-3 runtime cutover ledger; open residuals are intentional and
require a separate post-observation decision. No row below authorizes a second
production resolver. Captured: 2026-08-18. Current schema maximum is
`0046_model_mapping_rejection_metadata.sql`.

| Legacy owner | Current evidence | Phase 1 disposition | Exit condition |
|---|---|---|---|
| `routing_engine/model_alias.rs::mapped_model` | File removed; no production call site remains | `retired` | Keep the deletion record; no compatibility resolver may be reintroduced |
| `RoutingRepository::load_model_alias_pairs` | Trait/implementation only; no invocation | `active boundary` (unused compatibility seam) | Delete trait method and implementation after downstream test doubles migrate |
| Operational-facts alias SQL | Planning passes `without_legacy_aliases`; opt-in query remains for tests/audit | `historical migration` | Remove raw alias facts after migration review and portable import no longer needs them |
| `ModelAliasFact` / alias version vector | Compatibility fact and test projection; not resolver input | `historical migration` | Replace historical provenance reads and remove field from production fact bundle |
| `upsert_model_alias` / `delete_model_alias` IPC | Handler returns `Unsupported`; generated bindings and facade/store remain | `active boundary` (blocked write compatibility) | Remove command, bindings, DTOs, and store methods after consumers are migrated |
| `list_model_aliases` IPC | Read-only list remains available for audit/compatibility | `historical migration` | Replace with mapping workspace migration-review read model |
| `model_alias_revision` lifecycle/candidate fields | Historical provenance and compatibility projections remain | `historical migration` | New request/capability writes use mapping revision plus native model identity; then remove old fields |
| `model_aliases` SQLite table | Read by migration and explicit audit/test paths only | `historical migration` | All review records resolved, rollback window closed, portable migration no longer imports it |
| Frontend alias API/bridge types | No active page call; compatibility API remains | `active boundary` (no UI mutation path) | Delete API/query/bridge symbols with the command and DTO removal |

The only approved production mapping owner is the compiler/resolver snapshot in
`application/model_mapping`. Any future compatibility item must be added here
with an owner, a concrete consumer, and an exit condition before it is exposed
to proxy execution. The `routing_document_sync` rows and
`services::policy_documents::PolicyDocumentCoordinator` are shared document
control-plane owners, not legacy alias compatibility; the composition root
starts one native `notify` watcher with 750 ms coalescing and a 30-second digest
fallback for both document kinds. Mapping file materialization and
reconciliation must remain on that path.
