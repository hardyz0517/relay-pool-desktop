# P5 Gateway Design

## Goal

把 Relay Pool Desktop 做成一个本地 OpenAI-compatible 网关，让外部工具只连一个本地地址，后面由 Key 池负责选 key、转发、fallback 和记录日志。

## Core Model

- `Station`: 用户收录的中转站账号资产，不是最终路由对象。
- `Station Key`: 真正参与请求转发、fallback、健康记录的对象。
- `Key Pool`: 所有 `Station Key` 的统一管理视图，也是路由池。
- `Router`: 只负责从 Key 池挑选 key、fallback、写路由结果。
- `Adapter`: 负责把本地 OpenAI-compatible 请求映射到对应上游协议。
- `Gateway`: 对外暴露一个本地 `http://127.0.0.1:<port>/v1` 入口。

## Product Framing

P5 不应被理解成“做一个 `/chat/completions` 转发器”。它的目标是做本地网关。

对外：

- 外部工具只连本地地址
- 兼容 OpenAI-style 请求和错误
- 能被 Codex、VSCode、Chatbox、CCSwitch 等工具当成本地 endpoint 使用

对内：

- 由 Key 池选择 enabled key
- 按 priority fallback
- 记录请求日志
- 回写 key 状态和近期失败信息

对上：

- 不同中转站可能支持不同协议
- 需要先分清客户端协议和上游协议
- 不能把所有上游都假设成同一种实现

## Recommended Architecture

### 1. Gateway Core

负责：

- 本地 HTTP server 生命周期
- `/v1/models`
- `/v1/chat/completions`
- 未来的 `/v1/responses`
- OpenAI-compatible 错误输出
- 请求日志写入

不负责：

- 选 key
- 协议转换细节
- UI 状态展示

### 2. Router

负责：

- 从 Key 池读取 enabled key
- 按 priority 排序
- 选择候选 key
- fallback
- 记录最终命中的 station_key

不负责：

- 上游协议差异
- UI 逻辑
- 持久会话管理

### 3. Adapter Layer

负责：

- 将本地请求映射到具体上游 API
- 识别 station / station key 的上游格式
- 以后支持 `responses`、`chat completions` 或其他兼容入口

不负责：

- Key 池排序
- UI 状态
- 路由 fallback 策略

### 4. Management Surface

负责：

- 中转站页管理站点账号
- Key 池页管理可路由对象
- 请求日志页展示真实网关日志
- 设置页控制代理生命周期

## P5 Phase Split

### P5.0

已完成：

- 本地 HTTP server
- 仅监听 `127.0.0.1`
- start / stop / restart / status
- `/v1/models`
- `/v1/chat/completions` 非流式
- `/v1/chat/completions` SSE 流式透传
- Key 池 enabled + priority 路由
- 真实 request_logs

### P5.1

已完成：

- `POST /v1/responses`
- `Responses` 作为一等入口
- `Chat Completions` 继续兼容
- `upstream_api_format` 可配置
- responses / chat 之间做明确转换边界

### P5.2

已完成基础形态：

- `stream:true` SSE 支持
- chat / responses 双接口流式
- 流式错误边界：成功选中上游后透传，不在中途静默切 key

### P5.3

已完成基础形态：

- 请求日志反哺 Key 池
- 最近使用时间
- 渠道状态从 mock 转为基于 Key 池和 request_logs 的真实轻量统计

后续继续做：

- 长期成功率 / 失败次数
- 健康分和熔断
- 更完整的最近失败原因持久化

### P5.4

已完成基础形态：

- CORS / OPTIONS 兼容
- `/v1/models` 聚合与去重
- base_url 规范化
- OpenAI-style error 统一
- 脱敏 / 稳定性打磨

后续继续做：

- 模型名映射 / 归一化
- 超时配置
- 更细的上游协议选择

## Data Boundaries

- `Station` 存账号和上游来源
- `Station Key` 存可路由授权凭据
- `request_logs` 只存元数据
- 不保存完整 prompt / response / cookie / session / 完整 key

## Routing Rules

基础规则：

1. 从 Key 池取 enabled key
2. 按 priority 排序
3. 取其所属 station 的 upstream
4. 失败时 fallback 到下一候选
5. 成功后写日志并回写 key 近期状态

## Open Questions

- `Responses` 首发时是否需要先支持最小对象重建
- 上游协议是否需要按 station 级别显式配置
- 同一 station 的多个 key 是否只作为授权轮换，还是未来也可能对应不同上游出口
