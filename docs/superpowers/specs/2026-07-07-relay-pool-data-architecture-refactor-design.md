# Relay Pool 数据架构工程化改造设计

日期：2026-07-07

## 目标

Relay Pool Desktop 正在从“能跑的本地桌面工具”走向一个更成熟的本地 AI 网关控制面。项目里已经有不少正确的基础对象：中转站、Station Key、分组绑定、分组倍率历史、余额快照、价格规则、变更事件、采集任务、请求日志、路由候选、本地代理运行态等。

当前主要问题不是缺少对象，而是同一类事实在多个页面和多个字段里被重复拼装。部分字段同时承担“当前状态”“历史证据”“展示缓存”的角色，导致页面之间可能不同步，也让重复代码越来越多。

本设计的目标是：在不破坏现有功能的前提下，明确字段所有权，建立共享查询层和当前事实投影层，逐步删除页面里的重复拼装逻辑，并为后续成熟工程化打基础。

第一原则：**不先删字段，不先合并相似字段，不先重写表结构。先定语义、加测试、建投影、迁消费方，最后再评估字段是否可以降级或废弃。**

## 参考项目与架构启发

这些项目只作为架构参考，不复制实现。

- LiteLLM Proxy：Virtual Key 用来治理模型权限、预算和花费，上游 provider key 与本地入口 key 不是同一个概念。它的 model group / deployment 也说明模型、部署、路由目标需要分层。
  - https://docs.litellm.ai/docs/proxy/virtual_keys
  - https://docs.litellm.ai/docs/proxy/load_balancing
- Portkey AI Gateway：用可组合配置表达 fallback、load balancing、retry、cache 等策略。它说明运行时路由策略应该从控制面配置编译出来，而不是每个页面自己推导。
  - https://portkey.ai/docs/product/ai-gateway/configs
  - https://portkey.ai/docs/product/ai-gateway/fallbacks
- Envoy AI Gateway：强调 control plane / data plane 分离。Relay Pool 的 Tauri/React 管理面应该是控制面，本地 OpenAI-compatible proxy 应该读取稳定的运行时快照。
  - https://aigateway.envoyproxy.io/docs/concepts/architecture/
- Apache APISIX：支持控制面/数据面和 standalone 配置模式，也强调配置热更新和版本一致性。Relay Pool 的本地运行时配置也应该可版本化、可原子替换。
  - https://apisix.apache.org/docs/apisix/deployment-modes/
- Kong Gateway：Service、Route、Upstream、Consumer、Plugin 是分离实体。Relay Pool 也应该区分路由规则、上游站点、可用 Key、调用身份和策略。
  - https://developer.konghq.com/gateway/entities/route/
- TensorZero：把 gateway、observability、evaluation、optimization 拆成相关但独立的子系统。它说明请求日志、采集证据、优化分析不应该直接混成当前 UI 状态。
  - https://github.com/tensorzero/tensorzero
- Bifrost AI Gateway：强调 virtual key、模型过滤、加权分发、负载均衡和故障切换。Relay Pool 的 Key 治理和上游 Key 资产也需要明确边界。
  - https://docs.getbifrost.ai/overview
- Open WebUI 扩展机制：核心 UI 模型和扩展逻辑分离。Relay Pool 后续 provider 特性也应该放在 adapter/projection 边界里，而不是散落到页面分支。
  - https://docs.openwebui.com/features/extensibility/

## 当前审计结论

### 1. 数据加载编排重复

多个页面在重复组合相似调用：

- `PricingPage`：中转站、Station Key、分组绑定、倍率记录、价格规则。
- `StationDetailPage`：中转站、凭据、Station Key、分组绑定、倍率记录、采集任务、最新快照、余额快照、变更事件。
- `StationsPage`：站点列表，再补余额、变更、快照、Key、分组事实、采集任务。
- `AddProviderPage`：站点、凭据、Key、分组绑定、倍率记录、远端 Key 能力、远端 Key 列表。
- `KeyPoolPage`：站点、Key 池、监控项、模板、分组选项。

这些重复会造成加载失败策略、超时策略、旧数据保留策略、刷新行为逐渐不一致。

### 2. 分组与倍率投影重复

多个地方各自做了“binding + latest rate + 去重 + 倍率 fallback”：

- `AddProviderPage` 把 bindings/rates 转成草稿行，自己去重，再合并保存后的分组选项。
- `stationDetailViewModels.ts` 按 group name 去重，并自己选择更可信的 binding。
- `pricingComparisonViewModel.ts` 同时从 bindings 和 standalone rate records 生成价格候选。
- Rust 的 `shared_capabilities.rs` 已经有 `station_group_options_from_facts`，但现在只服务分组选项，没有成为通用事实投影中心。

方向是对的，但中心能力还太窄。项目需要一个统一的当前事实投影层，而不是每个页面各写一个近似版本。

### 3. 工具函数重复

项目已经有 `src/lib/errors.ts` 和 `src/lib/formatters.ts`，但页面里仍然重复写：

- `readError`
- `formatRate`
- `formatMultiplier`
- `formatTime`
- `toTime`
- status label / status tone 映射

这类重复风险比业务重复低，但会让文案、格式和交互细节持续漂移。

### 4. Preview fallback 语义重复

很多 `src/lib/api/*.ts` 都各自实现 `isInvokeUnavailable` 和 memory fallback。浏览器预览 fallback 是有价值的，但它不能成为第二套业务实现。应该有统一 fallback 包装，并且明确它只模拟 UI 交互，不定义业务真相。

### 5. 字段语义容易误合并

一些字段看起来相似，但语义不同，不能粗暴合并：

- `group_binding_id`：本地 durable row identity。
- `group_key_hash`：本地稳定 identity，用于去重和绑定查找。
- `group_id_hash`：有真实上游 group id 时的脱敏上游身份。
- `group_name`：展示名称，也作为 legacy fallback。
- `station_keys.rate_multiplier`：兼容/展示缓存，不应成为权威倍率来源。
- `station_group_bindings.effective_rate_multiplier`：当前 binding 上的最佳已知倍率投影。
- `group_rate_records`：历史倍率证据和变更检测来源，不等于唯一当前状态。

任何迁移都不能在没有读写清单和测试保护的情况下把这些字段合并。

## 目标架构

Relay Pool 应该被视为一个本地 AI 网关：

- React/Tauri 管理面是 **控制面**。
- 本地 OpenAI-compatible proxy 是 **数据面**。
- 数据库中的业务对象是 **事实层**。
- 采集记录、请求日志、变更记录是 **证据/历史层**。
- 页面和运行时读取的是 **当前事实投影** 或 **运行时快照**。

### 控制面

控制面包括 Tauri app、数据库、采集器、设置页、React UI。它负责配置、凭据、站点资产、采集事实、归一化投影和用户操作。

### 数据面

数据面是本地 OpenAI-compatible proxy。它不应该在请求时临时拼 UI 视图，也不应该到处查表。它应该读取一份编译好的 runtime snapshot，并写回请求日志、健康状态和路由证据。

### Canonical Facts：权威事实

这些对象拥有核心身份和关系：

- `stations`
- `station_keys`
- `station_group_bindings`
- `pricing_rules`
- `station_key_capabilities`
- 路由策略与模型别名
- `secrets`
- `settings`

权威事实可以引用证据，但不能依赖页面本地 join。

### Evidence / History：证据与历史

这些对象保存观察结果：

- `group_rate_records`
- `balance_snapshots`
- `collector_runs`
- `collector_snapshots`
- `station_key_health`
- `station_endpoint_health`
- `request_logs`
- `change_events`
- `remote_station_keys`

证据由 projection service 使用。产品页面不应该直接解析原始证据作为主状态，除非该数据还没有对应投影。

### Current Projections：当前事实投影

当前事实投影回答：“现在 UI 或 runtime 应该把什么当作当前状态？”

需要建立的投影包括：

- `StationCurrentSummary`
- `StationGroupCurrentFact`
- `StationKeyCurrentFact`
- `StationBalanceCurrentFact`
- `PricingComparisonCandidate`
- `RouteCandidate`
- `RuntimeRouteSnapshot`

投影函数尽量做成纯函数，独立测试。等类型稳定后，再决定是否把部分投影搬到 Rust/Tauri command。

### Compatibility Fields：兼容字段

以下字段第一轮不删除，只分类和限制新增消费：

- `station_keys.group_name`
- `station_keys.group_id_hash`
- `station_keys.rate_multiplier`
- `station_keys.rate_source`
- `station_keys.rate_collected_at`
- `stations.balance_raw`
- `stations.balance_cny`
- `stations.last_pricing_fetched_at`

这些字段在现阶段视为兼容/缓存字段。新代码应该通过 current projection 读取，而不是直接把它们当权威事实。

兼容字段的新增读取必须经过白名单：

- 允许读取：projection service、migration/backfill、旧页面尚未迁移的兼容路径、明确的调试/诊断展示。
- 禁止读取：新页面 view model、runtime snapshot 编译以外的运行时路径、价格/余额/分组的新业务判断。
- 每迁移一个消费者时，要把对应兼容字段读取从“旧页面临时允许”移动到“禁止新增消费”清单。

## 字段所有权决策

### Station

`stations` 拥有站点身份、上游地址、账号级设置、启用状态、优先级、充值比例 `credit_per_cny` 和粗粒度状态。

`stations.balance_raw` 和 `stations.balance_cny` 保留为兼容字段。新的当前余额 UI 应优先读取 `StationBalanceCurrentFact`，它来自最新 station-scope `balance_snapshots`，只有没有快照时才回退到 station 字段。

`stations.last_pricing_fetched_at` 保留为粗粒度采集时间。等分组/倍率投影存在后，它不能作为分组新鲜度的唯一来源。

### Station Key

`station_keys` 拥有可路由本地 Key 身份、父站点、启用状态、优先级、状态、路由能力引用和可选分组绑定引用。

`station_keys.group_binding_id` 是 Key 当前分组选择的主要引用。

`station_keys.group_name`、`group_id_hash`、`rate_multiplier`、`rate_source`、`rate_collected_at` 是兼容/展示缓存。新功能应通过 `StationKeyCurrentFact` 读取这些含义。

### Station Group Binding

`station_group_bindings` 拥有当前本地分组身份和绑定状态。

- `binding_kind = station_group`：表示站点当前已知的分组。
- `binding_kind = key_binding`：表示 Key 到分组的绑定，或 legacy/manual 关系。

binding 行可以保存当前倍率字段，因为它是当前投影锚点。这些字段应由采集/应用服务更新，不应由页面本地逻辑随意写。

### Group Rate Record

`group_rate_records` 拥有历史倍率观察记录。它服务于审计、变更检测和投影刷新，但页面不应该自己选择 latest rate。统一 projection 应该负责这件事。

### Pricing Rule

`pricing_rules` 拥有归一化模型价格事实和手动/采集价格覆盖。它不拥有分组归属。价格比较应该通过 shared pricing candidate builder join 价格规则与分组投影。

### Balance Snapshot

`balance_snapshots` 拥有余额证据。当前余额是按 station/key/scope 选出的最新投影，不应该在每个页面单独复制逻辑。

### Change Event

`change_events` 拥有用户可见事件和 unread 状态。它不是对象状态的权威来源，而是事实变化的通知/审计伴随物。

### Collector Snapshot

`collector_snapshots` 继续用于调试和脱敏来源检查。只要 fact tables/projections 已覆盖相同信息，产品页面就不应把 `normalized_json` 当主要业务状态。

## 共享接口设计

### Query Services：查询服务层

先新增 TypeScript 查询层，用现有 API 组合数据，不改变后端行为。

建议文件：

- `src/lib/queries/stationQueries.ts`
- `src/lib/queries/pricingQueries.ts`
- `src/lib/queries/dashboardQueries.ts`
- `src/lib/queries/keyPoolQueries.ts`
- `src/lib/queries/providerEditQueries.ts`

初始函数：

- `loadStationFactBundle(stationId)`
- `loadStationDetailBundle(stationId)`
- `loadAllStationPricingFacts()`
- `loadStationAssetWorkspace()`
- `loadKeyPoolWorkspace()`
- `loadProviderEditBundle(stationId)`

规则：

- 查询服务负责 `Promise.all` 组合和部分失败策略。
- 页面负责 loading state 和用户动作，不负责数据编排。
- 查询服务可以返回 raw facts，也可以返回 projection-ready maps。
- 先用 TypeScript 收敛，避免一开始就制造 Rust API churn。等消费者稳定后，再把必要 bundle 升级为 Tauri command。

### Projection Services：投影服务层

新增共享投影模块，尽量是纯函数。

建议文件：

- `src/lib/projections/groupFacts.ts`
- `src/lib/projections/balanceFacts.ts`
- `src/lib/projections/pricingFacts.ts`
- `src/lib/projections/stationFacts.ts`
- `src/lib/projections/runtimeSnapshot.ts`

初始函数：

- `buildCurrentStationGroupFacts(bindings, rates)`
- `latestGroupRatesByBindingOrHash(rates)`
- `buildStationGroupOptionsFromCurrentFacts(groupFacts)`
- `buildStationKeyCurrentFacts(keys, groupFacts)`
- `buildCurrentStationBalances(stations, balances)`
- `buildPricingCandidates(models, stations, groupFacts, pricingRules, modelEvidence)`
- `buildRuntimeRouteSnapshot(settings, aliases, keys, capabilities, health, groupFacts, pricingRules, balances)`

规则：

- 投影函数必须确定性、无副作用。
- 每个投影必须写清 fallback 顺序。
- 投影类型应暴露 `source` 和 `evidence`，让 UI 能解释值从哪里来。
- 页面级 view model 可以继续存在，但要消费投影，而不是直接消费多张事实表。

### Backend Shared Capabilities：后端共享能力

需要继续保留并增强 Rust service：

- `save_station_key_with_defaults`
- `list_station_group_options`
- `list_channel_monitor_summaries`
- 未来的 `compile_runtime_route_snapshot`

规则：

- 持久化和事务性操作放后端 service。
- 读侧先通过 TypeScript projection 收敛。
- 只有 runtime、性能或一致性需要时，再把稳定 projection 移到后端。

### API Fallback Wrapper

新增统一 preview fallback 包装：

- `invokeOrFallback(command, args, fallback)`
- `isInvokeUnavailable(error)`

规则：

- API module 可以提供浏览器预览 fallback。
- fallback 不能引入不同于真实 Tauri command 的业务规则。
- 复杂 fallback 应调用同一套 projection utilities。

### Shared Formatting And Labels

逐步统一：

- `readError`
- `toTime`
- `formatDateTime`
- `formatRelativeTime`
- `formatMoney`
- `formatRate`
- `formatMultiplier`
- `stationKeyStatusLabel`
- `collectorRunStatusLabel`
- `groupBindingStatusLabel`
- `balanceStatusLabel`
- status-to-tone maps

规则：

- formatter 不放业务 join 逻辑。
- 共享 status label 用来避免文案漂移；确实有差异的页面文案可以保留局部定义。

## 非破坏性迁移计划

### Stage 0：建立基线和安全网

不改生产行为。

新增或收紧测试：

- 价格页同一个当前分组 identity 不显示两次。
- 同一个站点/模型下多个真实不同分组仍然显示。
- 站点详情与价格页对当前分组状态和倍率选择一致。
- 站点资产列表余额优先使用最新 station-scope balance snapshot，再回退 station 字段。
- Key Pool 编辑无关字段时保留当前 group binding。
- Add Provider 远端分组同步保留 `group_binding_id` 和上游 group identity。
- 本地 proxy route candidate 仍然暴露 group binding、multiplier、pricing status、health facts。

再加字段所有权检查：如果新代码在未批准模块里直接读取兼容字段，测试应失败。第一版可以用源码扫描脚本实现，白名单只允许 `src/lib/projections/**`、明确的 migration/backfill 文件、以及尚未迁移的旧消费者路径。每迁走一个旧消费者，白名单必须同步收窄。

### Stage 1：低风险工具去重

先清理低风险重复：

- 页面本地 `readError` 改用 `src/lib/errors.ts`。
- 公共时间解析放到 `src/lib/time.ts`。
- 公共倍率/金额格式化放到 `src/lib/formatters.ts`。
- 共享 status label/tone 放到 `src/lib/statusLabels.ts`。

这一阶段不应改变行为。除非文件本身已有中文文案且明显 mojibake，否则不顺手改文案。

工具去重的验收不是“编译通过”，而是“输出保持一致”。每个被替换的 formatter、status label、`readError` 都要有小样例或源码负证明，确认页面显示文本、fallback 文案、数字精度和空值展示没有被顺手改掉。

### Stage 2：查询服务层

新增 TypeScript query services 包装现有调用：

- 价格页使用 `loadAllStationPricingFacts()`。
- 站点详情使用 `loadStationDetailBundle(stationId)`。
- 站点资产页使用 `loadStationAssetWorkspace()`。
- Add Provider 编辑页使用 `loadProviderEditBundle(stationId)`。

目标是减少重复 `Promise.all`，不改变事实语义。

### Stage 3：当前分组投影

新增 `buildCurrentStationGroupFacts` 和测试。

身份 fallback 顺序：

1. 当前 binding row id，也就是消费者看到的 `group_binding_id`
2. `group_key_hash`
3. `group_id_hash`
4. normalized `group_name`，仅作为 legacy fallback

`group_key_hash` 和 `group_id_hash` 不能互相替代：前者是本地稳定身份，后者是上游 group id 的脱敏身份。只有 projection 输出可以同时携带两者，页面不得自己决定二者等价。

倍率 fallback 顺序：

1. Binding `user_rate_multiplier`
2. Binding `effective_rate_multiplier`
3. Latest rate `user_rate_multiplier`
4. Latest rate `effective_rate_multiplier`
5. Binding `default_rate_multiplier`
6. Latest rate `default_rate_multiplier`
7. `null`

状态规则：

1. 当前状态以 `station_group_bindings.binding_status` 为准。
2. `missing` / `disabled` 不能被旧 rate record 掩盖。
3. rate history 可以解释新鲜度，但不能把 missing binding 复活成 available。

本阶段不删除、不重写已有数据。

### Stage 4：价格投影与价格页迁移

把价格候选构建从页面特定逻辑迁到 `pricingFacts.ts`。

规则：

- 行 identity 包含 model id 和 current group identity。
- 同一个当前分组不能因为 binding 和 rate record 都匹配而出现两次。
- 同站点/模型下多个真实不同分组仍然保留。
- 站点特定 provider 映射不能硬编码在页面里。必须存在时，应放在 provider metadata 或 group projection evidence 中。
- `pricingRules` 应用于模型证据和价格覆盖，不能被 `void input.pricingRules` 这种逻辑忽略。

迁移步骤：

1. 从 projection 构建 pricing candidates。
2. `PricingPage` 用 candidates 渲染现有 UI。
3. 测试通过后移除页面本地 group/rate matching。

这一阶段不得同时重排价格页 UI。先迁数据来源和去重规则，视觉结构保持原样，避免把架构回归和 UI 回归混在一起。

### Stage 5：站点详情和资产页迁移

站点详情 group rows 和站点资产 chips 都改用 current projections。

规则：

- 站点详情和资产列表对当前分组数量、missing 状态、倍率展示必须一致。
- snapshot 解析只作为老数据库没有 fact rows 时的 fallback。
- missing group 警告必须继续可见。

站点详情迁移时要特别保护现有刷新动作：余额刷新、分组采集、完整采集都必须保持旧数据可见，失败时只显示局部错误和 toast，不能因为 projection 迁移重新引入整页闪烁。

### Stage 6：Key Pool 和 Add Provider 迁移

分组选择和草稿合并改用共享 group option/current fact utilities。

规则：

- 创建/编辑 Key 时必须保留选中的 `group_binding_id`。
- 清除分组必须是显式动作。
- Key 兼容字段通过后端 workflow 更新，不在页面本地随意复制。
- 远端 Key 创建仍然由后端解析真实上游 group id。

### Stage 7：Runtime Snapshot

为本地 proxy 编译运行时输入。

输出应包含：

- snapshot id/version
- 生成时间
- station key candidates
- station id 和 base URL
- secret references，不包含明文 secret
- enabled 状态和 priority
- group binding id
- effective multiplier 和 source
- model allow/block/preferred lists
- pricing rule reference 或 pricing status
- balance status
- health/cooldown state
- route policy data

规则：

- proxy runtime 读取 snapshot 或 snapshot-producing service，不读取 UI view model。
- runtime snapshot 编译应可在不启动 proxy 的情况下测试。
- request logs 记录请求时实际使用的 route decision evidence。

### Stage 8：兼容字段复查

只有消费者迁移完成后才做：

- 生成兼容字段读写清单。
- 把字段标记为 active、compatibility cache、deprecated 或 removable。
- 写迁移说明。
- 只要老数据库状态或 runtime 路径仍然依赖字段，就不删除。

## 回归保护

### 必要测试类型

- group facts、balance facts、pricing candidates、runtime snapshot 的纯投影测试。
- 页面源码负证明测试，确认旧重复逻辑不再存在。
- 前端 view model 的 focused Node script tests。
- Rust transactional capabilities 的单元测试。
- 前端迁移后运行 `pnpm.cmd build`。
- Rust/service 改动后运行 `cargo check --manifest-path .\src-tauri\Cargo.toml`。

### 必须锁住的行为

- `/groups/available` 和 `/groups/rates` 发现的同一个分组只是一条当前分组，不是两行。
- 两个 display name 相同但上游 id 不同的分组不能被错误合并；只有 legacy name 数据时才允许按名字 fallback。
- 分组已经 missing 时，旧 rate history 不能让它显示为 available。
- Key 绑定分组后，编辑无关字段不会丢 binding。
- 用户可以显式清除 Key 的 group binding。
- 浏览器 preview 可以渲染页面，但 preview fallback 不定义业务真相。
- 本地 proxy route candidates 不因为 UI 投影迁移而静默改变。

## Review 与回滚策略

每个 stage 必须独立可 review、可回滚。

规则：

- 只精确 stage 任务路径。
- 不混入无关 dirty files。
- 消费者迁移完成前保留旧 command 和旧字段。
- 先加 additive module，再迁移读者。
- schema constraint 不在早期阶段改。
- 如果某个页面迁移出问题，可以只回滚该页面迁移；独立测试通过的 projection module 可以保留。
- 单个提交不要同时做“新增投影”“迁多个页面”“改 UI 文案”“清理旧代码”。如果必须触碰多个层，先提交 additive module，再提交单个消费者迁移，再提交旧逻辑删除。

## 非目标

- 不重做 UI 设计。
- 第一轮不删除现有字段。
- 不替换数据库。
- 不改成云服务或 SaaS 架构。
- 不新增账号、团队、支付、云同步、插件市场。
- 不复制 AGPL/LGPL 项目的实现。
- 不改变 Sub2API/NewAPI 语义，除非是明确的 adapter bugfix。

## 验收标准

这轮架构改造完成时，应满足：

- 页面通过 shared query services 获取数据，不再重复写大段 `Promise.all` 调用图。
- 当前分组、倍率、余额、价格状态由共享 projection utilities 生成。
- 价格页、站点详情、站点资产列表、Key Pool、Add Provider 对当前分组 identity 和倍率展示一致。
- 现有能力保持可用：远端 Key 同步、分组绑定、Key 编辑、价格比较、余额展示、变更中心、路由模拟、本地 proxy 路由。
- 兼容字段有清晰读写所有权，新代码不会随意直接消费。
- 陈旧页面本地重复逻辑和过期测试被删除或隔离。
- 验证覆盖前端 focused tests、必要 Rust tests，以及被触及层对应的 build/check。
