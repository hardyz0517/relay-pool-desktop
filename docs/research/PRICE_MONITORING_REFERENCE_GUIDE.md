# price-monitoring 参考技术指导

参考仓库：[`xianyvbang/price-monitoring`](https://github.com/xianyvbang/price-monitoring)  
阅读版本：`6ce3bd81df7794a1b198ab4b0534c1a9af0da2e4`  
适用项目：Relay Pool Desktop，本地 Tauri 桌面工具

这份文档只抽取可借鉴的技术经验，不建议照搬对方的 FastAPI + Vue Web 管理台形态。我们仍然保持 `Station`、`Station Key`、`Pricing Rule`、`Balance Snapshot`、`Request Cost` 的本地工具模型。

## 1. 参考项目值得借鉴的核心点

`price-monitoring` 的价值不在 UI，而在它把 sub2Api / newApi 的查询路径跑通了：

- sub2Api 余额：用 `apiKey` 调 `GET {baseUrl}/v1/usage`。
- sub2Api 查组/倍率：用登录 JWT 调 `GET {baseUrl}/api/v1/groups/available` 和 `GET {baseUrl}/api/v1/groups/rates`。
- sub2Api 当前 key 分组识别：优先用已配置 group id，其次用 `/v1/usage` 里的 plan/group 名称，必要时再调 `/api/v1/keys?page=1&page_size=100` 按 API key 反查 group id。
- sub2Api Turnstile 场景：不绕过验证，允许用户粘贴网页登录后的 `accessToken/auth_token` 和 `refreshToken`。
- newApi 余额：用 `Authorization: Bearer {accessToken}` 和 `New-Api-User: {userId}` 调 `GET {baseUrl}/api/user/self`。
- newApi 分组：调 `GET {baseUrl}/api/user/self/groups`，并兼容多种响应字段。
- 调度：余额刷新和分组倍率刷新分离，参考项目默认余额 5 分钟、分组倍率 20 分钟。
- 安全：密钥加密入库，日志和响应统一脱敏，HTTP 请求日志隐藏 `authorization/cookie/set-cookie`。

## 2. 映射到 Relay Pool 的对象模型

### Station

`Station` 仍然表示一个中转站网站加一个登录账号。参考项目里的 `accounts` 表可映射为 Station 的采集侧能力，但不要把它变成路由对象。

建议 Station 负责：

- `base_url`
- `station_type`，例如 `sub2api` / `newapi` / `openai_compatible`
- 登录账号字段的存在状态，而不是明文
- session/token 的存在状态、过期时间和最近刷新时间
- 最新余额快照摘要
- 最新分组倍率采集摘要
- 最近采集错误

### Station Key

`Station Key` 是真正参与代理、路由、fallback 和健康检测的对象。参考项目把分组监控挂在账号下；我们要进一步把分组与 Key 关联起来。

建议 Station Key 新增或派生这些字段：

- `group_id`
- `group_name`
- `rate_multiplier`
- `rate_source`
- `rate_collected_at`
- `balance_scope`，区分站点余额还是 key 余额
- `pricing_rule_id`

当前 `group_name / tier_label` 只能做展示，不能作为长期路由成本依据。

### Pricing Rule

分组倍率不是最终价格，但它是价格归一化的重要输入。建议把采集结果落到 `Pricing Rule` 或其上游原始记录：

- `station_id`
- `station_key_id`，可空
- `group_id`
- `model`
- `input_price`
- `output_price`
- `rate_multiplier`
- `currency`
- `unit`
- `source`
- `confidence`
- `collected_at`

如果某站只拿到 group rate，暂时没有模型基础价，应记录为 `source = group_rate_only`，不要伪造成完整模型价格。

### Balance Snapshot

参考项目把余额记录作为历史序列保存，并用剩余额度下降推算消耗。我们可以借鉴历史快照，但要保持单位明确：

- `scope = station | station_key`
- `value_raw`
- `unit_raw`
- `value_cny`
- `credit_per_cny`
- `source`
- `collected_at`

不建议在采集器里直接做复杂财务统计。采集器只负责事实，路由层只消费归一化结果。

## 3. sub2Api 采集流程建议

### 3.1 余额查询

请求：

```txt
GET {baseUrl}/v1/usage
Authorization: Bearer {apiKey}
```

字段兼容：

- `remaining`
- `quota.remaining`
- `balance`
- `unit` 或 `quota.unit`
- `planName` / `plan_name`
- `total`
- `used`
- `is_active` / `isValid`

落库建议：

- 生成 `Balance Snapshot`。
- 同步更新 Station 的摘要字段，便于 UI 快速展示。
- 如果接口失败，只更新采集状态和错误，不清空上一条可用余额。

### 3.2 登录 token 解析

参考项目的优先级值得照搬为策略，但实现要放入 Rust collector/session 层：

1. 如果用户保存了 `accessToken`，先用它。
2. 如果用户保存了 `refreshToken`，先刷新 `accessToken`。
3. 如果进程内有未过期 token 缓存，复用。
4. 如果有 email/password，调用登录接口。
5. 如果接口返回 401/403 或分组请求失败，清理对应缓存，刷新或重新登录后只重试一次。

登录请求：

```txt
POST {baseUrl}/api/v1/auth/login
```

刷新请求：

```txt
POST {baseUrl}/api/v1/auth/refresh
Content-Type: application/json

{ "refresh_token": "..." }
```

token 缓存建议：

- cache key: `station_id` 或 `base_url + login_username_hash`
- 保存 `access_token`、`refresh_token`、`expires_at`
- 从响应里的 `expires_at/expiresIn/exp` 解析过期时间
- 如果响应没有过期时间，尝试解析 JWT `exp`
- 仍然没有时，用短 TTL，例如 50 分钟，并预留 60 秒刷新窗口
- 持久化 token 必须走 SecretManager，不进入普通日志或快照

### 3.3 Turnstile / 2FA 边界

参考项目没有尝试绕过 Turnstile，这是正确边界。

Relay Pool 应采用这个产品策略：

- email/password 自动登录失败时，如果错误疑似 Turnstile、2FA 或验证码，给出明确状态：`manual_session_required`。
- UI 提供“粘贴网页登录 token / refresh token”或 WebView 捕获登录态的入口。
- 自动采集器不做 CAPTCHA 绕过，不内置模拟浏览器批量撞登录。
- `accessToken/auth_token` 和 `refreshToken` 视为高敏感凭据，保存前加密，展示时只显示存在状态。

### 3.4 分组和倍率查询

请求：

```txt
GET {baseUrl}/api/v1/groups/available
Authorization: Bearer {accessToken}

GET {baseUrl}/api/v1/groups/rates
Authorization: Bearer {accessToken}
```

归一化建议：

- `groups/available` 提供 group 列表、名称、默认倍率等。
- `groups/rates` 通常可视为当前用户在各 group 上的用户倍率映射。
- `effective_rate_multiplier = user_rate_multiplier ?? default_rate_multiplier`。
- 同时保留 `default_rate_multiplier` 和 `user_rate_multiplier`，不要只存一个最终值。

当前 key 分组识别顺序：

1. 用户已经在 Station Key 上选择了 `group_id`，直接匹配。
2. 使用 `/v1/usage` 中的 `planName/plan_name/group` 按 group 名称匹配。
3. 如果只有一个可用 group，可作为低置信度默认匹配。
4. 仍无法识别时，调用 `/api/v1/keys?page=1&page_size=100`，用完整 key 或脱敏 key 的可见前后缀匹配。
5. 仍无法识别时，保存全部可用 group，UI 提示用户手动绑定。

注意：第 4 步可能要求站点登录态权限足够，失败不应使整个采集失败。

## 4. newApi 采集流程建议

### 4.1 余额查询

请求：

```txt
GET {baseUrl}/api/user/self
Authorization: Bearer {accessToken}
New-Api-User: {userId}
Content-Type: application/json
```

参考项目把 NewAPI 的 `quota` 和 `used_quota` 除以 `500000` 转成美元额度。我们可以借鉴，但必须作为适配器规则写清楚来源，不要把它做成所有站点的默认规则。

落库建议：

- `remaining = quota / 500000`
- `used = used_quota / 500000`
- `total = (quota + used_quota) / 500000`
- `plan_name = group`
- `unit = USD`
- `source = newapi_user_self`

### 4.2 分组查询

请求：

```txt
GET {baseUrl}/api/user/self/groups
Authorization: Bearer {accessToken}
New-Api-User: {userId}
```

字段兼容：

- `user_group`
- `userGroup`
- `groups`
- `items`
- `list`
- 或对象 map 形式

倍率字段兼容：

- `default_rate_multiplier`
- `rate`
- `ratio`
- `rate_multiplier`
- `rateMultiplier`
- `user_rate_multiplier`
- `effective_rate_multiplier`

建议 newApi adapter 和 sub2Api adapter 共享同一个 `GroupRateSnapshot` 归一化结构。

## 5. 数据库设计建议

参考项目的 `account_monitor_groups` 和 `group_rate_records` 很值得借鉴，但应改成 Relay Pool 命名。

建议新增或确认这些表：

### `station_group_bindings`

用途：记录用户希望监控或绑定到 key 的分组。

关键字段：

- `id`
- `station_id`
- `station_key_id`，可空
- `group_id_enc`
- `group_id_hash`
- `group_name`
- `default_rate_multiplier`
- `user_rate_multiplier`
- `effective_rate_multiplier`
- `raw_json_redacted`
- `last_checked_at`
- `last_rate_changed`
- `sort_order`
- `created_at`
- `updated_at`

`group_id` 可能也算敏感业务信息，参考项目选择加密 group id 并存 hash 做唯一索引。我们可以按 P8 规则决定是否加密，但至少要有 hash 用于去重。

### `group_rate_records`

用途：保存倍率变化历史，给 UI 和路由解释使用。

关键字段：

- `id`
- `station_id`
- `station_key_id`，可空
- `group_binding_id`，可空
- `plan_name`
- `rate_multiplier`
- `raw_json_redacted`
- `checked_at`

插入规则：

- 没有上一条记录时插入。
- 倍率变化时插入。
- plan/group 名称变化时插入。
- 倍率不变时只更新 binding 快照，不刷历史表。

### token/session 存储

不要把 access token、refresh token、cookie、session 存进普通 `collector_snapshots.raw_json_redacted` 之外的明文字段。建议：

- `station_credentials` 只保存密文和 masked/present 状态。
- token 刷新后可更新密文，但命令返回只带 `hasAccessToken/hasRefreshToken`。
- collector 事件里只输出“已刷新 token”“登录态过期”等状态，不输出 token 内容。

## 6. 调度和任务模型

参考项目使用两个独立循环：

- 余额查询循环：默认 5 分钟。
- 分组倍率查询循环：默认 20 分钟。

Relay Pool 应延续这个思路：

- 余额刷新影响 `Balance Snapshot`、低余额抑制和 UI 状态。
- 倍率刷新影响 `Pricing Rule`、`cheap_first` 候选排序和路由解释。
- 这两个任务失败互不阻塞。
- 手动“立即刷新”可以串行执行余额 + 分组倍率，但后台循环仍应分离。

自动查组的跳过条件：

- sub2Api 缺少 `apiKey`。
- sub2Api 同时缺少 `refreshToken/accessToken` 和 `email/password`。
- newApi 缺少 `accessToken/userId`。
- newApi 没有用户选择或绑定的 group，除非当前操作是“加载可选分组”。

## 7. UI 交互建议

Relay Pool 是桌面工具，UI 应比参考项目更轻、更集成。

建议在 Station 详情或采集页提供：

- `测试余额`
- `刷新分组`
- `选择/绑定分组`
- `粘贴登录 token`
- `网页登录捕获`，作为高级 fallback

分组选择器建议：

- 支持多选 group。
- 保存时只提交仍在可选列表里的 group。
- 返回 `added_group_ids` 和 `removed_group_ids`，便于后端精确更新。
- 每个 group 显示名称、ID、倍率。
- 如果已绑定 group 不在最新可选列表里，UI 要明确提示“已移除或不可见”，不要静默继续当作有效 group。

Key Pool 展示建议：

- 每个 Key 展示当前绑定 group 与倍率。
- 倍率变化显示轻量状态，不要打断路由。
- 低余额、登录态过期、无法识别 group 分别展示，避免混成一个“异常”。

## 8. 安全与日志规则

参考项目有两条经验很重要：

- HTTP 请求/响应日志可用于调试采集器，但必须在写入前脱敏。
- 脱敏不是 UI 层职责，必须在采集器或日志服务边界完成。

Relay Pool 的最低规则：

- 敏感 header：`authorization`、`cookie`、`set-cookie`。
- 敏感字段：`api_key`、`apikey`、`password`、`access_token`、`refresh_token`、`token`、`secret`、`session`、`auth_token`。
- 原始响应最多保留 redacted + truncated 版本。
- 错误消息中截断远端响应，避免把 HTML、token、cookie 全量写进日志。
- 前端 developer JSON 最多展示截断后的脱敏快照。

## 9. 不应照搬的部分

- 不照搬 Web 登录账号系统。Relay Pool 是本地桌面工具，不需要管理员 Cookie 登录。
- 不照搬邮件告警作为核心能力。后续可以有桌面通知，但不是当前主线。
- 不照搬 OpenCode Go / CPA 导入逻辑，除非未来明确扩展该场景。
- 不把账号名称自动改成 `name-rate`。这对监控表格方便，但对我们的 Station / Key Pool 会制造名称漂移。
- 不把分组倍率变化直接等同于最终模型价格。倍率只是归一化价格的一个输入。

## 10. 分阶段落地建议

### 阶段 A：补齐真实 sub2Api/newApi 采集

- 在 Rust collector 中实现 sub2Api `/v1/usage`。
- 实现 sub2Api token resolver：configured access token、refresh token、缓存、email/password 登录。
- 实现 sub2Api group available/rates 拉取和归一化。
- 实现 newApi `/api/user/self` 和 `/api/user/self/groups`。
- 写入 `collector_snapshots`、`balance_snapshots`、group rate 相关记录。

验收：

- 一个真实 sub2Api 站点能拿到余额、当前分组、倍率。
- 一个 Turnstile 站点能通过手动 token 路径查组。
- 一个 newApi 站点能拿到余额和可选分组。

### 阶段 B：建立 group binding 与 Station Key 关系

- UI 允许把一个或多个 group 绑定到 Station 或 Station Key。
- 后端保存 `station_group_bindings`。
- Key Pool 显示 key 当前 group/rate。
- 无法自动识别时，采集结果提供可选 group 给用户绑定。

验收：

- 手动绑定不会被下一次刷新误删。
- 过期 group 会提示不可见。
- route explain 可以看到候选 key 的 group/rate 来源。

### 阶段 C：接入 P7 价格和路由

- group rate 变更后刷新或标记相关 `Pricing Rule`。
- `cheap_first` 使用归一化价格，不直接使用原始倍率。
- 低余额抑制逻辑消费 `Balance Snapshot`。
- 请求日志记录估算成本和使用的 pricing rule。

验收：

- Routing 页能解释为什么某 key 更便宜。
- 低余额 key 不会被 cheap_first 错误优先。
- 请求日志有 token/cost 元数据，但无 prompt/response/full key。

### 阶段 D：P8 安全复核

- 所有 token、密码、API key 进入 SecretManager。
- 明文迁移完成。
- collector snapshot、request log、route explain 全部走统一脱敏。
- 导入导出不包含原始 secret 或密文 secret payload。

验收：

- SQLite 原始内容不含完整 key/password/token/cookie。
- 日志里没有 Authorization/Cookie。
- 前端不会显示 refresh token 或完整 access token。

## 11. 推荐测试清单

Rust 单元测试：

- sub2Api `/v1/usage` 字段兼容。
- sub2Api token resolver 优先级。
- refresh token 成功后持久化新 token。
- access token 失败后只重试一次。
- groups/rates 归一化倍率优先级。
- API key 反查 group id 的完整 key 和脱敏 key 匹配。
- newApi quota 转换规则。
- 敏感字段递归脱敏。

前端/集成测试：

- 分组选择器过滤不存在的 group。
- 保存多 group 时返回 added/removed。
- dashboard/key pool 展示 group rate。
- 倍率变化状态可清除。
- token 粘贴保存后不回显。

验证命令建议：

```powershell
pnpm.cmd build
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
```

## 12. 参考源码入口

- README：`https://github.com/xianyvbang/price-monitoring/blob/6ce3bd81df7794a1b198ab4b0534c1a9af0da2e4/README.md`
- 余额、登录、分组、倍率：`app/services/balance.py`
- 调度器：`app/services/scheduler.py`
- SQLite 模型与 group rate 历史：`app/models.py`
- 加密：`app/security.py`
- 分组选择 UI：`frontend/src/components/GroupPickerDialog.vue`
- 前端 API：`frontend/src/api.js`
