# Relay Pool 数据架构工程化总 SPEC

日期：2026-07-07

## 用途

这份文档是后续所有数据架构升级线程的总入口。上下文压缩、换执行者、开新工作树或主线合并后，先读本文件，再读对应 stage 计划。

它回答三件事：

- Relay Pool 的长期架构方向是什么。
- 哪些字段和能力绝对不能被误删、误合并、误降级。
- 当主工作区继续修 bug 并新增字段时，升级工作树应该如何接入变化。

配套字段登记簿：`docs/superpowers/audits/relay-pool-field-ownership-ledger.md`。新增字段、漂移字段、准备降级字段都必须先登记到该文件。

## 总目标

Relay Pool Desktop 要从“页面能各自跑起来的本地桌面工具”升级为成熟的本地 AI 网关工程项目：

- React/Tauri 管理面是控制面。
- 本地 OpenAI-compatible proxy 是数据面。
- 数据库事实表保存权威事实、历史证据和兼容缓存。
- 页面和 runtime 不直接拼凑多张表，而是消费 query services 与 current projections。
- 每个阶段都保持已有能力可用，先加保护网，再迁移消费者，最后才讨论字段降级或删除。

## 不可破坏能力

后续任何 stage 都必须保护这些能力：

- 中转站增删改、启用禁用、优先级、采集间隔。
- Sub2API / NewAPI / OpenAI-compatible 站点接入。
- 登录凭据保存、远端 Key 扫描、远端 Key 创建、本地 Key 创建。
- Station Key 分组绑定、编辑页保留绑定、显式清除绑定。
- `/groups/available` 与 `/groups/rates` 的分组发现、倍率采集和变更事件。
- 价格比较页按模型、站点、Key、分组展示候选价格。
- Dashboard 和站点资产页余额展示。
- 变更中心未读、风险、已读状态。
- 路由模拟、本地 proxy route candidate、本地 OpenAI-compatible 入口。
- 浏览器 preview fallback 能展示 UI，但不定义业务真相。

## 第一原则

**不先删字段，不先合并相似字段，不先重写 schema。先定语义、加测试、建投影、迁消费者，最后再评估字段是否可降级。**

如果一个字段看起来冗余，先问：

- 它是权威事实、历史证据、当前投影、兼容缓存，还是未知新字段。
- 谁写它，谁读它。
- fallback 顺序是什么。
- 老数据库、preview fallback、runtime route 是否仍依赖它。
- 是否已经有 focused tests 证明删除或合并不会改变行为。

## 架构分层

### Control Plane

Tauri commands、React UI、设置页、采集器、数据库写入服务都属于控制面。控制面负责用户操作、配置管理、采集、事实归一化和投影生成。

### Data Plane

本地 OpenAI-compatible proxy 属于数据面。它应该读取稳定的 runtime snapshot 或 snapshot-producing service，不应该读取 UI view model，也不应该在请求时临时重建页面查询。

### Canonical Facts

权威事实有稳定身份，代表当前配置或当前对象关系：

- `stations`
- `station_keys`
- `station_group_bindings`
- `pricing_rules`
- `station_key_capabilities`
- route policies / model aliases
- secrets / settings

### Evidence / History

证据和历史记录来自采集、请求、健康检查和变更检测：

- `group_rate_records`
- `balance_snapshots`
- `collector_runs`
- `collector_snapshots`
- `station_key_health`
- `station_endpoint_health`
- `request_logs`
- `change_events`
- `remote_station_keys`

历史证据可以刷新投影，但页面不应该每个都自己决定哪个证据是“当前状态”。

### Current Projections

当前事实投影回答“现在 UI 或 runtime 应该相信什么”：

- `StationCurrentSummary`
- `StationGroupCurrentFact`
- `StationKeyCurrentFact`
- `StationBalanceCurrentFact`
- `PricingComparisonCandidate`
- `RouteCandidate`
- `RuntimeRouteSnapshot`

投影函数必须尽量纯函数化、可测试、显式暴露 source/evidence。

### Compatibility Cache

兼容缓存字段保留用于旧数据库、旧页面、preview fallback 或迁移期间的读写兼容：

- `station_keys.group_name`
- `station_keys.group_id_hash`
- `station_keys.rate_multiplier`
- `station_keys.rate_source`
- `station_keys.rate_collected_at`
- `stations.balance_raw`
- `stations.balance_cny`
- `stations.last_pricing_fetched_at`

这些字段第一轮不删除。新代码不得把它们当权威事实直接消费；必须通过白名单、projection 或明确旧消费者路径。

### Query Services

Query services 负责加载编排和 raw facts 搬运，不负责定义业务真相。

允许：

- 组合多个现有 API 调用。
- 返回 projection-ready 的 raw facts bundle。
- 保留页面既有 loading、toast、局部失败策略需要的错误信息。
- 在字段所有权扫描中作为 read-through 边界存在。

禁止：

- 把兼容字段解释为新的权威事实。
- 在 query 层实现分组 identity、倍率 fallback、余额优先级或 runtime route 决策。
- 复制 projection 层的业务判断。

如果 query service 必须读取兼容字段，只能作为搬运字段返回，并且对应字段必须登记在 `docs/superpowers/audits/relay-pool-field-ownership-ledger.md`。

## 关键字段语义

| 字段 | 语义 | 禁止事项 |
| --- | --- | --- |
| `station_group_bindings.id` / `group_binding_id` | 本地 durable row identity，表示当前绑定行 | 不能用上游 group id 替代 |
| `group_key_hash` | 本地稳定 group identity，用于本地去重和绑定查找 | 不能和 `group_id_hash` 互相替代 |
| `group_id_hash` | 脱敏后的上游 group id，仅在上游有真实 group id 时存在 | 不能当作本地 binding id |
| `group_name` | 展示名称和 legacy fallback | 不能作为首选 identity |
| `station_keys.rate_multiplier` | 兼容展示缓存 | 不能作为权威倍率来源 |
| `station_group_bindings.effective_rate_multiplier` | 当前 binding 的最佳已知倍率投影 | 不能由页面随意写 |
| `group_rate_records.*multiplier` | 历史倍率证据 | 不能单独复活 missing/disabled binding |
| `balance_snapshots.scope = station` | 当前站点余额证据候选 | 优先级高于 station-key 余额快照 |
| `stations.balance_raw` / `balance_cny` | 旧站点余额缓存 | 只能在没有 station-scope 快照时 fallback |
| `last_pricing_fetched_at` | 粗粒度采集时间 | 不能作为分组新鲜度唯一依据 |

## 分组 identity 决策

分组当前事实的 identity fallback 顺序：

1. `group_binding_id`
2. `group_key_hash`
3. `group_id_hash`
4. normalized `group_name`，仅 legacy fallback

`group_key_hash` 和 `group_id_hash` 不等价。即使两个字段当前字符串相同，也不能假设生命周期、写入者和用途相同。

## 倍率 fallback 顺序

当前分组倍率 fallback 顺序：

1. Binding `user_rate_multiplier`
2. Binding `effective_rate_multiplier`
3. Latest rate `user_rate_multiplier`
4. Latest rate `effective_rate_multiplier`
5. Binding `default_rate_multiplier`
6. Latest rate `default_rate_multiplier`
7. `null`

`missing` / `disabled` binding 不能被旧 rate history 覆盖成可用状态。

## 主线漂移规则

升级期间主工作区会继续修 bug。工作树不能把初始审计当作永久事实。

每个 stage 开始、结束和合并前必须做 drift intake：

- 记录主工作区 HEAD、工作树 HEAD、主工作区 dirty paths。
- 接入主线变更后重新跑字段所有权扫描和 focused tests。
- 新字段默认进入 `docs/superpowers/audits/relay-pool-field-ownership-ledger.md` 的 `unknown pending audit`，不得立即删除、合并或重命名。
- 如果新字段影响价格、余额、Key 分组、remote key、runtime route，先补审计再继续。
- 如果主线 bugfix 改变行为，以主线行为为事实，更新测试或计划，不回滚主线修复。
- 主工作区 dirty files 不是可 merge 的事实。必须等提交、使用用户批准的 patch、或明确记录未接入，不能把 `git merge master` 成功当作未提交改动已接入。

## 允许借鉴的成熟项目原则

只借鉴架构思想，不复制实现：

- LiteLLM：virtual key 与 provider key 分层，模型组和部署目标分层。
- Portkey：路由、fallback、retry、cache 应由配置编译，不由页面推导。
- Envoy AI Gateway：control plane / data plane 分离。
- APISIX：运行时配置可版本化、可替换、可热更新。
- Kong：Route、Service、Upstream、Consumer 等实体分离。
- TensorZero：gateway、observability、evaluation、optimization 分离。
- Bifrost：virtual key、模型过滤、负载均衡、故障切换边界清晰。
- Open WebUI：核心 UI 模型与 provider 扩展逻辑分离。

## 验收定义

这轮总升级完成时应满足：

- 页面通过 shared query services 获取数据，不重复写大段 `Promise.all` 编排。
- 当前分组、倍率、余额、价格状态由 shared projection utilities 生成。
- 价格页、站点详情、站点资产、Key Pool、Add Provider 对分组 identity 和倍率展示一致。
- runtime route snapshot 不消费 UI view model。
- 兼容字段有清晰读写所有权，新代码不能随意直接消费。
- 每个迁移 stage 都有 focused tests、必要 build/check 和可回滚提交。
