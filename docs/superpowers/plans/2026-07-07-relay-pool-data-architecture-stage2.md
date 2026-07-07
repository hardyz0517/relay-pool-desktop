# Relay Pool 数据架构 Stage 2 计划：Query Services

日期：2026-07-07

## 入口

执行 Stage 2 前必须先读：

1. `docs/superpowers/specs/2026-07-07-relay-pool-data-architecture-master-spec.md`
2. `docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-master-plan.md`
3. `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`
4. `docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage1-closeout.md`

## 目标

把页面中的重复加载编排逐步收敛到 query services。Stage 2 只搬运 raw facts 和页面所需的加载 bundle，不定义业务真相。

允许：

- 把多个 API 调用组合为一个 query service。
- 返回 projection-ready raw facts bundle。
- 保留页面原有 loading、toast、局部失败策略。
- 为 query service shape 添加 focused tests。

禁止：

- 不实现分组 identity。
- 不实现倍率 fallback。
- 不实现余额优先级。
- 不实现 runtime route 决策。
- 不删除、合并、重命名字段。
- 不把 compatibility cache 解释为 canonical fact。

## 当前 drift 风险

Stage 1 收口时主 checkout 有未提交 dirty 改动，且包含：

- `src/features/pricing/PricingPage.tsx`
- `src/features/pricing/pricingComparisonViewModel.ts`
- `src/features/stations/components/StationDetailContent.tsx`
- `src/features/stations/stationDetailViewModels.ts`
- `src/lib/types/groupFacts.ts`

因此 Stage 2 第一轮不要优先迁移 pricing / station detail / group facts 高重叠路径，除非主 checkout 对应改动已经提交并完成 merge。

## 第一批低重叠切片

### Slice 2A：Dashboard raw facts query service

新增：

- `src/lib/queries/dashboardQueries.ts`
- `scripts/dashboard-query-service.test.mjs`

候选函数：

- `loadDashboardWorkspace()`

返回 raw facts：

- `settings`
- `proxyStatus`
- `changeEvents`
- `balances`
- `requestLogs`
- `keyPoolItems`

边界：

- Query service 不调用 `summarizeDashboardBalances`。
- Query service 不计算 request metrics。
- Query service 不决定 health tone。
- Query service 不主动读取完整 `localAccessKey`；Dashboard 只在用户点击复制时沿用既有 `getLocalAccessKey()` 行为。
- Dashboard 原页面继续负责 view-model 或已有本地展示计算。

验证：

```powershell
node scripts/dashboard-query-service.test.mjs
node scripts/dashboard-local-route-start.test.mjs
node scripts/dashboard-balance-refresh.test.mjs
node scripts/test-dashboard-balance-summary.mjs
pnpm.cmd build
```

### Slice 2B：Change center raw facts query service

仅当 2A 完成且主 checkout 未产生新的高风险 drift 时执行。

新增：

- `src/lib/queries/changeQueries.ts`
- `scripts/change-query-service.test.mjs`

候选函数：

- `loadChangeCenterWorkspace()`

边界：

- Query service 只加载 `changeEvents`。
- 已读、未读、风险计数仍由 `changeEventViewModels.ts` 负责。

验证：

```powershell
node scripts/change-query-service.test.mjs
node scripts/change-center-mark-read.test.mjs
node scripts/change-center-collector-task-label.test.mjs
pnpm.cmd build
```

### Slice 2C：Request logs raw facts query service

仅当主 checkout 仍未提交高重叠 pricing / station / group facts 改动时，作为低重叠切片执行。

新增：

- `src/lib/queries/logQueries.ts`
- `scripts/log-query-service.test.mjs`

候选函数：

- `loadRequestLogWorkspace()`

边界：

- Query service 只加载 `requestLogs` 和 `keyPoolItems`。
- 日志过滤、格式化、成本展示、候选拒绝解析仍由 `LogsPage.tsx` 负责。
- 清空日志仍由页面动作调用 `clearRequestLogs()`，不得放进 query service。

验证：

```powershell
node scripts/log-query-service.test.mjs
node scripts/query-services-boundary.test.mjs
node scripts/channel-monitor-usage-request-log.test.mjs
pnpm.cmd build
```

### Slice 2D：Routing raw facts query service

仅当主 checkout 仍未提交高重叠 pricing / station / group facts 改动时，作为低重叠切片执行。

新增：
- `src/lib/queries/routingQueries.ts`
- `scripts/routing-query-service.test.mjs`

候选函数：

- `loadRoutingWorkspace()`

返回 raw facts：
- `settings`
- `modelAliases`

边界：
- Query service 只加载 `getSettings()` 和 `listModelAliases()`。
- 路由模拟、候选过滤、策略变更、模型映射增删改仍留在页面/API 动作层。
- Query service 不得调用 `simulateRoute()`。
- Query service 不得调用 `updateSettings()`、`upsertModelAlias()`、`deleteModelAlias()` 等写动作。
- 不接触 runtime route decision，也不解释模型映射业务语义。

验证：
```powershell
node scripts/routing-query-service.test.mjs
node scripts/query-services-boundary.test.mjs
node scripts/delete-confirmation-dialogs.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
pnpm.cmd build
```

### Slice 2E：Channels raw facts query service

仅当主 checkout 仍未提交高重叠 pricing / station detail / group facts 改动时，作为低重叠切片执行。

新增：
- `src/lib/queries/channelQueries.ts`
- `scripts/channel-query-service.test.mjs`

候选函数：

- `loadChannelMonitoringWorkspace()`
- `loadChannelStatusWorkspace()`

返回 raw facts：
- `monitorSummaries`
- `stations`
- `keyPoolItems`
- `templates`
- `requestLogs`
- `stationKeyHealth`

边界：
- Query service 只加载 channel tabs 初始刷新需要的 raw facts。
- Channel Monitoring 的新建、更新、删除、立即运行等写动作仍留在页面/API 动作层。
- Channel Status 的日志窗口过滤、健康卡片构建、排序和拖拽状态仍留在页面/view-model 层。
- Query service 不得调用 `runChannelMonitorNow()`、`createChannelMonitor()`、`updateChannelMonitor()`、`deleteChannelMonitor()`。
- Query service 不得调用 `filterLogsByWindow()`、`buildChannels()`、`orderChannelsBySavedOrder()` 等展示逻辑。
- 不解释 key 分组、健康状态或 runtime route decision。

验证：
```powershell
node scripts/channel-query-service.test.mjs
node scripts/query-services-boundary.test.mjs
node scripts/shared-capabilities-contract.test.mjs
node scripts/channel-monitoring-layout.test.mjs
node scripts/channel-status-drag-transform.test.mjs
node scripts/channel-status-card-layout.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/shared-utils-dedup.test.mjs
node scripts/delete-confirmation-dialogs.test.mjs
node scripts/channel-monitor-usage-request-log.test.mjs
pnpm.cmd build
```

## 后续切片需等待或重新 intake

以下切片与主 checkout dirty 路径重叠，执行前必须重新 drift intake：

- pricing query service
- station detail query service
- station asset query service
- key pool query service
- provider edit query service

如果主 checkout 相关改动仍未提交，只能记录未接入，不得声称已经合并主线字段或行为。

## 完成标准

- 每个 query service 都有 focused shape/source guard。
- `scripts/query-services-boundary.test.mjs` 必须随着新增 query service 更新显式 inventory，并防止 query service 调用 feature view-model、projection、secret 读取或写动作。
- 每迁移一个页面消费者，都保留原 loading/toast/partial failure 行为。
- `scripts/data-architecture-field-ownership.test.mjs` 继续通过。
- `pnpm.cmd build` 通过。
- Stage 2 收口时记录新的 drift intake 和未接入主 checkout dirty paths。
