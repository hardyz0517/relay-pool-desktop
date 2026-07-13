# Relay Pool Desktop 项目规划

## 1. 项目定位

Relay Pool Desktop 是一个本地桌面端 AI 中转站与 Key 池管理工具。

它不是网站，不是 SaaS，不是中转站后台，也不是 CCSwitch 的升级版或替代品。它和 CCSwitch 的关系是配合关系：

- CCSwitch 负责管理本机 AI 工具配置；
- Relay Pool Desktop 负责管理多个真实中转站，并对外暴露一个固定的本地 API 入口；
- Codex、Claude Code、Gemini CLI、CCSwitch 等工具只需要连接 Relay Pool Desktop 的本地入口；
- 背后的中转站账号管理、余额监控、倍率采集、Key 池排序、低价路由、失败切换由 Relay Pool Desktop 完成。

一句话定义：

> Relay Pool Desktop 是一个本地 AI 中转资产管理与路由网关控制台：对外提供固定 OpenAI-compatible 入口，对内管理多个 Sub2API / NewAPI / OpenAI-compatible 中转站账号及其 Station Key，持续采集余额、倍率、价格和模型能力，并通过变更中心追踪风险变化，最终根据能力、健康、价格、余额和策略进行本地路由。

### 1.1 参考与归因

自动路由调度参考了 Sub2API 的账号调度思路，但 Relay Pool Desktop 使用独立 Rust / TypeScript 实现，没有复制或链接其核心实现代码。

- 参考仓库：`https://github.com/Wei-Shaw/sub2api`
- 已审阅提交：`e316ebf52838a89d57fc790981cce7520f819ac8`
- 该提交观察到的许可证：LGPL-3.0
- 映射关系：Relay Pool Desktop 的 `Station Key` 对应 Sub2API 调度中的账号；Relay Pool 额外加入用户可配置的硬倍率上限和分组筛选边界。

## 2. 当前阶段

- P4 / P4.1 已完成登录态信息采集主线和 Key 池 MVP。
- P5 已完成本地 OpenAI-compatible 网关主干。
- P6 已完成模型 / 协议 / 健康感知路由层。
- P7 已完成价格归一化、余额快照、请求成本和 cheap_first 路由展示。
- P8 正在推进安全与凭据治理。
- P9 真实站点采集与路由事实层：补齐 Sub2API / NewAPI / OpenAI-compatible adapter，建立 group binding、倍率历史、collector run、价格归一化和路由经济解释，让 UI 和路由消费稳定事实而不是 raw snapshot JSON。

## 2.1 信息架构

- 总览：回答“现在有什么风险？”，展示本地代理、未读风险、今日请求、失败率和成本摘要。
- 中转站资产：回答“哪个站点资产状态好不好？”，展示站点、类型、Base URL、余额、倍率摘要、采集状态、Key 数、健康、更新时间和路由参与状态。
- Key 池：回答“哪把 Key 能不能路由？”，管理 Station Key 的启用、优先级、能力、模型范围、健康和备用状态。
- 路由规则：回答“为什么请求会走这把 Key？”，管理自动调度、候选分组、倍率限制、低余额边界、耗尽兜底、模型映射和路由模拟解释。
- 价格 / 倍率：回答“哪个站点更便宜？”，展示模型价格、分组倍率和模型可用性的跨站点矩阵，并管理模型基准价格。
- 渠道状态：回答“最近运行稳不稳？”，展示 Key / Channel 的成功率、延迟、冷却和最近请求状态。
- 变更中心：回答“最近有什么需要注意的变化？”，记录余额、Key、站点、采集、价格、倍率、模型和路由影响事件。
- 请求日志：回答“某次请求为什么成功或失败？”，展示请求、耗时、成本、fallback 和拒绝候选。
- 信息采集（高级工具）：回答“采集器为什么得到这些结果？”，运行站点采集任务，查看快照与任务记录，并在高级区域调整采集频率、超时和并发。
- 设置：回答“应用本身如何运行？”，管理本地代理、默认网络出口、数据目录和高级工具可见性。

## 3. 核心对象

### 3.1 Station

`Station` 是站点账号资产，表示一个中转站网站 + 一个用户登录账号。

它负责：

- name
- base_url
- station_type
- 登录账号 / 密码状态
- 登录状态
- 余额来源
- 分组来源
- 倍率来源
- 模型价格来源
- 采集快照
- 旗下 Station Key 列表

`Station` 不是最终请求路由对象。

### 3.2 Station Key

`Station Key` 是真正参与请求转发、路由、fallback 和健康检测的对象。

它负责：

- key 名称
- masked api key
- 所属 station
- enabled
- priority
- group / tier
- 状态
- 延迟
- 成功率
- 最近错误
- 模型适用范围
- 后续路由策略

### 3.3 Key Pool

`Key Pool` 是所有 station_keys 的统一管理视图。

它负责：

- 扁平展示所有 key
- 按中转站筛选
- 搜索
- 拖拽排序
- 启用 / 禁用
- 设置优先级
- 设置 fallback 顺序
- 查看健康状态
- 后续被本地 proxy / router 使用

### 3.4 Pricing Rule

`Pricing Rule` 是归一化的价格记录。

它负责：

- station / group / model 价格
- input / output / fixed price
- currency / unit
- source / confidence
- enabled 状态
- collected 时间

### 3.5 Balance Snapshot

`Balance Snapshot` 是归一化的余额或额度记录。

它负责：

- station / station key 余额
- scope
- value / currency / credit unit
- 阈值
- 状态
- source / confidence
- collected 时间

### 3.6 Request Cost

`Request Cost` 是单次请求的 usage 与成本元数据。

它负责：

- token 数
- estimated cost
- currency
- pricing rule 来源
- cost 状态

### 3.7 Group Binding

`Group Binding` 是 Station、站点分组和 Station Key 分组绑定之间的持久事实。

它负责：

- station 级分组身份
- key 级分组绑定身份
- group_key_hash
- 可选外部分组 ID hash
- binding status
- rate source
- effective multiplier
- confidence
- 最新可见状态

### 3.8 Collector Run

`Collector Run` 是 detect、balance、groups、models、full 等采集任务的任务级记录。

它负责：

- adapter
- task type
- parent run
- status
- endpoint counts
- duration
- manual action requirement
- 关联 collector snapshot

## 4. 路由职责

- `Collector` 围绕 `Station` 工作，负责登录和信息采集。
- `Router` 围绕 `Station Key` 工作，负责请求选择和 fallback。
- `Channel Status` 围绕 `Key / Channel` 工作，负责健康与请求状态展示。
- P7 在 Router 里新增 `cheap_first`，但它仍然先尊重 capability、health 和 cooldown。
- P9 后，`cheap_first` 只把 `complete` 价格当作精确价格；`group_rate_only` 只作为倍率解释，不参与精确低价排序。

## 5. P7 完成标准

P7 完成时应满足：

- 价格表能显示真实 `pricing_rules`
- 总览页能显示余额、请求数和成本摘要
- 请求日志能显示 token / cost 元数据
- 路由页能解释 `cheap_first` 和候选排序
- Key 池能显示简洁的余额 / 成本摘要
- 余额低或耗尽时，候选应被抑制或降权
- docs / README / 产品模型保持同一套术语

## 6. P8 安全与凭据治理

P8 安全与凭据治理：统一 SecretManager、本地加密存储、旧明文凭据迁移、UI 脱敏、日志 / 快照脱敏、导入导出安全边界和本地代理安全复核。

P8 不继续扩展路由、价格或采集能力。它的核心目标是让真实 Station Key、站点登录密码、token / cookie / session、collector snapshot 和 request log 可以在本地长期使用时保持可控暴露面。

## 7. 后续阶段

- 后续可继续扩展流式、策略、健康和成本联动，但不改变 Station / Station Key 的职责边界。
