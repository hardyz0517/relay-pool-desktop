# Station 余额作用域、权威性与聚合可靠性修正计划

状态：已实施；自动化代码与契约验证已完成，真实 KedayA 账号验收未执行，不能视为发布批准

日期：2026-09-05

适用范围：Sub2API / NewAPI 余额采集、余额事实契约、SQLite 持久化、当前余额投影、Station Asset/Detail、Dashboard、智能路由、零余额监控、生成 IPC 绑定与升级兼容。

实施收口：本轮已完成 typed balance kind、Sub2API/NewAPI direct account 事实保留、Key quota 与账号余额分离、legacy aggregate 排除、operational-facts/routing authority-freshness 约束、Dashboard/Station current projection 与静态门禁。已运行的命令和未完成的真实账号验收见 [`../audits/2026-09-05-station-balance-scope-authority-qualification.md`](../audits/2026-09-05-station-balance-scope-authority-qualification.md)。

关联入口：

- [`../README.md`](../README.md)
- [`../PRODUCT_MODEL.md`](../PRODUCT_MODEL.md)
- [`../PRICING_MULTIPLIER_MODEL.md`](../PRICING_MULTIPLIER_MODEL.md)
- [`../specs/INTELLIGENT_ROUTING_ENGINE_SPEC.md`](../specs/INTELLIGENT_ROUTING_ENGINE_SPEC.md)
- [`2026-08-25-monitoring-spendability-and-availability-sample-upgrade.md`](2026-08-25-monitoring-spendability-and-availability-sample-upgrade.md)
- [`2026-09-02-station-collection-authorization-state-reliability.md`](2026-09-02-station-collection-authorization-state-reliability.md)
- [`../research/SUB2API_SOURCE_AUDIT.md`](../research/SUB2API_SOURCE_AUDIT.md)
- 当前代码、生成 IPC 契约、数据库 schema 与自动化门禁

本文是一次专项修正计划，不是长期产品规范。实施完成后，应把稳定语义回写到 `PRODUCT_MODEL.md`，把验证证据写入独立 audit，并在本文顶部标注最终状态；不得长期依赖本文替代当前代码和长期规范。

## 1. 结论

本问题不是格式化或前端卡片计算错误，而是采集与投影边界把多把 Key 观察到的额度无条件相加，制造了不存在的站点账户余额。

现场现象为：同一 Sub2API 账户下两把 Key 都报告约 `2.8`，当前实现生成 `2.8 + 2.8 = 5.6` 的 `station_key_balance_aggregate`，Station Detail 随后把该派生行当成当前站点余额展示。相同错误值还可能进入 Dashboard 总余额与路由余额判断；零余额监控已经读取 `spendability_authority`，但其他消费者没有遵守同一权威性规则，因此系统内部存在跨页面、路由和监控不一致。

目标修复不是把 `sum` 改成 `max`、`min`、平均值或“相同数值去重”，而是建立以下可证明链路：

```text
provider response
  -> typed balance component + explicit scope/authority
  -> validated canonical fact
  -> append-only persistence
  -> one backend-owned current-balance selector
  -> purpose-specific projections
       -> station account balance display
       -> per-key routing spendability
       -> dashboard station totals
       -> zero-balance monitoring
```

核心规则固定为：

1. 账户余额属于 Station Account，一次账户只计算一次。
2. Key 额度属于 Station Key，不得默认汇总成账户余额。
3. 订阅额度、账户余额、Key 额度和使用量是不同经济事实，不得仅因单位相同就相加。
4. 只有明确的、类型化的 provider 契约可以声明某组组件可加；当前支持的 Sub2API 和 NewAPI 都不需要从多把 Key 推导账户余额。
5. 拿不到权威账户余额时显示 unknown/stale，而不是从 Key 数量猜总额。

## 2. 已确认的基线与因果链

### 2.1 当前生产路径

| 位置/符号 | 当前行为 | 问题 |
| --- | --- | --- |
| `services/collectors/drivers/sub2api/mod.rs::collect_balance` | 对 Station 下每把可用 Key 调用 `/v1/usage` | 这是合理的 per-key 观察，但不能证明这些值是相互独立的钱包 |
| `drivers/sub2api/mapping.rs::parse_usage_balance` | 把每次响应标记为 `scope = station_key` | 可作为当前 Key 的可用额度证据；不能直接成为站点账户余额 |
| `mapping.rs::merge_account_profile_balance` | 已有 Key 事实时，只把账号资料中的并发限制复制到 Key 事实 | 即使 `/auth/me` 或 `/user/profile` 返回直接账户余额，也会丢失该权威值 |
| `mapping.rs::merge_dashboard_usage_stats` | 没有 station 行时，先求和全部 Key 余额，再附加账号级 usage stats | 账号级统计被挂到错误的派生金额上 |
| `mapping.rs::merge_subscription_quota` | 没有 station 行时，求和全部 Key 余额后再叠加订阅额度 | 同时混淆 Key 额度、账户余额和订阅额度 |
| `collector_apply.rs::append_station_balance_aggregates` | 所有 adapter 输出只要有 Key 余额且没有 station 余额，就自动求和生成 station 行 | provider 无关的 apply 层在没有契约的情况下发明业务语义 |
| `application/collectors.rs::collector_balance_authority` | 仅根据 `source == station_key_balance_aggregate` 把行标记为 advisory | 权威性由字符串猜测，driver 无法显式表达，新增 source 容易漏判 |
| `PricingStore::latest_station_balances` | 按 `station_id + scope` 取最新 station 行 | 未筛选 authority、kind 或 validity，advisory 聚合行可成为 Dashboard/Station 当前值 |
| `routing_store.rs` 与 operational-facts 查询 | 在 station/key 行中选余额 | 部分路径不读取 authority/freshness；`project_runtime_balance` 又把输入视为 authoritative/fresh |
| `src/lib/projections/balanceFacts.ts` | 取每站最新 `scope = station` 行 | 前端无法区分直接账户值与错误派生值 |

### 2.2 为什么恰好翻倍

当前算法按 Key 行求和，而不是按账户身份去重：

```text
Station KedayA
  Key A -> /v1/usage -> 2.8
  Key B -> /v1/usage -> 2.8
  generic aggregate    -> 5.6
```

对于共享账户余额、共享订阅或 provider 返回同一账户视图的场景，Key 数量会成为错误倍率。增加、删除、启用或停用一把 Key 都可能改变显示余额，即使真实账户没有发生任何资金变化。

### 2.3 风险不只在 UI

| 消费者 | 当前潜在影响 | 目标行为 |
| --- | --- | --- |
| Station Detail | 展示翻倍金额，并标注为 Key 余额汇总 | 只展示权威账户余额；Key 额度单独解释 |
| Station 列表 | 风险标签和余额筛选基于错误 current row | 使用同一 current account projection |
| Dashboard | 多站总余额叠加已经翻倍的站点金额 | 每个 Station 最多贡献一个权威账户余额 |
| Routing | 错误认为账户仍有更多余额，或不同读取路径得出不同资格 | Key 事实优先用于对应 Key；站点事实仅作账户级 fallback |
| Monitoring | 目前会排除 advisory aggregate，但和 UI/路由结论不一致 | 与统一 selector 使用相同 authority/freshness 规则 |
| Alerting | 低余额/耗尽事件可能由错误金额触发或无法恢复 | 只由合格 current projection 驱动 |

## 3. 不采用的临时修补

以下方案均不得作为本计划的完成实现：

- 两把 Key 数值相等时只取一把；相等可能只是独立额度碰巧一致。
- 对 Key 余额取 `max`、`min`、平均值、中位数或第一把 Key；这些算法都没有账户语义依据。
- 按 Key 数量除回去；真实独立 Key 额度和部分失败会产生新的错误。
- 仅在 `kedaya.xyz`、某个站点名称或某个账号上加特判。
- 只在 React 卡片中把 `5.6` 改为 `2.8`，继续让数据库、Dashboard 和 Routing 使用错误值。
- 只把来源文案从“站点密钥余额汇总”改成“账号余额”。
- 删除全部历史余额行或直接修改旧 migration。
- 继续让 `source` 自由文本决定 authority、scope 或可加性。
- 为了保留旧显示值，在账户余额缺失时静默回退 compatibility cache。

## 4. 目标领域语义

### 4.1 余额组件

`balance_snapshots` 中的数值必须明确属于下列一种组件；组件类型与 scope 是正交但受约束的字段。

| `balance_kind` | 合法 scope | 语义 | Station 账户余额可否使用 | 是否可跨 Key 相加 |
| --- | --- | --- | --- | --- |
| `account_balance` | `station` | 登录账户直接返回的现金/积分余额 | 是 | 不适用；每账户最多一个 current fact |
| `station_key_quota` | `station_key` | 某把 Key 的额度、剩余额度或可消费上限 | 否 | 否，除非未来 provider 契约显式声明独立钱包并由专用 projector 实现 |
| `subscription_quota` | `station` 或未来更窄 scope | 订阅窗口剩余额度 | 不直接计入“账号余额”卡片 | 不与 Key 额度相加 |
| `usage_summary` | `station` | 今日/累计请求、消费、Token 等统计载体 | 否 | 账号级统计每次只计算一次 |
| `legacy_derived_aggregate` | `station` | 历史 `station_key_balance_aggregate` 兼容行 | 否 | 否 |
| `legacy_unknown` | 现有合法 scope | 无法安全判定语义的历史行 | 否，直到重新采集 | 否 |

如果实现阶段确认无需新增持久化列也能在所有读写边界中无歧义表达 `balance_kind`，必须在 Task 1 评审中给出等价的类型化证明；不得默认继续把 `source` 当作类型。默认实施方案是增加下一可用编号的 append-only migration，引入受 CHECK 约束的 `balance_kind`。

### 4.2 Authority 与 freshness

继续复用 schema 0056 已存在的字段，不建立第二套真假标记：

- `evidence_confidence`: `confirmed | probable | unknown | conflicting`
- `spendability_authority`: `authoritative | advisory | unknown`
- `observed_at_ms`
- `valid_until_ms`
- `evidence_profile_version`
- `spendability_reason_code`

新增/调整 Rust 和 IPC 类型，使这些字段进入 canonical fact、持久化 row、`BalanceSnapshot` DTO 和 runtime balance 输入。业务代码不得再通过 source 字符串二次推断 authority。

### 4.3 Provider 契约

当前 provider 行为固定为：

| Provider / endpoint | 输出 kind | scope | authority |
| --- | --- | --- | --- |
| NewAPI `/api/user/self` | `account_balance` | `station` | 响应通过既有 envelope 与单位校验时 authoritative |
| Sub2API `/api/v1/auth/me`、`/api/v1/user/profile` | `account_balance` | `station` | 身份和数值通过校验时 authoritative |
| Sub2API 每把 Key 的 `/v1/usage` | `station_key_quota` | `station_key` | 只对该 Key authoritative；对 Station account 不具 authority |
| Sub2API dashboard stats | `usage_summary` | `station` | 对对应统计字段 authoritative，不为账户金额补值 |
| Sub2API subscription/platform quota | `subscription_quota` | `station` | 对订阅组件 authoritative，不自动改变 `account_balance` |

未来 provider 如果真的拥有“每把 Key 独立钱包，站点总额为钱包之和”的业务契约，必须新增显式 provider capability、同币种/同单位校验、完整性证明和专用 reducer；不得重新启用 generic `append_station_balance_aggregates`。

### 4.4 Current projection 规则

#### Station Account Display

1. 仅考虑 `scope = station`、`station_key_id IS NULL`、`balance_kind = account_balance`。
2. authoritative + confirmed 的最新事实为当前可信值。
3. authoritative 但过期的事实可作为 stale last-known 值展示，必须明确标注过期，不得用于严格路由资格。
4. advisory/unknown/legacy aggregate 不进入当前金额；可在诊断历史中查看。
5. 没有合格事实时返回 `missing` 或 `untrusted`，不从 Key 事实或 `stations.balance_cny` 回退。

#### Station Key Routing

1. 对候选 Key 优先使用同一 `station_key_id` 的合格 `station_key_quota`。
2. Key 事实缺失时，才允许使用同 Station 的合格 `account_balance` 作为账户级 fallback。
3. Key 额度耗尽只影响该 Key；账户余额耗尽影响该账户下所有 Key。
4. subscription quota 是否影响路由属于独立、显式的 spendable-capacity policy；本计划不允许其通过金额相加隐式改变资格。

#### Dashboard Total

1. 每个 Station 最多贡献一个 current `account_balance`。
2. 只汇总相同归一化币种；当前规范为 USD。
3. missing/untrusted/stale 的金额不进入“已知余额总计”，但应计入未知/过期站点数量。

#### Usage 与并发

账号使用量和并发限制不得依赖“恰好被选中的余额快照”。Station Detail read model 应分别投影：

```text
StationEconomicCurrentReadModel {
  accountBalance
  usageSummary
  accountConcurrency
  keyQuotaSummary
}
```

这样账号资料只有并发限制、dashboard 只有 usage stats、账号余额来自另一个 endpoint 时，字段不会互相覆盖，也不需要制造假的 station balance 行。

### 4.5 强制不变量

1. `scope = station` 必须满足 `station_key_id IS NULL`。
2. `scope = station_key` 必须满足 `station_key_id IS NOT NULL` 且 Key 属于同一 Station。
3. `account_balance` 只能使用 station scope。
4. `station_key_quota` 只能使用 station_key scope。
5. apply 层不得从多条 station_key fact 自动制造 station fact。
6. authority、kind、scope、validity 由 typed input 给出并在写入边界校验，不由 source 文案猜测。
7. 同一次采集中账号级 usage stats 只能写入/计数一次，不因 Key 数量重复。
8. 当前余额选择必须确定性排序：`observed_at_ms/updated_at -> created_at -> id`，并固定 tie-break。
9. display、routing、monitoring 对 freshness 的用途可不同，但必须共享同一事实资格定义。
10. 历史错误行保留为不合格证据，不删除、不改金额、不提升 authority。
11. 新增 Key、禁用 Key 或删除 Key 不得改变 Station account balance，除非同时出现新的账户余额事实。
12. 任何 payload、fixture、日志和诊断都不得包含完整 Key、cookie、token 或真实账号数据。

## 5. 目标架构

### 5.1 写入链

```text
Sub2API driver
  |- account profile parser -> account_balance(station)
  |- per-key usage parser    -> station_key_quota(station_key)
  |- dashboard parser        -> usage_summary(station)
  `- subscription parser     -> subscription_quota(station)

CollectorApplyRequest
  -> structural validation
  -> authority/kind validation
  -> one atomic terminal commit
  -> balance snapshots + revisions + alert transitions
```

`CollectorApplyRequest` 保持一个 operation 的原子提交边界。修复不得在 commit 后追加第二次“修正余额”写入，也不得让 UI mutation 直接覆盖数据库金额。

### 5.2 读取链

建立单一 backend owner，例如 `BalanceCurrentQuery` / `BalanceCurrentFactRepository`，负责：

- 在一个 `ReadSession` 中批量读取候选 Station/Key 余额组件；
- 校验结构、kind、authority、freshness 和 revision；
- 调用纯 reducer 生成 current projection；
- 为 Station current API、Station Detail、Dashboard 和 Routing 提供一致输入；
- 返回 typed state，不返回“看起来像有效余额”的裸数值。

现有 `application/operational_facts/balance_projector.rs` 应成为 reducer 复用入口，或由新的窄 reducer 替代并删除旧入口。禁止维护两个彼此不同的 Station 余额选择算法。

### 5.3 投影状态

推荐统一状态：

```text
available | depleted | stale | untrusted | missing | unsupported | not_applicable
```

投影同时返回：

- selected snapshot id/revision；
- selected scope/kind；
- amount/currency；
- authority/confidence；
- observed/valid-until 时间；
- reason code；
- source label（仅用于解释，不用于决策）。

## 6. 分阶段实施任务

### Task 0：冻结基线与失败用例

目标：在修改算法前，用明显假值固定现场因果链与所有受影响消费者。

实施：

1. 为 Sub2API driver 增加双 Key fixture：两把 Key 的 `/v1/usage` 都返回 `2.8`，账号资料返回 `2.8`，dashboard 返回一份账号级统计。
2. 增加账号资料只返回并发限制、账号资料不可用、单 Key 请求失败、两把 Key 返回不同额度的 fixture。
3. 在 apply 层增加回归：输入两条 station_key fact 不得自动出现 station sum。
4. 记录当前预期失败点；不得通过修改测试期望为 `5.6` 固化错误。
5. 新增静态门禁，列出 `station_key_balance_aggregate` 的生产引用，作为后续删除台账。

主要文件：

- `src-tauri/src/services/collectors/drivers/sub2api/mod.rs`
- `src-tauri/src/services/collectors/drivers/sub2api/mapping.rs`
- `src-tauri/src/services/collectors/collector_apply.rs`
- `scripts/` 下新增或扩展余额 authority/aggregation 门禁

退出条件：测试能稳定证明旧实现得到错误 station aggregate，并覆盖 Key 数量变化不应影响账户余额。

### Task 1：建立 typed balance contract 与兼容 migration

目标：让 scope、kind、authority、freshness 成为类型和数据库共同约束，而不是 source 字符串约定。

实施：

1. 为 collector canonical fact 增加 typed `balance_kind`、authority、evidence confidence、observed/valid-until、profile version 和 reason code。
2. 使用 enum/validated newtype 替代写入边界的任意 scope/kind/status 字符串；IPC 仍按稳定 snake_case 序列化。
3. 使用实现时下一可用 migration 编号，为 `balance_snapshots` 增加 `balance_kind` CHECK 列和必要索引；不得预占当前工作区正在使用的 migration 编号。
4. backfill 仅做可证明分类：
   - `station_key_balance_aggregate` -> `legacy_derived_aggregate`，authority 保持 advisory/unknown；
   - station_key scope -> `station_key_quota`，但不提升历史 authority；
   - 已确认直接账户 source -> `account_balance`，仅保留原 authority；
   - 历史 composite/无法拆分 source -> `legacy_unknown`；
   - 不重算、不删除、不把未知历史提升为 authoritative。
5. 增加 `(station_id, balance_kind, scope, updated_at...)` 与 key current lookup 所需的有界索引；用 `EXPLAIN QUERY PLAN` 测试证明 current 查询走索引。
6. 更新 portable catalog、schema fingerprint、升级 fixture、artifact policy 和 migration checksum 契约。
7. 扩展 Rust `BalanceSnapshot`、IPC DTO、TypeScript types 和生成绑定，暴露 eligibility 所需字段。

退出条件：非法 kind/scope/key-id 组合无法写入；旧数据库可升级；未知历史不会被错误提升。

### Task 2：修正 Sub2API producer 与组件合成

目标：保留真实 per-key 证据，同时始终优先保存直接账号事实，彻底移除 Key 求和。

实施：

1. 保留每把可用 Key 的公平 `/v1/usage` 调用和既有 transient retry budget；每条成功结果写为 `station_key_quota`。
2. 重写 `merge_account_profile_balance`：
   - profile 有余额时，无论 Key fact 是否存在，都保留独立 `account_balance(station)`；
   - profile 只有并发限制时，写入/投影 account metadata，不制造金额为零或沿用 Key 金额的 account balance；
   - 禁止把 account concurrency 复制成每把 Key 的独立限制语义。
3. 重写 `merge_dashboard_usage_stats`：账号级 request/consumption/token 只附加一次；没有 account balance 时形成 `usage_summary`，value 保持 unknown，不从 Key 求和。
4. 重写 `merge_subscription_quota`：输出独立 `subscription_quota`；不与 Key 额度相加，也不改变“账号余额”金额。
5. 删除 `collector_apply.rs::append_station_balance_aggregates` 及其仅为无条件求和服务的 helpers。
6. 删除 `collector_balance_authority(source)` 的 source 特判，authority 直接来自 validated fact。
7. 保持 NewAPI `/api/user/self` 为 direct `account_balance`，增加防回归测试证明单 Key/多 Key 数量与 NewAPI 账户余额无关。

关键行为样例：

| 输入 | account projection | key projections |
| --- | --- | --- |
| profile=2.8；Key A=2.8；Key B=2.8 | 2.8 | A=2.8，B=2.8 |
| profile=2.8；Key A=1；Key B=3 | 2.8 | A=1，B=3 |
| profile unavailable；Key A=2.8；Key B=2.8 | missing/untrusted | A=2.8，B=2.8 |
| profile only concurrency=3000；Key A=2.8 | account amount missing；concurrency=3000 | A=2.8 |
| account=2.8；subscription=5 | account card=2.8；subscription component=5 | 不变 |

退出条件：生产源码不存在 generic key-to-station sum；双 Key fixture 的 Station account 结果严格为 `2.8`。

### Task 3：建立统一 current-balance selector

目标：所有消费者共享相同事实资格与确定性选择规则。

实施：

1. 将 station/key candidate 查询收敛到一个窄 read owner，批量读取并避免 N+1。
2. reducer 显式接收 `evaluation_at`，不在内部随意读取系统时钟，保证测试确定性。
3. display selector、routing selector 和 monitoring selector共享结构/authority 判断；purpose 只决定 stale 是否可显示或可参与资格。
4. 修改 runtime balance 类型，携带 authority、confidence、observed/valid-until；删除 `project_runtime_balance` 中硬编码 `authoritative: true`、`fresh: true`。
5. station key 优先、station account fallback 的路由规则通过同一 reducer 实现。
6. malformed DB row、未知 enum、非有限数值、货币缺失必须 fail closed，并返回 typed reason，不 panic、不静默使用。
7. 查询 instrumentation 证明 Station workspace、Detail 和 routing snapshot 都是有界批量读取。

退出条件：同一组 fixture 经 Station、Dashboard、Routing、Monitoring selector 得出的 authority/freshness 结论一致。

### Task 4：切换所有消费者并删除旁路

目标：错误 aggregate 即使仍在历史表，也不能成为任何 current decision。

实施顺序：

1. `PricingStore::latest_station_balances` / `list_current_station_balance_snapshots` 改为 backend current projection，不再仅按 scope 取最新裸行。
2. Station Asset/List 使用 current account projection；余额筛选、positive count、low/depleted tag 读取 typed state。
3. Station Detail read model 增加 `StationEconomicCurrentReadModel`，余额、usage、并发分别投影；历史 `balances` 继续作为诊断集合且保持上限。
4. Dashboard 只汇总 qualified account projections，并显示 unknown/stale station count；不得把 advisory 行当 0 悄悄混入。
5. RoutingStore 与 operational-facts query 切到统一 selector；station aggregate legacy row不得进入 candidate eligibility。
6. Monitoring/zero-balance pause 改用共享资格 helper，保留已实现的 authoritative + confirmed + validity 约束。
7. Alerting low/depleted observation 只由 qualified current projection产生；untrusted/missing 不伪造成 depleted。
8. 删除前端 `latestStationBalanceSnapshotsByStation` 中重复的权威选择职责；前端只格式化 backend projection。若保留纯 TS reducer用于离线/demo，必须由共享契约 fixture与 Rust reducer做 differential test。

退出条件：搜索生产源码后，不存在绕过统一 selector 的 current station balance 查询；历史接口可以读 raw snapshots，但名称和调用点明确为 history/diagnostics。

### Task 5：UI 语义与状态收口

目标：用户能分辨账户余额、Key 额度、订阅额度和未采集状态，且不增加页面噪声。

实施：

1. Station Detail 的“当前余额”改为“账号余额”，helper 显示直接来源和采集时间。
2. 当只有 Key 额度时显示“账号余额未采集”，不得把任意 Key 值放进账号余额卡。
3. 在登录与密钥/诊断区域增加紧凑的 Key 额度摘要，例如“2 把 Key 已采集”，按需展开每 Key；不显示完整 Key。
4. subscription quota 如需展示，使用独立卡片/行并明确窗口与 reset 时间，不与账号余额合并。
5. stale 显示上次金额和“已过期”；untrusted 显示“来源不足”；missing 显示“未采集”。
6. Dashboard 总额文案明确为“已知账号余额总计”，同时给出未知/过期站点数。
7. 覆盖 loading、empty、error、disabled、窄窗口、键盘焦点与长来源文案。

退出条件：截图场景显示 `USD 2.80`（前提是 direct account endpoint 成功）；若 direct account endpoint 不可用，则诚实显示未采集而不是 `USD 5.60`。

### Task 6：历史数据、升级与恢复资格

目标：现有安装升级后立即停止使用错误值，同时不破坏历史和回滚边界。

实施：

1. migration 只分类历史记录，不删除或重算金额。
2. `legacy_derived_aggregate` 和 `legacy_unknown` 从所有 current projection 排除，但保留在 bounded history/diagnostics。
3. 首次升级后无需等待重新采集即可停止展示 5.6；UI 允许暂时显示 missing/untrusted。
4. 下一次成功 balance collection 写入正确 direct account fact，并正常推进 Station Detail/domain revision。
5. 采集失败不得复活旧 aggregate，也不得清空上一条仍在 freshness 期限内的 direct account fact。
6. 验证旧 schema fixture、当前 schema fixture、升级中断/重启、backup manifest、portable export/import 和 foreign-key clean。
7. 若 migration 需要重建索引或表，遵守 `SCHEMA_UPGRADE_AUTHORING.md`，不得手工改生成 fixture 或 checksum。

退出条件：从包含错误 `station_key_balance_aggregate` 的旧数据库升级后，Station/Dashboard/Route 均不再使用该金额；历史仍可审计。

### Task 7：删除台账、文档与资格审计

必须删除或退役：

- `collector_apply.rs::append_station_balance_aggregates`；
- 无条件 `sum_present_values(key_balances...)` 生成 station value 的所有分支；
- `collector_balance_authority` 对 `station_key_balance_aggregate` 的 source 字符串特判；
- current UI 中“站点密钥余额汇总”作为账号余额来源的映射；
- 任何只按 `scope = station` 且不判断 kind/authority 的 current 查询；
- runtime projector 中固定 authoritative/fresh 的假设；
- 期望多 Key station sum 的旧测试。

必须保留：

- per-key `/v1/usage` 事实及其重试、公平性、脱敏 evidence；
- raw snapshot/history 的有界诊断用途；
- schema 0056 authority/freshness 字段；
- account-level dashboard usage 统计；
- Station Detail revision 与原子 collector terminal commit；
- 明显假值的回归 fixture。

实施完成后：

1. 更新 `PRODUCT_MODEL.md` 的 Balance Snapshot 语义和 aggregation 禁令。
2. 更新相关安全/字段 ownership 台账与 architecture gate。
3. 新建 qualification audit，记录代码证据、migration、测试数量、真实账号验收边界和 residual risk。
4. 在 `docs/README.md` 标记本计划状态；未完成真实账号验收时不得写“已发布验证通过”。

## 7. 测试矩阵

### 7.1 Rust 单元测试

- kind/scope/key-id 合法组合与拒绝矩阵；
- account profile 余额在已有 Key facts 时仍被保留；
- profile 仅有 concurrency 时不制造金额；
- dashboard stats 不从 Key 求和；
- subscription quota 不改变 account balance；
- generic apply 不从 Key 生成 station row；
- authority/confidence/validity 由 typed fact 持久化；
- current selector 的 available/depleted/stale/untrusted/missing；
- key scope 优先和 station account fallback；
- 非有限数值、未知 kind、currency 不一致 fail closed；
- tie-break 与固定 evaluation time 的确定性。

### 7.2 Rust 集成测试

| 场景 | 断言 |
| --- | --- |
| 双 Key 同值 + direct account | station=2.8；两条 key facts 保留；无 5.6 |
| 双 Key 异值 + direct account | station 仍等于 direct account；不等于 Key sum/max/min |
| direct account 缺失 | station current missing/untrusted；key facts 可用于对应 Key |
| 一把 Key 失败 | account fact 不受成功 Key 数量影响；operation status按既有策略处理 |
| Key 新增/禁用/删除 | 未出现新 account fact时 station amount不变化 |
| aggregate 历史升级 | legacy row不可进入 Station/Dashboard/Routing current projection |
| stale direct account + fresh key | Station display标 stale；Key routing使用对应 fresh key fact |
| account depleted + key positive | 依据明确的 scope precedence输出一致 reason，不由查询顺序决定 |
| collector atomic failure | 不留下半写 kind/authority/revision |
| restart/replay | 相同 operation 幂等，不重复组件、不重复 usage |

### 7.3 前端 Vitest

- Station Detail 账号余额为 2.8；
- 只有 Key quota 时账号余额显示未采集；
- Key quota 摘要不暴露 secret；
- stale/untrusted/missing 文案与 tone；
- Dashboard 每站只计算一个 account balance；
- Dashboard unknown/stale count；
- Station issue tags 不把 missing 当 depleted；
- narrow width、长站点名、长来源标签无重叠；
- read model schema mismatch fail closed。

### 7.4 静态与架构门禁

- 禁止生产代码出现 `station_key_balance_aggregate` writer；
- 禁止 generic key balance sum 生成 station amount；
- 禁止 current 查询绕过 authority/kind；
- 禁止 UI 读取 `stations.balance_cny` compatibility cache；
- generated bindings deterministic；
- migration checksum、schema fingerprint、portable catalog 一致；
- secret/log/artifact policy 通过。

### 7.5 真实账号验收

真实验收只使用用户本机已有账号，不记录或提交 secret、原始响应或真实标识。至少覆盖：

1. KedayA 两把 Key，真实账号余额约 2.8；采集后 Station Detail 不再显示 5.6。
2. Station 列表与 Dashboard 显示同一账号金额。
3. 添加第三把 Key 后账号余额不变。
4. 禁用/删除其中一把 Key 后账号余额不变。
5. 单 Key `/v1/usage` 失败时，direct account 仍可显示；direct account 也失败时进入 stale/missing。
6. 重启应用后 current projection一致。
7. Route simulation 对每把 Key 显示其 scope 与选择理由，不使用 legacy aggregate。

真实响应只记录脱敏结论、时间和 outcome；不把截图中的账号信息、cookie、Key 或 payload 加入 fixture。

## 8. 验证命令与门禁

实施时按任务逐步运行，最终至少执行：

```powershell
pnpm vitest run <相关余额与 Station/Dashboard 测试文件>
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml <相关 balance/sub2api/routing 测试>
pnpm generate:bindings --check
pnpm verify:fast
```

由于该改动跨 collector、persistence、IPC、Station UI、Routing 与 Monitoring，完成所有阶段后必须运行：

```powershell
pnpm verify:full
```

如果引入 migration，还必须运行 schema upgrade、portable migration、frozen fixture、checksum、fingerprint 和 backup/recovery 专项检查。不得以 `verify:fast` 替代这些专项验证，也不得声称未执行的真实账号验收已通过。

## 9. 分阶段提交与回滚边界

未经用户明确要求，不执行 stage、commit、push、建分支或 PR。需要提交时建议保持以下可回滚边界：

1. tests: freeze shared-account multi-key balance regression
2. feat: add typed balance component and authority contract
3. fix: preserve Sub2API account balance and remove key aggregation
4. refactor: centralize current balance selection
5. feat: expose station economic current read model
6. fix: retire legacy aggregate from all consumers
7. docs: qualify station balance scope reliability

任何阶段都不得留下“新 writer + 旧 current reader”或“旧 writer + 新 reader”导致的双真相。若无法在一个提交内原子切换，先让 reader 同时理解新旧事实但只信任新 authority，再切 writer，最后删除兼容读取；每一步都必须有门禁证明旧 aggregate不会重新成为 current。

回滚只回滚应用代码版本，不逆向执行破坏性 SQL。新增 migration 必须向前兼容旧应用无法识别新 kind 的风险，并在发布资格中声明最低回滚版本。历史分类不可通过手工 SQL 降级。

## 10. 完成定义

本计划只有在以下条件全部满足时才能标记“本轮可交付实现完成”：

- 双 Key `2.8 + 2.8` 场景的 Station account projection 为 `2.8`，不存在 `5.6` current value；
- generic Key-to-Station aggregation writer 已删除并有负向门禁；
- direct account、key quota、subscription quota、usage summary 具有明确 kind/scope/authority；
- 历史 aggregate 不参与 Station、Dashboard、Routing、Monitoring 或 Alerting current decision；
- 所有 current 消费者使用统一 backend selector；
- account profile 余额不再被 concurrency merge 丢弃；
- direct account 缺失时 fail closed，不使用数值相等、首 Key、平均或 compatibility cache 猜测；
- Station UI 能区分账号余额、Key 额度、stale、untrusted 和 missing；
- migration/upgrade/portable/backup/restart 测试通过；
- 相关 Vitest、build、Cargo fmt/check/test、`verify:fast` 与 `verify:full` 实际取得退出码 0；
- 真实 KedayA 验收完成，或在 qualification audit 中明确列为发布 no-go，不能伪装成已通过；
- 长期规范和 qualification audit 已更新，未提交任何 secret、真实 payload、日志或本地数据库。

## 11. 残余风险与后续边界

- 修改版 Sub2API 可能不提供直接账号余额 endpoint。此时正确结果是 unknown/stale，不是从多把 Key 猜总额；如产品希望支持人工账户余额，需要单独设计可审计的 manual source 和 revision，不在本计划中偷偷加入。
- 某些 provider 可能把 `/v1/usage` 的“余额”定义为账户余额、Key quota、订阅余额或三者最小值。adapter 必须按 endpoint 契约和 fixture 标注 kind；无法证明时使用 unknown/advisory。
- 订阅额度是否参与路由 spendable capacity 是独立产品决策。本计划只禁止把它混入账号余额和 Key 求和；后续若接入，必须使用独立 policy/reducer。
- 历史 composite 行无法可靠拆回账户余额与订阅额度，因此不会尝试数学反推。重新采集是恢复权威 current fact 的唯一自动路径。
- 本计划不重新设计价格倍率、请求成本、路由评分或 circuit；但它会修正这些模块读取余额事实的资格边界。
