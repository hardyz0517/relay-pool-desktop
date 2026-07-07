# Relay Pool 数据架构 Stage 2 进度审计

日期：2026-07-07

## 当前范围

Stage 2 目标是把页面里的重复加载编排收敛到 query services。Query service 只搬运 raw facts，不定义业务真相。

已完成低重叠切片：

- Slice 2A：Dashboard raw facts query service
- Slice 2B：Change Center raw facts query service
- Slice 2C：Request Logs raw facts query service
- Slice 2D：Routing raw facts query service
- Slice 2E：Channels raw facts query service
- Stage 2 query service boundary guard

## 已提交切片

- `f605115 refactor: add dashboard query service`
- `c081e65 refactor: add change center query service`
- `e457101 test: guard query service boundaries`
- `dbc291f refactor: add request log query service`
- `3721802 refactor: add routing query service`
- `6ad29db refactor: add channel query service`

## 新增 query services

### `src/lib/queries/dashboardQueries.ts`

导出：

- `DashboardWorkspace`
- `loadDashboardWorkspace()`

职责：

- 加载 `proxyStatus`
- 加载 `requestLogs`
- 加载 `keyPoolItems`
- 加载 `balanceSnapshots`
- 加载 `settings`
- 加载 `changeEvents`

明确不做：

- 不调用 `summarizeDashboardBalances`
- 不计算 request metrics
- 不决定 health tone
- 不主动读取完整 `localAccessKey`

### `src/lib/queries/changeQueries.ts`

导出：

- `ChangeCenterWorkspace`
- `loadChangeCenterWorkspace()`

职责：

- 加载 `changeEvents`
- 加载 `stations`

明确不做：

- 不过滤变更事件
- 不分页
- 不计算未读/风险数量
- 不处理已读写回
- 不构建事件展示文案

### `src/lib/queries/logQueries.ts`

导出：

- `RequestLogWorkspace`
- `loadRequestLogWorkspace()`

职责：

- 加载 `requestLogs`
- 加载 `keyPoolItems`

明确不做：

- 不过滤日志
- 不格式化日志
- 不解析 rejected candidates
- 不计算成本展示
- 不清空日志

### `src/lib/queries/routingQueries.ts`

导出：

- `RoutingWorkspace`
- `loadRoutingWorkspace()`

职责：

- 加载 `settings`
- 加载 `modelAliases`

明确不做：

- 不模拟路由
- 不修改默认策略
- 不创建、更新或删除模型映射
- 不解释 runtime route decision
- 不接触 secret 或 group binding

### `src/lib/queries/channelQueries.ts`

导出：

- `ChannelMonitoringWorkspace`
- `ChannelStatusWorkspace`
- `loadChannelMonitoringWorkspace()`
- `loadChannelStatusWorkspace()`

职责：

- 加载 `monitorSummaries`
- 加载 `stations`
- 加载 `keyPoolItems`
- 加载 `templates`
- 加载 `requestLogs`
- 加载 `stationKeyHealth`

明确不做：

- 不创建、更新、删除或立即运行 channel monitor
- 不过滤日志时间窗口
- 不构建 channel health cards
- 不处理拖拽排序
- 不解释 key 分组、健康状态或 runtime route decision

## 验证

Slice 2A / 2B / 2C / 2D / 2E 已运行：

```powershell
node scripts/query-services-boundary.test.mjs
node scripts/dashboard-query-service.test.mjs
node scripts/dashboard-local-route-start.test.mjs
node scripts/dashboard-balance-refresh.test.mjs
node scripts/test-dashboard-balance-summary.mjs
node scripts/change-query-service.test.mjs
node scripts/change-center-mark-read.test.mjs
node scripts/change-center-collector-task-label.test.mjs
node scripts/log-query-service.test.mjs
node scripts/channel-monitor-usage-request-log.test.mjs
node scripts/routing-query-service.test.mjs
node scripts/delete-confirmation-dialogs.test.mjs
node scripts/channel-query-service.test.mjs
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
- `pnpm.cmd build` 仅保留既有 Vite chunk size warning。

## Drift intake

工作树 HEAD：

- `6ad29db`

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

- Dashboard、Change Center、Request Logs、Routing 和 Channels 的 query service 切片未接入主 checkout dirty 改动。
- Query service boundary guard 已显式锁定当前 query service inventory：`changeQueries.ts`、`channelQueries.ts`、`dashboardQueries.ts`、`logQueries.ts`、`routingQueries.ts`。
- 后续 pricing / station detail / station asset / key pool / provider edit query service 都可能和主 checkout 的价格、站点、分组事实改动重叠。
- 继续这些切片前必须重新 drift intake。
- 如果 dirty 改动仍未提交，应等待主 checkout 提交，或由用户明确指定 patch 接入；不得通过 `git merge master` 声称接入未提交改动。

## 下一步建议

1. 若主 checkout 的价格/站点/分组改动已经提交，先 merge 主线提交并重新跑 Stage 0/1/2 guard。
2. 若主 checkout 仍 dirty，暂停高重叠 query-service 迁移，继续只做不触碰这些路径的 docs/test 审计。
3. 进入 pricing / station / key / provider query service 前，先补对应 query service shape test，再迁单页消费者。

## 剩余 query-service 候选审计

当前剩余页面级加载编排主要集中在：

- `src/features/collectors/CollectorsPage.tsx`
- `src/features/key-pool/KeyPoolPage.tsx`
- `src/features/stations/AddProviderPage.tsx`
- `src/features/stations/StationsPage.tsx`
- `src/features/stations/StationDetailPage.tsx`
- `src/features/pricing/PricingPage.tsx`
- `src/features/settings/SettingsPage.tsx`
- `src/features/changes/changeEventViewModels.ts`

处理结论：

- `PricingPage`、`StationDetailPage`、`StationsPage`、`AddProviderPage`、`KeyPoolPage` 与主 checkout 的 pricing / station detail / group facts dirty 改动高重叠，暂不迁移。
- `CollectorsPage` 与主 checkout 的 Rust collector dirty 改动高重叠，暂不迁移。
- `SettingsPage` 的加载包含 secret migration / scan 状态，不作为当前低风险 raw facts query service 切片。
- `changeEventViewModels.ts` 中的 `Promise.all` 是已读写动作批处理，不属于 Stage 2 raw facts 初始加载迁移目标。
- 在主 checkout dirty 改动提交、或用户明确批准 patch 接入前，Stage 2 不应继续迁移上述高重叠路径。
