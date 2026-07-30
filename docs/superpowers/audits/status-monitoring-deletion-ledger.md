# 状态监控旧实现删除台账

状态：Task 18 本地资格与文档收尾中
日期：2026-07-29
原则：production 不保留 V1/V2 runtime selector，不 dual-write；旧 `channel_monitor_runs` 只允许一个发布观察周期的只读兼容与迁移证据。

| 旧 authority | 当前状态 | 当前消费者 | 保留到 | 删除/替换任务 | 删除条件 |
|---|---|---|---|---:|---|
| `src-tauri/src/services/channel_monitors/mod.rs` legacy runner | deleted/replaced by `src-tauri/src/services/monitoring/runner.rs` | none | deleted | 16 | production composition only builds V2 runner/orchestrator/scheduler/read model |
| `src-tauri/src/services/channel_monitors/probe.rs` status-only probe | deleted | none | deleted | 16 | provider adapters reject fake-200/status-only success |
| `CompletedMonitorProbe` | deleted | none | deleted | 16 | old-symbol grep has no production/test/script hits |
| `CreateChannelMonitorRunInput` | deleted | none | deleted | 16 | V2 write model uses execution/target/attempt rows |
| `ChannelMonitorRunnerPort` | deleted | none | deleted | 16 | no runtime selector or V1 runner port remains |
| `ACTIVE_MONITOR_RUNS` static guard | deleted | none | deleted | 16 | V2 runtime single-flight and live execution cancellation token registry own concurrency/cancel |
| 30s status-monitor full-table polling | deleted | none | deleted | 16 | runner sleeps to nearest persisted `next_due_at_ms` |
| `record_probe_outcome` | deleted | none | deleted | 16 | V2 commit path persists buffered executions through `MonitoringExecutionCommitter` |
| `insert_run_and_advance_monitor` | deleted | none | deleted | 16 | schedule advancement occurs during V2 execution finalization |
| request-log monitor observation write | deleted | none | deleted | 16 | monitor health observations flow through `HealthTransitionService` |
| monitor request pricing evidence structs | deleted | none | deleted | 16/17 | old monitor request-log cost write path no longer exists |
| `buildRecentOutcomes` / `healthToRecentOutcomes` UI trend synthesis | deleted | none | deleted | 15/16 | status trends come from backend buckets/target results |
| `channel_monitor_runs` production write authority | deleted | none | one release observation cycle as read-only compatibility only | 18 follow-up | no production writer remains; remove table/reader after observation cycle |
| `ChannelMonitorRun` DTO and `list_channel_monitor_runs` IPC | read-only compatibility | old monitor history view only | one release observation cycle | 18 follow-up | remove with `docs/superpowers/plans/2026-07-29-status-monitoring-legacy-table-removal.md` |
| `MonitoringStore::summary_runs` and legacy run helpers | read-only compatibility | legacy reader only | one release observation cycle | 18 follow-up | V2 execution/target/attempt history fully replaces support diagnostics |

## Verification

- `rg -n "RUNNER_POLL_INTERVAL|ACTIVE_MONITOR_RUNS|ChannelMonitorRunnerPort|CompletedMonitorProbe|record_probe_outcome|insert_run_and_advance_monitor|healthToRecentOutcomes|buildRecentOutcomes|OrchestratedChannelMonitorRunnerAdapter|insert_completed_monitor_observation|CompletedMonitorRequestWrite|MonitorProbeUsageEvidence|MonitorRequestPricingEvidence|CompletedMonitorRequestEvidence|estimate_monitor_request_cost" src-tauri/src src-tauri/tests src scripts` returns no hits.
- `node scripts/monitoring-architecture.test.mjs` passes.
- `cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_write_path -- --nocapture` asserts V2 commits do not write `channel_monitor_runs`.
- `cargo test --manifest-path src-tauri/Cargo.toml --test monitoring_faults -- --nocapture` covers rollback, replay and hard-restart boundaries.

## Follow-up

The explicit follow-up to delete the legacy table and compatibility reader is:

`docs/superpowers/plans/2026-07-29-status-monitoring-legacy-table-removal.md`
