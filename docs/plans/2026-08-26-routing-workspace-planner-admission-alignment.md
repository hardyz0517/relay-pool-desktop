# 路由工作区与规划器准入结果对齐实施计划

状态：核心实现已完成；真实 Provider、发布和安装包验证未纳入本次范围。本文只处理路由工作区中“可展示候选、规划准入、评分状态”口径不一致的问题，不改路由策略或兑换率公式。

日期：2026-08-26

关联入口：[`../README.md`](../README.md)、[`../specs/INTELLIGENT_ROUTING_ENGINE_SPEC.md`](../specs/INTELLIGENT_ROUTING_ENGINE_SPEC.md)、[`../PRICING_MULTIPLIER_MODEL.md`](../PRICING_MULTIPLIER_MODEL.md)、[`2026-08-23-routing-ownership-lifecycle-cleanup.md`](2026-08-23-routing-ownership-lifecycle-cleanup.md)

适用范围：`PlanningSnapshotBuilder` 的候选准入结果、Routing Workspace read model/IPC DTO、路由状态页的评分与排除原因展示、模型基础价格/分组倍率/站点兑换率变更后的路由查询失效，以及对应 Rust/TypeScript 测试和生成绑定。

不在范围：评分权重或公式、`creditPerCny` 与实际倍率公式、余额/健康/模型映射的业务规则、代理请求选路语义、schema 迁移、主动监控、路由页面的整体视觉改版、真实 Provider 测试或发布验证。

> 本计划修复的是事实与解释的断裂，不是放宽任何资格门。当前价格模型以 `model_base_prices`、`station_group_bindings.effective_rate_multiplier` 和站点 `creditPerCny` 为来源；已移除的旧价格规则不参与价格或倍率选取。`0.07x` 等实际倍率仅是成本评分输入；候选是否进入评分仍由资格、分层和候选上限决定。

---

## 1. 已确认的问题

当前 `load_routing_workspace_snapshot` 存在三个不同来源的判断：

```text
canonical candidate read
  -> 工作区行、倍率、部分 hardRejectionCodes

PlanningSnapshotBuilder
  -> 模型映射、能力、分组、标签、余额、scoped health、错误率保护、候选上限
  -> 保留可评分候选和仅健康恢复探测候选
  -> 评分

workspace join by station_key_id
  -> 找不到 score 时返回 null
  -> 前端显示“—”
```

这会产生两类不可区分的 `score == null`：

1. 密钥确实未通过规划器准入，或被 `max_candidates` 截断，因此没有参与评分。
2. 规划快照读取失败；除 deadline 外的错误目前被转为空评分表，页面误显示为所有相应密钥“无评分”。

此外，工作区候选、规划快照和质量摘要分多次读取；定价/兑换率变更只失效定价与渠道查询，没有保证同时失效 Routing Workspace 查询。这会造成一次刷新窗口内“倍率已更新、评分或排除理由仍来自另一版本事实”的错配。

这与当前智能路由规范冲突：资格、评分、模拟、生产选择和决策解释必须使用相同的后端领域内核；`Eligibility`、`Tier`、`Score` 和 `Dispatch` 又必须保持为不同概念，不能用 UI 的一个布尔字段替代。

## 2. 目标契约与不可变决定

### 2.1 单一事实来源

`PlanningSnapshotBuilder` 仍然是候选资格的唯一 owner。不得在 Workspace projector、前端 view model 或 query 层再实现模型映射、能力、余额、scoped health、错误率保护或候选上限的第二套判断。

Builder 的资格判断必须有一个可复用的纯核心。Workspace 调用该核心时，获取覆盖本次 source-bounded canonical candidates 的只读 `PlanningCandidateAssessment` 报告；生产请求调用同一核心时，只保留原有的 `PlanningSnapshot.candidates`。两者不得各自实现 gate，也不得为了诊断而把全量 assessment 永久附加到生产 planner 输入。

建议的领域形状如下，命名可按现有模块术语微调：

```text
PlanningBuildResult (application-local, not a planner input)
  snapshot: PlanningSnapshot
    candidates: CandidateSnapshot[]          // 仅供 planner 选择，保持现有执行语义
  assessments: PlanningCandidateAssessment[] // Workspace/模拟诊断请求时才 materialize

PlanningCandidateAssessment
  station_key_id + source revision fence + request-context fingerprint
  eligibility:
    admitted_for_scoring
    excluded
    probe_discovery_only
  candidate_set:
    not_applicable
    within_limit
    capped_by_candidate_limit
  primary_reason: stable enum/code (required for excluded or probe_discovery_only)
  secondary_reason_codes: stable ordered code[]
  model-mapping disposition（在安全、必要的范围内）
```

`PlanningBuildResult` 与 `assessments` 都不是第二套 candidate model：它们只记录 builder 已经得出的准入结论、理由和版本围栏，不能自行决定资格或重新计算分数。生产热路径不序列化、不缓存、不遍历该报告；Workspace 在同一次 read 中构建一次并立刻投影。这样既保留可解释性，也不会让 UI 诊断负担进入每个代理请求的长期内存模型。

`probe_discovery_only` 是当前 error-rate protection 为恢复 Half-Open probe 保留的执行协调候选，不是用户目标评分候选。它即使暂时存在于 `PlanningSnapshot.candidates` 中，也不得获得 Workspace score 或被计入“可参与/已评分”统计；页面只说明“仅用于恢复探测”。`candidate_set` 与 `eligibility` 分离：业务排除为 `not_applicable`，通过资格门的候选再表示是否被上限截断。

最终 `score_status` 必须按固定优先级从 assessment 投影，而不能由 `PlanningSnapshot.candidates` 反推：顶层 `planner_evaluation=unavailable` 时为 `unavailable`；否则 `probe_discovery_only` 为 `probe_discovery`（即使同时被 cap，也以该状态为主）；其后 `admitted_for_scoring + capped_by_candidate_limit` 为 `candidate_limit`；`admitted_for_scoring + within_limit` 为 `scored`；剩余为 `excluded`。可能同时存在的 cap 信息作为诊断辅助字段，不再增加第二个用户主状态。

`request-context fingerprint` 至少包含 route kind、是否为基线工作区请求、请求模型/映射输入是否存在、stream/tools/vision/reasoning、分组/标签，以及已捕获的 settings、routing policy 和 model-mapping revision。首期 Workspace 明确只展示“基线请求”的 assessment，不能暗示一个 key 对所有实际模型请求都可路由；未来增加模型筛选时以新的 context 生成 assessment，而不是复用基线结果。

### 2.2 候选集合与快照身份

同一个 Workspace 响应中的展示行与 assessment 必须来自同一份 `OperationalFactBundle` 的 key/revision 集合。推荐做法是由该 bundle 投影出 Workspace 所需的身份、能力、经济和展示事实；若少量展示字段暂时仍需批量 companion query，该 query 必须以 bundle 的 `(station_key_id, endpoint_revision, credential_revision)` 为输入，并在 join 时验证一一对应。

“工作区行找不到 assessment”是 source-integrity 故障，不是 `excluded`。这种情况必须提升为 snapshot 的 `planner_evaluation=unavailable`，记录脱敏诊断并触发 focused test；不得用 key id 的宽松 join、默认值或 UI 文案掩盖。这样新增加的 candidate source、筛选条件或 revision 字段会在契约测试中立即暴露，而不是再次变成孤立的 `—`。

### 2.3 明确的评分状态

Workspace DTO 不再仅用可空 `score` 暗示状态。快照新增 `planner_evaluation`（`available` 或 `unavailable`，后者带安全稳定 code）；每行新增稳定的 `score_status`，并维持 `score`/`score_details` 作为有分时的载荷：

| `score_status` | `score` | 用户含义 |
| --- | --- | --- |
| `scored` | 必有 | `eligibility=admitted_for_scoring` 且在候选上限内，已计算评分。 |
| `excluded` | 必为空 | 未进入评分；`planner_exclusion_codes` 必须说明原因。 |
| `candidate_limit` | 必为空 | 本应进入本轮 planner candidate set，但被候选数量上限截断；保留其 underlying eligibility 供诊断。 |
| `probe_discovery` | 必为空 | 仅可用于受保护候选的恢复探测，不参与用户目标评分；cap 只作诊断信息。 |
| `unavailable` | 必为空 | 仅当 snapshot 的 `planner_evaluation=unavailable`；不是业务排除。 |

`planner_exclusion_codes` 由 `primary_reason` 加安全的 secondary codes 投影而来，前端优先解释 primary reason。当前 `hard_rejection_codes` 不能整体改名后继续当作第二个 owner：只有与 assessment 逐项等价的规则才能迁入。其余展示事实必须另起字段并在名称中表明其只读性质，不能再反向影响行级评分状态或汇总统计。

### 2.4 错误与一致性

非 deadline 的 planner 错误不得被降级成“无分”。`RoutingReadModelStatus` 继续只表达 Workspace read model 本身是否可用；新增的 `planner_evaluation=unavailable` 才表达 planner assessment 不可用，并带安全的稳定错误码。行级 `unavailable` 必须由该顶层状态派生，不能逐行拼出不同错误。日志可保留完整的脱敏诊断，IPC 与 UI 不得泄露凭据、请求内容或上游原始错误。

同一次 Workspace 加载必须在一个 caller-owned durable read transaction 内读取 settings/policy、同一 OperationalFactBundle 投影的候选与 companion facts、pricing context、planning assessment 和 quality summary。构建开始时同时捕获不可变的 request/model-mapping context；builder 不得在 Workspace join 期间再次读取可变全局 mapping configuration。为此提取接受现有 `&mut read` 及已捕获 context 的 planning-build helper，Workspace 不得先关闭一个 read 再打开第二个 read 来 join score。runtime overlay 仍是独立 process-lifetime 数据，保持现有单独读取和显式 revision，不能混入 durable transaction。此任务不采用“读到不一致就试一次”的模糊补救策略。

模型基础价格的同步、恢复、增删改，站点 `creditPerCny` 更新，以及分组 binding 的倍率/状态更新成功后必须复用 `refreshRoutingQueries` 使 `routingQueryKeys.all` 失效；仍保留各页面已有的 pricing、station、key-pool、channel 失效，不以路由刷新替代它们。不得为此项工作恢复已移除的旧价格规则 IPC/API。

## 3. 实施任务与切换顺序

### Task 0：冻结现状、原因词典与回归基线

**目标：** 先固定当前执行资格语义与用户可见原因，避免重构时把“解释变清楚”误做成“门槛变宽”。

**文件（预期）**

- Update: `src-tauri/src/application/operational_facts/planning_snapshot.rs`
- Update: `src-tauri/src/application/routing_engine/planning_snapshot.rs`
- Update: `src-tauri/src/models/operational/raw_facts.rs` and `src-tauri/src/persistence/stores/operational_facts/queries.rs` if the assessment needs multiplier facts not currently present in `OperationalFactBundle`
- Update: focused planner/workspace Rust tests
- Create or update: architecture/contract test only when已有测试目录已有同类入口

**步骤**

1. 枚举 builder 当前所有提前返回和过滤点：模型映射无 offering/target、unsupported model、credential、协议/模型/feature、标签、分组、余额、scoped health、错误率保护和 `max_candidates`。同时逐项审计 Workspace 当前的 `hard_rejection_codes`，包括 `candidate_unschedulable`、`multiplier_ceiling`、pricing basis 和 capacity；不能假定两者已经等价。
2. 将 planner 已拥有的原因收敛为闭合集合的 stable code，定义 primary reason 的优先级和安全的 secondary code 顺序；区分“业务排除”“仅允许 health probe discovery”“候选上限截断”和“系统不可用”。不要把原始错误字符串传入 DTO。
3. 固定倍率事实语义：`station_group_bindings.effective_rate_multiplier` 是站点原始倍率来源，必须在 Rust `effective_rate_multiplier(raw, credit_per_cny)` 中仅归一化一次；内部 assessment/operational fact 的命名使用 `station_native_multiplier` 或同等术语，不能把未归一化的数据库列再命名为 effective multiplier。为 `raw=2, creditPerCny=27 -> 2/27` 增加跨 projector/Workspace 回归，防止双重折算或漏折算。
4. 对 Workspace 独有而当前 planner 未实施的资格门，先按照当前规范判定 owner：若它属于实际代理资格（例如当前价格规范已规定的实际倍率上限），必须在同一变更中将分组原始倍率和站点兑换率作为批量 operational facts 接入 builder，并用 assessment 解释；若只属页面提示，则改为非资格展示字段。不得为删除重复代码而静默放宽或新增生产路由规则。
5. 为每个 code 写最小、确定性的 builder 测试，断言除前一条已确认并有现行规范依据的资格补齐外，planner 的 admitted candidates 与顺序不变。
6. 增加一个测试，证明有效、缺失或无效的 `creditPerCny` 均遵循现有“缺失/无效按 `1` 归一化”的倍率规范，且不会单独造成 `excluded`；原始倍率缺失仍保持未知，并沿用既有成本先验语义。

**完成条件：** 每一条不进入评分的生产分支都有稳定 code 和测试；任何 planner/Workspace 资格差异都有明确 owner、规范依据和回归测试，不存在“为了对齐而删除某条现有资格”的隐式行为变化。

### Task 1：在 builder 内生成全量 assessment

**目标：** 让“是否进入评分”及其理由只计算一次，并与实际 `PlanningSnapshot.candidates` 的 ordinary/probe 语义精确对齐。

**文件（预期）**

- Modify: `src-tauri/src/application/operational_facts/planning_snapshot.rs`
- Modify: `src-tauri/src/models/operational/raw_facts.rs`, `src-tauri/src/persistence/stores/operational_facts/queries.rs`, and `src-tauri/src/application/operational_facts/assembler.rs` to carry the minimal typed multiplier facts needed for eligibility and score projection
- Modify: `src-tauri/src/application/routing_engine/planning_snapshot.rs`
- Modify: `src-tauri/src/application/routing_engine/intelligent_planner.rs` only if type visibility requires
- Update: builder、planner、simulation focused tests

**步骤**

1. 先扩展 `RawOperationalCandidateRow` / `OperationalCandidateFact`，以强类型携带 eligibility 所需的站点 `credit_per_cny`、绑定原始倍率、binding status 以及 station / binding revision。保持数据库列 `station_group_bindings.effective_rate_multiplier` 的兼容，但在 Rust 领域模型中必须将其命名为未归一化的 `station_native_multiplier`（或等效语义名称）。不得从 Workspace canonical DTO 反向取数，也不得逐候选读 pricing。
2. 抽取单一 `assess_candidate` 核心；现有 production builder 只由其投影出 `PlanningSnapshot.candidates`，Workspace-only helper 在同一轮构建中额外收集 assessment。两者都消费同一个已加载的 `OperationalFactBundle`、已归一化的 typed multiplier fact 与已捕获的 mapping context。禁止将 diagnostics Vec 加入 `PlanningSnapshot` 或让 proxy planner 遍历它。
3. 将构建起点捕获一份不变的 compiled mapping configuration、其 revision 和 `ModelRequestFacts`，并以参数传入 `assess_candidate`、`capability_subjects_for_planning` 与 `candidate_native_models`。删除这一调用链中对 `current_configuration()` 的重读，包括 `durable_revision` 回退取值；同一 Workspace / planner build 中不得用不同 mapping revision 解析 capability subject、native model 和 candidate assessment。
4. 对 model mapping 无结果、model-scoped verdict 和 error-rate admission 保留精确的排除理由，不能在中间 `return None` 时丢失原因。无法继续判断的短路分支只发出一个 primary reason；只有已安全获得的独立原因才作为 ordered secondary code。
5. 维持现有 health probe discovery 的执行语义，并在 assessment 中写为 `probe_discovery_only`。它可以保留在 `PlanningSnapshot.candidates` 供执行协调使用，但不获得 Workspace score，也不得伪装成可评分候选。
6. 在现有 inclusion predicate 之后，按原有确定性顺序应用 `max_candidates`；先为每个 canonical candidate 生成 assessment，再将通过 gate 的候选标记为 `within_limit` 或 `capped_by_candidate_limit`，业务排除者为 `not_applicable`。被截断的 ordinary candidate 不得进入 `PlanningSnapshot.candidates` 或获得评分；被截断的 probe discovery candidate 仍以 `probe_discovery` 作为主状态，但保留 cap 诊断信息。
7. 给 assessment 附带 station key、endpoint/credential/account/group revision、planning snapshot id 和 request-context fingerprint，以防将旧评估或基线评估回填给修改后的 key/不同请求。
8. 保持 builder 的批量读取和既有 source upper bound；模型基础价格仅在请求成本估算/展示需要时通过现有 `PricingStore::resolve_station_key_pricing_many` 批量解析。实际倍率资格和 score proxy 只消费 bundle 中的原始倍率 + `credit_per_cny` 归一化结果，禁止为这两项逐候选 pricing 查询。

**完成条件：** 任何 `scored` assessment 都对应一个 `hard_eligible=true` 的 planner candidate；每个 `hard_eligible=true` 且非 probe discovery 的 planner candidate 都有且只有一个 `admitted_for_scoring + within_limit` assessment；`probe_discovery_only` 可以留在 snapshot 但永不评分；候选 cap 与 health probe 行为保持原样；生产 `PlanningSnapshot` 的结构和热路径遍历范围不因诊断需求扩大。

### Task 2：收口 Workspace 后端读模型与 IPC 契约

**目标：** 工作区直接消费 planner assessment，不再自行猜测是否合格。

**文件（预期）**

- Modify: `src-tauri/src/application/routing.rs`
- Modify: `src-tauri/src/application/queries/routing_workspace.rs`
- Modify: `src-tauri/src/ipc/dto/routing_health_reads.rs`
- Regenerate: run `pnpm generate:bindings` to update the repository-owned IPC artifacts; do not hand-edit generated files
- Update: IPC DTO snapshot、Rust workspace read-model tests

**步骤**

1. 用 `assessment_by_key + source revision fence + request-context fingerprint` 替代只含 score 的 `score_by_key` join；评分 breakdown 仅对 `eligibility=admitted_for_scoring && candidate_set=within_limit` 计算。`probe_discovery_only` 即使为了执行协调留在 `PlanningSnapshot.candidates`，也不生成 score。
2. 为每行填充 `score_status`、`planner_exclusion_codes` 和 assessment revision/snapshot provenance；行状态只能从 assessment 按 2.1 的固定优先级投影，不得直接从 `PlanningSnapshot.candidates` 推断。保留现有分数载荷的精度与评分公式。`candidate_limit` 不是 hard rejection，不得混入 exclusion codes；`probe_discovery` 不打开评分明细。
3. 保留 `RoutingReadModelStatus` 的原职责，新增 workspace-level `planner_evaluation` status/code；planner 读取失败时以 `planner_evaluation=unavailable` 和稳定 code 返回可用的基础候选列表，所有行派生 `score_status=unavailable`，而非 `unwrap_or_default()` 空评分表。deadline 继续向调用方传播。
4. 将 `load_intelligent_planning_snapshot_within_deadline` 的内部组成抽为接受现有 `&mut read`、已加载 `OperationalFactBundle` 与已捕获 mapping context 的 helper。`load_routing_workspace_snapshot` 在一个 caller-owned read transaction 中从该 bundle 投影候选、加载受 bundle identity 限定的 companion/pricing facts、构建 assessment 与 quality；不得另开 read 后按 key join，也不得以 retry 猜测一致性。无法建立一一对应时把整个 planner evaluation 标为 unavailable。runtime overlay 保持事务外读取并携带自身 revision。
5. 更新 read-model version 与生成 DTO 的兼容策略；所有 TypeScript fixture 必须显式声明新的 score 状态，禁止依赖 `null` 的隐式语义。

**完成条件：** `score == null` 不再是无解释状态；planner 失败、业务排除、候选截断和恢复探测在 DTO 上互不混淆；workspace 的“基线评估可参与”来自 assessment 加其已声明的 baseline request context，而非本地猜测。

### Task 3：前端状态与诊断展示

**目标：** 将 `—` 改为可操作的状态，不让用户误把“未评分”理解为“0 分”或“倍率异常”。

**文件（预期）**

- Modify: `src/lib/types/routingWorkspace.ts`
- Modify: `src/features/routing/LocalRoutingStatusCandidateRow.tsx`
- Modify: `src/features/routing/RoutingStatusDiagnosticsPanel.tsx`
- Modify: `src/lib/query/routingQuerySynchronization.ts`
- Modify: `src/features/pricing/ModelBasePricesPage.tsx` and the current station/group mutation callers (currently including `src/features/stations/useStationsPageController.ts` and `src/features/stations/useAddProviderPageController.ts`)
- Update: routing workspace view-model、状态行、诊断面板、query invalidation Vitest

**步骤**

1. 显示 `已评分`、`未进入评分：<原因>`、`候选上限外`、`仅用于恢复探测`、`评分暂不可用` 五类状态；score detail 弹窗仅在 `scored` 时可打开。“仅用于恢复探测”不表示普通用户请求一定不可用，也不展示伪造的零分。
2. 用共享的前端 reason-code 文案表展示稳定中文说明；未知 code 回退为通用安全文案，不显示后端错误原文。
3. 删除或改名 `previewEligible` 这个二元字段；汇总计数、诊断面板与行展示一律消费 `planner_evaluation`、`score_status` 与 `planner_exclusion_codes`。`planner_evaluation=unavailable` 时明确显示“基线评估暂不可用”，不把所有候选计入可用或排除；`probe_discovery` 也不计入可参与/已评分统计。
4. 审计并在现有 mutation caller 边界补上失效：模型基础价格同步、恢复、新增、编辑、删除，站点 `creditPerCny` 更新，以及分组 binding 倍率或状态的更新。成功后统一调用已有 pricing 失效和 `refreshRoutingQueries`；保留各自的 station、key-pool、channel 失效，不以 routing 刷新替代它们。测试证明 routing workspace 立即进入 refetch，而非等待五秒 stale window。
5. 保持现有浅色、紧凑的表格表现；窄窗口、loading、`planner_evaluation=unavailable`、空列表及键盘/焦点行为要有明确状态。

**完成条件：** 用户可从单行直接判断“为什么没有分”；不存在同一份 baseline assessment 同时显示“可参与”与 `excluded` 的矛盾文案，也不把它承诺为任意模型请求的最终选路结果。

### Task 4：删除已证实重复的 Workspace 判断并建立防回归门禁

**目标：** 避免下一次新增资格规则时再次只改了 planner 或只改了 Workspace。

**文件（预期）**

- Modify: `src-tauri/src/application/queries/routing_workspace.rs`
- Modify: `src-tauri/src/application/operational_facts/candidate_projector.rs` only after a caller inventory proves that the affected code is the Workspace duplicate rather than an independent legacy/preview path
- Update: relevant focused/contract tests; add a static architecture gate only when a stable ownership boundary can be asserted without source-text false positives

**步骤**

1. 只删除 Workspace 已由 assessment 覆盖的资格重算，特别是将 `hard_rejection_codes` 作为行级评分状态或“可参与”统计来源的路径。先以 caller inventory 证明 `candidate_projector` 的同名代码没有独立 consumer；未证明前保持它不动。
2. 对 `multiplier_ceiling`、pricing basis、capacity 等发现的差异，完成 Task 0 的 owner 决策和回归后才能删除旧判断。不存在“没有 assessment code 就删掉”的路径。
3. 保留真正页面专属的事实字段，例如展示用价格来源、容量概览、健康摘要；这些字段不能反过来决定 planner eligibility。
4. 添加测试不变量：相同 baseline request context + durable revision 下，Workspace 行的 key/revision 集合与 assessment 集合一一对应；`scored` 行集合等于 `PlanningSnapshot.candidates` 中 `hard_eligible=true` 且非 probe discovery 的候选集合；每个 probe discovery 行都没有 score；每个未评分行都有非空、稳定状态；planner error 或 source mismatch 不会伪造 `excluded`。
5. 对 Rust reason enum/DTO 与前端文案建立契约测试：已知 code 有预期文案，未来未知 code 使用安全 fallback。不要用“每新增一个后端 code 必须改多个前端文件”的脆弱静态搜索代替类型或契约检查。

**完成条件：** Workspace 生产路径中没有重复实现已经由 planner assessment 覆盖的模型映射、余额、能力、scoped health 或候选上限资格判断；有独立 caller 的 legacy/preview 路径已被明确保留或另立迁移任务；新 planner 规则必须通过 assessment 才能影响页面基线资格状态。

## 4. 验收矩阵

| 场景 | 预期 Workspace 状态 | 关键断言 |
| --- | --- | --- |
| 正常 key，实际倍率 `0.07x` | `scored` | 有 score/明细；实际倍率作为成本因子。 |
| `creditPerCny` 缺失或无效 | `scored`（其余资格通过时） | 按当前规范以 `1` 归一化；不单独成为排除原因。 |
| 余额耗尽且不允许兜底 | `excluded` + `balance_depleted` | 不进入 planner candidates。 |
| 模型映射无 target | `excluded` + mapping code | 页面可说明，不能只有 `—`。 |
| 模型/credential/endpoint scoped health 拒绝 | `excluded` + 对应 health code | 不误归因于倍率。 |
| 超过 `max_candidates` | `candidate_limit` | 原候选集合和顺序保持不变。 |
| 冷却结束的 error-rate 恢复探测候选 | `probe_discovery` | `score=null`，不计入已评分或可参与统计；不可打开评分明细。 |
| planner build/read 错误 | snapshot `planner_evaluation=unavailable`，行 `unavailable` | 不把系统错误伪装为业务排除。 |
| Workspace row 与 assessment revision 不匹配 | snapshot `planner_evaluation=unavailable`，行 `unavailable` | 不以宽松 key join 伪造 `excluded` 或 `scored`。 |
| 修改模型基础价格、分组倍率/状态或 `creditPerCny` | refreshed workspace | 倍率、assessment provenance、评分来自同一有效版本。 |

## 5. 验证与交付门槛

实施中每个 Task 按 RED-GREEN-REFACTOR 推进；在上一任务的 focused tests 未通过前，不进入下一任务。最终至少运行：

1. 相关 Rust unit/integration tests（planning snapshot、workspace query、routing IPC DTO）。
2. `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`。
3. `cargo check --locked --manifest-path src-tauri/Cargo.toml`。
4. 相关 Vitest（routing workspace view model、状态行、诊断面板、query invalidation、generated bridge contract）。
5. `pnpm generate:bindings` followed by `pnpm generate:bindings --check`.
6. `pnpm build`。
7. `pnpm verify:fast`；本次跨 Rust/IPC/前端共享契约变更完成后以它作为最低整体验证门槛。若改动触及现有架构门禁、生成绑定或广泛投影，再运行 `pnpm verify:full`。

交付必须报告：实际修改文件、每项验证的退出结果、未运行检查及原因、是否更新生成物。不得提交本地数据库、日志、真实密钥、诊断输出或生成过程中产生的非受控 artifact。

## 6. 风险与决策边界

| 风险 | 控制措施 |
| --- | --- |
| 为解释保留全量 assessment 导致生产 planner 输入膨胀 | assessment 是 Workspace-only build result，不属于 `PlanningSnapshot`；代理热路径只构建/遍历既有 `candidates`。 |
| 重构时改变候选顺序或 `max_candidates` 行为 | 先冻结现有顺序测试；assessment 只记录 cap 结果，不改变排序键。 |
| 将恢复探测行当作普通不可用候选 | `probe_discovery` 与 `excluded` 分立，不用这个 baseline 行状态推断任意实际请求的最终路由结果。 |
| UI 把 `excluded` 错当真实请求的最终不可路由 | DTO 携带 baseline request-context fingerprint，文案明确为“当前工作区基线评估”；运行时容量、亲和、请求模型及 retry 仍可能改变某次请求结果。 |
| 把内部错误暴露到 IPC | DTO 仅提供稳定错误码与安全文案键；完整错误仅进入本地脱敏日志。 |
| 定价变更引发无关路由刷新风暴 | 仅在会改变实际倍率、价格比较上下文或准入相关经济事实的成功 mutation 后失效 routing family；使用现有 TanStack Query 前缀去重。 |

## 7. 完成定义

本计划完成时，路由页面不再以孤立的 `—` 表示密钥评分。每个密钥都能明确显示“已评分”“被规划器排除及原因”“因候选上限未评分”“仅用于恢复探测”或“基线评估暂不可用”；恢复探测候选永不得获得评分。生产选择与 Workspace 解释复用同一资格核心，模拟在迁移时复用同一 assessment contract。这个 Workspace 评估只代表 baseline request context，不代表所有模型或实际请求的最终选路结果。兑换率修改后，实际倍率与评分状态在同一 durable read 中更新；除 Task 0 已按当前规范确认的遗漏资格门外，不改变既有资格与成本公式。
