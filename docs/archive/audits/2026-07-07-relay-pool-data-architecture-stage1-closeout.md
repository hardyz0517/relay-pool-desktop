# Relay Pool 数据架构 Stage 1 收口审计

日期：2026-07-07

## 范围

Stage 1 只做低风险工具去重，不改变业务语义，不迁移分组 identity，不迁移价格 projection，不改 UI 布局，不改 schema。

本阶段已完成：

- `readError` 收敛到 `src/lib/errors.ts`。
- 倍率格式化收敛到 `src/lib/formatters.ts` 的 `formatRate`。
- channel、日志、dashboard、价格、变更中心、站点、Key Pool 的 timestamp-like 时间解析收敛到 `src/lib/time.ts`。
- trailing-zero 小数裁剪收敛到 `src/lib/formatters.ts` 的 `formatTrimmedDecimal`。
- `scripts/shared-utils-dedup.test.mjs` 作为 Stage 1 source guard，防止已迁移重复工具回流。

## 已提交切片

- `34b4419 refactor: dedupe feature read error handling`
- `39f8d38 refactor: dedupe rate formatting helpers`
- `b5c328a refactor: share channel time parsing helpers`
- `d742937 refactor: share trimmed decimal formatter`
- `5b19107 refactor: reuse timestamp date parsing`
- `072bcda refactor: reuse remaining timestamp helpers`

## Drift intake

主 checkout HEAD：

- `7e7d567a42dc4139d8876ca8f50f97b8cbc0644a`

工作树 HEAD：

- `072bcdabd8616c45af89507cc5c756f6659df62b`

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

- 这些 dirty 改动不是可 merge 的事实，不能声称已接入。
- Dirty 路径与 Stage 2/3 的价格、站点详情、分组事实高度重叠。
- 进入 Stage 2 时应优先选择低重叠的 query-service 起点，或等待主 checkout 相关改动提交后再 merge。
- 若必须迁移价格、站点详情、分组事实相关消费者，先做新的 drift intake，并把主 checkout 的新增字段或行为登记到字段归属清单。

## 验证

本阶段收口时已运行：

```powershell
node scripts/shared-utils-dedup.test.mjs
node scripts/pricing-comparison-view-model.test.mjs
node scripts/add-provider-key-groups.test.mjs
node scripts/test-dashboard-balance-summary.mjs
node scripts/edit-key-page-flow.test.mjs
node scripts/shared-capabilities-contract.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
pnpm.cmd build
```

结果：

- 全部 exit 0。
- `pnpm.cmd build` 仅保留既有 Vite chunk size warning。

额外验证过的 Stage 1 相关脚本：

```powershell
node scripts/tests/channelMonitorViewModel.test.mjs
node scripts/channel-status-view-model.test.mjs
node scripts/channel-monitor-usage-request-log.test.mjs
node scripts/channel-status-card-layout.test.mjs
node scripts/channel-monitoring-layout.test.mjs
node scripts/channel-status-drag-transform.test.mjs
node scripts/change-center-mark-read.test.mjs
node scripts/change-center-collector-task-label.test.mjs
node scripts/dashboard-local-route-start.test.mjs
node scripts/dashboard-balance-refresh.test.mjs
node scripts/station-detail-header.test.mjs
node scripts/station-asset-loading-boundary.test.mjs
node scripts/station-asset-selection.test.mjs
node scripts/station-create-balance-refresh.test.mjs
node scripts/key-pool-monitor-toggle.test.mjs
```

## 已知非本阶段问题

- `node scripts/pricing-model-family.test.mjs` 在当前 HEAD 会失败，因为脚本引用不存在的旧路径 `src/features/pricing/rateSnapshotParser.ts`。
- 该文件在当前 HEAD 本来就不存在，不是 Stage 1 改动引入。
- 暂不把该旧脚本修复混入 Stage 1，以免扩大范围。

## 下一步建议

进入 Stage 2 Query Services 前：

1. 再次执行 drift intake。
2. 如果主 checkout 的价格、站点详情、分组事实改动已经提交，优先 merge 并重新跑 Stage 0/1 guard。
3. 如果主 checkout 仍有未提交 dirty 改动，Stage 2 先从低重叠查询服务开始，例如 dashboard 或 change center 的只读 raw facts bundle。
4. Query service 只搬运 raw facts，不定义分组 identity、倍率 fallback、余额优先级或 runtime route 决策。
