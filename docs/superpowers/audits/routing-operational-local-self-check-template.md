# Routing Operational Task 27 Local Self-check Template

Status: template only; do not commit filled runtime results
Owner task: Task 27

This checklist records the development-period reset/reimport/reconfigure proof for the current dev binary. It is not a release gate, signed installer checklist, install/upgrade matrix, or old-binary rollback proof.

Runtime output belongs under ignored `output/routing-operational/qualification/local-self-check/` or CI artifacts.

## Source snapshot

- Source revision:
- Worktree clean before run: yes/no
- Worktree clean after run: yes/no
- Operator:
- Machine/OS:
- Rust/Node/pnpm versions:

## Deterministic local command

- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-local-self-check.ps1`

The runner uses existing Rust/contract checks for:

- known-schema legacy fixture import into the current dev schema;
- interrupted upgrade recovery planning and fail-closed unsafe observations;
- fresh generation-two data store config;
- request-log URL sanitizer interruption/resume/startup readiness;
- startup lifecycle reconciliation before proxy admission;
- configured profile routing fields, model alias and catalog decision/cost stores;
- local routing redaction boundaries.

## Recovery boundary

- Supported development recovery: reset local data, reimport config, or reconfigure with the current dev binary.
- Unsupported in this phase: old binary rollback, signed installer recovery, install/upgrade matrix, automatic updater rollback, or partial feature-flag fallback.
- If writer unhealthy or a cutover blocker is found, stop admission and use reset/reimport/reconfigure. Do not mix old and new production owners.

## Manual / authorization-gated observations

- Real OpenAI-compatible client smoke:
- Real provider semantic fixture:
- CCSwitch fixed local entry:
- Windows sleep/resume:
- UI timeline versus SQLite journal/decision/health/cost:
- Canary/redaction notes:
