# Relay Pool 数据架构 Stage 4 进度审计

日期：2026-07-08

## 范围

Stage 4 把价格候选构建迁移到 `src/lib/projections/pricingFacts.ts`，价格页视觉结构不变，不删除字段，不改 schema。

## Drift Intake

- 主 checkout 路径：`D:\Dev\Projects\relay-pool-desktop`
- 主 checkout HEAD：`ca0d9a7 fix: refine sub2api group displays`
- 主 checkout dirty paths：未观察到 tracked / untracked dirty path；`git status --short` 仍输出 `.pnpm-store` 缺失目录 warning。
- 工作树路径：`D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`
- 工作树分支：`codex/data-architecture-stage0`
- Stage 4 起点 HEAD：`fb3c850 test: guard current group projection boundaries`
- 当前工作树 HEAD：`d235aa2 refactor: consume pricing projection candidates`

## 已完成

- `6fa8753 docs: add data architecture stage4 plan`
- `56833c0 docs: refine stage4 projection test loader`
- `bb60751 test: guard pricing projection candidates`
- `8c633fb refactor: add pricing projection candidates`
- `d235aa2 refactor: consume pricing projection candidates`
- `buildPricingGroupCandidates()` 基于 Stage 3 `buildCurrentStationGroupFacts()` 生成价格候选。
- `src/features/pricing/pricingComparisonViewModel.ts` 已消费 shared pricing projection candidates。
- `scripts/pricing-comparison-view-model.test.mjs` 已加入 source guard，防止回退到页面内 group/rate projection 大段逻辑。

## pricingRules 处理

- `pricingRules` 不再被 `void input.pricingRules` 忽略。
- Stage 4 仅允许 enabled matching rule 在 current group fact 没有倍率时提供 `rateMultiplier` fallback。
- Stage 4 不使用 `inputPrice` / `outputPrice` 覆盖官方目录价格。

## 字段审计

- 本阶段未新增 schema 字段。
- 本阶段未删除字段、未合并字段语义、未把 compatibility cache 升级为 authoritative fact。
- 字段归属清单无需新增 `unknown pending audit` 行。

## 验证

已在 `d235aa2` 后运行：

```powershell
node scripts/pricing-comparison-view-model.test.mjs
node scripts/pricing-facts-projection.test.mjs
node scripts/group-facts-projection.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
pnpm.cmd build
```

结果：

- 上述 Node 脚本全部 exit 0。
- `pnpm.cmd build` exit 0，仅保留既有 Vite chunk-size warning。

## 下一步

- 完成 Stage 4 final verification 后提交本审计。
- 更新 rolling heartbeat，下一轮进入 Stage 5：站点详情与站点资产迁移。
- Stage 5 先做 drift intake，再创建/审核 Stage 5 计划；不得改视觉结构，不改 schema，不删除字段。
