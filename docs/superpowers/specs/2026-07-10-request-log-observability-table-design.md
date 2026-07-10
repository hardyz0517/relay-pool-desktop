# 请求日志观测表格升级设计

## 目标

将“请求日志”主列表升级为高密度使用记录表，并补齐智能路由与性能分析需要的结构化数据：推理强度、缓存 Token、首字延迟和计费模式。首页“最近使用”保持不变，客户端 IP 不解析、不存储、不展示。

## 表格

列顺序为：密钥、模型、推理强度、端点、分组、类型、计费模式、Token、费用、延迟、状态、时间。

- 密钥显示中转站密钥名称和脱敏值。
- 推理强度显示规范化标签；旧记录或请求未指定时显示 `-`。
- 类型显示流式或非流式。
- Token 分行显示输入、输出、缓存读取和缓存创建，仅隐藏值为零或未知的缓存项。
- 延迟分行显示首字和总耗时；非流式请求的首字为 `-`。
- 宽表格保持稳定最小宽度并横向滚动，不压缩到文字重叠。
- 失败、兜底和成功仍有明确文字状态，颜色不是唯一信息。

## 观测管线

新增纯观测模块，负责请求元数据和响应 usage 的协议归一化。请求侧兼容 `reasoning.effort` 与 `reasoning_effort`。响应侧兼容 OpenAI Responses 和 Chat Completions 的 JSON/SSE usage 形状，并将输入、输出、缓存读取、缓存创建归一化为同一结构。

流式写出不再使用不可观测的 `std::io::copy`。显式分块读取和写入，在首个非空上游数据块成功写给客户端时记录首字延迟，同时把分片交给有界 SSE 观察器。观察器只保留未完成事件的尾部缓冲，不保存完整响应正文。

响应写完后统一生成日志结算结果。非流式和流式请求共享同一 usage 到成本的计算入口，避免两套计价逻辑漂移。流式中断仍保存已知观测值，但生命周期状态保持 `interrupted`。

## 持久化

`request_logs` 增加可空列：

- `reasoning_effort TEXT`
- `cache_creation_tokens INTEGER`
- `cache_read_tokens INTEGER`
- `first_token_ms INTEGER`
- `billing_mode TEXT`

旧记录保持空值，不回填伪造数据。请求日志查询列清单集中为单一常量，降低新增字段时三个 SELECT 列表与行映射错位的风险。

## 代码边界

- `proxy/observability.rs`：纯请求元数据解析、JSON/SSE usage 归一化与测试。
- `proxy/runtime.rs`：网络生命周期编排、首字计时、调用观测器和最终结算。
- `models/proxy.rs`：持久化/API 契约。
- `database.rs`：SQLite 建表、迁移、插入和读取。
- `requestLogViewModels.ts`：日志表格的纯显示映射。
- `RequestLogTable.tsx`：只负责表格渲染。
- `LogsPage.tsx`：保留加载、筛选、清空和开发者详情状态。

## 安全边界

不增加客户端地址字段。代理接收连接时继续丢弃 peer address。观测器不保存请求正文、响应正文、Authorization、Cookie 或未脱敏密钥。

## 验证

- Rust 单元测试覆盖推理强度、两类 usage JSON、跨分片 SSE、缓存 Token 和首字时序结果。
- 数据库测试覆盖迁移后列存在、插入读取往返和旧空值。
- 前端测试覆盖列顺序、无 IP、Token/延迟标签及首页未改动。
- 运行相关 Node 脚本、`cargo test` 聚焦模块、`cargo check`、`pnpm build` 和浏览器宽/窄视口检查。
