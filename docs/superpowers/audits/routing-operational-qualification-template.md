# Routing Operational Task 26 Development Self-check Template

Status: template only; do not commit filled runtime results
Owner task: Task 26

This file defines the human-readable checklist for a development-period local self-check run. It is not a release gate, signed installer checklist, install/upgrade matrix, or old-binary rollback proof. Runtime output belongs under ignored `output/routing-operational/qualification/` or CI artifacts.

## Source snapshot

- Source revision:
- Worktree clean before run: yes/no
- Worktree clean after run: yes/no
- Operator:
- Machine/OS:
- Rust/Node/pnpm versions:

## Required development commands

- `pnpm.cmd architecture:fixtures`
- `pnpm.cmd architecture:typescript`
- `pnpm.cmd architecture:commands`
- `pnpm.cmd architecture:security`
- `pnpm.cmd architecture:artifacts`
- `pnpm.cmd test:contracts`
- `pnpm.cmd build`
- `cargo check --locked --manifest-path src-tauri/Cargo.toml`
- `pnpm.cmd architecture:scale-baseline`
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-soak.ps1 -DurationMinutes 60`
- `node scripts/routing-operational-qualification.mjs`

Optional aggregate check, only when the local Node/toolchain lifecycle ledger matches:

- `pnpm.cmd verify:full`

## Evidence boundaries

- Do not commit local databases, WAL/SHM files, backups, raw logs, screenshots, API keys, cookies, authorization headers, full upstream URLs, provider payloads, prompts or responses.
- The self-check validator checks deterministic loopback artifacts and optional scale-baseline artifacts. It records the source revision for debugging, but does not freeze a release candidate.
- The 60-minute soak is deterministic loopback only and must not consume real provider quota.
- Development recovery is reset/reimport/reconfigure with the current dev binary; this template does not prove old binary rollback.

## Manual observations

- Windows sleep/resume:
- Wall-clock rollback / monotonic deadline:
- Shutdown counters/gauges:
- Canary scan notes:
