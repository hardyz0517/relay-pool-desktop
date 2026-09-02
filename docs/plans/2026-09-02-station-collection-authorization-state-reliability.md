# Station 采集与授权状态可靠性治理计划

状态：Proposed（仅完成调查与方案设计，尚未实施）

日期：2026-09-02

适用范围：Station Collector 写入链路、网页登录授权、collector run/snapshot/current state、Station Asset/Detail read model、Tauri revision notice 与前端查询同步。

关联入口：

- [`../README.md`](../README.md)
- [`../PRODUCT_MODEL.md`](../PRODUCT_MODEL.md)
- [`../specs/INTELLIGENT_ROUTING_ENGINE_SPEC.md`](../specs/INTELLIGENT_ROUTING_ENGINE_SPEC.md)，尤其是 32、40.12、40.15、40.16、40.17 节
- [`../specs/STATUS_MONITORING_REFACTOR_SPEC.md`](../specs/STATUS_MONITORING_REFACTOR_SPEC.md)
- 当前代码、生成 IPC 契约与自动化门禁

本文是一次专项升级计划，不是当前实现事实。实施完成后，当前事实应回写到长期规范或审计记录，不把本文长期维护成第二份产品规范。

## 1. 结论与升级原则

“重新授权后仍显示采集需关注”不是一个前端刷新小问题，而是三个边界同时失效：

1. `stations.status` 把 administrative、collection、credential 和 display rollup 压成一个字符串，任何写入者都可能覆盖另一类事实。
2. Full collection 的父任务和子任务逐笔提交。父任务先基于旧 task state 写 `stations.status`，带 `parent_run_id` 的成功子任务随后又被禁止更新该状态，产生持久的不一致。
3. Web 授权完成被记录成 `task_type = full` 的采集快照，而前端既没有授权完成回执，也没有统一的 revision notice 消费，只能等待下一次页面刷新或后台采集碰巧修正。

本计划不采用以下临时修补：

- 授权结束后额外跑一次 balance，然后直接把 `stations.status` 改成 `healthy`；
- 调换父子任务写入顺序，但继续逐笔更新同一个派生字符串；
- 给 Station 页面增加高频全量轮询；
- 从 `summary_json`、error message 或 HTTP 文案继续猜授权与采集状态；
- 长期双写旧状态和新投影，靠“兼容”维持两个事实来源；
- 只把 4175 行的 `application/collectors.rs` 拆成多个文件，却保留相同的宽职责和调用旁路。

目标是建立一条可证明的链路：

```text
durable intent
  -> bounded outbound execution
  -> one atomic terminal commit
  -> typed current projections + monotonic revisions
  -> best-effort revision notice
  -> backend-owned Station read model
  -> display-only asset rollup
```

其中 SQLite 中的事实和 revision 保证正确性，Tauri event 只降低 UI 延迟。任何 event 丢失都不能重新制造后端双真相。

## 2. 已确认的现象与根因

### 2.1 本次现场证据

2026-09-02 的本地数据中观察到以下顺序，检查过程未读取或输出任何原始凭据：

| 本地时间 | 事实 |
|---|---|
| 08:09:06 左右 | 旧 balance / published-status 结果为 `manual_required`，HTTP 401 |
| 08:09:49 左右 | 新 WebView session 已保存，`session_status = valid` |
| 08:09:54 左右 | 新 full 的 balance、groups、published-status 子任务全部成功，但 Station 仍为 `warning` |
| 08:15:00 左右 | 后续独立 balance 成功后 Station 才变为 `healthy` |

这证明当前凭据已恢复，错误标签由状态投影和刷新链路造成，不是“授权仍然失败”。后续 balance 能修正状态只说明系统最终偶然收敛，不能作为一致性保证。

### 2.2 代码级因果链

| 位置/符号 | 当前行为 | 可靠性问题 |
|---|---|---|
| `services/collectors/mod.rs::apply_prepared_full_collection_v2` | 先提交 full 父任务，再逐个提交子任务 | 一个逻辑操作跨多个事务暴露中间态；任一步骤失败都可能留下半套 current state |
| `CollectorService::apply_result` | root run 才调用 `update_station_collection_status` | 子任务事实成功写入后不再重算 Station 状态 |
| `CollectorStore::update_station_collection_status` | 读取旧 root task state，混入本次 status 后写 `stations.status` | Full 父任务提交时仍可读到授权失败前的 balance/groups 状态 |
| `project_station_collection_status` | 从 `summary_json.childRuns` 解释 core status | JSON 诊断载荷变成当前状态协议；字段漂移不会在类型系统中失败 |
| `CaptureCommandFacade::finish_capture_session_with_events_inner` | 将 WebView 捕获结果写成 `task_type = full` | “保存/验证会话”被错误等同为“完成业务采集” |
| `CollectorService::record_capture_snapshot` | capture 也更新 task state 和 `stations.status` | 没有业务字段的成功授权可以制造 collection warning/partial |
| `stationCollectionIssueTag` | `station.status === warning` 映射为“采集需关注” | UI 无法知道 warning 来自哪个轴、哪个 revision 或哪次操作 |
| `useStationsPageController::handleManualAuthorization` | 命令只负责打开窗口，随后立即结束 action | 页面不知道授权成功、失败、取消和 post-auth collection 的终态 |
| `DomainRevisionNotice` | Rust 内已有 bounded broadcast，但前端 bridge 尚未消费 | 后台 commit 不能可靠触发 Station read model 更新 |
| `AssetRevisionStore::load` | 对多个独立 scope 取 `MAX(revision)` | 某个低 revision scope 前进时，workspace revision 可能完全不变 |

### 2.3 技术债基线

调查时的主要模块规模如下，行数只用于说明职责堆积，不作为机械拆分目标：

| 文件 | 行数 | 混合职责 |
|---|---:|---|
| `src-tauri/src/application/collectors.rs` | 4175 | command use case、事务、投影、告警、query、JSON 状态解释和大量集成测试 |
| `src-tauri/src/services/collectors/mod.rs` | 2389 | driver 编排、任务计划、授权解析、full 聚合、apply 适配和展示摘要 |
| `src-tauri/src/application/command_facades/capture.rs` | 1343 | session/capture/window、授权验证、draft preview 和 collector snapshot 写入 |
| `src/features/stations/useStationsPageController.ts` | 763 | workspace query、本地 server-state 副本、弹窗状态、mutation、缓存失效和 toast |

本计划只处理与采集、授权、Station read model 和 revision 同步直接相关的职责。Updater、Data Recovery、Theme、Provider Draft 的独立能力以及未触及的 routing 算法不进入本轮重构。

## 3. 目标与非目标

### 3.1 完成目标

- Full collection 的 parent、children、snapshots、canonical facts、task projection、collection projection、authorization effect、alerting transition 和 revision 在一个 terminal write transaction 中提交。
- 新授权 revision 产生后，旧 credential revision 上迟到的 401/manual-required 只能进入历史，不能覆盖当前授权状态。
- Web 授权成功只证明 credential/session 状态；只有业务 collector task 才能推进 collection 状态。
- `stations` 只承载 Station identity 和 administrative configuration；其派生 `status`、`last_checked_at`、`last_pricing_fetched_at` 不再是权威来源。
- Station Asset/Detail read model 在单个 `ReadSession` 中返回 typed collection、authorization、endpoint、balance 等轴及 backend-owned rollup，不让前端 join raw snapshot。
- 前台 mutation 返回 `MutationReceipt`；后台 commit 发布 typed `DomainRevisionNotice`。前端通过唯一 scope-to-query-family 映射失效缓存。
- 同一 operation 重试幂等；并发、乱序、进程崩溃、endpoint 修改和 event 丢失都有明确结果。
- 删除本计划列出的旧状态 reducer、JSON 推断、V2 apply adapter、capture-as-full 和组件级缓存失效清单。
- 收敛 collector/capture/Station controller 的职责，每次提取都删除一个原 owner 或旁路，并有架构门禁阻止回流。

### 3.2 非目标

- 不在本计划中重写 Sub2API/NewAPI 的所有 endpoint parser。
- 不改变智能路由评分、重试、熔断或 station-key circuit 算法。
- 不把 published-status、主动 channel monitoring 或 endpoint connectivity 合并成 collector core health。
- 不建立可动态注册 handler 的通用事件总线。
- 不将 raw collector payload、cookie、token、secret id 或完整 URL 放入 event、receipt、日志或 UI read model。
- 不用文件行数、Service 数量或目录层级作为完成标准。

## 4. 目标状态语义

### 4.1 分轴模型

| 轴 | 权威写入者 | 目标状态/字段 | 明确不负责 |
|---|---|---|---|
| Administrative | Station mutation owner | `enabled`、priority、endpoint config/revision、schedule config | collection、credential、endpoint health |
| Authorization | Authorization workflow/reducer | `unknown / verifying / valid / reauthorization_required`、credential revision、evidence code | balance、group、published status、整体健康 |
| Collection execution | Collection operation coordinator | `queued / running / succeeded / partially_succeeded / failed / cancelled / interrupted / superseded` | 当前业务可用性 rollup |
| Collection summary | Collection projector | `not_collected / collecting / healthy / degraded / failed / stale`、core task reasons、freshness | credential 是否有效、routing circuit |
| Endpoint connectivity | 既有 endpoint health owner | typed connectivity verdict、latency、observed time | collector parser 成功率 |
| Balance | Balance projector | present/depleted/unknown/stale、amount、currency、authority | authorization 或 endpoint 状态 |
| Published status | Published-status owner | 独立 source state/completeness | core collection degradation |
| Asset rollup | `AssetStatusRollupProjector` | display-only `ready / attention / unavailable / disabled` + reason codes | 写回 canonical table 或供 Routing 反向解析 |

`manual_required` 不再是 collection summary status。目标 task outcome 使用结构化字段表达：

```text
TaskOutcome {
  completion: succeeded | partial | failed | skipped
  failure_class?: auth_rejected | timeout | transport | malformed_payload | unsupported | ...
  auth_effect: none | confirms_valid | requires_reauthorization | indeterminate
  evidence_authority
  observed_at_ms
}
```

展示“需重新授权”只读取 Authorization axis；展示“采集需关注”只读取 Collection axis 的 typed reason。两个问题同时存在时可同时展示，不互相覆盖。

### 4.2 Core/optional task 契约

每个 provider driver 提供版本化 `CollectionPlan`，为 task 声明 role：

```text
CollectionPlan {
  plan_version
  tasks[]: { task_type, role: core | optional, fact_families[] }
}
```

- core task 决定 collection summary；optional task 只更新自身 source state。
- `published_status` 保持 optional，不因其失败将 Station core collection 降为 failed。
- Full operation 必须带完整 plan version 和预期 task set；terminal commit 拒绝缺少未知原因的 core result。
- 新增 provider task 时扩展 plan 和 reducer 测试，不在 `match task_type` 的多个位置继续散落硬编码。

### 4.3 强制不变量

1. **单一事实来源**：`stations.status`、snapshot JSON、前端 view model 都不能成为 current collection/authorization authority。
2. **原子终态**：一个 operation 的 terminal facts 和 current projections 要么全部可见，要么全部不可见。
3. **单调意图**：current projection 只接受不旧于当前 watermark 的 intent sequence。
4. **revision fence**：collector 的 endpoint revision 或 credential revision 过期时，只记录 `superseded` 历史，不更新 current facts/projections。
5. **幂等重试**：相同 operation id + canonical result 重试返回同一 receipt；相同 id + 不同 payload 为 invariant violation。
6. **时间分离**：started、observed、committed、fresh-until、projected 时间各自保存，不用 `updated_at` 推断其他时间。
7. **类型驱动**：reducer 不解析 `summary_json`、error message 或 adapter 私有字符串决定状态。
8. **查询纯读**：Station query 不修 projection、不写 task state、不触发采集。
9. **通知非权威**：commit 先完成，再发布 notice；notice 丢失、重复或乱序不影响数据库正确性。
10. **敏感数据边界**：revision、reason code、operation id 可通知；cookie/token/raw body/credential handle 不可通知。

## 5. 目标持久化模型

最终表名和 migration 编号以实施时未占用的下一个 schema 为准；不得修改历史 migration。建议的目标结构如下。

### 5.1 Operation ledger

新增 `collector_operations` 作为逻辑采集操作，不再把 full parent run 同时当 operation、snapshot 和投影触发器：

| 字段 | 约束/用途 |
|---|---|
| `id` | UUIDv7，唯一 operation identity |
| `operation_key` | command/schedule slot 的幂等 key，唯一 |
| `station_id` | FK，删除 Station 时级联 |
| `endpoint_revision` | intent 创建时捕获 |
| `credential_revision` | intent 创建时捕获，不含 secret |
| `intent_sequence` | Station-scoped durable monotonic integer |
| `plan_version` | CollectionPlan 版本 |
| `requested_task` | full 或单任务 |
| `trigger_kind` | manual/scheduled/post_authorization/retry |
| `status` | queued/running/terminal/superseded/interrupted |
| 时间字段 | requested/started/finished/committed 分开 |
| terminal metadata | bounded reason/error code，不含原始响应 |

现有 `collector_runs` 收敛为 operation 内的 task attempt history：

- 新增非空 `operation_id`（迁移期允许旧行为空）和 task role；
- 不再用 `parent_run_id IS NULL` 判断谁有权更新 Station；
- `run_key` 保持 task attempt 幂等，但 current projection 由 operation terminal commit 统一推进；
- 历史 `parent_run_id` 在所有消费者迁移后进入物理删除资格，不成为永久兼容字段。

### 5.2 Current projections

新增或重建以下 typed projection：

1. `station_collection_task_summaries`
   - 主键 `(station_id, task_type)`；
   - 保存 current operation、intent sequence、endpoint/credential revision、completion、failure class、observed/committed/fresh-until；
   - 不保存需要从 JSON 读取的权威字段。
2. `station_collection_summaries`
   - 主键 `station_id`；
   - 保存 collection status、current operation/intent、core success/failure counts、reason codes、last attempt/success、fresh-until 和 projection version；
   - reducer 可从 task outcomes/facts 重建，禁止直接手工 UPDATE 某个 display status。
3. `station_authorization_summaries`
   - 主键 `station_id`；
   - 保存 authorization status、credential revision、evidence authority/code、source operation、observed/verified time；
   - 只接受同 credential revision 或明确更新 revision 的 transition。

`station_credentials` 增加 durable `credential_revision`，每次 session/credential material 创建、替换或清除均原子递增。该 revision 可公开到受限 read model，但绝不暴露 secret 内容或 secret id。

### 5.3 Revision scopes

创建并维护以下 durable scope：

- `station_collection:{station_id}`
- `station_authorization:{station_id}`
- `read_model:station_assets`
- 需要时的 `read_model:station_detail:{station_id}`，或由明确 revision vector 替代

`read_model:station_assets` 是 workspace family 的独立单调 revision，不能继续对多个互不相关 scope 做 `MAX(revision)`。每次影响 Station Asset rows 的 transaction 都推进该 family revision；read model 在同一 `ReadSession` 中读取 rows 和 revision vector。

### 5.4 Durable post-authorization work

授权成功后的业务采集不能依赖 WebView callback 中的一次易失 `spawn`。新增 typed durable work item，或扩展既有 collector schedule state 表达：

```text
post_authorization_collection {
  station_id
  credential_revision
  requested_task = full
  dedupe_key
  status
  next_attempt_at
  bounded_attempt_count
}
```

- session 保存与 work item 创建必须位于同一个可恢复用例边界；
- supervisor runner 按 at-least-once 消费，operation idempotency 去重；
- 重启后未完成 work item 可继续；
- credential revision 已被再次替换时，旧 work item 标记 superseded；
- 退避有上限，并在 UI 显示验证/采集中，而不是永久 spinner。

不要为此引入通用消息队列；这是 collector owner 的窄 durable work table。

## 6. 写入协议与并发语义

### 6.1 Intent 阶段

在任何 outbound I/O 前开启短事务：

1. 校验 Station enabled、endpoint revision 和当前 credential revision。
2. 为 Station 分配单调 `intent_sequence`。
3. 以 operation key 幂等创建 `collector_operations` row。
4. 提交后再执行网络请求，禁止持有 SQLite write lock 等待上游。

同站点并发策略不再依赖进程级 HashSet 作为正确性机制。运行时 guard 可以保留用于节流，但数据库 intent sequence 和 commit fence 才是跨重启、跨入口的正确性边界。

### 6.2 Execute 阶段

- driver 只返回 typed `PreparedCollectionResult`，不写数据库、不发布 UI event；
- Full 的 child task 可按 provider contract 串行或受控并行，但共享 operation id、endpoint revision、credential revision 和 cancellation；
- 所有 response 先完成脱敏和 typed classification；raw redacted evidence 只用于历史诊断；
- 取消、超时、panic/worker failure 必须形成 typed terminal outcome，不能让 operation 永久 running。

### 6.3 Terminal commit 阶段

`CollectionCommitService::commit_operation` 是唯一 terminal write owner，在一个 transaction 中：

1. 读取 operation 并校验仍为可提交状态。
2. 校验 canonical request hash、task set、plan version 和结果数量。
3. 比较当前 endpoint revision、credential revision 和每个 task watermark。
4. 若 stale，写 operation `superseded`/历史终态后结束；不写 current facts、task summary、authorization summary 或 alert transition。
5. 写 task runs、snapshots 和 canonical normalized facts。
6. 由纯函数 `CollectionProjectionReducer` 计算 task/collection transition。
7. 由纯函数 `AuthorizationProjectionReducer` 应用本批次的 auth effects。
8. 写 critical alerting observation/transition；optional projector work 只写 dirty/checkpoint 输入。
9. 推进 per-station 和 Station Asset family revisions，生成 `MutationReceipt`。
10. 将 operation 标记 terminal 并提交。

commit 成功后才发布 `DomainRevisionNotice`。发布失败记录 bounded runtime diagnostic，不能回滚已提交业务 transaction，也不能重试业务写入。

### 6.4 单任务与 Full 的统一

单任务采集是只包含一个 task result 的 operation；Full 是包含完整 plan results 的 operation。两者使用同一个 commit/reducer，不再维护两套 `apply_station_output` 与 full-parent 特例。

Full 成功时，本次 operation 内的 core results 直接形成一致的 collection summary，不读取授权失败前的 root task state。独立 balance 只推进 balance task watermark，并与当前其他 core task summary 重算；它不能清除一个更新 revision 上仍存在的 groups failure。

### 6.5 乱序规则

| 场景 | 结果 |
|---|---|
| 旧 full 在新 full 后完成 | 保存历史，旧 intent 不推进 current projection |
| 旧 credential 上的 401 在重新授权后完成 | 保存历史，不能写 `reauthorization_required` |
| endpoint 配置在采集中被修改 | operation superseded，facts/current state 均不应用到新 endpoint |
| 同 operation terminal 重试 | 相同 hash 返回原 receipt，不重复 facts/alert |
| 相同 operation id 携带不同结果 | invariant violation，拒绝提交并记录安全诊断 |
| optional task 失败 | optional source state更新，core collection 不降级 |
| terminal transaction 中任一步失败 | 整个 transaction 回滚，operation 保持可重试/可恢复状态 |

## 7. 授权工作流

### 7.1 正确的状态转换

```text
open authorization window
  -> capture candidate credential
  -> persist credential material + advance credential revision
  -> authorization = verifying
  -> driver-owned authenticated self probe
      -> valid: authorization = valid + enqueue post-auth full collection
      -> definitive auth reject: authorization = reauthorization_required
      -> timeout/transport/unsupported: authorization = unknown/verifying with typed reason
```

“捕获到了 cookie”不等于“authorization valid”；“authorization valid”也不等于“balance/groups 已采集成功”。两步必须分别显示和重试。

### 7.2 Capture 语义收敛

- WebView HTTP events 可写入独立、脱敏、有限保留的 capture evidence/history。
- 授权完成不再调用 `record_capture_snapshot(task_type = full)`。
- 没捕获到业务字段时，不创建 partial/full collection state；authorization probe 成功后仍可进入 valid。
- Draft preview 的 capture 能力保留，但不得通过共享 helper 误写已保存 Station 的 collection projection。
- WebView close、用户取消、窗口崩溃和 app shutdown 都产生 typed attempt terminal state，不静默停在 verifying。

### 7.3 恢复规则

- 同 credential revision 上的 authenticated success 可以清除该 revision 的 reauthorization requirement。
- 新 credential revision 初始为 verifying，不继承旧 revision 的 definitive failure。
- post-auth collection 因 timeout/transport 失败时，authorization 保持 valid/unknown（依据证据），collection 可 degraded/failed；不得重新提示授权。
- 只有 typed `AuthRejected` 或同等权威证据可以设置 `reauthorization_required`。
- 历史 snapshot 中的 `manualActionRequired` 只用于展示历史，不再参与 current reducer。

## 8. Read Model 与前端同步

### 8.1 后端 Station read model

升级 `StationAssetReadModel` 与 `StationDetailReadModel`，在后端单个 read transaction 中批量返回：

```text
StationAssetRow {
  identity
  administrative
  endpoint_summary
  collection_summary
  authorization_summary
  balance_summary
  capability_summary
  economics_summary
  quality_summary
  asset_rollup { status, reasons[] }
  revision_vector
}
```

- 列表 query 使用固定上限的批量 SQL，不按 station 循环查询 latest snapshot。
- 历史 runs/snapshots 只在详情按 cursor 加载，不参与列表 current state。
- projection 缺失或落后时明确返回 `unknown/stale/rebuilding`，query 不进行修复。
- 前端不读取 `summaryJson.loginRequired`、`station.status` 或 snapshot status 推导当前标签。

### 8.2 Mutation receipt 与 revision notice

统一窄回执：

```text
MutationReceipt {
  mutation_id
  committed_at_ms
  affected_scopes[]
  revision_vector[]
}
```

统一窄通知：

```text
DomainRevisionNotice {
  mutation_id
  affected_scopes[]
  revision_vector[]
}
```

- foreground collect/authorize command 返回 receipt 或 operation handle；完成事件携带 revision notice，不携带业务 payload。
- 新增版本化的 `domain-revision-updated` Tauri event（payload 为受限的 `DomainRevisionNotice`）；不要复用当前只表示告警变化、payload 为空的 `alerting-read-model-updated`。
- Rust `DomainRevisionNotice` 通过 composition-owned bridge 发到 Tauri；移除其 production `dead_code` 豁免。
- 订阅采用“先 subscribe，再 reconcile revision”的启动顺序，关闭注册竞态。
- broadcast lag/重连/窗口恢复时执行 revision reconciliation，而不是假设收到了每条 event。

### 8.3 前端唯一失效映射

新增一个 application-level synchronizer，维护唯一映射，例如：

| scope | query family |
|---|---|
| `station_collection:*` | Station Asset/Detail、collector current/history、相关 balance projection |
| `station_authorization:*` | Station Asset/Detail、credential metadata、capture/authorization attempt status |
| `read_model:station_assets` | Station Asset workspace |

组件不再手写 `stations + stationAssets + balances + snapshots + runs` 的 invalidate 列表。前台 command receipt 可以立即调用同一映射；后台 notice 也调用同一映射。

event 丢失的兜底策略：

- app foreground/resume 后比较 Station Asset family revision；
- Station 页面 active 时以约 30 秒的低频轻量 revision probe 兜底，而不是全量 refetch；
- revision 变化才失效对应 query family；
- query response 的 revision/workspace identity 防止旧响应覆盖新 cache。

### 8.4 Station 前端职责收敛

`useStationsPageController` 只保留页面编排和纯 UI state。以下 server state 迁回 React Query/read model owner：

- credentials metadata；
- collector snapshots/runs；
- group/rate current projection；
- per-station collection/authorization action terminal state。

可按真实职责提取 mutation hooks 和 dialog state，但每次提取必须同时删除 controller 内的 server-state copy、手工 loader 或失效旁路。禁止创建一组只原样转发 API 的 `useXxxService`。

## 9. 迁移与切换策略

### 9.1 Additive migration

第一阶段 migration 只新增 operation/projection/revision/work tables 和必要索引，不删除历史 run/snapshot。迁移应：

- 为既有 `station_credentials` 初始化 `credential_revision = 1`；不存在 credential row 的 Station 在读取时返回 explicit unknown，不伪造 valid；
- 为每个 Station seed collection/authorization/revision scope；
- 从 `collector_task_state`、关联 run 和 current endpoint revision 做 bounded backfill；
- 不从 `stations.status = healthy/warning/error` seed 新 collection、health 或 quality 事实；
- 不从任意 raw JSON 或错误文案推导授权状态；
- ambiguous legacy 状态标记 `unknown` 或 `degraded + migration_ambiguous`，等待下一次真实采集收敛；
- backfill 可重复执行且有 postcondition、row count 和 SQLite integrity 检查。

允许使用结构明确的 legacy task state 作为 collection backfill 输入，但必须记录 `provenance = legacy_task_state`。历史 capture-as-full 不能作为 core collection success。

### 9.2 Shadow qualification

切换前短期运行 new projection shadow：

- 旧 UI 仍读旧 contract；新 writer/reducer 计算新 projection；
- 审计任务比较可比较场景，记录差异原因，不自动把旧值覆盖到新值；
- 本次已知 bug 场景应明确为“新 healthy/valid，旧 warning”，这是预期修复而不是 parity failure；
- shadow 只用于资格判断，不允许无限期双写。

资格至少覆盖真实数据副本的脱敏统计、fixture 重放和故障注入。达到门禁后，Station read model/UI 原子切换到新 projection。

### 9.3 Authority cutover

切换 revision 内同时完成：

1. 所有 collector/capture writers 停止写 `stations.status` 和 legacy task projection。
2. Station Asset/Detail 改读新 projection。
3. 前端删除旧字段 fallback 和 JSON 推断。
4. generated Rust/TypeScript DTO、DemoBackend、fixtures、command registry 同步更新。
5. 架构门禁改为要求新 owner，并禁止旧 writer/reducer/symbol。

不能先停止旧写入但仍让某个页面读取旧字段，也不能先切 UI 后让 scheduled collector 继续走旧 writer。

### 9.4 物理删除

物理 DROP/重建表是独立资格阶段。在至少一个完整版本的读写扫描、portable migration、export/import、旧 fixture 升级和 rollback 方案通过前，不急于删除列；但 production 代码中不得继续读取或写入这些列。

最终删除候选：

- `stations.status`、`stations.last_checked_at`、`stations.last_pricing_fetched_at` 中已被明确 projection 替代的派生列；
- legacy `collector_task_state`；
- `collector_runs.parent_run_id`（operation_id 全量回填且消费者迁移后）；
- 仅服务旧 status contract 的 DTO 字段、fixture 和 index。

物理删除前需要明确 portable schema fingerprint、导入导出兼容性和旧安装包数据库不可降级事实。失败恢复使用升级前备份或新版本前滚，不尝试 SQL downgrade。

## 10. 代码职责收敛与删除清单

### 10.1 目标 owner

| Owner | 唯一职责 | 不允许依赖/承担 |
|---|---|---|
| `CollectionOperationCoordinator` | create intent、调用 driver、取消/终态编排 | SQL 细节、UI event、JSON current reducer |
| `CollectionCommitService` | terminal Unit of Work、幂等、fence、receipt | outbound I/O、页面 DTO |
| `CollectionProjectionReducer` | typed outcomes -> task/collection transition 的纯函数 | store、clock、Tauri |
| `AuthorizationWorkflow` | credential revision、probe、post-auth work | balance/group projection |
| `AuthorizationProjectionReducer` | typed auth evidence 的单调 transition | raw response/error text |
| `CollectorHistoryQuery` | cursor 化 run/snapshot history | current projection修复 |
| `StationAssetsQuery`/`StationDetailQuery` | 单事务装配 consumer read model | mutation、页面间 query 调用 |
| `DomainRevisionSynchronizer` | scope-to-query-family 映射和 reconcile | 业务状态 payload、后端正确性 |

`CollectorService` 可在迁移期间作为 composition facade，但最终不能继续同时拥有 query、command、transaction、projection 和 alert policy。facade 每迁移一个 caller 就删除对应旧方法，最后删除或仅保留有真实编排职责的窄入口。

### 10.2 强制删除/替换项

| 当前项 | 目标动作 | 删除门槛 |
|---|---|---|
| `apply_prepared_full_collection_v2` 的父后子逐笔 apply | 由统一 operation terminal commit 替换 | full atomic/fault-injection tests 通过 |
| `V2CollectorApplyAdapter` / `apply_station_output_v2` | 删除同形适配层 | 所有单任务/full caller 使用 operation coordinator |
| `update_station_collection_status` | 删除 | 无 writer 触碰 `stations.status` |
| `aggregate_station_collection_status` / `project_station_collection_status` | 删除 | typed reducer 成为唯一 collection owner |
| `station_collection_status_for_request` | 删除 | request 不再携带 display station status |
| `request_requires_manual_authorization` 对 JSON/字符串的推断 | 用 typed auth effect 替换 | 所有 driver failure 完成 typed classification |
| `record_capture_snapshot(task_type = full)` | 拆为 capture evidence + authorization workflow | capture/authorization regression 通过 |
| `parent_run_id IS NULL` 决定副作用权限 | 用 operation commit owner 替换 | history/query caller 迁移完成 |
| `AssetRevisionStore::MAX(revision)` | 用 family revision/vector 替换 | revision monotonic tests 通过 |
| `stationCollectionIssueTag` 读取 `station.status`/summary JSON | 只格式化 typed rollup/reasons | Station read model contract切换 |
| component/controller 手工 invalidate 清单 | 使用统一 revision mapping | foreground/background tests 通过 |
| Station controller 的 server-state maps | 使用 query cache/history queries | 页面行为测试和窄窗口测试通过 |

### 10.3 防止“屎山搬家”的门禁

新增 collector-state architecture gate，至少断言：

- production 中不存在 `UPDATE stations SET status`；
- current projection reducer 不引用 `serde_json::Value`、raw payload 或 error message；
- capture command facade 不能调用 collection full snapshot writer；
- driver 模块不依赖 persistence store、Tauri app handle 或 React-facing DTO；
- query 模块不开 write transaction；
- Station 前端不读取 `station.status`、`summaryJson.loginRequired` 推导 current tag；
- scope-to-query-family 映射只有一个 production owner；
- 新 DTO 只通过 binding generator 更新，不存在手写镜像 enum fallback；
- 每个新 wrapper 必须拥有 validation、use-case orchestration、transaction、domain conversion、transport 或 cache identity 中至少一个不变量。

门禁必须同时正向检查新 owner 和负向检查旧路径，不能用“搜索不到某字符串”的空匹配伪装完成。

## 11. 分阶段实施计划

### Task 0：冻结行为、调用图与事故 fixture

**工作**

- 建立本次状态写入者、query consumer、capture callback、scheduled/manual caller 和 revision consumer 清单。
- 将“旧 manual-required -> 新授权 -> full children 全 success -> 旧 Station warning”的脱敏场景固化为数据库/应用回归 fixture。
- 为现有 intended behavior 建 characterization tests：optional published-status、alert recovery、endpoint revision rejection、idempotent run key、scheduled cancellation。
- 建立删除台账，记录每个旧 symbol 的 caller、replacement、test 和删除阶段。

**退出门禁**

- fixture 在旧实现上稳定复现错误；测试名称不会被 Cargo 过滤成 0 tests。
- 每个 production writer/reader 都有归属，不以 `rg` 未命中代替调用图。
- 本 Task 不修改运行语义。

### Task 1：建立 typed outcome、plan 与纯 reducer

**工作**

- 定义 `CollectionPlan`、`TaskOutcome`、`AuthorizationEvidence/Effect`、projection state/reason code。
- 把 Sub2API/NewAPI failure classification 适配到 typed outcome；adapter 私有文本只留在诊断摘要。
- 实现纯 `CollectionProjectionReducer` 和 `AuthorizationProjectionReducer`。
- 对所有 core/optional 状态组合做 table/property tests，冻结 stale、auth-effect 和 freshness 规则。

**退出门禁**

- reducer 不依赖 JSON/store/clock/Tauri。
- 未知 enum/version fail closed 为 typed contract error，不默认 healthy。
- 所有现有 driver terminal path 能生成 typed outcome。

### Task 2：Additive schema、intent ledger 与 recovery

**工作**

- 新增 operation、task/collection/auth projection、credential revision、family revision 和 post-auth work schema。
- 实现 operation create/claim/interrupt/supersede、startup reconciliation 和幂等 store primitives。
- 实现 migration backfill、postconditions、portable schema fixture 和索引 query-plan 测试。
- 保持历史 run/snapshot 可读，尚不切换 UI authority。

**退出门禁**

- 迁移可重复启动，schema/foreign-key/quick-check 通过。
- running operation 在 crash/restart 后有确定的 interrupted/retry 结果。
- intent sequence 和 credential revision 同毫秒多次更新仍单调。

### Task 3：统一 atomic commit 与并发 fence

**工作**

- 引入 `CollectionCommitService` 和 `MutationReceipt`。
- 单任务、Full、manual、scheduled 统一走 operation protocol。
- parent/children/facts/projections/alert/revisions 在一个 terminal transaction 中提交。
- 增加 stale endpoint、stale credential、stale intent、duplicate commit 和 transaction fault injection。
- shadow 计算新 projection，保留短期审计，不切 UI。

**退出门禁**

- Full 任一步注入失败都没有半套可见 state。
- 本次事故 fixture 在新 projection 中稳定得到 authorization valid + collection healthy。
- 100 次受控乱序/并发重放结果确定，不依赖完成顺序。
- scheduled 与 manual caller 不存在旧 apply 旁路。

### Task 4：授权与 capture 解耦

**工作**

- 授权 session 保存推进 credential revision，并使用 driver-owned authenticated probe。
- capture evidence 与 collection snapshot 分离。
- 新增 durable post-auth full work 和 bounded retry/recovery。
- WebView success/failure/cancel/close/shutdown 形成 typed terminal attempt。
- 旧 credential 上迟到的 auth rejection 受 revision fence 约束。

**退出门禁**

- 授权成功但无业务 capture fields 不会制造 full partial/warning。
- post-auth work 在 commit 后崩溃并重启仍能执行且不重复 facts。
- auth valid + collection transport failure 能在 UI 模型中同时正确表达。

### Task 5：Station read model 与 revision bridge

**工作**

- 升级 Station Asset/Detail DTO 和固定 query-bound repository。
- 增加 backend-owned asset rollup/reason codes 和 revision vector。
- 将 Rust `DomainRevisionNotice` 接入 Tauri composition；实现 subscribe-before-reconcile。
- 实现 family revision，删除 `MAX(independent revisions)` 语义。
- 更新 command registry、generated bindings、DemoBackend 和 fixtures。

**退出门禁**

- 10/100/500 Station 的 query 数量保持固定上限，无 N+1。
- 同一 read response 中 rows 与 revisions 来自一个 `ReadSession`。
- notice 重复/乱序/丢失测试通过，丢失只增加 UI 延迟。

### Task 6：前端 authority cutover

**工作**

- Station 页面只消费 typed Station read model。
- 增加全局 revision synchronizer 和唯一 scope mapping。
- foreground receipt、background notice、resume/revision probe 复用同一 invalidation owner。
- 清除 controller 的 current server-state 副本和旧 invalidation 清单。
- 更新 loading/empty/error/disabled、授权 verifying、post-auth collecting 和窄窗口状态。

**退出门禁**

- 授权完成后无需切页或等下一次 scheduler 即更新标签。
- 页面不解析 raw/normalized snapshot 推导 current status。
- event 丢失后 foreground/revision probe 可在约定窗口内收敛。
- 相关 Vitest、键盘/focus 和窄窗口 UI 测试通过。

### Task 7：删除旧状态链与收敛模块职责

**工作**

- 执行 10.2 的代码删除清单，移除 V2/legacy 正向门禁和 dead-code 豁免。
- 将 `application/collectors.rs` 按 commit/query/reducer/alert transition 的真实 owner 迁移；每次迁移同步删除旧方法和 imports。
- 将 capture facade 限制为 command validation/window-session orchestration，不再拥有 collector projection。
- 删除前端 fallback、过期 query key、mock、fixture、re-export 和手写类型。
- 更新 architecture manifests/audits，新增长期负向门禁。

**退出门禁**

- 无 production read/write 依赖 `stations.status`。
- 无 production `parent_run_id` 副作用判断、capture-as-full 或 JSON current reducer。
- CollectorService 不再同时承担 command/query/transaction/projection/capture。
- dead-code inventory 没有本计划新增的临时 exception。

### Task 8：迁移资格、物理删除与文档收口

**工作**

- 跑受支持旧 schema fixture、portable export/import、真实数据脱敏 shadow diff、startup restart 和 recovery matrix。
- 在资格满足后执行独立物理删除 migration；否则保留列但保持零 production consumer，并记录未满足条件。
- 更新 `docs/README.md` 当前事实、长期规范、审计记录和 release note。
- 将本计划状态更新为 Completed 或明确记录未完成门禁，不能用部分通过冒充完成。

**退出门禁**

- 当前 schema fingerprint、fixture 和生成产物一致。
- 目标旧字段在 qualified migration 后物理不存在，或有明确 no-go 证据且 production 零依赖。
- 文档描述与代码、命令和测试名称一致。

## 12. 测试与故障注入矩阵

### 12.1 Reducer 单元测试

- core 全 success -> collection healthy；
- core partial/failed 的完整排列与 reason 优先级；
- optional failed 不降级 core；
- no evidence/not collected/stale/freshness boundary；
- auth valid/reauthorization/indeterminate authority precedence；
- 新 credential revision 不继承旧 revision failure；
- unknown task/plan version fail closed；
- reducer replay determinism，同输入永远同输出。

### 12.2 Persistence/transaction 测试

- Full parent + all children + facts + projections + revisions 原子成功；
- 在每个 write step 前后注入失败，事务无部分可见状态；
- 同 operation retry 幂等，不重复 run/fact/alert；
- operation key payload mismatch 被拒绝；
- old intent after new intent、old credential after reauth、old endpoint after edit 均不覆盖 current；
- individual balance 与 Full 乱序完成仍得到确定 projection；
- revision 同毫秒推进、broadcast lag 和 family revision 单调；
- startup 将遗留 running operation/work item 恢复为可解释状态。

### 12.3 授权集成测试

- 旧 manual-required -> 新授权 valid -> post-auth full core success，最终不显示 collection warning；
- 授权 valid 但 capture 无业务字段，collection 保持旧值/unknown，不写 partial full；
- 授权 valid 后网络 timeout，显示 collection failure，不显示 reauthorize；
- 授权后旧采集迟到 401，authorization 仍 valid；
- 用户取消、窗口关闭、自探测失败、app shutdown/restart；
- post-auth durable work 至少一次执行和去重；
- 所有日志/event/receipt/snapshot fixture 不含完整 secret、cookie、token 或原始认证 body。

### 12.4 Read model/前端测试

- Station Asset/Detail rows + revision 同 transaction；
- asset rollup reason priority 和多标签展示；
- scope-to-query-family 精确映射，不扇出刷新无关页面；
- subscribe-before-reconcile 关闭启动竞态；
- duplicate/out-of-order/lost notice；
- action state 从 authorize -> verifying -> collecting -> terminal；
- loading/empty/error/disabled、页面隐藏/恢复和窄窗口；
- current UI 源码门禁禁止 `station.status`/snapshot JSON authority。

### 12.5 规模与性能

- 10/100/500 Stations 的 list query 数量固定，SQL plan 使用目标索引；
- Full terminal transaction 不包含 outbound I/O，write-lock duration 有可观测上限；
- burst notice 合并为按 scope 的 bounded invalidation，避免 500 次全量 refetch；
- history 使用 cursor/bounded limit，不随运行时间无界加载；
- durable work retry 有 backoff、attempt cap 和诊断，不形成紧循环。

## 13. 可观测性与运维

新增稳定 runtime event code，至少覆盖：

- operation intent created/started/committed/superseded/interrupted；
- terminal commit retry/conflict/invariant violation；
- authorization verifying/valid/reauthorization-required（只含 reason code/revision）；
- post-auth work queued/retried/exhausted；
- revision notice published/lagged/reconcile-failed；
- projection rebuild/backfill started/completed/failed。

诊断输出包含 station 的 opaque id、operation id、endpoint/credential/intent revision、task type、reason code 和 correlation id；不包含 station cookie、token、密码、API key、完整响应或可还原 secret 的 URL query。

建议的 bounded 指标：

- operation terminal latency 与 commit latency；
- superseded/stale-result count；
- post-auth completion latency；
- projection/notice revision lag；
- authorization-required transitions 与 recoveries；
- terminal commit retry/invariant failure count。

## 14. 验证命令与交付门禁

每个 Task 优先运行聚焦测试。涉及 Rust/Tauri、跨层契约和生成绑定的阶段，最终至少执行：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml <focused-filter>
pnpm test -- <focused-vitest-files>
pnpm generate:bindings
pnpm build
pnpm verify:fast
```

Task 3-8 属于跨层共享事实和架构变更，合并资格使用 `pnpm verify:full`。`pnpm verify:release` 只在明确发布验证时执行。所有 filtered test 必须核验实际执行数大于 0；不能把 0 tests 的退出码 0 记为通过。

发布前必须补一轮真实 Tauri WebView 验收：重新授权、自动 self-probe、post-auth full、页面即时刷新、关闭/取消、离线失败和重启恢复。记录脱敏日志与 read-model revisions，不记录真实 credential。

## 15. 完成定义

本计划只有同时满足以下条件才可标记 Completed：

- 本次事故 fixture 和并发/故障注入矩阵全部通过。
- Full collection 不存在跨事务暴露 parent/child 中间 current state。
- 新授权 revision 能隔离所有旧授权失败结果。
- capture 不再伪装成 full collection。
- Station Asset/Detail 只消费 backend-owned typed projections。
- production 中没有 `stations.status` 的派生状态读写，没有 JSON/error-text current reducer。
- foreground receipt、background notice、resume reconciliation 共用一个 scope mapping owner。
- event 丢失只影响 UI 延迟，不影响后端 correctness。
- 10.2 删除清单已清零，或物理列删除有单独 no-go 证据且 production 零依赖；不能遗留无期限 compatibility path。
- 新 owner、forbidden dependencies、query bound、安全 redaction 和生成契约均有自动门禁。
- 相关 Rust tests、Vitest、`pnpm build`、`pnpm verify:fast`、`pnpm verify:full` 实际通过；未执行项有明确原因和影响说明。
- 当前规范、审计、schema/portable fingerprint、生成 bindings 与实现一致。

## 16. 实施顺序上的硬约束

1. 先冻结事故和并发行为，再移动代码。
2. 先建立 typed outcome/reducer，再建表；schema 不接收另一套字符串语义。
3. 先让所有 writer 统一进入 atomic commit，再切 read authority。
4. 授权/capture 解耦与 credential revision fence 必须在前端去掉旧 fallback 前完成。
5. read model、revision bridge 和前端 scope mapping 同一切换窗口交付。
6. 每迁移一个 caller 就删除一个旧入口；不得先复制全套新架构再无限期保留旧链。
7. 物理删除晚于逻辑 authority cutover，但零 production 依赖是本计划内的强制结果。

按此顺序，系统可以在每个阶段给出明确的正确性证据，并且不会用 UI 刷新、下一次定时采集或偶然的写入顺序掩盖数据库中的状态不一致。
