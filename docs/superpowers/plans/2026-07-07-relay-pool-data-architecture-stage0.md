# Relay Pool Data Architecture Stage 0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在隔离工作树中建立数据架构升级的第一层保护网，先锁住价格分组、余额、Key 分组绑定、兼容字段消费和共享能力边界，再进入生产逻辑迁移。

**Architecture:** Stage 0 只新增或收紧 focused tests、source guards 和交付证据，不做 schema 删除、不合并相似字段、不重排 UI。若新测试暴露现有真实缺陷，先停在明确 RED 证据，再拆出一个极小修复提交，修复完成后继续 Stage 0 护栏。

**Tech Stack:** Tauri 2, React, TypeScript, Vite, Node script tests, Rust service source guards, Git worktree.

---

## 执行边界

- 工作树：`D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`
- 分支：`codex/data-architecture-stage0`
- 先读总 SPEC：`docs/superpowers/specs/2026-07-07-relay-pool-data-architecture-master-spec.md`
- 先读总计划：`docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-master-plan.md`
- 先读字段归属清单：`docs/superpowers/audits/relay-pool-field-ownership-ledger.md`
- 主工作区已有无关 dirty files，不在本阶段触碰。
- 主工作区会继续修 bug；每个 Stage 开始、结束和准备合并前，都必须重新接入主线变化并做字段漂移审计。
- 精确路径 stage，禁止 `git add .`、`git add -A`、`git commit -a`。
- Stage 0 默认不改生产行为；只有 focused test 明确暴露当前 bug 时，才允许单独小修。

## 前置提交

本计划开始执行测试和护栏前，必须先把以下规划文件作为一个 docs 提交，使工作树回到干净状态：

- `docs/superpowers/specs/2026-07-07-relay-pool-data-architecture-master-spec.md`
- `docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-master-plan.md`
- `docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-stage0.md`
- `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`

推荐提交：

```powershell
git add -- docs/superpowers/specs/2026-07-07-relay-pool-data-architecture-master-spec.md docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-master-plan.md docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-stage0.md docs/superpowers/audits/relay-pool-field-ownership-ledger.md
git commit -m "docs: add data architecture master plan"
```

## 主线漂移接入规则

主工作区在升级期间可能新增字段、改接口或修复数据语义。工作树不能假设最初审计结果永远有效；每次接入主线变化时按下面规则处理：

- 新增字段默认视为未知语义，不删除、不合并、不重命名。
- 新增字段必须先进入 `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`，标注为 `canonical fact`、`evidence/history`、`current projection`、`compatibility cache` 或 `unknown pending audit`。
- 如果新增字段和旧字段名字相似，只能在测试证明读写来源、生命周期、fallback 顺序都一致后，才允许提出合并建议。
- 如果主线 bugfix 修改了页面行为，本 Stage 的测试要先确认新行为；不能为了让旧计划通过而回滚主线修复。
- 合并前必须重新跑 `scripts/data-architecture-field-ownership.test.mjs`，并把新增白名单解释写进审计文档。
- 如果字段漂移影响价格、余额、Key 分组、remote key 或 runtime route，停止当前 Stage，先补一个 drift intake 小审计，再继续实现。
- 主工作区 dirty files 不能通过 `git merge master` 接入。未提交主线改动只能等提交、用用户批准的 patch 接入，或记录为本 stage 未接入风险。

## 文件结构

- Modify: `scripts/pricing-comparison-view-model.test.mjs`
  - 继续作为价格候选、分组去重、分组身份语义的主保护脚本。
- Create: `scripts/data-architecture-field-ownership.test.mjs`
  - 扫描兼容字段直接消费位置，使用显式白名单记录当前旧消费者。
- Modify: `scripts/test-dashboard-balance-summary.mjs`
  - 收紧 station-scope 余额优先级和最新值选择契约。
- Modify: `scripts/edit-key-page-flow.test.mjs`
  - 收紧 Key 编辑页必须保留/显式清除 group binding 的页面契约。
- Modify: `scripts/shared-capabilities-contract.test.mjs`
  - 收紧 Rust shared capabilities 对 `group_binding_id`、`group_id_hash`、`group_name` 的保存边界。
- Create: `docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage0-baseline.md`
  - 记录 Stage 0 RED/GREEN 证据、白名单解释和下一阶段禁止事项。

## Task 1: 工作树基线与证据记录

**Files:**
- Create: `docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage0-baseline.md`

- [ ] **Step 1: 确认规划文档已经提交，工作树从干净状态开始**

Run:

```powershell
git status --short
git log --oneline -5
```

Expected:

- `git status --short` 为空。
- 最近提交包含 `docs: add data architecture master plan` 或同等规划文档提交。

- [ ] **Step 2: 确认隔离工作树和分支**

Run:

```powershell
git status --short
git branch --show-current
git log --oneline -5
```

Expected:

```text
codex/data-architecture-stage0
7e7d567 docs: tighten data architecture refactor safeguards
```

`git status --short` 应为空；如果不为空，只处理本计划列出的文件。

- [ ] **Step 3: 记录主线同步点**

Run:

```powershell
git branch --show-current
git rev-parse --abbrev-ref "master@{upstream}"
git -C D:\Dev\Projects\relay-pool-desktop rev-parse --short HEAD
git -C D:\Dev\Projects\relay-pool-desktop status --short
git -C D:\Dev\Projects\relay-pool-desktop log --oneline -8
git rev-parse --short HEAD
```

Expected:

- 记录主工作区当前 HEAD 和 dirty paths。
- 记录工作树当前 HEAD。
- 不从主工作区 stage 或修改任何文件。
- 记录集成分支和 upstream。

如果主工作区已有与 Stage 0 相关的新字段或 bugfix 提交，先用非破坏方式接入已提交内容：

```powershell
git fetch --all --prune
git merge master
```

Expected: merge 成功或产生明确冲突。若冲突涉及字段语义，停止并写 drift intake 说明，不在冲突中顺手删字段。

如果相关 bugfix 仍是主工作区未提交 dirty files，不执行 merge 接入声明；改为在审计里记录：

```markdown
## 未提交主线改动

- dirty paths：记录 `git -C D:\Dev\Projects\relay-pool-desktop status --short`
- 接入状态：未接入 / 等主线提交 / 用户批准 patch
- 风险：说明这些改动可能影响哪些 Stage 0 测试
```

- [ ] **Step 4: 运行当前基线脚本**

Run:

```powershell
node scripts/pricing-comparison-view-model.test.mjs
node scripts/add-provider-key-groups.test.mjs
node scripts/test-dashboard-balance-summary.mjs
node scripts/edit-key-page-flow.test.mjs
node scripts/shared-capabilities-contract.test.mjs
```

Expected: 每条命令 exit code 为 `0`。

- [ ] **Step 5: 写基线审计文档**

Create `docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage0-baseline.md`:

```markdown
# Relay Pool Data Architecture Stage 0 Baseline

日期：2026-07-07
工作树：`D:\Dev\Projects\relay-pool-desktop\.worktrees\data-architecture-stage0`
分支：`codex/data-architecture-stage0`

## 基线命令

- `node scripts/pricing-comparison-view-model.test.mjs`
- `node scripts/add-provider-key-groups.test.mjs`
- `node scripts/test-dashboard-balance-summary.mjs`
- `node scripts/edit-key-page-flow.test.mjs`
- `node scripts/shared-capabilities-contract.test.mjs`

## Stage 0 边界

- 不删除字段。
- 不合并 `group_key_hash` 和 `group_id_hash`。
- 不把 `group_name` 当作首选 identity。
- 不迁移 UI 布局。
- 不引入新的 Tauri command。
- 不改数据库 schema。

## 主线同步点

- 主工作区 HEAD：记录执行时的 `git -C D:\Dev\Projects\relay-pool-desktop rev-parse --short HEAD`。
- 工作树 HEAD：记录执行时的 `git rev-parse --short HEAD`。
- 主工作区 dirty paths：只记录路径，不复制、不回滚。
- 若接入了主线提交：记录 merge/rebase commit 和冲突处理结论。
- 未提交主线改动：记录是否未接入、等待提交或使用用户批准 patch。
- 字段归属清单：记录 `docs/superpowers/audits/relay-pool-field-ownership-ledger.md` 是否有新增/变更。

## 保护对象

- 价格页同一个当前分组不能因为 binding 和 rate record 同时存在而显示两次。
- 同站点同模型下多个真实不同分组必须保留。
- station-scope 余额快照优先于 station-key 快照。
- Key 编辑无关字段时必须保留当前 group binding；清除分组必须是显式动作。
- shared capabilities 保存 Key 时必须携带 group binding 选择，不能把上游 group id 当成本地 binding id。
- 兼容字段只能在白名单路径内直接消费，新代码应通过 projection/query 层消费。
```

- [ ] **Step 6: Commit 基线文档**

Run:

```powershell
git add -- docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage0-baseline.md
git commit -m "docs: record data architecture stage0 baseline"
```

Expected: 只提交审计文档。

## Task 2: 价格分组身份与去重保护

**Files:**
- Modify: `scripts/pricing-comparison-view-model.test.mjs`

- [ ] **Step 1: 写价格分组去重测试**

Append before the source-code assertions in `scripts/pricing-comparison-view-model.test.mjs`:

```javascript
const duplicateCurrentGroupView = buildPricingComparisonViewModel({
  models: [
    {
      provider: "openai",
      modelId: "gpt-5-mini",
      displayName: "GPT-5 mini",
      officialInputPrice: 0.25,
      officialOutputPrice: 2,
      currency: "USD",
      unit: "per_1m_tokens",
      aliases: ["gpt-5-mini"],
      groupMatchers: ["openai", "gpt", "default", "green"],
      enabledByDefault: true,
    },
  ],
  stations: [station("station-current-dedup", "Current Dedup Hub", 1)],
  stationKeys: [],
  groupBindings: [
    {
      ...group(
        "station-current-dedup",
        "binding-current",
        "default",
        0.8,
        "OpenAI green default",
        "2026-07-06T01:00:00.000Z",
      ),
      groupKeyHash: "stable-local-group",
      groupIdHash: "remote-group-default",
    },
  ],
  groupRates: [
    {
      ...rate(
        "station-current-dedup",
        "rate-current",
        "binding-current",
        "default",
        0.7,
        "OpenAI green default latest",
        "2026-07-06T03:00:00.000Z",
      ),
      groupKeyHash: "stable-local-group",
    },
    {
      ...rate(
        "station-current-dedup",
        "rate-shadow",
        null,
        "default",
        0.7,
        "OpenAI green default shadow",
        "2026-07-06T03:30:00.000Z",
      ),
      groupKeyHash: "stable-local-group",
    },
  ],
  pricingRules,
  modelEvidence: [],
  filters: {
    provider: "openai",
    modelQuery: "",
    stationId: "all",
    verifiedOnly: false,
  },
});
const duplicateCurrentGroupGpt = duplicateCurrentGroupView.sections.find(
  (section) => section.modelId === "gpt-5-mini",
);
assert.ok(duplicateCurrentGroupGpt, "duplicate-current-group GPT section should exist");
assert.deepEqual(
  duplicateCurrentGroupGpt.rows.map((row) => ({
    groupBindingId: row.groupBindingId,
    groupRateRecordId: row.groupRateRecordId,
    groupName: row.groupName,
  })),
  [{ groupBindingId: "binding-current", groupRateRecordId: "rate-current", groupName: "default" }],
  "same current group identity must not appear twice when binding and standalone rate share groupKeyHash",
);
```

- [ ] **Step 2: 运行测试确认结果**

Run:

```powershell
node scripts/pricing-comparison-view-model.test.mjs
```

Expected:

- If PASS: 说明当前逻辑已经覆盖该重复场景，继续 Step 3。
- If FAIL with two rows: 停止 Stage 0，记录失败输出到审计文档，然后拆出一个单独修复任务，只改 `src/features/pricing/pricingComparisonViewModel.ts` 和本测试。

- [ ] **Step 3: 写 `group_key_hash` 和 `group_id_hash` 不等价测试**

Append after the previous block:

```javascript
const distinctIdentityView = buildPricingComparisonViewModel({
  models: [
    {
      provider: "openai",
      modelId: "gpt-5-mini",
      displayName: "GPT-5 mini",
      officialInputPrice: 0.25,
      officialOutputPrice: 2,
      currency: "USD",
      unit: "per_1m_tokens",
      aliases: ["gpt-5-mini"],
      groupMatchers: ["openai", "gpt", "default", "green"],
      enabledByDefault: true,
    },
  ],
  stations: [station("station-distinct-identity", "Distinct Identity Hub", 1)],
  stationKeys: [],
  groupBindings: [
    {
      ...group(
        "station-distinct-identity",
        "binding-a",
        "default",
        0.8,
        "same remote id first local group",
        "2026-07-06T01:00:00.000Z",
      ),
      groupKeyHash: "local-group-a",
      groupIdHash: "same-remote-group-id",
    },
    {
      ...group(
        "station-distinct-identity",
        "binding-b",
        "default",
        0.6,
        "same remote id second local group",
        "2026-07-06T02:00:00.000Z",
      ),
      groupKeyHash: "local-group-b",
      groupIdHash: "same-remote-group-id",
    },
  ],
  groupRates: [],
  pricingRules,
  modelEvidence: [],
  filters: {
    provider: "openai",
    modelQuery: "",
    stationId: "all",
    verifiedOnly: false,
  },
});
const distinctIdentityGpt = distinctIdentityView.sections.find(
  (section) => section.modelId === "gpt-5-mini",
);
assert.ok(distinctIdentityGpt, "distinct-identity GPT section should exist");
assert.deepEqual(
  distinctIdentityGpt.rows.map((row) => ({
    groupBindingId: row.groupBindingId,
    groupName: row.groupName,
    groupMultiplier: row.groupMultiplier,
  })),
  [
    { groupBindingId: "binding-b", groupName: "default", groupMultiplier: 0.6 },
    { groupBindingId: "binding-a", groupName: "default", groupMultiplier: 0.8 },
  ],
  "group_key_hash and group_id_hash must not be treated as interchangeable identities",
);
```

- [ ] **Step 4: 运行测试**

Run:

```powershell
node scripts/pricing-comparison-view-model.test.mjs
```

Expected: PASS. If this fails because the implementation collapses rows by `groupIdHash` or `groupName`, stop and split a focused pricing identity fix.

- [ ] **Step 5: Commit 价格保护测试**

Run:

```powershell
git add -- scripts/pricing-comparison-view-model.test.mjs
git commit -m "test: guard pricing group identity deduplication"
```

Expected: 只提交 `scripts/pricing-comparison-view-model.test.mjs`。

## Task 3: 兼容字段所有权扫描

**Files:**
- Create: `scripts/data-architecture-field-ownership.test.mjs`

- [ ] **Step 1: 新增扫描脚本**

Create `scripts/data-architecture-field-ownership.test.mjs`:

```javascript
import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const scanRoots = ["src", "src-tauri/src"];
const compatibilityFields = [
  "balanceRaw",
  "balanceCny",
  "lastPricingFetchedAt",
  "groupIdHash",
  "groupName",
  "rateMultiplier",
  "rateSource",
  "rateCollectedAt",
  "group_id_hash",
  "group_name",
  "rate_multiplier",
  "rate_source",
  "rate_collected_at",
  "balance_raw",
  "balance_cny",
  "last_pricing_fetched_at",
];

const allowedPathPatterns = [
  /^src[\\/]lib[\\/]types[\\/]/,
  /^src[\\/]lib[\\/]api[\\/]/,
  /^src[\\/]lib[\\/]projections[\\/]/,
  /^src[\\/]features[\\/]pricing[\\/]pricingComparisonViewModel\.ts$/,
  /^src[\\/]features[\\/]stations[\\/]AddProviderPage\.tsx$/,
  /^src[\\/]features[\\/]stations[\\/]groupOptionViewModels\.ts$/,
  /^src[\\/]features[\\/]stations[\\/]stationDetailViewModels\.ts$/,
  /^src[\\/]features[\\/]key-pool[\\/]/,
  /^src-tauri[\\/]src[\\/]models[\\/]/,
  /^src-tauri[\\/]src[\\/]services[\\/]database\.rs$/,
  /^src-tauri[\\/]src[\\/]services[\\/]shared_capabilities\.rs$/,
  /^src-tauri[\\/]src[\\/]services[\\/]collectors[\\/]/,
];

const sourceFiles = [];
for (const scanRoot of scanRoots) {
  await collectSourceFiles(path.join(root, scanRoot), sourceFiles);
}

const violations = [];
for (const absolutePath of sourceFiles) {
  const relativePath = normalizePath(path.relative(root, absolutePath));
  if (allowedPathPatterns.some((pattern) => pattern.test(relativePath))) {
    continue;
  }

  const source = await readFile(absolutePath, "utf8");
  for (const field of compatibilityFields) {
    const pattern = new RegExp(`(?<![A-Za-z0-9_])${escapeRegExp(field)}(?![A-Za-z0-9_])`, "g");
    if (pattern.test(source)) {
      violations.push(`${relativePath}: ${field}`);
    }
  }
}

assert.deepEqual(
  violations,
  [],
  `compatibility fields must be read through approved legacy/projection/query boundaries:\n${violations.join("\n")}`,
);

async function collectSourceFiles(directory, files) {
  const entries = await readdir(directory, { withFileTypes: true });
  for (const entry of entries) {
    const absolutePath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules" || entry.name === "target" || entry.name === "dist") {
        continue;
      }
      await collectSourceFiles(absolutePath, files);
      continue;
    }
    if (/\.(ts|tsx|rs)$/.test(entry.name)) {
      files.push(absolutePath);
    }
  }
}

function normalizePath(value) {
  return value.split(path.sep).join("/");
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
```

- [ ] **Step 2: 运行扫描脚本**

Run:

```powershell
node scripts/data-architecture-field-ownership.test.mjs
```

Expected: PASS.

If it fails, inspect the listed paths. For an existing legacy consumer, add the narrowest path pattern and document why in the audit file. For a new accidental consumer, fix the consumer to use an existing helper or stop before production changes.

- [ ] **Step 3: 把白名单说明写进审计文档**

Append to `docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage0-baseline.md`:

```markdown
## 兼容字段白名单

当前允许直接消费兼容字段的路径只包括：

- 类型定义：字段必须存在于类型层。
- `src/lib/api/**`：当前 Tauri/preview fallback 边界仍需读写旧字段。
- `src/lib/projections/**`：未来 projection 所在层。
- pricing / station detail / Add Provider / Key Pool：尚未迁移的旧消费者，后续每迁走一个就收窄白名单。
- Rust models/services/collectors/database：当前持久化、采集和 shared capabilities 边界。

新增页面或 runtime snapshot 不得直接消费兼容字段。
```

- [ ] **Step 4: Commit 字段所有权扫描**

Run:

```powershell
git add -- scripts/data-architecture-field-ownership.test.mjs docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage0-baseline.md
git commit -m "test: guard compatibility field ownership"
```

Expected: 只提交扫描脚本和审计文档。

## Task 4: 余额来源保护

**Files:**
- Modify: `scripts/test-dashboard-balance-summary.mjs`

- [ ] **Step 1: 收紧 dashboard station-scope 余额测试**

Replace the current `const summary = summarizeDashboardBalances([...]);` block with:

```javascript
const summary = summarizeDashboardBalances([
  {
    id: "key-raw-newer",
    stationId: "station-a",
    scope: "station_key",
    value: 100,
    currency: "CNY",
    status: "normal",
    updatedAt: "5000",
  },
  {
    id: "station-normalized",
    stationId: "station-a",
    scope: "station",
    value: 10,
    currency: "CNY",
    status: "normal",
    updatedAt: "2000",
  },
  {
    id: "station-b-old-low",
    stationId: "station-b",
    scope: "station",
    value: 5,
    currency: "CNY",
    status: "low",
    updatedAt: "1000",
  },
  {
    id: "station-b-newer-normal",
    stationId: "station-b",
    scope: "station",
    value: 6,
    currency: "CNY",
    status: "normal",
    updatedAt: "4000",
  },
  {
    id: "station-c-usd",
    stationId: "station-c",
    scope: "station",
    value: 2,
    currency: "USD",
    status: "depleted",
    updatedAt: "3000",
  },
]);

assert.equal(summary.totalBalance, 18);
assert.equal(summary.lowBalanceStations, 1);
assert.equal(summary.primaryBalanceCurrency, "CNY");
assert.deepEqual(
  summary.latestStationBalances.map((balance) => balance.id),
  ["station-normalized", "station-b-newer-normal", "station-c-usd"],
);
```

- [ ] **Step 2: 运行余额测试**

Run:

```powershell
node scripts/test-dashboard-balance-summary.mjs
```

Expected: PASS.

- [ ] **Step 3: Commit 余额测试**

Run:

```powershell
git add -- scripts/test-dashboard-balance-summary.mjs
git commit -m "test: guard station balance summary source priority"
```

Expected: 只提交 `scripts/test-dashboard-balance-summary.mjs`。

## Task 5: Key 编辑和 shared capabilities 保护

**Files:**
- Modify: `scripts/edit-key-page-flow.test.mjs`
- Modify: `scripts/shared-capabilities-contract.test.mjs`

- [ ] **Step 1: 收紧 Edit Key 页面源码契约**

Append to `scripts/edit-key-page-flow.test.mjs` after the existing `editKeySource` assertions:

```javascript
assert.ok(
  editKeySource.includes("KEEP_GROUP_BINDING_VALUE") ||
    editKeySource.includes("groupBindingId: existingKey.groupBindingId") ||
    editKeySource.includes("groupBindingId: key.groupBindingId"),
  "edit-key page must preserve current group binding when unrelated fields are edited",
);

assert.ok(
  editKeySource.includes("CLEAR_GROUP_BINDING_VALUE") ||
    editKeySource.includes("groupBindingId: null"),
  "edit-key page must only clear group binding through an explicit clear action",
);
```

If this fails because Edit Key delegates to another helper instead of inline constants, replace the check with source assertions against that helper path in the same script.

- [ ] **Step 2: 查看 shared capabilities 当前测试并追加字段边界**

Open `scripts/shared-capabilities-contract.test.mjs` and append assertions that match the existing source-read style. Required assertions:

```javascript
assert.ok(
  sharedCapabilitiesSource.includes("group_binding_id") ||
    sharedCapabilitiesSource.includes("groupBindingId"),
  "shared capabilities must persist the selected local group binding id",
);

assert.ok(
  sharedCapabilitiesSource.includes("group_id_hash") ||
    sharedCapabilitiesSource.includes("groupIdHash"),
  "shared capabilities must preserve remote group identity hash as separate metadata",
);

assert.ok(
  sharedCapabilitiesSource.includes("group_name") ||
    sharedCapabilitiesSource.includes("groupName"),
  "shared capabilities must preserve group display name without using it as the primary identity",
);
```

Use the variable name already present in that file. If it currently names the Rust source `serviceSource`, keep that name and do not introduce a second read.

- [ ] **Step 3: 运行页面与后端契约测试**

Run:

```powershell
node scripts/edit-key-page-flow.test.mjs
node scripts/shared-capabilities-contract.test.mjs
```

Expected: PASS.

- [ ] **Step 4: Commit Key/shared capabilities 保护**

Run:

```powershell
git add -- scripts/edit-key-page-flow.test.mjs scripts/shared-capabilities-contract.test.mjs
git commit -m "test: guard key group binding preservation"
```

Expected: 只提交两个脚本。

## Task 6: Stage 0 汇总验证

**Files:**
- Modify: `docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage0-baseline.md`

- [ ] **Step 1: 运行 Stage 0 全套脚本**

Run:

```powershell
node scripts/pricing-comparison-view-model.test.mjs
node scripts/add-provider-key-groups.test.mjs
node scripts/test-dashboard-balance-summary.mjs
node scripts/edit-key-page-flow.test.mjs
node scripts/shared-capabilities-contract.test.mjs
node scripts/data-architecture-field-ownership.test.mjs
pnpm.cmd build
```

Expected: all PASS. `pnpm.cmd build` 必须完成 `tsc --noEmit && vite build`。

- [ ] **Step 2: 如未改 Rust 生产代码，不运行 cargo 作为强制门槛**

如果本阶段只改 Node scripts 和 docs，记录：

```markdown
## Rust 检查

本阶段未改 Rust 生产代码；Rust 侧通过 `scripts/shared-capabilities-contract.test.mjs` 做源码契约保护。`cargo check --manifest-path .\src-tauri\Cargo.toml` 留到 Rust service 或 Tauri command 改动阶段执行。
```

如果 Task 5 为了修复契约触碰了 Rust 源码，Run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: PASS.

- [ ] **Step 3: 更新审计文档验证结果**

Append:

```markdown
## Stage 0 验证结果

- `node scripts/pricing-comparison-view-model.test.mjs`: PASS
- `node scripts/add-provider-key-groups.test.mjs`: PASS
- `node scripts/test-dashboard-balance-summary.mjs`: PASS
- `node scripts/edit-key-page-flow.test.mjs`: PASS
- `node scripts/shared-capabilities-contract.test.mjs`: PASS
- `node scripts/data-architecture-field-ownership.test.mjs`: PASS
- `pnpm.cmd build`: PASS

## 下一阶段入口

Stage 1 只能做低风险工具去重，例如 `readError`、时间解析、倍率/金额格式化、status label/tone。不得在 Stage 1 删除字段、改 schema、重写 pricing projection 或迁移多个页面。
```

- [ ] **Step 4: Commit 汇总验证**

Run:

```powershell
git add -- docs/superpowers/audits/2026-07-07-relay-pool-data-architecture-stage0-baseline.md
git commit -m "docs: summarize data architecture stage0 verification"
```

Expected: 只提交审计文档。

- [ ] **Step 5: 最终状态检查**

Run:

```powershell
git status --short
git log --oneline -8
git diff --stat HEAD~4..HEAD
```

Expected:

- `git status --short` 为空。
- 最近提交只包含 Stage 0 tests/docs。
- diff stat 不包含 UI production refactor、schema migration 或无关文件。

## 自检结论

- Spec coverage: 本计划覆盖 Stage 0 要求中的价格分组去重、真实多分组保留、余额 station-scope 优先、Key 编辑保留 group binding、Add Provider/Rust shared capability 边界和兼容字段白名单。
- Placeholder scan: 本计划没有留空任务；每个修改都有具体路径、代码片段、命令和期望结果。
- Type consistency: `groupKeyHash`、`groupIdHash`、`groupBindingId`、`group_name`、`group_id_hash` 保持各自语义，不互相替代。
