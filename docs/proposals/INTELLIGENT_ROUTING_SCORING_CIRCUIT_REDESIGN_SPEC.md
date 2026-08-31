# Relay Pool Desktop 智能路由评分、重试与熔断重构规范

状态：Proposed；本文件是面向下一轮局部重构的目标规范，不覆盖当前已实施规范，也不表示代码已经符合本文。

日期：2026-08-28

适用范围：本地 OpenAI-compatible 代理的候选排序、请求重试与故障转移、Key 级跨请求熔断、可靠性统计、主动监控样本、路由设置页和相关持久化/IPC 契约。

关联入口：

- [`../specs/INTELLIGENT_ROUTING_ENGINE_SPEC.md`](../specs/INTELLIGENT_ROUTING_ENGINE_SPEC.md)
- [`../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md`](../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md)
- [`../specs/ROUTING_POLICY_CONFIGURATION_SYSTEM_SPEC.md`](../specs/ROUTING_POLICY_CONFIGURATION_SYSTEM_SPEC.md)
- [`../plans/2026-08-21-routing-retry-failover-hardening.md`](../plans/2026-08-21-routing-retry-failover-hardening.md)

本文使用 `MUST`、`MUST NOT`、`SHOULD`、`MAY` 表示约束级别。

## 1. 结论摘要

本次重构不是重写所有路由设施，而是把当前分散的“评分、重试、健康、监控”重新收敛成下面这条行为链：

```text
硬资格/边界过滤
  -> 按最终有效评分从高到低形成确定性候选序列（无分数候选才按 station_key_id 稳定兜底）
  -> 发送一次真实请求
  -> 记录成功或可归责失败
  -> 可重试错误消耗本请求重试预算并重新排序
  -> 连续失败达到阈值后打开该 Key 的熔断器
  -> 冷却结束后，仅在评分优于当前快照同一硬层最高 Closed 候选时进入 Half-Open
  -> Half-Open 同一 Key 同时只允许一个真实请求
  -> 连续成功达到恢复阈值后回到 Closed
```

关键决策：

1. 评分只负责同一硬资格层内的排序。生产路由不再使用 weighted rendezvous、近优分数带或随机探索；每轮按分数降序确定下一候选，分数相同用稳定 Key 标识打破平局。高分 Key 会优先承载请求，只有容量准入明确拒绝时才继续尝试后面的 Key。
2. `最大重试次数`表示第一次发送之外的额外 outbound attempt 次数。每次重试获取新的健康/评分快照，但当前请求不会无边界重复同一 Key。
3. 熔断器不再由独立“错误率保护开关”驱动。它由 Key 的连续失败次数驱动；可靠性错误率只是评分统计的一部分，不再单独作为用户参数。
4. 每个真实路由 attempt 的成功或可归责失败都进入可靠性观测。当前 `502 -> GenericStatus -> Neutral` 的路径必须改掉；上游明确返回的 `5xx`、`429`、已跨 outbound boundary 的超时/连接失败和上游不确定失败都至少产生一个失败样本。`429` 首版只按当前 `station_key_id` 的普通 Key 故障处理，不推断站点、账号、端点或容量域级联故障。
5. 可靠性来源先分别计算“实际路由可靠性”和“主动监控可靠性”，再按默认 70% / 30% 混合；两个权重可在设置中调整。来源权重只影响质量统计，不影响熔断器连续失败计数。
6. 不再使用 Beta 先验。历史窗口和最近窗口分别有最小样本数；未达到门槛时使用用户设置的乐观可靠性和响应时间（默认 `95% / 2.5s`），因此没有样本的 Key 仍然有确定的排序值。
7. “24 小时无样本”以真实路由样本为主：监控样本可以参与质量计算，但不会解除真实路由闲置标记。近期/历史公式按不重叠窗口计算，样本不足时把乐观值代入对应窗口；不额外制造探测流量，Key 是否恢复完全由确定性评分顺序和正常真实请求决定。
8. 路由熔断首版只按 `station_key_id` 生效。429、5xx、超时和连接失败等已跨 outbound boundary 的 Key 级失败统一影响这把 Key；账号、端点、模型等故障域暂不参与路由决策，旧代码只保留作后续参考。
9. 会话亲和、评分偏好、路由边界和超时能力保留；候选与探索从用户设置中移除；本地容量准入保留，但容量域身份和跨容量域回退从生产路由移除。

## 2. 当前实现审计

以下事实来自 2026-08-28 工作区代码和当前文档。它们是重构依据，不是目标行为。

### 2.1 评分和候选选择

| 当前实现 | 代码位置 | 与目标的差异 |
| --- | --- | --- |
| `plan_snapshot_with_budget` 先按 `target_rank`、tier 和 utility 组织候选，再把最佳 tier 交给 `weighted_rendezvous` | `src-tauri/src/application/routing_engine/intelligent_planner.rs` | 不是严格的“分数从高到低逐个尝试”；同一分数带内会按 seed 选择 |
| `weighted_rendezvous` 使用 utility band 和 hash 权重选择候选 | `src-tauri/src/application/routing_engine/dispatch.rs` | 引入了分流和非顺序选择，和本次确定性降序尝试不一致 |
| `ExplorationBudgetRegistry`、`explorationShareBasisPoints` 和 `choose_lane` 仍在生产规划路径 | `src-tauri/src/application/routing_engine/exploration.rs`、`intelligent_planner.rs` | 用户要求取消随机探索；当前设置仍能改变探索比例 |
| `CandidateSnapshot` 仍携带 `max_candidates` 对应的策略限制 | `src-tauri/src/models/routing_policy.rs`、`planning_snapshot.rs` | 候选上限可以保留为系统内部硬上限，但不应继续作为用户可调的“候选与探索”功能；容量不应提前改写评分顺序 |
| Primary/Backup 等硬层级仍存在 | `src-tauri/src/application/routing_engine/tiers.rs`、`admission.rs` | 这是安全边界，应保留；“按分数降序”只在同一硬资格层内生效 |

### 2.2 重试和健康保护

| 当前实现 | 代码位置 | 与目标的差异 |
| --- | --- | --- |
| `RetryFailoverPolicyV2` 暴露 `maxTotalAttempts`、`maxSameTargetCapacityRetries`、容量等待预算和跨容量域回退 | `src-tauri/src/models/routing_policy.rs` | 用户目标只需要最大重试次数和连续失败阈值；本地容量准入保留，容量域专用字段迁移为历史兼容数据，不再进入生产路由 |
| `ProtectionProfileConfigV2` 暴露 `enabled`、窗口样本数、窗口时长、最小样本、失败率和 Half-Open 成功次数 | `src-tauri/src/models/routing_policy.rs`、`LocalRoutingSettingsEditor.tsx` | 当前是独立错误率保护；用户要求改成始终存在的熔断器，设置只保留恢复等待时间和恢复成功阈值 |
| `HealthProtectionReducer` 使用滑动窗口样本数和失败率达到阈值来 Open | `src-tauri/src/application/health_protection.rs` | 目标是 Key 级连续失败阈值，不依赖可调错误率参数 |
| `ErrorRateProtectionService` 在应用组合中默认 disabled | `src-tauri/src/application/app_services.rs`、`error_rate_protection.rs` | 保护关闭时会出现“请求一直失败但不跳 Key”的风险 |
| Half-Open 真实 outbound probe 当前主要是 Credential scope，其他 scope 没有同等生产 resolver | `src-tauri/src/services/proxy/execution.rs`、`health_protection.rs` | 目标首版明确为 Key scope；需要统一 lease 和真实请求结果的状态机 |

### 2.3 可靠性和样本

| 当前实现 | 代码位置 | 与目标的差异 |
| --- | --- | --- |
| 真实请求通常以 `evidence_mass_basis_points = 10000` 写入，监控以 `5000` 写入 | `src-tauri/src/application/request_finalization/mod.rs`、`application/monitoring/write_path.rs` | 这是固定质量值，不是“先分别计算来源可靠性、再按 70/30 混合”；不同来源的语义混在 `evidence_mass` 中 |
| `quality_projection` 使用 `BetaPrior(alpha=2000,beta=2000)`、24 小时 recent 与 30 天历史窗口，并对 recent/historical 做动态混合 | `src-tauri/src/application/quality_projection.rs` | 需要改为每个来源分别计算、每个窗口按最小样本数门槛选择真实值或乐观值，再按来源权重混合；不得把 Beta 先验当作样本不足保护 |
| `GenericStatus` 映射为 `FailureClass::Uncertain`、`RetryDisposition::StopRequest`、`HealthEffect::Neutral` | `src-tauri/src/application/request_finalization/failure.rs` | 上游 502 若落入该分支，不会影响可靠性和健康 |
| `routing_observation` 对 `HealthEffect::Neutral` 直接返回 `None` | `src-tauri/src/application/request_finalization/mod.rs` | 中性失败没有可靠性样本，因此界面会出现“近 24 小时样本为 0” |
| 监控观测通过 `ObservationSource::ActiveProbe` 写入，真实请求通过 `RealRequest` 写入 | `src-tauri/src/models/routing_observation.rs` | 已有统一观察模型，可以复用；需要补 source weight、failure attribution 和请求去重关联字段 |
| `factors::reliability_posterior` 和 `DispatchAlgorithmProfile.reliability_prior_*` 仍被候选快照兜底使用 | `src-tauri/src/application/routing_engine/factors.rs`、`algorithm_profile.rs`、`operational_facts/planning_snapshot.rs` | 目标改为显式最小样本门槛和可调乐观值；无样本 Key 仍必须生成确定排序值，不再依赖隐式先验 |

### 2.4 设置页

当前 `src/features/routing/LocalRoutingSettingsEditor.tsx` 的页面分组为：

- “评分偏好”：符合目标，保留；在此分组内增加可靠性统计参数（来源权重、历史/最近最小样本数、乐观可靠性和乐观响应时间）；
- “路由边界”：符合目标，保留；
- “候选与探索”：包含最大候选数和探索比例，移除；
- “超时”：字段有统一说明，但每个输入缺少独立含义说明，补齐；
- “错误率保护参数”：包含错误率开关、窗口、最小样本、失败率阈值和 Half-Open 成功次数，替换为“熔断器设置”；
- “会话亲和”：保留；
- “重试与故障转移”：当前是容量路径专用字段，改名为“重试设置”，只展示最大重试次数和连续失败阈值；
- 本地容量准入：保留运行时并发/资源硬门；容量域身份、同域排除、跨域回退和相关等待状态不再参与生产路由，旧代码和数据仅保留迁移/审计参考。

## 3. 目标和非目标

### 3.1 目标

- 任何一次请求都能解释“当前为什么选择这把 Key、失败后为什么换 Key、为什么暂时跳过某 Key”。
- 连续失败的 Key 必须被跨请求隔离，不能因为评分未写回或 Neutral 分类而长期霸占流量。
- 真实请求结果和监控结果进入同一可靠性统计，但按可调来源权重计量。
- 评分不依赖随机探索；长期闲置 Key 按明确的近期/历史公式重新获得排序值，是否被尝试只由当前评分和硬资格决定。
- 熔断状态有明确作用域、冷却、Half-Open 单并发和连续成功恢复语义。
- 保存后的策略有明确版本、迁移、回滚和新旧请求生效边界。

### 3.2 非目标

- 不在本次重构中重新设计价格、倍率、分组、余额、能力和本地容量事实；容量域身份不属于本版本路由决策。
- 不把模型请求自动改成另一个模型；模型映射仍由现有明确配置和调用方模型字段决定。
- 不新增云端共享状态、跨设备熔断同步或黑盒机器学习。
- 不提供随机探索开关、候选 Top K 输入或错误率保护开关。
- 不把客户端参数错误、Relay 本地错误或下游取消伪装成 Key 失败。

## 4. 领域边界和术语

### 4.1 评分、资格、熔断的关系

- `Eligibility`：硬资格，回答“这把 Key 此刻能不能用”。凭据失效、模型不支持、用户禁用、未到期冷却和已取得的硬容量拒绝不能被高分抵消。
- `QualityScore`：软质量，回答“同一硬资格层中哪把 Key 更值得先试”。由可靠性、响应速度、成本和人工偏好组成。
- `CircuitState`：跨请求保护，回答“这把 Key 是否因连续失败暂时跳过”。`Open` 和没有到期的冷却是硬拒绝；`HalfOpen` 只在取得真实请求 lease 后短暂放行。
- `RetryBudget`：单请求预算，回答“这次请求还可以额外发送几次”。它不等同于熔断失败阈值。

### 4.2 默认熔断作用域

首版路由熔断作用域固定为一把 Key：

```text
station_key_id
```

这表示“这把 Key 暂时不要再接收路由请求”。本轮不再按账号、端点、凭据版本或模型拆分生产路由熔断域；这些复杂作用域代码可以保留作后续参考，但不得继续作为独立路由保护器。Key 被替换、删除或重新绑定时，必须通过 Key 标识的生命周期/版本校验避免旧状态误伤新对象。

可靠性统计可以保留更细的诊断维度，但用于本轮路由排序和熔断的主摘要必须能回落到 `station_key_id`。观测中的 endpoint、credential、account、model 字段仅用于诊断和未来迁移，不产生第二套路由熔断。模型不支持、请求参数错误等不属于 Key 故障的结果，不得因此熔断整把 Key。

## 5. 目标路由流程

### 5.1 首次选择

每一轮规划使用一个不可变 `PlanningSnapshot`，依次执行：

1. 解析请求模型、请求形态、重放安全和 deadline。
2. 批量读取候选事实、质量摘要、熔断状态和运行时容量。
3. 过滤硬资格不通过、当前请求已尝试和未到期 Open 的候选；冷却已结束的 Open Key 不直接视为 Closed，而是保留到本层 score gate 计算，通过后才重新加入排序；容量不在评分中扣分。
4. 保持现有 Primary、Backup、Emergency 等硬层级顺序，但明确容量例外：先选择最高层；如果该层所有候选都只因本地容量准入拒绝，才进入下一层。
5. 在当前层内计算每个候选的最终有效分数。
6. 按包含既有亲和修正的 `effective_score` 从高到低排序；相同最终分数才按稳定的 `station_key_id` 顺序打破平局。无论分数来自 observed、optimistic 还是因部分因子不可用而按可用因子归一化得到的 fallback，只要是有限分数都进入同一个比较器；`score_status`/`quality_basis` 只能用于诊断，不能把 fallback 候选整体排到所有 scored 候选之后。只有完全没有可计算分数的候选才按 `station_key_id` 稳定兜底。亲和不得绕过硬资格、熔断、容量或既有逃逸规则。
7. 按排序顺序逐个申请容量准入；高分 Key 容量足够就继续承载请求，只有容量明确不足/拒绝时才跳到下一把 Key。
8. 依次尝试已通过容量准入的候选，直到成功、重试预算耗尽、deadline 耗尽或没有可执行候选。

严格禁止：

- 使用 seed 在近优候选之间随机选择；
- 因为探索比例把低分候选插入当前序列；
- 让低层级候选越过仍可执行的高层级候选；
- 因为候选数组顺序变化而改变同一快照的排序；
- 在一次请求内无限重复同一 `routing_identity`。

`station_key_id` 是请求内排除和同 Key 去重的唯一身份。一个 Key 的模型 variant、endpoint variant 或 routing identity 不能绕过该排除。容量准入拒绝不消耗 outbound retry；如果当前层所有候选都容量不足，才按上述规则尝试下一层，所有层都容量不足时返回现有公共错误 `route_capacity_exhausted`，并以内部诊断 `capacity_exhausted` 标记。

本地容量 registry/read 或 lease CAS 不可用时必须 fail-closed：不得把未知容量当作可用，不创建 outbound attempt，返回现有公共错误 `route_capacity_exhausted`，并记录 `capacity_state_unavailable` 诊断。

出站边界前由 Relay 自己产生的连接建立、DNS、序列化或适配器错误，按本地终态结束当前请求：不计 Key 可靠性、不计 circuit、不得自动换另一把 Key，也不消耗 outbound retry。只有有证据证明请求已跨过 outbound boundary 的连接/传输失败，才进入 Key 级失败和普通重试路径。`Retry-After` 如需保留，只能记录经过范围限制的数值诊断，不得保存原始 header，也不得改变候选、预算或冷却。

候选数量仍受代码拥有的 `MAX_OPERATIONAL_CANDIDATES`（当前为 `1024`）保护，但该上限不是用户设置，也不能静默截断候选。`candidate_cap_count` 固定指通过当前模型/请求形态/协议能力、已启用站点、有效凭据和 Key 生命周期资格的候选数，用户禁用、circuit、request exclusion 和本地容量在 cap 统计后处理；该值也作为终态分类的候选基数。snapshot 在完成该完整评估后若发现候选总数超过上限，必须在任何 outbound attempt 前返回 typed `route_candidate_limit_exceeded`；该错误使用 HTTP `503`、`error.type=service_unavailable`、`error.code=route_candidate_limit_exceeded`，不申请容量 lease、不消耗 retry，也不再进入重试/故障转移。只有未超过上限时才继续排序、准入和发送。

### 5.2 会话亲和

会话亲和保持当前开关、TTL、有界 bonus、hysteresis 和逃逸规则；本次只移除随机探索和 rendezvous，不改变亲和修正的既有数值语义。亲和修正后的分数仍必须参与确定性降序排序。MUST NOT：

- 绕过硬资格、Open 冷却、Half-Open lease 或容量检查；
- 让亲和 bonus 越过现有 hysteresis/逃逸规则，把明显劣化的 Key 长期置于正常高分候选之前；
- 恢复随机探索。

迁移时保留现有亲和判定、bonus 上限、hysteresis margin、TTL 和逃逸条件；在 trace 中继续标记 `affinity_applied`，并增加亲和修正前后分数。不得把亲和候选强行缩成单候选，也不得以同分 tie-break 替代现有语义。

### 5.3 失败后的重新规划

每次 outbound attempt 结束后，必须先写入本次 attempt 的结果，再根据最新 runtime/durable revision 获取新的规划快照并重新规划。当前请求已经取得 candidate admission 的 Key 默认加入 `request_excluded_keys`，避免同一请求在准入后本地失败或已真实失败时反复使用同一把 Key；未完成准入的容量拒绝不加入。

候选准入的提交点必须可审计：只有本地容量 lease、circuit/Half-Open CAS 和 attempt slot 身份已经在同一事务或等价 durable outbox 中成功保存后，才写入 `candidate_admitted`。该 CAS 必须校验 candidate 的 Key lifecycle、容量 revision 和 circuit 状态（Closed，或成功占用 Half-Open lease）；事务提交后即视为已准入，不得再用一次状态重读把它降级为“未准入”。generation fence 只阻止尚未写入该事件的新准入；已提交准入的请求可以继续完成并持有 Half-Open lease 到结果或请求 `deadline_at`。未提交准入的竞争、取消或 fence 等待只释放临时资源，不创建 outbound attempt；准入后、跨边界前的取消、deadline、目标删除或本地连接/适配器错误必须写入 `local_abandoned` 终态并释放资源，不生成 Key 质量样本或 circuit event；Half-Open lease 的 `boundary_crossed` 必须由 attempt owner 在真正跨 outbound boundary 前原子标记。

本地容量准入只回答“当前 Key 现在能不能接这个请求”，不参与质量评分或负载扣分；容量不足时按既定评分顺序继续向后尝试。上游 `429` 和其他已经跨 outbound boundary 的可归责错误都按当前 Key 级失败进入可靠性和熔断计数；本版本不再做同容量域等待、同域 Key 排除或跨容量域 fallback。截图中的容量域身份字段不再是生产路由的输入。

## 6. 评分模型

### 6.1 评分组成

评分偏好四项保持不变：

- 可靠性；
- 响应速度；
- 成本效率；
- 人工偏好。

用户权重仍使用 basis points，和必须为 `10_000`。评分只在同一可比较层级内排序；健康、能力、余额、倍率上限、熔断和容量不是可以用权重抵消的软因子。

```text
effective_score =
    reliability_weight    * reliability_score
  + responsiveness_weight * responsiveness_score
  + cost_weight           * cost_score
  + preference_weight     * preference_score
```

四项因子先统一归一化到 `[0, 10_000]`，用户权重使用 basis points，基础分数为加权和除以 `10_000`；所有中间值和比较使用版本化 fixed-point。会话亲和继续沿用当前智能路由规范中的有界 bonus、hysteresis 和逃逸规则，作为基础分数之后的既有 dispatch 修正；本次不把它改成“仅同分 tie-break”，也不新增第二套亲和算法。最终排序使用现有亲和修正后的 `effective_score`；当质量因子缺失而生成 fallback score 时，也使用同一亲和修正。本版本不再把实时负载或 runtime anomaly 作为分数惩罚；负载只通过后置本地容量准入决定是否跳到下一候选。`weighted_rendezvous`、utility band 和 exploration lane 从生产选择路径移除。

### 6.2 样本门槛和乐观值

本版本不再把 `alpha/beta` 先验混入可靠性分数。取而代之的是两个显式、可解释的窗口门槛：

| 参数 | 默认值 | 说明 |
| --- | ---: | --- |
| `historicalMinimumSamples` | `15` | 对每个指标，历史窗口至少有这么多该指标的有效样本后，才使用历史真实可靠性或响应时间 |
| `recentMinimumSamples` | `5` | 对每个指标，最近 24 小时至少有这么多该指标的有效样本后，才使用最近真实可靠性或响应时间 |
| `optimisticReliabilityPercent` | `95` | 对应窗口样本不足时使用的可靠性排序值 |
| `optimisticLatencyMs` | `2500` | 对应窗口样本不足时使用的响应时间排序值 |
| `recentTimeDecayHalfLife` | `72 小时`（系统算法常量，不在设置页暴露） | 最近窗口内样本的时间衰减半衰期；修改必须提升算法版本并重建质量摘要 |
| `historicalTimeDecayHalfLife` | `24 小时`（系统算法常量，不在设置页暴露） | 超过最近 24 小时后的历史样本时间衰减半衰期；修改必须提升算法版本并重建质量摘要 |
| `historicalRetentionWindow` | `30 天`（系统数据保留边界，不在设置页暴露） | 历史窗口只读取最近 30 天内、且早于最近 24 小时的数据 |

规则：

1. 对 `RealRequest` 和 `ActiveProbe` 两个来源分别计算可靠性；每个来源都先按去重后的有效请求样本拆分最近窗口和历史窗口。一次 projection 先固定唯一的 `evaluation_at`（UTC 毫秒），同一 projection 的所有边界、年龄、衰减和 `c` 都使用这个值。
2. 最近窗口固定为 `[evaluation_at - 24h, evaluation_at]`；历史窗口固定为 `[evaluation_at - 30d, evaluation_at - 24h)`，两者不得重叠。超出 30 天保留边界的数据不参与评分，但可以保留作审计。
3. 对每个来源分别计算去重后的近期样本数 `n`，近期权重固定为：

   ```text
   c = min(0.9, n / (n + 20))
   ```

`n` 是该来源最近 24 小时内的**独立有效请求数**（最终 canonical outcome 为 `Success` 或 `AttributableFailure`，该来源的 outbound attempt 已跨 outbound boundary，且 `event_time_status=Valid`；`Excluded` 和缺少有效事件时间的 outcome 不计入），不是 outbound attempt 数，也不是未经去重的观测条数。
4. 每条已去重的独立请求样本必须先计算时间衰减权重。令 `t_i` 为该样本 canonical outcome 的事件时间，并令 `a_i = max(0, (evaluation_at - t_i) / 1 小时)`，即该请求距本次评估时刻的小时数。时间衰减函数固定为（`a_i` 的单位是小时）：

   ```text
   w(a_i) = 2 ^ (-a_i / 72),                         0 <= a_i <= 24
   w(a_i) = 2 ^ (-24 / 72) * 2 ^ (-(a_i - 24) / 24),  a_i > 24
   ```

   其中 `w_i = w(a_i)`。`a_i = 0` 时 `w_i = 1`；`a_i = 24` 时 `w_i = 2^(-24/72) ≈ 0.7937`；`a_i > 24` 后，每增加 24 小时再衰减为原来的一半。两段函数在 24 小时边界连续，`t_i = evaluation_at - 24h` 属于近期窗口，历史窗口从严格早于该边界的样本开始。`w_i` 只表示时间衰减，不包含 70/30 来源权重，也不包含重试次数权重。

   本文公式直接使用 `w_i`。实现固定每个独立请求 `base_mass_i = 1`，并以 `effective_weight_i = round_half_up(weight_scale * w_i)`（`weight_scale=1_000_000`）参与比例计算；不得继续使用旧 `evidence_mass` 作为来源差异，也不得把来源权重再次乘进单条样本。

   `a_i` 由整数时间戳计算，后端使用版本化 fixed-point `exp2` helper 计算 `w_i`，再按固定的 half-up 规则量化为 `weight_scale = 1_000_000` 的整数权重。golden vector 的量化期望值为 `a=0 -> 1,000,000`、`a=24 -> 793,701`、`a=48 -> 396,850`、`a=72 -> 198,425`；实现误差不得通过更换舍入模式掩盖。评分和样本质量只消费该整数权重；最终分数相同才使用稳定 Key 标识打破平局。前端不得自行用浮点数重算。任何算法 helper、量化精度或舍入规则变化都必须提升算法版本并重建质量摘要。

5. 先将设置中的百分比换算为比例：`optimisticReliability = optimisticReliabilityPercent / 100`（默认 `0.95`）。某个来源的近期样本数低于 `recentMinimumSamples` 时，将这个比例代入该来源的 `R_recent`；达到门槛后才使用近期真实可靠性。历史样本数低于 `historicalMinimumSamples` 时，将同一个乐观比例代入 `R_history`；达到门槛后才使用历史真实可靠性。
6. 每个来源的可靠性严格按以下公式合成：

   ```text
   R_recent(source) = sum(w_i * s_i) / sum(w_i)       # n >= recentMinimumSamples
   R_history(source) = sum(w_i * s_i) / sum(w_i)      # m >= historicalMinimumSamples
   R_recent(source) = optimisticReliability           # n < recentMinimumSamples
   R_history(source) = optimisticReliability          # m < historicalMinimumSamples
   R_source = c * R_recent(source) + (1 - c) * R_history(source)
   ```

其中 `s_i=1` 表示该独立请求成功，`s_i=0` 表示失败，`w_i` 是单条请求的时间衰减权重；`n` 是近期窗口独立有效请求数，`m` 是历史窗口独立有效请求数。只有 `event_time_status=Valid` 的去重样本才能进入窗口和分母；达到门槛时分母必定大于 0。
7. 最后才按来源权重混合：

   ```text
   R = effectiveRealTrafficWeight * R_source(RealRequest)
     + effectiveMonitoringWeight * R_source(ActiveProbe)
   ```

   默认配置为 `realTrafficWeight=0.7`、`monitoringWeight=0.3`。`effectiveRealTrafficWeight`/`effectiveMonitoringWeight` 由第 7.2 节的可比性规则得到；来源权重只在这里应用一次，不根据样本数量再次动态调整。
8. 响应速度也必须使用相同的去重样本口径、窗口边界、`w_i`、`c` 和最小样本门槛，但首版只使用可比的真实路由成功样本。令 `l_i` 为该独立请求的有效响应时间（毫秒，必须是有限且非负数）：非流式请求使用完整响应时间，流式请求使用首字节时间（TTFT）。则计算：

   ```text
   L_recent = sum(w_i * l_i) / sum(w_i)               # n_latency >= recentMinimumSamples
   L_history = sum(w_i * l_i) / sum(w_i)              # m_latency >= historicalMinimumSamples
   L_recent = optimisticLatencyMs                     # n_latency < recentMinimumSamples
   L_history = optimisticLatencyMs                    # m_latency < historicalMinimumSamples
   L_real = c_latency * L_recent
          + (1 - c_latency) * L_history
   c_latency = min(0.9, n_latency / (n_latency + 20))
   ```

   `n_latency`/`m_latency` 是对应窗口内去重后拥有有效 `l_i` 且请求成功完成的独立真实路由请求数；它们可以少于可靠性样本数 `n`/`m`。失败、取消、缺失、负数或非有限延迟只从响应速度样本中排除，但失败仍可作为可靠性样本。`L_real` 聚合完成后，才转换为现有的 `responsiveness_score`（延迟越低，速度分越高）；不能先把每条延迟转换成分数再做窗口合成。转换使用代码常量 `RESPONSIVENESS_SCORE_CAP_MS=120_000`：先将 `L_real` 按 `latency_scale=1_000` half-up 量化为 `q_ms`，再计算 `responsiveness_score = floor(10_000 * (120_000 - min(q_ms, 120_000)) / 120_000)`，结果 clamp 到 `[0,10_000]`。监控延迟首版只作诊断，不进入响应速度评分；70/30 可调权重只作用于可靠性来源混合。
9. 没有任何样本的 Key 也必须得到确定排序值，不得返回空值、`0/0` 或依赖候选数组顺序。乐观值只是排序输入，不写入观测、失败计数或熔断状态，也不能绕过硬资格、容量或冷却规则。

例如：Key A 没有样本，Key B 有足够样本且真实可靠性为 80%。若当前乐观可靠性为 95%，A 在其对应来源窗口中使用 95% 参与确定性排序；A 后续真实请求失败后，失败会写入观测并按去重样本和门槛重新计算，不能继续永久占据高位。

`reliability_prior_alpha`、`reliability_prior_beta` 不再有生产消费者；它们只能留在历史迁移/审计中，不能继续出现在新的算法 profile 和决策解释里。

### 6.3 24 小时无真实路由样本

“24 小时无样本”定义为：该 Key 在滚动 24 小时内没有新的**去重后真实路由可靠性样本**。主动监控样本可以参与监控来源的质量计算，但不会清除这个真实路由闲置标记。

闲置状态不触发额外请求，也不覆盖第 6.2 节的近期/历史公式：实际路由来源的近期窗口样本数为 `n=0`，因此 `c=0`，最终由历史窗口值主导；如果历史窗口也未达到 `historicalMinimumSamples`，则历史值使用乐观值。状态页仍标记 `idle_real_route_sample`，用于解释“为什么最近路由样本为 0”，但不把乐观值误报成真实成功率。若历史样本充分且真实分数长期很高，不需要为了“轮换”而额外探测该 Key。

Open 冷却、凭据阻断、能力不支持、用户禁用和容量不足优先级高于任何乐观值。Key 只有在正常请求按评分顺序轮到它时，才会自然获得新的真实路由样本。

### 6.4 响应速度计算

`optimisticLatencyMs` 用于最近/历史窗口样本不足时的响应时间输入，默认 `2_500ms`。响应速度严格使用第 6.2 节定义的 `w(t)`：每条成功真实请求的有效延迟先按时间衰减加权，分别得到 `L_recent` 和 `L_history`，再用 `c_latency = min(0.9, n_latency/(n_latency+20))` 合成 `L_real`。因此这里的响应时间是加权平均毫秒值，不是未经定义的“延迟摘要”或自动 p95；后续若要改为 p95/分位数，必须另立算法版本和公式。失败延迟可以写入原始观测和诊断，但不能进入 `responsiveness_score`。监控延迟首版同样只作诊断。长期保持高分的 Key 不需要被额外轮换，低分 Key 也不保证获得独立探测请求。`last_real_route_sample_at` 只接受具有有效 `event_at` 的真实路由 outcome；若最近事件缺少有效事件时间，状态页将 `idle_real_route_sample` 标为 `unknown`，不得把它误判为 24 小时内有样本或 24 小时闲置。

## 7. 可靠性观测和样本口径

### 7.1 统一观测

继续复用 `RoutingObservation`，但必须补齐或等价表达以下字段：

```text
RoutingObservation
├─ observation_id
├─ event_id / attempt_id
├─ correlation_id (required; request_id may remain diagnostic only)
├─ station_key_id
├─ station_key_lifecycle_revision
├─ endpoint_revision (diagnostic only; not a circuit key)
├─ credential_revision (diagnostic only; not a circuit key)
├─ model_class
├─ endpoint_shape / protocol / request_shape
├─ comparability_key
├─ source: RealRequest | ActiveProbe | Administrative
├─ attempt_index
├─ candidate_admitted / candidate_admitted_at
├─ capacity_lease_id / half_open_lease_id
├─ boundary_crossed
├─ response_origin: Upstream | Relay | Unknown
├─ event_time_status: Valid | Missing | Invalid
├─ outcome: Success | AttributableFailure | Excluded
├─ failure_code
├─ failure_attribution: Key | Local | Client | Unknown
├─ latency / ttft / throughput
├─ cluster_finalized
├─ cluster_expected_attempt_count
├─ cluster_finalized_at
├─ cluster_finalization_reason
├─ event_at
├─ algorithm_version
├─ source_weight_revision
├─ quality_policy_revision
├─ generation_eligibility: Active | Next | Legacy
├─ observed_at
└─ ingested_at
```

观测是原始证据，不直接等于熔断状态。质量 Projector 和 Key Circuit Reducer 消费同一条已分类观测，禁止各消费者重新猜测 HTTP 状态含义。`event_at` 必须由 outbound/monitor adapter 提供；缺失或非法时写入 `event_time_status=Missing|Invalid`，不得使用 `observed_at` 或 `ingested_at` 替代。该 outcome 仍可驱动 RealRequest 的 retry/circuit，但在有效事件时间修复前不得进入质量窗口或样本分母。`generation_eligibility` 是切换期的摄取标记，不是路由或质量算法字段：正常写入当前 active generation 的事件标记为 `Active`；v3 尚无 active generation 的 shadow/building 阶段，或 generation fence 冻结下一代输入水位后产生、只能留给下一代重建的事件标记为 `Next`；迁移来的旧事件标记为 `Legacy`，不得进入当前 v3 质量分母。`Next` 事件可以写入 immutable observation，但不得被当前 active projector/reducer 消费；下一代建立输入水位时才能纳入。`station_key_lifecycle_revision` 是 Key 删除、替换或重新绑定时递增的生命周期隔离条件；旧 revision 的观测只能保留审计，不能计入新对象的质量或 circuit。旧的 Endpoint/Account/Model 归因字段可以保留在审计兼容层，但不再参与本轮生产路由。

来源对状态效果的边界固定为：`RealRequest` 的可归责结果才生成 Key circuit event、推进连续失败或 Half-Open 恢复，以及驱动请求级 retry；`ActiveProbe` 只作为可比质量/诊断来源，不直接改变 Key circuit、不触发 request retry，也不加入 request exclusion。Half-Open 恢复必须来自带 lease revision 的真实路由请求。来源权重仍只作用于可靠性最终混合。

### 7.2 来源权重

新增版本化可靠性样本来源权重：

```text
real_traffic_weight_percent = 70
monitoring_weight_percent   = 30
real_traffic_weight_percent + monitoring_weight_percent = 100
```

默认值为 70/30，设置页允许调整两个值，但后端必须校验和为 100。前端可以编辑其中一个并自动计算另一个，不能在保存前静默改变后端值。

两类来源必须先独立计算，再做一次来源混合：

```text
source_mass(source, observation)
  = base_mass * w_i

source_reliability(source)
  = source_success_mass / source_total_mass

reliability
  = (effective_real_traffic_weight_bps * real_traffic_reliability
  + effective_monitoring_weight_bps   * monitoring_reliability) / 10_000
```

配置中的百分比在计算时转换为 basis points（70% = 7_000）；两项来源权重只能在最终来源混合处使用一次。

来源可比性是混合的前置条件：`ActiveProbe` 只有在模型族、endpoint 形态、协议和请求形态与实际路由样本一致，并且观测中有可验证的 comparability key 时，才是 `eligible` 来源。不可比的监控样本只能进入监控诊断，不能混入实际路由可靠性。若某来源不可比，则从本次混合中排除并把剩余 eligible 来源的配置权重重新归一化；若来源可比但暂时没有样本，仍保留其配置权重并按第 6.2 节使用乐观值。该“不可比”和“样本不足”必须在 quality basis 中区分。

`effective_*_weight_bps` 定义为：先把不可比来源的配置权重置零，再将剩余且配置权重大于 0 的 eligible 来源按比例归一化到总和 `10_000`。没有任何正权重的 eligible 来源时，不生成可靠性分数并返回 `quality_unavailable` 诊断；正常路由仍由硬资格和熔断规则决定。

来源权重不能提前乘到每条观测再在最终结果中重复使用。完成请求去重后，单条独立请求样本的有效质量只由观测本身和时间衰减决定：

```text
effective_mass = base_mass * w_i
```

监控样本不再通过固定 `5000` 质量值隐式表达，真实请求也不再默认无条件 `10000`；两者都携带来源权重版本并可在质量详情中解释。质量详情必须同时显示两类来源的独立可靠性、独立样本数、可比性状态和最终混合结果。

### 7.3 哪些真实请求结果计入可靠性

下表是 Key 可靠性和熔断的默认归责规则：

| 结果 | 可靠性样本 | 连续失败计数 | 请求级重试 |
| --- | --- | --- | --- |
| 上游成功并完成 | 成功 | 清零 | 结束 |
| 上游 `429` / rate limit | 失败 | 加一 | 安全时重试 |
| 已跨 outbound boundary 的上游 `408`、首字节超时、连接失败 | 失败 | 加一 | 安全时重试 |
| 上游 `500..599`，包括明确上游 `502` | 失败 | 加一 | 安全时重试 |
| 上游响应无法进一步确认语义，但已确认是选中目标返回的错误 | 失败，`failure_code=upstream_uncertain` | 加一 | 按通用可重试规则 |
| `401/403` 等凭据拒绝 | 失败 | 加一；必要时同时记录凭据阻断 | 不重试 |
| 模型不支持、能力不匹配 | 不计入 Key 可靠性；仅写能力诊断 | 不计入 | 不重试该目标 |
| 客户端 `400/422`、请求参数错误 | 不计入 | 不计入 | 不重试 |
| 下游在发送前取消 | 不计入 | 不计入 | 结束 |
| Relay 本地持久化/适配器错误，未能证明上游收到请求 | 不计入 Key 可靠性 | 不计入 | 按本地错误规则结束 |
| 本地容量准入拒绝，尚未发送 outbound 请求 | 不计入 | 不计入 | 按评分顺序继续尝试下一把 Key，不消耗 outbound retry |
| 已开始输出后的上游流中断 | 失败 | 加一 | 不普通重放已提交请求 |

“所有请求错误都计入”在本规范中具体指：所有已经跨过选中目标 outbound 边界且能归责到目标的真实结果都计入；与 Key 无关的客户端、取消和 Relay 本地错误不能污染 Key 评分。

### 7.4 502 的强制收口

只要 HTTP `502` 是由上游目标返回，或传输证据确认请求已到达目标并收到 `502`，canonical classifier MUST 生成：

```text
failure_class = Upstream5xx | UpstreamUncertain
failure_attribution = Key（路由归责）；endpoint 等细节仅作诊断字段
health_effect = ObserveFailure
reliability_effect = FailureSample
retry_disposition = RetryableBeforeCommit
```

只有 Relay 自己生成的本地 502，或根本没有跨出站边界的本地错误，才可以不写入 Key 可靠性。

`GenericStatus` 不能继续默认映射到 `Neutral + StopRequest`。如果语义确实未知，也要把“已选目标收到上游错误”的失败事实和“无法确定具体故障域”的不确定性分开表达；不确定作用域不等于没有失败样本。

### 7.5 相关重试的统计

同一个请求的多次 fallback attempt 必须：

- 每次候选完成本地容量与 circuit admission、即将跨 outbound boundary 时，在 outbound boundary 前创建连续的 `attempt_index`/`attempt_id` slot；容量准入拒绝、Half-Open CAS 竞争失败和未完成 admission 不创建 slot，也不占该索引；
- 每个已创建 attempt 都保留 attempt ledger/审计观测；只有已跨 outbound boundary 且可归责给 Key 的 canonical outcome 才生成 circuit event 并改变连续失败计数。准入后但未跨边界的 `local_abandoned` 只用于生命周期终结和审计，不进入质量分母或 circuit streak；
- 质量投影只读取当前 `station_key_lifecycle_revision` 的观测；Key 被删除、替换或重新绑定后的旧 revision 只保留审计；
- 可靠性样本按 `source + station_key_id + correlation_id` 去重。同一请求在同一 Key 上的重复观测只能贡献一个独立请求样本；优先采用该请求在该 Key 上的最终 canonical outcome，无法确定最终结果时采用最先跨 outbound boundary 的结果；
- `n` 和 `m` 只统计去重后、`event_time_status=Valid` 的独立有效请求数，不能用 raw attempt 数量替代；raw attempt 仍用于故障审计、连续失败熔断和诊断展示；
- 去重不隐藏失败事实。UI 应同时显示 raw attempt count、deduplicated request sample count 和加权可靠性结果。

主动监控的内部重试同样共享一个 correlation cluster，不能因为一次监控执行的内部重试放大质量。不同来源（真实路由、主动监控）分别去重，不能相互覆盖。`cluster_expected_attempt_count` 由 request/probe lifecycle owner 在 durable lifecycle 提交终态时从 attempt ledger 写入，表示该 correlation 在候选准入后创建的全部 attempt slot 数量；容量准入拒绝和未创建 slot 的本地规划循环不计入，已创建但在 outbound boundary 前取消/超时的 slot 必须写入 `local_abandoned` terminal outcome 并计入 expected count。cluster 只有在 durable request/probe lifecycle 已提交终态、存在 `0..cluster_expected_attempt_count-1` 的完整 terminal outcome 集合时，才由幂等 finalizer 写入 `cluster_finalized=true`、完成时间和原因；不能根据时间或当前已见行数猜测完成。未 finalized 的 cluster 只能产生 provisional 质量摘要，进程恢复仍按同一终态规则补齐。已 finalized 时先按最大 `attempt_index` 选择 canonical outcome，不因事件时间有效性跳过最终结果；若该结果的 `event_time_status` 不是 `Valid`，cluster 可以 finalized 但必须标记 `event_time_missing|invalid` 并排除该质量样本；不能以写入时间替代。

### 7.6 可靠性和响应速度计算

可靠性不含 Beta 先验。每个来源、每个窗口先执行第 6.2 节的最小样本门槛和去重规则：

```text
reliability_value(source, window) =
  observed_reliability(source, window),            if valid_sample_count >= window_minimum_samples
  optimisticReliabilityPercent / 100,               otherwise

latency_value(RealRequest, window) =
  weighted_latency(RealRequest, window),             if valid_latency_sample_count >= window_minimum_samples
  optimisticLatencyMs,                              otherwise

source_reliability(source) = blend(recent_value(source), historical_value(source))
reliability =
  real_traffic_weight * source_reliability(RealRequest)
  + monitoring_weight * source_reliability(ActiveProbe)
```

`observed_reliability` 的可靠性为该来源窗口内 `sum(w_i * s_i) / sum(w_i)`；`weighted_latency` 为同一窗口内 `sum(w_i * l_i) / sum(w_i)`，其中 `l_i` 是有效端到端延迟。可靠性使用 `n/m` 和 `c=min(0.9,n/(n+20))`，响应速度使用 `n_latency/m_latency` 和 `c_latency=min(0.9,n_latency/(n_latency+20))`；两者都先应用各自窗口的最小样本门槛，再做近期/历史合成。最近窗口和历史窗口不重叠，窗口权重不得偷偷变成来源权重。

当某个来源窗口没有达到门槛时，直接使用用户设置的乐观值，不返回 `Unknown`，也不得计算接近 `0/0` 的伪比例。只有在内部诊断中才可记录 `insufficient_samples`。

质量摘要必须保留：

- raw observation count；
- 每个来源的有效样本数、有效质量、成功/失败质量；
- 每个来源的 recent/history count 和 mass，以及是否达到对应门槛；真实路由成功延迟的 `n_latency/m_latency`、`L_recent/L_history/L_real`；监控延迟只保留诊断摘要；
- `last_real_route_sample_at` 和 `last_monitoring_sample_at`；
- source weight revision；
- 每个来源/窗口的 quality basis（`Observed`、`OptimisticInsufficientSamples`）；另记录 Key 级 `idle_real_route_sample`（`true|false|unknown`，缺少有效 `event_at` 时为 `unknown`）；
- 去重规则、被合并的 observation ID 和最终采用的 canonical outcome。

## 8. 熔断器状态机

### 8.1 状态

每个 Key circuit scope 使用：

```text
Closed(state_revision, consecutive_failures, reopen_level, policy_revision)
Open(state_revision, cooldown_until, consecutive_failures, reopen_level, policy_revision)
HalfOpen(state_revision, lease, recovery_successes, reopen_level, policy_revision)
```

状态必须持久化到现有 durable health/reducer 体系，带 `state_revision` 和 CAS/lease fence。Half-Open lease 记录必须同时保存 `attempt_id`、`boundary_crossed`（或等价可原子读取的状态）和 `candidate_admitted`；`boundary_crossed` 只能由 attempt owner 在真正跨 outbound boundary 前原子标记，reaper 不得凭 lease 存在本身猜测已经出站。每个真实 attempt 必须有唯一 `attempt_id/event_id`，reducer 对重复事件幂等；旧事件或已经应用过的事件只能保留审计，不能再次增加失败计数或清零更新的失败 streak。并发结果按该 Key reducer 的线性化提交顺序生效，不按客户端到达时间猜测。进程内容量保护仍是独立 overlay，不能冒充 Key 熔断状态。递增冷却级别也要持久化，避免应用重启后把反复失败的 Key 立即按第一次失败处理。

若单个 Key 的 circuit 持久化读写失败，该 Key 必须立即 fail-closed，当前请求排除它并记录 `circuit_persistence_unavailable`，不得伪造 Open/Closed；持久化的 `persistence_unavailable` gate 只能由明确成功的健康读写检查清除，不能因普通请求到达或进程重启自动清除。若 circuit store 的共享状态读写不可用，所有依赖该 store 的候选都禁止新的 candidate admission；状态读写健康的独立候选仍可继续，若没有则按终态优先级返回 `no_available_key`，并保留有界可重放 backlog。

### 8.2 打开条件

默认连续失败阈值为 `3`，由“重试设置”中的 `consecutiveFailureThreshold` 调整。每一个可归责真实 outbound 失败都使该 Key 的连续失败计数加一；成功把 Closed 状态的连续失败计数清零。

计数跨请求持久化，并按每个可归责 outbound attempt 的 reducer 提交顺序累积（以配置的阈值 `N` 为准）。由于同一请求在某个 Key 失败后会加入 `request_excluded_keys`，单个请求最多为同一 Key 贡献一次 attempt；因此阈值通常由多个请求在该 Key 上的连续失败触发。相关性只限制质量统计的独立样本质量，不能让同一请求通过重复打同一 Key 来放大可靠性或熔断计数。

达到阈值时（首次打开的 `reopen_level` 固定为 `1`，不能保持为 `0`）：

1. 原子地写入 `Open`、`state_revision + 1`、`opened_at`、`cooldown_until` 和非敏感最近失败码。
2. 当前请求继续按重试预算尝试其他可执行候选；不能因为打开保护就报告成功。
3. 后续请求在冷却未到期前硬跳过该 Key。
4. 如果本次请求仍有未尝试候选，但没有任何候选通过硬资格、容量和熔断检查，立即返回 `no_available_key`；不得为了消耗剩余重试次数而再次请求已经失败或已熔断的 Key。若所有候选只是因本请求已取得 admission/跨边界而被 `request_exclusion`，则按终态规则返回最后一个 canonical failure。

### 8.3 冷却和递增退避

新增 `recoveryWaitSeconds`，默认 `30` 秒，建议范围 `5..3600` 秒。它是第一次打开后的基础等待时间；递增倍数是系统内部行为，不在设置页暴露。

对同一 Key，若 Half-Open 失败后再次 Open，则递增冷却级别：

```text
cooldown = min(
  recoveryWaitSeconds * 2^(reopen_level - 1),
  system_max_cooldown_seconds
)
```

第一次 Open 使用 `reopen_level=1`；每次 Half-Open 失败并重新 Open 时级别加一；连续成功达到恢复阈值并回到 Closed 后级别清零。这样反复失败的 Key 会越来越久地退出路由，恢复成功后才恢复正常等待周期。

`429` 不使用特殊的站点级、账号级、端点级或容量域级等待规则，也不因为响应中的 `Retry-After` 延长当前 Key 的熔断冷却；它与其他可归责的单 Key 故障共用本节的基础等待时间和递增冷却。`Retry-After` MAY 保留在脱敏诊断中，但不得改变候选排序、请求重试预算或熔断作用域。

### 8.4 Half-Open 评分门

冷却结束不等于立即发送探测。每次规划时：

1. 先计算当前快照中该 Open Key 所属 Primary/Backup/Emergency 硬层内、且通过硬资格的 `Closed` 候选最终分数；容量在后续准入阶段判断，不提前改写分数。不同硬层的分数不可比较。
2. 令该硬层的 `best_closed_score` 为这些候选中的最高分；如果该硬层没有任何 `Closed` 候选，视为无比较基线。
3. 冷却结束的 Open Key 只有在以下条件之一满足时才允许申请 Half-Open：
   - 它的最终分数严格高于同一硬层的 `best_closed_score`；或
   - 当前该硬层没有任何通过硬资格的 `Closed` 候选。
4. 如果分数不高于同一硬层的 `best_closed_score`，继续保持 `Open(cooldown_elapsed)`，只记录 admission reason `half_open_admission_denied_by_score`，不消费 Half-Open lease，也不制造 synthetic 请求。若质量因子不可用但 planner 能按第 6 节的确定性 fallback 为该 Key 和同层 Closed 基线形成可比较的有限分数，则使用该 fallback 分数进行 gate；只有无法为两者形成可比较分数时才记录 `quality_unavailable` 并等待下一次规划，不能用乐观值冒充不可比较分数。`Open(cooldown_elapsed)` 是 `Open` 的派生显示状态，不是新的持久化状态。

这里比较的不是“上一把实际使用的 Key”，而是**这一次规划快照里同一硬层分数最高的 Closed Key**。例如同属 Primary 层的 A 是 Closed、评分 88；B 冷却结束、评分 92，则 B 可以进入 Half-Open；如果 B 评分 80，则即使上一请求恰好使用了 B，也不能绕过 A 的比较直接探测 B。Backup 层的 Closed Key 不能作为 Primary 层 B 的比较基线，反之亦然。这样每次恢复判断都基于当前完整候选状态，而不是某一次历史请求的偶然选择。通过评分门后仍要按容量准入规则申请真实请求。

这条规则让恢复尝试服从评分目标，同时避免低评分 Key 在有更高分正常 Key 时抢占流量。24 小时无真实路由样本时，近期样本数为 `n=0`、近期权重为 `c=0`，最终按历史窗口值或历史样本不足时的乐观值参与比较；不能绕过凭据、能力、容量和冷却硬门。

必须有单一 supervised lease reaper 按固定周期扫描 `boundary_crossed=true` 且超过 `lease_expires_at` 的 Half-Open lease，提交带原 lease revision 的幂等过期事件；reaper 与请求 finalizer 竞争时由同一 CAS 决定唯一结果。reaper 崩溃或重复运行不得重复递增 `reopen_level`，未跨边界的 lease 只能释放而不能打开 circuit。

### 8.5 Half-Open 单真实请求

同一 `station_key_id` 同时最多一个真实 outbound request：

- lease 必须在目标解析完成、即将跨 outbound boundary 前原子取得，并在同一准入事务中持久化 `attempt_id`、`boundary_crossed=false` 和 `candidate_admitted`；`lease_expires_at` 固定取申请请求的 immutable `deadline_at`，不得超过该 deadline；若 deadline 已过则不申请 lease；
- candidate admission 提交前的取消、deadline、目标删除、generation fence 和 lease race 必须释放临时 lease，不写成功/失败样本；candidate admission 已提交后不能因 generation fence 撤销，必须持有 lease 到结果或 `lease_expires_at`；
- 已跨 outbound boundary 的结果必须带 lease revision，迟到结果不能关闭新一轮 Half-Open；
- 其他并发请求看到 `HalfOpen(lease_in_flight)` 时直接跳过该 Key；
- 不允许为 Half-Open 自动发送 synthetic Provider 请求。
- lease/result 事件必须以 `event_id` 或等价幂等键去重；重复提交同一结果不得重复增加恢复成功数或连续失败数。该幂等要求同样适用于 Closed/Open 下的普通 attempt 结果，不只适用于 Half-Open。

系统常量固定为：`MAX_OPERATIONAL_CANDIDATES=1024`、`QUALITY_PROJECTOR_BATCH_SIZE=256`、`MAX_PROJECTOR_BACKLOG=100_000`、`SYSTEM_CUTOVER_FENCE_TIMEOUT_MS=30_000` 和 `HALF_OPEN_LEASE_REAPER_INTERVAL_MS=5_000`；这些常量不进入 policy v3 或设置页。

### 8.6 恢复和重新打开

新增 `recoverySuccessThreshold`，默认 `2`，建议范围 `1..16`。Half-Open 下：

- 一次来自独立真实路由请求的可归责成功使 `recovery_successes + 1`；同一请求的内部重试不能伪造多个恢复成功；
- 连续成功达到阈值，原子转换为 Closed，清除 cooldown 和连续失败摘要；
- 任一可归责失败立即转换回 Open，连续成功归零，并按递增冷却级别重新开始冷却；
- 429、5xx、超时、连接失败等 Key 级失败都按同一 Key 计数；模型不支持、客户端错误和本地未出站错误不伪装成 Key 恢复成功/失败。
- `429` 与其他可重试的单 Key 故障完全同路径：写入一个失败观测、连续失败计数加一、在重放安全且未提交时消耗一次 `maxRetryCount` 并按最新评分尝试下一把 Key；不得因为 `Retry-After` 改走另一套等待或重试路径。
- 本地容量准入拒绝不增加连续失败、不降低评分；它只表示当前 Key 暂时承载不下，请求按分数顺序继续向后找可接纳的 Key。

## 9. 重试和故障转移

### 9.1 用户参数

新增/替换为：

| 字段 | 中文标签 | 默认值 | 建议范围 | 语义 |
| --- | --- | ---: | ---: | --- |
| `maxRetryCount` | 最大重试次数 | 3 | `0..3` | 第一次发送之外的额外 outbound attempt 次数；总 attempt 上限为 `1 + maxRetryCount`，仍受系统 deadline/硬上限限制 |
| `consecutiveFailureThreshold` | 连续失败阈值 | 3 | `1..10` | 同一 Key circuit scope 连续可归责失败达到该次数后 Open；跨请求持久化 |

`maxRetryCount=0` 表示只发送一次，不是关闭熔断器。失败阈值与重试次数必须是两个独立字段。

### 9.2 可重试错误

默认可重试且尚未提交输出的错误包括：

- 上游返回的 `429` 或 rate limit 错误（本地尚未发送的容量准入拒绝不属于此项）；
- 已确认跨 outbound boundary 的连接/传输失败；
- 首字节前超时；
- 上游 `500..599`，包括上游 `502`；
- 已确认跨 outbound boundary、但具体语义未知的上游错误。

以下情况 MUST NOT 被用户设置强行重试：

- 客户端请求错误；
- 凭据拒绝；
- 能力/模型不支持；
- 已开始输出或上游可能已接受的非幂等请求；
- Relay 本地错误且无法证明重放安全。

出站边界前的本地连接建立、DNS、序列化和适配器错误，除非另有明确的本地 admission 规则，否则直接结束当前请求；它们不自动换 Key、不消耗 `maxRetryCount`，也不写入 Key 失败样本或 circuit。只有确认已跨出站边界的连接/传输失败，才使用上面的 Key 级重试路径。

重试安全仍由现有 `ReplayGate` 唯一负责；本规范不允许按 HTTP 状态绕过它。

### 9.3 尝试顺序

每次额外尝试默认执行：

```text
记录上一次 attempt
  -> 更新 Key circuit / quality observation
  -> 排除本请求已尝试 Key 和当前仍处于 Open 冷却的 Key
  -> 读取最新 PlanningSnapshot
  -> 按最终有效分数降序选择下一候选
  -> 在准入事务中 CAS 校验 Key lifecycle/circuit/capacity revision，取得容量 lease / Half-Open lease 并创建 attempt slot
  -> outbound attempt
```

同一个请求中不再次尝试刚刚失败的 Key，也不提供 `RetrySameTarget` 旁路。每次额外尝试先从当前快照中尚未尝试且通过熔断/硬资格检查的候选按评分顺序遍历，再对每个候选依序申请容量准入；不能先按容量结果重排评分序列。容量准入成功后才发送；如果所有候选都无法准入，立即按终态规则结束请求。

### 9.4 重试耗尽和终态错误

终态错误按以下优先级判定。planner 必须先计算并复用 `configured_key_count`、`capability_match_count`、`candidate_cap_count`；后两个计数分别表示能力匹配数和通过已启用站点、有效凭据、Key lifecycle 的候选数，均尚未应用用户禁用、circuit、request exclusion 或容量：

1. `candidate_cap_count` 超过 `MAX_OPERATIONAL_CANDIDATES` 时，返回 HTTP `503`、`error.type=service_unavailable`、`error.code=route_candidate_limit_exceeded`，不申请容量 lease、不消耗 retry、不进入故障转移。
2. `configured_key_count=0` 时返回 `no_available_key`；存在 Key 但 `capability_match_count=0` 时返回能力/模型不匹配错误，不伪装成 `no_available_key`。
3. `capability_match_count>0` 但 `candidate_cap_count=0`（全部因站点未启用、凭据失效或 Key lifecycle 无效而被静态资格阻断）时返回 HTTP 503、`error.type=service_unavailable`、`error.code=no_available_key`，不能误报成能力/模型不匹配或容量不足。
4. `candidate_cap_count>0` 且仍存在至少一个未尝试候选，但这些未尝试候选全部因 Open 冷却、Half-Open lease、用户禁用、Key lifecycle 或 circuit 持久化 fail-closed 而不可用，返回 `no_available_key`。新请求面对所有 Key 已熔断时命中此分支。
5. `candidate_cap_count>0` 且所有未尝试候选唯一阻断原因为本地容量准入拒绝，返回现有公共 `route_capacity_exhausted`；该结果不消耗 outbound retry。容量 registry/lease 服务不可用也使用该公共错误，并以诊断区分 `capacity_exhausted` 与 `capacity_state_unavailable`。`capacity_unavailable` 只作为内部分类/诊断，不作为公共 `error.code`。
6. 如果没有未尝试候选，且原因仅是本请求已经取得 admission/跨边界后产生的 `request_exclusion`，或重试次数耗尽、deadline 到期、ReplayGate 拒绝继续重放，返回最后一个安全的 canonical failure；不得为了区分终态再次发送请求。当前请求刚使某 Key Open 也不改变这一条；后续新请求在没有未尝试且可用候选时才命中第 4 条的 `no_available_key`。

所有终态都必须持久化本次 attempt 摘要和最终失败原因，不能把“重试耗尽”写成成功。已经达到阈值的 Key 保持 Open，供后续请求跳过。新增 `no_available_key` 与现有 `route_capacity_exhausted` 的对外契约均为 HTTP `503`、`error.type=service_unavailable`，不得暴露 Key、完整 URL 或内部容量信息。冷却已结束且通过 Half-Open 评分门的 Key 不属于“全部熔断”，可以申请唯一的真实恢复请求。

## 10. 设置页目标结构

### 10.1 保留的分组

1. **评分偏好**
    - 可靠性、响应速度、成本、偏好四项权重保持不变。
    - 增加“可靠性样本来源权重”：实际路由样本 / 监控样本，默认 70% / 30%；说明“先分别计算两类来源可靠性，再按这里的权重混合，只影响评分，不改变熔断连续失败计数”。
    - 增加“历史最小样本数”：默认 `15`；说明“历史窗口有效样本少于此值时，历史可靠性和响应速度使用乐观值”。
    - 增加“最近最小样本数”：默认 `5`；说明“最近 24 小时有效样本少于此值时，最近可靠性和响应速度使用乐观值”。
    - 增加“乐观可靠性”：默认 `95%`；说明“对应窗口样本不足时用于排序的假设可靠性，不写入真实统计”。
    - 增加“乐观响应时间”：默认 `2.5 秒`；说明“对应窗口样本不足时用于排序的假设响应时间，不写入延迟统计”。
2. **路由边界**
   - 倍率上限；
   - 默认分组类型；
   - 保持当前含义和校验。
3. **超时**
   - 连接超时；
   - 首字节超时；
   - 提交前超时；
   - 缓冲执行超时；
   - 流空闲超时。
4. **熔断器设置**
    - **恢复成功阈值**：默认 `2`；说明“Key 进入半开后，需要连续多少个独立真实请求成功才恢复正常”。
    - **恢复等待时间（秒）**：默认 `30`；说明“Key 打开后至少等待多久才有资格进入半开；反复失败时系统会自动递增等待时间”。
    - 不提供启用/禁用开关；熔断器是固定安全机制。
5. **会话亲和**
   - 保持当前开关和 TTL。
6. **重试设置**
    - **最大重试次数**：默认 `3`；说明“首次发送之外最多再发送几次；每次重试都重新按最新评分选择 Key，并受重放安全和总超时限制”。
    - **连续失败阈值**：默认 `3`；说明“同一 Key 的真实 outbound attempt 连续失败达到此次数后打开熔断，计数跨请求保留”。

### 10.2 删除或隐藏的内容

从前端设置页删除：

- “候选与探索”整个分组；
- `maxCandidates` 输入；
- `explorationShareBasisPoints` 输入；
- “错误率保护参数”整个分组；
- `protectionProfile.enabled`；
- `windowMaxSamples`、`windowSeconds`、`minSamples`、`failureThresholdPercent`；
- 中转站编辑页中的“容量域身份”字段和保存/清除入口；容量域的可调参数和技术细节。

后端仍可保留候选数量、本地容量和 transport hard cap 作为系统安全限制，但容量域身份、容量域状态和跨域回退不得被生产 planner、admission 或 execution 读取。相关表、DTO、IPC 和旧实现可以暂留作迁移/审计参考，但不应在中转站编辑页加载或保存；诊断中也不再显示“因容量域跳过”。

### 10.3 每个超时输入的说明文案

每个控件必须有独立的解释文本：

- **连接超时**：建立到中转站网络连接允许等待的最长时间；只有确认请求已跨 outbound boundary 的连接/传输失败才可能进入故障转移，Relay 在出站前产生的本地连接错误会直接结束当前请求。
- **首字节超时**：连接建立后，等待上游开始返回内容的最长时间；超过后视为上游响应异常。
- **提交前超时**：输出提交给客户端前，本次请求允许消耗的总预算；它包含排队、重规划和 outbound attempt。
- **缓冲执行超时**：非流式请求在完整响应返回前允许执行的最长时间。
- **流空闲超时**：流式输出已经开始后，两次输出之间允许的最长静默时间；触发后结束流，不自动重放已提交请求。

保存成功后继续明确“只影响保存后创建的新请求；在途请求沿用原 snapshot”。

## 11. 配置和数据契约

### 11.1 目标公开策略文档

managed JSON envelope 格式保持不变，`policy.version` 升为 `3`。示例：

```json
{
  "formatVersion": 1,
  "baseRevision": 42,
  "policy": {
    "version": 3,
    "reliabilityWeight": 4000,
    "responsivenessWeight": 2500,
    "costWeight": 2000,
    "preferenceWeight": 1500,
    "allowDepletedFallback": false,
    "affinityEnabled": false,
    "affinityTtlSeconds": 300,
    "maxRateMultiplier": null,
    "routingGroupFilter": "all_groups",
    "outboundProxyMode": "inherit",
    "outboundProxyUrl": null,
    "reliabilitySourceWeights": {
      "realTrafficPercent": 70,
      "monitoringPercent": 30
    },
    "reliabilitySampling": {
      "historicalMinimumSamples": 15,
      "recentMinimumSamples": 5,
      "optimisticReliabilityPercent": 95,
      "optimisticLatencyMs": 2500
    },
    "retry": {
      "version": 1,
      "maxRetryCount": 3,
      "consecutiveFailureThreshold": 3
    },
    "circuitBreaker": {
      "version": 1,
      "recoverySuccessThreshold": 2,
      "recoveryWaitSeconds": 30
    },
    "timeoutPolicy": {
      "version": 2,
      "connectSeconds": 10,
      "firstByteSeconds": 30,
      "precommitSeconds": 60,
      "bufferedExecutionSeconds": 300,
      "streamIdleSeconds": 90
    }
  }
}
```

约束：

- `reliabilitySourceWeights` 两项必须是整数百分比，和为 `100`；
- `historicalMinimumSamples` 必须在 `1..10_000`，默认 `15`；
- `recentMinimumSamples` 必须在 `1..10_000`，默认 `5`；
- `optimisticReliabilityPercent` 必须在 `0..100`，默认 `95`；
- `optimisticLatencyMs` 必须在 `100..120_000`，默认 `2_500`；
- `maxRetryCount` 必须在 `0..3`，默认 `3`；总 outbound hard cap 为独立的系统安全上限 `4`，不能由用户字段绕过，也不能把该硬上限误认为用户语义；
- `consecutiveFailureThreshold` 必须在 `1..10`，默认 `3`；
- `recoverySuccessThreshold` 必须在 `1..16`，默认 `2`；
- `recoveryWaitSeconds` 必须在 `5..3600`，默认 `30`，使用秒持久化；
- `timeoutPolicy` 继续使用现有秒字段和约束；
- 新 decoder MUST 拒绝 `maxCandidates`、`explorationShareBasisPoints`、`protectionProfile`、旧容量用户字段和未知字段；
- 旧字段只能在一次性 v2 -> v3 upgrader 中读取，不能继续进入运行时兼容分支。

内部存储约束：`routing_policy_v3_staged.config_json` 只保存 canonical `policy` payload；managed JSON 的 `formatVersion/baseRevision/policy` envelope 由 document/control-plane 边界组装和校验。每条 staged policy 必须有不可变且唯一的 `policy_generation_id`；migration audit 必须以 `(scope, config_revision, target_policy_version)` 唯一幂等，其中 `config_revision` 是源策略 revision，不能依赖自增 `audit_id` 充当幂等键。

### 11.2 迁移映射

| 旧字段 | v3 处理 |
| --- | --- |
| `maxTotalAttempts=4` | `maxRetryCount=3`，保持总 attempt 基线 |
| `maxSameTargetCapacityRetries` | 不再公开、不进入生产路由；保留为迁移审计字段 |
| `capacityRetryWaitBudgetSeconds` | 不再公开、不进入生产路由；保留为迁移审计字段 |
| `allowCrossCapacityDomainFallback` | 不再公开、不进入生产路由；保留为迁移审计字段 |
| `explorationShareBasisPoints` | 丢弃用户语义；记录 migration audit，生产 planner 不再读取 |
| `maxCandidates` | 转为系统内部候选硬上限；不再生成用户可编辑字段 |
| `protectionProfile.enabled` | 忽略开关，熔断器固定启用 |
| `protectionProfile.failureThresholdPercent` | 不可等价转换；使用 v3 默认连续失败阈值 `3`，migration audit 必须说明语义变化 |
| `protectionProfile.halfOpenSuccessesToClose` | `circuitBreaker.recoverySuccessThreshold` |
| `protectionProfile` window/min samples | 删除错误率窗口语义；可靠性统计由 Quality Projector 负责 |
| 旧内部 balanced cooldown 基线 | `circuitBreaker.recoveryWaitSeconds=30`，除非已有明确用户/系统值 |
| `reliability_prior_alpha/beta` | 从 production profile 移除；历史 trace 只读展示为旧算法版本 |
| 无对应旧字段 | `reliabilitySampling.historicalMinimumSamples=15`、`recentMinimumSamples=5`、`optimisticReliabilityPercent=95`、`optimisticLatencyMs=2500` |

迁移必须完整写入 policy history，并注明旧 revision、新 revision、数据语义变化和是否需要重建质量摘要。P8 前旧活动策略无法解析时保持旧 active policy，不启动空策略；这只是 staging 失败时保持现状，P8 激活后不得把 rollback 实现成回到 V2 运行时 planner。

v2 -> v3 转换必须先写入独立的 staged policy/audit 记录，再由 generation coordinator 激活；迁移 audit 的唯一幂等键固定为 `(scope, config_revision, target_policy_version)`，staged 记录以 `(scope, config_revision)` 定位并通过不可变 `policy_generation_id` 关联到唯一 payload，重复运行不能重复生成记录或改变 active policy。P8 之前任何 planner 都不得读取 staged 记录。若 migration 已发布但缺少上述唯一键或 generation 身份，必须使用新的 additive migration 修复，不能修改已发布 migration 文件。

### 11.3 运行时状态迁移

现有 durable health/reducer 表可复用，但事件和状态必须升级到新的 circuit profile：

- 旧滑动窗口失败率状态不能直接解释为连续失败计数；迁移时保留审计，重新以当前最近可归责 attempt 顺序重建连续失败计数；
- 旧 `Open` 冷却可以保留当前 `cooldown_until`，但新状态必须带 v3 `state_revision`；
- 旧 Half-Open lease 不能跨 profile revision 继续使用；应用重启或策略切换时按 revision fence 取消；
- 质量摘要需要由 immutable observations 重建，不能把旧 posterior 结果当成 v3 门槛/乐观值结果；新摘要必须保留每个来源和窗口的门槛命中状态。

代际元数据必须可独立解析：quality projection 和 circuit rebuild 各自持有不可变 generation ID、状态（`building/ready/active/retired/failed`）、输入 watermark、算法/policy revision、content hash 和 checkpoint；`routing_runtime_generation` 作为可保留多代记录的 registry，active 行保存对三者的唯一引用，并以 `status=active` 的 partial unique index/等价约束保证最多一个 active。不能把它实现成只能保存一行的 singleton 表，也不能用当前可变 summary 行或单独的 policy revision 代替 generation 身份，或让 pointer 指向半成品。

代际输出必须物理隔离：`building`/`ready` generation 以及被选为回滚目标的 generation，其 quality summary、pending cluster 和 circuit rebuild state 必须以 `generation_id` 与 Key lifecycle 作为隔离键，不能复用或覆盖 active 的可变 read-model 行。只有当前 `status=active` generation 的唯一 projector/reducer owner 可以接收增量事件；shadow/rebuild 只能写自己的 generation-scoped 输出。`generation_eligibility=Next` 的 observation 可以先写入 immutable evidence，但不得被当前 active projector/reducer 消费，必须留给下一代输入水位；P8 只能通过单事务切换 pointer，不能用原地覆盖摘要或 circuit state 代替切换。

## 12. 代码 owner 和实施边界

实现时沿用现有 owner，不新增第二套路由器：

| 领域 | 目标改动 | 主要 owner |
| --- | --- | --- |
| Policy | `RoutingPolicyConfigV3`、retry/circuit/source-weight 校验、v2 -> v3 upgrader | `src-tauri/src/models/routing_policy.rs`、`application/routing_policy.rs` |
| Planner | 从 utility band/rendezvous/exploration 改为同层有效分数降序列表 | `application/routing_engine/intelligent_planner.rs`、`dispatch.rs` |
| Candidate snapshot | 读取质量 basis、窗口门槛命中状态、`idle_real_route_sample` 和 Key circuit admission；只保留本地容量事实，按排序后准入 | `application/operational_facts/planning_snapshot.rs` |
| Failure classifier | 上游 502/5xx/uncertain failure 统一生成可计入样本的 canonical outcome | `application/request_finalization/failure.rs`、`services/proxy/adapters/*` |
| Public terminal errors | 新增 `no_available_key` 公共错误；保留 `route_capacity_exhausted` 并提供容量原因诊断 | `application/request_finalization/failure.rs`、`application/routing_engine/routing_failure.rs`、`services/proxy/error.rs`、IPC error DTO 和 contract fixtures |
| Observation writer | Neutral 不再吞掉可归责上游失败；写 source/correlation/attribution | `application/request_finalization/mod.rs`、`application/observation_ingestion.rs` |
| Quality | 去除 prior；实现来源独立计算、请求去重、历史/最近最小样本门槛、乐观值、70/30 source weight、24h 真实路由闲置标记 | `application/quality_projection.rs`、`persistence/stores/routing_quality_store.rs` |
| Circuit | Key 级连续失败 Open、递增冷却、Half-Open 单真实请求、连续成功 Close | `application/health_protection.rs`、`persistence/stores/routing_health_verdict_store.rs` |
| Retry | `maxRetryCount` 统一 request budget；保留 ReplayGate 和本地容量内部安全门 | `application/routing_engine/admission.rs`、`services/proxy/execution.rs` |
| Frontend | 新字段、每项说明、loading/error/dirty/conflict 状态；删除 candidate/exploration/error-rate controls | `src/features/routing/LocalRoutingSettingsEditor.tsx`、draft/types/query files |
| Station editor | 移除容量域身份区块及其页面挂载、加载/保存/清除调用；相关历史 API 不进入生产读路径 | `src/features/stations/AddProviderPage.tsx`、`src/features/stations/pages/add-provider/AddProviderSections.tsx`、`src/features/stations/useAddProviderPageController.ts`、`src/lib/api/stations.ts` |
| Diagnostics | 显示 score basis、sample source weights、circuit state、cooldown、Half-Open lease 和 `idle_real_route_sample` | routing status/decision read models |

`error_rate_protection.rs` 不得继续作为一个默认关闭的旁路 breaker。可以重命名/收敛为 circuit adapter，或删除其生产 service wiring；最终只能保留一个 Quality Projector 和一个 Circuit Reducer。

## 13. 可观测性和用户解释

每次请求至少记录以下低基数事件：

- `candidate_ranked`：候选有效分数、tier、quality basis、是否 `idle_real_route_sample`；
- `candidate_selected`：稳定排序位置和非敏感 Key 标识；
- `attempt_started` / `attempt_finished`；
- `failure_classified`：canonical failure、attribution、是否进入质量样本、是否进入连续失败计数；
- `retry_scheduled`：剩余 `maxRetryCount`、是否换 Key、等待原因；
- `circuit_opened`；
- `circuit_skipped`；
- `half_open_admission_denied_by_score`；
- `half_open_probe_started` / `half_open_probe_finished`；
- `circuit_recovered` / `circuit_reopened`；
- `request_finalized`。

用户文案示例：

- “该 Key 返回上游 502，已计入可靠性失败样本，并暂时跳过当前请求。”
- “该 Key 已连续失败 3 次，熔断至 14:32；本次改用下一把可用 Key。”
- “该 Key 冷却已结束，但当前评分不高于本次快照同一硬层里最高分的正常 Key，暂不放行恢复请求。”
- “该 Key 已进入半开，仅允许一个真实请求验证恢复。”
- “该 Key 连续成功达到 2 次，已恢复正常。”
- “该 Key 24 小时没有真实路由样本；近期权重为 0，当前按历史窗口或历史样本不足时的乐观值计算。”

不得显示完整 API key、Authorization、完整 URL、请求正文、原始错误响应或完整 seed。

## 14. 实施阶段

### Phase 0：冻结语义和基线

- 为 502、429、timeout、client error、local error、cancel 建立 canonical outcome matrix。
- 冻结 v3 字段、默认值、范围、总 attempt hard cap 和 circuit scope。
- 建立现状 fixture：证明当前 502 Neutral 不入样本、error-rate 默认关闭、rendezvous/exploration 会改变顺序。

### Phase 1：质量和观测收口

- 先让真实可归责 attempt 全部进入 `RoutingObservation`。
- 修复 GenericStatus/502 分类和 Neutral 丢样本路径。
- 实现来源独立计算、请求去重、历史/最近最小样本门槛、乐观值、source weights 和真实路由闲置标记。
- 给质量摘要补 golden vectors 和重建测试。
- 由单一 supervised projector/retention worker 保证 raw observation 至少覆盖 30 天历史窗口；仍被 active、building、ready 或已被选为回滚目标的 generation watermark 引用的数据不得清理，清理必须有界且保留脱敏 audit/rollup。

### Phase 2：Circuit Reducer 和 lease

- 将 Key circuit 改为连续失败阈值模型。
- 接入 durable Open/cooldown/Half-Open/Closed 状态和 revision fence。
- 接入 score gate、单真实请求 lease、取消释放、迟到结果保护。
- 保留容量 runtime overlay 的独立 owner。

### Phase 3：Planner 和 Retry

- 删除生产探索预算和 rendezvous 选择；改为同层有效分数降序。
- 将 request budget 改为 `maxRetryCount`；每次换 Key 重新读取最新 snapshot。
- 保留 ReplayGate、deadline、commit safety 和容量内部 hard cap。
- Half-Open score gate 只与同一 Primary/Backup/Emergency 硬层的最高 Closed 候选比较；不同硬层不得互相作比较基线。

### Phase 4：Policy migration 和前端

- 完成 policy v2 -> v3 additive migration、history 和 rollback。
- 更新 generated IPC/TypeScript 类型和 draft reducer；中转站编辑页移除容量域身份区块及其 UI API 调用，旧容量域 API 仅保留迁移/审计兼容层。
- 重做设置页分组，删除 candidate/exploration/error-rate 字段，增加 circuit/retry/source-weight 字段。
- 每个超时和新字段补充独立解释、校验错误、窄窗口和保存生效边界。

### Phase 5：组合验证和清理

- 删除旧 production consumers、disabled error-rate service wiring、Beta prior fallback、容量域/作用域级路由 breaker wiring；保留本地容量 overlay，不保留容量域读路径。
- 更新决策解释、状态页、审计文档和删除台账。
- 运行本规范第 15 节的专项和跨层验证。

组合切换时，generation fence 暂停新的 candidate admission；`candidate_admitted` 只有在容量 lease、circuit/Half-Open CAS 和 attempt slot 身份已经持久化后才成立。未取得 admission 的请求等待至自身 deadline，超时沿用现有 deadline/timeout 公共错误并附 `routing_generation_transition` 诊断，不伪装为 `no_available_key`；已取得 admission 的请求允许完成并持有原 generation/lease。切换必须同时校验 policy、quality、circuit generation 的不可变 ID、revision/hash 和输入 watermark。

## 15. 验收标准

### 15.1 评分和顺序

- 同一 snapshot、同一 policy、同一 runtime revision 下，候选总按有效分数降序；输入数组顺序变化不改变结果。
- 最终有效分数相同使用稳定 Key tie-break；亲和沿用既有 bonus/hysteresis/逃逸规则。不再调用 weighted rendezvous、exploration budget 或随机 seed 选择。
- Primary/Backup 等硬层级仍优先于分数跨层比较。
- 高分 Key 正常情况下承载更多请求；容量不作为评分扣分项，只有容量准入失败时才继续尝试后面的 Key。
- 已打开 Key 不进入普通候选序列；会话亲和不能绕过熔断。
- Half-Open gate 只使用同一硬层的 `best_closed_score`；跨 Primary/Backup/Emergency 层的分数不参与比较。

### 15.2 TNTAPI/502 回归场景

使用 TNTAPI · tkapi 的等价 fixture：

1. 连续产生 10 次上游 HTTP 502。
2. 每次 attempt 都有 `RoutingObservation`，实际路由来源的 `recent_24h_sample_count > 0`，`failure_mass > 0`；不能再显示 0 样本。
3. 在最近/历史样本未达到门槛时，评分仍有明确值，但 quality basis 必须标记样本不足乐观值；达到门槛后可靠性分数按真实失败下降，连续失败计数增长。
4. 达到阈值后 circuit 为 Open，后续请求跳过该 Key，改用下一个可执行候选。
5. request log、decision trace、quality summary 和 circuit status 对同一个失败使用一致的 failure code/作用域。
6. 将同一 fixture 的上游结果替换为 HTTP `429`（即使响应带 `Retry-After`），必须得到与 5xx 相同的 Key 级失败、样本、连续失败、熔断和普通重试结果；不得出现站点/容量域等待，也不得改变重试预算或候选顺序。

### 15.3 重试

- `maxRetryCount=0/1/3` 分别严格对应 1/2/4 次总 outbound attempt 上限。
- 429、上游 5xx、已跨 outbound boundary 的连接失败、首字节超时在 replay-safe 且未 commit 时消耗预算并重新排序；`429` 不触发站点/容量域级别的等待或排除。
- `429` 的处理与 5xx、已跨 outbound boundary 的连接失败、首字节超时一致：只标记当前 `station_key_id` 为失败并尝试下一候选；响应中的 `Retry-After` 不改变本次重试预算、候选顺序或冷却算法。
- 仅容量准入拒绝时不视为 outbound attempt；继续向后准入不消耗 `maxRetryCount`，但仍受 deadline 和系统候选硬上限限制。
- 400/422、凭据拒绝、能力不支持、已提交响应不被设置强行重试。
- 出站边界前的本地连接/DNS/序列化/适配器错误不自动换 Key、不消耗 retry，也不写 Key 失败或 circuit；只有已跨边界的传输失败才走 Key 级重试。
- 同一请求不会无限重复同一 Key；本地容量准入拒绝不进入同目标等待，直接按容量规则继续向后找候选。

### 15.4 熔断和恢复

- 连续失败达到阈值后原子 Open；成功清零 Closed 连续失败。
- 冷却未结束时 Open 永不被普通评分选中。
- 冷却结束但评分不高于当前 Closed 最佳候选时，不消费 Half-Open lease。
- 评分高于当前 Closed 最佳候选时（或没有任何硬资格通过的 Closed 候选时），只允许一个并发真实请求。
- Half-Open 连续成功达到阈值才 Close；成功必须来自独立的真实路由请求，任一可归责失败 Reopen 并按递增冷却重新等待。
- lease race、取消、目标移除和迟到结果不会错误关闭新一轮状态。
- Half-Open lease 的 `lease_expires_at` 不晚于请求 `deadline_at`；deadline 前的长请求持续持有唯一 lease，deadline 后由单一 reaper 只执行一次幂等重开。

### 15.5 质量统计

- 默认 source weight 为实际路由 70%、监控 30；调整后有效质量按新 revision 计算。
- 可靠性由实际路由和可比监控先分别计算，再按来源权重混合；响应速度首版只使用成功的真实路由样本；历史最小样本默认 15，最近最小样本默认 5，窗口样本不足时使用可调乐观值。
- 真实路由成功/可归责失败和监控成功/失败都进入同一质量摘要；source weight 不影响 circuit 连续失败计数。
- 502 upstream failure 不是 Neutral；客户端错误、取消和本地未出站错误不污染 Key 可靠性。
- 同请求 retries 有 raw attempt 记录，但质量样本按 `source + station_key_id + correlation_id` 去重。
- 不再使用 Beta prior；无样本、样本不足乐观值和 `idle_real_route_sample` 在 trace 中明确可见。

### 15.6 闲置恢复

- 24 小时没有真实路由样本的 Key，其实际路由来源近期样本数为 `n=0`，按 `c=0` 使用历史窗口结果；若历史样本不足则将乐观值代入历史窗口。不写入假观测；监控来源仍按各自最小样本门槛计算。
- 新真实样本到达后立即移除假设。
- 假设不能越过 Open、凭据、能力和容量硬门。
- 无随机探索、无额外轮换；Key 只有在正常评分顺序轮到它且通过 Half-Open 评分门时，才由真实请求完成恢复验证。
- 切换期间新请求不能取得 candidate admission；generation fence 超过请求 deadline 时沿用现有 deadline/timeout 公共错误并带 `routing_generation_transition` 诊断，不能伪装为 `no_available_key`，已取得 admission 的请求允许完成。

### 15.7 设置页

- 不显示“候选与探索”和“错误率保护参数”。
- 显示并保存 source weights、历史/最近最小样本数、乐观可靠性/响应时间、retry 两字段和 circuit 两字段。
- 中转站编辑页移除“容量域身份”区块，不再加载或保存 `providerFamily`、`deploymentIdentity`、`regionIdentity`；本地容量准入仍作为不可编辑的运行时硬门。
- 页面挂载、controller 初始化和保存流程均不得调用旧容量域 API；旧表/API 只能在迁移/审计 allowlist 中保留。
- 每个 timeout 字段都有独立含义说明。
- loading、saving、field validation、CAS conflict、external document change、窄窗口均有明确状态。

### 15.8 必跑验证

Rust/Tauri 改动至少运行：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_policy -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_engine -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_protection -- --nocapture
```

前端/契约改动至少运行：

```powershell
pnpm.cmd test -- src/features/routing/LocalRoutingSettingsEditor.test.tsx src/features/routing/RoutingStatusDiagnosticsPanel.test.tsx src/lib/queries/routingQueries.test.ts
pnpm.cmd test:contracts
pnpm.cmd generate:bindings --check
pnpm.cmd build
```

跨层 cutover 还必须运行 `pnpm.cmd verify:fast`，并补充 502、重试预算、circuit concurrency、migration rollback、请求去重和近期/历史公式的专项 fixture。时间衰减 fixture 至少断言 `a=0/24/48/72` 小时分别得到 `1`、`2^(-24/72)`、`2^(-24/72-1)`、`2^(-24/72-2)`，并断言 24 小时边界连续且边界样本只归入近期窗口。

## 16. 风险和未决点

1. **连续失败阈值的误伤风险**：本文固定默认 `3`；必须在真实 provider fault injection 中验证不会因短暂相关网络故障过早排空候选池，验证不改变字段语义。
2. **质量统计窗口和衰减**：最近窗口固定为最近 24 小时，历史窗口为 24 小时以前且与近期不重叠；实现时必须固定 `c=min(0.9,n/(n+20))` 和 `w(a)` 的 72/24 小时分段衰减，避免再次引入“先验 + recent/historical 二次收缩”或自行替换衰减曲线。
3. **来源权重与监控等价性**：监控请求若不是同模型、同 endpoint 形态、同协议和同请求形态，必须标记为不可比并从可靠性混合中排除；剩余可比来源的权重重新归一化，不能凭 30% 权重混入真实推理样本。
4. **单候选池**：新请求如果所有 Key 都 Open、被禁用或失去硬资格，必须返回 `no_available_key`；不能为了消耗重试次数再次请求已经失败或已熔断的 Key。当前请求若已把候选全部加入 `request_exclusion`，则返回最后一个 canonical failure。
5. **容量域**：本版本从生产路由彻底移除容量域身份、同域排除、跨域回退和容量域等待；旧表、DTO 和代码可暂留作迁移/审计参考。后续若重新启用，必须另立规范和迁移。
6. **模型字段**：本规范不自动选择 `gpt-5.6-luna` 或其他模型。调用方原始模型、显式 model mapping 和上游模型是独立字段；若日志显示未请求的 Luna，应沿模型映射/调用方配置链单独审计，不能归因于本次评分排序。
7. **已有工作区改动**：实施时必须保留当前未提交修改，按照 `AGENTS.md` 的 Rust、前端、迁移和安全验证要求逐阶段收口。

## 17. 完成定义

只有同时满足以下条件，本文目标才可标记为 Implemented：

- 生产 planner 已完全改为确定性评分降序尝试；
- 真实 attempt 的可归责成功/失败已进入统一可靠性观测，502 不再 Neutral 丢失；
- source 独立计算、请求去重、历史/最近样本门槛、乐观值、70/30 权重和 24 小时真实路由闲置标记已有后端 golden tests；
- Key circuit 的 Open/递增 cooldown/Half-Open/Closed、单真实请求 lease 和独立真实请求连续成功恢复已通过并发/重启/迟到结果测试；
- `maxRetryCount` 和 `consecutiveFailureThreshold` 已成为唯一用户重试/失败阈值字段；
- 独立 error-rate protection 参数、candidate/exploration 用户字段和随机探索 production consumer 已删除；
- 设置页和中转站编辑页结构、字段说明、容量域移除、本地容量硬门和状态边界与本文一致；
- 新增公共 `no_available_key` 错误码的 HTTP/IPC/客户端映射已覆盖；容量终态仍使用 `route_capacity_exhausted`，并能区分容量耗尽与容量状态不可用；
- v2 -> v3 migration、rollback、generated bindings、Rust/TypeScript/跨层验证和 TNTAPI/502 回归证据齐全。
