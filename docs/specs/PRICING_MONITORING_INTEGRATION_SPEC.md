# 价格分组与状态监控联动规范

状态：Implemented v2，已按实施计划完成并通过自动化验收；保留本文作为行为契约
日期：2026-08-03  
适用范围：价格 / 倍率页面、渠道状态页面、Station Key 分组绑定与监控状态读模型  
提案类型：跨域只读投影与价格页筛选升级  
替代关系：本规范进入实施后，取代价格页与状态监控联动相关的临时前端拼装方案；不替代状态监控 V2 规范。

参考规范：

- `docs/PROJECT_PLAN.md`
- `docs/PRODUCT_MODEL.md`
- `docs/specs/STATUS_MONITORING_REFACTOR_SPEC.md`
- `src/features/pricing/PricingPage.tsx`
- `src/features/pricing/pricingComparisonViewModel.ts`
- `src/lib/projections/pricingFacts.ts`
- `src-tauri/src/models/monitoring/read_model.rs`
- `src-tauri/src/persistence/stores/monitoring/status_read_repository.rs`

## 1. 执行摘要

价格域回答“哪个分组更便宜”，监控域回答“哪个 Key / Channel 最近是否可用”。两者通过一个后端拥有的轻量只读投影联动：`PricingGroupMonitorSummary`。

本提案不在 React 中临时合并两个完整工作区，也不新增持久化的 `group_status` 表。摘要根据分组绑定、Station Key、启用的 Monitor Definition、最新 Target Result 和运行中 Execution 派生，价格页只消费摘要。

核心不变量：

1. 监控结果只有一个事实来源，仍由状态监控 V2 读模型提供。
2. 价格页不读取 raw monitor run、request log 或完整时间桶。
3. 同一分组的代表监控选择规则确定、稳定、可测试，不依赖 SQL 或数组偶然顺序。
4. 联动状态不写入价格表，不维护第二套健康状态机。
5. 分组身份优先使用 `groupBindingId`，不默认使用分组名称模糊匹配。
6. 价格工作区和摘要工作区通过规范化 `groupRefsHash` 绑定，引用变化时不得复用旧摘要。

## 2. 背景与现状

当前价格页加载站点、Station Key、分组绑定、倍率和价格规则，再由前端投影成分组行。该投影适合展示价格事实，但价格行通常是站点级分组，没有直接的 `stationKeyId`。

当前状态页返回 `Monitor + Target` 维度的行；站点级监控还会展开到多个 Station Key。状态页工作区还包含 recent、小时桶、日桶、分页和排序，不适合作为价格页的轻量联动接口。

当前 Key 池已有单 Key 监控状态映射，但其选择策略是按 `updatedAt` 选择最近更新的监控。该策略不等同于本提案的“同分组第一把 Key”，不能直接复用为分组代表策略。

## 3. 目标

### 3.1 产品目标

- 价格页新增“状态”表头和状态徽标。
- 有 Key 但没有启用监控的分组明确显示“无监控”。
- 有多个 Key / Monitor 的同一分组只显示一个确定的代表状态。
- 支持按 Key、监控存在性和最近测试状态筛选。
- 状态显示最近检测时间，并可定位到渠道状态详情。
- 监控执行完成后，价格页在可见时能及时刷新状态。

### 3.2 工程目标

- 跨域逻辑集中在后端 query / projection 层。
- 后端批量查询，不产生逐分组或逐 Key 的 N+1 查询。
- DTO 只暴露价格页需要的稳定字段。
- 规则具有领域类型、纯函数测试和数据库 fixture 测试。
- 不引入数据库迁移，不复制监控状态，不扩大价格页对监控内部实现的依赖。

## 4. 非目标

本提案不包含：

- 重写完整价格分组投影为新的后端价格表读模型；
- 修改 Monitor Execution、Target Result、Probe Attempt 的写入流程；
- 新增通知、告警、Webhook 或公共状态页；
- 把价格状态列改造成路由健康的替代界面；
- 仅凭最近成功结果改变价格排序；
- 仅为了本功能新增持久化状态表；
- 仅凭分组名称猜测绑定关系并静默接受低置信匹配。

## 5. 领域边界与事实来源

| 数据 | 所属事实来源 | 联动层职责 |
|---|---|---|
| 价格、倍率、价格更新时间 | `pricing_rules`、`station_group_bindings`、`group_rate_records` | 读取并按现有规则展示 |
| 分组身份 | `station_group_bindings` | 解析价格行与 Key 的关系 |
| Key 是否存在、Key 顺序 | `station_keys` | 构造监控候选 |
| Monitor Definition 是否启用 | `channel_monitors` | 过滤监控候选 |
| 最近终态 | `channel_monitor_target_results` | 读取 latest outcome |
| 是否正在执行 | 运行中 Monitor Execution 读模型 | 覆盖展示状态，不改变 latest 终态 |
| 路由健康 | `station_key_health` | 本提案不复制、不修改 |

联动层只生成 `PricingGroupMonitorSummary`，不写入上述任何事实表。

## 6. 分组身份解析

### 6.1 价格行身份

每个价格行必须带有以下内部引用：

```text
stationId
groupBindingId: string | null
groupIdHash: string | null
groupKeyHash: string
```

前端必须将引用规范化为稳定键，格式固定为：

```text
station:{stationId}:binding:{groupBindingId}
station:{stationId}:group-id:{groupIdHash}
station:{stationId}:group-key:{groupKeyHash}
```

引用先按规范化键排序、去重，再参与 React Query key 和 `groupRefsHash` 计算。`groupBindingId` 非空时不得同时使用 group-id 或 group-key 作为主合并键；没有 binding id 时优先使用非空 `groupIdHash`，否则使用 `groupKeyHash`。

现有价格页的 `identityKey` 可以继续用于 UI 行 key，但跨域匹配不得只依赖显示名称或 UI 行 key。

### 6.2 Key 到分组的匹配优先级

在同一个 `stationId` 内，按以下顺序匹配：

1. `stationKey.groupBindingId == priceGroup.groupBindingId`；
2. Key 绑定指向 `key_binding` 时，沿 `parentGroupBindingId` 匹配站点级分组；
3. 没有可用 binding id 时，使用同站点、非空且唯一的 `groupIdHash`；
4. `groupName` 不作为默认唯一身份。

匹配结果必须记录：

```text
matchKind: exact_binding | parent_binding | group_id_hash | group_key_hash | unresolved
```

`group_key_hash` 只表示使用同站点唯一 `groupKeyHash` fallback；它不等价于 `group_id_hash`。`unresolved` 不得被当成“有 Key”或“有监控”，也不得因为同名而自动合并。若同一站点存在多个候选分组使用相同 hash，必须返回 `unresolved`，不能择一匹配。

### 6.3 引用规范化与哈希

规范化引用键必须使用以下字符串之一：

```text
station:{stationId}:binding:{groupBindingId}
station:{stationId}:group-id:{groupIdHash}
station:{stationId}:group-key:{groupKeyHash}
```

将规范化键按 UTF-8 字节序升序排序，以单个换行符连接，再计算 SHA-256 小写十六进制字符串作为 `groupRefsHash`。TypeScript 和 Rust 必须共享 contract fixture，禁止各自使用不同的 JSON 序列化或排序实现。

### 6.4 数据异常处理

- 价格行缺少稳定分组身份：仍可显示价格，但联动摘要返回 `unresolved`。
- Key 绑定不存在：不计入该分组的绑定 Key 数量。
- 分组已标记 `missing` 或 `disabled`：价格页按现有价格事实规则处理，联动层不自行恢复它。
- Station 被删除：相关监控由数据库级联删除，不返回悬空状态。

## 7. 代表监控选择策略

### 7.1 候选集合

一个价格分组的监控候选必须同时满足：

1. Key 与价格分组匹配；
2. Monitor Definition 的 `enabled = true`；
3. Monitor 目标属于该 Key，或是该 Station 的 station-wide Monitor；
4. Monitor 状态读模型能够识别该目标 Key。

停用 Monitor Definition 不算“有启用监控”。

station-wide Monitor 必须沿用现有 Channel Status 读模型的目标展开语义：摘要只能使用该 Monitor 对该具体 Key 产生的 Target Result，不得把一个没有 `station_key_id` 的站点级结果擅自复制给所有 Key。

### 7.2 稳定排序

候选按照以下字段升序排序，取第一项：

```text
station_key.priority
station_key.created_at
station_key.id
monitor.created_at
monitor.id
```

本规范冻结“第一把 Key”的定义为站点内可审计的 Key 顺序。除非新增产品决策并修改本规范，否则实现不得改成“最近成功”或“最早创建 Monitor”。

### 7.3 同一 Key 的多个 Monitor

同一 Key 有多个启用 Monitor 时，按 `monitor.created_at ASC, monitor.id ASC` 选择。不能按最近成功、最近更新时间或当前结果好坏切换代表 Monitor。

### 7.4 结果不会择优

如果代表候选的最新状态是 `missing`，即使同组第二个候选是 `available`，仍显示“未检测”。不得为了让状态看起来更好而选择成功项。

## 8. 状态语义

### 8.1 后端字段

`PricingGroupMonitorSummary` 至少包含：

```text
stationId: string
groupBindingId: string | null
groupIdHash: string | null
groupKeyHash: string
matchKind: exact_binding | parent_binding | group_id_hash | group_key_hash | unresolved
resolutionState: resolved | unresolved
hasBoundKey: boolean
boundKeyCount: number
enabledKeyCount: number
credentialedKeyCount: number
enabledMonitorDefinitionCount: number
monitoredKeyCount: number
testedKeyCount: number
representativeKeyId: string | null
representativeMonitorId: string | null
latestTargetResultId: string | null
latestOutcome: available | degraded | unavailable | skipped | missing
latestFailureKind: string | null
latestTerminalReason: string | null
running: boolean
checkedAtMs: number | null
latencyMs: number | null
generatedAtMs: number
```

`latestOutcome` 和 `running` 必须分开。运行中的 Monitor 不应覆写最近终态，但 UI 展示可以优先显示“检测中”。

字段定义：

- `boundKeyCount`：存在且成功匹配到该分组的 Station Key 记录数量，包含禁用 Key；
- `enabledKeyCount`：上述 Key 中 `enabled = true` 的数量；
- `credentialedKeyCount`：上述 Key 中存在可发送凭据的数量；
- `enabledMonitorDefinitionCount`：匹配到该分组的启用 Monitor Definition 数量；
- `monitoredKeyCount`：至少被一个启用 Monitor 覆盖的不同 Key 数量；
- `testedKeyCount`：至少存在一个已完成 Target Result 的不同 Key 数量；
- `hasBoundKey` 等价于 `boundKeyCount > 0`，不等价于 Key 可调度或有凭据。

### 8.2 展示状态

| 条件 | 展示 |
|---|---|
| `resolutionState = unresolved` | 无法关联 |
| `resolutionState = resolved` 且 `hasBoundKey = false` | 无 Key |
| `hasBoundKey = true` 且 `enabledMonitorDefinitionCount = 0` | 无监控 |
| `running = true` | 检测中 |
| 没有运行中且 `latestOutcome = missing` | 未检测 |
| `available` | 正常 |
| `degraded` | 降级 |
| `unavailable` | 失败 |
| `skipped` | 跳过 |

“无 Key”与“无监控”是资源存在性状态，不属于监控失败。

### 8.3 筛选语义

筛选由三个相互独立的维度组成，并与现有分组类型、站点和关键词筛选使用 AND：

```text
keyPresence: all | with_key | without_key | with_credentialed_key
monitorPresence: all | monitored | unmonitored
outcome: all | available | degraded | unavailable | skipped | missing | running | unresolved | unavailable_data
```

规则：

- `with_key` 表示 `hasBoundKey = true`；
- `with_credentialed_key` 表示 `credentialedKeyCount > 0`；
- `monitored` 表示 `enabledMonitorDefinitionCount > 0`，不表示已经成功运行；
- `unmonitored` 表示没有启用 Monitor Definition，停用定义不计入；
- `available` 按最近终态判断，运行中但最近终态仍为 `available` 时仍属于成功筛选；
- `running` 按 `running = true` 判断；
- 没有候选的分组不得因为 `latestOutcome = missing` 被误判为“已监控未检测”，必须先看 `enabledMonitorDefinitionCount`；
- `unresolved` 和 `unavailable_data` 只能被显式筛选，不能伪装成无 Key 或失败。

## 9. 后端读取接口

### 9.1 接口形式

新增价格域只读接口：

```text
load_pricing_group_monitor_status
```

建议输入：

```ts
type PricingGroupMonitorStatusInput = {
  schemaVersion: 1;
  groupRefsHash: string;
  groups: Array<{
    stationId: string;
    groupBindingId: string | null;
    groupIdHash: string | null;
    groupKeyHash: string;
  }>;
};
```

建议输出：

```ts
type PricingGroupMonitorStatusWorkspace = {
  schemaVersion: 1;
  generatedAtMs: number;
  groupRefsHash: string;
  requestedGroupCount: number;
  returnedGroupCount: number;
  omittedGroupCount: number;
  items: PricingGroupMonitorSummary[];
};
```

输入最多允许 500 个规范化分组引用。重复引用必须在 DTO 校验阶段去重或拒绝，不能让同一分组返回多份摘要。`groupRefsHash` 必须由规范化、排序后的引用计算，输出必须原样回显。

后端不得静默丢弃超限引用：要么完整返回，要么返回明确的 invalid-input 错误。`omittedGroupCount` 只允许在未来引入 cursor 后使用，当前版本必须为 `0`。

### 9.2 事务与查询

- 每次接口调用使用一个 `ReadSession`，分组、Key、Monitor 和 latest result 在该摘要快照中读取。它不宣称与前一个价格工作区请求属于同一数据库快照。
- 前端只合并 `groupRefsHash` 相同且引用完全匹配的摘要；价格工作区引用变化时，旧摘要必须丢弃并重新请求。
- 查询必须批量完成，不允许按 group、Key 或 Monitor 循环发 SQL。
- Repository 不得复用当前 `workspace_recent_results` 的逐 row 查询路径；必须新增批量 latest-result / running-state 查询。
- 500 个引用不得直接展开为超过 SQLite 变量预算的绑定参数；可以使用单 JSON 参数、SQLite `json_each` 或受控批次（每批最多 100 个引用），但必须保证完整覆盖。
- 不读取 recent、hourly、daily buckets，不读取完整 execution history。
- latest result 使用 `finished_at_ms DESC, id DESC` 的确定性排序。
- running execution 只返回是否存在，不返回完整运行对象。
- 接口返回 `generatedAtMs`，用于前端判断摘要是否来自同一次读取。

### 9.3 模块职责

建议新增或扩展以下职责边界：

```text
src-tauri/src/application/queries/pricing_group_monitor_status.rs
src-tauri/src/persistence/stores/monitoring/group_status_repository.rs
src-tauri/src/models/shared_capabilities.rs
src-tauri/src/ipc/dto/pricing_reads.rs
```

Repository 负责 SQL 行映射和批量读取；Application Query 负责分组候选、代表选择和状态 reducer；DTO 负责输入校验、哈希校验和序列化。React 不承担这些领域判断。

## 10. 前端集成

### 10.1 查询生命周期

价格页先读取现有价格工作区，再根据价格行生成稳定分组引用，读取监控摘要：

```text
pricing workspace
    -> stable group refs
    -> pricing group monitor status workspace
    -> merge by canonical group ref key and matching groupRefsHash
    -> view model filters and table
```

监控摘要查询：

- 仅在价格页可见时启用；
- 建议 `staleTime = 5s`、`refetchInterval = 5s`；
- 监控创建、更新、启停、手动执行完成或取消后主动失效；
- 摘要查询失败时，价格仍正常展示，状态列显示“暂不可用”，不得把请求错误伪装成“失败”；此状态不参与“仅成功 / 仅失败”筛选。

### 10.2 View Model

`PricingComparisonRow` 新增：

```ts
monitorSummary: PricingGroupMonitorSummary | null;
monitorDisplayState: unresolved | no_key | unmonitored | running | untested | available | degraded | unavailable | skipped | unavailable_data;
```

所有筛选先在 View Model 中完成，再计算当前筛选范围内的计数和最低倍率。表格组件不得直接读取原始 `ChannelStatusRow`。

### 10.3 表格与交互

- 在“倍率”与“最后变更时间”之间增加“状态”列。
- 状态徽标显示短文本，辅助信息显示代表 Key、代表 Monitor、最近检测时间和匹配方式。
- 点击状态徽标可以跳转到渠道状态页，并携带 `monitorId` 或 `stationKeyId` 深链；深链失败不影响价格页。
- 筛选区使用三个紧凑 Select 或一个分组筛选菜单，不改变现有搜索和分组类型筛选。
- 状态列不得改变价格排序；“正常”不是价格优先级。

## 11. 测试规范

### 11.1 纯函数测试

必须覆盖：

1. exact binding 匹配；
2. parent binding 匹配；
3. group id hash 兼容匹配；
4. 同名不同 binding 不合并；
5. 单 Key 单 Monitor；
6. 多 Key 同组按 priority / created_at / id 选代表；
7. 第一候选未检测、第二候选成功时仍返回未检测；
8. 同 Key 多 Monitor 按创建顺序选择；
9. station-wide Monitor 展开到分组 Key；
10. 停用 Monitor 不计入 `enabledMonitorDefinitionCount`；
11. running 与 latest outcome 同时存在；
12. 没有 Key、没有 Monitor、未检测三者不混淆；
13. 所有筛选组合使用 AND；
14. unresolved 不被计入“有 Key”或“有监控”。

### 11.2 Rust 持久化与查询测试

- 使用现有 monitoring fixtures 构造多 Key、多 Monitor 和 station-wide Monitor；
- 验证单次接口没有 N+1 查询路径；
- 验证 latest result 的时间和 id tie-break；
- 验证运行中的 execution 不会覆盖 latest terminal result；
- 验证 500 个输入引用的边界和重复引用处理；
- 验证规范化引用 hash、价格工作区变化后的旧摘要丢弃和完整批处理覆盖；
- 验证删除、解绑和旧绑定数据不会产生悬空 summary；
- 验证 `unresolved`、无 Key、无凭据 Key、禁用 Key 和 station-wide Monitor 计数；
- 验证返回 DTO 不含 API Key、Cookie、token 或原始响应正文。

### 11.3 TypeScript / UI 测试

- pricing view model 状态映射和筛选组合；
- 状态列显示和错误降级；
- 摘要查询刷新与监控 mutation invalidation；
- 状态跳转参数不包含秘密；
- 空、加载、失败、无匹配和长分组名称布局。

### 11.4 契约与验证

必须更新：

- Rust DTO serialization fixture；
- generated command registry / TypeScript binding；
- IPC command validation；
- `pnpm build`；
- 相关 Vitest；
- `cargo check`；
- 相关 Rust integration tests；
- architecture boundary checks。

## 12. 性能与容量约束

- 价格页首期最多处理现有价格工作区允许的 500 个分组引用。
- 后端一次批量读取，不允许每个分组一次查询；超过 SQLite 变量预算时必须内部批处理并合并，不能减少返回项。
- 摘要 DTO 不返回完整 monitor、attempt 或 bucket 历史。
- 当前版本超过 500 个分组必须明确报错；下一阶段增加 cursor，而不是静默截断或无限提高单次 DTO 上限。
- 必须使用 `EXPLAIN QUERY PLAN` 和生成数据验证索引，不能只凭代码阅读判断性能足够。
- 前端状态筛选必须是 O(rows)，不得在每个筛选条件中重复构造索引。

## 13. 可靠性与可维护性约束

### 13.1 不复制业务状态机

价格联动只能消费 `ChannelStatusOutcome` 和现有监控读模型。不得在价格模块重新计算连续失败、恢复阈值、健康写回或路由 cooldown。

### 13.2 不依赖偶然顺序

所有“第一项”必须显式排序并有 tie-break。SQL `ORDER BY`、Rust `sort_by` 和 TypeScript fallback 必须使用相同的字段定义。

### 13.3 不使用显示字段做身份

名称只用于搜索和展示。分组、Key、Monitor 的身份都必须使用稳定 id 或 hash。

### 13.4 不扩大 DTO 依赖

价格页不得依赖 `ChannelStatusWorkspace` 的时间桶、分页、执行详情或 attempt 历史。未来监控读模型升级时，价格页只需要保持摘要契约。

### 13.5 可解释性

摘要必须能够解释：

```text
这个状态来自哪个分组匹配、哪把 Key、哪个 Monitor、哪次 latest result、何时检测。
```

缺少这些信息的状态只能显示为“暂不可用”，不能显示为“失败”。

## 14. 实施阶段

### Phase 0：契约冻结

- 固化“第一把 Key”排序规则的 contract fixture 和纯函数测试；
- 冻结分组身份匹配顺序、`hasBoundKey` 语义和 station-wide 计数语义；
- 添加纯函数测试和 fixture；
- 不修改 UI。

退出条件：本规范中的排序和字段语义已经冻结，所有边界行为都有明确输入、输出和测试。

### Phase 1：后端摘要读模型

- 新增 Application Query、Repository、模型和 DTO；
- 更新 IPC registry 和 generated bindings；
- 完成查询、契约和 Rust 测试；
- 不新增数据库表或迁移。

退出条件：摘要查询可独立运行，且不读取完整状态工作区。

### Phase 2：价格页接入

- 增加摘要查询和可见页刷新；
- 增加 View Model 字段、状态列和筛选；
- 增加错误、加载和空状态；
- 增加深链入口。

退出条件：价格事实不受摘要接口失败影响，所有状态筛选测试通过。

### Phase 3：联动验证与清理

- 监控 mutation 失效价格摘要查询；
- 检查 Key 池、价格页和渠道状态页的状态文案是否一致；
- 删除任何临时前端 `ChannelStatusRow` 拼装代码；
- 更新 `PROJECT_PLAN.md`、`PRODUCT_MODEL.md` 和文档索引状态。

退出条件：跨域状态只有一个后端摘要路径，没有页面级特殊选择逻辑。

## 15. 验收标准

### 15.1 功能

- 价格页有状态表头；
- 有 Key 无启用监控显示“无监控”；
- 有启用监控未运行显示“未检测”；
- 运行中显示“检测中”；
- 多 Key 同分组严格使用代表策略；
- 无法解析的分组显示“无法关联”，不得显示为“无 Key”；
- 支持仅有 Key、仅有凭据 Key、仅有监控、仅成功、降级、失败、未检测等筛选；
- 价格排序和最低倍率计算不被监控状态改变。

### 15.2 数据一致性

- 价格页状态和渠道状态页的 latest outcome 一致；
- 不读取旧 `channel_monitor_runs` 作为新状态来源；
- 不存在重复的分组健康持久化字段；
- 分组名称变化不会导致错误合并；
- Monitor 停用后不会继续显示为启用监控。
- `groupRefsHash` 不一致时旧摘要不会合并到新价格行；
- 超过 500 个分组不会静默截断；
- station-wide Monitor 的 Definition 数量、覆盖 Key 数量和已测试 Key 数量语义一致。

### 15.3 工程质量

- 关键规则具有 Rust / TypeScript 单元测试；
- 关键 SQL 具有集成测试和分页边界测试；
- 关键 SQL 具有 `EXPLAIN QUERY PLAN` 和变量预算测试；
- IPC 类型由生成流程维护，无手写漂移；
- `pnpm build`、相关 Vitest、`cargo check` 和相关 Cargo 测试通过；
- 无 N+1 查询、无秘密泄露、无新增第二套健康状态机；
- React 页面不包含跨域数据库字段判断。

## 16. 回滚与后续扩展

本功能不新增数据表，因此回滚只需移除前端消费和 IPC 查询，不需要数据迁移回滚。监控原始事实和价格事实不受影响。

后续可扩展：

- 分组状态过期策略；
- 分组可用率摘要；
- 代表监控策略用户配置；
- 后端分页与服务端筛选；
- 价格与监控的联合排序解释。

这些能力必须扩展 `PricingGroupMonitorSummary` 或新的只读投影，不应重新把完整监控数据下沉到价格页。
