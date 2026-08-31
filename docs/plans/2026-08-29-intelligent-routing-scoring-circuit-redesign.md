# Relay Pool Desktop 智能路由评分、重试与 Key 熔断重构实施计划

状态：Completed（本轮交互发布范围）；v3 运行链、代际切换和兼容边界清理已完成，最终重试语义已通过聚焦验证。仓库级完整验证沿用本轮较早基线；最终 `verify:fast` 复跑被正在运行的桌面程序锁定可执行文件阻断。本计划不包含压测、长时间 soak 或安装包资源画像。

日期：2026-08-29

目标规范：[`../specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md`](../specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md)

关联当前规范：

- [`../specs/INTELLIGENT_ROUTING_ENGINE_SPEC.md`](../specs/INTELLIGENT_ROUTING_ENGINE_SPEC.md)
- [`../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md`](../specs/INTELLIGENT_ROUTING_RETRY_FAILOVER_CONFIGURATION_SPEC.md)
- [`../specs/ROUTING_POLICY_CONFIGURATION_SYSTEM_SPEC.md`](../specs/ROUTING_POLICY_CONFIGURATION_SYSTEM_SPEC.md)
- [`../README.md`](../README.md)

本文是执行计划和实施记录，不改变目标规范中的行为定义。目标规范与当前代码/自动化契约冲突时，先按本文的基线和迁移步骤收口，不通过修改测试或保留隐式兼容分支来掩盖差异。目标规范已经提升为 `docs/specs/` 的批准入口；本文件仍保留阶段门和未完成事项，不能单独作为运行时事实来源。

## 当前实施状态（2026-08-30）

以下状态以当前代码、自动化契约和 [`audits/2026-08-29-intelligent-routing-scoring-circuit-redesign-implementation.md`](../audits/2026-08-29-intelligent-routing-scoring-circuit-redesign-implementation.md) 为准：

| 阶段 | 状态 | 证据/剩余工作 |
| --- | --- | --- |
| P0 基线与契约冻结 | 已完成 | 批准规范、迁移占用、身份格式、时间衰减 golden vector 已登记 |
| P1 Policy v3 / migration / bindings | 已完成 | `0060`--`0070`、Rust/TypeScript 生成绑定和字段校验已通过契约门禁；`0070` 标记 circuit event 是否已应用，重建只消费已应用事件 |
| P2 outcome / observation / dedupe | 已完成 | 真实请求与监控写入、429/502 归责、correlation 去重和事件时间状态已接入 |
| P3 quality projector | 已完成 | 来源独立计算、最近/历史门槛、乐观值、70/30 混合、固定点衰减和 generation checkpoint 已实现 |
| P4 Key circuit | 已完成 | 连续失败、递增冷却、Half-Open 单 lease、CAS/迟到结果和 deadline reaper 已实现 |
| P5 planner / capacity | 已完成 | 同层评分降序、无随机探索/rendezvous、容量域生产输入移除；本地容量仍是后置硬门 |
| P6 retry / proxy | 已完成 | `maxRetryCount` 限制额外 Key 数量、连续失败阈值控制同 Key 重试、429 普通 Key 故障、boundary-only raw attempt count 和流式 commit gate 已实现 |
| P7 settings / station editor | 已完成 | 设置分组和说明、容量域编辑区移除、诊断字段已接入 |
| P8 generation cutover | 已完成 | coordinator、registry、rebuild/checkpoint、fence、qualification、tail replay、rollback replacement 和原子切换均有正常规模自动化证据 |
| P9 cleanup / release verification | 已完成 | v3 production admission 不再读取旧 error-rate/health 路径；兼容模块仅留在迁移、诊断和测试边界；先前 `pnpm.cmd verify:full` 已通过，最后的 schema/circuit 收口由聚焦回归覆盖 |

当前实现的关键边界：普通容量 lease 是进程内 RAII 资源，进程退出时 registry 一并丢弃，不会留下 SQLite 悬挂占用；Half-Open lease 才是需要 durable reaper 的持久化租约。若未来要把容量租约持久化，应另立 schema 和恢复规范。

当前验证基线：schema `0070`；portable catalog `110` 张用户表；fixture fingerprint 由 schema reader 的受信对象集合维护；先前 `pnpm.cmd verify:full` 最终退出码 `0`，前端 `139` 个测试文件 / `624` 项测试和 Rust `1512` 项库测试均通过。真人试用和反馈微调是本轮后续动作；压测、长时间 soak 和安装包资源画像不在本计划内，也不作为未完成事项跟踪。

## 0. 执行前置门和本轮审阅修正

本轮审阅把以下原先可由实现者自行解释的事项固定为计划约束：

1. **规范批准门**：P0 只能做基线、夹具和只读审计。开始任何生产 owner 改动前，必须将目标 proposal 复制/提升为已批准的 `docs/specs/` 规范，或在 `docs/README.md` 将其登记为当前规范，并保留批准记录。若批准门未满足，后续阶段只能在测试 harness 中实现，不得改变 active runtime、生产 observation、circuit 或 planner。
2. **迁移编号**：计划编写时最高迁移为 `0059`，当前工作区已有未提交的 `0060_routing_policy_v3.sql`（属于本任务的 P1 改动）。P0 必须核对该文件确实由本任务 owner 维护且 SQL/registry 一致；随后固定预留 `0061_routing_observation_v3.sql`、`0062_routing_key_circuit_v3.sql`、`0063_routing_runtime_generation.sql`。若 `0060` 不是本任务改动，或后续编号已被其他迁移占用，必须先整体顺延并同步本计划、测试和审计，禁止提交 `00xx` 占位文件或静默抢号。
3. **多代 registry 与唯一 active 指针**：`routing_runtime_generation` 是 P8 的必需持久化多代 registry；以 partial unique index/等价约束保证同一时刻最多一行 `status=active`，该行才是当前 active 指针。policy、quality、circuit 三者只有通过同一 active generation 指针才能被 planner 读取；不得把 registry 实现成只能保存一行的 singleton 表，也不得保留“复用现有机制或新增 singleton”的实现分支。
4. **请求尝试计数**：`outbound_attempt_count` 只统计跨 outbound boundary 的发送；`attempted_station_key_count` 只统计真正发出过请求的不同 Key。`maxRetryCount` 只限制首把 Key 外还能尝试的 Key 数量，同一 Key 的 raw retry 不消耗它；本地容量准入、Half-Open lease 竞争和快照重读两个计数都不增加。所有 hard cap、trace 和终态测试必须分别使用这两个定义。
5. **终态分类**：没有当前生命周期 Key 记录、存在 Key 但没有任何能力匹配、能力匹配但全部被用户资格/circuit/容量阻断，必须分别返回不同诊断；不得把“没有 Key”误报成模型不匹配。
6. **Half-Open 排序**：通过 score gate 的冷却结束 Key 与其所属同一 Primary/Backup/Emergency 硬层的 Closed 候选进入同一确定性排序，按 `effective_score DESC, station_key_id ASC` 竞争真实请求；score gate 只决定是否有资格进入序列，不隐式把 Half-Open 放到队尾，跨层分数不得比较。
7. **非重试错误术语**：`不重试该目标` 在本计划中明确表示终止当前请求，不自动换另一把 Key；只有矩阵明确标为可重试的错误才消耗 request retry budget。若将来要对凭据拒绝增加跨 Key 故障转移，必须另立策略字段和测试，不得在本次实现中自行扩展。
8. **事件时间缺失**：`event_at` 必须由产生 canonical outcome 的 outbound/monitor adapter 提供；缺失或非法时标记 `event_time_missing`/`event_time_invalid`，不得用 `observed_at` 或 `ingested_at` 补齐。该 outcome 仍可驱动 RealRequest 的 retry/circuit，但本版本不提供事后用写入时间补算的修复路径；修复 producer 后产生的新事件才可进入质量窗口或样本分母。`last_real_route_sample_at` 只接受有效 `event_at`，因此这类 Key 的 `idle_real_route_sample` 必须显示为 `unknown`，不能误判为 `true` 或 `false`。
9. **质量 cluster 完成条件**：`cluster_expected_attempt_count` 由 request/probe lifecycle owner 在终态提交时从 durable attempt ledger 写入，表示该 correlation 在候选准入后创建的全部 attempt slot 数量；容量准入拒绝和未创建 attempt slot 的本地规划循环不计入，已创建但在 outbound boundary 前取消/超时的 slot 必须写入 `local_abandoned` terminal outcome 并计入 expected count。必须存在 `0..expected-1` 的完整 terminal outcome 集合才能 finalized。未 finalized 的 cluster 只能保留 provisional 诊断和 pending 状态，不得进入质量分母、planner score 或 Half-Open score gate；projector 不得自行猜测数量、用超时终结或按当前已见行数终结。
10. **候选计数口径**：`candidate_cap_count` 是通过能力、已启用站点、有效凭据和当前 Key lifecycle 的候选数，用户禁用、circuit、request exclusion 和本地容量在 cap 统计后才应用。P6 的终态判断复用同一计数；不得另造含义相同的 `admission_candidate_count`。
11. **内部安全常量**：`MAX_OPERATIONAL_CANDIDATES=1024`、`QUALITY_PROJECTOR_BATCH_SIZE=256`、`MAX_PROJECTOR_BACKLOG=100_000`、`SYSTEM_RAW_ATTEMPT_HARD_CAP=40`、`SYSTEM_CUTOVER_FENCE_TIMEOUT_MS=30_000` 和 `HALF_OPEN_LEASE_REAPER_INTERVAL_MS=5_000` 均为代码常量，不进入 policy v3。Half-Open lease 的 `lease_expires_at` 取申请请求的 immutable `deadline_at`，不得另设用户可调 lease TTL。
12. **公共终态错误契约**：新增 `no_available_key` 作为 HTTP 503、`error.type=service_unavailable` 的公共错误码；容量终态继续使用现有公共 `route_capacity_exhausted`，内部以 `capacity_exhausted` 或 `capacity_state_unavailable` 诊断区分原因。`capacity_unavailable` 只允许作为内部分类/诊断，不得作为第二个公共错误码。
13. **迁移与代际身份**：当前工作区的 `0060_routing_policy_v3.sql` 仍是本任务草稿。P0 必须检查并在执行前补齐 `routing_policy_v3_migration_audit` 的幂等唯一键 `(scope, source_config_revision, target_policy_version)`（`source_config_revision` 明确表示被迁移的旧策略 revision），以及 staged policy 的不可变 `policy_generation_id`；若 `0060` 已在任何发布数据库执行且缺少这些字段，必须先预留新的 additive migration 并同步顺延 `0061`--`0063`，不得静默改写已发布 migration。`0061`/`0062` 必须为质量和 circuit generation 提供可解析的 metadata/checkpoint 身份，`0063` 的 pointer 不得指向只有可变 revision 的悬空对象。
14. **切换期准入**：P8 generation fence 期间，新请求不得取得新的 candidate admission。请求若尚未有 outbound attempt，则在 fence 完成前等待；等待超过自身 immutable `deadline_at` 时结束为现有 deadline/timeout 公共错误，并附 `routing_generation_transition` 内部诊断，不进入 retry loop，也不得伪装成 `no_available_key`。已取得 admission 的请求允许完成，不能被切换强行取消。
15. **质量数据保留**：原始 v3 observation 至少保留完整的 30 天历史窗口，并在仍被 active、building、ready 或已被选为回滚目标的 generation watermark 引用时继续保留；超出窗口且无 generation 引用后才可由单一 retention owner 批量清理，保留脱敏 audit/rollup。不能为了清理提前删除 projector 或回滚目标仍需要的原始事件。
16. **统一排序比较器**：硬层优先级先于评分；在同一硬层内，所有拥有有限 `effective_score` 的候选（无论质量来源是 observed、optimistic 还是因部分评分因子不可用产生的 fallback）进入同一个比较器，严格按 `effective_score DESC, station_key_id ASC` 排序。`score_status`/`quality_basis` 只能用于诊断，不能把 fallback 候选整体排到所有 scored 候选之后；只有完全没有可计算分数的候选才使用稳定 `station_key_id` 兜底排序。
17. **准入提交点和 Half-Open lease 边界**：`candidate_admitted` 只在本地容量 lease、circuit/Half-Open CAS 和 attempt slot 持久化均成功后产生；generation fence 只阻止尚未产生该事件的新准入。已提交准入的请求允许完成并持有其 Half-Open lease 到请求 `deadline_at`，不能被 fence 撤销；未提交准入的竞争/等待只释放临时资源。Half-Open lease 必须持久化 `attempt_id` 与 `boundary_crossed`（或等价可原子读取的状态），reaper 只能对已跨边界且超期的 lease 重新打开 circuit，不能凭 lease 存在本身猜测已出站。
18. **代际输出隔离**：`building`/`ready` generation 以及被选为回滚目标的 generation，其 quality summary、pending cluster、circuit rebuild state 必须按 `generation_id` 与 Key lifecycle 隔离，不能复用或覆盖 active 的可变 read-model 行。只有 active generation 的唯一 projector/reducer owner 可以接收增量事件；P8 只在事务中切换 pointer，不能以原地覆盖摘要或 circuit state 代替切换。

P0 完成时必须在实施记录中逐项勾选以上门槛，并记录批准的规范 revision、迁移编号占用检查结果和 active generation 设计评审结果。

本轮审阅已经收口的执行歧义如下：Half-Open 的 `best_closed_score` 只在该 Key 所属的同一 Primary/Backup/Emergency 硬层内比较；同一硬层内所有有限分数使用一个统一降序比较器；出站边界前的本地连接/适配器错误不触发跨 Key 故障转移；只有完成容量、circuit 和 attempt slot 持久化才算 candidate admission；已准入请求不因 generation fence 撤销 Half-Open lease，lease 的跨边界状态必须可持久化判断；质量、circuit、policy generation 都有不可变 ID 可追溯；P8 fence 的等待/超时行为已固定，超时不得伪装成 `no_available_key`；中转站编辑页的页面渲染、controller、测试和 API 调用均纳入清理范围。以下章节不得重新引入这些替代解释。

### 0.1 本轮计划审阅修正和执行产物

本轮审阅发现的缺口及修正固定如下，后续实现不得再自行解释：

1. **批准不是口头前置条件**：P0 必须将目标 proposal 提升为唯一批准入口
   `docs/specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md`，并在
   `docs/README.md` 登记其状态和路径；不得只在实施记录中口头批准或保留“复制/登记二选一”。
   P0 必须生成脱敏的实施记录
   `docs/audits/2026-08-29-intelligent-routing-scoring-circuit-redesign-implementation.md`。记录至少包含批准规范路径和内容 hash、批准时间、迁移编号占用扫描结果、当前 active generation 设计评审结论、基线命令及其退出码、P0--P9 每个完成门的状态。记录不得包含 key、URL、Authorization、原始响应或完整日志。没有该记录和批准的规范 revision，P1--P7 只能在测试 harness/shadow 中运行，不能改变生产读写路径。
2. **迁移 ID 不得依赖随机 SQL**：各类 generation ID 使用各自完整的稳定输入元组，不能共用一个会发生碰撞的短元组：
   - `policy_generation_id = H(policy_namespace, scope, source_policy_revision, target_policy_version, canonical_policy_hash, policy_algorithm_version)`；
   - `quality_generation_id = H(quality_namespace, scope, quality_policy_revision, evaluation_at_ms, input_observation_watermark, canonical_quality_input_hash, quality_algorithm_version)`；`quality_policy_revision` 只在 source weights、样本门槛、乐观值或 quality algorithm 改变时递增；
   - `circuit_generation_id = H(circuit_namespace, scope, circuit_policy_revision, input_circuit_event_watermark, canonical_circuit_input_hash, circuit_algorithm_version)`；`circuit_policy_revision` 只在连续失败/恢复阈值或等待时间改变时递增；
   - `runtime_generation_id = H(runtime_namespace, scope, policy_generation_id, quality_generation_id, circuit_generation_id, cutover_fence_revision)`。
   `H` 的具体编码、算法、前缀和 test vector 由第 0.2 节冻结；实施记录只需引用该规范并记录验证结果。禁止使用 `randomblob`、当前时间或自增 ID 作为幂等身份。重复执行同一重建或切换必须解析到同一个 generation ID；输入水位、算法版本或内容 hash 变化必须得到不同 ID。
3. **零 attempt cluster 不能靠空集通过**：`cluster_expected_attempt_count=0` 时必须走显式 `no_attempts` 终态分支；可以将 lifecycle 标记为已完成以便回收，但永远不产生质量样本、质量分母或 Half-Open gate 输入。finalizer 不得把空的 `0..-1` 集合当作普通 finalized cluster。
4. **崩溃恢复结果不可隐式重试**：普通 adapter 产生的 `upstream_uncertain` 可以依照 ReplayGate 重试；由崩溃恢复或 lease reaper 补写的未知结果必须带 `recovery_origin`，其 retry disposition 固定为 `StopRequest`，不能再次发送同一请求。`recovery_origin` 和 `retry_disposition` 是内部 observation/effect 元数据，不新增用户可调策略，也不改变公共错误码。
5. **所有 admission lease 都必须可回收**：Half-Open lease 的 reaper 不是唯一租约清理者。统一的 admission-lease supervisor 必须同时处理普通容量 lease 和 Half-Open lease 的进程崩溃、deadline、目标删除、取消和重复释放，且每个 `(attempt_id, lease_id)` 只能产生一次终态。
6. **P8 必须有明确 owner 和原子切换产物**：generation coordinator、registry store、rebuild runner、cutover fence 和 rollback report 必须各自有代码 owner 和测试 owner；P8 没有可复核的 comparison report、watermark、CAS 和 pointer 事务证据时不得激活 generation。

迁移的最低 schema postcondition 也在 P0 冻结，避免“表建出来了但运行时仍无法证明身份”：

- `0060`：`routing_policy_v3_staged` 每行有不可变、唯一的 `policy_generation_id`；`routing_policy_v3_migration_audit` 有唯一键 `(scope, source_config_revision, target_policy_version)`，并能通过该键解析到同一 staged generation；
- `0061`：v3 observation/attempt ledger 的 `event_id`、`attempt_id`、`correlation_id`、Key lifecycle revision、generation eligibility、cluster 状态和事件时间状态均为结构化列；物理幂等键和查询索引不能依赖 JSON 文本；
- `0062`：每个 `(station_key_id, station_key_lifecycle_revision)` 至多一行当前 circuit state；state 的 `state`、`opened_at`、`cooldown_until`、`lease_id`、`lease_revision`、`lease_expires_at`、`recovery_successes` 等列有固定 NULL/非 NULL 规则；circuit event 的 `(event_id, effect_kind)`、`(station_key_id, station_key_lifecycle_revision, reducer_commit_sequence)` 和同一 effect kind/lifecycle 下的 `attempt_id` 唯一；Half-Open lease 持久化 `attempt_id`、`lease_revision`、`deadline_at`、`boundary_crossed`、`released_at` 和终态枚举；reducer 的 monotonic clock watermark 也必须可持久化恢复，不能依赖进程内时间。
- `0063`：registry 支持多代状态和最多一行 `active` 的 partial unique 约束，policy/quality/circuit 三个 generation ID、输入 watermark、content hash、状态和 checkpoint 都能唯一解析；migration 不插入伪造 active row。

### 0.2 身份、哈希和水位的冻结格式

为避免不同实现者生成互不兼容的代际身份，以下格式不是“实现时再决定”的细节：

1. **Canonical JSON v1**：所有 `canonical_*_hash` 和 generation 输入/输出 hash 都使用 UTF-8、无 BOM、无空白的 RFC 8785/JCS 等价序列化；对象键按 Unicode 码点升序，数组保持语义顺序，字符串使用 JSON 标准转义，数字必须是有限值并使用最短十进制表示，`null`/布尔值按 JSON 字面量输出。禁止把 managed JSON envelope 的 `baseRevision` 或时间戳放入 `canonical_policy_hash`。若仓库没有现成 JCS 实现，新增唯一 `canonical_json_v1` helper，禁止在 SQL、前端或不同 owner 中复制实现。
2. **摘要算法**：`canonical_*_hash` 和 `content_hash` 均为 `sha256(canonical_bytes)` 的小写 64 位十六进制字符串；空数组的 hash 也必须按同一规则计算，不能使用 `NULL`、随机值或数据库 rowid。
3. **generation ID**：`H` 固定为 `sha256`，输出加固定前缀：`pg1_`、`qg1_`、`cg1_`、`rg1_`。预映像格式为：
   `routing-generation-id/v1` + U+001F + 每个字段的 `UTF8_BYTE_LENGTH:UTF8_BYTES`，字段之间用 U+001F 分隔；数字使用无前导零的十进制 ASCII（仅 `0` 例外），字段顺序严格按第 0 节第 2 条的元组顺序。不得使用当前时间、`randomblob`、自增 ID 或数据库返回顺序。
4. **generation 输入绑定**：`policy_generation_id` 的 `canonical_policy_hash` 必须是 staged 行 `config_json` 的 canonical `policy` payload hash；`quality_generation_id` 的 input hash 必须覆盖本 generation 纳入的去重样本身份和计算字段；`circuit_generation_id` 的 input hash 必须覆盖本 generation 重放的已应用 raw circuit event 身份和 reducer 字段。相应 hash、algorithm version 和 generation ID 必须同表持久化，读取时重新计算不一致即视为 generation 损坏，不能继续激活。
5. **固定 test vectors**（预映像中的 U+001F 以 `\\u001f` 表示；结果是完整 SHA-256 小写 hex，不含 ID 前缀）：

   | 类型 | 字段值（按顺序） | 期望摘要 |
   | --- | --- | --- |
   | policy | `routing-policy-v3`, `active`, `7`, `routing-policy-v3`, `sha256:abc`, `1` | `cbba4761e673913df381c1dae682a2e715cc8cf72e87d62b0977ad705dedd107` |
   | quality | `routing-quality-v3`, `active`, `7`, `1700000000000`, `42`, `sha256:def`, `1`（`7` 为 quality policy revision） | `d7764767fd7c11e9f57596ff686f4ac16b2b1394a86c99f643db67c40c65e624` |
   | circuit | `routing-circuit-v3`, `active`, `7`, `88`, `sha256:ghi`, `1`（`7` 为 circuit policy revision） | `d75adfebf787f65385edd629691bbbe3ca1d4e268c9b26e97fd8db6cd6528466` |

   contract test 必须断言完整 ID（例如 policy 为 `pg1_` 加第一行摘要），并断言同一输入重复计算相同、任一字段/算法版本变化得到不同 ID。
6. **事件水位**：所有 v3 observation 和 circuit event 在同一 SQLite 写事务中分配全局递增 `ingestion_sequence INTEGER`（由数据库 sequence/等价单一 owner 产生，不能使用 `event_at`）。`input_observation_watermark` 和 `input_circuit_event_watermark` 都是该 sequence 的已包含最大值，边界是 inclusive（`ingestion_sequence <= watermark`）；尾部只消费 `> watermark`。checkpoint 还保存稳定游标 `(station_key_id, observation_id)` 或 `(station_key_id, lifecycle_revision, reducer_commit_sequence, event_id)` 以保证批次内确定性。`event_at` 只用于质量窗口，绝不作为 projector/reducer 水位；同一事务分配的 sequence 必须在重放时保持不变。

### 0.3 冻结的 v3 schema contract

P1 开始前必须按下列实体名、关键列和约束实现 migration；不得让各阶段自行选择“原表扩展还是另建表”。列可以有仓库统一的 `created_at_ms/updated_at_ms`，但不能删掉这里的身份、状态或幂等列。

| migration | 必须创建/扩展的实体和关键约束 |
| --- | --- |
| `0060` | **只做结构 migration**：创建 `routing_policy_v3_staged`（`staged_id`、`scope`（`active\|history`）、`source_config_revision`、`target_policy_revision`、`target_policy_version`、`policy_generation_id`、`canonical_policy_hash`、`policy_algorithm_version`、`config_json`、`status`（`staged\|ready\|active\|retired\|failed`）、`failure_code`、时间列；`policy_generation_id` 唯一，且 `(scope, source_config_revision, target_policy_version)` 唯一）和 `routing_policy_v3_migration_audit`（`scope`、`source_config_revision`、`target_policy_revision`、`target_policy_version`、`policy_generation_id`、`migration_status`、来源/默认/丢弃字段 JSON、时间列；唯一键为 `(scope, source_config_revision, target_policy_version)`，`policy_generation_id` 外键直接关联 staged 行）。SQL 不读取/改写策略 JSON，不计算 hash，不插入伪造 staged/audit 数据；audit append-only，不得更新或物理删除；staged 行只能通过 owner 的 CAS 标记 `retired/failed`，不能删除。 |
| `0061` | 扩展现有 `routing_observations`，增加本计划第 6.4 节的结构化 v3 列和全局 `ingestion_sequence`；v3 写入要求非 NULL，legacy 行只允许 `generation_eligibility=Legacy`。新增 `routing_attempt_v3`（`attempt_id` 唯一、`correlation_id`、`station_key_id`、lifecycle revision、`attempt_index`、admission/boundary/terminal 字段）和 `routing_attempt_cluster_v3`（source/key/lifecycle/correlation 主键、`expected_attempt_count`、不可逆 finalized 状态及 reason）；唯一键为 `(source, station_key_id, station_key_lifecycle_revision, correlation_id, attempt_index)`。新增 `routing_quality_generation_v3` 与 checkpoint 表，保存不可变 `quality_generation_id`、`quality_policy_revision`、状态、input watermark/hash、output content hash、游标和 processed count。 |
| `0062` | 新增 `routing_circuit_state_v3`（主键 `(station_key_id, station_key_lifecycle_revision)`，每个当前 lifecycle 至多一行）、`routing_circuit_event_v3`（`event_id/effect_kind`、Key/lifecycle/`reducer_commit_sequence`、attempt 幂等唯一键）和 `routing_circuit_generation_v3`/checkpoint。`Closed` 时 opened/cooldown/lease 字段必须为 NULL 且 `recovery_successes=0`；`Open` 时 opened/cooldown 非 NULL 且 lease 字段为 NULL；`HalfOpen` 时 lease_id/revision/expires/deadline 非 NULL，`boundary_crossed`、released_at 和 terminal 状态遵循第 8 节规则；generation metadata 保存不可变 `circuit_generation_id`、`circuit_policy_revision`、状态、input watermark/hash、output content hash 和 checkpoint。 |
| `0063` | 新增多行 `routing_runtime_generation`（`runtime_generation_id` 唯一、三种 generation ID、总体 `policy_revision`、`quality_policy_revision`、`circuit_policy_revision`、两个 input watermark、三种 input hash、三种 output content hash、algorithm version、status、checkpoint 引用、cutover fence revision 和时间列），状态为 `building\|ready\|cutover_fencing\|active\|retired\|failed`，`status=active` 建 partial unique index；新增单行 `routing_runtime_cutover_marker`，状态为 `pre_cutover\|v3_active`。migration 只插入 `pre_cutover` marker，不插入伪造 active generation。 |

`routing_policy_v3_staged.scope` 是**源数据范围**而不是 active 指针；同一 scope 可以有多条不同 source revision。active 指针只能来自 `routing_runtime_generation`，不能从 staged 的 scope 或 status 推断。所有 v3 generation-scoped quality/circuit 输出必须带 generation ID，不能覆盖 active 之外的 read model。

`routing_policy_v3_staged.status` 的语义固定为：`staged` 表示 canonical payload 已写入但尚未完成代际重建，`ready` 表示 policy 自身和关联 quality/circuit generation 均已校验，`active` 只允许在 runtime pointer 事务成功后出现，`retired` 表示历史可回滚代际，`failed` 表示不可激活但保留诊断。只有 generation coordinator 可以改变 status；任何 payload/hash/revision 更新都必须创建新 staged 行。rollback 只改变 runtime pointer 和旧/新 generation status，不删除 staged 或 audit 行。

## 1. 交付目标

实现完成后，生产请求链路必须只有一套可解释的路由决策链：

```text
请求解析和硬资格过滤
  -> 同一硬资格层内按 effective_score 降序排序（无分数候选才按 station_key_id 稳定兜底）
  -> 按顺序申请本地容量准入并发送 outbound attempt
  -> 记录 canonical outcome
  -> 普通可重试错误在连续失败阈值内继续尝试当前 Key
  -> Key 连续失败达到阈值时持久化 Open/cooldown 并排除当前 Key
  -> 尚有额外 Key 名额时重新读取 PlanningSnapshot，尝试下一把未尝试 Key
  -> 冷却结束且评分高于同一硬层 Closed 最高候选时申请 Half-Open
  -> Half-Open 同一 Key 只允许一个真实请求
  -> 连续真实成功达到阈值后 Closed，否则按递增冷却重新 Open
```

必须同时交付：

1. `RoutingPolicy` v3 公开契约、v2 -> v3 迁移、回滚和字段级错误。
2. 真实路由/主动监控的统一观测、canonical failure、相关请求去重和可比性标记。
3. 明确定义的可靠性与响应时间质量投影：最近/历史窗口、`w(a)` 衰减、最小样本、乐观值、同时作用于可靠性和响应速度的 70/30 来源权重，以及 fixed-point 结果。
4. 仅以 `station_key_id` 为作用域的连续失败熔断状态机、持久化、CAS、lease 和迟到结果保护。
5. 严格评分降序 planner、请求级 retry budget、429 普通 Key 故障路径和终态错误契约。
6. 设置页分组重做、每个超时字段独立说明、容量域身份编辑区移除，以及必要的状态/诊断展示。
7. 旧 rendezvous/exploration、错误率保护、Beta prior、容量域生产读路径和多重 retry owner 的删除台账与验证证据。

## 2. 不可变约束

以下规则在所有阶段都必须成立，不能由实现便利或旧数据兼容绕过：

1. 路由熔断的唯一生产作用域是 `station_key_id`。账号、端点、模型、站点和容量域不得产生第二个生产 breaker。
2. `429` 是普通单 Key 可归责故障：写失败样本、连续失败加一；在 replay-safe 且未 commit 时，阈值内继续当前 Key，达到阈值 Open 后才消耗一个额外 Key 名额并按最新评分选下一把 Key。`Retry-After` 只能作为脱敏诊断，不能改变作用域、候选顺序、Key 数量预算或冷却算法。若保留诊断值，只允许保存 `retry_after_seconds_clamped`：解析失败、负数、非整数、溢出或超过 `86_400` 秒统一记为 `invalid`，合法值统一 clamp 到 `0..86_400`；不得保存原始 header、单位文本或上游 body，也不得把该值用于任何等待、cooldown、排序或请求预算。
3. 可靠性质量按 `source + station_key_id + correlation_id` 去重；连续失败熔断按每个可归责 outbound attempt 计数。两种计数不能混用。
4. 本请求只有在某个 `station_key_id` 达到阈值并 Open、或 durable circuit admission 已确认它不可用后才永久排除；阈值内必须允许重试当前 Key。排除后不得用 model/endpoint/routing identity 变体绕过。
5. 容量只保留本地运行时并发/资源硬门。容量域身份、同域排除、容量域等待、跨容量域 fallback 和容量域 UI 不属于生产路由。
6. 对至少有一个正配置权重的 eligible quality source 的 Key，无样本或样本不足不返回空评分，使用用户乐观值作为排序输入，但不得写入观测、失败计数或假成功；如果所有来源都不可比或其配置权重均为 0，则按目标规范返回 `quality_unavailable` 诊断。此时不得偷偷套用乐观可靠性：planner 使用“可用评分因子按其配置权重重新归一化”的确定性 fallback；若没有任何可用评分因子，则只按稳定 `station_key_id` 排序。该 fallback 不写入质量样本，也不改变 circuit。
7. `Closed`、`Open`、`HalfOpen` 的状态转换必须由一个 reducer owner 线性化提交；旧事件、重复事件和迟到结果只能审计，不能再次改变状态。
8. 每轮 planner 使用不可变 `PlanningSnapshot`。retry 只能刷新 snapshot，不能重置 deadline、retry budget、已尝试 Key 集合或 commit 状态。
9. 用户可调字段只来自目标规范；系统 hard cap、冷却递增级别和算法常量不能暴露成新的用户设置。
10. `ActiveProbe` 只作为可比质量/诊断来源，不直接改变 Key circuit 或请求 retry；Half-Open 恢复只接受带 lease revision 的真实路由请求。
11. 不提交或输出任何真实密钥、Authorization、完整 URL、请求体、上游原始响应或敏感诊断。

### 2.1 排序维度和模型 fallback 边界

模型映射的 `target_rank` 是评分之前的外层硬维度：`0` 为最高 rank，只有当前 rank 没有满足既有 `fallback_trigger` 的可执行候选时，才允许进入更低 rank。每个 rank 内再按硬层顺序 `Primary -> Backup -> Emergency`，同一硬层内才按 `effective_score DESC, station_key_id ASC, routing_identity ASC` 排序；`routing_identity` 只用于同一 Key 的 variant 最终打平，不能绕过请求级 `station_key_id` 排除。对应关系固定为现有 `AvailabilityTier::Primary`、`ConfiguredBackup`、`DepletedEmergency`，不得由实现者重新命名或把 `target_rank` 与 score 混合比较。无显式 model mapping 时 `target_rank=0`；本计划不根据评分或重试自动改写调用方请求的模型，Luna 等模型只能来自调用方或已配置的 mapping。

会话亲和仍沿用 [`INTELLIGENT_ROUTING_ENGINE_SPEC.md`](../specs/INTELLIGENT_ROUTING_ENGINE_SPEC.md) 第 23 节的既有 bonus 上限、hysteresis margin、TTL 和逃逸条件。P0 必须从当前代码与该节规范提取实际数值/公式，生成不含敏感数据的 affinity golden vectors；P5 只能复用该唯一实现并以 vectors 验收，不得用“现有语义”新增另一套算法，也不得把亲和降级为仅同分 tie-break。若当前代码与规范冲突，P0 标记为阻塞并在 P5 前完成一次明确的 owner 决策，不能由 planner 实现临时选择。

## 3. 实施策略和依赖关系

### 3.1 阶段顺序

```text
P0 基线、契约冻结和测试夹具
  -> P1 Policy v3 / migration / generated contract
  -> P2 canonical outcome / observation / dedupe
  -> P3 quality projector v3
  -> P4 station_key circuit reducer / durable lease
  -> P5 deterministic planner / admission / capacity-domain removal
  -> P6 retry and proxy execution integration
  -> P7 frontend settings / station editor / diagnostics
  -> P8 shadow comparison / atomic cutover / rebuild
  -> P9 legacy cleanup / full verification / handoff
```

P2 必须先于 P3、P4；P3 和 P4 完成后才能切换 planner；P1 的 DTO 和 migration 必须先于前端及所有生成绑定。P8 是唯一允许生产读路径进行短期双读/影子比较的阶段；P2-P4 在此之前若需写 shadow evidence，必须保持只写、不可被旧或新生产 planner 读取。P9 必须删除旧生产消费者，不保留第二套路由路径。

P1-P7 是同一版本内的实现和测试阶段，不发布任何“半旧半新”的中间生产状态。P1 的 data-stage 可以把每个 v2 source policy/history row 转换后写入 v3 `staged`/migration audit，但 runtime active pointer 必须继续指向旧完整 generation；`routing_policy_history` 仍是旧代审计/回滚输入，不承担 v3 active pointer 或 v3 quality/circuit 输出。若现有策略表不能表达 staged 状态，则把策略 JSON data-stage 延后到 P8 的切换事务。直到 P8 的 active generation/cutover pointer 原子切换前，旧 active 路由结果仍是唯一生产事实；v3 结果只能在测试或 P8 的短期 shadow 中读取。P8 切换后不得继续双写两个生产 planner、retry loop 或 circuit reducer。

P2-P4 若需要在 P8 前采集 v3 观测或演练 reducer，必须写入独立的 v3 shadow generation，或携带 `algorithm_version=routing_v3` 并让所有旧 production consumer 明确过滤；v3 事件不得改变旧 active 的 quality/circuit 读模型。若无法证明这种读隔离，相关写入只能在测试 harness 中启用，并延后到 P8 fence 后再接入生产。

### 3.2 每个任务的完成格式

每个任务提交前必须记录：

- 变更文件和 owner；
- 先失败的 RED 测试或基线证据；
- GREEN 命令、退出码和关键断言；
- 数据迁移前后样本/状态对比；
- 未运行的检查和原因；
- 兼容残留、删除台账条目和回滚方式。

禁止只写“已接入”而没有可观察的输入、输出、状态转换和测试证据。

### 3.3 可并行边界

默认按依赖顺序串行推进；如果多人同时实现，只允许在边界清晰的目录并行：

- P1 policy/domain 与 P2 classifier/observation 可以在接口草案冻结后并行，但不得各自定义一套 outcome、source weight 或 revision 字段；
- P3 quality、P4 circuit 必须共享 P2 的 observation contract，提交前先合并 contract tests；
- P7 前端只能在 v3 DTO 字段冻结后开始，不能自行发明字段名或默认值；
- P5 planner 与 P6 execution 必须串行切换 retry/action owner，避免短期出现两个 request loop；
- P8/P9 不得与任何生产 owner 重构并行，必须在所有专项测试通过后执行。

并行任务不得同时修改同一 migration 编号、generated 文件或 policy schema；由负责该 owner 的任务统一生成和验证。

## 4. P0：基线、契约冻结和夹具

### 4.1 目标

在修改生产 owner 前固定当前事实，确认工作区已有改动，建立后续所有阶段共用的测试数据和日志断言。P0 不改变生产行为。

### 4.2 步骤

1. 查看 `git status --short`，记录并保留用户已有改动；不得 reset、checkout、clean 或删除 `.tmp-deadcode-target/` 等未知产物。
2. 从 `docs/README.md` 确认当前规范入口；本任务已完成批准门，当前事实以 `docs/specs/INTELLIGENT_ROUTING_SCORING_CIRCUIT_REDESIGN_SPEC.md`、代码和自动化契约共同决定。
3. 建立只含假值的 `routing_v3` 测试 fixtures：
   - 至少 4 个 `station_key_id`，分布在 Primary/Backup 层；
   - 评分相同和评分相近的候选；
   - 一个高分但容量不足候选；
   - Closed、Open 未到期、Open 已到期、Half-Open lease in flight；
   - 真实路由和 ActiveProbe 两类来源；
   - 0 样本、低于/恰好达到最近与历史门槛、24 小时边界样本、超过 30 天样本和缺失/非法 `event_at`；
   - 同一 correlation 的多 attempt、不同 Key 的同一 correlation，以及重复/乱序 observation；
   - `cluster_expected_attempt_count=0` 的容量全拒绝请求、boundary 前崩溃和 boundary 后崩溃恢复；
   - 502、429、5xx、timeout、连接失败、401/403、400/422、模型不支持、本地容量拒绝、取消和流中断结果。
4. 记录当前失败基线：
   - `GenericStatus`/502 是否为 Neutral 且不进入样本；
   - Beta prior 和现有 recent/history 权重输出；
   - rendezvous/exploration 是否改变同一候选集合的第一选择；
   - error-rate protection 默认关闭时是否持续打到失败 Key；
   - capacity-domain exclusion/wait/fallback 当前是否会被 planner 或 execution 读取。
   - 现有 load/runtime-anomaly penalty 是否进入 dispatch utility；记录其 owner，目标只保留本地容量 hard gate。
5. 把 fixture 的关键 ID、时间戳和结果写入测试辅助函数，不把假 key 放入日志或共享 fixture 文件。
6. 检查迁移目录和 schema registry，确认 `0060` 是本任务的 P1 migration，且 `0061` 到 `0063` 未被其他任务占用，并在实施记录中登记结果；若 owner 或编号不符，先完成编号顺延再进入 P1/P2/P4/P8。`0060` 是否“已发布”不能以本地文件或工作区状态判断，必须同时检查：当前 release artifact/manifest 中是否包含该文件、数据库 migration journal/`_sqlx_migrations` 是否有该版本及 checksum、以及 schema15 升级 fixture 是否以该版本为 latest。任一受信来源显示已执行，就按已发布处理并只允许新增 additive migration；checksum 不一致必须阻塞，不能通过更新 manifest 掩盖。
   - 已对 `0060`--`0070` 逐项核对 postcondition（SQL 结构、audit/staged 唯一键、generation 字段、checkpoint、qualification report、恢复资格、circuit persistence gate 和 applied-event 标记）；当前工作区未发现编号冲突，P1/P8 运行时已按这些字段读写。任何已发布数据库若 checksum 或字段不一致，仍必须走 additive migration，不能改写已发布文件。
7. 建立基线断言的“当前行为”和“目标行为”两套名称。P0 的测试只能证明当前行为（例如 502/429 是否仍为 `Neutral`、样本是否为 0、是否没有 Key 级 Open），不得把目标 circuit 状态写成基线通过条件。

### 4.3 P0 验收

- 可以用单个测试复现 TNTAPI/tkapi 类似的连续 502/429 且样本为 0 的旧问题，并明确标注这是旧行为基线。
- 可以分别断言 raw attempt、旧实现的质量样本结果和旧实现的健康状态；目标的去重样本、连续失败计数和最终 circuit 状态放在 P2-P4 的 RED/GREEN 测试中，不在 P0 假定已存在。
- 所有新增 fixture 只使用 `test-key-*`、`test-station-*` 等明显假值。

### 4.4 最小验证

```powershell
git status --short
rg -n "weighted_rendezvous|ExplorationBudgetRegistry|BetaPrior|GenericStatus|HealthEffect::Neutral|capacity_domain|ProtectionProfileConfigV2" src-tauri/src src/features
git diff --check
```

## 5. P1：Policy v3、校验和迁移

### 5.1 目标

把用户可见策略收敛成目标规范中的 v3，不让旧字段继续进入运行时 planner、quality 或 circuit。迁移必须可重放、可审计、可回滚，且不改变当前 policy revision 的“是否用户编辑”语义。

### 5.2 代码范围

- `src-tauri/src/models/routing_policy.rs`
- `src-tauri/src/application/routing_policy.rs`
- `src-tauri/src/persistence/stores/routing_policy_store.rs`
- `src-tauri/src/application/routing_policy_control_plane.rs`
- `src-tauri/src/models/routing_read_models.rs`
- `src/lib/types/routing.ts`
- `src/lib/api/routing.ts`
- `src/lib/queries/routingQueries.ts`
- `src-tauri/src/application/request_finalization/failure.rs`、`src-tauri/src/services/proxy/error.rs`（新增 `no_available_key` 公共错误映射，保留 `route_capacity_exhausted`）
- `src-tauri/src/application/routing_engine/routing_failure.rs`、`src-tauri/src/ipc/dto/routing_mutations.rs`、`src-tauri/src/ipc/dto/routing_health_reads.rs`、`src-tauri/src/test_support/contract_scenarios.rs`（同步 typed error/IPC/contract 映射）
- `src-tauri/src/persistence/migrations/0060_routing_policy_v3.sql`（编号由 P0 预留；占用时按第 0 节整体顺延）

### 5.3 v3 公开字段

保持 managed JSON envelope：`formatVersion`、`baseRevision`、`policy`。`policy.version=3`，字段必须与目标规范示例一致：

```text
reliabilityWeight
responsivenessWeight
costWeight
preferenceWeight
allowDepletedFallback
affinityEnabled
affinityTtlSeconds
maxRateMultiplier
routingGroupFilter
outboundProxyMode
outboundProxyUrl
reliabilitySourceWeights.realTrafficPercent
reliabilitySourceWeights.monitoringPercent
reliabilitySampling.historicalMinimumSamples
reliabilitySampling.recentMinimumSamples
reliabilitySampling.optimisticReliabilityPercent
reliabilitySampling.optimisticLatencyMs
retry.version
retry.maxRetryCount
retry.consecutiveFailureThreshold
circuitBreaker.version
circuitBreaker.recoverySuccessThreshold
circuitBreaker.recoveryWaitSeconds
timeoutPolicy.version
timeoutPolicy.connectSeconds
timeoutPolicy.firstByteSeconds
timeoutPolicy.precommitSeconds
timeoutPolicy.bufferedExecutionSeconds
timeoutPolicy.streamIdleSeconds
```

锁定默认值和范围：

| 字段 | 默认值 | 范围/约束 |
| --- | ---: | --- |
| `reliabilitySourceWeights.realTrafficPercent` | 70 | 整数 `0..100`，与 monitoring 和为 100 |
| `reliabilitySourceWeights.monitoringPercent` | 30 | 整数 `0..100`，与 realTraffic 和为 100 |
| `historicalMinimumSamples` | 15 | `1..10_000` |
| `recentMinimumSamples` | 5 | `1..10_000` |
| `optimisticReliabilityPercent` | 95 | `0..100` |
| `optimisticLatencyMs` | 2,500 | `100..120_000` |
| `retry.version` | 1 | 固定为 `1` |
| `maxRetryCount` | 3 | `0..3`；首把 Key 外最多再尝试的不同 Key 数量 |
| `consecutiveFailureThreshold` | 3 | `1..10` |
| `circuitBreaker.version` | 1 | 固定为 `1` |
| `recoverySuccessThreshold` | 2 | `1..16` |
| `recoveryWaitSeconds` | 30 | `5..3600` |
| `timeoutPolicy.version` | 2 | 固定为现有 `2` |

timeout 字段沿用现有秒单位和约束。评分四项权重和仍必须为 10,000 basis points。

**Revision 语义固定如下：**

- 现有 `routing_policy.config_revision` 和 `routing_policy_history.config_revision` 在迁移输入中统一称为 `source_config_revision`；它表示旧策略/历史行已经提交的 domain revision，不能被重命名后重新解释。
- `routing_policy_v3_staged.target_policy_revision` 是 staged canonical payload 对外可见的 v3 policy revision。`0060` 是数据格式迁移而不是用户编辑，因此每个成功转换的行 `target_policy_revision = source_config_revision`，不递增 `domain_revisions`，也不制造一条“用户修改”的 history 事件。
- P8 之后的用户保存才通过 active generation 的 policy revision 做 CAS，并只递增一次 `target_policy_revision`；该新值写入 staged、runtime registry 和现有 `routing_policy_history` 的一条 v3 history/audit 记录（如旧表不能表达 generation ID，则新增 append-only v3 history 表，不能把 v3 行塞进 V2 schema）。迁移 audit 必须同时记录 source/target 两个值，不能只保留一个含义不稳定的 `config_revision`。
- 同一 canonical payload 重放时 source/target revision、hash 和 `policy_generation_id` 必须完全相同；canonical 内容变化必须生成新的 target revision 和新的 generation，即使旧的 source revision 相同。

### 5.4 实施步骤

1. 新增明确的 `RoutingPolicyConfigV3`、`RetryPolicyV3`、`CircuitBreakerPolicyV3`、`ReliabilitySamplingPolicyV3` 和 `ReliabilitySourceWeightsV3` 类型；不要继续扩展 V2 类型来同时表示新旧语义。
2. 为 v3 编译产出一个唯一 `CompiledRoutingPolicyV3`，其中：
   - retry 只提供 `max_retry_count` 和 `consecutive_failure_threshold`；
   - circuit 只提供恢复阈值、基础等待和系统内部 hard cap；
   - quality 只提供 source weights、窗口门槛、乐观值和算法版本；
   - 不含 `maxCandidates`、`explorationShareBasisPoints`、旧 `protectionProfile` 或容量域开关。
3. 新 decoder 使用 `deny_unknown_fields`；缺字段、未知字段、重复 key（raw JSON/file decoder）和错误类型都返回字段级错误。IPC 若仍接收已解析 `Value`，必须在契约中明确它不能检测原始重复 key，不能伪称具备该保证。
4. 新增 v2 -> v3 upgrader：
   - `maxTotalAttempts` 必须先通过旧版本范围校验，再转成 `maxRetryCount=maxTotalAttempts-1`；这是从 raw attempt 数到不同 Key 数的有意语义迁移，必须写入 audit；超出旧范围的坏数据保持 invalid，不静默 clamp；
   - 旧容量 retry/wait/cross-domain 字段写入 migration audit，不进入 v3；
   - 旧 exploration/maxCandidates 丢弃用户语义；运行时改用代码拥有的 `MAX_OPERATIONAL_CANDIDATES`（当前值为 1024，实施时必须从代码确认）作为候选硬上限，不把旧的用户值复制到 v3 或任何隐藏用户设置；旧值仅写入 audit；
   - `protectionProfile.enabled` 忽略，熔断器固定启用；旧错误率阈值不等价转换，使用默认连续失败阈值并记录语义变化；
   - `halfOpenSuccessesToClose` 转为 `recoverySuccessThreshold`；
   - prior alpha/beta 不进入新 profile；
   - 缺失的新字段使用 v3 默认值。
5. 迁移在 SQLite 中采用 additive 方式，并拆成两个有 journal 的 durable step，避免在 SQL 中复制 JSON canonicalization：
   - `0060_routing_policy_v3.sql` 只创建第 0.3 节的表、约束、索引和 schema postcondition；不得在 SQL 中读取/转换策略 JSON、调用 `json_extract` 组装 v3 payload、计算 hash 或写 staged/audit 行。若当前未发布草稿包含这些 INSERT/转换语句，发布前必须移到下一个 data-stage owner 并由 P0 记录。
   - `RoutingPolicyV3StageUpgrade` 由 `startup_upgrade_plan` 先调用纯 v2 -> v3 upgrader，对 `routing_policy` 当前行和 `routing_policy_history` 的每一行做 decode、字段校验、canonicalization、hash 和 ID 预计算；只有所有源行都成功时才执行该 data step。任何一行失败都在 data step 前返回 typed `routing_policy_migration_invalid` recovery（包含脱敏 scope、source revision、字段路径和错误码），数据库中的源 schema、active policy、history 和 domain revision 均保持原样，且不得创建部分 staged 行；不能静默跳过 history 或以默认值掩盖坏数据。修复源数据后可从同一 revision 重试。
   - 遵循仓库 schema-upgrade 的 preflight/单实例 migration lock 和 journal/备份约束；`0060` 结构 step 与 `RoutingPolicyV3StageUpgrade` data step 都必须可重放。结构 step 成功但 data step 失败时，允许 schema 保持在 60，journal 标记 `staging_pending`，旧 runtime 继续作为唯一生产事实；重试只执行 data step，不重复结构 migration。
   - data step 在单一 SQLite transaction 中写入 `routing_policy_v3_staged`（active/history source scope）及其 migration audit，并记录不可变 `policy_generation_id`；transaction 失败全部回滚，不能在 P8 前改变 runtime active pointer。不得再增加第二种“无 staged 直接改 active”的兼容分支；若后续实现采用等价命名，必须在 contract 中声明唯一 staging owner 和唯一键。
   - `routing_policy_v3_staged.config_json` 明确存 canonical `policy` payload；managed JSON 的 `formatVersion/baseRevision/policy` envelope 只在 document/control-plane 边界组装和校验，不能让 migration 同时产生两种不同的 payload 语义。写入前必须校验 `canonical_policy_hash == sha256(canonical_json_v1(config_json))`，写入后禁止修改 payload、hash、版本或 source/target revision。
   - `0060` 不递增 domain revision；写入 `source_config_revision`、`target_policy_revision`（两者相等）、迁移来源（migration 而非 user）、丢弃字段、默认字段和质量重建要求的 audit。
   - data step 提交后校验所有 staged JSON 可解码、policy/history source 行数与 staged 行数一一对应、revision/hash/ID 链连续；任一 postcondition 失败则回滚 data transaction，保留旧 active 和可重试的 `staging_pending` journal，不得把半成品标记为 ready。audit 与 staged 写入必须以 `(scope, source_config_revision, target_policy_version)` 和不可变 `policy_generation_id` 唯一键幂等，重复执行不得产生重复审计或重复 staged generation。
6. 编写 rollback：在 P8 前只有 `failed` staged generation 可标记为失败，不能物理删除；旧 active revision 保持不变。这不是新增 runtime fallback。迁移审计保留足够信息恢复上一个完整 JSON。P8 激活后不得把 rollback 实现成回到 V2 运行时 planner。
7. 更新 generated Rust/TypeScript bindings、DemoBackend、DesktopBackend、API/query 类型和 contract fixture。生成物必须由仓库脚本生成。

### 5.5 测试

- v1、v2、v2 缺字段、v3 完整、v3 缺字段、unknown field、错误类型、超范围、权重和不为 100、重复 key raw JSON。
- 迁移后 policy JSON、history、revision、audit 和 active status。
- P1 完成后 runtime active pointer 仍指向旧完整 generation；staged v3 与旧 active 的 revision/hash 关系可被查询，不能出现旧 planner 直接读取半成品 v3。
- 旧策略无法迁移时 active revision 不变。
- 新 decoder 不再接受 production `maxCandidates`、exploration、错误率保护和容量用户字段。
- v3 每个字段的后端错误路径与前端字段路径一致。
- `no_available_key` 的 Rust/IPC/HTTP 映射、错误 body 和脱敏诊断；`route_capacity_exhausted` 兼容映射及 `capacity_exhausted`/`capacity_state_unavailable` 内部诊断。

### 5.6 P1 完成门

v3 decoder/compiler/upgrader、字段校验、migration audit、rollback 和 generated contract 已通过；新的 v3 代码路径不得读取 V2 字段。旧 V2 production consumer 在 P1 后可以暂时存在，但必须登记在 P9 删除台账中，且在 P8 切换后不得再接收生产请求。`pnpm.cmd test:contracts` 和 `pnpm.cmd generate:bindings --check` 必须通过。

### 5.7 generated contract 和切换边界

为避免 UI、IPC 和 runtime 各自理解一个“v3”，版本切换按以下矩阵执行：

| 阶段 | `routing_policy.read/write` 公共命令 | v3 类型用途 | 生产读取者 |
| --- | --- | --- | --- |
| P1-P6 | 继续接受/返回现有 V2 document 作为兼容输入；同一命令不得把 V3 写入旧 active 行 | `RoutingPolicyDocumentV3`、`CompiledRoutingPolicyV3` 只用于 decoder/upgrader、staged control-plane 和 contract fixture | 旧 V2 active generation |
| P7 至 P8 fence 前 | 同一 control-plane owner 的版本化 union 可读写 V3 staged，并返回 `staged/ready/failed` 状态；V2 仍只用于兼容读取/迁移，任何写入都不得改旧 active 行，也不得宣称已生效 | V3 document/compiled DTO 与 staged generation 一一对应 | 旧 V2 active generation |
| P8 成功后 | 同一命令只返回/接受 V3 document；V2 仅保留 migration fixture/历史反序列化，不再接受用户写入 | V3 DTO 为唯一生产契约，携带 `policy_generation_id`、`target_policy_revision` 和 CAS base revision | `status=active` 的完整 v3 generation |

不新增第二个独立的“旧设置 API”和“新设置 API” owner；若生成器需要区分类型，使用同一 command descriptor 下的版本化 union。生成物至少包括 Rust DTO、TypeScript DTO、command descriptor、错误枚举和 contract hash；P0/P1 实施记录必须记录生成脚本、输入 descriptor 的 SHA-256 和 `generate:bindings --check` 结果。任何读取 staged 的诊断接口都必须显式带 generation ID，不能让旧 status/query 默认把 staged 当 active。

## 6. P2：Canonical outcome、观测和去重

### 6.1 目标

使真实 attempt 和主动监控都产生统一、可追踪且不暴露敏感数据的观测。任何已经跨 outbound boundary 且可归责给选中 Key 的结果都不能再被 `GenericStatus -> Neutral` 吞掉。

### 6.2 代码范围

- `src-tauri/src/application/request_finalization/failure.rs`
- `src-tauri/src/application/request_finalization/mod.rs`
- `src-tauri/src/application/request_finalization/outcome.rs`
- `src-tauri/src/application/request_finalization/effect_planner.rs`
- `src-tauri/src/application/request_lifecycle/*`、`src-tauri/src/services/proxy/lifecycle/*`（attempt slot、boundary 标记和终态 recovery owner）
- `src-tauri/src/services/proxy/adapters/*`
- `src-tauri/src/models/routing_observation.rs`
- `src-tauri/src/application/monitoring/write_path.rs`
- `src-tauri/src/persistence/stores/routing_observation_store.rs`
- `src-tauri/src/persistence/stores/health_observation_store.rs`
- `src-tauri/src/persistence/migrations/0061_routing_observation_v3.sql`（编号由 P0 预留；占用时按第 0 节整体顺延）

`0061` 只做 additive schema 变更：保留旧 observation 原文和主键，新增 v3 identity/outcome 字段、物理唯一键和查询索引；无法证明 correlation/lifecycle 的历史行只标记 `legacy_unclustered`，不在迁移中猜测或合并。迁移必须有 schema postcondition、重复运行和旧数据库备份恢复测试。

### 6.3 Canonical outcome 矩阵

在 classifier 中只保留一个状态到效果的映射 owner。至少实现以下矩阵：

| 结果 | `failure_attribution` | 可靠性样本 | circuit 连续失败 | 普通 retry |
| --- | --- | --- | --- | --- |
| 上游成功并完成 | Key/Success | 成功 | Closed 清零；Half-Open 成功加一 | 结束 |
| 上游 `429`/rate limit | Key | 失败 | 加一 | replay-safe 且未 commit 时先重试当前 Key；Open 后才换 Key |
| 上游 5xx（含 502） | Key | 失败 | 加一 | replay-safe 且未 commit 时先重试当前 Key；Open 后才换 Key |
| 已跨边界但上游语义未知（adapter 正常返回） | Key（细节 Unknown） | 失败 | 加一 | 按 `RetryableBeforeCommit` 和 ReplayGate 处理；启动恢复或 lease reaper 补写时为 `StopRequest`，不得重放 |
| 连接失败/DNS/首字节超时 | outbound boundary 后为 Key；边界前为 Local/Unknown | 已确认到达/由选中目标返回时失败 | 同左；边界前不加 | 已跨边界且 replay-safe、未 commit 时重试 |
| 401/403 凭据拒绝 | Key | 失败 | 加一并记录 credential diagnostic | 不重试该目标 |
| 模型不支持/能力不匹配 | Capability | 不计 Key 可靠性 | 不计 | 不重试该目标 |
| 客户端 400/422 | Client | 不计 | 不计 | 不重试 |
| 下游取消、发送前 deadline | Client/Local | 不计 | 不计 | 结束 |
| 本地错误且未证明出站 | Local | 不计 | 不计 | 按本地终态规则结束 |
| 本地容量准入拒绝 | Local | 不计 | 不计 | 尝试下一 Key，不消耗 outbound retry |
| 已提交后的流中断 | Key/Unknown | 失败 | 加一 | 不重放已提交请求 |

`source=RealRequest` 的 `429` 必须与 5xx 使用相同的 `RetryDisposition`、observation、circuit 和 request-exclusion 路径；`Retry-After` 不产生额外等待或第二种 classifier 分支。`source=ActiveProbe` 的 429 仅生成质量/诊断 observation，遵循后文的 source 过滤。

出站边界前由 Relay 自己产生的连接、DNS、适配器或本地序列化错误，按 `Local/Unknown` 终态处理：不计 Key 质量、不计 circuit、不得自动换另一把 Key，也不消耗 outbound retry。只有有证据证明请求已跨过 outbound boundary 的连接/传输失败，才进入上表的 Key 失败和普通重试路径。`Retry-After` 如需保留，只允许写入经过范围限制的数值诊断（不得保存原始 header），且不改变候选、预算或冷却。

表中的“上游 5xx/429”仅指已确认由选中目标返回，或传输证据证明请求已跨 outbound boundary；Relay 自己在发送前生成的 502/错误仍归类为 Local，不得污染 Key 可靠性或 circuit。

矩阵中的 retry/circuit 列默认针对 `source=RealRequest`。`source=ActiveProbe` 只写质量/诊断观测，不触发 request retry、不加入 request exclusion，也不直接改变 Key circuit streak；Half-Open 的恢复成功必须是带 lease revision 的 `RealRequest`。监控来源的 70/30 权重只参与可靠性和响应速度的最终质量混合，不会因为监控模板差异把真实路由熔断或恢复。

### 6.4 观测字段和身份

补齐或等价表达以下字段：

```text
observation_id
event_id / attempt_id
correlation_id
attempt_index
candidate_admitted / candidate_admitted_at
capacity_lease_id / half_open_lease_id
boundary_crossed
response_origin: Upstream | Relay | Unknown
event_time_status: Valid | Missing | Invalid
cluster_finalized
cluster_expected_attempt_count
cluster_finalized_at
cluster_finalization_reason
station_key_id (nullable only for Administrative)
station_key_lifecycle_revision
source: RealRequest | ActiveProbe | Administrative
model_class / endpoint_shape / protocol / request_shape
outcome: Success | AttributableFailure | Excluded
failure_code
failure_attribution: Key | Local | Client | Unknown
latency_ms / ttft_ms
 event_at / observed_at / ingested_at
 comparability_key
 recovery_origin: Normal | CrashRecovery | LeaseReaper
 retry_disposition: End | RetryableBeforeCommit | StopRequest
 algorithm_version
source_weight_revision
quality_policy_revision
generation_eligibility: Active | Next | Legacy
```

`generation_eligibility` 是切换期的摄取标记，不是路由或质量算法字段：正常写入当前 active generation 的事件标记为 `Active`；v3 尚无 active generation 的 shadow/building 阶段，或 generation fence 冻结下一代输入水位后产生、只能留给下一代重建的事件标记为 `Next`；迁移来的旧事件标记为 `Legacy`，不得进入当前 v3 质量分母。`Next` 事件可以写入 immutable observation，但不得被当前 active projector/reducer 消费；下一代建立输入水位时才能纳入。旧的 endpoint/account/model 归因字段可以留在审计兼容层，但 planner/circuit 只消费 `station_key_id`。不要把完整 endpoint、请求体或认证信息写入 evidence JSON。

观测字段的 source-specific 约束固定为：`RealRequest`/`ActiveProbe` 必须有 `station_key_id`、lifecycle revision、correlation 和 attempt identity；`Administrative` 的 `station_key_id`、lifecycle revision、correlation、attempt index、boundary 和 lease 字段必须为 NULL，`outcome=Excluded`、`failure_attribution=Local`，且永远不进入质量或 circuit。`local_abandoned` 不是新的质量结果枚举，而是 `outcome=Excluded`、`failure_attribution=Local`、`response_origin=Relay`、`boundary_crossed=false` 的固定组合；`upstream_uncertain` 是 `outcome=AttributableFailure`、`failure_attribution=Key`、`response_origin=Unknown`、`boundary_crossed=true` 的固定组合。其他 outcome/failure_attribution/response_origin/boundary 组合由 schema/check constraint 拒绝，不能靠 JSON 约定。

### 6.5 写入与去重步骤

1. 在请求进入路由时生成不可重复的 request `correlation_id`；候选完成本地容量与 circuit admission、即将跨 outbound boundary 时，原子创建一个 attempt slot，从 `attempt_index=0` 开始递增并生成不可重复的 `attempt_id/event_id`。容量拒绝、Half-Open CAS 竞争失败和未完成 admission 不创建 slot；重试沿用 correlation 但绝不复用 attempt index 或 attempt ID。普通容量 registry 是进程内 owner，`CapacityLease`/`CapacityWaitPermit` 采用幂等 RAII 释放；Half-Open lease、attempt/circuit 是 SQLite owner。SQLite 失败或进程崩溃时，持久化 Half-Open lease 由 reaper 按 `lease_expires_at_ms <= now_ms` 补偿；普通容量计数器随进程 registry 一并丢弃，不在数据库留下悬挂占用。任何未观察到 `candidate_admitted` durable commit 的执行路径都不得发送 outbound。
2. classifier 根据传输边界和上游 evidence 生成 canonical outcome；禁止 observation writer 重新猜 HTTP 状态。`event_at` 是该 canonical outcome 发生的 UTC 时间（由 outbound/monitor adapter 提供），`observed_at` 是观测写入前的业务观测时间，`ingested_at` 是持久化接收时间；质量窗口只使用 `event_at`，projector/reducer watermark 只使用同一事务分配的全局 `ingestion_sequence`。adapter 无法提供有效 `event_at` 时必须写 `event_time_status=Missing|Invalid`，绝不能以 `observed_at` 或 `ingested_at` 代替；该 outcome 仍可用于 retry/circuit，但本版本不进入质量窗口或样本分母，且不提供事后用写入时间补算的修复路径。
3. observation store 以 `observation_id`/`event_id` 幂等写入，并保留不可变的全局 `ingestion_sequence`；不得另造只在某个 producer 进程内单调的 sequence 作为 generation watermark。
 4. 在限定当前 `station_key_lifecycle_revision` 后，质量去重键固定为 `source + station_key_id + correlation_id`；生命周期 revision 必须由当前 Key binding snapshot 注入，绝不能从 observation 的最大 revision 推断；旧生命周期只保留审计，不得与新生命周期合并。每个 correlation cluster 必须有单调不可逆的 `cluster_finalized` 标记、`cluster_expected_attempt_count`、`cluster_finalized_at` 和 `cluster_finalization_reason`：request/probe lifecycle owner 在 durable lifecycle 提交终态时，从 attempt ledger 写入该 cluster 在候选准入后创建的全部 attempt slot 数量；容量准入拒绝和未创建 attempt slot 的本地规划循环不计入，已创建但在 outbound boundary 前取消/超时的 slot 必须写入 `local_abandoned` terminal outcome 并计入 expected count。`(source, correlation_id, attempt_index)` 必须唯一，attempt index 从 0 连续递增且不得为负；expected count 只能由 lifecycle owner 在终态 CAS 中一次写入，之后不可减少或修改，且不得超过代码常量 `MAX_ATTEMPTS_PER_CLUSTER`。slot 创建与 lifecycle terminal CAS 必须由同一 SQLite transaction/outbox 协议保护：终态提交后拒绝新 slot，崩溃前已持久化的 slot 必须计入，未持久化的 slot 不得凭空补齐。`cluster_expected_attempt_count=0` 时必须使用显式 `no_attempts` 终态，允许回收 lifecycle，但不得生成质量样本或进入任何质量分母、planner score、Half-Open score gate。对于大于 0 的 expected count，request finalizer 或 probe run finalizer 只有在存在 `0..cluster_expected_attempt_count-1` 的完整 terminal outcome 集合后才能置为 `true`，不能靠时间或当前已见行数猜测完成。未 finalized 的 cluster 只能保留 provisional 诊断和 pending 状态；provisional 诊断固定选择最早跨 outbound boundary 的结果（按 `attempt_index, observed_at, event_id` 升序），但不得进入质量分母、planner score 或 Half-Open score gate。finalizer 必须幂等，进程恢复也只能依据同一 lifecycle 终态补齐。`cluster_finalized=true` 后到达的迟到 attempt/outcome 只能写入不可变审计并标记 `late_after_finalization`，不得 reopen cluster、替换 canonical sample、改变质量摘要或 circuit state；重复 event 仍按唯一幂等键吸收。finalized 时选择 `attempt_index` 最大且已完成的 canonical outcome（同 index 以 `event_id` ASCII 升序），不因事件时间有效性改变最终结果选择；若该最终 outcome 的 `event_time_status` 不是 `Valid`，cluster 仍可 finalized 但质量摘要必须标记 `event_time_missing|invalid` 并排除该样本，不能以写入时间替代。raw attempt 仍完整保留给审计。
5. ActiveProbe 内部重试共享一个 correlation cluster；RealRequest 与 ActiveProbe 分开去重，不能相互覆盖。
 6. `ActiveProbe` 只有模型、endpoint 形态、协议、请求形态和 `comparability_key` 可验证一致时才进入质量来源；否则只进诊断。来源 `eligible` 必须由当前候选/有效 probe profile 的可比性快照显式提供，与样本数量无关；projector 不得从 observation 行数推断，也不得默认所有 Key 都有监控来源。`RealRequest` 在当前候选具备可比路由形态时即 eligible，即使暂时没有样本；`ActiveProbe` 只有当前有效 probe profile 声明的 comparability key 与请求形态一致时才 eligible，即使暂时没有样本；没有可验证的 probe profile/比较键才是 ineligible。`Administrative` 永远不参与质量来源。
7. 迁移旧 `routing_observations` 时，优先从现有 request/monitoring log 关联出真实 `correlation_id`；无法证明关联的旧行标记 `legacy_unclustered`，只保留审计，不作为 v3 独立质量样本，避免把历史 retry attempt 当成多个请求。新写入必须拒绝缺少 correlation/attempt 身份的质量观测。
8. 观测写入失败不能改变 outbound 响应成功/失败，但必须产生安全 diagnostics 和 projector backlog；不能用 Neutral 或假样本补写。当前请求仍排除已失败 Key，允许在 replay-safe 时尝试其他已知安全候选；不得因为写入失败继续重复同一 Key。
 9. 启动恢复必须扫描有 `attempt_started` 但没有 terminal outcome 的 durable attempt：若已记录 `boundary_crossed=true`，补写一次 `upstream_uncertain` 的 canonical failure 并送入 circuit/quality，设置 `recovery_origin=CrashRecovery|LeaseReaper` 和 `retry_disposition=StopRequest`；若未跨边界，补写 `local_abandoned` 审计但不污染 Key 质量。补写使用原始 `attempt_id/event_id` 幂等执行，不能因崩溃重新发送同一请求；request loop 必须识别该 disposition，不得把恢复事件当作普通可重试的 adapter 结果。

观测 migration/store 必须提供 `observation_id`、`event_id`/`attempt_id` 的唯一索引，以及按 `(source, station_key_id, station_key_lifecycle_revision, ingested_at, observation_id)` 和 `(station_key_id, station_key_lifecycle_revision, event_at)` 的查询索引；物理唯一键必须包含 `station_key_lifecycle_revision` 以隔离删除/重新绑定后的同名对象，逻辑去重键在单一当前 revision 内仍固定为 `source + station_key_id + correlation_id`，且不可依赖可变 JSON。写入失败进入有界 backlog，backlog 满或重试超限时只记录诊断并保持请求/Key 的 fail-closed 规则，不阻塞主请求直到无限重试。

### 6.6 P2 测试

- 每个矩阵分支都有 classifier unit test 和 request-finalization integration test。
- 502/429 都进入 recent quality sample，`failure_mass > 0`，不再显示 0。
- 同一 `source + station_key_id + correlation_id` 的 4 个 ActiveProbe attempt 只产生一个质量样本，并按 finalized/provisional 选择规则取值，且不产生 circuit event；另用 4 个不同 correlation 的 RealRequest fixture 断言 circuit 收到 4 个 attempt 事件。同一 RealRequest correlation 在不同 Key 上必须各自产生一个样本，不能跨 Key 合并。
- request/probe lifecycle 未提交终态或仍缺少预期 attempt 时，cluster 不能被错误 finalized；重复调用 finalizer、进程恢复补齐和 finalized 后迟到 attempt 都必须幂等且不产生第二个质量样本。
- 重复 event、乱序 event、迟到 event 不重复写入或覆盖新状态。
- 同一 Key 的并发成功/失败按 reducer 的 CAS 线性化顺序决定最终 streak；不能按客户端收到结果的先后覆盖状态。
- Key 删除/替换/重新绑定后，旧 lifecycle revision 的 observation/event 只进审计，不计入新对象质量或 circuit。
- 监控可比/不可比分支、不同来源去重、无 station key 的 administrative 观测。
- 缺失/非法 `event_at` 的观测可继续驱动 RealRequest retry/circuit，但不进入质量分母，`idle_real_route_sample=unknown`，且不能由 `observed_at`/`ingested_at` 补算。
- 启动恢复和 lease reaper 对 boundary 后无结果的 attempt 只补写一个 `recovery_origin` 标记的 `upstream_uncertain/StopRequest`，request loop 不会再次发送；boundary 前的 attempt 则只补写 `local_abandoned`。
- ActiveProbe 失败/成功只进入质量或诊断，不改变 circuit streak；带 Half-Open lease 的真实请求才推进恢复。
- 日志和 DTO 不含完整 key、URL、Authorization、原始响应。

### 6.7 P2 完成门

所有真实 outbound attempt 都能得到唯一 canonical outcome 和 attempt identity；502/429 不再走 Neutral 丢样本路径；质量去重和 circuit attempt 计数的差异有测试固定；观测写入失败有 backlog/diagnostic 并使该 Key fail-closed，不会在持久化状态未知时重复请求。

## 7. P3：Quality Projector v3

### 7.1 目标

以同一质量 projector 生成可靠性、响应时间、样本门槛和质量 basis。去除 Beta prior、旧 p95 作为评分输入、固定 evidence mass 和隐式来源权重。

### 7.2 owner 和数据存储

- 主实现：`src-tauri/src/application/quality_projection.rs`；如复杂度需要，新增同目录 `quality_projection_math.rs`，只允许一个数学 owner。
- 持久化：`src-tauri/src/persistence/stores/routing_quality_store.rs`。
- worker/retention：`src-tauri/src/background_tasks/routing_projection_runner.rs`（单一 supervised projector/retention owner，禁止再起第二个清理或投影 worker）。
- 快照接入：`src-tauri/src/application/operational_facts/planning_snapshot.rs`、`src-tauri/src/application/routing_engine/factors.rs`。
- 版本常量：`QUALITY_PROJECTOR_VERSION = routing_quality_v3`。
- 响应速度换算常量：由同一数学 owner 定义 `RESPONSIVENESS_SCORE_CAP_MS = 120_000`，planner、projector、UI 不得各自复制该值或换用另一种换算。
- 质量摘要使用 additive JSON/列迁移，不删除旧摘要直到 v3 重建完成。

### 7.3 计算顺序

每个 `station_key_id`、每个来源分别执行，严格按以下顺序：

1. 为一次 projection 固定单一 `evaluation_at`（UTC 毫秒）；所有窗口边界、年龄和 `c` 都使用这个值，不能在同一批处理中读取多次 `now`。
2. 读取 immutable observations，按 `(event_at, observation_id)` 稳定排序。
3. 按 `source + station_key_id + correlation_id` 去重，得到独立请求样本。
4. 只保留当前 `station_key_lifecycle_revision` 的质量 outcome，且要求 `event_time_status=Valid`；已删除、替换或重新绑定前的 observation 只保留审计，不污染新 Key 对象。取消、客户端、未出站本地错误和缺少有效事件时间的 outcome 不进入可靠性分母。
5. 划分不重叠窗口：近期 `[evaluation_at-24h, evaluation_at]`；历史 `[evaluation_at-30d, evaluation_at-24h)`。超出 30 天只保留审计。
 6. 对每条样本先排除 `event_at_ms > evaluation_at_ms` 的未来事件，再使用整数毫秒计算年龄：`age_ms_i=evaluation_at_ms-event_at_ms`，令 `a_i=age_ms_i/3_600_000` 小时并计算时间权重。不得通过把未来时间截成 0 来扩大样本集合。调用方只能使用唯一的 `time_decay_weight(age_ms_i)` API；该 API 内部按 24 小时边界选择 72 小时或 24 小时半衰期，调用方不得自行拼接两段公式或预先四舍五入年龄：

   ```text
   w(a_i) = 2 ^ (-a_i / 72),                         0 <= a_i <= 24
   w(a_i) = 2 ^ (-24 / 72) * 2 ^ (-(a_i - 24) / 24),  a_i > 24
   ```

    以 `weight_scale=1_000_000` 的 fixed-point 整数保存，采用固定 half-up 舍入；对窗口内任何真实正权重，量化结果必须执行 `max(1, round_half_up(weight_scale*w_i))`，防止接近 30 天的历史样本被量化为 0 而造成“样本足够但分母为 0”。该最小表示单位不改变未量化规范公式，且必须计入算法版本并有 30 天边界 golden vector。`exp2_neg_ratio(age_ms, half_life_ms)` 只能作为 `time_decay_weight` 的内部 helper，输入使用整数毫秒、输出只在最后量化一次；planner、projector、SQL 和前端不得各自实现指数或再次舍入。golden vector 的量化期望值为 `a=0 -> 1,000,000`、`a=24 -> 793,701`、`a=48 -> 396,850`、`a=72 -> 198,425`、`a=720 -> 1`；前端不得重算浮点值。

7. 近期独立样本数为 `n`，历史独立样本数为 `m`；两者只统计最终 canonical outcome 为 `Success` 或 `AttributableFailure`、且已跨 outbound boundary 的去重请求，`Excluded` 不进入分母。近期混合权重为：

   ```text
   c = min(0.9, n / (n + 20))
   ```

    `n=0` 时 `c=0`。实现使用 `ratio_scale=1_000_000`：`c_fp = round_half_up(ratio_scale * min(0.9, n/(n+20)))`，其中 `0.9` 表示为 `900_000`；不得使用浮点或按样本循环累计 `c`。

8. 可靠性中 `s_i=1` 为成功，`s_i=0` 为失败。先应用窗口门槛，再合成来源值：

   ```text
   optimisticReliability = optimisticReliabilityPercent / 100

   R_recent = sum(w_i * s_i) / sum(w_i),  if n >= recentMinimumSamples
   R_recent = optimisticReliability,       otherwise

   R_history = sum(w_i * s_i) / sum(w_i),  if m >= historicalMinimumSamples
   R_history = optimisticReliability,       otherwise

    R_source = c * R_recent + (1 - c) * R_history
    ```

    `R_recent`、`R_history` 和 `R_source` 均以 `[0, 10_000]` 的整数质量因子保存；乘法使用 checked `u128`，最终除法和 `c_fp` 的量化统一采用 half-up，不能因中间截断让全成功/全失败结果越界。

9. `RealRequest` 和可比的 `ActiveProbe` 独立得到 `R_source`，再按当前 policy source weight 只混合一次：

   ```text
   R = effectiveRealTrafficWeight * R_RealRequest
     + effectiveMonitoringWeight * R_ActiveProbe
   ```

   公式中的 `effective*Weight` 使用总和为 `1` 的比例；如果实现使用 basis points，则计算结果必须除以 `10_000`。不可比来源权重置零后，对剩余且配置权重大于 0 的 eligible 来源归一化到 10,000；可比但无样本仍保留其正配置权重并使用乐观值。若没有任何正权重的 eligible 来源，返回 `quality_unavailable`，不得除以 0 或擅自把用户配置的 0 权重改成正权重。该来源权重只在可靠性和响应速度的最终质量混合处各使用一次，不得乘到单条观测、成本、人工偏好或 circuit 计数上。

10. 响应时间分别使用成功完成的真实路由请求和可比主动监控探针：非流式为完整响应时间，流式为 TTFT。失败/取消/缺失/非法延迟不进入速度样本，但失败仍可进入可靠性。两个来源各自对有效延迟 `l_i` 使用同一 `w_i`，先完成来源内窗口聚合，再按有效来源权重混合：

   ```text
   L_recent = sum(w_i * l_i) / sum(w_i),  if n_latency >= recentMinimumSamples
   L_recent = optimisticLatencyMs,         otherwise

   L_history = sum(w_i * l_i) / sum(w_i),  if m_latency >= historicalMinimumSamples
   L_history = optimisticLatencyMs,        otherwise

   c_latency = min(0.9, n_latency / (n_latency + 20))
   L_source = c_latency * L_recent + (1 - c_latency) * L_history

   L = effectiveRealTrafficWeight * L_RealRequest
     + effectiveMonitoringWeight * L_ActiveProbe
   responsiveness_score = existing_latency_to_score(L)
    ```

    `c_latency` 使用同一 `ratio_scale` 和 half-up 规则；`L_recent/L_history/L_source/L` 以 `latency_scale=1_000`（0.001ms）保存，最后一次性按代码常量 `RESPONSIVENESS_SCORE_CAP_MS=120_000` 转换：先计算 `q_ms=round_half_up(L_fp / latency_scale)`（单位恢复为整数毫秒），再计算 `floor(10_000 * (120_000 - min(q_ms, 120_000)) / 120_000)` 并 clamp 到 `[0,10_000]`。不得使用未定义的 p95 或前端自算转换。

   监控延迟参与 `responsiveness_score` 的最终来源混合；不得先对每条延迟转分再做窗口或来源平均。监控仍只作为质量/诊断来源，不参与 request retry、request exclusion 或 Key circuit。

11. 至少有一个正配置权重的来源 `eligible` 时，没有样本的 Key 仍返回固定质量值和 `quality_basis=OptimisticInsufficientSamples`；不得写 synthetic observation。记录 `last_real_route_sample_at` 和 `idle_real_route_sample`（`true|false|unknown`；缺少有效 `event_at` 时为 `unknown`）。如果没有任何正权重的来源 `eligible`，返回 `quality_unavailable`，不把乐观值伪装成质量结果；planner 按第 2 节约定使用可用评分因子归一化 fallback，所有评分因子都不可用时按 `station_key_id` 排序。

实时增量更新只把受影响 Key 放入有界重算队列；该队列由单一 supervised projector worker 消费，并按 Key 合并重复任务，队列满时保留 observation、增加 backlog 诊断而不是无限扩容内存。重算时从 immutable observations 重新执行当前生命周期过滤、去重、窗口和 `c` 公式，不直接在派生摘要上累加，避免迟到事件、重复事件和样本门槛变化造成漂移。

### 7.4 fixed-point 实现要求

1. 扩展现有 `src-tauri/src/application/routing_engine/fixed_point.rs` 或新增同层数学 helper，禁止在 planner、UI、SQLite SQL 中各自实现指数和舍入。唯一对外 helper 为 `time_decay_weight(age_ms)`，内部使用版本化 `exp2_neg_ratio(age_ms, half_life_ms)` 并按 24 小时边界选择半衰期；调用方不得先按分钟/小时截断年龄，helper 只返回一次按 `weight_scale` 量化且带正权重最小值 1 的结果。
2. 所有 `w_i`、`c_fp`、source weight、质量因子和最终 score 使用明确 scale、checked arithmetic 和饱和/错误策略；除法分母为 0 时，eligible 但样本不足走 optimistic 分支，不可比来源走 `quality_unavailable` 诊断，任何分支都不得返回 NaN/Infinity。每个独立请求固定 `base_mass_i=1`，`effective_weight_i = max(1, round_half_up(weight_scale * w(a_i)))`（仅对已通过窗口过滤的正权重样本）；来源权重不得提前乘入单条观测，也不得继续使用旧 `evidence_mass` 作为来源差异。
3. 评分四项因子先各自量化到 `[0,10_000]`，再按 `sum_i(weight_bps_i * factor_i)` 使用 `u128` 累加，最后只在除以 `10_000` 时执行一次 half-up；缺失因子不填中性值，而按仍可用因子的原配置权重重新归一化，全部缺失才使用 `station_key_id` 稳定排序。亲和修正严格沿用 P0 固定的既有公式，修正后的值再 clamp 到 `[0,10_000]`，不得在因子层重复舍入。
 4. `algorithm_version`、`weight_scale`、half-life 和舍入模式进入质量摘要；算法变更必须触发摘要重建。
 5. golden vector 至少覆盖 `a=0/24/48/72/720` 小时、24 小时边界归属、n=0/1/5/20/100、历史不足/足够、全成功/全失败/混合成功、不可比来源和“所有可比来源配置权重为 0”的 `quality_unavailable` 分支。
 6. projector 查询必须按 `source/station_key_id/event_at/ingestion_sequence/correlation_id` 使用索引，固定使用代码常量分批处理并保存 checkpoint；批大小不是用户设置，禁止对每把 Key 发起 N+1 查询。一次运行固定 `quality_generation_id`、`evaluation_at`、`algorithm_version`、`policy_revision` 和输入 watermark；watermark 是全局 `ingestion_sequence`，不以 `event_at` 代替，迟到但事件时间较早的 observation 进入下一次增量 projection 并计入 lag。当前 Key lifecycle revision 和 source eligibility 必须由一次批量候选/probe profile 快照提供，不能用 observation 最大 revision 或默认两源 eligible 推断。每批以 `(station_key_id, observation_id)` 游标读取；若同一 correlation cluster 跨批，必须把未完成 cluster 的去重状态持久化到 generation-scoped checkpoint/临时表，直到该 cluster 的 durable lifecycle 终态确认后才能计入摘要，不能在批边界把部分 attempt 当成最终样本。质量摘要、pending-cluster 状态与 checkpoint 必须以 `(quality_generation_id, station_key_id, station_key_lifecycle_revision)` 隔离并在同一事务中幂等提交；runner 必须批量读取 Key 与 profile，禁止按 scope/Key 逐个 `list_for_scope` 形成 N+1。checkpoint 至少保存游标、已处理数量、输入 watermark、摘要 content hash 和状态（`building/ready/failed`）；重启必须从最后一个已提交 checkpoint 继续，不能从头重复计数。只有所有批次完成、hash/计数校验通过且状态为 `ready` 的 generation 才能被 P8 激活。聚焦测试覆盖批边界、重启续建和单次全量结果一致性。
 7. Active generation 运行时继续使用最近一次完整质量摘要；`quality_projection_lag` 不超过代码常量 `MAX_ACTIVE_QUALITY_LAG_SECONDS=900` 时允许继续服务但必须标记 `quality_stale`，超过该值或质量摘要不可读时将质量因子标记为 unavailable，按第 9.3 节的可用因子归一化 fallback/稳定 Key ID 排序，不把 stale 数据伪装成新鲜样本。该 freshness 行为只影响排序诊断，不改变 circuit、retry 或容量硬门；lag 超阈值仍禁止 P8 cutover。

 8. 由单一 supervised quality-retention worker 执行原始 observation 清理：只删除早于 30 天窗口、且不再被任何 active、building、ready 或已被选为回滚目标的 generation 输入 watermark 或 checkpoint 引用的行；删除按有界 batch、索引顺序和 SQLite busy budget 执行。circuit raw event 使用同一 retention owner，至少保留 30 天及所有 circuit generation watermark/rollback 引用；被清理行的脱敏计数与质量摘要保留为 audit/rollup，不能把 retention 失败伪装成“没有样本”，也不能删除仍可用于 circuit replay 的 raw event。

### 7.5 P3 验收

- 新摘要不再消费 `BetaPrior`、`reliability_prior_*`、旧 p95 评分或固定 5000/10000 source mass。
- `recent/history` 样本和质量分母只使用去重后的独立请求数。
- 实际路由 70%、监控 30% 默认混合，权重调整后按新 revision 生效。
- 24 小时无真实路由样本时 `n=0,c=0`，历史值或乐观值参与排序，不制造探测。
- 来源全部不可比或可比来源的配置权重均为 0 时只返回 `quality_unavailable` 诊断；planner 的 fallback 只使用确实存在的其他评分因子并按原配置权重归一化，全部因子不可用时按稳定 Key ID 排序。
- 质量摘要能解释每个来源、窗口、门槛、质量 basis、mass、最后样本时间和被合并 observation ID。

### 7.6 P3 完成门

使用同一 `QualityProjectorV3` 生成 v3 质量摘要；固定点 golden vectors 与现有实现对比证据齐全；planner 读取的 v3 质量值不含 prior、随机或浮点未定义结果；旧 active 摘要只有在 P8 的 v3 generation 完整后才可被替换。

## 8. P4：station_key circuit reducer、持久化和 Half-Open lease

### 8.1 目标

替换默认关闭的错误率保护，建立唯一的 Key 级连续失败状态机，并防止并发、重启、重复事件和旧结果把状态写坏。

### 8.2 owner 和代码范围

- `src-tauri/src/application/health_protection.rs`
- `src-tauri/src/persistence/stores/routing_health_verdict_store.rs`
- `src-tauri/src/persistence/stores/health_observation_store.rs`
- `src-tauri/src/models/health.rs`
- `src-tauri/src/application/routing_engine/routing_health.rs`
- `src-tauri/src/application/routing_engine/admission.rs`
- `src-tauri/src/persistence/migrations/0062_routing_key_circuit_v3.sql`（编号由 P0 预留；占用时按第 0 节整体顺延）

`0062` 只增加 v3 Key/lifecycle 状态、事件幂等键、reducer sequence 和 lease 字段；旧错误率窗口数据保留审计，不直接转换成连续失败。迁移 postcondition 必须证明每个当前 lifecycle 至多一行状态、旧 Half-Open lease 已按 fence 处理，重复运行不会重复打开或清零 Key。

`ErrorRateProtectionService` 不得继续作为旁路 owner；可以收敛成 adapter，最终只能有一个 reducer 和一个 planner admission read port。

### 8.3 状态和事件

状态持久化为（新 Key 初始值为当前 policy revision 下的 `Closed(consecutive_failures=0, reopen_level=0)`）：

```text
Closed(state_revision, consecutive_failures, reopen_level, policy_revision)
Open(state_revision, opened_at, cooldown_until, consecutive_failures, reopen_level, policy_revision)
HalfOpen(state_revision, lease_id, lease_revision, lease_expires_at, recovery_successes, reopen_level, policy_revision)
```

事件至少包含：`event_id`、`effect_kind`（observation/circuit/lease）、`source`（circuit event 必须为 `RealRequest`）、`attempt_id`、`station_key_id`、`station_key_lifecycle_revision`、`policy_revision`、`expected_state_revision`、`occurred_at`、`canonical_outcome`、`failure_code`、`recovery_origin`、`retry_disposition`、`lease_revision`（若有）。CAS 成功应用时由 reducer 在同一事务为该 Key/lifecycle 分配单调的 `reducer_commit_sequence`；重复 `(event_id, effect_kind)` 不重新分配 sequence，旧 expected revision 不得覆盖新 revision，结果 DTO 同时返回 applied/new state revision/commit sequence 供 trace 关联。只有已应用事件参与 circuit 重建，未应用或迟到事件只保留审计。

持久化约束必须显式落在 schema/store：每个 `(station_key_id, station_key_lifecycle_revision)` 只有一行当前状态；`(event_id, effect_kind)` 具备唯一幂等索引，`attempt_id` 在同一 effect kind/lifecycle 下唯一，`reducer_commit_sequence` 在 circuit event 中唯一且可按 Key/生命周期索引；`state_revision`、`lease_revision`、`cooldown_until` 和所有时间使用有界整数/UTC 毫秒；Half-Open lease 的过期清理不能删除审计 event。删除、替换或重新绑定 Key 时先递增 `station_key_lifecycle_revision`，再写新状态，不能复用旧行的身份。

每次 `RealRequest` attempt 的 observation 和 circuit event 必须通过同一 SQLite transaction 或 durable outbox 提交；`ActiveProbe` 只提交 observation，不生成 circuit event。先以 `event_id` 幂等落盘，再用 `state_revision` CAS 应用 reducer；CAS 冲突必须重新读取最新状态并按事件唯一键重放一次，不能直接覆盖。outbox 重放必须保持原始 `event_id` 和 `attempt_id`。单个 Key 的持久化读写失败时，该 Key 立即采取 fail-closed：当前请求排除该 Key、记录 `circuit_persistence_unavailable`，不伪造 Open/Closed；并持久化或由启动检查恢复 `persistence_unavailable` admission gate，只有一次明确成功的健康读写检查（不是普通新请求到达）才能清除。进程重启时先恢复该 gate，完成健康检查后才能解除，不能仅因重启自动清除。若 circuit store 的共享状态读写不可用，所有依赖该 store 的候选都禁止新的 candidate admission；独立且状态读写健康的候选仍可按 request budget 尝试，若没有则按终态优先级返回 `no_available_key`（若已有最后一个可归责失败且 retry 已耗尽，则返回该 canonical failure），并保留有界、可重放的 backlog。

### 8.4 状态转换

1. Closed 收到可归责失败：`consecutive_failures += 1`；达到阈值时原子转 Open，并将 `reopen_level` 至少设为 `1`（首次打开固定为 `1`，不能沿用 `0`）。
2. Closed 收到成功：连续失败清零，保持 Closed；成功不是质量 source weight 的函数。
3. Open 未到 `cooldown_until`：硬跳过，不申请 lease。
4. Open 冷却结束：候选 hard cap 必须先基于完整候选集合计数；超过上限时立即返回上限错误，不计算 score gate，也不能截断后再计算。未超过上限后，每次 planning 从同一候选 read snapshot 计算该 Key 所属硬层的 `best_closed_score`；比较基线只包含同一 Primary/Backup/Emergency 层内、当前模型/请求形态匹配、站点/生命周期/凭据有效且未被用户禁用的 `Closed` Key，不提前应用 circuit、request exclusion 或本地容量。不同硬层的分数不可比较，不能用跨层 Closed Key 作为 gate 基线。只有 Key 分数严格高于该值或该硬层没有这样的 Closed 候选时才申请 Half-Open。`best_closed_score` 必须从完整候选集合以批量扫描得到，不能因数据库返回顺序、候选 cap 截断或容量准入而改变比较基线。若质量因子不可用但 planner 能按第 9.3 步生成确定的有限 fallback score，则使用该 fallback score 做 gate；只有 Key 和同层比较基线都无法形成可比较分数时才记录 `quality_unavailable`/`half_open_admission_denied_by_score` 并等待下一次规划，不能用乐观值冒充不可比较分数。`Open(cooldown_elapsed)` 只是派生显示，不新增存储状态。
5. Half-Open 同时只允许一个真实 outbound lease；先取得本地容量 lease，再原子取得 Half-Open lease，并在同一准入事务中持久化 `attempt_id`、`boundary_crossed=false` 和 `candidate_admitted`。`lease_expires_at` 固定取申请请求的 immutable `deadline_at`，不得超过该 deadline；若 deadline 已过则不申请 lease。容量拒绝不消费 Half-Open lease，也不改变 circuit；Half-Open CAS 竞争失败时必须立即用同一 `capacity_lease_id` 幂等释放已取得的容量 lease。准入提交前的取消、deadline、目标删除、generation fence 和 lease race 释放临时 lease，不写样本；准入提交后不因 fence 撤销，直至结果或 deadline。
6. Half-Open 成功：只按独立真实路由请求增加 `recovery_successes`；达到阈值转 Closed、清 cooldown、连续失败和 reopen level。
7. Half-Open 任一可归责失败：立即 Open，恢复成功清零，`reopen_level += 1`，使用递增冷却。
8. `RealRequest` 的 `429`、5xx、以及已跨 outbound boundary 的超时/连接失败按同一 Key 失败路径；不依据 Retry-After 增加额外冷却。普通 `ActiveProbe` 结果不改变 circuit streak，只有显式绑定到真实 Half-Open lease 的 RealRequest 才能推进恢复。
9. 进程重启恢复持久化状态；旧 Half-Open lease 按第 12 条的过期/撤销规则处理，不能跨 profile revision 继续关闭新状态。
10. Closed 状态下并发 attempt 的成功和失败也必须经过同一个 CAS reducer；成功只清零其线性化提交点之前的失败 streak，不能清除之后已经 Open 的状态。
11. circuit 状态和 event 必须带 `station_key` 生命周期/绑定 revision；Key 被删除、替换或重新绑定时递增 revision，旧状态和迟到 event 只保留审计，不得误伤新 Key 对象。
12. Half-Open lease 在 candidate admission 提交前因取消、deadline、目标删除、generation fence 或 lease race 被拒绝时，只释放临时 lease，状态回到可重新申请的 `Open(cooldown_elapsed)`，不写成功/失败样本；candidate admission 已提交后必须保留 lease 到结果或 `lease_expires_at`，不因 fence 撤销。lease 已跨 boundary 但在 `lease_expires_at` 前没有结果时，必须原子转为新的 `Open`、`reopen_level += 1` 并使用递增冷却，之后到达的结果一律视为迟到审计，不能关闭新状态。`boundary_crossed` 必须由 attempt owner 在真正跨边界前原子标记，reaper 只能据此判断是否重开 circuit。
13. policy revision 变化不重置 Closed 的失败 streak，也不缩短已有 Open 的绝对 `cooldown_until`；尚未提交 candidate admission 的 Half-Open 竞争必须按上一条规则撤销并以新 policy revision 重新申请，已提交准入的 lease 按原 generation 完成，结果以 lease/state revision 防止误关新状态。新 revision 的阈值在下一次 reducer 事件或 admission 时生效：若现有 Closed 的 `consecutive_failures >= new_threshold`，该次 admission 先由 reducer 原子转 Open，再禁止发送；阈值提高只保留既有 streak，阈值降低不能等到下一次失败才处理。恢复成功阈值变更对已持有的 Half-Open lease 不追溯取消，lease 完成后按申请时 revision 结算；新申请使用新阈值。等待时间变更只影响下一次 Open 或下一次重开，已有 `cooldown_until` 绝不缩短。

`opened_at`、`cooldown_until` 和 `reopen_level` 的计算以唯一 `CircuitClockOwner` 在 reducer 事务线性化提交点读取的受控 UTC 毫秒为准；该 owner 将 `logical_now_ms=max(system_utc_now_ms, persisted_clock_watermark_ms)` 写回同一事务，重启先读取 watermark 后再采样系统时钟，多进程由 SQLite 写锁串行化。系统时钟回拨时不得缩短已有 cooldown；producer 的 `event_at`/`occurred_at` 只用于审计和质量窗口，不能让迟到或未来时间改变冷却长度。测试必须注入受控时钟，覆盖重启、多个进程、时钟相同、回拨、未来 event_at 和跨毫秒边界的结果。

必须有单一 supervised lease reaper 按固定周期扫描持久化 `boundary_crossed=true` 且超过 `lease_expires_at` 的 Half-Open lease，提交带原 lease revision 的幂等过期事件；reaper 与请求 finalizer 竞争时由同一 CAS 决定唯一结果。reaper 崩溃或重复运行不得重复递增 `reopen_level`，`boundary_crossed=false` 的 lease 只能释放而不能打开 circuit。

同一个 admission-lease supervisor 还必须扫描普通本地容量 lease 和所有已写入 `candidate_admitted` 的 attempt，不得只清理 Half-Open lease：

- `boundary_crossed=false` 且请求取消、目标删除或 `deadline_at` 到期时，幂等写入 `local_abandoned`，释放容量和未使用的 Half-Open lease，不写 Key 质量或 circuit event；
- `boundary_crossed=true` 且没有 terminal outcome 时，幂等补写带 `recovery_origin=LeaseReaper` 的 `upstream_uncertain`，按 circuit 失败路径处理并释放容量；该结果的 `retry_disposition` 固定为 `StopRequest`；
- 每个 `(attempt_id, capacity_lease_id, half_open_lease_id)` 只能产生一次释放和一次 terminal outcome。进程重启、reaper 重复运行、请求 finalizer 并发和 SQLite busy 重试都必须通过同一状态/CAS 保证不泄漏、不重复计数、不负计数；普通容量 lease 的回收不能另起第二个有竞争关系的 owner。

### 8.5 冷却算法

```text
cooldown = min(
  recoveryWaitSeconds * 2^(reopen_level - 1),
  system_max_cooldown_seconds
)
```

第一次 Open 使用 `reopen_level=1`；Half-Open 失败重开递增；恢复 Closed 后归零。指数计算必须先做 checked/saturating arithmetic，再与 hard cap 比较，不能因 `reopen_level` 很大溢出或绕过上限。系统最大冷却是内部 hard cap，不出现在 policy v3。

### 8.6 P4 测试

- 连续失败阈值 1/3/10；多个真实 outbound attempt 按该 Key 的 reducer 提交顺序累积并能 Open。Closed Key 在同一请求内的普通失败会继续重试当前 Key，因此同一请求可以贡献多次 circuit 失败并达到阈值；可靠性统计仍按 correlation 去重，不会把这些 attempt 重复放大成多条质量样本。Half-Open 每次 lease 只允许一个真实请求，连续成功仍按独立真实请求计数。
- 一次成功清零 Closed streak；Half-Open 必须达到连续真实成功阈值才 Closed。
- Half-Open 并发 2/10/100 请求只有一个 lease，其余明确 skip。
- Half-Open/capacity lease 取消释放、lease 超时、目标删除、策略 revision 变化、进程重启恢复；重复释放和迟到结果不造成容量泄漏或负计数。
- Half-Open lease 的 `lease_expires_at` 不晚于请求 `deadline_at`，deadline 到期后 reaper 只允许一次幂等重开；长请求在 deadline 前持续持有 lease，不得被 reaper 并发放行第二个真实请求。
- 普通容量 lease 与 Half-Open lease 在进程崩溃、取消、目标删除和 deadline 后都由同一 supervisor 回收；`boundary_crossed=false` 只产生 `local_abandoned`，`boundary_crossed=true` 只补写一次 `upstream_uncertain/StopRequest`，重复运行不泄漏容量或重复增加 streak。
- 重复 event、乱序 event、旧 lease result、迟到成功不能关闭新一轮 Open。
- `429` 与 502 完全相同的 state/event/retry 断言。
- ActiveProbe 事件不会打开/关闭 RealRequest 的 Key circuit；普通真实请求与带 Half-Open lease 的真实请求分别覆盖失败计数和恢复计数。
- 新请求面对所有 Key Open、冷却结束但 score gate 拒绝、禁用或 lease 占用时返回 `no_available_key`，不消耗剩余 retry；当前请求若只是因已取得 admission/跨边界后的 request exclusion 没有未尝试候选，则返回最后一个 canonical failure。
- `quality_unavailable` 但仍有其他评分因子时使用 planner 的确定性 fallback 参与 score gate；所有评分因子都不可比较时才拒绝 Half-Open，不得把乐观值当成比较分数。

### 8.7 P4 完成门

每个 Key 只有一个 durable circuit state owner；Open/冷却/Half-Open/恢复状态可在重启后恢复；CAS、幂等、lease fence 和 fail-closed 异常路径都有持久化测试；没有任何 V2 error-rate breaker 继续接收生产事件。

## 9. P5：确定性 planner、分层排序和容量域移除

### 9.1 目标

完成目标 planner 的实现和测试，但在 P8 原子切换前不改变生产 active generation。切换后，planner 在同一硬资格层中只按最终 fixed-point score 降序尝试，删除随机探索/rendezvous；保留本地容量硬门，彻底删除容量域生产输入。

### 9.2 代码范围

- `src-tauri/src/application/routing_engine/intelligent_planner.rs`
- `src-tauri/src/application/routing_engine/dispatch.rs`
- `src-tauri/src/application/routing_engine/admission.rs`
- `src-tauri/src/application/operational_facts/planning_snapshot.rs`（snapshot builder/read transaction owner）
- `src-tauri/src/application/operational_facts/candidate_projector.rs`、`src-tauri/src/persistence/stores/operational_facts/queries.rs`（capacity rejection code normalization and batch candidate facts）
- `src-tauri/src/application/routing_engine/planning_snapshot.rs`（snapshot DTO/validation；不得遗漏旧 V2 字段清理）
- `src-tauri/src/application/routing_engine/candidate_plan.rs`
- `src-tauri/src/application/routing_engine/tiers.rs`
- `src-tauri/src/application/routing_engine/exploration.rs`
- `src-tauri/src/application/routing_engine/failure_domains.rs`
- `src-tauri/src/persistence/stores/routing_store.rs`
- `src-tauri/src/models/station_capacity_domains.rs`
- `src-tauri/src/services/proxy/upstream.rs`、`src-tauri/src/services/proxy/adapters/openai.rs`（移除 production capacity-domain commitment 输入）

### 9.3 planner 步骤

1. 在一个 SQLite read transaction 中先读取 `runtime_generation` pointer，再读取与其 revision 相符的 policy、quality、health facts；随后捕获 runtime capacity registry revision。数据库事务与内存容量 registry 不能宣称为同一个原子快照，准入 CAS 必须再次校验两者的 revision，并在任一 revision/hash 改变时丢弃 snapshot、释放临时资源后重新规划。不可变 `PlanningSnapshot` 必须包含请求模型/形态、policy revision、quality revision、health revision、runtime capacity revision、deadline 和候选事实；禁止把一次读取中来自不同 generation 的事实拼接在一起。
2. 先建立 capability pool（支持当前模型/请求形态/协议的候选，不受用户禁用、凭据状态、circuit、request exclusion 或容量影响）；再过滤 Key 固有资格（凭据存在且生命周期有效、站点启用），并把用户禁用、未到期 Open、请求已尝试 Key 和 Half-Open lease 标为运行时阻断。这些硬门都不能被评分抵消，且终态分类必须保留“能力不匹配”和“暂时不可用”的区别。
 3. 保持 Primary/Backup/Emergency 等既有硬层级。对每层先保留通过 capability、生命周期、站点/凭据和用户资格的候选；未到期 Open、已占用 Half-Open lease 和本请求已尝试 Key 直接标为非容量阻断并排除。对冷却已结束的 Open Key，先用同层 `Closed` 候选计算 score gate，只有通过 gate 的 Key 才重新加入该层候选；该层无任何可执行候选时直接评估下一层；该层仍有候选但全部本地容量拒绝时才进入下一层；只要有一个候选取得容量 lease，就不得越级。任何 `failure_domain`、`capacity_domain` 或等价域字段只可写入诊断，不能参与候选排除、等待、排序或 fallback；容量不足必须立即尝试同层下一个候选。
 4. 四项评分因子统一为 `[0,10_000]`，按 policy basis points 加权，使用 fixed-point；成本或偏好因子缺失时必须标记 `factor_unavailable` 并按 7.4.3 的剩余权重归一化，不能填充 5,000 中性值。会话亲和继续使用 P0 固定的既有有界 bonus、hysteresis 和逃逸规则，作为基础分数之后的唯一 dispatch 修正，修正后的 `effective_score` 再做确定性降序排序。实时负载和 runtime anomaly 不再作为分数惩罚，负载只通过后置本地容量准入决定是否跳到下一候选。若 `quality_unavailable` 导致某个因子没有值，不得套乐观值；只对仍有值的因子按其原配置权重重新归一化计算 fallback score，fallback score 也要经过同一既有 affinity 修正；所有因子都不可用时以 `score_status=unavailable` 和 `station_key_id` 稳定排序。不得在本次重构中把亲和改成仅同分 tie-break 或新增第二套亲和算法。
5. 形成候选序列时，先将 `Closed` 和通过 score gate 的 `Open(cooldown_elapsed)` 统一视为可排序候选；所有具有有限 `effective_score` 的候选（`score_status=scored` 或 `score_status=unavailable` 但已按可用因子归一化得到 fallback score）使用同一个 `effective_score DESC, station_key_id ASC` 比较器。只有所有评分因子都不可用、没有有限分数的候选才最后按 `station_key_id ASC` 排序。Open Key 只有在 P4 score gate 通过后才进入序列，Half-Open lease 在实际 admission 时取得；输入数组顺序、hash seed、探索比例不能影响结果，trace 必须显示 quality basis/fallback 原因。
 6. 候选 hard cap 是代码拥有的 `MAX_OPERATIONAL_CANDIDATES`（当前代码值 1024），不是 policy 字段。`candidate_cap_count` 按 `(station_key_id, station_key_lifecycle_revision)` 去重，固定定义为通过当前模型/请求形态/协议能力、已启用站点、有效凭据和 Key 生命周期资格的 Key 数；同一 Key 的多个 model variant 不重复计数，variant 只影响该 Key 内部的执行身份和排序打平。用户禁用、circuit、request exclusion 和本地容量在 cap 统计之后处理；snapshot builder 必须在批量查询阶段完成该完整评估并统计候选数。不得先按数据库返回顺序截断，也不得用 top-K 静默丢弃候选。超过 hard cap 时在任何 outbound attempt 前返回 typed `route_candidate_limit_exceeded`，记录 `candidate_cap_reached` 和总数/上限诊断，不进入容量准入、重试或跨 Key 故障转移。未超过上限时才按上述确定比较器形成完整候选序列，并按每个硬层计算对应的 `best_closed_score`、Open score gate 和容量准入。
7. 依序申请本地容量 lease。容量不足只跳到后一个候选，不消费 outbound retry；不读取或比较 capacity-domain identity。容量 registry/read 或 lease CAS 不可用时必须 fail-closed，不把未知容量当作可用，不产生 outbound attempt，并返回明确的容量/运行时不可用诊断。`candidate_admitted` 之前的准入事务必须用 CAS/lease 校验 candidate 的 `station_key` lifecycle、circuit state revision/状态（Closed，或成功占用 Half-Open lease）和容量 revision；任一校验失败只释放临时资源并重新规划，不产生 outbound attempt、不消耗 retry。该事务成功提交后即视为已准入，不得再用一次状态重读把它降级为“未准入”；attempt owner 只需在实际跨 outbound boundary 时原子标记 `boundary_crossed=true`，准入后的本地连接/适配器失败按已准入但未跨边界的 `local_abandoned` 终态释放资源，不写 Key 质量或 circuit。所有候选事实、质量摘要、circuit 状态和容量快照必须由有界批量查询/一次 read snapshot 提供，禁止对每个候选发起 N+1 数据库往返。
   - 容量相关的内部拒绝码只允许 `capacity_exhausted`（已知当前承载不足）和 `capacity_state_unavailable`（registry/lease 状态不可读写）；现有 `capacity_unavailable` 只能在旧数据/兼容入口被翻译，不能继续作为 production planner、admission 或公共 error code。
8. Candidate plan 必须输出 admission reason、quality basis、score revision、circuit state、capacity result、stable rank 和 `candidate_cap_reached`，供 trace 使用。

`candidate_cap_reached` 是系统安全上限诊断，不是用户可调策略；达到上限时只返回 `route_candidate_limit_exceeded`，不触发额外查询、容量准入、outbound attempt 或 retry。未达到上限时，若候选全部被容量/硬门阻断，按这些候选的真实阻断原因执行第 10.4 节终态规则。

### 9.4 容量域清理

1. 从 `planning_snapshot`、`candidate_plan`、`admission`、`execution` 的生产读路径移除 `capacity_domain`、`capacity_domain_revision`、`excluded_capacity_domains`、`failure_domains` 驱动的排除、同域等待和跨域 fallback；这些字段如需出现在历史诊断，只能作为 opaque/non-routing metadata。
2. `station_capacity_domains` 表、store、旧 DTO/API 可以暂留迁移/审计兼容，但不得被 planner/admission/execution 读取；不在本计划删除历史数据。
3. 保留本地并发/资源容量 registry 及其 hard cap；它只回答“这把 Key 当前是否承载得下”。
4. 通过 `rg` 和 architecture test 证明没有 production consumer；更新删除台账，标记每个残留的 owner、迁移用途和删除前提。

### 9.5 P5 测试

- 相同 snapshot 改变输入数组顺序，候选序列完全相同。
- 评分和亲和修正后的分数相同才由 `station_key_id` tie-break；亲和沿用现有 bonus/hysteresis/逃逸 golden vectors，不能绕过硬资格、熔断、容量或明显劣化逃逸。
- Primary 有可执行 Key 时 Backup 不越级；Primary 全部容量不足时允许进入 Backup。
- Primary 因 Open/禁用/请求排除而没有剩余候选时评估 Backup；不能把非容量阻断误判成“容量不足”。
- 高分 Key 容量足够时连续承载；容量不足才尝试后一个 Key。
- 通过 score gate 的冷却结束 Open Key 与 Closed Key 共同按分数排序；未通过 gate 的 Open Key 不会因 cap 或数组顺序进入候选。
- Half-Open score gate 只与同一 Primary/Backup/Emergency 硬层的最高 Closed 分数比较；跨层高分不能越过既有层级边界，也不能因数组顺序改变 gate 结果。
- exploration/rendezvous/seed 不再被调用；用户字段不存在于 v3 planner input。
- capacity-domain identity 变化不改变生产候选顺序；本地 capacity overlay 仍有效。
- 候选数超过 hard cap 时在 outbound 前返回 `route_candidate_limit_exceeded`，不静默截断、不申请容量 lease、不消耗 retry；恰好达到上限时仍可完整规划，排序稳定且容量准入/数据库往返有界。
- instrumented store test 断言构建一个 snapshot 始终在同一 read transaction 内完成，数据库读取按明确 batch size 为 `O(ceil(candidate_rows / batch_size))`，而不是逐 Key 的 `O(candidate_rows)` N+1；并断言候选 cap 不会触发额外逐 Key 查询。
- `quality_unavailable` 不伪造乐观可靠性；可用评分因子按配置权重归一化，全部不可用时按稳定 `station_key_id` 排序并返回明确诊断。

### 9.6 P5 完成门

同一 snapshot 的排序结果完全确定；planner/admission/execution 不再读取容量域身份或旧探索字段；本地容量硬门和 Primary/Backup 层级测试通过；candidate plan 能提供完整的跳过原因。

## 10. P6：重试、故障转移和 proxy execution

### 10.1 目标

完成唯一 retry loop 的实现和测试；P8 切换前不发布新执行路径。切换后，`maxRetryCount` 是唯一请求级额外 Key 数量预算，`consecutiveFailureThreshold` 是同 Key 重试上限；两者必须与 snapshot、请求排除、ReplayGate、deadline 和 circuit admission 正确连接。

### 10.2 代码范围

- `src-tauri/src/services/proxy/execution.rs`
- `src-tauri/src/services/proxy/runtime.rs`
- `src-tauri/src/services/proxy/lifecycle/*`
- `src-tauri/src/application/routing_engine/request.rs`
- `src-tauri/src/application/routing_engine/admission.rs`
- `src-tauri/src/application/request_finalization/*`
- `src-tauri/src/models/routing.rs`
- loopback harness 和 request lifecycle tests

### 10.3 单请求循环

实现唯一的 request-local context：

```text
deadline_context
replay_gate_state
remaining_additional_key_count
outbound_attempt_count
next_attempt_index
attempted_station_key_ids
current_station_key_id
current_key_request_failure_count
correlation_id
runtime_generation
policy/quality/health/capacity revisions
commit_state
last_canonical_failure
```

初始化规则固定为：`remaining_additional_key_count=maxRetryCount`、`outbound_attempt_count=0`、`attempted_station_key_ids=empty`、`current_station_key_id=None`、`current_key_request_failure_count=0`、`commit_state=NotCommitted`。候选准入成功后创建一个 attempt slot 并递增 `attempt_index`；容量准入拒绝和未创建 slot 的规划循环不占用该索引。只有跨 outbound boundary 才递增 `outbound_attempt_count`；只有从已经真实尝试并 Open 的 Key 切到另一把不同 Key 时才使 `remaining_additional_key_count -= 1`。

每次 attempt：

1. 从当前 snapshot 取最高排序候选，并在一次准入事务中申请容量 lease/必要的 Half-Open lease；该事务同时 CAS 校验 Closed/Half-Open circuit 状态、Key lifecycle 和容量 revision，创建 attempt slot，并持久化 `candidate_admitted`。
2. 准入事务提交后即不可撤销；attempt owner 在即将跨 outbound boundary 时用同一 `attempt_id/event_id` 原子标记 `boundary_crossed=true`，outbound 完成后用同一 ID 生成 canonical outcome。若准入后、跨边界前发生取消、deadline、目标删除或本地连接/适配器错误，写入 `local_abandoned` 终态并释放资源，不生成 Key 失败样本或 circuit event。若由启动恢复或 lease reaper 代补跨边界后的未知结果，必须带 `recovery_origin` 且使用 `retry_disposition=StopRequest`，request loop 不得再为该 correlation 创建 outbound attempt。
3. 先提交 observation/circuit effect，再释放本次 attempt 的本地容量 lease（以及 Half-Open lease 的结果），再决定是否 retry；所有释放以 `attempt_id/lease_id` 幂等执行，写入失败不伪装成 Neutral。若 outbound 已成功但 observation/circuit 持久化失败，不能把成功改成失败或触发重试；必须设置该 Key 的持久化不可用 gate、将事件放入有界 outbox/backlog，并让后续规划按 fail-closed 跳过该 Key，直到明确健康读写成功。
4. 成功、严格确认余额不足、严格确认当前 Key 不支持请求模型，或请求安全边界拒绝重放时立即结束；这些终态不消耗额外 Key 名额。
5. 普通上游失败只有在本次 attempt 已完成、ReplayGate 允许且客户端尚未 commit 时才能继续。Closed Key 尚未达到 circuit 阈值时执行 `RetryCurrentKey`，保留同一 Key 且不消耗 `remaining_additional_key_count`；不得为同 Key retry 人为递增 runtime revision 或消耗全局 replan guard。
6. 当前失败使 Key Open，或下一次 durable circuit admission 发现它已经因跨请求历史 streak Open 时，才把当前 Key 加入 `attempted_station_key_ids`。若 `remaining_additional_key_count > 0`，使其减一并重新读取最新 snapshot 选择下一把不同 Key；否则按终态规则结束。未完成准入的容量拒绝和本次从未跨 outbound boundary 的 circuit skip 不消耗 Key 名额；model/endpoint/routing identity 变体不能绕过已经排除的 Key。
7. `outbound_attempt_count` 每跨一次 outbound boundary 加一；普通 Closed Key 的 raw hard limit 为 `min(min(1 + maxRetryCount, eligibleDistinctKeyCount) * consecutiveFailureThreshold, SYSTEM_RAW_ATTEMPT_HARD_CAP)`。Half-Open 每个 lease 只允许一次真实请求。任何本地准入或重规划循环都不得修改 raw attempt 或不同 Key 计数。

### 10.4 终态优先级

在判定优先级前固定三个计数口径：`configured_key_count` 是当前 routing scope 中存在且 lifecycle 有效的 Key 记录数（暂时禁用、凭据阻断和 circuit 状态仍计入）；`capability_match_count` 是其中支持当前模型/请求形态/协议的记录数（尚未应用用户禁用、凭据、circuit、容量和 request exclusion）；`candidate_cap_count` 是其中同时通过已启用站点、有效凭据和当前 Key lifecycle 的记录数，但仍未应用用户禁用、circuit、容量和本请求排除。`candidate_cap_count` 同时是系统 hard cap 的统计值和后续终态判断的候选基数；不得再引入语义相同的 `admission_candidate_count`。

1. 若 snapshot 构建发现候选总数超过 `MAX_OPERATIONAL_CANDIDATES`，优先返回 `route_candidate_limit_exceeded`（HTTP/错误 body 沿用现有候选上限契约），不创建 outbound attempt、不申请容量 lease、不消耗 retry；只有候选数未超过上限时才执行以下终态分类。
2. `configured_key_count=0` 时返回 `no_available_key`；存在 Key 但 `capability_match_count=0` 时返回能力/模型不匹配。
3. `capability_match_count>0` 但 `candidate_cap_count=0`（全部因站点未启用、凭据失效或 Key lifecycle 无效而被静态资格阻断）时返回 HTTP 503、`error.type=service_unavailable`、`error.code=no_available_key`，不能误报成能力/模型不匹配或容量不足。
4. `candidate_cap_count>0` 且仍存在至少一个未尝试候选，但这些未尝试候选全部因 Open 冷却、`quality_unavailable` 导致的 Half-Open score gate 拒绝、用户禁用、Half-Open lease 占用或 circuit 持久化 fail-closed 而不可用：HTTP 503，`error.type=service_unavailable`、`error.code=no_available_key`，不消耗剩余 retry。新请求面对所有 Key 已熔断时命中此分支。
5. `candidate_cap_count>0` 且存在未尝试的硬资格候选，每个候选唯一阻断原因为本地容量拒绝，或容量 registry/lease 服务当前不可读不可写：HTTP 503，`error.type=service_unavailable`、`error.code=route_capacity_exhausted`，诊断分别标记 `capacity_exhausted` 或 `capacity_state_unavailable`，不消耗 outbound retry。
6. 如果未尝试候选同时存在容量拒绝和非容量硬阻断，按非容量阻断处理为 `no_available_key`；不能把混合原因误报成纯容量不足。
7. 如果没有未尝试候选，且原因仅是本请求已经取得 admission/跨边界后产生的 `request_exclusion`，或 retry 耗尽、deadline 到期、ReplayGate 拒绝，返回最后一个安全 canonical failure；不得为了区分终态再次发送请求。当前请求刚使某 Key Open 也不改变这一条；后续新请求在没有未尝试且可用候选时才命中第 4 条的 `no_available_key`。

不得暴露内部 Key、容量域、完整 URL、上游原始 body 或 Retry-After 原文。

### 10.5 流和 commit 安全

- 已确认跨 outbound boundary、首字节前的 429/5xx/timeout/连接失败，在 ReplayGate 允许时可以换 Key。
- 出站边界前由 Relay 自己产生的连接建立、DNS、序列化或适配器错误，除非另有明确的本地 admission 规则，否则直接结束当前请求；不自动换 Key、不消耗 `maxRetryCount`，也不写入 Key 失败样本或 circuit。只有确认已跨出站边界的连接/传输失败，才使用 Key 级重试路径。
- 已开始输出后的流中断只结束并记录失败，不重放可能已提交的请求。
- deadline 覆盖排队、body/metadata、admission、规划 I/O、重规划等待、attempt 和 precommit；在途请求使用创建时的 snapshot。
- 每个 retry 的 trace 明确剩余预算、候选 rank、跳过原因和 canonical failure。

### 10.6 P6 测试

- `maxRetryCount=0/1/3` 对应最多 1/2/4 把不同 Key；同 Key raw attempt 由连续失败阈值控制。
- 本地容量准入拒绝、Half-Open lease 竞争和快照重读不增加 `outbound_attempt_count`，也不消耗 retry；每一次真正跨边界的发送恰好增加一次。
- 已确认跨 outbound boundary 的 429、502、5xx、timeout、连接失败和普通上游拒绝在 replay-safe 时先重试当前 Key，达到阈值 Open 后才换 Key；边界前的本地连接/适配器错误不伪装成 Key 失败，不安全或 commit 后不重放。
- 严格余额不足和严格确认当前 Key 不支持请求模型直接终止，不重试也不换 Key；其他普通上游错误不得自行新增第三套策略。
- 429 含 `Retry-After` 时不改变候选顺序、预算、冷却和作用域。
- `consecutiveFailureThreshold=3` 且 A、B 持续失败时，默认发送序列为 `A,A,A,B,B,B`；A Open 前不得提前发送 B。所有 Key Open 时直接 503 `no_available_key`。
- 最新 snapshot 分数变化影响下一次 retry；旧 snapshot 不影响已在途 attempt。
- capacity-only rejection 不消费 retry；全部容量不足返回现有公共 `route_capacity_exhausted`，内部诊断使用 `capacity_exhausted`。
- deadline、取消、lease race、迟到 result、流 idle 和 post-commit failure。
- 进程在 boundary 前后分别崩溃时的 attempt recovery；恢复只补一次 canonical outcome，不重放已持久化的 outbound attempt。
- outbound 成功后 observation/circuit 写入失败时，响应仍保持成功、不会重复发送；backlog、持久化不可用 gate、lease 释放和后续 fail-closed 诊断均可验证。
- 崩溃恢复/lease reaper 补写的 `upstream_uncertain` 只产生一次审计、质量/circuit 效果，不会被 request loop 当作普通可重试错误再次发送；普通 adapter 产生的 `upstream_uncertain` 仍按 ReplayGate 规则单独覆盖。
- 调用方请求的模型字段、显式 model mapping 和上游模型保持独立；未请求 `gpt-5.6-luna` 时不得由 planner/retry 自动改成 Luna。
- 终态分类覆盖：全量无模型硬资格、全部 Open/生命周期阻断、纯容量拒绝、容量与非容量混合阻断、以及因 request exclusion/预算耗尽返回最后 canonical failure；每个分支断言不额外发送请求且不错误消耗 retry。
- 终态还必须覆盖“没有配置/启用 Key”和“存在 Key 但没有能力匹配”两个分支，分别返回 `no_available_key` 与能力/模型不匹配，不互相混淆。
- `Retry-After`、上游 body、完整 URL、Authorization 和 Key secret 不出现在 public error、request log、decision trace、quality summary 或 metrics label。

### 10.7 P6 完成门

proxy 生产链路只有一个 retry loop 和一个 request budget owner；每次换 Key 都保留 request-local 状态并刷新 snapshot；429/502 的 request log、observation、circuit 和终态错误可以通过同一 correlation 串起来。

## 11. P7：设置页、站点编辑页、IPC 和诊断

### 11.1 设置页分组

修改：

- `src/features/routing/LocalRoutingSettingsEditor.tsx`
- `src/features/routing/useRoutingPolicyDraft.ts`
- `src/features/routing/LocalRoutingEditTab.tsx`
- `src/features/routing/LocalRoutingSettingsEditor.test.tsx`
- 相关 query/API/types、DemoBackend 和 generated bindings。

保留并调整分组：

1. **评分偏好**：四项评分权重、真实/监控质量来源权重、历史最小样本 15、最近最小样本 5、乐观可靠性 95%、乐观响应时间 2.5 秒；来源权重同时影响可靠性和响应速度，只影响质量评分，不影响成本、偏好或熔断。
2. **路由边界**：保留倍率、分组等现有语义。
3. **超时**：保留连接、首字节、提交前、缓冲执行、流空闲字段。
4. **熔断器设置**：恢复成功阈值、恢复等待时间（秒）；无启用/禁用开关。
5. **会话亲和**：保留当前开关、TTL、有界 bonus、hysteresis 和逃逸规则；前端说明不得把它误写成仅同分 tie-break。
6. **重试设置**：最大重试次数、连续失败阈值。

字段下方的 helper text 作为 UI 契约固定写入测试：最大重试次数说明“首把 Key 之外最多再尝试多少把 Key”；连续失败阈值说明“当前 Key 失败后继续重试，连续失败达到该次数后熔断并尝试下一把 Key，计数跨请求保留”。其他字段沿用已批准的简洁中文说明，不在前端暴露 ReplayGate、outbound boundary、hard cap、Half-Open/Closed 等内部字段名。

删除/隐藏：

- “候选与探索”、`maxCandidates`、exploration share；
- “错误率保护参数”、enabled/window/min samples/failure rate；
- 旧容量 retry/wait/cross-domain 控件；
- 中转站编辑页的容量域身份字段。

### 11.2 交互和错误

1. 每个输入都显示独立含义、单位、范围和默认值；超时字段不得只显示组级说明。
2. source weights 前端编辑时实时保持和为 100，保存仍由后端校验；不 silent clamp。
3. draft 使用已有 CAS/三方 dirty merge；P8 前保存只写 staged/旧 control-plane 兼容层，P8 后必须由 generation coordinator 创建并原子激活完整 generation；策略保存成功只影响新请求，在途请求使用旧 snapshot。
4. 覆盖 loading、saving、field validation、CAS conflict、external change、unavailable、disabled 和窄窗口状态；不显示完整 secret 或内部 stack。
5. 文案和行为明确：429 是当前 Key 的普通失败，阈值内重试当前 Key，达到阈值后才尝试下一把；不提站点级限流或容量域等待。

### 11.3 中转站编辑页

在 `src/features/stations/AddProviderPage.tsx`、`src/features/stations/pages/add-provider/AddProviderSections.tsx`、`src/features/stations/useAddProviderPageController.ts`、`src/lib/api/stations.ts` 及其测试中：

1. 删除 capacity-domain identity 表单区块、页面挂载、controller state/handlers 及保存/清除调用；页面不能通过隐藏控件或 effect 间接触发旧 API。
2. 不从编辑页加载 `providerFamily`、`deploymentIdentity`、`regionIdentity`；旧 API 只有在迁移/审计兼容边界保留，不能从生产页面 import 或调用。
3. 保留旧 API/表仅作为迁移/审计兼容，不进入生产 UI 读写路径。
4. 更新测试从“保存容量域”改为“字段不存在且其他 provider draft 流程不受影响”。

### 11.4 诊断 read model

显示但不允许编辑：effective score、score/quality revision、来源独立可靠性、门槛命中、乐观 basis、recent/history counts/mass、`idle_real_route_sample`、circuit state/cooldown/reopen level、Half-Open lease、raw attempt 与 dedup sample count、跳过原因和最终 canonical failure。

诊断必须区分：

- 无样本但使用乐观值；
- 监控不可比；
- 没有任何可比质量来源（`quality_unavailable`）及其确定性排序 fallback；
- Key Open/冷却；
- Half-Open score gate 拒绝；
- 本地容量不足；
- 全部 Key 不可用。

### 11.5 P7 测试

- v3 默认值、字段校验、错误路径、CAS conflict、draft merge 和保存生效边界。
- 所有删除控件不再渲染，旧 API 不被调用。
- 超时/样本/熔断/重试字段在窄窗口不重叠。
- 中转站编辑页容量域区块不存在，页面挂载和 controller 初始化不会调用旧 API，旧数据不会隐式保存或清除。
- 诊断 panel 覆盖 loading/error/empty/unavailable/`quality_unavailable`/optimistic/observed/Open/Half-Open/capacity。

## 12. P8：影子投影、重建和原子切换

### 12.1 目标

在生产 planner 使用 v3 前证明新摘要、circuit 状态和旧摘要的差异是可解释的；切换必须原子，不允许 planner 读到半个新 generation。P8 只负责首次切换；P8 后策略变更也必须遵循第 12.6 条的同一 generation coordinator，不允许回到直接改 active 行。

### 12.2 步骤

P8 的代码 owner 必须在实施记录中明确到模块，不得把切换逻辑散落在 planner、projector 和 migration 各自的副作用里。默认边界如下：

- generation domain/coordinator：`src-tauri/src/application/routing_generation.rs`、`src-tauri/src/application/routing_generation_coordinator.rs`；
- registry/read port：`src-tauri/src/models/routing_generation.rs`、`src-tauri/src/persistence/stores/routing_generation_store.rs`；
- quality/circuit rebuild runner：`src-tauri/src/background_tasks/routing_generation_rebuilder.rs`，分别调用 P3 的 `QualityProjectorV3` 和 P4 的 reducer replay API；
- fence、pointer CAS 和回滚：由 coordinator 单独拥有，不能由 UI、planner 或 migration 直接更新；
- P8 专项测试和脱敏 comparison/rollback report：`src-tauri/tests/` 与 `docs/audits/`，测试必须能独立复现中止、重放和原子切换。

如果现有模块名称不同，必须在实施记录中登记等价 owner 映射；不能因为目录不同而产生第二个 coordinator、projector 或 reducer。

P8 使用 P0 预留的 `0063_routing_runtime_generation.sql` 创建可保留多代记录的 generation registry；`status=active` 的唯一行就是当前 active 指针，使用 partial unique index/等价约束保证同一时刻最多一行 active。该 registry 不是可选优化：policy、quality 和 circuit 必须通过同一行 generation 原子切换，planner 只允许读取 `status=active` 的完整 generation。状态集合固定为 `building -> ready -> cutover_fencing -> active -> retired`，失败只能进入 `failed`；generation 的配置/重建内容在 `building` 后不可变，只允许带 CAS 的状态转换。`policy_generation_id`、`quality_generation_id`、`circuit_generation_id` 必须分别唯一解析到不可变的 staged policy、quality projection metadata/checkpoint 和 circuit rebuild metadata/checkpoint；不能只用可变 revision 或当前 summary 行充当 generation 身份。`0061`/`0062`/`0063` 的 schema 与 registry 必须为这些实体提供唯一键、状态、输入 watermark、input hash、output content hash 和 checkpoint 引用。active 之后 immutable observations、quality summary 和 circuit state 仍由各自唯一 owner 增量更新，并通过单调 `quality_revision`/`health_revision` 供 snapshot 读取；generation 中的 output content hash 是 activation/rebuild 指纹，不是要求每次增量写入后仍保持不变的 checksum。切换事务必须同时把旧 active 标记为 `retired`、把新 generation 标记为 `active`；不能把 registry 误建成只能保存一行的 singleton 表。

`0063` 的 schema migration 只创建表、约束、索引并插入一个 `pre_cutover` marker，不插入伪造的 v3 active row。启动时必须先读 marker：`pre_cutover + 无 active generation` 是合法的旧运行状态，继续使用旧完整 generation；`pre_cutover + 任意 active generation`、`v3_active + 非恰好一行 active`、active 行引用缺失/校验失败均是 typed `routing_generation_registry_corrupt` recovery，禁止猜测或回退。只有 pointer 事务成功后才能把 marker 改为 `v3_active`；此后空指针或非 `active` 指针一律 fail-closed，不得回到旧 planner。generation coordinator 必须在 fence 期间拒绝新的 candidate admission；这里的 admission 以容量 lease、circuit/Half-Open CAS 和 attempt slot 已持久化并写入 `candidate_admitted` 为准。新请求在此之前按第 0 节等待/自身 deadline 规则结束，已取得 admission 的请求可以完成并持有原 generation/lease，不因 fence 撤销。

1. 以 `routing_quality_v3` 创建 shadow quality generation，从 immutable observations 全量重建；在 pointer 切换前，旧质量摘要仍是唯一 planner 输入，P5/P6 的 v3 planner/execution 只在测试 harness 中运行。
2. 重建必须是可恢复的批量后台任务，而不是一次不可中断的大事务：
   - generation 初始为 `building`；可用内部 `build_run_id` 记录临时批处理，但**最终** `quality_generation_id` 只能在 fence 冻结最终 `input_observation_watermark` 后，连同 `evaluation_at`、`algorithm_version`、`quality_policy_revision` 和最终 `canonical_quality_input_hash` 一次性计算并写入。input hash 对纳入的 canonical 去重样本序列计算，output content hash 对稳定排序后的质量摘要序列计算，均不包含 secret 或非确定性字段；watermark/hash/ID 一经写入不可变；如果 fence 期间出现尾部事件导致输入变化，必须新建 generation 或重建受影响输出，不能原地修改 ready/building generation 的身份。
   - 按稳定 `(station_key_id, observation_id)` 游标分批读取，单批完成后在同一事务中幂等写入质量摘要、checkpoint、已处理计数和摘要 content hash；
   - 进程退出、数据库锁冲突或任务重试后从最后一个已提交 checkpoint 继续；重复批次只能通过唯一键 upsert，不能重复累加质量；
   - 任何 generation 的最终 `input_observation_watermark`/`input_circuit_event_watermark` 都必须在 fence 结束时确定；水位之后新写入的 observation/event 标记为 `Next`，不得混入当前 generation。若初始 build 已使用较早水位，尾部事件不能通过修改当前 generation 的 watermark/hash 追补，必须在最终水位上新建/重建 generation 并通过 checkpoint 复用已验证的中间结果；不能静默丢失或重复计算；
   - 全部批次完成后校验 observation 数量、去重计数、摘要 hash、算法/policy revision 和 checkpoint 连续性，再将 generation 标记 `ready`；任何校验失败标记 `failed`，不得被 planner 读取或激活。
3. 以新 reducer profile 重建 Key circuit 状态：
   - 输入必须是按 `attempt_id/event_id` 幂等保存的 raw canonical attempt/event（含 durable outbox backlog），不能使用质量去重后的样本计数代替；
   - circuit generation 同样从 `building` 开始；最终 `circuit_generation_id` 只能在 fence 冻结最终 `input_circuit_event_watermark` 后，连同 `circuit_policy_revision`、最终 `canonical_circuit_input_hash` 一次性计算并写入。按每个 Key/lifecycle 的 `reducer_commit_sequence` 游标分批重放，批次内幂等提交 reducer state、checkpoint、watermark、input hash 和输出 `circuit_content_hash`；input hash 对已应用 raw event 序列计算，output content hash 对稳定排序后的 circuit state 序列计算；进程重启或数据库锁冲突必须从最后 checkpoint 继续，不能重复累加 streak，也不能修改已经写入的 generation 身份。
   - 旧滑动窗口失败率不能直接当连续失败；按已应用事件的 reducer commit sequence 重放。无法证明历史事件顺序的 legacy 行只能保守保持 Open/人工恢复，并在 comparison report 标记 `legacy_order_uncertain`，不能自动 Closed；
   - 旧 Open 的 `cooldown_until` 可保留，但新状态带 v3 revision/reopen level；
   - 旧 Half-Open lease 一律 revision fence 取消；
4. 生成 comparison report：每个 Key 的旧/新 reliability、latency、score、样本数、source basis、circuit state 和排序差异；不包含 key secret。Key 标识必须使用稳定的 opaque commitment（例如带项目本地 secret salt 的 HMAC 截断值），不得直接输出数据库 ID、Key 文本或可逆编码；同一 report 内可关联，跨 report 不要求可关联。
5. 修正 comparison 发现的字段映射、时间窗口、去重和归责错误；不得通过调高乐观值掩盖实现错误。
6. 运行 TNTAPI/tkapi 等价回放：连续 502 和连续 429 都必须出现失败样本、连续失败和 Open/换 Key 证据。
7. 切换前执行一次有界的 generation fence：fence owner 先在同一 SQLite 写事务中把**候选新 generation**标记为 `cutover_fencing` 并递增 fence revision，observation writer/admission owner 每次写入或准入都必须在同一事务中读取该 revision；在 fence revision 之后创建的候选只能标记 `generation_eligibility=Next`，在 fence revision 之前已经提交 `candidate_admitted` 的 attempt 保持 `Active` 并允许完成。任何处于“读取 active 但尚未提交准入”的竞态请求在 CAS 失败后按第 0 节等待/自身 deadline 规则结束，不得标记为 Active。后台 ActiveProbe 可以继续写入，但必须带 `generation_eligibility=Next`，不得混入本次尾部重放。等待已跨 outbound boundary 的已准入 attempt 完成并记录，分别捕获最终 observation watermark 和 circuit event watermark；两者都使用第 0.2 节定义的全局 `ingestion_sequence`，而不是 producer 私有 sequence 或 wall-clock。将初始 watermark 到各自最终水位之间、且属于本 generation 的尾部事件重放到待激活 generation。质量尾部重放必须按受影响 Key 重新执行去重、窗口、`c` 和来源混合，不能只把新 mass 直接加到旧摘要；circuit 尾部重放必须按 raw event/idempotency 规则执行。使用代码拥有的 `system_cutover_fence_timeout_ms`，不新增用户设置；若无法在该上限内完成 drain、尾部重放或 policy revision CAS，以 CAS 将候选 generation 从 `cutover_fencing` 恢复为 `ready`、释放 fence 并保持 marker/当前 active generation 不变，不强行覆盖。
8. 在单一事务/active generation pointer 更新中同时切换 v3 policy、quality 和 Key circuit read model。`0063_routing_runtime_generation.sql` 创建多行 `routing_runtime_generation` registry（包含 `runtime_generation_id`、不可变 staged policy 的 `policy_generation_id`、总体 `policy_revision`、`quality_policy_revision`、`circuit_policy_revision`、`quality_generation_id`、`circuit_generation_id`、`algorithm_version`、`status`、`input_observation_watermark`、`input_circuit_event_watermark`、`policy_input_hash`、`quality_input_hash`、`circuit_input_hash`、`policy_content_hash`、`quality_content_hash`、`circuit_content_hash`、`checkpoint_ref`、`cutover_fence_revision`、`created_at`），并以 partial unique index/等价约束保证最多一行 `status=active`。`policy_generation_id` 必须能唯一解析到同一份已校验 staged policy，不能只凭可变的 revision 号查当前行。planner 只读取 active 行一次构造 snapshot，并在同一 read transaction 捕获当前 `quality_revision`/`health_revision`。只有 `status=ready` 且三者 revision/hash、算法版本和两个输入水位校验通过的 generation 才能进入 `cutover_fencing`；只有 fence drain、尾部重放和 pointer CAS 全部成功才可在同一事务改为 `active`。激活后的增量投影不修改 activation content hash，而是递增相应 read-model revision。planner snapshot 必须记录同一切换点的 policy/quality/health revisions，禁止出现 policy 已 v3 而 quality/health 仍读旧 generation 的混合快照。
9. 切换后保留有限时间的只读 comparison/rollback metadata；不继续让旧 planner 和新 planner 同时处理生产请求。未完成或 `failed` generation 只能清理其临时摘要，不能覆盖 active generation。

### 12.3 回滚

回滚只允许回到最近一个完整、经过校验且仍符合当前 v3 语义的 policy/quality/health generation；回滚候选由 `routing_runtime_generation.status=retired` 且仍被 retention 保护的行提供，不能从旧 `routing_policy_history` 临时拼装：

- 停止新策略写入；
- 保留已写入 immutable observations 和 attempt audit；
- 对候选回滚 generation 重放其创建 watermark 之后的全部 observation/circuit event，并重新校验 Key lifecycle/state revision；不能直接把较旧的 circuit 状态覆盖当前状态；
- 由 generation coordinator 在单一事务中把当前 active 标记为 `retired`、候选标记为 `active`，同步把 `routing_runtime_cutover_marker` 保持为 `v3_active`；不能只改 policy pointer 而留下旧 quality/circuit。
- P8 切换后的回滚不得切回只含 V2 错误率 breaker、容量域 fallback 或随机探索的旧 production generation；如果没有可用的 v3 generation，则保持 Key fail-closed、暂停新请求规划并报告恢复状态，而不是重新启用已删除语义；
- 回滚原因、source/target policy revision、generation ID、时间和影响范围写入 v3 append-only policy history/audit；不得把 v3 generation 回滚记录写成无法解析代际身份的旧 V2 history 行。

如果 v3 观测写入失败或 projector backlog 超过 hard threshold，保持当前 active generation 并暂停切换；不得降级成“样本为 0 但成功率正常”的伪状态。

### 12.4 运行时可观测性

切换前后只记录低基数、脱敏指标，不把 Key、URL、模型原文或请求正文作为 label：

- `routing_attempt_total{outcome,source}`；
- `routing_retry_total{failure_code,replay_safe}`；
- `routing_circuit_open_total{reason}`、`routing_circuit_reopen_total`；
- `routing_half_open_lease_conflict_total`、`routing_half_open_recovery_total{result}`；
- `routing_no_available_key_total`、`routing_capacity_exhausted_total`、`routing_capacity_state_unavailable_total`；
- `routing_quality_projection_lag`、`routing_observation_backlog`、`routing_observation_write_error_total`；
- `routing_generation_cutover_abort_total{reason}`；
- shadow 阶段的 v2/v3 排序差异数量和摘要重建失败数量。

指标契约在 P0 冻结并写入实施审计：所有 label（包括 `failure_code`）必须来自固定低基数枚举，禁止把 Key、URL、模型原文、correlation 或异常文本作为 label；本地采样周期 `60s`；`routing_quality_projection_lag` 以秒计，连续 3 个周期超过 `300s` 告警，超过 `900s` 或 backlog 超过代码常量 `MAX_PROJECTOR_BACKLOG=100_000` 时禁止 cutover；指标保留沿用现有本地 observability retention，不新增一套数据库 retention。发现 observation backlog、circuit persistence error、重复 event 激增或 `no_available_key` 异常上升时，暂停 v3 切换/回滚 generation，不通过放宽熔断或重试上限处理。告警阈值只影响切换和诊断，不改变请求级 retry 或 Key 评分。

### 12.5 P8 测试与完成门

- projector 在每个批次边界、进程退出、数据库锁冲突和重复重试后都能从 checkpoint 继续；质量摘要、去重计数、content hash 与单次全量重建一致，不出现重复累加。
- 初始 observation/circuit 两个 watermark、尾部 event watermark 和下一 generation 的增量边界可被分别断言；切换前产生的最后一条真实 502/429 既不会丢失，也不会被重复计算。
- shadow rebuild 期间发生 policy mutation、Key 删除/重新绑定或 circuit event 并发写入时，revision/CAS fence 能使切换中止或安全重放；不能覆盖新生命周期状态。
- active pointer 更新是单事务可见的：读者只能看到旧完整 generation 或新完整 generation，不能看到 `building`/`failed` 或 policy/quality/health 混合版本。
- 回滚只选择完整 v3 generation；没有可用 generation 时新请求按 fail-closed 处理，并有明确诊断和恢复指标。
- P8 后分别修改评分权重、quality sampling/source weight、circuit threshold/wait 和 timeout，断言 coordinator 走复用或重建的正确分支，且 planner 永远只读完整 generation。
- active 后收到新 observation/circuit event 时，activation content hash 保持为切换指纹，而 `quality_revision`/`health_revision` 单调递增；planner snapshot 能看到同一 read transaction 的 revision 对，不把正常增量更新误判为 generation 损坏。

P8 只有在上述测试、comparison report、TNTAPI/429 回放和 pointer 原子性检查全部通过后，才允许把 v3 generation 标记 `active`。

### 12.6 P8 后策略变更协议

P8 激活 `routing_runtime_generation` 后，任何设置保存都必须经过同一个 `RoutingPolicyMutationCoordinator`，禁止直接写 active policy 行：

1. 以当前 active generation 的 `runtime_generation_id` 和 policy revision 做 CAS，写入新的 staged policy；CAS 冲突只返回冲突，不覆盖外部修改。新 staged 行必须记录 `source_config_revision=当前 active policy_revision` 和 CAS 产生的唯一 `target_policy_revision`，并按第 0.2 节重新计算 `policy_generation_id`。
2. 仅改变评分权重、`allowDepletedFallback`/倍率/分组/出站代理等路由边界、亲和、`retry.maxRetryCount` 或 timeout 的变更，不改变 quality/circuit 输入，因此可以复用当前 `quality_generation_id`/`circuit_generation_id` 及其各自的 `quality_policy_revision`/`circuit_policy_revision`，创建新的 `ready` runtime generation 后原子切换；timeout 的运行时热加载仍遵循“新请求使用新快照，在途请求使用旧快照”。
3. 改变 source weights、样本门槛、乐观值或 quality algorithm version 的变更，必须提升 `quality_policy_revision` 并完成新的 quality projection；projector 未 `ready` 时 policy 只能保持 staged，不能让 planner 读取新 policy 搭配旧 quality。
4. 改变连续失败/恢复阈值或等待时间的变更必须提升 `circuit_policy_revision` 和总体 policy revision，并触发 P4 的 Half-Open lease fence；已有 Open 的绝对 cooldown 不缩短，后续新事件使用新阈值。
5. 新 generation 激活失败时保留旧 active generation，UI 显示保存失败/待重建状态；不得回退到 v2 planner，也不得修改 immutable observations。

## 13. P9：删除旧 owner 和完整验证

### 13.1 删除清单

在所有 v3 证据通过后再执行：

- production `weighted_rendezvous`、utility band、exploration budget/seed consumer；
- `ErrorRateProtectionService` 默认关闭旁路和旧 failure-rate reducer wiring；
- `BetaPrior`、`reliability_prior_alpha/beta` production consumer；
- `GenericStatus -> Neutral` 的可归责上游失败路径；
- V2 retry/protection 字段的 runtime read path；
- capacity-only `RetrySameTarget`、capacity-wait/`WaitThenReplan`、`try_different_failure_domain` 等旧动作在 production request loop 中的消费者；新的统一同 Key 动作必须命名为 `RetryCurrentKey`，旧 `retry_same_target` 如仍需历史反序列化，只能保留为非生产审计兼容值；
- capacity-domain identity 的 planner/admission/execution read path；
- `capacity_unavailable` 旧 production rejection literal；只允许在迁移/审计兼容边界转换为 `capacity_state_unavailable`；
- runtime load/anomaly penalty 在 planner/dispatch utility 中的 production consumer；只保留本地容量 registry/admission overlay；
- 重复的 retry budget/action/planner owner。

删除前必须用 `rg`、architecture test 和编译器 dead-code 结果确认没有消费者。对 `capacity_domain`/`capacity_unavailable` 的全仓命中必须逐文件分类：生产 planner、admission、execution、upstream adapter 和页面调用属于删除范围；迁移、历史 audit、旧 DTO/API 只有在明确 allowlist 中才能暂留，并标注“非生产读路径”和删除前提。旧表、迁移 audit、历史 DTO 可以暂留，但必须标注“非生产读路径”和删除前提。

完成删除后更新 `docs/audits/` 删除台账和本计划状态；只有实现、迁移、切换和验证证据齐全时，才在 `docs/README.md` 或目标规范中把状态从 Proposed/Draft 改为已实施。计划文件本身不提前宣称完成。

### 13.2 必跑验证

Rust/Tauri：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_policy -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib routing_engine -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib quality_projection -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib health_protection -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib request_finalization -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture --test-threads=1
cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_quality_projection -- --nocapture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_health_verdict_persistence -- --nocapture
```

前端/契约：

```powershell
pnpm.cmd test -- src/features/routing/LocalRoutingSettingsEditor.test.tsx src/features/routing/RoutingStatusDiagnosticsPanel.test.tsx src/lib/queries/routingQueries.test.ts
pnpm.cmd test:contracts
pnpm.cmd generate:bindings --check
pnpm.cmd build
```

跨层：

```powershell
pnpm.cmd verify:fast
pnpm.cmd verify:full
```

专项 fixture 必须覆盖：

- 502/429/5xx/timeout/connection failure；
- 0/1/3 retry、deadline、ReplayGate、commit；
- request dedupe 与 raw attempt；
- `w(a)` 和 recent/history 公式；
- 70/30 及不可比来源归一化；
- 连续失败 Open、递增 cooldown、Half-Open 单 lease、连续成功 Close、迟到结果；
- policy migration/rollback/CAS；
- capacity-only admission 和 `no_available_key`；
- planner 顺序稳定性、tier 边界、现有 affinity bonus/hysteresis/逃逸、quality_unavailable fallback 和达到 `MAX_OPERATIONAL_CANDIDATES` hard cap；
- 中转站编辑页容量域字段移除。

如果命令因已有工作区问题失败，必须记录真实失败原因和未验证范围；不能在计划执行记录中写“验证通过”。

## 14. 完成定义

本计划只有在以下条件全部满足后才可标记完成：

1. v3 policy、migration、rollback、generated bindings 和字段级验证均有测试证据。
2. 真实 429/502 等可归责失败进入统一 observation、质量统计和 Key circuit；TNTAPI 类场景不再出现样本为 0 且 Key 永远不跳过。
3. quality projector 使用去重后的请求样本、明确 `w(a)`、recent/history 门槛、乐观值、fixed-point 和 source weight；没有 Beta prior production consumer。
4. planner 同一硬层内严格按 score 降序，容量只作为后置本地准入；没有 rendezvous、随机探索或容量域生产读路径。
5. retry budget、request exclusion、ReplayGate、deadline、commit safety 和终态错误优先级全部由 loopback/integration tests 固定。
6. circuit 的 Open/cooldown/Half-Open/Closed、单真实请求 lease、重启、CAS、幂等和迟到结果测试通过。
7. 前端只展示 v3 可调字段；候选/探索、错误率保护和容量域编辑入口删除；每个超时字段有独立说明。
8. 诊断能解释 score basis、来源样本、429/502 失败、熔断、冷却、Half-Open、容量不足和无可用 Key。
9. 新增公共 `no_available_key` 错误码的 HTTP/IPC/客户端映射已覆盖；容量终态仍使用 `route_capacity_exhausted`，并能区分容量耗尽与容量状态不可用。
10. 旧 production owner 已删除或明确只剩迁移/审计用途，删除台账、README 状态和实施记录一致。
11. 必跑验证命令真实通过，或在交付记录中明确列出失败命令、原因、影响和后续动作。

## 15. 不得做的事情

- 不通过降低 `consecutiveFailureThreshold`、提高乐观值或隐藏错误来掩盖 provider 故障。
- 不把 429 升级成站点/账号/容量域故障，也不根据 `Retry-After` 创建特殊熔断路径。
- 不为了恢复低分 Key 恢复随机探索、synthetic probe 或隐式轮换。
- 不把 monitoring 样本直接覆盖真实请求样本，不用固定 evidence mass 伪造 70/30。
- 不把 raw attempt 数量当作质量样本数，不因重试次数放大可靠性。
- 不在 UI 复制后端资格/容量判断，不把旧 capacity-domain 表的存在误认为生产功能仍启用。
- 不保留两个 retry budget、两个 circuit reducer、两个 planner 或一个“兼容”旁路继续接收生产请求。
- 不在未完成 migration/重建/回滚证据前标记 Implemented。
