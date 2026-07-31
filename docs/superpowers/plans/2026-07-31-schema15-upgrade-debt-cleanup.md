# Schema 15 升级链路技术债清理实施计划

状态：schema15 清债主线已收口；生产架构门禁已通过；本轮技术债清理完成

日期：2026-07-31

适用范围：generation-2 SQLite 启动、schema `15 -> latest`、加密 secret baseline、设备 key 身份校验、升级 journal、旧 settings 修复链路和恢复路由。

> 本计划清理现有实现中的双重决策和遗留旁路，不重写数据层，也不引入通用工作流引擎。所有任务必须按顺序执行；每个任务先建立失败证据，再修改生产代码。

## 执行版摘要

这次清债的目标不是“再补一堆版本补丁”，而是把升级系统固定成一条可复用路线：

```text
read-only probe -> pure planner -> executor(plan) -> postconditions -> ready | typed recovery
```

可靠性、可维护性、可拓展性的落地标准如下：

| 原则 | 必须做到 | 禁止退化 |
|---|---|---|
| 可靠性 | 写入前完成只读 probe、key identity、journal 和 schema 判断；失败进入 typed recovery | wrong/missing key 时创建新 key；把未知状态当可修复状态 |
| 可维护性 | registry/probe/planner/executor/recovery 各自单一职责；旧链路只能作为 step implementation | 在 `lib.rs`、executor 或 UI 中新增 `if schema == N` 补丁 |
| 可拓展性 | schema `15` 是固定自动升级基线，未来普通 schema 只新增 migration、postcondition、fixture 和 release 声明 | 每个版本新增一条启动分支或永久双读/双写 |

当前执行状态：

| 状态 | 内容 |
|---|---|
| 已收口 | D-01 至 D-09；schema15 主路线；旧 recovery/legacy hot-path；production `application::* -> sqlx::*` 禁边 |
| 已有证据 | `pnpm test`、`pnpm test:contracts`、full `cargo test --locked`、`cargo fmt --check`、`cargo check --locked`、`pnpm verify:fast`、schema15 fixture、portable migration focused tests、`pnpm build` |
| 发布前门禁 | 发布版本前仍需在最终工作树完整跑通 `pnpm verify:release` |
| 当前清债结论 | 本轮不发布，因此 Tauri release bundling 签名密钥不阻塞 schema15 清债收口 |
| 可声称完成 | 可声明 schema15 清债完成、source-qualified、production-architecture-qualified；不能声称已完成发布签名打包 |

最新执行证据记录在 `docs/superpowers/audits/2026-07-31-schema15-upgrade-debt-closeout.md`。本轮已确认 `cargo test --locked --manifest-path src-tauri/Cargo.toml`、`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、`cargo check --locked --manifest-path src-tauri/Cargo.toml` 和 `pnpm verify:fast` 通过；PowerShell 设置 `http://127.0.0.1:7890` 代理后 `pnpm verify:release` 已越过 RustSec advisory/license/source gate；设置 `RELAY_POOL_RELEASE_TAG=v0.3.3` 后 release version contract 已通过。因本轮不发布，Tauri release bundling 签名密钥只作为发布前门禁保留，不作为本轮清债阻塞。

本轮已处理：

- updater bridge 未安装时的同步 throw 已改为 promise chain 捕获，避免 Provider 卸载 children 并污染前端测试；
- `pnpm test`、`pnpm test:contracts`、`pnpm verify:fast` 已在当前工作树通过；
- 首次 `pnpm verify:fast` 因 debug exe 占用失败，结束本项目 `relay-pool-desktop.exe` 后重跑通过。

后续发布队列：

1. 发布前重跑 `pnpm verify:release`；涉及 GitHub/RustSec 时在 PowerShell 设置 `http://127.0.0.1:7890` 代理。
2. 发布前提供 Tauri signing key 环境变量，优先使用 `TAURI_SIGNING_PRIVATE_KEY_PATH`，不要把私钥内容写入文档或日志。
3. 只有 `verify:release` 全绿后，才能把发布记录标为 full-release-qualified。

## 当前执行快照

截至 2026-07-31，schema15 升级与旧恢复链路债务 D-01 至 D-09 已具备关闭证据，broader application SQLx 生产路径债务也已由 AST 架构门禁收口：

- schema `15` 发布 fixture 已冻结，并能升级到 latest 后重启；
- latest schema、binary compatibility、fresh/init 路线已从 registry 收口；
- startup probe 会在任何写入前读取 persisted active key ID，并与系统 key identity 比较；
- startup coordinator 会把 planner 产出的 steps 交给 executor，executor 不再二次 probe/plan；
- legacy settings/local-key 转换已迁入 baseline conversion/import 路线，正常 startup 不再调用 `repair_legacy_settings()`；
- settings service 不再接受 unmigrated legacy plaintext local key；
- architecture gate 已覆盖 normal startup legacy 路径、settings legacy plaintext 路径、planner/executor 边界、bounded baseline migration，以及 production `application::* -> sqlx::*` 禁边。

已通过的 schema15 主线验收命令：

```powershell
node scripts/verify-persistence-v2-artifacts.mjs --sqlite src-tauri/tests/fixtures/persistence/schema15/released-schema15.sqlite3 --canary sk-live-canary
cargo test --manifest-path src-tauri/Cargo.toml --test schema15_upgrade_fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml frozen_schema15_fixture_upgrades_to_latest_and_restarts -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml baseline_conversion -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture startup_upgrade -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture normal_startup -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_fault_matrix -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade_recovery -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
pnpm verify:fast
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture -- --nocapture --test-threads=1
pnpm test:contracts
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

当前计划可以标成 schema15 清债完成、source-qualified、production-architecture-qualified。完整发布门禁不属于本轮必要条件；发布前仍必须重跑 Task 9 的 release gate，例如 `pnpm verify:release`，并确认 advisory/license/source、release tag、signing/bundle gate 全部通过。

注意：`rg -n "^use sqlx|sqlx::|QueryBuilder|SqliteConnection" src-tauri/src/application -g "*.rs"` 仍会命中 `#[cfg(test)]` 测试辅助代码中的 seed/assert SQL。这不代表生产路径回退；以 `persistence_architecture` 的 release AST 门禁为准。后续可以独立清理测试 SQLx helper，但不应把它混入 schema15 启动升级协议。

## 目标

把当前已经可用但仍有双源和旧入口的升级实现，收口成唯一、可验证、可延伸的启动协议：

```text
read-only probe -> pure planner -> executor(plan) -> postconditions -> ready | typed recovery
```

完成后必须满足：

- schema `15` 是最低自动升级基线，且永久由发布 fixture 回归；
- 最新 schema、可写 schema 和初始化 metadata 来自同一个 registry；
- 数据库记录的 active key ID 会在任何写入前与系统 active key ID 比较；
- planner 是升级与 journal 恢复策略的唯一所有者；
- executor 不重复 probe、不重新规划、不按版本自行分支；
- schema `17+` 正常启动不再执行 legacy settings repair 或 legacy local-key 导入；
- 新增普通 schema 只增加 migration、postcondition、fixture 路线测试和发布声明，不修改顶层启动编排；
- 所有已知失败使用 typed error/recovery reason，不依赖错误字符串匹配。

## 非目标与重量上限

本次明确不做：

- 不建立插件式 migration 系统、动态 DAG、通用工作流引擎或 repair DSL；
- 不支持 schema `15` 以下自动修复；
- 不自动轮换、重建或猜测已有数据库的设备 key；
- 不把跨设备迁移、导入导出和本机启动升级合并成同一套业务流程；
- 不为了清债重构无关 store、collector、IPC 或前端页面；
- 不为每个历史版本增加一个启动分支。

目标代码形态必须保持为：一个静态 registry、一个 probe 类型、一个 planner、一个 executor、一个 journal 观察入口和一个 recovery reason 枚举。

## 当前审计结论

### 已有可靠基础

- `startup_probe.rs` 已提供只读事实采集；
- `startup_upgrade_plan.rs` 已有纯 planner 和 schema `15` 基线；
- schema `15 -> 16 -> secret baseline -> 17` 的顺序已有回归测试；
- baseline conversion 已有 journal、备份、发布和最终 secret 校验；
- recovery reason 已类型化，并已贯通 IPC/前端；
- schema `15/16/17`、错误 key、journal kind 等关键测试当前可通过。

### 必须清理的债务

| ID | 级别 | 债务 | 当前风险 | 关闭任务 |
|---|---|---|---|---|
| D-01 | P0 | 最新 schema 在 registry、初始化 SQL、binary compatibility 中多源表达 | 新增 `0018.sql` 后可能出现 migration ledger `18`、compatibility `17` | Task 1 |
| D-02 | P0 | `__active_key_id` 只写不读 | 无 secret 的已有数据库可能接受错误 key，并覆盖 key metadata | Task 2 |
| D-03 | P1 | `lib.rs` 规划后丢弃 steps，执行器再次 probe/plan | planner 不是唯一策略源，策略可能漂移 | Task 3-4 |
| D-04 | P1 | schema 15 测试由当前 migration registry 动态截断造库 | 修改历史 migration 时测试可能仍通过，无法代表已发布用户数据 | Task 5 |
| D-05 | P1 | probe 错误被粗略归为 corrupted database | 缺表、锁定、权限、metadata 缺失给出错误恢复指导 | Task 2 |
| D-06 | P2 | 正常启动每次执行 `repair_legacy_settings()` | legacy 补丁长期留在热路径，行为和写入边界不透明 | Task 6-7 |
| D-07 | P2 | `ensure_local_access_key()` 保留 legacy 明文读取 | 新旧职责混合，无法证明 schema `17+` 不走旧转换 | Task 6-7 |
| D-08 | P2 | 没有旧链路删除门禁和未来 migration 合同 | 后续版本容易再次增加旁路补丁 | Task 8 |
| D-09 | P0 | secret baseline/fresh import 内调用完整 migrator | 新增 schema 18 后可能在 secret baseline 提交前提前执行 18 | Task 1 |

## 不可破坏约束

以下规则适用于全部任务：

1. 任何升级写入前必须完成只读 probe 和 plan；probe 不创建文件、不获取写连接、不创建 key。
2. 已有数据库加 missing/wrong key 必须 fail closed；仅经过证明的 fresh install 才能创建 key。
3. 不允许同时维护新旧 metadata，不允许双写后“以后再删”。每个切换任务必须在同一任务内删除旧写入口。
4. executor 只执行传入 plan。executor 内不得调用 planner，不得根据 schema/secret format 重新决定路线。
5. 普通 SQL migration 不引用 secret conversion 内部函数；高风险 secret/settings 转换必须有 journal、verified backup 和 postcondition。
6. 已发布 migration 文件按 append-only 处理。fixture 发现历史 checksum 漂移时停止发布，不自动接受新 checksum。
7. schema `< 15`、未知 schema 和无法可信读取 metadata 的数据库均不得猜测修复。
8. known failure 必须先增加 typed variant；禁止 `contains()`、正则或字符串前缀决定恢复原因。
9. runtime 和 Tauri command state 只能在最终 writable、schema、key identity 和 secret decryptability 验证全部通过后注册。
10. 当前工作区包含其他未提交改动。执行时只 stage 本任务明确路径，不使用 `git add .` 或 `git add -A`。

## 目标职责边界

| 组件 | 唯一职责 | 禁止职责 |
|---|---|---|
| schema/release registry | 声明最低基线、最新 migration、当前 secret format、允许的有序 transition | 打开数据库、执行 SQL、判断具体数据库状态 |
| startup probe | 只读采集 schema ledger、compatibility、secret format、persisted key ID、secret 数量、journal 和 SQLite 状态 | 创建 key、修复 metadata、决定升级步骤 |
| startup planner | 把完整 probe 转为 `Execute(steps)` 或 typed recovery | 文件/数据库 I/O、调用迁移器 |
| startup executor | 严格顺序执行 planner 给出的 steps 并返回 typed result | 再次 probe、再次 plan、自行新增版本分支 |
| step implementation | 完成一个局部 transition、backup/journal 和 postcondition | 决定全局支持窗口或 UI 恢复原因 |
| recovery UI | 展示 typed reason 和用户可控操作 | 解析后端错误字符串、自动改写用户数据 |

## 执行顺序

```text
Task 0  冻结证据和债务清单
  -> Task 1  单一版本 registry
  -> Task 2  完整只读 probe 与 key identity
  -> Task 3  planner 成为唯一策略源
  -> Task 4  executor 纯执行化
  -> Task 5  冻结 schema 15 发布 fixture
  -> Task 6  legacy 转换迁入正式升级步骤
  -> Task 7  删除正常启动 legacy 路径
  -> Task 8  架构门禁与未来 migration 合同
  -> Task 9  fault matrix、发布门禁和收口
```

Task 1-5 是可靠性主线；Task 6-8 是旧代码清理主线。不得先删除旧路径再补 fixture 和 planner 证据。

## Task 0：冻结现状与建立债务清单

**Files:**

- Create: `docs/superpowers/audits/2026-07-31-schema15-upgrade-debt-manifest.json`
- Modify: `docs/superpowers/specs/2026-07-31-schema15-upgrade-recovery-design.md`
- Modify: `docs/release/SCHEMA15_UPGRADE_RECOVERY.md`

**前置条件：**

- 保存 `git status --short`，确认本计划涉及文件是否已有用户改动；
- 运行现有 schema `15/16/17`、wrong-key 和 journal 测试，记录当前通过结果；
- 不把动态生成测试数据库当作 schema 15 发布 fixture。

- [ ] **Step 1：记录可机器读取的 debt manifest**

JSON 至少包含 `id`、`severity`、`owner_file`、`evidence`、`target_task`、`status`。初始登记 D-01 至 D-09，状态全部为 `open`。关闭债务时必须在对应任务提交中更新为 `closed` 并附测试名，不允许只改文档描述。

- [ ] **Step 2：建立当前基线证据**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml startup_upgrade_plan -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml schema_15_generation_two_runs_structural_migration_before_secret_baseline -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml encrypted_generation_two_with_wrong_key_routes_to_key_mismatch -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml observes_persistence_journal_kind_before_recovery_routing -- --nocapture
```

- [ ] **Step 3：修正文档状态**

把现有设计文档中的“已审计完成”改为“主路线已实现，D-01 至 D-09 待关闭”。release gate 在全部任务完成前标记为 blocked，避免文档比实现更乐观。

**退出条件：** 债务有稳定 ID、证据和关闭任务；基线测试结果可复现；未修改生产代码。

**失败/回滚：** 任一基线测试失败时停止，不进入 Task 1；先把失败登记到 manifest，不在本任务顺手修复。

## Task 1：建立单一 schema/release registry

**Files:**

- Create: `src-tauri/src/persistence/schema_registry.rs`
- Modify: `src-tauri/src/persistence/mod.rs`
- Modify: `src-tauri/src/persistence/migrations.rs`
- Modify: `src-tauri/src/services/data_store/startup_upgrade_plan.rs`
- Modify: `src-tauri/tests/persistence_architecture.rs`
- Modify: `docs/superpowers/audits/2026-07-31-schema15-upgrade-debt-manifest.json`

**设计决定：** SQLx embedded migrator 是 schema migration 列表的唯一事实源。`schema_registry` 只组合该列表与显式发布常量，不再复制 latest schema 数字。`current_schema_version()` 必须由 migrator 最大版本计算；初始化 metadata、binary writable schema、planner latest target 全部消费该值。

schema `17` 是需要应用层加密转换才能提交的 transition，不是普通“执行全部 SQL 就完成”的 migration。所有 fresh/import/existing 路线都必须先 bounded 到 structural schema `16`，完成 secret baseline 并提交 `17`，之后才允许 bounded 到 registry latest。baseline implementation 禁止调用无上限的 `migrator().run(...)`。

- [ ] **Step 1：先写 RED 测试**

增加测试证明：

- `current_schema_version() == migrator().max(version)`；
- `current_binary_compatibility().writable_schema == { current_schema_version() }`；
- fresh database 的 `_sqlx_migrations`、`persistence_schema_compatibility.schema_version` 和 `updated_by_migration` 最终一致；
- 架构测试拒绝 `migrations.rs` 中 `schema_version = 17`、`updated_by_migration = 17` 和 `BTreeSet::from([17])` 这类 latest-schema 字面量。
- 架构测试拒绝 `baseline_conversion.rs` 直接运行完整 migrator；
- fresh install、generation-1 import 和 schema 15 upgrade 都按 `16 -> baseline 17 -> latest` 分段，不能在 baseline 期间执行未来 migration。

先运行并确认至少一项因当前硬编码而失败：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture schema_registry -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml persistence::migrations -- --nocapture
```

- [ ] **Step 2：实现最小 registry**

registry 只暴露：

- `MINIMUM_AUTOMATIC_SCHEMA_BASELINE = 15`；
- `latest_schema()`，从 embedded migrator 推导；
- 当前 secret format 常量的单一导出；
- 从 latest schema 构造 binary compatibility 的函数；
- migration 版本连续性、唯一性和最新 migration 存在性的校验。

不要在 registry 中保存数据库路径、连接池、闭包或动态 handler。

- [ ] **Step 3：删除重复版本真相**

- fresh initialization 不再用硬编码 `17` 覆盖 compatibility；
- binary compatibility 的 readable/writable 集合由 registry 构造；
- planner 从 probe 中使用 registry 解析出的 latest，不另存当前版本常量；
- schema 17 的特殊 secret-baseline提交仍由该 step 自己完成，但最终必须由 registry postcondition 校验。

- [ ] **Step 4：封住 baseline 的 migration 上界**

把 `apply_migrations_and_finalize()` 和 `initialize_pre_baseline_runtime_for_import()` 中的完整 migrator 调用替换为显式 bounded API：structural 准备最多到 `16`，baseline step 只登记/提交 `17`，随后由 planner 的 `EnsureLatestSchema` 执行 `18+`。同一 bounded API 必须被 fresh install、generation-1 import 和 existing generation-2 upgrade 复用。

- [ ] **Step 5：关闭 D-01、D-09**

只有在全仓搜索不存在非 fixture/测试语义下的 latest-schema 硬编码时才关闭：

```powershell
rg -n "schema_version = 17|updated_by_migration = 17|BTreeSet::from\(\[17\]\)" src-tauri/src
rg -n -U "migrator\(\).*\.run" src-tauri/src/services/secrets/baseline_conversion.rs
```

**退出条件：** 新增 `0018_*.sql` 时，不修改初始化函数和 binary compatibility 也能令 latest 解析为 18，而且 18 只能在 baseline 17 成功后执行；D-01、D-09 关闭。

**失败/回滚：** registry 校验失败即中止启动/测试，不回退到硬编码版本，也不自动跳过 migration gap。

## Task 2：扩展只读 probe，先判定 key identity 和 typed probe errors

**Files:**

- Modify: `src-tauri/src/services/data_store/startup_probe.rs`
- Modify: `src-tauri/src/services/data_store/startup_upgrade_plan.rs`
- Modify: `src-tauri/src/services/data_store/types.rs`
- Modify: `src-tauri/src/services/secrets/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/ipc/dto/updater_data_recovery.rs`（仅新增 recovery variant 时）
- Regenerate: `src-tauri/src/ipc/dto/updater_data_recovery.typescript.txt`（仅 contract 变化时）
- Modify: `src/lib/types/dataRecovery.ts`（仅 contract 变化时）
- Modify: `docs/superpowers/audits/2026-07-31-schema15-upgrade-debt-manifest.json`

**目标 probe facts：**

- compatibility schema 与 SQLx ledger version；
- latest registry schema；
- secret format metadata：missing / legacy / current / unsupported / malformed；
- persisted active key ID：missing / valid ID / malformed；
- system active key ID：missing / valid ID / access failure；
- secret row count 以及是否存在要求 key 的 encrypted row；
- journal kind/state；
- SQLite quick check；
- probe I/O failure 的 typed category。

- [ ] **Step 1：先写 key identity RED matrix**

至少覆盖：

| 数据库状态 | 系统 key | 预期 |
|---|---|---|
| current format + persisted key A + key A | A | 可规划 verify/open |
| current format + persisted key A + key B | B | `keyMismatch`，零写入 |
| current format + persisted key A + missing | missing | `missingKey`，零写入 |
| current format + missing key ID + 有 encrypted rows | 任意 | `inconsistentSchemaMetadata`，零写入 |
| current format + missing key ID + 空 secrets | key B | `inconsistentSchemaMetadata`，不得补写 B |
| legacy format + 无 key ID | key A | 可规划 baseline conversion |
| proven fresh install | missing | 唯一允许创建新 key 的路线 |

关键回归名应表达空 secret 场景，例如：

```text
existing_encrypted_database_without_secret_rows_rejects_mismatched_active_key_id_before_write
```

- [ ] **Step 2：引入 typed probe error**

定义稳定枚举区分至少：missing database/table、missing migration metadata、invalid metadata、locked/busy、permission denied、SQLite corruption/integrity failure、journal invalid、key-store unavailable。`lib.rs` 只能通过 exhaustive match 映射 recovery reason，不得把所有 `Err(String)` 映射为 `CorruptedDatabase`。

- [ ] **Step 3：把 system key identity 作为 probe 输入事实**

读取 key ID，不读取或复制 key bytes。planner 比较 persisted/system key ID；真正 secret decryptability 仍由最终 verify step 完成。key-store access failure 与 key missing 必须分开，避免把系统故障误报为用户丢 key。

- [ ] **Step 4：证明 probe 只读**

测试 probe 前后数据库文件 hash、settings 行数、key-store 操作计数和 journal 文件状态完全不变。不得通过“打开 writable pool 但不执行 UPDATE”来声称只读。

- [ ] **Step 5：关闭 D-02、D-05**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml startup_probe -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml startup_upgrade_plan -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml key_identity -- --nocapture
pnpm generate:bindings
pnpm build
```

**退出条件：** 任何已有数据库在首个写操作前已有确定 key identity 结论；probe failure 不再统一伪装成 corruption；D-02、D-05 关闭。

**失败/回滚：** 无法读取 key ID、metadata 或 key-store 时进入 typed recovery，不创建 key、不覆盖 `__active_key_id`、不执行 schema migration。

## Task 3：让 planner 成为唯一完整策略源

**Files:**

- Modify: `src-tauri/src/services/data_store/startup_upgrade_plan.rs`
- Modify: `src-tauri/src/services/data_store/startup_probe.rs`
- Modify: `src-tauri/src/persistence/upgrade_recovery_plan.rs`
- Modify: `src-tauri/src/persistence/upgrade_recovery_executor.rs`
- Modify: `src-tauri/src/services/data_store/generation_upgrade.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `docs/superpowers/audits/2026-07-31-schema15-upgrade-debt-manifest.json`

**设计决定：** planner 输出完整且可执行的 value object。步骤应携带所需参数，例如 `SqlSchema { from, to }`、`SecretFormat { from, to }`、`ResumeJournal { kind, phase }`、`OpenRuntime`、`VerifyPostconditions`；不得靠 executor 再读全局状态补全含义。

- [ ] **Step 1：先写纯 planner RED matrix**

至少覆盖 schema `15/16/current/future`、legacy/current secret format、missing/wrong key ID、generation journal 每个 durable phase、baseline journal 每个 durable phase、invalid journal、metadata inconsistency。相同 probe 必须生成完全相同 plan。

- [ ] **Step 2：统一 journal 策略**

将“resume / restart from verified backup / halt”决策并入 startup plan。`generation_upgrade.rs` 可以保留具体 journal action 实现，但不能再根据 journal kind/phase自行选择策略。

- [ ] **Step 3：顶层只规划一次**

`lib.rs` 保留 probe 结果和完整 `StartupUpgradePlan`，将 `Execute(steps)` 原样传给 executor。禁止当前这种只判断 `Execute(_)` 后丢弃 steps 的调用方式。

- [ ] **Step 4：保证计划可诊断但不泄密**

允许记录 step kind、from/to schema、journal phase 和 typed outcome；禁止记录数据库 secret、key ID 全值、路径中的敏感用户信息或任意 SQL 错误原文到前端。

```powershell
cargo test --manifest-path src-tauri/Cargo.toml startup_upgrade_plan -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade_recovery -- --nocapture
```

**退出条件：** 顶层每次启动最多调用 planner 一次；journal 和 schema 路线都可仅凭 plan 审计；D-03 保持 open，待 Task 4 删除 executor 重规划后关闭。

**失败/回滚：** planner 遇到未知状态返回 typed halt/recovery；不允许 executor “尽量继续”。

## Task 4：把 executor 收缩为严格的 plan 执行器

**Files:**

- Create: `src-tauri/src/services/data_store/startup_upgrade_executor.rs`
- Modify: `src-tauri/src/services/data_store/mod.rs`
- Modify: `src-tauri/src/services/data_store/generation_upgrade.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/persistence_architecture.rs`
- Modify: `docs/superpowers/audits/2026-07-31-schema15-upgrade-debt-manifest.json`

- [ ] **Step 1：先写 executor contract RED tests**

用 fake step runner/fault injector 证明：

- steps 严格按 plan 顺序且最多执行一次；
- 某 step 失败后后续 step 不执行；
- `OpenRuntime` 前不得注册 runtime；
- final postconditions 失败时不返回 ready；
- executor 源码/AST 中不能调用 `probe_upgrade_state*` 或 `plan_upgrade`；
- `lib.rs` 不得丢弃 `Execute(steps)`。

- [ ] **Step 2：提取 executor**

`startup_upgrade_executor.rs` 只负责 dispatch 和 typed error boundary。`generation_upgrade.rs` 中 generation-1 import、baseline conversion、backup/journal 的成熟 step implementation 可暂留，不复制到新模块。

- [ ] **Step 3：删除重复 probe/plan**

删除 `open_and_validate_v2()` 内的 probe/plan。将它拆为 planner 可调用的 step implementation，如 bounded schema migration、baseline conversion、runtime open、writable verify、key/secret verify。

- [ ] **Step 4：统一最终 postconditions**

ready 前一次性验证：

- SQLite quick/integrity check；
- SQLx ledger == registry latest；
- compatibility schema == registry latest；
- secret format == registry current；
- persisted key ID == system active key ID；
- 所有 encrypted rows 可解密且 binding/constraint 有效；
- runtime open mode == writable；
- 无未完成 journal。

- [ ] **Step 5：关闭 D-03**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml startup_upgrade_executor -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml schema_15_generation_two_runs_structural_migration_before_secret_baseline -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture startup_upgrade -- --nocapture
```

**退出条件：** 一个 plan 对应唯一执行序列；executor 无策略判断和二次 probe；D-03 关闭。

**失败/回滚：** 普通 transactional SQL migration 依赖 SQLite rollback；高风险 step 依赖 journal 和 verified backup；执行失败统一返回 typed outcome，绝不吞错后打开 runtime。

## Task 5：冻结 schema 15 发布 fixture 和 checksum manifest

**Files:**

- Create: `src-tauri/tests/fixtures/persistence/schema15/released-v0.3.1.sqlite3`
- Create: `src-tauri/tests/fixtures/persistence/schema15/manifest.json`
- Create: `src-tauri/tests/schema15_upgrade_fixture.rs`
- Modify: `.gitignore`
- Modify: `scripts/architecture/check-fixtures.mjs`
- Modify: `docs/superpowers/audits/2026-07-31-schema15-upgrade-debt-manifest.json`

**fixture 约束：** fixture 必须由实际发布版本生成一次，使用纯合成数据并经过 secret/token/cookie 扫描。测试运行时只复制 fixture，不得调用当前 `migrator_through(15)` 重新生成。`.gitignore` 只允许该精确测试目录下被 manifest 登记的数据库，不放开全局 `*.sqlite3`。

- [ ] **Step 1：先写 fixture gate RED test**

在 fixture 尚不存在时，测试必须因 missing artifact/manifest 失败。manifest 至少记录 release version、schema、SQLx migration checksums、文件 SHA-256、合成数据断言和生成命令 revision。

- [ ] **Step 2：从发布构建生成并审计 fixture**

fixture 应至少包含：普通 settings、一个 legacy local access key 场景、一个可验证 legacy encrypted secret、外键关系和 migration canary。不得包含任何真实 key、cookie、路径、用户名或日志。

- [ ] **Step 3：建立不可漂移测试**

测试先校验 fixture SHA-256 和 schema 1-15 migration checksum，再复制到 tempdir，执行正式 startup plan，验证数据等价、secret 可解密、metadata 到 latest、重启幂等、备份可读。

- [ ] **Step 4：加入负向 fixture 校验**

`check-fixtures.mjs` 必须拒绝：未登记 `.sqlite3`、hash 不匹配、明显 secret pattern、WAL/SHM sidecar、绝对用户路径和超出大小上限的 fixture。

- [ ] **Step 5：关闭 D-04**

```powershell
node scripts/architecture/check-fixtures.mjs
cargo test --manifest-path src-tauri/Cargo.toml --test schema15_upgrade_fixture -- --nocapture
```

**退出条件：** schema 15 主回归不依赖当前历史 migration 重新造库；历史 migration 被改写时 fixture checksum 测试失败；D-04 关闭。

**失败/回滚：** fixture checksum 漂移视为发布阻断。只能提交新 fixture 版本并保留旧 fixture，或恢复误改的历史 migration；不得直接覆盖 manifest hash 来“修测试”。

## Task 6：把 legacy settings/local-key 转换纳入正式升级步骤

**Files:**

- Modify: `src-tauri/src/services/data_store/startup_probe.rs`
- Modify: `src-tauri/src/services/data_store/startup_upgrade_plan.rs`
- Modify: `src-tauri/src/services/data_store/startup_upgrade_executor.rs`
- Modify: `src-tauri/src/services/secrets/baseline_conversion.rs`
- Modify: `src-tauri/src/application/settings.rs`
- Modify: `src-tauri/src/persistence/settings_compat.rs`
- Modify: `src-tauri/src/persistence/stores/settings_store.rs`

**设计决定：** legacy settings/local access key 转换属于明确的一次性 data-format transition，不属于每次启动后的 service repair。转换要么并入现有 secret baseline step，要么作为紧随其后的静态 `FinalizeLegacySettings` step；只能选一个所有者。

- [ ] **Step 1：先写发布 fixture RED tests**

覆盖 legacy key 非空、空值、`sk-local-pool-change-me` placeholder、已有 encrypted local key、重复执行和中途失败。断言转换后明文字段被清空/废弃、encrypted binding 唯一、业务值保持、第二次执行零写入。

- [ ] **Step 2：声明 step precondition/postcondition**

precondition 必须来自 probe；postcondition 至少包括 encrypted local key 存在、legacy 明文不再被 runtime 读取、placeholder 不会成为有效凭据、转换 metadata 已提交。

- [ ] **Step 3：纳入 journal/backup 边界**

该转换与 secret baseline 共用同一原子发布边界时复用现有 journal；若在 baseline 后单独执行，则必须有独立 typed phase 和 verified backup，不能靠启动后 service write 补偿。

- [ ] **Step 4：收缩 SettingsService**

`ensure_local_access_key()` 只允许两种状态：读取已有 encrypted key；对 proven fresh install 创建新 key。它不再读取 legacy 明文。正常设置更新仍使用现有 encrypted store。

```powershell
cargo test --manifest-path src-tauri/Cargo.toml application::settings -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml schema_15_generation_two_runs_structural_migration_before_secret_baseline -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test schema15_upgrade_fixture -- --nocapture
```

**退出条件：** legacy settings/local key 只有一个明确升级所有者；正常 service 不再兼容读取明文。D-06、D-07 暂不关闭，待 Task 7 删除调用和 API。

**失败/回滚：** conversion 失败恢复 verified backup；不得留下 encrypted/legacy 两份都可能被读取的状态。

## Task 7：删除正常启动中的 legacy repair 路径

**Files:**

- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/application/settings.rs`
- Modify: `src-tauri/src/persistence/stores/settings_store.rs`
- Delete or restrict: `src-tauri/src/persistence/settings_compat.rs`
- Modify: `src-tauri/src/persistence/legacy_import/import.rs`
- Modify: `src-tauri/tests/persistence_architecture.rs`
- Modify: `docs/superpowers/audits/2026-07-31-schema15-upgrade-debt-manifest.json`

- [ ] **Step 1：先写旧链路不可达 RED gate**

使用现有 `syn` AST 测试设施断言：

- Tauri setup/ready 路径不引用 `repair_legacy_settings`；
- `SettingsService` 不公开 runtime repair API；
- `ensure_local_access_key` 不引用 `legacy_local_access_key_value` 或 placeholder；
- legacy import 只在 generation-1 import 或 planner 指定 transition 中可达；
- schema `17+` ready path 不调用 legacy repair/import/conversion API。

- [ ] **Step 2：删除热路径调用和无主 API**

从 `lib.rs` 删除每次启动的 `repair_legacy_settings()`。删除只为该路径存在的 service/store 方法。若 `settings_compat.rs` 仅剩 generation-1 import 使用，将其移动到 `persistence/legacy_import/` 私有模块；若无调用则删除文件。

- [ ] **Step 3：用行为测试证明没有功能回退**

fresh install 仍创建 encrypted local access key；schema 15 fixture 升级后仍保留原 key；schema current 重启不产生 settings 写入；local proxy 能读取已加密 key。

- [ ] **Step 4：关闭 D-06、D-07**

```powershell
rg -n "repair_legacy_settings|legacy_local_access_key_value|INSECURE_LOCAL_KEY_PLACEHOLDER" src-tauri/src
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture legacy_startup -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml application::settings -- --nocapture
```

搜索结果只能位于明确 allowlist 的 generation-1 import 私有模块或测试 fixture；D-06、D-07 关闭。

**退出条件：** schema current 正常启动没有 legacy repair 写入；旧转换只通过正式 plan 可达。

**失败/回滚：** 不恢复热路径 repair。缺失转换能力时回到 Task 6 完善正式 step 和 fixture，再继续删除。

## Task 8：建立架构门禁和未来 schema authoring contract

**Files:**

- Modify: `src-tauri/tests/persistence_architecture.rs`
- Create: `docs/SCHEMA_UPGRADE_AUTHORING.md`
- Modify: `docs/README.md`
- Modify: `docs/release/SCHEMA15_UPGRADE_RECOVERY.md`
- Modify: `package.json` 或 `scripts/verify.ps1`（按现有验证入口接入）
- Modify: `docs/superpowers/audits/2026-07-31-schema15-upgrade-debt-manifest.json`

- [ ] **Step 1：先让架构 gate 对当前旁路失败**

gate 必须检查：

- latest schema 不得在 registry 外硬编码；
- production 代码只有 startup coordinator 可调用 planner；
- executor 不能调用 probe/planner；
- runtime setup 不能调用 legacy repair/import；
- recovery UI/backend 不存在字符串分类；
- migration 文件版本连续且已发布 checksum 不变；
- schema baseline `15` 不因新增 migration 自动变化。

- [ ] **Step 2：写未来 migration 合同**

`docs/SCHEMA_UPGRADE_AUTHORING.md` 明确普通 schema `N -> N+1` 的唯一流程：

1. 新增一个 append-only SQL migration，并在 migration 内更新 compatibility metadata；
2. 声明 postcondition，不编辑顶层 startup；
3. 运行 `15 -> latest` frozen fixture 和 `N -> N+1` focused test；
4. 运行 interruption/idempotency 测试；
5. 更新 release gate 中 latest schema，不移动最低基线；
6. 通过 architecture gate。

secret-format 变更必须单独声明 key precondition、journal phase、backup、decryptability postcondition 和 typed recovery，不塞进普通 SQL migration。

- [ ] **Step 3：加入反例代码评审清单**

以下任一情况直接拒绝合并：

- `if schema == N` 出现在 startup coordinator/executor；
- 新增 migration 同时修改 Tauri setup 或前端 recovery routing policy；
- existing database 路径调用 key creation；
- migration 成功但没有 postcondition；
- 修改历史 migration 后只更新 checksum；
- 为兼容旧数据增加永久 runtime 双读/双写；
- known failure 经 `String` 穿过模块边界。

- [ ] **Step 4：接入 fast/release verification**

architecture gate 进入 `verify:fast`；frozen fixture、fault matrix 和完整编译进入 `verify:release`。

- [ ] **Step 5：关闭 D-08**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture startup_upgrade -- --nocapture
pnpm verify:fast
```

**退出条件：** 未来开发者只读 authoring contract 就能新增 schema；旁路代码在 CI 中失败；D-08 关闭。

**失败/回滚：** gate 误报时修正 AST 规则或建立最小、带理由的 allowlist；不得整项跳过 gate。

## Task 9：完整 fault matrix、发布验证和文档收口

**Files:**

- Modify: `src-tauri/tests/persistence_fault_matrix.rs`
- Modify: `src-tauri/tests/schema15_upgrade_fixture.rs`
- Modify: `docs/release/SCHEMA15_UPGRADE_RECOVERY.md`
- Modify: `docs/superpowers/specs/2026-07-31-schema15-upgrade-recovery-design.md`
- Modify: `docs/superpowers/audits/2026-07-31-schema15-upgrade-debt-manifest.json`
- Create: `docs/superpowers/audits/2026-07-31-schema15-upgrade-debt-closeout.md`

- [ ] **Step 1：补齐 fault matrix**

每个 durable step 至少在“写前、写中、fsync/rename 后、postcondition 前”注入失败，验证重启结果只能是：从 verified backup 重试、从 journal 确定性续跑、或 typed recovery。不得出现半 ready。

矩阵至少覆盖：

- schema 15 bounded migration；
- secret baseline candidate build/publish；
- active key metadata 写入；
- legacy settings/local-key finalize；
- latest schema migration；
- final metadata/secret/runtime verification；
- database locked、只读目录、磁盘写失败、journal truncate、backup hash mismatch；
- missing key、wrong key、key-store unavailable；
- 两次连续启动和 upgrade 后 crash/restart。

- [ ] **Step 2：执行完整验证**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml startup_upgrade -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test schema15_upgrade_fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_fault_matrix -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade_recovery -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture -- --nocapture
pnpm generate:bindings
pnpm build
# Required before publishing a release from this area:
pnpm verify:release
```

- [ ] **Step 3：完成真实升级 smoke**

在隔离 data dir 中依次验证：schema 15 fixture 升级、升级后重启、current schema 重启、错误 key 启动、缺 key 启动、恢复后再次启动。只记录脱敏 schema/reason/step 证据，不提交数据库、key 或日志。

- [ ] **Step 4：关闭 manifest 并更新发布文档**

所有 D-01 至 D-09 必须为 `closed`，每项附测试证据。设计文档更新为最终职责图和实际文件；清债 closeout 可在 source、architecture、fixture、fault 和 fast verification 全部通过后关闭。实际发布记录只有在 `pnpm verify:release` 全绿后才能标为 full-release-qualified。

**退出条件：** 自动化、fixture、fault injection、架构 gate 和真实 smoke 五类证据全部存在；旧路径不可达；文档与实现一致。

**失败/回滚：** 任一 source、fixture、fault 或 architecture gate 失败则清债不得关闭。发布前 `verify:release` 失败则发布记录保持非 full-release-qualified。不得因“普通启动看起来正常”跳过 fault/fixture/architecture 失败。

## 未来版本的固定变更预算

普通 schema `18` 的预期改动上限：

| 必改 | 通常不应改 |
|---|---|
| `0018_*.sql` | `src-tauri/src/lib.rs` |
| transition postcondition/测试 | startup probe/orchestrator 的控制流 |
| schema 15 fixture 路线期望 | recovery UI 枚举 |
| release gate latest 声明 | secret baseline 内部实现 |

secret format `2` 可以增加一个静态 transition 和对应 journal/postcondition，但不得修改普通 schema migration 的职责。超过上述变更面时，必须先更新设计文档，解释为什么现有状态模型无法表达该变化；不能直接增加启动补丁。

## 计划自审

### 可靠性

- 在首个写操作前验证 schema、journal、secret format 和 key identity；
- existing database 永不静默创建/替换 key；
- frozen release fixture 防止“用当前代码证明当前代码正确”的循环测试；
- bounded migration 保证 schema 18 不会越过 secret baseline 提前执行；
- durable step 由 backup、journal、postcondition 和 fault matrix 四层证据覆盖；
- 无法证明安全的状态 fail closed 到 typed recovery。

结论：满足。主要残余风险是 OS key-store 和真实磁盘故障无法由单元测试完全模拟，因此 Task 9 保留隔离 data dir smoke 和平台发布验证。

### 可维护性

- registry、probe、planner、executor 各有单一所有者；
- 先迁移 legacy 行为，再删除热路径和无主 API，避免永久双读/双写；
- debt manifest 让“暂留旧代码”有编号、退出任务和测试证据；
- AST architecture gate 防止后续又把版本判断塞回 `lib.rs` 或 executor；
- 每个任务都有明确文件、RED 测试、验证命令、退出条件和失败语义。

结论：满足。计划不要求一次重写 generation-upgrade 的成熟 journal/backup 实现，只把它降为 step implementation，控制了清债范围。

### 可拓展性

- latest schema 从 embedded migration registry 推导；
- 普通 schema、secret format 和 generation import 是不同 transition 类型；
- future schema 使用同一条 `15 -> latest` 路线，不增加顶层分支；
- authoring contract 和固定变更预算把扩展成本限制在 migration、postcondition、测试和 release 声明；
- 遇到现有状态模型表达不了的新变化时先更新设计，而不是旁路打补丁。

结论：满足。这里的“可拓展”是可预测地增加静态 transition，不是引入动态框架；这与本地桌面工具的规模匹配。

### 不过度设计

- 新增的长期生产模块只有小型 registry 和 executor；
- probe/planner 使用 enum/value object，不引入数据库内任务队列或脚本引擎；
- fixture 和 architecture gate 属于测试/发布资产，不增加运行时负担；
- legacy 代码净删除，预期生产路径复杂度应下降而不是上升。

结论：满足。若实施后 production startup 模块总分支数或公开 API 数增加，应视为 Task 9 closeout 失败并继续收缩。

## Task 10：broader application SQLx debt

**状态：production boundary 已清；测试辅助 SQLx 后续可选收敛。**

完整 `persistence_architecture` 已在单线程模式通过，并确认 release AST 中没有 `application::* -> sqlx::*` 禁边。这个债务早于 schema15 清理，原范围覆盖 monitoring/query/write-path/settings/collector/provider draft 等历史 application service；当前生产路径已经通过下沉 store/repository 和 boundary manifest 收口。

剩余的 `rg` 命中来自 `#[cfg(test)]` 测试辅助代码中的数据库 seed/assert。它们不是 schema15 发布阻断，也不是 production 架构破口。若后续要继续降低测试维护成本，可以把这些 SQL seed/assert 收拢成 persistence test helper，但必须作为单独测试清理任务处理。

**历史 first-pass inventory：**

- `src-tauri/src/application/monitoring/queries.rs`
- `src-tauri/src/application/monitoring/service.rs`
- `src-tauri/src/application/monitoring/write_path.rs`
- `src-tauri/src/application/settings.rs`
- `src-tauri/src/application/request_logs.rs`
- `src-tauri/src/application/collectors.rs`
- `src-tauri/src/application/provider_drafts.rs`
- `src-tauri/src/application/credentials.rs`
- `src-tauri/src/application/data_migration/export_service.rs`
- `src-tauri/src/application/data_migration/import_service.rs`

**已执行步骤：**

1. 冻结 inventory 并区分 production/test-only 命中：

```powershell
rg -n "^use sqlx|sqlx::|QueryBuilder|SqliteConnection" src-tauri/src/application -g "*.rs"
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture -- --nocapture --test-threads=1
```

2. 按 bounded context 下沉 production SQL：

- monitoring read model：把 `monitoring::queries` 的 SQL reader 下沉到 `persistence::stores::monitoring::status_queries`，application 只保留 DTO composition、clock、pagination 和 input validation；
- monitoring write path：把 execution commit transaction script 下沉到 persistence store，application 只提交 `BufferedExecution`；
- settings/local proxy：settings service 不直接查询 SQL，改为调用 `SettingsStore` 方法；
- collectors/provider drafts/credentials：把测试辅助 SQL 与 production write path 分离，production 统一走 store/repository API；
- data migration service：导出/导入包的 SQLite 验证 SQL 放到 portable migration/persistence validator，application 只编排状态机。

3. 每下沉一个 bounded context，补 architecture focused test 或扩大现有 manifest，禁止用 allowlist 掩盖 production `application -> sqlx`。

4. 最终验收证据：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture -- --nocapture --test-threads=1
pnpm verify:fast
```

**退出条件：** `rg -n "^use sqlx|sqlx::|QueryBuilder|SqliteConnection" src-tauri/src/application -g "*.rs"` 只允许出现在 `#[cfg(test)]` 测试辅助代码，完整 `persistence_architecture` 单线程通过。

**后续可选清理：** 把 test-only SQLx seed/assert 抽到 `persistence::test_support` 或 integration fixture helper，减少测试重复 SQL。这个清理不改变生产升级路线，不作为 schema15/full-architecture qualification 的前置条件。

## 分批提交建议

每个提交只关闭一个可独立验证的风险：

1. `docs: register schema upgrade cleanup debt`
2. `refactor: centralize schema release registry`
3. `fix: verify persisted device key identity before upgrade`
4. `refactor: make startup planner own recovery policy`
5. `refactor: execute startup upgrade plans once`
6. `test: freeze released schema15 upgrade fixture`
7. `refactor: migrate legacy settings in upgrade step`
8. `refactor: remove legacy startup repair path`
9. `test: enforce schema upgrade architecture contract`
10. `docs: qualify schema15 upgrade debt cleanup`
11. `refactor: move application SQLx queries behind persistence stores`

每次只使用 `git add -- <明确路径>`。提交前执行 `git diff --cached --name-only`，确认没有夹带 NewAPI、collector、UI 或其他用户改动。

## 最终通过判据

只有同时满足以下条件，才可以声称升级债务已经清理：

- [ ] D-01 至 D-09 全部关闭并有自动化证据；
- [ ] production `application::* -> sqlx::*` 禁边由 `persistence_architecture` 证明关闭；test-only SQLx 命中已标注为非阻断；
- [ ] schema 15 frozen fixture 可升级到 registry latest，数据等价且可重复重启；
- [ ] 空 secret 的 existing database 也会在写前拒绝 wrong/missing key identity；
- [ ] 顶层只 probe/plan 一次，executor 不 probe、不 plan；
- [ ] schema current 启动不执行 legacy repair，不产生无业务必要的 settings 写入；
- [ ] 所有 durable phase 都有确定性 crash recovery 测试；
- [ ] known failure 全部 typed，前端不匹配错误字符串；
- [ ] 新增普通 schema 不要求修改 Tauri setup 或新增版本分支；
- [ ] `cargo fmt`、`cargo check`、focused Rust tests、`pnpm build` 和清债范围内 verification 全部通过；
- [ ] full Rust/architecture tests 的失败必须全部归零；发布验证未跑完时只能声明非 full-release-qualified，但不阻塞非发布清债 closeout；
- [ ] release 文档声明与 registry、fixture 和生成 bindings 一致。

这套标准优先保证“失败时不破坏、不误写、可解释”，其次才是自动修复率。无法证明安全的状态进入 typed recovery，是可靠性设计的一部分，不是升级失败。
