# Status monitoring refactor baseline audit

Status: Task 0 baseline completed; implementation had not started at the time of this audit.
Date: 2026-07-29
Branch: `codex/status-monitoring-refactor`
Workspace: redacted local worktree
Baseline commit: `87280a4e1ac836870cbb1b69b7d5d4cfb613cad4` (`feat: add reliable remote key deletion`)

## Baseline result

The status monitoring refactor started from a green baseline. The audit recorded that the existing command, build, contract, and Rust checks passed before implementation work began. Local-only paths, screenshots, temporary Relay Pulse inspection directories, and machine-specific checkout locations are intentionally redacted from this tracked artifact.

## Legacy symbols observed

- `ChannelMonitorRun`: legacy run-shaped read/write model, later downgraded behind the monitoring v2 model.
- `CompletedMonitorProbe`: old probe persistence shape, replaced by execution/target/attempt facts.
- `run_monitor_probe`: old protocol success boundary, replaced by adapter/profile contracts.
- `RUNNER_POLL_INTERVAL`: old fixed polling scheduler, replaced by due-time scheduling.
- `ACTIVE_MONITOR_RUNS`: old in-memory guard, replaced by durable execution single-flight semantics.
- `record_probe_outcome`: old single transaction write path, replaced by the monitoring recorder.
- `channel_monitor_runs`: legacy table retained for migration/backfill compatibility.
- `buildRecentOutcomes` and `healthToRecentOutcomes`: frontend-derived recent status helpers replaced by backend read models.

## Risks captured before the refactor

- Protocol success semantics were too weak.
- Execution facts were incomplete and lacked execution/target/attempt layering.
- Scheduling used fixed polling rather than nearest-due planning.
- Health writeback paths were scattered.
- Frontend read models derived status from mixed sources.
- Several monitoring concepts were still represented as loose strings.

## Current merge note

This file is historical audit evidence only. It must not be used as a current implementation source, and it must not contain local absolute paths, screenshots, tokens, logs, or machine-specific artifacts.
