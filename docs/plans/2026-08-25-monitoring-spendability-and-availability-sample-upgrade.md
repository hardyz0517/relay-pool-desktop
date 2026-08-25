# Relay Pool Desktop 监控消费资格与可用性样本升级实施计划

状态：Ready for implementation；产品取舍已确认，尚未开始业务实现

日期：2026-08-25

适用范围：Station Collector 余额事实、主动渠道监控、渠道状态 Workspace、监控健康回写，以及与这些路径共享的上游错误证据和路由消费资格读取。

关联规范：

- [`../specs/STATUS_MONITORING_REFACTOR_SPEC.md`](../specs/STATUS_MONITORING_REFACTOR_SPEC.md)
- [`2026-08-13-upstream-error-classification-retry-closure.md`](2026-08-13-upstream-error-classification-retry-closure.md)
- [`../specs/2026-07-30-routing-operational-unification-upgrade-spec.md`](../specs/2026-07-30-routing-operational-unification-upgrade-spec.md)
- [`../SCHEMA_UPGRADE_AUTHORING.md`](../SCHEMA_UPGRADE_AUTHORING.md)

计划关系：本计划是状态监控和上游错误分类的专项收口，不建立第二套错误分类、健康事实或余额换算体系。HTTP/SSE 证据解析复用并抽取现有 Proxy 生产分类能力；持久化业务阻断继续使用现有 scoped routing health observation/verdict owner；数值余额继续由 `balance_snapshots` 持有。若本文与 `AGENTS.md`、当前代码、自动化契约或上述当前规范冲突，以优先级更高者为准。

> 执行要求：每个任务遵循 RED-GREEN-REFACTOR。先用确定性测试固定缺陷，再接入唯一生产路径，最后删除旧判断和重复查询。任何任务的退出门禁未以退出码 `0` 完成，该任务不得标记完成。

---

## 1. 背景与问题定义

当前 Grox 案例暴露的是一组相互关联的问题，而不是单个余额字段错误：

1. `station_key` 最新余额优先于整站余额，Key 普通余额为负时会覆盖仍可使用的订阅额度。
2. 监控暂停 SQL 只读取 `balance_snapshots.value <= 0`，忽略来源、状态、权威性、适用范围、采集时间和证据冲突。
3. HTTP 错误优先按状态码粗分；HTTP 200 错误 JSON、Chat SSE error 和 Responses `response.failed` 会丢失结构化业务错误码，并降级为 `protocol_mismatch/upstream_failed_event`。
4. 余额、订阅和本地执行条件错误会被当作技术不可用，污染状态柱、可用率、延迟统计、Station Key health 和路由错误率保护。
5. 暂停规则在 Monitoring Definition、Monitoring Store、Status Workspace 和前端批量检测中重复，已经出现 Key/整站选择顺序不一致。
6. 余额恢复没有事件唤醒，全部相关监控暂停时最多依赖 300 秒 idle wakeup 才重新评估。
7. 最近窗口与 rollup 的 `total` 口径不一致，rollup 的 P50/P95 还会接收不具备可用性统计资格的样本。
8. 仓库已经存在“权威、范围匹配、新鲜证据才可判定 depleted”的 projector 和测试，但关键实现被 `#[cfg(test)]` 限制；生产路由和监控仍有多套简化判断。

## 2. 目标与完成定义

目标生产链路固定为：

```text
Collector balance/subscription facts   HTTP / JSON / SSE probe evidence
                  \                    /
                   typed evidence normalization
                              |
                  canonical failure/effect planning
                              |
          scoped business verdict + spendability resolver
                     /                    \
       scheduler admission/pause       routing admission tier
                     |
              monitoring execution
                     |
          terminal sample disposition
            /          |          \
       trend/rate    latency     technical health
```

Engineering cutover 只有同时满足以下条件才完成：

- 只有明确、权威、范围匹配且仍然新鲜的余额/订阅/硬配额耗尽证据能自动暂停监控。
- `Unknown`、`NotSupported`、`NotApplicable`、低置信度、过期或冲突证据均不等于耗尽，也不暂停。
- HTTP JSON 与 SSE 中的安全结构化 `code/type/event` 在 monitor attempt 中可追溯，不再统一丢成 `protocol_mismatch`。
- 余额/订阅导致的请求失败保留在执行历史中，但不生成状态柱、不进入可用率分母、不进入技术延迟、不恶化技术健康或错误率。
- 最近 60 次表示最近 60 个 `availabilityEligible=true` 的终态 target result；被排除执行不占用 60 个槽位。
- 小时/日 rollup 保留排除次数和原因计数，但 `total_count` 只表示可用率 eligible 数量。
- Collector 恢复、成功请求、证据过期均能解除相应 scope 的暂停；恢复不改写用户的 `enabled` 设置。
- 调度暂停和恢复都使用同一个 resolver，余额变化后正常情况下 1 秒内重新评估；事件丢失仍有有界定时兜底。
- Monitoring、Routing 和前端不再各自解析 `value <= 0` 或余额状态字符串。
- 旧生产 owner 和重复 SQL 在同一 cutover 中删除，并有静态架构门禁防止回流。
- 相关 Rust、Vitest、生成契约、`pnpm build` 和 `pnpm verify:full` 全部通过。

## 3. 明确不做的事项

本计划不包含：

- 余额、点数、人民币、美元、倍率或订阅额度的换算算法调整。
- 新增支付、充值、账号或云端能力。
- 根据自由文本 message 猜测余额不足；message 只允许产生版本化、受 profile 约束的闭合 signature。
- 将未知错误、普通 4xx、任意负数或低余额提示一律解释为余额耗尽。
- 回填猜测历史 `protocol_mismatch` 是否曾经是余额错误。
- 重写整个 Monitoring V2、Routing Engine 或 Station Collector。
- 发布、打包、真实账号 smoke；真实 provider 验证需要单独授权，不能阻塞本地 engineering cutover。

## 4. 冻结的产品与领域决策

### 4.1 三套状态必须分离

系统必须同时维护但不得混淆：

1. **Spendability / 消费资格**：当前目标是否有资格继续发起可能计费的请求。
2. **Technical availability / 技术可用性**：端点、协议、模型响应和网络链路是否正常。
3. **Sample disposition / 样本处置**：一次已经发生的执行是否有资格进入状态柱、可用率、延迟和健康统计。

余额耗尽可以令 Spendability 为 `Depleted`，但它本身不把 Technical availability 置为 `Unavailable`。已经发出的余额错误请求记录为业务失败，同时 `availabilityEligible=false`、`healthEffect=neutral`。

### 4.2 Spendability 状态

V1 闭合状态：

| 状态 | 含义 | 自动暂停 |
| --- | --- | --- |
| `usable` | 有新鲜权威证据证明可消费，或成功请求证明对应 scope 当前可用 | 否 |
| `low` | 余额偏低但尚未耗尽 | 否 |
| `depleted` | 新鲜权威证据证明对应 scope 已耗尽 | 是 |
| `unknown` | 缺少足够证据、证据冲突或旧数据无法判定 | 否 |
| `not_supported` | 上游不支持查询余额 | 否 |
| `not_applicable` | 当前 provider/订阅模式不适用余额查询 | 否 |

`pause_on_zero_balance` 的用户可见语义调整为“消费资格耗尽时自动暂停”。首期保留数据库/API 旧字段用于兼容，不再按字段名实施 `value == 0` 的字面规则；后续若迁移为 policy enum，另立兼容计划。

### 4.3 可以产生 `depleted` 的证据

仅以下两类证据允许产生 `depleted`：

1. Collector driver 在完成账号余额与可适用订阅/平台额度合并后，显式输出 typed `Depleted`，且标记为 `Confirmed + Authoritative`。
2. 真实请求或主动监控从受信 provider rule profile 中解析出结构化余额、订阅或硬配额耗尽信号，并由 canonical classifier 输出 `Confirmed` scoped business effect。

禁止事项：

- Resolver 不得根据 `source` 字符串、`confidence >= 某阈值` 或裸 `value <= 0` 自行升级为 authoritative。
- HTTP 状态码本身不能产生 credential/account/balance durable block；例如普通 `429` 仍是 rate limit，只有合法错误 envelope 中的受信 code 才能成为硬配额证据。
- 5xx 中夹带凭据或余额 code、status/code/type 相互冲突时输出 `Conflicting`，不产生暂停。
- Key scope 不得无条件覆盖 Station/Subscription scope。先做 scope applicability，再做证据选择。

### 4.4 Scope 与短路规则

V1 支持以下消费资格 scope：

| Scope | 例子 | 暂停/短路范围 |
| --- | --- | --- |
| `station_account` | 整站余额耗尽 | 该 Station 下所有适用监控目标；停止同站模型 fallback |
| `station_key` | 上游明确声明某 Key 独立硬额度耗尽 | 仅该 Key；Station monitor 的其他 Key 可继续 |
| `station_group` | 当前订阅组不存在、失效或额度耗尽 | 仅绑定该 group 的 Key/目标 |
| `model_on_key` | 上游明确声明模型级硬配额 | 仅该 Key+模型；允许按既有规则尝试不受影响的 fallback model |

无法从受信证据确认 scope 时为 `Uncertain`，不得自动暂停整站。

### 4.5 新鲜度与恢复

V1 新鲜度规则由一个版本化 `SpendabilityPolicyV1` 持有，不允许散落 magic number：

- Collector 证据有效期：`clamp(2 × station.collection_interval, 10 分钟, 2 小时)`。
- 运行时明确余额/订阅/硬配额证据默认有效期：30 分钟。
- 新的同 scope `usable` 权威 Collector 事实立即覆盖旧的 depleted 事实。
- 同 scope 成功请求形成明确 recovery observation，立即解除相应 balance/quota/group-subscription verdict。
- 证据到期后状态退回 `unknown`，自动解除监控暂停；不得把过期 depleted 无限保留。
- Collector 查询失败不写 `depleted`，也不把最后一次成功事实的时间刷新为当前时间。
- 用户手工启停与自动暂停互不改写；恢复不能把用户主动停用的 Monitor 重新启用。

Resolver 返回 `nextRecheckAtMs`。Runner 的下一次唤醒时间必须取正常 `next_due_at_ms`、最早证据过期时间和兜底 idle wakeup 的最小值。

### 4.6 样本处置矩阵

V1 处置规则：

| 终态/原因 | 可用率 eligible | 延迟统计 | 技术健康 | 业务作用 |
| --- | --- | --- | --- | --- |
| 成功/降级成功 | 是 | 是 | success | 清除匹配 scope 的旧业务阻断 |
| network/timeout/5xx | 是 | 是 | observe failure | 无 |
| confirmed auth | 是 | 是 | hard fail credential | credential block |
| rate limit | 是 | 是 | cooldown | rate-limit cooldown |
| protocol/content mismatch | 是 | 是 | 保持现有技术语义 | capability/protocol evidence 按现有规则 |
| confirmed insufficient balance | 否 | 否 | neutral | scoped balance depleted |
| confirmed subscription invalid/depleted | 否 | 否 | neutral | scoped group-subscription depleted |
| confirmed hard quota exhausted | 否 | 否 | neutral | scoped quota depleted |
| cancelled/interrupted | 否 | 否 | neutral | 无 |
| local needs-configuration/budget exceeded/internal-before-send | 否 | 否 | neutral | 显示本地诊断，不产生上游健康结论 |
| upstream invalid request 或 model unavailable | 首期保持现有 eligible 语义 | 是 | 按现有规则 | 本计划不擅自重定义 |

`InvalidRequest`、`Internal` 等不能只凭旧 `FailureKind` 决定是否排除，必须同时有 `failureOrigin`。历史数据缺少 origin 时保留旧行为。

### 4.7 手动执行

- 用户仍可对自动余额暂停的 Monitor 执行“立即检测”，其 trigger 明确为 `manual_override`。
- 手动执行不修改 `pause_on_zero_balance`，也不预先伪造 `usable`。
- 手动执行若再次得到 confirmed depleted，记录业务执行但不进入技术统计，并刷新对应业务证据有效期。
- 手动执行成功时清除匹配 scope 的旧 depleted 证据并唤醒调度器。
- “检测全部启用项”排除自动暂停项；“检测有消费资格项”必须消费后端 resolver 结果，删除前端裸余额判断。

## 5. 目标领域契约

### 5.1 共享错误证据

从现有 `services/proxy/adapters/error_envelope.rs` 和 `error_rules.rs` 抽取不依赖 Proxy 执行器的共享模块，建议位置：

```text
src-tauri/src/services/provider_errors/
  mod.rs
  envelope.rs
  rules.rs
  profile.rs
  evidence.rs
```

核心输入：

```rust
ProviderErrorInput {
    provider_profile,
    transport,          // http | chat_sse_error | responses_sse_failure
    protocol,
    http_status,
    content_type,
    bounded_body_or_event,
    retry_after,
    received_at,
}
```

核心输出沿用现有 typed evidence，不复制第二个枚举：

```rust
UpstreamFailureEvidence {
    semantic_candidates,
    code,
    error_type,
    envelope,
    confidence,
    conflict_reason,
    retry_after_ms,
    flags,
    profile_version,
    rule_set_version,
}
```

安全约束：

- body/event、JSON 深度、JSON 复杂度和 message scan 继续使用现有硬上限。
- 持久化只允许 normalized code/type、闭合 signature、confidence、scope 和版本；不得保存原始自由 message、body、Authorization 或 API key。
- HTTP 非 2xx 必须在丢弃 body 前完成 evidence extraction。
- HTTP 2xx 的合法 error envelope 必须进入同一 classifier。
- Chat SSE error、Responses `response.failed` 和普通 HTTP JSON 使用同一 rule profile，不再只返回 `StreamError::UpstreamFailedEvent`。
- Anthropic/Gemini 当前 adapter 至少进入 conservative generic profile；provider-specific rule 只有具备受信 target metadata 和 contract fixture 后才能启用。

### 5.2 Spendability resolver

新增唯一 application owner，建议位置：

```text
src-tauri/src/application/spendability/
  mod.rs
  contract.rs
  policy.rs
  resolver.rs
  read_port.rs
```

输入事实只来自两个现有 durable owner：

1. `balance_snapshots`：Collector 数值余额和订阅合并事实。
2. `routing_health_observations/verdicts`：结构化运行时 balance/quota/group-subscription 阻断与 recovery。

首期不新增第三张 canonical spendability observation 表。若为性能增加缓存，只能是带 `projector_version/source refs`、可全部重建的 projection，不得成为新写入事实源。

建议输出：

```rust
SpendabilityProjection {
    subject,
    state,
    reason_code,
    evidence_source,
    evidence_confidence,
    authoritative,
    observed_at_ms,
    valid_until_ms,
    source_revision_refs,
    projector_version,
    next_recheck_at_ms,
}
```

Resolver 选择顺序：

1. 丢弃 scope 不适用、revision 不匹配或已过期证据。
2. 冲突证据降为 `unknown`，不以最后写入覆盖仍有效的正交 dimension。
3. `balance`、`quota`、`group_subscription` 分 dimension 解析，再合成为该请求/监控目标的最严格适用消费资格。
4. 同 dimension 中优先使用 `Confirmed + Authoritative`，再按 monotonic observation sequence/observed time 选择；不得按 Key-first/Station-first硬编码。
5. `low` 只用于提示和告警，不转成 depleted。
6. 输出完整 reason/source/time，供 Scheduler、Routing 和 UI 直接消费；消费者不得反向解析展示文本。

### 5.3 监控样本处置

新增闭合领域类型：

```rust
ProbeSampleDisposition {
    availability_eligible: bool,
    latency_eligible: bool,
    health_effect: TechnicalHealthEffect,
    exclusion_reason: Option<SampleExclusionReason>,
    profile_version: &'static str,
}
```

`SampleExclusionReason` V1：

- `balance_depleted`
- `subscription_unavailable`
- `quota_exhausted`
- `cancelled`
- `interrupted`
- `local_configuration`
- `local_budget`
- `local_internal_before_send`

唯一 classifier 以 canonical failure、failure origin、request send phase 和 trigger kind 为输入。Status read model、rollup、health writeback 和 routing error-rate writer只能读取 disposition，禁止再次按 `FailureKind` 写 switch。

## 6. 持久化与迁移设计

实施时先读取最新 migration head，再分配 `00NN`，不得预占当前工作区正在开发的编号。建议迁移名：

```text
00NN_monitoring_spendability_sample_disposition.sql
```

### 6.1 `balance_snapshots`

为新 Collector 事实增加明确证据元数据；旧列和数值含义保持兼容：

- `evidence_confidence`：`confirmed | probable | unknown | conflicting`，旧数据默认 `unknown`。
- `spendability_authority`：`authoritative | advisory | unknown`，旧数据默认 `unknown`。
- `observed_at_ms`：可靠的数值时间；不得用读取时间冒充采集时间。
- `valid_until_ms`：由版本化 policy 在写入时计算。
- `evidence_profile_version`：Collector mapping/profile 版本。
- `spendability_reason_code`：闭合 reason，不保存自由文本。

Collector driver 必须显式填充这些字段；repository 不根据 `source` 或 confidence 数字猜测 authority。

### 6.2 scoped routing health evidence

扩展 `routing_health_observations` 和 `routing_health_verdicts`：

- `evidence_valid_until_ms NULL`

规则：

- 只有 `balance`、`quota`、`group_subscription` 等时效业务 dimension 使用有效期。
- credential、account lifecycle 和 endpoint 等现有 revision-fenced 语义不因本计划被隐式加 TTL。
- Projector rebuild、active generation cutover、content hash 和 differential tests 必须包含新字段。
- 迁移时，历史 business verdict 以 `source_ingested_at_ms + 30 分钟` 形成有界兼容期限；已经过期的记录仍保留审计，但不再阻断消费资格。

### 6.3 monitor attempts 与 target results

`channel_monitor_attempts` 增加安全、稳定的分类证据：

- `canonical_failure_class NULL`
- `failure_origin NULL`
- `failure_scope_kind NULL`
- `failure_dimension NULL`
- `evidence_code NULL`
- `evidence_confidence NULL`
- `classifier_profile_version NULL`

`channel_monitor_target_results` 增加终态处置：

- `availability_eligible INTEGER NOT NULL DEFAULT 1`
- `latency_eligible INTEGER NOT NULL DEFAULT 1`
- `exclusion_reason NULL`
- `technical_health_effect TEXT NOT NULL DEFAULT 'legacy'`
- `disposition_profile_version TEXT NOT NULL DEFAULT 'legacy-monitoring-v1'`

Repository 写入前验证：

- eligible 时 `exclusion_reason` 必须为 `NULL`。
- excluded 时必须有闭合 reason。
- excluded business sample 的 technical health 必须是 `neutral`。
- legacy 默认不得重新猜测历史 `protocol_mismatch`。

### 6.4 rollup

`channel_monitor_bucket_rollups` 增加：

- `excluded_count INTEGER NOT NULL DEFAULT 0`
- `exclusion_counts_json TEXT NOT NULL DEFAULT '{}'`

冻结计数语义：

```text
eligible_count = available + degraded + unavailable
total_count    = eligible_count
observed_count = eligible_count + skipped_count + excluded_count
```

- strict/effective availability 只使用 `eligible_count`。
- P50/P95 只接收 `latency_eligible=true` 的样本。
- failure counts 只统计 availability eligible 的技术失败。
- exclusion counts 单独校验为闭合 reason -> 非负整数对象。
- raw execution 删除前必须确认对应小时 rollup 已持有 eligible、skipped、excluded 和 exclusion counts。
- 现有 dirty-range repair、generation/rebuild 和 corrupt JSON 检测同步覆盖新字段。

### 6.5 Workspace DTO

Channel Status Workspace schema 从 V2 升为 V3，新增：

- Monitor：`spendabilityState`、`spendabilityReason`、`spendabilityScope`、`spendabilityObservedAtMs`、`spendabilityValidUntilMs`、`spendabilitySource`。
- Recent point：`availabilityEligible`、`exclusionReason`；主 recent 数组默认只返回 eligible 点，执行详情仍返回全部记录。
- Bucket counts：`eligible`、`skipped`、`excluded`、`observed`；兼容期内 `total` 明确等于 eligible。
- Bucket：`exclusionCounts`。
- Latest：`latestEligibleResult` 与 `latestExecutionResult` 分离，避免余额执行覆盖技术当前状态。

所有 DTO/IPC 变更必须经现有 binding generator 生成，不手改生成文件。

## 7. 调度、执行与恢复行为

### 7.1 调度读取

删除 Monitoring SQL 中内联的 `COALESCE(latest key balance, latest station balance) <= 0`。

新的调度流程：

1. 在同一个 read session 中批量读取 enabled/due monitor definitions、目标 scope、最新余额证据和适用 active business verdict。
2. `SpendabilityResolver` 批量产生每个目标的 projection。
3. `pause_on_zero_balance=false` 时忽略自动消费资格暂停，但仍在 read model 暴露 projection。
4. `pause_on_zero_balance=true && state=depleted` 时不进入 scheduled execution。
5. `unknown/not_supported/not_applicable/low/usable` 保持可调度。
6. `next_due_at_ms` 同时考虑未暂停 Monitor 和最早 `nextRecheckAtMs`。

Definition CRUD、Workspace、Monitoring list/detail、Scheduler 必须调用同一 application owner，不能复制 resolver SQL。

### 7.2 事件唤醒

新增进程内、可合并的 `MonitoringScheduleInvalidation`：

- Collector balance/subscription facts 成功提交后发布受影响 station/key/group。
- scoped business verdict 或 recovery 成功提交后发布受影响 subject。
- Monitor policy/target/interval 更新后沿用同一唤醒入口。
- Runner 在 `tokio::select!` 中监听 invalidation，去重并重新计算 delay。
- 事件只在事务提交后发布；回滚不得唤醒并暴露不存在的事实。
- channel 满时允许合并为全量重查信号，不允许阻塞事实写入。
- 事件丢失时 300 秒 idle wakeup 和 `nextRecheckAtMs` 定时仍保证最终恢复。

验收时钟：在确定性 fake clock 测试中，事实恢复提交至重新评估不超过 1 秒；生产不承诺网络探测本身在 1 秒内完成。

### 7.3 执行短路

Monitor orchestrator 在收到 confirmed business failure 后按 scope 短路：

- station account depleted：停止该 Station 的剩余 retry 和 model fallback。
- station key depleted：停止该 Key，Station monitor 可继续其他 Key。
- station group invalid/depleted：停止同 group 目标，其他 group 不受影响。
- model-on-key quota：只停止当前模型，可按既有规则尝试不受影响的 fallback。
- uncertain/probable/conflicting：不产生 durable pause；按现有保守 retry/terminal 规则结束。

短路不得突破 execution/attempt budget，也不得把未发送 attempt 计为真实 outbound attempt。

### 7.4 写回顺序

一次 monitoring finalization 在同一事务内完成：

1. 写 attempt 安全分类证据。
2. 归约 target terminal result 与 sample disposition。
3. 写 scoped business observation/verdict 或 recovery。
4. 仅对 disposition 允许的样本写 technical health observation/error-rate observation。
5. 标记 rollup dirty range。
6. finalization commit。
7. commit 后发布 schedule invalidation/runtime event。

任一步失败遵循现有 partial/repair 契约；不得出现 target result 已提交但 health/business effect 被悄悄丢弃且 execution 仍标 completed。

## 8. 状态、统计与前端行为

### 8.1 当前状态

Channel Status 行同时表达：

- 调度/消费状态：启用、用户停用、余额暂停、订阅暂停、未知证据。
- 最近技术状态：最近一个 eligible terminal result 的正常/降级/错误。

暂停 badge 不再使用旧探测失败作为暂停原因。Tooltip/详情显示：

- 暂停原因闭合文案。
- 作用 scope。
- 证据来源。
- 观察时间和有效期。
- “不计入技术可用率”的说明。

### 8.2 趋势与可用率

- “近 60 次记录”读取最近 60 个 eligible target result。
- excluded 执行不生成红/绿/黄状态柱，也不占一个槽位。
- 若没有 eligible 样本，可用率显示 `--`，不能显示 `0%`。
- 24h/7d/30d bucket 有 eligible 样本时按既有公式计算；只有 excluded/skipped 时显示 missing/skipped-only，而不是 unavailable。
- Tooltip 可显示“另有 N 次因余额/订阅排除”，但不把这些计入主趋势色彩。
- 最近延迟取最近 eligible result；rollup latency 只来自 latency eligible 样本。

### 8.3 执行详情

执行抽屉保留所有 attempt/target result，并为 excluded 结果显示：

```text
已排除：余额耗尽
不影响状态柱、可用率、延迟与技术健康
证据：insufficient_quota（confirmed，station_account）
```

不得显示原始错误 body 或完整自由 message。

### 8.4 批量检测

- 删除前端 `hasCurrentBalance()`。
- 批量作用域根据 Workspace 的 typed spendability state 选择，不再额外拉余额并自行 `find()`。
- 手动单项检测允许绕过暂停；批量默认不绕过。
- 如保留“检测有消费资格项”，仅选择 `usable/low/unknown/not_supported/not_applicable`，排除 `depleted`。

## 9. 查询与代码结构清理

本计划允许且要求完成以下范围内清理：

1. 删除 `definitions.rs`、`monitoring_store.rs`、`status_read_repository.rs` 中重复余额暂停 SQL。
2. 删除前端裸余额批量判断。
3. 删除 Routing 的 `RuntimeRoutingBalance::is_depleted()`、`candidate_is_depleted()` 等重复生产 owner；Routing Planning Snapshot 改为消费同一 spendability projection。
4. 将 `balance_projector.rs` 中 test-only 的权威性/新鲜度模型产品化，测试与生产使用同一函数。
5. `workspace_recent_results()` 从逐行 N+1 查询改为一次 scoped CTE + `ROW_NUMBER() OVER (PARTITION BY monitor_id, station_key_id ...)` 批量查询，并在 SQL 层只让 eligible 点占 recent limit。
6. Rollup 聚合只消费 `ProbeSampleDisposition`，删除按 outcome 猜统计资格的分支。
7. `health_outcome()` 改为消费统一 disposition/effect，不再以默认 `_ => ObserveFailure` 吞掉业务错误。

不在本计划内重构 profile、模板、fallback UI、非监控 Dashboard 或无关 RoutingService owner。

## 10. 可执行任务拆分

### Task 0：建立基线与 RED 证据

改动：只增加失败测试和审计清单，不改生产行为。

步骤：

1. 为 Grox 等价 fixture 增加：Key 普通余额负数、整站订阅可用、当前错误 SSE `response.failed`。
2. 证明当前 Scheduler 暂停、SSE 降级为 protocol mismatch、状态柱/可用率被污染。
3. 增加 stale、conflicting、unknown、NotSupported、Key/Station/Group scope fixture。
4. 增加 runner 全暂停后恢复延迟测试。
5. 增加 recent 60 被 excluded 挤占和 rollup latency 污染测试。
6. 记录所有直接读取 `balance_snapshots` 并做 depleted 决策的生产路径，形成删除台账。

退出门禁：新增测试以预期方式失败；失败原因与本计划逐项对应，不允许因编译错误或 fixture 错误失败。

### Task 1：抽取共享 provider error evidence

主要文件：

- `src-tauri/src/services/proxy/adapters/error_envelope.rs`
- `src-tauri/src/services/proxy/adapters/error_rules.rs`
- `src-tauri/src/services/monitoring/adapters/*`
- `src-tauri/src/services/protocol_streaming/openai.rs`
- 新 `src-tauri/src/services/provider_errors/*`

步骤：

1. 移动/抽取纯 envelope parser、rule profiles 和 typed evidence，保持 Proxy 行为 golden parity。
2. HTTP monitor adapter 在 status-only 分类前解析 bounded error evidence。
3. OpenAI Chat/Responses 增量 SSE reducer 保留安全的 error code/type/event evidence。
4. 将 evidence 映射到现有 canonical failure/effect contract，不新建第二套语义枚举。
5. 为 200 error JSON、402、400/429+insufficient_quota、Chat SSE error、Responses failed、冲突 5xx、超限 body/event 增加 contract tests。
6. 删除 `upstream_failed_event -> protocol_mismatch` 的业务错误兜底；真正 framing/terminal 错误仍保持 protocol mismatch。

退出门禁：Proxy 原分类测试无行为漂移；Monitoring adapter 新 fixture 全绿；安全上限测试全绿。

### Task 2：产品化 Spendability contract/resolver

主要文件：

- `src-tauri/src/application/operational_facts/balance_projector.rs`
- `src-tauri/src/application/operational_facts/candidate_projection.rs`
- `src-tauri/src/application/operational_facts/planning_snapshot.rs`
- 新 `src-tauri/src/application/spendability/*`
- 新/调整 persistence batch read port

步骤：

1. 移除正确 balance projector 核心类型和函数上的 `#[cfg(test)]`。
2. 将状态扩展为本计划冻结的闭合 Spendability 状态。
3. 实现 authority、freshness、scope applicability、dimension composition、conflict 和 `nextRecheckAtMs`。
4. 用 batch read port 读取 balance snapshots 与 active business verdicts。
5. Routing Candidate/Planning Snapshot 切换到 resolver 输出。
6. 删除生产中的裸 `is_depleted/candidate_is_depleted` owner。
7. 增加 production-vs-projector parity 和 differential tests。

退出门禁：Monitoring/Routing 对同一 fixture 得到同一 state/reason/scope；架构测试禁止新增裸余额判断。

### Task 3：迁移证据与样本处置 schema

步骤：

1. 按最新 migration head 分配编号并新增迁移。
2. 扩展 balance snapshot、routing health evidence、attempt、target result 和 rollup 字段。
3. 实现 legacy 默认和 business verdict 有界兼容期限。
4. 更新 schema registry、legacy/portable migration policy、backup/restore allowlist 和 artifact tests。
5. 更新 persistence row models、validators、rebuild hash 和 differential fixtures。
6. 验证从当前真实 schema 升级与 fresh install schema 完全一致。

退出门禁：`monitoring_migration`、routing health persistence、portable migration、schema compatibility 和 fresh/upgrade differential tests 全绿。

### Task 4：Collector 输出 typed spendability evidence

主要文件：

- `src-tauri/src/services/collectors/facts.rs`
- `src-tauri/src/services/collectors/collector_apply.rs`
- `src-tauri/src/services/collectors/drivers/sub2api/*`
- `src-tauri/src/services/collectors/drivers/newapi/*`

步骤：

1. Driver 在完成订阅/平台额度合并后显式输出 state、authority、confidence、observed/valid time 和 profile version。
2. 普通 Key 余额、整站余额、订阅余额保持独立原始字段，不由 generic repository选择优先级。
3. Collector partial/failure 不刷新旧证据 observed time。
4. 新的 authoritative usable/depleted 事实产生对应 business recovery/block observation。
5. Commit 后发布 schedule invalidation。
6. 增加订阅可用但普通余额负数、订阅耗尽、订阅接口失败、过期事实测试。

退出门禁：Collector mapping/apply 测试证明只有合并后的 authoritative 事实影响 Spendability；不修改换算算法。

### Task 5：监控执行归类、短路与写回

主要文件：

- `src-tauri/src/application/monitoring/orchestrator.rs`
- `src-tauri/src/application/monitoring/write_path.rs`
- `src-tauri/src/models/monitoring/*`
- `src-tauri/src/persistence/stores/monitoring/executions.rs`

步骤：

1. 引入 canonical failure evidence 与 `ProbeSampleDisposition`。
2. 实现 frozen disposition matrix。
3. 按 station/key/group/model scope 短路 retry/fallback。
4. 在同一 finalization transaction 写 target result、business effect、technical health effect 和 rollup dirty range。
5. 成功执行写 matching scope recovery；不得清除不相关 credential/account/group verdict。
6. Manual trigger 标记为 `manual_override` 并遵守同一处置规则。
7. 删除 `health_outcome()` 的默认业务失败污染路径。

退出门禁：orchestrator/write-path/fault/idempotency tests 覆盖每种 scope、重复提交、partial repair 和 success recovery。

### Task 6：Scheduler 单一准入与事件唤醒

主要文件：

- `src-tauri/src/persistence/stores/monitoring/definitions.rs`
- `src-tauri/src/persistence/stores/monitoring_store.rs`
- `src-tauri/src/services/monitoring/runner.rs`
- runtime composition/event catalog

步骤：

1. Definition repository 只读取定义与 due 时间，不再内联余额 SQL。
2. Monitoring application service 批量解析 due candidates 的 spendability。
3. `list_due`、`next_due`、list/detail/status workspace 共用 resolver。
4. Runner 接入 invalidation receiver，并计算最早证据过期唤醒。
5. 保留 300 秒 fallback；事件 channel 使用 bounded/coalescing 语义。
6. 验证 depleted 不取消 in-flight、恢复不修改 enabled、用户停用不被恢复覆盖。

退出门禁：scheduler/concurrency tests 使用 fake clock 证明暂停、1 秒内恢复、过期恢复、丢事件兜底和 shutdown 无泄漏。

### Task 7：统计、rollup、retention 与 N+1 清理

主要文件：

- `src-tauri/src/application/monitoring/queries.rs`
- `src-tauri/src/application/monitoring/buckets.rs`
- `src-tauri/src/persistence/stores/monitoring/retention.rs`
- `src-tauri/src/persistence/stores/monitoring/status_read_repository.rs`

步骤：

1. recent window 只让 eligible target result 占 60 个槽位。
2. 批量查询替代逐 row N+1。
3. 统一 recent/rollup 的 eligible、skipped、excluded、observed 计数。
4. latency/failure counts 按 disposition 过滤。
5. 扩展 rollup dirty repair、corrupt detection 和 raw deletion资格。
6. 用 query-plan test 证明新查询使用 monitor/key/finished 索引且不发生无界全表扫描。

退出门禁：read-model、bucket、retention、query-plan 和 raw-deletion tests 全绿；最近 60 与 rollup 结果一致。

### Task 8：Workspace V3 与前端交互

主要文件：

- `src-tauri/src/models/monitoring/read_model.rs`
- `src-tauri/src/ipc/dto/channel_monitor_reads.rs`
- `src/features/channels/channelStatusViewModel.ts`
- `src/features/channels/useChannelStatusController.ts`
- `src/features/channels/components/*`

步骤：

1. 后端输出 spendability、latest eligible、latest execution 和 exclusion counts。
2. 运行 binding generator 更新 TypeScript 契约。
3. 当前状态区分用户停用、余额暂停、订阅暂停和最近技术状态。
4. Trend 只渲染 eligible 点；bucket tooltip 展示 excluded 旁路计数。
5. Execution drawer 展示安全的排除原因和证据。
6. 删除 `hasCurrentBalance()` 和额外前端余额选择逻辑。
7. 覆盖 loading/empty/error/disabled、窄窗口、键盘焦点和长原因截断。

退出门禁：相关 Vitest、bridge generated tests、`pnpm build` 全绿；截图中的 Grox 场景显示“余额/订阅暂停”且历史可用率不新增红柱。

### Task 9：删除旧 owner、架构门禁与文档收口

步骤：

1. 按 Task 0 删除台账逐项删除重复 SQL/switch/helper。
2. 新增静态架构检查，仅允许指定 persistence read owner 读取 balance evidence；禁止 Monitoring/Frontend 新增 `value <= 0` 决策。
3. 更新 `docs/README.md`、状态监控规范的实施状态和 release note 草稿。
4. 生成 bindings、command registry/runtime event catalog 等受影响 artifact。
5. 执行完整验证矩阵并保存实际退出码。

退出门禁：删除台账归零，`rg`/architecture test 无未授权 owner，`pnpm verify:full` 退出码为 `0`。

## 11. 测试矩阵

### 11.1 Resolver 单元测试

- Key 负普通余额 + Station 可用订阅：usable，不暂停。
- Station/Subscription confirmed depleted：depleted，暂停适用目标。
- stale/unknown/not-supported/not-applicable：不暂停。
- low：提示但不暂停。
- probable/conflicting：unknown，不暂停。
- Key、Station、Group、Model scope 互不越界。
- 新 usable 覆盖旧 depleted；旧 recovery 不覆盖更新的 depleted。
- success recovery 只清匹配 dimension/scope/revision。
- TTL 到期产生 `nextRecheckAtMs` 并退回 unknown。
- Monitor 与 Routing 对同一事实集输出一致。

### 11.2 Adapter contract

- HTTP 200 error JSON。
- HTTP 400/402/429 的 `insufficient_quota`。
- 429 普通 rate limit 不误判余额。
- 5xx body 中伪 auth/balance code 输出 conflicting/neutral。
- Chat SSE error event。
- Responses `response.failed` 与 nested error。
- malformed/oversized/deep/complex JSON 保持 bounded conservative failure。
- 未受信 profile 的 provider 自声明 code 不产生 durable block。
- 不持久化原始 message/body。

### 11.3 Monitoring execution

- business failure excluded，technical failure eligible。
- business failure不写 Station Key technical failure/error-rate。
- account scope 短路同站 fallback。
- key/group/model scope 只短路适用目标。
- manual override 成功恢复、失败刷新证据。
- cancel/interrupted/local-before-send 排除。
- finalization retry/idempotency 不重复 target result、verdict 或 counts。

### 11.4 Scheduler

- zero/negative raw value但非权威证据不暂停。
- confirmed fresh depleted 暂停且不改 enabled。
- collector usable commit 后 1 秒内重新评估。
- business verdict到期自动恢复。
- invalidation 丢失时 idle wakeup 最终恢复。
- user disabled 在恢复后仍 disabled。
- paused monitor 手动执行可用，scheduled execution不可用。
- 并发 invalidation 合并且不丢最终状态。

### 11.5 Read model、rollup 与 retention

- excluded 不占最近 60 槽位。
- excluded 不改变 strict/effective availability。
- excluded 不进入 P50/P95、failure counts。
- excluded/exclusion counts 在 rollup 中保留。
- recent 与 rollup `total=eligible` 一致。
- excluded-only bucket 不显示 unavailable。
- raw 删除后 rollup 仍可解释排除数量。
- corrupt exclusion JSON 会标 dirty 并可重建。
- 批量 recent 查询无 N+1 且结果排序稳定。

### 11.6 前端

- 暂停 badge 与最近技术状态并存。
- 余额与订阅暂停文案不同。
- 证据过期/unknown 不显示为暂停。
- 状态柱无 excluded cell。
- 执行详情仍能找到 excluded 执行。
- 无 eligible 数据显示 `--`。
- 批量检测不再依据前端裸余额。
- Desktop table/card 与窄窗口一致。

## 12. 验证命令

任务级优先运行相关测试，最终至少执行：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_adapter_contracts
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_orchestrator
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_write_path
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_scheduler
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_persistence
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_read_model
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_buckets_retention
cargo test --locked --manifest-path src-tauri/Cargo.toml --test monitoring_faults
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_health_verdict_persistence
cargo test --locked --manifest-path src-tauri/Cargo.toml --test operational_economics_projectors
pnpm generate:bindings
pnpm generate:runtime-event-catalog
pnpm test -- src/features/channels/channelStatusViewModel.test.ts
pnpm build
pnpm verify:fast
pnpm verify:full
```

若生成命令因工作区已有并行改动产生非本任务漂移，必须先区分任务产物与用户已有改动，不能覆盖或清理未知文件。任何未执行或失败的命令必须在交付中如实列出。

## 13. 迁移、兼容与回滚

### 13.1 历史数据

- 历史 target results 默认 `availability_eligible=1`，除现有明确 skipped/cancelled/interrupted 契约外不做猜测性重分类。
- 历史 `protocol_mismatch` 不根据 terminal reason/message 回填为余额错误。
- 历史 rollup 的 `excluded_count=0`；仍保留 raw 的区间可按 dirty rebuild，已删除 raw 的区间不伪造排除数。
- Workspace V3 与当前 Desktop 同版本原子升级，不保留前端自行推断旧 DTO 的路径。

### 13.2 失败恢复

- Resolver 是纯函数，可以从 balance snapshots 与 scoped verdict 重建。
- Runner invalidation 是加速器，不是事实源；丢失后可由定时重算恢复。
- Rollup 新字段纳入现有 dirty repair。
- schema migration 必须遵守 startup upgrade journal/backup/recovery 契约。
- 不通过删除数据库、清空历史或关闭 FK/check 约束来恢复。

### 13.3 回滚边界

本计划不依赖运行时 feature flag 长期保留两套 owner。工程实施可以在任务内部使用 shadow comparison，但 cutover 完成时必须删除旧判断。若升级后必须回滚应用版本，应通过现有数据升级恢复机制处理；不得让旧二进制直接打开已升级 schema。

## 14. 可观测性与安全

新增 bounded runtime events/metrics：

- spendability projection transition：旧状态、新状态、scope、reason、profile version；不含余额原值和 secret。
- monitor sample excluded：reason、scope、classifier version。
- schedule invalidation coalesced/dropped/recomputed。
- evidence expired/recovered。

禁止 metric label：station/key 名称、URL、API key、原始 error message、原始 provider code。允许稳定 ID 时仍应遵守现有本地诊断脱敏策略。

## 15. 性能预算

- Workspace 默认 200、最大 500 行时，recent results 使用一次批量查询，不允许按行查询。
- Spendability facts 使用批量读取，不能每个 Monitor 单独读取 balance/verdict。
- 单次 resolver 复杂度目标为 `O(monitors + facts + verdicts)`。
- invalidation channel 有界且可合并；Collector/Finalization 写事务不得等待 Runner 消费。
- 新 JSON 计数字段继续使用 bounded map 和长度校验。
- `next_due` 计算不得通过无界循环反复打开 read session。

## 16. 最终验收场景

使用 Grox 等价本地 fixture 完成以下端到端验收：

1. Key 普通余额小于等于零，但有效订阅仍有额度。
2. Collector 给出 authoritative usable Station/Subscription projection。
3. Monitor 保持可调度；页面不显示余额暂停。
4. 上游若返回 HTTP/SSE `insufficient_quota`，系统保留 normalized code/scope，不再记为 protocol mismatch。
5. 该执行出现在 execution history，标记“因余额排除”。
6. 状态柱不新增红柱，可用率和 P50/P95 不变，技术 health/error-rate 不恶化。
7. 对应 scope 自动暂停后，页面显示原因、来源和有效期。
8. Collector 恢复或手动探测成功后，1 秒内重新评估并恢复调度，但用户主动停用状态不被覆盖。
9. 重启应用后相同事实得到相同 projection；过期证据按 policy 自动退回 unknown。

以上九项和第 2 节 Engineering cutover 条件全部满足，计划才可标记完成。
