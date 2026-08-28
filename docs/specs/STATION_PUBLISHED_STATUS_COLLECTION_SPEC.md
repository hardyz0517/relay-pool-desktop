# 中转站发布状态采集规范

状态：Implemented，已完成本地自动化资格；真实 Sub2API smoke 与发布资格仍待完成  
日期：2026-08-16  
适用范围：中转站官方渠道状态采集、采集调度、持久化、IPC 读模型与中转站详情 UI  
提案类型：Station Collector 新事实类型与详情页只读工作区  
替代关系：不替代 `STATUS_MONITORING_REFACTOR_SPEC.md`；两者必须保持独立事实来源、执行路径和产品语义

关联入口：

- `AGENTS.md`
- `docs/README.md`
- `docs/PRODUCT_MODEL.md`
- `docs/SCHEMA_UPGRADE_AUTHORING.md`
- `docs/SECURITY_EXPORT_IMPORT.md`
- `docs/plans/2026-08-16-station-published-status-collection.md`
- `docs/specs/STATUS_MONITORING_REFACTOR_SPEC.md`
- `docs/research/SUB2API_SOURCE_AUDIT.md`
- `src-tauri/src/services/collectors/contract.rs`
- `src-tauri/src/services/collectors/facts.rs`
- `src-tauri/src/application/collectors.rs`
- `src-tauri/src/services/station_collectors.rs`
- `src/features/stations/StationDetailPage.tsx`
- `src/features/channels/components/StatusTrend.tsx`

## 实施状态（2026-08-17）

本规范的生产实现已落地：Sub2API 通过一次 `GET /api/v1/channel-monitors` 拉取官方 Monitor 列表与时间线，归一化后仅写入 `station_published_*` 事实表，并经独立 workspace command 在中转站详情的“官方渠道状态”区段展示。`published_status` 是独立 Collector task；它不写主动渠道监控、`station_key_health` 或路由健康，也不会把区段读取加入详情核心请求。

已自动验证 parser、4 MiB 响应上限、512 个 Monitor 上限、每 Monitor/model 60 条数值时间留存、旧 endpoint revision 清理、revision fence、故障保留、429/5xx 失败语义、IPC/ACL、调度、迁移、portable migration、前端故障隔离、站点切换迟到结果隔离和共享趋势视觉；真实 SQLite 测试还覆盖 run key 幂等、partial/failed 事实保留、事务回滚与 `512 x 60` workspace 截断。`pnpm verify:fast`、bindings、构建和专项架构门禁均通过。旧客户端省略 `publishedStatusIntervalMinutes` 时会采用 5 分钟兼容默认值，当前 source 为 `unsupported` 时不会以保留历史行替代明确的不支持提示。

发布前仍须在用户控制的、已授权 Sub2API 测试站完成脱敏 smoke，验证真实鉴权恢复和线上响应兼容性。`pnpm verify:full` 的 advisory、license/source、契约、构建、前端和其余 Rust 阶段已通过；完整 Rust suite 本次为 1144 通过、1 失败，唯一失败是既有的 `v2_loopback_upstream_disconnect_publishes_final_jsonl_event` 时序用例。该用例随后单独复跑通过，发布前仍应在稳定 CI 环境复验完整套件。

## 1. 执行摘要

本功能采集中转站管理端自己发布的渠道监控结果，并在中转站详情中显示每个上游 Monitor 的当前状态、最近 60 次可用率、延迟、Ping、官方更新时间和最近 60 次记录。

它不是 Relay Pool Desktop 主动发起的渠道探针，也不是路由健康事实。内部领域名称统一使用 `Station Published Status / 中转站发布状态`，UI 可使用“官方渠道状态”作为用户可理解的名称。

### 1.1 渠道状态官方聚合工作区

渠道状态页面提供三个并列入口：`本地状态`、`官方状态`、`探针管理`。官方状态入口只读取所有已登记且当前 endpoint revision 的发布状态事实，通过
`get_station_published_status_overview` 返回 Station、source 和 Monitor 的聚合投影；它不逐站 fan-out，也不触发网络采集。该投影固定按 Station priority、Station、Provider、Group、Monitor、Primary model 和稳定 ID 排序，支持受限搜索、Station、source state、outcome 筛选及版本化 cursor 分页。

官方状态同时展示两个维度：`sourceState` 表示 Relay Pool 最近一次采集官方 API 的结果和新鲜度，`currentOutcome` 表示中转站对该 Monitor 的当前判断；采集失败或过期时保留最后一次成功 Monitor 行，禁止覆盖成单一综合状态。每行仅展示主模型最近最多 60 条 sample 和后端计算的最近可用率，不提供 24h/7d 切换、不读取 `availability_7d`，也不提供全局重新采集按钮。页面刷新只重新读取本地 read model；单站重新采集仍沿用详情页既有 task。

目标链路如下：

```text
Sub2API management API
  -> provider-specific published-status parser
  -> canonical published-status batch
  -> transactional source/monitor/sample persistence
  -> StationPublishedStatusWorkspace query
  -> 中转站详情“官方渠道状态”表格
```

核心决策：

1. 本功能归属 `Station / Collector`，不归属 `Channel Status / Status Monitoring`。
2. 不抓取 `/monitor` HTML；采集页面背后的结构化管理 API。
3. 首版只支持契约已经审计的 Sub2API；NewAPI 和其他实现通过 provider capability 后续接入。
4. 首版使用一次列表请求取得 Monitor 列表和 `timeline[]`，不执行逐 Monitor 的 N+1 详情请求。
5. 官方状态只作为带来源的展示事实，不写入 `station_key_health`，不影响本地探针、路由、fallback 或 cooldown。
6. 可用率由主模型已保留、规范化后的最近至多 60 条 sample 计算；不得解析、保存或展示上游 `availability_7d`，也不得把它作为批次完整性或降级判断的依据。
7. 写入以一个采集批次为原子边界；失败或不完整批次不得删除最后一次成功事实。
8. 每个 Monitor / model 最多保留最近 60 条 sample，所有请求、响应、查询和历史均有明确上限。
9. 详情页使用独立的 loading、empty、error、authorization-required、partial 和 stale 状态；该区段失败不得导致整个中转站详情失败。
10. UI 只复用通用趋势条视觉，不复用主动探针表格的运行、取消、execution 或健康语义。

## 2. 现状审计与方向结论

### 2.1 当前代码事实

- `Collector` 已经围绕 Station 提供 provider registry、管理端鉴权、共享出站客户端、请求预算、后台调度、station single-flight、collector run 和原子持久化骨架。
- 当前 `CollectorTaskKind` 只有 `Detect`、`Balance`、`Groups`，用户任务另有 `Full`；后台仅调度余额和分组倍率。
- 当前 `CollectorFacts` 只包含余额、分组和倍率，尚无发布状态事实。
- 当前 `CollectorService::apply_result` 同时承担 run、snapshot、事实写入、task state、站点采集状态和告警副作用。新增任务必须明确哪些副作用适用，不能默认继承。
- 当前详情页把多项核心请求放在一个 `Promise.all` 中；新增的可选状态区段若直接加入，会扩大整页失败面。
- 当前主动状态页已有可复用的固定槽位趋势条，但完整表格包含本地 monitor execution、运行和取消操作，不能跨域复用。
- 当前 schema 最新迁移为 `0040`。按当前基线，本功能预计新增 `0041`；实际实施时必须以合并时最新迁移号为准。

### 2.2 可维护性、可拓展性与可靠性审查

| 方向 | 结论 | 约束 |
| --- | --- | --- |
| 作为新的 Collector task 接入 | 通过 | 使用闭合枚举和 provider capability，不用字符串旁路 |
| 复用管理端鉴权和出站客户端 | 通过 | Secret 只在发送前解析，不进入事实、日志或 DTO |
| 复用 station coordinator 和 due-task 调度 | 通过 | 独立刷新周期、同 Station single-flight、全局并发有界 |
| 新建发布状态事实表 | 通过 | 与 collector run 同事务提交，历史有界 |
| 复用趋势条共享视觉 | 通过 | 组件移到共享边界并保留现有 attribution |
| 写入主动监控表或健康状态机 | 拒绝 | 官方自报状态不是本地主动探测证据 |
| 把响应塞进 `collector_snapshots.normalized_json` 供前端解析 | 拒绝 | 快照只可作为脱敏诊断，不是产品读模型 |
| 抓取 `/monitor` HTML 或 DOM | 拒绝 | 页面结构不稳定、难测试且扩大安全边界 |
| 每次轮询逐 Monitor 请求详情 | 首版拒绝 | 产生 N+1、限流和部分失败复杂度 |
| 将可选请求加入详情页核心 `Promise.all` | 拒绝 | 官方状态失败不能拖垮资产详情 |
| 复用完整 `ChannelStatusTable` | 拒绝 | 主动运行、取消、execution 等语义不成立 |
| 根据官方状态改变路由健康 | 首版拒绝 | 来源可信度不足且会形成第二条隐式健康写回路径 |

审查结论：现有架构可以可靠承载本功能，但只有在“独立事实域、专用持久化、专用读模型、明确副作用策略”全部满足时才算可维护。最危险的捷径是复用主动监控数据表或快照 JSON；这两种方案都必须由测试和架构门禁禁止。

## 3. 术语与领域边界

| 术语 | 定义 |
| --- | --- |
| Published Status Source | 某 Station 当前 endpoint revision 下的官方状态采集来源和新鲜度状态 |
| Published Monitor | 中转站发布的一个 Monitor 定义或展示卡片 |
| Published Monitor Sample | 上游 Monitor 在某个模型和检查时间上的一次官方记录 |
| Published Status Batch | 一次列表采集形成的规范化批次，包含完整性和 source state |
| Source State | `never_collected`、`available`、`empty`、`unsupported`、`authorization_required`、`degraded`、`failed` |
| Sample Outcome | `available`、`degraded`、`unavailable`、`unknown` |
| Local Collection Freshness | Relay Pool 最近成功读取官方状态 API 的时间新鲜度 |
| Upstream Checked Time | 中转站声称执行该条官方检测的时间 |

### 3.1 与主动渠道监控的强制隔离

| 维度 | 中转站发布状态 | 主动渠道监控 |
| --- | --- | --- |
| 所有者 | Station Collector | Channel Status / Monitoring |
| 数据来源 | 中转站管理 API 自报 | Relay Pool 主动请求模型端点 |
| 目标身份 | 上游 Monitor | Station Key / Probe Target |
| 执行模型 | 拉取一个发布状态列表 | Execution -> Target Result -> Probe Attempt |
| 凭据 | 管理端登录 session/access token | Station Key secret |
| 可信度 | 第三方发布、仅展示 | 本地测量、可参与健康状态机 |
| 持久化 | `station_published_*` | `channel_monitor_*` |
| 路由影响 | 无 | 按监控规范受控写回健康 |
| UI | 中转站详情区段 | 独立渠道状态工作区 |

以下依赖和写入永久禁止，除非后续单独形成跨域规范并得到明确批准：

- 发布状态写入 `channel_monitors`、`channel_monitor_executions`、`channel_monitor_target_results` 或 `channel_monitor_probe_attempts`。
- 发布状态直接调用 `HealthTransitionService` 或写入 `station_key_health`。
- 主动监控读取 `station_published_*` 作为 execution 或 target result。
- 前端把两类状态合并成一个无来源标识的“最终状态”。

## 4. 目标

### 4.1 产品目标

- 在中转站详情快速查看该站自己发布的渠道运行情况。
- 每个上游 Monitor 显示当前状态、主模型、分组、最近 60 次可用率、延迟、Ping 和最近 60 次记录。
- 清楚区分“站点发布”和“Relay Pool 主动探测”。
- 保留最后一次成功结果，在授权失效、网络失败或接口下线时显示 stale，而不是清空页面。
- 手动“重新采集”能够刷新官方状态，后台能够按独立周期自动刷新。

### 4.2 工程目标

- Provider-specific 解析与通用调度、持久化、IPC 和 UI 解耦。
- 新 provider 只需增加 capability、adapter parser 和 fixture，不修改主动 monitoring 内核。
- 写模型和读模型分离，前端不解释上游原始 JSON。
- 批次写入幂等、原子、可取消、可审计并受 endpoint revision 保护。
- 失败不破坏最后一次成功事实，partial 不触发缺失淘汰。
- 请求数、响应体、Monitor 数、sample 数、数据库保留和 IPC payload 全部有界。

## 5. 非目标

首版不包含：

- 主动探测上游模型端点。
- 用官方状态更新路由健康、Key cooldown、调度权重或 fallback 顺序。
- 将官方状态和主动探针计算成单一综合分数。
- 公共互联网状态页发布、云同步、Webhook 或第三方告警。
- HTML/DOM 抓取、浏览器自动化或任意 CSS selector 配置。
- 任意用户自定义状态 URL、Header 或解析脚本。
- 逐 Monitor 获取 15 天、30 天或模型级完整详情。
- 解析、持久化或展示上游 `availability_7d` 等跨日可用率摘要。
- 保存上游完整响应、认证 Header、Cookie、token 或可还原凭据的诊断数据。
- 为 NewAPI 猜测并实现不存在的统一 `/monitor` 契约。

## 6. 上游契约与首版范围

### 6.1 Sub2API

仓库现有研究记录给出以下高置信 API：

```text
GET /api/v1/channel-monitors
GET /api/v1/channel-monitors/:id/status
```

列表项已审计字段：

```text
id
name
provider
group_name
primary_model
primary_status
primary_latency_ms
primary_ping_latency_ms
extra_models[]
timeline[]
```

首版仅调用列表接口。`timeline[]` 作为该 Monitor 主模型的最近记录；`extra_models[]` 只作为附加模型元数据展示，不为其伪造 60 条历史。上游响应即使包含 `availability_7d`，parser 也必须忽略它：不读取为事实字段、不影响 batch 完整性、不写入数据库且不进入 DTO。

详情接口只有在后续明确需要 15/30 日窗口或模型级详情时才接入。届时必须使用独立的按需查询、缓存、请求预算和有界并发，不得加入每次后台列表采集的默认路径。

### 6.2 契约固化要求

研究资料不是运行时契约。实施 Stage 0 必须从以下至少一种来源生成脱敏 fixture：

- 对应 Sub2API 版本的 handler/DTO 源码；
- 用户控制的测试站真实响应，删除全部认证和账号数据后固化；
- 仓库已经审计过的上游版本生成的最小合成响应。

fixture 至少覆盖：

- 完整列表与 60 条 timeline；
- 少于 60 条、超过 60 条、乱序和重复 timestamp；
- 空列表；
- 未知 status；
- nullable latency/Ping；
- 含任意合法或非法 `availability_7d` 的响应均不影响解析结果、batch 完整性或最近 60 次可用率；
- 单条损坏但其他条目合法；
- 错误 envelope；
- 401、403、404、429 和 5xx。

## 7. 目标模块与依赖方向

建议模块：

```text
src-tauri/src/models/station_published_status.rs

src-tauri/src/services/collectors/drivers/sub2api/
  published_status.rs

src-tauri/src/application/
  station_published_status.rs

src-tauri/src/persistence/stores/
  station_published_status_store.rs

src-tauri/src/commands/
  station_published_status.rs

src-tauri/src/ipc/dto/
  station_published_status.rs

src/lib/api/stationPublishedStatus.ts
src/lib/types/stationPublishedStatus.ts

src/features/stations/components/
  StationPublishedStatusSection.tsx
```

依赖规则：

- `models/station_published_status` 不依赖 Tauri、SQLx、Reqwest、SecretManager 或 monitoring 模块。
- Sub2API parser 只负责请求、envelope 解析、字段校验和 provider-specific 状态映射。
- Application 层拥有批次完整性、缺失标记、幂等和保留策略。
- Persistence store 只实现查询和事务内写入，不解释上游状态字符串。
- IPC 返回稳定 workspace DTO，不暴露 raw JSON 或数据库 row。
- UI view model 只负责格式化和视觉 tone；最近 60 次可用率由后端 read model 计算，前端不得从原始响应或独立摘要字段重算。
- 任一 `station_published_status` 生产模块都不得依赖 `application::monitoring`、`services::monitoring` 或 `persistence::stores::monitoring`。

## 8. 领域模型

### 8.1 PublishedStatusBatch

```text
station_id
endpoint_revision
source_kind
source_state
completeness: complete | partial
monitors[]
collected_at
safe_error_kind?
```

不变量：

- `station_id` 必须与采集目标一致。
- `endpoint_revision` 必须与准备阶段快照一致。
- `complete` 只允许用于完整解析的成功 envelope。
- `partial` 可以写入合法项，但不得把未出现项标记 missing。
- `empty` 必须是合法成功 envelope 的空 `items[]`，不能由解析失败推断。
- `unsupported` 必须来自受控的 404/feature-not-supported 分类，不能由任意网络错误推断。

### 8.2 PublishedMonitorFact

```text
upstream_monitor_id
identity_kind: upstream_id | derived_fallback
name
provider
group_name?
primary_model
extra_models[]
current_outcome
source_status
current_latency_ms?
current_ping_latency_ms?
upstream_checked_at?
samples[]
```

身份规则：

1. 优先使用非空上游 `id`。
2. 只有兼容站缺少 ID 时，才允许使用规范化 `name + provider + group + primary_model` 的 hash。
3. derived identity 必须在 DTO 中保留较低 confidence，不能静默伪装成上游稳定 ID。
4. 单批次出现重复 identity 时，该 identity 记为损坏；不能以数组顺序覆盖。

### 8.3 PublishedMonitorSampleFact

```text
model
outcome
source_status
latency_ms?
ping_latency_ms?
checked_at
safe_message?
```

校验规则：

- `checked_at` 必须可解析并规范化为 UTC 时间；无合法时间的 timeline 项不进入历史。
- latency/Ping 必须是有限、非负且不超过实现上限的整数毫秒。
- `source_status` 最长 64 字符，只用于诊断和未来兼容；业务逻辑只消费 `outcome`。
- `safe_message` 可选，必须经过 secret sanitizer 并按 UTF-8 边界截断，建议上限 512 字节。

### 8.4 最近 60 次可用率

`recent_availability_percent` 是 read model 派生值，不是上游摘要，也不持久化为 Monitor 事实字段。计算必须在主模型的规范化、排序、去重并裁剪后的 `recent_samples` 上执行：

```text
sample_count = recent_samples.len() // 0..=60
available_count = count(sample.outcome == available)
recent_availability_percent = available_count * 100 / sample_count
```

- 分母是全部保留 sample，包含 `degraded`、`unavailable` 与 `unknown`，不得只统计已知或成功状态。
- 无有效 sample 时返回 `null`，UI 显示 `--`。
- 计算只使用该 Monitor 的 `primary_model`；不得把 `extra_models` 或未来详情接口的记录混入分母。
- 精度、舍入和显示格式由稳定 DTO/UI 契约定义，但不得改变上述分子和分母。

## 9. Provider Capability 与采集任务

新增闭合任务：

```text
CollectorTaskKind::PublishedStatus
CollectorTask::PublishedStatus
```

序列化 task type 固定为：

```text
published_status
```

Provider capability：

- Sub2API：首版支持 `PublishedStatus`，并将其加入 provider-specific `FULL_COLLECTOR_TASKS`。
- NewAPI：首版不声明支持；`Full` 不会尝试该任务。
- 未来 provider：只有存在稳定结构化 API、鉴权策略和 fixture 后才能声明支持。

所有闭合匹配必须同步更新：

- task enum 与字符串转换；
- driver capabilities；
- full task 展开；
- request validation；
- due-task allowlist；
- collector task state 查询；
- IPC task type；
- UI action label；
- fixtures、架构门禁和 exhaustive tests。

禁止通过任意 task 字符串绕过这些更新。

## 10. 请求、鉴权与传输

### 10.1 请求策略

- 目标 URL 只能由 Station 的受信任 management website URL 与固定 provider path 构造。
- 不允许用户输入任意 published-status URL、scheme、Header 或 query。
- 复用 `AsyncOutboundClient`、collector proxy policy、超时预算、取消 token、重定向和 URL 安全规则。
- 首版每个 Station 每次任务最多一次列表请求；认证刷新可按现有 single-flight 规则产生受控附加请求。
- 响应体上限建议为 4 MiB；超过上限分类为 `response_too_large`，不保留截断 JSON。
- Monitor 上限为 512；每个 Monitor 输入 timeline 上限为 240，规范化后只保留最新 60。
- 对 429 尊重合法且有界的 `Retry-After`，但不在一次 collector task 内无界重试。

### 10.2 鉴权策略

- `/api/v1/channel-monitors` 使用 Sub2API 管理端 Bearer access token；允许复用现有 refresh token/session cookie 恢复路径。
- 不使用 Station Key，也不把登录 secret 转换成普通字符串跨越持久化或 IPC 边界。
- 401/403 在受控刷新仍失败后分类为 `authorization_required`。
- 鉴权失败不得清空最后一次成功状态。
- 日志、collector snapshot、runtime event 和错误 DTO 不得包含 Authorization、Cookie、token 或原始响应。

## 11. 状态归一化

状态映射必须是 provider-specific 纯函数。首版基于 fixture 建立显式表：

```text
documented healthy values   -> available
documented warning values   -> degraded
documented failed values    -> unavailable
unrecognized/null values    -> unknown
```

要求：

- 不使用子串模糊匹配，例如包含 `ok` 就判定 available。
- 不把未知状态映射为 unavailable；未知表示契约漂移，不表示上游故障。
- current outcome 优先使用明确的 `primary_status`；不得仅凭 latency 是否存在推断。
- timeline 顺序不可信，必须按 `checked_at` 排序后去重和截取。
- 同一 `(model, checked_at)` 重复记录完全相同时去重；内容冲突时保留确定性记录并将批次标记 partial，规则必须由 fixture 测试锁定。

## 12. 持久化设计

按当前 schema 40 基线，预计新增 append-only migration `0041_station_published_status.sql`。若实施时已有新迁移，则顺延编号，不修改历史 migration。

### 12.1 `station_published_status_sources`

建议字段：

```text
station_id TEXT
endpoint_revision INTEGER
source_kind TEXT
source_state TEXT
last_attempt_at TEXT
last_success_at TEXT NULL
last_complete_at TEXT NULL
last_error_kind TEXT NULL
monitor_count INTEGER
created_at TEXT
updated_at TEXT
PRIMARY KEY (station_id, endpoint_revision, source_kind)
```

`source_state` CHECK：

```text
never_collected
available
empty
unsupported
authorization_required
degraded
failed
```

### 12.2 `station_published_monitors`

建议字段：

```text
id TEXT PRIMARY KEY
station_id TEXT
endpoint_revision INTEGER
source_kind TEXT
upstream_monitor_id TEXT
identity_kind TEXT
name TEXT
provider TEXT
group_name TEXT NULL
primary_model TEXT
extra_models_json TEXT
presence_status TEXT
current_outcome TEXT
source_status TEXT
current_latency_ms INTEGER NULL
current_ping_latency_ms INTEGER NULL
availability_7d_percent REAL NULL
upstream_checked_at TEXT NULL
last_seen_run_id TEXT
last_seen_at TEXT
created_at TEXT
updated_at TEXT
UNIQUE (station_id, endpoint_revision, source_kind, upstream_monitor_id)
```

`presence_status` 为 `current | missing`。`extra_models_json` 只能保存已验证、去重和有界的字符串数组。

`availability_7d_percent` 是已发布 schema 的历史兼容列。新旧读模型均不得读取它；所有新采集批次的 insert/upsert 必须写入 `NULL`，以清除旧版本残留值。该列不得承载任何上游 `availability_7d` 数据，也不得据此新增 migration。

### 12.3 `station_published_monitor_samples`

建议字段：

```text
id TEXT PRIMARY KEY
monitor_id TEXT REFERENCES station_published_monitors(id) ON DELETE CASCADE
model TEXT
checked_at TEXT
outcome TEXT
source_status TEXT
latency_ms INTEGER NULL
ping_latency_ms INTEGER NULL
safe_message TEXT NULL
first_seen_run_id TEXT
last_seen_run_id TEXT
created_at TEXT
updated_at TEXT
UNIQUE (monitor_id, model, checked_at)
```

索引至少包括：

- source：`station_id, endpoint_revision, source_kind`；
- monitor：`station_id, endpoint_revision, presence_status, updated_at DESC`；
- sample：`monitor_id, model, checked_at DESC, id DESC`。

### 12.4 保留策略

- 每个 active Monitor / model 保留最近 60 条 sample。
- sample 裁剪在同一 apply 事务内完成，必须使用确定性 `checked_at DESC, id DESC` 顺序。
- 最近 60 次可用率只在读模型中由主模型裁剪后的 sample 计算，不保存独立汇总，也不读取上游跨日摘要字段。
- missing Monitor 及其样本默认保留 30 天，用于短期恢复和诊断。
- 每个 Station / endpoint revision 的 Monitor 总量最多 512；超过上限的输入不得静默覆盖现有事实。
- endpoint revision 改变后，读模型只读取当前 revision；旧 revision 由成功新批次或维护任务有界清理。
- Station 删除通过外键级联清理全部发布状态事实。

## 13. 原子写入与幂等

一次 `published_status` apply 必须在同一个 persistence write session 中完成：

1. 校验 station 和 endpoint revision。
2. 按现有 run key/request hash 规则幂等创建 collector run。
3. 写入最小、脱敏 collector snapshot；snapshot 不是产品读模型。
4. 更新 source state。
5. upsert Monitor 元数据。
6. upsert samples。
7. 将历史兼容列 `availability_7d_percent` 写为 `NULL`，不保留旧版本的上游摘要。
8. 仅在 `completeness=complete` 时标记本批未出现的 Monitor 为 missing。
9. 裁剪每个 Monitor/model 到最近 60 条。
10. 更新 `collector_task_state`。
11. 完成 collector run。

事务任一步失败必须整体回滚。commit 结果未知时沿用现有 run key 查询恢复，不得重复生成历史。

### 13.1 失败批次

网络、认证或响应解析在没有合法 batch 时：

- 记录失败 collector run；
- 更新 source `last_attempt_at`、`source_state` 和安全错误分类；
- 保留 `last_success_at`；
- 不修改 Monitor presence；
- 不删除、覆盖或裁剪最后一次成功 samples。

### 13.2 Partial 批次

- 可 upsert 已通过验证的 Monitor 和 sample；
- source state 为 `degraded`；
- 不标记缺失 Monitor；
- 不以不完整列表作为完整 inventory；
- 仍可按已写入 Monitor/model 执行 60 条局部裁剪。

### 13.3 Collector 通用副作用策略

`published_status` 是可选采集能力，不能无条件继承现有 Collector 的全部副作用：

- standalone published-status run 不覆盖 Station 的核心 `collection_status`。
- `unsupported` 和合法 `empty` 是成功语义，不产生 generic collector failure。
- 首版不产生 Change Center 告警或健康 observation。
- `Full` 中受支持的 published-status 子任务失败可以使 Full 显示 partial，但不得抹掉余额、分组等成功子任务事实。
- 任务副作用必须由闭合策略函数表达，例如 `updates_station_collection_status(task)`、`emits_generic_collector_observation(task)`，不得散落字符串判断。

## 14. 调度与手动刷新

新增设置建议：

```text
published_status_interval_minutes
default: 5
valid range: 1..=1440
```

不复用余额或分组倍率周期，也不重新解释当前兼容字段 `collector_interval_minutes`。

调度规则：

- 复用现有 30 秒 runner tick、`due_stations_for_task`、Station coordinator 和全局 collector 并发设置。
- 同一 Station 的余额、分组、发布状态和手动采集继续受同一 station lease 保护。
- due query 必须分页并有上限；不能一次加载无界 Station。
- 单个 Station 内 due tasks 按稳定顺序执行，某一任务失败不阻止其他独立任务。
- 应用关闭或任务取消时不得提交 partial in-memory batch。
- 手动“重新采集”通过 provider-specific Full task 包含 published status。
- UI 可提供区段级刷新，但必须调用相同 collector task 路径，不能另建直连 IPC。

本地采集 stale 定义：

```text
now - last_success_at > max(2 * configured_interval, 10 minutes)
```

该 stale 只描述 Relay Pool 没有及时读取官方 API，不推断上游 Monitor 自己是否按计划运行。

## 15. Read Model 与 IPC

新增单个有界查询：

```text
get_station_published_status_workspace(station_id)
```

建议 DTO：

```text
StationPublishedStatusWorkspace {
  stationId
  endpointRevision
  sourceKind
  sourceState
  lastAttemptAt?
  lastSuccessAt?
  stale
  safeErrorKind?
  rows[]
}

StationPublishedStatusRow {
  rowKey
  upstreamMonitorId
  identityKind
  name
  provider
  groupName?
  primaryModel
  extraModels[]
  currentOutcome
  currentLatencyMs?
  currentPingLatencyMs?
  recentAvailabilityPercent?
  upstreamCheckedAt?
  collectedAt?
  recentSamples[] // <= 60
}
```

查询规则：

- 在一个 read session 中批量读取 source、current monitors 和各自最近 60 条 samples。
- 不执行每行 SQL 查询，不产生 N+1。
- 默认只返回 `presence_status=current` 的 Monitor。
- `recentAvailabilityPercent` 由该 row 的主模型 `recentSamples` 计算：`available` sample 数除以全部 sample 数，再乘以 `100`；无 sample 时为 `null`。它不得读取 `availability_7d_percent`，也不得来自上游 `availability_7d`。
- workspace 总 Monitor 数和 sample 数必须受后端上限约束。
- 排序由后端确定：`provider -> group_name -> name -> primary_model -> upstream_monitor_id`，前端不得依赖数据库偶然顺序。
- DTO 不包含 raw JSON、source status message 原文、管理 URL、认证状态细节或 secret reference。
- 命令必须进入统一 IPC registry，并通过 `pnpm generate:bindings` 更新生成物；不得手写重复 bridge 类型。

## 16. 中转站详情 UI

新增独立区段：`官方渠道状态`。

推荐位置：中转站指标之后、分组与倍率之前。表格列：

```text
监控 / 分组
模型
当前状态
最近 60 次可用率
延迟 / Ping
官方更新时间
最近 60 次
```

UI 语义：

- 每行对应一个上游 Published Monitor，不对应 Station Key。
- 状态、可用率和时间旁必须通过标题或 tooltip 明确来源为“站点发布”。
- `recentAvailabilityPercent` 直接显示后端 read model 从最近至多 60 条主模型 sample 得到的值；无 sample 显示 `--`。UI 不读取或展示上游 `availability_7d`。
- 趋势格 tooltip 显示官方检查时间、状态、延迟和 Ping；不使用“本地探测”措辞。
- `extraModels` 可显示为有界标签或 tooltip；首版不为其显示虚构趋势。
- 区段允许水平滚动，表格应保持紧凑、高密度和浅色桌面工具风格。

必须覆盖状态：

- loading：稳定高度 skeleton，不改变详情页其他区段布局；
- never collected：尚未采集；
- empty：站点未发布监控；
- unsupported：当前站点类型或版本不支持；
- authorization required：需要重新授权；
- failed/stale：保留最后结果并显示采集新鲜度警告；
- partial：显示合法行和“部分数据未能解析”状态；
- narrow window：横向滚动且操作、文本和 tooltip 不重叠。

### 16.1 前端加载隔离

官方状态 workspace 不加入 Station 详情核心 `Promise.all`。推荐由独立 hook/controller 管理：

```text
useStationPublishedStatus(stationId)
```

要求：

- 区段请求失败只影响该区段。
- Station 切换时丢弃旧请求结果。
- 手动 full/published-status 采集成功后只刷新相关 workspace 和 collector metadata。
- 不使用定时前端轮询替代后端采集调度。

### 16.2 趋势组件复用

现有 `StatusTrend` 可抽取到共享状态可视化目录，但必须：

- 用通用 cell DTO 和可配置 aria label/tooltip content；
- 保持固定 60 槽布局和无数据占位；
- 保留当前来源 attribution 和许可证说明；
- 主动监控和发布状态分别构造自己的 view model；
- 不把主动监控的 `skipped/dirty/corrupt` 业务语义强加给发布状态。

## 17. 失败语义矩阵

| 场景 | Collector run | Source state | 事实处理 | UI |
| --- | --- | --- | --- | --- |
| 200，完整非空 | success | available | 原子 upsert、标记 missing、裁剪 60 | 显示当前行 |
| 200，合法空列表 | success | empty | 标记旧 Monitor missing，保留历史 | 空状态 |
| 200，部分条目损坏 | partial | degraded | 写合法项，不标记 missing | 部分数据提示 |
| 401/403，刷新失败 | manual_required 或 failed | authorization_required | 保留最后成功事实 | 重新授权状态 |
| 404，明确无能力 | success | unsupported | 不改变最后成功事实；当前 revision 无行 | 不支持状态 |
| 429 | failed | failed | 保留事实，记录安全分类 | stale/稍后重试 |
| 5xx、DNS、连接或超时 | failed | failed | 保留事实 | stale/失败 |
| 响应体超限 | failed | failed | 不解析、不保存截断 body | 数据过大安全错误 |
| envelope 损坏 | failed | failed | 不修改 Monitor/sample | 契约异常 |
| endpoint revision 已变化 | conflict/stale discard | 保持新 revision 状态 | 整批丢弃 | 等待新目标采集 |
| 应用取消或关闭 | cancelled | 不更新 | 不提交 batch | 保留旧状态 |

## 18. 安全与隐私

- 官方状态是第三方提供的数据，默认只具有展示可信度，不具有本地健康证明能力。
- 不保存请求 Header、Cookie、access token、refresh token、登录密码或完整响应。
- 上游 message/error 视为不可信输入；进入数据库和 DTO 前必须脱敏、截断和控制字符清理。
- 所有字符串字段必须有长度上限，防止恶意站点造成数据库和 UI 资源放大。
- Monitor 名称、模型、provider 和 group 名称必须按普通文本渲染，禁止 `dangerouslySetInnerHTML`。
- URL 只能通过现有 station endpoint builder 构造，禁止任意 scheme、userinfo、fragment 和跨 origin 重定向。
- runtime log 只记录固定事件 code、provider kind、结果分类和数量，不记录动态响应正文。
- default export 是否包含该元数据必须在实现时明确登记；无论何种导出都不得包含 raw response 或认证信息。
- portable migration catalog、artifact policy 和 support bundle 必须明确该表的包含或排除策略，并通过 secret canary。

## 19. 可拓展性规则

### 19.1 新 Provider

新增 provider 支持时只允许扩展：

- provider capability；
- provider-specific endpoint、auth 和 parser；
- status mapping；
- fixtures 与 contract tests。

以下模块不应因新增 provider 而修改核心算法：

- scheduler；
- source/monitor/sample persistence schema；
- retention；
- workspace DTO；
- 详情页表格核心结构。

如果新 provider 无稳定上游 Monitor ID，可以使用 derived fallback identity，但必须保留较低 identity confidence。若其数据无法映射到 `PublishedMonitor + Sample`，应新增显式版本化能力，而不是把任意 JSON 塞入现有模型。

### 19.2 新时间窗口

未来加入 15/30 日时：

- 必须先形成版本化的独立采集契约与产品需求；首版不得借此重新采集 `availability_7d` 或其他跨日可用率摘要；
- 不用最近 60 条重算跨日窗口；
- 详情请求按用户动作懒加载并缓存；
- 详情 endpoint 的失败不影响列表事实；
- 不默认永久保存上游所有历史。

### 19.3 与主动监控并列展示

未来可在 Station 详情并列显示“站点发布”和“本地主动探测”，但必须是两个来源明确的 read model。若需要差异提示，应新增只读 comparison projection：

```text
Published Status + Local Monitoring -> StatusComparisonView
```

该 projection 不写回任一事实源，也不产生综合健康状态，除非另有批准规范。

## 20. 测试策略

### 20.1 Domain 与 parser

- 每个已知 status 的精确映射；
- unknown 不误判；
- 时间、延迟和字符串边界；
- 乱序、重复、冲突、少于/超过 60 条；
- 最近 60 次可用率：0、1、60 条 sample，所有 outcome 均计入分母，只有 `available` 计入分子，无 sample 为 `null`，且只使用 `primary_model`；
- 合法、缺失、非法的 `availability_7d` 均被忽略，不使 batch 降级，也不进入事实或 DTO；
- 额外模型有界去重；
- malformed item partial 与 malformed envelope failed；
- secret canary 不进入 facts、diagnostics 或错误。

### 20.2 Transport 与 driver

- 正常列表 loopback；
- access token、refresh、cookie fallback；
- 401/403/404/429/5xx；
- timeout、cancel、redirect、oversized body；
- request count 证明首版没有 N+1；
- proxy policy 和 endpoint URL 安全。

### 20.3 Persistence

- migration N -> N+1 postcondition；
- schema 15 -> latest fixture；
- foreign key、CHECK、unique 和索引；
- 完整批次原子 upsert；
- 同 run key 幂等；
- endpoint revision 陈旧写拒绝；
- failed/partial 保留最后成功事实；
- complete 才标记 missing；
- 每 Monitor/model 精确保留最近 60 条；
- Station 删除级联；
- 旧 endpoint revision 和 missing Monitor 有界清理。

### 20.4 Scheduler 与应用层

- due query 支持 `published_status`；
- provider 不支持时不调度；
- 同 Station single-flight；
- 全局并发上限；
- 任务失败不阻止其他 due task；
- manual/full 与 scheduled 使用同一执行路径；
- published-status run 不覆盖核心 station collection status；
- 不产生 monitoring health writeback 或 generic alerting 副作用。

### 20.5 IPC 与前端

- workspace 序列化 golden 和生成 bindings；
- 单 read session 批量查询，无 N+1；
- DTO monitor/sample 数量上限；
- loading、empty、unsupported、authorization、partial、stale、error；
- 固定 60 槽和 tooltip 内容；
- Station 切换丢弃旧请求；
- 区段失败不影响详情其他内容；
- 窄窗口和键盘焦点。

## 21. 实施阶段

### Stage 0：契约冻结

- 固化脱敏 Sub2API fixture。
- 确认 status、timestamp、timeline 和 envelope 语义。
- 确认当前最新 schema 和生成绑定版本。
- 将本文中所有待实现上限落为具名常量和测试。

### Stage 1：Domain 与持久化

- 新增纯领域类型和 provider-independent batch。
- 新增 append-only migration、postcondition、store 和 retention。
- 在 collector apply 事务中接入 published-status helper。
- 增加明确的 task side-effect policy。

### Stage 2：Sub2API 采集

- 新增 capability 和 `PublishedStatus` task。
- 接入固定管理 API、现有鉴权恢复和出站预算。
- 完成 parser、状态映射、失败分类和 loopback tests。
- 接入 Full 和后台 due scheduler。

### Stage 3：Read Model 与 UI

- 新增 workspace query、IPC DTO、registry 和生成绑定。
- 新增独立详情区段 controller。
- 抽取共享趋势条视觉并保留 attribution。
- 完成所有 UI 状态和窄窗口测试。

### Stage 4：资格验证

- 运行专项 Cargo/Vitest。
- 运行 schema 15 升级 fixture 和 startup upgrade tests。
- 运行 `pnpm generate:bindings` 并确认生成物无漂移。
- 运行 `pnpm build`、`cargo fmt --check`、`cargo check --locked`。
- 因为涉及 schema、IPC、采集基础设施和跨层契约，至少运行 `pnpm verify:fast`；较大实现批次完成时运行 `pnpm verify:full`。
- 使用本地测试站进行一次脱敏人工验收，不把真实 token、响应、数据库或截图提交仓库。

## 22. 验收标准

实现只有同时满足以下条件才算完成：

1. Sub2API Station 能通过管理 API 获取官方 Monitor 列表并显示在详情页。
2. 每个 Monitor 最多显示并持久化最近 60 条有序、去重记录。
3. 每个 Monitor 的最近 60 次可用率由主模型保留的至多 60 条 sample 派生：`available` 为分子、全部 outcome 为分母；无 sample 为 `null`，且不解析、持久化或展示上游 `availability_7d`。
4. 同一批次重复执行不产生重复 Monitor 或 sample。
5. 失败、partial、授权失效和应用取消不删除最后一次成功事实。
6. endpoint revision 改变后的旧请求不能写入当前 Station 事实。
7. 404/empty/authorization/failed/stale 在 UI 中具有不同语义。
8. 官方状态区段失败不影响中转站其他详情。
9. 没有逐 Monitor N+1 请求和 SQL 查询。
10. `station_published_*` 与主动 monitoring 表之间没有生产写依赖。
11. 官方状态不会改变 `station_key_health`、路由选择或主动 Monitor execution。
12. 请求、数据库、IPC 和 UI 资源均受上限保护。
13. 日志、snapshot、DTO、fixture 和测试输出均不含 secret 或原始认证数据。
14. schema、生成绑定、Rust、前端和跨层验证按仓库门禁通过。

## 23. 已拒绝方案

### 23.1 复用主动监控数据表

拒绝原因：官方状态是第三方自报，缺少本地 execution、target、attempt 和语义验证证据。写入主动表会污染健康统计，并可能错误影响路由。

### 23.2 仅保存 collector snapshot JSON

拒绝原因：前端会绑定上游字段，无法可靠去重、增量更新、查询、迁移或执行 60 条保留；snapshot 也会成为事实和诊断的双重所有者。

### 23.3 抓取 `/monitor` 页面

拒绝原因：DOM、CSS、语言和构建产物不稳定；浏览器会话扩大攻击面；难以建立严格响应上限、字段验证和可重复 fixture。

### 23.4 默认调用每个 Monitor 的详情接口

拒绝原因：列表已经提供首版所需 timeline。N+1 会放大请求数、限流、取消、部分失败和调度时间。

### 23.5 用官方状态直接驱动路由

拒绝原因：Monitor 目标不一定等同于用户持有的 Station Key，官方结果也没有 Relay Pool 的请求画像和语义验证。首版只能展示带来源事实。

## 24. 后续实现约束

- 实现计划应引用本文，但计划文件不得修改本文的领域隔离和可靠性不变量。
- 若真实 Sub2API fixture 与研究记录冲突，应先更新本文的契约章节和测试，再实现 parser。
- 若实现需要修改主动 monitoring 内核、健康状态机或路由写入，说明范围已超出本 spec，必须停止并形成新的跨域设计。
- 若新 provider 只能通过 HTML 抓取实现，默认判定为不支持，不能降低本规范的结构化 API 要求。
- 完成实施后，将状态更新为 Implemented，并在资格记录中链接实际测试和门禁结果；不得只根据代码存在声称完成。
