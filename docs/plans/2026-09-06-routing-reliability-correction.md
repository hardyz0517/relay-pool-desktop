# 路由可靠性修正执行计划

状态：Executed through the safe Phase 1–5 boundary（Phase 3 remaining adapter migration and Phase 6 qualification remain gated）

日期：2026-09-06

适用范围：`RoutingService` 过渡 owner、endpoint 探测与监控写回、路由保护只读投影、V3 旧链兼容代码的安全清理，以及这些边界的自动化门禁。

不在范围：重新设计 V3 评分公式、重试算法、请求双终态、模型映射语义、transport timeout 数值、真实 Provider 兼容性、P6 schema DROP 和 UI 视觉改版。

关联入口：

- [`../README.md`](../README.md)
- [`2026-08-23-routing-ownership-lifecycle-cleanup.md`](2026-08-23-routing-ownership-lifecycle-cleanup.md)
- [`2026-09-04-routing-half-open-eligibility-reliability.md`](2026-09-04-routing-half-open-eligibility-reliability.md)
- [`../audits/2026-08-31-routing-v3-legacy-chain-retirement-implementation.md`](../audits/2026-08-31-routing-v3-legacy-chain-retirement-implementation.md)
- [`../audits/routing-v3-legacy-retirement-ledger.json`](../audits/routing-v3-legacy-retirement-ledger.json)
- [`../specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md`](../specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md)

## 1. 执行结论

本计划已按安全边界执行至 Phase 5 的可验证范围。已完成 endpoint/read 窄 port、revision-fenced 写回、monitoring caller 迁移、最小 V3 protection 收口、空壳与 routing test-only health 清理及架构门禁；`verify:fast` 与在隔离 D 盘、`CARGO_BUILD_JOBS=1` 下运行的 `verify:full` 均已通过。以下阶段仍不能凭本轮本地验证宣称完成：`RoutingService` 剩余宽 adapter 的逐 caller 拆除、仍被集成测试直接引用的 health projector/runtime port 迁移、station-key diagnostic durable owner、P6 七天运行窗口/soak/backup-restore/桌面验收及 schema DROP。实现证据见 [`../audits/2026-09-06-routing-reliability-correction-implementation.md`](../audits/2026-09-06-routing-reliability-correction-implementation.md)。

本计划不是“把 `routing.rs` 拆成多个文件”的重构任务，而是一次以不变量和证据为中心的可靠性修正。核心顺序固定为：

1. 建立可回放的行为基线和完整 caller inventory。
2. 先接入窄 port 与 revision fence，再迁移生产 caller。
3. 迁移完成后删除旧 wrapper；没有删除证据就不移动或改名。
4. 对无生产 producer 的 projection 分支做事实确认，再决定删除或保留兼容解码。
5. 最后清理 test-only 遗骸和空壳模块；旧 schema 只按 P6 资格单独处理。

任何阶段只要出现行为差异、旧 writer 复活、stale revision 被接受、生产路径重新依赖完整 `RoutingService`，立即停止该阶段，不以补丁或默认值掩盖问题。

## 2. 当前事实与可靠性问题

### 2.1 最大结构风险：过渡性万能 owner

`src-tauri/src/application/routing.rs` 当前约 2,271 行，`RoutingService` 同时承载：

- policy/CAS 和 managed document reconcile；
- V3 circuit admission、persistence gate 和 generation guard；
- execution planning 和 target resolution 适配；
- workspace/runtime overlay 查询；
- 候选、定价、simulation 查询；
- endpoint probe target 读取和 endpoint health 写回；
- 少量兼容性包装方法。

已有的新 owner：

- `RoutingPolicyMutationCoordinator`：policy mutation 及 activation 协调；
- `RoutingExecutionReadPort` / `RoutingExecutionReader`：proxy execution 窄读入口；
- `RoutingDiagnosticsReader`：持久化 diagnostics read owner；
- `RoutingPolicyReadService`、`ModelMappingService`：部分独立 read/mapping owner。

但生产依赖仍未完全收口：

- `RoutingCommandFacade` 仍直接持有 `Arc<RoutingService>`；
- `RoutingExecutionReader` 只是完整 service 的适配器；
- `MonitoringRunner` 仍持有完整 `RoutingService`；
- `StationKeyConnectivityCommandFacade` 仍直接调用 endpoint health writer；
- policy coordinator 暂时复用 `RoutingService` 的 policy aggregate。

这会造成两个可靠性问题：

1. 同一个事实可能从 query、command、monitoring、proxy 不同路径进入，难以证明 owner 唯一。
2. 迁移新 owner 时容易留下“看起来已拆分、实际仍绕回旧 service”的旁路。

### 2.2 endpoint 写回的竞态风险

手动 ping、station-key connectivity、周期 monitoring 都会产生 endpoint probe，但当前写回入口分散。探测开始时读取的 endpoint revision 可能已经过期；如果写回没有稳定的 revision fence，旧 probe 会覆盖新 endpoint snapshot。

三个入口还具有不同生命周期：

- 手动 ping：一次性用户命令；
- station-key connectivity：key/model diagnostic，可并行 endpoint ping；
- monitoring runner：有调度、取消、并发和执行记录的后台任务。

它们可以共享无状态 outbound probe adapter，但不能共享 retry 预算、调度器或事实类型。

### 2.3 protection projection 存在待确认的死分支

`src-tauri/src/application/queries/routing_protection.rs` 仍有 `CapacityProtectionFact`、`capacity_entry` 和 `runtime_capacity_available` 投影分支。当前生产 facade 调用固定传入空 capacity 列表和 `true`，因此这条分支没有已确认的生产 producer；不能仅凭测试存在就继续把它视为 runtime authority。

该文件整体仍是 V3 circuit-backed compatibility command，不能整块删除。应先确认 producer 和 DTO 契约，再最小化删除无生产事实的分支。

### 2.4 test-only health 遗骸和空壳模块

以下内容主要只在测试编译或测试引用：

- `application/routing_engine/routing_health.rs`；
- `models/routing.rs` 中 `StationKeyHealth` 与 `CanonicalRoutingCandidate.health`；
- `application/operational_facts/health_projector.rs`；
- `application/operational_facts/runtime_health_port.rs`；
- `models/operational/health.rs` 及其 `cfg(test)` 导出。

`services/health/mod.rs`、`services/routing/mod.rs`、`services/stations/mod.rs`、`services/logs/mod.rs` 目前只有“later phase”注释。它们维护成本低但没有业务价值，必须在确认无 import 后清理，不能把它们误当成可靠性修正的主战场。

### 2.5 旧 schema 的兼容窗口不能提前结束

`routing_health_snapshot`、`station_key_health_observations`、`routing_error_rate_history`、`station_capacity_domains` 已不再是 V3 runtime authority/writer，但仍被 migration、portable schema、legacy import/upgrade、delete cleanup 和 fixture 使用。

V3 旧链审计已明确 P6 schema DROP 为 no-go。该部分只做兼容矩阵和资格准备，不在本计划中删除表或创建 DROP migration。

## 3. 可靠性不变量

所有实现和测试必须围绕以下不变量，不得只以“代码更短”作为完成标准。

### I1. 单一事实 owner

- policy 的持久化与激活只经过 `RoutingPolicyMutationCoordinator`；
- proxy execution 只依赖 `RoutingExecutionReadPort`；
- endpoint snapshot 只有一个写 owner；
- station-key diagnostic 与 endpoint snapshot 是不同事实，不能互相冒充；
- runtime-only 状态不能成为 durable circuit 或 health 来源。

### I2. revision fence

所有 endpoint health write 必须携带 `expected_endpoint_revision`（或等价的强类型 revision）：

- revision 不匹配时返回 `ApplicationError::StaleRevision`；
- stale write 不能覆盖新 snapshot；
- 调用方必须能区分 stale、不可用和 commit outcome unknown；
- 写入成功后返回的事实必须包含实际提交 revision。

### I3. fail-closed

- circuit persistence/read model 不可用时，proxy admission 不得放行；
- port 未实现、依赖缺失或输入 revision 不合法时，不得默认空列表、默认 settings 或 `None`；
- 只有无法判断提交结果时才使用 `ResultUnknown`，不能把所有错误折叠成未知。

### I4. 调用级 deadline 和取消传播

- endpoint HEAD -> GET 是协议 fallback，不计入 upstream retry 次数；
- 一个 probe 使用同一个调用级 deadline，整个 probe 不得超过预算；
- monitoring、manual ping、connectivity 取消都必须传递到 outbound 和写回路径；
- 已取消或已过期的 probe 不得无条件提交结果。

### I5. 事实类型隔离

- proxy request observation、monitoring execution fact、endpoint snapshot、station-key diagnostic 分开建模；
- 不把 endpoint snapshot 当作 proxy traffic health；
- 不把 runtime capacity projection 当作 V3 circuit state；
- 不重新引入旧 error-rate、scoped health 或 capacity-domain identity。

### I6. 兼容性可追溯

- 旧 schema 只能出现在白名单兼容路径；
- V1/V2 decoder 字段只能解码、验证和投影，不能回流到 V3 execution；
- 每一项保留的兼容代码都要有 owner、删除条件、验证命令和预计阶段。

## 4. 分阶段执行计划

### Phase 0：冻结基线与 caller inventory

目标：在任何代码迁移前，建立可比较的行为基线和完整依赖清单。

#### 工作项

1. 记录工作区状态，保留用户已有改动；执行计划实施时不得覆盖无关修改。
2. 为 `RoutingService` 每个生产可见方法建立清单，字段至少包括：
   - 方法名和文件位置；
   - caller（facade、background task、proxy、test）；
   - 读/写性质；
   - 事实类型和生命周期；
   - 目标 owner；
   - 可删除条件；
   - 现有 focused test。
3. 生成 caller 分类：
   - `P`：policy mutation/activation；
   - `E`：proxy execution read/admission；
   - `Q`：query/workspace/diagnostics read；
   - `X`：endpoint probe target/write；
   - `T`：test-only compatibility。
4. 建立 workspace、runtime overlay、simulation、endpoint snapshot、protection status 的 golden fixture，记录排序、revision、状态和错误分类。
5. 对旧表做只读引用扫描，确认每个引用属于 migration/import/upgrade/schema/delete-cleanup/fixture 白名单。

#### 产物

- `docs/audits/` 中的 caller inventory（建议 JSON + 人读表格）；
- golden fixture 与结果摘要；
- 旧 schema 引用白名单；
- 风险清单和暂停条件。

#### 通过门槛

- 每个 `RoutingService` public/`pub(crate)` 方法都有 caller 归属；
- 没有未分类的生产 import；
- golden fixture 可由测试重复生成；
- `node scripts/routing-v3-legacy-retirement.test.mjs` 通过。

### Phase 1：建立窄 port 和强类型边界

目标：先建立可注入、可替换、可测试的 endpoint 能力，不改变现有行为。

#### 文件范围

- 新增 `src-tauri/src/application/routing_endpoint_ports.rs`；
- 修改 `src-tauri/src/application/mod.rs`；
- 修改 `src-tauri/src/application/routing.rs`，仅增加适配实现；
- 新增或扩展 endpoint/monitoring focused tests。

#### 设计要求

建议至少提供三类能力（名称可在实现时按现有术语调整）：

1. `RoutingEndpointTargetReadPort`
   - 输入：station id；
   - 输出：包含 `station_id`、`endpoint_revision`、已清洗 origin 的 probe target；
   - 不暴露 credential、SQLx connection 或 UI DTO。
2. `RoutingEndpointHealthWritePort`
   - 输入：带 expected revision 的 endpoint snapshot write command；
   - 输出：实际写入的 snapshot/revision；
   - 明确返回 stale、unavailable、commit unknown。
3. `RoutingStationKeyDiagnosticWritePort`
   - 输入：station-key diagnostic observation；
   - 输出：diagnostic receipt；
   - 禁止把结果标记成 proxy traffic health。

port 的约束：

- 只包含应用层数据契约和稳定错误；
- 不包含 HTTP、SQLx、调度器、重试器或 Tauri DTO；
- 生产 trait 方法必须是 required method；
- 测试 fake 默认 fail closed，必须显式声明允许的行为；
- 不通过 `Box<dyn Any>`、字符串状态或隐式 callback 绕过类型检查。

#### 适配策略

- 先用现有 `RoutingService`/`RoutingStore` 实现 adapter，确保行为不变；
- adapter 内部统一做 revision fence、输入校验和错误映射；
- 不在本阶段删除 `RoutingService` wrapper；
- 为每个 port 添加编译期和架构脚本断言，防止重新依赖宽 service。

#### 通过门槛

- stale revision focused test 证明旧写入不会覆盖新 snapshot；
- port fake 能注入 runtime unavailable、commit unknown、cancelled 和 stale 四类故障；
- `cargo check --locked` 和相关测试通过；
- golden fixture 结果不变。

### Phase 2：迁移 endpoint caller 和 monitoring runner

目标：先收口最容易产生竞态和生命周期混淆的 endpoint 读写路径。

#### 2.1 手动 endpoint ping

将 `RoutingCommandFacade::ping_station_endpoint` 固定为以下流程：

```text
read target + capture revision
    -> stateless outbound probe with one call deadline
    -> write endpoint snapshot with expected revision
    -> map stale/unavailable/unknown without collapsing errors
```

要求：

- HEAD -> GET fallback 仍由 outbound adapter 负责；
- fallback 不消费 upstream retry budget；
- probe 取消后不发起新的写回；
- endpoint origin、认证头和完整错误内容不能进入日志或 DTO；
- `ResultUnknown` 只表示提交结果无法判定。

#### 2.2 station-key connectivity

- key/model connectivity 的结果继续写 station-key diagnostic；
- endpoint ping 若并行执行，只写 endpoint snapshot；
- 两类事实的时间戳、状态枚举、revision 和错误摘要分开；
- endpoint stale 不得改变 key diagnostic 的成功/失败语义。

#### 2.3 monitoring runner

将 `MonitoringRunner` 从 `Arc<RoutingService>` 改为显式窄依赖：

- `RoutingEndpointTargetReadPort`；
- `RoutingEndpointHealthWritePort`；
- 现有 `MonitoringService`、credential 和 outbound 依赖。

`prepare_execution` 时固定目标和 revision；执行结束只用该 revision 写回。过期 probe 必须被拒绝并产生低基数诊断，不得覆盖新 snapshot。

#### 2.4 composition root

在 `app_services.rs` / `app_composition.rs` 组装 adapter：

- command facade、monitoring runner 和 connectivity facade 注入 port；
- 禁止在 composition root 为方便继续传递完整 `RoutingService`；
- 生产对象图中只保留必要的宽 service 引用，且在 caller inventory 中登记。

#### 通过门槛

- `MonitoringRunner` 生产代码不再 import/持有 `RoutingService`；
- endpoint target read、snapshot write 各只有一个生产 owner；
- stale、取消、deadline、并发写入和 commit unknown 测试通过；
- `scripts/routing-single-owner.test.mjs` 增加并通过 endpoint ownership 断言。

### Phase 3：迁移剩余 query/read caller，缩小 `RoutingService`

目标：按 caller 和删除条件移除过渡职责，不进行机械文件搬家。

#### 迁移顺序

1. **workspace read**
   - 将 `load_routing_workspace_snapshot` 及其依赖的纯 query/projector 迁移到明确的 workspace read owner；
   - 保持 V3 circuit、capability、balance、pricing 和排序语义不变；
   - 不在新 reader 中重新计算候选或 health。
2. **runtime overlay read**
   - 只读取 proxy runtime activity 和已持久化事实；
   - 明确 runtime-only 字段不会写入 SQLite，也不会成为 durable health 来源；
   - 保持窄窗口、空状态、runtime unavailable 的现有 DTO 语义。
3. **simulation**
   - 先标记为 query/planning caller；
   - 只有在 workspace/query 迁移后确认其依赖边界，才决定是否独立 owner；
   - 不因为文件大小而创建第二个万能 facade。
4. **policy aggregate**
   - coordinator 继续是唯一 mutation owner；
   - 在所有 policy caller 使用 coordinator 后，再将 `RoutingService` 的 policy aggregate 改为内部实现或窄 persistence port；
   - 不改变 CAS、activation、document mirror 和 generation 不变量。
5. **circuit/reaper bridge**
   - `CircuitPersistenceGate`、reaper 和 durable circuit read 保持独立 owner；
   - 只有当 caller inventory 证明不再需要宽 service 时，才抽取其 persistence/health-check wrapper。

#### 每个 caller 迁移的固定步骤

1. 添加目标 owner 的行为等价实现；
2. 把一个生产 caller 切换到目标 owner；
3. 增加/更新 focused regression 和 architecture gate；
4. 全量搜索旧方法 caller；
5. 只有搜索结果为空，才删除旧 wrapper、import 和兼容 alias；
6. 更新 caller inventory、删除台账和变更说明。

#### 通过门槛

- `RoutingCommandFacade` 不再为已迁移职责持有完整 `RoutingService`；
- 每个剩余 public 方法有一行 owner/lifecycle 说明；
- 旧 wrapper 删除后，编译器和架构脚本均能证明没有旁路；
- workspace、overlay、simulation golden fixture 与基线一致。

### Phase 4：处理 protection projection 的死分支

目标：在不误删当前 V3-backed command 的前提下，确认 runtime capacity 分支是否仍有生产事实来源。

#### 证据收集

1. 全仓搜索 `CapacityProtectionFact` 的生产构造点；
2. 区分 production、integration test、unit test、fixture 和历史 decoder；
3. 为 `get_routing_protection_status` 记录真实输入来源和前端字段消费；
4. 确认 `runtime_capacity_available` 是否代表真实 registry 能力，还是历史兼容参数。

#### 分支决策

- 若存在受支持的生产 producer：保留分支，给 producer 和 write/read owner 补齐接口、revision/availability 语义和 focused test。
- 若没有生产 producer：
  1. 先把 production call path 改成显式 V3 circuit-only 输入；
  2. 保留必要的反序列化/兼容 DTO，不让 capacity projection 回到 runtime authority；
  3. 迁移测试到 V3 circuit/capacity-unavailable 语义；
  4. 删除 `CapacityProtectionFact`、`capacity_entry` 和无 caller 参数；
  5. 运行前端 contract、generated binding 和 command regression。

禁止：

- 直接删除整个 `routing_protection.rs`；
- 仅因为 unit test 仍覆盖就保留未使用生产分支；
- 将 runtime capacity 重新接回 durable circuit 或旧 error-rate 链。

#### 通过门槛

- production caller inventory 中 capacity projection 有明确结论；
- protection command 的前端行为、Unavailable 状态和 timeout facts 不变；
- 旧 health/error-rate/capacity 表没有新增 writer。

### Phase 5：清理 test-only 遗骸和空壳模块

目标：降低维护噪音和概念混淆，不触碰仍有兼容职责的 `health_protection.rs`。

#### test-only health 遗骸

在 V3 行为测试已经覆盖以下语义后再删：

- 冷却结束后的 conditional eligibility；
- Half-Open lease、竞争、取消、deadline 和 reaper；
- capability unsupported、circuit unavailable、capacity exhausted/state unavailable；
- endpoint diagnostic 与 proxy traffic health 隔离。

候选清理对象：

- `routing_engine/routing_health.rs`；
- `models/routing.rs` 的 `StationKeyHealth` 及 test-only candidate health 字段；
- `operational_facts/health_projector.rs`、`runtime_health_port.rs` 的纯测试投影；
- `models/operational/health.rs` 和仅为这些类型服务的 test-only re-export。

清理规则：

- 先迁移测试断言到当前 V3 read model；
- 再删除类型和模块；
- 最后删除 `#[cfg(test)]` import/re-export；
- 不删除 `application/health_protection.rs` 中仍被 V1/V2 decoder、policy config 或历史 evidence 使用的类型。

#### 空壳模块

确认无 import 后，删除或合并以下仅含 placeholder 注释的模块声明：

- `services/health/mod.rs`；
- `services/routing/mod.rs`；
- `services/stations/mod.rs`；
- `services/logs/mod.rs`。

若未来确实需要这些目录，必须先有 accepted spec、真实 owner 和 caller，再重新创建；不能以空模块预留架构名额。

#### 通过门槛

- 生产构建不再包含这些 test-only 类型；
- `rg` 无残留生产 import；
- Rust unit/integration tests、retirement gate 和前端 contract 通过；
- 清理没有改变生成 DTO 或持久化 schema。

### Phase 6：兼容窗口和 P6 资格准备

目标：为旧 schema 的最终退役准备证据，但不在本计划提前 DROP。

#### 兼容矩阵

为每个旧表记录：

- 当前 schema/migration 版本；
- 允许的 reader 类型；
- 是否允许 writer（当前应为否，delete cleanup 除外）；
- portable import/export 行为；
- backup/restore 行为；
- fixture 和 downgrade 依赖；
- 删除前置条件和 owner。

#### 资格步骤

1. 连续七天 V3-only 运行窗口；
2. 至少 1,000 个 routing observation/monitoring event 的 parity soak；
3. 备份 manifest 校验；
4. 隔离 restore rehearsal；
5. 桌面 acceptance（启动、路由工作区、保护状态、monitoring、导入/升级）；
6. 重新扫描当前 migration 编号、portable/import caller 和 release bundle；
7. 单独评审 append-only DROP migration 和 forward-only rollback floor。

#### 通过门槛

- P6 资格全部满足并有可复核证据；
- release go 单独批准；
- 未满足任何一项时，旧表继续保留，不创建 DROP migration。

## 5. 可维护、可拓展的设计约束

### 5.1 能力优先于具体 service 名称

调用方依赖“它需要什么能力”，不依赖“某个大 service 里面恰好有这个方法”。新增能力时：

- 先定义最小 port 和输入/输出事实；
- 明确同步/异步、取消、deadline 和错误分类；
- 由 composition root 注入实现；
- 为 production 和 test fake 各自写 contract test；
- 不把新能力追加到 `RoutingService` 作为默认方案。

### 5.2 事实与编排分离

- reader/projector 只读取和投影，不执行 outbound I/O；
- command facade 负责编排用户命令和错误映射；
- monitoring runner 负责调度、并发、取消和执行记录；
- persistence adapter 负责 SQL、CAS 和 revision fence；
- proxy execution 只通过窄 execution port 获取事实。

### 5.3 强类型状态和错误

禁止使用以下方式表达跨边界状态：

- magic string deadline；
- `Option` 充当“未实现/不可用/没有事实”三种状态；
- `String` 混合 stale、unavailable、unknown；
- 通过错误文案判断业务分支。

状态应使用已有 enum 或新增最小 enum；新增枚举必须同步 Rust、IPC DTO、TypeScript binding、前端展示和 contract test。

### 5.4 默认 fail closed，测试显式 opt-in

任何 port 或 fake 缺少行为时：

- production 返回稳定的 unavailable/internal error；
- test fake 默认拒绝，不返回空 Vec/默认 settings；
- 测试必须显式配置允许的状态和调用次数；
- 不添加为了“让旧测试通过”的隐式 fallback。

### 5.5 可观测性低基数且不泄密

所有新诊断事件只记录：

- owner、operation、status、reason code；
- station/key 的非敏感承诺或 bounded hash；
- revision、attempt boundary、duration bucket；
- 不记录 endpoint URL、认证头、API key、原始 provider error 或完整请求内容。

## 6. 测试与验证矩阵

### 6.1 单元测试

- port 输入校验、revision fence、错误映射；
- probe deadline 和取消传播；
- protection projection 的 circuit-only / capacity-unavailable 分支；
- endpoint snapshot 状态归一化和敏感错误截断；
- test fake 缺失能力时 fail closed。

### 6.2 并发和故障注入

至少覆盖：

1. 两个 probe 读取同一 revision，只有一个写入成功；
2. endpoint revision 在 probe 期间变更；
3. 写入返回 stale、runtime unavailable、database busy、commit outcome unknown；
4. monitoring 执行在 probe 前、probe 中、写回前取消；
5. runner shutdown 时没有遗留任务或未释放 permit；
6. reaper 与普通 request admission 交错；
7. policy activation 与 monitoring read 交错；
8. persistence read model unavailable 时 proxy fail closed。

### 6.3 跨层 contract

每次 Rust DTO/enum 变化必须验证：

- IPC registry 和 ACL；
- generated command registry/bindings；
- `BackendClient`、`DesktopBackend`、`DemoBackend`；
- routing query、状态展示和空/错误态；
- 旧命令和 capacity-domain/error-rate 表面没有重新暴露。

### 6.4 建议验证命令

按阶段执行，不把最后的全量命令当作唯一证据：

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
node scripts/routing-v3-legacy-retirement.test.mjs
pnpm.cmd verify:fast
pnpm.cmd build
```

如果命令因桌面进程文件锁、外部 provider、网络或环境问题未完成，必须记录实际未验证范围，不能以部分通过替代全量结论。

## 7. 发布和回滚策略

### 7.1 分阶段发布

- Phase 0/1：只增加 inventory、port、adapter 和测试，不改变生产事实语义；
- Phase 2：endpoint/monitoring caller 切换，可独立回滚到旧 adapter，但不得恢复旧 schema writer；
- Phase 3：按 caller 小批迁移，每批只删除一组 wrapper；
- Phase 4/5：projection 和 test-only 清理必须在独立变更中完成；
- Phase 6：只有独立 release go 后才评审 schema DROP。

### 7.2 回滚原则

- 保留 V3 read-side cutover 作为 rollback floor；
- 不恢复旧 health/error-rate/capacity runtime authority；
- port adapter 可以回退实现，但不能回退 owner 规则；
- 任何数据写入变更必须有 backup/restore 证据；
- 不使用破坏性 Git 命令回滚用户无关改动。

### 7.3 自动停止条件

出现以下任一情况时停止当前阶段并回到诊断：

- golden fixture 结果变化且无法解释；
- stale revision 写入成功或覆盖新 snapshot；
- `MonitoringRunner`、proxy 或 policy runner 重新依赖完整 `RoutingService`；
- 发现旧 health/error-rate/capacity runtime writer；
- `ResultUnknown` 数量异常上升或错误分类丢失；
- generated binding、ACL 或前端状态出现未登记变化；
- 任何测试依赖隐式空值 fallback 才能通过。

## 8. 风险登记

| 风险 | 影响 | 预防 | 发现信号 | 处理 |
| --- | --- | --- | --- | --- |
| 迁移时改变候选排序或保护语义 | 高 | golden fixture、V3 focused test、禁止同时改算法 | workspace/planner diff | 暂停并回滚该 caller，不继续拆分 |
| stale endpoint probe 覆盖新 revision | 高 | 强类型 revision fence、并发测试 | snapshot revision 回退 | 立即阻断 writer，保留新 snapshot |
| 新 port 变成另一层万能 facade | 中高 | 每个 port 只表达一个能力，架构 gate | port 方法数量和依赖增长 | 退回设计评审，拆分事实/编排 |
| 错误被统一折叠为 unknown | 中高 | typed error contract、commit outcome 专门映射 | unknown 比例上升 | 恢复稳定错误分类并补回归 |
| test-only 代码被误删导致兼容覆盖下降 | 中 | 先迁移测试到 V3，保留 decoder 证据 | contract/fixture 失败 | 恢复测试覆盖，不恢复生产旧语义 |
| 旧 schema 被提前 DROP | 极高 | P6 独立资格和 release go | backup/restore 或 portable 失败 | 禁止 migration，回到兼容窗口 |
| 监控、手动 ping、connectivity 周期互相耦合 | 中高 | 共享无状态 probe，独立 orchestrator | 取消/并发行为交叉失败 | 拆回独立生命周期，不引入 event bus |

## 9. 完成定义

本计划只有在以下条件全部满足时才可标记完成：

1. `RoutingService` 剩余方法都有明确 owner、caller、生命周期和删除条件；
2. `MonitoringRunner`、endpoint command 和 station-key connectivity 不再持有完整 `RoutingService`；
3. endpoint target read、snapshot write、station-key diagnostic write 各自只有一个生产 owner；
4. stale revision、取消、deadline、commit unknown 和 persistence unavailable 都有 focused regression；
5. proxy、policy、monitoring、query 的生命周期边界由架构脚本持续守护；
6. protection projection 的 capacity 分支有生产 producer 证据，或已安全删除；
7. test-only legacy health 模块和空壳 service 已清理，且 V3 行为测试仍完整；
8. 旧 schema 仍遵守 P6 no-go，或另有独立批准的 DROP 资格记录；
9. Rust、frontend、IPC、generated bindings、architecture gates 和 `verify:fast` 均有实际执行记录；
10. 文档、caller inventory、retirement ledger 和 release notes 与代码事实一致。

## 10. 交付物清单

- [x] caller inventory（人读 + 机器可读）；
- [x] `routing_endpoint_ports.rs` 及其 contract tests；
- [x] endpoint revision fence / typed error regression；
- [x] monitoring runner 窄依赖迁移；
- [x] command facade / workspace / runtime overlay caller 迁移记录（窄 read port 已接入，宽 adapter 仍保留）；
- [x] protection capacity 分支决策记录（无生产 `CapacityProtectionFact` producer；保留兼容投影，禁止回流 runtime authority）；
- [x] test-only health 和空壳模块清理记录（仍被集成测试直接引用的 health projector/runtime port 保留）；
- [ ] V3 legacy ledger 更新；
- [x] 验证命令、失败原因和未验证范围；
- [ ] 若涉及发布：backup/restore、soak、desktop acceptance 和 release go 证据。

本计划的成功标准是“每个可靠性不变量都有唯一 owner、可注入实现、可观测失败和自动化门禁”，而不是文件数量或代码行数下降。
