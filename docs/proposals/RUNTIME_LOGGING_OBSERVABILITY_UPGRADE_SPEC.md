# 运行日志与本地可观测性重构升级规范

状态：Accepted / Implemented；核心代码、故障合同、诊断导出与工程门禁已完成；packaged marker-I/O fault smoke 的退出挂起仅属于验证 harness 限制，不改变生产降级合同。
日期：2026-08-14
适用范围：Rust/Tauri 后端、React 前端、本地代理、采集、状态监控、后台任务、持久化、导入导出、更新与应用启动/退出
提案类型：跨层安全基础设施重构
替代关系：本规范获批并进入实施后，取代散落的 `println!` / `eprintln!`、无 sink 的 `tracing` 调用、未接线的运行时诊断草图，建立唯一的运行事件、日志落盘、诊断读取和支持包导出路径。它不替代请求日志、采集历史、监控历史、变更/告警记录等业务事实。

参考规范与当前事实：

- `AGENTS.md`
- `docs/README.md`
- `docs/PROJECT_PLAN.md`
- `docs/PRODUCT_MODEL.md`
- `docs/SECURITY_EXPORT_IMPORT.md`
- `src-tauri/src/observability/correlation.rs`
- `src-tauri/src/observability/runtime/`
- `src-tauri/src/observability/metrics.rs`
- `src-tauri/src/services/secrets/mask.rs`
- `docs/audits/runtime-logging-qualification.md`
- `src-tauri/src/persistence/stores/request_log_store.rs`
- `src-tauri/src/persistence/stores/collector_store.rs`
- `src-tauri/src/services/monitoring/runner.rs`
- `src/app/ShellPageErrorBoundary.tsx`

## 1. 执行摘要

Relay Pool Desktop 已经保存请求日志、采集 run/snapshot、监控 execution/attempt/target result、变更与告警等业务事实。这些数据回答“业务动作产生了什么结果”，不等于应用运行日志。

实施前基线是：启动、IPC、后台任务、代理写入、采集调度、监控 runner、持久化故障和前端渲染错误没有同一条安全、可落盘、可关联、可检索的证据链。当前实现已建立 `observability::runtime`、统一 command boundary、JSONL sink、reader 和 support-bundle 受限出口；核心代码合同与工程门禁已闭合。本规范作为当前运行日志实现基线。

本提案建立以下单一架构：

```text
业务事实（SQLite）       请求 / 采集 / 监控 / 告警
运行事件（JSONL 文件）   启动 / IPC / task / transport / persistence / UI
实时指标（有界内存）     延迟 / 队列 / 重试 / 丢弃 / 饱和
          \                 |                 /
           \---------- 安全诊断包 ----------/
```

明确决策：

1. 运行日志使用本地、独立、滚动的 JSONL 文件，不写入业务 SQLite，也不上传网络。
2. 所有生产运行事件必须先成为类型化、安全字段受限的 `RuntimeEvent`，再进入日志 sink；业务模块不得向持久化日志传递任意 `Error`、`Debug` 文本或 `serde_json::Value`。
3. 现有 correlation id 保留并扩展为会话、命令、任务、operation 和代理请求之间的关联合同。
4. 请求日志、采集 run/snapshot、监控执行历史和告警/变更历史继续各自拥有业务事实；运行日志仅记录技术执行与故障阶段，通过稳定引用关联，不复制其业务载荷。
5. 日志 I/O 故障、队列饱和或目录不可用不得阻断代理、采集、监控、迁移或退出；日志丢失本身必须被有界地计数并在后续可写事件中暴露。
6. 旧日志相关代码在切换完成后删除或收敛，不保留永久 dual path、兼容 facade 或“临时”直接输出。

## 2. 背景与问题陈述

### 2.1 当前可复用基础

- `observability::correlation` 已使用 task-local correlation id，并为 IPC command 与 proxy request 建立了有界、匿名化关联标识。
- `observability::runtime` 已提供稳定 code、结果、匿名资源标识、catalog、sink、reader 和 recovery 的唯一方向。
- `observability::metrics` 保留有界业务指标；运行时摘要由 `RuntimeLogService`/`application::runtime_diagnostics` 提供。
- 请求、采集和监控已具备结构化持久化事实及 UI 读取路径。
- 数据存储诊断导出已经遵循匿名 candidate、状态与计数的最小化报告原则。

这些基础应被重构为一个可工作的内核，而不是再创建一套平行的 logger、metrics、redactor 和诊断 DTO。

### 2.2 当前问题

1. `tracing` 仅是调用点与 span，不是一个已初始化、可落盘的运行日志系统。
2. `println!`、`eprintln!`、`tracing::warn!` 和 `tracing::error!` 分散在启动、路由、后台任务、采集、监控和退出路径，字段格式、等级、脱敏、保留和可查询性不一致。
3. 一些调用直接格式化 `Error` 或使用 `error = ?error`。错误链可能携带 URL、远端响应文本、路径或认证相关上下文，不能被默认持久化。
4. 现有文本脱敏是启发式 marker 检查；它可作为受控文本预览的最后一道保护，不能充当任意动态日志字段的授权机制。
5. 历史 `StructuredEvent`、parallel diagnostics scaffold 和 duplicate redaction helper 已删除；剩余实现必须继续遵守唯一 runtime owner，不能重新引入平行模型。
6. `OperationRegistry` 的进度和终态是有界内存状态，不是可跨重启排错的记录；前端错误边界目前只负责恢复 UI。
7. 现有业务日志页面只显示请求事实，无法回答“采集没有被调度”“monitor runner 为什么退出”“启动恢复为何失败”“某条 IPC 为什么超时”这类运行问题。

### 2.3 不能采用的方案

- 不能只增加 `tracing-subscriber` 并将全局 `tracing` 输出到文件。
- 不能把所有运行事件插入 SQLite 的 `request_logs` 或新增无限增长的 `runtime_logs` 表。
- 不能让 React 直接读取日志文件，或让前端发送任意文本给后端记录。
- 不能通过记录完整 HTTP header、URL、prompt、response、cookie、token 或异常 `Debug` 输出换取可诊断性。
- 不能以“debug 模式”为理由关闭生产脱敏、保留上游原文或绕过导出审查。
- 不能长期保留旧 stdout/stderr 与新日志双写，或让每个模块各自发明文件格式、目录与 retention。

## 3. 目标与非目标

### 3.1 目标

- 在应用重启后仍能安全地定位启动、IPC、后台任务、网络、持久化、代理、采集、监控、迁移、更新与前端错误的失败阶段。
- 对一次用户操作、后台 task 或代理请求提供稳定关联链，不暴露原始身份和秘密。
- 每条运行记录具备稳定事件 code、组件、等级、结果、耗时与受控诊断字段；新增模块无需修改全局 switch 或日志表 schema。
- 日志文件、指标缓冲、诊断读取和支持包输出均有明确资源上限、失败语义和 owner。
- 所有日志相关的安全规则可由 Rust 单元/集成测试和静态架构检查验证。
- 开发者模式提供本地、分页、受控的运行诊断读取；普通用户不需要理解日志文件才能完成日常操作。

### 3.2 非目标

- 云日志、远程 telemetry、账号登录、团队共享、自动上传、SaaS 监控、第三方 APM 或公开状态页。
- 记录或回放用户 prompt、模型回复、上游完整 body、认证 header、cookie、token、密码、绝对本地路径或完整 URL。
- 用运行日志替代请求审计、监控历史、采集快照、告警历史或业务数据库备份。
- 为每个成功请求写一条高频 `INFO` 文件记录；正常代理成功仍以请求事实和聚合指标为主。
- 支持任意 grep/正则全文搜索、用户自定义日志模板或任意 JSON 属性。
- 在首期引入动态日志级别 DSL、插件机制或多进程日志聚合器。

## 4. 术语与所有权

| 术语 | 定义 | 唯一 owner |
| --- | --- | --- |
| Runtime Event | 一次技术执行的安全、结构化、可落盘事实 | `observability::runtime` |
| Business Fact | 请求、采集、监控、变更等领域事实 | 各领域 store/service |
| Metric | 有界、聚合或短期时序运行信号 | `observability::metrics` |
| Correlation | 同一命令、任务或代理请求的匿名关联标识 | `observability::correlation` |
| Interaction | 一次用户手势触发的、可跨多个 IPC command 传播的短生命周期匿名追踪标识 | 受控前端 IPC client + `observability::correlation` |
| Subject Ref | 站点、Key、execution、operation 等对象的匿名稳定引用 | `observability::runtime::subject` |
| Support Bundle | 用户显式导出的严格脱敏诊断档案 | `services::support_bundle` |
| Log Sink | 本地 JSONL writer、rotation 与 retention | `observability::runtime::sink` |

以下边界不可打破：

- `RuntimeEvent` 不拥有业务事实状态，不写回健康、采集、路由或告警。
- 业务 store 不直接写文件日志；它只返回类型化结果，由 application/service 边界产生运行事件。
- metric 不是审计记录。可被覆盖或过期的内存指标不能作为业务真相。
- support bundle 是导出物，不是常驻数据库或 remote telemetry。

## 5. 目标架构

### 5.1 模块布局

实现应在现有 `src-tauri/src/observability/` 内重构，不另建横跨全仓库的通用工具箱：

```text
observability/
  correlation.rs            # 保留并扩展现有关联合同
  runtime/
    mod.rs                  # 唯一公开 emit/read/bootstrap 面
    event.rs                # RuntimeEvent、字段与 code 校验
    catalog.rs              # 汇集组件就地声明、生成/校验全局 machine-readable manifest
    subject.rs              # 匿名资源引用
    error.rs                # Error -> 安全 error code/phase 映射
    sink.rs                 # JSONL writer、rotation、retention、degraded state
    reader.rs               # 有界分页读取，不暴露路径
    crash.rs                # 最小 crash marker 与 panic hook
    clock.rs                # monotonic duration、wall-clock guard 与逻辑 segment 时间
  metrics.rs                # 吸收现有有界 metric 模型
  diagnostics.rs            # 吸收现有 runtime snapshot，供 developer/support bundle 使用
  redaction.rs              # 单一的安全文本/URL末端清洗，非主数据通道
```

`events.rs`、`metrics.rs`、`diagnostics.rs` 和 `redaction.rs` 必须在切换中被接线、移动或删除；不得保留带 `allow(dead_code)` 的第二套运行诊断模型。保留 `correlation.rs` 的公开职责，不复制 correlation 实现。

事件 descriptor 仍必须与其 producer owner 同目录维护，不能把业务 owner 迁入全局 mega enum；但每次 Rust 构建都必须由 catalog collector 从这些声明生成并校验唯一的 `runtime-event-catalog.v1.json` machine-readable manifest。manifest 是构建产物和 CI 审计输入，不由业务代码手写第二份清单；构建/测试必须在产物与源声明不一致时失败。

### 5.2 RuntimeEvent 信封

所有持久化运行事件必须序列化为一行 JSON。最小信封如下：

```ts
type RuntimeEvent = {
  schemaVersion: 1;
  atMs: number;
  sequence: number;
  level: "error" | "warn" | "info" | "debug";
  eventCode: string;
  component: string;
  outcome: "ok" | "error" | "timeout" | "cancelled" | "overloaded" | "degraded";
  sessionId: string;
  correlationId: string | null;
  interactionId: string | null;
  operationId: string | null;
  subject: SubjectRef | null;
  durationMs: number | null;
  error: RuntimeError | null;
  detail: RuntimeDetail | null;
};
```

约束：

- `eventCode` 与 `component` 均为静态常量；只允许小写 ASCII、数字、`.`、`_`、`-`，最长 64 bytes。
- `sequence` 只在单个 process session 内递增，用于同毫秒内稳定排序；不能被用作业务 id。
- `sessionId` 为应用启动时生成的随机匿名 id；不得使用 Windows 用户名、数据目录、机器名或 Tauri window label。
- `correlationId` 复用现有 32 位匿名 id。代理 request 使用当前哈希派生方式；IPC、task 和 operation 使用同一父 id 传播。
- `interactionId` 是受控 IPC context 生成的随机、匿名、短生命周期 id；它表示一个用户手势及其多个 command，不是业务 id、`SubjectRef` 或 route/form 值。没有用户手势、验证失败或过期时必须为 `null`。
- `SubjectRef` 只允许 `kind + opaqueId`，其中 `opaqueId` 为已批准 scope 下的 hash 或既有无敏感 UUID；不得包含 Station 名、Key 名、URL、账号、文件路径或远端 id 原文。
- `durationMs` 只能由 monotonic clock（Rust `Instant` 或等价实现）计算，为非负有界整数；没有可靠开始点时为 `null`，不猜测。`atMs` 仅是 UTC 展示/取证时间，允许因系统时钟变化前后跳动，不能用于耗时、超时、重试退避或同 session 的因果排序。
- `RuntimeDetail` 是按 event code 定义的闭合 Rust enum，不接受 `serde_json::Value`、`HashMap<String, String>` 或任意错误文本。
- 运行日志不持久化自由文本 `message`。UI 与诊断包根据 `eventCode` 显示固定文案；任何外部响应、错误或自由文本一律映射为稳定 error/event code，若“曾收到但已省略”的事实有诊断价值，只能记录闭合 enum sentinel `redacted`，不得保留任何原文片段。

示例：

```json
{"schemaVersion":1,"atMs":1786674030123,"sequence":418,"level":"warn","eventCode":"persistence.request_terminal.retrying","component":"proxy.lifecycle","outcome":"degraded","sessionId":"6d80b0e73f3c4dc8a602ff8eb5a69c2d","correlationId":"7aa514eabe018f288b086516d426a022","interactionId":null,"operationId":null,"subject":{"kind":"proxy_request","opaqueId":"res_7aa514eabe018f288b086516d426a022"},"durationMs":18,"error":{"domain":"persistence","code":"sqlite_busy","retryable":true},"detail":{"kind":"retry","attempt":2,"maxAttempts":3}}
```

### 5.3 事件目录、错误映射与等级

事件 code 按 owner 组件常量化定义，例如：

```text
app.bootstrap.started
app.bootstrap.degraded
ipc.command.completed
ipc.command.failed
persistence.open.failed
persistence.request_terminal.retrying
background.task.failed
proxy.startup.failed
proxy.transport.failed
collector.run.failed
monitoring.runner.failed
migration.operation.failed
frontend.boundary.failed
runtime.log_sink.degraded
runtime.log_event.dropped
runtime.log_lease.unavailable
runtime.log_partial_recovered
runtime.clock.wall_adjusted
runtime.crash_marker.unavailable
ipc.runtime_context.invalid
```

目录规则：

1. code 的声明与触发模块同属一个 owner 子模块，不能集中到不断膨胀的全局 mega enum；catalog collector 是汇集和验证者，不是第二个业务 owner。
2. 每个 code 的 descriptor 必须声明 `eventSchemaVersion`、`detailSchemaVersion`、稳定 owner、level、允许 outcome、允许 detail variant、允许 subject kind、是否采样、是否可出现在 support bundle 以及 UI 固定 `messageKey`。生成 manifest 时产生不可变 `manifestId`；每个 segment metadata 必须记录它，随应用发布的当前和上一兼容 manifest snapshot 必须在运行日志根目录中原子保存至没有 segment 引用后才可清理。
3. descriptor 必须显式声明 `active` 或 `deprecated`；deprecated code 必须声明唯一的 `replacedBy` code、迁移理由和停止生产版本。被替代的 code 仍由 reader 按兼容合同显示，不能被悄然复用。
4. 每次构建生成的 manifest 必须全局验证 code 唯一性、owner 存在、schema/detail 版本、replacement 链无环、message key 存在、subject allowance 和 support-bundle permission；缺失或冲突即构建失败。
5. `RuntimeError` 由错误边界将内部错误映射为 `domain + stable code + retryable`；不序列化 `Display`、`Debug`、source chain 或 backtrace。
6. 领域已有稳定失败分类时必须复用，如 proxy failure kind、monitoring failure kind、collector driver failure kind；不得为同一失败新增第二个字符串宇宙。
7. 没有可安全映射的错误时记录 `internal_unclassified`，同时增加有界 metric；不能为了“更详细”回退到原始文本。
8. 目录和 reader 的兼容性是 versioned contract：当前 reader 至少读取当前与上一受支持 envelope/detail 版本及其 manifest snapshot；升级测试必须使用历史 JSONL fixture，验证已弃用 code 仍可读取、未知版本被安全隔离且不导致整段读取失败。

| 等级 | 语义 | 生产落盘规则 |
| --- | --- | --- |
| `error` | 操作失败且未在当前边界恢复，或应用进入降级/恢复模式 | 始终尝试写入 |
| `warn` | 已恢复的失败、重试、超时、预算/队列压力、数据降级 | 始终尝试写入 |
| `info` | 启动、停止、状态迁移、显式用户操作终态、关键后台任务终态 | 默认写入 |
| `debug` | 受控的开发诊断，不含任何额外敏感字段 | 仅 developer mode 的短期本地写入 |

- 单次成功代理请求、每次轮询、每个正常 timer tick 不得产生 `info` 文件事件。
- 高频成功行为只进入业务事实或聚合 metric；超时、失败、重试、异常慢请求和状态转换才产生运行事件。
- developer mode 只增加允许的 event 频率，不增加字段权限，也不改变 redaction 或 support bundle 策略。

### 5.4 `tracing` 集成限制

- `tracing` 保留给 correlation span 和 `RuntimeLogService` 的内部实现；生产业务模块不得直接发出可由全局 subscriber 落盘的 event。
- 如采用 `tracing-subscriber`，其日志 layer 只接收由已验证 `RuntimeEvent` 适配器发出的私有 target；不得把任意 crate、第三方依赖或历史 `tracing::*` 调用自动收集到 JSONL。
- correlation span 的字段也必须遵循稳定 code 与匿名 id 合同。span 是上下文，不是可持久化诊断载荷。
- 架构检查必须拒绝未经批准的 `tracing::event!`、`warn!`、`error!`、`info!`、`debug!` 调用；唯一允许的业务入口是 `RuntimeLogService::emit` 或其组件级类型化 adapter。

## 6. 安全与隐私合同

### 6.1 默认拒绝

下列数据不得进入 `RuntimeEvent`、文件日志、metric label、IPC DTO、support bundle、fixture 或测试输出：

- API key、Authorization、cookie、token、密码、会话、加密密文、nonce、AAD；
- prompt、模型回复、SSE event 原文、完整上游错误 body、原始 HTTP header；
- 完整 URL、query、fragment、userinfo、完整本地路径、Windows 用户名；
- Station/Key 的用户可编辑名称、远端账号 id、原始 request id（除非已定义为无敏感公开 id 且经专门评审）；
- 任意 `Error` 的 `Display` / `Debug` / source chain / backtrace。

### 6.2 允许字段、外部文本与末端检查

- 主路径使用类型与白名单字段，安全性由构造 API 保证，而非写完后字符串搜索。`RuntimeDetail` 的构造函数只接收闭合 enum、受限数字/布尔值、已批准的匿名引用和静态 code；不得接受 `String`、`&str`、通用 JSON 或实现了 `Display` 的值作为日志字段。
- `SafePreview` 不是运行日志数据类型，也不得以任何名称提供“审核后动态文本预览”的例外。所有来自 HTTP、IPC input/output、上游错误、文件系统、第三方 crate 或自由错误消息的文本都只能被丢弃，或映射为稳定 code 加闭合 enum sentinel `redacted`；marker 命中不能成为保留部分原文的授权条件。
- 现有 `redact_text_preview`、`redact_url_preview`、`services::secrets::mask`、capture redaction 与 observability redaction 必须收敛为一个底层 canonical mask/redaction 内核。该内核仅用于阻止错误接入时的末端泄露防御、support bundle canary 扫描和既有非日志职责，不得把动态文本转换为可持久化 runtime event 字段。
- writer、reader、bundle 和测试输出仍执行 marker/URL/path 扫描作为 defense-in-depth；扫描命中必须拒绝事件或导出并产生固定 code/计数，绝不回写、截断或部分展示原始文本。不得让不同模块维护不同敏感 marker 表。

### 6.3 导出与文件系统

- 日志根目录位于 Tauri 应用本地数据目录下的独立 `runtime-logs/`，不使用当前数据 SQLite 所在目录，不使用用户选择的导出路径，也不写入仓库。
- `.gitignore` 必须覆盖运行日志目录、JSONL segment、crash marker、support bundle 和部分文件。
- diagnostics/support bundle 只由显式用户动作生成。生成前由后端检查每个条目的最大大小、文件名和 schema；前端没有文件系统直读权限。
- 默认 support bundle 不含数据库、备份、原始配置、完整 URL、对象显示名称或密钥。它遵守并收紧 `SECURITY_EXPORT_IMPORT.md` 的 default export 边界。

## 7. Sink、保留与故障语义

### 7.1 初始化顺序

1. `run()` 最早阶段安装 bootstrap stderr sink，仅允许固定 `app.bootstrap.*` code。
2. Tauri 应用数据目录解析成功后，初始化 `RuntimeLogService`、session id、JSONL sink 和 panic hook。
3. 若应用数据目录不可用、日志目录创建失败或 writer 初始化失败，应用继续运行，保持 bootstrap stderr，并记录内存中的 `log_sink_degraded` 状态。
4. 数据库打开、迁移、代理启动和后台 task 注册之后的运行事件全部通过 `RuntimeLogService`。
5. 日志服务不得依赖 Persistence Runtime、SecretManager、网络、React window 或用户配置，避免启动/恢复环依赖。

### 7.2 文件格式与 rotation

- UTF-8 JSONL，每行一个 `RuntimeEvent`，尾随换行；单行最大 16 KiB，构造阶段超过上限必须失败关闭为 `runtime.log_event_rejected` 计数，不截断成不合法 JSON。
- 每个安装目录在任一时刻只能有一个持有 OS 强制排他锁的 log writer/retention lease。lease 以应用本地 `runtime-logs/` 下的独立锁文件和持续打开的 handle 实现；仅 `create_new`、PID 检查或 `single-instance` plugin 都不足以替代该锁。无法取得 lease 的重启、并发实例或 updater 进程继续执行业务和 bootstrap stderr，但不得写 JSONL、执行 recovery 或 retention；它们以退避重试 lease 并用固定 `runtime.log_lease.unavailable` 状态/metric 表示降级。
- 每个持有 lease 的 process 生成独立的随机 writer/session identity。segment 名称只含日期标签、该 identity 与递增 segment 序号，不含对象、站点、用户名、URL 或错误文本；所有新文件以 `create_new` 创建，禁止复用同名文件。
- 活跃 segment 始终以 `*.jsonl.partial` 存在。rotation 或正常退出时，writer 必须先 flush 并尽力 `sync_data`、关闭 handle，写入且验证对应的闭合 metadata（格式版本、`manifestId`、writer identity、segment 序号、逻辑 generation、字节数、首末 `atMs`、closed `atMs`）；metadata 也必须先以 partial 写入并原子发布，最后才以原子 rename 将数据文件发布为 `*.jsonl`。reader、support bundle 和 retention 只接受已发布且 metadata、manifest snapshot、大小均校验通过的 pair；永不读取、导出或删除 active/partial、metadata partial、未知命名或不完整 pair。
- 默认单 segment 最大 8 MiB，按逻辑 UTC 日或按大小轮转；保留最近 14 天且总量最多 96 MiB。文件名日期只是展示标签，retention 不得依靠文件名或文件 mtime 决定年龄或删除顺序。
- segment 的逻辑 generation 在持有 lease 时原子维护，metadata 的实际 `byteLength` 必须与文件大小匹配。retention 只按校验后的 metadata 和实际字节数统计，按 generation 从旧到新、每次有界批次执行；它始终执行 96 MiB 总量上限，且仅在 clock guard 健康时执行 14 天年龄上限。当前 writer、任何 `.partial`、unknown 文件和带有效 live lease owner 的文件一律跳过并在诊断摘要中计数。
- 获取 lease 后、写入前执行有界 startup recovery：最多检查配置的固定数量和总字节预算内的、符合本格式且不属于当前 writer 的遗留 partial。只从完整换行且能通过 schema/canary 校验的 event 行复制到新的本 process partial，按正常 metadata/atomic publish 流程生成 recovered segment；不得把旧 partial 直接 rename 成正式 segment。仅在 recovered segment 已成功发布后，recovery 才可删除其已验证源 partial；失败、超预算、未知或不完整输入保持原状并报告固定计数，交由后续受控恢复处理。
- retention worker 在获得 lease 后、startup recovery 完成后、每日一次和应用优雅退出前尝试运行；删除失败、目录忙、磁盘满或 rotation 失败不影响业务操作，产生一次 rate-limited `runtime.log_sink.degraded` 事件或 bootstrap stderr 固定码，并增加 metric。
- 默认不让用户配置原始路径、任意保留天数或无限大小。后续如开放设置，只允许在严格上限内选择保留级别，并写入安全配置审计。

### 7.3 时钟合同与跳变处理

- `Instant`（或等价 monotonic clock）是 duration、deadline、queue retry/backoff、lease retry 和 clock guard 的唯一计时来源；墙上 UTC 绝不参与这些判定。`atMs`、metadata 中的首末/关闭时间仅用于 UI 展示、取证和人工定位，`sequence` 才是单 session 同毫秒或时钟回拨时的稳定顺序。
- sink 比较相邻的 UTC 采样与相同区间的 monotonic elapsed。偏差超过固定容差、UTC 回拨或异常前跳时，最多每 session 记录一次固定 `runtime.clock.wall_adjusted`，detail 只允许 `backward`、`forward` 或 `unstable` enum 和有界 bucket，不能记录系统时区、主机信息或自由文本。
- 时钟不稳定时不向过去的日期标签轮转，大小轮转继续生效；clock guard 暂停基于“14 天”的 age deletion，直到经过固定的 monotonic 观察窗口确认 UTC 与 elapsed 再次一致。应用刚取得 lease 时也必须先完成该观察窗口，避免关机期间时钟被修改后立即批量按年龄删除；96 MiB byte cap 始终按 metadata generation 执行。

### 7.4 背压与丢失可见性

- 日志 writer 采用专属 worker 和有界队列，业务线程/async task 发射事件时不得等待磁盘 I/O。
- `error/warn` 使用独立保留容量，`info/debug` 使用普通容量；任一队列饱和时不阻塞业务，而是原子增加按 level 聚合的 dropped counter。
- 下一次成功写入或 diagnostics snapshot 必须包含 dropped 计数和上次 sink 错误 code。不能静默声称日志完整。
- writer 不得重试无限次；每次文件写入使用有界重试和退避。持续失败后进入 degraded mode，按冷却周期尝试恢复。
- 任何运行日志实现不得在 proxy response send path、SQLite transaction 持锁期间或 cancellation critical section 内进行同步文件操作。

### 7.5 Panic 和非优雅退出

- 在安装 panic hook 前，crash service 必须独立预创建并保持本 session 的最小 active marker handle；它不经过异步 event queue、JSONL writer、`RuntimeLogService` mutex 或任何可能被 panic 线程持有的锁。marker 文件名承载匿名 session identity，内容只允许固定 schema/code/status，不含 panic payload、backtrace、线程名、环境变量、时间区、路径或 correlation 值。
- panic hook 不得链式调用会输出动态 panic 文本的默认 hook。它使用原子 recursion guard，只有首次进入时才以 `try_lock` 取得 crash-marker 状态；取得后至多对预打开 handle 执行一次固定长度、单次 best-effort write，不 flush/sync/retry/等待队列或锁。任何 `try_lock` 或 write 失败只输出固定 stderr 文本；递归 panic 同样只输出该固定文本。
- 正常退出只在必要的 shutdown drain 完成后删除 active marker。下一次持有 lease 的启动读取上一 session marker：有固定 panic 状态则产生 `app.previous_session_unclean_exit` 的 `panic` detail，否则产生 `unknown_unclean`；读取后按 crash-marker 的独立、受限 retention 清理。marker 初始化或读取失败记录 `runtime.crash_marker.unavailable`，不得阻断应用、自动上传或展示任何原始 panic 内容。

## 8. 跨模块采集矩阵

| 范围 | 必须记录的运行事件 | 继续使用的业务事实 | 禁止记录 |
| --- | --- | --- | --- |
| App bootstrap / shutdown | 启动阶段、恢复模式、退出 drain、未清洁退出、sink 降级 | 数据库恢复状态 | 路径、配置全文、panic 文本 |
| Tauri IPC | command started/completed/failed、耗时、取消、payload 超限 | 各 command DTO 结果 | input/output 原文、secret form 值 |
| Persistence | open/migration/write retry/terminal failure、busy、recovery | schema、请求/采集/监控表 | SQL、bind value、数据库路径 |
| Local proxy / routing | listener start/stop、admission、request terminal persistence failure、transport failure、fallback summary | request log、attempt、routing outcome | prompt/response、header、原始 URL |
| Outbound transport | 建连/超时/协议错误/预算耗尽/重试 | monitoring attempt、collector snapshot | request/response body、认证 |
| Collector | scheduler start/stop、run failed/partial、任务拒绝、writer failure | collector run、snapshot、task state | 原始 snapshot、登录凭据 |
| Monitoring | runner start/stop、execution dispatch/persist failure、worker crash、maintenance failure | execution、attempt、target result、health | probe body、认证、任意上游错误文本 |
| Operations | admission、取消、deadline、terminal、progress overflow | 可持久化的 migration/operation 领域记录 | 自由 progress 文本 |
| Import/export/updater | 阶段终态、校验失败、回滚、恢复需求 | migration journal、用户可见结果 | 包内容、密码、目录、backup 数据 |
| React frontend | bootstrap failure、ErrorBoundary、IPC failure 归类、恢复动作 | 页面自身领域状态 | Error stack、component props、DOM、表单值 |

每个范围的 producer 必须在 application/service 边界记录终态，不在 repository、pure domain function 或 UI render 中随意打点。一次后台 operation 通过一个父 correlation id 串联；手动触发和定时触发必须经过相同的 producer adapter。

## 9. 关联合同

### 9.1 后端

- 应用启动生成唯一 `sessionId`。
- IPC command 继续调用 `correlation::in_command_scope`；调用链内的 application、outbound 和 persistence 读取同一个 correlation id。通过验证的 `interactionId` 在同一 command scope 内以独立 task-local 值传播，不能改写 correlation、operation 或 subject 语义。
- proxy request 继续使用从 request id 派生的匿名 correlation id，不把 request id 直接放入运行日志字段。
- 新 background task 由 supervisor 创建 correlation id；其 child operation、collector run、monitoring execution、migration operation 继承该 id。
- 跨重启的业务记录通过自身 id/匿名 subject ref 关联，不试图将已失效的 process-local correlation id 当成持久业务主键。

### 9.2 前端

- 受控前端 IPC client 在每次用户手势开始时生成随机、无语义的 `interactionId`；同一手势随后发起的多个 IPC command 必须复用它。id 只在内存 action context 中存在，固定 TTL（建议 10 分钟）到期后不可再附加；页面重载、窗口关闭、route、DOM、表单值、对象名称和 URL 参数都不得参与 id 构造。
- 前端 bootstrap 时，唯一 client adapter 必须向后端注册并取得一个只驻留内存、不会进入 runtime event 的随机 `IpcContextSessionId` capability。IPC binding 和命令 registry 把 `IpcRuntimeContextV1 { contextSessionId: IpcContextSessionId, interactionId?: InteractionId }` 作为显式、可生成、版本化 DTO 合同，由该 adapter 注入和后端 command boundary 验证；不得让每个调用手写任意 metadata。验证至少包括 capability 的当前 session 归属、ASCII 固定格式/长度、随机 interaction id 编码、首次见到时间、TTL 和有界 active-id 数量；后端将通过验证的 interaction id 写入 task-local context，使 command 内的 application、outbound、persistence 和 child task event 继承相同 `interactionId`。
- 无 context 的系统/定时调用保持 `interactionId: null`。未知/跨 session capability、格式错误、超过容量或过期 interaction，以及 TTL 后的重放，必须被丢弃而非中断业务 command，记录 rate-limited 固定 `ipc.runtime_context.invalid`（绝不记录被拒绝 id），并以 `null` 继续执行；同一有效 TTL 内的多 command 复用是预期行为。它不能被 hash 为 `SubjectRef` 或替代 command correlation。
- ErrorBoundary 将错误映射为固定 `frontend.boundary.failed`，只含页面类别、构建版本、已验证的 interaction 引用和恢复动作；原始 error/stack 只用于当前进程的 UI 恢复，不持久化。

## 10. 指标、读取 UI 与诊断包

### 10.1 有界指标

现有 `LocalMetricBuffer` 和 runtime diagnostics 作为唯一短期指标基础，扩展但不复制。首期至少提供：

- event emitted/dropped/rejected count；
- sink healthy/degraded、最后 sink error code、当前 segment size；
- writer/retention lease、partial recovery、clock guard 和 crash-marker 的固定健康状态/计数；
- IPC command latency/error；
- task queue/running/orphaned；
- operation terminal/cancel/deadline；
- outbound timeout/retry；
- collector/monitoring failure 分类计数。

指标 label 与 runtime event code 一样必须是静态、有界、无秘密。快照容量、TTL、label 数量、label 长度均继续由底层模型限制。指标只用于当前诊断和支持包摘要，不作为跨重启业务报表。

### 10.2 开发者运行诊断页

新增独立的开发者模式页面或设置子页，不能复用“使用记录”页面：

- 默认按时间倒序显示最多 200 条、总读取最大 1 MiB 的安全事件；
- 后端提供 cursor 分页、level/component/eventCode/correlationId/interactionId 精确筛选；不提供任意全文、正则或路径筛选；
- 行项目只展示时间、level、组件、固定事件文案、结果、耗时、匿名引用和稳定 error code；
- 展开详情只显示 `RuntimeDetail` 的已批准字段；
- loading、empty、error、sink degraded、retention 清理失败和窄窗口状态必须明确；
- 普通模式不暴露运行日志列表、日志路径或导出能力。

日志读取只能通过 Rust `RuntimeLogReader`。reader 只枚举已发布且 metadata、大小、manifest schema 均通过验证的 segment，在分页预算内流式解析；遇到损坏行、未知 schema 或已弃用 code 时按兼容 manifest 返回固定状态/计数，不把原始行传给 UI。`.partial`、unknown 或 lease-owned 文件绝不作为读取候选。

### 10.3 Support Bundle

新增后端 `SupportBundleService`，由显式用户动作调用。首期包内容：

```text
manifest.json                 应用版本、schema、生成时间、匿名 session
runtime-summary.json          sink/metric/queue/retention 摘要
runtime-events.jsonl          最近且有总量上限的安全事件片段
data-store-diagnostic.json    复用既有匿名数据存储诊断
business-summary.json         可选的计数与稳定失败分类摘要
```

合同：

- 导出前执行独立的 secret canary 扫描和条目大小检查；任一检查失败则终止导出并保留固定错误码。
- 不加入 SQLite、WAL、备份、迁移包、原始配置、业务 snapshot、请求日志原始字段或 crash payload。
- 生成到用户明确选择的位置；先写临时文件，再原子 rename；取消、失败或校验失败必须清理临时文件。
- 首期不加密、不自动上传、不调用网络。用户分享前由 UI 显示“严格脱敏诊断包”与文件大小，不能承诺物理删除历史缓存或旧备份。

## 11. 持久化与迁移策略

本升级默认不新增业务 SQLite 表，也不迁移现有请求、采集或监控历史。原因是运行日志必须覆盖数据库故障，并且高频日志写入不应争用业务写锁。

- 日志文件 schema 使用 `schemaVersion` 独立演进；reader 支持当前和上一兼容 envelope/detail 版本以及对应 catalog manifest snapshot，未知版本按不支持 segment 隔离并计数，不能阻断其余 segment。事件 code 废弃也必须保留 descriptor/replacement 映射直到相应 segment 已按 retention 到期。
- 业务事实如需在 runtime UI 中关联，提供只读、按 id 的受控 summary，而不是复制整个业务记录进 JSONL。
- 现有数据存储诊断导出保持单独能力；SupportBundleService 只组合其经过审查的输出。
- 如果后续确实需要持久化某种运行汇总，必须先证明 JSONL + metric + 业务事实不能表达需求，并以新的专项 schema 设计和 retention 合同评审，不能偷加万能 `runtime_logs` 表。

## 12. 旧日志代码收敛与删除台账

本次不是在当前散乱输出上套一层 facade。实现开始前冻结精确 inventory，结束前由自动化重新扫描。基线中至少处理以下类别：

| 现有类别 | 当前位置示例 | 处理方式 | 完成标准 |
| --- | --- | --- | --- |
| 启动/退出 stdout/stderr | `lib.rs`、`background_tasks/exit.rs` | 改为 bootstrap/runtime event；仅保留 logger 自身的固定 stderr fallback | 非测试生产代码无直接宏 |
| runner 原始错误输出 | `services/monitoring/runner.rs`、`monitoring/maintenance.rs` | 在 runner owner 映射稳定错误/阶段并发出事件 | 不格式化动态 `error` |
| collector warning | `services/station_collectors.rs` | 复用 collector failure 分类与匿名 subject | 无 `tracing` 动态文本字段 |
| proxy lifecycle retry | `services/proxy/lifecycle/writer.rs` | 复用 persistence/terminal 错误分类与重试 detail | 无 `error = ?error`、无 raw request id |
| routing projection warning | `background_tasks/routing_projection_runner.rs` | task failure event + metric | 失败能关联 task/session |
| startup auto-start 失败 | `services/proxy/startup_auto_start.rs` | proxy startup event | UI/diagnostics 可见固定 code |
| routing snapshot stderr | `application/routing.rs` | application boundary event | 不在 query/service 内输出 |
| 重复 redaction | observability/capture/secrets 多处 wrapper | 收敛为 canonical mask/redaction 内核 | 单一 marker/URL 策略测试 |
| 未接线 observability 草图 | `events.rs`、`metrics.rs`、`diagnostics.rs` | 接入新内核或删除 | 无为“未来”保留的 dead-code allow |

允许的例外只有：logger bootstrap 前或 logger 自身故障时输出一条不含动态数据的固定 stderr code。例外必须集中在 `observability::runtime::bootstrap`，由静态检查白名单精确到文件和行，不得在业务模块添加。

现有 `request_logs`、`collector_runs`、`collector_snapshots`、monitoring execution/attempt/target 记录、alerting history、migration journal 不是遗留日志，不在删除范围。它们需要补关联字段或读侧跳转时，必须保持各自领域 owner 和既有安全边界。

## 13. 分阶段实施与切换

### Phase 0：基线与批准

1. 将本规范评审为 accepted spec，并创建实施计划、事件目录与删除台账。
2. 重新扫描所有生产 `println!`、`eprintln!`、`tracing::*`、文本 redaction、文件写入和导出入口，记录精确文件/符号/owner。
3. 冻结 secret canary 集、允许字段矩阵、默认 retention 值、partial recovery 的文件/字节预算、clock guard 容差与观察窗口、interaction TTL/格式、diagnostic bundle manifest 和性能预算。
4. 审查拟引入 Rust crate 的许可证、维护状态、锁文件影响和 Windows 行为；不得未经审计地复制外部 logging 实现。

### Phase 1：安全运行事件内核

1. 将现有 stable code、匿名 resource id、correlation、interaction context 和 metric 模型收敛到目标布局；实现 `IpcRuntimeContextV1` 的生成 DTO、后端 validator 与 task-local propagation 合同。
2. 实现含 `interactionId` 的 `RuntimeEvent`、组件就地 event descriptor、build-time catalog manifest、error mapper、只含闭合 enum 的 detail 构造 API、schema 验证和 JSONL serialization；删除 `SafePreview` 作为 runtime event 的任何入口。
3. 单元/兼容测试覆盖每个字段的长度、字符集、null 语义、interaction 多 command 关联与到期/重放、secret/path/URL canary、未知 error、manifest collision/owner/replacement/message-key/permission 校验，以及旧 JSONL/已弃用 code 的 reader 兼容。
4. 删除或接线旧 dead-code 草图；不能以 `allow(dead_code)` 延后设计决策。

### Phase 2：Bootstrap、sink 与 crash 基础

1. 引入经过许可证审查的 `tracing-subscriber`/rotation writer 或等价 Rust 标准实现，由 `RuntimeLogService` 独占配置；不允许它绕过 typed event 直接持久化 `tracing` 字段。
2. 实现 installation-wide writer/retention lease、create-new writer identity、partial-to-published 原子 segment、闭合 metadata、bounded recovery、clock guard、bootstrap stderr、app-local JSONL、session、retention、degraded state、队列背压和独立 pre-open panic marker。
3. 注入目录不可用、磁盘满、writer 失败、两个 writer/updater 竞争、异常退出后的 partial recovery、损坏/unknown segment、retention 失败、wall-clock 前后跳和递归/持锁 panic marker；证明业务调用不会被日志阻断、不会读写非发布 segment、不会误删未知或活跃文件。

### Phase 3：后端 producer 切换

按以下顺序切换，每个模块完成后立即删除其旧输出，不保留双写：

1. app bootstrap/shutdown、data store startup/recovery、persistence runtime、task supervisor、operation registry；
2. Tauri command/application 边界、outbound transport、proxy startup/lifecycle/routing；
3. collector runner 与 driver error boundary；
4. monitoring runner、maintenance、orchestrator、transport；
5. migration/import/export、updater 与诊断导出。

每一项必须同时补 correlation/适用时的 interaction 传播、指标、事件 code 测试和删除台账条目。对于已有业务记录，运行事件只追加技术终态，不新增第二个业务写路径。

### Phase 4：前端、读取与支持包

1. 为前端 IPC client 和 ErrorBoundary 接入受控、版本化的 interaction context；验证一个用户手势的多 command 流程保留同一 `interactionId`，普通系统调用和过期 context 保持 `null`。
2. 实现 developer-gated runtime diagnostics command、分页 reader 和紧凑读取 UI。
3. 实现 SupportBundleService、导出 UI、预检查和秘密扫描。
4. 验证普通模式权限、developer mode gating、错误/空状态、窄窗口与 bundle cancel/failure。

### Phase 5：硬删除与门禁

1. 删除所有台账中的旧宏、重复 logger、临时 adapter、未接线 dead-code 及过时测试。
2. 添加 architecture/security checks，阻止新的直接输出、未审核 `tracing` 动态字段、动态文本/`SafePreview`/任意日志 JSON、未忽略的日志产物和非受控 reader。
3. 执行完整验证、并发 writer/crash/clock fault-injection、性能/容量 smoke、secret canary 和手工 Windows 诊断包核对。

## 14. 验证与验收

### 14.1 自动化合同

- Rust 单元测试：event schema、catalog manifest（全局 code collision、owner、schema/detail version、replacement、message key、subject/support-bundle permission）、error mapping、subject hash、dynamic-text 拒绝、redaction defense-in-depth、JSONL serialization、历史 segment/已弃用 code reader compatibility、rotation、metadata 校验、retention、queue overflow、writer recovery、panic marker。
- Rust 并发/故障注入测试：两个 process/session 争夺 installation-wide lease、restart/updater overlap、active/partial/unknown/lease-owned 文件跳过、partial salvage 成功/失败/超预算、clock rollback/forward jump、clock guard 暂停 age deletion、递归 panic 与 marker lock 已占用时的固定 fallback；这些测试必须证明不出现双 writer、半发布 segment 或错误删除。
- Rust 集成测试：启动前 bootstrap、数据库不可用、proxy/collector/monitoring failure、correlation propagation、单一 interaction 跨多个 IPC command 的传播、interaction 过期/跨 session/重放丢弃、日志目录不可用、support bundle 生成/取消/失败。
- 前端 Vitest：developer gating、分页/含 `interactionId` 的精确筛选、loading/empty/error/degraded 状态、单一手势复用 interaction、ErrorBoundary 固定事件、导出确认与失败显示。
- 架构测试：生产模块禁止 `println!` / `eprintln!`；`tracing` 仅允许 correlation span 和 runtime adapter 的批准位置；运行事件不得携带 `serde_json::Value`、任意 `Error` 或动态文本构造入口；catalog manifest 必须在构建时通过；日志读取/导出只允许经后端 service。
- 安全测试：将 fake `sk-secret`、`Authorization: Bearer`、cookie、password、userinfo URL、query token、Windows user path、prompt/response canary 注入每条 producer 与 bundle，断言所有日志、DTO、fixture 和错误输出均不含原值。

### 14.2 性能与容量合同

- 常规成功代理路径不产生同步磁盘 I/O，且不因日志背压失败。
- 单个 event 不超过 16 KiB；日志目录在默认 retention 下不超过 96 MiB；指标与 operation progress 保持既有有界语义。
- runtime reader 单页最多 200 行和 1 MiB 解析量；support bundle 的 runtime event 部分有独立大小上限。
- rotation、retention、bundle 和 reader 不得长时间持有 SQLite write lock，不得阻塞 Tauri UI thread，不得在 shutdown 期间无限等待。
- 竞争 lease、clock guard、partial recovery 和 panic marker 的所有等待都有 monotonic 上限；不能取得 lease 时业务路径仍可完成，且只进入可观测的降级状态。

### 14.3 最终退出门槛

- 每个跨模块范围在第 8 节矩阵中都有至少一个 success、failure、timeout/cancel 或 degraded 验证用例。
- 所有旧输出来源均在删除台账中标记为 removed，静态扫描只保留 logger bootstrap 的固定 fallback。
- 新日志目录、JSONL、crash marker 和 support bundle 都受 artifact policy/.gitignore 保护。
- 默认日志、diagnostic page、support bundle 和失败路径均通过 secret canary；没有完整请求/响应、认证、URL query、真实路径或真实账号信息。
- machine-readable catalog manifest 在全局唯一性、owner、event/detail version、废弃 replacement、UI message key、subject allowance 和 support-bundle permission 上通过构建校验；当前及上一兼容版本的真实历史 segment fixture 均可被 reader 安全读取。
- 任意时刻只有 lease owner 可以写入、recover 或 retention；所有正式 segment 都由 partial + metadata 验证 + 原子 rename 发布，竞争 writer、异常退出、partial recovery、未知文件与 retention 的测试均证明没有双写、半读或误删。
- 所有耗时/超时/退避来自 monotonic clock；UTC 跳变有固定 `runtime.clock.wall_adjusted` 事件并暂停年龄清理，字节上限仍按 validated metadata/generation 生效。
- 一个用户手势的多个 IPC command 共享经验证、短生命周期的 `interactionId`；过期、重放和跨 session context 从不进入 event，也不会被当作 `SubjectRef`。
- 请求、采集、监控、告警和迁移的既有业务事实测试仍通过，且没有被 runtime log 逻辑改写。
- `pnpm verify:fast`、相关前端 Vitest、Rust fmt/check/test、架构/安全专项检查全部通过；大范围实现完成前运行 `pnpm verify:full`。

## 15. 风险与控制

| 风险 | 控制 |
| --- | --- |
| 为了快速落盘泄露动态 error | 类型化事件、无 raw error、canary、静态检查 |
| 日志 I/O 拖慢本地代理 | 有界异步 writer、采样、非阻塞发射、丢失可见 |
| 日志与 SQLite 相互依赖 | 独立 app-local 文件目录，不使用数据库作为 sink |
| 再造 metrics/logger/diagnostics 三套模型 | 在现有 observability 目录收敛并删除旧草图 |
| 并发实例、崩溃重启或 updater 竞争破坏 segment | installation-wide OS lease、writer identity、`create_new`、partial + metadata + atomic publish、只由 lease owner recovery/retention |
| 日志文件无限增长或误删 | validated metadata generation、96 MiB byte cap、clock-guarded age retention；跳过 active/partial/unknown/lease-owned 文件 |
| 系统时钟回拨或跳变错误计算耗时/批量删日志 | monotonic duration/backoff/guard、固定 clock event、时钟稳定前暂停 age deletion |
| 日志损坏导致 UI 失败 | bounded reader、逐行容错、corrupt counter、固定事件 |
| 前端将隐私数据传给 logger 或错误关联多个 command | 受控、版本化 interaction DTO，随机短 TTL id、后端验证/传播、无 stack/props、无手写 metadata |
| 启发式脱敏被误当成动态文本授权 | 禁止 `SafePreview`/自由文本字段；仅稳定 code + `redacted` sentinel，marker 扫描只作末端防御 |
| panic hook 与 writer 锁重入或自身崩溃 | 独立预打开 marker、recursion guard、`try_lock`、单次固定写和固定 stderr fallback |
| 切换期间行为不一致 | 模块逐一替换并立即删除旧输出，不永久双写 |
| 导出包成为敏感外泄载体 | 最小 manifest、后端白名单、secret scan、显式用户动作 |
| 新依赖无人维护或许可不兼容 | 引入前审查 license、维护状态、lockfile 与 Windows 测试 |

## 16. 待评审决策

以下事项必须在实施计划开始前确认，不得由实现隐式决定：

1. 默认 retention 是否采用“14 天 / 96 MiB / 单 segment 8 MiB”；本规范建议该值作为首期固定上限。
2. developer mode 是否允许落盘 `debug` 事件；本规范建议允许，但字段与普通模式完全相同且自动回退到 `info`。
3. 运行诊断入口放在“设置 -> 开发者工具”还是独立高级页；本规范建议设置子页，避免和业务使用记录混淆。
4. support bundle 的默认 runtime event 上限；本规范建议不超过 10 MiB，并始终排除 SQLite/备份。
5. interaction context 的固定格式、TTL、失效/重放 policy 和 generated binding 版本；本规范要求 Phase 1 确认并实现，不能仅预留字段后分批绕过。
6. OS lease 的 Windows 实现方式、partial recovery 的固定文件/字节预算和 clock guard 的容差/观察窗口；本规范要求在 Phase 0 固化，并以多 process Windows 测试验证。
7. 是否使用 `tracing-subscriber` 与 `tracing-appender` 作为实现依赖；本规范推荐它们的成熟 Rust 生态方向，但要求实现前完成许可证与 Windows 行为审查，且它们不得成为未类型化的文件 sink。

## 17. 交付物

本规范获批后，实施计划至少应交付：

- accepted spec 与精确 source inventory/deletion ledger；
- component-local runtime event catalog、build-time machine-readable manifest、字段矩阵、error mapping、correlation 与 interaction contract；
- runtime log sink、installation-wide lease、partial recovery、clock guard、reader、retention、crash marker、metric snapshot；
- app/IPC/task/proxy/outbound/collector/monitoring/migration/frontend producer cutover；
- developer runtime diagnostics UI 与 SupportBundleService；
- artifact policy/.gitignore、命令权限、生成 binding 和文档更新；
- 安全、容量、故障注入、架构删除、前端及跨层验证证据；
- 已删除的 legacy 输出和未接线 observability 草图，不留下长期兼容层。
