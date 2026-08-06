# Intelligent Routing Qualification

Status: locally qualified; live-provider and release-machine qualification are separate.

The local qualification runner is `scripts/intelligent-routing-qualification.mjs`. It verifies the
architecture boundary, deterministic seed commitment, planner/coordinator contracts, source
redaction, portable schema compatibility, projection lifecycle, policy CAS, and the full local
Rust/TypeScript/contract test matrix. Provider, sleep/resume, and external monitoring
qualification remain separate gates requiring real credentials and a controlled environment.

Latest local result from `node scripts/intelligent-routing-qualification.mjs`:

```json
{"status":"qualified","deterministicReplay":true,"policyBounds":true,"plannerOwner":"intelligent_planner","fixtureHash":"4ce6621bb01afdb1f4dba8500c686b8f57719957894057d35f7c7aa290ae1c77"}
```

## Same-revision evidence

The following commands were rerun after the routing cutover and documentation
closeout. They are the source of truth for the acceptance matrix; the matrix
keeps a focused command per acceptance instead of treating this list as a
single blanket pass:

```text
cargo check --locked --manifest-path src-tauri/Cargo.toml --lib
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets -- --test-threads=1
pnpm.cmd test -- --runInBand
pnpm.cmd generate:bindings --check
pnpm.cmd exec tsc --noEmit
pnpm.cmd build
node scripts/run-contract-tests.mjs
node scripts/intelligent-routing-architecture.test.mjs
node scripts/intelligent-routing-architecture.test.mjs --fixtures
node scripts/routing-single-owner.test.mjs
node scripts/routing-cutover-schema.test.mjs
node scripts/routing-projection-runner.test.mjs
node scripts/routing-task24-predeletion-gate.test.mjs
node scripts/intelligent-routing-qualification.mjs
node scripts/routing-operational-qualification.mjs --preflight
node scripts/routing-operational-local-self-check.test.mjs
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-soak.ps1 -Smoke
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-local-self-check.ps1
node scripts/dead-code-inventory.mjs --mode ci --scope production
git diff --check
```

`src-tauri/tests/fixtures/intelligent_routing/projectors/v1.json` is the
projector/version artifact. The boundary manifest records migration head 26,
algorithm/profile versions, projector versions, source commit and generated
binding/schema hashes for this worktree.

The deterministic operational smoke completed one iteration with zero failures;
the local self-check completed all planned steps, including the active security
boundary and proxy-auth contract. The old `local-routing-redaction` script and
`routing_planner_controller` target are deleted and no longer appear in active
runner scripts.

V1 capacity boundary: station-key limits and collected Sub2API station-account
limits are loaded as typed execution-target facts. Canonical station/provider
configuration still does not expose a stable provider-account identity, so
production compiles `ProviderAccountConstraint::NotApplicable` and does not
claim provider-account concurrency enforcement. Unknown account capacity is
represented as zero/unbounded rather than copied from the global runtime limit.

## Source search classification

The final source search was:

```powershell
rg -n "PriorityFirst|CostFirst|LocalRoutingWorkspace|DispatchAlgorithmSettings" docs --glob '!archive/**'
```

Any remaining hit is inside a migration, prohibition, deletion ledger or
historical comparison section. No active implementation, API, page or current
capability section presents the old selector, old weights or
`LocalRoutingWorkspace` as a supported owner. `node
scripts/routing-operational-legacy-doc-consistency.test.mjs` is the regression
guard for this classification.

## Explicitly separate gates

- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-intelligent-routing-soak.ps1 -Smoke` is a local deterministic smoke wrapper around the qualification runner and does not claim provider health.
- Real provider, external monitoring, sleep/resume and the optional 60-minute confidence soak were not run because they require user-authorized credentials and a controlled release environment. They remain release gates, not compatibility exceptions.
