# Relay Pool 数据架构 Stage 5 进度审计

日期：2026-07-08

## 范围

Stage 5 迁移站点详情 group rows、站点资产 rate chips、站点余额展示到 shared current projections。页面视觉结构不变，不删除字段，不改 schema。

## Drift Intake

- 主 checkout 路径：`D:\Dev\Projects\relay-pool-desktop`
- 主 checkout HEAD：`ca0d9a7 fix: refine sub2api group displays`
- 主 checkout dirty paths：未观察到 tracked / untracked dirty path；`git status --short` 仍输出 `.pnpm-store` 缺失目录 warning。
- 工作树路径：`D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`
- 工作树分支：`codex/data-architecture-stage0`
- Stage 5 起点 HEAD：`bc8e9d1 docs: summarize data architecture stage4 progress`
- 当前工作树 HEAD：`7cba913 refactor: consume current facts in station assets`

## 已完成

- `47eebe6 docs: add data architecture stage5 plan`
- `22f5caa test: guard current station balance projection`
- `01b177d refactor: add current station balance projection`
- `52139e4 test: guard displayable group current facts`
- `3a533dd refactor: add displayable group current fact guard`
- `153ac7c test: guard station asset current projections`
- `1380073 test: align station detail with current group facts`
- `bf38761 refactor: consume current facts in station detail`
- `7cba913 refactor: consume current facts in station assets`
- `src/lib/projections/balanceFacts.ts`
- `buildCurrentStationBalanceFacts()`
- `currentStationBalanceFor()`
- `isDisplayableStationGroupCurrentFact()`
- `stationDetailViewModels.ts` 消费 current group facts 和 current balance facts。
- `stationAssetViewModels.ts` 消费 current group facts 和 current balance facts。

## 字段审计

- 本阶段未新增 schema 字段。
- `stations.balance_cny` 和 `stations.low_balance_threshold_cny` 仍是 compatibility cache，仅在没有 station-scope balance snapshot 时 fallback。
- `balance_snapshots.scope = station` 仍是当前站点余额优先证据；`station_key` snapshot 不覆盖站点级当前余额。
- `group_rate_records` 仍是 evidence/history，不直接复活 missing/disabled group。
- `station_group_bindings` 仍保留 canonical binding identity；展示层只通过 current projection 消费。
- 字段归属清单无需新增 `unknown pending audit` 行。

## 验证

已在 `7cba913` 后运行：

```powershell
node scripts/station-current-balance-projection.test.mjs
node scripts/group-facts-projection.test.mjs
node scripts/station-detail-group-source.test.mjs
node scripts/station-assets-current-projections.test.mjs
node scripts/station-asset-loading-boundary.test.mjs
node scripts/station-asset-selection.test.mjs
node scripts/test-dashboard-balance-summary.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
pnpm.cmd build
git status --short
```

结果：

- 上述 Node 脚本全部 exit 0。
- `pnpm.cmd build` exit 0，仅保留既有 Vite chunk-size warning。
- `git status --short` 在写入本审计前为 clean。

## 下一步

- 提交本审计后更新 rolling heartbeat。
- 进入 Stage 6：Key Pool 与 Add Provider 迁移。
- Stage 6 先做 drift intake，再创建/审核 Stage 6 计划；重点保护 selected `group_binding_id`、显式清除分组、远端 Key 创建的上游 group id 解析，不改 schema，不删除字段。
