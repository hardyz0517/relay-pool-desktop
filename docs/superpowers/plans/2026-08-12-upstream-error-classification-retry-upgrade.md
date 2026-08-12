# Relay Pool Desktop 上游错误分类与重试升级实施计划

状态：Ready for implementation；本次仅交付计划，代码实施尚未开始

日期：2026-08-12

上位规范：[`../../proposals/INTELLIGENT_ROUTING_ENGINE_SPEC.md`](../../proposals/INTELLIGENT_ROUTING_ENGINE_SPEC.md)

上位总计划：[`2026-08-05-intelligent-routing-engine-upgrade.md`](2026-08-05-intelligent-routing-engine-upgrade.md)

调查输入：[`../../research/SUB2API_RESPONSE_CASES.md`](../../research/SUB2API_RESPONSE_CASES.md)

适用范围：本地 OpenAI-compatible Proxy 当前公开的 `/v1/models`、`/v1/chat/completions`、`/v1/responses`、`/v1/embeddings`，以及这些端点经过 Sub2API / NewAPI / OpenAI-compatible 中转站时产生的 HTTP、SSE、网络和协议错误。

计划关系：本计划是智能路由总计划的专项后续阶段，不取代总计划，不建立第二套路由、健康或错误事实。若两者冲突，以 `AGENTS.md`、当前代码、批准规范和总计划为准。本计划实施完成后，相关任务证据应回填总计划和智能路由 qualification，而不是把本文件升级为新的总体事实来源。

> 每个任务使用 RED-GREEN-REFACTOR。先用确定性 fixture 证明当前缺陷，再实现唯一生产路径，最后删除旧分类分支并运行 task gate。任何必跑命令未以退出码 `0` 完成，对应任务不得标记完成。

---

## 1. 目标与完成定义

目标生产链路固定为：

```text
HTTP / SSE / transport evidence
  -> provider envelope parser
  -> typed semantic signal
  -> one CanonicalOutcome classifier
  -> one effect plan
       -> retry controller
       -> scoped health/capability effects
       -> request/attempt finalization
       -> OpenAI-compatible public error adapter
       -> bounded observability
```

只有同时满足以下条件才完成：

- 原始证据完整覆盖 `status/type/code/message`、`Retry-After`、协议、事件类型和输出阶段，但自由 message 不成为持久化业务键；
- capacity、overload、限流、凭据、账号、余额、配额、能力、请求、安全、服务端、传输和未知错误具有不同语义；
- capacity 在可信识别后优先对同一个 resolved target 做有界重试，不轮询同一 OpenAI capacity failure domain 内的其他 Key；
- 只有能够证明候选属于不同 provider capacity failure domain 时，capacity 才允许跨域 fallback；
- capacity、上游 5xx、并发和站点内部过载绝不产生 credential hard-fail；
- retry、health、capability、quality、日志和 public error 只消费同一份 `CanonicalOutcome`，不再根据转换后的 HTTP 状态码二次分类；
- SSE 在首个下游可见语义事件之前识别错误并允许重试；首个下游可见事件之后绝不透明切换上游；
- 发给 Codex/OpenAI SDK 的错误采用稳定 OpenAI-compatible 语义，capacity 最终耗尽映射为可重试的 `server_error`；
- 所有重试、等待、缓冲、并发、消息长度和 trace 字段有硬上限；
- 当前重复分类 switch、message 业务判断和错误作用域降级路径在同一 cutover 中删除；
- fixture、单元、集成、属性、并发、fault、架构、安全、构建和真实 provider 门禁全部通过。

## 2. 冻结决策

### 2.1 Capacity 使用同目标有界重试

可信的 provider capacity 信号包括：

- HTTP `400` 且 message 命中版本化的 `Selected model is at capacity` 特征；
- HTTP `400` 且 message 命中经确认的 `You can retry your request`、OpenAI help URL 与 request-id 组合特征；
- HTTP 或 SSE code 为 `server_is_overloaded`；
- HTTP 或 SSE code 为 `slow_down`；
- provider 原生 `529` / `overloaded_error`；
- 经评审加入同一版本化 signature registry 的等价 OpenAI capacity 响应。

默认策略为：

```text
首次请求
  -> capacity
  -> 同 resolved target 重试 1
  -> capacity
  -> 同 resolved target 重试 2
  -> capacity
  -> 若存在已证明不同的 capacity failure domain，则最多跨域 fallback 1 次
  -> 否则返回 OpenAI-compatible server_error
```

V1 参数冻结如下：

| 参数 | V1 值 |
|---|---:|
| 同目标额外 capacity retry | 最多 2 次 |
| 同一请求总 upstream attempt 硬上限 | 4 次 |
| capacity retry delay cap | 第一次 `250ms`，第二次 `1000ms` |
| jitter | deterministic equal jitter，范围为当前 delay cap 的 `1/2..=1`，种子来自内部 request/attempt identity，不接受客户端 seed |
| capacity retry 总等待预算 | `2000ms`，同时受现有 precommit deadline 限制 |
| capacity runtime cooldown | 最终耗尽后默认 `2000ms` |
| 跨 capacity domain fallback | 最多 1 次，且必须占用总 attempt 上限 |
| 单 capacity domain 同时执行 retry attempt | 最多 2 个 |
| Proxy 全局同时执行 capacity retry attempt | 最多 8 个 |
| 单 capacity domain 等待 retry admission | 最多 32 个，FIFO + cancellation |
| Proxy 全局等待 retry admission | 最多 128 个 |
| 单次错误响应 body 读取上限 | `256KiB` |
| 单次 message signature 扫描上限 | UTF-8 安全截断后的 `16KiB` |
| JSON 最大嵌套深度 | `32` |
| SSE bootstrap buffer | 每请求 `256KiB`，且必须占用 proxy shared memory budget |
| 单个待解析 SSE event 上限 | `256KiB`，bootstrap/committed 阶段一致 |
| Proxy error-body + SSE parser 共享内存硬上限 | 默认 `32MiB`，按实际 retained bytes 准入，由同一版本化 system profile 持有 |
| diagnostic memory admission | 无可用 permit 时 fail-fast，不另建无界 waiter queue |

规则：

1. 同目标是同一个 late-resolved target、credential revision、endpoint revision 和 upstream request body；任一 revision 变化必须释放 lease 并重新规划，不能拿旧 target 重试。重规划仍继承原逻辑请求已经消耗的 attempt、deadline 和 failure-domain exclusion，不能借 revision 变化重置预算或轮询同 capacity domain sibling Key。
2. capacity rule 除分类置信度外必须显式给出 `RequestAcceptance` 与 `ReplaySafety`。只有 `Confirmed + RejectedBeforeAcceptance + ReplaySafe` 才允许非幂等 POST 做同目标或跨域 replay；“尚未向下游 commit”本身不能证明上游未接收。无法确认是否已接受的 5xx/transport、`Probable` capacity 或 conflicting evidence，除非请求方法本身幂等或存在权威 provider idempotency 保证，否则不得透明重放。
3. `Retry-After` 合法且同时不超过 capacity 总等待预算与剩余 deadline 时优先采用；超过任一预算时不等待，直接返回可重试错误。
4. 所有同目标 retry 必须经过 proxy-instance shared retry admission，不能让每个请求自行制造 retry storm。
5. 同一个 OpenAI provider/model capacity domain 内的不同中转站或 Key 默认视为相关故障，不因换 Key 重复采样和惩罚。
6. Custom OpenAI-compatible endpoint 只有在可信 provider identity 表明其不是同一个 OpenAI capacity domain 时，才可视为跨域候选；不能根据站点名称或错误 message 猜测。
7. retry delay 期间必须释放当前 attempt 的网络 stream、并发/capacity lease 和临时 buffer；delay 结束后只对相同 target identity 重新取得 lease 并再次比较 revision。不能占着并发槽位睡眠，也不能重新跑普通候选选择悄悄换 Key。
8. 每次 attempt 使用同一份不可变 request body backing storage，重新构建 Authorization、时间敏感 headers 和 outbound request；不得深拷贝大 body，也不得复用 one-shot body stream 或旧认证 header。
9. V1 参数必须由一个版本化 system retry profile 持有并进入 Decision Trace；首期不作为用户设置，不允许散落为 Execution magic number。
10. shared admission 队列满时立即返回可重试 `server_error`，不得绕过预算改为同域换 Key；domain cooldown 已激活时只在剩余 deadline 足够时等待，否则立即结束。队列公平性、取消和 shutdown 由 proxy runtime owner 统一负责。
11. 同一逻辑请求的客户端/内部 idempotency identity、session hash、`previous_response_id` 和 affinity evidence 在同目标 retry 中保持稳定；attempt correlation/ordinal 单独递增。不得为每次 retry 生成新的逻辑幂等键。
12. 跨 capacity domain fallback 还必须通过 request portability 检查。带 provider-bound `previous_response_id`、不可迁移会话状态或其他 target-bound continuation 的请求默认禁止跨域，即使存在不同 capacity domain。
13. “总 upstream attempt”只统计真正越过 outbound send 边界的发送；首次发送 + 最多 2 次同目标发送 + 最多 1 次跨域发送合计最多 4 次。这些是上限而非预留槽位，之前的其他 fallback 已消耗 attempt 时，capacity 只能使用剩余额度。lease/revision 检查或 admission 在发送前失败不增加 attempt ordinal，但也不得重置总预算、等待预算或 deadline。
14. capacity runtime cooldown 使用 `Closed -> Open -> HalfOpen` 状态机；默认 Open `2000ms`，到期后同 domain 最多给一个已经到达、通过正常 admission 的用户请求发放 probe permit，其余请求有界等待或快速失败。不得生成后台模型请求，HalfOpen 也不得通过轮询 sibling Key 扩大探测并发。

### 2.2 Capacity 与站点过载必须分开

以下错误不是 provider model capacity，不使用同目标 capacity retry：

| 信号 | 分类 | 默认动作 |
|---|---|---|
| `API_KEY_AUTH_OVERLOADED` | 中转站认证服务过载 | 换不同站点 failure domain；不惩罚 Key 凭据 |
| `billing_service_error` | 中转站计费服务不可用 | 换不同站点；endpoint observe/cooldown |
| `No available accounts` | 中转站账号池耗尽 | 中转站 account-pool cooldown；不逐 Key 重试 |
| concurrency / pending queue | 运行时容量拥堵 | 短冷却或换独立容量域 |
| 普通 500/502/503/504 | endpoint/server failure | replay-safety 允许时输出前 fallback；按 endpoint 观察 |

### 2.3 证据与决策分层

输入证据必须保留：

```text
transport: http | sse | network
protocol: chat_json | chat_sse | responses_json | responses_sse | embeddings | models
http_status
event_type
error_type
error_code
bounded_message_signature
retry_after_ms
output_phase
station_id / station_key_id / endpoint_revision / credential_revision
provider_failure_domain
provider_identity_profile_version / model_alias_revision
request_acceptance / replay_safety
```

分类不是“code 永远压过 status”，而是由版本化 rule set 对证据组合做一致性校验：

1. exact normalized `code` 必须来自期望的 JSON/SSE envelope，并满足该规则允许的 status/protocol guard；
2. `type + status + protocol` 可以形成强证据，但不能覆盖与之矛盾的更高安全边界；
3. message signature 只在规则限定的 status/protocol 中生效，例如 capacity 文案只能在 HTTP 400 或合法 SSE error event 中生效；
4. status/transport 只提供保守 fallback，不能单独产生 credential block、account block 或 capability learning；
5. code、status、type 或 envelope 相互矛盾时输出 `Conflicting`，降级为 uncertain/neutral，不执行 durable hard effect。

证据置信度固定为 `Confirmed | Probable | Unknown | Conflicting`。Credential block、account block、余额耗尽和 capability learning 只接受 `Confirmed`；`Probable` 最多触发有界 runtime cooldown，只有独立 replay-safety gate 证明安全时才允许 retry。尤其是 5xx body 中出现 `invalid_api_key` 时，不得把中转站自己的上游账号故障解释成 Relay Pool 使用的 Station Key 失效。

`code` 和 `type` 比较使用 ASCII case-insensitive normalization；原值只作为进程内脱敏诊断，不作为业务键。message signature 只能返回闭合 evidence code，例如 `openai_model_at_capacity_v1`，不能把任意 message 写入 health、metric label 或 routing scope。

Rule set 只能由可信 target metadata（upstream protocol、station/gateway family、provider identity profile）选择，不能根据错误 body 自称的 provider 名称动态切换解析规则。找不到匹配 rule set 时进入通用 OpenAI-compatible conservative profile。

### 2.4 输出阶段

输出阶段固定为：

```text
NoHeaders
UpstreamHeadersObserved
ProtocolBootstrapping
DownstreamSemanticOutputCommitted
Terminal
```

- TCP 字节、HTTP headers、SSE heartbeat、`response.created` 和仅用于协议建立的控制事件不等于下游语义输出已提交；
- 在 `DownstreamSemanticOutputCommitted` 前发现 error/`response.failed`，可以终止当前 attempt 并执行 CanonicalOutcome 的 retry；
- 提交后发生错误只记录并规范化终止事件，不得透明切换上游；
- bootstrap 总 buffer 与任一待解析 SSE event 的硬上限均为 `256KiB`，时间同时受 first-event timeout 和 precommit deadline 约束；实际 retained bytes 必须先取得 shared memory permit，flush/终止后立即归还。超限或无 permit 按 malformed/uncertain/overload 失败，不得无限缓存或排队。

事件处理按完整事件顺序执行，即使 content delta 与 error 位于同一个 TCP chunk，也必须先提交 content、再按 committed error 处理，不能按整个 chunk 的最终状态回滚。协议事件分为：

| 事件类别 | 示例 | Bootstrap 行为 |
|---|---|---|
| Transport control | SSE comment、heartbeat | 忽略或有界缓存，不 commit |
| Protocol control | `response.created`、`response.in_progress` | 有界缓存，不 commit |
| Downstream-visible semantic | Chat role/content/tool/function/refusal/reasoning delta；Responses output item/content/function delta | 原序 flush 缓存并 commit |
| Successful terminal without semantic delta | 合法空响应的 completed/`[DONE]` | 原序 flush 并成功终止，不 retry |
| Failure terminal | `event:error`、`response.failed`、顶层 error | commit 前分类/retry；commit 后规范化终止 |
| Unknown but valid event | 新增 provider 事件 | 保守地先 commit 再透传；不得丢弃后假装可 retry |

### 2.5 未知错误默认策略

| 情况 | Retry | Health | Public mapping |
|---|---|---|---|
| 未知 4xx | 停止请求 | Neutral | `invalid_request_error` 或 `upstream_request_rejected` |
| 未知 5xx | 仅在 replay-safety gate 允许时于输出前换不同 endpoint failure domain，否则停止并标记 possibly-accepted | Observe endpoint | `server_error` |
| 2xx 但 body 是合法 error/`response.failed` envelope | 按 envelope 语义处理，不得算成功 | 按 typed signal | 对应 OpenAI error |
| 2xx 但 Content-Type/协议不符或 HTML body | 输出前停止或换 endpoint | Observe/uncertain endpoint | `server_error` |
| 3xx redirect | 不携带 Authorization 跟随；按 endpoint 配置错误处理 | Neutral/observe endpoint | `server_error` |
| 407 proxy authentication required | 停止当前 proxy route | Local/outbound proxy config，Station Key neutral | `server_error` |
| 408/425 | 仅在 replay safety 与 deadline 允许时 retry | Observe endpoint | `server_error` |
| 413 | 停止请求 | Request neutral | `invalid_request_error` |
| 499/客户端取消 | 不 retry | Downstream/request neutral | 不再主动写响应 |
| 连接未建立且可证明未发送 request body | 在安全预算内 retry/fallback | Observe endpoint | `server_error` |
| 请求可能已被接受的 transport failure | 停止或标记 uncertain | Neutral/uncertain | `server_error`，不得暗示未执行 |
| committed 后 stream failure | 不 retry | 按语义作用域观察 | SSE terminal `server_error` |

未知响应绝不产生 credential hard-fail，也不触发 capability learning。

## 3. 目标分类矩阵

| 语义 | 代表 code/type/message | Failure target | Retry disposition | Health/capability effect |
|---|---|---|---|---|
| Credential invalid | `INVALID_API_KEY`、`API_KEY_DISABLED`、`API_KEY_EXPIRED`、可信 `authentication_error` | Station Key credential revision | 换 credential failure domain | Credential block；不按 15 分钟自动恢复 |
| Station account disabled | `USER_NOT_FOUND`、`USER_INACTIVE` | Station account | 换 station account domain | Account block，等待 account subject revision 或可信恢复证据 |
| Group/subscription invalid | `GROUP_DELETED`、`GROUP_DISABLED`、`GROUP_NOT_ALLOWED`、`SUBSCRIPTION_NOT_FOUND`、`SUBSCRIPTION_INVALID` | Station account/group binding | 换独立站点/分组 | Policy/account verdict；不污染 Key credential |
| Balance depleted | `INSUFFICIENT_BALANCE`、上游 payment required | Station account 或明确 Key quota scope | 换独立余额域 | Depleted verdict；由余额事实或可信成功恢复 |
| Quota exhausted | `API_KEY_QUOTA_EXHAUSTED`、`insufficient_quota` | Key/account quota scope | 换独立 quota domain | 按 reset/Retry-After 冷却；缺失时有界 half-open |
| Rate limited | `rate_limit_exceeded`、普通 429 | code 指示的 Key/account/provider scope | Wait/replan 或换独立 rate-limit domain | Cooldown，尊重 `Retry-After` |
| Concurrency/queue | concurrency limit、pending queue | Runtime capacity scope | 短等待或换独立容量域 | Runtime-only cooldown，不写 credential failure |
| Provider capacity | capacity signature、`server_is_overloaded`、`slow_down`、529 | Provider/model capacity domain | `RetrySameTarget`，耗尽后仅跨不同 domain | Runtime-only cooldown；Key/账号 durable health neutral |
| Model unsupported | `model_not_found`、`model_not_available` | Model on Key | 换支持该模型的候选 | Confirm unsupported model，仅在 applicability 允许时学习 |
| Protocol/endpoint unsupported | 405、501、可信 endpoint 404、`compact_not_supported` | Provider protocol/endpoint | 换支持该协议的候选 | Confirm unsupported protocol/endpoint |
| Request rejected | 普通 400/409/422、上下文过长、字段错误 | Request | Stop | Neutral |
| Request too large | 413、可信 body-too-large code | Request | Stop | Neutral |
| Safety/policy | `content_policy_violation`、cyber policy | Request/session | Stop | Neutral；不得降低 Key 健康 |
| Redirect/proxy route failure | 3xx、407、协议 URL 配置错误 | Station endpoint 或 local outbound proxy config | 不带凭据跟随；换独立 endpoint 或 Stop | Key credential neutral |
| Client cancelled | 499、downstream disconnect/cancel | Downstream/request | Stop | Neutral |
| Relay service unavailable | auth/billing service overload、无账号池 | Station endpoint/account pool | 换独立 station domain | Observe/cooldown station service |
| Upstream server/transport | 5xx、connect、timeout、malformed | Station endpoint | 仅在独立 replay-safety gate 允许时输出前 fallback | Observe endpoint；达到阈值后 circuit；possibly-accepted 计费不确定 |
| Unknown | 动态或无法确认 | Uncertain | 按第 2.5 节 | Neutral unless typed endpoint evidence |

## 4. 目标代码边界

Task 0 可以根据实际依赖做一次有证据的文件名微调，但职责不得变化。

| 路径 | 最终职责 |
|---|---|
| `src-tauri/src/services/proxy/adapters/error_envelope.rs` | 解析 HTTP/OpenAI/Responses envelope，生成 bounded typed evidence；不决定 health/retry |
| `src-tauri/src/services/proxy/adapters/error_rules.rs` | 版本化 provider/gateway rule set、confidence 与冲突解析；纯函数、无 I/O |
| `src-tauri/src/services/proxy/protocol/chat_sse.rs` | 以完整 SSE event 识别 Chat protocol progress/error evidence |
| `src-tauri/src/services/proxy/protocol/responses_sse.rs` | 以完整 SSE event 识别 Responses progress/error evidence |
| `src-tauri/src/application/request_finalization/failure.rs` | 闭合 failure target/class/effect/public source 类型 |
| `src-tauri/src/application/request_finalization/classifier.rs` | 唯一 provider semantic signal -> CanonicalOutcome classifier |
| `src-tauri/src/application/request_finalization/effect_planner.rs` | CanonicalOutcome 到 retry/health/capability/finalization effect plan 的无损转换 |
| `src-tauri/src/application/routing_engine/failure_domains.rs` | provider/account/endpoint/key/model/capacity failure-domain identity 与比较 |
| `src-tauri/src/services/proxy/routing_runtime.rs` | proxy-instance shared retry/capacity admission、计数和 bounded cooldown overlay |
| `src-tauri/src/services/proxy/execution.rs` | 薄编排：执行 effect plan；不再按 HTTP status/code 二次分类 |
| `src-tauri/src/services/proxy/response_body.rs` | bootstrap/commit/terminal 生命周期与 committed 后错误规范化 |
| `src-tauri/src/services/proxy/public_error.rs` | Canonical public error source 到 OpenAI-compatible HTTP/SSE 输出 |
| `src-tauri/src/application/health_transitions.rs` | 只消费 scoped health effect；不从状态码和 message 推导语义 |
| `src-tauri/src/application/observation_ingestion.rs` | 原子写入 scoped typed observation，禁止复制分类逻辑 |
| `src-tauri/tests/fixtures/sub2api_errors/` | 明显假值、无 secret 的 HTTP/SSE envelope fixtures |
| `src-tauri/tests/upstream_error_contract.rs` | 跨模块分类、effect、public mapping golden contract |

扩展约束：首期不建立动态插件注册表或通用规则 DSL。新增 provider/gateway 只允许新增一个闭合 typed rule set 和 conformance fixtures；Transport、CanonicalOutcome、effect planner、health 和 public adapter 不应因新增供应商 code 而增加并行 switch。所有 rule set、provider identity、model alias、retry profile 和 public mapping profile 都携带版本并进入 trace/replay。Rule set 由可信 target metadata 静态选择，body 只能提供 evidence，不能选择自己的解释器。

若现有 `routing_health_axes`、observation scope 和 projectors 无法表达 credential/account/endpoint/model 多作用域，Task 6 必须通过下一可用 append-only migration 补足 typed scoped verdict；不得继续把所有 effect 压入 `routing_health_snapshot.station_key_id`，也不得新建与现有 Observation/Health projector 平行的第二事实源。

## 5. Task 依赖图

```text
0 Baseline / fixture catalog / deletion ledger
  -> 1 Error evidence and envelope parser
  -> 2 Canonical classifier and effect contract
       -> 3 Failure domains and same-target capacity retry runtime
       -> 4 HTTP production cutover
       -> 5 SSE bootstrap and post-commit terminal handling
       -> 6 Scoped health/capability persistence and projection
       -> 7 OpenAI-compatible public error adapter
  4 + 5 + 6 + 7 -> 8 Observability and decision trace
  8 -> 9 Atomic old-path deletion and architecture gates
  9 -> 10 Qualification and real-provider release gate
```

Tasks 3、5、6、7 可以在 Task 2 后并行准备测试，但 Tasks 4-9 是同一个不可拆分的 production cutover 单元。任何中间 revision 不得保留双分类器并作为可交付状态。

## 6. 可执行任务

### Task 0：冻结基线、fixture catalog 与删除台账

变更范围：

- 本计划；
- 新建 `docs/superpowers/audits/upstream-error-classification-baseline.md`；
- 新建或更新智能路由 deletion ledger/boundary manifest 中与重复分类有关的条目。

步骤：

1. 记录 `git status --short --branch`、最近五个提交和所有受保护用户改动。
2. 从调查表中只提取当前四个公开端点可达的响应，不把 Images/Videos/Anthropic/Gemini/WebSocket 误纳入首期生产范围。
3. 建立 fixture catalog，至少覆盖完成定义中的每个分类族、HTTP/SSE 两种 envelope、缺字段/动态字段变体，以及第二种 OpenAI-compatible gateway 的 conformance 样本；验证新增 code 只改 rule set/fixture，不修改 consumer。
4. 登记必须删除的旧 owner：
   - `routing_failure::classify_route_failure` 中与 upstream outcome 重复的状态码分类；
   - `RetryPolicy::decide` 的 HTTP status switch；
   - Execution `attempt_failure_kind` / `health_effect` 二次分类；
   - provider adapter 中无闭合 evidence code 的 message/status 推导；
   - `ProxyFailure::from_public_error` 后再反推 retry/health 的路径；
   - 所有把 scoped effect 降成当前 Station Key 的 writeback。
5. 写 RED 测试，证明当前至少存在以下失败：
   - HTTP 400 capacity 被当作 BadRequest；
   - 首个 SSE error frame 被当作可提交首字节；
   - canonical target/retry/capability 在转换为 `ProxyFailure` 后丢失；
   - quota/concurrency/普通 rate limit 三种 429 得到相同 effect；
   - `USER_INACTIVE` 被归因当前 Key；
   - capacity 最终错误没有输出 `server_error`；
   - 2xx error envelope 被记为成功；
   - 3xx/407/413/499、矛盾 status/code 和超大 error body 没有闭合处理；
   - retry delay 持有 capacity lease 或重复分配大 request body。

Exit gate：fixture catalog、RED 证据、owner/deletion ledger 和当前行为矩阵齐全；不得实现 GREEN。

### Task 1：错误证据与 envelope parser

RED：

- nested `error.code/type/message`、顶层 Responses failure、2xx error envelope、缺失字段、数字 code、大小写 code 和非 JSON body 不能稳定生成 typed evidence；
- 任意长 message、Authorization、假 secret canary 不能进入 evidence code、metric label 或持久化 preview。

GREEN：

1. 新增 bounded `UpstreamFailureEvidence`，字段闭合且不依赖 provider-specific public DTO；包含 confidence、rule/profile version 和 conflict reason code。
2. OpenAI/Responses adapter 只解析结构并生成 semantic candidates，不决定 retry/health。
3. message signature registry 使用版本化静态 matcher，首期只加入经 Sub2API 源码确认的 capacity 特征。
4. `type/code` normalization 不修改原始响应，仅生成 canonical comparison key。
5. `Retry-After` 同时支持 delta-seconds 和 HTTP-date，保持现有七天绝对上限，并在 effect policy 中进一步受 request deadline 限制。
6. 错误 body 使用有界 streaming read；`256KiB` 上限作用于解压后的实际字节，且每次扩容前必须按实际 retained bytes 占用与 SSE parser 共用的 proxy `32MiB` memory admission。超过单响应/共享上限后立即停止解析、关闭/丢弃该 response，并只生成 `error_body_too_large` 或 `diagnostic_memory_saturated` evidence；截断 JSON/message 不得形成 durable effect。禁止继续使用无上限 `response.bytes()`，也不得让压缩传输绕过限制、为连接复用无界 drain 或在 permit 外排队缓冲。
7. JSON 在解析后验证最大深度，message signature 只扫描 UTF-8 安全截断后的 `16KiB`；截断事实进入 evidence flag。
8. 对合法 2xx error envelope、错误 Content-Type、HTML/Cloudflare body、3xx、407、413 和 499 生成闭合 typed evidence。
9. HTTP-date `Retry-After` 在接收时转换为 duration；等待和 deadline 只使用 monotonic time，wall-clock 后跳不能延长在途请求。

REFACTOR：删除旧 `openai_error_code` 的单字段捷径，所有 provider adapter 使用共同 envelope parser。

Focused gate：parser table tests、fuzz/property test、secret canary/redaction test。

### Task 2：唯一 CanonicalOutcome classifier 与 effect contract

RED：对目标分类矩阵逐行建立 golden test，证明 target、class、retry、health、capability 和 public source 六轴缺一不可。

GREEN：

1. 扩展 `RetryDisposition`：
   - `RetrySameTarget`；
   - `TryDifferentFailureDomain`；
   - `WaitThenReplan`；
   - `StopRequest`。
2. 增加 provider/model capacity failure target，或通过现有 failure-domain typed target 表达，禁止塞入字符串 detail。
3. 将 capacity、relay overload、rate limit、quota、concurrency 分成不同 failure class。
4. classifier 一次性生成完整 effect plan；public mapping 只保存语义 source，不提前丢失内部 effect。
5. unknown 4xx/5xx、缺字段和冲突字段按第 2.5 节 fail closed；durable hard effect 必须验证 `Confirmed` confidence。
6. classifier、retry 和 public mapping profile 都有稳定 version，进入 CanonicalOutcome、trace 和 replay fixture。
7. 为每个 enum variant 要求真实 producer、consumer、effect test 和 trace code；不保留 speculative dead code。
8. 将请求接受事实与计费事实拆成正交轴：`RequestAcceptance = RejectedBeforeAcceptance | AcceptedOrMayHaveBeenAccepted | Unknown`，`ReplaySafety = ReplaySafe | RequiresProviderIdempotency | NotReplayable`，`BillingState = NotBillable | UsageObserved | BillingUncertain`。下游 precommit 不得直接推出上游 rejected 或 NotBillable；只有 provider contract/rule 明确提供计费证据时才能写 `NotBillable`，否则 capacity attempt 也保留 `BillingUncertain`，不得伪装为 missing usage 或零成本。
9. 所有 retry disposition 在发送前还必须经过统一 replay-safety gate；classifier 可以提出 retry intent，但不能绕过 method/body progress、provider idempotency capability、continuation portability、attempt budget 和 deadline。

REFACTOR：删除 classifier 外根据 status/code/message 修改 canonical effect 的入口。

Focused gate：`upstream_error_contract`、effect planner、outcome invariant 和 property tests。

### Task 3：Failure domain 与同目标 capacity retry runtime

RED：

- capacity 后第二 attempt 选中了另一 Key；
- 多个并发 capacity 请求各自重试，突破 proxy-instance shared budget；
- endpoint/credential revision 变化后仍重放旧 target；
- 同一 failure domain 被多个 Key 重复计为独立失败。

GREEN：

1. 在现有 failure-domain owner 中增加 `ProviderCapacityDomain`。domain equality 只使用可信的 provider family、alias 解析后的实际 upstream model/deployment family，以及存在权威证据时的 region/deployment identity；Station ID、Key ID、普通中转 endpoint revision、provider identity profile version 和 model alias revision 只能作为 provenance/revision fence，revision 数值本身不能把同一 OpenAI capacity domain 人为拆成多个域。不得包含完整 URL、Key 或账号 secret。Custom/unknown provider 缺少可信 identity 时默认“不能证明跨域”，而不是默认每个站点独立。
2. `RetrySameTarget` 复用同一个 candidate identity，但每次重试先释放旧 attempt 资源，等待后重新取得同 target constraint lease，并验证 target/credential/endpoint revision。
3. 在唯一 proxy runtime state 中加入 `CapacityRetryProfileV1`、shared admission、`Closed/Open/HalfOpen` domain cooldown、error-body/SSE parser 共用 memory admission 和 monotonic deadline。
4. 实现第 2.1 节的 attempt、delay、jitter、deadline、single-probe HalfOpen 和 cross-domain 限制；总 attempt 必须复用 Coordinator 的 request-local budget，不能另建一套 capacity 计数后与普通 fallback 叠加突破上限。
5. 同域 sibling Key 不进入 capacity fallback；不同域候选必须由 planner/failure-domain projector证明，而不是 Execution 手写筛选。
6. trace 记录 attempt ordinal、retry kind、domain commitment、suppression reason、provider identity profile version 和 model alias revision，不记录完整 domain 原料。domain commitment 使用版本化 canonical encoding 对非 secret identity 计算固定长度 digest，并随 outcome 保存，历史 replay 不用最新版 alias/profile 重算；alias revision 变化只触发 fence/replan，只有解析后的实际 upstream model/deployment identity 改变才可能改变 domain equality。
7. request body 使用共享 immutable bytes/lease；每次 attempt 重新构建 outbound request 和认证 header，不复制完整 payload。
8. domain/global admission 使用第 2.1 节冻结的 in-flight/waiter 上限、FIFO、公平取消和 shutdown 行为；admission 被拒绝时不得回退到无预算 retry。revision/fence 在发送前失效时释放 permit 并携带既有预算和 domain exclusion 重规划，不能重置逻辑请求。
9. cross-domain admission 先验证 request portability；`previous_response_id`、target-bound affinity 或 provider-specific continuation 不可迁移时记录 `capacity_cross_domain_not_portable` 并返回 `server_error`。

REFACTOR：Execution 不再自己维护“同 Key重试计数”和“全局 retry budget”。

Focused gate：deterministic retry、parallel admission、deadline、revision fence、shutdown/cancel、same-domain suppression、cross-domain fallback tests。

### Task 4：HTTP 错误生产切换

RED：为目标分类矩阵的 HTTP 行建立从 fake upstream 到 attempt terminal 的端到端测试。

GREEN：

1. `UpstreamAttemptExecutor` 对非 2xx 以及 2xx semantic error body 先解析 evidence，再调用唯一 classifier；不能把“HTTP 成功”直接等同于协议成功。
2. Execution 直接执行 canonical retry disposition，不经过 public HTTP status。
3. HTTP 400 capacity 进入同目标 retry；普通 400 保持 StopRequest。
4. 401 按 code 分成 Key credential、Station account 和 unknown auth；只有可信 credential code 才产生 credential block。
5. 403 按 balance/group/subscription/policy/auth 分流；禁止仅凭 403 hard-fail Key。
6. 429 按 quota/rate/concurrency/auth-rate-limit 分流；尊重 scope 和 Retry-After。
7. 404/405/501 只有可信 capability evidence 才学习能力；未知 404 不污染 capability。
8. 3xx 不自动携带 Authorization 跟随；407 归因 local/outbound proxy config；所有 408/425、5xx、timeout、malformed、capacity retry/fallback 都受统一 replay-safety gate 限制；413 和 499 不 retry。
9. 非成功 attempt 的 acceptance、replay-safety 与 billing state 一起 finalization；不得因“HTTP/SSE 尚未向下游 commit”推导 NotBillable。只有 rule/provider contract 提供权威证据时 capacity rejection 才记录 NotBillable，否则与 ambiguous 5xx/transport 一样记录 BillingUncertain。

REFACTOR：删除 `RetryPolicy::decide` status switch、Execution `health_effect`、`attempt_failure_kind` 和 `ProxyFailure` public-status 反推 effect。

Focused gate：HTTP integration matrix、max attempts、effect persistence atomicity、request outcome tests。

### Task 5：SSE bootstrap、有效输出与 committed 后终止

RED：

- Chat `event:error`、Responses `response.failed` 作为第一完整事件时当前路径已提交 200 stream；
- capacity error 被切成多个 TCP chunk 时解析失败；
- `response.created` 后 capacity 被误认为必须停在当前上游；
- 真正 content/tool/reasoning delta 后发生错误仍尝试 fallback；
- content 与 error 位于同一 TCP chunk 时错误地把整个 chunk 当成 precommit；
- 合法空响应 completed 被误判为 incomplete；
- 未知合法事件被静默丢弃后继续 retry；
- 多并发 bootstrap 绕过 shared memory budget。

GREEN：

1. bootstrap 按完整 SSE event 驱动 protocol machine，不按任意非空 chunk 判定成功。
2. 在下游 headers/body commit 前缓存控制事件；收到首个下游可见语义事件时原序 flush 并进入 committed。
3. precommit error 生成与 HTTP 相同的 `UpstreamFailureEvidence` 和 CanonicalOutcome。
4. capacity/rate/5xx 在 precommit 阶段先执行统一 replay-safety gate，再按 effect plan 重试；precommit 只描述下游输出状态，不能代替上游 acceptance 证明。重试前释放旧 stream、lease 和 attempt reservation。
5. committed 后 error 不 retry；将安全可改写的 terminal code 映射为客户端 `server_error`，同时保留内部 canonical class。
6. 处理 chunk boundary、CRLF/LF、多 event chunk、heartbeat、`[DONE]`、response completed/failed/incomplete 和 EOF。
7. 同一 chunk 内按事件顺序推进 commit；合法空成功终态可在无 semantic delta 时完成；未知合法事件采用保守 commit+透传。
8. bootstrap 与 committed SSE parser 的所有 retained bytes 必须持有 error-body/SSE 共用的 `32MiB` shared memory permit；单 event/单 bootstrap 超过 `256KiB`、admission 拒绝、等待取消、downstream drop、event flush 和 retry 时立即释放对应 permit并产生稳定 reason，不能退化为无界缓冲。
9. committed error 只允许结构化改写尚未向下游发送的当前 terminal event；若无法安全改写则原样发送一次并记录 mapping failure，禁止再注入第二个 terminal。
10. buffer、event、idle timeout、drop/cancel 和双终态 finalization 保持有界、exactly-once。

REFACTOR：`response_body.rs` 不再把所有显式 failed terminal 压成无语义 `UpstreamStreamFailed`。

Focused gate：Chat/Responses SSE state-machine、partial chunks、precommit retry、postcommit no-retry、RAII lease、downstream drop tests。

### Task 6：Scoped health、capability 与恢复语义

RED：证明 credential/account/endpoint/model/capacity 五种 effect 当前都落到 Station Key 或未被投影。

GREEN：

1. CanonicalOutcome 与 attempt terminal 在同一 transaction 追加 scoped Observation，并应用关键 health transition。
2. scope 规则：
   - credential invalid -> Station Key credential revision；
   - user/group/subscription -> Station account/group scope；
   - 5xx/transport -> endpoint revision；
   - model not found -> model-on-key capability；
   - capacity/concurrency -> runtime failure domain overlay，不写 durable credential failure。
3. credential block 与 credential revision 绑定；凭据更新后旧 block 自动失效。未经可信成功或 revision 变化，不靠固定 15 分钟假装恢复。
4. account/group verdict 与 account/group subject revision 绑定，只由对应账号/分组事实 revision 变化或可信同 scope 恢复 evidence 解除；endpoint revision 或另一 Key 的无关 success 不能清除。
5. quota/rate cooldown 使用 Retry-After/reset evidence；缺失时进入有界 half-open，而不是永久 block。
6. capability effect 真正写入唯一 capability owner，并由下一 PlanningSnapshot 排除不兼容候选。
7. 迟到/重复 Observation 按 ordering identity 和 revision 幂等处理，不得让旧 success 清除新 failure。
8. 当前生产 `routing_health_snapshot` 只有 Station Key 维度，Task 6 必须使用下一可用 append-only migration 建立 scoped durable verdict，而不能把多作用域继续留给测试投影。目标表 `routing_health_verdicts` 至少包含：
   - 由 typed constructor 生成的 canonical `scope` 与闭合 `scope_kind`；
   - `station_id`、可选 `station_key_id`、可选 `model`；
   - endpoint/credential/account/group/model-alias subject revision fence；
   - `admit | degraded | cooldown | blocked` verdict；
   - 可选 `cooldown_until_ms`、稳定 `evidence_code`、source observation identity/order；
   - projector version、updated time，以及按 scope shape 的 SQL CHECK/unique constraint。
9. `PlanningSnapshotBuilder` 在同一 durable read transaction 中批量读取 station-key credential、station-account、endpoint 和 model/capability verdict；禁止 N+1、字符串反解析和页面 DTO 回流。
10. `routing_health_verdicts` 由 versioned projector 从 immutable Observation 幂等重建；capacity/concurrency runtime-only effect 不落该表。旧 `routing_health_snapshot` 在原子 cutover 后停止作为 Planner 权威输入，观察期只读兼容和最终删除进入 ledger。
11. migration 必须有 postcondition、schema15->latest fixture、current-schema upgrade、projector rebuild、reset/reimport 和 rollback rehearsal；不修改已发布 migration。
12. `routing_health_axes` 继续只保存 availability/latency/reliability/freshness 数值轴；`routing_health_verdicts` 只保存 admission/block/cooldown 判定。两者不得复制同一字段或互相反向解析，均从同一 Observation 源按各自 projector version 派生。

REFACTOR：停止把所有 health writeback 映射到 `routing_health_snapshot.station_key_id`；旧表如需观察期只能只读，删除进入独立后续 migration。

Focused gate：scope matrix、single-transaction batch read、revision recovery、late event、duplicate event、projector rebuild parity、migration/postcondition/schema15/reset-reimport tests。

### Task 7：OpenAI-compatible public error adapter

RED：验证当前 `type=relay_pool_error/code=upstream_overloaded` 不能满足冻结的 Codex 重试契约。

GREEN：

| Canonical class | HTTP/SSE public status | type | code |
|---|---:|---|---|
| Provider capacity / transient overload exhausted | 503；committed SSE 保持 200 | `server_error` | `server_error` |
| Generic retryable upstream 5xx | 502/503 | `server_error` | `server_error` |
| Rate limited | 429 | `rate_limit_error` | `rate_limit_exceeded` |
| Quota exhausted | 429 | `insufficient_quota` | `insufficient_quota` |
| Local proxy authentication failure | 401 | `authentication_error` | `invalid_api_key` |
| Upstream credential pool exhausted | 502 | `server_error` | `server_error` |
| Invalid request | 400 | `invalid_request_error` | `invalid_request_error` 或 public allowlist 中的兼容 code |
| Model unavailable | 404 | `invalid_request_error` | `model_not_found` |
| Local routing/config/invariant | 对应本地状态 | `relay_pool_error` | 稳定内部 public code |

1. public adapter 不参与 retry/health/capability 决策。
2. 非 committed streaming 失败在重试耗尽后返回普通 HTTP JSON error；不先发送 SSE 200。
3. committed Chat SSE 输出标准 error event；Responses 输出标准 `response.failed`，capacity code 规范化为 `server_error`。
4. `Retry-After` 在适用时透传安全规范化值。
5. message 使用稳定、脱敏、有限文案；不把上游完整 body、request id、URL、账号或 secret 暴露给客户端。
6. 本地客户端认证和上游 Station Key 认证严格分开：本地认证成功后，上游 401/credential exhaustion 绝不向客户端返回 401，避免 Codex 误判自己的本地 Key 无效。
7. public mapping profile 版本进入 golden fixture；改变 status/type/code 任一项都视为兼容性变更并要求 Codex/SDK contract test。
8. provider-specific/internal reason code 只进入脱敏 trace；public adapter 不透传未知上游 code，也不把 `upstream_authentication_failed` 等内部拓扑语义暴露为客户端重试契约。

Focused gate：HTTP/SSE golden snapshots、Codex retry-code contract、redaction、header allowlist tests。

### Task 8：可观测性、trace 与解释

GREEN：

1. 增加低基数指标：classification class、confidence、classifier/retry/public profile version、provider-identity/model-alias profile version、evidence source、retry kind、same-target retry count、domain suppression、pre/post-commit terminal。具体 revision/identity 只进 bounded trace，不作为 metric label。
2. Decision Trace 展示稳定 reason code，例如：
   - `capacity_same_target_retry`；
   - `capacity_same_domain_fallback_suppressed`；
   - `capacity_cross_domain_fallback`；
   - `credential_revision_blocked`；
   - `sse_error_before_semantic_commit`。
3. request/attempt log 保留 canonical class、target kind、retry disposition、health effect 和 public code，不保存完整动态 message。
4. 同目标 retry 每次都有独立 attempt ordinal，但同一 failure domain 的相关失败在 reliability 聚合中按规范去重/降权，不能放大失败率。
5. metrics labels、trace、IPC 和截图 fixture 通过 secret、URL、query、动态高基数审计。
6. cost trace 分别展示 request acceptance 与 billing state，区分 rejected + not-billable、rejected + billing-uncertain、usage-observed 和 possibly-accepted + billing-uncertain；不得因为 capacity retry 让整个请求成本错误显示为缺失或零成本。
7. Observation/AttemptOutcome 持久化 stable evidence code、classifier profile version、已分类的 typed effect 和版本化 failure-domain commitment，但不持久化原始自由 message。Projector rebuild 使用当时已经分类的 immutable outcome，不用最新版 rule set/alias/provider identity 静默重解释历史；若未来需要重新分类，必须使用显式 versioned replay/migration 和新的幂等 identity。
8. proxy 状态/Decision Trace 暴露 retry admission 饱和、shared diagnostic-memory 饱和、classifier/projector/lifecycle writer fail-closed 和 profile-version mismatch；这些诊断不改变 canonical 决策，也不携带动态高基数 message。

Focused gate：observability contract、trace replay、bounded metric buffer、redaction tests。

### Task 9：原子删除旧路径与架构门禁

本 Task 与 Tasks 4-8 属于同一 cutover candidate。

删除目标：

- upstream outcome 使用的 `routing_failure::classify_route_failure`；
- `should_fallback(status)`；
- `RetryPolicy::decide` 状态码矩阵；
- Execution `attempt_failure_kind` / `health_effect`；
- `ProxyFailure` public response -> retry/health 反推；
- OpenAI/Responses 重复错误 code extractor；
- explicit SSE terminal -> generic stream failure 的降级路径；
- scoped effect -> 当前 Station Key 的兼容 writeback；
- 仅验证旧行为的 tests/fixtures/dead-code suppression。

架构门禁必须拒绝：

- 在 classifier 之外匹配 `server_is_overloaded`、`slow_down` 或 capacity message；
- 在 Execution/Health/Public adapter 中按 HTTP status 重新推导 canonical effect；
- 同一 capacity failure domain 逐 Key fallback；
- committed 后 transparent retry；
- 任意自由 message 进入 durable scope、metric label 或 capability key；
- production/test 使用不同 failure-domain 或 retry contract。
- provider 新 code 直接在 Execution/Health/Public adapter 增加 switch，而不是进入版本化 rule set。
- 无界 error body 读取、未持有 shared memory permit 的 SSE bootstrap 或 retry sleep 持有 capacity lease。

Exit gate：旧 owner 零生产引用、manifest/ledger 清零、production composition 只接新链、生成物无漂移。

### Task 10：Qualification 与真实 provider 门禁

#### 10.1 必跑自动化

在仓库根目录使用 PowerShell：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_failure_contract
cargo test --locked --manifest-path src-tauri/Cargo.toml --test upstream_error_contract
pnpm.cmd verify:fast
pnpm.cmd verify:full
```

如果新增或修改 generated binding、IPC、ACL、migration、schema manifest，必须额外运行仓库对应生成/check 命令；不得手工修改生成物来消除 diff。

#### 10.2 Fault 与并发矩阵

至少验证：

- 100 个并发 capacity 请求仍受 proxy-instance shared retry admission 限制；
- 100 个并发 error body/SSE parser 仍受同一 `32MiB` shared memory admission 限制，最大保留内存不超过配置预算；
- retry wait 期间 shutdown/cancel 能及时释放 lease、permit 和 buffer；
- monotonic deadline 不受 wall-clock jump 影响；
- response event 被任意 chunk 切分仍得到相同分类；
- 单个 pre/post-commit SSE event 超过 `256KiB` 时有界终止，permit exactly-once 释放，不继续累积到下一个 delimiter；
- content/error 同 chunk 保持事件顺序，2xx error body、3xx、407、413、499 和超大 body 均命中闭合策略；
- status/type/code 冲突只能得到 Conflicting/Neutral，不能产生 credential/account/capability durable effect；
- classifier panic/error、projector unavailable、lifecycle writer unavailable 时 fail closed；
- duplicate/late terminal 不会写两次 health/capability；
- 单候选池不会因普通 runtime suppression 形成永久无候选，但 credential hard block 不得被 max-ejection guard 放开；
- restart 后旧 runtime capacity cooldown/permit 不回写新 runtime instance。
- 最大尺寸 error body、最大 bootstrap buffer、最大 attempt 数和 deadline 组合下执行 bounded-memory soak；request body retry 只能共享 backing storage，内存不得按 attempt 倍增。
- domain/global retry admission 的 FIFO、公平性、waiter 上限、队列满快速失败、cancel 和 shutdown 均有确定性测试。
- `Closed/Open/HalfOpen` domain cooldown 在并发和 fake clock 下最多放行一个 probe，进程 restart 不继承旧 permit。
- `Probable` capacity、possibly-accepted 5xx/transport 和 precommit SSE 若没有独立 replay-safe 证据，均不会透明重放；权威 provider idempotency 能力存在时才按同一 budget 放行。

#### 10.3 真实 provider smoke

必须由用户明确提供测试授权和假业务请求后执行，测试输出不得保存真实 Key 或完整 response。至少包括：

1. 一个能够稳定注入/模拟 HTTP 400 capacity 的 OpenAI-compatible upstream；
2. 一个首事件为 `server_is_overloaded`/`slow_down` 的 SSE upstream；
3. 一个真实 Sub2API 测试站点的普通成功、401、429 和 5xx 路径；
4. Codex 客户端确认最终 `server_error` 会进入其重试路径；
5. 证明 capacity 期间 selected station/key identity 未切换，且 Key 未写 credential failure。

未经授权或无法稳定制造真实 capacity 时，release qualification 必须标记该项未完成，不能用 fixture 冒充真实 provider 证据。

## 7. 验收矩阵

| 场景 | 必须观察到的行为 |
|---|---|
| HTTP 400 capacity，第二次同目标成功 | 两个 attempt 使用同 station/key/credential/endpoint revision；客户端只看到成功 |
| HTTP 400 capacity，三次均失败 | 不轮询同 OpenAI domain sibling Key；最终 503 `server_error` |
| 存在可信不同 provider capacity domain | 同目标 retry 耗尽后最多跨域一次，并写明确 trace |
| 初次 + 两次同目标 + 一次跨域均失败 | 恰好最多 4 个 outbound attempt；任何 replan/admission/fence 变化都不重置预算 |
| 带 `previous_response_id` 的 provider-bound continuation | 可同目标 capacity retry；禁止跨域 fallback |
| `Probable` capacity 或 possibly-accepted 5xx | 没有独立 replay-safe/idempotency 证据时不透明重放；billing 保持 uncertain |
| 普通 HTTP 400 | 不 retry；Key/endpoint health neutral |
| 2xx error envelope | 不记 success；按 envelope canonical class 处理 |
| 5xx body 内含 `invalid_api_key` | Conflicting/endpoint failure；绝不 block Relay Station Key |
| 3xx redirect | 不携带 Authorization 跟随；按 endpoint 配置错误处理 |
| 407 | local/outbound proxy config failure；Station Key neutral |
| 413 | Stop + request neutral；不 fallback |
| 499/client cancel | 立即停止、释放资源，不 retry、不主动写第二响应 |
| `API_KEY_AUTH_OVERLOADED` | 不走 provider capacity 同目标 retry；不 credential hard-fail |
| `INVALID_API_KEY` | 当前 credential revision block；换独立 credential domain |
| `USER_INACTIVE` | station account block；不伪装为单 Key 失效 |
| `INSUFFICIENT_BALANCE` | balance/account verdict；换独立余额域 |
| `API_KEY_QUOTA_EXHAUSTED` | quota cooldown/block；与普通 429 分开 |
| concurrency/pending 429 | runtime-only 短冷却；不长期惩罚 Key |
| `model_not_found` | capability effect 持久化；下一快照不再选相同 model-on-key |
| 首个 SSE event 是 capacity error | 未提交下游，执行同目标 retry |
| `response.created` 后、实际下游 flush 前 capacity | 丢弃已缓存控制事件并安全 retry |
| 首个 content/tool/reasoning 输出后 capacity | 不切换上游；标准 `server_error` terminal |
| content 与 error 位于同一 chunk | content 先 commit，随后 postcommit error；不 retry |
| 合法空 completed | 成功完成，不因无 delta retry |
| 未知合法 SSE event | 保守 commit+透传，不静默丢弃后 retry |
| 未知 4xx | Stop + Neutral，不 credential hard-fail |
| 未知 5xx | 输出前跨 endpoint failure domain；Observe endpoint |
| Retry-After 超过 deadline | 不阻塞超预算，返回 retryable error |
| Retry-After 未超过 deadline 但超过 capacity 总等待预算 | 不阻塞超预算，返回 retryable error |
| credential/model-alias/endpoint revision 在 retry 前变化 | 释放旧 lease并携带原预算与 domain exclusion 重规划，不发送旧凭据、不按 revision 拆 capacity domain |
| capacity retry backoff | 不持有旧 capacity lease、stream 或 bootstrap memory；重试时重新取得同 target lease |
| capacity rejected attempt cost | acceptance 与 billing 分轴；无权威计费证据时为 BillingUncertain，不污染成功 attempt usage/cost |
| domain/global retry waiter 队列已满 | 立即返回 503 `server_error`；不换同域 Key、不突破 attempt/wait budget |
| capacity cooldown 到期的并发请求 | HalfOpen 只放行一个同域 probe，其余有界等待或快速失败 |
| error body/SSE parser 总保留量到达 32MiB | 新 admission 立即有界失败并释放资源；没有第二层 waiter/buffer，进程内存不随并发无界增长 |
| 进程重启 | 旧 runtime permit/cooldown 不跨 instance 回写 |
| 日志/trace/metric | 无完整 Key、URL query、动态高基数 message 或原始认证 body |

## 8. 完成后系统不变量

1. 同一次上游失败只有一个 canonical classifier owner。
2. public error 是 classifier 的消费者，不是 retry/health 的输入。
3. capacity 是 provider/model failure-domain 事实，不是 credential failure。
4. 同一 capacity domain 不逐 Key 重试。
5. retry 和 health 相互独立：可以 retry 但 health neutral，也可以 stop 但记录 scoped verdict。
6. capability 只有可信、适用的 evidence 才能学习。
7. HTTP 与 SSE 对相同语义生成相同 CanonicalOutcome。
8. TCP 字节不等于下游语义输出提交。
9. committed 后不透明切换上游。
10. credential block 与 revision 绑定，不靠任意固定 TTL 伪恢复。
11. transient runtime capacity 不进入 durable credential truth。
12. 未知错误不以激进猜测污染 Key、账号或模型事实。
13. Durable hard effect 必须来自 envelope/status/code 一致的 Confirmed evidence。
14. retry sleep 不持有 capacity lease，所有 error/bootstrap body 受 shared memory 与尺寸预算约束。
15. 新 provider 通过版本化 typed rule set 扩展，不修改 retry/health/public consumer 的分类 switch。
16. 本地认证失败与上游 credential failure 的客户端状态码不会混淆。
17. ProviderCapacityDomain 不以 Station/Key 数量分裂；无法证明跨域时默认抑制跨候选 capacity fallback。
18. Planner 的 station-key、station-account、endpoint、model verdict 来自同一批量 scoped read model，生产不再把多作用域健康留在 test-only projection。
19. 历史 CanonicalOutcome 按其 classifier profile version 重建投影，不被新 rule set 静默改写语义。
20. 下游尚未 commit 不等于上游尚未接受；任何透明 retry/fallback 都必须通过独立 replay-safety gate。
21. RequestAcceptance 与 BillingState 不互相反推；没有权威计费证据时不得把失败 attempt 伪装成零成本。
22. provider identity/model alias revision 只参与 provenance、trace 与 fence；capacity domain equality 只由解析后的可信实际 identity 决定。
23. 历史 failure-domain equality 使用 outcome 已保存的版本化 commitment，不以最新版 alias/provider profile 静默重算。

## 9. 回滚与停止条件

- Tasks 4-9 是原子 cutover；发现严重回归时回滚整个 cutover revision，不恢复双 classifier feature flag。
- Task 6 若引入 migration，只允许 additive migration与可验证 projector rebuild；旧表删除必须等待观察期并使用独立 migration。
- 任一以下情况发生时停止推进并修复，不得降低门禁：
  - capacity retry 可能重复已被 provider 接受的请求；
  - SSE 无法可靠判断下游是否已提交；
  - failure domain 需要从 secret、完整 URL 或自由 message 推导；
  - scoped health 无法与现有 Observation/Projector 原子接线；
  - Codex public error contract 未经真实客户端验证；
  - shared retry admission、deadline、cancel 或 lease cleanup 无法证明有界。
- 回滚后 additive schema 可保留未消费结构，但必须记录为未启用并在后续清理；不得让旧新 writer 同时写入。

## 10. 交付清单

完成实施时必须交付：

- 本计划各 Task 的 RED/GREEN/REFACTOR 证据；
- fixture catalog 和 Sub2API 表到 canonical class 的映射审计；
- deletion ledger 与 architecture gate 结果；
- HTTP/SSE/Codex golden contract；
- capacity same-target、same-domain suppression 和 cross-domain fallback trace；
- scoped health/capability migration、postcondition 和 rebuild 证据；
- `verify:fast`、`verify:full`、Cargo、生成物和真实 provider smoke 结果；
- 残余风险、未完成授权测试和发布建议；
- 更新智能路由 qualification、acceptance matrix 和 `docs/README.md` 状态。

## 11. Plan 质量审计结论

| 维度 | 已冻结保障 | 实施期仍需证明的外部证据 |
|---|---|---|
| 可靠性 | 同目标有界 retry、独立 replay-safety gate、同域 suppression/单探针 HalfOpen、共享 retry/memory admission、monotonic deadline、lease/memory 释放、HTTP/SSE 边界、conflict downgrade、scoped recovery、exactly-once finalization | 真实 capacity/流式故障、100 并发 soak、sleep/resume/shutdown、Codex 客户端重试 |
| 可维护性 | 唯一 CanonicalOutcome owner、acceptance/replay/billing 正交建模、effect 无损传递、版本化 profile、原子旧链删除、闭合 trace code、golden fixtures、migration/rebuild owner | architecture gate 零旧 owner、当前脏工作区合并冲突审计、全量 `verify:full` |
| 可拓展性 | typed provider rule set、保守 unknown fallback、provider/model-alias/profile version、failure-domain projector、HTTP/SSE 共用 evidence/outcome、consumer 不识别供应商 code、第二 gateway conformance | 未来新 code 的 rule-set-only 变更演练、真实不同 deployment identity 的跨域证明 |

审计后的残余风险与处理：

1. OpenAI capacity 是否真正全局会随 provider 部署变化；计划不假设站点一定独立，只有权威 provider/deployment identity 才允许跨域。
2. Codex 对 status/type/code 的重试行为属于外部客户端合同；fixture 只能做回归，发布前仍必须完成真实 Codex smoke。
3. Sub2API 动态 message 会变化；只有版本化 signature 命中才获得 Confirmed capacity，未命中安全降级且不得污染 credential。
4. scoped health 需要 schema/projector cutover；本计划已将其改为必做 Task，而不是可选优化。旧 key-only snapshot 未退出 Planner 前，不得宣称作用域问题已解决。
5. committed 后把 capacity terminal 映射为 `server_error` 可能促使客户端重放已产生部分输出的请求；本地代理自身不重放，Decision Trace 必须标记 partial/committed，由客户端决定是否重试。
6. 真实 capacity 难以稳定制造；缺少真实 provider 授权或无法复现时只能阻止 release qualification，不能放宽为“测试通过即发布”。
7. 对非 capacity 的 possibly-accepted 5xx/transport 关闭透明 fallback 会降低局部可用性，但避免代理静默重复执行或重复计费；只有后续获得权威 idempotency contract 才能放宽，不能以“未向客户端输出”代替证明。
8. 本次 plan-only 审计执行 `node scripts/intelligent-routing-architecture.test.mjs` 时，当前工作区因 `src/features/routing/LocalRoutingSettingsEditor.tsx` 出现 `updateSettings` 而触发既有 owner 门禁；该文件不属于本计划文档改动，Task 0 必须先确认是用户并行改动还是架构合同需要同步，并在 Task 9 前以正确 owner 设计消除，禁止删除断言或用 suppression 掩盖。

本计划评审通过的判定不是“覆盖了所有 Sub2API 文案”，而是：新增或变化的响应能够经有界 evidence parser 和版本化 rule set进入同一个 CanonicalOutcome；已知情况得到精确副作用，未知/矛盾情况安全降级，且任何 consumer 都不重新解释原始 status/type/code/message。
