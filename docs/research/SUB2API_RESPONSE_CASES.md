# Sub2API 客户端响应情况

## 调查范围

本文基于 Sub2API `main` 最新源码提交 `1e618dbc299fc0a82e9a690bcf2d5843be817113`（2026-08-11）。

范围是公开网关接口：

- `/v1/*`
- `/responses`、`/chat/completions`、`/messages/count_tokens`
- `/v1beta/*`
- `/antigravity/v1/*`、`/antigravity/v1beta/*`
- `/backend-api/codex/*`
- Images、Videos、TTS、STT、Realtime、Web Search、SSE、WebSocket

管理后台 `/api/v1/admin/*`、登录页面、支付面板和内部管理接口不属于中转客户端协议，因此不在本文主表内。

本文按客户端最终看到的 `状态码 + type + code + message` 归并。四项中任意一项不同，才视为不同情况。

## 记号说明

- `缺失`：该字段没有输出。
- `动态`：由上游响应、配置或运行时错误决定，源码不存在有限字符串集合。
- `原样上游`：Sub2API 允许透传上游内容。
- `客户端不可见`：Sub2API 在内部完成了重试或切换账号，没有向客户端发送这条错误。
- SSE 已经开始后，HTTP 状态码通常已经固定为 `200`，错误通过事件帧传递。
- WebSocket 升级成功后不再有 HTTP 状态码，状态码列表示 WebSocket Close Code。
- Anthropic 行中的 `error（内层 xxx）` 表示外层 JSON 的 `type` 是 `error`，内层 `error.type` 是 `xxx`。

## Capacity 与降载流程

```text
上游 capacity / server_is_overloaded / slow_down
                         |
                         v
             Sub2API 判定为瞬时错误
                         |
              同账号重试 -> 切换账号
                    |             |
                  成功          失败/流已开始
             客户端无感       返回最终错误
                              SSE code 改为 server_error
```

`server_is_overloaded` 和 `slow_down` 在流已经产生有效输出后会被改写成 `server_error`。Codex 将这两个原始 code 视为致命错误，而 `server_error` 会进入客户端内置退避重试。

## 认证与入口

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| 普通 API Key 认证频率限制 | 429 | 缺失 | `INVALID_AUTH_RATE_LIMITED` | `Too many invalid authentication attempts; retry later` |
| 普通 API Key 不存在 | 401 | 缺失 | `INVALID_API_KEY` | `Invalid API key` |
| query 中携带 API Key | 400 | 缺失 | `api_key_in_query_deprecated` | `API key in query parameter is deprecated. Please use Authorization header instead.` |
| 未提供 API Key | 401 | 缺失 | `API_KEY_REQUIRED` | `API key is required in Authorization header (Bearer scheme), x-api-key header, or x-goog-api-key header` |
| API Key 认证服务过载 | 503 | 缺失 | `API_KEY_AUTH_OVERLOADED` | `API key authentication is temporarily unavailable` |
| API Key 校验内部错误 | 500 | 缺失 | `INTERNAL_ERROR` | `Failed to validate API key` |
| API Key 已禁用 | 401 | 缺失 | `API_KEY_DISABLED` | `API key is disabled` |
| IP 限制 | 403 | 缺失 | `ACCESS_DENIED` | `Access denied. Your IP is {IP}` |
| Key 关联用户不存在 | 401 | 缺失 | `USER_NOT_FOUND` | `User associated with API key not found` |
| 用户未激活 | 401 | 缺失 | `USER_INACTIVE` | `User account is not active` |
| 分组已删除 | 403 | 缺失 | `GROUP_DELETED` | `API Key 所属分组已删除` |
| 分组已停用 | 403 | 缺失 | `GROUP_DISABLED` | `API Key 所属分组已停用` |
| 专属分组授权失效 | 403 | 缺失 | `GROUP_NOT_ALLOWED` | `API Key 所属专属分组不再允许当前用户使用` |
| 无有效订阅 | 403 | 缺失 | `SUBSCRIPTION_NOT_FOUND` | `No active subscription found for this group` |
| Key 已过期 | 403 | 缺失 | `API_KEY_EXPIRED` | `API key 已过期` |
| 余额不足 | 403 | 缺失 | `INSUFFICIENT_BALANCE` | `Insufficient account balance` |
| 订阅窗口维护失败 | 500 | 缺失 | `SUBSCRIPTION_MAINTENANCE_FAILED` | `Failed to maintain subscription usage windows` |
| 订阅校验失败 | 403 | 缺失 | `SUBSCRIPTION_INVALID` | 动态 |
| 使用限制 | 429 | 缺失 | `USAGE_LIMIT_EXCEEDED` | 动态 |
| Key 配额耗尽 | 429 | 缺失 | `API_KEY_QUOTA_EXHAUSTED` | `API key 额度已用完` |
| Responses/Codex Key 配额耗尽 | 429 | `insufficient_quota` | `insufficient_quota` | `API key 额度已用完` |
| Anthropic 未分组 Key | 403 | `error（内层 permission_error）` | 缺失 | `API Key is not assigned to any group and cannot be used. Please contact the administrator to assign it to a group.` |
| Gemini 未分组 Key | 403 | 缺失 | `403` | `API Key is not assigned to any group and cannot be used. Please contact the administrator to assign it to a group.` |

## Gemini 认证格式

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| Gemini 认证频率限制 | 429 | 缺失 | `429` | `Too many invalid authentication attempts; retry later` |
| Gemini Key 无效 | 401 | 缺失 | `401` | `Invalid API key` |
| Gemini query key 已弃用 | 400 | 缺失 | `400` | `Query parameter api_key is deprecated. Use Authorization header or key instead.` |
| Gemini 未提供 Key | 401 | 缺失 | `401` | `API key is required` |
| Gemini 认证服务过载 | 503 | 缺失 | `503` | `API key authentication is temporarily unavailable` |
| Gemini Key 校验错误 | 500 | 缺失 | `500` | `Failed to validate API key` |
| Gemini Key 已禁用 | 401 | 缺失 | `401` | `API key is disabled` |
| Gemini IP 限制 | 403 | 缺失 | `403` | `Access denied. Your IP is {IP}` |
| Gemini 用户不存在 | 401 | 缺失 | `401` | `User associated with API key not found` |
| Gemini 用户未激活 | 401 | 缺失 | `401` | `User account is not active` |
| Gemini 分组删除或停用 | 403 | 缺失 | `403` | `API Key 所属分组已删除` 或 `API Key 所属分组已停用` |
| Gemini 专属分组授权失效 | 403 | 缺失 | `403` | `API Key 所属专属分组不再允许当前用户使用` |
| Gemini 无有效订阅 | 403 | 缺失 | `403` | `No active subscription found for this group` |
| Gemini Key 过期 | 403 | 缺失 | `403` | `API key 已过期` |
| Gemini Key 配额耗尽 | 429 | 缺失 | `429` | `API key 额度已用完` |
| Gemini 订阅维护失败 | 500 | 缺失 | `500` | `Failed to maintain subscription usage windows` |
| Gemini 订阅校验失败 | 403 | 缺失 | `403` | 动态 |
| Gemini 使用限制 | 429 | 缺失 | `429` | 动态 |
| Gemini 余额不足 | 403 | 缺失 | `403` | `Insufficient account balance` |

## 普通请求校验

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| 用户上下文不存在 | 500 | `api_error` | 缺失 | `User context not found` |
| 服务依赖不可用 | 503 | `api_error` | 缺失 | `Service temporarily unavailable` |
| 请求体超过限制 | 413 | `invalid_request_error` | 缺失 | `Request body too large, limit is {limit}` |
| 读取请求体失败 | 400 | `invalid_request_error` | 缺失 | `Failed to read request body` |
| 请求体为空 | 400 | `invalid_request_error` | 缺失 | `Request body is empty` |
| JSON 解析失败 | 400 | `invalid_request_error` | 缺失 | `Failed to parse request body` |
| 缺少 model | 400 | `invalid_request_error` | 缺失 | `model is required` |
| Composite 模型不支持 | 400 | `invalid_request_error` | 缺失 | `Model is not supported by composite groups` |
| OpenAI 兼容模型不支持 | 400 | `invalid_request_error` | 缺失 | `Model is not supported by this OpenAI-compatible endpoint for composite groups` |
| stream 字段类型错误 | 400 | `invalid_request_error` | 缺失 | `invalid stream field type` |
| Responses 子路径不支持 | 404 | `not_found_error` | 缺失 | `Unsupported responses subpath` |
| Composite 路由解析失败 | 500 | `server_error` | 缺失 | `Failed to resolve composite model route` |
| Embeddings 功能未开放 | 404 | `not_found_error` | 缺失 | `Embeddings API is not supported for this platform` |
| Images 功能未开放 | 404 | `not_found_error` | 缺失 | `Images API is not supported for this platform` |
| Videos 功能未开放 | 404 | `not_found_error` | 缺失 | `Videos API is not supported for this platform` |
| Voice 功能未开放 | 404 | `not_found_error` | 缺失 | `Voice API is not supported for this platform` |
| Realtime 功能未开放 | 404 | `not_found_error` | 缺失 | `Realtime API is not supported for this platform` |
| Web Search 功能未开放 | 404 | `not_found_error` | 缺失 | `Web Search API is not supported for this platform` |
| 图像权限不足 | 403 | `permission_error` | 缺失 | `Image generation is not enabled for this group` |
| `/v1/messages` 调度未开放 | 403 | `permission_error`（Anthropic 内层） | 缺失 | `This group does not allow /v1/messages dispatch` |
| Claude Code 专属分组被其他端点访问 | 403 | `permission_error` | 缺失 | `This group is restricted to Claude Code clients (/v1/messages only)` |
| Chat Completions 不支持该模型 | 400 | `invalid_request_error` | 缺失 | `This model is not supported on the Chat Completions endpoint` |
| `previous_response_id` 类型错误 | 400 | `invalid_request_error` | 缺失 | `previous_response_id must be a response.id (resp_*), not a message id` |
| HTTP 不支持 `previous_response_id` | 400 | `invalid_request_error` | 缺失 | `previous_response_id is only supported on Responses WebSocket v2` |
| compact 请求归一化失败 | 400 | `invalid_request_error` | 缺失 | `Failed to normalize compact request body` |
| function call 缺少 call_id | 400 | `invalid_request_error` | 缺失 | `function_call_output requires call_id on HTTP requests; continuation via previous_response_id is only supported on Responses WebSocket v2` |
| function call 缺少 item_reference | 400 | `invalid_request_error` | 缺失 | `function_call_output requires item_reference ids matching each call_id on HTTP requests; continuation via previous_response_id is only supported on Responses WebSocket v2` |

## Responses 兼容层

Responses 兼容格式只有 `error.code` 和 `error.message`，没有 `error.type`。

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| 认证失败 | 401 | 缺失 | `authentication_error` | `Invalid API key` |
| 用户上下文不存在 | 500 | 缺失 | `api_error` | `User context not found` |
| 请求体读取失败 | 400 | 缺失 | `invalid_request_error` | `Failed to read request body` |
| 请求体为空 | 400 | 缺失 | `invalid_request_error` | `Request body is empty` |
| 请求体解析失败 | 400 | 缺失 | `invalid_request_error` | `Failed to parse request body` |
| 缺少模型 | 400 | 缺失 | `invalid_request_error` | `model is required` |
| 模型不支持 | 400 | 缺失 | `invalid_request_error` | `Model is not supported by composite groups` |
| Claude Code 限制 | 403 | 缺失 | `permission_error` | `This group is restricted to Claude Code clients (/v1/messages only)` |
| 无可用账号 | 503 | 缺失 | `api_error` | `No available accounts` |
| 无可用账号及原因 | 503 | 缺失 | `api_error` | `No available accounts: {动态选择错误}` |
| 所有候选账号被利润控制拒绝 | 503 | 缺失 | `api_error` | `No available accounts: all candidates rejected by group profit control` |
| 所有账号耗尽 | 502 | 缺失 | `server_error` | `All available accounts exhausted` |
| compact 无账号支持 | 503 | 缺失 | `compact_not_supported` | `No available accounts support /responses/compact` |
| Antigravity 兼容服务未配置 | 502 | 缺失 | `upstream_error` | `Antigravity compatibility service is not configured` |
| 模型没有任何账号支持 | 404 | 缺失 | `model_not_found` | `Model "{model}" is not supported by any configured account in this group` |

## 计费、并发与调度

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| 计费服务不可用（OpenAI/Chat） | 503 | `billing_service_error` | 缺失 | `Billing service temporarily unavailable. Please retry later.` |
| 计费服务不可用（Responses） | 503 | 缺失 | `billing_service_error` | `Billing service temporarily unavailable. Please retry later.` |
| 计费限制（OpenAI/Chat） | 429 | `rate_limit_exceeded` | 缺失 | 动态：Key 5h/1d/7d、RPM、用户/分组配额等 |
| 计费限制（Responses） | 429 | 缺失 | `rate_limit_exceeded` | 动态 |
| 计费错误 | 403 | `billing_error` | 缺失 | `Billing error` 或动态 |
| 用户并发槽位满 | 429 | `rate_limit_error` | 缺失 | `Concurrency limit exceeded for user, please retry later` |
| 账号并发槽位满 | 429 | `rate_limit_error` | 缺失 | `Concurrency limit exceeded for account, please retry later` |
| 等待队列满 | 429 | `rate_limit_error` | 缺失 | `Too many pending requests, please retry later` |
| 客户端取消请求 | 499 | `api_error` | 缺失 | `context canceled` |
| 服务并发不可用 | 503 | `api_error` | 缺失 | `Service temporarily unavailable, please retry later` |
| 图像并发限制 | 429 | `rate_limit_error` | 缺失 | `Image generation concurrency limit exceeded, please retry later` |
| 无可用账号 | 503 | `api_error` | 缺失 | `No available accounts` |
| 无可用账号及选择原因 | 503 | `api_error` | 缺失 | `No available accounts: {动态选择错误}` |
| 利润控制耗尽 | 503 | `api_error` | 缺失 | `No available accounts: all candidates rejected by group profit control` |

## 上游错误与 failover 耗尽

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| 上游认证失败，OpenAI 原生路径 | 502 | `upstream_error` | 缺失 | `Upstream authentication failed, please contact administrator` |
| 上游权限失败，OpenAI 原生路径 | 502 | `upstream_error` | 缺失 | `Upstream access forbidden, please contact administrator` |
| 上游限流，OpenAI 原生路径 | 429 | `rate_limit_error` | 缺失 | `Upstream rate limit exceeded, please retry later` |
| 上游 529，OpenAI 原生路径 | 503 | `upstream_error` | 缺失 | `Upstream service overloaded, please retry later` |
| 上游 500/502/503/504 | 502 | `upstream_error` | 缺失 | `Upstream service temporarily unavailable` |
| 其他上游错误 | 502 | `upstream_error` | 缺失 | `Upstream request failed` |
| 上游支付问题 | 502 | `upstream_error` | 缺失 | `Upstream payment required: insufficient balance or billing issue` |
| Anthropic/Gemini 兼容路径上游 529 | 503 | `overloaded_error` 或 Gemini 数字错误 | 缺失或 `529` | `Upstream service overloaded, please retry later` |
| 上游确定性 400 | 400 | 上游 `error.type` 或 `invalid_request_error` | 上游 `error.code` 或缺失 | 上游 `error.message` 或 `Upstream rejected the request` |
| 上游上下文长度错误 | 400 | `invalid_request_error` | 动态，常见 `context_length_exceeded` | 动态上游消息 |
| 请求体过大且所有账号都失败 | 413 | `invalid_request_error` | 缺失 | `Request payload is too large` |
| Responses failover 全部耗尽 | 动态，通常 502 | 缺失 | `server_error` | `All available accounts exhausted` |
| Chat failover 全部耗尽 | 动态，通常 502 | `server_error` | 缺失 | `All available accounts exhausted` |
| 自定义错误透传规则命中 | 任意配置值 | `upstream_error` 或动态 | 任意配置值或缺失 | 上游或自定义 message |
| 上游原始 body 直接透传 | 任意上游状态码 | 任意上游 type | 任意上游 code | 任意上游 message |

## Capacity、server_is_overloaded、slow_down

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| HTTP 400 `Selected model is at capacity...`，重试或换账号成功 | 不发送 | 不发送 | 不发送 | 客户端不可见 |
| HTTP 400 含 `You can retry your request... help.openai.com ... request id`，重试成功 | 不发送 | 不发送 | 不发送 | 客户端不可见 |
| HTTP/SSE `server_is_overloaded`，首个有效输出前 failover 成功 | 不发送 | 不发送 | 不发送 | 客户端不可见 |
| HTTP/SSE `slow_down`，首个有效输出前 failover 成功 | 不发送 | 不发送 | 不发送 | 客户端不可见 |
| 流已开始，普通 SSE `event:error` | 200 | 上游 `error.type` 原样，常见 `service_unavailable_error` | `server_error` | 原始上游 message |
| 流已开始，Responses `response.failed` | 200 | 缺失（顶层 `type=response.failed`） | `server_error` | 原始上游 message |
| 自定义透传保留 `server_is_overloaded` | 200 | 动态 | `server_is_overloaded` | 原始 message |
| 自定义透传保留 `slow_down` | 200 | 动态 | `slow_down` | 原始 message |
| Capacity failover 耗尽，普通 JSON | 502 | `upstream_error` | 缺失 | `Upstream request failed` |
| Capacity failover 耗尽，Responses JSON | 动态，通常 502 | 缺失 | `server_error` | `All available accounts exhausted` |
| Capacity failover 耗尽，Chat JSON | 动态，通常 502 | `server_error` | 缺失 | `All available accounts exhausted` |

## SSE

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| OpenAI/Chat 普通 SSE `event: error` | 200 | 动态错误类型 | 缺失 | 动态 |
| OpenAI/Chat SSE 上游错误 | 200 | `upstream_error` | 缺失 | `Upstream request failed` 或动态 |
| Responses `response.failed`，限流 | 200 | 缺失（顶层 `response.failed`） | `rate_limit_exceeded` | 动态 |
| Responses `response.failed`，非法请求 | 200 | 缺失 | `invalid_request` | 动态 |
| Responses `response.failed`，权限错误 | 200 | 缺失 | `permission_denied` | 动态 |
| Responses `response.failed`，认证错误 | 200 | 缺失 | `authentication_failed` | 动态 |
| Responses `response.failed`，上游错误 | 200 | 缺失 | `upstream_error` | 动态 |
| Responses `response.failed`，服务器错误 | 200 | 缺失 | `server_error` | 动态 |
| Responses 未知错误类型 | 200 | 缺失 | 原样内部类型 | 动态 |
| Anthropic SSE `event: error` | 200 | `error`（内层动态） | 缺失 | 动态 |
| Gemini SSE 错误 | 200 | 缺失 | 数字 HTTP 状态 | 动态 |
| SSE 已开始但读取上游失败 | 200 | `upstream_error` | 动态或缺失 | 动态 |

## WebSocket 升级前

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| 未提供 WebSocket Upgrade | 426 | `invalid_request_error` | 缺失 | `WebSocket upgrade required (Upgrade: websocket)` |
| WebSocket API Key 无效 | 401 | `authentication_error` | 缺失 | `Invalid API key` |
| WebSocket 用户上下文不存在 | 500 | `api_error` | 缺失 | `User context not found` |
| WebSocket ingress 容量不可用 | 503 | `service_unavailable` | 缺失 | `WebSocket ingress capacity is temporarily unavailable` |
| WebSocket 连接数超过限制 | 429 | `rate_limit_error` | 缺失 | `Too many open WebSocket connections, please retry later` |

## WebSocket 升级后

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| 缺少首个 `response.create` | WS close code 1008 | 缺失 | 缺失 | `missing first response.create message` |
| 空请求帧 | WS close code 1008 | 缺失 | 缺失 | `empty websocket request payload` |
| JSON 无效 | WS close code 1008 | 缺失 | 缺失 | `invalid JSON payload` 或 `invalid websocket request payload` |
| 不支持的消息类型 | WS close code 1008 | 缺失 | 缺失 | `unsupported websocket message type` |
| 不支持的请求类型 | WS close code 1008 | 缺失 | 缺失 | `unsupported websocket request type: {type}` |
| `response.append` 不支持 | WS close code 1008 | 缺失 | 缺失 | `response.append is not supported in ws v2; use response.create with previous_response_id` |
| 首个请求缺少模型 | WS close code 1008 | 缺失 | 缺失 | `model is required in first response.create payload` |
| 后续请求缺少模型 | WS close code 1008 | 缺失 | 缺失 | `model is required in response.create payload` |
| Composite 模型不兼容 | WS close code 1008 | 缺失 | 缺失 | `Responses WebSocket API only supports OpenAI-compatible models for composite groups` |
| `previous_response_id` 是 message id | WS close code 1008 | 缺失 | 缺失 | `previous_response_id must be a response.id (resp_*), not a message id` |
| WebSocket 模式关闭 | WS close code 1008 | 缺失 | 缺失 | `websocket mode is disabled for this account` |
| WebSocket 模式配置非法 | WS close code 1008 | 缺失 | 缺失 | `websocket mode only supports ctx_pool/passthrough/http_bridge` |
| 图像权限不足 | WS close code 1008 | 缺失 | 缺失 | `Image generation is not enabled for this group` |
| Cyber 会话屏蔽 | WS close code 1008 | 缺失 | 缺失 | `session blocked by cyber-security policy` |
| 用户并发槽位获取失败 | WS close code 1011 | 缺失 | 缺失 | `failed to acquire user concurrency slot` |
| 用户并发已满 | WS close code 1013 | 缺失 | 缺失 | `too many concurrent requests, please retry later` |
| 账号并发槽位获取失败 | WS close code 1011 | 缺失 | 缺失 | `failed to acquire account concurrency slot` |
| 账号忙 | WS close code 1013 | 缺失 | 缺失 | `account is busy, please retry later` |
| 没有可用账号 | WS close code 1013 | 缺失 | 缺失 | `no available account` |
| 计费检查失败 | WS close code 1008 | 缺失 | 缺失 | `billing check failed` |
| 获取上游 Token 失败 | WS close code 1011 | 缺失 | 缺失 | `failed to get access token` |
| 上游连接忙 | WS close code 1013 | 缺失 | 缺失 | `upstream websocket is busy, please retry later` |
| 上游续链连接不可用 | WS close code 1008 | 缺失 | 缺失 | `upstream continuation connection is unavailable; please restart the conversation` |
| 账号不再适合当前连接 | WS close code 1013 | 缺失 | 缺失 | `account is no longer eligible for this connection, please reconnect` |
| ingress lease 丢失 | WS close code 1013 | 缺失 | 缺失 | `websocket ingress capacity lease lost; please reconnect` |
| 上游限流 | WS close code 1013 | 缺失 | 缺失 | `upstream rate limit exceeded, please retry later` |
| 上游服务暂时不可用 | WS close code 1013 | 缺失 | 缺失 | `upstream service temporarily unavailable` |
| 上游 WebSocket 认证失败 | WS close code 1008 | 缺失 | 缺失 | `upstream websocket authentication failed` |
| 上游代理失败 | WS close code 1011 | 缺失 | 缺失 | `upstream websocket proxy failed` |
| 模型切换要求重连 | WS close code 1008 | 缺失 | 缺失 | `model switch requires reconnect` |
| 正常关闭 | WS close code 1000 | 缺失 | 缺失 | 空或动态 |

## WebSocket 文本错误帧

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| 上游普通文本错误帧 | 无 HTTP 状态 | 动态 | 动态 | 动态 |
| 上游 `error` JSON 帧 | 无 HTTP 状态 | 动态 | 动态 | 动态 |
| Capacity 文本帧 | 无 HTTP 状态 | 动态，常见 `service_unavailable_error` | `server_error` | 原始 message |
| 内容审核阻断 | 无 HTTP 状态 | `error`（内层 `invalid_request_error`） | `content_policy_violation` | `content moderation blocked this request` |
| Cyber 阻断 | 无 HTTP 状态 | `error`（内层 `permission_error`） | `session_blocked_by_cyber_policy` | `This session is blocked by cyber-security policy, please start a new session` |

## Gemini 原生 API

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| 分组平台不是 Gemini | 400 | 缺失 | `400` | `API key group platform is not gemini` |
| URL 缺少模型 | 400 | 缺失 | `400` | `Missing model in URL` |
| URL 模型非法 | 400 | 缺失 | `400` | `Invalid model in URL` |
| 路径缺失 | 404 | 缺失 | `404` | `missing path` |
| 模型动作路径非法 | 404 | 缺失 | `404` | `invalid model action path` |
| 请求体过大 | 413 | 缺失 | `413` | `Request body too large, limit is {limit}` |
| 读取请求体失败 | 400 | 缺失 | `400` | `Failed to read request body` |
| 请求体为空 | 400 | 缺失 | `400` | `Request body is empty` |
| Gemini 并发限制 | 429 | 缺失 | `429` | 动态并发错误 |
| 等待队列满 | 429 | 缺失 | `429` | `Too many pending requests, please retry later` |
| 无可用 Gemini 账号 | 503 | 缺失 | `503` | `No available Gemini accounts` |
| 无可用 Gemini 账号及原因 | 503 | 缺失 | `503` | `No available Gemini accounts: {动态选择错误}` |
| 上游认证失败 | 502 | 缺失 | `502` | `Upstream authentication failed, please contact administrator` |
| 上游权限失败 | 502 | 缺失 | `502` | `Upstream access forbidden, please contact administrator` |
| 上游限流 | 429 | 缺失 | `429` | `Upstream rate limit exceeded, please retry later` |
| 上游 529 | 503 | 缺失 | `503` | `Upstream service overloaded, please retry later` |
| 上游 500/502/503/504 | 502 | 缺失 | `502` | `Upstream service temporarily unavailable` |
| 其他上游错误 | 502 | 缺失 | `502` | `Upstream request failed` |
| 上游响应为空 | 502 | 缺失 | `502` | `Empty upstream response` |
| Gemini 上游 body 透传 | 动态 | 缺失 | 数字状态码 | 动态 |

## 图像、视频、语音和 Live

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| 图片模型缺失 | 400 | `invalid_request_error` | 缺失 | `images endpoint requires an image model` |
| 图片模型非法 | 400 | `invalid_request_error` | 缺失 | `images endpoint requires an image model, got "{model}"` |
| `n` 类型错误 | 400 | `invalid_request_error` | 缺失 | `invalid n field type` |
| `n` 小于等于 0 | 400 | `invalid_request_error` | 缺失 | `n must be greater than 0` |
| 图片字段类型错误 | 400 | `invalid_request_error` | 缺失 | `invalid images field type` |
| multipart Content-Type 错误 | 400 | `invalid_request_error` | 缺失 | `invalid multipart content-type: ...` |
| multipart boundary 缺失 | 400 | `invalid_request_error` | 缺失 | `multipart boundary is required` |
| multipart 字段读取失败 | 400 | `invalid_request_error` | 缺失 | `read multipart field ...: ...` |
| 图片文件缺失 | 400 | `invalid_request_error` | 缺失 | `image file is required` |
| 异步任务未启用 | 404 | `not_found_error` | `not_found_error` | `async image tasks are not enabled` |
| 异步任务权限不足 | 403 | `permission_error` | `permission_error` | 图像权限消息 |
| 异步任务网关不可用 | 500 | `api_error` | `api_error` | `image gateway is unavailable` |
| 异步图像请求禁止 stream | 400 | `invalid_request_error` | `invalid_request_error` | `streaming image requests cannot be submitted as asynchronous tasks` |
| 异步任务超时 | 504 | `timeout_error` | 缺失 | `image generation task timed out` |
| 上游返回非法图片响应 | 502 | `api_error` | 缺失 | `upstream returned an invalid image response` |
| Grok 媒体无可用账号 | 503 | `grok_media_no_eligible_account` | 缺失 | `No eligible Grok media accounts` |
| 视频请求不存在 | 404 | `not_found_error` | 缺失 | `Video request not found` |
| Live 不支持当前平台 | 404 | `not_found_error` | 缺失 | `Live is not supported for this platform` |
| Live 未对分组开放 | 403 | `permission_error` | 缺失 | `Live is not enabled for this group` |
| Live 并发不可用 | 503 | `api_error` | 缺失 | `Live concurrency unavailable` |
| Live 并发已满 | 429 | `rate_limit_error` | 缺失 | `Live concurrency limit reached` |
| Live 服务不可用 | 503 | `api_error` | 缺失 | `Live is unavailable` |
| Live 上游拒绝 | 动态 | `invalid_request_error` | 缺失 | `Live upstream rejected the request` |
| Live 上游失败 | 502 | `api_error` | 缺失 | `Live upstream request failed` |
| Live 呼叫不属于当前身份 | 403 | `permission_error` | 缺失 | `Live call belongs to another identity` |
| Live 呼叫不存在 | 404 | `not_found_error` | 缺失 | `Live call not found` |
| Voice 不支持当前平台 | 404 | `not_found_error` | 缺失 | `Voice API is not supported for this platform` |
| Realtime 非 WebSocket 请求 | 426 | `invalid_request_error` | 缺失 | `WebSocket upgrade required (Upgrade: websocket)` |
| 无可用 Grok 账号 | 503 | `api_error` | 缺失 | `No available Grok accounts` |
| Grok 凭据不可用 | 502 | `upstream_error` | 缺失 | `Grok credential unavailable` |

## Web Search

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| 请求 JSON 校验失败 | 400 | `invalid_request_error` | 缺失 | Gin `ShouldBindJSON` 动态错误 |
| 未提供 API Key | 401 | `authentication_error` | 缺失 | `API key required` |
| 非 Grok 分组 | 400 | `invalid_request_error` | 缺失 | `web search is only supported for grok groups` |
| 分组缺失 | 400 | `invalid_request_error` | 缺失 | `group required` |
| 调度失败 | 503 | `scheduling_error` | 缺失 | 动态选择错误 |
| 无可用账号 | 503 | `scheduling_error` | 缺失 | `No available accounts` |
| 上游搜索失败 | 502 | `web_search_error` | 缺失 | 动态底层错误 |
| 安全审计阻断 | 动态 | 动态 | 动态 | 动态 |

## Codex models

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| Key 未绑定分组 | 401 | `invalid_request_error` | 缺失 | `API key group is required` |
| 非 OpenAI 分组 | 404 | `not_found_error` | 缺失 | `Codex models manifest is only available for OpenAI groups` |
| 无 OpenAI 账号 | 503 | `upstream_error` | 缺失 | `No available OpenAI accounts` |
| manifest 上游错误 | 动态 | `upstream_error` | 缺失 | 动态基础设施错误 |

## 安全审计与 Cyber

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| OpenAI 传统内容审核 | 动态 | 动态 | 动态，常见 `content_policy_violation` | 动态，常见 `content moderation blocked this request` |
| OpenAI 新安全策略阻断 | 动态 | `permission_error` | 动态 | 动态 |
| OpenAI 安全策略非阻断错误 | 动态 | `api_error` | 动态 | 动态 |
| Responses 安全策略错误 | 动态 | `api_error` 或缺失 | 动态 | 动态 |
| Anthropic 安全策略错误 | 动态 | `error`（内层动态） | 动态或缺失 | 动态 |
| Gemini 安全策略错误 | 动态 | 缺失 | 数字 HTTP 状态 | 动态 |
| 本地 Cyber 会话阻断 | 403 | `permission_error` | `session_blocked_by_cyber_policy` | `该会话已被网络安全策略屏蔽，请开启新会话 / This session is blocked by cyber-security policy, please start a new session` |
| Cyber 阻断后的 Responses SSE | 200 | 缺失（顶层 `response.failed`） | `permission_denied` | 同上 |

## 批量图像接口

批量图像统一使用：

```json
{
  "error": {
    "type": "invalid_request_error",
    "code": "具体 BATCH_IMAGE_*",
    "message": "..."
  }
}
```

| 场景 | 状态码 | type | code | message |
|---|---:|---|---|---|
| 任务不存在 | 404 | `invalid_request_error` | `BATCH_IMAGE_NOT_FOUND` | `batch image job not found` |
| 任务已存在 | 409 | `invalid_request_error` | `BATCH_IMAGE_JOB_EXISTS` | `batch image job already exists` |
| 条目已存在 | 409 | `invalid_request_error` | `BATCH_IMAGE_ITEM_EXISTS` | `batch image item already exists` |
| 状态转换非法 | 400 | `invalid_request_error` | `BATCH_IMAGE_INVALID_TRANSITION` | `invalid batch image job status transition` |
| Provider 非法 | 400 | `invalid_request_error` | `BATCH_IMAGE_INVALID_PROVIDER` | `invalid batch image provider` |
| Provider job name 缺失 | 400 | `invalid_request_error` | `BATCH_IMAGE_MISSING_PROVIDER_JOB_NAME` | `batch image provider job name is missing` |
| Account ID 缺失 | 400 | `invalid_request_error` | `BATCH_IMAGE_MISSING_ACCOUNT_ID` | `batch image account id is missing` |
| Provider 不支持 | 400 | `invalid_request_error` | `BATCH_IMAGE_UNSUPPORTED_PROVIDER` | `unsupported batch image provider` |
| Provider 输出缺失 | 502 | `invalid_request_error` | `BATCH_IMAGE_INDEX_OUTPUT_MISSING` | `batch image provider output is missing` |
| Provider 输出解析失败 | 502 | `invalid_request_error` | `BATCH_IMAGE_INDEX_PARSE_FAILED` | `batch image provider output parse failed` |
| Provider 没有结果行 | 502 | `invalid_request_error` | `BATCH_IMAGE_INDEX_NO_RESULT_LINES` | `batch image provider output has no result lines` |
| 输出 custom_id 重复 | 502 | `invalid_request_error` | `DUPLICATE_CUSTOM_ID_IN_OUTPUT` | `batch image provider output contains duplicate custom id` |
| 索引状态冲突 | 409 | `invalid_request_error` | `BATCH_IMAGE_INDEX_STATE_CONFLICT` | `batch image job is no longer in indexing state` |
| 结算状态非法 | 400 | `invalid_request_error` | `BATCH_IMAGE_SETTLEMENT_INVALID_STATUS` | `batch image job is not ready for settlement` |
| 结算 manifest 冲突 | 409 | `invalid_request_error` | `BATCH_IMAGE_SETTLEMENT_MANIFEST_CONFLICT` | `batch image settlement manifest hash conflict` |
| 结算定价缺失 | 400 | `invalid_request_error` | `BATCH_IMAGE_SETTLEMENT_PRICING_MISSING` | `batch image settlement pricing is missing` |
| 结算计费失败 | 502 | `invalid_request_error` | `BATCH_IMAGE_SETTLEMENT_BILLING_FAILED` | `batch image settlement billing failed` |
| 已结算 | 409 | `invalid_request_error` | `BATCH_IMAGE_ALREADY_SETTLED` | `batch image job is already settled` |
| 结算 Key ID 缺失 | 400 | `invalid_request_error` | `BATCH_IMAGE_SETTLEMENT_MISSING_API_KEY_ID` | `batch image settlement api key id is missing` |
| 结算账号 ID 缺失 | 400 | `invalid_request_error` | `BATCH_IMAGE_SETTLEMENT_MISSING_ACCOUNT_ID` | `batch image settlement account id is missing` |
| 结算计数非法 | 400 | `invalid_request_error` | `BATCH_IMAGE_SETTLEMENT_INVALID_COUNTS` | `batch image settlement counts are invalid` |
| 结算成本超过冻结额 | 409 | `invalid_request_error` | `BATCH_IMAGE_SETTLEMENT_COST_EXCEEDS_HOLD` | `batch image settlement cost exceeds held balance` |
| 余额冻结失败 | 502 | `invalid_request_error` | `BATCH_IMAGE_BILLING_HOLD_FAILED` | `batch image balance hold failed` |
| 批量图像余额不足 | 402 | `invalid_request_error` | `BATCH_IMAGE_INSUFFICIENT_BALANCE` | `insufficient balance for batch image hold` |
| 批量图像功能关闭 | 404 | `invalid_request_error` | `BATCH_IMAGE_DISABLED` | `batch image API is disabled` |
| 分组未开放批量图像 | 403 | `invalid_request_error` | `BATCH_IMAGE_GROUP_DISABLED` | `batch image API is disabled for this group` |
| 模型缺失 | 400 | `invalid_request_error` | `BATCH_IMAGE_INVALID_MODEL` | `batch image model is required` |
| 无兼容账号 | 502 | `invalid_request_error` | `BATCH_IMAGE_NO_ACCOUNT_AVAILABLE` | `no compatible batch image account is available` |
| items 非法 | 400 | `invalid_request_error` | `BATCH_IMAGE_INVALID_ITEMS` | `batch image items are invalid` |
| custom_id 重复 | 400 | `invalid_request_error` | `BATCH_IMAGE_DUPLICATE_CUSTOM_ID` | `batch image custom ids must be unique` |
| prompt 过长 | 400 | `invalid_request_error` | `BATCH_IMAGE_PROMPT_TOO_LONG` | `batch image prompt is too long` |
| 参考图非法 | 400 | `invalid_request_error` | `BATCH_IMAGE_INVALID_REFERENCE_IMAGE` | `batch image reference image is invalid` |
| 参考图过多 | 400 | `invalid_request_error` | `BATCH_IMAGE_TOO_MANY_REFERENCE_IMAGES` | `too many batch image reference images for this model` |
| 参考图过大 | 400 | `invalid_request_error` | `BATCH_IMAGE_REFERENCE_IMAGES_TOO_LARGE` | `batch image reference images are too large` |
| 输出图过多 | 400 | `invalid_request_error` | `BATCH_IMAGE_TOO_MANY_OUTPUT_IMAGES` | `too many batch image output images` |
| Provider 提交失败 | 502 | `invalid_request_error` | `BATCH_IMAGE_PROVIDER_SUBMIT_FAILED` | `batch image provider submit failed` |
| 队列失败 | 502 | `invalid_request_error` | `BATCH_IMAGE_QUEUE_FAILED` | `batch image queue failed` |
| 幂等键冲突 | 409 | `invalid_request_error` | `BATCH_IMAGE_IDEMPOTENCY_CONFLICT` | `idempotency key reused with different batch image request` |
| 取消失败 | 502 | `invalid_request_error` | `BATCH_IMAGE_CANCEL_FAILED` | `batch image cancel failed` |
| Vertex GCS bucket 缺失 | 502 | `invalid_request_error` | `BATCH_IMAGE_VERTEX_GCS_BUCKET_MISSING` | `Vertex managed GCS bucket is not configured` |
| 任务未完成 | 409 | `invalid_request_error` | `BATCH_IMAGE_NOT_READY` | `batch image job is not completed` |
| 输出已删除 | 410 | `invalid_request_error` | `BATCH_IMAGE_OUTPUT_DELETED` | `batch image output has been deleted` |
| 条目不存在 | 404 | `invalid_request_error` | `BATCH_IMAGE_ITEM_NOT_FOUND` | `batch image item not found` |
| 条目失败 | 409 | `invalid_request_error` | `BATCH_IMAGE_ITEM_FAILED` | `batch image item did not succeed` |
| 结果缺失 | 500 | `invalid_request_error` | `BATCH_IMAGE_RESULT_MISSING` | `batch image result is missing` |
| 下载频率限制 | 429 | `invalid_request_error` | `BATCH_IMAGE_DOWNLOAD_LIMITED` | `too many batch image downloads` |
| 下载失败 | 500 | `invalid_request_error` | `BATCH_IMAGE_DOWNLOAD_FAILED` | `batch image download failed` |
| 下载内容过大 | 400 | `invalid_request_error` | `BATCH_IMAGE_DOWNLOAD_TOO_LARGE` | `batch image download is too large` |
| image index 越界 | 400 | `invalid_request_error` | `BATCH_IMAGE_ITEM_IMAGE_INDEX_OUT_OF_RANGE` | `batch image item image index is out of range` |
| ZIP 条目过多 | 400 | `invalid_request_error` | `BATCH_IMAGE_ZIP_TOO_MANY_ITEMS` | `batch image ZIP contains too many items; use single item downloads` |
| 输出删除时机错误 | 409 | `invalid_request_error` | `BATCH_IMAGE_OUTPUT_DELETE_NOT_READY` | `batch image output can only be deleted after completion` |
| 记录删除时机错误 | 409 | `invalid_request_error` | `BATCH_IMAGE_RECORD_DELETE_NOT_READY` | `batch image record can only be deleted after the job finishes` |
| 清理失败 | 502 | `invalid_request_error` | `BATCH_IMAGE_CLEANUP_FAILED` | `batch image cleanup failed` |
| 清理路径不安全 | 400 | `invalid_request_error` | `BATCH_IMAGE_CLEANUP_UNSAFE_PATH` | `batch image cleanup path is unsafe` |
| Provider 清理失败 | 502 | `invalid_request_error` | `BATCH_IMAGE_PROVIDER_CLEANUP_FAILED` | `batch image provider cleanup failed` |

## 不能有限枚举的情况

以下源码路径允许任意字符串，因此只能准确列出“动态集合”，不能伪造一个有限的 message 清单：

- 上游错误 body 透传。
- 自定义 error passthrough 规则。
- 安全审计的 code、message 和部分状态码。
- Gemini/Anthropic/OpenAI 上游原始错误。
- 计费校验、账号选择、利润控制和网络错误中的动态错误文本。
- WebSocket 的动态安全审计、请求校验和上游错误原因。

关键源码位置：

- `backend/internal/service/openai_gateway_upstream_errors.go`
- `backend/internal/service/openai_gateway_passthrough.go`
- `backend/internal/handler/stream_error_event.go`
- `backend/internal/handler/openai_gateway_handler.go`
- `backend/internal/server/middleware/api_key_auth.go`
- `backend/internal/server/middleware/api_key_auth_google.go`
- `backend/internal/handler/gemini_v1beta_handler.go`
- `backend/internal/handler/batch_image_handler.go`
