# Dashboard 请求指标聚合 Read Model 升级实施计划

状态：完成；直接聚合性能资格失败后已按计划晋级到 Dashboard rollup/增量投影，并完成重新资格

资格结论（2026-08-01）：

- 已完成 Dashboard live/cumulative 后端 read model、IPC、前端 Query owner、Dashboard 卡片 cutover、usage/cost/边界行为测试和 source architecture gates。
- 已删除 Dashboard request metrics 对 `RequestLog[]` 前端聚合、旧 base-cost comparison 和 dead `loadDashboardWorkspace` composite query service。
- 已添加 `received_at_ms` / `usage_status` migration、Dashboard metrics 专用 covering range index、request-level actual cost 聚合读取，以及可重复性能探针 `scripts/dashboard_metrics_perf_probe.py`。
- 直接聚合性能证据未通过主门槛：100k rows live warm p95 591.248 ms、cumulative warm p95 746.764 ms；500k rows live warm p95 2870.616 ms、cumulative warm p95 2962.371 ms。`EXPLAIN` 已证明 range index/covering index 被使用，因此瓶颈是 full-range scan 本身，不是丢失索引。
- 已补 0023 Dashboard request metric/cost rollup schema、request start/finish/cost aggregate 增量维护、clear cascade、query 前 repair/rebuild，以及 portable migration derived-object 边界；负 delta 使用纯 `UPDATE`，避免 SQLite `UPSERT` 先触发非负约束。
- Rollup 重新资格通过：100k rows live warm p95 0.302 ms、cumulative warm p95 0.076 ms；500k rows live warm p95 0.734 ms、cumulative warm p95 0.115 ms；writer p95 regression 6.156%，SQLite busy 0。`EXPLAIN` 证明 read path 使用 `dashboard_request_*_rollups` range index。
- 结论：本计划内的语义、架构、rollup 晋级、性能资格、migration/portable 修复和前后端验证均已闭环；未做 stage/commit，等待人工 review。

日期：2026-08-01

适用范围：本地路由请求生命周期、请求日志兼容投影、请求成本聚合、Dashboard 请求指标、前端 Query owner、生成式 IPC binding 与相关开发期验证。

上位规范：`AGENTS.md`、`docs/README.md`、`docs/PROJECT_PLAN.md`、`docs/archive/specs/2026-07-19-request-lifecycle-architecture-upgrade-design.md`、`docs/archive/specs/2026-07-22-architecture-scale-upgrade-design.md`、`docs/specs/2026-07-30-routing-operational-unification-upgrade-spec.md`

## 1. 背景与问题

当前 Dashboard 的“今日请求”“今日 Token”“累计 Token”“平均响应”“性能概览”和请求成本摘要都从 `list_request_logs` 返回的请求日志数组在前端临时计算。该命令固定只读取最近 `500` 条请求日志，因此这些卡片不是完整统计：

- 5 分钟窗口超过 500 条请求时，RPM 最大只能显示 `500 / 5 = 100`，TPM 同时被截断；
- 今日请求、今日 Token、今日平均耗时和今日成本只覆盖今日最近 500 条；
- 累计 Token 与累计成本实际只覆盖最近 500 条，并非累计事实；
- `duration_ms`、`first_token_ms` 和当前活跃请求具有不同语义，但 UI 只用“平均响应”统称；
- 缺失 usage 的请求被前端以 `totalTokens ?? 0` 处理，TPM 看起来完整，实际上没有暴露样本覆盖率；
- Dashboard 每 2 秒传输最多 500 条宽 `RequestLog` DTO，只为了计算少量标量；
- 生成的 `RequestLogDto` 已包含 `lifecycleStatus`，手写 `RequestLog` 类型没有该字段，生命周期样本口径不能由类型系统约束；
- 现有 `dashboard-performance-metrics.test.mjs` 主要匹配源码字符串和 CSS class，不能证明真实数据链路、时间窗口和 501 条以上样本的正确性。

请求生命周期写入本身已经打通：v2 ingress 持久化 request start，统一 finalization 回写 terminal、duration、usage 和成本聚合。本升级不创建第二套写入事实，而是在现有 request-level facts 上建立后端 use-case read model。

## 2. 目标与完成定义

本升级完成后，数据流必须收敛为：

```text
v2 ingress / request finalization / request cost aggregation
  -> request_logs + routing_request_cost_aggregates
  -> snapshot-consistent DashboardMetricsReadRepository
  -> DashboardMetricsQuery
  -> generated load_dashboard_live_request_metrics IPC
     + generated load_dashboard_cumulative_request_metrics IPC
  -> dashboardLiveRequestMetricsQueryOptions (2s while running)
     + dashboardCumulativeRequestMetricsQueryOptions (30s while running)
  -> Dashboard request metric cards

request_logs bounded page
  -> requestLogsQueryOptions
  -> recent usage list / request log page only

proxy runtime counters
  -> proxyStatusQueryOptions
  -> activeRequests only
```

只有同时满足以下条件才算完成：

- Dashboard 的请求计数、Token、耗时、吞吐和成本不再依赖 `RequestLog[]` 前端扫描；
- 最近 5 分钟存在 501 条以上请求时，RPM/TPM 仍返回完整窗口结果；
- 今日和累计指标不受请求日志页面大小影响；
- RPM、TPM、总耗时、首 Token、活跃请求和成本各自有固定且可测试的语义；
- usage 缺失、无 usage 适用性、未终结和损坏时间戳不会被静默伪装成完整的 0；
- 成本只聚合 request-level `routing_request_cost_aggregates`，不能再扫描当前价格，也不能把 attempt cost 与 request aggregate 相加；
- live snapshot 内的 recent/today/今日成本在同一 SQLite read session、同一 `captured_at_ms` 下生成；cumulative snapshot 内的 lifetime/全局质量/累计成本也独立满足该一致性，不要求两个不同 cadence 的 snapshot 共享捕获时间；
- 2 秒轮询只读取有界 recent/today live snapshot；lifetime 全量聚合进入 30 秒慢速 cumulative snapshot，避免随数据增长每 2 秒反复全表扫描；runtime active count 明确作为独立 overlay；
- IPC DTO 由 Rust 权威合同生成 TypeScript，前端不再手写一份不同字段集合；
- 前端保留稳定 loading/error/stale 状态，Dashboard 指标失败不影响最近使用列表，最近使用失败也不伪造指标为 0；
- 直接聚合在目标数据规模下通过性能门槛；只有性能证据不通过时才进入本文定义的 rollup 升级分支；
- Rust、TypeScript、Vite、binding、architecture 和目标回归检查真实退出 0。

## 3. 非目标

本计划不做以下工作：

- 不改变 v2 request lifecycle、fallback、routing outcome 或成本计算算法；
- 不把监控探针请求、Collector 网络请求或任意远端站点账号统计混入本地代理请求指标；
- 不实现云端 telemetry、账号系统、跨设备 Dashboard 同步或长期指标服务；
- 不用 request log retention 代替指标语义；
- 不在第一阶段引入 Prometheus、时序数据库、通用事件总线或后台 OLAP worker；
- 不为了显示平均值实现 p50/p95 全量排序；百分位只有在独立需求和有界算法确定后再增加；
- 不删除请求日志页、最近使用列表或现有请求 deep link；
- 不恢复 legacy proxy runtime、双写路径或前端 fallback 聚合；
- 不要求签名安装包、旧 binary rollback 或真实 provider secret；当前阶段仍按开发期 reset/reimport/重新配置恢复策略执行。

## 4. 指标语义冻结

### 4.1 时间窗口

两个 read model 响应各自包含 `captured_at_ms`。本次固定两个窗口和一个全量范围：

| 范围 | 边界 | 用途 |
|---|---|---|
| recent | `[captured_at_ms - 5min, captured_at_ms)` | RPM、TPM、近期数据质量；排除快照时点之后的时间戳 |
| today | `[local_day_start_ms, captured_at_ms)`，且已验证 `captured_at_ms < local_day_end_ms` | 截至快照时点的今日请求、Token、耗时、成本 |
| lifetime | `0 < received_at_ms < captured_at_ms` 的 canonical request rows | 截至快照时点的累计请求、Token、成本；损坏/未来 timestamp 只进入全局质量计数 |

前端只提供本地自然日的绝对毫秒边界，不提供 `now`。后端通过注入的 `Clock` 生成 `captured_at_ms`，并验证：

- `day_start_ms <= captured_at_ms < day_end_ms`，本地午夜整点是合法捕获时间；
- day window 长度允许 DST 造成的 23、24 或 25 小时，硬范围为 22 到 26 小时；
- recent window 由后端固定为 5 分钟，前端不能传任意大窗口；
- 所有 SQL 都使用同一个捕获时间，不在每条查询中重新读取系统时间；
- `received_at_ms > captured_at_ms` 不进入任何事实范围，并计入 cumulative `future_timestamp_count`；
- 前端跨本地午夜后必须刷新 query key 和数据，不继续展示上一自然日快照。

### 4.2 请求计数

`request_count` 表示通过本地认证并成功进入 v2 request lifecycle 的请求数，以 `request_logs` 中 canonical request start 为准：

- 包含 success、failed、interrupted 和仍在 `in_progress` 的 admitted 请求；
- 不包含 CORS preflight、认证失败和 lifecycle admission 之前被拒绝的请求；
- 每个 request_id 只计一次，不按 fallback attempt 计数；
- `active_requests` 不从数据库推导，继续来自 proxy runtime overlay；
- UI 文案保持“请求”，详情或 tooltip 明确为“已进入本地路由生命周期”。

### 4.3 RPM

`rpm = recent.request_count / 5`，表示 trailing 5-minute admitted request start rate：

- 分母始终是固定 5 分钟，不因窗口内第一条请求时间而缩短；
- 允许小数，显示层沿用 compact formatting，但 DTO 保留未格式化 `f64`；
- 不能使用 runtime `request_count`，该计数会随 proxy restart 清零且包含 lifecycle 之前的部分 ingress；
- 不能从最近 500 条日志近似；
- recent 窗口中进行中的请求计入 RPM，因为 RPM 描述 arrival throughput，而不是 terminal throughput。

### 4.4 Token 与 TPM

Token 汇总只消费 request-level usage compatibility projection：

- `prompt_tokens`、`completion_tokens`、`total_tokens` 分别求和；
- `total_tokens IS NOT NULL` 才是 known usage sample；合法的 `0` 与 unknown 必须区分；
- TPM 使用 known `total_tokens` 总和除以 5；
- response 中返回 `known_usage_request_count`、`missing_usage_request_count`、`stream_usage_missing_request_count`、`not_applicable_usage_request_count` 和 `unknown_usage_request_count`；
- `missing_usage_request_count` 只统计按 canonical endpoint 语义应产生 usage、但最终没有 usage 的终态请求，并包含 `stream_usage_missing_request_count` 这个可诊断子集；
- `/v1/models`、`/v1/usage` 和其他明确不产生模型 usage 的请求归入 `not_applicable`，不能污染缺失率；
- 进行中请求既不算 missing，也不算 known；
- `unknown_legacy` 不并入 known、missing 或 not-applicable，单独进入 `unknown_usage_request_count`；
- UI 在缺失或 unknown 样本大于 0 时仍可显示已知 TPM，但详情必须分别披露，不能声称完整总量。

endpoint 到 usage expectation 的映射必须由 Rust closed enum/helper 统一拥有。Task 2 固定增加 request-level `usage_status` compatibility projection，状态集合为 `in_progress`、`complete`、`missing_usage`、`stream_usage_missing`、`not_applicable`、`unknown_legacy`：request start 写 `in_progress`，统一 finalization 根据封闭 endpoint 语义、stream 标志和 token 字段写 terminal 状态；legacy migration 对可判断行回填，歧义行写 `unknown_legacy`。SQL repository 只消费该规范化状态，禁止复制 path 字符串列表。

### 4.5 耗时与首 Token

返回两个独立指标：

- `avg_total_duration_ms`：today 范围内所有 terminal request 的 `duration_ms` 算术平均；
- `avg_first_token_ms`：today 范围内所有 `first_token_ms IS NOT NULL` 请求的算术平均。

同时返回各自 `sample_count`。规则：

- `in_progress` 不进入平均耗时；
- success、failed 和 interrupted 终态都进入 `avg_total_duration_ms`，因为该值描述本地请求生命周期占用时间；
- UI 主标签改为“平均总耗时”，不再使用含混的“平均响应”；
- detail 优先显示 `TTFT {value}`，没有 TTFT 样本时显示“今日 N 个终态样本”；
- 如果产品之后需要“成功请求平均耗时”，新增显式字段，不能悄悄改变现有字段分母；
- 负数、超出 `i64` 或 lifecycle 不一致的数据必须作为 invariant/data-quality error 处理，不能参与平均。

### 4.6 请求成本

Dashboard 成本只消费 `routing_request_cost_aggregates`：

- 一个 request aggregate 只计一次；
- `totals_by_currency_json` 是 request-level `BTreeMap<currency, amount_micro>` 的持久化形式；
- today 和 lifetime 分别按 request 的 `received_at_ms` 归属窗口；
- 按 currency 返回整数 micro-unit 总额，前端只在显示边界转换为 decimal；
- 返回 `complete_single_currency`、`complete_mixed_currency`、`incomplete`、`not_applicable`、`no_attempts` 状态计数；
- mixed currency 仍按各 currency 分别累计，不压扁成默认 USD；
- incomplete aggregate 中已经有确定价格的 currency subtotal 可以展示，但必须同时暴露 incomplete request count；
- 旧 `request_logs.estimated_*` 只作为 legacy compatibility 诊断，不能与 request aggregate 相加；
- legacy row 若没有 `routing_request_cost_aggregates`，进入 `legacy_or_missing_aggregate_count`，不能默认归入 unpriced 或伪造 0 成本。
- `cost_totals_complete` 仅在 `incomplete_count == 0`、`legacy_or_missing_aggregate_count == 0`、`corrupt_cost_aggregate_count == 0` 且没有非法 currency/amount 时为 true；`not_applicable` 和 `no_attempts` 是明确零成本分类，不会单独把该标记置 false。
- 非法 currency、负数/溢出 amount 或超出 bounded JSON shape 的 row 按 corrupt aggregate 处理：跳过该金额、累计 quality count 并返回 degraded partial totals，不让整个 command 失败。

现有前端 `baseTotalCost` 对比数据不属于新 request aggregate 权威合同，当前 schema 也没有 request-level durable base-cost owner。本升级固定只展示权威 actual request cost，并移除 Dashboard 的旧 base-cost 对比；不得把 legacy estimate 称为累计 base cost。未来只有在独立升级建立 request-level durable owner 后，才能通过新版本合同恢复对比。禁止为了保留旧 UI 而扫描当前 pricing rules 反算历史成本。

## 5. 目标领域与 IPC 合同

建议 Rust domain model：

```rust
pub struct DashboardRequestMetricsInput {
    pub local_day_start_ms: i64,
    pub local_day_end_ms: i64,
}

pub struct DashboardLiveRequestMetricsSnapshot {
    pub schema_version: u16,
    pub captured_at_ms: i64,
    pub recent: DashboardRecentMetrics,
    pub today: DashboardPeriodMetrics,
    pub today_costs: DashboardCostMetrics,
    pub data_quality: DashboardLiveMetricsDataQuality,
}

pub struct DashboardCumulativeRequestMetricsSnapshot {
    pub schema_version: u16,
    pub captured_at_ms: i64,
    pub lifetime: DashboardPeriodMetrics,
    pub lifetime_costs: DashboardCostMetrics,
    pub data_quality: DashboardMetricsDataQuality,
}

pub struct DashboardRecentMetrics {
    pub period: DashboardPeriodMetrics,
    pub window_minutes: u16,
    pub rpm: f64,
    pub tpm: f64,
}

pub struct DashboardPeriodMetrics {
    pub request_count: u64,
    pub terminal_count: u64,
    pub success_count: u64,
    pub failed_count: u64,
    pub interrupted_count: u64,
    pub in_progress_count: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub known_usage_request_count: u64,
    pub missing_usage_request_count: u64,
    pub stream_usage_missing_request_count: u64,
    pub not_applicable_usage_request_count: u64,
    pub unknown_usage_request_count: u64,
    pub total_duration_ms: u64,
    pub duration_sample_count: u64,
    pub first_token_total_ms: u64,
    pub first_token_sample_count: u64,
    pub avg_total_duration_ms: Option<f64>,
    pub avg_first_token_ms: Option<f64>,
}

pub struct DashboardCostTotal {
    pub currency: String,
    pub amount_micro: i64,
    pub request_count: u64,
}

pub struct DashboardCostMetrics {
    pub totals: Vec<DashboardCostTotal>,
    pub cost_totals_complete: bool,
    pub complete_single_currency_count: u64,
    pub complete_mixed_currency_count: u64,
    pub incomplete_count: u64,
    pub not_applicable_count: u64,
    pub no_attempts_count: u64,
    pub legacy_or_missing_aggregate_count: u64,
}

pub struct DashboardMetricsDataQuality {
    pub invalid_timestamp_count: u64,
    pub future_timestamp_count: u64,
    pub invalid_duration_count: u64,
    pub unknown_lifecycle_count: u64,
    pub corrupt_cost_aggregate_count: u64,
}

pub struct DashboardLiveMetricsDataQuality {
    pub invalid_duration_count: u64,
    pub unknown_lifecycle_count: u64,
    pub corrupt_cost_aggregate_count: u64,
}
```

约束：

- repository row 使用整数 sums/counts；平均值和 rate 在 application query 中由 checked arithmetic 计算；
- DB 金额始终保留 integer micro-unit，禁止在 SQL 或 Rust domain 中先转 `f64` 再累计；
- response 中的 currency 必须规范化为后端认可的 uppercase code；未知/损坏 currency 进入 data-quality error；
- live 与 cumulative DTO 的 `schema_version` 首版分别固定为 1，为后续只增字段或显式版本升级提供边界；
- 每个 snapshot 只保证自身 read session 内一致；前端必须按卡片组原子消费各自 snapshot，不能把两个 `captured_at_ms` 拼成一个伪装的全局时点；
- live quality 只统计其有界 today/recent projection，不能为填充质量字段触发全表扫描；invalid/future timestamp 等无法归属窗口的全局质量只由 cumulative snapshot 返回；
- DTO 使用 Rust 生成式 binding，不在 `src/lib/types` 手抄同名 shape；
- 前端需要展示模型时，可以定义由生成 DTO 派生的 view model，但不能重新定义事实字段。

## 6. 持久化与查询策略

### 6.1 Canonical request timestamp

当前 `request_logs.started_at` 是 `TEXT`。下一可用 migration（计划编写时为示意 `0022`，实施前必须重新枚举）新增：

```sql
ALTER TABLE request_logs ADD COLUMN received_at_ms INTEGER;
```

迁移要求：

- 数字毫秒字符串使用严格 numeric predicate 后转换；
- ISO-8601 legacy 值使用 SQLite 可验证的 UTC conversion 回填；
- 无法转换的 legacy 值保持 `NULL`，从 recent/today/lifetime 时间范围排除，并计入 cumulative `invalid_timestamp_count`；合法整数但晚于 cumulative `captured_at_ms` 的行排除并计入 `future_timestamp_count`；单条损坏历史日志不能阻断启动，也不能被写成当前时间；
- `RequestLogStore::start_request` 在同一 INSERT 中写入 `started_at` compatibility text 和 `received_at_ms`；
- 所有新 production row 必须 `received_at_ms > 0`；
- 增加 `received_at_ms` 范围索引；
- 如 SQLite planner 证明有收益，可增加 terminal partial index，但不能凭猜测堆叠索引；
- migration fixture 覆盖 numeric text、ISO UTC、offset ISO、DST 边界和 malformed legacy value；
- portable migration catalog、schema fingerprint、fixture manifest 与 upgrade recovery 合同同步更新。

建议索引候选：

```sql
CREATE INDEX idx_request_logs_received_at
    ON request_logs(received_at_ms DESC, id DESC);

CREATE INDEX idx_request_logs_terminal_received_at
    ON request_logs(received_at_ms DESC)
    WHERE terminal_at_ms IS NOT NULL;
```

第二个索引只有 `EXPLAIN QUERY PLAN` 和目标数据集基线证明使用后才保留。

### 6.2 分 cadence、snapshot-consistent direct aggregation

首版使用 direct indexed aggregation，不建立 rollup 双写：

- `DashboardMetricsReadRepository` 接受一个现有 `ReadSession`；
- live application query 捕获一次 Clock，验证 day window，然后打开一个 read session；recent/today、今日成本和对应质量计数在该 session 内读取；
- cumulative application query 独立捕获一次 Clock并打开一个 read session；lifetime、累计成本和全局质量计数在该 session 内读取；
- 两个 snapshot 各自内部一致，但不跨 cadence 共享 transaction 或 `captured_at_ms`；
- today/recent 使用 conditional aggregation 在有界时间范围内一次完成，并只在 proxy running 且页面活跃时每 2 秒刷新；
- lifetime 聚合使用独立 SQL，页面进入时读取，proxy running 时最多每 30 秒刷新；proxy stopped 时停止定时轮询但进入页面仍读取一次；
- cost JSON 使用 SQLite `json_each` 或 Rust bounded parser，选择前必须用真实 shape fixture 比较可读性、错误隔离与性能；
- 任何 JSON 解析错误只增加 `corrupt_cost_aggregate_count`、跳过该损坏金额并设置 `cost_totals_complete = false`；command 返回其余可用指标的降级成功，不能 panic、不能泄露原始 JSON，也不能把不完整总额标成完整；
- repository 不格式化金额、日期、中文文本或 UI tone；
- query 不读取 `request_attempts` 或 `routing_attempt_costs` 来生成 Dashboard 总额。

禁止直接复用 `RecentRequestLogCache` 作为 moving-window cache。即使数据库 revision 不变，5 分钟窗口仍会随时间前移，旧 cache 会变陈旧。

### 6.3 Rollup 晋级条件

只有 Task 9 性能基线触发以下任一条件，才增加 minute/day rollup 后续 Task：

- 100,000 request rows 下 live snapshot p95 超过 50 ms，或 cumulative snapshot p95 超过 100 ms；
- 500,000 request rows 下 live snapshot p95 超过 150 ms，或 cumulative snapshot p95 超过 300 ms；
- 2 秒 live 与 30 秒 cumulative 轮询组合和 lifecycle writer 并发时出现可重复 SQLite busy、writer latency p95 回归超过 10%，或 UI query timeout；
- lifetime cost JSON 聚合成为明确主导热点。

晋级执行结果：直接聚合已触发 100k/500k p95 条件，因此本分支追加并实现 Dashboard rollup 投影。最终实现选择 `second` + `lifetime` 两类 bucket：recent/today 读取完整 second bucket 并仅对边缘毫秒范围回退 raw query；cumulative 读取 lifetime bucket，并继续用 raw count 排除 invalid/future timestamp。写路径在 request start、request terminal 和 cost aggregate 提交时同步维护 rollup；query 入口会在检测到 rollup 与 request count 不一致时执行可重建 repair。该实现没有引入不可重建 counter，也没有放宽性能门槛。

若需要 rollup，必须采用可重建投影和 dirty-range/reconciliation 模式，参考 monitoring V2 bucket owner：

- canonical request/cost facts 仍是唯一权威；
- rollup row 有 bucket start/end、projection version 和 source high-water mark；
- finalization 不允许无事务保障地 fire-and-forget 增量；
- crash/restart 后可从 canonical facts 有界重建；
- late cost aggregate 能标记对应 bucket dirty 并重算；
- direct query 保留为 qualification oracle，不作为 production fallback 双轨；
- rollup cutover 需要独立计划或在本计划追加经审阅 Task，不得在性能测试失败时临场加入 ad hoc counter。

## 7. 文件地图

最终文件名可按现有模块细调，但职责不能漂移。

| 路径 | 目标职责 |
|---|---|
| `src-tauri/src/models/dashboard_metrics.rs` | request/cost period metrics、quality、live/cumulative snapshot 纯类型 |
| `src-tauri/src/persistence/stores/dashboard_metrics_read.rs` | live/cumulative read session 内的 SQL rows 与 bounded cost parse |
| `src-tauri/src/application/queries/dashboard_metrics.rs` | 两类 query、Clock/window validation、checked projection、rate/average 计算 |
| `src-tauri/src/application/command_facades/dashboard.rs` | 窄 Dashboard query facade |
| `src-tauri/src/ipc/dto/dashboard_reads.rs` | input parse、DTO contract、binding fixture |
| `src-tauri/src/commands/dashboard.rs` | `load_dashboard_live_request_metrics` 与 `load_dashboard_cumulative_request_metrics` command adapters |
| `src/lib/query/resourceQueries.ts` | live 2 秒与 cumulative 30 秒 canonical query options |
| `src/lib/query/queryKeys.ts` | day-window-aware live key 与 versioned cumulative key |
| `src/features/dashboard/dashboardRequestMetricsViewModel.ts` | 纯显示选择、format-ready state，不做事实聚合 |
| `src/features/dashboard/DashboardPage.tsx` | 按卡片组消费 live/cumulative snapshots、proxy runtime overlay 和 recent logs |
| `src/features/dashboard/requestCostSummary.ts` | cutover 后删除或缩成 DTO-to-view formatting helper |
| `scripts/dashboard-performance-metrics.test.mjs` | 删除 brittle CSS/source 断言或改成窄 architecture anti-regression gate |
| `src-tauri/tests/dashboard_metrics_*.rs` | domain、persistence、transport、lifecycle integration、performance |

## 8. 执行纪律与依赖顺序

1. 每个 Task 开始前运行 `git status --short --branch`。当前 monitoring bucket/view model 改动属于用户，不得覆盖、格式化或纳入本升级提交。
2. migration 编号必须在 Task 2 执行时枚举后选择，本文 `0022` 只是当前快照。
3. 严格 RED-GREEN-REFACTOR：先添加能因缺失能力失败的行为测试，再实现最小完整路径。
4. 不使用 `git add .` 或 `git add -A`；每个 Task 只 stage 明确文件。
5. Task 1-5 可以先落后端且没有 production UI caller；Task 6 cutover 后必须立即执行 Task 7 删除旧前端 owner，不能长期双轨。
6. 不允许任一 Dashboard metrics command 失败后 fallback 到前端 `RequestLog[]` 聚合；失败必须按对应卡片组显示真实 error/stale state。
7. 不允许为了性能把无界内存 counter 变成第二事实源。
8. 所有测试 fixture、错误和 diagnostics 不得包含 key、cookie、Authorization、完整 upstream URL、prompt 或 response body。
9. 任一必跑命令没有退出 0，对应 Task 保持未完成；环境阻塞必须记录实际原因。

依赖图：

```text
Task 0 semantics/baseline
  -> Task 1 domain projection contract
  -> Task 2 canonical timestamp migration
     -> Task 3 persistence aggregate repository
        -> Task 4 application query
           -> Task 5 IPC/composition/generated binding
              -> Task 6 frontend query/view model
                 -> Task 7 atomic Dashboard cutover + old owner deletion
                    -> Task 8 type/contract cleanup
                       -> Task 9 performance/concurrency qualification
                          -> Task 10 docs/closeout
```

## 9. Task 0：冻结基线、语义与兼容决策

**Files:**

- Create: `docs/archive/audits/2026-08-01-dashboard-request-metrics-baseline.md`
- Modify: 本计划，仅在实施发现规范冲突时记录审阅决定
- Read only: request lifecycle、routing outcome、request log、Dashboard 和 current schema

**Steps:**

- [x] 记录 branch、commit、dirty paths、最新 migration、当前 request log retention 状态。
- [x] 记录当前 500 条 limit、所有 Dashboard request-log 聚合调用点和每个可见卡片的旧口径。
- [x] 用 fixture 证明 501 条/5 分钟时旧 RPM 被截断为 100，形成 before evidence。
- [x] 核对第 4 节已冻结语义与当前实现事实，尤其是 admitted count、terminal duration、`usage_status`、mixed currency 和 actual-only cost；发现上位规范冲突时先回到计划审阅，不能在实现中自行改口径。
- [x] 记录 base cost 当前没有 request-level durable owner，确认 UI 将移除该对比且不反算历史价格。
- [x] 记录 malformed cost JSON 固定为 degraded success：跳过损坏金额、quality count 增加、`cost_totals_complete = false`。
- [x] 确认最近使用列表仍只需要 bounded rows；请求日志页分页扩展不属于本计划 blocker。
- [x] 记录直接聚合性能目标和 rollup 晋级条件，不提前实现 rollup。

**Run:**

```powershell
git status --short --branch
Get-ChildItem src-tauri/src/persistence/migrations -File | Sort-Object Name | Select-Object -Last 5 -ExpandProperty Name
rg -n "averageDurationMs|getRecentPerformanceMetrics|summarizeDashboardRequestCosts|requestLogs.reduce|requestLogs.filter" src/features/dashboard
rg -n "PageLimit::new\(500\)|FROM request_logs|routing_request_cost_aggregates" src-tauri/src
node scripts/dashboard-performance-metrics.test.mjs
```

**Exit gate:**

- [x] 所有语义均有唯一 owner、分母、时间边界、unknown 行为和 UI label。
- [x] before evidence 可重复，且没有把 CSS 断言失败当成数据证据。
- [x] actual-only/base-cost degradation、cost corruption、legacy timestamp 和 usage status 均与本文冻结语义一致，无未决项。

**Commit:**

```powershell
git add -- docs/archive/audits/2026-08-01-dashboard-request-metrics-baseline.md docs/archive/plans/2026-08-01-dashboard-request-metrics-read-model-upgrade.md
git diff --cached --check
git commit -m "docs: freeze dashboard request metric semantics"
```

## 10. Task 1：建立纯领域聚合合同

**Files:**

- Create: `src-tauri/src/models/dashboard_metrics.rs`
- Create: `src-tauri/tests/dashboard_metrics_domain.rs`
- Modify: `src-tauri/src/models/mod.rs`

**Steps:**

- [x] 定义 input、period/recent metrics、cost totals、data quality 和 versioned live/cumulative snapshots。
- [x] 定义 repository raw rows 与 application output 的边界；SQL row 不直接作为 IPC DTO。
- [x] 用 checked conversion 处理 SQLite `i64` 到 domain `u64`，负数返回 invariant error。
- [x] 平均值由 integer sum/sample count 计算；0 sample 返回 `None`。
- [x] RPM/TPM 只存在于 `DashboardRecentMetrics`，today/lifetime 使用不含 rate 字段的 `DashboardPeriodMetrics`。
- [x] cost totals 显式携带按第 4.6 节唯一公式派生的 `cost_totals_complete`，损坏或缺失金额不能只靠可选文案表达。
- [x] currency totals 排序稳定，金额保持 micro-unit。
- [x] 对 unknown lifecycle/status 使用封闭分类器并计入 quality，不用 arbitrary string match 散落到 repository/UI。

**Tests:**

- [x] zero sample、单样本、多样本平均值。
- [x] duration 0 是合法样本，negative duration 被拒绝。
- [x] 5 分钟 501、3,000 request rate 计算。
- [x] known token 0 与 missing token 区分。
- [x] mixed currencies 稳定排序且不互相换算。
- [x] integer micro totals 不发生浮点精度损失。
- [x] unknown lifecycle 进入 quality count。
- [x] today/lifetime 类型无法构造无意义的 RPM/TPM；live/cumulative snapshot 可独立构造和版本化。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test dashboard_metrics_domain -- --nocapture
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Commit:**

```powershell
git add -- src-tauri/src/models/dashboard_metrics.rs src-tauri/src/models/mod.rs src-tauri/tests/dashboard_metrics_domain.rs
git diff --cached --check
git commit -m "feat: define dashboard request metric domain"
```

## 11. Task 2：增加 canonical request timestamp 与 migration

**Files:**

- Create: 下一可用 `src-tauri/src/persistence/migrations/NNNN_dashboard_request_metrics.sql`
- Modify: `src-tauri/src/persistence/migrations.rs` 或当前 migrator registry owner（如需要）
- Modify: `src-tauri/src/persistence/stores/request_log_store.rs`
- Modify: portable migration catalog/schema fixtures
- Create or Modify: migration/upgrade fixture tests

**Steps:**

- [x] 枚举 migration，选择下一编号并更新 schema compatibility version。
- [x] 添加 `received_at_ms`、严格 backfill 与 range index。
- [x] 增加规范化 request-level `usage_status`；start 写 `in_progress`，统一 finalization 使用 Rust closed enum/helper 写 terminal 状态，legacy 可判断行回填、歧义行写 `unknown_legacy`。
- [x] production request start 同时写 canonical integer 和 compatibility text。
- [x] request start duplicate compare-and-set 校验 canonical timestamp 一致。
- [x] migration 完成后执行 postcondition：所有可转换历史 row 已有 canonical timestamp；损坏 legacy row 保持 `NULL`、可报告且不阻塞启动。
- [x] 同步 fresh install、upgrade、portable import、schema fingerprint 和 fixture manifests。
- [x] 证明 downgrade 不在当前开发期合同内；schema 变更为 additive，代码回滚不能假装旧 binary 可安全写新库。

**Tests:**

- [x] fresh schema 创建字段和 index。
- [x] schema 21 -> next schema 升级。
- [x] numeric millisecond、ISO UTC、offset ISO 回填。
- [x] malformed timestamp 保持 `NULL`，所有窗口排除该行，cumulative quality 计数增加且应用可正常启动。
- [x] usage status 覆盖 start、non-stream complete、stream usage missing、missing、not-applicable 和 unknown legacy。
- [x] request start/finish 后 canonical timestamp、terminal timestamp、duration 一致。
- [x] upgrade recovery fault injection 不留下半迁移 active database。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_upgrade -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_upgrade_recovery -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_persistence -- --nocapture
pnpm verify:persistence-artifacts
```

**Commit:**

实施时仅 stage 实际 migration、store、catalog 和 fixture 路径，不得纳入无关 monitoring dirty files。提交前至少执行：

```powershell
git diff --cached --check
git diff --cached
git commit -m "feat: add canonical request metric timestamps"
```

## 12. Task 3：实现分 cadence、snapshot-consistent persistence read repository

**Files:**

- Create: `src-tauri/src/persistence/stores/dashboard_metrics_read.rs`
- Modify: `src-tauri/src/persistence/stores/mod.rs`
- Create: `src-tauri/tests/dashboard_metrics_persistence.rs`

**Steps:**

- [x] repository API 必须接受 `&mut ReadSession`，不能自行打开多个连接。
- [x] 实现 live recent/today conditional aggregate，完全移除 row limit，且不触碰 lifetime 全量扫描。
- [x] 实现独立 cumulative lifetime aggregate，不返回宽 request rows。
- [x] 明确 lifecycle status 到 terminal/success/failed/interrupted/in-progress 的映射。
- [x] usage known/missing/not-applicable 分类只使用 Task 2 的 canonical projection/helper。
- [x] 成本从 `routing_request_cost_aggregates` 以 request_id join 一次；禁止 join attempts 后累计。
- [x] bounded 解析 `totals_by_currency_json`，限制 currency 数、key/value 类型和金额范围；单行损坏时跳过金额、增加 quality count 并将 totals 标记为不完整。
- [x] 返回 raw integer sums/counts 与 quality rows，不计算 UI 字符串。
- [x] 用 `EXPLAIN QUERY PLAN` fixture 锁定 recent/today query 使用 canonical timestamp index。

**Tests:**

- [x] 0、1、500、501、3,000 recent rows，无 limit 截断。
- [x] recent/day start inclusive、captured/day end exclusive；future timestamp 从事实范围排除并只进入 cumulative quality。
- [x] today 与 recent 重叠但各自计数正确。
- [x] in-progress 只进入 admitted/RPM，不进入 duration 和 usage missing。
- [x] failed/interrupted terminal duration 进入固定口径。
- [x] no-usage endpoint 归 not-applicable，unknown legacy 独立计数。
- [x] missing usage、stream usage missing、unknown legacy 与合法 0 tokens 区分。
- [x] fallback 2 attempts 仍只计 1 request。
- [x] mixed currency request 各 currency 各计一次 subtotal，request status 只计一次。
- [x] incomplete、not-applicable、no-attempts 和缺 aggregate 分类正确。
- [x] malformed JSON、非法 currency/amount 不 panic、不泄露原值，quality count 与 `cost_totals_complete = false` 正确。
- [x] live 和 cumulative 各自在同一 read session 中于并发写入期间保持内部一致；测试不要求两个 snapshot 的捕获时间或总数相等。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test dashboard_metrics_persistence -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_outcome_persistence -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_sessions -- --nocapture
```

**Commit:**

```powershell
git add -- src-tauri/src/persistence/stores/dashboard_metrics_read.rs src-tauri/src/persistence/stores/mod.rs src-tauri/tests/dashboard_metrics_persistence.rs
git diff --cached --check
git commit -m "feat: aggregate dashboard request facts"
```

## 13. Task 4：实现 live/cumulative application queries 与窗口验证

**Files:**

- Create: `src-tauri/src/application/queries/dashboard_metrics.rs`
- Modify: `src-tauri/src/application/queries/mod.rs`
- Create: `src-tauri/tests/dashboard_metrics_query.rs`

**Steps:**

- [x] `DashboardMetricsQuery` 注入 `PersistenceHandle` 与 `Arc<dyn Clock>`，公开 `load_live(input)` 与 `load_cumulative()` 两个窄方法。
- [x] live query 捕获一次 `captured_at_ms`，按 `day_start <= captured < day_end` 验证 day boundaries 和 recent 固定窗口。
- [x] 两个方法各自打开一个 read session 并只读取所属 projection；cumulative 不需要 day-window input。
- [x] checked 计算 averages、RPM、TPM 和所属 quality summary；只有 recent type 计算 rate。
- [x] 对 overflow、非法窗口和 persistence error 使用稳定 `ApplicationError` code，不把 SQL/JSON 原文暴露到前端；corrupt cost row 按冻结语义返回 degraded success。
- [x] 两类 snapshot `schema_version = 1`，各自携带独立 `captured_at_ms`。
- [x] 不读取 proxy runtime active counter；该字段继续属于 runtime overlay。
- [x] 不实现基于 persistence revision 的无限期 moving-window cache。

**Tests:**

- [x] 23/24/25 小时 local day 合法，22 小时以下和 26 小时以上拒绝。
- [x] captured time 等于 day start 合法，等于 day end 或在 window 外拒绝；today 截止 captured time，不读取当日未来 row。
- [x] Clock 在 query 中只捕获一次。
- [x] 501/5 分钟返回 100.2 RPM，而不是 100。
- [x] 3,000/5 分钟返回 600 RPM。
- [x] live/cumulative query output 分别与 repository raw totals 一致，且 cumulative 无 day-window dependency。
- [x] corrupt cost row 返回 `cost_totals_complete = false` 和 quality count，不返回 command error。
- [x] persistence error 映射稳定且脱敏。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test dashboard_metrics_query -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml application::queries::dashboard_metrics -- --nocapture
```

**Commit:**

```powershell
git add -- src-tauri/src/application/queries/dashboard_metrics.rs src-tauri/src/application/queries/mod.rs src-tauri/tests/dashboard_metrics_query.rs
git diff --cached --check
git commit -m "feat: expose dashboard metric snapshot queries"
```

## 14. Task 5：接入窄 facade、command、ACL 与生成式 binding

**Files:**

- Create: `src-tauri/src/application/command_facades/dashboard.rs`
- Modify: `src-tauri/src/application/command_facades/mod.rs`
- Create: `src-tauri/src/commands/dashboard.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Create: `src-tauri/src/ipc/dto/dashboard_reads.rs`
- Create: `src-tauri/src/ipc/dto/dashboard_reads.typescript.txt`
- Modify: `src-tauri/src/ipc/dto/mod.rs`
- Modify: `src-tauri/src/app_composition.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify generated: `src/lib/bridge/generated.ts`
- Modify: `src/lib/bridge/BackendClient.ts`
- Modify: `src/lib/bridge/DesktopBackend.ts`
- Modify: `src/lib/bridge/DemoBackend.ts`

**Steps:**

- [x] 定义 live `DashboardRequestMetricsInputDto`，`deny_unknown_fields` 并执行类型/范围验证；cumulative command 无伪造的 day-window input。
- [x] 定义 live/cumulative output DTO 或使用可生成的权威 domain types，字段使用 camelCase binding。
- [x] 注册 `load_dashboard_live_request_metrics` 与 `load_dashboard_cumulative_request_metrics`；command 只 parse -> facade -> error map，不含 SQL、Clock 或业务计算。
- [x] facade 只持有 `Arc<DashboardMetricsQuery>` 并暴露两个窄入口，不能注入整个 AppServices。
- [x] composition root 显式构造 query/facade；command ACL 只允许 main window。
- [x] 生成 TypeScript binding 和 fixture，禁止手工编辑生成字段。
- [x] BackendClient 增加窄 `dashboard.loadLiveRequestMetrics(input)` 与 `dashboard.loadCumulativeRequestMetrics()` domain clients；不要把完整 BackendClient 注入 Dashboard feature helper。
- [x] DemoBackend 对两个方法明确 unsupported，不从 mock logs 静默生成 production snapshot。
- [x] 更新 command registry/hash/fixture gates。

**Tests:**

- [x] live valid/unknown/missing/invalid window input 与 cumulative no-input transport fixtures。
- [x] 两类 output fixtures 覆盖 mixed currency、missing usage、null averages、cost completeness 和 quality counts。
- [x] command registry、ACL、generated fixture 一致。
- [x] DesktopBackend 分别调用两个精确 command 和 payload，cumulative 不发送无意义窗口。
- [x] public error 不包含 SQL、JSON 原文或本地路径。

**Run:**

```powershell
pnpm generate:bindings
pnpm architecture:commands
pnpm architecture:typescript
pnpm exec vitest run src/lib/bridge/generated.test.ts
cargo test --locked --manifest-path src-tauri/Cargo.toml ipc::dto::dashboard_reads -- --nocapture
cargo check --locked --manifest-path src-tauri/Cargo.toml
```

**Commit:**

只 stage Task 5 列出的 facade、command、DTO、composition、registry 和 generated binding 路径，并在提交前检查生成 diff：

```powershell
git diff --cached --check
git diff --cached
git commit -m "feat: add dashboard metric IPC read model"
```

## 15. Task 6：建立前端 Query owner 与纯显示模型

**Files:**

- Modify: `src/lib/query/queryKeys.ts`
- Modify: `src/lib/query/resourceQueries.ts`
- Create: `src/lib/api/dashboard.ts`（如现有 bridge boundary 需要）
- Create: `src/features/dashboard/dashboardRequestMetricsViewModel.ts`
- Create: `src/features/dashboard/dashboardRequestMetricsViewModel.test.ts`
- Modify: `src/lib/queries/dashboardQueries.ts` 或删除其中 legacy composite owner

**Steps:**

- [x] 建立两个 query owner：live key 包含 schema version 与 local day start/end；cumulative key 包含独立 schema version，不能错误携带 day window。
- [x] live queryFn 调用时使用前端本地 Date 计算自然日边界；后端仍验证。cumulative queryFn 无窗口输入。
- [x] 用可测试的 local-day rollover hook/timer 在下一个本地午夜更新 live key；组件重新渲染或 2 秒 polling 不能作为跨日正确性的隐式依赖，系统休眠唤醒后也要重新计算边界。
- [x] live 在 proxy running 时 2 秒 refetch，cumulative 在进入页面时读取并仅在 running 时 30 秒 refetch；proxy stopped 时两者停止定时轮询。
- [x] 两个 query 分别设置 `staleTime` 小于等于各自 refetch interval，明确 placeholder/previous data 行为，不让 cumulative 的慢刷新拖慢 live 卡片。
- [x] view model 只选择字段、生成 sample/quality display state 和格式化输入，不重新 reduce request logs。
- [x] live 与 cumulative 分别建模 error、pending、stale-success、quality-warning，一个失败不能清空另一个最后成功的卡片组。
- [x] active requests 仍来自 `proxyStatus`，由页面组合，不复制到 durable metrics cache。
- [x] query hidden-page 行为继续受 `useActivityQuery` 控制；AppShell proxy status 全局 polling 不扩散到 metrics query。

**Tests:**

- [x] live query key 在跨午夜边界变化，cumulative key 保持不变但进入页面可刷新。
- [x] DST 23/25 小时边界生成正确。
- [x] 页面持续挂载跨午夜、系统跨午夜休眠后唤醒，均切换到新 local-day key 并取消旧 timer。
- [x] proxy stopped 不定时 refetch；running 时 live 按 2 秒、cumulative 按 30 秒刷新。
- [x] missing usage 产生 quality detail，不把 TPM 隐藏或伪装完整。
- [x] null duration/TTFT 显示无样本，不显示 0 ms。
- [x] mixed currencies 不相加。
- [x] 任一 stale snapshot 保留所属卡片组最后成功值并显示 stale/error 状态，不回退 request logs，也不覆盖另一 snapshot。

**Run:**

```powershell
pnpm exec vitest run src/features/dashboard/dashboardRequestMetricsViewModel.test.ts
pnpm exec tsc --noEmit
```

**Commit:**

```powershell
git add -- src/lib/query/queryKeys.ts src/lib/query/resourceQueries.ts src/lib/api/dashboard.ts src/features/dashboard/dashboardRequestMetricsViewModel.ts src/features/dashboard/dashboardRequestMetricsViewModel.test.ts src/lib/queries/dashboardQueries.ts
git diff --cached --check
git commit -m "feat: add dashboard metric query owner"
```

不存在的可选路径不得为了匹配上述命令而创建空文件；按 Task 实际采用的现有 bridge boundary 调整明确 path list。

## 16. Task 7：原子切换 Dashboard 并删除旧指标 owner

**Files:**

- Modify: `src/features/dashboard/DashboardPage.tsx`
- Modify or Delete: `src/features/dashboard/requestCostSummary.ts`
- Modify: Dashboard 相关 Vitest/contract tests
- Preserve: recent usage list 对 bounded request logs 的消费

**Steps:**

- [x] 新增 live/cumulative metrics queries，卡片只消费对应 snapshot；每个卡片组在一次 render 中读取同一 query result，不能逐字段混用新旧 snapshot。
- [x] “今日请求”读取 live `today.requestCount`，累计详情读取 cumulative `lifetime.requestCount`，允许两者 `capturedAtMs` 不同。
- [x] 今日 input/output/total Token 读取 live snapshot，累计 Token 读取 cumulative snapshot。
- [x] “平均响应”改为“平均总耗时”，主值读取 `avgTotalDurationMs`，detail 显示 TTFT 或 sample count。
- [x] “性能概览”读取 live backend RPM/TPM，并组合 runtime `activeRequests`。
- [x] 今日成本读取 live、累计成本读取 cumulative request aggregate currency totals 和 quality/completeness；不完整时显示明确警告，不把 partial total 标成完整。
- [x] 删除没有 durable owner 的 base-cost comparison，不保留 legacy estimate 冒充累计 base cost。
- [x] 最近使用继续使用 `requestLogs.slice(0, 5)`，但不得影响任何指标卡。
- [x] 删除 `todayLogs` 指标用途、`averageDurationMs`、`getRecentPerformanceMetrics`、request token reductions 和 request cost log scan。
- [x] 如果 `todayLogs` 仅剩 recent UI 无 caller，完全删除。
- [x] live/cumulative 未加载分别显示所属卡片 skeleton/placeholder；单方失败只影响对应组并显示错误状态和刷新入口，不能显示具有确定含义的 0。
- [x] 保持桌面工具现有密度、色彩和卡片尺寸，不在本任务顺带改版。
- [x] Task 6/7 作为一个用户可见 cutover candidate；不能提交一个同时显示新旧冲突数字的中间版本。

**Tests:**

- [x] Dashboard 501/5min fixture 显示 100.2 RPM。
- [x] recent list 只有 5 行，但累计指标仍来自完整 snapshot。
- [x] metrics query error 不影响最近使用列表。
- [x] request logs error 不清空最后成功 metrics。
- [x] active request overlay 更新不触发 request-log aggregation。
- [x] live query 失败时 cumulative 卡片保留最后成功值，反向亦然；两个 snapshot 不被 merge 成伪原子对象。
- [x] 文案准确区分平均总耗时与 TTFT。
- [x] CSS/theme 测试只检查语义 token/结构，不硬编码无关颜色 class。

**Run:**

```powershell
pnpm exec vitest run src/features/dashboard
node scripts/dashboard-performance-metrics.test.mjs
node scripts/dashboard-request-count-source.test.mjs
node scripts/dashboard-query-service.test.mjs
pnpm exec tsc --noEmit
pnpm build
```

**Commit:**

```powershell
git add -- src/features/dashboard/DashboardPage.tsx src/features/dashboard/requestCostSummary.ts
git add -p -- scripts/dashboard-performance-metrics.test.mjs scripts/dashboard-request-count-source.test.mjs scripts/dashboard-query-service.test.mjs scripts/dashboard-token-value-color.test.mjs
git diff --cached --check
git commit -m "feat: cut dashboard metrics to backend facts"
```

如果 `requestCostSummary.ts` 在 Task 7 被删除，使用 `git add -- src/features/dashboard/requestCostSummary.ts` 记录删除；不得用全仓 stage。

## 17. Task 8：清理类型漂移与旧合约测试

**Files:**

- Modify: `src/lib/types/proxy.ts`
- Modify: `src/lib/bridge/DesktopBackend.ts`
- Modify/Delete: `scripts/dashboard-performance-metrics.test.mjs`
- Modify/Delete: `scripts/dashboard-request-count-source.test.mjs`
- Modify/Delete: `scripts/dashboard-query-service.test.mjs`
- Modify: `scripts/dashboard-token-value-color.test.mjs`
- Modify: contract runner only if tests are renamed/replaced

**Steps:**

- [x] `RequestLog` transport 字段与 generated `RequestLogDto` 不再漂移；优先显式 mapper + compile-time `satisfies`，或直接复用生成 DTO。
- [x] 如果保留 domain `RequestLog`，把 `lifecycleStatus` 纳入并用 exhaustive mapping 测试锁定。
- [x] 删除要求 Dashboard 从 request logs 计算 RPM/TPM 的旧断言。
- [x] 删除匹配 `text-slate-900` 等无关 CSS 字符串的指标正确性断言。
- [x] 新 architecture gate 只禁止 `DashboardPage` 出现旧 aggregation owner，并要求 metrics query 存在；正确性由 Rust/Vitest behavior tests 负责。
- [x] `listRequestLogs` 继续保留给 LogsPage/recent usage，不能因指标 cutover 误删。
- [x] `loadDashboardWorkspace` 若已无 caller 则删除；不能保留 dead composite query service。

**Run:**

```powershell
pnpm test:contracts
pnpm exec vitest run src/lib/api/proxy.test.ts src/lib/bridge/generated.test.ts
pnpm lint
pnpm architecture:typescript
```

**Commit:**

```powershell
git add -- src/lib/types/proxy.ts src/lib/bridge/DesktopBackend.ts
git add -p -- scripts/dashboard-performance-metrics.test.mjs scripts/dashboard-request-count-source.test.mjs scripts/dashboard-query-service.test.mjs scripts/dashboard-token-value-color.test.mjs scripts/run-contract-tests.mjs
git diff --cached --check
git commit -m "test: lock dashboard metric ownership"
```

## 18. Task 9：性能、并发、故障与端到端资格

**Files:**

- Create: `src-tauri/tests/dashboard_metrics_performance.rs`
- Create: `src-tauri/tests/dashboard_metrics_integration.rs`
- Create or Modify: ignored/local performance harness artifact path under `output/`
- Modify: verification entrypoint only if the new deterministic target becomes a standard fast gate

**Performance dataset:**

- 10,000 rows：常规开发数据库；
- 100,000 rows：主性能门槛；
- 500,000 rows：扩展观察，不要求每次 PR 都运行；
- success/failed/interrupted/in-progress 混合；
- 0/1/2 attempts 混合，但 request metric 仍一行一请求；
- known/missing/not-applicable usage 混合；
- single/mixed/incomplete cost aggregate 混合；
- today/recent/lifetime 分布可重复，使用固定 seed。

**Steps:**

- [x] 测量 cold/warm p50/p95，不只记录单次最快值。
- [x] 记录 `EXPLAIN QUERY PLAN`，证明 recent/today 范围使用目标 index。
- [x] 并发运行 lifecycle writer、2 秒 live reads 和 30 秒 cumulative reads，分别测 writer/live/cumulative latency 与 SQLite busy。
- [x] proxy restart 后 durable RPM/today totals 不清零，active runtime counter 正确归零。
- [x] request terminal 已提交但 cost aggregate 尚未提交时，对应 snapshot 显示 missing/incomplete；cost 到达后 live 在下一次 live refresh、cumulative 在下一次 cumulative refresh 各自收敛。
- [x] lifecycle reconciliation 把启动中断 request 终结后，metrics 口径符合 interrupted 规则。
- [x] clear request logs 后 live/cumulative 在各自下一 snapshot 归零，关联 cost rows 级联删除且清空 Dashboard rollup 投影。
- [x] malformed row、database busy、read cancellation 和 app shutdown 不产生 panic、secret 或永久后台任务。
- [x] 达到第 6.3 节任一条件时停止 cutover qualification，提交性能证据并进入 rollup 设计审阅；不得直接放宽门槛。

**Run:**

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --test dashboard_metrics_integration -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test dashboard_metrics_performance --release -- --ignored --nocapture --test-threads=1
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_lifecycle_reconciliation -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_outcome_persistence -- --nocapture
```

**Commit:**

```powershell
git add -- src-tauri/tests/dashboard_metrics_performance.rs src-tauri/tests/dashboard_metrics_integration.rs
git diff --cached --check
git commit -m "test: qualify dashboard metric aggregation"
```

ignored `output/` 性能 artifact 不得 stage。

**Performance gate:**

- [x] 100,000 rows live warm p95 <= 50 ms、cumulative warm p95 <= 100 ms：rollup probe 实测 live 0.302 ms、cumulative 0.076 ms。
- [x] 500,000 rows live warm p95 <= 150 ms、cumulative warm p95 <= 300 ms：rollup probe 实测 live 0.734 ms、cumulative 0.115 ms。
- [x] 并发 read 下 writer p95 相对无 Dashboard read 基线回归 <= 10%：500k 实测 6.156%，100k 为 -38.349%。
- [x] 无可重复 SQLite busy、无 unbounded allocation、无 500-row transfer dependency：100k/500k writer/read busy 均为 0，Dashboard 指标不再传输 500 条宽日志计算。

## 19. Task 10：文档、全量自检与关闭旧 owner

**Files:**

- Modify: `docs/PROJECT_PLAN.md`
- Modify: `docs/README.md` 或 Dashboard/architecture current entry（如需要）
- Create: `docs/audits/2026-08-01-dashboard-request-metrics-qualification.md`
- Modify: 本计划状态与完成证据

**Steps:**

- [x] 更新 Dashboard 信息架构：request metrics 来自后端 aggregate read model，request logs 只负责明细和最近使用。
- [x] 记录指标口径、窗口、usage coverage、cost currency 和 active overlay。
- [x] 记录 migration、reset/reimport 策略和未运行的真实环境观察。
- [x] 搜索旧 owner、旧 label、旧 500-row metric assumptions 和 dead query service。
- [x] 运行全量开发期 fast/full 自检；任何已有无关红项必须写明 owner 和证据，不能假装通过。
- [x] 更新本计划状态为完成时，列出真实命令、revision 和性能结果。

**Run:**

```powershell
rg -n "averageDurationMs|getRecentPerformanceMetrics|summarizeDashboardRequestCosts\(requestLogs|proxyRequestCount = Math.max|requestLogs\.reduce" src scripts
pnpm exec tsc --noEmit
pnpm lint
pnpm test
pnpm test:contracts
pnpm build
pnpm architecture:fixtures
pnpm architecture:typescript
pnpm architecture:commands
pnpm architecture:security
pnpm architecture:artifacts
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml
git diff --check
```

**Commit:**

```powershell
git add -- docs/PROJECT_PLAN.md docs/README.md docs/audits/2026-08-01-dashboard-request-metrics-qualification.md docs/archive/plans/2026-08-01-dashboard-request-metrics-read-model-upgrade.md
git diff --cached --check
git commit -m "docs: close dashboard metric read model upgrade"
```

`docs/README.md` 若实施后无需更新则不得为凑路径制造无意义 diff。

## 20. 测试覆盖矩阵

| 风险 | 最低自动化证据 |
|---|---|
| 500 条截断 | 501 和 3,000 recent rows persistence + UI fixture |
| 时间窗口 off-by-one | start inclusive/end exclusive tests |
| 本地午夜/DST | 23/24/25 小时 input + frontend key rollover tests |
| in-progress 污染平均值 | admitted count 增加、duration sample 不增加 |
| missing usage 伪装 0 | known zero、missing、not-applicable 三态 tests |
| streaming 语义混淆 | duration 与 first-token 独立 sample tests |
| fallback double count | 2 attempts 只产生 1 request count/cost aggregate |
| mixed currency 压扁 | USD/CNY 独立 integer micro totals |
| attempt/request cost double count | repository source/SQL + behavior test |
| late cost arrival | live/cumulative 各自在后续 refresh 从 missing 收敛到 complete |
| restart 清零错误 | durable metrics 保留、runtime active 清零 |
| timestamp migration | numeric/ISO/offset/malformed fixtures；malformed 为 NULL、排除并计数但不阻塞启动 |
| cost JSON 损坏 | partial totals + `cost_totals_complete = false` + quality count，command 仍成功且不泄露原文 |
| cadence 隔离 | live 2 秒不执行 lifetime 扫描，cumulative 30 秒独立刷新且单方失败隔离 |
| DTO 漂移 | generated binding fixture + exhaustive mapper test |
| query owner 双轨 | architecture gate 禁止 Dashboard log aggregation |
| hidden page polling | useActivityQuery page visibility regression |
| SQL 性能 | 100k/500k p95 + query plan |
| writer/read 竞争 | concurrent lifecycle write/read qualification |
| 错误泄密 | public error/redaction fixtures |

## 21. 可观测性与诊断

新增 bounded、低基数 diagnostics：

- command/query duration，按 live/cumulative 低基数标签区分；
- snapshot row scope 或 dataset size bucket，不记录 request_id/model/key；
- data-quality counts；
- direct aggregation timeout/busy/error code；
- performance harness 中的 query plan hash 和 schema version。

禁止记录：

- request payload、response body、prompt；
- API key、Cookie、Authorization；
- 完整 upstream URL；
- request_id 列表或 model 高频标签；
- `totals_by_currency_json` 原文或损坏 JSON 原文；
- 任意可形成无界 cardinality 的用户字段。

## 22. 迁移、切换与回滚

### 22.1 开发期迁移

- schema migration 为 additive canonical timestamp/read index；
- 升级失败必须保持旧 active database 未切换或进入现有 recovery contract；
- 本项目当前非稳定阶段，结构性损坏允许 reset/reimport/重新配置，不实现 schema-specific startup repair loop；
- portable import 在激活临时副本前完成 schema upgrade 和 postcondition；
- export/import 保留 request logs 与 request cost aggregates，但继续遵守敏感字段策略。

### 22.2 UI cutover

- 后端 command/binding 可以先合并但不被 UI 调用；
- Task 6/7 形成单一用户可见 cutover；
- cutover 后 Dashboard 不保留 `invoke.catch -> requestLogs aggregation`；
- 请求日志页和最近使用列表继续工作，因此 `list_request_logs` 不删除；
- 若任一新 metrics command 失败，只在对应卡片组显示错误/最后成功 stale snapshot，由用户刷新或修复数据；不显示旧近似值，也不清空另一组成功 snapshot。

### 22.3 代码回滚

- 在开发期 rollback 只能回到一个完整 owner；不能恢复新 DTO + 旧前端聚合的混合状态；
- schema 已升级后不承诺旧 binary 兼容写入；需要时使用当前开发数据 reset/reimport 策略；
- rollup 未达到晋级条件前不存在 rollup rollback 问题；
- 不把“扩大 list limit”作为临时回滚，它仍然不能保证正确且会扩大 IPC/内存成本。

## 23. 安全与隐私复核

- 新 DTO 只包含聚合数字、currency code、时间边界和质量计数；
- 不返回 request_id、station_id、station_key_id、model、path、URL 或错误正文；
- SQL/JSON 解析错误通过稳定 code 和脱敏 summary 暴露；
- fixture 只使用虚构 request/currency，不包含真实 endpoint 或 key；
- performance artifact 位于 ignored `output/`，且只保存聚合 timings/schema/query-plan evidence；
- command ACL 只允许 main window，capture/preview window 不获得该 command；
- live `DashboardRequestMetricsInputDto` 拒绝超大窗口、未知字段和整数溢出；cumulative command 不接受窗口 payload。

## 24. 最终验收清单

### 正确性

- [x] 501/5min 不再显示固定 100 RPM。
- [x] 今日、累计 request/token/cost 不受 500 条限制。
- [x] average total duration 与 TTFT 分离。
- [x] missing usage 和 known zero 分离。
- [x] stream usage missing 与 unknown legacy usage 独立可见，不污染 known/not-applicable。
- [x] fallback attempts 不重复计数或计费。
- [x] mixed currency 不换算、不压扁。
- [x] cost JSON 损坏返回 partial + completeness warning，而不是失败、泄密或伪装完整。
- [x] malformed legacy timestamp 不阻断启动、不伪造当前时间，并进入全局质量计数。

### 架构

- [x] request lifecycle/request cost aggregate 是唯一 durable facts。
- [x] Dashboard 使用后端 live/cumulative aggregate read models；2 秒 live 路径不执行 lifetime 扫描。
- [x] request log page/list 只承担明细读取。
- [x] runtime active requests 保持独立 overlay。
- [x] 没有前端 fallback aggregation、双写 counter 或无界 rollup。
- [x] Rust DTO -> generated TS -> BackendClient -> Query owner 边界完整。

### 可靠性

- [x] live/cumulative 各自以同一 read session/same captured time 保证内部一致，跨 snapshot 不伪装全局原子性。
- [x] restart、interrupted reconciliation、late cost、clear logs 行为有测试。
- [x] migration/recovery/portable import postcondition 通过。
- [x] query error 不伪装 0，不泄露敏感数据。

### 性能

- [x] recent/today query 使用 canonical timestamp index。
- [x] 100k/500k 性能门槛有真实证据。
- [x] 并发 read 不造成可重复 writer busy 或 >10% p95 回归。
- [x] Dashboard 不再每 2 秒传输 500 条宽日志来计算指标。
- [x] lifetime/cumulative 全量查询不以 2 秒 cadence 执行，单方 query 失败不拖垮另一卡片组。

### 交付

- [x] TypeScript/Vite、Rust、binding、architecture、contract 检查退出 0。
- [x] qualification audit 记录 revision、命令和结果。
- [x] docs/PROJECT_PLAN 与当前实现术语一致。
- [x] 未运行的真实环境观察明确标记，不冒充通过。
- [x] `git diff --check` 退出 0，只 stage 本计划范围路径。

## 25. 明确禁止的修复方式

- 把 `PageLimit::new(500)` 改成 5,000、50,000 或“全部”。
- 在前端继续 `filter/reduce`，只增加更多 defensive checks。
- 用 runtime `request_count` 代替 durable RPM/today/lifetime count。
- 把 null token 当 0 且不返回 coverage。
- 把 `first_token_ms` 和 `duration_ms` 混成一个“响应时间”。
- 从 request attempts 直接累计请求成本，或同时累计 attempt/request totals。
- 用当前 pricing rule 反算历史成本。
- 为每个请求写第二张 Dashboard counter 表但没有事务/idempotency/rebuild 合同。
- moving-window cache 只按 persistence revision 缓存而不考虑时间前移。
- command 失败时回退旧 RequestLog 前端聚合。
- 用 CSS class/source regex 测试宣称数据正确。
- 在本任务中顺带重做 Dashboard 视觉、引入图表库或云 telemetry。
- 停止或覆盖用户当前 monitoring 工作区改动来完成本计划。
