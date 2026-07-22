# Relay Pool Desktop 规模化架构升级 Spec

日期：2026-07-22
状态：设计草案，待评审后冻结
适用范围：Tauri IPC、应用 composition、前端数据所有权与页面生命周期、后台任务和前台长操作、异步 outbound、Provider Driver、架构测试与工程门禁
上位约束：`docs/PROJECT_PLAN.md`、`2026-07-07-relay-pool-data-architecture-refactor-design.md`、`2026-07-12-navigation-performance-and-page-activity-design.md`、`2026-07-19-request-lifecycle-architecture-upgrade-design.md`
外部前置条件：Persistence V2 按其独立 spec 完成收尾；本 spec 不重新设计、修改或验收 Persistence V2

## 1. 执行摘要

Relay Pool Desktop 当前采用的 Tauri 2、React、TanStack Query、Axum、Tokio 和 SQLx 仍适合本地桌面网关产品。当前问题不是框架失效，而是早期 MVP 的组织方式继续承载了站点管理、凭据、采集、渠道监控、价格、路由、本地代理、更新和数据恢复等不断增长的职责。

本次升级不替换技术栈，不把应用拆成微服务，也不引入通用插件系统。目标是在现有模块化单体内建立七个稳定边界：

1. Rust 是 IPC 契约和错误语义的唯一权威，TypeScript 绑定由构建期生成。
2. Desktop backend 与 Demo backend 在启动时显式选择，运行时调用失败不得自动切换到 mock。
3. TanStack Query 是前端服务器状态的唯一权威，页面只拥有视图状态和未提交表单状态。
4. Application composition 只在启动边界组装依赖；command 和 feature 只能获得所需的窄领域 facade，不能使用全局 service locator。
5. 所有 daemon 和用户触发的长操作由统一 supervisor primitives 协调取消、等待、退避、状态、容量和关机，但业务依赖仍由各 task body 持有。
6. Provider 通过编译期封闭、按能力拆分的 driver family 扩展；采集、远端 Key 和授权共享 provider registry，但不汇总成万能 trait。
7. Provider/探测网络统一经过共享异步 outbound；真正阻塞的 OS/WebView 调用进入有界 blocking boundary。

目标依赖关系：

```text
React Page
  -> Feature Query / Mutation
  -> Domain Client
  -> App-scoped Backend Composition
  -> Generated Tauri Binding
  -> Tauri Command Adapter
  -> Domain Command Facade
  -> Application Service / Runtime Facade

TaskSupervisor
  -> Station Collector Task
  -> Channel Monitor Task
  -> Startup Task
  -> cancellable foreground Operation
  -> existing ProxyRuntime lifecycle

ProviderRegistry
  -> CollectorDriver | RemoteKeyDriver | AuthorizationDriver
  -> Sub2ApiProvider | NewApiProvider | OpenAiCompatibleProvider
  -> AsyncOutboundClient
  -> canonical CollectorFacts / AdapterEvidence
```

任何层都不得跨过相邻边界直接读取另一个层的内部状态。迁移按 feature 和运行时任务逐个切换，不长期保留双实现、双缓存、双写或隐式 fallback。

### 1.1 Spec 优先级与冲突处理

本 spec 是规模化架构边界的后续上位约束。与旧 spec 冲突时按以下规则解释：

- `2026-07-07-relay-pool-data-architecture-refactor-design.md` 的 canonical facts、query services 和 projection ownership 继续有效；其中 production API fallback wrapper 被本 spec 的显式 `DesktopBackend` / `DemoBackend` 取代。
- `2026-07-12-navigation-performance-and-page-activity-design.md` 的导航反馈、焦点、过渡和性能测量继续有效；其中“所有访问过的 shell 页面常驻”和页面自行组合 activation/query refresh 的策略被本 spec Section 9.4 取代。
- `2026-07-19-request-lifecycle-architecture-upgrade-design.md` 继续拥有 proxy request/attempt/protocol/delivery lifecycle；`TaskSupervisor` 不重新解释这些状态，只协调进程级启动和 shutdown。
- Persistence V2 的 schema、session、store、migration 和 upgrade recovery 仍由其独立 spec 决定。

实施计划必须显式引用本节，不能同时把冲突的新旧策略都列为目标。

## 2. 当前基线

### 2.1 IPC 边界

- 前端 API 层存在大量直接 `invoke("command_name")` 调用，命令名、入参和返回类型由人工同步。
- Rust commands 普遍返回 `Result<T, String>`，机器错误语义在序列化时丢失。
- 前端通过错误文本和正则区分 ACL、command not found 和 runtime unavailable。
- Rust 与 TypeScript 分别维护 `Station`、`StationKey`、`AppSettings`、`RequestLog`、`CollectorSnapshot` 等同名 DTO。
- streaming `Channel`、普通 request/response 和长任务状态尚未形成同一套契约治理规则。

### 2.2 Runtime backend

- 多个 API 模块在 Tauri invoke 不可用时直接返回内存数据或执行内存写入。
- mock 数据、浏览器 preview 和正式桌面数据访问共享同一组业务函数。
- IPC 初始化、ACL 或 command 注册故障可能被解释成“空数据”或“保存成功”。
- preview backend 复制了一部分站点、Key、设置和采集业务规则，容易与真实实现漂移。

### 2.3 前端数据与页面生命周期

- 部分页面已经直接消费 Query Cache，部分页面仍把相同服务器数据复制到本地 state 并手工 refresh。
- 部分 feature 同时依赖 Query invalidation、DOM CustomEvent 和页面 activation callback。
- Shell 为保留页面状态而常驻多个历史页面，并额外维护 interactive、refreshEnabled、subscribed 和 transition state。
- Stations 等页面按站点创建独立 query，IPC 数量随站点数量线性增长。
- 巨型页面同时承担查询编排、mutation、表单、弹窗、流式测试、筛选、拖拽和渲染职责。

### 2.4 后台任务

- Proxy runtime 已具备 cancellation、join、drain 和状态发布能力。
- Station collector runner 与 channel monitor runner 各自创建线程、维护 `AtomicBool` 和 `JoinHandle`。
- 周期任务失败主要输出到 stderr，缺少统一状态、退避次数、最近成功时间和可查询错误。
- 启动、退出、升级前 drain 和运行时重启没有统一任务注册表。

### 2.5 Collector adapters

- provider 分发仍依赖字符串 adapter 名称。
- Sub2API 和 NewAPI adapter 同时承担 endpoint 选择、认证、HTTP 调用、恢复、解析、兼容分支、事实映射和输出组装。
- provider-specific 兼容性容易扩散到通用 orchestration。
- adapter 的正确性大量由源码文本断言和少量 fixture 锁定，缺少统一 conformance suite。

### 2.6 工程门禁

- 大量 JavaScript contract tests 直接读取源码并用正则断言结构。
- 源码正则可以锁住临时迁移边界，但不能可靠理解 Rust/TypeScript AST、条件编译、重导出或同名符号。
- 前端行为测试数量相对较少，页面级数据所有权主要靠实现约定维持。
- 发布门禁缺少统一 lint、`cargo fmt --check` 和严格 clippy gate。

### 2.7 Application composition、长操作与 outbound

- `AppServices` 同时暴露多个 application service，并被大量 Tauri command 作为 `State<AppServices>` 获取，已经形成运行时 service locator。
- command 可以从同一个 state 穿透到 credentials、collectors、settings 和其他服务，使依赖半径无法从函数签名看清。
- 远端 Key、连通性测试、collector、endpoint ping、channel monitor、web authorization 和 updater 仍存在同步 `ureq`、`spawn_blocking` 或自建线程路径。
- Key connectivity 等用户触发操作只用前端 run token 忽略旧回调，不能真正取消后端网络、stream 和阻塞工作。
- command module 内仍包含 OpenAI-compatible request 构造、SSE 解析、模型 fallback 和网络执行，不只是 transport adapter。
- blocking 工作没有统一并发容量、排队超时和应用退出所有权。

### 2.8 CI、依赖与工作区产物

- 当前只有 tag release workflow，综合验证首次发生在发布路径，缺少 pull request/push 的日常快速门禁。
- 依赖锁定存在，但没有稳定的 advisory、license/来源和重复网络栈治理入口。
- 根目录及 `src-tauri` 下存在多个 target/output 变体；ignore 规则没有覆盖所有运行产物，代码索引和 watcher 会读取无关生成文件。
- 架构测试、性能结果和 release artifact 尚未统一记录 source revision、toolchain 和构建模式 provenance。

### 2.9 2026-07-22 审计快照

以下数字用于证明问题已超过偶发个案，不作为永久硬编码门槛；Stage 0 必须用 parser-backed inventory 重建基线：

- commands 中约 103 个函数签名直接获取 `State<AppServices>`。
- frontend API 中约 14 个文件、118 处 `isTauriInvokeUnavailable` 判断承担业务 fallback。
- production 命名路径中约 12 个 Rust source file 直接引用 `ureq`，并存在约 16 处 `spawn_blocking` 字面调用。
- 167 个 `.test.mjs` 中约 158 个读取源码文本；前端正常 component/unit test file 明显少于 source-contract scripts。
- workflow 只有 `release.yml`，没有 pull-request CI workflow。
- CodeGraph 索引曾包含 output/target 生成内容，说明现有 ignore 边界不足以隔离工具噪音。

这些统计只能作为 intake evidence。正式 architecture gate 必须排除 `#[cfg(test)]`、测试 server、generated code 和合法 OS blocking allowlist，不能用相同的文本搜索冒充最终证明。

## 3. 设计目标

### 3.1 可靠性

- Desktop runtime 故障必须显式失败，不得静默降级到模拟数据。
- 每个 IPC 失败具有稳定 code、公开 message、retryability 和可选的脱敏 details。
- 后台任务必须可取消、可等待、有界、可观测，并在关机时得到明确终态。
- 页面显示的数据必须来自单一权威，不因 activation、event 或 cache 竞争而回退到旧副本。
- provider adapter 的失败必须经过统一分类，不能依赖错误字符串决定重试或任务结果。
- 所有跨线程、跨进程边界默认 fail closed；未知枚举、未知错误 code 和未知 command version 不得猜测成功。
- 用户取消长操作后，后端必须停止后续网络、重试和持久化副作用；仅忽略前端回调不算取消成功。
- mutation 不自动重试；只有声明幂等并携带稳定 operation key 的调用才能由 transport 重放。
- blocking work 达到容量或排队超时时返回 `Overloaded`，不得无界占用 Tokio blocking pool。

### 3.2 可维护性

- 命令、DTO 和错误类型只有一个权威定义位置。
- 页面不再编排低层 invoke、跨资源 `Promise.all` 或手写缓存同步。
- Tauri command 只做反序列化、权限边界、service 调用和错误映射，不承载业务编排。
- 后台 runner 不各自发明启动、停止、sleep、join 和日志协议。
- provider-specific 代码只能存在于对应 driver 内或明确的共享协议模块内。
- 架构规则由 parser、类型系统和行为测试保护，不依赖脆弱的源码片段匹配。
- 不以行数作为唯一拆分标准；职责、依赖方向和状态所有权是模块拆分依据。
- command 函数签名必须显式暴露所依赖的领域 facade，禁止通过 `AppServices` 查找任意服务。
- 网络、proxy 解析、timeout、TLS、redaction 和 retry budget 由共享 outbound policy 治理，provider 不各自维护网络栈。

### 3.3 可拓展性

- 新增普通 command 时，Rust 定义应能生成 TypeScript 调用和类型，不需要在多个文件复制字符串和 DTO。
- 新增 provider 时，只新增 provider module、需要的 capability drivers 和 conformance fixtures，不修改通用 collector/remote-key 主循环。
- 新增周期任务时，通过 `TaskSpec` 注册，不复制线程和 shutdown 逻辑。
- 新增用户长操作时，通过 `OperationSpec` 注册取消、容量和进度合同，不在页面发明 run-token-only 协议。
- 新增页面 read model 时，以一个聚合 query 替代每行 IPC，不随数据规模放大前端调度开销。
- 新增错误类型时必须显式定义 code、用户可见性、retryability、日志级别和敏感字段策略。
- 新增 application service 不得扩大所有 command 的依赖面；只允许新增或扩展对应领域 facade。

### 3.4 安全边界是可靠性的一部分

- production Tauri WebView 必须使用显式、经过测试的 CSP；`csp: null` 不能进入 release bundle。dev/preview 如需放宽，使用独立配置且不得污染 production provenance。
- main window、capture window 和 browser preview 使用不同 composition/capability 边界。远程 capture window 只获得完成授权所需的最小 command，不能继承 main window capability。
- capture/navigation 的 remote origin 以当前 station endpoint revision 和精确 origin allowlist 校验；`http://*` / `https://*` 只能作为 Tauri capability 外壳，application 仍必须拒绝非当前 station、lookalike origin、userinfo 和非法 scheme。
- key、cookie、token、proxy credential 和 Authorization header 使用不可 Debug/Display 的短生命周期 secret wrapper；日志、IPC、operation progress、metric label、redirect history 和 error chain 不得持有原值。
- outbound redirect 不得跨 origin/scheme 转发敏感 header，HTTPS 不静默降级 HTTP；凭据刷新和远端创建必须有 single-flight、幂等或 `ResultUnknown + reconciliation` 合同。
- 本升级不引入账号、云遥测或远程控制面；安全范围聚焦本地进程、WebView/IPC、凭据、provider 网络和发布产物。

## 4. 非目标

- 不修改或重新验收 Persistence V2。
- 不在架构迁移 shard 中顺手替换 Tauri、React、TanStack Query、Axum、Tokio 或 SQLx；大版本升级使用独立 ADR、兼容性矩阵、回滚点和资格证据。
- 不因追新强制升级 React、Vite、Tailwind 或 Rust edition，但也不把“非目标”当成冻结过期依赖的理由。Stage 0 必须核对官方支持窗口、安全公告、MSRV/Node 要求和关键生态兼容性；已失支持或存在不可接受高危风险的版本先进入独立 prerequisite shard，并阻塞 release。
- 不引入微服务、sidecar、Kafka、Redis、NATS、actor framework 或工作流 DSL。
- 不引入运行时加载的第三方 provider 插件、动态库 ABI 或插件市场。
- 不重写已经完成分层的 proxy request lifecycle、protocol machine 或 routing policy。
- 不重新设计 UI 视觉，不借架构升级修改业务文案或页面布局。
- 不为每个 struct、函数或数据库 store 创建无意义 interface。
- 不以“一次提交完成所有迁移”为目标。

## 5. 架构原则

### 5.1 一个状态一个 owner

| 状态 | 唯一 owner | 禁止的第二 owner |
|---|---|---|
| 服务器资源数据 | TanStack Query cache | 页面 `useState` 副本、DOM event cache |
| 未提交表单 | feature form controller | Query cache、全局 singleton |
| IPC DTO 与 command signature | Rust contract | 手写 TypeScript 副本 |
| runtime backend mode | app bootstrap | 每个 API 函数的 catch fallback |
| task lifecycle | `TaskSupervisor` | runner 自有线程协议、fire-and-forget spawn |
| foreground operation lifecycle | `OperationRegistry` | 页面 run token、command fire-and-forget spawn |
| command 依赖 | 对应领域 command facade | `State<AppServices>` service lookup |
| provider 兼容行为 | 对应 capability driver | collector/remote-key orchestrator、UI、persistence |
| outbound policy | `AsyncOutboundClient` | provider 自建 ureq/reqwest agent 和 retry budget |

### 5.2 事实、决策和展示分离

- transport 返回 typed result 或 typed error。
- application service 决定业务结果。
- query/read model 组合页面所需事实。
- view model 只负责展示转换，不回写业务事实。
- observer、logger 和 metrics 不重新解释业务成功或失败。

### 5.3 边界失败必须 fail closed

- Desktop command 不存在、ACL 拒绝或 runtime 未初始化：返回 typed infrastructure error。
- generated binding 与 Rust contract 不一致：构建失败。
- task 无法启动或 shutdown 超时：状态进入 `Failed` 或 `ShutdownTimedOut`，不得报告 stopped/success。
- provider 返回未知响应：保留脱敏 evidence，返回 `UnsupportedResponse`，不得合成空成功。
- 聚合 read model 部分失败：使用显式 partial result，不把缺失数据写成零值。
- blocking queue 已满、operation id 不存在或 cancellation 未被确认：返回 typed terminal/error，不得伪造完成。
- 应用依赖注册失败：composition 整体失败，不留下部分可调用的 command state。

### 5.4 有界并发和显式背压

- 所有 queue、channel、JoinSet 和并发 fan-out 必须有上限。
- 达到容量时返回明确 overload 结果或延后任务，不允许无界 spawn。
- 周期任务必须有单实例保证；同一 task key 不得重入，除非策略明确允许。
- shutdown 有总预算和逐任务预算，超时进入可诊断终态。

### 5.5 渐进迁移但不长期双轨

- 允许在 adapter 边界短暂兼容旧调用者。
- 新旧实现不得同时写同一状态，也不得互相 fallback。
- 每个 stage 必须定义唯一 cutover 点和旧路径删除条件。
- feature flag 只能位于 composition root，不能散落在业务分支中。

### 5.6 优先复用成熟基础设施，不自造运行时平台

- `TaskSupervisor` 是业务无关的 lifecycle policy layer，不是自定义 async executor。task spawn/join/cancel/backpressure 优先复用 Tokio/Tokio-util 的 `CancellationToken`、`TaskTracker` 或 `JoinSet`、`Semaphore`、`mpsc`/`watch` 等成熟 primitives。
- `OperationRegistry` 复用相同 lifecycle primitives，但保持独立 registry；不引入 actor framework、workflow engine 或通用 `Any` context。
- async HTTP 复用 reqwest client/pool/TLS/streaming；outbound 只增加本地应用需要的 typed policy、budget、proxy、redaction 和 evidence，不重写 HTTP stack。
- server state 复用 TanStack Query 的 cache/invalidation/cancellation/stale/gc 能力；不另造全局 store、event bus 或缓存框架。
- Rust-to-TypeScript generator 只在 build/CI 工作。优先选择已有维护者、锁定版本、支持当前 Tauri/serde/Channel 合同且输出确定的成熟工具；spike 不通过时退回窄 repository generator，不让生成器进入 runtime。
- 成熟基础设施也必须受生命周期治理。依赖台账记录当前版本、官方支持状态、安全公告、MSRV/Node/Windows/Tauri 兼容性、升级建议、owner 和复查日期；架构迁移与大版本升级默认分 shard，但 unsupported/EOL 或不可接受安全风险不能以“避免扩大范围”为由延期到 release 之后。
- architecture fitness 优先由 Rust/TypeScript 可见性、类型系统、编译后 registry、ESLint 标准规则和行为测试承担；自定义 AST graph 只覆盖这些机制无法表达的跨模块约束。

## 6. 目标模块边界

### 6.1 前端

```text
src/
  app/
    bootstrap/
      backendMode.ts
      createBackendClient.ts
    navigation/
      ...

  lib/
    bridge/
      generated.ts          # 构建生成，不手改
      BackendClient.ts      # 仅供 bootstrap 组合，不注入 feature
      DesktopBackend.ts
      DemoBackend.ts
      errors.ts
      runtimeMode.ts

  features/
    stations/
      api.ts                # 领域 client，不直接 import Tauri invoke
      queries.ts
      mutations.ts
      models.ts
      viewModels.ts
      components/
      pages/
    key-pool/
      ...
```

边界规则：

- `features/**` 不直接 import `@tauri-apps/api/core`。
- `features/**` 不识别 Tauri 原始错误字符串。
- `bridge/generated.ts` 不包含 mock、toast、query invalidation 或 view model。
- `DemoBackend` 不 import Desktop binding，也不执行真实网络、凭据或文件系统操作。
- feature hook/component 只能依赖对应领域 client，不能接收完整 `BackendClient` 并从中查找其他领域。
- 领域 client 可以是 generated command group 的直接窄接口；只有需要 desktop/demo 组合、stream adapter 或跨 command transport policy 时才增加 wrapper，禁止为每个 generated method 再写无行为转发层。
- query key、queryFn、stale policy 和 aggregate read model 在 feature query 层定义。
- 页面只组合 hooks 和组件，不直接构造跨领域 transport 调用图。

### 6.2 Rust

```text
src-tauri/src/
  application/
    command_facades/
      stations.rs
      station_keys.rs
      collectors.rs
      routing.rs
      ...

  commands/
    mod.rs                  # 只注册和重导出
    stations.rs
    station_keys.rs
    collectors.rs
    routing.rs
    proxy.rs
    settings.rs
    changes.rs
    logs.rs
    updater.rs
    data_recovery.rs
    error.rs

  background_tasks/
    mod.rs
    supervisor.rs
    task.rs
    operation.rs
    blocking.rs
    status.rs
    shutdown.rs

  outbound/
    mod.rs
    client.rs
    policy.rs
    proxy.rs
    error.rs

  services/collectors/
    orchestration.rs
    contract.rs
    failure.rs
    evidence.rs
    drivers/
      mod.rs
      sub2api/
        mod.rs
        auth.rs
        client.rs
        endpoints.rs
        parsers.rs
        mapping.rs
      newapi/
        ...
      openai_compatible/
        ...
```

边界规则：

- command module 依赖 application service 或明确 runtime facade，不直接编排 provider、SQL 或页面 read model。
- command 每次只注入一个与命令领域相符的 facade；禁止接收完整 `AppServices`。
- command error 映射集中在 `commands/error.rs`。
- supervisor/operation registry 不理解 collector、monitor、connectivity 或 proxy 的业务语义，只理解 work lifecycle；它们不持有业务 service registry。
- collector orchestrator 不解析 provider payload。
- driver 不访问 Tauri `State`、UI DTO、Query key 或持久化 store。
- driver 输出 canonical facts、evidence 和 typed failure；是否持久化由 application 层决定。
- proxy runtime 保留自身 request lifecycle，supervisor 只协调其进程级启动和 shutdown。
- provider、探测和管理面网络默认只依赖 `AsyncOutboundClient`；阻塞 OS/WebView 能力只能依赖 `BlockingExecutor`。

### 6.3 Composition root 与依赖可见性

`app_composition.rs` 和 `runtime_composition.rs` 是唯一允许组装 concrete service、driver registry、supervisor 和 command facade 的位置。它们可以临时使用 construction-time bundle，但 bundle 不得作为 Tauri managed state 暴露给 command。

目标形态：

```text
build_application_services()
  -> build_provider_registry()
  -> build_background_runtime()
  -> build_domain_command_facades()
  -> preflight all concrete Tauri state slots
  -> register atomically
```

约束：

- `AppServices` 若继续存在，只能是 composition 内部、不可被 command 获取的 construction bundle。
- 每个 command facade 只暴露该领域用例，不公开内部 service 字段。
- 若一个现有 application service 已经只覆盖单一领域且接口适合作为 command state，可直接注册该窄 service；只有跨多个 service 的明确 use case 才创建 command facade。
- 跨领域用例由明确的 application use case 组合，不在 command 中临时抓取多个 service。
- facade 不复制业务逻辑，也不镜像转发 service 的全部方法；它只固定跨服务用例所需的最小依赖集合并提供面向 transport 的窄入口。
- command facade、provider registry、supervisor 的注册沿用 Persistence V2 收尾后确定的原子 composition 机制；本 spec 不改变 persistence state/schema/session。
- 新增 service 时，architecture gate 必须证明没有扩大无关 command 的 transitive dependency fan-out。

## 7. Typed IPC 合同

### 7.1 权威来源

Rust command input、output 和 public error 是 IPC 契约的唯一权威。TypeScript 绑定必须在构建期生成并提交，或在 CI 中生成后验证工作区无 diff。

首选工具路径是验证 `specta` / `tauri-specta` 能否覆盖当前普通 command、serde rename、enum 和 `Channel`。若 streaming `Channel` 无法可靠生成，只允许将 streaming transport 保留为一个经过类型封装的手写 adapter；普通 commands 仍必须生成。生成工具不进入运行时决策路径。

不得为了适配生成器而把内部 domain model 全部公开。为 IPC 定义窄 DTO，并显式实现 domain/DTO conversion。

### 7.2 Public command error

```rust
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: CommandErrorCode,
    pub message: String,
    pub retryable: bool,
    pub details: Option<PublicErrorDetails>,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CommandErrorCode {
    InvalidInput,
    NotFound,
    Conflict,
    PermissionDenied,
    RuntimeUnavailable,
    DataStoreUnavailable,
    ExternalUnavailable,
    Timeout,
    Overloaded,
    Unsupported,
    Internal,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicErrorDetails {
    Validation { fields: Vec<PublicFieldError> },
    Conflict { resource: String, current_revision: Option<String> },
    Retry { retry_after_ms: Option<u64> },
    External { provider: Option<String>, upstream_status: Option<u16> },
}
```

约束：

- `message` 面向用户但不泄露 secret、cookie、token、完整 URL query 或数据库路径。
- `details` 必须是封闭 tagged enum；禁止退回无约束 JSON、字符串 map 或直接序列化内部 error chain。
- `PublicFieldError` 只包含稳定 field code 和公开 message，不包含用户 secret 原值。
- `Internal` 默认不可重试；只有 owner 明确证明幂等且瞬态时才可标记 `retryable=true`。
- `retryable=true` 只表示允许调用者考虑重试，不授权 transport 自动重放 mutation；是否重试仍受 command idempotency 合同约束。
- `PublicErrorDetails::Retry` 只能出现在允许重试的 error；`retry_after_ms` 与顶层 retryable/idempotency 不一致时，Rust 构造器必须拒绝创建该 public error。
- `PublicErrorDetails` 只收纳稳定、跨 command 有意义的公开类别；领域结果或大诊断数据应进入专用 result/diagnostic DTO，不能不断扩张 error details。
- command adapter 只映射 error，不根据 error 决定业务补偿。
- 前端根据 `code` 决定交互，根据 `message` 展示，不解析 message。
- 未知 code 映射为 `internal` 并保留 correlation id，不猜测成 runtime unavailable。
- application、operation、driver 和 outbound 各自使用 typed internal error；`String` 只能作为已脱敏的叶子诊断，不能承担 retry、auth effect、terminal 或映射决策。

### 7.3 Command 命名和分组

- command 名称保持稳定 snake_case，生成 binding 隐藏字符串。
- 破坏性变更优先新增 versioned command，迁移所有调用者后删除旧 command。
- 同一资源的 list/get/create/update/delete/reorder 必须归属同一 command module。
- streaming command 单独标注 transport contract、取消语义和 terminal event。
- 所有 mutation 必须声明幂等性：`Idempotent`、`IdempotentWithKey` 或 `NonIdempotent`。
- transport 不自动重试 mutation。`IdempotentWithKey` 必须由 frontend 在一次用户意图内复用稳定 operation key，并由 application owner 去重；重新点击产生新 key。
- `NonIdempotent` 在 response 丢失时返回结果未知语义，不能通过重新 invoke 猜测完成。
- ACL/capability manifest 必须由同一 command registry 校验；生成了 binding 但未授权、已授权但未注册都应使 CI 失败。

### 7.4 生成门禁

- `pnpm generate:bindings` 生成确定性输出。
- CI 运行生成命令后要求 `git diff --exit-code -- src/lib/bridge/generated.ts`。
- Rust serialization fixture 与 TypeScript decode fixture 覆盖 nullable、enum、rename、时间、分页和 partial result。
- 禁止手改 generated 文件；文件头必须包含 generator version 和 canonical contract hash。无关源码提交不得改变 generated output。

### 7.5 Runtime contract handshake

Desktop bootstrap 在加载业务页面前调用最小、稳定的 `get_runtime_contract_info`：

```rust
pub struct RuntimeContractInfo {
    pub app_version: String,
    pub ipc_contract_version: u32,
    pub binding_hash: String,
    pub capabilities: Vec<RuntimeCapability>,
}
```

- generated binding 内嵌相同 contract version/hash。
- version/hash 不匹配时进入明确 incompatible-runtime recovery screen，不继续调用业务 command，也不切换 DemoBackend。
- capability 用于表达当前 binary 是否支持某组 command/stream，不根据 command-not-found 文本做探测。
- streaming event envelope 携带 event schema version；未知 version 终止 operation 并返回 typed incompatible error。
- handshake 不能携带 secret、数据库路径或可变业务数据。
- 同一安装包正常情况下必然匹配；该 gate 主要保护 dev HMR、旧 WebView asset、部分更新和错误 composition。

## 8. 显式 Backend Runtime

### 8.1 启动时选择

```ts
export type BackendMode = "desktop" | "demo";

export interface BackendClient {
  stations: StationClient;
  stationKeys: StationKeyClient;
  collectors: CollectorClient;
  routing: RoutingClient;
  proxy: ProxyClient;
  settings: SettingsClient;
  changes: ChangeClient;
  logs: LogClient;
}
```

`createBackendClient(mode)` 只能在 app bootstrap 调用一次。feature 不得读取 `window.__TAURI__`、环境变量或动态探测 mode。

Desktop mode 的判定必须来自明确的 Tauri bootstrap 成功，而不是任意 invoke 失败。bootstrap 失败时进入应用级 recovery/error screen，不进入 demo。

启动 composition 必须使用两个显式入口：

- production Tauri entry 固定走 `SelectingMode -> HandshakingDesktop -> DataStoreBootstrapping -> Ready`；contract mismatch/runtime unavailable 都 fail closed。
- browser preview/test 使用独立 Vite entry/build command 固定走 `SelectingMode -> DemoReady`；不得挂载 `DataStoreBootstrap` 或先尝试 desktop invoke。

不得用同一 production bundle 中的 runtime env/query string/localStorage 开启 demo。优先让 production build tree-shake/exclude demo entry；即使共享 interface/type，packaged binary 也必须通过 build/architecture gate 证明 preview composition 不可达。

### 8.2 DesktopBackend

- 只调用 generated binding 或手写 streaming adapter。
- 不捕获错误并返回业务默认值。
- transport error 统一转换为 frontend `BackendError`，保留 command code 和 correlation id。
- mutation 成功以 Rust 返回成功为准，不能在请求发送后提前更新为 durable success。

### 8.3 DemoBackend

- 只用于明确的 browser preview、组件开发和演示。
- 数据集固定、可重置、确定性，不使用当前时间生成不可重复状态。
- 所有写入仅修改 demo store，并在 UI 中保持 demo mode 可识别。
- 不读取真实 keyring、数据库、cookie、系统代理、更新器或本地文件。
- 不复制 provider 登录、采集、路由和价格决策；不支持的能力返回 typed `Unsupported`。

### 8.4 删除规则

- 每个 feature 切换到 `BackendClient` 后，删除其 API 文件内的 `isTauriInvokeUnavailable` catch。
- 所有 feature 完成后，禁止 `src/lib/api/**` 出现内存业务状态。
- `isTauriInvokeUnavailable` 只能在 bootstrap transport probe 中存在，业务 API 中为零。

## 9. 前端数据所有权

### 9.1 状态分类

| 类型 | Owner | 示例 |
|---|---|---|
| Server state | TanStack Query | stations、keys、settings、logs、runtime status |
| Derived server state | selector/view model | 当前余额、风险标签、route explanation |
| URL/navigation intent | navigation controller | 当前 shell route、transient page |
| View state | 页面或组件 | filter、sort、selected row、dialog open |
| Draft state | form controller | 尚未提交的站点、Key、设置表单 |
| Operation state | operation controller keyed by operation id | connectivity/scan/capture progress 与 terminal |
| Runtime mode | app bootstrap | desktop/demo |

禁止把 query result 复制到 `useState` 后作为长期读源。需要编辑时，应从 query data 初始化独立 draft；mutation 成功后通过 cache update/invalidation 获取新 server truth。

Operation progress 不进入普通 resource Query Cache，也不由页面零散 state/run token 充当权威。feature operation controller 订阅 backend `OperationRegistry`，按 operation id 保存有界 progress 和唯一 terminal；页面只订阅 controller。是否跨页面保留由 operation policy 决定。

### 9.2 Query 合同

- 每个资源只有一个 canonical query key factory。
- query options 必须显式定义 stale time、refetch trigger、timeout 和 partial semantics。
- hidden/unmounted 页面不得主动发起 polling。
- app shell 只查询全局必需的轻量状态，不重复页面 workspace query。
- mutation owner 负责精确 invalidation 或 atomic cache update，禁止用 DOM CustomEvent 通知数据变化。
- 同一数据不得同时由 query polling 和 page activation 的手工 loader 刷新。
- query error cycle 只负责通知去重，不改变 query 失败语义。
- mutation 开始前取消同资源的过期 refetch；成功时先应用 authoritative response，再按契约决定是否 refetch。
- 跨资源 mutation 默认等待相关 invalidation 完成后才报告 UI settled；不能让较早 query response 覆盖较新的 mutation result。
- optimistic update 只用于可逆、单资源、具备精确 rollback snapshot 的操作；跨站点/Key/路由联动默认不用 optimistic success。

### 9.3 Aggregate read models

列表页需要的行数据由后端一次返回，避免每行 command：

```text
list_station_workspace()
  -> stations
  -> current balance fact
  -> latest collector status
  -> key count / schedulable count
  -> current risk summary
```

约束：

- command 数量与站点数量无关，正常刷新为 O(1) 个 workspace command。
- partial 字段携带 `availability` 或 `errorCode`，不能用 `0`、空数组冒充查询成功。
- read model 是只读投影，不成为新的写入模型。
- proxy/runtime 不消费 UI read model，继续消费 canonical runtime snapshot。

### 9.4 页面生命周期

默认 shell 只同时挂载：

- 当前页面；
- 页面切换期间的上一个页面；
- 当前 transient page。

默认不常驻所有访问过的 shell 页面。快速返回依赖 Query Cache 和显式 prefetch，而不是挂载隐藏页面。

页面状态收敛为：

```text
Entering -> Active -> Leaving -> Unmounted
```

如果个别页面确需保留昂贵、不可序列化的 draft，必须通过显式 `PageRetentionPolicy` allowlist，并证明：

- background 时无 polling、无 focus side effect、无键盘响应；
- draft owner 唯一；
- 内存有上限；
- 退出应用不依赖 React unmount 才能提交业务数据。

页面可见性只有一个 host-owned 权威：

```ts
type PageVisibility = "foreground" | "background";
```

- current/entering page 为 `foreground`；leaving、retained 和 transient page 背后的 shell page 为 `background`。
- feature query hook 可以消费 `PageVisibility`，统一映射 query `enabled/subscribed/refetchInterval`；页面不得再组合 `interactive + refreshEnabled + activation callback`。
- `background` 页面保留现有 cache data，但不启动 polling、refetch-on-focus 或 activation loader。
- foreground 切换只触发 TanStack Query 标准 stale/refetch 语义，不调用第二套 `refresh()`。
- 预取使用 `queryClient.prefetchQuery`，不通过预挂载页面实现。
- host 只负责 visibility，不知道具体 query key、stale time 或 feature 业务。

因此，现有 `PageActivity` 可以作为迁移 adapter，但最终要收敛成单一 visibility context；`useActivityQuery` 可以保留一个等价的薄封装，但其输入只能是 `PageVisibility`。它可以在 invariant 被违反时记录 hidden-query metric，但不得真的执行隐藏 query，也不得用第二次 refresh 补偿。

### 9.5 页面拆分准则

页面只负责 layout 和 feature composition。以下职责必须外移：

- query/mutation：`queries.ts`、`mutations.ts`；
- form reducer/validation：`formModel.ts`；
- streaming connectivity test：独立 controller/hook；
- provider/group/pricing projection：纯 view model；
- dialog：独立组件；
- table/list row：独立可测试组件。

不设置机械的最大行数 gate，但单一页面同时出现 transport、query cache、复杂 reducer、stream parser 和多个 modal owner 时，architecture test 必须失败。

## 10. Work Lifecycle：TaskSupervisor 与 OperationRegistry

### 10.1 任务模型

```rust
pub(crate) struct TaskSpec {
    pub id: TaskId,
    pub kind: TaskKind,
    pub restart: RestartPolicy,
    pub shutdown_timeout: Duration,
    pub concurrency_key: Option<TaskConcurrencyKey>,
}

pub(crate) enum TaskStatus {
    Registered,
    Starting,
    Running,
    BackingOff { attempt: u32, retry_at: Instant },
    Stopping,
    Stopped,
    Failed(TaskFailure),
    ShutdownTimedOut,
}

pub(crate) enum RestartPolicy {
    Never,
    OnTransientFailure { max_backoff: Duration },
    Always { max_backoff: Duration },
}
```

每个任务获得 child `CancellationToken`，并由 supervisor 持有 join handle。runner 不持有自己的 OS thread shutdown 协议。

实现约束：

- 使用 Tokio/Tokio-util 的稳定 primitive 承担执行：`CancellationToken` 传播取消，`TaskTracker` 或有界 `JoinSet` 跟踪 join，`Semaphore`/bounded channel 做 admission/backpressure，`watch`/bounded status projection 做状态通知。
- Supervisor 不实现线程池、调度器、future executor、通用 mailbox 或任意 service lookup；它只实现注册、状态迁移、restart/backoff、concurrency key 和 shutdown policy。
- 选择 `TaskTracker` 还是 `JoinSet` 由 Stage 0 spike/ADR 固定；必须证明 panic/join error、close/wait、重复 shutdown 和 task admission race 的行为，禁止两套 primitive 长期并存。

### 10.2 周期任务循环

- 使用 Tokio interval/sleep 与 `tokio::select!` 等待 tick 或 cancellation。
- 禁止 `thread::sleep`、`async_runtime::block_on` 包裹永久 runner。
- 每次 tick 生成独立 run id，记录 started、completed/failed、duration 和 processed count。
- 同一 concurrency key 默认禁止重入。
- 单次 run 超时不等于 runner 退出；failure classifier 决定 continue、backoff 或 terminal failed。
- backoff 有上限并加入小幅 jitter，避免多个任务同步重试。

### 10.3 失败分类

```rust
pub(crate) enum TaskFailureClass {
    TransientExternal,
    TransientRuntime,
    Configuration,
    Authentication,
    Invariant,
    Panic,
    Cancelled,
}
```

- 只有 transient failure 可自动退避重试。
- configuration/authentication failure 保持可见并等待配置变化或显式重启。
- invariant/panic 进入 failed，不无限自动重启。
- cancellation 是正常终态，不记录为业务失败。

### 10.4 Shutdown 顺序

```text
1. stop admitting new UI-triggered background work
2. stop periodic collector/monitor scheduling
3. request proxy drain through existing ProxyRuntime
4. wait in-flight task runs within their budgets
5. flush structured diagnostics
6. release runtime resources through existing shutdown owner
```

Supervisor 聚合结果；任一任务超时必须进入最终 shutdown report。不得因为 process 即将退出而吞掉 join error。

Tauri lifecycle 约束：

- tray Quit、窗口真正退出、updater restart、OS exit request 和测试 shutdown 必须进入同一个幂等 `ExitCoordinator::request_exit(reason)`；隐藏到托盘不是 exit。
- 在 `RunEvent::ExitRequested`/等价可阻止阶段停止接纳新工作并启动异步 drain；最终 drain 完成或全局 deadline 到达后才调用一次真正 exit。不得等到 `RunEvent::Exit` 后再用 `block_on` 启动主要 shutdown。
- repeated exit request 返回/订阅同一个 in-flight shutdown，不重复 drain、释放 lease 或关闭 persistence。
- 强制 kill、系统崩溃和电源中断无法保证 graceful cleanup；下次启动只能报告上次非正常终止并执行各 owner 已有恢复合同，不能声称所有 operation 已取消成功。

### 10.5 Runtime status

前端可通过一个只读 command 获取 task summary：

```rust
pub struct RuntimeTaskSummary {
    pub id: String,
    pub status: RuntimeTaskStatus,
    pub last_started_at: Option<String>,
    pub last_succeeded_at: Option<String>,
    pub last_failure_code: Option<String>,
    pub consecutive_failures: u32,
    pub next_retry_at: Option<String>,
}
```

不得返回内部 error chain 或 secret。默认 UI 只展示用户可行动状态，完整诊断受开发者模式控制。

### 10.6 用户触发的长操作

Key connectivity、remote-key scan/create、manual collector run、web authorization completion 和其他超过普通 command timeout 的工作使用 `OperationRegistry`。它复用 supervisor 的 cancellation/join/status primitives，但与 daemon registry 分开，避免一个万能 manager 同时理解所有业务。

```rust
pub(crate) struct OperationSpec {
    pub id: OperationId,
    pub kind: OperationKind,
    pub owner: OperationOwner,
    pub deadline: Duration,
    pub concurrency_key: Option<TaskConcurrencyKey>,
    pub cancellation: CancellationPolicy,
}

pub(crate) enum OperationTerminal {
    Completed,
    Failed(OperationFailureCode),
    Cancelled,
    TimedOut,
    ResultUnknown,
}
```

合同：

- start 返回 operation id；progress 和 terminal event 都携带同一 id。
- 同一 operation 最多一个 terminal；Channel close 本身不等于 completed。
- `cancel_operation(id)` 必须推进 cancellation token，并等待或报告尚未停止，不能只让前端忽略后续 event。
- 页面关闭时按 `CancellationPolicy` 决定 cancel 或 detach；策略由 operation kind 固定，不由组件临时猜测。
- cancellation 后禁止开始新的 retry、fallback 和持久化副作用；已经越过不可逆 commit barrier 时返回 `ResultUnknown` 或真实 terminal。
- operation registry 只保存 bounded status/handle，不保存完整 response body、secret 或无限 progress history。
- admission 必须原子完成 id allocation、capacity/concurrency-key check、handle registration 和 spawn；不得出现工作已启动但 registry 无法查询/取消。
- progress 使用有界 buffer，可按 operation kind 合并或丢弃中间进度；terminal/result summary 使用不会被 progress 挤掉的独立状态/查询路径。lagged subscriber 通过 operation id resync，不把 Channel close 猜成终态。
- terminal summary、progress ring 和 result projection 有固定 TTL/容量/GC；只淘汰已 terminal 项，running handle 永不因容量回收。GC 后返回 typed Expired/NotFound。
- detach 后仍可查询脱敏 terminal/result projection。远端资源创建等不可逆 operation 必须声明 idempotency/reconciliation key；response 丢失时返回 `ResultUnknown`，不能自动重放。
- 普通短 command 不强行包装成 operation；只有需要 progress、cancel、跨页面存活或显著超时预算的工作才进入 registry。
- `OperationFailureCode` 属于 work/application 边界，command adapter 再映射为 public `CommandErrorCode`；background task/operation 不依赖 transport error 类型。

### 10.7 BlockingExecutor

确实只能同步执行的 OS dialog、WebView cookie API、keyring 或文件系统兼容调用进入统一 `BlockingExecutor`：

- semaphore 和等待队列容量在 Stage 0 冻结；
- 获取 permit 有 queue timeout；
- job 有 operation/correlation id、deadline 和 completion status；
- Tokio runtime 上禁止在持有 async mutex/transaction 时进入 blocking job；
- blocking job 无法被强制中断时，cancellation 后其结果必须被丢弃，且不得继续业务 commit；
- 无法及时停止的 orphaned blocking job 必须计数并进入 shutdown diagnostics。

网络 I/O、HTTP body、SSE 和 provider retry 不属于 blocking exception，必须迁移到 async outbound。production `spawn_blocking` 和 `thread::spawn` 使用 parser-backed allowlist；测试 server/thread 与真实 OS blocking port 单独分类。

## 11. Provider Driver 与 Async Outbound

### 11.1 编译期封闭接口

```rust
pub(crate) trait CollectorDriver: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn capabilities(&self) -> DriverCapabilities;

    fn collect<'a>(
        &'a self,
        context: &'a CollectorContext,
        task: CollectorTask,
    ) -> BoxFuture<'a, Result<DriverOutput, DriverFailure>>;
}
```

`ProviderKind` 使用封闭 enum，不使用自由字符串。driver registry 在 composition root 静态注册，不允许运行时从目录加载代码。

### 11.2 输入与输出

`CollectorContext` 只包含执行所需的稳定输入：

- station identity 和 endpoint revision；
- website/api endpoint roles；
- 已解析的 proxy route；
- 脱敏 credential handle，不暴露 UI model；
- request budget、clock、correlation id；
- 共享 outbound client 和 recovery policy port。

credential handle 只能通过 application-owned 短生命周期 accessor 解析为不可 Clone/Debug 的 secret；driver 不缓存 raw key/cookie/token。相同 station/credential revision 的 auth/session refresh 使用 single-flight，并共享原 operation 的 remaining budget；stale revision、等待者取消和 refresh side effect 必须可测试。

`DriverOutput` 包含：

- canonical `CollectorFacts`；
- endpoint evidence；
- provider capability observations；
- completeness/partial 状态；
- 可安全持久化的 redacted diagnostics。

driver 不写数据库、不发 change event、不更新 query cache、不决定下一次调度时间。

### 11.3 Driver 内部分层

```text
auth.rs       credential/session/header strategy
client.rs     provider request/response transport wrapper
endpoints.rs  endpoint role and path selection
parsers.rs    payload -> provider response types
mapping.rs    provider response -> canonical facts
mod.rs        task composition only
```

- auth 不解析余额或模型。
- client 不映射 canonical facts。
- parser 不执行网络和重试。
- mapping 不读取环境、设置、数据库或时间。
- `mod.rs` 不复制 HTTP request construction。

### 11.4 Typed failure

```rust
pub(crate) struct DriverFailure {
    pub kind: DriverFailureKind,
    pub retry: DriverRetryDisposition,
    pub auth_effect: AuthEffect,
    pub endpoint: Option<EndpointRole>,
    pub sanitized_detail: Option<String>,
    pub evidence: Vec<EndpointEvidence>,
}
```

固定覆盖：invalid endpoint、proxy resolution、connect、timeout、HTTP auth、rate limit、server error、malformed JSON、unsupported shape、missing facts、stale endpoint revision、cancelled。

错误分类由共享 policy 和 provider-specific mapping 协作完成，orchestrator 不解析 message。

### 11.5 新 provider 扩展流程

新增 provider 必须同时提交：

1. `ProviderKind` 和 `DriverCapabilities`。
2. 所需 capability driver 实现与 composition registration。
3. endpoint/auth matrix。
4. redacted success、partial、auth failure、rate limit、malformed 和 compatibility fixtures。
5. conformance suite。
6. secret/log redaction proof。

不得为了新增 provider 修改 station collector runner、TaskSupervisor、Query 层或通用 persistence workflow。

### 11.6 ProviderRegistry 与能力拆分

只建立一个 provider identity/registration 权威，但不建立万能 `ProviderDriver`：

```rust
pub(crate) struct ProviderEntry {
    pub descriptor: ProviderDescriptor,
    pub collector: Option<Arc<dyn CollectorDriver>>,
    pub remote_keys: Option<Arc<dyn RemoteKeyDriver>>,
    pub authorization: Option<Arc<dyn AuthorizationDriver>>,
}

pub(crate) trait RemoteKeyDriver: Send + Sync {
    fn capabilities(&self) -> RemoteKeyCapabilities;
    fn list<'a>(&'a self, context: &'a ProviderContext)
        -> BoxFuture<'a, Result<Vec<RemoteKeyRecord>, DriverFailure>>;
    fn create<'a>(&'a self, context: &'a ProviderContext, input: CreateRemoteKeyRequest)
        -> BoxFuture<'a, Result<CreatedRemoteKey, DriverFailure>>;
}
```

约束：

- `ProviderRegistry` 只能按 `ProviderKind` 查找 capability，不执行网络、重试、持久化或任务调度。
- collector orchestration 只请求 collector capability；remote-key service 只请求 remote-key capability。
- 缺少 capability 返回 typed `Unsupported`，不通过 provider 名称 `match` 拼默认行为。
- authorization capability 只表达 provider-specific session/header 验证；WebView window lifecycle 仍属于 capture service。
- provider module 可以复用 auth/client/parser，但 capability driver 不能互相调用对方的 orchestration。
- 历史数据库出现当前 binary 未知的 provider 字符串时必须可只读保留并标记 unsupported；不能反序列化失败、错误映射到 custom 或参与路由。
- 新 provider 不得要求在 remote-key、collector 和 authorization 三个 service 分别新增字符串分支。

### 11.7 AsyncOutboundClient

Provider management、collector、remote-key、endpoint ping、channel probe、connectivity test 和 web-authorization HTTP 验证统一依赖共享 async outbound。实现可基于已经用于 proxy 的 `reqwest`，但 client/policy 位于中立 outbound 边界，不能反向依赖 proxy runtime。

```rust
pub(crate) trait AsyncOutboundClient: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: OutboundRequest,
        budget: &'a RequestBudget,
        cancel: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<OutboundResponse, OutboundFailure>>;
}
```

统一治理：

- direct/system/manual HTTP/SOCKS proxy 解析；
- client pooling、TLS、redirect policy 和 connection reuse；
- connect/first-byte/body/total timeout；
- request/response body size limit；
- cancellation 和 operation deadline；
- header allowlist、secret redaction 和 endpoint evidence；
- retry-after 解析和剩余 budget，禁止每层重新获得完整 timeout；
- typed connect/timeout/status/body/decode failure。

安全/资源合同：

- URL 禁止 userinfo/control characters；endpoint role 决定允许 scheme、origin 和 redirect policy。
- cross-origin/scheme redirect 不携带 Authorization、Cookie 或 provider-specific secret header；HTTPS 不自动降级 HTTP。
- request/response/error body limit 作用于实际读取/解压后的 bytes；超限或 cancel 时停止读取并释放 connection/body resource。
- secret header/value 使用不可 Debug/Display 的短生命周期 wrapper，不进入 client pool key、retry clone、redirect history、evidence、trace 或 metric。
- client pool key 只包含稳定、低基数、无 secret 的 transport policy；client 数量与 request 数量无关，并有空闲回收/上限诊断。

边界：

- outbound 不解析 provider JSON，不决定 auth refresh、task success 或 health effect。
- driver 不直接构建 `ureq::Agent` / `reqwest::Client`，只构建 typed `OutboundRequest`。
- proxy request forwarding 可继续使用其经过验证的 client/cache 和 lifecycle；共享的是中立 proxy route/policy value object，不强行合并 request lifecycle。
- updater 优先使用 Tauri updater plugin；仍需直接 HTTP 的 inspection/check 必须使用 outbound 或登记有理由的独立 transport adapter。
- 最后一个 production 调用者迁移后删除 `ureq` dependency；测试 fixture server 可使用独立测试依赖，不构成 production 例外。

### 11.8 Connectivity Probe Service

`test_station_key_connectivity` 从 command module 移入独立 application/service 边界：

- command 只启动 cancellable operation 并转发 typed progress；
- probe service 负责模型候选、Responses/Chat fallback 和最终 `ConnectivityOutcome`；
- OpenAI-compatible request builder、SSE terminal decoder 和 envelope validator 放在可复用的 protocol probe kernel；
- protocol probe kernel 不依赖 Query、Tauri Channel、provider registry、routing scheduler 或 persistence；
- 与 proxy protocol 语义完全相同的 decoder 必须共享 fixture/contract，不能复制后逐渐漂移；proxy-specific lifecycle 状态仍留在 proxy。

## 12. Commands 与应用边界

### 12.1 Command adapter 职责

每个 command 只允许：

1. 接收入参和 Tauri state。
2. 执行 transport-level validation，例如参数大小上限。
3. 调用一个窄领域 command facade 方法。
4. 把内部错误映射成 `CommandError`。
5. 返回 IPC DTO。

command 不允许：

- 直接拼复杂 SQL；
- 选择 provider endpoint；
- 复制 collector orchestration；
- 创建永久后台线程；
- 注入或向下传递完整 `AppServices`；
- 直接调用 `ureq`/`reqwest`、解析 SSE 或执行 provider fallback；
- 无容量控制地调用 `spawn_blocking`；
- 解析 UI 文案或 query key；
- 在失败时返回 mock/default success。

### 12.2 拆分策略

- 先建立 generated command registry 和统一 error，再拆 `commands/mod.rs`。
- 按领域移动函数，不在同一提交改变行为。
- `commands/mod.rs` 最终只保留 module declarations、re-exports 和生成器所需 registry。
- command 注册列表必须由生成/编译期机制覆盖，避免新增 command 后只注册一端。
- connectivity、remote-key 和 capture 等复杂 command 必须先迁出业务实现，再做物理文件移动；不能把 1000 行 helper 原样搬到新的 command 文件。

## 13. 错误与可观测性

### 13.1 Correlation

- 每个 IPC command、collector run、monitor run、proxy request 和 supervised task run 都有 correlation/run id。
- 跨层调用沿用同一 id，不用错误 message 关联日志。
- 前端错误提示可包含短 correlation id，便于定位本地诊断。

### 13.2 Structured tracing

Rust 统一采用 `tracing` event/span：

- target/module；
- operation/task kind；
- correlation id；
- duration；
- stable error code；
- result status；
- redacted resource identity。

禁止记录完整 API key、cookie、token、Authorization header、用户 prompt/response body 和未经清洗的 provider payload。

### 13.3 最小指标

- command latency/error count，按 command/error code；
- query workspace latency 和 payload size；
- task running/backoff/failure/shutdown timeout；
- collector run duration、provider/task/failure class；
- generated binding drift；
- hidden-page query starts；
- per-page IPC count。

指标用于本地诊断和测试，不引入云遥测。

## 14. 架构门禁与测试策略

### 14.1 测试层级

| 层级 | 主要责任 |
|---|---|
| Rust unit | error mapping、task state machine、driver parser/mapping |
| Rust integration | command serialization、supervisor shutdown、driver orchestration |
| TypeScript unit | BackendError、query keys、cache transitions、view models |
| React component | loading/error/partial/mutation/draft lifecycle |
| Tauri smoke | generated command wiring、desktop bootstrap、真实错误 envelope |
| Performance/soak | O(1) workspace query、task backpressure、shutdown drain |

### 14.2 Parser-backed fitness functions

结构规则分层使用成熟机制：

- Rust 首先依赖 module visibility、类型系统和编译后 registry tests；跨模块 graph 使用 `cargo metadata`/真实 target cfg + `syn` visitor。`syn` 不负责猜测 macro expansion。
- TypeScript 的直接 import 边界优先使用 ESLint 标准规则；path alias、barrel/re-export、dynamic import 和 descendant graph 使用基于真实 `tsconfig` Program 的 TypeScript Compiler API 检查。
- command registration/ACL/binding 一致性使用编译后的 registry/serialization fixture，不用源码 regex 或 AST 猜 `generate_handler!`。
- dependency manifest 明确 allowed/forbidden/temporary edges、public exports、fan-in/fan-out baseline、owner 和 expiry。

parser gate 本身必须有 bypass regressions，覆盖 qualified/ordinary path、glob、alias、inline/out-of-line/nested module、`cfg/cfg_attr`、barrel/type-only/dynamic import、same-name symbol、descendant fan-out、cycle 和 stale/empty allowlist。无法可靠解析的 construct fail closed 或登记精确、有期限例外；不得静默忽略。

必须检查：

- `features/**` 不 import Tauri core。
- Desktop invoke 只存在于 bridge。
- 业务 API 不包含 `isTauriInvokeUnavailable` fallback。
- commands 不依赖 provider driver 的内部 parser/client。
- commands 不接收 `State<AppServices>`，每个 command 只接收 allowlist 中的领域 facade/state。
- drivers 不依赖 commands、Tauri state、frontend 或 persistence store。
- 永久 runner 不使用 `thread::spawn + block_on`。
- 后台 fire-and-forget spawn 只有显式 allowlist。
- production provider/probe/management HTTP 不直接使用 `ureq` 或自行构建 HTTP client。
- feature 不能接收完整 BackendClient；只能依赖领域 client/query hook。
- operation progress/terminal event 必须携带 operation id，且 cancellation command 有实际后端 owner。
- generated binding 与 Rust contract 同步。

源码 regex 只允许检查固定配置文本、敏感字面量和 generated marker，不得用来推断 Rust module/call graph 或 TypeScript symbol ownership。

### 14.3 行为基线

迁移前必须锁住：

- Desktop 正常读写、ACL 拒绝、command missing、runtime unavailable。
- Station/Key CRUD、reorder、group binding 和 cache refresh。
- 页面切换后无隐藏 polling，返回页面仍读取最新 cache。
- collector/monitor 单实例、失败退避、取消和关机等待。
- Sub2API/NewAPI 各任务 success/partial/failure 事实保持一致。
- proxy startup、drain 和 request lifecycle 不因 supervisor 接入而改变。
- demo mode 不访问真实系统能力。

### 14.4 工程门禁

新增并纳入 CI：

```text
pnpm install --frozen-lockfile
pnpm lint
pnpm generate:bindings --check
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

Windows Cargo 检查串行运行，使用明确 target directory，避免多个 gate 争用锁和污染开发 watcher。

CI 分两层：

- `ci.yml` 在 pull request 和主分支 push 运行 binding drift、lint、format、unit/integration、frontend build、architecture gates 和 `cargo check/clippy`。
- `release.yml` 在 tag 上复用相同脚本，再增加 locked release build、Tauri bundle、签名、artifact scan 和 release-only qualification。

release workflow 不能拥有另一份复制的验证命令；两层必须调用仓库内同一 fail-closed entrypoint。PR gate 未通过的 revision 不具备发布资格。

依赖治理：

- Node 与 Cargo lockfile 必须提交，CI 使用 frozen/locked 模式。
- 定期运行 RustSec/cargo-deny 等 advisory 和来源/license 检查，以及 production Node dependency advisory 检查；具体工具在 Stage 0 ADR 固定版本和例外格式。
- advisory 例外必须包含 dependency、漏洞编号、影响判断、owner 和到期日期，禁止永久全局 ignore。
- 新增第二套 HTTP/runtime/state-management library 必须有 ADR，证明现有边界无法满足。

### 14.5 工作区产物与索引卫生

- 所有 Cargo target、bundle、性能结果、浏览器截图和 dev-launch 日志进入统一、gitignored 的 `output/<purpose>/`，不得在 `src/`、`src-tauri/src/` 或任意 watcher 源目录生成。
- `.gitignore`、Vite watch ignore 和 CodeGraph ignore 使用同一 artifact directory policy；新增 output purpose 时必须同步验证三者。
- CodeGraph、lint、test discovery 和源码统计默认排除 target/output/dist/node_modules。
- release artifact verifier 必须显式接收 artifact path，不能扫描整个工作区后猜测产物。
- CI 和本地脚本记录 source revision、dirty flag、toolchain、profile、target triple 和 artifact hash。
- 临时测试目录使用系统 temp 或统一 output 子目录，并在测试结束后回收；异常中断留下的目录必须可识别，不与真实源文件同名。

### 14.6 Tauri/WebView 安全门禁

- Stage 0 建立本地 threat model，覆盖恶意/失陷 provider、lookalike origin、redirect、renderer compromise、stale WebView assets、IPC abuse、日志/fixture/bundle secret 泄露和 update/exit race。
- production Tauri config 的 CSP 必须非空并由测试解析；默认禁止 remote script、`unsafe-eval` 和主窗口任意 remote navigation。dev/preview 配置必须物理分离，release provenance 证明未合并放宽项。
- capability manifest 使用 least privilege：main window、`capture-*` remote windows 和 preview 分离；capture window 只能调用 capture-specific sanitized command，不能访问 station/key/settings/proxy/update 等 main commands。
- remote capture capability 即使需要宽 URL shell，command/application 仍按 window label、station id、endpoint revision 和 exact origin 校验；lookalike/跨 station/stale window 请求 fail closed。
- external website 使用系统浏览器或隔离 capture window，不在 main WebView 导航；window creation/navigation/close 由单一 capture owner 管理。
- CI 比较 compiled command registry、ACL/capability manifests、window patterns 和 build config；授权未注册、注册未授权、过宽 capability、production `csp: null` 或 packaged demo entry 可达均失败。

## 15. 性能与容量合同

### 15.1 前端/IPC

- Stations 主列表刷新 command 数量不随 station count 增长。
- Key Pool 主列表刷新 command 数量不随 key count 增长。
- aggregate workspace 的 backend SQL/read-port round trip 数也不随 row count 增长；把 N+1 藏进一个 IPC command 不算通过。
- hidden/unmounted shell 页面主动 query 数为 0。
- 同一 resource 在一个刷新周期只允许一个 in-flight canonical query。
- 页面切换不通过重新挂载全部历史页面换取速度。
- aggregate workspace 必须有分页/上限和 payload-size 指标；O(1) command 数不能以无界一次性 payload 为代价。
- 列表 read model 默认不携带 raw snapshot、完整日志、secret-bearing metadata 或大 response body；详情按需查询。
- Stage 0 记录固定规模档位的 command count、payload bytes、query duration 和 page commit duration，release gate 比较同机同构建基线。
- aggregate 使用一致 read snapshot、稳定排序/cursor 和显式 partial availability；并发 mutation 下不得重复/漏行或混合不同 revision。
- 性能资格同时使用绝对 SLO 和相对回归阈值，记录 dataset hash、warm-up、sample、p50/p95、machine/profile provenance；单次最快值和跨机器绝对耗时不作结论。

### 15.2 后台任务

- 所有 task queue/channel 有固定容量和 overload 指标。
- 同一 periodic task 不重入。
- cancellation 后在预算内停止；预算按任务类型记录并由 soak 验证。
- shutdown 报告所有 timeout，不因某个任务卡住而无限等待。
- foreground operation 有独立并发上限；重复 connectivity/remote scan 不得绕过 daemon capacity 或无限进入 blocking pool。
- cancellation qualification 同时检查 UI terminal、network stop、retry stop 和无后续持久化副作用。

### 15.3 Collector

- provider endpoint fan-out 有上限。
- task budget 包括认证刷新、重试和 fallback，不允许每个子请求重新获得完整预算。
- outbound client 数量与 request 数量无关；按稳定 proxy route/policy 复用，不在每次 endpoint 调用创建 client。
- fixture 和 live qualification 分开；debug fixture success 不等于 release qualification。
- 性能比较采用同机、同数据集、同构建模式和同 provenance，不使用单次绝对耗时下结论。

## 16. 实施阶段

### Stage 0：冻结边界与基线

产物：

- 本 spec 评审冻结。
- 七个 ADR：IPC generator/error、application composition、Backend mode、Query/page visibility、work supervision、provider registry/async outbound、CI/artifact policy。
- 当前 command/DTO/error/mock/AppServices/task/operation/ureq/spawn/driver/output inventory。
- 行为和性能基线。
- parser-backed architecture gate 骨架。
- 本地 threat model、Tauri CSP/capability/window baseline 和 production/dev/preview build-config 差异清单。
- 成熟度决策记录：generator 维护/兼容性、Tokio lifecycle primitive、标准 ESLint/compiled gate 与 custom AST gate 的最小边界。
- 依赖生命周期台账：至少覆盖当前 React 18、Vite 6、Tauri 2、TanStack Query 5、Tokio 1、reqwest、Axum、SQLx、Rust toolchain/edition 和 Node/pnpm；以官方支持/安全来源记录 keep、独立升级或 release blocker 决策。

退出条件：

- 不再新增直接 feature `invoke`、`Result<T, String>` command 和业务 fallback。
- 新增跨模块功能必须遵守目标边界。
- Persistence V2 收尾文件与本升级任务完全隔离。
- queue capacity、并发上限、per-task shutdown timeout 和全局 shutdown budget 已冻结为可执行数值；未给出数值的“有界”或“及时停止”不能通过设计门禁。
- PR CI 已建立最小 fail-closed gate，后续 stage 不能只依赖 tag release 验证。
- production `csp: null`、main/capture capability 混用和 packaged demo 可达均已进入 security manifest；新增/扩张立即被门禁阻止，现有项若不能在 Stage 2/4 对应 owner 消除则阻塞后续 Stage 和 release，不能成为永久 baseline。无 owner/expiry 的 architecture 例外为零。

### Stage 1：Typed IPC 与统一错误

顺序：

1. 选定并锁定生成工具版本。
2. 建立 `CommandError` 和 frontend `BackendError`。
3. 先迁移只读、低风险 command。
4. 再迁移 mutation。
5. 最后迁移 streaming Channel adapter。

退出条件：

- 所有 production commands 返回 typed public error。
- 普通 command 的 TS 类型和调用绑定由 Rust 生成。
- 前端不再解析 command error message。
- serialization 和 command registration gate 通过。

回滚：保留旧 command 作为单向 adapter，不能让新 binding fallback 到旧业务实现；迁移 feature 可按 command group 回滚。

### Stage 2：窄 Command Facade、显式 BackendClient 与 mock 隔离

首先建立 domain command facades，并通过现有原子 composition 机制注册；然后按 settings/stations -> keys -> changes/logs -> collectors -> routing/proxy -> updater/data recovery 迁移 frontend feature。

每个 feature：

1. 接入领域 client。
2. 建立 desktop/demo contract test。
3. 删除 API 内 memory fallback。
4. 验证 desktop failure 和 demo unsupported。

退出条件：

- runtime mode 仅 bootstrap 持有。
- production command 不再注入 `State<AppServices>`；`AppServices` 不再是 runtime managed service locator。
- feature 不接收完整 BackendClient，只接收领域 client/query hook。
- production API 中无隐式 mock fallback。
- DemoBackend 不访问真实能力。
- IPC 故障显示 recovery/error state，不显示模拟成功。

### Stage 3：Query ownership 与 aggregate read models

顺序：Key Pool -> Stations -> Dashboard -> Logs/Changes -> Pricing/Routing -> Settings/Collectors。

每个页面：

1. 定义 canonical query keys 和 workspace read model。
2. 移除服务器数据 `useState` 副本。
3. mutation 改为 cache transition/invalidation。
4. 删除 DOM data event 和 activation loader。
5. 拆出 form/controller/view model。

退出条件：

- 所有服务器状态只有 Query Cache owner。
- Stations/Key Pool 不存在按行 IPC fan-out。
- hidden page query 为 0。
- Shell 默认只保留 current/previous/transient，保活例外有显式 allowlist 并使用唯一 `PageVisibility`。

### Stage 4：Work supervision、前台 Operation 与 Async Outbound

顺序：

1. 基于 `CancellationToken` + `TaskTracker`/bounded `JoinSet` 等成熟 Tokio primitives 建立纯 task state machine 和 status projection。
2. 建立独立 `OperationRegistry`、`BlockingExecutor` 和共享 `AsyncOutboundClient`，不自造 executor/HTTP stack。
3. 把 connectivity probe 从 command 移入可取消 operation，并迁移到 async outbound。
4. 迁移 station collector runner。
5. 迁移 channel monitor runner。
6. 接入 startup task、capture blocking boundary 和 updater coordination。
7. 通过 facade 协调既有 ProxyRuntime shutdown。
8. 建立 app shutdown report。
9. 将 tray/window/updater/OS exit 统一接入 Tauri `ExitRequested` 阶段的幂等 ExitCoordinator，删除 `RunEvent::Exit + block_on` 主要关机路径。

退出条件：

- 永久 runner 不再创建自有 OS thread。
- 所有任务具有 cancel/join/status/backoff。
- connectivity 等长操作具有真实 backend cancellation、operation id 和唯一 terminal。
- blocking work 有容量、排队超时和 orphan diagnostics。
- shutdown fault/timeout 测试通过。
- task failure 不只存在于 stderr。
- 所有退出来源只触发一次 bounded async drain；隐藏到托盘不触发 shutdown，强制 kill 不被伪装为 graceful success。

### Stage 5：ProviderRegistry 与 Capability Drivers

顺序：

1. 冻结 `ProviderEntry`、`CollectorDriver`、`RemoteKeyDriver`、`AuthorizationDriver` 和 typed failure。
2. 建立 conformance suite，并让 provider/client 只依赖 Stage 4 async outbound。
3. 迁移 OpenAI-compatible 作为最小 provider module。
4. 迁移 NewAPI collector/remote-key/authorization capabilities。
5. 迁移 Sub2API collector/remote-key capabilities。
6. 迁移 endpoint ping、channel probe、web authorization HTTP validation 和必要 updater direct HTTP。
7. 删除 collector/remote-key provider 字符串 dispatcher、旧兼容网络路径和 production `ureq` dependency。

退出条件：

- provider-specific 代码封闭在 capability drivers。
- collector/remote-key orchestrator 不解析 payload/message，也不按 provider 字符串分发。
- 每个 provider capability conformance suite 完整。
- 现有 collector facts、events、partial/error 语义无漂移。
- production provider/probe/management HTTP 不再使用 `ureq` 或 per-request client construction。

### Stage 6：Command 与页面物理拆分

只有前述 owner 和边界稳定后执行：

- 按领域拆 `commands/mod.rs`。
- 按 controller/form/dialog/list 拆巨型页面。
- 按 driver 内部分层拆 provider adapter。
- 删除旧 API、event、fallback、runner 和 source-contract tests。
- 清理分散 target/output 目录并落实统一 artifact/index policy。

退出条件：

- 模块依赖图符合 Section 6。
- 无 facade/god object 取代原巨型文件。
- 删除代码后行为与架构 gate 全绿。

### Stage 7：Release qualification

- 完整 frontend、Rust、Tauri smoke 和 architecture gates。
- Desktop real backend 与 DemoBackend 分别验收。
- 多站点 workspace refresh 验证 O(1) command 数。
- collector/monitor/foreground operation 混合负载、cancellation 和 shutdown soak。
- Sub2API/NewAPI live qualification 与 fixture provenance 分离。
- PR CI 与 release workflow 复用同一验证 entrypoint；advisory 例外均未过期。
- generated bindings、ACL、command registry、architecture graph 和 artifact provenance gate 全部 fail closed。
- release/locked 构建、最终 staged snapshot 和产物扫描。
- 签名 Windows bundle 的 fresh install、受支持版本升级、update drain/relaunch、offline startup、旧 WebView asset contract mismatch、single-instance、tray/exit 和 packaged-demo-unreachable matrix。
- production CSP、main/capture capabilities、remote window origin validation 和 secret canary scan 通过。

## 17. 验收标准

### 17.1 可靠性

- Desktop invoke 故障不会返回 mock 数据或模拟写入成功。
- command error 100% 使用 stable code，前端不解析 message。
- public error details 使用封闭 tagged enum，不存在任意 JSON/error-chain 泄露。
- query cache 是服务器状态唯一 owner，无 DOM 数据事件和本地长期副本。
- 所有后台任务可取消、可等待、有界，并进入可查询终态。
- 所有纳入 registry 的前台长操作具有真实后端取消、唯一 terminal 和不可逆副作用边界。
- blocking work 有界；达到容量、排队超时和 orphaned job 均可诊断。
- provider 未知响应和 partial 数据不会被合成为空成功。
- 应用退出和更新准备报告 task/proxy drain 的真实结果。

### 17.2 可维护性

- Rust contract 可确定性生成 TypeScript binding。
- command 只依赖窄领域 facade，`AppServices` 不作为 runtime service locator。
- feature 只依赖领域 client，不通过完整 BackendClient 查找任意后端能力。
- command、feature、driver 和 task module 依赖方向由 parser gate 保护。
- command adapter 不包含业务编排，driver 不访问 persistence/Tauri state。
- provider/probe 网络统一走 async outbound，production 不存在分散 `ureq` agent 和无界 `spawn_blocking`。
- 巨型页面已按状态 owner 和职责拆分，而非机械移动代码。
- 关键架构 gate 不再依赖源码 regex 猜测语义。
- CI 包含 lint、format、clippy、generated diff、unit/integration/build。

### 17.3 可拓展性

- 新增普通 command 不需要手写 command string 和重复 DTO。
- 新增 provider 不修改 collector/remote-key runner、supervisor 和前端 query 基础设施。
- 新增周期任务只实现 task body/policy 并注册 `TaskSpec`。
- 新增长操作只实现 operation body/policy 并注册 `OperationSpec`。
- 新增列表字段通过 aggregate read model 扩展，不增加 per-row invoke。
- 新增 failure 必须显式提供 retry、用户可见性和 redaction 语义。
- 新增 provider capability 通过 ProviderRegistry 注册，不扩大不相关 capability trait。

### 17.4 安全与成熟度

- production CSP 非空，main/capture/preview capability 分离，capture remote request 同时通过 window/station/revision/exact-origin 校验。
- packaged production entry 不能进入 DemoBackend；旧/错配 WebView assets 在业务 invoke 前 fail closed。
- secret 不进入 Debug/Display/IPC/progress/trace/metric/fixture/bundle；redirect、retry 和 auth refresh 不复制或跨 origin 转发凭据。
- TaskSupervisor/OperationRegistry/BlockingExecutor 建立在 Tokio/Tokio-util 成熟 primitives 上，不实现自定义 executor、线程池或工作流引擎。
- outbound 建立在 reqwest 上，Query ownership 建立在 TanStack Query 上，架构门禁优先复用类型系统、编译后 registry 和 ESLint 标准规则。
- generator 若维护状态、Tauri/serde/Channel 兼容性或确定性不通过 Stage 0 spike，不得进入主路径；runtime 不依赖 generator。
- release 使用的框架、runtime、toolchain 和关键 build dependency 均处于受支持状态，或有已批准且未过期的风险接受；unsupported/EOL 与不可接受高危 advisory 为硬阻塞。版本升级必须独立于架构 cutover 取得兼容性和回滚证据。

## 18. 回滚与提交策略

- 每个 stage 使用 additive foundation -> 单个消费者迁移 -> 旧路径删除三类提交。
- 每个提交只迁移一个 command group、feature 页面、runtime task 或 provider driver。
- 不使用 `git add .` / `git add -A`，按 stage 精确路径提交。
- 不在同一提交混合架构迁移、UI 改版、依赖大版本升级和业务功能扩展。
- cutover 前保留兼容 adapter；cutover 后立即删除旧 owner，禁止长期双轨。
- 回滚只回到上一个完整 owner，不允许恢复隐式 mock fallback、双写或无界 spawn。
- dirty Persistence V2 文件不得被本升级 stage 修改、格式化或纳入提交。
- composition cutover 必须原子：不得提交一部分 command 使用 domain facade、另一部分从同一个新 facade 暴露回完整 AppServices 的过渡捷径。
- async outbound cutover 按调用面迁移；同一 provider operation 不得在失败时从 reqwest fallback 到 ureq。

## 19. 风险与缓解

### 19.1 Binding generator 无法覆盖 Channel

缓解：普通 commands 全量生成；Channel 使用一个窄手写 adapter，并用 Rust/TS event fixture 锁定。不得因此放弃其余生成式契约。

### 19.2 Query 迁移引入短暂陈旧数据

缓解：逐页面建立 cache transition 测试；mutation 成功以后端返回事实更新；失败回滚 optimistic state；禁止同时保留旧 refresh owner。

### 19.3 页面卸载丢失 draft

缓解：draft 迁入显式 form controller；有未保存变更的 transient page 使用退出确认；只有证明必要时进入 retention allowlist。

### 19.4 Supervisor 关机死锁

缓解：任务只能持有 child token 和自身 handle；shutdown 顺序固定；逐任务和全局 deadline；fault injection 覆盖忽略取消、panic、join error 和超时。

### 19.5 Driver 迁移改变 provider 兼容行为

缓解：先建立 golden fixtures 和 live evidence；parser/mapping 纯函数化；一次迁移一个 task/provider；对 raw evidence 做脱敏 snapshot 对比。

### 19.6 架构工程无限扩张

缓解：本 spec 只在已经反复变化的真实边界建立抽象：IPC/runtime backend、query/page visibility、work lifecycle、provider capabilities/async outbound 和 application composition。页面与 command 只按 owner 拆分，不引入通用 DI container、事件总线、repository-for-everything 或动态 plugin ABI。

### 19.7 Supervisor 或 ProviderRegistry 变成新 God Object

缓解：Supervisor 只持有 work metadata/token/handle，ProviderRegistry 只持有 descriptor/capability objects；二者都禁止业务方法和任意 service lookup。业务依赖由窄 task body、operation body 和 capability driver 构造时注入。

### 19.8 Async outbound 迁移改变代理语义

缓解：先锁定 direct/system/manual/SOCKS、redirect、timeout、TLS、Retry-After 和 redaction fixtures；proxy request forwarding 不强制改用新 facade，只共享中立 route/policy value。每个 provider operation 禁止双 transport fallback。

### 19.9 PR CI 变慢或不稳定

缓解：PR gate 分 fast deterministic 与显式集成 target；Windows Cargo 串行并缓存；live provider、签名和长 soak 保留在 release/manual qualification，但不能替代 PR 的 compile/unit/architecture gate。

### 19.10 Work lifecycle 演变成自造 runtime

缓解：TaskSupervisor/OperationRegistry 只在 Tokio primitives 上增加状态和策略；禁止实现 executor、线程池、通用 mailbox、actor address 或 workflow DSL。Stage 0 用 spike 在 `TaskTracker` 与 bounded `JoinSet` 中选一个主要 join owner，另一套不能长期并存。

### 19.11 CSP/capability 收紧破坏授权窗口

缓解：main/capture/preview 分离配置和 smoke fixtures；capture capability 可以覆盖用户配置的远程站点，但 application 仍逐请求校验 window label、station、revision 和 exact origin。先以真实 NewAPI/Sub2API 授权流程验证，再禁止 production `csp: null`，不通过扩大 main capability 解决问题。

### 19.12 Binding/architecture 工具自身不成熟或失维护

缓解：generator 和 custom AST gate 都不是 runtime dependency；Stage 0 检查维护状态、Tauri/serde/Channel 覆盖、确定性、Windows CI 和 bypass fixtures。标准编译器/ESLint/compiled registry 能表达的规则不重复自造；工具失败时 fail closed 并允许替换 build-time adapter，不影响 domain contract。

### 19.13 长迁移期间核心依赖失去支持

缓解：Stage 0 建立 dependency lifecycle ledger，PR/release 检查复查日期、官方支持状态和安全公告；React/Vite/Rust edition 等大版本迁移不与 owner cutover 混在同一 shard，但 unsupported/EOL 或不可接受高危风险必须先完成独立 prerequisite upgrade。升级失败回滚该版本 shard，不回退到长期运行的旧/新架构双轨。

## 20. 明确禁止的反模式

- 在 `invoke().catch()` 中切换到 mock。
- 继续新增 `Result<T, String>` public command。
- Rust 和 TypeScript 手工复制同一 IPC DTO。
- public error 使用任意 JSON details 或前端解析 message 决策。
- command 注入完整 `AppServices` 或 feature 注入完整 BackendClient 作为 service locator。
- 页面将 query data 复制到 state 后长期作为权威。
- 使用 DOM event 同步业务数据缓存。
- 通过挂载隐藏页面进行 prefetch。
- 每个列表行发起独立 IPC 获取稳定聚合字段。
- 后台永久任务 fire-and-forget spawn 或创建自有 block_on 线程。
- 在 TaskSupervisor/OperationRegistry 内实现自定义 executor、线程池、通用 mailbox 或 workflow engine。
- 只用前端 run token 忽略长操作结果，却不取消后端工作。
- 网络 I/O 包进 `spawn_blocking` 继续扩展同步 `ureq` 路径。
- 用无限重试掩盖 configuration、auth、invariant 或 panic。
- provider parser 直接写数据库或触发 UI event。
- collector orchestrator 通过字符串匹配 provider error message。
- remote-key/authorization service 继续按 provider 字符串 match 分发 capability。
- 用新的 `AppManager`、`RuntimeContext` 或 `GodService` 汇总所有依赖。
- 用文件行数作为唯一架构 gate。
- 用正则解析 Rust/TypeScript 结构并宣称架构门禁可信。
- 只在 tag release 才运行完整 compile/lint/architecture gate。
- 把 target、日志、截图或性能结果生成到源码和 watcher 目录。
- production `csp: null`、main/capture window 共用宽 capability，或让 remote capture window 调用 main commands。
- 在 packaged production 中通过 env/query/localStorage 切换 DemoBackend，或 handshake 失败后进入 demo。
- 等到 `RunEvent::Exit` 后才用 `block_on` 启动主要 async shutdown，或让 tray/window/updater 各自实现一套退出逻辑。
- 以“本次不做大版本升级”为由忽略 unsupported/EOL、不可接受高危 advisory 或已经失效的 runtime/toolchain 支持矩阵；也禁止把框架大版本升级混进业务 owner cutover 后用同一组回归结果验收。
- raw key/cookie/token 出现在可 Debug/Clone DTO、trace/metric、operation progress、redirect history、fixture 或 bundle。
- 在本升级中顺带修改 Persistence V2、视觉设计或依赖大版本。

## 21. 设计自审

### 21.1 可靠性审查

本设计消除了最危险的静默成功路径：正式 backend 不再 fallback 到内存；错误有稳定 code 和封闭 details；后台任务与前台长操作有真实终态；provider partial/unknown 不再伪装为 success；页面只消费一个服务器状态 owner。网络、blocking 和 work lifecycle 都有取消、容量、timeout、幂等与诊断要求。

### 21.2 可维护性审查

本设计明确防止把复杂度集中到新总管对象：`AppServices` 不再作为 runtime locator，完整 BackendClient 不注入 feature，Supervisor 只拥有 work metadata，ProviderRegistry 只拥有 capability registration。生成式 IPC 消除跨语言重复，Query ownership 消除缓存同步，Supervisor/OperationRegistry 消除 runner/run-token 模板，capability drivers 消除 provider 兼容逻辑外溢。物理拆文件晚于状态和依赖边界冻结，避免只移动代码。

### 21.3 可拓展性审查

command、provider capability、periodic task、foreground operation 和 aggregate read model 都具有明确扩展点，同时保持编译期封闭。provider 不是万能 trait，而是按 collector/remote-key/authorization 能力扩展；新增能力不会迫使无关 provider 实现空方法。扩展不需要修改不相关主循环，也不依赖动态插件或通用事件总线。对 Relay Pool 这种本地模块化单体，这比微服务或运行时插件更符合当前规模。

### 21.4 迁移可行性审查

所有 stage 都能按 command group、feature、task/operation 或 provider capability 独立迁移和回滚。Persistence V2 被明确排除，proxy request lifecycle 保留既有 owner。最先实施契约、错误和窄 composition，再迁移数据所有权、work lifecycle 和 async outbound，最后迁移 provider capability 并拆巨型文件。该顺序避免在新 driver 中固化旧同步网络，也避免在新 command 文件中保留原业务堆积。

### 21.5 行业成熟度审查

| 设计选择 | 行业成熟性判断 | 本项目采用方式 | 不采用的过度/不成熟方向 |
|---|---|---|---|
| 模块化单体 + composition root | 成熟，适合单机桌面应用和当前团队/部署规模 | application facade、明确依赖方向、同进程事务/生命周期 | 微服务、sidecar、动态插件平台 |
| Rust typed IPC + generated TS | 成熟，但 generator 生态兼容性必须 spike | Rust contract 权威、build-time 生成、compiled registry/ACL/hash gate | runtime reflection、手写双份 DTO、盲目锁定未验证 generator |
| TanStack Query server-state owner | 成熟的 React server-state 实践 | canonical keys、stale/gc、mutation invalidation、aggregate read model | Redux/Zustand 再复制 server state、DOM event cache |
| Tokio structured lifecycle primitives | 成熟稳定 | CancellationToken、TaskTracker/JoinSet、Semaphore、bounded channel；Supervisor 只加 policy | 自定义 executor、actor/workflow framework、永久 OS thread runner |
| reqwest async outbound | Rust 生态成熟主流 | 共享 client/pool、typed budget/proxy/redirect/redaction | `ureq + spawn_blocking`、自写 HTTP/TLS stack |
| `tracing` structured diagnostics | Rust 生态成熟主流 | local bounded spans/metrics/redaction，无云遥测 | stringly logs 作为业务状态、无界高基数 telemetry |
| 静态 capability driver registry | 成熟的 Strategy + Interface Segregation | 编译期封闭 provider kind、按能力 trait、conformance fixtures | 万能 ProviderDriver、运行时插件 ABI、字符串 dispatcher |
| Parser/fitness gates | 成熟理念，但 custom parser 容易脆弱 | 类型/编译/ESLint 优先，AST graph 只补缺口并有 bypass fixtures | regex 架构测试、把 CodeGraph 当唯一 CI correctness owner |
| Tauri CSP/capability/window isolation | 桌面 WebView 应用的标准安全边界 | production CSP、least privilege、capture exact-origin 二次校验 | `csp: null`、remote window 继承 main capability、runtime demo switch |
| 依赖生命周期治理 | 成熟的软件供应链实践 | 支持窗口/advisory/MSRV 台账；架构与大版本升级分 shard，风险版本阻塞 release | 永久冻结旧 major、无证据追 latest、把 major upgrade 混入业务重构 |

结论：升级方向采用的是成熟模式的保守组合，而不是追逐新框架。先进性来自强类型、结构化并发、单一状态 owner、build-time contract 和可执行边界；稳定性来自复用 Tauri/Tokio/reqwest/TanStack Query/tracing 的既有能力，并拒绝把本地工具扩张成分布式平台。

### 21.6 本轮正式审阅记录

| 原设计点 | 审阅发现 | 调整 | 三原则结论 |
|---|---|---|---|
| 完整 `BackendClient` | 可能成为 frontend service locator | 只允许 bootstrap 组合，feature 依赖领域 client | 可维护性通过 |
| `AppServices` 未纳入 | command 依赖半径仍不可见 | 增加窄 command facade 和原子 composition | 可维护性、可拓展性通过 |
| `CommandError.details: Value` | 无约束 JSON 会形成新字符串/字段债并可能泄密 | 改为封闭 `PublicErrorDetails` | 可靠性、可维护性通过 |
| Page retention | “挂载即订阅”与后台保活不刷新矛盾 | 单一 `PageVisibility` owner，后台保留 cache 但不订阅 | 可靠性通过 |
| `TaskSupervisor` | 只覆盖 daemon，无法取消 connectivity/scan | 增加独立 `OperationRegistry`，共享 lifecycle primitives | 可靠性、可拓展性通过 |
| blocking work | 原设计只禁止永久线程，未限制 blocking pool | 增加 `BlockingExecutor` 容量、排队、取消和 orphan diagnostics | 可靠性通过 |
| `CollectorDriver` | 无法消除 remote-key/auth provider 字符串分发 | 改为 ProviderRegistry + capability-specific traits | 可拓展性、可维护性通过 |
| provider network | 新 driver 可能只是包住 `ureq + spawn_blocking` | async outbound 先于 driver，最终删除 production ureq | 可靠性、可维护性通过 |
| tag-only verification | 发布时才发现 compile/architecture 漂移 | 增加 PR CI 与共享 fail-closed entrypoint | 可靠性通过 |
| output/target 变体 | 污染 watcher、索引和 provenance | 统一 artifact policy 与 ignore gate | 可维护性通过 |
| Work lifecycle 实现方式 | 自造 supervisor 可能变成新 runtime | 明确基于 Tokio primitives，只实现 policy/state | 可靠性、可维护性通过 |
| Bootstrap/handshake 顺序 | mode owner 建立前 handshake 会破坏 browser preview | production/demo 使用独立 entry 和互斥启动状态机 | 可靠性通过 |
| Tauri `csp: null` / remote capture | IPC typed 仍挡不住 WebView/capability 越权 | 增加 CSP、least privilege、window/station/revision/origin gate | 可靠性通过 |
| 自定义 parser/generator | 工具自身可能绕过规则或失维护 | 标准编译/ESLint 优先，spike + bypass fixtures + build-time only | 可维护性通过 |
| 退出路径 | `RunEvent::Exit + block_on` 太晚且多入口可能重复 drain | 增加幂等 ExitCoordinator，在 ExitRequested 阶段 bounded async drain | 可靠性通过 |
| React 18/Vite 6/Rust 2021 edition 等当前版本基线 | 稳定不等于永久受支持，长迁移可能跨过支持窗口 | 增加 dependency lifecycle ledger；major upgrade 独立分片，unsupported/EOL 阻塞 release | 可靠性、可维护性通过 |

审阅后没有发现需要替换 Tauri/React/TanStack Query/Tokio/Axum/SQLx 的证据，也没有发现拆微服务或引入运行时插件能降低当前风险。最合理方向仍是模块化单体内的强类型边界、单一 owner、有界异步和 capability-based provider 扩展。

### 21.7 最终结论

经本轮修订，该升级方向符合可靠性、可维护性、可拓展性、安全性和行业成熟度要求，但前提是严格执行窄 facade、真实后端取消、Tokio structured lifecycle primitives、async outbound 先行、capability driver、Tauri CSP/capability isolation、dependency lifecycle governance 和 parser-backed/PR gates。Stage 0 冻结后，新功能必须遵守目标边界；已有代码按 stage 渐进迁移。验收标准不是“文件变小”、引入了更多抽象或“测试变绿”，而是权威状态唯一、边界失败显式、异步工作可终结、provider 扩展局部化、凭据/窗口边界可证明、依赖半径可见、关键依赖处于受支持状态、契约由类型系统和可信门禁保护。
