# Relay Pool 数据架构工程化总计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement stage plans task-by-task. Every worker must read `docs/superpowers/specs/2026-07-07-relay-pool-data-architecture-master-spec.md` before editing code.

**Goal:** 把 Relay Pool Desktop 的数据读取、当前事实投影、字段所有权和 runtime snapshot 分阶段工程化，减少重复代码和数据不同步，同时保护现有功能。

**Architecture:** 采用“先保护、再抽公共工具、再建 query/projection、再迁消费者、最后评估兼容字段”的顺序。每个 stage 独立可 review、可回滚、可验证，并在主工作区继续修 bug 的情况下通过 drift intake 接入新字段和新行为。

**Tech Stack:** Tauri 2, React, TypeScript, Vite, Tailwind CSS, Rust services, SQLite, Node script tests, Cargo checks.

---

## 总入口

每个后续线程或压缩后的继续执行都按这个顺序读取：

1. `docs/superpowers/specs/2026-07-07-relay-pool-data-architecture-master-spec.md`
2. `docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-master-plan.md`
3. `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`
4. 当前 stage 的具体计划，例如 `docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-stage0.md`
5. 最新 stage 审计或 handoff 文档
6. 当前工作树 `git status --short`、`git log --oneline -8`

## 工作树与主线规则

- 架构升级必须在工作树中执行，不直接在主 checkout 大改。
- 主 checkout 可以继续修 bug；升级工作树每个 stage 开始、结束、合并前都做 drift intake。
- 不使用 `git add .`、`git add -A`、`git commit -a`。
- 每个提交只 stage 明确路径。
- 不回滚主 checkout 的用户改动。
- 如果主线新增字段，先归类和测试，不在 merge conflict 里顺手删字段或合并字段。
- 主 checkout 的未提交改动不能通过 `git merge master` 接入。必须等提交、用用户批准的 patch 接入，或在 stage 审计里明确记录未接入。
- push 不是默认动作；除非用户明确要求，否则只做本地提交和报告。

## Drift Intake Protocol

每个 stage 的第一步和合并前都执行：

```powershell
git branch --show-current
git rev-parse --abbrev-ref "master@{upstream}"
git -C D:\Dev\Projects\relay-pool-desktop status --short
git -C D:\Dev\Projects\relay-pool-desktop log --oneline -8
git status --short
git log --oneline -8
```

然后记录：

- 主 checkout HEAD。
- 工作树 HEAD。
- 主 checkout dirty paths。
- 当前 stage 文件范围。
- 集成分支和 upstream，例如 `master` / `origin/master`。
- 主线新增字段、改名字段、删除字段、接口签名变化。

如果主 checkout 只有已提交变更需要接入：

```powershell
git fetch --all --prune
git merge master
```

如果主 checkout 有未提交变更，不能声称已经接入。只能选择：

- 等主 checkout 提交后再 merge。
- 用户明确指定文件后，用 patch 接入工作树，并把 patch 来源写进审计。
- 记录 dirty paths 和风险，明确本 stage 未接入这些变化。

冲突处理规则：

- 字段冲突先保留双方语义，写审计，再决定迁移。
- 行为冲突先跑主线相关测试，确认 bugfix 行为。
- UI 冲突不借机重做设计。
- schema 冲突不在早期 stage 顺手改表结构。
- 新字段必须先登记到 `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`，默认状态为 `pending audit`。

## Stage 0: 基线与安全网

**目标：** 不改生产行为，先锁住关键回归契约。

**主要文件：**

- `docs/superpowers/specs/2026-07-07-relay-pool-data-architecture-master-spec.md`
- `docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-master-plan.md`
- `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`
- `scripts/pricing-comparison-view-model.test.mjs`
- `scripts/data-architecture-field-ownership.test.mjs`
- `scripts/test-dashboard-balance-summary.mjs`
- `scripts/edit-key-page-flow.test.mjs`
- `scripts/shared-capabilities-contract.test.mjs`
- `docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage0-baseline.md`

**必须保护：**

- 同一个当前分组不能因为 binding 和 rate record 同时存在而显示两次。
- 同站点同模型下多个真实不同分组必须保留。
- `group_key_hash` 和 `group_id_hash` 不得互相替代。
- station-scope balance snapshot 优先于 key-scope snapshot 和 station 缓存。
- Key 编辑无关字段时保留 group binding。
- 兼容字段直接消费只能出现在白名单路径。

**退出条件：**

Stage 0 具体实施开始前，必须先把总 SPEC、总计划、字段归属清单和 Stage 0 计划作为 docs 提交，使工作树回到干净状态。

```powershell
node scripts/pricing-comparison-view-model.test.mjs
node scripts/add-provider-key-groups.test.mjs
node scripts/test-dashboard-balance-summary.mjs
node scripts/edit-key-page-flow.test.mjs
node scripts/shared-capabilities-contract.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
pnpm.cmd build
```

全部通过。若未改 Rust 生产代码，Rust 只做源码契约；若改 Rust，必须运行：

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

## Stage 1: 低风险工具去重

**目标：** 清理重复工具函数，不改变业务语义。

**候选模块：**

- `src/lib/errors.ts`
- `src/lib/formatters.ts`
- `src/lib/time.ts`
- `src/lib/statusLabels.ts`

**允许迁移：**

- `readError`
- `toTime`
- 日期和相对时间格式化
- 金额、倍率格式化
- status label / tone 映射

**禁止：**

- 不迁移分组 identity。
- 不迁移价格 projection。
- 不改 UI 布局。
- 不顺手改中文文案，除非测试证明是错误输出。

**退出条件：**

- 被替换输出有 focused tests 或源码断言证明文本、精度、空值展示一致。
- `pnpm.cmd build` 通过。

## Stage 2: Query Services

**目标：** 把页面里的重复 `Promise.all` 和加载编排收敛到查询服务，不改事实语义。

**新增或修改模块：**

- `src/lib/queries/stationQueries.ts`
- `src/lib/queries/pricingQueries.ts`
- `src/lib/queries/dashboardQueries.ts`
- `src/lib/queries/keyPoolQueries.ts`
- `src/lib/queries/providerEditQueries.ts`

**候选函数：**

- `loadStationDetailBundle(stationId)`
- `loadAllStationPricingFacts()`
- `loadStationAssetWorkspace()`
- `loadKeyPoolWorkspace()`
- `loadProviderEditBundle(stationId)`

**迁移顺序：**

1. 先新增 query service 并让测试覆盖返回 shape。
2. 单页迁移消费者。
3. 保留页面原有 loading、toast、局部失败体验。
4. 每迁一页，运行对应 focused scripts 和 `pnpm.cmd build`。

**边界：**

- Query service 只做加载编排和 raw facts 搬运。
- Query service 可以读取兼容字段，但不得解释它们为权威事实。
- 分组 identity、倍率 fallback、余额优先级和 route 决策必须留给 projection/runtime 层。
- 新增 query service 读取的兼容字段必须登记到字段归属清单。

## Stage 3: 当前分组投影

**目标：** 建立统一的 `StationGroupCurrentFact`，消除页面各自写 binding/rate 去重和倍率 fallback。

**新增模块：**

- `src/lib/projections/groupFacts.ts`

**核心函数：**

- `buildCurrentStationGroupFacts(bindings, rates)`
- `latestGroupRatesByBindingOrHash(rates)`
- `buildStationGroupOptionsFromCurrentFacts(groupFacts)`

**规则：**

- identity fallback 顺序：`group_binding_id` -> `group_key_hash` -> `group_id_hash` -> normalized `group_name`。
- 倍率 fallback 顺序遵守总 SPEC。
- `missing` / `disabled` 不能被旧 rate record 复活。

**退出条件：**

- 纯函数测试覆盖 same binding+rate 去重、多真实分组保留、same name different remote id、missing 不复活。
- 尚未迁移页面前，旧消费者仍保持可用。

## Stage 4: 价格投影与价格页迁移

**目标：** 把价格候选构建迁到 shared pricing projection，价格页只渲染 view model。

**新增或修改模块：**

- `src/lib/projections/pricingFacts.ts`
- `src/features/pricing/pricingComparisonViewModel.ts`
- `src/features/pricing/PricingPage.tsx`

**规则：**

- row identity 包含 model id 与 current group identity。
- `pricingRules` 不得继续被 `void input.pricingRules` 忽略。
- 不改价格页视觉结构，先迁数据来源。

**退出条件：**

- 价格 focused tests 通过。
- 价格页源码不再包含旧 group/rate matching 大段逻辑。
- `pnpm.cmd build` 通过。

## Stage 5: 站点详情与站点资产迁移

**目标：** 站点详情 group rows、站点资产 chips、余额展示改用 current projections。

**候选模块：**

- `src/features/stations/stationDetailViewModels.ts`
- `src/features/stations/stationAssetViewModels.ts`
- `src/lib/projections/balanceFacts.ts`
- `src/lib/projections/stationFacts.ts`

**规则：**

- 站点详情和资产列表对分组数量、missing 状态、倍率展示一致。
- 余额优先使用 station-scope snapshot，再 fallback station 缓存。
- 刷新动作、局部错误、toast 行为保持。

## Stage 6: Key Pool 与 Add Provider 迁移

**目标：** 分组选项、Key 草稿合并、远端 Key 创建统一使用 current group facts。

**候选模块：**

- `src/features/key-pool/KeyPoolPage.tsx`
- `src/features/key-pool/EditKeyPage.tsx`
- `src/features/stations/AddProviderPage.tsx`
- `src/features/stations/groupOptionViewModels.ts`

**规则：**

- 创建/编辑 Key 必须保留 selected `group_binding_id`。
- 清除分组必须是显式动作。
- 远端 Key 创建由后端解析真实上游 group id。
- 页面不复制兼容字段当业务事实。

## Stage 7: Runtime Snapshot

**目标：** 为本地 proxy 编译稳定运行时输入。

**候选模块：**

- `src/lib/projections/runtimeSnapshot.ts`
- Rust side command or service for snapshot persistence when needed

**输出必须包含：**

- snapshot id/version
- station key candidates
- station/base URL
- secret references，不含明文 secret
- enabled/priority
- group binding id
- effective multiplier/source
- model policy
- pricing status
- balance status
- health/cooldown state
- route policy data

**规则：**

- runtime 不读 UI view model。
- request logs 记录实际 route decision evidence。
- snapshot 编译可在不启动 proxy 的情况下测试。

## Stage 8: 兼容字段复查

**目标：** 只有消费者迁移完成后，才评估兼容字段降级。

**步骤：**

1. 生成兼容字段读写清单。
2. 给每个字段标注 `active`、`compatibility cache`、`deprecated` 或 `removable candidate`。
3. 为每个 removable candidate 写 migration/rollback note。
4. 老数据库、preview fallback、runtime 任一路径仍依赖时，不删除。

## 合并前总门槛

每个 stage 合并前必须报告：

- 工作树路径和分支。
- 主 checkout HEAD 与工作树 HEAD。
- 主线 drift intake 结论。
- 字段归属清单更新情况。
- 修改文件列表。
- focused tests。
- `pnpm.cmd build` 结果。
- Rust 检查结果或未运行原因。
- `git status --short`。
- 未解决风险。

## 停止条件

出现下面任何情况，停止当前 stage，不继续扩大修改：

- 新字段语义无法判断。
- 测试失败但不能确认是本阶段引入。
- 主线 bugfix 与工作树改动冲突，且行为预期不明确。
- 需要删除字段或改 schema 才能继续。
- runtime route、remote key secret、group binding persistence 出现不确定风险。

## 推荐提交粒度

- `docs:` 总 SPEC、总计划、stage 审计。
- `test:` focused regression 或 source guard。
- `refactor:` 工具函数去重，不改变行为。
- `feat:` 新 query/projection module。
- `fix:` 被 focused RED 证明的极小行为修复。

每个提交只包含一个 stage 内的一类变化。
