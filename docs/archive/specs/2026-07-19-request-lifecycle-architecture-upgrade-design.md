# Relay Pool 请求生命周期架构升级 Spec

日期：2026-07-19
状态：设计冻结，实施计划已编制
适用范围：本地 OpenAI-compatible v2 proxy 的请求生命周期、候选尝试、协议完成、下游交付、健康反馈、请求日志与终结服务
上位约束：`docs/PROJECT_PLAN.md`、`2026-07-17-local-routing-reliability-upgrade-design.md`、`2026-07-18-persistence-architecture-v2-upgrade-design.md`

## 1. 执行摘要

当前 v2 proxy 已经完成 Axum/Hyper/Reqwest 迁移、统一候选执行、流式 body 包装、请求日志终结和基础健康反馈，但请求生命周期的最终解释权仍分散在 ingress、execution、runtime、response body、routing repository 和 database 六个边界中。

这不是单个字段或单条错误分支的问题。当前模型同时存在三项 P0 架构缺陷：

1. **请求生命周期状态所有权分散**：execution 在协议尚未完成时构造 `FinalRequestOutcome` 并标记 success；response body 随后原地改写为 failed 或 interrupted；runtime 还会构造未被消费的伪 outcome。
2. **候选反馈粒度错误**：一次请求可以尝试多个 Station Key，但持久化模型只允许一个 `feedback`；前置失败候选只有 `AttemptTrace`，不会可靠影响健康、冷却或下一次选路。
3. **协议完成语义不完整**：HTTP 2xx、收到响应头、首个可转发字节、协议终态、上游 EOF 和下游交付是不同事实；当前只有原生 `/v1/responses` 流对 `response.completed` 做了部分检查，其他流式和 buffered 协议没有统一完成合同。

本升级把现有的“一个可变 outcome 贯穿所有层”替换为四个有限状态机：

```text
RequestLifecycle       一次本地请求的唯一状态权威
AttemptLifecycle       每个 Station Key 尝试的唯一状态权威
ProtocolMachine        上游响应是否满足协议完成合同的唯一权威
DeliveryLifecycle      下游 body 是否完成交付或断开的唯一权威
```

状态机之间只传递 typed event 和 immutable evidence。最终持久化拆成两类事实：

- `request_attempts`：每个候选尝试一条权威记录，并原子应用该 attempt 的健康影响；
- `request_logs`：每个本地请求一条最终摘要，记录整体完成、失败、部分成功或下游中断。

本升级不引入通用事件总线、actor framework、工作流 DSL、动态 provider plugin 或第二套数据库权威。它是 Relay Pool 模块化单体内部的有限领域状态机升级。

## 2. 设计目标

### 2.1 可维护性

- 每类状态只有一个 owner；任何其他模块只能提交事件，不能直接改终态字段。
- 状态和失败分类使用封闭 enum；字符串只在数据库和 API DTO 边界序列化。
- transport、protocol、retry、health、delivery、persistence 可独立测试。
- 删除 `FinalRequestOutcome` 可变大对象、伪 `ProxyHttpResponse.outcome` 和单一 `CandidateFeedback`。
- `execution.rs` 不再写数据库终态，也不再根据 downstream 结果修改健康。
- `response_body.rs` 不再持有或原地改写完整 request log 结构。
- 不形成新的 `LifecycleManager`、`ProxyContext` 或 `AppServices` 同等级上帝对象。

### 2.2 可扩展性

- 新增 endpoint 或上游协议时，只新增一个编译期 `ProtocolContract` 和必要 transform，不复制 retry、feedback、logging 循环。
- 新增失败类型时，必须显式定义 blame、retry disposition 和 health effect；缺省分支 fail closed。
- 新增健康策略时消费 canonical `AttemptTerminal`，不解析 HTTP body、错误字符串或 request log JSON。
- 新增请求日志字段时从 canonical evidence 投影，不跨层回读 transport 或 provider 对象。
- provider-specific 兼容行为必须位于 sealed adapter/contract 中，不能污染 Router、Scheduler 或通用 lifecycle。

### 2.3 可靠性

- 每个 admitted request 最多一个 request terminal；每个 attempt 最多一个 attempt terminal。
- 每次失败尝试都被分类和持久化；是否惩罚由 HealthEffect 决定，而不是由“是否最终候选”决定。
- HTTP 2xx 和首块数据绝不等价于协议完成。
- retry 只由 downstream commit barrier 决定；健康影响不与 retryability 耦合。
- request log、attempt record 和健康投影具有数据库幂等键和明确事务 owner。
- finalization 队列有界、可观测、可重试、可 drain；持久化失败不能静默确认。
- request admission lease 持有到 downstream body EOF、error、cancel 或 drop，保证 active request 与 graceful shutdown 语义真实。
- authenticated admission 只有在 request-start durable ack 成功后才成立；此前的容量或持久化拒绝必须明确返回 503 并计数，不能伪造已持久化终态。
- request start、attempt terminal 和 request terminal 经同一个有序 writer command stream 提交；重试前等待前一 attempt 的 durable ack。

## 3. 非目标

- 不扩展 Files、Batches、Audio、Images、Realtime 或 Assistants endpoint。
- 不引入 Kafka、NATS、Redis、外部 sidecar 或跨进程事件系统。
- 不建设任意 provider/plugin ABI；协议集合保持编译期封闭。
- 不重写 Router/Scheduler 的价格、余额、倍率和 group filter 策略。
- 不把所有内部阶段逐条写入数据库；持久化只保留 request start/terminal、attempt terminal 和必要诊断。
- 不为了兼容偶然内部类型而永久保留双路径、双写或旧 outcome adapter。
- 不把请求日志变成完整 payload、header 或 secret 追踪系统。

## 4. 审计基线

### 4.1 生命周期所有权分散

当前关键路径：

```text
ingress::handle
  -> CanonicalProxyRequest
  -> V2ProxyExecutor::execute
  -> ExecutionEngine::execute
  -> ProxyExecutionResponse::from_prepared
       构造 status=success + feedback=Success
  -> FinalizingStream
       可能改写 failed/interrupted/feedback
  -> FinalizationDispatcher
  -> RoutingRepository::record_final_outcome
  -> AppDatabase::finalize_request_log
```

问题：

- `FinalRequestOutcome` 在最终协议结果未知时已经命名为 Final 并标记成功。
- `FinalizingStream` 直接修改 status、lifecycle status、failure source、completion source 和 feedback。
- pre-commit failure 由 runtime 的 `FailedRequestContext` 另行构造终态，成功和失败使用不同计时起点。
- `ProxyHttpResponse.outcome` 在 production ingress 中不被读取，runtime 仍填入 `success("queued")`。
- auth、body timeout、JSON parse、metadata rejection 和 finalization admission failure 是否进入 request journal 没有统一合同。
- `RequestLease` 随 `CanonicalProxyRequest` drop，不能保证 streaming response 生命周期内 `active_requests` 一直准确。

### 4.2 反馈粒度错误

当前 retry loop 为每次候选记录：

```rust
AttemptTrace {
    station_key_id,
    failure_code,
    duration_ms,
}
```

但 `FinalRequestOutcome` 只有一个：

```rust
feedback: Option<CandidateFeedback>
```

因此：

- A 失败、B 成功：A 只有日志 trace，B 得到 Success feedback；
- A、B、C 全失败：通常只有最后一个候选得到 Failure feedback；
- models aggregation 的多个成功/失败候选没有逐 attempt feedback；
- retry decision 和 health decision 缺少独立持久事实；
- 同一坏 key 可能在并发请求中继续被选择，直到另一个请求整体终结并刷新数据库健康。

### 4.3 协议完成语义不完整

当前 `ResponseMode` 只区分 buffered/streaming 和 Chat-to-Responses transform：

```text
BufferedJson
BufferedChatToResponses
StreamPassthrough
StreamChatToResponses
```

它没有表达协议完成条件。

已确认问题：

- 原生 Responses SSE 只检查是否见过 `response.completed`，没有将 `response.failed`、`response.incomplete` 建模成协议终态。
- Chat Completions passthrough stream 不验证 `[DONE]` 或其他显式 terminal。
- Chat-to-Responses decoder 在干净 EOF 时也调用 `complete_once()`，可能为缺失 `[DONE]` 的上游流合成假的 `response.completed`。
- native buffered Chat、Responses 和 Embeddings 对 2xx body 缺少 endpoint envelope 校验。
- HTTP headers accepted、raw first chunk、downstream first byte、protocol completed 和 transport EOF 没有独立事件。
- downstream drop 会清除统一 feedback，但它无法区分“上游协议已完成、只是客户端没收完”与“上游尚未完成”。

### 4.4 持久化与终结缺陷

- finalization worker 失败后只输出 stderr，job 不重试且没有 unhealthy admission gate。
- channel closed/full 的 fallback spawn 不提供可靠交付保证。
- `finalized_request_ids` 是进程内永久 HashSet，与数据库 unique constraint 形成第二幂等权威并持续增长。
- shutdown 没有显式等待 finalization queue、worker 和持久化事务 drain。
- `attempts_json` 是展示 blob，无法作为逐 attempt 原子健康写入的权威事实。

## 5. 架构原则

### 5.1 一个状态机一个权威

“统一所有权”不等于把所有逻辑放进一个对象。目标是：

| 状态 | 唯一 owner | 允许的输入 | 产物 |
|---|---|---|---|
| Request | `RequestLifecycle` | attempt、protocol、delivery terminal | `FinalRequestRecord` |
| Attempt | `AttemptLifecycle` | transport、protocol、commit event | `AttemptTerminalRecord` |
| Protocol | `ProtocolMachine` | headers、bytes、EOF、transport error | `ProtocolProgress/Terminal` |
| Delivery | `DeliveryLifecycle` | downstream poll/write/drop | `DeliveryTerminal` |
| Durable write | `LifecycleWriter` | 已完成的 canonical record | DB commit result |

### 5.2 事实与决策分离

- Transport 产生事实：connect error、HTTP status、headers、chunk、EOF、timeout。
- ProtocolMachine 解释事实：completed、failed、incomplete、malformed。
- Attempt classifier 产生两个独立决策：retry disposition 和 health effect。
- Request reducer 根据 attempt/protocol/delivery 事件生成请求终态。
- Persistence 只保存 canonical record 并应用已决定的 health transition，不重新解释错误字符串。

### 5.3 不可逆 commit barrier

downstream commit barrier 是第一个真正交给下游的非空字节，不是上游 raw chunk，也不是 HTTP 2xx。

- commit 前的 retryable failure 可以切换 candidate；
- commit 后任何 failure 都必须停止 fallback；
- commit 后的 upstream failure 仍可影响 key health；
- retryability 和 health effect 永远是两个字段。

## 6. 目标模块边界

```text
src-tauri/src/services/proxy/
  ingress.rs
  execution.rs
  upstream.rs
  endpoint_adapter.rs
  runtime.rs
  server.rs

  lifecycle/
    mod.rs
    request.rs             # RequestContext, RequestLifecycle, request reducer
    attempt.rs             # AttemptContext, AttemptLifecycle, classifier
    protocol.rs            # ProtocolContract, ProtocolMachine, terminal evidence
    delivery.rs            # LifecycleBody and downstream delivery tracking
    writer.rs              # bounded finalization worker and drain
    ports.rs               # RequestJournal / AttemptJournal consumer-owned ports

  protocol/
    mod.rs
    responses_json.rs
    responses_sse.rs
    chat_json.rs
    chat_sse.rs
    embeddings_json.rs
    models_json.rs
    local.rs

  observability.rs
  error.rs
  router.rs
  scheduler/
```

边界规则：

- `execution.rs` 只编排 route plan 和 attempt，不构造数据库 DTO。
- `upstream.rs` 只产生 transport event，不判断 retry、health 或 request success。
- `protocol/*` 不访问 Router、Scheduler、SQLite 或 Station Key secret。
- `lifecycle/request.rs` 不解析 JSON/SSE，不执行网络或 SQL。
- `lifecycle/attempt.rs` 不写 response body，不直接查询数据库。
- `lifecycle/delivery.rs` 不修改 candidate health，只向 owner 提交 delivery/protocol event。
- `lifecycle/writer.rs` 不重新分类错误，不读取 HTTP body，不选择 candidate。
- `ports.rs` 是 proxy domain 到 persistence 的唯一写边界。
- 所有新模块默认 crate-private；禁止为了测试扩大 public surface。

## 7. Canonical 类型

### 7.1 RequestContext

```rust
pub(crate) struct RequestContext {
    pub request_id: RequestId,
    pub received_at_ms: i64,
    pub method: RequestMethod,
    pub local_path: LocalEndpointPath,
    pub endpoint: RouteEndpointKind,
    pub model: Option<String>,
    pub stream: bool,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub requirements: RequestRequirements,
}
```

`received_at_ms` 在 ingress 分配 request id 后立即生成。body collection、routing、attempt 和 delivery 的所有时间都以它为共同起点。

### 7.2 Request 状态

```rust
pub(crate) enum RequestPhase {
    Accepted,
    Routing,
    Attempting { ordinal: u16 },
    Committed { attempt_id: AttemptId },
    Terminal(RequestTerminal),
}

pub(crate) enum RequestTerminal {
    Completed(RequestCompletion),
    PartialSuccess(RequestCompletion),
    Failed(RequestFailure),
    Interrupted(DeliveryFailure),
}
```

只有 `RequestLifecycle` 可以推进该状态。非法 transition 返回 typed invariant error，不能静默忽略。

### 7.3 Attempt 状态

```rust
pub(crate) struct AttemptContext {
    pub attempt_id: AttemptId,
    pub request_id: RequestId,
    pub ordinal: u16,
    pub station_id: String,
    pub station_key_id: String,
    pub station_endpoint_revision: i64,
    pub started_at_ms: i64,
}

pub(crate) enum AttemptPhase {
    Started,
    AwaitingHeaders,
    ReadingBufferedBody,
    BootstrappingStream,
    Committed,
    Terminal(AttemptTerminal),
}

pub(crate) enum AttemptTerminal {
    Succeeded(ProtocolEvidence),
    Failed(ClassifiedAttemptFailure),
    Abandoned(AttemptAbandonReason),
}
```

`AttemptId` 由 `(request_id, ordinal)` 确定生成，不使用时间戳随机碰撞语义。

### 7.4 失败分类

```rust
pub(crate) struct ClassifiedAttemptFailure {
    pub kind: AttemptFailureKind,
    pub blame: FailureBlame,
    pub retry: RetryDisposition,
    pub health: HealthEffect,
    pub public_code: ProxyFailureCode,
    pub sanitized_detail: Option<String>,
}

pub(crate) enum RetryDisposition {
    TryNextCandidate,
    StopRequest,
}

pub(crate) enum HealthEffect {
    Success,
    ObserveFailure,
    Cooldown { retry_after_ms: Option<i64> },
    HardFail,
    Neutral,
}
```

分类规则必须显式覆盖：auth、balance、rate limit、connect、timeout、5xx、model unavailable、capability mismatch、bad request、malformed response、protocol incomplete、stream interrupted、local adapter、downstream drop。

`output_committed` 只能影响 retry，不能自动把 upstream failure 改成 `Neutral`。

## 8. RequestLifecycle 合同

### 8.1 观测边界

request journal 默认记录“通过本地 bearer 鉴权、成功预留生命周期写容量并得到 durable request-start ack 的请求”。这个时刻定义为 `Admitted`，不是仅仅“鉴权成功”。

- missing/invalid local auth 不进入用户请求日志，避免安全噪声；只增加脱敏计数器；
- writer unavailable、容量预留失败或已确认 rollback 的 request-start failure 发生在 `Admitted` 之前：返回 typed 503，增加 `lifecycle_admission_rejected` 指标，不声称已有 request journal；
- request-start ack timeout/connection loss 属于 `CommitOutcomeUnknown`：停止 upstream，按 request_id 对账；row 存在则 CAS 终结为 `admission_aborted`，row 不存在才按无 journal rejection 处理，禁止猜测提交结果；
- `Admitted` 之后的 body timeout、body too large、invalid JSON、unsupported method、unknown route、routing failure 和 terminal write failure 必须进入唯一 request terminal 流程；
- OPTIONS preflight 不进入 request journal；
- local `/usage` 请求不产生 candidate attempt，但必须产生 request terminal。

Axum route matching 不得绕过该边界。通过本地鉴权的 unknown path、method-not-allowed 和 fallback 必须进入统一 ingress lifecycle；可以使用外层 middleware + catch-all handler 或等价结构，但不得继续由裸 `.fallback(not_found)` 直接返回未观测响应。

### 8.2 Admission lease

`RequestLifecycle` 拥有 `RequestLease`。lease 只能在以下任一终态释放：

- buffered body 完整交付；
- streaming body EOF；
- body error/cancel；
- downstream drop；
- pre-commit failure已提交终态；
- forced shutdown 已生成 interrupted terminal。

handler 返回 Axum response 不得提前释放 lease。

### 8.3 FinalRequestRecord

`FinalRequestRecord` 只能由 request reducer 在 terminal transition 时构造。它不提供 `success()` 默认构造器，不允许占位 request id、path、status 或时间。

```rust
pub(crate) struct FinalRequestRecord {
    pub context: RequestContextSnapshot,
    pub terminal: RequestTerminalSnapshot,
    pub selected_attempt_id: Option<AttemptId>,
    pub attempt_count: u16,
    pub fallback_count: u16,
    pub delivery: DeliveryTerminal,
    pub usage: Option<ObservedUsage>,
    pub timing: RequestTiming,
}
```

`ProxyHttpResponse` 只保留 status、headers 和 payload；删除 outcome 字段。

## 9. AttemptLifecycle 与逐候选反馈

### 9.1 每个 attempt 必须终结

每次 candidate 被调用前创建 AttemptLifecycle。除不可恢复的 hard process kill 外，以下路径都必须终结：

- adapter prepare failure；
- proxy route/client build failure；
- connect/DNS/TLS failure；
- HTTP non-success；
- buffered read/parse/schema failure；
- first downstream-visible byte 前 stream failure/EOF/timeout；
- commit 后 stream error/idle/incomplete；
- protocol completed；
- downstream 在 protocol terminal 前 drop；
- request cancellation/forced shutdown。

hard process kill 不得伪造未知 attempt 结果；durable request journal 在重启后标记 `process_terminated`，并把“最后一个 attempt 可能缺少 terminal record”作为显式 crash 语义和指标暴露。

### 9.2 即时反馈顺序

pre-commit 失败 attempt 的处理顺序固定为：

```text
classify failure
  -> terminalize AttemptLifecycle
  -> apply in-memory scheduler/health projection
  -> send FinishAttempt through reserved writer permit
  -> await durable commit acknowledgement
  -> evaluate retry disposition
  -> optionally begin next attempt
```

不能等整个 request 完成后才反馈前置失败候选，也不能在前一 attempt 尚未 durable commit 时启动下一次 retry。SQLite 写入延迟是明确的可靠性成本，必须单独测量；若超预算，应优化事务和 writer，而不是放宽确认顺序。

持久化失败不得让同一 attempt 被重复分类。内存投影按 `attempt_id` 幂等；数据库通过唯一键幂等。

### 9.3 健康含义

candidate health 描述上游 Station Key 的可靠性，不描述下游客户端行为。

| 事实 | HealthEffect |
|---|---|
| 完整、协议有效的 upstream response | Success |
| 401/403 或确定凭据错误 | HardFail |
| 402/确定余额耗尽 | HardFail |
| 429 | Cooldown |
| connect/timeout/5xx/协议中断 | ObserveFailure |
| request bad input/model unavailable/request-only 404 | Neutral |
| local adapter/persistence/downstream error | Neutral |
| protocol terminal 前 downstream drop | Neutral |
| protocol completed 后 downstream drop | Success |

### 9.4 Models 聚合

`/v1/models` 是一个 request 下的多 attempt 聚合，不再绕开统一模型：

- 每个被查询 candidate 都生成 AttemptTerminalRecord；
- 至少一个 candidate 成功且部分失败时，request terminal 为 `PartialSuccess`；
- 所有 candidate 失败时为 `Failed`；
- 每个 candidate 的 success/failure 独立更新健康；
- request log 的 fallback count 不再代替 models fan-out count。

## 10. 协议完成合同

### 10.1 ResponsePlan

替换当前 `ResponseMode`：

```rust
pub(crate) struct ResponsePlan {
    pub transport: TransportMode,
    pub upstream_protocol: UpstreamProtocol,
    pub downstream_transform: DownstreamTransform,
    pub completion_policy: CompletionPolicy,
}
```

```rust
pub(crate) enum UpstreamProtocol {
    ResponsesJson,
    ResponsesSse,
    ChatCompletionsJson,
    ChatCompletionsSse,
    EmbeddingsJson,
    ModelsJson,
    LocalJson,
}
```

adapter 根据 endpoint、`UpstreamApiFormat` 和 stream flag 编译出 ResponsePlan。Router/Scheduler 不读取该类型。

### 10.2 ProtocolMachine 接口

```rust
pub(crate) trait ProtocolMachine: Send {
    fn observe_headers(&mut self, status: StatusCode, headers: &HeaderMap)
        -> Result<(), ProtocolFailure>;
    fn observe_chunk(&mut self, bytes: &Bytes)
        -> Result<ProtocolProgress, ProtocolFailure>;
    fn finish_eof(&mut self)
        -> Result<ProtocolTerminal, ProtocolFailure>;
}
```

buffered 协议使用相同 contract 的 `validate_complete_body()` 便利入口，但不能绕过 terminal 语义。

### 10.3 完成矩阵

| 协议 | 成功证据 | 明确失败 | EOF 合同 |
|---|---|---|---|
| Responses SSE | `response.completed` | `response.failed`、`response.incomplete`、malformed event | terminal 前 EOF = incomplete |
| Chat SSE strict | `[DONE]` | malformed event、transport error | `[DONE]` 前 EOF = incomplete |
| Chat SSE adapter compatibility | adapter 声明的显式 terminal | malformed/ambiguous terminal | 禁止通用 EOF-success |
| Responses JSON | 完整 JSON + 合法 response terminal | failed/incomplete/error envelope/schema error | 完整 body 后验证 |
| Chat JSON | 完整 JSON + 合法 chat envelope | error envelope/schema error | 完整 body 后验证 |
| Embeddings JSON | 完整 JSON + 合法 data envelope | error envelope/schema error | 完整 body 后验证 |
| Models JSON | 完整 JSON + 合法 data array | error envelope/schema error | 完整 body 后验证 |
| Local JSON | 本地构造成功 | serialization/internal failure | 不依赖 upstream EOF |

### 10.4 Chat-to-Responses bridge

- decoder 不能在普通 EOF 时无条件调用 `complete_once()`；
- 只有 upstream Chat protocol machine 产生 Completed terminal 后，transform 才能生成 downstream `response.completed`；
- upstream `[DONE]`、usage 和 finish reason 的兼容策略必须由 Chat contract 处理；
- partial event、无 terminal EOF 和 malformed JSON 必须返回 typed protocol failure；
- synthesized Responses event 必须通过与 native Responses 相同的 downstream event contract test。

### 10.5 官方协议文档门禁

实现协议 machine 前必须从当前 OpenAI Developer Docs/API schema 固化以下 fixture，并在 spec 附属测试文档记录抓取日期和来源 URL：

- Responses streaming terminal event；
- Responses failed/incomplete event；
- Chat Completions streaming terminal；
- non-stream Responses/Chat/Embeddings/Models envelope。

当官方 OpenAI 合同与特定兼容站点行为不同，只能新增具名 adapter compatibility policy；禁止放宽通用 contract。

## 11. DeliveryLifecycle

DeliveryLifecycle 只回答下游发生了什么：

```rust
pub(crate) enum DeliveryTerminal {
    BodyCompleted,
    DownstreamDropped,
    DownstreamWriteFailed,
    CancelledByShutdown,
    NotStarted,
}
```

它不直接决定 candidate health。

关键组合：

| ProtocolTerminal | DeliveryTerminal | RequestTerminal | Attempt health |
|---|---|---|---|
| Completed | BodyCompleted | Completed | Success |
| Completed | DownstreamDropped | Interrupted | Success |
| 未完成 | DownstreamDropped | Interrupted | Neutral |
| Failed/Incomplete | 任意 | Failed | Classified failure |
| Completed | CancelledByShutdown | Interrupted | Success |

`LifecycleBody` 负责驱动 ProtocolMachine 和 DeliveryLifecycle，并把 typed terminal event 交回 RequestLifecycle/AttemptLifecycle。它不持有数据库 DTO。

`BodyCompleted` 只表示 Hyper server response body 已完成，不表示 TCP peer 已确认消费，也不是客户端业务层 acknowledgement。当前栈无法证明更强的交付语义，指标、日志和 UI 均不得把它展示为“客户端已收到”。

ProtocolMachine 一旦产生显式 terminal 即封闭，不允许后续 EOF、transport error 或 downstream drop 回写协议结论。terminal 后的 transport 异常可以形成独立诊断和 delivery 结果，但不能把已确认的 candidate `Success` 改成失败；terminal 前的 EOF/error 才进入 incomplete/failed 分类。

## 12. Retry 与 fallback

### 12.1 RetryPolicy 输入

RetryPolicy 只消费：

```rust
RetryInput {
    failure_kind,
    failure_blame,
    downstream_committed,
    idempotency,
    attempt_ordinal,
    remaining_budget,
}
```

它不读取 public error string、request log、health row 或 response body。

### 12.2 规则

- commit 前 connect、timeout、401/403、408/425、429、5xx 和明确 capability mismatch 按 typed policy 决定 retry；
- request-only 400/404/409/422 默认 stop，除非 adapter 提供明确 capability signal；
- commit 后始终 stop，不做透明 fallback；
- protocol malformed/incomplete 在 commit 前可按 adapter/idempotency policy retry，commit 后 stop；
- 所有 attempt 共享 request pre-commit 总预算；
- 每个失败 attempt 无论 retry/stop 都必须 terminalize 并产生 health effect。

## 13. 持久化模型

### 13.1 Request journal

`request_logs` 保持一次本地请求一行，但从“仅 final insert”升级为明确 journal contract：

- authenticated admission 时创建 `in_progress` row；
- terminal 时使用 compare-and-set 更新：`WHERE request_id = ? AND terminal_at IS NULL`；
- crash/restart 后 stale `in_progress` 由 recovery projection 标记 `process_terminated`；
- request id unique constraint 是唯一持久幂等权威；
- 禁止永久进程内 finalized request-id HashSet。

### 13.2 Attempt journal

新增规范化 `request_attempts`：

| 字段 | 语义 |
|---|---|
| request_id | 父请求 identity |
| attempt_ordinal | 请求内稳定序号 |
| attempt_id | `(request_id, ordinal)` 稳定 identity |
| station_id/key_id | 候选 identity |
| endpoint_revision | stale feedback guard |
| started/headers/first_byte/terminal ms | 阶段时间 |
| http_status | 可选上游状态 |
| transport_terminal | eof/error/timeout/not_started |
| protocol_terminal | completed/failed/incomplete/not_applicable |
| failure_kind/blame | canonical 分类 |
| retry_disposition | next/stop |
| health_effect | success/observe/cooldown/hard_fail/neutral |
| output_committed | 是否越过下游 commit barrier |
| sanitized_error | 脱敏错误摘要 |

唯一约束：`UNIQUE(request_id, attempt_ordinal)`。

增长策略：

- attempt 表跟随 request log retention；
- 删除 request log 时级联删除 attempts；
- UI 默认只按单 request id 有界查询 attempts；
- 禁止 unbounded attempt list API；
- `attempts_json` 作为兼容投影保留一个发布周期，然后删除 writer，只保留 migration/read fallback，最终删除字段。

### 13.3 事务 owner

Attempt terminal transaction 原子执行：

```text
insert request_attempts if absent
  -> validate endpoint_revision
  -> apply health transition when health_effect != Neutral
  -> update scheduler/health durable projection
  -> commit
```

Request terminal transaction原子执行：

```text
CAS finalize request_logs
  -> write usage/cost/request summary projection
  -> emit optional sanitized change event
  -> commit
```

transaction 内禁止网络调用、stream poll 或 retry sleep。

### 13.4 Persistence V2 集成约束

- 本 spec 不新增 `AppDatabase` 方法；写路径依赖 consumer-owned `RequestLifecycleStore` port。
- production schema/store 由 persistence V2 application/persistence boundary 实现。
- 如果 lifecycle domain kernel 先于 persistence V2 store 完成，只允许使用 in-memory test store；不得增加第二套 production SQL adapter。
- 禁止 V1/V2 dual write、shadow write 或从 `attempts_json` 反推 health。
- request/attempt schema migration 必须进入 persistence V2 versioned migration 和 released-schema fixture。

本 spec 对 Persistence V2 原“单个 final outcome + 单一 health feedback”切片构成具名的规范修订：旧的一次 request-finalization transaction 被替换为 durable request start、逐 attempt terminal+health transaction、request terminal CAS 三类命令。Persistence V2 的模块边界、SQLx runtime、single-writer、consumer-owned port、数据库幂等权威、无 dual write 和 drain 约束继续生效。实施 Task 10 前必须同步修订 Persistence V2 spec/plan 的 request-finalization 条款；两份冻结文档存在冲突时不得开始 production store 实现。

## 14. LifecycleWriter 与可靠终结

替换当前 `FinalizationDispatcher`：

```rust
pub(crate) struct LifecycleWriter {
    sender: mpsc::Sender<LifecycleWriteCommand>,
    health: Arc<WriterHealth>,
    shutdown: CancellationToken,
}

pub(crate) enum LifecycleWriteCommand {
    StartRequest {
        record: RequestStartRecord,
        ack: oneshot::Sender<Result<RequestStartAck, LifecycleWriteError>>,
    },
    FinishAttempt {
        record: AttemptTerminalRecord,
        ack: oneshot::Sender<Result<AttemptCommitAck, LifecycleWriteError>>,
    },
    FinishRequest {
        record: FinalRequestRecord,
        ack: oneshot::Sender<Result<RequestCommitAck, LifecycleWriteError>>,
    },
}
```

合同：

- 所有 request/attempt 写入共用一个有界、有序 command channel，禁止用两个 sender 造成 parent start、attempt terminal 和 request terminal 跨队列重排；
- 每个 command 都有 typed acknowledgement；ack 只在 durable commit、幂等 duplicate 已由数据库确认或明确 permanent failure 后完成；
- transient SQLite busy/locked 使用固定上限和 jitter backoff；
- permanent error 将 writer 标记 unhealthy，停止新 proxy admission，并保留脱敏诊断；
- sender 侧 owned permit 只能被对应 command 的 send 消费，不能释放后再临时抢容量；worker 取出 command 后 queue slot 可复用，但 typed ack 只能在 durable commit、数据库确认幂等 duplicate 或明确 permanent failure 后完成；
- 禁止 per-job fallback spawn；
- shutdown 固定顺序为：停止 admission、drain active bodies、关闭 sender、drain queues、等待 writer、关闭 persistence runtime；
- drain timeout 返回 typed failure，不能假装成功退出。

### 14.1 容量预留

attempt 数在 request admission 时未知，不能声称一次性预留所有未来写入。预留规则是：

1. admission 在 durable start 前预留两个 owned permits：一个发送 `StartRequest`，一个由 `RequestLifecycle` 持有到 `FinishRequest`；任一 permit 不可得则在 `Admitted` 前返回 503；
2. 每个普通 upstream attempt 在网络调用前预留一个 `FinishAttempt` permit；不可得则该 attempt 不得启动，request 以本地 lifecycle capacity failure 终止；
3. `/v1/models` 并行 fan-out 在启动任何网络调用前为本批全部候选一次性预留 attempt permits；若无法全部预留，则整批不启动，禁止形成只查询一半候选的伪 partial success；
4. permit 随对应 lifecycle owner 移动，Drop path 使用已持有 permit 同步入队，不得临时 spawn 或阻塞等待新容量；
5. queue capacity 必须满足 `2 * max_active_requests + max_concurrent_attempts + shutdown_margin` 的配置不变量，并由启动校验拒绝不合法配置。

### 14.2 request-start durability

本地 bearer 鉴权通过后，ingress 先预留容量、发送 `StartRequest` 并等待 durable ack，再收集 body、做 endpoint validation 或启动 upstream。这样 `request_attempts` 的父记录始终先存在，body timeout/invalid JSON/unknown route 也有可 CAS 终结的 journal。

- start commit failure 返回 `503 lifecycle_unavailable`，不得调用 upstream；
- start ack 必须有独立 latency histogram 和 timeout；timeout 将 writer 标记 degraded/unhealthy，不允许绕过持久化继续执行，并必须进入 `CommitOutcomeUnknown` reconciliation；
- request-start、attempt-terminal 和 request-terminal 写延迟都纳入性能门禁，避免以可靠性名义引入不可见的链路迟滞；
- hard process kill 可能留下 stale `in_progress` request；recovery 只能按事实标记 `process_terminated`，不得合成未知 attempt terminal 或 candidate health 结论。

### 14.3 attempt acknowledgement barrier

runtime overlay 在 attempt terminal 后立即应用，避免并发选路继续命中已失败 key；但 pre-commit retry 必须等待 `FinishAttempt` durable ack。ack 失败时停止当前 request 的 fallback，提交本地 persistence failure terminal，并关闭新 admission。post-commit/body Drop 路径无法 await 时，使用预留 permit 按 `FinishAttempt -> FinishRequest` 顺序同步入队；单 channel 保证 worker 观察到相同顺序，shutdown drain 负责最终确认。

## 15. 内存投影与并发一致性

为了避免失败 key 在数据库写回前被并发请求继续选中：

- AttemptLifecycle terminal 后立即把 typed HealthEffect 应用到共享 runtime health overlay；
- overlay 以 `(station_key_id, endpoint_revision)` 为 key；
- 同一个 attempt_id 只应用一次；
- durable store commit 后更新 overlay 的 durable watermark；
- persistence failure 时 overlay 保持保守状态，同时 writer unhealthy gate 停止新 admission；
- endpoint revision 变化时旧 overlay 和旧 durable feedback都不得覆盖新配置；
- Router/Scheduler 统一消费“durable snapshot + runtime overlay”的 canonical health view。

不得在 Router 中重新解析错误或自行维护第二套 cooldown 规则。

## 16. 错误与可观测性

### 16.1 Typed failure taxonomy

每个 failure 至少包含：

- stage；
- source/blame；
- canonical kind；
- retry disposition；
- health effect；
- public OpenAI-compatible code/message；
- sanitized internal detail；
- request_id/attempt_id 引用。

禁止通过字符串包含关系判断 retry、health 或 protocol completion。

### 16.2 指标

至少记录：

- active requests / active attempts；
- request/attempt queue depth；
- lifecycle writer retry count、oldest job age、health；
- lifecycle admission rejection count、request-start ack latency、attempt-terminal ack latency、request-terminal ack latency；
- protocol incomplete/failed/malformed count，按具名 contract 聚合；
- per-stage latency；
- candidate retry count 和 health effect count；
- downstream drop before/after protocol terminal；
- stale endpoint revision rejection；
- request/attempt terminal duplicate suppression count。

不记录完整 API key、Authorization、Cookie、request body、response body、原始 header、完整上游错误页或用户数据。

## 17. 可维护性门禁

### 17.1 单向依赖

允许依赖方向：

```text
ingress -> lifecycle ports + canonical request
execution -> router + adapter + attempt lifecycle
adapter -> protocol contracts + transform
delivery -> protocol machine + lifecycle event ports
lifecycle reducers -> canonical domain types only
persistence adapter -> lifecycle store ports
```

禁止：

- protocol import database/router/scheduler；
- persistence import Axum/Reqwest/ByteStream；
- router import protocol/delivery/persistence；
- response body import database DTO；
- execution 构造 `CreateRequestLogInput`；
- runtime 包含 endpoint-specific completion 判断。

### 17.2 Architecture fitness functions

CI 使用 CodeGraph/tree-sitter 结构检查：

- `FinalRequestOutcome`、`CandidateFeedback`、`ProxyHttpResponse.outcome` 不存在；
- `execution.rs` 不依赖 request-log persistence DTO；
- `protocol/` 不依赖 database、router 或 scheduler；
- `lifecycle/` 不依赖 Reqwest、Axum request 或 raw SQLite；
- SQL 只存在于 persistence V2 边界；
- 每个 request/attempt terminal enum match 必须 exhaustive；
- public boundary symbol 必须登记 owner 和 consumer；
- 新模块 fan-in/fan-out 超过基线时必须 ADR 评审；
- 无永久 request-id/attempt-id HashSet、无 unbounded queue、无 per-job spawn fallback。

## 18. 扩展规则

### 18.1 新 endpoint

必须提供：

1. canonical endpoint metadata；
2. request adapter；
3. upstream protocol contract；
4. buffered/stream completion fixture；
5. retry/health classification matrix；
6. request/attempt lifecycle integration test；
7. bounded resource profile。

不能复制 ExecutionEngine loop。

### 18.2 新 provider compatibility

- 只允许新增 sealed `CompletionPolicy`；
- policy 名称必须表达具体兼容行为，不能叫 `lenient` 或 `legacy`；
- 必须有真实 fixture 证明终态；
- policy 只能放宽该 provider/format，不能改变通用 OpenAI contract；
- 必须登记 retirement condition，防止兼容层永久增长。

### 18.3 新 health policy

- 只消费 canonical AttemptTerminalRecord；
- 明确 success/observe/cooldown/hard-fail/neutral；
- 明确 endpoint revision 和时间语义；
- 有纯 transition test；
- 不访问网络，不解析 raw body。

## 19. 实施阶段

### Stage 0：Freeze、ADR 与行为基线

- 冻结 `FinalRequestOutcome`、`CandidateFeedback`、`ResponseMode` 新增字段。
- 记录 CodeGraph dependency/fan-in/fan-out baseline。
- 固化当前真实 request log、feedback、protocol 和 active-request 行为矩阵。
- 获取官方协议 fixture 并记录来源。
- 先写所有已知错误行为的失败测试。

退出条件：目标 enum、状态机、持久化 port、schema 和删除清单无 TBD。

### Stage 1：纯领域 Lifecycle Kernel

- 实现 RequestLifecycle、AttemptLifecycle、ProtocolMachine 接口和 reducers。
- 使用 in-memory ports 完成状态转换、幂等和组合矩阵测试。
- 不接 production runtime，不写 SQLite。

退出条件：所有状态转换行为测试通过，非法 transition 确定失败。

### Stage 2：协议合同

- 实现 Responses SSE/JSON、Chat SSE/JSON、Embeddings JSON、Models JSON。
- 修复 Chat-to-Responses EOF 假完成。
- 将 ResponseMode 替换为 ResponsePlan。
- 完成 native/bridge/partial/malformed fixture matrix。

退出条件：所有协议只能通过显式 terminal 或完整 validated body 成功。

### Stage 3：Attempt Lifecycle 与反馈

- ExecutionEngine 为每个 candidate 创建 attempt。
- 拆分 retry disposition 与 health effect。
- 实现 runtime health overlay。
- Models aggregation 接入逐 attempt 记录。
- 暂以 in-memory store 运行完整 proxy integration tests。

退出条件：A 失败 B 成功、三候选全失败、post-commit failure、models partial 全部产生正确逐 attempt terminal。

### Stage 4：Persistence V2 Store

- versioned migration 增加 request journal/attempt journal。
- 实现 RequestLifecycleStore port。
- 完成 attempt+health、request terminal 原子事务。
- 加入 crash、duplicate、stale revision、busy/locked fault injection。

退出条件：无 AppDatabase 新方法、无 dual write、数据库 unique constraint 是唯一 durable 幂等权威。

### Stage 5：Production Cutover

- ingress 创建 RequestLifecycle 并转移 RequestLease。
- execution 不再构造 request log outcome。
- LifecycleBody 接管 protocol/delivery terminal。
- LifecycleWriter 接管终结、retry、health 和 drain。
- 删除旧 dispatcher 和旧 production path。

退出条件：真实 Chat/Responses/Embeddings/Models 及本地 usage 走唯一新路径。

### Stage 6：删除旧架构

- 删除 `FinalRequestOutcome`、`FailedRequestContext`、`CandidateFeedback`、`ResponseMode`、伪 outcome 和旧 tests。
- 停写 `attempts_json`；保留一个受控读取兼容周期后删除字段。
- 删除 source-shaped contract tests，替换为行为和结构门禁。
- 重新跑 CodeGraph impact，证明没有新上帝对象。

退出条件：仓库中无第二套 lifecycle/feedback/completion authority。

### Stage 7：Release Gate

- 完整 proxy test、soak、fault、shutdown、restart、update drain。
- 真实客户端 stream + tools + reasoning 回归。
- 使用真实 DB 按 request_id 核对 request/attempt/health 三类事实。
- release build、signed Windows package 和升级路径验证。
- secret/artifact scan 和性能基准通过。

## 20. 测试矩阵

### 20.1 Request ownership

- authenticated request 在 auth 后分配统一计时起点，但只有 durable request-start ack 后才进入 `Admitted`；
- writer capacity/start commit 失败在 upstream 前返回 503，且不伪造 journal row；
- authenticated unknown route/method 经过同一 lifecycle 并产生唯一 terminal；
- body timeout/invalid JSON 有唯一 terminal；
- pre-commit failure 有唯一 terminal；
- buffered body 未被 poll 即 drop；
- stream chunk 后 drop；
- protocol completed 后 downstream drop；
- shutdown cancel；
- duplicate finalize；
- active_requests 在 body terminal 前保持 1。

### 20.2 Attempt feedback

- A 401、B 成功：A HardFail，B Success；
- A 429、B 成功：A Cooldown，B Success；
- A timeout、B 5xx、C 成功：A/B ObserveFailure，C Success；
- A request-only 400：A Neutral，request stop；
- post-commit upstream error：attempt ObserveFailure，request failed，无 fallback；
- protocol terminal 前 downstream drop：attempt Neutral；
- protocol terminal 后 downstream drop：attempt Success；
- models 多候选部分成功：逐 attempt feedback + request PartialSuccess；
- stale endpoint revision：attempt 可记录，但旧 health effect 被拒绝。

### 20.3 Protocol completion

- Responses completed/failed/incomplete；
- Responses terminal 前 EOF；
- Responses terminal 后 transport error；
- Chat `[DONE]`；
- Chat event-boundary EOF without `[DONE]`；
- Chat partial SSE event；
- Chat-to-Responses 不得在普通 EOF 合成 completed；
- buffered 200 malformed JSON；
- buffered error envelope；
- Embeddings/Models schema mismatch；
- SSE JSON 跨 chunk 和多 event 单 chunk；
- pending SSE buffer 上限。

### 20.4 Persistence/finalization

- request start/terminal CAS；
- StartRequest 始终先于同 request 的 FinishAttempt/FinishRequest；
- attempt durable ack 前不会启动下一 retry；
- attempt unique key；
- attempt+health transaction rollback；
- request terminal transaction rollback；
- transient busy retry；
- permanent writer unhealthy；
- queue saturation 在 upstream 前拒绝；
- models fan-out permit 不足时全部不启动，不产生半批结果；
- shutdown drain 无丢失；
- crash 后 stale in-progress recovery；
- restart 后 duplicate request/attempt idempotency；
- retention cascade 和 bounded query。

### 20.5 性能与 soak

- 32 并发 stream 不提前释放 admission；
- 64 connection/32 request 上限不回归；
- 10,000 request logs + 30,000 attempts 的 request detail 查询有界；
- attempt terminal 写入 p95 小于 100 ms；
- request-start 写入 p95 小于 100 ms，并分别报告无 attempt、单 attempt、三 attempt fallback 的 lifecycle 持久化附加延迟；
- runtime overlay apply 不做阻塞 I/O；
- 1 小时混合 buffered/stream/failure soak 后 request、attempt、body budget、queue、task 全归零。

## 21. 验收标准

升级只有同时满足以下条件才算完成：

1. Request、Attempt、Protocol、Delivery 各有唯一 owner 和 exhaustive state machine。
2. production 中不存在可被多层原地修改的 FinalRequestOutcome。
3. 一次请求的每个 candidate attempt 都有唯一 terminal record。
4. retry disposition 与 health effect 类型和实现均分离。
5. A 失败 B 成功会同时惩罚/冷却 A 并奖励 B，且不会重复反馈。
6. `/v1/models` 多候选聚合使用相同 attempt lifecycle。
7. 所有协议通过显式终态或完整 validated body 才能成功。
8. Chat-to-Responses 不会为无 terminal EOF 合成 `response.completed`。
9. downstream drop 不会错误惩罚未完成但未知的 upstream，也不会抹掉已确认的 upstream success。
10. request admission lease 持有到 response body 真正 terminal。
11. authenticated request 仅在 durable request-start ack 后成为 admitted；unknown route/method 不绕过 lifecycle。
12. request/attempt terminal 使用数据库唯一键/CAS 幂等，无永久内存 HashSet。
13. attempt+health 和 request terminal 事务分别原子且可 fault-inject，retry 等待前一 attempt durable ack。
14. finalization writer 使用单一有序 command channel，有界、可重试、可 drain、可观测，failure 会关闭 admission。
15. 无 AppDatabase 新方法、无 V1/V2 dual write、无第二 production SQL adapter。
16. `attempts_json` 不再是权威事实并按计划退役。
17. 新 endpoint/provider/health policy 可通过 sealed contract 扩展，不复制执行循环。
18. CodeGraph architecture fitness functions 通过，无新高 fan-in/fan-out 上帝对象。
19. 完整 Rust test、frontend contract、Cargo check/build、release、soak、fault、真实 E2E 和 SQLite 核对通过。
20. 所有错误、日志、attempt record 和诊断均不泄露 secret 或完整用户 payload。
21. 旧 lifecycle、feedback、completion production path 已删除，不保留永久兼容 facade。

## 22. 回滚策略

- Stage 0-4 不切换 production runtime，可直接回滚新 domain/store 模块和未激活 migration。
- Stage 5 cutover 必须在单独提交完成；回滚只能回到 cutover 前完整版本，禁止运行时双写或按请求随机选择旧/新 lifecycle。
- schema 已包含 request_attempts 时，旧 binary 可忽略新表但不得删除；migration 必须 forward-compatible 且有 released-schema fixture。
- 若 cutover 后 writer unhealthy、attempt transaction 或 protocol contract 发生阻断级故障，停止 proxy admission并给出脱敏诊断；不得自动回退旧 lifecycle 继续写入。
- 发布回滚遵循 persistence V2 binary/schema compatibility gate，不绕过 generation/tombstone 规则。

## 23. 明确禁止的反模式

- 给 `FinalRequestOutcome` 继续加字段或 boolean 修补非法组合。
- 新建一个持有 Router、DB、Reqwest、Axum、Scheduler 的 `LifecycleManager`。
- 只在最终 request completion 时批量反馈所有 attempts。
- 把所有 failed attempt 都无差别计为 key hard failure。
- 用 EOF 作为所有 streaming protocol 的通用成功条件。
- 用 HTTP 2xx 或首 chunk 作为 candidate success。
- 用 `output_started` 同时决定 retry 和 health。
- 在 response body drop 中直接写数据库或修改 station health。
- 让 persistence 根据错误字符串重新推导业务决策。
- 继续把 attempts 仅保存在 JSON blob。
- per-attempt/per-job 无界 spawn、无界 queue 或永久 dedupe set。
- 为过渡期永久保留旧 outcome facade、双写或 shadow comparison production path。
- 为支持一个异常站点而放宽所有 OpenAI-compatible provider 的协议合同。

## 24. 设计自审

- 三项 P0 被同一个所有权模型解决，不是三个局部补丁。
- 可维护性由窄 owner、typed state、单向依赖、删除旧权威和 architecture fitness functions 保证。
- 可扩展性由 sealed protocol/adapter、canonical attempt result 和 consumer-owned ports 保证，不依赖动态框架。
- 可靠性由逐 attempt terminal、协议完成合同、commit barrier、原子事务、有界 writer、drain、幂等和 fault tests 保证。
- 目标架构与 persistence V2 单一写权威兼容，不新增 AppDatabase 债务。
- 文档没有 TODO、TBD、未选择分支或“先兼容以后再说”的永久接口。
- 实施可以分 stage 审阅，但最终 production 只有一条 lifecycle、feedback、protocol 和 persistence 路径。
