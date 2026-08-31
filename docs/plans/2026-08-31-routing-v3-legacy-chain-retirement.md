# Relay Pool Desktop 路由 V3 旧链退役与无回退升级修复计划

状态：P0-P5 代码升级已完成；P6 schema DROP 为 no-go，P7 不具备进入资格。整体计划仅在 P6/P7 资格、不可逆迁移和 post-drop 观察全部完成后才能标记 Completed。

日期：2026-08-31

计划修订：Revision 3；已按实际实施结果修正阶段状态、portable 基线和 P6/P7 资格结论，并保留回退下限、pre/post-drop qualification、circuit persistence gate、一致事务边界和历史 evidence 兼容审计。

实施状态与证据：[`../audits/2026-08-31-routing-v3-legacy-chain-retirement-implementation.md`](../audits/2026-08-31-routing-v3-legacy-chain-retirement-implementation.md)；机器可读删除台账：[`../audits/routing-v3-legacy-retirement-ledger.json`](../audits/routing-v3-legacy-retirement-ledger.json)。P0-P5 的完成只表示 V3-only 代码链已收口，不授权执行 P6 DROP migration。

当前事实与规范入口：

- [`../README.md`](../README.md)
- [`../SCHEMA_UPGRADE_AUTHORING.md`](../SCHEMA_UPGRADE_AUTHORING.md)
- [`../SECURITY_EXPORT_IMPORT.md`](../SECURITY_EXPORT_IMPORT.md)
- [`../release/SCHEMA15_UPGRADE_RECOVERY.md`](../release/SCHEMA15_UPGRADE_RECOVERY.md)
- [`../specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md`](../specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md)
- [`../audits/2026-08-29-intelligent-routing-scoring-circuit-redesign-implementation.md`](../audits/2026-08-29-intelligent-routing-scoring-circuit-redesign-implementation.md)
- [`2026-08-29-intelligent-routing-scoring-circuit-redesign.md`](2026-08-29-intelligent-routing-scoring-circuit-redesign.md)
- [`2026-08-26-routing-workspace-planner-admission-alignment.md`](2026-08-26-routing-workspace-planner-admission-alignment.md)
- [`2026-08-23-routing-ownership-lifecycle-cleanup.md`](2026-08-23-routing-ownership-lifecycle-cleanup.md)
- [`../audits/routing-retry-failover-deletion-ledger.md`](../audits/routing-retry-failover-deletion-ledger.md)

本文是减法和迁移计划。实施时以当前代码、自动化契约和上述当前规范为事实来源；不得为了完成删除清单而修改 V3 评分、重试、熔断、质量统计、模型能力或 endpoint 健康语义。

---

## 1. 目标与完成定义

本计划解决 V3 路由上线后仍存在的四类遗留：

1. `routing_health_snapshot` 与 V3 `StationKeyCircuitStore` 同时向 UI 提供 Key 状态，形成双事实源。
2. 请求、主动监控和手动连通性仍写旧 health ledger/snapshot；请求终态仍写非 V3 admission 所需的 scoped breaker verdict。
3. protection、Key Pool、全局 Settings 和 runtime execution settings 仍读取或携带旧 policy/health 字段。
4. 无生产消费者的旧 command、IPC、facade、store 方法、capacity-domain 服务和 test-only planner 仍留在生产组合或生成契约中。

全部完成必须同时满足：

- Proxy 选路仍只经过 `PlanningSnapshotBuilder -> intelligent planner -> StationKeyCircuitStore -> local capacity -> outbound`。
- workspace、runtime overlay、Key Pool 和 protection 对 Key 可用性、circuit state 和 cooldown 使用同一 V3 durable circuit 事实。
- 真实请求和主动监控仍产生完整的 V3 quality observation；停止旧 health 写入不能减少质量样本或改变来源权重。
- `model_on_key / unsupported_model` 能力判定仍能跨重启生效，模型不支持的 Key 不会重新进入对应模型的候选集合。
- `endpoint_health_snapshot`、站点 endpoint ping 和手动连通性诊断能力保持独立、可用且不被误删。
- V1/V2 decoder、历史 trace decoder、历史 migration 和 portable import 兼容数据保留到明确的兼容窗口结束。
- 旧 IPC、DTO、字段、writer 和表只有在生产 consumer 为零、回归齐全且满足降级窗口后才删除。
- 前端不再从 `healthState/cooldownUntil/consecutiveFailures` 猜测 V3 路由资格，也不会把暂停 Key 或 Open Key显示为可用。
- station-key connectivity operation 的启动、进度、取消、typed terminal/result、超时和错误语义保持不变；只允许移除其旧 health 副写。
- request attempt、circuit、quality observation、request terminal outbox 和 cost 的现有事务/幂等边界保持不变；旧链清理不得把它们重组为一个大事务。
- 任何阶段都存在明确的 rollback floor、可部署产物和 go/no-go 证据；进入 P4 后不得回退到仍以旧 health 表为事实源的版本。

本计划不新增账号、云同步、跨设备 circuit、站点级 breaker、容量域路由或新的用户可调保护器。

---

## 2. 不可回退的能力基线

以下能力是升级的硬门，不允许以“旧链清理”为理由弱化。

| 能力 | 唯一 owner / 事实 | 迁移期间要求 |
| --- | --- | --- |
| 候选硬资格 | `PlanningSnapshotBuilder` assessment | 不在 workspace、Key Pool 或前端重写资格规则。 |
| 同层确定性评分排序 | V3 intelligent planner | 不恢复随机探索、旧 strategy enum 或 scheduler advanced settings。 |
| Key 熔断与恢复 | `StationKeyCircuitStore` + V3 reducer | Open/Half-Open/lease/revision/cooldown 语义不变。 |
| 本地容量准入 | process-local capacity registry | 保留后置硬门；不恢复 capacity-domain 身份或跨域 fallback。 |
| 真实请求质量样本 | `routing_observations` V3 ingestion | 停旧写时必须证明样本数、归责、去重和 event time 不变。 |
| 主动监控质量样本 | monitoring -> V3 observation ingestion | 监控影响质量评分，但不得直接打开 Key circuit。 |
| 模型能力记忆 | durable `model_on_key / unsupported_model` verdict | 必须保留持久化、生命周期 revision、批量读取和失效语义。 |
| endpoint 连通性 | `endpoint_health_snapshot` | 与 Key circuit 分开；不能用 circuit 取代 endpoint ping。 |
| station-key 手动连通性 | connectivity operation + typed result | 保留启动、进度、取消、超时和错误契约；不把诊断结果升级为 circuit/quality。 |
| transport timeout 热加载 | transport policy store | 与 protection/circuit read model 解耦后仍可查询和编辑。 |
| 历史数据读取 | decoder + immutable migrations | 新版本可读旧库；不修改已发布 migration。 |
| 终态持久化可靠性 | 既有 attempt transaction + terminal outbox + cost transactions | 保留原子边界、幂等键、重试和 crash recovery，不因删除旧写而合并事务。 |

### 2.1 明确禁止的迁移捷径

1. 不允许先删 `routing_health_snapshot`，再让缺失字段回退为 `unknown`。
2. 不允许 V3 circuit 读取失败时回退旧 health snapshot；这会重新建立双 owner。读取失败必须显式返回 read-model unavailable，Proxy admission 继续使用自身既有 fail-closed 语义。
3. 不允许同时保留两个 authoritative writer 并无限期“观察”；双写只能作为有结束日期的对账阶段。
4. 不允许把旧 health 的 `consecutive_failures` 转抄成 V3 circuit state。旧窗口/冷却语义与 V3 连续失败 reducer 不等价。
5. 不允许整体删除 `RoutingHealthVerdictStore` 或 `routing_health_verdicts`，直到 `unsupported_model` 已迁入等价 durable owner并通过重启回归。
6. 不允许删除或改写 `0010`、`0026`、`0060`--`0071` 等历史 migration；schema 删除必须新增迁移。
7. 不允许手工修改 `src/lib/bridge/generated.ts`；IPC 变化必须修改 registry/DTO source 后运行生成脚本。
8. 不允许通过修改 fixture 预期、降低 architecture gate 或忽略旧库升级测试来让删除通过。
9. 不允许在 P4 停止旧 writer 后回退到 P0/P1 或任何仍读取旧 snapshot 的应用版本；表仍存在不代表数据仍可作为事实。
10. 不允许让前端重新实现 planner/circuit admission。候选参与状态、reason 和全量汇总由后端产生，前端只负责展示。
11. 不允许把 `probe_state_revision` 等已持久化 evidence 字段直接换名；字段演进必须版本化并继续解码旧名称。
12. 不允许在 pre-drop release qualification、verified backup 和恢复演练完成前执行任何 DROP migration。

---

## 3. 当前链路与目标链路

### 3.1 当前状态

```text
Proxy request
  -> V3 PlanningSnapshot / score / circuit admission
  -> outbound
  -> terminal
       -> V3 circuit event/state
       -> V3 routing observation / quality
       -> legacy station_key health observation/snapshot
       -> legacy scoped health observation/verdict

Monitoring
  -> V3 routing observation / quality
  -> legacy station_key health observation/snapshot

Workspace / runtime overlay / Key Pool
  -> V3 score/circuit diagnostics
  -> legacy health state/cooldown fallback

Protection API
  -> legacy scoped verdict + legacy snapshot + disabled reducer
  -> transport timeout facts
  -> no direct V3 circuit projection
```

### 3.2 目标状态

```text
Proxy request
  -> V3 PlanningSnapshot / score / circuit admission
  -> outbound
  -> terminal
       -> V3 circuit event/state
       -> V3 routing observation / quality
       -> model capability verdict only when applicable

Monitoring
  -> V3 routing observation / quality

Workspace / runtime overlay / Key Pool / circuit diagnostics
  -> one V3 station-key circuit read model
  -> explicit read-model status and revision

Transport timeout UI
  -> dedicated transport timeout facts query

Endpoint connectivity UI
  -> endpoint_health_snapshot / endpoint probe read model
```

迁移顺序固定为：**冻结基线 -> 建立 V3 等价读模型 -> 后端切读 -> 前端切读 -> 停旧写 -> 删除代码/API -> 兼容窗口与 pre-drop qualification -> schema 清理 -> post-drop 验收**。不得交换“切读”和“停写/删表”的顺序，也不得把最终验收首次放到不可逆 DROP 之后。

### 3.3 回退下限

| 已进入阶段 | 允许的最低回退目标 | 禁止事项 |
| --- | --- | --- |
| P0--P1 | P0 基线版本 | 不需要数据恢复。 |
| P2--P3 | P1/P0 版本；旧 writer 仍在持续更新 | 不得用运行时 fallback 长期双读。 |
| P4--P5 | 最低为“V3 read side 已切换”的 P2/P3 构建 | 不得回退到读取 `routing_health_snapshot` 的版本；若 P4 代码需撤销，只恢复旧副写代码，V3 仍是 read owner。 |
| P6 DROP 前 | P4/P5 的 V3-only read 构建 | 必须保留旧表且禁止旧生产读写。 |
| P6 DROP 后 | 当前 schema 兼容构建或前向修复版本 | 不支持应用二进制直接降级；只能使用已验证的同设备备份恢复流程或前向 migration。 |

每个发布候选必须在实施审计中记录 source revision、schema version、read-owner version、最低回退版本和备份/恢复资格。不能只写“可回退”而不指定可安装产物。

---

## 4. 分阶段实施计划

### P0：冻结调用图、能力矩阵与可比较基线

**目标：** 在任何生产行为变化前，证明哪些旧代码是真正无 consumer，哪些只是名称旧但仍承载 V3 能力。

**预期文件**

- Update: `docs/audits/routing-retry-failover-deletion-ledger.md`
- Create: 本任务的脱敏实施审计记录，例如 `docs/audits/2026-08-31-routing-v3-legacy-chain-retirement-implementation.md`
- Update: focused Rust/Vitest fixtures
- Update or create: routing legacy-owner architecture check

**步骤**

1. 生成生产调用清单，至少覆盖：
   - `routing_health_snapshot`、`station_key_health_observations` 的所有读写点；
   - `RoutingHealthVerdictStore` 每个 public 方法的生产/测试 caller；
   - `routing_error_rate_history` 的 command、reader、store 和测试 caller；
   - `StationCapacityDomainService`、command、facade、composition、table/catalog caller；
   - `RuntimeRoutingSettings.policy/scheduler_config` 和全局 Settings 兼容字段；
   - `list/get_station_key_health`、`get_station_key_operational_detail` 的 IPC/ACL/bridge/frontend caller；
   - `CircuitPersistenceGate` 的 process-local/durable gate 读写点，以及 workspace/protection 当前如何感知 gate；
   - `probe_scope/probe_state_revision`、`health_revision`、`health_applied` 等跨 port、DTO 或 evidence 的遗留字段；
   - command registry、窗口 ACL、compiled ACL、生成 bridge、serialization fixture、command-facade matrix 和 upgrade inventory 中的对应条目。
2. 将 caller 标为 `v3_required`、`migration_read`、`legacy_ui_read`、`legacy_write`、`test_only`、`no_consumer`，并为每项写删除条件。台账必须使用稳定 ID，并至少记录 owner、consumer、replacement、earliest removal phase、rollback floor、compatibility class 和验证命令；architecture gate 读取同一台账，避免文档与门禁维护两套清单。
3. 固定当前 V3 生产行为 golden fixtures：候选顺序、score、Open/Half-Open/Closed、同 Key 连续失败、跨 Key retry、监控样本、unsupported model、endpoint ping、station-key connectivity operation 的启动/进度/取消/typed result/timeout/error、Settings 读取和 timeout 热加载。
4. 冻结终态事务拓扑和故障注入基线：
   - attempt terminal、V3 circuit 和 capability effect 当前在哪个 write transaction 提交；
   - request terminal outbox 的 enqueue/reconcile/crash recovery；
   - quality observation 的幂等键和 generation eligibility；
   - attempt cost/request cost 的独立事务；
   - 任一步骤失败、重复执行或进程重启后的可观察结果。
5. 准备至少五类数据库 fixture：
   - 全新 V3 数据库；
   - 从 V2/旧 health 数据升级且 V3 generation active 的数据库；
   - 旧 health 与 V3 circuit 故意冲突的数据库，例如旧 snapshot cooldown、V3 circuit closed。
   - 冻结的最低自动升级基线 schema `15`；
   - migration 中断、postcondition 失败和未来 schema 拒绝场景。
6. 记录当前最高 migration 编号、portable catalog 数量和 fingerprint。P0 实施后基线为 schema `0071`、portable catalog `111` 张用户表、fingerprint `bc8b675f90012fe6179bd489170e24937600de6f711b79cb83952a943efbec48`。真正实施 schema 任务前必须重新扫描，不能提前占用编号。
7. 定义 shadow/soak 指标契约：指标 owner、采样点、分母、最小样本、观察窗口、阈值、脱敏与保留期限。默认只保留聚合 reason/count，不记录 Key ID、URL、secret、原始错误或正文；本地工具不存在集中遥测时，证据来自可导出的脱敏本机 qualification report 和自动化 fixture，不能虚构“全量用户 rate”。
8. 建立敏感信息门：fixture 只使用明显假 ID/secret，comparison report 不保存 URL、API key、Authorization、原始错误或请求正文。

**完成门**

- 每个待删符号有 caller 证据和删除条件。
- 冲突 fixture 能稳定复现“旧 UI 状态与 V3 circuit 不一致”，且真实 Proxy 仍以 V3 为准。
- 事务拓扑、幂等键、rollback floor、可观测指标和 station-key connectivity 契约已经由自动化固定。
- schema/portable 基线与 `docs/README.md`、registry 和 fixture 一致。
- P0 只增加测试、审计和门禁，不改变生产读写。

**回滚：** 仅删除新增测试/审计即可；无运行时或 schema 状态变化。

---

### P1：建立统一的 V3 station-key circuit 读模型

**目标：** 让非 Proxy 查询方批量读取与 Proxy admission 同源的 circuit 事实，但不改变任何现有 UI 输出。

**设计约束**

- `routing_circuit_state_v3` 的 mutable admission state 和 `CircuitPersistenceGate` 共同决定 Proxy 当前是否可安全使用 circuit；generation checkpoint 只用于 rebuild/qualification，不能代替 mutable admission state 成为 UI owner。
- 优先复用 `StationKeyCircuitStore` 已有批量 status API；只有现有 API 无法按当前 lifecycle 有界读取，或无法提供 read-model 所需 revision/status 时才增加窄 query。
- read model 只投影事实，不执行 admission、不申请 Half-Open lease、不推进 reducer。
- Key 身份必须包含 `station_key_id + station_key_lifecycle_revision`；旧生命周期状态不得附着到新凭据。
- 输出至少包含 `state`、`state_revision`、`policy_revision`、`cooldown_until_ms`、`reopen_level`、Half-Open lease 是否占用、persistence gate status/revision、read-model status、source/schema version 和生成时间；不得暴露 lease/attempt identity。
- `schedulable`、planner `score_status`、circuit state、本地容量是不同概念，不能合成一个无来源的 `available` 布尔值。
- read model 必须支持后续增加新的诊断字段而不改变既有字段语义；IPC DTO 使用版本标记和 typed enum/reason，未知扩展字段由旧前端忽略，不能复用旧字段表达新含义。

**预期文件**

- Modify: `src-tauri/src/persistence/stores/station_key_circuit_store.rs` only if needed
- Modify: `src-tauri/src/application/queries/routing_workspace.rs`
- Modify or create: narrow circuit query/projector under existing routing query boundary
- Update: circuit store/query focused tests

**步骤**

1. 定义共享的 application-local `StationKeyCircuitReadFact` 和 `CircuitReadSnapshotRevision`；不要直接向 UI 暴露 persistence row，也不要让每个 query 自行映射 circuit。
2. 增加 `CircuitPersistenceGate` 的只读 snapshot API。读取 gate revision/status 前后包围 caller-owned DB read transaction；若 revision 在读取期间变化，则有界重试一次，仍变化时返回 `read_model_unavailable`，不得拼接不一致快照。
3. 在 DB read transaction 中按当前 Key lifecycle 有界加载 mutable `routing_circuit_state_v3`。Key Pool 等非 planner consumer 不得假设最多只有 1024 行；使用稳定分页、JSON table 或有界 chunk，避免 SQLite bind 上限和全表陈旧 lifecycle 扫描。
4. 对不存在 state row 的当前 Key 投影为明确的 `closed/default` 或 `unavailable`，具体选择必须与 Proxy admission 的 `ensure_state` 既有语义一致，并由测试固定，不能由 UI 自行猜测。gate active 时即使 durable row 为 Closed，也必须显式显示 persistence unavailable，不能报告可参与。
5. 增加 shadow comparison，仅记录聚合计数和稳定 reason：
   - old cooldown + V3 closed；
   - old ready + V3 open；
   - lifecycle mismatch；
   - old row missing；
   - V3 row missing/unavailable；
   - process/durable persistence gate active；
   - gate revision changed during read。
6. comparison 不要求旧状态与 V3 状态相等。旧监控写会造成预期差异；报告的目的只是确认切换影响面，不把旧状态重新提升为权威。
7. 为 0/1/1024/1025 个 Key、超过单页数据、重复 ID、stale lifecycle、mutable state 缺失、gate active、gate revision race、数据库读取失败和读取无写副作用增加边界测试。

**完成门**

- 新 query 对真实 Proxy circuit 状态的投影与 store status 完全一致。
- gate active/race 场景中，workspace、protection 和 Proxy 都 fail-closed，revision vector 可解释同一快照。
- query 无写副作用且不会申请 lease。
- 生产 UI/IPC 仍未切换，P1 可单独发布而不改变用户行为。

**回滚：** 移除未被生产消费的新 read model；数据库无变化。

---

### P2：后端 read side 切换到 V3 circuit

**目标：** 先让所有后端 DTO 从 V3 circuit 取 Key 状态，再停止旧写入。

#### P2.1 Routing Workspace

1. `load_routing_workspace_snapshot` 继续由 planner assessment 决定 `score_status` 和排除原因。
2. candidate diagnostics 的 circuit 来自 P1 读模型；`health_state` 若仍为兼容 DTO 字段，只能由 V3 circuit 映射，不能读取 `CanonicalRoutingCandidate.health`。
3. 后端新增 typed `participation_status/participation_reason` 展示投影。它只描述当前 planner assessment、circuit score gate 和 schedulable 事实，不执行 admission，也不承诺瞬时容量 lease；不得由前端重算。
4. `availability_status` 和汇总计数使用显式规则，并在分页前对完整候选集合计算：
   - `schedulable=true`；
   - `score_status` 可参与本轮基线评估；
   - Closed、冷却中的 Open、冷却结束且 score gate 通过的 Open、空闲/占用的 Half-Open 使用不同 typed reason，不能用 `state != Open` 简化恢复资格；
   - 不把 runtime capacity 瞬时耗尽误写为 durable circuit Open。
5. workspace DTO 增加明确的 read-model version、`CircuitReadSnapshotRevision` 和 full-set aggregates；当前页 rows、总数和全量状态计数分开，UI 不从分页 rows 推算全局计数。
6. durable circuit/gate 读取失败时，workspace 顶层返回明确的 read-model unavailable/code；不得回退旧 snapshot。

#### P2.2 Runtime Overlay

1. overlay 只负责 process-local `in_flight/station_key_in_flight` 和必要的 circuit 展示刷新。
2. 删除从 `load_runtime_candidates().health` 构造 `health_state/cooldown_until` 的路径。
3. 如果 durable circuit 已在 workspace 同一响应中提供，评估是否从 overlay DTO 移除重复 circuit 字段；若保留，必须携带 revision 并由前端只接受匹配 lifecycle/revision 的数据。

#### P2.3 Key Pool

1. 删除 `credential_store::list_key_pool_items` 对 `routing_health_snapshot` 的 JOIN。
2. Key Pool 的行政字段、能力、endpoint ping 和 V3 circuit 分开投影。
3. `cooldownUntil/consecutiveFailures/successCount/failureCount/avgLatencyMs/lastErrorSummary` 中属于旧 health 的字段从 Key Pool DTO 退役；仍需展示的质量统计必须来自 V3 quality summary，而不是换名继续读旧表。

#### P2.4 Protection 与 timeout

1. 新增版本化 `get_routing_circuit_status`（例如 `statusVersion='routing_circuit_status_v1'`），直接投影 P1 的 V3 station-key circuit 与 gate status；`get_routing_protection_status` 只保留一个有明确删除版本的短期 adapter，不在原 V1 DTO 中复用旧字段承载新语义。
2. scope kind 固定为当前 V3 用户语义可识别的 `station_key`；不得把 `station_key_credential` 冒充 Key circuit。
3. 将 transport timeout facts 拆为独立 query/DTO；设置页不再通过旧 protection rows 的可用性推断 timeout read model。
4. capacity runtime fact 若继续展示，必须明确为 `runtime_capacity`，不进入 circuit 列表且不持久化。

**预期文件**

- Modify: `src-tauri/src/application/routing.rs`
- Modify: `src-tauri/src/application/queries/routing_workspace.rs`
- Modify: `src-tauri/src/application/queries/routing_protection.rs`
- Modify: `src-tauri/src/persistence/stores/routing_store.rs`
- Modify: `src-tauri/src/persistence/stores/credential_store.rs`
- Modify: routing command facade/DTO/registry sources
- Regenerate: IPC TypeScript bindings

**完成门**

- `rg` 证明 workspace、runtime overlay、Key Pool 和 protection production query 不再读取 `routing_health_snapshot`。
- 冲突 fixture 中，所有用户可见 circuit/cooldown 与 V3 一致。
- per-candidate participation、分页前 full-set aggregates、circuit/gate revision vector 由后端一次产生，前端无需重建资格规则。
- endpoint ping 字段仍来自 `endpoint_health_snapshot`。
- 新旧 command adapter、registry、ACL、serialization fixture 和 generated bridge 在兼容窗口内契约一致，且 adapter 有明确移除阶段。
- P2 完成后旧 writer 仍暂时运行，因此回滚应用版本不依赖数据重建。

**回滚：** 在 schema 未删除前可回退到 P1/P0 版本；不使用运行时 fallback 开关长期保留双读。

---

### P3：前端切换并删除旧 health fallback

**目标：** 前端只解释后端提供的 planner/circuit/quality 事实，不自行拼接旧健康状态。

**预期文件**

- Modify: `src/lib/types/routingWorkspace.ts`
- Modify: `src/lib/types/stationKeys.ts`
- Modify: `src/features/routing/RoutingStatusDiagnosticsPanel.tsx`
- Modify: `src/features/routing/LocalRoutingStatusCandidateRow.tsx`
- Modify: `src/features/routing/editableRoutingCandidates.ts`
- Modify: `src/features/routing/routingProtectionPresentation.ts`
- Modify: `src/features/routing/LocalRoutingSettingsEditor.tsx`
- Modify: Key Pool view models only where old health fields are displayed
- Update: generated bridge consumers and focused Vitest

**步骤**

1. 删除前端候选资格组合逻辑；状态页计数直接使用后端 full-set aggregates，候选 badge 和评分明细直接解释后端 typed `participation_status/participation_reason`。前端共享函数只能做文案/视觉映射，不能重新决定资格。
2. `availableCount`、总候选数和当前页行数必须分别使用后端对应字段；不得从分页 rows 重新统计，也不能依赖“当前后端通常会把暂停 Key 变成 excluded”的隐式不变量。
3. 删除以下 fallback：
   - `candidate.healthState === "cooldown" -> circuit open`；
   - `candidate.cooldownUntil -> cooldown deadline`；
   - Key Pool `consecutiveFailures/cooldownUntil -> degraded/cooldown`；
   - protection 对 `legacy_compatibility` row 的过滤后空展示。
4. V3 circuit read model unavailable 时显示明确 unavailable，不显示旧值、不显示默认 healthy。
5. 保持 loading、empty、error、disabled、窄窗口、键盘和焦点行为；刷新期间可保留上一次成功的 V3 数据，但必须显示 stale/error 状态。
6. 对后续新增的可选诊断字段保持向前兼容；typed enum 新增值必须触发明确的 `unknown/unavailable` 展示或编译期穷尽检查，不能落入 healthy 默认值。
7. 更新 DemoBackend 和测试 fixture，使它们显式提供 V3 circuit/read-model status、revision vector 和 full-set aggregates，不以旧字段维持编译通过。

**完成门**

- 暂停 Key、Open Key、Half-Open lease 已占用、无评分 Key、read-model unavailable 的 UI 断言齐全。
- 分页、stale snapshot、未知扩展 reason 和 gate active 的 UI 断言齐全。
- 前端生产代码不再读取旧 health fallback 字段。
- `pnpm build` 和相关 Vitest 通过。

**回滚：** P2 DTO 在兼容窗口内可暂时保留旧字段但前端不消费；回退应用仍能读取未删除的旧表。

---

### P4：停止旧 writer，同时保持 V3 quality、circuit 和 capability 完整

**目标：** 在所有生产 read side 已切换后，关闭重复写入 owner。

#### P4.1 请求终态

1. 冻结并保持 P0 记录的事务拓扑：
   - attempt write transaction 继续原子提交其当前拥有的 durable attempt terminal、V3 `StationKeyCircuitStore::finish_attempt` 和适用的 capability effect；
   - request terminal 继续使用现有 outbox enqueue/reconcile 和 crash recovery；
   - V3 quality observation 继续使用既有幂等键、generation eligibility 和当前所属事务；
   - attempt cost/request cost 保持独立事务，不并入 attempt 或 terminal outbox。
2. 只从既有 attempt transaction 删除 `attempt_health_observation -> HealthTransitionService::record_observation`；删除前后其他 SQL side effect、commit 点、错误传播和重试次数必须由 differential/fault-injection test 证明一致。
3. 按 P0 caller 台账区分旧 scoped health probe metadata 与 V3 circuit lease metadata，再删除旧 `AttemptHealthUpdate::ProbeSuccess/ProbeFailure`、`probe_scope`、scoped probe cancel/recovery 分支。不能仅凭字段名推断语义。
4. `probe_state_revision` 已进入持久化 observation evidence/decoder，不能原地换名。若仍需内部 `circuit_lease_revision`：
   - 新的内存字段使用准确名称；
   - persisted evidence 增加 schema/version 或新字段，decoder 同时接受旧 `probe_state_revision` 和新字段并定义优先级；
   - 历史行不重写，V1/V2/V3 fixture 全部可读；
   - 若没有生产 consumer，优先删除无用投影，不为改名新增持久化负担。
5. `AttemptCommitAck.health_applied`、`AttemptPersistenceResult.health_applied` 和相关 test double 在旧 health owner 删除后重新做 caller 证明；无行为 consumer 时一并删除，不能永久保留恒 false 的旧语义。
6. 非模型 scoped `HealthEffect` 的分类信息继续进入脱敏 attempt/observation 诊断，但不再写成第二个路由 breaker verdict。不能因删除 verdict 写入而丢失 public error code、failure attribution、retry disposition 或 quality exclusion。

#### P4.2 主动监控

1. `monitoring/write_path` 继续写 monitoring execution/attempt/target 和 V3 routing observation。
2. 删除 `HealthTransitionService` 写入。
3. 固定回归：监控成功/失败仍按 source weight 进入 quality summary；监控失败不直接改变 V3 circuit state。

#### P4.3 手动连通性

1. 手动 station endpoint ping 继续写 `endpoint_health_snapshot`。
2. station-key connectivity operation 保持现有启动、进度序列、取消、typed terminal/result、timeout、result-unknown 和错误映射；只删除 `station_key_health_observations` suppressed 副写。
3. bounded diagnostic result 的 owner、保留时间和重启语义沿用当前 operation contract；不得为了替代旧 health history 新建无界诊断表。
4. 不把手动测试升级成 V3 quality sample 或 circuit event，除非当前规范另行批准。

#### P4.4 Scoped verdict store 收敛

1. 保留并单独测试：
   - `load_unsupported_model_batch`；
   - `apply_unsupported_model`；
   - capability lifecycle/revision invalidation；
   - rebuild/active generation 中确实仍服务 capability 的最小方法。
2. 删除或隔离旧职责：
   - `apply_error_rate_observation`；
   - `load_health_protection_reducer/statuses`；
   - `begin/cancel/apply_health_protection_probe`；
   - 非 `model_on_key` 的 credential/account/group/endpoint breaker observation 写入；
   - startup `ensure_health_protection_state`。
3. 若同一 store/table 同时承载 capability 与旧 health，先把 Rust API 收敛为 capability-only；本阶段不强制改表名或移动数据。

**预期文件**

- Modify: `src-tauri/src/application/request_finalization/mod.rs`
- Modify: `src-tauri/src/application/request_lifecycle/attempt.rs`
- Modify: `src-tauri/src/application/monitoring/write_path.rs`
- Modify: `src-tauri/src/application/routing.rs`
- Modify: `src-tauri/src/persistence/stores/routing_health_verdict_store.rs`
- Remove after caller count reaches zero: `application/health_transitions.rs`
- Remove after caller count reaches zero: `persistence/stores/health_observation_store.rs`
- Update: request lifecycle, monitoring, circuit, quality and capability tests

**完成门**

- 生产代码不再 INSERT/UPDATE `station_key_health_observations` 或 `routing_health_snapshot`。
- 同一 real request 在迁移前后产生等量、等归责的 V3 quality observation 和 circuit event。
- monitoring quality sample 数、source、outcome、event time 和 dedupe 不变。
- unsupported model 在写入、重启、规划读取、Key lifecycle 变化后表现与迁移前一致。
- V3 Half-Open success/failure/late result/reaper 测试全部通过。
- attempt/outbox/quality/cost 的事务数量、commit 点、幂等结果、故障注入和重启恢复与 P0 基线一致；仅旧 health/scoped breaker side effect 消失。
- station-key connectivity operation 的 progress/cancel/result/error 合同与 P0 golden fixtures 一致。

**回滚：** P4 发布后旧表仍存在但立即开始陈旧，rollback floor 因此提升到 P2/P3 的 V3-read 构建。不得回退到读取旧 snapshot 的 P0/P1 版本。若 P4 实现需要撤销，只能在保持 V3 read owner 的前提下恢复旧副写代码，或发布前向修复；旧表不能重新成为事实源。

---

### P5：删除无 consumer API、旧 policy shadow 与 capacity-domain 代码

**目标：** 在行为迁移完成后清理生产组合、公开契约和测试负担。

#### P5.1 可直接删除的 command/API

- `list_error_rate_history` command/facade/reader；其未注册、无 ACL、无前端 consumer。
- `get/upsert/clear_station_capacity_domain` command、DTO、facade、service 和 composition wiring；其不在当前 IPC registry/UI 路径。
- `RoutingService::load_health_protection_statuses/begin_health_protection_probe/cancel_health_protection_probe` 空兼容 facade。
- 无生产前端 consumer 的 `list_station_key_health`、`get_station_key_health`、`get_station_key_operational_detail` IPC/API/query wrappers；删除前更新 registry、ACL、compiled ACL、BackendClient、DesktopBackend、DemoBackend、serialization fixture、command-facade matrix、upgrade inventory 和生成绑定。
- `RoutingPolicyStore::save_compare_and_swap` 及仅由它使用的旧 validation helper。
- `RoutingObservationStore::list_after/list_for_scope/list_for_scopes` 等仅旧测试调用的 reader；V3 readers/decoder 保留。

#### P5.2 Runtime policy/settings 收敛

1. 从 `RuntimeRoutingSettings` 删除固定的旧 `RoutingPolicy` 和无运行时作用的 `scheduler_config`。
2. `RouteRequestFacts` 的 ordering/profile 和 trace label 直接来自 active V3 policy/profile；不得从 `AutomaticBalanced` 常量生成影子语义。
3. 从全局 `AppSettings/UpdateSettingsInput`、IPC DTO 和 TypeScript 类型删除：
   - `defaultRoutingStrategy`；
   - `schedulerAdvancedSettings`；
   - 作为 settings 兼容输入的 `maxRateMultiplier`；
   - `defaultRoutingGroupFilter`；
   - `allowDepletedFallback`。
4. 路由设置继续只通过 V3 policy document API 读写上述仍有效的业务字段。
5. `settings_store::canonical_policy_projection` 不再直接解析 legacy `routing_policy` V2 行；需要展示 V3 policy 时调用 effective active V3 reader，否则彻底移除该投影。
6. 为每个删除字段记录用户可见 replacement、历史 decoder owner 和 portable import 行为；没有等价 replacement 的字段必须经过显式产品决策，不能仅以“当前 UI 未展示”为由删除能力。

#### P5.3 Test-only 旧 planner

1. 删除 `routing_engine/coordinator.rs`、`eligibility.rs`、`hierarchical_preview.rs` 前先把仍有价值的行为断言迁到 V3 planner/circuit/retry 测试。
2. `candidate_plan.rs`、旧 runtime health helper 等若只为上述测试存在，按 caller tree 一并删除。
3. 不删除 V3 planner 为兼容历史 trace 所需的 enum/decoder。

#### P5.4 Capacity-domain 数据边界

1. 生产 Rust service/model/store wiring 可在 consumer 为零后删除。
2. `station_capacity_domains` 表、portable catalog 和历史 migration 在 P5 保留，确保旧数据库、导入包和降级窗口不受影响。
3. `RetryFailoverPolicyV2.allow_cross_capacity_domain_fallback` 等字段作为 V2 decoder 输入保留；V3 runtime profile 不携带这些字段。
4. 未来若重新引入 provider/region/failure-domain 能力，必须通过新的受批准规格、明确 owner 和 additive contract 实现；不得复活本次删除的 legacy capacity-domain service 或复用历史字段改变语义。

**完成门**

- command registry、源码/compiled ACL、生成 bridge、serialization fixture、command-facade matrix、upgrade inventory 与实际 command 集合一致。
- `audit:dead-code` 不再报告本计划删除的生产引用。
- 与删除 owner 关联的 `#[expect(dead_code)]`/contract exception 同步移除；不留下恒 false/恒空 compatibility port。
- architecture gate 禁止重新从 workspace/Key Pool/settings 引入旧 health/policy/capacity-domain source。
- V1/V2 旧配置和旧数据库仍能升级到当前 V3 active generation。

**回滚：** P5 只删代码和公开面，不删历史表，但 rollback floor 仍是 P4 的 V3-read 构建。IPC 移除属于版本化本地客户端契约变化，必须与桌面前后端同版本发布，不支持混用旧前端 bundle 与新 Rust command set；若仓库以后声明支持外部 IPC consumer，必须另加弃用周期，不能沿用本计划的同 bundle 假设。

---

### P6：兼容窗口、pre-drop qualification 与 schema 清理

**目标：** 先用可复现证据证明 V3-only 代码稳定且可以恢复，再物理删除旧数据结构。P6 必须作为独立发布评审，不与 P2--P5 合并。

#### P6.1 兼容窗口与量化门

进入 pre-drop qualification 前必须同时满足：

- 至少发布并支持一个不含 DROP migration 的 V3-only read/write 版本，且经过不少于 7 个连续自然日的兼容窗口；仓库 release policy 要求更长时取更长值。
- 旧 health 生产 reader/writer caller、registry entry 和运行时 SQL 调用均为零；静态 architecture gate 连续通过。
- deterministic qualification corpus 至少覆盖 1000 个生成请求/监控事件以及 P0 定义的 fault cases；quality/circuit/capability/terminal 对账必须零未解释差异。
- 无注入故障的 qualification run 中 circuit read-model unavailable 和 workspace query error 必须为 0；注入 DB/gate 故障时必须 100% 返回 typed unavailable，且不得回退旧数据。
- 本机脱敏 soak report 明确给出实际样本数、观察起止时间和 reason counts。样本不足写 `insufficient_evidence`，不能伪造 rate，也不能用“没有报告”代替成功证据。
- release policy 已明确最低可降级版本，且无受支持版本、portable package 或 legacy import 路径仍要求旧表存在。

在兼容窗口内：

- 表可以保留，但禁止生产读写；
- 不新增同步 job，不把 V3 state 反写旧表；
- 用静态 gate 和只读诊断确认无 caller；
- 不把表存在本身视为仍可恢复旧运行时语义；
- 保留正式 read-model/gate 可观测性和迁移 shadow 指标，直到 P7 post-drop 观察完成。

#### P6.2 Pre-drop release qualification

1. 冻结待发布 source revision，重新生成 P0 caller inventory、schema/portable baseline 和 deletion ledger；任何新增 caller 都使 P6 退回对应前置阶段。
2. 在 DROP 前完成 `pnpm verify:full`、全部桌面手测、schema `15 -> latest` fixture、startup upgrade tests 和 `docs/release/SCHEMA15_UPGRADE_RECOVERY.md` 对应 release qualification。
3. 使用仓库现有 schema-upgrade backup owner 创建 verified same-device backup；校验 manifest、checksum、SQLite read-only integrity、schema/secret compatibility 和受保护密钥前置条件。备份路径和内容不得进入仓库或审计附件。
4. 在隔离的临时 data directory 完成一次恢复演练：从备份打开当前版本，验证 Proxy 路由、unsupported model、endpoint ping、station-key connectivity、Settings/timeout 和 portable capability；演练不得覆盖用户当前数据目录。
5. 记录 go/no-go 决策、release revision、备份资格、恢复结果和 rollback floor。任一证据缺失时结论只能是 no-go，不能执行 DROP。

#### P6.3 Schema 策略

1. 按相同兼容窗口分组新增 append-only migration。首组只删除已完全无 owner 的：
   - `station_key_health_observations`；
   - `routing_health_snapshot`；
   - `routing_error_rate_history` 及 meta/index。
2. `station_capacity_domains` 及 trigger/index 只有在其 portable/import/降级窗口独立结束后才进入后续 migration；不能为了减少 migration 数量与 health 表强行同批删除。
3. `routing_health_observations/routing_health_verdicts` 保持 capability-only，暂不因表名含 health 而冒险删除。
4. 若后续确需物理收敛 capability schema，必须另立计划并做 additive migration：
   - 创建专用 model capability observation/verdict 表；
   - 在单进程 startup migration transaction 中迁移并校验全部 `scope_kind='model_on_key'` 行；
   - 新 binary 只写新 owner，旧表只用于离线资格比较；不运行两个生产 writer；
   - planner 与 terminal writer 切换、重启、lifecycle invalidation 和旧库升级通过后，再在更晚版本删除混合表。
5. 每个 DROP 前通过 `sqlite_master`、row count、foreign key、trigger/index 和 portable schema fixture 断言预期对象集合。
6. 更新 compatibility metadata、postcondition、portable catalog、schema reader、fingerprint、legacy import validator、release declaration 和 differential tests；通过对应生成/验证脚本更新，不手工伪造 fingerprint。
7. migration 必须符合 `SCHEMA_UPGRADE_AUTHORING.md`：append-only、事务回滚、typed recovery、verified backup 和 postcondition；不得修改历史 migration，也不得在失败后留下半迁移数据。

**完成门**

- pre-drop qualification 全部通过且审计结论为 go。
- fresh DB、schema `15`、最高支持旧 schema、portable export/import、migration fault/rollback 和备份恢复测试通过。
- 新 schema 的 compatibility metadata、postcondition、portable catalog、fingerprint 和 release declaration 一致。
- downgrade policy 已明确；若仍要求回退到依赖旧表的版本，则 P6 不得执行。

**回滚：** DROP migration 执行后不能依赖应用代码自动降级。只允许使用 P6.2 已验证的同设备备份恢复到明确支持的 V3-read 构建，或发布前向修复 migration；默认优先前向修复。普通 default export 不包含完整 secret，不能被描述成数据库回滚方案。

---

### P7：Post-drop 验收、观察与台账收口

**目标：** 在不可逆 schema 变更后重复验证并完成一个有证据的观察窗口，再移除仅为迁移存在的 shadow/compat 代码。

**步骤**

1. 对最终 schema 重新运行 `verify:full`、schema release qualification 和桌面手测；P7 是重复验证，不替代 P6 前的首次完整验收。
2. 至少完成一个明确的 post-drop 观察窗口，记录 circuit/gate unavailable、workspace error、quality/circuit/capability 对账和 migration recovery 结果。默认不少于 7 个连续自然日；更严格的 release policy 优先。
3. 只有 post-drop 指标满足 P6.1 的量化门后，才删除 P1 shadow comparison、临时指标和仅为迁移存在的 DTO adapter；保留正式 circuit/read-model/gate 诊断。
4. 更新 deletion ledger，为每项记录 `removed`、`retained_for_decoder`、`retained_for_capability`、`deferred_schema_cleanup` 或 `blocked`，并附证据 revision。
5. 更新实施审计：实际 migration 编号、命令、退出码、fixture 数量、兼容窗口、post-drop 窗口和未完成项。
6. 重复桌面手测：
   - 正常多 Key 评分与切换；
   - 暂停/恢复 Key；
   - 连续失败打开 circuit；
   - cooldown 到期进入 Half-Open 并恢复；
   - circuit persistence gate fail-closed 与恢复；
   - 主动监控失败只影响质量，不伪造 circuit Open；
   - unsupported model 跨重启保持；
   - endpoint ping 与 Key circuit 分开显示；
   - station-key connectivity progress/cancel/result/error；
   - Settings 保存和 transport timeout 热加载；
   - 分页、窄窗口、loading、empty、error、unknown reason 和 stale data 状态。
7. 只有实现、pre/post-drop 验证和观察证据全部写入审计后，才把计划状态改为 Completed；计划本身不能代替实际证据。

---

## 5. 验收矩阵

| 场景 | Proxy 行为 | UI/read model | 数据写入 |
| --- | --- | --- | --- |
| 普通 Closed Key | 按同层 V3 score 参与 | `scored/closed`，显示 V3 revision | quality observation；成功 circuit event。 |
| `schedulable=false` | 硬资格排除 | 显示已暂停，不计入可用 | 不产生 attempt/circuit/quality。 |
| V3 Open、旧 snapshot ready | 跳过 Key | 显示 V3 Open/cooldown | 不读取旧 snapshot。 |
| V3 Closed、旧 snapshot cooldown | 正常参与 | 显示 V3 Closed；不显示旧 cooldown | 不读取旧 snapshot。 |
| V3 Open 冷却结束且 score gate 通过 | 允许竞争一次真实 Half-Open admission | 显示 recovery eligible，不把 Open 简化为永久不可用 | 只有成功 admission 后才产生 lease/event。 |
| V3 Open 冷却结束但 score gate 未通过 | 继续排除 | 显示 score gate denied | 不产生 lease/event。 |
| Half-Open lease 已占用 | 其他请求不得复用 lease | 显示 Half-Open/lease occupied | 只有 lease owner 可产生对应 terminal。 |
| Half-Open 成功/失败 | 按 V3 recovery threshold 关闭/重开 | revision/cooldown 与 store 一致 | 单一 idempotent circuit terminal event。 |
| 主动监控失败 | 不直接打开 circuit | circuit 不变，quality/reliability 可下降 | 只写 monitoring + V3 quality observation。 |
| 真实 429/502/跨边界 timeout | 按 V3 连续失败和 retry | score/circuit/trace 使用同一 failure code | V3 circuit + quality；不写旧 health。 |
| 出站前本地错误 | 不计 Key failure，不自动换 Key | 不显示 Key circuit failure | 无 Key quality/circuit failure。 |
| unsupported model | 对该模型排除当前 Key | 稳定 capability 原因 | durable model capability verdict。 |
| Key lifecycle 更新 | 旧 circuit/capability 不误伤新 Key | 只显示当前 lifecycle | revision fence 生效。 |
| endpoint ping 失败 | 不自动等同 Key circuit Open | endpoint health 单独显示 | `endpoint_health_snapshot`。 |
| circuit persistence gate active/read 失败 | Proxy 按既有 admission fail-closed | read-model unavailable 并显示 gate revision，不回退旧 health | 不产生补偿旧写。 |
| 读模型期间 gate revision 变化 | Proxy 不受 UI query 影响 | 有界重试后返回一致 snapshot 或 typed unavailable | query 无写副作用。 |
| workspace 多页候选 | 按完整候选集合规划 | full-set aggregates 与分页 rows 分开 | 不产生额外写入。 |
| station-key connectivity 取消/失败 | 不影响 Proxy circuit/quality | typed progress/terminal/result/error 与升级前一致 | 不写旧 health，不写 V3 circuit/quality。 |
| attempt/outbox/cost 任一步骤故障或重启 | 沿用既有幂等与 crash recovery | 诊断不丢失、不重复计数 | 事务边界与 P0 基线一致。 |
| active V3 policy + stale V2 row | 使用 active V3 policy | Settings/trace 不显示 V2 shadow | 不修改兼容行。 |
| 旧库升级 | 激活有效 V3 generation | read model 可用 | migration 原子、无 secret。 |

---

## 6. 数据对账与不变量

### 6.1 切读对账

对同一数据库 snapshot 比较：

- workspace candidate identity/lifecycle 集合不变；
- planner `score_status`、score、exclusion code 不变；
- circuit diagnostics 与 mutable `StationKeyCircuitStore` status、policy revision 和 `CircuitPersistenceGate` 一致；
- participation status 和 full-set aggregates 在分页前计算，所有页共享同一 revision vector；
- Key Pool 的行政、能力、倍率、endpoint ping 字段不变；
- 只有旧 health 派生字段允许发生有意变化，并必须在 comparison report 中按 reason 分类。

### 6.2 停写对账

对同一组请求/监控事件比较 P4 前后：

- `routing_observations` 插入数和幂等键相同；
- quality summary 的 real/monitor sample count、reliability、latency basis 相同；
- `routing_circuit_event_v3` 的 canonical outcome、failure code、applied、sequence 相同；
- request outcome、cost、trace、public error 不变；
- model capability observation/verdict 相同；
- attempt/outbox/quality/cost 的 transaction/commit 数、幂等结果、fault recovery 和 restart 结果相同；
- station-key connectivity progress/cancel/result/error 相同；
- 仅 `station_key_health_observations/routing_health_snapshot` 和非能力 scoped breaker rows 停止增长。

### 6.3 运行时单 owner 断言

实施后静态门应证明：

- Proxy execution 不 import 旧 health/error-rate/capacity-domain owner；
- workspace/runtime overlay/Key Pool/protection 不查询 `routing_health_snapshot`；
- frontend 不根据 raw schedulable/score/circuit 字段重新决定 participation；只展示后端 typed status/reason 和 full-set aggregates；
- request/monitor/manual writer 不修改旧 health 表；
- planner 只允许从 capability owner 读取 unsupported-model verdict；
- Settings 不读取 legacy V2 policy 行作为 active runtime facts；
- endpoint health 仍只有 endpoint probe owner 写入。
- circuit read model 读取 mutable state 与 read-only persistence gate snapshot，不读取 generation checkpoint 代替 admission state，也不因 query 推进 gate/reducer。

---

## 7. 验证门

每个阶段先运行 focused tests；P2--P5 属于跨层契约变更，必须在合并前运行 `verify:fast`，P4/P5 的 V3-only 候选必须运行 `verify:full`。P6 在 DROP 前和 DROP 后都必须运行 `verify:full`，最终 schema-drop release tree 还必须完成 `verify:release`；不能把首次完整验证推迟到 P7。Windows 环境使用 PowerShell / `pwsh` 命令。

最低命令集合：

```powershell
pnpm exec vitest run <affected-test-files>
pnpm generate:bindings
pnpm test:contracts
pnpm build

cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml <focused-test-filter>
cargo test --locked --manifest-path src-tauri/Cargo.toml --test schema15_upgrade_fixture -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml startup_upgrade -- --nocapture

pnpm verify:fast
pnpm verify:full
pnpm verify:release
```

阶段专项要求：

| 阶段 | 最低专项验证 |
| --- | --- |
| P0 | golden fixtures、事务拓扑/fault baseline、station-key connectivity contract、machine-readable inventory、schema/portable 文档对齐。 |
| P1 | mutable circuit + gate snapshot、lifecycle/policy fence、gate race、0/1/1024/1025 和分页/chunk、无写副作用。 |
| P2 | workspace/protection/Key Pool versioned DTO、full-set aggregates、registry/ACL/generated bridge、冲突数据库 fixture。 |
| P3 | backend-owned participation presentation、candidate row、分页/unknown/stale/gate diagnostics、settings timeout、Key Pool Vitest。 |
| P4 | 真实请求 429/502/timeout、transaction/fault/restart parity、monitor samples、Half-Open concurrency/late result、unsupported model restart、connectivity operation。 |
| P5 | source/compiled ACL、command/serialization/inventory parity、dead-code exceptions、V1/V2 upgrade、Settings round-trip、architecture gates。 |
| P6 pre-drop | `verify:full`、schema15/startup upgrade、verified backup/restore rehearsal、桌面手测、量化 soak report、release go/no-go。 |
| P6 post-drop | `verify:full`、`verify:release`、migration rollback、portable fingerprint、fresh/schema15/latest-old DB、differential tests。 |
| P7 | post-drop soak、重复桌面手测、shadow/adapter 收口和实现审计。 |

若检查因运行中的 `relay-pool-desktop.exe` 锁、Windows 内存限制或用户工作区现有格式差异无法完成，实施审计必须写明实际退出结果、阻断文件和未验证范围；不得以较窄测试替代后声称完整通过。

---

## 8. 删除台账

| 遗留对象 | 计划动作 | 最早阶段 | 删除前置条件 |
| --- | --- | --- | --- |
| `routing_health_snapshot` production reads | replace with mutable V3 circuit + gate read model | P2 | workspace/overlay/Key Pool/protection 全部切读且 revision vector 一致。 |
| `CircuitPersistenceGate` | retain and expose read-only snapshot | none | Proxy fail-closed owner；不得随旧 health 删除。 |
| workspace/overlay `health_revision` 和旧 health DTO 字段 | replace/remove | P2/P3 | versioned circuit/gate revision 与前端 typed consumer 完成。 |
| `station_key_health_observations` writer | remove | P4 | P2/P3 已完成，quality/circuit 对账通过。 |
| `routing_health_snapshot` writer | remove | P4 | 同上。 |
| `HealthTransitionService/HealthObservationStore` | remove | P4 | production caller 为零。 |
| manual connectivity legacy health side write | remove | P4 | operation progress/cancel/result/error golden contract 通过。 |
| legacy probe scope/health probe reducer | remove | P4 | caller 分类完成，V3 circuit lease/reaper 回归通过，历史 decoder 保留。 |
| persisted `probe_state_revision` evidence | retain decoder/version new writes | P4/P5 | 旧 fixture 可读；不能原地重命名。 |
| `AttemptCommitAck/AttemptPersistenceResult.health_applied` | remove if no behavior consumer | P4 | caller 为零且 lifecycle fault tests 已更新。 |
| non-model scoped health verdict writer | remove | P4 | failure diagnostics 已由 attempt/observation 保留。 |
| `model_on_key / unsupported_model` | retain or migrate equivalently | P4/P6 | 任何时候都不能直接删除。 |
| `list_error_rate_history` | remove | P5 | caller/registry/ACL 再次确认为零。 |
| health/operational-detail legacy IPC | remove | P5 | 生产前端 consumer 为零，registry/bindings 同步。 |
| `get_routing_protection_status` legacy adapter | remove | P5/P7 | versioned circuit/timeout consumers 已切换，兼容窗口结束。 |
| capacity-domain service/API | remove | P5 | production caller 为零。 |
| `station_capacity_domains` table | defer/drop in independent migration | P6 or later | 自身 import/portable/downgrade 窗口结束，不与 health DROP 强绑。 |
| old policy CAS | remove | P5 | V3 coordinator 是唯一 mutation owner。 |
| global Settings routing compatibility fields | remove | P5 | 所有路由编辑已走 V3 document。 |
| `RuntimeRoutingSettings.policy/scheduler_config` | remove | P5 | V3 profile/trace 直接提供等价事实。 |
| test-only legacy planner modules | remove | P5 | 有价值断言迁到 V3 tests。 |
| historical migrations/decoders/fixtures | retain | none | 只在支持矩阵另行改变时评审。 |
| `endpoint_health_snapshot` | retain | none | 独立 endpoint health owner。 |

---

## 9. 实施拆分建议

为降低 blast radius，实施变更应保持以下边界；每一项都可独立验证和回退：

1. P0 machine-readable deletion ledger、能力/事务/fault/schema 基线，不改生产。
2. P1 additive mutable circuit + gate read model，不切 consumer。
3. P2 后端 workspace/runtime overlay versioned DTO 与 full-set aggregates 切读。
4. P2 Key Pool/versioned circuit status/timeout 契约切读并生成 bindings。
5. P3 前端移除旧 fallback 和资格重算，只展示后端 typed status。
6. P4 请求终态在保持事务拓扑前提下停旧写，收敛 legacy probe/ack 字段。
7. P4 monitoring/manual connectivity 停旧写并保持 operation contract。
8. P4 verdict store capability-only 收敛。
9. P5 command/API/settings/runtime policy/capacity-domain 代码和所有契约清单同步清理。
10. 发布不含 DROP 的 V3-only 构建，完成不少于 7 日兼容窗口。
11. P6 pre-drop qualification、verified backup/restore 和独立 schema migration。
12. P7 post-drop qualification/soak 后移除 migration-only shadow/adapter。

不要把 P2--P5 和 P6 DROP migration 放进同一个不可分割变更。代码切换需要可回退，schema 删除需要单独的升级、备份和降级评审。

---

## 10. 最终交付要求

实施完成时必须交付：

- 更新后的 deletion ledger；
- 脱敏实施审计和每阶段验证结果；
- registry、源码/compiled ACL、serialization fixture、command inventories 和生成绑定的同步变更；
- fresh DB、schema 15、最高支持旧 schema、portable schema、migration fault 和 postcondition 证据；
- 每阶段 rollback floor、可安装 revision、明确的兼容窗口与最低可降级版本；
- verified backup manifest、隔离恢复演练结果和 P6 go/no-go 记录（不包含备份文件或 secret）；
- pre/post-drop 脱敏 soak report、样本数、观察窗口、阈值和未解释差异；
- 未删除对象及保留原因，特别是 capability、decoder、migration 和 endpoint health；
- 实际运行的命令、退出码、未运行项及原因。

未经用户明确要求，实施过程不 stage、commit、push、建分支或创建 PR；不得覆盖当前工作区中与本任务无关的修改。
