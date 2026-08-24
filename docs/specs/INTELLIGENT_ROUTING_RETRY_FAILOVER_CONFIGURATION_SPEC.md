# Relay Pool Desktop 智能路由重试、故障转移与熔断控制面升级规范

状态：首版已实施；Phase 2-4 的能力已按本文范围接入并通过聚焦验证，保护 bridge 默认关闭。真实 outbound Half-Open probe 当前为 Credential-scoped；reducer/read-model 虽可表达更多 scope，但 account/group/endpoint/model 尚未全部接入真实 proxy probe 编排。synthetic probe、完整历史和 raw IPC 重复键检测仍按边界逐项准入

日期：2026-08-20

适用范围：路由设置页、请求级重试、跨故障域故障转移、健康冷却、熔断恢复、路由决策解释、策略文档与 Tauri IPC

提案类型：智能路由控制面与可解释性升级

替代关系：本文补充并约束 [`INTELLIGENT_ROUTING_ENGINE_SPEC.md`](INTELLIGENT_ROUTING_ENGINE_SPEC.md) 中关于请求失败后的执行行为和用户控制面；不替代该文档对资格、分层、评分、容量和候选快照的定义。首版已落地的事实以当前代码、生成契约和实施计划执行记录为准；本文明确标注自动 synthetic probe、完整历史和 raw IPC 重复键检测等尚未开放边界。

关联文档：

- [`../README.md`](../README.md)
- [`INTELLIGENT_ROUTING_ENGINE_SPEC.md`](INTELLIGENT_ROUTING_ENGINE_SPEC.md)
- [`ROUTING_POLICY_CONFIGURATION_SYSTEM_SPEC.md`](ROUTING_POLICY_CONFIGURATION_SYSTEM_SPEC.md)
- [`../plans/2026-08-13-upstream-error-classification-retry-closure.md`](../plans/2026-08-13-upstream-error-classification-retry-closure.md)
- [`../SECURITY_EXPORT_IMPORT.md`](../SECURITY_EXPORT_IMPORT.md)

## 1. 规范约定

本文使用以下约束级别：

- `MUST`：实现、交互和验证必须满足。
- `MUST NOT`：明确禁止。
- `SHOULD`：默认应满足；偏离时必须记录理由和替代保障。
- `MAY`：可选扩展，不构成首版交付条件。

本文中的“重试次数”如果未特别说明，均指额外请求次数；面向用户的字段统一使用“最大总尝试次数”，其数值包含第一次请求。

## 2. 执行摘要

当前代理已经具备标准化上游错误分类、请求级容量重试、候选故障域排除、健康冷却、容量域 Half-Open、Credential-scoped 的真实 outbound probe、lease-race strict refresh 和结构化解释基础；这些能力的安全边界和测试基础总体可靠。首版已把尝试预算、typed action、控制面和读模型 owner 收敛；错误率 reducer/history 默认关闭，启用后可通过 V2 `protectionProfile` 和同源 Credential scope commitment bridge 参与匹配作用域的候选 admission。Provider/failure-domain 基础聚合诊断已可在状态面板查看，status 查询支持可选 requested model；没有提供模型时明确返回 `model_required`，不猜具体 commitment。用户现在可以看到本次运行的实际动作和 trace 可用性；重启后仍只能看到有界的 durable attempt/lifecycle 摘要，不能恢复完整 runtime 细节。

本升级建立一个清晰的两层控制面：

1. **请求级可靠性策略**：决定当前请求是否允许重放、最多尝试几次、等待多久以及是否跨故障域切换。
2. **跨请求故障保护策略**：根据一段时间内的失败证据暂时抑制上游，并通过冷却和半开探测决定何时恢复。

两层策略都必须服从错误分类和 replay gate。用户设置只能提供预算和敏感度，不能强制重试不可安全重放的请求，也不能绕过鉴权、能力、余额、容量、deadline 或安全边界。

## 3. 目标与非目标

### 3.1 产品目标

- 让用户能用少量、语义稳定的字段控制瞬态错误的重试预算和故障转移范围。
- 清楚区分“同一次请求再试”和“后续请求暂时不再选该上游”。
- 让每次重试、等待、切换、冷却和最终停止都有可读原因。
- 保留现有智能路由的硬资格、候选分层、容量准入和故障域隔离，不退化成“失败后按列表轮询”。
- 让策略配置、决策 trace、日志和测试使用同一套枚举、revision 和时间预算。
- 为未来的错误率熔断、按 Provider 覆盖和自适应保护留下版本化扩展点。

### 3.2 工程目标

- 请求执行器、错误分类器、健康 reducer、熔断状态机和 Planner 之间各有唯一 owner。
- 所有预算都以单调时钟和请求总 deadline 计算，不因 replan 重置。
- 运行时只消费带策略 revision 的不可变配置快照；进行中的请求不被配置热更新中途改写。
- UI、受管 JSON、恢复和未来 CLI 均调用同一策略 service、validation、compiler 和 CAS 流程。
- trace 是有界、可序列化、无敏感信息的证据，不依赖原始请求正文或认证数据。

### 3.3 非目标

- 不复制 CCSwitch 的 UI 或内部实现，也不把其“最大重试次数”直接照搬为本项目语义。
- 不提供任意脚本、正则或用户自定义错误判定器。
- 不在首版引入云端共享状态、跨设备熔断同步或黑盒机器学习。
- 不允许用户通过开关重试认证失效、能力不支持、请求错误、已提交请求或无法证明安全的请求。
- 不把所有内部阈值、窗口、权重和计数器放进普通设置页。

## 4. 当前基线、工程评估与前置收敛

### 4.1 当前已存在的生产基础

按当前代码和自动化契约，生产路径已有：

- `CanonicalFailure` 同时携带错误类别、目标作用域、重试意图、健康 effect、请求接纳结论和 replay safety；无 canonical producer 的失败会 fail-closed。
- 容量错误的同目标重试具有确定性 jitter、`Retry-After`、总等待预算、FIFO waiter、active limit、取消释放和 target commitment revalidation。
- `ProxyServerLimits` 定义连接、首字节、precommit、buffered 和 stream idle 的服务器级超时；生产默认值分别为 10 秒、30 秒、60 秒、300 秒和 90 秒。
- scoped durable health verdict 可按 credential、account、group、endpoint 等作用域写入 `Degraded`、`Cooldown` 或 `Blocked`；候选快照会消费这些事实。
- capacity domain 的 Closed/Open/Half-Open 保护由 `CapacityRetryRegistry` 管理，且没有跨同域 sibling Key 的容量轮询。
- durable health reducer 的 Half-Open lease 已接入真实 proxy outbound 的 Credential scope：lease 只在 Credential 目标解析成功并即将发送真实请求时消费，未跨出站边界的路径按 revision fence 取消；其他 reducer scope 目前没有同等的 production probe resolver。
- `ProtectionStatus` 已附带基础 failure-domain 聚合诊断；candidate count、schedulable count、解析状态、保护状态和 explanation key 均来自后端 read-model，前端不再拼接局部事实。
- decision trace 有硬上限和敏感信息过滤；进程内 ring 可补充 durable terminal outcome。
- target commitment、权威重载、committed 后停止以及非幂等 `Unknown` fail-closed 等安全门已存在。

这是一套可靠的执行基础，但它还不是一个已经统一、用户可参数化的 Circuit Breaker 产品能力。

### 4.2 对用户不可见的黑盒点

- UI 现在显示 canonical action、终态原因、summary-only/unavailable 和保护明细；历史请求可读取有界 durable attempt summary 及按序的 durable lifecycle event summary。若运行时详细事件已被驱逐或应用已重启，仍不能恢复完整 runtime 细节，但可看到这些受限的终态、attempt 和生命周期摘要。
- `RetryAction` 已是请求执行的唯一动作 owner；lifecycle 中保留的 `TryNextCandidate` 只是兼容终态投影，不能反推真实动作。
- “最多 4 次”已由编译后的 `AttemptBudgetProfileV1` 统一供 admission、execution、capacity 和 trace 消费；旧 `max_upstream_attempts` 与二值 `RetryDecision` 已删除。
- 容量 Half-Open 是进程内状态；持久化 health verdict 和旧 station-key health snapshot 是不同读模型。它们不能被统称为一个跨重启、统一状态机的熔断器。
- 运行时 trace 是有界进程内详细诊断；重启后可依赖 durable outcome、有界 attempt summary 和最多 64 条有序 lifecycle event summary，但不能承诺得到完整逐步 runtime 事件时间线。
- 当前编辑器使用 shared query、draft、外部变更合并和 typed CAS conflict resolver；重复键保证已明确限定为 raw/file document decoder，已解析的 IPC `Value` 不承诺原始 JSON 重复键检测。

### 4.3 必须先收敛的 owner 与技术债

| 问题 | 当前事实与风险 | 本升级的处理 |
| --- | --- | --- |
| 尝试预算分裂 | execution、`RouteAdmissionCoordinator`、decision trace 和 capacity profile 各自固定为 `4`；修改任一处都会产生行为或诊断不一致 | 新建一个编译后的 `AttemptBudgetProfileV1`，由 policy compiler 生成，并同时注入 coordinator、execution、capacity path 和 trace hard cap；删除无消费者的 `max_upstream_attempts` |
| 动作语义丢失 | `RetrySameTarget`、`WaitThenReplan`、`TryDifferentFailureDomain` 在 execution 中大多变为 `NextCandidate` | 将二值 `RetryDecision` 替换为带 reason、scope、budget 和 wait 的 typed `RetryAction`；不得新增第二个平行 planner |
| 路由配置投影仍含 legacy 形状 | `RoutingPolicyConfigV1`、`RuntimeRoutingSettings`、旧 `RoutingPolicy` 枚举和 `DispatchAlgorithmSettings::default()` 仍共同参与若干读取链 | retry/failover 只进入 versioned policy aggregate；逐步移除 legacy projection 的生产消费者，不能再把新字段投影到通用 settings |
| 健康读模型并存 | scoped verdict、旧 health snapshot、容量 registry 的生命周期和持久化语义不同 | 定义面向 UI 的单一 `ProtectionStatus` projector；保留旧表作兼容/历史输入，禁止前端自行拼接状态 |
| trace 的保留语义不清 | runtime ring 很安全但会在重启或驱逐后丢失步骤，durable history 只保存受限 attempt 和生命周期摘要 | 首版由 attempt lifecycle 生成 bounded durable attempt summary 与 lifecycle event summary；完整历史事件时间线仍不得通过扩大内存 ring 冒充 |
| 测试模型易被误用 | `RuntimeOutlierPolicyV1`、错误率和其 Half-Open 实现受 `cfg(test)` 保护，不是生产控制面 | 不得为它添加 UI 或把它写入 active policy；后续 error-rate breaker 必须从生产 observation/verdict 链重新设计 |
| 文档 decoder 不够严格 | 当前 document envelope 严格，但嵌套 `RoutingPolicyConfigV1` 不是公开的严格 schema owner | v2 必须引入拒绝未知字段和重复键的 public document DTO，再转换为领域类型；不可依赖 serde 默认的最后键获胜 |

这些是边界收敛，不是全量重写。现有 classifier、replay gate、capacity registry、durable outcome 和 snapshot fence 已保留；重复预算、旧动作胶水和旧写入入口已替换。proxy execution 已统一包裹规划 I/O deadline，并以 typed adapter 收口 `/models` 只读聚合；非 proxy 调用方已改为显式传入 caller-owned absolute deadline context，错误率保护到 Credential scope 的候选 admission typed bridge 已接通但默认关闭，attempt lifecycle 已提供有界 durable summary 和 lifecycle event summary，Credential-scoped probe lease 与 lease-race strict refresh 已接入 outbound 编排，路由工作区已公开可信的故障域身份/commitment 及基础聚合诊断 read-model，status query 支持可选 model 且 query key 按 model 隔离。其他 health scope 的真实 probe 需要独立的 scope resolver 和跨层回归，不能从 reducer 枚举或聚合诊断推断已完成。剩余工作还包括完整历史步骤时间线，以及未来若要改变 raw IPC duplicate-key 兼容边界时另建 raw JSON DTO 迁移；窗口、阈值和 Half-Open 成功次数已属于可配置 `protectionProfile`，不再是未实现的设置字段。

### 4.4 成熟度判断

当前实现作为本地代理的故障处理底座是保守且工程化的：有 fail-closed 重放判断、有界队列和内存、RAII 释放、revision fence、Credential-scoped 真实 outbound probe 边界、lease-race strict refresh 和专项测试。可调控制面已经完成首版收敛；其他 health scope 的 probe 编排、完整历史仍应作为独立扩展，不能通过增加局部字段绕过 owner、CAS 或 replay gate。正确路径仍是沿既有模块边界增量扩展，而不是重写路由引擎。

本升级必须将这些差异转换为稳定术语、显式事件和可追溯 revision，而不是仅增加更多数字输入框。

## 5. 用户心智模型

### 5.1 三种请求级处理意图

错误处理不应只分为“重试”和“不重试”，生产行为至少分为三种意图：

| 意图 | 行为 | 典型原因 |
| --- | --- | --- |
| `StopRequest` | 立即结束，不消耗普通重试预算 | 请求无效、能力不支持、凭据失效、已提交或无法安全重放 |
| `RetrySameTarget` | 在同一故障域内等待后重试同一目标 | 短暂容量竞争、明确可等待的容量响应 |
| `TryDifferentFailureDomain` | 排除当前故障域，使用最新快照重新规划 | 连接失败、首字节超时、5xx、账号限流或余额/订阅问题 |

`WaitThenReplan` 是一种调度动作，不是第四种错误类别：当等待期间策略、健康或容量快照发生变化时，系统可以重新规划而不发送旧目标。

### 5.2 两种“保护”必须分开

- **重试预算**属于单个请求。它在请求结束时销毁，不会影响下一请求。
- **熔断/冷却状态**属于跨请求的健康保护。它记录上游近期失败，暂时影响后续候选资格。

“失败阈值”表示跨请求健康保护何时打开；“最大总尝试次数”表示当前请求最多发多少次。两者不得共用字段或文案。

### 5.3 保护作用域

不同错误必须更新不同作用域，避免一次错误误伤无关候选：

| 错误证据 | 默认作用域 | 不应影响 |
| --- | --- | --- |
| 连接失败、首字节超时、上游 5xx | `station_key + endpoint_revision` | 同站点其他独立 Key（除非有聚合证据） |
| 容量不足 | `capacity_domain` | 同域 sibling Key 不应被轮询绕过 |
| 账号限流、余额或订阅问题 | credential / account 相关故障域 | 其他账号或独立 Provider |
| 模型不支持 | model capability / endpoint scope | 支持该模型的其他候选 |
| 凭据失效 | credential scope | 其他凭据 |

## 6. 路由设置页信息架构与发布边界

页面最终分为“重试与切换”“超时”“故障保护”“解释与诊断”四组，但不能一次把所有参数开放。每个字段必须有一个编译后的生产消费者、一个 trace 证据字段和一个升级/回滚语义；否则只显示当前有效值，不允许编辑。

### 6.1 首版可持久化字段

第 4.3 节完成后，首版只开放下列**容量重试**字段。它们与已有生产 envelope 完全对齐，避免在没有通用 action model 前承诺“任意故障都能等待或跨域”。

| 字段 ID | 中文标签 | 类型与默认值 | 首版约束 | 精确定义 |
| --- | --- | --- | --- | --- |
| `maxTotalAttempts` | 最大总尝试次数 | integer，默认 `4` | `1..4` | 包含第一次 outbound attempt；普通 replan、同目标重试和容量跨域分支共用同一预算 |
| `maxSameTargetCapacityRetries` | 同目标容量重试次数 | integer，默认 `2` | `0..2` 且小于总尝试次数 | 只适用于 canonical `RetrySameTarget`，且必须通过 replay gate |
| `capacityRetryWaitBudgetSeconds` | 容量重试等待总预算 | duration，默认 `2 秒` | `0..2 秒` | 仅覆盖容量路径的 jitter/`Retry-After` 等待；睡眠和队列均受 precommit deadline 裁剪；运行时换算为毫秒 |
| `allowCrossCapacityDomainFallback` | 容量不足时允许跨故障域兜底 | boolean，默认 `true` | replay gate 不允许时强制停止 | 同一 capacity domain 耗尽后，最多允许一个不同 capacity domain 的 outbound 分支 |

`maxTotalAttempts` 的范围暂时不得超过 `4`：现有 admission、execution、trace hard cap 和已验证的 capacity profile 都以该上限为边界。扩大上限需要新 profile version、jitter 曲线、内存/队列重新预算和完整故障注入验证，不能只改输入框范围。

同时开放版本化 `timeoutPolicy`，但它只对保存后新启动的本地路由实例生效；当前运行实例的实际限制继续作为只读运行时事实展示。字段为 `connectSeconds`、`firstByteSeconds`、`precommitSeconds`、`bufferedExecutionSeconds` 和 `streamIdleSeconds`，默认值分别为 `10`、`30`、`60`、`300` 和 `90` 秒。所有字段最小 `1` 秒，上限分别为 `120`、`300`、`600`、`1800` 和 `600` 秒；`precommitSeconds` 不得大于 `bufferedExecutionSeconds`。用户配置和 IPC 均使用秒（允许小数），运行时创建 `Duration` 时统一换算为毫秒。

### 6.2 运行时事实与生效边界

下列值来自当前运行实例的 `TransportPolicyStore` 或安全门。页面同时显示策略草稿值和“当前运行实例值”，并明确区分后续新请求与在途请求：

| 显示项 | 当前默认值 | 原因 |
| --- | --- | --- |
| 上游连接超时 | `10 s` | 出站 HTTP client 构建时的服务器级限制 |
| 首字节超时 | `30 s` | 由 proxy execution 和 server limits 共同消费 |
| precommit 超时 | `60 s` | 这是“输出提交前”的总预算，不是完整请求的总 deadline |
| 非流式执行超时 | `300 s` | buffered execution 限制 |
| 流式静默超时 | `90 s` | 输出开始后只负责终止和记录，不能触发普通重放 |
| `Retry-After` | 系统受限执行 | 目前只在已实现的容量路径使用；不能提供会改变安全语义的普通开关 |

### 6.3 后续字段的准入条件

| 候选字段 | 允许进入设置页的前置条件 |
| --- | --- |
| `allowCrossFailureDomainFallback`（通用） | typed `RetryAction` 已区分容量、等待和普通换域，并为每种动作完成 replay/deadline 测试 |
| 真正的 `requestDeadlineSeconds` | 明确定义跨排队、attempt、等待和输出阶段的总 deadline，并替代当前仅 precommit 的时间预算命名 |
| `protectionProfile` | 已通过 V2 policy/CAS、严格校验、0050/0052 物化迁移、reducer 恢复和 Credential-scoped admission bridge 接入；设置页可编辑窗口、最小样本、失败率和 Half-Open 成功次数，默认关闭。它只影响保存后创建的请求，不改变重放安全边界 |
| 半开阈值、错误率窗口等 `customProtection` | 当前只开放 `protectionProfile` 已有字段；bounded history、恢复、Credential-scoped typed planner admission bridge、Credential-scoped 真实 outbound probe lease、lease-race strict refresh、按模型 status query 和基础 Provider/domain 聚合诊断已接入且默认关闭。其他 scope 的 probe resolver、synthetic Provider 请求和完整历史仍不开放 |

`showDecisionExplanation` 是本地 UI 展示偏好，不影响候选选择、attempt 或健康状态。它必须存放在应用显示设置中，而非 `routing-policy.json`，不得生成 routing policy revision。

### 6.4 不开放为用户字段的安全门

以下规则由系统强制执行：

- `replaySafety`：根据请求幂等性、transport 三态和 commitment 判断能否重放。
- canonical failure 分类器：用户不能修改错误码到意图的映射。
- committed 后停止：下游已收到输出或上游可能已接受请求时，不得普通重试。
- 总内存、trace、body buffer、队列和 attempt 硬上限。
- 凭据、能力、倍率、余额、用户显式禁用和安全策略的硬资格过滤。

## 7. 配置模型与版本化契约

### 7.1 公开策略文档

沿用 `routing-policy.json` 的完整文档模型。通过第 4.3 节后，首版新增字段放在 `policy.retryFailover`，不在顶层散落重试参数；transport timeout、UI 展示偏好和未生产化的熔断参数不得提前进入此文档。

```json
{
  "formatVersion": 1,
  "baseRevision": 42,
  "policy": {
    "version": 2,
    "reliabilityWeight": 4000,
    "responsivenessWeight": 2500,
    "costWeight": 2000,
    "preferenceWeight": 1500,
    "maxCandidates": 64,
    "explorationShareBasisPoints": 500,
    "allowDepletedFallback": false,
    "retryFailover": {
      "version": 1,
      "maxTotalAttempts": 4,
      "maxSameTargetCapacityRetries": 2,
      "capacityRetryWaitBudgetSeconds": 2,
      "allowCrossCapacityDomainFallback": true
    }
  }
}
```

### 7.2 版本和兼容性

- `formatVersion` 继续表示 JSON 文档外壳；只有外壳变化时才升级。
- `policy.version` 从 `1` 升为 `2`，表示新增 `retryFailover` 语义；必须提供 `v1 -> v2` additive upgrader 和能够读取两种版本的 decoder。
- 存储、算法和系统版本分别由现有版本 owner 管理，不能复用 `policy.version`。
- v1 策略迁移必须写入与现有 hard-code 完全等价的值：`4`、`2`、`2` 秒、`true`。迁移记录 `source = migration`，但不得改变已在飞请求的 snapshot 或安全门。
- 已有 V2 存储中以毫秒命名的持续时间字段由迁移 `0054_routing_policy_duration_seconds.sql` 转换为秒字段；active policy 与 policy history 都转换，revision 保持不变。缺失、异常或同一对象同时出现新旧字段时迁移 fail-closed，不静默猜测。读取兼容层仍可把旧 V2 毫秒 JSON 转换为小数秒（例如 `750 ms -> 0.75 秒`），新写入只产生秒字段。
- public document decoder 必须拒绝未知字段、重复键、错误类型、越界数值和无法识别的枚举；不能把当前 storage serde 形状直接宣布为公开 schema。
- 未实现的后续字段必须被 public decoder 拒绝；不得以 `null`、默认值或“预留字段”静默保存。

### 7.3 约束校验

至少满足：

```text
retryFailover.version = 1
1 <= maxTotalAttempts <= 4
0 <= maxSameTargetCapacityRetries <= 2
maxSameTargetCapacityRetries < maxTotalAttempts
0 <= capacityRetryWaitBudgetSeconds <= 2
```

编译后的 `AttemptBudgetProfileV1` 还必须证明它能同时满足 execution、admission、capacity registry 和 trace 的固定硬上限。保存时必须返回字段级 validation error；任何字段校验失败都不能部分写入。

## 8. 请求执行协议

本节描述首版已实施的请求协议。生产代码已有 classifier、replay gate、admission coordinator、容量特例和统一 typed `RetryAction`；proxy 请求全链路使用 ingress-owned deadline，非 proxy planning caller 必须显式传入绝对 deadline context；通用 timeout 编辑和完整历史事件时间线仍在后续准入范围。

### 8.1 总流程

```text
接收请求
  -> 生成 RouteRequestFacts 与 ingress 起点的 precommit deadline context
  -> 读取带 policy revision 的 PlanningSnapshot
  -> 选择候选并取得 capacity lease
  -> 执行一次 outbound attempt
  -> 收集有界错误证据
  -> canonical failure 分类
  -> replay gate
  -> RetryActionPlanner 计算动作和剩余预算
  -> 更新健康 / 容量域 / 故障域状态
  -> 原目标重试、等待后重规划、跨域切换或终止
  -> 写入 bounded decision trace 和最终终态
```

### 8.2 `RetryAction` 合同

`RetryActionPlanner` 必须是现有 `retry_decision_from_canonical` 的演进 owner，而不是在 execution 外另加一套规则。它的输出至少为：

```text
Stop { reason }
RetrySameTarget { capacityDomain, delay, remainingBudget }
WaitThenReplan { excludedDomains, delay, remainingBudget }
TryDifferentFailureDomain { excludedDomains, remainingBudget }
```

每个 action 都必须带入 canonical failure code、replay gate 结论、policy revision、attempt ordinal 和不可变的 request-local budget。execution 只能执行该 action；它不得再次依据 HTTP status、legacy `RetryClass` 或页面字段猜测下一步。

### 8.3 预算规则

- `maxTotalAttempts` 是整个逻辑请求的硬上限；普通 replan、capacity retry 和跨域切换不得重置它。
- 首版预算为 `precommit` budget；deadline 起点已前移到 ingress，等待和 attempt 会消费同一 request-local context。
- proxy execution 的规划/快照/价格等前置 I/O 已统一包裹剩余 deadline，并由 ingress 起点的 request-local context 约束；非 proxy 直接调用方必须显式传入 caller-owned absolute context，service 不自动重置预算。deadline 仍属于 transport/runtime owner，不开放为用户策略字段。
- 任意预算为零、deadline 到期、取消、关闭或 shutdown 时，必须停止并给出对应终态原因。
- 同一容量域最多消耗 `maxSameTargetCapacityRetries`；同域 sibling Key 不得被普通 Planner 当作新故障域绕过。
- 首版的跨域 fallback 仅指 capacity-domain：最多消耗一个独立 outbound 分支，其后不再开启无界 retry 链。通用 failure-domain fallback 由 Phase 2 的 typed action 单独定义，不能借用此设置偷跑。
- 容量路径中的 `Retry-After` 只提供等待建议，最终等待值为：

```text
min(retry_after, capacity_retry_wait_budget_remaining, precommit_budget_remaining)
```

并应用确定性 jitter 与最小/最大等待边界。

通用 `WaitThenReplan` 的 typed action 和容量等待路径已接通；rate-limit 的通用等待仍需独立等待预算，首版不得把其意图文案伪装为已经等待了 `Retry-After`。

### 8.4 replay gate

重试动作必须同时满足：

1. 错误类别允许重试；
2. 请求尚未被下游提交为不可撤销结果；
3. transport 状态允许证明请求未被接受，或请求具备明确幂等语义；
4. body backing storage 仍可安全复用；
5. 剩余 attempt、等待和适用的 precommit/request deadline 足够。

当前 reqwest 三态边界为 `NotConnected`、`ResponseStarted` 和 `Unknown`。`Unknown` 对非幂等请求必须 fail-closed；`ResponseStarted` 或下游已提交后不得普通重放。

## 9. 错误分类与动作矩阵

下表以现有 canonical classifier 为基线，定义 typed action 收敛后的行为。所有标为“条件允许”的 retry action 都必须先通过第 8.4 节 replay gate；意图本身不是重放授权。

| canonical failure | 基线意图 | 保护作用域 | 动作边界与用户解释 |
| --- | --- | --- | --- |
| `request_invalid` / provider bad request | `StopRequest` | 无 | 请求本身无效，重试不会改变结果 |
| confirmed `model_unavailable` | `StopRequest` | model capability | 写入不支持证据供后续请求排除；当前请求不猜测改写模型 |
| `capability_mismatch` | `TryDifferentFailureDomain` | protocol/capability | 仅在存在等价候选且 replay gate 允许时重新规划 |
| `auth_invalid` | `TryDifferentFailureDomain` | credential，`Blocked` | 不再发送同一凭据；允许尝试独立故障域，不是“把认证失败重试多次” |
| `quota_or_billing` | `TryDifferentFailureDomain` | account | 当前账号额度、订阅或余额不可用 |
| `rate_limited` | `WaitThenReplan` | account，`Cooldown` | 保留 typed intent；通用等待预算未交付前，不承诺已执行 `Retry-After` 等待 |
| `provider_capacity` | `RetrySameTarget` | capacity domain，进程内 Open/Half-Open | 可在同一目标有界等待重试；耗尽后最多一个跨 capacity-domain 分支 |
| `transport` / precommit `timeout` | `TryDifferentFailureDomain` | endpoint，`Degraded` | 不确定是否已被接受时，非幂等请求停止；可安全重放时才换域 |
| `upstream_5xx` / `overloaded` | `TryDifferentFailureDomain` | endpoint 或 uncertain | 与 transport 相同，不能把 5xx 一律当作安全重试 |
| `stream_idle_timeout` / `stream_interrupted` | `StopRequest` | endpoint observation | 输出通道已经建立或可能已提交，为避免重复执行不自动重放 |
| `malformed_response` / generic 4xx | `StopRequest` | 通常无 | 证据不足或响应无效，默认 fail-closed |
| `committed_or_unknown` | `StopRequest` | 记录 observation | 请求可能已被上游接受，系统为避免重复执行而停止 |

分类器必须输出稳定的 `failureCode`、`retryIntent`、`replaySafety`、`scope` 和 `explanationKey`。UI 不得根据 HTTP 状态码自行推断动作。

## 10. 跨请求故障保护状态机

### 10.1 当前状态与目标状态的边界

当前生产中存在三类不同机制，但它们不是一个统一的“熔断器”：

1. **durable scoped health verdict**：`Degraded`、`Cooldown`、`Blocked`，持久化并投影到下一次 planning snapshot；它不是完整的 Closed/Open/Half-Open breaker。
2. **capacity retry registry**：按 capacity domain 的进程内 Open/Half-Open 和单 probe；代理重启后清空，且只代表短暂容量保护。
3. **error-rate reducer/history**：由 observation transaction 持久化并通过 `ProtectionStatus`、history IPC/UI 展示；默认关闭。V2 policy 的 enabled、窗口、最小样本、失败率阈值和 Half-Open 成功次数由同一 CAS 写入，并通过同源 Credential scope commitment 接入 planner admission：Closed/缺失放行，Open 抑制，Half-Open 仅匹配有效 probe lease 放行。reducer 的其他 scope 类型不代表已有真实 proxy probe 编排。

以下状态机是当前 error-rate protection reducer 的生产状态机；它已通过 V2 policy/CAS、Credential-scoped admission bridge 和 Credential-scoped 真实 proxy outbound probe lease 接入，但默认关闭。它不是自动 synthetic Provider probe 或完整的 Provider pool-ejection 调度器；`RuntimeOutlierPolicyV1` 的 test-only 状态不构成生产机制。

实现注意：health reducer 的内部 `Closed` 表示“该 scope 当前未打开保护，仍在收集/评估证据”，不等于“没有任何健康事实”。首版保留现有 projector 的 `Closed -> Degraded` 映射，但必须附带稳定的“当前未打开保护、仍在监控”解释；`NoProtection` 只表示没有可展示的保护/观测条目，不能把 `Degraded` 当作已经打开熔断。后续若引入独立 `Monitoring` 状态，必须升级 read-model 版本而不是复用旧枚举含义。`Closed`、`Degraded`、`Cooldown`、`Open` 和 `HalfOpen` 的映射必须由 projector 统一定义并有测试，前端不得自行推断。

### 10.2 目标状态

```text
Closed
  --达到失败阈值或错误率条件--> Open
Open
  --冷却完成--> HalfOpen
HalfOpen
  --探测成功达到阈值--> Closed
HalfOpen
  --任一探测失败--> Open
```

真实 probe 的边界必须固定：当前仅 Credential scope 进入生产编排。首轮 planning 只能发现冷却已到期的 Credential Open scope；第二轮 planning 必须携带准确的 `stateRevision` fence；只有选中的 candidate 与 Credential probe scope 一致、目标解析成功且即将跨 outbound boundary 时才消费 lease。目标解析、admission、deadline、replan 或取消未跨 outbound boundary 时，必须执行 revision-fenced cancel；出站后没有可归因 health outcome 的失败也必须释放 lease且不写入样本。迟到结果按 stale 处理，不能改变新 revision 的状态。该机制由真实用户请求触发，不自动制造 synthetic Provider 请求。account/group/endpoint/model 要获得同等能力，必须先实现统一 scope resolver 并补跨层证据。

状态机属于 health scope，不属于单次请求。每个作用域必须持有 `stateRevision`、`openedAt`、`cooldownUntil`、失败计数摘要、最近一次非敏感 reason 和 persistence kind。UI 必须区分 `durable`、`runtime_capacity` 与未来 `runtime_outlier`，不得把重启后消失的状态标为持久熔断。

### 10.3 作用域与更新规则

- 容量失败只更新容量域；它不能直接把凭据标记为 hard fail。
- 认证、余额、订阅等错误优先更新 credential/account 作用域。
- durable scoped health verdict 的 transport 和 5xx 默认更新 endpoint scope；error-rate observation/admission 的当前 production resolver 优先绑定 Credential commitment。只有统一 scope resolver、同源 commitment 和对应 probe/恢复证据齐备后，才可把其他 scope 宣称为 error-rate breaker 的实际作用域。
- capability 失败更新能力作用域，并让下一次 Planning Snapshot 排除不支持该能力的候选。
- durable 保护状态更新必须幂等、持久化且带冷却时间；进程内 capacity 状态必须通过 RAII/取消释放，不承诺跨重启保存。失败的状态写入不能伪造请求成功。

### 10.4 preset 语义

| preset | 适合场景 | 行为倾向 |
| --- | --- | --- |
| `conservative` | 上游昂贵、请求副作用高 | 更少重试、更快打开保护、更慢恢复 |
| `balanced` | 默认 | 使用经过冻结的 durable 冷却和容量保护基线 |
| `aggressive` | 候选多、瞬态故障频繁 | 允许更多瞬态预算、更晚打开保护、更快恢复，但仍受总 deadline 和安全门限制 |

preset 仍不是当前可保存字段；当前保存的是 `protectionProfile` 的受限参数。具体 profile 和 algorithm version 必须在 decision trace/status 中可追溯。

## 11. 决策解释与可观测性

### 11.1 Trace 事件

当前 runtime trace 已有 `attempt_start`、`canonical_failure`、`same_target_retry`、同域抑制、跨 capacity-domain fallback、fail-closed 和 request terminal 等事件；最多 4 个 outbound attempt、64 个事件、32 KiB 序列化估算，并保留在进程内 ring。下列事件是本升级的目标 trace contract，不得把尚未记录的事件标为当前事实：

- `request_started`
- `candidate_selected`
- `attempt_started`
- `failure_classified`
- `retry_suppressed`
- `retry_scheduled`
- `same_domain_retry_exhausted`
- `candidate_replanned`
- `failure_domain_excluded`
- `protection_opened`
- `half_open_probe_started`
- `protection_closed`
- `request_finalized`

运行时详细事件至少包含：事件序号、相对耗时、attempt ordinal、策略 revision、failure code（如有）、retry intent、剩余预算、候选/故障域的非敏感稳定标识和 `explanationKey`。当前跨重启的 durable lifecycle event 是有意更窄的摘要契约，只保存有界的 `eventKey`、`sequence`、`occurredAtMs`、`eventKind`、`detailCode`、`attemptOrdinal`、`retryDisposition` 和 `outputCommitted`；策略 revision、剩余预算、候选/故障域标识等未持久化字段必须在 UI 标记为不可用或仅本次运行可用。这样既兑现历史请求的受限可解释性，也不把摘要表误称为完整 durable trace；不得依赖加大 runtime ring。

不得写入 API key、Cookie、Authorization、完整 endpoint/query、请求正文、完整上游响应体或高基数动态 message。错误正文只能保存经过 bounded parser 的类别和截断安全摘要。

### 11.2 用户可见文案

UI 应将内部事件翻译为短句，例如：

- “上游返回容量不足，已在同一目标等待 0.42 秒后重试（剩余 1 次）。”
- “该账号连续失败，已暂时冷却至 14:32；本次请求改用其他故障域。”
- “请求已经开始输出内容，为避免重复执行，本次不再重试。”
- “凭据失效，未继续重试；请检查该站点的密钥。”

请求详情应显示一条时间线，并允许展开“技术详情”。首版若 trace 已被驱逐或应用重启，必须显示 durable terminal summary、durable lifecycle event summary（若存在）以及“详细步骤不可用”，不能把摘要事件伪造成完整 runtime 时间线。默认状态页显示由单一 `ProtectionStatus` projector 生成的状态、冷却剩余时间、最近失败类别、作用域和持久化类型。

### 11.3 指标

至少记录以下低基数指标：

- attempts total；
- retries by intent and failure code；
- retry suppressed by replay gate / deadline / budget；
- same-domain capacity retry；
- cross-domain fallback；
- protection transitions；
- half-open probe success/failure；
- final terminal reason；
- decision trace truncation / persistence failure。

候选、账号和站点标识必须使用受控标签或本地映射，禁止把 secret、完整 URL 或原始请求 ID 作为指标标签。

## 12. 前端交互规格

### 12.1 页面结构

1. **重试与切换**：第 6.1 节的容量字段，清楚标注“只影响安全可重放的容量故障”。
2. **超时**：编辑第 6.1 节的 `timeoutPolicy`，同时显示第 6.2 节的当前运行实例值和 owner；保存成功后提示仅后续新请求使用新值，在途请求保持旧快照。
3. **故障保护**：显示 `ProtectionStatus`，区分持久化冷却、进程内容量保护和不可用状态；显示已接入且有真实消费者的 `protectionProfile` 字段。真实 probe 是请求执行内部的只读行为，不提供 synthetic probe/pool-ejection 输入框。
4. **解释与诊断**：本地展示偏好、最近决策、当前运行 trace 可用性、durable summary 和 Provider/failure-domain 聚合诊断。

页面必须先从局部 `useState` server state 迁移到配置系统规格要求的 shared policy query、草稿和 typed conflict resolver，才可增加字段。它还必须覆盖 loading、保存中、保存成功、字段校验失败、CAS 冲突、外部文档变更、受管 JSON 无效和窄窗口状态；冲突不得静默覆盖。

### 12.2 交互规则

- 将 `maxTotalAttempts` 调小到低于已有 `maxSameTargetCapacityRetries + 1` 时，给出字段级修正建议；保存仍必须由后端拒绝非法组合，前端不得悄悄截断。
- 关闭 capacity-domain 跨域兜底时，说明它只影响容量路径且可能导致更早失败；不能承诺“绝不失败”。
- timeout 输入必须是真实草稿字段，保存由后端统一校验；当前实例值保持只读，并明确标注“保存后对后续新请求生效”。
- `protectionProfile` 字段编辑必须显示默认关闭、范围和“只影响后续请求”；真实 probe 不增加独立开关，synthetic probe、pool-ejection preset 只有在新增对应执行编排和安全门后才能加入。
- 任何字段旁提供“当前生效 revision”和“只影响后续请求”的说明。
- 取消或恢复默认值必须是可撤销草稿操作，只有保存后才生成新 revision。

## 13. 后端与 IPC 契约

### 13.1 Domain 类型

Rust 领域层新增或扩展：

```text
RetryFailoverPolicyV2
AttemptBudgetProfileV1
RetryAction
ReplaySafety
CanonicalFailureCode
ProtectionStatus
DecisionExplanationKey
```

这些类型必须由领域 owner 定义，前端使用生成的 TypeScript binding；禁止手写平行字符串 union。

### 13.2 Service 边界

- `RoutingPolicyService`：读取、校验、编译、CAS、迁移和 revision 通知。
- `RetryActionPlanner`：演进现有 retry decision，基于 request facts、policy snapshot、failure outcome 和 request-local budget 生成下一动作。
- `FailureClassifier`：唯一 canonical failure owner。
- `ReplayGate`：唯一重放安全 owner。
- `HealthProtectionReducer`：跨请求状态转移的唯一 owner；首版通过 `ProtectionStatus` 投影 durable verdict、runtime capacity 和默认关闭的 error-rate observation/history。error-rate 状态只有在显式启用 typed bridge 时才参与匹配 scope 的候选 admission，默认不改变候选选择。
- `DecisionTraceRecorder`：唯一有界 trace owner。

Proxy execution 不得自行解析设置、拼接错误码或创建第二套 retry budget；Planner 不得直接写健康状态，必须通过 typed transition。`RetryActionPlanner` 是实际动作唯一 owner；lifecycle 的 `TryNextCandidate` 仅是兼容投影，不能驱动下一步执行。

### 13.3 IPC 命令

现有命令已经提供 `load_routing_policy`、`apply_routing_policy_document` 和 `get_request_decision_trace`。在不破坏 generated registry 的前提下，新增或扩展以下能力：

```text
load_routing_policy()
validate_routing_policy_document(document)
apply_routing_policy_document(document)
get_routing_protection_status(filter)
list_error_rate_history(filter)
get_request_decision_trace(requestLogId)
```

`requestLogId` 只能查询本地已存在的受限 trace；命令不得返回原始凭据、请求正文或完整上游响应。

## 14. 迁移与回滚

- 从 `policy.version = 1` 升级到 `2` 时补齐第 7.2 节的 baseline-equivalent `retryFailover`，不是抽象的 `balanced` preset。
- 迁移必须遵守 [`../SCHEMA_UPGRADE_AUTHORING.md`](../SCHEMA_UPGRADE_AUTHORING.md)：additive、可恢复、可重建；旧策略无法解析时保持旧 active policy，不得启动空策略。
- 升级 decoder 必须先解析 v1/v2 tagged union，再转换为当前领域类型；不得通过给 `RoutingPolicyConfigV1` 增加 serde default 假装完成版本升级。
- 新字段写入现有 policy history，记录 `source = migration`、旧 revision、新 revision、`AttemptBudgetProfileV1` version 和算法版本。
- 恢复旧 revision 时，重试字段与其他路由字段作为完整文档一起恢复，禁止字段级回滚。恢复为 v1 时应走同一 upgrader，再以当前 baseline 编译；不得在 execution 中保留 v1 分支。
- 配置文件镜像失败不影响已提交 SQLite active policy；必须显示现有 document-sync 的 `pending_write` / `unavailable` 等状态，并在后续重试收敛。
- 未知的未来 `policy.version` 必须 fail-closed，保留当前 active policy。

## 15. 分阶段实施

### Phase 0：基线、owner 收敛与替换契约

- 固化当前 `4 / 2 / 2 秒 / true` 行为的 loopback、fault、replay 和 trace fixtures；运行时断言仍可使用换算后的毫秒值。
- 引入编译后的 `AttemptBudgetProfileV1`，移除四处独立尝试上限和无消费者的 `max_upstream_attempts`；所有真实 attempt、容量 retry、admission 与 trace 使用同一实例。
- 将 `RetryDecision` 演进为 typed `RetryAction`，保持分类器和 replay gate 作为现有 owner；禁止在此阶段改变默认重试行为。
- 明确 scoped verdict、legacy health snapshot、capacity runtime state 的读写职责，并先提供只读 `ProtectionStatus`。
- 将页面局部 policy state 替换为 shared query、草稿和 typed CAS conflict resolver。

### Phase 1：容量重试控制面与解释基础

- 增加 `RoutingPolicyConfigV2` / `RetryFailoverPolicyV2`、严格 public decoder、迁移和 compiler。
- 仅接入第 6.1 节四个字段；policy snapshot 与 execution snapshot 必须携带同一编译 profile。
- 把现有 runtime trace、durable attempt summary 和有界 lifecycle event summary 以真实可用性呈现；为缺失事件增加 stable explanation key。重启后可恢复受限的有序生命周期摘要，但不承诺恢复完整 runtime 时间线或所有技术字段。
- UI 只保存后端实际执行的字段，且每个字段都能从 trace 看见 policy revision 与 effective value。

### Phase 2：通用故障转移动作与 transport policy

- 容量路径已保留 `RetrySameTarget`、等待 budget 和跨域证据；`WaitThenReplan`/`TryDifferentFailureDomain` 的 typed action、排除域和 planner I/O deadline 已接入 proxy execution，通用 rate-limit 等待预算仍需单独准入。
- 独立的 versioned `TransportPolicySnapshot` 已编译并按 request snapshot 生效；proxy execution 的 planning snapshot、affinity、route snapshot 和 pricing 前置 I/O 已使用剩余 deadline。`timeoutPolicy` 已进入 V2 policy/CAS、迁移、IPC、设置页和运行时发布；保存后由后续新请求消费，在途请求继续使用旧快照；非 proxy 调用方仍必须显式传入 caller-owned deadline context。
- attempt lifecycle 已构建 bounded durable attempt summary 与 lifecycle event summary；完整历史事件时间线仍不属于首版承诺，UI 必须区分“可恢复的有界摘要”和“当前运行的详细步骤”。

### Phase 3：统一故障保护

- durable reducer、恢复、Half-Open 算法、显式 probe lease 和 `ProtectionStatus` 已接入；V2 policy/CAS 可配置字段与 scope/admission typed bridge 已接入但默认关闭，不能把 public `Degraded` 误解为 Open。
- Credential-scoped 真实 outbound probe lease 已由 `RoutingService`/proxy execution 编排：它不自动制造 synthetic 请求；Credential scope 的 transition、恢复和保留策略继续由 reducer 单一 owner 管理。路由工作区展示低敏感的故障域候选聚合、已解析/待模型/未配置/身份无效状态和保护 join，不把身份配置误报为运行时健康；该聚合诊断不等价于其他 scope 已具备 probe/熔断执行。status IPC 支持可选 requested model；未提供模型时生产查询对需要模型的域返回 `model_required`，提供模型时才生成对应 model-scoped health join。

### Phase 4：错误率与更丰富诊断

- 已交付默认关闭的 observation、reducer persistence、bounded history、IPC/query/UI、status projection、V2 policy profile、同源 Credential commitment 到 planner admission 的 typed bridge、Credential-scoped 真实 outbound probe lease 和基础 Provider/domain 聚合诊断；设置页可通过同一 CAS 修改受限窗口/阈值字段。probe 不自动制造 synthetic Provider 请求，基础聚合不等于完整 model-scoped health view，也不证明其他 scope 已有真实 probe。
- 后续只实现完整历史步骤和 raw IPC 原文重复键保证；bridge 默认关闭，不能绕过 replay gate。
- 启用前必须有明确配置/迁移/回滚和安全门测试；禁止用 test-only `RuntimeOutlierPolicyV1` 或旧 health snapshot 双写拼装实现。

## 16. 验证与验收标准

### 16.1 领域和后端

- 每个 canonical failure code 都有分类、意图、作用域和 replay gate 测试；测试还必须证明 typed `RetryAction` 没有退化回 `NextCandidate`。
- policy compiler 生成的同一 `AttemptBudgetProfileV1` 必须被 execution、admission、capacity retry 和 trace 消费；任一消费者缺失时 admission test 失败。
- 总尝试、同域尝试和容量等待预算在 replan 后不重置；规划 I/O 的剩余 deadline 包装已在 proxy execution 收口，并有 planning timeout fixture。
- committed、`ResponseStarted` 和非幂等 `Unknown` 均不得普通重放。
- capacity 同域 sibling 不被错误轮询；跨域 fallback 最多消耗规定分支。
- capacity Open/Half-Open 的并发、取消与重启语义无泄漏、无重复终态；保护 profile 的配置校验、迁移、恢复和 reconfigure 保持状态测试；真实 Provider probe 编排上线后再追加端到端执行测试。
- v1 -> v2 迁移后的默认策略在所有 baseline fixture 上与升级前等价；raw/file document 入口对未知字段、重复键和未来版本 fail-closed，已解析 IPC `Value` 不宣称具备原始重复键检测。
- `ProtectionStatus` 绝不把 runtime capacity 状态伪装成 durable 状态，test-only outlier 绝不进入 production snapshot。
- 策略 CAS 冲突、未知字段、越界值、未来版本和文件损坏均 fail-closed。

### 16.2 前端

- 每个字段有默认值、范围、帮助文案、错误态和窄窗口布局。
- 修改设置后，后续请求使用新 revision；进行中的请求保持原 snapshot。
- 请求详情能区分“运行时详细 trace”和“仅 durable summary”，并能显示“重试 / 切换 / 停止”的真实原因和剩余预算。
- 加载、保存、字段校验、typed CAS 冲突、外部文件变更、恢复默认值和每个字段实际生效均有测试。

### 16.3 推荐验证命令

首版实现与本轮审计按改动范围选择以下检查，不要求每次全部运行，也不构成发布资格：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib <相关模块>
pnpm.cmd test -- <相关 Vitest 文件>
pnpm.cmd build
pnpm.cmd test:contracts
# 跨层共享基础设施改动时再追加：
pnpm.cmd verify:fast
```

跨层契约或共享路由基础设施变化时，追加对应 integration test；不要求发布、安装包、真实 Provider、性能压测或 `verify:release`。任何未完成或因环境失败的验证必须在实施记录中明确说明。

## 17. 完成定义

首版可以标记为“首版已实施”的条件：

1. 第 4.3 节列出的预算、动作、健康和 trace owner 已收敛；无消费者字段和二值 action 已删除或完全迁移。
2. 首版设置字段均有唯一编译后消费者，并能从 trace/summary 证明 effective value；超时和 advanced breaker 不在未实现时出现在策略文档。
3. 错误分类、replay gate、RetryAction planner、health reducer 和 trace 没有第二套 owner。
4. 单次请求重试、持久化健康冷却和进程内容量保护在 UI、文档、代码和测试中使用不同术语、状态和生命周期说明。
5. 认证、能力、请求错误和 committed/unknown 安全边界不可被设置绕过。
6. 默认策略与升级前 baseline 保持兼容，迁移、回滚、CAS 冲突和未知版本行为可验证。
7. 用户可以从请求详情和状态面板理解每次失败后的实际动作与 trace 可用性，而不需要阅读日志或源码。
8. 相关 Rust、前端、契约、架构和安全门禁通过，未运行的发布类检查明确排除并登记。

以下条件仍是将本文全部扩展能力继续演进的后续门槛：其他 health scope 的 probe resolver、完整历史步骤和 raw IPC 原文重复键保证。非 proxy 调用方的 deadline 传播、V2 protection profile、Credential-scoped error-rate reducer 到 planner verdict 的 typed bridge、bounded durable attempt summary、重启后有界 lifecycle event summary、Credential-scoped 真实 outbound probe 编排、lease-race strict refresh、按模型 status query 和基础故障域聚合 read-model 已实现，其中 bridge 默认关闭；当前 raw IPC duplicate-key 明确采用“raw/file 入口严格拒绝、已解析 Tauri `Value` 不承诺原文重复键检测”的兼容边界；若要改变该边界，必须另建 raw JSON DTO 迁移任务。
