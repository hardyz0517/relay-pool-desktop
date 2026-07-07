# Relay Pool 数据架构 Stage 3 Intake 阻塞审计

日期：2026-07-07

## 结论

Stage 3 暂不执行代码迁移。当前分组投影会直接触碰 `groupFacts`、pricing、station detail 和 collector 语义；主 checkout 仍存在未提交高重叠 dirty 改动。根据总 SPEC 和 Stage 3 计划，未提交主线改动不是可 merge 的事实，不能通过工作树自行推断或覆盖。

## 当前工作树

- 路径：`D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`
- 分支：`codex/data-architecture-stage0`
- HEAD：`c3e104c docs: add data architecture stage3 plan`
- 状态：干净

## 主 checkout drift

主 checkout HEAD：

- `7e7d567 docs: tighten data architecture refactor safeguards`

主 checkout dirty paths：

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

## 为什么不能继续 Stage 3 代码

- `src/lib/types/groupFacts.ts` 已在主 checkout dirty 列表中，Stage 3 的投影类型必须以该文件最终提交后的语义为准。
- `src/features/pricing/PricingPage.tsx` 和 `src/features/pricing/pricingComparisonViewModel.ts` 已在主 checkout dirty 列表中，当前分组投影会影响后续价格候选行 identity 与倍率 fallback。
- `src/features/stations/components/StationDetailContent.tsx` 与 `src/features/stations/stationDetailViewModels.ts` 已在主 checkout dirty 列表中，当前分组投影会影响站点详情分组展示与 missing 状态。
- `src-tauri/src/services/collectors/adapters/sub2api.rs` 与 `src-tauri/src/services/collectors/apply.rs` 已在主 checkout dirty 列表中，collector 写入语义可能改变 group fact 的输入事实。

## 解除条件

继续 Stage 3 前必须满足其中之一：

1. 主 checkout 提交上述高重叠改动，本工作树 merge 对应提交后重新运行 Stage 0/1/2 guard。
2. 用户明确批准把指定 exact paths 以 patch 形式接入本工作树，并在审计中记录 patch 来源。

解除后第一步：

```powershell
node scripts/pricing-comparison-view-model.test.mjs
node scripts/add-provider-key-groups.test.mjs
node scripts/shared-capabilities-contract.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
pnpm.cmd build
```

全部通过后，再按 `docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-stage3.md` 从 Task 1 RED test 开始。
