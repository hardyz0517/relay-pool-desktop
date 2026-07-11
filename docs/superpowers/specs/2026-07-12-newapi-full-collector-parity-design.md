# NewAPI 完整采集能力对齐设计

日期：2026-07-12

## 1. 背景

Relay Pool Desktop 已经存在 `newapi` station type、NewAPI 凭据字段和一版后端 adapter，但当前产品仍表现为“待接入”。现有实现存在以下关键缺口：

- 登录测试和自动密码登录仍硬编码走 Sub2API 路径。
- 余额解析没有解包 NewAPI 标准的 `{ success, data, message }` 响应。
- 分组接口的 `data` 是以分组名为 key 的 object map，现有解析会丢失 map key，导致分组名称退化和 hash 冲突。
- `Detect` 只返回固定成功结果，没有请求真实站点状态。
- `Models` 任务仍返回不支持。
- 远程 Key capability、分页扫描、完整密钥读取和创建仍未适配。
- 前端仍显示“NewAPI 采集（待接入）”。

已使用真实 NewAPI 测试站点做只读验证，确认当前接口契约包括：

- `GET /api/status` 返回公开站点配置，包括 `quota_per_unit`。
- `POST /api/user/login` 成功后返回用户 ID 并建立 Cookie 会话。
- `GET /api/user/self` 返回 `quota`、`used_quota` 和当前分组。
- `GET /api/user/self/groups` 返回 `data: { groupName: { desc, ratio } }`。
- `GET /api/user/models` 返回模型名称数组。
- `GET /api/token/` 返回分页且脱敏的远程 Key 列表。

上游源码核对基线为 `QuantumNous/new-api` commit `bde9b2f44887d34ec54799ae191d50f97914359e`。实施时应把该 commit 的路由、鉴权 middleware 和 controller fixture 作为可复现参考，同时重新检查上游 HEAD 是否发生契约变化；不得只依赖测试站点的单一部署版本。

真实账号凭据和响应中的敏感值不得写入本文档、fixture、日志或版本控制。

## 2. 目标

NewAPI adapter 应达到当前 Sub2API adapter 的完整站点管理能力，并补齐已确认可用的模型采集：

- 真实站点识别。
- access token、Cookie、账号密码三路登录态兼容。
- 余额采集。
- 分组和倍率采集。
- 模型采集。
- 远程 Key 分页扫描。
- 用户显式操作时读取一次完整远程 Key。
- 创建远程 Key 并同步为本地 `StationKey`。
- 自动采集、快照、Collector Run、变更事件和现有 UI 展示。

## 3. 非目标

- 不新增 NewAPI 专属数据库模型或前端页面。
- 不重构 Sub2API 的业务解析。
- 不增加远程 Key 编辑或删除的产品功能。
- 不自动生成或轮换 NewAPI access token。
- 不绕过 2FA、Turnstile 或验证码。
- 不把分组倍率伪装成完整模型价格。
- 不在列表扫描时批量读取完整远程 Key。
- 不直接复制 NewAPI 开源项目的 AGPL 业务实现；只依据公开 HTTP 契约和独立测试 fixture 实现本地 adapter。

真实验收允许创建一个明确命名的临时 Key，并在验证结束后删除。该删除仅属于测试清理，不扩展为产品能力。

## 4. 总体架构

采用独立增强 NewAPI adapter 的方案。保留 NewAPI 与 Sub2API 各自的上游协议语义，共用现有基础设施和归一化事实层。

将当前 `src-tauri/src/services/collectors/adapters/newapi.rs` 拆分为：

- `newapi/mod.rs`：`CollectorTask` 分发、`AdapterOutput` 组装和 capability 声明。
- `newapi/auth.rs`：凭据解析、密码登录、会话持久化和一次鉴权重试。
- `newapi/client.rs`：HTTP 请求、代理、超时、分页、响应解包和错误脱敏。
- `newapi/parsers.rs`：余额、分组、模型和远程 Key 响应归一化。
- `newapi/remote_keys.rs`：远程 Key 扫描、完整密钥读取和创建。

继续使用现有：

- `CollectorFacts`
- `BalanceSnapshot`
- `StationGroupBinding`
- `GroupRateRecord`
- `CollectorSnapshot`
- `CollectorRun`
- `RemoteStationKey`
- `StationKey`

预计不需要数据库迁移。实现前仍需以当前工作区 schema 为准做一次字段审计；如果现有字段无法表达已确认契约，应优先调整映射，而不是新增平行表。

现有模型事实没有独立数据库表，`apply_model_facts` 只保留统一事实接口。NewAPI 模型必须写入本次 snapshot 的 `normalized_json.models`，由现有 snapshot diff 和展示路径消费；规格中的“模型持久化”均指这一现有契约，不得误解为新增模型表。

会话存储需要增加内部、原子化的数据库 helper，用于保存或失效指定凭据及其 `session_source`。该 helper 复用现有列和 SecretManager，不新增 schema，也不得复用会把来源固定写成 `manual_token` 的公共手动会话更新路径。

## 5. 鉴权与会话

### 5.1 凭据解析顺序

1. 加密保存的 `access token + NewAPI user ID`。
2. 加密保存的 `Cookie + NewAPI user ID`。
3. 保存的登录账号和密码。
4. WebView 捕获或手动登录态。

现有通用 session resolver 尚未把 Cookie 单独视为 Ready。实现必须修正为：非空 Cookie 与 NewAPI user ID 同时存在时，可独立生成 Cookie auth context；不得因为缺少 refresh token 或保存密码而忽略有效 Cookie。

access token 请求发送：

- `Authorization: Bearer {accessToken}`
- `New-Api-User: {userId}`

Cookie 请求发送：

- `Cookie: {cookie}`
- `New-Api-User: {userId}`

### 5.2 自动密码登录

自动登录调用 `POST /api/user/login`，请求体只包含用户名和密码。成功后：

- 从响应 `data.id` 取得 NewAPI user ID。
- 收集全部 `Set-Cookie`，只提取 cookie name/value，剥离 Path、Domain、Expires、HttpOnly、Secure 和 SameSite 等属性，按 `name=value; ...` 组成请求 Cookie。
- 使用现有 SecretManager 加密保存 Cookie。
- 在同一数据库事务中更新 `newapi_user_id`、会话状态、来源和时间字段。
- 可解析到 cookie expiry 时写入 `session_expires_at`；session cookie 没有 expiry 时允许先使用，并在鉴权失败后重新登录。
- 不调用 `GET /api/user/token`，避免生成或轮换用户 access token。

### 5.3 失效与重试

以下情况视为鉴权失败：

- HTTP `401` 或 `403`。
- HTTP 200 但响应 `success:false`，且 message 表示未登录、token 无效或 user ID 不匹配。

发生鉴权失败时只失效本次使用的凭据模式：access token 失败不得顺带删除仍可能有效的 Cookie，Cookie 失败也不得覆盖手动 access token。随后从下一条可用凭据路径重新解析并最多重试一次。不得无限登录或无限重试。

遇到 2FA、Turnstile 或验证码时返回 `manual_session_required`，引导用户使用现有 WebView 或手动 token/Cookie 路径。

## 6. HTTP 客户端契约

NewAPI client 负责：

- 使用 station 和全局 collector proxy 配置。
- 统一 20 秒请求超时，除非项目现有常量已有更严格约定。
- 解包 `{ success, data, message }`。
- HTTP 200 且 `success:false` 时返回业务错误。
- 将 endpoint path、status、duration 和成功状态写入脱敏摘要。
- 对响应错误文本递归脱敏并截断。
- 不在 endpoint result 中保存 Authorization、Cookie、Set-Cookie 或完整响应体。

瞬时失败重试与 Sub2API 保持一致，但必须按幂等性分类：

- 网络错误。
- HTTP `408`。
- HTTP `429`。
- HTTP `5xx`。

- Detect、Balance、Groups、Models、token list 等 GET 请求最多做一次瞬时重试。
- `POST /api/token/:id/key` 是逻辑只读操作，可以在没有收到业务响应时重试一次。
- `POST /api/token/` 创建远程 Key 是非幂等操作，禁止自动重发。
- 创建请求结果不确定时必须进入创建后对账流程，不得把网络重试当作恢复手段。

鉴权重试与瞬时重试分别计数，但每个 endpoint 的最大实际请求次数必须写成常量并由测试锁定，避免组合成无界重试。

## 7. 采集任务

### 7.1 Detect

请求 `GET /api/status`：

- 根据响应结构确认 NewAPI。
- 读取 `quota_per_unit`、`quota_display_type`、系统名称和非敏感版本信息。
- 将 `quota_per_unit` 作为本轮余额换算参数。
- 不再返回固定“NewAPI adapter 已确认”的假成功结果。

### 7.2 Balance

Balance 每次先请求公开 `GET /api/status` 取得本轮 `quota_per_unit`，再请求 `GET /api/user/self`。不得依赖用户曾经手动执行 Detect，也不得长期使用旧 snapshot 中的换算参数。

- 先解包 `data`。
- `quota` 映射剩余额度。
- `used_quota` 映射已用额度。
- `total = quota + used_quota`。
- 使用 `/api/status.quota_per_unit` 换算展示额度。
- `quota_per_unit` 缺失时回退 `500000`，同时降低 confidence，并在摘要中标记 fallback。
- 当前 `group` 作为账户分组事实保留。
- 写入 station scope 的 `BalanceSnapshot`。

失败时保留上一条有效余额，不写入伪造的零余额。

### 7.3 Groups

请求 `GET /api/user/self/groups`：

- 对 `data` object map 逐项遍历。
- map key 同时作为上游 group identity 和默认 group name。
- value 中的 `desc` 作为描述信息，不覆盖稳定 identity。
- 数字 `ratio` 写入原始 default/user/effective multiplier。
- 字符串数值可安全解析时转为数字。
- `自动` 等非数字值保留为可见分组，倍率为 `None`。
- 使用 group identity 生成稳定 hash，避免多个分组退化为 `default`。
- 写入 `StationGroupBinding`、`GroupRateRecord` 和相应变更事件。

倍率在 collector 边界保持上游原始值，不提前做价格归一化。

### 7.4 Models

请求 `GET /api/user/models`：

- 支持字符串数组。
- 对未来对象数组兼容 `id`、`name` 和 `model` 等常见字段。
- 去重并过滤空模型名。
- 同时写入 `CollectorFacts.models` 和该 Models snapshot 的顶层 `normalized_json.models` 字符串数组；后者是现有持久化与 diff 契约。
- 第一版不对每个 group 重复调用 `?group=`，避免请求量随分组数膨胀。

### 7.5 Full

NewAPI Full 子任务为：

1. `Balance`
2. `Groups`
3. `Models`

各子任务独立生成 Collector Run。Groups 完成后沿用现有流程刷新远程 Key 元数据。

父任务状态继续使用现有聚合规则：

- 全部成功为 `success`。
- 部分成功为 `partial`。
- 全部需要人工操作为 `manual_required`。
- 其余为 `failed`。

## 8. 远程 Key 能力

### 8.1 Capability

NewAPI capability 调整为：

- `can_list_remote_keys = true`
- `can_create_remote_key = true`
- `can_read_groups = true`
- `requires_manual_session = true`
- `unsupported_reason = None`

这里的 `requires_manual_session` 表示远程管理需要有效登录态，不表示必须手工输入 token；保存的账号密码也可自动建立登录态。

### 8.2 分页扫描

请求 `GET /api/token/?p={page}&size=100`：

- 按 `page`、`page_size`、`total` 和 `items` 完整翻页。
- 设置明确总量上限，防止异常站点导致无限分页或内存增长。
- 只有所有分页成功且 item 数量与响应 total 一致时，才允许调用现有 `replace_remote_station_keys`。
- 分页中途失败、total 漂移、重复页或达到安全上限时，整个扫描返回失败并保留数据库中的上一版完整远程 Key 集合。现有扫描返回类型无法安全表达部分集合，禁止把已读取页作为全量结果落库。
- 列表中的 masked key 只能用于展示和弱匹配。

映射到 `RemoteStationKey`：

- remote token ID -> 稳定远程 ID/hash。
- `name` -> `remote_key_name`。
- masked `key` -> `api_key_masked`。
- `group` -> group name/hash。
- `created_time` -> `created_at`。
- `accessed_time` -> `last_used_at`。
- 额度、状态、模型限制等进入脱敏 raw source 或现有可表达字段。

### 8.3 显式读取完整密钥

只有用户执行“保存为本地 Key”等明确操作时，调用：

`POST /api/token/:id/key`

完整 Key：

- 只在 Rust 后端短暂存在。
- 直接经现有加密路径保存为本地 `StationKey`。
- 现有 `fullKeyOnce` 返回字段为兼容保留，但完成加密保存后必须返回 `null`；不得把完整 Key 带回 React/Tauri IPC 调用方。
- 不写入日志、快照、错误文本或测试输出。

### 8.4 创建远程 Key

调用 `POST /api/token/`，基于现有 `CreateRemoteStationKeyInput` 映射：

- `name`：用户输入名称。
- `group`：所选 group binding 对应的上游 group name。
- `expired_time = -1`。
- `unlimited_quota = true`。
- `remain_quota = 0`。
- `model_limits_enabled = false`。
- `model_limits = ""`。
- `allow_ips = ""`。
- `cross_group_retry = false`。

NewAPI 创建接口只返回成功状态，不直接返回 token ID 或完整 Key，因此采用以下确定性流程：

1. 创建前读取远程 token ID 集合。
2. 发送创建请求。
3. 创建后重新分页扫描。
4. 使用“新增 ID + 名称 + 分组”唯一定位新 token。
5. 调用完整密钥接口一次。
6. 创建本地 `StationKey`。

如果无法唯一定位，必须报告“远程已创建，本地同步失败”，不得猜测 token，也不得导入错误密钥。生产流程不自动删除用户刚创建的远程资产。

如果创建 POST 超时、断连或返回无法解析的响应，状态必须记为 `create_outcome_unknown`，随后只做一次只读对账扫描：

- 恰好发现一个符合“新增 ID + 名称 + 分组”的 token 时，视为远程创建已成功并继续同步。
- 没有发现时报告结果不确定，由用户决定是否重试。
- 发现多个候选时报告歧义并停止，不读取任何候选的完整 Key。

该状态机和默认创建字段应封装为 NewAPI 自己的 policy/helper，避免散落在 UI、client 和 remote-key service 中，便于未来增加额度、过期时间、模型限制或 IP 限制时扩展。

## 9. 错误和状态语义

- HTTP 成功不等于业务成功，必须检查 `success`。
- 成功响应但关键事实为空时不得标记完整成功。
- 单个子任务失败不得清除其他成功子任务或历史有效事实。
- 业务子任务可以 `partial`，但远程 Key replacement 必须全量成功或完全不写；不得持久化部分分页集合。
- 鉴权需要人工操作时使用 `manual_required` 和稳定错误码。
- 错误码至少区分：缺少 user ID、缺少登录态、登录失败、人工会话、鉴权失败、瞬时上游失败、解析失败、分页不完整、创建结果不确定、创建后定位失败、完整密钥读取失败。
- 错误文本只保留脱敏且截断后的上游 message。

## 10. 前端设计

- 移除“NewAPI 采集（待接入）”文案。
- 登录测试按 station type 分发，不再统一走 Sub2API。
- 保留现有 access token、Cookie、NewAPI user ID 和账号密码输入。
- NewAPI 密码登录成功后只展示登录态存在、来源和时间，不回显 Cookie。
- Collector 页面分别展示余额、分组和模型的 Collector Run，并展示远程 Key 刷新事件与现有远程发现列表；不伪造一个当前类型系统中不存在的 remote-key CollectorTask。
- 远程 Key capability 显示为支持读取和创建。
- 继续复用现有远程 Key -> 本地 Key 和创建远程 Key 流程。
- NewAPI 创建确认框明确说明当前兼容策略是永久、无限额度、无模型/IP 限制，避免在没有对应输入字段时静默创建高权限 Key。
- 不新增 NewAPI 专属页面或重复卡片体系。

## 11. 安全边界

- 密码、access token、Cookie 和完整远程 Key 必须经 SecretManager 加密存储。
- Cookie 存储值只能包含 cookie name/value 对，不得把 Set-Cookie 属性或响应头整体持久化。
- fixture 只能使用人工构造或完全脱敏的响应。
- HTTP 捕获测试不得打印 Authorization、Cookie、Set-Cookie 或完整 Key。
- `raw_json_redacted` 必须经过递归脱敏。
- 前端只消费 present/status/masked 字段。
- `CreateRemoteStationKeyResult.fullKeyOnce` 兼容字段必须为 `null`，并用共享服务回归测试确保 Sub2API/NewAPI 都不会通过 IPC 回传完整 Key。
- 自动登录不绕过验证码、2FA 或 Turnstile。
- 真实测试账号凭据不得写入 shell 脚本、文档、测试源码或 git 历史。

## 12. 测试策略

### 12.1 Rust 单元测试

- 标准 response envelope 解包。
- `quota_per_unit` 动态换算与 fallback。
- 分组 object map 保留 map key。
- 数字、数字字符串和非数字倍率。
- 模型字符串数组和对象数组。
- token 分页与上限。
- remote token 到 `RemoteStationKey` 的字段映射。
- 三路凭据优先级。
- Cookie 和 access token 请求头。
- Cookie-only session 被解析为 Ready，且失效处理不会删除其他凭据模式。
- 401/403 单次鉴权重试。
- 2FA/Turnstile 转人工会话。
- 创建后唯一定位新 token。
- 非幂等创建请求不自动重发，超时后只做对账扫描。
- 完整 Key 不进入日志或 snapshot。
- Models snapshot 的顶层 `normalized_json.models` 保持现有 diff 契约。

### 12.2 HTTP 捕获与数据库集成测试

- 验证 login、self、groups、models、token list、token key 和 create 的 method/path/body/header。
- 验证瞬时重试与鉴权重试预算。
- 验证分页中途失败不会调用 `replace_remote_station_keys`，旧的完整扫描结果保持不变。
- 验证余额、binding、倍率历史、模型事实和 Collector Run 写入。
- 验证远程 Key 匹配和本地 Key 加密保存。

### 12.3 前端测试

- NewAPI 不再显示“待接入”。
- 登录表单和状态支持三路凭据。
- capability 支持扫描和创建。
- 子任务结果和错误语义正确展示。
- 敏感字段不回显。

### 12.4 真实站点验收

只读阶段验证：

- 登录。
- 余额。
- 可用分组和倍率。
- 模型列表。
- 远程 Key 分页和脱敏列表。

受控写入阶段：

1. 创建唯一命名的临时远程 Key。
2. 重新扫描并唯一定位。
3. 显式读取完整 Key。
4. 同步为本地 `StationKey`。
5. 验证本地只保存密文和脱敏状态。
6. 在 `finally` 清理远程临时 Key。

如果删除失败，必须停止后续操作并报告残留远程 token ID，不得声称真实验收完成。

真实 E2E 必须是默认跳过的显式 opt-in 测试，只从运行时环境变量读取 base URL、用户名和密码，并要求单独的写入开关。测试使用隔离的临时数据库和数据目录，不向用户日常 Relay Pool 数据库写入测试 Station、凭据或本地 Key。测试名称使用不可冲突的随机后缀，只删除本次记录的远程 token ID；临时本地数据在远程清理结果确认后删除，任何输出都不得包含凭据、Cookie 或完整 Key。

## 13. 验证命令

至少运行：

```powershell
pnpm.cmd build
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
```

同时运行本功能新增的聚焦 Node/Rust 测试。任何因环境或既有工作区改动无法运行的检查都必须报告实际原因。

## 14. 交付顺序

1. 建立脱敏 fixture，修复 response envelope 和 parser。
2. 实现三路鉴权和真实 Detect/Balance/Groups/Models。
3. 实现远程 Key capability、分页扫描和完整密钥读取。
4. 实现创建远程 Key 并同步本地 Key。
5. 接入 Full、自动采集和现有 UI。
6. 完成单元、捕获、数据库和前端测试。
7. 最后执行一次受控真实站点验收和清理。

## 15. 验收标准

- 同一 NewAPI Station 可通过 access token、Cookie 或保存账号密码完成采集。
- 余额、分组和倍率进入现有事实表；模型进入 `CollectorFacts` 和 snapshot `normalized_json.models`；远程 Key 进入现有远程发现表。
- Full 子任务状态、失败隔离和历史保留符合现有 Collector 语义。
- 远程 Key 能分页扫描、显式读取完整密钥、创建并同步为本地 Key。
- 分页扫描失败不会用部分结果覆盖上一版完整远程 Key 集合，创建请求结果不确定时不会重复创建。
- 临时远程 Key 能在真实验收后成功清理。
- UI 不再显示“待接入”，且不新增重复的 NewAPI 专属管理面。
- 日志、快照、fixture、前端和 git 历史中不存在密码、token、Cookie 或完整 Key。
- Tauri IPC 返回中的 `fullKeyOnce` 始终为 `null`。
- Sub2API 既有采集和远程 Key 流程无回归。
