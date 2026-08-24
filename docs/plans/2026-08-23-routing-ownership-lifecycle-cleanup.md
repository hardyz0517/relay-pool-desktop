# 路由职责与生命周期收敛计划

状态：分阶段实施中。本文只处理已确认的路由职责重叠、配置激活旁路、字符串错误边界和运行时状态混装；不改变路由算法、重试安全门、熔断算法、模型映射规则或请求双终态语义。

日期：2026-08-23

关联入口：[`../README.md`](../README.md)、[`../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md`](../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md)、[`2026-08-20-intelligent-routing-retry-failover-configuration.md`](2026-08-20-intelligent-routing-retry-failover-configuration.md)、[`2026-08-23-transport-timeout-hot-reload.md`](2026-08-23-transport-timeout-hot-reload.md)

适用范围：路由 policy 控制面、proxy 到 application 的 planning bridge、routing runtime process state、路由 query/command composition 与 policy document background runner。

不在范围：候选评分公式、RetryAction/ReplayedGate/FailureClassifier、HealthProtectionReducer 状态机、持久化 schema、请求/响应协议、模型映射语义、设置页视觉改版、请求双终态 finalization 重写、真实 Provider 测试和发布验证。

## 当前实施状态（2026-08-23）

本轮只完成已经有明确 caller 和回归证据的收口，不把“文件变小”当成完成标准。

| 任务 | 状态 | 已落地/保留的边界 |
| --- | --- | --- |
| Task 0：调用图与行为基线 | 已完成 | 增加了 production architecture gate、typed-error 和单一 policy owner 断言；未改变评分、重试、熔断或请求终态。 |
| Task 1：policy 控制面 | 已完成 | UI 和 managed document reconcile 使用 `RoutingPolicyMutationCoordinator`；CAS 提交后统一发布 transport snapshot，并由 mutation gate 串行化提交与激活。协调器内部暂时复用 `RoutingService` 的 policy aggregate 实现；当前没有生产历史恢复 caller。 |
| Task 2：execution bridge | 已完成 | proxy 依赖 `RoutingExecutionReadPort`；deadline 等结果使用 typed error；生产 trait 没有静默空值默认实现。`RoutingExecutionReader` 暂时是对既有 application 实现的窄适配器。 |
| Task 3：拆分 `RoutingService` | 部分完成，按需后续 | 已收口 proxy execution 和 policy mutation 的入口，但 `RoutingService` 仍承载剩余 query、model mapping、endpoint orchestration 及其过渡实现。没有证据证明 caller 已全部迁移前，不继续拆文件或改名。 |
| Task 4：runtime 生命周期拆分 | 已完成 | capacity/retry、activity、diagnostics 已分模块，`RoutingRuntimeState` 仅作组合根；RAII、容量、Half-Open 和 bounded diagnostics 行为保持不变。 |
| Task 5：read/command 门禁 | 部分完成 | 已加入 proxy 依赖、deadline magic string、policy runner 旁路等架构门禁；`RoutingCommandFacade` 仍保留 `Arc<RoutingService>` 以承载尚未迁移的 read/model-mapping/endpoint caller。 |
| Task 6：endpoint 探测与周期边界 | 已定义、待迁移 | 已新增 `routing_endpoint_ports.rs` 窄 port 草案，但尚未接入 caller/composition；手动 ping、站点密钥连通性和周期监控继续保持独立编排周期。 |

后续只在同时满足“caller 清单明确、旧方法可以删除、存在行为回归测试、能增加一条架构门禁”时推进 Task 3/5。仅为降低行数、统一目录或追求形式上的 service 数量而进行的重构不在本计划内。

> 这是减法计划。没有已验证的重复 owner、旁路或生命周期错误，不新增 service、trait、event bus 或目录。每次提取必须删除一个原 owner 的生产职责，并以现有行为测试证明没有改语义。

## 1. 已确认的问题

| 证据 | 问题 | 影响 |
| --- | --- | --- |
| `application/routing.rs` 当前约 2259 行 | `RoutingService` 同时代理模型映射、policy/CAS、managed document、健康保护、planning、候选/定价读取、工作区 read model、trace、模拟和 endpoint health。 | 任何改动都扩大依赖面，难以知道配置、请求与诊断各由谁负责。 |
| `background_tasks/policy_document_runner.rs` 仅持有 `PersistenceHandle`（基线，已修复） | 外部 routing policy 文件直接调用 `RoutingService::apply_routing_policy_document_v2`，绕过 command facade 和 runtime policy activation。 | UI 保存与文件导入可能产生不同的运行时生效行为。 |
| `services/proxy/routing_repository.rs` 持有整个 `RoutingService`（基线，已修复） | proxy execution 依赖一个包含大量写入与 UI 查询能力的 service；trait 用字符串 `PLANNING_DEADLINE_EXCEEDED` 保留 typed deadline。 | execution 层依赖过宽，错误分类可能因文案/字符串改动漂移。 |
| `services/proxy/routing_runtime.rs` 当前约 927 行（基线，已修复） | 容量 retry、request activity、decision trace、metrics、diagnostic memory 共享一个生命周期对象和文件。 | process-lifetime、request-lifetime、diagnostic-lifetime 难以区分，后续热加载容易继续向其中堆状态。 |

以下内容已经有独立 owner 和回归，不纳入“顺手清理”：`request_finalization` 双终态、`AttemptLifecycle`、`FailureClassifier`、`ReplayGate`、`RetryActionPlanner`、durable health reducer、现有 canonical planner/projection。不能以“统一路由”为理由把它们重新并进 `RoutingService` 或 proxy runtime。

## 2. 目标职责与生命周期

```text
Policy input (UI / managed JSON / history restore)
  -> RoutingPolicyMutationCoordinator
  -> existing `RoutingService` policy aggregate (temporary): validate + CAS persist
  -> TransportPolicyPublisher: activate committed revision
  -> document mirror: best-effort materialization

Proxy request
  -> ingress request snapshot
  -> RoutingExecutionReadPort: planning / operational target / protection probe
  -> admission + late credential/target resolution + attempt
  -> existing dual-terminal finalization

Read command
  -> routing query service/projector
  -> DTO
```

| 生命周期 | 创建者 | 唯一可变 owner | 销毁/失效 | 禁止事项 |
| --- | --- | --- | --- | --- |
| 持久化 routing policy | policy mutation coordinator | SQLite CAS aggregate | 被新 revision 替代 | proxy request 直接读 SQLite 或绕过 CAS。 |
| 运行中 transport policy | `ProxyRuntimeState` | `TransportPolicyStore` | proxy 进程退出；停机保留 desired snapshot | watcher/UI 各自维护一份当前配置。 |
| 单请求 planning/transport snapshot | ingress | request context | terminal response/cancel | retry/replan 再读当前全局配置。 |
| 容量 retry / request activity | `RoutingProcessState` | 相应 registry/index | proxy stop | 写入 durable health 或在 UI 自行推算。 |
| decision trace / metrics | diagnostics owner | bounded ring/accumulator | 容量淘汰或 proxy stop | 作为 durable request lifecycle 的替代品。 |
| durable request outcome/protection | existing finalization/reducer | persistence transaction | retention/新 state revision | 合并到 process runtime state。 |

### 2.1 周期与触发模型

“周期混乱”不通过再造一个总调度器解决，而是把已有触发源分成四种，并为每种触发源规定唯一入口：

| 周期 | 触发方式 | 允许做的事 | 明确不做的事 |
| --- | --- | --- | --- |
| 单请求周期 | ingress 接收请求时创建，响应、取消或终态写入时结束 | 固定本次 request/transport snapshot；在同一 deadline 内执行 retry、replan、late target resolution | retry 期间重新读取全局 policy、直接读 SQLite、修改 process registry 的 owner 规则 |
| 代理进程周期 | `ProxyRuntimeState.start/stop` | 创建/替换 transport snapshot、创建 `RoutingRuntimeState` 组合根、在 stop 时释放 capacity/activity/diagnostics | 把临时状态写入 durable health；由后台任务偷偷重建请求状态 |
| 受管文档同步周期 | 文件事件触发，750ms 稳定窗口后 reconcile；每 30s 做 digest fallback；watcher 失效时由 fallback 继续兜底 | 读取稳定文档并调用 `RoutingPolicyMutationCoordinator`；成功 CAS 后发布 revision；记录无效/不稳定诊断 | 直接写 policy store、直接发布 proxy runtime、与模型映射文档共用一个不透明 mutation owner |
| durable 投影维护周期 | routing projection 每 1s 批量消费 observation，最长 60s 刷新 stale 状态 | 更新 read model/quality summary，支持查询和诊断 | 参与请求选路、修改 active policy、覆盖 durable health reducer 的状态 |

周期之间只通过明确的数据契约连接：请求读取 immutable snapshot，文档同步提交 revision，投影消费 durable observation。不得通过共享可变字段、跨周期 callback 或“顺便刷新”方式建立隐式依赖。新增 timer 前必须注明所属周期、owner、取消点和失败后的重试/退避策略。

## 3. 不可变的工程决定

1. policy aggregate（当前仍由 `RoutingService` 承载）保留领域校验、CAS 和 persistence；它不持有 proxy、outbound client、watcher、Axum 或 UI composition。只有在 caller 迁移完成后，才考虑把它改名或提取成独立 service。
2. 每个 active policy 写入入口都经过 `RoutingPolicyMutationCoordinator`。它是应用组合层，不是新的领域 aggregate，也不是泛用 event bus。启动阶段的 `refresh_protection_configuration` 目前仍是从已持久化 policy 向 protection runtime 的兼容性 hydration bridge，不是用户 mutation 入口；在独立的启动激活回归完成前，不宣称所有 runtime hydration 都已由 coordinator 统一。
3. proxy 只依赖最窄的 `RoutingExecutionReadPort`，而非整个 `RoutingService`。该 port 只暴露 execution 必需的 planning、target、execution settings 和 probe 操作。
4. proxy/application bridge 以 typed error 表达 `DeadlineExceeded`、`Unavailable`、`InvalidState` 等稳定结果；禁止以 magic string 或错误文本驱动 retry/terminal 分类。
5. `RoutingRuntimeState` 可作为兼容外观暂时保留，但其成员按容量、活动、diagnostics 分模块；不能新增与上述三类无关的状态。
6. command facade 可以聚合多个窄 service 供 IPC 使用，但不能重新成为业务规则 owner。读命令只调用 query owner，写命令只调用 control-plane owner。
7. 本计划不要求将所有路由代码按文件行数拆散。只有迁移 caller 后能删除原方法、缩窄依赖或消除旁路的提取才执行。

## 4. 任务与切换顺序

### Task 0：冻结调用图与行为基线

**目标：** 在移动代码前把实际入口、状态和安全行为固化。

**文件**

- Update: `docs/audits/routing-retry-failover-deletion-ledger.md`
- Update: `scripts/routing-single-owner.test.mjs`
- Update: 相关 routing/proxy focused tests

**步骤**

1. 记录 `RoutingService` 每个公开方法的 consumer，标记为 policy mutation、execution read、query read、模型映射兼容代理或 endpoint operation。
2. 记录 policy 写入入口：IPC UI apply、managed JSON watcher、history restore、启动同步；明确哪个入口实际修改 active SQLite aggregate。
3. 固化以下黑盒行为：CAS conflict 不改变 active policy；外部 document 无效不改变 active policy；proxy planning deadline 保持 canonical terminal 分类；request finalization 不因本轮服务拆分产生双写。
4. 在 architecture script 增加初始 inventory 断言，而非仅依赖 LOC 阈值。后续 Task 删除一个职责时更新为“该职责只有新 owner 可调用”。

**完成条件：** 每个计划内迁移都有原调用方、目标 owner、删除条件和 focused test；没有“先移动，之后再找 caller”的任务。

### Task 1：收口 policy 控制面与 document 生命周期（已完成）

**目标：** 让 UI、文件导入和历史恢复的持久化与运行时激活行为完全相同。

**前置：** `2026-08-23-transport-timeout-hot-reload.md` 的 `TransportPolicyPublisher` 和长期 `TransportPolicyStore` 设计已落地或在同一原子变更中落地。

**文件**

- Create: `src-tauri/src/application/routing_policy_control_plane.rs`
- Modify: `src-tauri/src/application/routing.rs`
- Modify: `src-tauri/src/application/command_facades/routing.rs`
- Modify: `src-tauri/src/background_tasks/policy_document_runner.rs`
- Modify: `src-tauri/src/app_composition.rs` 与启动 task 注册处
- Update: policy CAS/document reconciliation tests

**步骤**

1. 创建 `RoutingPolicyMutationCoordinator`，依赖现有 `RoutingService` 的 policy aggregate 实现和 `ProxyRuntimeState` transport publisher；它只暴露 apply/import/restore/reconcile 的命令级方法。这里的 `RoutingService` 依赖是过渡实现，不代表 proxy 或 UI 可以继续直接调用 policy store。
2. 保留 `RoutingService` 内已有的 persistence/CAS 实现作为过渡性内部实现，但将 IPC facade 迁移到新的 coordinator；迁移完成后删除 facade 对 `RoutingService::apply_routing_policy_document_v2` 的直接调用。
3. 将 `policy_document_runner` 从 `PersistenceHandle` 直连模式改为注入 coordinator。model mapping 保持现有独立 reconcile 调用，不能借机把两种文档合并成一个不透明 runner。
4. 明确启动顺序：persistence ready -> policy document startup reconcile（只读/镜像）-> proxy 从 active SQLite policy 安装 runtime snapshot -> background document runner 开始。runner 发现外部有效更新后走 coordinator，发布较新 revision；无效文件只更新同步诊断。
5. 同一 coordinator 在进程内串行化 mutation + post-commit activation；runtime publisher 仍以 revision fence 防御延迟调用。不得要求 SQLite transaction 跨越 runtime lock。

**完成条件：** 搜索不到 `policy_document_runner` 直接创建 `RoutingService` 或直接调用 routing policy store 的 production 路径；所有 active policy mutation 都有同一 activation contract。启动 hydration bridge 单独登记并验证，不与 mutation contract 混为一谈。

**实际结果：** `policy_document_runner` 只持有 coordinator；UI command 也经由 coordinator；CAS 成功后才发布 runtime snapshot，失效、冲突或不稳定文档不会改 active runtime。当前没有生产历史恢复 caller，因此不保留无 caller 的兼容方法；将来恢复入口必须复用同一 coordinator 的内部 apply 边界。

### Task 2：缩窄 proxy 与 application 的 execution bridge（已完成）

**目标：** 让 proxy execution 只获得执行所需能力，并消除字符串 deadline 边界。

**文件**

- Create: `src-tauri/src/application/routing_execution_reader.rs`
- Modify: `src-tauri/src/services/proxy/routing_repository.rs`
- Modify: `src-tauri/src/application/routing.rs`
- Modify: `src-tauri/src/app_composition.rs`
- Update: proxy execution, planning deadline, Half-Open probe tests

**步骤**

1. 定义 `RoutingExecutionReadPort`，仅含 immutable planning snapshot、execution settings、balance/operational target snapshot、current target fence、health protection probe/status 操作。
2. 用 `RoutingExecutionReader` 实现该 port；它可复用既有 planner、operational facts 和 health service，但不得包含 UI workspace、trace、policy document 或 model mapping 方法。
3. 将 `RoutingExecutionRepository` 的字段从 `RoutingService` 替换为 `Arc<dyn RoutingExecutionReadPort>`，composition root 注入实现。
4. 将 `Result<_, String>` 演进为 `Result<_, RoutingExecutionReadError>`，至少表达 `DeadlineExceeded`、`Unavailable`、`InvalidState`。在 proxy 边界一次性映射到安全的 `ProxyFailure`；删除 `PLANNING_DEADLINE_EXCEEDED` magic string。
5. 移除 trait 中会给 production 漏实现提供空 `Vec`、默认 settings 或 `None` 的默认方法。为测试提供显式 `TestRoutingExecutionReader` builder，默认 fail closed，测试必须声明所需行为。

**完成条件：** proxy execution 不再 import `RoutingService`，deadline 不再依赖字符串匹配；遗漏 capability 会在编译期或测试 fixture 构造时失败，不会静默退化。

**实际结果：** `RoutingRepository` 和 execution path 只依赖 `RoutingExecutionReadPort`；`RoutingExecutionReadError` 在边界映射 deadline/unavailable/invalid-state/internal。reader 内部暂时委托既有 planner/query 实现，后续只有在 caller 迁移后才删除旧 wrapper。

### Task 3：按职责剥离 RoutingService，而非按文件切块（部分完成，后续受控）

**目标：** 删除当前 broad service 的真实职责，保留少量清晰 service/reader，而不是制造另一个 facade 神对象。

**文件**

- Modify: `src-tauri/src/application/routing.rs`
- Create only when caller migration needs it: `routing_policy_control_plane.rs`、`routing_execution_reader.rs`、`routing_diagnostics_reader.rs`
- Modify: `src-tauri/src/application/command_facades/routing.rs`
- Modify: corresponding commands and focused tests

**步骤**

1. 先迁移模型映射的五个纯转发方法。command facade 直接依赖已有 `application::model_mapping` owner 或其窄 service；删除 `RoutingService` 中无路由算法的 model-mapping wrapper。
2. 迁移 policy/document 方法到 Task 1 coordinator 后，删除 `RoutingService` 中同名 public mutation facade。
3. 将 proxy execution 必需读取迁移到 Task 2 reader 后，删除 `RoutingService` 面向 proxy 的 planning/target/probe wrapper。
4. 将 workspace、operation detail、recent decision 和 health status 保持为 read side；仅当一个 command 不再需要 `RoutingService` 时，才提取为 `RoutingDiagnosticsReader`。它必须调用现有 query/projector，不能重新计算候选、分数或 health。
5. endpoint ping 保持 command facade 的 orchestration：读取 probe target、执行 outbound、记录结果。不要把 outbound I/O 搬入 query service 或 routing planner。
6. 仅在原 `RoutingService` 所有 caller 被迁移且可删除时，决定其最终名称/文件布局；在此之前不做纯机械 `routing.rs` -> `routing/mod.rs` 大搬家。

**当前完成条件：** proxy planning、target/probe 读取和 policy mutation 已有窄入口；剩余 `RoutingService` caller 已登记，不新增依赖面。

**后续完成条件：** 只有在每一组 caller 已迁移且旧方法可删除时，才移除对应 model-mapping/policy/proxy wrapper；最终每项剩余公开方法都能用一行说明其读/写边界和生命周期。若不能删除旧方法，则保持现状并记录原因，不做机械搬家。

### Task 4：拆分 process runtime state 的内部生命周期（已完成）

**目标：** 不改变运行时行为，只让状态的存活时间和 owner 在结构上可见。

**文件**

- Create: `src-tauri/src/services/proxy/routing_runtime/{capacity_retry,activity,diagnostics}.rs`
- Modify: `src-tauri/src/services/proxy/routing_runtime.rs` 或转换为 module root
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Update: capacity lease, active request, trace eviction and shutdown tests

**步骤**

1. 将现有 `CapacityRetryRegistry` 及其 lease/waiter 迁移到 `capacity_retry`；保持当前 API、RAII release 和 failure-domain 语义。
2. 将 station/key 活动计数及 `RoutingRuntimeActivity` 实现迁移到 `activity`；它只反映 proxy process 内活跃请求，不能成为 durable health 来源。
3. 将 bounded decision trace、classification metrics 与 diagnostic memory budget 迁移到 `diagnostics`；它们必须明确可淘汰、可随 proxy stop 消失。
4. 保留小型 `RoutingProcessState`（可暂时以 `RoutingRuntimeState` 名称暴露）作为组合根，唯一职责为创建、持有并在 proxy stop 时释放以上三个 state。不得在此阶段修改 trace schema、容量阈值或统计算法。
5. 与 timeout hot reload 对齐：transport policy store 属于 `ProxyRuntimeState` 的配置激活生命周期，不塞进 `RoutingProcessState`。

**完成条件：** 任意字段可以明确归属 capacity、activity 或 diagnostics；停止代理的释放语义不变；没有把 runtime-only state 写入 SQLite 的新增路径。

**实际结果：** `routing_runtime.rs` 现在是组合根，三个子模块分别拥有对应状态；focused tests 覆盖 lease/drop、活动计数、trace/metrics bounded 行为。

### Task 5：收口 read/command 生命周期与删除门禁（部分完成）

**目标：** 防止被拆出的 owner 又被宽 facade 或后台任务绕回去。

**文件**

- Modify: `src-tauri/src/application/command_facades/routing.rs`
- Modify: `src-tauri/src/app_composition.rs`
- Modify: `scripts/routing-single-owner.test.mjs`
- Modify: `scripts/routing-projection-runner.test.mjs` only if its current assertion becomes stale
- Update: relevant command and architecture tests

**步骤**

1. command facade 构造函数显式接收 policy coordinator、execution reader、diagnostics reader、proxy runtime 和 endpoint outbound dependency；禁止重新注入完整 `RoutingService` 作为万能依赖。
2. read command 只调用 read owner；write command 只调用 policy/health mutation owner；proxy lifecycle command 只调用 runtime owner。
3. 扩展 architecture gate：禁止 proxy repository import `RoutingService`；禁止 policy document runner 直接使用 `PersistenceHandle` 写 routing policy；禁止执行边界出现 deadline magic string；禁止将 model mapping wrapper 加回 `RoutingService`。
4. 删除所有已迁移的 wrapper、旧 import、死 test helper 和临时 compatibility alias。架构脚本只校验生产代码，测试 fixture 可以使用显式 fake，不得反向决定生产依赖。

**完成条件：** 路由写、执行读、诊断读、process state 和后台文档同步均有一个可追溯 owner；architecture gate 防止已删除旁路重现。

**当前结果：** policy write、execution read、runtime state 和 document runner 已有可追溯 owner；read/model-mapping/endpoint caller 仍通过 `RoutingCommandFacade` 过渡复用 `RoutingService`，因此本任务不宣称全部完成。

## 5. 迁移风险与控制

| 风险 | 控制措施 |
| --- | --- |
| 提取 service 时改变 query 结果或候选顺序 | 每一步先保留相同 projector/store 调用，使用 workspace/planning snapshot fixture 比较结果；禁止同时改算法。 |
| reader trait 默认值掩盖缺失行为 | 生产 trait 方法全部 required；测试用 builder 显式声明，默认 fail closed。 |
| document watcher 与 UI 保存行为分叉 | watcher 只调用 coordinator；CAS/revision/activation 用同一回归矩阵。 |
| start/stop 与 policy publish 竞态 | 采用 timeout 热加载计划中的长期 store、activation gate 和 revision fence，并增加交错测试。 |
| 拆 runtime state 破坏 lease/drop | 只移动现有类型与 RAII 所有权，先通过 capacity cancellation/shutdown fault tests。 |
| 过度重构请求终态 | `request_finalization` 与 `request_lifecycle` 列为明确禁区；若发现必须改动，停止并另建设计。 |

## 5.1 Endpoint 探测的事实与周期边界

这一节是本次计划新增的最小范围。它解决的是“谁在什么时候做什么”和“写回的状态代表什么”，不是重新设计探测协议或路由健康算法。

| 入口 | 当前触发 | 允许产生的事实 | 目标 owner | 不得混入 |
| --- | --- | --- | --- | --- |
| 手动 endpoint ping | 用户在路由/站点页面点击一次 | 一次 endpoint snapshot（带 `station_id`、`endpoint_revision`、状态、延迟和摘要） | `RoutingCommandFacade` + `RoutingEndpointTargetReadPort` / `RoutingEndpointHealthWritePort` | 不触发请求重试策略，不直接改变 durable protection reducer |
| 站点密钥 connectivity | 用户发起一次 key/model 连通性操作；模型请求与 endpoint ping 并行 | 一条 station-key diagnostic observation；并可独立更新 endpoint snapshot | `StationKeyConnectivityCommandFacade` + `RoutingStationKeyDiagnosticWritePort` / endpoint health port | 不把 diagnostic 当作真实 proxy traffic，不与 endpoint snapshot 合并成一个状态 |
| 周期 monitoring runner | monitor due 时间或手动排队；执行内按 monitor plan 收集 endpoint ping | monitor execution 事实 + endpoint snapshot | `MonitoringRunner` 自己编排，依赖窄 port | 不依赖完整 `RoutingService`，不把 monitor 周期改成 request 周期 |
| proxy request | 每个请求的 attempt/retry/replan | request observation、durable outcome 和 protection reducer 输入 | 现有 request finalization / health protection owner | 不调用 endpoint snapshot writer 作为请求失败的快捷写入口 |

三个 endpoint 入口可以复用同一个无状态 `endpoint_ping` outbound adapter，但不能复用一个“总重试次数”或共享一个调度器：HEAD -> GET 是协议 fallback，不能计入用户配置的 upstream retry；monitor 周期也不是 proxy request 的 retry 周期。

## 5.2 Task 6：收口 endpoint orchestration 与监控 runner 依赖

**目标：** 让 endpoint target 读取、endpoint snapshot 写入和 station-key diagnostic 写入成为可注入的窄能力，同时保留三个 caller 的独立生命周期。

**前置：** Task 0 的 caller inventory 已标记三类入口；endpoint revision fence 和 `endpoint_health_snapshot` 的现有 schema/行为已有 focused regression。

**文件**

- Keep/Create: `src-tauri/src/application/routing_endpoint_ports.rs`
- Modify: `src-tauri/src/application/mod.rs`（导出 port 模块）
- Modify: `src-tauri/src/application/command_facades/routing.rs`
- Modify: `src-tauri/src/application/command_facades/station_key_connectivity.rs`
- Modify: `src-tauri/src/services/monitoring/runner.rs`
- Modify: `src-tauri/src/app_composition.rs`、`src-tauri/src/application/app_services.rs`
- Modify only after caller migration: `src-tauri/src/application/routing.rs`
- Update: `scripts/routing-single-owner.test.mjs`、endpoint/monitoring focused tests

**步骤**

1. 将 `RoutingEndpointTargetReadPort`、`RoutingEndpointHealthWritePort` 和 `RoutingStationKeyDiagnosticWritePort` 作为 application port 正式纳入模块；port 只表达数据契约和 revision fence，不包含 HTTP、SQLx、调度器或 UI DTO。
2. 先让现有 application 实现以适配器方式实现这三个 port，确保 `expected_endpoint_revision` 不匹配时返回 `ApplicationError::StaleRevision`，且绝不覆盖新 endpoint 的 snapshot。此阶段不改变 SQL 和返回 DTO。
3. 将 `RoutingCommandFacade::ping_station_endpoint` 迁移为“读 target -> 无状态 outbound probe -> 写 snapshot”的显式流程。写回错误必须保留 `StaleRevision`、`Unavailable` 等稳定分类；只有确实无法判断提交结果时才映射为 `ResultUnknown`，不能把所有写失败折叠成同一结果。
4. 将 `StationKeyConnectivityCommandFacade` 的两个写入口迁移到对应 port。模型连通性结果仍是 key diagnostic；并行的 endpoint ping 仍只写 endpoint snapshot。不要让 diagnostic 写入改变 endpoint snapshot 的状态转换规则。
5. 将 `MonitoringRunner` 从 `Arc<RoutingService>` 改为 endpoint target/read 和 health write 的窄依赖。`prepare_execution` 在 monitor 执行开始时固定 target/revision；执行结束后的写回只携带该 revision，旧 probe 结果只能被拒绝并记录可诊断结果。
6. 明确 `endpoint_ping` 的时间预算：HEAD -> GET 是协议 fallback；后续如需修复总预算，使用同一个调用级 deadline 传给两个请求，保证整个 probe 不超过配置上限。此项单独加测试，不借机引入 upstream retry/backoff。
7. caller 全部迁移并有行为测试后，删除 `RoutingService` 的 endpoint target/health/diagnostic wrapper；若仍有生产 caller，保留 wrapper 并在台账中写明 caller，不做机械删改。
8. 增加 architecture gate：monitoring runner 不得 import/持有 `RoutingService`；endpoint snapshot writer 必须经过唯一 port/owner；所有 endpoint health write 必须带 revision fence；station-key diagnostic 不得被标记为 proxy traffic health。

**Task 6 验证顺序**

1. 先运行 `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib endpoint_ping -- --nocapture`，覆盖 HEAD 成功、HTTP 失败、GET fallback、取消和总 deadline。
2. 运行 `cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_write_path -- --nocapture`，确认 stale endpoint revision 拒绝写回且不会覆盖新 snapshot。
3. 运行 `cargo test --locked --manifest-path src-tauri/Cargo.toml --test station_key_health_transitions -- --nocapture`，确认手动 key connectivity 仍是 diagnostic-only，endpoint revision 变化会隔离旧事实。
4. 运行 `cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_execution_integration -- --nocapture`，确认 monitor execution 的 prepared revision、取消和 writeback 行为不变。
5. 最后运行 `node scripts/routing-single-owner.test.mjs` 及新增的 endpoint ownership 断言；只检查生产路径，测试 fake 不得反向扩大生产依赖。

**完成条件**

- 三类 caller 的触发周期、取消点、事实类型和写入 owner 在代码中可追溯；没有通过共享可变字段或隐式 callback 互相触发。
- `MonitoringRunner` 不再依赖完整 `RoutingService`；手动 ping 和 station-key connectivity 仍由各自 command facade 编排。
- stale endpoint probe 不覆盖新 revision；写回错误不再无差别变成 `ResultUnknown`。
- endpoint fallback 的总 deadline、取消行为和敏感错误摘要有 focused regression；未改变请求 retry、health reducer 或 routing score。

## 5.3 只清理已证实的技术债

以下项目只有在对应 caller 迁移或 focused test 能证明行为时才处理：

| 技术债 | 最小处理 | 明确不做 |
| --- | --- | --- |
| `RoutingCommandFacade` 把所有 endpoint 写失败映射成 `ResultUnknown` | 保留 `ApplicationError` 分类，仅对 commit outcome unknown 做未知结果 | 不新增一套 command error hierarchy，不改 IPC 协议字段 |
| endpoint ping 两次请求各自拥有 timeout | 将“协议 fallback”与“upstream retry”命名和预算分开；必要时共享 probe deadline | 不把 HEAD/GET fallback 计入 retry 次数，不引入全局重试器 |
| `short_ping_error` 只截断字符串 | 先覆盖 URL、认证头和超长错误的回归；若 outbound error 已脱敏则只保留截断 | 不在本计划里重写全局日志/错误脱敏系统 |
| runner、manual ping、key connectivity 重复构造 checked_at 和错误摘要 | 仅提取无状态格式化/时间注入 helper，调用方仍负责事实类型 | 不建立跨周期 event bus 或“统一探测服务” |
| `RoutingService` endpoint wrapper | caller 迁移后删除；迁移前保留并标记过渡 owner | 不按行数拆 `routing.rs`，不为改名而改名 |

每个技术债都必须满足“有明确旧代码、有单一替代 owner、有回归测试、有可删除的旧路径”四项条件；缺一项就只记录，不实施。

## 6. 验证与完成定义

每个任务先跑相关 Rust focused tests 和对应 architecture script。完成全部任务后最低运行：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib endpoint_ping -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_write_path -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test station_key_health_transitions -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_execution_integration -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture
node scripts/routing-single-owner.test.mjs
node scripts/routing-projection-runner.test.mjs
node scripts/request-lifecycle-architecture.test.mjs
pnpm.cmd verify:fast
```

本轮实施已验证的条件：

1. policy mutation 无论来自 UI、文件或历史恢复，均经单一控制面并具有一致 activation 语义；启动时从已持久化 policy 恢复 protection runtime 的 hydration bridge 仍单独存在并已登记。
2. proxy execution 不再依赖 `RoutingService` 或字符串 deadline；所有生产 capability 显式实现。
3. `RoutingService` 的已迁移 policy/proxy 入口已通过 coordinator/reader 收口；尚未迁移的 model mapping、query 和 endpoint caller 明确保留为过渡实现，没有伪装成已拆空。
4. capacity、activity、diagnostics、transport policy、durable finalization 的生命周期各自可说明，且没有改变既有请求安全语义。
5. 本轮列出的 architecture/focused tests 已通过；`pnpm.cmd verify:fast` 若受正在运行的桌面进程文件锁影响，必须在交付中如实记录，不能替代性声称通过。

后续阶段的完成条件：

1. `RoutingCommandFacade` 不再为已迁移职责持有完整 `RoutingService`，且旧 wrapper 已删除。
2. 剩余 query、model mapping、endpoint orchestration 各自有明确 owner；没有为了“统一”重新引入万能 facade。
3. 每次迁移都有 caller inventory、focused behavior test 和 architecture gate；若任一项缺失则暂停迁移。
