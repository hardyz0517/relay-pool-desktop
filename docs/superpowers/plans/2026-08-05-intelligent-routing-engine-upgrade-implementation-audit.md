# Intelligent Routing Upgrade Implementation Audit

Date: 2026-08-05
Status: In progress; this audit records the current dirty worktree evidence and remaining cutover gaps

The canonical policy aggregate, PlanningSnapshot builder, intelligent planner,
policy editor CAS path, execution-only proxy candidate index, and canonical
workspace read model are present. Release-only provider qualification is not
claimed here.

## Verified

- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`: passed (`765 passed; 0 failed`).
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets -- --test-threads=1`: passed (all targets; no failures).
- `pnpm.cmd test -- --runInBand`: passed (`82` test files, `286` tests).
- `pnpm.cmd generate:bindings --check`: passed.
- `pnpm.cmd exec tsc --noEmit`: passed.
- `pnpm.cmd build`: passed after the workspace `policyConfig` binding change.
- `node scripts/run-contract-tests.mjs`: passed (including persistence architecture and portable migration fixture gates).
- `node scripts/dead-code-inventory.mjs --mode ci --scope production`: passed (`Unique dead_code identities: 0`).
- `node scripts/intelligent-routing-qualification.mjs`: passed (`status=qualified`, deterministic replay and policy bounds true).
- `git diff --check`: passed.
- `git status --short --branch`: captured on `codex/claude-audit`; the audit leaves implementation and documentation changes uncommitted by request.
- Contract, intelligent-routing architecture, single-owner, cutover-schema, projection-runner, and qualification gates passed.
- Decision trace reads `route_decisions` and `route_candidate_decisions` directly; it no longer scans `RequestLog`.
- Routing UI consumes `RoutingWorkspaceSnapshot` and canonical query families; the old `LocalRoutingWorkspace` frontend chain is deleted.
- `load_workspace_candidates_with_request_pricing` feeds the workspace read
  model with canonical candidates and request pricing in one read path; the
  old `RoutingWorkspaceProjectionCandidate` compatibility chain is deleted.

## Explicit compatibility boundary

- `channel_monitor_target_results.health_writeback_*` is physically removed by
  migration 0026 and absent from the portable catalog.
- `channel_monitors.health_writeback_mode` is renamed to
  `health_policy_mode`; this is monitor configuration, not route health truth.
- `station_endpoint_health` and `station_key_health` are dropped by migration
  0026 and replaced by projection-owned snapshots rebuilt from observations.
- `stations.status` and `station_keys.status` remain user-facing asset state;
  routing reads projected observations/quality axes instead.
- Reserved domain variants and IPC descriptor types remain explicitly tracked
  by the dead-code policy; no blanket or local `allow(dead_code)` suppression is
  used.

## Remaining release work

- Station-key limits and collected Sub2API station-account limits now come from
  the execution target read model; the global runtime limit is no longer copied
  into those scopes. Provider-account identity remains an explicit V1
  `NotApplicable` boundary until the canonical provider-account key exists.

Local qualification commands pass. Provider/live-monitoring and release-machine
soak evidence remain separate operational gates requiring real credentials and
controlled environments; provider-account identity remains a V1 boundary.
