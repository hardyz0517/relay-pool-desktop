# 智能路由重试、故障转移与熔断控制面实施计划

状态：首版核心链路已实施，Task 8a 的 request deadline、`/models` 只读适配器与终态分类已完成；非 proxy 规划调用方现在必须显式传入 caller-owned deadline context。V2 protection profile 已接入 policy/compiler、观测写入和 Credential-scoped planner admission；durable attempt summary、有界重启后决策事件、Credential-scoped 真实 outbound Half-Open probe 编排、并发 lease-race strict refresh、按模型保护状态查询和 Provider/failure-domain 聚合诊断 read-model 已接通。本文同时作为可执行计划和事实记录，不替代当前代码与自动化契约；reducer/read-model 支持更多 health scope 不等于 proxy 已为这些 scope 提供 probe resolver。probe 只由真实用户请求触发，不自动制造 synthetic Provider 请求；完整历史步骤时间线和 raw IPC 重复键检测仍按兼容边界保留为后续扩展。

日期：2026-08-20

目标规格：[`../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md`](../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md)

关联入口：[`../README.md`](../README.md)、[`2026-08-17-routing-policy-configuration-system.md`](2026-08-17-routing-policy-configuration-system.md)、[`2026-08-13-upstream-error-classification-retry-closure.md`](2026-08-13-upstream-error-classification-retry-closure.md)

适用范围：`routing_policy` aggregate、策略 compiler、Planning Snapshot、请求执行、容量重试、健康状态投影、Tauri IPC、路由设置页和决策 trace。

不在本计划的交付要求内：发布资格、安装包验证、真实 Provider smoke、`pnpm.cmd verify:full`、`pnpm.cmd verify:release`、性能压测和跨设备/云端同步。实施中发现这些工作有必要时，另建资格记录，不把它们伪装成当前功能已完成。

> 所有行为变更先建立 RED 证据，再实现 GREEN。每个 Task 只运行与其风险相称的测试；本计划的最终验证不是发布门槛。除非用户明确要求，不 stage、commit、push 或创建分支。

### 执行方式（本轮适用）

- 以目标 spec 的 MUST/MUST NOT 为验收依据，以当前代码和测试为事实依据；计划中的 `[x]` 只表示已有代码和测试证据，不表示发布资格。
- 先写出能复现问题的最小测试或 fixture，再修改 owner；工作包完成后运行与改动直接相关的聚焦回归。只有触及跨层契约、生成绑定或构建边界时才追加 contracts、bindings、TypeScript 或 build。
- 不做发布、安装包、真实 Provider、性能压测、`verify:full`、`verify:release` 或跨设备验证。这些不属于本次完成定义。
- 任何审计发现的语义缺口必须进入任务和剩余风险，不能通过改文案、扩大 ring buffer 或增加兼容别名“标记完成”。

### 当前审计闸门（先于 Phase 2-4 完成）

1. **请求 deadline 闸门（已闭合，仍不开放为策略字段）**：proxy ingress 起点、排队、body、admission、规划 snapshot、affinity reload、route snapshot、pricing、等待、attempt 和 precommit 均在同一 request-local deadline 下执行；非 proxy 直接调用 `RoutingService::load_intelligent_planning_snapshot` 的调用方必须显式传入同一绝对 deadline context，service 不会替调用方重置预算。该 context 传播已有过期 caller fixture；request deadline 仍属于 transport/runtime owner，不开放为用户策略字段。
2. **models 聚合闸门（已闭合）**：`execute_models` 使用 typed `ModelsRetryAdapter` 作为只读聚合契约，明确 action reason、候选继续/停止边界和 planning timeout 行为，并有 focused/loopback 证据；不得回退为普通 inference retry。
3. **终态分类闸门（当前范围已接通）**：deadline/budget 抑制的 replay-safe failure 已按 canonical request-acceptance 分类；后续回归必须保持 `FailedBeforeCommit`/`PossiblyAccepted`/`Committed` 区分。完整 deadline 覆盖仍受第 1 项约束。
4. **公开文档严格性闸门（已闭合，重复键采用兼容边界）**：V2 public document 不得依赖 storage compatibility defaults；缺少基础字段、嵌套 retryFailover 字段、unknown/future/非法值必须拒绝。raw/file decoder 拒绝重复键；IPC `Value` 接收已解析对象，不承诺原文重复键检测。V1 兼容默认只能存在于 migration/storage decoder。
5. **控制面可见性闸门（已闭合）**：设置页显示真实 timeout 数值和 owner；bounded error-rate history 已有 typed API/query/UI 消费，且默认关闭。
6. **迁移物化闸门（已闭合）**：0050 additive rewrite/history fixture 已证明有效 V1 active/history row 物化为 V2；0052 补齐旧代码生成的 V2 缺失 `protectionProfile`；错误类型 row 仍交由 typed recovery，不能静默覆盖。
7. **权限与生成物闸门（已闭合）**：源码 registry、`main-window` 权限源和编译 ACL 已同步包含当前 routing read/write commands；后续 DTO、registry 或权限变更仍必须走仓库生成流程，不能只改生成 JSON。
8. **重复键边界闸门（兼容方案已明确）**：raw/file document decoder 拒绝重复键；Tauri `Value` command 边界接收的是已解析对象，无法检测原始 JSON 重复键。当前契约明确只对 raw/file 入口承诺 duplicate-key reject，IPC 不伪造该能力；若未来需要 IPC 原文保证，另建 raw JSON DTO 迁移任务。
9. **兼容投影闸门**：`TryNextCandidate` 仍可出现在 lifecycle/terminal projection；它不是 planner。必须在台账和 trace 契约中标明 `RetryAction` 是唯一动作 owner，UI/trace 不得把该投影反推为实际动作。
10. **Closed/Degraded 语义闸门**：reducer 内部 `Closed` 表示未打开保护但仍在观测；首版保留 projector 的 `Closed -> Degraded` 映射，但必须显示“当前未打开保护、仍在监控”的解释，`NoProtection` 仅用于没有观测条目。禁止前端自行把两者当作同义词；未来新增 `Monitoring` 必须升级 read-model 版本。
11. **Probe scope 闸门（已收窄）**：生产 proxy outbound 的 Half-Open lease acquisition、discovery、candidate matching 和 cancellation 当前只支持 `Credential` scope。`HealthProtectionScopeKind`、reducer 和 status projector 中存在的 `Account`、`Group`、`Endpoint`、`Model` 等类型不能作为已完成 probe 编排的证据；扩展到这些 scope 前必须新增统一 scope resolver、同源 commitment、跨层执行测试和恢复/取消测试。

## 1. 交付边界与完成定义

本升级分为两类工作：

1. **Phase 0-1 是首个可实施交付单元。** 它收敛已有 owner，开放四个已经有生产 envelope 的容量重试字段，并让用户看到真实的执行原因和保护状态。
2. **Phase 2-4 已完成当前范围的聚焦闭合。** 错误率保护默认关闭；启用后由 V2 policy 统一控制阈值、窗口和 Half-Open 成功次数，并在设置页走同一 CAS。Credential-scoped Half-Open probe 已进入真实 outbound 编排，并覆盖 lease 竞争后的 strict snapshot refresh；Provider/failure-domain 聚合诊断支持显式 requested model。其他 health scope 的 probe resolver、自动 synthetic probe、完整历史步骤时间线仍是后续增强范围。

Phase 0-1 完成时，以下链路必须成立：

```text
Routing policy V2 document / routing settings draft
  -> RoutingPolicyService validate + compile + CAS
  -> immutable AttemptBudgetProfileV1 + policy revision
  -> PlanningSnapshot / RouteAdmissionCoordinator / ExecutionEngine
  -> RetryActionPlanner + ReplayGate + CapacityRetryRegistry
  -> bounded DecisionTrace + durable terminal summary
  -> ProtectionStatus projector + generated IPC + routing UI
```

首个交付单元的产品边界固定如下：

- 可编辑：`maxTotalAttempts`、`maxSameTargetCapacityRetries`、`capacityRetryWaitBudgetMs`、`allowCrossCapacityDomainFallback`。
- 只读：当前服务器级 timeout、容量保护运行时状态、durable health verdict、trace 可用性和 terminal summary。
- 不开放：通用跨故障域开关、通用等待预算、真正 request deadline、timeout 编辑、保护 preset、任意自定义错误规则和自动 synthetic probe。
- 所有设置只影响保存后创建的请求快照；进行中的请求继续使用启动时取得的不可变策略 revision。

## 2. 不可变的工程决定

| 决定 | 实施规则 | 禁止的偏移 |
| --- | --- | --- |
| 尝试预算 | `AttemptBudgetProfileV1` 是唯一编译产物，注入 admission、execution、capacity retry 与 trace。`maxTotalAttempts` 包含第一次出站发送。 | 在任意 consumer 再留一个可独立修改的 `4`，或把 replan 视为新请求重置预算。 |
| 动作语义 | 以 typed `RetryAction` 替换 `RetryDecision`；execution 只执行 planner 的动作。 | 让 execution 重新按 HTTP status、`RetryClass` 或 UI 字段猜测动作，或保留平行 planner。 |
| 重放安全 | `FailureClassifier` 和 `ReplayGate` 继续是唯一 owner；策略只能收紧预算，不能授权危险重放。 | 设置重试认证错误、请求错误、已 committed 响应或非幂等 `Unknown` 请求。 |
| 配置版本 | `formatVersion` 维持 document envelope 版本；`policy.version` 从 1 升到 2。v1 只作为升级输入，当前运行时只消费 V2 编译产物。 | 在 `RoutingPolicyConfigV1` 追加 serde default 并声称完成 V2，或在 execution 保留 V1 行为分支。 |
| 状态解释 | `ProtectionStatus` 是 UI 唯一 read model，明确标出 `durable`、`runtime_capacity` 和不可用。 | 前端拼接 legacy health snapshot、scoped verdict、capacity registry，或把重启即丢失的状态显示为持久熔断。 |
| 公开契约 | document DTO 严格拒绝 unknown field、重复键、future version、错误类型和越界值；TypeScript 类型由生成绑定产生。 | 手写平行 string union、宽松 JSON、重复键最后值获胜或部分保存。 |
| trace | trace 是有界、无敏感信息的执行证据；重启/驱逐后显示 durable terminal/attempt summary 和有序 lifecycle event summary，不能恢复完整 runtime 细节。 | 用日志或扩大内存 ring 假装提供完整历史时间线，或记录请求正文、凭据、完整 URL/响应体。 |

## 3. 实施顺序与切换规则

```text
0 基线与 RED fixture
  -> 1 尝试预算和 V2 policy domain
  -> 2 RetryAction 替换与容量执行收敛
  -> 3 严格文档、迁移、CAS 与 snapshot 注入
  -> 4 ProtectionStatus 与解释 DTO
  -> 5 IPC / generated binding / query-draft-conflict 基础
  -> 6 路由设置页和端到端回归
  -> 7 Phase 2: 通用 failover + transport policy
    -> 7a ingress deadline / models aggregation / terminal classification 闸门
  -> 8 Phase 3: 统一健康保护状态机
  -> 9 Phase 4: 错误率保护与历史诊断（backend foundation；UI 另有门槛）
```

- Task 1-3 是同一后端切换单元：中途可以有开发中的代码，但不能让 UI 写入新字段，直到所有 consumer 使用同一 profile。
- Task 4-6 是同一控制面切换单元：前端只能读取 generated DTO，不能在旧局部 `useState` 保存逻辑上加四个字段。
- `RoutingPolicyService`、现有 document coordinator 和 SQLite CAS 仍是策略写入入口。若关联的策略配置系统尚未完成其 shared query、typed conflict 或严格公共 decoder，先补齐最小前置，再接入本计划；不得复制第二套 service 或协调器。
- 迁移、恢复、UI 保存和未来文件导入都走同一 V1-to-V2 upgrader 和 compiler。没有 consumer 的字段不能持久化。

### 当前批次状态（2026-08-21 审计后）

| 工作包 | 当前结论 | 仍需完成的门槛 |
| --- | --- | --- |
| Task 0 基线与 owner 台账 | 文档、owner 台账和默认行为聚焦回归已完成 | 默认值/安全门的黑盒矩阵属于增强性回归，不阻断本轮交付 |
| Task 1-3 V2/profile/action | V2、严格 public decoder、0050/0052 物化、统一 profile 和 typed action 已接通 | 四字段端到端黑盒场景属于增强性回归 |
| Task 4-5 read model/IPC/draft | ProtectionStatus、structured trace、generated binding、shared draft/CAS 和当前 routing ACL 已接通 | IPC 使用已解析 `Value`，重复键保证限定于 raw/file decoder；不得声称 IPC 能检测原始重复键 |
| Task 6 设置页 | 四个容量字段、真实 timeout/owner、summary-only trace 和保护状态可读 | UI field error/CAS/外部变更/恢复/键盘/窄窗口矩阵已完成 |
| Task 8 Phase 2 | capacity replan/cross-domain、transport policy、ingress deadline anchor、规划前置 I/O deadline、typed `/models` 只读适配器、终态分类和非 proxy caller-owned deadline context 已接通 | 通用 timeout 编辑和全局 deadline 字段仍不开放 |
| Task 9 Phase 3 | durable reducer、恢复、Credential-scoped Half-Open、显式 probe lease、真实 outbound 编排、lease-race strict refresh、投影、runtime capacity read source、profile reconfigure 和 failure-domain identity read-model 已接通 | Account/group/endpoint/model probe resolver 尚未接通；不自动制造 synthetic probe；完整历史步骤时间线仍不开放 |
| Task 10 Phase 4 | observation/reducer/history backend、V2 protection profile、frontend API/query/UI、Credential scope commitment admission bridge、durable attempt summary、重启后有界 lifecycle event summary、按模型保护状态查询和 Provider/failure-domain 聚合诊断已接通，默认关闭 | 其他 scope 的 probe 编排、完整历史步骤时间线仍不开放；raw IPC 重复键检测仍不承诺 |

本表是本次计划的执行起点。未完成门槛不能通过勾选父任务或修改状态文字绕过；每个门槛必须有对应代码 owner、回归测试和不泄漏敏感信息的契约证据。

## 4. 目标模块边界

| 路径 | 最终职责 | 本计划中的变更 |
| --- | --- | --- |
| `src-tauri/src/models/routing_policy.rs` | 版本化领域策略、默认值、字段验证 | 保留 V1 upgrade input；新增 V2 与 `RetryFailoverPolicyV1`，不含 SQL、文件或 proxy 状态。 |
| `src-tauri/src/application/routing_policy.rs` | aggregate decode、compile 和 legacy mapping | 编译 `AttemptBudgetProfileV1`，让 compiled policy 带 revision 与 profile。 |
| `src-tauri/src/application/routing_engine/admission.rs` | request-local candidate admission 与 attempt budget | 接受已编译 profile，删除本地总尝试硬编码。 |
| `src-tauri/src/services/proxy/execution.rs` | outbound attempt、canonical failure 和 replay 边界 | 以 `RetryActionPlanner` 替换 `RetryDecision`，不自建规则。 |
| `src-tauri/src/services/proxy/routing_runtime.rs` | 容量 retry registry、Open/Half-Open 和等待 | 保留容量状态机；由 profile 接收同目标次数和等待预算，删除无消费者 `max_upstream_attempts`。 |
| `src-tauri/src/application/routing_engine/runtime_metrics.rs` | 有界 decision trace/metrics | 使用同一 attempt 上限，并记录 effective policy revision、action 和 budget。 |
| `src-tauri/src/application/operational_facts/` 与 `src-tauri/src/application/queries/routing_workspace.rs` | planning/protection read model | 将 compiled profile 带入 snapshot；提供单一 `ProtectionStatus` projector。 |
| `src-tauri/src/ipc/dto/routing_mutations.rs`、`routing_health_reads.rs` | public request/response DTO | V2 policy 与 protection/trace DTO；字段转换不泄漏内部 state。 |
| `src-tauri/src/application/command_facades/routing.rs`、`src-tauri/src/commands/`、`src-tauri/src/ipc/registry.rs` | command facade、权限和 registry | 扩展已有 load/apply/trace/health 命令，随后运行 binding generator。 |
| `src/lib/api/routing.ts`、`src/lib/types/routing.ts`、`src/lib/queries/` | 生成类型的前端 API/query owner | 去除手写 V1 假设，建立 policy/protection/trace query key。 |
| `src/features/routing/LocalRoutingSettingsEditor.tsx` | 路由设置的 UI composition | 改为 shared query + draft reducer + typed conflict，不在组件自行持久化服务端状态。 |

## 5. Task 0：冻结基线、调用图和最小 RED 证据

**目标：** 固化升级前默认行为，确认现有策略配置控制面是否已满足本计划的依赖；不改变生产逻辑。

**文件**

- Create: `docs/audits/2026-08-20-routing-retry-failover-baseline.md`
- Create: `docs/audits/routing-retry-failover-deletion-ledger.md`
- Create/update: 现有 routing loopback、execution、admission 和 routing policy focused tests

**步骤**

- [x] 记录当前 schema 最大编号、`routing_policy` aggregate revision 与 history 写入链、现有 document decoder、CAS 命令、生成 binding 入口和 UI query owner（见 baseline audit）。
- [x] 在 deletion ledger 列出硬编码尝试上限、`RetryDecision`、`max_upstream_attempts`、legacy health snapshot 和页面局部保存入口，并标记 replace/remove/retain 及完成条件。
- [x] （增强回归）补齐默认值黑盒 fixture：`4` 次总 attempt、`2` 次同目标容量 retry、`2000 ms` 总等待、允许一次跨 capacity domain fallback；通过真实 loopback outbound count、action、trace 和 terminal result 断言。
- [x] （增强回归）补齐 fail-closed fixture：request/auth invalid、committed、`ResponseStarted`、非幂等 `Unknown`、cancel、deadline exhausted；并单独断言 replay-safe deadline stop 不会被标成 `PossiblyAccepted`。核心安全门已有 focused execution/failure tests。
- [x] 已确认 routing-policy control plane 有 revision/CAS、shared query/draft/conflict；legacy direct-update facade、command、DTO 和 binding 已退役并由删除台账记录。

**完成条件：** 基线 fixture 全绿；删除台账没有“以后再看”的 production owner；后续 Task 使用这些 fixture 证明默认行为未漂移。

**最小验证**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib precommit_chat_capacity_event_enters_same_target_retry_path
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib cooldown_allows_only_one_half_open_probe_and_reopens_on_failure
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture
```

## 6. Task 1：策略 V2、AttemptBudgetProfileV1 与严格字段约束

**目标：** 先让四个字段成为一个合法、可编译、可迁移的领域能力，尚不暴露 UI，也不改变默认执行。

**文件**

- Modify: `src-tauri/src/models/routing_policy.rs`
- Modify: `src-tauri/src/application/routing_policy.rs`
- Modify: `src-tauri/src/persistence/stores/routing_policy_store.rs`
- Modify: schema upgrade 目录中的下一可用 migration，以及对应 migration/restore tests
- Create/update: routing policy model/compiler/document tests

**步骤**

- [x] 定义 `RetryFailoverPolicyV1`：四字段、默认值、范围和字段级 validation error。
- [x] 定义显式 `RoutingPolicyConfigV2`，将 `retryFailover` 嵌入 policy；active aggregate/compiler 使用 V2，V1 仅作 storage/migration input。
- [x] compiler 生成不可变 `AttemptBudgetProfileV1`，并校验平台 hard cap。
- [x] 实现 V1-to-V2 additive upgrader，并让 restore/storage 入口统一经过 canonical decoder。
- [x] 收紧 public V2 DTO：storage compatibility defaults 与 public document decoder 已分离；缺失基础字段、retryFailover 字段、unknown/future/非法值均拒绝。重复键由 raw/document decoder 在进入 DTO 前拒绝；已解析的 IPC `Value` 不伪造重复键检测能力。
- [x] 非法字段返回稳定 field ID/error code/message key；CAS/history/document mirror 不产生部分写入的后端路径已有聚焦证据。

**完成条件：** 新 policy 的默认 compiler 输出与 Task 0 基线相同；旧 policy 可升级并恢复；未知/重复/非法输入 fail-closed 且保留旧 active revision。

**最小验证**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml routing_policy -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_startup_cutover -- --nocapture
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

## 7. Task 2：将二值 RetryDecision 替换为 RetryAction

**目标：** 保留现有 classifier 和 replay gate，恢复 canonical intent 的动作语义；默认容量路径行为保持不变。

**文件**

- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/services/proxy/request_send.rs`（仅当 request-send 边界需要暴露 typed evidence）
- Modify: `src-tauri/src/application/routing_engine/admission.rs`
- Modify: `src-tauri/src/application/routing_engine/runtime_metrics.rs`
- Create/update: execution/action/replay focused tests

**步骤**

- [x] `RetryDecision` 已替换为 typed `RetryAction`，包含 reason、failure、replay、attempt、revision、剩余预算和等待证据。
- [x] `RetryActionPlanner` 使用 immutable request facts/canonical failure/replay evidence/profile，不读取 UI、SQL 或 mutable global state。
- [x] 容量 `RetrySameTarget`、`WaitThenReplan`、`TryDifferentFailureDomain` 已有执行分支和聚焦测试。
- [x] 收紧所有 execution consumer 只执行 planner action；`execute_models` 明确为只读聚合协议，Stop 不再静默推进到下一个 candidate，并有单测。
- [x] 统一 trace/terminal summary：deadline early-return、stream post-commit 和 runtime trace 缺失分别表达，不伪造完整时间线。
- [x] 生产代码已删除 `RetryDecision`、`max_upstream_attempts` 和独立总尝试 owner；legacy 兼容残留在 deletion ledger 中保留原因。

**完成条件：** 单测可以区分同目标重试、受 replay/deadline/budget 抑制的 retry、换故障域意图和最终停止；非幂等 `Unknown`、已 committed 和输出已开始的请求不重放。

**最小验证**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib replay_gate_consumes_every_transport_boundary_and_fails_closed
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib execution -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_failure_contract -- --nocapture
```

## 8. Task 3：将同一编译 profile 注入所有容量路径

**目标：** 让一次请求的总尝试、同域次数、等待与跨域 fallback 只由一个 compiled profile 决定。

**文件**

- Modify: `src-tauri/src/application/operational_facts/planning_snapshot.rs`
- Modify: `src-tauri/src/application/routing_engine/planning_snapshot.rs`
- Modify: `src-tauri/src/application/routing_engine/coordinator.rs`
- Modify: `src-tauri/src/application/routing_engine/admission.rs`
- Modify: `src-tauri/src/services/proxy/routing_runtime.rs`
- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/application/routing_engine/runtime_metrics.rs`
- Create/update: admission/capacity/trace integration tests

**步骤**

- [x] Planning Snapshot、admission、execution、capacity 和 trace 已接收同一 compiled `AttemptBudgetProfileV1`/revision。
- [x] capacity profile 已收敛为内部状态机参数，request-specific retry/wait/cross-domain 值由 profile 输入。
- [x] `max_upstream_attempts`、独立总尝试常量和 production `RetryDecision` 已通过 source-absence 检查清理。
- [x] 首次发送、同目标 retry、跨 capacity-domain outbound 分支共用 request-local attempt budget；replan 不重置预算。
- [x] 等待按 retry-after、capacity wait、precommit 剩余预算裁剪，并保留取消/确定性 jitter/同域 sibling 防绕过语义。

**完成条件：** 同一 profile 能从 trace 关联到 admission、capacity 和 execution；总 attempt 永不超过 4；同域、等待和跨域预算不因 replan 复位。

**最小验证**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib capacity -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib admission -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

## 9. Task 4：ProtectionStatus、决策解释与只读运行时事实

**目标：** 让用户理解当前保护状态和本次请求为何停止/等待/切换，但不把现有机制误称为统一熔断器。

**文件**

- Modify: `src-tauri/src/application/queries/routing_workspace.rs`
- Modify: `src-tauri/src/application/operational_facts/candidate_projection.rs`
- Modify: `src-tauri/src/commands/routing_health.rs`
- Modify: `src-tauri/src/ipc/dto/routing_health_reads.rs`
- Modify: `src-tauri/src/application/routing_engine/runtime_metrics.rs`
- Create/update: protection projection and trace read tests

**步骤**

- [x] `ProtectionStatus`/`persistenceKind` 已区分非敏感 scope、durable verdict、runtime capacity、冷却和 explicit unavailable。
- [x] projector 已集中 durable verdict、legacy compatibility 和 runtime capacity 读模型；capacity runtime 重启为空时不会伪装成持久熔断。
- [x] 为 decision trace 补齐结构化 `detailAvailability`、explanation key、action、attempt、remaining budget、policy revision；runtime trace 缺失时只能返回 terminal summary。
- [x] 设置页渲染 `ProxyServerLimits` 五项真实毫秒值和 owner；只读，不写入 V2 policy。
- [x] 已有低基数 action/failure、budget/deadline suppression、capacity fallback、trace truncate/persist failure 统计；继续核对 label 不含 endpoint、request ID、账号或凭据。
- [x] `RuntimeOutlierPolicyV1` 保持 test-only，不进入 projector、IPC、UI 或 policy document。

**完成条件：** 页面/API 可区分 durable 冷却、运行时容量 Open/Half-Open、无保护和 summary-only；状态来源不含秘密，且不承诺跨重启的容量熔断。

**最小验证**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml routing_health -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml decision_trace -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib cooldown_allows_only_one_half_open_probe_and_reopens_on_failure
```

## 10. Task 5：IPC、生成绑定、query/draft/conflict 控制面

**目标：** 所有前端字段和运行时 read model 都通过同一策略 service、严格 DTO 和 revision/CAS 流程访问。

**文件**

- Modify: `src-tauri/src/ipc/dto/routing_mutations.rs`
- Modify: `src-tauri/src/ipc/dto/routing_health_reads.rs`
- Modify: `src-tauri/src/application/command_facades/routing.rs`
- Modify: `src-tauri/src/commands/` 中现有 routing policy/health command
- Modify: `src-tauri/src/ipc/registry.rs`、`src-tauri/permissions/main-window.toml`、ACL manifest source
- Generate: `src-tauri/generated/command-registry.json`、`src-tauri/gen/schemas/acl-manifests.json`、`src/lib/bridge/generated.ts`（只能通过仓库生成流程）
- Modify: `src/lib/bridge/BackendClient.ts`、`DesktopBackend.ts`、`DemoBackend.ts`、`src/lib/api/routing.ts`、`src/lib/types/routing.ts`
- Create/update: routing query/draft/conflict tests

**步骤**

- [x] routing policy load/apply、protection status 和 generated DTO 已通过 V2/domain path 接通；未新增绕过 document/CAS 的 direct update command。
- [x] apply 使用 `document.baseRevision` CAS，并返回 typed validation/conflict/unavailable；DTO 不携带原始 document、凭据、请求/响应正文或完整 endpoint。
- [x] bindings、DemoBackend、DesktopBackend、API/query 类型已同步并有契约测试。
- [x] 旧 `update_routing_policy` command、DTO、facade、binding 和前端 adapter 已从源码/registry/前端生成类型删除；保存路径唯一保留完整 document apply/CAS。
- [x] 从 `src-tauri/permissions/main-window.toml` 删除旧 `update_routing_policy` command；当前源权限已改为 `apply_routing_policy_document`。
- [x] 将当前 registry 中的 `get_routing_protection_status`、`list_error_rate_history` 等 routing read commands 补入 `main-window` 权限源，并通过 Cargo/Tauri 生成流程更新 `src-tauri/gen/schemas/acl-manifests.json`；生成后的 ACL、registry 和 command source 已做一致性检查。
- [x] 为 public apply 入口保留已解析 `Value` 兼容边界；规格和测试明确重复键只对 raw/file document 入口保证，不能声称 IPC 能检测原始重复键。改为 raw JSON string 需另建兼容迁移任务。
- [x] 在 `docs/audits/routing-retry-failover-deletion-ledger.md` 登记 `TryNextCandidate` 仅为 lifecycle/terminal compatibility projection；`RetryAction` 保持唯一 planner/action owner，UI 和 trace 不得从二值投影推断实际动作。
- [x] shared draft 已接入，`retryFailover` 四个子字段按三方 dirty merge；冲突时不会覆盖远端未冲突字段。
- [x] `showDecisionExplanation` 独立保存在应用显示设置，不进入 routing policy revision，也不影响 backend 选择。

**完成条件：** 无论 UI、受管 JSON、恢复还是未来调用方，V2 写入都走同一 validation/compiler/CAS；前端没有手写 policy/action/status union，也不会静默覆盖 dirty draft。

**最小验证**

```powershell
pnpm.cmd generate:bindings
pnpm.cmd test:contracts
pnpm.cmd test -- src/lib/api/routing.test.ts src/lib/queries/routingQueries.test.ts
cargo test --locked --manifest-path src-tauri/Cargo.toml routing_policy -- --nocapture
```

## 10a. Task 5a：权限收口与 public document 解码边界

**目标：** 关闭审计发现的契约残留，使 command registry、窗口 ACL 和生成绑定只声明当前命令；同时让“重复 JSON key 必须拒绝”的保证有明确入口和可验证范围。

**执行顺序：** 先处理权限源和生成物，再决定 IPC document 输入形状；不要通过手工修改生成 JSON 或在 `Value` 上增加无法证明原始文本语义的测试来假装闭合。

**文件**

- Modify: `src-tauri/permissions/main-window.toml`
- Generate: `src-tauri/gen/schemas/acl-manifests.json`（使用仓库现有 Tauri 构建/生成流程）
- If raw IPC is selected: `src-tauri/src/commands/routing_health.rs`、`src-tauri/src/ipc/dto/routing_mutations.rs`、对应 generated TypeScript/API、contract fixtures
- Modify: `docs/audits/routing-retry-failover-deletion-ledger.md`

**步骤**

1. 确认 `update_routing_policy` 已从 `main-window.toml`、`src-tauri/src`、`src/lib` 和 `src-tauri/generated/command-registry.json` 消失；当前源码权限应声明 `apply_routing_policy_document`。
2. 将 registry 中当前 routing read commands（至少 `get_routing_protection_status`、`list_error_rate_history`）加入权限源，运行仓库已有的 Tauri ACL 生成流程更新 `src-tauri/gen/schemas/acl-manifests.json`；随后用 command/ACL 一致性检查确认 allow-list、registry、Rust command source 三方一致。
3. 选择一种重复键契约并写入代码注释、DTO 文档和测试：
   - **推荐：** IPC mutation 接收 `documentJson: string`（或等价 raw bytes DTO），由统一 `decode_strict_json::<RoutingPolicyDocumentV2>` 解析，再进入 validation/compiler/CAS；结构化 UI 先序列化为 canonical JSON，文件/导入保持原始文本路径。
   - **兼容方案：** 保留 `serde_json::Value` IPC 输入，但把重复键保证严格限定为 raw/file document decoder；完成定义、API 文档和测试不得声称已解析 `Value` 能检测原始重复键。
4. 为选定方案补最小契约测试：缺字段、unknown field、非法值、future version、重复 key、CAS conflict；所有失败都保留旧 active revision。
5. 更新删除台账，明确 `RetryAction` 是唯一 planner/action owner；`TryNextCandidate` 仅是 request lifecycle/terminal compatibility projection，不能作为 UI 或 trace 的实际动作来源。

**完成条件：** 不存在 stale ACL；生成物可由仓库流程重建且无漂移；重复键语义有真实 raw-input 证据或明确的边界声明；兼容投影不再被描述为第二个 retry planner。

**最小验证**

```powershell
pnpm.cmd test:contracts
pnpm.cmd generate:bindings --check
pnpm.cmd architecture:security
cargo check --locked --manifest-path src-tauri/Cargo.toml
git diff --check
```

## 11. Task 6：路由设置页与请求详情

**目标：** 提供紧凑、可解释且不夸大当前能力的用户界面。

**文件**

- Modify: `src/features/routing/LocalRoutingSettingsEditor.tsx`
- Modify/create: `src/features/routing/useRoutingPolicyDraft.ts`
- Modify/create: `src/features/routing/RoutingPolicyConflictDialog.tsx`
- Modify: `src/features/routing/LocalRoutingSettingsEditor.test.tsx`
- Modify: `src/features/routing/LocalRoutingEditTab.tsx` 与其测试
- Modify/create: request detail / routing workspace trace and protection components/tests

**步骤**

- [x] `LocalRoutingSettingsEditor` 已改为 shared query/draft/CAS owner；组件不直接持有 load/save/revision 职责。
- [x] “重试与切换”四字段已可编辑，展示范围、默认值、容量故障语义、policy revision 和只影响后续请求。
- [x] 前端不 silent clamp；数字控件有稳定宽度，窄窗口使用单列布局。
- [x] 超时分组渲染 `ProtectionStatus.timeouts` 的实际值和 owner；故障保护的 durable/runtime/unavailable 展示已接通。
- [x] 请求详情消费结构化时间线，展示 explanation key、技术字段和 summary-only/loading/empty/error 状态。
- [x] （增强回归）补齐 field error、CAS conflict、外部变更、invalid document、unavailable、restore default、disabled action、keyboard/focus 和窄窗口布局的 UI 回归矩阵。

**完成条件：** 用户可以明确看出哪些行为能设置、哪些仅为运行时事实、每个失败为什么停止/重试/换域，以及详细 trace 何时不可用。

**最小验证**

```powershell
pnpm.cmd test -- src/features/routing/LocalRoutingSettingsEditor.test.tsx
pnpm.cmd test -- src/features/routing/LocalRoutingEditTab.test.tsx
pnpm.cmd test -- src/lib/api/routing.test.ts
pnpm.cmd build
```

## 12. Task 7：Phase 0-1 集成回归、迁移与清理

**目标：** 证明默认升级等价、配置真正影响后续请求，并删除已替换的 owner。

**文件**

- Modify: Task 0 deletion ledger / baseline audit
- Modify: routing loopback, persistence migration, document codec, IPC contract and UI focused tests
- Modify/delete: 已无 consumer 的 `RetryDecision`、`max_upstream_attempts` 和遗留 default/adapter
- Modify: target spec/README 状态（仅在实现和证据完整后）

**步骤**

- [x] 用 V1 fixture 启动/升级，验证 active/history policy 的持久化形状为 V2、revision 保持不变；0050 additive rewrite 已有 migration fixture。另用旧代码生成的 V2 缺失 profile fixture 验证 0052 只补默认 profile、不改变 revision、不覆盖显式 profile。旧请求 snapshot 与新 revision 的跨请求行为仍保留在 loopback/运行时回归中。
- [x] （增强回归）对四个字段分别建立黑盒场景：总 attempt、同目标上限、等待总预算、关闭跨 capacity-domain fallback；每个场景断言 outbound count、terminal reason 和 trace effective value。容量 fixture 使用可识别的 `ProviderCapacity` envelope，未提供可信 capacity domain 时仍 fail-closed。
- [x] capacity 同域 sibling key 不绕过同目标/同域限制；Half-Open 并发、取消释放和进程重启基础语义已有聚焦回归，后续只补缺失的边界 fixture。
- [x] source-absence check 已证明 production 仅保留一个 attempt budget/action/planner owner，`RuntimeOutlierPolicyV1` 未进入生产 read path；兼容残留已写入 deletion ledger。
- [x] ACL source/compiled manifest 不再声明 `update_routing_policy`，当前 routing commands 均已声明，并有命令注册/权限一致性测试证据。
- [x] public apply 的重复键语义按兼容方案落地并写入 spec：raw/file 入口有证据，已解析 `Value` 不伪造原始重复键检测。
- [x] 删除台账已说明 `TryNextCandidate` 的兼容投影边界，trace/UI 使用 typed `RetryAction` 解释实际动作。
- [x] Task 8a 的 proxy 规划 I/O deadline、`/models` typed adapter、终态证据和非 proxy caller-owned deadline propagation 已闭合；error-rate reducer 到 planner 的 typed admission bridge 已闭合且由默认关闭的 V2 `protectionProfile` 控制；可配置窗口/阈值已通过 policy/CAS/migration/reducer/UI 接入。真实 outbound probe lease、lease-race strict refresh、按模型保护状态查询和基础 Provider/failure-domain 聚合诊断已在 Task 9/10 收口；synthetic probe 和完整历史步骤仍保留为后续门槛，不能因此把安全边界放进普通设置字段。

**完成条件：** Phase 0-1 没有双 budget、二值 action、无消费者字段或前端直写路径；四个字段均可追溯到 compiled profile、执行 trace 和终态。

**最小验证**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_policy -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib execution -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_health -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture --test-threads=1
pnpm.cmd test -- src/features/routing/LocalRoutingSettingsEditor.test.tsx src/lib/api/routing.test.ts
pnpm.cmd build
pnpm.cmd test:contracts
git diff --check
```

## 13. Task 8：Phase 2，通用故障转移与传输策略

**启动前置条件：** Task 1-7 已完成；`RetryAction` 已真实保留容量/等待/换域语义；每个 action 都有 replay、deadline 和 failure-domain fixture。未满足时不在设置页添加通用 fallback 字段。

**文件**

- Modify: `src-tauri/src/services/proxy/execution.rs`、`request_send.rs`、routing engine planning/admission modules
- Create/modify: versioned `TransportExecutionPolicy` domain/compiler/IPC/测试模块
- Modify: proxy client/runtime composition、trace DTO/UI

**步骤**

- [x] 为 `WaitThenReplan` 和 `TryDifferentFailureDomain` 实现明确执行分支：保留 excluded domain 集合、等待建议、action reason 与剩余预算；每次 replan 使用最新 snapshot，但不重置 request-local budget。
- [x] 定义独立 versioned `TransportExecutionPolicy`，分别拥有 connect、first-byte、buffered、stream idle 和 request deadline 字段；timeout 仍按 runtime/server limits 编译，不开放编辑。
- [x] 将 request deadline 起点前移到 ingress，并覆盖排队、body/metadata、lifecycle admission、规划 I/O、等待、attempt 和 precommit 输出前阶段；stream 已提交后仍只终止/记录、不自动重放。
- [x] capacity-only 的 `allowCrossCapacityDomainFallback` 继续由 V2 profile 控制；通用 `allowCrossFailureDomainFallback` 尚未进入 active policy，避免改变 capacity-only 字段语义。
- [x] 当前 UI/契约将完整历史步骤限定为 runtime-only；重启后提供 summary-only 的 durable attempt 与有界 lifecycle event projection。stream post-commit/idle 错误只保证 durable lifecycle，不扩大 runtime ring 冒充完整历史；可恢复摘要中的未持久化字段必须明确显示为 unavailable。

**最小验证**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_failure_contract -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture
pnpm.cmd test -- src/features/routing
pnpm.cmd build
```

## 13a. Task 8a：收口真实 deadline、models 聚合与终态证据

状态：已完成 proxy execution 与非 proxy caller-owned context 传播范围。已接入 ingress deadline anchor、规划前置 I/O 剩余 deadline 包装、replay/terminal classification 和 typed 只读 models 路径，并完成 focused/loopback 证据。该 deadline 仍由 transport/runtime owner 管理，不作为用户可编辑策略。

**目标：** 逐项关闭 Phase 2 审计发现的三个语义缺口；在所有闸门闭合前，不得把 `request_deadline` 描述为覆盖完整请求生命周期，也不得宣称所有 execution consumer 都只执行 `RetryAction`。

**实现顺序**

1. 在 ingress/请求上下文创建单调时钟起点和不可变 deadline context，并传入 `CanonicalProxyRequest`；ExecutionEngine 不再用新的 `Instant::now()` 重置预算。
2. 对 semaphore、body/metadata、lifecycle admission、settings/model-mapping/snapshot await 和 replan wait 统一使用剩余 deadline；超时返回稳定 failure/explanation key。
3. 给 deadline early-return 统一 terminal recorder；按 canonical `RequestAcceptance` 映射 `FailedBeforeCommit`、`PossiblyAccepted`、`Committed`，禁止把 replay-safe rejected-before-acceptance 误记为 `PossiblyAccepted`。
4. 明确 `execute_models` 的契约：若它是只读模型列表聚合器，定义独立的 GET 聚合 action policy、记录“不进行普通重放”的原因并补测试；若复用普通 execution，则必须逐个执行 `RetryAction`，不能无条件 next candidate。
5. 为 stream post-commit/stream-idle 维持 summary-only 语义，并在 DTO 中区分 runtime detail unavailable 与 durable terminal summary。

**当前证据与剩余完成条件**

- [x] deadline 起点已前移到 ingress，并在当前已接入的排队、body、admission、等待和 precommit 路径保持 request-local budget。
- [x] 规划 snapshot、affinity、route snapshot、pricing 等前置 I/O 统一包裹剩余 deadline；请求级 timeout fixture 证明这些阶段不会无限等待。
- [x] replay-safe deadline stop 的 lifecycle/health evidence 按 canonical request-acceptance 映射为 `FailedBeforeCommit`；只有明确可能已被上游接受的失败才是 `PossiblyAccepted`。
- [x] `execute_models` 当前明确为只读聚合路径并记录 action reason，不无条件推进普通 candidate。
- [x] 为 models 路径补齐 typed read-only adapter contract、候选继续/停止矩阵和 loopback 场景，确认其不会被误当成普通重试 planner。

**聚焦验证**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib services::proxy::execution -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_failure_contract -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture --test-threads=1
```

## 14. Task 9：Phase 3，统一跨请求故障保护

**启动前置条件：** 已确认 durable verdict、legacy health input 与 runtime capacity state 的读写边界；必须存在生产 observation、持久化和恢复设计。test-only `RuntimeOutlierPolicyV1` 不构成前置能力。

**文件**

- Create: production `HealthProtectionReducer` domain/state/store tests
- Modify: scoped health persistence、candidate projection、protection projector、IPC/UI
- Modify: migration/recovery modules，仅在需要存储新 state 时新增 additive schema

**步骤**

- [x] 为每个 health scope 定义唯一状态机：`Closed -> Open -> HalfOpen -> Closed/Open`，持久化 `stateRevision`、`openedAt`、`cooldownUntil`、有限失败摘要、reason key 和 persistence kind。
- [x] 将现有 durable cooldown/blocking 通过 additive migration/adapter 接入 reducer，避免同一 failure 双写到旧/新 reducer。容量 registry 仍是独立 runtime capacity protection。
- [x] 保留 `conservative`、`balanced`、`aggressive` compiler preset 作为内部基线；当前 public policy 仅开放经过校验的 `protectionProfile` 窗口、最小样本、失败率和 Half-Open 成功次数字段，冷却上限、entry 上限和 preset 选择仍由系统控制。
- [x] 状态转换具备幂等、可恢复和 Half-Open 单 probe 保护；UI/projector 区分 scope、冷却和 persistence kind，不把不同 kind 合并成单一“熔断”。
- [x] 将 Credential probe lease 接入真实 `RoutingService`/proxy outbound 编排：只有选中的候选与 Credential probe scope 一致、目标解析成功且即将跨 outbound boundary 时才消费 lease；未跨边界的 deadline、admission、replan、取消和无 health outcome 路径使用 revision-fenced cancel。
- [ ] 为 Account、Group、Endpoint、Model 增加统一 `probe_scope_for_candidate` resolver（后续任务，不计入本轮完成）：resolver 必须从同一 candidate/request snapshot 生成 scope commitment，第二轮 planning、lease admission、目标解析、终态归因和 revision-fenced cancellation 全部消费同一结果；不得按 UI 聚合 identity 或状态枚举猜 scope。
- [x] 将真实 runtime capacity registry read source 注入 ProtectionStatus；无 runtime 条目时仍返回 unavailable，不能显示伪造的容量条目。
- [x] 固化 reducer `Closed -> Degraded` 的首版映射和稳定 explanation key（当前未打开保护、仍在监控）；`NoProtection` 只用于无观测条目。统一 projector 现在同时接收 durable reducer、legacy、runtime capacity 输入，避免 facade 二次拼接或重复构造空状态；Rust projector/API、前端设置页均有回归测试，且 test-only `RuntimeOutlier` 不进入 production read model。
- [x] 增加 Provider/failure-domain 聚合诊断 read-model：按低敏感 identity、revision 和 commitment 聚合 candidate/schedulable counts，并 join durable/runtime protection、recent failure 与 explanation key；未配置、缺模型、身份无效和已解析状态均显式返回，未解析时不猜 commitment。

**最小验证**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml health_protection -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml routing_health -- --nocapture
pnpm.cmd test -- src/features/routing
```

## 15. Task 10：Phase 4，错误率保护与历史诊断

**启动前置条件：** Phase 3 reducer 生产化，且已有低基数 observation、持久化窗口、retention 和故障恢复证据。当前交付 durable wiring、恢复闭环、summary-only durable attempt projection、V2 `protectionProfile` 的窗口/阈值配置、默认关闭的 Credential-scoped typed planner admission bridge、Credential-scoped 真实 outbound probe 编排和 Provider/故障域聚合诊断；其他 scope resolver、自动 synthetic probe 和完整历史步骤时间线仍不在范围内。

**文件**

- Create: observation window/aggregation/persistence module and tests
- Modify: HealthProtectionReducer、metrics、trace/history query、routing workspace UI

**步骤**

- [x] 以受控 failure code 和 scope 聚合 routing observation；adapter 具备窗口计数、retention、事件上限和低基数 scope commitment，拒绝 credential/model/administrative 等不适合错误率保护的样本。
- [x] 将错误率条件作为 reducer 的输入，而非旁路 breaker；`ObservationIngestion` 在同一 observation transaction 中把 canonical input 交给唯一 `HealthProtectionReducer`，由 `RoutingHealthVerdictStore` 持久化 transition。提交、回滚、重启恢复均有 integration fixture。当前 planner admission 和 probe lease 的生产 scope 为 Credential。
- [x] 增加 bounded error-rate history 的 frontend API/query/UI 消费；支持 disabled、unavailable、empty、cursor、retention 和低基数 failure-code aggregation。
- [x] 保留显式默认关闭能力开关，并以 deterministic fixture 覆盖分类、匿名/行政/凭据/模型/取消过滤、retention、cursor、snapshot restore 和敏感信息不泄漏；这不是发布灰度或远程开关。

**最小验证**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml error_rate_protection -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml decision_trace -- --nocapture
pnpm.cmd test -- src/features/routing
pnpm.cmd build
```

## 15a. Task 11：扩展 Half-Open probe scope（后续独立任务）

**目标：** 在不改变当前 Credential-scoped 默认行为的前提下，让 Account、Group、Endpoint 或 Model scope 能够被真实 proxy 请求安全地发现、租约、发送、归因和恢复。该任务不是通过把现有 Credential 字段改成枚举或复用故障域聚合诊断来完成。

**启动前置条件：** 先选定一个 scope（建议从 Endpoint 开始），并提供该 scope 的稳定低基数 commitment、candidate/request snapshot 来源和明确的错误归因规则。没有这些事实时保持 Credential-only，不开放 UI 开关。

**执行步骤：**

1. 在 application 层新增唯一 `probe_scope_for_candidate` resolver，输入不可变 planning snapshot、candidate、requested model 和当前 protection profile，输出 `HealthProtectionScope` 或明确的 `NotEligible`；禁止从前端 status、日志文本或 URL 临时拼接。
2. 让首轮 discovery、第二轮 strict refresh、candidate admission、lease acquisition、目标解析前校验、出站后 health observation 和无 outcome cancellation 全部消费 resolver 输出；删除 execution/finalization 中 Credential-only 的重复判断。
3. 为 scope commitment 增加 revision/identity 约束和敏感信息测试，确保不同 account/group/endpoint/model 不会误共享 lease，也不会把一个 scope 的失败写入另一个 scope。
4. 增加跨层 loopback：Open 抑制、冷却后只允许一个 probe、并发 lease race、目标解析失败取消、deadline/cancel 取消、probe 成功关闭、probe 失败重开、迟到 revision 被拒绝；至少覆盖一个真实 proxy execution + SQLite reducer/store 闭环。
5. 只有上述证据通过后，才把该 scope 加入 status 的“可执行保护”文案或设置页说明；聚合诊断可以提前展示身份，但必须继续标记为诊断事实而非 probe 能力。

**完成条件：** scope resolver 是唯一 owner；生产 execution 不再硬编码 Credential 作为所有 probe 的匹配条件；scope 之间无误伤、无遗留 lease、无重复终态；当前默认配置和 Credential 回归保持不变。

**最小验证：** 运行新增 scope 的模块 focused tests，加一个 loopback integration；若改动 DTO/IPC 才运行 `pnpm.cmd test:contracts` 和 `pnpm.cmd generate:bindings --check`，不要求发布或真实 Provider smoke。

## 16. 完成门槛与交付顺序

本计划按三个可独立交付的工作包执行：

1. **后端策略包（Task 0-3）**：V2、统一 profile、RetryAction、容量路径和迁移/解码完成；必须通过 policy/execution/capacity/loopback 聚焦回归。
2. **控制面包（Task 4-7）**：ProtectionStatus、严格 IPC、shared draft/CAS、四字段设置和 trace 可见性完成；必须通过生成契约、query/draft、设置页和 build 回归。
3. **后续可靠性包（Task 8-10）**：Phase 2 的真实 deadline/终态/models 闸门、Phase 3 durable reducer、Credential-scoped 真实 outbound Half-Open probe、Phase 4 backend history、V2 `protectionProfile`、Credential-scoped typed admission bridge、summary-only durable attempt projection、按模型 status 查询和 Provider/failure-domain 聚合诊断已完成；profile 默认关闭且只能通过 policy/CAS 生效。其他 scope resolver、自动 synthetic probe、通用 timeout 编辑和完整历史步骤仍需单独准入，不能借本计划自动开放。Task 11 不属于本轮完成定义。

每个工作包完成时必须记录：变更 owner、兼容残留、实际测试命令/结果、未运行检查和剩余风险。本轮实施的完成定义是核心链路可用且有聚焦证据；以下条件用于未来把目标 spec 从 Proposed 改为 Implemented：

- Task 8a 的 proxy deadline、models action 和 terminal classification 闸门关闭；
- public V2 document 缺失字段/unknown/duplicate/future/非法值均 fail-closed；
- 设置页显示真实 timeout facts，trace 能区分详细可用和 summary-only；
- V1 数据库 row 的物化迁移策略有明确实现和恢复证据，或在目标规格中明确保持兼容读取；
- error-rate history 的 frontend owner、默认关闭的 Credential-scoped typed admission bridge、V2 profile 的可调窗口/阈值、summary-only durable attempt history、有界 durable lifecycle event timeline、Credential-scoped 真实 outbound probe lease 编排、按模型 status 查询和 Provider/failure-domain 聚合诊断已接通，且其他 scope resolver、synthetic probe、通用 timeout 编辑和完整历史步骤的范围边界仍有明确记录。

增强性黑盒 fixture、完整 UI 矩阵和极端故障注入属于后续质量提升，不阻断本轮核心交付；已通过的聚焦测试只证明对应工作包，不等同于发布资格。

## 17. 测试策略（开发验证，不是发布门槛）

本计划只要求证明本次改动真实生效且没有破坏直接相关行为。测试按改动范围选择，不要求发布、安装包、真实 Provider、性能压测、跨设备同步、`verify:full` 或 `verify:release`。文档或计划修订通常只需做文档一致性检查；只有代码、契约、迁移或 UI 发生变化时才运行相应的 focused test。

| 改动范围 | 至少验证 |
| --- | --- |
| Rust 策略/执行/保护 | 运行受影响模块的 focused tests；需要时补 `cargo check` 或 `cargo fmt -- --check` |
| 重试与故障转移 | 至少一个能断言 outbound 次数、动作或终态的 focused/loopback 场景 |
| 持久化 summary/决策事件/迁移 | 对应 store/migration focused test；重启语义变更时再测恢复和事件顺序 |
| IPC、权限、生成绑定 | 修改 DTO/registry/权限时运行 `pnpm.cmd test:contracts`，必要时运行 `pnpm.cmd generate:bindings --check` |
| React/query/UI | 相关 Vitest；改到构建类型边界时再运行 `pnpm.cmd build` |
| 文档/台账 | `git diff --check`，并核对当前代码边界、计划状态和剩余风险一致 |

需要快速确认后端核心链路时，可使用下面的轻量组合；不要求每次完整执行：

```powershell
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_policy -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib durable_decision_events_are_ordered_and_survive_runtime_restart -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture --test-threads=1
git diff --check
```

## 18. 每 Task 记录模板

```text
Task:
Start HEAD / End HEAD:
Dirty paths preserved:
Precondition / RED evidence:
Changed domain, runtime, persistence, IPC, UI contracts:
Owners removed / compatibility retained (with reason):
Focused tests run and results:
Checks intentionally not run (not release qualification):
Residual risks / follow-up phase gate:
```

## 19. 禁止偏移

- 不复制 CCSwitch 的代码、名称或“最大重试次数”语义；只实现本规格定义的、包含首次发送的总 attempts。
- 不把容量 retry、通用故障转移和跨请求熔断混成一个表单开关或一个状态机。
- 不把 `Retry-After`、timeout、request deadline 或 test-only outlier 参数提前写进 `routing-policy.json`。
- 不让 proxy、React、generic Settings 或 legacy health snapshot 直接写 active policy/保护状态。
- 不用 silent clamp、JSON default、文件时间戳或重新开始 replan 绕过 validation、CAS、deadline 或 budget。
- 不扩大 trace 保留量来替代历史数据设计，也不记录 secret、完整 URL、请求/响应正文。
- 不为这次实施增加发布、远程灰度、账号、云同步或真实 Provider 操作；这些均不属于本地桌面路由控制面的当前计划。

## 20. 本次执行记录

本轮按后端策略、控制面和 Phase 3/4 backend foundation 执行了聚焦实现与审计。已确认：

- V2 policy、V1-to-V2 storage upgrade、0050/0052 storage materialization、`AttemptBudgetProfileV1`、typed `RetryAction`、容量 profile 注入、ProtectionStatus、durable reducer/history transaction 和 generated IPC 基础均已接入。
- 生产路径不再保留独立 `max_candidate_attempts`、`max_upstream_attempts` 或 `RetryDecision` owner；V1 仍是兼容解码输入，0050 已将类型正确的 V1 active/history row 物化为 V2，0052 补齐旧 V2 缺失 profile，错误类型 row 留给 typed recovery。
- shared query/draft/CAS 和四个容量字段编辑已接通；`showDecisionExplanation` 独立保存。timeout 数值、structured trace、请求详情时间线、error-rate history frontend query 和 nested draft merge 已闭合。
- Phase 2 的容量重试、transport policy、failure-domain exclusion、ingress deadline anchor、规划 snapshot/affinity/route/pricing 前置 I/O deadline、typed `/models` 只读聚合适配器和非 proxy caller-owned deadline context 均已接入；通用 timeout 编辑和全局 deadline 字段不在本轮开放范围。
- Phase 3 reducer/recovery/Half-Open、显式 revision-fenced Credential probe lease、runtime capacity read source 与 Phase 4 observation/history backend/frontend 已默认关闭并通过聚焦验证；V2 protection profile 已通过同源 policy/CAS、0052 迁移和 reducer reconfigure 接入观测写入和 Credential-scoped planner admission，设置页可编辑受限窗口/阈值字段；durable attempt summary 与重启后有界 lifecycle event summary 已接入请求详情。Credential probe lease 已接入真实 `RoutingService`/proxy execution 编排，lease 竞争后的 strict refresh 也已覆盖；基础 Provider/domain 诊断和按模型 status query 已开放，其他 scope resolver、完整历史步骤仍不在当前范围。

本轮实际通过：

- 静态/编译：`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、`cargo check --locked --manifest-path src-tauri/Cargo.toml`、`git diff --check`。
- Rust 聚焦：routing policy 19、execution 37、schema 50 migration 2、schema 52 migration 1、strict IPC 5、health protection 8、routing protection 8、error-rate protection 9、request decision trace 4；隔离 target 的 `routing_loopback_e2e` 12、`routing_failure_contract` 2、`routing_capacity` 8 个用例全部通过。
- 前端/契约：`pnpm.cmd test -- --run` 实测 115 files/446 tests 全部通过，`pnpm.cmd test:contracts`、`pnpm.cmd build`（含 theme audit、TypeScript 检查和 Vite 构建）通过；UI 增强矩阵单独 21 tests 通过，`pnpm.cmd generate:bindings --check` 通过（4 artifacts、两次干净生成一致且 tracked artifacts 无漂移）。

Rust 输出包含仓库已有的 unused/dead-code/unfulfilled-lint warnings，前端测试包含既有 React/jsdom fixture 的 stderr，不影响退出码或通过结果。loopback 使用独立 `CARGO_TARGET_DIR`，没有结束正在运行的桌面进程。后续每次执行必须以实际命令输出更新本记录，不能沿用旧的“全量通过”摘要。

未运行且明确不属于本次门槛：发布/安装包、真实 Provider smoke、`verify:full`、`verify:release`、性能压测、跨设备同步和全量 Rust 单测。

### 2026-08-21 文档审计后追加验证

- 通过：`git diff --check`、`pnpm.cmd test:contracts`、`pnpm.cmd generate:bindings --check`、`pnpm.cmd architecture:security`。
- 通过：`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_policy -- --nocapture`（18/18）；使用独立 `CARGO_TARGET_DIR=target-blackbox` 重跑 `routing_loopback_e2e`（12/12）。默认 target 的同一 loopback 命令曾因桌面进程锁定 `src-tauri/target/debug/relay-pool-desktop.exe` 报 Windows `os error 5`，不计为断言失败，已通过隔离 target 完成验证。
- 上述 ACL/registry 失败已修复：`main-window.toml` 已声明 `apply_routing_policy_document`、`get_routing_protection_status`、`list_error_rate_history`，Cargo/Tauri 生成的 compiled ACL 已同步；`node scripts/architecture/check-command-registry.mjs` 通过（196 commands）。
- 通过：`pnpm.cmd architecture:security`、`pnpm.cmd test:contracts`、`pnpm.cmd generate:bindings --check`、`pnpm.cmd audit:dead-code -- --mode verify`（0 visible dead_code diagnostics）、`cargo fmt`、`cargo check`。
- 代码收口：删除无消费者的 legacy V1 apply facade、V2->V1 helper、生产无消费者 timeout/transport 构造；测试/迁移兼容构造显式登记 contract 或限制在 `cfg(test)`，未使用全局 dead-code allow。
- 结论：首版控制面和 ACL/生成契约已闭合；规划 I/O deadline、`/models` adapter、V2 protection profile、error-rate admission bridge、durable attempt summary projection、重启后有界 lifecycle event summary 和非 proxy caller-owned deadline 传播均已由本轮收口，仍不因此自动编排 Provider probe 或开放完整 Provider/domain 诊断。

### 2026-08-21 本轮计划收口验证

- 通过：`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、`cargo check --locked --manifest-path src-tauri/Cargo.toml`、`git diff --check`。
- 通过：`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_policy -- --nocapture`（18/18）、`--lib health_protection`（8/8）、`--lib routing_protection`（8/8）、`--lib error_rate_protection`（9/9）、`--lib request_decision_trace`（4/4）、`--lib application::routing::tests::planning_snapshot_rejects_an_expired_caller_deadline`（1/1）。
- 通过：使用 `CARGO_TARGET_DIR=target-blackbox` 的 `routing_loopback_e2e`（12/12），覆盖总尝试预算、同目标容量次数、等待预算和跨容量域开关。
- 通过：`pnpm.cmd test:contracts`、`pnpm.cmd test -- --run`（115 files / 446 tests）、`pnpm.cmd build`（theme audit、TypeScript 和 Vite build）。
- `pnpm.cmd verify:fast` 首次因桌面进程 PID `56392` 持有 `src-tauri/target/debug/relay-pool-desktop.exe` 返回 `os error 5`；未结束该进程，改用 `CARGO_TARGET_DIR=target-verify-fast` 重跑后通过（Rust architecture fixtures 4/4）。
- Rust/前端仍有仓库既有 warning（unused、unfulfilled lint expectation、chunk size），无新增 error；本轮未运行发布、安装包、真实 Provider、性能压测、`verify:full` 或 `verify:release`。
- 本轮新增聚焦证据：`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_protection -- --nocapture`（5/5）、`--lib execution -- --nocapture`（37/37）；`/models` adapter、planning timeout 和 terminal classification 均由 execution 单测覆盖，loopback 12/12 通过。默认 target 的 loopback 曾受 PID `56392` 锁文件影响，隔离 target 已完成实际断言。
- 剩余边界：非 proxy 直接调用方必须显式传入 caller-owned deadline，service 不自动猜测或重置 ingress deadline；Credential-scoped error-rate reducer 到 planner admission 的 bridge 默认关闭，V2 protection profile 启用后由同一 policy/CAS 控制；Credential probe lease、lease-race strict refresh、基础 Provider/domain 诊断和按模型 status query 均已接入；Account/group/endpoint/model probe resolver、完整历史仍未开放；durable trace 提供有界重启后事件，不承诺无限或完整原始历史。

### 2026-08-21 新增 durable probe foundation 回归

- 修复 `HealthProtectionProfileV1` 测试 fixture 的 `enabled` 字段缺失；生产默认配置行为未改变。
- 通过：`health_protection`（8/8）、`error_rate_protection`（10/10）、`routing_protection`（8/8）、`planning_snapshot`（6/6）、`durable_probe_reservation_is_single_use_and_revision_fenced`（1/1）。
- 通过：`cargo check --locked --manifest-path src-tauri/Cargo.toml`、相关保护文件的单文件 `rustfmt --check`、`git diff --check`。
- 结果含义：Open scope 的候选抑制、Half-Open 的匹配 lease admission、单 probe reservation 和 revision fence 已有后端证据；probe lease 尚未接入真实 `RoutingService`/proxy execution composition，不自动发起 Provider 探测请求，因此不能描述为已启用的生产 pool ejection/probe。
- Rust 输出仍含仓库既有 warnings；未运行发布、安装包、真实 Provider、性能压测、`verify:full` 或 `verify:release`。

### 2026-08-21 可配置保护 profile 与兼容迁移收口

- 新增 0052 additive migration：对旧代码生成、已是 V2 但缺少 `protectionProfile` 的 active/history row 补齐默认关闭 profile；显式 profile、revision 和损坏基础字段均不被覆盖。
- 新增 `HealthProtectionReducer::reconfigure` 回归：修改窗口只裁剪证据，不重置 Open/Half-Open、冷却时间或 probe fence。
- 修复前端历史 V2 fixtures，保护 profile 字段在 policy、workspace、draft、API/query 和设置页测试中保持完整。
- 通过：`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib schema_52_materializes_missing_protection_profile_on_existing_v2_rows -- --nocapture`（1/1）、`--lib routing_policy -- --nocapture`（19/19）、`--lib reconfigure_trims_window_without_resetting_open_or_probe_state -- --nocapture`（1/1）、相关 Vitest（115 files/446 tests）和 `pnpm.cmd build`。
- 当前边界：保护 profile 已是可配置控制面，但默认关闭；自动 Provider probe/pool ejection 编排、完整 Provider/domain 诊断和发布类验证仍不属于本计划。

### 2026-08-21 审计补丁与最终测试

- 修复重启恢复：未完成请求现在幂等物化为脱敏的 interrupted durable outcome 和 request_finalized 事件；durable terminal summary 与 lifecycle events 同时保留，post-commit 失败不再误记为成功。
- 收紧 public V1 policy decoder：缺少原 storage 默认字段、unknown field 或 snake_case storage 形状均拒绝；IPC V2 protectionProfile 缺字段、unknown 和非法值继续 fail-closed。IPC 原始重复键保证仍只属于 raw/file decoder，已解析的 Tauri Value 不承诺检测原文重复键。
- 补齐故障域 read-model：候选现在携带低敏感的 provider/deployment/region、revision、稳定 commitment 和 not_configured、model_required、invalid_identity、resolved 解析状态；UI 仅展示身份事实，不将其当作运行时健康或自动 probe。
- 通过：cargo fmt、cargo check、git diff --check；routing policy 19、strict V1 decoder 1、strict IPC V2 1、routing workspace 4、durable restart event 1、post-commit event classification 1、durable terminal projection 1、lifecycle reconciliation 2。
- 通过：pnpm generate:bindings（4 artifacts，两次生成确定性）及随后 pnpm generate:bindings --check、pnpm test:contracts、pnpm build；Vitest 全套此前通过 115 files / 446 tests。生成 bindings 期间仅有仓库既有 Rust warnings 和 Vite chunk size warning。
- 默认 Cargo integration target 曾因运行中的桌面进程占用 relay-pool-desktop.exe 返回 Windows os error 5；未结束该进程，已用独立 CARGO_TARGET_DIR 完成 lifecycle 和相关 focused tests。未运行发布、安装包、真实 Provider smoke、性能压测、verify:full 或 verify:release。
- 剩余边界：Half-Open lease 仍未接入真实 RoutingService/proxy execution probe 编排；完整 Provider/domain 聚合诊断、无限历史时间线和旧 codec 的资源上限仍需独立任务，不在本次开放范围。

### 2026-08-21 durable 决策事件重启回归

- 通过：`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib durable_decision_events_are_ordered_and_survive_runtime_restart -- --nocapture`（1/1）。
- 覆盖：请求开始、attempt 开始/完成、请求终态事件的有序写入；关闭并重新打开 SQLite runtime 后，事件序列、字段和顺序保持一致。
- 修复：测试夹具在重启 runtime 前释放第二个 `ReadSession`，避免 close 等待 active-work 而造成假性挂起；未修改生产关闭协议。

### 2026-08-21 本轮最终回归与观测边界修正

- 发现并修复：Half-Open `probe_state_revision` 判断曾把没有 probe fence 的普通 `RealRequest` 一并过滤，导致真实用户流量无法进入错误率观测。现规则为：普通 `RealRequest` 仍作为错误率样本；只有携带 revision fence 的 `RealRequest` 才标记为 Half-Open probe；没有 fence 的 `ActiveProbe` 仍 fail-closed，不进入保护 reducer。
- 通过：`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、`cargo check --locked --manifest-path src-tauri/Cargo.toml`、`git diff --check`。
- 通过：`routing_policy`（19/19）、`health_protection`（9/9）、`routing_protection`（8/8）、`error_rate_protection`（12/12）、`planning_snapshot`（6/6）、`execution`（38/38）。错误率模块额外覆盖未带 fence 的 `ActiveProbe` 不进入 reducer。
- 通过：隔离 `CARGO_TARGET_DIR=target-verify-latest` 的 `routing_loopback_e2e`（12/12），覆盖总尝试预算、同目标容量次数、等待预算、跨域开关和请求终态边界。
- 通过：`pnpm.cmd test:contracts`、`pnpm.cmd generate:bindings --check`（4 artifacts、两次生成确定性）。此前前端 Vitest 全套为 115 files / 446 tests，`pnpm.cmd build` 已通过；本次 Rust-only 修正未改变前端代码。
- 仍明确不运行：发布/安装包、真实 Provider smoke、性能压测、跨设备同步、`verify:full` 和 `verify:release`。Rust 输出中的 unused/dead-code/unfulfilled-lint warnings 属于仓库既有噪声，不影响上述退出码。

### 2026-08-21 probe 编排与故障域诊断收口

- 真实 proxy outbound 路径现在消费 Half-Open probe lease：首轮规划只负责发现冷却到期 scope，第二轮携带准确的 revision fence；仅在候选、scope 和 lease 一致且目标解析成功、即将发送真实 outbound request 时才保留 probe。目标解析失败、admission/deadline/replan 失败、显式取消以及出站后没有 health outcome 的路径均执行 revision-fenced cancel；迟到 reducer 结果按 stale 处理，避免 probe slot 永久占用。
- 新增 `RoutingFailureDomainDiagnostic` 聚合 read-model 和 `RoutingStatusDiagnosticsPanel`：按归一化 provider/deployment/region identity、revision 和 commitment 聚合候选总数/可调度数，并 join durable/runtime protection、recent failure 和 explanation key；`not_configured`、`model_required`、`invalid_identity`、`resolved` 均可见，未解析 identity 不猜 commitment。
- `get_routing_protection_status` IPC 现已接收可选 `model`，并由 RoutingPage 将最近请求模型纳入 query key；不同模型不会共用缓存。未提供模型时仍明确显示 `model_required`，不猜具体 commitment；有模型时按同一请求模型生成 model-scoped failure-domain join。
- 通过：`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、`cargo check --locked --manifest-path src-tauri/Cargo.toml`、`git diff --check`；`health_protection`（9/9）、`planning_snapshot`（6/6）、`execution`（38/38）、`routing_protection`（11/11）以及 store probe cancellation integration（1/1）。
- 通过：`pnpm.cmd generate:bindings`（4 artifacts，两次干净生成确定性）、`pnpm.cmd generate:bindings --check`、`pnpm.cmd test:contracts`；此前前端 Vitest 115 files/446 tests 和 `pnpm.cmd build` 已通过，本轮新增 `RoutingStatusDiagnosticsPanel` 2 tests 通过。
- 已知边界：自动 synthetic probe、通用 timeout 编辑、无限/完整历史时间线和旧 codec 的资源上限仍需独立任务；Half-Open lease 竞争后的 strict snapshot refresh 已接入并由聚焦回归覆盖。仍不运行发布、安装包、真实 Provider smoke、性能压测、`verify:full` 或 `verify:release`。

### 2026-08-21 本轮计划执行验证

- Rust 基础检查通过：`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、`cargo check --locked --manifest-path src-tauri/Cargo.toml`。
- Rust 聚焦测试通过：`routing_protection`（13/13）、`health_protection`（11/11）、`error_rate_protection`（13/13）、`planning_snapshot`（6/6）、`execution`（39/39）。
- 隔离 target 的集成测试通过：`routing_loopback_e2e`（12/12，覆盖总尝试预算、同目标容量次数、等待预算、跨容量域开关和终态边界）；`routing_lifecycle_reconciliation`（2/2，覆盖重启恢复和有界批处理）。
- 前端与契约通过：`pnpm.cmd test -- --run`（116 files / 449 tests）、`pnpm.cmd test:contracts`、`pnpm.cmd generate:bindings --check`（4 artifacts、两次生成确定性）、`pnpm.cmd build`（theme audit、TypeScript、Vite build）、`git diff --check`。
- 输出中的既有 Rust unused/dead-code/unfulfilled-lint warnings、Node 子进程 deprecation warning、React/jsdom `act(...)` stderr 和 Vite chunk size warning 不影响上述命令退出码；未运行发布/安装包、真实 Provider smoke、性能压测、跨设备同步、`verify:full` 或 `verify:release`。

### 2026-08-21 作用域审计修正（覆盖本记录更早的宽泛表述）

- 真实 proxy Half-Open probe 的 production composition 已核对为 Credential-only：`execution` 的 acquisition、planning snapshot 的 error-rate admission/discovery，以及 finalization 的 probe cancellation 都按 Credential commitment 处理。
- `HealthProtectionScopeKind`、durable reducer 和 `ProtectionStatus` 可以表达 Account/Group/Endpoint/Model 等 scope，这是状态模型和诊断能力，不是这些 scope 已具备真实 probe 的证明。
- 因此本计划当前完成定义只承诺 Credential-scoped probe；Account/Group/Endpoint/Model 的统一 resolver、scope-specific admission、目标解析归因和跨层闭环已列为 Task 11 后续任务。早期记录中未区分 scope 的“真实 outbound probe”字样按本节解释，不作为更宽能力的验收证据。
