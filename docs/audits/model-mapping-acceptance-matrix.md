# Model Mapping Acceptance Matrix

Status: Phase 1, Phase 2, bounded Phase 3 and shared control-plane paths are
locally implemented and ready for manual desktop verification. This is not a
release or live-provider qualification.
Captured: 2026-08-18. Commands use synthetic fixtures only.

The implementation now includes bounded Profile/Binding resolution,
`CandidateModelVariant` planning and retry identity, native model capability
fences, explicit fallback triggers, bounded glob matching, the Phase 2 Routing
UI, complete routing-policy document apply, typed trusted source adapters, and
the shared native watcher plus 30-second reconciliation task. Remaining work is
legacy mutation notice coverage, watcher restart/overflow release
qualification, and legacy alias retirement; routing-policy history does not
persist source provenance.

| ID | Acceptance area | Evidence | Status |
|---:|---|---|---|
| 1 | Compiler and resolver share one exact/default rule model | `cargo test --locked --manifest-path src-tauri/Cargo.toml --target-dir src-tauri/target-codex-audit --lib model_mapping` (16 passed, including CAS/idempotency) | Passed |
| 2 | Operational fact reader remains bounded and secret-safe | `cargo test --locked --manifest-path src-tauri/Cargo.toml --target-dir src-tauri/target-codex-audit --test operational_fact_reader` (7 passed) | Passed |
| 3 | Rust build after Phase 1 changes | `cargo check --locked --manifest-path src-tauri/Cargo.toml` | Passed with existing warnings |
| 4 | Rust formatting and patch hygiene | `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`; `git diff --check` | Passed |
| 5 | Frontend mapping and routing contract tests | `pnpm.cmd exec vitest run src/features/routing/ModelMappingPanel.test.tsx src/lib/api/routing.test.ts src/lib/queries/routingQueries.test.ts` (8 passed) | Passed |
| 6 | TypeScript contract and production build | `pnpm.cmd build` (theme audit, `tsc --noEmit`, Vite build) | Passed; Vite emitted a non-blocking chunk-size warning |
| 7 | Legacy write cannot fork active mapping truth | `upsert_model_alias` and `delete_model_alias` command handlers return stable `Unsupported`; migration/audit list remains read-only | Passed by code inspection; add command-level contract test later |
| 8 | Fixed mapping hand test | UI mapping tab with synthetic `fixture-client-model -> fixture-upstream-model`; proxy loopback request | Ready for manual test |
| 9 | Shared document-kind control plane | `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib services::policy_documents::tests` (strict decode, stable-read, unavailable-root, external-change, watcher/reconciliation and materialization failure fixtures) plus `document_sync_store` targeted test; composition root starts one native watcher with 750 ms coalescing, immediate reconciliation/rebuild on watcher errors, and a 30 s digest fallback for both kinds; `apply_routing_policy_document` uses document `baseRevision` CAS and typed internal source adapters | Passed locally; watcher failure-path release qualification and legacy mutation notice coverage remain |
| 10 | Rule-scoped revision consistency after full replacement | `cargo test --locked --manifest-path src-tauri/Cargo.toml --target-dir src-tauri/target-codex-final-check --lib application::model_mapping` (34 passed, including stale external base-revision rejection and startup crash-left materialized mapping recovery) | Passed |

## Phase 2 Runtime

| ID | Acceptance area | Evidence | Status |
|---:|---|---|---|
| 11 | Profile/Binding resolution precedence and bounded target expansion | Resolver/compiler unit suite (34 passed, including Key > Station > Profile default, disabled binding, rank preservation, duplicate actual-variant suppression, the three-target bound and startup recovery); production loopback `cargo test --locked --manifest-path src-tauri/Cargo.toml --target-dir src-tauri/target-codex-final-loopback --test routing_loopback_e2e -- --test-threads=1` (8 passed), including Profile Key > Station > default upstream body rewrites | Passed locally; desktop/manual qualification remains open |
| 12 | CandidateModelVariant planner / attempt consumption | Targeted admission and proxy execution tests cover variant identity, rank-aware candidate selection, retry progress and endpoint model rewriting; `cargo test ... admission` (5 passed) and `cargo test ... execution` (20 passed) | Passed locally |
| 13 | Model capability facts and fallback trigger semantics | Native capability store tests cover model identity fences; admission tests cover `no_eligible_target` and `retry_exhausted_before_output` without lower-rank jumps; production loopback suite (8 passed) covers JSON and stream fallback target rewrites, pre-output retry commitment (`output_committed=false` then `true`), post-output stream failure with fallback blocked (`output_committed=true`), and model-not-found capability recovery | Passed locally; real-provider/release qualification remains open |
| 14 | Phase 2 IPC admission and persistence | `model_mapping` command/store tests accept Profile/Binding/fallback documents and reject invalid scopes, duplicate bindings and unsupported enum values | Passed |
| 15 | Phase 2 UI and migration review workflows | `pnpm.cmd exec vitest run src/features/routing/ModelMappingPanel.test.tsx src/lib/api/routing.test.ts src/lib/queries/routingQueries.test.ts` (8 passed); UI includes Profile, Binding, fallback and legacy review editors | Passed |
| 16 | Bounded glob compiler and overlap diagnostics | Glob matcher tests cover escaping, malformed patterns, exact wildcard semantics and representative intersection; glob is compiled before runtime matching | Passed |

Not claimed by this matrix: live provider behavior, release-machine build,
full history/restore read-model UI, complete after-commit revision notices for
every legacy compatibility mutation, routing-policy history source provenance,
and legacy schema retirement. Isolated Cargo target directories are local build
directories and are not release artifacts.
