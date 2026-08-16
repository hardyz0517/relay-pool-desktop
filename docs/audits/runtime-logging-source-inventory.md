# Runtime Logging Source Inventory

状态：Implementation baseline；由运行日志升级计划维护。此文件只记录扫描证据，不授权任何生产输出。

日期：2026-08-15

扫描范围：`src-tauri/src` 生产 Rust、`src` 前端、`scripts` 架构/契约脚本。测试 fixture 和历史文档不自动视为生产例外，必须在删除台账中单独标注。

## 已确认的旧输出入口

| 来源 | 当前形式 | 目标 owner/event | 处置任务 | 状态 |
| --- | --- | --- | --- | --- |
| `src-tauri/src/lib.rs` | 启动、recovery、shutdown、tray | `observability::runtime` 固定事件与 bootstrap fallback | Task 4 | closed（architecture scan） |
| `src-tauri/src/background_tasks/exit.rs` | exit drain 状态 | shutdown runtime event/fixed fallback | Task 4 | closed（architecture scan） |
| `src-tauri/src/background_tasks/routing_projection_runner.rs` | projection failure | typed runtime event + stable error code | Task 5 | closed（architecture scan） |
| `src-tauri/src/application/routing.rs` | planning snapshot failure | application boundary event | Task 5 | closed（architecture scan） |
| `src-tauri/src/services/data_store/installation_lease.rs` | lease state | generic lease + runtime adapter | Task 3/4 | closed（architecture scan） |
| `src-tauri/src/services/proxy/lifecycle/writer.rs` | lifecycle/send state | typed persistence/proxy event | Task 5 | closed（architecture scan） |
| `src-tauri/src/services/proxy/startup_auto_start.rs` | proxy start-on-launch | proxy startup event | Task 5 | closed（architecture scan） |
| `src-tauri/src/services/station_collectors.rs` | collector state | collector owner adapter | Task 5 | closed（architecture scan） |
| `src-tauri/src/services/monitoring/runner.rs` | worker/query/runner state | monitoring runner typed event | Task 5 | closed（architecture scan） |
| `src-tauri/src/services/monitoring/maintenance.rs` | maintenance state | monitoring maintenance typed event | Task 5 | closed（architecture scan） |
| `src-tauri/src/commands/**` | 全部生产 command（bootstrap 专用初始化除外）经统一 runtime-context boundary；invalid context fail-closed | 统一 IPC boundary | Task 2 | closed（architecture scan + correlation/registry tests） |
| `src-tauri/src/observability/events.rs` | 已删除的 legacy scaffold | `observability::runtime::{event,subject,catalog}` | Task 1/8 | closed（source scan + contract test） |
| `src-tauri/src/observability/metrics.rs` | bounded routing classification ring（非 durable runtime event） | `services::proxy::routing_runtime` owner；diagnostics 不再复制该模型 | Task 1/4/8 | closed（producer inventory + routing tests） |
| `src-tauri/src/observability/diagnostics.rs` | 已删除的 parallel snapshot scaffold | `application::runtime_diagnostics` / `RuntimeLogService` reader | Task 1/4/7 | closed（source scan + command/support-bundle tests） |
| `src-tauri/src/observability/redaction.rs` | 已删除的 duplicate preview helper | `services::secrets::mask` canonical non-log redaction；runtime event API 无文本入口 | Task 1/8 | closed（source scan + security canary） |
| `src/app/ShellPageErrorBoundary.tsx` | UI boundary | `frontend.boundary.failed` | Task 6/7 | closed（Vitest + security scan） |

## Required re-scan

The inventory is not complete by assertion. Before Task 8, rerun:

```powershell
rg -n --glob '*.rs' 'println!|eprintln!|tracing::(error|warn|info|debug)!|error\s*=\s*\?error|error\s*=\s*%error' src-tauri/src
rg -n --glob '*.{ts,tsx}' 'console\.(error|warn|log)|JSON\.stringify\(.*error|stack' src
```

Every result must be either removed, mapped to a typed adapter, or listed as an exact fixed bootstrap/crash fallback. Unknown results fail the architecture gate.

## Frozen Runtime Parameters

这些参数是当前实现与测试的单一基线；修改任一项必须同时更新本表、相关 owner 测试、验收矩阵和设计评审记录：

| 参数 | 当前值 | Owner / 依据 | 变更要求 |
| --- | --- | --- | --- |
| segment 最大字节数 | 8 MiB | `runtime::sink::DEFAULT_MAX_SEGMENT_BYTES` | 重新评审 rotation、metadata 和 recovery 预算 |
| runtime-log 目录总容量 | 96 MiB | `runtime::retention::RetentionConfig::default` | 重新评审删除顺序与磁盘满降级 |
| 年龄保留窗口 | 14 天 | `RetentionConfig::default` | 仅 clock stable 时生效；重新评审时钟策略 |
| 单事件行上限 | 16 KiB | `runtime::event::MAX_SERIALIZED_EVENT_BYTES` / sink limit | 重新评审 schema 与 bundle 上限 |
| diagnostics page | 200 行 / 1 MiB | `runtime::reader::{DEFAULT_PAGE_LINES,DEFAULT_PAGE_BYTES}` | 重新评审 cursor、UI 和 support bundle 分页 |
| support bundle runtime events | 10 MiB / 10,000 events | `services::support_bundle` | 重新评审临时目录清理与导出耗时 |
| writer queue | 256；普通事件 224，保留 32 个 warn/error 槽位 | `runtime::service` | 重新评审丢弃优先级及 shutdown drain |
| partial recovery | 8 files / 8 MiB，总 segment 8 MiB | `runtime::recovery::RecoveryConfig::default` | 重新评审启动时延与 salvage 安全边界 |
| clock jump threshold | 5 分钟；lease backoff 100 ms 起、5 s 上限 | `runtime::clock` / `runtime::service` | 必须继续使用 monotonic deadline，并补 fake-clock 证据 |
| interaction admission | TTL 10 分钟；最多 128 active ids | `observability::runtime_context` | 重新评审 capability、跨 session 和并发串线风险 |

依赖决策：当前使用 Rust 标准库 `sync_channel`、`FileExt::try_lock`、`std::fs` 与既有 Tokio；本专项未新增日志 subscriber、远程 telemetry、归档 crate 或跨进程锁依赖。Windows rename/lock 行为由 runtime sink/lease focused tests 覆盖，真实 packaged Windows smoke 仍是未闭合证据。
