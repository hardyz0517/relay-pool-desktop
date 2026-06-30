# Relay Pool Desktop 项目规划

## 1. 项目定位

Relay Pool Desktop 是一个本地桌面端 AI 中转站与 Key 池管理工具。

它不是网站，不是 SaaS，不是中转站后台，也不是 CCSwitch 的升级版或替代品。它和 CCSwitch 的关系是配合关系：

- CCSwitch 负责管理本机 AI 工具配置；
- Relay Pool Desktop 负责管理多个真实中转站，并对外暴露一个固定的本地 API 入口；
- Codex、Claude Code、Gemini CLI、CCSwitch 等工具只需要连接 Relay Pool Desktop 的本地入口；
- 背后的中转站账号管理、余额监控、倍率采集、Key 池排序、低价路由、失败切换由 Relay Pool Desktop 完成。

一句话定义：

> Relay Pool Desktop 是一个本地 AI 中转站与 Key 池调度器：对外提供固定 OpenAI-compatible 入口，对内管理多个 Sub2API / NewAPI / OpenAI-compatible 中转站账号及其下的 API Key，自动采集余额和倍率，并根据 Key 池优先级、模型能力、协议能力、健康状态和价格 / 余额策略进行本地路由。

## 2. 当前阶段

- P4 / P4.1 已完成登录态信息采集主线和 Key 池 MVP。
- P5 已完成本地 OpenAI-compatible 网关主干。
- P6 已完成模型 / 协议 / 健康感知路由层。
- P7 已完成价格归一化、余额快照、请求成本和 cheap_first 路由展示。
- P8 正在推进安全与凭据治理。

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

## 4. 路由职责

- `Collector` 围绕 `Station` 工作，负责登录和信息采集。
- `Router` 围绕 `Station Key` 工作，负责请求选择和 fallback。
- `Channel Status` 围绕 `Key / Channel` 工作，负责健康与请求状态展示。
- P7 在 Router 里新增 `cheap_first`，但它仍然先尊重 capability、health 和 cooldown。

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
