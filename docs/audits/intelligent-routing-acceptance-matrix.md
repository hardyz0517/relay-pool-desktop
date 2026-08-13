# Intelligent Routing Acceptance Matrix

Status: open; the matrix is being rerun after the latest execution-index and workspace read-model changes. Each row below has an independent,
re-runnable evidence pointer. Live-provider, release-machine and long confidence
soak evidence remain explicitly separate operational gates.

The commands are run from the repository root. The current implementation is in
the dirty worktree revision recorded by `git status --short --branch`; no commit
was created for this audit because the user requested that changes remain
unstaged and uncommitted.

| ID | Primary tasks | Independent evidence |
|---:|---:|---|
| 1 | 12, 13 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_dispatch -- --test-threads=1`; `node scripts/intelligent-routing-architecture.test.mjs` |
| 2 | 11, 14 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity_faults -- --test-threads=1` |
| 3 | 7, 11 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_quality_projection -- --test-threads=1`; `src-tauri/tests/fixtures/intelligent_routing/projectors/v1.json` |
| 4 | 7, 13 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_dispatch -- --test-threads=1` (unknown lane and bounded exploration assertions) |
| 5 | 6 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_observations -- --test-threads=1` |
| 6 | 6 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_observations -- --test-threads=1` (anonymous probe quality rejection) |
| 7 | 6, 12 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_observations -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity_faults -- --test-threads=1` |
| 8 | 5, 11 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_dispatch -- --test-threads=1` (unknown-cost neutrality); `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_economics_projectors -- --test-threads=1` |
| 9 | 13, 14 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_dispatch -- --test-threads=1` |
| 10 | 12, 15 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_runtime_state -- --test-threads=1` (affinity TTL/bounds); `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_coordinator -- --test-threads=1` |
| 11 | 15 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_coordinator -- --test-threads=1` |
| 12 | 10, 15 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_policy -- --test-threads=1`; `node scripts/routing-dto-completeness.test.mjs` |
| 13 | 11, 15 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_coordinator -- --test-threads=1`; `node scripts/intelligent-routing-qualification.mjs` |
| 14 | 13, 19 | `node scripts/intelligent-routing-qualification.mjs`; `docs/audits/intelligent-routing-qualification.md` |
| 15 | 11 | `node scripts/intelligent-routing-architecture.test.mjs --fixtures`; `scripts/fixtures/intelligent-routing-architecture/red-planner-candidate-slice/planner.rs` |
| 16 | 3, 4, 17 | `node scripts/intelligent-routing-architecture.test.mjs`; `node scripts/routing-single-owner.test.mjs` |
| 17 | 5, 8, 16 | `node scripts/routing-projection-runner.test.mjs`; `node scripts/routing-read-model-architecture.test.mjs` |
| 18 | 8, 17 | `node scripts/routing-cutover-schema.test.mjs`; `node scripts/routing-query-service.test.mjs` |
| 19 | 6, 7 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_quality_projection -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_observations -- --test-threads=1` |
| 20 | 4, 14 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_runtime -- --test-threads=1` |
| 21 | 15 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_coordinator -- --test-threads=1` (revision fence) |
| 22 | 9 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_lifecycle_reconciliation -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_production_startup_shutdown -- --test-threads=1` |
| 23 | 4, 15 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_decision_store -- --test-threads=1` |
| 24 | 1, 17 | `node scripts/intelligent-routing-architecture.test.mjs`; `docs/audits/intelligent-routing-boundary-manifest.json` |
| 25 | 5, 8 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_projector_contract -- --test-threads=1`; `src-tauri/tests/fixtures/intelligent_routing/projectors/v1.json` |
| 26 | 17 | `node scripts/routing-single-owner.test.mjs` |
| 27 | 17, 18 | `node scripts/routing-cutover-schema.test.mjs`; `pnpm.cmd generate:bindings --check` |
| 28 | 20 | `docs/audits/intelligent-routing-deletion-ledger.md` (all rows have a closed disposition) |
| 29 | 1, 17 | `node scripts/dead-code-inventory.mjs --mode ci --scope production` (zero production identities) |
| 30 | 10 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_policy -- --test-threads=1` |
| 31 | 10, 16 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_policy_field_e2e -- --test-threads=1`; `node scripts/routing-single-owner.test.mjs` |
| 32 | 10, 18 | `node scripts/routing-operational-legacy-doc-consistency.test.mjs`; `node scripts/routing-cutover-schema.test.mjs` |
| 33 | 15, 17 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_decision_store -- --test-threads=1` |
| 34 | 15, 16, 17 | `node scripts/routing-workspace-integration.test.mjs`; `node scripts/routing-query-service.test.mjs` |
| 35 | 8, 9, 17 | `node scripts/query-services-boundary.test.mjs`; `node scripts/routing-single-owner.test.mjs` |
| 36 | 4, 15 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_security_boundaries -- --test-threads=1` |
| 37 | 6 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_observations -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_outcome_persistence -- --test-threads=1` |
| 38 | 14 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_runtime_state -- --test-threads=1` |
| 39 | 10 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_policy -- --test-threads=1` |
| 40 | 10, 15 | `node scripts/routing-workspace-integration.test.mjs`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_policy_field_e2e -- --test-threads=1` |
| 41 | 8, 16 | `node scripts/routing-dto-completeness.test.mjs`; `pnpm.cmd generate:bindings --check` |
| 42 | 4, 10, 15 | `node scripts/intelligent-routing-architecture.test.mjs --fixtures`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_production_composition -- --test-threads=1` |
| 43 | 6, 15 | `node scripts/routing-operational-loopback-contract.test.mjs`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_dual_terminal_lifecycle -- --test-threads=1` |
| 44 | 17 | `node scripts/routing-single-owner.test.mjs`; `node scripts/dead-code-inventory.mjs --mode ci --scope production` |
| 45 | 17 | `node scripts/query-services-boundary.test.mjs`; `node scripts/routing-single-owner.test.mjs` |
| 46 | 4 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --test-threads=1` |
| 47 | 15 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_decision_store -- --test-threads=1` |
| 48 | 4 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_quality_projection -- --test-threads=1` |
| 49 | 1, 18 | `node scripts/intelligent-routing-architecture.test.mjs --fixtures` (red legacy gate rejected) |
| 50 | 3, 4 | `node scripts/routing-dto-completeness.test.mjs`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --test-threads=1` |
| 51 | 3, 11 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_dispatch -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_quality_projection -- --test-threads=1` |
| 52 | 6, 18 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_observations -- --test-threads=1`; `node scripts/monitoring-architecture.test.mjs` |
| 53 | 6, 7 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_quality_projection -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_outcome_persistence -- --test-threads=1` |
| 54 | 12, 15 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_coordinator -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity_faults -- --test-threads=1` |
| 55 | 13, 14 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity_faults -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_concurrency -- --test-threads=1` |
| 56 | 19 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_fault_matrix -- --test-threads=1`; `node scripts/intelligent-routing-qualification.mjs` |
| 57 | 14, 19 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_runtime_state -- --test-threads=1` |
| 58 | 19 | `node scripts/intelligent-routing-qualification.mjs`; `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-intelligent-routing-soak.ps1 -Smoke`; long live-provider soak is a separate authorized release gate |
| 59 | 4, 14 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_runtime_state -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --test-threads=1` |
| 60 | 14 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_capacity -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_production_composition -- --test-threads=1` |
| 61 | 14, 17 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_production_composition -- --test-threads=1`; `node scripts/routing-single-owner.test.mjs` |
| 62 | 13 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_dispatch -- --test-threads=1` |
| 63 | 13 | `node scripts/intelligent-routing-qualification.mjs`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_dispatch -- --test-threads=1` |
| 64 | 7, 11 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_quality_projection -- --test-threads=1` |
| 65 | 11, 13 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_dispatch -- --test-threads=1`; `src-tauri/src/application/routing_engine/algorithm_profile.rs` |
| 66 | 13 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_dispatch -- --test-threads=1` |
| 67 | 13, 15 | `node scripts/intelligent-routing-qualification.mjs`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_security_boundaries -- --test-threads=1` |
| 68 | 6, 7 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_observations -- --test-threads=1`; `node scripts/monitoring-architecture.test.mjs` |
| 69 | 1, 16, 18 | `node scripts/intelligent-routing-architecture.test.mjs`; `node scripts/routing-cutover-schema.test.mjs`; `docs/audits/intelligent-routing-boundary-manifest.json` |
| 70 | 5, 8 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_fact_reader -- --test-threads=1`; `node scripts/routing-read-model-architecture.test.mjs` |
| 71 | 8, 17 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_economics_projectors -- --test-threads=1`; `node scripts/pricing-facts-projection.test.mjs` |
| 72 | 5, 6 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_collectors -- --test-threads=1`; `node scripts/collector-capture-contract.test.mjs` |
| 73 | 6, 18 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_health_projection -- --test-threads=1`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test station_key_health_transitions -- --test-threads=1` |
| 74 | 9 | `node scripts/query-services-boundary.test.mjs`; `cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_read_model -- --test-threads=1` |
| 75 | 6, 7, 8 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_observations -- --test-threads=1`; `node scripts/routing-projection-runner.test.mjs` |
| 76 | 9 | `cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_decision_store -- --test-threads=1`; `node scripts/routing-workspace-integration.test.mjs` |
| 77 | 8, 17 | `node scripts/query-services-boundary.test.mjs`; `node scripts/routing-single-owner.test.mjs`; `node scripts/routing-cutover-schema.test.mjs` |
| 78 | 5, 8, 19 | `node scripts/intelligent-routing-qualification.mjs`; `pnpm.cmd test:contracts`; `docs/audits/intelligent-routing-qualification.md` |

## Aggregate commands rerun for this matrix

```text
cargo check --locked --manifest-path src-tauri/Cargo.toml --lib
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets -- --test-threads=1
pnpm.cmd generate:bindings --check
pnpm.cmd exec tsc --noEmit
pnpm.cmd build
node scripts/run-contract-tests.mjs
node scripts/dead-code-inventory.mjs --mode ci --scope production
node scripts/intelligent-routing-architecture.test.mjs
node scripts/intelligent-routing-qualification.mjs
node scripts/routing-operational-qualification.mjs --preflight
node scripts/routing-operational-local-self-check.test.mjs
node scripts/routing-task24-predeletion-gate.test.mjs
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-soak.ps1 -Smoke
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-local-self-check.ps1
git diff --check
```

The aggregate commands are supporting evidence only; each acceptance row above
retains its own focused command or artifact pointer.
