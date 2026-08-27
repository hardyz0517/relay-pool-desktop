# 路由事实来源与评估周期收口升级计划

状态：待执行（已修订）

日期：2026-08-27

适用范围：路由工作区、PlanningSnapshot、OperationalFactBundle、余额准入、倍率归一化、评分状态、revision fence、模型映射快照和相关前端 DTO。

关联入口：

- [`../README.md`](../README.md)
- [`../PRODUCT_MODEL.md`](../PRODUCT_MODEL.md)
- [`../PRICING_MULTIPLIER_MODEL.md`](../PRICING_MULTIPLIER_MODEL.md)
- [`2026-08-26-routing-workspace-planner-admission-alignment.md`](2026-08-26-routing-workspace-planner-admission-alignment.md)

## 1. 背景与问题边界

当前实现已经增加了 assessment、score status 和 workspace planner evaluation 等结构，但新旧链路尚未完全收口。主要风险不是评分公式本身，而是同一个 key 在不同读取器、不同时间点和不同兼容字段中被重复解释。

已确认的结构性问题：

1. planner 的候选 SQL 与 workspace 的 canonical candidate query 使用不同过滤条件和数量上限。
2. `ProbeDiscoveryOnly` 与普通评分候选共用 candidate cap。
3. planner 非 deadline 错误会被 workspace 降级成空评分表，根因丢失。
4. station scope 和 key scope 的余额选择逻辑分散在 SQL、runtime Rust 和 projector 中。
5. 生产路径没有复用测试中的 balance projector。
6. workspace 仍计算 `hard_rejection_codes`，前端仍从旧字段猜测评分状态。
7. score status 和 assessment provenance 在 IPC DTO 中仍是 optional。
8. revision vector 使用最大值压缩多个来源，兑换率和余额 freshness 未完整进入版本围栏。
9. 一次评估使用多个 `now`，mapping 从进程全局读取，可能与事务内事实不一致。
10. native multiplier、effective multiplier 和 compatibility cache 仍存在隐式 fallback。

## 2. 目标与非目标

### 2.1 目标

- planner 与 workspace 消费同一份 `OperationalFactBundle`。
- 资格判断只有一个 owner，workspace 和前端只投影 assessment。
- 余额 scope、数值、status、freshness 的冲突规则只有一套生产实现。
- 一次评估使用一个固定的 evaluation time、mapping snapshot 和 revision fence。
- `score == null` 永远由显式 `score_status` 解释。
- 兑换率、余额和映射修改能够可靠使旧评估失效。
- 旧兼容字段被隔离并建立删除条件，避免继续形成第二套业务逻辑。

### 2.2 非目标

- 不修改评分权重、评分公式或路由策略语义。
- 不放宽 credential、health、group、capability 或 balance 资格门。
- 不加入账号、支付、团队权限、云同步或插件市场能力。
- 不在本计划中做完整 UI 视觉改版。
- 不以真实 Provider 或发布安装包验证替代本地自动化测试。

## 3. 不可变工程约束

以下约束必须在实现和验收中保持不变：

1. 代理执行和 workspace 诊断使用相同的事实来源和资格核心。
2. 诊断 assessment 不得被加入 proxy 热路径的长期输入或缓存。
3. probe discovery 不是用户目标评分候选，不得生成评分或占用普通评分 cap。
4. `candidate_limit`、`probe_discovery`、`excluded`、`unavailable` 是不同状态，不能通过空 score 混淆。
5. 有限数值余额优先于文本 status；正余额不能因为 low threshold 被判定为 depleted。
6. `station_keys.rate_multiplier` 是兼容缓存，不是当前 effective multiplier 的默认来源。
7. read model、assessment、pricing、balance 和 mapping provenance 必须指向同一版本身份。
8. 非 deadline 错误必须保留稳定错误码和脱敏诊断，不能静默变成“无分”。

## 3.1 局部重构原则

本计划采用边界收口，不采用一次性重写。每个局部重构都必须有单一职责、输入输出契约和可回滚入口。

1. `persistence` 只负责读取原始事实和 revision，不负责 eligibility、score 或 UI 文案。
2. `OperationalFactBundle` 只表示一次 durable read 的结果，不做全局缓存，也不承载 runtime overlay。
3. Bundle 内部按 typed slice 分组：`CandidateFacts`、`BalanceObservations`、`PricingFacts`、`PolicyFacts`、`MappingFacts`、`HealthFacts`。禁止继续增加无归属的字段，避免形成新的 God object。
4. `assess_candidate` 只负责资格和理由；candidate cap、score、projection 分别由独立函数负责。
5. `RoutingWorkspace` 只做 assessment 到 DTO 的投影；旧 `hard_rejection_codes` 不能再反向影响评分状态。
6. runtime overlay 是 process-lifetime 数据，拥有自己的 capture time 和 revision，不写入 durable assessment revision。
7. 兼容逻辑只能存在于边界 adapter，不能在 production planner、workspace projector 和前端 view model 中各保留一份 fallback。
8. 每个新字段必须回答三个问题：事实来源是谁、有效周期是什么、失效由哪个 revision 触发。答不清楚的字段不得加入共享 DTO。

禁止的局部修复：

- 在前端用 `score != null`、空数组或旧 rejection code 猜业务状态；
- 在 SQL、Rust 和 TypeScript 各自再实现一次余额或倍率规则；
- 用 `Utc::now()`、`updated_at` 或 `generated_at` 冒充 durable revision；
- 为了通过测试把错误转换成 `None` 或默认值；
- 在没有 owner 和删除条件的情况下新增长期兼容字段。

## 3.2 Owner 与边界表

| 领域事实或决策 | 唯一 owner | 允许的下游 | 明确禁止 |
| --- | --- | --- | --- |
| executable candidate 集合 | `OperationalFactReader` + bundle assembler | planner、workspace report | workspace/前端重新筛选 credential 或 enabled |
| balance observation 选择 | `BalanceSelectionPolicy` | planner、runtime projection、workspace | SQL、UI 各自比较 value/status |
| native/effective multiplier | pricing projector | eligibility、score、pricing display | 直接读取 compatibility cache 参与资格 |
| mapping snapshot | application evaluation context | assessment、capability resolution | transaction 内重新读全局 mapping |
| eligibility 和 primary reason | `assess_candidate` | cap、score、workspace | workspace 重新计算 hard rejection |
| candidate cap | `apply_ordinary_cap` | PlanningSnapshot | probe candidate 共用普通 cap |
| score breakdown | `score_candidate` | PlanningSnapshot、workspace score details | 用 UI boolean 代替 score |
| runtime in-flight/cooldown | runtime overlay owner | 状态展示、执行协调 | 写入 durable assessment revision |
| DTO 兼容 | IPC/view-model adapter | 旧客户端或迁移测试 | 在业务组件中散落 fallback |

新增规则必须先选择上表中的 owner，再修改该 owner 的输入输出契约；不得从调用方旁路加入判断。

## 4. 目标架构

```text
EvaluationContext (one per evaluation)
  - evaluated_at_ms / deadline
  - request facts
  - policy snapshot/revision
  - mapping snapshot/revision
          |
          v
OperationalFactBundle (one durable read, no cache)
  - CandidateFacts
  - BalanceObservations
  - PricingFacts
  - Policy/Mapping/HealthFacts
  - FactRevisionVector
          |
          v
normalize_facts -> select_balance -> normalize_pricing
          |
          v
assess_candidate (eligibility only)
          |
          +--> apply_candidate_cap (ordinary only)
          |       |
          |       +--> score_candidate -> PlanningSnapshot
          |
          +--> RoutingWorkspaceSnapshot (assessment projection)
          +--> runtime overlay (separate process revision)
```

workspace 可以物化全部 assessment，但 proxy 只接收普通候选和必要的 probe coordination 输入，不接收诊断 Vec。`normalize_facts`、`select_balance` 和 `normalize_pricing` 必须是可独立测试的纯步骤，不能重新读取数据库。

## 5. 执行阶段

### Phase 0：建立基线和回归样本

**目标**：在改动前固定当前行为，防止重构后无法判断是修复还是语义漂移。

**工作内容**：

1. 为以下场景建立 Rust fixture 和 TypeScript fixture：
   - station depleted、key positive；
   - 前 1024 个 key 含大量 credentialless rows；
   - probe discovery rows 超过普通 candidate cap；
   - planner 构建失败；
   - 修改 `credit_per_cny` 后重新加载 workspace；
   - balance、mapping、pricing 在相邻读取间发生变化。
2. 记录每个场景的 candidate IDs、assessment、score status、reason、revision 和时间字段。
3. 建立 owner 删除台账，列出 SQL、runtime Rust、workspace、frontend 仍在解释资格的函数和组件。
4. 不修改生产逻辑，只提交测试和审计记录时，必须确保测试不包含真实 secret 或真实账号数据。

**完成条件**：所有已确认故障都有可重复 fixture，且 fixture 能在当前代码上稳定复现或明确记录无法复现的原因。

### Phase 1：统一候选事实来源

**目标**：planner 和 workspace 使用同一组可路由候选。

**主要文件**：

- `src-tauri/src/persistence/stores/operational_facts/queries.rs`
- `src-tauri/src/persistence/stores/routing_store.rs`
- `src-tauri/src/application/operational_facts/assembler.rs`
- `src-tauri/src/models/operational/raw_facts.rs`

**工作内容**：

1. 引入或完善唯一的 `OperationalFactBundle`，由一次 caller-owned durable read transaction 构建；Bundle 只组合 typed slices，不把所有事实揉成无边界结构。
2. 将“路由候选集合”定义为 station enabled、key enabled、credential available 的 executable candidates；credentialless key 如需展示，走独立 inventory/diagnostic source。
3. 在事实源头完成 credential 过滤，不能让 credentialless row 占用 planner 的 SQL limit。
4. 保留 credentialless key 的展示需求时，另建配置诊断列表，不将它混入 planner candidate set。
5. 使用统一的稳定排序；删除 workspace 侧独立 `.take(MAX_OPERATIONAL_CANDIDATES)`。candidate cap 的定义只在 planner admission 层出现一次。
6. 如果仍需要数据库 source upper bound，必须显式返回 `source_truncated`，不能静默丢失后续可用 key；可先采用 keyset pagination 读取至“足够的 admitted candidates”或明确触发 unavailable。
7. companion query 只能接收 bundle 中的 `(station_key_id, endpoint_revision, credential_revision)`，并验证一一对应。

**完成条件**：

- planner 和 workspace 的 executable candidate ID 集合相同；
- 同一 key 在一次响应中只有一个 assessment；
- 无凭据 key 不会挤掉可评分 key；
- source 不完整时顶层状态为 unavailable 并带稳定 code。

**局部重构边界**：本阶段不改评分函数、不改 UI、不迁移历史表；只替换 candidate read port，并保留旧 `load_runtime_candidates` 作为短期 adapter，禁止新调用方继续依赖它。

### Phase 2：收口 assessment 与 candidate cap

**目标**：建立唯一资格判断 owner，解决“根本没进入评分流程”无法解释的问题。

**主要文件**：

- `src-tauri/src/application/operational_facts/planning_snapshot.rs`
- `src-tauri/src/application/routing.rs`
- `src-tauri/src/application/queries/routing_workspace.rs`

**工作内容**：

1. 抽取纯核心 `assess_candidate`，统一处理 mapping、capability、group、health、error rate、balance 和 multiplier ceiling。
2. `PlanningBuildResult` 至少包含：
   - 普通 candidate snapshot；
   - 普通 candidate assessments；
   - 独立的 probe discovery assessments 或 coordination candidates。
3. `max_candidates` 只作用于 `AdmittedForScoring`；probe discovery 不计入普通 cap。
4. 先生成所有 assessment，再按确定性顺序给普通候选标记 `within_limit` 或 `capped_by_candidate_limit`。
5. 只有 `admitted_for_scoring + within_limit` 才计算 score breakdown。
6. workspace 删除重复的模型、分组、能力、余额和倍率资格判断。
7. assessment join 必须校验 key、endpoint、snapshot ID、durable revision 和 request fingerprint。
8. 将 cap 逻辑拆成 `assess_all`、`partition_probe`、`apply_ordinary_cap` 三个纯步骤，避免以后新增 gate 时再次把 probe 或 exclusion 混入 cap。

**固定状态映射**：

```text
planner_evaluation=unavailable -> unavailable
probe_discovery_only           -> probe_discovery
admitted + capped              -> candidate_limit
admitted + within_limit        -> scored
其他                           -> excluded
```

**完成条件**：probe 永不获得 score，ordinary candidate 不被 probe 挤出 cap，workspace 不再拥有第二套 eligibility owner。

**局部重构边界**：`PlanningSnapshotBuilder` 保留为编排器，不拆成跨模块 service；只抽取纯 assessment/cap 核心和一个 workspace-only report helper。

### Phase 3：统一余额选择和生产 projector

**目标**：消除“有余额仍余额不足”和 SQL/Rust/test 规则不一致。

**主要文件**：

- `src-tauri/src/application/operational_facts/balance_projector.rs`
- `src-tauri/src/application/operational_facts/candidate_projection.rs`
- `src-tauri/src/persistence/stores/operational_facts/queries.rs`
- `src-tauri/src/persistence/stores/routing_store.rs`
- `src-tauri/src/models/routing.rs`

**工作内容**：

1. 将 `BalanceSelectionPolicy` 和 projector 改为生产代码，移除核心逻辑上的 `#[cfg(test)]`。
2. SQL 只读取 station scope、key scope 的原始 observation，不在 SQL 中决定最终选择。
3. 统一规则：
   - key scope 优先于 station scope；
   - 同 scope 选择最新且有效的 observation；
   - 有限数值优先于文本 status；
   - 数值大于 0 为可用；
   - 数值小于等于 0 为 depleted；
   - `low`、`warning` 只作提示；
   - 无数值时才使用 `depleted/exhausted/empty`；
   - low balance threshold 不得改变正余额的 spendability。
4. planner、runtime candidate projection 和 workspace 全部调用同一个 projector。
5. 为 numeric/status 冲突、scope 优先级、stale observation 和缺失值补充单元测试及跨层测试。
6. projector 输出同时包含 `selected_scope`、`spendability`、`display_status`、`observed_at` 和 balance fact revision，调用方不得再次从 `value`/`status` 推断资格。

**完成条件**：station depleted + key positive 的结果统一为 key scope、可路由；生产和测试使用同一实现。

**局部重构边界**：先让现有 SQL 返回两类原始 observation，再在 Rust projector 选择；不在本阶段修改余额采集器、余额币种换算或阈值配置含义。

### Phase 4：统一倍率、价格和 revision fence

**目标**：避免兑换率修改后评分、有效倍率和旧 assessment 错配。

**主要文件**：

- `src-tauri/src/application/operational_facts/pricing_projector.rs`
- `src-tauri/src/application/pricing.rs`
- `src-tauri/src/persistence/stores/station_catalog.rs`
- `src-tauri/src/application/operational_facts/assembler.rs`
- 必要时新增对应 persistence migration 和生成测试。

**工作内容**：

1. 在 Rust 类型中区分 `StationNativeMultiplier`、`EffectiveRateMultiplier`、`CreditPerCny` 和 compatibility cache。
2. inference route 只消费 canonical pricing context 的 effective multiplier。
3. 禁止将 `station_keys.rate_multiplier` 静默当作当前 effective multiplier。
4. 将 revision 从“多个 max scalar”升级为结构化 vector 或稳定 hash，至少包含 station、key、account、group、capability、health、balance、pricing、policy、mapping。
5. 修改 `credit_per_cny`、group multiplier/status、balance snapshot 后推进对应 revision。
6. 修改成功后统一触发 `refreshRoutingQueries`，同时保留 pricing、station、key-pool 等原有 query invalidation。
7. 对兑换率变化前后 workspace score、multiplier、source refs 和 revision 做回归断言。
8. 将 durable revision 与 ephemeral runtime overlay revision 分开命名和存储，禁止把 in-flight、cooldown 或当前进程计数加入 durable assessment identity。

**完成条件**：兑换率变化必然产生新的 assessment identity；不会对 compatibility cache 进行二次归一化。

**局部重构边界**：优先新增 typed pricing projector 和 revision builder，暂不改动消费记录和历史账单字段；需要 schema 变更时单独增加 migration、恢复测试和删除条件。

### Phase 5：统一评估时间和 mapping snapshot

**目标**：一次评估不再混用多个时间点或多个 mapping 版本。

**主要文件**：

- `src-tauri/src/application/routing.rs`
- `src-tauri/src/application/operational_facts/planning_snapshot.rs`
- `src-tauri/src/application/model_mapping/mod.rs`

**工作内容**：

1. 在 application boundary 创建 `EvaluationContext`：
   - `evaluated_at_ms`；
   - deadline；
   - request facts；
   - routing policy snapshot/revision；
   - compiled mapping snapshot/revision。
2. health、pricing 和 durable assessment 使用同一个 `evaluated_at_ms`；runtime overlay 使用自己的 capture time/revision，并在 DTO 中明确标注为 runtime fact，不混入 durable assessment fence。
3. 禁止 planner 在 transaction 内重新调用全局 `current_configuration()`。
4. mapping snapshot revision 与 durable mapping revision 不一致时返回 `mapping_revision_mismatch`。
5. 明确区分 `evaluated_at`、`observed_at`、`collected_at`、`updated_at` 和 durable revision，禁止用时间戳冒充 revision。

**完成条件**：一次 workspace 响应中的 durable assessment、pricing 和 health 读取具有同一 evaluation context；runtime overlay 能独立判断是否过期，且不会伪造 durable revision。

**局部重构边界**：只在 routing application boundary 捕获 context，不改全局时钟实现；测试通过 injected clock 或固定 `evaluated_at_ms` 保证确定性。

### Phase 6：错误、IPC 和前端状态收口

**目标**：前端不再从 null、旧字段或空数组猜测业务状态。

**主要文件**：

- `src-tauri/src/application/routing.rs`
- `src-tauri/src/ipc/dto/routing_health_reads.typescript.txt`
- `src/lib/bridge/generated.ts`
- `src/lib/types/routingWorkspace.ts`
- `src/features/routing/LocalRoutingCandidateRow.tsx`
- `src/features/routing/LocalRoutingStatusCandidateRow.tsx`

**工作内容**：

1. workspace DTO 升级为新 read-model version，`scoreStatus`、`plannerExclusionCodes` 和 assessment provenance 改为必填。
2. 旧 DTO 兼容集中在 adapter，业务 view model 不再根据 `null` 猜状态。
3. 删除或隔离 `previewEligible`、`previewRejectReasons` 的业务判断。
4. planner 非 deadline 错误保留稳定错误码和脱敏诊断；deadline 继续传播。
5. source mismatch 返回基础行时，顶层 `planner_evaluation=unavailable`，所有行派生 `score_status=unavailable`。
6. 通过 `pnpm generate:bindings` 重新生成 IPC artifacts，不手工修改 generated 文件。
7. 将错误分为 `fact_read`、`policy_invalid`、`mapping_mismatch`、`source_integrity`、`deadline`、`internal` 等稳定类别；应用层只在边界做一次映射。

**完成条件**：前端只消费显式 `scoreStatus`；任何 `score == null` 都能定位到状态和原因。

**局部重构边界**：先增加 DTO v2 和 adapter，再迁移页面；不在同一提交中同时删除所有旧类型和所有测试 fixture。

### Phase 7：影子对照和切换保护

**目标**：在正式删除旧路径前，检测新旧语义差异，避免把隐藏行为误判成 bug 修复。

**工作内容**：

1. 仅在 workspace/diagnostics 或测试模式运行新旧 assessment 对照，不进入 proxy 热路径。
2. 对照内容只包含脱敏的 key ID、状态、primary reason、effective multiplier 是否存在和 revision，不记录 secret、原始请求或上游响应。
3. 对每类 mismatch 分类：候选集合、余额、倍率、health、mapping、cap、错误映射。
4. 为每个 mismatch 指定预期新语义、保留兼容语义或阻断切换；不得用“新结果不同”自动判定新结果正确。
5. 当连续一个完整迭代周期没有未解释 mismatch 后，关闭影子对照并允许退役旧路径。

**完成条件**：所有差异都有测试、设计决策或明确的遗留项，不以 UI 手工观察作为切换依据。

### Phase 8：旧路径退役

**目标**：避免本轮收口后继续保留会产生分歧的旧逻辑。

**工作内容**：

1. 将旧组件和 helper 标记为 compatibility-only，并禁止新调用方使用。
2. 迁移现有测试 fixture 到新 DTO 和新 assessment 语义。
3. 删除 workspace 中重复的 `hard_rejection_codes` 资格计算，仅保留明确的展示事实字段。
4. 删除前端 fallback 后再移除 `previewEligible` 和 `previewRejectReasons`。
5. 更新 deletion ledger，记录每个旧 owner 的删除提交、替代 owner 和验证证据。
6. 在连续一个完整迭代周期内没有兼容调用后删除旧模块，而不是提前保留永久 fallback。

## 6. 测试与验证矩阵

### 6.1 Rust 单元与集成测试

- `BalanceSelectionPolicy`：scope、numeric/status 冲突、阈值、stale、missing。
- candidate assessment：每个排除原因、普通 cap、probe cap、mapping failure。
- bundle integrity：候选和 assessment 一一对应、revision 一致、source truncation。
- pricing：native/effective 单位、兑换率更新、兼容缓存不二次折算。
- evaluation context：同一次构建只有一个时间点和 mapping revision。
- error mapping：非 deadline 错误可观察，deadline 可传播。

### 6.2 TypeScript/Vitest

- DTO 必填状态反序列化。
- `scored`、`excluded`、`candidate_limit`、`probe_discovery`、`unavailable` 展示。
- 旧 DTO 只在 adapter 层兼容。
- planner `unavailable` 时前端不显示“余额不足”或“未参与”这类业务排除文案。

### 6.3 必跑命令

```powershell
pnpm vitest run src/lib/types/routingWorkspace.test.ts src/features/routing
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml
pnpm verify:fast
```

跨层 contract、generated binding、revision 或安全边界发生变化时，再运行：

```powershell
pnpm verify:full
```

## 7. 头脑风暴结论与设计决策

本次审阅后确认，最容易继续长成屎山的地方不是某个函数太长，而是“一个事实被多个周期和多个层级重复解释”。因此采取以下决策：

1. “一次 read”是事实读取边界，不等于把所有领域对象塞进一个大结构；Bundle 采用 typed slices。
2. “评估周期”只描述本次判断何时发生；provider observation 的 `observed_at/collected_at` 保留原始语义，不能强行改成同一个 now。
3. “revision”描述事实身份；时间戳只用于 freshness，runtime revision 不得冒充 durable revision。
4. “资格、cap、评分、展示”是四个阶段，新增规则只能进入明确阶段，不能直接往 workspace 或前端加判断。
5. “兼容”是迁移边界的短期能力，不是业务默认 fallback；每个兼容字段必须有 owner、调用清单和删除日期。
6. 优先局部抽纯函数、typed value object、read port 和 adapter；保留 `RoutingService` 外部接口，降低一次改动的 blast radius。
7. 先修正事实和版本，再切换 UI；否则 UI 修复只会把错误来源隐藏得更深。

## 8. 实施顺序与回滚策略

推荐分四个可审阅批次合并：

1. 批次 A：Phase 0、Phase 1、Phase 3，先统一事实和余额；
2. 批次 B：Phase 2、Phase 4、Phase 5，统一 assessment、pricing、revision 和时间；
3. 批次 C：Phase 6、Phase 7，先完成 DTO、前端迁移和影子对照；
4. 批次 D：Phase 8，满足 mismatch 和删除台账条件后退役旧路径。

每个批次都必须先通过对应 focused tests，再进入下一批次。不得在候选来源尚未统一时先修改前端文案。

回滚要求：

- 保留 adapter 和旧 read-model 的短期读取兼容，但不保留旧资格判断作为 fallback；
- 如果新 projector 或新 revision 导致结果异常，回滚调用入口，不回退数据库事实或删除 migration；
- 所有 migration 必须可前滚、可重启恢复，并在执行前完成备份和 schema 检查；
- 回滚后仍保留失败样本和 source provenance，避免再次出现无法定位的“无分”。

## 9. 最终完成标准

以下条件全部满足后，才能将计划标记为完成：

1. planner/workspace 候选集合来自同一 bundle。
2. workspace 不再实现资格判断。
3. probe 不占普通评分 cap。
4. 正余额不会被旧 station status 覆盖成余额不足。
5. 生产和测试使用同一个 balance projector。
6. 兑换率和余额修改能使 assessment revision 变化并触发路由查询失效。
7. 同一次评估只有一个 evaluation time 和 mapping revision。
8. DTO 状态字段必填，前端不再从 null 猜测。
9. planner 错误保留稳定 code，不会伪装成业务排除。
10. 旧路径已完成迁移或有明确删除日期、owner 和验证证据。
