# Relay Pool 数据架构 Stage 2 收口审计

日期：2026-07-07

## 范围

Stage 2 的目标是把低重叠页面中的重复 raw facts 加载编排收敛到 query services，同时不移动分组 identity、倍率 fallback、余额优先级、runtime route decision、secret 读取或写动作。

本阶段已完成：

- Dashboard raw facts query service：`src/lib/queries/dashboardQueries.ts`
- Change Center raw facts query service：`src/lib/queries/changeQueries.ts`
- Request Logs raw facts query service：`src/lib/queries/logQueries.ts`
- Routing raw facts query service：`src/lib/queries/routingQueries.ts`
- Channels raw facts query service：`src/lib/queries/channelQueries.ts`
- Query service boundary guard：`scripts/query-services-boundary.test.mjs`

## 已提交切片

- `f605115 refactor: add dashboard query service`
- `c081e65 refactor: add change center query service`
- `e457101 test: guard query service boundaries`
- `dbc291f refactor: add request log query service`
- `3721802 refactor: add routing query service`
- `6ad29db refactor: add channel query service`
- `6b92d4c docs: audit remaining stage2 query candidates`

## 边界确认

Query services 只负责加载编排和 raw facts 搬运：

- 不导入 `@/features/*`。
- 不导入 `@/lib/projections/*`。
- 不调用 dashboard/change/log/routing/channel 的 view-model 或展示构建函数。
- 不调用 `getLocalAccessKey()`。
- 不调用 `markChangeEventRead()`、`clearChangeEvents()`、`clearRequestLogs()`。
- 不调用 `simulateRoute()`、`updateSettings()`、`upsertModelAlias()`、`deleteModelAlias()`。
- 不调用 `runChannelMonitorNow()`、`createChannelMonitor()`、`updateChannelMonitor()`、`deleteChannelMonitor()`。
- 不实现分组 identity、倍率 fallback、余额优先级或 runtime route decision。

## 未迁移范围

以下 query-service 候选暂不迁移：

- `PricingPage` / pricing query service
- `StationDetailPage` / station detail query service
- `StationsPage` / station asset query service
- `KeyPoolPage` / key pool query service
- `AddProviderPage` / provider edit query service
- `CollectorsPage`
- `SettingsPage`

原因：

- pricing / station / key / provider 路径与主 checkout 的未提交 dirty 改动高重叠。
- collectors 路径与主 checkout 的 Rust collector 未提交 dirty 改动高重叠。
- settings 初始加载夹带 secret migration / scan 状态，不适合作为本轮低风险 raw facts query service。
- `changeEventViewModels.ts` 中的 `Promise.all` 是已读写动作批处理，不属于 Stage 2 初始 raw facts 加载迁移目标。

## Drift intake

工作树 closeout 前 HEAD：

- `6b92d4c`

主 checkout HEAD：

- `7e7d567a42dc4139d8876ca8f50f97b8cbc0644a`

主 checkout 仍有未提交 dirty 改动，未接入本工作树：

- `scripts/change-center-mark-read.test.mjs`
- `scripts/pricing-comparison-view-model.test.mjs`
- `src-tauri/src/services/collectors/adapters/sub2api.rs`
- `src-tauri/src/services/collectors/apply.rs`
- `src/components/shell/AppShell.tsx`
- `src/features/pricing/PricingPage.tsx`
- `src/features/pricing/pricingComparisonViewModel.ts`
- `src/features/stations/components/StationDetailContent.tsx`
- `src/features/stations/stationDetailViewModels.ts`
- `src/lib/types/groupFacts.ts`
- `scripts/station-detail-group-source.test.mjs`
- `scripts/station-group-visual-meta.test.mjs`
- `src/features/stations/components/Sub2ApiPlatformIcon.tsx`
- `src/features/stations/groupVisualMeta.ts`

结论：

- Stage 2 的低重叠 query-service 切片可以收口。
- Stage 2 剩余高重叠 query-service 迁移不能在当前 dirty 状态下继续推进。
- Stage 3 当前分组投影会直接触碰 `groupFacts` / pricing / station detail 语义，必须等待主 checkout dirty 改动提交后 merge，或由用户明确批准 patch 接入。

## Fresh verification

本次收口前已重新运行：

```powershell
node scripts/query-services-boundary.test.mjs
node scripts/dashboard-query-service.test.mjs
node scripts/change-query-service.test.mjs
node scripts/log-query-service.test.mjs
node scripts/routing-query-service.test.mjs
node scripts/channel-query-service.test.mjs
node scripts/dashboard-local-route-start.test.mjs
node scripts/dashboard-balance-refresh.test.mjs
node scripts/test-dashboard-balance-summary.mjs
node scripts/change-center-mark-read.test.mjs
node scripts/change-center-collector-task-label.test.mjs
node scripts/channel-monitor-usage-request-log.test.mjs
node scripts/shared-capabilities-contract.test.mjs
node scripts/channel-monitoring-layout.test.mjs
node scripts/channel-status-drag-transform.test.mjs
node scripts/channel-status-card-layout.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/shared-utils-dedup.test.mjs
pnpm.cmd build
```

结果：

- 全部 exit 0。
- `pnpm.cmd build` 仅保留既有 Vite chunk-size warning。
- 本阶段未修改 Rust 生产代码，未运行 `cargo check`。

## 下一步入口

继续升级前优先做其中之一：

1. 等主 checkout dirty 改动提交后，在本工作树 merge 主线提交，重新运行 Stage 0/1/2 guard。
2. 若用户明确批准，把指定 dirty 文件以 patch 形式接入本工作树，并在字段归属清单中登记任何新增字段。
3. 接入后再启动 Stage 3：`src/lib/projections/groupFacts.ts`，先写纯函数测试覆盖 identity fallback、倍率 fallback、多真实分组保留、missing 不复活。
