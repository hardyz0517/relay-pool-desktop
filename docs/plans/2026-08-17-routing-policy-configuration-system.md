# 路由策略配置系统升级实施计划

状态：Planned；本文是按目标规格执行的任务记录，不是当前实现事实。

日期：2026-08-17

目标规格：[`../specs/ROUTING_POLICY_CONFIGURATION_SYSTEM_SPEC.md`](../specs/ROUTING_POLICY_CONFIGURATION_SYSTEM_SPEC.md)

适用范围：`routing_policy` aggregate、策略 compiler、路由设置 UI、Tauri IPC、SQLite migration、活动数据目录内的 `routing-policy.json`、proxy Planning Snapshot、portable migration 与相关 legacy routing/settings 路径。

历史关系：本计划不重做智能路由的资格、分层、评分、容量或请求执行。它将现有版本化策略聚合升级为唯一的配置控制面，并删除仍把通用 Settings、旧 selector 或 proxy 路径重新变成策略来源的耦合。

> 每个行为 Task 必须先建立目标 RED 证据，再实现 GREEN，最后运行 task gate。未真实退出 0 的命令表示 Task 未完成。除非用户明确要求，执行过程不 stage、commit、push 或创建分支。

## 审计修订（2026-08-17）

本计划已按当前实现和 schema upgrade contract 复核，并固定以下不可在实现时临时改变的决定：

- `RoutingPolicyStore` 现有 `save_compare_and_swap` 自行开启 transaction，不能直接满足 aggregate、history 与 document-sync 同事务。Task 2 必须先将它拆为 service 持有的 transaction API；不得用第二个 transaction 补写 sync row。
- SQL migration 不能可靠生成 Rust canonical document digest。migration 只创建 `pending_bootstrap` sync row；首个启动的 service 从已提交 aggregate 计算 digest 并条件回填，之后才允许 materialize。
- 公共 document DTO 必须是无 serde default 的完整格式；当前 `RoutingPolicyConfigV1` 的 storage compatibility default 不能泄漏到文件合同。
- `source` 是可信的后端审计上下文，不能由 Tauri 调用方传入。UI、watcher、restore 和未来 CLI 在各自 command / adapter 内附加它。
- sync persistence 分为 projection/materialization 与 external-observation 两条状态轴；一个 overloaded `state` 无法同时正确表达 pending write、无效用户文件和 I/O retry。
- revision notice 仅用于低延迟刷新，不能作为 cache 正确性前提。每个新的 Planning Snapshot 必须以同一 SQLite read snapshot 中的 aggregate revision 作 fence。
- 不承诺跨 SQLite 与任意外部进程文件写入的理论原子性。持久化最新目标、compare-before-replace、atomic replace、post-write recheck 和 reconcile 保证数据库权威及最终收敛；不能使用“旧 revision 绝不会短暂写入文件”这种无法证明的表述。

## 1. 完成定义

目标链路固定为：

```text
Routing UI draft / routing-policy.json / future CLI / history restore
  -> trusted command/adapter source + RoutingPolicyService + PolicyDocumentCoordinator
  -> strict document decode -> validate -> compile -> CAS
  -> routing_policy + history + coalescing document-sync state (one transaction)
  -> DomainRevisionNotice
  -> Planning Snapshot reads compiled active revision
  -> atomic JSON materializer converges only the latest revision
```

完成时必须同时满足：

- SQLite `routing_policy` 是唯一 active policy；JSON 文件只有通过 service apply 后生效。
- 所有 mutation 都提交完整文档，且只使用 `document.baseRevision` 作为 apply 并发前置条件。
- coordinator 串行化同一进程中的文件与 UI 操作；SQLite CAS 是跨进程和 crash 的最终围栏。
- document sync 是单行、可合并的最新目标状态，不是 FIFO outbox；旧 revision 不能被标记为最新 projection，任何失去目标资格的写入必须立即重新 reconcile。
- proxy 只消费 application 组装的 Planning Snapshot / compiled policy，不读 JSON、generic settings 或 legacy strategy literal。
- UI 的 server state、草稿、校验、冲突与同步状态有唯一 owner；dirty draft 不被外部修改静默覆盖。
- legacy `RoutingPolicy` enum、`routing_policy_name`、generic Settings policy projection、旧 ordering profile 和重复策略读取均从生产路径删除。
- schema、portable recovery、生成绑定、Windows 文件 fault、data-directory relocation 与 secret redaction 有可重跑证据。

## 2. 执行纪律

1. 每个 Task 开始前记录 `git status --short --branch`、HEAD、dirty paths、迁移最大编号和相关 fixture 状态；已有用户改动一律保留。
2. 所有 UI、文件、CLI、恢复入口调用同一个 `RoutingPolicyService` application command。禁止各自 validate/save 或直接 SQL。
3. JSON 使用严格 parser：拒绝未知字段、重复键、JSONC 注释、非有限数字、未知 enum/version 和超出上限的输入；只接受 UTF-8（可去除一个 UTF-8 BOM），不得先落到 `serde_json::Value` 再丢失重复键信息。
4. watcher 只是低延迟唤醒。运行期间至少每 30 秒对受限大小文件复核 digest；不能只信任 mtime、size 或原生事件。
5. 复用 data-store 的同目录 temp、flush、Windows replace 与父目录 sync；禁止新建 `fs::write` 配置保存路径。
6. 不保留 feature flag、shadow write、deprecated facade、双表、旧 proxy fallback 或 `allow(dead_code)` 作为长期兼容。
7. API Key、Cookie、Authorization、完整 endpoint URL、prompt、response、原始无效 JSON 和完整文件路径不得进入日志、错误、sync table、fixture 或 support bundle。仅 `RoutingPolicyDocumentStatus` 这个受控 UI IPC 可以返回已经过后端路径验证的活动文档绝对路径；前端不得自行拼接路径，其他 IPC 只返回 redacted display path 或无路径。
8. 新依赖必须先确认许可证、维护状态、Cargo lock 与 Windows 行为；migration 使用执行时下一可用 `00NN`，不得修改已发布 migration。

## 3. 依赖与切换规则

```text
0 Baseline / inventory / architecture RED
  -> 1 Strict document domain contract
  -> 2 Policy service + conflict + revision notice
  -> 3 Coalescing sync schema
  -> 4 File coordinator / materializer / reconciliation
  -> 5 IPC and generated bindings
  -> 6 Read-path consolidation and legacy backend deletion
  -> 7 UI draft / conflict / sync diagnostics
  -> 8 Atomic cutover, migration and recovery
  -> 9 Architecture absence gates and full qualification
  -> 10 Evidence closeout
```

Tasks 6-9 是不可拆分的切换单元：中间开发提交可以存在于工作树，但不得作为可发布版本。Task 9 的 source-absence 删除与资格验证完成前，Task 8 不能称为可交付切换。新 service 若与旧 Settings / proxy / selector 同时承担 production policy，就会重新形成第二事实来源。

## 4. 目标模块边界

| 路径 | 最终职责 |
| --- | --- |
| `models/routing_policy.rs` | V1 config、版本常量、domain validation；不含文件、SQL、proxy 状态 |
| `services/routing_policy_document.rs` | strict codec、canonical serializer、受限读取和 digest；不含 SQL / 评分 |
| `application/routing_policy_service.rs` | 唯一 load / validate / compile / apply / restore owner |
| `application/policy_document_coordinator.rs` | 串行 watcher、import、materialize、reconcile；不持有独立 active state |
| `stores/routing_policy_store.rs` | aggregate/history SQL；不解析文件、不发布事件 |
| `stores/routing_policy_document_sync_store.rs` | 单行 desired/materialized revision、digest、projection 与 external-observation 状态 SQL |
| `ipc/dto/routing_policy_configuration.rs` | document/status/conflict DTO；不含文件系统逻辑 |
| `commands/routing_policy_configuration.rs` | 只 parse DTO、关联 scope 和调用 facade/service |
| `features/routing/` | query、draft reducer、conflict UI、同步诊断；不计算策略语义 |
| `services/data_store/atomic_file.rs` | 唯一 Windows 原子文档写入原语 |
| proxy / routing engine | 只消费 Planning Snapshot / compiled policy |

Task 0 可按实际模块微调文件名，但不得改变 owner 或依赖方向。

## 5. Task 0：冻结基线、调用图和删除台账

**Files**

- Create: `docs/audits/2026-08-17-routing-policy-configuration-baseline.md`
- Create: `docs/audits/routing-policy-configuration-deletion-ledger.md`
- Create: `docs/audits/routing-policy-configuration-acceptance-matrix.md`
- Create: `scripts/routing-policy-configuration-architecture.test.mjs` 及 RED/pass fixtures

**Steps**

- [ ] 记录 schema 最大编号、aggregate/history shape、domain revision、现有 command/DTO/UI query key。
- [ ] 画出 `load -> compile -> PlanningSnapshot -> proxy execution` 调用图，列出每个直接 `SELECT routing_policy`、`RoutingPolicy` enum、`routing_policy_name`、倍率/分组/耗尽回退 consumer。
- [ ] 在 deletion ledger 中逐项登记 `SettingsStore::canonical_policy_projection`、`RoutingStore::load_execution_settings`、proxy startup/execution、routing preview、candidate projection、test support 的最终动作：`delete`、`replace`、`migration-only` 或 `test-only`。
- [ ] 将规格第 13 节验收标准映射到 Task、owner、测试与最终 gate；未映射项阻止 Task 1。
- [ ] 架构 RED fixtures 必须拒绝：UI direct save、generic settings policy read、proxy file access、FIFO sync table、legacy selector production import、apply 同时接受 `baseRevision` 和第二 revision token。
- [ ] 清点 data-directory relocation、portable import/export、backup restore 对活动数据目录文件的现有 catalog/allowlist 影响。
- [ ] 固定 public command 与 internal context 的边界：Tauri caller 不能选择 `file_watch`、`history_restore` 等 source；document path 只由 status / reveal command 的后端 owner 返回或打开。
- [ ] 记录当前 `RoutingPolicyStore::save_compare_and_swap` 的 transaction 边界和 migration `0024`/`0025` 对 singleton aggregate 的 seed 保证；缺 aggregate 是 persistence recovery，不是新的 configuration-required 产品状态。

**Run**

```powershell
git status --short --branch
git log -5 --oneline
Get-ChildItem src-tauri/src/persistence/migrations -File | Sort-Object Name | Select-Object -Last 10 -ExpandProperty Name
rg -n "SELECT config_json FROM routing_policy|routing_policy_name|RoutingPolicy::|routing_policy_label|load_execution_settings|canonical_policy_projection|update_routing_policy" src-tauri/src src scripts
node scripts/routing-policy-configuration-architecture.test.mjs
cargo check --locked --manifest-path src-tauri/Cargo.toml --lib
pnpm.cmd test:contracts
```

**Exit gate:** 每个旧路径均有精确 symbol、最终 owner、删除 Task 和验证；RED fixture 已证明 gate 能捕捉禁止结构。

## 6. Task 1：严格文档合同与 canonical codec

**Files**

- Modify: `src-tauri/src/models/routing_policy.rs`
- Create: `src-tauri/src/services/routing_policy_document.rs` 及 unit tests
- Create: `src-tauri/tests/routing_policy_document_codec.rs`
- Create: `src-tauri/tests/fixtures/routing-policy-document/*.json`

**Steps**

- [ ] 定义 document、config、algorithm、system 的独立版本常量，禁止新增自由字符串拼接。
- [ ] 定义 `RoutingPolicyDocumentV1 { formatVersion, baseRevision, policy }` 及无 `#[serde(default)]` 的 `DocumentPolicyV1`；只有 decoder 完成完整字段检查后才能转换为 storage/domain `RoutingPolicyConfigV1`。`policy` 使用稳定 camelCase，不泄露 SQLite serde shape。
- [ ] 为所有 object（含 `RoutingGroupFilter` union）实现 token-aware duplicate-key rejection 和 unknown-field check，不允许中间 `Value` 吞掉重复 key；storage JSON 的 legacy 宽松读取不得复用为 public document decode。
- [ ] 固定 resource contract：输入最多 64 KiB、最大嵌套 16 层、每个 object 最多 32 key、每个 string 最多 512 UTF-8 bytes；只接受 UTF-8，可剥离一个 UTF-8 BOM，拒绝其他编码、NaN/Infinity、JSONC 与未知 discriminant。为 `group_binding_id`/`group_id_hash` 增加与该字符串上限一致的 domain validation。
- [ ] 用固定字段顺序的 typed struct 生成 canonical pretty JSON、SHA-256 raw digest 与 SHA-256 semantic digest；将 `-0.0` normalize 为 `0.0`，使合法数值的语义相等和输出字节相等。只改空白/顺序/stale revision 不产生 policy revision。
- [ ] 将 document validation 统一接入 `RoutingPolicyConfigV1::validate` 与 `compile_config`。
- [ ] 增加 default、全字段、group-filter 各分支、边界、重复键、未知字段、future version、同语义不同格式的 golden fixture。

**RED / GREEN**

- RED：当前普通 serde 路径无法拒绝 union object 重复键或无法区分 raw/semantic digest。
- GREEN：非法 fixture 全部拒绝，UTF-8 BOM 合法 fixture 与无 BOM 输入有相同语义，合法 fixture 的 canonical bytes 稳定，document 到 compiled policy 只有一种结果。

**Run**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml routing_policy_document_codec -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml application::routing_policy -- --nocapture
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 严格文档解析和 policy compiler 均有唯一 owner，保存前无法通过编译的配置不能进入 active state。

## 7. Task 2：唯一 policy service、typed conflict 与 revision notice

**Files**

- Create/modify: `src-tauri/src/application/routing_policy_service.rs`
- Modify: `src-tauri/src/application/routing.rs`、`application/command_facades/routing.rs`
- Modify: `src-tauri/src/persistence/stores/routing_policy_store.rs`
- Modify: `application/queries/read_model_revision.rs`、application/command error mapping
- Create: `src-tauri/tests/routing_policy_service.rs`

**Steps**

- [ ] 将当前分散的 `save_routing_policy` 收敛到 `RoutingPolicyService`：decode、validate、compile admission、CAS、history、sync target 与 notice 都在此 owner。strict decode、domain validation、纯 `compile_config` 必须在短 SQLite write transaction 之前完成，不能因 CPU 工作长期持有数据库写锁。
- [ ] 将现有 `RoutingPolicyStore::save_compare_and_swap` 拆为只接受 caller-owned transaction 的 `load_in_tx` / `save_cas_in_tx` / history APIs；service 在一个 transaction 中重读 aggregate、比较、CAS、domain revision advance、history append 与 document-sync target upsert。旧自行 transaction 的 public helper 必须删除，任何失败都不写 policy 或 sync target。
- [ ] no-op 在同一个短 transaction 中基于 typed canonical config 先于 stale-base CAS 判断：若 active policy 语义相同，返回 current snapshot 而不 bump revision/history/desired target；文件 coordinator 只可把现有 target 重新标为 pending canonical materialization，不能伪造新 mutation。
- [ ] 新增 `RoutingPolicyConflict { baseRevision, currentRevision, source, activeDocumentSummary, draftDocumentSummary }`，公共映射不能退化为通用 internal/stale 错误。
- [ ] internal `apply(document, trusted_context)` 的唯一并发条件是 `document.baseRevision`。`trusted_context.source` 只能在 UI command、file watcher、manual import adapter、history restore 或未来 CLI adapter 内创建；public `apply_routing_policy_document` 不接收 source。history restore 可带 `expectedRevision`，但必须先取回可编译的完整历史 config、构造 internal document 后走同一 preflight/CAS；历史不兼容时返回 typed restore error。
- [ ] commit 后发布 `DomainRevisionNotice`；notice 是可丢失的低延迟 hint，失败不回滚 committed policy，并有受限诊断。UI 和 compiled-policy cache 都必须存在 independent revision reconciliation/fence。
- [ ] 用 failpoint 覆盖 compile error、CAS conflict、history failure、notice failure 和 no-op。

**Run**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_policy_service -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml routing_policy_store -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml read_model_revision -- --nocapture
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 所有 source 得到同一 validation、revision、history 与 conflict 语义；不存在第二个 policy save implementation。

## 8. Task 3：可合并 document-sync schema 与恢复合同

**Files**

- Create: `src-tauri/src/persistence/migrations/00NN_routing_policy_document_sync.sql`
- Create: `src-tauri/src/persistence/stores/routing_policy_document_sync_store.rs`
- Modify: store registry、schema postcondition、persistence fixture、portable migration schema catalog/fingerprint
- Create: `src-tauri/tests/routing_policy_document_sync_store.rs`

**Steps**

- [ ] 建立单行 `routing_policy_document_sync`，保存 desired/materialized revision、canonical/raw SHA-256 digest、projection state、external-observation state、有限 error code、retry time、更新时间和有租约期限的 attempt token。projection 与 observation 必须是独立列，不能再使用一个 overloaded state。
- [ ] migration 只从已 seed 的 active aggregate 填入 `desiredRevision` 与 `pending_bootstrap`；`desiredCanonicalDigest` 在 SQL 中显式为 NULL。首次 service startup 在 transaction 内 typed-load、validate、compile aggregate 后条件回填 digest 并转为 `pending_write`，不得在 migration 中伪造 Rust canonical bytes、history 或 policy revision。aggregate 缺失或无法编译是 typed persistence recovery，阻止 policy service/proxy admission，不显示为可编辑的 configuration-required。
- [ ] apply transaction 使用 `desiredRevision = new revision` 和 canonical digest upsert，不插入 FIFO job。新 revision 覆盖旧 target，并清除早于 target 的 writer lease/attempt result。
- [ ] 实现 `claim_latest_target`、`abandon_stale_attempt` 与 `mark_materialized(targetRevision, attemptToken)` 条件写：只有 target、canonical digest 和 lease token 仍匹配 desired 才能标记完成；失配只能触发最新 target reconcile，不能把旧目标标为成功。
- [ ] 定义有限、分类的 read/parse/conflict/write error 与有上限 retry/backoff；`invalid_document`/`conflict` observation 阻止自动覆盖，只有文件改变或用户显式重写才解除。禁止保存原始无效文件内容、任意错误文本或无限 retry。
- [ ] 更新 portable migration schema catalog 与 schema postcondition，确认恢复后 aggregate 优先且 JSON 可重建；`routing-policy.json` 和 `.bak` 是派生文件，绝不写入 portable package、backup manifest 或导出 payload。

**Run**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_policy_document_sync_store -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test schema15_upgrade_fixture -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml startup_upgrade -- --nocapture
pnpm.cmd verify:persistence-artifacts
```

**Exit gate:** sync state 只保留最高 desired revision；bootstrap 不依赖 SQL canonicalization；retry/fault 不会把历史 revision 标记为最新 materialization。

## 9. Task 4：文件 coordinator、原子 materializer 与 reconcile

**Files**

- Create/modify: `src-tauri/src/application/policy_document_coordinator.rs`
- Modify: `src-tauri/src/services/routing_policy_document.rs`
- Modify: data-store startup/resume/relocation composition 与 `atomic_file.rs`（仅缺少安全原语时）
- Create: `src-tauri/tests/routing_policy_document_coordinator.rs`

**Steps**

- [ ] document path 固定为已验证活动 data dir 下的专用 config directory / `routing-policy.json`，只能由后端解析；创建和每次使用均验证 parent、regular file、reparse point、大小、权限边界。仅 status 的专用 IPC 可以回传该绝对路径，另设 backend-owned reveal/open command，前端不能拼接或对任意路径发起操作。
- [ ] 复用同目录 temp/write/`sync_all`/Windows replace/parent sync；不得新增简单写文件路径。
- [ ] 用唯一 `PolicyDocumentCoordinator` 的有界 coalescing wakeup 和单一 mutation guard 串行 watcher event、manual import、UI 触发的 materialize、startup/resume reconcile、retry 和 relocation pause/resume。不得叠加 service mutex；任何 domain notice、IPC event 或 timer callback 都在 guard / SQLite transaction 之外执行，避免重入和死锁。
- [ ] 引入原生 watcher 前完成许可和 Windows rename 审阅。watcher 只发有界 debounce wakeup；以 750 ms debounce 后、相隔至少 150 ms 的两次相同 file identity + SHA-256 读取定义 stable read。两次间变化重新 debounce 而非标为无效；稳定且无效才进入 invalid。无论 event 是否到达，每 30 秒重算受限文件 digest。
- [ ] materialize 前按 raw/semantic digest 处理：同语义内容不 bump revision 且 coordinator 只把当前 desired target 重新标为 pending canonical materialization；合法 current-base 差异走 internal file apply；stale-base 进入 conflict；invalid/unreadable 不覆盖用户文件。
- [ ] materialize 从 current target 的 history/aggregate 重建文档。每次准备、replace 前和 replace 后均验证 attempt token、target revision/digest 与 observation digest；replace 后若发现目标已失效，不能标记旧 revision 成功，必须立即合并并重新 reconcile 最新目标。
- [ ] 不依赖任意外部编辑器会遵守的文件锁。使用同目录原子 replace、compare-before-replace、post-write digest recheck 与自写 identity/digest suppression；对无合作 writer 只承诺数据库权威和最终收敛，不承诺跨资源瞬时原子性。
- [ ] 明确覆盖 active policy 前再次比较 raw digest，并用 `ApprovedLeaf` 创建唯一、不可覆盖的 `routing-policy.<timestamp>-<random>.bak`。备份只保存于受管 config directory，默认保留 7 天且最多 5 个；清理只能在成功新建替代物后 best-effort 进行，不能删除目标或未受管文件，不匹配即 conflict。
- [ ] relocation、portable restore、resume、watcher overflow 时停止旧 watcher，reconcile/materialize 新 active dir 后才恢复。

**Run**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_policy_document_coordinator -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml routing_policy_document -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml data_store::atomic_file -- --nocapture
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** 文件是受管输入/投影；仅最新完整文档可收敛，proxy 不接触文件系统。

## 10. Task 5：IPC、生成绑定与 document status

**Files**

- Create: `src-tauri/src/ipc/dto/routing_policy_configuration.rs`
- Create: `src-tauri/src/commands/routing_policy_configuration.rs`
- Modify: IPC registry、ACL manifest source、generated bindings、`BackendClient`/DesktopBackend/DemoBackend、`src/lib/api/routing.ts`
- Create/update: command/DTO/generated contract tests

**Steps**

- [ ] 新增 `get_routing_policy_document_status`、`validate_routing_policy_document`、不含 caller-provided source 的 `apply_routing_policy_document`、`restore_routing_policy_revision`、canonical document read，以及 backend-owned reveal/open / import-current-file 的受限契约。只有 valid canonical document 可从 IPC 读取；无效原文只保留在本地文件。
- [ ] command 仅做 DTO parse、correlation scope 和 facade/service 调用；不直接读文件、写 SQL 或返回原始无效内容。
- [ ] 使用 generated TypeScript 作为唯一前端 DTO，删除临时手写 normalizer/default type。
- [ ] conflict/invalid/unavailable 映射为稳定 public 类型；可供 diff 的摘要不含 secret、原始路径或完整用户文件。
- [ ] document status 独立 query，明确返回 projection state 与 observation state 的派生镜像文案、active/materialized revision、受限状态路径；它不能被 Routing Workspace 复制为第二策略真相。
- [ ] 生成 binding 后删除已迁移的 `update_routing_policy` public command/binding。

**Run**

```powershell
pnpm.cmd generate:bindings
pnpm.cmd test:contracts
pnpm.cmd architecture:commands
pnpm.cmd architecture:security
cargo test --locked --manifest-path src-tauri/Cargo.toml routing_policy_configuration -- --nocapture
pnpm.cmd exec tsc --noEmit
```

**Exit gate:** IPC 只公开 document control plane，调用方不能伪造审计 source；契约/生成物没有 second direct-update API 或 domain/file leaks。

## 11. Task 6：收敛读取链，删除 Settings 和 legacy backend 耦合

**Files**

- Modify: routing application / Planning Snapshot builder
- Modify/delete: `RoutingStore::load_execution_settings`、`SettingsStore::canonical_policy_projection`
- Modify/delete: settings model/DTO/facade/UI 的 policy fields
- Modify/delete: proxy startup/execution、routing preview、candidate projection 的 legacy `RoutingPolicy` usage
- Modify: legacy import mapper and tests only where historical mapping is required

**Steps**

- [ ] application boundary 提供唯一 `load_compiled_policy_for_snapshot`：它在与 operational facts 相同的 SQLite read snapshot 中读取 active aggregate revision，并只在 immutable compiled cache revision 精确匹配时复用 cache。cache miss/revision 不符必须重新 compile；proxy 不能通过 Store、Settings 或 file path 再读取策略。
- [ ] 从 `AppSettings`、settings input/output/commands/UI 移除 `routing_policy_name`、倍率、分组范围、耗尽回退的策略所有权字段；展示改读 policy snapshot。
- [ ] 删除 `RoutingStore::load_execution_settings` 直接 SQL。任何 legacy execution 参数改由 application 传入明确 compiled policy。
- [ ] 删除 production 的 `RoutingPolicy` enum、`routing_policy_label`、old ordering profile 在 proxy、preview、candidate projection、DTO 和前端 option 的 consumer；历史 import 显式映射后只输出 V1 complete config。
- [ ] request log / decision trace 记录 config/algorithm revision，而不再声称旧 literal 是 active policy。
- [ ] revision notice 只能使本进程 cache/UI 低延迟失效，不能替代上述 read-snapshot revision fence；测试必须覆盖 notice 丢失、另一实例写入和 policy read 与 candidate facts 不混用 revision。
- [ ] 加 source-absence gate，唯一 allowlist 为 migration/import/historical fixture。

**Run**

```powershell
node scripts/routing-policy-configuration-architecture.test.mjs
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_read_models -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_failure_contract -- --nocapture
pnpm.cmd test:contracts
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Exit gate:** generic Settings、proxy 和 legacy enum 均不再构成 production policy 事实来源。

## 12. Task 7：前端 query、draft、冲突与同步诊断

**Files**

- Create/modify: policy query keys/hooks、revision notice synchronization owner
- Create: `src/features/routing/useRoutingPolicyDraft.ts`
- Create: `src/features/routing/RoutingPolicyConflictDialog.tsx`
- Modify/delete: `src/features/routing/LocalRoutingSettingsEditor.tsx`
- Create/update: focused Vitest / React tests

**Steps**

- [ ] current policy、document status、validation 使用 React Query server state；draft reducer 保存 base snapshot/revision、dirty map 与 validation。
- [ ] 删除局部 `load/save` state machine 和 `JSON.stringify` dirty comparison；所有字段来自 generated DTO / canonical document。
- [ ] 覆盖 loading、sync bootstrap/pending、invalid document、conflict、unavailable、typed persistence recovery、validation error、disabled save 和窄窗口。有效 schema 的 aggregate 缺失不是 configuration-required UI 状态。
- [ ] revision notice 到来时 clean draft 自动 refresh；dirty draft 保留并显示安全 diff、reload、逐字段 merge、明确覆盖。notice 漏失时，window focus/resume、document status polling 和下一次 mutation 前的 query revalidation 必须收敛 server state。
- [ ] 保存只调 document apply。覆盖先以 current active document 重建 draft，再写入最新 `baseRevision`；不得重试旧 revision。
- [ ] 显示 active/materialized revision、projection/observation 的不同状态和“重新读取 / 导入当前文件 / 打开所在位置 / active policy 重写并备份”。路径只由 backend status 返回，其他 action 不接受前端路径。
- [ ] 未保存 simulation 继续调用同一后端 compiler/planner，带 `policy_source=draft`，前端不计算评分。

**Run**

```powershell
pnpm.cmd test -- src/features/routing/LocalRoutingSettingsEditor.test.tsx
pnpm.cmd test -- src/features/routing/RoutingPolicyConflictDialog.test.tsx
pnpm.cmd test -- src/lib/queries/routingPolicyQueries.test.ts
pnpm.cmd build
pnpm.cmd test:contracts
```

**Exit gate:** UI/文件更新无静默覆盖；草稿、revision、validation 和 status 各有唯一 owner。

## 13. Task 8：原子切换、迁移恢复与旧 command 删除

**Files**

- Modify: application dependency composition、startup/resume/relocation/portable recovery wiring
- Modify: generated command registry/TypeScript through generator
- Delete: old direct update command/binding、obsolete settings policy fields、legacy UI paths and tests
- Modify: migration catalog, source manifests, docs and architecture runner

**Steps**

- [ ] 启动顺序固定：data dir resolved -> schema/postcondition ready -> active aggregate typed-load + compile -> sync bootstrap/recovery -> coordinator starts -> proxy admission uses read-snapshot-fenced compiled policy。aggregate 缺失/损坏走 typed persistence recovery，不加载 legacy default，也不进入可编辑 configuration-required。
- [ ] sync worker 不阻塞 proxy startup/request planning；worker failure 只降级 document status，不能改变 committed aggregate。
- [ ] UI、manual import、watcher、startup reconcile、restore 全部接 document service；删除旧 direct update contract。
- [ ] policy revision notice 只由一个 scope-to-query mapping 失效 policy/routing workspace query family。
- [ ] 演练 relocation、backup restore、portable import：停止并 drain 来源 watcher/coordinator 后，目标目录 JSON 仅从 restored active aggregate + sync state 重建；不复制来源 JSON、`.bak` 或 watcher observation，成功 reconcile 后才恢复监听。
- [ ] 更新 model/settings/generated fixtures、DemoBackend、ACL、command registry；删无消费者字段。

**Run**

```powershell
pnpm.cmd generate:bindings
pnpm.cmd verify:fast
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_startup_cutover -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test portable_migration_e2e -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_policy_document_coordinator -- --nocapture
pnpm.cmd build
git diff --check
```

**Exit gate:** fresh DB、upgraded DB、portable import 和 relocated dir 启动时，只有新控制面读写策略；文件同步失败不影响 active revision。

## 14. Task 9：删除清理与完整资格验证

**Files**

- Modify: architecture absence gate、fixtures、contract runner、dead-code inventory
- Delete/modify: legacy selectors/labels, generic Settings policy contract, direct SQL helpers, obsolete editor mocks/tests
- Create: `scripts/routing-policy-configuration-qualification.mjs`
- Create: `docs/audits/routing-policy-configuration-qualification.md`

**Steps**

- [ ] 逐项完成 Task 0 ledger。任何保留 legacy symbol 只允许 migration/import/historical fixture，且记录删除条件与 allowlist。
- [ ] 删除 FIFO task 假设、重复 watcher、手写文件写入、proxy file access、无消费者 status 和 generic policy projection。
- [ ] 架构 RED fixtures 必须在旧 command、direct SQL、legacy selector、UI direct save、proxy file access 重新出现时失败。
- [ ] 资格矩阵覆盖 policy no-op/conflict/restore、invalid/duplicate/oversized/BOM document、UI-vs-file、watcher burst、stable-read partial save、stale writer attempt、disk/replace fault、missed-event digest、restart between commit/materialize、migration bootstrap digest、notice 丢失 cache fence、relocation/portable recovery、in-flight request revision、secret/path canary。
- [ ] 用 1,000 次 no-op/conflict/watcher event 验证 history、sync state、retry 与 coordinator 队列有界；并用两个 cooperating service instance 演练 lease / stale-attempt 收敛。结束时 worker drain/shutdown 无泄漏。
- [ ] 记录 Windows Notepad 和一款原子保存编辑器的人工互操作结果。该观察不替代 deterministic fault tests。

**Run**

```powershell
pnpm.cmd verify:full
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml
pnpm.cmd build
pnpm.cmd test:contracts
node scripts/routing-policy-configuration-architecture.test.mjs
node scripts/routing-policy-configuration-qualification.mjs
node scripts/dead-code-inventory.mjs --mode ci --scope production
git diff --check
```

**Exit gate:** 自动化 gate 全部退出 0；audit 记录环境、schema、规模、fault、人工观察和未运行范围；无未解释 conflict、legacy owner 或 secret canary。

## 15. Task 10：关闭证据与文档状态

**Files**

- Modify: baseline acceptance matrix、deletion ledger、qualification audit
- Modify: target spec status only after evidence exists
- Modify: `docs/README.md`、`docs/PROJECT_PLAN.md`、`docs/PRODUCT_MODEL.md` only where current product facts changed

**Steps**

- [ ] 对规格第 13 节每项验收填写 test、command、revision、fixture/audit evidence；禁止用“full suite 覆盖”笼统结案。
- [ ] deletion ledger 只剩 `deleted`、`migration-only`、`test-only`；不存在 `later`、未解释 `temporary` 或 production compatibility。
- [ ] 校验 document/config/algorithm/system version、schema version、generated IPC hash 与实际二进制行为一致。
- [ ] 只有实现和验证完整后才将 spec 从 `Proposed` 改为 implemented；本计划保留为历史执行记录，不改写为长期规范。

**Run**

```powershell
node scripts/routing-policy-configuration-architecture.test.mjs
node scripts/routing-policy-configuration-qualification.mjs
pnpm.cmd verify:full
git diff --check
git status --short
```

**Exit gate:** 规格、README、产品模型与代码状态一致；所有 acceptance、删除和未验证项可独立追溯。

## 16. 每 Task 交付模板

```text
Task:
Start HEAD / End HEAD:
Dirty paths preserved:
RED command and observed failure:
GREEN command:
Affected Rust / TypeScript / IPC / migration contracts:
Files added / modified / deleted:
Legacy owners removed or remaining with reason:
Security-sensitive paths reviewed:
Validation commands and results:
External/manual checks not run:
```

## 17. 禁止偏移

- 不把 JSON 与 SQLite 作为并列 active policy，也不按文件时间戳覆盖数据库。
- 不实现逐 revision FIFO sync queue；新 revision 必须合并并淘汰旧 target。
- 不让 apply 同时接受 `expectedRevision` 与 `document.baseRevision`。
- 不让 watcher、proxy、React 或 generic Settings 直接写 `routing_policy`。
- 不把 JSONC、宽松 unknown field、重复键后值覆盖或注释保留作为未设计兼容。
- 不因文件同步失败回滚 committed policy；恢复只通过 sync state、reconcile 和显式 conflict。
- 不保留旧 selector、`routing_policy_name` 或 old ordering profile 作为“回滚保险”。
- 不把 schema migration、portable recovery、generated binding 和 source absence gate 延后到主功能看似可用之后。
