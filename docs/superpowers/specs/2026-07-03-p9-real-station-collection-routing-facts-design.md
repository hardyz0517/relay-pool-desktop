# P9 真实站点采集与路由事实层升级设计

## 1. 背景

Relay Pool Desktop 当前已经完成本地 OpenAI-compatible 网关、Key 池、模型/协议/健康感知路由、价格与余额展示、变更中心和中转站资产视图。现有产品形态已经超过“中转站配置工具”，正在转向本地 AI 中转资产管理与路由网关控制台。

当前短板集中在真实站点兼容和事实模型成熟度：

- Sub2API 采集还偏通用探针和登录态快照，真实余额、分组、倍率和 key group 绑定没有形成稳定事实链路。
- NewAPI 目前主要是类型和 UI 预设，缺少一等 adapter。
- `collector_snapshots.normalized_json` 承担了过多业务展示职责，但快照 JSON 不应成为路由、价格和 Key 池的主事实来源。
- 分组和倍率缺少稳定身份、当前状态和历史记录。
- `cheap_first`、route explain 和请求日志已有经济信息接口，但价格/倍率/余额来源还不够可信。
- 变更中心已经存在，但还需要从“部分表变更事件”升级为资产状态变化的审计入口。

P9 的目标是一次性定好终态架构，分阶段落地，避免后续在采集、价格、路由、UI 之间反复返工。

## 2. 目标

P9 将 Relay Pool Desktop 升级为“可信采集事实层 + 路由决策控制台”。

目标能力：

- 对 Sub2API、NewAPI、OpenAI-compatible 使用独立 adapter 采集。
- 将余额、分组、倍率、模型、价格、Key 绑定状态落成稳定事实表。
- 将 Station、Station Key、Group Binding、Pricing Rule、Balance Snapshot、Request Log 的职责边界固定下来。
- 将采集 facts 写入数据库，再由 change detector、pricing、routing、UI 消费。
- 让价格 / 倍率 / 余额进入路由时带来源、置信度、更新时间和解释。
- 让变更中心覆盖余额、倍率、价格、模型、Key、采集、路由影响。
- 保留采集中心为开发者模式调试页，普通页面只展示摘要和业务状态。

## 3. 非目标

P9 不做以下事情：

- 不做云端账号、多用户权限、团队协作或 SaaS 化。
- 不绕过 CAPTCHA、Turnstile、2FA，只提供手动 token 或 WebView 捕获登录态路径。
- 不把倍率直接伪造成最终模型价格。
- 不把完整 raw response、token、cookie、password 暴露给前端。
- 不新增和现有页面职责重复的页面。
- 不为了某个魔改站点写散落在 UI 或路由层的特判。

## 4. 后端模块结构

P9 后端数据流：

```text
真实站点接口
  -> collector adapter
  -> normalized facts
  -> apply layer
  -> persistence
  -> change detection
  -> pricing / routing / UI view models
```

建议模块：

- `services/collectors/mod.rs`
  按 `station_type` 分发任务，负责调度 adapter，不再固定调用 Sub2API 逻辑。

- `services/collectors/adapters/sub2api.rs`
  负责 Sub2API 的 usage、login、refresh、groups、rates、keys group resolve。

- `services/collectors/adapters/newapi.rs`
  负责 NewAPI 的 user self、user groups、quota 换算和 NewAPI 专属 header。

- `services/collectors/adapters/openai_compatible.rs`
  负责 `/v1/models`、基础可达性和模型能力探测。

- `services/collectors/session.rs`
  负责 access token、refresh token、password login、manual token、WebView capture 的统一 session resolve。

- `services/collectors/facts.rs`
  定义 adapter 输出的统一 facts。

- `services/collectors/apply.rs`
  将 facts 写入 `collector_runs`、`collector_snapshots`、`balance_snapshots`、`station_group_bindings`、`group_rate_records`、`pricing_rules` 和 `change_events`。

- `services/change_events.rs`
  从单点事件构造升级为支持 fact diff 和对象跳转。

核心原则：adapter 只翻译外部世界，不决定 UI 展示和路由策略。

## 5. 数据模型升级

### 5.1 `station_group_bindings`

表示站点下一个可识别、可绑定、可监控的 group。它是当前事实，不是历史快照。

字段建议：

- `id`
- `station_id`
- `station_key_id`
- `group_id_hash`
- `group_id_enc`
- `group_name`
- `binding_status`: `available | bound | missing | disabled | manual_legacy`
- `default_rate_multiplier`
- `user_rate_multiplier`
- `effective_rate_multiplier`
- `rate_source`
- `confidence`
- `last_seen_at`
- `last_checked_at`
- `last_rate_changed_at`
- `raw_json_redacted`
- `created_at`
- `updated_at`

用途：

- Station 详情展示完整 group 列表。
- Key 池展示 key 的 group 和倍率。
- 价格 / 倍率页面构建矩阵。
- 路由解释展示倍率来源。

### 5.2 `group_rate_records`

表示倍率历史，只在首次识别或发生变化时插入。

字段建议：

- `id`
- `station_id`
- `station_key_id`
- `group_binding_id`
- `group_name`
- `default_rate_multiplier`
- `user_rate_multiplier`
- `effective_rate_multiplier`
- `source`
- `confidence`
- `raw_json_redacted`
- `checked_at`
- `created_at`

插入规则：

- 没有上一条记录时插入。
- 任一倍率变化时插入。
- group 名称变化时插入。
- 倍率不变时只更新 `station_group_bindings` 当前事实。

### 5.3 `collector_runs`

表示一次采集任务的运行记录，让采集中心可以调试任务级状态。

字段建议：

- `id`
- `station_id`
- `adapter`
- `task_type`: `detect | balance | groups | models | full`
- `status`: `success | partial | failed | manual_required`
- `started_at`
- `finished_at`
- `duration_ms`
- `endpoint_count`
- `success_count`
- `failure_count`
- `manual_action_required`
- `error_code`
- `error_message`
- `snapshot_id`
- `created_at`

`collector_snapshots` 继续保留，负责脱敏快照和调试 raw。`collector_runs` 负责任务观测。

### 5.4 `station_credentials` 扩展

token 和 cookie 密文本身继续进入 `secrets`，业务表只保存 secret ref 和状态。

字段建议：

- `access_token_secret_id`
- `refresh_token_secret_id`
- `cookie_secret_id`
- `newapi_user_id`
- `token_expires_at`
- `token_refreshed_at`
- `session_source`: `password_login | manual_token | webview_capture | none`
- `session_status`: `valid | expired | manual_required | failed | none`

### 5.5 `pricing_rules` 扩展

字段建议：

- `station_key_id`
- `group_binding_id`
- `rate_multiplier`
- `base_price_source`
- `normalization_status`: `complete | group_rate_only | manual | unknown`
- `valid_from`
- `valid_until`

规则：

- 完整模型价格 + group 倍率可以生成 `complete`。
- 只有倍率时写 `group_rate_only`，不参与精确 cheap-first。
- 手动价格保留 `manual` source，可补足自动采集不足。

### 5.6 `station_keys` 扩展

字段建议：

- `group_binding_id`
- `group_id_hash`
- `rate_multiplier`
- `rate_source`
- `rate_collected_at`
- `balance_scope`: `station | station_key | unknown`

这些字段让 Key Pool 和 route explain 不再只依赖 `group_name` 文本。

## 6. Collector Facts

Adapter 输出统一 facts，再由 apply layer 落库。

建议 facts：

- `CollectedBalanceFact`
  包含 scope、value、used、total、currency、credit unit、status、source、confidence、collected_at。

- `CollectedGroupFact`
  包含 group_id、group_name、visibility、source、confidence、raw_json_redacted。

- `CollectedRateFact`
  包含 group_id、group_name、default_rate_multiplier、user_rate_multiplier、effective_rate_multiplier、source、confidence、checked_at。

- `CollectedModelFact`
  包含 model、availability、capability hints、source、confidence。

- `CollectedPricingFact`
  包含 model、group、input/output/fixed price、currency、unit、base source、normalization status。

- `CollectorDiagnosticFact`
  包含 endpoint、status、duration、error code、schema recognition result。

- `ManualActionRequiredFact`
  表示验证码、2FA、Turnstile、权限不足、需要手动 token 或 user id。

## 7. Sub2API Adapter

Sub2API adapter 任务拆分：

### 7.1 `collect_balance`

请求：

```text
GET {baseUrl}/v1/usage
Authorization: Bearer {apiKey}
```

兼容字段：

- `remaining`
- `quota.remaining`
- `balance`
- `used`
- `total`
- `planName`
- `plan_name`
- `group`
- `is_active`
- `isValid`

输出 `CollectedBalanceFact`，并从 plan/group 字段输出 group 线索。

### 7.2 `resolve_session`

优先级：

1. 已保存且未过期 access token。
2. refresh token 刷新 access token。
3. email/password 登录。
4. manual token 或 WebView capture。
5. 返回 `manual_required`。

401/403 后只允许刷新或重登一次，避免循环重试。

### 7.3 `collect_groups`

请求：

```text
GET {baseUrl}/api/v1/groups/available
GET {baseUrl}/api/v1/groups/rates
Authorization: Bearer {accessToken}
```

归一化：

- 保留 group id、group name、default multiplier、user multiplier、effective multiplier。
- rates 中只有 group id 时与 available 结果 join。
- 只有 group name 时允许落库，但 confidence 较低。
- 保留 redacted raw。

### 7.4 `resolve_key_group`

顺序：

1. 用户手动绑定的 `group_binding_id`。
2. `/v1/usage` 中的 plan/group 名称。
3. 只有一个可用 group 时低置信度匹配。
4. `/api/v1/keys?page=1&page_size=100` 反查完整 key 或 masked key。
5. 失败后提示用户手动绑定。

## 8. NewAPI Adapter

NewAPI 是一等 adapter，不是 Sub2API 的分支条件。

### 8.1 `collect_balance`

请求：

```text
GET {baseUrl}/api/user/self
Authorization: Bearer {accessToken}
New-Api-User: {userId}
```

quota 换算仅限 NewAPI adapter：

```text
remaining = quota / 500000
used = used_quota / 500000
total = (quota + used_quota) / 500000
currency = USD
source = newapi_user_self
```

### 8.2 `collect_groups`

请求：

```text
GET {baseUrl}/api/user/self/groups
Authorization: Bearer {accessToken}
New-Api-User: {userId}
```

兼容字段：

- `user_group`
- `userGroup`
- `groups`
- `items`
- `list`
- 对象 map 形式

倍率字段：

- `rate`
- `ratio`
- `rate_multiplier`
- `rateMultiplier`
- `default_rate_multiplier`
- `user_rate_multiplier`
- `effective_rate_multiplier`

### 8.3 Session 要求

第一版以 `accessToken + userId` 作为可靠路径。不同 NewAPI 部署的账号密码登录差异大，不在 P9 第一轮强推自动登录。

## 9. OpenAI-compatible Adapter

通用 adapter 只承诺通用能力：

- `GET /v1/models`
- 基础连通性检测。
- 模型列表和 capability hints。
- 不承诺余额、分组、倍率。

用户可继续手动维护价格，参与价格矩阵和路由。

## 10. Routing 联动

Route candidate economics 扩展为包含：

- `pricing_rule_id`
- `group_binding_id`
- `rate_multiplier`
- `normalization_status`
- `price_confidence`
- `balance_status`
- `balance_scope`
- `balance_collected_at`
- `economic_freshness`

规则：

- `balance_status = depleted` 默认拒绝，除非设置允许耗尽兜底。
- `balance_status = low` 强降权。
- `normalization_status = complete` 可用于 cheap-first 精确排序。
- `group_rate_only` 不用于精确 cheap-first，但在 route explain 中提示。
- 价格过期会降低置信度，不能静默当作新价格。
- group missing 会降权或拒绝，具体由路由策略控制。

Route explain 必须展示 capability、health、cooldown、group binding、pricing rule、balance penalty、fallback 和 rejected candidates。

## 11. Request Log 联动

请求日志需要记录经济决策上下文：

- 使用的 station 和 key。
- 使用的 group binding。
- 使用的 pricing rule。
- 价格状态：完整、仅倍率、手动、未知、过期。
- balance 当时状态。
- fallback 原因。
- rejected candidates 结构化原因。

请求日志页面要回答：某次请求为什么成功或失败，以及为什么选了这把 key。

## 12. Change Center 升级

变更中心覆盖：

- group 新增：info。
- group 消失：warning。
- key 自动绑定成功：info。
- key group 无法识别：warning。
- 手动绑定 group 不可见：warning。
- 倍率上涨：warning。
- 倍率下降：info。
- 完整价格变贵：warning。
- 价格过期：warning。
- 余额偏低：warning。
- 余额耗尽：critical。
- key 失效：critical。
- 采集失败：warning。
- 站点不可用：critical。
- 路由候选受到余额、group、价格影响：warning 或 critical。

事件需要具备：

- severity。
- status。
- object type 和 object id。
- station / key / group / pricing / request 关联。
- old value / new value。
- impact。
- dedupe key。
- source。
- detected / resolved 时间。

## 13. UI 职责

### 13.1 总览

回答“现在有什么风险？”

只展示摘要和风险：

- 本地代理状态。
- 严重 / 警告变更计数。
- 低余额 / 耗尽站点数。
- 待绑定 Key 数。
- 采集失败站点数。
- 价格过期 / 倍率上涨摘要。
- 最近重要变更。

### 13.2 中转站资产

回答“哪个站点资产状态好不好？”

主表摘要：

- 站点名称。
- 类型。
- base_url。
- 余额。
- group 摘要 chip，最多 3 个。
- Key 数量。
- 采集状态。
- 健康状态。
- 最近更新时间。
- 是否参与路由。
- 操作：采集 / 详情。

右侧抽屉：

- 余额详情。
- group bindings。
- 倍率历史。
- Key 列表。
- 采集历史。
- 相关变更。
- 路由影响。

### 13.3 Key 池

回答“哪把 key 能不能路由？”

展示：

- 绑定 group。
- 绑定状态。
- effective multiplier。
- balance scope。
- 当前是否可路由。
- 被拒绝原因。
- 成功率 / 冷却。
- 价格状态。

Key 详情中允许手动绑定 group。

### 13.4 路由规则

回答“为什么请求会走这把 key？”

路由模拟和解释展示：

- capability 过滤。
- health / cooldown。
- group binding。
- price rule。
- balance penalty。
- fallback 顺序。
- rejected candidates。

### 13.5 价格 / 倍率

回答“哪个站点更便宜？”

三层矩阵：

- 模型价格矩阵：模型 x 站点。
- 分组倍率矩阵：group x 站点。
- 可用性矩阵：模型 x 站点或 key。

Cell 展示：

- 当前价格或倍率。
- 来源。
- 更新时间。
- 状态：完整、仅倍率、手动、过期、不可用。
- 是否最低价。

### 13.6 渠道状态

回答“最近运行稳不稳？”

仍围绕 key/channel health：

- 成功率。
- 延迟。
- 连续失败。
- cooldown。
- 最近错误。
- 最近请求状态条。

Group 和 price 只作为辅助信息。

### 13.7 变更中心

回答“最近有什么需要处理的变化？”

升级为事件工作台：

- 严重程度 tabs。
- 状态筛选。
- 对象筛选。
- 事件详情抽屉。
- 跳转相关对象。
- 批量标记已读。
- 解决 / 忽略。

详情展示 old/new value、impact、source、检测时间和建议动作。

### 13.8 采集中心

开发者模式页面，回答“adapter 为什么这样采集？”

展示：

- Station 选择。
- 任务选择：detect、balance、groups、models、full。
- adapter 识别结果。
- session 状态。
- endpoint 结果。
- normalized facts。
- redacted raw。
- 最近 runs。
- 手动 token、userId、refresh token 配置入口。

### 13.9 设置

新增或完善：

- 开发者模式。
- 余额采集周期。
- group/rate 采集周期。
- model 采集周期。
- 价格过期阈值。
- 是否允许余额耗尽兜底。
- 数据安全扫描和明文迁移状态。

## 14. 调度模型

设置项建议：

- `balance_interval_minutes`，默认 5。
- `group_rate_interval_minutes`，默认 20。
- `model_list_interval_minutes`，默认 60。
- `pricing_refresh_interval_minutes`，默认 60 或手动。
- `collector_timeout_seconds`。
- `collector_max_concurrency`。

任务类型：

- `detect`
- `balance`
- `groups`
- `models`
- `full`

手动“采集”执行 `full`。后台调度拆分余额、group/rate、models，任一任务失败不阻塞其他任务。

## 15. 错误分类

错误类型：

- `network_error`
- `auth_failed`
- `manual_session_required`
- `permission_denied`
- `schema_unrecognized`
- `partial_success`
- `rate_limited`
- `server_error`
- `adapter_not_supported`

UI 和变更中心必须按错误类型给出不同提示，不能全部显示为同一种“采集失败”。

## 16. 安全规则

敏感内容：

- API key。
- login password。
- access token。
- refresh token。
- cookie。
- session。
- Authorization header。
- Set-Cookie header。
- NewAPI user token。
- 远端响应中疑似 token 的字段。

规则：

- token、password、cookie 全部进入 SecretManager。
- `collector_snapshots.raw_json_redacted` 只保存脱敏和截断版本。
- `collector_runs.error_message` 不保存完整 HTML 或完整响应。
- 采集中心只展示 redacted raw。
- 导入导出不包含 secret 明文，也不导出密文 payload。
- 安全扫描覆盖新增表。

## 17. 迁移策略

迁移必须幂等。

规则：

- 现有 `station_keys.group_name` 生成初始 `station_group_bindings`，状态为 `manual_legacy` 或 `bound`。
- 现有 `collector_snapshots.normalized_json.rateMultipliers` 可迁移为低置信度 `group_rate_records`，source 为 `legacy_snapshot`。
- 现有 `pricing_rules.group_name` 尝试关联 group binding，找不到时保留原字段。
- 现有 `station_credentials.login_status` 和 `session_status` 保留，新增 token secret ref 字段为空。
- 现有 list APIs 继续返回旧字段，新字段 optional 加入。
- 新 UI 优先消费新表，缺数据时 fallback 到旧字段或快照。

## 18. 实施阶段

### 阶段 1：事实模型和迁移

- 新增表和字段。
- Rust / TypeScript 类型同步。
- 数据迁移和幂等测试。
- 保证旧页面仍可打开。

### 阶段 2：Collector Adapter 框架

- 按 `station_type` 分发。
- 定义 facts。
- 新增 apply layer。
- 新增 collector run 记录。
- 统一错误分类和 redaction。

### 阶段 3：Sub2API 完整采集

- `/v1/usage`。
- login / refresh token。
- groups available。
- groups rates。
- key group resolve。
- manual session required 状态。

### 阶段 4：NewAPI 完整采集

- access token + user id 配置。
- `/api/user/self`。
- `/api/user/self/groups`。
- quota 换算。
- group/rate 字段兼容。

### 阶段 5：价格 / 倍率 / 路由联动

- group rate 到 pricing normalization。
- `group_rate_only` 状态。
- cheap-first 只使用可信完整价格。
- 余额耗尽拒绝或兜底策略。
- route explain 展示 group、rate、pricing、balance 来源。
- request log 记录经济上下文。

### 阶段 6：UI 全面接入新事实层

- 中转站资产抽屉。
- Key 池 group 绑定。
- 价格 / 倍率矩阵状态。
- 变更中心事件详情。
- 采集中心任务和 facts 调试。
- 设置采集周期和安全选项。

### 阶段 7：验证、清理和文档

- Rust tests。
- TypeScript build。
- Tauri smoke。
- 文档更新。
- 安全扫描。
- 清理明显过时的 legacy fallback。

## 19. 验收标准

P9 完成后应满足：

- 一个真实 Sub2API 站点能采集余额、group、倍率，并自动或手动绑定 key group。
- 一个真实 NewAPI 站点能通过 access token + user id 采集余额、group、倍率。
- OpenAI-compatible 站点能采集模型列表，并继续支持手动价格。
- Key 池能显示 group binding、倍率、余额作用域和可路由状态。
- 价格 / 倍率页面不再只是 pricing rule CRUD，而是跨站点矩阵。
- route explain 能解释 capability、health、group、price、balance 和 fallback。
- 请求日志能追溯使用的 pricing rule、group binding、balance 状态和 rejected candidates。
- 变更中心能展示并跳转余额、倍率、价格、模型、Key、采集和路由影响事件。
- 采集中心能调试 adapter、session、endpoint、facts 和 redacted raw。
- SQLite 明文扫描不发现完整 API key、password、token、cookie。

## 20. 高风险点

- 数据库迁移风险：新增表和字段多，必须幂等，不能破坏已有 key、价格和快照。
- 凭据安全风险：新增 token/session 后，任何 raw/log/snapshot 泄漏都是严重问题。
- 真实站点差异风险：Sub2API/NewAPI 部署魔改多，parser 要容错，但业务层不能堆特判。
- 路由误判风险：倍率不是价格，`group_rate_only` 不能参与精确 cheap-first。
- UI 信息过载风险：主列表只放摘要，完整信息进抽屉和详情。
- 实施周期风险：必须按事实层、adapter、联动、UI 的顺序推进，避免先做页面再返工底层。

## 21. 测试计划

Rust 单元测试：

- Sub2API usage 字段兼容。
- Sub2API token resolver 优先级。
- refresh token 成功后持久化新 token。
- access token 失败后只重试一次。
- groups/rates join 和倍率优先级。
- API key 反查 group id。
- NewAPI quota 换算。
- NewAPI groups 多种字段兼容。
- group binding upsert 幂等。
- group rate history 仅变化插入。
- pricing normalization status。
- change event dedupe。
- 新增表安全扫描。

前端 / 集成测试：

- 采集中心任务选择和 run 展示。
- 手动 token 保存后不回显。
- Key 绑定 group 后刷新不丢。
- group missing 状态提示。
- 价格 / 倍率矩阵状态显示。
- route explain 展示经济上下文。
- 变更中心筛选、详情、跳转。

验证命令：

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
```

