# Relay Pool Desktop 上游错误分类与重试工程闭环计划

状态：范围修订后待完成：三态 transport 结论已接受，剩余为 full verifier 运行环境与文档资格收口

日期：2026-08-13

适用范围：关闭 `2026-08-12-upstream-error-classification-retry-upgrade.md` 已实施但尚未通过 exit gate 的工程内容。本计划不重新设计错误分类与路由策略，不替代智能路由总计划，也不把真实 provider smoke 混入本地工程完成条件。

> **2026-08-13 范围修订（当前有效）：** 后续只朝“正式接受当前 reqwest 三态
> `NotConnected | ResponseStarted | Unknown` transport 结论，并完成文档与可审计的
> `verify:full` 运行环境”推进。不得为了暴露 headers/body 的中间发送阶段替换、封装或新增
> transport；真实 provider/Codex smoke、额外的 fault/soak 扩展，以及与三态结论无关的旧 owner
> 删除，均移出本计划当前执行范围。下文原 Tasks 1–9 是历史实施清单，仅作为已完成行为和风险的
> 追溯记录；它们不再构成当前完成条件。

## 1. 事实来源与当前基线

实施时按以下优先级判断事实：

1. 仓库根目录 `AGENTS.md`；
2. 当前代码、自动化契约和 `docs/README.md` 标记的当前规范；
3. `2026-08-12-upstream-error-classification-retry-upgrade.md` 的冻结决策；
4. 本计划记录的剩余任务与执行顺序。

截至 2026-08-13，专项已经具备以下生产基础，不得重新实现第二套 owner：

- bounded HTTP/SSE error evidence parser 与唯一 CanonicalOutcome 链；
- capacity 同目标有界重试、共享 admission、累计等待预算、cooldown/HalfOpen 基础状态机；
- target commitment 与重试前权威 reload/revalidation；
- HTTP 非 2xx、bounded 2xx error envelope 与 SSE precommit/committed 基础切换；
- scoped health/capability/group effect 的迁移、store 与部分 Planner 消费；
- OpenAI-compatible public error adapter；
- `DecisionTraceProfileV1` 的事件/字段/序列化/ring 硬上限及 Execution 基础事件接线；
- dual-terminal fail-closed 与 persistence architecture 边界修复。

当前确定性基线：

- `routing_dual_terminal_lifecycle`：20 项通过；selected attempt 持久化失败会记录唯一
  fail-closed `Interrupted` request terminal 并释放 lease；
- `persistence_architecture`：42 项通过；
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --quiet`：959/959；
- 剩余 persistence、portable、proxy 和 routing integration binaries 已按显式 `--test`
  分批运行并全部退出码 0；
- `node scripts/upstream-error-contract.test.mjs`、
  `node scripts/intelligent-routing-architecture.test.mjs`、`cargo fmt --check`、
  `cargo check --locked`、`git diff --check` 和 `pnpm.cmd verify:fast`：退出码 0。

本轮关闭的工程差异：

- 将 capacity-domain、failure-domain、terminal outbox、durable outcome/trace、metrics 和
  对应 test-support 的实际依赖登记到 persistence boundary manifest；
  `persistence_architecture` 不再存在未登记边。
- 两条 production-composition E2E 已覆盖 group/subscription failure 和
  `model_not_found` capability failure 的“写入 -> 下一 snapshot 精确排除 -> 无关 subject
  不受影响 -> revision 恢复”链。
- `routing_failure::classify_route_failure` 是仍在使用的 planning-only helper；
  `routing_health_snapshot` 仍由持久化、legacy import/validate 与 portable catalog 使用。
  二者不属于可在本专项中预先删除的旧 owner。

范围修订后的剩余条件：

- 三态 transport 结论须从“partial implementation record”提升为正式接受的兼容性边界：
  当前 reqwest 只能可靠产生 `NotConnected`、`ResponseStarted` 和 `Unknown`；中间 socket
  write phase 不能从 body poll、HTTP status 或 downstream commit 推断。`Unknown` 的非幂等
  replay 继续 fail-closed；不再寻求替代 transport 或中间 phase。
- `pnpm.cmd verify:full` 仍需要重新取得 RustSec advisory database；此前失败为 GitHub
  网络连接重置，不能写作通过。本轮重试还受到执行宿主每条命令 124 秒上限中断，未取得
  `verify:full` 退出码；后台执行以保留日志的方式被执行策略拒绝。

当前工作区包含大量未提交并行改动。每个任务开始和结束都必须保存 `git status --short`，只修改本任务需要的文件；不得清理、覆盖或回退来源不明的改动。

## 2. 完成定义

### 2.1 当前范围的工程收口完成

范围修订后，必须同时满足：

- `docs/specs/2026-08-13-reliable-transport-send-phase-spike.md` 明确标记三态
  reqwest 结论为 accepted compatibility boundary，并列出不可推断中间 phase 的证据和非幂等
  `Unknown` fail-closed 保证；
- `pnpm.cmd verify:full` 有完整、可审计的退出码 0，或由仓库允许的运行环境提供等价的完整日志和
  失败归属；不可把 124 秒宿主中断当作通过；
- `cargo fmt --check`、`cargo check --locked`、已列出的 Rust library/integration tests、
  `pnpm.cmd test`、`pnpm.cmd build` 和 `pnpm.cmd verify:fast` 保持退出码 0；
- closure plan、transport spike 和 qualification 文档对本次范围、验证证据及残余非目标一致。

### 2.2 不属于当前范围的 release qualification

真实 OpenAI-compatible/Sub2API/Codex smoke 仍需要用户明确授权和隔离测试凭据，但已移出本计划。
它不影响本计划的工程收口，也不得以 fixture 替代后写作 release qualified。

## 3. 不变量与停止条件

整个实施过程必须保持：

- 未确认上游未接受的非幂等请求不得透明重放；
- capacity 不得写 credential/account hard failure；
- 同一 provider/model capacity domain 不得轮询 sibling Key；
- committed 后不得 retry，不得生成第二终态；
- raw secret、认证 header、完整 URL/query、动态 message 和真实 request id 不得进入 metric label、持久化 DTO、fixture 或日志；
- retry、trace、buffer、队列、attempt、deadline 和内存都有单一硬上限 owner；
- migration 只允许 additive、可恢复、可重建的演进，生成物只能通过脚本更新；
- 测试不得使用与 production 不同的 failure-domain、retry、transport 或 lifecycle 合同。

遇到以下情况立即停止扩大行为，只保留保守路径并记录 blocker：

- transport 层无法可靠证明 request bytes 的发送阶段；
- 无法为新依赖确认许可证、锁文件和 Windows/Tauri 兼容性；
- 需要用 `allow(dead_code)`、跳过架构门禁、手改生成物或双写来通过验证；
- 持久化 schema 无法在现有 upgrade/recovery contract 下安全演进；
- 工作区并行改动使任务文件的所有权或预期行为无法判定。

## 4. 依赖顺序

```text
Task 0 基线与门禁恢复
  -> Task 1 请求级 capacity 状态机
  -> Task 2 transport send phase
  -> Task 3 scoped health/capability/group E2E
  -> Task 4 durable observability
  -> Task 5 fault/concurrency qualification
  -> Task 6 原子删除与架构清零
  -> Task 7 全量工程验证
  -> Task 8 文档与交付闭环
  -> Task 9 真实 provider smoke（外部授权）
```

Task 3 的测试准备可与 Task 2 并行，但 production 行为不得在 Task 2 的 replay-safety 事实关闭前扩大。Task 4 可以先设计 schema/DTO，但只能在 Task 1-3 的稳定 reason/outcome 合同冻结后写入生产数据。

## 5. 可执行任务

### Task 0：冻结剩余差异并恢复快速门禁

目标：让后续失败来自真实功能缺口，而不是陈旧测试 API 或未使用辅助接口。

实施：

1. 保存工作区清单，按本专项、其他并行改动、生成物三类标注涉及文件，不改变用户已有内容。
2. 处理当前两个 dead-code group：
   - 将 `DecisionTraceRing::with_limits` 等仅用于边界测试的接口限制到测试构建，或让 production 通过真正的查询/状态读取消费所需接口；不得 suppress；
   - 若 `is_committed()` 仅用于外部测试，改由公开的 progress/terminal 合同断言；若 production 后续确实需要，必须在 Task 1/5 真实消费后保留。
3. 复跑专项架构测试，核对 `persistence-v2-boundary-manifest.json` 中新增边是否都是实际且最小的边。
4. 更新专项实施台账的事实状态，但此时不得把后续任务提前标为 done。

Focused gate：

```powershell
pnpm.cmd test:dead-code-policy
pnpm.cmd audit:dead-code
cargo test --locked --manifest-path src-tauri/Cargo.toml --test persistence_architecture
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_dual_terminal_lifecycle
pnpm.cmd verify:fast
```

Exit gate：`verify:fast` 不再被专项新增的 dead code、stale manifest 或测试拓扑问题阻断；若暴露新的功能测试失败，登记到对应 Task，不在 Task 0 临时绕过。

### Task 1：关闭请求级 capacity 状态机

目标：同目标重试耗尽后，普通规划不会重新选择同域 sibling Key；跨域 fallback 最多一次且是该逻辑请求的最后 outbound 分支。

实施：

1. 在唯一 routing Coordinator 中引入/补齐 request-local 状态：总 attempt、同目标 retry、累计 sleep、monotonic deadline、已排除 capacity domain、跨域是否已消费、admission wait 状态。
2. 同目标 capacity retry 必须复用同一不可变 body backing storage，sleep 前释放网络 stream、attempt/capacity lease 和诊断 buffer，醒来后重新取得 lease 并权威 revalidate commitment。
3. 同目标耗尽后把 domain exclusion 交给普通 Planner；同域 sibling Key 产生稳定 `capacity_same_domain_fallback_suppressed`，不启动 outbound attempt。
4. 仅当 provider/deployment identity、request portability 和 replay gate 均允许时选择一个不同 domain；跨域 attempt 结束后无论结果为何都终止本请求的 retry 链。
5. FIFO waiter 必须在等待前准入；cancel/shutdown/drop exactly-once 释放 waiter、active permit 与 HalfOpen probe ownership。
6. 用 fake clock 锁定 Closed/Open/HalfOpen、Retry-After、本地 jitter、累计 sleep 和 deadline；wall-clock jump 不改变结果。
7. 为生产 trace 接线 same-domain suppression、cross-domain fallback、retry admission saturation 和最终 terminal reason。

Focused gate：扩展 `routing_capacity.rs`、`routing_capacity_faults.rs`、`routing_loopback_e2e.rs` 和 coordinator 契约，至少证明：

- 连续 capacity 仅请求原 target 三次；
- 同域两个 sibling Key 不被请求；
- 至多一个跨域 attempt，跨域后不再开启普通 retry；
- revision 漂移发生在 sleep 期间时不发送旧 target；
- cancel/shutdown/queue-full/half-open race 不泄漏 permit；
- attempt、sleep 和 deadline 上限均不可被 replan 重置。

Exit gate：从 production composition 发起请求得到完整 trace，证明 selection、send ordinal、domain exclusion 与终局一致；不存在 Execution 自行拼装 commitment 或第二套 retry budget。

### Task 2：实现可靠的 transport send phase

目标：让 request acceptance/replay safety 使用 transport owner 的单调事实，并维持 uncertain 非幂等请求 fail closed。

实施分为硬决策门和生产切换：

1. 建立 transport spike，验证当前 reqwest/hyper 栈能否在不猜测 socket 状态的前提下报告：`NotConnected`、`ConnectedNoHeaders`、`HeadersSent`、`BodyPartiallySent`、`BodyFullySent`、`ResponseStarted`。
2. 证明“body 被 poll”与“bytes 已交给 socket”之间的差异；不得用 body stream poll 次数冒充实际发送完成。
3. 优先复用现有依赖栈的受控 adapter/connector。若必须引入更底层 transport 包装，先记录选择、许可证、Windows TLS/proxy/timeout/streaming 兼容性和锁文件影响，再实施。
4. transport phase 只能单调前进，连接/TLS/headers/body/response 的注入故障不得回退 phase。
5. 将中间 phase 从 `cfg(test)` 合同切到 production；HTTP failure、stream failure 和 timeout 均携带最后可靠 phase。
6. acceptance/replay 只消费 phase、方法/操作语义、body replayability、idempotency identity 与 provider capability。任何未知或可能已接受的非幂等请求停止透明 retry。
7. 保持保守降级：平台或 transport 无法提供精确信号时返回 `Unknown`，绝不能为了可用性推断 `NotConnected`。

Focused gate：使用本地可控 TCP/upstream harness 注入 connect、TLS、headers、partial-body、full-body、response-started 和 mid-stream failure，逐一断言 phase 单调性及非幂等 replay 结果；不得依赖真实网络时序。

Exit gate：production `upstream.rs` 不再只产生 `NotConnected | Unknown | ResponseStarted`；测试和 production 使用同一 reporter；不可靠平台路径明确返回 `Unknown` 并通过 fail-closed 回归。

### Task 3：关闭 scoped health、capability 与 group 生命周期

目标：typed durable verdict 能在下一次 PlanningSnapshot 精确影响对应 subject/dimension，并随权威 revision 恢复。

实施：

1. 补 StationGroup/Subscription 的 rule producer、typed subject、effect planner、store、batch read 和 Planner consumer E2E；无法解析 subject 时保持 neutral，不得降级污染 account/key。
2. 补 `model_not_found`：terminal 写 capability verdict，下一 snapshot 仅排除相同 key/model/deployment commitment；model-alias/profile/subject revision 变化后恢复。
3. 验证 credential、account、group、balance、quota、rate-limit、capability dimension 可并存，恢复一个 dimension 不清除其他 verdict。
4. PlanningSnapshot 必须使用单批读取与 revision fence；禁止 per-candidate N+1 查询和旧 `routing_health_snapshot` 兼容读取。
5. 完成 shadow rebuild 生产维护入口：读取 immutable typed outcome，不用当前 message/rule set 重分类历史；checkpoint、swap 和失败回滚可恢复。
6. terminal、Observation、verdict 和 projector checkpoint 保持唯一事务边界；duplicate/late terminal 不重复 effect，payload collision fail closed。
7. 补 migration/reimport/restart/crash parity，证明 0035/0036 与 portable migration catalog 一致。

Focused gate：扩展 `routing_health_verdict_persistence.rs`、`intelligent_routing_persistence.rs`、`operational_fact_reader.rs`、`proxy_lifecycle_*` 和 migration fault tests。

Exit gate：至少提供两条 production-composition E2E：group failure 与 model capability failure，均覆盖“写入 → 下一 snapshot 排除 → 无关 subject 不受影响 → revision 恢复”。

### Task 4：完成 durable observability 与 IPC 查询

目标：把当前 in-memory Decision Trace 基础升级为可审计但不泄密的 production 闭环，同时保持 ring 用于短期运行时诊断。

实施：

1. 冻结单一 trace/outcome profile，稳定枚举至少覆盖 classification、confidence、evidence source、target kind、request acceptance、send phase、replay/billing state、retry disposition、health/capability effect、profile versions 和 failure-domain commitment。
2. 补齐生产事件：same-domain suppression、cross-domain fallback、committed stop、SSE precommit error、retry/memory saturation、classifier/projector/lifecycle fail-closed、profile mismatch。
3. ring 继续遵守 512 traces/16MiB、每请求 64 events/32KiB、单字段 512 bytes 和一次 truncation；查询必须读取 ring 的真实状态及 dropped/retained 诊断，不能留下未使用 inspection API。
4. 将 request/attempt 的稳定 outcome 摘要写入现有 request lifecycle/read model；优先扩展已有表和 store，只有现有 schema 无法表达时才使用下一 additive migration。
5. 持久化只保存 stable code、闭合枚举、版本和经审核的内部 identity reference；不保存 raw message、Authorization、完整 URL/query 或 secret。
6. 让现有 `get_request_decision_trace` 从 legacy summary/trace unavailable 升级为真实版本化 trace 查询；经过 command facade、application query、store/运行时 composition 与生成 binding/ACL。
7. 接线低基数 metrics：分类、confidence、retry kind、pre/post-commit terminal、saturation/fail-closed、profile version。label 必须是闭合 enum；request/station/key/model/message 不作为 label。
8. reliability 聚合按 failure-domain commitment 对同一逻辑请求的相关 retry 去重或降权；成本展示区分 rejected/not-billable、billing-uncertain、usage-observed 和 possibly-accepted。
9. 移除 `metrics.rs` 的模块级 `allow(dead_code)`：真实 recorder 消费需要的合同，其余历史未接线合同按仓库 dead-code policy 单独处理，不能用本专项掩盖。

Focused gate：observability contract、trace replay、ring eviction、metric buffer、redaction、IPC serialization、binding generation 与 architecture producer/consumer tests。

Exit gate：从真实 proxy execution 产生 canonical failure，经 lifecycle 持久化后可通过 IPC 查询同一版本化摘要；运行时 trace 能查询 bounded 事件；所有 labels/DTO/fixture 通过 secret、高基数、URL/query 审计。

### Task 5：补齐 fault、并发、内存与协议矩阵

目标：用确定性测试证明闭环在压力和失败窗口下仍满足资源、终态与 exactly-once 不变量。

矩阵至少覆盖：

1. 100 个并发 capacity 请求：全局/单域 active、FIFO waiter、queue cap、deadline、cancel 和 shutdown；
2. 100 个并发 HTTP error body/SSE bootstrap：共享 32MiB admission，按 owned allocation capacity 加保守 scratch 核算；
3. gzip/deflate 解压错误、压缩炸弹、JSON 深度/node/token/string 上限、malformed UTF-8 和任意 chunk split；
4. control-only EOF、semantic 后缺成功终态 EOF、合法空 completed、content/error 同 chunk 顺序；
5. 256KiB event 上限、慢客户端背压、下游断开与 terminal 写失败；
6. precommit/postcommit persistence failure、writer unavailable、outbox/checkpoint replay、duplicate/late terminal 和不同 payload collision；
7. classifier/projector/profile mismatch fail closed，不改变 canonical 决策；
8. rule/provider/retry/public/trace profile 热更新只影响新请求，在途 attempts 使用同一快照；
9. restart 后旧 runtime cooldown/permit 不泄漏到新 runtime instance；
10. 最大 request body 的 retry 共享 backing storage，不随 attempt 深拷贝增长。

实现原则：优先扩展现有 `routing_capacity_faults`、`routing_stream_finalization_faults`、`proxy_lifecycle_faults/concurrency`、`persistence_fault_matrix` 与 loopback harness；只有现有 harness 无法表达 transport/protocol 故障时才新增共享 fixture。

Exit gate：所有测试有确定性时间源或本地 loopback，不依赖真实 provider；资源计数在成功、错误、取消、panic/shutdown 后回到基线；committed 路径永不产生 retry 或第二终态。

### Task 6：原子删除旧 owner 并收紧架构门禁

目标：新链有充分证据后，一次删除所有重复解释和兼容写回，避免长期双 owner。

删除/审计：

- `routing_failure::classify_route_failure` 中重复的 upstream status/message 分类；
- `should_fallback(status)`、`RetryPolicy::decide` 状态矩阵与 Execution 本地 retry/health 推导；
- `ProxyFailure` public response 反推 canonical effect；
- OpenAI/Responses 重复 error extractor；
- explicit SSE terminal 降级 generic stream failure；
- scoped effect 写回当前 Station Key 的兼容路径；
- 已转绿但仍以 `*-red*` 命名的长期脚本和仅验证旧行为的 fixture；
- 不再需要的 dead-code expectation/test-only production API。

架构门禁新增/确认拒绝：

- classifier 外匹配 provider capacity code/message；
- consumer 按 HTTP status/public code 重新分类；
- 同域 sibling Key fallback、committed retry、无 phase 的非幂等 replay；
- raw message/secret/URL 进入 durable scope、IPC 或 metric label；
- 无 permit buffer、无界 queue、sleep 持有 lease；
- 上游 499 冒充本地下游取消；
- test/production 使用不同核心合同；
- trace/metric 类型存在但 production 无 producer 或 consumer。

同步 deletion ledger、intelligent-routing/persistence boundary manifests、contract script 中性命名与 `package.json`/`verify.ps1` 引用。

Exit gate：所有删除目标零生产引用，architecture/contract/artifact/generated checks 全绿，且删除前的 focused behavior tests 仍通过。

### Task 7：全量工程资格验证

按由快到慢顺序运行，任一步失败都修根因后从相关层级重跑：

```powershell
git diff --check
node scripts/upstream-error-contract.test.mjs
node scripts/intelligent-routing-architecture.test.mjs
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_failure_contract
node scripts/upstream-error-contract.test.mjs
cargo test --locked --manifest-path src-tauri/Cargo.toml
pnpm.cmd test
pnpm.cmd build
pnpm.cmd verify:fast
pnpm.cmd verify:full
```

若修改 IPC、ACL、binding、migration 或 schema artifact，额外运行对应仓库生成/check 命令，确认生成后第二次运行无 diff。`verify:release` 不属于本计划默认验证。

Exit gate：以上必跑命令全部以退出码 0 完成；如外部 advisory 数据库不可用，保留完整失败证据并标记该项未验证，不能将 engineering cutover 标为完成。

### Task 8：文档、台账与交付闭环

目标：让后续维护者能够从当前规范和自动化直接判断专项状态，而不是依赖会话记录。

实施：

1. 更新原专项计划第 11 节，逐 Task 写 `done with evidence | partial | pending | externally blocked` 和具体测试证据。
2. 更新 `docs/README.md` 的当前状态，区分 engineering cutover 与 release qualification。
3. 更新 intelligent-routing acceptance matrix、qualification、deletion ledger 与必要 release note。
4. 记录 migration/schema/profile version、回滚边界、残余风险和真实 smoke 授权状态。
5. 输出最终变更文件、实际验证命令/结果、未验证事项；未经明确要求不 stage、commit、push、建分支或创建 PR。

Exit gate：文档中的状态、版本、命令和证据与当前代码 revision 一致；不再把已接线的 Decision Trace 写成 pending，也不把未完成的 transport/fault/smoke 写成 done。

### Task 9：真实 provider 与 Codex smoke（外部授权）

前置条件：Task 0-8 全部完成，用户明确授权，使用专门的测试账号/Key 和假业务输入，输出经过脱敏且不落库完整响应。

至少验证：

- 可控 OpenAI-compatible HTTP 400 capacity；
- SSE 首事件 capacity/overload；
- Sub2API 普通成功、401、429、5xx；
- Codex 对最终 OpenAI-compatible `server_error` 的实际行为；
- capacity 期间 target/key 未切换且未写 credential failure；
- trace/log/diagnostic artifact 不含真实 secret 或完整认证数据。

Exit gate：证据经脱敏审计后记录到 qualification；无法稳定制造场景时明确写 `pending external evidence`，不得用 fixture 替代真实 smoke。

## 6. 推荐实施批次

为降低当前大脏工作区的合并风险，按以下批次交付，每批都保持可编译和 focused tests 全绿：

1. Batch A：Task 0，恢复快速门禁并校准台账；
2. Batch B：Tasks 1-2，关闭 retry/domain/transport 安全边界；
3. Batch C：Task 3，关闭 durable health/capability/group 生产 E2E；
4. Batch D：Task 4，完成 observability 持久化、IPC 与 metrics；
5. Batch E：Task 5，补 fault/concurrency/soak；
6. Batch F：Tasks 6-8，删除旧 owner、全量验证和文档交付；
7. Batch G：Task 9，仅在外部授权后执行。

任何批次都不得通过增加 retry 次数、放宽 replay gate、保留双分类/双写、删除恢复能力或 suppress 门禁来换取通过。

## 7. 最终交付清单

- 唯一 transport/evidence/canonical/retry/effect/public/trace production chain；
- request-local capacity budget、同域 exclusion 和跨域终局证据；
- group/capability/scoped-health 写入、规划与恢复 E2E；
- bounded runtime trace、durable outcome/read model、IPC 查询和低基数 metrics；
- 100 并发、内存、协议、背压、取消、持久化与 crash/replay 测试证据；
- 旧 owner 零引用的 deletion ledger 与 architecture gate；
- `verify:fast/full`、build、完整 Rust/前端测试结果；
- 当前规范、qualification 和残余风险说明；
- 真实 provider smoke 的完成证据或明确的外部授权 pending 状态。
