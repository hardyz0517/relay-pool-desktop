# NewAPI 采集边界收口与 OpenAI-compatible Provider 移除实施计划

状态：待实施

日期：2026-08-01

适用范围：Relay Pool Desktop 当前 generation-2 数据层、Provider Registry、站点采集、采集调度、变更中心和信息采集 UI。

## 1. 背景与问题

当前 NewAPI 完整采集包含 `balance`、`groups` 和 `models`。这与 Sub2API 的站点采集边界不一致，也把账号可见的模型目录错误解释成站点或 Station Key 的真实路由能力。

现有实现同时存在以下问题：

1. NewAPI `balance` 会同步回溯全量历史 usage，包含跨月 dashboard 请求和日志分页，单次任务理论请求量可达到数百次。
2. NewAPI `full` 串行执行三个子任务，默认 UI 又选择 `full`，导致采集容易耗尽硬编码预算。
3. NewAPI `/api/user/models` 的结果会写入 `collector_model_facts`，并为每个模型生成 `model_added` / `model_removed` 事件。
4. 首次模型采集没有 baseline 语义，当前全部模型会被当成新增。
5. `partial`、`failed` 或空模型结果仍可能整体替换旧集合，形成“全量下架 -> 下次全量新增”的抖动。
6. `full` 父任务和子任务都会进入事实写入路径，集合型事实存在重复写入和错误清空风险。
7. `collectorTimeoutSeconds` 没有控制 NewAPI 的实际子任务预算。
8. OpenAI-compatible / `custom` 站点目前被注册成一个可采集 Provider，但项目没有为任意自研中转站定义可靠、统一的管理端采集规则。

本次升级同时移除 OpenAI-compatible Provider 类型，不再把任意 OpenAI-compatible API endpoint 当成可采集站点类型。

## 2. 决策边界

### 2.1 本次删除的内容

- `ProviderKind::OpenAiCompatible` 采集 Provider。
- `services/collectors/drivers/openai_compatible/` collector driver。
- OpenAI-compatible Provider Registry entry。
- `PreparedOpenAiCompatibleCollection` 及其 prepare / finish / dispatch 分支。
- 可创建 Station 类型中的 `openai-compatible`、`openai_compatible` 和 `custom`。
- 以 `custom` 为 station type 的官方模型厂商 presets。
- OpenAI-compatible 站点的模型采集、调度、事实写入和变更事件。

`custom` 当前只是 OpenAI-compatible Provider 的 UI/兼容别名，因此必须与该 Provider 一并退出创建和采集路径，不能保留一个仍映射到已删除 Provider 的空壳类型。

### 2.2 本次明确保留的内容

- Relay Pool Desktop 对外提供的本地 OpenAI-compatible 网关。
- 本地 `/v1/models`、`/v1/chat/completions`、`/v1/responses`、`/v1/embeddings` 等代理端点。
- Station 的 `api_base_url` 概念。
- Station Key 的 OpenAI Chat / Responses / Embeddings 上游协议能力。
- `UpstreamApiFormat::CustomOpenAiCompatible` 等协议格式选择。
- Station Key 连通性探测中的 `/v1/models` 请求。
- 状态监控中的 Generic OpenAI-compatible protocol adapter。
- 本地 OpenAI-compatible error envelope、请求转换、流式解析和 qualification 脚本。

删除的是“可采集 Provider 类型”，不是 OpenAI-compatible 网络协议能力。实施中禁止按字符串全仓替换或删除所有 `OpenAI-compatible` 引用。

## 3. 目标状态

### 3.1 Provider 能力矩阵

| Provider | 可执行单项任务 | `full` 组成 | 站点模型采集 | 模型变更事件 |
|---|---|---|---|---|
| Sub2API | `detect`, `balance`, `groups` | `balance`, `groups` | 不支持 | 不产生 |
| NewAPI | `detect`, `balance`, `groups` | `balance`, `groups` | 不支持 | 不产生 |

升级完成后，Collector Provider Registry 只能注册 Sub2API 和 NewAPI。

### 3.2 模型能力所有权

模型能力不再属于 Station collector：

- Station collector 只采集站点账号资产事实，例如余额、分组和倍率。
- Station Key capability 保存显式 `model_allowlist`、`model_blocklist` 和 `preferred_models`。
- Station Key connectivity probe 可以通过 API namespace 的 `/v1/models` 和真实协议探针获得本次探测候选。
- 路由只消费 Station Key capability、协议能力和运行时健康事实，不消费 NewAPI 管理端模型目录。

### 3.3 NewAPI Balance 合同

NewAPI `balance` 必须是固定成本任务：

1. 请求 `/api/status` 获取额度换算信息。
2. 请求 `/api/user/self` 获取余额、已用额度、请求数等账号聚合事实。
3. 如果当前 UI 确实需要今日统计，可额外请求一次仅覆盖当天的 `/api/data/self`。

禁止行为：

- 从 Unix timestamp `0` 回放。
- 以 30 天窗口向历史回溯。
- 请求或分页 `/api/log/self`。
- 为计算 token 总数扫描历史请求日志。
- 因可选 usage 字段失败而阻塞核心余额结果。

无法通过 O(1) 聚合接口可靠获得的字段必须返回 `unknown` / `None`，不能通过高成本回放补齐。

## 4. 架构升级方案

### 4.1 Provider capability 成为唯一任务来源

扩展 `CollectorCapabilityDescriptor`，明确区分 driver 可执行任务与 orchestration `full` 组成：

```rust
pub struct CollectorCapabilityDescriptor {
    pub supported_tasks: &'static [CollectorTaskKind],
    pub full_tasks: &'static [CollectorTaskKind],
}
```

约束：

- 从 driver 级 `CollectorTaskKind` 删除 `Full`。
- 外部 `CollectorTask::Full` 保留，由 orchestration 展开为 `full_tasks`。
- `full_child_tasks` 不再维护第二份 `match ProviderKind` 能力表。
- command、scheduler、provider draft preview 和 UI 都必须消费同一份 capability resolver。
- 后端必须在网络调用前拒绝不支持的 station/task 组合，返回稳定的 `unsupported_task`。

这样新增 Provider 时必须显式声明任务集合，不会因为遗漏某个 UI 或 scheduler 分支而获得错误能力。

### 4.2 Full 父任务不再写业务事实

`full` 父任务只负责：

- parent collector run；
- overall status；
- child run 引用；
- endpoint count 汇总；
- duration 和诊断摘要；
- parent snapshot。

`full` 父任务的 canonical facts 必须为空。余额、分组、倍率等事实只能由对应成功子任务写入。

该规则消除父子重复写入，也避免某个失败子任务让父任务携带空集合覆盖旧事实。

### 4.3 集合型事实引入完整性语义

共享 collector 输出需要给集合型事实增加显式完整性：

```rust
pub enum FactSetCompleteness {
    Complete,
    Partial,
    Unavailable,
}
```

写入规则：

- 只有 `status == success` 且集合为 `Complete` 时允许 replacement。
- `Partial` 只能 upsert 已观察到的事实，不能推断未出现项已删除。
- `Unavailable` 不修改既有事实。
- `failed`、`manual_required` 和取消任务不修改业务事实。
- 首次 complete 集合只建立 baseline，不生成 added/removed 事件。
- 后续 complete 集合才允许计算 transition。

虽然本次升级后 Sub2API 和 NewAPI 都不再产生模型集合，这项共享修复仍需要实施，用于消除 `full`/集合写入的结构性缺陷，并为未来新的完整集合事实提供安全合同。

### 4.4 变更事件不再按 adapter 字符串判断

删除 `supports_model_events(adapter: &str)`。

变更事件必须由以下条件共同决定：

- fact 类型声明允许跟踪；
- collection status 允许应用；
- 集合 completeness 为 complete；
- 已存在 baseline；
- transition 具有产品风险意义。

当前版本没有任何 Station collector 产生模型目录事件，因此 `model_added` / `model_removed` 不应从 collector application service 新建。

## 5. OpenAI-compatible Provider 移除步骤

### 5.1 Backend 类型与 Registry

修改：

- `src-tauri/src/services/collectors/contract.rs`
- `src-tauri/src/services/collectors/drivers/mod.rs`
- `src-tauri/src/services/collectors/drivers/openai_compatible/mod.rs`
- `src-tauri/src/services/collectors/mod.rs`
- `src-tauri/src/application/command_facades/station_collection.rs`
- `src-tauri/src/application/command_facades/provider_drafts.rs`
- `src-tauri/src/services/station_collectors.rs`

操作：

1. 删除 `ProviderKind::OpenAiCompatible`。
2. 删除 parser 对 `openai-compatible`、`openai_compatible` 和 `custom` 的映射。
3. 删除 Registry entry 和 driver 模块。
4. 删除 OpenAI-compatible prepared route enum variants。
5. 删除 prepare / finish helpers 以及仅服务该 Provider 的测试。
6. 保持所有协议层 `OpenAI-compatible` 类型不变。

### 5.2 Station 创建与更新合同

外部 StationType 输入收敛为：

```text
sub2api | newapi
```

修改：

- `src-tauri/src/ipc/dto/stations.rs`
- `src/lib/types/stations.ts`
- `src/features/stations/providerPresets.ts`
- Add Provider 页面及对应测试。

要求：

- 新建、更新和 provider draft commit 都拒绝三个旧别名。
- UI 不显示“自定义接口”类型。
- 删除依赖 `custom` station type 的官方厂商 presets；这些厂商不是当前支持的站点采集 Provider。
- 后端校验是权威边界，不能只依赖前端隐藏选项。

### 5.3 已有站点兼容策略

升级不得级联删除已有 OpenAI-compatible / custom Station、Station Key 或凭据。

schema 21 migration 应：

1. 将 `station_type IN ('openai-compatible', 'openai_compatible', 'custom')` 的站点设置为 `enabled = 0`。
2. 删除这些站点的 collector task state 和可重建 collector model facts。
3. 保留 Station、Station Key、加密凭据、历史请求日志和 collector runs。
4. 清理这些站点产生的 `model_added` / `model_removed` collector 事件。

应用层将这些记录识别为 `unsupported_legacy_provider`：

- 不允许重新启用、采集或进入路由候选。
- UI 可只读展示并允许用户删除。
- 不提供继续编辑成可运行 custom Provider 的入口。

该隔离策略让 Provider 从产品能力中删除，同时避免升级过程擅自删除用户资产。项目后续若确认无需保留，可通过显式用户操作删除，而不是在 migration 中级联清理。

## 6. NewAPI 模型采集移除步骤

修改 `src-tauri/src/services/collectors/drivers/newapi/mod.rs`：

1. 从 `SUPPORTED_COLLECTOR_TASKS` 删除 `Models`。
2. 删除 `CollectorTaskKind::Models` match 分支。
3. 删除 `collect_models`。
4. 删除 `/api/user/models` parser、fixture 和测试。
5. 将 NewAPI `full_tasks` 设置为 `Balance + Groups`。

修改 application write path：

1. NewAPI output 不再包含 canonical model facts。
2. 删除 NewAPI 模型事件资格。
3. 删除 `full` 父任务对 models 的聚合和 replacement。

修改前端：

1. NewAPI 任务选项仅显示探测、余额、分组/倍率和完整采集。
2. snapshot 仍可兼容读取历史 `models` 字段，但新快照不再产生该字段内容。
3. 兼容读取应有明确删除期限，不能成为永久双轨。

## 7. NewAPI Balance 固定成本改造

### 7.1 删除历史回放

删除或停止调用：

- `collect_usage_stats`
- `collect_log_stat_window`
- `collect_log_window`
- `collect_dashboard_usage_total`
- `collect_dashboard_usage_total_backwards`
- `NEWAPI_LOG_MAX_PAGES`
- `NEWAPI_DASHBOARD_TOTAL_MAX_WINDOWS`
- 相关全量日志路径 builder 和测试

### 7.2 保留 bounded 今日聚合

如果保留今日 dashboard：

- 时间范围只能是本地当天起点到当前时间。
- 最多一次请求。
- 失败时保留 `/api/user/self` 的核心余额事实。
- output 应为 `partial` 或在 diagnostics 标记 optional fact unavailable，不能伪装所有字段完整。

### 7.3 超时来源统一

- 删除 `NEWAPI_CHILD_TASK_TIMEOUT` 硬编码。
- prepare 阶段从 settings 读取 `collector_timeout_seconds`。
- `RequestBudget` 表示单项 child task 的总预算。
- `full` 的理论最大时长是各 child task 预算之和。
- endpoint request 不得重新创建更长预算。
- diagnostics 记录预算耗尽分类，但不记录 secret 或完整 query URL。

暂不为了性能引入站内并发。先通过删除 O(N) 历史回放把算法改成 O(1)，再根据真实采集数据决定是否并行请求 balance/groups。

## 8. Scheduler 与 UI 能力统一

新增 station-specific collector capability read model，例如：

```ts
type StationCollectorCapabilities = {
  stationId: string;
  provider: "sub2api" | "newapi";
  availableTasks: Array<"detect" | "balance" | "groups" | "full">;
  fullTasks: Array<"balance" | "groups">;
};
```

要求：

- Backend Registry 是 capability 唯一来源。
- CollectorsPage 不再硬编码五个任务选项。
- station 变化后，如果当前 task 不受支持，自动回退到 `full` 或第一个可用任务。
- scheduler 只为 provider 支持的 task 查询 due stations。
- `modelListIntervalMinutes` 暂时从采集设置 UI 和调度合同中移除；未来重新引入模型目录 Provider 时再通过 schema/API 版本明确恢复。
- `collectorMaxConcurrency` 仍表示跨站点并发，不表示单站点 child 并发。

## 9. Schema 21 数据升级

新增 append-only migration：

```text
src-tauri/src/persistence/migrations/0021_remove_unsupported_collector_providers.sql
```

迁移内容：

1. 禁用旧 OpenAI-compatible/custom stations。
2. 删除这些站点的 `collector_task_state`。
3. 删除这些站点的 `collector_model_facts`。
4. 删除 NewAPI 的 `collector_model_facts` 和 models task state。
5. 删除 NewAPI 及旧 Provider 的 collector `model_added` / `model_removed` events。
6. 更新 `persistence_schema_compatibility` 到 schema 21。

禁止修改历史 migration checksum，禁止在正常 startup 中加入 schema-specific repair 分支。

必须补充 postcondition：

- 不存在 enabled 的旧 Provider station。
- 不存在 NewAPI 或旧 Provider 的 collector model facts。
- 不存在 NewAPI 或旧 Provider 的 models task state。
- schema version 和 compatibility metadata 均为 21。

## 10. 实施顺序与提交边界

### Commit 1：Characterization 与失败测试

- 固化当前错误行为的边界测试。
- 新增目标 capability、请求路径、请求数量和事实保留测试。
- 不改生产行为。

### Commit 2：Provider Registry 收口

- capability descriptor 增加 `full_tasks`。
- driver task 删除 `Full`。
- 移除 OpenAI-compatible Provider 和 prepared routes。
- Station 输入类型收敛为 Sub2API/NewAPI。

### Commit 3：NewAPI 模型与共享写入止血

- NewAPI 删除 Models。
- Full 父任务 facts 置空。
- 集合完整性合同生效。
- collector 不再生成模型目录事件。

### Commit 4：NewAPI Balance 固定成本

- 删除历史 dashboard 回放和日志分页。
- 保留 O(1) 核心余额及可选今日聚合。
- settings timeout 接入 RequestBudget。

### Commit 5：Capability UI 与 Scheduler

- 新增 capability read DTO。
- UI 动态任务选项。
- 删除 custom presets 和模型周期设置。
- scheduler 按 capability 调度。

### Commit 6：Schema 21 与文档

- 追加 migration 和 postcondition。
- 隔离旧 Provider 数据。
- 清理无效模型事实和事件。
- 更新 `PROJECT_PLAN.md`、`PRODUCT_MODEL.md`、schema/release 文档和 generated contracts。

每个 commit 必须能独立编译和通过其范围内测试，禁止把所有变化压在一个无法二分定位的提交中。

## 11. 测试计划

### 11.1 Rust 单元与集成测试

- Provider Registry 只包含 Sub2API/NewAPI。
- Provider parser 拒绝 `openai-compatible`、`openai_compatible` 和 `custom`。
- NewAPI supported/full tasks 与 Sub2API 对齐。
- NewAPI models command 在发起网络前失败。
- NewAPI balance 请求路径 allowlist。
- NewAPI balance 请求数 `<= 3`。
- NewAPI full 请求数 `<= 4`。
- NewAPI 不请求 log endpoint。
- optional usage 失败仍保留核心余额。
- full parent 不写 canonical facts。
- failed/partial 集合不删除旧事实。
- 首次 complete 集合不产生 transition event。

### 11.2 Frontend 测试

- StationType 只接受 Sub2API/NewAPI。
- Add Provider 不显示自定义接口和相关 presets。
- NewAPI task selector 不显示模型。
- station capability 切换会修正失效 task selection。
- unsupported legacy provider 只能查看和删除，不能启用或采集。

### 11.3 Schema 测试

- schema 20 -> 21 精确升级。
- frozen schema 15 -> latest 升级。
- 旧 Provider station 被禁用但 Station Key/secret 仍存在。
- NewAPI/旧 Provider 模型事实与模型事件被清理。
- migration 重跑保持幂等结果。
- foreign key check 和 quick check 通过。

### 11.4 静态边界扫描

生产 collector/provider 模块中不得残留：

- `ProviderKind::OpenAiCompatible`
- `PreparedOpenAiCompatibleCollection`
- `collect_models` for NewAPI
- `/api/user/models`
- `/api/log/self` in NewAPI balance
- `supports_model_events`

扫描必须允许协议层、proxy、monitoring 和 qualification 中仍然存在合法的 `OpenAI-compatible` 文本。

## 12. 验收标准

1. 新建 Station 只能选择 Sub2API 或 NewAPI。
2. Collector Registry 只有两个 Provider。
3. NewAPI `full` 只有 balance/groups 两个 child run。
4. NewAPI 不访问 `/api/user/models` 或 `/api/log/self`。
5. NewAPI balance 最多三个请求，full 最多四个请求。
6. NewAPI 不创建或更新 collector model facts。
7. 变更中心不再产生 NewAPI 模型新增/下架事件。
8. 任意 failed/partial collector result 不会清空已有集合事实。
9. 已有旧 Provider station 在升级后禁用，Key 和凭据未被删除。
10. 本地 OpenAI-compatible 网关、上游协议路由和 Station Key connectivity probe 保持通过。

## 13. 验证命令

实现过程中至少运行：

```powershell
pnpm verify:fast
cargo test --manifest-path src-tauri/Cargo.toml newapi -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml collectors -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml station_collection -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test schema15_upgrade_fixture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml startup_upgrade -- --nocapture
```

还必须运行现有本地路由和状态监控边界测试，证明删除 Provider 没有误删 OpenAI-compatible 协议能力：

```powershell
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml routing -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml connectivity -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml monitoring -- --nocapture
```

真实站点验证只允许使用用户明确授权的测试站点和脱敏结果，不保存完整 endpoint、API key、cookie 或原始响应。

## 14. 非目标与后续扩展

本次不实现：

- 任意自研中转站采集 DSL。
- 用户自定义 collector 脚本或插件系统。
- 通用 OpenAI-compatible Provider 的余额、倍率或价格猜测。
- 全量历史 usage 分析。
- Station 内部 child task 并发。

未来重新支持新的 Provider 时，必须满足以下准入条件：

1. 有稳定、可测试的管理端协议或明确 adapter。
2. 每个采集事实有清晰所有权和 completeness 语义。
3. full task 组成由 capability descriptor 声明。
4. 所有网络遍历都有固定上限或增量水位。
5. 失败不会把未知解释为删除。
6. 变更事件只追踪具有产品风险意义的 transition。

在满足这些条件前，不恢复 OpenAI-compatible/custom 站点 Provider。
