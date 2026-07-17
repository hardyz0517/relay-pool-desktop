# Relay Pool 本地路由可靠性架构升级 Spec

日期：2026-07-17
状态：设计已确认，待实施计划
决策：采用成熟 Rust HTTP 运行时，并建设 Relay Pool 自有统一执行管线

## 1. 执行摘要

Relay Pool 当前本地路由不是简单缺少几个超时，而是基础设施边界错位：应用自行维护 `TcpListener`、HTTP/1.1 解析、连接线程、请求体读取、响应写回，同时把认证、端点分发、候选选择、fallback、协议转换、健康反馈和请求日志集中在 `runtime.rs`。这使每一次协议或路由修复都可能跨越多个失败域。

本升级不继续扩展手写 HTTP 栈。目标架构为：

```text
Tauri lifecycle
  -> bounded Hyper/Axum HTTP server
  -> ingress/auth/validation
  -> canonical request
  -> Relay Pool route planner
  -> unified execution engine
  -> endpoint adapter
  -> shared Reqwest upstream transport
  -> buffered or prepared streaming response
  -> outcome feedback + request log finalization
```

该方案借鉴 CC-Switch 的 Rust HTTP 运行时，借鉴 CLIProxyAPI 的统一执行、选择、翻译和 cooldown 边界，但不复制二者代码，也不引入 CPA 的插件宿主、OAuth 热加载、第二套凭据模型或全协议矩阵。Relay Pool 继续以 `Station -> Station Key -> capability/health/price/balance -> Scheduler` 为唯一领域主干。

## 2. 审计基线

### 2.1 Relay Pool 当前实现

审计时的关键事实：

- `src-tauri/src/services/proxy/runtime.rs` 为 6639 行，同时承担 lifecycle、listener、auth、dispatch、routing、forwarding、streaming、feedback 和 logging。
- `src-tauri/src/services/database.rs` 为 21366 行，包含本地路由候选读取及大量无关数据职责。
- 本地 server 使用非阻塞 `std::net::TcpListener`，接受连接后执行 `thread::spawn`。
- `read_http_request()` 把 accepted socket 改回 blocking，再调用基于 `httparse` 的自制 parser。
- 请求体限制为硬编码 `2 MiB`；生产路径没有完整的 header/body/write deadline、连接上限和全局 buffered-body 预算。
- upstream 使用同步 `ureq`；流式和非流式生命周期被迫与 OS 线程及手工 socket 写回耦合。
- fresh database 默认持久化占位 key `sk-local-pool-change-me`；CC-Switch 导入命令读取原始值，而运行时认证会生成新 key，存在首次导入后 401 的确定性合同缺陷。
- accepted socket 的 Windows `WouldBlock` 修复是必要回归，但它只修复了一个症状；继续围绕该 parser 加固会让项目长期承担 HTTP server 维护成本。

结论：当前 Router/Scheduler 领域模型可以保留；当前 transport、执行编排和文件边界不应继续作为长期框架。

### 2.2 CC-Switch 审计

审计快照：`farion1231/cc-switch@f6e37ed99443890a865669e28bf1caf5e85d466d`，MIT。

值得学习：

- `Tokio + Axum + Hyper + Reqwest` 承担异步 HTTP server/client、stream body 和连接生命周期。
- 生命周期具备 start、stop、graceful shutdown、运行状态和 runtime config 更新。
- 具有 first-byte、stream idle、buffered response 等不同超时概念。
- provider router、circuit breaker、request context、handler config 已形成可识别边界。
- request/response transform 与 streaming transform 有明确的 provider adapter 层。

不应照搬：

- 为保留 header 大小写而维护自定义 Hyper accept 细节；Relay Pool 没有该产品需求。
- `forwarder.rs`、`handlers.rs`、`services/proxy.rs` 已形成数千行巨型文件，不能作为模块组织模板。
- 其 provider 是客户端配置供应商，而 Relay Pool 的核心候选是带价格、余额和健康事实的 Station Key。
- `200 MiB` body limit 与 Relay Pool 的桌面资源预算不匹配。
- 成熟 HTTP 框架不自动提供业务并发上限，Relay Pool 必须自行定义 admission 和内存预算。

### 2.3 CLIProxyAPI 审计

审计快照：`router-for-me/CLIProxyAPI@106270bea6f18ba2f2cc8b0b5887987f2874eed8`，MIT。

值得学习：

- `Manager.Execute()` / `ExecuteStream()` 统一候选选择、executor 调用、失败反馈、cooldown 和 retry，而不是每个 endpoint 各写一套循环。
- Selector、ProviderExecutor、Translator Registry 和 inbound access manager 具有稳定接口。
- streaming bootstrap retry 与已提交 stream 的失败处理是不同阶段。
- credential refresh 使用 per-auth lock，避免并发刷新同一凭据。
- SDK/handler/selector/cooldown/translator 有大量专项测试；成熟度主要来自边界和测试，不是 Gin 这个框架名。

不应照搬：

- Go sidecar、FFI 或双进程部署；这会制造两套 lifecycle、配置、日志和故障排查链。
- CPA 的 auth 文件、OAuth refresh、provider plugin 和 management API；Relay Pool 已有 SecretManager、Station Key 和 Tauri command 边界。
- 任意 provider/plugin ABI 和全格式翻译矩阵；当前产品只承诺有限 OpenAI-compatible endpoints。
- Gin handler 中通过 context 注入大量动态插件能力；Relay Pool 应采用编译期明确、内部封闭的 adapter 集合。

## 3. 架构决策与反四不像约束

### 3.1 采用

- `Tokio + Hyper + Axum` 负责本地 HTTP/1.1 server 和 response body。
- `Reqwest` 作为唯一生产 upstream HTTP client，使用 Rustls，并保留 HTTP/SOCKS proxy 能力。
- 一条统一 `ExecutionEngine` 负责所有可路由 endpoint 的 candidate attempt lifecycle。
- 编译期 endpoint adapter，处理 endpoint metadata、上游路径和必要的 Responses/Chat bridge。
- Relay Pool 现有 Router/Scheduler、affinity、health、price、balance、concurrency 和 request-log 事实继续作为领域核心。
- SQLite 保持 Rusqlite，不在本升级引入异步 ORM 或第二数据库层。

### 3.2 不采用

- 不嵌入 CPA，不运行 Go sidecar，不引入 FFI。
- 不建设插件市场、动态 provider ABI、脚本 adapter 或 runtime-loaded translator。
- 不支持任意 forward proxy absolute-form request。
- 不扩大到 Files、Batches、Audio、Images、Realtime、Assistants 等 endpoint。
- 不为每个 endpoint 复制 scheduler、retry、feedback 和 logging 循环。
- 不把 transport 类型泄漏进 Router/Scheduler 领域对象。
- 不在本次升级改写价格排序、余额规则、group filter 或 Station Key 数据模型。

### 3.3 成熟度定义

本项目中的“成熟框架”必须同时满足：

1. HTTP framing、chunked transfer、keep-alive 和 disconnect 交给经过广泛使用的库。
2. 每个资源都有上限：连接、请求、header、单请求 body、全局 buffered body、等待、首字节和 stream idle。
3. retry 有明确安全边界和总预算。
4. stream 有不可逆的 downstream commit point。
5. lifecycle 可启动、停止、drain、超时强停和恢复失败状态。
6. 路由结果与传输结果在一次请求中只写入一次最终事实。
7. transport、routing、adapter、feedback 和 persistence 可以独立测试。

仅仅增加 Axum 依赖不算完成升级。

## 4. 目标模块边界

最终目标目录如下。实施期间允许短期存在 `legacy_runtime.rs`，切换完成后必须删除。

```text
src-tauri/src/services/proxy/
  mod.rs
  runtime.rs                 # Tauri-facing lifecycle state machine only
  server.rs                  # listener, Hyper connection serving, graceful shutdown
  limits.rs                  # immutable resource and timeout policy
  ingress.rs                 # auth, endpoint match, body collection, canonicalization
  execution.rs               # unified route plan and candidate attempt state machine
  upstream.rs                # shared Reqwest clients and upstream request/response I/O
  endpoint_adapter.rs        # sealed endpoint adapters and Responses/Chat bridge
  response_body.rs           # buffered/stream body, first chunk, completion/drop tracking
  routing_repository.rs      # proxy-owned DB reads/writes through existing AppDatabase
  error.rs                   # typed failure taxonomy and OpenAI-compatible rendering
  observability.rs           # request id, stage timing, sanitized diagnostic event
  router.rs                  # existing domain selection logic
  scheduler/                 # existing scheduler logic
  routing_*.rs               # existing routing facts, split only when touched
```

边界规则：

- `runtime.rs` 不得包含 endpoint path match、JSON transform 或 candidate fallback。
- `server.rs` 不得读取 SQLite、计算价格或选择 Station Key。
- `ingress.rs` 不得调用 upstream。
- `execution.rs` 不得解析 HTTP framing，也不得直接持有 `TcpStream`。
- `endpoint_adapter.rs` 不得选择 Station Key 或修改 health。
- `upstream.rs` 不得决定是否 retry；它只返回结构化 attempt outcome。
- `routing_repository.rs` 可以访问 decrypted Station Key，但 UI read model 和 error/log 类型不得包含 secret。
- 任一生产模块达到约 1000 行时，实施评审必须解释是否需要按职责继续拆分；不能只为满足行数做空壳文件。

## 5. 核心接口

以下类型名可在实施计划中做小幅 Rust 风格调整，但职责和依赖方向不可改变。

### 5.1 Server 配置

```rust
pub struct ProxyServerLimits {
    pub max_connections: usize,
    pub max_in_flight_requests: usize,
    pub max_header_bytes: usize,
    pub max_body_bytes: usize,
    pub max_buffered_body_bytes: usize,
    pub header_timeout: Duration,
    pub body_timeout: Duration,
    pub upstream_connect_timeout: Duration,
    pub upstream_first_byte_timeout: Duration,
    pub precommit_timeout: Duration,
    pub buffered_execution_timeout: Duration,
    pub stream_idle_timeout: Duration,
    pub shutdown_timeout: Duration,
}
```

首版固定默认值：

| 限制 | 默认值 | 原因 |
|---|---:|---|
| active connections | 64 | 防止 idle/slow client 无界占用任务 |
| in-flight requests | 32 | 本地桌面工具足够，超限快速 503 |
| header bytes | 64 KiB | 覆盖常见 OpenAI client headers |
| one request body | 32 MiB | 支持长文本、tools 和有限 base64 multimodal |
| aggregate buffered ingress | 128 MiB | 限制 32 个大请求同时进入的内存风险 |
| header timeout | 10 s | loopback header 不应长期占连接 |
| body timeout | 30 s | buffered request body 必须有终点 |
| upstream connect | 10 s | DNS/TCP/TLS 建连预算 |
| upstream first byte | 120 s/attempt | reasoning model 允许较长启动时间 |
| all-attempt pre-commit | 180 s/request | 限制多个候选累计首字节等待 |
| buffered execution total | 300 s/request | 非流式生成与 fallback 总预算 |
| stream idle | 90 s | 已开始 stream 的 chunk 空闲上限 |
| graceful shutdown | 30 s | 与 updater drain 现有合同一致 |

这些限制首版是 backend named constants，不增加 UI 高级设置。调整必须通过测试和实际测量，不从其他项目直接复制数值。

### 5.2 Canonical request

```rust
pub struct CanonicalProxyRequest {
    pub request_id: String,
    pub endpoint: RouteEndpointKind,
    pub model: Option<String>,
    pub stream: bool,
    pub requirements: RequestRequirements,
    pub body: bytes::Bytes,
    pub forwarded_headers: HeaderMap,
    pub idempotency_key: Option<String>,
    pub session_hash: Option<String>,
    pub previous_response_id: Option<String>,
}
```

只有 ingress 能从 Axum request 构造该类型。进入 execution 后不再读取 raw inbound request。

实际结构必须私有持有一个 `BodyBudgetLease`，其权重覆盖 `body` 当前占用；lease 随 canonical request、upstream send body 和任何必要的 retry clone 一起存活，最后一个 owner drop 时归还全局预算。不得只在读取完成时提前释放预算。

`forwarded_headers` 使用 allowlist：`content-type`、`accept`、`openai-organization`、`openai-project`、`idempotency-key` 以及已确认需要的客户端版本/feature headers。必须移除 inbound authorization、host、connection、content-length、transfer-encoding 和 proxy headers，上游 authorization 只能由 backend 注入。

### 5.3 Route plan

```rust
pub struct RoutePlan {
    pub candidates: Vec<PlannedCandidate>,
    pub explanations: Vec<RouteCandidateExplanation>,
    pub mapped_model: Option<String>,
    pub wait_decision: Option<RouteWaitDecision>,
}
```

现有 Router/Scheduler 负责生成 `RoutePlan`。Execution Engine 不重新排序候选，只应用 attempt budget、失败分类和 stream commit 规则。

DB candidate 读取通过 `RoutingRepository` 完成。所有 Rusqlite 调用在 `spawn_blocking` 中执行；不得持有 SQLite mutex 跨越 `.await`。

### 5.4 Unified Execution Engine

```rust
pub async fn execute(
    context: &ExecutionContext,
    request: CanonicalProxyRequest,
) -> Result<ProxyExecutionResponse, ProxyFailure>;
```

`execute()` 对 Models、Chat Completions、Responses 和 Embeddings 使用同一生命周期：

1. 获取不可变 request config snapshot。
2. 从 repository 读取一次候选事实。
3. 调用现有 Router/Scheduler 得到 ordered plan。
4. 应用已有 sticky/fallback concurrency wait 规则。
5. 按顺序尝试候选；每次通过 endpoint adapter 生成 upstream request。
6. upstream 返回 buffered response、prepared stream 或 typed failure。
7. RetryPolicy 根据失败阶段、HTTP status、idempotency 和剩余预算决定 stop/failover。
8. 在成功响应完成、stream 完成、downstream drop 或终止失败时只 finalize 一次 outcome。

`/usage` 和 `/v1/usage` 是 local read endpoint，不进入 candidate attempt loop，但仍经过 admission、request id、auth、error rendering 和 completion logging。

### 5.5 Endpoint adapter

adapter 集合是 crate-private、编译期封闭的，不提供插件注册 API：

- `ModelsAdapter`
- `ChatCompletionsAdapter`
- `ResponsesAdapter`
- `EmbeddingsAdapter`
- `ResponsesToChatBridge`

adapter 负责：

- endpoint-specific metadata extraction；
- mapped model 写入 request body；
- 上游 path 和 `Accept` 选择；
- Responses direct 与明确允许的 Chat fallback transform；
- buffered response transform；
- 必要的 streaming event transform。

adapter 不负责：候选排序、cooldown、attempt 次数、DB 写入、secret 获取或 request-log finalization。

## 6. HTTP Server 与并发模型

### 6.1 Runtime 依赖

实施时以兼容 Cargo lock 的当前稳定 minor 为准，主版本固定为：

- Tokio 1
- Axum 0.8
- Hyper 1
- Hyper-util 0.1
- Tower 0.5
- Tower-http 0.6
- Reqwest 0.12，`rustls-tls`、`stream`、`json`、`socks`
- Bytes 1、HTTP-body-util 0.1、Futures-util 0.3
- Tokio-util 0.7，用于 cancellation token
- Subtle 2，用于固定时间 credential 比较

迁移完成后，生产 proxy 路径移除 `httparse` 和 `ureq`。如果 collector 仍使用 `ureq`，只保留 collector 所需依赖；不得维持两套 proxy upstream client。

### 6.2 Connection serving

`server.rs` 可以保留一个小型 Tokio accept loop，用途仅限：

- acquire connection semaphore；
- accept loopback connection；
- 配置 Hyper HTTP/1.1 header timeout 和 keep-alive；
- 把 connection 交给 Axum router；
- 监听 cancellation token 并 graceful shutdown。

HTTP parsing、chunked decoding、keep-alive framing 和 body polling 必须由 Hyper 完成。禁止读取 raw bytes 后自行寻找 `\r\n\r\n`。

连接 permit 从 accept 前持有到 Hyper connection future 完成。请求 permit 从 ingress admission 持有到 response body 完成或 drop；streaming handler 返回 response 后不能提前释放。`ProxyStatus.active_requests` 保持“从通过 admission 到下游 body 结束”的语义。

### 6.3 Body budget

请求体仍采用 buffered ingress，因为现有路由和 adapter 需要读取 JSON metadata。buffer 必须同时受两个限制：

- 单请求最多 32 MiB；
- 所有正在收集/持有的 inbound body 合计最多 128 MiB。

全局预算按 KiB 计权 semaphore 管理。已知 `Content-Length` 在读取前预留；未知长度的 chunked body 随读取增量申请。超出单请求限制返回 413，无法获取全局预算返回 503 `local_proxy_memory_busy`，不得等待到系统内存耗尽。

### 6.4 Shared upstream clients

不得每次请求创建 Reqwest client。`UpstreamClientPool` 按当前网络出口配置持有复用 client：

- direct client；
- HTTP proxy client；
- SOCKS proxy client。

配置变更构造新 client snapshot，已有请求继续使用旧 `Arc` 直至结束。client pool 不包含 Station Key secret；secret 在 attempt 构造 request 时短暂解析并注入 header。

## 7. Lifecycle 状态机

```text
Stopped -> Starting -> Running -> Draining -> Stopping -> Stopped
             |            |          |            |
             +----------> Failed <---+------------+
```

状态合同：

- `start` 对相同 bind config 幂等；不同 port 的 start 必须走 restart，不可启动第二个 server。
- `Starting` 完成 bind 后才发布 `Running`，bind 失败进入 `Failed` 并保留 sanitized error。
- `Draining` 停止接收新 HTTP 请求，已经进入 execution 的请求继续；Tauri status command 必须继续反映 drain 进度。
- drain 等待 `active_requests == 0`，最长 30 秒。
- 超时后取消 remaining response bodies，关闭 listener，并把 forced-shutdown count 写入诊断状态。
- `stop` 必须等待 listener task 和已跟踪 connection task；不能只翻转 boolean。
- panic/JoinError 必须进入 `Failed`，UI status 不得继续显示 running。
- updater cleanup、普通 stop、port restart 共用同一 shutdown primitive。

Tauri command 名称和 TypeScript invoke shape 保持兼容；命令内部允许改成 async。禁止创建独立于 Tauri 的永久第二 Tokio runtime。

## 8. 本地认证合同

### 8.1 单一 key 生命周期

`sk-local-pool-change-me` 只能作为 migration sentinel，绝不能离开 backend 成为可用 credential。

所有“将 key 用于运行、显示、复制或导出”的入口统一调用：

```rust
ensure_secure_local_access_key()
```

至少包括：

- proxy start/config snapshot；
- `import_relay_pool_to_ccswitch`；
- 设置页复制/重新生成 key；
- 未来任何 deeplink/export。

`get_local_access_key()` 只允许 migration/test/内部检查使用，并应改为非 public 或更明确的 raw 名称。

运行时不在每个请求中访问 SQLite 以确保 key。server start 时确保并装载 key snapshot；key 更新后通过 runtime config reload 原子替换。比较使用固定时间比较，日志永远不包含 key、header 或 key hash。

### 8.2 Inbound auth

保持当前已支持 credential source，不随架构迁移扩展 query-string key。缺失与错误 credential 均返回 401，但内部 code 分别为：

- `local_auth_missing`
- `local_auth_invalid`

外部 JSON 保持 OpenAI-compatible error envelope。CORS preflight 不携带 secret，不触发 upstream；实际 API 请求必须认证。

server 只绑定 loopback。任何允许 LAN bind 的未来需求必须单独安全设计，不通过修改默认 address 顺带开放。

## 9. Retry、Fallback 与流式提交规则

### 9.1 Attempt 阶段

```text
Planned -> AcquiringCandidate -> Sending -> AwaitingHeaders
  -> BufferedReading -> Prepared
  -> StreamBootstrapping -> Prepared
  -> Failed
```

`Prepared` 表示 Execution Engine 已决定返回该候选。对于 buffered response，它已有完整 body；对于 stream，它至少已有成功 status、有效 headers 和第一个非空 upstream chunk/event。

### 9.2 Downstream commit point

streaming 请求只有在取得首个可转发 chunk 后才构造并返回 Axum response。handler 返回 response 即视为 committed：

- commit 前失败可以按 RetryPolicy 尝试下一候选；
- commit 后断流不得换候选、不得伪装为 non-stream response、不得重放完整请求；
- commit 后失败只终止 body、记录 `upstream_stream` 或 `downstream_write` outcome，并更新实际 candidate health。

这条规则同时适用于 direct Responses、Responses-to-Chat bridge 和 Chat streaming。

### 9.3 Retry 分类

| 失败 | 默认动作 | 说明 |
|---|---|---|
| local parse/auth/admission | stop | 尚未进入 routing |
| no eligible candidate | stop | 返回完整 rejection summary |
| DNS/TCP/TLS 建连前失败 | failover | 未发送 request body，安全 |
| upstream 401/403 | failover | candidate credential failure，记录该 key |
| upstream 404 capability/endpoint mismatch | conditional failover | 只在 adapter 明确分类时 |
| upstream 408/425/429/5xx | failover | 写入 cooldown 后立即尝试 ready candidate |
| generic 400/409/422 | stop | 除已列出的 Responses bridge signature |
| header timeout/reset after body may be sent | stop without idempotency key | 避免潜在重复计费 |
| same ambiguous failure with idempotency key | conditional failover | 上游必须收到相同 key |
| stream bootstrap failure before commit | failover | 尚未向客户端提交 |
| stream failure after commit | stop stream | 绝不换 candidate |

首版 `max_candidate_attempts = min(3, eligible_candidates)`。同一请求不重复尝试同一 candidate。`Retry-After` 用于该 candidate 的 cooldown；存在 ready candidate 时不原地等待。只有现有 sticky/fallback wait policy 明确允许等待且不超过剩余 pre-commit/buffered budget 时才等待，否则继续下一候选或终止。

现有 sticky/fallback concurrency wait 在 attempt 前执行，并保留原配置语义。等待时间、attempt 时间和 stream 时间分别记录，避免日志只显示一个无法解释的总耗时。

## 10. 错误模型与可观测性

### 10.1 Typed failure

```rust
pub struct ProxyFailure {
    pub code: ProxyFailureCode,
    pub source: FailureSource,
    pub retry_class: RetryClass,
    pub http_status: StatusCode,
    pub public_message: String,
    pub internal_detail: Option<String>,
    pub candidate_id: Option<String>,
}
```

`internal_detail` 在写入前必须 sanitize，不返回客户端。`candidate_id` 是 Station Key id，不是 secret。

最小稳定 code 集：

- `local_proxy_busy`
- `local_proxy_memory_busy`
- `request_header_timeout`
- `request_header_too_large`
- `request_body_timeout`
- `request_body_too_large`
- `request_body_invalid`
- `local_auth_missing`
- `local_auth_invalid`
- `route_no_candidate`
- `route_wait_timeout`
- `upstream_connect_failed`
- `upstream_first_byte_timeout`
- `upstream_http_error`
- `upstream_stream_failed`
- `downstream_disconnected`
- `application_update_in_progress`
- `internal_proxy_error`

从 Axum service 收到 request 后，以上 code 必须进入 OpenAI-compatible JSON envelope。Hyper 在完整 request head 产生前发现的 malformed header、header timeout 或 header overflow 属于 transport-level failure：允许由 Hyper 返回标准 4xx 或直接关闭连接，只要求 server diagnostic 记录分类，不承诺自定义 JSON、request id 或 response header。

### 10.2 Request trace

每个请求生成 `request_id`，并在响应 header 返回 `x-relay-request-id`。请求日志保存：

- request id、method、normalized path、model、stream；
- body byte length，不保存 body；
- route wait、candidate count、attempt count；
- selected Station/Station Key id 与 endpoint revision；
- 每次 attempt 的 sanitized failure code 和 duration；
- time to upstream headers、time to first downstream byte、total duration；
- completion source：buffered success、stream completed、upstream stream failed、downstream dropped；
- usage/cost 和现有 route reason。

禁止保存：authorization、API key、cookie、完整 request/response body、proxy credential、decrypted Station Key。

### 10.3 Finalize once

request outcome 使用一次性 finalizer。buffered response 在 write completion 后 finalize；stream body wrapper 在 EOF、error 或 drop 时 finalize。health、scheduler feedback、request log 和 active-request decrement 必须由同一个终态触发，不能由 handler 和 stream task 分别重复写入。

## 11. 数据与配置边界

`routing_repository.rs` 从 `database.rs` 提取以下 proxy-owned 操作：

- 加载 runtime route candidates；
- 解析 encrypted Station Key；
- 加载 routing settings/config snapshot；
- 写入 route health/feedback；
- 写入最终 request log。

首版 repository 仍包裹 `AppDatabase`，不引入 generic repository framework。UI candidate reads 与 runtime secret reads 使用不同返回类型；任何 UI/read model 类型在编译结构上都不能包含 decrypted secret 字段。

capability 缺失语义必须在迁移前冻结成单一函数，不允许 SQL `COALESCE`、Rust default 和 UI default 各自定义。当前 dirty worktree 中“缺失 capability 全部默认为 true”的变更属于独立行为决策；实施计划必须先验证并明确其产品语义，不能被 transport 重构顺带吞并。

## 12. 迁移策略

迁移采用可回退的 strangler 方式，但不长期维护双栈。

### Phase 0：立即修复 key 合同

- CC-Switch import 改用 ensuring key path。
- fresh DB 回归证明 deeplink key、persisted key 和 runtime auth key 相同。
- 这是独立、小范围修复，不等待 server 迁移。

退出门槛：fresh DB 导入后 `/v1/models` 不因 placeholder mismatch 返回 401。

### Phase 1：冻结行为合同

- 为现有 endpoint、auth、route reason、fallback、SSE、usage、update drain 编写 characterization tests。
- 建立 loopback fixture，覆盖 delayed body、chunked body、disconnect、slow body、stream idle 和 malformed upstream。
- 将测试断言放在公开行为和 typed outcome，不锁定旧函数结构。

退出门槛：旧 runtime 的当前承诺可由自动化测试描述；已知 bug 以 failing regression 表达。

### Phase 2：引入 async transport skeleton

- 添加依赖、`limits.rs`、`error.rs`、`server.rs`、`ingress.rs`。
- server 先提供 auth、usage 和受控 404，不接真实 upstream routing；lifecycle 通过 Tauri/test control API 验证。
- runtime selector 由 test harness 直接注入；仅 live dev 启动允许使用 `RELAY_POOL_PROXY_RUNTIME=legacy|v2`，不增加 UI 开关，也不让并行测试修改全局环境变量。

退出门槛：v2 server 通过 HTTP framing、limits、auth、lifecycle 和 drain tests。

### Phase 3：提取统一执行领域

- 提取 canonical request、RoutePlan bridge、RetryPolicy、attempt outcome 和 once finalizer。
- legacy path 复用纯 routing/retry 分类函数，避免新旧语义继续分叉。
- DB 调用迁入 `routing_repository.rs` 并使用 `spawn_blocking`。

退出门槛：Execution Engine 纯状态机测试覆盖所有 retry/commit 分支，Router/Scheduler 现有测试保持通过。

### Phase 4：迁移 buffered endpoints

- 依次迁移 Models、Embeddings、Chat non-stream、Responses non-stream。
- 引入 shared Reqwest client pool。
- 新旧 runtime 对同一 fixture 做 differential assertions：status、error code、selected candidate、route reason、upstream path/body 和 feedback。

退出门槛：buffered endpoint parity suite 全绿，且不再通过 legacy socket writer 返回 buffered response。

### Phase 5：迁移 streaming

- 实现 stream bootstrap、prefetched first chunk、idle timeout、drop detection 和 once finalizer。
- 依次迁移 Chat stream、Responses direct stream、Responses-to-Chat stream bridge。
- 明确验证 commit 后不 fallback。

退出门槛：真实 loopback client 可逐块收到数据；首字节前 failover 和首字节后终止均符合合同。

### Phase 6：切换默认并观察

- dev/test 默认 v2，legacy 仅通过 test/dev env 回退。
- 运行完整 Cargo、contract、frontend build 和 live Tauri/CC-Switch 验证。
- 收集 request id、error code、first-byte 和 stream completion 证据，不采集内容。

退出门槛：连续完成规定 soak matrix，无 crash、hang、重复 finalization 或 secret leak。

### Phase 7：删除 legacy

- 删除 `TcpListener + httparse` production path、raw HTTP writer 和 proxy 内 `ureq`。
- 删除 runtime selector 和临时 differential harness，只保留长期 contract tests。
- `runtime.rs` 必须只剩 lifecycle/control plane。

退出门槛：仓库中 production proxy 不再引用 `httparse`、`std::net::TcpListener`、`thread::spawn` 或 `ureq`。

## 13. 回滚策略

- Phase 2-5 的 v2 默认关闭；任何 parity blocker 可继续使用 legacy，不需要回滚 DB。
- Phase 6 在一个发布周期内保留仅环境变量可用的 legacy 回退，不在 UI 暴露，避免形成永久双架构。
- 本升级不得引入不可逆 DB schema migration；新增 request diagnostic 字段必须 nullable 或写入现有扩展 JSON/可兼容列。
- 如果 v2 出现 blocker，回滚只切换 runtime selector；不能恢复 placeholder key 导出或撤销已通过的安全修复。
- Phase 7 删除 legacy 前，必须已经发布并验证至少一个默认 v2 版本；具体版本号由实施时 release plan 决定，不在 spec 中预设。

## 14. 测试与验收矩阵

### 14.1 单元测试

- RetryPolicy 对每种 failure source、status、idempotency 和 commit state 的决定。
- RoutePlan 不被 Execution Engine 重排。
- endpoint adapter model/path/header/body transform。
- Responses-to-Chat buffered/stream event transform。
- fixed-time local key comparison及 placeholder rotation。
- body budget acquire/release，包括 body read error 和 cancellation。
- once finalizer 在 EOF、error、drop、panic cleanup 中最多执行一次。

### 14.2 Router service tests

使用 Tower `oneshot` 或等价方式验证：

- 所有允许 method/path；
- unsupported endpoint 为明确 404/405；
- CORS preflight；
- missing/wrong/correct auth；
- 32 MiB body boundary；
- global body budget 503；
- in-flight request admission 503；
- OpenAI-compatible error envelope 和 `x-relay-request-id`。

最后两项只适用于已经进入 Axum service 的 request；pre-parse Hyper failure 按 10.1 的 transport-level 合同断言。

### 14.3 Loopback integration tests

- headers 和 body 分段到达；
- `Content-Length`、chunked 和 keep-alive；
- slow header/body timeout；
- client body 中途断开；
- upstream connect failure、TLS failure、429 Retry-After、500、malformed JSON；
- non-stream timeout；
- stream 首 chunk 延迟、chunk 间 idle、上游半途断开；
- downstream client 不读取或主动断开；
- graceful drain 等待 active stream；
- forced shutdown 取消超时 stream；
- Windows 上不再复现 `os error 10035`。

### 14.4 Routing invariants

- capability/health/cooldown/group/multiplier/balance gate 的候选顺序不因 transport 迁移改变；
- Station Key max concurrency permit 始终释放；
- endpoint revision 仍阻止 stale feedback；
- candidate 401/403/429/5xx 只更新实际 candidate；
- success 只有在 buffered downstream write 或 stream EOF 后记录；
- committed stream failure 不选择第二 candidate；
- request log、health feedback 和 active count 不重复 finalize。

### 14.5 Live acceptance

1. 使用 fresh database 启动 Tauri app。
2. 导入 Relay Pool 到 CC-Switch，确认 provider key 非 placeholder。
3. 通过 CC-Switch 调用 `/v1/models`、一次 Chat、一次 Responses stream、一次 Embeddings。
4. 使用 Codex client 完成至少一次 tool call + reasoning Responses stream。
5. 验证响应逐块到达，没有静默 non-stream fallback。
6. 触发一个 candidate 失败，确认 fallback、request log 和 health 指向同一 Station Key。
7. 流式响应中触发 app update drain，确认等待、超时和 UI status 一致。
8. 检查日志与 SQLite，不含完整 local key、Station Key、authorization 或 request body。

## 15. 性能与可靠性门槛

在 release build、loopback fixture、关闭 debug logging 的条件下：

- `/v1/models` 本地框架额外 p95 延迟小于 10 ms，不含 DB/upstream 时间。
- 1 MiB buffered request 的 ingress + canonicalization p95 小于 25 ms。
- streaming proxy 不聚合完整 response；首 chunk 之后内存随总 stream 长度保持有界。
- 32 个并发普通请求不创建 32 个 OS connection worker thread。
- 64 个 active connections 之外不产生无界 Tokio task 或内存增长。
- 1000 次短请求后 active connection、request 和 body-budget counters 回到 0。
- 100 次 client disconnect/stream abort 后无 permit leak、无重复日志、无 panic。
- graceful stop 在无 active request 时 1 秒内完成；有 active request 时遵循 30 秒 drain 上限。

这些是验收门槛，不是营销 benchmark。测试环境差异导致绝对时间不可稳定时，必须保留资源计数和无泄漏门槛，并记录实测值。

## 16. 安全、许可证与归因

- CC-Switch 和 CLIProxyAPI 审计快照均为 MIT；可学习架构并独立实现。
- 不复制整段核心实现；如果实施中采用可识别的算法或代码片段，必须保留对应 MIT notice 并在 `THIRD_PARTY_NOTICES` 或现有归因文档记录。
- 新增依赖必须执行 license audit；不引入与项目分发目标冲突的 copyleft runtime dependency。
- server 继续默认只绑定 `127.0.0.1`。
- error、trace、test fixture 和 screenshot 均不得包含真实 credential。
- upstream redirect 默认关闭或限制到同 origin；authorization 不得跨 origin 自动转发。
- response header 采用 allowlist，移除 hop-by-hop、set-cookie 和不安全代理 headers。

## 17. 完成定义

只有同时满足以下条件，本升级才算完成：

- fresh CC-Switch import 首次可用，不导出 placeholder key。
- production local server 不再手写 HTTP framing/parser。
- 所有 endpoint 共用一条 Execution Engine attempt lifecycle。
- buffered 与 streaming retry/commit 合同都有自动化回归。
- connection/request/body/time budgets 均有硬上限及释放测试。
- runtime lifecycle 支持真实 graceful drain 和失败状态。
- `runtime.rs` 不再包含路由 endpoint 实现；`database.rs` 不再包含 proxy candidate 查询主体。
- legacy runtime、临时 selector 和 proxy 内 `httparse`/`ureq` 已删除。
- Cargo tests/check、frontend contracts/build、live Tauri + CC-Switch matrix 全部通过。
- request log 和诊断字段能回答“在哪一层、哪一个 candidate、哪个阶段失败”，且不泄露 secret。

## 18. 明确非目标

- 替代 CC-Switch 或 CPA。
- 支持 Claude/Gemini 原生 endpoint 全矩阵。
- 动态插件或第三方 adapter SDK。
- HTTP/2 inbound、WebSocket、Realtime 或 request-body streaming。
- 重写 Scheduler 评分、价格系统或余额采集。
- 引入异步数据库、微服务、sidecar 或云控制面。
- 在本次升级中顺带重构所有 collector/database 代码。
