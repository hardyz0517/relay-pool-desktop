# Relay Pool Desktop 上游错误分类与重试升级实施计划

状态：In implementation；Task 0-2、HTTP/SSE 公共错误链和 scoped-health 基础 cutover 已部分落地，剩余状态以第 11 节实施台账为准，任何任务仍只以自己的 exit gate 关闭

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

本计划区分两个不能混淆的完成状态：

- **Engineering cutover complete**：Tasks 0-9、Task 10.1/10.2 的本地确定性门禁全部通过，旧生产 owner 已删除；该状态不依赖用户提供真实密钥。
- **Release qualified**：在 engineering cutover complete 基础上，Task 10.3 经用户明确授权的真实 provider/Codex smoke 也通过。缺少授权只能标记 `release qualification pending external authorization`，不能把工程实现说成失败，也不能把 release 说成已合格。

Engineering cutover 只有同时满足以下条件才完成：

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
- fixture、单元、集成、属性、并发、fault、架构、安全和构建门禁全部通过；真实 provider 门禁决定独立的 release-qualified 状态。

## 2. 冻结决策

### 2.1 Capacity 使用同目标有界重试

可信的 provider capacity 信号必须同时命中请求开始时冻结的 provider/gateway rule profile 及其 status/protocol guard，包括：

- HTTP `400` 且 message 命中版本化的 `Selected model is at capacity` 特征；
- HTTP `400` 且 message 命中经确认的 `You can retry your request`、OpenAI help URL 与 request-id 组合特征；
- native OpenAI 或经 conformance 证明语义等价的 profile 中，HTTP 或 SSE code 为 `server_is_overloaded`；
- 仅在 profile 明确声明它表示模型容量而非 rate limit/queue pressure 时，code 为 `slow_down`；
- provider profile 明确声明为模型容量的原生 `529` / `overloaded_error`；
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
| capacity retry 本地 backoff cap | 第一次 `250ms`，第二次 `1000ms`；仅用于上游未给合法 `Retry-After` 时 |
| jitter | deterministic equal jitter，范围为当前 delay cap 的 `1/2..=1`，种子来自内部 request/attempt identity，不接受客户端 seed |
| capacity retry 总等待预算 | `2000ms`，同时受现有 precommit deadline 限制 |
| capacity runtime cooldown | 最终耗尽后默认 `2000ms` |
| 跨 capacity domain fallback | 最多 1 次，且必须占用总 attempt 上限 |
| native OpenAI 经多个中转站的跨域 fallback | 默认禁用；中转站差异不能证明 OpenAI provider/model deployment 不同 |
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

1. 同目标由唯一 resolver 生成的版本化 `TargetExecutionCommitment` 定义，至少绑定 candidate/station/key identity、credential revision、endpoint revision、station-account revision、group-binding identity + revision、resolved upstream model/deployment、model-alias revision、协议/能力 profile，以及会改变 eligibility 的 enabled/专用 routing-policy revision 和不可变 upstream request body identity。`durable_revision = max(若干 revision)` 不能冒充 policy revision：它会因无关展示/设置变化产生假失效，也可能无法证明具体 policy owner。Execution 只持有并把完整 commitment 交回 resolver 比较，不自行拼接或漏选 revision。任一相关 revision/eligibility 变化必须释放 lease 并重新规划，不能拿旧 target 重试；重规划仍继承原逻辑请求已经消耗的 attempt、deadline、等待预算和 failure-domain exclusion，不能借 revision 变化重置预算或轮询同 capacity domain sibling Key。只改变显示名称等与执行无关的 revision 不得无故使 commitment 失效。
2. capacity rule 除分类置信度外必须显式给出 `RequestAcceptance` 与 `ReplaySafety`。只有 `Confirmed + RejectedBeforeAcceptance + ReplaySafe` 才允许非幂等 POST 做同目标或跨域 replay；“尚未向下游 commit”本身不能证明上游未接收。无法确认是否已接受的 5xx/transport、`Probable` capacity 或 conflicting evidence，除非请求方法本身幂等或存在权威 provider idempotency 保证，否则不得透明重放。
3. `Retry-After` 合法时替代该次本地 jitter backoff（不是与之相加，也不受单次 `250ms/1000ms` 本地 cap 限制），但仍必须同时不超过 capacity 剩余总等待预算与请求剩余 monotonic deadline；超过任一预算时不等待，直接返回可重试错误。非法、负数、溢出或过去的 HTTP-date 视为无该 header，使用本地 backoff。对同一逻辑请求累计实际 sleep，排队/admission 等待也计入 precommit deadline，但只实际 sleep 计入 `2000ms` capacity retry 总等待预算，避免同一等待被重复扣减。
4. 所有同目标 retry 必须经过 proxy-instance shared retry admission，不能让每个请求自行制造 retry storm。
5. 同一个 OpenAI provider/model capacity domain 内的不同中转站或 Key 默认视为相关故障，不因换 Key 重复采样和惩罚。
6. Custom OpenAI-compatible endpoint 只有在可信 provider identity 表明其不是同一个 OpenAI capacity domain 时，才可视为跨域候选；不能根据站点名称或错误 message 猜测。多个中转站最终都落到 native OpenAI 且没有权威 deployment/region 隔离证据时一律视为同域，因此默认只做同 target 有界重试并结束，不换 Key/站点。
7. retry delay 期间必须释放当前 attempt 的网络 stream、并发/capacity lease 和临时 buffer；delay 结束后只对相同 target identity 重新取得 lease 并再次比较 revision。不能占着并发槽位睡眠，也不能重新跑普通候选选择悄悄换 Key。
8. 每次 attempt 使用同一份不可变 request body backing storage，重新构建 Authorization、时间敏感 headers 和 outbound request；不得深拷贝大 body，也不得复用 one-shot body stream 或旧认证 header。
9. V1 参数必须由一个版本化 system retry profile 持有并进入 Decision Trace；首期不作为用户设置，不允许散落为 Execution magic number。
10. shared admission 队列满时立即返回可重试 `server_error`，不得绕过预算改为同域换 Key；domain cooldown 已激活时只在剩余 deadline 足够时等待，否则立即结束。队列公平性、取消和 shutdown 由 proxy runtime owner 统一负责。
11. 同一逻辑请求的客户端/内部 idempotency identity、session hash、`previous_response_id` 和 affinity evidence 在同目标 retry 中保持稳定；attempt correlation/ordinal 单独递增。不得为每次 retry 生成新的逻辑幂等键。
12. 跨 capacity domain fallback 还必须通过 request portability 检查。带 provider-bound `previous_response_id`、不可迁移会话状态或其他 target-bound continuation 的请求默认禁止跨域，即使存在不同 capacity domain。
13. “总 upstream attempt”只统计真正越过 outbound send 边界的发送；首次发送 + 最多 2 次同目标发送 + 最多 1 次跨域发送合计最多 4 次。这些是上限而非预留槽位，之前的其他 fallback 已消耗 attempt 时，capacity 只能使用剩余额度。lease/revision 检查或 admission 在发送前失败不增加 attempt ordinal，但也不得重置总预算、等待预算或 deadline。
14. capacity runtime cooldown 使用 `Closed -> Open -> HalfOpen` 状态机；默认 Open `2000ms`，到期后同 domain 最多给一个已经到达、通过正常 admission 的用户请求发放 probe permit，其余请求有界等待或快速失败。不得生成后台模型请求，HalfOpen 也不得通过轮询 sibling Key 扩大探测并发。
15. 跨域 fallback 是该逻辑请求最后一个可用 outbound attempt；无论它返回 capacity、其他可重试错误还是 transport failure，都不能突破总 attempt/deadline 再开启一条重试链。若在真正发送前因 revision/admission 失败而未消费 attempt，可带原预算重新规划，但仍受既有跨域次数与 exclusion 限制。

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
request_send_phase / idempotency_capability
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

`RequestAcceptance` 不能凭 HTTP 状态或下游 precommit 猜测，必须消费 transport owner 给出的发送阶段：`NotConnected | ConnectedNoHeaders | HeadersSent | BodyPartiallySent | BodyFullySent | ResponseStarted`。只有在未发送请求 body，或 provider 的版本化契约明确证明该错误发生在接受前时，才能得到 `RejectedBeforeAcceptance`；其余发送中断默认 `AcceptedOrMayHaveBeenAccepted/Unknown`。`ReplaySafety` 由端点操作语义、请求 body 可重放性、客户端幂等 identity 和 provider idempotency capability 共同决定，不能只按 GET/POST 或错误 code 推断。

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
- 内存准入按实际分配容量而非逻辑 payload 长度计费，覆盖原始 chunk、解压缓冲、decoder scratch、事件组装和待 flush 控制事件；所有扩容必须先取得增量 permit。实现可以通过固定上限 buffer 降低核算复杂度，但不得用 `Vec::len()` 低估 allocator 已保留容量。

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
| 2xx 但在端点契约允许的有界 JSON/SSE bootstrap 范围内确认是合法 error/`response.failed` envelope | 按 envelope 语义处理，不得算成功 | 按 typed signal | 对应 OpenAI error |
| 2xx 但 Content-Type/协议不符或 HTML body | 输出前停止或换 endpoint | Observe/uncertain endpoint | `server_error` |
| 3xx redirect | 不携带 Authorization 跟随；按 endpoint 配置错误处理 | Neutral/observe endpoint | `server_error` |
| 407 proxy authentication required | 停止当前 proxy route | Local/outbound proxy config，Station Key neutral | `server_error` |
| 408/425 | 仅在 replay safety 与 deadline 允许时 retry | Observe endpoint | `server_error` |
| 413 | 停止请求 | Request neutral | `invalid_request_error` |
| 本地下游已取消/断开 | 立即取消上游，不 retry | Downstream/request neutral | 不再主动写响应 |
| 上游返回非标准 HTTP 499，但本地下游仍连接 | 视为 upstream endpoint/transport failure；仅在 replay-safety gate 允许时 retry | Observe endpoint；不得伪装成本地取消 | `server_error` |
| 连接未建立且可证明未发送 request body | 在安全预算内 retry/fallback | Observe endpoint | `server_error` |
| 请求可能已被接受的 transport failure | 停止或标记 uncertain | Neutral/uncertain | `server_error`，不得暗示未执行 |
| committed 后 stream failure | 不 retry | 按语义作用域观察 | SSE terminal `server_error` |

未知响应绝不产生 credential hard-fail，也不触发 capability learning。

### 2.6 协议终止、资源与配置一致性

1. 每个端点冻结成功终态：Chat SSE 以 `[DONE]`，Responses SSE 以 `response.completed`（兼容 profile 可显式允许 `[DONE]`）结束；仅控制事件后 EOF 是 precommit incomplete，语义输出后无成功终态 EOF 是 postcommit incomplete，二者都不能伪装成功，后者也不能 retry。合法空 completed 仍是成功。
2. malformed UTF-8、非法 SSE field/JSON、超深/超复杂 JSON、解压错误和 event 超限生成闭合 protocol evidence。解析预算同时限制字节、嵌套深度、token/node 数、单字符串长度和 CPU work；深度必须在构建完整递归对象前限制，message matcher 必须是线性/有界算法，禁止灾难性回溯。
3. committed 后向客户端写 terminal 只允许 best-effort：若下游已经断开或写失败，只完成内部 attempt/request finalization，不再构造第二终态。postcommit 转发和待写队列必须有背压与硬上限，不能把 bootstrap 的有界性换成 committed 阶段无界排队。
4. capacity circuit 只由同一 `ProviderCapacityDomain + resolved model/deployment commitment` 的 Confirmed capacity observation 驱动。达到冻结阈值后 `Closed -> Open`，Open 到期单 probe 进入 HalfOpen；probe 获得非 capacity 的权威接受/成功证据才 Close，再次 capacity 则重新 Open。credential、请求错误、下游取消、解析器本地饱和和 unknown/conflicting 不得关闭、打开或延长 capacity circuit。阈值、窗口、probe 结果和 fake-clock 行为进入版本化 retry profile。
5. rule set、provider identity、retry、public mapping 和 trace profile 在每个逻辑请求开始时取得不可变快照；热更新只影响新请求。配置缺失、版本不匹配或快照组合不兼容时 fail closed，不允许同一请求的不同 attempt 混用 profile 版本。

## 3. 目标分类矩阵

| 语义 | 代表 code/type/message | Failure target | Retry disposition | Health/capability effect |
|---|---|---|---|---|
| Credential invalid | `INVALID_API_KEY`、`API_KEY_DISABLED`、`API_KEY_EXPIRED`、与可信 gateway profile 一致的 `authentication_error` | Station Key credential revision | 换 credential failure domain | Credential block；不按 15 分钟自动恢复 |
| Ambiguous authentication | 缺少可信 code/profile 的 401，或 status/type/code 冲突 | Uncertain/auth gateway | 默认 Stop；只有明确可移植且 replay-safe 时才换独立 gateway domain | Durable neutral；不得 block Key/account |
| Station account disabled | `USER_NOT_FOUND`、`USER_INACTIVE` | Station account | 换 station account domain | Account block，等待 account subject revision 或可信恢复证据 |
| Group/subscription invalid | `GROUP_DELETED`、`GROUP_DISABLED`、`GROUP_NOT_ALLOWED`、`SUBSCRIPTION_NOT_FOUND`、`SUBSCRIPTION_INVALID` | 独立 Station Group/Subscription subject（无法解析 subject 时退回 Station account uncertain） | 换已证明不共享该 group/subscription 的候选 | Group/subscription verdict；不污染 Key credential/account lifecycle |
| Balance depleted | `INSUFFICIENT_BALANCE`、上游 payment required | Station account 或明确 Key quota scope | 换独立余额域 | Depleted verdict；由余额事实或可信成功恢复 |
| Quota exhausted | `API_KEY_QUOTA_EXHAUSTED`、`insufficient_quota` | 仅由可信规则确认的 Key/account quota scope | 换独立 quota domain | 独立 quota dimension；按 reset/Retry-After 冷却，缺失时有界 half-open |
| Rate limited | `rate_limit_exceeded`、普通 429 | 仅由可信规则确认的 Key/account/provider scope；否则 runtime uncertain scope | Wait/replan 或换已证明独立的 rate-limit domain | 独立 rate-limit dimension；尊重 `Retry-After`；未知 scope 不写 durable block |
| Concurrency/queue | concurrency limit、pending queue | Runtime capacity scope | 短等待或换独立容量域 | Runtime-only cooldown，不写 credential failure |
| Provider capacity | provider profile/status/protocol guard 下的 capacity signature、`server_is_overloaded`、capacity 语义的 `slow_down`/529 | Provider/model capacity domain | `RetrySameTarget`，耗尽后仅跨已证明不同 domain；native OpenAI 多中转默认不跨域 | Runtime-only cooldown；Key/账号 durable health neutral |
| Model unsupported | `model_not_found`、`model_not_available` | Model on Key | 换支持该模型的候选 | Confirm unsupported model，仅在 applicability 允许时学习 |
| Protocol/endpoint unsupported | 405、501、可信 endpoint 404、`compact_not_supported` | Provider protocol/endpoint | 换支持该协议的候选 | Confirm unsupported protocol/endpoint |
| Request rejected | 普通 400/409/422、上下文过长、字段错误 | Request | Stop | Neutral |
| Request too large | 413、可信 body-too-large code | Request | Stop | Neutral |
| Safety/policy | `content_policy_violation`、cyber policy | Request/session | Stop | Neutral；不得降低 Key 健康 |
| Redirect/proxy route failure | 3xx、407、协议 URL 配置错误 | Station endpoint 或 local outbound proxy config | 不带凭据跟随；换独立 endpoint 或 Stop | Key credential neutral |
| Client cancelled | 本地下游 disconnect/cancel token | Downstream/request | Stop | Neutral |
| Upstream non-standard 499 | 上游 HTTP 499 且本地下游仍连接 | Station endpoint/transport | 仅在 replay-safety gate 允许时输出前 fallback | Observe endpoint；不得当成本地取消 |
| Relay service unavailable | auth/billing service overload、无账号池 | Station endpoint/account pool | 换独立 station domain | Observe/cooldown station service |
| Upstream server/transport | 5xx、connect、timeout、malformed | Station endpoint | 仅在独立 replay-safety gate 允许时输出前 fallback | Observe endpoint；达到阈值后 circuit；possibly-accepted 计费不确定 |
| Unknown | 动态或无法确认 | Uncertain | 按第 2.5 节 | Neutral unless typed endpoint evidence |

## 4. 目标代码边界

以下是职责边界而非强制文件拆分清单。优先沿用当前模块 owner；只有拆分能降低依赖或复杂度时才新增文件。Task 0 可以根据实际依赖记录实际路径，架构门禁验证“单一入口/依赖方向/零重复解释”，不以文件名或行数替代设计正确性。

| 路径 | 最终职责 |
|---|---|
| `src-tauri/src/services/proxy/adapters/error_envelope.rs` | 解析 HTTP/OpenAI/Responses envelope，生成 bounded typed evidence；不决定 health/retry |
| `src-tauri/src/services/proxy/adapters/error_rules.rs` | 版本化 provider/gateway rule set、confidence 与冲突解析；纯函数、无 I/O |
| `src-tauri/src/services/proxy/protocol/chat_sse.rs` | 以完整 SSE event 识别 Chat protocol progress/error evidence |
| `src-tauri/src/services/proxy/protocol/responses_sse.rs` | 以完整 SSE event 识别 Responses progress/error evidence |
| `src-tauri/src/application/request_finalization/failure.rs`（需要降低复杂度时才拆出 `classifier.rs`） | 闭合 failure target/class/effect/public source 类型，以及唯一 provider semantic signal -> CanonicalOutcome 入口 |
| `src-tauri/src/application/request_finalization/effect_planner.rs` | CanonicalOutcome 到 retry/health/capability/finalization effect plan 的无损转换 |
| `src-tauri/src/application/routing_engine/failure_domains.rs` | provider/account/endpoint/key/model/capacity failure-domain identity 与比较 |
| resolver/planning snapshot 的既有单一 target owner（Task 0 记录实际路径） | 唯一构造、验证 `TargetExecutionCommitment`；Execution 不复制 revision 字段清单 |
| `src-tauri/src/services/proxy/routing_runtime.rs` | proxy-instance shared retry/capacity admission、计数和 bounded cooldown overlay |
| proxy shared diagnostic-memory owner（由 `routing_runtime.rs` 组合） | HTTP/SSE/decoder 的保守上界准入、RAII 释放与饱和诊断；不解析错误语义 |
| `src-tauri/src/services/proxy/execution.rs` | 薄编排：执行 effect plan；不再按 HTTP status/code 二次分类 |
| `src-tauri/src/services/proxy/response_body.rs` | bootstrap/commit/terminal 生命周期与 committed 后错误规范化 |
| `src-tauri/src/services/proxy/public_error.rs` | Canonical public error source 到 OpenAI-compatible HTTP/SSE 输出 |
| `src-tauri/src/application/health_transitions.rs` | 只消费 scoped health effect；不从状态码和 message 推导语义 |
| `src-tauri/src/application/observation_ingestion.rs` | 原子写入 scoped typed observation，禁止复制分类逻辑 |
| `src-tauri/tests/fixtures/sub2api_errors/` | 明显假值、无 secret 的 HTTP/SSE envelope fixtures |
| `scripts/upstream-error-contract.test.mjs` | 跨模块分类、effect、public mapping golden contract |

扩展约束：首期不建立动态插件注册表或通用规则 DSL。新增 provider/gateway 只允许新增一个闭合 typed rule set 和 conformance fixtures；Transport、CanonicalOutcome、effect planner、health 和 public adapter 不应因新增供应商 code 而增加并行 switch。所有 rule set、provider identity、model alias、retry profile 和 public mapping profile 都携带版本并进入 trace/replay。Rule set 由可信 target metadata 静态选择，body 只能提供 evidence，不能选择自己的解释器。

Rule registry 必须在构建/测试时拒绝同一 profile 内 guard 重叠但优先级未显式声明的规则、重复 normalized code/type、不可达规则和同等证据映射到不兼容 durable effect 的歧义。规则 precedence 使用闭合枚举/整数并由 golden test 锁定；不能依赖源文件顺序。新增 gateway 的 conformance gate 至少证明：已有 fixture 结果不变、未知输入仍保守、consumer 文件零改动。

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
6. 错误 body 使用有界 streaming read；`256KiB` 上限作用于解压后的实际字节。生产实现必须在分配/扩容前，从 HTTP/SSE 共用的 proxy `32MiB` memory admission 预留该阶段所有 owned buffer 的 `capacity` 加解析器/解压器 scratch 的**保守上界**，而不是事后按 `len` 记账。无法从第三方库观测的内部 allocation 必须通过固定容量 buffer、受限 streaming visitor，或在调用前按其文档上界预留；不能声称对不可观测 allocation 做了精确核算。超过单响应/共享上限后立即停止解析、关闭/丢弃该 response，并只生成 `error_body_too_large` 或 `diagnostic_memory_saturated` evidence；截断 JSON/message 不得形成 durable effect。禁止继续使用无上限 `response.bytes()`，也不得让压缩传输绕过限制、为连接复用无界 drain 或在 permit 外排队缓冲。
7. JSON parser 在构造完整递归对象前执行深度、token/node、单字符串和总 allocation 限制；优先使用 bounded streaming visitor 提取允许字段，禁止先构造任意 `serde_json::Value` 再检查复杂度。message signature 只扫描 UTF-8 安全截断后的 `16KiB`，使用线性/有界 matcher；截断或预算耗尽事实进入 evidence flag，不能命中 durable hard effect。
8. 对合法 2xx error envelope、错误 Content-Type、HTML/Cloudflare body、3xx、407、413 和上游 499 生成闭合 typed evidence；本地下游取消由 cancellation signal 单独建模，不通过上游 envelope parser 推断。
9. HTTP-date `Retry-After` 在接收时转换为 duration；等待和 deadline 只使用 monotonic time，wall-clock 后跳不能延长在途请求。

REFACTOR：删除旧 `openai_error_code` 的单字段捷径，所有 provider adapter 使用共同 envelope parser。

Focused gate：parser table tests、chunk/decompression bomb、深度/token/string/allocation/CPU-budget fuzz/property test、secret canary/redaction test。

Task exit gate：HTTP/SSE 共用 parser 在生产 composition 中只有一个入口；所有 buffer/scratch 在分配前取得 shared permit且 RAII 守恒；rule-registry 歧义/不可达检查、第二 gateway conformance 和上述 focused gate 全绿；旧单字段/message extractor 零生产引用。

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
10. classifier/rule-set/profile 缺失、版本不匹配、内部异常或 invariant violation 一律生成闭合的 `UncertainInternalFailure` outcome：durable health/capability neutral、默认不透明 retry（除非 replay-safety owner 独立证明安全且预算允许）、public `server_error`、稳定诊断码；禁止 panic 穿透请求任务，也禁止回退旧分类器。
11. `FailureTarget` 只回答“哪个对象受到影响”，还必须有闭合的 `FailureDimension` 回答“对象的哪一条独立约束受到影响”，至少区分 credential、account lifecycle、group/subscription、balance、quota、rate limit、endpoint availability、protocol capability 和 model capability。两个轴共同决定 effect identity；禁止用最后一次 `evidence_code` 覆盖同一 subject 上仍然有效的另一类 verdict。
12. canonical outcome 到 lifecycle DTO 必须做编译期穷尽、无损转换，保留 target、dimension、class、retry intent、health、capability、confidence、acceptance、replay safety、billing、稳定 evidence code 和所有 profile version。任何字段不得先压成 public HTTP/status 或旧的 `TryNextCandidate/HardFail` 再由 consumer 反推；新增 enum variant 若未更新 converter、persistence/public/trace contract，应编译失败或 architecture gate 失败。
13. `StationGroup/Subscription` 必须是 `FailureTarget` 的真实闭合 variant，并有 rule producer、effect-planner consumer、typed subject constructor、持久化映射和 Planner test。仅在 SQL 中预留 `station_group` 字符串、却让 canonical producer 继续输出 `StationAccount`，视为未完成；规则无法可靠解析 group subject 时必须产生 uncertain/account-level neutral outcome，禁止猜测 group id。

REFACTOR：删除 classifier 外根据 status/code/message 修改 canonical effect 的入口。

Focused gate：`node scripts/upstream-error-contract.test.mjs`、effect planner、outcome invariant 和 property tests。

Task exit gate：分类矩阵逐行 golden 全绿；每个 target/dimension/retry variant 都有 producer、consumer、trace/public/persistence 合同或被删除；StationGroup target 已端到端接通；任一 consumer 无法从 public status/message 反推 effect，architecture gate 对新增未消费 variant fail closed。

### Task 3：Failure domain 与同目标 capacity retry runtime

RED：

- capacity 后第二 attempt 选中了另一 Key；
- 多个并发 capacity 请求各自重试，突破 proxy-instance shared budget；
- endpoint/credential revision 变化后仍重放旧 target；
- 同一 failure domain 被多个 Key 重复计为独立失败。

GREEN：

1. 在现有 failure-domain owner 中增加 `ProviderCapacityDomain`。domain equality 只使用可信的 provider family、alias 解析后的实际 upstream model/deployment family，以及存在权威证据时的 region/deployment identity；Station ID、Key ID、普通中转 endpoint revision、provider identity profile version 和 model alias revision 只能作为 provenance/revision fence，revision 数值本身不能把同一 OpenAI capacity domain 人为拆成多个域。不得包含完整 URL、Key 或账号 secret。Custom/unknown provider 缺少可信 identity 时默认“不能证明跨域”，而不是默认每个站点独立。
2. `RetrySameTarget` 复用同一个 `TargetExecutionCommitment`，但每次重试先释放旧 attempt 资源，等待后重新取得同 target constraint lease，并由 resolver 使用一次新的权威只读快照重新验证完整 commitment 与当前 eligibility；首次冻结的 target map 只能提供 expected commitment，禁止同时充当 current state，否则禁用、换组、换凭据等并发变更永远不可见。Execution 不维护一份较弱的 revision 清单。
3. 在唯一 proxy runtime state 中加入 `CapacityRetryProfileV1`、shared admission、`Closed/Open/HalfOpen` domain cooldown、error-body/SSE parser 共用 memory admission 和 monotonic deadline。
4. 实现第 2.1 节的 attempt、delay、jitter、deadline、single-probe HalfOpen 和 cross-domain 限制；总 attempt 必须复用 Coordinator 的 request-local budget，不能另建一套 capacity 计数后与普通 fallback 叠加突破上限。
5. 同域 sibling Key 不进入 capacity fallback；不同域候选必须由 planner/failure-domain projector证明，而不是 Execution 手写筛选。
6. trace 记录 attempt ordinal、retry kind、domain commitment、suppression reason、provider identity profile version 和 model alias revision，不记录完整 domain 原料。domain commitment 使用版本化 canonical encoding 对非 secret identity 计算固定长度 digest，并随 outcome 保存，历史 replay 不用最新版 alias/profile 重算；alias revision 变化只触发 fence/replan，只有解析后的实际 upstream model/deployment identity 改变才可能改变 domain equality。
7. request body 使用共享 immutable bytes/lease；每次 attempt 重新构建 outbound request 和认证 header，不复制完整 payload。
8. domain/global admission 使用第 2.1 节冻结的 in-flight/waiter 上限、FIFO、公平取消和 shutdown 行为；admission 被拒绝时不得回退到无预算 retry。commitment/revision/eligibility fence 在发送前失效时释放 permit并携带既有预算和 domain exclusion 重规划，不能重置逻辑请求。
9. cross-domain admission 先验证 request portability；`previous_response_id`、target-bound affinity 或 provider-specific continuation 不可迁移时记录 `capacity_cross_domain_not_portable` 并返回 `server_error`。
10. transport owner 为每次 attempt 记录不可回退的 request-send phase；retry gate 使用该 phase、端点操作语义和版本化 idempotency capability，处理 connect/TLS 失败、headers 已发、body 部分发送、body 全发送后断开等边界。classifier 的 `ReplaySafety` 仅表达 provider 错误语义，不能单独授权重放；最终 gate 必须计算 `canonical intent × request-send phase × operation semantics × body replayability × authoritative provider idempotency capability`。客户端提供的幂等键只在 provider 契约允许时原样保持；代理不得擅自生成会改变上游语义的幂等键。
11. capacity circuit 的 key、开启阈值、观察窗口、HalfOpen 单 probe、成功/非 capacity/unknown/cancel 各类结果转换按第 2.6 节实现；本地 diagnostic-memory/retry-admission 饱和不得反馈成 provider capacity observation。
12. `RequestSendPhase` 由实际 outbound body/transport wrapper 单调推进；若当前 HTTP client 无法可靠暴露某一边界，该边界必须保守提升为 `BodyPartiallySent` 或 `Unknown` 并禁止非幂等透明 replay，不能用推测补齐。对于 reqwest 一类只暴露 `send().await` 结果、无法证明 header/body 写入边界的 client：只有权威 connect/TLS-before-write 证据可记为 `NotConnected`，其余 send error 一律为 `Unknown`，取得 response headers 后为 `ResponseStarted`。focused test 必须使用可控 fake transport 逐边界注入并驱动生产 Execution retry 路径；只测 phase 枚举或纯 gate 函数不算通过。

REFACTOR：Execution 不再自己维护“同 Key重试计数”和“全局 retry budget”。

Focused gate：deterministic retry、parallel admission、deadline、revision fence、shutdown/cancel、request-send-phase/replay boundary、same-domain suppression、cross-domain fallback、capacity circuit transition tests。

Task exit gate：完整 `TargetExecutionCommitment` 由 resolver 单点构造/复验；真实 transport wrapper 单调报告 send phase；request-local budget/exclusion、FIFO admission、HalfOpen 单探针和跨域终局均进入 production composition；fake-clock 与 100 并发测试证明 attempt/waiter/deadline/lease 上限，Execution 无私有 capacity 计数或弱 revision fence。

### Task 4：HTTP 错误生产切换

RED：为目标分类矩阵的 HTTP 行建立从 fake upstream 到 attempt terminal 的端到端测试。

GREEN：

1. `UpstreamAttemptExecutor` 对非 2xx 以及 2xx semantic error body 先解析 evidence，再调用唯一 classifier；不能把“HTTP 成功”直接等同于协议成功。2xx JSON 非流响应只能在端点契约和 Content-Type 表明可解析 JSON 时使用有界前缀/完整有界响应识别错误；大型成功响应不得为了探测 error envelope 被整包缓存，成功转发也不得受 `256KiB` 错误诊断上限截断。流响应统一交给 Task 5 bootstrap，不在 HTTP 分支抢读。
2. Execution 直接执行 canonical retry disposition，不经过 public HTTP status。
3. HTTP 400 capacity 进入同目标 retry；普通 400 保持 StopRequest。
4. 401 按 code 分成 Key credential、Station account 和 unknown auth；只有可信 credential code 才产生 credential block。
5. 403 按 balance/group/subscription/policy/auth 分流；禁止仅凭 403 hard-fail Key。
6. 429 按 quota/rate/concurrency/auth-rate-limit 分流；尊重 scope 和 Retry-After。
7. 404/405/501 只有可信 capability evidence 才学习能力；未知 404 不污染 capability。
8. 3xx 不自动携带 Authorization 跟随；407 归因 local/outbound proxy config；所有 408/425、5xx、timeout、malformed、capacity retry/fallback 都受统一 replay-safety gate 限制；413 不 retry。本地下游取消与上游非标准 499 使用不同信号源：前者立即停止，后者按 endpoint/transport failure 处理，禁止仅凭数值 499 伪造客户端取消。
9. 非成功 attempt 的 acceptance、replay-safety 与 billing state 一起 finalization；不得因“HTTP/SSE 尚未向下游 commit”推导 NotBillable。只有 rule/provider contract 提供权威证据时 capacity rejection 才记录 NotBillable，否则与 ambiguous 5xx/transport 一样记录 BillingUncertain。
10. 对 `204/205`、空 body、Content-Length/Transfer-Encoding 冲突、提前 EOF、压缩/解压失败按端点契约分类；需要响应对象的 Chat/Responses/Embeddings 不能把空 2xx 当成功，`/v1/models` 等端点也必须由自己的成功 schema 决定。连接复用和 drain 不能绕过诊断内存、deadline 或 cancel。

REFACTOR：删除 `RetryPolicy::decide` status switch、Execution `health_effect`、`attempt_failure_kind` 和 `ProxyFailure` public-status 反推 effect。

Focused gate：HTTP integration matrix、max attempts、effect persistence atomicity、request outcome tests。

Task exit gate：四个公开端点的非 2xx、允许的 2xx semantic error、空/超大/压缩/提前 EOF 与 transport phase 矩阵全绿；HTTP 路径只执行 canonical effect；所有旧 status retry/health switch 零生产引用；大型成功响应保持流式透传。

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
11. EOF 按第 2.6 节的协议终态表处理；malformed UTF-8/field/JSON 不得因位于 chunk 边界而改变分类。committed 后客户端写失败只做内部终结，error terminal 为 best-effort 且不得形成第二终态。
12. committed 阶段的 decoder scratch、单 event 和下游发送队列继续受 shared memory/backpressure 上限约束；慢客户端不能导致无界累计，取消必须传播到 upstream body task。

REFACTOR：`response_body.rs` 不再把所有显式 failed terminal 压成无语义 `UpstreamStreamFailed`。

Focused gate：Chat/Responses SSE state-machine、partial chunks、UTF-8/EOF/success-terminal matrix、precommit retry、postcommit no-retry、slow-consumer backpressure、RAII lease、downstream drop tests。

Task exit gate：Chat/Responses 的 HTTP/SSE 等价错误得到同一 canonical outcome；shared diagnostic-memory、event/bootstrap/发送队列上限和取消释放已在生产接线；任意 chunking 结果不变；precommit 只在 replay-safe 时重试，postcommit 在所有故障下注定 no-retry/no-double-terminal。

### Task 6：Scoped health、capability 与恢复语义

RED：证明 credential/account/endpoint/model/capacity 五种 effect 当前都落到 Station Key 或未被投影；并证明同一 account 上的 disabled、balance、quota/rate-limit verdict 会互相覆盖或被错误 success 清除。

GREEN：

1. CanonicalOutcome、attempt terminal、scoped Observation、关键 health transition 与 request/attempt 计费轴由唯一 lifecycle transaction owner 提交；网络响应成功/失败不能依赖数据库提交后才开始向客户端流式输出，因此 precommit 持久化失败必须 fail closed，committed 后持久化失败不得透明 retry、不得注入第二终态。能同一 SQLite transaction 完成的 durable effect 必须原子完成；跨越网络 commit window 的部分必须写入同一持久化域中的 durable bounded outbox/checkpoint 并由同一 owner 幂等重放。磁盘不可写且无法建立 durable 补偿时只能记录有界进程诊断并进入 degraded/fail-closed 状态，不能把内存队列宣称为 crash-safe；outbox/诊断缓冲都必须有硬上限、shutdown 策略和磁盘失败测试。
2. scope 规则：
   - credential invalid -> Station Key credential revision；
   - user lifecycle -> Station account scope；group/subscription -> 独立 Station Group/Subscription scope；无法可靠解析 group subject 时 durable neutral；
   - 5xx/transport -> endpoint revision；
   - model not found -> model-on-key capability；
   - capacity/concurrency -> runtime failure domain overlay，不写 durable credential failure。
   scope 只标识 subject，不能兼任 verdict reason。每条 durable effect 还必须携带闭合 `FailureDimension`；持久化/projector identity 至少是 `(typed subject scope, dimension)`，同一 subject 的 account-disabled、balance、quota、rate-limit、endpoint-availability 等约束可并存，Planner 取所有适用 dimension 的最严格 admission 组合，不采用“最后一条观察覆盖整个 subject”。
3. credential block 与 credential revision 绑定；凭据更新后旧 block 自动失效。未经可信成功或 revision 变化，不靠固定 15 分钟假装恢复。
4. account/group verdict 与 account/group subject revision 绑定，只由对应账号/分组事实 revision 变化或可信的同 scope、同 dimension 恢复 evidence 解除；endpoint revision、另一 Key 的无关 success，以及同 subject 另一 dimension 的恢复都不能清除。
5. quota/rate cooldown 使用 Retry-After/reset evidence；缺失时进入有界 half-open，而不是永久 block。cooldown 到期后的 probe admission、单飞和结果归并由唯一 runtime state-machine owner 完成；durable projector 不因读取时间经过而自行删 row，多个进程内 consumer 也不得各自放行 probe。
6. capability effect 真正写入唯一 capability owner，并由下一 PlanningSnapshot 排除不兼容候选。`model_not_found` 不得同时在 scoped health verdict 中写一份 `model_on_key blocked`；若未来存在与“能力不支持”正交的模型级临时 availability，必须使用独立 health dimension 并证明不会复制 capability truth。
7. 迟到/重复 Observation 按逻辑 request identity + attempt ordinal + terminal kind + effect owner/dimension 的稳定幂等键、单调 ordering identity 和 revision 处理；duplicate terminal、进程恢复重放与 committed 后补偿不得重复计费、重复 health/capability effect 或让旧 success 清除新 failure。同一个 attempt terminal 可以原子地产生 health 与 capability 两类不同 owner 的 effect，但各 owner 的 identity 必须确定且碰撞 fail closed。
8. 当前生产 `routing_health_snapshot` 只有 Station Key 维度，Task 6 必须使用下一可用 append-only migration 建立 scoped durable verdict，而不能把多作用域继续留给测试投影。目标表 `routing_health_verdicts` 至少包含：
   - 由 typed constructor 生成的 canonical `scope` 与闭合 `scope_kind`；
   - `station_id`、可选 `station_key_id`、可选 `model`；
   - endpoint/credential/account/group/model-alias subject revision fence；
   - 闭合 `failure_dimension`，且唯一键/重建 partition 包含 `(generation, scope, dimension)`；
   - `degraded | cooldown | blocked` verdict；无 durable 抑制统一以该 dimension 无 row 表达；
   - 可选 `cooldown_until_ms`、稳定 `evidence_code`、source observation identity/order；
   - projector version、updated time，以及按 scope shape 的 SQL CHECK/unique constraint。
9. `PlanningSnapshotBuilder` 在同一 durable read transaction 中批量读取 station-key credential、station-account、endpoint 和 model/capability verdict；禁止 N+1、字符串反解析和页面 DTO 回流。
10. `routing_health_verdicts` 由 versioned projector 从 immutable Observation 幂等重建；capacity/concurrency runtime-only effect 不落该表。旧 `routing_health_snapshot` 在原子 cutover 后停止作为 Planner 权威输入，观察期只读兼容和最终删除进入 ledger。
11. migration 必须有 postcondition、schema15->latest fixture、current-schema upgrade、projector rebuild、reset/reimport 和 rollback rehearsal；实施时先枚举迁移目录并选择唯一下一可用版本，CI 拒绝重复版本/缺号意外/修改已发布 migration。当前工作区若存在并行 migration，必须重新取号并同步 schema fixture/catalog，不得按计划文本硬编码序号。
12. `routing_health_axes` 继续只保存 availability/latency/reliability/freshness 数值轴；`routing_health_verdicts` 只保存 admission/block/cooldown 判定。两者不得复制同一字段或互相反向解析，均从同一 Observation 源按各自 projector version 派生。
13. `scope_kind` 与合法字段 shape 必须冻结成闭合 typed contract；canonical scope 只允许由 typed constructor 生成并采用无歧义规范编码，禁止 `split(':')` 反解析。若 group 是独立 failure domain，必须使用独立 `station_group` scope，不能把可选 group revision 混入 station-account identity。model-on-key scope 必须绑定 resolved upstream model/deployment commitment、credential revision 与 model-alias revision，不能只绑定客户端 alias 字符串。
14. Observation 的 projector 全序固定为数据库分配、严格单调且不依赖 wall clock 的 durable ingestion sequence（observation id 仅作稳定 tie-break/审计）；`producer_id + producer_sequence + payload_hash` 只用于来源幂等与碰撞检测，provider `event_at` 与调用方提供的时间不参与覆盖顺序。同 identity 不同 payload/effect 必须 fail closed；迟到旧 success 即使后来重放也不能清除新 failure。若 SQLite writer 序列化是该保证的前提，必须以事务和并发测试锁定，不能只靠毫秒时间戳宣称全序。
15. 无 durable 抑制统一用“该 scope + dimension 无 row”表达；恢复 Observation 必须明确指向同 scope、同 dimension，并只删除该维 verdict。普通请求成功不得解除 account disabled、余额/配额、group/subscription 或 model unsupported。cooldown 到期只能进入 runtime HalfOpen admission，不能直接等价为 durable healthy。
16. append Observation、apply verdict、推进 projector checkpoint 和 attempt terminal 的原子性边界必须由唯一 lifecycle transaction owner 定义；同一 SQLite 域内必须单事务原子提交。若不能同事务完成，使用 transactional durable outbox/checkpoint 提供至少一次投递，并靠稳定 effect identity、payload-hash 碰撞检测和幂等 projector 达到 externally observable effectively-once；不得声称网络/崩溃边界上的物理 exactly-once。禁止 terminal 已提交但 outbox/effect 丢失，也禁止 checkpoint 未推进导致同一 effect 被重复计费或产生不同 verdict。
17. Planner 的“批量读取”门禁必须证明真实 SQL statement count，而不是在一个 transaction 内循环 N 次；同时冻结 SQLite bind 上限、候选上限、去重与空输入行为，并用 query-plan/index 证据证明不会退化成全表/N+1 扫描。组合器必须对同一 candidate 的多 scope、多 dimension verdict 使用显式优先级/meet 规则，并输出所有阻断 reason；结果不得依赖 SQL 返回顺序。
18. `model_not_found` 的 durable unsupported truth 只进入唯一 capability evidence/projector；Planner 直接消费该 owner 的投影。`routing_health_verdicts.model_on_key` 不得承载 unsupported verdict；若当前 schema 已预留该 scope，只能保持无 producer，直到有经规范批准的、与 capability 正交的模型级 availability dimension。capability owner 与 health owner 各自单向消费 immutable outcome 并分别证明 rebuild parity，禁止互相反推。
19. rebuild/reset/reimport 采用 shadow generation + observation watermark + row/hash 对账 + 单事务 active-generation cutover；schema 必须显式提供 `generation_id` 或等价的 active-generation 元数据，不能只在流程文档中假设存在。失败时旧 generation 保持可读。禁止 `DELETE + replay` 暴露空窗，因为 Planner 可能把暂时缺行误判为健康。
20. scope/evidence/projector/observation 字段必须有相同的 Rust/SQL 字符集与长度上限；完整 URL、secret、动态 request id 和自由 message 不得进入 scope/唯一索引/metric label。Station/Key/model alias 删除或 revision 变化后，旧 row 仅作审计并由 retention/GC 策略处理，GC 不得破坏 immutable outcome replay。

REFACTOR：停止把所有 health writeback 映射到 `routing_health_snapshot.station_key_id`；旧表如需观察期只能只读，删除进入独立后续 migration。

Focused gate：scope/dimension/SQL-shape matrix、capability-single-owner/no-health-duplicate、coexisting-verdict/independent-recovery、order-independent strictest-admission、single-transaction batch read（statement-count/query-plan/bind-limit）、revision recovery、scope-specific recovery、durable-sequence concurrent writers、late event、duplicate/collision event、outbox crash/replay effectively-once、cooldown/HalfOpen/restart、projector crash-point、shadow rebuild parity/cutover、orphan retention、migration/postcondition/schema15/current-schema/reset-reimport/rollback tests。

Task exit gate：canonical lifecycle effect 的每个 durable target/dimension 都经唯一 transaction/outbox owner 落地；Planner 在同一 snapshot transaction 批量消费 scoped health 与 capability；旧 key-only snapshot 不再是权威输入；migration/postcondition/rebuild/crash/reimport 全矩阵通过，group 与 capability 两条端到端恢复测试通过。

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
9. 下游 header 采用显式 allowlist；剥离上游 `Set-Cookie`、认证挑战、hop-by-hop、内部 request id/拓扑和未经验证的 retry metadata。committed 后 terminal 写入失败不再改写 HTTP 状态、不重试且不生成第二终态。

Focused gate：HTTP/SSE golden snapshots、Codex retry-code contract、redaction、header allowlist tests。

Task exit gate：四端点 HTTP 与两类 SSE 的 status/type/code/message/header profile golden 全绿；本地认证和上游认证不可混淆；public adapter 无 retry/health 依赖；focused Codex contract 关闭工程门禁，真实 Codex smoke 只关闭 release qualification。

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
9. `DecisionTraceProfileV1` 冻结：每请求最多 `4` 个 outbound attempt、`64` 个 trace event、单字段最多 `512` UTF-8 bytes、序列化后最多 `32KiB`；进程内 ring 最多 `512` 个 request trace 且总 retained bytes 最多 `16MiB`，任一上限先到即淘汰最旧完整 request trace。达到请求内上限时只追加一次 `trace_truncated=true` 后停止追加；若连该标志也无法容纳，则在固定 envelope 位设置标志。指标仅使用编译期闭合 label 枚举；未知 code/message 只能计入 `unknown` bucket。这些值由单一 profile 持有并以边界测试锁定，不得在 DTO/store/Execution 散落常量。

Focused gate：observability contract、trace replay、bounded metric buffer、redaction tests。

Task exit gate：V1 所有硬上限、淘汰与 `trace_truncated` 行为由边界测试锁定；attempt/acceptance/replay/billing/profile version 可追溯；metric labels 编译期闭合；raw message、URL、secret、request id 不进入持久化/IPC/label；所有 fail-closed/saturation 都有稳定 reason code。

额外接线门禁：仅新增 trace/metric struct、常量或单元测试不算完成；生产 Execution、canonical finalization、持久化 read model 与 IPC 查询必须经过同一 composition root 实际写入和读取这些字段。架构测试应分别证明 producer 与 consumer 均存在，并拒绝“定义存在但零生产调用”的 dead contract。

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
- 已完成 RED 阶段后仍以 `*-red.test.*` 命名、但已成为长期 GREEN contract 的脚本；保留历史 RED 证据在 audit，长期门禁改用中性名称并同步 package/verify 引用。

架构门禁必须拒绝：

- 在 classifier 之外匹配 `server_is_overloaded`、`slow_down` 或 capacity message；
- 在 Execution/Health/Public adapter 中按 HTTP status 重新推导 canonical effect；
- 同一 capacity failure domain 逐 Key fallback；
- committed 后 transparent retry；
- 任意自由 message 进入 durable scope、metric label 或 capability key；
- production/test 使用不同 failure-domain 或 retry contract。
- provider 新 code 直接在 Execution/Health/Public adapter 增加 switch，而不是进入版本化 rule set。
- 无界 error body 读取、未持有 shared memory permit 的 SSE bootstrap 或 retry sleep 持有 capacity lease。
- 把上游 HTTP 499 当成本地下游 cancel，或让 2xx 错误探测整包缓存/截断正常成功响应。
- 在 committed 输出后因 lifecycle/数据库失败透明 retry、重复写 terminal，或以无上限内存队列弥补持久化失败。
- 按 payload `len` 而非实际 allocation 核算诊断内存，或只在 JSON 完整递归构造后检查深度/复杂度。
- 把下游 precommit、HTTP method 或错误 code 直接当作上游未接受证明，绕过 request-send-phase/idempotency gate。
- 把 diagnostic-memory/retry-admission 饱和、unknown/conflicting 或下游取消写入 provider capacity circuit，或让同一请求混用不同 profile snapshot。
- 把 control-only EOF、缺少成功终态的 committed EOF 当作成功，或在 committed 阶段建立无界下游发送队列。

Exit gate：旧 owner 零生产引用、manifest/ledger 清零、production composition 只接新链、生成物无漂移。

### Task 10：Qualification 与真实 provider 门禁

#### 10.1 必跑自动化

在仓库根目录使用 PowerShell：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_failure_contract
node scripts/upstream-error-contract.test.mjs
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
- duplicate/late terminal 与 outbox 重放不会产生两次可观察 health/capability/计费 effect；相同 identity 不同 payload 必须 fail closed；
- 同一 subject 的 disabled、balance、quota/rate-limit 等 dimension 可并存；恢复其中一个不会清除其他 verdict，Planner 组合结果与 SQL/Observation 顺序无关；
- 单候选池不会因普通 runtime suppression 形成永久无候选，但 credential hard block 不得被 max-ejection guard 放开；
- restart 后旧 runtime capacity cooldown/permit 不回写新 runtime instance。
- 最大尺寸 error body、最大 bootstrap buffer、最大 attempt 数和 deadline 组合下执行 bounded-memory soak；request body retry 只能共享 backing storage，内存不得按 attempt 倍增。
- domain/global retry admission 的 FIFO、公平性、waiter 上限、队列满快速失败、cancel 和 shutdown 均有确定性测试。
- `Closed/Open/HalfOpen` domain cooldown 在并发和 fake clock 下最多放行一个 probe，进程 restart 不继承旧 permit。
- `Probable` capacity、possibly-accepted 5xx/transport 和 precommit SSE 若没有独立 replay-safe 证据，均不会透明重放；权威 provider idempotency 能力存在时才按同一 budget 放行。
- 2xx 大型成功 JSON/stream 能持续透传且不受错误诊断 `256KiB` 上限截断；2xx error envelope 仍在有界 bootstrap 内识别。
- 本地下游取消与上游 499 分别注入，前者取消上游且不写响应，后者保持 endpoint attribution 并经过 replay-safety gate。
- lifecycle store 在 precommit/committed 两阶段分别故障时，前者 fail closed，后者不 retry/不双终态；有界补偿重放保持 attempt、计费和 health effect 幂等。
- request send phase 在 connect/TLS、headers、partial body、full body 和 response-started 注入点单调推进；只有 replay/idempotency 契约允许的边界会透明 retry。
- JSON 深度/token/string/allocation 与压缩炸弹在构造完整对象前有界失败；memory permit 按 owned allocation capacity 与不可观测 scratch 的保守上界守恒，不按 payload `len` 乐观核算。
- control-only EOF、semantic 后缺成功终态 EOF、malformed UTF-8 与慢客户端背压均有确定性结果；committed 后写 terminal 失败不产生第二终态。
- capacity circuit 仅响应匹配 domain/model commitment 的 Confirmed capacity；成功 probe、再次 capacity、非 capacity、unknown、cancel 和本地 admission 饱和的状态转换全部由 fake clock 并发测试锁定。
- 请求执行期间切换 rule/provider/retry/public profile，只影响新请求；在途 attempts 保持同一不可变 profile snapshot。

#### 10.3 真实 provider smoke

必须由用户明确提供测试授权和假业务请求后执行，测试输出不得保存真实 Key 或完整 response。至少包括：

1. 一个能够稳定注入/模拟 HTTP 400 capacity 的 OpenAI-compatible upstream；
2. 一个首事件为 `server_is_overloaded`/`slow_down` 的 SSE upstream；
3. 一个真实 Sub2API 测试站点的普通成功、401、429 和 5xx 路径；
4. Codex 客户端确认最终 `server_error` 会进入其重试路径；
5. 证明 capacity 期间 selected station/key identity 未切换，且 Key 未写 credential failure。

未经授权或无法稳定制造真实 capacity 时，engineering cutover 可以在 Tasks 0-9、10.1/10.2 全绿后关闭，但 release qualification 必须标记 `pending external authorization/evidence`，不能用 fixture 冒充真实 provider 证据。

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
| 本地下游 cancel/disconnect | 立即停止、释放上游资源，不 retry、不主动写第二响应 |
| 上游 HTTP 499 且下游仍连接 | 按 endpoint/transport failure 分类；仅在 replay-safe 时输出前 fallback，不污染 request cancel 事实 |
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
| 未知 5xx | 仅在 replay-safety gate 允许时输出前跨 endpoint failure domain；否则停止并标记 possibly-accepted；Observe endpoint |
| Retry-After 超过 deadline | 不阻塞超预算，返回 retryable error |
| Retry-After 未超过 deadline 但超过 capacity 总等待预算 | 不阻塞超预算，返回 retryable error |
| target commitment 中任一执行相关 revision/eligibility 在 retry 前变化 | 释放旧 lease并携带原预算与 domain exclusion 重规划，不发送旧凭据/旧模型/已禁用目标，不按 revision 拆 capacity domain |
| capacity retry backoff | 不持有旧 capacity lease、stream 或 bootstrap memory；重试时重新取得同 target lease |
| capacity rejected attempt cost | acceptance 与 billing 分轴；无权威计费证据时为 BillingUncertain，不污染成功 attempt usage/cost |
| domain/global retry waiter 队列已满 | 立即返回 503 `server_error`；不换同域 Key、不突破 attempt/wait budget |
| capacity cooldown 到期的并发请求 | HalfOpen 只放行一个同域 probe，其余有界等待或快速失败 |
| error body/SSE parser 总保留量到达 32MiB | 新 admission 立即有界失败并释放资源；没有第二层 waiter/buffer，进程内存不随并发无界增长 |
| 进程重启 | 旧 runtime permit/cooldown 不跨 instance 回写 |
| 日志/trace/metric | 无完整 Key、URL query、动态高基数 message 或原始认证 body |
| request body 部分发送后 transport failure | 未获 provider idempotency 保证时不透明 replay；acceptance/billing 保持 uncertain |
| control-only SSE 后 EOF | precommit incomplete；不记成功，按 replay-safety gate 决定是否可 retry |
| semantic SSE 后无成功终态 EOF | postcommit incomplete；不 retry、不产生第二 HTTP 响应 |
| JSON/decompression/parser 预算耗尽 | 有界 protocol/diagnostic failure；不产生 credential/account/capability durable effect |
| capacity circuit 收到本地内存/准入饱和或下游 cancel | circuit 不变；只记录对应本地稳定诊断码 |
| profile 在请求执行中更新 | 在途请求保持启动时快照；新请求使用新版本；trace 可区分版本 |
| 同一账号先 disabled、后 balance/rate-limit、再余额恢复 | 多 dimension verdict 并存；余额恢复只清除 balance，disabled 仍阻断；结果不受 observation/SQL 顺序影响 |

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
24. 本地下游取消事实只来自本地连接/cancellation token；上游状态码不能伪造该事实。
25. 错误诊断预算只限制错误识别所保留的字节，不得限制或整包缓存合法成功响应。
26. committed 输出后任何内部持久化/观测失败都不得触发透明 replay 或第二终态；补偿通过 durable outbox 至少一次投递、幂等应用并有界可诊断，不承诺跨网络物理 exactly-once。
27. Retry-After、本地 backoff、admission wait 与 request deadline 只有一个明确计时 owner；同一等待不重复累计，wall-clock 变化不延长请求。
28. replay safety 只由 request-send phase、操作语义、body 可重放性与权威 idempotency capability共同证明；下游 precommit 不证明上游未接受。
29. parser 的字节、allocation、深度、token/string 与 CPU work 均有硬上限；shared memory 按实际同时分配峰值守恒。
30. SSE 成功必须满足版本化协议终态；EOF、malformed 和 downstream write failure 不伪装成功，committed 后永不产生第二终态或透明 replay。
31. capacity circuit 只消费匹配 scope 的 Confirmed provider capacity observation；本地资源饱和、取消和 unknown/conflicting 与 provider health 正交。
32. 一个逻辑请求内所有 attempt 消费同一组不可变 profile snapshot；配置热更新不改变在途语义。
33. Subject scope 与 failure dimension 正交；同一 subject 的独立抑制原因不会互相覆盖或被无关成功清除。
34. 同目标 retry 只消费 resolver 生成的完整 `TargetExecutionCommitment`；新增执行相关 revision 必须更新唯一 constructor/contract test，而不是散改 Execution。
35. Observation 覆盖顺序来自数据库分配的 durable monotonic sequence，不依赖 wall clock 精度、provider 时间或并发调用先后猜测。

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

### 11.1 当前实现差距与继续实施阻断项

本节记录 2026-08-12 对当前工作区的实现复核。它不是新的设计范围，而是防止“基础类型已经存在”被误报为 production cutover 已完成。状态只允许 `done with evidence | partial | pending | externally blocked`，不得用“代码已存在”替代 exit gate。

| Task | 当前状态 | 已有证据 / 剩余关闭条件 |
|---|---|---|
| 0 基线 | done with evidence | baseline audit、fixture catalog、deletion ledger/manifest 与 `node scripts/upstream-error-contract.test.mjs` 已同步；`pnpm.cmd verify:fast`、`cargo check --locked`、`pnpm.cmd test` 和 `pnpm.cmd build` 于本工作区退出码 0。 |
| 1 Evidence/parser | partial | bounded envelope/rule、HTTP/SSE 共用解析、32MiB shared diagnostic-memory、保守 scratch、2xx semantic envelope 与定向协议回归已接入；HTTP client 显式关闭 gzip/deflate/brotli 自动解压，并以三种编码验证 error capture 保持 wire-byte 上限。100 个 SSE bootstrap 与 decoder（含 parser scratch）并发测试证明共享预算有界并 RAII 回收；仍缺完整取消/真实执行路径并发资格及全量资格。 |
| 2 Canonical/effect | partial | canonical outcome/effect 已进入 production composition，group/capability typed producer、effect 与 Planner batch consumer 的 durable/persistence 回归已接线；`routing_loopback_e2e`（3）已证明 `model_not_found` 和 Sub2API `SUBSCRIPTION_NOT_FOUND` 的“写入→下一 snapshot 精确排除→无关 subject 可选→对应 revision 恢复”。仍缺 scoped verdict 的 shadow-rebuild/crash 全矩阵。 |
| 3 Capacity runtime | partial | same-target retry、同域 exclusion、一次可信跨域终局、request-local budget、FIFO waiter/cancel RAII、HalfOpen、权威 reload/revalidation 已接入；`station_type` 已纳入 commitment 且变更推进 endpoint revision。100 个并发 permit 的上限与释放已有定向回归；精确 transport phase 与真实 execution soak 仍未关闭。 |
| 4 HTTP cutover | done with evidence | non-2xx 与 bounded 2xx semantic envelope 经唯一 canonical path，成功大响应保持受端点上限约束；专项 contract、architecture gate 与 `verify:fast` 通过。精确 socket send phase 见 Task 3/transport blocker。 |
| 5 SSE cutover | partial | Chat/Responses 共用 evidence/canonical，precommit 可重试、committed 不重试，memory/backpressure/EOF/fault 定向测试已接线；新增全两段边界首错误回归。编码 HTTP diagnostic、32 MiB 共享内存及 100 个 parser/bootstrap 并发已有定向证据；慢消费者和真实 execution 高并发矩阵仍待资格。 |
| 6 Scoped health | partial | `0035` scoped verdict、capability 单 owner、group producer、同事务 lifecycle 应用与 Planner 批量读取已接线；`routing_health_verdict_persistence`（14）已证明 shadow rebuild 将 `(ingested_at_ms, ingestion_sequence, observation_id)` 完整原子切换并拒绝 stale proof，启动时会从保留的 immutable observations 重建被 portable restore/reset 的 runtime projection，且第二次启动无操作；该组也证明同一 subject 的 balance recovery 不清除 account-lifecycle verdict。`intelligent_routing_persistence`（11）和 `routing_loopback_e2e`（3）覆盖 capability 与 group 的 production-composition recovery。仍缺所有维度组合及端到端 portable reimport/crash 故障注入矩阵。 |
| 7 Public adapter | done with focused evidence | capacity/upstream auth public golden、success header allowlist、HTTP/SSE mapping focused tests通过；真实 Codex 行为属于 Task 10.3 外部门禁 |
| 8 Observability | done with evidence | `DecisionTraceProfileV1` ring/截断/字段上限、低基数 metrics、durable routing outcome summary、IPC query 与 redaction contract 已接线；`observability_contract`（27）、outbox/trace专项测试和 bindings check 通过。 |
| 9 Deletion/architecture | done with evidence | `should_fallback` 与旧 compatibility writeback 已删除或隔离；`node scripts/intelligent-routing-architecture.test.mjs`、`node scripts/upstream-error-contract.test.mjs`、generated bindings check 和 `verify:fast` 全绿。 |
| 10 Qualification | partial | `verify:fast`、前端 build/test、专项 Rust tests 与 bindings check 均已取得退出码 0；`verify:full` 与完整 Cargo suite 受本环境 124 秒时限未取得退出码 0，真实 provider/Codex smoke 等待明确授权。 |

以下未关闭项目决定 Tasks 3-9 仍不能整体宣称完成：

| 阻断项 | 当前实现风险 | 必须达到的关闭证据 |
|---|---|---|
| Capacity retry 控制器 | immediate permit 已在 sleep 前取得、独立 sleep budget 与最终 cooldown 已接入，但 production 尚未消费 FIFO waiter/cancel/shutdown 路径；HalfOpen 与并发边界证据不足 | waiter 在等待前准入；取消/关闭可释放；fake-clock 证明总等待、deadline、Closed/Open/HalfOpen 和最终耗尽行为；100 并发不突破 active/waiter 上限 |
| 同域抑制与跨域终局 | 普通 Planner 尚未消费本请求的 capacity-domain exclusion；同目标耗尽后可能重新选择同域 sibling Key，跨域 attempt 也可能继续开启普通 retry 链 | request-local budget/exclusion 进入唯一 Coordinator；same-domain suppression、最多一次跨域且跨域后终止的端到端 trace |
| 完整同目标承诺 | 当前重试主要复用 candidate/snapshot，尚无 resolver 唯一生成的 `TargetExecutionCommitment`；模型别名、eligibility 或其他 revision 漂移可能漏检 | commitment 唯一 constructor、resolver 再验证、任一执行相关 revision 注入变化的契约测试；Execution 无字段拼装逻辑 |
| Replay safety 事实 | transport 尚未完整上报 request-send phase；仅凭 precommit/canonical 默认值可能错误重放已接受或已计费请求 | connect/TLS/headers/partial-body/full-body/response-started 单调 phase 注入测试；非幂等且无权威 idempotency 的 uncertain attempt 不重放 |
| SSE 错误分类 | 共享 evidence/canonical、precommit capacity retry 和 committed no-retry 已有 focused evidence；共享内存/背压/EOF/fault 全矩阵尚未完成 | 保持 HTTP/SSE canonical 等价测试，并补齐 shared-memory、慢消费者、EOF、取消、双终态和全量 qualification |
| 共享诊断内存 | `DiagnosticMemoryBudget` 已有基础类型，但生产 HTTP/SSE capture 尚未统一取得 permit；不可观测 parser allocation 也尚无保守预留合同 | HTTP/SSE/decoder scratch 共用 `32MiB`；扩容前按 owned capacity + documented scratch upper bound 准入；超限 fail-fast；100 并发 bounded-memory test |
| Scoped health 语义 | `(generation, scope, dimension)`、数据库 monotonic ingestion sequence 和 production lifecycle writer 已落地；但 canonical 没有独立 StationGroup target producer，Planner/batch/rebuild 的全部门禁未闭环 | group producer/consumer E2E；并存/独立恢复；单 SQL batch/query-plan；并发 writer、shadow rebuild、迁移/reimport/crash parity |
| Canonical lifecycle writer | terminal 与 durable effect 已在同一写事务接入，重复 terminal 不重复 effect；跨网络 commit、outbox/degraded 和 crash 矩阵尚未全部证明 | 编译期穷尽 converter；pre/postcommit store fault、outbox replay、payload collision、重复/迟到与崩溃恢复测试 |
| Capability 单一 owner | `model_not_found` 只写 capability，PlanningSnapshot 已读取 revision-fenced capability verdict；缺少端到端 focused test | terminal 写 verdict -> 下一 snapshot 精确排除 -> revision 变化恢复；manual blocklist 与 learned capability 分离 |
| Public/Codex 契约 | public golden、header allowlist、Retry-After/SSE 映射 focused tests 已通过；真实 Codex retry 尚未授权验证 | 保持 status/type/code/message/header golden；真实 Codex smoke 只作为 release qualification 外部门禁 |
| Group/subscription target | SQL/store 已支持 station_group，但 canonical/effect planner 仍没有独立 typed target producer，可能继续把 group failure 降级为 StationAccount | 新增闭合 target + typed subject + rule/effect/planner E2E；无法解析 group 时 neutral，绝不猜测或污染 account lifecycle |
| Trace 与架构删除 | bounded trace、低基数指标、版本持久化未完成；`should_fallback` 已移除，但旧 route classifier/helper 与 architecture gate 尚未清零 | DecisionTraceProfileV1 边界测试；无 raw message/secret；deletion ledger 与 architecture gate 证明旧 owner 零生产引用 |

剩余实施顺序继续遵守原 Tasks：先补齐 request-local attempt/deadline/wait/domain-exclusion 状态机，再扩大已接入 HTTP/SSE 的 retry 行为；先证明 canonical lifecycle effect 的生产持久化与故障恢复，再删除最后的旧 owner。不得通过延长重试次数、放宽 replay gate、保留双写/双分类或把新字段压回旧 `HardFail/TryNextCandidate` 来缩短 cutover。

| 维度 | 已冻结保障 | 实施期仍需证明的外部证据 |
|---|---|---|
| 可靠性 | 同目标有界 retry、完整 target commitment、request-send-phase/idempotency gate、同域 suppression/单探针 HalfOpen、共享 retry/memory admission、monotonic deadline、allocation/parse-work 上限、lease/memory 释放、HTTP/SSE/EOF/背压边界、conflict downgrade、scope+dimension 独立恢复、durable ordering、事务内原子与 outbox effectively-once effect | 真实 capacity/流式故障、100 并发 soak、sleep/resume/shutdown、Codex 客户端重试 |
| 可维护性 | 唯一 CanonicalOutcome owner、target/dimension/acceptance/replay/billing 正交建模、编译期穷尽的 effect 无损传递、请求级不可变 profile snapshot、原子旧链删除、闭合 trace code、golden fixtures、migration/rebuild owner | architecture gate 零旧 owner、当前脏工作区合并冲突审计、全量 `verify:full` |
| 可拓展性 | typed provider rule set、保守 unknown fallback、唯一 target commitment constructor、provider/model-alias/profile version、failure-domain projector、HTTP/SSE 共用 evidence/outcome、consumer 不识别供应商 code、第二 gateway conformance、热更新不改变在途请求语义 | 新 code 的 rule-set-only 变更演练、新 failure dimension 的穷尽性演练、真实不同 deployment identity 的跨域证明 |

审计后的残余风险与处理：

1. OpenAI capacity 是否真正全局会随 provider 部署变化；计划不假设站点一定独立，只有权威 provider/deployment identity 才允许跨域。
2. Codex 对 status/type/code 的重试行为属于外部客户端合同；fixture 只能做回归，发布前仍必须完成真实 Codex smoke。
3. Sub2API 动态 message 会变化；只有版本化 signature 命中才获得 Confirmed capacity，未命中安全降级且不得污染 credential。`server_is_overloaded`、`slow_down`、`529/overloaded_error` 也必须受 provider profile/status/protocol guard 约束，不能跨供应商按字符串全局解释。
4. scoped health 需要 schema/projector cutover；本计划已将其改为必做 Task，而不是可选优化。旧 key-only snapshot 未退出 Planner 前，不得宣称作用域问题已解决。
5. committed 后把 capacity terminal 映射为 `server_error` 可能促使客户端重放已产生部分输出的请求；本地代理自身不重放，Decision Trace 必须标记 partial/committed，由客户端决定是否重试。
6. 真实 capacity 难以稳定制造；缺少真实 provider 授权或无法复现时只能阻止 release qualification，不能放宽为“测试通过即发布”。
7. 对非 capacity 的 possibly-accepted 5xx/transport 关闭透明 fallback 会降低局部可用性，但避免代理静默重复执行或重复计费；只有后续获得权威 idempotency contract 才能放宽，不能以“未向客户端输出”代替证明。
8. 本次 plan-only 审计执行 `node scripts/intelligent-routing-architecture.test.mjs` 时，当前工作区因 `src/features/routing/LocalRoutingSettingsEditor.tsx` 出现 `updateSettings` 而触发既有 owner 门禁；该文件不属于本计划文档改动，Task 0 必须先确认是用户并行改动还是架构合同需要同步，并在 Task 9 前以正确 owner 设计消除，禁止删除断言或用 suppression 掩盖。
9. scoped verdict 最容易出现“表已经多作用域、语义仍然 key-only”的假完成：计划现已冻结独立 group scope、model/deployment commitment、durable ingestion 全序、scope-specific recovery、SQL statement-count 门禁和 capability 单 owner，避免只靠 nullable 列与字符串 scope 宣称完成。
10. projector rebuild 若采用清表后回放，会在重建窗口把 durable block 暂时解释成健康；计划现已要求 shadow generation、watermark/哈希对账和单事务 active-generation cutover，并把 crash-point 与 rollback 纳入 focused gate。
11. committed 输出与 SQLite 写入不存在真正跨网络原子事务；计划现已区分 durable outbox/checkpoint 与仅用于诊断的有界内存缓冲，禁止把内存补偿误称为 crash-safe exactly-once。
12. 2xx error envelope 检测若通过整包读取实现，会把正常大型 JSON/stream 变成新的内存和延迟风险；计划现已要求端点感知的有界 bootstrap，并明确成功转发不受错误诊断上限截断。
13. HTTP 499 在不同部署里既可能表示客户端取消，也可能只是上游自定义状态；计划现已把本地 cancellation signal 与 upstream status 分离，避免错误归因和错误停止策略。
14. 流式输出无法与本地数据库事务形成真正原子提交；计划现已将一致性承诺改为“precommit fail closed + postcommit 不重试/不双终态 + durable outbox 或明确 degraded 诊断”，并要求 fault test，不再用无法实现的原子性措辞掩盖 crash window。
15. 单纯把表拆成 credential/account/endpoint scope 仍会让同一对象上的 disabled、balance、quota、rate-limit 互相覆盖；计划现已把 `FailureDimension` 纳入 canonical effect、唯一键、projector partition、恢复和 Planner 组合合同。
16. `(ingested_at_ms, observation_id)` 在并发和时钟回拨下不是可靠业务全序；计划现已要求数据库分配的 durable monotonic ingestion sequence，并把并发 writer/rebuild parity 纳入门禁。
17. “同 target identity”若只检查 credential/endpoint revision，可能在模型别名、enabled、account、group/policy 变化后继续发送旧目标；计划现已冻结由 resolver 唯一生成和验证的 `TargetExecutionCommitment`，并要求携带 group-binding identity 与专用 routing-policy revision。Execution 不再维护易漏字段清单；测试必须逐轴变更并证明 lease 释放、预算不重置，不能只断言 struct 含有字段。
24. 仅重新调用 resolver 但不把首次 commitment 与重试 commitment 比较，仍然可能把“同 Key 的新配置”误当同目标重试；首次 commitment 必须贯穿 attempt，重试 resolver 产出的完整 commitment 由 resolver 自己比较，不允许 Execution 展开字段。
25. `ReplaySafe` 若被 Execution 当作无条件通行证，会在 transport 已发送或状态未知时重复执行非幂等 POST；计划现已把 canonical replay 结论降为 gate 的一个输入，并要求 fake transport 对全部 phase 驱动真实 fallback/same-target 分支。
26. 使用首次 planning snapshot 再调用一次 resolver 不等于重新验证：expected 与 current 来自同一冻结对象时比较必然通过。Task 3 必须为每次同目标 retry 建立新的权威 read snapshot，并以并发禁用、credential/account/group/policy revision 变化的集成测试证明旧 lease 释放且请求携带原预算进入 replan。
18. `model_not_found` 同时写 capability 与 model-on-key health 会产生两份 unsupported truth；计划现已规定 capability projector 是唯一 owner，health 表的 model scope 不得产生同义 verdict。
19. 跨网络/崩溃窗口无法证明物理 exactly-once；计划现已使用同事务原子提交或 transactional outbox 至少一次投递 + 幂等应用的 effectively-once 合同，并要求 crash replay 和 payload collision 测试。
20. 数据库预留 `station_group` 并不等于业务已支持 group 作用域；canonical target、rule producer、effect planner、typed subject 和 Planner consumer 缺一不可，否则必须 fail neutral。
21. 第三方 JSON/解压库内部 allocation 往往不可精确观测；计划不再要求不可证明的“精确记账”，而要求调用前保守预留可证明上界，宁可拒绝诊断也不能突破共享预算。
22. 多个中转站最终都调用 native OpenAI 时，换站通常不会跨越 provider/model capacity domain；没有权威 deployment/region 隔离证据时，跨域 fallback 默认禁用。这与“capacity 同 Key 重试”的用户预期一致。
23. 工程 cutover 与真实 provider 发布资格分开记账；缺密钥/授权不阻止工程任务诚实关闭，但永远不能据此宣称 release-qualified。

本计划评审通过的判定不是“覆盖了所有 Sub2API 文案”，而是：新增或变化的响应能够经有界 evidence parser 和版本化 rule set进入同一个 CanonicalOutcome；已知情况得到精确副作用，未知/矛盾情况安全降级，且任何 consumer 都不重新解释原始 status/type/code/message。
