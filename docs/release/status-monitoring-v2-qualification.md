# Status Monitoring V2 Qualification Note

Status Monitoring V2 replaces the legacy monitor-run implementation with a single execution model:

`MonitorExecution -> MonitorTargetResult -> ProbeAttempt`

The release-facing qualification entry is:

- `docs/superpowers/audits/status-monitoring-qualification.md`

Local deterministic qualification currently covers:

- V2-only write path and no production writes to `channel_monitor_runs`;
- typed provider/profile adapter contracts;
- semantic/protocol validation instead of HTTP-status-only success;
- manual and scheduled execution through the shared orchestrator/scheduler/runtime path;
- bounded nearest-due scheduling, single-flight, cancellation, and startup recovery;
- backend-owned recent/window/bucket read model for the horizontal status UI;
- transaction rollback and replay boundaries for attempts, target results, health observations, execution summaries, rollup dirty ranges, and probe budget reservation.
- large-fixture read-model performance: 500 workspace rows over 500,000 target results and 100,000 attempts passed at 134.93 ms workspace p95, below the 250 ms gate.
- deterministic 60-minute local monitoring soak passed with 985 iterations and 0 failures.
- release-build mixed-provider/stream/retry/fallback/missing 60-minute soak passed with 513 iterations and 0 failures.

Local deterministic and release-build local soak qualification have passed. Real provider verification, signing, and signed install/upgrade/sleep-resume remain explicit release gates and require the authorization/secrets described in the qualification document.
