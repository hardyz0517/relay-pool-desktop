# Relay Pool 字段归属清单

日期：2026-07-07

## 用途

这份清单是数据架构升级期间的字段登记簿。任何新增字段、漂移字段、相似字段、兼容字段或准备降级的字段，都必须先登记，再进入实现或合并讨论。

后续上下文压缩后，执行者必须先读：

1. `docs/superpowers/specs/2026-07-07-relay-pool-data-architecture-master-spec.md`
2. `docs/superpowers/plans/2026-07-07-relay-pool-data-architecture-master-plan.md`
3. 本文件
4. 当前 stage 计划和最新 handoff

## 字段分类

- `canonical fact`：权威事实，代表当前配置、身份或关系。
- `evidence/history`：采集、请求、健康检查或变更检测留下的证据。
- `current projection`：由事实和证据生成的当前可消费状态。
- `compatibility cache`：旧数据库、旧页面、preview fallback 或迁移期间需要保留的缓存。
- `unknown pending audit`：新出现但语义未审清的字段。不得删除、合并、重命名或作为新业务判断来源。
- `removable candidate`：所有消费者迁移完成、测试覆盖、回滚说明齐备后才允许进入删除评估。

## 登记模板

新增字段或漂移字段按这个表格追加一行：

| 字段 | 所在表/类型 | 分类 | 写入者 | 读取者 | fallback/来源顺序 | 保护测试 | 状态 | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `example_field` | `example_table` | `unknown pending audit` | 未知 | 未知 | 未定 | 未覆盖 | 待审计 | 新字段先登记，不直接合并语义 |

状态只能使用：

- `active`
- `compatibility`
- `pending audit`
- `migrating`
- `deprecated`
- `removable candidate`

## 当前核心字段

| 字段 | 所在表/类型 | 分类 | 写入者 | 读取者 | fallback/来源顺序 | 保护测试 | 状态 | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `station_group_bindings.id` | `station_group_bindings` | `canonical fact` | Rust database/shared capability/collector service | Key 编辑、分组选项、价格、站点详情、runtime route | 首选本地 durable identity | Stage 0 pricing/key/shared capabilities tests | `active` | 不能被上游 group id 替代 |
| `station_keys.group_binding_id` | `station_keys` | `canonical fact` | Key 保存 workflow | Key Pool、Edit Key、runtime route | 指向本地 binding row | Stage 0 key binding preservation tests | `active` | 编辑无关字段时必须保留 |
| `group_key_hash` | `station_group_bindings`, `group_rate_records` | `canonical fact` | collector/database projection | 分组去重、binding/rate 关联 | identity fallback 第 2 位 | Stage 0 pricing identity tests | `active` | 本地稳定 identity，不能等价于 `group_id_hash` |
| `group_id_hash` | `station_group_bindings`, `station_keys` | `compatibility cache` | collector/shared capability/remote key workflow | 远端 Key 创建、旧 UI、兼容路径 | identity fallback 第 3 位 | Stage 0 shared capabilities tests | `compatibility` | 脱敏上游 group id，不是本地 binding id |
| `group_name` | `station_group_bindings`, `group_rate_records`, `station_keys` | `compatibility cache` | collector/manual/legacy workflow | UI 展示、legacy fallback | identity fallback 最后一位 | Stage 0 field ownership scan | `compatibility` | 仅展示和 legacy fallback，不作为首选 identity |
| `station_keys.rate_multiplier` | `station_keys` | `compatibility cache` | legacy key workflow | 旧展示/preview fallback | 不参与权威倍率决策 | Stage 0 field ownership scan | `compatibility` | 不能作为当前倍率来源 |
| `station_group_bindings.effective_rate_multiplier` | `station_group_bindings` | `current projection` | collector/database service | 价格、详情、runtime route | 倍率 fallback 第 2 位 | Stage 0 pricing tests | `active` | 页面不得随意写 |
| `group_rate_records.effective_rate_multiplier` | `group_rate_records` | `evidence/history` | collector service | projection/旧消费者 | 倍率 fallback 第 4 位 | Stage 0 pricing tests | `active` | 不能复活 missing/disabled binding |
| `balance_snapshots.scope` | `balance_snapshots` | `evidence/history` | balance collector/API response | dashboard、station assets、projection | station-scope 优先 | Stage 0 balance summary tests | `active` | station-key 快照不能覆盖 station-scope 当前余额 |
| `stations.balance_raw` | `stations` | `compatibility cache` | legacy station balance update | 旧展示/fallback | 无 station-scope snapshot 时 fallback | Stage 0 field ownership scan | `compatibility` | 不作为首选余额来源 |
| `stations.balance_cny` | `stations` | `compatibility cache` | legacy station balance update | 旧展示/fallback | 无 station-scope snapshot 时 fallback | Stage 0 field ownership scan | `compatibility` | 不作为首选余额来源 |
| `stations.last_pricing_fetched_at` | `stations` | `compatibility cache` | collector summary update | 旧展示/粗粒度诊断 | 不作为分组新鲜度唯一依据 | Stage 0 field ownership scan | `compatibility` | 不能替代 group rate checked time |

## 新字段接入规则

当主工作区或合并过程出现新字段：

1. 先追加到本文件，分类为 `unknown pending audit`。
2. 记录写入者和读取者；不知道就写 `未知`。
3. 如果字段影响价格、余额、Key 分组、remote key、runtime route，当前 stage 停止扩大修改。
4. 补 focused test 或 source guard 后，才能把分类从 `unknown pending audit` 改为其他类型。
5. 只有状态不是 `pending audit` 的字段，才能进入删除、合并、重命名讨论。

## 未提交主线改动

主 checkout 的 dirty files 不是工作树可 merge 的事实。遇到未提交主线改动时只能选择一种处理：

- 等主 checkout 提交后，工作树再 merge 对应提交。
- 由用户明确要求，把特定文件生成 patch 并应用到工作树。
- 记录 dirty paths 和风险，明确本 stage 未接入这些变化。

不能仅凭 `git merge master` 成功就声称已经接入主 checkout 的未提交 bugfix。

## Stage 8 兼容字段复查结论

本轮无 removable candidate 字段。Stage 0-7 已将主要页面和 runtime snapshot 迁移到 query services 与 current projections，但旧数据库、preview fallback、远端 Key 兼容路径、Rust models、collector/database 写入路径仍需要这些字段作为兼容缓存或证据搬运字段。

| 字段 | 当前结论 | 删除评估 |
| --- | --- | --- |
| `station_keys.group_name` | `compatibility cache`，继续用于展示、legacy fallback、远端 Key 兼容路径和旧记录读写 | 不批准删除；不能替代 `group_binding_id` |
| `station_keys.group_id_hash` | `compatibility cache`，继续用于远端 identity metadata、remote key workflow 和 legacy fallback | 不批准删除；不能替代本地 binding id |
| `station_keys.rate_multiplier` | `compatibility cache`，继续用于旧数据库、preview fallback 和未迁移诊断展示 | 不批准删除；current rate 由 projection 提供 |
| `station_keys.rate_source` | `compatibility cache`，继续用于旧数据库、preview fallback 和诊断来源展示 | 不批准删除；current source 由 projection 提供 |
| `station_keys.rate_collected_at` | `compatibility cache`，继续用于旧数据库、preview fallback 和粗粒度诊断 | 不批准删除；不能作为 current group freshness 唯一依据 |
| `stations.balance_raw` | `compatibility cache`，无 station-scope balance snapshot 时继续作为余额 fallback | 不批准删除；station-scope snapshot 优先 |
| `stations.balance_cny` | `compatibility cache`，无 station-scope balance snapshot 时继续作为余额 fallback | 不批准删除；station-scope snapshot 优先 |
| `stations.last_pricing_fetched_at` | `compatibility cache`，继续作为粗粒度采集诊断和旧页面 fallback | 不批准删除；不能替代 group rate checked time |

Stage 8 仅完成复查和保护网，不删除字段、不改 schema、不合并字段语义。Runtime snapshot 不消费 UI view model，也不携带明文 secret；后续如需 Rust proxy 注入真实 secret，必须保持 secret manager 边界并新增专门测试。
