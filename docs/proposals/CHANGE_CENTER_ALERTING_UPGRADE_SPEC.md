# 变更中心告警闭环与提醒策略升级规范

状态：Draft，待设计评审与排期；不是当前实现基线  
日期：2026-08-08  
适用范围：变更中心、总览风险摘要、应用侧栏提醒、渠道监控与采集恢复联动、提醒设置、桌面系统通知  
提案类型：跨层领域模型、持久化、提醒策略与桌面交互升级  
替代关系：本规范获批并进入实施后，替代当前以 `change_events.status` 同时表达已读、忽略和问题生命周期的行为；历史设计记录仅用于迁移参考。

参考规范与当前事实：

- `AGENTS.md`
- `docs/README.md`
- `docs/PROJECT_PLAN.md`
- `docs/PRODUCT_MODEL.md`
- `docs/SCHEMA_UPGRADE_AUTHORING.md`
- `docs/proposals/STATUS_MONITORING_REFACTOR_SPEC.md`
- `src-tauri/src/persistence/migrations/0006_collectors_changes.sql`
- `src-tauri/src/persistence/stores/change_store.rs`
- `src-tauri/src/application/collectors.rs`
- `src/features/changes/ChangeCenterPage.tsx`
- `src/components/shell/AppShell.tsx`

## 1. 执行摘要

当前变更中心把一次性变化、当前故障、用户是否看过、用户是否忽略和问题是否恢复压缩进同一个 `change_events` 记录及单一 `status` 字段。除采集失败外，多数警告没有自动恢复路径；一次性倍率变化也会长期计入活跃警告；已读或已忽略事件复发时可能继续保持已读或忽略。前端只读取最新 200 条再进行本地汇总和分页，无法保证活跃风险计数完整。

本规范把变更中心升级为本地告警工作台，并明确五类职责：

1. `Change Event Occurrence`：不可变的变化或观测历史。
2. `Incident`：由当前事实驱动、可恢复和可复发的问题实例。
3. `Attention State`：用户是否已读、暂停提醒或静音。
4. `Alert Policy`：决定哪些问题何时形成告警、何时恢复和如何提醒。
5. `Notification Delivery`：每次本地提醒的计划、投递和结果审计。

产品入口新增：

```text
设置 -> 提醒与告警
变更中心 -> 提醒设置（跳转到同一配置页）
```

第一阶段只支持应用内提醒和桌面系统通知，不实现云通知、邮件、Webhook、脚本钩子或公共状态页。

## 2. 背景与问题陈述

### 2.1 当前状态模型

当前 `change_events.status` 允许：

```text
unread | read | dismissed | resolved
```

该模型混合了两个独立维度：

- 事实生命周期：问题是否仍存在、是否正在恢复、是否已经恢复；
- 用户关注状态：用户是否看过、是否确认、是否暂停或忽略提醒。

因此无法表达以下正常状态：

- 问题仍然存在，但用户已经确认；
- 问题仍然存在，但在指定时间内暂停提醒；
- 用户忽略了提醒，但系统随后已经自动恢复；
- 问题已恢复，之后再次发生并需要重新提醒；
- 同一问题持续发生多次，但没有产生多条重复问题记录。

### 2.2 当前恢复不完整

采集失败使用稳定 `dedupe_key`，成功或部分成功时会尝试将旧事件标记为 `resolved`。其他状态型事件没有对称闭环：

- `group_missing` 恢复时产生 `group_added`，旧问题不自动关闭；
- `key_group_unresolved` 恢复时产生 `key_group_bound`，旧问题不自动关闭；
- `group_rate_changed` 作为历史变化却长期保持活跃；
- 余额、价格过期、Key 健康和站点不可用没有统一的 incident 投影与恢复合同；
- `partial` 是否足以恢复采集失败没有按任务语义区分。

### 2.3 当前提醒能力不足

当前提醒主要是：

- AppShell 每 10 秒轮询最新变更；
- 侧栏显示未读数量；
- 进入变更中心时将当前所有未读事件标记为已读。

当前不存在：

- 可持久化的提醒规则；
- 连续发生次数或持续时间门槛；
- 连续恢复次数或恢复稳定时间；
- 重复提醒、冷却、安静时段；
- 恢复提醒；
- 桌面通知权限和投递审计；
- 按事件类型、站点或 Key 的覆盖配置。

### 2.4 当前读取不完整

`list_change_events` 固定返回最新 200 条，页面在这批数据上执行本地筛选、摘要和分页。超过窗口后：

- 活跃问题计数可能漏掉较旧但未恢复的问题；
- 筛选结果不是全量结果；
- 客户端分页只是 200 条窗口内分页；
- 清除历史成为缓解增长的主要方式，而不是 retention 策略。

## 3. 目标

### 3.1 产品目标

- 让变更中心首先回答“现在有哪些真实问题需要处理”。
- 所有状态型问题具有确定、可审计的触发和恢复条件。
- 问题恢复后活跃计数自动下降，复发后可以重新进入提醒周期。
- 用户可以配置提醒哪些问题、何时提醒、何时恢复、如何提醒和是否重复提醒。
- 用户标记已读、暂停和静音提醒时，不改变底层问题是否存在。
- 当前问题、变化历史和提醒规则在信息架构上清晰分离。
- 变更中心和总览消费同一个后端当前问题读模型。

### 3.2 工程目标

- 当前事实、incident 状态机和通知投递各有唯一 owner。
- 事件 occurrence 不可变；incident 通过稳定 condition key 聚合。
- 触发、恢复、复发、冷却和通知决策是确定性纯逻辑，可使用固定时钟测试。
- 后端拥有筛选、游标分页和聚合摘要，不再由前端用有限窗口推断全局状态。
- 复用现有 Station、Station Key、Collector、Pricing 和 Monitoring 事实，不建立第二套健康宇宙。
- 应用重启后保留 incident 状态、累计次数、冷却、暂停和投递历史。
- 时间型触发和恢复只在权威事实仍新鲜时执行；状态到期由持久化后台任务评估，不依赖下一次 UI 或业务观测。
- 任一已注册状态型事件始终能解析出一个有效策略；禁用提醒只能抑制 delivery，不能停止事实投影、恢复判断或当前问题计数。

### 3.3 用户配置目标

设置入口必须支持：

- 提醒哪些：事件类型、严重程度、站点、Key；
- 何时触发：立即、连续 N 次、持续 T 分钟；
- 何时恢复：连续正常 M 次、正常持续 T 分钟；
- 怎么提醒：变更中心、桌面系统通知；
- 重复提醒：不重复、每 T 分钟、严重程度升级时立即提醒；
- 抑制策略：冷却时间、暂停至指定时间、安静时段；
- 恢复提醒：独立开关；
- 严重度覆盖：针对特定事件或对象提升/降低一级。

## 4. 非目标

本提案不包含：

- 邮件、短信、云推送、Webhook 或公共状态页；
- 任意脚本、Shell 命令或插件回调；
- 正则表达式或通用布尔规则 DSL；
- 用提醒规则重新实现 Station Key 健康状态机；
- 用前端轮询结果决定问题恢复；
- 把请求日志的每次失败都写成一个 incident；
- 把操作系统通知当作唯一告警存储；
- 自动上传本地诊断、账号数据或通知历史；
- 在第一阶段提供团队协作、账号同步或跨设备规则同步。

## 5. 核心术语与领域边界

### 5.1 Change Event Occurrence

一次已经发生的变化、观测或状态转换，写入后不可修改。它负责回答：

```text
发生了什么、何时发生、涉及哪个对象、当时值是什么、来源是什么。
```

Occurrence 可以属于两类：

- `audit_change`：一次性变化，如倍率变化、模型新增；
- `condition_observation`：可能驱动 incident 的异常或恢复观测。

### 5.2 Incident

一个具有稳定身份的当前问题。它负责回答：

```text
问题是否仍存在、持续多久、发生多少次、严重程度如何、何时恢复。
```

Incident 不等同于 occurrence。一个 incident 可以关联多个 occurrence。

### 5.3 Attention State

用户对 incident 的关注和提醒处理状态：

- 是否已看；
- 是否暂停提醒；
- 是否被规则静音。

Attention State 不得修改 incident 的事实生命周期。

### 5.4 Alert Policy

一条可持久化规则，决定：

- 匹配哪些事实；
- 何时从正常进入待确认或活跃；
- 何时从活跃进入恢复确认或已恢复；
- 使用什么严重度；
- 使用哪些本地提醒渠道；
- 是否重复提醒或发送恢复提醒。

### 5.5 Notification Delivery

一次提醒投递的审计记录。它负责回答：

```text
何时计划、使用何种渠道、为什么发送或抑制、是否成功。
```

它不保存 secret、原始错误正文或完整上游 URL。

## 6. 事件分类与生命周期所有权

### 6.1 一次性变化

以下默认只进入变化历史，不进入活跃 incident：

| 事件 | 默认级别 | 默认提醒 |
|---|---|---|
| `group_added` | info | 否 |
| `rate_changed` / `group_rate_changed` 倍率下降 | info | 否 |
| `rate_changed` / `group_rate_changed` 倍率上涨 | warning | 可配置 |
| `price_changed` 价格下降 | info | 否 |
| `price_changed` 价格上涨 | warning | 可配置 |
| `model_added` | info | 否 |
| `model_removed` | warning | 可配置 |

一次性变化即使触发通知，也不得长期计入“当前问题”。

### 6.2 状态型问题

以下默认投影为 incident：

| 问题类型 | 默认严重度 | Condition Key 维度 | 默认恢复来源 |
|---|---|---|---|
| `balance_low` | warning | station + balance scope + currency | 新余额高于恢复阈值 |
| `balance_depleted` | critical | station + balance scope + currency | 新余额高于耗尽恢复阈值 |
| `group_missing` | warning | station + group stable identity | 成功采集后 group available |
| `key_group_unresolved` | warning | station key | binding bound/available |
| `price_expired` | warning | station + group + model | 新鲜完整价格可用 |
| `key_invalid` | critical | station key + health dimension | 共享健康状态机恢复 |
| `collector_failed` | warning | station + task type | 同任务确定性成功 |
| `station_down` | critical | station + endpoint revision | 当前 revision 恢复可用 |
| `route_impacted` | warning/critical | route scope + reason code | 当前投影不再受影响 |

该表是事件注册表的首期基线。实现新增事件类型时，必须同时声明：

- 分类；
- condition key；
- 严重度；
- 异常判据；
- 恢复判据；
- 是否允许用户覆盖；
- retention 类别；
- 敏感字段约束。

未注册事件只能作为 `audit_change/info` 保存，不能默认成为长期活跃警告。

### 6.3 恢复必须来自权威事实

- Collector 问题只由 collector task state 或同事务 apply result 恢复。
- Key 健康只由共享 `station_key_health` 状态转换结果恢复。
- 余额问题只由规范化 balance projection 恢复。
- 价格问题只由规范化 pricing projection 恢复。
- 路由影响只由当前 route candidate/current projection 恢复。
- 前端打开页面、刷新查询或点击“确认”不得恢复问题。

## 7. Incident 状态机

### 7.1 生命周期状态

```text
pending | open | recovering | resolved
```

含义：

- `pending`：观察到异常，但尚未达到触发次数或持续时间；
- `open`：达到触发条件，是当前活跃问题；
- `recovering`：观察到正常，但尚未达到恢复稳定条件；
- `resolved`：达到恢复条件，不再计入当前问题。

### 7.2 状态转换

```text
normal/resolved --abnormal--> pending
pending --trigger satisfied--> open
pending --normal before trigger--> resolved/suppressed observation
open --abnormal--> open (update count and last_seen)
open --normal--> recovering
recovering --recovery satisfied--> resolved
recovering --abnormal--> open
resolved --new abnormal--> pending or open
```

立即触发规则允许 `resolved -> open`，但仍必须创建新的 occurrence。

### 7.3 触发条件

规则支持以下模式：

```text
immediate
consecutive_occurrences(N)
active_duration(T)
```

首期一个 policy 只能选择一种主要触发模式，避免隐含 AND/OR 语义。后续如需组合条件，必须升级 schemaVersion 并更新本规范。

约束：

- `N` 范围：1 到 100；
- `T` 范围：1 分钟到 30 天；
- 连续次数只能由同 condition key、同 endpoint revision 或同事实版本序列累计；
- 来源任务失败或缺失数据不得被当作健康观测重置计数。

时间型触发合同：

- 事件注册表为每个状态型事件声明 `fact_freshness_seconds`；
- 异常 observation 写入 `fact_fresh_until_ms = observed_at_ms + fact_freshness_seconds`；
- `active_duration(T)` 写入 `next_state_evaluation_at_ms = pending_since_ms + T`；
- 到期 worker 只有在 `now <= fact_fresh_until_ms`，且没有相反 observation 时才能将 `pending` 转为 `open`；
- 若到期时事实已陈旧，incident 保持 `pending`，清空到期任务，等待新鲜权威 observation；不得依据旧值开告警；
- 新 observation 变为正常时取消 pending trigger deadline。

### 7.4 恢复条件

规则支持：

```text
consecutive_healthy(M)
healthy_duration(T)
```

约束：

- `M` 范围：1 到 100；
- `T` 范围：1 分钟到 30 天；
- 没有新鲜权威事实时保持原状态，不推断恢复；
- Collector `partial` 只有在目标 task 的恢复合同明确允许时才算健康；
- endpoint revision 必须进入 condition key 的身份维度；旧 revision 的恢复观测不得关闭新 revision incident。

时间型恢复合同：

- 健康 observation 写入自己的 `fact_fresh_until_ms`；
- `healthy_duration(T)` 写入 `next_state_evaluation_at_ms = healthy_since_ms + T`；
- 到期 worker 只在最新健康事实仍新鲜、期间没有异常 observation 时才能将 `recovering` 转为 `resolved`；
- 到期但事实陈旧时保持 `recovering`，不推断恢复，也不重复安排无界 timer；
- 任何新异常 observation 取消 recovery deadline 并立即回到 `open`。

### 7.5 复发

Resolved incident 再次异常时：

- 保留同一稳定 condition key；
- 增加 `episode_number`；
- 清空上一 episode 的触发/恢复计数；
- 创建新的 occurrence；
- 重新执行当前有效 policy；
- 不继承上一 episode 的 `seen`；
- `muted policy` 和尚未到期的全局暂停仍然生效。

### 7.6 严重度升级与降级

Incident 先由事件注册表默认值和当前事实计算不可被 policy 改写的 `base_severity`；再由 policy override 计算用于展示、聚合和投递的 `severity`：

```text
severity = clamp(base_severity + configured offset, info..critical)
```

- offset 只允许 `-1 | 0 | +1`；
- critical 不得提升，info 不得降低；
- 严重度升级可以绕过普通重复提醒间隔，但仍受安静时段的 critical 规则约束；
- 严重度降低不自动解决 incident。

## 8. 用户关注状态

### 8.1 Attention 字段

每个当前 episode 至少保存：

```text
seen_at_ms: number | null
snoozed_until_ms: number | null
```

### 8.2 行为语义

- 打开变更中心只更新页面级 `last_seen_cursor`，不自动标记全部问题为已读。
- 打开详情可标记该 incident 为 seen。
- “确认”表示用户知道问题存在，不表示已经恢复。
- “暂停提醒”只抑制后续 delivery，incident 仍在当前问题列表。
- 暂停到期后，如问题仍 open，按重复提醒规则重新评估。
- “永久忽略此类问题”必须修改或新建 Alert Policy，不能只改变单条 incident。
- “手动解决”仅允许没有权威自动恢复来源的注册事件；有自动恢复来源的事件只提供已读、暂停和跳转处理。

## 9. 提醒策略模型

### 9.1 规则层级

规则优先级固定为：

```text
指定 Key > 指定站点 > 事件类型 > 全局默认
```

同一层级多个规则同时匹配时：

1. 更具体的 event type 优先于 severity-only；
2. 更高的显式 `minimumSeverity` 优先于较低阈值或无阈值；这样可同时定义 `warning` 与 `critical` 的不同默认行为；
3. 显式 priority 数字升序；
4. `created_at_ms ASC, id ASC` 作为稳定 tie-break；
5. 只选择一条 effective policy，不合并字段。

设置页必须展示“当前生效规则来源”，避免用户无法解释结果。

### 9.1.1 匹配语法与约束

`scopeKind` 表示规则的主作用域；`eventType` 可以是 Station/Key 规则的附加限制：

| `scopeKind` | 必填字段 | 必须为 `null` 的字段 | 可选附加限制 |
|---|---|---|---|
| `global` | 无 | `stationId`、`stationKeyId` | `eventType`、`minimumSeverity` |
| `event_type` | `eventType` | `stationId`、`stationKeyId` | `minimumSeverity` |
| `station` | `stationId` | `stationKeyId` | `eventType`、`minimumSeverity` |
| `station_key` | `stationKeyId` | 无 | `eventType`、`minimumSeverity` |

`station_key` 规则若同时填写 `stationId`，保存时必须验证该 Key 属于该 Station；对象删除后规则进入 `orphaned`，不再匹配。规则匹配必须满足：

```text
enabled
AND object is inside the declared scope
AND eventType is null or equals incident.eventType
AND minimumSeverity is null or incident.baseSeverity >= minimumSeverity
```

有效规则按以下稳定元组选择一条，禁止字段合并：

```text
(scope rank DESC, eventType specified DESC, minimumSeverity rank DESC, priority ASC, created_at_ms ASC, id ASC)
```

其中 scope rank 为 `station_key=3`、`station=2`、`event_type=1`、`global=0`；minimum severity rank 为 `critical=3`、`warning=2`、`info=1`、`null=0`。DTO、数据库 CHECK、应用校验和前端表单必须执行同一份约束；不能由 `null` 的偶然组合推断规则语义。

### 9.2 Alert Policy 字段

建议领域结构：

```ts
type AlertPolicy = {
  id: string;
  name: string;
  enabled: boolean;
  scopeKind: "global" | "event_type" | "station" | "station_key";
  eventType: string | null;
  stationId: string | null;
  stationKeyId: string | null;
  minimumSeverity: "info" | "warning" | "critical" | null;
  severityOffset: -1 | 0 | 1;
  triggerMode: "immediate" | "consecutive_occurrences" | "active_duration";
  triggerCount: number | null;
  triggerDurationSeconds: number | null;
  recoveryMode: "consecutive_healthy" | "healthy_duration";
  recoveryCount: number | null;
  recoveryDurationSeconds: number | null;
  inAppEnabled: boolean;
  desktopEnabled: boolean;
  repeatMode: "never" | "interval" | "severity_escalation" | "interval_and_escalation";
  repeatIntervalSeconds: number | null;
  cooldownSeconds: number;
  recoveryNotificationEnabled: boolean;
  quietHoursPolicy: "inherit" | "respect" | "bypass_for_critical";
  priority: number;
  revision: number;
  createdAtMs: number;
  updatedAtMs: number;
};
```

`revision` 每次语义字段变化时递增。policy 删除采用 disabled/tombstone 语义，直至不存在引用它的 retention 数据；不得物理删除后让历史 delivery 失去解释来源。

### 9.2.1 系统默认策略

Policy resolver 必须始终返回一个有效策略。用户规则均不匹配、被停用或成为 orphaned 时，使用不可删除、随应用版本化的 `system_default` profile；它按 `base_severity` 提供第 9.4 节的默认值和事件注册表声明的更安全例外。`system_default` 不是普通可删除的 `alert_policies` 行，但在 incident/delivery snapshot 中以稳定 `policy_id` 和 profile revision 记录，确保历史可解释。

“恢复默认”只删除或停用用户覆盖规则，不得删除该 fallback。`alertingEnabled=false`、channel 开关、暂停和安静时段只影响 delivery planner；incident projector 仍继续创建 occurrence、计算状态、恢复问题并更新 current read model。

### 9.2.2 Policy 变更与重算

Policy 创建、更新、停用、删除及全局提醒设置变更后，后台必须在同一配置 mutation 后安排有界 reconcile：

1. 重新解析所有 `pending`、`open`、`recovering` incident 的 effective policy；
2. 重新计算未来 `next_state_evaluation_at_ms` 和 `next_notification_at_ms`；
3. 未 claim 的 scheduled delivery 使用新 policy 取消、抑制或替换；
4. 已 claim 或已 delivered 的 desktop delivery 保持其创建时的 policy snapshot，只完成结果记录；policy revision 变化本身不得为已通知 episode 重发 opened delivery；
5. 每个 delivery 保存 `policy_id`、`policy_revision` 和最小 effective policy snapshot，供历史解释；
6. reconcile 有分页、单实例和进度上限；完成前 UI 显示“规则正在应用”。

触发或恢复模式、次数、时长发生变化时，不得用配置修改前的 observation 回放出新的 open 或 resolved。reconcile 必须为 `pending` / `recovering` 建立新的 lifecycle evaluation epoch：仅当最新权威事实仍新鲜时，将其作为第一个样本，并从配置提交时间重新起算计数和持续时间；`open` 不会因规则修改自动关闭，`resolved` 不会因规则修改自动复发。incident 保存该 epoch 的 effective policy fingerprint，后续 observation 只能与相同 fingerprint 的计数累加。

`change_incidents.policy_id` 仅表示最近一次解析结果，不能成为后续决策的唯一来源。

### 9.3 全局提醒设置

建议领域结构：

```ts
type AlertingSettings = {
  revision: number;
  alertingEnabled: boolean;
  inAppEnabled: boolean;
  desktopEnabled: boolean;
  recoveryNotificationsEnabled: boolean;
  globalPausedUntilMs: number | null;
  quietHoursEnabled: boolean;
  quietHoursStartLocal: string | null;
  quietHoursEndLocal: string | null;
  quietHoursTimeZone: string;
  criticalBypassesQuietHours: boolean;
  historyRetentionDays: number;
  deliveryRetentionDays: number;
};
```

全局设置的语义字段修改必须递增 `revision`，并以乐观并发方式保存。`alertingEnabled` 的语义是全局 delivery kill switch，而不是 incident engine kill switch。关闭期间产生的 delivery 应以 `global_disabled` 抑制并保留审计；重新开启后仅按当时的 repeat/cooldown 规则有界重评估，不回放关闭期间的全部提醒。

### 9.4 默认值

建议首次启用默认值：

| 配置 | 默认值 |
|---|---|
| 提醒总开关 | 开 |
| 变更中心提醒 | 开 |
| 桌面通知 | 关，用户显式开启并授权 |
| warning 触发 | 连续 2 次 |
| critical 触发 | 立即 |
| 恢复 | 连续正常 2 次 |
| 重复提醒 | 不重复 |
| 严重度升级提醒 | 开 |
| 恢复提醒 | 开 |
| 冷却时间 | 30 分钟 |
| 安静时段 | 关 |
| occurrence 历史 | 90 天 |
| delivery 历史 | 30 天 |

事件注册表可以为特定类型声明更安全的内置默认值，例如余额耗尽和站点不可用立即触发。用户规则覆盖后必须在 UI 中明确显示。

## 10. 与现有健康和采集阈值的关系

### 10.1 单一健康状态机

渠道监控已有 `healthFailureThreshold` 和 `healthRecoveryThreshold`，它们决定共享 Station Key 健康何时转换。Alert Policy 不得重新按 raw probe 次数计算另一套 Key 健康。

对于 `key_invalid`：

```text
Monitor/Proxy observations
  -> shared station_key_health transition
  -> incident observation
  -> alert policy delivery decision
```

设置页可以展示并跳转到健康阈值配置，但必须标注“健康判定”和“提醒策略”是不同职责。

### 10.2 余额阈值

余额低阈值继续由 Station/Settings 的权威经济配置拥有。Alert Policy 只决定：

- 余额状态转换后是否提醒；
- 是否需要持续一段时间；
- 重复提醒和恢复提醒。

Alert Policy 不新增第二个金额阈值。

### 10.3 Collector 失败

Collector task state 已保存连续失败信息。Incident projector 应消费其确定性任务状态，而不是从 change event 数量反推失败次数。

## 11. 通知渠道与投递语义

### 11.1 应用内提醒

应用内提醒是必备渠道，表现为：

- 侧栏当前问题徽标；
- 变更中心当前问题列表；
- 可选的本地 toast，仅用于新的高优先级 incident，不替代持久列表。

徽标建议默认统计：

```text
open 且当前 episode 未 seen 的 warning/critical incident 数量
```

不得把普通 info occurrence 计入风险徽标。

### 11.2 桌面系统通知

桌面通知为显式 opt-in：

- 首次开启时请求操作系统权限；
- 权限拒绝或平台不可用时，保存 `desktopEnabled = false` 或明确 unavailable 状态；
- 自动退化为应用内提醒；
- 点击通知打开应用并深链到对应 incident；
- 通知正文只使用脱敏标题、对象显示名和持续时间；
- 不展示 API Key、Cookie、token、完整 URL、原始错误正文或请求正文。

实现阶段可引入官方 Tauri notification plugin，但必须同步更新依赖、capability、ACL、安全检查和许可证审计。

### 11.3 Delivery 状态

```text
scheduled | claimed | delivered | suppressed | failed | outcome_unknown
```

合法转换为 `scheduled -> claimed -> delivered | failed | outcome_unknown`，以及仅在固定重试预算内的 `outcome_unknown -> scheduled`。`delivered`、`suppressed` 和 `failed` 均为终态；已 claim 的 delivery 在策略修改时仍按本节完成结果记录，而不被 reconcile 覆盖。

`suppressed_reason` 使用稳定枚举：

```text
global_disabled
channel_disabled
permission_denied
quiet_hours
global_pause
incident_snoozed
cooldown
repeat_disabled
policy_muted
stale_episode
```

投递失败不得改变 incident 状态。失败记录保留脱敏 error code，不保存操作系统原始敏感正文。

### 11.3.1 外部投递的 crash-boundary 语义

应用内提醒是数据库读模型，不需要调用外部系统；desktop notification 是不可事务化副作用。对 desktop channel，本规范采用：

```text
正常运行：同一 delivery_key 至多投递一次。
进程在 OS 调用后、结果落库前崩溃：允许 best-effort at-least-once 重试，用户可能看到一次重复通知。
```

因此不得承诺跨崩溃边界的 exactly-once。每个 logical delivery 使用唯一：

```text
delivery_key = incident_id + episode_number + channel + delivery_kind + delivery_sequence
```

`delivery_sequence` 在每个 `(incident_id, episode_number, channel, delivery_kind)` 中单调递增。Policy revision 只记录在 delivery snapshot 中：这样同一 episode 可以有多次 repeat delivery，而编辑 policy 不会自动制造新的 opened delivery。

Worker 必须用带 lease 的原子 claim 获取投递权：

1. `scheduled -> claimed` 时写入随机 `claim_token`、`claimed_at_ms`、`lease_expires_at_ms` 并递增 `attempt_count`；
2. 只有持有相同 token 的 worker 可以写入 `delivered` 或 `failed`；
3. `claimed` lease 过期先转换为 `outcome_unknown`，保留未知发生时间和 claim token；仅该状态可按固定、有限的退避次数转换回同一行 `scheduled`，不得创建新的 logical delivery 或递增 delivery sequence；
4. 适配器能确定未调用 OS 或得到确定拒绝时写入终态 `failed`；未知结果重试耗尽后也写入 `failed`，使用稳定 error code；
5. retry 上限、退避和 lease 时长为版本化常量，不作为首期用户策略项；
6. 因 crash 重试的可能重复必须在 delivery detail 中可解释；
7. 不因单次 OS 投递失败改变 incident lifecycle。

### 11.4 重复提醒

重复提醒只针对仍为 `open` 的当前 episode：

- `never`：首次提醒后不重复；
- `interval`：距上次成功或确定抑制评估达到间隔后提醒；
- `severity_escalation`：严重度提升时立即提醒；
- `interval_and_escalation`：两者都支持。

调度必须使用持久化 `next_notification_at_ms` 或可重建字段，不能依赖 React timer。

### 11.5 安静时段

- 使用本地时区 ID 和本地时间保存用户意图；
- 支持跨午夜时间段；
- DST 切换必须使用时区库计算，不能固定加减小时；
- 被抑制的通知不排队形成恢复后的通知风暴；
- 安静时段结束后，只对仍 open 且满足 repeat policy 的 incident 重新评估一次；
- critical 是否绕过安静时段由全局设置和 policy 决定。

### 11.6 运行时可用性边界

提醒只在 Relay Pool Desktop 进程仍运行时调度和投递。窗口隐藏到 tray 时进程继续运行，可以继续提醒；用户选择退出应用或 tray behavior 为 `disabled` 后，应用不承诺离线提醒。下次启动时：

- 恢复未完成的状态与 claim；
- 对仍 open 的 incident 执行一次有界重评估；
- 遵循 cooldown、quiet hours 和 delivery_key；
- 不补发所有离线期间错过的通知，不形成通知风暴。

## 12. 设置入口与交互规范

### 12.1 信息架构

在设置页面新增一级入口：

```text
提醒与告警
```

由于规则配置较多，不把完整表单直接堆叠在通用 `SettingsPage`。设置页显示紧凑入口和摘要，进入独立设置工作区。变更中心工具栏提供齿轮图标“提醒设置”，跳转到同一工作区。

### 12.2 设置工作区结构

页面分四个不嵌套的全宽区域：

1. 通知方式；
2. 默认规则；
3. 提醒规则；
4. 时间与保留策略。

#### 通知方式

- 提醒总开关；
- 变更中心提醒开关；
- 桌面通知开关；
- 当前系统权限状态；
- 测试通知操作；
- 恢复提醒总开关。

#### 默认规则

- warning 默认触发模式；
- critical 默认触发模式；
- 默认恢复模式；
- 默认重复提醒；
- 默认冷却时间；
- critical 安静时段行为。

#### 提醒规则

使用高密度表格展示：

```text
启用 | 名称 | 范围 | 事件 | 触发 | 恢复 | 渠道 | 重复 | 来源/优先级 | 操作
```

支持创建、编辑、启停、复制和删除。删除使用确认对话框。内置默认规则不可删除，只能创建覆盖规则。

#### 时间与保留策略

- 安静时段；
- 全局暂停至指定时间；
- 立即恢复提醒；
- occurrence 历史保留天数；
- delivery 历史保留天数。

### 12.3 规则编辑器

规则编辑器按以下顺序展示：

```text
适用范围 -> 触发条件 -> 恢复条件 -> 提醒策略 -> 确认
```

控件要求：

- 事件类型和对象范围使用 Select；
- 单一触发/恢复模式使用 segmented control 或 radio group；
- 是否启用渠道和恢复提醒使用 Switch；
- 次数使用 stepper/number input；
- 时间使用数值 + 单位选择；
- 严重度覆盖使用 `-1 / 默认 / +1` segmented control；
- 所有数值即时校验并显示确定错误；
- 不用自由文本表达 condition key 或内部 ID。

### 12.4 设置预览

保存前展示自然语言摘要，例如：

```text
当“生产中转站”的采集任务连续失败 3 次时，
在变更中心和桌面通知中提醒；问题持续期间每 60 分钟重复一次；
连续成功 2 次后恢复，并发送恢复通知。
```

预览由结构化字段生成，不允许作为后端实际规则输入。

### 12.5 状态覆盖

设置 UI 必须覆盖：

- loading；
- empty（只有内置默认规则）；
- query error；
- save error；
- desktop permission denied；
- notification unsupported；
- disabled；
- narrow window；
- 长站点名和 Key 名；
- 被更高优先级规则覆盖。

## 13. 变更中心 UI 升级

### 13.1 顶层视图

变更中心提供三个视图：

```text
当前问题 | 变化历史 | 提醒记录
```

提醒规则不在这里维护，使用“提醒设置”跳转。

### 13.2 当前问题

摘要只统计后端 incident 聚合：

- 严重；
- 警告；
- 待确认；
- 已暂停提醒。

每行至少显示：

```text
严重度 | 状态 | 标题/对象 | 持续时间 | 当前 episode 次数 | 最近发生 | 提醒状态 | 操作
```

操作：

- 查看详情；
- 确认；
- 暂停提醒；
- 跳转相关 Station/Key/Channel/Pricing/Routing；
- 仅对允许手动恢复的类型显示“标记已解决”。

### 13.3 Incident 详情

详情 drawer/page 展示：

- 当前状态和严重度；
- 首次、最近发生和持续时间；
- 当前 episode 与累计 occurrence 次数；
- 触发条件及满足过程；
- 恢复条件及当前进度；
- 当前有效 policy 和来源；
- 最近脱敏 occurrences；
- 最近提醒投递及抑制原因；
- 相关对象深链；
- 建议动作。

建议动作来自注册表枚举和本地化模板，不存储可执行脚本。

### 13.4 变化历史

- 展示 audit changes 和 condition observations；
- 后端筛选和 cursor pagination；
- 支持事件类型、严重度、对象、来源和时间范围；
- 展示 old/new diff、impact、source 和关联 incident；
- 不使用“活跃”概念；
- 清除全部历史不再是页面主操作。

### 13.5 提醒记录

- 展示 delivery 时间、incident、渠道、结果和抑制原因；
- 不展示操作系统原始错误正文；
- 支持按渠道和结果筛选；
- 主要用于解释“为什么提醒/为什么没提醒”。

## 14. 数据模型与 SQLite 设计

具体表名可在实现设计中调整，但职责不得合并回单一 `change_events.status`。

### 14.1 `change_event_occurrences`

建议字段：

```text
id TEXT PRIMARY KEY
source_observation_key TEXT NOT NULL UNIQUE
event_type TEXT NOT NULL
category TEXT NOT NULL CHECK (... audit_change, condition_observation)
observation_kind TEXT NOT NULL CHECK (... abnormal, healthy, change)
severity TEXT NOT NULL
condition_key TEXT
incident_id TEXT
episode_number INTEGER
object_type TEXT NOT NULL
object_id TEXT
station_id TEXT
station_key_id TEXT
pricing_rule_id TEXT
request_log_id TEXT
source TEXT NOT NULL
reason_code TEXT
old_value_json TEXT
new_value_json TEXT
impact_json TEXT
observed_at_ms INTEGER NOT NULL
created_at_ms INTEGER NOT NULL
```

约束：

- occurrence 不执行 update；
- `source_observation_key` 由 producer 以来源、对象身份和来源版本/序列构造；同一键的重复 apply 必须返回已有 occurrence，不能依赖内存去重；
- JSON 只能存脱敏结构化事实；
- 不对高基数错误正文建索引；
- `condition_key` 使用稳定内部 ID/hash，不含 URL 或 secret。

### 14.2 `change_incidents`

建议字段：

```text
id TEXT PRIMARY KEY
condition_key TEXT NOT NULL UNIQUE
event_type TEXT NOT NULL
lifecycle_state TEXT NOT NULL CHECK (... pending, open, recovering, resolved)
base_severity TEXT NOT NULL
severity TEXT NOT NULL
object_type TEXT NOT NULL
object_id TEXT
station_id TEXT
station_key_id TEXT
policy_id TEXT
policy_revision INTEGER
lifecycle_policy_fingerprint TEXT NOT NULL
episode_number INTEGER NOT NULL
first_seen_at_ms INTEGER NOT NULL
last_seen_at_ms INTEGER NOT NULL
opened_at_ms INTEGER
recovering_at_ms INTEGER
resolved_at_ms INTEGER
occurrence_count INTEGER NOT NULL
episode_occurrence_count INTEGER NOT NULL
consecutive_abnormal_count INTEGER NOT NULL
consecutive_healthy_count INTEGER NOT NULL
pending_since_ms INTEGER
healthy_since_ms INTEGER
last_observation_id TEXT
last_observation_summary_json TEXT NOT NULL
fact_fresh_until_ms INTEGER
next_state_evaluation_at_ms INTEGER
last_notification_at_ms INTEGER
next_notification_at_ms INTEGER
version INTEGER NOT NULL
created_at_ms INTEGER NOT NULL
updated_at_ms INTEGER NOT NULL
```

`last_observation_summary_json` 是已脱敏、长度受限的去范式化摘要，用于 occurrence 被 retention 清理后仍能解释当前/已解决 incident。`last_observation_id` 不作为 retention 期间必须保持的外键。`lifecycle_policy_fingerprint` 标识累积计数和 deadline 所依据的有效策略版本；它在新的 lifecycle evaluation epoch 开始时更新。`version` 用于乐观并发或幂等状态转换。Incident projector 必须在一个写事务中写 occurrence、更新 incident 和安排 delivery。

### 14.3 `incident_attention`

建议字段：

```text
incident_id TEXT NOT NULL
episode_number INTEGER NOT NULL
seen_at_ms INTEGER
snoozed_until_ms INTEGER
updated_at_ms INTEGER NOT NULL
PRIMARY KEY (incident_id, episode_number)
```


### 14.4 `alert_policies`

结构化列保存匹配、触发、恢复和投递字段。禁止把整条规则只保存为任意 JSON 并在前端解释。可以为未来兼容增加有版本的扩展 JSON，但当前字段必须由后端验证。

### 14.5 `notification_deliveries`

建议字段：

```text
id TEXT PRIMARY KEY
delivery_key TEXT NOT NULL UNIQUE
incident_id TEXT NOT NULL
episode_number INTEGER NOT NULL
delivery_sequence INTEGER NOT NULL
policy_id TEXT
policy_revision INTEGER
policy_snapshot_json TEXT NOT NULL
channel TEXT NOT NULL CHECK (... in_app, desktop)
delivery_kind TEXT NOT NULL CHECK (... opened, repeated, escalated, recovered, test)
status TEXT NOT NULL CHECK (... scheduled, claimed, delivered, suppressed, failed, outcome_unknown)
scheduled_at_ms INTEGER NOT NULL
claim_token TEXT
claimed_at_ms INTEGER
lease_expires_at_ms INTEGER
attempt_count INTEGER NOT NULL DEFAULT 0
attempted_at_ms INTEGER
outcome_unknown_at_ms INTEGER
retry_not_before_ms INTEGER
delivered_at_ms INTEGER
suppressed_reason TEXT
error_code TEXT
created_at_ms INTEGER NOT NULL
updated_at_ms INTEGER NOT NULL
UNIQUE (incident_id, episode_number, channel, delivery_kind, delivery_sequence)
```

`policy_snapshot_json` 只保存投递所需的非敏感有效规则字段，用于 policy 已更新、停用或 tombstone 后解释历史 delivery；不得保存对象原始 URL、secret 或自由错误正文。`outcome_unknown_at_ms` 和 `retry_not_before_ms` 保留 crash-boundary retry 轨迹；delivery sequence 的复合唯一约束与 `delivery_key` 双重保证并发 planner 不会产生两个同义逻辑投递。

### 14.6 `alerting_upgrade_progress`

持久化 durable backfill 的唯一进度行，建议字段：

```text
singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1)
phase TEXT NOT NULL CHECK (... not_started, copying_history, rebuilding_current, verifying, complete, failed)
source_high_water_cursor TEXT
last_copied_cursor TEXT
copied_count INTEGER NOT NULL DEFAULT 0
rebuild_version INTEGER
last_error_code TEXT
started_at_ms INTEGER
updated_at_ms INTEGER NOT NULL
completed_at_ms INTEGER
```

该表只由 upgrade step 写入，不是普通 alerting service 的运行时状态。普通应用启动不得看见 `complete` 以外的状态并继续加载新变更中心。

### 14.7 全局设置存储

全局提醒设置可以沿用当前 settings store 的结构化 key/value owner，但 application model 和 DTO 必须是显式字段，不能让 React 直接读写自由 key。复杂 policy 必须使用独立表。

### 14.8 索引

至少验证：

```text
change_incidents(lifecycle_state, severity, updated_at_ms DESC, id DESC)
change_incidents(station_id, lifecycle_state, updated_at_ms DESC)
change_incidents(station_key_id, lifecycle_state, updated_at_ms DESC)
change_event_occurrences(incident_id, episode_number, observed_at_ms DESC, id DESC)
change_event_occurrences(event_type, observed_at_ms DESC, id DESC)
alert_policies(enabled, scope_kind, priority, id)
notification_deliveries(status, scheduled_at_ms, id)
notification_deliveries(incident_id, episode_number, created_at_ms DESC, id DESC)
notification_deliveries(delivery_key)
```

实际索引必须通过生成数据和 `EXPLAIN QUERY PLAN` 验证。

## 15. 写入与评估架构

### 15.1 建议模块职责

```text
application/alerting/event_registry.rs
application/alerting/condition_key.rs
application/alerting/incident_projector.rs
application/alerting/policy_resolver.rs
application/alerting/delivery_planner.rs
application/alerting/attention_service.rs
persistence/stores/alerting/*
application/queries/change_center_workspace.rs
```

- producer 只提交结构化 observation/change；
- event registry 验证类型与恢复合同；
- incident projector 计算状态转换；
- policy resolver 解析唯一 effective policy；
- delivery planner 决定投递或抑制；
- notification adapter 只负责平台投递；
- query service 生成 UI workspace。

### 15.1.1 旧变更中心的解耦边界

升级不是在现有 `ChangeService` / `ChangeStore` 上继续叠加 incident、策略和通知字段。新实现必须按以下职责拆分，防止写入、状态机、查询与 UI 重新耦合：

| 层 | 唯一职责 | 明确禁止 |
|---|---|---|
| producer ingress | 将 Collector、健康、余额、价格、绑定和路由的权威事实转换为结构化 observation/change | 直接写 incident、直接调用 legacy `upsert_change_event`、由 UI 产生恢复 observation |
| event registry + condition key | 注册事件、校验输入、定义 condition identity/恢复 owner/脱敏契约 | 查询数据库、调用通知、按页面需求临时修改事件语义 |
| incident projector | 以事务方式写 occurrence、变更 incident/attention 和安排 delivery | 调用 Tauri/OS、解析前端筛选参数、读取 React 状态 |
| policy resolver + delivery planner | 解析 effective policy、计算 deadline、投递或抑制 | 修改权威健康/余额/采集事实，或由通知结果改变 incident 生命周期 |
| alerting stores | 分别持有 occurrence、incident、attention、policy、delivery 的持久化读写 | 暴露“通用 change event status”写接口 |
| Change Center query service | cursor 查询、聚合摘要、脱敏 DTO | 在前端补算全量风险计数，或为兼容旧页面返回无限/固定 200 条全量数据 |
| notification adapter | claim 后调用应用内/OS 渠道并回写 delivery 结果 | 直接访问业务表或绕过 delivery ledger |

建议目录保持在第 15.1 节列出的 `application/alerting/*`、`persistence/stores/alerting/*` 和 `application/queries/change_center_workspace.rs`。旧 `application/changes.rs`、`persistence/stores/change_store.rs` 以及对应 command facade 只可在迁移观察期作为 legacy history read/backfill 的隔离适配层存在；它们不得被新 producer、总览、侧栏、设置页或新 Change Center import。

前端必须同样拆开：路由容器只负责导航和 query 生命周期；“当前问题 / 变化历史 / 提醒记录”各自使用独立 cursor query；筛选、摘要与计数来自后端 DTO；`incident` 操作和 policy mutation 使用显式 mutation hooks。`AppShell` 不得在 route effect 中批量写已读，也不得订阅旧 `changeEvents` 全量数组来计算徽标。页面组件不能兼任持久化状态机或完整历史的内存索引。

### 15.2 原子性

对会改变当前事实的 producer，单次 observation 与它所依赖的权威事实写入必须在同一写事务内完成：

1. 写 occurrence；
2. 读取/创建 incident；
3. 应用状态转换；
4. 解析 effective policy；
5. 更新 attention episode 边界；
6. 创建 scheduled/suppressed delivery；
7. 提交事务。

Policy 解析和 delivery planning 是纯持久化逻辑；已保存的无效 policy 不允许存在。若它们失败，所属权威事实和 observation 一并回滚，避免事实与 incident 漂移。操作系统投递在事务提交后执行。成功或失败以 claim token 保护的幂等命令更新 delivery，不得持有数据库事务等待系统 API。

### 15.3 幂等与并发

Observation 输入必须带稳定 `observation_id` 或来源唯一键。重复 apply：

- 不新增 occurrence；
- 不重复增加计数；
- 不重复创建 delivery；
- 返回已有状态转换结果。

`observation_id` 必须映射为 `change_event_occurrences.source_observation_key` 的数据库唯一约束；先命中唯一约束时直接返回已有结果，不得再次执行 projector 或 delivery planner。同 condition key 并发写入必须串行化到 persistence write runtime 或使用版本检查，不能丢失计数。

### 15.4 后台调度

后台 worker 负责：

- 到期重复提醒；
- snooze/global pause 到期后的单次重评估；
- 安静时段结束重评估；
- `next_state_evaluation_at_ms` 到期后的 incident trigger/recovery 评估；
- retention 清理；
- 启动时恢复 scheduled delivery。

要求：

- nearest-due 或有界轮询；
- 单实例；
- 有 jitter；
- shutdown 可取消；
- 重启不产生通知风暴；
- stale episode delivery 标记 suppressed；
- 到期状态转换先验证 `fact_fresh_until_ms`，不能把陈旧 observation 当作持续异常或持续正常；
- 不持有 secret。

## 16. Read Model 与 IPC

### 16.1 当前问题 Workspace

建议命令：

```text
load_change_center_incidents
```

输入至少包含：

```ts
type ChangeIncidentQuery = {
  lifecycle?: "current" | "pending" | "open" | "recovering" | "resolved";
  severities?: Array<"critical" | "warning" | "info">;
  eventTypes?: string[];
  objectType?: string | null;
  stationId?: string | null;
  stationKeyId?: string | null;
  attention?: "all" | "unseen" | "snoozed";
  query?: string;
  cursor?: string | null;
  limit: number;
};
```

输出：

- 后端聚合摘要；
- 当前页 items；
- next cursor；
- generatedAtMs；
- query fingerprint；
- data freshness。

摘要必须来自与列表同一 ReadSession，不由前端对当前页计数。

Cursor 是不透明后端值，基于稳定排序 `(updated_at_ms DESC, id DESC)` 生成，并绑定规范化 query fingerprint。客户端将 cursor 用于同一查询的下一页；后端必须拒绝 filter/sort fingerprint 不匹配或超出 limit 范围的 cursor，不能静默返回错误页面。

### 16.2 变化历史与投递历史

建议命令：

```text
list_change_occurrences
list_notification_deliveries
```

两者都使用 cursor pagination、后端筛选和有界 limit。禁止恢复固定 200 条全量接口作为主页面数据源。

### 16.3 Mutation 命令

建议：

```text
mark_incident_seen
snooze_incident
clear_incident_snooze
manually_resolve_incident (受注册表约束)
list_alert_policies
create_alert_policy
update_alert_policy
delete_alert_policy
update_alerting_settings
request_desktop_notification_permission
send_test_notification
```

所有 mutation 返回更新后的 canonical DTO 或版本号，并更新 TanStack Query canonical cache。不得恢复 DOM 自定义事件同步。

### 16.4 DTO 安全

- 不返回完整 API Key、Cookie、token 或 SecretRef 内容；
- 不返回原始认证错误正文；
- 对象显示名来自后端安全 join 或现有非敏感实体查询；
- query 不搜索 old/new JSON 原文和错误正文；
- desktop notification DTO 使用独立最小字段，不复用完整 incident detail DTO。

## 17. 迁移与兼容策略

### 17.1 Schema authoring

实现必须遵守 `docs/SCHEMA_UPGRADE_AUTHORING.md`：

- 在当前最新 schema 后新增一个 append-only migration；
- 不修改历史 `0006_collectors_changes.sql`；
- 更新 compatibility metadata；
- 增加 postcondition；
- 覆盖当前 schema 到新 schema；
- 保持冻结 schema 15 到 latest 路径通过；
- 不在 startup coordinator 增加 schema-specific 分支。

### 17.2 旧数据处理

旧 `change_events` 不直接推断当前 incident，因为旧事件缺少完整恢复事实和可靠生命周期。本升级不是普通的“仅建表” migration：历史复制和当前事实重建必须作为显式、可恢复的 durable upgrade step 实施。

执行顺序：

1. append-only SQL migration 只创建 alerting 表、索引、`alerting_upgrade_progress` 和 postcondition，不在 SQL migration 中无界复制历史；
2. 启动 upgrade planner 显式计划 `AlertingHistoryBackfill`，该步骤拥有 typed failure/recovery、备份前置条件和完成 postcondition；这属于本规范批准的 schema-specific durable transition，不得在普通 service startup 中隐式执行；
3. 该步骤在 progress row 中持久化 `phase`、source high-water cursor、last copied cursor、copied count、rebuild version 和完成时间；
4. 复制按有界 batch 完成，可安全映射的旧记录写为 legacy occurrence，标注 `source = legacy_change_event`；重复运行不得复制两次；
5. 不把旧 `read/dismissed/resolved` 映射为当前 incident attention/lifecycle；
6. copy 完成后，在 alerting writer 尚未对外接收 producer observation 前，从当前权威事实重建 incident projection；
7. postcondition 验证 source high-water 覆盖、去重、当前事实重建、schema compatibility 和 progress `complete`；失败进入 typed recovery，不创建虚假健康状态；
8. 正常业务页面和新 producer 写路径只在 durable transition complete 后启用；不以“旧版本兼容路径”掩盖半完成状态；
9. 保留旧表只读一个发布观察周期，之后通过独立计划删除旧表和旧命令。

该步骤必须同时覆盖当前 schema -> latest 与 schema 15 -> latest，且不得绕过 `SCHEMA_UPGRADE_AUTHORING.md` 的 planner/executor 边界。

### 17.3 当前事实重建

重建只读取：

- 当前 balance projection；
- 当前 group binding；
- 当前 pricing freshness；
- 当前 Station Key health；
- 当前 collector task state；
- 当前 route impact projection。

历史 change event 不能覆盖当前事实。

### 17.4 回滚边界

- schema migration 前使用既有备份与升级恢复机制；
- 不长期双写两套 incident 内核；
- feature activation 可以暂时隐藏新 UI，但不能让新旧状态机同时消费同一 observation；
- 回滚依靠已验证备份和应用版本，不靠运行时 repair SQL。

## 18. Retention 与清理

建议默认：

- unresolved/current incident：不按时间删除；
- resolved incident：保留 365 天；
- occurrences：90 天；
- notification deliveries：30 天；
- policy：用户删除前永久保留；
- attention：随 incident episode 保留。

Occurrence retention 可以删除已过期的最后 occurrence，因为 incident 保留了脱敏 `last_observation_summary_json`。若保留 `last_observation_id`，它只能是可空的逻辑关联，不得使用会阻止 retention 的强制外键。Delivery 保存 policy snapshot，因此 disabled/tombstone policy 可在关联历史仍存在时继续被解释。

清理要求：

- 只清理已达到 retention 的历史；
- 不删除 current incident 的最后 observation；
- 分批删除并有单轮上限；
- 删除顺序遵守外键；
- cleanup 失败不影响 incident projector 和通知；
- 记录脱敏计数和耗时；
- UI 不再提供危险的“一键清除所有当前问题”。

## 19. 安全与隐私

- Alert Policy 不保存 secret、URL query、Header 或请求正文。
- condition key 使用内部 ID/hash，不能包含 api_base_url 或凭据。
- occurrence JSON 继续遵守现有 redaction policy。
- 通知标题和正文使用本地化模板，不直接拼接 raw error。
- policy name 和本地备注有长度限制，不进入系统日志。
- notification failure 只保存稳定 error code。
- 测试通知使用固定假内容，不引用真实 Station/Key。
- 导入导出若后续覆盖提醒规则，只能导出规则本身，不导出 incident、delivery、设备权限或本地对象名称快照。

## 20. 可观测性

本地诊断指标：

- incident counts by lifecycle/severity/event type；
- occurrence apply/dedup counts；
- state transition counts；
- scheduled/delivered/suppressed/failed delivery counts；
- delivery lag；
- policy resolution duration；
- workspace query duration；
- retention duration/deleted rows。

禁止使用 Station/Key 原始 ID、名称、URL、错误正文作为 metric label。结构化诊断可使用稳定 correlation ID 和本地 entity hash。

## 21. 错误处理与降级

- Incident 投影失败时，生产事实写入必须按所属事务合同决定整体回滚，不允许只写 occurrence 后丢失状态转换。
- Desktop notification 失败只影响 delivery，应用内 current incident 保持可见。
- 权限拒绝时明确展示并退化为应用内提醒。
- Policy 无效时拒绝保存，不在运行时猜测默认值。
- Policy 引用已删除 Station/Key 时将规则标记为 disabled/orphaned，并提示用户处理；不得静默扩大为全局规则。
- 设置查询失败时保留上次完整快照并显示 freshness/error。
- Worker 启动失败不得阻止用户查看当前问题，但必须显示提醒调度降级状态。
- 系统时间大幅回拨或前跳时重新计算到期时间，不能重复投递同一 delivery id。

## 22. 测试策略

### 22.1 Event Registry 合同

每个状态型事件必须测试：

- condition key 稳定性；
- 默认严重度；
- 异常判据；
- 恢复判据；
- 允许的 policy override；
- 脱敏字段；
- 相关对象深链。

缺少恢复测试的状态型事件不得进入注册表。

### 22.2 状态机单元测试

使用固定时钟覆盖：

1. 立即触发；
2. 连续 N 次触发；
3. 持续 T 时间触发；
4. pending 中恢复；
5. open 后进入 recovering；
6. 连续 M 次恢复；
7. 恢复持续 T 时间；
8. recovering 中再次异常；
9. resolved 后复发并增加 episode；
10. 严重度升级/降级；
11. endpoint revision 隔离；
12. missing/stale observation 不被当作 healthy；
13. 重复 observation 幂等；
14. 并发 observation 不丢计数。
15. trigger/recovery deadline 到期且事实新鲜时转换；
16. deadline 到期但事实陈旧时保持原状态；
17. 新 observation 取消相反方向的 deadline。

### 22.3 事件类型集成测试

至少覆盖：

- group missing -> available 自动恢复；
- key group unresolved -> bound 自动恢复；
- collector failed -> 成功恢复，partial 按任务合同处理；
- balance low/depleted -> hysteresis 恢复；
- price expired -> 新鲜完整价格恢复；
- key invalid -> 共享 health recovery；
- station down -> 当前 endpoint revision 恢复；
- route impacted -> 当前投影恢复；
- rate/price change 只进入历史，不污染 current count。

### 22.4 Policy Resolver 测试

- 全局、事件、Station、Key 优先级；
- 同层 event type、minimum severity、priority 和 ID tie-break；
- disabled/orphaned policy；
- severity minimum/offset；
- 默认规则 fallback；
- 无匹配/所有用户规则停用时仍有确定的 `system_default`；
- 保存校验边界；
- policy 更新只影响后续评估，不篡改历史 delivery；
- trigger/recovery 修改为 pending/recovering 建立新 evaluation epoch，不使用旧 observation 立即开告警或恢复。

### 22.5 Delivery Planner 测试

- 首次 open；
- repeat never/interval/escalation；
- cooldown；
- snooze；
- global pause；
- quiet hours 和跨午夜；
- critical bypass；
- recovery notification；
- permission denied；
- stale episode；
- claim lease、claim token 和 expired claim reclaim；
- OS 调用后崩溃的 `outcome_unknown` 与 best-effort 重试；
- retry 只复用同一 logical delivery、次数有上限、耗尽后终态 failed；
- 重启恢复；
- 系统时间变化；
- 同 delivery id 幂等。

### 22.6 Persistence 与迁移测试

- 新 migration postcondition；
- 当前 schema -> latest；
- schema 15 -> latest；
- durable legacy history backfill 的 progress、high-water、断点恢复、去重和脱敏；
- `source_observation_key` 的数据库幂等与 delivery sequence 的复合唯一约束；
- current facts rebuild；
- transaction rollback；
- cursor pagination；
- 100k occurrences / 10k incidents 查询计划；
- retention 不删除 current incident 证据；
- portable migration / artifact policy 更新。

### 22.7 前端测试

- 当前问题、历史、提醒记录视图；
- 后端摘要不由当前页重算；
- seen 与 snooze 区分；
- snooze 到期显示；
- 设置优先级和生效来源；
- 规则编辑校验和自然语言预览；
- desktop permission denied/unsupported；
- loading/empty/error/disabled；
- 窄窗口、长文本、键盘和焦点；
- deep link 到 Station/Key/Channel/Pricing/Routing；
- 不再进入页面即标记全部问题为已读。

### 22.8 端到端测试

- producer observation -> occurrence -> incident -> delivery -> UI；
- 问题恢复后总览和变更中心计数下降；
- 应用退出/重启后 cooldown、snooze 和重复提醒保持；
- policy 修改后新 observation 使用新 policy；
- policy 修改后 active incident、deadline 和未 claim delivery 被 reconcile；
- 桌面权限拒绝时应用内提醒仍工作；
- 超过 200 条时聚合、筛选和分页准确；
- 清理 worker 与写入并发不破坏当前问题。

## 23. 性能与容量目标

本地基准目标：

- 10,000 current/resolved incidents、100,000 occurrences、100,000 deliveries；
- 当前问题首屏 100 条以内，workspace 查询目标小于 150 ms；
- occurrence 历史 100 条 cursor page 查询目标小于 200 ms；
- 总览聚合目标小于 50 ms；
- 单次 observation 投影和 delivery planning p95 小于 20 ms，不含 OS 投递；
- 前端不一次加载全部 history；
- 单个 workspace DTO 有明确上限；
- policy resolution 与规则数量线性或更优，首期支持至少 1,000 条 policy。

性能目标必须使用生成数据、固定机器指纹和 `EXPLAIN QUERY PLAN` 验证。

## 24. 实施阶段

### Phase 0：契约冻结与事件清单

- 建立当前行为基线和缺陷回归测试；
- 冻结 event registry 首期类型；
- 为每类状态问题确定权威事实与恢复条件；
- 建立 legacy ownership map：逐个登记旧 command、DTO、store、producer 调用点、query key、UI view model、测试 fixture、生成绑定和 schema/import catalog 的替代物与删除阶段；
- 明确旧命令、旧 UI 和旧表删除清单，并建立仅允许迁移 adapter 使用 legacy 模块的临时 allowlist；
- 设计 schema migration 与 postconditions。

退出条件：所有状态型事件都有异常/恢复 fixture，不存在“以后再补恢复”的注册项；legacy ownership map 没有未指定 owner 或删除阶段的入口。

### Phase 1：领域模型与持久化

- 新增 occurrence、incident、attention、policy、delivery 模型；
- 新增 migration、索引和 store；
- 实现 condition key、event registry 和纯状态机；
- 实现 legacy history backfill 和 current facts rebuild。

退出条件：固定时钟状态机、migration、schema15 upgrade 和 current facts rebuild 测试通过。

### Phase 2：生产者切换与恢复闭环

- Collector、group binding、pricing、balance、health、routing producer 接入统一 observation API；
- 按事件类型实现恢复；
- 禁止生产路径直接 upsert legacy `change_events`；
- 建立幂等与并发合同。

退出条件：所有首期状态问题都能自动触发、恢复和复发；一次性变化不进入 current count；除 durable backfill/只读 adapter allowlist 外，生产代码不存在对 legacy write service/store 的调用。

### Phase 3：Read Model 与变更中心 UI

- 新增 incident workspace、occurrence 和 delivery cursor APIs；
- 总览与侧栏切换到 incident aggregates；
- 变更中心改为当前问题、变化历史、提醒记录；
- 增加详情、已读、暂停和对象深链；
- 移除进入页面全量已读和客户端 200 条全量假设。

退出条件：超过 200 条仍能准确汇总和分页，恢复后所有消费面计数同步下降；总览、侧栏和页面不再 import 旧 `changeEvents` query、旧 unread view model 或批量 mark-read mutation。

### Phase 4：提醒设置入口与应用内策略

- 设置页增加“提醒与告警”入口；
- 实现独立设置工作区；
- 实现全局、事件类型、Station、Key 规则；
- 实现触发、恢复、重复、冷却、暂停和安静时段；
- 实现应用内 delivery 和投递解释。

退出条件：用户可以完成本规范 3.3 的全部配置，并能从 delivery 记录解释提醒或抑制原因。

### Phase 5：桌面系统通知

- 引入并审计 Tauri notification 能力；
- 权限请求、拒绝降级和测试通知；
- 通知点击 deep link；
- 平台投递 adapter 与 delivery 状态更新；
- Windows 实机 smoke。

退出条件：权限允许、拒绝和不可用三种路径都可验证，通知内容通过敏感信息扫描。

### Phase 6：Retention、清理与旧实现删除

- 启用 history/delivery retention worker；
- 一个发布观察周期后删除旧写路径、兼容命令、旧 DTO、旧 query key、旧页面 view model 和相关 generated binding；
- 仅在旧表已经完成 backfill、已无运行时 read adapter、且 backup/recovery 资格通过后，使用独立 schema migration 删除 legacy table、索引和对应 import/export catalog 项；
- 更新 PROJECT_PLAN、PRODUCT_MODEL、README、release 和审计清单；
- 删除只验证旧源码字符串形状的过时测试，替换为行为合同和结构性边界检查。

退出条件：仓库只有一套 incident 生命周期和提醒策略路径，legacy allowlist 清零，旧 command/DTO/IPC binding/测试 fixture/portable migration catalog 全部不存在。

## 25. 解耦、兼容与旧实现删除合同

### 25.1 原则与时序

新旧实现不能长期双写、双读或相互 fallback。切换以 producer write path 为原子边界：先完成 durable backfill 和 current-fact rebuild，再启用新 writer；旧 writer 一旦停止，任何调用都是错误而不是回退机会。观察期内只允许一个隔离的 legacy read adapter，用于已完成复制后的历史核对；它不参与总览、侧栏、当前问题、策略、delivery 或恢复判定。

Legacy adapter 必须具备明确过期条件、调用计数和单独开关，且默认关闭。它不能暴露为正常 IPC 命令，不能对旧表写入，不能吞掉新 query/service 错误后回退旧表。新 UI 仅显示新读模型，历史核对仅在诊断/迁移恢复流程中使用。

### 25.2 后端删除矩阵

| 旧责任或入口 | 替代 owner | 观察期规则 | 最终动作 |
|---|---|---|---|
| `change_events.status` 兼任 unread/read/dismissed/resolved 与事实生命周期 | `change_incidents.lifecycle_state` + `incident_attention` | 旧 status 只作为历史原样字段，禁止解释为新状态 | 删除 status 生产语义及旧表 |
| `application/changes.rs` 的通用 upsert/clear/set-status | alerting ingress、projector、attention service、retention worker | 仅 legacy adapter 可读历史 | 删除 service、facade 和单元测试 |
| `persistence/stores/change_store.rs` 的 upsert/resolve/clear | 分离的 alerting stores 与 migration backfill store | 禁止新代码 import；只读 adapter 不含 write method | 删除 store 和 `resolve_by_dedupe_key` |
| `upsert_change_event` / `resolve_change_event` / `mark_change_event_read` / `mark_change_events_read` / `dismiss_change_event` / `clear_change_events` IPC | incident、policy、delivery 和 retention IPC | 不保留隐藏兼容 alias | 删除 command、registry 条目、DTO、客户端 binding、序列化 fixture |
| `list_change_events` / `list_change_events_for_station` 固定窗口 API | incident workspace、occurrence、delivery cursor APIs | legacy adapter 仅供升级核对，不面向产品 UI | 删除 command、query option、query key 与 mock |
| legacy schema 表、索引、portable/import catalog 记录 | occurrence/incident/policy/delivery schema | 仅在 backfill 完成的发布观察期保留只读 | 独立 append-only migration 删除，并更新 postcondition、schema15 fixture、catalog 和备份恢复契约 |

删除 migration 前，必须证明 `alerting_upgrade_progress=complete`、source high-water 已覆盖、legacy adapter 调用计数为零、当前数据备份成功且 schema15 -> latest recovery tests 通过。任何一个条件不满足，都延后删除，不得用运行时 repair 或永久 compatibility flag 规避。

### 25.3 前端删除矩阵

| 旧耦合 | 替代 owner | 最终动作 |
|---|---|---|
| `AppShell` 进入 changes route 即 mark unread | 页面级 `last_seen_cursor` 与详情级 `mark_incident_seen` | 删除 route effect、optimistic batch mutation 和旧 merge helper |
| 侧栏的 `unreadChangeCount(changeEvents)` | 后端 incident aggregate | 删除对旧全量 query 的订阅和 99+ 计算来源 |
| `ChangeCenterPage` 对 200 条数组做 filter/paginate/severity aggregate | cursor query 的 server-filtered workspace summary | 删除 `changeEventViewModels` 的全量筛选、客户端分页和 active count helpers |
| “清除记录”主操作 | retention 说明、按对象/incident 的非破坏性操作 | 删除全局清除按钮、确认框、API 和测试；不以用户删除事实弥补数据增长 |
| 旧 `ChangeEvent` DTO、query key、TanStack cache 更新和 DOM/event 同步 | incident/occurrence/delivery DTO 与 canonical mutation response | 删除旧类型、hook、mock、fixture 和缓存 merge 分支 |

UI 切换不允许展示新列表却继续用旧数组驱动徽标、空状态、筛选项或 deep link。所有视图都必须在同一 release 切到相应的新 DTO，任何缺失/加载/错误状态由该 DTO 的 query owner 处理。

### 25.4 删除验证与防回归

Phase 0 建立结构性 allowlist；Phase 2、3 和 6 分别收缩，Phase 6 必须为空。检查至少覆盖：

- 生产 module graph 中，除 durable backfill、只读 legacy adapter 和删除 migration 外，不存在对 `ChangeService`、`ChangeStore`、`change_events` 写 API 或旧 IPC 名称的依赖；
- IPC registry、TypeScript binding 生成输入、DTO fixture、query key、mock 和路由测试不再包含已删除命令；
- 新 producer 的 contract test 证明一个 observation 只能通过 alerting ingress 进入状态机；
- 新 UI contract test 证明侧栏/总览/变更中心只消费 incident aggregate 和 cursor API，不会进入页面即写状态；
- 从当前 schema 和 schema 15 升级、完整 backfill、重启、retention 后，行为合同仍通过；
- 删除后运行 `pnpm verify:fast`、相关 Vitest、`pnpm build`、Rust fmt/check/test，以及本规范第 22 节的 migration、IPC 和架构测试。

禁止只用 `rg` 结果作为删除完成证据；它只能辅助发现遗留引用。最终证据必须包括编译、生成 binding/fixture 校验、架构边界测试、升级 fixture 和端到端行为合同。

## 26. 验收标准

### 26.1 生命周期

- 每个状态型事件都有确定的异常、恢复和复发路径。
- 恢复后 current warning/critical 数量自动下降。
- seen、snooze、mute 不改变 incident lifecycle。
- resolved 后复发创建新 episode 并重新评估提醒。
- 缺失数据、失败查询和陈旧事实不会误判恢复。
- 持续 T 时间的 trigger/recovery 只在新鲜事实下于持久化 deadline 到期后发生。

### 26.2 配置

- 用户可以按事件类型、严重度、Station 和 Key 配置规则。
- 支持立即、连续 N 次、持续 T 时间触发。
- 支持连续 M 次、正常持续 T 时间恢复。
- 支持应用内、桌面通知、重复提醒、冷却、暂停、安静时段和恢复提醒。
- 设置页能解释生效规则来源和优先级；无匹配规则时明确显示 `system_default`。
- 关闭提醒不影响当前问题的生成、恢复和计数；重新开启不回放通知。

### 26.3 通知

- 首次、重复、升级和恢复提醒互不混淆。
- 正常运行中相同 delivery key 不重复发送；OS 调用后的崩溃边界遵循已记录的 best-effort at-least-once 语义。
- 不确定投递仅在有限退避内复用同一 logical delivery 重试，耗尽后可审计地失败。
- 权限拒绝自动降级，不丢失当前问题。
- 重启不产生重复通知或通知风暴。
- 通知和投递记录不泄露敏感信息。

### 26.4 查询与 UI

- 当前问题摘要来自后端完整 incident projection。
- 超过 200 条时筛选、分页和聚合准确。
- 变更中心清晰区分当前问题、变化历史和提醒记录。
- 总览、侧栏和变更中心消费同一聚合事实。
- loading、empty、error、disabled 和窄窗口状态可用。

### 26.5 工程质量

- schema migration、postcondition、schema15 upgrade 测试通过；
- durable alerting backfill 的中断恢复、high-water 覆盖和 typed failure 测试通过；
- Rust domain/store/query/integration tests 通过；
- 相关 Vitest 和 UI contract tests 通过；
- legacy module graph allowlist 清零；旧 IPC/DTO/binding/fixture/查询缓存分支已删除，且由架构边界测试证明新生产者和 UI 不会回退旧栈；
- `pnpm build` 通过；
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` 通过；
- `cargo check --locked --manifest-path src-tauri/Cargo.toml` 通过；
- `pnpm verify:fast` 通过；
- 较大范围最终切换前 `pnpm verify:full` 通过；
- artifact、ACL、binding generation 和敏感信息扫描通过。

## 27. 风险与控制

| 风险 | 控制 |
|---|---|
| 新旧状态机长期共存 | 原子 producer cutover、删除清单、观察期只读兼容 |
| 规则过于复杂 | 首期单一 trigger/recovery mode，不实现 DSL |
| 假恢复 | 权威事实 owner、连续恢复/稳定时间、missing 不算 healthy |
| 通知风暴 | dedupe、cooldown、repeat policy、quiet hours、启动重评估上限 |
| 用户永久错过复发 | attention 按 episode 隔离，mute 必须属于显式 policy |
| 桌面权限不一致 | opt-in、权限状态、应用内降级、平台 adapter |
| 数据增长 | cursor、聚合读模型、分层 retention、批量清理 |
| 规则引用对象删除 | orphaned/disabled，不扩大匹配范围 |
| 敏感信息进入通知 | 独立最小 DTO、模板、redaction、扫描测试 |
| 时间与 DST 错误 | UTC epoch 调度 + IANA/系统时区解释安静时段 |

## 28. 待评审决策

以下事项必须在进入实施计划前确认：

1. 变更中心顶层是否采用“当前问题 / 变化历史 / 提醒记录”三个视图。
2. 默认 warning 触发是否为连续 2 次，还是由事件类型分别定义。
3. 是否允许用户对自动恢复型 incident 执行“强制关闭”；本规范建议不允许。
4. resolved incident 默认保留 365 天是否合适。
5. Key 级规则是否进入首期；本规范纳入数据模型和设置能力，但可在交付拆分中后置。
6. critical 是否默认绕过安静时段；本规范建议默认不绕过，由用户显式开启。
7. desktop notification 是否只支持主窗口 deep link，还是同时支持 tray action；首期建议只支持点击打开详情。

上述决策不得通过实现中的隐式默认代替设计评审。

## 29. 交付物

本规范获批后，实施计划至少应交付：

- event registry 与恢复矩阵；
- schema migration、postconditions 和 migration manifest 更新；
- incident/policy/delivery domain 与 stores；
- producer cutover 和旧写路径删除清单；
- change center workspace 与 cursor APIs；
- 设置 -> 提醒与告警工作区；
- 当前问题、变化历史、提醒记录 UI；
- 应用内和桌面通知 adapters；
- retention worker；
- 生命周期、策略、通知、迁移、性能和安全测试；
- PROJECT_PLAN、PRODUCT_MODEL、README、release 与审计文档更新。
