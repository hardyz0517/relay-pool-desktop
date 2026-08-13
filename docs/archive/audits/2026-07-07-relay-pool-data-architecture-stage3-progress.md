# Relay Pool 数据架构 Stage 3 进度审计

日期：2026-07-08

## 范围

Stage 3 当前只建立 `src/lib/projections/groupFacts.ts` 纯函数投影，不迁移页面消费者，不删除字段，不改 schema。

## Drift Intake

- 主 checkout 路径：`D:\Dev\Projects\relay-pool-desktop`
- 主 checkout HEAD：`ca0d9a7 fix: refine sub2api group displays`
- 主 checkout dirty paths：未观察到 tracked / untracked dirty path；`git status --short` 仍输出 `.pnpm-store` 缺失目录 warning。
- 工作树路径：`D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`
- 工作树分支：`codex/data-architecture-stage0`
- 工作树 intake merge：`dfe86c8 merge: intake sub2api group display fixes`

## 已完成

- 接入主线 `ca0d9a7 fix: refine sub2api group displays`，解决 `src/features/pricing/pricingComparisonViewModel.ts` merge conflict。
- `60bb844 test: guard current group projection`
- `fa5ab21 refactor: add current group projection`
- `buildCurrentStationGroupFacts()`
- `latestGroupRatesByBindingOrHash()`
- `buildStationGroupOptionsFromCurrentFacts()`
- `scripts/group-facts-projection.test.mjs`

## 字段审计

- `src/lib/types/groupFacts.ts` 本次主线接入未新增字段。
- 主线只把 `missing` 加入 `isCollectedStationGroupBinding()` 排除条件，符合总 SPEC 中 `missing` 不得被旧 rate history 复活的规则。
- 字段归属清单无需新增 `unknown pending audit` 行。

## 当前投影规则

- identity fallback：`group_binding_id` -> `group_key_hash` -> `group_id_hash` -> normalized `group_name`。
- 倍率 fallback：binding user -> binding effective -> latest rate user -> latest rate effective -> binding default -> latest rate default -> `null`。
- `missing` / `disabled` binding 保持 unavailable，但仍保留历史 rate evidence。
- 与已有 binding durable key 匹配的 standalone rate 不会重复生成第二个 current fact。

## 验证

```powershell
node scripts/pricing-comparison-view-model.test.mjs
node scripts/add-provider-key-groups.test.mjs
node scripts/shared-capabilities-contract.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
node scripts/query-services-boundary.test.mjs
node scripts/group-facts-projection.test.mjs
pnpm.cmd build
```

结果：

- 上述 Node 脚本全部 exit 0。
- `pnpm.cmd build` exit 0，仅保留既有 Vite chunk-size warning。

## 下一步

- 完成 Stage 3 Task 3：提交 projection boundary guard 和本审计。
- 然后更新 rolling heartbeat，下一轮从 Stage 4 规划/入口门禁开始，除非需要先做 Stage 3 closeout 验证。
