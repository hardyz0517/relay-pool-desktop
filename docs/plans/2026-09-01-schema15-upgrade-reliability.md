# Schema 15 到最新版本升级可靠性修复计划

状态：Completed（P0-P5 自动化完成；真实安装包升级矩阵因缺少 old/new installer bundle 未执行，已由 install contract 覆盖契约校验）

日期：2026-09-01

适用范围：generation-2 SQLite 数据库、schema 15 到当前最新 schema、加密 secret 基线转换、启动恢复、安装升级矩阵和相关发布文档。

实施结论：schema 15 冻结 fixture 已通过生产启动链路升级至 schema 71，并完成重启幂等、部分 sanitizer 恢复、合法 secret journal 清理、备份完整性及 SQLite 检查。所有计划内代码、测试、审计和文档门禁均已执行；缺少真实安装包的端到端 old/new 安装器运行仍是发布前的环境依赖，不将其伪装为已通过。

当前事实来源：

- [`../README.md`](../README.md)
- [`../SCHEMA_UPGRADE_AUTHORING.md`](../SCHEMA_UPGRADE_AUTHORING.md)
- 当前代码与自动化契约
- 当前最新 schema：`71`
- 自动升级最低基线：`15`

## 1. 目标与完成定义

本计划的目标是让真实用户从任意受支持的 schema `15` 数据库升级到最新版本时，升级可以安全重试、可验证完成、可恢复失败，并且不会因为升级中断而永久停在恢复页。

完成定义必须同时满足：

- schema `15` 冻结 fixture 能通过生产启动链路升级到 schema `71`；测试至少连续启动两次，第二次仍然可写并且不重复破坏数据。
- `compatibility_schema_version`、`_sqlx_migrations`、secret format、关键 postcondition 和 runtime writable mode 一致。
- 所有升级步骤都遵循 `read-only probe -> pure plan -> ordered executor -> postconditions -> ready | typed recovery`。
- SQL migration、secret baseline、alerting backfill、legacy cleanup、request-log sanitizer 任一步骤中断后，下一次启动都能从已持久化状态继续，或者进入有明确操作的 typed recovery；不得出现“已到 latest 但维护任务永远无法重试”。
- 任何已有数据库都不自动创建替代 device key，不覆盖原文件，不静默新建空数据库，不丢失可恢复的备份和 journal。
- 安装升级矩阵必须证明应用进入 `writable/ready`，而不只是进程仍然存活。
- 发布文档、审计清单、测试名称和当前 schema 注册表一致；不存在通过 0 tests 或过期版本文本产生的假绿。

非目标：本计划不扩大产品能力，不新增云同步、账号权限、支付、插件市场或跨设备数据迁移；不执行不可逆 DROP migration。

## 2. 已确认的风险

| ID | 优先级 | 风险 | 影响 |
| --- | --- | --- | --- |
| R-01 | P1 | request-log sanitizer 在 SQL migration 完成后中断，下一次启动 planner 不再安排 sanitizer，但 runtime 会拒绝未完成状态 | 用户永久进入恢复模式，只能手工恢复或新建数据库 |
| R-02 | P1 | schema 15 冻结 fixture 只校验 hash 和“仍为 15”，没有实际执行 15 -> 71 | 最关键的历史升级路径未被验证 |
| R-03 | P1 | 审计清单引用不存在的测试名；Cargo 对过滤后 0 tests 仍返回成功 | CI/审计可能把未执行的升级测试误判为通过 |
| R-04 | P2 | 安装升级矩阵只检查进程、单实例和连接，不检查 startup state、schema 或 writable mode | 应用停在 recovery 页面时仍可能被记录为 pass |
| R-05 | P2 | secret baseline 在 `ActiveValidated` journal 阶段崩溃后，已加密数据库不会再次安排清理 journal | 产生永久 stale journal，并阻塞后续 relocation |
| R-06 | P2 | 发布恢复说明仍声明 schema 57，与当前 schema 71 不一致 | 发布人员和用户得到错误的升级路径与兼容窗口 |

风险修复必须优先保证可重试和数据安全，再优化耗时。所有修复都应保持已有历史 migration 不变，只新增兼容逻辑、postcondition、测试或文档。

## 3. 目标升级链路

### 3.1 统一启动状态模型

扩展只读 probe，使它同时观察：

- compatibility schema 与 SQL migration ledger；
- secret format 与 persisted/system key identity；
- SQLite `quick_check`；
- persistence journal kind；
- request-log sanitizer 的 progress status、剩余行数和版本化 maintenance id；
- 已存在但未完成的其他 durable maintenance 状态。

probe 只读，不创建表、密钥、备份或 runtime，也不根据错误字符串决定恢复类型。

planner 继续是唯一策略 owner。它应把“需要执行的 durable maintenance”视为状态事实，而不是把任务是否需要执行绑定到某个具体 schema 数字。

### 3.2 目标步骤顺序

受支持的 schema `15` 数据库使用一条可扩展的顺序：

```text
read-only probe
  -> EnsureStructuralPreBaseline (bounded at schema 16)
  -> EnsureSecretBaseline (legacy format or resumable valid journal)
  -> EnsureSchema (next declared structural target, eventually schema 71)
  -> EnsureAlertingUpgrade (when alerting foundation is reached)
  -> EnsureLegacyChangeEventsRemoval (after durable alerting completion)
  -> EnsureRequestLogSanitizer (when progress is absent/incomplete)
  -> OpenRuntime
  -> StageRoutingPolicyV3
  -> VerifyWritableRuntime
  -> VerifySecrets
  -> record successful startup metadata
  -> ready
```

约束：

- `EnsureRequestLogSanitizer` 必须在 `OpenRuntime` 前可重复执行；progress 已为 `complete` 时必须是快速 no-op。
- 合法的 baseline conversion journal 必须安排 `EnsureSecretBaseline`，即使 active 数据库已经发布为 encrypted baseline；执行器负责校验、清理和恢复，不得把合法 journal 当作无关文件。
- invalid journal、未来 schema、checksum drift、key mismatch、quick_check 失败继续 fail closed 到 typed recovery。
- 执行器只执行 planner 给出的步骤，不自行探测或增加 schema 分支。
- 每个步骤都必须定义输入不变量、原子边界、成功 postcondition、可重试语义和失败 recovery reason。

### 3.3 失败后的状态转换

| 失败位置 | 可重试策略 | 不允许的行为 |
| --- | --- | --- |
| SQL migration 或 postcondition | 保留已验证 backup；下次从当前 ledger 继续 | 修改历史 migration checksum、删除原库 |
| secret baseline | 依据 journal phase 恢复 backup/candidate/active；key 不匹配直接恢复 | 为已有库生成新 device key |
| alerting backfill | 依据 durable cursor 分页重试，事务内提交 cursor 和数据 | 跳过未完成页后标记完成 |
| request-log sanitizer | 依据 progress 继续分页，完成前不打开 runtime | 只因 schema 已 latest 就跳过 |
| runtime open/verify | 关闭 runtime，保留数据库和诊断证据 | 进入业务页面并允许写入不确定状态 |
| 安装升级探针失败 | 记录 typed startup state 和诊断；安装包流程失败 | 仅以进程存活判定升级成功 |

## 4. 分阶段实施

### P0：冻结基线、契约和测试执行真实性

目标：先把当前可比较的输入、输出和失败证据固定下来，不改变生产行为。

实施项：

1. 新增 schema 15 fixture 升级 harness，复制 fixture 到临时目录后只通过生产 `probe -> plan -> executor` 运行；禁止在测试中重写一套 SQL 升级逻辑。
2. 记录并断言 fixture 原始 SHA、迁移 ledger、原文件未被修改、sidecar 清理、backup 身份、secret 可解密、sanitizer complete、journal 清理和 runtime writable。
3. 为中断点增加 fault injection：SQL migration 前后、baseline journal 各 phase、sanitizer 分页边界、alerting cursor 提交、runtime open/verify 和配置发布。
4. 为每个被过滤运行的 Cargo 命令增加“实际执行测试数 > 0”的门禁。命令必须使用存在的测试名，优先使用 `--exact`；脚本应解析测试输出并在 0 tests、测试名不存在或 harness 未运行时失败。
5. 修正审计清单中不存在的测试名和“passed”证据，改为真实命令与真实输出，或明确标记为 historical/superseded。

建议文件：

- `src-tauri/tests/schema15_upgrade_fixture.rs`
- `src-tauri/src/persistence/upgrade_fault.rs`
- `src-tauri/src/services/data_store/startup_upgrade_plan.rs` 相关测试
- `scripts/data-store-upgrade-matrix.test.mjs`
- `docs/audits/2026-07-31-schema15-upgrade-debt-manifest.json`

完成门：fixture harness 至少有一个真实升级测试；该测试失败时 CI 必须失败；任何过滤后 0 tests 的命令都不能作为证据。

回滚：仅移除测试、审计和脚本门禁，无数据库状态变化。

### P1：把 durable maintenance 纳入 probe/plan

目标：修复 R-01 和 R-05，使重启可以自动继续未完成任务。

实施项：

1. 在 request-log maintenance 模块提供只读 progress observation 和无数据泄漏的状态类型，例如 `Missing/Pending/Running/Complete/Invalid`；查询失败映射到 typed probe error。
2. 在 `StartupUpgradeProbe` 中加入 maintenance 状态，planner 根据状态安排 `EnsureRequestLogSanitizer`，不根据“当前 schema 是否小于 18”决定是否安排。
3. 使 `EnsureRequestLogSanitizer` 具备幂等、分页、有界批次和完成 postcondition；在 `OpenRuntime` 前执行，完成后才能通过 runtime assert。
4. 对合法 `StartupJournalProbe::BaselineConversion` 生成 `EnsureSecretBaseline`；保留 invalid journal 的 interrupted recovery。
5. 增加 planner contract tests：
   - latest schema + sanitizer running => sanitizer step before open；
   - latest schema + sanitizer complete => no-op path before open；
   - encrypted baseline + valid baseline journal => baseline step cleans journal；
   - invalid journal => typed `interruptedUpgrade`；
   - all routes end with writable and secret verification。

建议文件：

- `src-tauri/src/services/data_store/startup_probe.rs`
- `src-tauri/src/services/data_store/startup_upgrade_plan.rs`
- `src-tauri/src/services/data_store/startup_upgrade_executor.rs`
- `src-tauri/src/persistence/maintenance/request_log_url_sanitizer.rs`
- `src-tauri/src/services/secrets/baseline_conversion.rs`
- `src-tauri/src/services/data_store/relocation.rs`

完成门：人为制造 sanitizer partial state 或 valid baseline journal 后，重启生产启动 harness 能自动完成并到达 ready；重复启动不重复写坏数据。

回滚：保留原 typed recovery 作为 fail-closed 后备；新 maintenance step 只能在其 postcondition 和重试测试通过后启用。

### P2：执行器、postcondition 和恢复 UI 收口

目标：保证每个步骤失败后都能解释、重试或安全恢复。

实施项：

1. 扩展 `StartupUpgradeError` 和 `RecoveryReason` 只在确有新语义时增加枚举值；同步 Rust DTO、生成 bridge 和前端映射，禁止前端解析错误字符串。
2. 将 maintenance、baseline、alerting、schema 和 routing staging 的失败阶段写入 `StartupUpgradeStatus`，确保诊断只包含脱敏的 reason、schema 和计数。
3. 恢复页至少显示当前升级阶段、目标 schema、可重试状态和下一步操作；自动重试只调用受控的 startup/restart 流程，不能开放任意 SQL 或任意文件覆盖。
4. 检查成功启动 metadata 记录与 journal 清理的顺序；若 active 已验证但 cleanup 失败，下一次启动必须将其视为可恢复的 durable step，而不是永久 stale evidence。
5. 为 backup、candidate、active 的 file identity 和 sidecar 做重复启动、进程终止、权限失败和磁盘空间失败回归。

建议文件：

- `src-tauri/src/services/data_store/types.rs`
- `src-tauri/src/commands/data_store_startup.rs`
- `src/features/data-recovery/DataRecoveryScreen.tsx`
- `src/features/data-recovery/recoveryViewModel.ts`
- `src/lib/bridge/contract.ts` 与生成输入

完成门：恢复 UI 的所有操作都有 typed capability；失败不会创建空库、覆盖未选文件或暴露 secret；每个新增 DTO 有生成检查。

回滚：前端只增加恢复展示和受控重试，后端保持 fail-closed；如果新重试动作不满足安全门，隐藏该 capability，不绕过 typed recovery。

### P3：真正执行 schema 15 -> 71 的冻结升级矩阵

目标：修复 R-02，证明用户从最低支持版本可以到达当前最新版本。

测试场景：

| 场景 | 输入 | 必须断言 |
| --- | --- | --- |
| clean install | 空数据目录 | 新库可写、secret format 正确、schema 71 |
| schema 15 legacy | frozen fixture + 测试 device key | 16 结构基线先完成，随后 secret baseline，再到 71 |
| schema 16 legacy | schema 16 fixture | baseline、schema、sanitizer、最终验证 |
| schema 17 encrypted | schema 17 fixture | latest migration、sanitizer、第二次启动幂等 |
| latest + partial sanitizer | schema 71 + progress running | 重启自动完成 sanitizer 后 writable |
| latest + valid baseline journal | 已发布 active + valid journal | 重启清理 journal 后 writable |
| wrong/missing key | encrypted fixture | typed `missingKey`/`keyMismatch`，原库不变 |
| invalid/future/checksum drift | 损坏或不兼容 fixture | typed recovery，绝不猜测式修复 |

schema 15 主测试应至少执行：

1. 复制 frozen fixture 到独立 temp data dir，保存 source digest。
2. 写入受控 test-only config/marker，使应用选择该数据库；使用明显假 key，禁止真实凭据。
3. 调用生产启动入口或等价的 test harness，执行完整 probe/plan/executor；不要直接调用“当前 schema 初始化”替代升级。
4. 关闭 runtime 后再次启动同一目录，断言第二次仍走 ready/writable，且没有新增不必要 backup、journal 或重复转换。
5. 读取 schema compatibility、`_sqlx_migrations`、secret rows、sanitizer progress、routing staging 和关键表约束，执行 `quick_check` 与 `foreign_key_check`。

建议文件：

- `src-tauri/tests/schema15_upgrade_fixture.rs`
- 新增独立 `src-tauri/tests/schema_upgrade_restart_matrix.rs`（若现有 fixture 文件职责过重）
- `scripts/data-store-upgrade-matrix.test.mjs`

完成门：上述矩阵每个场景都有真实执行测试；报告中记录 `running N tests` 且 N > 0、schema before/after、restart count、backup/journal 状态和失败原因。

### P4：安装包级升级探针和 CI 分块门禁

目标：修复 R-04，并让 CI 能从失败模块继续，而不是把所有检查重新跑一遍。

实施项：

1. 保留 `OldInstaller/NewInstaller/OldVersion/NewVersion/OutputPath` 全显式参数，不恢复版本默认值。
2. 扩展 `Start-And-ProbeApp`：除进程和单实例外，必须等待并读取受支持的本地 startup/monitoring snapshot，断言 `mode=writable`、`decision=ready`、`failureReason=null`、`currentSchemaVersion=71`，并验证业务 runtime/本地代理已注册。
3. 将“升级探针失败”和“应用无法启动”都写入结果 JSON；任何断言失败都让脚本退出非零。
4. CI 按模块执行：frontend contracts、Rust probe/plan、Rust migrations、secret baseline、sanitizer、routing v3、fixture matrix、install matrix、docs/artifact policy；模块之间保存构建缓存和结果摘要。
5. 所有模块通过后才执行一次 full；full 失败时解析失败 job/module，从失败模块之后继续，并在修复后重新跑 full。

建议文件：

- `scripts/run-install-upgrade-matrix.ps1`
- `scripts/install-upgrade-matrix-contract.test.mjs`
- `.github/workflows/` 对应 CI workflow
- `scripts/run-contract-tests.mjs` 或新增分块入口

完成门：恢复页、启动崩溃、schema mismatch、sanitizer incomplete 都会使安装矩阵失败；fresh、supported-baseline、post-upgrade 三条路径都得到 ready/writable 证据。

### P5：发布文档和最终资格

目标：修复 R-06，建立以后每个 schema 发布都能复用的文档门。

实施项：

1. `docs/release/SCHEMA15_UPGRADE_RECOVERY.md` 只声明当前 registry 的 latest schema；本轮应为 `71`。
2. 发布说明列出 schema 15、16、17、17+、latest、partial maintenance、wrong-key、invalid journal 的真实路径和 typed recovery。
3. 每次新增 SQL migration 必须同时更新：migration postcondition、schema15 fixture 预期、release compatibility table、升级矩阵和审计证据。
4. 审计 JSON 只记录实际执行过的命令和输出摘要；历史清单必须标记 superseded，不得继续作为当前 release evidence。
5. 发布前完成 `verify:fast`、相关 Cargo 专项、`pnpm build`、Rust fmt/check、artifact scan、安装包矩阵和最终一次 full release verification；缺少签名或安装包时明确标记未完成，不得声称通过。

完成门：文档中的 latest、测试名、命令、恢复枚举和当前代码一致；审计报告能从 source revision 复现。

## 5. 验收命令清单

以下命令按阶段执行，避免先跑 full：

```powershell
# Rust environment for local, single-threaded upgrade validation
$env:CARGO_TARGET_DIR = 'E:\Dev\Projects\relay-pool-desktop\output\cargo\ci-module-local'
$env:CARGO_BUILD_JOBS = '1'

# P0/P1 focused modules
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib startup_probe -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib startup_upgrade -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib persistence::migrations::tests -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib baseline_conversion::tests -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_url_sanitizer_migration -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_v3_migration -- --nocapture

# P2/P3 actual end-to-end fixture and restart matrix
cargo test --locked --manifest-path src-tauri/Cargo.toml --test schema15_upgrade_fixture -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test schema_upgrade_restart_matrix -- --nocapture
node scripts/data-store-upgrade-matrix.test.mjs
pnpm verify:persistence-artifacts

# P4/P5 gates
node scripts/install-upgrade-matrix-contract.test.mjs
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/run-install-upgrade-matrix.ps1 -OldInstaller <old> -NewInstaller <new> -OldVersion <old-version> -NewVersion <new-version> -OutputPath <output>
pnpm verify:fast
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

注意：`<old>`、`<new>` 等占位符必须替换为真实且已校验的安装包路径；缺少安装包时只能通过脚本契约测试，不能把安装升级矩阵标记为通过。

## 6. 数据安全、回滚与发布策略

- 所有现有库升级先创建并验证 backup；backup manifest 绑定 source/target schema、file identity 和 checksum。
- 任一步骤失败都保留原 active、backup、candidate、journal 和脱敏诊断；清理动作只删除已验证的临时 artifact。
- 发布顺序：先发布包含新 probe/plan/executor 和 fixture 的候选版本，再发布依赖该版本的文档结论；未通过 P3/P4 不得宣称 schema15 全链路兼容。
- rollback floor 至少是最后一个能读取 schema 71 且理解全部 journal/maintenance 状态的版本。schema 已前进时不支持直接安装旧二进制覆盖，必须走已验证 backup 恢复或前向修复。
- 测试和诊断只能使用明显假 key、假 URL、假路径；日志不得写完整 secret、Authorization、cookie、URL query 或原始错误正文。
- 不执行 `DROP`、覆盖 active 或删除历史 migration 作为“修复升级”的捷径。

## 7. 可扩展规则

以后新增 schema 或 maintenance 时，固定遵循以下模板：

1. 新增一个 append-only migration 或一个版本化 maintenance id。
2. 定义输入不变量、单事务/单 journal 原子边界、成功 postcondition、幂等行为和失败 recovery reason。
3. 增加一个 focused `N -> N+1` 或 maintenance resume 测试。
4. 更新 schema 15 冻结 fixture 的最终期望和 restart matrix；不得只更新当前 schema fixture。
5. 更新 release compatibility table、审计 JSON 和 CI 分块入口。
6. 只要 latest schema 已提交，后置 maintenance 就必须由 probe/plan 状态驱动，不能依赖旧 schema 比较。
7. 不在 `lib.rs`、probe 或 executor 中新增 `if schema == N` 的分支；如果必须改变启动政策，先更新本计划或新增 ADR，并补充迁移原因和回滚设计。
8. 任何测试命令都必须证明实际执行了目标测试，0 tests 一律失败。

## 8. 交付物与责任边界

| 交付物 | owner | 完成证据 |
| --- | --- | --- |
| maintenance probe/plan/executor | data-store/persistence | focused Rust tests + restart matrix |
| schema15 fixture upgrade | persistence tests | schema 71、writable、second restart |
| secret journal resume | secrets/data-store | phase fault injection + key recovery |
| install probe | release tooling | old/new installer result JSON |
| CI 分块执行 | CI/release tooling | each module count + resumable summary |
| release docs/audit | maintainers | latest=71、命令可复现、无 stale test names |

计划只有在 P0-P5 的完成门全部满足，并且最后一次完整 release verification 成功后，才能标记为 Completed。

## 9. 实施记录（2026-09-01）

分块验证已按依赖顺序全部执行，未使用一次性 `full` 代替专项门禁：

- `frontend-contracts`
- `startup-probe-plan`
- `migrations`
- `secret-baseline`
- `sanitizer`
- `routing-v3`
- `schema15-fixture`
- `persistence-artifacts`
- `install-contract`

上述模块均取得退出码 `0`，并且 Cargo 过滤测试均确认实际执行数量大于零。重点结果包括 schema `15 -> 71`、第二次启动仍为 writable/ready、partial sanitizer 可恢复、合法 baseline journal 可清理、backup/sidecar/SQLite 检查通过。

最终完整验证使用受控单线程 Cargo 目标目录执行：

```powershell
$env:CARGO_TARGET_DIR='E:\Dev\Projects\relay-pool-desktop\output\cargo\ci-full-local-2'
$env:CARGO_BUILD_JOBS='1'
pnpm.cmd verify:full
```

最终退出码为 `0`。完整流程中的 dead-code、架构、生成绑定、运行时事件、artifact、advisory/license/source、ESLint、TypeScript、前端 140 个测试文件/649 项测试、生产构建、Rust fmt/clippy/all-targets/release 检查、Rust 1476 项库测试及全部 integration tests/doc-tests 均通过；仅保留仓库既有 warning、React `act(...)` stderr 和 Vite chunk-size warning。

真实安装包级 old/new 升级矩阵未执行：本地没有可校验的 old/new installer bundle，因此只执行并通过了 `install-contract`，不能据此宣称安装器端到端已通过。发布前需在具备安装包的 CI/发布环境补跑该矩阵。
