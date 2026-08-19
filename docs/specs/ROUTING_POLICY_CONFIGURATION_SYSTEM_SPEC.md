# 路由策略配置系统升级规范

状态：Proposed，待实现与验收

日期：2026-08-17

适用范围：智能路由策略、路由设置页、本地数据目录、Tauri IPC、SQLite 策略聚合、代理规划快照与数据搬家

提案类型：配置控制面重构与文本化配置入口

替代关系：本文在实施后替代路由策略的临时页面本地状态保存方式；不替代 [`INTELLIGENT_ROUTING_ENGINE_SPEC.md`](INTELLIGENT_ROUTING_ENGINE_SPEC.md) 对资格、分层、评分、调度和决策解释的领域约束。

关联入口：

- [`../README.md`](../README.md)
- [`../PROJECT_PLAN.md`](../PROJECT_PLAN.md)
- [`../PRODUCT_MODEL.md`](../PRODUCT_MODEL.md)
- [`INTELLIGENT_ROUTING_ENGINE_SPEC.md`](INTELLIGENT_ROUTING_ENGINE_SPEC.md)
- [`../SECURITY_EXPORT_IMPORT.md`](../SECURITY_EXPORT_IMPORT.md)
- [`../SCHEMA_UPGRADE_AUTHORING.md`](../SCHEMA_UPGRADE_AUTHORING.md)
- [实施计划：`../plans/2026-08-17-routing-policy-configuration-system.md`](../plans/2026-08-17-routing-policy-configuration-system.md)

## 1. 执行摘要

Relay Pool Desktop 当前已拥有版本化、可审计的 `routing_policy` SQLite 聚合和前端编辑页，但没有一个用户可编辑的策略文件。前端保存通过 `expectedRevision` 进入 compare-and-swap（CAS），不过文件入口、统一配置服务、冲突交互、文件恢复与前端外部变更处理尚不存在。

本升级将路由策略建设为本地桌面应用的第一类配置系统，提供接近 VS Code 设置的体验：用户既可在“路由规则”页面编辑，也可修改一个稳定的 JSON 文档；两条路径由同一后端控制面验证、编译、提交和通知。

本规范选择的成熟工程模型是：

1. SQLite 中的已提交策略是唯一运行时权威状态。
2. `routing-policy.json` 是受管的文本配置文档与提交入口，不是第二个可独立生效的数据库。
3. UI、文件导入、未来 CLI 和恢复操作都调用同一个 `RoutingPolicyService::apply`；它们不能直接修改数据库，也不能在前端计算策略语义。调用来源是后端附加的可信审计上下文，不能由 IPC caller 自报。
4. 每次提交都是完整文档、带 base revision 的乐观并发更新。冲突必须显式暴露，不能“最后写入者胜出”。
5. 数据库与文件无法组成跨资源原子事务。因此数据库提交后文件镜像采用持久化、可合并到最新 revision 的 materialization record 收敛；文件失败不会使已提交策略失效，也不会被伪装为同步成功。系统保证数据库权威和最终收敛，不对不合作外部进程承诺跨资源瞬时原子性。
6. 自定义仅指版本化、受校验的策略参数与预设，不包含可编程脚本、任意表达式或可绕过安全边界的 DSL。

该模型避免“UI 一份设置、JSON 一份设置、代理缓存又一份设置”的长期分叉，并让策略 revision、审计历史、决策 trace 和恢复行为保持一致。

## 2. 当前基线与问题

| 当前能力 | 现状 | 升级要求 |
| --- | --- | --- |
| 策略模型 | `RoutingPolicyConfigV1` 有完整字段和边界校验 | 保持严格版本化，增加面向用户的 JSON 文档 codec，不能暴露内部存储形状作为公开格式 |
| 持久化 | `routing_policy` 单例表、`config_revision`、`routing_policy_history` 已存在 | 保持为 active policy 与审计历史的权威存储；新增文档同步状态而非第二套策略表 |
| 前端保存 | 编辑页请求 `update_routing_policy(config, expectedRevision)` | 替换为共享 policy query、草稿状态和显式 conflict resolver |
| 冲突 | Store 会拒绝 revision 不一致，但编辑器仅显示通用错误 | 返回 typed conflict/current revision；前端 reload、diff 和明确覆盖必须可用 |
| 路由运行时 | Planning Snapshot 记录策略 revision，Compiler 有纯函数入口 | 所有新请求读取已提交 revision；进行中的请求继续使用已有快照，不允许文件监听直接改 proxy 内存 |
| 配置文件 | `relay-pool-data-dir.json` 只负责数据目录选择 | 新增独立 `routing-policy.json`；禁止在数据目录选择文件中混入路由字段 |
| 写文件能力 | 数据目录模块已有临时文件、flush、replace、父目录同步的 Windows 兼容实现 | 新文件必须复用该原子文件基础设施，不新增简化版 `fs::write` |

当前实现中还存在必须在此升级内收口的结构债务：

- 页面以局部 `useState` 保存 server state、revision 和草稿；它没有订阅外部 mutation，且用通用错误处理 CAS 冲突。
- policy version 与 system version 以不同模块中的字符串字面量写入，seed 与后续保存的命名不完全一致；版本身份缺少唯一 owner。
- `RoutingStore::load_execution_settings`、`SettingsStore::canonical_policy_projection`、proxy startup 和 routing preview 仍直接读取或投影 policy；它们使同一 aggregate 有多条消费者链和 legacy `RoutingPolicy` 适配层。
- `AppSettings` / settings DTO 中的 `routing_policy_name`、倍率、分组范围和耗尽回退兼容字段仍可能把通用 settings 重新变成策略写入入口。旧策略 literal 只能保留在一次性 migration / import 边界，不能继续成为生产策略输入。

## 3. 目标与非目标

### 3.1 目标

- 用户能从路由页安全编辑全部公开策略字段，并立即看到已提交 revision 和文档同步状态。
- 用户能在活动数据目录的受管路径编辑 JSON；有效修改在应用运行期间自动导入并对后续请求生效。
- 同一策略以 UI、文件、CLI 或恢复路径提交时拥有相同的 validation、compiler、CAS、历史和审计语义。
- 文件损坏、版本未知、权限不足、监听丢失、写入中断、外部并发编辑和应用重启均不会替换当前 active policy。
- 每一条 route decision 能说明它使用的 policy revision；每次 policy mutation 能说明来源而不记录敏感内容。
- 新增可配置字段、预设或 policy document format 时有显式升级路径，不要求重写 UI、proxy 或 SQLite 读取链。

### 3.2 非目标

- 用户可编程的 JavaScript、Lua、WASM、正则脚本或任意条件表达式。
- 允许策略权重绕过凭据、能力、健康、容量、余额、倍率上限、deadline 或 retry budget。
- 把 API Key、Cookie、token、完整 endpoint URL、请求正文或运行时健康指标写入策略文件。
- 将策略文件放入仓库、作为发布包内的默认用户文件，或用它替代加密迁移 / 数据库备份。
- 多设备实时协同、云端配置同步或团队策略管理。
- 长期同时运行 legacy strategy selector 和智能路由策略 compiler。

## 4. 核心决策与不变量

### 4.1 单一控制面

`RoutingPolicyService` 是策略的唯一 command owner。它负责：

- 读取 active aggregate 与历史 revision；
- 解析 / 升级外部文档；
- 在 write transaction 前完成完整字段校验与纯 policy compile admission；
- 在一个短 SQLite write transaction 中执行 re-read、CAS、revision bump、history 写入和 document materialization record upsert；
- 提交后发布 `DomainRevisionNotice { scope: routing_policy, revision }`；
- 驱动文件 materialization、重试与诊断状态。

`RoutingPolicyStore` 只负责 aggregate 与 history 的 SQL 读写。文件 watcher、Tauri command、React 页面和 proxy 不得各自重写相同的 validate/save 逻辑。

service 内部必须有单一 `PolicyDocumentCoordinator`，以一个 mutation guard 串行化同一进程中的读取、导入、materialize 与 reconciliation。SQLite CAS 仍是跨进程 / crash 的最终围栏；进程内 coordinator 则防止多个 watcher event 或 UI mutation 对同一文件产生交错的 read-compare-write。notice、IPC event 和 timer callback 必须在该 guard 与 SQLite transaction 外发布，不能再叠加第二个 service mutex。

### 4.2 单一运行时真相

已提交的 SQLite aggregate 是唯一 active policy。`routing-policy.json` 的内容只有在 `RoutingPolicyService::apply_document` 成功后才成为 active policy。

这意味着：

- JSON 被手工编辑为无效内容时，正在运行的代理继续使用上一个 active revision；
- 文件或 UI 发生冲突时，不会根据文件时间戳、文件系统事件顺序或数组排序猜测胜者；
- 内存 cache 只能缓存已编译、带 revision 的不可变快照，不能成为独立配置来源；revision notice 只是低延迟失效 hint，不能作为 cache 正确性前提；
- 新 Planning Snapshot 必须在与 operational facts 相同的 SQLite read snapshot 中 fence active policy revision，只有 cache revision 精确匹配时才可复用 compiled policy；已开始请求保持 trace 中的 revision 或按既有 fence 规则重规划。

### 4.3 完整文档与乐观并发

每次更新提交完整 policy，而不是字段级 patch。所有入口都必须携带其读取时的 `baseRevision`。

```text
read snapshot (revision = R)
  -> edit a complete draft/document
  -> validate + compile
  -> apply(document.baseRevision = R)
  -> active revision = R + 1, or typed conflict with no write
```

字段级自动 merge 只可用于 UI 展示冲突差异；它不能自动提交。两个入口同时修改同一字段或无法证明 base 相同，必须要求用户重新加载、逐项合并或明确以当前草稿覆盖。唯一例外是 service 在同一个短 transaction 中确认 incoming policy 与 current active policy 语义相同的 no-op：它返回 current snapshot，不检查 stale `baseRevision`，也不写 revision/history/新的 sync target；文件 coordinator 只能将已存在的 current target 标为待 canonical materialize。

### 4.4 失败关闭与确定性

- 文档 format version、policy config version、枚举、数值范围或未知字段不受支持时必须拒绝，而不是静默忽略。
- `version`、权重总和、candidate limit、探索上限、亲和 TTL、倍率上限和分组筛选继续由 domain validation 统一验证。
- `compile_config` 是唯一编译入口；导入与 UI 保存都必须在写入前运行它。不能让一个“能保存但直到代理请求才编译失败”的 revision 进入 active 状态。
- 重复提交相同完整内容必须幂等，不制造无意义 revision、history 或新的文件写入任务；合法 `-0.0` 必须 canonicalize 为 `0.0`，避免数值相等却出现不同 digest / revision 语义。

### 4.5 版本身份的唯一 owner

定义常量或受限枚举，而非跨模块字符串字面量：

```text
PolicyDocumentFormat = routing-policy-document/v1
PolicyConfigVersion  = 1
PolicyAlgorithmVersion = intelligent-routing/v1
PolicySystemVersion = routing-system/v1
```

文档格式、存储配置形状、算法语义和系统实现版本是不同概念，不能复用同一 `version` 字段。它们必须分别随兼容性变化而升级，并在 decision trace / history 中可追溯。

## 5. 公开文档契约

### 5.1 文档位置与权限

文件名固定为 `routing-policy.json`，位于活动数据目录内的专用配置子目录。前端可仅通过 `get_routing_policy_document_status` 获取已经后端验证的活动文档绝对路径，或通过 backend-owned reveal/open command 打开位置；不得自行推导、拼接或提交文件路径。

文件只包含公开路由设置，默认继承活动数据目录权限；写入前须确保父目录存在并拒绝非文件、reparse-point / symlink 异常目标和不受信任路径。文件内容、解析错误与 conflict artifact 不得写入运行日志；诊断只记录错误类别、revision、哈希前缀和受限路径标签。

数据目录迁移、便携导入导出和恢复流程必须将此文件视为可从 active aggregate 重建的派生物。迁移前若文件含未应用修改，流程必须先导入成功、显式放弃，或中止操作；不得静默丢失。

### 5.2 `RoutingPolicyDocumentV1`

JSON 使用 camelCase，保持面向用户的稳定命名；内部 SQLite `config_json`、Rust serde 字段名和 IPC DTO 不构成该文件格式。

```json
{
  "formatVersion": 1,
  "baseRevision": 42,
  "policy": {
    "version": 1,
    "reliabilityWeight": 4000,
    "responsivenessWeight": 2500,
    "costWeight": 2000,
    "preferenceWeight": 1500,
    "maxCandidates": 64,
    "explorationShareBasisPoints": 500,
    "allowDepletedFallback": false,
    "affinityEnabled": false,
    "affinityTtlSeconds": 300,
    "maxRateMultiplier": null,
    "routingGroupFilter": "all_groups"
  }
}
```

约束：

- `baseRevision` 表示该文档编辑时基于的 active revision，导入成功后文件会被 materialize 为新 revision。
- `policy` 是完整 effective config，不使用隐式 patch、未声明继承或“缺失字段沿用旧值”。这避免默认值变化导致同一文件在不同版本产生不同语义。
- 文档不包含 `status`、`updatedAt`、system version、历史记录或运行时 cache；这些元数据只由 service 生成。
- 顶层与 `policy` 均使用拒绝未知字段、重复键和 JSONC 注释的严格 decoder。public decoder 使用无默认值的 `DocumentPolicyV1`，完成完整字段检查后才转换为有 storage compatibility default 的 `RoutingPolicyConfigV1`；未来扩展必须提高对应 version，并提供明确 upgrader；不得依赖 `serde_json` 的“重复键后值覆盖前值”默认行为。
- `routingGroupFilter` 复用版本化 DTO 的 discriminated shape，不能接受显示名称或不稳定 ID 猜测。
- 输入上限固定为 64 KiB、最大嵌套 16 层、每个 object 最多 32 key、每个 string 最多 512 UTF-8 bytes。只接受 UTF-8，可剥离一个 UTF-8 BOM，拒绝其他编码；canonical 输出始终为无 BOM UTF-8。`group_binding_id` 和 `group_id_hash` 还必须通过相同上限的 domain validation。

该文件是严格 JSON 而非 JSONC。UI 和 service 会输出稳定的 canonical formatting；只改变空白或字段顺序不产生 policy revision，但后续 materialization 可以恢复 canonical formatting。产品不承诺保留注释，因此不得在未支持 JSONC 的前提下暗示 VS Code 的注释兼容性。

### 5.3 文档 API

新增专用 IPC 契约，TypeScript binding 仍由 generator 生成：

```text
get_routing_policy_document_status() -> RoutingPolicyDocumentStatus
validate_routing_policy_document(document) -> RoutingPolicyDocumentValidation
apply_routing_policy_document({ document }) -> RoutingPolicySnapshot
import_current_routing_policy_document() -> RoutingPolicySnapshot
reveal_routing_policy_document() -> void
restore_routing_policy_revision({ revision, expectedRevision }) -> RoutingPolicySnapshot
```

`source` 是 service internal context 的受限枚举：`ui`、`file_watch`、`manual_import`、`history_restore`、`startup_reconcile`。它只用于审计与交互，不影响策略评分，且只能由相应 command / adapter 附加，不能由 IPC caller 传入。`document.baseRevision` 是 apply 的唯一并发前置条件，command 不得再接收第二个 `expectedRevision`，以免两者不一致。UI 保存、file watcher、manual import 和 restore 均在 trusted context 中调用同一 internal apply，不再保留两个行为不同的 update command。

`RoutingPolicyDocumentStatus` 至少返回：

```text
path
activeRevision
materializedRevision | null
projectionState: pending_bootstrap | pending_write | synchronized | retry_wait
observationState: unknown | synchronized | invalid_document | conflict | unavailable
mirrorState: synchronized | pending_write | invalid_document | conflict | unavailable
lastObservedDocumentHashPrefix | null
lastErrorCode | null
```

`mirrorState` 是两条 state axis 的稳定派生视图，优先级固定为 `conflict`、`invalid_document`、`unavailable`、`pending_write`、`synchronized`，不得让单一 state 字段同时承担两种语义。接口不得回传原始未解析的无效文件内容，避免将潜在敏感误写内容带入 IPC 或日志。读取 / 导出有效文档可通过专用 command 返回 canonical document；完整路径不得进入日志、错误、fixture、support bundle 或其他 IPC。

## 6. 提交、同步与恢复协议

### 6.1 UI 保存

1. 页面加载 `RoutingPolicySnapshot` 及 document status，建立包含 `baseRevision` 的本地 draft。
2. 用户修改只更新 draft；编辑期可调用 `validate_routing_policy_document` 获得后端 compiler 的可解释结果，但不改变 active policy。
3. 保存时调用 `apply_routing_policy_document`。
4. service 在短 write transaction 之前完成 strict decode、validate 与纯 compile admission；transaction 中只 re-read、比较、CAS、domain revision advance、history append 与 document materialization record upsert。
5. transaction commit 后发布 revision notice，返回新的 snapshot；页面以返回值替换 server state 并清除 draft。
6. file materializer 最终写入 canonical JSON。若写入失败，UI 显示“策略已生效，配置文件待同步”，而不是把本次提交显示为失败。

### 6.2 外部文件编辑

文件 watcher 是输入唤醒信号，不是正确性依据。它必须：

1. 监听专用目录的 create / modify / rename，并进行有界 debounce；所有后续动作交给 `PolicyDocumentCoordinator` 的单一 mutation guard 串行执行；
2. 忽略临时文件与 service 最近 materialize 的 revision + digest；
3. 在 750 ms debounce 后以相隔至少 150 ms 的两次相同 file identity + SHA-256 读取定义 stable read，限制文件大小，严格 decode，validate 并 compile；两次间变化时重新 debounce，不能将编辑中的中间内容标为 invalid；
4. 将完整文档及其 `baseRevision` 传给同一 internal apply，并由 watcher 附加 `file_watch` trusted context；
5. 成功时发布新的 revision 并 materialize canonical 文件；失败时保留 active policy 与原用户文件，document status 进入 `invalid_document` 或 `conflict`；
6. 在 startup、resume、watcher overflow / error 后执行一次 digest reconciliation；运行期间至少每 30 秒对受限大小的目标文件重新计算一次内容 digest，作为静默漏事件和同大小替换的正确性兜底。metadata 只可用于减少额外解析，不能成为跳过该定期 digest 的依据。

原生 watcher 是低延迟唤醒信号；每 30 秒一次的受限文件 digest reconciliation 才是最终发现的正确性兜底，不是高频全文件轮询。应用不得在用户正在输入 JSON 时把中间内容当成已提交配置；只有编辑器完成保存并满足 stable read 条件的文件才会尝试导入。notice、IPC event 和 timer callback 在 coordinator guard 与 SQLite transaction 外发布，避免 reentry / deadlock。

### 6.3 文件与数据库不能原子提交时的处理

SQLite transaction 和 Windows 文件 replace 不可组成单个 ACID transaction。规范固定以下顺序：

```text
validate + compile
  -> SQLite CAS + history + document_sync target upsert (one transaction)
  -> commit active policy
  -> atomically materialize the newest target JSON
  -> conditionally mark that target materialized
```

`routing_policy_document_sync` 是单行、可合并的 projection 状态，不是按 revision FIFO 消费的 outbox。它至少包含：

```text
desiredRevision
desiredCanonicalDigest | null (only pending_bootstrap)
materializedRevision | null
materializedCanonicalDigest | null
lastObservedRawDigest | null
projectionState: pending_bootstrap | pending_write | synchronized | retry_wait
observationState: unknown | synchronized | invalid_document | conflict | unavailable
lastProjectionErrorCode | null
lastObservationErrorCode | null
retryAfterMs | null
attemptToken | null
attemptLeaseUntilMs | null
updatedAtMs
```

migration 只以已存在的 active aggregate seed `desiredRevision` 和 `pending_bootstrap`，不得在 SQL 中伪造应用层 canonical digest。首个 service startup 必须从 aggregate typed-load、validate、compile，并条件回填 digest 后转为 `pending_write`；aggregate 缺失或无法编译是 typed persistence recovery，不是可编辑的 configuration-required。

每次 policy mutation 在同一 transaction 内将 `desiredRevision` 和 canonical digest upsert 为新 revision。materializer 只取得当前最高 `desiredRevision`，并通过 `targetRevision + targetDigest + attemptToken` 条件标记完成；失去其中任意资格的 attempt 只能触发最新 target reconcile，不能把旧目标标记为已 materialize。若 commit 后进程崩溃、磁盘满、权限拒绝或文件被占用，materialization record 保留最新目标与 canonical digest；下次启动、文件可用时或后台重试会重新 materialize。`invalid_document` 和 `conflict` observation 会阻止自动覆盖，直到文件变化或用户明确选择重写；write fault 才走有限 retry/backoff。

因此，任何时刻都以数据库 active revision 决定路由行为。materialization 的目标文档由对应 policy history revision 重建；当目标已被新的 revision 覆盖时，旧目标直接丢弃，不重放旧文件内容。cross-resource compare 与 replace 之间无法防御不合作外部进程的极小竞态；实现必须在 replace 前后核验 target/attempt/digest，检测到失效立即合并并 reconcile 最新目标。它保证最终投影不是旧 revision，不宣称文件在每个瞬间都不可能短暂陈旧。

materialize 时先比较 `lastObservedRawDigest` 与当前文件的 raw digest，再严格解析文件。处理规则固定为：

1. 文件与 active policy 语义相同，即使只因 whitespace、canonical formatting 或 stale `baseRevision` 不同，也不产生新 revision；更新 observation，并将当前 `desiredRevision` 标为待 materialize，以便无 revision bump 地写回当前 canonical 文档。
2. 文件是合法、语义不同且 `baseRevision` 等于 active revision：按普通 file-watch apply 提交。
3. 文件是合法、语义不同但 `baseRevision` 落后：标记 `conflict`，不覆盖文件、不提交数据库。
4. 文件无效或无法稳定读取：标记 `invalid_document` / `unavailable`，不覆盖文件、不提交数据库。

只有用户明确选择“以 active policy 重写文件”时才允许覆盖，并在同目录使用安全、唯一且不可覆盖的 `routing-policy.<timestamp>-<random>.bak` 创建恢复副本。备份限 7 天、最多 5 个，清理只能在成功新建替代物后 best-effort 执行，并只删除经 `ApprovedLeaf` 验证的受管备份。此动作必须再次读取并比较 raw digest；比较不通过则返回 conflict，而不是执行陈旧的覆盖请求。

任意不配合协议的外部进程都可能在极小时间窗强行替换同一文件；操作系统无法替应用保存该进程未提供的版本信息。实现不依赖文件锁，而使用同目录原子 replace、replace 前比较、post-write digest recheck、自写 identity/digest suppression 和 backup 降低该风险，并将结果显示为 conflict / pending，不能宣称绝对双向文件原子性。

### 6.4 冲突协议

所有 conflict 是预期用户状态，不能折叠为 `internal` 或通用网络错误。

```text
RoutingPolicyConflict
├─ baseRevision
├─ currentRevision
├─ source
├─ activeDocumentSummary
└─ draftDocumentSummary
```

摘要只含非敏感策略字段。前端规则：

- 没有未保存 draft：收到 revision notice 后自动 reload；notice 漏失时在 window focus/resume、状态轮询和下一 mutation 前 query revalidate。
- 有 draft：保留草稿，显示“策略已在外部更新”，禁用普通保存，并提供 reload、逐字段合并、明确覆盖三种动作。
- 覆盖必须先基于最新 active document 建立新的 complete draft，再以最新 revision 写入 `baseRevision` 提交；不能复用旧 base revision 重试。
- 文件冲突不自动覆盖 JSON。状态页提供“打开位置 / 重新读取 / 导入当前文件 / 用 active policy 重写（创建备份）”。

### 6.5 运行时切换

commit 成功后，所有新 Planning Snapshot 必须通过 application 的 `load_compiled_policy_for_snapshot` 在与 operational facts 相同的 SQLite read snapshot 中取得编译后的 active revision；immutable compiled cache 只有 revision 精确匹配才可用，revision notice 只是失效 hint。Proxy 不读取文件、不解析 app settings，也不维持可由文件 watcher 就地修改的可变策略对象。

正在执行的请求必须继续使用已记录的 policy revision；最终 decision trace、request log 和模拟结果均带有该 revision。模拟未保存 draft 时使用同一 compiler + planner，并明确标记 `policy_source = draft`，不得被误作 active policy。

## 7. 模块与职责边界

建议完成后的模块边界如下；具体文件名可以随现有模块布局微调，但职责不能混合：

```text
models/routing_policy
  - versioned policy value object, validation, document-neutral types

application/routing_policy
  - RoutingPolicyService, compiler admission, caller-owned write transaction, apply/restore orchestration

application/policy_document_coordinator
  - one mutation guard, coalesced watcher/retry/reconcile/materialize scheduling

persistence/stores/routing_policy_store
  - aggregate + history SQL only; caller-owned transaction primitives

persistence/stores/routing_policy_document_sync_store
  - durable, coalescing desired/materialization and external-observation state SQL only

services/routing_policy_document
  - strict JSON codec, atomic materializer, watcher, reconciliation

ipc/dto/routing_policy_configuration
  - explicit document/status/conflict DTO parsing and serialization

features/routing
  - server query, draft reducer, conflict UI, document sync diagnostics
```

边界规则：

- Document codec 不访问 SQLite、proxy、secret 或 React；其升级和 canonical serialization 必须有 golden fixture。
- Store 不解析文件、不计算评分、不发布事件；它不能自行开启 aggregate mutation transaction，否则无法与 sync upsert 保持原子性。
- File watcher 不包含业务校验、CAS SQL 或路由逻辑；它只调用 service。
- Compiler 不读取文件、数据库、网络或全局 mutable state。
- 前端只格式化后端 validation / conflict / snapshot，不保留另一套默认值、权重校验或评分公式。
- Proxy 只消费 Planning Snapshot / compiled policy，不调用 IPC 或文件系统。

## 8. 策略可配置性的范围

`RoutingPolicyConfigV1` 已公开的权重、候选上限、探索比例、倍率上限、分组筛选、耗尽回退和亲和字段可以进入文档。后续字段只有在满足以下条件时才能新增：

1. 它有明确领域语义、默认值、校验边界与唯一 production consumer。
2. 它不把 eligibility、tier、score 与 runtime dispatch 混成一个可任意覆盖的分数。
3. 它在 decision evidence 中可解释实际贡献或边界效果。
4. 它有 policy config version 升级、golden fixture、compiler 测试与旧文档迁移。
5. UI 不认识新字段时必须返回 typed compatibility error，不能默认丢弃或默默回退。

推荐通过具名 preset 实现“可靠优先”“均衡”“成本敏感”等体验。Preset 只是生成完整配置草稿的产品快捷方式，保存后仍是完整可审计 policy；它不是新增 selector、第二个策略枚举或历史行为兼容层。

## 9. 数据迁移与兼容性

### 9.1 数据库演进

新增 migration：

- 建立 `routing_policy_document_sync` 单例表，保存 desired / materialized revision、canonical/raw SHA-256 digest、projection state、external-observation state、有限错误 code、retry、attempt token / lease 和更新时间；不保存原始无效文档文本或 FIFO task rows。
- migration 只初始化当前 active policy 对应的 `pending_bootstrap` desired revision，canonical digest 为 NULL；第一个 service startup 在 strict-load、validate、compile 后条件回填 digest 并进入 pending write，不能由 SQL 模拟 canonical serializer。
- 为 policy history 保留现有 revision，不重写历史 JSON 或伪造历史来源。
- 将 policy algorithm/system version 字符串迁移到 canonical 常量表示；无法判明旧值的历史只保留原值并标记 legacy provenance，不作为新 active 输入。

Migration 必须是 schema authoring contract 中定义的 append-only 升级，且能在新库、已有数据库、portable import 和 interrupted upgrade 后恢复。当前 schema 已保证 singleton aggregate 存在；若损坏、缺失或无法 compile，必须进入 typed persistence recovery，而不是创建新的默认 policy 或展示 configuration-required 编辑态。

### 9.2 首次启动与文件升级

- 第一次运行新版本时，从 active aggregate 生成 `routing-policy.json`；这不是额外 mutation，不得 bump policy revision。
- 已存在文档时先进行严格读取。如果 `baseRevision` 与 active revision 相同且内容等价，标记 synchronized；若不等价，走 normal apply/conflict，而不是启动时直接覆盖。
- 已支持的旧 document format 只能经明确 `Vn -> Vn+1` upgrader 转换；未知未来版本必须 fail closed，并保留文件原样。
- 更新完成后只保留一个 active document format。兼容 decoder 必须写明删除版本与迁移条件，不允许无限长期累积。

### 9.3 数据目录、备份与便携迁移

- 数据目录 relocation 在启动时必须停止并 drain watcher/coordinator、完成或记录 document materialization state，并在目标目录重新 materialize active document 后才恢复监听。
- 数据库备份和加密 portable package 继续以数据库 aggregate 为权威。`routing-policy.json`、`.bak` 和 watcher observation 均不进入 package / backup manifest；恢复后必须从 restored aggregate + sync state 重新 materialize，不能用来源机器的陈旧文件覆盖导入后的 active policy。
- 用户显式导出 JSON 时仅导出 canonical public document；导入仍经过 apply，不允许替换 SQLite 文件或 patch 任意表。

## 10. 本次必须清理的遗留与耦合

升级完成前必须逐项审计，删除、收敛或改为一次性迁移边界；不得用 deprecated facade、feature flag 或第二条生产路径无限期保留。

| 类别 | 处理要求 |
| --- | --- |
| `LocalRoutingSettingsEditor` 的独立 server state | 改为唯一 `useRoutingPolicy` query + draft reducer；revision notice 统一驱动刷新，删除局部 `load/save` 状态机和 `JSON.stringify` 作为语义比较 |
| 直接 `update_routing_policy` 写入 | 迁移为 document apply command；旧 command/binding 在切换单元内删除，不能保留 deprecated facade 或双实现 |
| 通用错误形式的 stale revision | 新增 typed policy conflict，返回 current revision 与安全摘要；删除前端“仅 toast 一条错误”的路径 |
| 版本字符串字面量 | 收敛到 policy version owner；seed、migration、application save、trace 和测试共用同一值或显式兼容映射 |
| `SettingsStore::canonical_policy_projection` 与 AppSettings policy 字段 | 从通用 settings input/output 删除 `routing_policy_name`、倍率、分组范围、耗尽回退等策略兼容字段；需要展示时仅消费 policy snapshot，禁止 generic settings mutation 回写或投影策略 |
| `RoutingStore::load_execution_settings` 与 proxy startup | 删除其直接读取 `routing_policy.config_json` 的生产路径；代理只接收 application 组装的 Planning Snapshot / compiled policy，不能从 Settings 或 Store 重新拼策略 |
| `RoutingPolicy` enum、`routing_policy_name`、`routing_policy_label` 与旧 ordering profile | 仅在 legacy import/migration 边界保留。清除 routing preview、candidate projection、proxy fallback、前端 option、测试 fixture parser 和新的持久化写入中的生产消费者；替换为 compiled V1 policy 与版本化 decision evidence |
| 页面自建默认值、字段范围和评分解释 | 由 generated DTO 与 service validation 取代，避免 UI/文件/后端三种默认语义 |
| 手写文件写入 | 复用现有 data-store atomic file adapter；删除任何简单 `fs::write` 配置保存实现 |
| 永久双写或双 watcher | coalescing `routing_policy_document_sync` 是唯一的数据库到文件收敛机制；不得建立 FIFO 重放 outbox、独立轮询器或在 proxy 内监听文件 |
| 无消费者的历史 / document status 代码 | 每个表字段、command、事件与 UI 状态必须有测试和明确 consumer；未接线的“未来扩展”不进入生产 |

旧策略值若仍被老数据导入需要读取，必须只在 import/migration 中显式映射到完整 `RoutingPolicyConfigV1`，记录 loss semantics，并在写入后不再回传旧值。这与智能路由规范的 legacy cutover 要求一致。

## 11. 测试与验证契约

### 11.1 Domain / codec

- V1 canonical JSON round-trip、字段排序和稳定格式 golden fixture。
- 所有公开字段、null、discriminated group filter、边界值、权重总和和 affinity 组合。
- 缺失字段、未知字段、重复键、超大文件、超深/超宽 object、非有限数、未知 enum / document format / config version 必须拒绝；无 BOM 与单 UTF-8 BOM 输入必须具有相同语义，其他编码必须拒绝。
- 已支持 document format 的 upgrader 与未来版本 fail-closed。
- UI draft、文件文档和 IPC DTO 经过同一 compiler 得到同一 compiled policy。

### 11.2 Persistence / service

- UI、file watch、manual import、history restore 提交相同文档产生同一 active aggregate 和 revision 语义。
- CAS 成功、语义变化的 stale revision、语义相同且 stale base 的 no-op、compile failure rollback、history append、sync target 与 domain revision 原子性。
- typed conflict 包含正确 current revision，且没有字段被静默覆盖。
- policy revision 与 route Planning Snapshot / simulation / decision trace 一致；进行中请求不被热更新改写。
- migration bootstrap digest、首次 materialize、最新目标合并、应用崩溃于 commit 后、文件权限拒绝、磁盘失败、进程重启和 watcher overflow 后均可收敛；失效 attempt 不得标记旧 revision 已 materialize，最终投影必须回到最新 revision。
- 数据目录迁移、portable import、backup restore 后 active aggregate 优先，文档重新生成且不覆盖新策略。

### 11.3 文件系统与并发

- Windows atomic replace 的每个失败边缘保留旧文件或完整新文件，不能产生半个 JSON。
- 用户写入临时无效内容再完成有效保存时，以两次 stable read 只最终导入一次。
- 自身 materialize 事件不造成递归 revision bump。
- UI 与文件基于同 revision 修改时，至多一方提交；另一方收到 typed conflict。
- raw file hash 不同于最后 observation 时，materializer 不盲写；语义相同的格式变化不 bump revision，语义不同的 stale 文件进入 conflict；用户明确覆盖才创建 `.bak`。
- watcher 事件静默丢失、overflow、resume 和启动均通过每 30 秒的 digest reconciliation 发现文档变化，包括同大小替换；两个 cooperating service instance 的 lease / stale-attempt 测试证明最终收敛。

### 11.4 前端

- loading、sync bootstrap/pending、typed persistence recovery、validation error、invalid document、conflict、disabled save 和窄窗口状态。
- clean UI 自动刷新外部 revision；dirty UI 不丢草稿且能展示 diff / reload / merge / explicit overwrite；notice 漏失时 focus/resume、轮询和 mutation 前 revalidation 仍会收敛。
- 保存成功、文件待同步、冲突、权限错误使用不同的用户可行动文案。
- keyboard focus、表单错误关联、可读 revision 和同步状态满足 accessibility。
- 前端没有本地权重规则、默认策略 normalizer 或评分计算。

### 11.5 质量门禁

本升级属于跨层契约、生成绑定、持久化、文件系统与共享基础设施改动。实现阶段至少运行：

```text
pnpm verify:fast
pnpm build
相关 Vitest
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
相关 Cargo 单元 / integration 测试
IPC 生成与契约检查
数据迁移 / portable recovery 专项检查
```

完成切换和删除遗留路径前必须运行 `pnpm verify:full`。任何未运行的门禁、Windows atomic replace 行为或真实编辑器互操作验证必须如实列为未验证项。

## 12. 实施阶段与退出条件

### Phase 0：基线、契约与删除清单

- 固定现有 policy aggregate、compiler、CAS、planning revision 与 UI 保存行为测试。
- 建立当前所有策略读取 / 写入调用图和 legacy literal inventory。
- 定义 document fixture、重复键拒绝 parser、错误代码、conflict DTO、version 常量、coalescing materialization state 和数据目录路径契约。

退出条件：当前入口、将删除的遗留路径和迁移影响全部列明；不存在未说明的第二个 production writer。

### Phase 1：后端控制面与文档 codec

- 实现 strict document codec、canonical serializer、compiler admission、typed conflict 和 `RoutingPolicyService`。
- 将 UI update command、文件 apply、restore 统一到同一 service。
- 收敛版本字符串和 policy 读取边界。

退出条件：所有来源对同一文档产生相同行为；非法或冲突输入不会改变 active policy。

### Phase 2：持久化同步与恢复

- 添加 coalescing document-sync migration、atomic materializer、watcher、coordinator 和 reconciliation。
- 接入 startup / resume、data-directory relocation、portable import/export 与诊断。
- 覆盖 commit 与文件替换间所有故障边缘。

退出条件：文件和数据库失败可恢复，且 active route policy 永远由已提交 revision 决定。

### Phase 3：前端设置体验

- 以 query + draft reducer 重建路由设置编辑器。
- 提供 validation、sync status、history restore 和三向冲突解决交互。
- 订阅 revision notice 并统一失效 Routing Workspace query family。

退出条件：UI 与外部编辑不会静默丢失草稿或覆盖 active policy，窄窗口与无障碍状态完整。

### Phase 4：切换、清理与资格验证

- 删除旧 UI state machine、旧 update 路径、遗留运行时策略读取和 legacy strategy 生产入口。
- 移除无消费者模块、重复默认值和无契约 watcher。
- 更新 IPC 生成物、文档、迁移 / recovery 清单和测试基线。

退出条件：仓库只有一个 policy command owner、一个 compiler、一个 active aggregate、一个 document synchronizer 和一个前端 draft owner；架构检查不再需要 routing-policy legacy allowlist。

## 13. 验收标准

本升级完成时必须同时满足：

1. 用户在 UI 或 `routing-policy.json` 保存合法完整策略后，后续请求使用相同新 revision。
2. 非法 JSON、未知 schema、校验失败、权限失败、文件损坏或 watcher 失败不会中断现有路由策略。
3. 并发 UI / 文件更新绝不静默覆盖；冲突可定位、可比较、可恢复。
4. active database policy 与文本文件在正常条件下立即同步，在故障条件下通过持久化、只保留最新目标的 materialization state 可诊断地最终收敛；失效 attempt 不能标记旧 revision 成功，遇到不合作 writer 的短暂竞态会检测并 reconcile 到最新文件。
5. route decision、simulation、policy history、runtime diagnostics 都能定位到同一版本身份。
6. policy 文件与 DTO、日志、错误、fixture、support bundle 和 portable migration 均不泄露 secret 或原始认证数据。
7. 旧 selector、重复 settings 读取、前端权威公式与长期双路径已删除，而不是以 deprecated 名义继续运行。
8. 所有第 11 节门禁已通过，或交付明确列出未验证范围与原因。

## 14. 风险与控制

| 风险 | 控制 |
| --- | --- |
| UI 与文件成为双真相 | SQLite active aggregate 为唯一运行时真相；所有入口走同一 service 和 revision CAS |
| 文件保存过程产生半文件 | 同目录 temp + flush + atomic replace + parent sync；watcher 只读稳定文件 |
| 数据库已提交但文件失败 | durable coalescing materialization state、状态诊断和启动 / resume retry；不回滚 active policy，也不重放旧 revision |
| 外部编辑覆盖 UI 草稿 | base revision、typed conflict、dirty draft protection、显式 merge / overwrite |
| watcher 漏事件或 Windows rename 差异 | watcher 仅作 wakeup；每 30 秒 digest reconciliation、启动 / resume / overflow 后检查兜底 |
| SQL migration 无法生成 canonical digest | migration 只 seed pending bootstrap；service strict-load + compile 后条件回填 digest |
| revision notice 丢失导致 cache 陈旧 | notice 仅为失效 hint；Planning Snapshot 以同一 SQLite read snapshot 的 revision fence cache |
| 新字段导致旧版本静默改义 | 严格 document/config version、explicit upgrader、unknown fail closed、golden fixture |
| 为“自定义”开放脚本绕过路由边界 | 只开放受校验字段和 preset；所有 eligibility / capacity / health 保护仍在领域内核 |
| 文件同步代码反向耦合 proxy | proxy 只消费 Planning Snapshot；文件系统只存在于 configuration service 边界 |
| 保留 legacy 路径导致长期维护 | Phase 4 删除清单和 architecture gate；不接受永久 deprecated 双写 |
