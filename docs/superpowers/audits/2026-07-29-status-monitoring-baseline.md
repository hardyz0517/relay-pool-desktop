# 状态监控重构 Task 0 基线审计

状态：Task 0 completed baseline, implementation not started
日期：2026-07-29
分支：`codex/status-monitoring-refactor`
工作区：`<redacted local status-monitoring worktree>`
基线提交：`87280a4e1ac836870cbb1b69b7d5d4cfb613cad4` (`feat: add reliable remote key deletion`)

## 1. 工作区状态

Task 0 开始前 `git status --short --branch`：

```text
## codex/status-monitoring-refactor
?? docs/README.md
?? docs/proposals/
?? docs/superpowers/plans/2026-07-29-status-monitoring-refactor.md
```

当前 worktree 是从 `master` HEAD 新建的专用 worktree。上述未跟踪文件是本次监控重构 spec、实施计划和 docs 索引，不属于应用代码。

`pnpm build` 曾触发 `src-tauri/gen/schemas/{desktop,windows}-schema.json` 的 CRLF/LF 噪音；检查确认无内容 diff 后已恢复。Relay Pulse 指定 commit 曾临时检出到 `.codex-tmp/relay-pulse-c625370` 做只读审计，该目录被 Git 忽略，不进入交付范围。

最近提交：

```text
87280a4 feat: add reliable remote key deletion
bb65a48 feat: add reusable provider login profiles
4648d30 fix: preserve group rate change context
9d34149 fix: preserve generated binding bytes across checkouts
1a8f489 merge: architecture scale upgrade
```

Migration 目录当前最高编号为 `0008_legacy_parity.sql`。本 worktree 没有 `0009_provider_drafts.sql`；后续 Task 6 仍必须按执行时目录枚举选择下一编号，不能硬编码。

## 2. 基线命令结果

| 命令 | 结果 | 耗时 | 备注 |
|---|---:|---:|---|
| `cargo test --manifest-path src-tauri/Cargo.toml --lib channel_monitor -- --nocapture` | 40 passed, 0 failed | 76.60s | 首次编译占主要耗时；产生既有 dead_code/linker warnings |
| `pnpm.cmd test -- src/features/channels src/lib/api/channelMonitors.test.ts src/lib/queries/channelQueries.test.ts` | 54 files passed, 181 tests passed | 15.85s | pnpm/Vitest 参数实际运行全量前端测试；全部通过 |
| `pnpm.cmd test:contracts` | passed | 62.05s | updater、sqlx offline、data-store、proxy、request lifecycle contracts 全绿；Node DEP0190 warning 为既有脚本 warning |
| `pnpm.cmd build` | passed | 8.09s | theme audit、`tsc --noEmit`、Vite build 通过；Vite chunk size warning 为现有体积提示 |
| `cargo check --manifest-path src-tauri/Cargo.toml` | passed | 49.51s | 21 个既有 warning；无编译失败 |

Task 0 结论：当前基线可复现且为绿基线。后续 RED 测试如果失败，应归因于新增监控 V2 能力尚未实现，而不是当前工程不可构建。

## 3. 旧生产符号与消费者

| 旧符号/表 | 当前 owner | 生产消费者 | 问题 | 目标处理 |
|---|---|---|---|---|
| `ChannelMonitorRun` | `src-tauri/src/models/channel_monitors.rs` | monitoring service/store、channel status query、DTO、frontend bridge、ChannelMonitoringTab、ChannelStatusTab | 单条 run 混合 station-wide/key 语义，没有 execution/target/attempt 分层 | Task 6 backfill 后降级为 legacy read-only；Task 17 删除旧生产 authority |
| `CompletedMonitorProbe` | `src-tauri/src/models/channel_monitors.rs` | `services/channel_monitors`, `application/monitoring.rs` | probe 与 persistence write 紧耦合；一条 probe 直接推进 monitor schedule | Task 8/9 recorder 用 target result 取代 |
| `run_monitor_probe` | `src-tauri/src/services/channel_monitors/probe.rs` | background runner、probe unit tests | `status_code < 400` 即 `ok=true`，协议语义和 validator 缺失 | Task 2-4 adapter contract 取代 |
| `RUNNER_POLL_INTERVAL` | `src-tauri/src/services/channel_monitors/mod.rs` | `ChannelMonitorRunnerState::start_v2` | 固定 30 秒轮询，非 nearest-due | Task 11 scheduler 删除 |
| `ACTIVE_MONITOR_RUNS` | `src-tauri/src/services/channel_monitors/mod.rs` | guarded monitor run | 只有 monitor 级 HashSet guard，没有 durable execution single-flight | Task 8/11 用 orchestrator/scheduler permits 取代 |
| `record_probe_outcome` | `src-tauri/src/application/monitoring.rs` and runner port | manual/background write path | 单事务插入 run、推进 schedule、可选写 request log；没有 attempt/target/execution 三段事务 | Task 7/9 recorder 取代 |
| `channel_monitor_runs` | `src-tauri/src/persistence/migrations/0007_pricing_monitoring.sql` | monitoring store、status query、legacy import、DTO/read commands | raw run 直接做 availability 和 timeline 输入；无 retention 上限 | Task 6 backfill 到 V2 tables；Task 13 rollups/read model 不再消费 |
| `buildRecentOutcomes` | `src/features/channels/channelStatusViewModel.ts` | `ChannelStatusTab.tsx` | 前端从 request logs 推趋势，监控事实与真实流量混合 | Task 14/15 改为后端 bucket/read model |
| `healthToRecentOutcomes` | `src/features/channels/channelStatusViewModel.ts` | `buildRecentOutcomes` fallback | 用健康累计数伪造最近 60 格，不是时间事实 | Task 14/15 删除 |

## 4. 当前实现风险

- 协议成功语义过弱：`src-tauri/src/services/channel_monitors/probe.rs` 的 `response_result` 仅用 HTTP `<400` 判定成功。空 JSON、HTML、错误 JSON、无内容 SSE 均可能被误判。
- 执行事实不完整：station scope 会为每个 key 写多条 run，但没有父 execution 和 per-key terminal result，最后写入的 run 容易成为 UI/latest 的事实来源。
- 调度模型偏机械：`RUNNER_POLL_INTERVAL = 30s` 轮询 due monitors，不能表达 nearest-due、持久预算、全局/目标许可、schedule lag、deadline。
- 健康写入分散：monitor 通过 request-log observation 间接写 `station_key_health`；proxy/request log/routing 也有自己的写入路径。
- 前端 read model 不稳：`ChannelStatusQuery::load_workspace` 聚合 key pool、request logs、station health、monitor summaries；前端继续拼 recent outcomes，不是固定 bucket。
- 弱字符串广泛存在：status、target type、template endpoint kind、failure summary 等仍以字符串穿层传递。

## 5. Relay Pulse 概念映射

参考 commit：`c62537085f4202f6f1f28716f45c107303f2836f`，许可证：MIT。只做架构观察，不复制实现。

值得学习：

- `internal/scheduler/scheduler.go` 使用最小堆按最近到期任务唤醒，配置更新通过 wake channel 重建任务堆。
- 调度器维护 bounded semaphore、context cancel、wait group drain，生命周期比固定轮询更清晰。
- `internal/monitor/client.go` 按 provider/proxy 复用 HTTP client/transport，并明确冷启动口径、代理语义。
- `internal/monitor/probe.go` 有动态 challenge、重试、失败 body 截断、细分 sub-status、响应解压与日志摘要。
- `internal/storage/storage.go` 把 status/sub-status/counts/timepoint 建模为前后端共享事实。
- `frontend/src/components/StatusTable.tsx` 是横向密集状态表，包含过滤、排序、稳定列、状态点、可用率、最近检测和固定趋势格。
- `frontend/src/utils/heatmapAggregator.ts` 对移动/窄屏做趋势格聚合，并保留聚合来源和状态计数。

不照搬：

- 公网站点/多用户服务端部署、PostgreSQL 多实例锁、通知平台、赞助/公告/SEO。
- 深色视觉、品牌文案、公开站点 API 和前端整体页面结构。
- 任意未授权 CLI 身份字段、OAuth/device identity、账号 UUID 或大段系统 prompt。
- 以 provider URL/name 猜协议的松散 parser；本项目必须用显式 adapter/profile capability。

## 6. 用户横向 UI 截图观察

截图路径：`<redacted local clipboard screenshot>`

结构观察：

- 顶部工具栏包含多个下拉筛选、视图切换、刷新按钮和时间窗切换。
- 主体是高密度横向表格，列包括服务商、赞助者、服务、通道、当前状态、可用率、最后检测、历史趋势。
- 趋势列由固定宽度色块组成，适合快速扫最近 24h/7d/30d 状态。
- 状态色传递可用、波动、不可用、缺失；延迟作为最近检测的辅助信息。

本项目 UI 决策：学习横向信息架构和固定趋势格；保留 Relay Pool Desktop 的浅色、本地桌面工具、紧凑低饱和视觉，不复制深色主题、品牌、文字或源码。

## 7. Task 0 退出结论

Task 0 的基线、旧符号账本、边界 manifest 和协议来源已建立。应用代码尚未进入生产改造。下一步按计划进入 Task 1：建立纯 `models/monitoring` 领域模型与架构门禁，先写 RED 测试。
