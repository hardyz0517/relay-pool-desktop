# Relay Pool 数据架构 Stage 6 进度审计

日期：2026-07-08

## 范围

Stage 6 迁移 Key Pool、Edit Key、Add Provider 和分组选项 helper 到 shared current group facts。页面视觉结构不变，不删除字段，不改 schema，不改变远端 Key 创建由后端解析真实上游 group id 的边界。

## Drift Intake

- 主 checkout 路径：`D:\Dev\Projects\relay-pool-desktop`
- 主 checkout HEAD：`ca0d9a7 fix: refine sub2api group displays`
- 主 checkout dirty paths：未观察到 tracked / untracked dirty path；`git status --short` 仍输出 `.pnpm-store` 缺失目录 warning。
- 工作树路径：`D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`
- 工作树分支：`codex/data-architecture-stage0`
- Stage 6 起点 HEAD：`437c45c docs: summarize data architecture stage5 progress`
- 当前工作树 HEAD：`838e0ac refactor: consume current group facts in key flows`

## 已完成

- `e913484 docs: add data architecture stage6 plan`
- `2971ed5 test: align add-provider guard with current group facts`
- `d012f55 test: guard group options from current facts`
- `96312c8 refactor: build group options from current facts`
- `1e1c0c1 test: guard key group current fact selection`
- `838e0ac refactor: consume current group facts in key flows`
- `src/features/stations/groupOptionViewModels.ts` 新增 `buildStationGroupOptionsFromCurrentFactsForSelect()`，从 `StationGroupCurrentFact[]` 生成既有 `StationGroupOption[]`，并过滤 non-displayable current facts。
- `src/features/key-pool/KeyPoolPage.tsx` 和 `src/features/key-pool/EditKeyPage.tsx` 不再直接消费 `listStationGroupOptions()`；改为加载 bindings + rates，经 `buildCurrentStationGroupFacts()` 和 `buildStationGroupOptionsFromCurrentFactsForSelect()` 生成选择项。
- `src/features/stations/AddProviderPage.tsx` 增加 current group options：持久化 current facts 优先，草稿分组作为 overlay；删除中的草稿会显式遮蔽对应 current option。
- `src/features/stations/components/StationKeyRowsEditor.tsx` 不再用 `rateSource: null` 回填草稿 option。
- `KEEP_GROUP_BINDING_VALUE` 与 `CLEAR_GROUP_BINDING_VALUE` 保留，编辑 Key 仍区分保留绑定、显式清除绑定和设置新绑定。

## 字段审计

- 本阶段未新增 schema 字段。
- `station_group_bindings.id` / `station_keys.group_binding_id` 仍是本地 durable binding identity。
- `group_id_hash` 仍是远端 identity metadata / compatibility cache，不替代 binding id。
- `group_name` 仍只作为展示和 legacy fallback。
- `group_rate_records` 仍是 evidence/history，由 current projection 决定当前倍率和可展示状态。
- `station_keys.rate_multiplier` / `station_keys.rate_source` 仍是 compatibility cache；Stage 6 没有把页面草稿或 compatibility cache 升格为业务事实。
- 字段归属清单无需新增 `unknown pending audit` 行。

## 验证

已在 `838e0ac` 后运行：

```powershell
node scripts/group-option-current-facts.test.mjs
node scripts/key-group-selection-current-facts.test.mjs
node scripts/add-provider-key-groups.test.mjs
node scripts/edit-key-page-flow.test.mjs
node scripts/shared-capabilities-contract.test.mjs
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
- 进入 Stage 7：Runtime Snapshot。
- Stage 7 重点保护 runtime 不读取 UI view model，编译稳定 `RuntimeRouteSnapshot`，保留 secret references 不暴露明文 secret，并把 group binding id、effective multiplier/source、model policy、pricing/balance/health/cooldown evidence 纳入 snapshot 测试。
