# 路由重试、故障转移与保护诊断收口计划

状态：待执行的审计收口计划

日期：2026-08-21

目标规格：[`../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md`](../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md)

前置实施记录：[`2026-08-20-intelligent-routing-retry-failover-configuration.md`](2026-08-20-intelligent-routing-retry-failover-configuration.md)

## 1. 目的与边界

这份计划不是重写智能路由，也不是新增一套熔断器。它只收口当前实现审计中会让用户误解路由行为的缺口：最近请求的模型没有贯通保护查询、保护 profile 的字段错误不够精确、诊断面板混淆查询状态、候选可调度事实在前端被重复推断，以及请求决策 ID 的命名/深链语义需要核对。

完成后，设置页和状态页必须满足：

- 保护状态查询使用最近请求的同一个 `model`；没有模型时明确显示 `model_required`，不能猜测模型作用域。
- 后端验证错误保留 `protectionProfile.<field>`，前端只标红真实错误字段。
- 状态页能区分“保护查询加载中”“保护查询失败”“确实没有保护/故障域诊断”。
- “可用候选”只使用后端 `schedulable` 事实，不由 React 根据局部健康码重算。
- 最近决策的 `route_decision.id`、`request_id`、`request_logs.id` 和 UI 的 `requestLogId` 语义经过明确校验，查看请求不会打开错误记录。
- 诊断组件不保留未启用的模拟路由死代码；清理不改变行为。

本计划不包含：发布资格、安装包、真实 Provider smoke、性能压测、云同步、`verify:full`、`verify:release`、自动 synthetic probe、其他 health scope 的真实 Half-Open probe，以及完整无限历史时间线。前置计划已经证明的能力不重复重构。

## 2. 不可变工程约束

1. 保留现有 `FailureClassifier`、`ReplayGate`、`RetryActionPlanner`、`HealthProtectionReducer`、policy CAS 和 generated IPC 作为唯一 owner；本计划不得增加旁路推断器。
2. 模型是查询作用域事实，不是前端筛选偏好。它必须从 recent decision read model 进入 query key、API input、IPC DTO 和后端 projector。
3. `schedulable` 是后端 admission 的事实；`healthState`、hard rejection 和 runtime overlay 只用于展示或解释，不能在组件内重新组合成资格规则。
4. 所有修复先补最小 RED 回归，再改 owner；测试只覆盖受影响模块和直接契约，不把本计划升级为发布门槛。
5. 保留工作区已有改动，不执行 stage、commit、push、分支操作，不删除用户生成的 target 或日志产物。

## 3. 执行顺序

```text
Task 0  基线与 ID 语义确认
   -> Task 1  最近决策 model read model + 页面 query 透传
   -> Task 2  protectionProfile 字段级错误保真
   -> Task 3 诊断面板加载/错误/空状态
   -> Task 4 统一使用后端 schedulable
   -> Task 5 诊断死代码清理与用户文案收口
   -> Task 6 组合回归、文档状态和剩余风险记录
```

Task 1、2 会触及 Rust read model/编译器；Task 3、4、5 是前端层；Task 6 只在代码变更后运行组合验证。中途不得通过改测试 fixture 或改变文案掩盖真实数据链路缺失。

## 4. Task 0：建立基线并确认 ID 语义

### 目标

记录现状，先证明两个问题确实存在：保护 query 未使用模型，决策摘要的 ID 是否可安全作为 `requestLogId`。如果事实证明两个 ID 在所有生产写入中恰好相同，也必须用测试固定这一不变量；不能只依赖字段名称。

### 核对范围

- `src-tauri/src/persistence/migrations/0005_request_logs.sql`
- `src-tauri/src/persistence/migrations/0019_routing_decisions.sql`
- `src-tauri/src/persistence/stores/routing_decisions/{write,queries}.rs`
- `src-tauri/src/application/queries/request_decision_trace.rs`
- `src-tauri/src/application/routing.rs`
- `src-tauri/src/application/command_facades/routing.rs`
- `src/features/routing/RoutingPage.tsx`
- `src/features/routing/RoutingStatusDiagnosticsPanel.tsx`

### 步骤

1. 用一条 fixture 写入不同的 `route_decisions.id`、`route_decisions.request_id` 和 `request_logs.id`，确认 list、trace、最近决策按钮和日志深链各自要求的 ID。
2. 若需要跨表关联，优先在 `RoutingDecisionSummaryRow` 增加明确的 `request_log_id` 字段，通过 `route_decisions.request_id = request_logs.request_id` 左连接取得；不得把 decision ID 重命名成 request log ID。
3. 为决策详情命令写入“输入是 decision ID 还是 request log ID”的注释和单测；若保持兼容别名，必须在 facade 内完成转换，不能让 UI 猜测。

### RED/GREEN 证据

- RED：不同 ID fixture 下，当前按钮/trace 返回错误记录或无法按模型作用域读取。
- GREEN：fixture 断言 list、trace、request log deep link 的 ID 均指向同一请求，且不存在跨请求串联。

## 5. Task 1：贯通最近请求模型到保护查询

### 目标

让生产页真正使用 `latestDecision?.model` 构造保护状态 query；后端最近决策摘要也必须保留模型。模型为空时保持显式 `model_required` 行为，不用默认模型或上一次缓存值。

### 代码变更

- `src-tauri/src/persistence/stores/routing_decisions/queries.rs`
  - 在 summary row 增加 `model: Option<String>`。
  - `get_decision` 和分页查询以稳定的 request 关联读取 `request_logs.model`；没有匹配行时返回 `None`。
  - 保持 cursor、排序、分页和旧数据兼容，不做 N+1 查询。
- `src-tauri/src/application/queries/request_decision_trace.rs`
  - `summary_from_decision` 复制 row model；对外 DTO 的字段名称保持 `model`。
  - 补充 decision/read-model 测试，避免 runtime trace 或 durable summary 再次丢模型。
- `src/features/routing/RoutingPage.tsx`
  - 在得到 `latestDecision` 后，用 `{ model: latestDecision?.model ?? null }` 调用 `routingProtectionStatusQueryOptions`。
  - query 的 `enabled` 仍由 status tab 控制；模型变化必须产生新的 query key 和请求。
- `src/features/routing/RoutingPage.test.tsx` 或最小可测试的 query composition fixture
  - 验证模型 A/B 使用不同 query key/API input。
  - 验证 `null` 不会沿用上一次模型。
- `src/lib/queries/routingQueries.test.ts`、相关 Rust IPC read tests
  - 保留现有 query-key 隔离，并增加生产调用链证据。

### 完成条件

- 最近决策携带模型时，保护查询输入和 failure-domain join 均为该模型。
- 最近决策没有模型时，页面显示等待模型/`model_required`，不猜测 commitment。
- 不同模型不会共享 React Query 缓存；后端分页查询不会改变既有排序和限额。

## 6. Task 2：保留 protectionProfile 字段级验证错误

### 目标

后端 profile 编译失败必须返回真实字段路径，例如 `protectionProfile.windowMs`，而不是统一变成 `protectionProfile`，否则前端会同时标红所有 profile 控件。

### 代码变更

- `src-tauri/src/application/health_protection.rs`
  - 确认 `HealthProtectionProfileV1::from_policy_config`/`validate` 返回的错误包含字段、code 和 message key。
  - 如领域错误字段是 profile 内部路径，提供一次集中转换为 public path 的 helper，禁止在 compiler 各处手写字符串。
- `src-tauri/src/application/routing_policy.rs`
  - 删除 `map_err(|_| InvalidField { field: "protectionProfile", ... })` 这类信息丢失映射。
  - 只把未知/无法归类的内部错误映射为聚合错误；已知字段错误必须原样保留并加上 `protectionProfile.` 前缀。
- `src/features/routing/LocalRoutingSettingsEditor.tsx`
  - 继续支持嵌套字段优先、聚合错误兜底；不要用聚合错误覆盖已有具体错误。
  - 每个输入保持稳定 `aria-invalid`、`aria-describedby` 和错误节点。
- `src-tauri/src/application/routing_policy.rs` tests、`src/features/routing/LocalRoutingSettingsEditor.test.tsx`
  - 覆盖 `windowMaxSamples`、`windowMs`、`minSamples`、`failureThresholdPercent`、`halfOpenSuccessesToClose` 各一条真实 compiler error。
  - 增加真实后端聚合错误 fixture，确认 UI 只显示通用 profile 提示而不错误标红五个输入。

### 完成条件

- 同一份非法 policy 在 Rust error DTO、前端 field map 和用户文案中保持同一字段路径。
- 前端永远不静默 clamp 或把后端错误改写为成功保存。
- 未知错误仍可显示安全的聚合提示，不暴露内部栈、凭据或原始 JSON。

## 7. Task 3：区分保护查询的 loading、error、empty

### 目标

状态页同时依赖 workspace snapshot、recent decisions 和 protection status。面板必须让用户知道是“正在读”“读取失败”还是“没有条目”，不能用“暂无故障域诊断”掩盖查询失败。

### 代码变更

- `src/features/routing/RoutingPage.tsx`
  - 分别传入 `protectionLoading`、`protectionError`、可选 `protectionUnavailable`，不要复用 workspace `loading`。
  - 使用 `readError` 转换安全错误文案；查询失败时保留已有 snapshot 和候选列表。
- `src/features/routing/RoutingStatusDiagnosticsPanel.tsx`
  - snapshot 未加载时显示页面级 loading/empty。
  - snapshot 已有但 protection query pending 时，在 Provider/故障域区域显示“正在读取保护诊断”。
  - protection query error 时显示可重试/刷新提示和错误文案；不渲染空状态冒充成功。
  - protection 返回 `unavailable` 或 `model_required` 时，显示对应解释；空 `failureDomains` 只表示确实没有可展示域。
- `src/features/routing/RoutingStatusDiagnosticsPanel.test.tsx`
  - 增加 snapshot ready + protection pending/error/unavailable/empty 四种状态。
  - 验证错误状态不把候选计数清零、不丢最近决策。

### 完成条件

- 三种状态都有稳定可读文案和可测试 DOM 标记。
- 保护查询失败不会触发对整个路由 workspace 的误报 toast 或空白页。
- 恢复查询后面板自动从 error/pending 回到 resolved/empty。

## 8. Task 4：以 schedulable 作为候选可用事实

### 目标

后端已经为每个候选计算 `schedulable`。前端不再根据 health state、硬拒绝码和 overlay 复制一份容易漂移的 admission 规则。

### 代码变更

- `src/features/routing/RoutingStatusDiagnosticsPanel.tsx`
  - `availableCount` 直接统计当前候选的 `schedulable === true`。
  - `CandidateLine` 的 blocked/可参与文案以 `schedulable` 为准；health state 只显示健康/保护状态。
  - overlay 只用于实时并发、冷却和状态展示，不覆盖后端调度资格。
- `src/features/routing/RoutingStatusDiagnosticsPanel.test.tsx`
  - fixture 覆盖 `schedulable=false` 但无 hard rejection、`schedulable=true` 但处于展示性 degraded、以及 overlay 与 snapshot 不同的情况。
  - 断言统计数字和候选文案与后端字段一致。

### 完成条件

- UI 的“可用候选/可调度”与 workspace read model 一致。
- 没有在前端新增 hard rejection、health 或 capacity 推断规则。

## 9. Task 5：清理诊断死代码并收口文案

### 目标

降低维护成本，避免后续开发误以为模拟路由仍是当前功能 owner。

### 步骤

1. 从 `RoutingStatusDiagnosticsPanel.tsx` 删除注释掉的模拟路由区块、`RouteCandidateExplanation`、`RouteEndpointKind`、`RouteSimulationResult`、`SimulationSummary` 等无消费者残留；仅在 `rg` 证明无生产消费者后删除。
2. 将最近决策的内部 `status`、`routeReason` 转成稳定用户文案；保留原始 code 作为受限技术详情，不在状态卡片直接展示 `trace_incomplete` 等内部值。
3. 补充文案映射测试，未知 code 使用安全 fallback，不把原始上游 message、URL 或 secret 渲染到页面。

## 10. Task 6：组合回归与记录

### 开发验证

只运行与实际改动相称的命令，不要求发布或安装包：

```powershell
# 文档或计划单独变更
git diff --check

# Rust read model / compiler 变更
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_policy -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_protection -- --nocapture

# 前端 query / diagnostics / settings 变更
pnpm.cmd test -- src/lib/queries/routingQueries.test.ts src/features/routing/RoutingStatusDiagnosticsPanel.test.tsx src/features/routing/LocalRoutingSettingsEditor.test.tsx
pnpm.cmd build

# 只有 DTO/registry/权限/生成绑定发生变化时
pnpm.cmd test:contracts
pnpm.cmd generate:bindings --check
```

必要时补一个 `routing_loopback_e2e` 或 persistence focused test；不要求每次执行全仓库 Vitest。若 Windows 桌面进程锁定默认 Cargo target，使用独立 `CARGO_TARGET_DIR` 重跑，不结束用户进程。

### 记录要求

每个 Task 记录：变更文件/owner、RED 证据、GREEN 命令与结果、未运行检查、兼容残留和剩余风险。测试输出中的既有 warning、React `act` stderr 或 Vite chunk warning 需区分于真正的退出码失败。

## 11. 完成定义

- Task 0 的 ID 语义有明确结论和回归证据。
- Task 1-4 的四条用户可见链路均有至少一个直接测试；没有只改文案或只测 query helper 的假闭合。
- 页面在窄窗口、loading、error、empty、unavailable 下不发生字段重叠或状态误导。
- 变更不引入第二套 retry/protection/admission owner，不扩大当前 Credential-only probe 边界。
- 文档 README 的计划链接可打开，计划状态与代码/测试事实一致。

剩余风险单独保留：Account/Group/Endpoint/Model 的真实 probe resolver、自动 synthetic probe、完整历史步骤时间线和 raw IPC 原文重复键检测仍不属于本计划完成范围。

## 12. 本轮基线验证记录

本轮只读审计和计划编写没有修改生产代码；以下结果用于区分当前基线与后续 Task 的 GREEN 证据：

- 通过：`pnpm.cmd test src/lib/queries/routingQueries.test.ts src/features/routing/RoutingStatusDiagnosticsPanel.test.tsx src/features/routing/LocalRoutingSettingsEditor.test.tsx`（3 files / 23 tests）。
- 通过：`pnpm.cmd test`（116 files / 450 tests）。第一次带有多余 `--` 的命令实际也以 0 退出，但后续已用不带 `--` 的明确路径重跑并确认聚焦结果。
- 通过：`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_policy -- --nocapture`（19/19）。
- 通过：`cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_protection -- --nocapture`（13/13）。
- 通过：`cargo check --locked --manifest-path src-tauri/Cargo.toml`。
- 通过：`pnpm.cmd test:contracts`。
- 通过：`pnpm.cmd generate:bindings --check`（4 artifacts，两次生成确定性）。
- 通过：`git diff --check`。

当前基线仍有两个独立问题，不能在本计划中伪装成通过：

- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` 失败于已有的 `src-tauri/src/services/portable_migration/catalog.rs` 格式差异；不是本计划文件引入。
- `pnpm.cmd build` 的 TypeScript 阶段失败于若干既有测试 fixture 缺少生成 DTO 已要求的 `maxRateMultiplier`、`routingGroupFilter`（见 `LocalRoutingSettingsEditor.test.tsx`、`useRoutingPolicyDraft.test.ts`、`src/lib/queries/routingQueries.test.ts` 等）。这应作为后续 fixture 技术债修复，不在文档任务中扩大修改范围。

Rust 输出包含仓库已有的 unused/dead-code/unfulfilled-lint warnings；它们没有改变上述命令的退出码。未运行发布、安装包、真实 Provider smoke、性能压测、`verify:full` 或 `verify:release`。
