# 价格分组与监控状态联动实施计划

状态：Implemented，功能与验收证据已完成；全量 `cargo fmt --check` 仍受既有无关文件差异阻断
日期：2026-08-03
目标规范：`docs/specs/PRICING_MONITORING_INTEGRATION_SPEC.md`（Implemented v2）
参考计划：`docs/archive/plans/2026-07-29-status-monitoring-refactor.md`
实施目标：在价格 / 倍率页面增加后端拥有的分组监控摘要、状态列和组合筛选，不改写价格事实或监控 V2。

## 1. 执行规则

1. 每个 Task 开始前运行 `git status --short --branch`，记录当前任务会触及的 dirty hunk；只合并本任务 hunk，不覆盖用户或其他任务的改动。
2. 本计划不新增数据库表、不新增 migration、不写入 `group_status` 或任何第二套健康状态。监控原始事实和价格事实继续由现有模块拥有。
3. 采用 RED-GREEN-REFACTOR：先添加能表达行为的测试并确认因能力缺失失败，再实现最小完整路径，最后运行本 Task 的回归命令。
4. 不使用 `git add .` 或 `git add -A`。提交前只 stage 本任务列出的明确路径，并运行 `git diff --cached --check`。
5. fixture、日志、截图和测试报告不得包含 API key、Cookie、Authorization 值、完整响应正文、prompt 或可还原账号身份的 metadata。
6. 默认测试只使用本地 SQLite fixture 和 loopback 数据；不调用真实 provider。真实 provider 验证需要独立授权，不属于本计划默认验收。
7. 所有新 SQL 必须批量执行，禁止按 group、Key 或 Monitor 循环发 SQL。超过 SQLite 变量预算时使用 JSON 参数或受控批次（每批最多 100 个引用），不得减少返回项。
8. 所有前端跨域判断必须位于 query / projection / View Model；React 表格不得读取 `ChannelStatusRow` 或数据库字段。
9. 任一退出命令没有真实退出码 0，Task 保持未完成；超时、只看日志或只跑部分测试不能视为通过。

## 2. 结果边界

实现后固定的数据流：

```text
pricing workspace
  -> canonical group refs + groupRefsHash
  -> load_pricing_group_monitor_status
  -> PricingGroupMonitorSummary read projection
  -> pricing View Model merge/filter
  -> status column and deep link
```

必须保持以下不变量：

- 分组身份按 `groupBindingId`、其次唯一 `groupIdHash`、最后 `groupKeyHash` 解析；显示名称不能作为身份或默认合并键。
- 同组多 Key 按 `priority ASC -> created_at ASC -> id ASC` 选代表；同一 Key 多 Monitor 按 `monitor.created_at ASC -> monitor.id ASC` 选代表。不能按最近成功或最好结果择优。
- `running` 与 `latestOutcome` 独立；运行中不能覆盖 latest terminal result。
- `unresolved`、`no_key`、`unmonitored`、`untested`、`unavailable_data` 不得互相伪装。
- station-wide Monitor 只能使用已有 Target Result 对具体 Key 的展开语义，不得把无 `station_key_id` 的结果复制给所有 Key。
- 输入最多 500 个规范化引用；当前版本超限或重复输入必须明确拒绝，`omittedGroupCount` 固定为 `0`，不能静默截断。
- 摘要查询失败时价格仍可显示；“暂不可用”不参与“仅成功 / 仅失败”筛选。
- `groupRefsHash` 由规范化、去重、排序后的 canonical ref 用 UTF-8、换行连接后 SHA-256 小写十六进制计算；Rust 与 TypeScript 使用同一 contract fixture。

## 3. 文件地图

| 路径 | 变更后职责 |
|---|---|
| `tests/contracts/pricing-group-monitoring.v1.json` | Rust / TypeScript 共用的 canonical ref、hash 和边界输入 fixture |
| `src-tauri/src/models/pricing_group_monitoring.rs` | 纯类型、canonical ref、匹配类型、摘要状态和纯 reducer；不依赖 SQLx / Tauri |
| `src-tauri/src/models/mod.rs` | 导出新领域模块 |
| `src-tauri/src/application/queries/pricing_group_monitor_status.rs` | 一个 ReadSession 内的候选解析、代表选择、状态 reducer |
| `src-tauri/src/application/queries/mod.rs` | 注册摘要 Query |
| `src-tauri/src/persistence/stores/monitoring/group_status_repository.rs` | 批量读取 Key / Monitor / latest result / running existence，并映射 SQL 行 |
| `src-tauri/src/persistence/stores/monitoring/mod.rs` | 注册新 repository |
| `src-tauri/src/application/app_services.rs` | 构造摘要 Query |
| `src-tauri/src/application/command_facades/pricing.rs` | 暴露摘要读取方法 |
| `src-tauri/src/commands/pricing_workspace.rs` | 新 IPC command 和输入解析调用 |
| `src-tauri/src/ipc/dto/pricing_reads.rs` | 输入、输出、字段校验和 serialization fixture |
| `src-tauri/src/ipc/dto/pricing_reads.typescript.txt` | 摘要 DTO 的生成源类型 |
| `src-tauri/src/ipc/registry.rs` | command facade 注册、contract fixture 注册（仅按生成器要求修改） |
| `src-tauri/permissions/main-window.toml` | 新 command 的窗口权限 |
| `src-tauri/generated/command-registry.json` | 生成文件，禁止手改 |
| `src/lib/bridge/generated.ts` | 生成文件，禁止手改 |
| `src/lib/bridge/BackendClient.ts` | pricing domain client 类型和方法 |
| `src/lib/bridge/DesktopBackend.ts` | desktop binding 适配 |
| `src/lib/bridge/DemoBackend.ts` | demo 模式的明确 unsupported 实现 |
| `src/lib/types/pricingMonitoring.ts` | 前端摘要、输入和展示状态类型 |
| `src/lib/projections/pricingGroupRefs.ts` | canonical ref、hash、行身份转换；不包含 UI 状态判断 |
| `src/lib/projections/pricingGroupRefs.test.ts` | Rust / TypeScript hash contract 和 ref 规则测试 |
| `src/lib/queries/pricingQueries.ts` | 摘要 query client |
| `src/lib/queries/pricingQueries.test.ts` | query 输入、输出和失败行为测试 |
| `src/lib/query/queryKeys.ts` | 版本化、包含 `groupRefsHash` 的摘要 query key |
| `src/lib/query/resourceQueries.ts` | dependent pricing summary query options |
| `src/lib/query/pricingMonitoringInvalidation.test.ts` | 价格工作区与摘要 query 统一失效测试 |
| `src/features/pricing/pricingComparisonViewModel.ts` | 行合并、展示状态映射、AND 筛选和 metrics |
| `src/features/pricing/pricingComparisonViewModel.test.ts` | 纯 View Model 行为测试 |
| `src/features/pricing/PricingPage.tsx` | 状态列、状态筛选、加载/错误/空状态和深链入口 |
| `src/features/channels/ChannelMonitoringTab.tsx` | monitor mutation 后失效价格摘要 |
| `src/features/channels/ChannelStatusPage.tsx` | 状态页 mutation / 返回路径失效价格摘要 |
| `src-tauri/tests/pricing_group_monitor_status.rs` | repository/query/DTO 集成、批处理、EXPLAIN 和无泄漏测试 |
| `scripts/pricing-group-monitoring-contract.test.mjs` | 跨层 contract fixture、命令和禁止 N+1 的静态门禁 |

说明：当前集成测试使用测试内创建的内嵌 SQLite fixture，保证数据、断言和 query-count 统计在同一测试边界内，避免维护一套与 schema 脱节的静态数据库文件。若后续需要跨测试复用 fixture，再新增脱敏的 `src-tauri/tests/fixtures/pricing_group_monitor_status/`，并先补 schema/version 校验；这不属于本次交付的前置条件。

如果实现前发现某个文件已被用户改动，先保留其语义并把本计划的改动拆成最小 hunk；不得为了套用文件地图而重写整文件。

## 4. Task 0：冻结基线与跨语言契约 fixture

**依赖：** 无。

**Files：**

- Create: `tests/contracts/pricing-group-monitoring.v1.json`
- Create: `scripts/pricing-group-monitoring-contract.test.mjs`
- Read only: `docs/specs/PRICING_MONITORING_INTEGRATION_SPEC.md`
- Read only: `src/features/pricing/PricingPage.tsx`
- Read only: `src/features/pricing/pricingComparisonViewModel.ts`
- Read only: `src-tauri/src/persistence/stores/monitoring/status_read_repository.rs`

**RED：**

- fixture 写入 canonical refs、乱序输入、重复输入、超过 500、同名不同 binding、unresolved 和预期 SHA-256。
- 先建立基线记录和 fixture 读取测试；command/DTO/禁止旧 repository 依赖的断言留到 Task 3/4 的 RED，避免 Task 0 在没有业务实现时无法完成。
- 记录基线 `git status --short --branch`、当前 commit、前端构建和 Rust 检查结果；不修复无关红项。

**GREEN：**

- fixture 只使用 `station-1`、`key-1` 等非秘密稳定值，并明确 schemaVersion=1。
- contract 脚本能被 Node 直接执行，验证 fixture 没有 `apiKey`、`cookie`、`authorization`、`token` 等字段或疑似秘密。

**Run：**

```powershell
git status --short --branch
git log -1 --oneline
node scripts/pricing-group-monitoring-contract.test.mjs
pnpm.cmd test -- src/lib/queries/pricingQueries.test.ts
pnpm.cmd build
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate：** 基线命令和失败项有记录；contract fixture 的预期 hash 已冻结；无业务代码变更被 Task 0 偷渡。

## 5. Task 1：实现 canonical group refs 与 hash

**依赖：** Task 0。

**Files：**

- Create: `src-tauri/src/models/pricing_group_monitoring.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src/lib/projections/pricingGroupRefs.ts`
- Create: `src/lib/projections/pricingGroupRefs.test.ts`
- Modify: `scripts/pricing-group-monitoring-contract.test.mjs`

**RED：**

- Rust 测试覆盖 exact binding、唯一 group id、group key fallback、重复引用、排序和 UTF-8 hash。
- TypeScript 测试读取同一 `tests/contracts/pricing-group-monitoring.v1.json`，验证 hash 与 Rust fixture 期望值完全一致。
- 测试证明 group name 改变不改变 canonical identity，同名不同 binding 不会合并。

**GREEN：**

- 提供 `CanonicalGroupRef`、`MatchKind`、`ResolutionState` 和 bounded input 校验；所有 ref 先规范化、去重、按 UTF-8 bytes 排序。
- canonical key 只允许：`station:{stationId}:binding:{id}`、`station:{stationId}:group-id:{hash}`、`station:{stationId}:group-key:{hash}`；binding 存在时不再并入 id/key。
- Rust 使用 `sha2::Sha256`；TypeScript 使用 Web Crypto `crypto.subtle.digest`，不引入新的 hash npm 依赖。两端输出小写 64 字符 hex。
- 统一拒绝空 station、空 group key、超过 500、重复引用（错误码为 invalid-input），不在任何层静默截断。

**Run：**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib models::pricing_group_monitoring -- --nocapture
pnpm.cmd test -- src/lib/projections/pricingGroupRefs.test.ts
node scripts/pricing-group-monitoring-contract.test.mjs
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

**Exit gate：** Rust 和 TypeScript 对共用 fixture 得出相同 canonical keys/hash；纯模块不依赖 Tauri、SQLx、React 或数据库。

## 6. Task 2：冻结代表 Key、Monitor 和状态 reducer

**依赖：** Task 1。

**Files：**

- Modify: `src-tauri/src/models/pricing_group_monitoring.rs`
- Create: `src-tauri/tests/pricing_group_monitor_status.rs`
- Modify: `src/features/pricing/pricingComparisonViewModel.ts`
- Create: `src/features/pricing/pricingComparisonViewModel.test.ts`

**RED：**

- Rust table-driven tests 覆盖：多 Key 稳定排序、同 Key 多 Monitor 创建顺序、第一候选未检测而第二候选成功、running 与 latest 同时存在、停用 Monitor、station-wide 具体 Key 展开、unresolved/no_key/unmonitored/untested 区分。
- TypeScript tests 覆盖显式 `success/degraded/failure/skipped/running/untested/unavailable_data/unresolved` 筛选，所有 filter 维度使用 AND，状态不改变价格排序和最低倍率。

**GREEN：**

- 纯 reducer 只消费已读取的 Key、Monitor、latest Target Result、running existence；输出 `PricingGroupMonitorSummary` 所需字段，包括 `representativeKeyId`、`representativeMonitorId`、`latestTargetResultId`、failure kind、terminal reason、checkedAt、latency。
- 代表排序显式使用 `priority ASC -> created_at ASC -> id ASC -> monitor.created_at ASC -> monitor.id ASC`，禁止使用 SQL 返回顺序或最近成功替换策略。
- 展示状态固定为 `unresolved | no_key | unmonitored | running | untested | available | degraded | unavailable | skipped | unavailable_data`；`running` 只覆盖展示优先级，不改写 latest outcome。
- View Model 只接受摘要映射，不引用 `ChannelStatusRow`、recent points、bucket 或 execution history。

**Run：**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test pricing_group_monitor_status reducer_ -- --nocapture
pnpm.cmd test -- src/features/pricing/pricingComparisonViewModel.test.ts
pnpm.cmd test -- scripts/pricing-group-comparison-view-model.test.mjs
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate：** 每个领域边界均有失败前后可审计测试；同输入无论数据库返回顺序如何都得到同一代表和状态。

## 7. Task 3：新增后端批量摘要 repository 和 Application Query

**依赖：** Task 1、Task 2。

**Files：**

- Create: `src-tauri/src/persistence/stores/monitoring/group_status_repository.rs`
- Modify: `src-tauri/src/persistence/stores/monitoring/mod.rs`
- Create: `src-tauri/src/application/queries/pricing_group_monitor_status.rs`
- Modify: `src-tauri/src/application/queries/mod.rs`
- Modify: `src-tauri/src/application/app_services.rs`
- Modify: `src-tauri/src/application/command_facades/pricing.rs`
- Modify: `src-tauri/tests/pricing_group_monitor_status.rs`

**RED：**

- fixture 构造 exact/parent/group-id/unresolved、多 Key、多 Monitor、disabled Key、无凭据 Key、station-wide Monitor 和并列 `finished_at_ms`。
- 测试先要求一次 Query 返回完整 500 个引用，且查询统计没有按 group/Key/Monitor 循环 SQL；新 repository 不得调用 `workspace_recent_results`。
- 测试要求 latest result 使用 `finished_at_ms DESC, id DESC`，running 只返回存在性，运行中不能覆盖 terminal result。

**GREEN：**

- 每次 Application Query 开始一个 `ReadSession`，在同一 session 内批量读候选 Key、启用 Monitor Definition、latest Target Result 和 running existence，然后交给纯 reducer。
- Repository 使用 JSON 参数 + `json_each` 或每批最多 100 个引用的受控批处理，计算并验证 SQLite 变量预算；批次结果按 canonical ref 合并且保持 requested order 无关。
- station-wide Monitor 通过现有 Target Result 的 `station_key_id` 语义关联；无具体 Key 的结果不能复制给每个 Key。
- latest result 仅读取必要列，不读取 recent/hourly/daily buckets、完整 execution history 或 response body。
- 加入 repository 内部的 query-count/trace hook 或测试 spy，证明不存在 N+1；必要 SQL 为新路径，不改动旧 `workspace_recent_results`。
- 加入 `EXPLAIN QUERY PLAN` 测试并检查使用 station/key/monitor/result 相关索引；若出现全表扫描，先补现有索引设计或缩小查询，再继续。

**Run：**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test pricing_group_monitor_status repository_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_pricing_monitoring -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test pricing_group_monitor_status batch_500 -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate：** 500 引用完整返回且无 N+1；重复、超限、unresolved、station-wide 和 tie-break 行为均由数据库 fixture 证明；没有 migration 或写路径变更。

## 8. Task 4：冻结 DTO、IPC command 和生成绑定

**依赖：** Task 3。

**Files：**

- Modify: `src-tauri/src/ipc/dto/pricing_reads.rs`
- Modify: `src-tauri/src/ipc/dto/pricing_reads.typescript.txt`
- Modify: `src-tauri/src/commands/pricing_workspace.rs`
- Modify: `src-tauri/src/ipc/registry.rs`
- Modify: `src-tauri/permissions/main-window.toml`
- Modify via generator: `src/lib/bridge/generated.ts`
- Modify via generator: `src-tauri/generated/command-registry.json`
- Modify via generator: `src-tauri/src/ipc/dto/fixtures/pilot-serialization.json`
- Modify: `src-tauri/tests/pricing_group_monitor_status.rs`

**RED：**

- DTO tests 拒绝未知字段、空/非法 ref、重复 ref、超过 500、hash 不匹配；验证输出字段使用 camelCase 且 `omittedGroupCount=0`。
- IPC contract test 要求新 command 出现在 registry、main-window ACL、generated TypeScript wrapper 和 serialization fixture 中。
- serialization test 断言摘要输出不含 API key、Cookie、token、原始响应或完整 monitor/execution 对象。
- repository boundary test 要求新路径不引用 `workspace_recent_results`，且只返回摘要字段，不返回 recent/bucket/history。

**GREEN：**

- 新 command 固定为 `load_pricing_group_monitor_status`，输入含 `schemaVersion`、`groupRefsHash`、`groups`，输出含 `schemaVersion`、`generatedAtMs`、`groupRefsHash`、requested/returned/omitted counts 和摘要 items。
- DTO 只负责 shape、长度、重复、hash 和 public error 映射；不把领域选择逻辑塞入 DTO。
- 在 `PricingCommandFacade` 和 `AppServices` 中接入同一个 Query 实例；不创建绕过 Application 层的 command 专用 SQL。
- 先修改 registry 的 authoritative Rust source，再运行生成器，禁止手改 generated.ts、registry JSON 或 pilot fixture。

**Run：**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib ipc::dto::pricing_reads -- --nocapture
pnpm.cmd generate:bindings
pnpm.cmd generate:bindings --check
pnpm.cmd test:contracts
node scripts/architecture/check-command-registry.mjs
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate：** command 可被 registry、ACL、Rust DTO 和 generated TypeScript 同时发现；两次生成字节级一致；IPC 错误不会把 query failure 伪装成 monitor failure。

## 9. Task 5：接入前端 query、client 和 hash 生命周期

**依赖：** Task 1、Task 4。

**Files：**

- Create: `src/lib/types/pricingMonitoring.ts`
- Modify: `src/lib/bridge/BackendClient.ts`
- Modify: `src/lib/bridge/DesktopBackend.ts`
- Modify: `src/lib/bridge/DemoBackend.ts`
- Modify: `src/lib/queries/pricingQueries.ts`
- Modify: `src/lib/queries/pricingQueries.test.ts`
- Modify: `src/lib/query/queryKeys.ts`
- Modify: `src/lib/query/resourceQueries.ts`

**RED：**

- query test 验证先由价格 workspace 生成 canonical refs，再用同一 hash 请求摘要；引用变化时 query key 变化，旧摘要不能复用。
- 失败测试验证摘要请求失败不丢失价格 workspace，返回可区分的 error 状态而不是合成 `unavailable` outcome。
- query key 测试验证 schemaVersion + `groupRefsHash` + canonical refs 共同形成稳定 key，引用顺序变化不会造成重复缓存。

**GREEN：**

- `PricingDomainClient` 增加 `loadPricingGroupMonitorStatus(input)`；DesktopBackend 调用生成 binding，DemoBackend 返回明确 unsupported，不制造假状态。
- `pricingGroupMonitorStatusQueryOptions` 使用 `enabled` 绑定价格页可见性，`staleTime=5_000`、`refetchInterval=5_000`；空价格行不发摘要请求。
- query input 由 canonical refs 工具构造，先完成 async Web Crypto hash；hash 不匹配或超过 500 在调用前 fail closed。
- 错误对象保留 query error 与 generatedAt，不把 network/IPC failure 转换成 `unavailable`。

**Run：**

```powershell
pnpm.cmd test -- src/lib/queries/pricingQueries.test.ts src/lib/projections/pricingGroupRefs.test.ts
pnpm.cmd test -- src/lib/bridge/generated.test.ts
pnpm.cmd build
pnpm.cmd architecture:typescript
```

**Exit gate：** Desktop / Demo / test backend 均满足类型；价格工作区变化会丢弃旧摘要；摘要失败时价格事实仍能单独渲染。

## 10. Task 6：价格 View Model、状态列和组合筛选

**依赖：** Task 2、Task 5。

**Files：**

- Modify: `src/features/pricing/pricingComparisonViewModel.ts`
- Create: `src/features/pricing/pricingComparisonViewModel.test.ts`
- Modify: `src/features/pricing/PricingPage.tsx`

**RED：**

- View Model tests 覆盖 `with_key`、`with_credentialed_key`、`monitored`、`unmonitored`、`available`、`degraded`、`unavailable`、`skipped`、`running`、`missing/untested`、`unresolved`、`unavailable_data`。
- 组合筛选测试证明条件按 AND 工作，并且筛选后 metrics 和最低倍率只使用当前可见 rows。
- component test 或 DOM contract test 先要求“状态”表头位于“倍率”和“最后变更时间”之间，长名称、加载、错误、无匹配不重排布局。

**GREEN：**

- `PricingComparisonRow` 增加 `monitorSummary`、`monitorDisplayState` 和稳定 canonical ref；价格行没有摘要时只显示明确的 loading/unavailable-data 语义。
- 在现有价格筛选旁增加紧凑的 key presence、monitor presence、outcome 三个筛选维度；保留分组类型、站点和搜索，全部在 View Model 一次 O(rows) 过滤。
- 状态徽标显示短文案和 tooltip 解释代表 Key、Monitor、latest result、检测时间、匹配方式；不显示秘密或原始响应。
- 状态列不得参与价格排序、cheapest 标记或最低倍率计算；“正常”不能改变价格优先级。

**Run：**

```powershell
pnpm.cmd test -- src/features/pricing/pricingComparisonViewModel.test.ts
pnpm.cmd test -- scripts/pricing-group-comparison-view-model.test.mjs
pnpm.cmd build
pnpm.cmd lint
```

**Exit gate：** 价格页面在摘要成功、摘要失败、无 Key、无 Monitor、未检测和 running 下均有可解释状态；原有价格排序和分组展示回归通过。

## 11. Task 7：刷新失效、深链和错误/加载体验

**依赖：** Task 5、Task 6。

**Files：**

- Modify: `src/features/pricing/PricingPage.tsx`
- Modify: `src/features/channels/ChannelMonitoringTab.tsx`
- Modify: `src/features/channels/ChannelStatusPage.tsx`
- Modify: `src/lib/query/queryKeys.ts`
- Modify: `src/lib/query/resourceQueries.ts`
- Create or modify: `src/features/pricing/pricingMonitoringDeepLink.ts`
- Create: `src/features/pricing/pricingMonitoringDeepLink.test.ts`

**RED：**

- mutation test 覆盖 monitor create/update/delete、enable/disable、Run Now 完成/取消后摘要 query 被失效；只在成功或终态确认后失效，不在点击时伪造结果。
- deep-link test 验证只携带 `monitorId` / `stationKeyId` / stable group reference，不携带 API key、Cookie、token 或原始 URL。
- UI test 覆盖摘要 loading 不遮挡价格、摘要 error 显示“暂不可用”且不进入成功/失败筛选、深链失败不阻塞价格页。

**GREEN：**

- 抽取最小 query invalidation helper，统一失效 `queryKeys.pricing` 和所有带 `pricingGroupMonitorStatus` 前缀的 key，避免各页面复制字符串。
- 在监控 mutation 的成功/终态回调中调用 helper；价格页进入可见状态时依赖 React Query 按新 refs/hash 重新读取。
- 状态徽标跳转渠道状态页时使用既有导航/deep-link 约定；没有可定位 monitor 时只打开渠道状态页，不伪造 id。
- 为 loading、partial workspace、IPC error、empty rows、filtered empty 保持独立 UI 状态和可访问名称。

**Run：**

```powershell
pnpm.cmd test -- src/features/pricing/pricingMonitoringDeepLink.test.ts src/features/channels/channelStatusViewModel.test.ts
pnpm.cmd test -- src/features/channels src/lib/query
pnpm.cmd build
```

**Exit gate：** 监控变化在价格页可见时最多等待一个 query refresh 周期；深链可解释且无秘密；错误不会改变价格事实或筛选语义。

## 12. Task 8：跨层集成、性能、安全和发布回归

**依赖：** Task 0 至 Task 7 全部完成。

**Files：**

- Modify: `src-tauri/tests/pricing_group_monitor_status.rs`
- Modify: `scripts/pricing-group-monitoring-contract.test.mjs`
- Modify: `docs/README.md`、`docs/PROJECT_PLAN.md`、`docs/PRODUCT_MODEL.md`（仅在功能实际交付后更新状态和索引）

**验证项：**

- 用 fixture 验证 exact/parent/group-id/unresolved 解析、同名不同 binding、不确定 group 不计入有 Key/有监控、多 Key 代表、同 Key 多 Monitor、station-wide、disabled key/monitor、无凭据 key、latest tie-break、running overlay。
- 验证输入 0、1、100、500、501 个引用；501 明确 invalid-input，500 完整返回，任何情况下 `omittedGroupCount` 不静默增加。
- 对候选、latest、running SQL 运行 `EXPLAIN QUERY PLAN`；记录索引命中和 query count，证明无 N+1。不得用“代码里只写了一条循环”代替 SQL 证据。
- 扫描 fixture、生成绑定、日志和截图，确认没有 API key、Cookie、Authorization、token、完整 response body；确认没有新增 migration 或 `group_status` 持久化字段。
- 验证价格页与渠道状态页对相同 latest Target Result 的 outcome、failure kind、terminal reason、checkedAt 一致；不读取 `channel_monitor_runs` 作为新状态来源。

**Run：**

```powershell
git status --short --branch
git diff --check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
pnpm.cmd architecture:fixtures
pnpm.cmd architecture:typescript
pnpm.cmd architecture:commands
pnpm.cmd architecture:security
pnpm.cmd test:contracts
pnpm.cmd test -- src/features/pricing src/lib/queries/pricingQueries.test.ts src/lib/projections/pricingGroupRefs.test.ts
pnpm.cmd lint
pnpm.cmd build
cargo test --manifest-path src-tauri/Cargo.toml --test pricing_group_monitor_status -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_pricing_monitoring -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

**Exit gate：** 前端、Rust、IPC、架构、安全、批处理和 explain 证据全部通过；失败项有明确归因，不能用跳过测试替代。

## 13. 最终检查点与交付

**完成定义：**

- [x] 价格页存在“状态”表头，状态来源是 `PricingGroupMonitorSummary`，不是页面拼装完整监控工作区。
- [x] 多 Key / 多 Monitor 代表规则、station-wide 语义和 latest/running 分离均有 Rust + TypeScript 测试。
- [x] `groupRefsHash` contract fixture 在 Rust 与 TypeScript 中一致；输入超限不静默截断。
- [x] 一次 ReadSession + 批量 SQL，无 N+1；`EXPLAIN QUERY PLAN` 结果已由集成测试输出验证。
- [x] 价格状态失败时价格仍显示；`unavailable_data` 不混入成功/失败筛选。
- [x] 监控 mutation 会失效摘要；深链失败不影响价格页；无秘密泄露。
- [x] 未新增 migration、持久化 group status、旧监控状态机或价格事实字段。
- [x] 仅计划范围文件发生改动；已有用户 dirty changes 原样保留。

**提交前：**

```powershell
git status --short --branch
git diff -- docs/specs/PRICING_MONITORING_INTEGRATION_SPEC.md docs/archive/plans/2026-08-03-pricing-monitoring-integration.md
git diff --check
```

如需提交，只能显式 stage 本任务涉及路径，例如：

```powershell
git add docs/archive/plans/2026-08-03-pricing-monitoring-integration.md
git diff --cached --check
git diff --cached --stat
```

## 14. 回滚与后续扩展

回滚只移除新 IPC command、前端 query/client、状态列和失效调用；不需要数据迁移回滚，也不触碰已有价格事实、监控 Target Result、Execution 或健康数据。若后端摘要不可用，价格页应保留不带状态的旧渲染路径，直到新路径连续验证通过后再移除临时消费。

后续若要加入状态过期、可用率、服务端筛选、联合排序或可配置代表策略，必须扩展新的只读投影/版本化 DTO；不得把完整 monitor、attempt、bucket history 再下沉到价格页，也不得把这些扩展写成 UI 专属特殊逻辑。
