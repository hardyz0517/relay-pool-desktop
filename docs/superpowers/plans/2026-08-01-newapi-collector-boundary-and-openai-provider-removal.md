# NewAPI 采集边界收口与 OpenAI-compatible Provider 移除实施计划

状态：已完成设计复审，待实施

日期：2026-08-01

适用范围：Relay Pool Desktop 当前 generation-2 数据层、Provider Registry、站点采集、采集调度、变更中心和信息采集 UI。

## 1. 背景与问题

当前 NewAPI 完整采集包含 `balance`、`groups` 和 `models`。这与 Sub2API 的站点采集边界不一致，也把账号可见的模型目录错误解释成站点或 Station Key 的真实路由能力。

本文中的“与 Sub2API 对齐”指采集任务和事实所有权对齐，不要求两个 Provider 暴露完全相同的原始字段。两者都只向 Station collector 提供余额、分组和倍率事实；Provider 特有且无法稳定归一化的字段可以留在脱敏 diagnostics 中，但不能据此扩大产品能力边界。

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

1. 先请求 `/api/user/self` 获取余额、已用额度、请求数等账号聚合事实。这是核心请求。
2. 在剩余预算允许时请求 `/api/status` 获取额度换算信息。这是可选归一化请求。

禁止行为：

- 从 Unix timestamp `0` 回放。
- 以 30 天窗口向历史回溯。
- 请求或分页 `/api/log/self`。
- 请求 `/api/data/self` 计算今日或历史 usage。
- 为计算 token 总数扫描历史请求日志。
- 因可选 usage 字段失败而阻塞核心余额结果。

`/api/user/self` 成功而 `/api/status` 失败时，保留原始额度事实并返回 `partial`，归一化货币值为 `unknown` / `None`。无法通过这两个 O(1) 接口可靠获得的今日 usage、token 拆分等字段同样返回 `unknown` / `None`，不能通过高成本回放补齐。

## 4. 架构升级方案

### 4.1 Provider capability 成为唯一任务来源

扩展 `CollectorCapabilityDescriptor`，明确区分 driver 可执行任务与 orchestration `full` 组成：

```rust
pub struct CollectorCapabilityDescriptor {
    pub direct_tasks: &'static [CollectorTaskKind],
    pub full_tasks: &'static [CollectorTaskKind],
}
```

约束：

- 从 driver 级 `CollectorTaskKind` 删除 `Full`。
- 外部 `CollectorTask::Full` 保留，由 orchestration 展开为 `full_tasks`。
- `full_child_tasks` 不再维护第二份 `match ProviderKind` 能力表。
- command、scheduler、provider draft preview 和 UI 都必须消费同一份 capability resolver。
- 后端必须在网络调用前拒绝不支持的 station/task 组合，返回稳定的 `unsupported_task`。
- Registry 构建时校验 `full_tasks` 非空、无重复、顺序稳定，且每一项都属于 `direct_tasks`；`Detect` 和 `Full` 不能出现在 `full_tasks` 中。
- station type alias 在所有 descriptor 之间必须唯一，Registry 初始化遇到重复或缺少必需 Provider 时失败关闭。

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

父任务可以更新自身的 `collector_task_state(full)`，但不得生成业务 transition 或 `collector_failed` 变更事件；失败事件由 leaf child task 按既有 dedupe key 负责，避免同一次失败同时出现父级和子级告警。父任务、子任务及其 snapshot/run 仍受现有 `endpoint_revision` fence 保护，过期任务不能写回当前事实。

该规则消除父子重复写入，也避免某个失败子任务让父任务携带空集合覆盖旧事实。

### 4.3 集合型事实引入完整性语义

共享 collector 输出需要给每一种集合事实增加显式完整性，不能只在整个 output 上放一个会混淆不同事实种类的全局标记：

```rust
pub enum FactSetCompleteness {
    Complete,
    Partial,
    Unavailable,
}

pub struct CollectedFactSet<T> {
    pub items: Vec<T>,
    pub completeness: FactSetCompleteness,
}
```

本次把同一 groups endpoint 产生的 `groups` 和 `rates` 组合为一个逻辑 `GroupCatalogFactSet`，共享单一 completeness，禁止两者各自声称不同完整性。余额仍是独立标量事实。未来增加新的集合事实时复用 `CollectedFactSet<T>`，不能依赖“空数组”猜测完整、失败或未提供。

写入规则：

- 只有 task outcome 可应用（`success` 或携带安全子事实的 `partial`）且该集合为 `Complete` 时允许 replacement；某个无关的可选 enrichment 失败不能阻止已证明完整的集合提交。
- `Partial` 只能 upsert 已观察到的事实，不能推断未出现项已删除。
- `Unavailable` 不修改既有事实。
- `failed`、`manual_required`、预算耗尽且无安全子事实的任务和取消任务不修改业务事实。
- 首次 complete 集合只建立 baseline，不生成 added/removed 事件。
- 后续 complete 集合才允许计算 transition。
- 集合 replacement、baseline 更新、task state 更新和 transition event 必须在同一数据库事务内完成。

新增持久化 `collector_fact_set_state(station_id, fact_kind, last_complete_run_id, updated_at)`，明确记录某类集合是否已有 complete baseline。`fact_kind` 由后端闭合 enum 管理；本次 groups/rates 共享同一 `group_catalog` baseline，避免同一 endpoint 的两个向量各自漂移。`last_complete_run_id` 可空并以 `ON DELETE SET NULL` 保留 baseline，station 删除时级联删除状态。不能用“当前事实表里是否有行”代替 baseline，因为一个合法的 complete 空集合也必须被记住。`Partial` 和 `Unavailable` 不推进 baseline；`full` 父任务不写该表。schema 21 升级后已有站点在下一次 complete collection 建立 baseline，因此升级后的第一次采集不会制造历史变化事件。

虽然本次升级后 Sub2API 和 NewAPI 都不再产生模型集合，这项共享修复仍需要实施，因为 groups 当前同样存在“partial 结果触发 missing”风险，也为未来新的完整集合事实提供安全合同。

### 4.4 变更事件不再按 adapter 字符串判断

删除 `supports_model_events(adapter: &str)`。

变更事件必须由以下条件共同决定：

- fact 类型声明允许跟踪；
- collection status 允许应用；
- 集合 completeness 为 complete；
- 已存在 baseline；
- transition 具有产品风险意义。

当前版本没有任何 Station collector 产生模型目录事件，因此从 collector application service 删除 `model_event` 创建路径，而不是保留一个永远为 false 的 feature flag。历史 run 和 snapshot 仍可读取，不能被当成当前 capability 或 transition baseline。

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

Station 类型必须拆成“可写输入”和“历史读取”两个合同，不能用一个窄 union 同时承担两者：

```text
SupportedStationTypeInput = sub2api | newapi
StationRead.stationType = string
StationRead.providerSupport = supported | unsupported
StationRead.providerSupportReason = null | legacy_provider | unknown_provider
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
- 旧站点仍需通过宽读取 DTO 返回，前端以只读隔离态渲染；不能把 `Station.stationType` 直接收窄为两项后导致旧数据反序列化失败。
- provider draft 的 preview、commit 和 Station update 必须在 application service 再次调用同一个 `StationProviderSupportPolicy`，不能只依赖 IPC enum。
- unsupported provider 返回稳定的 `unsupported_provider`；provider 合法但 task 不在 capability 中返回 `unsupported_task`，前端不能通过匹配错误文本分支。
- 路由/代理测试中用来表达“上游协议兼容”的 `station_type = openai-compatible` fixture 改为受支持的 Station 类型，并用 `upstream_api_format` 表达协议；避免测试继续混淆 Provider 类型和协议类型。

### 5.3 已有站点兼容策略

升级不得级联删除已有 OpenAI-compatible / custom Station、Station Key 或凭据。

schema 21 migration 应：

1. 将规范化后不在 `('sub2api', 'newapi')` 中的所有站点设置为 `enabled = 0`；三个已知旧别名在 UI 中显示 `legacy_provider`，其他未知值显示 `unknown_provider`。
2. 删除这些站点的 collector task state；模型事实表在 schema 21 的全局清理步骤中统一删除。
3. 保留 Station、Station Key、加密凭据、历史请求日志和 collector runs。
4. 清理这些站点产生的 `model_added` / `model_removed` collector 事件。

应用层将这些记录识别为 `unsupported_provider`，并通过 reason 区分已知 legacy alias 与未知类型：

- 不允许重新启用、采集或进入路由候选。
- UI 可只读展示并允许用户删除。
- 不提供继续编辑成可运行 custom Provider 的入口。

迁移不是唯一防线。新增单一 `StationProviderSupportPolicy`，由 Station create/update、provider draft、collector prepare、scheduler、remote-key 管理入口和路由候选构建共同调用。任何不在当前 Registry 中的 station type 一律 fail closed；即使测试或外部数据库操作把旧站点的 `enabled` 改回 `1`，也不能采集、管理远端 Key 或进入路由候选。

导入/恢复属于显式 transition，必须在切换为 active database 前执行相同隔离规则：

1. schema 15-20 的包先走正常升级到 schema 21，再验证隔离 postcondition。
2. schema 21 包仍要验证所有 unsupported station 均为 disabled；不满足时在导入临时副本中禁用并清理可重建 collector 状态，再做原子切换。
3. 导入摘要记录被隔离的 station 数量，但不包含 endpoint、Key、cookie 或凭据内容。
4. export 保留这些只读资产，确保用户仍可备份、恢复或显式删除。

这段逻辑只能存在于 migration/import transition，不能放进正常 startup 作为 schema-specific repair；正常运行时仅保留无副作用的 eligibility guard。

该隔离策略让 Provider 从产品能力中删除，同时避免升级过程擅自删除用户资产。项目后续若确认无需保留，可通过显式用户操作删除，而不是在 migration 中级联清理。Station 删除仍沿用现有级联语义，并必须由用户明确触发。

## 6. NewAPI 模型采集移除步骤

修改 `src-tauri/src/services/collectors/drivers/newapi/mod.rs`：

1. 从 `SUPPORTED_COLLECTOR_TASKS` 删除 `Models`。
2. 删除 `CollectorTaskKind::Models` match 分支。
3. 删除 `collect_models`。
4. 删除 `/api/user/models` parser、fixture 和测试。
5. 将 NewAPI `full_tasks` 设置为 `Balance + Groups`。

修改 application write path：

1. 从当前可执行 task 输入 DTO、`CollectorTask`、driver `CollectorTaskKind` 和前端 command type 中删除 `Models`。
2. 从 `CollectorFacts` 删除 `models`，并删除 `CollectedModelFact`、`replace_models` 和 collector model event 路径。
3. 删除 `full` 父任务对 models 的聚合和 replacement。
4. schema 21 删除无当前消费者的 `collector_model_facts` 表，而不是只清空后永久保留死表。

修改前端：

1. NewAPI 任务选项仅显示探测、余额、分组/倍率和完整采集。
2. 新 snapshot 不再产生 `models` 字段；历史 snapshot 是松散 JSON，详情页可继续展示其中的原始 `models`，但必须标注为历史采集结果且不参与当前摘要、路由或 capability。
3. `collector_runs.task_type` 继续按字符串读取，因此旧 `models` run 可在历史记录中显示；写入命令和调度器均不能再创建这种 run。读取兼容不是双写，也不要求保留可执行 enum variant。

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

### 7.2 核心余额优先

- `/api/user/self` 是 required endpoint，必须先执行。
- `/api/status` 只负责额度换算 enrichment；失败、超时或预算不足时，保留 self 返回的原始事实并标记 `partial`。
- 不保留“今日 dashboard”请求，避免客户端时区与服务端统计时区不一致，也避免可选字段改变核心余额任务的成功语义。
- established session 下 balance 只有两个 logical endpoint；认证或 token refresh 属于单独 evidence role，但也必须共享同一根预算。

### 7.3 超时来源统一

- 删除 `NEWAPI_CHILD_TASK_TIMEOUT` 硬编码。
- command/scheduler 开始一次用户可见任务时，从 settings 读取 `collector_timeout_seconds`，只创建一个 absolute deadline。
- 认证、token refresh、`full` 的所有 child task 和 endpoint retry 共享该 deadline；禁止 child task 续期或重新创建预算。
- `collector_timeout_seconds` 明确定义为一次用户可见任务的远端 I/O 总预算；`full` 的远端阶段不超过配置值，而不是 child 数量乘以配置值。UI 标签同步改成“采集网络预算”，避免把本地数据库提交时间误解为网络 timeout。
- 预算耗尽后未开始的 child run 记录为 `failed` + `error_code = budget_exhausted` + `endpoint_count = 0`，不引入新的持久化 status；已成功 child 的事实仍可按 child transaction 写入。
- diagnostics 记录预算耗尽分类，但不记录 secret 或完整 query URL。
- cancellation 和应用退出必须传播到正在执行的认证、retry backoff 和 endpoint request。
- diagnostics 分开记录 auth、remote I/O、apply 和 total duration，便于确认未来的慢点在网络还是本地持久化；任何阶段都不得记录完整 endpoint query 或凭据。

请求上限分两层验证：logical endpoint 数由 capability/driver 合同限制；wire attempt 数由统一的 collector retry policy 限制。实现必须明确 `max_attempts_per_endpoint` 并让每次 retry 消耗根预算，不能只断言成功路径的 URL 数量。默认是否保留一次瞬时错误重试由现有 outbound policy 决定，但最大次数必须是常量且有测试。

暂不为了性能引入站内并发。先通过删除 O(N) 历史回放把算法改成 O(1)，再根据真实采集数据决定是否并行请求 balance/groups。

## 8. Scheduler 与 UI 能力统一

新增由 Registry 一次性导出的 provider capability read model，而不是按 station 发起 N 次能力查询：

```ts
type CollectorProviderCapabilities = {
  stationType: "sub2api" | "newapi";
  availableTasks: Array<"detect" | "balance" | "groups" | "full">;
  fullTasks: Array<"balance" | "groups">;
};
```

要求：

- Backend Registry 是 capability 唯一来源。
- capability command 一次返回稳定排序、去重后的全部 descriptor；前端按所选 Station 的 `stationType` 映射，unsupported legacy station 没有可执行任务。
- CollectorsPage 不再硬编码五个任务选项。
- station 变化后，如果当前 task 不受支持，自动回退到 `full` 或第一个可用任务。
- scheduler 只为 provider 支持的 task 查询 due stations。
- `modelListIntervalMinutes` 从采集设置 UI、Rust/TypeScript settings model、IPC/generated contract、preset、scheduler 和 schema settings row 中删除；旧数据库由 schema 21 migration 删除该 key。未来重新引入模型目录 Provider 时必须通过新 schema/API 版本明确恢复，不能复活旧字段。
- `collectorMaxConcurrency` 仍表示跨站点并发，不表示单站点 child 并发。

## 9. Schema 21 数据升级

新增 append-only migration：

```text
src-tauri/src/persistence/migrations/0021_remove_unsupported_collector_providers.sql
```

迁移内容：

1. 禁用所有不受当前 Registry 支持的 stations。
2. 删除这些站点的 `collector_task_state`。
3. 删除所有 `task_type = 'models'` 的 task state。
4. 删除 `source = 'collector' AND event_type IN ('model_added', 'model_removed')` 的错误派生事件；不删除其他来源的同名历史事件。
5. 删除 `collector_model_facts` 表以及 portable migration/schema catalog 中对该表的当前依赖。
6. 新建 `collector_fact_set_state`，用于 complete baseline；升级不猜测或回填历史 baseline。
7. 删除 settings 中的 `model_list_interval_minutes`。
8. 更新 `persistence_schema_compatibility` 到 schema 21。

禁止修改历史 migration checksum，禁止在正常 startup 中加入 schema-specific repair 分支。

必须补充 postcondition：

- 不存在 enabled 的 unsupported Provider station。
- 不存在 `collector_model_facts` 表。
- 不存在 models task state 或 collector 来源的模型 transition event。
- `collector_fact_set_state` 存在且外键有效。
- 不存在 `model_list_interval_minutes` setting。
- schema version 和 compatibility metadata 均为 21。

迁移顺序必须满足外键：先删依赖模型事实的派生数据与 task state，再 drop table；整个 migration 在单一事务中完成。postcondition 还要执行 `PRAGMA foreign_key_check` 和 `PRAGMA quick_check`。同时更新 frozen schema fixture、portable migration catalog/schema reader、release schema declaration 和生成的 contract fixture，不能只更新 SQL 文件。

## 10. 实施顺序与提交边界

### Commit 1：Characterization 与失败测试

- 固化当前错误行为的边界测试。
- 新增目标 capability、请求路径、请求数量和事实保留测试。
- 不改生产行为。

### Commit 2：Capability 与事实合同基础

- capability descriptor 增加 `full_tasks`。
- driver task 删除 `Full`。
- 增加 Registry invariant 校验和 provider capability read model。
- 增加 Registry invariant 校验和 per-fact-set completeness 类型；暂不改 schema 或删除 Provider。

### Commit 3：NewAPI 边界与固定成本

- NewAPI 删除 Models。
- Full 父任务 facts 置空。
- NewAPI 不再写模型事实或生成模型事件；共享 OpenAI-compatible 路径暂留到原子移除 commit。
- 删除历史 dashboard 回放和日志分页。
- 保留两个 O(1) 核心 endpoint。
- settings timeout 接入共享 absolute RequestBudget，并限制 retry attempts。

### Commit 4：Provider 移除与数据隔离（原子变更）

- schema 21 migration、postcondition 和 portable import 隔离规则。
- 移除 OpenAI-compatible Provider、prepared routes 和 driver。
- Station 写入输入收敛为 Sub2API/NewAPI，历史读取保持宽类型。
- routing/scheduler/remote-key eligibility guard 同步生效。
- 集合完整性、durable baseline、父任务事件抑制和模型事实表删除同步生效。
- Rust/TypeScript settings、UI、generated contract 与 schema 中的模型周期字段同步删除。
- 该 commit 不得拆成“先删除运行时代码、后补迁移”的可交付中间状态。

### Commit 5：Capability UI 与 Scheduler

- 新增 capability read DTO。
- UI 动态任务选项。
- 删除 custom presets。
- scheduler 按 capability 调度。
- unsupported station 只读展示与删除入口。

### Commit 6：清理、文档与生成物

- 删除已无引用的模型 collector 代码与 fixture。
- 更新 `PROJECT_PLAN.md`、`PRODUCT_MODEL.md`、schema/release 文档和 generated contracts。
- 更新仍使用 `openai-compatible` station type 表达协议的 routing/proxy fixtures。

每个 commit 必须能独立编译和通过其范围内测试，禁止把所有变化压在一个无法二分定位的提交中。commit 是 review unit，不是独立 release；Provider 代码删除、schema 21 和 import quarantine 必须在同一发布版本交付，不能发布中间态。

## 11. 测试计划

### 11.1 Rust 单元与集成测试

- Provider Registry 只包含 Sub2API/NewAPI。
- Provider parser 拒绝 `openai-compatible`、`openai_compatible` 和 `custom`。
- NewAPI supported/full tasks 与 Sub2API 对齐。
- 当前 task 输入 DTO 不再接受 `models`，且旧 models run 仍能按字符串读取。
- NewAPI balance 请求路径 allowlist。
- established session 下 NewAPI balance logical endpoint 恰为 `/api/user/self` 和可选 `/api/status`，数量 `<= 2`。
- established session 下 NewAPI full logical endpoint 数 `<= 3`：balance 两个、groups 一个。
- retry 测试证明 wire attempts 不超过 `logical endpoints * max_attempts_per_endpoint`，并受共享 deadline 限制。
- NewAPI 不请求 log endpoint。
- `/api/status` 失败仍保留 `/api/user/self` 核心余额并返回 partial。
- 慢响应、retry backoff 和 cancellation 均不能突破 full 的根 deadline。
- full parent 不写 canonical facts。
- full parent 不生成重复 failure/transition event，成功 child 事实不因其他 child 失败而回滚或被清空。
- `Partial`/`Unavailable` 集合不删除旧事实；task outcome 为 partial 但某集合明确为 `Complete` 时，只允许替换该集合。
- complete 空集合能建立 durable baseline；首次 complete 集合不产生 transition event，后续 complete 才产生。
- endpoint revision 改变后，旧 run 不能更新事实、baseline 或 change event。
- 即使旧 Provider station 被手工改为 enabled，collector、remote-key mutation 和 routing candidate 仍 fail closed。

### 11.2 Frontend 测试

- Station 写入类型只接受 Sub2API/NewAPI，读取模型能表示 unsupported station 及原因。
- Add Provider 不显示自定义接口和相关 presets。
- NewAPI task selector 不显示模型。
- station capability 切换会修正失效 task selection。
- unsupported provider 只能查看和删除，不能启用或采集。
- capability 只请求一次，不随 station 数产生 N+1 IPC；descriptor 未加载或未知 station type 时 UI fail closed。
- 历史 models run/snapshot 可查看，但不出现在当前任务、当前 capability 或路由摘要中。

### 11.3 Schema 测试

- schema 20 -> 21 精确升级。
- frozen schema 15 -> latest 升级。
- 旧 Provider station 被禁用但 Station Key/secret 仍存在。
- `collector_model_facts` 被删除，collector 模型事件与 models task state 被清理。
- `collector_fact_set_state` 支持 complete 空 baseline，且 foreign key 正确。
- `model_list_interval_minutes` setting 被删除。
- 升级完成后重复启动不会重复执行 migration 或改变隔离结果。
- foreign key check 和 quick check 通过。
- schema 15-20 portable import 会先升级再隔离；不合规 schema 21 包在临时副本中隔离后才允许原子切换。
- export/import 后旧 Provider 的 Station、Station Key 和 secret 数量保持不变，且仍然 disabled/unschedulable。
- migration 或 import postcondition 失败时不替换 active database。

### 11.4 静态边界扫描

生产 collector/provider 模块中不得残留：

- `ProviderKind::OpenAiCompatible`
- `PreparedOpenAiCompatibleCollection`
- `collect_models` for NewAPI
- `/api/user/models`
- `/api/log/self` in NewAPI balance
- `/api/data/self` in NewAPI balance
- `supports_model_events`
- `collector_model_facts` outside historical migrations, schema-21 drop SQL and migration/catalog tests
- `model_list_interval_minutes` outside historical migrations, schema-21 delete SQL and compatibility tests

扫描必须允许协议层、proxy、monitoring 和 qualification 中仍然存在合法的 `OpenAI-compatible` 文本。

## 12. 验收标准

1. 新建 Station 只能选择 Sub2API 或 NewAPI。
2. Collector Registry 只有两个 Provider。
3. NewAPI `full` 只有 balance/groups 两个 child run。
4. NewAPI 不访问 `/api/user/models`、`/api/log/self` 或 `/api/data/self`。
5. established session 下 NewAPI balance 最多两个 logical endpoint，full 最多三个；wire attempts 和远端 I/O 阶段同时有固定上限，完整任务各 phase duration 可观测。
6. NewAPI 不创建或更新 collector model facts。
7. 变更中心不再产生 NewAPI 模型新增/下架事件。
8. 任意 failed task 或 `Partial`/`Unavailable` 集合不会清空已有集合事实。
9. 首次或空 complete 集合只建立 baseline，不制造“新增”事件；full 父任务不重复写事实或告警。
10. 已有旧 Provider station 在升级/导入后禁用，Key 和凭据未被删除，且任何运行路径都不能重新激活它。
11. 新建、更新、draft commit 和 task command 只能使用当前 Registry 支持的类型/任务；历史读取不崩溃。
12. 本地 OpenAI-compatible 网关、上游协议路由和 Station Key connectivity probe 保持通过。

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

## 14. 发布、回滚与观测

### 14.1 发布前置条件

- migration runner 在修改 schema 前沿用现有 generation-2 备份、journal 和 typed recovery 边界；本次不得新增 startup 特判。
- 新二进制必须先完成 schema 21 与 import postcondition，才启动 scheduler、本地 proxy 和后台 collector。
- schema compatibility metadata 必须让旧二进制拒绝直接打开 schema 21，避免旧代码重新写入已删除的模型表或旧 Provider 状态。
- 发布资格记录只保存数量和耗时：被隔离 station 数、删除的派生模型事件数、各采集 phase duration、logical endpoint 数和 wire attempts；不得保存站点 URL 或 secret。

### 14.2 回滚策略

本次包含 drop table 和输入合同收口，不提供 down migration。若 schema 21 升级或 postcondition 失败，保持 active database 未切换并进入现有 typed recovery；若升级后必须回退旧版本，只能先完全退出应用，再恢复升级前备份。禁止让旧二进制直接写 schema 21 数据库。

### 14.3 失败门禁

出现以下任一情况不得发布：

- unsupported station 能进入 routing candidate、remote-key mutation 或 collector prepare。
- portable import 能恢复出 enabled unsupported station。
- full 的 remote I/O 超过根预算，或 retry 没有固定 attempt 上限。
- partial/unavailable group catalog 会把已有 group 标记 missing。
- schema 15 -> latest、schema 20 -> 21、foreign key check 或本地 OpenAI-compatible 协议回归任一失败。

## 15. 非目标与后续扩展

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
