# 状态监控功能重构升级规范

状态：Draft，已完成一致性审阅，待实施<br>
日期：2026-07-29<br>
适用范围：Relay Pool Desktop 状态监控、主动探针、渠道健康、路由健康联动与状态监控 UI<br>
替代关系：本规范进入实施后，取代 2026-07-05 的 Channel Monitoring Design 作为状态监控升级依据；旧文档继续作为历史记录保留<br>
参考项目：Relay Pulse，审阅提交 `c62537085f4202f6f1f28716f45c107303f2836f`，MIT License

## 1. 执行摘要

本次升级不是 UI 调整，也不是在现有监控实现上追加若干状态判断。它是一次监控领域的系统性重构，目标是建立可长期维护的主动探测、调度、健康事实、时间序列和桌面端状态工作区。

明确决策如下：

1. 保留已经成熟的公共基础设施：Tauri IPC 边界、SecretManager、Reqwest/Tokio 网络栈、Persistence Runtime、SQLite 事务、React Query 和现有 Station/Station Key 所有权模型。
2. 重构监控内核：协议适配、请求画像、语义校验、执行模型、调度器、健康写回、时间桶、保留策略和状态 UI。
3. 手动执行和定时执行必须经过同一个 orchestrator，不允许继续存在行为不一致的双路径。
4. 一个 HTTP 2xx 响应不等于模型可用。成功必须通过协议解析和内容语义校验。
5. 一次触发必须有一个父级 `MonitorExecution`；每个 Key 形成唯一终态 `MonitorTargetResult`，模型选择和重试形成其下的独立 `ProbeAttempt`。
6. `station_key_health` 继续是路由和 UI 共同消费的健康摘要，不创建第二套互相冲突的健康宇宙。
7. OpenAI、Anthropic、Gemini、xAI/Grok 使用独立、类型化的协议适配器；OpenAI-compatible 只作为明确的方言和兜底能力。
8. CLI 请求画像是正式能力，但必须可选、版本化、受控和可审计。不得把某个 CLI 版本的请求样本硬编码成永久协议。
9. 24 小时、7 天和 30 天趋势必须来自固定时间桶，前端不得根据总成功率伪造趋势格。
10. 新架构切换完成后删除旧 runner、旧 run 模型、旧状态拼装和失效配置，不长期维护两套实现。

## 2. 背景与问题陈述

当前实现具备服务边界、持久化端口、异步取消、共享出站客户端、SQLite 索引和 React Query 页面活动控制等良好骨架，但核心监控语义仍不足以作为可靠健康事实：

- 仅以 HTTP 状态码小于 400 判断成功，可能将空响应、HTML、错误 JSON、伪造 200 和无模型输出的 SSE 标为成功。
- `consecutive_failure_threshold` 等配置被保存但没有进入统一健康状态机。
- 手动执行绕过后台 single-flight guard。
- 30 秒轮询加逐 monitor 串行执行会产生调度漂移。
- station-wide 运行没有父 execution，子结果逐条提交并覆盖 monitor 的 latest 状态。
- `fallback_models` 第一项实际充当主模型，其余模型没有真正 fallback。
- `skipped` 被计入可用率分母。
- 24 小时和 7 天只有窗口汇总，没有可信固定桶时间线。
- 历史记录没有保留上限。
- 状态和错误原因使用弱约束字符串。
- 默认模板种植与前端默认模板 ID 没有形成可靠启动契约。
- 当前大卡片网格不适合横向比较、筛选、排序和固定桶趋势展示。

这些问题不能通过局部补丁解决，因为它们跨越领域模型、执行边界、持久化语义和 UI read model。继续修补会扩大隐式耦合并延长旧架构寿命。

## 3. 目标

### 3.1 产品目标

- 用户能够快速判断某个 Station Key、协议和模型当前是否真正可用。
- 用户能够区分鉴权、限流、网络、超时、协议不匹配、内容不匹配和慢响应。
- 用户能够查看近 60 次、24 小时、7 天和 30 天的可信趋势。
- 用户能够选择标准 API 或受控 CLI compatibility profile。
- 用户能够控制探测频率、并发、重试和健康写回策略，降低上游账号风险。
- 主动探测能够向路由健康提供可信事实，但不会因错误探针配置误伤正常路由。
- 状态监控页面保持浅色、紧凑、高密度的本地桌面工具体验。

### 3.2 工程目标

- 领域类型表达核心状态，避免跨层传递未校验字符串和任意 JSON。
- 协议、客户端画像、认证、传输和验证策略相互解耦。
- 所有执行可取消、可恢复、可审计且具有明确终态。
- 调度器按最近到期时间唤醒，具备全局并发、目标级限流和 single-flight。
- 写模型与读模型分离，UI 消费稳定 workspace DTO。
- 数据库历史有界，迁移可验证，失败时不破坏旧数据。
- 新增协议和 profile 不需要修改调度器、持久化事务或 UI 核心逻辑。

## 4. 非目标

本次升级不包含：

- 公开互联网状态页、SaaS 多租户、账号系统、团队权限或云同步。
- 邮件、短信、Webhook 或第三方告警平台。
- 替代真实请求日志或路由器的完整可观测性。
- 自动破解或绕过上游认证、风控、设备绑定和服务条款。
- 伪造 OAuth 身份、账号 UUID、官方设备证明或不存在的授权范围。
- 将所有未知厂商协议都塞进无约束 custom HTTP 模板。
- 复制 Relay Pulse 的深色视觉、公开网站 API、PostgreSQL 多实例锁或服务端部署模型。

## 5. 设计原则与不变量

### 5.1 单一健康事实

- `Station Key` 是路由健康目标。
- 主动监控产生 `HealthObservation`，由统一 `HealthTransitionService` 更新 `station_key_health`。
- 代理真实流量、连通性测试和主动监控不得分别维护互相覆盖的状态机。
- 每条健康变化必须记录来源、原因、观测时间和关联 execution。

### 5.2 HTTP 成功不等于语义成功

一次 attempt 只有同时满足以下条件才可成为 `available`：

1. 请求成功发送并在截止时间内完成。
2. HTTP 状态属于协议允许范围。
3. 响应 Content-Type 或流式 framing 可解析。
4. 协议结束事件完整，未收到协议级 error。
5. 提取到有效模型文本或协议定义的等价输出。
6. 内容 validator 通过。

任何一步失败都必须得到结构化 `FailureKind`。

### 5.3 配置快照

- execution 启动后使用不可变配置快照。
- monitor 在运行中被编辑，不改变已经开始的 execution。
- execution 保存配置版本、profile 版本和 request profile hash，不保存秘密。
- endpoint revision 变化后，旧 execution 不得写回当前健康。

### 5.4 有界资源

- 全局任务并发、单 Station 并发、单 Key 并发、响应读取大小、队列长度和历史保留都必须有界。
- 手动执行也受目标级限流和 single-flight 约束。
- 取消和应用退出必须有超时与明确终态。

### 5.5 安全默认

- API Key 只在请求发送前通过 SecretManager 解析。
- 认证 Header 不进入 profile 持久化、日志或 DTO。
- 默认不保存上游响应正文、prompt、expected answer 或完整请求 Header。
- 自定义 profile 不能覆盖认证、目标主机、安全限制和脱敏策略。

## 6. 术语

| 术语 | 定义 |
|---|---|
| Monitor Definition | 用户配置的监控定义，描述目标、协议、模型、周期和健康策略 |
| Monitor Execution | 一次手动、定时或恢复触发的父级执行 |
| Probe Target | execution 中的一个具体 Station Key |
| Monitor Target Result | 一个 Probe Target 在 execution 内的唯一终态结论，是统计与健康写回的事实来源 |
| Probe Attempt | 对一个 Key、模型和重试序号的一次真实网络尝试 |
| Protocol Adapter | 负责端点、请求结构、响应解析和错误映射 |
| Client Profile | 负责标准 API 或 CLI compatibility 请求画像 |
| Auth Strategy | 负责将秘密放入正确的认证 Header 或参数 |
| Transport Profile | 负责代理、连接复用、超时和 warm/cold 测量 |
| Validation Strategy | 负责判断模型是否返回预期语义内容 |
| Health Observation | target result 或真实代理事实转换出的路由健康证据 |
| Status Bucket | 固定时间范围内的统计结果 |

## 7. 目标架构

### 7.1 分层

```text
UI / React Query
    |
Tauri Commands + DTO
    |
Application Commands / Queries
    |
MonitorOrchestrator ---- MonitorScheduler
    |
ProbePlanner
    |
ProtocolAdapter + ClientProfile + AuthStrategy
    |
ProbeTransport / Reqwest
    |
ResultRecorder ---- HealthTransitionService
    |
SQLite Write Model ---- Status Read Model / Buckets
```

### 7.2 依赖方向

- 纯监控领域层（沿用仓库约定落在 `models/monitoring`）不依赖 Tauri、SQLx、Reqwest 或 React DTO。
- Application 只依赖 domain 和 ports。
- Protocol adapters 可以依赖通用 request/response domain，但不依赖持久化。
- Persistence 实现 repository ports，不决定健康业务规则。
- UI 不解析 raw run，不自行合并 request logs、health 和 monitor history。
- 所有 workspace 聚合在后端 query 层完成。

### 7.3 建议模块

```text
src-tauri/src/models/monitoring/
  definition.rs
  execution.rs
  outcome.rs
  policy.rs

src-tauri/src/application/monitoring/
  commands.rs
  queries.rs
  orchestrator.rs
  planner.rs
  recorder.rs
  health_bridge.rs

src-tauri/src/services/monitoring/
  scheduler.rs
  transport.rs
  challenge.rs
  adapters/
    openai_chat.rs
    openai_responses.rs
    anthropic_messages.rs
    gemini_native.rs
    xai_grok.rs
    generic_openai.rs
  profiles/
    standard.rs
    codex_cli.rs
    claude_code.rs
    gemini_cli.rs

src-tauri/src/persistence/stores/
  monitor_definition_store.rs
  monitor_execution_store.rs
  monitor_status_query_store.rs
  monitor_retention_store.rs
```

实际文件拆分可以服从现有模块边界，但职责不得重新合并成单个超大 `mod.rs`。

## 8. 领域模型

### 8.1 MonitorDefinition

`MonitorDefinition` 表示稳定配置，不承载运行状态。

核心字段：

| 字段 | 说明 |
|---|---|
| `id`、`name` | 稳定身份与名称 |
| `target_scope` | `station_key` 或 `station` |
| `station_id`、`station_key_id` | 所属目标 |
| `protocol_kind` | 类型化协议 |
| `template_id` | 可选的受控请求模板 |
| `client_profile_id`、`client_profile_version` | 请求画像 |
| `primary_model` | 主探测模型 |
| `fallback_models` | 有序备用模型 |
| `enabled` | 是否调度 |
| `interval_seconds`、`jitter_seconds` | 调度周期 |
| `timeout_seconds` | execution 总超时 |
| `attempt_timeout_seconds` | 单次 attempt 超时 |
| `slow_latency_threshold_ms` | 慢响应阈值 |
| `retry_policy` | 重试策略 |
| `concurrency_policy` | station-wide 扩展策略 |
| `risk_policy` | 频率、日预算和账号保护 |
| `health_policy` | 阈值、恢复和写回策略 |
| `transport_mode` | `warm` 或 `cold_diagnostic` |
| `next_due_at` | 下次计划时间 |
| `schedule_revision` | 调度配置版本 |
| `created_at`、`updated_at` | 审计时间 |

配置校验：

- station_key scope 必须同时提供 station 和所属 Key。
- station scope 必须没有 station_key_id，执行时展开当前可探测 Key。
- primary model 必填；fallback model 去重且不能包含 primary，内置策略最多允许 3 个 fallback model。
- interval 默认 300 秒，建议范围 60 秒到 24 小时。
- 从旧数据迁移的 15 到 59 秒 interval 可以读取，但 UI 必须提示高频风险；再次保存时要求用户确认风险策略。
- schedule jitter 是 `[0, jitter_seconds]` 的正向随机延迟，不得超过 interval 的 25%，且不超过 600 秒。
- attempt timeout 小于 execution timeout。
- retry policy 每个模型最多 3 次 attempt（含首次）；保存配置时必须展示理论最大 attempt 数，并确保至少一次 primary attempt 能在 execution timeout 内完成。
- profile 必须支持选定 protocol。
- authoritative 健康写回只允许受信任内置 adapter 和受信任 profile。

### 8.2 MonitorExecution

一次触发只创建一个 execution。

字段：

- `id`
- `monitor_id`
- `trigger_kind`：`scheduled`、`manual`、`startup_recovery`、`legacy_import`
- `trigger_request_id`：手动命令幂等键
- `status`：`queued`、`running`、`completed`、`partial`、`cancelled`、`skipped`、`interrupted`
- `planned_at`
- `started_at`
- `finished_at`
- `schedule_lag_ms`
- `config_revision`
- `config_snapshot_hash`
- `endpoint_revision`
- `target_count`
- `available_count`
- `degraded_count`
- `unavailable_count`
- `skipped_count`
- `summary_outcome`
- `summary_failure_kind`
- `created_at`

execution 终态规则：

- 所有目标都有且只有一个终态 `MonitorTargetResult`，execution 才能 completed。
- 部分目标持久化或执行失败为 partial。
- 用户取消为 cancelled。
- 应用异常退出留下的 running execution 在下次启动标为 interrupted，不自动当作失败写回健康。
- station-wide summary 由全部目标结果计算，不能由最后写入的 Key 决定。

### 8.3 MonitorTargetResult

`MonitorTargetResult` 表示一个 Station Key 在一次 execution 中的唯一终态。它聚合该目标的 primary、retry 和 fallback attempts，是可用率、固定桶、当前 synthetic 状态和健康写回的直接输入。

字段：

- `id`
- `execution_id`
- `monitor_id`
- `station_id`
- `station_key_id`
- `endpoint_revision`
- `terminal_outcome`
- `terminal_failure_kind`
- `terminal_reason`
- `requested_model`
- `effective_model`
- `used_fallback`
- `attempt_count`
- `decisive_attempt_id`
- `protocol_kind`
- `resolved_adapter_kind` 与 `resolved_dialect`
- `client_profile_id`、`client_profile_version`
- `request_profile_hash`
- `traffic_equivalence`
- `health_writeback_mode`
- `health_writeback_decision` 与 `health_writeback_reason`
- decisive attempt 的 `latency_ms`、`ttfb_ms`、`first_content_ms`
- `started_at`、`finished_at`、`created_at`

不变量：

- `(execution_id, station_key_id)` 唯一；重试 finalization 必须返回同一个结果而不是新增记录。
- `attempt_count` 等于该 target 已持久化的 attempt 数；零 attempt 只允许得到带结构化原因的 `skipped`。
- `decisive_attempt_id` 必须属于相同 execution 和 Station Key；skipped 可以为空。
- fallback 或 retry 后成功的 terminal outcome 至少为 `degraded`。
- 统计和健康状态机不直接消费 raw attempts，避免重试和 fallback 放大失败分母。

### 8.4 ProbeAttempt

字段：

- `id`
- `execution_id`
- `station_id`
- `station_key_id`
- `endpoint_revision`
- `model`
- `model_role`：`primary` 或 `fallback`
- `model_index`
- `attempt_number`
- `protocol_kind`
- `client_profile_id`
- `client_profile_version`
- `request_profile_hash`
- `transport_mode`
- `started_at`
- `headers_received_at`
- `first_content_at`
- `finished_at`
- `latency_ms`
- `ttfb_ms`
- `first_content_ms`
- `http_status`
- `outcome`
- `failure_kind`
- `retryable`
- `retry_after_ms`
- `response_model`
- `content_extracted`
- `validation_kind`
- `validation_passed`
- `output_bytes`
- redacted usage 与 cost evidence
- `error_summary`
- `created_at`

不保存：

- API Key、Cookie、OAuth token。
- 完整请求 Header。
- 完整 prompt、expected answer。
- 上游响应正文。
- 可还原用户身份的 CLI metadata。

### 8.5 ProbeOutcome

`ProbeOutcome` 是受约束枚举：

- `available`：首选路径一次成功且未超过慢响应阈值。
- `degraded`：功能可用但慢响应、重试后成功、fallback 模型成功或部分协议能力退化。
- `unavailable`：目标在该 protocol/model/profile 下不可用。
- `skipped`：未发起网络请求或结果不应参与健康统计。

`skipped` 不计入成功、失败或可用率分母，但必须保存原因。

### 8.6 FailureKind

最小稳定集合：

| FailureKind | 说明 | 默认是否重试 |
|---|---|---:|
| `dns` | DNS 解析失败 | 是 |
| `connect` | TCP/代理连接失败 | 是 |
| `tls` | TLS 建连或证书失败 | 否 |
| `network` | 其他网络错误 | 是 |
| `connect_timeout` | 连接阶段超时 | 是 |
| `response_timeout` | 响应或流读取超时 | 是 |
| `cancelled` | 用户或应用取消 | 否 |
| `auth` | 401/403 或协议鉴权失败 | 否 |
| `rate_limit` | 429/配额限制 | 按 Retry-After |
| `invalid_request` | 400/422 或参数不兼容 | 否 |
| `client_error` | 其他 4xx | 否 |
| `server_error` | 5xx | 是 |
| `redirect` | 非法或未完成重定向 | 否 |
| `protocol_mismatch` | Content-Type、JSON 或 SSE framing 不符合协议 | 否 |
| `stream_incomplete` | 没有合法完成事件或流提前断开 | 可配置 |
| `empty_response` | 没有可用模型输出 | 可配置 |
| `content_mismatch` | 未返回 challenge 预期结果 | 可配置一次 |
| `model_unavailable` | 模型不存在或无权限 | 进入 model fallback |
| `profile_rejected` | 请求画像不被目标接受 | 否 |
| `needs_configuration` | 协议、profile、模型或模板无法形成唯一合法计划 | 否，记 skipped |
| `budget_exhausted` | 达到本地探针日预算 | 否，记 skipped |
| `execution_deadline` | execution 剩余时间不足以开始下一次网络尝试 | 否；零 attempt 时记 skipped |
| `target_disabled` | Key/monitor 已停用 | 否，记 skipped |
| `stale_target` | endpoint revision 已变化 | 否，记 skipped |
| `internal` | 本地不可恢复错误 | 否 |

错误字符串仅用于脱敏排障，业务逻辑只能依赖枚举和结构化字段。

## 9. 协议适配

### 9.1 统一 Adapter 契约

每个 `ProtocolAdapter` 必须实现：

1. 校验 protocol-specific options。
2. 根据 target、model、profile 和 challenge 构建受控请求。
3. 选择 AuthStrategy。
4. 解析普通响应。
5. 增量解析流式响应。
6. 提取模型文本、模型名、usage 和协议错误。
7. 判断协议是否完整结束。
8. 将状态码和错误对象映射为 FailureKind。
9. 提供请求能力说明供 UI 使用。

Adapter 不允许直接读取数据库、更新健康或决定重试次数。

### 9.2 OpenAI Chat Completions

基本请求：

- `POST /v1/chat/completions`
- Bearer auth
- `model`
- `messages`
- 低输出预算
- 可选 `stream: true`

解析：

- 非流式：`choices[].message.content`
- 流式：`choices[].delta.content`
- 识别顶层 error、流内 error 和非 JSON 响应
- 兼容 `max_tokens` 与 `max_completion_tokens` 的能力差异，但不同时发送冲突字段

### 9.3 OpenAI Responses

基本请求：

- `POST /v1/responses`
- Bearer auth
- `model`
- `input`，可选最小 `instructions`
- `store: false`
- 可选 `stream: true`

流式解析必须按 typed event 分支，至少处理：

- `response.created`
- `response.output_text.delta`
- `response.output_text.done`
- `response.completed`
- `response.failed`
- `error`

不得把 Responses SSE 当作 Chat delta。没有 completion event 或明确失败事件时不能标为 available。

### 9.4 Anthropic Messages

基本请求：

- `POST /v1/messages`
- 标准 API 使用 `x-api-key` 和受支持的 `anthropic-version`
- OAuth/Claude Code compatibility 只在 profile 与秘密类型匹配时使用 Bearer
- `model`
- `messages`
- `max_tokens`
- 可选 `stream: true`

解析：

- 非流式 content blocks 中的 text
- 流式 message/content block 生命周期与 `text_delta`
- Anthropic 原生 error object
- message stop/end 事件完整性

不得为普通 API Key 自动注入 Claude Code OAuth Header，也不得伪造 OAuth scope。

### 9.5 Gemini Native

基本能力：

- `generateContent`
- `streamGenerateContent?alt=sse`
- Header 或 query API key 由 AuthStrategy 决定
- `contents[].parts[].text`
- 低温度和低输出预算
- 可选关闭 thinking，但仅在模型和端点支持时发送

解析：

- `candidates[].content.parts[].text`
- 流式多 chunk 文本拼接
- prompt feedback、finish reason、安全拦截和 Gemini 原生错误

Gemini OpenAI-compatible endpoint 使用单独 adapter/dialect，不和 native parser 混用。

### 9.6 xAI / Grok

xAI/Grok 作为独立 adapter 注册，不仅是 UI 中的 OpenAI 别名。

第一阶段以经过验证的 OpenAI-compatible Chat Completions 方言为基础，独立维护：

- base URL 与 endpoint capability
- 认证策略
- model capability
- 流式结束语义
- 错误和限流映射
- 支持参数白名单

实现时必须依据当时的 xAI 官方协议和真实中转兼容测试更新 fixture。不得预先假设所有 Grok 端点支持 OpenAI Responses，也不得将未知字段透传给上游。

### 9.7 Generic OpenAI-compatible

兜底 adapter 只支持经过定义的最小交集：

- Chat Completions 请求
- Bearer auth
- 标准 message content
- 标准 Chat SSE delta

任何 vendor-specific 扩展必须通过 dialect capability 显式启用，不能靠 URL 字符串猜测。

### 9.8 Protocol 解析与 Auto

`protocol_kind=auto` 只是一种 definition 输入方式，不是运行期依次尝试所有协议：

- planner 优先消费 Station/Station Key 已持久化的协议能力、upstream API format 和模型能力事实。
- 无法唯一解析时，monitor 必须处于 needs-configuration，不发送计费探针。
- 用户可以通过一次显式 capability test 选择协议；该测试有独立预算和历史。
- execution 启动时将解析后的具体 adapter/dialect 固化到配置快照。
- 协议失败不会静默切换另一个协议并标绿；协议迁移必须产生可见配置变化。
- 新增 adapter 后不自动改变已有 monitor 的协议选择。

## 10. Client Profile 与 CLI compatibility

### 10.1 定位

`ClientProfile` 表示请求画像，不表示协议本身，也不拥有秘密。内置 profile：

- `standard_api`
- `codex_cli_compat`
- `claude_code_compat`
- `gemini_cli_compat`
- `grok_cli_compat`，仅在存在可验证客户端画像时启用
- `custom_compat`，高级模式

### 10.2 字段

- `id`
- `version`
- `display_name`
- `compatible_protocols`
- `user_agent`
- 受控 required/optional headers
- 受控 body defaults
- `identity_policy`
- `max_output_tokens`
- `stream_default`
- `reasoning_policy`
- `parser_compatibility_version`
- `built_in`
- `enabled`

### 10.3 版本规则

- 内置 profile 由代码和 golden fixture 定义。
- monitor 固定 profile id 和 version。
- 新版本不静默改变历史 execution。
- UI 显示 profile 更新可用，并提供显式升级。
- execution 保存实际 profile version 和 hash。
- 旧 profile 可停止新建，但已有 monitor 仍可读取并迁移。

### 10.4 覆盖边界

profile 可以控制：

- User-Agent。
- 协议允许的兼容 Header。
- 已校验的 body 默认值。
- stream、reasoning、输出预算等受控参数。

profile 不能控制：

- Authorization、API Key、Cookie。
- 任意完整 URL 或非目标主机。
- TLS 校验。
- 日志脱敏。
- 响应大小上限。
- 任意脚本或表达式。
- OAuth 身份、账号 UUID 和官方设备证明。

### 10.5 稳定本地身份

只有协议确实需要客户端实例身份时才生成稳定、本地、非敏感标识：

- 标识按 installation + station + profile scope 派生。
- 同一 scope 内稳定，不按请求随机变化。
- 不包含真实账号 ID、设备序列号或 API Key。
- 不写入普通日志。
- 用户可在高级设置中重置。

## 11. Challenge 与内容验证

### 11.1 默认策略

默认使用低成本随机算术 challenge：

- 两个小整数和多种自然语言题面。
- expected answer 只保存在 execution 内存和不可逆 validation evidence 中。
- prompt 中不得包含完整 expected marker + answer。
- 要求模型只返回稳定短格式，例如 `RP_ANSWER=<number>`。
- 输出预算保持 8 到 32 token。

### 11.2 Validator

内置 validator：

- `arithmetic_exact`
- `non_empty_text`，仅用于兼容诊断，不得 authoritative 写回健康
- `json_schema`，后续能力
- `custom_contains`，高级模式且默认 observe-only

内容 validator 必须运行在协议抽取后的模型文本上，不能直接在原始 SSE/JSON 字符串中搜索。

### 11.3 防止假成功

以下结果必须失败：

- HTTP 200 + 空 body。
- HTTP 200 + HTML 登录页。
- HTTP 200 + 顶层 error。
- HTTP 200 + 没有模型文本。
- SSE 只有 keepalive/comment。
- SSE 未完成且没有足够有效输出。
- 模型原样回显题面但没有正确答案。
- 返回错误答案或不符合 validator。

## 12. 账号风险与探针保护

CLI profile 只是兼容手段，不是唯一的账号保护措施。系统必须同时提供：

- 默认 5 分钟周期和周期 jitter。
- 单 Key 全局最小探测间隔。
- 单 Key 每日探测预算。
- 全局、Station 和 Key 三层并发限制。
- 应用启动错峰。
- 低输出预算、无工具、无多轮上下文、无持久化。
- 401/403 立即停止，不重试并进入人工检查状态。
- 429 尊重 `Retry-After`，进入长 cooldown。
- 余额耗尽、账号禁用、Key 禁用或已 cooldown 时不继续探测。
- 相同 target/profile/protocol 的并发请求合并或拒绝。
- 手动连续点击受到最小间隔和幂等约束。
- conservative、balanced、aggressive 三档 risk policy；默认 balanced。

建议默认：

| 配置 | 默认值 |
|---|---:|
| interval | 300 秒 |
| jitter | 30 秒 |
| attempt timeout | 15 秒 |
| execution timeout | 45 秒 |
| retry count | 1 |
| retry base delay | 500 ms |
| retry max delay | 2 秒 |
| retry jitter | 0.2 |
| max output tokens | 20 |
| per-key min gap | 60 秒 |
| per-key daily budget | 288 |
| global concurrency | 4 |
| station concurrency | 2 |
| key concurrency | 1 |

高频配置必须在 UI 显示风险状态，但本地高级用户仍可显式调整。

## 13. 重试与模型 fallback

### 13.1 重试

- 只重试被 adapter 判为 retryable 的 FailureKind。
- auth、invalid request、TLS、明确 profile rejection 不重试。
- rate limit 只在 execution deadline 和 Retry-After 允许时重试。
- 每个 attempt 都持久化，重试后成功的 execution 为 degraded。
- 退避采用 capped exponential backoff + jitter。
- 取消立即中止退避等待。
- execution deadline 覆盖排队、permit 等待、重试退避、模型 fallback 和响应读取；planner 只有在剩余预算足以容纳一次 attempt 时才可启动网络请求。
- deadline 前已得到失败 attempt 时，以最后一个决定性失败形成 unavailable target result；从未开始 attempt 时形成 `skipped/execution_deadline`。

### 13.2 模型 fallback

- 先完成 primary model 的允许 attempts。
- 只有 `model_unavailable`、受支持的 server error 或策略允许错误才进入 fallback。
- auth、profile rejection 和 target-level rate limit 不切换模型掩盖问题。
- fallback 成功记录 `model_role=fallback`，对应 target result 至少为 degraded；execution summary 再按全部 target results 聚合。
- UI 同时显示请求模型和实际成功模型。
- fallback 列表硬上限为 3；planner 必须在执行前计算有界计划，并在运行中按剩余 deadline 截断，不允许靠无限配置扩大请求数。

## 14. 调度器与 Orchestrator

### 14.1 MonitorScheduler

调度器使用按 `next_due_at` 排序的最小堆或等价优先队列：

- 等待最近到期任务，不固定每 30 秒扫描全部 monitor。
- monitor 新建、编辑、启停和删除时通过 notify 唤醒并重建受影响任务。
- 启动时加载 enabled monitor，并在安全窗口内错峰。
- 到期任务进入有界队列，不能直接无限 spawn。
- 队列满时记录 schedule lag，不丢失任务或忙等。

### 14.2 至少间隔语义

下一次执行时间：

```text
max(previous_planned_at + interval, finished_at + interval) + jitter
```

其中 jitter 每次独立采样且只取非负值，因此实际开始间隔不会短于配置 interval。手动执行不重置定时基准；启停或编辑调度配置时按新的 schedule revision 重算 next due。

系统休眠、应用退出或执行耗时过长后不补跑多个遗漏周期。重新启动只安排下一次合理执行。

### 14.3 统一入口

定时和手动执行都调用：

```text
MonitorOrchestrator::request_execution(command)
```

orchestrator 负责：

1. 校验 monitor 和 target。
2. 应用幂等、single-flight、频率和日预算。
3. 创建 queued execution。
4. 获取全局与目标并发许可。
5. 固化配置快照。
6. 展开 target 和模型计划。
7. 执行 attempts 并为每个目标形成唯一 target result。
8. 原子记录 target result 和幂等 health observation。
9. 计算 execution summary。
10. 释放许可并安排 next due。

### 14.4 Single-flight

- 同一个 monitor 同时最多一个 running execution。
- 同一个 Station Key 同时最多一个 synthetic probe。
- 相同手动 `trigger_request_id` 返回同一 execution。
- 手动请求遇到现有 execution 时返回现有 execution ID，不新建重复请求。
- 删除 monitor 会取消 queued execution；running execution 进入 cancellation 并保留终态。

### 14.5 并发层次

按固定顺序获取许可，避免死锁：

1. global permit
2. station permit
3. station-key permit

等待许可必须受 execution cancellation 和 deadline 控制。任何 permit 使用 RAII guard 释放。

### 14.6 应用生命周期

- 后台任务由现有 task supervisor 管理。
- shutdown 先停止接收新执行，再取消 queued，最后给 running execution 有界 drain 时间。
- 超时后 remaining execution 标为 interrupted。
- 启动恢复将遗留 queued/running 标为 interrupted，再重新计算调度，不自动重复网络请求。

## 15. 健康状态机与路由联动

### 15.1 Observation 而非直接覆盖

`ResultRecorder` 将终态 `MonitorTargetResult` 转换为 `HealthObservation`：

- source：`synthetic_monitor`、`proxy_request`、`manual_connectivity`
- station_key_id
- endpoint_revision
- protocol/model/profile
- outcome/failure kind
- observed_at
- confidence
- traffic_equivalence
- execution_id/target_result_id/decisive_attempt_id

`HealthTransitionService` 是唯一允许修改 `station_key_health` 的业务入口。

### 15.2 Traffic equivalence

探针与真实路由的等价性：

- `exact`：协议、认证、profile 和路由真实请求一致。
- `compatible`：协议一致，body 为最小 synthetic challenge。
- `diagnostic`：cold connect、自定义 contains、未知 profile 等诊断请求。

只有 exact/compatible observation 默认允许影响路由健康。diagnostic 只进入监控历史。

### 15.3 Writeback mode

monitor 配置：

- `observe_only`：不写路由健康。
- `eligible`：满足可信度和阈值后写回，默认。
- `authoritative`：用于受信任内置协议的明确硬失败；仅高级模式。

自定义 adapter/profile 默认 observe-only。

### 15.4 状态转换

健康摘要至少维护：

- current status
- consecutive successes
- consecutive failures
- last success/failure/observation time
- last failure kind
- cooldown reason/until
- source 与 confidence
- endpoint revision

建议规则：

- available：增加连续成功，清零连续失败。
- degraded：不视为完全失败；更新慢响应或重试证据。
- unavailable：增加连续失败，清零连续成功。
- skipped/cancelled/interrupted：不改变成功失败计数。
- 达到 failure threshold 后进入 degraded/offline 或 cooldown。
- 达到 recovery threshold 后才恢复 healthy，避免抖动。
- auth 和明确 revoked key 可以使用更强转换，但必须确认 observation 的 traffic equivalence。
- rate limit 使用 Retry-After；没有 Retry-After 时使用有上限退避。
- endpoint revision 不匹配时拒绝写回。

### 15.5 多来源合并

- 真实代理流量和主动探针都写 observation，不相互直接覆盖。
- 新鲜真实请求成功可以加速从偶发 synthetic network failure 恢复。
- synthetic auth failure 不得在 CLI profile 与真实路由 profile 不一致时直接禁用 Key。
- UI 展示当前综合健康，同时能查看最近一次 synthetic 结果。
- 合并策略必须是纯函数并有完整状态机测试。
- 当前散落在 request log、proxy runtime 和 routing store 的健康写入口必须分阶段收敛到同一个 application `HealthTransitionService`；迁移期可以有调用适配器，但不得双写、不得保留两套状态转换规则。

## 16. 持久化模型

### 16.1 总体策略

- 使用下一可用 migration 编号，不在本规范中硬编码编号。
- 调度需要查询和约束的字段使用真实列。
- provider-specific options 可以保存版本化 JSON，但写入前必须经 adapter typed validation。
- 时间统一保存 Unix 毫秒 INTEGER；不再对 TEXT 时间进行 CAST 排序。
- outcome、failure kind、trigger kind 和 execution status 使用数据库 CHECK。

### 16.2 channel_monitors 演进

保留 `channel_monitors` 作为 definition 表以降低外键和迁移成本，但重建字段语义：

- 拆出 `primary_model`。
- 保留真正的 `fallback_models_json`。
- 新增 protocol、profile、retry/risk/health policy。
- 新增 schedule revision 与 next due 毫秒列。
- 不再保存由最后一个子 run 覆盖的 `last_status` 作为权威事实。
- last execution 信息由 execution/read model 查询或维护受约束 summary。

### 16.3 channel_monitor_executions

建立 execution 表并增加索引：

- `(monitor_id, started_at_ms DESC, id DESC)`
- `(status, planned_at_ms)`
- `(trigger_request_id)` 条件唯一
- `(finished_at_ms)` 用于 retention

### 16.4 channel_monitor_attempts

建立 attempt 表并增加索引：

- `(execution_id, station_key_id, model_index, attempt_number)` 唯一
- `(station_key_id, started_at_ms DESC, id DESC)`
- `(monitor_id, started_at_ms DESC, id DESC)`，可冗余 monitor_id 以优化 read model
- `(outcome, failure_kind, started_at_ms)`
- `(finished_at_ms)` 用于 retention

### 16.5 channel_monitor_target_results

建立 target result 表并增加索引：

- `(execution_id, station_key_id)` 唯一
- `(monitor_id, station_key_id, finished_at_ms DESC, id DESC)`，用于 recent 和当前状态
- `(monitor_id, finished_at_ms DESC, id DESC)`，用于 execution/workspace 查询
- `(terminal_outcome, terminal_failure_kind, finished_at_ms)`
- `(finished_at_ms)`，用于 retention
- `decisive_attempt_id` 外键必须指向同 execution、同 Station Key；SQLite 无法用普通外键表达的跨列约束由 target-finalization 事务和 integration test 保证

`channel_monitor_target_results` 是 synthetic 统计的 raw fact；attempt 只用于诊断和审计。

### 16.6 channel_monitor_bucket_rollups

为长窗口建立 rollup 表：

- monitor_id
- station_key_id
- protocol_kind
- primary_model
- bucket_kind：hour/day
- bucket_start_ms
- eligible_count
- available_count
- degraded_count
- unavailable_count
- skipped_count
- latency_sum_ms
- latency_count
- latest_outcome
- latest_result_at_ms
- `failure_counts_schema_version`
- `failure_counts_json`

唯一键：

```text
(monitor_id, station_key_id, protocol_kind, primary_model, bucket_kind, bucket_start_ms)
```

`failure_counts_json` 使用版本化、写前校验的对象结构，key 只能是稳定 `FailureKind`，value 是非负整数；读取失败时该 rollup 标记 dirty 并由 target result 重建，不允许 UI 静默使用损坏计数。

rollup 必须可由 raw target result 重建，不能成为唯一原始事实。rollup 更新失败只标记对应范围 dirty，不回滚或改写已经提交的 target result。

使用 `channel_monitor_rollup_dirty_ranges` 保存待修复维度、bucket range、首次/最近失败时间和有界重试状态；同一维度/range 合并，不允许故障期间无限增长 repair 记录。

### 16.7 health observations

建立统一 `station_key_health_observations` 账本，至少保存：

- `id`
- `source` 与 `source_event_id`
- `station_key_id`、`endpoint_revision`
- protocol/model/profile、outcome/failure kind
- confidence、traffic equivalence、writeback mode/decision/reason
- observed_at 与 applied_at

`(source, source_event_id)` 唯一；synthetic monitor 的 `source_event_id` 必须是 `target_result_id`，proxy request 使用稳定 attempt identity。所有来源通过同一 service 写 observation 和更新 `station_key_health`，不允许 store 内另藏第二套转换规则。

### 16.8 probe budget usage

建立持久化 `channel_monitor_probe_budget_usage`：

- `station_key_id`
- `budget_date` 与 `time_zone_id`
- `reserved_attempt_count`
- `updated_at_ms`

在发出网络请求前以事务原子预留一次 attempt 预算；进程崩溃也不返还，避免重启绕过日预算。`(station_key_id, budget_date, time_zone_id)` 唯一，过期记录随 retention worker 分批清理。

### 16.9 client profiles

- 内置 profile 存在代码注册表和 fixture，不以可编辑数据库行为覆盖。
- custom profile 使用独立表，保存受控字段和 schema version。
- secret/auth 字段不允许出现在 profile JSON。
- 删除 custom profile 前检查 monitor 引用。

### 16.10 事务边界

持久化采用三个明确边界：

1. **Attempt append transaction**：追加一个不可变 attempt 及其脱敏 request observation；以 attempt ID/唯一序号幂等，不修改健康或统计分母。
2. **Target-finalization transaction**：校验 attempts，写唯一 target result，写以 `target_result_id` 为幂等键的 HealthObservation，并在符合 revision/traffic/writeback policy 时调用统一健康转换。任一步失败均回滚该 target finalization，可安全重试。
3. **Execution-finalization transaction**：校验所有 target result 已终态，以它们计算并更新 execution summary，再更新 monitor schedule。重复执行必须得到同一 summary 和 next due。

Bucket rollup 是派生缓存，不属于 execution 完成事务。execution 完成后异步或增量更新 rollup；失败时写入有界的 dirty-range/rebuild marker，并由 repair worker 重建。rollup 故障不得把已提交 attempt、target result 或 execution 改为失败。

health writeback 失败时 target finalization 回滚，execution 最终为 partial，直到有界重试或后续 repair 以同一 `target_result_id` 完成；不得生成重复 observation 或重复增加连续失败计数。

## 17. 旧数据迁移

### 17.1 Definition 迁移

- 旧 `fallback_models[0]` 迁移为 primary model。
- 剩余项目迁移为真正 fallback models。
- 空数组使用明确默认模型，并记录 migration warning。
- 旧 template 根据 endpoint_kind 映射 protocol。
- 无法映射的 template 进入 disabled/custom observe-only。
- 旧 interval、jitter、timeout 经新约束规范化，不静默丢弃用户设置。

### 17.2 Run 迁移

旧 `channel_monitor_runs` 没有父 execution 和语义验证：

- 每条旧 run 迁移为一个 `legacy_import` execution + target result + attempt，或按可证明的 station-wide 批次分组。
- outcome 映射保留原值，但增加 `semantic_confidence=legacy_http_only`。
- legacy 数据只用于历史展示，不回放健康写回。
- 迁移后新代码只写新表，不进行长期 dual-write。

### 17.3 模板种植

启动/迁移时幂等种植受版本控制的内置 adapter/profile/template：

- 稳定 ID。
- 明确版本。
- built-in 不允许删除，只允许停用或复制。
- 内容变更创建新版本，不原地改变历史执行语义。

### 17.4 安全迁移流程

1. 迁移前创建现有数据目录备份或使用项目既有恢复机制。
2. 在事务中创建新表、校验约束和 backfill。
3. 比较 monitor/run 数量与 orphan 数量。
4. 记录 migration manifest。
5. 校验通过后启用新 read/write path。
6. 旧表保留一个发布观察周期为只读 legacy 数据。
7. 下一次明确迁移删除旧表和兼容代码。

不允许永久保留双写开关。

## 18. 时间桶与统计语义

### 18.1 固定窗口

| 窗口 | 输出 |
|---|---|
| `recent` | 最近 60 个终态 target result |
| `24h` | 24 个 1 小时桶 |
| `7d` | 7 个本地时区自然日桶，包含当前日 |
| `30d` | 30 个本地时区自然日桶，包含当前日 |

Workspace DTO 返回规范 IANA `time_zone_id`、时区来源、每个 bucket 的起止 Unix 毫秒和显示标签。24h 始终是包含当前小时的 24 个滚动小时桶；7d/30d 按本地自然日边界计算并正确处理 DST。前端不得重建、平移或猜测 bucket 时间；无法取得系统时区时后端明确回退 UTC 并返回诊断标记。

### 18.2 缺失数据

- 没有 eligible target result 的 bucket 为 missing。
- missing 不等于 unavailable。
- skipped-only bucket 为 missing，并在 tooltip 显示 skipped 数。
- UI 使用灰色明确表示没有数据。

### 18.3 可用率

```text
eligible = available + degraded + unavailable
strict_availability = available / eligible
effective_availability =
  (available + degraded * degraded_weight) / eligible
```

- 默认 degraded weight 为 0.5，并在 DTO 中返回使用值。
- skipped、cancelled、interrupted 不进入分母。
- UI 主列显示 effective availability，tooltip 同时显示 strict availability 和详细计数。
- 所有 availability、bucket 和当前 synthetic 状态只按终态 target result 计算；attempt retry/fallback 细节只进入 execution history/tooltip，避免一次 execution 多次尝试放大分母。

### 18.4 当前状态

- 当前状态来自该监控行最新完成的 terminal target result；execution 的其他 target 是否 partial 不覆盖这条事实。
- running execution 单独显示运行中，不覆盖上一次稳定结果。
- station group summary 根据目标结果聚合，不使用最后一个子结果。
- disabled monitor 显示 disabled，不伪装成 missing。

### 18.5 延迟

- 保存 total latency、TTFB 和 first content。
- 列表默认显示 total latency，并可在 tooltip 展示其他阶段。
- bucket 返回平均延迟和样本数。
- P95 可以从保留 raw 数据计算；如果进入 rollup，必须使用可重建的直方图或明确标注近似，不能平均 P95。

## 19. Retention 与清理

建议默认：

- raw attempts：30 天、每 monitor 10,000 条、全局 100,000 条，任一上限先达到即按最旧记录分批清理。
- target results：90 天、每 monitor 20,000 条、全局 500,000 条，任一上限先达到即清理。
- executions：90 天、每 monitor 20,000 条、全局 500,000 条，任一上限先达到即清理。
- hourly rollups：90 天。
- daily rollups：365 天。
- change/health observations：遵循健康事实层独立 retention。

清理器要求：

- 启动延迟和 jitter。
- 单实例防重入。
- 分批删除，每轮有最大批数和最大耗时。
- 删除顺序遵守外键。
- 清理前确保需要的 rollup 已生成；dirty rollup 未修复时不得删除其唯一 raw target result 来源。
- 支持配置热更新。
- cleanup 失败不影响调度器运行。
- 记录删除计数和耗时，不记录用户数据。

## 20. 后端 Read Model 与 IPC

### 20.1 命令

建议命令边界：

- `list_channel_monitor_definitions`
- `get_channel_monitor_definition`
- `create_channel_monitor_definition`
- `update_channel_monitor_definition`
- `delete_channel_monitor_definition`
- `run_channel_monitor`
- `cancel_channel_monitor_execution`
- `get_channel_monitor_execution`
- `list_channel_monitor_executions`
- `load_channel_status_workspace`
- `list_monitor_profiles`
- `list_monitor_protocol_capabilities`
- custom profile 管理命令

可以在迁移期保留旧命令 facade，但必须转发到新 application service，不得保留旧业务实现。

### 20.2 Workspace DTO

`load_channel_status_workspace` 一次返回：

- filters/options
- row summaries
- current/running state
- selected window buckets
- aggregate counts
- pagination cursor
- generated_at
- data freshness
- time zone id/source 和后端计算的 bucket boundaries

UI 切换时间窗口可以使用参数化 workspace query，避免一次加载所有历史。

### 20.3 分页与排序

- 监控行、execution history 使用 cursor pagination。
- 排序在后端完成，稳定次序包含唯一 ID。
- 支持 station、status、protocol、model、profile、enabled 筛选。
- 搜索只匹配非敏感名称和模型，不搜索错误正文。
- 所有 limit 有上限。

## 21. 状态监控 UI

### 21.1 信息架构

状态监控第一页直接是可操作工作区，不增加营销式介绍页。

顶部工具栏：

- 搜索。
- Station 筛选。
- 当前状态筛选。
- Protocol/Model/Profile 筛选。
- 时间窗口：近 60 次、24 小时、7 天、30 天。
- 排序。
- 刷新。
- 新建监控。
- 列表/紧凑视图切换仅在确有使用价值时提供。

主表横向列：

```text
Key | Station | Model / Protocol | Client Profile |
Current Status | Availability | Last Probe / Latency | Trend | Actions
```

### 21.2 行身份

一行表示一个稳定监控流：

```text
(monitor_id, station_key_id, protocol_kind, primary_model)
```

- key monitor 通常产生一行。
- station-wide monitor 展开为多个 Key 行，并可按 monitor/Station 分组。
- 同一 Key 的不同 protocol/model 不合并，避免混淆可用率。
- monitor summary 可以作为分组 header，不作为虚假的 Key 健康行。

### 21.3 Trend

- 固定格数和固定宽度，不因 loading、tooltip 或状态变化导致布局跳动。
- 每格来自后端 bucket/terminal result。
- available、degraded、unavailable、missing 使用低饱和绿、黄、红、灰。
- tooltip 展示起止时间、有效样本、四类计数、失败分类和延迟。
- 运行中使用独立轻量状态，不覆盖格子历史。
- 不使用前端按成功率人工生成颜色分布。

### 21.4 交互

- 点击行打开未嵌套卡片的详情 drawer/page。
- 详情包括 definition、当前配置、execution history、attempt tree、失败分类和 profile。
- Run Now 返回 execution ID，并订阅/轮询 execution 状态。
- 已运行时显示正在运行，不发送第二次请求。
- 编辑 profile、频率或 health writeback 时显示影响范围。
- auth/rate-limit/profile-rejected 给出明确但脱敏的处理方向。

### 21.5 桌面与移动

- 桌面保持密集横表。
- 窄窗口固定关键列，趋势允许横向滚动或聚合更少桶。
- 不将每行退化成多层嵌套卡片。
- 文本必须截断并可查看完整 tooltip，不挤压状态和趋势。
- 行高、图块宽度、图标按钮尺寸使用稳定约束。

### 21.6 视觉

- 浅灰窗口背景、白色或近白面板、细边框。
- 低饱和状态色，不做 Relay Pulse 深色复刻。
- 状态徽标紧凑，避免大面积彩色背景。
- 图标按钮使用项目现有 Lucide 体系和 tooltip。
- 不使用装饰性渐变、发光、圆球或营销卡片。

## 22. 安全与隐私

### 22.1 Secret

- API Key 通过 SecretRef 解析。
- secret 只在最小 async scope 中存在，使用 zeroize 容器。
- profile 和 template 不保存 Authorization value。
- DTO 永不返回 secret。

### 22.2 日志

允许记录：

- execution/attempt ID。
- station/key 的内部 ID 或脱敏名称。
- protocol/profile 版本。
- outcome/failure kind。
- HTTP 状态、耗时、字节数。

禁止记录：

- API Key、Cookie、token。
- 完整 Header。
- prompt 和 expected answer。
- 响应正文或响应片段。
- 原始 upstream URL query 中的 secret。
- 用户登录或 CLI account metadata。

错误摘要在 application 和 persistence 两层脱敏，并限制长度。

### 22.3 URL 与代理

- 请求目标从 Station `api_base_url` 和 adapter path 组合。
- 禁止 profile/template 指向另一主机。
- 重定向默认限制同源；跨源重定向失败。
- 代理必须来自显式 Station/Settings 配置。
- 环境代理是否可用由应用设置明确决定，不隐式继承。
- custom profile 不能关闭 TLS 校验。

### 22.4 CLI compatibility 边界

- CLI profile 用于协议兼容，不承诺规避上游风控。
- UI 应说明 profile 可能受上游政策和版本变化影响。
- 不模拟不存在的 OAuth 授权。
- 不自动从本机 Codex、Claude Code、Gemini CLI 配置读取凭据。
- 不在 profile 之间静默轮换以绕过拒绝。

## 23. 可观测性与诊断

本地诊断指标：

- scheduler queue depth
- active executions/attempts
- schedule lag
- permits in use
- outcome/failure kind counts
- retry/fallback counts
- profile rejection counts
- bucket query duration
- cleanup duration/deleted rows

这些指标默认用于本地诊断，不上传。

每个 execution 应能回答：

- 为什么执行。
- 使用了哪个配置和 profile。
- 展开了哪些目标。
- 每个目标尝试了哪些模型和次数。
- 为什么重试或 fallback。
- 为什么写回或没有写回健康。
- 最终 summary 如何计算。

## 24. 错误处理与降级

- 单个 target 失败不取消 station-wide 其他目标，除非 execution 被取消。
- persistence 暂时失败时保留内存结果并有限次数重试 finalization，但有界且不写临时明文文件。
- profile/template 无效时 target result 为 `skipped/needs_configuration`，不发出网络请求。
- bucket rollup 失败不将已提交 attempt、target result 或 execution 改为失败；标记需要重建。
- health writeback 失败使 execution finalization partial，并可幂等修复。
- UI workspace 查询失败保留上次成功数据并明确 freshness。
- 内置 profile/parser 版本不兼容时 fail closed，不回退到任意 raw parser。

## 25. 测试策略

### 25.1 Domain 单元测试

- outcome/failure 映射。
- retry decision。
- model fallback decision。
- target result reduction 与 decisive attempt 选择。
- execution summary。
- availability 分母和 degraded weight。
- health state transition。
- traffic equivalence/writeback。
- schedule next due 和 jitter 边界。

### 25.2 Adapter contract tests

每个 adapter 至少覆盖：

- 正常非流式成功。
- 正常流式成功。
- 随机 chunk 边界。
- 空响应。
- malformed JSON/SSE。
- HTTP 200 error object。
- 401、403、429、400/422、5xx。
- stream error、提前 EOF、缺失 completion。
- content mismatch。
- usage/model extraction。
- redaction。

使用本地 fixture/HTTP server，不依赖真实互联网。

### 25.3 Profile golden tests

- 每个内置 profile 有脱敏 request golden fixture。
- 验证方法、path、Header 名称、body shape 和默认值。
- fixture 不含真实 secret、设备或账号身份。
- profile 版本变化必须显式更新 golden。
- profile 与 adapter capability 不匹配必须拒绝。

### 25.4 Scheduler 测试

使用 Tokio paused time：

- 最近到期唤醒。
- 无 catch-up storm。
- startup stagger。
- global/station/key concurrency。
- manual/scheduled single-flight。
- cancellation during permit/backoff/request。
- config reload。
- shutdown/recovery。
- queue saturation 和 schedule lag。

### 25.5 Persistence 测试

- 新库 migration 和 built-in seed。
- 旧 definition/run backfill。
- legacy semantic confidence。
- execution finalization 幂等。
- attempt 唯一约束。
- target result 唯一约束、target-finalization 幂等和 health observation exactly-once。
- cursor/index query plan。
- bucket rollup 重建。
- retention batch cleanup。
- transaction rollback。
- endpoint revision stale write rejection。

### 25.6 UI 测试

- filters、sorting、pagination。
- 近 60 次/24h/7d/30d 窗口。
- missing 与 unavailable 区分。
- tooltip 计数。
- running 不覆盖历史。
- station-wide 分组。
- profile/risk 状态。
- 长名称和窄窗口不重叠。
- 键盘与 accessibility。

### 25.7 端到端测试

- 创建 monitor -> 定时触发 -> execution -> attempts -> health -> UI。
- 手动与定时重叠只产生一个 execution。
- fallback 后 degraded。
- 401 停止重试并正确 health writeback。
- 429 Retry-After cooldown。
- endpoint revision 变化拒绝旧结果。
- execution deadline 截断 retry/fallback 且不启动超预算 attempt。
- 应用重启恢复 interrupted execution。

### 25.8 可选真实上游测试

- 只在本地显式环境变量和人工授权下运行。
- 不进入默认 CI。
- 凭据不写 fixture/log。
- 每 provider 低频执行并有预算。
- 用于确认官方协议与 CLI profile，不替代本地 contract tests。

## 26. 性能与容量目标

本地基准目标：

- 1,000 个 monitor definitions 可加载。
- 500 个状态行筛选/排序保持可交互，必要时使用虚拟列表。
- 100,000 raw attempts / 500,000 target results 下 500 行 24h workspace 查询目标小于 250 ms。
- 正常容量范围内 scheduler p95 lag 小于 2 秒；容量不足时明确暴露 lag。
- 单次 workspace DTO 大小有上限，不返回所有 execution history。
- 响应 body 读取有硬上限；超过上限在保留已提取文本的前提下终止并分类。

性能目标必须通过生成数据基准验证，不能只凭查询看起来有索引。

## 27. 实施阶段

### Phase 0：基线与冻结

- 固定当前行为测试和数据库 fixture。
- 建立失败分类与协议 fixture 清单。
- 明确旧命令、表和模块删除清单。
- 解决当前与本功能无关的 Cargo 编译阻断。
- 不新增 UI 功能。

退出条件：当前数据可迁移、当前关键行为有测试、工作区可运行 Cargo 检查。

### Phase 1：Domain 与 Adapter 地基

- 新增类型化 domain。
- 建立 ProtocolAdapter、ClientProfile、AuthStrategy、ValidationStrategy。
- 实现 OpenAI Chat/Responses、Anthropic、Gemini、xAI/Grok adapters。
- 实现 challenge 和 streaming parsers。
- 建立 golden/contract tests。

退出条件：所有 adapter 能正确拒绝假 200、空输出和错误内容。

### Phase 2：新执行与持久化模型

- 新 migration。
- definition 演进、execution、target result、attempt、profile 和 rollup schema。
- built-in seed。
- 原子 recorder 和 config snapshot。
- 旧数据 backfill。

退出条件：新 writes 不再写旧 run 模型；迁移和回滚测试通过。

### Phase 3：Scheduler 与 Orchestrator

- nearest-due scheduler。
- 有界队列和三级并发。
- manual/scheduled 统一入口。
- single-flight、risk budget、retry/fallback、shutdown/recovery。

退出条件：压力和 paused-time 测试证明无重复执行、无 catch-up storm、无 permit 泄漏。

### Phase 4：统一健康联动

- HealthObservation。
- HealthTransitionService 统一 proxy/monitor 写回规则。
- endpoint revision 与 traffic equivalence。
- cooldown/recovery。

退出条件：监控失败阈值真实影响路由，诊断探针不会误伤健康，旧 request-log/proxy/routing 健康入口已全部委托给唯一状态转换服务。

### Phase 5：Buckets 与 Read Model

- recent/24h/7d/30d 固定桶。
- workspace query、筛选、排序、cursor。
- rollup 和 retention worker。
- 数据 freshness。

退出条件：所有窗口桶数固定、missing 正确、skipped 不进分母。

### Phase 6：横向 UI

- 新 toolbar 和横向表。
- 状态、可用率、最近检测、趋势和详情。
- execution/attempt history。
- profile/risk controls。
- 窄窗口和 accessibility。

退出条件：真实后端 bucket 驱动 UI，不存在前端伪造趋势。

### Phase 7：切换与旧代码删除

- 旧 command facade 转发或删除。
- 删除旧 runner、guard、run persistence、status card 拼装和死配置。
- 删除旧表/兼容 read 的后续 migration。
- 更新 PROJECT_PLAN、PRODUCT_MODEL、README、attribution 和发布说明。

退出条件：仓库只有一套监控执行和状态统计路径，架构检查没有 legacy allowlist。

## 28. 旧代码删除清单

实施完成时必须审计并删除或替换：

- 30 秒全表轮询 runner。
- 只覆盖后台路径的静态 MonitorRunGuard。
- HTTP status-only success 判断。
- `fallback_models[0]` 充当 primary 的隐式约定。
- 逐子 run 更新 monitor latest 状态。
- 旧 `channel_monitor_runs` 写路径。
- UI 合并 request log/health/raw monitor 的临时 view model。
- 前端按成功率制造 60 格趋势的逻辑。
- 未消费的 consecutive failure/fallback 配置。
- 生产缺失的 built-in template 假设。
- 无 retention 的历史查询。

禁止为了短期回滚长期保留两套后台任务。回滚应通过数据库备份、feature activation 和发布版本完成。

## 29. 验收标准

### 29.1 可靠性

- HTTP 200 空响应、错误 JSON、错误 SSE 和错误答案不能标绿。
- 每次触发有唯一 execution。
- 每个 execution/Station Key 有唯一 target result，retry/fallback 不放大可用率分母。
- 手动与定时执行不会重叠。
- retry/fallback 全过程可追踪。
- 取消、退出和重启都有明确终态。
- station-wide summary 不受子结果写入顺序影响。

### 29.2 健康

- skipped/cancelled/interrupted 不影响健康计数。
- failure/recovery threshold 生效。
- 429 cooldown 尊重 Retry-After。
- endpoint revision 过期结果不写回。
- observe-only/diagnostic 不影响路由。
- proxy 与 monitor 共用健康状态机。

### 29.3 数据

- 24h 固定 24 桶、7d 固定 7 桶、30d 固定 30 桶。
- missing 与 unavailable 有区别。
- 可用率分母排除 skipped。
- retention 保证表大小有界。
- migration 不丢 definition 和历史。
- 关键查询使用预期索引。

### 29.4 协议

- OpenAI Chat、OpenAI Responses、Anthropic Messages、Gemini Native、xAI/Grok 均有独立 contract tests。
- 普通和流式响应均覆盖。
- CLI profiles 可选、版本化、可审计。
- `grok_cli_compat` 在没有经过版本化 fixture 与真实授权验证前保持 disabled；xAI/Grok 标准协议 adapter 不受此限制。
- profile 不保存或泄露 secret。
- custom profile 默认 observe-only。

### 29.5 UI

- 横向密集表符合本地浅色工具风格。
- 筛选、排序和窗口切换稳定。
- 500 行场景可用。
- 文字、按钮、tooltip 和趋势不重叠。
- 不存在假趋势或嵌套卡片。

### 29.6 工程质量

- `pnpm build`、前端测试通过。
- `cargo check`、Rust 单元/集成测试通过。
- migration、contract、scheduler、health 和 E2E 测试进入验证脚本。
- 敏感日志扫描通过。
- 新增 adapter 不要求修改 scheduler 和 persistence 核心。
- 旧监控内核已删除，不以 deprecated 名义继续运行。

## 30. 风险与控制

| 风险 | 控制 |
|---|---|
| CLI profile 过期 | 版本化、golden fixture、显式升级 |
| 探针导致账号风险 | 默认低频、jitter、预算、并发、401/429 熔断 |
| 自定义 profile 误伤健康 | observe-only、受控字段、traffic equivalence |
| migration 丢数据 | 备份、事务、manifest、数量/orphan 校验 |
| 两套架构长期共存 | 明确切换阶段和删除验收 |
| station-wide 数据量增长 | 有界并发、rollup、retention、分页 |
| 流式方言碎片化 | adapter/dialect capability，不在通用 parser 堆条件 |
| synthetic 与真实流量冲突 | 统一 observation 与纯函数状态机 |
| UI 数据量过大 | 参数化 workspace、cursor、虚拟化 |

## 31. 明确不照搬 Relay Pulse 的内容

学习其理念：

- 随机 challenge 与语义校验。
- 多协议流式提取。
- 结构化 sub-status。
- 重试退避与 jitter。
- 最近到期调度、全局并发和启动错峰。
- 固定时间桶和 missing。
- retention worker。
- 横向状态表。

不照搬：

- Go Web 服务和公开状态站部署模型。
- 深色网站视觉。
- 明文配置 Key。
- prompt、expected answer、response snippet 日志。
- 全局禁用 keep-alive/HTTP2。
- 空代理自动继承环境代理。
- 将具体 CLI 版本的大型 system prompt、工具和 Header 永久写死。
- 任意模板控制认证和完整目标 URL。
- 多实例数据库锁、赞助商/公共目录等网站特性。
- 无法证明合规的 OAuth/设备身份伪装。

## 32. 文档与归因

- 在实现阶段记录 Relay Pulse MIT 参考提交和概念映射。
- OpenAI Responses 与 streaming 按 OpenAI 官方协议实现，不以参考项目 fixture 代替官方规范。
- Anthropic、Gemini、xAI/Grok adapter 在实现时记录所依据的官方协议版本和验证日期。
- CLI profile 记录来源、版本和验证方法，但不得纳入任何真实凭据或账号数据。
- 本规范是待实施 Draft；详细执行顺序见 `docs/superpowers/plans/2026-07-29-status-monitoring-refactor.md`，完成时更新 PRODUCT_MODEL 和 PROJECT_PLAN。

固定参考：

- Relay Pulse：<https://github.com/prehisle/relay-pulse/tree/c62537085f4202f6f1f28716f45c107303f2836f>
- OpenAI Responses 迁移：<https://developers.openai.com/api/docs/guides/migrate-to-responses>
- OpenAI Responses streaming：<https://developers.openai.com/api/docs/guides/streaming-responses>
