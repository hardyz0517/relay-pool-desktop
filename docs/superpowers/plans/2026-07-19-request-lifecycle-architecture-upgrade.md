# Relay Pool 请求生命周期架构升级实施计划

日期：2026-07-19
状态：实施中；Task 2/4/11 的 test-only slice 已完成，production cutover 待 Persistence V2
对应设计：`docs/superpowers/specs/2026-07-19-request-lifecycle-architecture-upgrade-design.md`
上位依赖：`docs/superpowers/plans/2026-07-18-persistence-architecture-v2-upgrade.md`

当前执行记录（2026-07-19）：

- 已完成：crate-private Request/Attempt lifecycle kernel、`Admitted` admission phase、一次终结 invariant。
- 已完成：Responses/Chat SSE protocol machine、显式 terminal、普通 EOF=incomplete、partial/malformed fixture tests。
- 已完成：consumer-owned in-memory lifecycle port、单有序 bounded writer、request/attempt capacity reservation、ack 和 unhealthy admission gate tests。
- 未开始：Persistence V2 runtime/schema/store、ingress production cutover、LifecycleBody cutover、旧 authority 删除、真实 authenticated E2E。
- 硬门禁：仓库当前没有 `src-tauri/src/persistence` 或 `src-tauri/src/application`；在该前置依赖完成前禁止新增 production SQL adapter 或接入旧 `AppDatabase`。

## 1. 审计结论

当前实现不能判定为符合可靠性、可维护性、可扩展性原则。目标设计在本次复核后满足架构原则，但“设计合理”不等于“实现已达标”；只有本计划全部 release gate 通过后才能宣布升级完成。

| 维度 | 当前实现 | 目标设计 | 必须通过的最终证据 |
|---|---|---|---|
| 可维护性 | 不符合。request、attempt、protocol、delivery 和 persistence 共同修改一个可变 outcome | 符合。四个窄 owner、typed event、单向依赖、单 writer port | 旧 symbol 删除；CodeGraph/AST 无反向依赖、无新上帝对象 |
| 可扩展性 | 不符合。endpoint/stream 特例分散在 execution、runtime、body 和 observer | 符合。`ResponsePlan` + sealed protocol contract + canonical attempt result | 新增一个 fixture-only compatibility contract 不修改 retry loop |
| 可靠性 | 不符合。前置失败不反馈、EOF 可假完成、lease 提前释放、writer 可丢写且不可 drain | 有条件符合。durable admission、逐 attempt 终态、显式协议终态、有序有界 writer | fault、shutdown、soak、真实客户端、真实 SQLite 三方事实一致 |

根因不是三个互不相干的 bug，而是“终态事实的 owner 和 commit 时机没有被建模”：

1. `execution.rs` 在协议未结束时构造 success；`response_body.rs` 再改写它；`runtime.rs` 还构造未消费的伪 outcome。
2. retry loop 只把每次尝试写进 `AttemptTrace`，持久化和健康接口却只接受一个最终 feedback。
3. HTTP headers、首字节、协议 terminal、transport EOF 和 response body 完成没有独立事实。
4. `RequestLease` 属于请求输入对象，stream response 返回后会提前 drop。
5. 当前 finalization 的多路径入队、fallback spawn、永久内存去重和无 drain 令“记录成功”无法证明。

本计划不允许用以下方式缩短工期：

- 不以 non-stream fallback 隐藏 stream contract 缺陷；
- 不新增 `AppDatabase` 方法或临时 production SQL adapter；
- 不运行 V1/V2 dual write、shadow write 或按请求切换旧/新 lifecycle；
- 不保留新的通用 `LifecycleManager`、`ProxyContext` 或类似 service locator；
- 不把 source-string 检查冒充行为测试；
- 不以 mock-only 绿色代替真实 loopback、真实 app 和真实 SQLite 验证。

## 2. 执行纪律

每个任务严格执行以下循环：

1. 先写最小可复现的 RED test，保存失败名称和关键输出。
2. 一次只迁移一个 owner 或一条合同，不同时重写不相关模块。
3. 运行任务内 focused GREEN，再运行最近一级 regression。
4. 检查 `git diff -- <exact paths>`，不覆盖当前工作区已有改动。
5. 达到 checkpoint 后再进入下一任务；任何失败不得用 timeout 当通过。

工作区规则：

- 只编辑任务列出的文件；发现重叠的用户改动时逐 hunk 合并。
- 不使用 `git add .` 或 `git add -A`。
- 本计划执行本身不授权 commit、push、release 或发布。
- Windows 上 Cargo 命令顺序执行；发现 build lock 时先检查残留 `cargo`、`rustc`、`link` 进程。
- 测试日志、fixture 和 SQLite 查询不得输出 bearer、API key、Cookie、完整 body 或上游错误页。

## 3. 依赖与切换顺序

```text
行为基线/RED tests
  -> 纯 Lifecycle kernel -----------+
  -> Protocol fixtures/machines ----+-> ResponsePlan
                                     -> Attempt lifecycle/classifier
                                     -> Runtime health overlay
                                     -> Models fan-out

Persistence V2 runtime/ports
  -> request/attempt schema
  -> ordered LifecycleWriter

Lifecycle kernel + writer
  -> ingress durable admission + RequestLease transfer
  -> LifecycleBody cutover
  -> 删除旧 outcome/finalization path
  -> fault/concurrency/soak
  -> authenticated live E2E + SQLite verification
  -> compatibility deletion + architecture gate
  -> release validation
```

Domain 和 protocol 可先依赖 in-memory fake。production cutover 必须等待 Persistence V2 consumer-owned port 可用；若依赖未就绪，停在 test-only 集成，不得造临时 production 数据库通道。

## 4. Task 0：冻结边界并保存可复核基线

文件：

- Create: `docs/superpowers/audits/request-lifecycle-baseline.md`
- Create: `docs/superpowers/audits/request-lifecycle-boundary-manifest.json`
- Create: `docs/superpowers/audits/request-lifecycle-deletion-ledger.md`
- Modify: `docs/superpowers/specs/2026-07-18-persistence-architecture-v2-upgrade-design.md`
- Modify: `docs/superpowers/plans/2026-07-18-persistence-architecture-v2-upgrade.md`
- Modify: `src-tauri/src/services/proxy/mod.rs`，只加 freeze 注释或测试 module registration；不改行为

步骤：

- [ ] 用 CodeGraph 保存 `FinalRequestOutcome`、`CandidateFeedback`、`AttemptTrace`、`ResponseMode`、`FinalizationDispatcher`、`RequestLease` 的 definition/callers/impact。
- [ ] 在 baseline 中记录当前调用链和已确认的具体位置，不凭记忆重写行号。
- [ ] 记录 current fan-in/fan-out、module edge、public export 和 persistence consumer。
- [ ] deletion ledger 为每个旧 symbol 指定 replacement、删除 task 和最终检索条件。
- [ ] 把 Persistence V2 的旧“单 final outcome + 单 health feedback”条款修订为 request start、逐 attempt+health、request terminal 三类事务，并保持其 single-writer/port/idempotency/drain 约束。
- [ ] 冻结旧类型：后续任务不得给它们增加字段、构造器或新 consumer。
- [ ] 保存当前 focused test、完整 proxy test、Cargo check 的真实基线；已有失败单独列出，不归因于本升级。

基线命令，顺序执行：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

退出条件：manifest 无 `unknown owner`；每个旧 authority 都有明确删除 task；基线失败有原始输出和责任边界。

## 5. Task 1：为三项 P0 建立 characterization RED tests

文件：

- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/services/proxy/response_body.rs`
- Modify: `src-tauri/src/services/proxy/responses_chat_stream.rs`
- Modify: `src-tauri/src/services/proxy/ingress.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Create: `src-tauri/tests/proxy_lifecycle_characterization.rs`

先写且确认失败：

- [ ] `failed_candidate_is_recorded_before_successful_fallback`：A 401、B 成功，断言 A/B 都有独立反馈。
- [ ] `all_failed_candidates_each_have_terminal_feedback`：A timeout、B 5xx、C 429，断言三条 attempt terminal。
- [ ] `chat_bridge_eof_without_done_is_incomplete`：event boundary 干净但无 `[DONE]`，不得合成 `response.completed`。
- [ ] `responses_failed_and_incomplete_are_protocol_terminals`：不能只观察 completed。
- [ ] `request_lease_survives_until_stream_body_drop`：handler 返回 response 后 active request 仍为 1。
- [ ] `authenticated_unknown_route_has_lifecycle_terminal`：当前裸 fallback 应暴露缺口。
- [ ] `post_terminal_transport_error_does_not_erase_candidate_success`。
- [ ] `proxy_http_response_outcome_is_not_a_second_authority`：先作为 deletion characterization，最终转为结构门禁。

运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_characterization -- --nocapture
```

RED 证据要求：每个测试必须因目标缺陷失败，不得因 fixture 无效、panic setup、端口冲突或超时失败。若当前行为与假设不一致，先更新审计，不修改实现硬凑 RED。

退出条件：三项 P0、lease、fallback 和 terminal-after-error 均有可重复的失败测试。

## 6. Task 2：实现纯 Request/Attempt lifecycle kernel

文件：

- Create: `src-tauri/src/services/proxy/lifecycle/mod.rs`
- Create: `src-tauri/src/services/proxy/lifecycle/request.rs`
- Create: `src-tauri/src/services/proxy/lifecycle/attempt.rs`
- Create: `src-tauri/src/services/proxy/lifecycle/ports.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/error.rs`
- Create: `src-tauri/tests/proxy_lifecycle_domain.rs`

RED：

- [ ] 每个合法 request transition 的 table test。
- [ ] terminal 后任何 transition 返回 typed invariant error。
- [ ] `(request_id, ordinal)` 产生稳定 AttemptId。
- [ ] attempt 最多 terminal 一次；duplicate event 返回幂等结果或明确 invariant error。
- [ ] request completion 组合矩阵覆盖 Completed、PartialSuccess、Failed、Interrupted。
- [ ] reducer 不接受 raw HTTP body、Reqwest error string 或数据库 DTO。

GREEN：

- [ ] 实现 `RequestContext`、`RequestPhase`、`RequestTerminal`、`FinalRequestRecord`。
- [ ] 实现 `AttemptContext`、`AttemptPhase`、`AttemptTerminal`、`AttemptTerminalRecord`。
- [ ] port 只接收 canonical records，不暴露 SQLx、Axum、Reqwest 或 `AppDatabase`。
- [ ] 类型默认 `pub(crate)`；不为 integration test 扩大 production visibility，测试通过 crate-local harness 或明确 test support。

运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_domain -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::lifecycle -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

退出条件：纯状态机无 I/O；非法组合无法通过构造或 exhaustive reducer。

## 7. Task 3：固化协议来源与真实 fixture

文件：

- Create: `src-tauri/tests/fixtures/proxy_protocol/provenance.json`
- Create: `src-tauri/tests/fixtures/proxy_protocol/responses/*.sse`
- Create: `src-tauri/tests/fixtures/proxy_protocol/responses/*.json`
- Create: `src-tauri/tests/fixtures/proxy_protocol/chat/*.sse`
- Create: `src-tauri/tests/fixtures/proxy_protocol/chat/*.json`
- Create: `src-tauri/tests/fixtures/proxy_protocol/embeddings/*.json`
- Create: `src-tauri/tests/fixtures/proxy_protocol/models/*.json`
- Create: `docs/superpowers/audits/openai-protocol-contract-2026-07-19.md`

步骤：

- [ ] 从当前官方 OpenAI API schema/docs 固化 completed、failed、incomplete、`[DONE]` 和 non-stream envelope 证据。
- [ ] `provenance.json` 记录 URL、抓取日期、合同摘要、fixture SHA-256；不保存认证信息或真实用户 payload。
- [ ] 兼容站点 fixture 单独命名 provider/format，不混入 official fixture。
- [ ] 为跨 chunk、同 chunk 多 event、partial event、malformed JSON、超限 pending buffer 创建最小 fixture。
- [ ] 人工核对 fixture 语义后再开始 protocol machine；禁止根据当前实现反向生成“证明自己正确”的 fixture。

退出条件：每个成功/失败/不完整 terminal 都有来源；兼容行为与官方合同可区分。

## 8. Task 4：实现 ProtocolMachine

文件：

- Create: `src-tauri/src/services/proxy/lifecycle/protocol.rs`
- Create: `src-tauri/src/services/proxy/protocol/mod.rs`
- Create: `src-tauri/src/services/proxy/protocol/responses_json.rs`
- Create: `src-tauri/src/services/proxy/protocol/responses_sse.rs`
- Create: `src-tauri/src/services/proxy/protocol/chat_json.rs`
- Create: `src-tauri/src/services/proxy/protocol/chat_sse.rs`
- Create: `src-tauri/src/services/proxy/protocol/embeddings_json.rs`
- Create: `src-tauri/src/services/proxy/protocol/models_json.rs`
- Create: `src-tauri/src/services/proxy/protocol/local.rs`
- Create: `src-tauri/tests/proxy_protocol_contracts.rs`

RED：

- [ ] Responses SSE completed、failed、incomplete、terminal 前 EOF。
- [ ] Chat SSE `[DONE]`、无 terminal EOF、partial event、malformed event。
- [ ] terminal 后 EOF/error 不改变已封闭 ProtocolTerminal。
- [ ] buffered 2xx malformed JSON 和 error envelope 不成功。
- [ ] Responses/Chat/Embeddings/Models envelope 各自验证必要字段。
- [ ] 一次 terminal 后重复 terminal 产生 typed duplicate/invariant 结果，不双反馈。

GREEN：

- [ ] `ProtocolMachine` 只消费 headers/chunk/EOF/transport error facts。
- [ ] machine terminal 后 sealed；transport terminal 单独记录，不回写 protocol terminal。
- [ ] shared SSE framing 只负责 frame，不决定 endpoint success。
- [ ] provider compatibility 通过 sealed、具名 `CompletionPolicy`，每项带 retirement condition。

运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_protocol_contracts -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::protocol -- --nocapture
```

退出条件：没有协议使用通用 EOF-success；fixture matrix 全绿。

## 9. Task 5：用 ResponsePlan 替换 ResponseMode

文件：

- Modify: `src-tauri/src/services/proxy/endpoint_adapter.rs`
- Modify: `src-tauri/src/services/proxy/adapters/openai.rs`
- Modify: `src-tauri/src/services/proxy/adapters/responses.rs`
- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/services/proxy/responses_chat_fallback.rs`
- Modify: `src-tauri/src/services/proxy/responses_chat_stream.rs`
- Modify: `src-tauri/src/services/proxy/request.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`

RED：

- [ ] endpoint x upstream format x stream 的完整 plan table。
- [ ] Router/Scheduler 不读取 ResponsePlan。
- [ ] Chat-to-Responses 普通 EOF 不生成 downstream completed。
- [ ] upstream protocol terminal 后 transform 才能生成对应 downstream terminal。

GREEN：

- [ ] 引入 `TransportMode`、`UpstreamProtocol`、`DownstreamTransform`、`CompletionPolicy`。
- [ ] adapter 编译 ResponsePlan；execution 只携带 plan，不做 endpoint terminal 判断。
- [ ] 暂保留 `ResponseMode` 适配仅限 test/cutover 内部，deletion ledger 标明 Task 16 删除；不得增加 production 分支。

运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::endpoint_adapter -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::responses_chat_stream -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_protocol_contracts -- --nocapture
```

退出条件：协议选择集中于 adapter 编译，retry loop 不复制 endpoint 分支。

## 10. Task 6：拆分 retry、health 和 blame classifier

文件：

- Modify: `src-tauri/src/services/proxy/lifecycle/attempt.rs`
- Modify: `src-tauri/src/services/proxy/routing_failure.rs`
- Modify: `src-tauri/src/services/proxy/error.rs`
- Modify: `src-tauri/src/services/proxy/execution.rs`
- Create: `src-tauri/tests/proxy_attempt_classification.rs`

RED table 至少包含：

- [ ] 401/403 => stop-or-next typed policy + HardFail，不能由 `output_started` 抹成 Neutral。
- [ ] 402/balance => HardFail。
- [ ] 429 => Cooldown 并保留 retry-after。
- [ ] connect/timeout/5xx => ObserveFailure，retry 单独决定。
- [ ] request-only 400/404/409/422 => Neutral + StopRequest。
- [ ] protocol incomplete/malformed 在 commit 前后拥有不同 retry，但 health 保持上游事实。
- [ ] downstream drop/local persistence failure => Neutral。
- [ ] protocol terminal 后 transport error => candidate Success + 独立 delivery/transport diagnostic。

GREEN：

- [ ] 实现 `ClassifiedAttemptFailure { kind, blame, retry, health, ... }`。
- [ ] 删除任何用 error string 包含关系推导 retry/health 的新路径。
- [ ] `output_started` 迁移为明确 `downstream_committed`，仅作为 RetryPolicy 输入。

运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_attempt_classification -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::routing_failure -- --nocapture
```

退出条件：retry 和 health 的 Cartesian matrix 可独立审阅；无 boolean 同时控制两者。

## 11. Task 7：ExecutionEngine 接入逐 attempt lifecycle

文件：

- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/services/proxy/observability.rs`
- Modify: `src-tauri/src/services/proxy/routing_repository.rs`
- Modify: `src-tauri/src/services/proxy/test_support.rs`
- Modify: `src-tauri/tests/proxy_lifecycle_characterization.rs`

RED：

- [ ] A 失败 B 成功产生 ordinal 0/1 两条 terminal。
- [ ] A/B/C 全失败产生三条 terminal，request 只生成一个 terminal。
- [ ] adapter prepare/client build/connect/status/parse 每条早退路径终结 attempt。
- [ ] commit 后 failure 不 fallback。

GREEN：

- [ ] candidate 网络调用前创建 AttemptLifecycle。
- [ ] pre-commit failure 立即 terminalize 并调用 attempt port。
- [ ] execution 返回 prepared response + lifecycle handles，不构造 FinalRequestOutcome。
- [ ] `AttemptTrace` 降级为由 canonical attempt record 投影的展示 DTO，不再是权威。

运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_characterization failed_candidate_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::execution -- --nocapture
```

退出条件：所有执行早退路径都有 attempt terminal；当前 GREEN 可用 in-memory store，不接 production SQL。

## 12. Task 8：建立 runtime health overlay

文件：

- Create: `src-tauri/src/services/proxy/routing_health_overlay.rs`
- Modify: `src-tauri/src/services/proxy/routing_health.rs`
- Modify: `src-tauri/src/services/proxy/routing_snapshot.rs`
- Modify: `src-tauri/src/services/proxy/scheduler/eligibility.rs`
- Modify: `src-tauri/src/services/proxy/execution.rs`
- Create: `src-tauri/tests/proxy_health_overlay.rs`

RED：

- [ ] A 失败后，在 durable snapshot 刷新前并发 request 不再选择 A。
- [ ] 同一 attempt_id 重放不重复惩罚。
- [ ] endpoint revision 变化使旧 overlay/ack 无法覆盖新配置。
- [ ] persistence failure 时 overlay 保持保守状态且 admission 关闭。
- [ ] overlay 有界；配置删除或 durable watermark 推进后可清理。

GREEN：

- [ ] overlay key 为 `(station_key_id, endpoint_revision)`。
- [ ] scheduler 只读取 canonical `durable snapshot + overlay` view。
- [ ] health transition 仍只有一个实现；Router 不解析错误。

运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_health_overlay -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::scheduler -- --nocapture
```

退出条件：并发选路立即观察失败 key，overlay 无永久增长或第二套 cooldown 规则。

## 13. Task 9：将 `/v1/models` fan-out 接入统一 attempt 模型

文件：

- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/services/proxy/protocol/models_json.rs`
- Modify: `src-tauri/src/services/proxy/observability.rs`
- Create: `src-tauri/tests/proxy_models_lifecycle.rs`

RED：

- [ ] 三候选一成功两失败 => 三条 attempt + request PartialSuccess。
- [ ] 全失败 => request Failed，每个失败独立 health effect。
- [ ] capacity 不能覆盖整批时，任何 upstream call count 都为 0。
- [ ] fan-out cancellation/timeout 终结每个已启动 attempt。

GREEN：

- [ ] 在启动 batch 前一次性预留全部 attempt terminal permits。
- [ ] 保留并发查询，但结果只通过 AttemptTerminalRecord 聚合。
- [ ] `fallback_count` 不再冒充 fan-out count。

运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_models_lifecycle -- --nocapture
```

退出条件：models 不再有旁路反馈/日志路径；半批启动不可能发生。

## 14. Task 10：Persistence V2 schema 和 consumer-owned store

前置条件：Persistence V2 的 `PersistenceRuntime`、`WriteCoordinator`、`WriteSession` 和 migration infrastructure 已完成。未满足则本任务阻塞 production cutover。

文件：

- Modify/Create: `src-tauri/src/persistence/migrations/0005_request_logs.sql`
- Modify/Create: `src-tauri/src/persistence/stores/request_log_store.rs`
- Modify/Create: `src-tauri/src/application/request_finalization.rs`
- Modify: `src-tauri/src/persistence/stores/mod.rs`
- Modify: `src-tauri/src/application/mod.rs`
- Modify: `src-tauri/src/services/proxy/lifecycle/ports.rs`
- Modify: `docs/superpowers/audits/persistence-v2-boundary-manifest.json`
- Create: `src-tauri/tests/proxy_lifecycle_persistence.rs`
- Modify: `.sqlx/**`，仅由 `cargo sqlx prepare` 生成的相关 metadata

Schema：

- [ ] `request_logs` 支持 authenticated durable start、terminal CAS、stale in-progress recovery。
- [ ] `request_attempts` 具备 `UNIQUE(request_id, attempt_ordinal)` 和 parent FK。
- [ ] attempt row 保存 canonical retry/health/blame/protocol/transport/delivery evidence 的必要投影。
- [ ] retention 删除 request 时级联 attempts；查询必须以 request_id + page limit 有界。
- [ ] released-schema fixture 和 SQLx offline metadata 更新。

事务 RED：

- [ ] request start duplicate 的 affected rows 语义明确。
- [ ] request-start ack timeout/connection loss 按 request_id reconcile：row 存在则终结 `admission_aborted`，不存在才确认 admission rejection。
- [ ] request terminal CAS 最多成功一次。
- [ ] attempt insert + endpoint revision validate + health transition 同事务 rollback。
- [ ] stale revision 记录 attempt 但不污染新 health。
- [ ] commit outcome unknown 通过 idempotency key/state precondition reconcile。
- [ ] 进程重启后 duplicate 仍由数据库约束抑制，不依赖 HashSet。

运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_persistence -- --nocapture
powershell -ExecutionPolicy Bypass -File scripts/prepare-sqlx.ps1 -Check
```

退出条件：无新 `AppDatabase` method、无 rusqlite proxy adapter、无 dual write；数据库是唯一 durable 幂等权威。

## 15. Task 11：实现单一有序 LifecycleWriter

文件：

- Create: `src-tauri/src/services/proxy/lifecycle/writer.rs`
- Modify: `src-tauri/src/services/proxy/lifecycle/mod.rs`
- Modify: `src-tauri/src/services/proxy/lifecycle/ports.rs`
- Modify: `src-tauri/src/application/request_finalization.rs`
- Modify: `src-tauri/src/services/proxy/limits.rs`
- Create: `src-tauri/tests/proxy_lifecycle_writer.rs`

RED：

- [ ] 同 request 的 `StartRequest` 总在 `FinishAttempt`/`FinishRequest` 前 commit。
- [ ] request admission 原子预留 start + terminal 两个 permit；失败时 upstream call count 为 0。
- [ ] 每个 attempt 在网络前预留 terminal permit。
- [ ] pre-commit retry 等待前一 FinishAttempt durable ack。
- [ ] Drop path 使用已有 permit 同步入队，不 spawn、不等待容量。
- [ ] transient busy/locked 有界重试；permanent error writer unhealthy 并拒绝新 admission。
- [ ] worker 拥有 join handle；shutdown drain 顺序可证明。
- [ ] queue capacity 配置小于不变量时启动失败。

GREEN：

- [ ] 单 `mpsc::Sender<LifecycleWriteCommand>`；禁止 request/attempt 双 channel。
- [ ] command 只有 `StartRequest`、`FinishAttempt`、`FinishRequest`，均携带 typed oneshot ack。
- [ ] reservation 生命周期由 RequestLifecycle/AttemptLifecycle owner 持有。
- [ ] worker health、queue depth、oldest age、retry 和 ack latency 指标齐全。

运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_writer -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_persistence -- --nocapture
```

退出条件：顺序、backpressure、ack、failure 和 drain 均有 deterministic test；无 per-job fallback spawn。

## 16. Task 12：Ingress durable admission 与 RequestLease 转移

文件：

- Modify: `src-tauri/src/services/proxy/ingress.rs`
- Modify: `src-tauri/src/services/proxy/request.rs`
- Modify: `src-tauri/src/services/proxy/limits.rs`
- Modify: `src-tauri/src/services/proxy/server.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Create: `src-tauri/tests/proxy_ingress_lifecycle.rs`

RED：

- [ ] invalid/missing auth 只有脱敏 metric，无 request row。
- [ ] writer capacity/start commit 失败返回 503，且不调用 body parser/upstream。
- [ ] request-start `CommitOutcomeUnknown` 不调用 upstream，并按 request_id 对账而不是假定 rollback。
- [ ] durable start ack 发生在 body collection、endpoint validation 和 upstream 前。
- [ ] authenticated invalid JSON、body timeout、unknown route、method-not-allowed 都有唯一 request terminal。
- [ ] OPTIONS 不进入 journal。
- [ ] handler 返回 streaming response 后 active_requests 保持 1。

GREEN：

- [ ] route/middleware 结构让 authenticated fallback 进入 lifecycle，删除裸未观测 `not_found` path。
- [ ] `RequestLifecycle` 接收 RequestLease；`CanonicalProxyRequest` 不再拥有 lease。
- [ ] pre-commit error response 使用 lifecycle terminal，而不是 `FailedRequestContext` 旁路。
- [ ] start ack timeout 返回 typed `lifecycle_unavailable`，不继续 upstream。

运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_ingress_lifecycle -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::ingress -- --nocapture
```

退出条件：authenticated request 只有一个 admission 边界；lease 归 request owner。

## 17. Task 13：LifecycleBody 接管 protocol 和 delivery

文件：

- Create: `src-tauri/src/services/proxy/lifecycle/delivery.rs`
- Modify: `src-tauri/src/services/proxy/response_body.rs`
- Modify: `src-tauri/src/services/proxy/responses_chat_stream.rs`
- Modify: `src-tauri/src/services/proxy/observability.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Create: `src-tauri/tests/proxy_lifecycle_body.rs`

RED：

- [ ] buffered body unpolled drop、stream first chunk drop、write failure、shutdown cancel。
- [ ] protocol terminal 前 drop => attempt Abandoned/Neutral + request Interrupted。
- [ ] protocol terminal 后 drop => attempt Success + request Interrupted。
- [ ] terminal 后 transport error 不覆盖 candidate success。
- [ ] `BodyCompleted` 只表示 Hyper body 完成，不声称 peer ack。
- [ ] Drop 顺序为 FinishAttempt 后 FinishRequest；重复 poll/drop 不双终结。

GREEN：

- [ ] LifecycleBody 持有 protocol machine、delivery owner、request/attempt handles 和预留 permits，不持有 DB DTO。
- [ ] upstream bytes 先进入 ProtocolMachine，再由 transform 产生 downstream bytes。
- [ ] downstream 第一非空可见字节推进 commit barrier。
- [ ] body terminal/drop 释放 RequestLease；handler return 不释放。
- [ ] native Responses、Chat passthrough 和 Chat bridge 使用相同 terminal contract。

运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_body -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::response_body -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_protocol_contracts -- --nocapture
```

退出条件：body 只提交 typed events；不再原地改写 request outcome 或直接写数据库。

## 18. Task 14：production 单路径 cutover

文件：

- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/services/proxy/ingress.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/request.rs`
- Modify: `src-tauri/src/services/proxy/response_body.rs`
- Modify: `src-tauri/src/services/proxy/routing_repository.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/lib.rs`

步骤：

- [ ] production composition 只注入 lifecycle ports/writer，不注入旧 finalization path。
- [ ] 删除 `ProxyHttpResponse.outcome` 和 `success("queued")`。
- [ ] success/pre-commit failure/stream terminal 全部经同一个 RequestLifecycle。
- [ ] `/usage`、`/v1/models`、Chat、Responses、Embeddings 走唯一路径。
- [ ] cutover 作为单独可审阅 change set；不加入 runtime flag 或双写保险。

回归：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_characterization -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_ingress_lifecycle -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_body -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

退出条件：production 没有按请求选择旧/新 lifecycle 的可能；P0 characterization 全部 GREEN。

## 19. Task 15：故障、并发、取消和 soak qualification

文件：

- Create: `src-tauri/tests/proxy_lifecycle_faults.rs`
- Create: `src-tauri/tests/proxy_lifecycle_concurrency.rs`
- Modify: `src-tauri/src/services/proxy/soak_tests.rs`
- Modify: `src-tauri/src/services/proxy/test_support.rs`
- Create: `scripts/run-proxy-lifecycle-soak.ps1`

故障矩阵：

- [ ] request start/attempt terminal/request terminal 每个 DB 边界注入 busy、locked、rollback、permanent error、commit outcome unknown。
- [ ] queue saturation、writer closed、ack sender drop、worker panic、drain timeout。
- [ ] DNS/TLS/connect/header/body/idle/protocol/downstream 每个阶段取消。
- [ ] 32 stream 并发、64 connection/32 request 限制、models fan-out 与普通 retry 竞争容量。
- [ ] forced shutdown 顺序：stop admission -> body terminal/drop -> close sender -> drain commands -> join worker -> close persistence。
- [ ] hard kill/restart 后 stale request 标记 process_terminated，不合成未知 candidate health。

soak gate：

- [ ] 1 小时 mixed buffered/stream/models/failure workload。
- [ ] 结束后 active requests、active attempts、body permits、writer permits、queue、worker task 全归零。
- [ ] 10,000 requests / 30,000 attempts 下 detail query 有界。
- [ ] request-start 和 attempt commit 各自 p95 < 100 ms。
- [ ] 单成功 attempt 的 TTFB p95 相比 Task 0 基线回归不超过 `max(30 ms, 15%)`；fallback 场景单独报告每次 durable barrier 成本。

运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_faults -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_concurrency -- --nocapture
powershell -ExecutionPolicy Bypass -File scripts/run-proxy-lifecycle-soak.ps1 -DurationMinutes 60
```

退出条件：无静默丢写、无资源泄漏、无 timeout 冒充成功；性能报告包含原始样本和分位数。

## 20. Task 16：真实 authenticated E2E 与 SQLite 事实核对

文件：

- Create: `scripts/verify-local-routing-lifecycle.ps1`
- Create: `scripts/verify-request-lifecycle-db.ps1`
- Create: `docs/superpowers/audits/request-lifecycle-live-e2e.md`

环境前置：

- 使用真实 Tauri app 和它实际打开的 v2 local route，不用仅调用内存 executor 代替。
- 使用用户已配置且允许测试的真实 station/key；脚本只从进程环境读取 local bearer，不打印它。
- 运行前记录 app version、commit、SQLite generation/schema version、route port；不记录 secret。

真实请求矩阵：

- [ ] `/v1/models` 部分成功和全部成功。
- [ ] non-stream Chat、stream Chat，验证 `[DONE]`/terminal 和 usage。
- [ ] native Responses non-stream/stream，验证 completed/failed/incomplete。
- [ ] Chat-to-Responses bridge stream，不允许 EOF 假完成。
- [ ] Embeddings buffered envelope。
- [ ] 客户端中途取消 stream，分别覆盖 protocol terminal 前后。
- [ ] 注入一个可控失败候选后 fallback 成功，证明 A/B 两条 attempt health 都更新。

每次请求按 request_id 做三方核对：

1. 客户端看到的 HTTP/SSE terminal；
2. runtime sanitized trace/metrics；
3. SQLite `request_logs` + `request_attempts` + health projection。

SQLite 脚本必须断言：

- [ ] request row 恰好一条且 terminal 非空；
- [ ] attempt ordinal 连续且数量符合真实调用；
- [ ] A 失败 B 成功时两条 health effect 正确；
- [ ] protocol/delivery terminal 不被混写；
- [ ] request/attempt 无重复；
- [ ] 查询输出不含 key、bearer、Cookie 或完整 payload。

启动与验证：

```powershell
$env:CARGO_TARGET_DIR = (Resolve-Path 'output/local-routing-lifecycle-target')
pnpm.cmd tauri:dev
```

另开终端执行：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/verify-local-routing-lifecycle.ps1
powershell -ExecutionPolicy Bypass -File scripts/verify-request-lifecycle-db.ps1
```

退出条件：不能以 loopback fake 或单元测试代替；真实 request_id 的客户端、runtime、SQLite 三方一致并保存脱敏证据。

## 21. Task 17：删除兼容权威并运行架构 fitness gate

文件：

- Delete/modify owners: `src-tauri/src/services/proxy/routing_repository.rs`
- Delete/modify owners: `src-tauri/src/services/proxy/response_body.rs`
- Modify: `src-tauri/src/services/proxy/execution.rs`
- Modify: `src-tauri/src/services/proxy/request.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/observability.rs`
- Modify: `src-tauri/src/services/proxy/routing_failure.rs`
- Create: `scripts/request-lifecycle-architecture.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`
- Modify: `docs/superpowers/audits/request-lifecycle-boundary-manifest.json`
- Modify: `docs/superpowers/audits/request-lifecycle-deletion-ledger.md`

删除门禁：

- [ ] 删除 `FinalRequestOutcome`、`CandidateFeedback`、`FailedRequestContext`、`ProxyHttpResponse.outcome`。
- [ ] 删除 `ResponseMode` 和所有旧 completion adapter。
- [ ] 删除旧 `FinalizationDispatcher`、fallback spawn 和 permanent request-id HashSet。
- [ ] 停止写 `attempts_json`；按 migration 合同保留受控 read fallback 一个发布周期后，在明确后续 migration 删除字段。
- [ ] 删除只验证旧源码文本形状的 tests；保留/新增行为 tests 和 AST dependency gate。
- [ ] `AttemptTrace` 若保留，只能由 canonical record 投影且不是 persistence/health authority。

AST/CodeGraph gate：

- [ ] protocol 不依赖 database/router/scheduler。
- [ ] lifecycle reducers 不依赖 Axum request、Reqwest client、SQLx 或 raw SQLite。
- [ ] persistence 不依赖 response body、Router 或 provider adapter。
- [ ] execution 不构造 persistence DTO。
- [ ] no unregistered public export；fan-in/fan-out 不出现新的 catch-all owner。
- [ ] no unbounded queue、per-job spawn fallback、permanent request/attempt dedupe set。

运行：

```powershell
pnpm.cmd test:contracts
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_domain -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

再用 CodeGraph 对 deletion ledger 中每个旧 symbol 做 search/impact，并保存最终结构快照。任何旧 production consumer 残留都阻塞发布。

退出条件：仓库只有一套 lifecycle、feedback、protocol completion 和 persistence authority。

## 22. Task 18：完整 release validation

顺序执行：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
pnpm.cmd test:contracts
pnpm.cmd test
pnpm.cmd build
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_domain -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_protocol_contracts -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_persistence -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_faults -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_concurrency -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
pnpm.cmd tauri:build
```

Release gate：

- [ ] Task 16 真实 E2E 在 release candidate binary 上重跑。
- [ ] signed Windows package 安装、升级、完整退出和重启通过。
- [ ] shutdown drain、updater prepare 和 persistence close 顺序通过。
- [ ] secret/artifact scan 覆盖 logs、fixtures、output、SQLite verification report。
- [ ] 性能与 soak 使用 release build，不只使用 debug build。
- [ ] CI/release workflow 有最终成功结论；本地 timeout 不计通过。
- [ ] 发布前复核 `git diff --name-only`，没有无关前端、用户数据、日志、SQLite、key 或 output artifacts。

退出条件：所有命令真实退出 0，所有手工/真实 gate 有脱敏证据；否则状态保持“未完成”。

## 23. Checkpoint 与停止条件

Checkpoint A，Task 0-5：

- 允许纯 domain/protocol 继续；不得接 production persistence。
- 若官方 protocol 语义仍有未决分支，停下更新 spec，不以 lenient policy 兜底。

Checkpoint B，Task 6-9：

- in-memory integration 必须证明逐 attempt feedback 和 models 聚合。
- 若第四个局部补丁仍需要共享可变 outcome，说明迁移方向错误，停止并回到 owner 设计。

Checkpoint C，Task 10-13：

- Persistence V2 port、单 writer 顺序和 durable admission 必须一起评审。
- 任一 production 临时 SQL adapter、双 channel 重排或 drop-time spawn 出现即停止。

Checkpoint D，Task 14-17：

- cutover 后立即删除旧 production authority，不允许“下一版再删”。
- 真实 E2E 或 SQLite 三方核对不一致即回到最早分歧边界调查，不继续 release。

Checkpoint E，Task 18：

- timeout、部分绿色、仅 mock 绿色、仅 debug build 绿色都不满足发布条件。
- 发布和 push 需要用户另行明确授权。

## 24. 最终完成定义

- [ ] Request、Attempt、Protocol、Delivery 各有唯一 owner。
- [ ] authenticated admission 以 durable request-start ack 为边界。
- [ ] 每个正常结束的 candidate attempt 有唯一 terminal record；hard kill 不伪造未知事实。
- [ ] retry 与 health 完全分离，retry 等待前一 attempt durable ack。
- [ ] protocol terminal 与 transport EOF/error、delivery terminal 分离。
- [ ] `BodyCompleted` 不被错误宣称为客户端 acknowledgement。
- [ ] RequestLease 持有到 body terminal/drop/cancel。
- [ ] 单一有序 LifecycleWriter 有界、可重试、可观测、可 drain。
- [ ] request/attempt/health transaction 和数据库幂等约束通过 fault/restart test。
- [ ] Persistence V2 单一权威，无 AppDatabase 新债务、无 dual write。
- [ ] stream 本身通过真实客户端验证，没有 non-stream 掩盖路径。
- [ ] 真实 request_id 的客户端、runtime、SQLite 三方事实一致。
- [ ] 旧 outcome、feedback、completion 和 finalization production path 已删除。
- [ ] CodeGraph/AST、完整测试、soak、release build、安装升级验证全部通过。
